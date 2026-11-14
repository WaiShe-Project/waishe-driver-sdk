//! Run WaiShe drivers.
//!
//! Each [`DriverInstance`] is one WebAssembly instance in its own store, with its own memory and
//! fuel limits, serving one endpoint. HID access goes through the [`HidBackend`] the caller injects,
//! which only knows the handles granted to that endpoint.
//!
//! Time is bounded twice: fuel limits pure computation, and every host function checks a
//! wall-clock deadline for the current call, so a driver that keeps calling into the host is
//! stopped too.

use serde::de::DeserializeOwned;
use std::time::{Duration, Instant};
use thiserror::Error;
use waishe_driver_types::abi::{self, HidStatus};
use waishe_driver_types::{
    self as types, CapabilityDescriptor, CapabilityEvent, EndpointInfo, ProbeError, ProbeResult,
    ReadError, Value, WriteError, WriteOutcome,
};
use wasmi::{
    Caller, Config, Engine, Extern, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc,
};

/// HID operations for one driver instance. Handles are the ones listed in [`EndpointInfo`].
pub trait HidBackend: Send + 'static {
    fn send_feature(&mut self, handle: u32, data: &[u8]) -> Result<(), HidStatus>;
    fn get_feature(&mut self, handle: u32, buf: &mut [u8]) -> Result<usize, HidStatus>;
    fn write(&mut self, handle: u32, data: &[u8]) -> Result<usize, HidStatus>;
    /// Returns 0 on timeout.
    fn read(&mut self, handle: u32, buf: &mut [u8], timeout_ms: u32) -> Result<usize, HidStatus>;
}

#[derive(Debug, Clone)]
pub struct Limits {
    /// Linear memory cap per instance.
    pub memory_bytes: usize,
    /// Fuel per call into the driver. Measured in M0: roughly 6.7 million per millisecond of compute.
    pub fuel_per_call: u64,
    /// Wall-clock budget per call, including time blocked in host functions.
    pub call_deadline: Duration,
    pub max_sleep: Duration,
    pub max_read_timeout: Duration,
    /// Largest HID buffer a driver may pass.
    pub max_report_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            memory_bytes: 16 << 20,
            fuel_per_call: 2_000_000_000,
            call_deadline: Duration::from_secs(10),
            max_sleep: Duration::from_secs(1),
            max_read_timeout: Duration::from_secs(5),
            max_report_len: 4096,
        }
    }
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("driver module is invalid: {0}")]
    Module(String),
    #[error("driver ABI {found:#x} does not match host ABI {expected:#x}")]
    AbiMismatch { found: i32, expected: i32 },
    #[error("driver trapped: {0}")]
    Trap(String),
    #[error("driver returned an undecodable value: {0}")]
    Decode(String),
    #[error("driver instance is unusable after an earlier trap")]
    Poisoned,
}

/// Shared compilation engine. Create one per process.
#[derive(Clone)]
pub struct Runtime {
    engine: Engine,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        let mut cfg = Config::default();
        cfg.consume_fuel(true);
        Runtime {
            engine: Engine::new(&cfg),
        }
    }
}

/// A compiled driver module. Cheap to instantiate many times.
#[derive(Clone)]
pub struct DriverModule {
    module: Module,
}

impl DriverModule {
    pub fn compile(rt: &Runtime, wasm: &[u8]) -> Result<Self, HostError> {
        let module = Module::new(&rt.engine, wasm).map_err(|e| HostError::Module(e.to_string()))?;
        for import in module.imports() {
            if import.module() != abi::IMPORT_MODULE {
                return Err(HostError::Module(format!(
                    "imports from unknown module `{}`",
                    import.module()
                )));
            }
        }
        Ok(DriverModule { module })
    }
}

struct State<B> {
    backend: B,
    limits: Limits,
    store_limits: StoreLimits,
    created: Instant,
    call_started: Instant,
}

impl<B> State<B> {
    fn check_deadline(&self) -> Result<(), wasmi::Error> {
        if self.call_started.elapsed() > self.limits.call_deadline {
            Err(wasmi::Error::new("call deadline exceeded"))
        } else {
            Ok(())
        }
    }
}

type Exports = (
    TypedFunc<u32, u32>,             // alloc
    TypedFunc<(u32, u32), i64>,      // probe
    TypedFunc<(), i64>,              // describe
    TypedFunc<(u32, u32), i64>,      // read
    TypedFunc<(u32, u32), i64>,      // write
    TypedFunc<(u32, u32, u32), i64>, // on_input
);

pub struct DriverInstance<B: HidBackend> {
    store: Store<State<B>>,
    instance: Instance,
    exports: Exports,
    poisoned: bool,
}

fn guest_memory<'a, B: HidBackend>(
    c: &'a mut Caller<'_, State<B>>,
) -> Result<(&'a mut [u8], &'a mut State<B>), wasmi::Error> {
    let mem = c
        .get_export("memory")
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("driver exports no memory"))?;
    Ok(mem.data_and_store_mut(c))
}

/// Validates a guest buffer and returns its range, or a negative status for the guest.
fn span(mem_len: usize, ptr: u32, len: u32, max: usize) -> Result<std::ops::Range<usize>, i32> {
    let (p, l) = (ptr as usize, len as usize);
    if l > max || p.checked_add(l).is_none_or(|end| end > mem_len) {
        return Err(HidStatus::Invalid as i32);
    }
    Ok(p..p + l)
}

fn status<T>(r: Result<T, HidStatus>, ok: impl FnOnce(T) -> i32) -> i32 {
    match r {
        Ok(v) => ok(v),
        Err(s) => s as i32,
    }
}

fn linker<B: HidBackend>(engine: &Engine) -> Result<Linker<State<B>>, wasmi::Error> {
    let mut l = Linker::<State<B>>::new(engine);
    let m = abi::IMPORT_MODULE;
    l.func_wrap(
        m,
        abi::IMPORT_HID_SEND_FEATURE,
        |mut c: Caller<'_, State<B>>, h: u32, p: u32, n: u32| {
            let (mem, s) = guest_memory(&mut c)?;
            s.check_deadline()?;
            Ok(match span(mem.len(), p, n, s.limits.max_report_len) {
                Ok(r) => status(s.backend.send_feature(h, &mem[r]), |_| 0),
                Err(e) => e,
            })
        },
    )?;
    l.func_wrap(
        m,
        abi::IMPORT_HID_GET_FEATURE,
        |mut c: Caller<'_, State<B>>, h: u32, p: u32, n: u32| {
            let (mem, s) = guest_memory(&mut c)?;
            s.check_deadline()?;
            Ok(match span(mem.len(), p, n, s.limits.max_report_len) {
                Ok(r) => status(s.backend.get_feature(h, &mut mem[r]), |n| n as i32),
                Err(e) => e,
            })
        },
    )?;
    l.func_wrap(
        m,
        abi::IMPORT_HID_WRITE,
        |mut c: Caller<'_, State<B>>, h: u32, p: u32, n: u32| {
            let (mem, s) = guest_memory(&mut c)?;
            s.check_deadline()?;
            Ok(match span(mem.len(), p, n, s.limits.max_report_len) {
                Ok(r) => status(s.backend.write(h, &mem[r]), |n| n as i32),
                Err(e) => e,
            })
        },
    )?;
    l.func_wrap(
        m,
        abi::IMPORT_HID_READ,
        |mut c: Caller<'_, State<B>>, h: u32, p: u32, n: u32, t: u32| {
            let (mem, s) = guest_memory(&mut c)?;
            s.check_deadline()?;
            let t = t.min(s.limits.max_read_timeout.as_millis() as u32);
            Ok(match span(mem.len(), p, n, s.limits.max_report_len) {
                Ok(r) => status(s.backend.read(h, &mut mem[r], t), |n| n as i32),
                Err(e) => e,
            })
        },
    )?;
    l.func_wrap(
        m,
        abi::IMPORT_SLEEP_MS,
        |c: Caller<'_, State<B>>, ms: u32| {
            let s = c.data();
            s.check_deadline()?;
            std::thread::sleep(Duration::from_millis(ms as u64).min(s.limits.max_sleep));
            Ok(())
        },
    )?;
    l.func_wrap(m, abi::IMPORT_NOW_MS, |c: Caller<'_, State<B>>| {
        c.data().created.elapsed().as_millis() as i64
    })?;
    l.func_wrap(
        m,
        abi::IMPORT_LOG,
        |mut c: Caller<'_, State<B>>, level: i32, p: u32, n: u32| {
            let (mem, _) = guest_memory(&mut c)?;
            if let Ok(r) = span(mem.len(), p, n, 1024) {
                let msg = String::from_utf8_lossy(&mem[r]);
                match level {
                    0 => tracing::error!(target: "driver", "{msg}"),
                    1 => tracing::warn!(target: "driver", "{msg}"),
                    2 => tracing::info!(target: "driver", "{msg}"),
                    _ => tracing::debug!(target: "driver", "{msg}"),
                }
            }
            Ok(())
        },
    )?;
    Ok(l)
}

impl<B: HidBackend> DriverInstance<B> {
    pub fn instantiate(
        rt: &Runtime,
        module: &DriverModule,
        backend: B,
        limits: Limits,
    ) -> Result<Self, HostError> {
        let module_err = |e: wasmi::Error| HostError::Module(e.to_string());
        let now = Instant::now();
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .instances(1)
            .build();
        let mut store = Store::new(
            &rt.engine,
            State {
                backend,
                limits,
                store_limits,
                created: now,
                call_started: now,
            },
        );
        store.limiter(|s| &mut s.store_limits);
        store
            .set_fuel(store.data().limits.fuel_per_call)
            .map_err(module_err)?;
        let instance = linker::<B>(&rt.engine)
            .map_err(module_err)?
            .instantiate_and_start(&mut store, &module.module)
            .map_err(module_err)?;

        let found = instance
            .get_typed_func::<(), i32>(&store, abi::EXPORT_ABI)
            .and_then(|f| f.call(&mut store, ()))
            .map_err(module_err)?;
        if found != abi::VERSION {
            return Err(HostError::AbiMismatch {
                found,
                expected: abi::VERSION,
            });
        }
        let f = |name: &str| HostError::Module(format!("missing or mistyped export `{name}`"));
        let exports = (
            instance
                .get_typed_func(&store, abi::EXPORT_ALLOC)
                .map_err(|_| f(abi::EXPORT_ALLOC))?,
            instance
                .get_typed_func(&store, abi::EXPORT_PROBE)
                .map_err(|_| f(abi::EXPORT_PROBE))?,
            instance
                .get_typed_func(&store, abi::EXPORT_DESCRIBE)
                .map_err(|_| f(abi::EXPORT_DESCRIBE))?,
            instance
                .get_typed_func(&store, abi::EXPORT_READ)
                .map_err(|_| f(abi::EXPORT_READ))?,
            instance
                .get_typed_func(&store, abi::EXPORT_WRITE)
                .map_err(|_| f(abi::EXPORT_WRITE))?,
            instance
                .get_typed_func(&store, abi::EXPORT_ON_INPUT)
                .map_err(|_| f(abi::EXPORT_ON_INPUT))?,
        );
        Ok(DriverInstance {
            store,
            instance,
            exports,
            poisoned: false,
        })
    }

    pub fn backend(&self) -> &B {
        &self.store.data().backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.store.data_mut().backend
    }

    /// True after a trap; the caller should drop the instance and create a new one.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn begin(&mut self) -> Result<(), HostError> {
        if self.poisoned {
            return Err(HostError::Poisoned);
        }
        let fuel = self.store.data().limits.fuel_per_call;
        self.store
            .set_fuel(fuel)
            .map_err(|e| HostError::Trap(e.to_string()))?;
        self.store.data_mut().call_started = Instant::now();
        Ok(())
    }

    fn trap(&mut self, e: wasmi::Error) -> HostError {
        self.poisoned = true;
        HostError::Trap(e.to_string())
    }

    fn put_input(&mut self, bytes: &[u8]) -> Result<u32, HostError> {
        let ptr = self
            .exports
            .0
            .call(&mut self.store, bytes.len() as u32)
            .map_err(|e| self.trap(e))?;
        let mem = self
            .instance
            .get_memory(&self.store, "memory")
            .ok_or_else(|| HostError::Module("no memory export".into()))?;
        mem.write(&mut self.store, ptr as usize, bytes)
            .map_err(|e| {
                self.poisoned = true;
                HostError::Trap(format!("input outside guest memory: {e}"))
            })?;
        Ok(ptr)
    }

    fn take_output<T: DeserializeOwned>(&mut self, packed: i64) -> Result<T, HostError> {
        let (ptr, len) = abi::unpack(packed);
        let mem = self
            .instance
            .get_memory(&self.store, "memory")
            .ok_or_else(|| HostError::Module("no memory export".into()))?;
        let data = mem.data(&self.store);
        let bytes = data
            .get(ptr as usize..ptr as usize + len as usize)
            .ok_or_else(|| HostError::Decode("output outside guest memory".into()))?;
        types::decode(bytes).map_err(|e| HostError::Decode(e.to_string()))
    }

    fn call_with_input<T: DeserializeOwned>(
        &mut self,
        input: &[u8],
        f: impl FnOnce(&mut Self, u32, u32) -> Result<i64, wasmi::Error>,
    ) -> Result<T, HostError> {
        self.begin()?;
        let ptr = self.put_input(input)?;
        let packed = f(self, ptr, input.len() as u32).map_err(|e| self.trap(e))?;
        self.take_output(packed)
    }

    pub fn probe(
        &mut self,
        endpoint: &EndpointInfo,
    ) -> Result<Result<ProbeResult, ProbeError>, HostError> {
        let probe = self.exports.1;
        self.call_with_input(&types::encode(endpoint), |s, p, n| {
            probe.call(&mut s.store, (p, n))
        })
    }

    pub fn describe(&mut self) -> Result<Vec<CapabilityDescriptor>, HostError> {
        self.begin()?;
        let packed = self
            .exports
            .2
            .call(&mut self.store, ())
            .map_err(|e| self.trap(e))?;
        self.take_output(packed)
    }

    pub fn read(&mut self, capability: &str) -> Result<Result<Value, ReadError>, HostError> {
        let read = self.exports.3;
        self.call_with_input(&types::encode(&capability), |s, p, n| {
            read.call(&mut s.store, (p, n))
        })
    }

    pub fn write(
        &mut self,
        capability: &str,
        value: &Value,
    ) -> Result<Result<WriteOutcome, WriteError>, HostError> {
        let write = self.exports.4;
        self.call_with_input(&types::encode(&(capability, value)), |s, p, n| {
            write.call(&mut s.store, (p, n))
        })
    }

    pub fn on_input(
        &mut self,
        handle: u32,
        report: &[u8],
    ) -> Result<Vec<CapabilityEvent>, HostError> {
        let on_input = self.exports.5;
        self.call_with_input(report, |s, p, n| {
            on_input.call(&mut s.store, (handle, p, n))
        })
    }
}

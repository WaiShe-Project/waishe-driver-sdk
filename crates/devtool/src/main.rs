//! Run a WaiShe driver against a real device, without the Bridge.
//!
//! ```text
//! waishe-driver-dev <driver-dir> <wasm> probe
//! waishe-driver-dev <driver-dir> <wasm> read [capability...]
//! waishe-driver-dev <driver-dir> <wasm> write <capability> <value>
//! waishe-driver-dev <driver-dir> <wasm> listen <seconds>
//! ```
//!
//! `<driver-dir>` holds `driver.toml`. Values: `1600` or `1600x800` for DPI, `true`/`false`,
//! integers, or text. Writes change real device settings: note the old value first.

use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};
use waishe_driver_host::{DriverInstance, DriverModule, HidBackend, Limits, Runtime};
use waishe_driver_types::abi::HidStatus;
use waishe_driver_types::manifest::DriverManifest;
use waishe_driver_types::{EndpointInfo, HidInterface, Value, ValueKind};

/// hidapi devices granted to the driver, indexed by handle.
struct RealHid {
    devices: Vec<HidDevice>,
}

impl RealHid {
    fn dev(&self, h: u32) -> Result<&HidDevice, HidStatus> {
        self.devices.get(h as usize).ok_or(HidStatus::NoHandle)
    }
}

impl HidBackend for RealHid {
    fn send_feature(&mut self, h: u32, data: &[u8]) -> Result<(), HidStatus> {
        self.dev(h)?
            .send_feature_report(data)
            .map_err(|_| HidStatus::Io)
    }
    fn get_feature(&mut self, h: u32, buf: &mut [u8]) -> Result<usize, HidStatus> {
        self.dev(h)?
            .get_feature_report(buf)
            .map_err(|_| HidStatus::Io)
    }
    fn write(&mut self, h: u32, data: &[u8]) -> Result<usize, HidStatus> {
        self.dev(h)?.write(data).map_err(|_| HidStatus::Io)
    }
    fn read(&mut self, h: u32, buf: &mut [u8], timeout_ms: u32) -> Result<usize, HidStatus> {
        self.dev(h)?
            .read_timeout(buf, timeout_ms as i32)
            .map_err(|_| HidStatus::Io)
    }
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        fail(
            "usage: waishe-driver-dev <driver-dir> <wasm> probe | read [cap...] | write <cap> <value> | listen <seconds>",
        );
    }
    let dir = std::path::Path::new(&args[0]);
    let manifest: DriverManifest = toml::from_str(
        &std::fs::read_to_string(dir.join("driver.toml")).unwrap_or_else(|e| fail(e)),
    )
    .unwrap_or_else(|e| fail(format!("driver.toml: {e}")));
    let wasm = std::fs::read(&args[1]).unwrap_or_else(|e| fail(format!("{}: {e}", args[1])));

    // Group matching collections by physical USB device (vendor, product, serial).
    let api = HidApi::new().unwrap_or_else(|e| fail(e));
    let mut endpoint: Option<EndpointInfo> = None;
    let mut devices = Vec::new();
    for d in api.device_list() {
        let hit = manifest.matches.iter().any(|m| {
            m.matches(
                d.vendor_id(),
                d.product_id(),
                d.interface_number(),
                d.usage_page(),
                d.usage(),
            )
        });
        if !hit {
            continue;
        }
        let ep = endpoint.get_or_insert_with(|| EndpointInfo {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            product: d.product_string().unwrap_or_default().to_string(),
            interfaces: vec![],
        });
        if (ep.vendor_id, ep.product_id) != (d.vendor_id(), d.product_id()) {
            continue; // one endpoint per run
        }
        match d.open_device(&api) {
            Ok(dev) => {
                ep.interfaces.push(HidInterface {
                    handle: devices.len() as u32,
                    interface_number: d.interface_number(),
                    usage_page: d.usage_page(),
                    usage: d.usage(),
                });
                devices.push(dev);
            }
            Err(e) => eprintln!(
                "cannot open {:04x}:{:04x} page {:04x}: {e}",
                d.vendor_id(),
                d.product_id(),
                d.usage_page()
            ),
        }
    }
    let endpoint = endpoint.unwrap_or_else(|| fail("no connected device matches driver.toml"));
    println!(
        "endpoint {:04x}:{:04x} \"{}\", {} interface(s)",
        endpoint.vendor_id,
        endpoint.product_id,
        endpoint.product,
        endpoint.interfaces.len()
    );

    let rt = Runtime::new();
    let module = DriverModule::compile(&rt, &wasm).unwrap_or_else(|e| fail(e));
    let mut d = DriverInstance::instantiate(&rt, &module, RealHid { devices }, Limits::default())
        .unwrap_or_else(|e| fail(e));

    let t = Instant::now();
    let probe = d.probe(&endpoint).unwrap_or_else(|e| fail(e));
    let probe = probe.unwrap_or_else(|e| fail(format!("probe: {e:?}")));
    println!(
        "probe ({:?}): model {}, transport {:?}, firmware {:?}, stable id {}, listen {:?}",
        t.elapsed(),
        probe.model,
        probe.transport,
        probe.firmware,
        if probe.stable_id.is_some() {
            "present"
        } else {
            "none"
        },
        probe.listen
    );
    let caps = d.describe().unwrap_or_else(|e| fail(e));

    match args[2].as_str() {
        "probe" => {
            for c in &caps {
                println!("  {} {:?} {:?} {:?}", c.id, c.access, c.kind, c.constraint);
            }
        }
        "read" => {
            let wanted: Vec<&str> = if args.len() > 3 {
                args[3..].iter().map(String::as_str).collect()
            } else {
                caps.iter()
                    .filter(|c| c.access.read)
                    .map(|c| c.id.as_str())
                    .collect()
            };
            for cap in wanted {
                let t = Instant::now();
                let r = d.read(cap);
                println!("  {cap:<20} {r:?}  ({:?})", t.elapsed());
            }
        }
        "write" => {
            let (cap, raw) = (
                args.get(3).unwrap_or_else(|| fail("missing capability")),
                args.get(4).unwrap_or_else(|| fail("missing value")),
            );
            let kind = caps
                .iter()
                .find(|c| &c.id == cap)
                .map(|c| &c.kind)
                .unwrap_or_else(|| fail("unknown capability"));
            let value = parse_value(kind, raw);
            println!("  before: {:?}", d.read(cap));
            let t = Instant::now();
            println!(
                "  write {cap} = {value:?}: {:?}  ({:?})",
                d.write(cap, &value),
                t.elapsed()
            );
        }
        "listen" => {
            let secs: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
            let end = Instant::now() + Duration::from_secs(secs);
            let mut buf = [0u8; 64];
            println!("  listening {secs} s on {:?}", probe.listen);
            while Instant::now() < end {
                for &h in &probe.listen {
                    let n = d.backend_mut().read(h, &mut buf, 200).unwrap_or(0);
                    if n > 0 {
                        for e in d.on_input(h, &buf[..n]).unwrap_or_else(|e| fail(e)) {
                            println!(
                                "  +{:>6} ms  {} = {:?}",
                                (secs * 1000) as i64 - (end - Instant::now()).as_millis() as i64,
                                e.capability,
                                e.reading
                            );
                        }
                    }
                }
            }
        }
        other => fail(format!("unknown command {other}")),
    }
}

fn parse_value(kind: &ValueKind, raw: &str) -> Value {
    match kind {
        ValueKind::Dpi => {
            let (x, y) = raw.split_once('x').unwrap_or((raw, raw));
            let n = |s: &str| s.parse().unwrap_or_else(|_| fail(format!("bad DPI {s}")));
            Value::Dpi { x: n(x), y: n(y) }
        }
        ValueKind::Bool => Value::Bool(
            raw.parse()
                .unwrap_or_else(|_| fail("expected true or false")),
        ),
        ValueKind::Integer { .. } => {
            Value::Integer(raw.parse().unwrap_or_else(|_| fail("expected an integer")))
        }
        ValueKind::Percent => {
            Value::Percent(raw.parse().unwrap_or_else(|_| fail("expected 0-100")))
        }
        ValueKind::Enum { .. } => Value::Enum(raw.into()),
        ValueKind::Text => Value::Text(raw.into()),
    }
}

//! Test a driver offline: the real `.wasm` runs in the real host, against a scripted HID exchange.
//!
//! Script format, one step per line (`#` starts a comment):
//!
//! ```text
//! send_feature 0: 00 00 1f 00 00 00 02 07 80 ..   # expected bytes from the driver
//! get_feature  0: 00 02 1f 00 00 00 02 07 80 00 e3 ..   # bytes handed to the driver
//! write        0: 66 89 ..
//! read         0: 66 89 0e c9 2f ..
//! read         0: timeout
//! ```
//!
//! The number before `:` is the handle. `..` at the end means "zero up to the buffer length":
//! for expected bytes, the driver's remaining bytes must be zero; for returned bytes, the rest of
//! the driver's buffer is filled with zeros. Without `..` lengths must match exactly.

use std::collections::VecDeque;
use std::path::Path;
pub use waishe_driver_host::{DriverInstance, HostError, Limits};
use waishe_driver_host::{DriverModule, HidBackend, Runtime};
pub use waishe_driver_types as types;
use waishe_driver_types::abi::HidStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    SendFeature,
    GetFeature,
    Write,
    Read,
}

#[derive(Debug, Clone)]
struct Step {
    line: usize,
    op: Op,
    handle: u32,
    bytes: Vec<u8>,
    pad: bool,
    timeout: bool,
}

/// A [`HidBackend`] that plays a script. Any deviation panics with the script line.
#[derive(Debug)]
pub struct Replay {
    steps: VecDeque<Step>,
}

impl Replay {
    pub fn parse(script: &str) -> Self {
        let mut steps = VecDeque::new();
        for (i, raw) in script.lines().enumerate() {
            let line = raw.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let (head, body) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("line {}: missing `:`", i + 1));
            let mut head = head.split_whitespace();
            let op = match head.next() {
                Some("send_feature") => Op::SendFeature,
                Some("get_feature") => Op::GetFeature,
                Some("write") => Op::Write,
                Some("read") => Op::Read,
                other => panic!("line {}: unknown operation {other:?}", i + 1),
            };
            let handle = head
                .next()
                .and_then(|h| h.parse().ok())
                .unwrap_or_else(|| panic!("line {}: bad handle", i + 1));
            let body = body.trim();
            let timeout = body == "timeout";
            let pad = body.ends_with("..");
            let bytes = if timeout {
                Vec::new()
            } else {
                body.trim_end_matches("..")
                    .split_whitespace()
                    .map(|b| {
                        u8::from_str_radix(b, 16)
                            .unwrap_or_else(|_| panic!("line {}: bad byte `{b}`", i + 1))
                    })
                    .collect()
            };
            if timeout && op != Op::Read {
                panic!("line {}: only `read` can time out", i + 1);
            }
            steps.push_back(Step {
                line: i + 1,
                op,
                handle,
                bytes,
                pad,
                timeout,
            });
        }
        Replay { steps }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Self {
        let p = path.as_ref();
        Self::parse(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
    }

    /// Panics if steps remain.
    pub fn assert_finished(&self) {
        if let Some(s) = self.steps.front() {
            panic!(
                "script line {}: {:?} was never performed ({} steps left)",
                s.line,
                s.op,
                self.steps.len()
            );
        }
    }

    fn next(&mut self, op: Op, handle: u32) -> Step {
        let s = self.steps.pop_front().unwrap_or_else(|| {
            panic!("driver did {op:?} on handle {handle} after the script ended")
        });
        assert!(
            s.op == op && s.handle == handle,
            "script line {}: expected {:?} on {}, driver did {op:?} on {handle}",
            s.line,
            s.op,
            s.handle
        );
        s
    }

    fn expect(step: &Step, actual: &[u8]) {
        let ok = if step.pad {
            actual.len() >= step.bytes.len()
                && actual[..step.bytes.len()] == step.bytes[..]
                && actual[step.bytes.len()..].iter().all(|&b| b == 0)
        } else {
            actual == &step.bytes[..]
        };
        assert!(
            ok,
            "script line {}: driver sent\n  {}\nexpected\n  {}{}",
            step.line,
            hex(actual),
            hex(&step.bytes),
            if step.pad { " .." } else { "" }
        );
    }

    fn respond(step: &Step, buf: &mut [u8]) -> usize {
        assert!(
            step.bytes.len() <= buf.len(),
            "script line {}: {} bytes do not fit the driver's {}-byte buffer",
            step.line,
            step.bytes.len(),
            buf.len()
        );
        buf[..step.bytes.len()].copy_from_slice(&step.bytes);
        if step.pad {
            buf[step.bytes.len()..].fill(0);
            buf.len()
        } else {
            step.bytes.len()
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl HidBackend for Replay {
    fn send_feature(&mut self, handle: u32, data: &[u8]) -> Result<(), HidStatus> {
        let s = self.next(Op::SendFeature, handle);
        Self::expect(&s, data);
        Ok(())
    }

    fn get_feature(&mut self, handle: u32, buf: &mut [u8]) -> Result<usize, HidStatus> {
        let s = self.next(Op::GetFeature, handle);
        Ok(Self::respond(&s, buf))
    }

    fn write(&mut self, handle: u32, data: &[u8]) -> Result<usize, HidStatus> {
        let s = self.next(Op::Write, handle);
        Self::expect(&s, data);
        Ok(data.len())
    }

    fn read(&mut self, handle: u32, buf: &mut [u8], _timeout_ms: u32) -> Result<usize, HidStatus> {
        let s = self.next(Op::Read, handle);
        Ok(if s.timeout { 0 } else { Self::respond(&s, buf) })
    }
}

/// Loads a built driver and instantiates it against a script.
pub fn instantiate(wasm: impl AsRef<Path>, script: Replay) -> DriverInstance<Replay> {
    let p = wasm.as_ref();
    let bytes = std::fs::read(p).unwrap_or_else(|e| {
        panic!("{}: {e}\nbuild the driver first: cargo build --release --target wasm32-unknown-unknown", p.display())
    });
    let rt = Runtime::new();
    let module = DriverModule::compile(&rt, &bytes).expect("compile driver");
    DriverInstance::instantiate(&rt, &module, script, Limits::default())
        .expect("instantiate driver")
}

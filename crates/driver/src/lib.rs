//! Write a WaiShe driver.
//!
//! Implement [`Driver`] and call [`export_driver!`] once. Build the crate as a `cdylib` for
//! `wasm32-unknown-unknown`. The host creates one instance per endpoint, so the driver may keep
//! per-device state in `self`.
//!
//! ```ignore
//! struct MyDriver { h: hid::Handle }
//! impl Driver for MyDriver { /* ... */ }
//! waishe_driver::export_driver!(MyDriver);
//! ```
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

pub use waishe_driver_types as types;
pub use waishe_driver_types::cap::{charging, id, link};
pub use waishe_driver_types::{
    Access, CapabilityDescriptor, CapabilityEvent, Constraint, EndpointInfo, HidInterface,
    Persistence, Precision, ProbeError, ProbeResult, ReadError, Transport, Unavailable, Value,
    ValueKind, WriteError, WriteOutcome,
};

pub mod hid;
#[doc(hidden)]
pub mod rt;
mod sys;

/// A protocol adapter for one endpoint.
pub trait Driver: Sized + 'static {
    /// Decides whether the endpoint is this driver's and identifies the device. Must not change
    /// any device setting.
    fn probe(endpoint: &EndpointInfo) -> Result<(Self, ProbeResult), ProbeError>;

    /// Capabilities available on this endpoint, with constraints for its current transport.
    fn describe(&self) -> Vec<CapabilityDescriptor>;

    /// Reads one capability. Return `Unavailable` rather than a made-up value when the device
    /// does not answer.
    fn read(&mut self, capability: &str) -> Result<Value, ReadError>;

    /// Writes one capability. Read back when the protocol allows it and return `Applied`.
    fn write(&mut self, capability: &str, value: &Value) -> Result<WriteOutcome, WriteError> {
        let _ = (capability, value);
        Err(WriteError::Unsupported)
    }

    /// Called with each input report on the handles listed in `ProbeResult::listen`.
    /// Input reports reach every open handle, including other software's: ignore unrelated ones.
    fn on_input(&mut self, handle: u32, report: &[u8]) -> Vec<CapabilityEvent> {
        let _ = (handle, report);
        Vec::new()
    }
}

/// Milliseconds since this driver instance was created.
pub fn now_ms() -> u64 {
    sys::now_ms()
}

/// Sleeps; the host clamps long durations.
pub fn sleep_ms(ms: u32) {
    sys::sleep_ms(ms)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

/// Logs through the host. Never log serial numbers or full reports.
pub fn log(level: Level, message: &str) {
    sys::log(level as i32, message)
}

/// Exports the ABI functions for a [`Driver`] type. Use exactly once per driver crate.
#[macro_export]
macro_rules! export_driver {
    ($driver:ty) => {
        static __WAISHE_DRIVER: $crate::rt::Slot<$driver> = $crate::rt::Slot::new();

        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_abi() -> i32 {
            $crate::types::abi::VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_alloc(len: u32) -> u32 {
            $crate::rt::alloc(len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_probe(ptr: u32, len: u32) -> i64 {
            $crate::rt::probe(&__WAISHE_DRIVER, ptr, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_describe() -> i64 {
            $crate::rt::describe(&__WAISHE_DRIVER)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_read(ptr: u32, len: u32) -> i64 {
            $crate::rt::read(&__WAISHE_DRIVER, ptr, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_write(ptr: u32, len: u32) -> i64 {
            $crate::rt::write(&__WAISHE_DRIVER, ptr, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn waishe_on_input(handle: u32, ptr: u32, len: u32) -> i64 {
            $crate::rt::on_input(&__WAISHE_DRIVER, handle, ptr, len)
        }
    };
}

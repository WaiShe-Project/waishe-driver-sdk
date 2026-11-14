//! Types shared by WaiShe drivers and the host that runs them.
//!
//! Everything crossing the WebAssembly boundary is one of these types, encoded with
//! [postcard](https://docs.rs/postcard). See `docs/driver-interface.md` for the ABI.
#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub mod abi;
pub mod cap;
pub mod manifest;

/// Encodes a value for the ABI boundary.
pub fn encode<T: Serialize>(v: &T) -> Vec<u8> {
    postcard::to_allocvec(v).expect("postcard encoding into a Vec cannot fail")
}

/// Decodes a value received over the ABI boundary.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

// ---------------------------------------------------------------- endpoint and probe

/// One HID top-level collection the host has opened and granted to the driver instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidInterface {
    /// Handle to pass to the HID host functions. Only valid inside this driver instance.
    pub handle: u32,
    /// USB interface number, or -1 when unknown.
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
}

/// What the host knows about an endpoint before the driver has looked at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    /// Collections that matched the driver's manifest rules, already opened by the host.
    pub interfaces: Vec<HidInterface>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Wired,
    Receiver,
    Bluetooth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    /// `model` of an entry in the driver's `devices.toml`.
    pub model: String,
    /// Identifier that is the same on every endpoint of one physical device, typically the
    /// serial number. The host uses it only locally, to merge endpoints, and never exposes it.
    pub stable_id: Option<String>,
    pub transport: Transport,
    pub firmware: Option<String>,
    /// Handles whose input reports the host must read and pass to `on_input`.
    pub listen: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeError {
    /// Not a device this driver handles. The host will not retry.
    NotMine,
    /// Could not decide now (for example a receiver whose device is asleep). The host retries later.
    Retry,
    Io(String),
}

// ---------------------------------------------------------------- capabilities

pub use cap::{Access, CapabilityDescriptor, Constraint, Persistence, Precision, ValueKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Percent(u8),
    Bool(bool),
    Integer(i64),
    Enum(String),
    Text(String),
    Dpi { x: u32, y: u32 },
}

/// Why a capability the device has cannot be read or written right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unavailable {
    Asleep,
    Disconnected,
    Busy,
    Timeout,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadError {
    Unsupported,
    Unavailable(Unavailable),
    Protocol(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteOutcome {
    /// Written and confirmed by reading back; carries the value read back.
    Applied(Value),
    /// The device acknowledged but the protocol cannot read the value back.
    Accepted,
    /// The write makes the device re-enumerate. The host waits for the endpoint and reads back.
    PendingReenumeration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteError {
    /// Invalid value, or refused by the device. Nothing changed.
    Rejected(String),
    Unsupported,
    /// Device state is unknown afterwards; the host reads again.
    Unavailable(Unavailable),
    Protocol(String),
}

/// A change reported by the device without a request, or derived from one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEvent {
    pub capability: String,
    pub reading: Result<Value, ReadError>,
}

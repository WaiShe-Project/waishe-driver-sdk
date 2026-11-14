//! HID access through the host. A [`Handle`] only reaches collections the host granted.

use crate::sys;
use crate::types::Unavailable;
use crate::types::abi::HidStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidError {
    NoHandle,
    Disconnected,
    Io,
    Busy,
    Invalid,
    Unknown(i32),
}

impl HidError {
    fn from_code(code: i32) -> Self {
        match HidStatus::from_code(code) {
            Some(HidStatus::NoHandle) => Self::NoHandle,
            Some(HidStatus::Disconnected) => Self::Disconnected,
            Some(HidStatus::Io) => Self::Io,
            Some(HidStatus::Busy) => Self::Busy,
            Some(HidStatus::Invalid) => Self::Invalid,
            None => Self::Unknown(code),
        }
    }
}

impl From<HidError> for Unavailable {
    fn from(e: HidError) -> Self {
        match e {
            HidError::Disconnected => Unavailable::Disconnected,
            HidError::Busy => Unavailable::Busy,
            _ => Unavailable::Io,
        }
    }
}

impl Handle {
    /// Sends a feature report. `data[0]` is the report id (0 when the device uses none).
    pub fn send_feature(self, data: &[u8]) -> Result<(), HidError> {
        match sys::send_feature(self.0, data) {
            c if c < 0 => Err(HidError::from_code(c)),
            _ => Ok(()),
        }
    }

    /// Gets a feature report. Set `buf[0]` to the report id first. Returns the bytes read.
    pub fn get_feature(self, buf: &mut [u8]) -> Result<usize, HidError> {
        match sys::get_feature(self.0, buf) {
            c if c < 0 => Err(HidError::from_code(c)),
            n => Ok(n as usize),
        }
    }

    /// Writes an output report. `data[0]` is the report id.
    pub fn write(self, data: &[u8]) -> Result<usize, HidError> {
        match sys::write(self.0, data) {
            c if c < 0 => Err(HidError::from_code(c)),
            n => Ok(n as usize),
        }
    }

    /// Reads one input report. `Ok(None)` on timeout.
    ///
    /// Only use this for request/response exchanges. Reports the device sends on its own are
    /// delivered to [`crate::Driver::on_input`] for handles listed in `ProbeResult::listen`.
    pub fn read(self, buf: &mut [u8], timeout_ms: u32) -> Result<Option<usize>, HidError> {
        match sys::read(self.0, buf, timeout_ms) {
            c if c < 0 => Err(HidError::from_code(c)),
            0 => Ok(None),
            n => Ok(Some(n as usize)),
        }
    }
}

//! Names and numbers of the core-module ABI. The prose specification is `docs/driver-interface.md`.

/// ABI version implemented by this crate. Before 1.0, a different minor version is incompatible.
pub const VERSION_MAJOR: u16 = 0;
pub const VERSION_MINOR: u16 = 1;

/// `VERSION_MAJOR << 16 | VERSION_MINOR`, as returned by the guest's [`EXPORT_ABI`] function.
pub const VERSION: i32 = ((VERSION_MAJOR as i32) << 16) | VERSION_MINOR as i32;

/// Module name of every host import.
pub const IMPORT_MODULE: &str = "waishe";

// Host imports. Buffers are (pointer, length) into guest memory.
/// `(handle, ptr, len) -> i32`: 0, or a negative [`HidStatus`].
pub const IMPORT_HID_SEND_FEATURE: &str = "hid_send_feature";
/// `(handle, ptr, len) -> i32`: bytes read, or a negative [`HidStatus`]. `ptr[0]` holds the report id.
pub const IMPORT_HID_GET_FEATURE: &str = "hid_get_feature";
/// `(handle, ptr, len) -> i32`: bytes written, or a negative [`HidStatus`].
pub const IMPORT_HID_WRITE: &str = "hid_write";
/// `(handle, ptr, len, timeout_ms) -> i32`: bytes read, 0 on timeout, or a negative [`HidStatus`].
pub const IMPORT_HID_READ: &str = "hid_read";
/// `(ms)`: the host clamps the duration.
pub const IMPORT_SLEEP_MS: &str = "sleep_ms";
/// `() -> i64`: monotonic milliseconds since the instance was created.
pub const IMPORT_NOW_MS: &str = "now_ms";
/// `(level, ptr, len)`: UTF-8 message. Level 0 error, 1 warn, 2 info, 3 debug.
pub const IMPORT_LOG: &str = "log";

// Guest exports. Results are returned as `ptr << 32 | len` of an encoded value in guest memory,
// valid until the next call into the guest.
/// `() -> i32`: [`VERSION`].
pub const EXPORT_ABI: &str = "waishe_abi";
/// `(len) -> i32`: pointer to a guest buffer of `len` bytes for the next call's input.
pub const EXPORT_ALLOC: &str = "waishe_alloc";
/// `(ptr, len) -> i64`: input `EndpointInfo`, output `Result<ProbeResult, ProbeError>`.
pub const EXPORT_PROBE: &str = "waishe_probe";
/// `() -> i64`: output `Vec<CapabilityDescriptor>`.
pub const EXPORT_DESCRIBE: &str = "waishe_describe";
/// `(ptr, len) -> i64`: input capability id `String`, output `Result<Value, ReadError>`.
pub const EXPORT_READ: &str = "waishe_read";
/// `(ptr, len) -> i64`: input `(String, Value)`, output `Result<WriteOutcome, WriteError>`.
pub const EXPORT_WRITE: &str = "waishe_write";
/// `(handle, ptr, len) -> i64`: input raw report bytes, output `Vec<CapabilityEvent>`.
pub const EXPORT_ON_INPUT: &str = "waishe_on_input";

/// Negative status codes returned by the HID host functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HidStatus {
    /// The handle was not granted to this instance.
    NoHandle = -1,
    Disconnected = -2,
    Io = -3,
    Busy = -4,
    /// The buffer is too large or otherwise invalid.
    Invalid = -5,
}

impl HidStatus {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            -1 => Self::NoHandle,
            -2 => Self::Disconnected,
            -3 => Self::Io,
            -4 => Self::Busy,
            -5 => Self::Invalid,
            _ => return None,
        })
    }
}

/// Packs a guest buffer location into the `i64` returned by exports.
pub const fn pack(ptr: u32, len: u32) -> i64 {
    ((ptr as u64) << 32 | len as u64) as i64
}

/// Inverse of [`pack`].
pub const fn unpack(v: i64) -> (u32, u32) {
    ((v as u64 >> 32) as u32, v as u64 as u32)
}

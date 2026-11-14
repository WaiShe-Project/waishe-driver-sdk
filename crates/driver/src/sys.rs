//! Raw host imports. On non-wasm targets (unit tests of pure codec code) they are unavailable.

#[cfg(target_arch = "wasm32")]
mod imp {
    #[link(wasm_import_module = "waishe")]
    unsafe extern "C" {
        fn hid_send_feature(handle: u32, ptr: *const u8, len: u32) -> i32;
        fn hid_get_feature(handle: u32, ptr: *mut u8, len: u32) -> i32;
        fn hid_write(handle: u32, ptr: *const u8, len: u32) -> i32;
        fn hid_read(handle: u32, ptr: *mut u8, len: u32, timeout_ms: u32) -> i32;
        fn sleep_ms(ms: u32);
        fn now_ms() -> i64;
        fn log(level: i32, ptr: *const u8, len: u32);
    }

    pub fn send_feature(h: u32, d: &[u8]) -> i32 {
        unsafe { hid_send_feature(h, d.as_ptr(), d.len() as u32) }
    }
    pub fn get_feature(h: u32, b: &mut [u8]) -> i32 {
        unsafe { hid_get_feature(h, b.as_mut_ptr(), b.len() as u32) }
    }
    pub fn write(h: u32, d: &[u8]) -> i32 {
        unsafe { hid_write(h, d.as_ptr(), d.len() as u32) }
    }
    pub fn read(h: u32, b: &mut [u8], timeout_ms: u32) -> i32 {
        unsafe { hid_read(h, b.as_mut_ptr(), b.len() as u32, timeout_ms) }
    }
    pub fn sleep(ms: u32) {
        unsafe { sleep_ms(ms) }
    }
    pub fn now() -> u64 {
        unsafe { now_ms() as u64 }
    }
    pub fn log_msg(level: i32, m: &str) {
        unsafe { log(level, m.as_ptr(), m.len() as u32) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    const MSG: &str = "WaiShe host functions exist only inside the host (wasm32 target)";
    pub fn send_feature(_: u32, _: &[u8]) -> i32 {
        panic!("{MSG}")
    }
    pub fn get_feature(_: u32, _: &mut [u8]) -> i32 {
        panic!("{MSG}")
    }
    pub fn write(_: u32, _: &[u8]) -> i32 {
        panic!("{MSG}")
    }
    pub fn read(_: u32, _: &mut [u8], _: u32) -> i32 {
        panic!("{MSG}")
    }
    pub fn sleep(_: u32) {
        panic!("{MSG}")
    }
    pub fn now() -> u64 {
        panic!("{MSG}")
    }
    pub fn log_msg(_: i32, _: &str) {}
}

pub(crate) use imp::{get_feature, read, send_feature, write};

pub(crate) fn sleep_ms(ms: u32) {
    imp::sleep(ms)
}
pub(crate) fn now_ms() -> u64 {
    imp::now()
}
pub(crate) fn log(level: i32, m: &str) {
    imp::log_msg(level, m)
}

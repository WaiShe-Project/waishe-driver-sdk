//! Glue behind [`crate::export_driver!`]. Not a stable API.

use crate::Driver;
use crate::types::{
    self, CapabilityEvent, EndpointInfo, ProbeError, ReadError, Value, WriteError, abi,
};
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

/// Driver storage. WebAssembly instances are single-threaded, so `Sync` is sound here.
pub struct Slot<T>(RefCell<Option<T>>);

unsafe impl<T> Sync for Slot<T> {}

impl<T> Slot<T> {
    pub const fn new() -> Self {
        Slot(RefCell::new(None))
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct Buffers {
    input: Vec<u8>,
    output: Vec<u8>,
}

static BUFFERS: Slot<Buffers> = Slot::new();

fn with_buffers<R>(f: impl FnOnce(&mut Buffers) -> R) -> R {
    let mut b = BUFFERS.0.borrow_mut();
    f(b.get_or_insert_with(|| Buffers {
        input: Vec::new(),
        output: Vec::new(),
    }))
}

pub fn alloc(len: u32) -> u32 {
    with_buffers(|b| {
        b.input.clear();
        b.input.resize(len as usize, 0);
        b.input.as_mut_ptr() as u32
    })
}

/// Copies the input the host wrote after `alloc`. Panics (trap) if the host passed another buffer.
fn take_input(ptr: u32, len: u32) -> Vec<u8> {
    with_buffers(|b| {
        assert!(
            ptr == b.input.as_ptr() as u32 && len as usize <= b.input.len(),
            "input outside the alloc buffer"
        );
        core::mem::take(&mut b.input)
    })
}

fn output<T: serde::Serialize>(v: &T) -> i64 {
    let bytes = types::encode(v);
    with_buffers(|b| {
        b.output = bytes;
        abi::pack(b.output.as_ptr() as u32, b.output.len() as u32)
    })
}

fn decode<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> T {
    types::decode(bytes).expect("host sent an undecodable value")
}

pub fn probe<D: Driver>(slot: &Slot<D>, ptr: u32, len: u32) -> i64 {
    let input = take_input(ptr, len);
    let ep: EndpointInfo = decode(&input[..len as usize]);
    let result = match D::probe(&ep) {
        Ok((d, r)) => {
            *slot.0.borrow_mut() = Some(d);
            Ok(r)
        }
        Err(e) => Err(e),
    };
    output::<Result<types::ProbeResult, ProbeError>>(&result)
}

fn with_driver<D: Driver, R>(slot: &Slot<D>, f: impl FnOnce(&mut D) -> R) -> R {
    let mut s = slot.0.borrow_mut();
    f(s.as_mut().expect("called before a successful probe"))
}

pub fn describe<D: Driver>(slot: &Slot<D>) -> i64 {
    let d = with_driver(slot, |d| d.describe());
    output(&d)
}

pub fn read<D: Driver>(slot: &Slot<D>, ptr: u32, len: u32) -> i64 {
    let input = take_input(ptr, len);
    let id: String = decode(&input[..len as usize]);
    let r: Result<Value, ReadError> = with_driver(slot, |d| d.read(&id));
    output(&r)
}

pub fn write<D: Driver>(slot: &Slot<D>, ptr: u32, len: u32) -> i64 {
    let input = take_input(ptr, len);
    let (id, value): (String, Value) = decode(&input[..len as usize]);
    let r: Result<types::WriteOutcome, WriteError> = with_driver(slot, |d| d.write(&id, &value));
    output(&r)
}

pub fn on_input<D: Driver>(slot: &Slot<D>, handle: u32, ptr: u32, len: u32) -> i64 {
    let input = take_input(ptr, len);
    let events: Vec<CapabilityEvent> =
        with_driver(slot, |d| d.on_input(handle, &input[..len as usize]));
    output(&events)
}

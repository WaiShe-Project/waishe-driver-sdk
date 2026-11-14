//! Test fixture for fault isolation (requirement DRV-05, acceptance AC-10).
//!
//! Probes successfully and then never returns from `read`: the host must stop it with its fuel
//! or deadline limit, restart it, and after repeated faults disable it, while every other driver
//! keeps working. Never install this outside a test.

use waishe_driver::{
    Access, CapabilityDescriptor, Driver, EndpointInfo, ProbeError, ProbeResult, ReadError, Transport, Value, ValueKind,
    id,
};

struct Faulty;

impl Driver for Faulty {
    fn probe(_: &EndpointInfo) -> Result<(Self, ProbeResult), ProbeError> {
        Ok((
            Faulty,
            ProbeResult { model: "faulty".into(), stable_id: None, transport: Transport::Receiver, firmware: None, listen: vec![] },
        ))
    }

    fn describe(&self) -> Vec<CapabilityDescriptor> {
        vec![CapabilityDescriptor::new(id::BATTERY_LEVEL, Access::READ, ValueKind::Percent).poll_every(300)]
    }

    fn read(&mut self, _: &str) -> Result<Value, ReadError> {
        let mut x = 0u64;
        loop {
            x = core::hint::black_box(x.wrapping_add(1));
        }
    }
}

waishe_driver::export_driver!(Faulty);

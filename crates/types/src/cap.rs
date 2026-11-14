//! Capability descriptors and the standard capability catalog. Prose: `docs/capabilities.md`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Standard capability ids.
pub mod id {
    pub const BATTERY_LEVEL: &str = "battery.level";
    pub const BATTERY_CHARGING: &str = "battery.charging";
    pub const LINK_STATE: &str = "link.state";
    pub const DEVICE_FIRMWARE: &str = "device.firmware";
    pub const MOUSE_DPI: &str = "mouse.dpi";
    pub const MOUSE_POLLING_RATE: &str = "mouse.polling_rate";
    pub const HEADSET_MIC_MUTE: &str = "headset.mic_mute";
}

/// Values of `battery.charging`.
pub mod charging {
    pub const DISCHARGING: &str = "discharging";
    pub const CHARGING: &str = "charging";
    pub const FULL: &str = "full";
}

/// Values of `link.state`.
pub mod link {
    pub const CONNECTED: &str = "connected";
    pub const ASLEEP: &str = "asleep";
    pub const DISCONNECTED: &str = "disconnected";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Access {
    pub read: bool,
    pub write: bool,
    /// The driver emits this capability from `on_input`.
    pub notify: bool,
}

impl Access {
    pub const READ: Self = Self {
        read: true,
        write: false,
        notify: false,
    };
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        notify: false,
    };
    pub const READ_NOTIFY: Self = Self {
        read: true,
        write: false,
        notify: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueKind {
    Percent,
    Bool,
    Integer { unit: String },
    Enum { variants: Vec<String> },
    Text,
    Dpi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    Range {
        min: i64,
        max: i64,
        step: i64,
    },
    Choices(Vec<i64>),
    Dpi {
        min: u32,
        max: u32,
        step: u32,
        independent_xy: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Persistence {
    /// Stored on the device; survives power cycles.
    Onboard,
    /// Lost on power cycle or reconnect.
    Volatile,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Precision {
    Exact,
    /// The device reports one of `n` levels; the percentage is approximate.
    Steps(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: String,
    pub access: Access,
    pub kind: ValueKind,
    /// Required for writable capabilities.
    pub constraint: Option<Constraint>,
    /// Required for writable capabilities.
    pub persistence: Option<Persistence>,
    /// Suggested polling interval for readable capabilities without notifications.
    pub poll_interval_s: Option<u32>,
    /// Required for `battery.level`.
    pub precision: Option<Precision>,
}

impl CapabilityDescriptor {
    pub fn new(id: &str, access: Access, kind: ValueKind) -> Self {
        Self {
            id: id.into(),
            access,
            kind,
            constraint: None,
            persistence: None,
            poll_interval_s: None,
            precision: None,
        }
    }
    pub fn constraint(mut self, c: Constraint) -> Self {
        self.constraint = Some(c);
        self
    }
    pub fn persistence(mut self, p: Persistence) -> Self {
        self.persistence = Some(p);
        self
    }
    pub fn poll_every(mut self, seconds: u32) -> Self {
        self.poll_interval_s = Some(seconds);
        self
    }
    pub fn precision(mut self, p: Precision) -> Self {
        self.precision = Some(p);
        self
    }
}

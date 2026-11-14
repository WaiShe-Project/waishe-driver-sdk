//! `driver.toml` and `devices.toml`. Parsing is left to the host; these are the shapes.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverManifest {
    pub driver: DriverInfo,
    #[serde(rename = "match")]
    pub matches: Vec<MatchRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverInfo {
    /// Unique within the registry: lowercase letters, digits and `-`.
    pub id: String,
    pub name: String,
    /// Semantic version of the driver.
    pub version: String,
    /// Driver ABI version, `"major.minor"`.
    pub api: String,
    /// SPDX license expression.
    pub license: String,
    pub repository: String,
    #[serde(default)]
    pub description: String,
}

/// A HID collection the driver wants. Every present field must match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRule {
    pub vendor_id: u16,
    pub product_ids: Vec<u16>,
    #[serde(default)]
    pub interface_number: Option<i32>,
    #[serde(default)]
    pub usage_page: Option<u16>,
    #[serde(default)]
    pub usage: Option<u16>,
}

impl MatchRule {
    pub fn matches(
        &self,
        vid: u16,
        pid: u16,
        interface_number: i32,
        usage_page: u16,
        usage: u16,
    ) -> bool {
        self.vendor_id == vid
            && self.product_ids.contains(&pid)
            && self.interface_number.is_none_or(|n| n == interface_number)
            && self.usage_page.is_none_or(|p| p == usage_page)
            && self.usage.is_none_or(|u| u == usage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceTable {
    #[serde(rename = "device")]
    pub devices: Vec<DeviceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub model: String,
    pub name: String,
    pub brand: String,
    /// `mouse`, `headset`, `keyboard`, ...
    pub kind: String,
    #[serde(rename = "endpoint")]
    pub endpoints: Vec<DeviceEndpoint>,
    #[serde(rename = "verification", default)]
    pub verifications: Vec<Verification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEndpoint {
    /// `wired`, `receiver` or `bluetooth`.
    pub transport: String,
    pub product_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    pub capability: String,
    pub transport: String,
    /// `verified`, `simulated`, `inferred` or `broken`.
    pub level: String,
    #[serde(default)]
    pub firmware: Option<String>,
    #[serde(default)]
    pub verified_by: Option<String>,
    #[serde(default)]
    pub verified_on: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

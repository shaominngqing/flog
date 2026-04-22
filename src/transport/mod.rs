//! Transport layer — device discovery and platform-specific connectivity.

pub mod adb;
pub mod device_monitor;
pub mod usbmuxd;

pub use device_monitor::{start_discovery, ConnectionMethod, Device, DeviceEvent, DeviceKind};

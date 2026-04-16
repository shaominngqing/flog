//! Transport layer — device discovery and platform-specific connectivity.

pub mod device_monitor;
pub mod adb;
pub mod usbmuxd;

pub use device_monitor::{Device, DeviceKind, DeviceEvent, ConnectionMethod, start_discovery};

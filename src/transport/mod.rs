//! Transport layer — device discovery and platform-specific connectivity.

pub mod device_monitor;
pub mod adb;
pub mod flutter_logs;
pub mod usbmuxd;

pub use device_monitor::{DeviceMonitor, FlutterDevice, ConnectionMethod};

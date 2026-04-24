//! Transport layer — device discovery and platform-specific connectivity.

pub mod adb;
pub mod device_monitor;
pub mod resolve;
pub mod usbmuxd;

pub use device_monitor::{start_discovery, DeviceEvent};
// `ConnectionMethod` remains in `device_monitor` — only the transport-plan
// abstraction (`TransportAddr`) is needed by main.rs now (TRANS-009).
pub use resolve::{resolve_transport_addr, TransportAddr};

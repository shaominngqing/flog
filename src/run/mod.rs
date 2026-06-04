//! Startup wiring + top-level Tokio tasks.
//!
//! Split out of `main.rs` as of Phase 4. `main()` itself now only
//! handles CLI parsing, terminal lifecycle, and panic hooks; everything
//! else lives here.

mod dispatch;
mod render_loop;
mod server;

pub(crate) use dispatch::dispatch_client_message;
pub(crate) use render_loop::run_loop;
pub(crate) use server::{spawn_device_discovery, spawn_switch_app_handler};

// Test-only re-exports exercised by `main_tests.rs`.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use dispatch::{format_ts, split_stacktrace, RAW_LOG_RE};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use server::{
    RECONNECT_BACKOFF_FACTOR, RECONNECT_INITIAL_DELAY_SECS, RECONNECT_MAX_DELAY_SECS,
};

#[cfg(test)]
#[path = "../main_tests.rs"]
mod tests;

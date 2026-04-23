//! Shared helpers for Phase 2.5B characterization tests.
//!
//! Each tests/characterization_*.rs crate includes this via:
//!   #[path = "support/mod.rs"] mod support;
#![allow(dead_code)]

pub mod fake_flog_server;
pub mod fixtures;
pub mod ui_inspect;

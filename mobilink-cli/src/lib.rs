//! Mobilink CLI library: everything the `mobilink` binary does, split into
//! testable modules. The binary itself (main.rs) only parses arguments and
//! wires these pieces together.

pub mod args;
pub mod local;
pub mod tls;
pub mod tunnel;
pub mod ui;

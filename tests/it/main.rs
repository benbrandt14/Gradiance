//! Umbrella integration-test binary.
//!
//! Every Bevy-linking integration test lives here as a module, so the suite
//! pays **one** Bevy-sized link instead of one per file (which previously
//! dominated CI time and container memory). Add new integration tests as a
//! module in this directory and declare it below. Filter as usual:
//! `cargo test --test it csg::`.
//!
//! `tests/boundaries.rs` intentionally stays a separate binary: it links no
//! Bevy and CI runs it as an early, fast gate.

mod harness;

mod csg;
mod editor_ops;
mod interaction;
mod joint_edit;
mod joints;
mod persistence;
mod physics_sync;
mod reflect_intents;
mod registry_validation;
mod scripting;
mod tools;
mod unit_commands;

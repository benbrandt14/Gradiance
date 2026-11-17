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

mod array_drag;
mod booleans;
mod csg;
mod editor_ops;
mod groups;
mod history;
mod interaction;
mod joint_breaking;
mod joint_edit;
mod joint_sensors;
mod joints;
mod nodes;
mod op_conformance;
mod packing;
mod persistence;
mod physics_sync;
mod reflect_intents;
mod registry_validation;
mod remote;
mod replay;
mod review;
mod rod;
mod script_examples;
mod script_tools;
mod scripting;
mod signals;
mod tools;
mod ui_conflicts;
mod ui_input;
mod ui_panels;
mod ui_registry;
mod ui_snapshots;
mod unit_commands;

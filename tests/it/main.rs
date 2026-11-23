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

mod actuators;
mod adjust_last;
mod air_drag;
mod array_drag;
mod body_arrows;
mod booleans;
mod buoyancy;
mod camera_views;
mod chains;
mod client;
mod clipboard;
mod context_menu;
mod contour_cache;
mod conveyor;
mod csg;
mod cut_select_stress;
mod debug_overlay;
mod defaults;
mod editor_ops;
mod gears;
mod graph_keys;
mod groups;
mod headless_run;
mod history;
mod image_import;
mod initial_motion;
mod interaction;
mod joint_breaking;
mod joint_edit;
mod joint_sensors;
mod joint_stress;
mod joints;
mod lasers;
mod lifetime;
mod loader_fuzz;
mod look;
mod mirror;
mod new_scene;
mod nodes;
mod op_conformance;
mod packing;
mod particle_water;
mod persistence;
mod phase_plot;
mod physics_sync;
mod quantities;
mod reflect_intents;
mod registry_validation;
mod remote;
mod replay;
mod review;
mod rod;
mod run_read;
mod script_examples;
mod script_tools;
mod script_units;
mod scripting;
mod signal_bind;
mod signals;
mod sketch;
mod sleeping;
mod sponge;
mod texture;
mod thrusters;
mod tools;
mod ui_conflicts;
mod ui_groups;
mod ui_input;
mod ui_panels;
mod ui_registry;
mod ui_snapshots;
mod ui_surfaces;
mod ui_workspaces;
mod undo_matrix;
mod unit_commands;
mod units_display;
mod water;

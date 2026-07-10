//! Scripting layer (spike stage).
//!
//! This module is the seed of the scripting / symbolic-modeling feature
//! described in `docs/script-lisp-decision.md`. It is being grown through
//! *linchpin spikes* before any user-facing surface lands — the spikes
//! answer the load-bearing feasibility questions that the whole design
//! rests on.
//!
//! Present contents:
//!
//! - [`kernel`] — **Spike 2 (perf).** Proves the "Tier-B" hot-path claim:
//!   a numeric driver expression compiles to a flat, allocation-free tape
//!   that evaluates over columnar (structure-of-arrays) data with no
//!   interpreter/VM and no per-element allocation in the loop. This is the
//!   property that keeps particle/fluid-scale per-frame updates feasible.
//!
//! - [`reflect_bridge`] — **Spike 1 (feature-gated).** The generic
//!   `bevy_reflect` <-> steel value bridge behind the `script` feature: read
//!   any `#[derive(Reflect)]` value by reflect-path, write scalars back, and
//!   convert whole values to steel data. Off by default; alongside
//!   [`bridge`] the only place `steel` may be imported.
//!
//! - [`bridge`] — **P1: the World-facing seam (feature-gated).** Embeds the
//!   steel engine, registers scene-verb builtins, and runs the exclusive
//!   `run_scripts` system that dispatches script-emitted operation records
//!   through the existing intent bus (the World-integration constraint —
//!   scripts emit intent *data*, never `&mut World`). The only ECS-touching
//!   part of the module.
//!
//! Not yet present (later phases, see the decision record): the driver
//! component + dataflow seam and the REPL panel. The pure core (`kernel`) has
//! **no ECS imports**, exactly as the `geometry` module is structured.

pub mod kernel;

#[cfg(feature = "script")]
pub mod bridge;

#[cfg(feature = "script")]
pub mod reflect_bridge;

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
//!   convert whole values to steel data. Off by default; the only place
//!   `steel` may be imported.
//!
//! Not yet present (later phases, see the decision record): the World-facing
//! operation registry that dispatches through intents/settings, the driver
//! component + dataflow seam, and the REPL. The pure core (`kernel`) has
//! **no ECS imports**, exactly as the `geometry` module is structured.

pub mod kernel;

#[cfg(feature = "script")]
pub mod reflect_bridge;

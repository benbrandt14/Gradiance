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
//! Not yet present (later phases, see the decision record): the steel
//! embedding, the `bevy_reflect`-backed operation registry, the driver
//! component + dataflow seam, and the REPL. This module intentionally has
//! **no ECS imports** yet — the pure core comes first, exactly as the
//! `geometry` module is structured.

pub mod kernel;

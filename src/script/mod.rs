//! Scripting: the tool's Lisp control plane (`docs/script-lisp-decision.md`).
//!
//! A first-class, always-compiled part of the tool — a steel (Scheme) VM whose
//! scene verbs re-enter the *same* intent seam tools and UI use, so scripting
//! is a governed doorway, never a bypass around the command discipline. The
//! two-tier PERF rule is what makes an always-on VM acceptable: the VM runs
//! only on the authoring (cold) path, **never** in the per-frame loop —
//! continuous drivers lower to the numeric [`kernel`] instead.
//!
//! Contents:
//!
//! - [`kernel`] — **Tier B (hot path).** A numeric driver expression compiles
//!   to a flat, allocation-free tape that evaluates over columnar
//!   (structure-of-arrays) data with no interpreter/VM and no per-element
//!   allocation in the loop — the property that keeps particle/fluid-scale
//!   per-frame updates feasible. Pure, no ECS imports (like `geometry`).
//!
//! - [`reflect_bridge`] — the generic `bevy_reflect` <-> steel value bridge:
//!   read any `#[derive(Reflect)]` value by reflect-path, write scalars back,
//!   and convert a whole value to steel data (the read-total path). Pure
//!   w.r.t. the ECS.
//!
//! - [`bridge`] — the **World-facing seam** (the only ECS-touching part).
//!   Embeds the steel engine, registers scene-verb builtins, and runs the
//!   exclusive `run_scripts` system that dispatches script-emitted operation
//!   records through the intent bus (the World-integration constraint —
//!   scripts emit intent *data*, never `&mut World`).
//!
//! `steel` may be imported only in [`bridge`]/[`reflect_bridge`] (enforced by
//! `tests/boundaries.rs`). Later phases (see the decision record): the driver
//! component + named-signal dataflow seam over [`kernel`], and the REPL panel.

pub mod bridge;
pub mod kernel;
pub mod reflect_bridge;

pub use bridge::ScriptPlugin;

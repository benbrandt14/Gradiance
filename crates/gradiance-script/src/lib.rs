//! Scripting: the tool's Lisp control plane (`docs/script-lisp-decision.md`).
//!
//! A first-class, always-compiled part of the tool — a steel (Scheme) VM whose
//! scene verbs re-enter the *same* intent seam tools and UI use, so scripting
//! is a governed doorway, never a bypass around the command discipline. The
//! two-tier PERF rule is what makes an always-on VM acceptable: the VM runs
//! only on the authoring (cold) path, **never** in the per-frame loop —
//! continuous drivers lower to the numeric `gradiance-kernel` crate instead
//! (the `signal` layer depends on it; this crate never runs per-frame).
//!
//! Contents (Tier B — the hot-path numeric kernel — is its own crate,
//! `gradiance-kernel`, at the very bottom of the workspace):
//!
//! - [`reflect_bridge`] — the generic `bevy_reflect` <-> steel value bridge:
//!   read any `#[derive(Reflect)]` value by reflect-path, write scalars back,
//!   and convert a whole value to steel data (the read-total path). Pure
//!   w.r.t. the ECS.
//!
//! - [`registry`] — the **operation catalog**: pure metadata (name, signature,
//!   doc, governance category) for every scene verb the VM understands. The
//!   single source of truth the console, the in-VM `(ops)`/`(describe)`
//!   builtins, and later data-driven menus / user tools bind to, so what the
//!   editor advertises tracks what the VM implements. No ECS, no `steel`.
//!
//! - [`bridge`] — the **World-facing seam** (the only ECS-touching part).
//!   Embeds the steel engine, registers scene-verb builtins (edits *and*
//!   read-total geometric queries), and runs the exclusive `run_scripts` system
//!   that dispatches script-emitted operation records through the intent bus
//!   (the World-integration constraint — scripts emit intent *data*, never
//!   `&mut World`; reads come from a per-run [`bridge::SceneView`] snapshot).
//!
//! `steel` may be imported only in [`bridge`]/[`reflect_bridge`] (enforced by
//! `tests/boundaries.rs`). Later phases (see the decision record): the driver
//! component + named-signal dataflow seam over the kernel, and menus / tools
//! authored as `.scm` over the same registry.

pub mod bridge;
pub mod reflect_bridge;
pub mod registry;

pub use bridge::ScriptPlugin;

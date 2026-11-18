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
//! - [`registry`] — the **operation catalog** the console, the in-VM
//!   `(ops)`/`(describe)` built-ins and `gradiance.ops` show: a projection of
//!   the operation registry (`gradiance-op`, where every verb is declared)
//!   plus the few VM built-ins, each listed with the reason it is not an
//!   operation. No ECS, no `steel`.
//!
//! - [`ops`] — this layer's own operations: the editor verbs
//!   (`register-action`, `register-tool`, `label`) and the script side of the
//!   signal bus (`signal-get`, `signal-set`, `defparam`, `defsignal`).
//!
//! - [`bridge`] — the **World-facing seam** (the only ECS-touching part).
//!   Embeds the steel engine, registers scene-verb builtins (edits *and*
//!   read-total geometric queries), and runs the exclusive `run_scripts` system
//!   that dispatches script-emitted operation records through the intent bus
//!   (the World-integration constraint — scripts emit intent *data*, never
//!   `&mut World`; reads come from a per-run
//!   [`SceneSnapshot`](gradiance_command::snapshot::SceneSnapshot)).
//!
//! - [`remote`] — the **external control surface**: Bevy's remote protocol
//!   (JSON-RPC over HTTP, localhost, off unless `--remote`) bound to the
//!   operation registry: a call is one registry invocation (a VM built-in is
//!   refused: it exists only inside a script run). Bevy's built-in world-mutation methods are removed.
//!
//! - [`client`] — the **client doorways**: `gradiance ops` / `gradiance op`
//!   on the command line and `gradiance mcp` (the Model Context Protocol on
//!   stdio). Thin clients of a running app's [`remote`] surface; no ECS.
//!
//! - [`budget`] — the wall-clock budget one script run gets before it is
//!   stopped (a guard against runaway scripts, not a scheduler).
//!
//! `steel` may be imported only in this crate (enforced by
//! `tests/boundaries.rs`). Later phases (see the decision record): the driver
//! component + named-signal dataflow seam over the kernel, and menus / tools
//! authored as `.scm` over the same registry.

pub mod actions;
pub mod bridge;
pub mod budget;
pub mod client;
pub mod ops;
pub mod reflect_bridge;
pub mod registry;
pub mod remote;

pub use bridge::ScriptPlugin;

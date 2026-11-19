//! The operation registry: **one declaration per operation**, shared by every
//! doorway (the Lisp VM, the remote protocol, menus, and later the CLI, MCP
//! and the graph). Plan: `docs/unified-surface-plan.md` § 3 and § 13.
//!
//! An [`Operation`] is a typed parameter struct plus [`OpMeta`]. Its one
//! function, [`Operation::lower`], turns decoded params and a read-only
//! [`OpContext`] into an [`OpOutput`]: the writes the operation's seam already
//! takes (intent messages, a settings write) plus an optional value to return.
//! Nothing here mutates the world except [`apply`], and an [`OpOutput`] can
//! only hold the two write kinds its constructors build, so the registry adds
//! **no new mutation path** (invariants 1 and 2 hold).
//!
//! Layering: this crate sits just above `gradiance-core`. It names no scene
//! type. Each layer registers its own operations and publishes its own
//! read-only mirrors into the context ([`OpContextFillers`]); a doorway
//! dispatches by id and needs no compile-time edge to the owning layer.
//!
//! Every doorway produces a **named JSON object** and the registry decodes it
//! with serde: one decoder. Reflection describes the params (field names and
//! order, for the Lisp positional mapping and for schemas); it does not decode.

mod context;
mod last_op;
mod output;
pub mod reflect_path;
mod registry;
mod schema;
pub mod search;
mod target;

pub use context::{OpContext, OpContextFillers, OpIdSource};
pub use last_op::{LastOp, LastOpRecord, OpCall, OpJournal, invoke_as, journal, set_param};
pub use output::{OpOutput, OpTrace, TraceEntry, apply};
pub use registry::{
    InvokeOp, OpEntry, OpError, OpInvokeSet, OpMeta, OpPlugin, OpRegistry, Operation, Seam, invoke,
};
pub use target::{ResolveTarget, SelectionMirror, Target};

/// Re-exported so a doorway builds [`InvokeOp`] params without its own JSON
/// dependency.
pub use serde_json::{Value, json};

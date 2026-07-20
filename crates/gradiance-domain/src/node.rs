//! Behavior nodes: authored, placeable dataflow entities.
//!
//! A **behavior node** is an authored entity that lives in the scene like a
//! body or a joint, but represents a piece of the signal dataflow
//! (`docs/signal-dataflow.md`) rather than physical matter. Today the one
//! kind is the [`Tracer`](crate::tracer::Tracer) node — a placeable
//! trajectory probe — but the framework is deliberately general: every
//! future **sensor** (velocity/force reader) and **actuator** (a driver
//! that writes a signal) becomes another [`NodeKind`], its own tool, and
//! its own selectable glyph. This is the "eventually all sensors and
//! actuators are their own tools" direction made concrete.
//!
//! Nodes carry a [`StableId`] (identity, never a
//! raw `Entity`), a `Transform` (their placed pose), and an optional
//! [`NodeAttachment`] to a body they follow — so a node placed on a body
//! rides it, and one placed in empty space stays put. Everything else about
//! a node is its `NodeKind` payload.

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use serde::{Deserialize, Serialize};

/// Marker on every authored behavior-node entity (the node analogue of
/// [`Body`](crate::Body) / [`Joint`](crate::Joint)).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BehaviorNode;

/// A node's optional attachment to a body: it follows the body's pose,
/// offset by `offset` (in the body's local frame at attach time). `None`
/// target = a free node fixed at its own `Transform`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NodeAttachment {
    /// The body this node follows, or `None` for a free node.
    pub target: Option<StableId>,
    /// Offset from the target's origin, in the target's local frame.
    pub offset: Vec2,
}

impl Default for NodeAttachment {
    fn default() -> Self {
        Self {
            target: None,
            offset: Vec2::ZERO,
        }
    }
}

impl NodeAttachment {
    /// A free node (no target) at its own transform.
    pub fn free() -> Self {
        Self::default()
    }

    /// A node attached to `target` at `offset` in the target's local frame.
    pub fn to(target: StableId, offset: Vec2) -> Self {
        Self {
            target: Some(target),
            offset,
        }
    }
}

/// What a behavior node *is* — its authored payload. Sensors and actuators
/// are **not** placeable nodes: they are ports on objects (read via
/// [`SignalSource`](crate::signal::SignalSource), written via
/// [`SignalSink`](crate::signal::SignalSink)), wired in the node
/// editor. The one remaining placeable node kind is the **tracer** — a
/// trajectory probe you can drop anywhere or ride on a body.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum NodeKind {
    /// A trajectory-trail probe (the placeable [`Tracer`]).
    ///
    /// [`Tracer`]: crate::tracer::Tracer
    Tracer(crate::tracer::Tracer),
}

impl NodeKind {
    /// A short human label for glyphs / menus.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tracer(_) => "tracer",
        }
    }
}

//! Behavior nodes: authored, placeable dataflow entities.
//!
//! A **behavior node** is an authored entity that lives in the scene like a
//! body or a joint, but represents a piece of the signal dataflow
//! (`docs/signal-dataflow.md`) rather than physical matter. Today the one
//! kind is the [`Tracer`](crate::domain::tracer::Tracer) node — a placeable
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

use crate::core::ids::StableId;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker on every authored behavior-node entity (the node analogue of
/// [`Body`](crate::domain::Body) / [`Joint`](crate::domain::Joint)).
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

/// What a behavior node *is* — its authored payload. One variant per node
/// kind; adding a sensor/actuator is one variant here plus its tool.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum NodeKind {
    /// A trajectory-trail probe (the placeable [`Tracer`]).
    ///
    /// [`Tracer`]: crate::domain::tracer::Tracer
    Tracer(crate::domain::tracer::Tracer),
    /// A **sensor**: reads a scene quantity at its attached body and
    /// publishes it on the bus under `signal`. The placeable, node-shaped
    /// counterpart of a binding's *source* — the dataflow's input port.
    Sensor {
        /// Which scene quantity to read.
        quantity: SensorQuantity,
        /// Bus name to publish the reading under.
        signal: String,
    },
    /// An **actuator**: reads a bus `signal`, maps it through a domain +
    /// gradient, and tints its attached body's fill. The node-shaped
    /// counterpart of a binding's *sink* — the dataflow's output port.
    Actuator {
        /// Bus name to read.
        signal: String,
        /// Input-domain mapping to `[0, 1]`.
        map: crate::domain::signal::SignalMap,
        /// Color ramp.
        gradient: crate::domain::signal::GradientSpec,
    },
}

/// A scene quantity a [sensor node](NodeKind::Sensor) reads from its body —
/// the same readings a binding [`SignalSource`](crate::domain::signal::SignalSource)
/// offers, in placeable form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum SensorQuantity {
    /// Linear speed (px/s).
    Speed,
    /// Angular speed (rad/s, absolute).
    Spin,
    /// Height (world y, px).
    Height,
    /// Net normal contact force.
    ContactForce,
    /// Number of bodies touched.
    ContactCount,
}

impl SensorQuantity {
    /// Every quantity, for the tool/menu cycle.
    pub const ALL: [Self; 5] = [
        Self::Speed,
        Self::Spin,
        Self::Height,
        Self::ContactForce,
        Self::ContactCount,
    ];

    /// A short label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Speed => "speed",
            Self::Spin => "spin",
            Self::Height => "height",
            Self::ContactForce => "force",
            Self::ContactCount => "contacts",
        }
    }
}

impl NodeKind {
    /// A short human label for glyphs / menus.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tracer(_) => "tracer",
            Self::Sensor { .. } => "sensor",
            Self::Actuator { .. } => "actuator",
        }
    }
}

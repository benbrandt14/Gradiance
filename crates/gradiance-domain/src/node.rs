//! Behavior nodes: authored, placeable dataflow entities.
//!
//! A **behavior node** is an authored entity that lives in the scene like a
//! body or a joint, but represents a piece of the signal dataflow
//! (`docs/signal-dataflow.md`) rather than physical matter. Today there are
//! three kinds: the [`Tracer`](crate::tracer::Tracer) node — a placeable
//! trajectory probe — the [`Thruster`](crate::thruster::Thruster) node —
//! a push at a point of a body (brief N-3) — and the
//! [`Laser`](crate::laser::Laser) node — a reflecting beam that publishes a
//! hit signal (brief N-8). The framework is general: every
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
use gradiance_units::Displacement;
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
    pub offset: Displacement,
}

impl NodeKind {
    /// Whether this kind carries an axis in its body's frame (a thruster
    /// pushes along it, a laser aims along it; a tracer has none).
    fn has_axis(&self) -> bool {
        matches!(self, Self::Thruster(_) | Self::Laser(_))
    }

    /// This payload with its axis re-expressed from a body frame rotated
    /// `old_rot` into one rotated `new_rot`, so the **world** direction it
    /// pushes or aims never changes. `None` when nothing changes (a
    /// tracer has no axis, and equal frames need no turn). The one rule
    /// every re-parenting shares: a drag that re-attaches, a cut that
    /// moves a node onto a piece, a merge that consumes its body.
    #[must_use]
    pub fn reframe(&self, old_rot: f32, new_rot: f32) -> Option<Self> {
        if !self.has_axis() || (old_rot - new_rot).abs() < 1e-6 {
            return None;
        }
        let turn = old_rot - new_rot;
        match self {
            Self::Thruster(t) => Some(Self::Thruster(crate::thruster::Thruster {
                angle: gradiance_units::Angle(t.angle.value() + turn),
                ..*t
            })),
            Self::Laser(l) => Some(Self::Laser(crate::laser::Laser {
                angle: gradiance_units::Angle(l.angle.value() + turn),
                ..l.clone()
            })),
            Self::Tracer(_) | Self::Water(_) => None,
        }
    }
}

impl Default for NodeAttachment {
    fn default() -> Self {
        Self {
            target: None,
            offset: Displacement(Vec2::ZERO),
        }
    }
}

impl NodeAttachment {
    /// A free node (no target) at its own transform.
    pub fn free() -> Self {
        Self::default()
    }

    /// Where the node sits while its target is at `target`: the offset
    /// turned into the target's frame. The one formula the per-frame
    /// follower (`render::tracer::follow_node_attachments`) and the scene
    /// capture (`scene::NodeRecord::capture`) share, so a snapshot holds the
    /// position the follower gives (brief 6-1).
    pub fn follow(&self, target: gradiance_core::units::PosRot) -> Vec2 {
        target.pos + Vec2::from_angle(target.rot).rotate(self.offset.value())
    }

    /// A node attached to `target` at `offset` (metres) in the target's
    /// local frame.
    pub fn to(target: StableId, offset: Vec2) -> Self {
        Self {
            target: Some(target),
            offset: Displacement(offset),
        }
    }
}

/// What a behavior node *is* — its authored payload. Sensors and actuators
/// are **not** placeable nodes: they are ports on objects (read via
/// [`SignalSource`](crate::signal::SignalSource), written via
/// [`SignalSink`](crate::signal::SignalSink)), wired in the node
/// editor. The placeable node kinds are the **tracer** — a trajectory probe
/// you can drop anywhere or ride on a body — the **thruster**, which
/// pushes the body it rides, and the **laser**, whose beam reflects off
/// bodies and reports whether it reaches a target.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum NodeKind {
    /// A trajectory-trail probe (the placeable [`Tracer`]).
    ///
    /// [`Tracer`]: crate::tracer::Tracer
    Tracer(crate::tracer::Tracer),
    /// A push at a point of the body the node rides (the placeable
    /// [`Thruster`], brief N-3).
    ///
    /// [`Thruster`]: crate::thruster::Thruster
    Thruster(crate::thruster::Thruster),
    /// A beam cast along the node's axis that reflects off bodies (the
    /// placeable [`Laser`], brief N-8).
    ///
    /// [`Laser`]: crate::laser::Laser
    Laser(crate::laser::Laser),
    /// A region that fills with particle water when a run opens (the
    /// placeable [`WaterEmitter`], brief S-11).
    ///
    /// [`WaterEmitter`]: crate::water::WaterEmitter
    Water(crate::water::WaterEmitter),
}

impl NodeKind {
    /// A short human label for glyphs / menus.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tracer(_) => "tracer",
            Self::Thruster(_) => "thruster",
            Self::Laser(_) => "laser",
            Self::Water(_) => "water",
        }
    }

    /// The tracer payload, if this is a tracer node.
    pub const fn tracer(&self) -> Option<crate::tracer::Tracer> {
        match self {
            Self::Tracer(t) => Some(*t),
            Self::Thruster(_) | Self::Laser(_) | Self::Water(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload moved into a differently-rotated frame keeps its world
    /// direction; a tracer or an equal frame changes nothing.
    #[test]
    fn a_reframe_keeps_the_world_direction() {
        let thruster = NodeKind::Thruster(crate::thruster::Thruster::default());
        let turned = thruster.reframe(0.5, -0.25).unwrap();
        let NodeKind::Thruster(t) = &turned else {
            panic!("still a thruster");
        };
        assert_eq!(
            t.angle.value(),
            crate::thruster::Thruster::default().angle.value() + 0.75
        );

        // The same frame: nothing to do.
        assert!(thruster.reframe(0.5, 0.5).is_none());
        // A tracer has no axis to turn.
        let tracer = NodeKind::Tracer(crate::tracer::Tracer::default());
        assert!(tracer.reframe(0.0, 1.0).is_none());
    }
}

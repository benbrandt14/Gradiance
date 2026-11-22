//! Water emitters (brief S-11): the authored half of particle water.
//!
//! A [`WaterEmitter`] is the payload of a placeable behaviour node
//! ([`NodeKind::Water`](crate::node::NodeKind::Water)): a region and a
//! particle spacing. The particles are derived (`physics::particles`, the
//! S-10 decision): when a run opens the emitter fills its region with
//! particles on a square lattice, and undoing the run takes them away. Only
//! the emitter is saved and undone.

use bevy::math::Vec2;
use bevy::reflect::Reflect;
use gradiance_units::Length;
use serde::{Deserialize, Serialize};

/// The spacing a new emitter fills at, in metres (the S-10 constants).
pub const DEFAULT_SPACING: f32 = 0.04;

/// The smallest spacing an emitter takes: below it a small region holds
/// more particles than the step budget (`docs/particle-fluid-decision.md`).
pub const MIN_SPACING: f32 = 0.02;

/// The largest spacing an emitter takes.
pub const MAX_SPACING: f32 = 0.5;

/// A region that fills with particle water when a run opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct WaterEmitter {
    /// The region's outline, counter-clockwise, relative to the node's
    /// position, in world axes (the node's rotation does not turn it).
    pub outline: Vec<Vec2>,
    /// The particle spacing.
    pub spacing: Length,
}

impl WaterEmitter {
    /// An emitter of `outline` at the default spacing.
    pub fn new(outline: Vec<Vec2>) -> Self {
        Self {
            outline,
            spacing: Length(DEFAULT_SPACING),
        }
    }

    /// The spacing held to `[MIN_SPACING, MAX_SPACING]` (not finite: the
    /// default).
    pub fn spacing(&self) -> f32 {
        let s = self.spacing.0;
        if s.is_finite() {
            s.clamp(MIN_SPACING, MAX_SPACING)
        } else {
            DEFAULT_SPACING
        }
    }
}

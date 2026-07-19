//! Authored rod metadata.
//!
//! A rigid rod ("strut") is an ordinary authored body whose shape is a
//! [`ShapeDef::Capsule`](crate::domain::shape::ShapeDef::Capsule) leaf,
//! marked with [`Rod`]. Its end constraints are ordinary authored joints
//! ([`JointDef`](crate::domain::joint::JointDef)) referencing the rod's
//! `StableId`, so undo/duplicate/persistence/inspection all ride the
//! existing joint machinery. This component only records the end-kind
//! choice for inspection and future re-kinding.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// The constraint kind of one rod end.
///
/// `Prismatic` is planned (an end that slides along the rod axis); the
/// enum leaves room for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub enum RodEndKind {
    /// Weld: the end holds position *and* orientation.
    Fixed,
    /// Revolute: the end holds position, orientation stays free.
    Hinge,
}

/// Marker + creation metadata for a rigid rod body.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Rod {
    /// Constraint kind at the first-drawn end.
    pub end_a: RodEndKind,
    /// Constraint kind at the release end.
    pub end_b: RodEndKind,
}

//! Simulation planes — the authored side of the 2D↔3D seam.
//!
//! Gradiance authors in 2D and simulates in 3D. A body belongs to a
//! **simulation plane**, and its authored pose, shape, joint anchors and
//! velocities are all expressed in that plane's local 2D coordinates; the
//! physics layer lifts them through the plane's
//! [`PlaneFrame`] and projects results back.
//!
//! Keeping interaction implicitly planar is the point. A mechanism built by 2D
//! drags needs no degree-of-freedom decisions — the plane *is* the constraint —
//! while the engine underneath is a genuine 3D solver with real volumes and
//! real constraints available when they are wanted.
//!
//! # There is exactly one plane today
//!
//! [`SimPlanes`] holds a single frame, [`PlaneFrame::XY`], and every body's
//! [`SimPlaneId`] is [`SimPlaneId::DEFAULT`]. Both types exist now, authored and
//! serialized, so that adding a second plane later is a matter of storing
//! another *value*: no component signature changes, and no save-format break —
//! `SimPlaneId` already round-trips through the current format.
//!
//! Authoring *with* multiple planes — creating them, selecting them, drawing
//! them, and constraining bodies across them — is deliberately not built here.

use bevy::prelude::*;
use gradiance_core::units::PlaneFrame;
use serde::{Deserialize, Serialize};

/// Which simulation plane a body lives on.
///
/// Authored and saved, but always [`SimPlaneId::DEFAULT`] until multi-plane
/// authoring lands — which is exactly why it is here: the format already
/// carries it, so the feature costs no migration.
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
pub struct SimPlaneId(pub u8);

impl SimPlaneId {
    /// The plane every body is authored on today: world XY at the interaction
    /// plane.
    pub const DEFAULT: Self = Self(0);
}

/// Every simulation plane's frame, indexed by [`SimPlaneId`].
///
/// Scene content, not a workstation setting: a plane is part of the document.
/// Holds exactly one entry today.
#[derive(Resource, Debug, Clone, PartialEq, bevy::reflect::Reflect)]
pub struct SimPlanes(pub Vec<PlaneFrame>);

impl Default for SimPlanes {
    fn default() -> Self {
        Self(vec![PlaneFrame::XY])
    }
}

impl SimPlanes {
    /// The frame for `id`, falling back to the default plane.
    ///
    /// Never fails: an out-of-range id means a scene referenced a plane that
    /// no longer exists, and silently authoring onto the default plane is a far
    /// better outcome than dropping the body.
    #[must_use]
    pub fn frame(&self, id: SimPlaneId) -> PlaneFrame {
        self.0.get(id.0 as usize).copied().unwrap_or(PlaneFrame::XY)
    }

    /// The frame every body currently uses.
    #[must_use]
    pub fn default_frame(&self) -> PlaneFrame {
        self.frame(SimPlaneId::DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_plane_is_the_interaction_plane() {
        let planes = SimPlanes::default();
        assert_eq!(planes.0.len(), 1, "one plane, for now");
        assert_eq!(planes.default_frame(), PlaneFrame::XY);
        assert_eq!(planes.frame(SimPlaneId::DEFAULT), PlaneFrame::XY);
    }

    #[test]
    fn an_unknown_plane_falls_back_rather_than_dropping_the_body() {
        let planes = SimPlanes::default();
        assert_eq!(planes.frame(SimPlaneId(7)), PlaneFrame::XY);
    }

    #[test]
    fn a_body_defaults_onto_the_default_plane() {
        assert_eq!(SimPlaneId::default(), SimPlaneId::DEFAULT);
    }
}

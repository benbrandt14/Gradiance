//! Tracers: authored trajectory-trail markers (Algodoo's "show plot").
//!
//! A [`Tracer`] on a body asks the renderer to draw the body's recent
//! path as a fading polyline. Only the marker is authored (persisted,
//! undoable via `PropertyValue::Tracer`); the sampled trail itself is
//! *derived* state (`render::tracer::TraceTrail`) — rebuilt live, never
//! serialized, never in undo records (rule #5). The trail color comes
//! from the body's own [`Appearance`](crate::domain::appearance::Appearance),
//! so traced bodies stay visually identifiable.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// An authored trajectory-trail marker.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Tracer {
    /// How long a sample stays visible, in simulated seconds (the trail
    /// ages on the physics clock, so pausing freezes it).
    pub fade_secs: f32,
}

impl Default for Tracer {
    fn default() -> Self {
        Self { fade_secs: 3.0 }
    }
}

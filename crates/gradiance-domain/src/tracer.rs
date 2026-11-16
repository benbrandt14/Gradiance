//! Tracers: authored trajectory-trail markers (Algodoo's "show plot").
//!
//! A [`Tracer`] on a body asks the renderer to draw the body's recent
//! path as a fading polyline. Only the marker is authored (persisted,
//! undoable via `PropertyValue::Tracer`); the sampled trail itself is
//! *derived* state (`render::tracer::TraceTrail`) — rebuilt live, never
//! serialized, never in undo records (rule #5). The trail color comes
//! from the body's own [`Appearance`](crate::appearance::Appearance),
//! so traced bodies stay visually identifiable.

use bevy::prelude::Component;
use gradiance_units::{Length, Time};
use serde::{Deserialize, Serialize};

/// How a trail is drawn.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum TracePattern {
    /// A continuous fading polyline.
    #[default]
    Line,
    /// A dot at each sample (radius = `size`).
    Dots,
}

impl TracePattern {
    /// Both patterns, for the UI combo.
    pub const ALL: [Self; 2] = [Self::Line, Self::Dots];
}

/// An authored trajectory-trail marker.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Tracer {
    /// How long a sample stays visible, in simulated seconds (the trail
    /// ages on the physics clock, so pausing freezes it).
    pub fade_secs: Time,
    /// Trail thickness: dot radius (`Dots`) / point size (world metres).
    #[serde(default = "default_size")]
    pub size: Length,
    /// How the trail is rendered.
    #[serde(default)]
    pub pattern: TracePattern,
}

/// Default trail size (world metres) — for `serde` on pre-config saves.
fn default_size() -> Length {
    Length(0.02)
}

impl Default for Tracer {
    fn default() -> Self {
        Self {
            fade_secs: Time(3.0),
            size: default_size(),
            pattern: TracePattern::default(),
        }
    }
}

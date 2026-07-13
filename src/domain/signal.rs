//! Signal-dataflow bindings: the persisted half of the signal substrate
//! (`docs/signal-dataflow.md`).
//!
//! These are the *data* types — sources, mapping, gradients, sinks, and
//! the [`SignalBindings`] config-seam resource that persists with the
//! scene (like [`settings`](crate::domain::settings)). The runtime that
//! evaluates them (the bus, the derived color override, the per-frame
//! evaluator) lives in [`crate::signal`].

use crate::core::ids::StableId;
use bevy::prelude::*;
use colorgrad::Gradient;
use serde::{Deserialize, Serialize};

/// Gradient lookups are quantized to this many bands so a continuously
/// varying signal only re-tints (and re-builds materials) when it crosses
/// a band, not every frame.
const GRADIENT_BANDS: f32 = 48.0;

/// Where a signal's value comes from — a *read* of scene state. Bodies are
/// referenced by [`StableId`] (never `Entity`); an unresolvable source
/// simply produces no sample this frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SignalSource {
    /// Linear speed of a body (px/s).
    Speed(StableId),
    /// Angular speed of a body (rad/s, absolute).
    Spin(StableId),
    /// A body's height (world y, px).
    Height(StableId),
    /// Centre-to-centre distance between two bodies (px).
    Distance(StableId, StableId),
    /// Net normal contact force on a body (impulse / fixed dt).
    ContactForce(StableId),
    /// Number of bodies a body is currently touching.
    ContactCount(StableId),
    /// A named value published on the [`SignalBus`](crate::signal::SignalBus)
    /// — by a script
    /// (`signal-set`), a future node, or another binding.
    Named(String),
}

/// Input-domain mapping: `value` is normalized to `t ∈ [0, 1]` over
/// `[in_min, in_max]` (clamped). Pure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SignalMap {
    /// Value mapped to `t = 0`.
    pub in_min: f32,
    /// Value mapped to `t = 1`.
    pub in_max: f32,
}

impl Default for SignalMap {
    fn default() -> Self {
        Self {
            in_min: 0.0,
            in_max: 500.0,
        }
    }
}

impl SignalMap {
    /// Normalizes `value` into `[0, 1]` (degenerate domains map to 0).
    pub fn normalize(self, value: f32) -> f32 {
        let span = self.in_max - self.in_min;
        if span.abs() < f32::EPSILON {
            return 0.0;
        }
        ((value - self.in_min) / span).clamp(0.0, 1.0)
    }
}

/// A named color gradient (via the `colorgrad` crate — no reinvented
/// wheels). Serializable so bindings persist; the node editor later swaps
/// this for user-authored gradients through the same `at()` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum GradientSpec {
    /// Perceptually uniform blue→green→yellow (the default).
    #[default]
    Viridis,
    /// High-contrast blue→red rainbow.
    Turbo,
    /// Purple→orange→yellow.
    Plasma,
    /// Black→red→yellow (heat).
    Inferno,
    /// Blue→white→red centered diverging ramp.
    CoolWarm,
}

impl GradientSpec {
    /// Every preset, for the UI combo.
    pub const ALL: [Self; 5] = [
        Self::Viridis,
        Self::Turbo,
        Self::Plasma,
        Self::Inferno,
        Self::CoolWarm,
    ];

    /// The gradient color at `t ∈ [0, 1]`, quantized to `GRADIENT_BANDS`.
    pub fn at(self, t: f32) -> Color {
        let t = (t.clamp(0.0, 1.0) * GRADIENT_BANDS).round() / GRADIENT_BANDS;
        let c = match self {
            Self::Viridis => colorgrad::preset::viridis().at(t),
            Self::Turbo => colorgrad::preset::turbo().at(t),
            Self::Plasma => colorgrad::preset::plasma().at(t),
            Self::Inferno => colorgrad::preset::inferno().at(t),
            Self::CoolWarm => colorgrad::preset::rd_bu().at(1.0 - t),
        };
        Color::srgba(c.r, c.g, c.b, 1.0)
    }
}

/// What a signal drives. Color sinks write the derived
/// [`SignalColorOverride`](crate::signal::SignalColorOverride); `Plot`
/// only publishes (every binding's value
/// lands on the bus and in the plot panel regardless).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SignalSink {
    /// Tint a body's fill (render override; authored appearance untouched).
    Fill(StableId),
    /// Tint a body's tracer trail.
    TracerColor(StableId),
    /// Publish/plot only.
    Plot,
}

/// One source → map → gradient → sink wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SignalBinding {
    /// Bus name the value publishes under (also the plot label).
    pub name: String,
    /// The read.
    pub source: SignalSource,
    /// Input domain → `[0, 1]`.
    pub map: SignalMap,
    /// Color ramp for color sinks.
    pub gradient: GradientSpec,
    /// The derived write.
    pub sink: SignalSink,
}

/// The signal graph: every active binding. Config-seam resource (UI edits
/// directly, persisted in the scene's `EnvironmentRecord`, not undoable).
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct SignalBindings(pub Vec<SignalBinding>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_map_normalizes_and_clamps() {
        let map = SignalMap {
            in_min: 100.0,
            in_max: 300.0,
        };
        assert!(map.normalize(100.0).abs() < 1e-6);
        assert!((map.normalize(300.0) - 1.0).abs() < 1e-6);
        assert!((map.normalize(200.0) - 0.5).abs() < 1e-6);
        assert!(map.normalize(-50.0).abs() < 1e-6, "clamps below");
        assert!((map.normalize(900.0) - 1.0).abs() < 1e-6, "clamps above");
        let degenerate = SignalMap {
            in_min: 5.0,
            in_max: 5.0,
        };
        assert!(degenerate.normalize(9.0).abs() < 1e-6);
    }

    #[test]
    fn gradients_span_distinct_endpoint_colors() {
        for spec in GradientSpec::ALL {
            let lo = spec.at(0.0);
            let hi = spec.at(1.0);
            assert_ne!(lo, hi, "{spec:?} endpoints are distinct");
            // Quantization is stable: values in the same band agree.
            assert_eq!(spec.at(0.500), spec.at(0.505));
        }
    }
}

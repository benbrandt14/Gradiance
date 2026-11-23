//! Authored optics (brief S-3): a body's refractive index.
//!
//! A body with [`Optics`] lets a laser beam through, bent by Snell's law
//! (`physics::laser`, `geometry::beam::trace_optics`), and reflects it
//! inside on total internal reflection. A body without it is a mirror, as
//! every body was before S-3, so old scenes trace the same.
//!
//! Dispersion (brief S-15): the index depends on the wavelength by Cauchy's
//! law, `n(λ) = A + B / λ²`. The authored `index` is `n` at the sodium D
//! line ([`REFERENCE_WAVELENGTH`]) and `dispersion` is `B`, so a body with
//! no dispersion bends every colour alike, as before.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Glass: the index a new transparent body starts with.
pub const DEFAULT_INDEX: f32 = 1.5;

/// The smallest index a body takes (air is 1; below it a beam would bend
/// away from the normal on the way in, which no ordinary material does).
pub const MIN_INDEX: f32 = 1.0;

/// The largest index a body takes (diamond is about 2.42).
pub const MAX_INDEX: f32 = 4.0;

/// The wavelength the authored index holds at: the sodium D line (µm).
pub const REFERENCE_WAVELENGTH: f32 = 0.5893;

/// The largest Cauchy `B` a body takes (µm²); flint glass is about 0.01.
pub const MAX_DISPERSION: f32 = 0.05;

/// Crown glass's Cauchy `B` (µm²): the dispersion a prism starts with.
pub const GLASS_DISPERSION: f32 = 0.004;

/// The colours a white laser splits into (brief S-15): their wavelengths
/// in µm, red to violet. The colours are `theme::SPECTRUM`, in this order.
pub const SPECTRUM_WAVELENGTHS: [f32; 7] = [0.70, 0.62, 0.58, 0.53, 0.47, 0.44, 0.40];

/// A transparent body's refractive index.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Optics {
    /// The refractive index at [`REFERENCE_WAVELENGTH`] (1 = air, 1.33
    /// water, 1.5 glass).
    pub index: f32,
    /// Cauchy's `B` (µm²): how much more the index is for blue than for
    /// red. 0: no dispersion (a file from before S-15 reads 0).
    #[serde(default)]
    pub dispersion: f32,
}

impl Default for Optics {
    fn default() -> Self {
        Self {
            index: DEFAULT_INDEX,
            dispersion: GLASS_DISPERSION,
        }
    }
}

impl Optics {
    /// The optics with the index held to `[MIN_INDEX, MAX_INDEX]` (a
    /// non-finite index reads as glass).
    #[must_use]
    pub fn sanitized(self) -> Self {
        let index = if self.index.is_finite() {
            self.index.clamp(MIN_INDEX, MAX_INDEX)
        } else {
            DEFAULT_INDEX
        };
        let dispersion = if self.dispersion.is_finite() {
            self.dispersion.clamp(0.0, MAX_DISPERSION)
        } else {
            0.0
        };
        Self { index, dispersion }
    }

    /// A body of this index with crown glass's dispersion.
    #[must_use]
    pub fn of_index(index: f32) -> Self {
        Self {
            index,
            dispersion: GLASS_DISPERSION,
        }
    }

    /// The index at `wavelength` µm (Cauchy), at least [`MIN_INDEX`].
    #[must_use]
    pub fn index_at(self, wavelength: f32) -> f32 {
        let b = self.dispersion;
        let shift = b * (wavelength.powi(-2) - REFERENCE_WAVELENGTH.powi(-2));
        (self.index + shift).max(MIN_INDEX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use gradiance_geometry::beam::{RayHit, trace_optics};

    #[test]
    fn an_index_is_held_to_its_range() {
        assert_eq!(Optics::of_index(0.5).sanitized().index, MIN_INDEX);
        assert_eq!(Optics::of_index(9.0).sanitized().index, MAX_INDEX);
        assert_eq!(Optics::of_index(f32::NAN).sanitized().index, DEFAULT_INDEX);
        assert_eq!(Optics::of_index(1.33).sanitized().index, 1.33);
    }

    #[test]
    fn the_index_rises_toward_violet_by_cauchy() {
        let glass = Optics {
            index: 1.5,
            dispersion: 0.004,
        };
        assert!((glass.index_at(REFERENCE_WAVELENGTH) - 1.5).abs() < 1e-6);
        assert!((glass.index_at(0.4) - 1.513_48).abs() < 1e-4);
        assert!((glass.index_at(0.7) - 1.496_65).abs() < 1e-4);
        let plain = Optics {
            dispersion: 0.0,
            ..glass
        };
        assert_eq!(plain.index_at(0.4), 1.5, "no dispersion");
    }

    /// The edges of a 60° prism with its base on y = 0: the ray-cast the
    /// physics layer does, against three segments. Key 0 is the prism.
    fn prism(o: Vec2, d: Vec2, max: f32, _inside: Option<usize>) -> Option<RayHit<usize>> {
        let h = 3.0_f32.sqrt();
        let corners = [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, h)];
        let mut best: Option<RayHit<usize>> = None;
        for k in 0..3 {
            let (a, b) = (corners[k], corners[(k + 1) % 3]);
            let e = b - a;
            let den = d.perp_dot(e);
            if den.abs() < 1e-9 {
                continue;
            }
            let t = (a - o).perp_dot(e) / den;
            let u = (a - o).perp_dot(d) / den;
            if t > 1e-6
                && t <= max
                && (0.0..=1.0).contains(&u)
                && best.is_none_or(|h| t < h.distance)
            {
                best = Some(RayHit {
                    distance: t,
                    normal: e.perp(),
                    key: 0,
                });
            }
        }
        best
    }

    /// The S-15 pure test: red and violet leave a 60° prism of n = 1.5,
    /// B = 0.004 µm² at the angles the prism formula gives, ± 0.05°.
    #[test]
    fn red_and_violet_leave_a_prism_at_the_computed_angles() {
        let glass = Optics {
            index: 1.5,
            dispersion: 0.004,
        };
        let apex = 60.0_f32.to_radians();
        let theta1 = 50.0_f32.to_radians();
        // The left face's outward normal points at 150°; the ray meets it
        // 50° off the inward normal (−30°), so it travels at +20°.
        let heading = 20.0_f32.to_radians();
        let d = Vec2::from_angle(heading);
        let face_mid = Vec2::new(-0.5, 3.0_f32.sqrt() / 2.0);
        let origin = face_mid - d * 2.0;
        let mut exits = [0.0_f32; 2];
        for (slot, wavelength) in exits.iter_mut().zip([0.70, 0.40]) {
            let n = glass.index_at(wavelength);
            let mut path = Vec::new();
            let end = trace_optics(origin, d, 10.0, 4, &mut path, prism, |_| false, |_| Some(n));
            assert!(!end.absorbed);
            assert_eq!(path.len(), 4, "in, through and out: {path:?}");
            let out = (path[3] - path[2]).normalize();
            // The prism formula: δ = θ1 + θ4 − A.
            let theta2 = (theta1.sin() / n).asin();
            let theta4 = (n * (apex - theta2).sin()).asin();
            let deviation = theta1 + theta4 - apex;
            let expected = heading - deviation;
            let got = out.y.atan2(out.x);
            assert!(
                (got - expected).abs().to_degrees() < 0.05,
                "λ {wavelength}: {} vs {}",
                got.to_degrees(),
                expected.to_degrees()
            );
            *slot = got;
        }
        let fan = (exits[0] - exits[1]).to_degrees();
        assert!(fan > 0.5, "violet bends more than red, by {fan}°");
    }
}

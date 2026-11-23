//! A spur gear's outline (brief S-5).
//!
//! A gear is an ordinary [`ShapeDef::Polygon`](crate::shape::ShapeDef) leaf:
//! two gears mesh through contact, as in the reference sandbox, with no gear
//! constraint.
//!
//! The teeth are **generated**, as a hob cuts them: a straight-sided rack
//! (the standard basic rack, 20 degree pressure angle) rolls along the pitch
//! circle and removes everything it sweeps. Every gear cut by one rack is
//! conjugate to every other, so two gears of the same module (pitch diameter
//! over tooth count) turn at the ratio of their tooth counts when their
//! centres are the sum of their pitch radii apart, and a small gear's teeth
//! are undercut where a mate's tip would otherwise jam. A first version with
//! straight trapezoid teeth jammed after one tooth on a 12:24 pair (measured
//! in `tests/it/gears.rs`).
//!
//! Pure math: no systems, no records. The command layer wraps the outline in
//! a body record.

use bevy::math::Vec2;
use std::f32::consts::{PI, TAU};

/// The fewest teeth a gear has. Below six the undercut eats most of each
/// tooth.
pub const MIN_TEETH: u32 = 6;

/// The most teeth a gear has. It bounds the outline at
/// `SAMPLES_PER_TOOTH × MAX_TEETH` vertices, and so the convex decomposition
/// of the collider.
pub const MAX_TEETH: u32 = 48;

/// Outline vertices per tooth (one pitch: a tip land, two flanks and a root).
pub const SAMPLES_PER_TOOTH: usize = 20;

/// The module the gear tool and "Make gear" use when no tooth count is
/// given (metres): every gear they make has teeth of one size, so any two of
/// them mesh. A 0.6 m gear gets 12 teeth.
pub const DEFAULT_MODULE: f32 = 0.1;

/// The tooth tip height above the pitch circle, in modules (the standard
/// addendum).
pub const ADDENDUM: f32 = 1.0;

/// The root depth below the pitch circle, in modules (the standard
/// dedendum): a quarter module deeper than the addendum, so a tooth tip of
/// one gear never bottoms out in the root of its mate.
pub const DEDENDUM: f32 = 1.25;

/// The fraction of the pitch-circle arc a tooth takes; the rest is gap.
/// Below one half: the backlash a meshed pair needs on a contact solver.
pub const TOOTH_FRACTION: f32 = 0.44;

/// The tangent of the 20 degree pressure angle: how far a rack flank leans
/// per unit of depth.
const PRESSURE_TAN: f32 = 0.363_970_2;

/// Rack positions sampled per ray.
const ROLL_STEPS: usize = 96;

/// Radius steps from the root circle to the tip circle along one ray.
const RADIAL_STEPS: usize = 96;

/// The module of a gear: pitch diameter over tooth count. Two gears mesh
/// only if their modules are equal.
#[must_use]
pub fn module(pitch_radius: f32, teeth: u32) -> f32 {
    2.0 * pitch_radius / teeth.max(1) as f32
}

/// The pitch radius of the gear of `teeth` teeth that meshes with gears of
/// `module`.
#[must_use]
pub fn pitch_radius(module: f32, teeth: u32) -> f32 {
    module * teeth as f32 / 2.0
}

/// `teeth` clamped to [`MIN_TEETH`]`..=`[`MAX_TEETH`].
#[must_use]
pub fn clamp_teeth(teeth: u32) -> u32 {
    teeth.clamp(MIN_TEETH, MAX_TEETH)
}

/// The tooth count a gear of pitch radius `radius` gets at
/// [`DEFAULT_MODULE`], clamped (see [`clamp_teeth`]).
#[must_use]
pub fn default_teeth(radius: f32) -> u32 {
    let teeth = (2.0 * radius / DEFAULT_MODULE).round();
    // A NaN or negative radius clamps to the fewest teeth.
    clamp_teeth(if teeth.is_finite() && teeth > 0.0 {
        teeth as u32
    } else {
        0
    })
}

/// The basic rack of module `m`, in rack coordinates: `u` along the pitch
/// line, `v` away from the gear centre (`v = 0` on the pitch line). Whether
/// the point is rack material: a rack tooth cuts a gear gap, so it is a
/// trapezoid `1 - TOOTH_FRACTION` of a pitch wide on the pitch line, its tip
/// a dedendum below it, and the rack is solid an addendum above it.
fn in_rack(u: f32, v: f32, m: f32) -> bool {
    if v >= ADDENDUM * m {
        return true;
    }
    if v < -DEDENDUM * m {
        return false;
    }
    let pitch = PI * m;
    let u = u - pitch * (u / pitch).round();
    let half = (1.0 - TOOTH_FRACTION) * pitch / 2.0 + v * PRESSURE_TAN;
    u.abs() <= half
}

/// The gear's radius along the ray at `theta` (gear frame, a gap centred at
/// `theta = π/2`): the nearest point the rolling rack reaches, or the tip
/// circle. Conservative where an undercut is not visible from the centre.
fn profile_radius(theta: f32, radius: f32, m: f32) -> f32 {
    let tip = radius + ADDENDUM * m;
    let root = radius - DEDENDUM * m;
    // The rack reaches a point only within this angle of the tangent point.
    let reach = (root / tip).clamp(-1.0, 1.0).acos();
    let mut best = tip;
    for k in 0..=ROLL_STEPS {
        let t = k as f32 / ROLL_STEPS as f32;
        // The roll that brings the ray within `reach` of the tangent point:
        // the gear turns by `phi`, the rack moves `radius × phi` with it.
        let phi = PI / 2.0 - theta + (2.0 * t - 1.0) * reach;
        let dir = Vec2::from_angle(theta + phi);
        for j in 0..=RADIAL_STEPS {
            let rho = root + (best - root) * j as f32 / RADIAL_STEPS as f32;
            let p = dir * rho;
            if in_rack(p.x + radius * phi, p.y - radius, m) {
                best = rho;
                break;
            }
        }
    }
    best
}

/// The outline of a spur gear with pitch radius `radius` and `teeth` teeth
/// (clamped, see [`clamp_teeth`]): counter-clockwise, centred on the origin,
/// [`SAMPLES_PER_TOOTH`] vertices a tooth, tooth `0` centred on the +x axis.
/// The tip circle is `radius + ADDENDUM × module`, the root circle
/// `radius - DEDENDUM × module`. One tooth pitch is generated and repeated.
#[must_use]
pub fn gear_outline(radius: f32, teeth: u32) -> Vec<Vec2> {
    let teeth = clamp_teeth(teeth);
    let m = module(radius, teeth);
    let pitch_angle = TAU / teeth as f32;
    // One pitch, centred on a tooth: the profile is generated with a gap
    // centred at π/2, so a tooth centre is half a pitch further on.
    let tooth: Vec<(f32, f32)> = (0..SAMPLES_PER_TOOTH)
        .map(|s| {
            let local = (s as f32 / SAMPLES_PER_TOOTH as f32 - 0.5) * pitch_angle;
            let rho = profile_radius(PI / 2.0 + pitch_angle / 2.0 + local, radius, m);
            (local, rho)
        })
        .collect();
    (0..teeth)
        .flat_map(|i| {
            let centre = i as f32 * pitch_angle;
            tooth
                .iter()
                .map(move |&(local, rho)| Vec2::from_angle(centre + local) * rho)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contours::ring_signed_area;

    /// The flanks: places where the outline crosses the pitch circle.
    fn flanks(outline: &[Vec2], radius: f32) -> usize {
        (0..outline.len())
            .filter(|&i| {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                (a.length() - radius).signum() != (b.length() - radius).signum()
            })
            .count()
    }

    #[test]
    fn an_outline_has_two_flanks_per_tooth_around_the_pitch_circle() {
        for (radius, teeth) in [(0.6, 12), (1.2, 24), (0.3, 6), (2.4, 48)] {
            let outline = gear_outline(radius, teeth);
            assert_eq!(outline.len(), SAMPLES_PER_TOOTH * teeth as usize);
            assert_eq!(
                flanks(&outline, radius),
                2 * teeth as usize,
                "{teeth} teeth"
            );
            // The pitch circle sits an addendum under the tips and a
            // dedendum over the roots.
            let (lo, hi) = outline.iter().fold((f32::MAX, 0.0_f32), |(lo, hi), p| {
                (lo.min(p.length()), hi.max(p.length()))
            });
            let m = module(radius, teeth);
            let pitch = (hi - ADDENDUM * m).midpoint(lo + DEDENDUM * m);
            assert!(
                (pitch - radius).abs() <= 0.01 * radius,
                "{teeth} teeth: pitch {pitch} vs {radius}"
            );
        }
    }

    #[test]
    fn an_outline_is_counter_clockwise_and_centred() {
        let outline = gear_outline(0.6, 12);
        assert!(ring_signed_area(&outline) > 0.0);
        let mean = outline.iter().copied().sum::<Vec2>() / outline.len() as f32;
        assert!(mean.length() < 1e-4, "centred: {mean}");
        // Tooth 0 is centred on +x, its middle vertex on the tip circle.
        let m = module(0.6, 12);
        let mid = outline[SAMPLES_PER_TOOTH / 2];
        assert!(
            mid.y.abs() < 1e-5 && (mid.x - (0.6 + ADDENDUM * m)).abs() < 1e-4,
            "{mid}"
        );
    }

    #[test]
    fn a_tooth_on_the_pitch_circle_is_its_fraction_of_the_pitch() {
        let (radius, teeth) = (1.2, 24);
        let outline = gear_outline(radius, teeth);
        let outside = outline[..SAMPLES_PER_TOOTH]
            .iter()
            .filter(|p| p.length() > radius)
            .count();
        let fraction = outside as f32 / SAMPLES_PER_TOOTH as f32;
        assert!(
            (fraction - TOOTH_FRACTION).abs() <= 1.5 / SAMPLES_PER_TOOTH as f32,
            "{fraction}"
        );
    }

    #[test]
    fn the_count_is_clamped_and_the_default_keeps_one_tooth_size() {
        let fewest = SAMPLES_PER_TOOTH * MIN_TEETH as usize;
        let most = SAMPLES_PER_TOOTH * MAX_TEETH as usize;
        assert_eq!(gear_outline(1.0, 2).len(), fewest);
        assert_eq!(gear_outline(1.0, 500).len(), most);
        assert_eq!(default_teeth(0.6), 12);
        assert_eq!(default_teeth(1.2), 24);
        assert_eq!(default_teeth(0.01), MIN_TEETH);
        assert_eq!(default_teeth(f32::NAN), MIN_TEETH);
        assert_eq!(default_teeth(100.0), MAX_TEETH);
    }

    #[test]
    fn gears_of_one_module_have_pitch_radii_in_the_tooth_ratio() {
        let m = module(0.3, 12);
        assert!((pitch_radius(m, 24) - 0.6).abs() < 1e-6);
    }
}

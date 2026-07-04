//! Pure snap math: grid systems, feature snapping, and quantization.
//!
//! All functions take plain values so they are trivially unit-testable;
//! the interaction layer adapts settings resources into these calls.

use bevy::math::Vec2;

/// Snaps `p` to a square grid with the given origin, rotation, and spacing.
pub fn cartesian_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32) -> Vec2 {
    snap_in_basis(p, origin, rotation, spacing, Vec2::X, Vec2::Y)
}

/// Snaps `p` to an isometric grid (line families at ±30° from the grid's
/// x-axis). Intersections form a rhombic lattice.
pub fn isometric_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32) -> Vec2 {
    let a = Vec2::from_angle(30_f32.to_radians());
    let b = Vec2::from_angle(150_f32.to_radians());
    snap_in_basis(p, origin, rotation, spacing, a, b)
}

/// Snaps `p` to a polar grid: rings every `spacing` and
/// `angular_divisions` spokes around `origin`.
pub fn polar_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32, divisions: u32) -> Vec2 {
    let rel = p - origin;
    let radius = rel.length();
    let snapped_radius = (radius / spacing).round() * spacing;
    if snapped_radius < spacing / 2.0 {
        return origin;
    }
    let step = std::f32::consts::TAU / divisions.max(1) as f32;
    let angle = rel.y.atan2(rel.x) - rotation;
    let snapped_angle = (angle / step).round() * step + rotation;
    origin + Vec2::from_angle(snapped_angle) * snapped_radius
}

/// Snaps in an arbitrary (possibly non-orthogonal) 2D lattice basis.
fn snap_in_basis(
    p: Vec2,
    origin: Vec2,
    rotation: f32,
    spacing: f32,
    basis_a: Vec2,
    basis_b: Vec2,
) -> Vec2 {
    let rot = Vec2::from_angle(rotation);
    let a = rot.rotate(basis_a) * spacing;
    let b = rot.rotate(basis_b) * spacing;
    let rel = p - origin;
    // Solve rel = u·a + v·b, round (u, v) to the lattice.
    let det = a.perp_dot(b);
    if det.abs() < f32::EPSILON {
        return p;
    }
    let u = rel.perp_dot(b) / det;
    let v = a.perp_dot(rel) / det;
    origin + a * u.round() + b * v.round()
}

/// Closest point to `p` on segment `ab`.
pub fn project_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < f32::EPSILON {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// Quantizes an angle (radians) to multiples of `step` (radians).
pub fn quantize_angle(angle: f32, step: f32) -> f32 {
    if step.abs() < f32::EPSILON {
        return angle;
    }
    (angle / step).round() * step
}

/// Constrains a translation delta to an axis; `dominant` keeps only the
/// larger component (CAD shift-drag behavior).
pub fn constrain_axis(delta: Vec2, horizontal: bool, vertical: bool, dominant: bool) -> Vec2 {
    if dominant {
        if delta.x.abs() >= delta.y.abs() {
            Vec2::new(delta.x, 0.0)
        } else {
            Vec2::new(0.0, delta.y)
        }
    } else if horizontal {
        Vec2::new(delta.x, 0.0)
    } else if vertical {
        Vec2::new(0.0, delta.y)
    } else {
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartesian_snap_rounds_to_cells_with_rotation_and_origin() {
        let p = Vec2::new(103.0, 96.0);
        assert_eq!(
            cartesian_snap(p, Vec2::ZERO, 0.0, 100.0),
            Vec2::new(100.0, 100.0)
        );

        // Origin offset shifts the lattice.
        let snapped = cartesian_snap(p, Vec2::new(50.0, 0.0), 0.0, 100.0);
        assert_eq!(snapped, Vec2::new(150.0, 100.0));

        // 45° rotated grid: the snapped point must lie on the rotated
        // lattice — its coordinates in the rotated basis are integers.
        let rot = 45_f32.to_radians();
        let snapped = cartesian_snap(Vec2::new(70.0, 2.0), Vec2::ZERO, rot, 100.0);
        let local = Vec2::from_angle(-rot).rotate(snapped) / 100.0;
        assert!((local.x - local.x.round()).abs() < 1e-4);
        assert!((local.y - local.y.round()).abs() < 1e-4);
    }

    #[test]
    fn isometric_snap_lands_on_rhombic_lattice() {
        let snapped = isometric_snap(Vec2::new(90.0, 45.0), Vec2::ZERO, 0.0, 100.0);
        // Must be expressible as u·(cos30,sin30)·100 + v·(cos150,sin150)·100
        // with integral u,v.
        let a = Vec2::from_angle(30_f32.to_radians()) * 100.0;
        let b = Vec2::from_angle(150_f32.to_radians()) * 100.0;
        let det = a.perp_dot(b);
        let u = snapped.perp_dot(b) / det;
        let v = a.perp_dot(snapped) / det;
        assert!((u - u.round()).abs() < 1e-4, "u = {u}");
        assert!((v - v.round()).abs() < 1e-4, "v = {v}");
    }

    #[test]
    fn polar_snap_hits_ring_and_spoke() {
        let snapped = polar_snap(Vec2::new(95.0, 8.0), Vec2::ZERO, 0.0, 100.0, 8);
        assert!((snapped.length() - 100.0).abs() < 1e-3);
        assert!((snapped.y).abs() < 1e-3, "snaps onto the 0° spoke");

        // Near the origin collapses to the origin.
        assert_eq!(
            polar_snap(Vec2::new(10.0, 10.0), Vec2::ZERO, 0.0, 100.0, 8),
            Vec2::ZERO
        );
    }

    #[test]
    fn segment_projection_clamps_to_endpoints() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert_eq!(
            project_on_segment(Vec2::new(5.0, 3.0), a, b),
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(project_on_segment(Vec2::new(-5.0, 3.0), a, b), a);
        assert_eq!(project_on_segment(Vec2::new(15.0, 3.0), a, b), b);
    }

    #[test]
    fn angle_quantization() {
        let step = 15_f32.to_radians();
        assert!((quantize_angle(17_f32.to_radians(), step) - step).abs() < 1e-5);
        assert!((quantize_angle(-8_f32.to_radians(), step) - -step).abs() < 1e-5);
        assert!((quantize_angle(1.0, 0.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn axis_constraints() {
        let d = Vec2::new(10.0, 4.0);
        assert_eq!(constrain_axis(d, false, false, false), d);
        assert_eq!(constrain_axis(d, true, false, false), Vec2::new(10.0, 0.0));
        assert_eq!(constrain_axis(d, false, true, false), Vec2::new(0.0, 4.0));
        assert_eq!(constrain_axis(d, false, false, true), Vec2::new(10.0, 0.0));
        assert_eq!(
            constrain_axis(Vec2::new(3.0, -9.0), false, false, true),
            Vec2::new(0.0, -9.0)
        );
    }
}

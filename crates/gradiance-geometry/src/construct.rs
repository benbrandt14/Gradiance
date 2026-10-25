//! Alternate shape constructors: shapes defined by clicked points rather
//! than a two-corner drag.
//!
//! Pure math for the creation tools. A constructor turns the points a tool
//! collected into the pose and dimensions of an analytic leaf; the tool
//! wraps the result in a `ShapeDef` and a body record.

use bevy::math::Vec2;

/// Below this, an edge length or box height counts as degenerate.
const DEGENERATE: f32 = 1e-6;

/// A rectangle in a rotated frame: what a `ShapeDef::Box` plus a body pose
/// describe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotatedBox {
    /// Centre, world space.
    pub center: Vec2,
    /// Full extent along the box's local X (the first edge's direction).
    pub width: f32,
    /// Full extent along the box's local Y.
    pub height: f32,
    /// Rotation of local X from world X, radians, in `(-π, π]`.
    pub rotation: f32,
}

impl RotatedBox {
    /// The four corners, counter-clockwise from local `(-w/2, -h/2)`.
    pub fn corners(&self) -> [Vec2; 4] {
        let (hw, hh) = (self.width / 2.0, self.height / 2.0);
        let rot = Vec2::from_angle(self.rotation);
        [
            Vec2::new(-hw, -hh),
            Vec2::new(hw, -hh),
            Vec2::new(hw, hh),
            Vec2::new(-hw, hh),
        ]
        .map(|local| self.center + rot.rotate(local))
    }
}

/// The box with one edge `a → b` whose opposite edge passes through `c`.
///
/// Width is `|b − a|` and the rotation is the angle of `b − a`; the height is
/// the distance of `c` from the line through `a` and `b`, and the box extends
/// to `c`'s side of that line (where along the line `c` lies does not
/// matter). `None` when the edge or the height is degenerate.
pub fn three_point_box(a: Vec2, b: Vec2, c: Vec2) -> Option<RotatedBox> {
    let edge = b - a;
    let width = edge.length();
    if width < DEGENERATE {
        return None;
    }
    let dir = edge / width;
    // Signed distance of c from the line, positive on the left of a → b.
    let offset = dir.perp_dot(c - a);
    if offset.abs() < DEGENERATE {
        return None;
    }
    Some(RotatedBox {
        center: (a + b) / 2.0 + dir.perp() * (offset / 2.0),
        width,
        height: offset.abs(),
        rotation: dir.to_angle(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-5
    }

    #[test]
    fn axis_aligned_box_above_its_edge() {
        let b = three_point_box(Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(1.0, 2.0))
            .expect("non-degenerate");
        assert!(close(b.center, Vec2::new(2.0, 1.0)));
        assert!((b.width - 4.0).abs() < 1e-6);
        assert!((b.height - 2.0).abs() < 1e-6);
        assert!(b.rotation.abs() < 1e-6);
    }

    #[test]
    fn c_on_the_other_side_flips_the_box_not_the_rotation() {
        let b = three_point_box(Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(7.0, -2.0))
            .expect("non-degenerate");
        assert!(close(b.center, Vec2::new(2.0, -1.0)));
        assert!((b.height - 2.0).abs() < 1e-6, "height is unsigned");
        assert!(b.rotation.abs() < 1e-6, "rotation follows a → b only");
    }

    #[test]
    fn rotated_edge_sets_rotation_and_width() {
        let a = Vec2::new(1.0, 1.0);
        let b = a + Vec2::new(3.0, 3.0);
        // c one unit to the left of the edge (direction (-1, 1)/√2).
        let c = b + Vec2::new(-1.0, 1.0).normalize();
        let bx = three_point_box(a, b, c).expect("non-degenerate");
        assert!((bx.rotation - FRAC_PI_4).abs() < 1e-5);
        assert!((bx.width - 18f32.sqrt()).abs() < 1e-5);
        assert!((bx.height - 1.0).abs() < 1e-5);
        // Two corners are the clicked edge; the other two lie on c's line.
        let corners = bx.corners();
        assert!(close(corners[0], a));
        assert!(close(corners[1], b));
        assert!(close(corners[2], c));
    }

    #[test]
    fn right_of_a_vertical_edge() {
        // Edge pointing down: its left is +x, so c at -x is on the right.
        let bx = three_point_box(Vec2::ZERO, Vec2::new(0.0, -2.0), Vec2::new(-3.0, -1.0))
            .expect("non-degenerate");
        assert!((bx.rotation + FRAC_PI_2).abs() < 1e-5);
        assert!(close(bx.center, Vec2::new(-1.5, -1.0)));
        assert!((bx.width - 2.0).abs() < 1e-6 && (bx.height - 3.0).abs() < 1e-6);
    }

    #[test]
    fn reversed_edge_rotation_wraps_to_pi() {
        let bx = three_point_box(Vec2::new(2.0, 0.0), Vec2::ZERO, Vec2::new(1.0, 1.0))
            .expect("non-degenerate");
        assert!((bx.rotation.abs() - PI).abs() < 1e-5);
        assert!(close(bx.center, Vec2::new(1.0, 0.5)));
    }

    #[test]
    fn degenerate_inputs_are_none() {
        let p = Vec2::new(1.0, 2.0);
        assert!(
            three_point_box(p, p, Vec2::new(5.0, 5.0)).is_none(),
            "zero edge"
        );
        assert!(
            three_point_box(Vec2::ZERO, Vec2::X, Vec2::new(7.0, 0.0)).is_none(),
            "c on the edge's line"
        );
        assert!(three_point_box(p, p, p).is_none(), "all coincident");
    }
}

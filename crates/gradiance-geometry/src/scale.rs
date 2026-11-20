//! Pure scaling math: shapes under a linear map, exactly.
//!
//! Scaling happens in a *frame* (world axes or a body-local frame). For a
//! body rotated by `body_rot`, scaling by `factors` along the axes of a
//! frame rotated by `frame_rot` transforms local geometry by
//! `M = R(-body_rot) · R(frame_rot) · S · R(-frame_rot) · R(body_rot)`.
//!
//! Boxes and circles stay parametric when `M` keeps them representable
//! (axis-aligned diagonal / uniform); otherwise they **polygonize exactly**
//! (the same convention as CSG) instead of being silently approximated.

use crate::polygonize::polygonize;
use crate::shape::ShapeDef;
use bevy::math::{Mat2, Vec2};

/// Numeric tolerance for "no shear / uniform" checks.
const EPS: f32 = 1e-4;

/// The body-local linear map for scaling by `factors` along `frame_rot`
/// axes, applied to a body rotated by `body_rot`.
pub fn body_scale_matrix(body_rot: f32, frame_rot: f32, factors: Vec2) -> Mat2 {
    let rb = Mat2::from_angle(body_rot);
    let rf = Mat2::from_angle(frame_rot);
    rb.inverse() * rf * Mat2::from_diagonal(factors) * rf.inverse() * rb
}

/// A reflection of a body's local frame: which local axis changes sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    /// `x ↦ −x`: a left-right (horizontal) flip.
    X,
    /// `y ↦ −y`: an up-down (vertical) flip.
    Y,
}

impl MirrorAxis {
    /// The reflected point.
    #[must_use]
    pub fn point(self, p: Vec2) -> Vec2 {
        match self {
            Self::X => Vec2::new(-p.x, p.y),
            Self::Y => Vec2::new(p.x, -p.y),
        }
    }

    /// The reflected direction angle, wrapped to `(−π, π]`: `π − a` across
    /// the y axis, `−a` across the x axis.
    #[must_use]
    pub fn angle(self, a: f32) -> f32 {
        match self {
            Self::X => crate::wrap_angle(std::f32::consts::PI - a),
            Self::Y => -a,
        }
    }
}

/// `shape` reflected across a local axis, **structurally**: every leaf, CSG
/// node and placement keeps its kind, so a CSG or fillet body stays a tree
/// and mirroring twice gives the same tree back bit for bit (a polygon's
/// vertices are negated and its rings reversed, both exact).
///
/// A half-plane is symmetric across the y axis; across the x axis its solid
/// side turns over, so it comes back placed at a half turn.
#[must_use]
pub fn mirror_shape(shape: &ShapeDef, axis: MirrorAxis) -> ShapeDef {
    let ring = |r: &[Vec2]| -> Vec<Vec2> { r.iter().rev().map(|v| axis.point(*v)).collect() };
    match shape {
        ShapeDef::Box { .. } | ShapeDef::Circle { .. } | ShapeDef::Capsule { .. } => shape.clone(),
        ShapeDef::HalfPlane => match axis {
            MirrorAxis::X => ShapeDef::HalfPlane,
            MirrorAxis::Y => ShapeDef::Placed {
                pos: Vec2::ZERO,
                rot: std::f32::consts::PI,
                shape: Box::new(ShapeDef::HalfPlane),
            },
        },
        ShapeDef::Polygon { outline, holes } => ShapeDef::Polygon {
            outline: ring(outline),
            holes: holes.iter().map(|h| ring(h)).collect(),
        },
        ShapeDef::Csg { op, lhs, rhs } => ShapeDef::Csg {
            op: *op,
            lhs: Box::new(mirror_shape(lhs, axis)),
            rhs: Box::new(mirror_shape(rhs, axis)),
        },
        // A reflection conjugates a rotation into its inverse:
        // `Σ · R(rot) = R(−rot) · Σ`.
        ShapeDef::Placed { pos, rot, shape } => match (axis, &**shape) {
            (MirrorAxis::Y, ShapeDef::HalfPlane) => ShapeDef::Placed {
                pos: axis.point(*pos),
                rot: crate::wrap_angle(std::f32::consts::PI - rot),
                shape: Box::new(ShapeDef::HalfPlane),
            },
            _ => ShapeDef::Placed {
                pos: axis.point(*pos),
                rot: -rot,
                shape: Box::new(mirror_shape(shape, axis)),
            },
        },
    }
}

/// How one body's local frame changes under a frame scale
/// ([`scale_point`] moves its centre): its new rotation, and the map from an
/// old body-local point to the new one. Anchors, node offsets and the shape
/// all go through the same map, so a joint stays on the material it was
/// drawn on.
///
/// A scale with **one negative factor** is a mirror. Mirroring a rotated
/// body keeps it a rotated copy of the same shape: with `φ` the frame's
/// rotation, `F` the factors and `Σ` their signs,
/// `R(φ)·F·R(−φ)·R(θ) = R(θ')·M·Σ` where `θ' = 2φ − θ` and `M` is the
/// positive scale `|F|` seen from the new rotation. So the body turns to
/// `θ'`, its shape is reflected across one **local** axis (structurally,
/// [`mirror_shape`]) and then scaled by `M`, which is the identity for a
/// pure mirror: a box stays a box at any rotation, where the plain
/// [`body_scale_matrix`] would shear it into a polygon. Two negative factors
/// are a half turn and keep the plain map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyScale {
    /// The body's rotation before.
    old_rot: f32,
    /// The body's rotation after.
    pub rot: f32,
    /// The local reflection, applied first; `None` keeps the handedness.
    pub flip: Option<MirrorAxis>,
    /// The local linear map, applied after the flip.
    pub linear: Mat2,
    /// Whether `linear` is exactly the identity (a pure mirror).
    identity: bool,
}

impl BodyScale {
    /// The map of a body rotated by `body_rot` under a scale by `factors`
    /// along the axes of a frame rotated by `frame_rot`. `half_plane`: the
    /// body's whole shape is a bare half-plane, which cannot turn over
    /// across its own x axis, so a mirror across x reflects it across y and
    /// turns it half a turn more (`Σ_y = R(π)·Σ_x`).
    #[must_use]
    pub fn new(body_rot: f32, frame_rot: f32, factors: Vec2, half_plane: bool) -> Self {
        let mirror = match (factors.x < 0.0, factors.y < 0.0) {
            (true, false) => Some(MirrorAxis::X),
            (false, true) => Some(MirrorAxis::Y),
            _ => None,
        };
        let Some(axis) = mirror else {
            return Self {
                old_rot: body_rot,
                rot: body_rot,
                flip: None,
                linear: body_scale_matrix(body_rot, frame_rot, factors),
                identity: false,
            };
        };
        let (axis, rot) = if half_plane && axis == MirrorAxis::Y {
            (
                MirrorAxis::X,
                2.0 * frame_rot - body_rot + std::f32::consts::PI,
            )
        } else {
            (axis, 2.0 * frame_rot - body_rot)
        };
        let magnitude = factors.abs();
        let identity = magnitude == Vec2::ONE;
        Self {
            old_rot: body_rot,
            rot,
            flip: Some(axis),
            linear: if identity {
                Mat2::IDENTITY
            } else {
                body_scale_matrix(rot, frame_rot, magnitude)
            },
            identity,
        }
    }

    /// Whether the map reverses handedness: relative angles change sign.
    #[must_use]
    pub fn mirrors(&self) -> bool {
        self.flip.is_some()
    }

    /// An old body-local point in the new body frame.
    #[must_use]
    pub fn point(&self, p: Vec2) -> Vec2 {
        let p = self.flip.map_or(p, |axis| axis.point(p));
        if self.identity { p } else { self.linear * p }
    }

    /// Whether directions keep their angle under `linear` (the identity or
    /// a positive uniform scale).
    fn keeps_directions(&self) -> bool {
        self.identity || (is_uniform(self.linear) && self.linear.x_axis.x > 0.0)
    }

    /// A body-local direction angle in the new body frame. A pure mirror
    /// and a positive uniform scale map it exactly.
    #[must_use]
    pub fn direction(&self, angle: f32) -> f32 {
        let angle = self.flip.map_or(angle, |axis| axis.angle(angle));
        if self.keeps_directions() {
            return angle;
        }
        (self.linear * Vec2::from_angle(angle)).to_angle()
    }

    /// A body-local unit vector in the new body frame, unit again. A pure
    /// mirror and a positive uniform scale map it exactly.
    #[must_use]
    pub fn unit(&self, v: Vec2) -> Vec2 {
        let v = self.flip.map_or(v, |axis| axis.point(v));
        if self.keeps_directions() {
            return v;
        }
        (self.linear * v).normalize_or(v)
    }

    /// A world rotation tied to this body (a joint's rest rotation) after
    /// the map: a mirror reflects it about the line the body was reflected
    /// about (`a ↦ θ' + θ − a`); any other scale keeps it.
    #[must_use]
    pub fn rotation(&self, a: f32) -> f32 {
        if self.mirrors() {
            self.rot + self.old_rot - a
        } else {
            a
        }
    }

    /// The shape in the new body frame.
    #[must_use]
    pub fn shape(&self, shape: &ShapeDef) -> ShapeDef {
        match self.flip {
            Some(axis) => {
                let mirrored = mirror_shape(shape, axis);
                if self.identity {
                    mirrored
                } else {
                    scale_shape(&mirrored, self.linear)
                }
            }
            None => scale_shape(shape, self.linear),
        }
    }
}

/// Maps a world-space point through the frame scale about `pivot`.
pub fn scale_point(p: Vec2, pivot: Vec2, frame_rot: f32, factors: Vec2) -> Vec2 {
    let rf = Mat2::from_angle(frame_rot);
    pivot + rf * (factors * (rf.inverse() * (p - pivot)))
}

fn is_diagonal(m: Mat2) -> bool {
    m.x_axis.y.abs() < EPS && m.y_axis.x.abs() < EPS
}

fn is_uniform(m: Mat2) -> bool {
    is_diagonal(m) && (m.x_axis.x.abs() - m.y_axis.y.abs()).abs() < EPS
}

fn map_ring(ring: &[Vec2], m: Mat2) -> Vec<Vec2> {
    ring.iter().map(|v| m * *v).collect()
}

/// Applies a body-local linear map to a shape, exactly.
///
/// Falls back from parametric shapes to polygons when the map is not
/// representable (shear on a box, non-uniform on a circle). If the map
/// flips orientation, ring windings are reversed to stay CCW/CW-correct.
/// `HalfPlane` is scale-invariant.
pub fn scale_shape(shape: &ShapeDef, m: Mat2) -> ShapeDef {
    let flips = m.determinant() < 0.0;
    let mapped_polygon = |shape: &ShapeDef| {
        let contours = polygonize(shape);
        let mut outline = map_ring(&contours.outline, m);
        let mut holes: Vec<Vec<Vec2>> = contours.holes.iter().map(|h| map_ring(h, m)).collect();
        if flips {
            outline.reverse();
            for h in &mut holes {
                h.reverse();
            }
        }
        ShapeDef::Polygon { outline, holes }
    };

    match shape {
        ShapeDef::Box { width, height } if is_diagonal(m) => ShapeDef::Box {
            width: width * m.x_axis.x.abs(),
            height: height * m.y_axis.y.abs(),
        },
        ShapeDef::Circle { radius } if is_uniform(m) => ShapeDef::Circle {
            radius: radius * m.x_axis.x.abs(),
        },
        ShapeDef::Capsule {
            half_length,
            radius,
        } if is_uniform(m) => ShapeDef::Capsule {
            half_length: half_length * m.x_axis.x.abs(),
            radius: radius * m.x_axis.x.abs(),
        },
        ShapeDef::HalfPlane => ShapeDef::HalfPlane,
        other => mapped_polygon(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contours::Contours;

    #[test]
    fn aligned_box_scaling_stays_parametric() {
        let m = body_scale_matrix(0.0, 0.0, Vec2::new(2.0, 0.5));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        assert_eq!(
            scaled,
            ShapeDef::Box {
                width: 80.0,
                height: 10.0
            }
        );
    }

    #[test]
    fn quarter_turned_box_swaps_axes_and_stays_parametric() {
        let m = body_scale_matrix(std::f32::consts::FRAC_PI_2, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        // World-X scaling of a 90°-rotated box stretches its local Y.
        let ShapeDef::Box { width, height } = scaled else {
            panic!("expected parametric box, got {scaled:?}");
        };
        assert!((width - 40.0).abs() < 1e-3);
        assert!((height - 40.0).abs() < 1e-3);
    }

    #[test]
    fn off_axis_box_polygonizes_with_correct_area() {
        let rot = 30_f32.to_radians();
        let m = body_scale_matrix(rot, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        let ShapeDef::Polygon { outline, holes } = &scaled else {
            panic!("expected polygonized result, got {scaled:?}");
        };
        let area = Contours {
            outline: outline.clone(),
            holes: holes.clone(),
        }
        .area();
        assert!((area - 1600.0).abs() < 1.0, "area doubles (got {area})");
    }

    #[test]
    fn circles_stay_circles_only_under_uniform_scale() {
        let uniform = body_scale_matrix(0.3, 1.1, Vec2::splat(3.0));
        let ShapeDef::Circle { radius } = scale_shape(&ShapeDef::Circle { radius: 10.0 }, uniform)
        else {
            panic!("uniform scale must keep circles parametric");
        };
        assert!((radius - 30.0).abs() < 1e-3, "radius {radius}");

        let squash = body_scale_matrix(0.0, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(&ShapeDef::Circle { radius: 10.0 }, squash);
        let ShapeDef::Polygon { outline, .. } = &scaled else {
            panic!("expected ellipse polygon, got {scaled:?}");
        };
        let area = crate::contours::ring_signed_area(outline).abs();
        let ideal = std::f32::consts::PI * 10.0 * 10.0 * 2.0;
        assert!((area - ideal).abs() / ideal < 0.02, "{area} vs {ideal}");
    }

    #[test]
    fn scale_point_moves_positions_about_the_pivot() {
        let p = scale_point(Vec2::new(10.0, 0.0), Vec2::ZERO, 0.0, Vec2::new(2.0, 1.0));
        assert_eq!(p, Vec2::new(20.0, 0.0));
        // Pivot is a fixed point.
        let pivot = Vec2::new(5.0, 5.0);
        assert_eq!(scale_point(pivot, pivot, 0.7, Vec2::new(3.0, 0.2)), pivot);
    }

    #[test]
    fn mirroring_keeps_winding_valid() {
        let m = body_scale_matrix(0.0, 0.0, Vec2::new(-1.0, 1.0));
        let tri = ShapeDef::Polygon {
            outline: vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(0.0, 10.0),
            ],
            holes: vec![],
        };
        let ShapeDef::Polygon { outline, .. } = scale_shape(&tri, m) else {
            panic!()
        };
        assert!(
            crate::contours::ring_signed_area(&outline) > 0.0,
            "outline stays CCW after mirror"
        );
    }

    fn tree() -> ShapeDef {
        ShapeDef::Csg {
            op: crate::shape::CsgOp::Union,
            lhs: Box::new(ShapeDef::Polygon {
                outline: vec![
                    Vec2::new(-0.5, -0.25),
                    Vec2::new(0.75, -0.25),
                    Vec2::new(0.0, 0.5),
                ],
                holes: vec![],
            }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(0.3, 0.1),
                rot: 0.4,
                shape: Box::new(ShapeDef::Box {
                    width: 0.5,
                    height: 0.25,
                }),
            }),
        }
    }

    #[test]
    fn mirroring_a_tree_twice_gives_it_back_exactly() {
        for axis in [MirrorAxis::X, MirrorAxis::Y] {
            let once = mirror_shape(&tree(), axis);
            assert!(matches!(once, ShapeDef::Csg { .. }), "stays a tree");
            assert_eq!(mirror_shape(&once, axis), tree(), "{axis:?}");
        }
    }

    #[test]
    fn a_mirrored_shape_is_the_reflected_field() {
        for axis in [MirrorAxis::X, MirrorAxis::Y] {
            let mirrored = mirror_shape(&tree(), axis);
            for p in [
                Vec2::new(0.1, 0.0),
                Vec2::new(0.4, 0.15),
                Vec2::new(-0.3, -0.1),
                Vec2::new(0.6, 0.3),
            ] {
                let a = crate::sdf::eval(&tree(), p);
                let b = crate::sdf::eval(&mirrored, axis.point(p));
                assert!((a - b).abs() < 1e-5, "{axis:?} at {p}: {a} vs {b}");
            }
        }
        // A half-plane across x turns over; mirrored twice it is equivalent.
        let flipped = mirror_shape(&ShapeDef::HalfPlane, MirrorAxis::Y);
        let p = Vec2::new(0.2, 0.5);
        assert!(
            (crate::sdf::eval(&flipped, p) + crate::sdf::eval(&ShapeDef::HalfPlane, p)).abs()
                < 1e-5
        );
    }

    /// The world image of a body-local point is the same whether the world
    /// point is scaled about the pivot or the body is re-posed by the map.
    #[test]
    fn a_body_scale_agrees_with_the_world_scale() {
        let pivot = Vec2::new(0.3, -0.2);
        let local = Vec2::new(0.4, 0.15);
        for factors in [
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-2.0, 0.5),
            Vec2::new(1.5, 0.75),
            Vec2::new(-1.0, -1.0),
        ] {
            for (body_rot, frame_rot) in [(0.0, 0.0), (0.7, 0.0), (0.7, 0.7), (-1.2, 0.4)] {
                for half_plane in [false, true] {
                    let pos = Vec2::new(1.0, 0.5);
                    let map = BodyScale::new(body_rot, frame_rot, factors, half_plane);
                    let world = pos + Vec2::from_angle(body_rot).rotate(local);
                    let want = scale_point(world, pivot, frame_rot, factors);
                    let new_pos = scale_point(pos, pivot, frame_rot, factors);
                    let got = new_pos + Vec2::from_angle(map.rot).rotate(map.point(local));
                    assert!(
                        (got - want).length() < 1e-4,
                        "{factors} {body_rot} {frame_rot} {half_plane}: {got} vs {want}"
                    );
                    // A body-local direction lands on the world direction.
                    let dir = 0.3_f32;
                    let world_dir = Mat2::from_angle(frame_rot)
                        * Mat2::from_diagonal(factors)
                        * Mat2::from_angle(-frame_rot)
                        * Vec2::from_angle(body_rot + dir);
                    let got_dir = Vec2::from_angle(map.rot + map.direction(dir));
                    assert!(
                        got_dir.perp_dot(world_dir.normalize()).abs() < 1e-4
                            && got_dir.dot(world_dir) > 0.0,
                        "{factors} {body_rot} {frame_rot}: direction {got_dir} vs {world_dir} {map:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_pure_mirror_keeps_a_rotated_box_a_box() {
        let map = BodyScale::new(0.7, 0.0, Vec2::new(-1.0, 1.0), false);
        let b = ShapeDef::Box {
            width: 0.5,
            height: 0.25,
        };
        assert_eq!(map.shape(&b), b);
        assert!((map.rot + 0.7).abs() < f32::EPSILON);
        assert!(map.mirrors());
        // A joint's rest rotation turns with the body.
        assert!((map.rotation(0.7) - map.rot).abs() < 1e-6);
        assert!((map.rotation(0.2) + 0.2).abs() < 1e-6);
    }
}

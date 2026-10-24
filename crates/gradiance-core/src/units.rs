//! Small strongly-typed value types shared across layers.

use crate::constants::INTERACTION_PLANE_Z;
use bevy::math::{Mat3, Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// A 2D position + rotation pair — the authored transform of a body.
///
/// This is the unit moved by transform commands and stored in snapshots;
/// it deliberately excludes scale (bodies are resized by editing their
/// `ShapeDef` (the geometry layer's shape tree), never by scaling).
///
/// Capture from a Bevy `Transform` round-trips its X/Y and Z-rotation
/// while dropping Z-depth and scale — and it quantizes rotation so that
/// `save → load → save` reaches a byte-stable fixpoint:
///
/// ```
/// use gradiance_core::units::PosRot;
/// use bevy::prelude::Transform;
/// use bevy::math::Vec3;
///
/// let mut t = Transform::from_translation(Vec3::new(10.0, -4.0, 99.0));
/// t.rotation = bevy::math::Quat::from_rotation_z(0.5);
///
/// let pose = PosRot::from_transform(&t);
/// assert_eq!(pose.pos, bevy::math::Vec2::new(10.0, -4.0)); // Z dropped
/// assert!((pose.rot - 0.5).abs() < 1e-4);
///
/// // Writing back preserves the Transform's Z and scale.
/// let mut back = Transform::from_translation(Vec3::new(0.0, 0.0, 99.0));
/// pose.apply_to(&mut back);
/// assert_eq!(back.translation.z, 99.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct PosRot {
    /// World-space translation in metres.
    pub pos: Vec2,
    /// Rotation around +Z in radians.
    pub rot: f32,
}

impl PosRot {
    /// Rotation capture resolution in radians (≈ 0.0006°).
    ///
    /// Quaternion→angle extraction is not bit-idempotent (it oscillates by
    /// ~1 ULP), which would make save→load→save never reach a byte-stable
    /// fixpoint. Snapping to a grid ~40× coarser than that noise — and far
    /// below any physical significance — makes capture deterministic.
    const ROT_RESOLUTION: f32 = 1e-5;

    /// Builds a [`PosRot`] from a Bevy [`Transform`](bevy::prelude::Transform),
    /// discarding Z and scale.
    pub fn from_transform(transform: &bevy::prelude::Transform) -> Self {
        Self {
            pos: transform.translation.truncate(),
            rot: quantize_rot(transform.rotation.to_euler(bevy::math::EulerRot::ZYX).0),
        }
    }

    /// Writes this pose onto a Bevy [`Transform`](bevy::prelude::Transform),
    /// preserving its Z translation and scale.
    pub fn apply_to(&self, transform: &mut bevy::prelude::Transform) {
        transform.translation.x = self.pos.x;
        transform.translation.y = self.pos.y;
        transform.rotation = bevy::math::Quat::from_rotation_z(self.rot);
    }
}

/// Snaps a rotation to [`PosRot::ROT_RESOLUTION`] so capture is idempotent.
fn quantize_rot(radians: f32) -> f32 {
    (radians / PosRot::ROT_RESOLUTION).round() * PosRot::ROT_RESOLUTION
}

/// The lift/project isometry between plane-local 2D authoring and the 3D
/// simulation — **the single place two dimensions become three**.
///
/// Gradiance authors in 2D and simulates in 3D. Every authored value ([`PosRot`],
/// a joint anchor, a gravity vector, an angular velocity) is *plane-local*, and
/// every engine value is a `Vec3`/`Quat`; this type is the only sanctioned
/// conversion between them. Nothing outside the physics sync systems should
/// build a `Vec3` out of authored 2D data by hand.
///
/// Today there is exactly one instance, [`PlaneFrame::XY`] — the identity frame
/// at [`INTERACTION_PLANE_Z`], which reproduces [`PosRot::apply_to`] and
/// [`PosRot::from_transform`] exactly. That is what makes multiple simulation
/// planes a matter of supplying a second *value*: no consumer's signature
/// changes, because the 2D side of every conversion is unchanged.
///
/// `x` and `y` are the orthonormal in-plane basis; the plane normal is their
/// cross product, so the frame is right-handed.
///
/// ```
/// use gradiance_core::units::{PlaneFrame, PosRot};
/// use bevy::math::Vec2;
///
/// let plane = PlaneFrame::XY;
/// let pose = PosRot { pos: Vec2::new(3.0, -1.0), rot: 0.5 };
/// // Lift to 3D and project straight back.
/// let round_trip = plane.pose(&plane.transform(pose));
/// assert!((round_trip.pos - pose.pos).length() < 1e-5);
/// assert!((round_trip.rot - pose.rot).abs() < 1e-4);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct PlaneFrame {
    /// Plane origin in world space.
    pub origin: Vec3,
    /// Unit in-plane basis vector that plane-local +X maps to.
    pub x: Vec3,
    /// Unit in-plane basis vector that plane-local +Y maps to.
    pub y: Vec3,
}

impl Default for PlaneFrame {
    fn default() -> Self {
        Self::XY
    }
}

impl PlaneFrame {
    /// The default simulation plane: world XY at [`INTERACTION_PLANE_Z`],
    /// with plane-local axes equal to the world axes.
    ///
    /// Under this frame every lift is an identity embedding and every
    /// projection drops a zero, so authored data round-trips bit-for-bit.
    pub const XY: Self = Self {
        origin: Vec3::new(0.0, 0.0, INTERACTION_PLANE_Z),
        x: Vec3::X,
        y: Vec3::Y,
    };

    /// The plane normal — the axis every in-plane rotation turns about, and the
    /// direction the depth band extends along.
    #[must_use]
    pub fn normal(&self) -> Vec3 {
        self.x.cross(self.y)
    }

    /// The rotation taking world axes to this frame's axes.
    #[must_use]
    pub fn basis(&self) -> Quat {
        Quat::from_mat3(&Mat3::from_cols(self.x, self.y, self.normal()))
    }

    /// Lifts a plane-local point, `off_plane` metres along the normal.
    #[must_use]
    pub fn point(&self, p: Vec2, off_plane: f32) -> Vec3 {
        self.origin + self.x * p.x + self.y * p.y + self.normal() * off_plane
    }

    /// Lifts a plane-local direction or displacement (no origin shift).
    #[must_use]
    pub fn dir(&self, d: Vec2) -> Vec3 {
        self.x * d.x + self.y * d.y
    }

    /// Lifts a scalar spin about the plane normal — angular velocity, torque,
    /// or angular impulse, all of which stay scalars outside the physics layer.
    #[must_use]
    pub fn spin(&self, w: f32) -> Vec3 {
        self.normal() * w
    }

    /// Lifts an authored pose into a world [`Transform`](bevy::prelude::Transform).
    #[must_use]
    pub fn transform(&self, pose: PosRot) -> bevy::prelude::Transform {
        bevy::prelude::Transform {
            translation: self.point(pose.pos, 0.0),
            rotation: self.basis() * Quat::from_rotation_z(pose.rot),
            scale: Vec3::ONE,
        }
    }

    /// Projects a world point to plane-local coordinates plus its signed
    /// distance along the normal.
    #[must_use]
    pub fn project(&self, p: Vec3) -> (Vec2, f32) {
        let d = p - self.origin;
        (
            Vec2::new(d.dot(self.x), d.dot(self.y)),
            d.dot(self.normal()),
        )
    }

    /// Projects a world direction onto the plane, dropping its normal component.
    #[must_use]
    pub fn project_dir(&self, d: Vec3) -> Vec2 {
        Vec2::new(d.dot(self.x), d.dot(self.y))
    }

    /// Projects a world angular vector to a scalar spin about the plane normal.
    #[must_use]
    pub fn unspin(&self, w: Vec3) -> f32 {
        w.dot(self.normal())
    }

    /// Projects a world [`Transform`](bevy::prelude::Transform) back to an
    /// authored pose, dropping the off-plane offset and scale.
    ///
    /// Quantizes rotation exactly as [`PosRot::from_transform`] does, so
    /// `save → load → save` still reaches a byte-stable fixpoint.
    #[must_use]
    pub fn pose(&self, transform: &bevy::prelude::Transform) -> PosRot {
        let local = self.basis().inverse() * transform.rotation;
        PosRot {
            pos: self.project(transform.translation).0,
            rot: quantize_rot(local.to_euler(bevy::math::EulerRot::ZYX).0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Transform;
    use std::f32::consts::FRAC_PI_2;

    fn poses() -> Vec<PosRot> {
        vec![
            PosRot {
                pos: Vec2::ZERO,
                rot: 0.0,
            },
            PosRot {
                pos: Vec2::new(3.0, -1.5),
                rot: 0.75,
            },
            PosRot {
                pos: Vec2::new(-120.0, 44.0),
                rot: -2.4,
            },
        ]
    }

    /// The load-bearing equivalence: on the default plane the new lift/project
    /// pair *is* the old `PosRot` pair, so introducing the frame changes no
    /// behaviour anywhere.
    #[test]
    fn xy_frame_is_pos_rot() {
        for pose in poses() {
            let framed = PlaneFrame::XY.transform(pose);

            let mut direct = Transform::IDENTITY;
            pose.apply_to(&mut direct);
            assert!(
                (framed.translation - direct.translation).length() < 1e-5,
                "{pose:?}: {framed:?} vs {direct:?}"
            );
            // Compare by how each rotation moves the basis, not with
            // `Quat::angle_between` — that is `2·acos(dot)`, which is
            // ill-conditioned near zero and reads ~7e-4 for *identical*
            // quaternions. This form is well conditioned and sign-agnostic.
            for probe in [Vec3::X, Vec3::Y, Vec3::Z] {
                assert!(
                    (framed.rotation * probe - direct.rotation * probe).length() < 1e-5,
                    "{pose:?}: {framed:?} vs {direct:?}"
                );
            }

            // ...and the projection agrees with `from_transform`.
            let by_frame = PlaneFrame::XY.pose(&framed);
            let by_pos_rot = PosRot::from_transform(&framed);
            assert!((by_frame.pos - by_pos_rot.pos).length() < 1e-6);
            assert!((by_frame.rot - by_pos_rot.rot).abs() < 1e-6);
        }
    }

    #[test]
    fn lift_and_project_round_trip_on_a_tilted_frame() {
        // A frame that is not the identity, to prove the maths is general —
        // rotated a quarter turn about world +X, so the plane is world XZ.
        let plane = PlaneFrame {
            origin: Vec3::new(1.0, 2.0, 3.0),
            x: Vec3::X,
            y: Vec3::Z,
        };
        assert!((plane.normal() - Vec3::NEG_Y).length() < 1e-6);

        for pose in poses() {
            let round_trip = plane.pose(&plane.transform(pose));
            assert!(
                (round_trip.pos - pose.pos).length() < 1e-3,
                "{pose:?} -> {round_trip:?}"
            );
            assert!(
                (round_trip.rot - pose.rot).abs() < 1e-3,
                "{pose:?} -> {round_trip:?}"
            );
        }
    }

    #[test]
    fn off_plane_offsets_ride_the_normal() {
        let plane = PlaneFrame::XY;
        let lifted = plane.point(Vec2::new(2.0, 5.0), -0.4);
        assert!((lifted - Vec3::new(2.0, 5.0, -0.4)).length() < 1e-6);
        let (local, off) = plane.project(lifted);
        assert!((local - Vec2::new(2.0, 5.0)).length() < 1e-6);
        assert!((off + 0.4).abs() < 1e-6);
    }

    #[test]
    fn spin_and_unspin_are_inverse_about_the_normal() {
        for plane in [
            PlaneFrame::XY,
            PlaneFrame {
                origin: Vec3::ZERO,
                x: Vec3::Z,
                y: Vec3::Y,
            },
        ] {
            for w in [0.0_f32, 2.5, -7.25] {
                assert!((plane.unspin(plane.spin(w)) - w).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn directions_ignore_the_origin() {
        let plane = PlaneFrame {
            origin: Vec3::new(500.0, -300.0, 12.0),
            x: Vec3::X,
            y: Vec3::Y,
        };
        let d = plane.dir(Vec2::new(0.0, 1.0));
        assert!((d - Vec3::Y).length() < 1e-6);
        assert!((plane.project_dir(d) - Vec2::Y).length() < 1e-6);
    }

    #[test]
    fn a_quarter_turn_in_plane_turns_about_the_normal() {
        let plane = PlaneFrame::XY;
        let turned = plane.transform(PosRot {
            pos: Vec2::ZERO,
            rot: FRAC_PI_2,
        });
        // Plane-local +X must end up along plane-local +Y.
        let moved = turned.rotation * plane.dir(Vec2::X);
        assert!((plane.project_dir(moved) - Vec2::Y).length() < 1e-5);
    }
}

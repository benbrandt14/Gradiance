//! The sponge operation (brief S-12): a body becomes a lattice of small
//! circles joined by springs, inside its outline. One undo step.
//!
//! No new solver: the circles are ordinary bodies and the springs ordinary
//! spring joints, laid by `geometry::lattice::sponge` (structural and shear
//! edges, one connected part). The lattice is one group, so a click selects
//! the whole sponge.

use super::body_of;
use crate::intent::{SpongeIntent, name};
use crate::snapshot::SceneSnapshot;
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{JointCommon, JointDef, JointKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::contours::ring_signed_area;
use gradiance_geometry::polygonize::polygonize;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::{BodyRecord, JointRecord};
use gradiance_units::{Angle, Displacement, Frequency, Length};
use serde::Deserialize;

/// The spring frequency (Hz) the context row uses: stiff enough that a
/// sponge holds its shape under its own weight, soft enough to squash.
pub const SPONGE_STIFFNESS: f32 = 12.0;
/// The springs' damping ratio: a squash settles in about a second.
pub const SPONGE_DAMPING: f32 = 0.5;
/// The most circles one sponge may have: about 4 springs each, so 400
/// circles are about 1 600 joints.
pub const MAX_SPONGE_POINTS: usize = 400;
/// The smallest spacing (m). Below it the circles are smaller than the
/// solver's comfortable size.
pub const MIN_SPONGE_SPACING: f32 = 0.03;
/// Auto spacing: the smaller side of the body's box over this count.
const AUTO_ACROSS: f32 = 6.0;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Sponge>();
}

/// Turn the body `i` into a sponge: circles `spacing` apart (0: about six
/// across the body) joined by springs of natural frequency `stiffness`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Sponge {
    /// The body (`"selection"`: the primary selected body).
    pub i: Target,
    /// Distance between circle centres; 0 picks about six across.
    pub spacing: Length,
    /// The springs' natural frequency (Hz): higher is stiffer.
    pub stiffness: Frequency,
}

impl Operation for Sponge {
    const META: OpMeta = OpMeta {
        id: "body.sponge",
        lisp: "sponge",
        doc: "Turn the i-th body into a sponge: circles spacing apart (0: about six across) inside its outline, joined by springs of natural frequency stiffness (Hz), as one group — one undo step. Grounds and fluid regions stay.",
        seam: Seam::Edit,
        menu: Some("Context/Make sponge"),
        chord: None,
        undo: Some(name::SPONGE),
        example: r#"{"i": 5, "spacing": 0, "stiffness": 12}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none());
        };
        if body.shape.contains_half_plane() || body.fluid.is_some() {
            return Ok(OpOutput::none());
        }
        let stiffness = self.stiffness.0;
        if !(stiffness.is_finite() && stiffness > 0.0) {
            return Err(OpError::Invalid(format!(
                "stiffness {stiffness}: a positive frequency"
            )));
        }
        // The largest outer ring, in the body's frame.
        let contours = polygonize(&body.shape);
        let Some(outline) = contours
            .rings()
            .filter(|r| ring_signed_area(r) > 0.0)
            .max_by(|a, b| ring_signed_area(a).total_cmp(&ring_signed_area(b)))
        else {
            return Ok(OpOutput::none());
        };
        let spacing = match self.spacing.0 {
            0.0 => auto_spacing(outline),
            s if s.is_finite() && s >= MIN_SPONGE_SPACING => s,
            s => {
                return Err(OpError::Invalid(format!(
                    "spacing {s}: 0 (auto) or at least {MIN_SPONGE_SPACING} m"
                )));
            }
        };
        let (points, edges) = gradiance_geometry::lattice::sponge(outline, spacing);
        if points.len() > MAX_SPONGE_POINTS {
            return Err(OpError::Invalid(format!(
                "{} circles at spacing {spacing} m: at most {MAX_SPONGE_POINTS} (a larger spacing)",
                points.len()
            )));
        }
        if points.len() < 2 {
            return Err(OpError::Invalid(format!(
                "spacing {spacing} m leaves fewer than two circles in the body"
            )));
        }
        let turn = Vec2::from_angle(body.rot);
        let bodies: Vec<BodyRecord> = points
            .iter()
            .map(|p| {
                let mut record = super::spawned_body_record(
                    ctx.new_id(),
                    ShapeDef::Circle {
                        radius: 0.5 * spacing,
                    },
                    body.pos + turn.rotate(*p),
                );
                record.physics = body.physics;
                record.physics.rigid_body = RigidBody::Dynamic;
                record.appearance = body.appearance;
                record.depth = body.depth;
                record
            })
            .collect();
        let joints = edges
            .iter()
            .map(|&(a, b)| spring(ctx, &bodies[a], &bodies[b], stiffness))
            .collect();
        Ok(OpOutput::none().message(SpongeIntent {
            target: body.id,
            bodies,
            joints,
        }))
    }
}

/// About [`AUTO_ACROSS`] circles across the smaller side of `outline`'s box.
fn auto_spacing(outline: &[Vec2]) -> f32 {
    let (lo, hi) = outline.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(*v), hi.max(*v)),
    );
    ((hi - lo).min_element() / AUTO_ACROSS).max(MIN_SPONGE_SPACING)
}

/// A spring between the centres of two lattice circles, relaxed at their
/// distance. Connected circles do not collide (they are a spacing apart
/// and touch).
fn spring(ctx: &OpContext, a: &BodyRecord, b: &BodyRecord, frequency: f32) -> JointRecord {
    let PosRot { pos: pa, .. } = a.pose;
    let PosRot { pos: pb, .. } = b.pose;
    JointRecord {
        id: ctx.new_id(),
        def: JointDef {
            kind: JointKind::Spring {
                rest_length: Length(pa.distance(pb)),
                frequency: Frequency(frequency),
                damping_ratio: SPONGE_DAMPING,
                range: None,
            },
            common: JointCommon::default(),
            body_a: a.id,
            body_b: Some(b.id),
            anchor_a: Displacement(Vec2::ZERO),
            anchor_b: Displacement(Vec2::ZERO),
            rest_rot_a: Angle(0.0),
            rest_rot_b: Angle(0.0),
        },
    }
}

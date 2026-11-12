//! The rod tool: a rigid rod drawn between two points.
//!
//! Press at one end, drag, release at the other. The gesture authors a
//! thin capsule body ([`ShapeDef::Capsule`] — a true 1-D collider) plus a
//! **hinge** for each end that lands on a body; an end over empty space
//! stays free. Everything commits atomically as one
//! [`ToolCommit::SpawnRod`] (one undo step).
//!
//! A *fixed* end is deliberately not offered: a rod fixed to a body is a
//! merge (the weld tool's SDF union), not a weld joint — see
//! `docs/briefs/2-3-rigid-rod.md`. The spring-damper counterpart is the
//! spring tool (`spring_tool`).

use crate::selection::Selection;
use crate::snap::DraftReference;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::joint::{JointCommon, JointDef, JointKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_scene::{BodyRecord, JointRecord};

/// Shortest rod that will commit (world metres); a near-zero drag is a no-op.
pub const MIN_ROD_LENGTH: f32 = 0.05;
/// Rod capsule radius (half the drawn thickness, metres) — reads as a thick
/// line next to 1-px gizmo strokes at the default zoom.
pub const ROD_RADIUS: f32 = 0.025;
/// End-attachment tolerance in screen pixels (scaled to world by the
/// camera): a snapped end lands *exactly on* an edge or another rod's end,
/// where exact containment is a coin flip.
const ATTACH_TOLERANCE_PX: f32 = 8.0;
/// Rod density (kg/m²): a thin capsule at unit density is the light side of
/// a bad solver mass ratio against typical attached bodies (loose, jittery
/// hinges). A denser rod keeps the constraint firm; editable per rod in the
/// physics inspector as usual.
const ROD_DENSITY: f32 = 5.0;

/// In-progress rod gesture (the first end).
#[derive(Resource, Default, Debug)]
pub struct RodDraft(pub Option<Vec2>);

impl ManipTool for RodDraft {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        if ctx.left == GesturePhase::Pressed
            && let Some(p) = ctx.cursor
        {
            self.0 = Some(p);
        }

        if ctx.left == GesturePhase::Released
            && let Some(end_a) = self.0.take()
            && let Some(end_b) = ctx.cursor
            && let Some(commit) =
                build_rod(end_a, end_b, ATTACH_TOLERANCE_PX * ctx.cam_scale, world)
        {
            return ManipOutput::commit(commit);
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    /// The first end: the rod is a segment drawn from it.
    fn snap_reference(&self) -> Option<DraftReference> {
        self.0.map(DraftReference::Line)
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(a) = self.0 else {
            return;
        };
        let dot = 4.0 * ctx.cam_scale;
        out.circle(a, dot, css::BLACK);
        if let Some(b) = ctx.cursor {
            // The capsule's two long sides plus its axis read as the rod's
            // real thickness.
            let perp = (b - a).normalize_or_zero().perp() * ROD_RADIUS;
            out.line(a, b, css::BLACK);
            out.line(a + perp, b + perp, css::BLACK);
            out.line(a - perp, b - perp, css::BLACK);
            out.circle(b, dot, css::BLACK);
        }
    }
}

/// The rod's record set for a completed gesture from `a` to `b`, or `None`
/// when the drag is too short. `tol` is the end-attachment tolerance in
/// world units.
fn build_rod(a: Vec2, b: Vec2, tol: f32, world: &ToolWorld) -> Option<ToolCommit> {
    let (body, ends) = rod_body(a, b)?;
    let joints = ends
        .into_iter()
        .filter_map(|(point, rod_anchor)| {
            let hit = *world.attachment_bodies_at(point, tol).first()?;
            let (_, pose) = world.shape_pose(hit)?;
            let id = world.id_of(hit)?;
            Some(end_hinge(&body, rod_anchor, id, pose.pos, pose.rot, point))
        })
        .collect();
    Some(ToolCommit::SpawnRod {
        body: Box::new(body),
        joints,
    })
}

/// The capsule body spanning `a`→`b` and its two ends as `(world point,
/// rod-local anchor)`, or `None` below [`MIN_ROD_LENGTH`].
pub fn rod_body(a: Vec2, b: Vec2) -> Option<(BodyRecord, [(Vec2, Vec2); 2])> {
    let length = a.distance(b);
    if length.is_nan() || length < MIN_ROD_LENGTH {
        return None;
    }
    let half = length / 2.0;
    let mut body = crate::tools::new_body_record(
        ShapeDef::Capsule {
            half_length: half,
            radius: ROD_RADIUS,
        },
        (a + b) / 2.0,
        (b - a).to_angle(),
    );
    body.physics.density = avian2d::prelude::ColliderDensity(ROD_DENSITY);
    body.appearance = Appearance {
        fill: Rgba {
            r: 0.08,
            g: 0.08,
            b: 0.09,
            a: 1.0,
        },
        ..Appearance::default()
    };
    Some((
        body,
        [(a, Vec2::new(-half, 0.0)), (b, Vec2::new(half, 0.0))],
    ))
}

/// A hinge pinning the rod's end (`rod_anchor`, rod-local) to the body `id`
/// posed at (`pos`, `rot`), at the world point `point`.
pub fn end_hinge(
    rod: &BodyRecord,
    rod_anchor: Vec2,
    id: StableId,
    pos: Vec2,
    rot: f32,
    point: Vec2,
) -> JointRecord {
    JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind: JointKind::Hinge {
                limits: None,
                motor: None,
            },
            common: JointCommon::default(),
            body_a: rod.id,
            body_b: Some(id),
            anchor_a: rod_anchor,
            anchor_b: Vec2::from_angle(-rot).rotate(point - pos),
            rest_rot_a: rod.pose.rot,
            rest_rot_b: rot,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rod_body_spans_its_ends_exactly() {
        let (a, b) = (Vec2::new(1.0, 1.0), Vec2::new(1.0, 3.0));
        let (body, ends) = rod_body(a, b).unwrap();
        assert_eq!(
            body.shape,
            ShapeDef::Capsule {
                half_length: 1.0,
                radius: ROD_RADIUS
            }
        );
        assert!((body.pose.pos - Vec2::new(1.0, 2.0)).length() < 1e-6);
        assert!((body.pose.rot - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // Each rod-local anchor maps back to its world end.
        for (world, local) in ends {
            let back = body.pose.pos + Vec2::from_angle(body.pose.rot).rotate(local);
            assert!((back - world).length() < 1e-5, "{back} != {world}");
        }
    }

    #[test]
    fn a_too_short_drag_is_no_rod() {
        assert!(rod_body(Vec2::ZERO, Vec2::new(0.01, 0.0)).is_none());
        assert!(rod_body(Vec2::ZERO, Vec2::splat(f32::NAN)).is_none());
    }

    #[test]
    fn an_end_hinge_anchors_both_bodies_at_the_same_world_point() {
        let (a, b) = (Vec2::ZERO, Vec2::new(2.0, 0.0));
        let (rod, ends) = rod_body(a, b).unwrap();
        let (point, anchor) = ends[1];
        let other = StableId::new();
        let (pos, rot) = (Vec2::new(3.0, 1.0), 0.7);
        let hinge = end_hinge(&rod, anchor, other, pos, rot, point);
        assert_eq!(hinge.def.body_a, rod.id);
        assert_eq!(hinge.def.body_b, Some(other));
        assert!(!hinge.def.common.broken && hinge.def.common.break_force.is_none());
        let on_b = pos + Vec2::from_angle(rot).rotate(hinge.def.anchor_b);
        assert!((on_b - point).length() < 1e-5);
    }
}

//! The strut tool: a spring-damper connecting two anchor points.
//!
//! Unlike the connector tools (which link two bodies at a single shared
//! point), a strut spans a *distance*: press on the first anchor, drag to the
//! second, release. The drag length becomes the rest length, so the spring
//! starts relaxed. Endpoints resolve to the topmost body under each anchor —
//! or a world pin when the far end sits on empty space.
//!
//! World reads go through the read-total [`ToolWorld`] facade; the created
//! joint leaves as a [`ToolCommit::SpawnJoint`] through the shared commit seam.

use crate::selection::Selection;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{DEFAULT_SPRING_DAMPING, JointCommon, JointDef, JointKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_scene::JointRecord;

/// Shortest strut that will commit (world metres); a near-zero drag is a no-op.
const MIN_STRUT_LENGTH: f32 = 0.05;
/// Spring stiffness per unit mass proxy (AABB area, px²). Tuned so a typical
/// body sags ~10 px under the default gravity (|g| ≈ 1000): `k ≈ m·g / sag`, so
/// heavier bodies get proportionally stiffer struts instead of drooping.
const SPRING_STIFFNESS_PER_MASS: f32 = 100.0;
/// Floor for the mass-based stiffness so tiny bodies still get a firm strut.
const MIN_SPRING_STIFFNESS: f32 = 0.01;

/// In-progress strut gesture (the first anchor).
#[derive(Resource, Default, Debug)]
pub struct StrutDraft(pub Option<Vec2>);

impl ManipTool for StrutDraft {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        // Press: the first anchor is the snapped cursor.
        if ctx.left == GesturePhase::Pressed
            && let Some(p) = ctx.cursor
        {
            self.0 = Some(p);
        }

        // Release: build the strut from the first anchor to the cursor.
        if ctx.left == GesturePhase::Released
            && let Some(anchor_a) = self.0.take()
            && let Some(anchor_b) = ctx.cursor
            && let Some(commit) = build_strut(anchor_a, anchor_b, world)
        {
            return ManipOutput::commit(commit);
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(anchor) = self.0 else {
            return;
        };
        out.circle(anchor, 4.0, css::SPRING_GREEN);
        if let Some(p) = ctx.cursor {
            out.line(anchor, p, css::SPRING_GREEN);
            out.circle(p, 4.0, css::SPRING_GREEN);
        }
    }
}

/// Builds the strut record for a completed gesture, or `None` if no body sits
/// at the first anchor or the drag is too short.
fn build_strut(anchor_a: Vec2, anchor_b: Vec2, world: &ToolWorld) -> Option<ToolCommit> {
    let rest = anchor_a.distance(anchor_b);
    if rest < MIN_STRUT_LENGTH {
        return None;
    }

    // The first anchor must land on a body (the strut's fixed end).
    let &hit_a = world.bodies_at(anchor_a).first()?;
    let (shape_a, pose_a) = world.shape_pose(hit_a)?;
    let id_a = world.id_of(hit_a)?;
    let to_local = |pose: PosRot, world: Vec2| Vec2::from_angle(-pose.rot).rotate(world - pose.pos);

    // The far end: the first *different* body under it, else a world pin.
    let far = world
        .bodies_at(anchor_b)
        .into_iter()
        .find(|&e| e != hit_a)
        .and_then(|e| Some((world.shape_pose(e)?, world.id_of(e)?)));

    // Stiffness scales with the heavier connected body's mass so the strut
    // isn't too soft under gravity.
    let mass = mass_proxy(&shape_a).max(far.as_ref().map_or(0.0, |((s, _), _)| mass_proxy(s)));
    let stiffness = (SPRING_STIFFNESS_PER_MASS * mass).max(MIN_SPRING_STIFFNESS);

    let mut def = JointDef {
        kind: JointKind::Spring {
            rest_length: rest,
            stiffness,
            damping: DEFAULT_SPRING_DAMPING,
            range: None,
        },
        // Strut bodies collide by default; adjust via the collision-layer menu.
        common: JointCommon {
            collide_connected: true,
        },
        body_a: id_a,
        body_b: None,
        anchor_a: to_local(pose_a, anchor_a),
        anchor_b, // world point for the world-pin case
        rest_rot_a: pose_a.rot,
        rest_rot_b: 0.0,
    };

    if let Some(((_, pose_b), id_b)) = far {
        def.body_b = Some(id_b);
        def.anchor_b = to_local(pose_b, anchor_b);
        def.rest_rot_b = pose_b.rot;
    }

    Some(ToolCommit::SpawnJoint(Box::new(JointRecord {
        id: StableId::new(),
        def,
    })))
}

/// A body's mass proxy for the default strut stiffness: its AABB area (× unit
/// density), or 0 for a ground half-plane (statics don't sag, so they never set
/// the stiffness).
fn mass_proxy(shape: &ShapeDef) -> f32 {
    if shape.contains_half_plane() {
        return 0.0;
    }
    let (min, max) = gradiance_geometry::sdf::aabb(shape);
    ((max.x - min.x) * (max.y - min.y)).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_proxy_scales_with_area_and_ignores_ground() {
        let small = ShapeDef::Box {
            width: 10.0,
            height: 10.0,
        };
        let big = ShapeDef::Box {
            width: 40.0,
            height: 20.0,
        };
        assert!(
            mass_proxy(&big) > mass_proxy(&small),
            "bigger body = more mass"
        );
        assert!(
            mass_proxy(&small) >= 1.0,
            "a floor keeps struts from being zero-stiffness"
        );
        assert!(
            mass_proxy(&ShapeDef::HalfPlane).abs() < f32::EPSILON,
            "ground doesn't sag"
        );
    }
}

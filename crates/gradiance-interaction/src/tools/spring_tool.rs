//! The spring tool (formerly "strut"): a spring-damper connecting two
//! anchor points. The rigid counterpart is the rod tool (`rod_tool`).
//!
//! Unlike the connector tools (which link two bodies at a single shared
//! point), a spring spans a *distance*: press on the first anchor, drag to the
//! second, release. The drag length becomes the rest length, so the spring
//! starts relaxed. Endpoints resolve to the topmost body under each anchor —
//! or a world pin when one end sits on empty space, whichever end that is
//! (Algodoo's spring joins the background at either end).
//!
//! World reads go through the read-total [`ToolWorld`] facade; the created
//! joint leaves as a [`ToolCommit::SpawnJoint`] through the shared commit seam.

use crate::selection::Selection;
use crate::snap::DraftReference;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{
    DEFAULT_SPRING_DAMPING_RATIO, DEFAULT_SPRING_FREQUENCY, JointCommon, JointDef, JointKind,
};
use gradiance_domain::theme::Role;
use gradiance_scene::JointRecord;

/// Shortest spring that will commit (world metres); a near-zero drag is a no-op.
const MIN_SPRING_LENGTH: f32 = 0.05;
/// How far (screen pixels, scaled to world by the camera) a body's surface
/// may sit from an end and still count as under it: a snapped end lies
/// exactly on an edge or a vertex (see the connector tools).
const ANCHOR_TOLERANCE_PX: f32 = 1.0;
/// In-progress spring gesture (the first anchor).
#[derive(Resource, Default, Debug)]
pub struct SpringDraft(pub Option<Vec2>);

impl ManipTool for SpringDraft {
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

        // Release: build the spring from the first anchor to the cursor.
        if ctx.left == GesturePhase::Released
            && let Some(anchor_a) = self.0.take()
            && let Some(anchor_b) = ctx.cursor
            && let Some(commit) = build_spring(
                anchor_a,
                anchor_b,
                ANCHOR_TOLERANCE_PX * ctx.cam_scale,
                world,
            )
        {
            return ManipOutput::commit(commit);
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    /// The first anchor: the spring is a segment drawn from it.
    fn snap_reference(&self) -> Option<DraftReference> {
        self.0.map(DraftReference::Line)
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(anchor) = self.0 else {
            return;
        };
        // Screen-sized handle dots: 4 px, scaled to world by `cam_scale`.
        let r = 4.0 * ctx.cam_scale;
        out.circle(anchor, r, Role::Alternate);
        if let Some(p) = ctx.cursor {
            out.line(anchor, p, Role::Alternate);
            out.circle(p, r, Role::Alternate);
        }
    }
}

/// Builds the spring record for a completed gesture, or `None` if neither
/// end lands on a body or the drag is too short. When only the far end
/// lands on a body, that body is the spring's body and the first anchor
/// its world pin (the joint is symmetric, so the order is free).
fn build_spring(anchor_a: Vec2, anchor_b: Vec2, tol: f32, world: &ToolWorld) -> Option<ToolCommit> {
    let rest = anchor_a.distance(anchor_b);
    if rest < MIN_SPRING_LENGTH {
        return None;
    }

    // One end must land on a body (the spring's body A); started on empty
    // space, the ends swap so the far end's body is body A.
    let (anchor_a, anchor_b, hit_a) = match world.joint_bodies_at(anchor_a, tol).first() {
        Some(&hit) => (anchor_a, anchor_b, hit),
        None => (
            anchor_b,
            anchor_a,
            *world.joint_bodies_at(anchor_b, tol).first()?,
        ),
    };
    let (_, pose_a) = world.shape_pose(hit_a)?;
    let id_a = world.id_of(hit_a)?;
    let to_local = |pose: PosRot, world: Vec2| Vec2::from_angle(-pose.rot).rotate(world - pose.pos);

    // The far end: the first *different* body under it, else a world pin.
    let far = world
        .joint_bodies_at(anchor_b, tol)
        .into_iter()
        .find(|&e| e != hit_a)
        .and_then(|e| Some((world.shape_pose(e)?, world.id_of(e)?)));

    let mut def = JointDef {
        kind: JointKind::Spring {
            rest_length: gradiance_units::Length(rest),
            // Mass-relative: the physics seam derives the stiffness from the
            // connected bodies' mass, so the default bounces alike on a
            // pebble and on a truck (brief 4b-4).
            frequency: gradiance_units::Frequency(DEFAULT_SPRING_FREQUENCY),
            damping_ratio: DEFAULT_SPRING_DAMPING_RATIO,
            range: None,
        },
        // Spring bodies collide by default; adjust via the collision-layer menu.
        common: JointCommon {
            collide_connected: true,
            ..JointCommon::default()
        },
        body_a: id_a,
        body_b: None,
        anchor_a: gradiance_units::Displacement(to_local(pose_a, anchor_a)),
        anchor_b: gradiance_units::Displacement(anchor_b), // world point for the world-pin case
        rest_rot_a: gradiance_units::Angle(pose_a.rot),
        rest_rot_b: gradiance_units::Angle(0.0),
    };

    if let Some(((_, pose_b), id_b)) = far {
        def.body_b = Some(id_b);
        def.anchor_b = gradiance_units::Displacement(to_local(pose_b, anchor_b));
        def.rest_rot_b = gradiance_units::Angle(pose_b.rot);
    }

    Some(ToolCommit::SpawnJoint(Box::new(JointRecord {
        id: StableId::new(),
        def,
    })))
}

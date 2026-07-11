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

use crate::command::snapshot::JointRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::joint::{
    DEFAULT_SPRING_DAMPING, DEFAULT_SPRING_STIFFNESS, JointCommon, JointDef, JointKind,
};
use crate::interaction::selection::Selection;
use crate::interaction::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Shortest strut that will commit (world px); a near-zero drag is a no-op.
const MIN_STRUT_LENGTH: f32 = 5.0;

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
    let pose_a = world.pose_of(hit_a)?;
    let id_a = world.id_of(hit_a)?;
    let to_local = |pose: PosRot, world: Vec2| Vec2::from_angle(-pose.rot).rotate(world - pose.pos);

    let mut def = JointDef {
        kind: JointKind::Spring {
            bounds: [rest, rest],
            stiffness: DEFAULT_SPRING_STIFFNESS,
            damping: DEFAULT_SPRING_DAMPING,
        },
        common: JointCommon::default(),
        body_a: id_a,
        body_b: None,
        anchor_a: to_local(pose_a, anchor_a),
        anchor_b, // world point for the world-pin case
        rest_rot_a: pose_a.rot,
        rest_rot_b: 0.0,
    };

    // The far end connects to the first *different* body under it, else a world
    // pin at the released point.
    if let Some(hit_b) = world.bodies_at(anchor_b).into_iter().find(|&e| e != hit_a) {
        let id_b = world.id_of(hit_b)?;
        let pose_b = world.pose_of(hit_b)?;
        def.body_b = Some(id_b);
        def.anchor_b = to_local(pose_b, anchor_b);
        def.rest_rot_b = pose_b.rot;
    }

    Some(ToolCommit::SpawnJoint(Box::new(JointRecord {
        id: StableId::new(),
        def,
    })))
}

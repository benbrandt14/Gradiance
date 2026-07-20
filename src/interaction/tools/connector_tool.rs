//! Connector tools: hinge and slider joints, and the weld (merge) tool.
//!
//! One shared [`ManipTool`] state machine (a new joint kind = one match arm):
//! - **Hinge**: click at a point — the two topmost bodies there get
//!   connected; a single body gets pinned to the world. The anchor is the
//!   *snapped* cursor, so joints land exactly on vertices, centers, or
//!   grid points (CAD-style precise linkages).
//! - **Weld** is *not a joint* (M20 rework, feedback 4.2): over two bodies
//!   it merges them into one (SDF `Union` — physically identical to a rigid
//!   link, with no constraint drift); over a single body it pins it to the
//!   world by making it static.
//! - **Slider**: press at the anchor, drag to define the slide axis,
//!   release. A short drag defaults to the body's local X axis.
//!
//! World reads (which bodies are at the anchor, their poses/ids) go through
//! the read-total [`ToolWorld`] facade; commits leave through the shared
//! commit seam ([`ToolCommit::SpawnJoint`] / `Merge` / `MakeStatic`).

use crate::scene::JointRecord;
use crate::core::ids::StableId;
use crate::core::states::ToolState;
use crate::core::units::PosRot;
use crate::domain::joint::{JointCommon, JointDef, JointKind};
use crate::interaction::selection::Selection;
use crate::interaction::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use avian2d::prelude::RigidBody;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Below this drag length a slider uses the body's local X axis.
const AXIS_THRESHOLD: f32 = 5.0;

/// In-progress connector gesture (anchor point).
#[derive(Resource, Default, Debug)]
pub struct ConnectorDraft(pub Option<Vec2>);

/// Run condition: the connector driver is active for the three joint tools.
pub fn connector_active(tool: Res<State<ToolState>>) -> bool {
    matches!(
        *tool.get(),
        ToolState::Hinge | ToolState::Weld | ToolState::Slider
    )
}

impl ManipTool for ConnectorDraft {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        // Press on the canvas: set the anchor (the snapped cursor).
        if ctx.left == GesturePhase::Pressed
            && let Some(p) = ctx.cursor
        {
            self.0 = Some(p);
        }

        // Release: connect (or weld) the topmost body/bodies at the anchor.
        if ctx.left == GesturePhase::Released
            && let Some(anchor) = self.0.take()
        {
            let commit = if ctx.tool == ToolState::Weld {
                build_weld(anchor, world)
            } else {
                build_joint(
                    ctx.tool,
                    anchor,
                    ctx.cursor.unwrap_or(anchor),
                    ctx.defaults,
                    world,
                )
            };
            if let Some(commit) = commit {
                return ManipOutput::commit(commit);
            }
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
        out.circle(anchor, 5.0, css::VIOLET);
        if ctx.tool == ToolState::Slider
            && let Some(p) = ctx.cursor
            && anchor.distance(p) > AXIS_THRESHOLD
        {
            let dir = (p - anchor).normalize();
            out.line(anchor - dir * 200.0, anchor + dir * 200.0, css::VIOLET);
        }
    }
}

/// Builds the weld tool's commit: two (finite) bodies at the anchor merge
/// into one; a single body is pinned to the world by becoming static; the
/// infinite ground is never a weld target (welding *onto* it is exactly the
/// make-static case).
fn build_weld(anchor: Vec2, world: &ToolWorld) -> Option<ToolCommit> {
    let hits: Vec<_> = world
        .bodies_at(anchor)
        .into_iter()
        .filter(|e| {
            world
                .shape_pose(*e)
                .is_some_and(|(shape, _)| !shape.contains_half_plane())
        })
        .collect();
    match hits.as_slice() {
        [] => None,
        [single] => {
            let id = world.id_of(*single)?;
            let old = world.body_kind(*single)?;
            // Welding an already-static body is a no-op, not a dead command.
            (old != RigidBody::Static).then_some(ToolCommit::MakeStatic { id, old })
        }
        [a, b, ..] => Some(ToolCommit::Merge {
            targets: vec![world.id_of(*a)?, world.id_of(*b)?],
        }),
    }
}

/// Builds the joint record for a completed connector gesture, or `None` if no
/// body sits at the anchor (or an endpoint can't be resolved).
fn build_joint(
    tool: ToolState,
    anchor: Vec2,
    release: Vec2,
    defaults: crate::domain::settings::ToolDefaults,
    world: &ToolWorld,
) -> Option<ToolCommit> {
    // The two topmost bodies at the anchor; one body → world pin.
    let hits = world.bodies_at(anchor);
    let &body_a = hits.first()?;
    let pose_a = world.pose_of(body_a)?;
    let id_a = world.id_of(body_a)?;
    let to_local = |pose: PosRot, world: Vec2| Vec2::from_angle(-pose.rot).rotate(world - pose.pos);

    let kind = match tool {
        ToolState::Slider => {
            let drag = release - anchor;
            let world_axis = if drag.length() < AXIS_THRESHOLD {
                Vec2::from_angle(pose_a.rot) // body-A local X in world
            } else {
                drag.normalize()
            };
            // Default travel limits (ToolDefaults option): the drag *draws*
            // the allowed travel, `[0, drag length]` along the axis. A short
            // drag never carried travel intent, so it stays unlimited.
            let limits = (defaults.slider_limits && drag.length() >= AXIS_THRESHOLD)
                .then_some([0.0, drag.length()]);
            JointKind::Slider {
                // Authored in body-A local space.
                axis: Vec2::from_angle(-pose_a.rot).rotate(world_axis),
                limits,
                motor: None,
            }
        }
        _ => JointKind::Hinge {
            limits: None,
            motor: None,
        },
    };

    let mut def = JointDef {
        kind,
        common: JointCommon::default(),
        body_a: id_a,
        body_b: None,
        anchor_a: to_local(pose_a, anchor),
        anchor_b: anchor, // world point for the world-pin case
        rest_rot_a: pose_a.rot,
        rest_rot_b: 0.0,
    };
    if let Some(&body_b) = hits.get(1) {
        let id_b = world.id_of(body_b)?;
        let pose_b = world.pose_of(body_b)?;
        def.body_b = Some(id_b);
        def.anchor_b = to_local(pose_b, anchor);
        def.rest_rot_b = pose_b.rot;
    }

    Some(ToolCommit::SpawnJoint(Box::new(JointRecord {
        id: StableId::new(),
        def,
    })))
}

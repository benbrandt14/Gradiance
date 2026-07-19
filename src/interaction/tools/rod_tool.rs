//! The strut tool: a rigid rod drawn between two points.
//!
//! Press at one end, drag, release at the other. The gesture authors a
//! thin black capsule body (a true 1D collider on the rod's depth layer)
//! plus an end joint for each endpoint that lands on another body — weld
//! or hinge per [`ToolDefaults`](crate::domain::settings::ToolDefaults).
//! An end over empty space stays free. Everything commits atomically as
//! one [`ToolCommit::SpawnRod`] (one undo step).

use crate::command::rod_cmd::RodSpec;
use crate::command::snapshot::{BodyRecord, JointRecord};
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::joint::{JointCommon, JointDef, JointKind};
use crate::domain::rod::{Rod, RodEndKind};
use crate::domain::shape::ShapeDef;
use crate::interaction::selection::Selection;
use crate::interaction::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Shortest rod that will commit (world px).
const MIN_ROD_LENGTH: f32 = 10.0;
/// Rod capsule radius (half the drawn thickness, world px) — reads as a
/// thick line next to 1-px gizmo strokes.
pub const ROD_RADIUS: f32 = 2.5;

/// In-progress rod gesture (the first endpoint).
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
            && let Some(commit) = build_rod(end_a, end_b, world, ctx)
        {
            return ManipOutput::commit(commit);
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(a) = self.0 else {
            return;
        };
        out.circle(a, ROD_RADIUS + 1.5, css::BLACK);
        if let Some(b) = ctx.cursor {
            // A tripled stroke reads as the rod's real thickness.
            let dir = (b - a).normalize_or_zero();
            let perp = Vec2::new(-dir.y, dir.x) * ROD_RADIUS;
            out.line(a, b, css::BLACK);
            out.line(a + perp, b + perp, css::BLACK);
            out.line(a - perp, b - perp, css::BLACK);
            out.circle(b, ROD_RADIUS + 1.5, css::BLACK);
        }
    }
}

/// The `RodEndKind` the current tool defaults author.
fn default_end(ctx: &ManipContext) -> RodEndKind {
    if ctx.defaults.rod_fixed_ends {
        RodEndKind::Fixed
    } else {
        RodEndKind::Hinge
    }
}

/// Builds the rod's composite record set for a completed gesture, or
/// `None` when the drag is too short.
fn build_rod(a: Vec2, b: Vec2, world: &ToolWorld, ctx: &ManipContext) -> Option<ToolCommit> {
    let length = a.distance(b);
    if length < MIN_ROD_LENGTH {
        return None;
    }
    let end_kind = default_end(ctx);
    let rot = (b - a).to_angle();
    let rod_id = StableId::new();
    let body = BodyRecord {
        id: rod_id,
        pose: PosRot {
            pos: (a + b) / 2.0,
            rot,
        },
        shape: ShapeDef::Capsule {
            half_length: length / 2.0,
            radius: ROD_RADIUS,
        },
        physics: crate::domain::props::BodyPhysics::default(),
        appearance: rod_appearance(),
        depth: crate::domain::depth::DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        rod: Some(Rod {
            end_a: end_kind,
            end_b: end_kind,
        }),
    };

    // End joints: each endpoint that lands on a body gets one; empty
    // space leaves the end free.
    let half = length / 2.0;
    let joints: Vec<JointRecord> = [(a, Vec2::new(-half, 0.0)), (b, Vec2::new(half, 0.0))]
        .into_iter()
        .filter_map(|(point, rod_anchor)| {
            end_joint(world, point, rod_id, rod_anchor, rot, end_kind)
        })
        .collect();

    Some(ToolCommit::SpawnRod(Box::new(RodSpec {
        bodies: vec![body],
        joints,
    })))
}

/// The rod's thick-black-line look.
fn rod_appearance() -> crate::domain::appearance::Appearance {
    crate::domain::appearance::Appearance {
        fill: crate::domain::appearance::Rgba {
            r: 0.02,
            g: 0.02,
            b: 0.02,
            a: 1.0,
        },
        ..Default::default()
    }
}

/// A weld/hinge joint attaching the rod's end to the topmost body under
/// `point`, or `None` when nothing is there (a free end).
fn end_joint(
    world: &ToolWorld,
    point: Vec2,
    rod_id: StableId,
    rod_anchor: Vec2,
    rod_rot: f32,
    kind: RodEndKind,
) -> Option<JointRecord> {
    let &hit = world.bodies_at(point).first()?;
    let (_, pose) = world.shape_pose(hit)?;
    let hit_id = world.id_of(hit)?;
    let to_local = Vec2::from_angle(-pose.rot).rotate(point - pose.pos);
    let joint_kind = match kind {
        RodEndKind::Fixed => JointKind::Weld,
        RodEndKind::Hinge => JointKind::Hinge {
            limits: None,
            motor: None,
        },
    };
    Some(JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind: joint_kind,
            common: JointCommon {
                collide_connected: false,
            },
            body_a: rod_id,
            body_b: Some(hit_id),
            anchor_a: rod_anchor,
            anchor_b: to_local,
            rest_rot_a: rod_rot,
            rest_rot_b: pose.rot,
        },
    })
}

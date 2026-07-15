//! The tracer tool: click to place a [tracer node](crate::domain::node) —
//! a standalone, individually-selectable trajectory probe.
//!
//! Placed **on a body**, the node attaches and rides it (its trail traces
//! that body's motion); placed **in empty space**, it is a free node fixed
//! at the click. This is the first "a dataflow node is its own tool"
//! instance; future sensors/actuators are sibling tools returning
//! [`ToolCommit::SpawnNode`] the same way.
//!
//! World reads (the body under the cursor, its pose/id) go through the
//! read-total [`ToolWorld`] facade; the commit leaves through the shared
//! seam. One click commits exactly one `SpawnNode`.

use crate::command::snapshot::NodeRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::node::{NodeAttachment, NodeKind};
use crate::domain::tracer::Tracer;
use crate::interaction::selection::Selection;
use crate::interaction::tools::appearance_for_id;
use crate::interaction::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// The tracer tool holds no drag state — a single click places a node.
#[derive(Resource, Default, Debug)]
pub struct TracerTool;

impl ManipTool for TracerTool {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        if ctx.left != GesturePhase::Released {
            return ManipOutput::default();
        }
        let Some(p) = ctx.cursor else {
            return ManipOutput::default();
        };
        // Attach to the body under the cursor, if any (offset in its frame).
        let attachment = match world
            .topmost_body_at(p)
            .and_then(|e| Some((world.id_of(e)?, world.pose_of(e)?)))
        {
            Some((id, pose)) => {
                let (id, pose): (StableId, PosRot) = (id, pose);
                NodeAttachment::to(id, Vec2::from_angle(-pose.rot).rotate(p - pose.pos))
            }
            None => NodeAttachment::free(),
        };

        let id = StableId::new();
        let record = NodeRecord {
            id,
            pose: PosRot { pos: p, rot: 0.0 },
            attachment,
            kind: NodeKind::Tracer(Tracer::default()),
            appearance: appearance_for_id(id),
        };
        ManipOutput::commit(ToolCommit::SpawnNode(Box::new(record)))
    }

    fn drafting(&self) -> bool {
        false
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        // A ghost ring at the cursor, tinted when it would attach to a body.
        if let Some(p) = ctx.cursor {
            out.circle(p, 6.0, css::AQUAMARINE);
        }
    }
}

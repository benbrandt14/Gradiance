//! The placeable **tracer** tool — the one remaining behavior-node tool.
//!
//! One-click placement: click a body to attach (the trail rides it), click
//! empty space for a free node. Sensors and actuators are **not** tools —
//! they are ports on objects (read via `SignalSource`, written via
//! `SignalSink`), wired in the node editor — so only the tracer places a
//! [`NodeKind`] entity, through the shared [`ToolCommit::SpawnNode`] seam.

use crate::selection::Selection;
use crate::tools::appearance_for_id;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_domain::tracer::Tracer;
use gradiance_scene::NodeRecord;

/// Places a node of `kind` on release: attached to the body under the
/// cursor (offset in its frame) or free. One click commits one `SpawnNode`.
fn place_node(ctx: &ManipContext, world: &ToolWorld, kind: NodeKind) -> ManipOutput {
    if ctx.left != GesturePhase::Released {
        return ManipOutput::default();
    }
    let Some(p) = ctx.cursor else {
        return ManipOutput::default();
    };
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
        kind,
        appearance: appearance_for_id(id),
    };
    ManipOutput::commit(ToolCommit::SpawnNode(Box::new(record)))
}

/// Ghost ring at the cursor — shared node-placement preview.
fn ghost(ctx: &ManipContext, out: &mut ToolPreview) {
    if let Some(p) = ctx.cursor {
        out.circle(p, 6.0, css::AQUAMARINE);
    }
}

macro_rules! node_tool {
    ($ty:ident, $doc:literal, $make:expr) => {
        #[doc = $doc]
        #[derive(Resource, Default, Debug)]
        pub struct $ty;

        impl ManipTool for $ty {
            fn update(
                &mut self,
                ctx: &ManipContext,
                world: &ToolWorld,
                _: &Selection,
            ) -> ManipOutput {
                let make: fn() -> NodeKind = $make;
                place_node(ctx, world, make())
            }
            fn drafting(&self) -> bool {
                false
            }
            fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
                ghost(ctx, out);
            }
        }
    };
}

node_tool!(
    TracerTool,
    "Places tracer nodes (trajectory trails).",
    || { NodeKind::Tracer(Tracer::default()) }
);

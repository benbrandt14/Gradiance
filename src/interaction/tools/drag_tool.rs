//! Drag tool: physically fling bodies with a mouse spring.
//!
//! While playing, grabbing pulls the body by the grabbed point (a velocity
//! spring — no command, physical interaction is not undoable, matching
//! Algodoo). While paused, dragging behaves like a move: the body is held
//! kinematically and one transform command commits on release.
//!
//! Both interactions are transient preview state the driver applies from the
//! returned [`ManipOutput`] (spring / kinematic `Transform`); the tool itself
//! only reads the world and, on release, emits a [`ToolCommit::Move`].

use crate::command::intent::TransformChange;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::interaction::tools::context::{
    GesturePhase, GrabState, HoldState, ManipContext, ManipOutput, ManipTool, ToolCommit,
    ToolPreview, ToolWorld,
};
use crate::physics::grab::Grab;
use bevy::prelude::*;

/// Minimum move distance (world px) before a paused drag commits.
const MOVE_EPSILON: f32 = 0.5;

/// The drag tool's gesture state.
#[derive(Resource, Default, Debug)]
pub struct DragTool {
    /// Paused-mode drag: `(entity, id, original pose, grab offset)`.
    paused: Option<(Entity, StableId, PosRot, Vec2)>,
    /// Play-mode spring grab (mirrors `MouseSpring` while a fling is active).
    grab: Option<Grab>,
}

impl ManipTool for DragTool {
    fn update(&mut self, ctx: &ManipContext, world: &ToolWorld) -> ManipOutput {
        let mut out = ManipOutput::default();

        // Press: grab the topmost body at the *raw* cursor (grabs land where
        // you click; snapping is for placement, not flinging).
        if ctx.left == GesturePhase::Pressed
            && let Some(p) = ctx.raw_cursor
            && let Some(hit) = world.topmost_body_at(p)
            && let Some(pose) = world.pose_of(hit)
        {
            let local = Vec2::from_angle(-pose.rot).rotate(p - pose.pos);
            if ctx.playing {
                self.grab = Some(Grab {
                    entity: hit,
                    local_point: local,
                    target: p,
                });
            } else if let Some(id) = world.id_of(hit) {
                self.paused = Some((hit, id, pose, p - pose.pos));
            }
        }

        // While the button is down (press frame included): drive the spring
        // target / move the held body.
        if matches!(ctx.left, GesturePhase::Pressed | GesturePhase::Held) {
            if let Some(grab) = &mut self.grab {
                if let Some(p) = ctx.raw_cursor {
                    grab.target = p;
                }
                out.grab = GrabState::Set(*grab);
            }
            if let Some((entity, _, original, offset)) = self.paused
                && let Some(p) = ctx.cursor
            {
                out.hold = HoldState::Set(vec![(
                    entity,
                    PosRot {
                        pos: p - offset,
                        rot: original.rot,
                    },
                )]);
            }
        }

        // Release: drop the spring, release the hold, and commit the move.
        if ctx.left == GesturePhase::Released {
            out.grab = GrabState::Clear;
            self.grab = None;
            if let Some((entity, id, old, _)) = self.paused.take() {
                out.hold = HoldState::Clear;
                if let Some(new) = world.pose_of(entity)
                    && new.pos.distance(old.pos) > MOVE_EPSILON
                {
                    out.commit = Some(ToolCommit::Move(vec![TransformChange { id, old, new }]));
                }
            }
        }

        out
    }

    fn drafting(&self) -> bool {
        self.paused.is_some() || self.grab.is_some()
    }

    fn preview(&self, _ctx: &ManipContext, _out: &mut ToolPreview) {
        // The drag tool draws no gizmo — the body itself is the preview.
    }
}

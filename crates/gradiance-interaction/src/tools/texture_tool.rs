//! Texture tool (brief S-13): place the image on a body's front face.
//!
//! - Left-drag on an image moves it.
//! - Shift + left-drag scales it about its centre: the width follows the
//!   cursor's distance from the centre. (The wheel zooms the canvas in
//!   every tool, so the scale is a drag; decision `texture-tool-scale`.)
//! - Right-drag turns it about its centre.
//!
//! The gesture previews an outline and commits one `body.place_texture`
//! on release: one undo step per gesture.

use crate::selection::Selection;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::texture::{BodyTexture, MIN_TEXTURE_WIDTH};
use gradiance_domain::theme::Role;
use gradiance_units::{Angle, Displacement, Length};

/// What the gesture changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// The image follows the cursor.
    Move,
    /// The width follows the cursor's distance from the centre.
    Scale,
    /// The image turns with the cursor about its centre.
    Turn,
}

/// One gesture on one body's image.
#[derive(Debug, Clone)]
struct Gesture {
    change: Change,
    id: StableId,
    pose: PosRot,
    start: BodyTexture,
    grab: Vec2,
    now: BodyTexture,
}

impl Gesture {
    /// The image centre in the world.
    fn centre(&self, texture: &BodyTexture) -> Vec2 {
        self.pose.pos + Vec2::from_angle(self.pose.rot).rotate(texture.offset.0)
    }

    /// The placement for the cursor at `p`.
    fn at(&self, p: Vec2) -> BodyTexture {
        let centre = self.centre(&self.start);
        match self.change {
            Change::Move => BodyTexture {
                offset: Displacement(
                    self.start.offset.0 + Vec2::from_angle(-self.pose.rot).rotate(p - self.grab),
                ),
                ..self.start.clone()
            },
            Change::Scale => {
                let from = (self.grab - centre).length().max(1e-4);
                let width =
                    (self.start.width.0 * (p - centre).length() / from).max(MIN_TEXTURE_WIDTH);
                BodyTexture {
                    width: Length(width),
                    ..self.start.clone()
                }
            }
            Change::Turn => {
                let turn = (p - centre).to_angle() - (self.grab - centre).to_angle();
                BodyTexture {
                    angle: Angle(self.start.angle.0 + turn),
                    ..self.start.clone()
                }
            }
        }
    }
}

/// The texture tool's gesture, if one is in progress.
#[derive(Resource, Default, Debug)]
pub struct TextureTool(Option<Gesture>);

impl ManipTool for TextureTool {
    fn update(&mut self, ctx: &ManipContext, world: &ToolWorld, _: &Selection) -> ManipOutput {
        let Some(cursor) = ctx.cursor else {
            return ManipOutput::default();
        };
        let change = match (ctx.left, ctx.right) {
            (GesturePhase::Pressed, _) if ctx.shift => Some(Change::Scale),
            (GesturePhase::Pressed, _) => Some(Change::Move),
            (_, GesturePhase::Pressed) => Some(Change::Turn),
            _ => None,
        };
        if let Some(change) = change
            && self.0.is_none()
            && !ctx.over_ui
            && let Some((id, pose, texture)) = world.texture_at(cursor)
        {
            self.0 = Some(Gesture {
                change,
                id,
                pose,
                start: texture.clone(),
                grab: cursor,
                now: texture,
            });
            return ManipOutput::default();
        }
        let Some(gesture) = self.0.as_mut() else {
            return ManipOutput::default();
        };
        gesture.now = gesture.at(cursor);
        let released = match gesture.change {
            Change::Turn => ctx.right == GesturePhase::Released,
            Change::Move | Change::Scale => ctx.left == GesturePhase::Released,
        };
        if !released {
            return ManipOutput::default();
        }
        let Some(gesture) = self.0.take() else {
            return ManipOutput::default();
        };
        if gesture.now == gesture.start {
            return ManipOutput::default();
        }
        ManipOutput::commit(ToolCommit::Texture {
            id: gesture.id,
            texture: gesture.now,
        })
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, _: &ManipContext, out: &mut ToolPreview) {
        let Some(gesture) = &self.0 else {
            return;
        };
        // The image's width as a circle about its centre, and its +x axis.
        let t = &gesture.now;
        let centre = gesture.centre(t);
        let half = 0.5 * t.width.0;
        out.circle(centre, half, Role::Preview);
        let axis = Vec2::from_angle(gesture.pose.rot + t.angle.0);
        out.line(centre - axis * half, centre + axis * half, Role::Preview);
    }
}

/// The `body.place_texture` invocation for a texture commit.
pub fn place_invocation(id: StableId, texture: &BodyTexture) -> gradiance_op::InvokeOp {
    gradiance_op::InvokeOp::with(
        "body.place_texture",
        gradiance_op::json!({
            "i": id.0.to_string(),
            "x": texture.offset.0.x,
            "y": texture.offset.0.y,
            "width": texture.width.0,
            "angle": texture.angle.0,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gesture(change: Change) -> Gesture {
        let start = BodyTexture::new("a.png", 1.0);
        Gesture {
            change,
            id: StableId::new(),
            // A body at (2, 0), a quarter turn.
            pose: PosRot {
                pos: Vec2::new(2.0, 0.0),
                rot: core::f32::consts::FRAC_PI_2,
            },
            start: start.clone(),
            grab: Vec2::new(2.5, 0.0),
            now: start,
        }
    }

    #[test]
    fn a_move_is_in_the_body_frame_and_scale_and_turn_are_about_the_centre() {
        // World +y on a quarter-turned body is its local +x.
        let moved = gesture(Change::Move).at(Vec2::new(2.5, 1.0));
        assert!(moved.offset.0.abs_diff_eq(Vec2::new(1.0, 0.0), 1e-5));
        let scaled = gesture(Change::Scale).at(Vec2::new(3.0, 0.0));
        assert!(
            (scaled.width.0 - 2.0).abs() < 1e-5,
            "twice as far: twice as wide"
        );
        let turned = gesture(Change::Turn).at(Vec2::new(2.0, 0.5));
        assert!((turned.angle.0 - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let call = place_invocation(StableId::new(), &scaled);
        assert_eq!(call.id, "body.place_texture");
        assert_eq!(call.params["width"], 2.0);
    }
}

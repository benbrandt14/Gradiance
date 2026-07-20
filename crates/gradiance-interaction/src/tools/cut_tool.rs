//! Cut tool: drag a stroke; every body it crosses is CSG-subtracted
//! (severed bodies split into independent pieces).

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Stroke width in *screen* pixels (scaled by zoom at commit, so a cut
/// always looks and behaves like a thin slit at any magnification).
const CUT_SCREEN_WIDTH: f32 = 3.0;

/// Minimum stroke length (world px) to commit a cut.
const MIN_STROKE: f32 = 1.0;

/// In-progress cut stroke (anchor point).
#[derive(Resource, Default, Debug)]
pub struct CutTool(pub Option<Vec2>);

impl DraftTool for CutTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let anchor = self.0.take()?;
                let p = ctx.cursor?;
                (anchor.distance(p) >= MIN_STROKE).then_some(ToolCommit::Cut {
                    a: anchor,
                    b: p,
                    width: CUT_SCREEN_WIDTH * ctx.cam_scale,
                })
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if let (Some(anchor), Some(p)) = (self.0, ctx.cursor) {
            out.line(anchor, p, css::ORANGE_RED);
        }
    }
}

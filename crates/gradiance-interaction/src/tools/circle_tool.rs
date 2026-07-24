//! Circle creation tool: press at the center, drag out the radius.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_domain::shape::ShapeDef;

/// Minimum accepted radius, world metres (~0.5 px on screen). The SI flip
/// left this at its pixel-era `0.5`, which demanded a 0.5 m (50 px) minimum
/// radius and silently swallowed small draws.
const MIN_RADIUS: f32 = 0.005;

/// In-progress circle draft (center).
#[derive(Resource, Default, Debug)]
pub struct CircleTool(pub Option<Vec2>);

impl DraftTool for CircleTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let center = self.0.take()?;
                let p = ctx.cursor?;
                let radius = center.distance(p);
                (radius >= MIN_RADIUS).then(|| {
                    ToolCommit::SpawnBody(Box::new(new_body_record(
                        ShapeDef::Circle { radius },
                        center,
                        0.0,
                    )))
                })
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if let (Some(center), Some(p)) = (self.0, ctx.cursor) {
            out.circle(center, center.distance(p).max(0.1), css::AQUAMARINE);
            out.line(center, p, css::AQUAMARINE.with_alpha(0.5));
        }
    }
}

//! Water tool (brief S-11): drag out a rectangle of water.
//!
//! The release invokes `water.fill` with the rectangle as its outline, like
//! the gear tool invokes its operation: one lowering for the tool and the
//! verb. The water is a saved emitter; its particles fill when the run plays.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use bevy::prelude::*;
use gradiance_domain::theme::Role;

/// Smallest accepted side, world metres. Below it the region holds too few
/// particles to show.
const MIN_SIDE: f32 = 0.1;

/// In-progress water draft (the press point).
#[derive(Resource, Default, Debug)]
pub struct WaterTool(pub Option<Vec2>);

impl DraftTool for WaterTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let a = self.0.take()?;
                let b = ctx.cursor?;
                let (min, max) = (a.min(b), a.max(b));
                let size = max - min;
                (size.min_element() >= MIN_SIDE).then_some(ToolCommit::Water { min, max })
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if let (Some(a), Some(b)) = (self.0, ctx.cursor) {
            let (min, max) = (a.min(b), a.max(b));
            out.polyline(rectangle(min, max).to_vec(), Role::Preview);
        }
    }
}

/// The closed rectangle `min`..`max`, counter-clockwise, first point repeated.
fn rectangle(min: Vec2, max: Vec2) -> [Vec2; 5] {
    [
        min,
        Vec2::new(max.x, min.y),
        max,
        Vec2::new(min.x, max.y),
        min,
    ]
}

/// The `water.fill` invocation for a water commit.
pub fn water_invocation(min: Vec2, max: Vec2) -> gradiance_op::InvokeOp {
    let path: Vec<f32> = rectangle(min, max)[..4]
        .iter()
        .flat_map(|p| [p.x, p.y])
        .collect();
    gradiance_op::InvokeOp::with("water.fill", gradiance_op::json!({ "path": path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_region_is_its_rectangle() {
        let call = water_invocation(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 2.0));
        assert_eq!(call.id, "water.fill");
        assert_eq!(
            call.params["path"],
            gradiance_op::json!([-1.0, 0.0, 1.0, 0.0, 1.0, 2.0, -1.0, 2.0])
        );
    }
}

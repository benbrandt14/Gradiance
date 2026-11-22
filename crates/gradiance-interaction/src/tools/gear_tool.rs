//! Gear tool (brief S-5): press at the centre, drag out the pitch radius.
//!
//! The release invokes `body.spawn_gear` with the tooth count the radius
//! gives at [`gear::DEFAULT_MODULE`], so every drawn gear has teeth of one
//! size and any two of them mesh. Through the operation, like the sketch and
//! the chain: one lowering for the tool and the verb, and Adjust Last
//! Operation can change the radius and the teeth afterwards.

use crate::snap::DraftReference;
use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use bevy::prelude::*;
use gradiance_domain::theme::Role;
use gradiance_geometry::gear;

/// Smallest accepted pitch radius, world metres. Below it the teeth are too
/// small for the contour to resolve.
const MIN_RADIUS: f32 = 0.05;

/// In-progress gear draft (centre).
#[derive(Resource, Default, Debug)]
pub struct GearTool(pub Option<Vec2>);

impl DraftTool for GearTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let center = self.0.take()?;
                let radius = center.distance(ctx.cursor?);
                (radius >= MIN_RADIUS).then_some(ToolCommit::Gear { center, radius })
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    /// While the radius is dragged, the snapper offers the radius that just
    /// touches a nearby outline, as for a circle.
    fn snap_reference(&self) -> Option<DraftReference> {
        self.0.map(DraftReference::Radius)
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if let (Some(center), Some(p)) = (self.0, ctx.cursor) {
            let radius = center.distance(p).max(MIN_RADIUS);
            let outline = gear::gear_outline(radius, gear::default_teeth(radius));
            let mut ring: Vec<Vec2> = outline.iter().map(|v| center + *v).collect();
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            out.polyline(ring, Role::Preview);
            out.line(center, p, Role::Preview.alpha(0.5));
        }
    }
}

/// The `body.spawn_gear` invocation for a gear commit.
pub fn gear_invocation(center: Vec2, radius: f32) -> gradiance_op::InvokeOp {
    gradiance_op::InvokeOp::with(
        "body.spawn_gear",
        gradiance_op::json!({
            "x": center.x,
            "y": center.y,
            "r": radius,
            "teeth": gear::default_teeth(radius),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_gear_gets_the_default_tooth_size() {
        let call = gear_invocation(Vec2::new(1.0, 2.0), 1.2);
        assert_eq!(call.id, "body.spawn_gear");
        assert_eq!(call.params["teeth"], 24);
        assert_eq!(call.params["x"], 1.0);
    }
}

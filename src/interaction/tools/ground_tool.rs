//! Ground tool: place an infinite half-plane; drag sets its tilt.

use crate::domain::shape::ShapeDef;
use crate::interaction::tools::context::{
    DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview,
};
use crate::interaction::tools::new_body_record;
use avian2d::prelude::RigidBody;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Below this drag length the ground is horizontal.
const FLAT_THRESHOLD: f32 = 5.0;

/// In-progress ground draft (surface anchor point).
#[derive(Resource, Default, Debug)]
pub struct GroundTool(pub Option<Vec2>);

impl DraftTool for GroundTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let anchor = self.0.take()?;
                let p = ctx.cursor?;
                let drag = p - anchor;
                let rot = if drag.length() < FLAT_THRESHOLD {
                    0.0
                } else {
                    // Ctrl quantizes the tilt like any rotation gesture.
                    ctx.constraints.apply_rotation(drag.to_angle(), ctx.snap)
                };
                let mut record = new_body_record(ShapeDef::HalfPlane, anchor, rot);
                record.physics.rigid_body = RigidBody::Static;
                record.appearance.fill = crate::domain::appearance::Rgba::rgb(0.35, 0.4, 0.35);
                Some(ToolCommit::SpawnBody(Box::new(record)))
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    fn drafting(&self) -> bool {
        self.0.is_some()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if let (Some(anchor), Some(p)) = (self.0, ctx.cursor) {
            let drag = p - anchor;
            let dir = if drag.length() < FLAT_THRESHOLD {
                Vec2::X
            } else {
                drag.normalize()
            };
            out.line(anchor - dir * 10_000.0, anchor + dir * 10_000.0, css::OLIVE);
            // Hatch the solid side (local −Y of the surface).
            let down = -dir.perp();
            for i in -5..=5 {
                let base = anchor + dir * (i as f32 * 40.0);
                out.line(base, base + (down - dir) * 15.0, css::OLIVE.with_alpha(0.6));
            }
        }
    }
}

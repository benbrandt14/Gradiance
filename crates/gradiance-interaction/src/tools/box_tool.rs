//! Box creation tool: drag out a rectangle.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_domain::shape::ShapeDef;

/// Minimum accepted box side, world pixels.
const MIN_SIDE: f32 = 1.0;

/// In-progress box draft (anchor corner).
#[derive(Resource, Default, Debug)]
pub struct BoxTool(pub Option<Vec2>);

impl DraftTool for BoxTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor;
                None
            }
            GesturePhase::Released => {
                let anchor = self.0.take()?;
                let p = ctx.cursor?;
                let size = (p - anchor).abs();
                (size.x >= MIN_SIDE && size.y >= MIN_SIDE).then(|| {
                    ToolCommit::SpawnBody(Box::new(new_body_record(
                        ShapeDef::Box {
                            width: size.x,
                            height: size.y,
                        },
                        (anchor + p) / 2.0,
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
        if let (Some(anchor), Some(p)) = (self.0, ctx.cursor) {
            out.rect((anchor + p) / 2.0, (p - anchor).abs(), css::AQUAMARINE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::GestureConstraints;
    use crate::tools::context::ToolCommit;
    use gradiance_domain::settings::SnapConfig;

    /// Builds a context at a phase/cursor with default constraints — the
    /// same shape a scripted tool would receive (no ECS involved).
    fn ctx<'a>(
        phase: GesturePhase,
        cursor: Vec2,
        constraints: &'a GestureConstraints,
        snap: &'a SnapConfig,
    ) -> ToolContext<'a> {
        ToolContext {
            phase,
            cursor: Some(cursor),
            raw_cursor: Some(cursor),
            over_ui: false,
            confirm: false,
            cancel: false,
            constraints,
            snap,
            cam_scale: 1.0,
        }
    }

    #[test]
    fn press_then_release_commits_one_centered_box() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();

        assert!(!tool.drafting(), "starts idle");
        assert!(
            tool.update(&ctx(GesturePhase::Pressed, Vec2::ZERO, &constraints, &snap))
                .is_none(),
            "press does not commit"
        );
        assert!(tool.drafting(), "drafting after press");

        let commit = tool.update(&ctx(
            GesturePhase::Released,
            Vec2::new(60.0, 40.0),
            &constraints,
            &snap,
        ));
        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("release commits a SpawnBody, got {commit:?}");
        };
        assert_eq!(
            record.shape,
            ShapeDef::Box {
                width: 60.0,
                height: 40.0
            }
        );
        assert!((record.pose.pos - Vec2::new(30.0, 20.0)).length() < 1e-6);
        assert!(!tool.drafting(), "draft cleared on commit");
    }

    #[test]
    fn a_zero_area_drag_commits_nothing() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        tool.update(&ctx(GesturePhase::Pressed, Vec2::ZERO, &constraints, &snap));
        let commit = tool.update(&ctx(
            GesturePhase::Released,
            Vec2::ZERO,
            &constraints,
            &snap,
        ));
        assert!(commit.is_none(), "sub-minimum box is rejected");
        assert!(!tool.drafting());
    }
}

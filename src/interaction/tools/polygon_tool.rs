//! Polygon creation tool: click vertices, close near the start (or with
//! Enter); Escape cancels.

use crate::domain::shape::ShapeDef;
use crate::geometry::contours::{ring_centroid, ring_signed_area};
use crate::interaction::tools::context::{
    DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview,
};
use crate::interaction::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Clicking within this distance of the first vertex closes the loop.
const CLOSE_RADIUS: f32 = 8.0;

/// In-progress polygon vertices (world space).
#[derive(Resource, Default, Debug)]
pub struct PolygonTool(pub Vec<Vec2>);

impl PolygonTool {
    /// Converts the draft into a centroid-relative CCW polygon commit,
    /// clearing the draft. Returns `None` (but still clears) if the loop is
    /// degenerate.
    fn finish(&mut self) -> Option<ToolCommit> {
        let points = std::mem::take(&mut self.0);
        if points.len() < 3 {
            return None;
        }
        let centroid = ring_centroid(&points);
        let mut outline: Vec<Vec2> = points.iter().map(|p| *p - centroid).collect();
        if ring_signed_area(&outline) < 0.0 {
            outline.reverse();
        }
        let shape = ShapeDef::Polygon {
            outline,
            holes: vec![],
        };
        shape
            .validate()
            .is_ok()
            .then(|| ToolCommit::SpawnBody(Box::new(new_body_record(shape, centroid, 0.0))))
    }
}

impl DraftTool for PolygonTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if ctx.cancel {
            self.0.clear();
            return None;
        }
        if ctx.confirm && self.0.len() >= 3 {
            return self.finish();
        }
        if ctx.phase == GesturePhase::Pressed
            && let Some(p) = ctx.cursor
        {
            let closes = self.0.len() >= 3
                && self
                    .0
                    .first()
                    .is_some_and(|f| f.distance(p) <= CLOSE_RADIUS);
            if closes {
                return self.finish();
            }
            self.0.push(p);
        }
        None
    }

    fn drafting(&self) -> bool {
        !self.0.is_empty()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if self.0.is_empty() {
            return;
        }
        out.polyline(self.0.clone(), css::AQUAMARINE);
        if let (Some(last), Some(p)) = (self.0.last().copied(), ctx.cursor) {
            out.line(last, p, css::AQUAMARINE.with_alpha(0.5));
        }
        if let Some(first) = self.0.first().copied() {
            out.circle(first, CLOSE_RADIUS, css::GOLD);
        }
    }
}

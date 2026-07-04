//! Polygon creation tool: click vertices, close near the start (or with
//! Enter); Escape cancels.

use crate::command::intent::SpawnBodyIntent;
use crate::domain::shape::ShapeDef;
use crate::geometry::contours::{ring_centroid, ring_signed_area};
use crate::interaction::PointerOverUi;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, new_body_record};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Clicking within this distance of the first vertex closes the loop.
const CLOSE_RADIUS: f32 = 8.0;

/// In-progress polygon vertices (world space).
#[derive(Resource, Default, Debug)]
pub struct PolygonDraft(pub Vec<Vec2>);

/// Click-out-a-polygon state machine.
pub fn run_polygon_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    keys: Res<ButtonInput<KeyCode>>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    mut draft: ResMut<PolygonDraft>,
    mut active: ResMut<ActiveGesture>,
    mut spawn: MessageWriter<SpawnBodyIntent>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        draft.0.clear();
        active.0 = false;
        return;
    }
    let close_requested = keys.just_pressed(KeyCode::Enter) && draft.0.len() >= 3;

    if buttons.just_pressed(MouseButton::Left)
        && !over_ui.0
        && let Some(p) = snapped.effective()
    {
        let closes = draft.0.len() >= 3
            && draft
                .0
                .first()
                .is_some_and(|first| first.distance(p) <= CLOSE_RADIUS);
        if closes {
            finish(&mut draft, &mut spawn);
            active.0 = false;
            return;
        }
        draft.0.push(p);
        active.0 = true;
    }

    if close_requested {
        finish(&mut draft, &mut spawn);
        active.0 = false;
    }
}

/// Converts the draft into a centroid-relative CCW polygon and spawns it.
fn finish(draft: &mut PolygonDraft, spawn: &mut MessageWriter<SpawnBodyIntent>) {
    let points = std::mem::take(&mut draft.0);
    if points.len() < 3 {
        return;
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
    if shape.validate().is_ok() {
        spawn.write(SpawnBodyIntent {
            record: new_body_record(shape, centroid, 0.0),
        });
    }
}

/// Outline-so-far plus a rubber segment to the cursor.
pub fn draw_polygon_preview(
    draft: Res<PolygonDraft>,
    snapped: Res<SnappedCursor>,
    mut gizmos: Gizmos,
) {
    if draft.0.is_empty() {
        return;
    }
    gizmos.linestrip_2d(draft.0.iter().copied(), css::AQUAMARINE);
    if let (Some(last), Some(p)) = (draft.0.last().copied(), snapped.effective()) {
        gizmos.line_2d(last, p, css::AQUAMARINE.with_alpha(0.5));
    }
    if let Some(first) = draft.0.first().copied() {
        gizmos.circle_2d(Isometry2d::from_translation(first), CLOSE_RADIUS, css::GOLD);
    }
}

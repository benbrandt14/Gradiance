//! Circle creation tool: press at the center, drag out the radius.

use crate::command::intent::SpawnBodyIntent;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, new_body_record};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Minimum accepted radius, world pixels.
const MIN_RADIUS: f32 = 0.5;

/// In-progress circle draft (center).
#[derive(Resource, Default, Debug)]
pub struct CircleDraft(pub Option<Vec2>);

/// Center-then-radius state machine.
pub fn run_circle_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    mut draft: ResMut<CircleDraft>,
    mut active: ResMut<ActiveGesture>,
    mut spawn: MessageWriter<SpawnBodyIntent>,
) {
    if buttons.just_pressed(MouseButton::Left)
        && !over_ui.0
        && let Some(p) = snapped.effective()
    {
        draft.0 = Some(p);
        active.0 = true;
    }
    if buttons.just_released(MouseButton::Left)
        && let Some(center) = draft.0.take()
    {
        active.0 = false;
        if let Some(p) = snapped.effective() {
            let radius = center.distance(p);
            if radius >= MIN_RADIUS {
                spawn.write(SpawnBodyIntent {
                    record: new_body_record(ShapeDef::Circle { radius }, center, 0.0),
                });
            }
        }
    }
}

/// Radius preview.
pub fn draw_circle_preview(
    draft: Res<CircleDraft>,
    snapped: Res<SnappedCursor>,
    mut gizmos: Gizmos,
) {
    if let (Some(center), Some(p)) = (draft.0, snapped.effective()) {
        gizmos.circle_2d(
            Isometry2d::from_translation(center),
            center.distance(p).max(0.1),
            css::AQUAMARINE,
        );
        gizmos.line_2d(center, p, css::AQUAMARINE.with_alpha(0.5));
    }
}

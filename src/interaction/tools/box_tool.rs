//! Box creation tool: drag out a rectangle.

use crate::command::intent::SpawnBodyIntent;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, new_body_record};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Minimum accepted box side, world pixels.
const MIN_SIDE: f32 = 1.0;

/// In-progress box draft (anchor corner).
#[derive(Resource, Default, Debug)]
pub struct BoxDraft(pub Option<Vec2>);

/// Drag-out-a-box state machine.
pub fn run_box_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    mut draft: ResMut<BoxDraft>,
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
        && let Some(anchor) = draft.0.take()
    {
        active.0 = false;
        if let Some(p) = snapped.effective() {
            let size = (p - anchor).abs();
            if size.x >= MIN_SIDE && size.y >= MIN_SIDE {
                spawn.write(SpawnBodyIntent {
                    record: new_body_record(
                        ShapeDef::Box {
                            width: size.x,
                            height: size.y,
                        },
                        (anchor + p) / 2.0,
                        0.0,
                    ),
                });
            }
        }
    }
}

/// Rubber-band preview of the box being drawn.
pub fn draw_box_preview(draft: Res<BoxDraft>, snapped: Res<SnappedCursor>, mut gizmos: Gizmos) {
    if let (Some(anchor), Some(p)) = (draft.0, snapped.effective()) {
        gizmos.rect_2d(
            Isometry2d::from_translation((anchor + p) / 2.0),
            (p - anchor).abs(),
            css::AQUAMARINE,
        );
    }
}

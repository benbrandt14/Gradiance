//! Cut tool: drag a stroke; every body it crosses is CSG-subtracted
//! (severed bodies split into independent pieces).

use crate::command::intent::CutIntent;
use crate::interaction::PointerOverUi;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::ActiveGesture;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Stroke width in *screen* pixels (scaled by zoom at commit, so a cut
/// always looks and behaves like a thin slit at any magnification).
const CUT_SCREEN_WIDTH: f32 = 3.0;

/// Minimum stroke length (world px) to commit a cut.
const MIN_STROKE: f32 = 1.0;

/// In-progress cut stroke (anchor point).
#[derive(Resource, Default, Debug)]
pub struct CutDraft(pub Option<Vec2>);

/// Drag-a-stroke state machine: press anchors, release commits one
/// [`CutIntent`].
pub fn run_cut_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    projections: Query<&Projection, With<Camera3d>>,
    mut draft: ResMut<CutDraft>,
    mut active: ResMut<ActiveGesture>,
    mut cuts: MessageWriter<CutIntent>,
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
        if let Some(p) = snapped.effective()
            && anchor.distance(p) >= MIN_STROKE
        {
            let scale = crate::interaction::camera::camera_scale(&projections);
            cuts.write(CutIntent {
                a: anchor,
                b: p,
                width: CUT_SCREEN_WIDTH * scale,
            });
        }
    }
}

/// Stroke preview while dragging.
pub fn draw_cut_preview(draft: Res<CutDraft>, snapped: Res<SnappedCursor>, mut gizmos: Gizmos) {
    if let (Some(anchor), Some(p)) = (draft.0, snapped.effective()) {
        gizmos.line_2d(anchor, p, css::ORANGE_RED);
    }
}

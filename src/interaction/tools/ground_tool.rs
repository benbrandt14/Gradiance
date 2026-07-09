//! Ground tool: place an infinite half-plane; drag sets its tilt.

use crate::command::intent::SpawnBodyIntent;
use crate::domain::shape::ShapeDef;
use avian2d::prelude::RigidBody;
use crate::interaction::PointerOverUi;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, new_body_record};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Below this drag length the ground is horizontal.
const FLAT_THRESHOLD: f32 = 5.0;

/// In-progress ground draft (surface anchor point).
#[derive(Resource, Default, Debug)]
pub struct GroundDraft(pub Option<Vec2>);

/// Click places a horizontal ground; click-drag tilts the surface along
/// the drag direction. Grounds are static and truly infinite (half-space
/// collider).
pub fn run_ground_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    constraints: Res<crate::interaction::gesture::GestureConstraints>,
    config: Res<crate::domain::settings::SnapConfig>,
    mut draft: ResMut<GroundDraft>,
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
            let drag = p - anchor;
            let rot = if drag.length() < FLAT_THRESHOLD {
                0.0
            } else {
                // Ctrl quantizes the tilt like any rotation gesture.
                constraints.apply_rotation(drag.to_angle(), &config)
            };
            let mut record = new_body_record(ShapeDef::HalfPlane, anchor, rot);
            record.physics.rigid_body = RigidBody::Static;
            record.appearance.fill = crate::domain::appearance::Rgba::rgb(0.35, 0.4, 0.35);
            spawn.write(SpawnBodyIntent { record });
        }
    }
}

/// Surface-line preview while dragging.
pub fn draw_ground_preview(
    draft: Res<GroundDraft>,
    snapped: Res<SnappedCursor>,
    mut gizmos: Gizmos,
) {
    if let (Some(anchor), Some(p)) = (draft.0, snapped.effective()) {
        let drag = p - anchor;
        let dir = if drag.length() < FLAT_THRESHOLD {
            Vec2::X
        } else {
            drag.normalize()
        };
        gizmos.line_2d(anchor - dir * 10_000.0, anchor + dir * 10_000.0, css::OLIVE);
        // Hatch the solid side (local −Y of the surface).
        let down = -dir.perp();
        for i in -5..=5 {
            let base = anchor + dir * (i as f32 * 40.0);
            gizmos.line_2d(base, base + (down - dir) * 15.0, css::OLIVE.with_alpha(0.6));
        }
    }
}

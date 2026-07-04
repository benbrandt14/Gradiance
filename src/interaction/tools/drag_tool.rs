//! Drag tool: physically fling bodies with a mouse spring.
//!
//! While playing, grabbing pulls the body by the grabbed point (a
//! velocity spring — no command, physical interaction is not undoable,
//! matching Algodoo). While paused, dragging behaves like a move: the
//! body is held kinematically and one transform command commits on
//! release.

use crate::command::intent::{CommitTransformIntent, TransformChange};
use crate::core::ids::StableId;
use crate::core::states::GameState;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::cursor::CursorWorldPos;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, topmost_body_at};
use crate::physics::grab::{Grab, MouseSpring};
use crate::physics::hold::KinematicHold;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;

/// Paused-mode drag state.
#[derive(Default, Debug)]
pub struct PausedDrag {
    body: Option<(Entity, StableId, PosRot, Vec2 /* grab offset */)>,
}

/// The drag tool state machine.
pub fn run_drag_tool(
    buttons: Res<crate::interaction::pointer::PointerButtons>,
    cursor: Res<CursorWorldPos>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    state: Res<State<GameState>>,
    physics: PhysicsQueries,
    hit_query: Query<(&ShapeDef, &LayerMask32), With<Body>>,
    ids: Query<&StableId>,
    mut transforms: Query<&mut Transform, With<Body>>,
    mut spring: ResMut<MouseSpring>,
    mut hold: ResMut<KinematicHold>,
    mut exclusions: ResMut<crate::interaction::snap::SnapExclusions>,
    mut paused_drag: Local<PausedDrag>,
    mut active: ResMut<ActiveGesture>,
    mut commit: MessageWriter<CommitTransformIntent>,
) {
    let playing = *state.get() == GameState::Playing;

    if buttons.just_pressed(MouseButton::Left)
        && !over_ui.0
        && let Some(p) = cursor.0
        && let Some(hit) = topmost_body_at(p, &physics, &hit_query)
        && let Ok(transform) = transforms.get_mut(hit)
    {
        {
            let pose = PosRot::from_transform(&transform);
            let local = Vec2::from_angle(-pose.rot).rotate(p - pose.pos);
            if playing {
                spring.0 = Some(Grab {
                    entity: hit,
                    local_point: local,
                    target: p,
                });
            } else if let Ok(id) = ids.get(hit) {
                hold.entities = vec![hit];
                exclusions.0 = vec![hit];
                paused_drag.body = Some((hit, *id, pose, p - pose.pos));
            }
            active.0 = true;
        }
    }

    // Update targets while held.
    if buttons.pressed(MouseButton::Left) {
        if let Some(grab) = &mut spring.0
            && let Some(p) = cursor.0
        {
            grab.target = p;
        }
        if let Some((entity, _, original, offset)) = paused_drag.body
            && let Some(p) = snapped.effective()
            && let Ok(mut transform) = transforms.get_mut(entity)
        {
            PosRot {
                pos: p - offset,
                rot: original.rot,
            }
            .apply_to(&mut transform);
        }
    }

    if buttons.just_released(MouseButton::Left) {
        spring.0 = None;
        active.0 = false;
        if let Some((entity, id, old, _)) = paused_drag.body.take() {
            hold.entities.clear();
            exclusions.0.clear();
            if let Ok(transform) = transforms.get(entity) {
                let new = PosRot::from_transform(transform);
                if new.pos.distance(old.pos) > 0.5 {
                    commit.write(CommitTransformIntent {
                        changes: vec![TransformChange { id, old, new }],
                    });
                }
            }
        }
    }
}

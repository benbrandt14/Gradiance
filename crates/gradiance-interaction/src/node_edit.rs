//! Behavior-node picking: click a node's glyph to select it individually.
//!
//! Runs in the Select tool state, after [`pick_joint`](crate::joint_edit::pick_joint)
//! (joints keep priority) and before the body select gesture. A hit selects
//! the node through the shared [`SelectTransition`] seam and consumes the
//! press via [`SuppressSelectPress`], so the body select does not also fire.
//! Nodes are first-class selectable entities (the render glyph shows the
//! selection state; Delete removes them like a body).

use crate::PointerOverUi;
use crate::joint_edit::SuppressSelectPress;
use crate::pointer::PointerButtons;
use crate::selection::{SelectTransition, SelectedJoint, Selection};
use crate::snap::SnappedCursor;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::node::BehaviorNode;

/// Screen-space pick radius for a node glyph (logical px).
pub const NODE_PICK_PX: f32 = 10.0;

/// Selects the nearest behavior node whose glyph is under a left-click.
#[expect(clippy::too_many_arguments)] // grouped picking reads
pub fn pick_node(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    keys: Res<ButtonInput<KeyCode>>,
    cam_scale: Res<crate::camera::CameraScale>,
    nodes: Query<(Entity, &Transform), With<BehaviorNode>>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<SelectedJoint>,
    mut suppress: ResMut<SuppressSelectPress>,
) {
    // A joint pick this frame already consumed the press.
    if !buttons.just_pressed(MouseButton::Left) || over_ui.0 || suppress.0 {
        return;
    }
    let Some(p) = snapped.effective() else {
        return;
    };
    let radius = NODE_PICK_PX * cam_scale.0;

    let mut best: Option<(f32, Entity)> = None;
    for (entity, transform) in &nodes {
        let d = transform.translation.truncate().distance(p);
        if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, entity));
        }
    }
    let Some((_, node)) = best else {
        return;
    };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let transition = if shift {
        SelectTransition::ToggleBodies(vec![node])
    } else {
        SelectTransition::SetBodies(vec![node])
    };
    transition.apply(&mut selection, &mut selected_joint, &groups);
    suppress.0 = true;
}

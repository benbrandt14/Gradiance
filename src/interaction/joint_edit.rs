//! Selecting and editing joints directly (Select tool).
//!
//! Joints are authored entities, so making them first-class editable
//! reuses the whole command machinery: picking sets [`SelectedJoint`],
//! configuration goes through `PropertyEditIntent` ([`PropertyValue`]'s
//! `Joint` variant), deletion through `DeleteJointIntent`, and moving an
//! anchor emits a `PropertyEditIntent` with the relocated [`JointDef`].
//!
//! Gesture flow (Select tool, left button):
//! - press within a joint anchor's screen radius → select that joint,
//!   clear the body selection, and (paused only) arm an anchor drag;
//! - press elsewhere → clear the joint selection and let the body select
//!   tool run;
//! - drag past a deadzone (paused) → preview the anchor at the cursor;
//! - release → commit one undoable anchor move.

use crate::command::intent::PropertyEditIntent;
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::{IdIndex, StableId};
use crate::core::states::{GameState, ToolState};
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::group::SelectionGroup;
use crate::domain::joint::JointDef;
use crate::interaction::PointerOverUi;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::selection::{SelectTransition, SelectedJoint, Selection};
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::ActiveGesture;
use bevy::prelude::*;

/// Screen-space radius (logical px) for clicking a joint anchor. Shared with
/// the context menu so left-click picking and right-click config agree.
pub const ANCHOR_PICK_PX: f32 = 12.0;
/// World-space deadzone before an anchor drag engages.
const DRAG_DEADZONE: f32 = 4.0;

/// Set true by joint picking to tell the body select tool to skip this
/// press (a joint click must not also start a body gesture).
#[derive(Resource, Default, Debug)]
pub struct SuppressSelectPress(pub bool);

/// In-progress anchor drag of the selected joint (paused-mode only).
#[derive(Resource, Default, Debug)]
pub struct JointAnchorDrag {
    /// The joint entity whose anchor is being dragged.
    joint: Option<Entity>,
    /// Whether the deadzone has been left.
    engaged: bool,
}

/// Picks a joint on left-press (Select tool) and arms anchor dragging.
///
/// Runs before the body select tool; on a joint hit it consumes the press
/// via [`SuppressSelectPress`].
#[expect(clippy::too_many_arguments)] // one gesture, grouped reads
pub fn pick_joint(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    state: Res<State<GameState>>,
    joints: Query<(Entity, &JointDef)>,
    bodies: Query<&Transform, With<Body>>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
    index: Res<IdIndex>,
    projections: Query<&Projection, With<Camera3d>>,
    mut selected_joint: ResMut<SelectedJoint>,
    mut body_selection: ResMut<Selection>,
    mut suppress: ResMut<SuppressSelectPress>,
    mut drag: ResMut<JointAnchorDrag>,
    mut active: ResMut<ActiveGesture>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    suppress.0 = false;
    let Some(p) = snapped.effective() else {
        return;
    };
    if over_ui.0 {
        return;
    }

    let radius = ANCHOR_PICK_PX * crate::interaction::camera::camera_scale(&projections);
    let anchor_of = |def: &JointDef| -> Option<Vec2> {
        let entity = index.entity(def.body_a)?;
        let pose = PosRot::from_transform(bodies.get(entity).ok()?);
        Some(def.anchor_world(pose.pos, pose.rot))
    };

    // Nearest joint whose anchor is within the pick radius.
    let mut best: Option<(f32, Entity)> = None;
    for (entity, def) in &joints {
        if let Some(anchor) = anchor_of(def) {
            let d = anchor.distance(p);
            if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, entity));
            }
        }
    }

    if let Some((_, joint)) = best {
        // Selecting the joint clears the body selection (the invariant is
        // enforced by the transition, not by hand here).
        SelectTransition::SelectJoint(joint).apply(
            &mut body_selection,
            &mut selected_joint,
            &groups,
        );
        suppress.0 = true;
        // Anchor dragging only while paused (never fight the solver).
        if *state.get() == GameState::Paused {
            drag.joint = Some(joint);
            drag.engaged = false;
            active.0 = true;
        }
    } else {
        // A non-joint press drops joint focus; the body tool handles the
        // rest (body select / box select / clear).
        SelectTransition::DeselectJoint.apply(&mut body_selection, &mut selected_joint, &groups);
    }
}

/// Drives the anchor drag: preview on move, commit one edit on release.
pub fn drag_joint_anchor(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    ids: Query<&StableId>,
    joints: Query<&JointDef>,
    bodies: Query<&Transform, With<Body>>,
    index: Res<IdIndex>,
    mut drag: ResMut<JointAnchorDrag>,
    mut active: ResMut<ActiveGesture>,
    mut edits: MessageWriter<PropertyEditIntent>,
) {
    let Some(joint) = drag.joint else {
        return;
    };
    let Some(p) = snapped.effective() else {
        return;
    };

    if buttons.just_released(MouseButton::Left) {
        drag.joint = None;
        active.0 = false;
        if !drag.engaged {
            return;
        }
        if let Ok(def) = joints.get(joint)
            && let Ok(&joint_id) = ids.get(joint)
            && let Some(new_def) = relocated(&def.clone(), p, &bodies, &index)
        {
            edits.write(PropertyEditIntent {
                changes: vec![PropertyChange {
                    id: joint_id,
                    old: PropertyValue::Joint(def.clone()),
                    new: PropertyValue::Joint(new_def),
                }],
            });
        }
        return;
    }

    // Engage once the cursor leaves the deadzone (relative to the current
    // anchor), so a plain click never nudges the anchor.
    if !drag.engaged
        && let Ok(def) = joints.get(joint)
        && let Some(anchor) = current_anchor(def, &bodies, &index)
        && anchor.distance(p) > DRAG_DEADZONE
    {
        drag.engaged = true;
    }
}

/// The joint's current world anchor, if body A resolves.
fn current_anchor(
    def: &JointDef,
    bodies: &Query<&Transform, With<Body>>,
    index: &IdIndex,
) -> Option<Vec2> {
    let entity = index.entity(def.body_a)?;
    let pose = PosRot::from_transform(bodies.get(entity).ok()?);
    Some(def.anchor_world(pose.pos, pose.rot))
}

/// A copy of `def` with both anchors moved to world point `target`.
///
/// `anchor_a` is re-expressed in body A's frame; `anchor_b` in body B's
/// frame (or left as the world point for a world pin).
fn relocated(
    def: &JointDef,
    target: Vec2,
    bodies: &Query<&Transform, With<Body>>,
    index: &IdIndex,
) -> Option<JointDef> {
    let pose_a = {
        let e = index.entity(def.body_a)?;
        PosRot::from_transform(bodies.get(e).ok()?)
    };
    let mut new = def.clone();
    new.anchor_a = Vec2::from_angle(-pose_a.rot).rotate(target - pose_a.pos);
    match def.body_b {
        Some(id) => {
            let e = index.entity(id)?;
            let pose_b = PosRot::from_transform(bodies.get(e).ok()?);
            new.anchor_b = Vec2::from_angle(-pose_b.rot).rotate(target - pose_b.pos);
        }
        None => new.anchor_b = target, // world pin: anchor_b is the world point
    }
    Some(new)
}

/// Clears the joint selection when leaving the Select tool.
pub fn clear_joint_on_tool_change(
    tool: Res<State<ToolState>>,
    mut selected_joint: ResMut<SelectedJoint>,
    mut drag: ResMut<JointAnchorDrag>,
) {
    if *tool.get() != ToolState::Select {
        selected_joint.0 = None;
        drag.joint = None;
    }
}

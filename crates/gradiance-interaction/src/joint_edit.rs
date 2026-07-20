//! Selecting and editing joints directly (Select tool).
//!
//! Joints are authored entities, so making them first-class editable
//! reuses the whole command machinery: picking sets [`SelectedJoint`],
//! configuration goes through `PropertyEditIntent` ([`PropertyValue`]'s
//! `Joint` variant), deletion through `DeleteJointIntent`, and moving an
//! anchor emits a `PropertyEditIntent` with the relocated [`JointDef`].
//!
//! Gesture flow (Select tool, left button):
//! - press anywhere on a joint's *glyph* (ring, travel line, coil — not
//!   just the anchor) → select that joint and clear the body selection;
//! - press on the selected joint (paused only): nearest grab wins — the
//!   anchor arms an anchor drag, a **limit handle** (hinge arc end /
//!   prismatic travel cap) arms a limit drag;
//! - press elsewhere → clear the joint selection and let the body select
//!   tool run;
//! - drag past a deadzone (paused) → preview at the cursor;
//! - release → commit one undoable edit (anchor move or new limits).

use crate::PointerOverUi;
use crate::pointer::PointerButtons;
use crate::selection::{SelectTransition, SelectedJoint, Selection};
use crate::snap::SnappedCursor;
use crate::tools::ActiveGesture;
use bevy::prelude::*;
use gradiance_command::intent::PropertyEditIntent;
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::states::{GameState, ToolState};
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_geometry::snapping::project_on_segment;

/// Screen-space radius (logical px) for clicking a joint glyph. Shared with
/// the context menu so left-click picking and right-click config agree.
pub const ANCHOR_PICK_PX: f32 = 12.0;
/// World-space deadzone before an anchor drag engages.
const DRAG_DEADZONE: f32 = 4.0;
/// Hinge glyph ring radius (logical px) — shared with `render::joint_viz`.
pub const HINGE_RING_PX: f32 = 6.0;
/// Hinge limit-arc radius (logical px) — shared with `render::joint_viz`.
pub const HINGE_LIMIT_RADIUS_PX: f32 = 14.0;
/// Reference half-length (logical px) of an unlimited prismatic's axis line.
pub const SLIDER_REF_HALF_PX: f32 = 40.0;

/// Which limit end a handle drag edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitEnd {
    /// The lower limit (travel start / min angle).
    Min,
    /// The upper limit (travel end / max angle).
    Max,
}

/// The prismatic glyph's drawn span: the exact travel when limited, a
/// reference line when unlimited.
pub fn slider_span(anchor: Vec2, dir: Vec2, limits: Option<[f32; 2]>, s: f32) -> (Vec2, Vec2) {
    match limits {
        Some([min, max]) => (anchor + dir * min, anchor + dir * max),
        None => (
            anchor - dir * SLIDER_REF_HALF_PX * s,
            anchor + dir * SLIDER_REF_HALF_PX * s,
        ),
    }
}

/// Distance from `p` to the joint's drawn glyph — the whole ring / travel
/// line / coil is selectable, not only the anchor point.
pub fn glyph_distance(
    def: &JointDef,
    anchor: Vec2,
    rot_a: f32,
    world_b: Option<Vec2>,
    p: Vec2,
    s: f32,
) -> f32 {
    match &def.kind {
        JointKind::Hinge { .. } => (anchor.distance(p) - HINGE_RING_PX * s).max(0.0),
        JointKind::Slider { axis, limits, .. } => {
            let dir = Vec2::from_angle(rot_a).rotate(*axis);
            let (from, to) = slider_span(anchor, dir, *limits, s);
            p.distance(project_on_segment(p, from, to))
        }
        JointKind::Spring { .. } => match world_b {
            Some(b) => p.distance(project_on_segment(p, anchor, b)),
            None => anchor.distance(p),
        },
    }
}

/// The selected joint's limit-handle positions (hinge arc ends / prismatic
/// travel caps), `None` when the kind has no limits set.
pub fn limit_handles(
    def: &JointDef,
    anchor: Vec2,
    rot_a: f32,
    s: f32,
) -> Option<[(Vec2, LimitEnd); 2]> {
    match &def.kind {
        JointKind::Hinge {
            limits: Some([min, max]),
            ..
        } => {
            let r = HINGE_LIMIT_RADIUS_PX * s;
            Some([
                (anchor + Vec2::from_angle(rot_a + min) * r, LimitEnd::Min),
                (anchor + Vec2::from_angle(rot_a + max) * r, LimitEnd::Max),
            ])
        }
        JointKind::Slider {
            axis,
            limits: Some([min, max]),
            ..
        } => {
            let dir = Vec2::from_angle(rot_a).rotate(*axis);
            Some([
                (anchor + dir * *min, LimitEnd::Min),
                (anchor + dir * *max, LimitEnd::Max),
            ])
        }
        _ => None,
    }
}

/// The limits that would result from dragging `end` to cursor `p`
/// (prismatic: projection on the axis; hinge: cursor angle about the
/// anchor, relative to body A). Ends never cross.
pub fn dragged_limits(
    def: &JointDef,
    anchor: Vec2,
    rot_a: f32,
    p: Vec2,
    end: LimitEnd,
) -> Option<[f32; 2]> {
    match &def.kind {
        JointKind::Slider {
            axis,
            limits: Some([min, max]),
            ..
        } => {
            let dir = Vec2::from_angle(rot_a).rotate(*axis);
            let t = (p - anchor).dot(dir);
            Some(match end {
                LimitEnd::Min => [t.min(*max - 1.0), *max],
                LimitEnd::Max => [*min, t.max(*min + 1.0)],
            })
        }
        JointKind::Hinge {
            limits: Some([min, max]),
            ..
        } => {
            let a = wrap_pi((p - anchor).to_angle() - rot_a);
            Some(match end {
                LimitEnd::Min => [a.min(*max - 0.05), *max],
                LimitEnd::Max => [*min, a.max(*min + 0.05)],
            })
        }
        _ => None,
    }
}

/// A copy of `def` with its limits replaced (kinds without limits pass
/// through unchanged).
fn with_limits(def: &JointDef, new_limits: [f32; 2]) -> JointDef {
    let mut new = def.clone();
    match &mut new.kind {
        JointKind::Hinge { limits, .. } | JointKind::Slider { limits, .. } => {
            *limits = Some(new_limits);
        }
        JointKind::Spring { .. } => {}
    }
    new
}

/// Wraps an angle into `(-π, π]`.
fn wrap_pi(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(std::f32::consts::TAU);
    if wrapped > std::f32::consts::PI {
        wrapped - std::f32::consts::TAU
    } else {
        wrapped
    }
}

/// Set true by joint picking to tell the body select tool to skip this
/// press (a joint click must not also start a body gesture).
#[derive(Resource, Default, Debug)]
pub struct SuppressSelectPress(pub bool);

/// In-progress limit-handle drag of the selected joint (paused-mode only).
/// `tentative` is the preview the glyph renders while dragging; the edit
/// commits once, on release.
#[derive(Resource, Default, Debug)]
pub struct JointLimitDrag {
    /// The dragged joint and which end its handle edits.
    target: Option<(Entity, LimitEnd)>,
    /// Cursor at press (deadzone reference).
    press: Vec2,
    /// Whether the deadzone has been left.
    engaged: bool,
    /// The limits the glyph previews this frame.
    pub tentative: Option<[f32; 2]>,
}

impl JointLimitDrag {
    /// The joint currently being limit-dragged, if any.
    pub fn dragging(&self) -> Option<Entity> {
        self.target.map(|(e, _)| e)
    }
}

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
    cam_scale: Res<crate::camera::CameraScale>,
    mut selected_joint: ResMut<SelectedJoint>,
    mut body_selection: ResMut<Selection>,
    mut suppress: ResMut<SuppressSelectPress>,
    mut drag: ResMut<JointAnchorDrag>,
    mut limit_drag: ResMut<JointLimitDrag>,
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

    let s = cam_scale.0;
    let radius = ANCHOR_PICK_PX * s;
    let frame_of = |def: &JointDef| -> Option<(Vec2, f32)> {
        let entity = index.entity(def.body_a)?;
        let pose = PosRot::from_transform(bodies.get(entity).ok()?);
        Some((def.anchor_world(pose.pos, pose.rot), pose.rot))
    };
    let world_b_of = |def: &JointDef| -> Option<Vec2> {
        match def.body_b {
            Some(id) => {
                let pose = PosRot::from_transform(bodies.get(index.entity(id)?).ok()?);
                Some(pose.pos + Vec2::from_angle(pose.rot).rotate(def.anchor_b))
            }
            None => Some(def.anchor_b),
        }
    };

    // Nearest joint whose *glyph* is within the pick radius — the whole
    // ring / travel line / coil is selectable, not just the anchor.
    let mut best: Option<(f32, Entity)> = None;
    for (entity, def) in &joints {
        if let Some((anchor, rot_a)) = frame_of(def) {
            let d = glyph_distance(def, anchor, rot_a, world_b_of(def), p, s);
            if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, entity));
            }
        }
    }

    if let Some((_, joint)) = best {
        let reselected = selected_joint.0 == Some(joint);
        // Selecting the joint clears the body selection (the invariant is
        // enforced by the transition, not by hand here).
        SelectTransition::SelectJoint(joint).apply(
            &mut body_selection,
            &mut selected_joint,
            &groups,
        );
        suppress.0 = true;
        // Grabs only while paused (never fight the solver). On the already
        // selected joint, the nearest affordance wins: the anchor arms an
        // anchor drag, a limit handle arms a limit drag (ties → anchor, so
        // a prismatic whose travel starts at the anchor keeps its anchor
        // draggable; its min is still editable in the inspector).
        if *state.get() == GameState::Paused
            && let Ok((_, def)) = joints.get(joint)
            && let Some((anchor, rot_a)) = frame_of(def)
        {
            let handle_grab = reselected
                .then(|| limit_handles(def, anchor, def.limit_reference_angle(rot_a), s))
                .flatten()
                .and_then(|handles| {
                    handles
                        .into_iter()
                        .map(|(pos, end)| (pos.distance(p), end))
                        .filter(|(d, _)| *d <= radius && *d < anchor.distance(p))
                        .min_by(|(a, _), (b, _)| a.total_cmp(b))
                });
            if let Some((_, end)) = handle_grab {
                limit_drag.target = Some((joint, end));
                limit_drag.press = p;
                limit_drag.engaged = false;
                limit_drag.tentative = None;
                active.0 = true;
            } else if anchor.distance(p) <= radius {
                drag.joint = Some(joint);
                drag.engaged = false;
                active.0 = true;
            }
        }
    } else {
        // A non-joint press drops joint focus; the body tool handles the
        // rest (body select / box select / clear).
        SelectTransition::DeselectJoint.apply(&mut body_selection, &mut selected_joint, &groups);
    }
}

/// Drives a limit-handle drag: tentative limits preview on move (rendered
/// by the joint glyph), one undoable `PropertyEditIntent` on release.
pub fn drag_joint_limit(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    ids: Query<&StableId>,
    joints: Query<&JointDef>,
    bodies: Query<&Transform, With<Body>>,
    index: Res<IdIndex>,
    mut drag: ResMut<JointLimitDrag>,
    mut active: ResMut<ActiveGesture>,
    mut edits: MessageWriter<PropertyEditIntent>,
) {
    let Some((joint, end)) = drag.target else {
        return;
    };
    let Some(p) = snapped.effective() else {
        return;
    };
    let frame = joints.get(joint).ok().and_then(|def| {
        let entity = index.entity(def.body_a)?;
        let pose = PosRot::from_transform(bodies.get(entity).ok()?);
        Some((def, def.anchor_world(pose.pos, pose.rot), pose.rot))
    });

    if buttons.just_released(MouseButton::Left) {
        let commit = drag.engaged.then_some(drag.tentative).flatten();
        drag.target = None;
        drag.tentative = None;
        active.0 = false;
        if let (Some(new_limits), Some((def, ..))) = (commit, frame)
            && let Ok(&joint_id) = ids.get(joint)
        {
            edits.write(PropertyEditIntent {
                changes: vec![PropertyChange {
                    id: joint_id,
                    old: PropertyValue::Joint(def.clone()),
                    new: PropertyValue::Joint(with_limits(def, new_limits)),
                }],
            });
        }
        return;
    }

    let Some((def, anchor, rot_a)) = frame else {
        drag.target = None;
        drag.tentative = None;
        active.0 = false;
        return;
    };
    if !drag.engaged && drag.press.distance(p) > DRAG_DEADZONE {
        drag.engaged = true;
    }
    if drag.engaged {
        drag.tentative = dragged_limits(def, anchor, def.limit_reference_angle(rot_a), p, end);
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
    mut limit_drag: ResMut<JointLimitDrag>,
) {
    if *tool.get() != ToolState::Select {
        selected_joint.0 = None;
        drag.joint = None;
        limit_drag.target = None;
        limit_drag.tentative = None;
    }
}

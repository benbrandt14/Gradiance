//! Behavior-node picking and dragging: click a node's glyph to select it
//! individually; press and drag to **move** it — the glyph follows the
//! cursor and, on release, the node re-attaches to the body under the
//! release point (offset in its frame) or becomes a free node on empty
//! canvas, all as one undoable property edit.
//!
//! Node picking runs in the Select tool state, after
//! [`pick_joint`](crate::joint_edit::pick_joint) (joints keep priority)
//! and before the body select gesture. A hit selects the node through the
//! shared [`SelectTransition`] seam and consumes the press via
//! [`SuppressSelectPress`], so the body select does not also fire. In the
//! **Drag** tool, [`arm_node_drag`] gives the same press the same
//! meaning: the gizmo under the pointer moves, not the body behind it.
//! Nodes are first-class selectable entities (the render glyph shows the
//! selection state; Delete removes them like a body).

use crate::PointerOverUi;
use crate::joint_edit::SuppressSelectPress;
use crate::pointer::PointerButtons;
use crate::selection::{SelectTransition, SelectedJoint, Selection};
use crate::snap::SnappedCursor;
use crate::tools::ActiveGesture;
use bevy::prelude::*;
use gradiance_command::intent::PropertyEditIntent;
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::states::GameState;
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::laser::Laser;
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_domain::thruster::Thruster;

/// Screen-space pick radius for a node glyph (logical px): the drawn ring
/// (6 px, `render::tracer::draw_node_glyphs`) plus the shared 4 px pick
/// slop — the click target is exactly the glyph the user sees, selected or
/// not.
pub const NODE_PICK_PX: f32 = 10.0;
/// Screen-space deadzone (logical px) before a node drag engages — a plain
/// click selects and never nudges the node (the joint anchor drag's rule).
const NODE_DRAG_DEADZONE_PX: f32 = 4.0;

/// The node under a left-press, if any: the nearest whose glyph is within
/// the pick radius. Shared by the Select tool's pick and the Drag tool's
/// arm so the two cannot disagree about what a press on a node means.
fn node_at(
    p: Vec2,
    nodes: &Query<(Entity, &Transform), With<BehaviorNode>>,
    radius: f32,
) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for (entity, transform) in nodes {
        let d = transform.translation.truncate().distance(p);
        if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, entity));
        }
    }
    best.map(|(_, node)| node)
}

/// In-progress behavior-node drag. The node's glyph follows
/// [`cursor`](Self::cursor) (the renderer draws it there — its `Transform`
/// is untouched until the commit); release commits **one** property edit:
/// re-attached to the body under the release, or free on empty canvas.
#[derive(Resource, Default, Debug)]
pub struct NodeDrag {
    /// The dragged node and its stable id.
    target: Option<(Entity, StableId)>,
    /// Cursor at press (deadzone reference).
    press: Vec2,
    /// Whether the deadzone has been left (the glyph follows the cursor).
    engaged: bool,
    /// Where the glyph draws this frame (`None` when the cursor is gone).
    pub cursor: Option<Vec2>,
}

impl NodeDrag {
    /// The node currently being dragged, if any.
    pub fn dragging(&self) -> Option<Entity> {
        self.target.map(|(e, _)| e)
    }

    /// Whether the drag has engaged past its deadzone.
    pub fn engaged(&self) -> bool {
        self.engaged
    }
}

/// Selects the nearest behavior node whose glyph is under a left-click
/// (Select tool) and, while the press is held, arms a node drag.
#[expect(clippy::too_many_arguments)] // grouped picking reads
pub fn pick_node(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    cam_scale: Res<crate::camera::CameraScale>,
    nodes: Query<(Entity, &Transform), With<BehaviorNode>>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
    ids: Query<&StableId>,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<SelectedJoint>,
    mut suppress: ResMut<SuppressSelectPress>,
    mut drag: ResMut<NodeDrag>,
    mut active: ResMut<ActiveGesture>,
) {
    // A joint pick this frame already consumed the press.
    if !buttons.just_pressed(MouseButton::Left) || over_ui.0 || suppress.0 {
        return;
    }
    let Some(p) = snapped.effective() else {
        return;
    };
    let radius = NODE_PICK_PX * cam_scale.0;
    let Some(node) = node_at(p, &nodes, radius) else {
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
    // Holding the press on the node arms a drag of it: the gizmo under
    // the pointer is what moves, not the body behind it. Grabs only while
    // paused, like the joint anchor drag (never fight the solver).
    if *state.get() == GameState::Paused
        && let Ok(&id) = ids.get(node)
    {
        drag.target = Some((node, id));
        drag.press = p;
        drag.engaged = false;
        active.0 = true;
    }
}

/// The Drag tool's press on a node: the same arm [`pick_node`] gives the
/// Select tool — the node moves, the body behind it does not. Runs before
/// the drag tool's driver and consumes the press through the shared
/// [`SuppressSelectPress`] the tool then checks.
pub fn arm_node_drag(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    cam_scale: Res<crate::camera::CameraScale>,
    nodes: Query<(Entity, &Transform), With<BehaviorNode>>,
    ids: Query<&StableId>,
    mut suppress: ResMut<SuppressSelectPress>,
    mut drag: ResMut<NodeDrag>,
    mut active: ResMut<ActiveGesture>,
) {
    if !buttons.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    suppress.0 = false;
    let Some(p) = snapped.effective() else {
        return;
    };
    let Some(node) = node_at(p, &nodes, NODE_PICK_PX * cam_scale.0) else {
        return;
    };
    let Ok(&id) = ids.get(node) else {
        return;
    };
    suppress.0 = true;
    drag.target = Some((node, id));
    drag.press = p;
    drag.engaged = false;
    active.0 = true;
}

/// Drives a node drag: engages past the deadzone (the glyph follows the
/// cursor), and on release commits **one** `PropertyEditIntent` — the node
/// re-attached to the topmost body under the release point (offset in its
/// frame, a thruster's or laser's axis re-expressed so its **world** angle
/// is unchanged) or free at the release point. The old placement is
/// captured for undo, so the edit reverses cleanly.
#[expect(clippy::too_many_arguments)] // one gesture, grouped reads
pub fn drag_node(
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    cam_scale: Res<crate::camera::CameraScale>,
    world: crate::tools::context::ToolWorld,
    nodes: Query<(&NodeKind, &NodeAttachment, &Transform), With<BehaviorNode>>,
    index: Res<IdIndex>,
    mut drag: ResMut<NodeDrag>,
    mut active: ResMut<ActiveGesture>,
    mut edits: MessageWriter<PropertyEditIntent>,
) {
    let Some((node, node_id)) = drag.target else {
        return;
    };
    let Some(p) = snapped.effective() else {
        return;
    };
    drag.cursor = drag.engaged.then_some(p);

    if buttons.just_released(MouseButton::Left) {
        let engaged = drag.engaged;
        drag.target = None;
        drag.engaged = false;
        drag.cursor = None;
        active.0 = false;
        if !engaged {
            return;
        }
        let Ok((kind, attachment, transform)) = nodes.get(node) else {
            return;
        };
        // The poses the relocation needs: where the node currently rides
        // (its old target's live rotation, for the world angle) and where
        // the release lands (the topmost body there, if any).
        let old_rot = attachment
            .target
            .and_then(|t| index.entity(t))
            .and_then(|e| world.pose_of(e))
            .map_or(0.0, |pose| pose.rot);
        let target = world
            .topmost_body_at(p)
            .and_then(|e| Some((world.id_of(e)?, world.pose_of(e)?)));
        let changes = relocate_changes(
            node_id,
            kind,
            attachment,
            PosRot::from_transform(transform),
            p,
            old_rot,
            target,
        );
        if !changes.is_empty() {
            edits.write(PropertyEditIntent { changes });
        }
        return;
    }

    if !drag.engaged && drag.press.distance(p) > NODE_DRAG_DEADZONE_PX * cam_scale.0 {
        drag.engaged = true;
    }
}

/// The property changes that move a node from `(attachment, pose)` to
/// world point `p`: re-attached to `target` (offset in its frame) or free,
/// with a thruster's or laser's axis re-expressed so the world direction
/// it pushes/aims stays put (`old_rot` is the old target's live rotation).
/// Empty when nothing changes. Pure, so the relocation rules are
/// unit-testable without a world.
#[must_use]
fn relocate_changes(
    id: StableId,
    kind: &NodeKind,
    attachment: &NodeAttachment,
    pose: PosRot,
    p: Vec2,
    old_rot: f32,
    target: Option<(StableId, PosRot)>,
) -> Vec<PropertyChange> {
    // The node's world axis: an attached thruster/laser stores its angle
    // in the body's frame, so the world direction is the frame angle plus
    // the body's live rotation (the draw code's formula).
    let world_angle = match kind {
        NodeKind::Thruster(t) => t.angle.value() + old_rot,
        NodeKind::Laser(l) => l.angle.value() + old_rot,
        NodeKind::Tracer(_) => 0.0,
    };
    let (new_attachment, new_rot) = match target {
        Some((body_id, body_pose)) => (
            NodeAttachment::to(
                body_id,
                Vec2::from_angle(-body_pose.rot).rotate(p - body_pose.pos),
            ),
            body_pose.rot,
        ),
        None => (NodeAttachment::free(), 0.0),
    };
    if new_attachment == *attachment {
        return Vec::new();
    }
    let mut changes = vec![PropertyChange {
        id,
        old: PropertyValue::NodePlace(*attachment, pose),
        new: PropertyValue::NodePlace(
            new_attachment,
            PosRot {
                pos: p,
                rot: pose.rot,
            },
        ),
    }];
    // A moved target re-expresses the axis in the new frame; the same
    // target keeps its frame angle (and a free node's frame *is* the
    // world's).
    if new_attachment.target != attachment.target {
        let new_kind = match kind {
            NodeKind::Thruster(t) => NodeKind::Thruster(Thruster {
                angle: gradiance_units::Angle(world_angle - new_rot),
                ..*t
            }),
            NodeKind::Laser(l) => NodeKind::Laser(Laser {
                angle: gradiance_units::Angle(world_angle - new_rot),
                ..l.clone()
            }),
            NodeKind::Tracer(_) => kind.clone(),
        };
        if new_kind != *kind {
            changes.push(PropertyChange {
                id,
                old: PropertyValue::NodeKind(kind.clone()),
                new: PropertyValue::NodeKind(new_kind),
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_domain::tracer::Tracer;

    /// A laser moved from a rotated body to empty space keeps aiming the
    /// same world way: its angle is re-expressed into the world frame, one
    /// placement change plus one kind change.
    #[test]
    fn a_detached_laser_keeps_its_world_angle() {
        let id = StableId::new();
        let kind = NodeKind::Laser(Laser::default());
        let attached = NodeAttachment::to(StableId::new(), Vec2::ZERO);
        let changes = relocate_changes(
            id,
            &kind,
            &attached,
            PosRot {
                pos: Vec2::ZERO,
                rot: 0.0,
            },
            Vec2::new(1.0, 2.0),
            0.5,  // the body it rode was rotated 0.5 rad
            None, // released over empty canvas
        );
        assert_eq!(changes.len(), 2, "place + re-expressed angle: {changes:?}");
        let PropertyValue::NodePlace(attachment, pose) = &changes[0].new else {
            panic!("the first change is the placement: {changes:?}");
        };
        assert_eq!(attachment.target, None, "empty canvas: free");
        assert_eq!(pose.pos, Vec2::new(1.0, 2.0));
        let PropertyValue::NodeKind(NodeKind::Laser(laser)) = &changes[1].new else {
            panic!("the second change is the kind: {changes:?}");
        };
        assert_eq!(laser.angle.value(), Laser::default().angle.value() + 0.5);
    }

    /// Re-attaching to a rotated body re-expresses the axis in *its*
    /// frame, so the world direction is unchanged.
    #[test]
    fn a_retargeted_thruster_points_the_same_world_way() {
        let id = StableId::new();
        let kind = NodeKind::Thruster(Thruster::default());
        let free = NodeAttachment::free();
        let changes = relocate_changes(
            id,
            &kind,
            &free,
            PosRot {
                pos: Vec2::ZERO,
                rot: 0.0,
            },
            Vec2::new(2.0, 0.0),
            0.0, // it was free: the world and its frame agree
            Some((
                StableId::new(),
                PosRot {
                    pos: Vec2::ZERO,
                    rot: -0.25,
                },
            )),
        );
        let PropertyValue::NodeKind(NodeKind::Thruster(t)) = &changes[1].new else {
            panic!("kind change: {changes:?}");
        };
        assert_eq!(
            t.angle.value(),
            Thruster::default().angle.value() + 0.25,
            "the body's -0.25 rotation comes out of the frame angle"
        );
    }

    /// A tracer's drag never invents a kind change (there is no axis to
    /// re-express).
    #[test]
    fn a_tracer_drag_changes_only_its_placement() {
        let id = StableId::new();
        let kind = NodeKind::Tracer(Tracer::default());
        let attached = NodeAttachment::to(StableId::new(), Vec2::ZERO);
        let changes = relocate_changes(
            id,
            &kind,
            &attached,
            PosRot {
                pos: Vec2::ZERO,
                rot: 0.0,
            },
            Vec2::new(0.0, 5.0),
            0.0,
            None,
        );
        assert_eq!(changes.len(), 1, "placement only: {changes:?}");
    }

    /// Releasing where the node already lives changes nothing.
    #[test]
    fn a_move_that_changes_nothing_is_no_change() {
        let id = StableId::new();
        let kind = NodeKind::Thruster(Thruster::default());
        let free = NodeAttachment::free();
        assert!(
            relocate_changes(
                id,
                &kind,
                &free,
                PosRot {
                    pos: Vec2::new(2.0, 2.0),
                    rot: 0.0
                },
                Vec2::new(2.0, 2.0),
                0.0,
                None,
            )
            .is_empty(),
            "free at the same point: no change"
        );
    }
}

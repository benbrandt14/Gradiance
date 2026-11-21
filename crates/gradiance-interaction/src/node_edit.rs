//! Behavior-node picking and dragging: click a node's glyph to select it
//! individually; press and drag to **move** it — the glyph follows the
//! cursor and, on release, the node re-attaches to the body under the
//! release point (offset in its frame, its effect centered where it
//! rests) — and on empty canvas the node goes with the gesture (a node
//! rests on a body or not at all), one undoable edit either way.
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
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_op::{InvokeOp, json};

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
/// re-attached to the body under the release, or deleted on empty canvas.
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
/// is unchanged) — or, on empty canvas, deleted outright. The old placement is
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
    mut delete: MessageWriter<InvokeOp>,
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
        let pose = PosRot::from_transform(transform);
        match relocate(kind, attachment, p, old_rot, target) {
            NodeRelocation::Move {
                attachment: to,
                kind: turned,
            } => {
                let mut changes = vec![PropertyChange {
                    id: node_id,
                    old: PropertyValue::NodePlace(*attachment, pose),
                    new: PropertyValue::NodePlace(
                        to,
                        PosRot {
                            pos: p,
                            rot: pose.rot,
                        },
                    ),
                }];
                if let Some(new_kind) = turned {
                    changes.push(PropertyChange {
                        id: node_id,
                        old: PropertyValue::NodeKind(kind.clone()),
                        new: PropertyValue::NodeKind(new_kind),
                    });
                }
                edits.write(PropertyEditIntent { changes });
            }
            // Nothing under the release: the node goes with the gesture —
            // the same delete operation the menu's row invokes, undo and
            // all. A node rests on a body or not at all.
            NodeRelocation::Delete => {
                delete.write(InvokeOp::with(
                    "body.delete",
                    json!({"i": node_id.0.to_string()}),
                ));
            }
            NodeRelocation::Unchanged => {}
        }
        return;
    }

    if !drag.engaged && drag.press.distance(p) > NODE_DRAG_DEADZONE_PX * cam_scale.0 {
        drag.engaged = true;
    }
}

/// What one node drag commits on release. The node **re-attaches** to
/// the body under the release (offset in its frame — its effect, the
/// push or the beam, centered where it rests), with a thruster's or
/// laser's axis re-expressed so the world direction it pushes/aims never
/// changes; with nothing under the release the node **goes with the
/// gesture** — a node rests on a body or not at all, never orphaned on
/// the background. Pure over its reads, so the rules are unit-testable
/// without a world.
#[derive(Debug, PartialEq)]
enum NodeRelocation {
    /// Re-attached: the new placement, with the payload re-expressed when
    /// the new frame is turned relative to the old.
    Move {
        /// The new attachment (target + offset in its frame).
        attachment: NodeAttachment,
        /// The payload with its axis re-expressed, when the frame differs.
        kind: Option<NodeKind>,
    },
    /// Nothing under the release: delete the node.
    Delete,
    /// The node already lives exactly where the release landed.
    Unchanged,
}

/// Where a drag that releases at `p` puts a node that was
/// `(kind, attachment)` on a frame rotated `old_rot`, given the topmost
/// `target` body under the release (`None` on empty canvas).
#[must_use]
fn relocate(
    kind: &NodeKind,
    attachment: &NodeAttachment,
    p: Vec2,
    old_rot: f32,
    target: Option<(StableId, PosRot)>,
) -> NodeRelocation {
    let Some((body_id, body_pose)) = target else {
        return NodeRelocation::Delete;
    };
    let new_attachment = NodeAttachment::to(
        body_id,
        Vec2::from_angle(-body_pose.rot).rotate(p - body_pose.pos),
    );
    if new_attachment == *attachment {
        return NodeRelocation::Unchanged;
    }
    // A moved target re-expresses the axis in the new frame; the same
    // target keeps its frame angle. The rule is `NodeKind::reframe`,
    // shared with the cut/merge rewires.
    let turned = (new_attachment.target != attachment.target)
        .then(|| kind.reframe(old_rot, body_pose.rot))
        .flatten();
    NodeRelocation::Move {
        attachment: new_attachment,
        kind: turned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_domain::laser::Laser;
    use gradiance_domain::thruster::Thruster;
    use gradiance_domain::tracer::Tracer;

    /// A release over empty canvas takes the node with it: a node rests
    /// on a body or not at all.
    #[test]
    fn a_release_on_empty_canvas_deletes_the_node() {
        let kind = NodeKind::Laser(Laser::default());
        let attached = NodeAttachment::to(StableId::new(), Vec2::ZERO);
        assert_eq!(
            relocate(&kind, &attached, Vec2::new(1.0, 2.0), 0.5, None),
            NodeRelocation::Delete
        );
        // A free node dragged within empty space goes too.
        assert_eq!(
            relocate(
                &kind,
                &NodeAttachment::free(),
                Vec2::new(1.0, 2.0),
                0.0,
                None
            ),
            NodeRelocation::Delete
        );
    }

    /// Re-attaching to a rotated body re-expresses the axis in *its*
    /// frame, so the world direction is unchanged, and the offset puts
    /// the node — its effect — exactly where it rests.
    #[test]
    fn a_retargeted_thruster_points_the_same_world_way() {
        let kind = NodeKind::Thruster(Thruster::default());
        let free = NodeAttachment::free();
        let moved = relocate(
            &kind,
            &free,
            Vec2::new(2.0, 1.0),
            0.0, // it was free: the world and its frame agree
            Some((
                StableId::new(),
                PosRot {
                    pos: Vec2::new(2.0, 1.0),
                    rot: -0.25,
                },
            )),
        );
        let NodeRelocation::Move { attachment, kind } = &moved else {
            panic!("a body was under the release: {moved:?}");
        };
        assert_eq!(
            attachment.offset.value(),
            Vec2::ZERO,
            "the node rests at the body's origin, its effect centered there"
        );
        let Some(NodeKind::Thruster(t)) = kind else {
            panic!("the payload turned with the frame: {moved:?}");
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
    fn a_tracer_drag_moves_only_its_placement() {
        let kind = NodeKind::Tracer(Tracer::default());
        let attached = NodeAttachment::to(StableId::new(), Vec2::ZERO);
        let moved = relocate(
            &kind,
            &attached,
            Vec2::new(0.0, 5.0),
            0.0,
            Some((
                StableId::new(),
                PosRot {
                    pos: Vec2::new(1.0, 0.0),
                    rot: 0.0,
                },
            )),
        );
        let NodeRelocation::Move { kind, .. } = &moved else {
            panic!("{moved:?}");
        };
        assert!(kind.is_none(), "no axis, no turn: {moved:?}");
    }

    /// Releasing where the node already lives changes nothing.
    #[test]
    fn a_release_where_it_already_lives_changes_nothing() {
        let kind = NodeKind::Thruster(Thruster::default());
        let host = StableId::new();
        let attached = NodeAttachment::to(host, Vec2::new(3.0, 0.0));
        assert_eq!(
            relocate(
                &kind,
                &attached,
                Vec2::new(5.0, 0.0),
                0.0,
                Some((
                    host,
                    PosRot {
                        pos: Vec2::new(2.0, 0.0),
                        rot: 0.0
                    }
                )),
            ),
            NodeRelocation::Unchanged,
            "the same body at the same offset: nothing to commit"
        );
    }
}

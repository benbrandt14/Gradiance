//! Behavior nodes: the placeable tracer tool, node selection/delete, and
//! behavior-copies-on-duplicate (`docs/signal-dataflow.md`).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::intent::{DuplicateIntent, SpawnNodeIntent};
use gradiance::core::ids::StableId;
use gradiance::core::units::PosRot;
use gradiance::domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance::domain::tracer::Tracer;
use gradiance::interaction::selection::Selection;
use gradiance::prelude::*;
use gradiance::scene::NodeRecord;

fn tracer_record(pos: Vec2, attach: NodeAttachment) -> NodeRecord {
    let id = StableId::new();
    NodeRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        attachment: attach,
        kind: NodeKind::Tracer(Tracer {
            fade_secs: gradiance::units::Time(2.0),
            ..Default::default()
        }),
        appearance: gradiance::domain::appearance::Appearance::default(),
    }
}

fn node_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<BehaviorNode>>()
        .iter(app.world())
        .count()
}

#[test]
fn spawning_a_tracer_node_is_undoable_and_persists() {
    let mut app = paused_app();
    let record = tracer_record(Vec2::new(30.0, 10.0), NodeAttachment::free());
    let id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();

    assert_eq!(node_count(&mut app), 1);
    let entity = entity_of(&app, id).unwrap();
    assert!(
        app.world().get::<Tracer>(entity).is_some(),
        "tracer carried"
    );
    assert!(app.world().get::<NodeKind>(entity).is_some());

    undo(&mut app);
    assert_eq!(node_count(&mut app), 0, "undo removes the node");
}

#[test]
fn an_attached_node_follows_its_body() {
    let mut app = paused_app();
    let mut body = box_record(Vec2::new(100.0, 0.0), 20.0, 20.0);
    body.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    // A node attached at offset (5, 0) in the body's frame.
    let record = tracer_record(
        Vec2::new(105.0, 0.0),
        NodeAttachment::to(body_id, Vec2::new(5.0, 0.0)),
    );
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    step(&mut app, 2);

    // Move the body; the follow system repositions the node.
    let body_entity = entity_of(&app, body_id).unwrap();
    app.world_mut()
        .get_mut::<Transform>(body_entity)
        .unwrap()
        .translation = Vec3::new(-50.0, 40.0, 0.0);
    step(&mut app, 2);
    let node_pos = app
        .world()
        .get::<Transform>(entity_of(&app, node_id).unwrap())
        .unwrap()
        .translation
        .truncate();
    assert!(
        (node_pos - Vec2::new(-45.0, 40.0)).length() < 1e-3,
        "node rides the body at its offset ({node_pos})"
    );
}

#[test]
fn a_node_is_individually_selectable_and_deletable() {
    let mut app = paused_app();
    let record = tracer_record(Vec2::new(0.0, 0.0), NodeAttachment::free());
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    let node_entity = entity_of(&app, node_id).unwrap();

    // Select the node directly (the pick system is exercised in ui tests;
    // here we drive the selection + delete command seam).
    app.world_mut().resource_mut::<Selection>().set(node_entity);
    step(&mut app, 1);
    assert!(
        app.world().resource::<Selection>().contains(node_entity),
        "the node stays selected (prune keeps nodes)"
    );

    // Delete removes it, undo restores it (same id).
    app.world_mut().write_message(DeleteIntent {
        targets: vec![node_id],
    });
    app.update();
    assert_eq!(node_count(&mut app), 0);
    undo(&mut app);
    assert_eq!(node_count(&mut app), 1);
    assert!(entity_of(&app, node_id).is_some());
}

#[test]
fn duplicating_a_body_copies_its_attached_tracer_and_bindings() {
    use gradiance::signal::{SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource};
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    // A tracer attached to the body, and a speed→fill binding about it.
    app.world_mut().write_message(SpawnNodeIntent {
        record: tracer_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO)),
    });
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "speed".into(),
            source: SignalSource::Speed(body_id),
            map: SignalMap::default(),
            curve: None,
            gradient: gradiance::signal::GradientSpec::default(),
            sink: SignalSink::Fill(body_id),
        });
    app.update();
    assert_eq!(node_count(&mut app), 1);

    // Duplicate the body.
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![body_id],
        offset: Vec2::new(200.0, 0.0),
    });
    app.update();

    // The attached tracer was cloned onto the duplicate.
    assert_eq!(node_count(&mut app), 2, "the tracer copied with the body");
    // And a binding referencing the clone exists.
    assert_eq!(
        app.world().resource::<SignalBindings>().0.len(),
        2,
        "the binding copied with the body"
    );
    // The clone is the body that isn't the original.
    let ids: Vec<StableId> = app
        .world_mut()
        .query_filtered::<&StableId, With<gradiance::domain::Body>>()
        .iter(app.world())
        .copied()
        .collect();
    let clone_body: StableId = ids.into_iter().find(|id| *id != body_id).unwrap();
    assert!(
        app.world()
            .resource::<SignalBindings>()
            .0
            .iter()
            .any(|b| matches!(b.source, SignalSource::Speed(id) if id == clone_body)),
        "the copied binding points at the duplicate"
    );

    // Undo removes the clones — bodies, nodes, and the copied binding.
    undo(&mut app);
    assert_eq!(node_count(&mut app), 1, "undo removes the cloned tracer");
    assert_eq!(
        app.world().resource::<SignalBindings>().0.len(),
        1,
        "undo removes the copied binding"
    );
}

#[test]
fn a_speed_to_fill_binding_tints_the_target_body() {
    use avian2d::prelude::LinearVelocity;
    use gradiance::domain::signal::{GradientSpec, SignalMap};
    use gradiance::signal::{
        SignalBinding, SignalBindings, SignalColorOverride, SignalSink, SignalSource,
    };

    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = gradiance::units::Acceleration2(Vec2::ZERO);
    app.update();

    // Body A moves; body B is tinted by A's speed through a binding (the
    // object-port model: a sensor port on A wired to an actuator port on B).
    let mut a = box_record(Vec2::ZERO, 20.0, 20.0);
    a.physics.rigid_body = avian2d::prelude::RigidBody::Dynamic;
    let a_id = a.id;
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    let b = box_record(Vec2::new(200.0, 0.0), 20.0, 20.0);
    let b_id = b.id;
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();

    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "wire".into(),
            source: SignalSource::Speed(a_id),
            map: SignalMap {
                in_min: 0.0,
                in_max: 200.0,
            },
            curve: None,
            gradient: GradientSpec::Turbo,
            sink: SignalSink::Fill(b_id),
        });

    // Drive A; the binding reads its speed and tints B's fill.
    let a_entity = entity_of(&app, a_id).unwrap();
    app.world_mut()
        .entity_mut(a_entity)
        .insert(LinearVelocity(Vec2::new(150.0, 0.0)));
    step(&mut app, 3);

    let b_entity = entity_of(&app, b_id).unwrap();
    assert!(
        app.world()
            .get::<SignalColorOverride>(b_entity)
            .is_some_and(|o| o.fill.is_some()),
        "the binding tinted B from A's speed"
    );
}

#[test]
fn a_pos_x_to_tracer_binding_drives_the_trail_channel() {
    use gradiance::domain::signal::{GradientSpec, SignalMap};
    use gradiance::signal::{
        SignalBinding, SignalBindings, SignalColorOverride, SignalSink, SignalSource,
    };

    let mut app = paused_app();
    let mut body = box_record(Vec2::new(300.0, 0.0), 20.0, 20.0);
    body.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();

    // A pos-x → tracer-color binding on the same body drives its *trail*.
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "px".into(),
            source: SignalSource::PosX(body_id),
            map: SignalMap {
                in_min: 0.0,
                in_max: 500.0,
            },
            curve: None,
            gradient: GradientSpec::Turbo,
            sink: SignalSink::TracerColor(body_id),
        });
    step(&mut app, 3);

    let entity = entity_of(&app, body_id).unwrap();
    let over = app.world().get::<SignalColorOverride>(entity).unwrap();
    assert!(over.trail.is_some(), "the binding drove the trail channel");
    assert!(over.fill.is_none(), "fill left untouched by a tracer sink");
}

#[test]
fn deleting_a_body_cascades_its_attached_nodes() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let node = tracer_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO));
    let node_id = node.id;
    app.world_mut()
        .write_message(SpawnNodeIntent { record: node });
    app.update();
    assert_eq!(node_count(&mut app), 1);

    // Delete the body — the attached node goes too (like a joint cascade).
    app.world_mut().write_message(DeleteIntent {
        targets: vec![body_id],
    });
    app.update();
    assert_eq!(
        node_count(&mut app),
        0,
        "attached node deleted with its body"
    );
    assert!(entity_of(&app, node_id).is_none());

    // Undo restores both the body and its node.
    undo(&mut app);
    assert!(entity_of(&app, body_id).is_some(), "body back");
    assert!(entity_of(&app, node_id).is_some(), "node back");
    assert_eq!(node_count(&mut app), 1);
}

#[test]
fn a_free_tracer_node_does_nothing() {
    use gradiance::render::tracer::TraceTrail;
    let mut app = headless_app();
    // A free tracer (no attachment) — inert, no trail accumulates.
    let free = tracer_record(Vec2::new(50.0, 50.0), NodeAttachment::free());
    let free_id = free.id;
    app.world_mut()
        .write_message(SpawnNodeIntent { record: free });
    step(&mut app, 20);
    let entity = entity_of(&app, free_id).unwrap();
    assert!(
        app.world().get::<TraceTrail>(entity).is_none(),
        "a free tracer accumulates no trail"
    );
}

#[test]
fn a_body_despawned_while_its_override_is_queued_does_not_panic() {
    // The flake CI hit on #223 (and likely #221's first-run failure):
    // `evaluate_signals` and the command dispatcher are unordered in
    // `Update`, so an undo can despawn a bound body after the evaluator
    // queued its colour-override insert and before that insert is applied.
    // Replay that order deterministically by driving the system by hand.
    use bevy::ecs::system::System;
    use gradiance::signal::{SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource};
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let entity = entity_of(&app, body_id).unwrap();
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "speed".into(),
            source: SignalSource::Speed(body_id),
            map: SignalMap::default(),
            curve: None,
            gradient: gradiance::signal::GradientSpec::default(),
            sink: SignalSink::Fill(body_id),
        });

    let mut evaluate = IntoSystem::into_system(gradiance::signal::evaluate_signals);
    let world = app.world_mut();
    evaluate.initialize(world);
    // The evaluator sees the live body and queues its override insert...
    evaluate.run_without_applying_deferred((), world).unwrap();
    // ...the dispatcher's undo despawns the body...
    world.despawn(entity);
    // ...and the queued insert lands on a dead entity. Panicked before.
    evaluate.apply_deferred(world);
    assert!(app.world().get_entity(entity).is_err());
    app.update(); // the next ordinary frame is clean too
}

// --- dragging a node (brief: gizmo polish) ----------------------------
//
// A press on a node's glyph selects it; dragging past the deadzone moves
// it, and the release re-attaches it to the body under the release point
// or leaves it free on empty canvas — one undoable property edit. The
// Drag tool gives the same press the same meaning: the node moves, not
// the body behind it.

use gradiance::command::CommandStack;
use gradiance::command::intent::SpawnBodyIntent;
use gradiance::domain::laser::Laser;
use gradiance::domain::thruster::Thruster;
use gradiance::interaction::cursor::CursorWorldPos;
use gradiance::interaction::node_edit::NodeDrag;
use gradiance::interaction::snap::SnappedCursor;

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn mouse(app: &mut App, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(MouseButton::Left);
    } else {
        input.release(MouseButton::Left);
    }
}

fn thruster_record(pos: Vec2, attach: NodeAttachment) -> NodeRecord {
    let id = StableId::new();
    NodeRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        attachment: attach,
        kind: NodeKind::Thruster(Thruster::default()),
        appearance: gradiance::domain::appearance::Appearance::default(),
    }
}

/// Drag a thruster off its body onto empty canvas: it becomes a free node
/// at the release point, as one undo step that restores both the
/// attachment and the position.
#[test]
fn dragging_a_node_off_its_body_makes_it_free_and_undoes() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let record = thruster_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO));
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    let before = app.world().resource::<CommandStack>().undo_len();

    // Press on the node: selected, drag armed, nothing moved yet.
    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, true);
    app.update();
    let node = entity_of(&app, node_id).unwrap();
    assert!(
        app.world().resource::<Selection>().contains(node),
        "the press selected the node"
    );
    assert_eq!(app.world().resource::<NodeDrag>().dragging(), Some(node));

    // Drag it well past the deadzone, onto empty canvas, and release.
    set_cursor(&mut app, Vec2::new(0.0, 60.0));
    app.update();
    // The engage lands at the end of that frame; the preview publishes
    // from the next one.
    app.update();
    assert!(
        app.world().resource::<NodeDrag>().engaged()
            && app.world().resource::<NodeDrag>().cursor == Some(Vec2::new(0.0, 60.0)),
        "the glyph previews at the cursor"
    );
    mouse(&mut app, false);
    app.update();

    let (attachment, pos) = node_placement(&app, node_id);
    assert_eq!(attachment.target, None, "empty canvas: free");
    assert_eq!(pos, Vec2::new(0.0, 60.0));
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before + 1,
        "one step for the whole gesture"
    );

    undo(&mut app);
    let (attachment, pos) = node_placement(&app, node_id);
    assert_eq!(attachment.target, Some(body_id), "undo restores the ride");
    assert_eq!(pos, Vec2::ZERO, "and the position");
}

/// A plain click on a node (no deadzone-crossing drag) selects it and
/// commits nothing.
#[test]
fn clicking_a_node_selects_it_without_moving_it() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let record = thruster_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO));
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    let before = app.world().resource::<CommandStack>().undo_len();

    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, true);
    app.update();
    mouse(&mut app, false);
    app.update();

    let (attachment, pos) = node_placement(&app, node_id);
    assert_eq!(attachment.target, Some(body_id), "still attached");
    assert_eq!(pos, Vec2::ZERO, "never nudged");
    assert_eq!(app.world().resource::<CommandStack>().undo_len(), before);
}

/// Releasing over another body re-attaches to it, offset in its frame,
/// with a laser's axis re-expressed so its world aim is unchanged.
#[test]
fn releasing_over_a_rotated_body_re_expresses_the_axis() {
    let mut app = paused_app();
    let mut rider = box_record(Vec2::new(60.0, 0.0), 20.0, 20.0);
    rider.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    rider.pose.rot = 0.5;
    let rider_id = rider.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: rider });
    app.update();
    let mut record = NodeRecord {
        id: StableId::new(),
        pose: PosRot {
            pos: Vec2::ZERO,
            rot: 0.0,
        },
        attachment: NodeAttachment::free(),
        kind: NodeKind::Laser(Laser::default()),
        appearance: gradiance::domain::appearance::Appearance::default(),
    };
    record.kind = NodeKind::Laser(Laser {
        angle: gradiance::units::Angle(0.0),
        ..Laser::default()
    });
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();

    // Drag the free laser onto the rotated body's centre.
    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(60.0, 0.0));
    app.update();
    mouse(&mut app, false);
    app.update();

    let (attachment, pos) = node_placement(&app, node_id);
    assert_eq!(
        attachment.target,
        Some(rider_id),
        "re-attached to the rider"
    );
    assert_eq!(attachment.offset.value(), Vec2::ZERO, "at its centre");
    assert_eq!(pos, Vec2::new(60.0, 0.0));
    let node = entity_of(&app, node_id).unwrap();
    let NodeKind::Laser(laser) = app.world().get::<NodeKind>(node).unwrap() else {
        panic!("still a laser");
    };
    assert!(
        (laser.angle.value() + 0.5).abs() < 1e-4,
        "world aim unchanged: the frame angle cancels the body's 0.5 ({})",
        laser.angle.value()
    );
}

/// The Drag tool moves the node under its press, not the body behind it.
#[test]
fn the_drag_tool_moves_the_node_not_the_body_behind_it() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 40.0, 40.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let record = thruster_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO));
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    let body_entity = entity_of(&app, body_id).unwrap();

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Drag);
    app.update();

    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(0.0, 80.0));
    app.update();
    mouse(&mut app, false);
    app.update();

    let (attachment, pos) = node_placement(&app, node_id);
    assert_eq!(attachment.target, None, "the node came off its body");
    assert_eq!(pos, Vec2::new(0.0, 80.0));
    let body_pos = app
        .world()
        .get::<Transform>(body_entity)
        .unwrap()
        .translation
        .truncate();
    assert_eq!(body_pos, Vec2::ZERO, "the body behind it never moved");
}

/// One node's `(attachment, pose)` as the tests read it.
fn node_placement(app: &App, id: StableId) -> (NodeAttachment, Vec2) {
    let node = entity_of(app, id).unwrap();
    (
        *app.world().get::<NodeAttachment>(node).unwrap(),
        app.world()
            .get::<Transform>(node)
            .unwrap()
            .translation
            .truncate(),
    )
}

// --- a node dies with its body, on every path that consumes one ---------
//
// The delete command already cascaded; cut and the booleans rewired joints
// but left a node attached to a despawned body — orphaned, floating where
// its body used to be. Now every body-consuming path shares the joints'
// rule: a node moves onto what holds its point, or goes with its material.

use gradiance::command::intent::{BooleanIntent, CutIntent};

fn cut(app: &mut App, a: Vec2, b: Vec2, width: f32) {
    app.world_mut().write_message(CutIntent { a, b, width });
    app.update();
}

fn attachment_of(app: &App, id: StableId) -> NodeAttachment {
    *app.world()
        .get::<NodeAttachment>(entity_of(app, id).unwrap())
        .unwrap()
}

/// A cut through a thrustered body re-attaches the thruster to the piece
/// that holds its point; a cut that swallows the point deletes the node.
#[test]
fn a_cut_rewires_or_deletes_the_nodes_on_the_body() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 40.0, 40.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let record = thruster_record(
        Vec2::new(0.0, 10.0),
        NodeAttachment::to(body_id, Vec2::new(0.0, 10.0)),
    );
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    assert_eq!(node_count(&mut app), 1);

    // Split the box under the thruster: the top half keeps its point.
    cut(&mut app, Vec2::new(-30.0, 0.0), Vec2::new(30.0, 0.0), 2.0);
    assert_eq!(node_count(&mut app), 1, "the node survived the split");
    let moved = attachment_of(&app, node_id);
    let piece = moved.target.expect("re-attached to a piece");
    assert_ne!(piece, body_id, "the original body is gone");
    let piece_shape = app
        .world()
        .get::<gradiance::domain::shape::ShapeDef>(entity_of(&app, piece).unwrap())
        .unwrap()
        .clone();
    assert!(
        gradiance::geometry::sdf::eval(&piece_shape, moved.offset.value()) < 1.0,
        "the piece contains the node's point (offset {:?})",
        moved.offset.value()
    );

    // A strip wide enough to swallow the point: the node goes with it.
    cut(
        &mut app,
        Vec2::new(-30.0, 11.0),
        Vec2::new(30.0, 11.0),
        24.0,
    );
    assert_eq!(
        node_count(&mut app),
        0,
        "no piece holds the node: deleted with its material"
    );

    // The snapshot undoes both cuts and the node comes back on its body.
    undo(&mut app);
    undo(&mut app);
    assert_eq!(node_count(&mut app), 1);
    assert_eq!(
        attachment_of(&app, node_id).target,
        Some(body_id),
        "undo restores the original attachment"
    );
}

/// A subtract consumes its tool body: a node on the consumed body moves
/// onto the host (its axis re-expressed so its world direction never
/// changes), and a node on the material the subtract removed goes with it.
#[test]
fn a_boolean_moves_consumed_nodes_onto_the_host() {
    let mut app = paused_app();
    // Host: a box at the origin. Tool: an overlapping box to its right.
    let host = box_record(Vec2::ZERO, 40.0, 40.0);
    let host_id = host.id;
    let mut tool = box_record(Vec2::new(20.0, 0.0), 40.0, 40.0);
    tool.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    tool.pose.rot = 0.25;
    let tool_id = tool.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: host });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: tool });
    app.update();

    // A laser on the tool, far off its material: subtract moves it onto
    // the host, aiming the same world way (the tool was rotated 0.25).
    let record = NodeRecord {
        id: StableId::new(),
        pose: PosRot {
            pos: Vec2::new(-10.0, 0.0),
            rot: 0.0,
        },
        attachment: NodeAttachment::to(tool_id, Vec2::new(-30.0, 0.0)),
        kind: NodeKind::Laser(Laser {
            angle: gradiance::units::Angle(0.0),
            ..Laser::default()
        }),
        appearance: gradiance::domain::appearance::Appearance::default(),
    };
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    // A tracer on the tool's own material, inside the subtracted overlap:
    // the material vanishes, and the tracer with it.
    let swallowed = tracer_record(
        Vec2::new(20.0, 0.0),
        NodeAttachment::to(tool_id, Vec2::ZERO),
    );
    let swallowed_id = swallowed.id;
    app.world_mut()
        .write_message(SpawnNodeIntent { record: swallowed });
    app.update();

    app.world_mut().write_message(BooleanIntent {
        op: gradiance::domain::shape::CsgOp::Subtract,
        targets: vec![host_id, tool_id],
    });
    app.update();

    let moved = attachment_of(&app, node_id);
    assert_eq!(moved.target, Some(host_id), "the laser rides the host now");
    let laser = app
        .world()
        .get::<NodeKind>(entity_of(&app, node_id).unwrap())
        .unwrap();
    let NodeKind::Laser(l) = laser else {
        panic!("still a laser: {laser:?}");
    };
    assert!(
        (l.angle.value() - 0.25).abs() < 1e-4,
        "world aim unchanged: the frame angle advanced by the tool's rotation ({})",
        l.angle.value()
    );
    assert!(
        entity_of(&app, swallowed_id).is_none(),
        "the tracer on the removed material went with it"
    );

    undo(&mut app);
    assert_eq!(
        attachment_of(&app, node_id).target,
        Some(tool_id),
        "undo restores the pre-boolean attachment"
    );
    assert!(entity_of(&app, swallowed_id).is_some(), "and the tracer");
}

//! Selectable-joint contracts: pick a joint by its anchor, edit its
//! config through the property path, delete it, and drag its anchor —
//! all undoable, all headless.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::domain::joint::MotorDef;
use gradiance::interaction::selection::{SelectedJoint, Selection};
use gradiance::prelude::*;
use harness::{box_record, entity_of, paused_app};

fn spawn_box_at(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, def: JointDef) -> StableId {
    let record = JointRecord {
        id: StableId::new(),
        def,
    };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn hinge_pin(body_a: StableId, anchor_world: Vec2, anchor_a: Vec2) -> JointDef {
    JointDef {
        kind: JointKind::Hinge {
            limits: None,
            motor: None,
        },
        common: JointCommon::default(),
        body_a,
        body_b: None,
        anchor_a,
        anchor_b: anchor_world,
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    }
}

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

fn click_at(app: &mut App, p: Vec2) {
    set_cursor(app, p);
    mouse(app, true);
    app.update();
    mouse(app, false);
    app.update();
}

fn selected_joint(app: &App) -> Option<Entity> {
    app.world().resource::<SelectedJoint>().0
}

#[test]
fn clicking_an_anchor_selects_the_joint_and_clears_body_selection() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    // Pin anchored at the body's right edge, world (20,0), body-local (20,0).
    let joint = spawn_joint(
        &mut app,
        hinge_pin(body, Vec2::new(20.0, 0.0), Vec2::new(20.0, 0.0)),
    );

    // Select the body first.
    let body_entity = entity_of(&app, body).unwrap();
    app.world_mut().resource_mut::<Selection>().set(body_entity);

    // Click the anchor glyph.
    click_at(&mut app, Vec2::new(20.0, 0.0));

    let joint_entity = entity_of(&app, joint).unwrap();
    assert_eq!(
        selected_joint(&app),
        Some(joint_entity),
        "the joint is now selected"
    );
    assert!(
        app.world().resource::<Selection>().is_empty(),
        "body selection cleared by the joint pick"
    );

    // Clicking the body (away from the anchor) deselects the joint.
    click_at(&mut app, Vec2::new(-10.0, 0.0));
    assert_eq!(selected_joint(&app), None, "joint deselected by body click");
}

#[test]
fn editing_a_selected_joint_adds_a_motor_and_undoes() {
    // The UI is not headless, but it emits the same PropertyEditIntent
    // the command layer applies — exercise that path directly.
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let joint = spawn_joint(
        &mut app,
        hinge_pin(body, Vec2::new(20.0, 0.0), Vec2::new(20.0, 0.0)),
    );
    let entity = entity_of(&app, joint).unwrap();

    let old = app.world().get::<JointDef>(entity).unwrap().clone();
    let mut new = old.clone();
    new.kind = JointKind::Hinge {
        limits: None,
        motor: Some(MotorDef::default()),
    };
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: joint,
            old: PropertyValue::Joint(old.clone()),
            new: PropertyValue::Joint(new),
        }],
    });
    app.update();

    let def = app.world().get::<JointDef>(entity).unwrap().clone();
    assert!(
        matches!(def.kind, JointKind::Hinge { motor: Some(_), .. }),
        "motor added"
    );

    app.world_mut().write_message(UndoIntent);
    app.update();
    assert_eq!(
        app.world().get::<JointDef>(entity).unwrap().clone(),
        old,
        "undo restores the motorless hinge"
    );
}

#[test]
fn deleting_a_selected_joint_removes_it_and_undoes() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let joint = spawn_joint(
        &mut app,
        hinge_pin(body, Vec2::new(20.0, 0.0), Vec2::new(20.0, 0.0)),
    );

    app.world_mut()
        .write_message(DeleteJointIntent { id: joint });
    app.update();
    assert!(entity_of(&app, joint).is_none(), "joint deleted");
    // The body survives — deleting a joint is not a body cascade.
    assert!(entity_of(&app, body).is_some());

    app.world_mut().write_message(UndoIntent);
    app.update();
    let restored = entity_of(&app, joint).expect("undo restores the joint");
    let def = app.world().get::<JointDef>(restored).unwrap();
    assert_eq!(def.body_a, body, "restored with its endpoints");
}

#[test]
fn dragging_a_selected_anchor_relocates_it_in_pause_mode() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let joint = spawn_joint(
        &mut app,
        hinge_pin(body, Vec2::new(20.0, 0.0), Vec2::new(20.0, 0.0)),
    );
    let entity = entity_of(&app, joint).unwrap();
    let depth = app.world().resource::<CommandStack>().undo_len();

    // Press on the anchor (selects + arms drag), move well past the
    // deadzone, release.
    set_cursor(&mut app, Vec2::new(20.0, 0.0));
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(20.0, 15.0));
    app.update();
    mouse(&mut app, false);
    app.update();

    assert_eq!(
        selected_joint(&app),
        Some(entity),
        "the joint stayed selected through the drag"
    );
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth + 1,
        "one undoable anchor move"
    );
    let def = app.world().get::<JointDef>(entity).unwrap();
    // World pin: anchor_b is the world point, now at the release position.
    assert!(
        (def.anchor_b - Vec2::new(20.0, 15.0)).length() < 1.0,
        "world anchor followed the cursor (got {})",
        def.anchor_b
    );
    // anchor_a re-expressed in body-local (body at origin, unrotated).
    assert!(
        (def.anchor_a - Vec2::new(20.0, 15.0)).length() < 1.0,
        "body-local anchor updated (got {})",
        def.anchor_a
    );
}

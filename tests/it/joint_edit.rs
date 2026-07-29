//! Selectable-joint contracts: pick a joint by its anchor, edit its
//! config through the property path, delete it, and drag its anchor —
//! all undoable, all headless.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, paused_app, undo};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::domain::joint::AngularMotorDef;
use gradiance::interaction::selection::{SelectedJoint, Selection};
use gradiance::prelude::*;

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
        body: None,
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
    // A wide body so a body-click well away from the anchor lands clearly
    // outside the anchor glyph's (screen-scaled) pick radius.
    let body = spawn_box_at(&mut app, Vec2::ZERO, 120.0, 40.0);
    // Pin anchored at world (20,0), body-local (20,0).
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

    // Clicking the body (well away from the anchor) deselects the joint.
    click_at(&mut app, Vec2::new(-50.0, 0.0));
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
        motor: Some(AngularMotorDef::default()),
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
    let entity = entity_of(&app, joint).unwrap();
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

fn prismatic_pin(body_a: StableId, limits: Option<[f32; 2]>) -> JointDef {
    JointDef {
        kind: JointKind::Slider {
            axis: Vec2::X,
            limits,
            motor: None,
        },
        common: JointCommon::default(),
        body_a,
        body_b: None,
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    }
}

#[test]
fn joints_are_selectable_anywhere_on_their_glyph() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 20.0, 20.0);
    let joint = spawn_joint(&mut app, prismatic_pin(body, Some([0.0, 120.0])));
    let entity = entity_of(&app, joint).unwrap();

    // Click far from the anchor but on the drawn travel line: the whole
    // glyph is selectable, not just the anchor point.
    click_at(&mut app, Vec2::new(90.0, 3.0));
    assert_eq!(
        selected_joint(&app),
        Some(entity),
        "glyph-wide pick selects the joint"
    );
}

#[test]
fn dragging_a_travel_cap_resizes_prismatic_limits() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 20.0, 20.0);
    let joint = spawn_joint(&mut app, prismatic_pin(body, Some([0.0, 120.0])));
    let entity = entity_of(&app, joint).unwrap();

    // Select it, then grab the max travel cap and drag it out.
    click_at(&mut app, Vec2::new(90.0, 3.0));
    let depth = app.world().resource::<CommandStack>().undo_len();
    set_cursor(&mut app, Vec2::new(120.0, 0.0));
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(200.0, 0.0));
    app.update();
    mouse(&mut app, false);
    app.update();

    let def = app.world().get::<JointDef>(entity).unwrap();
    let JointKind::Slider { limits, .. } = def.kind else {
        panic!("kind changed: {:?}", def.kind);
    };
    let [min, max] = limits.expect("limits kept");
    assert!(min.abs() < 1.0, "min untouched ({min})");
    assert!(
        (max - 200.0).abs() < 10.0,
        "max followed the cursor ({max})"
    );
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth + 1,
        "one undoable limit edit"
    );

    undo(&mut app);
    let entity = entity_of(&app, joint).unwrap();
    let def = app.world().get::<JointDef>(entity).unwrap();
    let JointKind::Slider { limits, .. } = def.kind else {
        panic!("kind changed: {:?}", def.kind);
    };
    assert_eq!(limits, Some([0.0, 120.0]), "undo restores the old travel");
}

#[test]
fn dragging_a_hinge_arc_handle_changes_angle_limits() {
    let mut app = paused_app();
    let body = spawn_box_at(&mut app, Vec2::ZERO, 20.0, 20.0);
    let mut def = hinge_pin(body, Vec2::ZERO, Vec2::ZERO);
    def.kind = JointKind::Hinge {
        limits: Some([-0.5, 0.5]),
        motor: None,
    };
    let joint = spawn_joint(&mut app, def);
    let entity = entity_of(&app, joint).unwrap();

    // Select (click on the ring), then grab the max-angle handle (on the
    // limit arc at radius 14) and swing it out to ~1.2 rad.
    click_at(&mut app, Vec2::ZERO);
    let depth = app.world().resource::<CommandStack>().undo_len();
    set_cursor(&mut app, Vec2::from_angle(0.5) * 14.0);
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::from_angle(1.2) * 60.0);
    app.update();
    mouse(&mut app, false);
    app.update();

    let def = app.world().get::<JointDef>(entity).unwrap();
    let JointKind::Hinge { limits, .. } = def.kind else {
        panic!("kind changed: {:?}", def.kind);
    };
    let [min, max] = limits.expect("limits kept");
    assert!((min + 0.5).abs() < 1e-3, "min untouched ({min})");
    assert!((max - 1.2).abs() < 0.15, "max followed the cursor ({max})");
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth + 1,
        "one undoable limit edit"
    );
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

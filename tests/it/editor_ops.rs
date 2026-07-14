//! Editor-operation contracts: property edits, grouping, lasso/box
//! selection semantics (with and without constraints), and settings
//! application — everything below the egui skin, tested headless.

use crate::harness::{body_count, box_record, entity_of, headless_app, paused_app, step, undo};
use avian2d::prelude::Friction;
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::prelude::*;

fn spawn_box(app: &mut App, pos: Vec2) -> StableId {
    let record = box_record(pos, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
}

fn mouse(app: &mut App, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(MouseButton::Left);
    } else {
        input.release(MouseButton::Left);
    }
}

fn joint_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<Joint>>()
        .iter(app.world())
        .count()
}

#[test]
fn property_edit_batches_multi_target_into_one_undo_step() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let b = spawn_box(&mut app, Vec2::new(100.0, 0.0));
    let before = app.world().resource::<CommandStack>().undo_len();

    let changes: Vec<PropertyChange> = [a, b]
        .into_iter()
        .map(|id| {
            let entity = entity_of(&app, id).unwrap();
            let old = *app.world().get::<Friction>(entity).unwrap();
            let new = Friction {
                dynamic_coefficient: 0.9,
                static_coefficient: 0.9,
                ..old
            };
            PropertyChange {
                id,
                old: PropertyValue::Friction(old),
                new: PropertyValue::Friction(new),
            }
        })
        .collect();
    app.world_mut()
        .write_message(PropertyEditIntent { changes });
    app.update();

    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before + 1,
        "one command for the whole batch"
    );
    for id in [a, b] {
        let entity = entity_of(&app, id).unwrap();
        assert!(
            (app.world()
                .get::<Friction>(entity)
                .unwrap()
                .dynamic_coefficient
                - 0.9)
                .abs()
                < 1e-6
        );
    }
    undo(&mut app);
    for id in [a, b] {
        let entity = entity_of(&app, id).unwrap();
        assert!(
            (app.world()
                .get::<Friction>(entity)
                .unwrap()
                .dynamic_coefficient
                - 0.5)
                .abs()
                < 1e-6,
            "undo restored both targets"
        );
    }
}

#[test]
fn depth_band_edit_round_trips_and_undoes() {
    // The depth panel commits band changes through the same
    // PropertyEditIntent seam as every other property.
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let entity = entity_of(&app, a).unwrap();
    let old_band = *app.world().get::<DepthBand>(entity).unwrap();
    let new_band = DepthBand {
        near: 10.0,
        far: 32.5, // non-integer depth is first-class
    };

    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: a,
            old: PropertyValue::Depth(old_band),
            new: PropertyValue::Depth(new_band),
        }],
    });
    app.update();
    assert_eq!(*app.world().get::<DepthBand>(entity).unwrap(), new_band);

    undo(&mut app);
    assert_eq!(
        *app.world().get::<DepthBand>(entity).unwrap(),
        old_band,
        "undo restores the depth band"
    );
}

#[test]
fn field_property_edit_round_trips_and_undoes() {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let entity = entity_of(&app, a).unwrap();
    assert!(app.world().get::<FieldSource>(entity).is_none());

    let field = FieldSource {
        strength: -750.0,
        falloff: FieldFalloff::Linear,
    };
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: a,
            old: PropertyValue::Field(None),
            new: PropertyValue::Field(Some(field)),
        }],
    });
    app.update();
    assert_eq!(app.world().get::<FieldSource>(entity).copied(), Some(field));

    undo(&mut app);
    assert!(
        app.world().get::<FieldSource>(entity).is_none(),
        "undo removes the field"
    );
}

#[test]
fn invalid_shape_in_a_property_batch_refuses_the_whole_command() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let before = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: a,
            old: PropertyValue::Shape(ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            }),
            new: PropertyValue::Shape(ShapeDef::Box {
                width: 0.0,
                height: 20.0,
            }),
        }],
    });
    app.update();

    assert_eq!(app.world().resource::<CommandStack>().undo_len(), before);
    let entity = entity_of(&app, a).unwrap();
    assert_eq!(
        app.world().get::<ShapeDef>(entity).unwrap().clone(),
        ShapeDef::Box {
            width: 40.0,
            height: 20.0
        }
    );
}

#[test]
fn grouping_makes_click_select_the_whole_group_and_undoes() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let b = spawn_box(&mut app, Vec2::new(100.0, 0.0));
    app.update();

    app.world_mut().write_message(GroupIntent {
        targets: vec![a, b],
    });
    app.update();

    let (ea, eb) = (entity_of(&app, a).unwrap(), entity_of(&app, b).unwrap());
    let ga = app.world().get::<SelectionGroup>(ea).map(|g| g.0.clone());
    let gb = app.world().get::<SelectionGroup>(eb).map(|g| g.0.clone());
    assert!(ga.is_some() && ga == gb, "same fresh group stack");

    // CONTRACT: clicking one member selects the whole group.
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse(&mut app, true);
    app.update();
    mouse(&mut app, false);
    app.update();
    assert_eq!(app.world().resource::<Selection>().len(), 2);

    // Ungroup, then undo restores the grouping.
    app.world_mut().write_message(UngroupIntent {
        targets: vec![a, b],
    });
    app.update();
    assert!(app.world().get::<SelectionGroup>(ea).is_none());
    undo(&mut app);
    assert!(app.world().get::<SelectionGroup>(ea).is_some());
}

#[test]
fn lasso_selects_bodies_whose_centers_are_inside_the_loop() {
    let mut app = paused_app();
    spawn_box(&mut app, Vec2::new(0.0, 0.0));
    spawn_box(&mut app, Vec2::new(60.0, 0.0));
    spawn_box(&mut app, Vec2::new(500.0, 500.0));
    app.update();

    // Alt-drag a loop around the first two bodies.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::AltLeft);
    set_cursor(&mut app, Vec2::new(-100.0, -100.0));
    mouse(&mut app, true);
    app.update();
    for p in [
        Vec2::new(200.0, -100.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(-100.0, 100.0),
    ] {
        set_cursor(&mut app, p);
        app.update();
    }
    mouse(&mut app, false);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::AltLeft);

    assert_eq!(
        app.world().resource::<Selection>().len(),
        2,
        "loop contains exactly the two nearby centers"
    );
}

#[test]
fn box_selected_assembly_deletes_and_restores_with_its_constraints() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let b = spawn_box(&mut app, Vec2::new(30.0, 0.0));
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: a,
                body_b: Some(b),
                anchor_a: Vec2::new(15.0, 0.0),
                anchor_b: Vec2::new(-15.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    // Box-select the assembly on empty canvas.
    set_cursor(&mut app, Vec2::new(-100.0, -100.0));
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(150.0, 100.0));
    app.update();
    mouse(&mut app, false);
    app.update();
    assert_eq!(app.world().resource::<Selection>().len(), 2);

    // CONTRACT: deleting a box-selected assembly removes its joints too,
    // and a single undo restores bodies *and* constraints.
    let targets: Vec<StableId> = vec![a, b];
    app.world_mut().write_message(DeleteIntent { targets });
    app.update();
    assert_eq!(body_count(&mut app), 0);
    assert_eq!(joint_count(&mut app), 0);
    undo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert_eq!(joint_count(&mut app), 1);
}

#[test]
fn box_selected_assembly_duplicates_with_its_constraints() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO);
    let b = spawn_box(&mut app, Vec2::new(30.0, 0.0));
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: a,
                body_b: Some(b),
                anchor_a: Vec2::new(15.0, 0.0),
                anchor_b: Vec2::new(-15.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    // CONTRACT: duplicating a multi-selection carries internal joints.
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![a, b],
        offset: Vec2::new(300.0, 0.0),
    });
    app.update();
    assert_eq!(body_count(&mut app), 4);
    assert_eq!(joint_count(&mut app), 2);
}

#[test]
fn sim_settings_gravity_is_applied_by_the_physics_seam() {
    let mut app = headless_app();
    let id = spawn_box(&mut app, Vec2::ZERO);
    {
        let mut sim = app
            .world_mut()
            .resource_mut::<gradiance::domain::settings::SimSettings>();
        sim.gravity = Vec2::new(0.0, 800.0); // upward!
    }
    step(&mut app, 120);
    let entity = entity_of(&app, id).unwrap();
    let y = app.world().get::<Transform>(entity).unwrap().translation.y;
    assert!(y > 50.0, "body rose under inverted gravity (y = {y})");
}

/// Duplicating a grouped assembly must create an independent group —
/// clones joining the originals' group was the "group selection
/// deteriorates after repeated operations" bug (feedback 2.3).
#[test]
fn duplicated_groups_are_independent_of_their_source_group() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::new(-30.0, 0.0));
    let b = spawn_box(&mut app, Vec2::new(30.0, 0.0));
    app.world_mut().write_message(GroupIntent {
        targets: vec![a, b],
    });
    app.update();

    app.world_mut().write_message(DuplicateIntent {
        sources: vec![a, b],
        offset: Vec2::new(200.0, 0.0),
    });
    app.update();

    let mut groups: Vec<u32> = app
        .world_mut()
        .query::<&SelectionGroup>()
        .iter(app.world())
        .filter_map(SelectionGroup::outermost)
        .collect();
    groups.sort_unstable();
    assert_eq!(groups.len(), 4, "both pairs grouped");
    assert_eq!(
        groups[0], groups[1],
        "originals share a group; clones share a group"
    );
    assert_eq!(groups[2], groups[3]);
    assert_ne!(
        groups[0], groups[2],
        "clone group must be distinct from the source group"
    );
}

/// Hierarchical grouping: `ungroup(group(group(A,B,C), D))` must leave
/// the inner group `(A,B,C)` intact — the user's exact contract.
#[test]
fn ungroup_peels_only_the_outermost_group() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::new(0.0, 0.0));
    let b = spawn_box(&mut app, Vec2::new(30.0, 0.0));
    let c = spawn_box(&mut app, Vec2::new(60.0, 0.0));
    let d = spawn_box(&mut app, Vec2::new(200.0, 0.0));
    app.update();

    // Inner group over A, B, C.
    app.world_mut().write_message(GroupIntent {
        targets: vec![a, b, c],
    });
    app.update();
    // Outer group wrapping the inner assembly plus D.
    app.world_mut().write_message(GroupIntent {
        targets: vec![a, b, c, d],
    });
    app.update();

    let stack = |app: &App, id| {
        entity_of(app, id)
            .and_then(|e| app.world().get::<SelectionGroup>(e).map(|g| g.0.clone()))
            .unwrap_or_default()
    };
    assert_eq!(stack(&app, a).len(), 2, "A is in inner + outer group");
    assert_eq!(stack(&app, d).len(), 1, "D is only in the outer group");
    // Every member shares the same outermost id.
    let outer = *stack(&app, a).last().unwrap();
    assert_eq!(stack(&app, d).last(), Some(&outer));

    // Ungroup the whole outer selection.
    app.world_mut().write_message(UngroupIntent {
        targets: vec![a, b, c, d],
    });
    app.update();

    // Inner group survives on A, B, C; D is fully ungrouped.
    let inner = stack(&app, a);
    assert_eq!(inner.len(), 1, "A keeps its inner group");
    assert_eq!(stack(&app, b), inner, "B shares the inner group");
    assert_eq!(stack(&app, c), inner, "C shares the inner group");
    assert!(stack(&app, d).is_empty(), "D is fully ungrouped");

    // Clicking A now selects only the inner trio, not D.
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse(&mut app, true);
    app.update();
    mouse(&mut app, false);
    app.update();
    assert_eq!(
        app.world().resource::<Selection>().len(),
        3,
        "inner group selects A, B, C — not D"
    );

    undo(&mut app);
    assert_eq!(stack(&app, d).len(), 1, "undo restores D's outer group");
}

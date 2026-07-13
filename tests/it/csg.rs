//! CSG cut contracts: subtract trees, splitting, joint reattachment,
//! undo restoration, and a randomized conservation proptest.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{body_count, box_record, entity_of, paused_app, redo, undo};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::geometry::polygonize::polygonize_components;
use gradiance::prelude::*;

fn spawn_box_at(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn cut(app: &mut App, a: Vec2, b: Vec2, width: f32) {
    app.world_mut().write_message(CutIntent { a, b, width });
    app.update();
}

/// Total authored area across all bodies (excluding the infinite ground).
fn total_area(app: &mut App) -> f32 {
    let shapes: Vec<ShapeDef> = app
        .world_mut()
        .query_filtered::<&ShapeDef, With<Body>>()
        .iter(app.world())
        .cloned()
        .collect();
    shapes
        .iter()
        .filter(|s| !s.contains_half_plane())
        .flat_map(polygonize_components)
        .map(|c| c.area())
        .sum()
}

#[test]
fn severing_a_box_splits_it_into_two_recentered_pieces() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 20.0);
    let before = total_area(&mut app);

    cut(&mut app, Vec2::new(0.0, -30.0), Vec2::new(0.0, 30.0), 4.0);

    assert_eq!(body_count(&mut app), 2, "box severed into two pieces");
    assert!(entity_of(&app, id).is_none(), "original body replaced");
    let after = total_area(&mut app);
    let ideal = before - 4.0 * 20.0;
    assert!(
        (after - ideal).abs() / ideal < 0.02,
        "area conserved minus the slit: {after} vs {ideal}"
    );
    // Pieces are recentered around their own centroids, left and right.
    let mut xs: Vec<f32> = app
        .world_mut()
        .query_filtered::<&Transform, With<Body>>()
        .iter(app.world())
        .map(|t| t.translation.x)
        .collect();
    xs.sort_by(f32::total_cmp);
    assert!(xs[0] < -20.0 && xs[1] > 20.0, "piece centers {xs:?}");

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1);
    let entity = entity_of(&app, id).expect("undo restores the original id");
    assert_eq!(
        app.world().get::<ShapeDef>(entity),
        Some(&ShapeDef::Box {
            width: 100.0,
            height: 20.0
        })
    );

    redo(&mut app);
    assert_eq!(body_count(&mut app), 2, "redo replays the same split");
}

#[test]
fn cut_pieces_inherit_velocity_and_spin() {
    use avian2d::prelude::{AngularVelocity, LinearVelocity};
    // Physics continuity (Algodoo 7.5): severing a moving, spinning body
    // hands each piece `v + ω × r` and the shared spin, so pieces keep
    // flying instead of freezing mid-air.
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 20.0);
    let entity = entity_of(&app, id).unwrap();
    let (v, omega) = (Vec2::new(30.0, 0.0), 2.0);
    app.world_mut()
        .entity_mut(entity)
        .insert((LinearVelocity(v), AngularVelocity(omega)));

    cut(&mut app, Vec2::new(0.0, -30.0), Vec2::new(0.0, 30.0), 4.0);
    assert_eq!(body_count(&mut app), 2);

    let mut pieces: Vec<(Vec2, Vec2, f32)> = app
        .world_mut()
        .query_filtered::<(&Transform, &LinearVelocity, &AngularVelocity), With<Body>>()
        .iter(app.world())
        .map(|(t, lv, av)| (t.translation.truncate(), lv.0, av.0))
        .collect();
    pieces.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    for (pos, piece_v, piece_omega) in &pieces {
        let expected = v + omega * pos.perp();
        assert!(
            (*piece_v - expected).length() < 1.0,
            "piece at {pos:?} inherits v + ω×r ({piece_v:?} vs {expected:?})"
        );
        assert!(
            (piece_omega - omega).abs() < 1e-3,
            "piece keeps the spin ({piece_omega} vs {omega})"
        );
    }
    // The two halves shear oppositely: the spin flings the left piece down
    // and the right piece up.
    assert!(pieces[0].1.y < -20.0 && pieces[1].1.y > 20.0, "{pieces:?}");
}

#[test]
fn partial_cuts_are_rejected_entirely() {
    // User contract: a cut only applies when it severs — no notches, no
    // microscopic thin features, and no undo entry for a failed stroke.
    let mut app = paused_app();
    let record = BodyRecord {
        shape: ShapeDef::Circle { radius: 50.0 },
        ..box_record(Vec2::ZERO, 1.0, 1.0)
    };
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    let depth = app.world().resource::<CommandStack>().undo_len();

    // A slot from above into the interior: would notch, not sever.
    cut(&mut app, Vec2::new(0.0, 70.0), Vec2::new(0.0, 30.0), 4.0);

    assert_eq!(body_count(&mut app), 1);
    let entity = entity_of(&app, id).expect("body untouched");
    assert_eq!(
        app.world().get::<ShapeDef>(entity),
        Some(&ShapeDef::Circle { radius: 50.0 }),
        "no notch is applied"
    );
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth,
        "rejected cuts leave history untouched"
    );
}

#[test]
fn severing_a_polygon_body_yields_polygon_pieces() {
    let mut app = paused_app();
    let square: Vec<Vec2> = vec![
        Vec2::new(-40.0, -40.0),
        Vec2::new(40.0, -40.0),
        Vec2::new(40.0, 40.0),
        Vec2::new(-40.0, 40.0),
    ];
    let record = BodyRecord {
        shape: ShapeDef::Polygon {
            outline: square,
            holes: vec![],
        },
        ..box_record(Vec2::ZERO, 1.0, 1.0)
    };
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    cut(&mut app, Vec2::new(0.0, -60.0), Vec2::new(0.0, 60.0), 6.0);

    assert_eq!(body_count(&mut app), 2, "square severed");
    let total = total_area(&mut app);
    assert!(
        (total - (6400.0 - 6.0 * 80.0)).abs() < 60.0,
        "area conserved minus the slit ({total})"
    );
}

#[test]
fn joints_reattach_to_the_piece_containing_their_anchor() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 20.0);
    let joint_def = JointDef {
        kind: JointKind::Hinge {
            limits: None,
            motor: None,
        },
        common: JointCommon::default(),
        body_a: id,
        body_b: None,
        anchor_a: Vec2::new(-40.0, 0.0),
        anchor_b: Vec2::new(-40.0, 0.0),
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    };
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: joint_def.clone(),
        },
    });
    app.update();

    cut(&mut app, Vec2::new(0.0, -30.0), Vec2::new(0.0, 30.0), 4.0);
    assert_eq!(body_count(&mut app), 2);

    let def = app
        .world_mut()
        .query::<&JointDef>()
        .single(app.world())
        .expect("joint survives the split")
        .clone();
    assert_ne!(def.body_a, id, "rewired to a piece");
    let host = entity_of(&app, def.body_a).expect("piece id resolves");
    let host_x = app.world().get::<Transform>(host).unwrap().translation.x;
    assert!(host_x < 0.0, "anchor at x=-40 lands in the left piece");
    // Anchor remapped into the piece frame: world position preserved.
    let world_anchor = host_x + def.anchor_a.x;
    assert!(
        (world_anchor + 40.0).abs() < 1.0,
        "world anchor preserved (got {world_anchor})"
    );

    undo(&mut app);
    let def = app
        .world_mut()
        .query::<&JointDef>()
        .single(app.world())
        .expect("undo restores the joint")
        .clone();
    assert_eq!(def, joint_def, "joint def restored verbatim");
}

#[test]
fn joints_whose_anchor_material_is_destroyed_are_deleted() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 20.0);
    // Anchor dead-center — exactly where the cut removes material.
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: id,
                body_b: None,
                anchor_a: Vec2::ZERO,
                anchor_b: Vec2::ZERO,
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    cut(&mut app, Vec2::new(0.0, -30.0), Vec2::new(0.0, 30.0), 6.0);
    let joints = app
        .world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .count();
    assert_eq!(joints, 0, "anchor material gone, joint deleted");

    undo(&mut app);
    let joints = app
        .world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .count();
    assert_eq!(joints, 1, "undo restores the deleted joint");
}

#[test]
fn a_cut_that_swallows_a_body_destroys_it() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::new(200.0, 0.0), 10.0, 10.0);
    spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);

    cut(&mut app, Vec2::new(180.0, 0.0), Vec2::new(220.0, 0.0), 20.0);
    assert_eq!(body_count(&mut app), 1, "small box swallowed");
    assert!(entity_of(&app, id).is_none());

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert!(entity_of(&app, id).is_some());
}

#[test]
fn a_cut_that_misses_everything_is_not_recorded() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let depth_before = app.world().resource::<CommandStack>().undo_len();

    cut(
        &mut app,
        Vec2::new(500.0, 500.0),
        Vec2::new(600.0, 500.0),
        4.0,
    );

    assert_eq!(body_count(&mut app), 1);
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth_before,
        "no-effect cuts leave history untouched"
    );
}

#[test]
fn csg_trees_survive_scene_round_trips() {
    let mut app = paused_app();
    let record = BodyRecord {
        // What future boolean-modeling ops produce: an analytic tree.
        shape: ShapeDef::Csg {
            op: CsgOp::Subtract,
            lhs: Box::new(ShapeDef::Circle { radius: 50.0 }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(0.0, 50.0),
                rot: 0.3,
                shape: Box::new(ShapeDef::Box {
                    width: 44.0,
                    height: 4.0,
                }),
            }),
        },
        ..box_record(Vec2::ZERO, 1.0, 1.0)
    };
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    let scene = SceneRecord::capture(app.world_mut());
    let text = gradiance::persist::to_ron(&scene).expect("serialize");
    let parsed = gradiance::persist::from_ron(&text).expect("parse");
    assert_eq!(scene, parsed, "tree shapes round-trip through RON");
    assert!(
        matches!(parsed.bodies[0].shape, ShapeDef::Csg { .. }),
        "the analytic tree is what got saved"
    );
}

// ---------- Randomized conservation ----------

mod conservation {
    use super::*;
    use proptest::prelude::*;

    fn arb_cut() -> impl Strategy<Value = (Vec2, Vec2, f32)> {
        (
            -80.0f32..80.0,
            -80.0f32..80.0,
            -80.0f32..80.0,
            -80.0f32..80.0,
            2.0f32..10.0,
        )
            .prop_map(|(ax, ay, bx, by, w)| (Vec2::new(ax, ay), Vec2::new(bx, by), w))
    }

    fn arb_body() -> impl Strategy<Value = (Vec2, f32, f32, f32)> {
        (
            -60.0f32..60.0,
            -60.0f32..60.0,
            10.0f32..80.0,
            10.0f32..80.0,
            0.0f32..std::f32::consts::TAU,
        )
            .prop_map(|(x, y, w, h, rot)| (Vec2::new(x, y), w, h, rot))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(12))]

        /// Any sequence of cuts on any boxes: area never grows, joints
        /// never dangle, and undoing every cut restores the exact scene.
        #[test]
        fn cuts_conserve_area_and_undo_exactly(
            bodies in proptest::collection::vec(arb_body(), 1..4),
            cuts in proptest::collection::vec(arb_cut(), 1..3),
        ) {
            let mut app = paused_app();
            let mut first_id = None;
            for (pos, w, h, rot) in bodies {
                let mut record = box_record(pos, w, h);
                record.pose.rot = rot;
                first_id.get_or_insert(record.id);
                app.world_mut().write_message(SpawnBodyIntent { record });
                app.update();
            }
            // A world pin on the first body exercises joint rewiring.
            app.world_mut().write_message(SpawnJointIntent {
                record: JointRecord {
                    id: StableId::new(),
                    def: JointDef {
                        kind: JointKind::Hinge { limits: None, motor: None },
                        common: JointCommon::default(),
                        body_a: first_id.expect("at least one body"),
                        body_b: None,
                        anchor_a: Vec2::ZERO,
                        anchor_b: Vec2::ZERO,
                        rest_rot_a: 0.0,
                        rest_rot_b: 0.0,
                    },
                },
            });
            app.update();

            let baseline = SceneRecord::capture(app.world_mut());
            let depth_before = app.world().resource::<CommandStack>().undo_len();
            let mut area = total_area(&mut app);

            for (a, b, w) in cuts {
                cut(&mut app, a, b, w);
                let next = total_area(&mut app);
                prop_assert!(
                    next <= area + area.max(1.0) * 5e-3,
                    "cutting must never create material ({area} -> {next})"
                );
                area = next;

                // Every joint endpoint resolves to a live body.
                let defs: Vec<JointDef> = app
                    .world_mut()
                    .query::<&JointDef>()
                    .iter(app.world())
                    .cloned()
                    .collect();
                for def in defs {
                    for body in def.referenced_bodies() {
                        prop_assert!(
                            entity_of(&app, body).is_some(),
                            "dangling joint endpoint after cut"
                        );
                    }
                }
            }

            let applied = app.world().resource::<CommandStack>().undo_len() - depth_before;
            for _ in 0..applied {
                undo(&mut app);
            }
            let restored = SceneRecord::capture(app.world_mut());
            prop_assert_eq!(baseline, restored, "undoing all cuts restores the scene");
        }
    }
}

// ---------- Merge (weld-as-one-body) ----------

#[test]
fn merging_overlapping_boxes_makes_one_union_body() {
    let mut app = paused_app();
    let host = spawn_box_at(&mut app, Vec2::ZERO, 80.0, 40.0);
    let absorbed = spawn_box_at(&mut app, Vec2::new(60.0, 0.0), 80.0, 40.0);

    app.world_mut().write_message(MergeIntent {
        targets: vec![host, absorbed],
    });
    app.update();

    assert_eq!(body_count(&mut app), 1, "two bodies became one");
    assert!(entity_of(&app, absorbed).is_none());
    let entity = entity_of(&app, host).expect("host survives with its id");
    let shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    assert!(
        matches!(
            shape,
            ShapeDef::Csg {
                op: CsgOp::Union,
                ..
            }
        ),
        "merged shape is a union tree, got {shape:?}"
    );
    let area: f32 = polygonize_components(&shape)
        .iter()
        .map(gradiance::geometry::contours::Contours::area)
        .sum();
    let ideal = 80.0 * 40.0 * 2.0 - 20.0 * 40.0; // minus the overlap
    assert!(
        (area - ideal).abs() / ideal < 0.02,
        "union area {area} vs {ideal}"
    );

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "undo restores both bodies");
    assert!(entity_of(&app, absorbed).is_some());
    let shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    assert!(matches!(shape, ShapeDef::Box { .. }), "host shape restored");

    redo(&mut app);
    assert_eq!(body_count(&mut app), 1, "redo replays the merge");
}

#[test]
fn merging_disjoint_bodies_is_refused() {
    let mut app = paused_app();
    let a = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let b = spawn_box_at(&mut app, Vec2::new(300.0, 0.0), 40.0, 40.0);
    let depth = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().write_message(MergeIntent {
        targets: vec![a, b],
    });
    app.update();

    assert_eq!(body_count(&mut app), 2, "disjoint bodies stay separate");
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        depth,
        "refused merges leave history untouched (grouping is for rigid links)"
    );
}

#[test]
fn merge_rewires_external_joints_and_deletes_internal_ones() {
    let mut app = paused_app();
    let host = spawn_box_at(&mut app, Vec2::ZERO, 80.0, 40.0);
    let absorbed = spawn_box_at(&mut app, Vec2::new(60.0, 0.0), 80.0, 40.0);

    // External: a world pin on the absorbed body at world (70, 0).
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: absorbed,
                body_b: None,
                anchor_a: Vec2::new(10.0, 0.0),
                anchor_b: Vec2::new(70.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    // Internal: a joint between the two merged bodies.
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: host,
                body_b: Some(absorbed),
                anchor_a: Vec2::new(30.0, 0.0),
                anchor_b: Vec2::new(-30.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    app.world_mut().write_message(MergeIntent {
        targets: vec![host, absorbed],
    });
    app.update();

    let defs: Vec<JointDef> = app
        .world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .cloned()
        .collect();
    assert_eq!(defs.len(), 1, "internal weld deleted, external pin kept");
    assert_eq!(defs[0].body_a, host, "pin rewired onto the host");
    assert!(
        (defs[0].anchor_a - Vec2::new(70.0, 0.0)).length() < 1e-3,
        "anchor keeps its world point in host space (got {})",
        defs[0].anchor_a
    );

    undo(&mut app);
    let joints = app
        .world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .count();
    assert_eq!(joints, 2, "undo restores both joints");
}

//! The close-packing optimizer, driven through the real app.
//!
//! The solver's own arithmetic is unit-tested inside `gradiance-optimize`;
//! what can only be tested here is the *wiring*: that a request gathers the
//! selection's real geometry, that a run advances across frames without
//! touching the world, that acceptance produces exactly one undoable
//! command, and that cancelling leaves nothing behind.

use crate::harness::{box_record, entity_of, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::interaction::pack::{PackControl, PackSession, StartPackRequest};
use gradiance::interaction::selection::Selection;
use gradiance::optimize::{Boundary, LayerRule, PackConfig, RotationMode, SolverKind};
use gradiance::prelude::*;

/// Spawns a box and returns its id.
fn spawn_box(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// Selects every listed body.
fn select(app: &mut App, ids: &[StableId]) {
    let entities: Vec<Entity> = ids.iter().filter_map(|id| entity_of(app, *id)).collect();
    let mut selection = app.world_mut().resource_mut::<Selection>();
    selection.clear();
    for e in entities {
        selection.add(e);
    }
}

/// Overwrites the optimizer rulebook.
fn configure(app: &mut App, config: PackConfig) {
    app.world_mut().insert_resource(config);
}

/// A configuration that finishes quickly and deterministically.
///
/// These tests are about the *session* — its frames, its preview, its single
/// commit. Solver quality is measured in `gradiance-optimize`'s own tests.
fn quick(solver: SolverKind) -> PackConfig {
    PackConfig {
        solver,
        clearance: 0.0,
        rotation: RotationMode::Fixed,
        max_iterations: 800,
        patience: 200,
        iterations_per_frame: 400,
        ..Default::default()
    }
}

/// Runs frames until the session finishes or the budget runs out.
fn run_to_completion(app: &mut App, max_frames: usize) {
    for _ in 0..max_frames {
        let done = app
            .world()
            .resource::<PackSession>()
            .status()
            .is_some_and(gradiance::optimize::RunStatus::is_done);
        if done || !app.world().resource::<PackSession>().is_active() {
            return;
        }
        app.update();
    }
}

/// World-space bounding box of the given bodies.
fn bounds_of(app: &mut App, ids: &[StableId]) -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for id in ids {
        let Some(entity) = entity_of(app, *id) else {
            continue;
        };
        let Some(shape) = app.world().get::<ShapeDef>(entity).cloned() else {
            continue;
        };
        let Some(transform) = app.world().get::<Transform>(entity).copied() else {
            continue;
        };
        let pose = PosRot::from_transform(&transform);
        let (sin, cos) = pose.rot.sin_cos();
        for ring in gradiance::geometry::polygonize::polygonize(&shape).rings() {
            for v in ring {
                let w = Vec2::new(
                    pose.pos.x + v.x * cos - v.y * sin,
                    pose.pos.y + v.x * sin + v.y * cos,
                );
                min = min.min(w);
                max = max.max(w);
            }
        }
    }
    (min, max)
}

fn area(bounds: (Vec2, Vec2)) -> f32 {
    let size = bounds.1 - bounds.0;
    size.x * size.y
}

/// Five boxes spread far apart along x.
fn scattered(app: &mut App, n: usize, spacing: f32) -> Vec<StableId> {
    (0..n)
        .map(|i| spawn_box(app, Vec2::new(i as f32 * spacing, 0.0), 0.4, 0.4))
        .collect()
}

#[test]
fn packing_a_scattered_selection_shrinks_it_into_one_undo_step() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 5, 4.0);
    select(&mut app, &ids);
    configure(&mut app, quick(SolverKind::Shelf));

    let before_area = area(bounds_of(&mut app, &ids));
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 200);
    // One more frame for the auto-apply commit to dispatch.
    step(&mut app, 2);

    let after_area = area(bounds_of(&mut app, &ids));
    assert!(
        after_area < before_area * 0.25,
        "packing should collapse a 16 m spread: {before_area:.2} m² -> {after_area:.2} m²"
    );
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo + 1,
        "the whole rearrangement is exactly one command"
    );
    assert!(
        !app.world().resource::<PackSession>().is_active(),
        "the session ends when it commits"
    );
}

#[test]
fn undo_restores_every_body_the_pack_moved() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 4, 5.0);
    select(&mut app, &ids);
    configure(&mut app, quick(SolverKind::Shelf));

    let before: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();

    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 200);
    step(&mut app, 2);

    undo(&mut app);
    let after: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert!(a.distance(*b) < 1e-3, "undo must restore {a:?}, got {b:?}");
    }
}

#[test]
fn a_running_session_does_not_touch_the_world_until_it_is_accepted() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 5, 4.0);
    select(&mut app, &ids);
    // Hold the result rather than auto-applying, and pace it so the run is
    // still going after a frame.
    configure(
        &mut app,
        PackConfig {
            auto_apply: false,
            iterations_per_frame: 1,
            ..quick(SolverKind::Descent)
        },
    );

    let before: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().write_message(StartPackRequest);
    step(&mut app, 10);

    assert!(
        app.world().resource::<PackSession>().is_active(),
        "the run should still be going"
    );
    let during: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();
    for (a, b) in before.iter().zip(&during) {
        assert!(
            a.distance(*b) < 1e-6,
            "a live preview must not move authored bodies"
        );
    }
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo,
        "and must not push a command"
    );

    // Accepting is what moves them.
    app.world_mut().write_message(PackControl::Apply);
    step(&mut app, 2);
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo + 1
    );
}

#[test]
fn cancelling_leaves_the_scene_exactly_as_it_was() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 4, 4.0);
    select(&mut app, &ids);
    configure(
        &mut app,
        PackConfig {
            auto_apply: false,
            iterations_per_frame: 1,
            ..quick(SolverKind::Descent)
        },
    );

    let before: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().write_message(StartPackRequest);
    step(&mut app, 5);
    app.world_mut().write_message(PackControl::Cancel);
    step(&mut app, 2);

    assert!(!app.world().resource::<PackSession>().is_active());
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo,
        "a cancelled run is not an undo step"
    );
    let after: Vec<Vec2> = ids
        .iter()
        .filter_map(|id| entity_of(&app, *id))
        .filter_map(|e| app.world().get::<Transform>(e))
        .map(|t| t.translation.truncate())
        .collect();
    for (a, b) in before.iter().zip(&after) {
        assert!(a.distance(*b) < 1e-6);
    }
}

#[test]
fn a_one_body_selection_starts_nothing() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 1, 4.0);
    select(&mut app, &ids);
    configure(&mut app, quick(SolverKind::Shelf));

    app.world_mut().write_message(StartPackRequest);
    step(&mut app, 3);
    assert!(
        !app.world().resource::<PackSession>().is_active(),
        "nothing to arrange"
    );
}

#[test]
fn ground_planes_are_left_out_of_the_packing() {
    let mut app = paused_app();
    let mut ids = scattered(&mut app, 3, 4.0);

    // A ground half-plane: infinite, so it has no footprint to pack and must
    // never be dragged around by the optimizer.
    let mut ground = box_record(Vec2::new(0.0, -3.0), 1.0, 1.0);
    ground.shape = ShapeDef::HalfPlane;
    let ground_id = ground.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: ground });
    app.update();
    ids.push(ground_id);
    select(&mut app, &ids);
    configure(&mut app, quick(SolverKind::Shelf));

    let ground_entity = entity_of(&app, ground_id).expect("ground exists");
    let before = app
        .world()
        .get::<Transform>(ground_entity)
        .map(|t| t.translation)
        .expect("ground has a transform");

    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 200);
    step(&mut app, 2);

    let after = app
        .world()
        .get::<Transform>(ground_entity)
        .map(|t| t.translation)
        .expect("ground still exists");
    assert!(
        before.distance(after) < 1e-6,
        "the floor must not be rearranged"
    );
}

#[test]
fn depth_aware_packing_beats_flat_packing_on_the_same_bodies() {
    // Six boxes alternating between two disjoint depth bands. Depth-aware
    // packing may stack them; flat packing may not, so it must end up
    // measurably larger. This is the whole point of a 2.5D packer, and it
    // exercises the DepthBand -> collision-layer derivation end to end.
    let build = |app: &mut App| -> Vec<StableId> {
        let mut ids = Vec::new();
        for i in 0..6 {
            let mut record = box_record(Vec2::new(i as f32 * 3.0, 0.0), 0.5, 0.5);
            let slab = gradiance::core::constants::LAYER_HEIGHT;
            record.depth = if i % 2 == 0 {
                DepthBand {
                    near: 0.0,
                    far: slab,
                }
            } else {
                DepthBand {
                    near: slab,
                    far: slab * 2.0,
                }
            };
            ids.push(record.id);
            app.world_mut().write_message(SpawnBodyIntent { record });
        }
        app.update();
        ids
    };

    let measure = |layers: LayerRule| -> f32 {
        let mut app = paused_app();
        let ids = build(&mut app);
        select(&mut app, &ids);
        configure(
            &mut app,
            PackConfig {
                layers,
                ..quick(SolverKind::Shelf)
            },
        );
        app.world_mut().write_message(StartPackRequest);
        app.update();
        run_to_completion(&mut app, 200);
        step(&mut app, 2);
        area(bounds_of(&mut app, &ids))
    };

    let aware = measure(LayerRule::Respect);
    let flat = measure(LayerRule::Solid);
    assert!(
        aware < flat * 0.8,
        "depth-aware packing should be much tighter: {aware:.3} m² vs flat {flat:.3} m²"
    );
}

/// The shipped defaults, end to end, on a real scene.
///
/// Everything else here pins a mechanism with a deliberately simplified
/// config; this one checks that what a user actually gets when they press
/// "Pack selection" is a dense, legal arrangement.
#[test]
fn the_default_configuration_packs_a_selection_densely() {
    let mut app = paused_app();
    let ids: Vec<StableId> = (0..9)
        .map(|i| {
            spawn_box(
                &mut app,
                Vec2::new((i % 3) as f32 * 5.0, (i / 3) as f32 * 5.0),
                0.6,
                0.6,
            )
        })
        .collect();
    select(&mut app, &ids);
    configure(
        &mut app,
        PackConfig {
            iterations_per_frame: 400,
            ..Default::default()
        },
    );

    let before = area(bounds_of(&mut app, &ids));
    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 400);
    step(&mut app, 2);

    let after = area(bounds_of(&mut app, &ids));
    let body_area = 9.0 * 0.6 * 0.6;
    let fill = body_area / after;
    assert!(
        fill > 0.55,
        "the default pack should be dense: {fill:.2} fill \
         ({before:.2} m² -> {after:.2} m²)"
    );
}

#[test]
fn a_hard_rectangle_boundary_contains_the_result() {
    let mut app = paused_app();
    let ids = scattered(&mut app, 6, 3.0);
    select(&mut app, &ids);
    configure(
        &mut app,
        PackConfig {
            boundary: Boundary::Rect {
                width: 3.0,
                height: 3.0,
            },
            ..quick(SolverKind::Descent)
        },
    );

    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 400);
    step(&mut app, 2);

    let (min, max) = bounds_of(&mut app, &ids);
    let size = max - min;
    // The container is centred on the selection's centroid; allow the bodies'
    // own half-extents, since the constraint is on centres plus radius.
    assert!(
        size.x <= 3.5 && size.y <= 3.5,
        "result escaped the 3x3 box: {size:?}"
    );
}

/// The acid test's fixture: `cols × rows` identical boxes, thrown around.
///
/// Deterministic scatter (a fixed LCG) so a failure is reproducible, and
/// deliberately messy: overlapping piles, big gaps, nothing aligned. This is
/// what a grid looks like after you drop it.
fn scattered_grid(app: &mut App, cols: usize, rows: usize, size: f32) -> Vec<StableId> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut rand = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ((state >> 33) as f32) / (u32::MAX >> 1) as f32
    };
    (0..cols * rows)
        .map(|_| {
            let p = Vec2::new(rand() * 12.0 - 6.0, rand() * 12.0 - 6.0);
            spawn_box(app, p, size, size)
        })
        .collect()
}

/// Every body's world AABB centre and half-size.
fn cells(app: &mut App, ids: &[StableId]) -> Vec<(Vec2, Vec2)> {
    ids.iter()
        .filter_map(|id| {
            let entity = entity_of(app, *id)?;
            let (lo, hi) = bounds_of(app, std::slice::from_ref(id));
            let _ = entity;
            Some(((lo + hi) / 2.0, (hi - lo) / 2.0))
        })
        .collect()
}

/// Groups coordinates into clusters no wider than `tol`, returning the count.
fn lanes(mut values: Vec<f32>, tol: f32) -> usize {
    values.sort_by(f32::total_cmp);
    let mut n = 0;
    let mut last = f32::NEG_INFINITY;
    for v in values {
        if v - last > tol {
            n += 1;
        }
        last = v;
    }
    n
}

#[test]
fn a_scattered_grid_of_equal_boxes_packs_back_into_a_grid() {
    // The headline scenario, in one test: take a grid of identical
    // rectangles, drop them so they scatter, and ask the packer to put them
    // back. Anything less than clean rows and columns is a failure — density
    // alone is not the ask, *tidiness* is.
    let mut app = paused_app();
    let ids = scattered_grid(&mut app, 4, 3, 1.0);
    select(&mut app, &ids);
    // The app's own defaults, not a test-tuned config: this has to work for
    // someone who never opens the optimizer panel.
    configure(&mut app, PackConfig::default());

    app.world_mut().write_message(StartPackRequest);
    app.update();
    run_to_completion(&mut app, 600);
    step(&mut app, 2);

    let cells = cells(&mut app, &ids);
    assert_eq!(cells.len(), 12);

    // No overlaps: axis-aligned boxes, so an AABB test is exact.
    for (i, a) in cells.iter().enumerate() {
        for b in cells.iter().skip(i + 1) {
            let d = (a.0 - b.0).abs();
            let touch = a.1 + b.1;
            assert!(
                d.x >= touch.x - 1e-3 || d.y >= touch.y - 1e-3,
                "bodies overlap after packing: {a:?} vs {b:?}"
            );
        }
    }

    // A grid: 12 equal unit boxes should land on a small number of shared
    // columns and rows, not 12 arbitrary positions.
    let xs = lanes(cells.iter().map(|c| c.0.x).collect(), 0.2);
    let ys = lanes(cells.iter().map(|c| c.0.y).collect(), 0.2);
    assert!(
        xs * ys <= 16 && xs <= 6 && ys <= 6,
        "expected a tidy grid, got {xs} columns x {ys} rows"
    );

    // ...and a tight one: 12 unit boxes have area 12, so a 4x3 or 3x4
    // packing bounds 12 m². Allow a row of slack, not a scatter.
    let bounds = bounds_of(&mut app, &ids);
    let filled = 12.0 / area(bounds);
    assert!(
        filled > 0.75,
        "packing is loose: fill {filled:.3} over {:?}",
        bounds.1 - bounds.0
    );
}

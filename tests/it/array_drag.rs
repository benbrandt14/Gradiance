//! The array drag, driven through the real app.
//!
//! The pitch arithmetic is unit-tested in `gradiance-geometry`, and the plan
//! arithmetic in `gradiance-interaction`. What can only be tested here is the
//! gesture: that alt-pressing a handle starts an array rather than a scale,
//! that the drag writes nothing until release, that release makes exactly one
//! undoable command, and — the headline behaviour — that the copies land
//! flush.

use crate::harness::{body_count, box_record, entity_of, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::command::array_cmd::{ArrayTweens, TweenStep};
use gradiance::interaction::camera::CameraScale;
use gradiance::interaction::cursor::CursorWorldPos;
use gradiance::interaction::selection::Selection;
use gradiance::interaction::tools::array_tool::{ArrayConfig, ArrayPattern, ArraySpacing};
use gradiance::interaction::tools::handles::{HandleKind, ScaleFrame, SelectionBox};
use gradiance::prelude::*;

/// Spawns a box and returns its id.
fn spawn_box(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// Selects the listed bodies.
fn select(app: &mut App, ids: &[StableId]) {
    let entities: Vec<Entity> = ids.iter().filter_map(|id| entity_of(app, *id)).collect();
    let mut selection = app.world_mut().resource_mut::<Selection>();
    selection.clear();
    for e in entities {
        selection.add(e);
    }
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

fn alt(app: &mut App, down: bool) {
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    if down {
        keys.press(KeyCode::AltLeft);
    } else {
        keys.release(KeyCode::AltLeft);
    }
}

/// The selection's box, as the tool computes it.
fn sbox(app: &mut App) -> SelectionBox {
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let selection = app.world().resource::<Selection>();
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for entity in selection.iter() {
        let Ok((shape, transform)) = q.get(app.world(), entity) else {
            continue;
        };
        let affine = transform.compute_affine();
        for ring in gradiance::geometry::polygonize::polygonize(shape).rings() {
            for v in ring {
                let w = affine.transform_point3(v.extend(0.0)).truncate();
                min = min.min(w);
                max = max.max(w);
            }
        }
    }
    SelectionBox {
        center: (min + max) / 2.0,
        rot: 0.0,
        half: (max - min) / 2.0,
    }
}

/// A camera scale where the handles are actually distinguishable.
///
/// The handle capture radius is a fixed number of *pixels*, so it scales with
/// the camera. The headless default of 1 m/px turns a 10 px radius into 10 m,
/// which swallows every handle of a metre-sized selection and grabs whichever
/// is first in the list. A metre-sized body filling ~100 px is the realistic
/// case, and the one these tests mean to exercise.
const TEST_CAM_SCALE: f32 = 0.01;

/// Performs a complete alt-drag from `handle` to `to`.
fn array_drag(app: &mut App, handle: HandleKind, to: Vec2) {
    app.world_mut().insert_resource(ScaleFrame::Global);
    app.world_mut().insert_resource(CameraScale(TEST_CAM_SCALE));
    let start = sbox(app).point(handle.unit());
    alt(app, true);
    set_cursor(app, start);
    mouse(app, true);
    app.update();
    set_cursor(app, to);
    app.update();
    mouse(app, false);
    app.update();
    alt(app, false);
    // A frame for dispatch to drain the intent.
    app.update();
}

/// Every body's world AABB, sorted left to right.
fn boxes(app: &mut App) -> Vec<(Vec2, Vec2)> {
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let mut out: Vec<(Vec2, Vec2)> = Vec::new();
    for (shape, transform) in q.iter(app.world()) {
        let affine = transform.compute_affine();
        let mut min = Vec2::splat(f32::MAX);
        let mut max = Vec2::splat(f32::MIN);
        for ring in gradiance::geometry::polygonize::polygonize(shape).rings() {
            for v in ring {
                let w = affine.transform_point3(v.extend(0.0)).truncate();
                min = min.min(w);
                max = max.max(w);
            }
        }
        out.push((min, max));
    }
    out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    out
}

#[test]
fn alt_dragging_a_side_handle_builds_a_flush_wall() {
    // The headline case from the feature request: one block, drag the side,
    // get a wall with no spacing between the blocks.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(3.6, 0.0));

    assert_eq!(body_count(&mut app), 4, "the original plus three copies");
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo + 1,
        "the whole wall is one undo step"
    );

    // Flush: each block's right edge is the next block's left edge.
    let rects = boxes(&mut app);
    for pair in rects.windows(2) {
        let gap = pair[1].0.x - pair[0].1.x;
        assert!(
            gap.abs() < 1e-3,
            "blocks should touch exactly, found a {gap:.4} m gap"
        );
    }
}

#[test]
fn alt_dragging_up_a_two_block_stack_builds_a_seamless_tower() {
    // The second case from the request: two blocks stacked, drag up, get a
    // tower with no gaps — which requires stepping by the *stack* height,
    // not one block's height.
    let mut app = paused_app();
    let lower = spawn_box(&mut app, Vec2::new(0.0, 0.5), 1.0, 1.0);
    let upper = spawn_box(&mut app, Vec2::new(0.0, 1.5), 1.0, 1.0);
    select(&mut app, &[lower, upper]);

    array_drag(&mut app, HandleKind::EdgeY(1), Vec2::new(0.0, 6.2));

    assert_eq!(
        body_count(&mut app),
        6,
        "two originals plus two copies of two"
    );
    let rects = boxes(&mut app);
    let mut ys: Vec<(f32, f32)> = rects.iter().map(|(min, max)| (min.y, max.y)).collect();
    ys.sort_by(|a, b| a.0.total_cmp(&b.0));
    for pair in ys.windows(2) {
        let gap = pair[1].0 - pair[0].1;
        assert!(
            gap.abs() < 1e-3,
            "the tower should be seamless, found a {gap:.4} m gap"
        );
    }
}

#[test]
fn a_corner_handle_builds_a_grid() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);

    array_drag(&mut app, HandleKind::Corner(1, 1), Vec2::new(2.5, 3.5));

    // 3 columns × 4 rows.
    assert_eq!(body_count(&mut app), 12);
    let rects = boxes(&mut app);
    let mut xs: Vec<f32> = rects.iter().map(|(min, _)| min.x).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    assert_eq!(xs.len(), 3, "three distinct columns, got {xs:?}");
    let mut ys: Vec<f32> = rects.iter().map(|(min, _)| min.y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    assert_eq!(ys.len(), 4, "four distinct rows, got {ys:?}");
}

#[test]
fn the_drag_writes_nothing_until_release() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    app.world_mut().insert_resource(ScaleFrame::Global);
    app.world_mut().insert_resource(CameraScale(TEST_CAM_SCALE));
    let start = sbox(&mut app).point(HandleKind::EdgeX(1).unit());
    alt(&mut app, true);
    set_cursor(&mut app, start);
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    step(&mut app, 3);

    assert_eq!(body_count(&mut app), 1, "the preview creates no bodies");
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo,
        "and pushes no command"
    );

    mouse(&mut app, false);
    step(&mut app, 2);
    assert!(body_count(&mut app) > 1, "release is what commits");
}

#[test]
fn without_alt_the_same_drag_scales_instead() {
    // The modifier is the whole difference; without it the handles must keep
    // their existing behaviour.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);

    app.world_mut().insert_resource(ScaleFrame::Global);
    app.world_mut().insert_resource(CameraScale(TEST_CAM_SCALE));
    let start = sbox(&mut app).point(HandleKind::EdgeX(1).unit());
    set_cursor(&mut app, start);
    mouse(&mut app, true);
    app.update();
    set_cursor(&mut app, Vec2::new(3.0, 0.0));
    app.update();
    mouse(&mut app, false);
    step(&mut app, 2);

    assert_eq!(body_count(&mut app), 1, "scaling makes no copies");
    let rects = boxes(&mut app);
    let width = rects[0].1.x - rects[0].0.x;
    assert!(
        width > 1.5,
        "the body should have been stretched, got {width}"
    );
}

#[test]
fn dragging_inward_makes_nothing() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    let before_undo = app.world().resource::<CommandStack>().undo_len();

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(-4.0, 0.0));

    assert_eq!(body_count(&mut app), 1);
    assert_eq!(
        app.world().resource::<CommandStack>().undo_len(),
        before_undo,
        "an inward drag is a cancel, not an empty command"
    );
}

#[test]
fn undo_removes_the_whole_array() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(5.5, 0.0));
    assert!(body_count(&mut app) >= 5);

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1, "one undo removes every copy");
}

#[test]
fn a_gap_spacing_rule_leaves_the_gap_it_asks_for() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    app.world_mut().insert_resource(ArrayConfig {
        spacing: ArraySpacing::Gap(0.25),
        ..Default::default()
    });

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(4.0, 0.0));

    let rects = boxes(&mut app);
    assert!(rects.len() >= 3);
    for pair in rects.windows(2) {
        let gap = pair[1].0.x - pair[0].1.x;
        assert!(
            (gap - 0.25).abs() < 1e-3,
            "expected a 0.25 m gap, found {gap:.4}"
        );
    }
}

#[test]
fn a_fixed_count_ignores_how_far_the_drag_went() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    app.world_mut().insert_resource(ArrayConfig {
        count_override: Some(5),
        ..Default::default()
    });

    // A drag that would otherwise fit only one copy.
    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(1.2, 0.0));
    assert_eq!(
        body_count(&mut app),
        6,
        "one original plus the five asked for"
    );
}

#[test]
fn a_radial_pattern_sweeps_copies_around_the_selection() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::new(3.0, 0.0), 1.0, 1.0);
    select(&mut app, &[id]);
    app.world_mut().insert_resource(ArrayConfig {
        pattern: ArrayPattern::Radial,
        angle_step: std::f32::consts::FRAC_PI_2,
        count_override: Some(3),
        ..Default::default()
    });

    array_drag(&mut app, HandleKind::Corner(1, 1), Vec2::new(6.0, 2.0));

    assert_eq!(body_count(&mut app), 4);
    // A quarter-turn sweep about the body's own centre leaves every copy the
    // same distance from that centre.
    let rects = boxes(&mut app);
    let centers: Vec<Vec2> = rects.iter().map(|(min, max)| (*min + *max) / 2.0).collect();
    let pivot = centers.iter().copied().sum::<Vec2>() / centers.len() as f32;
    let radii: Vec<f32> = centers.iter().map(|c| c.distance(pivot)).collect();
    for r in &radii {
        assert!(
            (r - radii[0]).abs() < 1e-2,
            "copies should share a radius: {radii:?}"
        );
    }
}

#[test]
fn a_depth_step_walks_the_copies_into_the_scene() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    app.world_mut().insert_resource(ArrayConfig {
        tweens: gradiance::command::array_cmd::ArrayTweens {
            along_x: gradiance::command::array_cmd::TweenStep {
                depth: 0.2,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(3.5, 0.0));

    let mut q = app.world_mut().query_filtered::<&DepthBand, With<Body>>();
    let mut nears: Vec<f32> = q.iter(app.world()).map(|b| b.near).collect();
    nears.sort_by(f32::total_cmp);
    assert!(nears.len() >= 4);
    assert!(
        nears.last().is_some_and(|n| *n > 0.5),
        "the last copy should sit well behind the first: {nears:?}"
    );
}

/// Sets the per-copy taper on one lane and leaves everything else alone.
fn taper(app: &mut App, along_x: Vec2, along_y: Vec2) {
    app.world_mut().insert_resource(ArrayConfig {
        tweens: ArrayTweens {
            along_x: TweenStep {
                scale: along_x,
                ..Default::default()
            },
            along_y: TweenStep {
                scale: along_y,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });
}

#[test]
fn a_tapering_wall_shrinks_its_copies_and_still_lands_flush() {
    // The headline of the per-copy parameter change: every copy 80% of the
    // last, *and* contact spacing keeps up, so the wall closes rather than
    // developing a widening seam.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    taper(&mut app, Vec2::splat(0.8), Vec2::ONE);

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(3.0, 0.0));

    let boxes = boxes(&mut app);
    assert!(
        boxes.len() >= 3,
        "expected a wall, got {} boxes",
        boxes.len()
    );
    let widths: Vec<f32> = boxes.iter().map(|(lo, hi)| hi.x - lo.x).collect();
    for pair in widths.windows(2) {
        assert!(
            (pair[1] / pair[0] - 0.8).abs() < 0.02,
            "each copy should be 80% of the last: {widths:?}"
        );
    }
    // Flush: the gap between neighbours never opens up, and they never
    // interpenetrate. Both matter — a taper that ignored spacing would drift
    // apart, and one that over-corrected would overlap.
    for pair in boxes.windows(2) {
        let gap = pair[1].0.x - pair[0].1.x;
        assert!(
            gap.abs() < 2e-2,
            "copies drifted out of contact by {gap} m: {boxes:?}"
        );
    }
}

#[test]
fn a_grid_can_narrow_across_and_flatten_down_independently() {
    // The other user-facing promise: "scale x and y separately if patterning
    // a grid". The column lane narrows, the row lane flattens, and neither
    // leaks into the other.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    taper(&mut app, Vec2::new(0.8, 1.0), Vec2::new(1.0, 0.8));

    array_drag(&mut app, HandleKind::Corner(1, 1), Vec2::new(2.5, 2.5));

    let boxes = boxes(&mut app);
    assert!(
        boxes.len() >= 4,
        "expected a grid, got {} boxes",
        boxes.len()
    );
    let size = |p: Vec2| -> Option<(f32, f32)> {
        boxes
            .iter()
            .find(|(lo, hi)| ((*lo + *hi) / 2.0).distance(p) < 0.35)
            .map(|(lo, hi)| (hi.x - lo.x, hi.y - lo.y))
    };
    let origin = size(Vec2::ZERO).expect("the original is still there");
    assert!((origin.0 - 1.0).abs() < 1e-3 && (origin.1 - 1.0).abs() < 1e-3);

    // One column across: 20% narrower, exactly as tall.
    let across = boxes
        .iter()
        .map(|(lo, hi)| (*lo + *hi) / 2.0)
        .filter(|c| c.y.abs() < 0.3 && c.x > 0.3)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .and_then(size)
        .expect("a cell one column across");
    assert!((across.0 - 0.8).abs() < 0.02, "narrowed: {across:?}");
    assert!(
        (across.1 - 1.0).abs() < 0.02,
        "but not flattened: {across:?}"
    );

    // One row down: exactly as wide, 20% shorter.
    let down = boxes
        .iter()
        .map(|(lo, hi)| (*lo + *hi) / 2.0)
        .filter(|c| c.x.abs() < 0.3 && c.y > 0.3)
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .and_then(size)
        .expect("a cell one row down");
    assert!((down.0 - 1.0).abs() < 0.02, "not narrowed: {down:?}");
    assert!((down.1 - 0.8).abs() < 0.02, "flattened: {down:?}");
}

#[test]
fn a_fixed_step_ignores_the_taper_it_was_told_to_ignore() {
    // Spacing rules that name an explicit step must keep it, even while the
    // copies change size — otherwise "fixed" would not mean fixed.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    app.world_mut().insert_resource(ArrayConfig {
        spacing: ArraySpacing::Fixed(1.5),
        tweens: ArrayTweens {
            along_x: TweenStep {
                scale: Vec2::splat(0.5),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(4.6, 0.0));

    let centres: Vec<Vec2> = boxes(&mut app)
        .iter()
        .map(|(lo, hi)| (*lo + *hi) / 2.0)
        .collect();
    assert!(centres.len() >= 3, "got {centres:?}");
    for pair in centres.windows(2) {
        let step = pair[1].x - pair[0].x;
        assert!(
            (step - 1.5).abs() < 1e-2,
            "a fixed step must stay fixed: {step} in {centres:?}"
        );
    }
}

#[test]
fn a_steep_taper_packs_a_short_run_that_is_flush_the_whole_way() {
    // Halving each copy: the pitches are 0.75, 0.375, 0.1875 … so the run is
    // short and the gaps have to close by the same factor. A single measured
    // pitch reused for every copy would leave the second gap twice too wide.
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    select(&mut app, &[id]);
    taper(&mut app, Vec2::splat(0.5), Vec2::ONE);

    // The handle starts at x = 0.5, so this is 1.2 m of pull: 0.75 + 0.375
    // fits, 0.1875 more does not.
    array_drag(&mut app, HandleKind::EdgeX(1), Vec2::new(1.7, 0.0));

    assert_eq!(body_count(&mut app), 3, "two copies fit in that pull");
    let boxes = boxes(&mut app);
    let widths: Vec<f32> = boxes.iter().map(|(lo, hi)| hi.x - lo.x).collect();
    assert!(
        (widths[1] - 0.5).abs() < 0.02 && (widths[2] - 0.25).abs() < 0.02,
        "each copy halves: {widths:?}"
    );
    for pair in boxes.windows(2) {
        let gap = pair[1].0.x - pair[0].1.x;
        assert!(
            gap.abs() < 1e-2,
            "flush all the way down, but gap was {gap}: {boxes:?}"
        );
    }
}

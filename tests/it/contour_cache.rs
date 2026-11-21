//! The derived contour cache (brief Q-8): every body's `ShapeContours` is
//! `polygonize_components` of its shape, rebuilt on a shape edit (scale,
//! cut, merge) and never saved. Per-frame readers (borders, outlines,
//! selection box, snapping) read it instead of contouring.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, paused_app, step};
use bevy::prelude::*;
use gradiance::geometry::polygonize::{ShapeContours, polygonize_components};
use gradiance::op::invoke;
use gradiance::prelude::*;
use serde_json::json;

/// Every body's cache equals a fresh contour of its shape.
fn assert_cache_current(app: &mut App) {
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &ShapeContours), With<Body>>();
    let mut n = 0;
    for (shape, cached) in q.iter(app.world()) {
        assert_eq!(cached.0, polygonize_components(shape), "{shape:?}");
        n += 1;
    }
    assert!(n > 0, "bodies have a cache");
}

#[test]
fn the_cache_follows_every_shape_edit_and_is_never_saved() {
    let mut app = paused_app();
    for x in [0.0, 0.6] {
        app.world_mut().write_message(SpawnBodyIntent {
            record: box_record(Vec2::new(x, 0.0), 1.0, 1.0),
        });
    }
    step(&mut app, 2);
    assert_cache_current(&mut app);
    for (id, params) in [
        ("body.scale", json!({"i": 0, "fx": 2, "fy": 1.5})),
        (
            "body.cut",
            json!({"ax": -2, "ay": 0.1, "bx": 2, "by": 0.1, "width": 0.05}),
        ),
        ("body.merge", json!({"a": 0, "b": 1})),
    ] {
        invoke(app.world_mut(), id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
        step(&mut app, 3);
        assert_cache_current(&mut app);
    }
    let saved =
        gradiance::scene::to_ron(&gradiance::scene::SceneRecord::capture(app.world_mut())).unwrap();
    assert!(!saved.contains("ShapeContours"));
}

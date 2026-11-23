//! Brush erase (brief S-16): the sketch tool's erase mode subtracts the
//! stroke, at the sketch width, from every body it crosses. One undo step.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, undo};
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    app.update();
}

/// Every body's area (polygon pieces by the shoelace, a box by its sides).
fn areas(app: &mut App) -> Vec<f32> {
    let mut query = app.world_mut().query_filtered::<&ShapeDef, With<Body>>();
    let mut areas: Vec<f32> = query
        .iter(app.world())
        .map(|shape| match shape {
            ShapeDef::Box { width, height } => width * height,
            ShapeDef::Polygon { outline, holes } => {
                let ring = |r: &[Vec2]| {
                    r.iter()
                        .zip(r.iter().cycle().skip(1))
                        .map(|(a, b)| a.perp_dot(*b))
                        .sum::<f32>()
                        .abs()
                        / 2.0
                };
                ring(outline) - holes.iter().map(|h| ring(h)).sum::<f32>()
            }
            other => panic!("unexpected shape {other:?}"),
        })
        .collect();
    areas.sort_by(f32::total_cmp);
    areas
}

#[test]
fn erasing_across_a_box_makes_two_pieces_without_the_stroke() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 2 1)");
    assert_eq!(areas(&mut app), vec![2.0]);
    run(&mut app, "(erase (list 0 0 0 2) 0.2)");
    let pieces = areas(&mut app);
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let total: f32 = pieces.iter().sum();
    // The box less a 0.2 m strip across its 1 m height.
    let expected = 2.0 - 0.2;
    assert!(
        (total - expected).abs() / expected < 0.02,
        "{total} vs {expected}"
    );
    assert!((pieces[0] - pieces[1]).abs() < 0.02, "halves: {pieces:?}");
    undo(&mut app);
    assert_eq!(areas(&mut app), vec![2.0], "one undo step brings it back");
}

#[test]
fn an_erase_that_ends_inside_leaves_a_notch() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 2 1)");
    // Up from below to the middle: the box spans y 0.5..1.5, so a slot
    // 0.5 m deep with a round end.
    run(&mut app, "(erase (list 0 0 0 1) 0.2)");
    let pieces = areas(&mut app);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    let slot = 0.2 * 0.5 + std::f32::consts::PI * 0.01 / 2.0;
    let expected = 2.0 - slot;
    assert!(
        (pieces[0] - expected).abs() / expected < 0.02,
        "{} vs {expected}",
        pieces[0]
    );
}

#[test]
fn an_erase_that_misses_records_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 2 1)");
    let depth = app
        .world()
        .resource::<gradiance::command::HistoryInfo>()
        .undo_depth;
    run(&mut app, "(erase (list 5 0 5 2) 0.2)");
    assert_eq!(areas(&mut app), vec![2.0]);
    assert_eq!(
        app.world()
            .resource::<gradiance::command::HistoryInfo>()
            .undo_depth,
        depth
    );
}

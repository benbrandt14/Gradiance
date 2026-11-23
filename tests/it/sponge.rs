//! Make sponge (brief S-12): a body becomes circles joined by springs, one
//! group, one undo step; a dropped sponge squashes and recovers.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::domain::group::SelectionGroup;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

/// The centres of the dynamic circles.
fn circles(app: &mut App) -> Vec<Vec2> {
    let mut query = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    query
        .iter(app.world())
        .filter(|(s, _)| matches!(s, ShapeDef::Circle { .. }))
        .map(|(_, t)| t.translation.truncate())
        .collect()
}

/// The area of the convex hull of `points` (monotone chain).
fn hull_area(points: &[Vec2]) -> f32 {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<Vec2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Vec2>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for &q in iter {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0
            {
                hull.pop();
            }
            hull.push(q);
        }
        hull.pop();
    }
    let n = hull.len();
    (0..n)
        .map(|i| hull[i].perp_dot(hull[(i + 1) % n]))
        .sum::<f32>()
        .abs()
        / 2.0
}

/// A 1 m sponge dropped 1 m recovers to within 10 % of its area; undo
/// brings the box back.
#[test]
fn a_dropped_sponge_recovers_its_area_and_undo_restores_the_box() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 -0.1 6 0.2) (spawn-box 0 1.5 1 1))",
    );
    run(&mut app, "(begin (set-static 0 1) (sponge 1 0 12))");
    let start = circles(&mut app);
    assert_eq!(start.len(), 36, "six across a 1 m box");
    {
        let mut groups = app
            .world_mut()
            .query_filtered::<&SelectionGroup, With<Body>>();
        let stacks: Vec<&SelectionGroup> = groups
            .iter(app.world())
            .filter(|g| !g.0.is_empty())
            .collect();
        assert_eq!(stacks.len(), 36, "every circle is grouped");
        assert!(stacks.windows(2).all(|w| w[0].0 == w[1].0), "in one group");
    }
    let rest = hull_area(&start);
    set_state(&mut app, GameState::Playing);
    let mut least = rest;
    for _ in 0..12 {
        step(&mut app, 15);
        least = least.min(hull_area(&circles(&mut app)));
    }
    let end = hull_area(&circles(&mut app));
    println!("sponge hull: rest {rest:.3}, least {least:.3}, end {end:.3} m2");
    assert!(
        least < rest * 0.98,
        "the impact squashes it: {least} vs {rest}"
    );
    assert!(
        (end / rest - 1.0).abs() <= 0.10,
        "recovers to within 10 %: {end} vs {rest}"
    );
    set_state(&mut app, GameState::Paused);
    undo(&mut app); // the run
    app.update();
    undo(&mut app); // the sponge
    app.update();
    assert!(circles(&mut app).is_empty(), "the circles go");
    let mut boxes = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let back: Vec<_> = boxes
        .iter(app.world())
        .filter(|(_, t)| t.translation.y > 1.0)
        .collect();
    assert_eq!(back.len(), 1, "the box is back");
    assert!((back[0].1.translation.truncate() - Vec2::new(0.0, 1.5)).length() < 1e-5);
}

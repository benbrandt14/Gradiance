//! Groups as editing layers (brief 4b-1): names that survive a file, group
//! edits and spreads that are one undo step, decompose, and the
//! filter → group → members round trip. Driven through the operation
//! registry (`gradiance::op::invoke`), the path every doorway shares.

use crate::harness::{paused_app, step, undo};
use avian2d::prelude::{ColliderDensity, Friction};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::command::intent::LoadSceneIntent;
use gradiance::core::ids::StableId;
use gradiance::domain::Body;
use gradiance::domain::appearance::Appearance;
use gradiance::domain::group::SelectionGroup;
use gradiance::op::{OpIdSource, invoke};
use gradiance::scene::{SceneRecord, from_ron, to_ron};
use gradiance::script::bridge::ScriptInputs;
use serde_json::{Value, json};

/// Runs one script with fixed ids (index `i` is the `i`-th spawn) and lets
/// it commit.
fn run(app: &mut App, source: &str) {
    app.insert_resource(OpIdSource::Sequence(100));
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    step(app, 2);
}

/// Invokes one operation and lets its intent commit; returns its value.
fn op(app: &mut App, id: &str, params: Value) -> Option<Value> {
    let value = invoke(app.world_mut(), id, params).expect("the operation lowers");
    step(app, 1);
    value
}

/// Three circles at x = -1, 0, 1 and a box at x = 3, the circles grouped.
fn scene() -> App {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-circle -1 1 0.3) (spawn-circle 0 1 0.3) (spawn-circle 1 1 0.3) \
         (spawn-box 3 1 0.5 0.5))",
    );
    assert_eq!(
        op(&mut app, "group.create", json!({"targets": [0, 1, 2]})),
        Some(json!(0))
    );
    app
}

/// Every body's component `C`, in id order.
fn values<C: Component + Clone>(app: &mut App) -> Vec<C> {
    let mut rows: Vec<(StableId, C)> = app
        .world_mut()
        .query_filtered::<(&StableId, &C), With<Body>>()
        .iter(app.world())
        .map(|(id, c)| (*id, c.clone()))
        .collect();
    rows.sort_by_key(|(id, _)| id.0);
    rows.into_iter().map(|(_, c)| c).collect()
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

#[test]
fn a_named_group_keeps_its_name_through_a_file_and_undo() {
    let mut app = scene();
    op(
        &mut app,
        "group.rename",
        json!({"group": 0, "name": "wheels"}),
    );
    assert_eq!(
        op(&mut app, "group.name", json!({"group": 0})),
        Some(json!("wheels"))
    );
    assert_eq!(
        op(&mut app, "group.list", json!({})),
        Some(json!(["wheels"]))
    );
    // A body names its outermost group too.
    assert_eq!(
        op(&mut app, "group.name", json!({"group": {"at": [0, 1]}})),
        Some(json!("wheels"))
    );

    let text = to_ron(&SceneRecord::capture(app.world_mut())).expect("saves");
    let mut loaded = paused_app();
    let record = from_ron(&text).expect("loads");
    loaded
        .world_mut()
        .write_message(LoadSceneIntent { scene: record });
    step(&mut loaded, 2);
    assert_eq!(
        op(&mut loaded, "group.name", json!({"group": 0})),
        Some(json!("wheels"))
    );

    undo(&mut app);
    step(&mut app, 1);
    assert_eq!(
        op(&mut app, "group.name", json!({"group": 0})),
        Some(json!(""))
    );
}

#[test]
fn a_group_set_is_one_step_that_undo_restores_exactly() {
    let mut app = scene();
    let before: Vec<Friction> = values(&mut app);
    let depth = undo_depth(&app);
    op(
        &mut app,
        "group.set",
        json!({"group": 0, "prop": "friction", "v": 0.2}),
    );
    assert_eq!(undo_depth(&app), depth + 1, "one step");
    let after: Vec<Friction> = values(&mut app);
    for f in &after[..3] {
        assert!((f.dynamic_coefficient - 0.2).abs() < 1e-6);
    }
    assert_eq!(after[3], before[3], "the box is not a member");
    undo(&mut app);
    step(&mut app, 1);
    assert_eq!(values::<Friction>(&mut app), before);
}

#[test]
fn a_distribute_is_one_step_hitting_both_bounds_in_key_order() {
    let mut app = scene();
    let before: Vec<ColliderDensity> = values(&mut app);
    let depth = undo_depth(&app);
    // Descending x order: x = 1 gets 0.5, x = -1 gets 5.
    op(
        &mut app,
        "group.distribute",
        json!({"group": 0, "prop": "density", "lo": 5, "hi": 0.5, "by": "x"}),
    );
    assert_eq!(undo_depth(&app), depth + 1, "one step");
    let densities: Vec<f32> = values::<ColliderDensity>(&mut app)
        .iter()
        .map(|d| d.0)
        .collect();
    assert_eq!(&densities[..3], &[5.0, 2.75, 0.5]);
    undo(&mut app);
    step(&mut app, 1);
    assert_eq!(values::<ColliderDensity>(&mut app), before);

    // A seeded scatter is reproducible and in bounds.
    op(
        &mut app,
        "group.scatter",
        json!({"group": 0, "prop": "hue", "lo": 100, "hi": 200, "seed": 3}),
    );
    let first: Vec<Appearance> = values(&mut app);
    for a in &first[..3] {
        let hue = a.fill.to_hsl().0;
        assert!((99.9..=200.1).contains(&hue), "hue {hue}");
    }
    undo(&mut app);
    step(&mut app, 1);
    op(
        &mut app,
        "group.scatter",
        json!({"group": 0, "prop": "hue", "lo": 100, "hi": 200, "seed": 3}),
    );
    assert_eq!(values::<Appearance>(&mut app), first);
}

#[test]
fn decompose_by_colour_makes_one_group_per_colour() {
    let mut app = scene();
    op(&mut app, "group.create", json!({"targets": [0, 1, 2, 3]}));
    op(
        &mut app,
        "group.set_color",
        json!({"group": 1, "r": 1, "g": 0, "b": 0}),
    );
    op(
        &mut app,
        "body.set_color",
        json!({"i": 2, "r": 0, "g": 0, "b": 1}),
    );
    op(
        &mut app,
        "body.set_color",
        json!({"i": 3, "r": 0, "g": 0, "b": 1}),
    );
    let depth = undo_depth(&app);
    op(
        &mut app,
        "group.decompose",
        json!({"group": 1, "key": "color"}),
    );
    assert_eq!(undo_depth(&app), depth + 1, "one step");
    // Group 0 (the circles) survives; the outer group became red + blue.
    let groups = op(&mut app, "group.list", json!({})).expect("a list");
    assert_eq!(groups.as_array().map(Vec::len), Some(3));
    assert_eq!(
        op(&mut app, "group.members", json!({"group": 1})),
        Some(json!([0, 1]))
    );
    assert_eq!(
        op(&mut app, "group.members", json!({"group": 2})),
        Some(json!([2, 3]))
    );
    // Body 2 keeps its inner group under the new outer one.
    assert_eq!(
        op(&mut app, "group.members", json!({"group": 0})),
        Some(json!([0, 1, 2]))
    );
}

#[test]
fn a_filter_result_becomes_a_group_whose_members_round_trip() {
    let mut app = scene();
    let circles = op(
        &mut app,
        "body.where",
        json!({"clauses": ["kind:circle", "not:palette:99"]}),
    )
    .expect("a list");
    assert_eq!(circles, json!([0, 1, 2]));
    assert_eq!(
        op(&mut app, "body.where", json!({"clauses": ["ungrouped"]})),
        Some(json!([3]))
    );
    let bad = invoke(
        app.world_mut(),
        "body.where",
        json!({"clauses": ["x:0..2"]}),
    );
    assert!(
        bad.is_err(),
        "x is not a table property, so the clause is an error"
    );
    let some = op(&mut app, "body.where", json!({"clauses": ["not:kind:box"]})).expect("a list");
    let g = op(&mut app, "group.create", json!({"targets": some})).expect("an index");
    assert_eq!(
        op(&mut app, "group.members", json!({"group": g})),
        Some(json!([0, 1, 2]))
    );
}

#[test]
fn set_color_is_reversible() {
    let mut app = scene();
    let before: Vec<Appearance> = values(&mut app);
    op(
        &mut app,
        "body.set_color",
        json!({"i": 3, "r": 0.1, "g": 0.8, "b": 0.2}),
    );
    let after: Vec<Appearance> = values(&mut app);
    assert_eq!(
        (after[3].fill.r, after[3].fill.g, after[3].fill.b),
        (0.1, 0.8, 0.2)
    );
    assert_eq!(after[3].border, before[3].border);
    undo(&mut app);
    step(&mut app, 1);
    assert_eq!(values::<Appearance>(&mut app), before);
}

/// Every body's group stack, in id order (empty when ungrouped).
fn stacks(app: &mut App) -> Vec<Vec<u32>> {
    let mut rows: Vec<(StableId, Vec<u32>)> = app
        .world_mut()
        .query_filtered::<(&StableId, Option<&SelectionGroup>), With<Body>>()
        .iter(app.world())
        .map(|(id, g)| (*id, g.map_or_else(Vec::new, |g| g.0.clone())))
        .collect();
    rows.sort_by_key(|(id, _)| id.0);
    rows.into_iter().map(|(_, s)| s).collect()
}

/// Groups nest or stay apart; they never overlap: every two groups are
/// disjoint or one holds the other, and each body's stack runs from the
/// smallest group to the largest.
fn assert_groups_nest(stacks: &[Vec<u32>]) {
    let members = |g: u32| -> Vec<usize> {
        (0..stacks.len())
            .filter(|&i| stacks[i].contains(&g))
            .collect()
    };
    for stack in stacks {
        for pair in stack.windows(2) {
            let (inner, outer) = (members(pair[0]), members(pair[1]));
            assert!(
                inner.iter().all(|m| outer.contains(m)),
                "group {} sits inside {} but is not a subset: {stacks:?}",
                pair[0],
                pair[1]
            );
        }
    }
}

/// Grouping a member of a group with an outside body groups the member's
/// whole group (as a viewport click selects it): the circles' group nests
/// inside the new one, and no body is in two sibling groups (A-1 finding 8,
/// brief A-2; archive entry `group-overlap`).
#[test]
fn grouping_a_grouped_body_wraps_its_whole_group() {
    let mut app = scene();
    let depth = undo_depth(&app);
    assert_eq!(
        op(&mut app, "group.create", json!({"targets": [2, 3]})),
        Some(json!(1))
    );
    assert_eq!(undo_depth(&app), depth + 1, "one step");
    let after = stacks(&mut app);
    assert_groups_nest(&after);
    let outer = after[3][0];
    assert!(
        after.iter().all(|s| s.last() == Some(&outer)),
        "the new group holds all four bodies: {after:?}"
    );
    undo(&mut app);
    step(&mut app, 1);
    assert!(stacks(&mut app)[3].is_empty(), "undo restores the box");
}

/// Grouping part of a group nests the new group **inside** it: the outer
/// group still holds all three circles (a click still selects them all),
/// and the two picked circles share an inner group.
#[test]
fn grouping_part_of_a_group_nests_inside_it() {
    let mut app = scene();
    let before = stacks(&mut app);
    let circles = before[0][0];
    op(&mut app, "group.create", json!({"targets": [0, 1]}));
    let after = stacks(&mut app);
    assert_groups_nest(&after);
    for (i, stack) in after.iter().enumerate().take(3) {
        assert_eq!(stack.last(), Some(&circles), "circle {i}: {after:?}");
    }
    assert_eq!(after[0], after[1], "the two picked circles share a stack");
    assert_eq!(after[0].len(), 2, "{after:?}");
    assert_eq!(after[2], vec![circles]);
}

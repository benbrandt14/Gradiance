//! The Groups panel end to end (brief 4b-2): the panel's leaf renderer is
//! drawn with `egui_kittest` over a model built from a headless app, a
//! button is clicked by label, and what the panel recorded (operations to
//! invoke, a selection filter) is written into the app as the dock writes
//! it. Headless there is no egui pass, so `ui.click` cannot reach the
//! panel; this is the same path one frame later.

use crate::harness::{paused_app, step};
use bevy::prelude::*;
use bevy_egui::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gradiance::core::ids::StableId;
use gradiance::domain::Body;
use gradiance::domain::appearance::{Appearance, PALETTE};
use gradiance::domain::group::{GroupIndex, GroupNames, SelectionGroup};
use gradiance::domain::shape::ShapeDef;
use gradiance::interaction::selection::Selection;
use gradiance::op::{OpIdSource, invoke};
use gradiance::script::bridge::ScriptInputs;
use gradiance::ui::groups::{
    BodyFacts, GroupsFilter, GroupsModel, GroupsOutput, GroupsPanel, groups_section,
};
use serde_json::json;
use std::cell::RefCell;

/// Four circles in a row: two red, two blue (palette slots 0 and 7).
fn scene() -> App {
    let mut app = paused_app();
    app.insert_resource(OpIdSource::Sequence(100));
    app.world_mut().resource_mut::<ScriptInputs>().submit(
        "(begin (spawn-circle -1 1 0.3) (spawn-circle 0 1 0.3) (spawn-circle 1 1 0.3) \
         (spawn-circle 2 1 0.3))",
    );
    step(&mut app, 2);
    for (i, slot) in [(0, 0), (1, 7), (2, 0), (3, 7)] {
        let c = PALETTE[slot].color;
        invoke(
            app.world_mut(),
            "body.set_color",
            json!({"i": i, "r": c.r, "g": c.g, "b": c.b}),
        )
        .unwrap_or_else(|e| panic!("set_color: {e}"));
    }
    step(&mut app, 1);
    app
}

/// The panel's model, as the dock builds it.
fn model(app: &mut App) -> GroupsModel {
    let facts = app
        .world_mut()
        .query_filtered::<(&StableId, &ShapeDef, Option<&SelectionGroup>, &Appearance), With<Body>>(
        )
        .iter(app.world())
        .map(|(id, shape, groups, appearance)| BodyFacts {
            id: *id,
            groups: groups.map_or_default(|g| g.0.clone()),
            fill: appearance.fill,
            kind: shape.kind_label(),
        })
        .collect();
    let names = app.world().resource::<GroupNames>().clone();
    let selected = app.world().resource::<Selection>().len();
    GroupsModel::build(facts, &names, selected)
}

/// Draws the panel, clicks each label in turn (one frame per click), and
/// returns what it recorded.
fn click(model: &GroupsModel, panel: &RefCell<GroupsPanel>, labels: &[&str]) -> GroupsOutput {
    let out = RefCell::new(GroupsOutput::default());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 640.0))
        .build_ui(|ui| {
            groups_section(ui, model, &mut panel.borrow_mut(), &mut out.borrow_mut());
        });
    harness.run();
    for label in labels {
        harness.get_by_label(label).click();
        harness.run();
    }
    drop(harness);
    out.into_inner()
}

/// Writes what the panel recorded, as the dock does, and lets it commit.
fn apply(app: &mut App, out: GroupsOutput) {
    for op in out.invoke {
        invoke(app.world_mut(), &op.id, op.params).unwrap_or_else(|e| panic!("{}: {e}", op.id));
    }
    if let Some(filter) = out.filter {
        app.world_mut().write_message(filter);
    }
    step(app, 2);
}

fn selected_fills(app: &mut App) -> Vec<usize> {
    let selection: Vec<Entity> = app.world().resource::<Selection>().iter().collect();
    let mut slots: Vec<usize> = selection
        .iter()
        .filter_map(|e| app.world().get::<Appearance>(*e))
        .filter_map(|a| gradiance::domain::appearance::palette_slot(a.fill))
        .collect();
    slots.sort_unstable();
    slots
}

fn group_count(app: &mut App) -> usize {
    let stacks: Vec<(StableId, Vec<u32>)> = app
        .world_mut()
        .query_filtered::<(&StableId, Option<&SelectionGroup>), With<Body>>()
        .iter(app.world())
        .map(|(id, g)| (*id, g.map_or_default(|g| g.0.clone())))
        .collect();
    GroupIndex::build(stacks.iter().map(|(id, s)| (*id, s.as_slice()))).len()
}

/// Task 3: one chip click selects every red body, and nothing else.
#[test]
fn the_red_chip_selects_every_red_body() {
    let mut app = scene();
    let panel = RefCell::new(GroupsPanel::default());
    let out = click(&model(&mut app), &panel, &["red"]);
    assert_eq!(
        out.filter,
        Some(GroupsFilter {
            clauses: vec!["palette:0".into()]
        })
    );
    apply(&mut app, out);
    assert_eq!(selected_fills(&mut app), [0, 0]);

    // Clearing the chip clears the selection.
    let out = click(&model(&mut app), &panel, &["red"]);
    apply(&mut app, out);
    assert_eq!(selected_fills(&mut app), Vec::<usize>::new());
}

/// Task 4: a mixed group's row selects it, then "By colour" splits it into
/// one group per colour in one undo step.
#[test]
fn a_row_then_split_by_colour_splits_a_mixed_group() {
    let mut app = scene();
    invoke(
        app.world_mut(),
        "group.create",
        json!({"targets": [0, 1, 2, 3]}),
    )
    .unwrap_or_else(|e| panic!("group: {e}"));
    step(&mut app, 1);
    assert_eq!(group_count(&mut app), 1);

    let panel = RefCell::new(GroupsPanel::default());
    let out = click(&model(&mut app), &panel, &["Group 0"]);
    assert!(out.invoke.is_empty(), "a row click edits nothing");
    apply(&mut app, out);
    assert_eq!(
        selected_fills(&mut app),
        [0, 0, 7, 7],
        "the row selected the group"
    );

    let out = click(&model(&mut app), &panel, &["By colour"]);
    let ids: Vec<&str> = out.invoke.iter().map(|op| op.id.as_ref()).collect();
    assert_eq!(ids, ["group.decompose"]);
    assert_eq!(out.invoke[0].params, json!({"group": 0, "key": "color"}));
    let before = app
        .world()
        .resource::<gradiance::command::CommandStack>()
        .undo_len();
    apply(&mut app, out);
    assert_eq!(group_count(&mut app), 2, "one group per colour");
    assert_eq!(
        app.world()
            .resource::<gradiance::command::CommandStack>()
            .undo_len(),
        before + 1,
        "one undo step"
    );
}

/// A chip on a **mixed** group selects exactly the matching bodies, and a
/// nested group's row selects that group, not its outer one: the panel's
/// selection is a query result, not a viewport click, so it is not expanded
/// to outermost groups (brief A-1; the red chip used to select the whole
/// red-and-blue group).
#[test]
fn a_chip_on_a_mixed_group_selects_only_the_matching_bodies() {
    let mut app = scene();
    let inner = invoke(app.world_mut(), "group.create", json!({"targets": [0, 1]}))
        .unwrap_or_else(|e| panic!("group: {e}"))
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| panic!("no group index"));
    step(&mut app, 1);

    let panel = RefCell::new(GroupsPanel::default());
    let out = click(&model(&mut app), &panel, &["red"]);
    apply(&mut app, out);
    assert_eq!(
        selected_fills(&mut app),
        [0, 0],
        "the red chip selects the two red bodies, not the blue group member"
    );

    // Nest the mixed pair in an outer group with the third circle.
    invoke(
        app.world_mut(),
        "group.create",
        json!({"targets": [0, 1, 2]}),
    )
    .unwrap_or_else(|e| panic!("group: {e}"));
    step(&mut app, 1);
    let names = invoke(app.world_mut(), "group.list", json!({}))
        .unwrap_or_else(|e| panic!("groups: {e}"))
        .and_then(|v| v.as_array().map(Vec::len));
    assert_eq!(names, Some(2));
    let members = invoke(app.world_mut(), "group.members", json!({"group": inner}))
        .unwrap_or_else(|e| panic!("members: {e}"));
    assert_eq!(
        members,
        Some(json!([0, 1])),
        "the inner group is still [0, 1]"
    );
    app.world_mut().write_message(GroupsFilter {
        clauses: vec![format!("group:{inner}")],
    });
    step(&mut app, 2);
    assert_eq!(
        selected_fills(&mut app),
        [0, 7],
        "the inner group's row selects the inner pair only"
    );
}

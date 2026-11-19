//! Workspaces and areas (brief U-10), driven headless through their
//! operations: the docks' trees live in `Update`, so a workspace's editors,
//! tile order and sizes are readable without a window.

use crate::harness::{body_count, paused_app};
use bevy::prelude::*;
use gradiance::domain::workspace::Workspaces;
use gradiance::scene::{SceneRecord, from_ron, to_ron};
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::ui::bottom_dock::BottomDock;
use gradiance::ui::dock::RightDock;
use gradiance::ui::panels::PanelToggle;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    // The requests apply in the frame they are made; the docks sync after.
    app.update();
}

fn last_ok(app: &App) -> (bool, String) {
    app.world()
        .resource::<ScriptLog>()
        .0
        .last()
        .map_or_default(|e| (e.ok, e.output.clone()))
}

/// The right dock's editors in tile order, by panel name.
fn right(app: &App) -> Vec<&'static str> {
    app.world()
        .resource::<RightDock>()
        .pane_order()
        .into_iter()
        .map(gradiance::ui::dock::Pane::name)
        .collect()
}

fn bottom(app: &App) -> Vec<&'static str> {
    app.world()
        .resource::<BottomDock>()
        .pane_order()
        .into_iter()
        .map(gradiance::ui::bottom_dock::BottomPane::name)
        .collect()
}

/// The dock editors whose panels are open, by panel name.
fn open_panels(app: &mut App) -> Vec<String> {
    use gradiance::ui;
    let w = app.world();
    let rows: [(&str, bool); 7] = [
        (
            "outliner",
            w.resource::<ui::outliner::ObjectTreePanel>().is_open(),
        ),
        ("groups", w.resource::<ui::groups::GroupsPanel>().is_open()),
        (
            "depth",
            w.resource::<ui::depth_panel::DepthPanel>().is_open(),
        ),
        ("plot", w.resource::<ui::plot::PlotPanel>().is_open()),
        (
            "properties",
            w.resource::<ui::inspector::InspectorPanel>().is_open(),
        ),
        (
            "console",
            w.resource::<ui::console::ScriptConsole>().is_open(),
        ),
        ("nodes", w.resource::<ui::node_graph::NodeGraph>().is_open()),
    ];
    rows.into_iter()
        .filter(|(_, open)| *open)
        .map(|(name, _)| name.to_owned())
        .collect()
}

#[test]
fn each_preset_sets_the_visible_editors_and_the_tile_order() {
    let mut app = paused_app();
    run(&mut app, "(workspace-select \"Scripting\")");
    assert_eq!(right(&app), ["outliner", "console", "properties"]);
    assert_eq!(bottom(&app), Vec::<&str>::new());
    let open = open_panels(&mut app);
    for name in ["outliner", "console", "properties"] {
        assert!(open.iter().any(|o| o == name), "{name} open: {open:?}");
    }
    assert!(!open.iter().any(|o| o == "nodes"), "{open:?}");
    assert_eq!(app.world().resource::<Workspaces>().active, "Scripting");

    run(&mut app, "(workspace-select \"signals\")");
    assert_eq!(right(&app), ["plot", "properties"]);
    assert_eq!(bottom(&app), ["nodes"]);
    let open = open_panels(&mut app);
    assert!(
        !open.iter().any(|o| o == "console" || o == "outliner"),
        "{open:?}"
    );
    assert_eq!(app.world().resource::<Workspaces>().active, "Signals");

    run(&mut app, "(workspace-select \"Layout\")");
    assert_eq!(right(&app), ["outliner", "properties"]);
    assert_eq!(bottom(&app), Vec::<&str>::new());
}

/// A floating window is not part of a workspace: switching leaves it alone.
#[test]
fn a_workspace_leaves_floating_windows_alone() {
    let mut app = paused_app();
    run(&mut app, "(panel-show \"settings\")");
    run(&mut app, "(workspace-select \"Signals\")");
    assert!(
        app.world()
            .resource::<gradiance::ui::settings::SettingsWindow>()
            .is_open()
    );
}

#[test]
fn a_saved_workspace_round_trips_through_select_and_the_scene_file() {
    let mut app = paused_app();
    run(&mut app, "(workspace-select \"Signals\")");
    run(&mut app, "(panel-swap \"plot\" \"console\")");
    assert_eq!(right(&app), ["console", "properties"]);
    run(&mut app, "(layout-save \"Mine\")");
    let (ok, output) = last_ok(&app);
    assert!(ok, "{output}");
    assert_eq!(app.world().resource::<Workspaces>().active, "Mine");

    run(&mut app, "(workspace-select \"Layout\")");
    assert_eq!(right(&app), ["outliner", "properties"]);
    run(&mut app, "(workspace-select \"mine\")");
    assert_eq!(right(&app), ["console", "properties"]);
    assert_eq!(bottom(&app), ["nodes"]);

    run(&mut app, "(length (workspaces))");
    let (ok, output) = last_ok(&app);
    assert!(ok, "{output}");
    assert_eq!(output, "4", "three presets and one saved");
    let mirror =
        gradiance::ui::workspaces::WorkspaceMirror::of(app.world().resource::<Workspaces>());
    let names: Vec<&str> = mirror.rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Layout", "Scripting", "Signals", "Mine"]);

    // Workstation config travels with the scene file, like the grid.
    let record = SceneRecord::capture(app.world_mut());
    let text = to_ron(&record).expect("serialise");
    let back = from_ron(&text).expect("parse");
    assert_eq!(
        back.environment.workspaces,
        *app.world().resource::<Workspaces>()
    );
    let saved = back.environment.workspaces.find("Mine").expect("saved");
    assert_eq!(saved.right.panes(), ["console", "properties"]);
    assert_eq!(saved.bottom.panes(), ["nodes"]);

    // A layout drifts, then `layout-apply` puts the saved one back.
    run(&mut app, "(panel-hide \"properties\")");
    assert_eq!(right(&app), ["console"]);
    run(&mut app, "(layout-apply)");
    assert_eq!(right(&app), ["console", "properties"]);

    // Deleting the active one falls back to Layout; a preset stays.
    run(&mut app, "(workspace-delete \"Mine\")");
    assert_eq!(app.world().resource::<Workspaces>().active, "Layout");
    assert!(app.world().resource::<Workspaces>().find("Mine").is_none());
    run(&mut app, "(workspace-delete \"Layout\")");
    let (ok, output) = last_ok(&app);
    assert!(!ok && output.contains("preset"), "{output}");
}

#[test]
fn switching_workspaces_records_no_undo_step_and_no_scene_content() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 1 1)");
    assert_eq!(body_count(&mut app), 1);
    let depth = |app: &App| {
        app.world()
            .resource::<gradiance::command::HistoryInfo>()
            .undo_depth
    };
    let before = depth(&app);
    let scene = SceneRecord::capture(app.world_mut());
    run(&mut app, "(workspace-select \"Scripting\")");
    run(&mut app, "(layout-save \"Mine\")");
    run(&mut app, "(panel-swap \"outliner\" \"groups\")");
    run(&mut app, "(workspace-delete \"Mine\")");
    // Let the settings watcher see a settled state.
    app.update();
    app.update();
    assert_eq!(depth(&app), before, "no undo step");
    let after = SceneRecord::capture(app.world_mut());
    assert!(scene.authored_eq(&after), "scene content untouched");
}

/// A script switches a tile's editor: the tile keeps its place, the old
/// editor closes, and exchanging with a shown editor closes nothing.
#[test]
fn a_script_switches_a_tiles_editor() {
    let mut app = paused_app();
    run(&mut app, "(workspace-select \"Layout\")");
    run(&mut app, "(panel-swap \"outliner\" \"groups\")");
    assert_eq!(right(&app), ["groups", "properties"]);
    let open = open_panels(&mut app);
    assert!(!open.iter().any(|o| o == "outliner"), "{open:?}");
    assert!(open.iter().any(|o| o == "groups"), "{open:?}");

    run(&mut app, "(panel-swap \"groups\" \"properties\")");
    assert_eq!(right(&app), ["properties", "groups"]);
    let open = open_panels(&mut app);
    assert!(open.iter().any(|o| o == "groups"), "{open:?}");

    // Across docks is refused at the call.
    run(&mut app, "(panel-swap \"groups\" \"nodes\")");
    let (ok, output) = last_ok(&app);
    assert!(!ok && output.contains("different sides"), "{output}");
}

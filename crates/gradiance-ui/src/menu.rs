//! The top **menu bar** — File / Edit / View / Help — the first piece of the
//! desktop-app shell (`docs/ui-shell-decision.md`). It is a thin projection: every
//! item routes to an existing intent (undo/redo/delete/group, scene save/load) or
//! toggles an existing panel resource, so it adds discoverability, not a new
//! mutation path. Docked at the screen top via the same background-layer root
//! pattern the other panels use; its rect feeds [`PanelRects`] so clicks don't
//! leak to the scene.

use crate::PanelRects;
use crate::panels::PanelToggle;
use crate::widgets;
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_command::intent::{
    DeleteIntent, GroupIntent, LoadSceneIntent, RedoIntent, UndoIntent, UngroupIntent,
};
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::settings::GridSettings;
use gradiance_interaction::selection::Selection;
use gradiance_persist::{LoadSceneRequest, SaveSceneRequest, SnapshotRequest};
use gradiance_scene::FORMAT_VERSION;
use gradiance_scene::{EnvironmentRecord, SceneRecord};

/// The About dialog's visibility (Help ▸ About).
#[derive(Resource, Default)]
pub struct AboutWindow {
    open: bool,
}

/// Every panel the View menu offers, bundled so `menu_bar` stays under Bevy's
/// system-parameter limit. Each field implements
/// [`PanelToggle`], which is what lets the View menu be a table rather
/// than a branch per panel.
#[derive(SystemParam)]
pub struct Panels<'w> {
    /// Settings window.
    pub settings: ResMut<'w, crate::settings::SettingsWindow>,
    /// Properties inspector dock pane.
    pub inspector: ResMut<'w, crate::inspector::InspectorPanel>,
    /// Depth-band editor dock pane.
    pub depth: ResMut<'w, crate::depth_panel::DepthPanel>,
    /// Live plot panel.
    pub plot: ResMut<'w, crate::plot::PlotPanel>,
    /// Physics probe panel.
    pub probe: ResMut<'w, crate::probe::ProbePanel>,
    /// Signals dock section.
    pub signals: ResMut<'w, crate::signals::SignalsPanel>,
    /// Node-graph canvas.
    pub node_graph: ResMut<'w, crate::node_graph::NodeGraph>,
    /// Object tree (outliner).
    pub outliner: ResMut<'w, crate::outliner::ObjectTreePanel>,
    /// Scripting console.
    pub console: ResMut<'w, crate::console::ScriptConsole>,
    /// Array-pattern options window.
    pub array: ResMut<'w, crate::array_panel::ArrayWindow>,
    /// Layout-optimizer window.
    pub optimizer: ResMut<'w, crate::optimizer::OptimizerExpanded>,
    /// Debug overlays (field vectors).
    pub debug: ResMut<'w, gradiance_domain::settings::DebugSettings>,
}

/// Every message the menu bar can emit, grouped to stay under the
/// system-parameter limit (same idiom as `interaction::input::ShortcutWriters`).
#[derive(SystemParam)]
pub struct MenuWriters<'w> {
    undo: MessageWriter<'w, UndoIntent>,
    redo: MessageWriter<'w, RedoIntent>,
    delete: MessageWriter<'w, DeleteIntent>,
    group: MessageWriter<'w, GroupIntent>,
    ungroup: MessageWriter<'w, UngroupIntent>,
    load_scene: MessageWriter<'w, LoadSceneIntent>,
    save: MessageWriter<'w, SaveSceneRequest>,
    load: MessageWriter<'w, LoadSceneRequest>,
    snapshot: MessageWriter<'w, SnapshotRequest>,
    exit: MessageWriter<'w, AppExit>,
}

/// An empty scene (the File ▸ New target) — the current format, no content.
fn empty_scene() -> SceneRecord {
    SceneRecord {
        version: FORMAT_VERSION,
        app_version: String::new(),
        bodies: Vec::new(),
        joints: Vec::new(),
        nodes: Vec::new(),
        environment: EnvironmentRecord::default(),
    }
}

/// Renders the top menu bar and its About dialog.
pub fn menu_bar(
    mut contexts: EguiContexts,
    mut panels: Panels,
    mut writers: MenuWriters,
    mut grid: ResMut<GridSettings>,
    mut about: ResMut<AboutWindow>,
    mut panel_rects: ResMut<PanelRects>,
    selection: Res<Selection>,
    ids: Query<&StableId, With<Body>>,
    history: Res<gradiance_command::HistoryInfo>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let selected_ids: Vec<StableId> = selection
        .iter()
        .filter_map(|e| ids.get(e).ok().copied())
        .collect();
    let has_selection = !selected_ids.is_empty();

    let mut root = egui::Ui::new(
        ctx.clone(),
        "menu-bar-root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let bar = egui::Panel::top("menu-bar").show(&mut root, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            file_menu(ui, &mut writers);
            edit_menu(ui, &mut writers, has_selection, &selected_ids, &history);
            view_menu(ui, &mut panels, &mut grid);
            help_menu(ui, &mut about);
        });
    });
    panel_rects.push(bar.response.rect);

    let mut open = about.open;
    egui::Window::new("About Gradiance")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            widgets::section_header(ui, "Gradiance");
            ui.label("An Algodoo-inspired 2.5D physics sandbox.");
            ui.label(format!("Scene format v{FORMAT_VERSION}"));
        });
    about.open = open;
    Ok(())
}

fn file_menu(ui: &mut egui::Ui, writers: &mut MenuWriters) {
    ui.menu_button("File", |ui| {
        if ui.button("New scene").clicked() {
            writers.load_scene.write(LoadSceneIntent {
                scene: empty_scene(),
            });
            ui.close();
        }
        if ui.button("Open…").clicked() {
            writers.load.write(LoadSceneRequest { path: None });
            ui.close();
        }
        ui.separator();
        if ui.button("Save").clicked() {
            writers.save.write(SaveSceneRequest { path: None });
            ui.close();
        }
        if ui.button("Save snapshot").clicked() {
            writers.snapshot.write(SnapshotRequest::default());
            ui.close();
        }
        ui.separator();
        if ui.button("Quit").clicked() {
            writers.exit.write(AppExit::Success);
            ui.close();
        }
    });
}

/// Builds an Edit-menu label like `Undo Spawn Body` from a verb and the
/// pending step's kebab-case name (`intent::name`), falling back to the bare
/// verb when there is nothing to undo.
fn step_label(verb: &str, step: Option<&'static str>) -> String {
    let Some(step) = step.filter(|s| !s.is_empty()) else {
        return verb.to_owned();
    };
    let mut out = String::from(verb);
    for word in step.split('-') {
        out.push(' ');
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

fn edit_menu(
    ui: &mut egui::Ui,
    writers: &mut MenuWriters,
    has_selection: bool,
    selected_ids: &[StableId],
    history: &gradiance_command::HistoryInfo,
) {
    ui.menu_button("Edit", |ui| {
        // Naming the pending step matters mid-run: a simulation run is one
        // undo step, so the first press after playing reads "Undo Simulate"
        // instead of appearing to ignore the edit the user has in mind.
        if ui
            .add_enabled(
                history.undo_depth > 0,
                egui::Button::new(step_label("Undo", history.undo_label)),
            )
            .clicked()
        {
            writers.undo.write(UndoIntent);
            ui.close();
        }
        if ui
            .add_enabled(
                history.redo_depth > 0,
                egui::Button::new(step_label("Redo", history.redo_label)),
            )
            .clicked()
        {
            writers.redo.write(RedoIntent);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_selection, egui::Button::new("Delete selection"))
            .clicked()
        {
            writers.delete.write(DeleteIntent {
                targets: selected_ids.to_vec(),
            });
            ui.close();
        }
        if ui
            .add_enabled(has_selection, egui::Button::new("Group"))
            .clicked()
        {
            writers.group.write(GroupIntent {
                targets: selected_ids.to_vec(),
            });
            ui.close();
        }
        if ui
            .add_enabled(has_selection, egui::Button::new("Ungroup"))
            .clicked()
        {
            writers.ungroup.write(UngroupIntent {
                targets: selected_ids.to_vec(),
            });
            ui.close();
        }
    });
}

/// The View menu, as a table. Every panel implements [`PanelToggle`], so the
/// two idioms that used to need separate code paths are one row each — and a
/// new panel is a row, not a branch. Grouped: right-dock sections, then the
/// bottom dock and floating windows, then scene overlays.
fn view_menu(ui: &mut egui::Ui, panels: &mut Panels, grid: &mut GridSettings) {
    ui.menu_button("View", |ui| {
        // (label, shortcut hint, panel). The hint is the real binding — see
        // `dock::right_dock` for `` ` `` and `\`; an empty hint means unbound.
        let dock_sections: [(&str, &str, &mut dyn PanelToggle); 5] = [
            ("Outliner", "", &mut *panels.outliner),
            ("Properties", "", &mut *panels.inspector),
            ("Depth", "", &mut *panels.depth),
            ("Signals", "", &mut *panels.signals),
            ("Plot", "\\", &mut *panels.plot),
        ];
        for (label, shortcut, panel) in dock_sections {
            toggle_item(ui, label, shortcut, panel);
        }
        ui.separator();
        let windows: [(&str, &str, &mut dyn PanelToggle); 5] = [
            ("Node Graph", "", &mut *panels.node_graph),
            ("Script console", "`", &mut *panels.console),
            ("Probe", "", &mut *panels.probe),
            ("Array", "", &mut *panels.array),
            ("Optimizer", "", &mut *panels.optimizer),
        ];
        for (label, shortcut, panel) in windows {
            toggle_item(ui, label, shortcut, panel);
        }
        ui.separator();
        toggle_item(ui, "Settings", "", &mut *panels.settings);
        ui.checkbox(&mut grid.visible, "Grid");
        ui.checkbox(&mut panels.debug.show_fields, "Field overlay");
    });
}

/// One View-menu row: a checkbox bound to the panel, with its keyboard
/// shortcut right-aligned when it has one.
fn toggle_item(ui: &mut egui::Ui, label: &str, shortcut: &str, panel: &mut dyn PanelToggle) {
    let mut shown = panel.is_open();
    // `Checkbox` has no `shortcut_text` (only `Button` does), so build the same
    // shape from atoms: label, a grow spacer, then the key in weak text.
    let response = if shortcut.is_empty() {
        ui.checkbox(&mut shown, label)
    } else {
        ui.add(egui::Checkbox::new(
            &mut shown,
            (
                egui::Atom::from(label),
                egui::Atom::grow(),
                egui::Atom::from(egui::RichText::new(shortcut).weak()),
            ),
        ))
    };
    if response.changed() {
        panel.set_open(shown);
    }
}

fn help_menu(ui: &mut egui::Ui, about: &mut AboutWindow) {
    ui.menu_button("Help", |ui| {
        if ui.button("About Gradiance").clicked() {
            about.open = true;
            ui.close();
        }
    });
}

//! The top **menu bar** — File / Edit / View / Help — the first piece of the
//! desktop-app shell (`docs/ui-shell-decision.md`). It is a thin projection: every
//! item routes to an existing intent (undo/redo/delete/group, scene save/load) or
//! toggles an existing panel resource, so it adds discoverability, not a new
//! mutation path. Docked at the screen top via the same background-layer root
//! pattern the other panels use; its rect feeds [`PanelRects`] so clicks don't
//! leak to the scene.

use crate::command::intent::{
    DeleteIntent, GroupIntent, LoadSceneIntent, RedoIntent, UndoIntent, UngroupIntent,
};
use crate::command::snapshot::{EnvironmentRecord, SceneRecord};
use crate::core::ids::StableId;
use crate::domain::Body;
use crate::domain::settings::GridSettings;
use crate::interaction::selection::Selection;
use crate::persist::{FORMAT_VERSION, LoadSceneRequest, SaveSceneRequest, SnapshotRequest};
use crate::ui::PanelRects;
use crate::ui::toolbar::Panels;
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// The About dialog's visibility (Help ▸ About).
#[derive(Resource, Default)]
pub struct AboutWindow {
    open: bool,
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
            edit_menu(ui, &mut writers, has_selection, &selected_ids);
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
            ui.label(egui::RichText::new("Gradiance").strong());
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

fn edit_menu(
    ui: &mut egui::Ui,
    writers: &mut MenuWriters,
    has_selection: bool,
    selected_ids: &[StableId],
) {
    ui.menu_button("Edit", |ui| {
        if ui.button("Undo").clicked() {
            writers.undo.write(UndoIntent);
            ui.close();
        }
        if ui.button("Redo").clicked() {
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

fn view_menu(ui: &mut egui::Ui, panels: &mut Panels, grid: &mut GridSettings) {
    ui.menu_button("View", |ui| {
        // Panels whose visibility is a plain bool field.
        ui.checkbox(&mut panels.inspector.open, "Properties");
        ui.checkbox(&mut panels.settings.open, "Settings");
        ui.separator();
        // Panels toggled through their `is_open`/`toggle` API.
        toggle_item(ui, "Outliner", panels.outliner.is_open(), || {
            panels.outliner.toggle();
        });
        toggle_item(ui, "Node Graph", panels.node_graph.is_open(), || {
            panels.node_graph.toggle();
        });
        toggle_item(ui, "Signals", panels.signals.is_open(), || {
            panels.signals.toggle();
        });
        toggle_item(ui, "Plot", panels.plot.is_open(), || panels.plot.toggle());
        toggle_item(ui, "Probe", panels.probe.is_open(), || {
            panels.probe.toggle();
        });
        toggle_item(ui, "Console", panels.console.is_open(), || {
            panels.console.toggle();
        });
        ui.separator();
        ui.checkbox(&mut grid.visible, "Grid");
        ui.checkbox(&mut panels.debug.show_fields, "Field overlay");
    });
}

/// A menu checkbox for a panel exposed only through `is_open`/`toggle`.
fn toggle_item(ui: &mut egui::Ui, label: &str, open: bool, mut toggle: impl FnMut()) {
    let mut shown = open;
    if ui.checkbox(&mut shown, label).changed() {
        toggle();
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

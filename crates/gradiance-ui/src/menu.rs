//! The top **menu bar** — File / Edit / View / Help — the first piece of the
//! desktop-app shell (`docs/ui-shell-decision.md`). It is a thin projection: every
//! item invokes a registry operation (undo/redo/delete/group), routes to an
//! existing request (scene save/load) or toggles an existing panel resource,
//! so it adds discoverability, not a new mutation path. Docked at the screen top via the same background-layer root
//! pattern the other panels use; its rect feeds [`PanelRects`] so clicks don't
//! leak to the scene.

use crate::PanelRects;
use crate::panels::PanelToggle;
use crate::widgets;
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::settings::{DebugSettings, GridSettings};
use gradiance_interaction::debug_ops::DEBUG_TOGGLES;
use gradiance_interaction::input::{EditorAction, key_invocations};
use gradiance_interaction::selection::Selection;
use gradiance_op::{InvokeOp, json};
use gradiance_persist::SnapshotRequest;
use gradiance_scene::FORMAT_VERSION;

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
    /// Node-graph canvas.
    pub node_graph: ResMut<'w, crate::node_graph::NodeGraph>,
    /// Object tree (outliner).
    pub outliner: ResMut<'w, crate::outliner::ObjectTreePanel>,
    /// Groups panel (groups as editing layers).
    pub groups: ResMut<'w, crate::groups::GroupsPanel>,
    /// Scripting console.
    pub console: ResMut<'w, crate::console::ScriptConsole>,
    /// Array-pattern options window.
    pub array: ResMut<'w, crate::array_panel::ArrayWindow>,
    /// Layout-optimizer window.
    pub optimizer: ResMut<'w, crate::optimizer::OptimizerExpanded>,
    /// Review-mode window (questions as scenes).
    pub review: ResMut<'w, crate::review::ReviewWindow>,
    /// F3 operation search (brief U-9).
    pub search: ResMut<'w, crate::search::SearchPanel>,
    /// The status bar along the bottom edge (brief U-9).
    pub status: ResMut<'w, crate::status_bar::StatusBar>,
    /// Debug overlays (field vectors; View > Debug reads the physics
    /// toggles here and invokes their operations).
    pub debug: ResMut<'w, gradiance_domain::settings::DebugSettings>,
}

/// Every message the menu bar can emit, grouped to stay under the
/// system-parameter limit (same idiom as `interaction::input::ShortcutWriters`).
#[derive(SystemParam)]
pub struct MenuWriters<'w> {
    /// Edit rows invoke registry operations by id (`gradiance-op`); a
    /// conformance test checks every id resolves. The rows that repeat a key
    /// write that key's invocations ([`key_invocations`]), so the two cannot
    /// differ.
    invoke: MessageWriter<'w, InvokeOp>,
    snapshot: MessageWriter<'w, SnapshotRequest>,
    /// Edit ▸ Paste: the clipboard bridge reads the text (`clipboard`).
    paste: MessageWriter<'w, gradiance_interaction::input::PasteRequest>,
    exit: MessageWriter<'w, AppExit>,
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
    workspaces: Res<gradiance_domain::workspace::Workspaces>,
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
    // The bar is a header (Blender's top bar): darker than the panels.
    let header =
        egui::Frame::side_top_panel(&ctx.global_style()).fill(crate::theme::tokens(ctx).header);
    let bar = egui::Panel::top("menu-bar")
        .frame(header)
        .show(&mut root, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                file_menu(ui, &mut writers);
                edit_menu(ui, &mut writers, has_selection, &history);
                view_menu(ui, &mut panels, &mut grid, &mut writers.invoke);
                help_menu(ui, &mut about);
                // The workspace tabs sit in the top bar after the menus, as
                // Blender's do (brief U-10).
                let mut picks = Vec::new();
                crate::workspaces::workspace_tabs(ui, &workspaces, &mut picks);
                writers.invoke.write_batch(picks);
            });
        });
    panel_rects.push(bar.response.rect);
    crate::widget_registry::publish_rect("menu", bar.response.rect);

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
            writers.invoke.write(InvokeOp::new("scene.new"));
            ui.close();
        }
        if ui.button("Open…").clicked() {
            // An empty path: the dialog (`scene.load`, brief R-1).
            writers
                .invoke
                .write(InvokeOp::with("scene.load", json!({"path": ""})));
            ui.close();
        }
        if ui.button("Insert…").clicked() {
            // A scene file's objects added to this one (brief R-3); an
            // empty path is the dialog, `[]` keeps the file's positions.
            writers.invoke.write(InvokeOp::with(
                "scene.insert",
                json!({"path": "", "at": []}),
            ));
            ui.close();
        }
        ui.separator();
        if ui.button("Save").clicked() {
            // An empty path: the last one, else the dialog (`scene.save`).
            writers
                .invoke
                .write(InvokeOp::with("scene.save", json!({"path": ""})));
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
            writers.invoke.write(InvokeOp::new("edit.undo"));
            ui.close();
        }
        if ui
            .add_enabled(
                history.redo_depth > 0,
                egui::Button::new(step_label("Redo", history.redo_label)),
            )
            .clicked()
        {
            writers
                .invoke
                .write_batch(key_invocations(EditorAction::Redo));
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_selection, egui::Button::new("Copy"))
            .clicked()
        {
            writers
                .invoke
                .write_batch(key_invocations(EditorAction::Copy));
            ui.close();
        }
        if ui.button("Paste").clicked() {
            // From the menu the pointer is on the menu, not the canvas: the
            // copy lands where it was copied (Ctrl+V: at the cursor).
            writers
                .paste
                .write(gradiance_interaction::input::PasteRequest { at: None });
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_selection, egui::Button::new("Delete selection"))
            .clicked()
        {
            writers
                .invoke
                .write_batch(key_invocations(EditorAction::DeleteSelection));
            ui.close();
        }
        if ui
            .add_enabled(has_selection, egui::Button::new("Group"))
            .clicked()
        {
            writers
                .invoke
                .write_batch(key_invocations(EditorAction::Group));
            ui.close();
        }
        if ui
            .add_enabled(has_selection, egui::Button::new("Ungroup"))
            .clicked()
        {
            writers
                .invoke
                .write_batch(key_invocations(EditorAction::Ungroup));
            ui.close();
        }
        ui.separator();
        // No key: hotkeys are not settled (brief N-9).
        for (label, id) in MIRROR_ROWS {
            if ui
                .add_enabled(has_selection, egui::Button::new(label))
                .clicked()
            {
                writers
                    .invoke
                    .write(InvokeOp::with(id, json!({"targets": ["selection"]})));
                ui.close();
            }
        }
    });
}

/// Edit ▸ Mirror rows: the label and the operation each invokes on the
/// selection (brief N-9); the body context menu reads the same table
/// (brief Q-3).
pub(crate) const MIRROR_ROWS: [(&str, &str); 2] = [
    ("Mirror horizontally", "body.mirror_h"),
    ("Mirror vertically", "body.mirror_v"),
];

/// The View menu, as a table over the panel registry.
///
/// Every panel implements [`PanelToggle`], so the two idioms that used to need
/// separate code paths are one row each — and the rows come from
/// `Panels::named`, the same registry the `panel.*` operations
/// ([`crate::panel_ops`]) resolve against. A panel cannot appear in the menu but not the API, or
/// the reverse.
///
/// The groups are presentation only: right-dock sections, then the bottom dock
/// and floating windows, then scene overlays.
fn view_menu(
    ui: &mut egui::Ui,
    panels: &mut Panels,
    grid: &mut GridSettings,
    invoke: &mut MessageWriter<InvokeOp>,
) {
    // Where a horizontal rule goes, by row index. Presentation, not structure.
    const GROUPS: [usize; 2] = [5, 10];
    ui.menu_button("View", |ui| {
        let mut rows = panels.named();
        for (i, (name, panel)) in rows.iter_mut().enumerate() {
            if GROUPS.contains(&i) {
                ui.separator();
            }
            toggle_item(ui, menu_label(name), shortcut_for(name), &mut **panel);
        }
        ui.separator();
        ui.checkbox(&mut grid.visible, "Grid");
        ui.checkbox(&mut panels.debug.show_fields, "Field overlay");
        ui.menu_button("Debug", |ui| debug_menu(ui, &panels.debug, invoke));
    });
}

/// View > Debug: one checkbox per physics debug toggle (brief 6-3), a loop
/// over [`DEBUG_TOGGLES`]. A click invokes the toggle's operation, so the
/// menu and the `debug.*` verbs are one path. No key: the keymap is not
/// settled.
fn debug_menu(ui: &mut egui::Ui, debug: &DebugSettings, invoke: &mut MessageWriter<InvokeOp>) {
    for row in DEBUG_TOGGLES {
        let mut on = (row.get)(debug);
        if ui.checkbox(&mut on, row.label).changed() {
            invoke.write(InvokeOp::with(row.id, json!({ "on": on })));
        }
    }
}

/// The menu label for a registry name. Names are an API and stay stable; labels
/// are prose and may be retitled, which is why they are not the same string.
pub(crate) fn menu_label(name: &str) -> &'static str {
    match name {
        "outliner" => "Outliner",
        "groups" => "Groups",
        "properties" => "Properties",
        "depth" => "Depth",
        "plot" => "Plot",
        "nodes" => "Node Graph",
        "console" => "Script console",
        "probe" => "Probe",
        "array" => "Array",
        "optimizer" => "Optimizer",
        "settings" => "Settings",
        "review" => "Review",
        "search" => "Search operations",
        "status" => "Status bar",
        _ => "Panel",
    }
}

/// The keyboard shortcut shown against a panel's row, or empty when unbound.
/// The real bindings live in `dock::right_dock`; a shortcut that exists only in
/// code is a shortcut nobody finds.
fn shortcut_for(name: &str) -> &'static str {
    match name {
        "console" => "`",
        "plot" => "\\",
        "search" => "F3",
        _ => "",
    }
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

//! **Workspaces and areas** (brief U-10): saved dock layouts, a tab row that
//! switches between them, and the per-tile editor-type menu.
//!
//! A workspace ([`Workspace`], in `gradiance-domain`) names which editors
//! each dock shows, how its tiles split, and how large it is. Three presets
//! ship ([`presets`]): **Layout**, **Scripting** and **Signals**; the person
//! saves more from the tab row's `+`, and deletes their own from a tab's
//! context menu. The saved ones live in the [`Workspaces`] resource, which is
//! **workstation config**: saved with the scene file (there is no per-user
//! config file yet), never scene content, never in undo history.
//!
//! Every change is an operation, so the tab row, a script and the remote
//! surface do the same thing:
//!
//! | id | Lisp | what |
//! |---|---|---|
//! | `layout.save` | `layout-save` | save the live docks as a workspace |
//! | `layout.apply` | `layout-apply` | re-apply the active workspace |
//! | `layout.workspace.select` | `workspace-select` | switch workspace |
//! | `layout.workspace.list` | `workspaces` | the tabs, in order |
//! | `layout.workspace.delete` | `workspace-delete` | delete a saved one |
//! | `panel.swap` | `panel-swap` | a tile switches its editor |
//!
//! The writes lower to messages ([`LayoutRequest`], [`PanelSwapRequest`])
//! that [`apply_layout_requests`] applies: visibility through
//! [`PanelToggle`], the tree through [`crate::dock_sync`].
//! [`sync_docks`] then keeps each dock's tiles in step with the open
//! toggles, in `Update`, so all of it also runs headless.

use crate::bottom_dock::{BottomDock, BottomPane};
use crate::dock::{Pane, RightDock};
use crate::dock_sync;
use crate::menu::Panels;
use crate::panels::PanelToggle;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_domain::workspace::{DockLayout, LayoutNode, Workspace, Workspaces};
use gradiance_op::{
    InvokeOp, OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam,
    Value, json,
};
use serde::Deserialize;

/// The preset workspaces' names, in tab order.
pub const PRESETS: [&str; 3] = ["Layout", "Scripting", "Signals"];

/// The longest workspace name `layout.save` takes, in characters.
const MAX_NAME: usize = 32;

/// The egui-tiles id of each dock's tree.
const RIGHT_TREE: &str = "right-dock-tiles";
const BOTTOM_TREE: &str = "bottom-dock-tiles";

/// The three preset workspaces.
///
/// - **Layout**: the Outliner over Properties in the right dock, the
///   modelling arrangement (Blender's Layout).
/// - **Scripting**: a wide right dock, the Outliner over the console (the
///   console tabbed with Properties), large.
/// - **Signals**: the node graph large in the bottom dock, the plot over
///   Properties in the right.
pub fn presets() -> Vec<Workspace> {
    let workspace =
        |name: &str, right: Option<LayoutNode>, width, bottom: Option<LayoutNode>, height| {
            Workspace {
                name: name.to_owned(),
                right: DockLayout::from_node(right.as_ref(), width),
                bottom: DockLayout::from_node(bottom.as_ref(), height),
            }
        };
    vec![
        workspace(
            "Layout",
            Some(LayoutNode::Column(vec![
                (LayoutNode::tabs(&["outliner"]), 0.4),
                (LayoutNode::tabs(&["properties"]), 0.6),
            ])),
            crate::dock::RIGHT_DOCK_WIDTH,
            None,
            crate::bottom_dock::BOTTOM_DOCK_HEIGHT,
        ),
        workspace(
            "Scripting",
            Some(LayoutNode::Column(vec![
                (LayoutNode::tabs(&["outliner"]), 0.35),
                (LayoutNode::tabs(&["console", "properties"]), 0.65),
            ])),
            480.0,
            None,
            crate::bottom_dock::BOTTOM_DOCK_HEIGHT,
        ),
        workspace(
            "Signals",
            Some(LayoutNode::Column(vec![
                (LayoutNode::tabs(&["plot"]), 0.6),
                (LayoutNode::tabs(&["properties"]), 0.4),
            ])),
            380.0,
            Some(LayoutNode::tabs(&["nodes"])),
            340.0,
        ),
    ]
}

/// Whether `name` is a preset's (any case).
fn is_preset(name: &str) -> bool {
    PRESETS.iter().any(|p| p.eq_ignore_ascii_case(name.trim()))
}

/// The workspace called `name`: a preset, else a saved one.
fn lookup(workspaces: &Workspaces, name: &str) -> Option<Workspace> {
    presets()
        .into_iter()
        .find(|w| w.name.eq_ignore_ascii_case(name.trim()))
        .or_else(|| workspaces.find(name).cloned())
}

/// The panel names a dock can show, right dock first.
pub fn dock_editors() -> Vec<&'static str> {
    Pane::ALL
        .iter()
        .map(|p| p.name())
        .chain([BottomPane::Graph.name()])
        .collect()
}

/// Registers the workspace operations, their mirror, the request messages,
/// the resources and the systems that apply them (headless too).
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<LayoutSave>();
        registry.register::<LayoutApply>();
        registry.register::<WorkspaceSelect>();
        registry.register::<WorkspaceList>();
        registry.register::<WorkspaceDelete>();
        registry.register::<PanelSwap>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_workspace_mirror);
    app.add_message::<LayoutRequest>();
    app.add_message::<PanelSwapRequest>();
    app.init_resource::<Workspaces>();
    app.init_resource::<RightDock>();
    app.init_resource::<BottomDock>();
    app.add_systems(
        Update,
        (apply_layout_requests, sync_docks)
            .chain()
            .after(crate::panel_ops::apply_panel_requests),
    );
}

/// A workspace change an operation asked for, applied by
/// [`apply_layout_requests`].
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum LayoutRequest {
    /// Save the live docks under this name and make it active.
    Save(String),
    /// Re-apply the active workspace's saved layout.
    Apply,
    /// Make this workspace active and apply it.
    Select(String),
    /// Delete this saved workspace.
    Delete(String),
}

/// A dock tile switching its editor, applied by [`apply_layout_requests`].
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PanelSwapRequest {
    /// The editor the tile shows now (panel name).
    pub from: &'static str,
    /// The editor it should show (panel name).
    pub to: &'static str,
}

/// The workspace tabs as the operations see them: each name, whether it is
/// a preset, and the active one.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceMirror {
    /// Every workspace, presets first: `(name, preset)`.
    pub rows: Vec<(String, bool)>,
    /// The active workspace's name.
    pub active: String,
}

impl WorkspaceMirror {
    /// The mirror of `workspaces` (presets first, then the saved ones).
    pub fn of(workspaces: &Workspaces) -> Self {
        let rows = PRESETS
            .iter()
            .map(|p| ((*p).to_owned(), true))
            .chain(
                workspaces
                    .saved
                    .iter()
                    .filter(|w| !is_preset(&w.name))
                    .map(|w| (w.name.clone(), false)),
            )
            .collect();
        Self {
            rows,
            active: workspaces.active.clone(),
        }
    }

    /// The row for `name` (trimmed, any case), or an error that lists them.
    fn resolve(&self, name: &str) -> Result<&(String, bool), OpError> {
        self.rows
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| {
                let known: Vec<&str> = self.rows.iter().map(|(n, _)| n.as_str()).collect();
                OpError::Invalid(format!(
                    "no workspace named `{name}`; the workspaces are {}",
                    known.join(", ")
                ))
            })
    }
}

/// Publishes the workspace tabs into a new operation context.
fn fill_workspace_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(workspaces) = world.get_resource::<Workspaces>() {
        ctx.insert(WorkspaceMirror::of(workspaces));
    }
}

/// Captures the live docks as a workspace called `name`.
fn capture(name: &str, right: &RightDock, bottom: &BottomDock) -> Workspace {
    let right_node = right
        .tree
        .as_ref()
        .and_then(|t| dock_sync::layout_from_tree(t, |p: &Pane| p.name()));
    let bottom_node = bottom
        .tree
        .as_ref()
        .and_then(|t| dock_sync::layout_from_tree(t, |p: &BottomPane| p.name()));
    Workspace {
        name: name.to_owned(),
        right: DockLayout::from_node(right_node.as_ref(), right.size),
        bottom: DockLayout::from_node(bottom_node.as_ref(), bottom.size),
    }
}

/// Applies `workspace`: every dock editor it names opens and every other
/// one closes (floating windows and the status bar are left alone), each
/// dock's tree takes the saved shape, and each dock its saved size.
fn apply(
    workspace: &Workspace,
    right: &mut RightDock,
    bottom: &mut BottomDock,
    panels: &mut Panels,
) {
    let shown = workspace.panes();
    let editors = dock_editors();
    for (name, panel) in panels.named() {
        if editors.contains(&name) {
            panel.set_open(shown.iter().any(|s| s == name));
        }
    }
    right.tree = workspace
        .right
        .to_node()
        .and_then(|n| dock_sync::tree_from_layout(RIGHT_TREE, &n, Pane::from_name));
    bottom.tree = workspace
        .bottom
        .to_node()
        .and_then(|n| dock_sync::tree_from_layout(BOTTOM_TREE, &n, BottomPane::from_name));
    if workspace.right.size > 0.0 && workspace.right.size.is_finite() {
        right.size = workspace.right.size;
        right.pending_size = Some(workspace.right.size);
    }
    if workspace.bottom.size > 0.0 && workspace.bottom.size.is_finite() {
        bottom.size = workspace.bottom.size;
        bottom.pending_size = Some(workspace.bottom.size);
    }
}

/// Applies the workspace and swap requests the operations wrote. A name the
/// operation checked but that has gone since (a delete in the same frame)
/// warns and is dropped.
pub fn apply_layout_requests(
    mut requests: MessageReader<LayoutRequest>,
    mut swaps: MessageReader<PanelSwapRequest>,
    mut workspaces: ResMut<Workspaces>,
    mut right: ResMut<RightDock>,
    mut bottom: ResMut<BottomDock>,
    mut panels: Panels,
) {
    for request in requests.read() {
        match request {
            LayoutRequest::Save(name) => {
                let workspace = capture(name, &right, &bottom);
                workspaces.upsert(workspace);
                workspaces.active.clone_from(name);
            }
            LayoutRequest::Apply => {
                if let Some(w) = lookup(&workspaces, &workspaces.active) {
                    apply(&w, &mut right, &mut bottom, &mut panels);
                }
            }
            LayoutRequest::Select(name) => {
                if let Some(w) = lookup(&workspaces, name) {
                    apply(&w, &mut right, &mut bottom, &mut panels);
                    workspaces.active = w.name;
                } else {
                    warn!("no workspace named `{name}`");
                }
            }
            LayoutRequest::Delete(name) => {
                let was_active = workspaces.active.eq_ignore_ascii_case(name);
                if workspaces.remove(name) && was_active {
                    PRESETS[0].clone_into(&mut workspaces.active);
                }
            }
        }
    }
    for swap in swaps.read() {
        swap_editor(swap, &mut right, &mut panels);
    }
}

/// A tile switches from `swap.from` to `swap.to`. If `from` is not shown,
/// `to` simply opens; if `to` is already shown elsewhere, the two exchange
/// tiles. Visibility follows through [`PanelToggle`].
fn swap_editor(swap: &PanelSwapRequest, right: &mut RightDock, panels: &mut Panels) {
    let (Some(from), Some(to)) = (Pane::from_name(swap.from), Pane::from_name(swap.to)) else {
        // The bottom dock has one editor: switching it can only open it.
        set_open(panels, swap.to, true);
        return;
    };
    let swapped = right
        .tree
        .as_mut()
        .is_some_and(|tree| dock_sync::swap_pane(tree, &from, &to));
    let from_still_shown = swapped && right.pane_order().contains(&from);
    if swapped && !from_still_shown {
        set_open(panels, swap.from, false);
    }
    set_open(panels, swap.to, true);
}

fn set_open(panels: &mut Panels, name: &str, open: bool) {
    if let Some((_, panel)) = panels.named().into_iter().find(|(n, _)| *n == name) {
        panel.set_open(open);
    }
}

/// Keeps each dock's tiles in step with the open toggles, through
/// [`sync_panes`](dock_sync::sync_panes) (no rebuild). Runs in `Update`,
/// so the docks' trees exist headless too and the render systems only draw.
#[expect(clippy::too_many_arguments)] // one read per dock editor
pub fn sync_docks(
    outliner: Res<crate::outliner::ObjectTreePanel>,
    groups: Res<crate::groups::GroupsPanel>,
    depth: Res<crate::depth_panel::DepthPanel>,
    plot: Res<crate::plot::PlotPanel>,
    inspector: Res<crate::inspector::InspectorPanel>,
    console: Res<crate::console::ScriptConsole>,
    graph: Res<crate::node_graph::NodeGraph>,
    mut right: ResMut<RightDock>,
    mut bottom: ResMut<BottomDock>,
) {
    let open = |pane: Pane| -> bool {
        match pane {
            Pane::Tree => outliner.is_open(),
            Pane::Groups => groups.is_open(),
            Pane::Depth => depth.is_open(),
            Pane::Plot => plot.is_open(),
            Pane::Properties => inspector.is_open(),
            Pane::Console => console.is_open(),
        }
    };
    let desired: Vec<Pane> = Pane::ALL.into_iter().filter(|p| open(*p)).collect();
    // Only take the resource mutably when something changes.
    if right.pane_order().len() != desired.len()
        || desired.iter().any(|p| !right.pane_order().contains(p))
    {
        dock_sync::sync_panes(&mut right.tree, RIGHT_TREE, &desired);
    }
    let desired_bottom: Vec<BottomPane> = graph
        .is_open()
        .then_some(BottomPane::Graph)
        .into_iter()
        .collect();
    if bottom.pane_order() != desired_bottom {
        dock_sync::sync_panes(&mut bottom.tree, BOTTOM_TREE, &desired_bottom);
    }
}

/// The workspace tab row, drawn in the menu bar after the menus: one tab
/// per workspace (the active one selected, in the accent), a context menu
/// on a saved tab to delete it, and `+` to save the live docks as a new
/// workspace. Every click is an operation.
pub fn workspace_tabs(ui: &mut egui::Ui, workspaces: &Workspaces, invoke: &mut Vec<InvokeOp>) {
    ui.separator();
    let mirror = WorkspaceMirror::of(workspaces);
    for (name, preset) in &mirror.rows {
        let active = name.eq_ignore_ascii_case(&mirror.active);
        let tab = ui.selectable_label(active, name.as_str());
        crate::widget_registry::publish_target(&format!("menu:workspace:{name}"), tab.rect);
        if tab.clicked() {
            invoke.push(InvokeOp::with(
                "layout.workspace.select",
                json!({"name": name}),
            ));
        }
        if !preset {
            tab.context_menu(|ui| {
                if ui.button("Delete workspace").clicked() {
                    invoke.push(InvokeOp::with(
                        "layout.workspace.delete",
                        json!({"name": name}),
                    ));
                    ui.close();
                }
            });
        }
    }
    let draft_id = egui::Id::new("workspace-save-draft");
    let save = ui.menu_button("+", |ui| {
        ui.label("Save the docks as a workspace");
        let mut draft: String = ui.data_mut(|d| d.get_temp(draft_id).unwrap_or_default());
        let field = ui.add(
            egui::TextEdit::singleline(&mut draft)
                .hint_text("Name")
                .char_limit(MAX_NAME),
        );
        crate::widget_registry::publish_target("menu:workspace-name", field.rect);
        let name = draft.trim().to_owned();
        let valid = !name.is_empty() && !is_preset(&name);
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let button = ui.add_enabled(valid, egui::Button::new("Save"));
        crate::widget_registry::publish_target("menu:workspace-save-confirm", button.rect);
        if valid && (button.clicked() || enter) {
            invoke.push(InvokeOp::with("layout.save", json!({"name": name})));
            draft.clear();
            ui.close();
        }
        ui.data_mut(|d| d.insert_temp(draft_id, draft));
    });
    let save = save
        .response
        .on_hover_text("Save the docks as a new workspace");
    crate::widget_registry::publish_target("menu:workspace-save", save.rect);
}

/// Checks a name `layout.save` may take.
fn savable_name(name: &str) -> Result<String, OpError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::Invalid("a workspace needs a name".into()));
    }
    if name.chars().count() > MAX_NAME {
        return Err(OpError::Invalid(format!(
            "a workspace name has at most {MAX_NAME} characters"
        )));
    }
    if is_preset(name) {
        return Err(OpError::Invalid(format!(
            "`{name}` is a preset workspace; save under another name"
        )));
    }
    Ok(name.to_owned())
}

/// Save the docks as a workspace.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LayoutSave {
    /// The workspace's name (not a preset's); an existing saved one is replaced.
    pub name: String,
}

impl Operation for LayoutSave {
    const META: OpMeta = OpMeta {
        id: "layout.save",
        lisp: "layout-save",
        doc: "Save the docks' layout (editors, splits, sizes) as a workspace and make it active; a saved one of that name is replaced. Workstation config: saved with the scene, never undone.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "Mine"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        ctx.get::<WorkspaceMirror>()?;
        Ok(OpOutput::none().message(LayoutRequest::Save(savable_name(&self.name)?)))
    }
}

/// Re-apply the active workspace.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LayoutApply {}

impl Operation for LayoutApply {
    const META: OpMeta = OpMeta {
        id: "layout.apply",
        lisp: "layout-apply",
        doc: "Put the docks back to the active workspace's saved layout.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        ctx.get::<WorkspaceMirror>()?;
        Ok(OpOutput::none().message(LayoutRequest::Apply))
    }
}

/// Switch workspace.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSelect {
    /// The workspace's name, as `layout.workspace.list` gives it.
    pub name: String,
}

impl Operation for WorkspaceSelect {
    const META: OpMeta = OpMeta {
        id: "layout.workspace.select",
        lisp: "workspace-select",
        doc: "Switch to a workspace by name (a preset: Layout, Scripting, Signals; or a saved one): its tab becomes active and the docks take its layout.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "Scripting"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let (name, _) = ctx.get::<WorkspaceMirror>()?.resolve(&self.name)?;
        Ok(OpOutput::none().message(LayoutRequest::Select(name.clone())))
    }
}

/// Every workspace tab.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceList {}

impl Operation for WorkspaceList {
    const META: OpMeta = OpMeta {
        id: "layout.workspace.list",
        lisp: "workspaces",
        doc: "Every workspace in tab order: its name, whether it is a preset, and whether it is active.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mirror = ctx.get::<WorkspaceMirror>()?;
        let rows: Vec<Value> = mirror
            .rows
            .iter()
            .map(|(name, preset)| {
                json!({
                    "name": name,
                    "preset": preset,
                    "active": name.eq_ignore_ascii_case(&mirror.active),
                })
            })
            .collect();
        Ok(OpOutput::none().returning(Value::Array(rows)))
    }
}

/// Delete a saved workspace.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDelete {
    /// The saved workspace's name (a preset cannot be deleted).
    pub name: String,
}

impl Operation for WorkspaceDelete {
    const META: OpMeta = OpMeta {
        id: "layout.workspace.delete",
        lisp: "workspace-delete",
        doc: "Delete a saved workspace by name (presets stay). Deleting the active one makes Layout active and leaves the docks as they are.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "Mine"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let (name, preset) = ctx.get::<WorkspaceMirror>()?.resolve(&self.name)?;
        if *preset {
            return Err(OpError::Invalid(format!(
                "`{name}` is a preset workspace and cannot be deleted"
            )));
        }
        Ok(OpOutput::none().message(LayoutRequest::Delete(name.clone())))
    }
}

/// A dock tile switches its editor.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelSwap {
    /// The editor the tile shows now (a panel name).
    pub from: String,
    /// The editor it should show (a panel name that docks on the same side).
    pub to: String,
}

impl Operation for PanelSwap {
    const META: OpMeta = OpMeta {
        id: "panel.swap",
        lisp: "panel-swap",
        doc: "Switch the dock tile showing one editor to another, in place, as the tile's editor-type menu does. If `to` is shown elsewhere the two exchange tiles; if `from` is not shown, `to` just opens. Right dock: outliner, groups, depth, plot, properties, console; bottom: nodes.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"from": "outliner", "to": "console"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let editors = dock_editors();
        let resolve = |name: &str| -> Result<&'static str, OpError> {
            let key = name.trim().to_lowercase();
            editors.iter().find(|e| **e == key).copied().ok_or_else(|| {
                OpError::Invalid(format!(
                    "`{name}` is not a dock editor; the dock editors are {}",
                    editors.join(", ")
                ))
            })
        };
        let from = resolve(&self.from)?;
        let to = resolve(&self.to)?;
        if Pane::from_name(from).is_some() != Pane::from_name(to).is_some() {
            return Err(OpError::Invalid(format!(
                "`{from}` and `{to}` dock on different sides; a tile can only switch to an editor of its own dock"
            )));
        }
        Ok(OpOutput::none().message(PanelSwapRequest { from, to }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(workspaces: &Workspaces) -> OpContext {
        let mut ctx = OpContext::new(gradiance_op::OpIdSource::Random);
        ctx.insert(WorkspaceMirror::of(workspaces));
        ctx
    }

    #[test]
    fn the_presets_are_the_three_tabs_and_name_only_dock_editors() {
        let names: Vec<String> = presets().into_iter().map(|w| w.name).collect();
        assert_eq!(names, PRESETS);
        let editors = dock_editors();
        for w in presets() {
            for pane in w.panes() {
                assert!(editors.contains(&pane.as_str()), "{}: `{pane}`", w.name);
            }
        }
    }

    #[test]
    fn a_preset_name_cannot_be_saved_over_or_deleted() {
        let all = Workspaces::default();
        let save = LayoutSave {
            name: " layout ".into(),
        }
        .lower(&ctx(&all));
        assert!(matches!(save, Err(OpError::Invalid(_))));
        let delete = WorkspaceDelete {
            name: "Signals".into(),
        }
        .lower(&ctx(&all));
        assert!(matches!(delete, Err(OpError::Invalid(_))));
        let empty = LayoutSave { name: "  ".into() }.lower(&ctx(&all));
        assert!(matches!(empty, Err(OpError::Invalid(_))));
    }

    #[test]
    fn an_unknown_workspace_lists_the_known_ones() {
        let err = WorkspaceSelect {
            name: "nope".into(),
        }
        .lower(&ctx(&Workspaces::default()))
        .err()
        .map(|e| e.to_string());
        assert!(
            err.as_deref()
                .is_some_and(|e| e.contains("Layout, Scripting, Signals")),
            "{err:?}"
        );
    }

    #[test]
    fn a_swap_stays_on_one_side_and_names_dock_editors() {
        let all = Workspaces::default();
        let across = PanelSwap {
            from: "outliner".into(),
            to: "nodes".into(),
        }
        .lower(&ctx(&all));
        assert!(matches!(across, Err(OpError::Invalid(_))));
        let floating = PanelSwap {
            from: "outliner".into(),
            to: "settings".into(),
        }
        .lower(&ctx(&all));
        assert!(matches!(floating, Err(OpError::Invalid(_))));
        let ok = PanelSwap {
            from: " Outliner".into(),
            to: "CONSOLE".into(),
        }
        .lower(&ctx(&all));
        assert!(ok.is_ok());
    }
}

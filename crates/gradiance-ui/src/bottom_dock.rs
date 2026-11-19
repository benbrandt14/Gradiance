//! The bottom **dock**: an [`egui_tiles`] workspace hosting the Node Graph
//! canvas — the vvvv/Simulink-style block diagram — docked to the screen's
//! bottom edge (`docs/ui-shell-decision.md`), sibling to the right
//! [`dock`](crate::dock). It's a one-pane workspace today (the Live Plot
//! moved to the right dock next to Signals); keeping the `egui_tiles` container
//! lets future bottom panes (a timeline, a second canvas) dock and re-arrange
//! here without a rewrite.
//!
//! The pane stays a self-contained renderer over its own state
//! (`node_graph::node_graph_section`); the dock's
//! [`Behavior`](egui_tiles::Behavior) just routes it. The node-graph pane is
//! prepared (reconcile → viewer) before the tree renders and its edits are
//! drained after, so the config-seam mutation path is unchanged. It feeds its
//! rect to [`PanelRects`](crate::PanelRects) so input over it doesn't leak
//! to the scene.

use crate::node_graph::{self, GraphParams, GraphViewer, NodeGraph};
use crate::panels::PanelToggle;
use crate::signals::{self, SignalListView};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_signal::{SignalBindings, SignalBus};

/// A dockable pane of the bottom workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomPane {
    /// The Simulink-style node-graph canvas.
    Graph,
}

impl BottomPane {
    /// The pane's panel name (see [`crate::dock::Pane::name`]).
    pub fn name(self) -> &'static str {
        match self {
            Self::Graph => "nodes",
        }
    }

    /// The pane a panel name shows in the bottom dock, if it docks there.
    pub fn from_name(name: &str) -> Option<Self> {
        (name == "nodes").then_some(Self::Graph)
    }

    fn title(self) -> &'static str {
        match self {
            Self::Graph => "Node Graph",
        }
    }
}

/// The bottom dock's `egui_tiles` layout. [`sync_panes`](crate::dock_sync::sync_panes)
/// keeps its tiles in step with the open set without rebuilding, so a user's
/// arrangement survives. Editor view-state; persisted only as a saved
/// workspace ([`crate::workspaces`]).
#[derive(Resource)]
pub struct BottomDock {
    pub(crate) tree: Option<egui_tiles::Tree<BottomPane>>,
    /// The dock's height when last drawn, in points.
    pub(crate) size: f32,
    /// A height a workspace asked for, applied on the next draw.
    pub(crate) pending_size: Option<f32>,
}

impl Default for BottomDock {
    fn default() -> Self {
        Self {
            tree: None,
            size: BOTTOM_DOCK_HEIGHT,
            pending_size: None,
        }
    }
}

impl BottomDock {
    /// The panes in depth-first tile order (empty when the dock is closed).
    pub fn pane_order(&self) -> Vec<BottomPane> {
        self.tree
            .as_ref()
            .map_or_default(crate::dock_sync::pane_order)
    }
}

/// The bottom dock's default height, in points.
pub const BOTTOM_DOCK_HEIGHT: f32 = 260.0;

/// Routes each `egui_tiles` pane to its renderer. The node-graph viewer is
/// `Some` exactly when the Graph pane is present.
struct BottomBehavior<'a, 'ws> {
    graph: &'a mut NodeGraph,
    viewer: Option<&'a mut GraphViewer>,
    /// The signal list beside the canvas, and everything it edits.
    list: &'a mut SignalList<'a, 'ws>,
    /// Set when a tab's ✕ is pressed; drained by [`bottom_dock`] into the
    /// pane's open toggle (see [`crate::dock`] for why it can't be immediate).
    closed: &'a mut Option<BottomPane>,
}

impl egui_tiles::Behavior<BottomPane> for BottomBehavior<'_, '_> {
    fn tab_title_for_pane(&mut self, pane: &BottomPane) -> egui::WidgetText {
        pane.title().into()
    }

    fn is_tab_closable(
        &self,
        _tiles: &egui_tiles::Tiles<BottomPane>,
        _tile_id: egui_tiles::TileId,
    ) -> bool {
        true
    }

    fn on_tab_close(
        &mut self,
        tiles: &mut egui_tiles::Tiles<BottomPane>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(egui_tiles::Tile::Pane(pane)) = tiles.get(tile_id) {
            *self.closed = Some(*pane);
        }
        true
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut BottomPane,
    ) -> egui_tiles::UiResponse {
        match pane {
            BottomPane::Graph => {
                // The canvas is the surface; the list is the same graph as a
                // form — names, compile errors, and the edits the canvas has no
                // gesture for (rename a binding, retarget a sink, delete a
                // param). It sits beside the canvas rather than in a pane of
                // its own so the two views of one model stay together.
                let list = &mut *self.list;
                egui::Panel::right("signal-list")
                    .resizable(true)
                    .default_size(260.0)
                    .show(ui, |ui| {
                        signals::signals_section(
                            ui,
                            &mut list.view,
                            list.bindings,
                            &list.selected,
                            list.edits,
                        );
                    });
                if let Some(viewer) = self.viewer.as_deref_mut() {
                    node_graph::node_graph_section(ui, self.graph, viewer);
                }
            }
        }
        egui_tiles::UiResponse::None
    }
}

/// What the signal list needs beyond what [`GraphParams`] already holds.
///
/// The canvas host owns `SignalParams`, `ComputedSignals` and `SignalBindings`
/// for its own reconcile, so the list borrows those from it rather than
/// requesting them again — two `ResMut`s of one resource in one system is a
/// panic at schedule build, which `tests/it/ui_conflicts.rs` guards.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SignalListParams<'w, 's> {
    compiled: Res<'w, gradiance_signal::CompiledSignals>,
    edits: MessageWriter<'w, gradiance_command::intent::PropertyEditIntent>,
    ids: Query<'w, 's, &'static StableId, With<Body>>,
    nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static StableId,
            &'static gradiance_domain::node::NodeKind,
        ),
        With<gradiance_domain::node::BehaviorNode>,
    >,
}

/// The list's borrowed state for one frame, assembled by [`bottom_dock`].
struct SignalList<'a, 'w> {
    view: SignalListView<'a>,
    bindings: &'a mut SignalBindings,
    edits: &'a mut MessageWriter<'w, gradiance_command::intent::PropertyEditIntent>,
    selected: Vec<StableId>,
}

/// Renders the bottom dock when the Node Graph is open, as an `egui_tiles` tab
/// workspace. The graph toggles from the View menu / context menu.
pub fn bottom_dock(
    mut contexts: EguiContexts,
    mut gp: GraphParams,
    bus: Res<SignalBus>,
    mut list: SignalListParams,
    mut panels: ResMut<crate::PanelRects>,
    mut dock: ResMut<BottomDock>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Which panes are present was settled in `Update` by
    // `workspaces::sync_docks`.
    let pending_size = dock.pending_size.take();
    let Some(tree) = dock.tree.as_mut() else {
        return Ok(());
    };

    // Prepare the node-graph pane (reconcile + per-pin readouts) before render.
    let mut viewer = node_graph::prepare(&mut gp, &bus);
    let mut closed: Option<BottomPane> = None;

    // Scope the behavior so its partial borrow of `gp` (the graph pane) ends
    // before we drain the pane's edits back into `gp` below.
    let selected: Vec<StableId> = gp
        .selection
        .iter()
        .filter_map(|e| list.ids.get(e).ok().copied())
        .collect();
    // The behavior node to edit, resolved here so the renderer holds no query
    // (the same prepare/render/apply shape the outliner uses).
    let node = gp
        .selection
        .iter()
        .find_map(|e| list.nodes.get(e).ok().map(|(_, id, k)| (*id, k.clone())));
    let panel_rect = {
        let mut signal_list = SignalList {
            view: SignalListView {
                params: &mut gp.params.0,
                computed: &mut gp.computed.0,
                bus: &bus,
                compiled: &list.compiled,
                node,
            },
            bindings: &mut gp.bindings,
            edits: &mut list.edits,
            selected,
        };
        let mut behavior = BottomBehavior {
            graph: &mut gp.graph,
            viewer: Some(&mut viewer),
            closed: &mut closed,
            list: &mut signal_list,
        };
        // egui 0.36 panels dock inside a `Ui`; build the screen-root one (the
        // same background-layer pattern the right dock uses, so it claims the
        // edge). Stop at the right dock's left edge so the two don't overlap in
        // the bottom-right corner (the right dock pushed its rect first).
        let viewport = ctx.viewport_rect();
        let right = panels.right_inset(viewport);
        // …and above the status bar, which pushed its rect before both docks.
        let floor = panels.bottom_inset(viewport);
        let area = egui::Rect::from_min_max(viewport.min, egui::pos2(right, floor));
        let mut root = egui::Ui::new(
            ctx.clone(),
            "bottom-dock".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(area),
        );
        let mut dock_panel = egui::Panel::bottom("bottom-dock-panel")
            .resizable(true)
            .default_size(BOTTOM_DOCK_HEIGHT);
        // A workspace's height holds for one frame; egui keeps it after.
        if let Some(size) = pending_size {
            dock_panel = dock_panel.exact_size(size);
        }
        dock_panel
            .show(&mut root, |ui| {
                tree.ui(&mut behavior, ui);
            })
            .response
            .rect
    };
    // Claim the dock's rect so input over it doesn't leak to the scene.
    panels.push(panel_rect);
    crate::widget_registry::publish_rect("bottom_dock", panel_rect);
    dock.size = panel_rect.height();

    // Drain the node-graph pane's edits into the config-seam resources.
    node_graph::apply_pane(viewer, &mut gp);
    // A closed tab turns the pane's toggle off, so it agrees with the View menu.
    if closed == Some(BottomPane::Graph) {
        gp.graph.set_open(false);
    }
    Ok(())
}

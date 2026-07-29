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
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_signal::SignalBus;

/// A dockable pane of the bottom workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomPane {
    /// The Simulink-style node-graph canvas.
    Graph,
}

impl BottomPane {
    fn title(self) -> &'static str {
        match self {
            Self::Graph => "Node Graph",
        }
    }
}

/// The bottom dock's persisted `egui_tiles` layout plus the open-set it was
/// built for (so we only rebuild — losing the user's arrangement — when the
/// visible panes actually change). Editor view-state; never persisted.
#[derive(Resource, Default)]
pub struct BottomDock {
    tree: Option<egui_tiles::Tree<BottomPane>>,
    shown: Vec<BottomPane>,
}

/// Routes each `egui_tiles` pane to its renderer. The node-graph viewer is
/// `Some` exactly when the Graph pane is present.
struct BottomBehavior<'a> {
    graph: &'a mut NodeGraph,
    viewer: Option<&'a mut GraphViewer>,
}

impl egui_tiles::Behavior<BottomPane> for BottomBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &BottomPane) -> egui::WidgetText {
        pane.title().into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut BottomPane,
    ) -> egui_tiles::UiResponse {
        match pane {
            BottomPane::Graph => {
                if let Some(viewer) = self.viewer.as_deref_mut() {
                    node_graph::node_graph_section(ui, self.graph, viewer);
                }
            }
        }
        egui_tiles::UiResponse::None
    }
}

/// Renders the bottom dock when the Node Graph is open, as an `egui_tiles` tab
/// workspace. The graph toggles from the toolbar / context menu.
pub fn bottom_dock(
    mut contexts: EguiContexts,
    mut gp: GraphParams,
    bus: Res<SignalBus>,
    mut panels: ResMut<crate::PanelRects>,
    mut dock: ResMut<BottomDock>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Which panes are visible, in a stable order. Rebuild the tree only when
    // this set changes, so a user's tab arrangement persists between frames.
    let desired: Vec<BottomPane> = [gp.graph.is_open().then_some(BottomPane::Graph)]
        .into_iter()
        .flatten()
        .collect();
    if desired.is_empty() {
        dock.tree = None;
        dock.shown.clear();
        return Ok(());
    }
    if dock.shown != desired {
        dock.tree = Some(egui_tiles::Tree::new_tabs(
            "bottom-dock-tiles",
            desired.clone(),
        ));
        dock.shown = desired;
    }
    let Some(tree) = dock.tree.as_mut() else {
        return Ok(());
    };

    // Prepare the node-graph pane (reconcile + per-pin readouts) before render.
    let mut viewer = node_graph::prepare(&mut gp, &bus);

    // Scope the behavior so its partial borrow of `gp` (the graph pane) ends
    // before we drain the pane's edits back into `gp` below.
    let panel_rect = {
        let mut behavior = BottomBehavior {
            graph: &mut gp.graph,
            viewer: Some(&mut viewer),
        };
        // egui 0.35 panels dock inside a `Ui`; build the screen-root one (the
        // same background-layer pattern the right dock uses, so it claims the
        // edge). Stop at the right dock's left edge so the two don't overlap in
        // the bottom-right corner (the right dock pushed its rect first).
        let viewport = ctx.viewport_rect();
        let right = panels.right_inset(viewport);
        let area = egui::Rect::from_min_max(viewport.min, egui::pos2(right, viewport.max.y));
        let mut root = egui::Ui::new(
            ctx.clone(),
            "bottom-dock".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(area),
        );
        egui::Panel::bottom("bottom-dock-panel")
            .resizable(true)
            .default_size(260.0)
            .show(&mut root, |ui| {
                tree.ui(&mut behavior, ui);
            })
            .response
            .rect
    };
    // Claim the dock's rect so input over it doesn't leak to the scene.
    panels.push(panel_rect);

    // Drain the node-graph pane's edits into the config-seam resources.
    node_graph::apply_pane(viewer, &mut gp);
    Ok(())
}

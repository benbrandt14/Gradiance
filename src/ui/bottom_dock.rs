//! The bottom **dock**: an [`egui_tiles`] workspace hosting the Node Graph and
//! the Live Plot as re-arrangeable tabs — the second `egui_tiles` surface of
//! the desktop-app shell (`docs/ui-shell-decision.md`), sibling to the right
//! [`dock`](crate::ui::dock).
//!
//! Both panes stay self-contained renderers over their own state
//! (`node_graph::node_graph_section`, `plot::plot_section`); the dock's
//! [`Behavior`](egui_tiles::Behavior) just routes each pane to its renderer. The
//! node-graph pane is prepared (reconcile → viewer) before the tree renders and
//! its edits are drained after, so the config-seam mutation path is unchanged.
//! Which panes exist tracks the open toggles: the tree is rebuilt only when that
//! open-set changes, so a user's tab layout survives between frames. It docks the
//! screen's bottom edge on the background layer and feeds its rect to
//! [`PanelRects`](crate::ui::PanelRects).

use crate::signal::SignalBus;
use crate::ui::node_graph::{self, GraphParams, GraphViewer, NodeGraph};
use crate::ui::plot::{self, PlotConfig, PlotPanel};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::VecDeque;

/// A dockable pane of the bottom workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomPane {
    /// The Simulink-style node-graph canvas.
    Graph,
    /// The live time-series plotter.
    Plot,
}

impl BottomPane {
    fn title(self) -> &'static str {
        match self {
            Self::Graph => "⬡ Node Graph",
            Self::Plot => "Live Plot",
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

/// Routes each `egui_tiles` pane to its renderer, holding the state each needs
/// for this frame. The node-graph viewer is `Some` exactly when the Graph pane
/// is present.
struct BottomBehavior<'a> {
    graph: &'a mut NodeGraph,
    viewer: Option<&'a mut GraphViewer>,
    plottable: &'a [(&'a str, &'a VecDeque<f32>)],
    plot_config: &'a mut PlotConfig,
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
            BottomPane::Plot => {
                plot::plot_section(ui, self.plottable, self.plot_config);
            }
        }
        egui_tiles::UiResponse::None
    }
}

/// Renders the bottom dock when the Node Graph or the Live Plot is open, as an
/// `egui_tiles` tab workspace. Backslash toggles the plot (unless something is
/// capturing keyboard input); the graph toggles from the toolbar / context menu.
pub fn bottom_dock(
    mut contexts: EguiContexts,
    mut gp: GraphParams,
    bus: Res<SignalBus>,
    mut plot: ResMut<PlotPanel>,
    mut plot_config: ResMut<PlotConfig>,
    keys: Res<ButtonInput<KeyCode>>,
    mut panels: ResMut<crate::ui::PanelRects>,
    mut dock: ResMut<BottomDock>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Toggle the plot with `\` — but not while typing (so the key reaches
    // editors), matching the old standalone plot panel.
    if keys.just_pressed(KeyCode::Backslash) && !ctx.egui_wants_keyboard_input() {
        plot.toggle();
    }

    // Which panes are visible, in a stable order. Rebuild the tree only when
    // this set changes, so a user's tab arrangement persists between frames.
    let desired: Vec<BottomPane> = [
        gp.graph.is_open().then_some(BottomPane::Graph),
        plot.is_open().then_some(BottomPane::Plot),
    ]
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

    // Prepare the node-graph pane (reconcile + per-pin readouts) before render,
    // and the plot's series list — both self-contained once built.
    let graph_open = gp.graph.is_open();
    let mut viewer = graph_open.then(|| node_graph::prepare(&mut gp, &bus));
    let plottable = plot::plottable_series(&bus);

    // Scope the behavior so its partial borrow of `gp` (the graph pane) ends
    // before we drain the pane's edits back into `gp` below.
    let panel_rect = {
        let mut behavior = BottomBehavior {
            graph: &mut gp.graph,
            viewer: viewer.as_mut(),
            plottable: &plottable,
            plot_config: &mut plot_config,
        };
        // egui 0.35 panels dock inside a `Ui`; build the screen-root one (the
        // same background-layer pattern the right dock uses, so it claims the
        // edge).
        let mut root = egui::Ui::new(
            ctx.clone(),
            "bottom-dock".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(ctx.viewport_rect()),
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
    if let Some(viewer) = viewer {
        node_graph::apply_pane(viewer, &mut gp);
    }
    Ok(())
}

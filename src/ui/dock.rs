//! The right **dock**: an [`egui_tiles`] workspace hosting the Depth, Signals,
//! and Script-console sections as re-arrangeable tabs — the first
//! `egui_tiles` surface of the desktop-app shell (`docs/ui-shell-decision.md`).
//!
//! The sections (`depth_panel::depth_section`, `signals::signals_section`,
//! `console::console_section`) stay self-contained renderers over their own
//! state; the dock's [`Behavior`](egui_tiles::Behavior) just routes each pane
//! to its renderer. Which panes exist tracks the open toggles: the tree is
//! rebuilt only when that open-set changes, so a user's tab layout survives
//! between frames. It docks the screen's right edge on the background layer and
//! feeds its rect to [`PanelRects`](crate::ui::PanelRects).

use crate::command::intent::PropertyEditIntent;
use crate::core::ids::StableId;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::depth::DepthBand;
use crate::interaction::selection::Selection;
use crate::script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use crate::signal::SignalBus;
use crate::ui::console::{self, ScriptConsole};
use crate::ui::depth_panel::{self, DepthPanel};
use crate::ui::outliner::{self, OutlinerClick, OutlinerModel, OutlinerParams};
use crate::ui::plot::{self, PlotConfig, PlotPanel};
use crate::ui::signals::{self, SignalsDock, SignalsPanel};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::VecDeque;

/// A dockable section of the right workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The object tree (outliner).
    Tree,
    /// Depth-band editor for the selection.
    Depth,
    /// Signal bindings / params / computed.
    Signals,
    /// The live time-series plotter (drawn from the signal bus).
    Plot,
    /// The scripting REPL.
    Console,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Self::Tree => "Outliner",
            Self::Depth => "Depth",
            Self::Signals => "Signals",
            Self::Plot => "Live Plot",
            Self::Console => "Script",
        }
    }
}

/// The scripting-console resources, bundled so the dock host stays under Bevy's
/// system-parameter cap.
#[derive(SystemParam)]
pub struct ConsoleParams<'w> {
    pub(crate) console: ResMut<'w, ScriptConsole>,
    pub(crate) inputs: ResMut<'w, ScriptInputs>,
    pub(crate) registry: Res<'w, OperationRegistry>,
    pub(crate) log: Res<'w, ScriptLog>,
}

/// The plotter's resources, bundled likewise. `bus` is the single signal
/// history the plot draws from.
#[derive(SystemParam)]
pub struct PlotParams<'w> {
    pub(crate) panel: ResMut<'w, PlotPanel>,
    pub(crate) config: ResMut<'w, PlotConfig>,
    pub(crate) bus: Res<'w, SignalBus>,
}

/// The right dock's persisted `egui_tiles` layout plus the open-set it was
/// built for (so we only rebuild — losing the user's arrangement — when the
/// visible sections actually change). Editor view-state; never persisted.
#[derive(Resource, Default)]
pub struct RightDock {
    tree: Option<egui_tiles::Tree<Pane>>,
    shown: Vec<Pane>,
}

/// Routes each `egui_tiles` pane to its section renderer, holding the state the
/// renderers need for this frame. The writer and the signals bundle carry
/// independent system lifetimes, so each gets its own.
struct DockBehavior<'a, 'we, 'ws, 'ss> {
    depth: &'a mut DepthPanel,
    rows: &'a [(StableId, DepthBand, egui::Color32)],
    edits: &'a mut MessageWriter<'we, PropertyEditIntent>,
    signals: &'a mut SignalsDock<'ws, 'ss>,
    selected: &'a [StableId],
    selection: &'a Selection,
    outliner: &'a OutlinerModel,
    outliner_click: &'a mut Option<OutlinerClick>,
    plottable: &'a [(&'a str, &'a VecDeque<f32>)],
    plot_config: &'a mut PlotConfig,
    console: &'a mut ScriptConsole,
    inputs: &'a mut ScriptInputs,
    registry: &'a OperationRegistry,
    log: &'a ScriptLog,
}

impl egui_tiles::Behavior<Pane> for DockBehavior<'_, '_, '_, '_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.title().into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let height = ui.available_height();
        match pane {
            Pane::Tree => {
                outliner::outliner_section(ui, self.outliner, self.outliner_click);
            }
            Pane::Depth => {
                depth_panel::depth_section(ui, self.depth, self.rows, self.edits, height);
            }
            Pane::Signals => {
                signals::signals_section(
                    ui,
                    self.signals,
                    self.selected,
                    self.selection,
                    self.edits,
                );
            }
            Pane::Plot => {
                plot::plot_section(ui, self.plottable, self.plot_config);
            }
            Pane::Console => {
                console::console_section(ui, self.console, self.inputs, self.registry, self.log);
            }
        }
        egui_tiles::UiResponse::None
    }
}

/// The depth section's rows: each selected body projected to `(id, band,
/// fill-color)` bars. Split out so [`right_dock`] stays within the line lint.
fn depth_rows(
    selection: &Selection,
    ids: &Query<&StableId, With<Body>>,
    bands: &Query<&DepthBand, With<Body>>,
    appearances: &Query<&Appearance, With<Body>>,
) -> Vec<(StableId, DepthBand, egui::Color32)> {
    selection
        .iter()
        .filter_map(|e| {
            let id = *ids.get(e).ok()?;
            let band = bands.get(e).ok()?.sanitized();
            let c = appearances
                .get(e)
                .map_or(crate::domain::appearance::Rgba::rgb(1.0, 1.0, 1.0), |a| {
                    a.fill
                });
            let color = egui::Color32::from_rgb(
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
            );
            Some((id, band, color))
        })
        .collect()
}

/// Renders the dock when any section is open, as an `egui_tiles` tab workspace.
/// The backquote key toggles the console and `\` the plot (unless something is
/// capturing keyboard input).
#[expect(clippy::too_many_arguments)] // one dock host, grouped section reads
pub fn right_dock(
    mut contexts: EguiContexts,
    mut panel: ResMut<DepthPanel>,
    signals_panel: Res<SignalsPanel>,
    mut signals: SignalsDock,
    mut selection: ResMut<Selection>,
    ids: Query<&StableId, With<Body>>,
    bands: Query<&DepthBand, With<Body>>,
    appearances: Query<&Appearance, With<Body>>,
    mut edits: MessageWriter<PropertyEditIntent>,
    mut plot: PlotParams,
    mut console: ConsoleParams,
    mut op: OutlinerParams,
    keys: Res<ButtonInput<KeyCode>>,
    mut panels: ResMut<crate::ui::PanelRects>,
    mut dock: ResMut<RightDock>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Panel shortcuts — but not while typing (so keys reach editors): `` ` ``
    // toggles the console, `\` toggles the plot.
    if !ctx.egui_wants_keyboard_input() {
        if keys.just_pressed(KeyCode::Backquote) {
            console.console.toggle();
        }
        if keys.just_pressed(KeyCode::Backslash) {
            plot.panel.toggle();
        }
    }

    // Which panes are visible, in a stable order. Rebuild the tree only when
    // this set changes, so a user's tab arrangement persists between frames.
    let desired: Vec<Pane> = [
        op.panel.is_open().then_some(Pane::Tree),
        panel.open.then_some(Pane::Depth),
        signals_panel.is_open().then_some(Pane::Signals),
        plot.panel.is_open().then_some(Pane::Plot),
        console.console.is_open().then_some(Pane::Console),
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
            "right-dock-tiles",
            desired.clone(),
        ));
        dock.shown = desired;
    }
    let Some(tree) = dock.tree.as_mut() else {
        return Ok(());
    };

    // Rows for the depth section: the selection projected to bars.
    let rows = depth_rows(&selection, &ids, &bands, &appearances);
    // Selected body ids (for the signal binding add-buttons).
    let selected: Vec<StableId> = selection
        .iter()
        .filter_map(|e| ids.get(e).ok().copied())
        .collect();
    // The plot pane's series list, computed from the bus.
    let plottable = plot::plottable_series(&plot.bus);
    // The outliner snapshot (empty when the pane is closed).
    let outliner_model = if op.panel.is_open() {
        outliner::build_model(&op, &selection)
    } else {
        OutlinerModel::default()
    };
    let mut outliner_click: Option<OutlinerClick> = None;

    // Scope the behavior so its borrows of the section state end before we
    // drain the outliner click back into the selection below.
    let panel_rect = {
        let mut behavior = DockBehavior {
            depth: &mut panel,
            rows: &rows,
            edits: &mut edits,
            signals: &mut signals,
            selected: &selected,
            selection: &selection,
            outliner: &outliner_model,
            outliner_click: &mut outliner_click,
            plottable: &plottable,
            plot_config: &mut plot.config,
            console: &mut console.console,
            inputs: &mut console.inputs,
            registry: &console.registry,
            log: &console.log,
        };

        // egui 0.35 panels dock inside a `Ui`; build the screen-root one.
        let mut root = egui::Ui::new(
            ctx.clone(),
            "right-dock".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(ctx.viewport_rect()),
        );
        egui::Panel::right("right-dock-panel")
            .resizable(true)
            .default_size(240.0)
            .show(&mut root, |ui| {
                tree.ui(&mut behavior, ui);
            })
            .response
            .rect
    };
    // Claim the dock's rect so input over it doesn't leak to the scene.
    panels.push(panel_rect);

    // Universal selection: a tree-row click drives the same Selection /
    // SelectedJoint the viewport and node graph use, via the sanctioned seam.
    if let Some(click) = outliner_click {
        outliner::apply_click(click, &mut selection, &mut op);
    }
    Ok(())
}

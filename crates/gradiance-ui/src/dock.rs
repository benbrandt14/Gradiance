//! The right **dock**: an [`egui_tiles`] workspace hosting the Outliner, Depth,
//! Signals+Plot, Properties, and Script-console sections as re-arrangeable tabs
//! — the first `egui_tiles` surface of the desktop-app shell
//! (`docs/ui-shell-decision.md`).
//!
//! The sections stay self-contained renderers over their own state; the dock's
//! [`Behavior`](egui_tiles::Behavior) just routes each pane to its renderer. The
//! Properties pane hosts the body inspector, and its [`BodyProps`] bundle is
//! also where the dock keeps its single `PropertyEditIntent` writer and
//! `SignalBindings` (the Depth and Signals sections edit through them), so the
//! one dock system holds exactly one of each. Which panes exist tracks the open
//! toggles, kept in step by [`sync_panes`](crate::dock_sync::sync_panes), which
//! adds and removes individual tiles so a user's splits and tab order survive an
//! unrelated section being toggled. Each tab's ✕ turns its section's toggle off,
//! so the dock and the View menu never disagree. It docks the screen's right
//! edge on the background layer and feeds its rect to
//! [`PanelRects`](crate::PanelRects).

use crate::console::{self, ScriptConsole};
use crate::depth_panel::{self, DepthPanel};
use crate::inspector::{self, BodyProps, InspectorPanel};
use crate::outliner::{self, OutlinerClick, OutlinerModel, OutlinerParams};
use crate::panels::PanelToggle;
use crate::plot::{self, PlotConfig, PlotPanel};
use crate::signals::{self, SignalsDock, SignalsPanel};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_interaction::selection::Selection;
use gradiance_script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use gradiance_signal::SignalBus;
use std::collections::VecDeque;

/// A dockable section of the right workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The object tree (outliner).
    Tree,
    /// Depth-band editor for the selection.
    Depth,
    /// Signal bindings / params / computed **and** the live plotter — one
    /// merged pane (the signal dataflow and its readout live together).
    Signals,
    /// The property inspector for the current selection.
    Properties,
    /// The scripting REPL.
    Console,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Self::Tree => "Outliner",
            Self::Depth => "Depth",
            Self::Signals => "Signals + Plot",
            Self::Properties => "Properties",
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

/// The right dock's `egui_tiles` layout. [`sync_panes`](crate::dock_sync::sync_panes)
/// keeps its tiles in step with the open set by adding and removing individual
/// tiles, so splits and tab order survive. Editor view-state; never persisted.
#[derive(Resource, Default)]
pub struct RightDock {
    tree: Option<egui_tiles::Tree<Pane>>,
}

/// Routes each `egui_tiles` pane to its section renderer, holding the state the
/// renderers need for this frame. The writer and the signals bundle carry
/// independent system lifetimes, so each gets its own.
struct DockBehavior<'a, 'wp, 'sp, 'ws, 'ss, 'wo> {
    depth: &'a mut DepthPanel,
    rows: &'a [(StableId, DepthBand, egui::Color32)],
    /// The property inspector's read/write bundle — also the single
    /// `PropertyEditIntent` writer and `SignalBindings` the Depth and Signals
    /// sections edit through (so the dock host holds exactly one of each).
    props: &'a mut BodyProps<'wp, 'sp>,
    optimizer: &'a mut crate::optimizer::OptimizerPanel<'wo>,
    signals: &'a mut SignalsDock<'ws, 'ss>,
    selected: &'a [StableId],
    selection: &'a Selection,
    outliner: &'a OutlinerModel,
    outliner_click: &'a mut Option<OutlinerClick>,
    show_signals: bool,
    show_plot: bool,
    plottable: &'a [(&'a str, &'static str, &'a VecDeque<f32>)],
    plot_config: &'a mut PlotConfig,
    console: &'a mut ScriptConsole,
    inputs: &'a mut ScriptInputs,
    registry: &'a OperationRegistry,
    log: &'a ScriptLog,
    /// Set by [`Behavior::on_tab_close`](egui_tiles::Behavior::on_tab_close) —
    /// drained after the tree renders, because closing a tab has to turn the
    /// section's *open toggle* off, and those resources are borrowed by the
    /// caller for the duration of this borrow.
    closed: &'a mut Option<Pane>,
}

impl egui_tiles::Behavior<Pane> for DockBehavior<'_, '_, '_, '_, '_, '_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.title().into()
    }

    /// Every section is closable — the ✕ on the tab is the direct counterpart
    /// of its View-menu checkbox.
    fn is_tab_closable(
        &self,
        _tiles: &egui_tiles::Tiles<Pane>,
        _tile_id: egui_tiles::TileId,
    ) -> bool {
        true
    }

    /// Record the close and let `egui_tiles` drop the tile; [`right_dock`]
    /// turns the matching toggle off afterwards, so [`sync_panes`] does not
    /// simply put the pane back next frame.
    fn on_tab_close(
        &mut self,
        tiles: &mut egui_tiles::Tiles<Pane>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(egui_tiles::Tile::Pane(pane)) = tiles.get(tile_id) {
            *self.closed = Some(*pane);
        }
        true
    }

    /// Keep every pane inside a `Tabs` container even when it is the only one,
    /// so a lone section still shows a tab bar (and therefore its close button),
    /// and so [`sync_panes`] always has a container to graft new panes onto.
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
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let height = ui.available_height();
        match pane {
            // Both of these overflow their pane routinely — a large scene's
            // outliner and a body's full property sheet are taller than any
            // dock height — so they scroll rather than clip.
            Pane::Tree => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        outliner::outliner_section(ui, self.outliner, self.outliner_click);
                    });
            }
            Pane::Depth => {
                depth_panel::depth_section(
                    ui,
                    self.depth,
                    self.rows,
                    &mut self.props.edits,
                    height,
                );
            }
            Pane::Signals => {
                // One merged pane: the bindings/params/computed above, the live
                // plotter below. Each half honors its own open toggle.
                if self.show_signals {
                    signals::signals_section(
                        ui,
                        self.signals,
                        &mut self.props.bindings,
                        self.selected,
                        self.selection,
                        &mut self.props.edits,
                    );
                }
                if self.show_plot {
                    if self.show_signals {
                        ui.separator();
                    }
                    plot::plot_section(ui, self.plottable, self.plot_config);
                }
            }
            Pane::Properties => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        inspector::inspector_pane(ui, self.selection, self.props, self.optimizer);
                    });
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
            let c = appearances.get(e).map_or(
                gradiance_domain::appearance::Rgba::rgb(1.0, 1.0, 1.0),
                |a| a.fill,
            );
            let color = egui::Color32::from_rgb(
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
            );
            Some((id, band, color))
        })
        .collect()
}

/// The Properties pane's own parameters: whether it is open, and the
/// optimizer it hosts. Bundled to keep [`right_dock`] under Bevy's
/// system-parameter count limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PropertiesParams<'w> {
    panel: ResMut<'w, InspectorPanel>,
    optimizer: crate::optimizer::OptimizerPanel<'w>,
}

/// Renders the dock when any section is open, as an `egui_tiles` tab workspace.
/// The backquote key toggles the console and `\` the plot (unless something is
/// capturing keyboard input).
#[expect(clippy::too_many_arguments)] // one dock host, grouped section reads
pub fn right_dock(
    mut contexts: EguiContexts,
    mut panel: ResMut<DepthPanel>,
    mut signals_panel: ResMut<SignalsPanel>,
    mut signals: SignalsDock,
    mut selection: ResMut<Selection>,
    ids: Query<&StableId, With<Body>>,
    bands: Query<&DepthBand, With<Body>>,
    appearances: Query<&Appearance, With<Body>>,
    mut props: BodyProps,
    mut properties: PropertiesParams,
    mut plot: PlotParams,
    mut console: ConsoleParams,
    mut op: OutlinerParams,
    keys: Res<ButtonInput<KeyCode>>,
    mut panels: ResMut<crate::PanelRects>,
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

    // Which panes are visible, in a stable order. `sync_panes` adds and removes
    // individual tiles, so a user's splits and tab order survive an unrelated
    // section being toggled.
    let desired: Vec<Pane> = [
        op.panel.is_open().then_some(Pane::Tree),
        panel.open.then_some(Pane::Depth),
        (signals_panel.is_open() || plot.panel.is_open()).then_some(Pane::Signals),
        properties.panel.open.then_some(Pane::Properties),
        console.console.is_open().then_some(Pane::Console),
    ]
    .into_iter()
    .flatten()
    .collect();
    crate::dock_sync::sync_panes(&mut dock.tree, "right-dock-tiles", &desired);
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
    let plottable = plot::plottable_series(&plot.bus, &props.bindings);
    // The outliner snapshot (empty when the pane is closed).
    let outliner_model = if op.panel.is_open() {
        outliner::build_model(&op, &selection)
    } else {
        OutlinerModel::default()
    };
    let mut outliner_click: Option<OutlinerClick> = None;
    let mut closed: Option<Pane> = None;

    // Scope the behavior so its borrows of the section state end before we
    // drain the outliner click back into the selection below.
    let panel_rect = {
        let mut behavior = DockBehavior {
            depth: &mut panel,
            rows: &rows,
            props: &mut props,
            optimizer: &mut properties.optimizer,
            signals: &mut signals,
            selected: &selected,
            selection: &selection,
            outliner: &outliner_model,
            outliner_click: &mut outliner_click,
            show_signals: signals_panel.is_open(),
            show_plot: plot.panel.is_open(),
            plottable: &plottable,
            plot_config: &mut plot.config,
            console: &mut console.console,
            inputs: &mut console.inputs,
            registry: &console.registry,
            log: &console.log,
            closed: &mut closed,
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

    // A closed tab turns its section's toggle off — the same state the View
    // menu edits, so the two agree and the pane doesn't reappear next frame.
    // Signals+Plot is one tab over two toggles, so closing it closes both.
    match closed {
        Some(Pane::Tree) => op.panel.set_open(false),
        Some(Pane::Depth) => panel.open = false,
        Some(Pane::Signals) => {
            signals_panel.set_open(false);
            plot.panel.set_open(false);
        }
        Some(Pane::Properties) => properties.panel.open = false,
        Some(Pane::Console) => console.console.set_open(false),
        None => {}
    }

    // Universal selection: a tree-row click drives the same Selection /
    // SelectedJoint the viewport and node graph use, via the sanctioned seam.
    if let Some(click) = outliner_click {
        outliner::apply_click(click, &mut selection, &mut op);
    }
    Ok(())
}

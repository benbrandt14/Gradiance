//! The egui UI layer — the **only** module allowed to import `egui`.
//!
//! Testability strategy: this layer contains no decisions worth testing.
//! Every mutation of authored content is a typed intent (headless-tested
//! at the command layer); every widget is a thin projection of component
//! copies. The one sanctioned direct mutation is *editor settings
//! resources* (`GridSettings`, `SnapConfig`, `SimSettings`) — non-authored,
//! non-undoable configuration that downstream seams consume via
//! `Changed<>`/`resource_changed` (physics applies `SimSettings`; UI never
//! touches avian).

pub mod array_panel;
pub mod bottom_dock;
pub mod console;
pub mod context_menu;
pub mod curve;
pub mod depth_panel;
pub mod dock;
pub mod dock_sync;
pub mod fonts;
pub mod icons;
pub mod inspector;
pub mod joint_inspector;
pub mod labels;
pub mod menu;
pub mod node_graph;
pub mod optimizer;
pub mod outliner;
pub mod panels;
pub mod plot;
pub mod ports;
pub mod probe;
pub mod reflect_grid;
pub mod settings;
pub mod signals;
pub mod toolbar;
pub mod view_cube;
pub mod widgets;

use bevy::prelude::*;
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use gradiance_interaction::PointerOverUi;

/// Screen rects claimed this frame by the **background-layer** docked panels
/// (the right dock, the bottom dock). egui draws these into a root `Ui` on
/// [`egui::LayerId::background()`], which `is_pointer_over_egui` deliberately
/// ignores — so without this the scene tools/camera would react to input over
/// them (the "click-through"). Each panel pushes its rect while open;
/// `capture_pointer_over_ui` folds them into [`PointerOverUi`] and clears the
/// list for the next frame.
#[derive(Resource, Default)]
pub struct PanelRects(pub Vec<egui::Rect>);

impl PanelRects {
    /// Records a docked panel's occupied screen rect for this frame.
    pub fn push(&mut self, rect: egui::Rect) {
        self.0.push(rect);
    }

    /// The bottom `y` of a full-width panel docked to the top edge (the menu
    /// bar), or the viewport top when none — so floating chrome can sit below
    /// it instead of under it. Only meaningful once the top panel has pushed
    /// its rect this frame.
    pub fn top_inset(&self, viewport: egui::Rect) -> f32 {
        self.0
            .iter()
            .filter(|r| {
                (r.top() - viewport.top()).abs() < 2.0 && r.width() > viewport.width() * 0.8
            })
            .map(egui::Rect::bottom)
            .fold(viewport.top(), f32::max)
    }

    /// The left `x` of a panel docked to the right edge (the right dock), or the
    /// viewport right when none — so bottom overlays and the view cube can stay
    /// left of it. Only meaningful once the right panel has pushed its rect.
    pub fn right_inset(&self, viewport: egui::Rect) -> f32 {
        self.0
            .iter()
            .filter(|r| {
                (r.right() - viewport.right()).abs() < 2.0 && r.left() > viewport.center().x
            })
            .map(egui::Rect::left)
            .fold(viewport.right(), f32::min)
    }

    /// The top `y` of a panel docked to the bottom edge (the bottom dock), or
    /// the viewport bottom when none. The filter requires the panel to sit in
    /// the lower half so the full-height right dock (whose bottom also touches
    /// the edge) doesn't count.
    pub fn bottom_inset(&self, viewport: egui::Rect) -> f32 {
        self.0
            .iter()
            .filter(|r| {
                (r.bottom() - viewport.bottom()).abs() < 2.0 && r.top() > viewport.center().y
            })
            .map(egui::Rect::top)
            .fold(viewport.bottom(), f32::min)
    }

    /// The central scene rect (logical): the viewport inset by the menu (top),
    /// the right dock, and the bottom dock. The left edge is never inset — the
    /// tool palette and transport float *over* the scene (Blender-style).
    pub fn scene_rect(&self, viewport: egui::Rect) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(viewport.min.x, self.top_inset(viewport)),
            egui::pos2(self.right_inset(viewport), self.bottom_inset(viewport)),
        )
    }
}

/// Converts the scene pane's logical rect into a camera `Viewport` in physical
/// pixels, clamped to the window. Returns `None` when the pane is degenerate
/// (fully covered / off-window) so the camera falls back to the full window.
/// Pure, so the routing math is unit-testable without a window.
pub fn scene_viewport(
    scene: egui::Rect,
    scale_factor: f32,
    window_physical: bevy::math::UVec2,
) -> Option<bevy::camera::Viewport> {
    let sf = scale_factor.max(1e-3);
    let to_px = |v: f32, cap: u32| ((v * sf).round().max(0.0) as u32).min(cap);
    let x0 = to_px(scene.min.x, window_physical.x);
    let y0 = to_px(scene.min.y, window_physical.y);
    let x1 = to_px(scene.max.x, window_physical.x);
    let y1 = to_px(scene.max.y, window_physical.y);
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    // A near-empty pane (docks cover the window) falls back to the full window
    // rather than a zero-area viewport (which the renderer rejects).
    if w < 16 || h < 16 {
        return None;
    }
    Some(bevy::camera::Viewport {
        physical_position: bevy::math::UVec2::new(x0, y0),
        physical_size: bevy::math::UVec2::new(w, h),
        ..default()
    })
}

/// Whether `pos` falls inside any recorded background-layer panel rect — pure,
/// so the click-through gate is unit-testable without a window.
fn pointer_over_panels(rects: &[egui::Rect], pos: Option<egui::Pos2>) -> bool {
    pos.is_some_and(|p| rects.iter().any(|r| r.contains(p)))
}

/// Installs egui and the editor panels (no-op headless).
#[derive(Default)]
pub struct GradianceUiPlugin;

impl Plugin for GradianceUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::render::RenderPlugin>() {
            return;
        }
        app.add_plugins(EguiPlugin::default());
        // Give egui its OWN full-window camera (below): the scene `Camera3d`
        // must be free to route into a sub-viewport (stage 2) without dragging
        // egui's drawable area with it. So stop bevy_egui from auto-attaching
        // the primary context to the scene camera, and spawn a dedicated UI
        // camera that owns it.
        if let Some(mut egui_settings) = app.world_mut().get_resource_mut::<EguiGlobalSettings>() {
            egui_settings.auto_create_primary_context = false;
        }
        app.add_systems(Startup, spawn_ui_camera);
        app.init_resource::<toolbar::ToolIcons>();
        app.add_systems(Startup, toolbar::load_tool_icons);
        app.init_resource::<icons::Icons>();
        app.add_systems(Startup, icons::load_icons);
        app.init_resource::<settings::SettingsWindow>();
        app.init_resource::<inspector::InspectorPanel>();
        app.init_resource::<context_menu::ContextMenu>();
        app.init_resource::<depth_panel::DepthPanel>();
        app.init_resource::<console::ScriptConsole>();
        app.init_resource::<plot::PlotPanel>();
        app.init_resource::<plot::PlotConfig>();
        app.init_resource::<probe::ProbePanel>();
        app.init_resource::<signals::SignalsPanel>();
        app.init_resource::<node_graph::NodeGraph>();
        app.init_resource::<outliner::ObjectTreePanel>();
        app.init_resource::<optimizer::OptimizerExpanded>();
        app.init_resource::<array_panel::ArrayWindow>();
        app.init_resource::<PanelRects>();
        app.init_resource::<menu::AboutWindow>();
        app.init_resource::<dock::RightDock>();
        app.init_resource::<bottom_dock::BottomDock>();
        app.add_systems(
            EguiPrimaryContextPass,
            (
                fonts::install_fonts,
                menu::menu_bar,
                toolbar::toolbar,
                dock::right_dock,
                view_cube::view_cube,
                labels::draw_workspace_labels,
                joint_inspector::joint_inspector,
                settings::settings_window,
                optimizer::optimizer_window,
                array_panel::array_window,
                bottom_dock::bottom_dock,
                probe::probe_panel,
                context_menu::context_menu,
                // Stage 2: route the scene camera into the dock-bounded pane.
                // Safe now that egui renders on its own full-window camera
                // (`spawn_ui_camera`), so shrinking the scene camera no longer
                // shrinks the UI.
                apply_scene_viewport,
                capture_pointer_over_ui,
            )
                .chain(),
        );
        app.add_systems(Update, context_menu::open_context_menu);
    }
}

/// Spawns the dedicated **UI camera** that hosts the primary egui context: a
/// full-window `Camera2d` rendering *after* the scene (higher `order`) and not
/// clearing it (`ClearColorConfig::None`), so egui overlays the scene. Because
/// this camera has no viewport of its own, egui's drawable area stays the full
/// window even when [`apply_scene_viewport`] routes the scene `Camera3d` into a
/// sub-pane — which is what breaks the stage-2 feedback loop.
fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        // Render NO world entities — in particular no gizmos (grid, outlines,
        // joint glyphs, handles), which otherwise this 2D camera would redraw
        // flat over the 3D scene's gizmos (the "duplicate flat view"). egui
        // still overlays: bevy_egui's pass runs regardless of render layers.
        bevy::camera::visibility::RenderLayers::none(),
        PrimaryEguiContext,
    ));
}

/// Routes the scene camera to render only into the **scene pane** — the central
/// area left by the docked panels — so the docks are a real shell around the
/// viewport rather than an overlay on top of it (`docs/ui-shell-decision.md`,
/// stage 2). Bevy's `viewport_to_world` already offsets by the camera viewport,
/// so picking/gizmos follow the pane for free. Only the scene `Camera3d` is
/// routed; egui lives on its own full-window camera (`spawn_ui_camera`), so
/// shrinking this viewport no longer shrinks the UI. Runs after the docks have
/// pushed their rects (before `capture_pointer_over_ui` clears them).
pub fn apply_scene_viewport(
    mut contexts: EguiContexts,
    panels: Res<PanelRects>,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let scene = panels.scene_rect(ctx.viewport_rect());
    let Some(window) = windows.iter().next() else {
        return Ok(());
    };
    let phys = bevy::math::UVec2::new(window.physical_width(), window.physical_height());
    let viewport = scene_viewport(scene, window.scale_factor(), phys);
    for mut camera in &mut cameras {
        camera.viewport.clone_from(&viewport);
    }
    Ok(())
}

/// Publishes whether egui wants the pointer/keyboard, for shortcut and
/// tool/camera gating.
fn capture_pointer_over_ui(
    mut contexts: EguiContexts,
    mut over_ui: ResMut<PointerOverUi>,
    mut keyboard: ResMut<gradiance_interaction::KeyboardCaptured>,
    mut panels: ResMut<PanelRects>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // `is_pointer_over_egui` covers normal areas/windows but not the
    // background-layer docked panels — fold their rects in so input over them
    // doesn't leak to the scene. Runs last in the pass; clear for next frame.
    let pointer = ctx.pointer_latest_pos();
    over_ui.0 = ctx.is_pointer_over_egui() || pointer_over_panels(&panels.0, pointer);
    keyboard.0 = ctx.egui_wants_keyboard_input();
    panels.0.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PanelRects, pointer_over_panels, scene_viewport};
    use bevy::math::UVec2;
    use bevy_egui::egui::{Pos2, Rect};

    #[test]
    fn the_scene_rect_insets_by_top_right_and_bottom_docks_but_not_the_left() {
        let viewport = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(800.0, 600.0));
        let mut panels = PanelRects::default();
        // Menu bar (full width, top), right dock (full height, right), bottom
        // dock (lower-half, bottom-left), and a small floating view cube that
        // must NOT inset the scene.
        panels.push(Rect::from_min_max(
            Pos2::new(0.0, 0.0),
            Pos2::new(800.0, 24.0),
        ));
        panels.push(Rect::from_min_max(
            Pos2::new(560.0, 24.0),
            Pos2::new(800.0, 600.0),
        ));
        panels.push(Rect::from_min_max(
            Pos2::new(0.0, 400.0),
            Pos2::new(560.0, 600.0),
        ));
        panels.push(Rect::from_min_max(
            Pos2::new(480.0, 30.0),
            Pos2::new(540.0, 90.0),
        )); // view cube
        let scene = panels.scene_rect(viewport);
        assert_eq!(
            scene.min,
            Pos2::new(0.0, 24.0),
            "top inset by menu, left free"
        );
        assert_eq!(
            scene.max,
            Pos2::new(560.0, 400.0),
            "right inset by dock, bottom by bottom-dock"
        );
    }

    #[test]
    fn empty_panels_leave_the_scene_rect_at_the_full_viewport() {
        // No docks pushed → every inset falls back to the viewport edge, so the
        // scene is the whole window (and scene_viewport is the full window).
        let viewport = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(800.0, 600.0));
        let panels = PanelRects::default();
        assert_eq!(panels.scene_rect(viewport), viewport);
    }

    #[test]
    fn the_insets_ignore_panels_on_the_wrong_edge() {
        // Each inset only counts a panel actually docked to its edge: a
        // full-height right dock must not be read as a bottom dock, and a small
        // interior overlay (the view cube) must not inset any edge.
        let viewport = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(800.0, 600.0));
        let mut panels = PanelRects::default();
        let right_dock = Rect::from_min_max(Pos2::new(560.0, 0.0), Pos2::new(800.0, 600.0));
        let view_cube = Rect::from_min_max(Pos2::new(480.0, 30.0), Pos2::new(540.0, 90.0));
        panels.push(right_dock);
        panels.push(view_cube);
        // The right dock touches the bottom edge but sits in the upper half →
        // not a bottom dock; the view cube is interior → insets nothing.
        assert!(
            (panels.right_inset(viewport) - 560.0).abs() < 1e-4,
            "right dock recognized"
        );
        assert!(
            (panels.bottom_inset(viewport) - 600.0).abs() < 1e-4,
            "full-height right dock is not a bottom dock"
        );
        assert!(
            panels.top_inset(viewport).abs() < 1e-4,
            "no top-docked panel"
        );
    }

    #[test]
    fn scene_viewport_scales_to_physical_and_rejects_degenerate() {
        let scene = Rect::from_min_max(Pos2::new(0.0, 24.0), Pos2::new(560.0, 400.0));
        // scale_factor 2 → physical is doubled; position and size in px.
        let vp = scene_viewport(scene, 2.0, UVec2::new(1600, 1200)).expect("valid pane");
        assert_eq!(vp.physical_position, UVec2::new(0, 48));
        assert_eq!(vp.physical_size, UVec2::new(1120, 752));
        // A sliver (docks cover almost everything) → None (full-window fallback).
        let sliver = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(8.0, 600.0));
        assert!(scene_viewport(sliver, 1.0, UVec2::new(800, 600)).is_none());
    }

    #[test]
    fn a_pointer_inside_a_panel_rect_counts_as_over_ui() {
        let dock = Rect::from_min_max(Pos2::new(100.0, 0.0), Pos2::new(200.0, 300.0));
        let rects = [dock];
        assert!(pointer_over_panels(&rects, Some(Pos2::new(150.0, 100.0))));
        // Over the free scene area, not any panel.
        assert!(!pointer_over_panels(&rects, Some(Pos2::new(50.0, 100.0))));
        // No pointer (cursor off-window) is never "over UI".
        assert!(!pointer_over_panels(&rects, None));
        // No panels drawn this frame.
        assert!(!pointer_over_panels(&[], Some(Pos2::new(150.0, 100.0))));
    }
}

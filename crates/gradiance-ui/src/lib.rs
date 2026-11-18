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
pub mod groups;
pub mod host;
pub mod icons;
pub mod inspector;
pub mod joint_inspector;
pub mod labels;
pub mod menu;
pub mod node_graph;
pub mod ops;
pub mod optimizer;
pub mod outliner;
pub mod panel_ops;
pub mod panels;
pub mod plot;
pub mod ports;
pub mod probe;
pub mod reflect_grid;
pub mod review;
pub mod settings;
pub mod signals;
pub mod theme;
pub mod toolbar;
pub mod vendor;
pub mod view_cube;
pub mod widget_registry;
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

/// The full-window equivalent of a scene-pane `viewport`: the
/// [`SubCameraView`](bevy::camera::SubCameraView) that, on a camera with **no**
/// viewport, frames the scene exactly as `projection` would inside `viewport`
/// — same scale, same pane centre, every world point on the same window pixel.
/// The window around the pane is simply more of the same view (it lies under
/// the opaque docks). `None` for a custom projection, whose sub-view maths we
/// cannot know; the caller keeps the viewport then.
///
/// Why it exists: Bevy 0.19's SSAO assumes the camera's viewport *is* its
/// render target. Its textures are viewport-sized, but the depth/normal
/// prepass textures are target-sized and the PBR shader reads the AO texture
/// at framebuffer coordinates (`in.position.xy`, which include the viewport
/// offset). With the pane below the menu bar, every row past the AO texture's
/// height read as zero (full occlusion: a black strip the bar's height at the
/// bottom) and the AO above it was shifted. Rendering full-window with an
/// off-centre projection gives SSAO the only layout it supports.
///
/// The two projection kinds need different sub-views because Bevy feeds
/// `Projection::update` the *logical viewport size* — the window, once the
/// viewport is gone. An orthographic area then already has the right
/// world-per-pixel scale, so the "full" view is window-sized and only
/// re-centred on the pane. A perspective field of view spans the full view's
/// height, so the full view is the pane itself and the window is a larger
/// sub-rect around it. Pure, so the equivalence is unit-tested.
pub fn full_window_sub_view(
    projection: &bevy::camera::Projection,
    viewport: &bevy::camera::Viewport,
    window_physical: bevy::math::UVec2,
) -> Option<bevy::camera::SubCameraView> {
    use bevy::camera::{Projection, SubCameraView};
    let origin = viewport.physical_position.as_vec2();
    let pane = viewport.physical_size.as_vec2();
    match projection {
        Projection::Orthographic(_) => Some(SubCameraView {
            full_size: window_physical,
            offset: window_physical.as_vec2() * 0.5 - (origin + pane * 0.5),
            size: window_physical,
        }),
        Projection::Perspective(_) => Some(SubCameraView {
            full_size: viewport.physical_size,
            offset: -origin,
            size: window_physical,
        }),
        Projection::Custom(_) => None,
    }
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
        // The operations register headless too, so every doorway lists them.
        ops::install(app);
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
        app.init_resource::<context_menu::ContextMenu>();
        app.init_resource::<plot::PlotConfig>();
        app.add_systems(Startup, review::open_in_review_mode);
        app.init_resource::<PanelRects>();
        app.init_resource::<menu::AboutWindow>();
        app.init_resource::<dock::RightDock>();
        app.init_resource::<bottom_dock::BottomDock>();
        app.add_systems(
            EguiPrimaryContextPass,
            (
                widget_registry::begin_frame,
                fonts::install_fonts,
                theme::apply_chrome,
                menu::menu_bar,
                toolbar::toolbar,
                dock::right_dock,
                view_cube::view_cube,
                labels::draw_workspace_labels,
                joint_inspector::joint_inspector,
                settings::settings_window,
                optimizer::optimizer_window,
                array_panel::array_window,
                review::review_window,
                bottom_dock::bottom_dock,
                probe::probe_panel,
                context_menu::context_menu,
                // Stage 2: route the scene camera into the dock-bounded pane.
                // Safe now that egui renders on its own full-window camera
                // (`spawn_ui_camera`), so shrinking the scene camera no longer
                // shrinks the UI.
                apply_scene_viewport,
                capture_pointer_over_ui,
                widget_registry::end_frame,
            )
                .chain(),
        );
        // The widget registry keeps a frame's names in thread-locals
        // (upstream shape): one thread must run the whole pass, from
        // `begin_frame` to `end_frame`. The pass is one chain, so nothing
        // ran in parallel before either.
        host::install_render_busy(app);
        app.edit_schedule(EguiPrimaryContextPass, |schedule| {
            schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
        // The panel operations (`panel_ops`) install headless with `ops`.
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
///
/// A camera with SSAO gets the same framing a different way — no viewport,
/// and a [`full_window_sub_view`] — because Bevy's SSAO breaks under a
/// viewport offset (see that function). Picking is unaffected either way:
/// `viewport_to_world` maps window coordinates through whichever projection
/// the camera has.
pub fn apply_scene_viewport(
    mut contexts: EguiContexts,
    panels: Res<PanelRects>,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<
        (
            &mut Camera,
            &bevy::camera::Projection,
            Has<bevy::pbr::ScreenSpaceAmbientOcclusion>,
        ),
        With<Camera3d>,
    >,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let scene = panels.scene_rect(ctx.viewport_rect());
    let Some(window) = windows.iter().next() else {
        return Ok(());
    };
    let phys = bevy::math::UVec2::new(window.physical_width(), window.physical_height());
    let viewport = scene_viewport(scene, window.scale_factor(), phys);
    for (mut camera, projection, ssao) in &mut cameras {
        let sub_view = viewport
            .as_ref()
            .filter(|_| ssao)
            .and_then(|vp| full_window_sub_view(projection, vp, phys));
        if sub_view.is_some() {
            camera.viewport = None;
        } else {
            camera.viewport.clone_from(&viewport);
        }
        camera.sub_camera_view = sub_view;
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
    use super::{PanelRects, full_window_sub_view, pointer_over_panels, scene_viewport};
    use bevy::camera::{OrthographicProjection, PerspectiveProjection, Projection, Viewport};
    use bevy::math::{Mat4, UVec2, Vec2, Vec3};
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

    /// Window pixel (physical) of a view-space point under `clip`, for a
    /// render rect at `origin` of `size`.
    fn pixel(clip: Mat4, point: Vec3, origin: Vec2, size: Vec2) -> Vec2 {
        let ndc = clip.project_point3(point);
        origin + Vec2::new(f32::midpoint(ndc.x, 1.0), (1.0 - ndc.y) * 0.5) * size
    }

    /// The SSAO framing: a full-window camera with [`full_window_sub_view`]
    /// puts every point on the same window pixel as the pane-viewport camera,
    /// for both projection kinds and at a non-unit scale factor.
    #[test]
    fn the_full_window_sub_view_frames_like_the_pane_viewport() {
        let window = UVec2::new(1280, 720);
        let scale_factor = 2.0;
        // The menu bar above (44 px physical) and a right dock (280 px).
        let viewport = Viewport {
            physical_position: UVec2::new(0, 44),
            physical_size: UVec2::new(1000, 676),
            ..Default::default()
        };
        let ortho = Projection::Orthographic(OrthographicProjection {
            near: -50.0,
            far: 50.0,
            scale: 0.01,
            ..OrthographicProjection::default_3d()
        });
        let persp = Projection::Perspective(PerspectiveProjection::default());
        let points = [
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::new(1.5, -2.0, -6.0),
            Vec3::new(-3.0, 1.0, -9.0),
        ];
        for base in [ortho, persp] {
            // Bevy updates the projection with the logical viewport size: the
            // pane with a viewport, the window without one.
            let mut in_pane = base.clone();
            let pane = viewport.physical_size.as_vec2();
            in_pane.update(pane.x / scale_factor, pane.y / scale_factor);
            let pane_clip = in_pane.get_clip_from_view();

            let mut full = base.clone();
            let win = window.as_vec2();
            full.update(win.x / scale_factor, win.y / scale_factor);
            let Some(sub) = full_window_sub_view(&full, &viewport, window) else {
                panic!("built-in projections always have a sub-view");
            };
            let full_clip = full.get_clip_from_view_for_sub(&sub);

            for point in points {
                let a = pixel(pane_clip, point, viewport.physical_position.as_vec2(), pane);
                let b = pixel(full_clip, point, Vec2::ZERO, win);
                assert!(a.distance(b) < 1e-2, "{point}: pane {a} vs full window {b}");
            }
        }
    }
}

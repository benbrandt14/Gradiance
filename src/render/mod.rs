//! Rendering: derived meshes/materials, scene lighting, and the grid.
//!
//! Default aesthetic: matte bodies under a shadow-casting directional
//! light. Everything here is derived state — meshes and materials are
//! rebuilt from authored components by `Changed<>`-driven sync systems and
//! never serialized.

pub mod debug_viz;
pub mod decorations;
pub mod extrude_sync;
pub mod grid;
pub mod joint_viz;
pub mod material_sync;
pub mod overlay;
pub mod plane;
pub mod scenery;
pub mod toon;

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

/// Installs cameras, lights, and the derived-visuals sync systems.
///
/// A no-op in headless apps (no render plugin present) apart from the
/// [`RenderSettings`](crate::domain::settings::RenderSettings) resource,
/// which persistence captures regardless of a renderer being attached.
#[derive(Default)]
pub struct GradianceRenderPlugin;

impl Plugin for GradianceRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::domain::settings::RenderSettings>();
        app.init_resource::<crate::domain::settings::LightingSettings>();
        app.init_resource::<crate::domain::settings::ScenerySettings>();
        // Reflected config seam for scripting (see script-lisp-decision.md).
        app.register_type::<crate::domain::settings::RenderSettings>();
        app.register_type::<crate::domain::settings::LightingSettings>();
        app.register_type::<crate::domain::settings::ScenerySettings>();
        if !app.is_plugin_added::<bevy::render::RenderPlugin>() {
            return;
        }
        toon::install(app);
        plane::install(app);
        overlay::install(app);
        app.init_resource::<GlobalAmbientLight>();
        app.add_systems(Startup, (setup_scene, scenery::setup));
        app.add_systems(
            Update,
            (
                scenery::apply_lighting
                    .run_if(resource_changed::<crate::domain::settings::LightingSettings>),
                scenery::apply_scenery
                    .run_if(resource_changed::<crate::domain::settings::ScenerySettings>),
                scenery::track_scene_depth,
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                extrude_sync::sync_body_meshes,
                material_sync::sync_body_materials,
                plane::sync_ground_planes,
            ),
        );
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default());
        app.add_systems(
            Update,
            (
                grid::draw_grid,
                joint_viz::draw_joints,
                decorations::draw_circle_radius_lines,
                decorations::draw_body_borders,
                debug_viz::draw_debug_overlays,
            ),
        );
        app.add_systems(
            PostUpdate,
            toon::apply_render_settings
                .run_if(resource_changed::<crate::domain::settings::RenderSettings>),
        );
    }
}

/// Spawns the editor camera (the light and back plane are settings-driven —
/// see [`scenery`]).
fn setup_scene(mut commands: Commands) {
    // Orthographic 3D camera looking down −Z at the XY sandbox plane;
    // extruded bodies span z ∈ [-320, 0]. The depth slab is made very
    // wide (near/far ±50k) so orbiting the view never pushes the scene —
    // or the huge back plane — outside the frustum (the "background clips
    // when the view tilts" bug); orthographic near/far only define a
    // depth range, so a large slab costs nothing.
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            near: -50_000.0,
            far: 50_000.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 600.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

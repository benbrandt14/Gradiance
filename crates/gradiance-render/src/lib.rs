//! Rendering: derived meshes/materials, scene lighting, and the grid.
//!
//! Default aesthetic: matte bodies under a shadow-casting directional
//! light. Everything here is derived state — meshes and materials are
//! rebuilt from authored components by `Changed<>`-driven sync systems and
//! never serialized.

pub mod arrows;
pub mod debug_viz;
pub mod decorations;
pub mod extrude_sync;
pub mod grid;
pub mod joint_viz;
pub mod material_sync;
pub mod particles_viz;
pub mod plane;
pub mod scenery;
pub mod stack;
pub mod toon;
pub mod tracer;

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

/// Installs cameras, lights, and the derived-visuals sync systems.
///
/// A no-op in headless apps (no render plugin present) apart from the
/// [`RenderSettings`](gradiance_domain::settings::RenderSettings) resource,
/// which persistence captures regardless of a renderer being attached.
#[derive(Default)]
pub struct GradianceRenderPlugin;

impl Plugin for GradianceRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<gradiance_domain::settings::RenderSettings>();
        app.init_resource::<gradiance_domain::settings::LightingSettings>();
        app.init_resource::<gradiance_domain::settings::ScenerySettings>();
        // Reflected config seam for scripting (see script-lisp-decision.md).
        app.register_type::<gradiance_domain::settings::RenderSettings>();
        app.register_type::<gradiance_domain::settings::LightingSettings>();
        app.register_type::<gradiance_domain::settings::ScenerySettings>();
        // Trail sampling is derived-state upkeep, not drawing — it runs
        // headless too (tests assert on trails).
        tracer::install_sampling(app);
        joint_viz::install_load(app);
        if !app.is_plugin_added::<bevy::render::RenderPlugin>() {
            return;
        }
        // The overlay gizmo groups are defined in `interaction` (their
        // writers) but registered here: gizmo groups schedule mesh-update
        // systems that need render-world assets, so headless apps must not
        // register them.
        gradiance_interaction::overlay::install(app);
        toon::install(app);
        plane::install(app);

        app.init_resource::<GlobalAmbientLight>();
        app.add_systems(Startup, (setup_scene, scenery::setup));
        app.add_systems(
            Update,
            (
                scenery::apply_lighting
                    .run_if(resource_changed::<gradiance_domain::settings::LightingSettings>),
                scenery::apply_scenery
                    .run_if(resource_changed::<gradiance_domain::settings::ScenerySettings>),
                scenery::track_scene_depth,
                // After the lighting sync so an SSAO toggle's MSAA change
                // reaches the overlay camera the same frame.
                scenery::match_overlay_msaa.after(scenery::apply_lighting),
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                extrude_sync::sync_body_meshes,
                material_sync::sync_body_materials,
                stack::stack_bodies.before(bevy::transform::TransformSystems::Propagate),
                plane::sync_ground_planes,
                toon::attach_face_fields,
            ),
        );
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default());
        app.add_systems(
            Update,
            (
                // Before the camera update: snapping ran in PreUpdate on the
                // previous `CameraScale`, so drawing from that same value keeps
                // the drawn step and the snapped step identical on every frame,
                // including the one where a zoom crosses a doubling boundary.
                grid::draw_grid.before(gradiance_interaction::InteractionSet),
                joint_viz::draw_joints.after(joint_viz::measure_joint_loads),
                decorations::draw_circle_radius_lines,
                decorations::draw_body_borders,
                decorations::draw_fluid_surfaces,
                arrows::draw_body_arrows,
                debug_viz::draw_debug_overlays,
                tracer::draw_traces,
                tracer::draw_node_glyphs,
                particles_viz::draw_particles,
            ),
        );
        app.add_systems(
            PostUpdate,
            toon::apply_render_settings
                .run_if(resource_changed::<gradiance_domain::settings::RenderSettings>),
        );
    }
}

/// Spawns the editor camera (the light and back plane are settings-driven —
/// see [`scenery`]).
fn setup_scene(mut commands: Commands) {
    // Orthographic 3D camera looking down −Z at the XY sandbox plane;
    // extruded bodies span z ∈ [-320, 0]. The depth slab is sized past
    // the infinite-plane quad corners so orbiting the view never pushes
    // the scene — or the megaquads — outside the frustum (the "background
    // clips when the view tilts" bug); see `camera::DEPTH_SLAB`.
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            near: -gradiance_interaction::camera::DEPTH_SLAB,
            far: gradiance_interaction::camera::DEPTH_SLAB,
            // The world is SI (metres); render `PIXELS_PER_METER` screen
            // pixels per metre. This world-per-pixel scale is the single
            // render seam between SI world space and screen space.
            scale: 1.0 / gradiance_units::world::PIXELS_PER_METER,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        // An identity tone curve: the toon top band shows a fill exactly as
        // authored. The default (TonyMcMapface) darkened and desaturated
        // saturated fills by up to a third, so the palette's contrast
        // design never reached the screen (brief 4b-3).
        bevy::render::view::Tonemapping::Linear,
    ));
}

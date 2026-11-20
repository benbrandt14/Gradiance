//! The engine's own debug view (brief 6-3): avian's `PhysicsDebugPlugin`,
//! driven by four switches of [`DebugSettings`].
//!
//! [`DebugSettings`] is the one place the switches live (workstation
//! config: never saved, never undone). [`apply_debug_settings`] copies them
//! onto avian's [`PhysicsGizmos`] config in [`GizmoConfigStore`] whenever
//! they change, so the menu rows, the `debug.*` operations and the Settings
//! window all reach the overlay the same way. With every switch off the
//! group is disabled and avian's draw systems do not run.
//!
//! What the overlay shows is what the engine holds — the colliders it built
//! (convex pieces), their broad-phase AABBs, the contacts it is solving and
//! each joint's anchors and error — where Gradiance's own overlays
//! (`gradiance_render::debug_viz`) draw the authored model.

use avian2d::prelude::{PhysicsDebugPlugin, PhysicsGizmos};
use bevy::gizmos::config::{GizmoConfig, GizmoConfigStore};
use bevy::prelude::*;
use gradiance_domain::settings::DebugSettings;
use gradiance_domain::theme::debug;

/// Adds avian's debug plugin and applies the default (all off) at once.
///
/// Headless (no `RenderPlugin`) the plugin is left out: a gizmo group
/// schedules mesh-update systems that need render-world assets. The config
/// is still put in the store and kept in step, so a headless app (the
/// tests, a remote client) sees the same switches reach it.
pub(crate) fn install(app: &mut App) {
    if app.is_plugin_added::<bevy::render::RenderPlugin>() {
        app.add_plugins(PhysicsDebugPlugin);
    }
    let mut config = GizmoConfig::default();
    let mut gizmos = PhysicsGizmos::default();
    if let Some(store) = app.world().get_resource::<GizmoConfigStore>()
        && let Some((live, _)) = store.get_config::<PhysicsGizmos>()
    {
        // Keep what avian's plugin set (its line width).
        config = live.clone();
    }
    configure(&mut config, &mut gizmos, &DebugSettings::default());
    app.world_mut()
        .get_resource_or_init::<GizmoConfigStore>()
        .insert(config, gizmos);
    // Before transform propagation: avian's draw systems run after it and
    // read the config in their run condition, so a switch shows the frame it
    // is flipped.
    app.add_systems(
        PostUpdate,
        apply_debug_settings
            .run_if(resource_exists_and_changed::<DebugSettings>)
            .before(TransformSystems::Propagate),
    );
}

/// Whether any engine overlay is on.
pub fn any_engine_overlay(debug: &DebugSettings) -> bool {
    debug.show_colliders || debug.show_aabbs || debug.show_contacts || debug.show_joints
}

/// The avian gizmo config for `debug`: a colour for each overlay that is on,
/// `None` for each that is off, and nothing avian draws that Gradiance has
/// no switch for (body axes, ray and shape casts, islands).
pub fn physics_gizmos(debug: &DebugSettings) -> PhysicsGizmos {
    let on =
        |flag: bool, color: gradiance_domain::appearance::Rgba| flag.then_some(Color::from(color));
    PhysicsGizmos {
        collider_color: on(debug.show_colliders, debug::COLLIDER),
        aabb_color: on(debug.show_aabbs, debug::AABB),
        contact_point_color: on(debug.show_contacts, debug::CONTACT),
        contact_normal_color: on(debug.show_contacts, debug::CONTACT),
        joint_anchor_color: on(debug.show_joints, debug::JOINT_ANCHOR),
        joint_separation_color: on(debug.show_joints, debug::JOINT_SEPARATION),
        // A sleeping body's outline dims, as avian's default does.
        sleeping_color_multiplier: Some([1.0, 1.0, 0.4, 1.0]),
        // Never hide the body meshes: the overlay draws over the scene.
        hide_meshes: false,
        ..PhysicsGizmos::none()
    }
}

/// Writes `debug` onto avian's gizmo group: enabled only while an overlay
/// is on, drawn in front of the body prisms like every editor overlay.
pub fn configure(config: &mut GizmoConfig, gizmos: &mut PhysicsGizmos, debug: &DebugSettings) {
    config.enabled = any_engine_overlay(debug);
    // In front of the extruded bodies, as `OverlayGizmos` is.
    config.depth_bias = -1.0;
    *gizmos = physics_gizmos(debug);
}

/// Copies the switches onto [`PhysicsGizmos`] when they change.
///
/// The store is written without change detection on purpose: avian's
/// `change_mesh_visibility` resets the `Visibility` of every rigid body
/// whenever the store changes, which would show a ground the Scenery
/// settings hid. Nothing else reads the store's change flag.
pub fn apply_debug_settings(debug: Res<DebugSettings>, mut store: ResMut<GizmoConfigStore>) {
    let (config, gizmos) = store
        .bypass_change_detection()
        .config_mut::<PhysicsGizmos>();
    configure(config, gizmos, &debug);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_off_disables_the_group_and_draws_nothing() {
        let debug = DebugSettings::default();
        assert!(!any_engine_overlay(&debug));
        let gizmos = physics_gizmos(&debug);
        assert!(gizmos.collider_color.is_none());
        assert!(gizmos.aabb_color.is_none());
        assert!(gizmos.contact_point_color.is_none());
        assert!(gizmos.joint_anchor_color.is_none());
        assert!(gizmos.axis_lengths.is_none());
        assert!(!gizmos.hide_meshes);
    }

    #[test]
    fn each_switch_turns_on_its_own_colour_only() {
        let debug = DebugSettings {
            show_contacts: true,
            ..DebugSettings::default()
        };
        assert!(any_engine_overlay(&debug));
        let gizmos = physics_gizmos(&debug);
        assert!(gizmos.contact_point_color.is_some());
        assert!(gizmos.contact_normal_color.is_some());
        assert!(gizmos.collider_color.is_none());
        assert!(gizmos.aabb_color.is_none());
        assert!(gizmos.joint_separation_color.is_none());
    }

    #[test]
    fn velocities_are_not_an_engine_overlay() {
        // avian 0.8 has no velocity gizmo: Gradiance draws its own.
        let debug = DebugSettings {
            show_velocities: true,
            ..DebugSettings::default()
        };
        assert!(!any_engine_overlay(&debug));
    }
}

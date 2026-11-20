//! The physics debug toggles as operations (brief 6-3): `debug.colliders`,
//! `debug.contacts`, `debug.joints`, `debug.aabbs` and `debug.velocities`.
//!
//! Each sets one switch of [`DebugSettings`] through the config seam. The
//! switches are **workstation config** (CLAUDE.md invariant 4): never
//! saved with the scene and never an undo step, like the Settings
//! window's Debug tab that edits the same resource. The first four reach
//! avian's `PhysicsGizmos` config through `gradiance_physics::debug_gizmos`;
//! velocities are Gradiance's own overlay (avian 0.8 has no velocity gizmo).
//!
//! [`DEBUG_TOGGLES`] is the one table: the View > Debug menu is a loop over
//! it, so a menu row and its operation cannot drift.

use bevy::prelude::*;
use gradiance_domain::settings::DebugSettings;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam,
};
use serde::Deserialize;

/// One debug toggle: its menu label, the operation that sets it, and how
/// to read it.
#[derive(Debug, Clone, Copy)]
pub struct DebugToggle {
    /// The View > Debug row's label.
    pub label: &'static str,
    /// The operation's id.
    pub id: &'static str,
    /// The switch's field in [`DebugSettings`] (a reflect path).
    pub field: &'static str,
    /// Reads the switch.
    pub get: fn(&DebugSettings) -> bool,
}

/// Every physics debug toggle, in menu order.
pub const DEBUG_TOGGLES: [DebugToggle; 5] = [
    DebugToggle {
        label: "Colliders",
        id: DebugColliders::META.id,
        field: "show_colliders",
        get: |d| d.show_colliders,
    },
    DebugToggle {
        label: "Contacts",
        id: DebugContacts::META.id,
        field: "show_contacts",
        get: |d| d.show_contacts,
    },
    DebugToggle {
        label: "Joints",
        id: DebugJoints::META.id,
        field: "show_joints",
        get: |d| d.show_joints,
    },
    DebugToggle {
        label: "AABBs",
        id: DebugAabbs::META.id,
        field: "show_aabbs",
        get: |d| d.show_aabbs,
    },
    DebugToggle {
        label: "Velocities",
        id: DebugVelocities::META.id,
        field: "show_velocities",
        get: |d| d.show_velocities,
    },
];

/// Registers the toggles and the [`DebugSettings`] mirror.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<DebugColliders>();
        registry.register::<DebugContacts>();
        registry.register::<DebugJoints>();
        registry.register::<DebugAabbs>();
        registry.register::<DebugVelocities>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_debug_mirror);
}

/// Publishes the debug switches into a new operation context.
fn fill_debug_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(debug) = world.get_resource::<DebugSettings>() {
        ctx.insert(debug.clone());
    }
}

/// Lowers one toggle: the switch `field` set to `on`.
fn set_switch(ctx: &OpContext, field: &str, on: bool) -> Result<OpOutput, OpError> {
    let debug = ctx.get::<DebugSettings>()?;
    let value = if on { 1.0 } else { 0.0 };
    Ok(OpOutput::none().setting(debug, field, value)?)
}

/// Declares one toggle operation over a [`DebugSettings`] switch.
macro_rules! debug_toggle {
    ($(#[$doc:meta])* $name:ident, $id:literal, $lisp:literal, $menu:literal, $field:literal, $help:literal) => {
        $(#[$doc])*
        #[derive(Debug, Deserialize, Reflect)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            /// Show (true) or hide (false) the overlay.
            pub on: bool,
        }

        impl Operation for $name {
            const META: OpMeta = OpMeta {
                id: $id,
                lisp: $lisp,
                doc: $help,
                seam: Seam::Config,
                menu: Some($menu),
                chord: None,
                undo: None,
                example: r#"{"on": true}"#,
            };

            fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
                set_switch(ctx, $field, self.on)
            }
        }
    };
}

debug_toggle!(
    /// Show or hide the engine's collider outlines.
    DebugColliders,
    "debug.colliders",
    "debug-colliders",
    "View/Debug/Colliders",
    "show_colliders",
    "Show or hide the colliders the physics engine built (its convex pieces), drawn over the bodies. Workstation config: never saved, never undone."
);

debug_toggle!(
    /// Show or hide the engine's contact points.
    DebugContacts,
    "debug.contacts",
    "debug-contacts",
    "View/Debug/Contacts",
    "show_contacts",
    "Show or hide the contact points the physics engine is solving, with their normals. Workstation config: never saved, never undone."
);

debug_toggle!(
    /// Show or hide the engine's joints.
    DebugJoints,
    "debug.joints",
    "debug-joints",
    "View/Debug/Joints",
    "show_joints",
    "Show or hide the physics engine's joints: each anchor, and a line where a joint's two anchors have drifted apart. Workstation config: never saved, never undone."
);

debug_toggle!(
    /// Show or hide the engine's broad-phase boxes.
    DebugAabbs,
    "debug.aabbs",
    "debug-aabbs",
    "View/Debug/AABBs",
    "show_aabbs",
    "Show or hide each collider's broad-phase bounding box (AABB), as the physics engine holds it. Workstation config: never saved, never undone."
);

debug_toggle!(
    /// Show or hide the velocity arrows.
    DebugVelocities,
    "debug.velocities",
    "debug-velocities",
    "View/Debug/Velocities",
    "show_velocities",
    "Show or hide each body's velocity arrow (and a ring on a sleeping body), read from the physics engine. Workstation config: never saved, never undone."
);

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_op::reflect_path::write_path;

    #[test]
    fn every_row_names_its_own_field() {
        for row in DEBUG_TOGGLES {
            let mut debug = DebugSettings::default();
            assert!(!(row.get)(&debug), "{} is off by default", row.id);
            assert!(write_path(&mut debug, row.field, 1.0).is_ok(), "{}", row.id);
            assert!((row.get)(&debug), "{} reads its own field", row.id);
            let others = DEBUG_TOGGLES
                .iter()
                .filter(|other| other.id != row.id && (other.get)(&debug))
                .count();
            assert_eq!(others, 0, "{} turns on only itself", row.id);
        }
    }
}

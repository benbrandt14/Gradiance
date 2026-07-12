//! The interaction layer: cursor, camera, selection, shortcuts, snapping,
//! and gesture constraints.
//!
//! Design notes (Algodoo + CAD conventions, captured as behavior):
//! - **Object snaps beat grid snaps** (vertex/midpoint/center/edge, nearest
//!   wins) and every tool consumes the same [`snap::SnappedCursor`], so new
//!   tools and gestures inherit snapping for free.
//! - **Constrained motion**: hold `X`/`Y` for horizontal/vertical-only,
//!   `X`+`Y` for dominant-axis lock; hold `Ctrl` while rotating for
//!   quantized angle steps ([`gesture::GestureConstraints`]).
//!   `Shift` is selection-only (toggle / additive band).
//! - **Grids are coordinate systems**, not wallpaper: movable origin,
//!   rotatable basis, and pluggable geometry (Cartesian / Isometric /
//!   Polar) via `GridSystem`.
//!
//! This module may read avian directly (or through the `physics::queries`
//! read facade) but must not import `egui` (UI publishes [`PointerOverUi`]
//! instead).

pub mod align;
pub mod camera;
pub mod cursor;
pub mod gesture;
pub mod indicators;
pub mod input;
pub mod joint_edit;
pub mod pointer;
pub mod selection;
pub mod snap;
pub mod tools;

use bevy::prelude::*;

/// Set for interaction systems that produce intents (ordered before the
/// command dispatcher by the plugin).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractionSet;

/// True while the pointer is over UI; tools and camera gestures must not
/// react to world input then. Written by the UI layer, read here.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PointerOverUi(pub bool);

/// True while UI text editing wants the keyboard; editor shortcuts must
/// not fire then (typing "s" in a field is not a tool switch). Written
/// by the UI layer, read here.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct KeyboardCaptured(pub bool);

/// Installs the interaction layer.
#[derive(Default)]
pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerOverUi>();
        app.init_resource::<KeyboardCaptured>();
        app.init_resource::<pointer::PointerButtons>();
        app.init_resource::<cursor::CursorWorldPos>();
        app.init_resource::<selection::Selection>();
        app.init_resource::<selection::SelectedJoint>();
        app.init_resource::<joint_edit::SuppressSelectPress>();
        app.init_resource::<joint_edit::JointAnchorDrag>();
        app.init_resource::<snap::SnappedCursor>();
        app.init_resource::<snap::SnapExclusions>();
        app.init_resource::<gesture::GestureConstraints>();
        app.init_resource::<crate::domain::settings::GridSettings>();
        app.init_resource::<crate::domain::settings::SnapConfig>();
        app.init_resource::<crate::domain::settings::DebugSettings>();
        app.init_resource::<crate::domain::settings::ToolDefaults>();
        // Register the config-seam settings for reflection: the scripting
        // registry addresses these resources by reflected type name
        // (see `docs/script-lisp-decision.md`).
        app.register_type::<crate::domain::settings::GridSettings>();
        app.register_type::<crate::domain::settings::SnapConfig>();
        app.register_type::<crate::domain::settings::DebugSettings>();
        app.register_type::<crate::domain::settings::ToolDefaults>();
        app.init_resource::<camera::CameraRig>();

        app.add_plugins(input::EditorInputPlugin);
        app.add_plugins(tools::ToolsPlugin);

        app.add_systems(
            PreUpdate,
            (
                pointer::update_pointer_buttons,
                cursor::update_cursor_world_pos,
                gesture::update_gesture_constraints,
                snap::update_snapped_cursor,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                camera::pan_and_zoom_camera,
                camera::apply_camera_rig,
                input::apply_shortcuts,
                selection::prune_dead_selection,
                joint_edit::clear_joint_on_tool_change,
            )
                .chain()
                .in_set(InteractionSet)
                .before(crate::command::CommandDispatchSet),
        );
        // Indicator gizmos need the gizmo plugin (present with rendering).
        if app.is_plugin_added::<bevy::render::RenderPlugin>() {
            // The selection outline draws in front of the extruded prisms
            // and the grid (same treatment as joint glyphs).
            app.insert_gizmo_config(
                indicators::SelectionGizmos,
                GizmoConfig {
                    depth_bias: -1.0,
                    ..default()
                },
            );
            app.add_systems(
                Update,
                (
                    indicators::draw_snap_indicator,
                    indicators::draw_constraint_guides,
                    indicators::draw_selection_outlines,
                ),
            );
        }
    }
}

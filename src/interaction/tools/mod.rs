//! Editor tools: gesture state machines gated on [`ToolState`].
//!
//! The gesture contract (mechanically reviewed, see `CLAUDE.md`):
//! press → transient preview only (gizmos, kinematic-held `Transform`
//! writes) → release → **exactly one intent**, which dispatch turns into
//! one undoable command. Tools never touch authored components or the
//! command stack directly.
//!
//! Tools read the [`SnappedCursor`](crate::interaction::snap::SnappedCursor)
//! (never the raw cursor) and honor
//! [`GestureConstraints`](crate::interaction::gesture::GestureConstraints),
//! so snapping and axis/rotation constraints work identically everywhere.

pub mod box_tool;
pub mod circle_tool;
pub mod drag_tool;
pub mod ground_tool;
pub mod handles;
pub mod polygon_tool;
pub mod select;

use crate::core::states::ToolState;
use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::InteractionSet;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;

/// True while any tool gesture is in progress. The camera skips
/// right-drag panning while set (rotation gestures own the right button).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ActiveGesture(pub bool);

/// Installs every tool.
pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveGesture>();
        app.init_resource::<select::SelectGesture>();
        app.init_resource::<handles::ScaleFrame>();
        app.init_resource::<box_tool::BoxDraft>();
        app.init_resource::<circle_tool::CircleDraft>();
        app.init_resource::<ground_tool::GroundDraft>();
        app.init_resource::<polygon_tool::PolygonDraft>();

        app.add_systems(
            Update,
            (
                select::run_select_tool.run_if(in_state(ToolState::Select)),
                drag_tool::run_drag_tool.run_if(in_state(ToolState::Drag)),
                box_tool::run_box_tool.run_if(in_state(ToolState::Box)),
                circle_tool::run_circle_tool.run_if(in_state(ToolState::Circle)),
                ground_tool::run_ground_tool.run_if(in_state(ToolState::Ground)),
                polygon_tool::run_polygon_tool.run_if(in_state(ToolState::Polygon)),
            )
                .in_set(InteractionSet)
                .before(crate::command::CommandDispatchSet),
        );

        if app.is_plugin_added::<bevy::render::RenderPlugin>() {
            app.add_systems(
                Update,
                (
                    select::draw_select_previews.run_if(in_state(ToolState::Select)),
                    handles::draw_handles.run_if(in_state(ToolState::Select)),
                    box_tool::draw_box_preview.run_if(in_state(ToolState::Box)),
                    circle_tool::draw_circle_preview.run_if(in_state(ToolState::Circle)),
                    ground_tool::draw_ground_preview.run_if(in_state(ToolState::Ground)),
                    polygon_tool::draw_polygon_preview.run_if(in_state(ToolState::Polygon)),
                ),
            );
        }
    }
}

/// Hit-tests the topmost authored body at `p`.
///
/// Front-most layer bit wins; ground half-planes always lose to shapes
/// (Algodoo behavior: you never accidentally grab the floor).
pub fn topmost_body_at(
    p: Vec2,
    physics: &PhysicsQueries,
    bodies: &Query<(&ShapeDef, &LayerMask32), With<Body>>,
) -> Option<Entity> {
    let mut best: Option<(bool, u32, Entity)> = None;
    for entity in physics.bodies_at_point(p) {
        let Ok((shape, layers)) = bodies.get(entity) else {
            continue;
        };
        let is_ground = matches!(shape, ShapeDef::HalfPlane);
        let front_bit = layers.occupied_range().map_or(32, |(min, _)| min);
        let key = (is_ground, front_bit, entity);
        if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
            best = Some(key);
        }
    }
    best.map(|(_, _, e)| e)
}

/// Default appearance for newly authored bodies: hue follows the
/// front-most layer bit (`hsl(bit · 30°, 0.8, 0.5)`).
pub fn appearance_for_layers(layers: &LayerMask32) -> crate::domain::appearance::Appearance {
    let bit = layers.occupied_range().map_or(0, |(min, _)| min);
    crate::domain::appearance::Appearance {
        fill: crate::domain::appearance::Rgba::from_hsl(bit as f32 * 30.0, 0.8, 0.5),
    }
}

/// Builds a fresh body record with default props/layers at `pose`.
pub fn new_body_record(
    shape: ShapeDef,
    pos: Vec2,
    rot: f32,
) -> crate::command::snapshot::BodyRecord {
    let layers = LayerMask32::default();
    crate::command::snapshot::BodyRecord {
        id: crate::core::ids::StableId::new(),
        pose: crate::core::units::PosRot { pos, rot },
        shape,
        props: crate::domain::props::PhysicalProps::default(),
        appearance: appearance_for_layers(&layers),
        layers,
        group: None,
    }
}

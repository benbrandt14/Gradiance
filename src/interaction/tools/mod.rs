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
pub mod connector_tool;
pub mod cut_tool;
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
        app.init_resource::<connector_tool::ConnectorDraft>();
        app.init_resource::<box_tool::BoxDraft>();
        app.init_resource::<cut_tool::CutDraft>();
        app.init_resource::<circle_tool::CircleDraft>();
        app.init_resource::<ground_tool::GroundDraft>();
        app.init_resource::<polygon_tool::PolygonDraft>();

        app.add_systems(
            Update,
            (
                select::run_select_tool.run_if(in_state(ToolState::Select)),
                drag_tool::run_drag_tool.run_if(in_state(ToolState::Drag)),
                box_tool::run_box_tool.run_if(in_state(ToolState::Box)),
                cut_tool::run_cut_tool.run_if(in_state(ToolState::Cut)),
                circle_tool::run_circle_tool.run_if(in_state(ToolState::Circle)),
                ground_tool::run_ground_tool.run_if(in_state(ToolState::Ground)),
                polygon_tool::run_polygon_tool.run_if(in_state(ToolState::Polygon)),
                connector_tool::run_connector_tool,
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
                    cut_tool::draw_cut_preview.run_if(in_state(ToolState::Cut)),
                    circle_tool::draw_circle_preview.run_if(in_state(ToolState::Circle)),
                    ground_tool::draw_ground_preview.run_if(in_state(ToolState::Ground)),
                    polygon_tool::draw_polygon_preview.run_if(in_state(ToolState::Polygon)),
                    connector_tool::draw_connector_preview,
                ),
            );
        }
    }
}

/// All authored bodies at `p`, topmost first.
///
/// Front-most layer bit wins; ground half-planes always sort last
/// (Algodoo behavior: you never accidentally grab the floor).
pub fn bodies_at_sorted(
    p: Vec2,
    physics: &PhysicsQueries,
    bodies: &Query<(&ShapeDef, &LayerMask32), With<Body>>,
) -> Vec<Entity> {
    let mut hits: Vec<(bool, u32, Entity)> = physics
        .bodies_at_point(p)
        .into_iter()
        .filter_map(|entity| {
            let (shape, layers) = bodies.get(entity).ok()?;
            let is_ground = shape.contains_half_plane();
            let front_bit = layers.occupied_range().map_or(32, |(min, _)| min);
            Some((is_ground, front_bit, entity))
        })
        .collect();
    hits.sort_by_key(|(ground, bit, entity)| (*ground, *bit, entity.index()));
    hits.into_iter().map(|(_, _, e)| e).collect()
}

/// Hit-tests the topmost authored body at `p`.
pub fn topmost_body_at(
    p: Vec2,
    physics: &PhysicsQueries,
    bodies: &Query<(&ShapeDef, &LayerMask32), With<Body>>,
) -> Option<Entity> {
    bodies_at_sorted(p, physics, bodies).first().copied()
}

/// Default appearance for a newly authored body: a random pleasant hue
/// derived from its stable id (Algodoo behavior — every new body gets
/// its own color; deterministic per id, so undo/redo repaint the same).
pub fn appearance_for_id(id: crate::core::ids::StableId) -> crate::domain::appearance::Appearance {
    let hue = (id.0.as_u128() % 360) as f32;
    crate::domain::appearance::Appearance {
        fill: crate::domain::appearance::Rgba::from_hsl(hue, 0.65, 0.55),
    }
}

/// Builds a fresh body record with default props/layers at `pose`.
pub fn new_body_record(
    shape: ShapeDef,
    pos: Vec2,
    rot: f32,
) -> crate::command::snapshot::BodyRecord {
    let id = crate::core::ids::StableId::new();
    crate::command::snapshot::BodyRecord {
        id,
        pose: crate::core::units::PosRot { pos, rot },
        shape,
        props: crate::domain::props::PhysicalProps::default(),
        appearance: appearance_for_id(id),
        layers: LayerMask32::default(),
        group: None,
    }
}

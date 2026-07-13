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
pub mod click_select;
pub mod connector_tool;
pub mod context;
pub mod cut_tool;
pub mod drag_tool;
pub mod ground_tool;
pub mod handles;
pub mod polygon_tool;
pub mod select;
pub mod strut_tool;

use context::{draw_draft_preview, draw_manip_preview, run_draft_tool, run_manip_tool};

use crate::core::states::ToolState;
use crate::domain::Body;
use crate::domain::depth::DepthBand;
use crate::domain::shape::ShapeDef;
use crate::interaction::InteractionSet;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;

/// True while any tool gesture is in progress. The camera skips
/// right-drag panning while set (rotation gestures own the right button).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ActiveGesture(pub bool);

/// All tool gesture drivers run in this set. Click-through selection runs
/// after it, so it observes this frame's [`ActiveGesture`] and commit
/// intents when deciding whether a click fell through.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolDriverSet;

/// Installs every tool.
pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveGesture>();
        app.init_resource::<select::SelectGesture>();
        app.init_resource::<handles::ScaleFrame>();
        app.init_resource::<connector_tool::ConnectorDraft>();
        app.init_resource::<drag_tool::DragTool>();
        app.init_resource::<box_tool::BoxTool>();
        app.init_resource::<cut_tool::CutTool>();
        app.init_resource::<circle_tool::CircleTool>();
        app.init_resource::<ground_tool::GroundTool>();
        app.init_resource::<polygon_tool::PolygonTool>();
        app.init_resource::<strut_tool::StrutDraft>();

        app.add_systems(
            Update,
            (
                // Joint picking runs first: a joint click consumes the
                // press so the body select tool skips it this frame.
                crate::interaction::joint_edit::pick_joint
                    .run_if(in_state(ToolState::Select))
                    .before(run_manip_tool::<select::SelectGesture>),
                crate::interaction::joint_edit::drag_joint_anchor
                    .run_if(in_state(ToolState::Select)),
                crate::interaction::joint_edit::drag_joint_limit
                    .run_if(in_state(ToolState::Select)),
                run_manip_tool::<select::SelectGesture>.run_if(in_state(ToolState::Select)),
                run_manip_tool::<drag_tool::DragTool>.run_if(in_state(ToolState::Drag)),
                // Pure creation tools share one generic driver over the
                // DraftTool facade (see `context`).
                run_draft_tool::<box_tool::BoxTool>.run_if(in_state(ToolState::Box)),
                run_draft_tool::<cut_tool::CutTool>.run_if(in_state(ToolState::Cut)),
                run_draft_tool::<circle_tool::CircleTool>.run_if(in_state(ToolState::Circle)),
                run_draft_tool::<ground_tool::GroundTool>.run_if(in_state(ToolState::Ground)),
                run_draft_tool::<polygon_tool::PolygonTool>.run_if(in_state(ToolState::Polygon)),
                run_manip_tool::<connector_tool::ConnectorDraft>
                    .run_if(connector_tool::connector_active),
                run_manip_tool::<strut_tool::StrutDraft>.run_if(in_state(ToolState::Strut)),
            )
                .in_set(ToolDriverSet)
                .in_set(InteractionSet)
                .before(crate::command::CommandDispatchSet),
        );
        app.init_resource::<click_select::ClickThrough>();
        app.add_systems(
            Update,
            click_select::click_through_select
                .after(ToolDriverSet)
                .in_set(InteractionSet)
                .before(crate::command::CommandDispatchSet),
        );

        if app.is_plugin_added::<bevy::render::RenderPlugin>() {
            app.add_systems(
                Update,
                (
                    draw_manip_preview::<select::SelectGesture>.run_if(in_state(ToolState::Select)),
                    handles::draw_handles.run_if(in_state(ToolState::Select)),
                    draw_draft_preview::<box_tool::BoxTool>.run_if(in_state(ToolState::Box)),
                    draw_draft_preview::<cut_tool::CutTool>.run_if(in_state(ToolState::Cut)),
                    draw_draft_preview::<circle_tool::CircleTool>
                        .run_if(in_state(ToolState::Circle)),
                    draw_draft_preview::<ground_tool::GroundTool>
                        .run_if(in_state(ToolState::Ground)),
                    draw_draft_preview::<polygon_tool::PolygonTool>
                        .run_if(in_state(ToolState::Polygon)),
                    draw_manip_preview::<connector_tool::ConnectorDraft>
                        .run_if(connector_tool::connector_active),
                    draw_manip_preview::<strut_tool::StrutDraft>.run_if(in_state(ToolState::Strut)),
                ),
            );
        }
    }
}

/// All authored bodies at `p`, topmost first.
///
/// The shallowest band (front face nearest the viewer) wins; ground
/// half-planes always sort last (Algodoo behavior: you never
/// accidentally grab the floor).
pub fn bodies_at_sorted(
    p: Vec2,
    physics: &PhysicsQueries,
    bodies: &Query<(&ShapeDef, &DepthBand), With<Body>>,
) -> Vec<Entity> {
    let mut hits: Vec<(bool, f32, Entity)> = physics
        .bodies_at_point(p)
        .into_iter()
        .filter_map(|entity| {
            let (shape, band) = bodies.get(entity).ok()?;
            let is_ground = shape.contains_half_plane();
            Some((is_ground, band.sanitized().near, entity))
        })
        .collect();
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.index().cmp(&b.2.index()))
    });
    hits.into_iter().map(|(_, _, e)| e).collect()
}

/// Hit-tests the topmost authored body at `p`.
pub fn topmost_body_at(
    p: Vec2,
    physics: &PhysicsQueries,
    bodies: &Query<(&ShapeDef, &DepthBand), With<Body>>,
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
        ..Default::default()
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
        physics: crate::domain::props::BodyPhysics::default(),
        appearance: appearance_for_id(id),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
    }
}

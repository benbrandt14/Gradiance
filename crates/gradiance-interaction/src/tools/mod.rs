//! Editor tools: gesture state machines gated on [`ToolState`].
//!
//! The gesture contract (mechanically reviewed, see `CLAUDE.md`):
//! press → transient preview only (gizmos, kinematic-held `Transform`
//! writes) → release → **exactly one intent**, which dispatch turns into
//! one undoable command. Tools never touch authored components or the
//! command stack directly.
//!
//! Tools read the [`SnappedCursor`](crate::snap::SnappedCursor)
//! (never the raw cursor) and honor
//! [`GestureConstraints`](crate::gesture::GestureConstraints),
//! so snapping and axis/rotation constraints work identically everywhere.

pub mod array_tool;
pub mod box_tool;
pub mod chain_tool;
pub mod circle_tool;
pub mod click_select;
pub mod connector_tool;
pub mod context;
pub mod cut_tool;
pub mod drag_tool;
pub mod ground_tool;
pub mod handles;
pub mod move_align;
pub mod node_tools;
pub mod polygon_tool;
pub mod rod_tool;
pub mod script_tool;
pub mod select;
pub mod sketch_tool;
pub mod spring_tool;

use context::{draw_draft_preview, draw_manip_preview, run_draft_tool, run_manip_tool};

use crate::InteractionSet;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::states::ToolState;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_physics::queries::PhysicsQueries;

/// True while any tool gesture is in progress. The camera skips
/// right-drag panning while set (rotation gestures own the right button).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ActiveGesture(pub bool);

/// All tool gesture drivers run in this set. Click-through selection runs
/// after it, so it observes this frame's [`ActiveGesture`] and commits
/// when deciding whether a click fell through.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolDriverSet;

/// Installs every tool.
pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveGesture>();
        app.init_resource::<select::SelectGesture>();
        app.init_resource::<handles::ScaleFrame>();
        app.init_resource::<array_tool::ArrayConfig>();
        app.register_type::<array_tool::ArrayConfig>();
        app.init_resource::<connector_tool::ConnectorDraft>();
        app.init_resource::<drag_tool::DragTool>();
        app.init_resource::<box_tool::BoxTool>();
        app.init_resource::<cut_tool::CutTool>();
        app.init_resource::<circle_tool::CircleTool>();
        app.init_resource::<ground_tool::GroundTool>();
        app.init_resource::<polygon_tool::PolygonTool>();
        app.init_resource::<spring_tool::SpringDraft>();
        app.init_resource::<rod_tool::RodDraft>();
        app.init_resource::<chain_tool::ChainDraft>();
        app.init_resource::<sketch_tool::SketchDraft>();
        app.init_resource::<node_tools::TracerTool>();
        app.init_resource::<node_tools::ThrusterTool>();
        app.init_resource::<node_tools::LaserTool>();
        app.init_resource::<script_tool::ScriptTool>();
        app.init_resource::<gradiance_core::script_tools::ScriptTools>();
        app.add_message::<gradiance_core::script_tools::ScriptToolFired>();

        // A draft abandoned by a tool switch must not leave its reference
        // behind; the new tool's driver republishes its own this frame.
        app.add_systems(
            Update,
            clear_snap_reference
                .run_if(state_changed::<ToolState>)
                .before(ToolDriverSet)
                .in_set(InteractionSet),
        );
        app.add_systems(
            Update,
            (
                // Joint picking runs first: a joint click consumes the
                // press so the body select tool skips it this frame.
                crate::joint_edit::pick_joint
                    .run_if(in_state(ToolState::Select))
                    .before(run_manip_tool::<select::SelectGesture>),
                // Node picking runs after joint picking (joints keep
                // priority) and before the body select gesture.
                crate::node_edit::pick_node
                    .run_if(in_state(ToolState::Select))
                    .after(crate::joint_edit::pick_joint)
                    .before(run_manip_tool::<select::SelectGesture>),
                crate::joint_edit::drag_joint_anchor.run_if(in_state(ToolState::Select)),
                crate::joint_edit::drag_joint_limit.run_if(in_state(ToolState::Select)),
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
                run_manip_tool::<spring_tool::SpringDraft>.run_if(in_state(ToolState::Spring)),
                run_manip_tool::<rod_tool::RodDraft>.run_if(in_state(ToolState::Rod)),
                run_manip_tool::<chain_tool::ChainDraft>.run_if(in_state(ToolState::Chain)),
                run_manip_tool::<sketch_tool::SketchDraft>.run_if(in_state(ToolState::Sketch)),
                run_manip_tool::<node_tools::TracerTool>.run_if(in_state(ToolState::Tracer)),
                run_manip_tool::<node_tools::ThrusterTool>.run_if(in_state(ToolState::Thruster)),
                run_manip_tool::<node_tools::LaserTool>.run_if(in_state(ToolState::Laser)),
                // Script tools: a generic draft over the table the script
                // layer publishes. The VM runs once, after the last point.
                (
                    script_tool::sync_script_tool,
                    run_draft_tool::<script_tool::ScriptTool>,
                )
                    .chain()
                    .run_if(script_tool::script_tool_active),
            )
                .in_set(ToolDriverSet)
                .in_set(InteractionSet)
                .before(gradiance_command::CommandDispatchSet),
        );
        app.init_resource::<click_select::ClickThrough>();
        app.init_resource::<click_select::ToolCommitted>();
        app.add_systems(
            Update,
            click_select::click_through_select
                .after(ToolDriverSet)
                .in_set(InteractionSet)
                .before(gradiance_command::CommandDispatchSet),
        );

        add_tool_previews(app);
    }
}

/// The tools' gizmo previews (only with the render plugin present).
fn add_tool_previews(app: &mut App) {
    if app.is_plugin_added::<bevy::render::RenderPlugin>() {
        app.add_systems(
            Update,
            (
                draw_manip_preview::<select::SelectGesture>.run_if(in_state(ToolState::Select)),
                handles::draw_handles.run_if(in_state(ToolState::Select)),
                draw_draft_preview::<box_tool::BoxTool>.run_if(in_state(ToolState::Box)),
                draw_draft_preview::<cut_tool::CutTool>.run_if(in_state(ToolState::Cut)),
                draw_draft_preview::<circle_tool::CircleTool>.run_if(in_state(ToolState::Circle)),
                draw_draft_preview::<ground_tool::GroundTool>.run_if(in_state(ToolState::Ground)),
                draw_draft_preview::<polygon_tool::PolygonTool>
                    .run_if(in_state(ToolState::Polygon)),
                draw_manip_preview::<connector_tool::ConnectorDraft>
                    .run_if(connector_tool::connector_active),
                draw_manip_preview::<spring_tool::SpringDraft>.run_if(in_state(ToolState::Spring)),
                draw_manip_preview::<rod_tool::RodDraft>.run_if(in_state(ToolState::Rod)),
                draw_manip_preview::<chain_tool::ChainDraft>.run_if(in_state(ToolState::Chain)),
                draw_manip_preview::<sketch_tool::SketchDraft>.run_if(in_state(ToolState::Sketch)),
                draw_manip_preview::<node_tools::TracerTool>.run_if(in_state(ToolState::Tracer)),
                draw_manip_preview::<node_tools::ThrusterTool>
                    .run_if(in_state(ToolState::Thruster)),
                draw_draft_preview::<script_tool::ScriptTool>
                    .run_if(script_tool::script_tool_active),
            ),
        );
    }
}

/// Drops the snapper's draft reference (on a tool change).
fn clear_snap_reference(mut reference: ResMut<crate::snap::SnapReference>) {
    reference.0 = None;
}

/// The reads a hit-test sorts by: a body's shape (is it a ground?), its
/// depth band, and its id (the stacking order within one band).
pub type HitQuery<'w, 's> =
    Query<'w, 's, (&'static ShapeDef, &'static DepthBand, &'static StableId), With<Body>>;

/// All authored bodies at `p`, topmost first.
///
/// The shallowest band (front face nearest the viewer) wins; ground
/// half-planes always sort last (Algodoo behavior: you never
/// accidentally grab the floor). Within one band the **newest** body is on
/// top: ids are minted in creation order (`StableId::new`), and the
/// renderer stacks equal bands the same way (`render::stack`), so a click
/// picks the body you see. The tie used to fall to the entity index,
/// which a cut or an undo reshuffles, while the drawing order was the
/// GPU's: a body moved onto another picked the hidden one (brief J-2).
pub fn bodies_at_sorted(p: Vec2, physics: &PhysicsQueries, bodies: &HitQuery) -> Vec<Entity> {
    let mut hits: Vec<(bool, f32, StableId, Entity)> = physics
        .bodies_at_point(p)
        .into_iter()
        .filter_map(|entity| {
            let (shape, band, id) = bodies.get(entity).ok()?;
            let is_ground = shape.contains_half_plane();
            Some((is_ground, band.sanitized().near.value(), *id, entity))
        })
        .collect();
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(b.2.0.cmp(&a.2.0))
    });
    hits.into_iter().map(|(.., e)| e).collect()
}

/// Hit-tests the topmost authored body at `p`.
pub fn topmost_body_at(p: Vec2, physics: &PhysicsQueries, bodies: &HitQuery) -> Option<Entity> {
    bodies_at_sorted(p, physics, bodies).first().copied()
}

/// Builds a fresh body record with default props/layers at `pose`.
pub fn new_body_record(shape: ShapeDef, pos: Vec2, rot: f32) -> gradiance_scene::BodyRecord {
    let id = gradiance_core::ids::StableId::new();
    gradiance_scene::BodyRecord {
        id,
        pose: gradiance_core::units::PosRot { pos, rot },
        shape,
        physics: gradiance_domain::props::BodyPhysics::default(),
        appearance: gradiance_domain::appearance::Appearance::for_id(id),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        fluid: None,
    }
}

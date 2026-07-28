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

pub mod click_select;
pub mod connector_tool;
pub mod context;
pub mod cut_tool;
pub mod drag_tool;
pub mod ground_tool;
pub mod handles;
pub mod node_tools;
pub mod select;
pub mod sketch_session;
pub mod strut_tool;

use context::{draw_draft_preview, draw_manip_preview, run_draft_tool, run_manip_tool};

use crate::InteractionSet;
use bevy::prelude::*;
use gradiance_core::states::{GameState, ToolState};
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_physics::queries::PhysicsQueries;

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
        app.init_resource::<cut_tool::CutTool>();
        app.init_resource::<ground_tool::GroundTool>();
        app.init_resource::<strut_tool::StrutDraft>();
        app.init_resource::<node_tools::TracerTool>();
        app.init_resource::<sketch_session::SketchSession>();

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
                run_draft_tool::<cut_tool::CutTool>.run_if(in_state(ToolState::Cut)),
                run_draft_tool::<ground_tool::GroundTool>.run_if(in_state(ToolState::Ground)),
                run_manip_tool::<connector_tool::ConnectorDraft>
                    .run_if(connector_tool::connector_active),
                run_manip_tool::<strut_tool::StrutDraft>.run_if(in_state(ToolState::Strut)),
                run_manip_tool::<node_tools::TracerTool>.run_if(in_state(ToolState::Tracer)),
            )
                // Deliberately ungated: selecting, dragging and rotating are
                // how you interact with a *running* simulation, and gating
                // them on Paused would break play mode. Only the geometry
                // authoring below is paused-only.
                .in_set(ToolDriverSet)
                .in_set(InteractionSet)
                .before(gradiance_command::CommandDispatchSet),
        );
        app.add_systems(
            Update,
            (
                // One driver, not one per tool: the sketch is a single
                // document, so the session dispatches internally on the active
                // tool rather than swapping resources underneath it.
                sync_sketch_tool,
                // Before the session sees the click, so re-opening a body does
                // not also register as a selection gesture inside it.
                open_sketch_on_click,
                run_draft_tool::<sketch_session::SketchSession>,
            )
                .chain()
                .run_if(in_state(GameState::Paused))
                .in_set(ToolDriverSet)
                .in_set(InteractionSet)
                .before(gradiance_command::CommandDispatchSet),
        );
        app.init_resource::<click_select::ClickThrough>();
        app.add_systems(
            Update,
            click_select::click_through_select
                .after(ToolDriverSet)
                .in_set(InteractionSet)
                .before(gradiance_command::CommandDispatchSet),
        );

        if app.is_plugin_added::<bevy::render::RenderPlugin>() {
            app.add_systems(
                Update,
                (
                    draw_manip_preview::<select::SelectGesture>.run_if(in_state(ToolState::Select)),
                    handles::draw_handles.run_if(in_state(ToolState::Select)),
                    draw_draft_preview::<cut_tool::CutTool>.run_if(in_state(ToolState::Cut)),
                    draw_draft_preview::<ground_tool::GroundTool>
                        .run_if(in_state(ToolState::Ground)),
                    draw_manip_preview::<connector_tool::ConnectorDraft>
                        .run_if(connector_tool::connector_active),
                    draw_manip_preview::<strut_tool::StrutDraft>.run_if(in_state(ToolState::Strut)),
                    draw_manip_preview::<node_tools::TracerTool>
                        .run_if(in_state(ToolState::Tracer)),
                )
                    .run_if(in_state(GameState::Paused)),
            );
            app.add_systems(
                Update,
                (draw_draft_preview::<sketch_session::SketchSession>,)
                    .run_if(in_state(GameState::Paused)),
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
pub fn appearance_for_id(
    id: gradiance_core::ids::StableId,
) -> gradiance_domain::appearance::Appearance {
    let hue = (id.0.as_u128() % 360) as f32;
    gradiance_domain::appearance::Appearance {
        fill: gradiance_domain::appearance::Rgba::from_hsl(hue, 0.65, 0.55),
        ..Default::default()
    }
}

/// Builds a fresh body record with default props/layers at `pose`.
pub fn new_body_record(shape: ShapeDef, pos: Vec2, rot: f32) -> gradiance_scene::BodyRecord {
    let id = gradiance_core::ids::StableId::new();
    gradiance_scene::BodyRecord {
        id,
        pose: gradiance_core::units::PosRot { pos, rot },
        shape,
        physics: gradiance_domain::props::BodyPhysics::default(),
        appearance: appearance_for_id(id),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        sketch: None,
    }
}

/// Mirror the `ToolState` onto the session.
///
/// The session dispatches on its own copy so that `update` stays a pure
/// `&mut self` step with no ECS access, keeping the `DraftTool` seam intact.
fn sync_sketch_tool(
    tool: Res<State<ToolState>>,
    mut session: ResMut<sketch_session::SketchSession>,
) {
    session.set_tool(*tool.get());
}

/// Re-open a committed body's sketch by clicking it.
///
/// Without this a sketch is a one-shot recipe and the constraints it carries
/// are decoration — "make that edge 3 metres" has to be answerable for a body
/// that already exists, or the solver only ever pays for itself once.
///
/// Deliberately gated on an *empty* session: while a sketch is in progress a
/// click means "select" or "draw", and quietly swapping the document out from
/// under a half-drawn chain would be indefensible. Empty sketch plus the Select
/// tool is the one unambiguous moment where a click on a body can only mean
/// "edit this".
fn open_sketch_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    over_ui: Res<crate::PointerOverUi>,
    cursor: Res<crate::snap::SnappedCursor>,
    tool: Res<State<ToolState>>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &DepthBand), With<Body>>,
    sketched: Query<(
        &gradiance_core::ids::StableId,
        &gradiance_domain::sketch::SketchDoc,
        &Transform,
    )>,
    mut session: ResMut<sketch_session::SketchSession>,
) {
    if over_ui.0
        || *tool.get() != ToolState::Select
        || !session.is_empty()
        || !buttons.just_pressed(MouseButton::Left)
    {
        return;
    }
    let Some(p) = cursor.effective() else { return };

    for entity in bodies_at_sorted(p, &physics, &bodies) {
        if let Ok((id, doc, transform)) = sketched.get(entity) {
            session.open(*id, doc.clone(), transform.translation.truncate());
            return;
        }
    }
}

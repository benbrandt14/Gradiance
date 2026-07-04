//! Connector tools: hinge, weld, and slider joints.
//!
//! One shared state machine (a new joint kind = one match arm):
//! - **Hinge/Weld**: click at a point — the two topmost bodies there get
//!   connected; a single body gets pinned to the world. The anchor is the
//!   *snapped* cursor, so joints land exactly on vertices, centers, or
//!   grid points (CAD-style precise linkages).
//! - **Slider**: press at the anchor, drag to define the slide axis,
//!   release. A short drag defaults to the body's local X axis.

use crate::command::intent::SpawnJointIntent;
use crate::command::snapshot::JointRecord;
use crate::core::ids::StableId;
use crate::core::states::ToolState;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::joint::{JointCommon, JointDef, JointKind};
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::{ActiveGesture, bodies_at_sorted};
use crate::physics::queries::PhysicsQueries;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Below this drag length a slider uses the body's local X axis.
const AXIS_THRESHOLD: f32 = 5.0;

/// In-progress connector gesture (anchor point).
#[derive(Resource, Default, Debug)]
pub struct ConnectorDraft(pub Option<Vec2>);

/// The connector tool state machine (hinge/weld: click; slider: drag).
pub fn run_connector_tool(
    tool: Res<State<ToolState>>,
    buttons: Res<PointerButtons>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &LayerMask32), With<Body>>,
    poses: Query<&Transform, With<Body>>,
    ids: Query<&StableId>,
    mut draft: ResMut<ConnectorDraft>,
    mut active: ResMut<ActiveGesture>,
    mut spawn: MessageWriter<SpawnJointIntent>,
) {
    let tool = *tool.get();
    if !matches!(tool, ToolState::Hinge | ToolState::Weld | ToolState::Slider) {
        return;
    }

    if buttons.just_pressed(MouseButton::Left)
        && !over_ui.0
        && let Some(p) = snapped.effective()
    {
        draft.0 = Some(p);
        active.0 = true;
    }

    if buttons.just_released(MouseButton::Left)
        && let Some(anchor) = draft.0.take()
    {
        active.0 = false;
        let release = snapped.effective().unwrap_or(anchor);

        // The two topmost bodies at the anchor; one body → world pin.
        let hits = bodies_at_sorted(anchor, &physics, &bodies);
        let Some(&body_a) = hits.first() else {
            return;
        };
        let Ok(pose_a) = poses.get(body_a).map(PosRot::from_transform) else {
            return;
        };
        let Ok(&id_a) = ids.get(body_a) else {
            return;
        };
        let to_local =
            |pose: PosRot, world: Vec2| Vec2::from_angle(-pose.rot).rotate(world - pose.pos);

        let kind = match tool {
            ToolState::Weld => JointKind::Weld,
            ToolState::Slider => {
                let drag = release - anchor;
                let world_axis = if drag.length() < AXIS_THRESHOLD {
                    Vec2::from_angle(pose_a.rot) // body-A local X in world
                } else {
                    drag.normalize()
                };
                JointKind::Slider {
                    // Authored in body-A local space.
                    axis: Vec2::from_angle(-pose_a.rot).rotate(world_axis),
                    limits: None,
                    motor: None,
                }
            }
            _ => JointKind::Hinge {
                limits: None,
                motor: None,
            },
        };

        let mut def = JointDef {
            kind,
            common: JointCommon::default(),
            body_a: id_a,
            body_b: None,
            anchor_a: to_local(pose_a, anchor),
            anchor_b: anchor, // world point for the world-pin case
        };
        if let Some(&body_b) = hits.get(1) {
            let (Ok(&id_b), Ok(pose_b)) = (
                ids.get(body_b),
                poses.get(body_b).map(PosRot::from_transform),
            ) else {
                return;
            };
            def.body_b = Some(id_b);
            def.anchor_b = to_local(pose_b, anchor);
        }

        spawn.write(SpawnJointIntent {
            record: JointRecord {
                id: StableId::new(),
                def,
            },
        });
    }
}

/// Anchor dot; axis preview while dragging a slider.
pub fn draw_connector_preview(
    tool: Res<State<ToolState>>,
    draft: Res<ConnectorDraft>,
    snapped: Res<SnappedCursor>,
    mut gizmos: Gizmos,
) {
    let Some(anchor) = draft.0 else {
        return;
    };
    gizmos.circle_2d(Isometry2d::from_translation(anchor), 5.0, css::VIOLET);
    if *tool.get() == ToolState::Slider
        && let Some(p) = snapped.effective()
        && anchor.distance(p) > AXIS_THRESHOLD
    {
        let dir = (p - anchor).normalize();
        gizmos.line_2d(anchor - dir * 200.0, anchor + dir * 200.0, css::VIOLET);
    }
}

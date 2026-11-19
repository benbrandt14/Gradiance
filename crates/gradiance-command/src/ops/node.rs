//! Behavior-node operations: placeable nodes on a body (brief N-3).
//!
//! `node.thruster` places what the thruster tool places: a thruster node
//! attached to a body at a world point, pushing along a world angle. The
//! point and the angle are stored in the body's frame, as the tool stores
//! them, so the thruster rides and turns with its body.

use super::body_of;
use crate::intent::SpawnNodeIntent;
use crate::intent::name;
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::NodeRecord;
use gradiance_units::{Angle, Force, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Thruster>();
}

/// Attach a thruster to `body` at the world point (`x`, `y`), pushing with
/// `thrust` newtons along the world angle `angle` (radians from +x).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Thruster {
    /// The body the thruster pushes.
    pub body: Target,
    /// Point x (world).
    pub x: Length,
    /// Point y (world).
    pub y: Length,
    /// Push direction (world, radians from +x).
    pub angle: Angle,
    /// Push along the axis (N; negative pushes backwards).
    pub thrust: Force,
}

impl Operation for Thruster {
    const META: OpMeta = OpMeta {
        id: "node.thruster",
        lisp: "thruster",
        doc: "Attach a thruster to body at world point (x, y), pushing with thrust newtons along the world angle (radians); returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_NODE),
        example: r#"{"body": 0, "x": 0, "y": 0, "angle": 1.5708, "thrust": 10}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.body)? else {
            return Ok(OpOutput::none().returning(""));
        };
        let p = Vec2::new(self.x.0, self.y.0);
        let id = ctx.new_id();
        let record = NodeRecord {
            id,
            pose: PosRot { pos: p, rot: 0.0 },
            attachment: NodeAttachment::to(
                body.id,
                Vec2::from_angle(-body.rot).rotate(p - body.pos),
            ),
            kind: NodeKind::Thruster(gradiance_domain::thruster::Thruster {
                thrust: self.thrust,
                angle: Angle(self.angle.0 - body.rot),
            }),
            appearance: Appearance::for_id(id),
        };
        Ok(OpOutput::none()
            .message(SpawnNodeIntent { record })
            .returning(id.0.to_string()))
    }
}

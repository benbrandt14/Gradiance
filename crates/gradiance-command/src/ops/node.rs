//! Behavior-node operations: placeable nodes on a body (briefs N-3, N-8).
//!
//! `node.thruster` places what the thruster tool places: a thruster node
//! attached to a body at a world point, pushing along a world angle. The
//! point and the angle are stored in the body's frame, as the tool stores
//! them, so the thruster rides and turns with its body.
//!
//! `node.laser` places what the laser tool places: a laser on a body (or
//! free, when `body` names nothing) at a world point, aimed along a world
//! angle, with an optional target body and the bus name it publishes under.

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
    registry.register::<Laser>();
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

/// Place a laser at the world point (`x`, `y`) on `body` (free when `body`
/// names nothing, e.g. `-1`), aimed along the world angle `angle`, reading
/// `1` on the bus `name` while its beam reaches `target` (`-1` for none).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Laser {
    /// The body the laser rides (`-1`: a free laser).
    pub body: Target,
    /// Point x (world).
    pub x: Length,
    /// Point y (world).
    pub y: Length,
    /// Beam direction (world, radians from +x).
    pub angle: Angle,
    /// The body whose hit reads `1` (`-1`: no target).
    pub target: Target,
    /// The bus name of the hit signal; the distance is `<name>.distance`.
    pub name: String,
}

impl Operation for Laser {
    const META: OpMeta = OpMeta {
        id: "node.laser",
        lisp: "laser",
        doc: "Place a laser at world point (x, y) on body (-1: free), aimed along the world angle (radians); the bus value name reads 1 while its beam reaches target (-1: none), name.distance its path length; returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_NODE),
        example: r#"{"body": -1, "x": 0, "y": 2, "angle": -0.7854, "target": 0, "name": "gate"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let body = body_of(ctx, scene, &self.body)?;
        let target = body_of(ctx, scene, &self.target)?.map(|b| b.id);
        let name = self.name.trim();
        if name.is_empty() {
            return Err(OpError::Invalid("a laser needs a bus name".into()));
        }
        let p = Vec2::new(self.x.0, self.y.0);
        let (attachment, rot) = match body {
            Some(body) => (
                NodeAttachment::to(body.id, Vec2::from_angle(-body.rot).rotate(p - body.pos)),
                body.rot,
            ),
            None => (NodeAttachment::free(), 0.0),
        };
        let id = ctx.new_id();
        let record = NodeRecord {
            id,
            pose: PosRot { pos: p, rot: 0.0 },
            attachment,
            kind: NodeKind::Laser(gradiance_domain::laser::Laser {
                name: name.to_owned(),
                target,
                angle: Angle(self.angle.0 - rot),
                ..gradiance_domain::laser::Laser::default()
            }),
            appearance: Appearance::for_id(id),
        };
        Ok(OpOutput::none()
            .message(SpawnNodeIntent { record })
            .returning(id.0.to_string()))
    }
}

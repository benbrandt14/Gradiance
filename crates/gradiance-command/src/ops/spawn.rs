//! Operations that **author new geometry**: the spawn family and the cut.

use super::spawned_body_record;
use crate::intent::name;
use crate::intent::{CutIntent, SpawnBodyIntent};
use bevy::prelude::*;
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::shape::ShapeDef;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use gradiance_scene::BodyRecord;
use gradiance_units::{Angle, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<SpawnBox>();
    registry.register::<SpawnBoxRot>();
    registry.register::<SpawnCircle>();
    registry.register::<SpawnGround>();
    registry.register::<Cut>();
}

/// The output of every spawn: the intent, plus the new body's handle (its id)
/// as the value.
fn spawn(record: BodyRecord) -> OpOutput {
    let handle = record.id.0.to_string();
    OpOutput::none()
        .message(SpawnBodyIntent { record })
        .returning(handle)
}

/// Author a box body of size `w` x `h` centred at (`x`, `y`), in metres.
/// Returns the new body's handle (its id).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SpawnBox {
    /// Centre x.
    pub x: Length,
    /// Centre y.
    pub y: Length,
    /// Width.
    pub w: Length,
    /// Height.
    pub h: Length,
}

impl Operation for SpawnBox {
    const META: OpMeta = OpMeta {
        id: "body.spawn_box",
        lisp: "spawn-box",
        doc: "Author a box body of size w×h centred at (x, y), in metres; returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_BODY),
        example: r#"{"x": 0.5, "y": 2, "w": 1.6, "h": 1.2}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let shape = ShapeDef::Box {
            width: self.w.0,
            height: self.h.0,
        };
        Ok(spawn(spawned_body_record(
            ctx.new_id(),
            shape,
            Vec2::new(self.x.0, self.y.0),
        )))
    }
}

/// `spawn-box`, rotated by `angle`. A separate verb because `place` indexes
/// the pre-run snapshot, so a script run cannot rotate a box it has just
/// spawned (one snapshot per run).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SpawnBoxRot {
    /// Centre x.
    pub x: Length,
    /// Centre y.
    pub y: Length,
    /// Width.
    pub w: Length,
    /// Height.
    pub h: Length,
    /// Rotation.
    pub angle: Angle,
}

impl Operation for SpawnBoxRot {
    const META: OpMeta = OpMeta {
        id: "body.spawn_box_rot",
        lisp: "spawn-box-rot",
        doc: "Author a w×h box centred at (x, y), in metres, rotated by angle radians; returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_BODY),
        example: r#"{"x": 2, "y": 3, "w": 1, "h": 0.5, "angle": 0.3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let shape = ShapeDef::Box {
            width: self.w.0,
            height: self.h.0,
        };
        let mut record = spawned_body_record(ctx.new_id(), shape, Vec2::new(self.x.0, self.y.0));
        record.pose.rot = self.angle.0;
        Ok(spawn(record))
    }
}

/// Author a circle body of radius `r` centred at (`x`, `y`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SpawnCircle {
    /// Centre x.
    pub x: Length,
    /// Centre y.
    pub y: Length,
    /// Radius.
    pub r: Length,
}

impl Operation for SpawnCircle {
    const META: OpMeta = OpMeta {
        id: "body.spawn_circle",
        lisp: "spawn-circle",
        doc: "Author a circle body of radius r centred at (x, y), in metres; returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_BODY),
        example: r#"{"x": 0, "y": 3, "r": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let shape = ShapeDef::Circle { radius: self.r.0 };
        Ok(spawn(spawned_body_record(
            ctx.new_id(),
            shape,
            Vec2::new(self.x.0, self.y.0),
        )))
    }
}

/// Author a fixed ground half-plane through (`x`, `y`), tilted by `angle`.
/// Static physics so it never falls; the surface passes through the body
/// origin (see [`ShapeDef::HalfPlane`]).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SpawnGround {
    /// A point on the surface, x.
    pub x: Length,
    /// A point on the surface, y.
    pub y: Length,
    /// Tilt.
    pub angle: Angle,
}

impl Operation for SpawnGround {
    const META: OpMeta = OpMeta {
        id: "body.spawn_ground",
        lisp: "spawn-ground",
        doc: "Author a fixed ground half-plane through (x, y) (metres), tilted by angle radians.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_BODY),
        example: r#"{"x": 0, "y": -2, "angle": 0.1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mut record = spawned_body_record(
            ctx.new_id(),
            ShapeDef::HalfPlane,
            Vec2::new(self.x.0, self.y.0),
        );
        record.pose.rot = self.angle.0;
        record.physics = BodyPhysics::fixed();
        Ok(spawn(record))
    }
}

/// Sever every body crossed by the stroke from (`ax`, `ay`) to (`bx`, `by`)
/// of the given `width`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Cut {
    /// Stroke start x.
    pub ax: Length,
    /// Stroke start y.
    pub ay: Length,
    /// Stroke end x.
    pub bx: Length,
    /// Stroke end y.
    pub by: Length,
    /// Stroke width.
    pub width: Length,
}

impl Operation for Cut {
    const META: OpMeta = OpMeta {
        id: "body.cut",
        lisp: "cut",
        doc: "Sever every body crossed by the stroke a→b of the given width (metres).",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::CUT),
        example: r#"{"ax": -2, "ay": 1, "bx": 2, "by": 1, "width": 0.05}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(CutIntent {
            a: Vec2::new(self.ax.0, self.ay.0),
            b: Vec2::new(self.bx.0, self.by.0),
            width: self.width.0,
        }))
    }
}

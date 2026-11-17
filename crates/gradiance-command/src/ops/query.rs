//! Read-only **queries** over the committed scene and the simulation
//! settings. Each returns one number through [`OpOutput::returning`] and
//! writes nothing (reads are total).
//!
//! A read of something that is not there (a target that names no body, a
//! settings path that does not resolve) returns NaN, which JSON carries as
//! `null`. The Lisp doorway turns a query's `null` back into NaN, so
//! `(body-x 99)` still reads NaN, as `docs/scripting.md` documents: zero
//! would read as a real measurement.

use super::body_of;
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance_domain::settings::SimSettings;
use gradiance_op::reflect_path::read_path;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_units::Length;
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<BodyX>();
    registry.register::<BodyY>();
    registry.register::<BodyRot>();
    registry.register::<BodyFriction>();
    registry.register::<BodyRestitution>();
    registry.register::<BodyDensity>();
    registry.register::<BodyStatic>();
    registry.register::<TouchCount>();
    registry.register::<CountAt>();
    registry.register::<NearestAt>();
    registry.register::<NearestDist>();
    registry.register::<IndexAt>();
    registry.register::<JointCount>();
    registry.register::<SimGet>();
}

/// One number read off the body `target` names; NaN when it names none.
fn body_read(
    ctx: &OpContext,
    target: &Target,
    read: impl FnOnce(&BodySnapshot) -> f64,
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let value = body_of(ctx, scene, target)?.map_or(f64::NAN, read);
    Ok(OpOutput::none().returning(value))
}

/// One number read off the whole scene.
fn scene_read(
    ctx: &OpContext,
    read: impl FnOnce(&SceneSnapshot) -> f64,
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    Ok(OpOutput::none().returning(read(scene)))
}

/// The metadata every query shares: no menu, no chord, no undo step.
const fn query(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example,
    }
}

/// X of a body.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyX {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyX {
    const META: OpMeta = query(
        "body.x",
        "body-x",
        "X of the i-th body (bodies ordered by id); NaN if out of range.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| f64::from(b.pos.x))
    }
}

/// Y of a body.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyY {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyY {
    const META: OpMeta = query(
        "body.y",
        "body-y",
        "Y of the i-th body; NaN if out of range.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| f64::from(b.pos.y))
    }
}

/// Rotation of a body.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyRot {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyRot {
    const META: OpMeta = query(
        "body.rot",
        "body-rot",
        "Rotation (radians) of the i-th body; NaN if out of range.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| f64::from(b.rot))
    }
}

/// Friction of a body: the read half of `set-friction`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyFriction {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyFriction {
    const META: OpMeta = query(
        "body.friction",
        "body-friction",
        "Coulomb friction of the i-th body.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| {
            f64::from(b.physics.friction.dynamic_coefficient)
        })
    }
}

/// Restitution of a body: the read half of `set-restitution`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyRestitution {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyRestitution {
    const META: OpMeta = query(
        "body.restitution",
        "body-restitution",
        "Bounciness of the i-th body.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| {
            f64::from(b.physics.restitution.coefficient)
        })
    }
}

/// Density of a body: the read half of `set-density`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyDensity {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyDensity {
    const META: OpMeta = query(
        "body.density",
        "body-density",
        "Mass density of the i-th body.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| f64::from(b.physics.density.0))
    }
}

/// Whether a body is static: the read half of `set-static`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyStatic {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for BodyStatic {
    const META: OpMeta = query(
        "body.is_static",
        "body-static?",
        "Whether the i-th body is static (1) or not (0).",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| {
            f64::from(u8::from(b.physics.rigid_body == RigidBody::Static))
        })
    }
}

/// How many bodies a body touches.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TouchCount {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
}

impl Operation for TouchCount {
    const META: OpMeta = query(
        "body.touch_count",
        "touch-count",
        "How many bodies the i-th body is currently touching; NaN if out of range.",
        r#"{"i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        body_read(ctx, &self.i, |b| b.touching as f64)
    }
}

/// How many bodies contain a point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CountAt {
    /// World x.
    pub x: Length,
    /// World y.
    pub y: Length,
}

impl Operation for CountAt {
    const META: OpMeta = query(
        "body.count_at",
        "count-at",
        "Number of bodies whose shape contains the world point (x, y).",
        r#"{"x": 0, "y": 1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        scene_read(ctx, |s| s.count_at(Vec2::new(self.x.0, self.y.0)))
    }
}

/// The body nearest a point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NearestAt {
    /// World x.
    pub x: Length,
    /// World y.
    pub y: Length,
}

impl Operation for NearestAt {
    const META: OpMeta = query(
        "body.nearest_at",
        "nearest-at",
        "Index of the body whose centre is nearest (x, y); -1 if none.",
        r#"{"x": 0, "y": 1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        scene_read(ctx, |s| s.nearest_at(Vec2::new(self.x.0, self.y.0)))
    }
}

/// The distance to the body nearest a point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NearestDist {
    /// World x.
    pub x: Length,
    /// World y.
    pub y: Length,
}

impl Operation for NearestDist {
    const META: OpMeta = query(
        "body.nearest_dist",
        "nearest-dist",
        "Distance from (x, y) to the nearest body centre; -1 if the scene is empty.",
        r#"{"x": 0, "y": 1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        scene_read(ctx, |s| s.nearest_dist(Vec2::new(self.x.0, self.y.0)))
    }
}

/// A body under a point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct IndexAt {
    /// World x.
    pub x: Length,
    /// World y.
    pub y: Length,
}

impl Operation for IndexAt {
    const META: OpMeta = query(
        "body.index_at",
        "body-index-at",
        "Index of a body whose shape contains (x, y) (first in id order); -1 if none.",
        r#"{"x": 0, "y": 1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        scene_read(ctx, |s| s.index_at(Vec2::new(self.x.0, self.y.0)))
    }
}

/// How many joints the committed scene holds.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct JointCount {}

impl Operation for JointCount {
    const META: OpMeta = query(
        "joint.count",
        "joint-count",
        "Number of authored joints in the committed scene.",
        "{}",
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        scene_read(ctx, SceneSnapshot::joint_count)
    }
}

/// Read one simulation setting by reflect-path, from the context's settings
/// mirror: one run reads one settings snapshot.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimGet {
    /// The reflect-path of a scalar in `SimSettings`.
    pub path: String,
}

impl Operation for SimGet {
    const META: OpMeta = query(
        "sim.get",
        "sim-get",
        "Read a simulation setting by reflect-path, e.g. \"gravity.y\", \"speed\"; NaN if the path does not resolve.",
        r#"{"path": "speed"}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<SimSettings>()?;
        let value = read_path(settings, &self.path).unwrap_or(f64::NAN);
        Ok(OpOutput::none().returning(value))
    }
}

//! Water operations (brief S-11): liquify a body, add water in a region,
//! drain all water. Each is one undo step. The particles are derived
//! (`physics::particles`): an emitter fills when the run plays, and undoing
//! the run empties it.

use super::bodies_of;
use crate::intent::name;
use crate::intent::{DeleteIntent, LiquifyIntent, SpawnNodeIntent};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_domain::water::WaterEmitter;
use gradiance_geometry::contours::ring_signed_area;
use gradiance_geometry::polygonize::polygonize;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::NodeRecord;
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Liquify>();
    registry.register::<AddWater>();
    registry.register::<Drain>();
}

/// A free water emitter of the world outline `ring`, its node at `at`.
fn emitter(ctx: &OpContext, at: Vec2, ring: Vec<Vec2>) -> NodeRecord {
    let id = ctx.new_id();
    NodeRecord {
        id,
        pose: PosRot { pos: at, rot: 0.0 },
        attachment: NodeAttachment::free(),
        kind: NodeKind::Water(WaterEmitter::new(
            ring.into_iter().map(|p| p - at).collect(),
        )),
        appearance: Appearance::for_id(id),
    }
}

/// Turn the body `i` (`"selection"`: every selected body) into water of the
/// same area: the body goes, a water emitter of its outline takes its
/// place, in one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Liquify {
    /// The body.
    pub i: Target,
}

impl Operation for Liquify {
    const META: OpMeta = OpMeta {
        id: "body.liquify",
        lisp: "liquify",
        doc: "Turn the i-th body into particle water of the same area (grounds and fluid regions stay): it fills when the sim plays — one undo step.",
        seam: Seam::Edit,
        menu: Some("Context/Liquify"),
        chord: None,
        undo: Some(name::LIQUIFY),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let mut targets = Vec::new();
        let mut emitters = Vec::new();
        for id in bodies_of(ctx, scene, &self.i)? {
            let Some(body) = scene.get(id) else {
                continue;
            };
            if body.shape.contains_half_plane() || body.fluid.is_some() {
                continue;
            }
            let turn = Vec2::from_angle(body.rot);
            let contours = polygonize(&body.shape);
            // The outer rings (counter-clockwise); a hole fills too.
            for ring in contours.rings().filter(|r| ring_signed_area(r) > 0.0) {
                let world = ring.iter().map(|v| body.pos + turn.rotate(*v)).collect();
                emitters.push(emitter(ctx, body.pos, world));
            }
            targets.push(id);
        }
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(LiquifyIntent { targets, emitters }))
    }
}

/// Add water filling a region (brief S-11): a water emitter of the closed
/// outline `path`, the water tool's commit.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AddWater {
    /// The outline as a flat list `x0 y0 x1 y1 …` in metres, at least three
    /// points, either winding.
    pub path: Vec<f32>,
}

impl Operation for AddWater {
    const META: OpMeta = OpMeta {
        id: "water.fill",
        lisp: "add-water",
        doc: "Add particle water filling the closed outline path (x0 y0 x1 y1 …, metres, three points or more): it fills when the sim plays — undoable, saved. Returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_NODE),
        example: r#"{"path": [-1, 0, 1, 0, 1, 1, -1, 1]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mut ring: Vec<Vec2> = self
            .path
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| Vec2::new(x, y))
            .collect();
        if !self.path.len().is_multiple_of(2)
            || ring.len() < 3
            || ring.iter().any(|p| !p.is_finite())
        {
            return Err(OpError::Invalid(
                "add-water takes three finite points or more".into(),
            ));
        }
        if ring_signed_area(&ring) < 0.0 {
            ring.reverse();
        }
        if ring_signed_area(&ring) <= 0.0 {
            return Err(OpError::Invalid("the water outline has no area".into()));
        }
        let at = ring.iter().copied().sum::<Vec2>() / ring.len() as f32;
        let record = emitter(ctx, at, ring);
        let handle = record.id.0.to_string();
        Ok(OpOutput::none()
            .message(SpawnNodeIntent { record })
            .returning(handle))
    }
}

/// Remove every water emitter (and so all water), in one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Drain {}

impl Operation for Drain {
    const META: OpMeta = OpMeta {
        id: "water.drain",
        lisp: "drain",
        doc: "Remove all particle water (every water emitter) — one undo step.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::DELETE),
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let targets: Vec<_> = scene.water.clone();
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(DeleteIntent { targets }))
    }
}

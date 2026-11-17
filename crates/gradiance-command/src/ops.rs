//! The scene operations this layer owns, declared once for every doorway
//! (`gradiance-op`; plan `docs/unified-surface-plan.md` § 3, § 13).
//!
//! Each one is a parameter struct that lowers to the intent (or value, or
//! settings write) its seam already takes. Doorways reach them by id through
//! the [`OpRegistry`]; none names these types.

use crate::CommandDispatchSet;
use crate::intent::name;
use crate::intent::{CommitTransformIntent, SpawnBodyIntent, TransformChange, UndoIntent};
use crate::snapshot::{SceneSnapshot, snapshot_scene};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::settings::SimSettings;
use gradiance_domain::shape::ShapeDef;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpInvokeSet, OpMeta, OpOutput, OpPlugin, OpRegistry,
    Operation, ResolveTarget, Seam, Target,
};
use gradiance_scene::BodyRecord;
use gradiance_units::{Angle, Length};
use serde::Deserialize;

/// A body of `shape` at `pos`, styled like a tool-drawn one: the record every
/// scripted or remote spawn authors. The colour derives from `id`, so a
/// scripted scene and a hand-built one look the same.
pub fn spawned_body_record(id: StableId, shape: ShapeDef, pos: Vec2) -> BodyRecord {
    BodyRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        shape,
        physics: BodyPhysics::default(),
        appearance: Appearance::for_id(id),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
    }
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
        let id = ctx.new_id();
        let shape = ShapeDef::Box {
            width: self.w.0,
            height: self.h.0,
        };
        let record = spawned_body_record(id, shape, Vec2::new(self.x.0, self.y.0));
        Ok(OpOutput::none()
            .message(SpawnBodyIntent { record })
            .returning(id.0.to_string()))
    }
}

/// Move the body `i` to (`x`, `y`) and rotate it to `angle`: one undo step.
/// A target that names no body, or a move to where the body already is, does
/// nothing (no empty undo step).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Place {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
    /// New centre x.
    pub x: Length,
    /// New centre y.
    pub y: Length,
    /// New rotation.
    pub angle: Angle,
}

impl Operation for Place {
    const META: OpMeta = OpMeta {
        id: "body.place",
        lisp: "place",
        doc: "Move the i-th body to (x, y) and rotate it to `angle` radians — one undo step.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::COMMIT_TRANSFORM),
        example: r#"{"i": 0, "x": 1.5, "y": 0.25, "angle": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(id) = scene.resolve(&self.i) else {
            return Ok(OpOutput::none());
        };
        let Some(old) = scene.bodies.iter().find(|b| b.id == id).map(|b| PosRot {
            pos: b.pos,
            rot: b.rot,
        }) else {
            return Ok(OpOutput::none());
        };
        let new = PosRot {
            pos: Vec2::new(self.x.0, self.y.0),
            rot: self.angle.0,
        };
        if new == old {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(CommitTransformIntent {
            changes: vec![TransformChange { id, old, new }],
        }))
    }
}

/// Undo the last command.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Undo {}

impl Operation for Undo {
    const META: OpMeta = OpMeta {
        id: "edit.undo",
        lisp: "undo",
        doc: "Undo the last command — the same step Edit ▸ Undo takes.",
        seam: Seam::Edit,
        menu: Some("Edit/Undo"),
        chord: Some("Ctrl+Z"),
        // Undo removes a step; it records none.
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(UndoIntent))
    }
}

/// How many authored bodies the committed scene holds.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyCount {}

impl Operation for BodyCount {
    const META: OpMeta = OpMeta {
        id: "body.count",
        lisp: "body-count",
        doc: "Number of authored bodies in the committed scene.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        Ok(OpOutput::none().returning(scene.count()))
    }
}

/// Set one simulation setting by reflect-path (`"gravity.y"`, `"speed"`).
/// Scene content: a settled edit is one undo step through
/// `commit_settings_edits`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimSet {
    /// The reflect-path of a scalar in `SimSettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for SimSet {
    const META: OpMeta = OpMeta {
        id: "sim.set",
        lisp: "sim-set",
        doc: "Set a simulation setting by reflect-path (config seam; a settled edit is one undo step).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"path": "speed", "value": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<SimSettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}

/// Publishes this layer's mirrors into a new operation context: the scene
/// snapshot and the simulation settings.
fn fill_context(world: &mut World, ctx: &mut OpContext) {
    ctx.insert(snapshot_scene(world));
    if let Some(settings) = world.get_resource::<SimSettings>() {
        ctx.insert(settings.clone());
    }
}

/// Registers this layer's operations and its context mirrors.
#[derive(Default)]
pub struct SceneOpsPlugin;

impl Plugin for SceneOpsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OpPlugin>() {
            app.add_plugins(OpPlugin);
        }
        {
            let mut registry = app.world_mut().resource_mut::<OpRegistry>();
            registry.register::<SpawnBox>();
            registry.register::<Place>();
            registry.register::<Undo>();
            registry.register::<BodyCount>();
            registry.register::<SimSet>();
        }
        app.world_mut()
            .resource_mut::<OpContextFillers>()
            .add(fill_context);
        // An invoked edit becomes a command in the same frame.
        app.configure_sets(Update, OpInvokeSet.before(CommandDispatchSet));
    }
}

/// Adds [`SceneOpsPlugin`] once. The command and script layers both call it,
/// so an app with either one has the scene operations.
pub fn install(app: &mut App) {
    if !app.is_plugin_added::<SceneOpsPlugin>() {
        app.add_plugins(SceneOpsPlugin);
    }
}

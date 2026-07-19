//! Snapshots of authored entities — the shared unit of undo and persistence.

use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::depth::DepthBand;
use crate::domain::group::SelectionGroup;
use crate::domain::layers::LayerMask32;
use crate::domain::props::BodyPhysics;
use crate::domain::shape::ShapeDef;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A complete authored-state snapshot of one body.
///
/// Captures exactly the components that constitute the save file; spawning
/// a record recreates the body and every derived component follows via the
/// sync systems. Used by undo records and by scene files alike.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct BodyRecord {
    /// Stable identity (preserved across undo/redo and save/load).
    pub id: StableId,
    /// Authored pose.
    pub pose: PosRot,
    /// Authored geometry.
    pub shape: ShapeDef,
    /// Authored physics (avian components, grouped for the record).
    #[serde(default)]
    pub physics: BodyPhysics,
    /// Authored appearance.
    pub appearance: Appearance,
    /// Authored depth band (extrusion *and* collision volume, v5+).
    #[serde(default)]
    pub depth: DepthBand,
    /// Legacy v4 layer mask — parsed only so `persist` can migrate old
    /// files; never written (`None` after capture and after migration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<LayerMask32>,
    /// Group stack, innermost first (empty = ungrouped).
    #[serde(default)]
    pub groups: Vec<u32>,
    /// Field source (attraction/repulsion), if the body carries one.
    #[serde(default)]
    pub field: Option<crate::domain::field::FieldSource>,
    /// Trajectory-trail marker, if the body carries one.
    #[serde(default)]
    pub tracer: Option<crate::domain::tracer::Tracer>,
    /// Rigid-rod marker (a capsule body authored by the strut tool), if
    /// the body is one (v6+).
    #[serde(default)]
    pub rod: Option<crate::domain::rod::Rod>,
    /// Flexure tip-body marker (v6+).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rod_tip: bool,
}

impl BodyRecord {
    /// Captures the authored state of `entity`, or `None` if it is not a
    /// complete authored body.
    pub fn capture(world: &World, entity: Entity) -> Option<Self> {
        let entity_ref = world.get_entity(entity).ok()?;
        Some(Self {
            id: *entity_ref.get::<StableId>()?,
            pose: PosRot::from_transform(entity_ref.get::<Transform>()?),
            shape: entity_ref.get::<ShapeDef>()?.clone(),
            physics: BodyPhysics::capture(&entity_ref),
            appearance: *entity_ref.get::<Appearance>()?,
            depth: *entity_ref.get::<DepthBand>()?,
            layers: None,
            groups: entity_ref
                .get::<SelectionGroup>()
                .map(|g| g.0.clone())
                .unwrap_or_default(),
            field: entity_ref
                .get::<crate::domain::field::FieldSource>()
                .copied(),
            tracer: entity_ref.get::<crate::domain::tracer::Tracer>().copied(),
            rod: entity_ref.get::<crate::domain::rod::Rod>().copied(),
            rod_tip: entity_ref.get::<crate::domain::rod::RodTip>().is_some(),
        })
    }

    /// Spawns a body with exactly this authored state.
    pub fn spawn(&self, world: &mut World) -> Entity {
        let mut transform = Transform::default();
        self.pose.apply_to(&mut transform);
        let mut entity = world.spawn((
            Body,
            self.id,
            transform,
            self.shape.clone(),
            self.appearance,
            self.depth.sanitized(),
        ));
        self.physics.insert_into(&mut entity);
        if !self.groups.is_empty() {
            entity.insert(SelectionGroup(self.groups.clone()));
        }
        if let Some(field) = self.field {
            entity.insert(field);
        }
        if let Some(tracer) = self.tracer {
            entity.insert(tracer);
        }
        if let Some(rod) = self.rod {
            entity.insert(rod);
        }
        if self.rod_tip {
            entity.insert(crate::domain::rod::RodTip);
        }
        entity.id()
    }
}

/// A complete authored-state snapshot of one behavior node (a placeable
/// dataflow entity — see [`domain::node`](crate::domain::node)).
///
/// The node analogue of [`BodyRecord`]: shared by undo records,
/// duplicate cloning, and scene files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NodeRecord {
    /// Stable identity.
    pub id: StableId,
    /// Authored pose (a free node's fixed pose; an attached node's is
    /// re-derived each frame from its target).
    pub pose: PosRot,
    /// Optional attachment to a body.
    pub attachment: crate::domain::node::NodeAttachment,
    /// The node's payload (kind).
    pub kind: crate::domain::node::NodeKind,
    /// Appearance (glyph / trail color).
    pub appearance: Appearance,
}

impl NodeRecord {
    /// Captures the authored state of a behavior-node entity.
    pub fn capture(world: &World, entity: Entity) -> Option<Self> {
        let entity_ref = world.get_entity(entity).ok()?;
        Some(Self {
            id: *entity_ref.get::<StableId>()?,
            pose: PosRot::from_transform(entity_ref.get::<Transform>()?),
            attachment: *entity_ref.get::<crate::domain::node::NodeAttachment>()?,
            kind: entity_ref.get::<crate::domain::node::NodeKind>()?.clone(),
            appearance: *entity_ref.get::<Appearance>()?,
        })
    }

    /// Spawns a behavior-node entity with exactly this authored state.
    /// The `Tracer` kind also inserts the `Tracer` component so the trail
    /// sampler (`render::tracer`) drives it uniformly with body tracers.
    pub fn spawn(&self, world: &mut World) -> Entity {
        let mut transform = Transform::default();
        self.pose.apply_to(&mut transform);
        let mut entity = world.spawn((
            crate::domain::node::BehaviorNode,
            self.id,
            transform,
            self.attachment,
            self.kind.clone(),
            self.appearance,
        ));
        // The tracer kind also gets a `Tracer` component so the trail sampler
        // drives it uniformly with body tracers.
        let crate::domain::node::NodeKind::Tracer(tracer) = self.kind;
        entity.insert(tracer);
        entity.id()
    }
}

/// A complete authored-state snapshot of one joint.
///
/// The joint analogue of [`BodyRecord`]: shared by undo records,
/// duplicate/array cloning, and scene files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct JointRecord {
    /// Stable identity.
    pub id: StableId,
    /// The authored joint definition.
    pub def: crate::domain::joint::JointDef,
}

impl JointRecord {
    /// Captures the authored state of a joint entity.
    pub fn capture(world: &World, entity: Entity) -> Option<Self> {
        let entity_ref = world.get_entity(entity).ok()?;
        Some(Self {
            id: *entity_ref.get::<StableId>()?,
            def: entity_ref.get::<crate::domain::joint::JointDef>()?.clone(),
        })
    }

    /// Spawns a joint entity with exactly this authored state.
    ///
    /// The joint entity carries an identity `Transform` so derived pin
    /// anchors can live as its children.
    pub fn spawn(&self, world: &mut World) -> Entity {
        world
            .spawn((
                crate::domain::Joint,
                self.id,
                self.def.clone(),
                Transform::default(),
            ))
            .id()
    }
}

/// Persisted editor environment (settings that travel with the scene).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct EnvironmentRecord {
    /// Simulation tuning.
    pub sim: crate::domain::settings::SimSettings,
    /// Grid configuration.
    pub grid: crate::domain::settings::GridSettings,
    /// Snap configuration.
    pub snap: crate::domain::settings::SnapConfig,
    /// Rendering style (defaulted when absent — pre-M10 files).
    #[serde(default)]
    pub render: crate::domain::settings::RenderSettings,
    /// Scene lighting (defaulted when absent — pre-V1 files).
    #[serde(default)]
    pub lighting: crate::domain::settings::LightingSettings,
    /// Back plane / ground scenery (defaulted when absent — pre-V1 files).
    #[serde(default)]
    pub scenery: crate::domain::settings::ScenerySettings,
    /// Signal-dataflow bindings (defaulted when absent — pre-signal files).
    #[serde(default)]
    pub signals: crate::signal::SignalBindings,
    /// Signal parameters (`defparam` knobs; defaulted for pre-P2 files).
    #[serde(default)]
    pub params: crate::signal::SignalParams,
    /// Computed signals (`defsignal` modulators; defaulted for pre-P2 files).
    #[serde(default)]
    pub computed: crate::signal::ComputedSignals,
}

/// A complete scene: the save file, and the unit of whole-world undo.
///
/// Bodies and joints are sorted by [`StableId`] so capture is
/// deterministic — saving the same world twice yields identical bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneRecord {
    /// Format version (see `persist::FORMAT_VERSION`).
    pub version: u32,
    /// `CARGO_PKG_VERSION` of the app that wrote the file (repro aid).
    pub app_version: String,
    /// All authored bodies.
    pub bodies: Vec<BodyRecord>,
    /// All authored joints.
    pub joints: Vec<JointRecord>,
    /// All authored behavior nodes (tracers, future sensors/actuators).
    #[serde(default)]
    pub nodes: Vec<NodeRecord>,
    /// Scene-level settings.
    pub environment: EnvironmentRecord,
}

impl SceneRecord {
    /// Captures the entire authored world, deterministically ordered.
    pub fn capture(world: &mut World) -> Self {
        let mut bodies: Vec<BodyRecord> = {
            let mut query = world.query_filtered::<Entity, With<Body>>();
            let entities: Vec<Entity> = query.iter(world).collect();
            entities
                .into_iter()
                .filter_map(|e| BodyRecord::capture(world, e))
                .collect()
        };
        bodies.sort_by_key(|r| r.id.0);
        let mut joints: Vec<JointRecord> = {
            let mut query = world.query_filtered::<Entity, With<crate::domain::Joint>>();
            let entities: Vec<Entity> = query.iter(world).collect();
            entities
                .into_iter()
                .filter_map(|e| JointRecord::capture(world, e))
                .collect()
        };
        joints.sort_by_key(|r| r.id.0);
        let mut nodes: Vec<NodeRecord> = {
            let mut query =
                world.query_filtered::<Entity, With<crate::domain::node::BehaviorNode>>();
            let entities: Vec<Entity> = query.iter(world).collect();
            entities
                .into_iter()
                .filter_map(|e| NodeRecord::capture(world, e))
                .collect()
        };
        nodes.sort_by_key(|r| r.id.0);
        Self {
            // Keep in sync with persist::FORMAT_VERSION (v2: avian-component
            // physics — see docs/physics-deadapter-decision.md).
            version: crate::persist::FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            bodies,
            joints,
            nodes,
            environment: EnvironmentRecord {
                sim: world
                    .get_resource::<crate::domain::settings::SimSettings>()
                    .cloned()
                    .unwrap_or_default(),
                grid: world
                    .get_resource::<crate::domain::settings::GridSettings>()
                    .cloned()
                    .unwrap_or_default(),
                snap: world
                    .get_resource::<crate::domain::settings::SnapConfig>()
                    .cloned()
                    .unwrap_or_default(),
                render: world
                    .get_resource::<crate::domain::settings::RenderSettings>()
                    .cloned()
                    .unwrap_or_default(),
                lighting: world
                    .get_resource::<crate::domain::settings::LightingSettings>()
                    .cloned()
                    .unwrap_or_default(),
                scenery: world
                    .get_resource::<crate::domain::settings::ScenerySettings>()
                    .cloned()
                    .unwrap_or_default(),
                signals: world
                    .get_resource::<crate::signal::SignalBindings>()
                    .cloned()
                    .unwrap_or_default(),
                params: world
                    .get_resource::<crate::signal::SignalParams>()
                    .cloned()
                    .unwrap_or_default(),
                computed: world
                    .get_resource::<crate::signal::ComputedSignals>()
                    .cloned()
                    .unwrap_or_default(),
            },
        }
    }

    /// Despawns every authored entity, then spawns this scene and applies
    /// its environment. Derived state (colliders, meshes, engine joints)
    /// rebuilds through the ordinary sync systems — loading has no
    /// special cases.
    pub fn apply(&self, world: &mut World) {
        let existing: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<StableId>>();
            query.iter(world).collect()
        };
        for entity in existing {
            if world.get_entity(entity).is_ok() {
                world.despawn(entity);
            }
        }
        for record in &self.bodies {
            record.spawn(world);
        }
        for record in &self.joints {
            record.spawn(world);
        }
        for record in &self.nodes {
            record.spawn(world);
        }
        world.insert_resource(self.environment.sim.clone());
        world.insert_resource(self.environment.grid.clone());
        world.insert_resource(self.environment.snap.clone());
        world.insert_resource(self.environment.render.clone());
        world.insert_resource(self.environment.lighting.clone());
        world.insert_resource(self.environment.scenery.clone());
        world.insert_resource(self.environment.signals.clone());
        world.insert_resource(self.environment.params.clone());
        world.insert_resource(self.environment.computed.clone());
    }
}

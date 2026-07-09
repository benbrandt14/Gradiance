//! Snapshots of authored entities — the shared unit of undo and persistence.

use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
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
    /// Collision layers / depth mapping.
    pub layers: LayerMask32,
    /// Group stack, innermost first (empty = ungrouped).
    #[serde(default)]
    pub groups: Vec<u32>,
    /// Legacy single group id from pre-hierarchy saves — folded into
    /// `groups` on spawn, never written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
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
            layers: *entity_ref.get::<LayerMask32>()?,
            groups: entity_ref
                .get::<SelectionGroup>()
                .map(|g| g.0.clone())
                .unwrap_or_default(),
            group: None,
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
            self.layers,
        ));
        self.physics.insert_into(&mut entity);
        let mut groups = self.groups.clone();
        if groups.is_empty()
            && let Some(legacy) = self.group
        {
            groups.push(legacy);
        }
        if !groups.is_empty() {
            entity.insert(SelectionGroup(groups));
        }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
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
        Self {
            // Keep in sync with persist::FORMAT_VERSION (v2: avian-component
            // physics — see docs/physics-deadapter-decision.md).
            version: 2,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            bodies,
            joints,
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
        world.insert_resource(self.environment.sim.clone());
        world.insert_resource(self.environment.grid.clone());
        world.insert_resource(self.environment.snap.clone());
        world.insert_resource(self.environment.render.clone());
    }
}

//! Snapshots of authored entities — the shared unit of undo and persistence.

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::layers::LayerMask32;
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::shape::{ShapeDef, ShapeError};
use serde::{Deserialize, Serialize};

/// The shared contract of every authored-entity record: capture from a live
/// entity, spawn back into a world, keyed by [`StableId`].
///
/// The command layer's generic spawn/despawn machinery works over this
/// trait, so a new authored entity kind (body, joint, node, …) gets undoable
/// spawn/delete for the cost of one record type.
pub trait AuthoredRecord: Clone + Send + Sync + std::fmt::Debug + 'static {
    /// Stable identity of the recorded entity.
    fn id(&self) -> StableId;
    /// Validates the record before spawning (shape sanity, etc.).
    fn validate(&self) -> Result<(), ShapeError> {
        Ok(())
    }
    /// Captures the authored state of `entity`, or `None` if it is not a
    /// complete authored entity of this kind.
    fn capture_from(world: &World, entity: Entity) -> Option<Self>;
    /// Spawns an entity with exactly this authored state.
    fn spawn_into(&self, world: &mut World) -> Entity;
}

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
    /// Legacy v4 layer mask — parsed only so the loader can migrate old
    /// files; never written (`None` after capture and after migration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<LayerMask32>,
    /// Group stack, innermost first (empty = ungrouped).
    #[serde(default)]
    pub groups: Vec<u32>,
    /// Field source (attraction/repulsion), if the body carries one.
    #[serde(default)]
    pub field: Option<gradiance_domain::field::FieldSource>,
    /// Trajectory-trail marker, if the body carries one.
    #[serde(default)]
    pub tracer: Option<gradiance_domain::tracer::Tracer>,
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
                .get::<gradiance_domain::field::FieldSource>()
                .copied(),
            tracer: entity_ref
                .get::<gradiance_domain::tracer::Tracer>()
                .copied(),
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
        entity.id()
    }
}

impl AuthoredRecord for BodyRecord {
    fn id(&self) -> StableId {
        self.id
    }

    fn validate(&self) -> Result<(), ShapeError> {
        self.shape.validate()
    }

    fn capture_from(world: &World, entity: Entity) -> Option<Self> {
        Self::capture(world, entity)
    }

    fn spawn_into(&self, world: &mut World) -> Entity {
        self.spawn(world)
    }
}

/// A complete authored-state snapshot of one behavior node (a placeable
/// dataflow entity — see [`domain::node`](gradiance_domain::node)).
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
    pub attachment: gradiance_domain::node::NodeAttachment,
    /// The node's payload (kind).
    pub kind: gradiance_domain::node::NodeKind,
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
            attachment: *entity_ref.get::<gradiance_domain::node::NodeAttachment>()?,
            kind: entity_ref
                .get::<gradiance_domain::node::NodeKind>()?
                .clone(),
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
            gradiance_domain::node::BehaviorNode,
            self.id,
            transform,
            self.attachment,
            self.kind.clone(),
            self.appearance,
        ));
        // The tracer kind also gets a `Tracer` component so the trail sampler
        // drives it uniformly with body tracers.
        let gradiance_domain::node::NodeKind::Tracer(tracer) = self.kind;
        entity.insert(tracer);
        entity.id()
    }
}

impl AuthoredRecord for NodeRecord {
    fn id(&self) -> StableId {
        self.id
    }

    fn capture_from(world: &World, entity: Entity) -> Option<Self> {
        Self::capture(world, entity)
    }

    fn spawn_into(&self, world: &mut World) -> Entity {
        self.spawn(world)
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
    pub def: gradiance_domain::joint::JointDef,
}

impl JointRecord {
    /// Captures the authored state of a joint entity.
    pub fn capture(world: &World, entity: Entity) -> Option<Self> {
        let entity_ref = world.get_entity(entity).ok()?;
        Some(Self {
            id: *entity_ref.get::<StableId>()?,
            def: entity_ref
                .get::<gradiance_domain::joint::JointDef>()?
                .clone(),
        })
    }

    /// Spawns a joint entity with exactly this authored state.
    ///
    /// The joint entity carries an identity `Transform` so derived pin
    /// anchors can live as its children.
    pub fn spawn(&self, world: &mut World) -> Entity {
        world
            .spawn((
                gradiance_domain::Joint,
                self.id,
                self.def.clone(),
                Transform::default(),
            ))
            .id()
    }
}

impl AuthoredRecord for JointRecord {
    fn id(&self) -> StableId {
        self.id
    }

    fn capture_from(world: &World, entity: Entity) -> Option<Self> {
        Self::capture(world, entity)
    }

    fn spawn_into(&self, world: &mut World) -> Entity {
        self.spawn(world)
    }
}

/// Declares [`EnvironmentRecord`] — every settings resource that travels
/// with the scene — from a single field list.
///
/// One line per resource generates the struct field, the capture arm, and
/// the apply arm, so adding a scene-travelling settings resource cannot
/// forget one of the three.
macro_rules! environment_record {
    ($( $(#[$meta:meta])* $field:ident : $ty:ty ),* $(,)?) => {
        /// Persisted editor environment (settings that travel with the
        /// scene). Every field defaults when absent, so files from before a
        /// setting existed still load.
        #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
        pub struct EnvironmentRecord {
            $(
                $(#[$meta])*
                #[serde(default)]
                pub $field: $ty,
            )*
        }

        impl EnvironmentRecord {
            /// Captures every scene-travelling settings resource (missing
            /// resources record their default).
            pub fn capture(world: &World) -> Self {
                Self {
                    $( $field: world.get_resource::<$ty>().cloned().unwrap_or_default(), )*
                }
            }

            /// Installs every recorded resource into the world.
            pub fn apply(&self, world: &mut World) {
                $( world.insert_resource(self.$field.clone()); )*
            }
        }
    };
}

impl EnvironmentRecord {
    /// Restores only the signal-dataflow resources — the `StableId`-keyed scene
    /// graph (bindings, `defparam` knobs, `defsignal` modulators) that commands
    /// mutate as scene *content* — while leaving the config/workstation settings
    /// (grid, snap, sim tuning, rendering) untouched. Used by undo/redo, which
    /// reverts what the scene contains but never how the editor is configured.
    pub fn apply_signals(&self, world: &mut World) {
        world.insert_resource(self.signals.clone());
        world.insert_resource(self.params.clone());
        world.insert_resource(self.computed.clone());
    }
}

environment_record! {
    /// Simulation tuning.
    sim: gradiance_domain::settings::SimSettings,
    /// Grid configuration.
    grid: gradiance_domain::settings::GridSettings,
    /// Snap configuration.
    snap: gradiance_domain::settings::SnapConfig,
    /// Rendering style (defaulted when absent — pre-M10 files).
    render: gradiance_domain::settings::RenderSettings,
    /// Scene lighting (defaulted when absent — pre-V1 files).
    lighting: gradiance_domain::settings::LightingSettings,
    /// Back plane / ground scenery (defaulted when absent — pre-V1 files).
    scenery: gradiance_domain::settings::ScenerySettings,
    /// Signal-dataflow bindings (defaulted when absent — pre-signal files).
    signals: gradiance_domain::signal::SignalBindings,
    /// Signal parameters (`defparam` knobs; defaulted for pre-P2 files).
    params: gradiance_domain::signal::SignalParams,
    /// Computed signals (`defsignal` modulators; defaulted for pre-P2 files).
    computed: gradiance_domain::signal::ComputedSignals,
}

/// A complete scene: the save file, and the unit of whole-world undo.
///
/// Bodies and joints are sorted by [`StableId`] so capture is
/// deterministic — saving the same world twice yields identical bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneRecord {
    /// Format version (see [`FORMAT_VERSION`](super::FORMAT_VERSION)).
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

/// Captures every authored entity matching `F` as records, sorted by id.
fn capture_all<R: AuthoredRecord, F: bevy::ecs::query::QueryFilter>(world: &mut World) -> Vec<R> {
    let mut query = world.query_filtered::<Entity, F>();
    let entities: Vec<Entity> = query.iter(world).collect();
    let mut records: Vec<R> = entities
        .into_iter()
        .filter_map(|e| R::capture_from(world, e))
        .collect();
    records.sort_by_key(|r| r.id().0);
    records
}

impl SceneRecord {
    /// Captures the entire authored world, deterministically ordered.
    pub fn capture(world: &mut World) -> Self {
        Self {
            version: super::FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            bodies: capture_all::<BodyRecord, With<Body>>(world),
            joints: capture_all::<JointRecord, With<gradiance_domain::Joint>>(world),
            nodes: capture_all::<NodeRecord, With<gradiance_domain::node::BehaviorNode>>(world),
            environment: EnvironmentRecord::capture(world),
        }
    }

    /// Despawns every authored entity, then spawns this scene and applies
    /// its environment. Derived state (colliders, meshes, engine joints)
    /// rebuilds through the ordinary sync systems — loading has no
    /// special cases. This is the full **load** restore (scene file / autosave).
    pub fn apply(&self, world: &mut World) {
        self.apply_authored(world);
        self.environment.apply(world);
    }

    /// Restores only the authored entities (bodies, joints, nodes) — despawn
    /// all, respawn from records — **without** touching the config-seam
    /// environment settings. This is the **undo/redo** restore: settings are
    /// not authored state (invariant #4), so reverting to a prior snapshot must
    /// not roll them back. Derived state rebuilds through the sync systems.
    pub fn apply_authored(&self, world: &mut World) {
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
        // Signal-dataflow graph is scene content (StableId-keyed), so it is
        // reverted by undo — unlike the config-seam settings.
        self.environment.apply_signals(world);
    }
}

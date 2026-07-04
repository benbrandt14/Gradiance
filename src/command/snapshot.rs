//! Snapshots of authored entities — the shared unit of undo and persistence.

use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::group::SelectionGroup;
use crate::domain::layers::LayerMask32;
use crate::domain::props::PhysicalProps;
use crate::domain::shape::ShapeDef;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A complete authored-state snapshot of one body.
///
/// Captures exactly the components that constitute the save file; spawning
/// a record recreates the body and every derived component follows via the
/// sync systems. Used by undo records and by scene files alike.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyRecord {
    /// Stable identity (preserved across undo/redo and save/load).
    pub id: StableId,
    /// Authored pose.
    pub pose: PosRot,
    /// Authored geometry.
    pub shape: ShapeDef,
    /// Authored physical properties.
    pub props: PhysicalProps,
    /// Authored appearance.
    pub appearance: Appearance,
    /// Collision layers / depth mapping.
    pub layers: LayerMask32,
    /// Selection group, if grouped.
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
            props: *entity_ref.get::<PhysicalProps>()?,
            appearance: *entity_ref.get::<Appearance>()?,
            layers: *entity_ref.get::<LayerMask32>()?,
            group: entity_ref.get::<SelectionGroup>().map(|g| g.0),
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
            self.props,
            self.appearance,
            self.layers,
        ));
        if let Some(group) = self.group {
            entity.insert(SelectionGroup(group));
        }
        entity.id()
    }
}

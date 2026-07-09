//! Stable, persistence-safe entity identity.
//!
//! Bevy `Entity` ids are ephemeral (they are recycled and change across
//! save/load and undo/redo). Every authored entity carries a [`StableId`],
//! and all cross-references — joints, intents, commands, save files — use
//! it instead of `Entity`. The [`IdIndex`] resource resolves ids back to
//! live entities and is maintained automatically by component hooks.

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A globally unique, persistence-safe identifier for an authored entity.
///
/// Reflects as an **opaque** value (`#[reflect(opaque)]`): the wrapped
/// [`Uuid`] is an identity handle, not a script-authorable field structure, so
/// the reflection API sees `StableId` as a single leaf rather than recursing
/// into its bytes. This is what lets authored intents/records that reference
/// bodies by id (`SpawnBodyIntent`, `JointDef`, …) derive `Reflect` and
/// register with the scripting operation registry — see
/// `docs/script-spike-findings.md` (spike #1 resolution). Opacity also keeps
/// the reflect graph free of `bevy_reflect`'s optional `uuid` feature.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(opaque)]
#[component(
    immutable,
    on_insert = on_insert_stable_id,
    on_discard = on_discard_stable_id,
    on_remove = on_discard_stable_id
)]
pub struct StableId(pub Uuid);

impl StableId {
    /// Generates a fresh random id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for StableId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Live lookup table from [`StableId`] to [`Entity`].
///
/// Maintained by `StableId`'s component hooks; never mutate it manually.
#[derive(Resource, Default, Debug)]
pub struct IdIndex(HashMap<StableId, Entity>);

impl IdIndex {
    /// Resolves a stable id to its live entity, if one exists.
    pub fn entity(&self, id: StableId) -> Option<Entity> {
        self.0.get(&id).copied()
    }

    /// Number of indexed entities.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn on_insert_stable_id(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&id) = world.get::<StableId>(ctx.entity) else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<IdIndex>() {
        index.0.insert(id, ctx.entity);
    }
}

fn on_discard_stable_id(mut world: DeferredWorld, ctx: HookContext) {
    let Some(&id) = world.get::<StableId>(ctx.entity) else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<IdIndex>() {
        // Only remove if the mapping still points at this entity (a re-insert
        // of the same id on another entity should win).
        if index.0.get(&id) == Some(&ctx.entity) {
            index.0.remove(&id);
        }
    }
}

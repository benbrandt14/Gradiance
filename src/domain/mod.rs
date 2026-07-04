//! Authored components — the durable content of a scene.
//!
//! Everything in this module (plus [`StableId`](crate::core::ids::StableId)
//! and `Transform` pose) **is** the save file. No engine-specific types:
//! colliders, meshes, materials, and avian joints are *derived* from these
//! components by `Changed<>`-driven sync systems and are never serialized.

pub mod appearance;
pub mod group;
pub mod joint;
pub mod layers;
pub mod props;
pub mod settings;
pub mod shape;

use bevy::prelude::*;

/// Marker component present on every authored body.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Body;

/// Registers domain-level requirements (currently nothing beyond types).
#[derive(Default)]
pub struct DomainPlugin;

impl Plugin for DomainPlugin {
    fn build(&self, _app: &mut App) {}
}

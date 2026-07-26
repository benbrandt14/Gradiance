//! Spawn, delete, and duplicate commands.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_scene::{AuthoredRecord, BodyRecord, NodeRecord};

/// Spawns one authored entity (body, behavior node, …) from its record.
///
/// Generic over [`AuthoredRecord`]: the record validates and spawns itself,
/// and its `id` is reused on redo so later commands referencing the entity
/// stay valid across undo/redo cycles.
#[derive(Debug)]
pub struct SpawnCommand<R: AuthoredRecord> {
    /// The authored state to create; its `id` is reused on redo.
    pub record: R,
    name: &'static str,
}

impl<R: AuthoredRecord> SpawnCommand<R> {
    /// Builds a spawn command; `name` is the shared [`intent::name`]
    /// constant of the intent that produced it.
    ///
    /// [`intent::name`]: crate::intent::name
    pub fn new(record: R, name: &'static str) -> Self {
        Self { record, name }
    }
}

impl<R: AuthoredRecord> GameCommand for SpawnCommand<R> {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.record.validate()?;
        self.record.spawn_into(world);
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Deletes a set of bodies (and every joint referencing them), restoring
/// everything — same ids — on undo.
#[derive(Debug)]
pub struct DeleteCommand {
    /// Bodies and/or behavior nodes to delete.
    pub targets: Vec<StableId>,
}

impl DeleteCommand {
    /// Builds a delete command for `targets` (bodies and/or nodes).
    pub fn new(targets: Vec<StableId>) -> Self {
        Self { targets }
    }
}

impl GameCommand for DeleteCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Capture-first so a missing entity aborts before any despawn. Each
        // target is either a body (with a cascade of joints) or a behavior
        // node — a mixed selection deletes both in one undoable step.
        let mut body_pairs = Vec::new();
        let mut node_pairs = Vec::new();
        for &id in &self.targets {
            let entity = resolve(world, id)?;
            if let Some(record) = BodyRecord::capture(world, entity) {
                body_pairs.push((entity, record));
            } else if let Some(record) = NodeRecord::capture(world, entity) {
                node_pairs.push((entity, record));
            } else {
                return Err(CommandError::MissingEntity(id));
            }
        }
        if body_pairs.is_empty() && node_pairs.is_empty() {
            return Err(CommandError::NoEffect);
        }
        // Cascade: joints referencing any deleted body go too (a joint
        // with a dangling endpoint must never exist).
        let body_ids: Vec<StableId> = body_pairs.iter().map(|(_, r)| r.id).collect();
        let joints = crate::joint_cmd::joints_referencing(world, &body_ids);
        // Cascade: behavior nodes attached to a deleted body go too (a
        // tracer/sensor/actuator on a body dies with it, like a joint).
        let already: Vec<Entity> = node_pairs.iter().map(|(e, _)| *e).collect();
        let node_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<gradiance_domain::node::BehaviorNode>>()
            .iter(world)
            .collect();
        for entity in node_entities {
            if already.contains(&entity) {
                continue;
            }
            if let Some(record) = NodeRecord::capture(world, entity)
                && record
                    .attachment
                    .target
                    .is_some_and(|t| body_ids.contains(&t))
            {
                node_pairs.push((entity, record));
            }
        }
        for (entity, _) in joints {
            world.despawn(entity);
        }
        for (entity, _) in body_pairs {
            world.despawn(entity);
        }
        for (entity, _) in node_pairs {
            world.despawn(entity);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::DELETE
    }
}

/// The next unused selection-group id.
pub(crate) fn next_group_id(world: &mut World) -> u32 {
    let mut query = world.query::<&gradiance_domain::group::SelectionGroup>();
    query
        .iter(world)
        .flat_map(|g| g.0.iter().copied())
        .max()
        .map_or(1, |m| m + 1)
}

/// Rewrites cloned bodies' selection groups to fresh ids (one fresh id
/// per distinct source group), so duplicates form their *own* groups
/// instead of joining the originals' — the "group selection deteriorates
/// after repeated operations" bug.
pub(crate) fn remap_clone_groups(clones: &mut [BodyRecord], next_group: &mut u32) {
    let mut remap: Vec<(u32, u32)> = Vec::new();
    for record in clones {
        for group in &mut record.groups {
            let new = if let Some((_, new)) = remap.iter().find(|(old, _)| *old == *group) {
                *new
            } else {
                let new = *next_group;
                *next_group += 1;
                remap.push((*group, new));
                new
            };
            *group = new;
        }
    }
}

/// Clones the joints internal to a cloned body set: every joint whose
/// referenced bodies all lie within `id_map` is copied with fresh ids,
/// endpoints remapped, world anchors transformed by `map_world`, and
/// rest rotations advanced by `rot_offset` (the rotation the cloned
/// bodies received — radial arrays with rotated items).
pub(crate) fn clone_internal_joints(
    world: &mut World,
    id_map: &[(StableId, StableId)],
    map_world: impl Fn(Vec2) -> Vec2,
    rot_offset: f32,
) -> Vec<gradiance_scene::JointRecord> {
    let sources: Vec<StableId> = id_map.iter().map(|(old, _)| *old).collect();
    let remap = |id: StableId| {
        id_map
            .iter()
            .find(|(old, _)| *old == id)
            .map(|(_, new)| *new)
    };
    crate::joint_cmd::joints_referencing(world, &sources)
        .into_iter()
        .filter_map(|(_, record)| {
            let mut def = record.def;
            // Every endpoint must be inside the cloned set.
            def.body_a = remap(def.body_a)?;
            def.rest_rot_a += rot_offset;
            match def.body_b {
                Some(b) => {
                    def.body_b = Some(remap(b)?);
                    def.rest_rot_b += rot_offset;
                }
                // World pin: the anchor is a world point — transform it
                // like the cloned bodies. The pin itself never rotates,
                // so only `rest_rot_a` advanced.
                None => def.anchor_b = map_world(def.anchor_b),
            }
            Some(gradiance_scene::JointRecord {
                id: StableId::new(),
                def,
            })
        })
        .collect()
}

/// Duplicates a set of bodies at an offset, including the joints internal
/// to the set (a duplicated hinge assembly stays hinged).
///
/// Clone ids are minted fresh on the single apply; the stack snapshots the
/// result, so nothing has to be replayed.
#[derive(Debug)]
pub struct DuplicateCommand {
    /// Bodies to clone.
    pub sources: Vec<StableId>,
    /// World-space offset applied to each clone.
    pub offset: Vec2,
}

impl DuplicateCommand {
    /// Builds a duplicate command for `sources` offset by `offset`.
    pub fn new(sources: Vec<StableId>, offset: Vec2) -> Self {
        Self { sources, offset }
    }
}

impl GameCommand for DuplicateCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let mut clones = Vec::with_capacity(self.sources.len());
        let mut id_map = Vec::with_capacity(self.sources.len());
        for &id in &self.sources {
            let entity = resolve(world, id)?;
            let mut record =
                BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
            record.id = StableId::new();
            record.pose.pos += self.offset;
            id_map.push((id, record.id));
            clones.push(record);
        }
        if clones.is_empty() {
            return Err(CommandError::NoEffect);
        }
        let offset = self.offset;
        let joint_clones = clone_internal_joints(world, &id_map, |p| p + offset, 0.0);
        let node_clones = clone_attached_nodes(world, &id_map, offset);
        // Installs the cloned bindings into `SignalBindings` (the returned
        // names were only ever needed to undo them).
        clone_bindings(world, &id_map);
        let mut next_group = next_group_id(world);
        remap_clone_groups(&mut clones, &mut next_group);
        for record in &clones {
            record.spawn(world);
        }
        for record in &joint_clones {
            record.spawn(world);
        }
        for record in &node_clones {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::DUPLICATE
    }
}

/// Clones every behavior node attached to a duplicated body, remapping its
/// attachment to the clone and offsetting a free node's pose. Nodes not
/// attached to any duplicated body are left alone.
fn clone_attached_nodes(
    world: &mut World,
    id_map: &[(StableId, StableId)],
    offset: Vec2,
) -> Vec<NodeRecord> {
    let remap = |id: StableId| id_map.iter().find(|(s, _)| *s == id).map(|(_, c)| *c);
    let node_entities: Vec<Entity> = world
        .query_filtered::<Entity, With<gradiance_domain::node::BehaviorNode>>()
        .iter(world)
        .collect();
    let mut clones = Vec::new();
    for entity in node_entities {
        let Some(mut record) = NodeRecord::capture(world, entity) else {
            continue;
        };
        let Some(target) = record.attachment.target else {
            continue; // free nodes don't belong to any duplicated body
        };
        let Some(clone_target) = remap(target) else {
            continue; // attached to a body outside the duplicated set
        };
        record.id = StableId::new();
        record.attachment.target = Some(clone_target);
        record.pose.pos += offset;
        clones.push(record);
    }
    clones
}

/// Clones every signal binding that references a duplicated body, remapping
/// its source/sink to the clone and giving it a fresh unique name; returns
/// the new names (for undo). Bindings referencing bodies outside the set are
/// left alone.
fn clone_bindings(world: &mut World, id_map: &[(StableId, StableId)]) -> Vec<String> {
    let Some(bindings) = world.get_resource::<gradiance_signal::SignalBindings>() else {
        return Vec::new();
    };
    let existing: Vec<gradiance_signal::SignalBinding> = bindings.0.clone();
    let mut taken: Vec<String> = existing.iter().map(|b| b.name.clone()).collect();
    let mut new_bindings = Vec::new();
    for binding in &existing {
        if let Some(mut clone) = gradiance_signal::remap_binding(binding, id_map) {
            let name = fresh_binding_name(&binding.name, &taken);
            clone.name.clone_from(&name);
            taken.push(name);
            new_bindings.push(clone);
        }
    }
    let names: Vec<String> = new_bindings.iter().map(|b| b.name.clone()).collect();
    if let Some(mut res) = world.get_resource_mut::<gradiance_signal::SignalBindings>() {
        res.0.extend(new_bindings);
    }
    names
}

/// A fresh `base-copy`, `base-copy-2`, … name not already taken.
fn fresh_binding_name(base: &str, taken: &[String]) -> String {
    let first = format!("{base}-copy");
    if !taken.iter().any(|n| n == &first) {
        return first;
    }
    let mut n = 2;
    loop {
        let name = format!("{base}-copy-{n}");
        if !taken.iter().any(|t| t == &name) {
            return name;
        }
        n += 1;
    }
}

//! Grouping commands.

use crate::command::{CommandError, GameCommand};
use crate::core::ids::{IdIndex, StableId};
use crate::domain::group::SelectionGroup;
use bevy::prelude::*;

fn prior_groups(
    world: &mut World,
    targets: &[StableId],
) -> Result<Vec<(StableId, Option<u32>)>, CommandError> {
    targets
        .iter()
        .map(|&id| {
            let entity = world
                .resource::<IdIndex>()
                .entity(id)
                .ok_or(CommandError::MissingEntity(id))?;
            Ok((id, world.get::<SelectionGroup>(entity).map(|g| g.0)))
        })
        .collect()
}

fn restore_groups(world: &mut World, prior: &[(StableId, Option<u32>)]) {
    for (id, group) in prior {
        if let Some(entity) = world.resource::<IdIndex>().entity(*id) {
            match group {
                Some(g) => {
                    world.entity_mut(entity).insert(SelectionGroup(*g));
                }
                None => {
                    world.entity_mut(entity).remove::<SelectionGroup>();
                }
            }
        }
    }
}

/// Puts the targets into one fresh group (select-one-selects-all).
#[derive(Debug)]
pub struct GroupCommand {
    /// Bodies to group.
    pub targets: Vec<StableId>,
    prior: Vec<(StableId, Option<u32>)>,
}

impl GroupCommand {
    /// Builds a group command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            prior: Vec::new(),
        }
    }
}

impl GameCommand for GroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.targets.len() < 2 {
            return Err(CommandError::NoEffect);
        }
        let prior = prior_groups(world, &self.targets)?;
        // Fresh id: one past the largest in use.
        let next = {
            let mut query = world.query::<&SelectionGroup>();
            query.iter(world).map(|g| g.0).max().map_or(1, |m| m + 1)
        };
        for &id in &self.targets {
            if let Some(entity) = world.resource::<IdIndex>().entity(id) {
                world.entity_mut(entity).insert(SelectionGroup(next));
            }
        }
        if self.prior.is_empty() {
            self.prior = prior;
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        restore_groups(world, &self.prior);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Group"
    }
}

/// Removes the targets from their groups.
#[derive(Debug)]
pub struct UngroupCommand {
    /// Bodies to ungroup.
    pub targets: Vec<StableId>,
    prior: Vec<(StableId, Option<u32>)>,
}

impl UngroupCommand {
    /// Builds an ungroup command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            prior: Vec::new(),
        }
    }
}

impl GameCommand for UngroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let prior = prior_groups(world, &self.targets)?;
        if prior.iter().all(|(_, g)| g.is_none()) {
            return Err(CommandError::NoEffect);
        }
        for &id in &self.targets {
            if let Some(entity) = world.resource::<IdIndex>().entity(id) {
                world.entity_mut(entity).remove::<SelectionGroup>();
            }
        }
        if self.prior.is_empty() {
            self.prior = prior;
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        restore_groups(world, &self.prior);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Ungroup"
    }
}

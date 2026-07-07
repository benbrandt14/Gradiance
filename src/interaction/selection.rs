//! The selection set.

use crate::domain::Body;
use crate::domain::group::SelectionGroup;
use bevy::prelude::*;

/// The currently selected bodies (insertion-ordered, no duplicates).
///
/// Holds `Entity` for frame-to-frame ergonomics; [`prune_dead_selection`]
/// drops despawned entries every frame, and intents translate to
/// `StableId` at the command boundary.
#[derive(Resource, Default, Debug)]
pub struct Selection {
    entities: Vec<Entity>,
}

impl Selection {
    /// Selected entities in selection order.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter().copied()
    }

    /// Whether `entity` is selected.
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.contains(&entity)
    }

    /// The first-selected (primary) entity.
    pub fn primary(&self) -> Option<Entity> {
        self.entities.first().copied()
    }

    /// Number of selected entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Adds an entity (no-op if present).
    pub fn add(&mut self, entity: Entity) {
        if !self.contains(entity) {
            self.entities.push(entity);
        }
    }

    /// Removes an entity.
    pub fn remove(&mut self, entity: Entity) {
        self.entities.retain(|e| *e != entity);
    }

    /// Adds if absent, removes if present (shift-click semantics).
    pub fn toggle(&mut self, entity: Entity) {
        if self.contains(entity) {
            self.remove(entity);
        } else {
            self.add(entity);
        }
    }

    /// Replaces the selection with a single entity.
    pub fn set(&mut self, entity: Entity) {
        self.entities.clear();
        self.entities.push(entity);
    }

    /// Clears the selection.
    pub fn clear(&mut self) {
        self.entities.clear();
    }

    /// Retains only entities satisfying the predicate.
    pub fn retain(&mut self, f: impl FnMut(&Entity) -> bool) {
        self.entities.retain(f);
    }
}

/// The currently selected **joint**, if any.
///
/// Joints are selected separately from bodies: clicking a joint's anchor
/// glyph selects the joint (and clears the body [`Selection`]); clicking a
/// body clears this. A selected joint drives the joint inspector and the
/// delete/anchor-drag gestures.
#[derive(Resource, Default, Debug)]
pub struct SelectedJoint(pub Option<Entity>);

/// Drops despawned entities from the selection.
pub fn prune_dead_selection(
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<SelectedJoint>,
    bodies: Query<(), With<Body>>,
    joints: Query<(), With<crate::domain::Joint>>,
) {
    if !selection.is_empty() {
        selection.retain(|e| bodies.contains(*e));
    }
    if let Some(joint) = selected_joint.0
        && !joints.contains(joint)
    {
        selected_joint.0 = None;
    }
}

/// Expands `entities` with all members of any [`SelectionGroup`] they
/// belong to (selecting one grouped body selects the whole group).
pub fn expand_groups(
    entities: &mut Vec<Entity>,
    groups: &Query<(Entity, &SelectionGroup), With<Body>>,
) {
    // Expansion follows the *outermost* group id — selecting a member of
    // a nested assembly selects the whole assembly.
    let mut group_ids: Vec<u32> = Vec::new();
    for entity in entities.iter() {
        if let Ok((_, group)) = groups.get(*entity)
            && let Some(outer) = group.outermost()
            && !group_ids.contains(&outer)
        {
            group_ids.push(outer);
        }
    }
    for (entity, group) in groups.iter() {
        if group
            .outermost()
            .is_some_and(|outer| group_ids.contains(&outer))
            && !entities.contains(&entity)
        {
            entities.push(entity);
        }
    }
}

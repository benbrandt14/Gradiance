//! The selection set and its transition state machine.
//!
//! Selection state is two resources — the body [`Selection`] and the
//! [`SelectedJoint`] — under one **invariant**: they are never both
//! populated (you are editing bodies *or* a joint, never both). Every
//! change goes through one function, [`SelectTransition::apply`], so the
//! invariant lives in exactly one place and new interactions extend the
//! vocabulary instead of poking the resources directly.
//!
//! ```text
//!                    ┌──────────────── Clear ───────────────┐
//!                    ▼                                       │
//!              ┌───────────┐   SelectJoint(j)        ┌──────────────┐
//!              │  Empty    │ ─────────────────────▶  │ Joint(j)     │
//!              │ (nothing) │ ◀───── DeselectJoint ── │              │
//!              └───────────┘                         └──────────────┘
//!                    │  ▲                                    │
//!     SetBodies /    │  │ Clear                              │ SetBodies /
//!     Add / Toggle   ▼  │                                    │ Add / Toggle
//!              ┌──────────────┐  ◀───────────────────────────┘
//!              │  Bodies{…}   │   (any body transition clears the joint)
//!              └──────────────┘
//! ```
//!
//! Group expansion (selecting one member selects its whole group) happens
//! inside the body transitions, so callers pass raw hits and never worry
//! about it.

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
/// delete/anchor-drag gestures. Mutated only via [`SelectTransition`].
#[derive(Resource, Default, Debug)]
pub struct SelectedJoint(pub Option<Entity>);

/// The complete vocabulary of selection changes — the transition alphabet
/// of the selection state machine (see the [module docs](self)).
///
/// This is the **only** sanctioned way to change selection; going through
/// it guarantees the joint-xor-bodies invariant and group expansion.
#[derive(Debug, Clone)]
pub enum SelectTransition {
    /// Replace the body selection with these hits (group-expanded).
    SetBodies(Vec<Entity>),
    /// Add these hits to the body selection (group-expanded).
    AddBodies(Vec<Entity>),
    /// Toggle these hits in the body selection (group-expanded).
    ToggleBodies(Vec<Entity>),
    /// Select a single joint (clears the body selection).
    SelectJoint(Entity),
    /// Clear the joint only, leaving the body selection untouched.
    DeselectJoint,
    /// Clear everything.
    Clear,
}

impl SelectTransition {
    /// Applies the transition, maintaining the invariant that the body
    /// selection and the joint selection are never both populated.
    pub fn apply(
        self,
        bodies: &mut Selection,
        joint: &mut SelectedJoint,
        groups: &Query<(Entity, &SelectionGroup), With<Body>>,
    ) {
        match self {
            Self::SetBodies(mut hits) => {
                expand_groups(&mut hits, groups);
                bodies.entities = dedup_preserving_order(hits);
                joint.0 = None;
            }
            Self::AddBodies(mut hits) => {
                expand_groups(&mut hits, groups);
                for e in hits {
                    bodies.add(e);
                }
                joint.0 = None;
            }
            Self::ToggleBodies(mut hits) => {
                expand_groups(&mut hits, groups);
                for e in hits {
                    bodies.toggle(e);
                }
                joint.0 = None;
            }
            Self::SelectJoint(e) => {
                bodies.clear();
                joint.0 = Some(e);
            }
            Self::DeselectJoint => joint.0 = None,
            Self::Clear => {
                bodies.clear();
                joint.0 = None;
            }
        }
    }
}

/// Deduplicates entities, preserving first-seen order (the order
/// `SetBodies` keeps).
pub(crate) fn dedup_preserving_order(entities: Vec<Entity>) -> Vec<Entity> {
    let mut out = Vec::with_capacity(entities.len());
    for e in entities {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

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

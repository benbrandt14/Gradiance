//! Particle **groups**: the shared, locked attributes a population of
//! particles inherits, and the unit selection/aggregates act on.
//!
//! A group is the "act like a single entity" handle the user asked for: a
//! whole cloud shares one `DepthBand` (so it **obeys collision layers as
//! one entity** — S4 gates particle↔body interactions on
//! [`DepthBand::overlaps`]), one tint, one self-gravity, etc. Particles
//! carry only a `group` index into this live table (see
//! [`ParticleState`](crate::sim::kernel::ParticleState)).
//!
//! Editor-state for now (the table is rebuilt as groups are created);
//! the authored, persisted `ParticleGroup` component — so groups survive
//! save/undo — lands in S3c. Never breaks the default build: feature-gated.

use crate::domain::appearance::Rgba;
use crate::domain::depth::DepthBand;
use bevy::prelude::*;

/// The locked, shared attributes of one particle group.
#[derive(Debug, Clone, Copy)]
pub struct GroupAttrs {
    /// Depth band — the group's collision layer, obeyed as one entity.
    pub depth: DepthBand,
    /// Base tint (particles warm/brighten from it with speed).
    pub tint: Rgba,
    /// Self-gravity strength among this group's particles (N-body driver).
    pub self_gravity: f32,
    /// How strongly this group responds to the superposed force field
    /// (`Fields::accel_at`): a per-group multiplier on the sampled
    /// acceleration. `1.0` = full response (default); `0.0` = the group
    /// ignores fields (inert dust); `<0` repels. This is the extensible
    /// field↔particle handle — new field *kinds* land at the single
    /// cut-point, and each group tunes its response with one coefficient.
    pub field_response: f32,
    /// Seconds a particle lives before culling. `INFINITY` = persists
    /// (placed/filled material); fountains set a finite lifetime.
    pub max_age: f32,
}

impl Default for GroupAttrs {
    fn default() -> Self {
        Self {
            depth: DepthBand::default(),
            tint: Rgba::rgb(0.45, 0.6, 1.0),
            self_gravity: 0.0,
            field_response: 1.0,
            // Placed material persists; emitters override this per group.
            max_age: f32::INFINITY,
        }
    }
}

/// The live group table. Index `i` is the `group` value particles store;
/// group 0 always exists (the default fountain / ungrouped bucket).
#[derive(Resource, Debug)]
pub struct ParticleGroups(pub Vec<GroupAttrs>);

impl Default for ParticleGroups {
    fn default() -> Self {
        // Group 0 is the default bucket (the demo emitter, etc.).
        Self(vec![GroupAttrs::default()])
    }
}

impl ParticleGroups {
    /// Registers a new group and returns its index.
    pub fn add(&mut self, attrs: GroupAttrs) -> u32 {
        let index = self.0.len() as u32;
        self.0.push(attrs);
        index
    }

    /// The attributes of `group`, or the default bucket if out of range.
    pub fn get(&self, group: u32) -> GroupAttrs {
        self.0.get(group as usize).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_table_has_the_zero_bucket() {
        let groups = ParticleGroups::default();
        assert_eq!(groups.0.len(), 1, "group 0 always exists");
    }

    #[test]
    fn add_allocates_increasing_indices() {
        let mut groups = ParticleGroups::default();
        let a = groups.add(GroupAttrs::default());
        let b = groups.add(GroupAttrs::default());
        assert_eq!((a, b), (1, 2));
        assert_eq!(groups.get(a).depth, DepthBand::default());
        // Out-of-range falls back to the default bucket.
        assert_eq!(groups.get(99).tint, GroupAttrs::default().tint);
    }
}

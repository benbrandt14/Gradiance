//! Array repeats: patterned copies of the selection.
//!
//! # Patterns are data, not code paths
//!
//! Every mode reduces to a list of [`CopyPlacement`]s — one translation each —
//! and [`ArrayCommand`] does nothing but walk that list. Adding a pattern is a
//! new arm of [`ArrayMode::placements`], with no new cloning,
//! joint-remapping, or group-renumbering logic to get subtly wrong. It also
//! means a pattern can be *inspected* before it is applied, which is what lets
//! the tool draw an exact ghost of what pressing release would do.
//!
//! # Deliberately small
//!
//! An earlier revision carried per-copy tweens (per-axis size ratios, spin,
//! depth stepping, and a closed-form tapered contact pitch to keep shrinking
//! copies flush). The maths worked, but the feature was never polished enough
//! to be trusted, and it made every other part of the tool harder to reason
//! about — placements stopped being pure translations, the ghost needed a
//! matching shear decomposition, and the pitch became a function of the copy
//! index. It was removed so the part that *does* work — repeat a selection at
//! contact spacing — can be built on cleanly. See `docs/array-decision.md`.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_scene::BodyRecord;

/// How array copies are placed.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub enum ArrayMode {
    /// Copies at `step`, `2·step`, … along one axis.
    Linear {
        /// Translation between consecutive copies.
        step: Vec2,
    },
    /// A two-axis grid: `count` copies along `step`, `cross_count` along
    /// `cross`, filling the rectangle they span.
    Grid {
        /// Translation between consecutive columns.
        step: Vec2,
        /// Translation between consecutive rows.
        cross: Vec2,
        /// Extra rows beyond the original's.
        cross_count: u32,
    },
}

/// Where one copy goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyPlacement {
    /// Translation from the source set to this copy, world space.
    pub translate: Vec2,
}

impl CopyPlacement {
    /// Maps a world point through this copy's placement.
    pub fn map_point(&self, p: Vec2) -> Vec2 {
        p + self.translate
    }
}

impl ArrayMode {
    /// The placements this mode implies for `count` copies.
    ///
    /// The original is never in the list — only the copies — so the caller
    /// can spawn one body per placement without filtering.
    pub fn placements(&self, count: u32) -> Vec<CopyPlacement> {
        let mut out = Vec::new();
        match *self {
            Self::Linear { step } => {
                for k in 1..=count {
                    out.push(CopyPlacement {
                        translate: step * k as f32,
                    });
                }
            }
            Self::Grid {
                step,
                cross,
                cross_count,
            } => {
                for row in 0..=cross_count {
                    for col in 0..=count {
                        // (0, 0) is the original, not a copy.
                        if row == 0 && col == 0 {
                            continue;
                        }
                        out.push(CopyPlacement {
                            translate: step * col as f32 + cross * row as f32,
                        });
                    }
                }
            }
        }
        out
    }

    /// Short human label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear { .. } => "Linear",
            Self::Grid { .. } => "Grid",
        }
    }
}

/// Creates patterned copies of the source bodies (one undo step), including
/// joints internal to the source set (patterned linkages stay linked).
///
/// Clone ids are minted once and reused on redo.
#[derive(Debug)]
pub struct ArrayCommand {
    /// Bodies to pattern.
    pub sources: Vec<StableId>,
    /// Number of copies along the pattern's primary axis.
    pub count: u32,
    /// Placement rule.
    pub mode: ArrayMode,
}

impl ArrayCommand {
    /// Builds an array command.
    pub fn new(sources: Vec<StableId>, count: u32, mode: ArrayMode) -> Self {
        Self {
            sources,
            count,
            mode,
        }
    }
}

impl GameCommand for ArrayCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let placements = self.mode.placements(self.count);
        if placements.is_empty() || self.sources.is_empty() {
            return Err(CommandError::NoEffect);
        }
        let mut clones = Vec::with_capacity(self.sources.len() * placements.len());
        let mut joint_clones = Vec::new();
        let mut next_group = crate::spawn::next_group_id(world);

        for placement in placements {
            let copy_start = clones.len();
            let mut id_map = Vec::with_capacity(self.sources.len());
            for &id in &self.sources {
                let entity = resolve(world, id)?;
                let mut clone =
                    BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
                clone.id = StableId::new();
                clone.pose.pos = placement.map_point(clone.pose.pos);
                id_map.push((id, clone.id));
                clones.push(clone);
            }
            joint_clones.extend(crate::spawn::clone_internal_joints(
                world,
                &id_map,
                |p| placement.map_point(p),
                0.0,
            ));
            // Each copy gets its own selection groups.
            crate::spawn::remap_clone_groups(&mut clones[copy_start..], &mut next_group);
        }
        for record in &clones {
            record.spawn(world);
        }
        for record in &joint_clones {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::ARRAY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linear_array_steps_uniformly() {
        let mode = ArrayMode::Linear {
            step: Vec2::new(2.0, 0.0),
        };
        let places = mode.placements(3);
        assert_eq!(places.len(), 3, "the original is not a copy");
        for (k, place) in places.iter().enumerate() {
            let want = Vec2::new(2.0 * (k + 1) as f32, 0.0);
            assert!(place.translate.distance(want) < 1e-5, "{place:?}");
        }
    }

    #[test]
    fn a_grid_fills_the_rectangle_minus_the_original() {
        let mode = ArrayMode::Grid {
            step: Vec2::new(1.0, 0.0),
            cross: Vec2::new(0.0, 1.0),
            cross_count: 2,
        };
        let places = mode.placements(3);
        // 4 columns x 3 rows, less the original.
        assert_eq!(places.len(), 11);
        assert!(
            places
                .iter()
                .any(|p| p.translate.distance(Vec2::new(3.0, 2.0)) < 1e-5),
            "the far corner is present"
        );
        assert!(
            !places.iter().any(|p| p.translate.length() < 1e-6),
            "the original's own cell is not a copy"
        );
    }

    #[test]
    fn a_zero_count_pattern_produces_nothing() {
        assert!(ArrayMode::Linear { step: Vec2::X }.placements(0).is_empty());
    }

    #[test]
    fn a_grid_with_no_extra_rows_is_just_a_row() {
        let places = ArrayMode::Grid {
            step: Vec2::X,
            cross: Vec2::Y,
            cross_count: 0,
        }
        .placements(3);
        assert_eq!(places.len(), 3);
        assert!(places.iter().all(|p| p.translate.y.abs() < 1e-6));
    }
}

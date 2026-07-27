//! Array repeats: patterned copies of the selection.
//!
//! # Patterns are data, not code paths
//!
//! Every mode reduces to a list of [`CopyPlacement`]s — one rigid map plus a
//! few per-copy tweens — and [`ArrayCommand`] does nothing but walk that
//! list. Adding a pattern is a new arm of [`ArrayMode::placements`], with no
//! new cloning, joint-remapping, or group-renumbering logic to get subtly
//! wrong. It also means a pattern can be *inspected* before it is applied,
//! which is what lets the tool draw an exact ghost of what pressing release
//! would do.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_scene::BodyRecord;

/// Per-copy changes that accumulate along the pattern, on top of whatever
/// rigid placement the mode produces.
///
/// These are what turn a plain repeat into a *pattern*: a fan of blades, a
/// tapering spiral, a staircase that walks back into the scene. They apply to
/// every mode, because they are about the copy's index rather than about
/// where the copy sits.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct ArrayTweens {
    /// Extra rotation added to copy `k`, as `k · spin` radians about the
    /// copy's own centre.
    pub spin: f32,
    /// Uniform scale of copy `k`, as `ratio^k`. 1.0 leaves sizes alone.
    pub scale_ratio: f32,
    /// Depth-band shift of copy `k`, as `k · depth` world units into the
    /// screen — a staircase *through* the 2.5D layers rather than across
    /// them, which also means successive copies stop colliding with each
    /// other once the step exceeds a layer.
    pub depth: f32,
}

impl Default for ArrayTweens {
    fn default() -> Self {
        Self {
            spin: 0.0,
            scale_ratio: 1.0,
            depth: 0.0,
        }
    }
}

impl ArrayTweens {
    /// Whether every tween is inert (lets callers skip work).
    pub fn is_identity(&self) -> bool {
        self.spin == 0.0 && (self.scale_ratio - 1.0).abs() < 1e-6 && self.depth == 0.0
    }
}

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
        /// Fraction of `step` that alternate rows are offset by — 0 for a
        /// plain grid, 0.5 for a running-bond brick wall.
        stagger: f32,
    },
    /// Copies rotated about `pivot` by multiples of `angle_step`.
    Radial {
        /// Center of the pattern.
        pivot: Vec2,
        /// Angle between consecutive copies (radians).
        angle_step: f32,
        /// Also rotate each copy's own orientation.
        rotate_items: bool,
    },
}

/// Where one copy goes: a rigid map of the source set, plus its tweens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyPlacement {
    /// Rotation applied about [`pivot`](Self::pivot), radians.
    pub angle: f32,
    /// Centre of that rotation, world space.
    pub pivot: Vec2,
    /// Translation applied after the rotation.
    pub translate: Vec2,
    /// Extra spin of each body about its own centre, radians.
    pub spin: f32,
    /// Uniform scale of each body about its own centre.
    pub scale: f32,
    /// Depth-band shift, world units into the screen.
    pub depth: f32,
}

impl CopyPlacement {
    /// Maps a world point through this copy's rigid placement.
    pub fn map_point(&self, p: Vec2) -> Vec2 {
        self.pivot + Vec2::from_angle(self.angle).rotate(p - self.pivot) + self.translate
    }

    /// The total rotation a body under this placement receives.
    pub fn body_rotation(&self) -> f32 {
        self.angle + self.spin
    }

    /// Adds `spin` to this placement's own-axis rotation.
    fn with_spin(mut self, spin: f32) -> Self {
        self.spin += spin;
        self
    }
}

impl ArrayMode {
    /// Expands the pattern into one placement per copy (the original is not
    /// included).
    ///
    /// `count` means "copies along the primary axis" for every mode, so the
    /// tool's drag distance maps to the same field whichever pattern is
    /// selected.
    pub fn placements(&self, count: u32, tweens: ArrayTweens) -> Vec<CopyPlacement> {
        let mut out = Vec::new();
        match *self {
            Self::Linear { step } => {
                for k in 1..=count {
                    out.push(placement_at(k, step * k as f32, Vec2::ZERO, 0.0, tweens));
                }
            }
            Self::Grid {
                step,
                cross,
                cross_count,
                stagger,
            } => {
                for row in 0..=cross_count {
                    for col in 0..=count {
                        // (0, 0) is the original, not a copy.
                        if row == 0 && col == 0 {
                            continue;
                        }
                        // Odd rows shift along the column axis — this is what
                        // makes a running bond rather than a stack bond.
                        let offset = if row % 2 == 1 { stagger } else { 0.0 };
                        let translate = step * (col as f32 + offset) + cross * row as f32;
                        // Tweens index by distance through the pattern, so a
                        // taper grows smoothly outward from the original
                        // rather than resetting at every row break.
                        out.push(placement_at(col + row, translate, Vec2::ZERO, 0.0, tweens));
                    }
                }
            }
            Self::Radial {
                pivot,
                angle_step,
                rotate_items,
            } => {
                for k in 1..=count {
                    let angle = angle_step * k as f32;
                    // The orbit always rotates the *set* about the pivot;
                    // `rotate_items` decides whether each body turns with it
                    // or stays upright, which is an un-spin of the orbit.
                    let spin = if rotate_items { 0.0 } else { -angle };
                    out.push(placement_at(k, Vec2::ZERO, pivot, angle, tweens).with_spin(spin));
                }
            }
        }
        out
    }

    /// Human label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear { .. } => "Linear",
            Self::Grid { .. } => "Grid",
            Self::Radial { .. } => "Radial",
        }
    }
}

/// Builds one placement, folding in the index-dependent tweens.
fn placement_at(
    index: u32,
    translate: Vec2,
    pivot: Vec2,
    angle: f32,
    tweens: ArrayTweens,
) -> CopyPlacement {
    CopyPlacement {
        angle,
        pivot,
        translate,
        spin: tweens.spin * index as f32,
        scale: tweens.scale_ratio.powf(index as f32),
        depth: tweens.depth * index as f32,
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
    /// Per-copy tweens.
    pub tweens: ArrayTweens,
}

impl ArrayCommand {
    /// Builds an array command with no per-copy tweens.
    pub fn new(sources: Vec<StableId>, count: u32, mode: ArrayMode) -> Self {
        Self::with_tweens(sources, count, mode, ArrayTweens::default())
    }

    /// Builds an array command with tweens.
    pub fn with_tweens(
        sources: Vec<StableId>,
        count: u32,
        mode: ArrayMode,
        tweens: ArrayTweens,
    ) -> Self {
        Self {
            sources,
            count,
            mode,
            tweens,
        }
    }
}

impl GameCommand for ArrayCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let placements = self.mode.placements(self.count, self.tweens);
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
                apply_placement(&mut clone, placement);
                id_map.push((id, clone.id));
                clones.push(clone);
            }
            joint_clones.extend(crate::spawn::clone_internal_joints(
                world,
                &id_map,
                |p| placement.map_point(p),
                placement.body_rotation(),
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

/// Applies one placement to a captured record.
fn apply_placement(clone: &mut BodyRecord, placement: CopyPlacement) {
    clone.pose.pos = placement.map_point(clone.pose.pos);
    clone.pose.rot += placement.body_rotation();
    if (placement.scale - 1.0).abs() > 1e-6 {
        let m = Mat2::from_diagonal(Vec2::splat(placement.scale));
        clone.shape = gradiance_geometry::scale::scale_shape(&clone.shape, m);
    }
    if placement.depth != 0.0 {
        // Shift the whole band, keeping its thickness: a depth *step*, not a
        // stretch. `sanitized` keeps the near face out of negative depth.
        clone.depth = gradiance_domain::depth::DepthBand {
            near: clone.depth.near + placement.depth,
            far: clone.depth.far + placement.depth,
        }
        .sanitized();
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exactly-inert tweens
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn a_linear_array_steps_uniformly() {
        let mode = ArrayMode::Linear {
            step: Vec2::new(2.0, 0.0),
        };
        let places = mode.placements(3, ArrayTweens::default());
        assert_eq!(places.len(), 3, "the original is not a copy");
        for (k, place) in places.iter().enumerate() {
            let expected = Vec2::new(2.0 * (k + 1) as f32, 0.0);
            assert!(place.map_point(Vec2::ZERO).distance(expected) < 1e-5);
        }
    }

    #[test]
    fn a_grid_fills_the_rectangle_minus_the_original() {
        let mode = ArrayMode::Grid {
            step: Vec2::new(1.0, 0.0),
            cross: Vec2::new(0.0, 1.0),
            cross_count: 2,
            stagger: 0.0,
        };
        let places = mode.placements(3, ArrayTweens::default());
        // 4 columns × 3 rows = 12 cells, minus the original.
        assert_eq!(places.len(), 11);
        let corners: Vec<Vec2> = places.iter().map(|p| p.map_point(Vec2::ZERO)).collect();
        assert!(
            corners
                .iter()
                .any(|c| c.distance(Vec2::new(3.0, 2.0)) < 1e-5)
        );
        assert!(
            !corners.iter().any(|c| c.length() < 1e-5),
            "the original's own cell is never emitted"
        );
    }

    #[test]
    fn stagger_offsets_only_the_odd_rows() {
        let mode = ArrayMode::Grid {
            step: Vec2::new(2.0, 0.0),
            cross: Vec2::new(0.0, 1.0),
            cross_count: 2,
            stagger: 0.5,
        };
        let places = mode.placements(1, ArrayTweens::default());
        let at = |row: f32| {
            places
                .iter()
                .map(|p| p.map_point(Vec2::ZERO))
                .filter(move |p| (p.y - row).abs() < 1e-5)
                .map(|p| p.x)
                .collect::<Vec<_>>()
        };
        assert!(
            at(0.0).iter().any(|x| (x - 2.0).abs() < 1e-5),
            "row 0 is unshifted"
        );
        let row1 = at(1.0);
        assert!(
            row1.iter().any(|x| (x - 1.0).abs() < 1e-5),
            "row 1 shifted by half a step: {row1:?}"
        );
    }

    #[test]
    fn a_radial_array_orbits_the_pivot() {
        let mode = ArrayMode::Radial {
            pivot: Vec2::ZERO,
            angle_step: FRAC_PI_2,
            rotate_items: true,
        };
        let places = mode.placements(3, ArrayTweens::default());
        let first = places[0].map_point(Vec2::new(1.0, 0.0));
        assert!(first.distance(Vec2::new(0.0, 1.0)) < 1e-5, "got {first:?}");
        assert!(
            (places[0].body_rotation() - FRAC_PI_2).abs() < 1e-5,
            "rotate_items turns the body with the pattern"
        );
    }

    #[test]
    fn radial_without_rotate_items_keeps_bodies_upright() {
        let mode = ArrayMode::Radial {
            pivot: Vec2::ZERO,
            angle_step: FRAC_PI_2,
            rotate_items: false,
        };
        let places = mode.placements(2, ArrayTweens::default());
        for place in &places {
            assert!(
                place.body_rotation().abs() < 1e-5,
                "the orbit must not turn the body: {}",
                place.body_rotation()
            );
        }
        assert!(
            places[0]
                .map_point(Vec2::new(1.0, 0.0))
                .distance(Vec2::new(0.0, 1.0))
                < 1e-5,
            "...but they still orbit"
        );
    }

    #[test]
    fn tweens_accumulate_with_the_copy_index() {
        let mode = ArrayMode::Linear { step: Vec2::X };
        let tweens = ArrayTweens {
            spin: 0.1,
            scale_ratio: 0.5,
            depth: 0.25,
        };
        let places = mode.placements(3, tweens);
        for (k, place) in places.iter().enumerate() {
            let index = (k + 1) as f32;
            assert!((place.spin - 0.1 * index).abs() < 1e-5);
            assert!((place.scale - 0.5_f32.powf(index)).abs() < 1e-5);
            assert!((place.depth - 0.25 * index).abs() < 1e-5);
        }
    }

    #[test]
    fn default_tweens_are_inert() {
        let places = ArrayMode::Linear { step: Vec2::X }.placements(2, ArrayTweens::default());
        for place in places {
            assert_eq!(place.spin, 0.0);
            assert!((place.scale - 1.0).abs() < 1e-6);
            assert_eq!(place.depth, 0.0);
        }
        assert!(ArrayTweens::default().is_identity());
    }

    #[test]
    fn a_zero_count_pattern_produces_nothing() {
        assert!(
            ArrayMode::Linear { step: Vec2::X }
                .placements(0, ArrayTweens::default())
                .is_empty()
        );
    }
}

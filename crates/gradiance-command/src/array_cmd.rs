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
//!
//! # Per-copy change, with every axis specified on its own
//!
//! A per-copy change ([`TweenStep`]) is indexed by *which way through the
//! pattern* the copy sits, not by a single running counter: [`ArrayTweens`]
//! carries one lane per pattern axis — [`along_x`](ArrayTweens::along_x)
//! driven by the column index, [`along_y`](ArrayTweens::along_y) by the row
//! index. A grid can therefore change across and down independently, which is
//! the whole point of grids. Sizes inside a lane are a `Vec2` ratio for the
//! same reason: "scale x and y separately" has to be sayable, and it is a
//! different question from "which way through the pattern".
//!
//! Both are measured in a **frame** ([`ArrayTweens::basis`] /
//! [`ArrayTweens::origin`]): the selection's own axes and centre at press
//! time, so tapering a rotated selection shrinks it along its own sides rather
//! than the world's.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_geometry::array::geometric_span;
use gradiance_scene::BodyRecord;

/// The tolerance below which a size ratio counts as "1", i.e. inert.
const RATIO_EPS: f32 = 1e-6;

/// One lane of per-copy change: what happens for each step taken along one of
/// the pattern's two axes.
///
/// Additive fields accumulate (`k · spin`); the size ratio compounds
/// (`scale^k`), because "0.99 each copy" is a multiplication, not a
/// subtraction. Every field is inert at its default, so a lane that has never
/// been touched costs nothing and changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct TweenStep {
    /// Extra rotation per step, radians, about the copy's own centre.
    pub spin: f32,
    /// Size ratio per step, **one factor per frame axis**. `(1, 1)` is inert;
    /// `(0.99, 0.99)` shrinks each copy by a percent; `(0.99, 1.0)` narrows
    /// without flattening.
    pub scale: Vec2,
    /// Depth-band shift per step, world units into the screen — a staircase
    /// *through* the 2.5D layers rather than across them, which also means
    /// successive copies stop colliding once the step exceeds a layer.
    pub depth: f32,
}

impl Default for TweenStep {
    fn default() -> Self {
        Self {
            spin: 0.0,
            scale: Vec2::ONE,
            depth: 0.0,
        }
    }
}

impl TweenStep {
    /// Whether this lane changes anything.
    pub fn is_identity(&self) -> bool {
        self.spin == 0.0 && !scales(self.scale) && self.depth == 0.0
    }

    /// This lane's cumulative effect after `k` steps.
    fn after(&self, k: u32) -> (f32, Vec2, f32) {
        let k = k as f32;
        (
            self.spin * k,
            Vec2::new(self.scale.x.powf(k), self.scale.y.powf(k)),
            self.depth * k,
        )
    }

    /// A copy with every field forced into a sane range.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        Self {
            spin: finite(self.spin, 0.0),
            scale: Vec2::new(
                finite(self.scale.x, 1.0).clamp(0.05, 20.0),
                finite(self.scale.y, 1.0).clamp(0.05, 20.0),
            ),
            depth: finite(self.depth, 0.0).clamp(-100.0, 100.0),
        }
    }
}

/// Whether a per-axis ratio actually resizes anything.
fn scales(ratio: Vec2) -> bool {
    (ratio - Vec2::ONE).abs().max_element() > RATIO_EPS
}

/// Per-copy changes that accumulate along the pattern, on top of whatever
/// rigid placement the mode produces.
///
/// These are what turn a plain repeat into a *pattern*: a fan of blades, a
/// tapering spiral, a staircase that walks back into the scene. They apply to
/// every mode, because they are about the copy's index rather than about
/// where the copy sits.
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Default)]
pub struct ArrayTweens {
    /// Applied once per step along the frame's X axis — a row's copies or a
    /// grid's columns.
    pub along_x: TweenStep,
    /// Applied once per step along the frame's Y axis — a column's copies or
    /// a grid's rows. Inert for patterns that never move that way.
    pub along_y: TweenStep,
    /// Centre the per-axis size ratios act about: the selection's centre, so
    /// a shrinking copy shrinks toward its own middle.
    pub origin: Vec2,
    /// Rotation of the axes the per-axis ratios are measured in (radians).
    /// The selection frame at press time; 0 is world axes.
    ///
    /// Set by the tool rather than the options panel — "x" in the panel means
    /// the selection's own x.
    pub basis: f32,
}

impl ArrayTweens {
    /// Whether every tween is inert (lets callers skip work).
    pub fn is_identity(&self) -> bool {
        self.along_x.is_identity() && self.along_y.is_identity()
    }

    /// The size ratio applied to the copy `(col, row)` steps into the pattern.
    pub fn scale_at(&self, col: u32, row: u32) -> Vec2 {
        self.along_x.after(col).1 * self.along_y.after(row).1
    }

    /// Both lanes clamped into sane ranges, keeping the frame as-is.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            along_x: self.along_x.sanitized(),
            along_y: self.along_y.sanitized(),
            origin: self.origin,
            basis: if self.basis.is_finite() {
                self.basis
            } else {
                0.0
            },
        }
    }
}

/// How array copies are placed.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub enum ArrayMode {
    /// Copies at `step`, `step·(1+r)`, … along one axis.
    Linear {
        /// Translation to the first copy.
        step: Vec2,
        /// How much each successive step shrinks — 1.0 is a uniform array.
        /// Set from the size taper when spacing tracks contact, so shrinking
        /// copies stay flush.
        ratio: f32,
        /// Whether this array runs along the frame's Y axis, and is therefore
        /// driven by [`ArrayTweens::along_y`] rather than `along_x`.
        axis_y: bool,
    },
    /// A two-axis grid: `count` copies along `step`, `cross_count` along
    /// `cross`, filling the rectangle they span.
    Grid {
        /// Translation to the first column.
        step: Vec2,
        /// Translation to the first row.
        cross: Vec2,
        /// Extra rows beyond the original's.
        cross_count: u32,
        /// Fraction of the local column pitch that alternate rows are offset
        /// by — 0 for a plain grid, 0.5 for a running-bond brick wall.
        stagger: f32,
        /// Per-axis shrink of the column pitch per column (the X lane's
        /// size ratio, or `ONE` when spacing ignores the taper).
        ratio: Vec2,
        /// Per-axis shrink of the row pitch per row (the Y lane's).
        cross_ratio: Vec2,
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
    /// Per-axis size ratio for this copy, in the [`basis`](Self::basis) frame,
    /// about [`origin`](Self::origin).
    pub scale: Vec2,
    /// Centre the size ratio acts about.
    pub origin: Vec2,
    /// Rotation of the axes `scale` is measured in, radians.
    pub basis: f32,
    /// Depth-band shift, world units into the screen.
    pub depth: f32,
}

impl CopyPlacement {
    /// Maps a world point through this copy's placement — resize first, then
    /// the pattern's rigid move.
    pub fn map_point(&self, p: Vec2) -> Vec2 {
        let resized = if self.scales() {
            gradiance_geometry::scale::scale_point(p, self.origin, self.basis, self.scale)
        } else {
            p
        };
        self.pivot + Vec2::from_angle(self.angle).rotate(resized - self.pivot) + self.translate
    }

    /// The total rotation a body under this placement receives.
    pub fn body_rotation(&self) -> f32 {
        self.angle + self.spin
    }

    /// Whether this copy is a different size from the original.
    pub fn scales(&self) -> bool {
        scales(self.scale)
    }

    /// The body-local linear map that resizes a body currently rotated by
    /// `body_rot`.
    ///
    /// The size ratio is expressed in the pattern's frame, so for a body at
    /// some other angle it is a general (possibly shearing) map — which is
    /// exactly what [`gradiance_geometry::scale`] exists to apply exactly.
    pub fn body_matrix(&self, body_rot: f32) -> Mat2 {
        gradiance_geometry::scale::body_scale_matrix(body_rot, self.basis, self.scale)
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
            Self::Linear {
                step,
                ratio,
                axis_y,
            } => {
                for k in 1..=count {
                    // Partial sum rather than `k · step`: a tapered array's
                    // steps shrink with the copies, so the k-th copy sits at
                    // `step · (1 + r + … + r^(k-1))`.
                    let translate = step * geometric_span(ratio, k);
                    // A column of copies is driven by the Y lane, a row by the
                    // X lane — so the options panel's "Y" always means the
                    // direction you can see the array running.
                    let (col, row) = if axis_y { (0, k) } else { (k, 0) };
                    out.push(placement_at(col, row, translate, Vec2::ZERO, 0.0, tweens));
                }
            }
            Self::Grid {
                step,
                cross,
                cross_count,
                stagger,
                ratio,
                cross_ratio,
            } => {
                for row in 0..=cross_count {
                    for col in 0..=count {
                        // (0, 0) is the original, not a copy.
                        if row == 0 && col == 0 {
                            continue;
                        }
                        // Both lanes resize, so the column pitch inside row
                        // `r` carries the row lane's x-shrink, and the row
                        // pitch in column `c` carries the column lane's
                        // y-shrink. Those cross terms are what keep a
                        // doubly-tapered grid consistent (and flush).
                        let row_x = cross_ratio.x.powf(row as f32);
                        let col_y = ratio.y.powf(col as f32);
                        let along = row_x * geometric_span(ratio.x, col);
                        let across = col_y * geometric_span(cross_ratio.y, row);
                        // Odd rows shift by a fraction of the *local* pitch —
                        // this is what makes a running bond rather than a
                        // stack bond, and it has to taper with the cells.
                        let offset = if row % 2 == 1 {
                            stagger * row_x * ratio.x.powf(col as f32)
                        } else {
                            0.0
                        };
                        let translate = step * (along + offset) + cross * across;
                        out.push(placement_at(col, row, translate, Vec2::ZERO, 0.0, tweens));
                    }
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
        }
    }
}

/// Builds one placement, folding in both lanes' tweens at the copy's
/// `(col, row)` position in the pattern.
fn placement_at(
    col: u32,
    row: u32,
    translate: Vec2,
    pivot: Vec2,
    angle: f32,
    tweens: ArrayTweens,
) -> CopyPlacement {
    let (spin_a, scale_a, depth_a) = tweens.along_x.after(col);
    let (spin_b, scale_b, depth_b) = tweens.along_y.after(row);
    CopyPlacement {
        angle,
        pivot,
        translate,
        spin: spin_a + spin_b,
        scale: scale_a * scale_b,
        origin: tweens.origin,
        basis: tweens.basis,
        depth: depth_a + depth_b,
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
    if placement.scales() {
        // Resize in the body's *current* frame, before the pattern rotates
        // it — the same decomposition `map_point` uses, so ghost and result
        // cannot disagree.
        clone.shape = gradiance_geometry::scale::scale_shape(
            &clone.shape,
            placement.body_matrix(clone.pose.rot),
        );
    }
    clone.pose.pos = placement.map_point(clone.pose.pos);
    clone.pose.rot += placement.body_rotation();
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

    #[test]
    fn a_linear_array_steps_uniformly() {
        let mode = ArrayMode::Linear {
            step: Vec2::new(2.0, 0.0),
            ratio: 1.0,
            axis_y: false,
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
            ratio: Vec2::ONE,
            cross_ratio: Vec2::ONE,
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
            ratio: Vec2::ONE,
            cross_ratio: Vec2::ONE,
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
    fn tweens_accumulate_with_the_copy_index() {
        let mode = ArrayMode::Linear {
            step: Vec2::X,
            ratio: 1.0,
            axis_y: false,
        };
        let tweens = ArrayTweens {
            along_x: TweenStep {
                spin: 0.1,
                scale: Vec2::splat(0.5),
                depth: 0.25,
            },
            ..Default::default()
        };
        let places = mode.placements(3, tweens);
        for (k, place) in places.iter().enumerate() {
            let index = (k + 1) as f32;
            assert!((place.spin - 0.1 * index).abs() < 1e-5);
            assert!((place.scale - Vec2::splat(0.5_f32.powf(index))).length() < 1e-5);
            assert!((place.depth - 0.25 * index).abs() < 1e-5);
        }
    }

    #[test]
    fn each_axis_of_the_size_tween_moves_on_its_own() {
        // The user-facing promise: "scale x and y separately". A lane that
        // narrows must not also flatten.
        let mode = ArrayMode::Linear {
            step: Vec2::X,
            ratio: 1.0,
            axis_y: false,
        };
        let tweens = ArrayTweens {
            along_x: TweenStep {
                scale: Vec2::new(0.5, 2.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let places = mode.placements(2, tweens);
        assert!((places[0].scale - Vec2::new(0.5, 2.0)).length() < 1e-5);
        assert!((places[1].scale - Vec2::new(0.25, 4.0)).length() < 1e-5);
    }

    #[test]
    fn a_grid_drives_the_two_lanes_from_its_two_indices() {
        // The other half of the promise: in a grid, the column lane indexes
        // by column and the row lane by row, so a grid can taper across and
        // down independently instead of by a single diagonal counter.
        let mode = ArrayMode::Grid {
            step: Vec2::X,
            cross: Vec2::Y,
            cross_count: 2,
            stagger: 0.0,
            ratio: Vec2::ONE,
            cross_ratio: Vec2::ONE,
        };
        let tweens = ArrayTweens {
            along_x: TweenStep {
                scale: Vec2::new(0.5, 1.0),
                ..Default::default()
            },
            along_y: TweenStep {
                scale: Vec2::new(1.0, 0.5),
                ..Default::default()
            },
            ..Default::default()
        };
        let places = mode.placements(2, tweens);
        let at = |col: f32, row: f32| {
            places
                .iter()
                .find(|p| (p.translate.x - col).abs() < 1e-4 && (p.translate.y - row).abs() < 1e-4)
                .expect("that cell exists")
        };
        // Two columns across: x halved twice, y untouched.
        assert!((at(2.0, 0.0).scale - Vec2::new(0.25, 1.0)).length() < 1e-5);
        // Two rows down: y halved twice, x untouched.
        assert!((at(0.0, 2.0).scale - Vec2::new(1.0, 0.25)).length() < 1e-5);
        // The far corner carries both.
        assert!((at(2.0, 2.0).scale - Vec2::new(0.25, 0.25)).length() < 1e-5);
    }

    #[test]
    fn a_tapered_linear_array_closes_its_steps_geometrically() {
        // Contact spacing under a taper: each step is `ratio` times the last,
        // so copy k sits at the partial sum rather than at `k · step`.
        let mode = ArrayMode::Linear {
            step: Vec2::X * 2.0,
            ratio: 0.5,
            axis_y: false,
        };
        let places = mode.placements(3, ArrayTweens::default());
        let x = |i: usize| places[i].translate.x;
        assert!((x(0) - 2.0).abs() < 1e-5);
        assert!((x(1) - 3.0).abs() < 1e-5, "2 + 1");
        assert!((x(2) - 3.5).abs() < 1e-5, "2 + 1 + 0.5");
    }

    #[test]
    fn default_tweens_are_inert() {
        let places = ArrayMode::Linear {
            step: Vec2::X,
            ratio: 1.0,
            axis_y: false,
        }
        .placements(2, ArrayTweens::default());
        for place in places {
            assert_eq!(place.spin, 0.0);
            assert!(!place.scales());
            assert_eq!(place.depth, 0.0);
        }
        assert!(ArrayTweens::default().is_identity());
    }

    #[test]
    fn a_rotated_basis_tapers_along_the_selections_own_axes() {
        // "x" in the options panel means the selection's x, not the world's.
        let quarter = std::f32::consts::FRAC_PI_2;
        let place = CopyPlacement {
            angle: 0.0,
            pivot: Vec2::ZERO,
            translate: Vec2::ZERO,
            spin: 0.0,
            scale: Vec2::new(0.5, 1.0),
            origin: Vec2::ZERO,
            basis: quarter,
            depth: 0.0,
        };
        // The frame's +x is world +y here, so it is *y* that halves.
        let moved = place.map_point(Vec2::new(0.0, 2.0));
        assert!(moved.distance(Vec2::new(0.0, 1.0)) < 1e-4, "got {moved:?}");
        let across = place.map_point(Vec2::new(2.0, 0.0));
        assert!(
            across.distance(Vec2::new(2.0, 0.0)) < 1e-4,
            "got {across:?}"
        );
    }

    #[test]
    fn sanitizing_pulls_a_hostile_tween_back_into_range() {
        let wild = ArrayTweens {
            along_x: TweenStep {
                spin: f32::NAN,
                scale: Vec2::new(0.0, 1e9),
                depth: f32::INFINITY,
            },
            along_y: TweenStep::default(),
            origin: Vec2::ZERO,
            basis: f32::NAN,
        }
        .sanitized();
        assert_eq!(wild.along_x.spin, 0.0);
        assert!(
            wild.along_x.scale.x >= 0.05,
            "a zero ratio would erase bodies"
        );
        assert!(wild.along_x.scale.y <= 20.0);
        assert!(wild.along_x.depth.is_finite());
        assert_eq!(wild.basis, 0.0);
    }

    #[test]
    fn a_zero_count_pattern_produces_nothing() {
        assert!(
            ArrayMode::Linear {
                step: Vec2::X,
                ratio: 1.0,
                axis_y: false,
            }
            .placements(0, ArrayTweens::default())
            .is_empty()
        );
    }
}

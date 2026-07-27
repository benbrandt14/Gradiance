//! The array drag: turn a selection into a repeated pattern by dragging a
//! scale handle with `Alt` held.
//!
//! # Why it rides the scale handles
//!
//! An array is a bounding-box operation in exactly the way scaling is: you
//! grab a side and pull, and the box grows along that side. Reusing the same
//! eight handles means the affordance is already on screen, already
//! frame-aware (global or local axes), and already understood — the only new
//! thing to learn is the modifier. It also makes the two operations read as
//! siblings, which they are: scale stretches the content, array repeats it.
//!
//! Edge handles array along one axis; corner handles array along both and
//! produce a grid.
//!
//! # Contact spacing is the default, and it is exact
//!
//! The point of the feature is that dragging a single block sideways gives a
//! seamless wall, and dragging a two-block stack upward gives a seamless
//! tower. That needs the *smallest step that clears the selection from
//! itself* along the drag axis, which
//! [`gradiance_geometry::array::contact_pitch`] computes exactly (for convex
//! pieces) rather than approximating with the bounding box. The difference
//! shows the moment a selection can interlock with its own copy: a staircase
//! of two blocks steps by one block, not two.
//!
//! The count then falls out of the drag: how many whole pitches fit in the
//! distance pulled.
//!
//! # Gesture contract
//!
//! Same as every other drag. While the pointer is down this writes nothing
//! but gizmos — the ghost outlines are computed from the *same*
//! [`CopyPlacement`] list the command will use, so the preview cannot drift
//! from the result. Release emits exactly one
//! [`ArrayIntent`](gradiance_command::intent::ArrayIntent), so an array of
//! two hundred blocks is one undo step.

use bevy::ecs::reflect::ReflectResource;
use bevy::math::Vec2;
use bevy::prelude::{Reflect, Resource};
use gradiance_command::array_cmd::{ArrayMode, ArrayTweens, CopyPlacement};
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::array::{copies_within, extent_along, tapered_contact_pitch};

use crate::tools::handles::{HandleKind, SelectionBox};

/// The array tool's rulebook.
///
/// Editor configuration, not authored scene state — the same carve-out
/// `PackConfig` sits in, and edited the same way (UI writes it directly
/// through the Config seam). It is deliberately *not* part of the saved
/// document: like `ToolDefaults`, it describes how the tool behaves, not what
/// the scene contains.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource)]
pub struct ArrayConfig {
    /// Which family of repetition a drag produces.
    pub pattern: ArrayPattern,
    /// How far apart copies sit.
    pub spacing: ArraySpacing,
    /// Fraction of a step that alternate grid rows are offset by — 0.5 gives
    /// a running-bond brick wall.
    pub stagger: f32,
    /// Angle between copies for [`ArrayPattern::Radial`], radians.
    pub angle_step: f32,
    /// Whether a radial sweep turns each body with the pattern.
    pub rotate_items: bool,
    /// Ignore the drag distance and always make this many copies.
    pub count_override: Option<u32>,
    /// Per-copy parameter change, one lane per pattern axis and each size
    /// axis specified on its own.
    pub tweens: ArrayTweens,
}

impl Default for ArrayConfig {
    fn default() -> Self {
        Self {
            pattern: ArrayPattern::default(),
            spacing: ArraySpacing::default(),
            stagger: 0.0,
            angle_step: std::f32::consts::TAU / 8.0,
            rotate_items: true,
            count_override: None,
            tweens: ArrayTweens::default(),
        }
    }
}

impl ArrayConfig {
    /// A copy with every field forced into a sane range, so a hand-edited or
    /// scripted config cannot ask for something degenerate.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let mut out = self.clone();
        out.stagger = finite(out.stagger, 0.0).clamp(-1.0, 1.0);
        out.angle_step = finite(out.angle_step, std::f32::consts::TAU / 8.0)
            .clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
        out.count_override = out.count_override.map(|c| c.clamp(1, MAX_COPIES_PER_AXIS));
        out.tweens = out.tweens.sanitized();
        out.spacing = match out.spacing {
            ArraySpacing::Gap(g) => ArraySpacing::Gap(finite(g, 0.0).clamp(-100.0, 100.0)),
            ArraySpacing::Fixed(s) => ArraySpacing::Fixed(finite(s, 1.0).clamp(1e-3, 1000.0)),
            ArraySpacing::Multiple(m) => ArraySpacing::Multiple(finite(m, 1.0).clamp(0.05, 100.0)),
            ArraySpacing::Contact => ArraySpacing::Contact,
        };
        out
    }
}

/// The largest number of copies a single drag may produce, per axis.
///
/// A guard rail, not a preference: the drag distance divided by a pitch that
/// happens to be tiny (a sliver of a body) can otherwise ask for hundreds of
/// thousands of bodies from one flick of the wrist.
pub const MAX_COPIES_PER_AXIS: u32 = 512;

/// Everything the gesture needs to know about the selection's geometry.
///
/// Measured from the selection's hulls (once per drag frame, since the taper
/// the options panel is showing can change under the pointer).
#[derive(Debug, Clone, Default)]
pub struct ArrayMetrics {
    /// Flush pitch along the frame's +X axis, taper included: the step that
    /// clears the *first* copy from the original. Later steps shrink from it
    /// geometrically — see [`gradiance_geometry::array`].
    pub pitch_x: f32,
    /// Flush pitch along the frame's +Y axis, taper included.
    pub pitch_y: f32,
    /// How much each successive column pitch shrinks (1.0 when untapered).
    pub ratio_x: f32,
    /// How much each successive row pitch shrinks.
    pub ratio_y: f32,
}

impl ArrayMetrics {
    /// Measures the selection's contact pitch along both frame axes, with no
    /// per-copy taper.
    ///
    /// `pieces` are world-space convex outlines, one per body.
    pub fn measure(pieces: &[Vec<Vec2>], frame_rot: f32) -> Self {
        Self::measure_tapered(pieces, frame_rot, Vec2::ZERO, &ArrayTweens::default())
    }

    /// Measures the pitch a *tapered* array needs so its copies stay flush.
    ///
    /// The column lane resizes the columns and the row lane the rows, so each
    /// axis gets its own first-gap measurement and its own shrink ratio. When
    /// the taper is inert this reduces exactly to [`measure`](Self::measure).
    pub fn measure_tapered(
        pieces: &[Vec<Vec2>],
        frame_rot: f32,
        origin: Vec2,
        tweens: &ArrayTweens,
    ) -> Self {
        let x = Vec2::from_angle(frame_rot);
        let y = Vec2::from_angle(frame_rot + std::f32::consts::FRAC_PI_2);
        let pitch = |dir: Vec2, factors: Vec2| -> f32 {
            match tapered_contact_pitch(pieces, dir, factors, origin, frame_rot) {
                Some(p) if p > 1e-6 => p,
                // Same fallback as the untapered path: a set that never
                // overlaps itself still needs a usable step.
                _ => extent_along(pieces, dir),
            }
        };
        Self {
            pitch_x: pitch(x, tweens.along_x.scale),
            pitch_y: pitch(y, tweens.along_y.scale),
            // The pitch along an axis shrinks with that lane's own ratio for
            // that axis: a row closes up as its copies narrow, a column as
            // its copies flatten.
            ratio_x: tweens.along_x.scale.x,
            ratio_y: tweens.along_y.scale.y,
        }
    }

    /// The step along an axis, with the configured spacing rule applied.
    pub fn step(&self, axis_y: bool, config: &ArrayConfig) -> f32 {
        let contact = if axis_y { self.pitch_y } else { self.pitch_x };
        config.spacing.step(contact)
    }

    /// Measures for a live drag: the selection's own frame and the taper the
    /// options panel is currently showing.
    ///
    /// Re-measured each frame rather than frozen at press, so turning the
    /// taper up mid-drag closes the copies up under the pointer instead of
    /// waiting for the next gesture.
    pub fn for_drag(pieces: &[Vec<Vec2>], sbox: &SelectionBox, config: &ArrayConfig) -> Self {
        Self::measure_tapered(pieces, sbox.rot, sbox.center, &config.tweens)
    }

    /// The pitch shrink along an axis, or 1.0 when the spacing rule does not
    /// track the geometry (a fixed step stays fixed however the copies grow).
    pub fn ratio(&self, axis_y: bool, config: &ArrayConfig) -> f32 {
        if !config.spacing.tracks_contact() {
            return 1.0;
        }
        let r = if axis_y { self.ratio_y } else { self.ratio_x };
        if r.is_finite() && r > 0.0 { r } else { 1.0 }
    }
}

/// How far apart copies sit, on top of the measured contact pitch.
#[derive(Debug, Clone, Copy, PartialEq, Reflect, Default)]
pub enum ArraySpacing {
    /// Flush: copies just touch. The default, and the reason the feature
    /// exists.
    #[default]
    Contact,
    /// Flush plus a fixed gap, in metres. Negative overlaps deliberately.
    Gap(f32),
    /// An explicit step, ignoring the geometry entirely.
    Fixed(f32),
    /// Contact pitch scaled by a factor — 2.0 leaves a body-sized hole
    /// between copies, 0.5 interleaves them.
    Multiple(f32),
}

impl ArraySpacing {
    /// Every variant at its default value, for UI enumeration.
    pub const ALL: [Self; 4] = [
        Self::Contact,
        Self::Gap(0.05),
        Self::Fixed(1.0),
        Self::Multiple(2.0),
    ];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contact => "Contact (flush)",
            Self::Gap(_) => "Contact + gap",
            Self::Fixed(_) => "Fixed step",
            Self::Multiple(_) => "Multiple of contact",
        }
    }

    /// Whether the step this rule produces is derived from the selection's
    /// geometry — and therefore whether it should shrink along with a size
    /// taper to keep copies flush.
    ///
    /// [`Fixed`](Self::Fixed) is the odd one out on purpose: an explicit step
    /// means an explicit step, even when the copies are changing size.
    pub fn tracks_contact(self) -> bool {
        !matches!(self, Self::Fixed(_))
    }

    /// The step this rule produces from a measured contact pitch.
    ///
    /// Never returns a non-positive step: a zero or negative pitch would make
    /// the count computation divide by zero and ask for an unbounded number
    /// of copies stacked in one place.
    pub fn step(self, contact: f32) -> f32 {
        let step = match self {
            Self::Contact => contact,
            Self::Gap(g) => contact + g,
            Self::Fixed(s) => s,
            Self::Multiple(m) => contact * m,
        };
        if step.is_finite() && step > 1e-4 {
            step
        } else {
            // Fall back to something usable rather than refusing to array.
            contact.max(1e-3)
        }
    }
}

/// A fully resolved array, ready to preview or commit.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayPlan {
    /// Copies along the primary axis.
    pub count: u32,
    /// Extra rows along the cross axis (grid only).
    pub cross_count: u32,
    /// The pattern to hand the command.
    pub mode: ArrayMode,
    /// Per-copy tweens.
    pub tweens: ArrayTweens,
}

impl ArrayPlan {
    /// Whether this plan would create anything.
    pub fn is_empty(&self) -> bool {
        self.placements().is_empty()
    }

    /// Total number of copies.
    pub fn total(&self) -> usize {
        self.placements().len()
    }

    /// The exact placements the command will use — the preview draws these,
    /// so what you see is what you get.
    pub fn placements(&self) -> Vec<CopyPlacement> {
        self.mode.placements(self.count, self.tweens)
    }
}

/// Builds the plan for a drag of `delta_frame` (frame-local) on `handle`.
///
/// The handle decides which axes participate: an edge handle arrays along its
/// own axis only, a corner handle along both. Dragging *inward* (against the
/// handle's outward direction) produces nothing rather than copies stacked
/// behind the original — pulling a handle back is how you cancel.
pub fn plan_drag(
    handle: HandleKind,
    sbox: &SelectionBox,
    metrics: &ArrayMetrics,
    delta_frame: Vec2,
    config: &ArrayConfig,
) -> Option<ArrayPlan> {
    let unit = handle.unit();
    let (uses_x, uses_y) = handle.scales();
    let step_x = metrics.step(false, config);
    let step_y = metrics.step(true, config);
    let ratio_x = metrics.ratio(false, config);
    let ratio_y = metrics.ratio(true, config);

    // Copies fit along each participating axis, in the handle's outward
    // direction only. A taper makes the steps a geometric series rather than a
    // constant, so this is not a plain division — and a converging taper has a
    // finite reach however far you pull.
    let count_along = |active: bool, sign: f32, delta: f32, step: f32, ratio: f32| -> u32 {
        if !active || sign == 0.0 || step <= 0.0 {
            return 0;
        }
        copies_within(delta * sign, step, ratio, MAX_COPIES_PER_AXIS)
    };
    let n_x = count_along(uses_x, unit.x, delta_frame.x, step_x, ratio_x);
    let n_y = count_along(uses_y, unit.y, delta_frame.y, step_y, ratio_y);

    let axis_x = Vec2::from_angle(sbox.rot) * step_x * unit.x.signum();
    let axis_y =
        Vec2::from_angle(sbox.rot + std::f32::consts::FRAC_PI_2) * step_y * unit.y.signum();
    // The spacing ratios are per-axis because the two lanes resize
    // independently: a grid column's pitch also follows the *row* lane's
    // x-shrink, and vice versa.
    let taper = config.spacing.tracks_contact();
    let lane = |scale: Vec2| if taper { scale } else { Vec2::ONE };

    let mode = match config.pattern {
        ArrayPattern::Repeat if uses_x && uses_y => ArrayMode::Grid {
            step: axis_x,
            cross: axis_y,
            cross_count: n_y,
            stagger: config.stagger,
            ratio: lane(config.tweens.along_x.scale),
            cross_ratio: lane(config.tweens.along_y.scale),
        },
        ArrayPattern::Repeat => {
            // One axis only: whichever the handle owns, and the lane that
            // drives it is the one named after that axis.
            let (step, ratio) = if uses_y {
                (axis_y, ratio_y)
            } else {
                (axis_x, ratio_x)
            };
            ArrayMode::Linear {
                step,
                ratio,
                axis_y: uses_y && !uses_x,
            }
        }
        ArrayPattern::Radial => ArrayMode::Radial {
            pivot: sbox.center,
            angle_step: config.angle_step,
            rotate_items: config.rotate_items,
        },
    };

    let count = match mode {
        // A grid's primary axis is X; a linear array's is whichever axis the
        // handle owns; a radial array counts the same either way.
        ArrayMode::Grid { .. } => n_x,
        ArrayMode::Linear { .. } => {
            if uses_y && !uses_x {
                n_y
            } else {
                n_x
            }
        }
        ArrayMode::Radial { .. } => n_x.max(n_y),
    };
    let count = config.count_override.unwrap_or(count);
    let cross_count = if matches!(mode, ArrayMode::Grid { .. }) {
        n_y
    } else {
        0
    };

    // The per-axis tweens are stated in the selection's own frame, so the
    // frame travels with them into the command — "x" in the options panel
    // means the selection's x, whichever way it is turned.
    let mut tweens = config.tweens;
    tweens.origin = sbox.center;
    tweens.basis = sbox.rot;

    let plan = ArrayPlan {
        count,
        cross_count,
        mode,
        tweens,
    };
    (!plan.is_empty()).then_some(plan)
}

/// Which family of repetition a drag produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum ArrayPattern {
    /// Translate copies along the dragged axes — a row, a column, or a grid.
    #[default]
    Repeat,
    /// Sweep copies around the selection's centre.
    Radial,
}

impl ArrayPattern {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 2] = [Self::Repeat, Self::Radial];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Repeat => "Repeat (row / grid)",
            Self::Radial => "Radial sweep",
        }
    }
}

/// The world-space convex outlines of a selection, for pitch measurement.
///
/// Each body contributes the convex hull of its contour. Hulls (rather than
/// raw contours) because the pitch computation is exact only for convex
/// pieces — see [`gradiance_geometry::array`] for what that costs.
pub fn selection_pieces(shapes: &[(ShapeDef, PosRot)]) -> Vec<Vec<Vec2>> {
    shapes
        .iter()
        .filter(|(shape, _)| !shape.contains_half_plane())
        .filter_map(|(shape, pose)| {
            let (sin, cos) = pose.rot.sin_cos();
            let pts: Vec<Vec2> = gradiance_geometry::polygonize::polygonize(shape)
                .rings()
                .flatten()
                .map(|v| {
                    Vec2::new(
                        pose.pos.x + v.x * cos - v.y * sin,
                        pose.pos.y + v.x * sin + v.y * cos,
                    )
                })
                .collect();
            let hull = gradiance_geometry::hull::convex_hull(&pts);
            (hull.len() >= 3).then_some(hull)
        })
        .collect()
}

/// The ids and `(shape, pose)` pairs an array gesture works from.
#[derive(Debug, Clone, Default)]
pub struct ArraySources {
    /// Bodies to pattern.
    pub ids: Vec<StableId>,
    /// Their shapes and poses, for measurement and ghosts.
    pub shapes: Vec<(ShapeDef, PosRot)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_geometry::array::extent_along;

    fn unit_square(center: Vec2, half: f32) -> Vec<Vec2> {
        vec![
            center + Vec2::new(-half, -half),
            center + Vec2::new(half, -half),
            center + Vec2::new(half, half),
            center + Vec2::new(-half, half),
        ]
    }

    fn sbox(half: Vec2) -> SelectionBox {
        SelectionBox {
            center: Vec2::ZERO,
            rot: 0.0,
            half,
        }
    }

    #[test]
    fn a_single_block_measures_its_own_size_as_the_pitch() {
        let pieces = vec![unit_square(Vec2::ZERO, 0.5)];
        let m = ArrayMetrics::measure(&pieces, 0.0);
        assert!((m.pitch_x - 1.0).abs() < 1e-4);
        assert!((m.pitch_y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_two_block_stack_measures_the_whole_stack_vertically() {
        // The user's example: two blocks, one on the other. Dragging up must
        // step by two blocks so the tower has no gaps and no overlaps.
        let pieces = vec![
            unit_square(Vec2::new(0.0, 0.5), 0.5),
            unit_square(Vec2::new(0.0, 1.5), 0.5),
        ];
        let m = ArrayMetrics::measure(&pieces, 0.0);
        assert!((m.pitch_y - 2.0).abs() < 1e-4, "got {}", m.pitch_y);
        assert!(
            (m.pitch_x - 1.0).abs() < 1e-4,
            "sideways is still one block"
        );
    }

    #[test]
    fn dragging_a_side_handle_counts_whole_pitches() {
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let config = ArrayConfig::default();
        let plan = plan_drag(
            HandleKind::EdgeX(1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(3.4, 0.0),
            &config,
        )
        .expect("a plan");
        assert_eq!(plan.count, 3, "3.4 m of drag fits three 1 m copies");
        assert_eq!(plan.total(), 3);
        assert!(matches!(plan.mode, ArrayMode::Linear { .. }));
    }

    #[test]
    fn dragging_inward_produces_nothing() {
        // Pulling a handle back through the box is how you cancel mid-drag;
        // it must not stack copies behind the original.
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        assert!(
            plan_drag(
                HandleKind::EdgeX(1),
                &sbox(Vec2::splat(0.5)),
                &metrics,
                Vec2::new(-3.0, 0.0),
                &ArrayConfig::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn a_left_handle_arrays_leftward() {
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let plan = plan_drag(
            HandleKind::EdgeX(-1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(-2.5, 0.0),
            &ArrayConfig::default(),
        )
        .expect("a plan");
        assert_eq!(plan.count, 2);
        let first = plan.placements()[0].map_point(Vec2::ZERO);
        assert!(first.x < 0.0, "copies go left, got {first:?}");
    }

    #[test]
    fn a_corner_handle_makes_a_grid() {
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 2.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let plan = plan_drag(
            HandleKind::Corner(1, 1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(3.0, 4.5),
            &ArrayConfig::default(),
        )
        .expect("a plan");
        assert_eq!(plan.count, 3, "three columns");
        assert_eq!(plan.cross_count, 2, "4.5 / 2.0 = two rows");
        // 4 columns × 3 rows minus the original.
        assert_eq!(plan.total(), 11);
    }

    #[test]
    fn spacing_rules_change_the_step_but_not_the_geometry() {
        let contact = 2.0;
        assert!((ArraySpacing::Contact.step(contact) - 2.0).abs() < 1e-6);
        assert!((ArraySpacing::Gap(0.5).step(contact) - 2.5).abs() < 1e-6);
        assert!((ArraySpacing::Fixed(0.75).step(contact) - 0.75).abs() < 1e-6);
        assert!((ArraySpacing::Multiple(1.5).step(contact) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_degenerate_spacing_rule_never_yields_a_zero_step() {
        // A zero step would divide by zero in the count and ask for an
        // unbounded pile of copies in one place.
        for rule in [
            ArraySpacing::Fixed(0.0),
            ArraySpacing::Fixed(-3.0),
            ArraySpacing::Multiple(0.0),
            ArraySpacing::Gap(-10.0),
        ] {
            assert!(rule.step(1.0) > 0.0, "{rule:?} produced a bad step");
        }
    }

    #[test]
    fn the_copy_count_is_capped_however_far_you_drag() {
        let metrics = ArrayMetrics {
            pitch_x: 1e-4,
            pitch_y: 1e-4,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let plan = plan_drag(
            HandleKind::EdgeX(1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(1e6, 0.0),
            &ArrayConfig::default(),
        )
        .expect("a plan");
        assert_eq!(plan.count, MAX_COPIES_PER_AXIS);
    }

    #[test]
    fn a_count_override_wins_over_the_drag() {
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let config = ArrayConfig {
            count_override: Some(7),
            ..Default::default()
        };
        let plan = plan_drag(
            HandleKind::EdgeX(1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(2.0, 0.0),
            &config,
        )
        .expect("a plan");
        assert_eq!(plan.count, 7);
    }

    #[test]
    fn the_radial_pattern_sweeps_about_the_selection_centre() {
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let config = ArrayConfig {
            pattern: ArrayPattern::Radial,
            angle_step: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let plan = plan_drag(
            HandleKind::Corner(1, 1),
            &sbox(Vec2::splat(0.5)),
            &metrics,
            Vec2::new(3.0, 0.0),
            &config,
        )
        .expect("a plan");
        assert!(matches!(plan.mode, ArrayMode::Radial { .. }));
        assert_eq!(plan.count, 3);
    }

    #[test]
    fn a_rotated_frame_arrays_along_the_rotated_axis() {
        // Local-frame scaling rotates the handles; the array has to follow.
        let quarter = std::f32::consts::FRAC_PI_2;
        let metrics = ArrayMetrics {
            pitch_x: 1.0,
            pitch_y: 1.0,
            ratio_x: 1.0,
            ratio_y: 1.0,
        };
        let rotated = SelectionBox {
            center: Vec2::ZERO,
            rot: quarter,
            half: Vec2::splat(0.5),
        };
        let plan = plan_drag(
            HandleKind::EdgeX(1),
            &rotated,
            &metrics,
            Vec2::new(2.0, 0.0),
            &ArrayConfig::default(),
        )
        .expect("a plan");
        let first = plan.placements()[0].map_point(Vec2::ZERO);
        assert!(
            first.distance(Vec2::new(0.0, 1.0)) < 1e-4,
            "the frame's +X is world +Y here, got {first:?}"
        );
    }

    #[test]
    fn selection_pieces_skips_infinite_ground_planes() {
        let pieces = selection_pieces(&[
            (
                ShapeDef::Box {
                    width: 1.0,
                    height: 1.0,
                },
                PosRot {
                    pos: Vec2::ZERO,
                    rot: 0.0,
                },
            ),
            (
                ShapeDef::HalfPlane,
                PosRot {
                    pos: Vec2::ZERO,
                    rot: 0.0,
                },
            ),
        ]);
        assert_eq!(pieces.len(), 1, "a floor has no pitch worth measuring");
    }

    #[test]
    fn extent_and_contact_agree_for_a_lone_convex_body() {
        let pieces = vec![unit_square(Vec2::ZERO, 0.75)];
        let contact = gradiance_geometry::array::contact_pitch_or_extent(&pieces, Vec2::X);
        let extent = extent_along(&pieces, Vec2::X);
        assert!((contact - extent).abs() < 1e-4);
    }
}

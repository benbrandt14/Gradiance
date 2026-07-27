//! The packing problem: what is being arranged, and under which rules.
//!
//! A [`PackProblem`] is a pure value — a list of convex hulls with start
//! poses plus a [`PackConfig`]. It knows nothing about the ECS, avian, or
//! the editor; the interaction layer builds one from a selection, hands it
//! to a solver, and translates the resulting [`Layout`] back into pose
//! changes. That separation is what makes the whole family unit-testable
//! without a `World`.
//!
//! # Identity
//!
//! Items are identified by **index**. The crate deliberately does not know
//! about `StableId`: one packing item can stand for a whole group of bodies
//! moved rigidly together, so there is no one-to-one mapping to hand it.
//! The caller keeps the parallel table.

use bevy::ecs::reflect::ReflectResource;
use bevy::math::Vec2;
use bevy::prelude::{Reflect, Resource};
use gradiance_core::units::PosRot;

use crate::hull::{circumradius, convex_hull, polygon_area, polygon_centroid};

/// Which solver runs the arrangement.
///
/// The three are genuinely different search strategies rather than tuning
/// presets, which is the point: a packing instance that one handles badly is
/// usually easy for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum SolverKind {
    /// One-shot constructive placement into rows ("shelves"), largest first.
    ///
    /// Instant and fully deterministic. Ignores the start poses entirely —
    /// it *builds* an arrangement rather than improving one — so it is the
    /// right first press on a scattered pile, and the usual seed for the
    /// iterative solvers.
    Shelf,
    /// Penalty relaxation: separate every violating pair along its minimum
    /// translation vector while pulling everything toward the arrangement
    /// centre.
    ///
    /// Preserves the rough arrangement the user already has (things settle
    /// where they were, only tighter), converges smoothly, and animates
    /// legibly. Reach for it when *where things are* carries meaning; it is
    /// measurably less dense than shelf or descent on a scattered pile.
    Relax,
    /// Metropolis simulated annealing over translations, rotations, and
    /// pairwise swaps.
    ///
    /// The only one that can escape a bad local minimum (it accepts uphill
    /// moves early), at the cost of many more iterations and a result that
    /// depends on the seed. Use it when relaxation jams.
    Anneal,
    /// Quasi-Newton descent on the objective's analytic gradient, driven by
    /// the `argmin` optimization crate (L-BFGS with a Hager–Zhang line
    /// search).
    ///
    /// The only solver here that uses real curvature information, so it
    /// converges in far fewer iterations than relaxation once it is near a
    /// solution — but it follows the gradient into the nearest minimum and
    /// will not climb out, so it is warm started by default.
    ///
    /// The default, on the benchmark's evidence: it matches or beats every
    /// other strategy on fill ratio across all three reference scenes.
    #[default]
    Descent,
    /// The baseline: attract everything to the centroid and push overlapping
    /// pairs apart, both on every iteration.
    ///
    /// This is deliberately the *naive* method — it is what turning up
    /// gravity in the physics engine amounts to — and it exists to be
    /// measured against. It has no objective function, so it stops at a
    /// force balance rather than at an arrangement, which is exactly the
    /// failure the rest of this crate is built to avoid. Keep it: a solver
    /// family with no yardstick has no way to know whether it is any good.
    Naive,
}

impl SolverKind {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 5] = [
        Self::Shelf,
        Self::Relax,
        Self::Descent,
        Self::Anneal,
        Self::Naive,
    ];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Shelf => "Shelf (constructive)",
            Self::Relax => "Relaxation",
            Self::Anneal => "Annealing",
            Self::Descent => "Gradient descent (L-BFGS)",
            Self::Naive => "Naive attraction (baseline)",
        }
    }

    /// Whether this strategy benefits from a constructive warm start.
    ///
    /// True for both search strategies that begin from wherever they are put:
    /// descent because it strictly descends, annealing because its budget is
    /// far better spent refining a decent arrangement than discovering one.
    pub fn wants_warm_start(self) -> bool {
        matches!(self, Self::Descent | Self::Anneal)
    }

    /// One-line description for a tooltip.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Shelf => {
                "Instant, deterministic row packing. Discards the current \
                 arrangement and rebuilds it largest-first."
            }
            Self::Relax => {
                "Pushes overlapping bodies apart along the shortest exit while \
                 pulling the set inward. Keeps the arrangement you already have."
            }
            Self::Anneal => {
                "Randomized search that accepts uphill moves early to escape \
                 local minima. Slower; result depends on the seed."
            }
            Self::Descent => {
                "Quasi-Newton descent on the analytic gradient (argmin's \
                 L-BFGS). Fastest to polish a nearly-good layout; will not \
                 escape a bad one, so pair it with a warm start."
            }
            Self::Naive => {
                "The yardstick: pull everything together and push overlaps \
                 apart, with no objective. What a physics settle does. Here \
                 to be beaten, not used."
            }
        }
    }
}

/// What "smallest" means — the scalar the solvers drive down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum Objective {
    /// Area of the axis-aligned bounding box. The plain reading of "fit into
    /// the smallest area", and the one that matches a rectangular container.
    #[default]
    BoundingArea,
    /// Area of the convex hull of the whole arrangement. Rewards genuinely
    /// interlocked packings that a bounding box cannot see, and does not
    /// care how the result is oriented.
    HullArea,
    /// Area of the smallest enclosing circle (approximated by the maximum
    /// radius from the centroid). Use when the result has to fit a round
    /// container or spin about its centre.
    EnclosingCircle,
    /// Perimeter of the arrangement's convex hull. Favours compact blobs
    /// over long thin strips more aggressively than area does.
    HullPerimeter,
}

impl Objective {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 4] = [
        Self::BoundingArea,
        Self::HullArea,
        Self::EnclosingCircle,
        Self::HullPerimeter,
    ];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::BoundingArea => "Bounding-box area",
            Self::HullArea => "Convex-hull area",
            Self::EnclosingCircle => "Enclosing circle",
            Self::HullPerimeter => "Hull perimeter",
        }
    }
}

/// An optional container the arrangement must respect.
#[derive(Debug, Clone, Copy, PartialEq, Default, Reflect)]
pub enum Boundary {
    /// No container — minimize the objective in open space.
    #[default]
    Free,
    /// No hard container, but penalize departures from a target
    /// width∶height ratio, so the result comes out roughly this shape.
    Aspect {
        /// Target width divided by height (> 0).
        ratio: f32,
    },
    /// A hard axis-aligned rectangle centred on the arrangement's start
    /// centroid. Items are pushed back inside every iteration.
    Rect {
        /// Full width in metres.
        width: f32,
        /// Full height in metres.
        height: f32,
    },
    /// A hard circle centred on the arrangement's start centroid.
    Circle {
        /// Radius in metres.
        radius: f32,
    },
}

impl Boundary {
    /// Short human label (the discriminant only — the numbers are edited
    /// separately).
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Aspect { .. } => "Target aspect",
            Self::Rect { .. } => "Rectangle",
            Self::Circle { .. } => "Circle",
        }
    }

    /// Whether this boundary is a hard wall (as opposed to a soft bias).
    pub fn is_hard(self) -> bool {
        matches!(self, Self::Rect { .. } | Self::Circle { .. })
    }
}

/// How much freedom items have to turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum RotationMode {
    /// Items keep their authored angle. Necessary when orientation carries
    /// meaning (labelled parts, gravity-sensitive assemblies).
    Fixed,
    /// Quarter turns only — the usual choice for boxy parts, and much easier
    /// to search than free rotation.
    #[default]
    Quarter,
    /// `steps` equal divisions of a full turn.
    Steps {
        /// Number of allowed orientations around the circle (≥ 1).
        steps: u32,
    },
    /// Any angle.
    Free,
}

impl RotationMode {
    /// Snaps `angle` to the nearest orientation this mode allows, relative to
    /// the item's `start` angle (so `Fixed` genuinely means "unchanged" even
    /// for a body authored at 37°).
    pub fn snap(self, angle: f32, start: f32) -> f32 {
        match self {
            Self::Fixed => start,
            Self::Free => angle,
            Self::Quarter => Self::quantize(angle, start, 4),
            Self::Steps { steps } => Self::quantize(angle, start, steps.max(1)),
        }
    }

    fn quantize(angle: f32, start: f32, steps: u32) -> f32 {
        let step = std::f32::consts::TAU / steps as f32;
        start + ((angle - start) / step).round() * step
    }

    /// Whether the solver may change orientation at all.
    pub fn allows_rotation(self) -> bool {
        !matches!(self, Self::Fixed)
    }
}

/// How the 2.5D depth bands enter the no-overlap constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum LayerRule {
    /// Two items may share the same XY footprint when their collision-layer
    /// bits are **disjoint** — they are at different depths and would never
    /// touch in the running scene.
    ///
    /// This is what makes the solver a 2.5D packer rather than a flat one,
    /// and it is the default because it matches what the scene actually
    /// simulates: collision layer ≡ visual depth.
    #[default]
    Respect,
    /// Every item is solid against every other regardless of depth. Produces
    /// a flat, single-plane arrangement — what you want when the packing is
    /// for a *layout* (a parts sheet, a diagram) rather than for physics.
    Solid,
}

impl LayerRule {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 2] = [Self::Respect, Self::Solid];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Respect => "Depth-aware (overlap allowed off-layer)",
            Self::Solid => "Flat (everything solid)",
        }
    }

    /// Whether a pair with these layer masks must be kept apart.
    pub fn pair_collides(self, a: u32, b: u32) -> bool {
        match self {
            Self::Respect => a & b != 0,
            Self::Solid => true,
        }
    }
}

/// How edge directions are folded before their alignment is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum EdgeAlignment {
    /// Literally parallel: two edges agree when they point the same way or
    /// exactly opposite. A quarter-turned part is *not* aligned.
    Parallel,
    /// Axis-aligned: perpendicular edges count as agreeing too. This is the
    /// one that makes rectangular parts settle into a grid, because a box
    /// contributes both of its edge directions no matter how it is turned.
    #[default]
    Orthogonal,
}

impl EdgeAlignment {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 2] = [Self::Parallel, Self::Orthogonal];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Parallel => "Parallel edges",
            Self::Orthogonal => "Axis-aligned (grid)",
        }
    }

    /// The angular fold factor: directions are compared modulo `2π / fold`.
    pub fn fold(self) -> u32 {
        match self {
            Self::Parallel => 2,
            Self::Orthogonal => 4,
        }
    }
}

/// How much each objective term counts.
///
/// The score is a weighted sum of dimensionless terms (see
/// [`crate::objective`]), so these are the dial that decides *what kind of
/// arrangement* is being asked for — not just how hard to look for it.
/// Setting a weight to zero removes its term entirely, which is also how you
/// buy back the CPU it costs.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct ObjectiveWeights {
    /// Shrink the overall [`Objective`] measure. The global "make it small"
    /// term.
    pub extent: f32,
    /// Drive the fill ratio (item area ÷ extent area) toward 1. Says the
    /// same thing as `extent` for a fixed set of bodies, but bounded to
    /// `0..1`, so it keeps pushing when the extent term has flattened out.
    pub fill: f32,
    /// Close the leftover space to each body's `gap_neighbors` nearest
    /// neighbours.
    ///
    /// A *local* measure, where extent is global: a body in the middle of a
    /// cluster does not move the bounding box at all, so it receives no
    /// signal from the extent term. Use this to ask for a specific neighbour
    /// spacing. Benchmarking found it neutral for raw density, so it is off
    /// by default — it is a goal, not a free win.
    pub gap: f32,
    /// Line edges up with each other (see [`EdgeAlignment`]). Turns a pile
    /// into a tidy block rather than merely a dense one.
    pub parallel: f32,
    /// Signed: **positive** minimizes how much boundary bodies share
    /// (spreads them apart within whatever container they are in),
    /// **negative** maximizes it (pulls them into flush, interlocked
    /// contact). Zero ignores contact.
    pub contact: f32,
}

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            extent: 1.0,
            fill: 1.0,
            // Zero by default: benchmarking across the three reference scenes
            // showed the gap term neutral at best for density, and slightly
            // negative when it steered the relaxation pulse. It stays as a
            // goal you can ask for — "hold a specific spacing between
            // neighbours" is a real request — but it does not earn a place in
            // the default packing.
            gap: 0.0,
            parallel: 0.0,
            contact: 0.0,
        }
    }
}

impl ObjectiveWeights {
    /// Tightest possible packing: density above all.
    pub const TIGHT: Self = Self {
        extent: 1.0,
        fill: 1.5,
        gap: 0.5,
        parallel: 0.0,
        contact: -0.5,
    };

    /// A tidy block — dense, but with everything squared up.
    pub const TIDY: Self = Self {
        extent: 1.0,
        fill: 1.0,
        gap: 0.0,
        parallel: 3.0,
        contact: 0.0,
    };

    /// Spread out inside the container, touching as little as possible.
    /// Only meaningful with a hard boundary, which is what stops the
    /// arrangement simply flying apart.
    pub const SPACED: Self = Self {
        extent: 0.0,
        fill: 0.0,
        gap: -1.0,
        parallel: 0.0,
        contact: 1.5,
    };

    /// The named presets, for UI enumeration.
    pub const PRESETS: [(&'static str, Self); 3] = [
        ("Tight", Self::TIGHT),
        ("Tidy", Self::TIDY),
        ("Spaced", Self::SPACED),
    ];
}

/// Tuning for [`SolverKind::Relax`].
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct RelaxParams {
    /// Fraction of each pair's minimum translation applied per iteration.
    /// 1.0 resolves a pair in one step but oscillates in dense piles; the
    /// default under-relaxes for stability.
    pub separation_gain: f32,
    /// Strength of one compaction pulse: the fraction of its distance to the
    /// arrangement centre an item is pulled inward. This is the force that
    /// actually *closes* the packing.
    pub attraction: f32,
    /// Residual overlap (metres) below which the arrangement counts as
    /// settled and the next compaction pulse may fire. Defaults to
    /// [`Metrics::PENETRATION_TOLERANCE`](crate::Metrics::PENETRATION_TOLERANCE),
    /// so a pulse fires exactly when the scorer would call the layout legal.
    ///
    /// Squeezing and separating on the *same* iteration is what makes a naive
    /// penalty relaxation stall: the two forces reach a balance and leave a
    /// permanent residual overlap, so the run "converges" to an illegal
    /// layout. Pulsing instead — squeeze, then separate until clear — drives
    /// the overlap back to zero between squeezes, so every pulse starts from
    /// a legal arrangement and the best-so-far is always taken from one.
    ///
    /// The gate is on *overlap*, not on an iteration count, because how long
    /// settling takes depends entirely on how tangled the selection was: a
    /// fixed period lets a deeply interpenetrating pile get re-squeezed
    /// before it has come apart, and it never converges. It is also what
    /// makes the preview legible — you watch it clench and relax.
    pub settle_epsilon: f32,
    /// Random displacement added on each compaction pulse (metres), which
    /// breaks the symmetric deadlocks that stall a pure gradient scheme.
    /// Settling iterations stay deterministic so they can actually converge.
    pub jitter: f32,
    /// Per-iteration blend toward the orientation that minimizes an item's
    /// own bounding box against the pull direction. Ignored when the
    /// rotation mode is `Fixed`.
    pub rotation_gain: f32,
    /// Fraction of the previous iteration's displacement carried into the
    /// next one. Momentum lets a pile keep flowing through a tight spot
    /// instead of stalling at the first contact.
    pub inertia: f32,
}

impl Default for RelaxParams {
    fn default() -> Self {
        Self {
            separation_gain: 0.6,
            attraction: 0.06,
            settle_epsilon: crate::objective::Metrics::PENETRATION_TOLERANCE,
            jitter: 0.002,
            rotation_gain: 0.15,
            inertia: 0.25,
        }
    }
}

/// Tuning for [`SolverKind::Anneal`].
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct AnnealParams {
    /// Starting temperature, in objective units. Higher explores more.
    pub start_temperature: f32,
    /// Geometric cooling factor applied per iteration (0 < c < 1).
    pub cooling: f32,
    /// Standard deviation of a translation proposal, in metres.
    pub move_scale: f32,
    /// Standard deviation of a rotation proposal, in radians.
    pub rotation_scale: f32,
    /// Probability that a proposal swaps two items' positions instead of
    /// nudging one. Swaps are what let a badly ordered pile re-sort itself.
    pub swap_probability: f32,
    /// Inward drift added to every nudge, as a fraction of the item's
    /// distance to the arrangement centre.
    ///
    /// Without it, annealing on a spread-out selection barely works: a
    /// symmetric random nudge almost always *grows* the bounding box (any
    /// vertical component widens a flat row), so the Metropolis test spends
    /// its whole budget rejecting. Leaning the proposal distribution inward
    /// makes shrinking moves the common case and lets the acceptance test do
    /// the job it is good at — deciding which of them survive the overlaps.
    pub compaction: f32,
    /// Weight of residual overlap in the energy. Large enough that a
    /// violating layout never beats a legal one at low temperature.
    pub overlap_penalty: f32,
}

impl Default for AnnealParams {
    fn default() -> Self {
        Self {
            start_temperature: 0.5,
            cooling: 0.997,
            move_scale: 0.15,
            rotation_scale: 0.4,
            swap_probability: 0.15,
            compaction: 0.05,
            overlap_penalty: 40.0,
        }
    }
}

/// The order [`SolverKind::Shelf`] feeds items to the packer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum ShelfOrder {
    /// Largest area first — the standard heuristic, best average density.
    #[default]
    Area,
    /// Tallest first; produces the tidiest rows for mixed heights.
    Height,
    /// Widest first.
    Width,
    /// Longest diagonal first.
    Diagonal,
    /// Keep the selection order — lets the user dictate the layout by
    /// clicking bodies in the order they want them placed.
    Selection,
}

impl ShelfOrder {
    /// Every variant, for UI enumeration.
    pub const ALL: [Self; 5] = [
        Self::Area,
        Self::Height,
        Self::Width,
        Self::Diagonal,
        Self::Selection,
    ];

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Area => "Area",
            Self::Height => "Height",
            Self::Width => "Width",
            Self::Diagonal => "Diagonal",
            Self::Selection => "Selection order",
        }
    }
}

/// Tuning for [`SolverKind::Shelf`].
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct ShelfParams {
    /// Which dimension decides placement order.
    pub order: ShelfOrder,
    /// Largest-first (the usual) versus smallest-first.
    pub descending: bool,
    /// Try the item's other allowed orientations and keep the one that
    /// wastes least row height. Costs a few extra fits per item.
    pub try_rotations: bool,
}

impl Default for ShelfParams {
    fn default() -> Self {
        Self {
            order: ShelfOrder::Area,
            descending: true,
            try_rotations: true,
        }
    }
}

/// Everything the user can dial about a packing run.
///
/// Lives as a Bevy resource so the UI can edit it in place (it is editor
/// **configuration**, not authored scene state — see the settings-resource
/// carve-out in `CLAUDE.md`) and so the scripting registry can address it by
/// reflected type name.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource)]
pub struct PackConfig {
    /// Which search strategy runs.
    pub solver: SolverKind,
    /// The extent measure the `extent`/`fill` weights act on.
    pub objective: Objective,
    /// How much each objective term counts — what kind of arrangement is
    /// being asked for.
    pub weights: ObjectiveWeights,
    /// How edge directions are folded for the `parallel` term.
    pub alignment: EdgeAlignment,
    /// Neighbourhood radius for the contact term, as a multiple of the mean
    /// item radius.
    ///
    /// Small values keep the term strictly local (fast, and it only sees
    /// true neighbours); large values make everything see everything, which
    /// degenerates into the naive baseline.
    pub neighborhood: f32,
    /// How many nearest neighbours each item's gap term considers.
    ///
    /// A **count**, not a radius, and that is the whole point. Scoring the
    /// gap over "every pair within R" has a trivial exploit: move everything
    /// far apart and no pair is within R, so the term reads zero and the
    /// solver has discovered that the tightest packing is an explosion.
    /// A fixed number of nearest neighbours can never empty out, so spreading
    /// always costs.
    pub gap_neighbors: u32,
    /// Optional container constraint.
    pub boundary: Boundary,
    /// How much items may turn.
    pub rotation: RotationMode,
    /// Whether disjoint depth bands may share a footprint.
    pub layers: LayerRule,

    /// Required gap between items, in metres. Zero packs flush.
    pub clearance: f32,
    /// Relative objective improvement below which the run is called
    /// converged (compared over the `patience` window).
    pub tolerance: f32,
    /// Hard iteration ceiling — the run stops here even if still improving.
    pub max_iterations: u32,
    /// Iterations without a `tolerance`-sized improvement before the run is
    /// declared converged.
    pub patience: u32,
    /// Iterations executed per rendered frame. Higher finishes sooner;
    /// lower makes the ghost readable.
    pub iterations_per_frame: u32,
    /// Ceiling on how far one item may move in a single iteration, in
    /// metres. Keeps the preview from teleporting and the solver from
    /// exploding on a deeply interpenetrating start.
    pub max_step: f32,
    /// Seed for the stochastic solvers. Same seed ⇒ same arrangement.
    pub seed: u64,
    /// Independent runs from different seeds; the best result wins. Only
    /// meaningful for the stochastic solvers.
    pub restarts: u32,

    /// Treat items the user pinned as immovable obstacles to pack around.
    pub honor_pinned: bool,
    /// Move each selection group as one rigid unit rather than as members.
    pub keep_groups: bool,
    /// A direction the arrangement settles toward (e.g. down, to pile
    /// against a floor) on top of the centre attraction. Zero for none.
    pub gravity_bias: Vec2,
    /// Apply the result automatically on convergence. When off, the run
    /// holds its ghost until the user confirms.
    pub auto_apply: bool,
    /// Begin the iterative solvers from a constructive shelf packing instead
    /// of the current arrangement.
    ///
    /// Trades one property for another: a warm start reaches a far denser
    /// result (gradient descent in particular is close to useless without
    /// one, since it walks downhill from wherever it is told to start), but
    /// it discards the arrangement the user had.
    ///
    /// On by default, because the default solver needs it and density is
    /// what "pack this" asks for. Turn it off — or pick
    /// [`SolverKind::Relax`] — when the existing arrangement is meaningful.
    pub warm_start: bool,

    /// [`SolverKind::Relax`] tuning.
    pub relax: RelaxParams,
    /// [`SolverKind::Anneal`] tuning.
    pub anneal: AnnealParams,
    /// [`SolverKind::Shelf`] tuning.
    pub shelf: ShelfParams,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            solver: SolverKind::default(),
            objective: Objective::default(),
            weights: ObjectiveWeights::default(),
            alignment: EdgeAlignment::default(),
            neighborhood: 1.5,
            gap_neighbors: 4,
            boundary: Boundary::default(),
            rotation: RotationMode::default(),
            layers: LayerRule::default(),
            clearance: 0.02,
            tolerance: 1e-4,
            max_iterations: 1500,
            patience: 120,
            iterations_per_frame: 8,
            max_step: 0.25,
            seed: 1,
            restarts: 1,
            honor_pinned: true,
            keep_groups: true,
            gravity_bias: Vec2::ZERO,
            auto_apply: true,
            warm_start: true,
            relax: RelaxParams::default(),
            anneal: AnnealParams::default(),
            shelf: ShelfParams::default(),
        }
    }
}

impl PackConfig {
    /// A copy with every numeric field forced into a sane range, so a
    /// hand-edited or scripted config cannot hang or NaN a run.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let mut out = self.clone();
        out.clearance = finite(out.clearance, 0.0).clamp(0.0, 100.0);
        out.tolerance = finite(out.tolerance, 1e-4).clamp(1e-9, 1.0);
        out.max_iterations = out.max_iterations.clamp(1, 1_000_000);
        out.patience = out.patience.clamp(1, out.max_iterations);
        out.iterations_per_frame = out.iterations_per_frame.clamp(1, 10_000);
        out.max_step = finite(out.max_step, 0.25).clamp(1e-4, 1000.0);
        out.neighborhood = finite(out.neighborhood, 1.5).clamp(0.0, 100.0);
        out.gap_neighbors = out.gap_neighbors.clamp(1, 64);
        out.weights.extent = finite(out.weights.extent, 1.0).clamp(0.0, 100.0);
        out.weights.fill = finite(out.weights.fill, 1.0).clamp(0.0, 100.0);
        out.weights.gap = finite(out.weights.gap, 1.5).clamp(-100.0, 100.0);
        out.weights.parallel = finite(out.weights.parallel, 0.0).clamp(0.0, 100.0);
        out.weights.contact = finite(out.weights.contact, 0.0).clamp(-100.0, 100.0);
        out.restarts = out.restarts.clamp(1, 64);
        if !out.gravity_bias.is_finite() {
            out.gravity_bias = Vec2::ZERO;
        }
        out.boundary = match out.boundary {
            Boundary::Aspect { ratio } => Boundary::Aspect {
                ratio: finite(ratio, 1.0).clamp(0.01, 100.0),
            },
            Boundary::Rect { width, height } => Boundary::Rect {
                width: finite(width, 1.0).max(1e-3),
                height: finite(height, 1.0).max(1e-3),
            },
            Boundary::Circle { radius } => Boundary::Circle {
                radius: finite(radius, 1.0).max(1e-3),
            },
            Boundary::Free => Boundary::Free,
        };
        out.relax.separation_gain = finite(out.relax.separation_gain, 0.6).clamp(0.01, 1.0);
        out.relax.attraction = finite(out.relax.attraction, 0.06).clamp(0.0, 0.5);
        out.relax.settle_epsilon = finite(
            out.relax.settle_epsilon,
            crate::objective::Metrics::PENETRATION_TOLERANCE,
        )
        .clamp(0.0, 10.0);
        out.relax.jitter = finite(out.relax.jitter, 0.0).clamp(0.0, 1.0);
        out.relax.rotation_gain = finite(out.relax.rotation_gain, 0.15).clamp(0.0, 1.0);
        out.relax.inertia = finite(out.relax.inertia, 0.25).clamp(0.0, 0.95);
        out.anneal.start_temperature = finite(out.anneal.start_temperature, 0.5).clamp(1e-6, 1e6);
        out.anneal.cooling = finite(out.anneal.cooling, 0.997).clamp(0.5, 0.999_99);
        out.anneal.move_scale = finite(out.anneal.move_scale, 0.15).clamp(1e-4, 100.0);
        out.anneal.rotation_scale = finite(out.anneal.rotation_scale, 0.4).clamp(0.0, 10.0);
        out.anneal.swap_probability = finite(out.anneal.swap_probability, 0.15).clamp(0.0, 1.0);
        out.anneal.compaction = finite(out.anneal.compaction, 0.05).clamp(0.0, 0.5);
        out.anneal.overlap_penalty = finite(out.anneal.overlap_penalty, 40.0).clamp(0.0, 1e6);
        if let RotationMode::Steps { steps } = out.rotation {
            out.rotation = RotationMode::Steps {
                steps: steps.clamp(1, 360),
            };
        }
        out
    }
}

/// One thing being arranged: a convex hull with a starting pose.
#[derive(Debug, Clone, PartialEq)]
pub struct PackItem {
    /// Convex hull in the item's own frame, **centroid-relative** — so a
    /// rotation is about the item's own centre of area, which is what makes
    /// rotation proposals well-behaved.
    pub hull: Vec<Vec2>,
    /// Hull area, in m². Drives the shelf ordering and the fill ratio.
    pub area: f32,
    /// Largest vertex radius — the broad-phase reject distance.
    pub radius: f32,
    /// Where the item starts (`pos` is the hull centroid in world space).
    pub start: PosRot,
    /// Derived collision-layer bits from the body's depth band.
    pub layers: u32,
    /// Immovable: the solver packs around it but never moves it.
    pub pinned: bool,
}

impl PackItem {
    /// Builds an item from a **world-space** outline.
    ///
    /// The outline is hulled, its centroid becomes the item's start position,
    /// and the hull is stored relative to that centroid. `rot` is the body's
    /// current world angle, so a solved `rot` can be differenced back into a
    /// rotation delta for the caller's transform.
    pub fn from_world_outline(outline: &[Vec2], rot: f32, layers: u32, pinned: bool) -> Self {
        let world_hull = convex_hull(outline);
        let centroid = polygon_centroid(&world_hull);
        // Un-rotate into the body frame so a stored `rot` of `start.rot`
        // reproduces the original world hull exactly.
        let (sin, cos) = (-rot).sin_cos();
        let local: Vec<Vec2> = world_hull
            .iter()
            .map(|v| {
                let d = *v - centroid;
                Vec2::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
            })
            .collect();
        Self {
            area: polygon_area(&local),
            radius: circumradius(&local),
            hull: local,
            start: PosRot { pos: centroid, rot },
            layers,
            pinned,
        }
    }

    /// The item's hull placed at `pose`, in world space.
    ///
    /// `hull` is stored in the body's own (un-rotated) frame, so placement
    /// applies the pose's **absolute** angle — placing at
    /// [`start`](Self::start) reproduces the outline the item was built from.
    pub fn placed(&self, pose: PosRot) -> Vec<Vec2> {
        crate::hull::place(&self.hull, pose.pos, pose.rot)
    }

    /// Writes the item's hull placed at `pose` into a reused buffer.
    pub fn place_into(&self, pose: PosRot, out: &mut Vec<Vec2>) {
        crate::hull::place_into(&self.hull, pose.pos, pose.rot, out);
    }

    /// The item's axis-aligned footprint when turned to `rot`, as a size.
    pub fn footprint(&self, rot: f32, buf: &mut Vec<Vec2>) -> Vec2 {
        crate::hull::place_into(&self.hull, Vec2::ZERO, rot, buf);
        crate::hull::bounds(buf).map_or(Vec2::ZERO, |(min, max)| max - min)
    }
}

/// A candidate arrangement: one pose per [`PackItem`], by index.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// Poses parallel to [`PackProblem::items`].
    pub poses: Vec<PosRot>,
}

impl Layout {
    /// The layout every item starts in.
    pub fn from_starts(items: &[PackItem]) -> Self {
        Self {
            poses: items.iter().map(|i| i.start).collect(),
        }
    }
}

/// A complete packing instance.
#[derive(Debug, Clone)]
pub struct PackProblem {
    /// The items to arrange.
    pub items: Vec<PackItem>,
    /// The rules (already sanitized by [`PackProblem::new`]).
    pub config: PackConfig,
}

impl PackProblem {
    /// Builds a problem, sanitizing the config.
    pub fn new(items: Vec<PackItem>, config: PackConfig) -> Self {
        Self {
            items,
            config: config.sanitized(),
        }
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there is nothing to pack.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether the solver may move item `i` (respects the pinned flag only
    /// when the config says to honor it).
    pub fn movable(&self, i: usize) -> bool {
        !(self.config.honor_pinned && self.items.get(i).is_some_and(|it| it.pinned))
    }

    /// Whether items `i` and `j` must be kept apart under the layer rule.
    pub fn pair_collides(&self, i: usize, j: usize) -> bool {
        let (Some(a), Some(b)) = (self.items.get(i), self.items.get(j)) else {
            return false;
        };
        self.config.layers.pair_collides(a.layers, b.layers)
    }

    /// Total hull area of all items — the denominator of the fill ratio.
    pub fn total_area(&self) -> f32 {
        self.items.iter().map(|i| i.area).sum()
    }

    /// Mean item circumradius — the length scale the neighbourhood cutoff
    /// for the gap and contact terms is expressed in, so those terms behave
    /// the same on tiny parts and huge ones.
    pub fn mean_radius(&self) -> f32 {
        if self.items.is_empty() {
            return 0.0;
        }
        self.items.iter().map(|i| i.radius).sum::<f32>() / self.items.len() as f32
    }

    /// Each item's `k` nearest neighbours, as a deduplicated pair list.
    ///
    /// Distances are between item centres — a broad-phase proxy, which is
    /// all the gap term needs to decide *which* pairs to look at (the exact
    /// separation is measured afterwards). Pairs that cannot collide under
    /// the layer rule are skipped, so off-layer bodies are never each other's
    /// neighbours.
    pub fn nearest_pairs(&self, centers: &[Vec2], k: usize) -> Vec<(usize, usize)> {
        let n = self.items.len();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut candidates: Vec<(f32, usize)> = Vec::with_capacity(n);
        for i in 0..n {
            let Some(&ci) = centers.get(i) else { continue };
            candidates.clear();
            for j in 0..n {
                if i == j || !self.pair_collides(i, j) {
                    continue;
                }
                if !self.movable(i) && !self.movable(j) {
                    continue;
                }
                let Some(&cj) = centers.get(j) else { continue };
                candidates.push((ci.distance_squared(cj), j));
            }
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
            for &(_, j) in candidates.iter().take(k) {
                let pair = if i < j { (i, j) } else { (j, i) };
                if !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
        pairs
    }

    /// Indices of the items a solver may move, in order.
    pub fn movable_indices(&self) -> Vec<usize> {
        (0..self.len()).filter(|i| self.movable(*i)).collect()
    }

    /// Centroid of the starting arrangement, area-weighted. Hard boundaries
    /// are centred here, and it is the default attraction target.
    pub fn start_center(&self) -> Vec2 {
        let total = self.total_area();
        if total > 1e-9 {
            self.items
                .iter()
                .map(|i| i.start.pos * i.area)
                .sum::<Vec2>()
                / total
        } else if self.items.is_empty() {
            Vec2::ZERO
        } else {
            self.items.iter().map(|i| i.start.pos).sum::<Vec2>() / self.items.len() as f32
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exact clamp results
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn square_outline(center: Vec2, half: f32) -> Vec<Vec2> {
        vec![
            center + Vec2::new(-half, -half),
            center + Vec2::new(half, -half),
            center + Vec2::new(half, half),
            center + Vec2::new(-half, half),
        ]
    }

    #[test]
    fn an_item_recenters_its_hull_and_keeps_the_world_position() {
        let item =
            PackItem::from_world_outline(&square_outline(Vec2::new(4.0, 1.0), 0.5), 0.0, 1, false);
        assert!(item.start.pos.distance(Vec2::new(4.0, 1.0)) < 1e-5);
        assert!((item.area - 1.0).abs() < 1e-5);
        assert!(
            polygon_centroid(&item.hull).length() < 1e-5,
            "hull is centroid-relative"
        );
    }

    #[test]
    fn placing_an_item_at_its_start_reproduces_the_input_outline() {
        for rot in [0.0, 0.7, -2.0] {
            let outline = square_outline(Vec2::new(-2.0, 3.0), 0.75);
            let item = PackItem::from_world_outline(&outline, rot, 1, false);
            let back = item.placed(item.start);
            let bounds_in = crate::hull::bounds(&outline).expect("non-empty");
            let bounds_out = crate::hull::bounds(&back).expect("non-empty");
            assert!(bounds_in.0.distance(bounds_out.0) < 1e-4, "rot {rot}");
            assert!(bounds_in.1.distance(bounds_out.1) < 1e-4, "rot {rot}");
        }
    }

    #[test]
    fn rotation_modes_quantize_relative_to_the_authored_angle() {
        let start = 0.3;
        assert!((RotationMode::Fixed.snap(2.0, start) - start).abs() < 1e-6);
        assert!((RotationMode::Free.snap(2.0, start) - 2.0).abs() < 1e-6);
        // A quarter-turn mode snaps to start + k·(π/2).
        let snapped = RotationMode::Quarter.snap(start + FRAC_PI_2 + 0.1, start);
        assert!((snapped - (start + FRAC_PI_2)).abs() < 1e-5);
        // Two steps means half turns.
        let halved = RotationMode::Steps { steps: 2 }.snap(start + PI - 0.2, start);
        assert!((halved - (start + PI)).abs() < 1e-5);
    }

    #[test]
    fn the_layer_rule_decides_whether_off_layer_items_conflict() {
        assert!(!LayerRule::Respect.pair_collides(0b0001, 0b0010));
        assert!(LayerRule::Respect.pair_collides(0b0011, 0b0010));
        assert!(LayerRule::Solid.pair_collides(0b0001, 0b0010));
    }

    #[test]
    fn sanitizing_repairs_hostile_numbers() {
        let cfg = PackConfig {
            clearance: f32::NAN,
            tolerance: -5.0,
            max_iterations: 0,
            patience: 9999,
            max_step: 0.0,
            restarts: 500,
            gravity_bias: Vec2::new(f32::INFINITY, 0.0),
            boundary: Boundary::Rect {
                width: -3.0,
                height: f32::NAN,
            },
            ..Default::default()
        }
        .sanitized();
        assert_eq!(cfg.clearance, 0.0);
        assert!(cfg.tolerance > 0.0);
        assert_eq!(cfg.max_iterations, 1);
        assert_eq!(cfg.patience, 1, "patience never exceeds the iteration cap");
        assert!(cfg.max_step > 0.0);
        assert_eq!(cfg.restarts, 64);
        assert_eq!(cfg.gravity_bias, Vec2::ZERO);
        assert!(
            matches!(cfg.boundary, Boundary::Rect { width, height } if width > 0.0 && height > 0.0)
        );
    }

    #[test]
    fn the_start_center_is_area_weighted() {
        let problem = PackProblem::new(
            vec![
                // A big square at the origin and a small one far away: the
                // centre must sit near the big one.
                PackItem::from_world_outline(&square_outline(Vec2::ZERO, 1.0), 0.0, 1, false),
                PackItem::from_world_outline(
                    &square_outline(Vec2::new(10.0, 0.0), 0.1),
                    0.0,
                    1,
                    false,
                ),
            ],
            PackConfig::default(),
        );
        assert!(problem.start_center().x < 0.5);
    }

    #[test]
    fn pinned_items_are_only_immovable_when_the_config_honors_them() {
        let items = vec![PackItem::from_world_outline(
            &square_outline(Vec2::ZERO, 0.5),
            0.0,
            1,
            true,
        )];
        let honoring = PackProblem::new(items.clone(), PackConfig::default());
        assert!(!honoring.movable(0));
        let ignoring = PackProblem::new(
            items,
            PackConfig {
                honor_pinned: false,
                ..Default::default()
            },
        );
        assert!(ignoring.movable(0));
    }
}

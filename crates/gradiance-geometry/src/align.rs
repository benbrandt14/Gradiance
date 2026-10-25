//! Alignment snapping: line a moving selection up with its neighbours.
//!
//! While a selection is dragged, its edges may lightly snap collinear with
//! other bodies' edges, and (optionally) its centre onto their centre
//! lines. "Collinear" is measured along the **active grid's axes**, which
//! need not be orthogonal (isometric). The axes `a0`, `a1` define oblique
//! coordinates: a point is `p = u·a0 + v·a1`. A line parallel to `a0` is a
//! level set of `v`, so "these two edges parallel to `a0` are collinear"
//! is "their `v` coordinates are equal", and lining them up is a shift of
//! `v` alone — a translation along `a1`, which leaves `u` untouched. The
//! two coordinates therefore align independently, and an axis lock that
//! freezes one of them (motion along `a0` only freezes `v`) simply turns
//! that coordinate's alignment off.
//!
//! The coordinates come from the dual basis ([`AlignFrame::coords`]); for
//! an orthonormal basis they are plain projections. Extents are exact for
//! circles (read analytically through [`snap_outline`]) and taken from the
//! polygonized vertices otherwise — [`polygonize`](crate::polygonize) stays
//! the single discretization point.
//!
//! Everything here is pure; the select tool gathers the intervals once at
//! gesture start and calls [`align`] each frame.

use crate::shape::ShapeDef;
use crate::snap_features::{SnapOutline, snap_outline};
use bevy::math::Vec2;

/// A closed coordinate interval `[min, max]` along one oblique axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    /// Lower end.
    pub min: f32,
    /// Upper end.
    pub max: f32,
}

impl Interval {
    /// The interval's midpoint (where a centre line runs).
    pub fn mid(self) -> f32 {
        0.5 * (self.min + self.max)
    }

    /// The interval moved by `d`.
    #[must_use]
    pub fn shifted(self, d: f32) -> Self {
        Self {
            min: self.min + d,
            max: self.max + d,
        }
    }

    /// The smallest interval containing both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn include(&mut self, x: f32) {
        self.min = self.min.min(x);
        self.max = self.max.max(x);
    }
}

/// A body's (or selection's) extent in both oblique coordinates:
/// `[0]` is the `u` interval (along `a0`), `[1]` the `v` interval.
pub type Extents = [Interval; 2];

/// The oblique frame of a two-axis basis: the axes and their duals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignFrame {
    axes: [Vec2; 2],
    duals: [Vec2; 2],
}

impl AlignFrame {
    /// The frame of `basis` (two unit axes), or `None` when they are
    /// (nearly) parallel and no oblique coordinates exist.
    pub fn new(basis: [Vec2; 2]) -> Option<Self> {
        let [a0, a1] = basis;
        let det = a0.perp_dot(a1);
        if !det.is_finite() || det.abs() < 1e-6 {
            return None;
        }
        // d0·a0 = 1, d0·a1 = 0; d1·a0 = 0, d1·a1 = 1.
        let d0 = -a1.perp() / det;
        let d1 = a0.perp() / det;
        Some(Self {
            axes: basis,
            duals: [d0, d1],
        })
    }

    /// The axes this frame was built from.
    pub fn axes(&self) -> [Vec2; 2] {
        self.axes
    }

    /// Oblique coordinates `(u, v)` of a world point.
    pub fn coords(&self, p: Vec2) -> Vec2 {
        Vec2::new(p.dot(self.duals[0]), p.dot(self.duals[1]))
    }

    /// The world point (or translation) with oblique coordinates `c`.
    pub fn point(&self, c: Vec2) -> Vec2 {
        self.axes[0] * c.x + self.axes[1] * c.y
    }

    /// Extents of a point set, or `None` when it is empty or non-finite.
    pub fn points_extents(&self, points: impl IntoIterator<Item = Vec2>) -> Option<Extents> {
        let mut out: Option<Extents> = None;
        for p in points {
            let c = self.coords(p);
            if !c.is_finite() {
                return None;
            }
            match &mut out {
                None => {
                    out = Some([
                        Interval { min: c.x, max: c.x },
                        Interval { min: c.y, max: c.y },
                    ]);
                }
                Some([u, v]) => {
                    u.include(c.x);
                    v.include(c.y);
                }
            }
        }
        out
    }

    /// Extents of `shape` placed at (`pos`, `rot`): exact for a circle,
    /// from the polygonized outline vertices otherwise.
    pub fn shape_extents(&self, shape: &ShapeDef, pos: Vec2, rot: f32) -> Option<Extents> {
        let r = Vec2::from_angle(rot);
        match snap_outline(shape) {
            SnapOutline::Circle { center, radius, .. } => {
                let c = self.coords(pos + r.rotate(center));
                // max over the circle of (c + r·n)·d is c·d + r·|d|.
                let ext = |k: usize, at: f32| {
                    let h = radius * self.duals[k].length();
                    Interval {
                        min: at - h,
                        max: at + h,
                    }
                };
                let out = [ext(0, c.x), ext(1, c.y)];
                out.iter()
                    .all(|i| i.min.is_finite() && i.max.is_finite())
                    .then_some(out)
            }
            SnapOutline::Contours(contours) => self.points_extents(
                contours
                    .rings()
                    .flat_map(|ring| ring.iter().map(move |v| pos + r.rotate(*v))),
            ),
        }
    }
}

/// Which features may align.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlignFeatures {
    /// Edges: either end of the moving interval onto either end of a
    /// reference's (collinear, and flush when stacked).
    pub edges: bool,
    /// Centre lines: the moving midpoint onto a reference's midpoint.
    pub centers: bool,
}

/// What matched on one coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignFeature {
    /// An edge onto an edge.
    Edge,
    /// A centre line onto a centre line.
    Center,
}

/// An alignment holding on one coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisMatch {
    /// The shift (in that coordinate) that makes the features coincide.
    pub correction: f32,
    /// The coordinate of the shared line after the shift.
    pub line: f32,
    /// Index of the matched reference in the slice given to [`align`].
    pub reference: usize,
    /// Which feature matched.
    pub feature: AlignFeature,
}

/// The smallest correction within `tolerance` that lines `moving` up with
/// one of `refs` on a single coordinate, or `None`.
///
/// Ties keep the earliest candidate (reference order, edges before
/// centres), so the result is deterministic.
pub fn align_axis(
    moving: Interval,
    refs: impl IntoIterator<Item = (usize, Interval)>,
    tolerance: f32,
    features: AlignFeatures,
) -> Option<AxisMatch> {
    if tolerance.is_nan() || tolerance <= 0.0 || !(features.edges || features.centers) {
        return None;
    }
    let mut best: Option<AxisMatch> = None;
    let mut offer = |from: f32, to: f32, reference: usize, feature: AlignFeature| {
        let correction = to - from;
        if correction.abs() <= tolerance
            && best.is_none_or(|b| correction.abs() < b.correction.abs())
        {
            best = Some(AxisMatch {
                correction,
                line: to,
                reference,
                feature,
            });
        }
    };
    for (i, r) in refs {
        if features.edges {
            for from in [moving.min, moving.max] {
                for to in [r.min, r.max] {
                    offer(from, to, i, AlignFeature::Edge);
                }
            }
        }
        if features.centers {
            offer(moving.mid(), r.mid(), i, AlignFeature::Center);
        }
    }
    best
}

/// Per-coordinate alignment of a moving selection against references.
///
/// `moving` is the selection's extents at its current (unaligned)
/// position; `free[k]` says whether coordinate `k` may shift (an axis lock
/// freezes one). Returns the match per coordinate; the world correction is
/// [`correction`] of the result.
pub fn align(
    moving: Extents,
    refs: &[Extents],
    tolerance: f32,
    features: AlignFeatures,
    free: [bool; 2],
) -> [Option<AxisMatch>; 2] {
    [0, 1].map(|k| {
        if !free[k] {
            return None;
        }
        align_axis(
            moving[k],
            refs.iter().enumerate().map(|(i, r)| (i, r[k])),
            tolerance,
            features,
        )
    })
}

/// The world translation the matches add to the move.
pub fn correction(frame: &AlignFrame, matches: &[Option<AxisMatch>; 2]) -> Vec2 {
    let c = |k: usize| matches[k].map_or(0.0, |m| m.correction);
    frame.point(Vec2::new(c(0), c(1)))
}

/// The guide segment for a match on coordinate `k`: along the shared line,
/// spanning both the (aligned) moving extents and the matched reference.
pub fn guide_segment(
    frame: &AlignFrame,
    k: usize,
    m: &AxisMatch,
    moving_aligned: &Extents,
    reference: &Extents,
) -> (Vec2, Vec2) {
    let j = 1 - k;
    let span = moving_aligned[j].union(reference[j]);
    let at = |other: f32| {
        let mut c = Vec2::ZERO;
        c[k] = m.line;
        c[j] = other;
        frame.point(c)
    };
    (at(span.min), at(span.max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapping::{CARTESIAN_LINE_DIRECTIONS, ISOMETRIC_LINE_DIRECTIONS};

    const EDGES: AlignFeatures = AlignFeatures {
        edges: true,
        centers: false,
    };
    const CENTERS: AlignFeatures = AlignFeatures {
        edges: false,
        centers: true,
    };
    const BOTH: AlignFeatures = AlignFeatures {
        edges: true,
        centers: true,
    };

    fn iv(min: f32, max: f32) -> Interval {
        Interval { min, max }
    }

    fn cartesian() -> AlignFrame {
        AlignFrame::new(CARTESIAN_LINE_DIRECTIONS).unwrap()
    }

    fn box_extents(frame: &AlignFrame, pos: Vec2, w: f32, h: f32) -> Extents {
        frame
            .shape_extents(
                &ShapeDef::Box {
                    width: w,
                    height: h,
                },
                pos,
                0.0,
            )
            .unwrap()
    }

    #[test]
    fn an_edge_within_tolerance_snaps_collinear() {
        // Moving bottom at 0.03, reference bottom at 0.
        let m = align_axis(iv(0.03, 1.0), [(0, iv(0.0, 2.0))], 0.05, EDGES).unwrap();
        assert!((m.correction + 0.03).abs() < 1e-6);
        assert!(m.line.abs() < 1e-6);
        assert_eq!(m.feature, AlignFeature::Edge);
    }

    #[test]
    fn an_edge_outside_tolerance_does_not_snap() {
        assert!(align_axis(iv(0.1, 1.0), [(0, iv(0.0, 2.0))], 0.05, EDGES).is_none());
    }

    #[test]
    fn stacked_edges_snap_flush() {
        // Moving bottom 0.02 above the reference's top.
        let m = align_axis(iv(2.02, 3.0), [(0, iv(0.0, 2.0))], 0.05, EDGES).unwrap();
        assert!((m.correction + 0.02).abs() < 1e-6);
        assert!((m.line - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_centre_within_tolerance_snaps_onto_the_centre_line() {
        let m = align_axis(iv(0.0, 1.0), [(0, iv(0.46, 0.06 + 0.46))], 0.1, CENTERS);
        // Reference mid 0.49, moving mid 0.5.
        let m = m.unwrap();
        assert!((m.correction + 0.01).abs() < 1e-6, "{m:?}");
        assert_eq!(m.feature, AlignFeature::Center);
        assert!(align_axis(iv(0.0, 1.0), [(0, iv(2.0, 3.0))], 0.1, CENTERS).is_none());
    }

    #[test]
    fn the_smallest_correction_wins_across_features_and_references() {
        let refs = [(0, iv(0.04, 5.0)), (1, iv(-0.02, 5.0)), (2, iv(-3.0, 0.49))];
        let m = align_axis(iv(0.0, 1.0), refs, 0.05, BOTH).unwrap();
        assert_eq!(m.reference, 1);
        assert!((m.correction + 0.02).abs() < 1e-6);
        // Centres only: reference 2's mid (-1.255) is far; nothing else.
        assert!(align_axis(iv(0.0, 1.0), refs, 0.05, CENTERS).is_none());
    }

    #[test]
    fn disabled_features_never_match() {
        let exact = [(0, iv(0.0, 1.0))];
        assert!(align_axis(iv(0.0, 1.0), exact, 0.05, AlignFeatures::default()).is_none());
        // Coincident centres, edges 1.0 apart: only the centre feature
        // can match.
        let centred = [(0, iv(-1.0, 2.0))];
        assert!(align_axis(iv(0.0, 1.0), centred, 0.05, EDGES).is_none());
        assert!(align_axis(iv(0.0, 1.0), centred, 0.05, CENTERS).is_some());
        // Zero / non-finite tolerance matches nothing.
        assert!(align_axis(iv(0.0, 1.0), exact, 0.0, BOTH).is_none());
        assert!(align_axis(iv(0.0, 1.0), exact, f32::NAN, BOTH).is_none());
    }

    #[test]
    fn a_frozen_coordinate_does_not_align() {
        let frame = cartesian();
        let moving = box_extents(&frame, Vec2::new(5.0, 0.03), 1.0, 1.0);
        let reference = box_extents(&frame, Vec2::new(0.0, 0.0), 1.0, 1.0);
        let free = align(moving, &[reference], 0.05, EDGES, [true, true]);
        assert!(free[0].is_none(), "x is far apart");
        assert!(free[1].is_some(), "bottoms within tolerance");
        let locked = align(moving, &[reference], 0.05, EDGES, [true, false]);
        assert!(locked[1].is_none(), "a horizontal lock freezes v");
        let c = correction(&frame, &free);
        assert!((c - Vec2::new(0.0, -0.03)).length() < 1e-6, "{c}");
    }

    #[test]
    fn circle_extents_are_analytic() {
        let frame = cartesian();
        let e = frame
            .shape_extents(&ShapeDef::Circle { radius: 0.5 }, Vec2::new(1.0, 2.0), 0.3)
            .unwrap();
        assert!((e[0].min - 0.5).abs() < 1e-6 && (e[0].max - 1.5).abs() < 1e-6);
        assert!((e[1].min - 1.5).abs() < 1e-6 && (e[1].max - 2.5).abs() < 1e-6);
    }

    #[test]
    fn isometric_coordinates_are_oblique_and_align_independently() {
        let frame = AlignFrame::new(ISOMETRIC_LINE_DIRECTIONS).unwrap();
        let [a0, a1] = ISOMETRIC_LINE_DIRECTIONS;
        // Round trip, and the duals pick out each axis.
        let p = Vec2::new(0.7, -1.3);
        assert!((frame.point(frame.coords(p)) - p).length() < 1e-5);
        assert!((frame.coords(a0) - Vec2::X).length() < 1e-5);
        assert!((frame.coords(a1) - Vec2::Y).length() < 1e-5);

        // Two unit "diamonds" (squares in oblique coords) whose a0-parallel
        // edges are 0.03 apart in v, far apart in u.
        let diamond = |origin: Vec2| {
            frame
                .points_extents([origin, origin + a0, origin + a0 + a1, origin + a1])
                .unwrap()
        };
        let reference = diamond(Vec2::ZERO);
        let moving = diamond(a0 * 4.0 + a1 * 0.03);
        let matches = align(moving, &[reference], 0.05, EDGES, [true, true]);
        assert!(matches[0].is_none());
        let m = matches[1].unwrap();
        assert!((m.correction + 0.03).abs() < 1e-5, "{m:?}");
        // The correction is a move along a1 only: u is untouched, and the
        // aligned edge lies on the reference's a0-parallel edge line.
        let c = correction(&frame, &matches);
        assert!((c - a1 * -0.03).length() < 1e-5, "{c}");
        let moved_corner = a0 * 4.0 + a1 * 0.03 + c;
        assert!(frame.coords(moved_corner).y.abs() < 1e-5);
        assert!((frame.coords(moved_corner).x - 4.0).abs() < 1e-5);

        // The guide runs along a0 through the shared line, spanning both.
        let aligned = [moving[0], moving[1].shifted(m.correction)];
        let (g0, g1) = guide_segment(&frame, 1, &m, &aligned, &reference);
        assert!((g0 - Vec2::ZERO).length() < 1e-4, "{g0}");
        assert!((g1 - a0 * 5.0).length() < 1e-4, "{g1}");
    }

    #[test]
    fn a_degenerate_basis_has_no_frame() {
        assert!(AlignFrame::new([Vec2::X, Vec2::X]).is_none());
        assert!(AlignFrame::new([Vec2::X, Vec2::NAN]).is_none());
    }
}

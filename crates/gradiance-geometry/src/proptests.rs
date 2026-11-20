//! Property tests over the shape pipeline (brief 6-2).
//!
//! `polygonize` is the single discretization point every derived consumer
//! reads, so its contours are checked as properties over generated shapes
//! rather than at a handful of hand-picked ones:
//!
//! - **Well-formed contours** for random leaves and random CSG trees:
//!   every ring closed (at least three distinct, finite vertices), outlines
//!   counter-clockwise, holes clockwise and inside their outline, and no
//!   two edges of any rings of the result touching except at the vertex
//!   two consecutive edges share.
//! - **Area** against the analytic area of the leaves, and the
//!   contour's inside against the field's sign away from the boundary.
//! - **CSG identities:** `A ∪ A = A` and `A ∩ A = A` exactly, `A − A = ∅`
//!   exactly, `A ∩ B ⊆ A`, `|A ∪ B| ≤ |A| + |B|` and inclusion-exclusion.
//! - **Phase N shape math:** sketch strokes, chain splits, width profiles,
//!   the submerged clip and `mirror_shape`.
//!
//! Every tolerance is stated next to the property with its reason.
//!
//! Runs are deterministic: a fixed RNG seed ([`SEED`]; set
//! `PROPTEST_RNG_SEED` to explore others, `PROPTEST_CASES` to search
//! longer) and no failure-persistence file,
//! so a test run never writes into the repository. A minimised failure is
//! committed as a named regression test at the end of this file instead.

use crate::chain::{MAX_CHAIN_LINKS, MIN_CHAIN_LENGTH, chain_points, path_length};
use crate::contours::{Contours, point_in_ring, ring_signed_area};
use crate::polygonize::{field_cell, polygonize, polygonize_components};
use crate::profile::WidthProfile;
use crate::scale::{MirrorAxis, mirror_shape};
use crate::sdf;
use crate::shape::{CsgOp, MAX_CSG_DEPTH, ShapeDef};
use crate::sketch::{
    MAX_SKETCH_OUTLINE, MAX_SKETCH_SEGMENTS, MIN_SKETCH_LENGTH, leaf_count, ring_is_simple,
    sketch_shape,
};
use crate::submerge::{ClipScratch, FluidVolume, SolidOutline, ring_moment};
use bevy::math::Vec2;
use gradiance_core::constants::CIRCLE_SEGMENTS;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use std::f32::consts::{PI, TAU};

/// The RNG seed every property runs from unless `PROPTEST_RNG_SEED` is set.
const SEED: u64 = 0x0006_0002;

/// A run of `cases` cases (`PROPTEST_CASES` overrides it, for a longer
/// local search): fixed seed, nothing written to disk.
fn config(cases: u32) -> ProptestConfig {
    let default = ProptestConfig::default();
    ProptestConfig {
        cases: if std::env::var_os("PROPTEST_CASES").is_some() {
            default.cases
        } else {
            cases
        },
        failure_persistence: None,
        rng_seed: match default.rng_seed {
            RngSeed::Random => RngSeed::Fixed(SEED),
            fixed @ RngSeed::Fixed(_) => fixed,
        },
        ..default
    }
}

// ---------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------

/// A star-shaped simple polygon about the origin (counter-clockwise),
/// optionally with one clockwise regular hole about the origin.
///
/// `n ≥ 5` vertices at angles jittered by at most ±30 % of the even
/// spacing, so no angular gap reaches `π` and the origin is inside; the
/// hole's radius is at most `0.4 · r_min`, below `r_min · cos(gap / 2)`
/// (≥ `0.53 · r_min` at the widest gap), so it lies inside the outline.
fn star_polygon() -> impl Strategy<Value = ShapeDef> {
    (5usize..12)
        .prop_flat_map(|n| {
            (
                proptest::collection::vec((-0.3f32..0.3, 0.3f32..1.5), n),
                proptest::option::of((3usize..8, 0.3f32..1.0, 0.0f32..TAU)),
            )
        })
        .prop_map(|(spokes, hole)| {
            let n = spokes.len() as f32;
            let outline: Vec<Vec2> = spokes
                .iter()
                .enumerate()
                .map(|(i, (jitter, r))| Vec2::from_angle(TAU * (i as f32 + jitter) / n) * *r)
                .collect();
            let r_min = spokes.iter().map(|(_, r)| *r).fold(f32::MAX, f32::min);
            let holes = hole
                .map(|(m, s, phase)| {
                    (0..m)
                        .rev()
                        .map(|k| {
                            Vec2::from_angle(phase + TAU * k as f32 / m as f32) * 0.4 * r_min * s
                        })
                        .collect()
                })
                .into_iter()
                .collect();
            ShapeDef::Polygon { outline, holes }
        })
}

/// One analytic leaf at a size the contour resolves: every feature at
/// least 0.1 m across, under 4 m overall (no half-plane: its 1 km slab is
/// a render stand-in, not a shape to measure).
fn leaf() -> impl Strategy<Value = ShapeDef> {
    prop_oneof![
        (0.1f32..2.5, 0.1f32..2.5).prop_map(|(width, height)| ShapeDef::Box { width, height }),
        (0.1f32..1.5).prop_map(|radius| ShapeDef::Circle { radius }),
        (0.05f32..1.5, 0.05f32..0.8).prop_map(|(half_length, radius)| ShapeDef::Capsule {
            half_length,
            radius
        }),
        star_polygon(),
    ]
}

/// A leaf rigidly placed near the origin.
fn placed_leaf() -> impl Strategy<Value = ShapeDef> {
    (leaf(), -1.2f32..1.2, -1.2f32..1.2, -PI..PI).prop_map(|(shape, x, y, rot)| ShapeDef::Placed {
        pos: Vec2::new(x, y),
        rot,
        shape: Box::new(shape),
    })
}

/// Every CSG operator, a smooth union with a 5-40 cm blend.
fn csg_op() -> impl Strategy<Value = CsgOp> {
    prop_oneof![
        Just(CsgOp::Union),
        Just(CsgOp::Subtract),
        Just(CsgOp::Intersect),
        Just(CsgOp::Xor),
        (0.05f32..0.4).prop_map(|radius| CsgOp::SmoothUnion { radius }),
    ]
}

/// A random CSG tree: placed leaves under up to three levels of
/// operators, sometimes placed again as a whole.
fn tree() -> impl Strategy<Value = ShapeDef> {
    placed_leaf().prop_recursive(3, 8, 2, |inner| {
        prop_oneof![
            4 => (csg_op(), inner.clone(), inner.clone()).prop_map(|(op, lhs, rhs)| {
                ShapeDef::Csg {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            }),
            1 => (inner, -0.5f32..0.5, -0.5f32..0.5, -PI..PI).prop_map(|(shape, x, y, rot)| {
                ShapeDef::Placed {
                    pos: Vec2::new(x, y),
                    rot,
                    shape: Box::new(shape),
                }
            }),
        ]
    })
}

/// A tree that may also hold half-planes (the mirror and validation
/// properties never contour it).
fn tree_with_planes() -> impl Strategy<Value = ShapeDef> {
    let leaf = prop_oneof![
        4 => placed_leaf(),
        1 => Just(ShapeDef::HalfPlane),
        1 => (-1.0f32..1.0, -PI..PI).prop_map(|(y, rot)| ShapeDef::Placed {
            pos: Vec2::new(0.0, y),
            rot,
            shape: Box::new(ShapeDef::HalfPlane),
        }),
    ];
    leaf.prop_recursive(3, 8, 2, |inner| {
        prop_oneof![
            (csg_op(), inner.clone(), inner.clone()).prop_map(|(op, lhs, rhs)| ShapeDef::Csg {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            }),
            (inner, -0.5f32..0.5, -0.5f32..0.5, -PI..PI).prop_map(|(shape, x, y, rot)| {
                ShapeDef::Placed {
                    pos: Vec2::new(x, y),
                    rot,
                    shape: Box::new(shape),
                }
            }),
        ]
    })
}

// ---------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------

/// The analytic area of a leaf.
fn analytic_area(leaf: &ShapeDef) -> f32 {
    match leaf {
        ShapeDef::Box { width, height } => width * height,
        ShapeDef::Circle { radius } => PI * radius * radius,
        ShapeDef::Capsule {
            half_length,
            radius,
        } => 4.0 * half_length * radius + PI * radius * radius,
        ShapeDef::Polygon { outline, holes } => {
            ring_signed_area(outline) + holes.iter().map(|h| ring_signed_area(h)).sum::<f32>()
        }
        ShapeDef::Placed { shape, .. } => analytic_area(shape),
        ShapeDef::HalfPlane | ShapeDef::Csg { .. } => f32::NAN,
    }
}

/// The perimeter of a leaf's exact boundary (the curve, not its polygon).
fn analytic_perimeter(leaf: &ShapeDef) -> f32 {
    let ring_length = |r: &[Vec2]| path_length(r) + r[r.len() - 1].distance(r[0]);
    match leaf {
        ShapeDef::Box { width, height } => 2.0 * (width + height),
        ShapeDef::Circle { radius } => TAU * radius,
        ShapeDef::Capsule {
            half_length,
            radius,
        } => 4.0 * half_length + TAU * radius,
        ShapeDef::Polygon { outline, holes } => {
            ring_length(outline) + holes.iter().map(|h| ring_length(h)).sum::<f32>()
        }
        ShapeDef::Placed { shape, .. } => analytic_perimeter(shape),
        ShapeDef::HalfPlane | ShapeDef::Csg { .. } => f32::NAN,
    }
}

/// Corners of a leaf's exact boundary (where marching squares chamfers).
fn corner_count(leaf: &ShapeDef) -> usize {
    match leaf {
        ShapeDef::Box { .. } => 4,
        ShapeDef::Polygon { outline, holes } => {
            outline.len() + holes.iter().map(Vec::len).sum::<usize>()
        }
        ShapeDef::Placed { shape, .. } => corner_count(shape),
        _ => 0,
    }
}

/// The contour cell `polygonize` samples `shape` at.
fn cell_of(shape: &ShapeDef) -> f32 {
    let (min, max) = sdf::aabb(shape);
    field_cell(max - min, sdf::thinnest_solid(shape))
}

/// Total perimeter of every ring of every component.
fn perimeter(components: &[Contours]) -> f32 {
    components
        .iter()
        .flat_map(Contours::rings)
        .map(|r| path_length(r) + r[r.len() - 1].distance(r[0]))
        .sum()
}

/// Total area of every component.
fn area(components: &[Contours]) -> f32 {
    components.iter().map(Contours::area).sum()
}

/// Twice the signed area of the triangle `a b c`, in `f64` (positive
/// counter-clockwise).
fn orient(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let (ax, ay) = (f64::from(a.x), f64::from(a.y));
    let (bx, by) = (f64::from(b.x), f64::from(b.y));
    let (cx, cy) = (f64::from(c.x), f64::from(c.y));
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

/// Whether `p`, known collinear with `a b`, lies within that segment's box.
fn on_segment(a: Vec2, b: Vec2, p: Vec2) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Whether the closed segments `a b` and `c d` share any point.
fn segments_touch(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let (d1, d2) = (orient(c, d, a), orient(c, d, b));
    let (d3, d4) = (orient(a, b, c), orient(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_segment(c, d, a))
        || (d2 == 0.0 && on_segment(c, d, b))
        || (d3 == 0.0 && on_segment(a, b, c))
        || (d4 == 0.0 && on_segment(a, b, d))
}

/// The first pair of touching edges among `rings`, other than consecutive
/// edges of one ring at their shared vertex.
fn first_touch(rings: &[&[Vec2]]) -> Option<String> {
    let edges: Vec<(usize, usize, Vec2, Vec2)> = rings
        .iter()
        .enumerate()
        .flat_map(|(r, ring)| {
            (0..ring.len()).map(move |i| (r, i, ring[i], ring[(i + 1) % ring.len()]))
        })
        .collect();
    for (k, &(ra, ia, a, b)) in edges.iter().enumerate() {
        for &(rb, ib, c, d) in &edges[k + 1..] {
            if ra == rb {
                let n = rings[ra].len();
                if ib == (ia + 1) % n || ia == (ib + 1) % n {
                    continue;
                }
            }
            if segments_touch(a, b, c, d) {
                return Some(format!(
                    "ring {ra} edge {ia} {a}-{b} touches ring {rb} edge {ib} {c}-{d}"
                ));
            }
        }
    }
    None
}

/// Every check a [`Contours`] set must pass, as one error message.
fn well_formed(components: &[Contours]) -> Result<(), TestCaseError> {
    well_formed_text(components).map_err(TestCaseError::fail)
}

/// [`well_formed`]'s checks, the failure as text.
fn well_formed_text(components: &[Contours]) -> Result<(), String> {
    for (k, c) in components.iter().enumerate() {
        for (r, ring) in c.rings().enumerate() {
            if ring.len() < 3 {
                return Err(format!("component {k} ring {r}: {} vertices", ring.len()));
            }
            if let Some(v) = ring.iter().find(|v| !v.is_finite()) {
                return Err(format!("component {k} ring {r}: vertex {v}"));
            }
            if let Some(i) = (0..ring.len()).find(|&i| ring[i] == ring[(i + 1) % ring.len()]) {
                return Err(format!("component {k} ring {r}: repeated vertex {i}"));
            }
            let signed = ring_signed_area(ring);
            if (r == 0) != (signed > 0.0) {
                return Err(format!(
                    "component {k} ring {r}: signed area {signed} (outline CCW, holes CW)"
                ));
            }
        }
        for (h, hole) in c.holes.iter().enumerate() {
            if let Some(v) = hole.iter().find(|v| !point_in_ring(**v, &c.outline)) {
                return Err(format!("component {k} hole {h}: vertex {v} outside"));
            }
        }
    }
    let rings: Vec<&[Vec2]> = components
        .iter()
        .flat_map(Contours::rings)
        .map(Vec::as_slice)
        .collect();
    first_touch(&rings).map_or(Ok(()), Err)
}

/// Whether `p` is inside the material of a contour set (even-odd over
/// every ring of every component).
fn inside_contours(components: &[Contours], p: Vec2) -> bool {
    components
        .iter()
        .flat_map(Contours::rings)
        .filter(|r| point_in_ring(p, r))
        .count()
        % 2
        == 1
}

/// The `f32` rounding bound of the shoelace area of `rings`: summing `n`
/// cross terms `a × b` loses at most about `n · ε · Σ|a × b|` (the
/// recursive-summation bound, products included), counted per ring.
fn shoelace_rounding(rings: &[Vec<Vec2>]) -> f32 {
    rings
        .iter()
        .map(|r| {
            let terms: f32 = (0..r.len())
                .map(|i| r[i].length() * r[(i + 1) % r.len()].length())
                .sum();
            (r.len() as f32 + 2.0) * f32::EPSILON * terms
        })
        .sum()
}

/// The chord-to-arc area ratio of a regular polygon of `n` vertices
/// inscribed in a circle: `n·sin(2π/n) / 2π`.
fn inscribed_ratio(n: usize) -> f32 {
    let n = n as f32;
    n * (TAU / n).sin() / TAU
}

// ---------------------------------------------------------------------
// 1. polygonize: leaves and trees
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(config(512))]

    /// A bare leaf takes its exact path: one well-formed ring set whose
    /// area is the analytic area.
    ///
    /// Tolerance: boxes and polygons are exact up to `f32` summation
    /// (`1e-5` relative). Circles and capsule caps are inscribed
    /// [`CIRCLE_SEGMENTS`]-gons, short of the disc by
    /// `1 − 48·sin(2π/48)/2π ≈ 0.29 %`, all of it on the curved part;
    /// that bound plus `1e-5` is the tolerance, and the polygon may never
    /// exceed the curve.
    #[test]
    fn a_bare_leaf_polygonizes_exactly(leaf in leaf()) {
        let contours = polygonize(&leaf);
        well_formed(std::slice::from_ref(&contours))?;
        let ideal = analytic_area(&leaf);
        let got = contours.area();
        let curved = match &leaf {
            ShapeDef::Circle { radius } | ShapeDef::Capsule { radius, .. } => PI * radius * radius,
            _ => 0.0,
        };
        let chord_loss = curved * (1.0 - inscribed_ratio(CIRCLE_SEGMENTS));
        prop_assert!(got <= ideal * (1.0 + 1e-5), "{got} above {ideal}");
        prop_assert!(
            got >= ideal - chord_loss - ideal * 1e-5,
            "{got} below {ideal} by more than the chord loss {chord_loss}"
        );
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// A placed leaf goes through the field contour: one component,
    /// well formed, with the leaf's analytic area.
    ///
    /// Tolerance `0.05 · cell · P + (corners + 2) · cell²`, `P` the exact
    /// perimeter. Vertices are bisected onto the zero set, so the only
    /// errors are: `simplify_ring` drops vertices within `0.05 · cell` of
    /// the chord that replaces them, which shifts the boundary by at most
    /// that along its whole length (on a long thin rod that is the largest
    /// term: the straight side's chord ends inside the caps); a curve
    /// keeps only one-cell chords, whose sag `cell² / 8R` over the curve
    /// loses about `π/6 · cell²` around a whole circle (under `2 · cell²`
    /// for any radius the contour resolves); and the grid cannot place a
    /// corner: the crossings either side of it are joined by a chamfer of
    /// at most a cell square.
    #[test]
    fn a_contoured_leaf_keeps_its_analytic_area(placed in placed_leaf()) {
        let components = polygonize_components(&placed);
        prop_assert_eq!(components.len(), 1, "one leaf, one component");
        well_formed(&components)?;
        let cell = cell_of(&placed);
        let ideal = analytic_area(&placed);
        let got = area(&components);
        let tol = 0.05 * cell * analytic_perimeter(&placed)
            + (corner_count(&placed) as f32 + 2.0) * cell * cell;
        prop_assert!(
            (got - ideal).abs() <= tol,
            "area {got} vs analytic {ideal} (tol {tol}, cell {cell})"
        );
        // The contour's own length is close to the true boundary's: never
        // longer than it beyond float error (chords and chamfers are
        // shortcuts), and not shorter than the chamfers and sag allow.
        let length = perimeter(&components);
        let exact = analytic_perimeter(&placed);
        prop_assert!(length <= exact * (1.0 + 1e-3), "perimeter {length} vs {exact}");
        prop_assert!(
            length >= exact * 0.99 - corner_count(&placed) as f32 * cell,
            "perimeter {length} vs {exact}"
        );
    }

    /// A random CSG tree contours to well-formed components that classify
    /// the plane as the field does.
    ///
    /// Tolerance: points within `1.5 · cell` of the boundary (by the
    /// field, which after min/max is a lower bound on the true distance,
    /// so the band is at least that wide) are not judged. Marching squares
    /// can only misplace the boundary within a cell, and a sliver or hole
    /// under `4 · cell²` is dropped on purpose (`contour_components`); a
    /// disc of that area reaches `1.13 · cell` from its rim.
    #[test]
    fn a_contoured_tree_is_well_formed_and_matches_its_field(
        shape in tree(),
        probes in proptest::collection::vec((0.0f32..1.0, 0.0f32..1.0), 200),
    ) {
        let components = polygonize_components(&shape);
        well_formed(&components)?;
        let cell = cell_of(&shape);
        let (min, max) = sdf::aabb(&shape);
        let span = max - min + Vec2::splat(4.0 * cell);
        for (u, v) in probes {
            let p = min - Vec2::splat(2.0 * cell) + span * Vec2::new(u, v);
            let field = sdf::eval(&shape, p);
            if field.abs() <= 1.5 * cell {
                continue;
            }
            prop_assert_eq!(
                inside_contours(&components, p),
                field < 0.0,
                "at {} the field is {} (cell {})",
                p,
                field,
                cell
            );
        }
        // `polygonize` is the largest of these components.
        let largest = polygonize(&shape);
        let best = components.iter().map(Contours::area).fold(0.0, f32::max);
        prop_assert!((largest.area() - best).abs() <= 1e-6 * best.max(1.0));
    }
}

// ---------------------------------------------------------------------
// 2. CSG identities
// ---------------------------------------------------------------------

fn csg(op: CsgOp, lhs: &ShapeDef, rhs: &ShapeDef) -> ShapeDef {
    ShapeDef::Csg {
        op,
        lhs: Box::new(lhs.clone()),
        rhs: Box::new(rhs.clone()),
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// `A ∪ A` and `A ∩ A` are `A` and `A − A` is empty, **exactly**:
    /// `min(a, a) = max(a, a) = a` and `max(a, −a) = |a| ≥ 0`, under the
    /// same bounding box and cell, so the contours are bit-identical (and
    /// the subtraction has no sample below zero).
    #[test]
    fn self_booleans_are_exact(a in tree()) {
        let alone = polygonize_components(&a);
        prop_assert_eq!(&polygonize_components(&csg(CsgOp::Union, &a, &a)), &alone);
        prop_assert_eq!(&polygonize_components(&csg(CsgOp::Intersect, &a, &a)), &alone);
        prop_assert!(polygonize_components(&csg(CsgOp::Subtract, &a, &a)).is_empty());
        prop_assert!(polygonize_components(&csg(CsgOp::Xor, &a, &a)).is_empty());
    }

    /// Two placed leaves: the intersection lies in each operand, the union
    /// is no bigger than both together and no smaller than either, and the
    /// areas obey inclusion-exclusion; the symmetric difference is the
    /// union less the intersection, and `A − B` is `A` less `A ∩ B`.
    ///
    /// Tolerance: each contoured area is off its exact region by at most
    /// [`band`] (see there); a relation over `k` contoured areas gets the
    /// sum of their bands. Vertex containment: a vertex of `A ∩ B` is on
    /// the zero set of `max(a, b)` to within the bisection step
    /// (`cell / 2¹²`), so `a ≤ cell / 1000` there.
    #[test]
    fn booleans_of_two_leaves_obey_set_algebra(a in placed_leaf(), b in placed_leaf()) {
        let union = csg(CsgOp::Union, &a, &b);
        let inter = csg(CsgOp::Intersect, &a, &b);
        let xor = csg(CsgOp::Xor, &a, &b);
        let sub = csg(CsgOp::Subtract, &a, &b);
        let parts = |s: &ShapeDef| polygonize_components(s);
        let (pa, pb, pu, pi, px, ps) =
            (parts(&a), parts(&b), parts(&union), parts(&inter), parts(&xor), parts(&sub));
        for p in [&pu, &pi, &px, &ps] {
            well_formed(p)?;
        }
        let (aa, ab, au, ai, ax, asub) =
            (area(&pa), area(&pb), area(&pu), area(&pi), area(&px), area(&ps));
        let (ba, bb, bu, bi, bx, bs) = (
            band(&a, &pa),
            band(&b, &pb),
            band(&union, &pu),
            band(&inter, &pi),
            band(&xor, &px),
            band(&sub, &ps),
        );
        let cell = cell_of(&inter);
        for v in pi.iter().flat_map(Contours::rings).flatten() {
            prop_assert!(sdf::eval(&a, *v) <= cell / 1000.0, "A ∩ B vertex {v} outside A");
            prop_assert!(sdf::eval(&b, *v) <= cell / 1000.0, "A ∩ B vertex {v} outside B");
        }
        prop_assert!(ai <= aa.min(ab) + bi + ba.max(bb), "|A∩B| {ai} vs {aa}, {ab}");
        prop_assert!(au <= aa + ab + bu + ba + bb, "|A∪B| {au} > {aa} + {ab}");
        prop_assert!(au >= aa.max(ab) - bu - ba.max(bb), "|A∪B| {au} < max({aa}, {ab})");
        prop_assert!(
            (au + ai - aa - ab).abs() <= bu + bi + ba + bb,
            "inclusion-exclusion: {au} + {ai} vs {aa} + {ab}"
        );
        prop_assert!((ax - (au - ai)).abs() <= bx + bu + bi, "xor {ax} vs {au} - {ai}");
        prop_assert!((asub - (aa - ai)).abs() <= bs + ba + bi, "A−B {asub} vs {aa} - {ai}");
    }

    /// A union of placed exact leaves evaluates through the flattened
    /// `UnionField` exactly as through `sdf::eval` (its docs promise
    /// bit-for-bit equality; the contour relies on it).
    #[test]
    fn the_flattened_union_field_is_the_tree_field(
        leaves in proptest::collection::vec(
            (prop_oneof![
                (0.1f32..2.0, 0.1f32..2.0).prop_map(|(width, height)| ShapeDef::Box { width, height }),
                (0.1f32..1.0).prop_map(|radius| ShapeDef::Circle { radius }),
                (0.05f32..1.0, 0.05f32..0.5)
                    .prop_map(|(half_length, radius)| ShapeDef::Capsule { half_length, radius }),
            ], -2.0f32..2.0, -2.0f32..2.0, -PI..PI),
            2..10,
        ),
        probes in proptest::collection::vec((-4.0f32..4.0, -4.0f32..4.0), 64),
    ) {
        let placed: Vec<ShapeDef> = leaves
            .into_iter()
            .map(|(shape, x, y, rot)| ShapeDef::Placed {
                pos: Vec2::new(x, y),
                rot,
                shape: Box::new(shape),
            })
            .collect();
        let tree = crate::sketch::balanced_union(&placed).expect("two or more leaves");
        let field = sdf::UnionField::of(&tree).expect("a union of placed exact leaves");
        for (x, y) in probes {
            let p = Vec2::new(x, y);
            prop_assert_eq!(field.eval(p).to_bits(), sdf::eval(&tree, p).to_bits(), "at {}", p);
        }
    }
}

/// How far a contoured area may be from the exact area of its region:
/// `0.25 · cell · P + 4 · cell² · (components + 1)`, `P` the perimeter the
/// contour measures.
///
/// The leaf bound (see [`a_contoured_leaf_keeps_its_analytic_area`]) is
/// `0.05 · cell · P` for the simplification plus a chamfer per corner. A
/// boolean makes corners anywhere along the boundary (wherever two
/// operand boundaries cross), and drops slivers under `4 · cell²` on
/// purpose, so the per-length term is five times the leaf's and every
/// component may lose a sliver.
fn band(shape: &ShapeDef, parts: &[Contours]) -> f32 {
    let cell = cell_of(shape);
    0.25 * cell * perimeter(parts) + 4.0 * cell * cell * (parts.len() as f32 + 1.0)
}

// ---------------------------------------------------------------------
// 3. Phase N shape math
// ---------------------------------------------------------------------

/// A drawn stroke: a random walk of 2-60 points with steps of 0-40 cm and
/// a heading that drifts, sometimes pulled closed onto its first point.
fn stroke() -> impl Strategy<Value = Vec<Vec2>> {
    (
        proptest::collection::vec((0.0f32..0.4, -1.2f32..1.2), 1..60),
        any::<bool>(),
        -5.0f32..5.0,
        -5.0f32..5.0,
    )
        .prop_map(|(steps, close, x, y)| {
            let mut heading = 0.0f32;
            let mut points = vec![Vec2::new(x, y)];
            for (len, turn) in steps {
                heading += turn;
                let last = points[points.len() - 1];
                points.push(last + Vec2::from_angle(heading) * len);
            }
            if close && points.len() > 3 {
                points.push(points[0]);
            }
            points
        })
}

/// A drawn loop: a noisy circle that ends where it began (the closed
/// sketch path).
fn loop_stroke() -> impl Strategy<Value = Vec<Vec2>> {
    (
        proptest::collection::vec(-0.15f32..0.15, 8..60),
        0.2f32..3.0,
    )
        .prop_map(|(noise, r)| {
            let n = noise.len() as f32;
            let mut points: Vec<Vec2> = noise
                .iter()
                .enumerate()
                .map(|(i, e)| Vec2::from_angle(TAU * i as f32 / n) * r * (1.0 + e))
                .collect();
            points.push(points[0]);
            points
        })
}

proptest! {
    #![proptest_config(config(256))]

    /// A stroke makes a valid shape or none: none only when it is shorter
    /// than [`MIN_SKETCH_LENGTH`] (after dropping repeated points); a
    /// closed stroke is a simple counter-clockwise polygon about its
    /// centroid; an open one is a union of at most
    /// [`MAX_SKETCH_SEGMENTS`] capsules within the depth cap, whose
    /// centre line lies inside it, and which contours as **one piece**
    /// (the radius is widened until the contour resolves it).
    ///
    /// Tolerances: a centre-line vertex is `radius` deep in its capsule,
    /// and a capsule's field is exact, so it must read `≤ −radius` to
    /// `1e-4 · (1 + |p|)` (the body-frame rotation in `f32`). The closed
    /// centroid is at the origin to `1e-4` of the ring's extent.
    #[test]
    fn a_stroke_makes_a_valid_connected_shape(
        points in prop_oneof![stroke(), loop_stroke()],
        radius in 0.005f32..0.2,
    ) {
        let Some(sketch) = sketch_shape(&points, radius) else {
            prop_assert!(
                path_length(&points) < MIN_SKETCH_LENGTH + 2.0 * crate::shape::MIN_SHAPE_SIZE * points.len() as f32,
                "a {}-m stroke made no shape",
                path_length(&points)
            );
            return Ok(());
        };
        prop_assert!(sketch.shape.validate().is_ok());
        prop_assert!(sketch.shape.depth() <= MAX_CSG_DEPTH);
        prop_assert!(sketch.radius >= radius);
        if sketch.closed {
            let ShapeDef::Polygon { outline, holes } = &sketch.shape else {
                return Err(TestCaseError::fail("a closed sketch is a polygon"));
            };
            prop_assert!(holes.is_empty());
            prop_assert!(outline.len() <= MAX_SKETCH_OUTLINE);
            prop_assert!(ring_signed_area(outline) > 0.0, "counter-clockwise");
            prop_assert!(ring_is_simple(outline), "simple");
            let extent = outline.iter().map(|v| v.length()).fold(0.0, f32::max);
            let centroid = crate::contours::ring_centroid(outline);
            prop_assert!(centroid.length() <= 1e-4 * extent.max(1.0), "centroid {centroid}");
        } else {
            prop_assert!(leaf_count(&sketch.shape) <= MAX_SKETCH_SEGMENTS);
            prop_assert!(sketch.line.len() <= MAX_SKETCH_SEGMENTS + 1);
            let to_body = Vec2::from_angle(-sketch.rot);
            for p in &sketch.line {
                let local = to_body.rotate(*p - sketch.pos);
                let depth = sdf::eval(&sketch.shape, local);
                prop_assert!(
                    depth <= -sketch.radius + 1e-4 * (1.0 + local.length()),
                    "centre-line point {p} reads {depth} (radius {})",
                    sketch.radius
                );
            }
            let parts = polygonize_components(&sketch.shape);
            prop_assert_eq!(parts.len(), 1, "an open stroke is one piece");
            well_formed(&parts)?;
        }
    }

    /// A chain split: `n + 1` points from the path's first point to its
    /// last (exactly), `n` the nearest whole number to `length / link`
    /// within `1..=MAX_CHAIN_LINKS`; no link is longer than its arc share
    /// and the links never add up to more than the path.
    ///
    /// Tolerance: `1e-4` of the path length for the `f32` arc walk (one
    /// rounding per segment walked).
    #[test]
    fn a_chain_splits_its_path_by_arc_length(
        path in stroke(),
        link in 0.01f32..1.0,
    ) {
        let total = path_length(&path);
        let Some(points) = chain_points(&path, link) else {
            prop_assert!(total < MIN_CHAIN_LENGTH, "a {total}-m path made no chain");
            return Ok(());
        };
        let n = ((total / link).round() as usize).clamp(1, MAX_CHAIN_LINKS);
        prop_assert_eq!(points.len(), n + 1);
        prop_assert_eq!(points[0], path[0]);
        prop_assert_eq!(points[n], path[path.len() - 1]);
        let pitch = total / n as f32;
        let eps = 1e-4 * total.max(1.0);
        for w in points.windows(2) {
            prop_assert!(w[0].distance(w[1]) <= pitch + eps, "link {} > pitch {pitch}", w[0].distance(w[1]));
        }
        prop_assert!(path_length(&points) <= total + eps);
    }

    /// On a straight path every link is exactly its share.
    #[test]
    fn a_straight_chain_has_equal_links(
        start in (-5.0f32..5.0, -5.0f32..5.0),
        heading in -PI..PI,
        length in 0.05f32..10.0,
        cuts in proptest::collection::vec(0.0f32..1.0, 0..6),
        link in 0.01f32..1.0,
    ) {
        let a = Vec2::new(start.0, start.1);
        let dir = Vec2::from_angle(heading);
        // Extra collinear points along the way (sorted) must not matter.
        let mut ts = cuts;
        ts.sort_by(f32::total_cmp);
        let path: Vec<Vec2> = std::iter::once(0.0)
            .chain(ts)
            .chain(std::iter::once(1.0))
            .map(|t| a + dir * length * t)
            .collect();
        let points = chain_points(&path, link).expect("a straight path of at least 5 cm");
        let pitch = path_length(&path) / (points.len() - 1) as f32;
        for w in points.windows(2) {
            prop_assert!((w[0].distance(w[1]) - pitch).abs() <= 1e-4 * length.max(1.0));
        }
    }

    /// A width profile is never negative, never above its maximum, the
    /// same for `d` and `−d`, and no wider than the outline's diameter.
    ///
    /// Tolerance: `1e-5 · (1 + width)` on the `±d` symmetry (the angle
    /// wraps through `atan2` and `rem_euclid` in `f32`).
    #[test]
    fn a_width_profile_is_never_negative(
        shape in tree(),
        directions in proptest::collection::vec(-PI..PI, 32),
    ) {
        let contours = polygonize(&shape);
        let profile = WidthProfile::from_contours(&contours);
        let max = profile.max_width();
        let diameter = contours
            .outline
            .iter()
            .flat_map(|p| contours.outline.iter().map(move |q| p.distance(*q)))
            .fold(0.0, f32::max);
        prop_assert!(max <= diameter * (1.0 + 1e-5) + 1e-6, "max {max} > diameter {diameter}");
        for angle in directions {
            let d = Vec2::from_angle(angle);
            let w = profile.width_across(d);
            prop_assert!(w >= 0.0 && w.is_finite(), "width {w} across {d}");
            prop_assert!(w <= max * (1.0 + 1e-6), "{w} > max {max}");
            let back = profile.width_across(-d);
            prop_assert!((w - back).abs() <= 1e-5 * (1.0 + w), "{w} vs {back} reversed");
        }
    }

    /// Any point set, including an empty one, gives a profile with no
    /// negative or non-finite width.
    #[test]
    fn a_point_set_profile_is_never_negative(
        points in proptest::collection::vec((-1e3f32..1e3, -1e3f32..1e3), 0..8),
        direction in -PI..PI,
    ) {
        let points: Vec<Vec2> = points.into_iter().map(|(x, y)| Vec2::new(x, y)).collect();
        let w = WidthProfile::from_points(&points).width_across(Vec2::from_angle(direction));
        prop_assert!(w >= 0.0 && w.is_finite());
    }

    /// A body wholly inside a fluid is wholly submerged: its submerged
    /// area and centroid are its own (in a convex box tank, a convex
    /// 48-gon tank, a concave L tank split into triangles, and under an
    /// open sea); one wholly outside has none; and the parts below and
    /// above a surface add up to the whole.
    ///
    /// Tolerance: `1e-4` relative on areas plus the `f32` rounding bound
    /// of the shoelace sum ([`shoelace_rounding`]: a thin body far from
    /// the origin sums large cross terms that cancel), and `1e-3` of the
    /// body's radius on centroids plus the same rounding over the area.
    /// A wholly inside ring clips to itself, so only the `f32` sums over
    /// pieces differ (the triangulated L adds one ring sum per triangle).
    #[test]
    fn a_body_inside_a_fluid_is_wholly_submerged(
        shape in tree(),
        tank in 0usize..4,
        offset in (-PI..PI, 0.0f32..1.0),
        rot in -PI..PI,
        surface in -1.0f32..1.0,
    ) {
        let outline = SolidOutline::from_components(&polygonize_components(&shape));
        prop_assume!(!outline.is_empty());
        let whole = outline.moment();
        // The tank, the centre of a disc inside it, and that disc's radius.
        let (fluid, centre, room) = match tank {
            0 => (
                FluidVolume::from_components(&[polygonize(&ShapeDef::Box { width: 12.0, height: 10.0 })]),
                Vec2::ZERO,
                5.0,
            ),
            1 => (
                FluidVolume::from_components(&[polygonize(&ShapeDef::Circle { radius: 7.0 })]),
                Vec2::ZERO,
                7.0 * (PI / CIRCLE_SEGMENTS as f32).cos(),
            ),
            2 => {
                let l = ShapeDef::Polygon {
                    outline: vec![
                        Vec2::new(0.0, 0.0),
                        Vec2::new(20.0, 0.0),
                        Vec2::new(20.0, 10.0),
                        Vec2::new(10.0, 10.0),
                        Vec2::new(10.0, 20.0),
                        Vec2::new(0.0, 20.0),
                    ],
                    holes: vec![],
                };
                (FluidVolume::from_components(&[polygonize(&l)]), Vec2::splat(5.0), 5.0)
            }
            _ => (FluidVolume::half_plane(), Vec2::new(0.0, -6.0), 5.0),
        };
        // Scale the body to fit with a margin, then place it in the disc.
        let scale = (0.9 * room / outline.radius()).min(1.0);
        let free = (0.95 * room - outline.radius() * scale).max(0.0);
        let at = centre + Vec2::from_angle(offset.0) * free * offset.1;
        let rotation = Vec2::from_angle(rot);
        let rings: Vec<Vec<Vec2>> = outline
            .rings()
            .map(|r| r.iter().map(|v| rotation.rotate(*v * scale) + at).collect())
            .collect();
        let full: f32 = rings.iter().map(|r| ring_moment(r).area).sum();
        let tol = 1e-4 * full + shoelace_rounding(&rings);
        prop_assert!((full - whole.area * scale * scale).abs() <= tol, "{full} vs {}", whole.area * scale * scale);
        let mut scratch = ClipScratch::default();
        let inside = fluid.submerged(rings.iter().map(Vec::as_slice), &mut scratch);
        prop_assert!((inside.area - full).abs() <= tol, "submerged {} of {full}", inside.area);
        let first: Vec2 = rings.iter().map(|r| ring_moment(r).first).sum();
        let (Some(c_in), true) = (inside.centroid(), full > 0.0) else {
            return Err(TestCaseError::fail("a submerged body has a centroid"));
        };
        prop_assert!(
            (c_in - first / full).length()
                <= 1e-3 * (outline.radius() * scale).max(1e-3) + (1.0 + at.length()) * tol / full,
            "centroid {c_in} vs {}",
            first / full
        );

        // Far outside: nothing.
        let away: Vec<Vec<Vec2>> = rings.iter().map(|r| r.iter().map(|v| *v + Vec2::new(0.0, 500.0)).collect()).collect();
        let none = fluid.submerged(away.iter().map(Vec::as_slice), &mut scratch);
        prop_assert!(none.area.abs() <= tol, "{} submerged far away", none.area);

        // Across a sea surface: below + above = whole, each within [0, whole].
        let level = Vec2::new(0.0, surface * outline.radius() * scale);
        let sea = FluidVolume::half_plane();
        let local: Vec<Vec<Vec2>> = rings.iter().map(|r| r.iter().map(|v| *v - at - level).collect()).collect();
        let flipped: Vec<Vec<Vec2>> = local.iter().map(|r| r.iter().map(|v| -*v).collect()).collect();
        let below = sea.submerged(local.iter().map(Vec::as_slice), &mut scratch).area;
        let above = sea.submerged(flipped.iter().map(Vec::as_slice), &mut scratch).area;
        prop_assert!(below >= -tol && above >= -tol, "{below}, {above}");
        prop_assert!((below + above - full).abs() <= 3.0 * tol, "{below} + {above} vs {full}");
    }

    /// Mirroring twice across the same axis gives the tree back: bit for
    /// bit when it has no half-plane, and the same field when it has one
    /// (a half-plane mirrored across x comes back placed at a half turn,
    /// by design; see `mirror_shape`). Mirroring once reflects the field
    /// and keeps validity, depth and area.
    ///
    /// Tolerance: the field of a reflected tree is the original field at
    /// the reflected point to `1e-4 · (1 + |p|)` (`R(−θ)` and `R(θ)` round
    /// differently in `f32`); the mirrored contour's area is within one
    /// [`band`] of the original's (the grid is not reflected with it).
    #[test]
    fn mirroring_twice_is_the_identity(
        shape in tree_with_planes(),
        axis in prop_oneof![Just(MirrorAxis::X), Just(MirrorAxis::Y)],
        probes in proptest::collection::vec((-3.0f32..3.0, -3.0f32..3.0), 32),
    ) {
        let once = mirror_shape(&shape, axis);
        let twice = mirror_shape(&once, axis);
        prop_assert_eq!(once.validate(), shape.validate());
        prop_assert!(once.depth() <= shape.depth() + 1);
        if shape.contains_half_plane() {
            for &(x, y) in &probes {
                let p = Vec2::new(x, y);
                let (a, b) = (sdf::eval(&shape, p), sdf::eval(&twice, p));
                prop_assert!((a - b).abs() <= 1e-4 * (1.0 + p.length()), "at {p}: {a} vs {b}");
            }
        } else {
            prop_assert_eq!(&twice, &shape);
        }
        for (x, y) in probes {
            let p = Vec2::new(x, y);
            let (a, b) = (sdf::eval(&shape, p), sdf::eval(&once, axis.point(p)));
            prop_assert!((a - b).abs() <= 1e-4 * (1.0 + p.length()), "at {p}: {a} vs reflected {b}");
        }
        if !shape.contains_half_plane() {
            let (before, after) = (polygonize_components(&shape), polygonize_components(&once));
            let (a, b) = (area(&before), area(&after));
            prop_assert!(
                (a - b).abs() <= band(&shape, &before) + band(&once, &after),
                "area {a} mirrored to {b}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// Regressions: minimised cases the properties above found
// ---------------------------------------------------------------------

/// Regression (found by [`the_flattened_union_field_is_the_tree_field`],
/// minimised): inside a leaf's bounding box, `UnionField` skipped the leaf
/// whenever the best value so far was negative (`0 ≥ best + margin`), so a
/// point deep in a box but shallow in an overlapping circle read the
/// circle's depth. The sign was right (the contour was unaffected); the
/// value was not the tree's field, which its docs promise.
#[test]
fn regression_union_field_never_culls_a_leaf_whose_box_holds_the_point() {
    let placed = |shape: ShapeDef, x: f32| ShapeDef::Placed {
        pos: Vec2::new(x, 0.0),
        rot: 0.0,
        shape: Box::new(shape),
    };
    let tree = csg(
        CsgOp::Union,
        &placed(
            ShapeDef::Circle {
                radius: 0.643_371_3,
            },
            1.568_845_9,
        ),
        &placed(
            ShapeDef::Box {
                width: 1.906_128,
                height: 1.723_461_7,
            },
            1.572_141_8,
        ),
    );
    let field = sdf::UnionField::of(&tree).expect("a union of placed exact leaves");
    let p = Vec2::new(1.127_605, 0.0);
    assert_eq!(field.eval(p).to_bits(), sdf::eval(&tree, p).to_bits());
}
/// Regression (found by [`a_contoured_leaf_keeps_its_analytic_area`]
/// with `PROPTEST_RNG_SEED=2 PROPTEST_CASES=2000`, minimised): the
/// contour's collinear-vertex simplification tested each vertex against
/// the chord from the last kept vertex to its *next* neighbour only, so a
/// run of dropped vertices could drift off the chord finally drawn. Along
/// this capsule's flat side the drift carried one edge 0.48 m long into
/// both caps, `4.4e-4` m inside the curve (more than its `0.05 · cell`
/// tolerance), and the area came out 0.63 % short.
///
/// Tolerance: every point of every edge within the simplification
/// tolerance `0.05 · cell` of the boundary, plus the sag of a one-cell
/// chord on the caps (`cell² / 8r`).
#[test]
fn regression_contour_simplification_stays_within_its_tolerance() {
    let rod = ShapeDef::Placed {
        pos: Vec2::ZERO,
        rot: 0.260_418_65,
        shape: Box::new(ShapeDef::Capsule {
            half_length: 0.233_102_3,
            radius: 0.066_906_26,
        }),
    };
    let cell = cell_of(&rod);
    let tol = 0.05 * cell + cell * cell / (8.0 * 0.066_906_26) + 1e-6;
    let parts = polygonize_components(&rod);
    assert_eq!(parts.len(), 1);
    let ring = &parts[0].outline;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        for k in 1..8 {
            let p = a.lerp(b, k as f32 / 8.0);
            let off = sdf::eval(&rod, p).abs();
            assert!(
                off <= tol,
                "edge {i} at {p} is {off} off the rod (tol {tol})"
            );
        }
    }
}

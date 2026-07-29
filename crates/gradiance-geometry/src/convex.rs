//! Exact convex decomposition — the collider construction path.
//!
//! A 3D physics engine's narrow phase handles non-convex geometry only through
//! its own composites, so a body's collider is a *compound of convex pieces*.
//! This module produces those pieces, in the pure layer, strictly downstream of
//! [`polygonize`](crate::polygonize) so the SDF tree keeps exactly one
//! discretization point.
//!
//! The pipeline is [`tessellate`](crate::tessellate) (lyon, even-odd fill, so
//! holes need no special handling) followed by **Hertel–Mehlhorn**: walk the
//! triangulation's internal edges and delete every one whose removal leaves
//! both sides convex. It is exact, deterministic, and produces at most four
//! times the minimum number of pieces.
//!
//! Deliberately *not* the physics engine's own `convex_decomposition`, which is
//! V-HACD: approximate, slow, non-deterministic across runs, and it produces
//! more pieces than this for the polygonal input we actually have.
//!
//! # Piece count is the scaling cost
//!
//! An `N`-piece compound costs `O(N)` broad-phase leaves and `O(N·M)`
//! narrow-phase sub-pairs against an `M`-piece neighbour, so piece count — not
//! vertex count — is what governs how many bodies a scene can hold. Callers
//! compare against [`MAX_PIECES`] and fall back to a single convex hull.

use crate::contours::{Contours, ring_signed_area};
use crate::tessellate::tessellate;
use bevy::math::Vec2;
use std::collections::HashMap;

/// A convex polygon, counter-clockwise, in the plane. Never fewer than three
/// vertices — the unit a collider is built from.
pub type ConvexPiece = Vec<Vec2>;

/// Hard ceiling on the pieces one body's collider may be built from.
///
/// Above this a caller should fall back to the shape's convex hull: a body that
/// costs more than this to represent exactly is not worth the narrow-phase
/// budget, and losing its concavity is a better trade than losing frame rate.
pub const MAX_PIECES: usize = 24;

/// Soft budget. Crossing it is worth one diagnostic per shape change — it is
/// usually a sign of an over-detailed CSG tree, not of a genuinely complex body.
pub const PIECE_BUDGET_HINT: usize = 8;

/// Pieces below this area (m²) are dropped as tessellation slivers.
///
/// Lyon emits zero-area triangles at self-touching vertices; handing one to a
/// physics engine produces a degenerate shape with no useful support map.
pub const MIN_PIECE_AREA: f32 = 1e-8;

/// How close to straight a merged corner may be before it counts as reflex.
///
/// Compared against the *normalized* cross product (the sine of the turn), so
/// the threshold is scale-free — the same for a millimetre bracket and a
/// hundred-metre ground slab.
const CONVEX_EPS: f32 = 1e-6;

/// Decomposes the filled region of `contours` (outline minus holes) into convex
/// pieces with disjoint interiors.
///
/// Returns an empty vector for degenerate input. The result is deterministic:
/// the same contours always produce the same pieces in the same order.
///
/// ```
/// use gradiance_geometry::contours::Contours;
/// use gradiance_geometry::convex::convex_decompose;
/// use bevy::math::Vec2;
///
/// // A concave L needs more than one convex piece — but not many.
/// let l = Contours {
///     outline: vec![
///         Vec2::new(0.0, 0.0),
///         Vec2::new(2.0, 0.0),
///         Vec2::new(2.0, 1.0),
///         Vec2::new(1.0, 1.0),
///         Vec2::new(1.0, 2.0),
///         Vec2::new(0.0, 2.0),
///     ],
///     holes: vec![],
/// };
/// let pieces = convex_decompose(&l);
/// assert_eq!(pieces.len(), 2);
/// ```
#[must_use]
pub fn convex_decompose(contours: &Contours) -> Vec<ConvexPiece> {
    let triangulation = tessellate(contours);
    if triangulation.indices.len() < 3 {
        return Vec::new();
    }
    merge_triangles(&triangulation.vertices, &triangulation.indices)
}

/// Decomposes several disjoint components — the shape of
/// [`polygonize_components`](crate::polygonize::polygonize_components) output —
/// into one flat piece list.
#[must_use]
pub fn convex_decompose_components(components: &[Contours]) -> Vec<ConvexPiece> {
    components.iter().flat_map(convex_decompose).collect()
}

/// Hertel–Mehlhorn over an indexed triangulation.
fn merge_triangles(vertices: &[Vec2], indices: &[u32]) -> Vec<ConvexPiece> {
    // One polygon per triangle, each forced counter-clockwise.
    let mut polygons: Vec<Option<Vec<u32>>> = Vec::with_capacity(indices.len() / 3);
    // Which polygon each triangle currently belongs to, and the reverse.
    let mut owner: Vec<usize> = Vec::with_capacity(indices.len() / 3);
    let mut members: Vec<Vec<usize>> = Vec::with_capacity(indices.len() / 3);

    for tri in indices.chunks_exact(3) {
        let mut ring = vec![tri[0], tri[1], tri[2]];
        if signed_area(vertices, &ring) < 0.0 {
            ring.reverse();
        }
        let id = polygons.len();
        polygons.push(Some(ring));
        owner.push(id);
        members.push(vec![id]);
    }

    // Internal edges: those shared by exactly two triangles. `BTreeMap`-free —
    // determinism comes from iterating `indices`, not the map.
    let mut shared: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    let mut order: Vec<(u32, u32)> = Vec::new();
    for (t, tri) in indices.chunks_exact(3).enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let key = if a < b { (a, b) } else { (b, a) };
            let entry = shared.entry(key).or_default();
            if entry.is_empty() {
                order.push(key);
            }
            entry.push(t);
        }
    }

    for key in order {
        let Some(pair) = shared.get(&key) else {
            continue;
        };
        let [left, right] = pair[..] else {
            continue; // boundary edge, or a non-manifold fold
        };
        let (a, b) = (owner[left], owner[right]);
        if a == b {
            continue; // already merged through another diagonal
        }
        let (Some(ring_a), Some(ring_b)) = (polygons[a].as_ref(), polygons[b].as_ref()) else {
            continue;
        };
        let Some(merged) = merge_across(vertices, ring_a, ring_b, key.0, key.1) else {
            continue; // the corner would go reflex — this diagonal is essential
        };

        polygons[a] = Some(merged);
        polygons[b] = None;
        let moved = std::mem::take(&mut members[b]);
        for t in &moved {
            owner[*t] = a;
        }
        members[a].extend(moved);
    }

    polygons
        .into_iter()
        .flatten()
        .map(|ring| ring.iter().map(|i| vertices[*i as usize]).collect())
        .filter(|piece: &ConvexPiece| {
            piece.len() >= 3 && ring_signed_area(piece).abs() >= MIN_PIECE_AREA
        })
        .collect()
}

/// Splices two counter-clockwise polygons that share the undirected edge
/// `(u, v)`, returning the merged ring if it is still convex.
///
/// Both inputs are convex, so only the two junction vertices can turn reflex —
/// that is the whole Hertel–Mehlhorn test.
fn merge_across(vertices: &[Vec2], a: &[u32], b: &[u32], u: u32, v: u32) -> Option<Vec<u32>> {
    // Orient the shared edge as it runs in `a`, so `b` must contain it reversed.
    let (start, end, ia) = match directed_edge(a, u, v) {
        Some(i) => (u, v, i),
        None => (v, u, directed_edge(a, v, u)?),
    };
    let jb = directed_edge(b, end, start)?;

    // `a` from `end` all the way round to `start`, then `b`'s remaining
    // vertices — everything strictly between `start` and `end`.
    let mut merged = Vec::with_capacity(a.len() + b.len() - 2);
    for t in 0..a.len() {
        merged.push(a[(ia + 1 + t) % a.len()]);
    }
    for t in 0..b.len().saturating_sub(2) {
        merged.push(b[(jb + 2 + t) % b.len()]);
    }

    // The junctions are the two shared vertices, now interior to the merge.
    let at_start = merged.iter().position(|i| *i == start)?;
    let at_end = merged.iter().position(|i| *i == end)?;
    (is_convex_at(vertices, &merged, at_start) && is_convex_at(vertices, &merged, at_end))
        .then_some(merged)
}

/// The index `i` where `ring[i] == from` and `ring[i + 1] == to`.
fn directed_edge(ring: &[u32], from: u32, to: u32) -> Option<usize> {
    (0..ring.len()).find(|i| ring[*i] == from && ring[(*i + 1) % ring.len()] == to)
}

/// Whether the corner at `i` turns left (or runs straight) — scale-free.
fn is_convex_at(vertices: &[Vec2], ring: &[u32], i: usize) -> bool {
    let n = ring.len();
    let prev = vertices[ring[(i + n - 1) % n] as usize];
    let here = vertices[ring[i] as usize];
    let next = vertices[ring[(i + 1) % n] as usize];
    let (into, out) = (here - prev, next - here);
    let scale = into.length() * out.length();
    if scale < f32::EPSILON {
        return true; // a duplicate vertex cannot make the ring reflex
    }
    into.perp_dot(out) / scale >= -CONVEX_EPS
}

fn signed_area(vertices: &[Vec2], ring: &[u32]) -> f32 {
    let points: Vec<Vec2> = ring.iter().map(|i| vertices[*i as usize]).collect();
    ring_signed_area(&points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hull::convex_hull;
    use crate::polygonize::polygonize;
    use crate::shape::ShapeDef;
    use std::f32::consts::TAU;

    /// Every piece is convex, counter-clockwise, and non-degenerate.
    fn assert_pieces_are_convex(pieces: &[ConvexPiece]) {
        for (p, piece) in pieces.iter().enumerate() {
            assert!(piece.len() >= 3, "piece {p} has {} vertices", piece.len());
            let area = ring_signed_area(piece);
            assert!(
                area > 0.0,
                "piece {p} is not counter-clockwise (area {area})"
            );
            let n = piece.len();
            for i in 0..n {
                let (prev, here, next) = (piece[(i + n - 1) % n], piece[i], piece[(i + 1) % n]);
                let (into, out) = (here - prev, next - here);
                let scale = into.length() * out.length();
                if scale < f32::EPSILON {
                    continue;
                }
                let sine = into.perp_dot(out) / scale;
                assert!(sine >= -1e-4, "piece {p} is reflex at {i} (sine {sine})");
            }
        }
    }

    fn total_area(pieces: &[ConvexPiece]) -> f32 {
        pieces.iter().map(|p| ring_signed_area(p).abs()).sum()
    }

    fn l_shape() -> Contours {
        Contours {
            outline: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
            holes: vec![],
        }
    }

    /// A comb with `teeth` prongs — the stress case for piece count.
    fn comb(teeth: usize) -> Contours {
        let mut outline = vec![Vec2::new(0.0, 0.0)];
        for i in 0..teeth {
            let x = i as f32 * 2.0;
            outline.push(Vec2::new(x + 1.0, 0.0));
            outline.push(Vec2::new(x + 1.0, 2.0));
            outline.push(Vec2::new(x + 2.0, 2.0));
            outline.push(Vec2::new(x + 2.0, 0.0));
        }
        outline.push(Vec2::new(teeth as f32 * 2.0 + 1.0, 0.0));
        outline.push(Vec2::new(teeth as f32 * 2.0 + 1.0, -1.0));
        outline.push(Vec2::new(0.0, -1.0));
        Contours {
            outline,
            holes: vec![],
        }
    }

    fn ngon(radius: f32, n: usize) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f32 / n as f32;
                Vec2::new(radius * a.cos(), radius * a.sin())
            })
            .collect()
    }

    #[test]
    fn a_convex_shape_stays_one_piece() {
        let square = Contours {
            outline: vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(-1.0, 1.0),
            ],
            holes: vec![],
        };
        let pieces = convex_decompose(&square);
        assert_eq!(pieces.len(), 1, "a square is already convex");
        assert_pieces_are_convex(&pieces);
        assert!((total_area(&pieces) - 4.0).abs() < 1e-3);
    }

    #[test]
    fn an_l_splits_into_two() {
        let pieces = convex_decompose(&l_shape());
        assert_pieces_are_convex(&pieces);
        assert_eq!(pieces.len(), 2, "an L is exactly two rectangles");
        assert!((total_area(&pieces) - 3.0).abs() < 1e-3);
    }

    #[test]
    fn area_is_conserved() {
        for contours in [l_shape(), comb(4)] {
            let expected = contours.area();
            let pieces = convex_decompose(&contours);
            assert_pieces_are_convex(&pieces);
            let got = total_area(&pieces);
            assert!(
                (got - expected).abs() < expected * 1e-3,
                "got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn holes_are_decomposed_and_subtracted() {
        let mut hole = ngon(1.0, 8);
        hole.reverse();
        let contours = Contours {
            outline: ngon(3.0, 8),
            holes: vec![hole],
        };
        let expected = contours.area();
        let pieces = convex_decompose(&contours);
        assert_pieces_are_convex(&pieces);
        assert!(pieces.len() > 1, "an annulus cannot be one convex piece");
        let got = total_area(&pieces);
        assert!(
            (got - expected).abs() < expected * 1e-3,
            "got {got}, want {expected}"
        );
    }

    #[test]
    fn merging_beats_the_raw_triangulation() {
        // The Hertel-Mehlhorn payoff: far fewer pieces than triangles.
        let contours = comb(6);
        let triangles = tessellate(&contours).indices.len() / 3;
        let pieces = convex_decompose(&contours).len();
        assert!(
            pieces * 2 <= triangles,
            "{pieces} pieces vs {triangles} triangles — merging did nothing"
        );
    }

    #[test]
    fn a_disc_collapses_to_one_piece() {
        // A convex polygon of any vertex count must merge all the way back.
        let pieces = convex_decompose(&Contours {
            outline: ngon(2.0, 64),
            holes: vec![],
        });
        assert_eq!(pieces.len(), 1);
        assert_pieces_are_convex(&pieces);
    }

    #[test]
    fn pieces_never_exceed_the_hull() {
        // Every piece lies inside the shape's convex hull — the guarantee that
        // makes the hull a sound over-budget fallback.
        let pieces = convex_decompose(&l_shape());
        let hull = convex_hull(&l_shape().outline);
        for piece in &pieces {
            for v in piece {
                let n = hull.len();
                let inside = (0..n).all(|i| {
                    let (a, b) = (hull[i], hull[(i + 1) % n]);
                    (b - a).perp_dot(*v - a) >= -1e-3
                });
                assert!(inside, "{v} escaped the hull");
            }
        }
    }

    #[test]
    fn degenerate_input_yields_nothing() {
        assert!(
            convex_decompose(&Contours {
                outline: vec![],
                holes: vec![]
            })
            .is_empty()
        );
        assert!(
            convex_decompose(&Contours {
                outline: vec![Vec2::ZERO, Vec2::X],
                holes: vec![]
            })
            .is_empty()
        );
    }

    #[test]
    fn a_csg_tree_decomposes() {
        // The real production path: an SDF tree through polygonize.
        let shape = ShapeDef::Csg {
            op: crate::shape::CsgOp::Subtract,
            lhs: Box::new(ShapeDef::Box {
                width: 2.0,
                height: 2.0,
            }),
            rhs: Box::new(ShapeDef::Circle { radius: 0.6 }),
        };
        let contours = polygonize(&shape);
        let pieces = convex_decompose(&contours);
        assert_pieces_are_convex(&pieces);
        assert!(!pieces.is_empty());
        let got = total_area(&pieces);
        let expected = contours.area();
        assert!(
            (got - expected).abs() < expected * 5e-3,
            "got {got}, want {expected}"
        );
    }

    #[test]
    fn is_deterministic() {
        let contours = comb(5);
        let a = convex_decompose(&contours);
        let b = convex_decompose(&contours);
        assert_eq!(a, b, "the same contours must give the same pieces");
    }
}

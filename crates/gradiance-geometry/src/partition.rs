//! Exact convex partition of a polygon (brief S-5).
//!
//! A collider made by an approximate convex decomposition covers the shape
//! with a few hulls, and a hull fills every notch narrower than its voxel:
//! the 12- and 24-tooth gears of `tests/it/gears.rs` became 5 and 2 pieces
//! with their tooth gaps filled, and jammed. This partition is exact: the
//! fill triangulation ([`tessellate`]) merged greedily into convex pieces
//! (Hertel-Mehlhorn: drop a shared edge when the union stays convex). The
//! pieces cover the polygon exactly, and there are at most four times as
//! many as the fewest possible.
//!
//! Pure math: no systems. The physics layer turns the pieces into a compound
//! collider.

use crate::contours::Contours;
use crate::tessellate::tessellate;
use bevy::math::Vec2;
use std::collections::HashMap;

/// Slack on the convexity test, as a cross product of unit edges: a vertex
/// this close to straight counts as convex.
const CONVEX_EPS: f32 = 1e-6;

/// Whether the closed ring `poly` (counter-clockwise) turns left or goes
/// straight at every vertex.
fn is_convex(poly: &[Vec2]) -> bool {
    let n = poly.len();
    (0..n).all(|i| {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let (e0, e1) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
        e0.perp_dot(e1) >= -CONVEX_EPS
    })
}

/// The union of pieces `a` and `b` (counter-clockwise vertex indices) that
/// share the edge `a[i] → a[i + 1]`, which `b` walks the other way.
fn merged(a: &[u32], i: usize, b: &[u32]) -> Option<Vec<u32>> {
    let (u, v) = (a[i], a[(i + 1) % a.len()]);
    let j = b.iter().position(|&x| x == v)?;
    if b[(j + 1) % b.len()] != u {
        return None;
    }
    // A from v round to u, then B strictly between u and v.
    let mut out: Vec<u32> = (0..a.len()).map(|k| a[(i + 1 + k) % a.len()]).collect();
    out.extend((1..b.len() - 1).map(|k| b[(j + 1 + k) % b.len()]));
    Some(out)
}

/// The convex pieces that exactly cover `contours` (holes included), each
/// counter-clockwise. Empty when the contours do not triangulate.
#[must_use]
pub fn convex_partition(contours: &Contours) -> Vec<Vec<Vec2>> {
    let tri = tessellate(contours);
    let verts = &tri.vertices;
    let mut pieces: Vec<Option<Vec<u32>>> = tri
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|&[a, b, c]| {
            let (pa, pb, pc) = (verts[a as usize], verts[b as usize], verts[c as usize]);
            let area = (pb - pa).perp_dot(pc - pa);
            if area > 0.0 {
                Some(Some(vec![a, b, c]))
            } else if area < 0.0 {
                Some(Some(vec![a, c, b]))
            } else {
                None
            }
        })
        .collect();
    // Directed edge → the piece that walks it.
    let mut owner: HashMap<(u32, u32), usize> = HashMap::new();
    for (p, piece) in pieces.iter().enumerate() {
        if let Some(piece) = piece {
            for k in 0..piece.len() {
                owner.insert((piece[k], piece[(k + 1) % piece.len()]), p);
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for p in 0..pieces.len() {
            let mut k = 0;
            while let Some(a) = pieces[p].as_ref()
                && k < a.len()
            {
                let (u, v) = (a[k], a[(k + 1) % a.len()]);
                let Some(&q) = owner.get(&(v, u)) else {
                    k += 1;
                    continue;
                };
                let (Some(b), true) = (pieces[q].as_ref(), q != p) else {
                    k += 1;
                    continue;
                };
                let Some(union) = merged(a, k, b) else {
                    k += 1;
                    continue;
                };
                let ring: Vec<Vec2> = union.iter().map(|&x| verts[x as usize]).collect();
                if !is_convex(&ring) {
                    k += 1;
                    continue;
                }
                for e in 0..union.len() {
                    owner.insert((union[e], union[(e + 1) % union.len()]), p);
                }
                owner.remove(&(u, v));
                owner.remove(&(v, u));
                pieces[q] = None;
                pieces[p] = Some(union);
                changed = true;
                k = 0;
            }
        }
    }
    pieces
        .into_iter()
        .flatten()
        .map(|piece| piece.iter().map(|&x| verts[x as usize]).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::gear_outline;
    use crate::hull::polygon_area;

    fn check(contours: &Contours, expected_area: f32) -> Vec<Vec<Vec2>> {
        let pieces = convex_partition(contours);
        let area: f32 = pieces.iter().map(|p| polygon_area(p).abs()).sum();
        assert!(
            (area - expected_area).abs() <= 1e-3 * expected_area,
            "area {area} vs {expected_area}"
        );
        for piece in &pieces {
            assert!(is_convex(piece), "a piece is convex: {piece:?}");
        }
        pieces
    }

    #[test]
    fn an_l_shape_is_two_convex_pieces() {
        let l = Contours {
            outline: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
            holes: vec![],
        };
        assert_eq!(check(&l, 3.0).len(), 2);
    }

    #[test]
    fn a_square_with_a_hole_is_covered_exactly() {
        let ring = Contours {
            outline: vec![
                Vec2::new(-2.0, -2.0),
                Vec2::new(2.0, -2.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(-2.0, 2.0),
            ],
            holes: vec![vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, -1.0),
            ]],
        };
        check(&ring, 12.0);
    }

    #[test]
    fn a_gear_keeps_its_tooth_gaps() {
        let outline = gear_outline(1.2, 24);
        let area = polygon_area(&outline).abs();
        let pieces = check(
            &Contours {
                outline,
                holes: vec![],
            },
            area,
        );
        assert!(
            pieces.len() >= 24,
            "at least a piece a tooth: {}",
            pieces.len()
        );
    }
}

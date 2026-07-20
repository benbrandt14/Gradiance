//! Extracting polygon contours from a signed distance field.
//!
//! Marching squares over the field's bounding box, with every emitted
//! vertex **bisection-refined onto the true zero set** — so straight
//! boundaries come out collinear (and simplify away) and curved
//! boundaries are accurate to far below the cell size. Segments are
//! directed with the solid on the left, which makes outer rings
//! counter-clockwise and holes clockwise — exactly the [`Contours`]
//! convention. Rings are grouped into connected components (one outline
//! plus its holes each): the unit the cut tool splits bodies by.

use crate::contours::{Contours, point_in_ring, ring_signed_area};
use bevy::math::Vec2;
use std::collections::BTreeMap;

/// Target number of cells across a shape's larger dimension.
pub const FIELD_CELLS: f32 = 96.0;

/// Hard cap on samples per axis (bounds worst-case cost).
const MAX_SAMPLES: usize = 512;

/// Bisection iterations when refining a crossing onto the zero set.
const REFINE_STEPS: u32 = 12;

/// A grid edge carrying at most one boundary crossing.
///
/// `(ix, iy, horizontal)`: horizontal edges run from grid point
/// `(ix, iy)` to `(ix + 1, iy)`, vertical ones to `(ix, iy + 1)`.
type EdgeKey = (i32, i32, bool);

/// Extracts the boundary contours of `field`'s solid region (`field < 0`)
/// inside the box `(min, max)`, as connected components.
///
/// `cell` is the sampling step; boundary detail below it may be missed
/// (the returned vertices themselves are refined far beyond it).
/// Components with area below ~4 cells are discarded as slivers.
#[expect(clippy::too_many_lines)] // one algorithm: sample, march, chain
#[expect(clippy::cast_possible_wrap)] // grid axes are capped at MAX_SAMPLES
pub fn contour_components(
    field: impl Fn(Vec2) -> f32,
    min: Vec2,
    max: Vec2,
    cell: f32,
) -> Vec<Contours> {
    let cell = cell.max(1e-3);
    // Pad so the boundary never touches the sampling border.
    let min = min - Vec2::splat(2.0 * cell);
    let max = max + Vec2::splat(2.0 * cell);
    let span = max - min;
    let nx = ((span.x / cell).ceil() as usize + 1).clamp(2, MAX_SAMPLES);
    let ny = ((span.y / cell).ceil() as usize + 1).clamp(2, MAX_SAMPLES);
    let step = Vec2::new(span.x / nx as f32, span.y / ny as f32);

    let point_at = |ix: i32, iy: i32| min + step * Vec2::new(ix as f32, iy as f32);
    let mut values = vec![0.0f32; (nx + 1) * (ny + 1)];
    for iy in 0..=ny {
        for ix in 0..=nx {
            values[iy * (nx + 1) + ix] = field(point_at(ix as i32, iy as i32));
        }
    }
    let value = |ix: usize, iy: usize| values[iy * (nx + 1) + ix];

    // Refined crossing point on a grid edge whose endpoint signs differ.
    let crossing = |a: Vec2, va: f32, b: Vec2, vb: f32| -> Vec2 {
        let (mut lo, mut hi, mut vlo) = (a, b, va);
        debug_assert!(va < 0.0 || vb < 0.0);
        let _ = vb;
        for _ in 0..REFINE_STEPS {
            let mid = (lo + hi) / 2.0;
            let vm = field(mid);
            if (vm < 0.0) == (vlo < 0.0) {
                lo = mid;
                vlo = vm;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    };

    // Directed segments (solid on the left), keyed by their source edge.
    let mut next: BTreeMap<EdgeKey, EdgeKey> = BTreeMap::new();
    let mut points: BTreeMap<EdgeKey, Vec2> = BTreeMap::new();

    for iy in 0..ny {
        for ix in 0..nx {
            let v_bl = value(ix, iy);
            let v_br = value(ix + 1, iy);
            let v_tr = value(ix + 1, iy + 1);
            let v_tl = value(ix, iy + 1);
            let mut case = 0u8;
            if v_bl < 0.0 {
                case |= 1;
            }
            if v_br < 0.0 {
                case |= 2;
            }
            if v_tr < 0.0 {
                case |= 4;
            }
            if v_tl < 0.0 {
                case |= 8;
            }
            if case == 0 || case == 15 {
                continue;
            }

            let (ix32, iy32) = (ix as i32, iy as i32);
            let bottom: EdgeKey = (ix32, iy32, true);
            let top: EdgeKey = (ix32, iy32 + 1, true);
            let left: EdgeKey = (ix32, iy32, false);
            let right: EdgeKey = (ix32 + 1, iy32, false);

            let ensure_point = |edge: EdgeKey, points: &mut BTreeMap<EdgeKey, Vec2>| {
                if points.contains_key(&edge) {
                    return;
                }
                let (ex, ey, horizontal) = edge;
                let a = point_at(ex, ey);
                let (b, va, vb) = if horizontal {
                    (
                        point_at(ex + 1, ey),
                        value(ex as usize, ey as usize),
                        value(ex as usize + 1, ey as usize),
                    )
                } else {
                    (
                        point_at(ex, ey + 1),
                        value(ex as usize, ey as usize),
                        value(ex as usize, ey as usize + 1),
                    )
                };
                points.insert(edge, crossing(a, va, b, vb));
            };

            // Directed with the solid region on the left of travel.
            let segments: &[(EdgeKey, EdgeKey)] = match case {
                1 => &[(bottom, left)],
                2 => &[(right, bottom)],
                3 => &[(right, left)],
                4 => &[(top, right)],
                6 => &[(top, bottom)],
                7 => &[(top, left)],
                8 => &[(left, top)],
                9 => &[(bottom, top)],
                11 => &[(right, top)],
                12 => &[(left, right)],
                13 => &[(bottom, right)],
                14 => &[(left, bottom)],
                // Saddles: the center sample decides connectivity.
                5 => {
                    if field(point_at(ix32, iy32) + step / 2.0) < 0.0 {
                        &[(top, left), (bottom, right)]
                    } else {
                        &[(bottom, left), (top, right)]
                    }
                }
                10 => {
                    if field(point_at(ix32, iy32) + step / 2.0) < 0.0 {
                        &[(left, bottom), (right, top)]
                    } else {
                        &[(right, bottom), (left, top)]
                    }
                }
                _ => &[],
            };
            for &(from, to) in segments {
                ensure_point(from, &mut points);
                ensure_point(to, &mut points);
                next.insert(from, to);
            }
        }
    }

    // Chain directed segments into closed rings.
    let mut rings: Vec<Vec<Vec2>> = Vec::new();
    while let Some(&start) = next.keys().next() {
        let mut ring = Vec::new();
        let mut key = start;
        let budget = points.len() + 1;
        for _ in 0..budget {
            if let Some(&p) = points.get(&key) {
                ring.push(p);
            }
            let Some(to) = next.remove(&key) else {
                ring.clear(); // Broken chain: discard, never loop forever.
                break;
            };
            key = to;
            if key == start {
                break;
            }
        }
        if ring.len() >= 3 {
            simplify_ring(&mut ring, cell * 0.05);
        }
        if ring.len() >= 3 && ring_signed_area(&ring).abs() > 4.0 * cell * cell {
            rings.push(ring);
        }
    }

    assemble_components(rings)
}

/// Removes vertices that deviate less than `tol` from the line through
/// their neighbors (refined vertices on straight boundaries collapse to
/// the segment endpoints).
fn simplify_ring(ring: &mut Vec<Vec2>, tol: f32) {
    let mut out: Vec<Vec2> = Vec::with_capacity(ring.len());
    let n = ring.len();
    for i in 0..n {
        let prev = *out.last().unwrap_or(&ring[(i + n - 1) % n]);
        let cur = ring[i];
        let nxt = ring[(i + 1) % n];
        let chord = nxt - prev;
        let deviation = if chord.length_squared() > 0.0 {
            (chord.perp_dot(cur - prev)).abs() / chord.length()
        } else {
            f32::MAX
        };
        if deviation > tol {
            out.push(cur);
        }
    }
    if out.len() >= 3 {
        *ring = out;
    }
}

/// Groups rings into components: each counter-clockwise ring is an
/// outline; each clockwise ring becomes a hole of the smallest outline
/// containing it (unmatched holes are dropped).
fn assemble_components(rings: Vec<Vec<Vec2>>) -> Vec<Contours> {
    let (outlines, holes): (Vec<_>, Vec<_>) =
        rings.into_iter().partition(|r| ring_signed_area(r) > 0.0);
    let mut components: Vec<Contours> = outlines
        .into_iter()
        .map(|outline| Contours {
            outline,
            holes: vec![],
        })
        .collect();
    // Largest-first so "smallest containing outline" is the last match.
    components.sort_by(|a, b| {
        ring_signed_area(&b.outline)
            .partial_cmp(&ring_signed_area(&a.outline))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for hole in holes {
        let probe = hole[0];
        if let Some(owner) = components
            .iter_mut()
            .rev()
            .find(|c| point_in_ring(probe, &c.outline))
        {
            owner.holes.push(hole);
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_field_contours_to_one_accurate_ring() {
        let r = 50.0;
        let components = contour_components(
            |p| p.length() - r,
            Vec2::splat(-r),
            Vec2::splat(r),
            2.0 * r / FIELD_CELLS,
        );
        assert_eq!(components.len(), 1);
        let c = &components[0];
        assert!(c.holes.is_empty());
        let ideal = std::f32::consts::PI * r * r;
        assert!(
            (c.area() - ideal).abs() / ideal < 0.005,
            "area {} vs {ideal}",
            c.area()
        );
        assert!(ring_signed_area(&c.outline) > 0.0, "outline is CCW");
        // Refined vertices sit on the true circle.
        for v in &c.outline {
            assert!((v.length() - r).abs() < 0.05, "vertex {v} off boundary");
        }
    }

    #[test]
    fn annulus_yields_outline_with_hole() {
        let components = contour_components(
            |p| (p.length() - 30.0).max(-(p.length() - 15.0)),
            Vec2::splat(-30.0),
            Vec2::splat(30.0),
            0.7,
        );
        assert_eq!(components.len(), 1);
        let c = &components[0];
        assert_eq!(c.holes.len(), 1);
        assert!(ring_signed_area(&c.holes[0]) < 0.0, "hole is CW");
        let ideal = std::f32::consts::PI * (30.0f32.powi(2) - 15.0f32.powi(2));
        assert!((c.area() - ideal).abs() / ideal < 0.01);
    }

    #[test]
    fn severed_box_yields_two_components_conserving_area() {
        // A 100×20 bar minus a full-height 4-wide strip at x = 10.
        let bar = |p: Vec2| {
            let q = p.abs() - Vec2::new(50.0, 10.0);
            q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
        };
        let strip = |p: Vec2| {
            let q = (p - Vec2::new(10.0, 0.0)).abs() - Vec2::new(2.0, 40.0);
            q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
        };
        let components = contour_components(
            |p| bar(p).max(-strip(p)),
            Vec2::new(-50.0, -10.0),
            Vec2::new(50.0, 10.0),
            1.0,
        );
        assert_eq!(components.len(), 2);
        let total: f32 = components.iter().map(Contours::area).sum();
        let ideal = 100.0 * 20.0 - 4.0 * 20.0;
        assert!(
            (total - ideal).abs() / ideal < 0.01,
            "area {total} vs {ideal}"
        );
    }

    #[test]
    fn straight_edges_simplify_to_few_vertices() {
        let components = contour_components(
            |p| {
                let q = p.abs() - Vec2::new(40.0, 25.0);
                q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
            },
            Vec2::new(-40.0, -25.0),
            Vec2::new(40.0, 25.0),
            1.0,
        );
        assert_eq!(components.len(), 1);
        assert!(
            components[0].outline.len() <= 12,
            "axis-aligned box should collapse to near-4 vertices, got {}",
            components[0].outline.len()
        );
    }
}

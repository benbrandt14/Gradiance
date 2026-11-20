//! The submerged part of a body: the area of its outline inside a fluid
//! region and that area's centroid (brief N-7, buoyancy).
//!
//! A fluid region is a [`FluidVolume`]: convex pieces, each an intersection
//! of half-planes ([`Plane`]), in the fluid's own frame. A convex outline is
//! one piece; any other outline (concave, with holes, several components) is
//! split into the triangles of its fill tessellation, which are convex and
//! do not overlap. A half-plane fluid (an open sea under a surface) is one
//! piece with one plane and no bounds.
//!
//! The body is a [`SolidOutline`]: its rings, outlines counter-clockwise and
//! holes clockwise, so the signed area of the rings is the body's area.
//!
//! The submerged moment is the sum, over the pieces and the rings, of the
//! signed moment of the ring clipped to the piece (Sutherland–Hodgman). The
//! clip is exact for any ring against a convex piece: clipping to one plane
//! replaces each part of the ring outside the plane by a segment on the
//! plane, which keeps the winding number of every point inside the plane and
//! makes it 0 outside. So the signed area of the clipped ring is the area of
//! the ring's inside within the piece, also for a concave ring, and holes
//! subtract because they wind the other way.
//!
//! Both the volume and the outline are built once, from the contours
//! [`crate::polygonize`] gives, when a shape changes; the per-step clip only
//! reads them and writes into a reused [`ClipScratch`], so it never contours
//! and, once the scratch has grown, never allocates.

use crate::contours::{Contours, decimate_ring, ring_signed_area};
use crate::tessellate::tessellate;
use bevy::math::Vec2;

/// A half-plane `{p : normal · p ≤ offset}`: the inside of one edge of a
/// convex piece. `normal` is a unit vector pointing out of the piece.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// Unit outward normal.
    pub normal: Vec2,
    /// `normal · p` on the boundary.
    pub offset: f32,
}

impl Plane {
    /// The signed distance of `p` outside the plane (negative inside).
    #[must_use]
    pub fn distance(&self, p: Vec2) -> f32 {
        self.normal.dot(p) - self.offset
    }
}

/// The zeroth and first area moments of a region: its area and `∫ p dA`, so
/// the centroid is `first / area`. Moments add over disjoint regions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AreaMoment {
    /// Area (m²); signed while summing rings, non-negative for a region.
    pub area: f32,
    /// `∫ p dA` (m³).
    pub first: Vec2,
}

impl AreaMoment {
    /// The centroid, or `None` for a region with (nearly) no area.
    #[must_use]
    pub fn centroid(&self) -> Option<Vec2> {
        (self.area > f32::EPSILON).then(|| self.first / self.area)
    }
}

impl core::ops::AddAssign for AreaMoment {
    fn add_assign(&mut self, rhs: Self) {
        self.area += rhs.area;
        self.first += rhs.first;
    }
}

/// The signed moment of one closed ring (positive counter-clockwise).
#[must_use]
pub fn ring_moment(ring: &[Vec2]) -> AreaMoment {
    let n = ring.len();
    if n < 3 {
        return AreaMoment::default();
    }
    let mut twice_area = 0.0;
    let mut six_first = Vec2::ZERO;
    for i in 0..n {
        let p = ring[i];
        let q = ring[(i + 1) % n];
        let cross = p.perp_dot(q);
        twice_area += cross;
        six_first += (p + q) * cross;
    }
    AreaMoment {
        area: twice_area / 2.0,
        first: six_first / 6.0,
    }
}

/// Reused buffers for [`clip_ring`]: they grow to the largest ring once and
/// are then cleared, never reallocated.
#[derive(Debug, Default, Clone)]
pub struct ClipScratch {
    a: Vec<Vec2>,
    b: Vec<Vec2>,
}

/// The signed moment of `ring` clipped to the convex piece `planes`
/// (Sutherland–Hodgman, one plane at a time). Exact for any ring; see the
/// module docs. Allocation-free once `scratch` has grown.
pub fn clip_ring(ring: &[Vec2], planes: &[Plane], scratch: &mut ClipScratch) -> AreaMoment {
    if ring.len() < 3 {
        return AreaMoment::default();
    }
    let ClipScratch { a, b } = scratch;
    a.clear();
    a.extend_from_slice(ring);
    for plane in planes {
        b.clear();
        let n = a.len();
        for i in 0..n {
            let prev = a[(i + n - 1) % n];
            let cur = a[i];
            let (dp, dc) = (plane.distance(prev), plane.distance(cur));
            let (prev_in, cur_in) = (dp <= 0.0, dc <= 0.0);
            if cur_in != prev_in {
                // The crossing: dp and dc have opposite signs, so dp - dc ≠ 0.
                let t = dp / (dp - dc);
                b.push(prev + (cur - prev) * t);
            }
            if cur_in {
                b.push(cur);
            }
        }
        core::mem::swap(a, b);
        if a.len() < 3 {
            return AreaMoment::default();
        }
    }
    ring_moment(a)
}

/// A fluid region in its own frame: convex pieces that do not overlap.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FluidVolume {
    planes: Vec<Plane>,
    /// Each piece is `planes[start..end]`.
    pieces: Vec<(u32, u32)>,
    /// The region's bounding box, `None` when it is unbounded (a sea).
    bounds: Option<(Vec2, Vec2)>,
}

impl FluidVolume {
    /// The open sea `y ≤ 0` — a half-plane shape's solid side (its surface
    /// is the local x axis, local +y points out of it).
    #[must_use]
    pub fn half_plane() -> Self {
        Self {
            planes: vec![Plane {
                normal: Vec2::Y,
                offset: 0.0,
            }],
            pieces: vec![(0, 1)],
            bounds: None,
        }
    }

    /// The region one shape's polygonized components cover: a convex
    /// component without holes is one piece, any other is split into its
    /// fill triangles.
    #[must_use]
    pub fn from_components(components: &[Contours]) -> Self {
        let mut volume = Self::default();
        let (mut min, mut max) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for contours in components {
            for v in &contours.outline {
                min = min.min(*v);
                max = max.max(*v);
            }
            if contours.holes.is_empty() && is_convex(&contours.outline) {
                volume.push_convex(&contours.outline);
            } else {
                let triangles = tessellate(contours);
                for &[i, j, k] in triangles.indices.as_chunks::<3>().0 {
                    let corner = |n: u32| triangles.vertices.get(n as usize).copied();
                    if let (Some(p), Some(q), Some(r)) = (corner(i), corner(j), corner(k)) {
                        volume.push_convex(&[p, q, r]);
                    }
                }
            }
        }
        if !volume.pieces.is_empty() {
            volume.bounds = Some((min, max));
        }
        volume
    }

    /// Appends a convex ring (either winding) as one piece; a degenerate
    /// ring adds nothing.
    fn push_convex(&mut self, ring: &[Vec2]) {
        let area = ring_signed_area(ring);
        if area.abs() <= f32::EPSILON {
            return;
        }
        let start = self.planes.len();
        let n = ring.len();
        for i in 0..n {
            let (p, q) = if area > 0.0 {
                (ring[i], ring[(i + 1) % n])
            } else {
                (ring[(i + 1) % n], ring[i])
            };
            // Counter-clockwise: the inside is on the left, so the outward
            // normal is the edge turned clockwise.
            let edge = q - p;
            let normal = Vec2::new(edge.y, -edge.x).normalize_or_zero();
            if normal != Vec2::ZERO {
                self.planes.push(Plane {
                    normal,
                    offset: normal.dot(p),
                });
            }
        }
        if self.planes.len() - start >= 3 {
            self.pieces.push((start as u32, self.planes.len() as u32));
        } else {
            self.planes.truncate(start);
        }
    }

    /// The bounding box in the fluid's frame; `None` for an unbounded sea.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        self.bounds
    }

    /// Whether the region holds no fluid at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// How many convex pieces the region has (the per-body clip cost is
    /// pieces × ring vertices).
    #[must_use]
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    /// Whether the circle at `center` with `radius` (in the fluid's frame)
    /// may touch the region: a cheap reject before any clip.
    #[must_use]
    pub fn may_touch(&self, center: Vec2, radius: f32) -> bool {
        match self.bounds {
            Some((min, max)) => {
                center.x + radius >= min.x
                    && center.x - radius <= max.x
                    && center.y + radius >= min.y
                    && center.y - radius <= max.y
            }
            // A sea: only its planes bound it.
            None => self.pieces.iter().all(|&(s, e)| {
                self.planes[s as usize..e as usize]
                    .iter()
                    .all(|p| p.distance(center) <= radius)
            }),
        }
    }

    /// The moment of the part of `rings` (in the fluid's frame; outlines
    /// counter-clockwise, holes clockwise) inside the region.
    pub fn submerged<'a>(
        &self,
        rings: impl Iterator<Item = &'a [Vec2]> + Clone,
        scratch: &mut ClipScratch,
    ) -> AreaMoment {
        let mut total = AreaMoment::default();
        for &(s, e) in &self.pieces {
            let planes = &self.planes[s as usize..e as usize];
            for ring in rings.clone() {
                total += clip_ring(ring, planes, scratch);
            }
        }
        total
    }
}

/// Whether a ring is convex (every turn the same way; straight runs allowed).
#[must_use]
pub fn is_convex(ring: &[Vec2]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let (mut pos, mut neg) = (false, false);
    for i in 0..n {
        let (a, b, c) = (ring[i], ring[(i + 1) % n], ring[(i + 2) % n]);
        let turn = (b - a).perp_dot(c - b);
        let scale = (b - a).length() * (c - b).length();
        if turn > 1e-6 * scale {
            pos = true;
        } else if turn < -1e-6 * scale {
            neg = true;
        }
        if pos && neg {
            return false;
        }
    }
    true
}

/// A body's outline for the submerged clip, in its own frame: outlines
/// counter-clockwise, holes clockwise (so their signed areas add up to the
/// body's area), and the radius of a circle about the origin that holds it
/// (the cheap reject).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SolidOutline {
    rings: Vec<Vec<Vec2>>,
    radius: f32,
}

/// Decimation tolerance as a fraction of the outline's extent (1 mm on a
/// 1 m body, as the collider uses): a contoured curve keeps one vertex per
/// field cell, many more than the clip needs.
pub const OUTLINE_DECIMATE_FRACTION: f32 = 1e-3;

impl SolidOutline {
    /// The outline of a shape's polygonized components (every component,
    /// as the collider covers every component).
    #[must_use]
    pub fn from_components(components: &[Contours]) -> Self {
        let (mut min, mut max) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for v in components.iter().flat_map(|c| c.outline.iter()) {
            min = min.min(*v);
            max = max.max(*v);
        }
        let tol = (max - min).max_element().max(0.0) * OUTLINE_DECIMATE_FRACTION;
        let mut rings = Vec::new();
        for contours in components {
            for (index, ring) in contours.rings().enumerate() {
                let mut ring = decimate_ring(ring, tol);
                if ring.len() < 3 {
                    continue;
                }
                // Outline counter-clockwise, holes clockwise.
                let ccw = ring_signed_area(&ring) > 0.0;
                if ccw != (index == 0) {
                    ring.reverse();
                }
                rings.push(ring);
            }
        }
        let radius = rings
            .iter()
            .flatten()
            .map(|v| v.length())
            .fold(0.0_f32, f32::max);
        Self { rings, radius }
    }

    /// The rings.
    pub fn rings(&self) -> impl Iterator<Item = &[Vec2]> + Clone {
        self.rings.iter().map(Vec::as_slice)
    }

    /// The radius of the origin-centred circle that holds every ring.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// The outline's own area moment (its full area and centroid).
    #[must_use]
    pub fn moment(&self) -> AreaMoment {
        let mut total = AreaMoment::default();
        for ring in &self.rings {
            total += ring_moment(ring);
        }
        total
    }

    /// Whether there is nothing to submerge.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rings.is_empty()
    }
}

/// Writes `ring` moved by the rigid transform `p ↦ rotation·p + offset`
/// into `out` (cleared first): the body's rings in the fluid's frame.
/// `rotation` is a unit complex number (`Vec2::from_angle`).
pub fn transform_ring_into(ring: &[Vec2], rotation: Vec2, offset: Vec2, out: &mut Vec<Vec2>) {
    out.clear();
    out.extend(ring.iter().map(|p| rotation.rotate(*p) + offset));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f32, h: f32, center: Vec2) -> Vec<Vec2> {
        let (x, y) = (w / 2.0, h / 2.0);
        vec![
            center + Vec2::new(-x, -y),
            center + Vec2::new(x, -y),
            center + Vec2::new(x, y),
            center + Vec2::new(-x, y),
        ]
    }

    fn contours(outline: Vec<Vec2>) -> Contours {
        Contours {
            outline,
            holes: vec![],
        }
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_ring_moment_is_its_area_and_centroid() {
        let m = ring_moment(&rect(2.0, 1.0, Vec2::new(3.0, -1.0)));
        assert!(close(m.area, 2.0, 1e-5));
        let c = m.centroid().expect("area");
        assert!((c - Vec2::new(3.0, -1.0)).length() < 1e-5, "{c}");
        // Clockwise reads negative.
        let mut cw = rect(2.0, 1.0, Vec2::ZERO);
        cw.reverse();
        assert!(close(ring_moment(&cw).area, -2.0, 1e-5));
    }

    #[test]
    fn a_box_half_under_a_sea_has_half_its_area_below_the_surface() {
        let sea = FluidVolume::half_plane();
        let body = rect(1.0, 1.0, Vec2::ZERO);
        let m = sea.submerged(
            std::iter::once(body.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 0.5, 1e-5), "{m:?}");
        let c = m.centroid().expect("area");
        assert!((c - Vec2::new(0.0, -0.25)).length() < 1e-5, "{c}");
    }

    #[test]
    fn a_tilted_box_in_a_tank_matches_the_analytic_wedge() {
        // A 2 x 2 tank's top at y = 0; a unit square turned 45 degrees with
        // its centre on the surface: half under, a triangle of area 0.5.
        let tank = FluidVolume::from_components(&[contours(rect(2.0, 2.0, Vec2::new(0.0, -1.0)))]);
        assert_eq!(tank.piece_count(), 1, "a box is one convex piece");
        let rot = Vec2::from_angle(core::f32::consts::FRAC_PI_4);
        let mut body = Vec::new();
        transform_ring_into(&rect(1.0, 1.0, Vec2::ZERO), rot, Vec2::ZERO, &mut body);
        let m = tank.submerged(
            std::iter::once(body.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 0.5, 1e-5), "{m:?}");
        // The lower half of a diamond of half-diagonal s = 1/√2: a triangle
        // with its centroid a third of the way down, -s/3.
        let s = core::f32::consts::FRAC_1_SQRT_2;
        let c = m.centroid().expect("area");
        assert!((c - Vec2::new(0.0, -s / 3.0)).length() < 1e-5, "{c}");
    }

    #[test]
    fn a_body_wholly_inside_is_wholly_submerged_and_one_outside_not_at_all() {
        let tank = FluidVolume::from_components(&[contours(rect(4.0, 4.0, Vec2::ZERO))]);
        let inside = rect(1.0, 0.5, Vec2::new(0.5, 0.5));
        let m = tank.submerged(
            std::iter::once(inside.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 0.5, 1e-5));
        let outside = rect(1.0, 1.0, Vec2::new(10.0, 0.0));
        let m = tank.submerged(
            std::iter::once(outside.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 0.0, 1e-6));
        assert!(m.centroid().is_none());
        assert!(!tank.may_touch(Vec2::new(10.0, 0.0), 0.8));
        assert!(tank.may_touch(Vec2::new(2.5, 0.0), 0.8));
    }

    #[test]
    fn a_concave_body_against_a_concave_tank_is_exact() {
        // An L-shaped tank (3 x 3 minus its top-right 2 x 2 corner: area 5)
        // splits into triangles; a 3 x 3 body over the whole square covers
        // exactly the L.
        let l = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(3.0, 0.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 3.0),
            Vec2::new(0.0, 3.0),
        ];
        let tank = FluidVolume::from_components(&[contours(l.clone())]);
        assert!(tank.piece_count() >= 2);
        let body = rect(3.0, 3.0, Vec2::splat(1.5));
        let m = tank.submerged(
            std::iter::once(body.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 5.0, 1e-4), "{m:?}");
        let expected = ring_moment(&l);
        let c = m.centroid().expect("area");
        assert!(
            (c - expected.centroid().expect("area")).length() < 1e-4,
            "{c}"
        );
        // And the L as the body, in a sea at y <= 0.5: the strip 3 x 0.5.
        let outline = SolidOutline::from_components(&[contours(l)]);
        let sea = FluidVolume::half_plane();
        let mut moved = Vec::new();
        transform_ring_into(
            outline.rings().next().expect("a ring"),
            Vec2::X,
            Vec2::new(0.0, -0.5),
            &mut moved,
        );
        let m = sea.submerged(
            std::iter::once(moved.as_slice()),
            &mut ClipScratch::default(),
        );
        assert!(close(m.area, 1.5, 1e-5), "{m:?}");
    }

    #[test]
    fn a_hole_is_not_submerged() {
        let ring = Contours {
            outline: rect(2.0, 2.0, Vec2::ZERO),
            // Deliberately counter-clockwise: the outline normalizes it.
            holes: vec![rect(1.0, 1.0, Vec2::ZERO)],
        };
        let outline = SolidOutline::from_components(&[ring]);
        assert!(close(outline.moment().area, 3.0, 1e-5));
        let sea = FluidVolume::half_plane();
        let m = sea.submerged(outline.rings(), &mut ClipScratch::default());
        // Below y = 0: the 2 x 1 lower half minus the hole's 1 x 0.5 half.
        assert!(close(m.area, 1.5, 1e-5), "{m:?}");
        assert!(close(outline.radius(), 2.0_f32.sqrt(), 1e-5));
    }

    #[test]
    fn the_sea_reject_reads_the_surface() {
        let sea = FluidVolume::half_plane();
        assert!(sea.may_touch(Vec2::new(100.0, 0.5), 0.6));
        assert!(!sea.may_touch(Vec2::new(0.0, 2.0), 1.0));
        assert!(sea.bounds().is_none());
    }

    #[test]
    fn the_clip_reuses_its_scratch() {
        let tank = FluidVolume::from_components(&[contours(rect(4.0, 4.0, Vec2::ZERO))]);
        let body = rect(1.0, 1.0, Vec2::new(2.0, 2.0));
        let mut scratch = ClipScratch::default();
        let first = tank.submerged(std::iter::once(body.as_slice()), &mut scratch);
        let capacity = (scratch.a.capacity(), scratch.b.capacity());
        let again = tank.submerged(std::iter::once(body.as_slice()), &mut scratch);
        assert_eq!(first, again);
        assert_eq!(capacity, (scratch.a.capacity(), scratch.b.capacity()));
        assert!(close(first.area, 0.25, 1e-5));
    }
}

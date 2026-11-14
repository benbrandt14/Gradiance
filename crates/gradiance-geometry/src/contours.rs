//! Polygon-with-holes representation and pure polygon math.

use bevy::math::Vec2;

/// A simple polygon with optional holes.
///
/// Convention: `outline` counter-clockwise, `holes` clockwise, vertices in
/// body-local (centroid-relative) space.
#[derive(Debug, Clone, PartialEq)]
pub struct Contours {
    /// Outer boundary (counter-clockwise).
    pub outline: Vec<Vec2>,
    /// Interior holes (each clockwise).
    pub holes: Vec<Vec<Vec2>>,
}

impl Contours {
    /// All rings: the outline followed by every hole.
    pub fn rings(&self) -> impl Iterator<Item = &Vec<Vec2>> {
        std::iter::once(&self.outline).chain(self.holes.iter())
    }

    /// Net enclosed area (outline area minus hole areas), always ≥ 0 for
    /// correctly wound input.
    pub fn area(&self) -> f32 {
        self.rings().map(|r| ring_signed_area(r)).sum::<f32>().abs()
    }

    /// Hole-aware area centroid.
    ///
    /// Returns `Vec2::ZERO` for degenerate (zero-area) input.
    pub fn centroid(&self) -> Vec2 {
        let mut weighted = Vec2::ZERO;
        let mut total = 0.0;
        for ring in self.rings() {
            let a = ring_signed_area(ring);
            weighted += ring_centroid(ring) * a;
            total += a;
        }
        if total.abs() < f32::EPSILON {
            Vec2::ZERO
        } else {
            weighted / total
        }
    }

    /// Translates all vertices so the centroid lands at the origin,
    /// returning the offset that was subtracted (the old centroid).
    pub fn recenter(&mut self) -> Vec2 {
        let c = self.centroid();
        for ring in std::iter::once(&mut self.outline).chain(self.holes.iter_mut()) {
            for v in ring {
                *v -= c;
            }
        }
        c
    }
}

/// Even-odd point-in-polygon test against one ring.
pub fn point_in_ring(p: Vec2, ring: &[Vec2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Signed area of one ring (positive for counter-clockwise winding).
pub fn ring_signed_area(ring: &[Vec2]) -> f32 {
    if ring.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum / 2.0
}

/// Area centroid of one ring (winding-independent).
pub fn ring_centroid(ring: &[Vec2]) -> Vec2 {
    let a = ring_signed_area(ring);
    if a.abs() < f32::EPSILON {
        // Degenerate: fall back to the vertex average.
        let n = ring.len().max(1) as f32;
        return ring.iter().copied().sum::<Vec2>() / n;
    }
    let mut c = Vec2::ZERO;
    for (i, p) in ring.iter().enumerate() {
        let q = ring[(i + 1) % ring.len()];
        let cross = p.x * q.y - q.x * p.y;
        c += (*p + q) * cross;
    }
    c / (6.0 * a)
}

/// A closed ring decimated by Ramer-Douglas-Peucker: every dropped vertex
/// lies within `tol` of the kept outline. Returns the ring unchanged when
/// decimation would leave fewer than three vertices.
///
/// Contours keep every vertex of a curved boundary (one per field cell),
/// which is right for meshes but not for convex decomposition: parry
/// treats adjacent edges turning by less than about 0.026 rad as
/// collinear, so a fine near-circular ring (some 240+ vertices) collapses
/// to no convex part at all and the collider build panics. Colliders
/// decimate first (`physics::body_sync`).
pub fn decimate_ring(ring: &[Vec2], tol: f32) -> Vec<Vec2> {
    let n = ring.len();
    if n <= 3 {
        return ring.to_vec();
    }
    // Split the closed ring at vertex 0 and the vertex farthest from it.
    let far = (1..n)
        .max_by(|&a, &b| {
            ring[a]
                .distance_squared(ring[0])
                .total_cmp(&ring[b].distance_squared(ring[0]))
        })
        .unwrap_or(n / 2);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let mut stack = vec![(0, far), (far, n)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (ring[a], ring[b % n]);
        let chord = pb - pa;
        let len = chord.length();
        let mut worst = (0.0f32, a);
        for (i, p) in ring.iter().enumerate().take(b).skip(a + 1) {
            let d = if len > 0.0 {
                chord.perp_dot(*p - pa).abs() / len
            } else {
                p.distance(pa)
            };
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    let out: Vec<Vec2> = ring
        .iter()
        .zip(&keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect();
    if out.len() >= 3 { out } else { ring.to_vec() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(half: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ]
    }

    #[test]
    fn ring_area_and_winding() {
        assert!((ring_signed_area(&square(10.0)) - 400.0).abs() < 1e-3);
        let mut cw = square(10.0);
        cw.reverse();
        assert!((ring_signed_area(&cw) + 400.0).abs() < 1e-3);
    }

    #[test]
    fn area_subtracts_holes() {
        let mut hole = square(5.0);
        hole.reverse(); // clockwise
        let c = Contours {
            outline: square(10.0),
            holes: vec![hole],
        };
        assert!((c.area() - 300.0).abs() < 1e-3);
    }

    #[test]
    fn centroid_and_recenter() {
        let offset = Vec2::new(30.0, -12.0);
        let mut c = Contours {
            outline: square(10.0).into_iter().map(|v| v + offset).collect(),
            holes: vec![],
        };
        assert!((c.centroid() - offset).length() < 1e-3);
        let removed = c.recenter();
        assert!((removed - offset).length() < 1e-3);
        assert!(c.centroid().length() < 1e-3);
    }

    #[test]
    fn decimation_bounds_the_error_and_keeps_corners() {
        // A 400-gon circle of radius 1: decimated at 2 mm it keeps a few
        // dozen vertices, all on the circle, and no chord sags past 2 mm.
        let ring: Vec<Vec2> = (0..400)
            .map(|i| Vec2::from_angle(i as f32 * std::f32::consts::TAU / 400.0))
            .collect();
        let fine = decimate_ring(&ring, 0.002);
        assert!(
            fine.len() > 20 && fine.len() < 80,
            "{} vertices",
            fine.len()
        );
        let n = fine.len();
        for i in 0..n {
            let mid = (fine[i] + fine[(i + 1) % n]) / 2.0;
            assert!(
                1.0 - mid.length() < 0.0021,
                "chord sags {}",
                1.0 - mid.length()
            );
        }
        assert!((ring_signed_area(&fine) - ring_signed_area(&ring)).abs() < 0.02);

        // A square with points along its edges keeps exactly its corners.
        let mut square = Vec::new();
        for (a, b) in [
            (Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0)),
            (Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0)),
            (Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0)),
            (Vec2::new(-1.0, 1.0), Vec2::new(-1.0, -1.0)),
        ] {
            for k in 0..5 {
                square.push(a.lerp(b, k as f32 / 5.0));
            }
        }
        assert_eq!(decimate_ring(&square, 1e-4).len(), 4);
        // Too few vertices to decimate: returned as is.
        let tri = [Vec2::ZERO, Vec2::X, Vec2::Y];
        assert_eq!(decimate_ring(&tri, 10.0), tri.to_vec());
    }
}

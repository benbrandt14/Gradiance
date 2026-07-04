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
}

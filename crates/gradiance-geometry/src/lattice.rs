//! Lattice fills (brief S-11): the points of a square lattice inside a
//! polygon, the way a water emitter lays its particles.

use bevy::math::Vec2;

/// Whether `p` is inside the closed polygon `outline` (even-odd rule).
pub fn contains(outline: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let n = outline.len();
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The points of the square lattice of `spacing` whose cells fit the
/// polygon's box, offset half a spacing from its corner, that lie inside
/// `outline`. Each point stands for one `spacing²` cell, so the count times
/// `spacing²` is close to the polygon's area.
pub fn fill_polygon(outline: &[Vec2], spacing: f32) -> Vec<Vec2> {
    if outline.len() < 3 || !(spacing.is_finite() && spacing > 0.0) {
        return Vec::new();
    }
    let (lo, hi) = outline.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(*v), hi.max(*v)),
    );
    let mut points = Vec::new();
    let mut y = lo.y + 0.5 * spacing;
    while y < hi.y {
        let mut x = lo.x + 0.5 * spacing;
        while x < hi.x {
            let p = Vec2::new(x, y);
            if contains(outline, p) {
                points.push(p);
            }
            x += spacing;
        }
        y += spacing;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fill_keeps_the_area() {
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        assert_eq!(fill_polygon(&square, 0.04).len(), 625);
        let circle: Vec<Vec2> = (0..64)
            .map(|k| 0.5 * Vec2::from_angle(k as f32 * std::f32::consts::TAU / 64.0))
            .collect();
        let area = 0.04 * 0.04 * fill_polygon(&circle, 0.04).len() as f32;
        let exact = std::f32::consts::PI * 0.25;
        assert!((area / exact - 1.0).abs() < 0.03, "{area} vs {exact}");
    }
}

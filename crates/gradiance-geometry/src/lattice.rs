//! Lattice fills (brief S-11): the points of a square lattice inside a
//! polygon, the way a water emitter lays its particles, and the same points
//! joined as a sponge (brief S-12).

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
    cells(outline, spacing, lo, hi)
        .into_iter()
        .map(|(_, p)| p)
        .collect()
}

/// The lattice points inside `outline`, each with its `(column, row)`.
fn cells(outline: &[Vec2], spacing: f32, lo: Vec2, hi: Vec2) -> Vec<((i32, i32), Vec2)> {
    let mut points = Vec::new();
    let mut row = 0;
    let mut y = lo.y + 0.5 * spacing;
    while y < hi.y {
        let mut column = 0;
        let mut x = lo.x + 0.5 * spacing;
        while x < hi.x {
            let p = Vec2::new(x, y);
            if contains(outline, p) {
                points.push(((column, row), p));
            }
            x += spacing;
            column += 1;
        }
        y += spacing;
        row += 1;
    }
    points
}

/// The root of `i` in a union-find forest, halving the path.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// A sponge lattice (brief S-12): the [`fill_polygon`] points of `outline`
/// and the edges that join them, as index pairs. Structural edges join
/// side and upper neighbours; shear edges join the two diagonals of each
/// cell. Only the largest connected part is kept, so the graph is
/// connected (a thin neck of the outline can cut the lattice in two).
pub fn sponge(outline: &[Vec2], spacing: f32) -> (Vec<Vec2>, Vec<(usize, usize)>) {
    if outline.len() < 3 || !(spacing.is_finite() && spacing > 0.0) {
        return (Vec::new(), Vec::new());
    }
    let (lo, hi) = outline.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(*v), hi.max(*v)),
    );
    let all = cells(outline, spacing, lo, hi);
    let at: std::collections::HashMap<(i32, i32), usize> = all
        .iter()
        .enumerate()
        .map(|(i, (cell, _))| (*cell, i))
        .collect();
    let mut edges = Vec::new();
    for (i, ((c, r), _)) in all.iter().enumerate() {
        for step in [(1, 0), (0, 1), (1, 1), (1, -1)] {
            if let Some(&j) = at.get(&(c + step.0, r + step.1)) {
                edges.push((i, j));
            }
        }
    }
    // Union-find over the edges, then keep the largest part.
    let mut parent: Vec<usize> = (0..all.len()).collect();
    for &(a, b) in &edges {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let roots: Vec<usize> = (0..all.len()).map(|i| root(&mut parent, i)).collect();
    let mut sizes = std::collections::HashMap::<usize, usize>::new();
    for r in &roots {
        *sizes.entry(*r).or_default() += 1;
    }
    let Some(keep) = sizes
        .iter()
        .max_by_key(|(r, n)| (**n, std::cmp::Reverse(**r)))
        .map(|(r, _)| *r)
    else {
        return (Vec::new(), Vec::new());
    };
    let mut new_index = vec![usize::MAX; all.len()];
    let mut points = Vec::new();
    for (i, (_, p)) in all.iter().enumerate() {
        if roots[i] == keep {
            new_index[i] = points.len();
            points.push(*p);
        }
    }
    let edges = edges
        .into_iter()
        .filter(|(a, _)| roots[*a] == keep)
        .map(|(a, b)| (new_index[a], new_index[b]))
        .collect();
    (points, edges)
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

    #[test]
    fn a_sponge_lies_inside_and_is_connected() {
        // An L: two arms meeting at a corner.
        let l = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 0.3),
            Vec2::new(0.3, 0.3),
            Vec2::new(0.3, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let (points, edges) = sponge(&l, 0.1);
        assert!(points.len() > 30);
        assert!(points.iter().all(|p| contains(&l, *p)), "inside");
        // Connected: a walk from point 0 reaches every point.
        let mut seen = vec![false; points.len()];
        let mut stack = vec![0];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            for &(a, b) in &edges {
                if a == i && !seen[b] {
                    stack.push(b);
                }
                if b == i && !seen[a] {
                    stack.push(a);
                }
            }
        }
        assert!(seen.iter().all(|s| *s), "connected");
        // A full 3 × 3 block has 12 structural and 8 shear edges.
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.3, 0.0),
            Vec2::new(0.3, 0.3),
            Vec2::new(0.0, 0.3),
        ];
        let (points, edges) = sponge(&square, 0.1);
        assert_eq!((points.len(), edges.len()), (9, 20));
    }

    #[test]
    fn a_neck_keeps_the_larger_part() {
        // Two squares joined by a strip thinner than a spacing.
        let dumbbell = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.5, 0.0),
            Vec2::new(0.5, 0.17),
            Vec2::new(0.9, 0.17),
            Vec2::new(0.9, 0.1),
            Vec2::new(1.2, 0.1),
            Vec2::new(1.2, 0.4),
            Vec2::new(0.9, 0.4),
            Vec2::new(0.9, 0.23),
            Vec2::new(0.5, 0.23),
            Vec2::new(0.5, 0.5),
            Vec2::new(0.0, 0.5),
        ];
        let (points, _) = sponge(&dumbbell, 0.1);
        assert_eq!(points.len(), 25, "the 5 x 5 square, not the 3 x 3 one");
    }
}

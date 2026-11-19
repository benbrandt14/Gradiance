//! The **face field**: per-texel distance to the outline over a body's
//! front face, so the toon shader can light the face as a rounded body
//! (brief 4b-6).
//!
//! A front cap is flat and its vertices all lie on the outline, so no
//! vertex attribute can say how far a pixel is from the edge. This samples
//! that distance once, on the CPU, into a small grid over the face frame
//! ([`extrusion::face_frame`](crate::extrusion::face_frame): the outline's
//! bounding box, the same frame as the mesh's face coordinates), and the
//! render layer uploads it as a texture. The shader then tilts the face's
//! normal toward the nearest outline by a profile of the distance: a
//! circle becomes a dome, a box a slab with rounded edges, a cut shape is
//! rounded along its cut.
//!
//! The distance is to the **polygonized** contours — the outline the mesh
//! is built from — so the shading agrees with the drawn edge exactly, and
//! [`polygonize`](crate::polygonize::polygonize) stays the only place a
//! field is contoured (this reads its output; it contours nothing).

use crate::contours::{Contours, point_in_ring};
use crate::extrusion::face_frame;
use bevy::math::Vec2;

/// Texels along the longer side of the face frame.
pub const FIELD_TEXELS: u32 = 64;

/// Texels along the shorter side, at least.
const MIN_TEXELS: u32 = 4;

/// A sampled face field (module docs), row-major from the frame's
/// bottom-left: texel `(i, j)` covers face coordinates
/// `u = (i + 0.5) / width * 2 - 1`, `v = (j + 0.5) / height * 2 - 1`.
///
/// Each texel is `[r, g, b, a]` bytes (a linear `Rgba8Unorm` texture):
/// `r, g` = the unit direction toward the nearest outline (body-local,
/// mapped from `[-1, 1]` to `[0, 255]`), `b` = the depth inside the shape
/// as a fraction of the deepest depth (0 on and outside the outline, 255
/// at the deepest point), `a` = 255.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceField {
    /// Texels across.
    pub width: u32,
    /// Texels up.
    pub height: u32,
    /// The texels (module docs), `width * height` of them.
    pub texels: Vec<[u8; 4]>,
    /// The deepest depth inside the shape (body-local units): what `b = 255`
    /// means.
    pub depth: f32,
}

impl FaceField {
    /// The texels as one byte slice (an image upload).
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.texels.iter().flatten().copied().collect()
    }

    /// The decoded texel `(i, j)`: (direction toward the outline, depth
    /// fraction). For tests and diagnostics.
    #[must_use]
    pub fn at(&self, i: u32, j: u32) -> (Vec2, f32) {
        let t = self.texels[(j * self.width + i) as usize];
        let unit = |b: u8| f32::from(b) / 255.0;
        (
            Vec2::new(unit(t[0]) * 2.0 - 1.0, unit(t[1]) * 2.0 - 1.0),
            unit(t[2]),
        )
    }
}

/// Samples the face field of `contours` (module docs).
#[must_use]
pub fn face_field(contours: &Contours) -> FaceField {
    let (centre, half) = face_frame(contours);
    let long = half.x.max(half.y);
    let side = |h: f32| {
        // Proportional to the side, rounded; truncation is intended.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((h / long) * FIELD_TEXELS as f32).round() as u32;
        n.clamp(MIN_TEXELS, FIELD_TEXELS)
    };
    let (width, height) = (side(half.x), side(half.y));
    let mut samples = Vec::with_capacity((width * height) as usize);
    let mut deepest = 0.0_f32;
    for j in 0..height {
        for i in 0..width {
            let uv = Vec2::new(
                (i as f32 + 0.5) / width as f32 * 2.0 - 1.0,
                (j as f32 + 0.5) / height as f32 * 2.0 - 1.0,
            );
            let p = centre + uv * half;
            let (closest, dist) = nearest_on_rings(contours, p);
            let inside = contours.rings().filter(|r| point_in_ring(p, r)).count() % 2 == 1;
            let away = if dist > f32::EPSILON {
                (closest - p) / dist
            } else {
                Vec2::ZERO
            };
            // Toward the outline: from inside, toward the nearest point;
            // from outside, away from it (still outward), so a filtered
            // read across the edge never flips.
            let (dir, depth) = if inside { (away, dist) } else { (-away, 0.0) };
            deepest = deepest.max(depth);
            samples.push((dir, depth));
        }
    }
    let byte = |x: f32| {
        // In [0, 255] after the clamp; truncation is the rounding.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        b
    };
    let texels = samples
        .into_iter()
        .map(|(dir, depth)| {
            let frac = if deepest > f32::EPSILON {
                depth / deepest
            } else {
                0.0
            };
            [
                byte(dir.x * 0.5 + 0.5),
                byte(dir.y * 0.5 + 0.5),
                byte(frac),
                255,
            ]
        })
        .collect();
    FaceField {
        width,
        height,
        texels,
        depth: deepest,
    }
}

/// The nearest point to `p` on any ring edge, and its distance.
fn nearest_on_rings(contours: &Contours, p: Vec2) -> (Vec2, f32) {
    let mut best = (p, f32::INFINITY);
    for ring in contours.rings() {
        let n = ring.len();
        for k in 0..n {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let ab = b - a;
            let len2 = ab.length_squared();
            let t = if len2 > f32::EPSILON {
                ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a + ab * t;
            let d = q.distance(p);
            if d < best.1 {
                best = (q, d);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygonize::polygonize;
    use crate::shape::{CsgOp, ShapeDef};

    #[test]
    fn a_circle_is_deepest_at_its_centre_and_points_outward() {
        let field = face_field(&polygonize(&ShapeDef::Circle { radius: 0.5 }));
        assert_eq!((field.width, field.height), (FIELD_TEXELS, FIELD_TEXELS));
        assert!((field.depth - 0.5).abs() < 0.02, "depth {}", field.depth);
        let mid = FIELD_TEXELS / 2;
        let (_, centre) = field.at(mid, mid);
        assert!(centre > 0.95, "centre depth {centre}");
        // Right of centre, halfway out: pointing +x, about half deep.
        let (dir, depth) = field.at(mid + FIELD_TEXELS / 4, mid);
        assert!(dir.x > 0.95 && dir.y.abs() < 0.1, "{dir}");
        assert!((depth - 0.5).abs() < 0.06, "{depth}");
        // A corner of the frame is outside: depth 0, pointing out (+x, +y).
        let (dir, depth) = field.at(FIELD_TEXELS - 1, FIELD_TEXELS - 1);
        assert!(depth.abs() < f32::EPSILON);
        assert!(dir.x > 0.5 && dir.y > 0.5, "{dir}");
    }

    #[test]
    fn a_long_box_keeps_its_aspect_and_points_to_its_nearest_side() {
        let field = face_field(&polygonize(&ShapeDef::Box {
            width: 2.0,
            height: 0.5,
        }));
        assert_eq!(field.width, FIELD_TEXELS);
        assert_eq!(field.height, FIELD_TEXELS / 4);
        assert!((field.depth - 0.25).abs() < 0.02);
        // Near the top edge, mid-length: toward +y.
        let (dir, _) = field.at(FIELD_TEXELS / 2, field.height - 2);
        assert!(dir.y > 0.95, "{dir}");
        // Near the left end: toward -x.
        let (dir, _) = field.at(1, field.height / 2);
        assert!(dir.x < -0.95, "{dir}");
    }

    #[test]
    fn a_hole_is_outside_and_its_rim_points_into_it() {
        let ring = ShapeDef::Csg {
            op: CsgOp::Subtract,
            lhs: Box::new(ShapeDef::Circle { radius: 1.0 }),
            rhs: Box::new(ShapeDef::Circle { radius: 0.4 }),
        };
        let field = face_field(&polygonize(&ring));
        let mid = field.width / 2;
        let (_, depth) = field.at(mid, mid);
        assert!(depth.abs() < f32::EPSILON, "the hole is outside");
        // Inside the material, just right of the hole: toward the hole (-x).
        let at = mid + mid * 45 / 100; // x about 0.45: in the ring, near the hole
        let (dir, depth) = field.at(at, mid);
        assert!(depth > 0.0);
        assert!(dir.x < -0.9, "{dir}");
        assert_eq!(
            field.bytes().len(),
            (field.width * field.height * 4) as usize
        );
    }
}

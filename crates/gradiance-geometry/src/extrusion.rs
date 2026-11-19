//! 2.5D extrusion: contours + an authored depth span → prism mesh buffers.
//!
//! Callers pass the continuous `(z_front, depth)` straight from the body's
//! `DepthBand` (`z_front = -near`, `depth = far - near`); the legacy
//! layer-bit mapping retired with save v5.

use crate::contours::Contours;
use crate::tessellate::tessellate;
use bevy::math::{Vec2, Vec3};

/// Raw mesh buffers, engine-agnostic.
#[derive(Debug, Clone, Default)]
pub struct MeshBuffers {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals.
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices (CCW = outward).
    pub indices: Vec<u32>,
    /// Per-vertex face coordinates ([`face_coords`]): the vertex's outline
    /// point mapped so the outline's bounding box spans `[-1, 1]` on both
    /// axes. Affine in position, so it interpolates exactly across any
    /// triangulation; the toon shader reads a front cap as a shallow dome
    /// over it (brief 4b-5).
    pub uvs: Vec<[f32; 2]>,
}

/// The bounding box of `contours` as `(centre, half extents)`, the frame
/// [`MeshBuffers::uvs`] is measured in. Half extents are never zero, so a
/// degenerate outline maps to finite coordinates.
#[must_use]
pub fn face_frame(contours: &Contours) -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for p in contours.rings().flatten() {
        min = min.min(*p);
        max = max.max(*p);
    }
    if min.x > max.x {
        return (Vec2::ZERO, Vec2::ONE);
    }
    (
        (min + max) * 0.5,
        ((max - min) * 0.5).max(Vec2::splat(1e-6)),
    )
}

/// `p` in the face frame `(centre, half)`: the outline's bounding box maps
/// to `[-1, 1]` on both axes (a circle to the unit disc).
#[must_use]
pub fn face_coords(p: Vec2, (centre, half): (Vec2, Vec2)) -> [f32; 2] {
    ((p - centre) / half).to_array()
}

/// Extrudes `contours` into a closed prism spanning
/// `z ∈ [z_front - depth, z_front]`.
///
/// Front and back caps come from fill tessellation; side walls are flat
/// quads with outward normals (correct for CCW outlines and CW holes).
pub fn extrude_contours(contours: &Contours, z_front: f32, depth: f32) -> MeshBuffers {
    let z_back = z_front - depth;
    let cap = tessellate(contours);
    let frame = face_frame(contours);
    let mut mesh = MeshBuffers::default();

    // Front cap (+Z normal), original winding.
    let front_base = 0u32;
    for v in &cap.vertices {
        mesh.positions.push([v.x, v.y, z_front]);
        mesh.normals.push([0.0, 0.0, 1.0]);
        mesh.uvs.push(face_coords(*v, frame));
    }
    mesh.indices.extend(cap.indices.iter().copied());

    // Back cap (−Z normal), reversed winding.
    let back_base = mesh.positions.len() as u32;
    for v in &cap.vertices {
        mesh.positions.push([v.x, v.y, z_back]);
        mesh.normals.push([0.0, 0.0, -1.0]);
        mesh.uvs.push(face_coords(*v, frame));
    }
    for &[a, b, c] in cap.indices.as_chunks::<3>().0 {
        mesh.indices
            .extend([back_base + a, back_base + c, back_base + b]);
    }
    let _ = front_base;

    // Side walls: one flat quad per ring edge.
    for ring in contours.rings() {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let edge = b - a;
            if edge.length_squared() < f32::EPSILON {
                continue;
            }
            // Outward for CCW outlines; also outward (into the cavity)
            // for CW holes.
            let normal = Vec2::new(edge.y, -edge.x).normalize();
            let nrm = [normal.x, normal.y, 0.0];
            let base = mesh.positions.len() as u32;
            mesh.positions.extend([
                [a.x, a.y, z_front],
                [b.x, b.y, z_front],
                [b.x, b.y, z_back],
                [a.x, a.y, z_back],
            ]);
            mesh.normals.extend([nrm; 4]);
            let (ua, ub) = (face_coords(a, frame), face_coords(b, frame));
            mesh.uvs.extend([ua, ub, ub, ua]);
            // Two CCW triangles facing `normal`.
            mesh.indices
                .extend([base, base + 3, base + 1, base + 1, base + 3, base + 2]);
        }
    }

    orient_to_normals(&mut mesh);
    mesh
}

/// Turns every triangle so its winding (CCW = front) agrees with its
/// vertex normal.
///
/// The fill tessellator's winding is not tied to the outline's, and a cap
/// wound away from its normal is drawn back-facing: on a double-sided
/// material the renderer then flips the normal, so the camera-facing front
/// cap was lit as if it faced *into* the screen — ambient light only, and
/// one toon band whatever the key light did (brief 4b-3).
fn orient_to_normals(mesh: &mut MeshBuffers) {
    let p = |i: u32| Vec3::from_array(mesh.positions[i as usize]);
    for tri in mesh.indices.as_chunks_mut::<3>().0 {
        let [a, b, c] = *tri;
        let face = (p(b) - p(a)).cross(p(c) - p(a));
        let normal = Vec3::from_array(mesh.normals[a as usize]);
        if face.dot(normal) < 0.0 {
            tri.swap(1, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygonize::polygonize;
    use crate::shape::ShapeDef;

    fn z_extent(mesh: &MeshBuffers) -> (f32, f32) {
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for p in &mesh.positions {
            min = min.min(p[2]);
            max = max.max(p[2]);
        }
        (min, max)
    }

    #[test]
    fn extrusion_spans_the_requested_z_range() {
        let contours = polygonize(&ShapeDef::Box {
            width: 20.0,
            height: 10.0,
        });
        let (z_front, depth) = (-10.0, 20.0);
        let mesh = extrude_contours(&contours, z_front, depth);
        let (zmin, zmax) = z_extent(&mesh);
        assert!((zmax - -10.0).abs() < 1e-4);
        assert!((zmin - -30.0).abs() < 1e-4);
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.positions.len(), mesh.uvs.len());
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    /// The face coordinates span the outline's bounding box as `[-1, 1]`:
    /// a box's corners land on `(±1, ±1)`, a circle's rim on the unit
    /// circle, whatever the size, so the toon dome looks alike on a pebble
    /// and on a plank.
    #[test]
    fn face_coords_span_the_outline_box() {
        let mesh = extrude_contours(
            &polygonize(&ShapeDef::Box {
                width: 3.0,
                height: 0.5,
            }),
            0.0,
            0.1,
        );
        for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
            assert!((uv[0].abs() - 1.0).abs() < 1e-4, "{p:?} -> {uv:?}");
            assert!((uv[1].abs() - 1.0).abs() < 1e-4, "{p:?} -> {uv:?}");
            assert!((uv[0] * 1.5 - p[0]).abs() < 1e-4 && (uv[1] * 0.25 - p[1]).abs() < 1e-4);
        }
        let mesh = extrude_contours(&polygonize(&ShapeDef::Circle { radius: 0.3 }), 0.0, 0.1);
        for uv in &mesh.uvs {
            let r = Vec2::from_array(*uv).length();
            assert!((r - 1.0).abs() < 0.02, "rim at r = {r}");
        }
    }

    #[test]
    fn every_triangle_winds_toward_its_normal() {
        for shape in [
            ShapeDef::Box {
                width: 2.0,
                height: 1.0,
            },
            ShapeDef::Circle { radius: 0.5 },
        ] {
            let mesh = extrude_contours(&polygonize(&shape), 0.0, 0.1);
            let p = |i: u32| Vec3::from_array(mesh.positions[i as usize]);
            let mut front = 0;
            for &[a, b, c] in mesh.indices.as_chunks::<3>().0 {
                let face = (p(b) - p(a)).cross(p(c) - p(a));
                let normal = Vec3::from_array(mesh.normals[a as usize]);
                assert!(face.dot(normal) > 0.0, "{shape:?}: triangle winds away");
                if normal.z > 0.5 {
                    front += 1;
                    assert!(face.z > 0.0, "the front cap faces the viewer (+Z)");
                }
            }
            assert!(front > 0);
        }
    }

    #[test]
    fn side_normals_point_outward_for_a_box() {
        let contours = polygonize(&ShapeDef::Box {
            width: 20.0,
            height: 20.0,
        });
        let mesh = extrude_contours(&contours, 0.0, 10.0);
        // Every side-wall vertex (normal.z == 0) must satisfy
        // dot(normal, position.xy) > 0 for a centered convex box.
        for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
            if n[2] == 0.0 {
                let dot = p[0] * n[0] + p[1] * n[1];
                assert!(dot > 0.0, "outward side normal ({p:?}, {n:?})");
            }
        }
    }
}

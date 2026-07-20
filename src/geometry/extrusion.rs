//! 2.5D extrusion: contours + collision-layer bits → prism mesh buffers.
//!
//! The depth convention (re-derived, behavior-preserving): a body occupying
//! layer bits `min..=max` spans `z ∈ [z_front - depth, z_front]` where
//! `z_front = -(min · LAYER_HEIGHT)` and `depth = (max - min + 1) · LAYER_HEIGHT`
//! — bit 0 is front-most, bit 31 back-most.

use crate::core::constants::LAYER_HEIGHT;
use crate::geometry::contours::Contours;
use crate::geometry::tessellate::tessellate;
use bevy::math::Vec2;

/// Raw mesh buffers, engine-agnostic.
#[derive(Debug, Clone, Default)]
pub struct MeshBuffers {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals.
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices (CCW = outward).
    pub indices: Vec<u32>,
}

/// Maps occupied layer bits to `(z_front, depth)`.
///
/// ```
/// use gradiance::geometry::extrusion::layer_z_range;
///
/// // A body on layer bit 0 alone: front face at z=0, one layer deep.
/// assert_eq!(layer_z_range(0, 0), (0.0, 10.0));
///
/// // Occupying bits 1..=3: front pushed back to -10, three layers deep.
/// assert_eq!(layer_z_range(1, 3), (-10.0, 30.0));
/// ```
pub fn layer_z_range(min_bit: u32, max_bit: u32) -> (f32, f32) {
    let z_front = -(min_bit as f32 * LAYER_HEIGHT);
    let depth = (max_bit - min_bit + 1) as f32 * LAYER_HEIGHT;
    (z_front, depth)
}

/// Extrudes `contours` into a closed prism spanning
/// `z ∈ [z_front - depth, z_front]`.
///
/// Front and back caps come from fill tessellation; side walls are flat
/// quads with outward normals (correct for CCW outlines and CW holes).
pub fn extrude_contours(contours: &Contours, z_front: f32, depth: f32) -> MeshBuffers {
    let z_back = z_front - depth;
    let cap = tessellate(contours);
    let mut mesh = MeshBuffers::default();

    // Front cap (+Z normal), original winding.
    let front_base = 0u32;
    for v in &cap.vertices {
        mesh.positions.push([v.x, v.y, z_front]);
        mesh.normals.push([0.0, 0.0, 1.0]);
    }
    mesh.indices.extend(cap.indices.iter().copied());

    // Back cap (−Z normal), reversed winding.
    let back_base = mesh.positions.len() as u32;
    for v in &cap.vertices {
        mesh.positions.push([v.x, v.y, z_back]);
        mesh.normals.push([0.0, 0.0, -1.0]);
    }
    for t in cap.indices.chunks_exact(3) {
        mesh.indices
            .extend([back_base + t[0], back_base + t[2], back_base + t[1]]);
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
            // Two CCW triangles facing `normal`.
            mesh.indices
                .extend([base, base + 3, base + 1, base + 1, base + 3, base + 2]);
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::shape::ShapeDef;
    use crate::geometry::polygonize::polygonize;

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
    fn layer_bits_map_to_depth_convention() {
        assert_eq!(layer_z_range(0, 0), (0.0, 10.0));
        assert_eq!(layer_z_range(1, 2), (-10.0, 20.0));
        assert_eq!(layer_z_range(31, 31), (-310.0, 10.0));
    }

    #[test]
    fn extrusion_spans_the_requested_z_range() {
        let contours = polygonize(&ShapeDef::Box {
            width: 20.0,
            height: 10.0,
        });
        let (z_front, depth) = layer_z_range(1, 2);
        let mesh = extrude_contours(&contours, z_front, depth);
        let (zmin, zmax) = z_extent(&mesh);
        assert!((zmax - -10.0).abs() < 1e-4);
        assert!((zmin - -30.0).abs() < 1e-4);
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.indices.len() % 3, 0);
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

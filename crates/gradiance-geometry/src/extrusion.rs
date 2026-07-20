//! 2.5D extrusion: contours + an authored depth span → prism mesh buffers.
//!
//! Callers pass the continuous `(z_front, depth)` straight from the body's
//! `DepthBand` (`z_front = -near`, `depth = far - near`); the legacy
//! layer-bit mapping retired with save v5.

use crate::contours::Contours;
use crate::tessellate::tessellate;
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

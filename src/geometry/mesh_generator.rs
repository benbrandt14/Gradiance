use crate::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy_rapier2d::rapier::geometry::TypedShape;

/// The thickness (Z-extent) of each collision layer visualization.
pub const LAYER_THICKNESS: f32 = 1.0;

/// Generates a 3D mesh from a 2D collider, extruded based on collision groups.
pub fn generate_mesh_from_collider(
    collider: &Collider,
    groups: Option<&CollisionGroups>,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let memberships = groups.map(|g| g.memberships.bits()).unwrap_or(Group::GROUP_1.bits());

    // Iterate first 5 layers for visualization
    for i in 0..5 {
        if (memberships & (1 << i)) != 0 {
            let z_base = i as f32 * LAYER_THICKNESS;
            let z_top = z_base + LAYER_THICKNESS;

            append_shape(
                &collider.raw, // SharedShape
                Vec2::ZERO,
                0.0, // Rotation
                z_base,
                z_top,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn append_shape(
    shape: &bevy_rapier2d::rapier::geometry::SharedShape,
    offset: Vec2,
    rotation: f32,
    z_min: f32,
    z_max: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    match shape.as_typed_shape() {
        TypedShape::Cuboid(c) => {
            let h = c.half_extents;
            // Apply rotation to box corners
            let rect_points = vec![
                Vec2::new(-h.x, -h.y),
                Vec2::new(h.x, -h.y),
                Vec2::new(h.x, h.y),
                Vec2::new(-h.x, h.y),
            ];
            append_extruded_polygon(
                &rect_points,
                offset,
                rotation,
                z_min,
                z_max,
                positions,
                normals,
                uvs,
                indices,
            );
        }
        TypedShape::Ball(b) => {
            append_cylinder(
                b.radius,
                offset,
                rotation,
                z_min,
                z_max,
                positions,
                normals,
                uvs,
                indices,
            );
        }
        TypedShape::ConvexPolygon(p) => {
            let pts: Vec<Vec2> = p.points().iter().map(|p| Vec2::new(p.x, p.y)).collect();
            append_extruded_polygon(
                &pts,
                offset,
                rotation,
                z_min,
                z_max,
                positions,
                normals,
                uvs,
                indices,
            );
        }
        TypedShape::Compound(c) => {
            for (iso, sub_shape) in c.shapes() {
                // Iso translation and rotation
                let sub_offset = Vec2::new(iso.translation.vector.x, iso.translation.vector.y);
                let sub_rot = iso.rotation.angle();

                // Combine with parent transform
                // New pos = parent_offset + parent_rot * sub_offset
                let rot_complex = Vec2::from_angle(rotation);
                let combined_offset = offset + rotate_vec2(sub_offset, rot_complex);
                let combined_rot = rotation + sub_rot;

                append_shape(
                    sub_shape,
                    combined_offset,
                    combined_rot,
                    z_min,
                    z_max,
                    positions,
                    normals,
                    uvs,
                    indices,
                );
            }
        }
        _ => {}
    }
}

fn rotate_vec2(v: Vec2, rot: Vec2) -> Vec2 {
    Vec2::new(v.x * rot.x - v.y * rot.y, v.x * rot.y + v.y * rot.x)
}

fn append_extruded_polygon(
    points: &[Vec2],
    offset: Vec2,
    rotation: f32,
    z_min: f32,
    z_max: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    if points.len() < 3 {
        return;
    }

    let rot_complex = Vec2::from_angle(rotation);
    let transformed_points: Vec<Vec2> = points
        .iter()
        .map(|p| offset + rotate_vec2(*p, rot_complex))
        .collect();

    let start_index = positions.len() as u32;
    let count = transformed_points.len();

    // Vertices
    // Top cap (z_max)
    for p in &transformed_points {
        positions.push([p.x, p.y, z_max]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.0, 0.0]); // Todo UVs
    }
    // Bottom cap (z_min)
    for p in &transformed_points {
        positions.push([p.x, p.y, z_min]);
        normals.push([0.0, 0.0, -1.0]);
        uvs.push([0.0, 0.0]);
    }

    // Indices for caps
    // Use a simple fan for convex polygons
    // Top cap (0, 1, 2), (0, 2, 3) ...
    for i in 1..count - 1 {
        indices.push(start_index);
        indices.push(start_index + i as u32);
        indices.push(start_index + i as u32 + 1);
    }

    // Bottom cap - reverse winding
    let bottom_start = start_index + count as u32;
    for i in 1..count - 1 {
        indices.push(bottom_start);
        indices.push(bottom_start + i as u32 + 1);
        indices.push(bottom_start + i as u32);
    }

    // Side walls
    // For each segment i -> (i+1)%count
    // We need duplicate vertices for flat shading normals,
    // OR we can share vertices if we want smooth normals (but sharp edges on boxes want split vertices).
    // Box needs split vertices for normals to be correct (flat shading).
    // For now, I'll generate new vertices for sides to ensure sharp edges.

    for i in 0..count {
        let p1 = transformed_points[i];
        let p2 = transformed_points[(i + 1) % count];

        let normal_2d = (p2 - p1).perp().normalize();
        let normal = [normal_2d.x, normal_2d.y, 0.0];

        let base_idx = positions.len() as u32;

        // 4 vertices per quad
        positions.push([p1.x, p1.y, z_max]); // Top Left
        positions.push([p2.x, p2.y, z_max]); // Top Right
        positions.push([p2.x, p2.y, z_min]); // Bottom Right
        positions.push([p1.x, p1.y, z_min]); // Bottom Left

        for _ in 0..4 {
            normals.push(normal);
            uvs.push([0.0, 0.0]);
        }

        // Quad indices
        indices.push(base_idx);
        indices.push(base_idx + 2); // Split quad into tri 1
        indices.push(base_idx + 1);

        indices.push(base_idx);
        indices.push(base_idx + 3); // Tri 2
        indices.push(base_idx + 2);
    }
}

fn append_cylinder(
    radius: f32,
    offset: Vec2,
    rotation: f32,
    z_min: f32,
    z_max: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let segments = 32;
    let mut points = Vec::with_capacity(segments);
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        points.push(Vec2::new(angle.cos() * radius, angle.sin() * radius));
    }

    append_extruded_polygon(
        &points,
        offset,
        rotation,
        z_min,
        z_max,
        positions,
        normals,
        uvs,
        indices,
    );
}

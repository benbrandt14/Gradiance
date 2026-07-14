//! Scalable particle rendering: **one rebuilt mesh**, not per-particle
//! entities or per-particle gizmos.
//!
//! The whole population draws as a single [`ParticleCloud`] mesh —
//! camera-facing quads, one per particle, colored by group + speed —
//! rebuilt each frame from the derived `SoA` buffer (the `extrude_sync`
//! rebuild pattern) and drawn opaque with depth writes so particles
//! depth-test against the extruded prisms. One mesh, one draw call: at the
//! spike's 50k-particle target this comfortably beats the per-particle
//! gizmo path (which buffered ~1.6M line segments/frame).
//!
//! Deliberate deviation from the trade study's "custom instanced buffer":
//! a rebuilt mesh on the *stable* `Mesh`/`Material` path is far more robust
//! than bevy 0.19's low-level instancing pipeline (which is render-internal
//! and targets `Transparent3d`). True GPU instancing — uploading only
//! per-instance data — is the **S8 scale** optimization, when the budget
//! grows past ~100k. The `build_particle_mesh` core is pure and
//! unit-tested, so that swap does not disturb it.
//!
//! Feature-gated (`sim`); render systems only run with a renderer present.

use crate::domain::appearance::Rgba;
use crate::sim::bridge::Particles;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Vec2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Particle disc half-size, world px.
const PARTICLE_HALF_PX: f32 = 2.0;
/// Depth the particle sheet renders at (just in front of the interaction
/// plane, like the old gizmo bias; per-particle depth arrives with groups).
const PARTICLE_Z: f32 = 1.0;
/// Speed (px/s) mapped to the warm/bright end of the tint ramp.
const WARM_SPEED: f32 = 1200.0;

/// Marker: the single mesh entity the whole population renders through.
#[derive(Component)]
pub struct ParticleCloud;

/// The vertex buffers for one frame's population — pure output of
/// [`build_particle_mesh`], assembled into a `Mesh` by the sync system.
#[derive(Debug, Default, PartialEq)]
pub struct ParticleMeshData {
    /// 4 positions per particle (a quad).
    pub positions: Vec<[f32; 3]>,
    /// 4 colors per particle (flat per quad).
    pub colors: Vec<[f32; 4]>,
    /// 6 indices per particle (2 triangles).
    pub indices: Vec<u32>,
}

/// Per-particle tint: a group hue (golden-angle walk, matching the layer
/// palette) warmed and brightened by speed, so flow and group both read.
pub fn particle_color(speed: f32, group: u32) -> [f32; 4] {
    let hue = (group as f32 * 137.508) % 360.0;
    let t = (speed / WARM_SPEED).clamp(0.0, 1.0);
    let c = Rgba::from_hsl(hue, 0.55 + 0.35 * t, 0.45 + 0.2 * t);
    [c.r, c.g, c.b, 1.0]
}

/// Builds the population's quad mesh buffers: one camera-facing quad per
/// particle (billboarded via the camera `right`/`up` basis), colored by
/// group + speed. Pure — unit-tested; no ECS, no `Assets`.
pub fn build_particle_mesh(
    pos: &[Vec2],
    vel: &[Vec2],
    group: &[u32],
    right: Vec3,
    up: Vec3,
    half: f32,
    z: f32,
) -> ParticleMeshData {
    let n = pos.len();
    let mut data = ParticleMeshData {
        positions: Vec::with_capacity(n * 4),
        colors: Vec::with_capacity(n * 4),
        indices: Vec::with_capacity(n * 6),
    };
    let r = right * half;
    let u = up * half;
    for (i, p) in pos.iter().enumerate() {
        let c = Vec3::new(p.x, p.y, z);
        let base = (i * 4) as u32;
        for corner in [c + r + u, c + r - u, c - r - u, c - r + u] {
            data.positions.push([corner.x, corner.y, corner.z]);
        }
        let speed = vel.get(i).map_or(0.0, |v| v.length());
        let col = particle_color(speed, group.get(i).copied().unwrap_or(0));
        data.colors.extend_from_slice(&[col; 4]);
        data.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    data
}

/// A zero-area (invisible, transparent) triangle. The cloud mesh is never
/// left empty: a 0-vertex mesh trips bevy's mesh slab allocator
/// ("use-after-free: unallocated key") on the frames before any particle
/// exists. This placeholder keeps the buffers validly allocated.
fn placeholder_data() -> ParticleMeshData {
    ParticleMeshData {
        positions: vec![[0.0, 0.0, PARTICLE_Z]; 3],
        colors: vec![[0.0; 4]; 3],
        indices: vec![0, 1, 2],
    }
}

/// Writes `data` (or the placeholder if empty) into `mesh`.
fn write_mesh(mesh: &mut Mesh, mut data: ParticleMeshData) {
    if data.positions.is_empty() {
        data = placeholder_data();
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors);
    mesh.insert_indices(Indices::U32(data.indices));
}

/// Spawns the single particle-cloud entity. An unlit, double-sided,
/// vertex-colored material — particles glow their own tint rather than
/// being shaded. `NoShadowCaster`/`NoShadowReceiver` because glowing
/// billboards must not cast or catch shadows; `NoFrustumCulling` because
/// the mesh AABB changes every frame with the population. The mesh starts
/// with a valid placeholder (never empty — see `placeholder_data`).
pub fn setup_particle_cloud(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    write_mesh(&mut mesh, ParticleMeshData::default());
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    commands.spawn((
        ParticleCloud,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Rebuilds the cloud mesh each frame from the population and the camera
/// basis (for billboarding). Render-only: reads the buffer, writes a mesh.
pub fn sync_particle_mesh(
    particles: Res<Particles>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    clouds: Query<&Mesh3d, With<ParticleCloud>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let Ok(cloud) = clouds.single() else {
        return;
    };
    let Some(mut mesh) = meshes.get_mut(&cloud.0) else {
        return;
    };
    let rotation = camera.rotation();
    let data = build_particle_mesh(
        &particles.0.pos,
        &particles.0.vel,
        &particles.0.group,
        rotation * Vec3::X,
        rotation * Vec3::Y,
        PARTICLE_HALF_PX,
        PARTICLE_Z,
    );
    write_mesh(&mut mesh, data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_particle_becomes_one_quad() {
        let pos = vec![Vec2::ZERO, Vec2::new(100.0, 0.0)];
        let vel = vec![Vec2::ZERO, Vec2::ZERO];
        let group = vec![0, 0];
        let d = build_particle_mesh(&pos, &vel, &group, Vec3::X, Vec3::Y, 2.0, 1.0);
        assert_eq!(d.positions.len(), 8, "4 verts per particle");
        assert_eq!(d.colors.len(), 8);
        assert_eq!(d.indices.len(), 12, "6 indices per particle");
        // The second quad indexes into its own vertex block.
        assert_eq!(d.indices[6..], [4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn quad_spans_the_camera_basis_at_the_particle() {
        let d = build_particle_mesh(
            &[Vec2::new(50.0, 20.0)],
            &[Vec2::ZERO],
            &[0],
            Vec3::X,
            Vec3::Y,
            3.0,
            1.0,
        );
        // Corners are center ± right·half ± up·half at z.
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-6);
        assert!(near(d.positions[0], [53.0, 23.0, 1.0]));
        assert!(near(d.positions[2], [47.0, 17.0, 1.0]));
    }

    #[test]
    fn empty_population_yields_empty_buffers() {
        let d = build_particle_mesh(&[], &[], &[], Vec3::X, Vec3::Y, 2.0, 1.0);
        assert_eq!(d, ParticleMeshData::default());
    }

    #[test]
    fn color_warms_with_speed_and_shifts_with_group() {
        let slow = particle_color(0.0, 0);
        let fast = particle_color(WARM_SPEED, 0);
        // Faster → brighter (higher luminance) in the same group.
        let lum = |c: [f32; 4]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!(lum(fast) > lum(slow));
        // Different groups get different hues (so colors differ).
        let g0 = particle_color(0.0, 0);
        let g1 = particle_color(0.0, 1);
        assert!((0..3).any(|k| (g0[k] - g1[k]).abs() > 1e-3));
    }
}

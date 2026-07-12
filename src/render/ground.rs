//! The infinite-ground material for half-plane bodies.
//!
//! Half-plane colliders are truly infinite (avian `half_space`); their render
//! used to be a visibly finite slab. Grounds now extrude a slab far larger
//! than any navigable view (no edge to find) with a dedicated material that
//! fades out when the camera orbits *inside* the solid half-space — the 3D
//! half-plane reading. Color comes from the body's own `Appearance`;
//! visibility from `ScenerySettings::ground_visible`.

use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::settings::ScenerySettings;
use crate::domain::shape::ShapeDef;
use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// Rendered ground width — far beyond any navigable pan/zoom range.
pub const GROUND_RENDER_WIDTH: f32 = 2_000_000.0;
/// Rendered ground drop below the surface line.
pub const GROUND_RENDER_DROP: f32 = 200_000.0;

/// The material half-plane bodies render with.
pub type GroundMaterial = ExtendedMaterial<StandardMaterial, GroundExtension>;

/// World-space plane data appended to the standard material's bind group.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct GroundExtension {
    /// `xy` = outward surface normal (world), `z` = plane offset
    /// (`dot(n, p) == z` on the surface), `w` = alpha floor when the camera
    /// is inside the solid.
    #[uniform(100)]
    pub plane: Vec4,
}

impl MaterialExtension for GroundExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://gradiance/render/ground.wgsl".into()
    }
}

/// Installs the ground material pipeline and its embedded shader.
pub fn install(app: &mut App) {
    embedded_asset!(app, "ground.wgsl");
    app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
}

/// (Re)builds a half-plane body's ground material when its appearance or
/// pose changes (the pose defines the world-space plane the shader fades
/// against). The prism mesh comes from `extrude_sync` like any body.
pub fn sync_ground_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<GroundMaterial>>,
    scenery: Res<ScenerySettings>,
    grounds: Query<
        (Entity, &ShapeDef, &Appearance, &Transform),
        (
            With<Body>,
            Or<(Changed<ShapeDef>, Changed<Appearance>, Changed<Transform>)>,
        ),
    >,
) {
    for (entity, shape, appearance, transform) in &grounds {
        if !matches!(shape, ShapeDef::HalfPlane) {
            continue;
        }
        let pose = crate::core::units::PosRot::from_transform(transform);
        let normal = Vec2::from_angle(pose.rot).rotate(Vec2::Y);
        let fill = appearance.fill;
        let handle = materials.add(GroundMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(fill.r, fill.g, fill.b, fill.a),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                double_sided: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: GroundExtension {
                plane: Vec4::new(normal.x, normal.y, normal.dot(pose.pos), 0.12),
            },
        });
        // Grounds render through this material, not the body toon material.
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<crate::render::toon::ToonMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
    // Visibility toggle (config seam); grounds only — bodies are unaffected.
    if scenery.is_changed() {
        commands.queue(|world: &mut World| {
            let visible = world.resource::<ScenerySettings>().ground_visible;
            let mut query = world.query_filtered::<(&ShapeDef, &mut Visibility), With<Body>>();
            for (shape, mut visibility) in query.iter_mut(world) {
                if matches!(shape, ShapeDef::HalfPlane) {
                    *visibility = if visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
        });
    }
}

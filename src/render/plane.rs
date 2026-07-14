//! Infinite-plane rendering, shared by half-plane grounds and the back
//! plane.
//!
//! Both are drawn as one enormous **opaque** quad with a material that
//! (a) fogs toward the backdrop color at the horizon so the surface reads
//! as infinite, (b) dissolves when the camera crosses *inside* the solid
//! half-space so orbiting through never blinds the view, and (c) traces a
//! faint line where the plane crosses the authoring plane (z = 0).
//! Opacity (with depth writes) matters: plane/plane and plane/body
//! intersections resolve as crisp seams, and there is no transparent-sort
//! order to flip as the view rotates. They differ **only in orientation**: a ground's plane
//! contains its surface line and sweeps along ±Z (the scene rests *on*
//! it, like pieces standing on a floor); the back plane is parallel to
//! the sim plane behind the deepest layer. Physics is untouched — the
//! ground collider is avian's genuinely infinite `half_space`; this
//! module only completes the visual.

use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::settings::ScenerySettings;
use crate::domain::shape::ShapeDef;
use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// Half-extent of the plane quads — far beyond any navigable range; the
/// shader's horizon fog ends the surface long before geometry does, and
/// the camera's depth slab ([`DEPTH_SLAB`](crate::interaction::camera))
/// is sized past the quad corners so the frustum never cuts them.
pub const PLANE_EXTENT: f32 = 500_000.0;

/// Horizon fade band, world pixels (distance from the eye).
pub const HORIZON_FADE: (f32, f32) = (60_000.0, 400_000.0);

/// The material infinite planes render with.
pub type InfinitePlaneMaterial = ExtendedMaterial<StandardMaterial, PlaneExtension>;

/// World-space plane data appended to the standard material's bind group.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct PlaneExtension {
    /// `xyz` = outward plane normal (world), `w` = plane offset
    /// (`dot(n, p) == w` on the surface).
    #[uniform(100)]
    pub plane: Vec4,
    /// `y`/`z` = horizon fog start/end distance from the eye (`x`/`w`
    /// spare).
    #[uniform(101)]
    pub fade: Vec4,
    /// Linear-space backdrop color the horizon fog fades toward (matches
    /// the camera clear color).
    #[uniform(102)]
    pub horizon: Vec4,
}

impl PlaneExtension {
    /// Standard fade parameters for a plane with the given world equation.
    /// `dot_spacing` is the ground dot-grid pitch in world px (0 = no dots,
    /// e.g. the vertical back plane).
    pub fn for_plane(normal: Vec3, offset: f32, horizon: Color, dot_spacing: f32) -> Self {
        let h = horizon.to_linear();
        Self {
            plane: normal.extend(offset),
            fade: Vec4::new(dot_spacing, HORIZON_FADE.0, HORIZON_FADE.1, 0.0),
            horizon: Vec4::new(h.red, h.green, h.blue, 1.0),
        }
    }
}

impl MaterialExtension for PlaneExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://gradiance/render/plane.wgsl".into()
    }
}

/// A matte plane material in the given color, fogging toward `horizon`.
/// `dot_spacing` sets the ground dot-grid pitch in world px (0 = no dots).
pub fn plane_material(
    color: Color,
    normal: Vec3,
    offset: f32,
    horizon: Color,
    dot_spacing: f32,
) -> InfinitePlaneMaterial {
    InfinitePlaneMaterial {
        base: StandardMaterial {
            base_color: color,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            metallic: 0.0,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: PlaneExtension::for_plane(normal, offset, horizon, dot_spacing),
    }
}

/// Installs the plane material pipeline and its embedded shader.
pub fn install(app: &mut App) {
    embedded_asset!(app, "plane.wgsl");
    app.add_plugins(MaterialPlugin::<InfinitePlaneMaterial>::default());
}

/// The flat ground quad in body-local space: spans local X (the surface
/// line) and world Z (the depth axis), normal = local +Y. The body's
/// rotation orients it, exactly like the collider.
pub fn ground_plane_mesh() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let e = PLANE_EXTENT;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-e, 0.0, e], [e, 0.0, e], [e, 0.0, -e], [-e, 0.0, -e]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 4]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// (Re)builds a half-plane body's plane material when its appearance or
/// pose changes (the pose defines the world plane the shader fades
/// against). The quad mesh comes from `extrude_sync`.
pub fn sync_ground_planes(
    mut commands: Commands,
    mut materials: ResMut<Assets<InfinitePlaneMaterial>>,
    scenery: Res<ScenerySettings>,
    clear: Res<ClearColor>,
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
        let handle = materials.add(plane_material(
            Color::srgba(fill.r, fill.g, fill.b, fill.a),
            normal.extend(0.0),
            normal.dot(pose.pos),
            clear.0,
            crate::core::constants::PIXELS_PER_METER,
        ));
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

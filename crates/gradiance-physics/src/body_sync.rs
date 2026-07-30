//! Authored components → engine components, `Changed<>`-driven and idempotent.

use crate::plane::{locked_axes, plane_constraint};
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gradiance_core::units::PlaneFrame;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::props::{BodyKind, BodyPhysics};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::contours::Contours;
use gradiance_geometry::convex::{self, MAX_PIECES};
use gradiance_units::{Area, mass_of};
use std::f32::consts::FRAC_PI_2;

/// Half-thickness of the collision slab, in plane-normal metres.
///
/// **Every** collider is extruded to exactly `z ∈ [−SLAB, +SLAB]` in body-local
/// space, regardless of its [`DepthBand`]. Two consequences, both load-bearing:
///
/// 1. Identical z extents mean the plane normal is never the minimum separating
///    axis for any in-plane penetration shallower than `SLAB`. Every contact
///    normal is therefore in-plane and every manifold is the 2D manifold
///    extruded — the 3D solver is mathematically a 2D one. That is what makes
///    "every feature behaves identically" true rather than aspirational.
/// 2. Depth separation is done **entirely** by [`sync_collision_groups`], a 1:1
///    port of the previous collision-layer semantics — including the
///    adjacent-band case that real extruded geometry would silently break.
///
/// Deriving the extrusion from the band instead looks more honest and is worse.
/// Bands `[0, 0.1]` and `[0.1, 0.2]` are disjoint under [`DepthBand::bits`] but
/// their faces *touch* in 3D, so bodies that never collided would start resting
/// on each other. And partially overlapping bands share only a sliver of z:
/// once in-plane penetration exceeds it, the solver picks z as the separating
/// axis and pushes along a locked degree of freedom — a no-op — so the pair
/// interpenetrates permanently.
///
/// 1 m is ~10× a layer and ~10× a typical body: deep enough that z never wins
/// the separating-axis contest, shallow enough to keep hulls well conditioned.
/// The visual extrusion still uses the band exactly; collider z and mesh z
/// differ on purpose.
const SLAB: f32 = 1.0;

/// Builds the engine collider for a validated shape.
///
/// Returns `None` (and the caller keeps the previous collider) when the shape
/// fails validation — commands should have refused it upstream.
fn collider_for(shape: &ShapeDef, plane: &PlaneFrame) -> Option<Collider> {
    shape.validate().ok()?;
    match shape {
        ShapeDef::Box { width, height } => Some(Collider::cuboid(width / 2.0, height / 2.0, SLAB)),
        // Exact, where the 2D engine had to settle for a 48-gon: a cylinder
        // about the plane normal rolls perfectly, which Algodoo parity wants.
        // rapier's cylinder is Y-axial, so stand it up along the normal.
        ShapeDef::Circle { radius } => Some(Collider::compound(vec![(
            Vec3::ZERO,
            Quat::from_rotation_x(FRAC_PI_2),
            Collider::cylinder(SLAB, *radius),
        )])),
        // Genuinely infinite, as before: the surface passes through the body
        // origin with local +Y outward, and body rotation orients it.
        ShapeDef::HalfPlane => Collider::halfspace(plane.dir(Vec2::Y)),
        ShapeDef::Polygon { outline, holes } => compound_for(&[Contours {
            outline: outline.clone(),
            holes: holes.clone(),
        }]),
        // CSG trees collide as their contoured components, so a merged-but-
        // barely-touching union keeps full collision coverage.
        ShapeDef::Csg { .. } | ShapeDef::Placed { .. } => compound_for(
            &gradiance_geometry::polygonize::polygonize_components(shape),
        ),
    }
}

/// Convex-decomposes `components` and extrudes each piece into a prism.
///
/// Exact where rapier's own decomposition is not: its `convex_decomposition` is
/// V-HACD over a mesh — approximate, slow, and non-deterministic — whereas the
/// input here is already polygonal, so
/// [`convex_decompose_components`](gradiance_geometry::convex::convex_decompose_components)
/// splits it exactly and each piece extrudes to a convex prism.
///
/// Over [`MAX_PIECES`] the body collides as its convex hull. Concavity is lost,
/// which is visible; the alternative is a compound whose narrow-phase cost
/// scales with the product of piece counts, which is not.
fn compound_for(components: &[Contours]) -> Option<Collider> {
    let pieces = convex::convex_decompose_components(components);
    if pieces.is_empty() {
        return None;
    }
    if pieces.len() > MAX_PIECES {
        let all: Vec<Vec2> = components
            .iter()
            .flat_map(Contours::rings)
            .flatten()
            .copied()
            .collect();
        let hull = gradiance_geometry::hull::convex_hull(&all);
        warn!(
            pieces = pieces.len(),
            "shape exceeds the collider piece budget; colliding as its convex hull"
        );
        return prism(&hull).map(|c| Collider::compound(vec![(Vec3::ZERO, Quat::IDENTITY, c)]));
    }
    let parts: Vec<(Vec3, Quat, Collider)> = pieces
        .iter()
        .filter_map(|piece| prism(piece).map(|c| (Vec3::ZERO, Quat::IDENTITY, c)))
        .collect();
    (!parts.is_empty()).then(|| Collider::compound(parts))
}

/// One convex 2D piece swept across the collision slab.
///
/// The hull of the 2N extruded vertices is exact for a convex cross-section,
/// and cheaper for the narrow phase than a triangle mesh.
fn prism(piece: &[Vec2]) -> Option<Collider> {
    if piece.len() < 3 {
        return None;
    }
    let points: Vec<Vec3> = piece
        .iter()
        .flat_map(|v| [v.extend(-SLAB), v.extend(SLAB)])
        .collect();
    Collider::convex_hull(&points)
}

/// Regenerates colliders for bodies whose shape changed.
pub fn sync_colliders(
    mut commands: Commands,
    changed: Query<(Entity, &ShapeDef), (With<Body>, Changed<ShapeDef>)>,
) {
    let plane = PlaneFrame::XY;
    for (entity, shape) in &changed {
        match collider_for(shape, &plane) {
            Some(collider) => {
                commands.entity(entity).insert(collider);
            }
            None => {
                warn!(?entity, "invalid ShapeDef reached body_sync; collider kept");
            }
        }
    }
}

/// Applies collision groups for bodies whose depth band (or shape) changed.
///
/// Memberships *and* filters are the band's derived layer bits, so two bodies
/// are candidate-paired exactly when their bands share a layer — collision
/// layer ≡ visual depth, unchanged from the 2D engine. Ground half-planes are
/// the base everything rests on and collide with all.
///
/// With the uniform collision slab (see `SLAB`) this is the **sole**
/// depth-separation mechanism, so it cannot disagree with geometry: there is no
/// geometric depth left to disagree with. It is also the cheapest one
/// available — a filtered pair never reaches the narrow phase at all, which is
/// what keeps a deeply layered scene affordable.
pub fn sync_collision_groups(
    mut commands: Commands,
    changed: Query<
        (Entity, &DepthBand, &ShapeDef),
        (With<Body>, Or<(Changed<DepthBand>, Changed<ShapeDef>)>),
    >,
) {
    for (entity, band, shape) in &changed {
        let bits = if shape.contains_half_plane() {
            u32::MAX
        } else {
            band.bits()
        };
        let group = Group::from_bits_truncate(bits);
        commands
            .entity(entity)
            .insert(CollisionGroups::new(group, group));
    }
}

/// Translates the authored [`BodyPhysics`] into engine components.
///
/// **The one write path for authored physics.** Everything upstream — tools,
/// commands, the inspector, scripts — edits the plain domain component; this is
/// where it becomes something the solver understands.
pub fn sync_body_physics(
    mut commands: Commands,
    changed: Query<(Entity, &BodyPhysics), (With<Body>, Changed<BodyPhysics>)>,
) {
    let constraint = plane_constraint(&PlaneFrame::XY);
    for (entity, physics) in &changed {
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            rigid_body_for(physics.kind),
            Friction::coefficient(physics.friction),
            Restitution::coefficient(physics.restitution),
            GravityScale(physics.gravity_scale),
            locked_axes(constraint, physics),
        ));
        if physics.sensor {
            entity_commands.insert(Sensor);
        } else {
            entity_commands.remove::<Sensor>();
        }
    }
}

/// Sets each body's mass and inertia **explicitly, from 2D**.
///
/// The engine would happily derive them from the collider's volume — and would
/// be wrong, because the collider's z extent is the uniform slab rather than
/// anything physical. Computing them here keeps areal density (kg/m²) meaning
/// what it always meant and every body's mass identical to the 2D build.
///
/// Mass goes through [`mass_of`], the single density × geometry seam; the
/// rotational half is the contour's polar moment. The two locked rotation axes
/// get the same inertia — unreachable, but they must stay finite and positive.
pub fn sync_mass_properties(
    mut commands: Commands,
    changed: Query<
        (Entity, &ShapeDef, &BodyPhysics),
        (With<Body>, Or<(Changed<ShapeDef>, Changed<BodyPhysics>)>),
    >,
) {
    for (entity, shape, physics) in &changed {
        // A half-plane is infinite; it is always static, so let the engine
        // treat it as immovable rather than integrating an infinite area.
        if shape.contains_half_plane() {
            continue;
        }
        let contours = gradiance_geometry::polygonize::polygonize(shape);
        let mass = mass_of(physics.density, Area(contours.area()));
        if mass.value() <= 0.0 {
            continue;
        }
        let inertia =
            physics.density.value() * gradiance_geometry::inertia::polar_moment(&contours);
        commands
            .entity(entity)
            .insert(ColliderMassProperties::MassProperties(MassProperties {
                local_center_of_mass: Vec3::ZERO,
                mass: mass.value(),
                principal_inertia_local_frame: Quat::IDENTITY,
                principal_inertia: Vec3::splat(inertia.max(f32::MIN_POSITIVE)),
            }));
    }
}

/// The engine's simulation role for an authored [`BodyKind`].
fn rigid_body_for(kind: BodyKind) -> RigidBody {
    match kind {
        BodyKind::Dynamic => RigidBody::Dynamic,
        BodyKind::Static => RigidBody::Fixed,
        BodyKind::Kinematic => RigidBody::KinematicPositionBased,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_leaves_build_exact_primitives() {
        let plane = PlaneFrame::XY;
        assert!(
            collider_for(
                &ShapeDef::Box {
                    width: 1.0,
                    height: 2.0
                },
                &plane
            )
            .is_some()
        );
        assert!(collider_for(&ShapeDef::Circle { radius: 0.5 }, &plane).is_some());
        assert!(collider_for(&ShapeDef::HalfPlane, &plane).is_some());
    }

    #[test]
    fn a_concave_polygon_builds_a_compound() {
        let l = ShapeDef::Polygon {
            outline: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
            holes: vec![],
        };
        assert!(collider_for(&l, &PlaneFrame::XY).is_some());
    }

    #[test]
    fn an_invalid_shape_builds_nothing() {
        let degenerate = ShapeDef::Polygon {
            outline: vec![Vec2::ZERO, Vec2::X],
            holes: vec![],
        };
        assert!(collider_for(&degenerate, &PlaneFrame::XY).is_none());
    }

    #[test]
    fn body_kinds_map_to_the_engine_roles() {
        assert_eq!(rigid_body_for(BodyKind::Dynamic), RigidBody::Dynamic);
        assert_eq!(rigid_body_for(BodyKind::Static), RigidBody::Fixed);
        assert_eq!(
            rigid_body_for(BodyKind::Kinematic),
            RigidBody::KinematicPositionBased
        );
    }

    /// Depth separation is the filter's job, so the mask must be exactly the
    /// band's bits — including for adjacent bands, whose faces touch in 3D.
    #[test]
    fn adjacent_bands_have_disjoint_groups() {
        let front = DepthBand {
            near: 0.0,
            far: 0.1,
        };
        let back = DepthBand {
            near: 0.1,
            far: 0.2,
        };
        assert_eq!(
            front.bits() & back.bits(),
            0,
            "adjacent bands must not be candidate-paired"
        );
    }
}

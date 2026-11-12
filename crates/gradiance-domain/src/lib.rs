//! Authored components — the durable content of a scene.
//!
//! Everything in this module (plus [`StableId`](gradiance_core::ids::StableId)
//! and `Transform` pose) **is** the save file. No engine-specific types:
//! colliders, meshes, materials, and avian joints are *derived* from these
//! components by `Changed<>`-driven sync systems and are never serialized.

pub mod appearance;
pub mod depth;
pub mod field;
pub mod group;
pub mod joint;
pub mod layers;
pub mod node;
pub mod props;
pub mod settings;
pub use gradiance_geometry::shape;
pub mod signal;
pub mod tracer;

use bevy::prelude::*;

/// Marker component present on every authored body.
///
/// **Required components** enforce the authored body archetype: spawning a
/// bare `Body` (a test, a future script path or importer) still yields an
/// entity with every authored companion, so capture, persistence, and the
/// derived sync systems never meet a half-built body. An explicit value in
/// the spawn bundle always wins over these defaults; they only fill gaps.
///
/// - [`StableId`]: its `Default` mints a **fresh** UUID (never a shared
///   value — invariant 3).
/// - `ShapeDef`: a 1 m unit box (see its `Default`).
/// - [`Appearance`](appearance::Appearance), [`DepthBand`](depth::DepthBand),
///   `Transform`: their own defaults.
/// - The authored avian components, at the values [`BodyPhysics::default`]
///   assigns (so a bare body's live physics matches what capture records):
///   `RigidBody::Dynamic`, `Friction`, `Restitution`, `ColliderDensity`,
///   `GravityScale`. The optional markers (`Sensor`, `LockedAxes`) are absent
///   by default and so are not required.
///
/// Derived state (`Collider`, `Mass`, meshes) is deliberately **not**
/// required — it is rebuilt by the `Changed<>`-driven sync systems
/// (invariant 5).
///
/// [`StableId`]: gradiance_core::ids::StableId
/// [`BodyPhysics::default`]: props::BodyPhysics
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(
    gradiance_core::ids::StableId,
    Transform,
    shape::ShapeDef,
    appearance::Appearance,
    depth::DepthBand,
    avian2d::prelude::RigidBody = avian2d::prelude::RigidBody::Dynamic,
    avian2d::prelude::Friction = props::default_friction(),
    avian2d::prelude::Restitution = props::default_restitution(),
    avian2d::prelude::ColliderDensity = props::default_density(),
    avian2d::prelude::GravityScale = props::default_gravity_scale()
)]
pub struct Body;

/// Marker component present on every authored joint.
///
/// Requires only the companions with a safe default: a fresh [`StableId`]
/// and the identity `Transform` the derived pin anchors parent under.
/// `JointDef` is **not** required: a joint is defined by the two bodies it
/// connects (`body_a` by `StableId`), and no default can name a real body —
/// a defaulted joint would silently reference nothing. A joint spawned
/// without its `JointDef` stays visibly incomplete (capture skips it)
/// rather than being papered over.
///
/// [`StableId`]: gradiance_core::ids::StableId
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(gradiance_core::ids::StableId, Transform)]
pub struct Joint;

/// Registers domain-level requirements (currently nothing beyond types).
#[derive(Default)]
pub struct DomainPlugin;

impl Plugin for DomainPlugin {
    fn build(&self, _app: &mut App) {}
}

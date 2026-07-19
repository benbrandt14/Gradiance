//! Reflection contract for the authored intent surface.
//!
//! The scripting operation registry binds body/joint operations by reflected
//! type name (`docs/script-lisp-decision.md`). That rests on spike #1's
//! resolution: `StableId` (a `Uuid` newtype) and `ShapeDef` (an SDF enum in
//! flux) reflect as **opaque** values, which unblocks every intent/record that
//! references a body by id or carries a shape from deriving `Reflect`.
//!
//! These tests are deliberately **feature-independent** (no `--features
//! script`): they prove the reflect substrate — registration + a `Reflect`
//! round-trip through the opaque leaves — is real in the default build, which
//! is the substrate the steel bridge (Tier-A, behind the feature) sits on.

// Test-only: panics/expects are the failure mechanism.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use bevy::reflect::{FromReflect, PartialReflect, ReflectRef};
use gradiance::command::CommandPlugin;
use gradiance::prelude::*;

/// Builds the `AppTypeRegistry`-populating `CommandPlugin` in isolation and
/// returns the app so the registry can be inspected.
fn app_with_registry() -> bevy::app::App {
    let mut app = bevy::app::App::new();
    app.add_plugins(CommandPlugin);
    app
}

#[test]
fn stable_id_and_shape_reflect_opaque() {
    // The two spike-#1 linchpins: identity handle + geometry handle both hide
    // their structure from the reflection API.
    let id = StableId::new();
    assert!(
        matches!(id.reflect_ref(), ReflectRef::Opaque(_)),
        "StableId must reflect as opaque (identity handle, not a field struct)"
    );

    let shape = ShapeDef::Circle { radius: 5.0 };
    assert!(
        matches!(shape.reflect_ref(), ReflectRef::Opaque(_)),
        "ShapeDef must reflect as opaque (constructor-built foreign value)"
    );

    // A deep CSG tree is still one opaque leaf — the enum's shape never leaks.
    let csg = ShapeDef::Csg {
        op: gradiance::prelude::CsgOp::Union,
        lhs: Box::new(ShapeDef::Box {
            width: 2.0,
            height: 2.0,
        }),
        rhs: Box::new(ShapeDef::Circle { radius: 1.0 }),
    };
    assert!(matches!(csg.reflect_ref(), ReflectRef::Opaque(_)));
}

#[test]
fn authored_intents_are_registered() {
    let app = app_with_registry();
    let registry = app
        .world()
        .resource::<bevy::ecs::reflect::AppTypeRegistry>()
        .read();

    // The whole authored intent surface must be addressable by type (keyed by
    // `TypeId`, the registry's identity — this is what lets the operation
    // registry bind an op to its reflected type).
    let registered =
        |id, name: &str| assert!(registry.contains(id), "intent `{name}` unregistered");
    registered(std::any::TypeId::of::<SpawnBodyIntent>(), "SpawnBodyIntent");
    registered(
        std::any::TypeId::of::<SpawnJointIntent>(),
        "SpawnJointIntent",
    );
    registered(std::any::TypeId::of::<DeleteIntent>(), "DeleteIntent");
    registered(std::any::TypeId::of::<DuplicateIntent>(), "DuplicateIntent");
    registered(
        std::any::TypeId::of::<CommitTransformIntent>(),
        "CommitTransformIntent",
    );
    registered(std::any::TypeId::of::<ScaleIntent>(), "ScaleIntent");
    registered(std::any::TypeId::of::<ArrayIntent>(), "ArrayIntent");
    registered(
        std::any::TypeId::of::<PropertyEditIntent>(),
        "PropertyEditIntent",
    );
    registered(std::any::TypeId::of::<GroupIntent>(), "GroupIntent");
    registered(std::any::TypeId::of::<UngroupIntent>(), "UngroupIntent");
    registered(std::any::TypeId::of::<CutIntent>(), "CutIntent");
    registered(
        std::any::TypeId::of::<DeleteJointIntent>(),
        "DeleteJointIntent",
    );
    registered(std::any::TypeId::of::<MergeIntent>(), "MergeIntent");
    registered(std::any::TypeId::of::<LoadSceneIntent>(), "LoadSceneIntent");
}

#[test]
fn intent_registration_pulls_in_transitive_types() {
    let app = app_with_registry();
    let registry = app
        .world()
        .resource::<bevy::ecs::reflect::AppTypeRegistry>()
        .read();
    let has = |id| registry.get(id).is_some();

    // Registering the intents must recursively register their field types —
    // records, opaque handles, domain types, and the authored avian
    // components (the read-total path reads those through reflection).
    assert!(has(std::any::TypeId::of::<BodyRecord>()), "BodyRecord");
    assert!(has(std::any::TypeId::of::<JointRecord>()), "JointRecord");
    assert!(has(std::any::TypeId::of::<StableId>()), "StableId");
    assert!(has(std::any::TypeId::of::<ShapeDef>()), "ShapeDef");
    assert!(has(std::any::TypeId::of::<BodyPhysics>()), "BodyPhysics");
    assert!(has(std::any::TypeId::of::<JointDef>()), "JointDef");
    assert!(has(std::any::TypeId::of::<MotorDef>()), "MotorDef");
    assert!(
        has(std::any::TypeId::of::<avian2d::prelude::RigidBody>()),
        "avian RigidBody (transitive, read-total path)"
    );
    assert!(
        has(std::any::TypeId::of::<avian2d::prelude::Friction>()),
        "avian Friction (transitive, read-total path)"
    );
}

#[test]
fn spawn_body_intent_round_trips_through_reflection() {
    let record = BodyRecord {
        id: StableId::new(),
        pose: PosRot {
            pos: bevy::math::Vec2::new(12.0, -3.5),
            rot: 0.25,
        },
        shape: ShapeDef::Box {
            width: 40.0,
            height: 20.0,
        },
        physics: BodyPhysics::default(),
        appearance: Appearance::default(),
        depth: DepthBand::default(),
        layers: None,
        groups: vec![7],
        field: Some(gradiance::domain::field::FieldSource {
            strength: -12.5,
            falloff: gradiance::domain::field::FieldFalloff::Linear,
        }),
        tracer: Some(gradiance::domain::tracer::Tracer {
            fade_secs: 1.5,
            ..Default::default()
        }),
        rod: None,
        rod_tip: false,
    };
    let intent = SpawnBodyIntent {
        record: record.clone(),
    };

    // Type-erase to a dynamic representation, then rebuild the concrete type —
    // this exercises `FromReflect` across the opaque `StableId`/`ShapeDef`
    // leaves and the structural avian-backed `BodyPhysics`.
    let dynamic = intent.to_dynamic();
    let rebuilt =
        SpawnBodyIntent::from_reflect(&*dynamic).expect("from_reflect rebuilds the intent");
    assert_eq!(rebuilt.record, record);
}

#[test]
fn spawn_joint_intent_round_trips_through_reflection() {
    let record = JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind: JointKind::Hinge {
                limits: Some([-1.0, 1.0]),
                motor: Some(MotorDef::default()),
            },
            common: JointCommon {
                collide_connected: true,
            },
            body_a: StableId::new(),
            body_b: Some(StableId::new()),
            anchor_a: bevy::math::Vec2::new(1.0, 2.0),
            anchor_b: bevy::math::Vec2::new(3.0, 4.0),
            rest_rot_a: 0.1,
            rest_rot_b: -0.2,
        },
    };
    let intent = SpawnJointIntent {
        record: record.clone(),
    };

    let dynamic = intent.to_dynamic();
    let rebuilt =
        SpawnJointIntent::from_reflect(&*dynamic).expect("from_reflect rebuilds the joint intent");
    assert_eq!(rebuilt.record, record);
}

//! Scene-format pins for the typed quantities (brief 5-1; format v8 since 4b-4).
//!
//! [`every_quantity_scene`] sets every authored field that carries a
//! `gradiance-units` quantity to a distinctive non-default value. The golden
//! test saves it and compares the bytes with the checked-in fixture, then
//! reloads the fixture and compares values.
//!
//! The structural guard walks the same scene by reflection instead: every
//! `gradiance_units` quantity it reaches gets a unique sentinel, and the saved
//! text must spell each one bare (`0.5`, never `(0.5)`). That catches a
//! quantity field this fixture forgot to set, or a quantity type that lost
//! `#[serde(transparent)]`.
//!
//! Regenerate the fixture only for an intended format change:
//! `UPDATE_GOLDEN=1 cargo test -p gradiance-scene format_golden`.

// float_cmp: sentinels are written and read back exactly.
#![allow(clippy::float_cmp)]

use crate::{BodyRecord, from_ron, to_ron};
use crate::{EnvironmentRecord, FORMAT_VERSION, JointRecord, NodeRecord, SceneRecord};
use bevy::math::Vec2;
use bevy::reflect::{PartialReflect, ReflectMut};
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::field::{FieldFalloff, FieldSource};
use gradiance_domain::fluid::Fluid;
use gradiance_domain::joint::{AngularMotorDef, JointCommon, JointDef, JointKind, LinearMotorDef};
use gradiance_domain::laser::Laser;
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::thruster::Thruster;
use gradiance_domain::tracer::{TracePattern, Tracer};

const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/v8_every_quantity.ron");

fn id(n: u8) -> StableId {
    let text = format!("(\"00000000-0000-0000-0000-0000000000{n:02x}\")");
    ron::from_str(&text).expect("a fixed StableId parses")
}

/// A `gradiance-units` quantity from its base-SI text (`"0.25"`,
/// `"(1.5, -9.75)"`), typed by inference from the field it is assigned to —
/// so this crate builds typed values without a `gradiance-units` dependency
/// edge.
fn q<T: serde::de::DeserializeOwned>(si: &str) -> T {
    ron::from_str(si).expect("a quantity parses")
}

fn body(n: u8, depth: DepthBand) -> BodyRecord {
    BodyRecord {
        id: id(n),
        pose: PosRot {
            pos: Vec2::new(f32::from(n), 0.5),
            rot: 0.25,
        },
        shape: ShapeDef::Circle { radius: 0.5 },
        physics: BodyPhysics::default(),
        appearance: Appearance::default(),
        depth,
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        fluid: None,
        motion: None,
        optics: None,
        lifetime: None,
        surface: None,
        texture: None,
    }
}

fn joint(n: u8, kind: JointKind) -> JointRecord {
    JointRecord {
        id: id(n),
        def: JointDef {
            kind,
            common: JointCommon {
                collide_connected: false,
                break_force: Some(q("250.0")),
                broken: false,
            },
            body_a: id(1),
            body_b: Some(id(2)),
            anchor_a: q("(0.125, -0.375)"),
            anchor_b: q("(-0.625, 0.875)"),
            rest_rot_a: q("0.3125"),
            rest_rot_b: q("-0.4375"),
        },
    }
}

/// A scene with every quantity-bearing authored field set to a distinctive,
/// non-default value.
#[expect(
    clippy::too_many_lines,
    reason = "one literal scene that names every saved field"
)]
pub(crate) fn every_quantity_scene() -> SceneRecord {
    let mut field_body = body(1, DepthBand::metres(0.75, 3.5));
    field_body.field = Some(FieldSource {
        strength: q("-12.5"),
        falloff: FieldFalloff::Quadratic,
    });
    let mut tracer_body = body(2, DepthBand::metres(0.0, 1.25));
    tracer_body.tracer = Some(Tracer {
        fade_secs: q("2.75"),
        size: q("0.0625"),
        pattern: TracePattern::Dots,
    });
    // A fluid region (brief N-7) on the tracer body: its density is a quantity.
    tracer_body.fluid = Some(Fluid {
        density: q("1.75"),
        drag: 2.5,
    });

    let hinge = JointKind::Hinge {
        limits: Some([q("-1.5"), q("0.875")]),
        motor: Some(AngularMotorDef::default()),
    };
    let slider = JointKind::Slider {
        axis: Vec2::new(0.6, 0.8),
        limits: Some([q("-0.25"), q("1.75")]),
        motor: Some(LinearMotorDef::default()),
    };
    let spring = JointKind::Spring {
        rest_length: q("1.375"),
        frequency: q("2.5"),
        damping_ratio: 0.375,
        range: Some([q("0.5"), q("2.25")]),
    };

    let mut environment = EnvironmentRecord::default();
    environment.sim.gravity = q("(1.5, -9.75)");
    environment.sim.timestep_hz = q("120.0");
    environment.sim.air_density = q("1.25");
    environment.sim.drag_coefficient = 0.75;
    environment.sim.linear_drag = 0.5;
    environment.sim.wind = q("(2.5, -0.5)");
    environment.sim.air_buoyancy = true;
    environment.grid.spacing = q("0.25");
    environment.grid.origin = q("(0.5, -1.5)");
    environment.grid.rotation = q("0.125");
    environment.lighting.shadow_distance = q("42.5");
    environment.scenery.back_offset = q("0.375");

    SceneRecord {
        version: FORMAT_VERSION,
        app_version: "golden".into(),
        bodies: vec![field_body, tracer_body],
        joints: vec![joint(16, hinge), joint(17, slider), joint(18, spring)],
        nodes: vec![
            NodeRecord {
                id: id(32),
                pose: PosRot {
                    pos: Vec2::new(-2.0, 1.0),
                    rot: 0.0,
                },
                attachment: NodeAttachment {
                    target: Some(id(1)),
                    offset: q("(0.1875, -0.8125)"),
                },
                kind: NodeKind::Tracer(Tracer {
                    fade_secs: q("4.5"),
                    size: q("0.03125"),
                    pattern: TracePattern::Line,
                }),
                appearance: Appearance::default(),
            },
            // A thruster (brief N-3): its thrust and axis are quantities.
            NodeRecord {
                id: id(33),
                pose: PosRot {
                    pos: Vec2::new(1.0, 0.25),
                    rot: 0.0,
                },
                attachment: NodeAttachment {
                    target: Some(id(1)),
                    offset: q("(-0.3125, 0.4375)"),
                },
                kind: NodeKind::Thruster(Thruster {
                    thrust: q("12.5"),
                    angle: q("0.625"),
                }),
                appearance: Appearance::default(),
            },
            // A free laser aimed at the field body (brief N-8): its axis and
            // range are quantities.
            NodeRecord {
                id: id(34),
                pose: PosRot {
                    pos: Vec2::new(-1.5, 2.0),
                    rot: 0.0,
                },
                attachment: NodeAttachment::free(),
                kind: NodeKind::Laser(Laser {
                    name: "gate".into(),
                    target: Some(id(1)),
                    angle: q("-0.75"),
                    range: q("12.5"),
                    bounces: 3,
                    color: None,
                    white: false,
                }),
                appearance: Appearance::default(),
            },
        ],
        environment,
    }
}

#[test]
fn every_quantity_saves_byte_identical_to_the_v7_fixture() {
    let saved = to_ron(&every_quantity_scene()).expect("the scene saves");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(FIXTURE_PATH, &saved).expect("the fixture writes");
    }
    let fixture = std::fs::read_to_string(FIXTURE_PATH).expect("the fixture reads");
    assert!(!fixture.is_empty(), "missing fixture {FIXTURE_PATH}");
    assert_eq!(
        saved, fixture,
        "the scene format moved; regenerate only for an intended change"
    );
}

#[test]
fn the_fixture_reloads_to_the_same_values() {
    let fixture = std::fs::read_to_string(FIXTURE_PATH).expect("the fixture reads");
    let loaded = from_ron(&fixture);
    assert!(loaded.is_ok(), "{loaded:?}");
    assert_eq!(loaded.ok(), Some(every_quantity_scene()));
}

/// One quantity leaf found by the walk: its path, sentinel, and shape.
struct Found {
    path: String,
    sentinel: f32,
    vector: bool,
}

/// Walks `value`, giving every `gradiance_units` quantity a unique sentinel.
fn plant_sentinels(value: &mut dyn PartialReflect, path: &str, found: &mut Vec<Found>) {
    let is_quantity = value
        .get_represented_type_info()
        .is_some_and(|info| info.type_path().starts_with("gradiance_units::"));
    #[expect(
        clippy::cast_precision_loss,
        reason = "a few dozen sentinels, exact in f32"
    )]
    let sentinel = 7000.5 + found.len() as f32;
    match value.reflect_mut() {
        ReflectMut::TupleStruct(ts) if is_quantity => {
            let Some(inner) = ts.field_mut(0) else { return };
            let vector = inner.try_downcast_ref::<Vec2>().is_some();
            if vector {
                let _ = inner.try_apply(&Vec2::new(sentinel, sentinel));
            } else {
                let _ = inner.try_apply(&sentinel);
            }
            found.push(Found {
                path: path.to_string(),
                sentinel,
                vector,
            });
        }
        ReflectMut::Struct(s) => {
            for i in 0..s.field_len() {
                let name = s.name_at(i).unwrap_or_default().to_string();
                if let Some(f) = s.field_at_mut(i) {
                    plant_sentinels(f, &format!("{path}.{name}"), found);
                }
            }
        }
        ReflectMut::TupleStruct(ts) => {
            for i in 0..ts.field_len() {
                if let Some(f) = ts.field_mut(i) {
                    plant_sentinels(f, &format!("{path}.{i}"), found);
                }
            }
        }
        ReflectMut::Tuple(t) => {
            for i in 0..t.field_len() {
                if let Some(f) = t.field_mut(i) {
                    plant_sentinels(f, &format!("{path}.{i}"), found);
                }
            }
        }
        ReflectMut::List(l) => {
            for i in 0..l.len() {
                if let Some(f) = l.get_mut(i) {
                    plant_sentinels(f, &format!("{path}[{i}]"), found);
                }
            }
        }
        ReflectMut::Array(a) => {
            for i in 0..a.len() {
                if let Some(f) = a.get_mut(i) {
                    plant_sentinels(f, &format!("{path}[{i}]"), found);
                }
            }
        }
        ReflectMut::Enum(e) => {
            let variant = e.variant_name().to_string();
            for i in 0..e.field_len() {
                let name = e
                    .name_at(i)
                    .map_or_else(|| i.to_string(), ToString::to_string);
                if let Some(f) = e.field_at_mut(i) {
                    plant_sentinels(f, &format!("{path}.{variant}.{name}"), found);
                }
            }
        }
        _ => {}
    }
}

/// Whether `text` contains `needle` (which starts with `(`) as an unnamed
/// tuple — not as the payload parenthesis of a named one (`Some(0.5)` is a
/// bare value inside an `Option`; `(0.5)` alone is a wrapped newtype).
fn contains_unnamed(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

#[test]
fn the_wrapped_detector_ignores_named_payloads() {
    assert!(contains_unnamed("near: (0.5),", "(0.5)"));
    assert!(contains_unnamed("Some((0.5))", "(0.5)"));
    assert!(!contains_unnamed("Some(0.5)", "(0.5)"));
    assert!(!contains_unnamed("near: 0.5,", "(0.5)"));
}

#[test]
fn every_quantity_field_in_the_save_format_is_spelled_bare() {
    let mut scene = every_quantity_scene();
    let mut found = Vec::new();
    plant_sentinels(&mut scene, "scene", &mut found);
    assert!(
        found.len() >= 3,
        "the walk found only {} quantities; is it still reaching the records?",
        found.len()
    );
    let text = to_ron(&scene).expect("the scene saves");
    let mut wrapped = Vec::new();
    for f in &found {
        let s = format!("{:?}", f.sentinel);
        assert!(text.contains(&s), "{} ({s}) was not saved", f.path);
        let wrapped_form = if f.vector {
            format!("(({s}, {s}))")
        } else {
            format!("({s})")
        };
        if contains_unnamed(&text, &wrapped_form) {
            wrapped.push(f.path.clone());
        }
    }
    assert!(
        wrapped.is_empty(),
        "quantity fields saved wrapped `(x)` -- is the type #[serde(transparent)]? {wrapped:?}"
    );
}

/// Brief 6-1 (the loader mutation test found it): a file in which two
/// objects share an id loaded, and the id index then mapped the id to one of
/// them, so a node followed the wrong body and the next frame changed the
/// open scene with no command. Such a file is refused with a typed error.
#[test]
fn a_file_with_a_shared_id_is_refused() {
    let scene = every_quantity_scene();
    assert!(scene.duplicate_id().is_none());
    assert!(from_ron(&to_ron(&scene).expect("saves")).is_ok());
    for share in [
        |s: &mut SceneRecord| s.bodies[1].id = s.bodies[0].id,
        |s: &mut SceneRecord| s.joints[0].id = s.bodies[0].id,
        |s: &mut SceneRecord| s.nodes[0].id = s.bodies[0].id,
    ] {
        let mut bad = scene.clone();
        share(&mut bad);
        let text = to_ron(&bad).expect("saves");
        assert!(
            matches!(from_ron(&text), Err(crate::PersistError::DuplicateId(id)) if id == scene.bodies[0].id),
            "a shared id must be refused"
        );
    }
}

/// Brief 6-1 (the loader mutation test found it): a NaN in a settings
/// field loaded, the settings sanitizer rewrote it a frame later, and the
/// watcher recorded a phantom `settings` step; NaN equals nothing, so undo's
/// value compares cannot settle on it either. Such a file is refused.
#[test]
fn a_file_with_a_non_finite_number_is_refused() {
    let text = to_ron(&every_quantity_scene()).expect("saves");
    for bad in ["NaN", "inf", "-inf"] {
        // The first body's x position, and a settings field.
        for (from, field) in [("pos: (", "pos"), ("gravity: (", "gravity")] {
            let at = text.find(from).expect("the field is in the file") + from.len();
            let end = at + text[at..].find(',').expect("a number");
            let broken = format!("{}{bad}{}", &text[..at], &text[end..]);
            match from_ron(&broken) {
                Err(crate::PersistError::NonFinite(path)) => {
                    assert!(path.contains(field), "{bad} in {field}: reported at {path}");
                }
                other => panic!("{bad} in {field} must be refused: {other:?}"),
            }
        }
    }
}

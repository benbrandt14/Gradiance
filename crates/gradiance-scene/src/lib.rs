//! The scene format: authored-state records, RON serialization, and
//! version migrations — the save file as a first-class module.
//!
//! [`SceneRecord`] and its per-entity records ([`BodyRecord`],
//! [`JointRecord`], [`NodeRecord`]) are the shared unit of **undo capture**
//! (the command layer stores them in undo records) and **persistence** (the
//! RON file is exactly these records on disk). Keeping the records, the
//! format version, and the migrations together means a format change is a
//! change to *one* module — the command layer consumes records, the
//! `persist` layer moves bytes, and neither owns the format.

#[cfg(test)]
mod format_golden;
mod records;
pub mod review;

pub use records::{
    AuthoredRecord, BodyRecord, EnvironmentRecord, JointRecord, NodeRecord, SceneRecord,
};

/// Scene format version accepted by this build.
///
/// v2: the de-adapter collapse — authored physics is now avian components
/// serialized directly (`docs/physics-deadapter-decision.md`). v1 files do
/// not load (save-format stability across the collapse is not a goal).
///
/// v3: the weld rework (M20) — `JointKind::Weld` no longer exists (the weld
/// tool merges bodies or makes them static), so v2 files carrying weld
/// joints do not load.
///
/// v4: the strut rework — `JointKind::Spring` is authored as `rest_length`
/// with an optional `range` clamp (was `bounds`), so v3 files carrying
/// struts do not load.
///
/// v5: continuous depth (V3) — bodies author a `DepthBand` instead of a
/// `LayerMask32`.
///
/// v6: **SI units** — the world is now metres/kilograms/newtons and stored
/// values are SI (positions, sizes, gravity, spacing all ÷100 from the prior
/// pixel world at 100 px/m). Older files are not loaded; a units-only v5→v6
/// migration (÷100, with `ShapeDef` tree scaling) is a planned follow-up
/// (`docs/units-decision.md`).
///
/// Additive within v6: `CsgOp::Xor` (body booleans, brief 4-1). Every
/// existing v6 file still loads, so the version is not bumped; a file that
/// uses xor fails to parse ("unknown variant") in a build that predates it.
///
/// v7: **typed quantities save bare** (units P2·types, brief 5-1). Authored
/// physical scalars are `gradiance-units` newtypes, which are
/// `#[serde(transparent)]`: every quantity saves as the number it holds
/// (`near: 0.5`), including the joint motor fields and `break_force` that v6
/// wrote wrapped (`target_velocity: (2.0)`). Values are unchanged; v6 files
/// are rejected, not migrated (the format is pre-release). avian's own
/// components keep avian's spelling (`density: (1.0)`).
///
/// v8: a **spring is authored mass-relative** (brief 4b-4): `frequency` (Hz)
/// and `damping_ratio` replace `stiffness` (N/m) and `damping`; the physics
/// seam derives the stiffness from the bodies' mass. v7 files are rejected.
pub const FORMAT_VERSION: u32 = 8;

/// What went wrong while serializing, parsing, or migrating a scene.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// RON serialization failure.
    #[error("serialize: {0}")]
    Serialize(#[from] ron::Error),
    /// RON parse failure.
    #[error("parse: {0}")]
    Parse(#[from] ron::de::SpannedError),
    /// The file's format version is not supported.
    #[error("unsupported scene version {0} (this build reads {FORMAT_VERSION})")]
    Version(u32),
    /// Two objects in the file share one identity. Identity is what undo,
    /// joints and nodes resolve through, so the file is refused rather than
    /// guessed at (brief 6-1, decision `duplicate-id-load`).
    #[error("two objects share the id {0}")]
    DuplicateId(gradiance_core::ids::StableId),
    /// A number in the file is NaN or infinite (the path names it). Undo,
    /// the settings watcher and the physics all compare or integrate by
    /// value, and NaN equals nothing, so the file is refused (brief 6-1,
    /// decision `non-finite-load`).
    #[error("a number is not finite at {0}")]
    NonFinite(String),
}

/// Serializes a scene to pretty RON (deterministic for identical scenes).
pub fn to_ron(scene: &SceneRecord) -> Result<String, PersistError> {
    Ok(ron::ser::to_string_pretty(
        scene,
        ron::ser::PrettyConfig::new(),
    )?)
}

/// Parses and version-checks a scene. Only the current [`FORMAT_VERSION`] is
/// accepted; older files are rejected (a units-only v5→v6 migration is a
/// planned follow-up — see the version history above). A file in which two
/// objects (bodies, joints, nodes) share a [`StableId`] is refused
/// ([`PersistError::DuplicateId`]).
///
/// [`StableId`]: gradiance_core::ids::StableId
pub fn from_ron(text: &str) -> Result<SceneRecord, PersistError> {
    let scene: SceneRecord = ron::from_str(text)?;
    if scene.version != FORMAT_VERSION {
        return Err(PersistError::Version(scene.version));
    }
    if let Some(id) = scene.duplicate_id() {
        return Err(PersistError::DuplicateId(id));
    }
    if let Some(path) = non_finite(&scene, "scene") {
        return Err(PersistError::NonFinite(path));
    }
    Ok(scene)
}

/// The path of the first NaN or infinite `f32`/`f64` in `value`, walked by
/// reflection (so a field a later brief adds is covered with no change).
fn non_finite(value: &dyn bevy::reflect::PartialReflect, path: &str) -> Option<String> {
    use bevy::reflect::ReflectRef;
    let field = |v: &dyn bevy::reflect::PartialReflect, at: String| non_finite(v, &at);
    match value.reflect_ref() {
        ReflectRef::Struct(s) => (0..s.field_len()).find_map(|i| {
            let name = s.name_at(i).unwrap_or_default();
            field(s.field_at(i)?, format!("{path}.{name}"))
        }),
        ReflectRef::TupleStruct(t) => {
            (0..t.field_len()).find_map(|i| field(t.field(i)?, format!("{path}.{i}")))
        }
        ReflectRef::Tuple(t) => {
            (0..t.field_len()).find_map(|i| field(t.field(i)?, format!("{path}.{i}")))
        }
        ReflectRef::List(l) => l
            .iter()
            .enumerate()
            .find_map(|(i, v)| field(v, format!("{path}[{i}]"))),
        ReflectRef::Array(a) => a
            .iter()
            .enumerate()
            .find_map(|(i, v)| field(v, format!("{path}[{i}]"))),
        ReflectRef::Map(m) => m
            .iter()
            .find_map(|(k, v)| field(v, format!("{path}[{k:?}]"))),
        ReflectRef::Set(s) => s.iter().find_map(|v| field(v, format!("{path}{{}}"))),
        ReflectRef::Enum(e) => (0..e.field_len()).find_map(|i| {
            let name = e
                .name_at(i)
                .map_or_else(|| i.to_string(), ToOwned::to_owned);
            field(
                e.field_at(i)?,
                format!("{path}.{}.{name}", e.variant_name()),
            )
        }),
        ReflectRef::Opaque(o) => {
            let finite = o
                .try_downcast_ref::<f32>()
                .map(|x| x.is_finite())
                .or_else(|| o.try_downcast_ref::<f64>().map(|x| x.is_finite()))
                .unwrap_or(true);
            (!finite).then(|| path.to_owned())
        }
    }
}

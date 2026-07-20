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

mod records;

pub use records::{
    AuthoredRecord, BodyRecord, EnvironmentRecord, JointRecord, NodeRecord, SceneRecord,
};

use bevy::prelude::*;

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
/// `LayerMask32`. v4 files migrate on load: each mask's occupied bit range
/// maps to the equivalent band; non-default *filters* are dropped with a
/// warning (checkbox filter art is unrepresentable by design — collision
/// is depth overlap).
pub const FORMAT_VERSION: u32 = 5;

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
}

/// Serializes a scene to pretty RON (deterministic for identical scenes).
pub fn to_ron(scene: &SceneRecord) -> Result<String, PersistError> {
    Ok(ron::ser::to_string_pretty(
        scene,
        ron::ser::PrettyConfig::new(),
    )?)
}

/// Parses and version-checks a scene, migrating supported old versions.
pub fn from_ron(text: &str) -> Result<SceneRecord, PersistError> {
    let mut scene: SceneRecord = ron::from_str(text)?;
    match scene.version {
        FORMAT_VERSION => Ok(scene),
        4 => {
            migrate_v4_layers(&mut scene);
            Ok(scene)
        }
        v => Err(PersistError::Version(v)),
    }
}

/// v4 → v5: each body's legacy layer mask becomes the equivalent depth
/// band. Custom filter masks cannot be represented (collision is depth
/// overlap now) and are dropped with a warning.
fn migrate_v4_layers(scene: &mut SceneRecord) {
    use gradiance_domain::depth::DepthBand;
    for body in &mut scene.bodies {
        if let Some(mask) = body.layers.take() {
            body.depth = mask
                .occupied_range()
                .map_or_else(DepthBand::default, |(min, max)| {
                    DepthBand::from_bit_range(min, max)
                });
            if mask.filters != u32::MAX {
                warn!(
                    id = %body.id.0,
                    "v4 custom collision filters dropped on migration \
                     (collision is depth overlap in v5)"
                );
            }
        }
    }
    scene.version = FORMAT_VERSION;
}

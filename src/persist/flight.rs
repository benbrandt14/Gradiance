//! Dev-only RON dump of the flight recorder — **F9** writes the ring buffer
//! to `snapshots/flight-<timestamp>.ron` (pair with an F12 scene snapshot in
//! bug reports). Lives here because the boundary test confines `Serialize`
//! to persistence.

use crate::command::flight_recorder::FlightRecorder;
use bevy::prelude::*;
use bevy::reflect::TypeRegistry;
use bevy::reflect::serde::ReflectSerializer;
use serde::Serialize;
use std::path::PathBuf;

/// One dumped frame (only frames that did something are written).
#[derive(Serialize)]
struct DumpFrame {
    /// Recorder frame counter (gaps = idle frames).
    frame: u64,
    /// Drained intents: `(type path, reflection-serialized value)`.
    intents: Vec<(String, String)>,
    /// Executed commands: `(name, debug detail, outcome)`.
    commands: Vec<(String, String, String)>,
    /// Sync systems that fired: `(system, entities matched)`.
    syncs: Vec<(String, usize)>,
}

/// The dump document.
#[derive(Serialize)]
struct Dump {
    frames: Vec<DumpFrame>,
}

/// One reflected intent as RON (reflect-`Debug` fallback); no per-intent code.
fn intent_ron(value: &dyn bevy::reflect::PartialReflect, registry: &TypeRegistry) -> String {
    ron::ser::to_string(&ReflectSerializer::new(value, registry))
        .unwrap_or_else(|_| format!("{value:?}"))
}

/// Builds the dump document from the recorder's ring buffer.
fn build_dump(recorder: &FlightRecorder, registry: &TypeRegistry) -> Dump {
    Dump {
        frames: recorder
            .frames()
            .filter(|f| !f.is_empty())
            .map(|f| DumpFrame {
                frame: f.frame,
                intents: f
                    .intents
                    .iter()
                    .map(|i| {
                        (
                            i.type_path.to_string(),
                            intent_ron(i.value.as_ref(), registry),
                        )
                    })
                    .collect(),
                commands: f
                    .commands
                    .iter()
                    .map(|c| {
                        let outcome = match &c.outcome {
                            Ok(()) => "ok".to_string(),
                            Err(e) => e.clone(),
                        };
                        (c.name.to_string(), c.detail.clone(), outcome)
                    })
                    .collect(),
                syncs: f
                    .syncs
                    .iter()
                    .map(|(name, count)| ((*name).to_string(), *count))
                    .collect(),
            })
            .collect(),
    }
}

/// F9: dumps the flight recorder to `snapshots/flight-<timestamp>.ron`.
pub fn dump_flight_recorder(
    keys: Res<ButtonInput<KeyCode>>,
    recorder: Res<FlightRecorder>,
    registry: Res<AppTypeRegistry>,
) {
    if !keys.just_pressed(KeyCode::F9) {
        return;
    }
    let dump = build_dump(&recorder, &registry.read());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dir = PathBuf::from("snapshots");
    let path = dir.join(format!("flight-{stamp}.ron"));
    let result = std::fs::create_dir_all(&dir)
        .map_err(crate::scene::PersistError::from)
        .and_then(|()| {
            let text = ron::ser::to_string_pretty(&dump, ron::ser::PrettyConfig::new())?;
            Ok(std::fs::write(&path, text)?)
        });
    match result {
        Ok(()) => {
            info!(path = %path.display(), frames = dump.frames.len(), "flight recorder dumped")
        }
        Err(e) => warn!(error = %e, "flight recorder dump failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::intent::CutIntent;
    use bevy::reflect::TypePath;

    #[test]
    fn dump_serializes_recorded_intents() {
        let mut recorder = FlightRecorder::default();
        let intent = CutIntent {
            a: Vec2::ZERO,
            b: Vec2::new(10.0, 0.0),
            width: 4.0,
        };
        recorder.record_intent(CutIntent::type_path(), intent.as_partial_reflect());
        recorder.record_command("cut", format!("{intent:?}"), Ok(()));

        let mut registry = TypeRegistry::default();
        registry.register::<CutIntent>();
        registry.register::<Vec2>();
        let dump = build_dump(&recorder, &registry);
        assert_eq!(dump.frames.len(), 1);
        let (path, value) = &dump.frames[0].intents[0];
        assert!(path.ends_with("CutIntent"));
        assert!(value.contains("width"), "value carries fields: {value}");
        let text = ron::ser::to_string_pretty(&dump, ron::ser::PrettyConfig::new())
            .expect("dump serializes");
        assert!(text.contains("cut"));
    }
}

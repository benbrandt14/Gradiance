//! Scene loader mutation tests (brief 6-1): every committed scene file,
//! mutated by a seeded generator, loaded through the real load path
//! (`scene.load`, the operation File ▸ Open and Ctrl+O invoke: it pre-reads
//! the file, then `LoadSceneRequest` → `LoadSceneIntent` →
//! `LoadSceneCommand`).
//!
//! For every case: nothing panics (the load, and a few played frames of
//! what loaded); the result is a typed error or a loaded scene; a failed
//! load leaves the open scene and the history exactly as they were; a
//! loaded scene is exactly the file's records, as one undo step.

use crate::harness::{headless_app, step};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::core::ids::StableId;
use gradiance::prelude::GameState;
use gradiance::scene::{SceneRecord, from_ron, to_ron};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Where the committed scene files live (relative to the package root).
/// Review scenes (`docs/review/scenes/`) are `.scm` scripts, not scene
/// files, so the loader never reads them.
const SCENE_DIRS: &[&str] = &[
    "tests/fixtures/scenes",
    "tests/it/golden",
    "crates/gradiance-scene/golden",
    "snapshots",
];

/// How many seeded cases run (the brief asks for at least 200).
const CASES: u64 = 256;

/// The mutations, one per case in turn.
#[derive(Debug, Clone, Copy)]
enum Mutation {
    Truncate,
    DropField,
    DuplicateId,
    DanglingJoint,
    NonFinite,
    WrongVersion,
    SwapEnumTag,
    ByteFlips,
}

const MUTATIONS: [Mutation; 8] = [
    Mutation::Truncate,
    Mutation::DropField,
    Mutation::DuplicateId,
    Mutation::DanglingJoint,
    Mutation::NonFinite,
    Mutation::WrongVersion,
    Mutation::SwapEnumTag,
    Mutation::ByteFlips,
];

/// SplitMix64: a seeded generator with no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n.max(1) as u64).unwrap_or(0)
    }

    fn id(&mut self) -> StableId {
        StableId(uuid::Uuid::from_u64_pair(self.next(), self.next()))
    }
}

/// Every committed `.ron` scene file, sorted by path, with its bytes.
fn sources() -> Vec<(PathBuf, Vec<u8>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in SCENE_DIRS {
        let entries = std::fs::read_dir(root.join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}"));
        for entry in entries {
            let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
            if path.extension().is_some_and(|e| e == "ron") {
                let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{e}"));
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

/// The byte offsets of every line start.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Removes one `name: value` field (its whole value, however many lines).
fn drop_field(text: &str, rng: &mut Rng) -> String {
    let lines = lines(text);
    let fields: Vec<usize> = (0..lines.len())
        .filter(|&i| {
            let t = lines[i].trim_start();
            t.split_once(": ").is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
        .collect();
    if fields.is_empty() {
        return text.to_owned();
    }
    let start = fields[rng.below(fields.len())];
    let mut depth = 0_i32;
    let mut end = start;
    for (i, line) in lines.iter().enumerate().skip(start) {
        for c in line.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        end = i;
        if depth <= 0 {
            break;
        }
    }
    let mut out: String = lines[..start].concat();
    out.push_str(&lines[end + 1..].concat());
    out
}

/// Every float literal's byte range (`-?digits.digits[e±digits]`).
fn float_literals(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let starts = bytes[i].is_ascii_digit()
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-'));
        let negative = bytes[i] == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
        if !(starts || negative) {
            i += 1;
            continue;
        }
        let begin = i;
        i += 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'.' | b'e' | b'-'))
        {
            i += 1;
        }
        if text[begin..i].contains('.') {
            out.push((begin, i));
        }
    }
    out
}

/// Replaces one float literal with a non-finite (or out-of-range) number.
fn non_finite(text: &str, rng: &mut Rng) -> String {
    let floats = float_literals(text);
    if floats.is_empty() {
        return text.to_owned();
    }
    let (a, b) = floats[rng.below(floats.len())];
    let with = ["NaN", "inf", "-inf", "1e39", "-1e39"][rng.below(5)];
    format!("{}{with}{}", &text[..a], &text[b..])
}

/// Swaps one occurrence of a capitalised identifier (an enum tag or a
/// variant) for another one the file uses.
fn swap_tag(text: &str, rng: &mut Rng) -> String {
    let mut tags: Vec<(usize, usize)> = Vec::new();
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_string = !in_string;
        }
        if !in_string
            && bytes[i].is_ascii_uppercase()
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
        {
            let begin = i;
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            tags.push((begin, i));
            continue;
        }
        i += 1;
    }
    let mut names: Vec<&str> = tags.iter().map(|&(a, b)| &text[a..b]).collect();
    names.sort_unstable();
    names.dedup();
    if names.len() < 2 {
        return text.to_owned();
    }
    let (a, b) = tags[rng.below(tags.len())];
    let old = &text[a..b];
    let others: Vec<&&str> = names.iter().filter(|n| **n != old).collect();
    let new = others[rng.below(others.len())];
    format!("{}{new}{}", &text[..a], &text[b..])
}

/// The scene's records changed so that two objects share a `StableId`.
fn duplicate_id(scene: &mut SceneRecord, rng: &mut Rng) {
    let mut ids: Vec<StableId> = scene.bodies.iter().map(|b| b.id).collect();
    ids.extend(scene.joints.iter().map(|j| j.id));
    ids.extend(scene.nodes.iter().map(|n| n.id));
    if ids.len() < 2 {
        return;
    }
    let from = ids[rng.below(ids.len())];
    let to = rng.below(ids.len());
    let mut slot = 0;
    for id in scene
        .bodies
        .iter_mut()
        .map(|b| &mut b.id)
        .chain(scene.joints.iter_mut().map(|j| &mut j.id))
        .chain(scene.nodes.iter_mut().map(|n| &mut n.id))
    {
        if slot == to {
            *id = if *id == from {
                ids[(to + 1) % ids.len()]
            } else {
                from
            };
        }
        slot += 1;
    }
}

/// The scene's records changed so that a joint (or a node) names a body the
/// file does not hold. A file with no joint gets the pendulum's, whose body
/// it does not hold either.
fn dangling_joint(scene: &mut SceneRecord, pendulum: &SceneRecord, rng: &mut Rng) {
    if scene.joints.is_empty() {
        scene.joints.extend(pendulum.joints.first().cloned());
        return;
    }
    let unknown = rng.id();
    let n = scene.joints.len() + scene.nodes.len();
    let pick = rng.below(n);
    if let Some(joint) = scene.joints.get_mut(pick) {
        if rng.below(2) == 0 {
            joint.def.body_a = unknown;
        } else {
            joint.def.body_b = Some(unknown);
        }
    } else if let Some(node) = scene.nodes.get_mut(pick - scene.joints.len()) {
        node.attachment.target = Some(unknown);
    }
}

/// Applies `mutation` to `bytes` with `rng`.
fn mutate(bytes: &[u8], mutation: Mutation, rng: &mut Rng, pendulum: &SceneRecord) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes).into_owned();
    let structured = |f: &mut dyn FnMut(&mut SceneRecord)| -> Option<Vec<u8>> {
        let mut scene = from_ron(&text).ok()?;
        f(&mut scene);
        to_ron(&scene).ok().map(String::into_bytes)
    };
    match mutation {
        Mutation::Truncate => {
            let mut cut = rng.below(bytes.len());
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text[..cut].as_bytes().to_vec()
        }
        Mutation::DropField => drop_field(&text, rng).into_bytes(),
        Mutation::NonFinite => non_finite(&text, rng).into_bytes(),
        Mutation::WrongVersion => {
            let to = [0_u64, 7, 9, 99, u64::from(u32::MAX), u64::MAX][rng.below(6)];
            text.replacen(
                &format!("version: {}", gradiance::scene::FORMAT_VERSION),
                &format!("version: {to}"),
                1,
            )
            .into_bytes()
        }
        Mutation::SwapEnumTag => swap_tag(&text, rng).into_bytes(),
        Mutation::ByteFlips => {
            let mut out = bytes.to_vec();
            for _ in 0..=rng.below(4) {
                let at = rng.below(out.len());
                out[at] ^= 1 << rng.below(8);
            }
            out
        }
        Mutation::DuplicateId => structured(&mut |s| duplicate_id(s, rng))
            .unwrap_or_else(|| non_finite(&text, rng).into_bytes()),
        Mutation::DanglingJoint => structured(&mut |s| dangling_joint(s, pendulum, rng))
            .unwrap_or_else(|| drop_field(&text, rng).into_bytes()),
    }
}

/// The open scene, as a save writes it, minus the app version.
fn open_scene(app: &mut App) -> String {
    let mut scene = SceneRecord::capture(app.world_mut());
    scene.app_version.clear();
    to_ron(&scene).unwrap_or_else(|e| panic!("{e}"))
}

/// What the open scene must be once `scene` has loaded: the records
/// ordered by id, as a capture orders them.
fn as_loaded(mut scene: SceneRecord) -> String {
    scene.app_version.clear();
    scene.bodies.sort_by_key(|b| b.id.0);
    scene.joints.sort_by_key(|j| j.id.0);
    scene.nodes.sort_by_key(|n| n.id.0);
    for body in &mut scene.bodies {
        body.layers = None;
    }
    to_ron(&scene).unwrap_or_else(|e| panic!("{e}"))
}

/// The first line where `a` and `b` differ, for a readable failure.
fn first_line_difference(a: &str, b: &str) -> String {
    a.lines()
        .zip(b.lines())
        .enumerate()
        .find(|(_, (x, y))| x != y)
        .map_or_else(
            || {
                format!(
                    "lengths {} vs {} lines",
                    a.lines().count(),
                    b.lines().count()
                )
            },
            |(i, (x, y))| format!("line {}: `{}` vs `{}`", i + 1, x.trim(), y.trim()),
        )
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

/// The mutation test: [`CASES`] seeded cases over every committed scene
/// file, each loaded through `scene.load` into one app that keeps the scene
/// the last good case loaded.
#[test]
fn mutated_scene_files_load_or_fail_cleanly() {
    let files = sources();
    assert!(files.len() >= 6, "found only {} scene files", files.len());
    let pendulum = files
        .iter()
        .find(|(p, _)| p.ends_with("pendulum.ron"))
        .and_then(|(_, b)| from_ron(&String::from_utf8_lossy(b)).ok())
        .unwrap_or_else(|| panic!("the pendulum fixture loads"));
    let dir = std::env::temp_dir().join(format!("gradiance-6-1-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));

    let mut app = headless_app();
    set_state(&mut app, GameState::Paused);
    let (mut loaded, mut failed) = (0, 0);
    for case in 0..CASES {
        let (source, bytes) = &files[usize::try_from(case).unwrap_or(0) % files.len()];
        let mutation =
            MUTATIONS[usize::try_from(case / files.len() as u64).unwrap_or(0) % MUTATIONS.len()];
        let mut rng = Rng(case.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x61);
        let mutated = mutate(bytes, mutation, &mut rng, &pendulum);
        let name = format!(
            "case {case}: {mutation:?} of {}",
            source
                .file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default()
        );
        let path = dir.join(format!("case-{case}.ron"));
        std::fs::write(&path, &mutated).unwrap_or_else(|e| panic!("{e}"));

        let before = open_scene(&mut app);
        let depth = undo_depth(&app);
        // The typed answer the file gets: a parse or version error, or a
        // record (which the command may still refuse: a bad shape).
        let parsed = std::str::from_utf8(&mutated)
            .map_err(|e| e.to_string())
            .and_then(|t| from_ron(t).map_err(|e| e.to_string()));
        let call = gradiance::op::invoke(
            app.world_mut(),
            "scene.load",
            json!({"path": path.to_string_lossy()}),
        );
        step(&mut app, 2);
        let after = open_scene(&mut app);
        match parsed {
            Err(why) => {
                assert!(call.is_err(), "{name}: `{why}`, but scene.load accepted it");
                assert_eq!(
                    after, before,
                    "{name}: a failed load changed the scene ({why})"
                );
                assert_eq!(
                    undo_depth(&app),
                    depth,
                    "{name}: a failed load recorded a step"
                );
                failed += 1;
            }
            Ok(scene) => {
                assert!(
                    call.is_ok(),
                    "{name}: parses, but scene.load refused: {call:?}"
                );
                if undo_depth(&app) == depth {
                    // The command refused it (a typed `CommandError`).
                    assert_eq!(after, before, "{name}: a refused load changed the scene");
                    failed += 1;
                } else {
                    let want = as_loaded(scene);
                    assert!(
                        undo_depth(&app) == depth + 1 && after == want,
                        "{name}: not one load-scene step ({} -> {}, {:?}) or not the file's scene: {}",
                        depth,
                        undo_depth(&app),
                        app.world().resource::<HistoryInfo>().undo_label,
                        first_line_difference(&after, &want),
                    );
                    // Whatever loaded must also run.
                    set_state(&mut app, GameState::Playing);
                    step(&mut app, 4);
                    set_state(&mut app, GameState::Paused);
                    loaded += 1;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        loaded > 0 && failed > 0,
        "loaded {loaded}, failed {failed}: the mix is broken"
    );
}

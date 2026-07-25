//! Architecture boundary tests.
//!
//! Since the workspace split, the layer boundaries are primarily enforced
//! by the package graph itself: a crate can only import what its
//! `Cargo.toml` declares, so `egui` cannot leave `gradiance-ui` and `steel`
//! cannot leave `gradiance-script` without a manifest diff. These tests are
//! the second line: they scan the source as text so that even a
//! *manifest* drift (someone adding `egui` to another crate's
//! dependencies) is caught in review by a failing test, and they hold
//! rules the package graph cannot express (serde confinement, exact pins).

// Test-only file: panics are the failure mechanism (clippy's
// allow-*-in-tests config does not extend to integration-test helpers).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

/// Repo root (the workspace root; this test crate is the root package).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Recursively collect all files under `dir` with extension `ext`.
fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files.extend(files_with_ext(&path, ext));
        } else if path.extension().is_some_and(|e| e == ext) {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// All Rust sources of every workspace package (members under `crates/`
/// plus the root package's `src/`).
fn workspace_sources() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = files_with_ext(&root.join("src"), "rs");
    files.extend(files_with_ext(&root.join("crates"), "rs"));
    files
}

/// Return `(path, line_number, line)` for every line matching `needle`,
/// excluding files whose repo-relative path starts with one of `allowed`.
fn violations(needle: &str, allowed: &[&str]) -> Vec<String> {
    let root = repo_root();
    let mut found = Vec::new();
    for file in workspace_sources() {
        let rel = file
            .strip_prefix(&root)
            .expect("path under repo root")
            .to_string_lossy()
            .replace('\\', "/");
        if allowed.iter().any(|a| rel.starts_with(a)) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("readable source file");
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Comments may mention foreign crates; only code counts.
            if trimmed.starts_with("//") {
                continue;
            }
            // Strip a trailing line comment before matching.
            let code = trimmed.split("//").next().unwrap_or(trimmed);
            if code.contains(needle) {
                found.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    found
}

#[test]
fn egui_is_confined_to_the_ui_layer() {
    let v = violations("egui", &["crates/gradiance-ui/src/"]);
    assert!(
        v.is_empty(),
        "egui may only be referenced from crates/gradiance-ui/:\n{}",
        v.join("\n")
    );
}

#[test]
fn steel_is_confined_to_the_script_layer() {
    let v = violations("steel", &["crates/gradiance-script/src/"]);
    assert!(
        v.is_empty(),
        "the steel scripting engine may only be referenced from \
         crates/gradiance-script/ (Tier-A authoring seam):\n{}",
        v.join("\n")
    );
}

#[test]
fn command_stack_is_only_driven_by_the_command_module() {
    let v = violations("CommandStack", &["crates/gradiance-command/src/"]);
    assert!(
        v.is_empty(),
        "CommandStack may only be touched inside crates/gradiance-command/ \
         (dispatch is the choke point):\n{}",
        v.join("\n")
    );
}

#[test]
fn engine_facing_dependencies_stay_exact_pinned() {
    // Agent sessions must not drift engine APIs: bevy-adjacent crates are
    // exact-pinned (`=x.y.z`) in the workspace dependency table. Member
    // manifests only say `workspace = true`, so the root table is the one
    // place a pin can drift.
    let manifest =
        std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml");
    let pinned = [
        "bevy",
        "avian2d",
        "bevy_egui",
        "leafwing-input-manager",
        "steel-core",
        "egui-snarl",
        "egui_tiles",
        "egui_kittest",
    ];
    for name in pinned {
        let line = manifest
            .lines()
            .find(|l| {
                let l = l.trim_start();
                l.starts_with(&format!("{name} ")) || l.starts_with(&format!("{name}="))
            })
            .unwrap_or_else(|| panic!("{name} not found in Cargo.toml"));
        assert!(
            line.contains("\"="),
            "{name} must stay exact-pinned (`=x.y.z`), found: {line}"
        );
    }
}

#[test]
fn serialization_is_confined_to_authored_data() {
    let allowed = [
        "crates/gradiance-domain/src/",
        "crates/gradiance-core/src/",
        "crates/gradiance-geometry/src/shape.rs",
        "crates/gradiance-units/src/",
        "crates/gradiance-scene/src/",
        "crates/gradiance-persist/src/",
        // The sketch document is authored state: it is what the person drew
        // plus the relationships they asked for, and it rides in the save file
        // alongside the body it produced. The solver's own handles are
        // ephemeral and never serialized.
        "crates/gradiance-sketch/src/doc.rs",
    ];
    let v = violations("Serialize", &allowed);
    assert!(
        v.is_empty(),
        "Serde derives are only allowed on authored/persisted data \
         (domain, core, the shape tree, typed quantities, scene records, \
         persist, the sketch document):\n{}",
        v.join("\n")
    );
}

/// The `gradiance-*` dependency names a member manifest declares.
fn member_gradiance_deps(manifest: &str) -> Vec<String> {
    let mut deps: Vec<String> = manifest
        .lines()
        .filter_map(|l| {
            let l = l.trim_start();
            l.strip_prefix("gradiance-")
                .map(|rest| rest.split(['.', ' ', '=']).next().unwrap_or("").to_owned())
        })
        .collect();
    deps.sort();
    deps.dedup();
    deps
}

#[test]
fn the_crate_dag_matches_the_architecture() {
    // The layer diagram from CLAUDE.md, as data. Cargo already enforces
    // acyclicity and that *undeclared* edges do not compile; this test
    // enforces the absence of legal-but-unwanted edges. Adding a
    // `gradiance-*` dependency to a member manifest must come with a
    // matching row change here — a deliberate, reviewed architecture
    // decision, never a convenience side effect.
    let allowed: &[(&str, &[&str])] = &[
        (
            "command",
            &["core", "domain", "geometry", "scene", "signal"],
        ),
        ("core", &[]),
        ("domain", &["core", "geometry", "units"]),
        ("geometry", &["core"]),
        (
            "interaction",
            &[
                "command", "core", "domain", "geometry", "persist", "physics", "scene",
            ],
        ),
        ("kernel", &[]),
        ("persist", &["command", "core", "scene"]),
        ("physics", &["core", "domain", "geometry", "units"]),
        (
            "render",
            &[
                "core",
                "domain",
                "geometry",
                "interaction",
                "physics",
                "signal",
                "units",
            ],
        ),
        ("scene", &["core", "domain"]),
        (
            "script",
            &["command", "core", "domain", "geometry", "scene", "signal"],
        ),
        ("signal", &["core", "domain", "kernel", "physics"]),
        // Sketching is an authoring-time subsystem: geometry in, geometry out.
        // The absence of `physics` here is the point — the constraint solver
        // must never reach a `Transform`, a joint, or an avian component, and
        // that stays a compile error rather than a review note.
        ("sketch", &["core", "geometry"]),
        (
            "ui",
            &[
                "command",
                "core",
                "domain",
                "geometry",
                "interaction",
                "persist",
                "physics",
                "scene",
                "script",
                "signal",
                "units",
            ],
        ),
        ("units", &[]),
    ];

    let crates_dir = repo_root().join("crates");
    let mut members: Vec<String> = std::fs::read_dir(&crates_dir)
        .expect("read crates/")
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter_map(|n| n.strip_prefix("gradiance-").map(str::to_owned))
        .collect();
    members.sort();
    let expected: Vec<String> = allowed.iter().map(|(n, _)| (*n).to_owned()).collect();
    assert_eq!(
        members, expected,
        "workspace members changed — update the DAG table (and CLAUDE.md)"
    );

    for (name, edges) in allowed {
        let manifest_path = crates_dir
            .join(format!("gradiance-{name}"))
            .join("Cargo.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
        let declared = member_gradiance_deps(&manifest);
        let expected: Vec<String> = edges.iter().map(|e| (*e).to_owned()).collect();
        assert_eq!(
            declared, expected,
            "gradiance-{name}'s dependency edges drifted from the architecture \
             (CLAUDE.md layer diagram); change this table only as a deliberate \
             architecture decision"
        );
    }
}

#[test]
fn ui_and_script_stacks_stay_confined_in_manifests() {
    // The source-text scans above catch imports; this catches the manifest
    // half — declaring the dependency at all. The root manifest is exempt:
    // its [workspace.dependencies] table *defines* the pins, and its
    // dev-dependencies drive the whole app in the integration suite.
    let confined: &[(&str, &str)] = &[
        ("bevy_egui", "gradiance-ui"),
        ("egui-snarl", "gradiance-ui"),
        ("egui_tiles", "gradiance-ui"),
        ("egui_kittest", "gradiance-ui"),
        ("steel-core", "gradiance-script"),
    ];
    let crates_dir = repo_root().join("crates");
    for entry in std::fs::read_dir(&crates_dir).expect("read crates/") {
        let dir = entry.expect("dir entry").path();
        let name = dir
            .file_name()
            .expect("crate dir")
            .to_string_lossy()
            .into_owned();
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("read {name}/Cargo.toml: {e}"));
        for (dep, home) in confined {
            if name != *home {
                let declares = manifest.lines().any(|l| {
                    let l = l.trim_start();
                    l.starts_with(&format!("{dep} "))
                        || l.starts_with(&format!("{dep}."))
                        || l.starts_with(&format!("{dep}="))
                });
                assert!(
                    !declares,
                    "{name} declares `{dep}` — that stack is confined to {home}"
                );
            }
        }
    }
}

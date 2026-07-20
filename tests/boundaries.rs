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
        "crates/gradiance-scene/src/",
        "crates/gradiance-persist/src/",
    ];
    let v = violations("Serialize", &allowed);
    assert!(
        v.is_empty(),
        "Serde derives are only allowed on authored/persisted data \
         (domain, core, the shape tree, scene records, persist):\n{}",
        v.join("\n")
    );
}

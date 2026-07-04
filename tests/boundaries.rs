//! Architecture boundary tests.
//!
//! These scan `src/` as text and fail when a module reaches across a layer
//! boundary. They are the local mirror of the CI "Architecture boundaries"
//! step and the mechanical enforcement of the invariants in `CLAUDE.md`.

use std::path::{Path, PathBuf};

/// Recursively collect all `.rs` files under `dir`.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}"));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// Return `(path, line_number, line)` for every line in `src/` matching `needle`,
/// excluding files whose path (relative to the repo root) starts with one of `allowed`.
fn violations(needle: &str, allowed: &[&str]) -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for file in rust_files(&src) {
        let rel = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .expect("path under manifest dir")
            .to_string_lossy()
            .replace('\\', "/");
        if allowed.iter().any(|a| rel.starts_with(a)) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("readable source file");
        for (i, line) in text.lines().enumerate() {
            if line.contains(needle) {
                found.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    found
}

#[test]
fn avian_is_confined_to_the_physics_seam() {
    let v = violations("avian2d", &["src/physics/"]);
    assert!(
        v.is_empty(),
        "avian2d may only be referenced from src/physics/ (engine-swap seam):\n{}",
        v.join("\n")
    );
}

#[test]
fn egui_is_confined_to_the_ui_layer() {
    let v = violations("egui", &["src/ui/"]);
    assert!(
        v.is_empty(),
        "egui may only be referenced from src/ui/:\n{}",
        v.join("\n")
    );
}

#[test]
fn command_stack_is_only_driven_by_the_command_module() {
    let v = violations("CommandStack", &["src/command/"]);
    assert!(
        v.is_empty(),
        "CommandStack may only be touched inside src/command/ (dispatch is the choke point):\n{}",
        v.join("\n")
    );
}

#[test]
fn serialization_is_confined_to_authored_data() {
    let allowed = [
        "src/domain/",
        "src/core/",
        "src/command/snapshot.rs",
        "src/persist/",
    ];
    let v = violations("Serialize", &allowed);
    assert!(
        v.is_empty(),
        "Serde derives are only allowed on authored/persisted data (domain, core, snapshot, persist):\n{}",
        v.join("\n")
    );
}

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
        "egui_plot",
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
        // The graph IR is scene content from G-2 on (brief G-1).
        "crates/gradiance-graph/src/",
        "crates/gradiance-units/src/",
        "crates/gradiance-scene/src/",
        "crates/gradiance-persist/src/",
    ];
    let v = violations("Serialize", &allowed);
    assert!(
        v.is_empty(),
        "Serde derives are only allowed on authored/persisted data \
         (domain, core, the shape tree, the graph IR, typed quantities, scene records, persist):\n{}",
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
            // units: commands rewire typed joint anchors / rest angles (5-1).
            // op: the scene operations register here (U-1).
            &[
                "core", "domain", "geometry", "op", "scene", "signal", "units",
            ],
        ),
        ("core", &[]),
        ("domain", &["core", "geometry", "units"]),
        ("geometry", &["core"]),
        // The one graph IR (brief G-1, docs/graph-model-decision.md): pure,
        // above the shape tree and the signal types it lowers to.
        ("graph", &["core", "domain"]),
        (
            "interaction",
            &[
                "command", "core", "domain", "geometry",
                // op: keys invoke registry operations by id, and this layer
                // publishes the selection mirror (U-2a).
                "op", "optimize", "persist", "physics", "scene",
                // signal: held graph-input keys are published on the bus
                // here, where the keyboard gate lives (brief N-2).
                "signal",
                // Typed joint limits / spring parameters (units P2·types,
                // brief 5-1): units is a bottom node any layer may use.
                "units",
            ],
        ),
        ("kernel", &[]),
        // The operation registry (U-1): above core only.
        ("op", &["core"]),
        ("optimize", &["core", "geometry"]),
        (
            "persist",
            // op: the scene-file operations (`scene.save`, `scene.load`,
            // `scene.dump`) register where the disk is (R-1).
            &["command", "core", "op", "scene"],
        ),
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
            // units: the spring verb builds typed spring parameters (5-1).
            // op: verbs are generated from the operation registry (U-1).
            &[
                "command", "core", "domain", "geometry", "op", "scene", "signal", "units",
            ],
        ),
        ("signal", &["core", "domain", "kernel", "physics"]),
        (
            "ui",
            &[
                "command",
                "core",
                "domain",
                "geometry",
                "interaction",
                // op: menu rows invoke registry operations by id (U-1).
                "op",
                "optimize",
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
        ("egui_plot", "gradiance-ui"),
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

#[test]
fn the_toolchain_pin_is_one_version() {
    // `rust-toolchain.toml` selects the compiler for local and agent builds;
    // `RUST_PIN` selects it for CI. Pedantic lints are errors under
    // `-D warnings`, so the two must name the same release or a local green
    // gate says nothing about CI.
    let quoted_after = |file: &str, key: &str| -> String {
        let text = std::fs::read_to_string(repo_root().join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with(key))
            .unwrap_or_else(|| panic!("{file} has no `{key}` line"));
        line.split('"')
            .nth(1)
            .unwrap_or_else(|| panic!("{file}: `{key}` has no quoted value"))
            .to_owned()
    };
    assert_eq!(
        quoted_after("rust-toolchain.toml", "channel"),
        quoted_after(".github/workflows/ci.yml", "RUST_PIN:"),
        "rust-toolchain.toml `channel` and ci.yml `RUST_PIN` must be equal"
    );
}

/// The colour constructors a literal scene or chrome colour would be
/// written with (bevy, the domain `Rgba`, egui's `Color32`).
const COLOUR_CONSTRUCTORS: &[&str] = &[
    "Color::srgb(",
    "Color::srgba(",
    "Color::srgb_u8(",
    "Color::srgba_u8(",
    "Color::linear_rgb(",
    "Color::linear_rgba(",
    "Color::hsl(",
    "Color::hsla(",
    "Color::hsv(",
    "Color::oklab(",
    "Color::oklch(",
    "LinearRgba::new(",
    "LinearRgba::rgb(",
    "Srgba::new(",
    "Srgba::rgb(",
    "Rgba::rgb(",
    "Rgba::from_hsl(",
    "Color32::from_rgb(",
    "Color32::from_rgba_unmultiplied(",
    "Color32::from_rgba_premultiplied(",
    "Color32::from_gray(",
    "Color32::from_black_alpha(",
    "Color32::from_white_alpha(",
    "Color32::from_additive_luminance(",
    "Color32::from_hex(",
];

/// egui's named colours that are not a colour choice: no paint at all.
const NOT_A_COLOUR: &[&str] = &["Color32::TRANSPARENT", "Color32::PLACEHOLDER"];

/// Whether `code` builds a colour from a number written in place: one of
/// [`COLOUR_CONSTRUCTORS`] whose first argument starts with a digit (or a
/// sign or point), a named engine colour, or an `Rgba { r: <number>, …}`.
fn colour_literal(code: &str) -> bool {
    let literal_arg = COLOUR_CONSTRUCTORS.iter().any(|ctor| {
        code.match_indices(ctor).any(|(at, _)| {
            code[at + ctor.len()..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
        })
    });
    let named = ["css::", "palettes::", "Color::WHITE", "Color::BLACK"]
        .iter()
        .any(|n| code.contains(n));
    // egui's named constants (`Color32::WHITE`, `Color32::GRAY`, …).
    let egui_named = code.match_indices("Color32::").any(|(at, n)| {
        let rest = &code[at..];
        rest[n.len()..].starts_with(|c: char| c.is_ascii_uppercase())
            && !NOT_A_COLOUR.iter().any(|ok| rest.starts_with(ok))
    });
    // A colour field set from a number: `r: 0.1` (not `near: 0.1`).
    let field = ["r: ", "red: "].iter().any(|n| {
        code.match_indices(n).any(|(at, _)| {
            let ident_before = code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            !ident_before
                && code[at + n.len()..].starts_with(|c: char| c.is_ascii_digit() || c == '.')
        })
    });
    literal_arg || named || egui_named || field
}

/// **One source for colour** (briefs 4b-3 and U-7): a colour written as
/// numbers lives in a theme module and nowhere else, as a non-ASCII glyph
/// lives in `ui::fonts::glyph`. Scene colour (and the `debug` overlay
/// colours) is `gradiance_domain::theme`; chrome colour is
/// `gradiance_ui::theme`'s tokens (`docs/unified-surface-plan.md` § 7.1).
/// Test modules are exempt (fixtures name colours).
#[test]
fn scene_colours_have_one_source() {
    let root = repo_root();
    let allowed = [
        "crates/gradiance-domain/src/theme.rs",
        "crates/gradiance-ui/src/theme.rs",
    ];
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
            // A file's unit tests sit at its end, after `#[cfg(test)]`.
            if trimmed.starts_with("#[cfg(test)]") {
                break;
            }
            if trimmed.starts_with("//") {
                continue;
            }
            let code = trimmed.split("//").next().unwrap_or(trimmed);
            if colour_literal(code) {
                found.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "colour literals outside gradiance_domain::theme (add a token there):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_colour_scanner_sees_literals() {
    assert!(colour_literal("let c = Color::srgba(0.5, 0.5, 0.5, 1.0);"));
    assert!(colour_literal("x(css::GOLD)"));
    assert!(colour_literal("Rgba::rgb(.3, 0.0, 0.0)"));
    assert!(colour_literal("Rgba { r: 0.13, g: 0.1, b: 0.1, a: 1.0 }"));
    assert!(!colour_literal("Color::srgba(c.r, c.g, c.b, c.a)"));
    assert!(!colour_literal("Rgba::rgb(r.clamp(0.0, 1.0), g, b)"));
    assert!(!colour_literal("Color::from(ink.snap)"));
    assert!(colour_literal("r: 0.08,"));
    assert!(!colour_literal("near: 0.5,"));
    assert!(!colour_literal("angular: 0.0,"));
    assert!(colour_literal("egui::Color32::from_rgb(120, 200, 255)"));
    assert!(colour_literal("egui::Color32::from_gray(70)"));
    assert!(colour_literal(
        ".fill(egui::Color32::from_black_alpha(170))"
    ));
    assert!(colour_literal(".color(egui::Color32::WHITE)"));
    assert!(colour_literal("Stroke::new(1.0, egui::Color32::GRAY)"));
    assert!(!colour_literal(
        "egui::Color32::from_rgba_unmultiplied(shade, shade, shade, 200)"
    ));
    assert!(!colour_literal(".fill(egui::Color32::TRANSPARENT)"));
    assert!(!colour_literal("fn f(c: egui::Color32) -> egui::Color32 {"));
}

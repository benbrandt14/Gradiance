//! The string table (brief S-19): the menus' labels by id, in the chosen
//! language.
//!
//! English is the table `assets/lang/en.ron`, compiled in, so the editor
//! always has its labels. A second language is a data file beside it
//! (`assets/lang/<code>.ron`, the same ids); an id that file lacks shows
//! the English text, and an id English lacks shows the id itself, so a
//! missing row is visible, never blank. [`Language`] is workstation
//! config, like the unit settings: written by Settings > Interface, never
//! undone, not saved with a scene (decision `language-not-in-scene`).
//!
//! The migration is a ratchet (decision `string-table-ratchet`): the menu
//! bar reads every label from here, and `tests::no_new_label_literals`
//! counts the label literals left in each panel file, so the count can only
//! go down.

use bevy::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The English table, compiled in.
const ENGLISH: &str = include_str!("../../../assets/lang/en.ron");

/// The chosen language's code: the file name of its table (`en`).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Language(pub String);

impl Default for Language {
    fn default() -> Self {
        Self("en".to_owned())
    }
}

/// Where the language tables are (`assets/lang` under the asset root).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct LanguageDir(pub PathBuf);

impl Default for LanguageDir {
    fn default() -> Self {
        Self(bevy::asset::io::file::FileAssetReader::get_base_path().join("assets/lang"))
    }
}

/// A parsed string table: id to text.
pub type Table = HashMap<String, String>;

/// Parses a table's RON text.
///
/// # Errors
///
/// The text is not a RON map of strings.
pub fn parse(text: &str) -> Result<Table, gradiance_persist::ron::de::SpannedError> {
    gradiance_persist::ron::from_str(text)
}

/// The active labels: the chosen language's table over English.
#[derive(Resource, Debug, Clone)]
pub struct Strings {
    english: Table,
    chosen: Table,
}

impl Default for Strings {
    fn default() -> Self {
        Self {
            english: parse(ENGLISH).unwrap_or_default(),
            chosen: Table::new(),
        }
    }
}

impl Strings {
    /// English with `chosen` over it.
    #[must_use]
    pub fn with(chosen: Table) -> Self {
        Self {
            chosen,
            ..Self::default()
        }
    }

    /// The label for `id`: the chosen language's, else English, else the
    /// id itself.
    #[must_use]
    pub fn t<'a>(&'a self, id: &'a str) -> &'a str {
        self.chosen
            .get(id)
            .or_else(|| self.english.get(id))
            .map_or(id, String::as_str)
    }

    /// Whether English has `id` (the tests' check that a label exists).
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.english.contains_key(id)
    }
}

/// Where [`share_with_egui`] keeps the table in egui's memory.
fn egui_key() -> bevy_egui::egui::Id {
    bevy_egui::egui::Id::new("gradiance-strings")
}

/// English, parsed once, for [`tr`] before the first frame shares a table.
static ENGLISH_STRINGS: std::sync::LazyLock<Strings> = std::sync::LazyLock::new(Strings::default);

/// The label for `id` from the table the last frame shared with `ctx`: for
/// panel code deep in a call tree that has the `Ui` but not the resource.
#[must_use]
pub fn tr(ctx: &bevy_egui::egui::Context, id: &str) -> String {
    ctx.data(|d| d.get_temp::<std::sync::Arc<Strings>>(egui_key()))
        .map_or_else(
            || ENGLISH_STRINGS.t(id).to_owned(),
            |strings| strings.t(id).to_owned(),
        )
}

/// Shares the active table with egui's memory when it changes (or on the
/// first frame), so [`tr`] reads it.
pub fn share_with_egui(
    mut contexts: bevy_egui::EguiContexts,
    strings: Res<Strings>,
    mut shared: Local<bool>,
) -> Result {
    if *shared && !strings.is_changed() {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let table = std::sync::Arc::new(strings.clone());
    ctx.data_mut(|d| d.insert_temp(egui_key(), table));
    *shared = true;
    Ok(())
}

/// The language codes with a table in `dir`, English first.
#[must_use]
pub fn languages(dir: &Path) -> Vec<String> {
    let mut codes: Vec<String> = std::fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |read| {
            read.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .filter_map(|p| Some(p.file_stem()?.to_string_lossy().into_owned()))
                .filter(|c| c != "en")
                .collect()
        },
    );
    codes.sort();
    codes.insert(0, "en".to_owned());
    codes
}

/// Rebuilds [`Strings`] when the [`Language`] changes: English needs no
/// file; another code reads `<dir>/<code>.ron`. A file that cannot be read
/// or parsed leaves English, with a warning.
pub fn load_language(language: Res<Language>, dir: Res<LanguageDir>, mut strings: ResMut<Strings>) {
    if !language.is_changed() {
        return;
    }
    if language.0 == "en" {
        *strings = Strings::default();
        return;
    }
    let path = dir.0.join(format!("{}.ron", language.0));
    match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| parse(&text).map_err(|e| e.to_string()))
    {
        Ok(table) => *strings = Strings::with(table),
        Err(e) => {
            warn!(path = %path.display(), error = %e, "language table not loaded");
            *strings = Strings::default();
        }
    }
}

/// The language picker's resources, bundled to keep the settings window
/// under Bevy's system-parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct LanguageParams<'w> {
    /// The chosen language.
    pub language: ResMut<'w, Language>,
    /// Where the tables are.
    pub dir: Res<'w, LanguageDir>,
    /// The active labels.
    pub strings: Res<'w, Strings>,
}

/// Settings > Interface > Language: one row per table in the language
/// folder. Workstation config: written here directly, never undone, not
/// saved with a scene.
pub fn language_section(ui: &mut bevy_egui::egui::Ui, params: &mut LanguageParams) {
    let strings = &*params.strings;
    crate::widgets::section_header(ui, strings.t("settings-language"));
    let current = params.language.0.clone();
    bevy_egui::egui::ComboBox::from_id_salt("language")
        .selected_text(current.as_str())
        .show_ui(ui, |ui| {
            for code in languages(&params.dir.0) {
                if ui
                    .selectable_label(code == current, code.as_str())
                    .clicked()
                    && code != current
                {
                    params.language.0 = code;
                }
            }
        })
        .response
        .on_hover_text(strings.t("settings-language-hint"));
}

/// Registers the language resources and the loader (headless too, so a
/// test can switch the table without a window).
pub(crate) fn install(app: &mut App) {
    app.init_resource::<Language>();
    app.init_resource::<LanguageDir>();
    app.init_resource::<Strings>();
    app.add_systems(PreUpdate, load_language);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_parses_and_an_unknown_id_shows_itself() {
        let strings = Strings::default();
        assert_eq!(strings.t("menu-file"), "File");
        assert_eq!(strings.t("no.such.id"), "no.such.id");
    }

    #[test]
    fn a_chosen_table_wins_and_falls_back_to_english() {
        let mut table = Table::new();
        table.insert("menu-file".into(), "Datei".into());
        let strings = Strings::with(table);
        assert_eq!(strings.t("menu-file"), "Datei");
        assert_eq!(strings.t("menu-edit"), "Edit", "not in the table");
    }

    /// The label calls a panel file may still make with a literal string:
    /// the files not yet on the table, at their count when the table came
    /// (S-19). A file over its count fails: put the new label in
    /// `assets/lang/en.ron` and read it with `Strings::t`. A file under its
    /// count fails too, so the number here goes down with it.
    const BASELINE: &[(&str, usize)] = &[
        ("adjust_panel.rs", 2),
        ("array_panel.rs", 3),
        ("console.rs", 4),
        ("context_menu.rs", 4),
        ("depth_panel.rs", 1),
        ("groups.rs", 19),
        ("inspector.rs", 45),
        ("joint_inspector.rs", 18),
        ("node_graph.rs", 18),
        ("optimizer.rs", 38),
        ("plot.rs", 1),
        ("probe.rs", 3),
        ("ramp_editor.rs", 2),
        ("reflect_grid.rs", 1),
        ("review.rs", 7),
        ("settings.rs", 29),
        ("signals.rs", 10),
        ("toolbar.rs", 3),
        ("unit_fields.rs", 3),
        ("widgets.rs", 17),
        ("workspaces.rs", 4),
    ];

    /// The calls that draw a label, followed by a string literal.
    const LABEL_CALLS: &[&str] = &[
        ".button(\"",
        ".small_button(\"",
        ".label(\"",
        ".menu_button(\"",
        ".checkbox(",
        "Button::new(\"",
        "Window::new(\"",
        "section_header(ui, \"",
    ];

    /// Counts label calls with a literal in `text`, comments left out.
    fn label_literals(text: &str) -> usize {
        text.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(|line| {
                LABEL_CALLS
                    .iter()
                    .map(|call| {
                        if *call == ".checkbox(" {
                            // `checkbox(&mut x, "label")`
                            line.match_indices(call)
                                .filter(|(i, _)| {
                                    line[*i..]
                                        .split_once(',')
                                        .is_some_and(|(_, rest)| rest.trim_start().starts_with('"'))
                                })
                                .count()
                        } else {
                            line.matches(call).count()
                        }
                    })
                    .sum::<usize>()
            })
            .sum()
    }

    #[test]
    fn no_new_label_literals() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut problems = Vec::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{e}"))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();
        files.sort();
        for path in files {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            // This file names the calls it counts.
            if name == "text.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // Test modules draw throwaway labels.
            let text = text.split("#[cfg(test)]").next().unwrap_or_default();
            let found = label_literals(text);
            let allowed = BASELINE
                .iter()
                .find(|(f, _)| *f == name)
                .map_or(0, |(_, n)| *n);
            if found != allowed {
                problems.push(format!(
                    "{name}: {found} label literals, baseline {allowed}"
                ));
            }
        }
        assert!(
            problems.is_empty(),
            "label literals changed (new label: add it to assets/lang/en.ron and use \
             `Strings::t`; fewer: lower the baseline):\n{}",
            problems.join("\n")
        );
    }
}

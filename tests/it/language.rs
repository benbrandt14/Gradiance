//! Language choice (brief S-19): the menu bar's labels come from the string
//! table, and choosing another table changes them; an id the table lacks
//! keeps its English text.

#![allow(clippy::unwrap_used)]

use crate::harness::headless_app;
use gradiance::ui::text::{Language, LanguageDir, Strings, languages};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lang")
}

#[test]
fn switching_to_a_test_table_changes_the_menu_labels() {
    let mut app = headless_app();
    let label =
        |app: &bevy::prelude::App, id: &str| app.world().resource::<Strings>().t(id).to_owned();
    assert_eq!(label(&app, "menu-file"), "File");
    assert_eq!(label(&app, "panel-probe"), "Probe");

    app.insert_resource(LanguageDir(fixtures()));
    app.insert_resource(Language("xx".into()));
    app.update();
    assert_eq!(label(&app, "menu-file"), "[File]");
    assert_eq!(label(&app, "panel-probe"), "[Probe]");
    assert_eq!(label(&app, "menu-edit"), "Edit", "not in the test table");

    app.insert_resource(Language("en".into()));
    app.update();
    assert_eq!(label(&app, "menu-file"), "File", "back to English");
}

#[test]
fn the_picker_lists_english_first_then_each_table() {
    assert_eq!(languages(&fixtures()), ["en", "xx"]);
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/lang");
    assert_eq!(
        languages(&shipped),
        ["en"],
        "English is the only shipped table"
    );
}

# Language choice (string table)
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("language") · status: ready

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

Every user-facing label in menus, panels and operation titles comes from
one string table keyed by id. Settings has a language picker. English is
the only shipped table; a second table is a data file, not code.

## Read first

- `crates/gradiance-op` — `OpMeta` labels (menus, catalog, search).
- `docs/ui-design.md` — glyph and font rules (`ui::fonts::glyph`).
- `crates/gradiance-ui/src/menu.rs` — the menu tables.

## Facts

- Most labels already come from tables (`OpMeta`, `Panels::named`,
  `DEBUG_TOGGLES`). Hand-written panel strings are the work.
- Non-Latin scripts need a font with those glyphs. *Out of scope* until a
  second language is wanted.

## Approach

- `ui::text::t(id)` over a RON table loaded at start; a source scanner
  test (like the glyph scanner) rejects new string literals in panels.
- Language is workstation config.

## Out of scope

Translations. Right-to-left layout.

## Done when

- Scanner test passes; switching to a test table changes the menu labels.
- Gates green.

## Outcome

Done 2026-11-23 (PR on the S-18 branch).

- `assets/lang/en.ron`: the English table, ids in `group-name` form
  (hyphens, so no id reads as an operation id to the side-door scanner).
  It is compiled in (`include_str!`); another language is
  `assets/lang/<code>.ron` with the same ids. A missing id shows the
  English text, then the id itself.
- `ui::text`: `Strings::t(id)`, `Language` (workstation config, not saved
  in a scene: decision `language-not-in-scene`), `LanguageDir`,
  `load_language` (PreUpdate, on change), `languages(dir)`, and the
  Settings > Interface > Language picker. `persist` re-exports `ron` so the
  UI reads a table without a new dependency edge.
- Migrated: the whole menu bar (File, Edit, View, Help, Debug, the panel
  rows, camera and mirror rows, About) and the Scenes panel; the context
  menu's mirror rows read the same ids.
- The rest is a ratchet (decision `string-table-ratchet`):
  `text::tests::no_new_label_literals` counts the label literals per file
  against `BASELINE` (about 230 in 21 files). A count that rises or falls
  fails, so the baseline only goes down. Operation titles (`OpMeta`) are
  not on the table yet.
- Tests: `tests/it/language.rs` (switching to the fixture table
  `tests/fixtures/lang/xx.ron` changes the menu labels, an id it lacks
  stays English, English is the only shipped table); unit tests in
  `ui::text`; `panels.rs` checks every panel label id is in `en.ron`;
  `ui_conflicts` initializes `scenes_window` and `settings_window`.


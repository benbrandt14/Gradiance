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

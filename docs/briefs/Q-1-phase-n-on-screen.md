# Phase N on screen
Phase Q (follow-ups) · the "not checked on screen" lines of the N-1..N-8 and
6-3 Outcomes · status: ready

Written by the planning thread, 2026-11-20, on `523ed82`, from the
"Not done / follow-ups" sections of the Phase N and Phase 6 Outcomes (owner
choice: follow-ups first).

## Goal

Every visual that Phase N added is seen once in the running app, and what
is wrong is fixed in this PR. Today most of them were only tested
headless: the Outcomes of N-1, N-2, N-3, N-4, N-5, N-6, N-8 and 6-3 say
"not checked on screen". After this brief, each tool, glyph, block and
settings row below has a lavapipe screenshot on the PR thread, and each
defect that the screenshots show is either fixed (with a test where a test
can see it) or written down as a follow-up with its reason.

The brief also ingests the two review answers that wait in
`docs/review/answers.ron` (phase start, `docs/orchestration.md` § Review
mode), because one of them changes the default look that the screenshots
show.

## Read first

1. `tools/screenshot/README.md`: the harness, the coordinates, the
   lessons (the sim plays from start-up; a gesture plays over the next
   frames).
2. `docs/ui-design.md`: the design system (glyphs, fields, panels). A fix
   to chrome follows it.
3. `docs/orchestration.md` § Review mode, "Ingesting".
4. The Outcomes of `docs/briefs/N-1-*` to `N-8-*` and `6-3-*` ("Not done /
   follow-ups"): the list below comes from them.

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

The check list, with where each thing is drawn:

| Brief | What to see | Where |
| --- | --- | --- |
| N-1 | body block `field` and `gravity` pins; the joint block with motor pins and units; the Signals list hiding domain/gradient for an actuator ("raw value") | `crates/gradiance-ui/src/node_graph.rs:76` (`NodeData`), `:755` (block list), `crates/gradiance-ui/src/signals.rs:291` |
| N-2 | Key block footer: the `+` capture button, **pair**, the `-` button, the clash warning lines | `crates/gradiance-ui/src/node_graph.rs:1633` (`key_capture`) |
| N-3 | thruster flame (18 px at 10 N, up to 4x); palette icon; thruster block | `crates/gradiance-render/src/tracer.rs:198` (`flame_scale`), `:258`; `crates/gradiance-ui/src/toolbar.rs:58`; `assets/icons/thruster.png` |
| N-4 | Settings > Simulation > Air: five rows | `crates/gradiance-ui/src/settings.rs:381` (`air_section`) |
| N-5 | chain stroke preview (links and hinge rings); palette icon | `crates/gradiance-interaction/src/tools/chain_tool.rs:104` (`preview`); `toolbar.rs:47`; `assets/icons/chain (2).png` |
| N-6 | sketch preview (faint stroke, centre line, rings, closed outline); palette icon | `crates/gradiance-interaction/src/tools/sketch_tool.rs:133`; `toolbar.rs:36`; `assets/icons/sketch.png` |
| N-7 | the inspector fluid toggle and its two fields (the pool itself was checked in N-7) | `crates/gradiance-ui/src/inspector.rs:346` (`fluid_block`) |
| N-8 | beam (danger ink, accent ring on the target), barrel glyph, palette icon, the laser editor rows | `crates/gradiance-render/src/tracer.rs:279`; `toolbar.rs:59`; `crates/gradiance-ui/src/widgets.rs:506` (`laser_editor`) |
| N-9 | Edit > Mirror rows (the load colours were checked in N-9) | `crates/gradiance-ui/src/menu.rs:304` (`MIRROR_ROWS`) |
| 6-3 | View > Debug submenu opened | `crates/gradiance-ui/src/menu.rs:347` (`debug_menu`) |

- Scenes for each row can be built with the existing verbs: `thruster`,
  `laser`, `chain`, `sketch`, `set-fluid`, `defkey`, `defsignal`,
  `hinge` (`docs/scripting.md`). A tool's preview needs a held drag:
  `ui.drag` with many `steps`, and a grab before the release, or a press
  through `ui.click` is not enough (README).
- Open review answers (`docs/review/answers.ron`): `toon-ramp` picked
  **"Ramp, hard bands"** (dither 0; the owner's note: "we'll go with the
  hard bands for now"); `status-bar-and-pie` picked **"Status bar on"**,
  which is already the default (`crates/gradiance-ui/src/status_bar.rs:28`).
  The default dither is `STUDIO_DITHER = 0.025`
  (`crates/gradiance-domain/src/theme.rs:406`), and the v8 golden holds
  `dither: 0.025` (`crates/gradiance-scene/golden/v8_every_quantity.ron:375`).
  Their `expected_bodies` rows are `tests/it/review.rs:97` and `:99`.
- Snapshot fixtures: `tests/snapshots/ui/` (six PNGs). Only an intended
  change regenerates one (`UPDATE_SNAPSHOTS=1`).

## Approach

1. **Ingest first** (its own commit, "review: ingest toon-ramp,
   status-bar-and-pie"): move both questions and answers to
   `archive.ron` with `ingested_as`, delete them from `questions.ron` and
   `answers.ron`, drop their two `expected_bodies` rows, keep their
   `scenes/` files. Set the default dither to 0 (the looks that use
   `STUDIO_DITHER`; keep the setting and its slider). Regenerate the
   golden only if it holds a default it must match.
2. **Shoot** each row of the table in one or two harness sessions with a
   start-up script per scene. Keep the images out of git (default
   `/tmp/gradiance-shots`); attach them to the PR thread with one line
   each.
3. **Fix** what is wrong where the fix is local to the drawn item (a
   glyph size, a colour from the wrong token, a clipped row, a missing
   label, an icon that does not load). Each fix that a headless test can
   see gets a test; a pure look fix is shown by a before/after pair on the
   PR.
4. **Write down** what is too big for this PR as a line in the Outcome
   ("Found, not fixed"), with the screenshot reference. The orchestrator
   turns it into a brief.

## Tests

- `cargo test --test it -- review` passes after the ingestion.
- A test per fixed defect where the defect is visible headless (a layout
  value, a widget name from `ui.inspect`, a gizmo count).
- `ui_snapshots` with `GRADIANCE_REQUIRE_GPU=1`: only intended changes.
- fmt, clippy, doc, `--test boundaries`, the `-p` runs of touched crates.

## Out of scope

- New features or new looks. The water surface is Q-5; the laser block is
  Q-4.
- The fluid pool and the joint load colours (already checked in N-7 and
  N-9), except as background in another shot.
- Committing ad-hoc screenshots. An image a test or a review question uses
  may be committed (owner ruling, 2026-11-17).
- Keys (owner ruling: keys out of scope).

## Outcome

To be filled by the worker at handoff.

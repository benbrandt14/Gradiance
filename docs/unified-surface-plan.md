# Plan: one operation surface for UI, Lisp, CLI, MCP and the graph

Status: **accepted plan; order decided (§ 10); no code.** Written
2026-11-17 on branch `claude/laughing-euler-sh2g1r`, after `main` `04067ab`.
It replaces the 2026-11-16 investigation `docs/ui-blender-investigation.md`
(same branch, never merged). The owner rulings in `docs/state.md` still apply:
no unbriefed work, one worker at a time. § 10 proposes briefs and where they
go in the phase order.

## 0. Owner direction (2026-11-17)

Recorded from the owner's message, in order:

1. **Polished egui is fine now**, if the polish can be excellent. This
   replaces `orchestration.md` § Future intent 1's "don't polish egui".
2. **Lean towards Blender's theme**, but not at the cost of other things.
3. **Upgrade crates such as egui** where that prevents a lot of rework.
   Dependency decisions from months ago may be out of date.
4. **Graphite is the capability goal** (integrated node graph, procedural
   modelling). **Shore up the MVP first.**
5. A **3D workflow** follows. It needs detail planning, it is a major
   refactor, and the existing roadmap comes first.
6. The source projects (Graphite, the Craft apps) change fast; Graphite is
   getting a dedicated desktop app. **Plan the architecture so that porting
   their future changes is easy**, even without a direct dependency.
7. **Drop BUI.**
8. **Licensing is not a concern yet**: everything would be rewritten before a
   real release.
9. **The goal:** MCP, the command line, the UI, the Steel Lisp dialect and
   the procedural graph are **one integrated system** that supports
   extension and **cannot become misaligned from itself**.

---

## 1. Recommendation in six lines

1. Make the **operation** the one unit that every surface shows. Declare each
   operation once, as a typed Rust struct with metadata. Generate the Lisp
   verb, the JSON schema, the CLI form, the menu row, the default key, the
   docs and (later) the graph node from that declaration.
2. Make alignment a **test**, not a habit. Generated conformance tests prove
   that each doorway (UI, Lisp, BRP/MCP, CLI) turns the same call into the
   same intent.
3. **Upgrade to the Bevy 0.20 stack first** (Bevy 0.20.1, avian2d 0.8,
   bevy_egui 0.43, egui 0.36). Every later UI brief then lands on the new
   APIs once. egui 0.37 waits for a bevy_egui release.
4. **Port, do not depend.** Keep ported code in upstream shape, record the
   upstream revision, and diff upstream with one script.
5. Then the test and control tier (widget registry, input injection,
   screenshots, MCP), then the Blender-leaning polish.
6. The procedural graph and 3D stay **planned, not built**. The operation
   design keeps both open (§ 5).

---

## 2. The alignment problem today (measured)

A new scene operation now touches up to **eight** hand-kept places. Each has
its own table:

| # | Surface | Where | Source of truth | Drift guard today |
|---|---|---|---|---|
| 1 | Intent + command | `gradiance-command/src/intent.rs`, `dispatch.rs` `command_intents!` | the macro row | registration and dispatch come from one macro row |
| 2 | Op catalogue (names, signature **strings**, hand-counted `args`) | `gradiance-script/src/registry.rs` (`OpSpec`, 796 lines) | hand-written rows | unique names; signature starts with name |
| 3 | Steel verb (hand-written argument coercion) | `gradiance-script/src/bridge.rs` (49 `register_fn` calls, 2898 lines) | closures | shared `name::` constants |
| 4 | Verb → intent link | `bridge.rs` `EditBinding` | hand list | type path must resolve |
| 5 | BRP | `gradiance-script/src/remote.rs` | lowers JSON args to **Scheme text** | none beyond the catalogue |
| 6 | Keys | `gradiance-interaction/src/input.rs` `EditorAction` (leafwing) | its own enum | none: not linked to ops |
| 7 | Menus | `gradiance-ui/src/menu.rs` | writes `UndoIntent`, `DeleteIntent`, … **directly** | none |
| 8 | Panels, script actions | `menu::Panels::named`; `script/src/actions.rs` | two more tables | a test pairs names with labels |

Not yet present: a CLI beyond `--script`, an MCP server, a procedural graph.
The signal graph (`gradiance-signal`, `gradiance-kernel`) is a runtime
dataflow, and it is a separate model.

Thus the existing guards stop **name** drift. They do not stop **shape** drift
(argument count, argument types, units, what a menu does versus what the verb
does). The `args: usize` field and the signature string are both written by
hand, next to a closure that decides the real arity.

---

## 3. Target architecture: one operation, many projections

### 3.1 The declaration

An operation is a typed parameter struct plus metadata, declared once:

```rust
/// Author a box body of size w×h centred at (x, y).
#[derive(Operation, Reflect, Serialize, Deserialize, Clone)]
#[op(id = "body.spawn_box", seam = Edit, menu = "Add/Box", key = "B",
     undo = "Spawn Box")]
pub struct SpawnBox {
    /// Centre.
    pub at: Vec2<Length>,
    /// Size.
    #[op(default = (1.0, 1.0), hard = 0.001..)]
    pub size: Vec2<Length>,
}
```

(Sketch. Names and the macro syntax are a brief's decision.)

- **Id:** dotted and namespaced (`body.spawn_box`, `panel.open`, `ui.click`,
  `layout.save`). This is the Craft control-protocol vocabulary, so their
  protocol text, tests and docs port with few edits. The Lisp name is derived
  (`spawn-box`); an explicit alias keeps today's verbs.
- **Doc comments are the docs**, as in Graphite's `#[node_macro::node]`.
  Field docs become parameter help in the console, MCP schema, CLI `--help`
  and tooltips.
- **Typed parameters** use `gradiance-units` quantities (brief 5-1 landed),
  so units, defaults and ranges are data, not prose.
- **The struct is not the intent; it lowers to one.** Intents are low-level
  and carry what undo needs: `SpawnBodyIntent { record: BodyRecord }`,
  `CommitTransformIntent { changes: Vec<TransformChange> }` (old *and* new
  poses). Each operation has one `lower(params, &OpSnapshot) ->
  Result<Intent, OpError>`, written by the layer that owns it. The snapshot
  gives the old values, as `SceneView` does for scripts now
  (`gradiance-script/src/bridge.rs:558`), so the scripting rule "one
  snapshot per run" holds for every doorway.
- Execution does not change: doorway → `lower` → intent →
  `command::dispatch` → `GameCommand` → `CommandStack`. Invariants 1–2 hold.
  No new mutation path.

### 3.2 References to scene objects

Today a verb takes an index into the pre-run snapshot, and the UI uses the
selection. One parameter type covers both:

```rust
enum Target { Index(u32), Id(StableId), Selection, At(Vec2<Length>) }
```

Each **doorway** resolves `Target` to `StableId`s with the context it owns,
before it emits the intent. The UI owns the selection; Lisp owns the
snapshot; MCP and the CLI use `Id` or `At`. The intent then holds only
`StableId`s (invariant 3). This removes the reason for pairs such as
`(delete i)` versus "delete selection", and it obeys the scripting rule that
"a read never reaches up the layer graph".

### 3.3 Where the registry lives (layer rule)

A new bottom crate `gradiance-op` holds the trait, the metadata types, the
`Target` type and the registry resource. It depends on `gradiance-core` and
`gradiance-units` only. Each layer **registers its own operations at app
build**, as Bevy's type registry works:

- `command` registers scene operations;
- `interaction` registers tool and selection operations;
- `persist` registers file operations;
- `ui` registers `panel.*`, `layout.*` and `ui.*` operations.

A doorway (Lisp, BRP, CLI, MCP) dispatches **by id through the registry**. It
does not need a compile-time edge to the layer that owns the operation. This
removes the upward-edge problem that the first investigation found for
`gradiance.ui.*` methods, and `tests/boundaries.rs` keeps the DAG.

### 3.4 Projections, generated from the registry

| Surface | What it gets from the registry | Replaces |
|---|---|---|
| Steel | A generated prelude: `(define (spawn-box at size) (%op "body.spawn_box" at size))`, plus one native `%op` builtin with one generic coercion from `SteelVal` to reflected fields | 49 hand-written `register_fn` closures, `EditBinding` |
| BRP / MCP | JSON schema per operation from reflection; `gradiance.op {id, params}` with **named** JSON params, decoded by serde, not lowered to Scheme text | `remote.rs` Scheme lowering |
| CLI | `gradiance op body.spawn_box --at 0,1 --size 1.6,1.2`; `gradiance ops` lists them; `gradiance mcp` serves MCP on stdio | — |
| Menus, F3 search, pie menus, status bar | `menu` path, label, icon, enable predicate | hand-wired rows, direct intent writes |
| Keys | default chord from `key`; leafwing `InputMap` built from the registry; user overrides stored separately | `EditorAction` enum |
| Console | completion, signatures, docs | `OpSpec` strings |
| Docs | generated `docs/ops.md` | prose in `scripting.md` |
| Graph (later) | operations marked `node` become node types (§ 5) | — |

`OpCategory` (edit / config / query / editor) becomes `seam`. The `seam`
decides the undo behaviour, as `ui-design.md` § 4 does now.

### 3.5 Conformance tests: why it cannot drift

Tests that iterate the registry, so a new operation is covered without a new
test:

1. **Doorway equivalence.** For each operation with sample params (generated
   from its defaults), invoke it through Lisp, BRP JSON, the CLI parser and the
   UI invoke path. All four **lower to equal intents** against the same
   snapshot.
2. **Schema round trip.** JSON schema ↔ serde ↔ reflection agree for each
   operation.
3. **No side doors.** A source-scanner test (as `no_source_file_uses_an_unlisted_glyph`
   does) fails if `gradiance-ui` or `gradiance-interaction` declares a
   `MessageWriter<…Intent>` outside the one invoke path.
4. **Every menu row, key and action names a registered id.**
5. **Generated docs are current** (`docs/ops.md` equals the generator output).
6. **One undo step per invocation**; `op.batch` (as Craft's
   `command.batch`) is one undo step for the whole batch.

### 3.6 Migration shape

Do not migrate all at once. Brief U-1 builds `gradiance-op` and moves three
operations end to end (`body.spawn_box`, `body.place`, `edit.undo`) with the
conformance tests. Brief U-2 moves the rest by category. Old and new verbs
can live together during U-2, because both end at the same intents.

---

## 4. Dependencies: upgrade first

Measured on crates.io and GitHub, 2026-11-17:

| Crate | Pin now | Target (Bevy 0.20 stack) | Note |
|---|---|---|---|
| `bevy` | `=0.19.0` | 0.20.1 | |
| `avian2d` | `=0.7.0` | 0.8.0 (needs Bevy 0.20) | |
| `bevy_egui` | `=0.41.0` (egui 0.35) | 0.43.1 (egui 0.36) | bevy_egui `main` (0.43.2) is still egui 0.36 |
| `egui_tiles` | `=0.16.0` | 0.17.0 (egui 0.36) | 0.18 needs egui 0.37 |
| `egui-snarl` | `=0.11.0` | 0.12.0 (egui 0.36) | |
| `egui_plot` | `=0.36.0` | 0.37.0 (egui 0.36) | 0.38 needs egui 0.37 |
| `egui_kittest` | `=0.35.0` | 0.36.2 | |
| `leafwing-input-manager` | `=0.21.0` | 0.22.0 (needs Bevy 0.20) | may go away (§ 3.4) |
| `steel-core` | `=0.8.2` | 0.8.3 | patch |

Recommendation:

- **D-0: toolchain first.** Bevy 0.20.0 and 0.20.1 declare `rust_version`
  1.97.1; the pin is 1.97.0. `static.rust-lang.org` answered HTTP 200 from
  the container on 2026-11-17 (stable is 1.100.0).
- **D-1: one brief for the Bevy 0.20 stack**, before the UI work. Each UI brief
  after it is written once against current APIs. `docs/bevy19-notes.md`
  becomes `bevy20-notes.md`.
- **D-2: egui 0.37** when bevy_egui releases it. The Craft code is on 0.37, so
  this step lowers porting cost again. Do **not** fork bevy_egui to get there
  earlier: a fork is one more upstream to track.
- The 3D refactor (avian2d → a 3D engine) is separate and later. D-1 does not
  make it harder.

---

## 5. The procedural graph (planned now, built after the MVP)

Graphite is the capability goal. Its node graph **is** the document: nodes are
Rust functions declared with `#[node_macro::node(category(…), name(…))]`, with
`#[default]` and `#[hard]` ranges on inputs, and doc comments as help. It also
runs the same graph from a CLI (`node-graph/graphene-cli`).

For Gradiance the alignment rule is: **one IR, three views.**

- The IR is a typed DAG of operation calls whose arguments are values or other
  nodes' outputs.
- **Lisp text is one view**: an s-expression is a tree, and `let` gives
  sharing, so it is a DAG.
- **The node canvas is a second view** (`egui-snarl`, as the signal canvas now).
- **The inspector and the tools are a third view**: a tool gesture edits a
  node's parameters.
- **Evaluation is compiled Rust**, never Steel in the frame loop (the
  two-tier rule). The signal graph already lowers to `gradiance-kernel`; the
  geometry graph lowers to `gradiance-geometry` calls in the same way.

What § 3 must hold **now** so this stays cheap later:

- an operation's params are plain data (no hidden selection, no ECS reads
  inside the struct);
- operations that are pure functions of their inputs say so (`node` flag);
- outputs are typed (a body, a shape, a scalar), even when only the Lisp
  doorway uses them today;
- every reference is `StableId`, so a graph edge survives save and load.

This is a design note for the next planning stage, not a brief. The signal
graph and the geometry graph should become **one** graph model there; that is
the decision to make, together with the 3D plan.

---

## 6. Porting from fast-moving upstreams

The Craft apps and Graphite change daily. Three rules make re-porting
mechanical:

1. **Vendor in upstream shape.** Ported files keep upstream's file name,
   module layout and public function names, under
   `crates/<crate>/src/vendor/<upstream>/`. Bevy-specific changes go in a thin
   adapter next to it, not in the ported file.
2. **Record the revision.** `vendor/UPSTREAM.toml` lists, per ported file:
   repository, path, revision, and a short list of local changes.
   `tools/upstream-diff.sh <entry>` fetches the file at upstream `HEAD` and
   shows the diff since the recorded revision. The orchestrator runs it at each
   phase gate and writes a brief when an upstream change is worth taking.
3. **Adopt upstream vocabulary at the seams.**
   - Craft: control-protocol ids and JSON shapes (`ui.click`, `ui.key`,
     `ui.screenshot`, `ui.inspect`, `panel.open`, `layout.save`,
     `command.batch`), the widget-registry API (`publish_handle`,
     `handle_at`, `publish_rect`), the MCP tool names (`execute`, `batch`).
   - Graphite: node-definition metadata (`category`, `name`, `default`,
     `hard`), so a later port of node definitions maps field by field.

A host adapter in `gradiance-ui` (`ui::host`: input events, cursor grab,
clipboard, screenshot request) hides the bevy_egui/eframe difference. Craft
code calls eframe in these places; the adapter gives it one Bevy-side target.

**What ports and what does not:**

| Source | Ports | Does not port |
|---|---|---|
| `craft-libs/crates/ui` (egui 0.37) | widgets, `testing` (`Screens`, snapshots on lavapipe), docking model as a spec | eframe window glue |
| `craft-libs/crates/keymap` | not needed now (keys are out of scope) | — |
| `craft-libs/crates/control`, `mcp` | protocol, token file, loopback server, MCP envelopes (no UI toolkit) | — |
| SolidCraft `ui-registry`, `control-egui/screenshot.rs` | registry; screenshot settle rules | — |
| VectorCraft control protocol | input injection rules and budgets | app-specific `ui.*` verbs |
| Graphite `node-graph` (`node-macro`, `graph-craft`, `interpreted-executor`) | node declaration and graph execution design | — |
| Graphite desktop | — | its UI is web: the `desktop` crate uses CEF (`cef = "156"`), winit, wgpu and vello. No egui code to take. |

`docs/upstream-watch.md` (new, short) lists each upstream, what to watch, and
the last revision reviewed.

---

## 7. UI: Blender-leaning, excellent egui

### 7.1 Theme

A `ui::theme` module of **semantic tokens** (panel background, header, widget
inner/outline/selected, text, accent, warning) drives `egui::Visuals`. The
default palette leans to Blender's dark theme: neutral greys, low contrast
chrome, one accent. Measure the values from Blender's own theme preset, then
adjust where Gradiance needs more (scene readability, the 4b-3 art
direction). Review mode (4b-0) can show two palettes side by side for the
owner to choose.

The font question (Blender uses Inter; Gradiance uses egui's fonts + Hack for
glyphs) goes through the glyph-coverage tests in `ui::fonts`; Hack stays as the
fallback for `glyph`.

### 7.2 Widget behaviour (Blender's number field and friends)

| Behaviour | Home | Note |
|---|---|---|
| Drag to scrub; pointer hides and wraps; Shift = 1/10; Esc restores | `widgets::precise_drag_unit`, `labelled_drag` | Esc must not emit an intent. Wrap needs a cursor grab through `ui::host`. |
| Ctrl snaps (whole units, 10° angles, range-based steps) | step rule in `gradiance-units` | pure, unit-tested |
| Type maths (`2*pi`, `90/4`) with units | parser shared with brief 5-3 | one parser for fields, console and kernel |
| Backspace = default, `-` = negate, Ctrl+wheel = step, right-click Copy/Paste/Reset | widgets | |
| Drag down a column of fields, then sideways: all change | widgets + `op.batch` | **one** undo step |
| Collapsible panels; Ctrl+click opens one, closes the rest | `section_header` | |
| F3 search over operations | new `ui::search`, scorer in a lower crate | reads the registry |
| Pie menus | new `ui::pie` | rows are operation ids |
| Status bar: the active tool's keys | new strip | reads the registry's chords |
| Adjust Last Operation panel | over `CommandStack` top | re-invokes the op with new params as one undo step; needs § 3 |

### 7.3 Areas and workspaces

Any dock tile can switch its editor type (outliner, properties, node graph,
console, plot, …) over the `panel.*` operations. Workspaces (Layout,
Scripting, Signals) are saved layouts, stored as **workstation config** (not
scene content, not undo). SolidCraft's `layout.save`, `layout.apply`,
`layout.workspace.select` are the spec. Keep `egui_tiles` and
`dock_sync::sync_panes` (no tree rebuild).

---

## 8. Control, MCP and UI tests on lavapipe

| Tier | What | Needs Xvfb | Catches |
|---|---|---|---|
| T1 | `egui_kittest` accessibility tests (now) + image snapshots (`snapshot`, `wgpu` features; Craft `Screens` harness; a GPU test lock; CI fails if no adapter, as SolidCraft's `SOLIDCRAFT_REQUIRE_GPU`) | no | layout, theme, label regressions |
| T2 | `ui.*` operations in the registry: `ui.inspect`, `ui.click/drag/key/text/wheel` by **widget-registry name** or point, `ui.screenshot` | yes (not tested without) | gestures, shortcuts, tool previews, rendering |
| T3 | `gradiance mcp` on stdio → BRP | as T2 | agent sessions; the owner drives the app from an MCP client |

Input enters inside the app, so the harness faults in `docs/state.md` go away:
keys that do not arrive (no window focus), clicks dropped when held over
0.8 s, hand-measured coordinates. `tools/screenshot/session.sh` keeps Xvfb
and drops `xdotool` and `xwd`. The injection writes Bevy input **messages**,
not `ButtonInput` (the retained 3-5 hazard: headless presses lose
`just_pressed`).

Because `ui.*` are ordinary operations, Lisp scripts, the CLI and MCP all get
them for free, and review mode (4b-0) can script its scenes with them.

---

## 9. Constraints

- **Invariant 4.** egui stays in `gradiance-ui`; `gradiance-op` has no egui.
  Pure logic (step rules, search scorer, parser) goes to lower crates.
- **Invariants 1–3.** Every doorway emits intents; one gesture or one
  invocation is one command; intents hold `StableId`s.
- **Two tiers.** Steel never runs in the frame loop; the graph evaluates in
  compiled Rust.
- **Other agents.** One worker at a time (`docs/state.md`). 4b-0 merged
  (#238). 4b-1 (group op surface) was already in flight when this order was
  set, so it lands first and **U-2 migrates its verbs** (`set-color`, the
  group verbs) onto the registry.
- **Exact pins and `--locked`.** Each upgrade is its own reviewed brief with
  the lockfile diff.

---

## 10. Order (decided 2026-11-17; the owner left it to the planner)

Numbers changed from the first draft of this plan; these are final. Each line
gives the reason for its place.

| # | Item | Why here |
|---|---|---|
| 1 | **4b-1** group op surface (in flight; 4b-0 merged as #238) | Already running; do not disturb it. |
| 2 | **D-0** toolchain 1.97.0 → 1.100.0 | Bevy 0.20 requires rustc ≥ 1.97.1 (crates.io `rust_version`). Clears the lint debt in `orchestration.md` § Recurring. |
| 3 | **D-1** Bevy 0.20 stack (§ 4) | Touches every crate; do it once, before code that U-briefs write. |
| 4 | **U-1** `gradiance-op` + 3 operations + conformance tests | The foundation all later surfaces project from. |
| 5 | **U-2** migrate the rest (4b-1's verbs included); delete `OpSpec` strings, `EditBinding`, Scheme lowering, direct menu intents | Before 4b-2 builds a panel over the group verbs, so the panel is a projection of the registry. |
| 6 | **U-3** widget registry, `ui.inspect`, `ui.screenshot`; **U-4** input injection; **U-5** snapshot tests | Every later visual brief (4b-2, 4b-3, U-7..U-11) is then checkable in a container without `xdotool`. |
| 7 | **4b-2** groups panel | Owner's 4b direction, now on the registry. |
| 8 | **U-6** CLI + MCP server | Thin once U-2..U-4 exist; gives the owner an MCP door before the polish phase. |
| 9 | **U-7** theme tokens with **4b-3** art direction; then **4b-4** | One "look" pass for chrome and scene; review mode shows the A/B. |
| 10 | **Phase 5**: 5-2, 5-3, **U-8** number fields, 5-4, 5-5 | U-8 shares 5-3's parser; 5-4 lands on typed op params. |
| 11 | **U-9** F3 search, status bar, pie menus; **U-10** editor areas + saved workspaces; **U-11** Adjust Last Operation | Pure projections of the registry; last because they need it complete. |
| 12 | **Phase 6** testing & diagnostics | Action matrix over the registry; mutation tests of the scene loader. |
| 13 | Next planning stage: **G-0** one graph model, **3D-0** the 3D plan, **D-2** egui 0.37 | After the roadmap, per the owner. |

Brief files: D-0, D-1 and U-1 are written (`docs/briefs/D-0-*`, `D-1-*`,
`U-1-*`). Each later brief is written at its predecessor's handoff, because
its facts depend on that outcome (scoped-work ruling, `docs/state.md`).

---

## 11. Owner answers (2026-11-17)

- Polish: **yes**, excellent egui. Theme: **lean Blender**, not at the cost
  of other goals. Licensing: **not now**. BUI: **dropped**.
- Snapshot images in git: **yes, when useful.** Visual review for the owner
  goes through review mode (4b-0), not ad-hoc images.
- Dock layouts: the planner may revisit. **Decision:** dock layouts and
  workspaces persist as **workstation config** (like `GridSettings`), in
  U-10. Node-canvas positions wait for G-0, because they become document
  content if the graph becomes the document.
- Keys: **out of scope.** Keep the current bindings. § 3.4 still moves the
  *existing* bindings into the registry (alignment, not new keys);
  `craft-keymap` profiles are dropped from the plan.
- Order: the planner decides (§ 10).

---

## 12. What I did not check

- Bevy 0.20's migration size for this workspace.
- Other pins that Bevy 0.20 moves (e.g. `glam`: Bevy 0.20 uses `^0.33.2`, the
  workspace pins `0.32`).
- Whether Steel accepts one variadic native `%op` with reflected coercion as
  sketched; the generated-prelude design does not depend on it (a fixed-arity
  `register_fn` per arity also works).
- Whether a Bevy app on lavapipe presents frames without Xvfb.
- Bevy 0.19/0.20 screenshot API details.
- No code was compiled. The `#[derive(Operation)]` syntax is a sketch.

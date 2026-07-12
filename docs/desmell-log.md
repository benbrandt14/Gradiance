# De-smell log

Running record of the maintainability pass: pattern → location → action.
Numbers refer to the smell list in the pass brief (1–10 Rust/LLM smells,
11–15 architectural). This log doubles as the house-style guide: entries
marked **clean** document patterns that were *checked and deliberately kept*,
so future sessions don't re-litigate them.

## Fixed

| # | Pattern | Location | Action |
|---|---|---|---|
| 4/11 | One-use plumbing: three parallel tool-registration artifacts (11 `Tool*` enum variants + `tool()` match + `TOOLS` array) that could drift independently | `interaction/input.rs` | Replaced with one data-carrying `EditorAction::Tool(ToolState)` variant; `apply_shortcuts` matches `get_just_pressed()`. A new tool now touches `input.rs` in exactly one place (its key binding). `ToolState` gained `Reflect` (required by `Actionlike`) |
| 13 | Symmetric-but-unused API: `range_selector` written alongside `precise_drag`, zero callers | `ui/widgets.rs` | Deleted (the joint inspector edits `[min,max]` with two `precise_drag`s directly) |
| 2 | Dead legacy compat: `BodyRecord.group: Option<u32>` (pre-hierarchy saves) + fold-in logic on spawn. Provably dead: `groups: Vec<u32>` landed at M14 (v2 era) and the v3 version gate rejects all pre-v3 files, so no loadable file carries the field | `command/snapshot.rs` + 7 construction sites | Deleted the field, the spawn-time fold, and every `group: None` literal |
| 7 (DRY) | `resolve(world, id)` — 3 identical private copies + ~11 inline four-line expansions of the same `IdIndex` lookup + `MissingEntity` mapping | all of `src/command/` | One `pub(crate) fn resolve` in `command/mod.rs`; every command uses it |
| 12 | Two names for one idea: `wrap_pi` (grab.rs) vs `wrap_angle` (motor.rs), both short-way angle wrapping | `physics/grab.rs`, `physics/motor.rs` | Unified as `geometry::wrap_angle` (pure, unit-tested). The two originals differed only at exactly ±π (measure-zero, inside the motors' 0.05 reversal buffer) |
| 12 | Duplicate order-preserving dedup: `dedup_entities` (select.rs) vs `dedup_preserving_order` (selection.rs) | `interaction/` | select.rs now uses `selection::dedup_preserving_order` |
| 10 | Ceremony hack: `let _ = JointCommon::default(); // referenced for doc-link stability` — a runtime no-op keeping an import alive for rustdoc | `physics/joint_sync.rs` | Deleted (with the now-unused import) |
| 12 | Command display names ("Move/rotate", "Edit properties") diverging from intent names, defeating grep | `command/*` | All `GameCommand::name()`s return the shared `intent::name` kebab constants (Phase 1 naming lockstep) |

## Checked and clean (kept deliberately)

- **`interaction/tools/context.rs`** (the DraftTool/ManipTool facade): the
  two traits have 5+ implementors each and are the scripting seam's landing
  pad — not speculative generics (smell 6 does not apply). `HoldState` /
  `GrabState` / `TwistState` are three-variant-with-payload request enums the
  driver interprets; they are the *seam*, not plumbing.
- **`SelectGesture`** (select.rs): a 7-state gesture machine in one resource,
  one `update` entry point. Dense but exactly one concept per state; no
  over-splitting (smell 8 inverse — correctly *not* split into systems).
- **Defensive `let … else { return }` in tool/UI systems**: these guard
  *legitimate* runtime states (cursor off-screen, nothing selected, entity
  despawned between frames), not seam-excluded states. Kept.
- **`guard_dangling_joints`** (joint_sync.rs): looks like a defensive branch
  for a state commands exclude (delete cascades joints), but scripts/tests
  can despawn outside the command path and avian panics on dangling joints —
  a real safety net with a `warn!`, not a silent swallow. Kept.
- **`ClickThrough` evidence model** (click_select.rs): reads commit intents
  as evidence rather than growing per-tool knowledge — the low-coupling
  choice. Kept.
- **`physics::queries`**: post-de-adapter it's a convenience read facade,
  not an abstraction boundary; its five methods all have multiple callers.
  Not smell 6/9.
- **`ui/reflect_grid.rs`**: reflection-driven settings UI with real users
  (settings tabs). Not a pre-plumbed extension point.
- **Command staging pattern** (capture-on-first-apply, replay-on-redo in
  cut/merge/duplicate/array): repeated shape but each captures different
  state; a shared abstraction would be a trait for four call sites (smell 6
  in reverse). Kept as a *pattern*, documented in `command/mod.rs`.
- **`toolbar.rs` `TOOLS` table** (label + key-hint strings): presentational
  UI copy, deliberately not derived from the input map — a rebind currently
  requires updating the hint text, which is acceptable for 11 lines of copy.

## Follow-ups (recorded, not churned now)

- **Plot signals bypass the read facade** (`ui/plot.rs::joint_signals`
  computes joint length/angle in the panel): when the next plottable signal
  lands, move the computation behind `physics::queries` so scripts/probes
  ("sensors") get it too. See `docs/recipe-audit.md` §4.
- `geometry/contour.rs` `let _ = vb;` inside a `debug_assert!`-adjacent
  closure and `render/grid.rs` `let _ = i;` — cosmetic underscore-binding
  choices inside tight loops; not worth diff noise.

## Net LOC

Phase 3 changes are net-negative in `src/` (deletions: tool triple,
range_selector, legacy group field, resolve copies, wrap duplicate, doc-link
hack). The Phase 1 additions (flight recorder, edit-bindings table) are the
justified net-positive: they are the observability deliverable, dev-gated
where runtime-relevant.

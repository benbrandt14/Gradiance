# Chain and rope tool
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Drag a path; release makes N capsule links joined by hinges, one undo
step. Options: link length, width, end pins (to the bodies under the ends,
or to the world).

## Facts (planning thread, 2026-11-19, on `92ab302`)

- Spawn and joint intents exist (`spawn-body`, `spawn-joint`); one
  gesture commits one command (invariant 2), so the command batches the
  links and joints.
- `ShapeDef::Capsule` exists (`geometry::polygonize` ≈ line 41).
- The rod tool (`tools::rod_tool`) is the closest gesture to copy.

## Out of scope

A slack rope as one distance-limit joint (can follow as a mode).

## Done when

- A chain pinned at both ends hangs as a catenary (sag within 5 % of
  the analytic value, behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-n5`.

**Landed:**

- `gradiance-geometry/src/chain.rs` (new): `chain_points(path, link)` splits
  a stroke by arc length into the whole number of links nearest to
  `length / link` (1..=`MAX_CHAIN_LINKS` = 200), ends exactly on the
  stroke's ends; `path_length`, `MIN_CHAIN_LENGTH` (5 cm).
- Domain: `ChainPins { Free, Bodies, World }` and three `ToolDefaults`
  fields, `chain_link` (0.25 m), `chain_width` (0.06 m), `chain_pins`
  (default `Bodies`, like the rod). `ToolDefaults` lost `Eq` (it now holds
  lengths). Workstation config: not saved, never undone.
- Command: `SpawnRodCommand` now holds `bodies: Vec<BodyRecord>` and an
  undo label; `SpawnRodCommand::chain` serves the new `SpawnChainIntent`
  (one row in `command_intents!`, undo label `spawn-chain`). No new
  cloning or spawning logic: records spawn through `BodyRecord::spawn` /
  `JointRecord::spawn` as the rod's do.
- Operation `body.chain` / `(chain path link width a b pins)`
  (`command/src/ops/chain.rs`): path is flat world `x y` pairs; `a`, `b`
  name the bodies the start and end hinge to (a negative index: none);
  `pins` `"free"`, `"bodies"` or `"world"`. Links are rod-like capsules
  (density 5, the rod tone) from hinge point to hinge point; hinges join
  consecutive links at their shared point and pin the ends (bodies by
  `StableId`, the world at the end point). Returns the links' ids.
  `docs/ops.md` regenerated.
- Interaction: `ToolState::Chain`, `chain_tool::ChainDraft` (records the
  stroke every 4 px; preview: the links it would lay, a ring per hinge at
  the link width; release finds the end bodies with the rod's tolerant end
  hit-test), `ToolCommit::Chain` → an `InvokeOp` of `body.chain` (like the
  array gesture), so one gesture is one `spawn-chain` step and Adjust Last
  Operation can re-lay a drawn chain. `tool.chain`, **no key**, status-bar
  hint. No editor key binding changed. The preview systems moved into
  `add_tool_previews` (the plugin's `build` passed clippy's line limit).
- UI: palette entry "Chain" in Connect after Rod (`chain (2).png`, no key);
  Settings > Grid & Snap > Tools shows chain link / chain width (unit
  fields) and a chain-pins drop-down. Tool palette snapshot regenerated
  (canvas 352 → 374 px; only the Chain row is new).
- Tests: `tests/it/chains.rs` —
  `a_chain_pinned_at_both_ends_hangs_as_a_catenary` (the "done when":
  20 links of 25 cm drawn as a V between world pins 4 m apart, viscous air
  on, 10 s of fixed steps: sag 1.360 m against the analytic 1.327 m, 2.5 %;
  hinge gaps under 1 cm; sag unchanged over the last second),
  `a_chain_of_many_links_swings_without_coming_apart` (15 links from a
  static post, no damping, 4 s: finite, hinge gaps under 2 cm, no link past
  25 m/s), `a_chain_between_two_boxes_is_links_and_hinges_in_one_undo_step`
  (tool drag; one step; undo/redo), `the_pin_rule_decides_the_ends`,
  `the_link_length_and_width_come_from_the_tool_defaults_and_adjust_after`,
  `a_too_short_drag_lays_no_chain`,
  `the_operation_refuses_a_bad_path_or_pin_rule`. Unit tests in geometry,
  command and interaction.
- Docs: `docs/scripting.md` (`chain`).

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`chain-path-split`, `chain-options`, `chain-commit-path`, `chain-links`,
`chain-operation`, `chain-tool-key`.

**Learned:** a hanging chain at rest keeps a small solver buzz in one link
next to a world pin (about 0.4 m/s at its centre, steady, with linear drag
on; it neither grows nor moves the shape, and more substeps did not remove
it). The catenary test therefore checks that the sag is settled and the
mean link speed is low, not that every link is still. Worth a look if
chains are used as sensors.

**Not done / follow-ups:** not checked on screen (no screenshot run): the
stroke preview and the icon in the running app. The slack rope (one
distance-limit joint) stays out of scope. Links are not grouped (decision
`chain-links`). A drawn chain's `path` parameter carries every recorded
stroke point (4 px apart), so a long stroke makes a long Adjust Last entry.

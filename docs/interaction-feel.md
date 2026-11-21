# Interaction feel — which reference governs which operation

**Status: owner direction (2026-11-21).** "The feel should be somewhat a mix
of Algodoo, Blender, Illustrator and a CAD tool, based on what operation is
occurring." This note turns that into rules a brief can cite. Read it before
building a tool, a gesture, an on-canvas widget or a readout.
`docs/ui-design.md` stays the design system for chrome (glyphs, fields,
panels); this note covers the *behaviour* of the canvas.

## The rule: the operation in progress picks the feel

There is no global "mode" switch. The feel follows from what the person is
doing *right now*, and it never changes in the middle of a gesture.

| Operation family | Reference | What it feels like |
|---|---|---|
| **Play** — the sim runs; grab, throw, twist, poke, spawn into a live scene | **Algodoo** | Direct and physical. Things react at once, forgivingly; no dialogs, no confirmations; a click that no tool consumes selects (M16). Grabs are springs and torques, not teleports. Mistakes are cheap because a run is one undo step. |
| **Command & navigate** — camera, selection, operations, search, keys, pies, docks, node canvas | **Blender** | Keyboard-first and operation-centred. Everything is a registered operation (F3 finds it, a pie or key runs it); the last one can be adjusted (U-11); a modal transform can take a typed value. Panels are projections of operations, never logic. |
| **Draw & shape** — paths, anchors, handles, corners, contours, arrays, alignment while moving | **Illustrator** | Direct manipulation of the outline. On-canvas widgets sit on the thing they change (corner dots, count and radius handles, width handles); smart guides pull with short labels; a small contextual bar floats by the selection. The outline you see is the outline you get. |
| **Measure & construct** — lengths, angles, dimensions, tangents, 3-point constructors, typed geometry | **CAD** | Numbers first and exact. Snaps go to exact features, never to tessellation (1-4); a typed length or angle during a draft overrides the pointer; dimensions are associative (they follow the bodies) and are live quantities; readouts carry SI units (5-3). |

A tool can span two rows over its lifetime — the Curvature tool *draws* with
Illustrator feel, and a typed segment length during that draft is CAD feel —
but one gesture has one owner: the press decides it.

## Invariants every feel shares

These hold in all four rows; a brief that breaks one says so and the
orchestrator decides.

1. **One gesture, one undo step** (invariant 2). A press-drag-release on a
   widget, a handle or a draft commits exactly one command.
2. **Esc cancels** the gesture in progress and leaves nothing behind.
3. **Typed values work everywhere a number is shown**, with units and maths
   (`gradiance_units::expr`, brief 5-3).
4. **One snap language.** One snapper (brief C-1) serves every tool; a snap
   shows one glyph and, for smart guides, one short label (`anchor`,
   `centre`, `edge`, `tangent`, `grid`, `path`). Glyphs come from
   `ui::fonts::glyph`.
5. **Every canvas action is an operation** reachable from Lisp, remote and
   MCP (`gradiance-op`); a widget is one more control surface over it.
6. **Play never edits authored state silently.** In play, a gesture is
   physics (spring, torque) or it is an edit that pauses and records — never
   a hidden mix.

## How a brief uses this note

A brief that adds canvas behaviour names its row in **Approach** ("feel:
Illustrator — on-canvas widget") and lists which shared invariants it
touches. Reviews check the row's column above. Where a reference app's
convention clashes with an existing Gradiance key or modifier (for example
`Ctrl` is duplicate-drag in the select tool, `Shift` is selection-only), the
Gradiance binding wins and the brief notes the clash.

## Where each reference is studied

- Algodoo: `docs/parity-audit.md` (features described generically).
- Blender: `docs/unified-surface-plan.md` (operations, F3, pies, Adjust Last).
- Illustrator: through **VectorCraft** (`docs/vectorcraft-survey.md`), an
  open-source Rust/egui reimplementation whose code we may port (MIT OR
  Apache-2.0). We port behaviour and code from VectorCraft, never names,
  icons or art of the commercial app.
- CAD: the constructors and snap rules already in M18 (briefs 1-1..1-7) and
  `docs/curvilinear-tools-decision.md`; VectorCraft's Dimension, Measure and
  Path Precision tools for associative dimensions and constructors.

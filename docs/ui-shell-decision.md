# UI shell decision: the egui app shell

Status: **decided (option A) and executed** (opened 2026-07-18). Prompted by
"I am open to other options for UI other than purely egui." We evaluated the
options honestly, chose the egui shell, and built it out (through the
scene-viewport routing). The north star is a **Blender-style UI** (dockable
panels, a T-panel tool strip, an embedded node editor, menu-driven chrome).

## The one hard constraint

The app's heart is a **live wgpu-rendered viewport** (Bevy's render graph owns a
`wgpu::Device`/`Queue` and a winit window). Every framework option is really a
question of *how the UI toolkit coexists with that* — that, not widget richness,
separates "a weekend" from "an R&D project."

## Options considered (record)

- **A. egui as an app shell — CHOSEN.** Keep `bevy_egui`; add `egui_tiles`
  (Rerun's docking). Zero new integration risk, the whole editor already runs on
  it, Rerun proves egui scales to a viewport-centric desktop tool. Cost:
  immediate-mode ceilings for deeply-styled/animated chrome, theming, and
  accessibility.
- **B. Hybrid retained shell (Slint+wgpu / web / gpui).** The real "move off
  egui" path — richest, but compositing a Bevy texture across two render loops +
  input routing is weeks of R&D and ongoing maintenance. Deferred unless a
  concrete wall (rich-text/document editing, deep accessibility, a design system
  egui can't express) forces it.
- **C. `bevy_ui`.** ECS-native, no integration seam, but lacks docking/rich
  editor widgets today. Worth re-checking each Bevy release.

## No-regret growth work (framework-independent, still open)

- ~~**Split into a Cargo workspace**~~ — **done**, and further than proposed here:
  one package per architectural layer, so the layer diagram *is* the crate DAG
  and `tests/boundaries.rs` asserts that DAG as data. See
  `docs/workspace-plan.md`.
- ~~**App-shell architecture**~~ — **done** for panels. `ui::panels::PanelToggle`
  is the trait; `menu::Panels::named` is the registry; the View menu is a loop
  over it, and so are the `panel-show`/`panel-hide`/`panel-toggle` script verbs
  and the `panel-open?` read. "A menu item is a registered op" is literal:
  adding a panel is one row, and it appears in the menu and the scripting API
  together (a test asserts every name has a label, so they cannot drift).

  Note the shape this took. A verb does **not** get its own write path — it
  queues a `PanelRequest` and the UI calls `set_open`, exactly as a menu click
  does. And because panel state lives in `gradiance-ui`, which sits *above*
  `gradiance-script` in the DAG, the read direction (`panel-open?`) is a mirror
  the UI publishes rather than a dependency edge upward. Any later extension of
  the action layer — tools, commands — should copy that asymmetry.

  Still open: the *action* half beyond panels. Menu items that emit intents
  (undo, delete, group) are hand-wired, not registry rows.
- Keep the `dev` dynamic-linking loop; widen `egui_kittest` headless coverage.

## What landed (option A, by accretion)

- **Menu bar** — File/Edit/View/Help, routing to existing intents/toggles.
- **Right dock** (`src/ui/dock.rs`, `egui_tiles`) — Outliner / Depth /
  Signals+Plot / Properties / Script as re-arrangeable, splittable tabs. The
  Properties inspector and the Live Plot are dock panes (their floating windows
  are gone). One `BodyProps` bundle holds the dock's single `PropertyEditIntent`
  writer + `SignalBindings`, so the one dock system stays under Bevy's param cap.
- **Bottom dock** (`src/ui/bottom_dock.rs`, `egui_tiles`) — the Node Graph canvas.
- **Object-tree outliner** with **universal selection** — a tree row, a viewport
  shape, and a node-graph block are the same ECS entity, so selecting one
  highlights all three (via the `SelectTransition` seam).
- **Scene-viewport routing** (stage 2, `apply_scene_viewport`) — the scene
  `Camera3d` renders only into the dock-bounded central pane. This required
  giving egui its **own full-window camera** (`spawn_ui_camera`,
  `RenderLayers::none()`): bevy_egui otherwise attaches the UI to the scene
  camera, so shrinking that camera's viewport oscillated the whole UI. Picking
  needs no changes — Bevy's `viewport_to_ndc` offsets by the viewport origin.
  The pane math (`PanelRects::{top,right,bottom}_inset`, `scene_rect`,
  `scene_viewport`) is pure and unit-tested.
- **Tool strip** — a fixed, translucent left T-panel of image icons
  (from `assets/icons/tool_*.png`, registered as egui textures).

**Next (fresh context):** dock the Probe & Settings pop-ups (needs a right-dock
param restructure — it's at Bevy's 16-param cap), outliner grouping/filters, the
undo/redo overhaul (incl. undoing scene-play), and embedded node-editor polish.

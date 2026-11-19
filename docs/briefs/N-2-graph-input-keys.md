# Keys as node-graph inputs, with settable bindings
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A held key publishes a bus value during play (0/1, or −1/0/1 for a key
pair), so N-1 can steer a motor or a thruster. The person sets the key per
input in the node graph. This is **not** the editor keymap: the owner
ruled that tool hotkeys are not settled (2026-11-17, restated 2026-11-19);
this brief adds no editor key.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- Editor keys use `leafwing-input-manager` (`=0.22.0`,
  `interaction::input`, `EditorAction`). Input bindings for the graph
  reuse that crate; they add no dependency.
- A hovered number field already claims keys (U-8); a focused egui field
  sets `KeyboardCaptured`. Graph inputs must respect both.

## Decisions (best estimate)

1. Bindings are part of the signal graph, so they are **scene content**
   (saved and undoable). Classify them in `scene_content!`
   (`scene::records`).
2. During play a graph-bound key wins over an editor key on the same
   chord; while paused the editor key wins. A chord clash shows a warning
   on the input node.

## Out of scope

Gamepads; the editor keymap.

## Done when

- A scene with a key-driven motor runs from a script that injects the key
  through `ui.key` (behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)

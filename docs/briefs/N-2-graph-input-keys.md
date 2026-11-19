# Keys as node-graph inputs, with settable bindings
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

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

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-n2`.

**Landed:**

- `gradiance-domain/src/keys.rs` (new): the one key-name table (63 keys:
  letters, digits, F1-F12, Enter, Escape, Tab, Space, Backspace, Delete,
  Insert, Home, End, PageUp, PageDown, Up, Down, Left, Right; names as
  egui's `Key::name`, case-insensitive, a few aliases) and `KeySet`, a `u64`
  mask over it. `ui.key`'s chord parser reads key names through it, so a
  saved binding and a script name keys one way.
- `BlockOp::Key { key, negative }` (`domain::signal`): a source block that
  lowers to the bus input `key@<name>` (or `key@+ - key@-` for a pair).
  It lives in `ComputedSignals`, which is already in `scene_content!`, so a
  key binding is saved and one `settings` undo step per edit: no new
  resource, no format change (decision 1).
- `gradiance-interaction/src/graph_keys.rs` (new): `GraphKeys` (bound set
  and the `key@` references, rebuilt only when `ComputedSignals` changes),
  `publish_graph_keys` (between `publish_sensor_refs` and
  `evaluate_computed`; `1`/`0` per reference, derived: never saved, never a
  command), `keys_live` (the gate: `Playing` and no `KeyboardCaptured`),
  `EditorKeys` (the keyboard as editor systems see it: minus graph-owned
  keys while live; used by the camera pan/Home, the gesture X/Y lock, a
  draft tool's Enter/Escape, the pack Escape) and `EditorKeyTable` (the
  editor's bare-key uses, built once from the leafwing input map plus the
  direct keys). `apply_shortcuts` skips an action whose bare key a graph
  input owns while live (decision 2). **No editor key binding changed.**
  New dependency edge interaction → signal (row in `tests/boundaries.rs`;
  `Cargo.lock` gains the one internal line).
- Node graph: Input ▸ **Key** adds a block (E = +1, Q = -1). Its footer
  has a `+` key capture button (click, then press the key; Escape cancels;
  while it listens it claims the keyboard), a **pair** checkbox, a `-`
  key button, and a warning line per editor use of either key ("W also
  picks the Weld tool: in play this input wins"). The output pin shows the
  live value.
- Operations: `signal.key` / `(defkey name key [negative])`; `ui.key`
  takes an optional `hold` (1-600 frames). Positional calls may now leave
  off trailing `Option` fields for every operation (`OpEntry::required`;
  signatures show `[hold]`), so `(ui-key "Escape")` is unchanged.
  `docs/ops.md` regenerated (134 operations).
- Tests: `tests/it/graph_keys.rs` —
  `a_script_holds_keys_that_drive_a_motor_through_a_key_input` (the "done
  when": a script declares `(defkey "throttle" "E" "Q")` and `(defsignal
  "spin" "throttle 4 *")`, bound to a still hinge motor's speed; `(ui-key
  "E" 60)` turns the wheel at +4 rad/s, released it stops, `(ui-key "Q"
  60)` turns it at -4 rad/s; the authored motor never changes and no undo
  step is recorded), `a_bound_key_is_the_editors_while_paused_and_the_inputs_in_play`
  (paused: W picks the Weld tool and the input reads 0; in play: W drives
  the input and the tool stays; B still picks Box),
  `a_field_with_the_keyboard_keeps_the_key_from_the_input`,
  `a_key_input_is_scene_content_with_one_undo_step`. Unit tests in domain
  (key table, Key lowering), op (optional trailing args, signature),
  interaction (bound set, editor table, gate), ui (held `ui.key` frames;
  the capture button binds, claims the keyboard, Escape cancels).
- Docs: `docs/signal-dataflow.md` (key inputs, the derived row),
  `docs/scripting.md` (`defkey`, `ui-key` hold, optional positional args).

**Key gate, stated:** a key input reads its key only while the sim plays
and no text or number field has the keyboard; paused (or stepping) it
reads 0 and the editor keeps every key. In play, a graph-bound key's
bare-key editor uses yield; chords with a modifier still reach the editor.

**Per-frame cost:** `publish_graph_keys` does one `ButtonInput::pressed`
and one bus publish (a scan of the bus names, no allocation once the name
exists) per distinct key reference, plus two resource reads for the gate;
`EditorKeys::claimed` is a table scan of 63 `KeyCode`s and a mask test,
reached only for the few keys editor systems read; `apply_shortcuts` scans
the ~40-row editor table only for an action pressed that frame. Nothing
allocates per frame; the bound set and references are rebuilt only on a
`ComputedSignals` change.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`key-input-as-block`, `key-input-gate`, `graph-key-precedence`,
`interaction-signal-edge`, `ui-key-hold`, `key-input-authoring`.

**Not done / follow-ups:** the capture button and the warning lines were
not checked on screen (no screenshot run). Keys are read as physical codes
(a non-QWERTY layout binds by position); egui reports logical keys to the
capture button, so on such a layout the captured name may differ from the
physical key. Escape cannot be captured (it cancels); `defkey` can bind it.
Binding Space means the keyboard cannot pause during play (the warning
says so; the transport button still works). No `signal.bind` verb yet
(N-1 follow-up); the test binds the motor directly.

# Thruster node
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A placeable node, beside the tracer, that pushes its body along its local
axis at its own point. Strength is authored; N-1 can drive it.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `NodeKind` has one variant, `Tracer` (`gradiance-domain/src/node.rs`
  ≈ line 72). Nodes ride a body or float free, are saved in
  `SceneRecord.nodes`, and copy with the body.
- The node tool lives in `interaction::tools::node_tools`.
- `physics::fields` already applies per-frame external forces; the
  thruster force joins that pass (off-centre force gives torque).

## Out of scope

Exhaust visuals beyond a gizmo flame scaled by thrust.

## Done when

- A thruster on a free body accelerates it at F/m along the node axis
  (behaviour test, gravity off).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-n3`.

**Landed:**

- `gradiance-domain/src/thruster.rs` (new): `Thruster { thrust: Force,
  angle: Angle }` — the push in N (negative pushes backwards) and its axis
  in the body's frame; default 10 N along local +y; `axis`, `force` (a
  non-finite thrust gives no force). `NodeKind::Thruster`, `NodeKind::tracer()`.
  `SignalSink::Thrust(node id)` → `ActuatorTarget::Node`, unit N, label
  "thrust", remapped on duplication. Additive variants: old scenes load, no
  format bump; the v8 golden gains a thruster node (regenerated).
- `gradiance-physics/src/thrust.rs` (new): `apply_thrusters`, one system in
  `FixedUpdate` (once per physics step, `sim_running`), pushes each attached
  thruster's dynamic body with `apply_force_at_point` at the node's offset;
  `SignalDrive::thrust` replaces the authored thrust. Free thrusters, gone
  bodies and non-dynamic bodies push nothing.
- Signal: `desired_drive` fills `thrust`; `drive_actuators` puts the drive
  on the node entity (no change to its loop).
- Command: `PropertyValue::NodeKind` and `NodeRecord::spawn` keep the
  `Tracer` component only on tracer nodes; `DuplicateCommand` adds the
  cloned nodes to the id map before cloning bindings, so a duplicated
  body's thruster keeps its drive. `node.thruster` /
  `(thruster body x y angle thrust)` (world point and angle, stored in the
  body's frame; one `spawn-node` step). `docs/ops.md` regenerated (136).
- Interaction: `ToolState::Thruster`, `ThrusterTool` (press where it
  pushes; a click pushes up, a drag over 8 px aims), `tool.thruster` with
  **no chord** (the macro takes `""` for no key). No editor key binding
  changed.
- Render: the thruster glyph draws a flame against the push, 18 px at the
  default thrust, up to 4x, using the driven thrust when bound. All node
  glyph sizes are now screen pixels via `CameraScale` (they were pixel-era
  numbers drawn as metres).
- UI: palette entry "Thruster" (Modify group, `thruster.png`, no key);
  outliner label; thrust and axis fields (edit seam, one undo step per
  gesture) in the inspector, context menu and Signals list; a **thruster
  block** on the node canvas (one input pin, N) shown while a wire names it
  or its body is on the canvas; `ports::thruster_actuators`; the Signals
  list offers thrust or plot for a thrust binding.
- Tests: `tests/it/thrusters.rs` —
  `a_thruster_accelerates_a_free_body_at_f_over_m_along_its_axis` (the
  "done when": gravity off, 3 N at 30 degrees through the centre of a 2x1 m
  box; measured acceleration within 2% of F/m along the axis, no spin),
  `an_off_centre_thruster_turns_its_body`,
  `a_thruster_without_a_dynamic_body_pushes_nothing`,
  `a_thrust_edit_is_one_undo_step`, `a_signal_drives_the_thrust_until_unbound`,
  `the_thruster_operation_attaches_in_the_body_frame`,
  `duplicating_a_body_copies_its_thruster_and_its_drive`. Unit tests in
  domain, physics, signal, interaction (aim), render (flame scale).
- Docs: `docs/signal-dataflow.md` (thruster section, rows),
  `docs/scripting.md` (`thruster`).

**Per-frame cost:** `apply_thrusters` is `O(T)` over the behavior nodes:
per thruster one `IdIndex` hash probe and one `get_mut` on the body query,
a few multiplies; no allocation. It runs once per physics step, not per
frame. The glyph adds one body lookup per node (render only).

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`thruster-axis-frame`, `thruster-tool`, `thruster-fixed-step`,
`thrust-actuator`, `thruster-operation`, `node-glyph-screen-size`.

**Not done / follow-ups:** not checked on screen (no screenshot run): the
flame, the palette icon and the thruster block. No exhaust visuals beyond
the gizmo flame (out of scope). Still no `signal.bind` verb (N-1 follow-up).
A paused `sim.step` run that owes fewer steps than fixed ticks in a frame
could apply a push on a tick with no step (the same edge the field forces
have).

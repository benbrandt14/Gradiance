# Thruster node
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

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

(empty)

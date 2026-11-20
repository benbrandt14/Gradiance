# Nodes

<!-- Generated from `gradiance_graph::CATALOG` by `crates/gradiance-graph/src/tests.rs` (`docs_nodes_md_is_current`). Do not edit by hand: run `UPDATE_NODES_DOC=1 cargo test -p gradiance-graph docs_nodes_md_is_current`. -->

Every node kind of the one graph (`docs/graph-model-decision.md`): 21 rows. A node is not an operation; the `graph.*` operations (`docs/ops.md`) edit the graph. **Rate**: a frame-rate node runs every frame through the signal kernel; an edit-rate node changes only on a settled edit, and an edit-rate input never takes a frame-rate output. An input port reads its default while nothing is bound to it; a constant must lie in its hard range.

## Signal: Sources

| Id | Name | Rate | Inputs | Outputs | Doc |
|---|---|---|---|---|---|
| `signal::time` | Time | frame | — | `out`: Scalar = 0 | The elapsed-seconds clock t (the external bus input "t"). |
| `signal::constant` | Constant | frame | `value`: Scalar = 0 | `out`: Scalar = 0 | A fixed value. |
| `signal::oscillator` | Oscillator | frame | `amp`: Scalar = 1; `freq`: Scalar = 1 | `out`: Scalar = 0 | amp * sin(freq * t), freq in radians per second. |
| `signal::key` | Key | frame | — | `out`: Scalar = 0 | 1 while the key is held in play; with a negative key, +1, -1 or 0. Reads the key@<name> bus inputs. |
| `signal::param` | Param | edit | — | `out`: Scalar = 0 | A tunable parameter (defparam): an edit-rate scalar that also publishes each frame under its label, so it may feed both families. |

## Signal: Math

| Id | Name | Rate | Inputs | Outputs | Doc |
|---|---|---|---|---|---|
| `signal::gain` | Gain | frame | `in`: Scalar = 0; `k`: Scalar = 1 | `out`: Scalar = 0 | in * k. |
| `signal::sum` | Sum | frame | `a`: Scalar = 0; `b`: Scalar = 0 | `out`: Scalar = 0 | a + b. |
| `signal::product` | Product | frame | `a`: Scalar = 0; `b`: Scalar = 0 | `out`: Scalar = 0 | a * b. |
| `signal::sub` | Sub | frame | `a`: Scalar = 0; `b`: Scalar = 0 | `out`: Scalar = 0 | a - b. |
| `signal::abs` | Abs | frame | `in`: Scalar = 0 | `out`: Scalar = 0 | \|in\|. |
| `signal::min` | Min | frame | `a`: Scalar = 0; `b`: Scalar = 0 | `out`: Scalar = 0 | The lesser of a and b. |
| `signal::max` | Max | frame | `a`: Scalar = 0; `b`: Scalar = 0 | `out`: Scalar = 0 | The greater of a and b. |
| `signal::curve` | Curve | frame | `in`: Scalar = 0; `curve`: Curve = identity | `out`: Scalar = 0 | Reshapes in through a response curve over [0, 1]. |
| `signal::expr` | Expression | frame | one scalar per name the expression reads | `out`: Scalar = 0 | A console defsignal expression; one scalar port per name it reads, in first-appearance order. |

## Shape: Primitives

| Id | Name | Rate | Inputs | Outputs | Doc |
|---|---|---|---|---|---|
| `shape::box` | Box | edit | `width`: Scalar = 1, at least 0.0001; `height`: Scalar = 1, at least 0.0001 | `shape`: Shape = 1 m box | An axis-aligned rectangle, full width and height in metres. |
| `shape::circle` | Circle | edit | `radius`: Scalar = 0.5, at least 0.0001 | `shape`: Shape = 1 m box | A circle of radius metres. |
| `shape::polygon` | Polygon | edit | `outline`: Points = 4 points; `holes`: Contours = none | `shape`: Shape = 1 m box | A simple polygon: a counter-clockwise outline and clockwise holes. |
| `shape::capsule` | Capsule | edit | `half_length`: Scalar = 0.5, at least 0.0001; `radius`: Scalar = 0.1, at least 0.0001 | `shape`: Shape = 1 m box | The segment from -half_length to +half_length on x, inflated by radius. |
| `shape::half_plane` | Half-plane | edit | — | `shape`: Shape = 1 m box | The infinite ground: solid below local y = 0. |

## Shape: Combine

| Id | Name | Rate | Inputs | Outputs | Doc |
|---|---|---|---|---|---|
| `shape::csg` | Boolean | edit | `lhs`: Shape = 1 m box; `rhs`: Shape = 1 m box | `shape`: Shape = 1 m box | A CSG boolean (union, subtract, intersect, xor, smooth union) of lhs and rhs. |
| `shape::placed` | Place | edit | `shape`: Shape = 1 m box; `pos`: Vec2 = (0, 0); `rot`: Scalar = 0 | `shape`: Shape = 1 m box | A subtree moved to pos and rotated by rot radians (an isometry). |

# A named catalog of physical quantities
Phase 5 · units P3 · "`PhysicalQuantity` catalog + signal/plotter binding …
*Remaining:* a fuller catalog keyed by name (readers over
`physics::queries`) for node-graph ports and set-aggregate readers."
(`docs/units-decision.md` § Phasing 4) · status: in progress (after 5-1)

## Goal

One catalog, keyed by a stable name (e.g. `"speed"`, `"kinetic-energy"`,
`"angle"`), lists each body quantity the app can read. Each entry has:
- a name;
- a `Dimension`;
- a short description;
- a pure reader over `physics::queries`.

These read from it instead of keeping their own lists:
- the signal sources;
- the node-graph's sensor ports;
- a script query verb, e.g. `(quantity name i)` and `(quantities)`.

It is the "one table, several surfaces" pattern (`Panels::named`, the action
table) applied to reads.

## Revision (orchestrator, 2026-11-18)

The brief predates the operation registry (U-1..U-6). Measured on the A-2
head (`90dfc61`), it maps onto today's code like this:

- **Where the catalog lives.** The readers become pure functions over two
  small read traits (`BodyRead`: pose, velocity, mass, inertia, contact
  force, contact count; `JointRead`: anchor length/angle, reaction force),
  so the lowest crate that can hold them is **`gradiance-domain`**
  (`domain::quantity`), next to `SignalSource`, which then takes its name
  and `Dimension` from the catalog instead of a second list. `units`
  could hold the functions but knows no body, joint or `SignalSource`.
  `physics::queries` implements the traits over the live world (`LiveBody`,
  `LiveJoint`), so the frame-loop path is still one query per read, with
  no allocation and no VM. The facade's derived reads (`kinetic_energy_of`,
  `momentum_of`, `angular_momentum_of`) use the catalog's formulas. No new
  DAG edge.
- **How scripts, remote, CLI and MCP read it.** As `Seam::Query`
  operations in `gradiance-command` (`ops/quantity.rs`), the layer that owns
  the scene queries and target resolution: `quantity.list` `(quantities)`,
  `quantity.unit` `(quantity-unit name)`, `quantity.read`
  `(quantity name i)` (a body or a joint, per the entry's subject) and
  `quantity.between` `(quantity-between name a b)` (pair entries). Every
  doorway (Lisp, `gradiance.op`, `gradiance op`, MCP) gets them from the
  registry. Reads go through a context mirror, `QuantitySamples`, captured
  once per context by `gradiance_signal::sample_quantities` (signal
  already depends on physics, command on signal), so "one snapshot per
  run" holds. An unknown name is an `OpError::Invalid` that lists the
  names.
- **Node-graph and plotter binding today.** `ui::ports::body_sensors` /
  `joint_sensors` feed the inspector rows and the canvas's body-block output
  pins; `node_graph::sensor_index` keeps a second, hand-ordered copy;
  `plot.rs` labels a series with `SignalSource::dimension()`;
  `signal::read_source` is the one per-variant reader. After this brief the
  port lists, the pin index, the plot unit and `read_source` are all
  projections of the catalog. `SignalSource` keeps its variants (they are
  the save format); a variant names its catalog entry.

## Read first

- `docs/units-decision.md` § Architecture (`catalog.rs` is sketched there)
  and § Phasing 4.
- `crates/gradiance-domain/src/signal.rs` ≈L300–340:
  `SignalSource::dimension()`, the first slice.
- `crates/gradiance-physics/src/queries.rs`: the read facade (typed
  returns since P2·types).
- `crates/gradiance-ui/src/ports.rs` and `node_graph.rs`: where sensor
  ports are listed today.
- `crates/gradiance-script/src/registry.rs` and `docs/scripting.md` § adding
  a verb; CLAUDE.md "reads are total".
- Brief 5-1's Outcome.

## Facts (orchestrator, 2026-11-16, main `3b8023a`)

- The catalog's home is constrained by the DAG:
  - readers need `physics::queries`;
  - signal (`signal → physics`) and script can use it;
  - `gradiance-units` cannot (bottom node).
  *Unverified:* whether it lives in `gradiance-physics` (readers next to
  the facade) or `gradiance-signal`. Pick the lowest crate that can hold
  the readers; say why. No new DAG edge without a `tests/boundaries.rs` row
  and a reason in the PR.
- Readers are per-body pure functions. Set aggregates (sum/mean over a
  group) are a Horizon item (`roadmap.md` § Grouped node behaviors). Design
  the reader signature so an aggregate can wrap it later. Don't build
  aggregates.
- Tier rule: readers run in the frame loop for live plots. They must be
  allocation-free and VM-free.

## Approach

1. Catalog type + entries for every quantity `SignalSource` and the ports
   expose today, plus any reader `physics::queries` already offers.
2. `SignalSource` and ports bind to catalog names. A no-drift test asserts
   every port/source maps to an entry and every entry's dimension matches.
3. Query verbs with `OpSpec` rows. Unknown name: a visible script error.
4. A screenshot of the node-graph sensor port list or a live plot labelled
   from the catalog.

## Out of scope

Set aggregates, new sensors beyond what exists, unit-aware display/input
(5-3), DSL units (5-4), `docs/state.md`.

## Done when

- Catalog with tests (no drift; dimensions; one reader per entry checked
  against a known body state).
- Verbs documented in `docs/scripting.md`.
- `units-decision.md` P3 and the roadmap line updated. Gates green.
- Screenshot in the PR. Outcome filled.

## Outcome

*(worker fills in)*

# A named catalog of physical quantities
Phase 5 · units P3 · "`PhysicalQuantity` catalog + signal/plotter binding …
*Remaining:* a fuller catalog keyed by name (readers over
`physics::queries`) for node-graph ports and set-aggregate readers."
(`docs/units-decision.md` § Phasing 4) · status: in review (after 5-1)

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

Branch `claude/project-thread-tw1mvl-5-2`, stacked on A-2 (#261) → #260 →
#259 → #258 → #257.

### Landed

- **The catalog:** `gradiance_domain::quantity::QUANTITIES`, 14 rows (10
  body, 1 pair, 3 joint). Each row has a stable name, a port label, a
  `Dimension`, a description and a `Reader` (a plain `fn` over the
  `BodyRead` or `JointRead` trait). `quantity(name)`, `quantities_of(subject)`
  and `known_names()` look rows up. The shared formulas (`kinetic_energy`,
  `momentum`, `angular_momentum`) live here; the physics facade's
  `kinetic_energy_of` / `momentum_of` / `angular_momentum_of` call them.
- **Live reads:** `PhysicsQueries::body(entity, position, dt)` →
  `LiveBody` and `PhysicsQueries::joint(entity, index)` → `LiveJoint`
  implement the traits. Each trait method is one facade read: no
  allocation, no VM.
- **Signal sources:** a `SignalSource` variant names its row
  (`quantity_name`, `subjects`, `reading`, inverse `of_quantity`,
  `binds_quantity`). `dimension()` is the row's dimension.
  `signal::read_source` is now a dispatch on the row's reader.
- **Ports and panels:** `ui::ports::body_sensors` / `joint_sensors` are
  the catalog's rows that a source can bind, in catalog order, with the
  rows' labels. The inspector's mass row reads the `mass` row.
  `node_graph::sensor_index` is the position in that list (the second,
  hand-ordered copy is gone), and `source_pin` / `binding_bodies` use
  `reading()`. The signals panel's "add" buttons, its row headers and its
  plot-to-colour target come from the catalog. The plot already took its
  unit from `dimension()`.
- **Operations** (`gradiance-command/src/ops/quantity.rs`, `Seam::Query`):
  `quantity.list` `(quantities)`, `quantity.unit` `(quantity-unit name)`,
  `quantity.read` `(quantity name i)` (a body, or a joint for a `joint-*`
  row) and `quantity.between` `(quantity-between name a b)`. They read the
  `QuantitySamples` mirror, which `gradiance_signal::sample_quantities`
  captures once per context (command's filler). An unknown name, or a row
  read with the wrong verb, is an `OpError::Invalid` that shows the right
  call. `docs/ops.md` is regenerated.
- **Docs:** `docs/scripting.md` (verb reference), `docs/units-decision.md`
  (P3 landed, the architecture sketch, the extensibility contract),
  `docs/roadmap.md` (P3 ticked).

### Decisions (worker, within scope)

- **Home: `gradiance-domain`, not `physics` or `signal`.** Once the readers
  became functions over read traits, the lowest crate that can hold them is
  the one that also holds `SignalSource`. Then `dimension()` reads the row,
  so there is no second dimension list to drift. `units` could hold the
  functions but has no body, joint or source. No new DAG edge: command
  reaches the live reads through `gradiance_signal::sample_quantities`
  (command → signal → physics already exist).
- **Reader shape:** `Reader::{Body, Pair, Joint}`, not a `&[StableId]`
  slice. Arity is checked by type, and an aggregate can call a `Body`
  reader once per member. A per-body reader over a `&dyn BodyRead`
  calls only the facade reads it needs.
- **Script-only rows:** `mass` is a catalog row with no `SignalSource`
  variant. Adding a variant changes the save format, and the brief rules
  out new sensors. So `mass` is readable from scripts and in the inspector,
  but it is not a port or a plot binding. `binds_quantity` filters it out
  of the port lists and the add buttons.
- **Labels:** a port keeps its short label (`pos x`, `force`, `ang mom`).
  The signals panel shows the row name with spaces (`kinetic energy`,
  `angular momentum`, `contact force`) and names a new binding after the
  row (`contact-force-1`, not `contact force-1`).
- **Verbs:** four, not two. `(quantity name i)` keeps the brief's form.
  The pair row gets its own verb because the Lisp doorway has exact
  arity. `quantity-unit` makes the dimension reachable from every doorway.
- **Snapshot:** the operations read a mirror captured at context build, as
  `body-x` does. They do not read the live world mid-run, so scripting
  rule 3 holds.

### Measured

- Every catalog reader checked against a known state
  (`quantity::tests::every_reader_reads_a_known_state`). The test fails
  if a row has no expected value.
- No drift: `signal::tests::every_scene_source_names_a_catalog_row_of_its_shape`
  (every variant has a row of the right subject, the inverse round-trips,
  the dimensions match, the 13 rows are distinct) and
  `ports::tests::every_port_is_a_catalog_row`.
- End to end (`tests/it/quantities.rs`): height, pos-x, distance and mass
  read through the operations. A falling body's `(quantity "speed" id)`
  equals its plot binding's bus value to 1e-5. A slider reads
  `joint-length` 0.5 m. A wrong name is an error in the console log.
- Screenshots (not in git; `/mnt/project-files/5-2/`):
  `node-graph-ports.png` (the body block's output pins, labelled and
  unit-suffixed from the catalog), `inspector-sensors.png` (the
  Properties sensor rows with units, the mass row, and the signals panel's
  add buttons from the catalog), `plot-units.png` (live plot series
  `force@… [N]`, `height@… [m]`).

### Not checked

- The full `cargo test --workspace` (CI runs it). Local runs: listed below.
- A set aggregate over a real group (not built, Horizon).

### Cut (for later briefs)

- `SignalSource` as `{quantity: name, subjects}` instead of one variant
  per row. This would make a new plottable sensor one catalog row, but it
  changes the save format.
- Rows the facade could offer but no `Dimension` names: moment of inertia
  (kg·m²), and the joint motor ceiling (N·m or N, depending on the joint
  kind). `is_sleeping` is a flag, not a quantity.
- Set aggregates (sum/mean/min/max over a group), per the brief.

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p` domain, physics,
signal, command, ui (lib); `cargo test --test it` filtered to `quantities::`,
`op_conformance::`, `signals::`, `nodes::`, `joint_sensors::`,
`script_examples::`, `scripting::`, `registry_validation::`, `remote::`,
`client::`, `physics_sync::`, `ui_panels::`, `ui_registry::`,
`ui_snapshots::`. All green. rustc is 1.100.0 (the pin).

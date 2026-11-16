# A named catalog of physical quantities
Phase 5 · units P3 · "`PhysicalQuantity` catalog + signal/plotter binding …
*Remaining:* a fuller catalog keyed by name (readers over
`physics::queries`) for node-graph ports and set-aggregate readers."
(`docs/units-decision.md` § Phasing 4) · status: ready (after 5-1)

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

# One declaration per operation: the `gradiance-op` registry
Phase U · `docs/unified-surface-plan.md` § 3 ("MCP, the command line, the UI,
the Steel Lisp dialect and the procedural graph are one integrated system …
that cannot become misaligned from itself", owner 2026-11-17) · status: ready
(after D-1)

## Goal

A new bottom crate `gradiance-op` holds the operation trait, its metadata, a
`Target` reference type, the `OpContext` container and the registry. **Five
operations** go through it end to end, at least one per category that
`gradiance-script` can own: `body.spawn_box`, `body.place`, `edit.undo`
(edit), `body.count` (query), `sim.set` (config). For those five, the Lisp
verb, the BRP call and (for undo) the menu row are **generated or dispatched
from the registry**, and generated conformance tests prove that every doorway
lowers the same call to the same output. The old paths for the other verbs
stay as they are (U-2 moves them).

Revised 2026-11-17 (plan § 13). The earlier text had three edit operations
and a snapshot type inside `gradiance-op`.

## Read first

- `docs/unified-surface-plan.md` § 3 (all), § 5 (what must stay open for
  the graph) and **§ 13** (the revision; it wins over § 3 where they differ).
- `crates/gradiance-script/src/registry.rs`: `name`, `OpSpec`,
  `OpCategory`, `OperationCatalog` (what the registry replaces).
- `crates/gradiance-script/src/bridge.rs`: `SceneView` (line 558),
  `EditBinding` (775), `edit_bindings` (798), `register_edit_verbs` (1021),
  the `place` verb (1593): how a verb reads the snapshot for old values.
- `crates/gradiance-script/src/remote.rs`: `gradiance.op` lowering JSON args
  to Scheme text (to be replaced for the three operations).
- `crates/gradiance-command/src/dispatch.rs` (`command_intents!`) and
  `intent.rs` (`SpawnBodyIntent`, `CommitTransformIntent`, `UndoIntent`).
- `tests/boundaries.rs` (`the_crate_dag_matches_the_architecture`, line
  181): a new crate needs a row there and in `CLAUDE.md`'s layer diagram.

## Facts (planner, 2026-11-17)

- Intents are low-level: `SpawnBodyIntent { record: BodyRecord }`,
  `CommitTransformIntent { changes: Vec<TransformChange> }` holds old **and**
  new poses. Thus an operation **lowers** to an intent against a snapshot; it
  is not the intent (plan § 3.1).
- The menu writes `UndoIntent` directly (`gradiance-ui/src/menu.rs:207`).
- Existing drift tests to keep green and later replace:
  `tests/it/registry_validation.rs` (every catalog op is a bound VM symbol;
  every Edit op maps to a registered reflected intent), `tests/it/remote.rs`,
  `tests/it/reflect_intents.rs`.
- The scripting rules in `CLAUDE.md` § Scripting (one snapshot per run; reads
  never reach up; a verb queues through an existing seam) apply to every
  doorway, not only Lisp.
- The catalogue has 61 rows: 25 edit, 25 query, 2 config, 9 editor
  (`registry.rs`, counted 2026-11-17). The trait must fit all four; that is
  why `lower` returns `OpOutput` (messages to write + a return value), not an
  intent (plan § 13.1).
- `SceneView` (`bridge.rs:558`) holds `ShapeDef`, `BodyPhysics` and contact
  counts read from avian. It cannot move into a crate that depends on `core`
  and `units` only (plan § 13.2).
- `spawn-box` mints a random `StableId` inside the verb and derives the
  colour from it (`body_record`, `Appearance::for_id`). Two doorways thus
  never emit equal intents unless the id comes from the context.
- *Unverified:* whether serde decodes the `gradiance-units` quantity newtypes
  from the JSON a client would write. Check it before the params are typed.
- *Unverified:* whether Steel can register one native variadic `%op`
  builtin. The plan's fallback: a generated Scheme prelude of `define`s that
  call a fixed-arity native per operation. Decide by trying.

## Approach

1. **Crate.** `crates/gradiance-op`, depending on `gradiance-core` and
   `gradiance-units` (+ `bevy` for `Reflect`/`Resource`, `serde`). It holds:
   - `OpMeta` (id, Lisp name, doc, seam, menu path, default chord, undo
     label) and the `Operation` trait (params type, `lower(params,
     &OpContext) -> Result<OpOutput, OpError>`);
   - `OpOutput`: zero or more reflected messages for the registry to write
     to their existing queues, plus an optional `serde_json::Value` to return;
   - `Target { Index, Id(StableId), Selection, At }` and its resolution
     contract (the doorway resolves; intents hold only `StableId`s);
   - `OpContext`: a type-keyed container of read-only mirrors
     (`ctx.get::<T>()`) plus the source of new `StableId`s. It names no
     mirror type;
   - `OpRegistry` resource with type-erased entries: decode params from a
     **named JSON object** (the one decoder, plan § 13.3), `lower`, and write
     the output.
   `SceneSnapshot` (today's `SceneView`, the parts the five operations read)
   moves from `gradiance-script` down to `gradiance-command`, with the one
   builder function that every doorway calls. The query verbs that stay in
   `bridge.rs` read the same value; do not keep two snapshot types.
   A derive macro is optional in this brief; a declarative `macro_rules!` or
   plain trait impls are fine if they keep "declare once".
2. **Registration by owner.** `gradiance-command` registers the three
   `body.*` operations and `edit.undo` at app build; the crate that owns the
   `SimSettings` config write registers `sim.set`. No doorway names their
   types.
3. **Doorways for the three operations.**
   - Lisp: verbs generated from the registry (prelude or native), replacing
     the hand-written closures and `EditBinding` rows of the five. Today's
     verb names and argument order stay valid. The Lisp-specific code is
     `SteelVal → serde_json::Value` and the positional-to-named mapping; it
     has no decoder of its own.
   - BRP: `gradiance.op {id, params}` decodes named JSON params with serde for
     registry operations; other names keep today's Scheme path until U-2.
   - UI: the Edit menu's Undo row invokes `edit.undo` through the registry.
4. **Conformance tests** (generated over the registry, plan § 3.5): doorway
   equivalence (Lisp, BRP JSON, UI invoke lower to equal outputs against one
   context that has a fixed id sequence and a fixture scene), schema round
   trip, every menu row names a registered id (for the migrated rows), one
   undo step per edit invocation, and no undo step for the query.
5. Update `CLAUDE.md` (layer diagram, "adding a verb" recipe pointer) and
   `docs/scripting.md` (the recipe, for migrated operations).

## Out of scope

Migrating any other verb, menu row or key (U-2); any change to
`EditorAction` or the leafwing `InputMap` (plan § 13.4); a proc-macro crate
(use `macro_rules!` or plain impls; U-2 decides with 61 rows of evidence);
the CLI and MCP server (U-6); `ui.*`/`panel.*` operations (U-3); the graph (G-0); key changes
(owner: keys out of scope); new user-visible behaviour.

## Done when

- `gradiance-op` exists with a DAG row in `tests/boundaries.rs` and
  `CLAUDE.md`.
- The five operations work from Lisp and BRP (undo also from the menu), with
  today's behaviour; existing tests stay green.
- One snapshot type and one decoder exist for the migrated operations.
- The conformance tests exist, iterate the registry (not a hand list), and
  fail when a doorway is broken (show one failing case in the Outcome).
- fmt, clippy, doc, `cargo test --workspace`; CI green.
- The Outcome states what U-2 needs: the migration list by category, and any
  design change to the plan's § 3.

## Outcome

(filled by the worker)

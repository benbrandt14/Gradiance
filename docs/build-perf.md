# Build & test performance

4-core Linux container, rustc 1.97, `lld`, dev profile opt-level 1 (deps 3,
`line-tables-only`). Use as ratios, not absolutes. The table below was measured
**before** the workspace split (see the section after it); the shape of the
numbers still holds, but the single-crate figures now describe the root package
rather than everything.

| Metric | Value |
|---|---|
| Cold `cargo test --no-run` | ≈ 16 min (1350 unique crates) |
| Warm `cargo check` after touching `src/lib.rs` | ≈ 3.2 s |
| Incremental test cycle after a src touch | ≈ 34 s (link-bound: lib test + `it` binaries) |
| Full suite runtime, warm | ≈ 13 s (77 lib + 5 boundaries + 125 it + 8 doc) |

LOC (base `3fea9d4` → the de-smell head): `src/` 16 538 → 17 016. The de-smell pass is
net-negative; the additions are the dev-gated flight recorder (437 lines,
zero default-build cost) and the validation tables/trace docs the
observability phase requires (`docs/desmell-log.md` § Net LOC).

## Workspace split — executed

The split landed, and it went further than the `gradiance-core` cut proposed
here: **one package per architectural layer**, so the layer diagram *is* the
crate DAG and a boundary violation is a compile error rather than a review
comment. `docs/workspace-plan.md` records the rationale and the coupling data
behind the cut lines.

What it bought, against the gate above:

- **Pure-math tests skip the Bevy link entirely**, which was the original
  motivation. `cargo test -p gradiance-kernel` and `-p gradiance-geometry`
  run in seconds because neither package links the engine (`kernel` has no
  bevy dependency at all).
- **Touching a leaf layer no longer rebuilds the app.** Editing
  `gradiance-geometry` recompiles it and its dependents, not everything.

What it cost, and the container consequence to plan around:

- `cargo test --workspace` now links **35 Bevy-sized binaries** rather than a
  handful. That is fine on a real machine and rough in a small container: a
  full run can exhaust the writable allowance. The cheap recovery is to purge
  `target/debug/deps/*gradiance*` (reclaims ~16 GB) rather than
  `cargo clean`, which would also discard the third-party build.
- Per-package runs (`-p`) are the everyday loop; `--workspace` is a gate, not
  an inner loop.

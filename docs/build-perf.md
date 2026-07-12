# Build & test performance

4-core Linux container, rustc 1.97, `lld`, dev profile opt-level 1 (deps 3,
`line-tables-only`). Use as ratios, not absolutes.

| Metric | Value |
|---|---|
| Cold `cargo test --no-run` | ≈ 16 min (1350 unique crates) |
| Warm `cargo check` after touching `src/lib.rs` | ≈ 3.2 s |
| Incremental test cycle after a src touch | ≈ 34 s (link-bound: lib test + `it` binaries) |
| Full suite runtime, warm | ≈ 13 s (77 lib + 5 boundaries + 125 it + 8 doc) |

LOC (base `3fea9d4` → head): `src/` 16 538 → 17 016. The de-smell pass is
net-negative; the additions are the dev-gated flight recorder (437 lines,
zero default-build cost) and the validation tables/trace docs the
observability phase requires (`docs/desmell-log.md` § Net LOC).

## Workspace split — proposed, not executed

The check loop (3 s) doesn't need a split; the pain is the ~34 s link-bound
test cycle. Cheaper levers first: test filters on the umbrella `it` binary,
`--features dev` (dynamic_linking) for the run loop. If still wanted, the
natural cut is a `gradiance-core` crate (`core/`, `geometry/`,
`script/kernel` + `reflect_bridge`) so pure-math tests skip the Bevy link
entirely. Decision gate: prototype on a branch with `cargo build --timings`;
adopt only if the incremental test cycle improves >30%.

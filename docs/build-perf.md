# Build & test performance — measurements from the maintainability pass

Environment: 4-core Linux container, rustc 1.97.0, `lld` via
`.cargo/config.toml`, `profile.dev` opt-level 1 (deps at 3,
`line-tables-only` debuginfo). Numbers are wall-clock on this hardware; use
them as ratios, not absolutes.

## Measurements (after the pass)

| Metric | Value | Notes |
|---|---|---|
| Cold `cargo test --no-run` | ≈ 16 min | full dependency graph (1350 unique crates) + all test binaries |
| Warm `cargo check` after touching `src/lib.rs` | ≈ 3.2 s | the type-feedback loop is healthy |
| Incremental test cycle (`cargo test --test it <filter>` after a src touch) | ≈ 34 s | dominated by re-linking the two Bevy-sized binaries (lib test + `it`) |
| Full test suite, warm | ≈ 13 s runtime | 77 lib + 5 boundaries + 125 it + 8 doc |
| Dependency graph | 1350 unique crates | engine-facing deps exact-pinned (now asserted by `tests/boundaries.rs`) |

What this pass changed for build time: nothing structural (deliberately —
see scope). It added one dev-dependency (`egui_kittest`, small, shares the
already-built egui 0.35) and kept the dev-only flight recorder behind the
`dev` feature so the default build is unaffected. CI now reports the
`rust-cache` hit state so a silent cache-key rotation shows up in the log
instead of as a mystery 3× CI time.

## LOC accounting (base `3fea9d4` → head)

| Tree | Before | After | Delta |
|---|---|---|---|
| `src/` | 16 538 | 17 052 | **+514** |
| `tests/` | 4 487 | 5 082 | +595 (replay goldens harness, UI tests, registry validation) |

`src/` net is **positive**, which the pass brief requires justifying
(`docs/desmell-log.md` § Net LOC): the de-smell pass itself deleted more than
it added (−321 deleted lines across 34 files include the tool-registration
triple, `range_selector`, the legacy `group` field, three `resolve` copies,
a duplicate angle-wrap). The additions are the two Phase-1 deliverables:
the flight recorder + RON dump (437 lines, **dev-feature-gated** — zero cost
in default builds) and the intent trace/naming-lockstep docs + the
`edit_bindings` table (~80 lines that exist to be validated by CI). Excluding
the dev-gated instrumentation, `src/` is ~+77 for the whole pass, all of it
seam documentation and the validation table.

## Workspace split — proposed, not executed

The brief forbids executing a split without `--timings` evidence; the
evidence gathered here says the **check loop does not need it** (3 s) and the
pain is **link time in the test cycle** (~34 s). A split should therefore be
judged on link/dev-loop impact, not check time:

1. **First, cheaper levers** (in order): keep using the single umbrella `it`
   binary (already done); consider `cargo test --test it <module>::` filters
   in the inner loop (already supported); use `--features dev`
   (dynamic_linking) for the run-loop, which skips the static Bevy relink.
2. **If a split is still wanted**, the seam the boundaries tests already
   enforce is the natural cut: a `gradiance-core` crate (`core/`, `geometry/`,
   `domain/` minus avian, `script/kernel` + `reflect_bridge`) that compiles
   without Bevy's render stack. That would let pure-math tests (`geometry`,
   `kernel`, registry) build and run in seconds with **no** Bevy link at all.
   The ECS layers gain little from further splitting (they all link Bevy
   anyway, and one link is cheaper than three).
3. **Decision gate**: run `cargo build --timings` cold and warm before and
   after a prototype split on a branch; adopt only if the *incremental test
   cycle* (the 34 s number) drops meaningfully (>30%). Check-loop and CI
   cold-build numbers will not improve enough to justify two `Cargo.toml`s
   of coordination cost.

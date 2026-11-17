# Toolchain bump to rustc 1.100.0
Phase U · `docs/unified-surface-plan.md` § 4, § 10 row 2; `orchestration.md`
§ Recurring ("Bump `RUST_PIN`, fix in the same PR") · status: done
(PR open; see Outcome)

## Goal

CI and agent containers build on **rustc 1.100.0** (stable on 2026-11-17).
`RUST_PIN` says so, the new lints are fixed in the same PR, and the docs that
name a toolchain are current. This unblocks D-1: Bevy 0.20.0 and 0.20.1
declare `rust_version = 1.97.1`, above the current pin.

## Read first

- `CLAUDE.md` § Build & test, and § Agent sessions (the toolchain hazards).
- `.github/workflows/ci.yml` lines 15–21 and 50–55 (`RUST_PIN`, the pin
  rationale, `dtolnay/rust-toolchain`).
- `docs/orchestration.md` § Recurring — toolchain bump (known lint debt).
- `docs/state.md` § Known hazards (Toolchain).

## Facts (planner, 2026-11-17, branch `claude/laughing-euler-sh2g1r`)

- Container: `rustc 1.97.0 (2d8144b78 2026-07-07)`; `RUST_PIN: "1.97.0"`.
  No `rust-toolchain.toml` in the repo.
- `https://static.rust-lang.org/dist/channel-rust-stable.toml` answered
  HTTP 200 from the container; it names `rustc 1.100.0 (0ef1bc386
  2026-11-09)`. *Unverified:* that `rustup toolchain install 1.100.0`
  finishes through the proxy (download size, timeouts). Check it first. If it
  fails, stop and hand back: the pin must equal what containers can run.
- Known debt (found 2026-10): a lint against `assert!(x.is_empty())` /
  `assert!(!x.is_empty())`, about 21 single-line sites plus multi-line ones.
  Releases 1.98–1.100 may add more pedantic lints.
- Toolchain text to update: `CLAUDE.md:194-195` ("rustc ≥ 1.95.0 (Bevy
  0.19's floor)", "currently 1.97.0"), `CLAUDE.md:254-259` (rust-lang.org
  "not reachable"), `docs/state.md` Toolchain hazard,
  `docs/agent-context.md:29`. `docs/bevy19-notes.md` is history; D-1
  replaces it.

## Approach

1. Install 1.100.0 with `rustup` (`clippy`, `rustfmt` components). Record the
   command and its time in the Outcome.
2. Run `cargo clippy --workspace --all-targets -- -D warnings`. Fix every new
   lint by changing code. Do not add `allow` attributes unless a lint is wrong
   for this codebase, and then explain each one in the Outcome.
3. Set `RUST_PIN: "1.100.0"`. Decide whether to add a `rust-toolchain.toml`
   pinned to the same version, so local and CI agree without reading
   `ci.yml`; record the decision.
4. Update the toolchain text listed in Facts.

## Out of scope

Any dependency version change (D-1), any behaviour change, any edit to code
that a lint does not flag.

## Done when

- fmt, clippy (`-D warnings`), `cargo doc` (`RUSTDOCFLAGS="-D warnings"`) and
  `cargo test --workspace` pass on 1.100.0 locally, and CI is green.
- `Cargo.lock` is unchanged (no dependency change in this brief).
- The Outcome lists the lints fixed (name, count) and the install command.

## Outcome

Worker session, 2026-11-17, branch `claude/d-0-toolchain-1-100`.

- **Install:** `rustup toolchain install 1.100.0 --profile minimal -c clippy
  -c rustfmt` finished in 21 s through the container proxy (`rustc 1.100.0
  (0ef1bc386 2026-11-09)`). The unverified fact is now verified.
- **Order:** this ran before 4b-1. GitHub had no branch and no PR for 4b-1
  on 2026-11-17, and the owner asked for the roadmap to start. A 4b-1 branch
  that lands later rebases onto this; `rust-toolchain.toml` gives its worker
  the new compiler without a manual step.
- **Lints fixed** (no `allow` added):

  | Lint | Sites | Fix |
  |---|---|---|
  | `clippy::assert_is_empty` | 18 | `assert_eq!(x, [] as [T; 0])` / `assert_ne!`, as clippy suggests, with short type names. A failure now prints the value. |
  | `clippy::map_unwrap_or` (now also `map(..).unwrap_or_default()`) | 7 | `map_or_default(..)` |
  | `clippy::manual_midpoint` | 3 | `f32::midpoint(a, b)` |

  The known debt said about 21 `assert!` sites; 1.100.0 flags 18.
  `f32::midpoint` gives the same result as `0.5 * (a + b)` except where
  `a + b` overflows `f32`, which these three sites (a curve bound, an AABB
  centre, an NDC coordinate) do not reach. `crates/gradiance-ui/src/lib.rs`
  now reads `f32::midpoint(ndc.x, 1.0)` next to `(1.0 - ndc.y) * 0.5`; the
  lint flags only the first.
- **`rust-toolchain.toml`: added** (`channel = "1.100.0"`, `rustfmt`,
  `clippy`). Reason: the container default is still 1.97.0, so without the
  file each later session gates on the wrong compiler unless it reads
  `ci.yml`. `RUST_PIN` stays (the CI action needs an explicit version);
  `tests/boundaries.rs::the_toolchain_pin_is_one_version` asserts that the two
  are equal. Side effect: the `coupling` job and `release.yml` ask for
  `@stable` but `cargo` there now uses 1.100.0 through the file.
- **Gates on 1.100.0, local:** fmt, clippy (`-D warnings`, all targets) and
  `cargo doc` (`-D warnings`) pass. `Cargo.lock` is unchanged.
  **`cargo test --workspace` did not run locally:** the owner ruled on
  2026-11-17 that long runs go to GitHub CI. CI on the PR is the test gate,
  and it is the first run of the new boundaries test.
- **Container note:** a fresh container needed `apt-get update` before the
  native-library install; without it three packages were not found.

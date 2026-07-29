# Decision: how Gradiance sources SolveSpace

**Status:** accepted, implemented.
**Supersedes:** the vendored `slvs` crate (`third_party/rust_slvs`), deleted.

## The problem

The sketch spike reached SolveSpace through [`slvs`](https://crates.io/crates/slvs)
0.6.0, vendored into `third_party/rust_slvs` because the published crate does not
build on a modern toolchain. That arrangement had four faults, in ascending order
of seriousness:

1. **It was stale.** No upstream commit in three years, pointed at a three-year-old
   *fork* of SolveSpace rather than SolveSpace itself.
2. **It carried a local patch set.** Four patches (bindgen 0.71, `CargoCallbacks::new()`,
   a module visibility change, a libstdc++ link-search fix) that had to be re-applied
   and re-justified on every touch — the definition of a maintenance liability.
3. **It needed `bindgen`, and therefore `libclang`,** on every build machine. The
   original failure was precisely a bindgen/libclang version interaction that
   produced *silently incomplete* bindings, and it put an `install-llvm-action` step
   in the Windows release job.
4. **It was 518 files and 233,701 lines** — 150k of Eigen, 14k of mimalloc — for a
   solver we use through about six functions.

## The decision

Three parts, each chosen against the constraint that there be **no fork, everything
in this repository, and no bulk duplicate code**.

### 1. Vendor a pristine subset of upstream, pinned to a release tag

`third_party/solvespace/` holds **23 files, 12,071 lines**, copied byte-for-byte
from `solvespace/solvespace` at **v3.2** (`27b6a080…`). That is exactly upstream's
own `slvs-solver` + `slvs-interface` CMake targets plus the headers they include.

The tree is regenerated, never edited, by `third_party/solvespace/vendor.sh`. Its
pristineness is a *tested* property, not an intention: `tests/boundaries.rs`
asserts that no vendored file mentions Gradiance and that `SOURCE.md` records the
exact upstream commit. Re-vendoring a newer tag is therefore an overwrite, and
upstream changes can never conflict with a local edit because there are none.

A tag rather than `master` because a pin should be something upstream considered
shippable. v3.2 is also the first release carrying the modern C API
(`Slvs_SolveResult`, `Slvs_SetParamValue`, `Slvs_Solve(Slvs_System *, uint32_t)`),
which the three-year-old fork predates.

### 2. Hand-written bindings in `crates/gradiance-slvs-sys`

`slvs.h` is 516 lines: four POD structs, ~40 integer constants, six functions.
Transcribing it costs less than depending on `bindgen`, and buys the removal of
`libclang` from every build machine and every CI runner — including the Windows
LLVM install step, now deleted.

The risk of hand-written bindings is layout drift going unnoticed on an upstream
bump, since a wrong offset would not fail to link, it would feed the solver
garbage. Both sides are therefore pinned to one table of literal offsets:
`src/layout_check.cpp` `static_assert`s them against the real header at compile
time, and `layout_matches_the_c_header` checks the identical numbers against the
Rust structs. Drift on either side breaks the build.

This crate is the only one allowed to write `unsafe` or link C++ — its own DAG
row, and `unsafe_is_confined_to_the_ffi_crate` enforces it. The workspace's
`unsafe_code = "deny"` stands everywhere else, unmodified.

The API deliberately stays thin and *total*: every entity kind and all 38
constraint types, with no opinion about which an editor should offer. Deciding
that vocabulary is `gradiance-sketch`'s job. Because the solver's handles are
untyped, the four-way `arc`/`circle`/`cubic` match explosions the old typed-handle
API forced disappeared — `solve.rs` went from 1,060 lines to 875 with its 14 tests
unchanged.

### 3. Eigen from the system, mimalloc dropped

**Eigen** (150k lines) is a build prerequisite rather than vendored source.
`system.cpp` needs it for the sparse QR factorisation behind Newton iteration and
rank detection; it is header-only and packaged everywhere. This puts it in the
same category as the `libasound2-dev`/`libudev-dev` this workspace already
requires. `build.rs` finds it via `EIGEN3_INCLUDE_DIR`, then `pkg-config`, then
the conventional locations, and fails with install instructions naming the package
for each platform.

**mimalloc** (14k lines) is dropped outright. Upstream uses it in
`platform/platformbase.cpp` for one thing — a bump arena behind
`AllocTemporary`/`FreeAllTemporary` that the expression allocator fills and then
discards wholesale. `crates/gradiance-slvs-sys/src/platform_shim.cpp` supplies the
four `Platform::` symbols in 30 lines over `calloc`/`free`. That file is an
*addition beside* upstream, not a modification of it, which is what lets the
vendored tree stay pristine.

## What it costs

Eigen becomes a documented build prerequisite. That is the one real trade, and it
is why `README.md`, `CLAUDE.md` and both CI workflows name the package for Linux,
macOS and Windows. The alternative — vendoring 150k lines of a header-only library
that every platform packages — reads worse against "no ton of duplicate code" than
one `apt install` line does against "everything in this repo".

## Result

| | before | after |
|---|---|---|
| Vendored files | 518 | 23 |
| Vendored lines | 233,701 | 12,071 |
| Upstream age | 3-year-old fork | v3.2 release |
| Local patches | 4 | 0 |
| Build needs libclang | yes | no |
| Build needs CMake | no | no |
| `unsafe` in workspace | 0 (in `slvs`) | confined to one tested crate |
| System dependencies | — | Eigen (header-only) |

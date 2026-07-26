# Vendored SolveSpace constraint solver

Upstream: <https://github.com/solvespace/solvespace>
Pinned tag: **v3.2**
Pinned commit: **`27b6a080c8b669421bd4d444650c3b8eddec5687`** (2026-03-26)
License: **GPL-3.0-or-later** (`COPYING.txt`, copied verbatim from upstream)

## This tree is pristine — there is no fork

Every file under `include/` and `src/` is a **byte-identical** copy of upstream at
the pinned commit. Nothing here is patched, and nothing here should ever be
edited. `vendor.sh` regenerates the tree from scratch, so bumping to a newer
upstream tag is an overwrite, not a merge:

```sh
third_party/solvespace/vendor.sh v3.3    # then update the pin above
```

Verify the tree still matches upstream at any time:

```sh
third_party/solvespace/vendor.sh v3.2 && git diff --stat third_party/solvespace
```

An empty diff means no local drift has crept in.

Everything Gradiance needs in order to *use* the solver — the FFI declarations,
the safe wrapper, and the small platform adapter that upstream would otherwise
get from mimalloc — lives in `crates/gradiance-slvs-sys/`. Keeping adaptation
on our side of the line is what makes the "no fork" property hold: upstream
changes can never conflict with a local edit, because there are no local edits.

## What is vendored, and why so little

23 files, ~12k lines. Upstream is ~500k lines and the previously vendored
`slvs` crate carried 518 files / 233k lines, so this is a 95% reduction.

The set is exactly upstream's own `slvs-solver` + `slvs-interface` CMake targets
(`src/CMakeLists.txt`, `src/slvs/CMakeLists.txt`) plus the headers those
translation units include transitively:

| Group | Files |
|---|---|
| Public C API | `include/slvs.h` |
| Solver core | `src/{constrainteq,entity,expr,system,util}.cpp` |
| C API implementation | `src/slvs/lib.cpp` |
| Headers | `src/{defs,dsc,expr,handle,param,polygon,resource,sketch,solvespace,ttf,ui,util}.h`, `src/platform/{gui,platform}.h`, `src/render/render.h`, `src/srf/surface.h` |

The header list is wider than the compiled set because `solvespace.h` includes
the geometry, rendering and UI headers wholesale. They are needed to *parse* the
solver's translation units; none of the corresponding `.cpp` files are compiled,
and no rendering or UI code is linked.

## Dependencies we deliberately do not vendor

- **Eigen** — required by `system.cpp` (`Eigen/Core`, `Eigen/SparseQR`) for the
  sparse QR factorisation that drives Newton iteration and rank detection.
  Vendoring it would add ~150k lines for a header-only library that every
  platform packages. It is a documented build prerequisite instead, in the same
  category as the `libasound2-dev`/`libudev-dev` this workspace already needs.
- **mimalloc** — upstream uses it in `src/platform/platformbase.cpp` for one
  thing: a bump arena behind `Platform::AllocTemporary` /
  `Platform::FreeAllTemporary`, which the expression allocator uses and frees
  wholesale. That is ~14k lines of allocator for a use we can satisfy in 30.
  `platformbase.cpp` is therefore not vendored; `crates/gradiance-slvs-sys/`
  supplies the four `Platform::` symbols itself. This is an *addition* beside
  upstream, not a modification of it.

## Licensing

SolveSpace is GPL-3.0-or-later. Linking it makes a distributed Gradiance binary
GPL-3.0-or-later as well. The root `Cargo.toml` carries no `license` field
precisely so it does not assert a licence the distributed artefact could not
honour.

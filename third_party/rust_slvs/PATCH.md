# Vendored `slvs` — local patch notes

Vendored copy of [`slvs` 0.6.0](https://crates.io/crates/slvs)
([upstream](https://github.com/thekakkun/rust_slvs)), a Rust binding for
[SolveSpace](https://github.com/solvespace/solvespace)'s geometric constraint solver.

The published crate bundles the SolveSpace C++ sources (`solvespace/src`,
`extlib/eigen`, `extlib/mimalloc`) and compiles them with `cc`, so there is **no**
CMake, git-submodule, or system-SolveSpace requirement at build time.

## Why this is vendored rather than a plain crates.io dependency

`slvs` 0.6.0 **does not build against a modern libclang.** It pins `bindgen 0.64`,
which mis-parses `include/slvs.h` under libclang 18: the generated `bindings.rs`
contains only the 53 `#define` constants and none of the typedefs or functions, so
the crate fails to compile with 62 errors of the form

```
error[E0432]: unresolved imports `crate::bindings::Slvs_hEntity`, `crate::bindings::Slvs_hGroup`
  no `Slvs_hEntity` in `bindings`
```

The header itself is fine — `clang -x c++ -std=c++11 -fsyntax-only include/slvs.h`
parses it without error, and `bindgen 0.71` generates complete bindings (238 lines,
with `Slvs_hEntity` and `Slvs_Solve` present). The failure is purely the old
bindgen/new libclang combination.

## The patch

Two lines in the build setup, plus one visibility change in `src/`.

1. `Cargo.toml` — `[build-dependencies.bindgen]`: `version = "0.64.0"` → `"0.71"`
2. `build.rs` — `Box::new(CargoCallbacks)` → `Box::new(CargoCallbacks::new())`
   (bindgen ≥ 0.69 wants the struct's constructor; the `use bindgen::CargoCallbacks`
   import stays, and emits a deprecation warning for the same-named constant it also
   brings in. The warning is confined to this excluded crate's build script and does
   not reach the workspace lint gate.)
3. `src/lib.rs` — `mod element;` → `pub mod element;`

Change 3 is not about the build. `ConstraintHandle` exposes its `handle` as a
public field, but `SolveResult::Fail::failed_constraints` hands back
`Vec<Box<dyn AsConstraintHandle>>`, and reading a handle off that trait object
requires the `AsHandle` supertrait — which lived in a private module, so no
downstream crate could attribute a failure to a specific constraint. Publishing
the module is additive (it removes no API and changes no behaviour) and is what
lets `gradiance-sketch` report *which* constraint the solver could not satisfy
instead of only that the system was inconsistent.

Regenerate the diff against a pristine copy with:

```sh
cargo package --list --allow-dirty   # from a scratch `cargo add slvs` checkout
diff -ru "$CARGO_HOME/registry/src/index.crates.io-*/slvs-0.6.0" third_party/rust_slvs
```

## Build requirements this introduces

- a C++ compiler (via `cc`)
- **libclang** (via `bindgen`); on Windows `LIBCLANG_PATH` must point at the clang
  library directory

CI already installs `clang` on Linux. `release.yml` does not, on any platform — see
the workflow changes that accompany this vendoring.

Measured cost: ~35 s clean build for the C++ solver plus bindings, fully cached
afterwards.

## Licensing

`slvs` and SolveSpace are **GPL-3.0**. Linking them makes a distributed Gradiance
binary GPL-3.0; the workspace `license` field is set accordingly.

## Removing this patch

Upstream bumping `bindgen` is the only thing needed to drop the fork. When that
lands, delete `third_party/rust_slvs`, remove the `exclude` entry in the root
`Cargo.toml`, and point `gradiance-sketch` at the crates.io release.

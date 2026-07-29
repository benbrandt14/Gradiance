//! Compile the vendored SolveSpace solver and link it into this crate.
//!
//! No `CMake`, no `bindgen`, no `libclang`: `cc` drives a plain C++ compiler
//! over eight translation units. The only external requirement is Eigen, which
//! is header-only — see `locate_eigen`.

// The workspace bans panicking paths in product code, which is the right rule
// for anything that runs in the app. A build script is the opposite case:
// panicking *is* its error-reporting channel — cargo catches it and prints the
// message as the build failure. Returning `Result` here would only bury the
// "install libeigen3-dev" advice that makes a failed build actionable.
#![allow(clippy::expect_used, clippy::panic)]

use std::env;
use std::path::{Path, PathBuf};

/// Upstream's `slvs-solver` and `slvs-interface` `CMake` targets, as source paths
/// relative to `third_party/solvespace/`. `platform/platformbase.cpp` is
/// deliberately absent: `src/platform_shim.cpp` stands in for it so the
/// vendored tree needs no mimalloc. See `third_party/solvespace/SOURCE.md`.
const SOLVER_SOURCES: &[&str] = &[
    "src/constrainteq.cpp",
    "src/entity.cpp",
    "src/expr.cpp",
    "src/system.cpp",
    "src/util.cpp",
    "src/slvs/lib.cpp",
];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let vendor = manifest
        .parent()
        .and_then(Path::parent)
        .expect("crates/<pkg>/ has a workspace root above it")
        .join("third_party/solvespace");

    assert!(
        vendor.join("include/slvs.h").is_file(),
        "vendored SolveSpace is missing at {}. Run third_party/solvespace/vendor.sh",
        vendor.display()
    );

    let target = env::var("TARGET").unwrap_or_default();
    let eigen = locate_eigen();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        // Selects the solver-library build of the vendored sources: no GUI, no
        // application singleton, just the constraint system behind slvs.h.
        .define("LIBRARY", None)
        .include(vendor.join("include"))
        .include(vendor.join("src"))
        .include(&eigen)
        .files(SOLVER_SOURCES.iter().map(|f| vendor.join(f)))
        .file(manifest.join("src/platform_shim.cpp"))
        .file(manifest.join("src/layout_check.cpp"))
        // Upstream's own warning suppressions for this target. These are third
        // party sources; the workspace lint gate governs Rust, not them.
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-missing-field-initializers");

    if target.contains("windows") && target.contains("msvc") {
        for def in [
            "_CRT_SECURE_NO_DEPRECATE",
            "_CRT_SECURE_NO_WARNINGS",
            "_SCL_SECURE_NO_WARNINGS",
            "NOMINMAX",
            "_USE_MATH_DEFINES",
            "WIN32_LEAN_AND_MEAN",
        ] {
            build.define(def, None);
        }
    }

    build.compile("gradiance_slvs");

    link_cxx_stdlib(&target);

    println!("cargo:rerun-if-changed=src/platform_shim.cpp");
    println!("cargo:rerun-if-changed=src/layout_check.cpp");
    println!("cargo:rerun-if-env-changed=EIGEN3_INCLUDE_DIR");
    println!("cargo:rerun-if-changed={}", vendor.display());
}

/// Find Eigen's include root — the directory containing the `Eigen/` folder.
///
/// Eigen is header-only and packaged everywhere, so it is a build prerequisite
/// rather than something vendored: bundling it would add ~150k lines to the
/// repository for no build-reproducibility gain. The search order goes from
/// most explicit to most conventional so a caller can always override it.
fn locate_eigen() -> PathBuf {
    if let Some(dir) = env::var_os("EIGEN3_INCLUDE_DIR") {
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("Eigen/Core").is_file(),
            "EIGEN3_INCLUDE_DIR={} does not contain Eigen/Core",
            dir.display()
        );
        return dir;
    }

    if let Ok(out) = std::process::Command::new("pkg-config")
        .args(["--cflags-only-I", "eigen3"])
        .output()
        && out.status.success()
    {
        let flags = String::from_utf8_lossy(&out.stdout);
        for dir in flags
            .split_whitespace()
            .filter_map(|f| f.strip_prefix("-I"))
        {
            let dir = PathBuf::from(dir);
            if dir.join("Eigen/Core").is_file() {
                return dir;
            }
        }
    }

    // vcpkg and Homebrew both land in a predictable place, and distributions
    // agree on /usr/include/eigen3.
    let candidates = [
        "/usr/include/eigen3",
        "/usr/local/include/eigen3",
        "/opt/homebrew/include/eigen3",
        "/opt/homebrew/opt/eigen/include/eigen3",
    ];
    for dir in candidates {
        let dir = PathBuf::from(dir);
        if dir.join("Eigen/Core").is_file() {
            return dir;
        }
    }

    // vcpkg installs headers directly under <root>/include, without the eigen3
    // level, which is why this is checked separately from the list above.
    if let Some(root) = env::var_os("VCPKG_ROOT") {
        let triplet = if env::var("TARGET").unwrap_or_default().contains("x86_64") {
            "x64-windows"
        } else {
            "arm64-windows"
        };
        let dir = PathBuf::from(root)
            .join("installed")
            .join(triplet)
            .join("include");
        if dir.join("Eigen/Core").is_file() {
            return dir;
        }
    }

    panic!(
        "Eigen headers not found. SolveSpace's solver needs them for its sparse \
         QR factorisation.\n  \
         Debian/Ubuntu: apt install libeigen3-dev\n  \
         Fedora:        dnf install eigen3-devel\n  \
         macOS:         brew install eigen\n  \
         Windows:       vcpkg install eigen3\n  \
         Or set EIGEN3_INCLUDE_DIR to the directory containing Eigen/Core."
    );
}

/// Tell the linker where the C++ standard library actually lives.
///
/// `cc` emits `cargo:rustc-link-lib=stdc++`, but on most Linux distributions the
/// *dev* symlink `libstdc++.so` sits in GCC's private directory
/// (`/usr/lib/gcc/<triple>/<version>/`) rather than in the default library path,
/// which holds only the runtime `libstdc++.so.6`. The GCC driver knows about its
/// own directory implicitly; `lld` does not, and this workspace pins
/// `linker = "clang"` with `-fuse-ld=lld` in `.cargo/config.toml`. When clang
/// fails to detect the GCC installation the link dies with
/// `ld.lld: error: unable to find library -lstdc++`.
///
/// Asking the compiler itself is the portable answer — it reports the path for
/// whatever toolchain is actually in use, on any distribution.
fn link_cxx_stdlib(target: &str) {
    if target.contains("windows") || target.contains("apple") {
        return;
    }
    let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_string());
    let Ok(out) = std::process::Command::new(&cxx)
        .arg("-print-file-name=libstdc++.so")
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    // A compiler that cannot locate the library echoes the bare name back; only
    // a resolved absolute path is worth emitting. Deliberately *not*
    // canonicalized: the dev symlink is the file the linker needs to find, and
    // following it lands in the runtime directory, which does not contain one.
    let reported = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    if reported.is_absolute()
        && let Some(dir) = reported.parent()
    {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
}

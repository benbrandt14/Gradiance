use std::env;
use std::path::PathBuf;

extern crate bindgen;
use bindgen::CargoCallbacks;
use dunce::canonicalize;

fn main() {
    let libdir_path = canonicalize(PathBuf::from("solvespace")).expect("Cannot canonicalize path.");
    let target = env::var("TARGET").unwrap();

    // Build solvespace library
    let mut slvs_cfg = cc::Build::new();

    // Things necessary for Windows but not Linux, dunno about building on Mac OS.
    if target.contains("windows") {
        println!(
            "cargo:rustc-link-search={}",
            PathBuf::from(r"C:\Windows\System32").to_str().unwrap()
        );
        println!("cargo:rustc-link-lib=shell32");

        slvs_cfg.define("_CRT_SECURE_NO_DEPRECATE", None);
        slvs_cfg.define("_CRT_SECURE_NO_WARNINGS", None);
        slvs_cfg.define("_SCL_SECURE_NO_WARNINGS", None);
        slvs_cfg.define("WINVER", "0x0501");
        slvs_cfg.define("_WIN32_WINNT", "0x0501");
        slvs_cfg.define("_WIN32_IE", "_WIN32_WINNT");
        slvs_cfg.define("ISOLATION_AWARE_ENABLED", None);
        slvs_cfg.define("WIN32", None);
        slvs_cfg.define("WIN32_LEAN_AND_MEAN", None);
        slvs_cfg.define("UNICODE", None);
        slvs_cfg.define("_UNICODE", None);
        slvs_cfg.define("NOMINMAX", None);
        slvs_cfg.define("_USE_MATH_DEFINES", None);
    }

    slvs_cfg
        .cpp(true)
        .define("LIBRARY", None)
        .includes(
            [
                "src",
                "include",
                "extlib/eigen",
                "src/SYSTEM",
                "extlib/mimalloc/include",
            ]
            .map(|file| libdir_path.join(PathBuf::from(file))),
        )
        .files(
            [
                "src/util.cpp",
                "src/entity.cpp",
                "src/expr.cpp",
                "src/constraint.cpp",
                "src/constrainteq.cpp",
                "src/system.cpp",
                "src/platform/platform.cpp",
                "src/lib.cpp",
            ]
            .map(|file| libdir_path.join(PathBuf::from(file))),
        )
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-missing-field-initializers")
        .compile("slvs");

    // Build mimalloc
    let mut mimalloc_cfg = cc::Build::new();

    mimalloc_cfg
        .include(libdir_path.join(PathBuf::from("extlib/mimalloc/include")))
        .files(
            [
                "extlib/mimalloc/src/stats.c",
                "extlib/mimalloc/src/random.c",
                "extlib/mimalloc/src/os.c",
                "extlib/mimalloc/src/bitmap.c",
                "extlib/mimalloc/src/arena.c",
                "extlib/mimalloc/src/segment-cache.c",
                "extlib/mimalloc/src/segment.c",
                "extlib/mimalloc/src/page.c",
                "extlib/mimalloc/src/alloc.c",
                "extlib/mimalloc/src/alloc-aligned.c",
                "extlib/mimalloc/src/alloc-posix.c",
                "extlib/mimalloc/src/heap.c",
                "extlib/mimalloc/src/options.c",
                "extlib/mimalloc/src/init.c",
            ]
            .map(|file| libdir_path.join(PathBuf::from(file))),
        )
        .compile("mimalloc");

    // Tell the linker where the C++ standard library actually lives.
    //
    // `cc` emits `cargo:rustc-link-lib=stdc++`, but on most Linux distributions
    // the *dev* symlink `libstdc++.so` sits in GCC's private directory
    // (/usr/lib/gcc/<triple>/<version>/) rather than in the default library
    // path, which holds only the runtime `libstdc++.so.6`. The GCC driver knows
    // about its own directory implicitly; `lld` does not, and this workspace
    // pins `linker = "clang"` with `-fuse-ld=lld` in .cargo/config.toml. When
    // clang fails to detect the GCC installation the link dies with
    // `ld.lld: error: unable to find library -lstdc++`.
    //
    // Asking the compiler itself is the portable answer — it reports the path
    // for whatever toolchain is actually in use, on any distribution.
    if !target.contains("windows") && !target.contains("apple") {
        let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_string());
        if let Ok(out) = std::process::Command::new(&cxx)
            .arg("-print-file-name=libstdc++.so")
            .output()
        {
            if out.status.success() {
                let reported = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let path = PathBuf::from(&reported);
                // A compiler that cannot locate the library echoes the bare
                // name back; only a resolved absolute path is worth emitting.
                if path.is_absolute() {
                    if let Some(dir) = path.parent() {
                        println!("cargo:rustc-link-search=native={}", dir.display());
                    }
                }
            }
        }
    }

    // Generate bindings to library header
    let bindings = bindgen::Builder::default()
        .opaque_type("std::.*")
        .allowlist_var("SLVS_.*")
        .allowlist_type("Slvs_.*")
        .allowlist_function("Slvs_.*")
        .header(
            libdir_path
                .join(PathBuf::from("include/slvs.h"))
                .to_str()
                .unwrap(),
        )
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++11")
        .clang_arg("-fvisibility=default")
        .parse_callbacks(Box::new(CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("Couldn't write bindings.");
}

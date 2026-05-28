//! Build script for `indirect-predicates-sidecar-rs`.
//!
//! 1. Locates the upstream `Indirect_Predicates` source directory via
//!    the `INDIRECT_PREDICATES_SRC` env var (preferred) or a default
//!    vendored path (`/home/claude/cherchi2022/...`).
//! 2. If found: compiles `src/wrapper.cpp` against the library
//!    headers using `cc::Build` (C++20, no GMP, no SIMD in v1).
//! 3. If not found: compiles `src/stub.cpp` as a no-op fallback and
//!    sets `cargo:rustc-cfg=ip_unavailable` + emits a build warning.
//! 4. Runs `bindgen` against `src/wrapper.h` (pure C; `-x c`) to
//!    produce Rust FFI bindings in `$OUT_DIR/bindings.rs`.

use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_SRC: &str =
    "/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/external/Indirect_Predicates";

fn main() {
    // Re-run if the env var changes or our own shim files change.
    println!("cargo:rerun-if-env-changed=INDIRECT_PREDICATES_SRC");
    println!("cargo:rerun-if-changed=src/wrapper.h");
    println!("cargo:rerun-if-changed=src/wrapper.cpp");
    println!("cargo:rerun-if-changed=src/stub.cpp");
    println!("cargo:rustc-check-cfg=cfg(ip_unavailable)");

    let src_dir = resolve_source_dir();
    match src_dir {
        Some(dir) => compile_real_shim(&dir),
        None => compile_stub(),
    }

    generate_bindings();
}

/// Returns the `Indirect_Predicates` source root if it can be found,
/// otherwise `None` (caller falls back to the stub).
///
/// If `INDIRECT_PREDICATES_SRC` is set, it is honored exclusively
/// (success or failure — no fallback to the default). If it is
/// unset, the default vendored path is probed.
fn resolve_source_dir() -> Option<PathBuf> {
    if let Ok(env_path) = env::var("INDIRECT_PREDICATES_SRC") {
        let p = PathBuf::from(&env_path);
        if header_present(&p) {
            return Some(p);
        }
        // Explicit user override — do NOT silently fall back to the
        // default path; the user wanted to test the unavailable
        // path or use a custom location they need to fix.
        println!(
            "cargo:warning=INDIRECT_PREDICATES_SRC is set ({env_path}) but \
             indirect_predicates.h was not found at that path; \
             building stub (no fallback to default)."
        );
        return None;
    }
    let default = PathBuf::from(DEFAULT_SRC);
    if header_present(&default) {
        return Some(default);
    }
    None
}

fn header_present(src_dir: &Path) -> bool {
    src_dir.join("include").join("indirect_predicates.h").is_file()
}

fn compile_real_shim(src_dir: &Path) {
    let include = src_dir.join("include");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .include(&include)
        .file("src/wrapper.cpp")
        // Upstream uses (T&)(*this) casts and friends; silence noisy
        // warnings without changing semantics.
        .flag_if_supported("-Wno-strict-aliasing")
        .flag_if_supported("-Wno-cast-qual")
        .flag_if_supported("-Wno-unused-parameter")
        // Required for the predicate cascade's `_interval` path; the
        // PR-CR-IP1 link-probe input short-circuits earlier, but
        // setting the flag now keeps PR-CR-IP2 unblocked.
        .flag_if_supported("-frounding-math");
    // DO NOT define USE_GNU_GMP_CLASSES — library uses its own
    // bigfloat in the `#else` branch (numerics.h:514+).
    // DO NOT define USE_SIMD_INSTRUCTIONS — pure-double path is
    // enough for v1; PR-CR-IP8 banked for AVX2/SSE2 opt-in.
    build.compile("indirect_predicates_shim");
    println!(
        "cargo:warning=indirect-predicates-sidecar-rs: linked real shim against {}",
        src_dir.display()
    );
}

fn compile_stub() {
    let mut build = cc::Build::new();
    build.cpp(true).file("src/stub.cpp");
    build.compile("indirect_predicates_shim");
    println!("cargo:rustc-cfg=ip_unavailable");
    println!(
        "cargo:warning=indirect-predicates-sidecar-rs: Indirect_Predicates source \
         not found (set INDIRECT_PREDICATES_SRC or place at {}); built no-op stub.",
        DEFAULT_SRC
    );
}

fn generate_bindings() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let bindings = bindgen::Builder::default()
        .header("src/wrapper.h")
        // Treat wrapper.h as PURE C (not C++) so bindgen doesn't
        // attempt to parse <indirect_predicates.h> through it.
        .clang_arg("-x")
        .clang_arg("c")
        .allowlist_function("ip_.*")
        .generate()
        .expect("bindgen failed to generate FFI bindings for wrapper.h");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindgen output to $OUT_DIR/bindings.rs");
}

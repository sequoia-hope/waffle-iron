//! FFI sidecar for Marco Attene's `Indirect_Predicates` C++ library
//! (LGPL-2.1, header-only).
//!
//! **NOT WASM-compatible.** This crate compiles a thin C++ shim and
//! links it into the binary; WASM targets fire `compile_error!` to
//! make the incompatibility loud. The user has accepted a broken
//! WASM build during the Yang validation phase. PR-CR-IP10 banked
//! to restore WASM via cfg-gating in consumers.
//!
//! ## Status (PR-CR-IP1 — scaffold)
//!
//! Establishes the FFI build chain (`cc` + `bindgen` against a
//! pure-C [`wrapper.h`]) and exposes a single link-probe
//! ([`link_probe`]) that calls `dotProductSign2D` on a well-conditioned
//! input. Proves end-to-end that:
//!
//! - the upstream LGPL header-only library compiles via our shim,
//! - bindgen produces usable FFI for the pure-C interface,
//! - the linker resolves real library symbols (not just our shim).
//!
//! Real predicate wrappers (LPI / TPI lambdas, `orient3d_indirect_IIII`,
//! `lessThanOnX/Y/Z_*`, `genericPoint` opaque-handle) arrive in
//! PR-CR-IP2 through PR-CR-IP7.
//!
//! ## License
//!
//! This crate's source is **MIT**. The C++ library it dynamically
//! links is **LGPL-2.1-or-later** (Marco Attene, IMATI-GE / CNR).
//! See `LICENSE-THIRD-PARTY.md` for boundary semantics.
//!
//! ## Availability
//!
//! The crate is designed to build successfully even when the
//! upstream source is unavailable — in that case it falls back to a
//! no-op stub. Check [`AVAILABLE`] at runtime to detect.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "indirect-predicates-sidecar-rs is NOT WASM-compatible (FFI-links \
     LGPL Indirect_Predicates C++ library). User has accepted broken \
     WASM during Yang validation phase. See PR-CR-IP10 (banked) for \
     the eventual cfg-gating restoration in cherchi-rs / yang-rs / \
     kernel-v2 consumers."
);

mod ffi {
    // Bindgen output. Populated by build.rs.
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(dead_code)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// `true` when the upstream `Indirect_Predicates` source was found
/// at build time and the real C++ shim was compiled. `false` when
/// the build fell back to the no-op stub.
#[cfg(not(ip_unavailable))]
pub const AVAILABLE: bool = true;

/// `true` when the upstream `Indirect_Predicates` source was found
/// at build time and the real C++ shim was compiled. `false` when
/// the build fell back to the no-op stub.
#[cfg(ip_unavailable)]
pub const AVAILABLE: bool = false;

/// Probes the FFI link.
///
/// - Returns **`+1`** when the upstream library is available
///   (`AVAILABLE == true`): the sign of `dotProductSign2D` on a
///   well-conditioned input.
/// - Returns **`-2`** when the build fell back to the stub
///   (`AVAILABLE == false`).
///
/// Used by PR-CR-IP1's smoke tests to prove the entire build chain
/// (cc compile + bindgen FFI + linker resolves real symbols)
/// completed end-to-end.
pub fn link_probe() -> i32 {
    // Safety: `ip_link_probe` is declared `extern "C"` with no
    // arguments and returns `int`. It has no preconditions, no
    // side effects in PR-CR-IP1's input, and never panics.
    unsafe { ffi::ip_link_probe() }
}

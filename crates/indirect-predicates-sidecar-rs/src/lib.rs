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

// =========================================================================
// PR-CR-IP2 — IntervalNumber + lambda3d_LPI_interval + init_fpu
// =========================================================================

/// Real-number interval `[inf, sup]`.
///
/// Crosses the Rust/C++ FFI boundary as a flat `(inf, sup)` pair of
/// doubles (NEVER by value). The C++ `interval_number` class uses a
/// sign-inverted lower-bound representation internally for SIMD
/// optimization; the FFI shim converts at the boundary so Rust
/// always sees the natural `[inf, sup]` form.
///
/// No validation: `IntervalNumber::new(2.0, 1.0)` is allowed (the
/// upstream library decides whether such inputs are meaningful).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct IntervalNumber {
    pub inf: f64,
    pub sup: f64,
}

impl IntervalNumber {
    /// Construct an interval `[inf, sup]`. No validation.
    pub const fn new(inf: f64, sup: f64) -> Self {
        Self { inf, sup }
    }

    /// Construct a degenerate point interval `[x, x]`.
    pub const fn point(x: f64) -> Self {
        Self { inf: x, sup: x }
    }
}

/// Result of [`lambda3d_lpi_interval`]: four interval lambda
/// coordinates (numerators + denominator) plus a `reliable` flag.
///
/// When `reliable == false`, the denominator interval `lambda_d`
/// straddles zero and the caller should fall back to an exact
/// computation (`lambda3d_LPI_exact`, PR-CR-IP3).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LpiIntervalResult {
    pub lambda_x: IntervalNumber,
    pub lambda_y: IntervalNumber,
    pub lambda_z: IntervalNumber,
    pub lambda_d: IntervalNumber,
    pub reliable: bool,
}

/// One-time-per-thread FPU initialization for interval-arithmetic
/// predicates. Idempotent.
///
/// On 64-bit Linux without `USE_SIMD_INSTRUCTIONS` (our default
/// target), this is a no-op — the upstream `initFPU()` is empty in
/// that configuration. Call it anyway: harmless if no-op, required
/// on platforms where it isn't (PR-CR-IP8 SIMD opt-in will activate
/// it).
///
/// Multi-threaded policy is deferred until cherchi-rs goes parallel.
/// For now, calling it once at process start suffices.
pub fn init_fpu() {
    // Safety: `ip_init_fpu` is declared `extern "C"` with no
    // arguments, no return value, no preconditions, no side effects
    // visible in our threading model.
    unsafe { ffi::ip_init_fpu() }
}

/// Line-plane intersection in interval arithmetic. Wraps upstream
/// `lambda3d_LPI_interval`.
///
/// - `p, q`: two points defining the line.
/// - `r, s, t`: three points defining the plane.
///
/// Returns the four lambda interval coordinates plus a `reliable`
/// flag. When `reliable == false`, fall back to the exact computation
/// (`lambda3d_LPI_exact`, PR-CR-IP3) — this happens when the line is
/// parallel to or contained in the plane (denominator straddles
/// zero).
///
/// In stub mode (`cfg!(ip_unavailable)`), returns all-zero lambdas
/// with `reliable: false`.
pub fn lambda3d_lpi_interval(
    p: [IntervalNumber; 3],
    q: [IntervalNumber; 3],
    r: [IntervalNumber; 3],
    s: [IntervalNumber; 3],
    t: [IntervalNumber; 3],
) -> LpiIntervalResult {
    let flatten = |pt: [IntervalNumber; 3]| -> [f64; 6] {
        [
            pt[0].inf, pt[0].sup, pt[1].inf, pt[1].sup, pt[2].inf, pt[2].sup,
        ]
    };
    let p_arr = flatten(p);
    let q_arr = flatten(q);
    let r_arr = flatten(r);
    let s_arr = flatten(s);
    let t_arr = flatten(t);
    let mut lambda_out = [0.0_f64; 8];
    let mut reliable: bool = false;
    // Safety: all five input arrays have length 6; `lambda_out` has
    // length 8; the FFI signature matches the C declaration in
    // `wrapper.h`. The C++ shim performs no allocation, returns no
    // ownership, and writes only to the output buffers we provided.
    unsafe {
        ffi::ip_lambda3d_lpi_interval(
            p_arr.as_ptr(),
            q_arr.as_ptr(),
            r_arr.as_ptr(),
            s_arr.as_ptr(),
            t_arr.as_ptr(),
            lambda_out.as_mut_ptr(),
            &mut reliable,
        )
    }
    LpiIntervalResult {
        lambda_x: IntervalNumber::new(lambda_out[0], lambda_out[1]),
        lambda_y: IntervalNumber::new(lambda_out[2], lambda_out[3]),
        lambda_z: IntervalNumber::new(lambda_out[4], lambda_out[5]),
        lambda_d: IntervalNumber::new(lambda_out[6], lambda_out[7]),
        reliable,
    }
}

// =========================================================================
// PR-CR-IP3 — lambda3d_LPI_exact (Shewchuk expansion arithmetic)
// =========================================================================

/// Result of [`lambda3d_lpi_exact`]: four Shewchuk expansions (the
/// numerators `lambda_x`, `lambda_y`, `lambda_z` and the denominator
/// `lambda_d`).
///
/// Each `Vec<f64>` is a Shewchuk "expansion of doubles" — the
/// geometric value of a lambda is the sum of its expansion entries.
/// The expansion length is data-dependent.
///
/// In stub mode (`cfg!(ip_unavailable)`), all four Vecs are empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LpiExactResult {
    pub lambda_x: Vec<f64>,
    pub lambda_y: Vec<f64>,
    pub lambda_z: Vec<f64>,
    pub lambda_d: Vec<f64>,
}

/// Exact line-plane intersection via Shewchuk expansion arithmetic.
/// Wraps upstream `lambda3d_LPI_exact`.
///
/// - `p, q`: two points defining the line.
/// - `r, s, t`: three points defining the plane.
///
/// Each output lambda is a variable-length expansion whose geometric
/// value is the sum of its entries. `lambda_d` is exactly zero iff
/// the line is parallel to or contained in the plane.
///
/// In stub mode (`cfg!(ip_unavailable)`), returns 4 empty Vecs.
///
/// **Memory safety:** the C++ function allocates each expansion
/// from a thread-local memory pool (`expansionObject::mempool`).
/// This Rust function copies each pool buffer to an owned
/// `Vec<f64>` and releases the pool memory before returning. No
/// raw pointers cross the public API boundary.
pub fn lambda3d_lpi_exact(
    p: [f64; 3],
    q: [f64; 3],
    r: [f64; 3],
    s: [f64; 3],
    t: [f64; 3],
) -> LpiExactResult {
    let mut lx_ptr: *mut f64 = core::ptr::null_mut();
    let mut lx_len: core::ffi::c_int = 0;
    let mut ly_ptr: *mut f64 = core::ptr::null_mut();
    let mut ly_len: core::ffi::c_int = 0;
    let mut lz_ptr: *mut f64 = core::ptr::null_mut();
    let mut lz_len: core::ffi::c_int = 0;
    let mut ld_ptr: *mut f64 = core::ptr::null_mut();
    let mut ld_len: core::ffi::c_int = 0;
    // Safety: input arrays are length 3; out-param pointers and
    // length pointers are all valid for the duration of the call.
    // The C++ shim writes pool-allocated pointers + actual lengths
    // into them. FFI signature matches src/wrapper.h.
    unsafe {
        ffi::ip_lambda3d_lpi_exact(
            p.as_ptr(),
            q.as_ptr(),
            r.as_ptr(),
            s.as_ptr(),
            t.as_ptr(),
            &mut lx_ptr,
            &mut lx_len,
            &mut ly_ptr,
            &mut ly_len,
            &mut lz_ptr,
            &mut lz_len,
            &mut ld_ptr,
            &mut ld_len,
        )
    };
    LpiExactResult {
        lambda_x: copy_and_free(lx_ptr, lx_len),
        lambda_y: copy_and_free(ly_ptr, ly_len),
        lambda_z: copy_and_free(lz_ptr, lz_len),
        lambda_d: copy_and_free(ld_ptr, ld_len),
    }
}

/// Copy a pool-allocated expansion to an owned `Vec<f64>` and
/// release the pool memory. Null pointer or non-positive length
/// produces an empty Vec without invoking the free shim.
///
/// **Alignment note:** upstream's `MultiPool` (memPool.h) stores
/// data in `uint32_t` chunks, so a pool-allocated `double*` may be
/// only 4-byte aligned. C++ tolerates misaligned f64 reads on
/// x86_64, but Rust's `slice::from_raw_parts<f64>` requires 8-byte
/// alignment. We use `copy_nonoverlapping` at the byte level
/// (alignment-agnostic) and write into a Rust-allocated Vec<f64>
/// which IS properly aligned at the destination.
fn copy_and_free(ptr: *mut f64, len: core::ffi::c_int) -> Vec<f64> {
    if ptr.is_null() || len <= 0 {
        return Vec::new();
    }
    let n = len as usize;
    let mut v: Vec<f64> = Vec::with_capacity(n);
    // Safety: the C++ shim guarantees `ptr` points to `n` doubles
    // worth of valid bytes. Bytes-level memcpy doesn't require
    // source alignment. The destination (`v.as_mut_ptr()`) is
    // 8-byte aligned by Rust's allocator. Caller bound: this
    // function never yields between alloc and free (no `await`,
    // no panicking-on-unwinding code) — pool memory stays on the
    // same thread.
    unsafe {
        core::ptr::copy_nonoverlapping(
            ptr.cast::<u8>(),
            v.as_mut_ptr().cast::<u8>(),
            n * core::mem::size_of::<f64>(),
        );
        v.set_len(n);
        ffi::ip_free_doubles(ptr);
    }
    v
}

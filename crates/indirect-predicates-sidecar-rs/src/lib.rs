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

// =========================================================================
// PR-CR-IP4 — lambda3d_TPI_interval + lambda3d_TPI_exact
// =========================================================================

/// Result of [`lambda3d_tpi_interval`]: same shape as
/// [`LpiIntervalResult`] but for the triangle-plane intersection.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TpiIntervalResult {
    pub lambda_x: IntervalNumber,
    pub lambda_y: IntervalNumber,
    pub lambda_z: IntervalNumber,
    pub lambda_d: IntervalNumber,
    /// `true` iff the denominator interval does not straddle zero.
    /// When `false`, fall back to `lambda3d_tpi_exact`.
    pub reliable: bool,
}

/// Result of [`lambda3d_tpi_exact`]: same shape as
/// [`LpiExactResult`] but for the triangle-plane intersection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TpiExactResult {
    pub lambda_x: Vec<f64>,
    pub lambda_y: Vec<f64>,
    pub lambda_z: Vec<f64>,
    pub lambda_d: Vec<f64>,
}

/// Triangle-plane intersection in interval arithmetic. Wraps
/// upstream `lambda3d_TPI_interval`.
///
/// Each input triangle is a `[[IntervalNumber; 3]; 3]`: outer index
/// is the vertex (0..3), inner index is the coordinate (x, y, z).
///
/// Returns interval lambdas + `reliable` flag. When `reliable` is
/// false, the denominator interval contains zero — fall back to
/// [`lambda3d_tpi_exact`].
///
/// In stub mode, returns all-zero lambdas with `reliable: false`.
pub fn lambda3d_tpi_interval(
    v: [[IntervalNumber; 3]; 3],
    w: [[IntervalNumber; 3]; 3],
    u: [[IntervalNumber; 3]; 3],
) -> TpiIntervalResult {
    let flatten = |tri: [[IntervalNumber; 3]; 3]| -> [f64; 18] {
        [
            tri[0][0].inf,
            tri[0][0].sup,
            tri[0][1].inf,
            tri[0][1].sup,
            tri[0][2].inf,
            tri[0][2].sup,
            tri[1][0].inf,
            tri[1][0].sup,
            tri[1][1].inf,
            tri[1][1].sup,
            tri[1][2].inf,
            tri[1][2].sup,
            tri[2][0].inf,
            tri[2][0].sup,
            tri[2][1].inf,
            tri[2][1].sup,
            tri[2][2].inf,
            tri[2][2].sup,
        ]
    };
    let v_arr = flatten(v);
    let w_arr = flatten(w);
    let u_arr = flatten(u);
    let mut lambda_out = [0.0_f64; 8];
    let mut reliable: bool = false;
    // Safety: each input array is length 18 (matches `wrapper.h` contract:
    // 3 verts × 3 coords × 2 bounds). `lambda_out` is length 8. FFI
    // signature matches the C declaration.
    unsafe {
        ffi::ip_lambda3d_tpi_interval(
            v_arr.as_ptr(),
            w_arr.as_ptr(),
            u_arr.as_ptr(),
            lambda_out.as_mut_ptr(),
            &mut reliable,
        )
    };
    TpiIntervalResult {
        lambda_x: IntervalNumber::new(lambda_out[0], lambda_out[1]),
        lambda_y: IntervalNumber::new(lambda_out[2], lambda_out[3]),
        lambda_z: IntervalNumber::new(lambda_out[4], lambda_out[5]),
        lambda_d: IntervalNumber::new(lambda_out[6], lambda_out[7]),
        reliable,
    }
}

/// Triangle-plane intersection in Shewchuk expansion arithmetic.
/// Wraps upstream `lambda3d_TPI_exact`.
///
/// Each input triangle is a `[[f64; 3]; 3]`: outer index is the
/// vertex (0..3), inner index is the coordinate (x, y, z). Each
/// output lambda is a variable-length expansion whose geometric
/// value is the sum of its entries.
///
/// In stub mode, returns 4 empty Vecs.
///
/// **Memory safety:** same model as [`lambda3d_lpi_exact`] — the
/// C++ function allocates from a thread-local pool; this function
/// copies to owned `Vec<f64>` and releases pool memory before
/// returning.
pub fn lambda3d_tpi_exact(v: [[f64; 3]; 3], w: [[f64; 3]; 3], u: [[f64; 3]; 3]) -> TpiExactResult {
    let flatten = |tri: [[f64; 3]; 3]| -> [f64; 9] {
        [
            tri[0][0], tri[0][1], tri[0][2], tri[1][0], tri[1][1], tri[1][2], tri[2][0], tri[2][1],
            tri[2][2],
        ]
    };
    let v_arr = flatten(v);
    let w_arr = flatten(w);
    let u_arr = flatten(u);
    let mut lx_ptr: *mut f64 = core::ptr::null_mut();
    let mut lx_len: core::ffi::c_int = 0;
    let mut ly_ptr: *mut f64 = core::ptr::null_mut();
    let mut ly_len: core::ffi::c_int = 0;
    let mut lz_ptr: *mut f64 = core::ptr::null_mut();
    let mut lz_len: core::ffi::c_int = 0;
    let mut ld_ptr: *mut f64 = core::ptr::null_mut();
    let mut ld_len: core::ffi::c_int = 0;
    // Safety: input arrays are length 9; out-param pointers valid for
    // the duration of the call. Same shape as ip_lambda3d_lpi_exact.
    unsafe {
        ffi::ip_lambda3d_tpi_exact(
            v_arr.as_ptr(),
            w_arr.as_ptr(),
            u_arr.as_ptr(),
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
    TpiExactResult {
        lambda_x: copy_and_free(lx_ptr, lx_len),
        lambda_y: copy_and_free(ly_ptr, ly_len),
        lambda_z: copy_and_free(lz_ptr, lz_len),
        lambda_d: copy_and_free(ld_ptr, ld_len),
    }
}

// =========================================================================
// PR-CR-IP5 — ExplicitPoint3D opaque handle
// =========================================================================

/// Opaque handle to a C++ `explicitPoint3D` (a concrete 3D point with
/// x/y/z coordinates, subclass of `genericPoint`). Heap-allocated by
/// the FFI shim; freed via [`Drop`].
///
/// Cannot be cloned — that would risk double-free. Pass by reference
/// if you need to share access.
///
/// **Stub mode**: backed by a `malloc`'d `double[3]` buffer; the
/// coordinate round-trip is correct regardless of whether the
/// upstream Indirect_Predicates source is available.
///
/// **Send + Sync**: the C++ class has no thread-local state.
pub struct ExplicitPoint3D {
    ptr: core::ptr::NonNull<core::ffi::c_void>,
}

// Safety: `explicitPoint3D` is a value type holding three `double`s
// + a `Point_Type` tag (implicit_point.h:336-355). No thread-local
// state, no internal mutability. Send + Sync are sound.
unsafe impl Send for ExplicitPoint3D {}
unsafe impl Sync for ExplicitPoint3D {}

impl ExplicitPoint3D {
    /// Construct an explicit 3D point. Heap-allocates the C++ object
    /// via `ip_explicit_point3d_new`.
    ///
    /// # Panics
    ///
    /// Panics if the FFI shim returns null — i.e., out-of-memory.
    /// In well-resourced environments this never happens.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        // Safety: `ip_explicit_point3d_new` is declared `extern "C"`
        // with three `double` args and returns `void*`. It either
        // returns a valid heap pointer or null on OOM.
        let raw = unsafe { ffi::ip_explicit_point3d_new(x, y, z) };
        let ptr = core::ptr::NonNull::new(raw)
            .expect("ip_explicit_point3d_new returned null (out-of-memory?)");
        Self { ptr }
    }

    /// Read the x coordinate.
    pub fn x(&self) -> f64 {
        // Safety: `self.ptr` is a valid heap pointer to an
        // `explicitPoint3D` instance for as long as `self` exists
        // (Drop has not yet run). The accessor shim reads x through
        // the C++ class accessor.
        unsafe { ffi::ip_explicit_point3d_x(self.ptr.as_ptr()) }
    }

    /// Read the y coordinate.
    pub fn y(&self) -> f64 {
        unsafe { ffi::ip_explicit_point3d_y(self.ptr.as_ptr()) }
    }

    /// Read the z coordinate.
    pub fn z(&self) -> f64 {
        unsafe { ffi::ip_explicit_point3d_z(self.ptr.as_ptr()) }
    }

    /// Crate-internal accessor: a `const genericPoint*` view of the
    /// underlying C++ object, suitable for passing to predicate
    /// shims (PR-CR-IP6). The C++ subclass-to-base implicit
    /// conversion is type-safe at the C++ side; our `void*` carries
    /// the same address.
    #[allow(dead_code)] // Used by PR-CR-IP6+.
    pub(crate) fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

impl Drop for ExplicitPoint3D {
    fn drop(&mut self) {
        // Safety: `self.ptr` was produced by `ip_explicit_point3d_new`
        // and has not been freed. The shim calls `delete (explicitPoint3D*)p`
        // (or `free(p)` in stub mode).
        unsafe { ffi::ip_explicit_point3d_drop(self.ptr.as_ptr()) };
    }
}

impl core::fmt::Debug for ExplicitPoint3D {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExplicitPoint3D")
            .field("x", &self.x())
            .field("y", &self.y())
            .field("z", &self.z())
            .finish()
    }
}

// =========================================================================
// PR-CR-IP5b — ImplicitPoint3DLpi<'a> + ImplicitPoint3DTpi<'a>
// (lifetime-parameterized opaque handles)
// =========================================================================

/// Opaque handle to a C++ `implicitPoint3D_LPI` (line-plane
/// intersection point). Holds `&'a ExplicitPoint3D` references to
/// 5 explicit points: `p, q` define the line; `r, s, t` define the
/// plane.
///
/// The lifetime parameter `'a` ensures the implicit point cannot
/// outlive any of its referenced explicit points — preventing the
/// C++ class from dereferencing dangling references (which would
/// be UB).
///
/// # Borrow checker example
///
/// ```compile_fail
/// use indirect_predicates_sidecar_rs::{ExplicitPoint3D, ImplicitPoint3DLpi};
/// let lpi;
/// {
///     let p = ExplicitPoint3D::new(1.0, 2.0, 3.0);
///     let q = ExplicitPoint3D::new(5.0, 7.0, 9.0);
///     let r = ExplicitPoint3D::new(0.0, 0.0, 0.0);
///     let s = ExplicitPoint3D::new(1.0, 0.0, 0.0);
///     let t = ExplicitPoint3D::new(0.0, 1.0, 0.0);
///     lpi = ImplicitPoint3DLpi::new(&p, &q, &r, &s, &t);
/// } // p..t dropped here
/// // `lpi` would now hold dangling references — borrow checker rejects.
/// let _ = lpi;
/// ```
pub struct ImplicitPoint3DLpi<'a> {
    ptr: core::ptr::NonNull<core::ffi::c_void>,
    _phantom: core::marker::PhantomData<&'a ExplicitPoint3D>,
}

// Safety: per recon (cherchi_rs_pr_cr_ip5.md banked findings), the
// implicit point's mutable interval cache is filled once during
// construction (single-threaded) and read-only thereafter. The
// instance carries no thread-local state; pools are global-per-
// thread, not per-instance. Send + Sync are sound, conditional on
// the referenced `ExplicitPoint3D: Sync` (established in IP5).
unsafe impl<'a> Send for ImplicitPoint3DLpi<'a> {}
unsafe impl<'a> Sync for ImplicitPoint3DLpi<'a> {}

impl<'a> ImplicitPoint3DLpi<'a> {
    /// Construct an implicit line-plane intersection point.
    ///
    /// # Panics
    ///
    /// Panics if the FFI shim returns null (OOM).
    pub fn new(
        p: &'a ExplicitPoint3D,
        q: &'a ExplicitPoint3D,
        r: &'a ExplicitPoint3D,
        s: &'a ExplicitPoint3D,
        t: &'a ExplicitPoint3D,
    ) -> Self {
        // Safety: the 5 input references are valid for `'a` (proven
        // by the borrow checker); the shim reinterprets each
        // `*const c_void` back to `const explicitPoint3D*` and
        // dereferences it (well-defined because the void pointers
        // come from `ExplicitPoint3D::as_generic_ptr()` which
        // returns the same underlying address as the C++ object).
        let raw = unsafe {
            ffi::ip_implicit_point3d_lpi_new(
                p.as_generic_ptr(),
                q.as_generic_ptr(),
                r.as_generic_ptr(),
                s.as_generic_ptr(),
                t.as_generic_ptr(),
            )
        };
        let ptr = core::ptr::NonNull::new(raw)
            .expect("ip_implicit_point3d_lpi_new returned null (out-of-memory?)");
        Self {
            ptr,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Crate-internal accessor for PR-CR-IP6 predicate shims.
    #[allow(dead_code)] // Used by PR-CR-IP6+.
    pub(crate) fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

impl<'a> Drop for ImplicitPoint3DLpi<'a> {
    fn drop(&mut self) {
        // Safety: `self.ptr` was produced by `ip_implicit_point3d_lpi_new`
        // and has not been freed. The shim calls
        // `delete (implicitPoint3D_LPI*)p` (or `free` in stub mode).
        unsafe { ffi::ip_implicit_point3d_lpi_drop(self.ptr.as_ptr()) };
    }
}

impl<'a> core::fmt::Debug for ImplicitPoint3DLpi<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ImplicitPoint3DLpi")
            .field("ptr", &self.ptr.as_ptr())
            .finish()
    }
}

/// Opaque handle to a C++ `implicitPoint3D_TPI` (triangle-plane
/// intersection point). Holds `&'a ExplicitPoint3D` references to
/// 9 explicit points: three triangles (v, w, u) × 3 vertices each.
///
/// Same lifetime + Send/Sync rationale as [`ImplicitPoint3DLpi`].
pub struct ImplicitPoint3DTpi<'a> {
    ptr: core::ptr::NonNull<core::ffi::c_void>,
    _phantom: core::marker::PhantomData<&'a ExplicitPoint3D>,
}

unsafe impl<'a> Send for ImplicitPoint3DTpi<'a> {}
unsafe impl<'a> Sync for ImplicitPoint3DTpi<'a> {}

impl<'a> ImplicitPoint3DTpi<'a> {
    /// Construct an implicit triangle-plane intersection point
    /// from 3 triangles (each defined by 3 vertices).
    ///
    /// # Panics
    ///
    /// Panics if the FFI shim returns null (OOM).
    #[allow(clippy::too_many_arguments)] // 9 references mirror the C++ constructor signature.
    pub fn new(
        v1: &'a ExplicitPoint3D,
        v2: &'a ExplicitPoint3D,
        v3: &'a ExplicitPoint3D,
        w1: &'a ExplicitPoint3D,
        w2: &'a ExplicitPoint3D,
        w3: &'a ExplicitPoint3D,
        u1: &'a ExplicitPoint3D,
        u2: &'a ExplicitPoint3D,
        u3: &'a ExplicitPoint3D,
    ) -> Self {
        // Safety: see ImplicitPoint3DLpi::new — same justification with
        // 9 input references instead of 5.
        let raw = unsafe {
            ffi::ip_implicit_point3d_tpi_new(
                v1.as_generic_ptr(),
                v2.as_generic_ptr(),
                v3.as_generic_ptr(),
                w1.as_generic_ptr(),
                w2.as_generic_ptr(),
                w3.as_generic_ptr(),
                u1.as_generic_ptr(),
                u2.as_generic_ptr(),
                u3.as_generic_ptr(),
            )
        };
        let ptr = core::ptr::NonNull::new(raw)
            .expect("ip_implicit_point3d_tpi_new returned null (out-of-memory?)");
        Self {
            ptr,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Crate-internal accessor for PR-CR-IP6 predicate shims.
    #[allow(dead_code)] // Used by PR-CR-IP6+.
    pub(crate) fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

impl<'a> Drop for ImplicitPoint3DTpi<'a> {
    fn drop(&mut self) {
        unsafe { ffi::ip_implicit_point3d_tpi_drop(self.ptr.as_ptr()) };
    }
}

impl<'a> core::fmt::Debug for ImplicitPoint3DTpi<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ImplicitPoint3DTpi")
            .field("ptr", &self.ptr.as_ptr())
            .finish()
    }
}

// =========================================================================
// PR-CR-IP6 — Sign + AsGenericPoint sealed trait + orient3d + comparators
// =========================================================================

/// Sign result from upstream predicates. Mirrors `IP_Sign`
/// (implicit_point.h:51-59).
///
/// - `Negative` (-1): orientation below; first argument greater.
/// - `Zero` (0): coplanar / equal coordinate.
/// - `Positive` (+1): orientation above; first argument lesser.
/// - `Undefined` (2): NaN input, catastrophic cancellation, or
///   stub mode (when the upstream library isn't linked).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Sign {
    Negative = -1,
    Zero = 0,
    Positive = 1,
    Undefined = 2,
}

impl Sign {
    /// Map a raw C++ predicate return value to this enum.
    ///
    /// Defensive: any value outside the four documented domains
    /// is mapped to `Undefined` rather than panicking.
    pub const fn from_int(i: i32) -> Self {
        match i {
            -1 => Self::Negative,
            0 => Self::Zero,
            1 => Self::Positive,
            2 => Self::Undefined,
            _ => Self::Undefined,
        }
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ExplicitPoint3D {}
    impl<'a> Sealed for super::ImplicitPoint3DLpi<'a> {}
    impl<'a> Sealed for super::ImplicitPoint3DTpi<'a> {}
}

/// Marker trait for our crate's point handle types
/// (`ExplicitPoint3D`, `ImplicitPoint3DLpi<'_>`, `ImplicitPoint3DTpi<'_>`).
///
/// **Sealed**: external crates cannot implement this trait — only
/// the three handle types in this crate satisfy it. The single
/// method `as_generic_ptr` is `#[doc(hidden)]` because it returns
/// a raw pointer that's only meaningful inside our predicate
/// shims.
pub trait AsGenericPoint: sealed::Sealed {
    #[doc(hidden)]
    fn as_generic_ptr(&self) -> *const core::ffi::c_void;
}

impl AsGenericPoint for ExplicitPoint3D {
    fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

impl<'a> AsGenericPoint for ImplicitPoint3DLpi<'a> {
    fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

impl<'a> AsGenericPoint for ImplicitPoint3DTpi<'a> {
    fn as_generic_ptr(&self) -> *const core::ffi::c_void {
        self.ptr.as_ptr()
    }
}

/// 4-point orientation predicate. Wraps `orient3d_indirect_IIII`.
///
/// - `Positive` if `p4` lies above the plane defined by `p1, p2, p3`
///   in CCW orientation.
/// - `Zero` if all four points are coplanar.
/// - `Negative` if `p4` lies below.
/// - `Undefined` if the upstream cascade exhausts at NaN / overflow
///   (rare) or in stub mode.
///
/// Accepts any combination of `&ExplicitPoint3D`,
/// `&ImplicitPoint3DLpi<'_>`, `&ImplicitPoint3DTpi<'_>` via the
/// sealed `AsGenericPoint` trait. Internally always calls
/// `orient3d_indirect_IIII`, which dispatches on the C++ side
/// based on each point's `Point_Type` tag.
pub fn orient3d(
    p1: &impl AsGenericPoint,
    p2: &impl AsGenericPoint,
    p3: &impl AsGenericPoint,
    p4: &impl AsGenericPoint,
) -> Sign {
    // Safety: each `as_generic_ptr()` returns a valid pointer to a
    // C++ genericPoint subclass object (single inheritance: base
    // address equals subclass address). The FFI shim reinterprets
    // as `const genericPoint*` and dereferences for the C++
    // reference parameter. All four points are borrowed for the
    // duration of the call.
    let r = unsafe {
        ffi::ip_orient3d_indirect_iiii(
            p1.as_generic_ptr(),
            p2.as_generic_ptr(),
            p3.as_generic_ptr(),
            p4.as_generic_ptr(),
        )
    };
    Sign::from_int(r)
}

/// Per-axis comparator on x. Wraps `genericPoint::lessThanOnX`.
///
/// Returns `Sign::Positive` iff `p1.x < p2.x`; `Zero` if equal;
/// `Negative` if `p1.x > p2.x` — but **only** when at least one
/// argument is an implicit point (LPI or TPI).
///
/// **Explicit-vs-explicit caveat**: upstream's EE branch returns
/// `a.X() < b.X()` as `int` (bool → 0 or 1). The "greater" case
/// is mapped to `Zero` instead of `Negative`. This matches the
/// Cherchi 2022 §6.4 boolean-labeling algorithm's actual usage
/// (always implicit-implicit). If you need full Sign semantics
/// between two explicit points, compare their `x()` accessors
/// directly.
pub fn less_than_on_x(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign {
    let r = unsafe { ffi::ip_less_than_on_x_ii(p1.as_generic_ptr(), p2.as_generic_ptr()) };
    Sign::from_int(r)
}

/// Per-axis comparator on y. Wraps `genericPoint::lessThanOnY`.
/// See [`less_than_on_x`] for EE-branch caveat.
pub fn less_than_on_y(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign {
    let r = unsafe { ffi::ip_less_than_on_y_ii(p1.as_generic_ptr(), p2.as_generic_ptr()) };
    Sign::from_int(r)
}

/// Per-axis comparator on z. Wraps `genericPoint::lessThanOnZ`.
/// See [`less_than_on_x`] for EE-branch caveat.
pub fn less_than_on_z(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign {
    let r = unsafe { ffi::ip_less_than_on_z_ii(p1.as_generic_ptr(), p2.as_generic_ptr()) };
    Sign::from_int(r)
}

// =========================================================================
// PR-CR-AR2a Cycle 1 (CR-IP6b) — orient2d_{xy,yz,zx} + point_in_triangle
// =========================================================================

/// 2D orientation predicate on the `xy` projection. Wraps
/// `genericPoint::orient2Dxy`.
///
/// Projects the triple onto the `(x, y)` plane and returns the
/// CCW / left-turn sign:
///
/// - `Positive` if `(a, b, c)` is counter-clockwise.
/// - `Negative` if clockwise.
/// - `Zero` if collinear.
/// - `Undefined` on NaN / catastrophic cancellation, or in stub mode.
///
/// Accepts any combination of `&ExplicitPoint3D`,
/// `&ImplicitPoint3DLpi<'_>`, `&ImplicitPoint3DTpi<'_>` via the sealed
/// `AsGenericPoint` trait; the C++ side dispatches on each point's
/// `Point_Type` tag.
pub fn orient2d_xy(
    p1: &impl AsGenericPoint,
    p2: &impl AsGenericPoint,
    p3: &impl AsGenericPoint,
) -> Sign {
    // Safety: each `as_generic_ptr()` returns a valid pointer to a
    // C++ genericPoint subclass object (single inheritance: base
    // address equals subclass address). The FFI shim reinterprets
    // as `const genericPoint*` and dereferences for the C++
    // reference parameter. All three points are borrowed for the
    // duration of the call.
    let r = unsafe {
        ffi::ip_orient2d_xy(
            p1.as_generic_ptr(),
            p2.as_generic_ptr(),
            p3.as_generic_ptr(),
        )
    };
    Sign::from_int(r)
}

/// 2D orientation predicate on the `yz` projection. Wraps
/// `genericPoint::orient2Dyz`. See [`orient2d_xy`] for sign
/// conventions and stub-mode behavior.
pub fn orient2d_yz(
    p1: &impl AsGenericPoint,
    p2: &impl AsGenericPoint,
    p3: &impl AsGenericPoint,
) -> Sign {
    // Safety: see [`orient2d_xy`].
    let r = unsafe {
        ffi::ip_orient2d_yz(
            p1.as_generic_ptr(),
            p2.as_generic_ptr(),
            p3.as_generic_ptr(),
        )
    };
    Sign::from_int(r)
}

/// 2D orientation predicate on the `zx` projection. Wraps
/// `genericPoint::orient2Dzx`. See [`orient2d_xy`] for sign
/// conventions and stub-mode behavior.
pub fn orient2d_zx(
    p1: &impl AsGenericPoint,
    p2: &impl AsGenericPoint,
    p3: &impl AsGenericPoint,
) -> Sign {
    // Safety: see [`orient2d_xy`].
    let r = unsafe {
        ffi::ip_orient2d_zx(
            p1.as_generic_ptr(),
            p2.as_generic_ptr(),
            p3.as_generic_ptr(),
        )
    };
    Sign::from_int(r)
}

/// Boundary-inclusive point-in-triangle test. Wraps
/// `genericPoint::pointInTriangle`.
///
/// Returns `true` when `p` lies inside **or on the boundary** (an
/// edge or vertex) of triangle `a, b, c`, and `false` when strictly
/// outside.
///
/// In stub mode (`cfg!(ip_unavailable)`), always returns `false`.
///
/// Accepts any combination of `&ExplicitPoint3D`,
/// `&ImplicitPoint3DLpi<'_>`, `&ImplicitPoint3DTpi<'_>` via the sealed
/// `AsGenericPoint` trait; the C++ side dispatches on each point's
/// `Point_Type` tag.
pub fn point_in_triangle(
    p: &impl AsGenericPoint,
    a: &impl AsGenericPoint,
    b: &impl AsGenericPoint,
    c: &impl AsGenericPoint,
) -> bool {
    // Safety: each `as_generic_ptr()` returns a valid pointer to a
    // C++ genericPoint subclass object (single inheritance: base
    // address equals subclass address). The FFI shim reinterprets
    // as `const genericPoint*` and dereferences for the C++
    // reference parameter. All four points are borrowed for the
    // duration of the call.
    let r = unsafe {
        ffi::ip_point_in_triangle(
            p.as_generic_ptr(),
            a.as_generic_ptr(),
            b.as_generic_ptr(),
            c.as_generic_ptr(),
        )
    };
    r != 0
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

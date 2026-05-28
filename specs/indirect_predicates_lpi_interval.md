# `IntervalNumber` + `lambda3d_LPI_interval` + FPU init — PR-CR-IP2

## Goal

Ship the **first real predicate wrapper** in `indirect-predicates-sidecar-rs`: line-plane intersection coordinates in interval arithmetic, plus the supporting `IntervalNumber` Rust type and `init_fpu()` entry point. This is the predicate at the center of Cherchi 2022 §6.4's boolean labeling cascade (filtered → interval → exact).

After PR-CR-IP2, the crate exposes:
- `IntervalNumber { inf: f64, sup: f64 }` — interval representation crossing the Rust/C++ boundary.
- `LpiIntervalResult { lambda_x, lambda_y, lambda_z, lambda_d, reliable }` — the four interval-arithmetic outputs of `lambda3d_LPI_interval` plus the `signIsReliable()` flag.
- `init_fpu()` — idempotent wrapper around the upstream `initFPU()`; documented "call once per thread" though no-op on 64-bit Linux without `USE_SIMD_INSTRUCTIONS`.
- `lambda3d_lpi_interval(p, q, r, s, t: [IntervalNumber; 3]) -> LpiIntervalResult`.

## Public API

```rust
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct IntervalNumber {
    pub inf: f64,
    pub sup: f64,
}

impl IntervalNumber {
    pub const fn new(inf: f64, sup: f64) -> Self;
    pub const fn point(x: f64) -> Self;   // [x, x]
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LpiIntervalResult {
    pub lambda_x: IntervalNumber,
    pub lambda_y: IntervalNumber,
    pub lambda_z: IntervalNumber,
    pub lambda_d: IntervalNumber,
    /// `true` iff `lambda_d.signIsReliable()` — i.e., the denominator
    /// interval does not straddle zero. When `false`, fall back to
    /// `lambda3d_LPI_exact` (PR-CR-IP3).
    pub reliable: bool,
}

/// Idempotent FPU initialization. Call once per thread before using
/// interval-arithmetic predicates. On 64-bit Linux without
/// `USE_SIMD_INSTRUCTIONS`, this is a no-op.
pub fn init_fpu();

/// Line-plane intersection in interval arithmetic.
///
/// - `p, q`: two points defining the line.
/// - `r, s, t`: three points defining the plane.
///
/// Returns the four lambda interval values + a `reliable` flag. When
/// `reliable == false`, the determinant `lambda_d` straddles zero and
/// the result should be recomputed via `lambda3d_LPI_exact` (PR-CR-IP3).
pub fn lambda3d_lpi_interval(
    p: [IntervalNumber; 3],
    q: [IntervalNumber; 3],
    r: [IntervalNumber; 3],
    s: [IntervalNumber; 3],
    t: [IntervalNumber; 3],
) -> LpiIntervalResult;
```

When `cfg!(ip_unavailable)`, `init_fpu()` is a no-op and `lambda3d_lpi_interval` returns all-zero lambdas with `reliable: false`.

## C++ source references

- **`interval_number` class** at `/home/claude/cherchi2022/.../include/numerics.h:200-301` (non-SIMD branch).
  - Storage: `double min_low, high;` — represents `[-min_low, high]`.
  - 2-arg constructor at line 218: `interval_number(double minf, double sup)` stores `min_low = minf` directly. To produce `inf() == x`, pass `minf = -x`.
  - Accessors: `inf()` returns `-min_low`, `sup()` returns `high`.
  - Sign queries: `signIsReliable()` returns `isNegative() || isPositive()` — i.e., interval doesn't contain zero (exclusive).
- **`initFPU`** at `/home/claude/cherchi2022/.../include/numerics.hpp:42-60`. No-op on 64-bit Linux without `USE_SIMD_INSTRUCTIONS`.
- **`lambda3d_LPI_interval` declaration** at `/home/claude/cherchi2022/.../include/indirect_predicates.h:68`.
- **Implementation** at `/home/claude/cherchi2022/.../include/indirect_predicates.hpp:7178-7225`. Sets `setFPUModeToRoundUP()` at line 7180, restores `setFPUModeToRoundNEAR()` at line 7222. Returns `bool` (`lambda_d.signIsReliable()`).

## FFI strategy

- `IntervalNumber` does **NOT** cross FFI by value. The C++ class has constructors, alignment requirements (16-byte under SIMD), and bindgen can't represent it reliably.
- Inputs are flattened to **arrays of doubles** in pair-encoded `(inf, sup)` order. Each point is 6 doubles (3 intervals × 2 bounds).
- Outputs are written into a fixed-size 8-double array `[lx_inf, lx_sup, ly_inf, ly_sup, lz_inf, lz_sup, ld_inf, ld_sup]` + a `bool*` for reliability.
- `wrapper.cpp` handles the `interval_number` ↔ `(inf, sup)` conversion at the boundary:
  - **Construct**: `interval_number(-inf, sup)` (2-arg ctor; pass negated inf).
  - **Destructure**: `lambda.inf()` and `lambda.sup()`.

This is the **canonical pattern** for all future interval-arithmetic shims (PR-CR-IP3, PR-CR-IP4).

## FPU mode

`lambda3d_LPI_interval` is **self-contained**: it sets UPWARD rounding on entry and restores TONEAREST on exit. The Rust caller does NOT need to manipulate FPU state per call.

`init_fpu()` is a one-time-per-thread setup. On x86-64 with SSE/SIMD (our default), the SSE rounding mode lives in MXCSR which is per-thread by design. The function is harmless to call repeatedly — idempotent semantics are preserved.

Multi-threaded policy is **banked** until cherchi-rs goes parallel. For v1 (single-threaded usage in PR-CR-IP7+), `init_fpu()` can be called once at process start.

## Algorithm

```text
lambda3d_lpi_interval(p, q, r, s, t):
    let p_arr = [p[0].inf, p[0].sup, p[1].inf, p[1].sup, p[2].inf, p[2].sup]
    // same for q_arr, r_arr, s_arr, t_arr (each 6 doubles)
    let mut lambda_out = [0.0_f64; 8]
    let mut reliable = false
    unsafe {
        ip_lambda3d_lpi_interval(
            p_arr.as_ptr(), q_arr.as_ptr(), r_arr.as_ptr(),
            s_arr.as_ptr(), t_arr.as_ptr(),
            lambda_out.as_mut_ptr(), &mut reliable,
        )
    }
    LpiIntervalResult {
        lambda_x: IntervalNumber::new(lambda_out[0], lambda_out[1]),
        lambda_y: IntervalNumber::new(lambda_out[2], lambda_out[3]),
        lambda_z: IntervalNumber::new(lambda_out[4], lambda_out[5]),
        lambda_d: IntervalNumber::new(lambda_out[6], lambda_out[7]),
        reliable,
    }

// wrapper.cpp:
extern "C" void ip_lambda3d_lpi_interval(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double* lambda_out, bool* reliable
) {
    interval_number px(-p[0], p[1]), py(-p[2], p[3]), pz(-p[4], p[5]);
    interval_number qx(-q[0], q[1]), qy(-q[2], q[3]), qz(-q[4], q[5]);
    interval_number rx(-r[0], r[1]), ry(-r[2], r[3]), rz(-r[4], r[5]);
    interval_number sx(-s[0], s[1]), sy(-s[2], s[3]), sz(-s[4], s[5]);
    interval_number tx(-t[0], t[1]), ty(-t[2], t[3]), tz(-t[4], t[5]);
    interval_number lx, ly, lz, ld;
    *reliable = lambda3d_LPI_interval(
        px, py, pz, qx, qy, qz, rx, ry, rz, sx, sy, sz, tx, ty, tz,
        lx, ly, lz, ld);
    lambda_out[0] = lx.inf(); lambda_out[1] = lx.sup();
    lambda_out[2] = ly.inf(); lambda_out[3] = ly.sup();
    lambda_out[4] = lz.inf(); lambda_out[5] = lz.sup();
    lambda_out[6] = ld.inf(); lambda_out[7] = ld.sup();
}
```

## Invariants

1. `IntervalNumber::point(x).inf == x && .sup == x`.
2. `IntervalNumber::new(a, b).inf == a && .sup == b` (no normalization, no validation).
3. `init_fpu()` is idempotent and never panics in either mode.
4. For a non-degenerate line/plane (line not parallel to plane), `lambda3d_lpi_interval` returns `reliable == true` and all four lambdas are non-NaN.
5. For a degenerate input (line parallel to or in the plane), `reliable == false`.
6. In stub mode (`cfg!(ip_unavailable)`), `lambda3d_lpi_interval` returns all lambdas zero, `reliable == false`.
7. PR-CR-IP1 contracts preserved: `link_probe()` still returns +1 / -2; `AVAILABLE` unchanged.

## Test plan (7 tests in `tests/smoke.rs`)

### Group A — types (3 tests)
1. `interval_number_point_constructor` — `IntervalNumber::point(3.0) == IntervalNumber::new(3.0, 3.0)`.
2. `interval_number_new_constructor` — verifies field round-trip; tolerates `inf > sup` (no validation).
3. `interval_number_copy_and_eq` — `Copy + PartialEq` traits behave.

### Group B — algorithm (4 tests)
4. `init_fpu_is_callable_and_idempotent` — call 5×, no panic.
5. `lambda3d_lpi_non_degenerate_is_reliable` — line `P=(1,2,3) → Q=(5,7,9)`, plane through `R=(0,0,0)`, `S=(1,0,0)`, `T=(0,1,0)` (i.e., z=0). Assert `reliable == true`, lambdas non-NaN. (`#[cfg(not(ip_unavailable))]`.)
6. `lambda3d_lpi_coplanar_is_unreliable` — line lies in the same plane (e.g., both endpoints have z=0; plane z=0). Assert `reliable == false`. (`#[cfg(not(ip_unavailable))]`.)
7. `lambda3d_lpi_stub_returns_zeros` — `#[cfg(ip_unavailable)]`. Asserts all lambdas zero and `reliable == false`.

## Limitations (banked)

1. No interval arithmetic methods on `IntervalNumber` (add/sub/mul) — banked PR-CR-IP2b if cherchi-rs needs to compose intervals in Rust.
2. No exact / bigfloat fallback yet — PR-CR-IP3.
3. No `lambda3d_TPI_*` (triangle-plane) — PR-CR-IP4.
4. No `genericPoint` opaque-handle — PR-CR-IP5.
5. No `orient3d_indirect_IIII` — PR-CR-IP6.
6. Multi-threaded FPU policy deferred — single-threaded usage assumed for v1.

## References

- `/home/claude/cherchi2022/.../include/numerics.h:200-301` — `interval_number` (non-SIMD branch).
- `/home/claude/cherchi2022/.../include/numerics.hpp:42-60` — `initFPU` definition.
- `/home/claude/cherchi2022/.../include/indirect_predicates.h:68` — `lambda3d_LPI_interval` declaration.
- `/home/claude/cherchi2022/.../include/indirect_predicates.hpp:7178-7225` — implementation (FPU set/restore at lines 7180/7222).
- `specs/indirect_predicates_sidecar_scaffold.md` — PR-CR-IP1 (scaffold).
- `memory/cherchi_rs_pr_cr_ip1.md` — scaffold conventions inherited.
- Cherchi 2022 §6.4 — boolean labeling via filtered/interval/exact cascade.

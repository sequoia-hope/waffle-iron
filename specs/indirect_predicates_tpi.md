# `lambda3d_TPI_interval` + `lambda3d_TPI_exact` — PR-CR-IP4

## Goal

Ship the **TPI (triangle-plane intersection)** wrappers at both
interval and exact tiers. Mirrors the LPI work in PR-CR-IP2 + IP3
with three planes (each defined by a triangle) instead of one line
+ one plane. TPI computes the point where the three planes meet.

After PR-CR-IP4, the LPI + TPI cascade is feature-complete at the
interval and exact tiers. The bigfloat tier (PR-CR-IP3b for LPI,
PR-CR-IP4b for TPI) remains banked.

## Public API

```rust
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TpiIntervalResult {
    pub lambda_x: IntervalNumber,
    pub lambda_y: IntervalNumber,
    pub lambda_z: IntervalNumber,
    pub lambda_d: IntervalNumber,
    pub reliable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TpiExactResult {
    pub lambda_x: Vec<f64>,
    pub lambda_y: Vec<f64>,
    pub lambda_z: Vec<f64>,
    pub lambda_d: Vec<f64>,
}

/// Triangle-plane intersection in interval arithmetic. Wraps
/// `lambda3d_TPI_interval`.
///
/// Each input triangle's vertices are passed as a `[[IntervalNumber; 3]; 3]`:
/// outer index = vertex (0..3), inner index = coordinate (x, y, z).
///
/// Returns interval lambdas + `reliable` flag. Same fallback semantics
/// as `lambda3d_lpi_interval`.
pub fn lambda3d_tpi_interval(
    v: [[IntervalNumber; 3]; 3],
    w: [[IntervalNumber; 3]; 3],
    u: [[IntervalNumber; 3]; 3],
) -> TpiIntervalResult;

/// Triangle-plane intersection in Shewchuk expansion arithmetic.
/// Wraps `lambda3d_TPI_exact`.
///
/// Each input triangle's vertices are passed as a `[[f64; 3]; 3]`:
/// outer index = vertex (0..3), inner index = coordinate (x, y, z).
pub fn lambda3d_tpi_exact(
    v: [[f64; 3]; 3],
    w: [[f64; 3]; 3],
    u: [[f64; 3]; 3],
) -> TpiExactResult;
```

In stub mode (`cfg!(ip_unavailable)`): interval returns all-zero
lambdas + `reliable: false`; exact returns four empty Vecs.

## C++ source references

- **`lambda3d_TPI_interval` declaration** at `indirect_predicates.h:71`.
  Signature: 27 `interval_number` inputs (9 points × 3 coords) + 4
  `interval_number&` outputs + `bool` return.
- **Implementation** at `indirect_predicates.hpp:7364-7476`. Sets
  FPU UPWARD at 7366, restores TONEAREST at 7474. Returns
  `lambda_d.signIsReliable()` at 7476.
- **`lambda3d_TPI_exact` declaration** at `indirect_predicates.h:72`.
  Signature: 27 `double` inputs + 4 (`double**`, `int&`) output pairs.
- **Implementation** at `indirect_predicates.hpp:7590-7814`. Uses
  `Gen_*_With_PreAlloc` pattern; allocates from
  `expansionObject::mempool` when initial buffers are null+0.
- Same `AllocDoubles` / `FreeDoubles` pool mechanism as LPI exact
  (PR-CR-IP3).

## FFI signatures

```c
// wrapper.h
void ip_lambda3d_tpi_interval(
    const double* v,            // 18 doubles per triangle:
    const double* w,            // (vert0 xi, xs, yi, ys, zi, zs, vert1 ..., vert2 ...)
    const double* u,
    double* lambda_out,         // 8 doubles: lx_inf, lx_sup, ly_inf, ..., ld_sup
    bool* reliable);

void ip_lambda3d_tpi_exact(
    const double* v,            // 9 doubles per triangle:
    const double* w,            // (vert0 x, y, z, vert1 ..., vert2 ...)
    const double* u,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len);
```

Reuses the established conventions:
- Interval shim uses `ip_from_pair(inf, sup)` (from PR-CR-IP2).
- Exact shim passes initial null+0 to trigger pool allocation.
- Rust safe wrapper uses `copy_and_free` (from PR-CR-IP3) for byte-
  level alignment-relaxed copying.

## Algorithm

```text
// Rust:
lambda3d_tpi_interval(v, w, u):
    flatten each triangle into 18 doubles (inf/sup pairs per coord):
        v_arr[0..2]  = v[0][0].inf, .sup
        v_arr[2..4]  = v[0][1].inf, .sup
        ...
        v_arr[16..18] = v[2][2].inf, .sup
    let mut lambda_out = [0.0_f64; 8]
    let mut reliable = false
    unsafe { ip_lambda3d_tpi_interval(v_arr.as_ptr(), w_arr.as_ptr(), u_arr.as_ptr(),
                                       lambda_out.as_mut_ptr(), &mut reliable) }
    TpiIntervalResult {
        lambda_x: IntervalNumber::new(lambda_out[0], lambda_out[1]),
        lambda_y: IntervalNumber::new(lambda_out[2], lambda_out[3]),
        lambda_z: IntervalNumber::new(lambda_out[4], lambda_out[5]),
        lambda_d: IntervalNumber::new(lambda_out[6], lambda_out[7]),
        reliable,
    }

lambda3d_tpi_exact(v, w, u):
    flatten each triangle into 9 doubles
    let mut lx_ptr, lx_len, ly_ptr, ly_len, lz_ptr, lz_len, ld_ptr, ld_len  // all (null, 0)
    unsafe { ip_lambda3d_tpi_exact(v_arr.as_ptr(), w_arr.as_ptr(), u_arr.as_ptr(),
                                    &mut lx_ptr, &mut lx_len, &mut ly_ptr, &mut ly_len,
                                    &mut lz_ptr, &mut lz_len, &mut ld_ptr, &mut ld_len) }
    TpiExactResult {
        lambda_x: copy_and_free(lx_ptr, lx_len),
        ...
        lambda_d: copy_and_free(ld_ptr, ld_len),
    }

// wrapper.cpp (TPI interval shim):
ip_lambda3d_tpi_interval(v, w, u, lambda_out, reliable):
    interval_number v1x = ip_from_pair(v[0], v[1]),  v1y = ip_from_pair(v[2], v[3]),  v1z = ip_from_pair(v[4], v[5]);
    interval_number v2x = ip_from_pair(v[6], v[7]),  v2y = ip_from_pair(v[8], v[9]),  v2z = ip_from_pair(v[10], v[11]);
    interval_number v3x = ip_from_pair(v[12], v[13]), v3y = ip_from_pair(v[14], v[15]), v3z = ip_from_pair(v[16], v[17]);
    // ... same unpacking for w and u
    interval_number lx, ly, lz, ld;
    bool ok = lambda3d_TPI_interval(
        v1x,v1y,v1z, v2x,v2y,v2z, v3x,v3y,v3z,
        w1x,w1y,w1z, w2x,w2y,w2z, w3x,w3y,w3z,
        u1x,u1y,u1z, u2x,u2y,u2z, u3x,u3y,u3z,
        lx, ly, lz, ld);
    lambda_out[0] = lx.inf(); lambda_out[1] = lx.sup();
    // ... 6 more
    *reliable = ok;

// wrapper.cpp (TPI exact shim):
ip_lambda3d_tpi_exact(v, w, u, lx_out, lx_len, ..., ld_len):
    double* lx = nullptr; int lxn = 0;
    // ... ly, lz, ld
    lambda3d_TPI_exact(
        v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8],
        w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7], w[8],
        u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7], u[8],
        &lx, lxn, &ly, lyn, &lz, lzn, &ld, ldn);
    *lambda_x_out = lx; *lambda_x_len = lxn;
    ...
```

## Invariants

1. `lambda3d_tpi_interval` for 3 non-coplanar non-parallel planes
   returns `reliable=true` with non-NaN lambdas.
2. For 2+ parallel planes: `reliable=false`.
3. `lambda3d_tpi_exact` returns 4 non-empty `Vec<f64>` with finite
   entries on non-degenerate input.
4. **Cross-tier consistency**: `sum(exact.lambda_d) ∈ interval.lambda_d.inf..=sup`.
5. Parallel-plane input to exact: `sum(lambda_d).abs() < 1e-12`.
6. Stub mode: interval all-zeros + `reliable=false`; exact 4 empty
   Vecs.
7. All PR-CR-IP1..IP3 contracts preserved.
8. No raw pointers leak across the public API.

## Error contract

No errors. Both functions always return a result struct.

## Limitations (banked)

1. No bigfloat tier (PR-CR-IP4b banks `lambda3d_TPI_bigfloat`,
   analogous to IP3b for LPI).
2. No `genericPoint` opaque-handle yet (PR-CR-IP5).
3. No `orient3d_indirect_IIII` yet (PR-CR-IP6).
4. No cherchi-rs Stage 2 integration yet (PR-CR-IP7).
5. Result types (`TpiIntervalResult` / `TpiExactResult`) are
   structurally identical to their LPI counterparts. Deduplication
   into unified `IntervalLambda` / `ExactLambda` types is a banked
   future refactor PR.

## Test plan (8 tests in `tests/smoke.rs`)

### Group A — types (3 tests, run in both modes)
1. `tpi_interval_result_construct_and_eq`
2. `tpi_exact_result_default_empty`
3. `tpi_exact_result_clone_and_eq`

### Group B — algorithm (5 tests, `#[cfg(not(ip_unavailable))]`)
4. `lambda3d_tpi_interval_orthogonal_planes_reliable` — 3 coord
   planes; reliable + non-NaN.
5. `lambda3d_tpi_interval_parallel_planes_unreliable` — 2 parallel
   + 1 other; not reliable.
6. `lambda3d_tpi_exact_orthogonal_non_empty` — 4 non-empty Vecs +
   finite entries.
7. `lambda3d_tpi_exact_parallel_d_approximately_zero` — parallel
   input; `sum(lambda_d).abs() < 1e-12`.
8. `lambda3d_tpi_exact_agrees_with_interval` — cross-tier:
   `sum(exact.lambda_d) ∈ interval.lambda_d` range.

Stub-mode test coverage is already provided by IP1/IP2/IP3
cfg-gated tests — no need to duplicate here.

## Honest framing

PR-CR-IP4 is mechanical replication of the patterns established in
PR-CR-IP2 + IP3 with one substitution (3-triangle TPI input instead
of 5-point LPI input). No new FFI mechanism. No algorithmic
deviation from upstream `lambda3d_TPI_*`.

## References

- `/home/claude/cherchi2022/.../include/indirect_predicates.h:71-72`
- `/home/claude/cherchi2022/.../include/indirect_predicates.hpp:7364, 7590`
- Cherchi 2022 §6.4 — boolean labeling cascade.
- `specs/indirect_predicates_lpi_interval.md` — PR-CR-IP2 (LPI interval).
- `specs/indirect_predicates_lpi_exact.md` — PR-CR-IP3 (LPI exact).
- `memory/cherchi_rs_pr_cr_ip2.md` + `memory/cherchi_rs_pr_cr_ip3.md` — established FFI conventions.

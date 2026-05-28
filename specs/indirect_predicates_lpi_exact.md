# `lambda3d_LPI_exact` (Shewchuk expansion arithmetic) — PR-CR-IP3

## Goal

Ship the **exact** tier of the LPI cascade: `lambda3d_LPI_exact`. This
is the predicate that downstream cherchi-rs Stage 2 falls back to
when `lambda3d_LPI_interval` (PR-CR-IP2) returns `reliable: false`
(denominator interval straddles zero).

The novel mechanism is **Shewchuk expansion arithmetic**: each output
lambda is a variable-length array of doubles (not a single double,
not an interval). The C++ function allocates the result from a
**thread-local memory pool** (`expansionObject::mempool`); the Rust
wrapper copies the data to an owned `Vec<f64>` and returns the pool
memory.

**Scope**: PR-CR-IP3 ships **exact only**. The `bigfloat` tier and
its `lambda3d_LPI_bigfloat` wrapper are PR-CR-IP3b (different FFI
mechanism — opaque C++ object handles, Drop semantics).

## Public API

```rust
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LpiExactResult {
    pub lambda_x: Vec<f64>,
    pub lambda_y: Vec<f64>,
    pub lambda_z: Vec<f64>,
    pub lambda_d: Vec<f64>,
}

/// Exact line-plane intersection via Shewchuk expansion arithmetic.
///
/// - `p, q`: two points defining the line.
/// - `r, s, t`: three points defining the plane.
///
/// Returns the four lambda expansions (variable-length arrays of
/// doubles). The geometric value of each lambda is the SUM of the
/// entries in its `Vec<f64>` (an "expansion of doubles" per
/// Shewchuk 1997). The denominator `lambda_d` is exactly zero iff
/// the line is parallel to or contained in the plane.
///
/// In stub mode (`cfg!(ip_unavailable)`), returns 4 empty Vecs.
pub fn lambda3d_lpi_exact(
    p: [f64; 3],
    q: [f64; 3],
    r: [f64; 3],
    s: [f64; 3],
    t: [f64; 3],
) -> LpiExactResult;
```

## C++ source references

- **`lambda3d_LPI_exact` declaration** at `/home/claude/cherchi2022/.../include/indirect_predicates.h:69`:
  ```cpp
  void lambda3d_LPI_exact(double px, double py, double pz, double qx, double qy, double qz,
                          double rx, double ry, double rz, double sx, double sy, double sz,
                          double tx, double ty, double tz,
                          double **lambda_x, int& lambda_x_len,
                          double **lambda_y, int& lambda_y_len,
                          double **lambda_z, int& lambda_z_len,
                          double **lambda_d, int& lambda_d_len);
  ```
- **Implementation** at `indirect_predicates.hpp:7273+`. Uses
  stack-allocated intermediate arrays (`double a11[2]`, ...) and
  builds the final expansions via
  `expansionObject::Gen_*_With_PreAlloc`.
- **Pool allocator** at `numerics.h:330-333`:
  ```c
  #define AllocDoubles(n) ((double *)expansionObject::mempool.alloc((n) * sizeof(double)))
  #define FreeDoubles(p) (expansionObject::mempool.release(p))
  ```
- **`expansionObject::mempool`**: `thread_local MultiPool` (numerics.h:339).

## FFI memory model

The cleanest approach: **pass `null pointer + length 0`** as the
initial expansion buffer. The C++ function's `Gen_Diff_With_PreAlloc`
checks `if (hlen < newlen) *h = AllocDoubles(newlen)` (numerics.h:448)
— with `hlen = 0`, this condition triggers unconditionally, and the
callee allocates the entire result from the pool.

After the call:
- Each `lambda_*` pointer points to pool-allocated memory.
- Each `lambda_*_len` is the actual expansion length.
- The Rust side: copies the slice to `Vec<f64>`, calls
  `ip_free_doubles` to return the buffer to the pool.

This is the **canonical FFI pattern for variable-length expansion
outputs** — PR-CR-IP4 (TPI exact) will reuse it.

### Thread safety

`expansionObject::mempool` is `thread_local`. The Rust safe wrapper
calls `ip_lambda3d_lpi_exact`, copies to Vec, and calls
`ip_free_doubles` ALL inline on the same thread. The pool memory
never crosses thread boundaries. ✓

## Algorithm

```text
lambda3d_lpi_exact(p, q, r, s, t):
    let mut lx_ptr: *mut f64 = null_mut
    let mut lx_len: c_int = 0
    // ... ly_ptr, lz_ptr, ld_ptr similarly
    unsafe {
        ip_lambda3d_lpi_exact(
            p.as_ptr(), q.as_ptr(), r.as_ptr(),
            s.as_ptr(), t.as_ptr(),
            &mut lx_ptr, &mut lx_len,
            &mut ly_ptr, &mut ly_len,
            &mut lz_ptr, &mut lz_len,
            &mut ld_ptr, &mut ld_len,
        );
    }
    LpiExactResult {
        lambda_x: copy_and_free(lx_ptr, lx_len),
        lambda_y: copy_and_free(ly_ptr, ly_len),
        lambda_z: copy_and_free(lz_ptr, lz_len),
        lambda_d: copy_and_free(ld_ptr, ld_len),
    }

copy_and_free(ptr, len):
    if ptr.is_null() || len <= 0: return Vec::new()
    let slice = unsafe { from_raw_parts(ptr, len as usize) }
    let v: Vec<f64> = slice.to_vec()
    unsafe { ip_free_doubles(ptr) }
    v


// wrapper.cpp:
extern "C" void ip_lambda3d_lpi_exact(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double** lx_out, int* lx_len,
    double** ly_out, int* ly_len,
    double** lz_out, int* lz_len,
    double** ld_out, int* ld_len
) {
    double* lx = nullptr; int lxn = 0;
    double* ly = nullptr; int lyn = 0;
    double* lz = nullptr; int lzn = 0;
    double* ld = nullptr; int ldn = 0;
    // Initial null + 0 → callee allocates from expansionObject::mempool
    lambda3d_LPI_exact(
        p[0], p[1], p[2], q[0], q[1], q[2],
        r[0], r[1], r[2], s[0], s[1], s[2], t[0], t[1], t[2],
        &lx, lxn, &ly, lyn, &lz, lzn, &ld, ldn);
    *lx_out = lx; *lx_len = lxn;
    *ly_out = ly; *ly_len = lyn;
    *lz_out = lz; *lz_len = lzn;
    *ld_out = ld; *ld_len = ldn;
}

extern "C" void ip_free_doubles(double* p) {
    if (p) FreeDoubles(p);  // == expansionObject::mempool.release(p)
}
```

## Invariants

1. `lambda3d_lpi_exact` for a non-degenerate line/plane returns 4
   non-empty `Vec<f64>` with finite (non-NaN) entries.
2. The approximate value of `lambda_d` (sum of its entries) is
   non-zero for non-degenerate input.
3. For coplanar / parallel-to-plane input, `sum(lambda_d) ≈ 0`
   (within FP precision).
4. **Cross-tier consistency**: the f64 approximation of `lambda_d`
   from `lambda3d_lpi_exact` lies within `lambda_d.inf..=sup`
   returned by `lambda3d_lpi_interval` for the same input.
5. Stub mode: all 4 Vecs empty.
6. No raw pointers leak across the public API.
7. PR-CR-IP1 + PR-CR-IP2 contracts preserved.

## Error contract

No errors. The function always returns an `LpiExactResult`. Empty
Vecs signal stub-mode or degenerate-allocation behavior (defensive).

## Limitations (banked)

1. No bigfloat tier — PR-CR-IP3b (`bigfloat` opaque handle + Drop).
2. No TPI variants — PR-CR-IP4.
3. No `genericPoint` opaque-handle wrapper — PR-CR-IP5.
4. No `orient3d_indirect_IIII` — PR-CR-IP6.
5. No cherchi-rs Stage 2 integration — PR-CR-IP7.
6. Multi-threaded usage deferred (mempool is thread-local; usage
   pattern must keep alloc/copy/free on the same thread, which the
   safe wrapper enforces by design).

## Test plan (6 tests in `tests/smoke.rs`)

### Group A — types (2 tests, run in both modes)
1. `lpi_exact_result_default_empty` — `LpiExactResult::default()`
   has 4 empty Vecs.
2. `lpi_exact_result_clone_and_eq` — `Clone + PartialEq + Debug`
   behave correctly.

### Group B — algorithm (4 tests)
3. `lambda3d_lpi_exact_non_degenerate_non_empty`
   (`#[cfg(not(ip_unavailable))]`): line P=(1,2,3) → Q=(5,7,9),
   plane z=0. Assert all 4 Vecs non-empty; all entries finite (not
   NaN, not infinite).
4. `lambda3d_lpi_exact_coplanar_d_approximately_zero`
   (`#[cfg(not(ip_unavailable))]`): line (0,0,0) → (1,1,0) lies in
   plane z=0. Assert `sum(lambda_d).abs() < 1e-12`.
5. `lambda3d_lpi_exact_agrees_with_interval`
   (`#[cfg(not(ip_unavailable))]`): non-degenerate input. Run BOTH
   `lambda3d_lpi_exact` and `lambda3d_lpi_interval` on the same
   data. Assert `sum(exact.lambda_d)` lies within
   `interval.lambda_d.inf..=interval.lambda_d.sup`.
6. `lambda3d_lpi_exact_stub_returns_empty_vecs`
   (`#[cfg(ip_unavailable)]`): assert 4 empty Vecs.

## Honest framing

PR-CR-IP3 is a thin Rust shim over the upstream LGPL `lambda3d_LPI_exact`.
The expansion-arithmetic mathematics is entirely in the C++ library;
the Rust side owns only the memory model (Vec<f64> ownership + pool
lifetime management). No algorithmic deviations from Shewchuk
expansion arithmetic.

## References

- `/home/claude/cherchi2022/.../include/indirect_predicates.h:69` — `lambda3d_LPI_exact` declaration.
- `/home/claude/cherchi2022/.../include/indirect_predicates.hpp:7273+` — implementation.
- `/home/claude/cherchi2022/.../include/numerics.h:330-339` — `AllocDoubles` / `FreeDoubles` / `expansionObject::mempool`.
- Shewchuk 1997, "Adaptive Precision Floating-Point Arithmetic and
  Fast Robust Geometric Predicates" — expansion arithmetic theory.
- Cherchi 2022 §6.4 — boolean labeling cascade.
- `specs/indirect_predicates_lpi_interval.md` — PR-CR-IP2 (interval tier; this PR's predecessor).
- `memory/cherchi_rs_pr_cr_ip2.md` — establishing the predicate-shim convention.

# `ExplicitPoint3D` opaque handle — PR-CR-IP5

## Goal

Wrap the C++ `explicitPoint3D` class (a subclass of the polymorphic
`genericPoint`) as a Rust opaque handle with **Drop semantics**.
This is the first FFI mechanism with C++ object lifetime since
PR-CR-IP3's thread-local pool. The pattern established here unlocks
the remaining `genericPoint` subclasses (LPI, TPI, LNC — banked
PR-CR-IP5b) and downstream PR-CR-IP6 (`orient3d_indirect_IIII`,
which takes `const genericPoint&` parameters).

## Architectural finding

**Critical recon result**: `genericPoint` has **NO virtual
destructor** (verified in implicit_point.h:62-275). This rules out
a single polymorphic Rust handle that calls `delete` on a base
pointer — that would be undefined behavior when the actual type is
a subclass.

**Solution**: per-subclass Rust handles, each with its own `Drop`
calling the type-specific destructor shim. PR-CR-IP5 ships
`ExplicitPoint3D` only. PR-CR-IP5b will add `ImplicitPoint3DLpi`
and `ImplicitPoint3DTpi`.

## Public API

```rust
/// Opaque handle to a C++ `explicitPoint3D` (a concrete 3D point
/// with x/y/z coordinates, subclass of `genericPoint`).
///
/// Heap-allocated by the C++ shim; freed via `Drop`. Cannot be
/// cloned — that would risk double-free. Pass by reference if you
/// need to share access.
pub struct ExplicitPoint3D { /* opaque */ }

impl ExplicitPoint3D {
    /// Construct an explicit 3D point. Heap-allocates the C++
    /// object via `ip_explicit_point3d_new`. Panics if the shim
    /// returns null (i.e., out-of-memory).
    pub fn new(x: f64, y: f64, z: f64) -> Self;

    /// Read the x coordinate via the C++ class accessor.
    pub fn x(&self) -> f64;
    pub fn y(&self) -> f64;
    pub fn z(&self) -> f64;
}

impl Drop for ExplicitPoint3D {
    fn drop(&mut self);  // calls ip_explicit_point3d_drop
}

// SAFETY: explicitPoint3D has no thread-local state.
unsafe impl Send for ExplicitPoint3D {}
unsafe impl Sync for ExplicitPoint3D {}
```

A `pub(crate) fn as_generic_ptr(&self) -> *const c_void` is kept
crate-private for now; PR-CR-IP6 will use it (or promote it to
`pub` if downstream needs to construct shim arguments by hand).

## C++ source references

- **`Point_Type` enum** at `/home/claude/cherchi2022/.../include/implicit_point.h:51-59`. Variants: `UNDEF`, `EXPLICIT2D`, `SSI`, `EXPLICIT3D`, `LPI`, `TPI`, `LNC`.
- **`genericPoint` class** at `implicit_point.h:62-275`. Tagged-union base; **no virtual destructor**. Holds a single `Point_Type type` field.
- **`explicitPoint3D` class** at `implicit_point.h:336-355`. Subclass of `genericPoint`; stores `double x, y, z`. Constructor `explicitPoint3D(double, double, double)`. Accessors `X()`, `Y()`, `Z()`.

## FFI shim signatures (`src/wrapper.h`)

```c
/* Construct a new explicit 3D point. Heap-allocated via `new`. */
void* ip_explicit_point3d_new(double x, double y, double z);

/* Destroy via `delete (explicitPoint3D*)p`. Null-safe. */
void ip_explicit_point3d_drop(void* p);

/* Coordinate accessors. */
double ip_explicit_point3d_x(const void* p);
double ip_explicit_point3d_y(const void* p);
double ip_explicit_point3d_z(const void* p);
```

The pointer returned by `ip_explicit_point3d_new` is also a valid
`const genericPoint*` at the C++ level (subclass-to-base implicit
conversion). PR-CR-IP6's predicate shims can accept the same
`void*` and reinterpret as `const genericPoint*` for upstream
predicates.

## Stub backing

When the upstream `Indirect_Predicates` source is unavailable, the
crate falls back to `src/stub.cpp` (already established in
PR-CR-IP1). For ExplicitPoint3D, the stub allocates a 3-double
`double*` buffer via `malloc(3 * sizeof(double))`, stores `x, y, z`
at offsets 0/1/2, and accessors read from those offsets. `Drop`
calls `free`. Rust API behavior is identical in both modes — the
stub is fully round-trip correct (unlike PR-CR-IP1/IP2/IP3 stubs
which used sentinel values).

This means **no `#[cfg(...)]` gating on the tests** — they all run
in both modes.

## Algorithm

```text
Rust:
struct ExplicitPoint3D { ptr: NonNull<c_void> }

impl ExplicitPoint3D:
    new(x, y, z) -> Self:
        raw = unsafe { ip_explicit_point3d_new(x, y, z) }
        ptr = NonNull::new(raw).expect("ip_explicit_point3d_new returned null")
        Self { ptr }

    x(&self) -> f64:
        unsafe { ip_explicit_point3d_x(self.ptr.as_ptr() as *const c_void) }
    // ... same for y, z

    pub(crate) fn as_generic_ptr(&self) -> *const c_void:
        self.ptr.as_ptr() as *const c_void

impl Drop:
    fn drop(&mut self):
        unsafe { ip_explicit_point3d_drop(self.ptr.as_ptr()) }


C++ wrapper.cpp (real):
extern "C" void* ip_explicit_point3d_new(double x, double y, double z) {
    return new explicitPoint3D(x, y, z);
}
extern "C" void ip_explicit_point3d_drop(void* p) {
    delete (explicitPoint3D*)p;
}
extern "C" double ip_explicit_point3d_x(const void* p) {
    return ((const explicitPoint3D*)p)->X();
}
// ... y, z


C++ stub.cpp (fallback):
extern "C" void* ip_explicit_point3d_new(double x, double y, double z) {
    double* buf = (double*)malloc(3 * sizeof(double));
    buf[0] = x; buf[1] = y; buf[2] = z;
    return buf;
}
extern "C" void ip_explicit_point3d_drop(void* p) {
    free(p);
}
extern "C" double ip_explicit_point3d_x(const void* p) {
    return ((const double*)p)[0];
}
// ... y (offset 1), z (offset 2)
```

## Invariants

1. `ExplicitPoint3D::new(x, y, z).x() == x` (same for y, z) in both
   available and stub modes.
2. `Drop` runs cleanly on every constructed instance.
3. `ExplicitPoint3D: Send + Sync` (compile-time check).
4. The internal pointer is never null (defensive `NonNull::new(...).expect`).
5. No `Clone` impl — prevents double-free.
6. Stub mode behaves identically from Rust's perspective.
7. PR-CR-IP1..IP4 contracts preserved.
8. No raw pointers leak from the public API.

## Error contract

`ExplicitPoint3D::new` panics if the shim returns null. This is the
only failure mode and would indicate C++ OOM (in real mode) or
stdlib `malloc` failure (in stub mode). Both are "abort the
process" situations in our test environment.

## Limitations (banked)

1. No `PointType` Rust enum / `point_type()` accessor — PR-CR-IP5c.
2. No `ImplicitPoint3DLpi` / `ImplicitPoint3DTpi` / `ImplicitPoint3DLnc` — PR-CR-IP5b.
3. No equality / comparison ops — PR-CR-IP5d.
4. `as_generic_ptr` is `pub(crate)` — PR-CR-IP6 may promote.
5. C++ `new` throwing `std::bad_alloc` is UB through extern "C". Not handled in PR-CR-IP5; could add try/catch in a future PR if OOM becomes a concern.

## Test plan (5 tests in `tests/smoke.rs`)

All tests run in both available and stub modes (the stub is
round-trip correct, unlike PR-CR-IP1/IP2/IP3 stubs):

1. `explicit_point_3d_send_sync_compile` — compile-time check that
   `ExplicitPoint3D: Send + Sync` via `fn requires_send_sync<T: Send + Sync>()`.
2. `explicit_point_3d_drop_runs` — construct + drop via scope exit;
   wrap in `catch_unwind` to verify no panic.
3. `explicit_point_3d_positive_coords` — `new(1.0, 2.0, 3.0)` →
   `x()==1.0, y()==2.0, z()==3.0`.
4. `explicit_point_3d_origin` — `new(0.0, 0.0, 0.0)` → all zeros.
5. `explicit_point_3d_negative_coords` — `new(-1.5, -2.5, -3.5)` →
   matching reads.

## Honest framing

PR-CR-IP5 is a thin Rust opaque-handle wrapper over the upstream
`explicitPoint3D` class. The geometry math is entirely in the C++
library (or in the stub's noop buffer). The Rust side owns only
memory lifetime (Drop + Send/Sync claims).

## References

- `/home/claude/cherchi2022/.../include/implicit_point.h:51-59` — `Point_Type` enum.
- `/home/claude/cherchi2022/.../include/implicit_point.h:62-275` — `genericPoint` class (no virtual destructor).
- `/home/claude/cherchi2022/.../include/implicit_point.h:336-355` — `explicitPoint3D` class.
- `crates/indirect-predicates-sidecar-rs/src/wrapper.cpp` — existing PR-CR-IP1..IP4 shim file; include-order requirement (`implicit_point.h` before `indirect_predicates.h`) preserved.
- `memory/cherchi_rs_pr_cr_ip1.md` — established conventions (compile_error on wasm32, `cargo:rustc-cfg=ip_unavailable`, stub fallback).
- `memory/cherchi_rs_pr_cr_ip3.md` — established conventions (FFI memory safety patterns; not directly applicable but reference for "Rust-side ownership of foreign-allocated memory").

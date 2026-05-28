# `ImplicitPoint3DLpi<'a>` + `ImplicitPoint3DTpi<'a>` opaque handles — PR-CR-IP5b

## Goal

Wrap the two main implicit point subclasses of `genericPoint` —
`implicitPoint3D_LPI` and `implicitPoint3D_TPI` — as Rust opaque
handles with **lifetime parameters**. PR-CR-IP5 shipped
`ExplicitPoint3D` without a lifetime; IP5b adds the first lifetime-
parameterized FFI handles.

The novel mechanism is that the C++ implicit point subclasses store
**references** to other explicit points (5 for LPI, 9 for TPI).
Rust must enforce that the implicit point doesn't outlive any of
its referenced explicit points — a job for the borrow checker via
`PhantomData<&'a ExplicitPoint3D>`.

**Scope** (per Option A from recon): LPI + TPI in one PR. The
linear-combination subclass `implicitPoint3D_LNC` (2 refs + a
`double t`) is banked to PR-CR-IP5d because its constructor
signature differs enough to warrant separate treatment.

## Public API

```rust
/// Opaque handle to a C++ `implicitPoint3D_LPI` (line-plane
/// intersection point). Borrows 5 `&'a ExplicitPoint3D` references:
/// p, q define the line; r, s, t define the plane.
pub struct ImplicitPoint3DLpi<'a> { /* opaque */ }

unsafe impl<'a> Send for ImplicitPoint3DLpi<'a> {}
unsafe impl<'a> Sync for ImplicitPoint3DLpi<'a> {}

impl<'a> ImplicitPoint3DLpi<'a> {
    /// Construct an implicit LPI point. The implicit point cannot
    /// outlive any of its 5 input references. Panics if the FFI
    /// shim returns null (OOM).
    pub fn new(
        p: &'a ExplicitPoint3D,
        q: &'a ExplicitPoint3D,
        r: &'a ExplicitPoint3D,
        s: &'a ExplicitPoint3D,
        t: &'a ExplicitPoint3D,
    ) -> Self;
}

impl<'a> Drop for ImplicitPoint3DLpi<'a> { /* destructor shim */ }


/// Opaque handle to a C++ `implicitPoint3D_TPI` (triangle-plane
/// intersection point). Borrows 9 `&'a ExplicitPoint3D` references
/// (three triangles × 3 vertices).
pub struct ImplicitPoint3DTpi<'a> { /* opaque */ }

unsafe impl<'a> Send for ImplicitPoint3DTpi<'a> {}
unsafe impl<'a> Sync for ImplicitPoint3DTpi<'a> {}

impl<'a> ImplicitPoint3DTpi<'a> {
    pub fn new(
        v1: &'a ExplicitPoint3D, v2: &'a ExplicitPoint3D, v3: &'a ExplicitPoint3D,
        w1: &'a ExplicitPoint3D, w2: &'a ExplicitPoint3D, w3: &'a ExplicitPoint3D,
        u1: &'a ExplicitPoint3D, u2: &'a ExplicitPoint3D, u3: &'a ExplicitPoint3D,
    ) -> Self;
}

impl<'a> Drop for ImplicitPoint3DTpi<'a> { /* destructor shim */ }
```

No public accessors in v1. Implicit points exist to be passed to
predicates (PR-CR-IP6). The `pub(crate) fn as_generic_ptr()` is
reserved for IP6's predicate shim signatures.

## C++ source references

- **`implicitPoint3D_LPI` constructor** at `implicit_point.h:358-365`:
  ```cpp
  implicitPoint3D_LPI(const explicitPoint3D& _p, const explicitPoint3D& _q,
      const explicitPoint3D& _r, const explicitPoint3D& _s, const explicitPoint3D& _t);
  ```
  Stores 5 const references.

- **`implicitPoint3D_TPI` constructor** at `implicit_point.h:384-392`:
  ```cpp
  implicitPoint3D_TPI(const explicitPoint3D& _v1, const explicitPoint3D& _v2, const explicitPoint3D& _v3,
      const explicitPoint3D& _w1, const explicitPoint3D& _w2, const explicitPoint3D& _w3,
      const explicitPoint3D& _u1, const explicitPoint3D& _u2, const explicitPoint3D& _u3);
  ```
  Stores 9 const references.

- **No explicit destructors** for either subclass. The compiler-
  generated default destructor (a no-op) is correct because:
  - References don't need cleanup.
  - The `mutable interval_number dfilter_lambda_*` cache fields are
    value-type and self-destruct.
  - No heap pointers are stored on the implicit point object itself.

- **Lazy interval cache** (`implicit_point.h:373`, 405): the
  constructor calls `lambda3d_LPI_interval` / `lambda3d_TPI_interval`
  internally and stores the result in mutable cache fields. After
  construction, the cache is read-only — making the type safe for
  `Sync` access from multiple threads.

## FFI shim signatures (`src/wrapper.h`)

```c
void* ip_implicit_point3d_lpi_new(
    const void* p, const void* q,
    const void* r, const void* s, const void* t);
void  ip_implicit_point3d_lpi_drop(void* p);

void* ip_implicit_point3d_tpi_new(
    const void* v1, const void* v2, const void* v3,
    const void* w1, const void* w2, const void* w3,
    const void* u1, const void* u2, const void* u3);
void  ip_implicit_point3d_tpi_drop(void* p);
```

ABI note: C++ `const &` becomes `const T*` at the C ABI level. The
shim takes `const void*` (because the Rust side carries opaque
pointers), reinterprets to `const explicitPoint3D*`, dereferences,
and passes by reference to the C++ constructor. Standard pattern.

The returned `void*` is also a valid `const genericPoint*` at the
C++ level (subclass-to-base implicit conversion). PR-CR-IP6 will
pass the same pointer to `orient3d_indirect_IIII` shims.

## Stub backing

Per IP5's "round-trip correct stub" convention where feasible.
Implicit points have no public accessors in v1, so the stub just
needs `new` + `drop` to round-trip without UB:

```cpp
extern "C" void* ip_implicit_point3d_lpi_new(/*5 ptrs*/) {
    return malloc(1);  // 1-byte sentinel
}
extern "C" void ip_implicit_point3d_lpi_drop(void* p) {
    free(p);
}
// Same shape for TPI.
```

From Rust's perspective the stub and real implementations are
indistinguishable — all 5 v1 tests pass in both modes with no
`#[cfg]` gating.

## Algorithm

```text
Rust:
struct ImplicitPoint3DLpi<'a> {
    ptr: NonNull<c_void>,
    _phantom: PhantomData<&'a ExplicitPoint3D>,
}

impl<'a> ImplicitPoint3DLpi<'a>:
    new(p, q, r, s, t: &'a ExplicitPoint3D) -> Self:
        raw = unsafe { ip_implicit_point3d_lpi_new(
            p.as_generic_ptr(), q.as_generic_ptr(), r.as_generic_ptr(),
            s.as_generic_ptr(), t.as_generic_ptr()) }
        ptr = NonNull::new(raw).expect("...returned null")
        Self { ptr, _phantom: PhantomData }

    pub(crate) fn as_generic_ptr(&self) -> *const c_void:
        self.ptr.as_ptr()

impl<'a> Drop for ImplicitPoint3DLpi<'a>:
    fn drop(&mut self):
        unsafe { ip_implicit_point3d_lpi_drop(self.ptr.as_ptr()) };

unsafe impl<'a> Send for ImplicitPoint3DLpi<'a> {}
unsafe impl<'a> Sync for ImplicitPoint3DLpi<'a> {}

// Same shape for ImplicitPoint3DTpi<'a> with 9 args.


C++ wrapper.cpp (real):
extern "C" void* ip_implicit_point3d_lpi_new(
    const void* p, const void* q, const void* r,
    const void* s, const void* t
) {
    return new implicitPoint3D_LPI(
        *(const explicitPoint3D*)p,
        *(const explicitPoint3D*)q,
        *(const explicitPoint3D*)r,
        *(const explicitPoint3D*)s,
        *(const explicitPoint3D*)t);
}

extern "C" void ip_implicit_point3d_lpi_drop(void* p) {
    delete (implicitPoint3D_LPI*)p;
}

// Same shape for TPI with 9 args.
```

## Invariants

1. `ImplicitPoint3DLpi<'a>` cannot outlive any `&'a ExplicitPoint3D`
   it references — enforced at compile time by the borrow checker.
2. Same for `ImplicitPoint3DTpi<'a>`.
3. `Drop` runs cleanly (no panic) for every constructed instance.
4. `Send + Sync` for both types (compile-time check).
5. Internal pointer never null (defensive `.expect`).
6. No raw pointers leak across the public API.
7. PR-CR-IP1..IP5 contracts preserved.
8. Stub mode behaves identically from Rust's perspective.

## Error contract

Constructors panic if the FFI shim returns null (OOM). No other
failure modes in v1.

## Limitations (banked)

1. **No public accessors**. Implicit points exist to be passed to
   predicates; PR-CR-IP6 wires that up.
2. **`PointType` enum + accessor**: PR-CR-IP5c.
3. **`ImplicitPoint3DLnc<'a>`**: PR-CR-IP5d.
4. **`Bigfloat` opaque handle**: PR-CR-IP3b (reuses the
   non-lifetime variant of this pattern).

## Test plan (5 tests + 1 doc-test)

### Group A — compile-time checks (3 tests + 1 doc-test, both modes)
1. `implicit_point_3d_lpi_send_sync_compile` — `requires_send_sync::<ImplicitPoint3DLpi<'_>>()`.
2. `implicit_point_3d_tpi_send_sync_compile` — same for TPI.
3. **Doc test `compile_fail`** on `ImplicitPoint3DLpi`: shows the
   borrow checker rejecting a dangling reference (implicit point
   outliving its explicit points).

### Group B — runtime construct + drop (2 tests, both modes)
4. `implicit_point_3d_lpi_construct_and_drop` — 5 ExplicitPoint3Ds
   → 1 ImplicitPoint3DLpi → drop via scope exit; `catch_unwind`
   verifies no panic.
5. `implicit_point_3d_tpi_construct_and_drop` — 9 ExplicitPoint3Ds
   → 1 ImplicitPoint3DTpi → drop. No panic.

## Honest framing

PR-CR-IP5b adds opaque-handle wrappers for the two main implicit
point subclasses. No new C++ algorithm is exposed — the implicit
points are just inputs for predicates (which arrive in PR-CR-IP6).
The novel aspect is the **lifetime parameter** plumbing on the
Rust side. The borrow checker now enforces a compile-time invariant
that C++ silently requires (and where C++ would silently UB).

## References

- `/home/claude/cherchi2022/.../include/implicit_point.h:358-365` — LPI constructor.
- `/home/claude/cherchi2022/.../include/implicit_point.h:384-392` — TPI constructor.
- `/home/claude/cherchi2022/.../include/implicit_point.h:62-275` — `genericPoint` base (no virtual destructor — IP5's critical finding still applies).
- `crates/indirect-predicates-sidecar-rs/src/lib.rs` — `ExplicitPoint3D` + `pub(crate) as_generic_ptr` (PR-CR-IP5).
- `memory/cherchi_rs_pr_cr_ip5.md` — canonical opaque-handle pattern.
- Cherchi 2022 §6.4 — boolean labeling uses implicit points as inputs to `orient3d_indirect_IIII` (PR-CR-IP6 will wire that).

# `orient3d` + `less_than_on_{x,y,z}` predicates — PR-CR-IP6

## Goal

Wrap the Cherchi 2022 §6.4 boolean labeling trigger set:
- `orient3d_indirect_IIII` — 4-point orientation predicate over generic points
- `lessThanOnX_II / lessThanOnY_II / lessThanOnZ_II` — pairwise per-axis comparators

These are the predicates Stage 2 invokes to label arrangement
cells. After PR-CR-IP6, **PR-CR-IP7 (cherchi-rs Stage 2 integration)
is unblocked** — yang-rs's PR-YR3 + PR-YR4 ad-hoc substitutes can
finally be deleted in IP7.

## Strategic recon finding

`orient3d_indirect_IIII` handles **all combinations** of explicit /
implicit points via genericPoint polymorphism. Explicit points
have lambda coefficients `(1, 0, 0, 0)` and denominator `1`;
`getIntervalLambda()` returns those constants; the IIII logic
computes correctly. The four C++ variants
(IEEE / IIEE / IIIE / IIII) exist only for C++ optimization, not
correctness.

**API simplification**: Rust exposes a **single** `orient3d`
function using IIII unconditionally. The C++ pays a small
per-call type-tag dispatch cost; in exchange Rust gets a uniform
API. Variant-specific wrappers can be added in PR-CR-IP6b if
profiling demands it.

## Public API

```rust
/// Sign result from upstream predicates. Mirrors `IP_Sign`
/// (implicit_point.h:51-59).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Sign {
    Negative = -1,
    Zero = 0,
    Positive = 1,
    /// NaN input or catastrophic cancellation.
    Undefined = 2,
}

impl Sign {
    /// Map a raw C++ predicate return to this enum.
    pub const fn from_int(i: i32) -> Self;
}

/// Sealed trait: any of our crate's point handle types can be
/// passed to the predicate functions. External crates cannot
/// implement this trait.
pub trait AsGenericPoint: sealed::Sealed {
    #[doc(hidden)]
    fn as_generic_ptr(&self) -> *const core::ffi::c_void;
}

impl AsGenericPoint for ExplicitPoint3D { ... }
impl<'a> AsGenericPoint for ImplicitPoint3DLpi<'a> { ... }
impl<'a> AsGenericPoint for ImplicitPoint3DTpi<'a> { ... }

/// 4-point orientation. `Positive` = `p4` is above plane `p1-p2-p3`
/// in CCW orientation; `Zero` = coplanar; `Negative` = below.
pub fn orient3d(
    p1: &impl AsGenericPoint,
    p2: &impl AsGenericPoint,
    p3: &impl AsGenericPoint,
    p4: &impl AsGenericPoint,
) -> Sign;

/// `Positive` iff `p1.x < p2.x`; `Zero` if equal; `Negative` if
/// `p1.x > p2.x`. (Per upstream's `IP_Sign::POSITIVE` convention.)
pub fn less_than_on_x(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign;
pub fn less_than_on_y(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign;
pub fn less_than_on_z(p1: &impl AsGenericPoint, p2: &impl AsGenericPoint) -> Sign;
```

In stub mode (`cfg!(ip_unavailable)`), all predicates return
`Sign::Undefined`.

## C++ source references

- **`IP_Sign` enum** at `implicit_point.h:51-59`. Values: `UNDEF = 0`, `EXPLICIT2D = 1`, ..., used for `Point_Type`. Sign return values share the same int domain but with different semantic mapping: `-1`/`0`/`+1`/`2(undef)`.
- **`orient3d_indirect_*` declarations** at `indirect_predicates.h:92-95`:
  ```cpp
  int orient3d_indirect_IEEE(const genericPoint&, ...);   // line 92
  int orient3d_indirect_IIEE(const genericPoint&, ...);   // line 93
  int orient3d_indirect_IIIE(const genericPoint&, ...);   // line 94
  int orient3d_indirect_IIII(const genericPoint&, const genericPoint&,
                              const genericPoint&, const genericPoint&);   // line 95
  ```
- **Implementation** at `indirect_predicates.hpp:10712+`. Internal 3-tier cascade (filtered → interval → exact).
- **Comparators** at `indirect_predicates.h:75, 77, 79`:
  ```cpp
  int lessThanOnX_II(const genericPoint& p1, const genericPoint& p2);   // line 75
  int lessThanOnY_II(const genericPoint& p1, const genericPoint& p2);   // line 77
  int lessThanOnZ_II(const genericPoint& p1, const genericPoint& p2);   // line 79
  ```
- **No `_EE` (explicit-vs-explicit) comparators** exist upstream. Banked PR-CR-IP6c to implement in Rust if needed.

## FFI shim signatures (`src/wrapper.h`)

```c
int ip_orient3d_indirect_iiii(
    const void* p1, const void* p2, const void* p3, const void* p4);
int ip_less_than_on_x_ii(const void* p1, const void* p2);
int ip_less_than_on_y_ii(const void* p1, const void* p2);
int ip_less_than_on_z_ii(const void* p1, const void* p2);
```

The shims cast each `const void*` to `const genericPoint*` (valid
because the void pointers come from our handle types whose
underlying objects are genericPoint subclasses with single
inheritance — base address == subclass address) and dereference
to bind to the C++ reference parameter.

## Algorithm

```text
// Rust:
pub fn orient3d(p1, p2, p3, p4: &impl AsGenericPoint) -> Sign:
    let r = unsafe { ip_orient3d_indirect_iiii(
        p1.as_generic_ptr(), p2.as_generic_ptr(),
        p3.as_generic_ptr(), p4.as_generic_ptr()) };
    Sign::from_int(r)

pub fn less_than_on_x(p1, p2: &impl AsGenericPoint) -> Sign:
    let r = unsafe { ip_less_than_on_x_ii(
        p1.as_generic_ptr(), p2.as_generic_ptr()) };
    Sign::from_int(r)
// Same for y, z


// wrapper.cpp:
extern "C" int ip_orient3d_indirect_iiii(
    const void* p1, const void* p2, const void* p3, const void* p4
) {
    return orient3d_indirect_IIII(
        *(const genericPoint*)p1,
        *(const genericPoint*)p2,
        *(const genericPoint*)p3,
        *(const genericPoint*)p4);
}
extern "C" int ip_less_than_on_x_ii(const void* p1, const void* p2) {
    return lessThanOnX_II(*(const genericPoint*)p1, *(const genericPoint*)p2);
}
// Same for y, z.


// stub.cpp:
extern "C" int ip_orient3d_indirect_iiii(...) { return 2; }  // UNDEFINED
extern "C" int ip_less_than_on_x_ii(...) { return 2; }
// Same for y, z.
```

## Sealed trait pattern

```rust
pub trait AsGenericPoint: sealed::Sealed {
    #[doc(hidden)]
    fn as_generic_ptr(&self) -> *const core::ffi::c_void;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ExplicitPoint3D {}
    impl<'a> Sealed for super::ImplicitPoint3DLpi<'a> {}
    impl<'a> Sealed for super::ImplicitPoint3DTpi<'a> {}
}
```

The `pub` trait can appear in public function bounds; the private
supertrait `Sealed` prevents external crates from implementing
`AsGenericPoint` for their own types. The method itself is
`#[doc(hidden)]` to discourage direct misuse — it returns a raw
pointer that's only meaningful inside this crate.

The existing `pub(crate) fn as_generic_ptr` on `ExplicitPoint3D`,
`ImplicitPoint3DLpi`, `ImplicitPoint3DTpi` get **bumped to `pub fn`**
via the trait method declaration. Effective visibility remains
crate-private (no external types can satisfy the bound).

## Invariants

1. `Sign::from_int(-1) == Negative`, `from_int(0) == Zero`,
   `from_int(1) == Positive`, `from_int(2) == Undefined`,
   `from_int(other) == Undefined` (defensive).
2. `orient3d` over positive tetrahedron
   `((0,0,0),(1,0,0),(0,1,0),(0,0,1))` returns `Sign::Positive` in
   available mode.
3. `orient3d` over coplanar input returns `Sign::Zero` in available mode.
4. `orient3d` over orientation-flipped input returns
   `Sign::Negative` in available mode.
5. `less_than_on_x(p1, p2)` where `p1.x < p2.x` returns
   `Sign::Positive` in available mode.
6. `less_than_on_y(p1, p2)` where `p1.y == p2.y` returns
   `Sign::Zero` in available mode.
7. `AsGenericPoint` is sealed — external crates cannot implement it.
8. PR-CR-IP1..IP5b contracts preserved.
9. Stub mode: all 4 predicates return `Sign::Undefined`.

## Error contract

No errors. All 4 predicates return `Sign`. The `Undefined` variant
captures the upstream NaN / catastrophic-cancellation case (and
also stub mode).

## Limitations (banked)

1. **No variant-specific orient3d** (IEEE/IIEE/IIIE) — banked PR-CR-IP6b.
2. **No `lessThanOn{X,Y,Z}_EE`** (explicit-vs-explicit) — none upstream; if needed, implement in Rust as straight double comparison.
3. **No `lessThanOn{X,Y,Z}_IE`** variants exposed in v1 — IIII handles via type-tag dispatch.
4. **`PointType` enum + `point_type()` accessor**: PR-CR-IP5c.
5. **`ImplicitPoint3DLnc`**: PR-CR-IP5d.

## Test plan (10 tests in `tests/smoke.rs`)

### Group A — Sign + AsGenericPoint (3 tests, both modes)
1. `sign_from_int_round_trip` — all 4 valid mappings + defensive default.
2. `sign_derives` — Copy + PartialEq + Debug.
3. `as_generic_point_trait_impls_compile` — compile-time check
   that all 3 handle types implement `AsGenericPoint`.

### Group B — orient3d (4 tests, cfg-gated)
4. `orient3d_positive_explicit_tetrahedron` (cfg !ip_unavailable).
5. `orient3d_coplanar_explicit_zero` (cfg !ip_unavailable).
6. `orient3d_negative_explicit_swapped` (cfg !ip_unavailable).
7. `orient3d_stub_returns_undefined` (cfg ip_unavailable).

### Group C — comparators (3 tests, cfg-gated)
8. `less_than_on_x_explicit_ordered` (cfg !ip_unavailable).
9. `less_than_on_y_explicit_equal` (cfg !ip_unavailable).
10. `less_than_on_z_stub_returns_undefined` (cfg ip_unavailable).

## Honest framing

PR-CR-IP6 wraps the predicate functions; the C++ library does all
the geometric math. The Rust side adds a thin Sign-typed return
and trait-dispatched argument passing. No algorithmic deviation
from upstream Cherchi 2022 §6.4.

## References

- `/home/claude/cherchi2022/.../include/indirect_predicates.h:75-79` — comparator declarations.
- `/home/claude/cherchi2022/.../include/indirect_predicates.h:92-95` — `orient3d_indirect_*` variants.
- `/home/claude/cherchi2022/.../include/indirect_predicates.hpp:7996, 8136, 8276, 10712` — implementations.
- `/home/claude/cherchi2022/.../include/implicit_point.h:51-59` — `IP_Sign` enum reference.
- `memory/cherchi_rs_pr_cr_ip5.md` + `memory/cherchi_rs_pr_cr_ip5b.md` — handle conventions.
- Cherchi 2022 §6.4 — boolean labeling algorithm.

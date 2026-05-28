# `indirect-predicates-sidecar-rs` — Scaffold spec (PR-CR-IP1)

## Goal

Establish an FFI sidecar crate that links Marco Attene's
**Indirect_Predicates** C++ library (LGPL-2.1, header-only). The
sidecar will provide the exact geometric predicates that
`cherchi-rs` needs to implement Cherchi 2022's Stage 2 mesh
arrangement and §6.4 boolean labeling.

PR-CR-IP1 is the **scaffold PR only** — analogous to PR-CSR1, which
established the `cherchi-sidecar-rs` subprocess sidecar before any
real boolean wrapper was implemented. PR-CR-IP1 establishes:

1. The build chain (cc + bindgen against a header-only C++ library)
2. The `wrapper.h` + `wrapper.cpp` FFI boundary pattern
3. Graceful skip behavior when the C++ source isn't present
4. A single link-probe predicate call (`dotProductSign2D`) that proves
   the entire chain works end-to-end

All 47 other public functions in `indirect_predicates.h` are
**banked** to follow-up PRs PR-CR-IP2..IP10. This PR ships
infrastructure, not algorithmic capability.

## Architectural context

- PR-CR10 paused at the LGPL-2.1 license question — `orient3D_LPI`
  and the lambda constructors are LGPL, blocking a direct Rust port.
- The user has now decided to accept LGPL via FFI sidecar (dynamic
  linking is LGPL-compatible).
- The sidecar is **NOT WASM-compatible** by design. WASM is
  intentionally broken during the Yang validation phase.
- A clean-room Rust reimplementation will eventually replace this
  sidecar; the sidecar serves as both build-time validation tool and
  algorithmic reference oracle.
- This crate sits at the **bottom of the dependency stack**, below
  `cad-primitives`. It has zero workspace runtime deps. Only
  `[build-dependencies] cc` and `bindgen`.

## Public API

```rust
//! indirect-predicates-sidecar-rs — FFI sidecar for Marco Attene's
//! Indirect_Predicates (LGPL-2.1). NOT WASM-compatible.

/// `true` if the Indirect_Predicates source was found at build time
/// and the FFI shim was successfully compiled and linked.
/// `false` if the build fell back to the no-op stub.
pub const AVAILABLE: bool;

/// Probes the FFI link. Returns `+1` when the library is available
/// (calls real `dotProductSign2D` on a well-conditioned input).
/// Returns `-2` when the build fell back to the stub (source not
/// found at build time).
pub fn link_probe() -> i32;
```

That is the entire public surface for PR-CR-IP1. PR-CR-IP2+ will
add real predicate wrappers.

## Build strategy

### Library is header-only

Indirect_Predicates contains only `.h` + `.hpp` files in `include/`
(no `src/`, no library `.cpp`). All function bodies are inline in
the `.hpp` files. Therefore `cc::Build` cannot just `.file(...)`
library sources — instead, our crate compiles its own
`src/wrapper.cpp` that `#include`s the library headers, triggering
instantiation of the inline functions we actually call.

### `wrapper.h` (pure C) + `wrapper.cpp` (C++ impl)

Bindgen handles a subset of C++ poorly: `__m128d` intrinsics,
template specializations, the `genericPoint` class, etc.  To
sidestep all of this, the FFI boundary is a pure-C header
(`src/wrapper.h`) declaring `extern "C"` functions. Bindgen sees
only the C header (`-x c`); the C++ horror stays inside
`src/wrapper.cpp`.

### Build flow

```
build.rs:
    src_dir = env("INDIRECT_PREDICATES_SRC")
            ?? "/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/
                 arrangements/external/Indirect_Predicates"

    if src_dir / "include" / "indirect_predicates.h" exists:
        cc::Build::new()
            .cpp(true)
            .std("c++20")
            .include(src_dir / "include")
            .file("src/wrapper.cpp")
            .flag_if_supported("-O2")
            .flag_if_supported("-frounding-math")
            .flag_if_supported("-Wno-strict-aliasing")
            .flag_if_supported("-Wno-cast-qual")
            .compile("indirect_predicates_shim")
        // DO NOT define USE_GNU_GMP_CLASSES (no libgmp dep)
        // DO NOT define USE_SIMD_INSTRUCTIONS (no AVX2 in v1)
    else:
        cc::Build::new()
            .file("src/stub.cpp")
            .compile("indirect_predicates_shim")
        emit cargo:rustc-cfg=ip_unavailable
        emit cargo:warning="..."

    bindgen on src/wrapper.h with -x c
        --allowlist-function "ip_.*"
        → $OUT_DIR/bindings.rs

    cargo:rerun-if-env-changed=INDIRECT_PREDICATES_SRC
    cargo:rerun-if-changed=src/wrapper.{h,cpp,stub.cpp}
```

### Link-probe target: `dotProductSign2D`

Declared at `indirect_predicates.h:35`:

```cpp
int dotProductSign2D(double px, double py, double rx, double ry, double qx, double qy);
```

Properties:
- Free function (no class, no template) — already C-callable signature.
- 6 doubles → int. No FFI marshalling complexity.
- Implementation cascades `_filtered` → `_interval` → `_exact`. With
  well-conditioned input `(0,0)(1,0)(0,1)` the filtered fast path
  short-circuits — no FPU mode initialization, no SIMD, no bigfloat,
  no exact arithmetic needed.
- Real library symbol; proves the LGPL library object code was
  actually linked, not just our wrapper.

The shim:

```c
// src/wrapper.h
#ifdef __cplusplus
extern "C" {
#endif
int ip_link_probe(void);
#ifdef __cplusplus
}
#endif
```

```cpp
// src/wrapper.cpp
#include "indirect_predicates.h"
extern "C" int ip_link_probe(void) {
    return dotProductSign2D(0.0, 0.0, 1.0, 0.0, 0.0, 1.0);
}
```

```cpp
// src/stub.cpp — used when INDIRECT_PREDICATES_SRC is unset and the
// default path is missing.
extern "C" int ip_link_probe(void) { return -2; }
```

## License posture

- This crate's source code: **MIT** (workspace default).
- The library it links: **LGPL-2.1** by Marco Attene (IMATI-GE/CNR).
- LGPL-2.1 allows dynamic linking with non-LGPL code; the consumer
  inherits LGPL distribution obligations only if they statically
  embed the library into a closed-source binary.
- Documented in `LICENSE-THIRD-PARTY.md` at crate root.

## Invariants

1. Build never fails. Missing source → `cargo:warning` + `ip_unavailable`
   cfg + stub.cpp returns sentinel `-2`.
2. **Available state**: `cfg!(ip_unavailable) == false`,
   `AVAILABLE == true`, `link_probe() == 1`.
3. **Unavailable state**: `cfg!(ip_unavailable) == true`,
   `AVAILABLE == false`, `link_probe() == -2`.
4. WASM build is intentionally broken: `compile_error!` at the top
   of `src/lib.rs` if `target_arch == "wasm32"`.
5. No `unsafe` outside `mod ffi` or one-line `pub fn` wrappers.
6. No new workspace runtime deps. Bindgen output lives in
   `$OUT_DIR`, never checked in.

## Error contract

No errors. The crate has no runtime failure modes:
- Missing source is detected at build time → graceful stub.
- `link_probe()` always returns an `i32`.
- No `Result<>` types in the public API yet (will arrive in
  PR-CR-IP2+ when actual predicates wrap implicit-point lifetimes).

## Limitations (v1)

1. **Only `dotProductSign2D` exposed** via `link_probe()`. All other
   47 functions (LPI lambdas, TPI lambdas, `orient3d_indirect_IIII`,
   `lessThanOnX/Y/Z_*`, `genericPoint` class wrapper, etc.) are
   banked.
2. **No FPU mode initialization**. The `_interval` cascade path
   needs `fesetround(FE_UPWARD)`. PR-CR-IP1 sidesteps by using a
   well-conditioned input that short-circuits on `_filtered`.
   Per-thread `std::sync::Once + thread-local` policy lands in
   PR-CR-IP2 when the interval path is first exercised.
3. **No SIMD**. `USE_SIMD_INSTRUCTIONS` is left undefined; library
   uses pure-double arithmetic. PR-CR-IP8 banked.
4. **No GMP**. `USE_GNU_GMP_CLASSES` left undefined; the library
   uses Attene's own `bigfloat` implementation. PR-CR-IP9 banked
   for opt-in bit-compat with upstream Cherchi binaries.
5. **WASM build is broken**. `compile_error!` fires on `wasm32`.
   PR-CR-IP10 banked for cfg-gating the consumers (cherchi-rs,
   yang-rs, kernel-v2) so WASM builds skip the FFI-linked code.

## Test plan (6 tests in `tests/smoke.rs`)

1. `link_probe_returns_one_when_available` — `#[cfg(not(ip_unavailable))]`
2. `link_probe_returns_sentinel_when_unavailable` — `#[cfg(ip_unavailable)]`
3. `available_flag_matches_cfg`
4. `link_probe_is_deterministic` — 1000× equality (catches uninit/FPU bugs)
5. `link_probe_does_not_panic` — `catch_unwind`
6. `description_documents_wasm_incompatibility` — guards the doc-comment string

Test 1 and 2 are mutually exclusive via cfg. Tests 3-6 run in either
state with appropriate logic.

## Banked PRs

| PR | Scope |
|---|---|
| PR-CR-IP2 | `interval_number` + `lambda3d_LPI_interval` + FPU init policy |
| PR-CR-IP3 | `bigfloat` + `lambda3d_LPI_exact` / `_bigfloat` (ownership story for `double**` out-params) |
| PR-CR-IP4 | `lambda3d_TPI_*` family |
| PR-CR-IP5 | `genericPoint` opaque-handle wrapper (Box + Drop via shim) |
| PR-CR-IP6 | `orient3d_indirect_IIII` + `lessThanOnX/Y/Z_II` (Cherchi 2022 §6.4 trigger set) |
| PR-CR-IP7 | **cherchi-rs Stage 2 integration** (resumes PR-CR14+ chain) |
| PR-CR-IP8 | SIMD opt-in (`USE_SIMD_INSTRUCTIONS`, AVX2 / SSE2) |
| PR-CR-IP9 | Optional `gmp` cargo feature for bit-compat with upstream binaries |
| PR-CR-IP10 | WASM cfg-gating restoration |
| (Eventual) | Clean-room Rust reimplementation; sidecar becomes reference oracle |

## References

- `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/external/Indirect_Predicates/include/indirect_predicates.h` — public C++ header
- `/home/claude/cherchi2022/.../include/indirect_predicates.hpp` — inline implementations (where `_filtered`/`_interval`/`_exact` cascade lives)
- `/home/claude/cherchi2022/.../include/numerics.h` — `bigfloat` + `interval_number` (no GMP needed)
- `crates/slvs-patch/slvs-0.6.0/build.rs` — cc + bindgen precedent in workspace
- `crates/cherchi-sidecar-rs/` — non-WASM subprocess sidecar precedent
- `memory/cherchi_rs_spike_pr_cr10.md` — the LGPL pivot being unwound
- `memory/cherchi_sidecar_rs_pr_csr1.md` — scaffold-PR convention precedent
- `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt` §6.4 — boolean labeling algorithmic context

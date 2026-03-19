# R7: nalgebra on wasm32-unknown-unknown — Practical Cookbook

**Feeds into**: Wave 5 (WASM verification)
**Priority**: Medium

## What We Know

nalgebra is pure Rust and claims wasm32-unknown-unknown support.
We need QR decomposition (ColPivQR) and basic matrix operations.
The solver will run in a Web Worker via wasm-bindgen.

## What We Need

Practical confirmation that our specific nalgebra usage compiles and
performs correctly on WASM, plus any gotchas.

## Specific Questions

### Q1: Feature flags
- Which nalgebra features do we need? Just the default?
- Does `nalgebra` pull in anything platform-specific via default features?
- Do we need `no-std` support? (Probably not — we're in a worker with
  full std access via wasm-bindgen.)
- Does `nalgebra` have a `wasm` feature or any WASM-specific configuration?

### Q2: Performance on WASM
- nalgebra uses SIMD on native targets. Does this work on WASM?
  (wasm-simd is a thing but requires opt-in.)
- For our use case (matrices up to ~200×200), is WASM performance
  adequate? Any benchmarks?
- Does nalgebra use BLAS/LAPACK backends? These would break WASM.
  Confirm that the pure-Rust path is used.

### Q3: ColPivQR specifically
- `nalgebra::linalg::ColPivQR` — does this work on WASM?
- Any known issues with numerical accuracy on WASM? (wasm32 uses
  f64 natively, so there shouldn't be precision loss.)
- Memory allocation patterns — does ColPivQR allocate? Is this a
  problem for WASM's linear memory?

### Q4: Build configuration
- Our WASM build uses: `cargo +nightly build --target wasm32-unknown-unknown -Zbuild-std`
- Does nalgebra compile under `-Zbuild-std`?
- Any issues with the `panic=unwind` configuration we use?

### Q5: Alternatives if nalgebra has issues
- `faer` crate — newer, faster, also pure Rust. WASM support?
- `ndarray` — WASM support?
- Hand-rolled QR for small matrices?
- What's the simplest dependency that gives us dense QR with
  column pivoting on WASM?

## Desired Output

1. Confirmed: nalgebra X.Y with features [...] works on wasm32-unknown-unknown
2. Cargo.toml snippet with correct features
3. Any WASM-specific gotchas or workarounds
4. Performance expectations (rough: "200×200 QR in <1ms" or similar)
5. Fallback recommendation if nalgebra has issues

## References

- nalgebra GitHub issues tagged "wasm"
- nalgebra documentation on WASM
- faer crate documentation
- wasm-bindgen guide on numeric computation

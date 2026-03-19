# Wave 5: WASM Verification

**Executor**: Opus (sequential, final integration)
**Depends on**: Wave 4 Fork F merged (feature gate removed)
**Estimated scope**: build + test commands, minimal code changes

## Goal

Verify the pure-Rust solver compiles to wasm32-unknown-unknown and works
in the browser. This is the final gate before declaring success.

## Steps

### 5.1 WASM build

Per CLAUDE.md WASM rebuild workflow:

```bash
# Build with nightly + build-std (required for panic=unwind)
cargo +nightly build -p wasm-bridge \
  --target wasm32-unknown-unknown \
  --release \
  --no-default-features \
  -Zbuild-std

# Generate JS bindings
wasm-bindgen \
  target/wasm32-unknown-unknown/release/wasm_bridge.wasm \
  --out-dir crates/wasm-bridge/pkg \
  --target web \
  --no-typescript

# Copy to app
cp crates/wasm-bridge/pkg/wasm_bridge{_bg.wasm,.js} app/static/pkg/
```

Key change: `--no-default-features` is no longer needed to skip `native-solver`
because the feature gate is gone. But we still use it because the WASM build
may have other feature considerations.

Actually — with the feature gate removed, `sketch-solver` is now a required
dependency of `wasm-bridge`. The WASM build MUST compile sketch-solver to
wasm32-unknown-unknown. This is the whole point — it should work because
we replaced the C FFI `slvs` with pure Rust `nalgebra`.

### 5.2 Verify nalgebra WASM compatibility

`nalgebra` is pure Rust and supports wasm32-unknown-unknown. But verify:
- No `std` features that pull in incompatible system calls
- No `rayon` feature accidentally enabled (parallel features use threads)

Check: `cargo tree -p sketch-solver --target wasm32-unknown-unknown` should
show no platform-incompatible dependencies.

### 5.3 Dev server verification

```bash
cd app && npm run dev
```

- Open browser to localhost:8083
- Enter sketch mode
- Draw a rectangle
- Add constraints
- Verify constraints solve (green status)
- Verify profile extraction (extrude button becomes available)

### 5.4 GUI test suite

```bash
./scripts/test.sh gui-fast
```

All sketch-related GUI tests must pass.

### 5.5 Final cleanup

- Update `specs/sketch_solver_rewrite.md` status from "Design spec" to "Implemented"
- Check success criteria checkboxes
- Update CLAUDE.md if any workflow instructions changed

## Deliverables

- Updated WASM bundle in `app/static/pkg/`
- Verified GUI tests pass
- Updated spec status

## Verification (Success Criteria from Spec)

- [ ] All 59 existing sketch-solver tests pass
- [ ] All GUI sketch tests pass (Playwright)
- [ ] No feature gate — single build for native and WASM
- [ ] slvs-solver.js and slvs.wasm deleted
- [ ] DOF reporting matches or exceeds libslvs accuracy
- [ ] Solve time ≤ 2x libslvs for sketches under 100 entities
- [ ] Conflicting constraint identification at constraint granularity
- [ ] No C/C++ dependencies in sketch-solver crate
- [ ] Tolerances from units.rs (A14 compliance)

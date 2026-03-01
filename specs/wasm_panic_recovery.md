# Spec: WASM Panic Recovery

**Status**: In Progress
**FIP**: Section 3 — Specification

## Goal

The WASM engine must never permanently crash on boolean operation failure.
Panics deep in truck internals (e.g., "knot vector consists single value")
must produce recoverable error responses, not kill the WASM module with an
`unreachable` trap.

## Parameters

None (infrastructure fix).

## Branch Table

| Condition | Outcome |
|-----------|---------|
| No panic occurs | Normal `ModelUpdated` or `Error` response from engine |
| Panic occurs, `catch_unwind` catches it | `Error` response with panic reason; engine state valid |
| Panic bypasses `catch_unwind` (abort/trap) | JS worker detects `WebAssembly.RuntimeError`; flags `needsRestart`; bridge auto-reinitializes worker; user sees recoverable error |
| Subsequent operation after caught panic | Works normally (engine state preserved by `catch_unwind`) |
| Subsequent operation after module restart | Works after re-init (document state replayed) |

## Architecture — Two-Layer Defense

### Layer 1: Rust `catch_unwind` (existing)

- `crates/wasm-bridge/src/wasm_api.rs:52` wraps `process_message` in
  `std::panic::catch_unwind(AssertUnwindSafe(..))`.
- `crates/kernel-fork/src/healing.rs:1320` wraps individual boolean attempts
  in `catch_unwind` for the perturbation cascade.
- Requires `panic = "unwind"` in `[profile.release]` (workspace `Cargo.toml`).
- On Rust 1.82+ with `wasm32-unknown-unknown`, this is natively supported.

### Layer 2: JS worker crash recovery (new)

- `worker.js:processMessage` detects `WebAssembly.RuntimeError` or
  `"unreachable"` in error messages.
- Sets `wasmModule = null` and returns `{ needsRestart: true }`.
- `bridge.js:_handleMessage` detects `needsRestart` flag and triggers
  automatic worker re-initialization.

## Invariants

1. After any failed operation (panic caught), subsequent operations still work
   without page reload.
2. Error messages include the panic reason text (not just "unreachable executed").
3. The UI never shows a permanently broken state — either the engine recovers
   in-place or the bridge auto-restarts it.

## Oracles

1. **GUI test**: Extrude-on-extrude (coplanar face scenario) produces either
   `ModelUpdated` (boolean succeeded) or a recoverable `Error` (not a crash).
2. **Liveness test**: After any engine error, a subsequent sketch+extrude
   operation succeeds.

## Failure Modes

- If `catch_unwind` catches the panic, engine state MAY be partially
  corrupted (the `thread_local!` `ENGINE_STATE` may have inconsistent data).
  The engine should still accept new commands but results may be incorrect.
- If the WASM module traps (Layer 1 fails), the JS layer restarts the
  module from scratch. Document state must be replayed to restore the model.

## Test Coverage

- `app/tests/gui/wasm-panic-recovery.spec.js`:
  - Extrude-on-extrude coplanar face scenario
  - Engine liveness check after error
  - Sequential operations after recovery

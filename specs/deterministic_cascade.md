# Deterministic Perturbation Cascade

**Status:** IMPLEMENTING
**Sprint:** 37

## Goal

Replace the 120-second wall-clock timeout in `try_boolean_with_perturbation()` with a
fixed attempt-count limit (`MAX_CASCADE_ATTEMPTS = 25`), making the perturbation cascade
deterministic across native and WASM platforms.

## Motivation

The wall-clock timeout (`std::time::Duration::from_secs(120)`) causes non-deterministic
behavior:
- **WASM cannot use `Instant::now()`** — the timeout is `#[cfg(not(target_arch = "wasm32"))]`
  gated, meaning WASM has no cascade limit at all.
- **Native timing varies by CPU/load** — the same geometry may succeed on a fast machine
  but timeout on a slow one (or under CPU contention from parallel tests).
- **Test flakiness** — K8's 6-operation cascade is sensitive to parallel test execution
  because each attempt takes ~12s on a 31-face shell.

## Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `MAX_CASCADE_ATTEMPTS` | 50 | Must cover all strategy branches (~46 for 2-coplanar-dir with 4 epsilons). 50 provides headroom. |

## Branch Table: Old vs New

| Scenario | Old (wall-clock) | New (attempt-count) |
|----------|-------------------|---------------------|
| Fast machine, simple op | Succeeds in 1-3 attempts | Identical |
| Slow machine, simple op | Succeeds in 1-3 attempts | Identical |
| K8 complex op (native) | 3-5 attempts, ~15-60s | 3-5 attempts, same result |
| K8 complex op (WASM) | No limit (could run forever) | Limited to 25 attempts |
| Impossible geometry | Times out after ~120s (~10 attempts) | Fails after 25 attempts |
| Parallel test contention | May timeout early due to CPU sharing | Unaffected by CPU load |

## Implementation

**File:** `crates/kernel-fork/src/healing.rs`

1. Add `const MAX_CASCADE_ATTEMPTS: u32 = 25;`
2. Replace `check_timeout!()` macro with `check_cascade_limit!()` that checks
   `_attempt_count >= MAX_CASCADE_ATTEMPTS`
3. Remove `cascade_timeout` variable and wall-clock timeout logic
4. Gate `_perturb_start` under `#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]`
   for diagnostic timing only (not control flow)
5. Update EXHAUSTED message to print on all platforms (remove wasm32 gate)

## Invariants

- **Determinism:** Same geometry + same tolerance → same number of attempts on all platforms
- **Platform parity:** WASM and native use identical cascade logic
- **No regression:** K8 and all existing tests pass with the 25-attempt limit
- **Diagnostic preservation:** Per-attempt timing logs remain in debug builds on native

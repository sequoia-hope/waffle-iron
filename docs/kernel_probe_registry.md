# Kernel env-gated probe registry

**Design review 2026-07-12 F12.** The kernel stack carries ~68 distinct
`std::env::var`-gated probes across `yang-rs`, `kernel-v2`, `cherchi-rs`, and
`test-harness`. They are per-campaign bisection scaffolding. This registry
records the policy and, crucially, enumerates the ones that **change geometry
output** so they are auditable.

## Policy

- **Tracing probes** (the large majority) gate `eprintln!` diagnostics ONLY —
  they never alter a geometric or topological decision. They default off
  (unset ⇒ silent) and are safe to leave in the tree, though probes belonging
  to shipped tasks may be swept opportunistically.
- **Behavior-altering neuters** are dev-only bisection kill-switches that DO
  change output. Since F12 they are gated with `cfg!(debug_assertions)`, so
  they are **compiled to the default (correct) path in release / WASM builds**
  and cannot be flipped by a stray environment variable in production. Use them
  only in debug builds when bisecting; the assay runs in release and never
  depends on them.

## Behavior-altering neuters (release-gated)

| Env var | Site | Effect when set (debug only) | Default (always in release) |
|---|---|---|---|
| `TIEBREAK_NEUTER` | `yang-rs/src/stage1_tessellate.rs:~1257` | forces the tessellation tiebreak comparator to `Equal` | real exact-rational tiebreak |
| `YANG_SHIFT_NEUTER` | `yang-rs/src/stage1_tessellate.rs:~2582` | forces the rim sample `shift` to 0 | computed shift |
| `YANG_NSEG_FLOOR` | `yang-rs/src/stage1_tessellate.rs:~271` | raises every rim segment count to a floor | producer-chosen `n_seg` |
| `YANG_RIM_JUNCTION_DISABLE` | `yang-rs/src/boolean.rs:~1853` | disables the Stage-1 rim-junction insertion | insertion enabled |

kernel-v2's ~24 `KV2_*_PROBE` and cherchi-rs's `CHERCHI_*_PROBE` are all
tracing-only (verified in the design review): each gates `eprintln!` and, in
the two that sit inside failure blocks, the `return Err(...)` is unconditional
whether the var is set or not.

## Full tracing-probe list

Enumerate on demand (avoids a list that rots):

```
grep -rhoE 'env::var(_os)?\("[A-Z0-9_]+"\)' \
  crates/{yang-rs,kernel-v2,cherchi-rs,test-harness}/src \
  | grep -oE '"[A-Z0-9_]+"' | sort -u
```

Any probe added to a kernel crate that **alters output** must be added to the
neuter table above AND gated with `cfg!(debug_assertions)`. A tracing probe
that only `eprintln!`s needs no registry entry.

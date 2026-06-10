# predicate-gen — Scope Rules

Code generator for cherchi-rs's clean-room indirect predicates (kernel
rewrite milestone M7). **Dev tooling only** — this crate sits OUTSIDE the
kernel crate layering: nothing depends on it, it ships nothing, and it
must keep **zero dependencies** (pure std). Its output is checked-in Rust
source.

## What it does

- Small SSA expression IR (`src/ir.rs`): variables, `+`, `−`, `×`. No
  division — indirect predicates are polynomial fractions whose
  denominator signs are resolved by Attene's parity rule, never divided.
- FPG forward error analysis (`src/fpg.rs`): Meyer-Pion 2008 Appendix B
  rules, every constant rounded conservatively UP (`next_up` emulation
  of round-toward-+∞). Produces the semi-static filter constant `δ(1)`
  and the homogeneous degree `k` for Attene's runtime scaling `δ·β^k`.
- Emission (`src/codegen.rs` + `src/orient3d.rs`): for each predicate
  instance, a filtered-f64 function, an interval (`Iv`) function, an
  exact (`dashu::rational::RBig`) function over the same polynomial, and
  the tier dispatchers.

## Regeneration

```
cargo run -p predicate-gen
```

writes `crates/cherchi-rs/src/predicates/indirect/generated.rs`
(checked in). NEVER hand-edit that file — the
`checked_in_file_is_fresh` test in `tests/generator.rs` diffs it against
a fresh generation and fails on drift. After changing this crate:
regenerate, then run `cargo test -p predicate-gen -p cherchi-rs`.

## CLEAN-ROOM RULE (absolute)

Sources are the papers ONLY:

- `refs/text/attene-predicates.txt` — Attene 2025: lambda framework
  (§3-4), tiered evaluation + denominator parity (§5), instance
  reduction (§6), semi-static filter `δ(1)·β^k` (Appendix A).
- `refs/text/mesh_arrangement.txt` §4.2.2 — Cherchi 2020: LPI lambdas
  (cross-check vs Attene §4.2) and TPI lambdas; published filter
  constants used as test cross-checks.
- `refs/text/meyer_pion2008_fpg.txt` — Meyer-Pion 2008: forward error
  analysis rules (Appendix B).

Do NOT open anything under `/home/claude/cherchi2022/` (LGPL C++ —
neither headers nor implementation). The FFI crate
`indirect-predicates-sidecar-rs` may be exercised only as a black-box
test oracle from cherchi-rs's test suite.

## Tests

- `tests/generator.rs` — error-analysis hand checks (FPG's published
  det2x2 constant, Cherchi's published `εdL`/`εdT` bands), degree table,
  emission snapshot, determinism, checked-in freshness.
- The generated code's BEHAVIOR is tested in cherchi-rs:
  `tests/indirect_filter_soundness.rs` (filter soundness + hit rate) and
  `tests/indirect_ffi_parity.rs` (differential parity vs the FFI
  reference, dev-dep gated (M7c: the feature is gone; the FFI is a dev-dependency oracle)).

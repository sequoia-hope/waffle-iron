# Waffle Iron — Comprehensive Project Audit (2026-06-09)

Audited at commit `9763b388` on branch `fix/start-sh-port-cleanup` (clean tree).
Method: four parallel investigation passes (new-crate code audit, documentation-vs-code
audit, build/test health run, legacy/consumer audit), with the headline findings
re-verified directly by the driver. No files were modified by the audit itself.

## Executive summary

The kernel rewrite is **architecturally disciplined and hygienically clean** — all seven
new crates build with zero warnings, pass clippy and fmt, and dependency layering is
genuinely compiler-enforced with no imports from the legacy kernel. The honesty
machinery (deviation log, demoted substitutes, loud `Err` instead of fallbacks) is
real and working.

The biggest systemic problem is **environmental reproducibility of correctness claims**:
both C++ sidecars (the Cherchi 2022 `mesh_booleans` binary and the Attene
`Indirect_Predicates` source) are absent from this container at
`/home/claude/cherchi2022/`. Every roadmap milestone marked DONE that was verified
against a sidecar (M0–M3, much of M5, the M6 FFI-backed tests) is currently
**not re-verifiable here**. The default test runs stay green because sidecar-dependent
tests skip or are `#[ignore]`d — which means the green is partly vacuous.

Secondary themes: documentation drift (ARCHITECTURE.md still describes the legacy
kernel as the clean-sheet kernel), test-infrastructure gaps (`scripts/test.sh` covers
no new crate; its fast tier is RED on legacy failures), and the legacy/app side
quietly rotting (WASM bundle out of sync, 34 failing legacy kernel tests).

## Verdicts by crate (new stack)

| Crate | Verdict | Tests (this container) | Notes |
|---|---|---|---|
| cad-primitives | Solid | 13 pass | Types-only, as specced |
| cherchi-rs (default features) | Partial | 331 pass | Predicates, FastTrimesh, Stage-1, soup all real |
| cherchi-rs (`indirect-predicates`) | **Unverifiable here** | 337 pass / **35 fail** | Failures = FFI no-op stub, not regression (see F1) |
| cherchi-sidecar-rs | Solid wrapper | 35 pass / 1 ignored | Binary absent → oracle value currently zero here |
| indirect-predicates-sidecar-rs | Scaffold (by design) | 37 pass | Builds no-op stub when source missing |
| ssi-rs | Partial | 10 pass | 10 solvers, ~1,676 LOC math, thin coverage (F5) |
| yang-rs | Partial | 329 pass / 4 ignored | Stage 5/6 real; sidecar-label path unverifiable here |
| kernel-v2 | Empty scaffold | 0 tests | 34 lines; expected per Phase 4 |

## Findings, ranked

### F1 — `cherchi-rs --features indirect-predicates`: 35 test failures from silent stub fallback (HIGH)
Reproduced directly. `/home/claude/cherchi2022/.../Indirect_Predicates` is absent, so
`indirect-predicates-sidecar-rs/build.rs` emits a warning and builds the no-op stub
(`AVAILABLE = false`). All 35 FFI-dependent tests in `arrangements::{aux_structure,
enforce, retriangulate, soup, intersection_points}` then fail with misleading geometric
errors (e.g. `NoContainingTriangle`, `Retriangulate`), in 0.02s.

Two distinct defects:
1. **Tests don't gate on `indirect_predicates_sidecar_rs::AVAILABLE`.** A missing
   sidecar should produce one clear "FFI sidecar unavailable — run
   scripts/build_sidecars.sh" failure (or skip), not 35 confusing geometric panics.
   A future session could easily burn a day "debugging" the arrangement.
2. **No routine run exercises the FFI path.** Default `cargo test -p cherchi-rs` never
   compiles the feature, and `scripts/test.sh` never runs it, so a real regression in
   the M6 arrangement would also be invisible.

### F2 — Sidecar absence makes the roadmap's DONE claims unreproducible (HIGH)
`docs/yang_functional_roadmap.md` marks M0–M3 DONE ("900-case fuzz, 100% correct,
0 silent-wrong") and most of M5 DONE with sidecar verification. The sidecars don't
exist in this container; `scripts/build_sidecars.sh` exists and looks correct but has
not been run here. Consequences:
- The parity oracle (the project's stated definition of GREEN) is currently inoperative.
- Sidecar-gated tests (`cf1_adversary.rs:632` GREEN witness, `fuzz_curved.rs`) are
  ignored, so the green suite partially measures "tests that can run without the oracle".

This is the first thing any session doing M-milestone work should fix (it is literally
M0). Recommend: run `scripts/build_sidecars.sh` at session start when touching kernel
work, and annotate the roadmap's DONE entries as "verified against sidecar build of
<date>; re-verification requires M0 setup".

### F3 — `scripts/test.sh` covers zero new crates, and its fast tier is RED (HIGH)
- The fast/full crate lists (`scripts/test.sh:26-70`) include only legacy crates. The
  crates that constitute the actual project priority (cherchi-rs, yang-rs, ssi-rs,
  cad-primitives, kernel-v2, both sidecars) are not in any tier.
- `./scripts/test.sh fast` currently FAILS in 16s on legacy code: file-format (2 STEP
  export tests), kernel tessellation (1), modeling-ops (9 `truck_*` fillet/chamfer/
  revolve tests), wasm-bridge (4 incl. boolean union and sketch-solve dispatch).
  A permanently red pre-commit gate trains everyone to ignore it.

Recommend: add a `new`/`rewrite` tier (or fold new crates into `fast`), and either fix
or explicitly quarantine the legacy failures with a tracking note so `fast` is green.

### F4 — Production panic sites in yang-rs (MEDIUM)
- `crates/yang-rs/src/lib.rs:3130` — `matched_idx.unwrap()` in `ssi_curve_to_curve`
  can panic if an SSI curve matches no candidate.
- `crates/yang-rs/src/lib.rs:3941-3943` — `.unwrap()` on `remap[...]` during Stage-5/6
  reconstruction; panics if remap is sparse.
- `crates/cherchi-rs/src/arrangements/retriangulate.rs:1067,1098` — `.expect()` on
  invariants that, per F1, demonstrably *can* fire when predicates misbehave.

All new crates are supposed to be `Result<>`-only (no `catch_unwind` post-rewrite), so
these should become typed `YangError`/`ArrangementError` variants.

### F5 — ssi-rs coverage is thin relative to its risk (MEDIUM)
~1,676 lines of closed-form conic/quadric math, 10 solvers + dispatcher + curve
evaluator, with only 10 integration tests passing in this container and no unit tests
in `lib.rs`. Tolerance discipline is good (consistent `TAU_MODEL`, loud
`AnalyticalSolutionNotAvailable` for non-coaxial degree-4 cases — correct per A15.2).
But tangency, near-degeneracy, and symmetry (argument-order) cases are largely
untested. This feeds Yang Stage 4; wrong curves there become wrong B-Rep edges.

### F6 — Documentation drift would mislead a new session (MEDIUM)
- **ARCHITECTURE.md** (lines ~13-189) still presents the *legacy* `crates/kernel/` as
  "the clean-sheet kernel" with "980 tests" and "15 SSI solvers, all integrated" —
  the new layered architecture (cad-primitives → cherchi-rs/ssi-rs → yang-rs →
  kernel-v2) is absent from the system diagram. This is the primary onboarding doc.
- **`crates/yang-rs/src/lib.rs:23`** header says "Current implementation status
  (PR-YR5)"; reality is PR-YR23+.
- **CLAUDE.md priorities** still front M0/M1 as "the next concrete work" while the
  recent commit stream is deep in M6 (PR-CR-AR3b just closed); M0 remains undone in
  this container though (see F2), so the two are entangled.
- **docs/yang_deviations.md** interleaves historical legacy deviations (D1-D14, some
  REMOVED) with active new-crate deviations (N1-N18) without a clear split.

### F7 — WASM bundle out of sync with kernel source (MEDIUM, legacy)
`app/static/pkg/` was last rebuilt at `db657fd3` (Y62, May 21), but `crates/kernel`
has at least one later behavioral fix (`e96d725a`, Y63 Newell face-normal fix, May 22).
CLAUDE.md requires the bundle to ship in the same commit as Rust changes. The app is
running stale kernel behavior. Either rebuild+commit, or decide the legacy app bundle
is frozen until Phase 5 and write that down.

### F8 — Legacy-patch policy vs. actual kernel commit stream (DECISION NEEDED)
`crates/kernel` received the full Y58–Y63 cycle series (active debugging of
`yang_integration.rs` / tessellation) after the rewrite was declared. Under CLAUDE.md
these should be rare, flagged "legacy patch", and minimized. Either these were
intended as ship-blocking legacy patches (fine — say so), or the routing rule needs
re-stating. Worth an explicit decision so future sessions don't follow the precedent.

### F9 — Kernel trait surface carries dead weight into kernel-v2 (LOW)
`Kernel`/`KernelIntrospect` in waffle-types total 24 methods including
fillet/chamfer/shell (deferred indefinitely). CLAUDE.md already permits trait
refinement; slimming before Phase 4 implementation starts reduces kernel-v2's
obligation surface.

### F10 — Hygiene backlog (LOW, mostly legacy)
- `cargo fmt --check` fails on 7 legacy files (kernel tessellation/boolean,
  test-harness).
- Legacy kernel: 105 clippy warnings, 60 build warnings, **1250 pass / 34 fail / 43
  ignored** lib tests.
- 20 Playwright specs carry `.fixme()`/`.skip()` (boolean-dialog: 7, auto-union: 7,
  arc-drag: 3, etc.) out of 124 spec files.
- `projects/*/PLAN.md` files are 3-4 months stale and predate the rewrite.
- Stale branches (`prototype-waffle`, `auto-waffle/*` Mar-May, `docs/architecture-v1`).
- Fillet/chamfer/shell deferral banners verified still in place (good).

## What is genuinely in good shape

- **Layering**: no cycles, no legacy imports, enforced in Cargo.toml — verified.
- **New-crate hygiene**: 0 warnings, clippy clean, fmt clean across all seven.
- **Honesty markers**: substitute attribution demoted to `#[cfg(test)]` and kept as a
  differential oracle; non-coaxial SSI returns loud errors, no mesh fallback;
  WASM-incompatibility of the sidecars is documented everywhere it matters.
- **Tolerance discipline**: centralized `TAU_MODEL`/`TAU_WORK`/`MIN_FEATURE_SIZE`; no
  ad-hoc epsilons found in the new crates.
- **Unsafe**: confined to the FFI sidecar, documented invariants, none elsewhere.

## Recommended order of work

1. Run `scripts/build_sidecars.sh`; restore the parity oracle (this is M0, and it
   unblocks re-verification of every DONE claim). (F2)
2. Make FFI-dependent cherchi-rs tests fail loudly/skip on `!AVAILABLE`, and add the
   feature-gated suite to a test tier so the M6 path is routinely exercised. (F1, F3)
3. Fix the two yang-rs production `unwrap()`s. (F4)
4. Update ARCHITECTURE.md and the yang-rs lib.rs status header. (F6)
5. Decide and record the legacy-patch / WASM-bundle posture (freeze vs. maintain). (F7, F8)
6. Strengthen ssi-rs tests (tangency, near-degenerate, argument symmetry). (F5)
7. Hygiene sweep when convenient. (F9, F10)

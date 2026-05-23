# ssi-rs — Scope Rules

Phase 3 of the clean-sheet kernel rewrite (see root `CLAUDE.md`). Analytical surface-surface intersection solvers, used by `yang-rs` Stage 3 refinement.

## What this crate does

- One solver per quadric surface pair (15 unique pairs total — see `src/lib.rs` matrix)
- Each solver: takes two analytical surface representations, returns the analytical intersection curves on both
- Curves are exact parameterized representations (lines, circles, ellipses, conics, NURBS in general) — NOT polylines or sampled points

## What this crate does NOT do

- Mesh anything — SSI is purely analytical
- Boolean dispatch — that's `yang-rs`
- B-Rep construction — that's `kernel-v2`
- Surface evaluation / tessellation — out of scope
- Numerical fallback for cases where analytical solution doesn't exist — also out of scope (return `Err(SsiError::AnalyticalSolutionNotAvailable)` and let `yang-rs` decide whether to fall back to mesh-only refinement)

## Hard rules

1. **Zero workspace deps except `cad-primitives`.** No imports from sibling tier crates.
2. **Reference-cited solver implementations.** Every solver must cite Patrikalakis Ch.5 (or equivalent reference) in its doc comment. No ad-hoc algebraic manipulation.
3. **No `unsafe`, no `panic!` in production paths.** All errors return `Result<>`.
4. **Determinism.** Same inputs → byte-identical outputs across runs and platforms.
5. **Single-threaded.** Each solver is a pure function; parallelism is the caller's concern.

## When working on this crate

You may read:
- Everything inside `crates/ssi-rs/` and `crates/cad-primitives/`
- Reference papers: Patrikalakis ch.5, Yang 2025 §4.3
- Mathematical references / textbooks as needed

You may NOT read:
- `crates/kernel/src/ssi/` (the legacy partial port being replaced)
- Sibling tier crates (`cherchi-rs`, `yang-rs`, `kernel-v2`)
- The old `ssi_status_correction.md` audit (concerns the legacy port's drift)

## Migration policy for existing SSI solvers

The legacy kernel has partial SSI solvers (per `ssi_status_correction.md`, many are stubs or partial). For each solver in the matrix:

- If the legacy implementation is **complete and verified** (independent test passes), port it as a clean rewrite — read the legacy file ONCE for reference, then close it and implement from the spec.
- If the legacy implementation is a **stub or partial**, implement from scratch using Patrikalakis as the spec.

Never read multiple legacy files at once. The discipline is to treat legacy as a hint of which math to use, NOT as source code to lift.

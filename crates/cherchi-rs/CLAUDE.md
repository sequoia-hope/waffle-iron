# cherchi-rs — Scope Rules

Phase 1 of the clean-sheet kernel rewrite (see root `CLAUDE.md` §"Kernel Rewrite In Progress"). This crate is a pure-Rust port of Cherchi 2020 + 2022 mesh booleans.

## What this crate does

- Mesh arrangement (Cherchi 2020 §4-§5): take two triangle soups, produce a non-self-intersecting triangle soup with consistent topology
- Boolean labeling (Cherchi 2022 §5): ray-cast each face to determine inside/outside relative to each input mesh
- Boolean op output: from labels, emit the triangle subset for union / intersect / subtract / xor
- Stage 2 produces a **`LabeledArrangement`** — the producer-agnostic Stage-2
  contract defined once in `docs/yang_functional_roadmap.md` §2 (per-output-tri
  `source: SmallVec<(InputId, parent_tri_index)>` — multi-valued at coplanar
  overlap — plus `patch_id` and a per-input in/out vector). Do NOT redefine that
  shape here; implement to it. The native arrangement and the interim
  `cherchi-sidecar-rs` producer must satisfy the **same** interface so the
  sidecar can serve as this crate's differential-parity oracle.

## What this crate does NOT do

- B-Rep anything — no faces, edges, loops, vertices-as-topology, surface geometry
- Analytical surfaces — no planes, cylinders, spheres beyond what's needed for predicates
- SSI refinement — that's `ssi-rs`'s job
- Tessellation of B-Rep faces — that's `yang-rs`'s job
- Boolean dispatch / fallback / hybrid pipeline — that's `yang-rs`'s job

If asked to "fix a boolean bug" or "make booleans support X," check whether the issue is mesh-level (this crate) or B-Rep-level (yang-rs / kernel-v2) before opening a file here.

## Hard rules

1. **Zero workspace deps except `cad-primitives`.** No imports from `crates/kernel/`, `crates/feature-engine/`, `crates/waffle-types/`, etc. The compiler enforces this via `Cargo.toml`.
2. **Reference parity is the correctness oracle.** Every PR runs the C++ sidecar differential diff (`docs/sidecar/cherchi2022_build_guide.md`). If the diff has regressions, the port is wrong.
3. **License attribution on ported files.** Any `.rs` file that ports C++ from the upstream MIT-licensed repos must include a header:
   ```
   //! Ported from Cherchi et al. 2020 / 2022 (MIT).
   //! © Gianmarco Cherchi et al.
   //! https://github.com/gcherchi/FastAndRobustMeshArrangements
   //! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
   ```
   `LICENSE-THIRD-PARTY.md` in this crate tracks the master list.
4. **No `unsafe`.** The C++ uses some unsafe pointer work; the Rust port uses indices and references.
5. **Single-threaded by default.** Cherchi C++ uses TBB which is non-deterministic at multi-thread. Determinism trumps speed during the port. Parallelism via `rayon` is a future feature flag, not the default.
6. **No `panic!` in production code paths.** All error conditions return `Result<>`. Panics in release builds are bugs. (This keeps the WASM build on stable Rust.)
7. **Exact arithmetic via `dashu`** (pure Rust, WASM-compatible) for the
   *explicit* predicates. **Amended (2026-05-28):** the Stage-2 arrangement may
   take a **temporary, feature-gated, non-WASM** dependency on
   `indirect-predicates-sidecar-rs` for the *indirect/implicit-point* predicates
   (LPI/TPI) during the development phase. This intentionally breaks the WASM
   build behind that feature — accepted per root `CLAUDE.md` / the user directive
   (FFI now → functional Yang → clean-room the predicates from Attene's paper →
   restore WASM, roadmap M7). The pure-Rust/WASM end state is restored at M7;
   until then, gate the FFI dep so the rest of the crate still builds for wasm32.
   No `rug`, no `num-bigint`, no rolling our own bignum.
8. **Predicates are demand-driven.** Do NOT port indirect predicates ahead of a
   caller (the IP1–IP6 PRs ported against zero consumers, which is why parity
   never engaged). Add an FFI predicate wrapper only when the arrangement code
   that calls it lands in the same slice.

## When working on this crate

You may read:
- Everything inside `crates/cherchi-rs/` and `crates/cad-primitives/`
- The reference paper texts: `refs/text/cherchi2020_*.txt`, `refs/text/cherchi2022_*.txt`
- The upstream C++ source if available locally (typically at `/home/claude/cherchi2022/` or `/home/claude/cherchi2020/`)
- `docs/sidecar/cherchi2022_build_guide.md`

You may NOT read:
- `crates/kernel/src/boolean/` (the legacy port being replaced)
- Sibling crates' internals (`yang-rs`, `ssi-rs`, `kernel-v2`)
- Old audit memos under `docs/audits/cherchi_port_audit.md` etc. (they document the old port's drift — irrelevant to the clean port)

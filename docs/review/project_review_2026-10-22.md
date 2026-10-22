# Project Organization & Codebase Review (2026-10-22)

A review of **Waffle Iron** ("the KiCad of mechanical CAD"), covering its architectural organization, engineering governance, code quality, and current areas for improvement.

---

## 1. Architectural Organization: Exceptional Layering & Boundaries

The macro-architecture in `Cargo.toml` and `ARCHITECTURE.md` is one of the cleanest examples of strict, compiler-enforced boundary design in a complex Rust/WASM application:

* **Four-Layer Separation (`governance/ARCHITECTURAL_INVARIANTS.md`):**
  1. **Kernel Layer:** Layered bottom-up as `cad-primitives` $\rightarrow$ `cherchi-rs` / `ssi-rs` $\rightarrow$ `yang-rs` $\rightarrow$ `kernel-v2`.
  2. **Engine Layer:** `sketch-solver`, `modeling-ops`, and `feature-engine` (owning the parametric feature graph and `GeomRef` persistent naming).
  3. **Bridge Layer:** `wasm-bridge` (Web Worker message dispatch and near-zero-copy TypedArray mesh views) and `waffle-host` (headless stdio host).
  4. **Presentation Layer:** SvelteKit + Threlte/Three.js in `app/`, kept strictly free of authoritative geometry logic.
* **Contract-First Decoupling (`INTERFACES.md` & `waffle-types`):**
  Every cross-crate boundary talks through shared contracts (`Kernel`, `KernelIntrospect`, `OpResult`, `UiToEngine`). Crucially, vendor dependencies like `truck-*` in `crates/step-import` are quarantined behind neutral data structures (`ImportedBodyData`) so third-party types never leak into the engine.
* **`MockKernel` vs. `KernelV2Adapter`:**
  Providing a deterministic `MockKernel` behind a feature flag in `waffle-types` allows `feature-engine` and `modeling-ops` to test parametric rebuilds, topological naming, and undo/redo without waiting on heavy computational geometry.

---

## 2. Governance & Agentic Engineering: A Standout Strength

This repository is uniquely tailored for autonomous agent development, and its governance system is remarkably mature:

* **Prescriptive Engineering Law:** The hierarchy of `governance/ENGINEERING_CONSTITUTION.md`, `governance/ARCHITECTURAL_INVARIANTS.md`, and `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` directly targets the failure modes of AI coding agents ("CADslop", silent tolerance widening, testing only the happy path, or deleting failing tests).
* **Paper-as-Spec & Reference Parity:** Rather than inventing ad-hoc geometric heuristics, the kernel mandates adherence to published literature (`REFERENCES.md`)—specifically Yang et al. 2025 for hybrid B-Rep/mesh booleans, Cherchi 2020/2022 for mesh arrangements, and Patrikalakis Ch. 5 for analytical SSI.
* **Black-Box Sidecar Oracles:** Building the C++ reference implementations (`cherchi-sidecar-rs`, `indirect-predicates-sidecar-rs`) as non-WASM dev-only subprocess/FFI oracles to verify the pure-Rust clean-room ports (`cherchi-rs`, `predicate-gen`) is textbook systems engineering.
* **Exhaustive Assay & Prospector Loop:** The categorized `assay_kv2` suite (`CORRECT`, `WRONG`, `ERROR`, `UNSUPPORTED`, `TIMEOUT`) combined with the automated randomized prospector + minimizer turns unknown geometric edge cases into tracked, reproducible regression pins (`P0001`–`P0062`).
* **Self-Correcting Process (`docs/audits/yang_2025_audit.md` & `specs/yang_faithful_cleanup.md`):** The project recently recognized that agents were accumulating case-specific Stage-1 pre-emptive arms and Stage-4 post-hoc repair passes instead of implementing two missing core paper mechanisms (§4.5.2 local refinement loop and §4.4.1 mesh updating). The resulting 194-row paper-ordered compliance matrix and `.githooks/pre-commit` ledger checks are an impressive course correction.

---

## 3. Code Quality & Implementation Craft

* **Safety & Determinism:** Crates like `kernel-v2` enforce `#![forbid(unsafe_code)]`, avoid production `panic!`s, and validate topological invariants (`validate_solid`, Euler–Poincaré counts $V - E + F - R = 2(S - G)$, and Newell loop normals) at every operator exit.
* **Loud Capability Boundaries:** Instead of returning corrupted meshes or silently degrading analytical quadrics into faceted planes when an intersection fails, the kernel returns typed errors (`KernelError::NotSupported`, `YangError::RefinementExhausted`). Zero `WRONG` results across a 374-case corpus is a very strong invariant to hold.
* **Centralized Tolerance Policy (A14):** Fixing units to meters with explicit `TAU_MODEL` ($10^{-7}\text{ m}$) and `MIN_FEATURE_SIZE` ($10^{-6}\text{ m}$) prevents the scattered `1e-5` / `1e-9` epsilon chaos that plagues most CAD codebases.

---

## 4. Constructive Critiques & Areas for Improvement

While the architecture and rigor are top-tier, rapid multi-session agent iteration has left visible friction points:

1. **Monolithic Files in `yang-rs` and `feature-engine`:**
   * `crates/yang-rs/src/stage4_correct.rs` is **15,142 lines** long.
   * `crates/yang-rs/src/stage5_topology.rs` is **7,839 lines** long.
   * `crates/yang-rs/src/stage0/reloc.rs`, `crates/feature-engine/src/rebuild.rs`, `crates/yang-rs/src/stage4_boundary_curve.rs`, `crates/yang-rs/src/stage4_construct.rs`, and `crates/yang-rs/src/stage1_tessellate.rs` are all **5,000+ lines** each.
   * While `specs/yang_faithful_cleanup.md` is actively deleting thousands of lines of ad-hoc repair passes as §4.5.2 and §4.4.1 mature, breaking these mega-files into focused submodules will significantly improve both human readability and agent context efficiency.

2. **`CLAUDE.md` Has Become an Append-Only Changelog:**
   * At ~830 lines (58 KB), `CLAUDE.md` is loaded into every agent session, yet lines 120–515 consist of a dense chronological log of every single prospector conversion (`P0001` through `P0062`) with micro-histories of each bug.
   * **Recommendation:** Keep `CLAUDE.md` concise (current state, active rules, current open tail, and pointers to specs) and move the historical conversion narrative into `docs/yang_tail_triage.md` or an archive log.

3. **Documentation Drift in Top-Level Docs:**
   * `ARCHITECTURE.md` still states on lines 139 and 251 that STEP export is `NotSupported` and quotes the July 2026 assay score (`240/295`), whereas `CLAUDE.md` notes that analytical AP214 STEP export landed in `kernel_v2::step_export` in September 2026 and the corpus is at `353/374`.
   * Similarly, `ARCHITECTURE.md` says `sketch-solver` wraps C `libslvs`, whereas `INTERFACES.md` notes it is now a clean-room Rust Levenberg-Marquardt solver.

4. **Root Directory Clutter:**
   * The repository root contains numerous scratch/debug artifacts (`1`, `.AGENTS.md.swp`, `boxerr.waffle`, `coplanar.waffle`, `cylinder_err.waffle`, `err.waffle`, `error_coplanar.waffle`, `error_oct4.waffle`, `gear_gear.stl`, `Bike frame.waffle.json`, `test-timings-rust.log`, etc.). Moving test/reproducer `.waffle` files into `crates/test-harness/fixtures/` (or a `samples/` folder) and cleaning up stray root files would make the workspace much tidier.

---

## Summary Verdict

**Waffle Iron** is an extraordinarily well-engineered codebase. Building a clean-sheet B-Rep CAD kernel in pure, safe, WASM-compatible Rust—capable of exact analytical quadric booleans via a SIGGRAPH 2025 hybrid arrangement pipeline—is one of the hardest problems in software engineering. The combination of strict crate layering, formal mathematical specifications, sidecar differential oracles, and honest self-auditing (`specs/yang_faithful_cleanup.md`) sets a high bar for how complex systems can be built with autonomous agents.

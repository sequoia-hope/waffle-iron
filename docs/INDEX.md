# Documentation Index

All documentation files in `docs/`, categorized. Updated 2026-03-02.

## Architecture

| File | Description |
|------|-------------|
| `SYSTEM-INTERFACES.md` | Cross-crate data flows, dependency graph, interface type contracts |
| `PERSISTENT-NAMING.md` | GeomRef persistent naming system for stable geometry references across rebuilds |
| `test-tooling-design.md` | GUI architecture and testing infrastructure component hierarchy |
| `NONDETERMINISM-REPORT.md` | Determinism and stability analysis of truck boolean operations |
| `SKETCH-SYSTEM-PLAN.md` | Sketch system development plan with current capabilities and roadmap |

## Governance

Located in `/governance/`, not `docs/`:

| File | Description |
|------|-------------|
| `governance/ENGINEERING_CONSTITUTION.md` | Non-negotiable engineering rules (P1-P7), test requirements, amendment process |
| `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` | Required workflow for modeling features: spec, test, implement, validate |

## Testing

| File | Description |
|------|-------------|
| `TESTING.md` | Test tiers (Rust fast/full, GUI fast/full), how to run, how to add tests |
| `TESTING-STRATEGY.md` | Test pyramid, layer definitions, testing philosophy |
| `gui-test-plan.md` | 5 core parametric CAD workflows with test matrices |
| `gui-test-skeptic-report.md` | Adversarial quality review of all GUI tests (2026-02-11) |
| `MANUAL-SNAP-CLICK-TEST.md` | Manual browser verification steps for snap-click behavior |

## Plans

| File | Description |
|------|-------------|
| `SELECTION-ENHANCEMENT-PLAN.md` | Interactive viewport picking and face region selection |
| `SKETCH-SYSTEM-PLAN.md` | Sketch system development roadmap (also listed under Architecture) |
| `PROMPTING.md` | Recommendations for backlog management and agent workflow |

## References

Academic papers and external specifications in `docs/references/`:

| File | Description |
|------|-------------|
| `references/opencascade-boolean-operations.md` | OpenCascade boolean operations specification |
| `references/edelsbrunner-mucke-sos.md` | "Simulation of Simplicity" (1990) — degenerate case handling |
| `references/sugihara-iri-topology-oriented.md` | "Topology-Oriented Implementation" (Algorithmica 2000) |
| `references/zhou-mesh-arrangements-2016.md` | "Mesh Arrangements for Solid Geometry" (SIGGRAPH 2016) |
| `references/levy-exact-constructions-2025.md` | "Exact Predicates, Exact Constructions for Mesh CSG" (Inria 2025) |
| `references/levy-exact-constructions-2025-tex.md` | TeX source of the Levy paper (arXiv 2405.12949) |
| `references/hachenberger-nef-ewcg2005.md` | "Boolean Operations on 3D Selective Nef Complexes" (EWCG 2005) |
| `references/barki-robust-booleans-2015.md` | "Exact, Robust, and Efficient Regularized Booleans" (2015) |
| `references/astarlioglu-comparing-booleans-2023.md` | "Comparing Boolean Operation Methods" — Aalto MSc thesis (2023) |
| `references/cherchi-indirect-predicates-2020.md` | "Fast and Robust Mesh Arrangements" (SIGGRAPH Asia 2020) |

## Reviews

Pipeline analysis reports in `docs/review/`:

| File | Description |
|------|-------------|
| `review/phase1-intersection-division.md` | Intersection + face division + coplanar handling |
| `review/phase2-classification-assembly.md` | Classification + shell assembly + robustness infrastructure |
| `review/phase3-healing-perturbation.md` | Healing + perturbation analysis vs literature solutions |
| `review/phase4-governance-gaps-roadmap.md` | Governance compliance, spec gaps, test coverage, improvement roadmap |

## Specifications

Spec files live in `/specs/`, not `docs/`. See [`specs/STATUS.md`](../specs/STATUS.md) for the full inventory with implementation status.

## Top-Level Architecture Docs

These live at the repo root, not in `docs/`:

| File | Description |
|------|-------------|
| `ARCHITECTURE.md` | System architecture: 4 layers, data flow, sub-project map |
| `INTERFACES.md` | Cross-crate type contracts and shared interface definitions |

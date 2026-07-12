# Documentation Index

All documentation files in `docs/`, categorized. Updated 2026-07-12.

## Architecture

| File | Description |
|------|-------------|
| `SYSTEM_DESIGN.md` | Research-annotated system architecture with `[#N]` reference citations per subsystem |
| `ALGORITHM_DECISIONS.md` | Algorithm Decision Records (ADR-1 through ADR-9) with alternatives and rationale |
| `CROSS_REFERENCE.md` | Algorithm → paper → code mapping for 30+ implementations |
| `SYSTEM-INTERFACES.md` | Cross-crate data flows, dependency graph, interface type contracts |
| `PERSISTENT-NAMING.md` | GeomRef persistent naming system for stable geometry references across rebuilds |
| `test-tooling-design.md` | GUI architecture and testing infrastructure component hierarchy |
| `NONDETERMINISM-REPORT.md` | HISTORICAL (pre-2026-06-11): determinism/stability analysis of the retired truck boolean operations |
| `SKETCH-SYSTEM-PLAN.md` | Sketch system development plan with current capabilities and roadmap |

## Governance

Located in `/governance/`, not `docs/`. The four pillars, highest precedence
first:

| File | Description |
|------|-------------|
| `governance/ENGINEERING_CONSTITUTION.md` | Non-negotiable engineering rules (P1-P10, incl. P9 no-hack-to-green / P10 abort-on-wrong-diagnosis), test requirements, amendment process |
| `governance/ARCHITECTURAL_INVARIANTS.md` | Architectural invariants (A0–A15), incl. A14 tolerance layering and A15 analytical-primacy / Yang hybrid boolean pipeline |
| `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` | Required workflow for modeling features: spec, test, implement, validate (roles, phases, oracles) |
| `governance/DEFINITION_OF_DONE.md` | Definition of Done checklist gating a change as complete |

## Kernel & Yang Boolean Pipeline

The live kernel is the layered `kernel-v2` stack (Phase-6 migration COMPLETE
2026-06-11); these docs govern it. See root `CLAUDE.md` §"Kernel: kernel-v2".

| File | Description |
|------|-------------|
| `yang_functional_roadmap.md` | **Plan of record** for the whole kernel stack: `LabeledArrangement` interface, milestones M0–M8, reference-parity strategy |
| `yang_deviations.md` | Ledger of known divergences between the implementation and Yang 2025 / Cherchi 2022 — a per-increment entry is a **merge blocker** (Constitution P2 artifact for Yang increments) |
| `audits/yang_2025_audit.md` | Per-step CORRECT / INCOMPLETE / WRONG / STUB assessment of the Yang pipeline vs the paper — read before working on a stage |
| `review/design_review_2026-07-12_kernel.md` | Multi-agent kernel design review (findings F1–F18, governance-doc issues G1–G11) driving current remediation |

## Testing

| File | Description |
|------|-------------|
| `TESTING.md` | Test tiers (`rewrite`, `parity`, `fast`, `full`, GUI fast/full, `all`) + the categorized kernel-v2 `assay_kv2` oracle, how to run, how to add tests |
| `TESTING-STRATEGY.md` | Test pyramid, layer definitions, testing philosophy |
| `gui-test-plan.md` | 5 core parametric CAD workflows with test matrices |
| `gui-test-skeptic-report.md` | Adversarial quality review of all GUI tests (2026-02-11) |
| `MANUAL-SNAP-CLICK-TEST.md` | Manual browser verification steps for snap-click behavior |

## Plans

| File | Description |
|------|-------------|
| `prototype_release_roadmap.md` | Cross-cutting epic: path to the planetary-gearbox / prototype-release demo (sequences kernel KV12/KV13 + app/UX + tooling) |
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

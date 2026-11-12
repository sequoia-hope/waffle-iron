# Documentation Index

The documents that describe how Waffle Iron works and what is being built
next. Paths are relative to the repository root.

## Start here

| File | What it is |
|------|------------|
| `README.md` | Project overview |
| `ARCHITECTURE.md` | The layers, the crates, and how data flows between them |
| `INTERFACES.md` | The contracts between crates and between the engine and the UI |
| `DEVELOPMENT.md` | Setting up a development environment |
| `CONTRIBUTING.md` | Checks before a commit, test tiers, rules |
| `CLAUDE.md` | Session guide for coding agents: priorities, routing, conventions |
| `AGENTS.md` | Agent roles and which agent owns which directory |

## Governance

Highest precedence first. These override every other document.

| File | What it is |
|------|------------|
| `governance/ENGINEERING_CONSTITUTION.md` | The non-negotiable engineering rules |
| `governance/ARCHITECTURAL_INVARIANTS.md` | Architectural invariants, including tolerance layering (A14) and the hybrid boolean pipeline (A15) |
| `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` | The spec → test → implement → validate workflow |
| `governance/DEFINITION_OF_DONE.md` | The checklist that gates a change as complete |
| `agents/ORCHESTRATION.md` | How agent roles are dispatched within a feature cycle |

## Kernel

| File | What it is |
|------|------------|
| `docs/kernel_1_0.md` | What kernel 1.0 means and the gates to reach it |
| `docs/yang_faithfulness_roadmap.md` | The work order: milestones F0–F7 toward a faithful implementation of Yang et al. 2025 |
| `specs/yang_faithful_cleanup.md` | The cleanup plan: which code stands in for which paper mechanism, and the order of retirement |
| `docs/yang_deviations.md` | The ledger of every divergence from the paper. A kernel change updates it in the same commit |
| `docs/audits/yang_2025_audit.md` | Compliance matrix: one row per procedural statement of the paper, with code site and status |
| `docs/yang_functional_roadmap.md` | Pipeline architecture, the `LabeledArrangement` interface, and reference-parity strategy |
| `docs/yang_tail_triage.md` | Per-case ledger of the boolean assay's failing cases |
| `docs/kernel_probe_registry.md` | The environment-variable probes in the kernel stack and which of them change output |
| `docs/review/remediation_2026-10-28.md` | Open remediation items from the independent review: oracles, gates, robustness |
| `docs/sidecar/cherchi2022_build_guide.md` | Building the reference C++ sidecars used as parity oracles |

## Engine, file format and features

| File | What it is |
|------|------------|
| `docs/FILE_FORMAT.md` | The `.waffle` file format specification |
| `docs/schema/` | JSON Schemas generated from the persisted Rust types and pinned by tests |
| `specs/waffle_v4_document_model.md` | The document model: identity, git-aware sources, scoped references |
| `docs/CUSTOM_FEATURE_SCRIPTS.md` | Custom feature scripts (Rhai): language, API, tools |
| `specs/custom_features_and_modeling_roadmap.md` | Design of custom feature scripts and patterns |
| `specs/drawings_and_mbd.md` | Drawings, annotations, materials and model-based definition |
| `specs/sketch3d.md` | 3D sketches |
| `docs/step_import_roadmap.md` | STEP import |
| `specs/step_import_si5_exact_analytic_ingestion.md` | Exact (analytic) STEP ingestion |
| `specs/kicad_board_link.md`, `specs/board_shape_feature.md` | KiCad board link and the board-shape feature |

## Agent link

| File | What it is |
|------|------------|
| `specs/waffle_mcp_server.md` | The MCP agent link between a local agent and the app |
| `specs/waffle_server_mode.md` | The headless engine host and viewer sync |
| `specs/agent_mechanical_design.md` | The tool surface an agent uses to design parts: references, names, measurement, sketching |
| `relay/README.md` | Running the MCP relay |

## Testing

| File | What it is |
|------|------------|
| `docs/TESTING.md` | Test tiers, how to add tests, and how to run the boolean assay, the prospector and the other oracles |
| `specs/assay_prospector.md` | The prospector that searches for the assay's next failing case |

## Other material

`specs/` holds one design spec per feature or mechanism; a spec is current when
a document above or the code cites it. `REFERENCES.md` lists the reference
papers and materials. Files under `docs/` that are not listed here are working
notes and investigation records; they are not maintained as descriptions of the
current system.

# Boolean Foundation Burndown

Long-term roadmap for bringing Waffle Iron's boolean subsystem to production quality,
as defined by `docs/SHAPEOPS-BOOLEAN-SPEC.md`.

**Priority legend**: P0 = blocking, P1 = important, P2 = wanted, P3 = nice-to-have
**Size legend**: S = hours, M = day, L = multi-day, XL = week+

---

## Phase A: Foundation (Tolerance + Errors + Predicates)

Lays the infrastructure all subsequent boolean work depends on.

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| A1 | Structured `BooleanError` enum replacing `Option<Solid>` | P0 | M | — | kernel-fork/types.rs, truck-shapeops/integrate, kernel-fork/truck_kernel.rs | Sprint 1 |
| A2 | `BooleanOptions` tolerance context (tau_model/mesh/weld/work/coplanar) | P0 | M | — | kernel-fork/types.rs, kernel-fork/truck_kernel.rs, kernel-fork/healing.rs | Sprint 1 |
| A3 | Robust geometric predicates (`robust` crate) in ray-cast classification | P1 | L | A1 | truck-shapeops/Cargo.toml, truck-shapeops/integrate, truck-shapeops/coplanar.rs | Sprint 1 |
| A4 | Replace AND/OR tagging with `RelationToOther` classification | P2 | L | A1,A3 | truck-shapeops/loops_store, truck-shapeops/integrate, truck-shapeops/divide_face | — |

**Parallelization**: A1 and A2 in parallel. A3 after A1. A4 after A1+A3.

---

## Phase B: Boolean Pipeline Hardening

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| B1 | Complete coplanar face splitting (HANDOFF doc) | P0 | XL | A1 | truck-shapeops/loops_store, coplanar_splitting.rs, integrate | — |
| B2 | Add `difference()` and XOR boolean operations | P1 | M | A4 or standalone | truck-shapeops/integrate, kernel-fork/traits.rs, modeling-ops/boolean.rs | Sprint 1 (difference only) |
| B3 | Box-cylinder boolean reliability | P1 | XL | A2,B1 | truck-shapeops/intersection_curve, kernel-fork/healing.rs | — |
| B4 | `Solid::try_new` enforcement (no panics) | P1 | S | A1 | truck-shapeops/integrate | — |
| B5 | `TouchingPolicy` for degenerate cases | P2 | M | A2,A4 | kernel-fork/types.rs, truck-shapeops/integrate | — |

**Parallelization**: B1, B2, B4 in parallel. B3 after B1. B5 after A4.

---

## Phase C: Missing Kernel Operations

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| C1 | TruckKernel `chamfer_edges` (planar geometry only) | P1 | L | — | kernel-fork/truck_kernel.rs | — |
| C2 | TruckKernel `shell` (face offset + boundary rebuild) | P1 | XL | — | kernel-fork/truck_kernel.rs | — |
| C3 | TruckKernel `fillet_edges` (rolling-ball surfaces) | P2 | XL | C1 | kernel-fork/truck_kernel.rs | — |

**Parallelization**: C1 and C2 in parallel. C3 after C1 (shares trimming infra).
All of Phase C is independent of Phases A-B.

---

## Phase D: UI Completion

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| D1 | Edge selection mode in 3D viewport | P0 | M | — | app/viewport/, app/engine/store.svelte.js | — |
| D2 | Fillet dialog | P1 | M | D1,C3 | app/ui/FilletDialog.svelte, Toolbar.svelte | — |
| D3 | Chamfer dialog | P1 | S | D1,C1 | app/ui/ChamferDialog.svelte, Toolbar.svelte | — |
| D4 | Shell dialog | P1 | S | C2 | app/ui/ShellDialog.svelte, Toolbar.svelte | — |
| D5 | Revolve live preview (ghost mesh) | P2 | M | — | app/viewport/GhostPreview.svelte, RevolveDialog.svelte | — |

**Parallelization**: D1 and D5 in parallel. D2/D3/D4 after their kernel + D1 deps.

---

## Phase E: Advanced Features + Polish

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| E1 | GeomRef testing against real TruckKernel | P1 | M | A | test-harness, feature-engine/resolve.rs | — |
| E2 | Query-based `Selector::Query` resolution | P2 | M | E1 | feature-engine/resolve.rs, waffle-types/geom_ref.rs | — |
| E3 | Local per-edge tolerances (`tau_local`) | P3 | L | A2,A4 | truck-shapeops/intersection_curve | — |
| E4 | Input validation/healing modes (strict + heal) | P3 | M | A1 | truck-shapeops/healing, kernel-fork/truck_kernel.rs | — |
| E5 | Property tests + degenerate regression corpus | P2 | L | A1,B1,B2 | test-harness, kernel-fork/tests/ | — |
| E6 | Revolve role detection fix (real truck normals) | P2 | S | E1 | modeling-ops/revolve.rs | — |

---

## Dependency Graph

```
Phase A (Foundation)
  A1 (BooleanError) ─────┬──────────────────────> Phase B1, B4
  A2 (BooleanOptions) ───┤                        Phase B3, B5, E3
  A3 (Robust Preds) ─────┤──> A4 (RelationToOther) ──> B5, E3
                          │
Phase B (Hardening)       │
  B1 (Coplanar split) ───┤──> B3 (Box-cyl)
  B2 (difference/XOR) ───┤
  B4 (try_new enforce) ──┘

Phase C (Kernel Ops) — independent of A/B
  C1 (Chamfer) ──> C3 (Fillet)
  C2 (Shell)

Phase D (UI) — depends on C
  D1 (Edge select) ──> D2, D3
  D5 (Revolve preview) — independent

Phase E (Advanced) — depends on A, B
  E1 (GeomRef real) ──> E2 (Query selector), E6
  E5 (Property tests)
```

---

## Sprint History

### Sprint 1: Boolean Foundation (4 agents)

**Goal**: BooleanOptions, BooleanError, robust predicates, difference().

| Agent | Task | Burndown IDs | Status |
|-------|------|-------------|--------|
| tolerance-architect | `BooleanOptions` struct + layered tolerances | A2 | In Progress |
| error-engineer | `BooleanError` enum + `Result<>` propagation | A1 | In Progress |
| robust-predicates | Shewchuk predicates in ray-cast classification | A3 | In Progress |
| difference-impl | Proper `difference()` in truck-shapeops | B2 (partial) | In Progress |

**Merge order**: Agent 1 (additive) -> Agent 2 (new errors) -> Agent 3 (behavioral) -> Agent 4 (behavioral)

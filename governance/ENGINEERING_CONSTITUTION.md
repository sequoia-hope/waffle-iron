# Waffle Iron Engineering Constitution (v1)

This document defines **non-negotiable engineering rules** for Waffle Iron.  
It is **prescriptive**. It exists to prevent “CADslop” and to make autonomous agent work reliable.

- Architectural intent lives in `ARCHITECTURE.md` (descriptive).
- Engineering law lives in `/governance` (this file and related docs).
- Agent execution patterns live in `/agents`.

If there is a conflict, **this Constitution overrides all other project documentation** unless a human explicitly amends it.

---

## 1. Scope

This Constitution applies to all changes that affect any of the following:

- Modeling logic (feature engine, modeling ops, kernel, sketch solver integration)
- Geometry representation, BREP, topology, meshing, picking metadata
- Serialization/file format of CAD data
- WASM bridge/protocol between JS and WASM
- Any UI feature that changes modeling behavior or parameters (e.g., “direction toggle”)

UI-only changes that do not alter modeling behavior are subject to a lighter process, but must still include tests when logic branches are introduced.

---

## 2. Definitions

- **Feature**: A user-facing modeling operation or parameter that changes the model (extrude, revolve, fillet, boolean, draft, patterns, etc.).
- **Branch**: Any new conditional logic path (`if`, `match`, early return, option handling, mode flags) that can change behavior.
- **Spec**: A short, formal description of behavior including measurable invariants and branch space.
- **Oracle**: A check that can mechanically validate correctness (e.g., bbox, volume, centroid shift, face count, manifoldness).
- **Protocol**: The required workflow for feature work as defined in `/governance/FEATURE_IMPLEMENTATION_PROTOCOL.md`.

---

## 3. Non-Negotiable Principles

### P1 — Correctness is measurable
Every modeling feature must have at least one **numeric or structural oracle** (not “it runs,” not “it looks right”).  
Acceptable examples:

- Bounding box shift along sketch normal
- Centroid shift
- Volume for analytically predictable cases
- Cap plane offset sign/distance
- Face/edge/vertex counts for canonical primitives
- Manifoldness / watertight checks
- Deterministic mesh properties for stable inputs (within tolerances)

### P2 — Specs precede implementation
No modeling feature work begins without a spec file in `/specs/` describing:

- Parameters (inputs)
- Branch table (explicit enumeration of modes)
- Invariants and oracles
- Failure modes / expected errors

### P3 — Tests must fail before the fix
Feature work and bug fixes use red/green TDD: write a failing test (red), then implement until it passes (green).

### P4 — Branches are not allowed to be untested
If a change introduces a behavior branch, there must be tests exercising **every branch**.  
If this is not possible, the branch must be refactored away via parameter normalization or redesigned.

### P5 — No self-approval
The same agent (or role) must not:
- both author the tests **and** change the implementation in the same feature cycle.

(Sequentially, the same human can execute both, but agent teams must respect role separation.)

### P6 — Architecture must not be eroded
Agents may not introduce “convenient” cross-layer shortcuts. The layering described in `ARCHITECTURE.md` must remain intact. In particular:

- No kernel logic in JS.
- No UI dependencies in core modeling crates.
- WASM bridge must not leak kernel implementation types beyond the bridge boundary.

Architecture changes require explicit human approval (see §10).

### P7 — Small, auditable changes
Feature work must be decomposed into small increments that preserve passing tests at each step.
Large multi-subsystem “one-shot” merges are not allowed.

### P8 — Design decisions cite research
Kernel and algorithm design choices must reference published techniques from REFERENCES.md.
Specs must include a “Research Basis” section listing the references that informed the design.
Implementation comments must cite reference numbers for non-obvious algorithmic choices
(e.g., `// Ref #4: Shewchuk adaptive predicates for orient3d`).
Ad-hoc algorithmic invention is not acceptable when a published, peer-reviewed solution exists.

**Analytical primacy corollary**: When a closed-form algorithm exists for a geometric
computation on analytical surfaces, using a mesh approximation as the *final
representation* is a violation of P8. Closed-form SSI solutions exist for all
quadric surface pairs (plane, cylinder, cone, sphere, torus) [#1 Patrikalakis Ch.5,
#25 Yang et al.]. The kernel must implement these solvers.

**Hybrid boolean corollary**: Using meshes as an *exact computational intermediate*
to derive correct B-Rep topology is NOT a violation of P8 — it is the recommended
approach [#24 Yang et al. 2025]. The Yang hybrid pipeline uses exact mesh boolean
(indirect predicates, provably correct topology) as stage 2, then refines to
analytical SSI curves in stage 4. The mesh is never the final representation.
Tolerance-based heuristics (S-H clipping + progressive escalation + synthetic
mesh repair) are deprecated — they mask errors rather than solving them. See
ARCHITECTURAL_INVARIANTS.md A15.6.

---

## 4. Required Workflow for Modeling Features

All modeling features and modeling-affecting UI changes **must** follow:

- `/governance/FEATURE_IMPLEMENTATION_PROTOCOL.md`

At minimum, each feature cycle includes:

1. **Spec phase**: create `/specs/<feature>.md`
2. **Test phase**: write failing tests from spec
3. **Implementation phase**: implement without modifying tests
4. **Validation phase**: adversarial/edge cases + regression tests

---

## 5. Minimum Quality Bar for Specs

Each spec in `/specs/` must contain, at minimum:

- **Goal**: what the feature enables
- **Parameters**: all inputs and defaults
- **Branch table**: enumerate all modes and toggle combinations
- **Invariants**: formal statements of what must hold
- **Oracles**: how tests will check the invariants
- **Failure modes**: what errors are expected and how they are reported

Specs must be short and concrete. If the spec cannot be written clearly, the feature is not ready.

---

## 6. Minimum Quality Bar for Tests

Tests for modeling features must include:

- At least one **canonical** case with analytically predictable behavior.
- At least one **edge** case (small distances, degenerate profiles, coincident geometry, etc.).
- At least one **regression** test for any bug fix.
- Assertions that are **numerical/structural**, not only “no panic” / “no error”.

When a feature introduces a mode toggle (e.g., “opposite direction”), tests must directly assert that the mode materially changes results (e.g., bbox.z changes sign).

---

## 7. Parameter Normalization Rule (Branch Control)

To prevent branching complexity explosion:

- Feature parameters must be normalized into a **canonical representation** early.
- Downstream code should operate on canonical parameters and avoid mode branching.

Example:
- `Direction::{Forward, Opposite}` must become `signed_distance = ±abs(distance)` in one place.
- No additional direction checks are allowed after normalization.

If a feature can be represented without a branch, do so.

---

## 8. Determinism and Reproducibility

Model rebuild and testing must be deterministic for the same inputs:

- No nondeterministic ordering in topology traversal that affects IDs or results.
- No random seeds unless explicitly controlled and recorded.
- Mesh generation must be stable given fixed tolerances and inputs.

Tests must set or assume deterministic settings.

---

## 9. “Slop Stops Here” Rule

If a feature “kind of works” but breaks obvious branches or invariants, it is not acceptable.

- Missing branch tests is considered a **broken feature**, even if the demo path works.
- “Looks correct in the viewport” is not acceptance criteria.

---

## 10. Fix It Right or Don't Fix It

### P9 — No hack-to-green

**If you cannot explain why a test fails, you may not change code to make it pass.** Document the failure in PLAN.md and move to the next task.

Fixes must address the root cause in the layer where the defect lives. Workarounds that make tests pass without fixing the underlying problem — tolerance widening, special-case branches, fallback paths that produce right answers for wrong reasons — are prohibited and will be reverted. An assay regression is acceptable only when a unit test proves the new code is more correct and recovery is scoped in PLAN.md.

### P10 — Plan says what to fix; agent says when to stop

The plan is where root-cause analysis and architectural reasoning happen. If a plan's diagnosis turns out to be wrong, the agent **must abort that fix and report what it learned** — not improvise an alternative. Unplanned fixes bypass the review that planning provides and tend to produce hacks. Plans are cheap; reverting hacks is expensive.

---

## 11. Amendments and Protected Files

The following are **protected** and may only be changed with explicit human approval:

- `ARCHITECTURE.md`
- `/governance/*`
- `/agents/*` (once established)

Agents may propose edits, but cannot merge them as part of normal feature work.

Amending this Constitution requires:
- A PR titled “Amend Constitution: <summary>”
- A clear rationale and migration plan

---

## 12. Enforcement Roadmap (v1)

This Constitution is intended to be enforced mechanically over time.

Initial enforcement expectations:
- Specs required for modeling features
- Tests-first required for bug fixes and new branches
- Role separation respected by agent teams

Planned enforcement additions (may be introduced incrementally):
- Diff coverage gate
- Branch coverage gate
- Mutation sanity checks for toggles/branches
- Geometry health suite (manifoldness, watertightness, NaN checks)
- Golden scene render + pixel diff for geometry-affecting changes

---

## 13. Interpretation Rule

When ambiguity exists, choose the interpretation that is **more testable** and **more robust**.

If a requirement cannot be tested, it must be rewritten until it can.

---

## 14. Quick Checklist (for humans and agents)

A feature is not “done” unless:

- [ ] `/specs/<feature>.md` exists and enumerates all modes/branches
- [ ] Red/green TDD followed: tests fail before implementation, pass after
- [ ] Every new branch is executed by at least one test
- [ ] Tests assert numeric/structural oracles (not only “no error”)
- [ ] Parameters are normalized early to reduce branching
- [ ] Architecture boundaries remain intact
- [ ] No workarounds to make tests pass — root cause addressed or fix aborted (P9–P10)

---


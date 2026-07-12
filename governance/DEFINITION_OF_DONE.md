# Definition of Done (DoD) — v1

This document defines the final acceptance criteria for any change in Waffle Iron.

No work is considered complete unless all applicable criteria below are satisfied.

This file operationalizes:

- ENGINEERING_CONSTITUTION.md
- FEATURE_IMPLEMENTATION_PROTOCOL.md (FIP)
- ARCHITECTURAL_INVARIANTS.md

If ambiguity exists, the stricter interpretation applies.

---

# 0. Change Classification

Every change must be classified by the Manager as one of:

1. Modeling Feature
2. Bug Fix (modeling-related)
3. Refactor
4. UI-Only Change
5. Documentation Change
6. Infrastructure / Tooling Change

The applicable sections below must be satisfied before completion.

---

# 1. Modeling Feature — Done Criteria

A modeling feature is complete only if ALL of the following are satisfied.

---

## 1.1 Specification

*(Yang-increment variant, amended 2026-07-12: for Yang hybrid-boolean
increments, the spec artifact is the roadmap milestone entry + a mandatory
`docs/yang_deviations.md` entry — see ENGINEERING_CONSTITUTION.md P2
clarification. The deviations entry is a merge blocker. All other checklist
items below still apply.)*

- [ ] A spec file exists in `/specs/<feature>.md`
- [ ] Spec includes:
  - [ ] Goal
  - [ ] Parameters (all inputs enumerated)
  - [ ] Branch table (all modes/toggles explicitly listed)
  - [ ] Invariants (formal behavioral rules)
  - [ ] Oracles (measurable validation rules)
  - [ ] Failure modes
- [ ] Research Basis section lists applicable references from REFERENCES.md
- [ ] Non-obvious algorithmic choices cite reference numbers in code comments
- [ ] No undocumented branches exist in implementation
- [ ] Spec matches final behavior (no divergence)

---

## 1.2 Test Requirements

- [ ] Red/green TDD followed: tests failed before implementation, pass after
- [ ] Every branch in the branch table is exercised by at least one test
- [ ] Tests include numeric or structural assertions (not only "no panic")
- [ ] At least one canonical case exists
- [ ] At least one edge or degenerate case exists
- [ ] No test was weakened to make the feature pass

Unacceptable tests:
- Only checking that the function returns without error
- Only snapshotting mesh buffers without invariant validation
- Tests that do not distinguish between alternate branch behaviors

---

## 1.3 Invariant Validation

- [ ] Invariants from spec are explicitly validated by tests
- [ ] Bounding box, centroid, volume, or structural checks are used where applicable
- [ ] Toggle or direction changes materially alter geometry and are verified
- [ ] Deterministic behavior is preserved

If a branch can be inverted and tests still pass, the feature is not done.

---

## 1.4 Implementation Integrity

- [ ] Implementation did not modify tests written in the same cycle
- [ ] Parameters are normalized early to reduce branching
- [ ] No redundant downstream branching exists
- [ ] Code comments reference key invariants where relevant
- [ ] No architecture boundary violations (per ARCHITECTURAL_INVARIANTS.md)
- [ ] No new global mutable state introduced

---

## 1.5 Adversarial Validation

- [ ] Pathological or near-tolerance inputs tested
- [ ] Degenerate geometry behavior validated (if relevant)
- [ ] No NaN values introduced
- [ ] No invalid topology produced
- [ ] No regression introduced in existing test suite

If geometry health checks exist, they must pass.

---

## 1.6 CI and Quality Gates

- [ ] All tests pass in CI
- [ ] No new warnings introduced
- [ ] Coverage thresholds (if enabled) are satisfied
- [ ] No uncovered branches introduced without justification

---

# 2. Bug Fix — Done Criteria

A bug fix is complete only if:

- [ ] A failing reproduction test was added first
- [ ] The reproduction test fails on prior code
- [ ] The fix makes the test pass
- [ ] A regression test exists (if not identical to reproduction)
- [ ] All other tests still pass
- [ ] No new undocumented branches introduced

If the bug involves modeling logic, invariant validation is required.

---

# 3. Refactor — Done Criteria

A refactor is complete only if:

- [ ] No behavior changes unless explicitly specified
- [ ] All existing tests pass unchanged
- [ ] No new undocumented branches introduced
- [ ] Architecture boundaries preserved
- [ ] Parameter normalization is not degraded
- [ ] Determinism preserved

If behavior changes, full Modeling Feature criteria apply.

---

# 4. UI-Only Change — Done Criteria

If modeling behavior is unaffected:

- [ ] No modeling crates modified
- [ ] No modeling parameters altered
- [ ] Any new UI branches are tested
- [ ] No architecture boundary violations introduced

If UI introduces or modifies modeling parameters, full Modeling Feature criteria apply.

---

# 5. Documentation Change — Done Criteria

- [ ] Documentation accurately reflects current behavior
- [ ] Governance documents modified only with explicit rationale
- [ ] Architecture documents modified only with explicit human approval

---

# 6. Infrastructure / Tooling Change — Done Criteria

- [ ] Does not alter modeling behavior unintentionally
- [ ] Tests still pass
- [ ] No silent change in determinism
- [ ] Build remains reproducible

If infrastructure affects modeling behavior (e.g., tolerance changes), full Modeling Feature criteria apply.

---

# 7. Manager Authorization Rule

Manager may declare work complete only if:

- All applicable checklists are satisfied
- No known unresolved branch or edge case exists
- Spec matches implementation
- Tests meaningfully validate invariants
- Architecture remains intact

If uncertain, the feature is NOT done.

---

# 8. Prohibited Completion States

Work must NOT be marked complete if:

- It “kind of works” in a demo path
- Alternate branches are untested
- Invariants are assumed but not validated
- Architecture layering was bypassed
- Tests were weakened to pass
- Behavior differs from spec without spec update

---

# 9. Amendment Policy

Changes to this file require:

- PR titled: "Amend Definition of Done"
- Clear reasoning
- Confirmation alignment with ENGINEERING_CONSTITUTION.md

---

End of Definition of Done (v1)


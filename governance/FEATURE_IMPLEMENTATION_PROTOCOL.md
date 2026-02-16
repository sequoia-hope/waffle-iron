# Feature Implementation Protocol (FIP) — v1

This document defines the mandatory lifecycle for any modeling feature
or modeling-affecting change in Waffle Iron.

This protocol operationalizes the rules in:

- ENGINEERING_CONSTITUTION.md
- ARCHITECTURAL_INVARIANTS.md

No modeling feature may bypass this protocol.

---

## 0. Applicability

This protocol is REQUIRED for:

- New modeling features (extrude modes, booleans, fillets, patterns, etc.)
- New parameters or toggles on existing features
- Behavioral changes to existing modeling logic
- Bug fixes that affect geometry behavior
- Modeling-affecting UI changes (e.g. a direction toggle)

It is NOT required for:

- Pure documentation updates
- Non-modeling UI layout tweaks
- Build system changes (unless they affect modeling behavior)

---

## 1. Roles and Separation of Responsibility

Each feature cycle must involve the following roles:

- Manager
- Spec Writer
- Test Author
- Implementer
- Adversary

The same agent may not act as both Test Author AND Implementer within a single feature cycle.

Manager orchestrates but does not implement modeling code.

---

## 2. Phase Overview

Every feature must pass through the following phases:

1. Spec Phase
2. Test Phase (failing tests required)
3. Implementation Phase
4. Validation Phase (adversarial + regression)
5. Merge Authorization

Each phase produces artifacts.

---

## 3. Phase 1 — Spec Phase

### 3.1 Trigger

Manager receives a natural-language request.

Example:

> Add symmetric extrude mode

Manager must create:

- `/specs/<feature_name>.md`

Example:

- `/specs/symmetric_extrude.md`

### 3.2 Spec Requirements

The spec file MUST contain the following sections:

#### 1. Goal

Clear description of user-visible behavior.

#### 2. Parameters

List:

- All inputs
- Default values
- Units
- Valid ranges
- Error conditions

#### 3. Branch Table

Explicit enumeration of all behavioral branches.

Example:

| Mode | Distance | Direction | Expected Behavior |
|------|----------|-----------|------------------|

If a toggle is introduced, it must appear in the table. No implicit modes allowed.

#### 4. Invariants

Formal statements such as:

- Cap plane offset = signed_distance along sketch normal
- Volume = area(profile) * |distance| (prism case)
- Base face remains coplanar with sketch plane
- Bounding box delta aligns with sketch normal

These must be measurable.

#### 5. Oracles

Concrete mechanisms for test validation:

- Bounding box assertions
- Centroid offset assertions
- Volume tolerance bounds
- Face count
- Manifoldness checks

#### 6. Failure Modes

Define:

- Invalid inputs
- Degenerate geometry handling
- Expected error type or behavior

### 3.3 Exit Criteria for Spec Phase

Spec is complete when:

- All parameters enumerated
- All branches enumerated
- At least one numeric oracle defined per branch
- Failure modes listed

Implementation cannot begin before this.

---

## 4. Phase 2 — Test Phase

Test Author executes this phase.

### 4.1 Constraints

- May not modify implementation code.
- Must cause at least one failing test on current code.

### 4.2 Test Requirements

Each feature must include:

1. Canonical Case
   - Simple geometry
   - Analytically predictable result

2. Branch Coverage
   - At least one test per branch in branch table

3. Edge Case
   - Small distance
   - Zero distance (if allowed)
   - Degenerate input (if relevant)

4. Regression Case (for bug fixes)

### 4.3 Required Assertions

Tests must include numeric or structural assertions such as:

- Bounding box equality within tolerance
- Volume within tolerance
- Face count
- Orientation of cap face normal
- Manifoldness validation

Tests that only check "no panic" are insufficient.

### 4.4 Failing Demonstration

Before implementation begins, Test Author must demonstrate:

- Tests fail on current code, OR
- Coverage report shows new branches unexecuted

This failure must be documented in PR or logs.

### 4.5 Exit Criteria for Test Phase

- All new branches have associated tests
- At least one test fails prior to implementation

Only then may implementation begin.

---

## 5. Phase 3 — Implementation Phase

Implementer executes this phase.

### 5.1 Constraints

- May not modify test files written in this cycle.
- May not modify spec file except for typo clarification.
- Must not introduce undocumented branches.
- Must normalize parameters early (see Constitution §7).

### 5.2 Required Practices

1. Parameter normalization must occur before core logic.
2. No repeated branch checks downstream.
3. Invariant references must appear in code comments.

Example comment:

- `// Invariant: cap plane offset must equal signed_distance along sketch normal.`

### 5.3 Exit Criteria

- All tests pass.
- No new uncovered branches introduced.
- No architecture boundary violations.

---

## 6. Phase 4 — Validation Phase (Adversarial)

Adversary executes this phase.

### 6.1 Responsibilities

Add:

- Pathological inputs
- Near-tolerance values
- Coincident geometry
- Extreme magnitudes

### 6.2 Geometry Health Checks

Must run:

- Manifoldness check
- No NaN coordinates
- No zero-area faces (unless expected)
- No duplicate coincident faces

If any fail, feature is not complete.

### 6.3 Mutation Sanity Check (Optional but Recommended)

For branch-based features:

- Temporarily invert a branch or sign.
- If tests still pass, tests are insufficient and must be strengthened.

### 6.4 Exit Criteria

- All adversarial tests pass.
- No regression introduced in existing test suite.

---

## 7. Phase 5 — Merge Authorization

Manager verifies:

- Spec exists
- Tests cover all branches
- Tests were failing prior to implementation
- Implementation did not modify tests
- Validation phase completed
- CI passes

Only then may merge occur.

---

## 8. Bug Fix Variant of Protocol

For bug fixes:

1. Reproduce bug with failing test first.
2. Confirm failing test.
3. Implement fix.
4. Add regression test.
5. Run adversarial validation.

Bug fixes may not skip test-first requirement.

---

## 9. UI Toggle Variant

If UI introduces a new modeling parameter:

- Full protocol required.
- UI-only logic may be tested separately, but modeling effect must follow modeling protocol.

---

## 10. Emergency Override

In rare cases (e.g., broken main branch):

- Temporary direct fix allowed.
- Must be followed by retroactive spec + tests within next commit.

No permanent bypass allowed.

---

## 11. Definition of Done

The full acceptance criteria for feature completion are defined in `/governance/DEFINITION_OF_DONE.md`. Manager may not declare work complete unless all applicable DoD criteria are satisfied.


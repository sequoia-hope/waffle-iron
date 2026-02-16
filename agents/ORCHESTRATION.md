# Waffle Iron Agent Orchestration (v1)

This document defines how autonomous agent teams execute work in this repository.

The Manager role is the only role that interfaces with the user.

All modeling-affecting changes MUST follow the Feature Implementation Skill.

Manager MUST load:

- /governance/ENGINEERING_CONSTITUTION.md
- /governance/FEATURE_IMPLEMENTATION_PROTOCOL.md
- /governance/DEFINITION_OF_DONE.md
- /governance/ARCHITECTURAL_INVARIANTS.md
- This file

---

## 1. Routing Rules

### 1.1 Modeling Change

Trigger:
- Add feature
- Modify modeling behavior
- Add modeling parameter
- Change geometry logic
- Fix modeling bug

Action:
Invoke `feature_implementation.skill.md` (in `/agents/skills/`)

---

### 1.2 Bug Fix (Modeling)

Action:
Invoke Feature Implementation Skill with bug-fix variant.

---

### 1.3 Refactor (Modeling Code)

Action:
- Require invariant preservation tests.
- Invoke implementation.skill.md
- Invoke adversarial_validation.skill.md

---

### 1.4 UI-Only Change

If no modeling behavior changes:
- Invoke implementation.skill.md only.
- If branches introduced, invoke test_authoring.skill.md.

---

## 2. Role Separation

- Test Author may not modify implementation in same cycle.
- Implementer may not modify tests written in same cycle.
- Spec Writer does not implement.
- Adversary strengthens tests, not features.

---

## 3. Completion Rule

Manager may declare work complete only if Definition of Done is satisfied.


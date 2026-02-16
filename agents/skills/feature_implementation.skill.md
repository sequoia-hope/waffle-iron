# Skill: Feature Implementation (v1)

Purpose:
Execute full Feature Implementation Protocol (FIP).

Steps:

1. Create or update `/specs/<feature>.md`.
2. Invoke Spec Generation Skill.
3. Invoke Test Authoring Skill.
4. Confirm failing tests exist.
5. Invoke Implementation Skill.
6. Invoke Coverage Enforcement Skill.
7. Invoke Adversarial Validation Skill.
8. Invoke Golden Scene Validation Skill (if geometry changes).
9. Confirm Definition of Done.
10. Produce final summary.

Refuse to proceed if:
- Spec missing.
- Tests do not fail first.
- Branch table incomplete.
- Architecture violation detected.


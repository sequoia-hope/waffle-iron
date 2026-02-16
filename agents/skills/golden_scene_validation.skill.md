# Skill: Golden Scene Validation (v1)

Purpose:
Detect visual regressions in geometry output.

Actions:

- Render canonical deterministic scenes.
- Compare against baseline images.
- Fail if difference exceeds tolerance.

Only invoked for geometry-affecting changes.


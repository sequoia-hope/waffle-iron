# Spec: Sketch Profile Extraction on Rebuild

**Type**: Bug Fix
**Affected Crates**: `waffle-types`, `sketch-solver`, `feature-engine`

## Goal

When a `.waffle` file is loaded and the feature tree is rebuilt, sketches must have
their `solved_positions` and `solved_profiles` recomputed from their entities. Currently
these fields are `#[serde(skip_serializing)]` and documented as "recomputed on load",
but the recomputation step is missing.

This causes every Extrude/Revolve that references a non-gear sketch to fail with
"profile index 0 out of range (sketch has 0 profiles)".

## Parameters

None — this is a pipeline fix, not a user-facing feature.

## Branch Table

| Branch | Condition | Expected Behavior |
|--------|-----------|-------------------|
| B1 | Sketch has Point + Line entities forming closed loop | `solved_positions` populated from Point x,y; profile extracted |
| B2 | Sketch has Circle entity | Profile extracted with single circle entity |
| B3 | Sketch has only construction entities | No profiles extracted (empty) |
| B4 | Sketch has no entities | No profiles, no crash |
| B5 | Sketch has Gear entities | `expand_gears()` handles this (existing path) |
| B6 | Sketch has Arc entities in profile | Profile extracted including arcs |
| B7 | Sketch with multiple closed loops | Multiple profiles extracted |
| B8 | Sketch already has solved_profiles (e.g. from interactive session) | Profiles preserved, not re-extracted |

## Invariants

1. After rebuild, every sketch referenced by an Extrude/Revolve must have
   `solved_profiles.len() > 0` if the sketch has non-construction entities
   forming at least one closed loop.
2. Positions built from Point entities must match the entity's stored x,y values.
3. Profile extraction must produce the same results as the interactive path
   (`sketch-solver::extract_profiles`).

## Oracles

- **Rectangle sketch → extrude**: Must produce 1 profile with 4 entity_ids.
- **Circle sketch → extrude**: Must produce 1 profile with 1 entity_id.
- **Empty sketch → extrude**: Must fail with ProfileOutOfRange, not panic.
- **Assay score**: Should improve from 1/160 to significantly higher.

## Failure Modes

- If Point entities have degenerate coordinates (NaN, inf), positions should
  still be built but profile extraction may find 0 profiles. No panic.
- If entities reference point IDs that don't exist as Point entities,
  profile extraction handles missing positions gracefully (returns 0 profiles).

## Research Basis

Profile extraction algorithm (minimal face detection on planar graphs) is
documented in `sketch-solver/src/profiles.rs`. Uses angle-sorted half-edge
traversal per [#16] Mäntylä's half-edge principles.

## Implementation Approach

1. Move `extract_profiles` and its helper types/functions from `sketch-solver`
   to `waffle-types` (it has zero external dependencies beyond std + waffle-types).
2. Add `Sketch::recompute_derived_data()` method that builds `solved_positions`
   from Point entities and calls `extract_profiles()`.
3. In `feature-engine/src/rebuild.rs`, call `recompute_derived_data()` on
   sketches with empty `solved_profiles` and non-empty `entities` before
   processing Extrude/Revolve operations.
4. Update `sketch-solver` to re-export from `waffle-types` for backward compat.

## A15 Compliance

Not applicable — this fix does not modify geometry or boolean operations.
It only wires up existing profile detection to the rebuild pipeline.

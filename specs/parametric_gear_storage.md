# Parametric Gear Storage

## Goal

Store gear profiles compactly as `GearParams` in `.waffle` files instead of expanding
to hundreds of primitive sketch entities. Expand on demand during rebuild and rendering.

## Parameters

- `GearParams`: `{tooth_count, module, pressure_angle_deg, backlash, center_x, center_y, rotation_offset}`
- Expansion via `generate_gear_profile(params)` — deterministic for same inputs

## Branch Table

| Sketch contains | Serialized form | On rebuild |
|----------------|-----------------|------------|
| No gears | entities only (unchanged) | No expansion needed |
| Gear entity | `{"type":"Gear","id":N,"params":{...}}` | `expand_gears()` called |
| Gear + other entities | Mixed: Gear + primitives | Gear expanded, primitives untouched |
| Old format (expanded) | Primitives + solved_positions | Loads normally (backward compat) |

## Invariants

1. `expand_gears()` produces identical entities/positions/profiles to direct `generate_gear_profile()` call
2. Serialized gear entity is <200 bytes (vs 50-150KB expanded)
3. `solved_positions` and `solved_profiles` are not serialized (derived data)
4. Old .waffle files with expanded gear entities still load correctly
5. Gear expansion is deterministic for same GearParams

## Oracles

- Round-trip test: serialize(gear_sketch) → deserialize → expand → compare profiles
- Size test: gear sketch serialized size < 1KB per gear
- Feature engine: extrude/revolve on gear sketch produces same mesh before/after change

## Failure Modes

- `expand_gears()` called on sketch with no gears: no-op (not an error)
- Invalid GearParams (0 teeth, negative module): `generate_gear_profile` handles gracefully

## Research Basis

Involute gear geometry: standard mechanical engineering (Litvin & Fuentes, "Gear Geometry and Applied Theory").
B-spline fitting of involute curves: `fit_bspline_to_points()` in waffle-types/src/bspline.rs.

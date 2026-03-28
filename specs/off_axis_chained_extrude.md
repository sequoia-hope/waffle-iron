# Off-Axis Chained Extrudes (F0076-F0085)

## Purpose

Test near-coplanar boolean faces that stress the SSI solver. Each step tilts the
extrusion normal by 0-5° from the previous step, creating a chain of boss extrudes
whose faces are nearly but not exactly parallel.

## Algorithm

### Normal rotation (Rodrigues' formula)

Given current normal **n**, generate a random rotation:

1. Pick a random axis **k** perpendicular to **n** (rejection-sample until `|k × n| > ε`)
2. Pick angle θ ∈ [0, max_angle_deg] (uniform)
3. Apply Rodrigues' rotation:
   ```
   v_rot = v·cos(θ) + (k × v)·sin(θ) + k·(k · v)·(1 - cos(θ))
   ```
4. Re-normalize to unit length

### Origin advancement

Each step advances the origin along the *current* (tilted) normal:
```
origin_{i+1} = origin_i + normal_i * depth_i
```

This creates lateral drift proportional to chain length and tilt angle.

### Profile shapes

Same 7 shape types as axis-aligned chained extrudes (L, T, notch, plus, rect, circle, gear).

## Cases

| ID    | Chain length | Seed  |
|-------|-------------|-------|
| F0076 | 5           | 20001 |
| F0077 | 5           | 20002 |
| F0078 | 8           | 20003 |
| F0079 | 8           | 20004 |
| F0080 | 10          | 20005 |
| F0081 | 12          | 20006 |
| F0082 | 15          | 20007 |
| F0083 | 15          | 20008 |
| F0084 | 20          | 20009 |
| F0085 | 20          | 20010 |

## Oracle expectations

- `euler_target`: 2 (genus-0, all boss — no cuts)
- `expect_watertight`: true
- `max_bbox_extent`: `4.0 + chain_length * 0.5` (extra margin for lateral drift)
- `volume_monotonicity`: all "increase"
- `expect_rebuild_error`: false

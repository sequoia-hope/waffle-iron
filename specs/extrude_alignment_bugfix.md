# Spec: Extrude Alignment Bug Fix

## Goal

`tangent_x_from_normal()` in Rust (`crates/feature-engine/src/rebuild.rs`) must produce
an identical coordinate frame to `buildSketchPlane()` in JS (`app/src/lib/sketch/sketchCoords.js`).

Both functions use the same algorithm:
```
ref = |n.z| < 0.99 ? Z : X    (Z=[0,0,1], X=[1,0,0])
xAxis = ref x n               (cross product)
```

## Branch Table

| Normal        | `|n.z| < 0.99`? | ref_vec     | xAxis = ref x n | Notes          |
|---------------|------------------|-------------|------------------|----------------|
| `[0,0,1]` XY  | No (1.0 >= 0.99) | `[1,0,0]`  | `[0,-1,0]`       | Standard XY    |
| `[0,0,-1]`    | No (1.0 >= 0.99) | `[1,0,0]`  | `[0,+1,0]`       | Flipped XY     |
| `[0,1,0]` XZ  | Yes (0.0 < 0.99) | `[0,0,1]`  | `[-1,0,0]`       | Standard XZ    |
| `[1,0,0]` YZ  | Yes (0.0 < 0.99) | `[0,0,1]`  | `[0,+1,0]`       | Standard YZ    |
| `[0,-1,0]`    | Yes (0.0 < 0.99) | `[0,0,1]`  | `[+1,0,0]`       | Flipped XZ     |
| `[-1,0,0]`    | Yes (0.0 < 0.99) | `[0,0,1]`  | `[0,-1,0]`       | Flipped YZ     |

## Invariants

1. **Perpendicularity**: `xAxis . n == 0` (within 1e-10)
2. **Unit length**: `|xAxis| == 1` (within 1e-10)
3. **Rust == JS**: Per branch table row, Rust result matches JS result exactly

## Bug Description

The old `tangent_x_from_normal()` used a different formula that produced `[0,+1,0]` for
`n=[0,0,1]` instead of `[0,-1,0]`. This caused sketch coordinates to map to the wrong
world-space axes when extruding, producing misaligned geometry.

## Oracle

- **Rust unit test**: Direct arithmetic assertion on each branch table row
- **E2E bbox test**: Sketch rectangle at known coordinates -> extrude -> verify world-space
  bounding box coordinates match expected mapping through the xAxis

# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-21
Score: **93/160** (93 pass, 60 fail, 7 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| pass-boss-only | 56 | passed |
| pass-genuine | 37 | passed |
| boolean-watertight | 35 | failed |
| mesh-too-simple | 9 | failed |
| cascading-failure | 7 | errored |
| multiple-failures | 6 | failed |
| revolve-normals | 5 | failed |
| auto-union-failed | 2 | failed |
| volume-magnitude | 1 | failed |
| tessellation-degenerate | 1 | failed |
| aabb-collapse | 1 | failed |

## Highest-Leverage Fixes

1. **Fix boolean-watertight** → would address ~35 cases
2. **Fix mesh-too-simple** → would address ~9 cases
3. **Fix cascading-failure** → would address ~7 cases
4. **Fix multiple-failures** → would address ~6 cases
5. **Fix revolve-normals** → would address ~5 cases
6. **Fix auto-union-failed** → would address ~2 cases
7. **Fix volume-magnitude** → would address ~1 cases
8. **Fix tessellation-degenerate** → would address ~1 cases
9. **Fix aabb-collapse** → would address ~1 cases

## Individual Case Results

### R0001 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 4.54e-1 (log: -0.34)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0002 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.30e0 (log: 0.36)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 66 unpaired edges out of 705 total

### R0003 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.16e2 (log: 2.33)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 127 unpaired edges out of 1892 total

### R0004 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 1161 total

### R0005 — PASS

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + revolve(gear,cut)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0006 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,boss)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0007 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 96 for operations [("extrude", "circle"), ("extrude", "rectangle"), ("extrude", "gear")]

### R0008 — PASS

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0009 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0010 — PASS

- **Operations**: extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0011 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 66 unpaired edges out of 3051 total

### R0012 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): bc9220a7-b035-4902-9842-67f2cecad33c: operation error: kernel error: operation not supported: polygon boolean: 310x590 effective face product (5790) too large for non-convex solids

### R0013 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 7.25e1 (log: 1.86)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0014 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.28e3 (log: 3.11)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0015 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss) + revolve(circle,boss)
- **Scale**: 1.12e-4 (log: -3.95)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 376 unpaired edges out of 1286 total (369 boundary, 7 non-manifold); no_degenerate_triangles: 1 of 739 triangles are degenerate

### R0016 — ERROR

- **Operations**: extrude(gear,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0017 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 12 of 402 triangles have reversed normals

### R0018 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 288 for operations [("extrude", "rectangle"), ("extrude", "gear"), ("revolve", "gear")]

### R0019 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 120 triangles < expected minimum 288 for operations [("extrude", "circle"), ("revolve", "gear")]

### R0020 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 74 unpaired edges out of 703 total

### R0021 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 2.73e-1 (log: -0.56)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0022 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.89e-1 (log: -0.54)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0023 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.06e3 (log: 3.03)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0024 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.92e1 (log: 1.28)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0025 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + revolve(circle,cut)
- **Scale**: 2.22e3 (log: 3.35)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0026 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 33 unpaired edges out of 1764 total

### R0027 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(circle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("extrude", "gear"), ("revolve", "circle")]

### R0028 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0030 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0031 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 471 unpaired edges out of 5786 total (459 boundary, 12 non-manifold)

### R0032 — PASS

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0033 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 4 unpaired edges out of 581 total

### R0034 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,boss)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 8 unpaired edges out of 2476 total

### R0036 — PASS

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0037 — PASS

- **Operations**: extrude(gear,boss) + revolve(gear,cut) + extrude(circle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1 unpaired edges out of 605 total (0 boundary, 1 non-manifold)

### R0039 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0040 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.58e1 (log: 1.41)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 66 unpaired edges out of 1488 total; consistent_normals: 10 of 970 triangles have reversed normals

### R0041 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.13e0 (log: 0.05)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0042 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 8.59e2 (log: 2.93)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0043 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0044 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 3807 total

### R0045 — FAIL

- **Operations**: revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: volume-magnitude
- **Detail**: volume_magnitude: volume 5.519793e-17 outside [1.174499e-15, 1.174499e1] for scale 4.897234e-3

### R0046 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 151 unpaired edges out of 3218 total

### R0047 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut) + revolve(circle,cut)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 288 for operations [("extrude", "rectangle"), ("revolve", "gear"), ("revolve", "circle")]

### R0048 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("extrude", "gear")]

### R0049 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 4 unpaired edges out of 581 total

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 240 unpaired edges out of 2838 total (236 boundary, 4 non-manifold)

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 373 unpaired edges out of 16059 total (179 boundary, 194 non-manifold); consistent_normals: 262 of 11031 triangles have reversed normals

### R0052 — PASS

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 738 unpaired edges out of 9393 total

### R0054 — ERROR

- **Operations**: extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0055 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0056 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 126 unpaired edges out of 3690 total

### R0057 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0058 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 366x394 effective face product (15224) too large for non-convex solids. Body created as standalone.; partial rebuild (1 error(s)): 2d4513b2-1d7e-4018-ae1d-6bc19a9c035a: operation error: kernel error: operation not supported: polygon boolean: 366x506 effective face product (9145) too large for non-convex solids; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 90 unpaired edges out of 3801 total

### R0060 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 10 unpaired edges out of 230 total

### R0061 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,boss)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0062 — PASS

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0063 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0064 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0065 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + revolve(circle,cut)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: multiple-failures
- **Detail**: bbox diagonal 2.763e-2 exceeds max 2.617e-2

### R0066 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 1.19e0 (log: 0.08)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0067 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.24e-1 (log: -0.91)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0068 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 4.84e-2 (log: -1.31)
- **Category**: multiple-failures
- **Detail**: bbox diagonal 1.814e-1 exceeds max 1.453e-1

### R0069 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 10 unpaired edges out of 599 total

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 4 unpaired edges out of 581 total

### R0071 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,cut)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1368 unpaired edges out of 4827 total

### R0072 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0073 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0074 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: multiple-failures
- **Detail**: bbox diagonal 1.185e0 exceeds max 1.020e0

### R0075 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): bf39bb13-caea-4782-a987-73d7310c7c75: operation error: kernel error: operation not supported: polygon boolean: 680x674 effective face product (12657) too large for non-convex solids; watertight_mesh: 32 unpaired edges out of 4009 total

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 17 unpaired edges out of 1565 total (6 boundary, 11 non-manifold)

### R0077 — PASS

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0078 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0079 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0080 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 288 for operations [("extrude", "rectangle"), ("revolve", "gear")]

### R0081 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 602x588 effective face product (41828) too large for non-convex solids. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); volume_magnitude: volume 1.483912e-11 outside [1.128408e-10, 1.128408e6] for scale 2.242963e-1

### R0082 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: multiple-failures
- **Detail**: bbox diagonal 1.972e3 exceeds max 1.921e3

### R0083 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0084 — PASS

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0085 — ERROR

- **Operations**: extrude(gear,boss) + revolve(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0086 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0087 — PASS

- **Operations**: revolve(circle,boss) + extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0088 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 96 for operations [("extrude", "circle"), ("extrude", "gear"), ("extrude", "rectangle")]

### R0089 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0090 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0091 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: multiple-failures
- **Detail**: bbox diagonal 4.946e-4 exceeds max 4.771e-4

### R0092 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "circle")]

### R0093 — PASS

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0094 — PASS

- **Operations**: revolve(gear,boss) + revolve(circle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0095 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0096 — PASS

- **Operations**: revolve(circle,boss) + revolve(circle,cut)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0097 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0098 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0099 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 64 unpaired edges out of 18260 total (0 boundary, 64 non-manifold)

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,cut) + revolve(rectangle,boss)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 70 unpaired edges out of 1286 total

### F0001 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0002 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e-2 (log: -2.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0003 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e2 (log: 2.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0004 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0005 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0006 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e-3 (log: -3.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0007 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e1 (log: 1.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0008 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0009 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e-1 (log: -1.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0010 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0011 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0012 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0013 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0014 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0015 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0016 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0017 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0018 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0019 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0020 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0021 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0022 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0023 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0024 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0025 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0026 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0027 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0028 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0030 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0031 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0032 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0033 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0034 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0035 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0036 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0037 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0038 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0039 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0040 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0041 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0042 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0043 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0044 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 4 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "circle")]; volume_magnitude: volume 3.140986e-11 outside [1.000000e-8, 1.000000e8] for scale 1.000000e0

### F0045 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: aabb-collapse
- **Detail**: aabb_collapse: all 146 unique vertices lie on AABB faces — mesh collapsed to bounding box

### F0046 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0047 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0048 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0049 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 130 unpaired edges out of 124330 total (0 boundary, 130 non-manifold)

### F0050 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0051 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e-4 (log: -4.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0052 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e-4 (log: -4.00)
- **Category**: tessellation-degenerate
- **Detail**: no_degenerate_triangles: 64 of 892 triangles are degenerate

### F0053 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0054 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0055 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 2130 total

### F0056 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 94 unpaired edges out of 275 total

### F0057 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 33 unpaired edges out of 330 total; consistent_normals: 6 of 209 triangles have reversed normals

### F0058 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 73 unpaired edges out of 284 total

### F0059 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 59 unpaired edges out of 316 total

### F0060 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 9 unpaired edges out of 372 total; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); volume_magnitude: volume 0.000000e0 outside [1.000000e-8, 1.000000e8] for scale 1.000000e0

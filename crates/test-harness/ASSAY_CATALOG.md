# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-19
Score: **35/160** (35 pass, 93 fail, 32 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| mesh-too-simple | 40 | failed |
| pass-boss-only | 32 | passed |
| cascading-failure | 32 | errored |
| multiple-failures | 25 | failed |
| boolean-watertight | 21 | failed |
| pass-genuine | 3 | passed |
| tessellation-degenerate | 2 | failed |
| auto-union-failed | 2 | failed |
| volume-magnitude | 2 | failed |
| revolve-normals | 1 | failed |

## Highest-Leverage Fixes

1. **Fix mesh-too-simple** → would address ~40 cases
2. **Fix cascading-failure** → would address ~32 cases
3. **Fix multiple-failures** → would address ~25 cases
4. **Fix boolean-watertight** → would address ~21 cases
5. **Fix tessellation-degenerate** → would address ~2 cases
6. **Fix auto-union-failed** → would address ~2 cases
7. **Fix volume-magnitude** → would address ~2 cases
8. **Fix revolve-normals** → would address ~1 cases

## Individual Case Results

### R0001 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 4.54e-1 (log: -0.34)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 9f753923-d2d4-44aa-928b-394746d3283d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0002 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.30e0 (log: 0.36)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 260 triangles < expected minimum 288 for operations [("revolve", "rectangle"), ("revolve", "gear")]

### R0003 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.16e2 (log: 2.33)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 140 unpaired edges out of 1882 total

### R0004 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2 unpaired edges out of 778 total (0 boundary, 2 non-manifold)

### R0005 — FAIL

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + revolve(gear,cut)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 1c0eff66-04a7-4636-b762-02e6714872f1: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 16 unpaired edges out of 3525 total (11 boundary, 5 non-manifold)

### R0006 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,boss)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 7a581ff5-ba10-4d91-aad9-6ba25cf35ff5: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 1d45833d-8c95-4214-a99a-98fb1e30b9d8: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle"), ("extrude", "circle")]

### R0007 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 284f44ec-672a-4d52-9b3b-958fc3a68207: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; b45d8e37-ea75-4687-b294-8938f93161e5: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 633f2cf5-127f-48c6-aff9-3dfa40b8aef4: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0008 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 5 unpaired edges out of 432 total (3 boundary, 2 non-manifold)

### R0009 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 3dd1ac8d-fea3-4e1f-8941-3bb014866f50: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 16545414-f288-4e87-aab2-02374e858ce5: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 59ce4859-a45a-4eb5-a1ba-a1c714ac1c29: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0010 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: volume-magnitude
- **Detail**: partial rebuild (1 error(s)): 518e1a3f-d724-4b7d-8e45-ce79a6b0553c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; volume_magnitude: volume 2.548968e-3 outside [2.281929e-2, 2.281929e14] for scale 1.316540e2

### R0011 — PASS

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0012 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): bc9220a7-b035-4902-9842-67f2cecad33c: operation error: kernel error: operation not supported: polygon boolean: 310x590 effective face product (5790) too large for non-convex solids

### R0013 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 7.25e1 (log: 1.86)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): f5ad1d43-9e8a-432c-909e-a4e2074f22d4: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0014 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.28e3 (log: 3.11)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0015 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss) + revolve(circle,boss)
- **Scale**: 1.12e-4 (log: -3.95)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 218e8f25-fd1e-42c0-ae71-cdf8d67af6fa: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 031ac248-deac-4c64-bc98-0975a7d3c3b0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0016 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): f8474247-16ea-431e-a647-2efd6c1605fc: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0017 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 6 unpaired edges out of 415 total (4 boundary, 2 non-manifold)

### R0018 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 288 for operations [("extrude", "rectangle"), ("extrude", "gear"), ("revolve", "gear")]

### R0019 — ERROR

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): c35ff54c-4fb2-4bb2-bc42-83b802a3df6a: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 4abaa468-b8c1-4c2e-866c-e93c31e36cbb: GeomRef resolution failed: Cut revolve requires an existing body to subtract from

### R0020 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: volume-magnitude
- **Detail**: partial rebuild (2 error(s)): e5fe07d0-bb12-4133-9f9c-0e92c4429192: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 0a769474-e3ff-45be-989e-23b4c606536e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; volume_magnitude: volume 4.704854e-5 outside [1.198368e-3, 1.198368e13] for scale 4.930187e1

### R0021 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 2.73e-1 (log: -0.56)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 785c56fc-032e-4862-aa3a-e1df9f525d72: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 6b23d67e-d360-4142-b3c4-71d770393d49: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle"), ("extrude", "circle")]

### R0022 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.89e-1 (log: -0.54)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): f9ef06f4-9612-44a4-908b-584441aa0a6c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0023 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.06e3 (log: 3.03)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 63ecd575-896e-4cd2-82fb-08cbad9114c6: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 1114397e-8498-4d3d-857e-0a2b3e3141fd: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle"), ("extrude", "rectangle")]

### R0024 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.92e1 (log: 1.28)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): e458550d-5738-4602-9d77-79ff3c41d3e0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0025 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + revolve(circle,cut)
- **Scale**: 2.22e3 (log: 3.35)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 6a9adfa2-f5c0-4d35-88b8-31ba45737d1c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 3eb5de42-cc8f-4783-988a-6fab6a6ce765: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0026 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 34374b0b-121d-478b-bd08-754150e02649: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 64cdee21-dbe2-4d26-ad68-375d2115683a: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "circle"), ("revolve", "circle"), ("extrude", "rectangle")]

### R0027 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(circle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: revolve-normals
- **Detail**: partial rebuild (1 error(s)): 6a5e3179-9dfd-4472-956c-66d632fbfb4c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("extrude", "gear"), ("revolve", "circle")]

### R0028 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): b2dff0cc-c77e-49b4-ab7a-3eedf5aa8018: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; e06f9947-b61a-4204-8c97-dfe06c03a2da: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("revolve", "circle"), ("extrude", "circle")]

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0030 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 3b4aafb4-2b90-40c9-920b-510bbb7c61e7: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 7fea6e36-87cd-4f47-9697-61a6dd4e522f: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle"), ("extrude", "circle")]

### R0031 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 791 unpaired edges out of 6149 total (779 boundary, 12 non-manifold)

### R0032 — ERROR

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): d292383e-29ad-4a45-b06a-9550e868dde5: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; a0fc7c89-1e56-4f38-87cf-b404ce5f4646: GeomRef resolution failed: Cut revolve requires an existing body to subtract from; 80cc6a8c-a635-4e4d-9324-4cf20461f32f: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0033 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1 unpaired edges out of 389 total (0 boundary, 1 non-manifold); minimum_triangle_count: 260 triangles < expected minimum 288 for operations [("revolve", "rectangle"), ("revolve", "gear")]

### R0034 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 1bf87fe5-a96d-4479-b54f-0fac753b39dd: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 156 unpaired edges out of 4749 total (56 boundary, 100 non-manifold)

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,boss)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 0529ae38-142a-4bca-8f7a-45cccd2f0021: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 1 unpaired edges out of 2105 total (0 boundary, 1 non-manifold)

### R0036 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 41f81957-0784-47e1-8f6b-c735dfdc7932: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0037 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,cut) + extrude(circle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): f9fface7-24e1-47bd-b6f5-b9332a7c6479: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): c6b760f4-5e78-4008-ac1d-d9b13f0375ed: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0039 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0040 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.58e1 (log: 1.41)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 7586a4a4-6716-4969-a4be-f7323d972e76: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 27df7e97-3243-42de-a122-824ac5084e81: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0041 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.13e0 (log: 0.05)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): b6dec8c9-f707-4364-94a9-0d428822af4b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 45e7e2b6-f9ca-4f1d-97f4-30d01bb197d5: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0042 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 8.59e2 (log: 2.93)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): b0ea7641-67c2-4262-adc3-9e284aebbf5e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 1cc77025-33ae-49cf-a1d3-be933c39990b: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0043 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 254ce032-b157-4b29-8b77-23e355ad8f4c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; c8bd51ad-beea-467b-bc48-62862a1239e0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0044 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 7d9848ef-fd3f-4a78-b791-d351e9fe3153: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 230 unpaired edges out of 3247 total

### R0045 — ERROR

- **Operations**: revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 6168f91c-23f4-4650-bbf0-be5f25c496cc: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 0c04221f-943f-43b2-804c-3d3a0408f51e: GeomRef resolution failed: Cut revolve requires an existing body to subtract from

### R0046 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 80e9429a-5b0b-404b-9c51-4a7e5207f19b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; ac1169b9-4457-4283-a45a-d1d3b5f07480: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; a06309c7-0498-4304-bd15-7760f43448fc: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0047 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut) + revolve(circle,cut)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 27841a24-ff10-40d1-b0b2-3335c6bafb54: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 288 for operations [("extrude", "rectangle"), ("revolve", "gear"), ("revolve", "circle")]

### R0048 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: mesh-too-simple
- **Detail**: minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("extrude", "gear")]

### R0049 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1 unpaired edges out of 389 total (0 boundary, 1 non-manifold)

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): cce05460-8fec-4e28-b375-a9d8675b14e2: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; a40e4236-0d2c-449a-a79f-35a4d577ecc9: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 2e3673d6-9bf6-474a-88d9-da7749966534: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; b1d7e49d-278a-4e98-879e-29d2968ea023: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0052 — PASS

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 532 unpaired edges out of 7594 total (531 boundary, 1 non-manifold)

### R0054 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 577 unpaired edges out of 9432 total (411 boundary, 166 non-manifold)

### R0055 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 9dcf23ca-5708-432c-9103-5a5a9b42dc3b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 36 for operations [("extrude", "rectangle"), ("revolve", "rectangle"), ("extrude", "circle")]

### R0056 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: tessellation-degenerate
- **Detail**: partial rebuild (1 error(s)): a3268ef9-69cc-4acf-a274-b736521515cf: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; no_degenerate_triangles: 42 of 2460 triangles are degenerate

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): f4bf358a-49da-4d4b-907d-3dad24fee1e0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "rectangle"), ("revolve", "circle")]

### R0058 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 366x394 effective face product (15224) too large for non-convex solids. Body created as standalone.; partial rebuild (1 error(s)): 2d4513b2-1d7e-4018-ae1d-6bc19a9c035a: operation error: kernel error: operation not supported: polygon boolean: 366x506 effective face product (9145) too large for non-convex solids; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): 2e2a3575-d979-4dab-b8c7-32b8299eb7f8: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; d73536bd-2c3c-49c5-bc46-64073b60282c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "circle"), ("revolve", "circle"), ("extrude", "rectangle")]

### R0060 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 3d9c0961-28ae-46a4-bc07-1dfa5dbfda06: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 36 for operations [("extrude", "rectangle"), ("revolve", "rectangle"), ("extrude", "circle")]

### R0061 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,boss)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): e9bb0554-098c-4238-a47e-86f1aa9a4e36: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; b3836ade-6edc-4117-bb74-a51027c46eae: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("extrude", "circle"), ("extrude", "gear"), ("extrude", "rectangle")]

### R0062 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): f09ce2b4-1196-4d34-9cd8-eba9da2645c9: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle")]

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: tessellation-degenerate
- **Detail**: partial rebuild (2 error(s)): f46cae2f-8c46-4ebf-81e4-fc3c0db34887: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; a2a0f3b8-7a76-49c2-af45-16b258d9556c: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; no_degenerate_triangles: 132 of 2460 triangles are degenerate

### R0064 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 6 unpaired edges out of 4018 total (4 boundary, 2 non-manifold)

### R0065 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + revolve(circle,cut)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 540137d7-1e4a-4db4-81ee-4ca41a608fbb: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; befd021d-3541-4ce5-8989-be4ffa3b3928: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0066 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 1.19e0 (log: 0.08)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 0eadbb0c-6a65-48df-8ecf-63f9aeb75cfd: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 1ccea5f2-b9b0-487d-836c-40aaef7f820d: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; d31712c0-9daa-4b42-93f0-fb2e4d5f6a82: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0067 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.24e-1 (log: -0.91)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 03f0b4be-0202-470d-bbef-e6bcdd55d86f: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle")]

### R0068 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 4.84e-2 (log: -1.31)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 188a789c-0ddd-4bce-ad01-1adafc0bb7f5: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0069 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 13 unpaired edges out of 407 total (12 boundary, 1 non-manifold)

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 84925bb1-a75c-4230-992a-6e67b324f52d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 1 unpaired edges out of 389 total (0 boundary, 1 non-manifold)

### R0071 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,cut)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1379 unpaired edges out of 4838 total (1373 boundary, 6 non-manifold)

### R0072 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 5779222f-5283-4e1e-ab10-6b8b52976f1c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle")]

### R0073 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0074 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 2a67dad0-35cb-4126-b239-2382890ed3cc: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 11f9f06c-7fd6-48ae-a4bb-52f1eb0911e4: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0075 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 1ccb0e58-a9c5-4c05-8111-e99448c07561: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; bf39bb13-caea-4782-a987-73d7310c7c75: operation error: kernel error: operation not supported: polygon boolean: 646x674 effective face product (12657) too large for non-convex solids

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 47644801-6dfa-489e-bbe9-ecc63a925a49: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 30 unpaired edges out of 1342 total (28 boundary, 2 non-manifold)

### R0077 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 9cb109fe-c31c-498f-980d-92a8c461311f: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle")]

### R0078 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): e76f9117-188a-4ecc-9d57-fc0b46f02082: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### R0079 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 980e4a85-74b8-4bfe-85b7-ce4953af472b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 87c7d186-9617-4a2c-ab88-54a4148584e7: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; e642aebf-e7d4-4ca5-88a8-ec0072c2698d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

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
- **Detail**: partial rebuild (1 error(s)): c2e5cfab-c97c-451f-8e6b-793c63932f16: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0083 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): b28baa13-3487-43e5-abed-b0bdfffccd5e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle")]

### R0084 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 171fd789-6e20-484c-8e9f-a4d807978018: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0085 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 692c21a8-b1b9-4fbc-85a8-a819071ae232: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; watertight_mesh: 344 unpaired edges out of 4198 total

### R0086 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 50df3488-8db6-4366-b4e4-fb74d1e371f1: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### R0087 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 353fbec3-7590-4495-8725-a9f59ea51266: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0088 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 3386c85b-1062-4aa3-9608-8e5342b9cf75: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 731a3e41-bcca-495d-ac66-b974fe309fc3: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; caaa6fe6-a64e-4a1e-aa34-6a53b1e100ad: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0089 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): d2290ba0-839c-479a-9e33-43fe46fd255c: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0090 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: multiple-failures
- **Detail**: partial rebuild (2 error(s)): 8083e0de-b3f7-471e-9185-2401bf2ee845: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 45e8ec1e-96d8-4761-808a-612e77fdd147: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0091 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (2 error(s)): fa0fe16d-deba-4572-b93e-a48d27109c39: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 57b08973-6e92-4b72-b631-968b4310914a: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle"), ("extrude", "circle")]

### R0092 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 4ace9de3-18e8-400e-81b3-da67106aad62: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 7a707bc4-92fc-4f5b-a8f0-d27b377b0b7a: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0093 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 4 unpaired edges out of 2221 total (3 boundary, 1 non-manifold)

### R0094 — FAIL

- **Operations**: revolve(gear,boss) + revolve(circle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 56c54fc9-e4e4-4562-bd0d-edd43e80761b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### R0095 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 7bf03fc0-7128-445f-93f2-f23491db9b2d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; e7aef3cd-194d-4fc3-8a86-39702201f093: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 01a8afd0-ebc4-4bed-b31b-048db31a2024: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0096 — ERROR

- **Operations**: revolve(circle,boss) + revolve(circle,cut)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 298853c1-c479-48fa-864b-f01cbf43e4fe: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 62cd085f-c706-484b-99f7-17392cd646f7: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

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

### R0099 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): a539c97c-0f92-4191-956d-7d83e872345d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 6aa8de23-743e-4785-a5c3-41a116b2c07e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 53d48ac8-f6c7-41e1-8fe6-6de5d408e61e: GeomRef resolution failed: Cut revolve requires an existing body to subtract from

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,cut) + revolve(rectangle,boss)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1 unpaired edges out of 779 total (0 boundary, 1 non-manifold)

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

### F0026 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 2c610b1f-f9c4-44cd-a255-e61243c9e3e8: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0027 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 8645830e-6dc2-409c-960b-12811f199e28: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0028 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): b2f5b30a-57e4-4d4a-b833-0b960f063aa8: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0029 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 73f9a016-c4c6-4185-8bab-b5d4e88b8805: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0030 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): a161dc31-7d2a-4696-948c-057f68ef9347: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0031 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): b0da61d1-08c2-44ea-8393-abca17ded3ac: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0032 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 4a96e348-de9d-458b-a15e-4d4cfe625059: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0033 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): fc043f9e-c539-4024-9026-072c67e15f1b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0034 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 54976706-1755-4246-b34d-8f0bf5482591: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0035 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 0d39bfc3-6df9-45ec-8874-6691862780c5: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0036 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): a191fcfd-d887-4658-84bd-dfa9bc2678e0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 7f57659d-83d7-40db-ac2d-43e8d61d2a28: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0037 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 41fe91c3-3fd9-44bf-9a1b-3a83c83cb4d0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 3429977b-7f37-4f1e-ac8c-3382e3faa2f6: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0038 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 93d1b01e-ee48-477c-811b-6f9875352232: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 61cb0508-7bc8-43c0-8d47-6affb3b3c3af: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0039 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 9ada6f3c-00dc-4aa7-b69a-ee08626a921b: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 33da04f7-7e43-4333-a970-fcc397e6b7c3: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0040 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 58b5e9ef-59a3-4f25-aa24-502506761283: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 1d3ed56f-4a39-4e50-bf37-d4315f62ca2a: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0041 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 5bb9e0e7-a947-42e0-9e19-324ba41fa77e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 30366144-fb85-4764-8e3f-1b6bcfd1cda0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0042 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 0bf53d6e-a507-4350-b9f0-6d5f10855659: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; ca69322c-5ec7-4850-9275-be3b2a4c4ce6: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0043 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): bc06f836-a702-4104-b48a-178dd079fc25: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 562aee4c-b781-4f24-93fc-699e913a027e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0044 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): dd3be96e-28c8-43fe-b672-fef6ad4fbda0: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; c0147899-7b77-4fb2-bdcc-16a10c1c7ef4: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0045 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): f43cf403-38dd-446a-8f4b-ccbae4e58dcf: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 140b36a4-5a80-42e5-b5e6-de5020a400e4: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0046 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 15a15ade-8011-47bf-8765-f7d0bf25f312: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0047 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): e1fc68f0-be4a-4d26-8eb8-dfaf39fada17: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0048 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 767a1412-0ec7-4b27-8436-51d7337498f7: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0049 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): b7fa8055-a5ab-4387-8025-88ababafddd9: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0050 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): e8f6e982-16c9-4c2f-831f-ad89ac8d83a9: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0051 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e-4 (log: -4.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0052 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e-4 (log: -4.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): ff3c4027-4ac1-4b94-9af2-fa4bdcb3cd38: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 7b053b86-c3a0-4a08-a332-04b3212691b5: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0053 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0054 — ERROR

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 0c02b865-23f3-4f87-b3b4-895ad76bf768: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 79735117-12a4-4cff-af75-1040fd82f5cc: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### F0055 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e4 (log: 4.00)
- **Category**: mesh-too-simple
- **Detail**: partial rebuild (1 error(s)): 35aa3be8-9177-4d47-b1d5-28f749f7c176: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "rectangle"), ("extrude", "circle")]

### F0056 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): c5ff8d80-52cf-47b7-aa44-4bea29d359ab: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; ccba0e41-c8ef-467b-81aa-ceb47cc779e1: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0057 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): e23c0546-5722-474e-ab41-c522bd72a55f: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 078d5ec7-e99a-47b3-be61-bdcf4fe1cdef: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0058 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 0a9c492f-0c3d-47f8-925e-d00f53f84469: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 2782500d-861a-4334-956e-0f7a15fcc2a7: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0059 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 00818263-9190-4b9f-b25a-c96e98d47913: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; bb68b3bb-80ac-4e58-8d97-4bfa1213b56e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

### F0060 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 6c6d8230-95c8-48ea-85b3-aa5faeae797e: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1; 9cc63e95-4791-4b7c-8aa6-eed1e3b4837d: kernel error: kernel error: Need at least 3 vertices for a polygon, got 1

# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-27
Score: **66/172** (66 pass, 80 fail, 26 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| pass-boss-only | 37 | passed |
| auto-union-failed | 32 | failed |
| pass-genuine | 29 | passed |
| cascading-failure | 26 | errored |
| multiple-failures | 20 | failed |
| boolean-watertight | 14 | failed |
| revolve-normals | 10 | failed |
| tessellation-degenerate | 3 | failed |
| boolean-normals | 1 | failed |

## Highest-Leverage Fixes

1. **Fix auto-union-failed** → would address ~32 cases
2. **Fix cascading-failure** → would address ~26 cases
3. **Fix multiple-failures** → would address ~20 cases
4. **Fix boolean-watertight** → would address ~14 cases
5. **Fix revolve-normals** → would address ~10 cases
6. **Fix tessellation-degenerate** → would address ~3 cases
7. **Fix boolean-normals** → would address ~1 cases

## Individual Case Results

### R0001 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 4.54e-1 (log: -0.34)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0002 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.30e0 (log: 0.36)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 16 of 73744 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("revolve", "gear")]; volume_magnitude: volume 0.000000e0 outside [1.224280e-7, 1.224280e9] for scale 2.304767e0

### R0003 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.16e2 (log: 2.33)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 7a56ef97-a774-4dee-90c3-4fc502601f20: operation error: kernel error: operation not supported: polygon boolean: 13448 total faces exceeds limit (8000)

### R0004 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 16 of 73744 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("revolve", "rectangle"), ("extrude", "gear")]; volume_magnitude: volume 0.000000e0 outside [2.869500e-8, 2.869500e8] for scale 1.421026e0

### R0005 — ERROR

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + revolve(gear,cut)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0006 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,boss)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0007 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle"), ("extrude", "gear")]

### R0008 — PASS

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0009 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(32) - E(84) + F(56) = 4 (expected 2)

### R0010 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); volume_magnitude: volume 2.548968e-3 outside [2.281929e-2, 2.281929e14] for scale 1.316540e2

### R0011 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0012 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1559 unpaired edges out of 9652 total; mesh_euler_characteristic: V(3765) - E(9652) + F(5915) = 28 (expected 2)

### R0013 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 7.25e1 (log: 1.86)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0014 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.28e3 (log: 3.11)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(1268) - E(3792) + F(2528) = 4 (expected 2)

### R0015 — ERROR

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss) + revolve(circle,boss)
- **Scale**: 1.12e-4 (log: -3.95)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0016 — ERROR

- **Operations**: extrude(gear,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0017 — ERROR

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0018 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0019 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 0793c345-7a51-429e-a06b-8bb4fd23f0bc: operation error: kernel error: operation not supported: polygon boolean: 20644 total faces exceeds limit (8000)

### R0020 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("extrude", "circle"), ("extrude", "circle")]; volume_magnitude: volume 0.000000e0 outside [1.198368e-3, 1.198368e13] for scale 4.930187e1

### R0021 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 2.73e-1 (log: -0.56)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(104) - E(300) + F(200) = 4 (expected 2)

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

### R0025 — ERROR

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + revolve(circle,cut)
- **Scale**: 2.22e3 (log: 3.35)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0026 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 85 unpaired edges out of 15323 total (84 boundary, 1 non-manifold); unit_normals: 272 of 31503 normals are not unit length; mesh_euler_characteristic: V(4743) - E(15323) + F(10188) = -392 (expected 2)

### R0027 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + revolve(circle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 12 for operations [("extrude", "rectangle"), ("extrude", "gear"), ("revolve", "circle")]

### R0028 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0030 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0031 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 468 unpaired edges out of 5792 total (461 boundary, 7 non-manifold); mesh_euler_characteristic: V(2081) - E(5792) + F(3711) = 0 (expected 2)

### R0032 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(circle,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; partial rebuild (1 error(s)): 8c8e168d-ecbf-4f24-a71b-c1ddb59ca815: operation error: kernel error: operation not supported: polygon boolean: 20868 total faces exceeds limit (8000); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0033 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,cut)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 16 of 73744 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("revolve", "gear")]; volume_magnitude: volume 0.000000e0 outside [7.737396e-14, 7.737396e2] for scale 1.977872e-2

### R0034 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 680 unpaired edges out of 26644 total; mesh_euler_characteristic: V(9112) - E(26644) + F(17536) = 4 (expected 2)

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,boss)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

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
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 8 of 72208 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("revolve", "rectangle"), ("revolve", "circle")]; volume_magnitude: volume 0.000000e0 outside [2.447684e-5, 2.447684e11] for scale 1.347675e1

### R0039 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0040 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.58e1 (log: 1.41)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 131 unpaired edges out of 15852 total (2 boundary, 129 non-manifold); mesh_euler_characteristic: V(4944) - E(15852) + F(10926) = 18 (expected 2)

### R0041 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.13e0 (log: 0.05)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(9025) - E(28116) + F(18744) = -347 (expected 2)

### R0042 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 8.59e2 (log: 2.93)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(63) - E(177) + F(118) = 4 (expected 2)

### R0043 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); mesh_euler_characteristic: V(64) - E(180) + F(120) = 4 (expected 2)

### R0044 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: operation not supported: polygon boolean: 14468 total faces exceeds limit (8000). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 1040 unpaired edges out of 43144 total; mesh_euler_characteristic: V(14728) - E(43144) + F(28416) = 0 (expected 2)

### R0045 — FAIL

- **Operations**: revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: multiple-failures
- **Detail**: unit_normals: 256 of 8448 normals are not unit length

### R0046 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(204) - E(582) + F(388) = 10 (expected 2)

### R0047 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut) + revolve(circle,cut)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0048 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0049 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 32 of 47264 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("extrude", "gear")]; volume_magnitude: volume 0.000000e0 outside [8.017676e-16, 8.017676e0] for scale 4.312040e-3

### R0050 — ERROR

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0051 — ERROR

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0052 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 14442 total faces exceeds limit (8000). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0054 — ERROR

- **Operations**: extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0055 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(42) - E(111) + F(74) = 5 (expected 2)

### R0056 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: tessellation-degenerate
- **Detail**: no_degenerate_triangles: 42 of 2460 triangles are degenerate

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 135 unpaired edges out of 25032 total; mesh_euler_characteristic: V(8389) - E(25032) + F(16643) = 0 (expected 2)

### R0058 — PASS

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0059 — ERROR

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0060 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0061 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,boss)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "gear"), ("extrude", "rectangle")]

### R0062 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle")]

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 354 unpaired edges out of 3873 total; mesh_euler_characteristic: V(1302) - E(3873) + F(2464) = -107 (expected 2)

### R0064 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(1648) - E(4944) + F(3296) = 0 (expected 2)

### R0065 — ERROR

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + revolve(circle,cut)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0066 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 1.19e0 (log: 0.08)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0067 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.24e-1 (log: -0.91)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0068 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 4.84e-2 (log: -1.31)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0069 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("extrude", "rectangle")]

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; unit_normals: 16 of 40208 normals are not unit length; face_range_coverage: no face ranges defined; outward_normals: no valid triangles; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 36 for operations [("revolve", "rectangle"), ("extrude", "gear"), ("extrude", "circle")]; volume_magnitude: volume 0.000000e0 outside [5.261258e-14, 5.261258e2] for scale 1.739255e-2

### R0071 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,cut)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: tessellation-degenerate
- **Detail**: partial rebuild (1 error(s)): 11242d1f-574e-4964-a7ed-7d0f4d4cc27a: operation error: kernel error: operation not supported: polygon boolean: 8572 total faces exceeds limit (8000); no_degenerate_triangles: 648 of 2012 triangles are degenerate

### R0072 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0073 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0074 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,boss) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0075 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 32 unpaired edges out of 151 total; minimum_triangle_count: 90 triangles < expected minimum 96 for operations [("extrude", "gear"), ("extrude", "circle"), ("extrude", "gear")]; mesh_euler_characteristic: V(64) - E(151) + F(90) = 3 (expected 2)

### R0076 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0077 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle")]

### R0078 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0079 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(160) - E(462) + F(308) = 6 (expected 2)

### R0080 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0081 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 19420 total faces exceeds limit (8000). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); volume_magnitude: volume 1.483912e-11 outside [1.128408e-10, 1.128408e6] for scale 2.242963e-1

### R0082 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0083 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "rectangle")]

### R0084 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

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

### R0087 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0088 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; minimum_triangle_count: 0 triangles < expected minimum 32 for operations [("extrude", "circle"), ("extrude", "gear"), ("extrude", "rectangle")]

### R0089 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0090 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0091 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); minimum_triangle_count: 12 triangles < expected minimum 96 for operations [("revolve", "circle"), ("extrude", "rectangle"), ("extrude", "circle")]

### R0092 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0093 — ERROR

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0094 — ERROR

- **Operations**: revolve(gear,boss) + revolve(circle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0095 — ERROR

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0096 — FAIL

- **Operations**: revolve(circle,boss) + revolve(circle,cut)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: multiple-failures
- **Detail**: unit_normals: 256 of 8448 normals are not unit length

### R0097 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0098 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0099 — ERROR

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,cut) + revolve(rectangle,boss)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: operands are disjoint (bounding boxes do not overlap). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); volume_magnitude: volume 9.886150e-3 outside [1.132784e-1, 1.132784e15] for scale 2.245859e2

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

### F0011 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: expected rebuild error but rebuild succeeded

### F0012 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: expected rebuild error but rebuild succeeded

### F0013 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: expected rebuild error but rebuild succeeded

### F0014 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: expected rebuild error but rebuild succeeded

### F0015 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: expected rebuild error but rebuild succeeded

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

### F0021 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(225) - E(678) + F(452) = -1 (expected 2)

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
- **Category**: multiple-failures
- **Detail**: mesh_euler_characteristic: V(128) - E(360) + F(240) = 8 (expected 2)

### F0045 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### F0046 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0047 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 129 unpaired edges out of 47763 total (1 boundary, 128 non-manifold); consistent_normals: 473 of 32476 triangles have reversed normals; mesh_euler_characteristic: V(15446) - E(47763) + F(32476) = 159 (expected 2)

### F0048 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0049 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

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
- **Detail**: watertight_mesh: 12 unpaired edges out of 2130 total; mesh_euler_characteristic: V(712) - E(2130) + F(1416) = -2 (expected 2)

### F0056 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 94 unpaired edges out of 275 total; mesh_euler_characteristic: V(119) - E(275) + F(152) = -4 (expected 2)

### F0057 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 33 unpaired edges out of 330 total; consistent_normals: 4 of 209 triangles have reversed normals; mesh_euler_characteristic: V(118) - E(330) + F(209) = -3 (expected 2)

### F0058 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 73 unpaired edges out of 284 total; mesh_euler_characteristic: V(106) - E(284) + F(165) = -13 (expected 2)

### F0059 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 59 unpaired edges out of 316 total; consistent_normals: 2 of 191 triangles have reversed normals; mesh_euler_characteristic: V(122) - E(316) + F(191) = -3 (expected 2)

### F0060 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 9 unpaired edges out of 372 total; volume_magnitude: volume 1.152024e-18 outside [1.000000e-8, 1.000000e8] for scale 1.000000e0; mesh_euler_characteristic: V(126) - E(372) + F(245) = -1 (expected 2)

### F0061 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0062 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### F0063 — FAIL

- **Operations**: extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-normals
- **Detail**: watertight_mesh: 624 unpaired edges out of 1452 total; outward_normals: only 716 of 760 triangles (94.2%) have outward normals (need 95%); mesh_euler_characteristic: V(700) - E(1452) + F(760) = 8 (expected 2)

### F0064 — FAIL

- **Operations**: extrude(gear,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 546 unpaired edges out of 3003 total; mesh_euler_characteristic: V(1190) - E(3003) + F(1820) = 7 (expected 2)

### F0065 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0066 — FAIL

- **Operations**: extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: revolve-normals
- **Detail**: outward_normals: only 180 of 296 triangles (60.8%) have outward normals (need 95%); mesh_euler_characteristic: V(164) - E(444) + F(296) = 16 (expected 2)

### F0067 — ERROR

- **Operations**: extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(polygon,boss) + extrude(rectangle,boss) + extrude(circle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0068 — ERROR

- **Operations**: extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(circle,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0069 — ERROR

- **Operations**: extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(circle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0070 — ERROR

- **Operations**: extrude(gear,boss) + extrude(polygon,boss) + extrude(rectangle,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(circle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(rectangle,boss) + extrude(gear,boss) + extrude(rectangle,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0071 — ERROR

- **Operations**: extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(rectangle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### F0072 — ERROR

- **Operations**: extrude(gear,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(circle,boss) + extrude(circle,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(circle,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(gear,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss) + extrude(polygon,boss)
- **Scale**: 1.00e0 (log: 0.00)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

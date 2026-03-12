# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-10
Score: **28/110** (28 pass, 82 fail, 0 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| boolean-watertight | 54 | failed |
| pass-boss-only | 22 | passed |
| auto-union-failed | 19 | failed |
| pass-genuine | 6 | passed |
| tessellation-degenerate | 3 | failed |
| multiple-failures | 2 | failed |
| revolve-normals | 2 | failed |
| boolean-normals | 2 | failed |

## Highest-Leverage Fixes

1. **Fix boolean-watertight** → would address ~54 cases
2. **Fix auto-union-failed** → would address ~19 cases
3. **Fix tessellation-degenerate** → would address ~3 cases
4. **Fix multiple-failures** → would address ~2 cases
5. **Fix revolve-normals** → would address ~2 cases
6. **Fix boolean-normals** → would address ~2 cases

## Individual Case Results

### R0001 — PASS

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 4.54e-1 (log: -0.34)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0002 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 2.30e0 (log: 0.36)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0003 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.16e2 (log: 2.33)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): cf3f04dc-bb3a-4ab2-9a94-2c5671c176b5: operation error: kernel error: boolean operation failed: non-manifold result: 53 half-edges unpaired out of 499 (10.6%); watertight_mesh: 258 unpaired edges out of 17162 total; no_degenerate_triangles: 256 of 11696 triangles are degenerate; outward_normals: only 0 of 11440 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -2.155921e3 (should be > 0); expected positive signed volume, got -2.155921e3

### R0004 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 8 unpaired edges out of 808 total

### R0005 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0006 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: tessellation-degenerate
- **Detail**: partial rebuild (1 error(s)): 98a30b16-ef86-47af-9344-a0d469cb4c9f: operation error: kernel error: operation not supported: polygon boolean: 259 total faces exceeds limit (250); no_degenerate_triangles: 1 of 512 triangles are degenerate; face_range_coverage: empty range at index 4

### R0007 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: revolve-normals
- **Detail**: partial rebuild (1 error(s)): 289b6cfa-14d4-432a-b04b-bdefc761615d: operation error: kernel error: boolean operation failed: one or both solids have no planar faces; consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; expected positive signed volume, got 0.000000e0

### R0008 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut) + extrude(gear,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 500 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0009 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 775e9387-cce8-43c4-a1cc-4aabb0689bbe: operation error: kernel error: boolean operation failed: non-manifold result: 256 half-edges unpaired out of 1280 (20.0%); watertight_mesh: 14 unpaired edges out of 16 total; no_degenerate_triangles: 33 of 380 triangles are degenerate

### R0010 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: tessellation-degenerate
- **Detail**: no_degenerate_triangles: 4 of 828 triangles are degenerate; face_range_coverage: empty range at index 9

### R0011 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 1518 total; no_degenerate_triangles: 2 of 1008 triangles are degenerate

### R0012 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,boss)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 42 half-edges unpaired out of 476 (8.8%). Body created as standalone.; partial rebuild (1 error(s)): a4feac21-93fc-4ea0-b4f8-bd4e2943a30f: operation error: kernel error: boolean operation failed: non-manifold result: 6 half-edges unpaired out of 8 (75.0%); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

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

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(circle,boss)
- **Scale**: 1.12e-4 (log: -3.95)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 17 unpaired edges out of 17 total; no_degenerate_triangles: 33 of 402 triangles are degenerate

### R0016 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 178 half-edges unpaired out of 2310 (7.7%). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0017 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): cd841cd7-27a4-49b0-84d4-348ad6f3c8fa: operation error: kernel error: operation not supported: polygon boolean: 512 total faces exceeds limit (250); watertight_mesh: 61 unpaired edges out of 1652 total

### R0018 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): e18b7525-eb2f-4590-b7a4-7cbf3ef0d123: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 52 (15.4%)

### R0019 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): f042edc8-9022-40cd-9d0a-77d4f4d34a18: operation error: kernel error: operation not supported: polygon approx boolean: 96 total faces exceeds limit

### R0020 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: operation not supported: polygon approx boolean: 156 total faces exceeds limit. Body created as standalone.; partial rebuild (1 error(s)): abd6232d-86e3-47f2-8876-0fa1dd056fc2: operation error: kernel error: operation not supported: polygon approx boolean: 156 total faces exceeds limit; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 258 unpaired edges out of 23012 total; no_degenerate_triangles: 256 of 15596 triangles are degenerate; outward_normals: only 0 of 15340 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -7.578841e3 (should be > 0); expected positive signed volume, got -7.578841e3

### R0021 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 2.73e-1 (log: -0.56)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0022 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.89e-1 (log: -0.54)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0023 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.06e3 (log: 3.03)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 36ab64d2-b45b-4dd2-a654-eb3c4ecaf6cc: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 28 (28.6%)

### R0024 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.92e1 (log: 1.28)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0025 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + revolve(circle,cut)
- **Scale**: 2.22e3 (log: 3.35)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 17 unpaired edges out of 1549 total; face_range_coverage: empty range at index 43

### R0026 — FAIL

- **Operations**: revolve(circle,boss) + extrude(circle,cut) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 7235c0d2-a765-4556-8d82-f50025794bd1: operation error: kernel error: boolean operation failed: non-manifold result: 17 half-edges unpaired out of 111 (15.3%); watertight_mesh: 31 unpaired edges out of 989 total; no_degenerate_triangles: 3 of 680 triangles are degenerate

### R0027 — FAIL

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut) + revolve(rectangle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): d2b0feb1-a73f-4b5e-9359-4a778be3f586: operation error: kernel error: boolean operation failed: non-manifold result: 37 half-edges unpaired out of 395 (9.4%)

### R0028 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2204 unpaired edges out of 17784 total; no_degenerate_triangles: 324 of 13914 triangles are degenerate

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0030 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut) + extrude(circle,cut)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 5728cc04-1001-46a7-a51e-f0e2540a2084: operation error: kernel error: operation not supported: polygon approx boolean: 102 total faces exceeds limit; watertight_mesh: 3 unpaired edges out of 3 total

### R0031 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0032 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 303 unpaired edges out of 7701 total

### R0033 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,boss)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 57 unpaired edges out of 1026 total; no_degenerate_triangles: 17 of 687 triangles are degenerate

### R0034 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 69135679-c412-43ed-afe6-c26774168323: operation error: kernel error: operation not supported: polygon approx boolean: 90 total faces exceeds limit; watertight_mesh: 94 unpaired edges out of 4811 total; face_range_coverage: empty range at index 524

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 77817b8e-a014-4428-b934-3c0c1883dc09: operation error: kernel error: boolean operation failed: non-manifold result: 6 half-edges unpaired out of 8 (75.0%)

### R0036 — PASS

- **Operations**: extrude(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0037 — FAIL

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + extrude(rectangle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: boolean-normals
- **Detail**: partial rebuild (1 error(s)): 04173271-baf8-4b74-b056-4d3208c10d3e: operation error: kernel error: operation not supported: polygon approx boolean: 114 total faces exceeds limit; watertight_mesh: 24 unpaired edges out of 491 total; outward_normals: only 0 of 320 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -5.704904e-7 (should be > 0); expected positive signed volume, got -5.704904e-7

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 5a4a10ed-f532-45d7-a830-0d2e4d459dd8: operation error: kernel error: boolean operation failed: non-manifold result: 6 half-edges unpaired out of 8 (75.0%); 7a9b6470-58af-4040-9149-72d9ddaf9108: operation error: kernel error: boolean operation failed: non-manifold result: 189 half-edges unpaired out of 2965 (6.4%); outward_normals: only 0 of 516 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -2.733682e2 (should be > 0); expected positive signed volume, got -2.733682e2

### R0039 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 442 unpaired edges out of 3762 total; no_degenerate_triangles: 112 of 2694 triangles are degenerate

### R0040 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 2.58e1 (log: 1.41)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0041 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.13e0 (log: 0.05)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0042 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,boss)
- **Scale**: 8.59e2 (log: 2.93)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 29 half-edges unpaired out of 67 (43.3%). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); consistent_normals: 21 of 516 triangles have reversed normals

### R0043 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): ba814631-298c-4935-863e-8890394ca8af: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 52 (15.4%); 0744d042-8bd4-4bd5-a9c2-867c8ce73652: operation error: kernel error: boolean operation failed: non-manifold result: 134 half-edges unpaired out of 478 (28.0%)

### R0044 — FAIL

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): e36b99e9-b087-4230-8a4a-68e25b08095a: operation error: kernel error: boolean operation failed: non-manifold result: 55 half-edges unpaired out of 593 (9.3%); watertight_mesh: 258 unpaired edges out of 20672 total; no_degenerate_triangles: 256 of 14036 triangles are degenerate

### R0045 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: polygon approx boolean: 102 total faces exceeds limit. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 12 unpaired edges out of 378 total

### R0046 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 2b0f2a5d-9e38-4286-8dbf-c2522c275dcf: operation error: kernel error: boolean operation failed: non-manifold result: 22 half-edges unpaired out of 206 (10.7%); watertight_mesh: 17 unpaired edges out of 1081 total

### R0047 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 20 unpaired edges out of 23 total; no_degenerate_triangles: 48 of 100 triangles are degenerate

### R0048 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): 7e456068-f987-4034-a461-e7404e44cf5a: operation error: kernel error: operation not supported: polygon approx boolean: 180 total faces exceeds limit

### R0049 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 19 unpaired edges out of 800 total

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 93 half-edges unpaired out of 927 (10.0%). Body created as standalone.; partial rebuild (1 error(s)): f8a13e97-18a9-4931-96d4-ab6f305fe383: operation error: kernel error: boolean operation failed: non-manifold result: 60 half-edges unpaired out of 222 (27.0%); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 258 unpaired edges out of 5852 total; no_degenerate_triangles: 256 of 4156 triangles are degenerate; outward_normals: only 0 of 3900 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -1.826607e2 (should be > 0); expected positive signed volume, got -1.826607e2

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 382 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 133 unpaired edges out of 267 total; consistent_normals: 38 of 516 triangles have reversed normals; outward_normals: only 478 of 516 triangles (92.6%) have outward normals (need 95%)

### R0052 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 24 unpaired edges out of 558 total

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 16 half-edges unpaired out of 32 (50.0%). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 57 unpaired edges out of 2442 total; no_degenerate_triangles: 3 of 1635 triangles are degenerate; face_range_coverage: empty range at index 151

### R0054 — FAIL

- **Operations**: revolve(gear,boss) + revolve(gear,cut) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 707 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 258 unpaired edges out of 24182 total; no_degenerate_triangles: 256 of 16376 triangles are degenerate

### R0055 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 6d3546f0-e057-46c0-938a-0c62043f55b5: operation error: kernel error: boolean operation failed: non-manifold result: 4 half-edges unpaired out of 26 (15.4%); watertight_mesh: 28 unpaired edges out of 626 total

### R0056 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 94a73a42-7558-4331-ac12-86dc02b7ea31: operation error: kernel error: operation not supported: polygon approx boolean: 180 total faces exceeds limit; watertight_mesh: 97 unpaired edges out of 233 total

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 255 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0058 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 623d1870-6b99-4ee4-95bd-d4ee1627567c: operation error: kernel error: boolean operation failed: non-manifold result: 72 half-edges unpaired out of 200 (36.0%); watertight_mesh: 3 unpaired edges out of 1458 total; no_degenerate_triangles: 2 of 971 triangles are degenerate

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (2 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 49 half-edges unpaired out of 239 (20.5%). Body created as standalone.; Extrude 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 21 half-edges unpaired out of 99 (21.2%). Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0060 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,cut)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 32 unpaired edges out of 811 total

### R0061 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 16 unpaired edges out of 164 total

### R0062 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 37 unpaired edges out of 41 total; no_degenerate_triangles: 402 of 1372 triangles are degenerate

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): d4208db3-7f66-482d-8724-b072c6fc521b: operation error: kernel error: boolean operation failed: non-manifold result: 34 half-edges unpaired out of 194 (17.5%); watertight_mesh: 95 unpaired edges out of 265 total

### R0064 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): ccbb482d-dd22-42c1-a1ab-7bed48f9dd18: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 64 (12.5%)

### R0065 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(circle,boss)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 252758d7-bd19-432d-ad05-236b452673e9: operation error: kernel error: boolean operation failed: tool encloses or equals blank (concentric); watertight_mesh: 192 unpaired edges out of 2759 total; no_degenerate_triangles: 45 of 1977 triangles are degenerate

### R0066 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.19e0 (log: 0.08)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0067 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.24e-1 (log: -0.91)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0068 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,boss)
- **Scale**: 4.84e-2 (log: -1.31)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 25 half-edges unpaired out of 57 (43.9%). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 27 unpaired edges out of 747 total; consistent_normals: 8 of 516 triangles have reversed normals; outward_normals: only 8 of 516 triangles (1.6%) have outward normals (need 95%); positive_signed_volume: signed volume = -7.507733e-6 (should be > 0); expected positive signed volume, got -7.507733e-6

### R0069 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 22 total

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): cb9600ba-f8cf-4d7c-9b91-e6d2fb99392f: operation error: kernel error: boolean operation failed: non-manifold result: 419 half-edges unpaired out of 3815 (11.0%); 81509434-14b1-44c4-8b64-49662d509751: operation error: kernel error: boolean operation failed: non-manifold result: 88 half-edges unpaired out of 232 (37.9%); consistent_normals: 26 of 516 triangles have reversed normals; outward_normals: only 26 of 516 triangles (5.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -4.294541e-7 (should be > 0); expected positive signed volume, got -4.294541e-7

### R0071 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 92 half-edges unpaired out of 452 (20.4%). Body created as standalone.; partial rebuild (1 error(s)): 4f166544-0784-473b-8651-ce87a6e8feec: operation error: kernel error: boolean operation failed: non-manifold result: 28 half-edges unpaired out of 384 (7.3%); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 3 unpaired edges out of 3 total; no_degenerate_triangles: 256 of 4156 triangles are degenerate

### R0072 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 65c74d95-4935-438a-a7f1-373009eb04a9: operation error: kernel error: boolean operation failed: non-manifold result: 440 half-edges unpaired out of 6616 (6.7%); watertight_mesh: 279 unpaired edges out of 323 total; no_degenerate_triangles: 473 of 4276 triangles are degenerate

### R0073 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: tessellation-degenerate
- **Detail**: no_degenerate_triangles: 1 of 52 triangles are degenerate; face_range_coverage: empty range at index 7

### R0074 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 57 unpaired edges out of 963 total

### R0075 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: boolean-normals
- **Detail**: watertight_mesh: 50 unpaired edges out of 1570 total; outward_normals: only 0 of 1030 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -2.592174e4 (should be > 0); expected positive signed volume, got -2.592174e4

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): f17890aa-6e91-437a-8e1a-351952d007d4: operation error: kernel error: boolean operation failed: non-manifold result: 144 half-edges unpaired out of 852 (16.9%)

### R0077 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 77450250-0d11-454f-a30f-a87905f345da: operation error: kernel error: boolean operation failed: non-manifold result: 21 half-edges unpaired out of 141 (14.9%); watertight_mesh: 258 unpaired edges out of 5852 total; no_degenerate_triangles: 256 of 4156 triangles are degenerate; outward_normals: only 0 of 3900 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -3.413674e8 (should be > 0); expected positive signed volume, got -3.413674e8

### R0078 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 49 half-edges unpaired out of 229 (21.4%). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 258 unpaired edges out of 10078 total; no_degenerate_triangles: 256 of 7016 triangles are degenerate

### R0079 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(rectangle,boss)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 34 half-edges unpaired out of 518 (6.6%). Body created as standalone.; partial rebuild (1 error(s)): 0f445b2c-f9a8-49bb-a0bd-a8281326e430: operation error: kernel error: boolean operation failed: non-manifold result: 293 half-edges unpaired out of 5067 (5.8%); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0080 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0081 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 268 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 280 unpaired edges out of 24112 total; no_degenerate_triangles: 256 of 16376 triangles are degenerate

### R0082 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 74c2b03d-da75-410e-b8ed-0e38cb2bc0fb: operation error: kernel error: boolean operation failed: non-manifold result: 98 half-edges unpaired out of 922 (10.6%); consistent_normals: 17 of 516 triangles have reversed normals; outward_normals: only 17 of 516 triangles (3.3%) have outward normals (need 95%); positive_signed_volume: signed volume = -8.704864e4 (should be > 0); expected positive signed volume, got -8.704864e4

### R0083 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 3ba1bbe2-56d1-493c-bb83-0a3345d18a53: operation error: kernel error: boolean operation failed: non-manifold result: 242 half-edges unpaired out of 1210 (20.0%)

### R0084 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 222 unpaired edges out of 315 total; no_degenerate_triangles: 30 of 1360 triangles are degenerate

### R0085 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 59 half-edges unpaired out of 945 (6.2%). Body created as standalone.; partial rebuild (1 error(s)): 502aad62-b1dc-481f-8481-5eb667ca5e5c: operation error: kernel error: operation not supported: polygon approx boolean: 162 total faces exceeds limit; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0086 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0087 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 121 half-edges unpaired out of 1729 (7.0%). Body created as standalone.; partial rebuild (1 error(s)): 6d7acb35-c124-4d2d-9a11-a1cbb2459b03: operation error: kernel error: boolean operation failed: non-manifold result: 331 half-edges unpaired out of 2901 (11.4%); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0088 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 6c7cffa7-dbbc-4a80-95f5-2ce9142c1881: operation error: kernel error: operation not supported: polygon boolean: 1467 total faces exceeds limit (250); watertight_mesh: 28 unpaired edges out of 4832 total; face_range_coverage: empty range at index 27

### R0089 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0090 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + revolve(rectangle,cut)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 3e89f1d1-cbb2-406c-b63e-eb40240b0ca2: operation error: kernel error: boolean operation failed: non-manifold result: 4 half-edges unpaired out of 26 (15.4%)

### R0091 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): eb0cee47-6d85-422b-aae7-06a59c784b86: operation error: kernel error: operation not supported: polygon boolean: 394 total faces exceeds limit (250); watertight_mesh: 18 unpaired edges out of 19 total; no_degenerate_triangles: 113 of 1051 triangles are degenerate

### R0092 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 0a284f21-55e3-44ce-bc5a-12fb13ed9546: operation error: kernel error: boolean operation failed: non-manifold result: 32 half-edges unpaired out of 160 (20.0%)

### R0093 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 82f1837f-457d-4b89-b8a9-0d182759816f: operation error: kernel error: operation not supported: polygon approx boolean: 144 total faces exceeds limit; d1cd177f-5233-43e3-ae54-4d310215b4c0: operation error: kernel error: operation not supported: polygon approx boolean: 108 total faces exceeds limit; watertight_mesh: 46 unpaired edges out of 334 total

### R0094 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 997d436f-cf9c-48ec-a00d-7dd8cd88ff07: operation error: kernel error: boolean operation failed: non-manifold result: 4 half-edges unpaired out of 20 (20.0%); 0709316f-95f2-4579-8f3e-a332afd4bd95: operation error: kernel error: boolean operation failed: non-manifold result: 100 half-edges unpaired out of 500 (20.0%)

### R0095 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 2e32d2f9-134a-4088-80c6-f514b0b3f6b9: operation error: kernel error: boolean operation failed: non-manifold result: 21 half-edges unpaired out of 105 (20.0%); watertight_mesh: 101 unpaired edges out of 137 total; no_degenerate_triangles: 24 of 753 triangles are degenerate; outward_normals: only 0 of 619 triangles (0.0%) have outward normals (need 95%); positive_signed_volume: signed volume = -1.471383e-13 (should be > 0); expected positive signed volume, got -1.471383e-13

### R0096 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 309 unpaired edges out of 1733 total; no_degenerate_triangles: 108 of 1452 triangles are degenerate

### R0097 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 18 unpaired edges out of 510 total

### R0098 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): a475deac-8ed0-4250-977a-073489858ce9: operation error: kernel error: operation not supported: polygon approx boolean: 722 total faces exceeds limit; watertight_mesh: 64 unpaired edges out of 2204 total; no_degenerate_triangles: 4 of 1448 triangles are degenerate

### R0099 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; expected positive signed volume, got 0.000000e0

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): c4d347ab-c0b2-4fa3-ba77-68628ee93a4e: operation error: kernel error: boolean operation failed: non-manifold result: 10 half-edges unpaired out of 12 (83.3%); consistent_normals: 12 of 516 triangles have reversed normals

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

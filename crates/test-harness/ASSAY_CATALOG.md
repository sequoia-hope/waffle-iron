# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-10
Score: **37/110** (37 pass, 73 fail, 0 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| boolean-watertight | 59 | failed |
| pass-boss-only | 23 | passed |
| pass-genuine | 14 | passed |
| auto-union-failed | 12 | failed |
| multiple-failures | 1 | failed |
| revolve-normals | 1 | failed |

## Highest-Leverage Fixes

1. **Fix boolean-watertight** → would address ~59 cases
2. **Fix auto-union-failed** → would address ~12 cases
3. **Fix multiple-failures** → would address ~1 cases
4. **Fix revolve-normals** → would address ~1 cases

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
- **Detail**: watertight_mesh: 54 unpaired edges out of 453 total

### R0004 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 8 unpaired edges out of 808 total

### R0005 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 8 unpaired edges out of 922 total

### R0006 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 98a30b16-ef86-47af-9344-a0d469cb4c9f: operation error: kernel error: operation not supported: polygon boolean: 260 total faces exceeds limit (250); watertight_mesh: 7 unpaired edges out of 767 total

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
- **Detail**: watertight_mesh: 562 unpaired edges out of 1205 total

### R0010 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 14 unpaired edges out of 1240 total

### R0011 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 24 unpaired edges out of 1530 total

### R0012 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,boss)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): a4feac21-93fc-4ea0-b4f8-bd4e2943a30f: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 16 (50.0%); watertight_mesh: 24 unpaired edges out of 807 total

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
- **Detail**: watertight_mesh: 121 unpaired edges out of 602 total

### R0016 — PASS

- **Operations**: revolve(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0017 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): cd841cd7-27a4-49b0-84d4-348ad6f3c8fa: operation error: kernel error: operation not supported: polygon boolean: 514 total faces exceeds limit (250); watertight_mesh: 61 unpaired edges out of 1652 total

### R0018 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 12 unpaired edges out of 105 total

### R0019 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 60 unpaired edges out of 297 total

### R0020 — PASS

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

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
- **Detail**: watertight_mesh: 17 unpaired edges out of 1549 total

### R0026 — FAIL

- **Operations**: revolve(circle,boss) + extrude(circle,cut) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 19 unpaired edges out of 437 total

### R0027 — PASS

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut) + revolve(rectangle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0028 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 808 unpaired edges out of 18753 total

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0030 — PASS

- **Operations**: extrude(circle,boss) + revolve(gear,cut) + extrude(circle,cut)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0031 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0032 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 327 unpaired edges out of 7596 total

### R0033 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,boss)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 67 unpaired edges out of 1040 total

### R0034 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: one or both solids have no planar faces. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 77817b8e-a014-4428-b934-3c0c1883dc09: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 16 (50.0%)

### R0036 — PASS

- **Operations**: extrude(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0037 — FAIL

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + extrude(rectangle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 0f4e1b1a-d6e4-4b97-a886-fd5aa6b263fe: operation error: kernel error: operation not supported: polygon boolean: 387 total faces exceeds limit (250); watertight_mesh: 230 unpaired edges out of 1525 total

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 5a4a10ed-f532-45d7-a830-0d2e4d459dd8: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 16 (50.0%); watertight_mesh: 99 unpaired edges out of 2349 total

### R0039 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 339 unpaired edges out of 2697 total

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
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 23 unpaired edges out of 98 total

### R0043 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 0744d042-8bd4-4bd5-a9c2-867c8ce73652: operation error: kernel error: boolean operation failed: non-manifold result: 136 half-edges unpaired out of 480 (28.3%); watertight_mesh: 8 unpaired edges out of 49 total

### R0044 — PASS

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0045 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 224 unpaired edges out of 2632 total

### R0046 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 41 unpaired edges out of 424 total

### R0047 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 52 unpaired edges out of 104 total

### R0048 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 158 unpaired edges out of 637 total

### R0049 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): f8a13e97-18a9-4931-96d4-ab6f305fe383: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 16 (50.0%); watertight_mesh: 76 unpaired edges out of 842 total

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 384 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 38 of 516 triangles have reversed normals; outward_normals: only 478 of 516 triangles (92.6%) have outward normals (need 95%)

### R0052 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 24 unpaired edges out of 558 total

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 16 half-edges unpaired out of 42 (38.1%). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 57 unpaired edges out of 2459 total

### R0054 — FAIL

- **Operations**: revolve(gear,boss) + revolve(gear,cut) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 689 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 18 unpaired edges out of 24162 total

### R0055 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 28 unpaired edges out of 626 total

### R0056 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 247 unpaired edges out of 4697 total

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 254 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0058 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 623d1870-6b99-4ee4-95bd-d4ee1627567c: operation error: kernel error: boolean operation failed: non-manifold result: 58 half-edges unpaired out of 166 (34.9%); watertight_mesh: 20 unpaired edges out of 1333 total

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 93 unpaired edges out of 3011 total

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
- **Detail**: watertight_mesh: 418 unpaired edges out of 1361 total

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 267 unpaired edges out of 3557 total

### R0064 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 11 unpaired edges out of 64 total

### R0065 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(circle,boss)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 252758d7-bd19-432d-ad05-236b452673e9: operation error: kernel error: boolean operation failed: tool encloses or equals blank (concentric); watertight_mesh: 157 unpaired edges out of 2947 total

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
- **Detail**: auto-union-failed (1 warning(s)): Revolve 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 20 half-edges unpaired out of 76 (26.3%). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); consistent_normals: 8 of 516 triangles have reversed normals

### R0069 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 81509434-14b1-44c4-8b64-49662d509751: operation error: kernel error: operation not supported: polygon approx boolean: 1170 total faces exceeds limit; watertight_mesh: 492 unpaired edges out of 3633 total

### R0071 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 257 unpaired edges out of 661 total

### R0072 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): cf875bba-ab5e-4ab1-b49e-bbee4e51f1ca: operation error: kernel error: operation not supported: polygon boolean: 882 total faces exceeds limit (250); watertight_mesh: 669 unpaired edges out of 3204 total

### R0073 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0074 — PASS

- **Operations**: extrude(circle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0075 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 53 unpaired edges out of 1264 total

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon approx boolean: 214 total faces exceeds limit. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0077 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 21 unpaired edges out of 135 total

### R0078 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: boolean operation failed: one or both solids have no planar faces. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0079 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(rectangle,boss)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 1236 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0080 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0081 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Revolve 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 268 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0082 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 127 unpaired edges out of 1367 total

### R0083 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 242 unpaired edges out of 1207 total

### R0084 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 99 unpaired edges out of 2019 total

### R0085 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 502aad62-b1dc-481f-8481-5eb667ca5e5c: operation error: kernel error: operation not supported: polygon approx boolean: 290 total faces exceeds limit; watertight_mesh: 31 unpaired edges out of 1172 total

### R0086 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0087 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 1130 total faces exceeds limit (250). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0088 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 6c7cffa7-dbbc-4a80-95f5-2ce9142c1881: operation error: kernel error: operation not supported: polygon boolean: 1216 total faces exceeds limit (250); watertight_mesh: 49 unpaired edges out of 4095 total

### R0089 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0090 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + revolve(rectangle,cut)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0091 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): eb0cee47-6d85-422b-aae7-06a59c784b86: operation error: kernel error: operation not supported: polygon boolean: 396 total faces exceeds limit (250); watertight_mesh: 289 unpaired edges out of 1544 total

### R0092 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 591 unpaired edges out of 2465 total

### R0093 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 174 unpaired edges out of 513 total

### R0094 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 168 unpaired edges out of 882 total

### R0095 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 60 unpaired edges out of 674 total

### R0096 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 150 unpaired edges out of 1441 total

### R0097 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 18 unpaired edges out of 510 total

### R0098 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): a475deac-8ed0-4250-977a-073489858ce9: operation error: kernel error: operation not supported: polygon approx boolean: 722 total faces exceeds limit; watertight_mesh: 76 unpaired edges out of 2204 total

### R0099 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: multiple-failures
- **Detail**: partial rebuild (1 error(s)): aad1c0ba-a719-4d21-9877-71b4a7f767a0: operation error: kernel error: boolean operation failed: one or both solids have no planar faces

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): c4d347ab-c0b2-4fa3-ba77-68628ee93a4e: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 16 (50.0%); consistent_normals: 12 of 516 triangles have reversed normals

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

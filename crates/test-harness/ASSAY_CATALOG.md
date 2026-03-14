# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-10
Score: **57/110** (57 pass, 49 fail, 4 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| boolean-watertight | 47 | failed |
| pass-genuine | 32 | passed |
| pass-boss-only | 25 | passed |
| cascading-failure | 4 | errored |
| revolve-normals | 1 | failed |
| auto-union-failed | 1 | failed |

## Highest-Leverage Fixes

1. **Fix boolean-watertight** → would address ~47 cases
2. **Fix cascading-failure** → would address ~4 cases
3. **Fix revolve-normals** → would address ~1 cases
4. **Fix auto-union-failed** → would address ~1 cases

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
- **Detail**: watertight_mesh: 132 unpaired edges out of 1885 total (129 boundary, 3 non-manifold)

### R0004 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0005 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0006 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1136 unpaired edges out of 5515 total (1098 boundary, 38 non-manifold)

### R0007 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 0 of 0 triangles have reversed normals; face_range_coverage: no face ranges defined; outward_normals: empty mesh; positive_signed_volume: signed volume = 0.000000e0 (should be > 0); empty mesh: no triangles; expected positive signed volume, got 0.000000e0

### R0008 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut) + extrude(gear,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0009 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2335 unpaired edges out of 8940 total (2057 boundary, 278 non-manifold)

### R0010 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0011 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0012 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,boss)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 252 unpaired edges out of 5785 total (158 boundary, 94 non-manifold)

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
- **Detail**: watertight_mesh: 1059 unpaired edges out of 2451 total

### R0016 — PASS

- **Operations**: revolve(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0017 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 44 unpaired edges out of 1666 total

### R0018 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0019 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 280 unpaired edges out of 1397 total

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

### R0026 — PASS

- **Operations**: revolve(circle,boss) + extrude(circle,cut) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0027 — PASS

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut) + revolve(rectangle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0028 — ERROR

- **Operations**: revolve(circle,boss) + revolve(gear,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

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

### R0032 — ERROR

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0033 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,boss)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 553 unpaired edges out of 3530 total

### R0034 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0035 — PASS

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0036 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2162 unpaired edges out of 15900 total (2111 boundary, 51 non-manifold)

### R0037 — FAIL

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + extrude(rectangle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 253 unpaired edges out of 4813 total (251 boundary, 2 non-manifold)

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 13 unpaired edges out of 2423 total (0 boundary, 13 non-manifold)

### R0039 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 460 unpaired edges out of 4244 total

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
- **Detail**: watertight_mesh: 9 unpaired edges out of 109 total (6 boundary, 3 non-manifold)

### R0043 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 242 unpaired edges out of 3563 total (239 boundary, 3 non-manifold)

### R0044 — PASS

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0045 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 666 unpaired edges out of 7302 total (665 boundary, 1 non-manifold)

### R0046 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0047 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 52 unpaired edges out of 104 total

### R0048 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1762 unpaired edges out of 5786 total (1729 boundary, 33 non-manifold)

### R0049 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 132 unpaired edges out of 3858 total

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 107 unpaired edges out of 1335 total (68 boundary, 39 non-manifold)

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 63 unpaired edges out of 1679 total (59 boundary, 4 non-manifold)

### R0052 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 633 unpaired edges out of 9837 total (627 boundary, 6 non-manifold)

### R0054 — ERROR

- **Operations**: revolve(gear,boss) + revolve(gear,cut) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0055 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0056 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1905 unpaired edges out of 15559 total (1850 boundary, 55 non-manifold)

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 661 unpaired edges out of 4013 total

### R0058 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 127 unpaired edges out of 2933 total (120 boundary, 7 non-manifold)

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 77 unpaired edges out of 2971 total (64 boundary, 13 non-manifold)

### R0060 — PASS

- **Operations**: extrude(gear,boss) + revolve(circle,cut)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0061 — PASS

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0062 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1123 unpaired edges out of 3493 total (1117 boundary, 6 non-manifold)

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 751 unpaired edges out of 7570 total (742 boundary, 9 non-manifold)

### R0064 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0065 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(circle,boss)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

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
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 17 unpaired edges out of 78 total (12 boundary, 5 non-manifold)

### R0069 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 119 unpaired edges out of 955 total

### R0071 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1147 unpaired edges out of 3208 total (1123 boundary, 24 non-manifold)

### R0072 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 3013 unpaired edges out of 15940 total (2968 boundary, 45 non-manifold)

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
- **Detail**: watertight_mesh: 186 unpaired edges out of 2640 total (183 boundary, 3 non-manifold)

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 82 unpaired edges out of 1378 total (80 boundary, 2 non-manifold)

### R0077 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 21 unpaired edges out of 135 total

### R0078 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 303 unpaired edges out of 6741 total (291 boundary, 12 non-manifold)

### R0079 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(rectangle,boss)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: polygon boolean: 10880 total faces exceeds limit (5000). Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0080 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0081 — ERROR

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: cascading-failure
- **Detail**: timeout after 90s

### R0082 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 105 unpaired edges out of 1673 total (65 boundary, 40 non-manifold)

### R0083 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2163 unpaired edges out of 10971 total (2097 boundary, 66 non-manifold)

### R0084 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 562 unpaired edges out of 4043 total (546 boundary, 16 non-manifold)

### R0085 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 226 unpaired edges out of 7689 total (176 boundary, 50 non-manifold)

### R0086 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: pass-boss-only
- **Detail**: 8 oracles passed

### R0087 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 271 unpaired edges out of 7928 total (258 boundary, 13 non-manifold)

### R0088 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 410 unpaired edges out of 7422 total (388 boundary, 22 non-manifold)

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
- **Detail**: watertight_mesh: 883 unpaired edges out of 2859 total (881 boundary, 2 non-manifold)

### R0092 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 675 unpaired edges out of 5704 total (649 boundary, 26 non-manifold)

### R0093 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 726 unpaired edges out of 2024 total (719 boundary, 7 non-manifold)

### R0094 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 1051 unpaired edges out of 5478 total (1049 boundary, 2 non-manifold)

### R0095 — FAIL

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 29 unpaired edges out of 677 total (27 boundary, 2 non-manifold)

### R0096 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 246 unpaired edges out of 1878 total

### R0097 — PASS

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0098 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0099 — PASS

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

### R0100 — PASS

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: pass-genuine
- **Detail**: 8 oracles passed

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

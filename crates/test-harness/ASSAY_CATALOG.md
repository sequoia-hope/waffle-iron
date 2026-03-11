# ASSAY v3 Failure Catalog — WaffleKernel

Generated: 2026-03-10
Score: **5/100** (5 pass, 86 fail, 9 error)

## Summary by Root Cause

| Category | Count | Status |
|---|---|---|
| boolean-watertight | 26 | failed |
| auto-union-failed | 17 | failed |
| revolve-not-supported | 14 | failed |
| merge-incomplete | 14 | failed |
| revolve-normals | 11 | failed |
| cascading-failure | 7 | errored |
| boolean-not-supported | 5 | failed |
| pass-boss-only | 4 | passed |
| pass-genuine | 1 | passed |
| boolean-normals | 1 | failed |

## Highest-Leverage Fixes

1. **Fix boolean-watertight** → would address ~26 cases
2. **Fix auto-union-failed** → would address ~17 cases
3. **Fix revolve-not-supported** → would address ~14 cases
4. **Fix merge-incomplete** → would address ~14 cases
5. **Fix revolve-normals** → would address ~11 cases
6. **Fix cascading-failure** → would address ~7 cases
7. **Fix boolean-not-supported** → would address ~5 cases
8. **Fix boolean-normals** → would address ~1 cases

## Individual Case Results

### R0001 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(circle,boss)
- **Scale**: 4.54e-1 (log: -0.34)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0002 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 2.30e0 (log: 0.36)
- **Category**: pass-boss-only
- **Detail**: 7 oracles passed

### R0003 — ERROR

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut)
- **Scale**: 2.16e2 (log: 2.33)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): 2eafc12b-da2b-48db-ba6f-fd35f4d379fb: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; de728ec4-8e5a-4f93-b1d6-c3cb117000ce: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0004 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 1.42e0 (log: 0.15)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): cfb44fd9-1c48-410d-8f19-4520719a98c3: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 824

### R0005 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 1.70e-1 (log: -0.77)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 10 of 632 triangles have reversed normals; outward_normals: only 535 of 632 triangles (84.7%) have outward normals (need 95%)

### R0006 — ERROR

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 5.80e1 (log: 1.76)
- **Category**: cascading-failure
- **Detail**: timeout after 30s

### R0007 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.21e-4 (log: -3.92)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): a2293d8a-e99d-4e4e-afe0-e2230525f692: operation error: kernel error: operation not supported: cylinder minus box; cbca49a2-9ec4-4f54-b9fe-11e69ea794a3: operation error: kernel error: operation not supported: cylinder minus box; watertight_mesh: 22 unpaired edges out of 26 total

### R0008 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut) + extrude(gear,boss)
- **Scale**: 1.27e2 (log: 2.10)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): d1ffa7d1-7f95-4d10-9de9-40429468acac: operation error: kernel error: operation not supported: revolve: circle profile (torus); watertight_mesh: 1 unpaired edges out of 1148 total; consistent_normals: 67 of 766 triangles have reversed normals; face_range_coverage: empty range at index 9

### R0009 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.05e-4 (log: -3.98)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 8b33009e-78c9-48d0-9ae1-79a980c14567: operation error: kernel error: boolean operation failed: non-manifold result: 256 half-edges unpaired out of 1280; watertight_mesh: 14 unpaired edges out of 16 total; no_degenerate_triangles: 33 of 380 triangles are degenerate

### R0010 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.32e2 (log: 2.12)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 7 unpaired edges out of 1250 total; consistent_normals: 87 of 835 triangles have reversed normals; no_degenerate_triangles: 4 of 835 triangles are degenerate; face_range_coverage: empty range at index 41; outward_normals: only 730 of 835 triangles (87.4%) have outward normals (need 95%)

### R0011 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 7.15e3 (log: 3.85)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 79 of 1012 triangles have reversed normals; no_degenerate_triangles: 1 of 1012 triangles are degenerate; outward_normals: only 760 of 1012 triangles (75.1%) have outward normals (need 95%)

### R0012 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,boss)
- **Scale**: 6.95e1 (log: 1.84)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0013 — PASS

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 7.25e1 (log: 1.86)
- **Category**: pass-boss-only
- **Detail**: 7 oracles passed

### R0014 — PASS

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.28e3 (log: 3.11)
- **Category**: pass-genuine
- **Detail**: 7 oracles passed

### R0015 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + extrude(circle,boss)
- **Scale**: 1.12e-4 (log: -3.95)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): e70af511-c5e7-4187-beea-2a614239be9a: operation error: kernel error: boolean operation failed: non-manifold result: 28 half-edges unpaired out of 658; watertight_mesh: 17 unpaired edges out of 17 total; no_degenerate_triangles: 35 of 404 triangles are degenerate

### R0016 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 5.01e-2 (log: -1.30)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 33f4929c-4c82-4ee9-87ec-dd611e225056: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; watertight_mesh: 256 unpaired edges out of 536 total; consistent_normals: 17 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0017 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + revolve(rectangle,cut)
- **Scale**: 4.03e3 (log: 3.61)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): 5ac440ab-b291-4c80-8ea3-6324b906aac5: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 124 of 516 triangles have reversed normals; outward_normals: only 386 of 516 triangles (74.8%) have outward normals (need 95%)

### R0018 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 4.41e1 (log: 1.64)
- **Category**: boolean-not-supported
- **Detail**: partial rebuild (1 error(s)): 63f3f73d-e749-419d-abd2-24456bc5feb6: operation error: kernel error: operation not supported: cylinder minus box

### R0019 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.31e-2 (log: -1.64)
- **Category**: boolean-not-supported
- **Detail**: partial rebuild (1 error(s)): b63a8293-7e78-4dfa-aaa9-cb87b711b9a1: operation error: kernel error: operation not supported: partial box-cylinder subtract

### R0020 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,boss) + extrude(circle,cut)
- **Scale**: 4.93e1 (log: 1.69)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): 98694c9b-b337-49d1-9310-57903832952b: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 199ed53e-39a8-4187-a0d7-010fb3804ae8: operation error: kernel error: boolean operation failed: tool encloses or equals blank (concentric)

### R0021 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss) + extrude(circle,boss)
- **Scale**: 2.73e-1 (log: -0.56)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0022 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.89e-1 (log: -0.54)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 256 unpaired edges out of 536 total; consistent_normals: 57 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0023 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(rectangle,boss)
- **Scale**: 1.06e3 (log: 3.03)
- **Category**: boolean-not-supported
- **Detail**: partial rebuild (1 error(s)): fd39939c-8a7e-4616-972f-7e74abb4a922: operation error: kernel error: operation not supported: cylinder minus box

### R0024 — PASS

- **Operations**: extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.92e1 (log: 1.28)
- **Category**: pass-boss-only
- **Detail**: 7 oracles passed

### R0025 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,boss) + revolve(circle,cut)
- **Scale**: 2.22e3 (log: 3.35)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; partial rebuild (1 error(s)): ffbbf521-bdf1-4fc3-baee-d043c1df00ba: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0026 — FAIL

- **Operations**: revolve(circle,boss) + extrude(circle,cut) + extrude(rectangle,boss)
- **Scale**: 1.33e-1 (log: -0.88)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): 82b064bf-5260-4a8c-bbaa-2d5b971d9945: operation error: kernel error: operation not supported: revolve: circle profile (torus); 9c7567cb-90b4-491e-a6b8-3a704d14ba23: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0027 — FAIL

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut) + revolve(rectangle,cut)
- **Scale**: 7.82e3 (log: 3.89)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): 430badd4-2068-4322-acca-39f405c7cb01: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 128 of 516 triangles have reversed normals; outward_normals: only 260 of 516 triangles (50.4%) have outward normals (need 95%)

### R0028 — ERROR

- **Operations**: revolve(circle,boss) + revolve(gear,boss)
- **Scale**: 2.36e-2 (log: -1.63)
- **Category**: revolve-not-supported
- **Detail**: no solid — 2 engine error(s): f6040581-253b-4473-a428-afb599f0f87f: operation error: kernel error: operation not supported: revolve: circle profile (torus); 7cb0610f-9ea4-4b4d-87c8-f34dd7a78c97: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0029 — PASS

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 4.21e2 (log: 2.62)
- **Category**: pass-boss-only
- **Detail**: 7 oracles passed

### R0030 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut) + extrude(circle,cut)
- **Scale**: 1.78e-4 (log: -3.75)
- **Category**: boolean-normals
- **Detail**: partial rebuild (1 error(s)): b1b8b53c-3898-45fe-8dcd-019c3237e7fb: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; watertight_mesh: 3 unpaired edges out of 3 total; outward_normals: only 384 of 512 triangles (75.0%) have outward normals (need 95%)

### R0031 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 7.97e-2 (log: -1.10)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 258 unpaired edges out of 534 total; consistent_normals: 72 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0032 — FAIL

- **Operations**: revolve(gear,boss) + extrude(gear,boss)
- **Scale**: 2.33e2 (log: 2.37)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): 42458b1e-e596-43b2-aebe-bd486bcf1c0f: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0033 — FAIL

- **Operations**: extrude(gear,boss) + revolve(gear,boss)
- **Scale**: 1.98e-2 (log: -1.70)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): b83a3f65-ae75-4cf5-96f6-ebc0b3933843: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0034 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut) + revolve(gear,boss)
- **Scale**: 9.45e2 (log: 2.98)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): 45a0fb84-7c09-4ac6-bc42-2b1d2a6a07ab: operation error: kernel error: operation not supported: partial box-cylinder subtract; 312dfd5e-03cc-4139-8dcf-24dec771c60f: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0035 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 1.45e0 (log: 0.16)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); consistent_normals: 136 of 516 triangles have reversed normals; outward_normals: only 259 of 516 triangles (50.2%) have outward normals (need 95%)

### R0036 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 7.33e-2 (log: -1.14)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 8030. Body created as standalone.; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 256 unpaired edges out of 536 total; consistent_normals: 32 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0037 — ERROR

- **Operations**: revolve(gear,boss) + extrude(circle,cut) + extrude(rectangle,cut)
- **Scale**: 2.18e-2 (log: -1.66)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 411b712c-2c2d-4c4d-abdf-4906d433b513: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; b07641ab-fe08-4c8c-845f-2844c5037e78: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 35481419-85f0-40fe-9275-f3b896a39213: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0038 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut) + revolve(circle,cut)
- **Scale**: 1.35e1 (log: 1.13)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): b7fb4a00-2d17-4599-b3d5-14e324758552: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 126 of 516 triangles have reversed normals; outward_normals: only 129 of 516 triangles (25.0%) have outward normals (need 95%)

### R0039 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(gear,cut)
- **Scale**: 2.38e-2 (log: -1.62)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): 5339d09f-3137-4ee1-bbdb-f609580b5383: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0040 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut)
- **Scale**: 2.58e1 (log: 1.41)
- **Category**: revolve-normals
- **Detail**: outward_normals: only 384 of 512 triangles (75.0%) have outward normals (need 95%)

### R0041 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.13e0 (log: 0.05)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 256 unpaired edges out of 536 total; consistent_normals: 6 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0042 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,boss)
- **Scale**: 8.59e2 (log: 2.93)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); consistent_normals: 122 of 516 triangles have reversed normals; outward_normals: only 258 of 516 triangles (50.0%) have outward normals (need 95%)

### R0043 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut) + extrude(gear,cut)
- **Scale**: 1.86e-2 (log: -1.73)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 09332637-f9cf-4018-8014-53c9a6bc54c9: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 52; 7bd84b45-121b-43eb-ba7b-29ae8dd9bec1: operation error: kernel error: boolean operation failed: non-manifold result: 134 half-edges unpaired out of 478

### R0044 — FAIL

- **Operations**: revolve(gear,boss) + revolve(rectangle,cut)
- **Scale**: 3.98e3 (log: 3.60)
- **Category**: revolve-normals
- **Detail**: partial rebuild (1 error(s)): e837ffa0-b2e8-40c9-9cf4-b7fe97244163: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; consistent_normals: 117 of 516 triangles have reversed normals; outward_normals: only 258 of 516 triangles (50.0%) have outward normals (need 95%)

### R0045 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.90e-3 (log: -2.31)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 12 unpaired edges out of 378 total

### R0046 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + revolve(circle,cut)
- **Scale**: 4.62e-1 (log: -0.34)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): 28e43665-fd67-476e-8f4f-15c621e29f58: operation error: kernel error: operation not supported: cylinder minus box; 1cf12932-c2ff-449d-aa90-6929d8c8cf5a: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0047 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,boss)
- **Scale**: 2.09e-4 (log: -3.68)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 19 unpaired edges out of 26 total; consistent_normals: 129 of 516 triangles have reversed normals; outward_normals: only 257 of 516 triangles (49.8%) have outward normals (need 95%)

### R0048 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut)
- **Scale**: 1.20e-1 (log: -0.92)
- **Category**: boolean-not-supported
- **Detail**: partial rebuild (1 error(s)): 69a29728-7cd4-48a1-9d14-1a306dc28566: operation error: kernel error: operation not supported: cylinder minus box

### R0049 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss)
- **Scale**: 4.31e-3 (log: -2.37)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 19 unpaired edges out of 800 total

### R0050 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut) + revolve(circle,boss)
- **Scale**: 1.15e1 (log: 1.06)
- **Category**: revolve-normals
- **Detail**: partial rebuild (2 error(s)): 089aae83-a855-4102-a2b3-91b88330e2e0: operation error: kernel error: operation not supported: revolve: circle profile (torus); 133d439b-a6ba-44e1-82f2-0a49a326205b: operation error: kernel error: operation not supported: revolve: circle profile (torus); consistent_normals: 117 of 516 triangles have reversed normals; outward_normals: only 257 of 516 triangles (49.8%) have outward normals (need 95%)

### R0051 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,cut) + revolve(rectangle,boss)
- **Scale**: 3.37e-3 (log: -2.47)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): 8fc9f51e-ffb7-4003-bd6f-7a9d1a8e510b: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 133 unpaired edges out of 267 total; consistent_normals: 127 of 516 triangles have reversed normals; outward_normals: only 386 of 516 triangles (74.8%) have outward normals (need 95%)

### R0052 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut)
- **Scale**: 5.87e1 (log: 1.77)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): a0a8a309-e068-4508-8f08-6202b433a308: operation error: kernel error: boolean operation failed: non-manifold result: 24 half-edges unpaired out of 580

### R0053 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(rectangle,boss) + revolve(gear,boss)
- **Scale**: 1.49e2 (log: 2.17)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; partial rebuild (1 error(s)): 0896c481-31f7-4f35-a4f2-a72d2c9cfebc: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0054 — ERROR

- **Operations**: revolve(gear,boss) + revolve(gear,cut) + revolve(gear,boss)
- **Scale**: 4.83e1 (log: 1.68)
- **Category**: revolve-not-supported
- **Detail**: no solid — 3 engine error(s): 2ba7aa1c-f668-4361-9fa2-05233091d6ca: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 069d9f11-23f4-4e3c-9c72-68aa73f5544e: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 1a4082b7-08e6-4498-bd67-4393ea5ab0f3: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0055 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(circle,cut)
- **Scale**: 7.27e1 (log: 1.86)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): 72e7f959-a36c-404a-92b6-3be7e35722d0: operation error: kernel error: operation not supported: boolean on revolve solids; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 135 of 516 triangles have reversed normals; outward_normals: only 258 of 516 triangles (50.0%) have outward normals (need 95%)

### R0056 — FAIL

- **Operations**: extrude(circle,boss) + revolve(gear,cut)
- **Scale**: 3.81e-3 (log: -2.42)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): c2cd5329-bf55-4aee-8657-8a7b953b2326: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; watertight_mesh: 97 unpaired edges out of 233 total

### R0057 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 1.04e2 (log: 2.02)
- **Category**: revolve-normals
- **Detail**: partial rebuild (1 error(s)): 1eddf950-5939-4129-8a7e-33c5645962f1: operation error: kernel error: operation not supported: revolve: circle profile (torus); consistent_normals: 1 of 540 triangles have reversed normals

### R0058 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.33e-1 (log: -0.36)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): a7939659-53ba-45a8-8126-209de39d78d4: operation error: kernel error: boolean operation failed: non-manifold result: 72 half-edges unpaired out of 200; consistent_normals: 47 of 972 triangles have reversed normals; no_degenerate_triangles: 1 of 972 triangles are degenerate

### R0059 — FAIL

- **Operations**: extrude(circle,boss) + revolve(circle,boss) + extrude(rectangle,boss)
- **Scale**: 3.80e2 (log: 2.58)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; partial rebuild (1 error(s)): 9d72cc18-ad15-4278-923c-ee0993ab6857: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0060 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,cut)
- **Scale**: 3.77e1 (log: 1.58)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): 87dcceeb-2f91-4dca-b4de-a564997673f1: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0061 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut)
- **Scale**: 1.23e-1 (log: -0.91)
- **Category**: boolean-not-supported
- **Detail**: partial rebuild (1 error(s)): c39b72b8-8df5-4097-81f7-61b7384b63a8: operation error: kernel error: operation not supported: cylinder minus box

### R0062 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss)
- **Scale**: 1.87e-4 (log: -3.73)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: boolean operation failed: non-manifold result: 127 half-edges unpaired out of 2539. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 24 unpaired edges out of 26 total; no_degenerate_triangles: 22 of 260 triangles are degenerate

### R0063 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.74e-3 (log: -2.76)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 14c5eedb-a958-41ad-b1c0-2ebd08cd0d35: operation error: kernel error: operation not supported: cylinder minus box; watertight_mesh: 142 unpaired edges out of 588 total

### R0064 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 1.53e3 (log: 3.19)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): d62863c3-c2f6-4ea9-82ea-7199adb28383: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 64

### R0065 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(circle,boss)
- **Scale**: 8.72e-3 (log: -2.06)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): 35129c28-e0db-4151-91f6-0838b853c0b1: operation error: kernel error: boolean operation failed: tool encloses or equals blank (concentric); 017d570d-0261-495a-b8fc-c6dc0761edc7: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0066 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,boss) + extrude(circle,boss)
- **Scale**: 1.19e0 (log: 0.08)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 104 of 964 triangles have reversed normals

### R0067 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,boss)
- **Scale**: 1.24e-1 (log: -0.91)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0068 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,boss)
- **Scale**: 4.84e-2 (log: -1.31)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 27 unpaired edges out of 747 total; consistent_normals: 114 of 516 triangles have reversed normals; outward_normals: only 386 of 516 triangles (74.8%) have outward normals (need 95%)

### R0069 — FAIL

- **Operations**: extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 1.79e-4 (log: -3.75)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 8dfb4e5b-49d7-4b20-865c-bc9bcd4b97d3: operation error: kernel error: operation not supported: partial box-cylinder subtract; watertight_mesh: 26 unpaired edges out of 30 total; no_degenerate_triangles: 28 of 332 triangles are degenerate

### R0070 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(circle,cut)
- **Scale**: 1.74e-2 (log: -1.76)
- **Category**: revolve-normals
- **Detail**: partial rebuild (2 error(s)): 116f0f25-b9c8-4fb8-82f6-ec970eab7388: operation error: kernel error: operation not supported: boolean on revolve solids; fc073149-ca66-449d-a5b8-37b2674a5f9c: operation error: kernel error: operation not supported: boolean on revolve solids; consistent_normals: 120 of 516 triangles have reversed normals; outward_normals: only 259 of 516 triangles (50.2%) have outward normals (need 95%)

### R0071 — ERROR

- **Operations**: revolve(gear,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.86e-4 (log: -3.73)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): ec49d7df-1704-4f3f-bb29-3e387328f7ed: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 471b860a-ed33-48e5-9a85-f1f2e02e522b: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 7822eee9-8395-42b2-96f4-43de6c01ab09: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0072 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut) + revolve(gear,cut)
- **Scale**: 5.55e-4 (log: -3.26)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 691d1057-0a1a-4b04-b4a4-f4a3dcc17ac7: operation error: kernel error: boolean operation failed: non-manifold result: 495 half-edges unpaired out of 6329; a0c03aee-5af0-41d3-80d5-260665b76d56: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; watertight_mesh: 119 unpaired edges out of 189 total

### R0073 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.97e2 (log: 2.60)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 2 unpaired edges out of 79 total; consistent_normals: 3 of 54 triangles have reversed normals; no_degenerate_triangles: 1 of 54 triangles are degenerate; face_range_coverage: empty range at index 4

### R0074 — FAIL

- **Operations**: extrude(circle,boss) + revolve(rectangle,cut) + extrude(circle,boss)
- **Scale**: 3.40e-1 (log: -0.47)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; merge incomplete: 3 operations produced 3 separate solids (expected 1 merged)

### R0075 — ERROR

- **Operations**: revolve(gear,boss) + extrude(gear,cut)
- **Scale**: 1.89e2 (log: 2.28)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): ffd26745-d939-4b01-a3f0-a1aa0f40edfc: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 92d4bd82-73c9-4a49-86a1-7297e0029ef2: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0076 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(gear,cut) + extrude(circle,boss)
- **Scale**: 2.31e0 (log: 0.36)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; partial rebuild (1 error(s)): 792211a6-badf-47a4-8f2a-443826960f31: operation error: kernel error: boolean operation failed: non-manifold result: 144 half-edges unpaired out of 852; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0077 — ERROR

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut)
- **Scale**: 4.50e3 (log: 3.65)
- **Category**: cascading-failure
- **Detail**: no solid — 2 engine error(s): d2bf3b19-2244-4132-8464-932fc20f024b: operation error: kernel error: operation not supported: revolve: circle profile (torus); 5286cb05-9565-401a-8a5c-1793b7c088ee: GeomRef resolution failed: Cut extrude requires an existing body to subtract from

### R0078 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,boss) + revolve(gear,boss)
- **Scale**: 1.37e-1 (log: -0.86)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): db91e0e3-1315-427b-9173-8b1f9d03f307: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0079 — FAIL

- **Operations**: revolve(circle,boss) + revolve(gear,cut) + extrude(rectangle,boss)
- **Scale**: 7.88e-3 (log: -2.10)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (2 error(s)): a43f8dcd-b450-4d02-ac81-0ff88750f3a0: operation error: kernel error: operation not supported: revolve: circle profile (torus); a0b62186-dc73-445f-b810-d8f942f627d0: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0080 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss)
- **Scale**: 3.91e-2 (log: -1.41)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 2 of 52 triangles have reversed normals; no_degenerate_triangles: 1 of 52 triangles are degenerate

### R0081 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,cut) + revolve(gear,boss)
- **Scale**: 2.24e-1 (log: -0.65)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): dbacb0ff-3e4a-40cf-a494-4ea57581b0f3: operation error: kernel error: boolean operation failed: non-manifold result: 26 half-edges unpaired out of 852; b3407682-c9e7-4d82-a320-521783ae8b88: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial

### R0082 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(circle,cut)
- **Scale**: 6.40e2 (log: 2.81)
- **Category**: revolve-normals
- **Detail**: partial rebuild (1 error(s)): 680332a8-8c0a-4c07-b3ec-beb1a8c9f789: operation error: kernel error: operation not supported: revolve: circle profile (torus); consistent_normals: 151 of 516 triangles have reversed normals; outward_normals: only 258 of 516 triangles (50.0%) have outward normals (need 95%)

### R0083 — FAIL

- **Operations**: extrude(gear,boss) + extrude(gear,cut)
- **Scale**: 1.62e0 (log: 0.21)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 3d1e21cb-e782-4658-a8f3-2bc91acbd5f7: operation error: kernel error: boolean operation failed: non-manifold result: 242 half-edges unpaired out of 1210

### R0084 — FAIL

- **Operations**: revolve(circle,boss) + extrude(gear,boss)
- **Scale**: 9.59e-4 (log: -3.02)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 45c95a46-1388-46f7-8a92-2cd4512c40eb: operation error: kernel error: operation not supported: revolve: circle profile (torus); watertight_mesh: 100 unpaired edges out of 226 total

### R0085 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,boss) + extrude(circle,cut)
- **Scale**: 2.79e0 (log: 0.45)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; partial rebuild (1 error(s)): 536834de-2a2d-4df8-9f08-26e55171716e: operation error: kernel error: operation not supported: partial box-cylinder subtract; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0086 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.96e2 (log: 2.29)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)

### R0087 — FAIL

- **Operations**: revolve(rectangle,boss) + extrude(gear,cut) + extrude(gear,boss)
- **Scale**: 4.04e1 (log: 1.61)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: boolean on revolve solids. Body created as standalone.; partial rebuild (1 error(s)): aa6f38d5-4105-4a7d-b6e3-d4399d46d78b: operation error: kernel error: operation not supported: boolean on revolve solids; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0088 — FAIL

- **Operations**: revolve(gear,boss) + extrude(rectangle,boss) + extrude(rectangle,cut)
- **Scale**: 7.56e2 (log: 2.88)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 8c9e197f-783b-41d2-b469-24d61bd6b540: operation error: kernel error: operation not supported: revolve: profile edge neither radial nor axial; 40e457d5-2845-4cd4-825c-cecd94c84166: operation error: kernel error: boolean operation failed: non-manifold result: 8 half-edges unpaired out of 48

### R0089 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(rectangle,boss) + extrude(circle,boss)
- **Scale**: 1.66e-1 (log: -0.78)
- **Category**: revolve-normals
- **Detail**: consistent_normals: 2 of 36 triangles have reversed normals; outward_normals: only 24 of 36 triangles (66.7%) have outward normals (need 95%)

### R0090 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,boss) + revolve(rectangle,cut)
- **Scale**: 8.30e2 (log: 2.92)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 124 of 516 triangles have reversed normals; outward_normals: only 259 of 516 triangles (50.2%) have outward normals (need 95%)

### R0091 — FAIL

- **Operations**: extrude(gear,boss) + revolve(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.59e-4 (log: -3.80)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): 823da386-51d8-4cd8-bc08-387ff8c5dc7f: operation error: kernel error: operation not supported: revolve: circle profile (torus); merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); watertight_mesh: 20 unpaired edges out of 24 total; consistent_normals: 136 of 516 triangles have reversed normals; outward_normals: only 258 of 516 triangles (50.0%) have outward normals (need 95%)

### R0092 — FAIL

- **Operations**: extrude(circle,boss) + extrude(rectangle,cut) + extrude(gear,boss)
- **Scale**: 1.56e-2 (log: -1.81)
- **Category**: auto-union-failed
- **Detail**: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: partial box-cylinder union. Body created as standalone.; partial rebuild (1 error(s)): f599cb4c-e9be-43d8-b9f8-369f3e33dd7c: operation error: kernel error: operation not supported: cylinder minus box; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged)

### R0093 — FAIL

- **Operations**: extrude(circle,boss) + extrude(gear,cut) + extrude(gear,cut)
- **Scale**: 7.12e-3 (log: -2.15)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (2 error(s)): 33a80866-f4fe-4449-8f7f-32ab158981c0: operation error: kernel error: operation not supported: cylinder minus box; 048a0354-49be-4a1e-93ef-b3903711f1e3: operation error: kernel error: operation not supported: cylinder minus box; watertight_mesh: 46 unpaired edges out of 334 total

### R0094 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(rectangle,cut) + extrude(gear,cut)
- **Scale**: 8.79e2 (log: 2.94)
- **Category**: merge-incomplete
- **Detail**: partial rebuild (1 error(s)): d2f059f0-014b-4cd2-b5b0-458267cdfa89: operation error: kernel error: operation not supported: boolean on revolve solids; merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 128 of 516 triangles have reversed normals; outward_normals: only 130 of 516 triangles (25.2%) have outward normals (need 95%)

### R0095 — ERROR

- **Operations**: revolve(circle,boss) + extrude(rectangle,cut) + revolve(circle,boss)
- **Scale**: 1.14e-3 (log: -2.94)
- **Category**: cascading-failure
- **Detail**: no solid — 3 engine error(s): 85686a4e-09b6-4a3a-a2de-7fe8b58abcad: operation error: kernel error: operation not supported: revolve: circle profile (torus); dea960a4-5a0e-479b-b2f5-f70206fe1a3d: GeomRef resolution failed: Cut extrude requires an existing body to subtract from; 8591a898-4ab2-4717-813b-9ccae3cf2dec: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0096 — FAIL

- **Operations**: extrude(gear,boss) + extrude(rectangle,boss)
- **Scale**: 8.88e-3 (log: -2.05)
- **Category**: boolean-watertight
- **Detail**: watertight_mesh: 313 unpaired edges out of 1730 total; consistent_normals: 47 of 1452 triangles have reversed normals; no_degenerate_triangles: 106 of 1452 triangles are degenerate

### R0097 — FAIL

- **Operations**: extrude(rectangle,boss) + revolve(circle,cut)
- **Scale**: 1.38e1 (log: 1.14)
- **Category**: revolve-not-supported
- **Detail**: partial rebuild (1 error(s)): 8d9c5971-0790-43db-b9eb-90e39ec11412: operation error: kernel error: operation not supported: revolve: circle profile (torus)

### R0098 — FAIL

- **Operations**: extrude(rectangle,boss) + extrude(circle,cut) + extrude(circle,cut)
- **Scale**: 2.21e3 (log: 3.34)
- **Category**: boolean-watertight
- **Detail**: partial rebuild (1 error(s)): 085e3fb5-a3fc-4040-9071-ea84f74662d4: operation error: kernel error: operation not supported: partial box-cylinder subtract; watertight_mesh: 256 unpaired edges out of 536 total; consistent_normals: 2 of 272 triangles have reversed normals; outward_normals: only 144 of 272 triangles (52.9%) have outward normals (need 95%)

### R0099 — FAIL

- **Operations**: extrude(circle,boss) + extrude(circle,cut) + revolve(rectangle,cut)
- **Scale**: 1.09e1 (log: 1.04)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 3 operations produced 2 separate solids (expected 1 merged); consistent_normals: 140 of 516 triangles have reversed normals; outward_normals: only 259 of 516 triangles (50.2%) have outward normals (need 95%)

### R0100 — FAIL

- **Operations**: revolve(rectangle,boss) + revolve(rectangle,cut)
- **Scale**: 2.25e2 (log: 2.35)
- **Category**: merge-incomplete
- **Detail**: merge incomplete: 2 operations produced 2 separate solids (expected 1 merged); consistent_normals: 139 of 516 triangles have reversed normals; outward_normals: only 129 of 516 triangles (25.0%) have outward normals (need 95%)

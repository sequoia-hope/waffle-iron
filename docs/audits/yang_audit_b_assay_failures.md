# Yang Assay Failure-Pattern Audit (Auditor B, Slice #2)

**Audit date**: 2026-04-30
**Branch**: `yang-audit-2026-04-30`
**Slice**: per-case classification of yang_fast assay failures (157 cases)
**HEAD at run**: `3af7fd6` (PR7 R0033 mechanism classification)

## §1. Methodology

### What was run

```
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture
```

The yang_fast suite covers the 157-case subset of the 190-case randomized
assay corpus that excludes 33 cases known to time out at 90s; it uses a
30-second per-case budget and treats overruns as `error`.

The `run_single_case` runner used by yang_fast (`crates/test-harness/src/assay/randomized_runner.rs:140`)
auto-updates `app/tests/cases/assay/results.json` per case as it finishes,
so the post-run `results.json` reflects MY run's first-call result for every
case it observed (155 of 157; the two timeout cases retained their 90s-budget
detail from a prior run because the per-case writer never observed them).

Live run summary:

```
Yang fast: 8/157 passed, 147 failed, 2 errored (skipped 33 known timeouts)
finished in 275.50s
```

### Baseline source

For each case, classification uses the **post-run** `results.json` `detail` string
plus the live status. results.json is the canonical baseline for the
detail field; the live run is canonical for status (yang_fast 30s budget).
Two cases (F0064, R0071) timed out in live but completed-with-failure in the
prior 90s baseline — they are recorded as `TIMEOUT-30s`, not by their 90s
detail content.

### R0080 / R0018 nondeterminism handling

Per `feedback_no_regression_chasing.md`, R0080 and R0018 are flagged as
nondeterministic across runs. In this run:

- **R0018 — pass** (matches the 8/157 baseline; it is the lone R-series pass
  that's not "expected rebuild error").
- **R0080 — fail** with `SELF-INTERSECT` (2 inter-face penetrations,
  face pairs (1,2), (1,5)). This is the typical R0080 fail signature.

I do not deduct R0018/R0080 from any bucket count or rebroker between
runs; I record what I observed.

The brief specified ~150 failing cases. The exact count: **149 of 157
non-passing** (147 fail + 2 error in this run; or 147 fail + 1 error if the
F0064/R0071 yang_fast timeouts are excluded — see §3).

### Classification policy

Per the brief, each case is bucketed by **first failure point**:

1. Pipeline error fired (Yang refused to produce a result for an op) takes
   priority over downstream oracle failures. The error string in the
   `partial rebuild` / `auto-union-failed` prefix is the first failure point.
2. If the Yang pipeline ran to completion but oracles caught problems,
   classify by the highest-severity oracle failure (watertight > euler >
   self-intersect > minimum_triangle_count > others).
3. Live-run TIMEOUT-30s overrides any 90s-budget detail string, since the
   case never ran to completion in the yang_fast budget that auditor B
   was tasked to characterize.

I do not propose fixes; I characterize state per the brief constraint.

### Tooling artifacts

- `/tmp/final_classification.json` — machine-readable per-case JSON
- `/tmp/per_case_table.md` — Markdown rows used in §2 below
- `/tmp/assay_run.log` — full live-run stderr/stdout (~3,200 lines)

These are local artifacts only, not committed.

---

## §2. Per-case classification (157 cases)

Sorted by class, then case_id ascending.

| case_id | status | class | brief detail |
|---------|--------|-------|--------------|
| F0001 | pass | PASS | 9 oracles passed |
| F0003 | pass | PASS | 9 oracles passed |
| F0007 | pass | PASS | 9 oracles passed |
| F0051 | pass | PASS | 9 oracles passed |
| F0053 | pass | PASS | 9 oracles passed |
| R0018 | pass | PASS | 9 oracles passed |
| F0073 | pass | PASS-expected-error | expected rebuild error (axis-touching profile) |
| F0074 | pass | PASS-expected-error | expected rebuild error (axis-touching profile) |
| F0002 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=67 |
| F0004 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=30 |
| F0005 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| F0006 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=0 |
| F0016 | fail | YANG-ERR-twin-validation | twin[33].twin=0; twin.twin=29 |
| F0017 | fail | YANG-ERR-twin-validation | twin[13].twin=0; twin.twin=28 |
| F0018 | fail | YANG-ERR-twin-validation | twin[22].twin=0; twin.twin=8 |
| F0019 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=0 |
| F0020 | fail | YANG-ERR-twin-validation | twin[20].twin=0; twin.twin=23 |
| F0021 | fail | YANG-ERR-twin-validation | twin[25].twin=0; twin.twin=37 |
| F0022 | fail | YANG-ERR-twin-validation | twin[10].twin=0; twin.twin=19 |
| F0023 | fail | YANG-ERR-twin-validation | twin[5].twin=0; twin.twin=34 |
| F0024 | fail | YANG-ERR-twin-validation | twin[48].twin=0; twin.twin=7 |
| F0025 | fail | YANG-ERR-twin-validation | twin[18].twin=0; twin.twin=87 |
| F0026 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=19 |
| F0027 | fail | YANG-ERR-twin-validation | twin[24].twin=0; twin.twin=20 |
| F0028 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=55 |
| F0029 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=0 |
| F0030 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=32 |
| F0041 | fail | YANG-ERR-twin-validation | twin[44].twin=0; twin.twin=123 |
| F0042 | fail | YANG-ERR-twin-validation | twin[92].twin=0; twin.twin=170 |
| F0043 | fail | YANG-ERR-twin-validation | twin[30].twin=0; twin.twin=95 |
| F0045 | fail | YANG-ERR-twin-validation | twin[46].twin=0; twin.twin=133 |
| F0046 | fail | YANG-ERR-twin-validation | twin[18].twin=0; twin.twin=37 |
| F0047 | fail | YANG-ERR-twin-validation | twin[11].twin=0; twin.twin=25 |
| F0048 | fail | YANG-ERR-twin-validation | twin[14].twin=0; twin.twin=93 |
| F0049 | fail | YANG-ERR-twin-validation | twin[8].twin=0; twin.twin=11 |
| F0050 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=62 |
| F0052 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| F0054 | fail | YANG-ERR-twin-validation | twin[22].twin=0; twin.twin=119 |
| F0055 | fail | YANG-ERR-twin-validation | twin[34].twin=0; twin.twin=38 |
| F0056 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=25 |
| F0057 | fail | YANG-ERR-twin-validation | twin[9].twin=0; twin.twin=54 |
| F0058 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=36 |
| F0059 | fail | YANG-ERR-twin-validation | twin[24].twin=0; twin.twin=149 |
| F0060 | fail | YANG-ERR-twin-validation | twin[10].twin=0; twin.twin=72 |
| F0061 | fail | YANG-ERR-twin-validation | twin[341].twin=0; twin.twin=1211 |
| F0062 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| F0066 | fail | YANG-ERR-twin-validation | twin[13].twin=0; twin.twin=66 |
| F0075 | fail | YANG-ERR-twin-validation | twin[10].twin=0; twin.twin=181 |
| F0076 | fail | YANG-ERR-twin-validation | twin[14].twin=0; twin.twin=36 |
| F0086 | fail | YANG-ERR-twin-validation | twin[9].twin=0; twin.twin=169 |
| R0007 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=44 |
| R0008 | fail | YANG-ERR-twin-validation | twin[10].twin=0; twin.twin=43 |
| R0009 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| R0013 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=651 |
| R0014 | fail | YANG-ERR-twin-validation | twin[618].twin=0; twin.twin=2857 |
| R0015 | fail | YANG-ERR-twin-validation | twin[5].twin=0; twin.twin=146 |
| R0016 | fail | YANG-ERR-twin-validation | twin[384].twin=0; twin.twin=3475 |
| R0017 | fail | YANG-ERR-twin-validation | twin[35].twin=0; twin.twin=52 |
| R0019 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=736 |
| R0020 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=42 |
| R0021 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=33 |
| R0022 | fail | YANG-ERR-twin-validation | twin[1008].twin=0; twin.twin=2349 |
| R0023 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=0 |
| R0024 | fail | YANG-ERR-twin-validation | twin[842].twin=0; twin.twin=1306 |
| R0025 | fail | YANG-ERR-twin-validation | twin[562].twin=0; twin.twin=1356 |
| R0027 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=6 |
| R0029 | fail | YANG-ERR-twin-validation | twin[8].twin=0; twin.twin=26 |
| R0031 | fail | YANG-ERR-twin-validation | twin[12].twin=0; twin.twin=283 |
| R0032 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| R0034 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=638 |
| R0035 | fail | YANG-ERR-twin-validation | twin[6].twin=0; twin.twin=36 |
| R0038 | fail | YANG-ERR-twin-validation | twin[6].twin=0; twin.twin=316 |
| R0040 | fail | YANG-ERR-twin-validation | twin[3].twin=0; twin.twin=23 |
| R0041 | fail | YANG-ERR-twin-validation | twin[53].twin=0; twin.twin=87 |
| R0043 | fail | YANG-ERR-twin-validation | twin[2196].twin=0; twin.twin=791 |
| R0044 | fail | YANG-ERR-twin-validation | twin[71].twin=0; twin.twin=455 |
| R0046 | fail | YANG-ERR-twin-validation | twin[70].twin=0; twin.twin=231 |
| R0049 | fail | YANG-ERR-twin-validation | twin[20].twin=0; twin.twin=64 |
| R0050 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=90 |
| R0051 | fail | YANG-ERR-twin-validation | twin[15].twin=0; twin.twin=243 |
| R0054 | fail | YANG-ERR-twin-validation | twin[17].twin=0; twin.twin=19 |
| R0055 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=0 |
| R0058 | fail | YANG-ERR-twin-validation | twin[124].twin=0; twin.twin=483 |
| R0061 | fail | YANG-ERR-twin-validation | twin[153].twin=0; twin.twin=274 |
| R0063 | fail | YANG-ERR-twin-validation | twin[10].twin=0; twin.twin=58 |
| R0066 | fail | YANG-ERR-twin-validation | twin[13].twin=0; twin.twin=11 |
| R0067 | fail | YANG-ERR-twin-validation | twin[30].twin=0; twin.twin=96 |
| R0072 | fail | YANG-ERR-twin-validation | twin[33].twin=0; twin.twin=72 |
| R0074 | fail | YANG-ERR-twin-validation | twin[91].twin=0; twin.twin=3427 |
| R0076 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=45 |
| R0077 | fail | YANG-ERR-twin-validation | twin[136].twin=0; twin.twin=689 |
| R0078 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=27 |
| R0079 | fail | YANG-ERR-twin-validation | twin[2].twin=0; twin.twin=0 |
| R0081 | fail | YANG-ERR-twin-validation | twin[1511].twin=0; twin.twin=1106 |
| R0088 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=0 |
| R0089 | fail | YANG-ERR-twin-validation | twin[17].twin=0; twin.twin=32 |
| R0090 | fail | YANG-ERR-twin-validation | twin[4].twin=0; twin.twin=29 |
| R0092 | fail | YANG-ERR-twin-validation | twin[6].twin=0; twin.twin=11 |
| R0095 | fail | YANG-ERR-twin-validation | twin[187].twin=0; twin.twin=202 |
| R0096 | fail | YANG-ERR-twin-validation | twin[1].twin=0; twin.twin=0 |
| R0005 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0037 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0052 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0075 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0093 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0094 | fail | YANG-ERR-tri-pair-limit | yang_boolean: triangle-pair count exceeds limit (5M) |
| R0047 | fail | YANG-ERR-bijective-oracle | yang_boolean: bijective oracle / unmapped triangles |
| R0004 | fail | REVOLVE-SELF-INTERSECT | revolve self-intersection (Onshape-style rejection) |
| F0031 | fail | WATERTIGHT-unpaired | 17/58 unpaired |
| F0032 | fail | WATERTIGHT-unpaired | 16/44 unpaired |
| F0034 | fail | WATERTIGHT-unpaired | 30/66 unpaired |
| F0035 | fail | WATERTIGHT-unpaired | 3/54 unpaired |
| F0036 | fail | WATERTIGHT-unpaired | 44/94 unpaired |
| F0037 | fail | WATERTIGHT-unpaired | 27/114 unpaired |
| F0038 | fail | WATERTIGHT-unpaired | 10/65 unpaired |
| F0039 | fail | WATERTIGHT-unpaired | 24/102 unpaired |
| F0040 | fail | WATERTIGHT-unpaired | 22/104 unpaired |
| R0001 | fail | WATERTIGHT-unpaired | 9/4068 unpaired |
| R0002 | fail | WATERTIGHT-unpaired | 41/136 unpaired |
| R0006 | fail | WATERTIGHT-unpaired | 24/57 unpaired |
| R0011 | fail | WATERTIGHT-unpaired | 27/2487 unpaired |
| R0030 | fail | WATERTIGHT-unpaired | 3/102 unpaired |
| R0033 | fail | WATERTIGHT-unpaired | 23/136 unpaired |
| R0045 | fail | WATERTIGHT-unpaired | 88/950 unpaired |
| R0057 | fail | WATERTIGHT-unpaired | 9/1083 unpaired |
| R0060 | fail | WATERTIGHT-unpaired | 18/123 unpaired |
| R0064 | fail | WATERTIGHT-unpaired | 12/1800 unpaired |
| R0068 | fail | WATERTIGHT-unpaired | 15/75 unpaired |
| R0069 | fail | WATERTIGHT-unpaired | 5/70 unpaired |
| R0082 | fail | WATERTIGHT-unpaired | 10/3044 unpaired |
| R0084 | fail | WATERTIGHT-unpaired | 22/212 unpaired |
| R0086 | fail | WATERTIGHT-unpaired | 30/126 unpaired |
| R0087 | fail | WATERTIGHT-unpaired | 6/3468 unpaired |
| R0098 | fail | WATERTIGHT-unpaired | 11/2998 unpaired |
| F0009 | fail | SELF-INTERSECT | 8 inter-face penetrations |
| F0010 | fail | SELF-INTERSECT | 8 inter-face penetrations |
| F0011 | fail | SELF-INTERSECT | 2 inter-face penetrations |
| F0012 | fail | SELF-INTERSECT | 2 inter-face penetrations |
| F0013 | fail | SELF-INTERSECT | 5 inter-face penetrations |
| F0015 | fail | SELF-INTERSECT | 1 inter-face penetrations |
| R0036 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| R0039 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| R0042 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| R0048 | fail | SELF-INTERSECT | 5 inter-face penetrations |
| R0056 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| R0073 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| R0080 | fail | SELF-INTERSECT | 2 inter-face penetrations (nondeterministic) |
| R0083 | fail | SELF-INTERSECT | 3 inter-face penetrations |
| R0097 | fail | SELF-INTERSECT | 10 inter-face penetrations |
| F0008 | fail | EULER-VIOLATION | χ=4 (exp 2) |
| F0014 | fail | EULER-VIOLATION | χ=4 (exp 2) |
| F0033 | fail | EULER-VIOLATION | χ=6 (exp 4) |
| F0044 | fail | EULER-VIOLATION | χ=22 (exp 2) |
| R0062 | fail | EULER-VIOLATION | χ=4 (exp 2) |
| R0091 | fail | EULER-VIOLATION | χ=4 (exp 2) |
| F0064 | error | TIMEOUT-30s | timed out in 30s yang_fast budget |
| R0071 | error | TIMEOUT-30s | timed out in 30s yang_fast budget |

---

## §3. Histogram by class

| Class | Count | % of 157 |
|---|---:|---:|
| **YANG-ERR-twin-validation** | **92** | **58.6%** |
| **WATERTIGHT-unpaired** | **26** | **16.6%** |
| **SELF-INTERSECT** | **15** | **9.6%** |
| YANG-ERR-tri-pair-limit | 6 | 3.8% |
| EULER-VIOLATION | 6 | 3.8% |
| PASS | 6 | 3.8% |
| PASS-expected-error | 2 | 1.3% |
| TIMEOUT-30s | 2 | 1.3% |
| YANG-ERR-bijective-oracle | 1 | 0.6% |
| REVOLVE-SELF-INTERSECT | 1 | 0.6% |
| **Total** | **157** | 100.0% |

### Dominant buckets (≥10% of population)

- **YANG-ERR-twin-validation: 92 cases (58.6%)** — by a wide margin the largest single signature.
- **WATERTIGHT-unpaired: 26 cases (16.6%)** — second-largest, downstream of survival-but-not-watertight.
- **SELF-INTERSECT: 15 cases (9.6%)** — close to the 10% threshold; notable as a **post-Yang oracle** failure (Yang produced output, but oracle caught penetrations).

Combined, the top three classes account for **133/157 = 84.7%** of cases.

### F-series vs R-series breakdown of dominant buckets

| Class | F-series | R-series |
|---|---:|---:|
| YANG-ERR-twin-validation | 42 | 50 |
| WATERTIGHT-unpaired | 9 | 17 |
| SELF-INTERSECT | 6 | 9 |

Both the YANG-ERR-twin-validation and WATERTIGHT-unpaired buckets are
roughly proportional to the corpus split (F/R = 90/100 in the full assay,
~57/100 in yang_fast after skips). No bucket is concentrated in only one
operation family.

---

## §4. Spot-checked case analyses

For each dominant bucket, three cases analysed in depth.

### 4.1 YANG-ERR-twin-validation (bucket size: 92)

**Error site**: `validate_yang_result_topology` in
`crates/kernel/src/boolean/yang_integration.rs:979`.
Specifically the twin-symmetry check at line 1029-1031:
```
let twin_he = &arena.half_edges[he.twin.0];
if twin_he.twin.0 != i { ... return Err ... }
```

**Why `twin = 0`**: Half-edges are constructed in
`crates/kernel/src/boolean/topology_extract.rs:224-231` with
`twin: HalfEdgeIdx(0)` as a placeholder ("set during twin pairing").
If the pairing loop in `flood_fill_patches`
(`topology_extract.rs:825+`) finds zero reverse candidates for a
forward half-edge (the `[]` arm at line 876-892), it increments
`unpaired_count` and leaves the half-edge with the unset
`HalfEdgeIdx(0)` sentinel. The validator then sees `twin = 0` and
flags it.

The reported `twin.twin = Y` in the error is just whatever value
HE[0]'s twin happens to hold by coincidence — a non-meaningful number.
This signature **always means "flood_fill_patches left an unpaired
half-edge"**.

#### Case F0002 (smallest spot-check)

- `app/tests/cases/assay/F0002.meta.json`: 2 ops, `extrude(rectangle,boss)+extrude(rectangle,boss)` ("Small cross-shaped").
- Both rectangles are extruded on the same XY plane — their bottom faces are coplanar.
- Detail: `Extrude 2: Auto-union failed: ... yang_boolean: result validation failed: half_edge[3].twin = 0 but twin.twin = 67 (expected 3). Body created as standalone.; merge incomplete: 2 operations produced 2 separate solids (expected 1 merged)`.
- The 2nd extrude was created as a standalone solid because the union failed; the oracle then catches "merge incomplete" but the FIRST failure is the Yang twin-validation.

#### Case F0005

- 2 ops, `extrude(rectangle,boss)+extrude(rectangle,boss)`, identical rectangle profile but different extrude depths.
- Detail: `twin[1].twin=0; twin.twin=0` (both indices zero — HE[0]'s twin was also unset, so the sentinel collision shows up).
- Same coplanar-bottom-face geometry as F0002.

#### Case F0086

- 6 ops, `extrude(circle,boss) + 5×extrude(circle,cut)` — the swiss-cheese disc fixture.
- Detail: `twin[9].twin=0; twin.twin=169`.
- Cross-reference: `f0086_investigation.md` memory file independently flagged this as "polygon boolean fallback for coaxial holes leaves unpaired edges." Confirms the same root mechanism (unpaired twins surfacing through validate_yang_result_topology).
- Triggered from `flood_fill_patches`-derived topology after coaxial-hole subtraction.

**Cross-reference to prior audit**: The cherchi port audit
(`docs/audits/cherchi_port_audit.md`) Cluster I "Predicate-kernel
symptom-paper-over" enumerates 11 findings (A-01, A-02, B-06,
C-01/C-02/C-05/C-07, C-08/C-09, C-11/C-13) where defensive guards in
the Cherchi port silently drop or duplicate triangles. Any such
defensive drop reduces the per-edge directed_he candidate set,
which is exactly what flood_fill_patches's twin pairing depends on.
The 92-case dominance of this bucket is consistent with a single
upstream input-corruption site feeding flood_fill_patches with
non-conformal triangulation across most fixtures.

**Cross-reference to PR1-7**: The bijective oracle work
(`tessellation_bounded_residuals.md` §§1-11 and PR3 corpus dump in
`tessellation_pr3_corpus_dump.md`) measured per-case
non-bijective face pair counts; only 14/81 linear-bounded cases had
nonzero non-bijective pairs. Most of the 92 YANG-ERR-twin cases
have bijective tessellation but still fail at flood_fill_patches —
suggesting bijectivity is necessary but not sufficient for
flood_fill twin-pairing success. (R0033 has 23 unpaired edges
and 2 non-bijective pairs; the bijectivity gap can't fully account
for the unpaired-twin count.)

### 4.2 WATERTIGHT-unpaired (bucket size: 26)

**Error site**: `watertight_mesh` oracle in test-harness, fired AFTER
the Yang pipeline returned a successful BooleanResult.

**Why post-Yang**: For these cases, validate_yang_result_topology
passed (twin-symmetry intact), so the B-Rep arena is internally
consistent. The cached_render_mesh produced by retessellation
(`yang_integration.rs:900`) is what the oracle measures, and IT has
unpaired edges. The B-Rep is OK; the retessellation is not.

#### Case R0030 (lowest absolute count)

- 2 ops, revolve+revolve (per the `multiple-failures` category and detail format).
- Detail: `watertight_mesh: 3 unpaired edges out of 102 total; mesh_euler_characteristic: V(44) - E(102) + F(67) = 9 (expected 2)`.
- 3 unpaired edges, but Euler characteristic χ=9 is far from the expected 2 — suggests multiple holes or non-2-manifold topology, not a simple pinhole. (For a closed 2-manifold sphere-like solid, χ=2; χ=9 implies V-E+F is wildly off.)

#### Case R0033 (PR4 anchor candidate from prior work)

- 2 ops, revolve(rectangle,boss) + revolve(gear,cut). T-junction test case.
- Detail: `23 unpaired edges out of 136 total; consistent_normals: 2 of 83 reversed; 10 inter-face penetrations; χ=-5 (exp 2)`.
- Cross-reference: `tessellation_pr3_corpus_dump.md` ranked R0033 as the smallest-mesh-with-multiple-non-bij-pairs (12 face pairs total, 2 non-bijective). PR3-PR7 narrowed the mechanism to revolve-primitive cap-pool sharing in tessellation; not yet fixed (per `cherchi_port_audit.md` D-10 unblocked-on-bijective-tessellation).

#### Case R0001 (smallest unpaired-rate)

- 2 ops, extrude(gear,boss)+extrude(circle,boss).
- Detail: `9 unpaired edges out of 4068 total; 10 inter-face penetrations; χ=3 (exp 2)`.
- 0.22% unpaired rate on a large mesh — consistent with a single localized seam (e.g., a single intersection chain that didn't close), not systemic brokenness.

**Cross-reference to prior audit / specs**: The 26 WATERTIGHT-unpaired
cases break into:
- **High-rate ( >30%)**: F0032 (36%), R0006 (42%), F0034 (45%), F0036 (47%), R0002 (30%) — these meshes are nearly half-broken, suggesting macroscopic survival selection error, not edge-case stitching.
- **Low-rate (<1%)**: R0001, R0011, R0057, R0064, R0082, R0087, R0098 — large meshes (>1000 edges) with single-digit unpaired counts, consistent with localized intersection-curve seam closure issues.

The dichotomy suggests *at least two distinct mechanisms* under this single bucket label — see §5 finding YB-04.

### 4.3 SELF-INTERSECT (bucket size: 15)

**Error site**: `no_self_intersection` oracle, AFTER Yang completed
successfully.

**Why post-Yang**: Yang produced a B-Rep that twin-validates and is
watertight — but the resulting mesh has triangles from different B-Rep
faces that physically intersect (interpenetrate). Per Yang §4.4.3 the
mesh boolean's watertightness is supposed to imply correctness; clearly
that's not what's happening here. Possible: post-Yang retessellation at
Render LOD reintroduces face-pair penetrations the Boolean LOD didn't
have. This is consistent with the comment at
`yang_integration.rs:892-901`:
```
The sub-triangle mesh from 16-segment Boolean LOD has chord error on
curved surfaces that causes inter-face triangle penetrations
detected by the self-intersection oracle. Retessellation at 64-segment
Render LOD matches legacy pipeline quality and eliminates these artifacts.
```
…but for these 15 cases, retessellation evidently does NOT eliminate the
artifacts.

#### Case F0015 (smallest signature)

- 2 ops, extrude+extrude.
- Detail: `1 inter-face triangle penetration, face pair (7,11); χ=4 (expected 2)`.
- Single penetration, but χ=4 (a torus-like topology, two disjoint genus-1 surfaces? or two disjoint shells?) — suggests the mesh is more wrong than the single penetration count suggests.

#### Case R0080 (known nondeterministic)

- Detail this run: `2 inter-face triangle penetrations, face pairs (1,2), (1,5)`.
- Per `feedback_no_regression_chasing.md`, R0080 alternates between pass and fail across runs. cherchi_port_audit.md Cluster II offers a partial explanation: `SimplexIntersection` state-space collapse (4-state cinolib enum collapsed to 2-state Rust enum) can produce inconsistent intersection sets when share-vertex situations are classified differently across runs.

#### Case R0042 (volume-magnitude category)

- Detail: `10 inter-face triangle penetrations; volume_magnitude outside [6.34e0, 6.34e16] for scale 8.59e2; χ=4 (expected 2)`.
- Volume is 6.67e-1 vs expected ≥6.34e0 — solid is **smaller** than expected by ~10×, suggesting a chunk was wrongly classified Outside and dropped. The oracle's cascading: too-small + penetrations + wrong Euler all rooted in the same survival-selection error.

**Cross-reference to prior audit**: cherchi_port_audit.md Cluster II
(SimplexIntersection state-space collapse — 5 findings B-03, B-04,
B-05, B-12, B-14) directly predicts intersection over-detection at
shared-vertex / shared-edge configurations, which would generate
extra "penetrations" in the post-Yang mesh.

---

## §5. Findings

Each finding labelled `YB-NN`. No fixes proposed (per brief constraint).

### YB-01 — flood_fill_patches twin-pairing produces unpaired half-edges in 92/157 cases

- **Severity**: CORRECTNESS-BUG
- **Affected case count**: 92 of 157 (58.6%); 92 of 149 failing+errored (61.7%)
- **Code site**: `crates/kernel/src/boolean/topology_extract.rs:825-892` (twin pairing in `flood_fill_patches`); `crates/kernel/src/boolean/yang_integration.rs:1029-1056` (twin-symmetry validator that catches it)
- **Suggested investigation direction**: This is a SYMPTOM, not a root cause. The twin-pairing arms only see what the upstream subdivision + survival selection feeds them. Investigating WHY survived triangles produce a non-conformal directed-edge graph requires looking upstream at `subdivide_mesh_pair` (Cherchi-port output) and the survival-selection step (`label_cells` + `survival_detect`).
- **Cross-reference**: cherchi_port_audit.md Cluster I (12 findings; defensive guards in Cherchi port silently corrupting triangulation output) is consistent with this hypothesis. This bucket is also the dominant "canonical Yang error" — the bucket the brief explicitly named.

### YB-02 — twin index value is uninformative; HE[i].twin = 0 always means "unpaired"

- **Severity**: UNKNOWN (informational)
- **Affected case count**: All 92 in YB-01
- **Code site**: `topology_extract.rs:224-231` (HE constructed with `twin: HalfEdgeIdx(0)` placeholder)
- **Description**: The error message "twin = 0 but twin.twin = Y (expected X)" looks like it carries information about which other HE got wrongly paired with HE[i]. It does not. The `twin = 0` part is the placeholder sentinel; `twin.twin = Y` is just whatever HE[0]'s real twin is. The `(expected X)` is `i`. Diagnostically: the message is equivalent to "HE[i] never got paired."
- **Cross-reference**: This pattern is what makes the twin-validation bucket so large — every flood_fill_patches unpaired case that survives `auto-union` collapse rolls into this single signature. From the auditor brief's perspective it looks like one coherent bug-class; from the codebase's it's the single error message that aggregates many unpaired-twin causes.

### YB-03 — F-series and R-series both contribute to YB-01; not a primitive-tessellator bug

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 42 F-series + 50 R-series = 92
- **Description**: F-series uses extrude operations (mostly polygon-soup tessellation); R-series mixes extrude+revolve (revolve uses primitive-dispatch tessellation). Both flow through the same `flood_fill_patches` twin pairing and both fail at similar rates. The bug is not in either tessellator alone; it's in the shared mesh-boolean stage that consumes their output.
- **Cross-reference**: Consistent with cherchi_port_audit.md's identification of the predicate kernel and `subdivide_mesh_pair` as cross-cutting weak points.

### YB-04 — WATERTIGHT-unpaired bucket has bimodal unpaired-rate distribution

- **Severity**: UNKNOWN-NEEDS-INVESTIGATION
- **Affected case count**: 26 cases; ~9 high-rate (>30%) and ~7 low-rate (<1%); rest in middle
- **Description**: The same bucket label hides at least two distinct mechanisms. High-rate cases (e.g., R0006 at 42%, F0036 at 47%) suggest entire face groups got wrongly classified Outside; low-rate cases (R0001 at 0.22%) suggest single-seam closure issues. Treating these as one fix-target may be premature.
- **Suggested investigation direction**: Bisect by mesh size and unpaired/total ratio. Spot-check both ends to confirm the mechanism difference before grouping for triage.

### YB-05 — SELF-INTERSECT is a post-Yang-success signature

- **Severity**: CORRECTNESS-BUG
- **Affected case count**: 15 of 157 (9.6%)
- **Code site**: `crates/kernel/src/boolean/yang_integration.rs:892-901` — comment claims Render LOD retessellation eliminates these; for these 15 cases it does NOT.
- **Description**: Yang produces a valid B-Rep (twin-symmetric, watertight). The retessellation at LOD::Render then produces a render mesh where triangles from different B-Rep faces interpenetrate. Either (a) the B-Rep itself has impossible adjacency that retessellation faithfully rendered as penetration, or (b) the retessellation introduced the penetration via per-face surface sampling without inter-face awareness.
- **Cross-reference**: cherchi_port_audit.md Cluster II (SimplexIntersection state-space collapse) is one mechanism that could cause silently-wrong intersection sets that twin-validate but don't represent valid geometry.

### YB-06 — YANG-ERR-tri-pair-limit hits 6 large R-series cases (R0005, R0037, R0052, R0075, R0093, R0094)

- **Severity**: PERFORMANCE-DRIFT (tractable correctness-related)
- **Affected case count**: 6
- **Code site**: `crates/kernel/src/boolean/yang_integration.rs:1135-1146`. `MAX_YANG_TRI_PAIRS` from `crates/kernel/src/units.rs`.
- **Description**: Triangle-pair products (n_a × n_b) of 6.9M, 12M, 14M, 15M, 36M, and 85M exceeded the 5M limit. All six cases involve gear or polygon profiles at large scales (R0005's profile is gear-shape extruded to a multi-MB mesh). The limit is a defensive cutoff against pathological subdivision blowup; raising it likely just delays the timeout, since label_cells is O(n_a × n_b).
- **Suggested investigation direction**: This bucket is unlikely to be unblocked by a YB-01 fix; it requires either coarser tessellation defaults for large meshes, or BVH-based pair filtering inside subdivide_mesh_pair (Cherchi 2022 §4.1 spatial structure).

### YB-07 — YANG-ERR-bijective-oracle (R0047) is a single-case bucket

- **Severity**: UNKNOWN-NEEDS-INVESTIGATION
- **Affected case count**: 1 (R0047)
- **Code site**: `yang_boolean: bijective oracle / unmapped triangles` (path: bijective tessellation guard inside Yang stage 1)
- **Description**: Singleton; not statistically distinct from random outliers. Useful as a *minimal reproducer* of the bijective-tessellation gap for a fix targeting that path, but not a leverage target on its own.

### YB-08 — REVOLVE-SELF-INTERSECT (R0004) is intentional rejection per current code

- **Severity**: DELIBERATE-DIVERGENCE
- **Affected case count**: 1 surfaced as primary failure (R0004); 2 surfaced as PASS-expected-error (F0073, F0074)
- **Description**: F0073 and F0074 PASS because they're `expect_rebuild_error: true` (axis-touching profile is a known invalid revolve input). R0004 has a similar revolve-profile-straddles-axis condition but its meta does NOT set `expect_rebuild_error`, so the failure surfaces. This is an oracle/generator inconsistency, not a Yang bug.
- **Suggested investigation direction**: Generator-side fix (assay_gen meta annotation), not a Yang-pipeline fix. Will not move the needle on Yang correctness.

### YB-09 — TIMEOUT-30s differs from baseline TIMEOUT 90s for F0064 and R0071

- **Severity**: PERFORMANCE-DRIFT
- **Affected case count**: 2 cases (F0064, R0071); both excluded from any "passing-when-Yang-fixes-X" leverage analysis since they fail to complete the pipeline at all
- **Description**: F0064 ran to completion (with twin-validation failure) at 90s but timed out at 30s. R0071 similar but with revolve self-intersection. These are at the boundary of the budget; the 30s yang_fast cutoff is calibrated for fast feedback, not for letting heavy cases finish.
- **Suggested investigation direction**: Don't change the budget; instead acknowledge that yang_fast is a 155-case suite and treat F0064/R0071 as known slowcases.

### YB-10 — SELF-INTERSECT count distribution: 9 of 15 cases hit oracle cap of 10

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 9 (R0036, R0039, R0042, R0056, R0073, R0083 has 3, R0097, plus F0009/F0010 at 8)
- **Description**: The oracle reports up to 10 inter-face penetrations (truncated). 9 of 15 SELF-INTERSECT cases hit that cap, meaning the actual penetration counts could be much higher. The bucket size (15) is a lower bound on cases with self-intersection; cases marked WATERTIGHT-unpaired or YANG-ERR-twin-validation may also have self-intersections that are masked by an earlier-firing oracle.
- **Suggested investigation direction**: When YB-01 (twin-validation) is reduced, the SELF-INTERSECT bucket will likely grow as cases that previously failed earlier surface their downstream penetrations. The current bucket size of 15 should not be treated as an upper bound.

### YB-11 — EULER-VIOLATION cases (6) all involve χ off by ≥2; likely missing/duplicate handles

- **Severity**: UNKNOWN-NEEDS-INVESTIGATION
- **Affected case count**: 6 (F0008, F0014, F0033, F0044, R0062, R0091)
- **Description**: For a closed orientable 2-manifold with g handles and h holes, χ = 2 - 2g - h. Observed deviations: F0008 χ=4 (expected 2; +2), F0014 χ=4 (+2), F0033 χ=6 (expected 4; +2), F0044 χ=22 (vs expected 2; +20 — extreme), R0062 χ=4 (+2), R0091 χ=4 (+2). Most are off by exactly +2, consistent with a single extra disconnected component; F0044 is dramatically wrong, suggesting many spurious components or a degenerate mesh.
- **Suggested investigation direction**: F0044 stands out as an outlier and may have a separate root cause (revolve-normals category); the +2-offset cluster (5 cases) all consistent with auto-union producing a separate component instead of merging.

### YB-12 — F0001/F0003/F0007 pass: 1-op or trivially merged 2-op cases only

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 6 PASS + 2 PASS-expected-error
- **Description**: Of the 8 yang_fast passes:
  - F0001: 2 identical extrudes → trivial merge (no real boolean).
  - F0003: 2 ops (per category `pass-boss-only`); investigation needed.
  - F0007, F0051, F0053: per category, all `pass-boss-only` — same trivial-boss pattern.
  - R0018: per category `pass-genuine` — the lone non-trivial pass; non-deterministic flapper per memory.
  - F0073, F0074: pass via expected-rebuild-error.
- This means the Yang pipeline has effectively zero confirmed-genuine non-trivial passes that reproduce reliably across runs. R0018 is the lone candidate, and it's flagged nondeterministic.
- **Cross-reference**: `yang_implementation_status.md` memory notes "0/157 honest baseline" was the post-cleanup state. The current 8/157 includes "trivial passes" that don't exercise the boolean stage.

### YB-13 — Live-run nondeterminism observed: details for unpaired-edge counts shift across runs

- **Severity**: UNKNOWN (informational)
- **Affected case count**: ~5 cases in WATERTIGHT-unpaired; counts of unpaired edges drifted between consecutive runs (e.g., R0001 went from 20 to 9 unpaired; R0033 went from 23 to 23, but face-pair list contents changed; R0098 went from 7 to 11).
- **Description**: The classification BUCKET is stable per case (R0001 stayed WATERTIGHT-unpaired; R0033 stayed WATERTIGHT-unpaired), but the *count* and *which edges* are reported drifts. Suggests a HashMap/HashSet iteration order somewhere in the pipeline produces deterministic structure but nondeterministic detail. Per `feedback_no_regression_chasing.md` this is observed-not-fixed.
- **Cross-reference**: cherchi_port_audit.md mentions BTreeMap-based determinism in v_map (Cherchi 2020 §5 uses `btree_map<genericPoint*>`) but other pipeline stages may still use hash-iteration that's not seeded.

### YB-14 — F0086 swiss-cheese disc fails YANG-ERR-twin, NOT WATERTIGHT-unpaired

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 1 (F0086)
- **Description**: Memory note `f0086_investigation.md` framed F0086 as "Polygon boolean fallback for coaxial holes leaves unpaired edges. Recommended fix: analytical coaxial multi-hole builder." But the current run shows F0086 fails at YANG-ERR-twin-validation (Yang refuses to produce output), not at the watertight oracle. Either the failure mode shifted between when the memory was written and now, or the original "unpaired edges" referred to twin pairing (HE-level), not mesh edges (oracle-level), and the memory's nomenclature was ambiguous.
- **Suggested investigation direction**: Update or correct the memory file. The "polygon boolean fallback" theory may be outdated.

### YB-15 — Live results.json is updated per-case during yang_fast runs; baseline drift risk

- **Severity**: UNKNOWN (operational)
- **Affected case count**: All 155 cases that completed in this run
- **Code site**: `crates/test-harness/src/assay/randomized_runner.rs:140-152` (`run_single_case` calls `update_single_result`)
- **Description**: `results.json` is auto-updated per-case as yang_fast runs. Any future yang_fast invocation overwrites the per-case detail. This is fine for the GUI's intended use (showing latest status) but means **`results.json` cannot be relied on as a stable historical baseline**; it's a snapshot of the most recent run. Audit-report data should be derived from the same run that produced the histogram, not from `results.json` at audit time minus run time.
- **Suggested investigation direction**: This audit's per-case table was generated immediately post-run from in-memory state piped through results.json; future audits should snapshot results.json under a per-audit filename (e.g., `results-2026-04-30-auditor-b.json`) before running additional assays.

### YB-16 — F0064 lost detail signal due to TIMEOUT-30s reclassification

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 1 (F0064)
- **Description**: F0064's 90s-budget detail (in pre-run results.json) was a 4-error YANG-ERR-twin-validation rollup. Under yang_fast it timed out at 30s, so the auditor classified it as TIMEOUT-30s. If the leverage analysis (§6) considered F0064 as a YB-01 instance, that would inflate that bucket by 1. Conservatively counted it under TIMEOUT-30s.
- **Suggested investigation direction**: For full-budget assay analyses, this case is a YANG-ERR-twin instance; for yang_fast-only analyses, it's a timeout. Be explicit about which budget any aggregate count is computed under.

### YB-17 — REVOLVE-SI-CASCADE hidden by yang_fast run completing

- **Severity**: UNKNOWN (informational)
- **Affected case count**: 0 currently visible in yang_fast 30s; baseline 90s shows 1 (R0050) as full revolve-SI cascade in errored state
- **Description**: The R0050 entry in 90s baseline shows `"no solid — 3 engine error(s): ... revolve self-intersection ..."` (pure error, no oracle data). My yang_fast run produced a YANG-ERR-twin-validation for R0050 instead. This is because some revolve operations have self-intersection geometry that's near-borderline; tighter scheduling or different early-pass exit creates timing-dependent behavior. The bucket sum is unaffected (R0050 is in YB-01 in this run), but the case's failure mechanism could change between runs.

### YB-18 — Aggregating "cause-of-cause" requires upstream visibility outside this audit's slice

- **Severity**: UNKNOWN (scope limit)
- **Description**: This slice classifies by surface error message. It does not (and per brief, cannot) trace each twin-validation failure back to the specific subdivide_mesh_pair / label_cells / survival decision that introduced the unpaired edge. That tracing requires per-case PR_DEBUG=1 instrumentation per `feedback_anchor_before_fix.md` and is outside this slice. The 92-case YB-01 bucket may decompose into 4-6 root-cause sub-buckets when traced — auditor C's Cherchi-port slice is the natural place for that.

---

## §6. Leverage analysis

Leverage = (failing-case count) × (tractability), per the brief.

### Tier A: high count × moderate tractability

#### YB-01 (YANG-ERR-twin-validation) — 92 cases

Fixing the upstream cause of unpaired half-edges in
`flood_fill_patches` would, *if* the unpaired edges are the only
problem in those 92 cases, produce some unknown number of new
oracle-completed cases. **But**: per YB-10, those cases would then
re-fail at downstream oracles (SELF-INTERSECT, WATERTIGHT-unpaired,
EULER-VIOLATION) — the bucket count is an *upper bound* on cases
that would PASS, not a count of cases that would PASS.

A conservative reading: fixing YB-01 unblocks per-case progression
to the next failure layer, allowing those 92 cases to *contribute
data* to other oracles. Whether they then pass or fail at the next
layer is a separate question.

Per the brief constraint and `feedback_no_last_bug.md`, I am NOT
claiming "fixing YB-01 unblocks 92 cases." I am claiming "92 cases
share a single error signature, and the upstream cause of that
signature is the highest-count single class to investigate."

#### YB-04 (WATERTIGHT-unpaired bimodal) — 26 cases

Two sub-mechanisms; investigation should split before any
"unblock" estimate. The high-rate sub-bucket (~9 cases) likely
shares a survival-classification bug; the low-rate sub-bucket
(~7 cases) likely shares an intersection-curve closure bug.
These may be partially independent of YB-01.

### Tier B: moderate count × variable tractability

#### YB-05 (SELF-INTERSECT, post-Yang) — 15 cases

These are genuine Yang completions where the result is wrong.
Diagnostically valuable: the B-Rep is internally consistent
(twin-validates) but geometrically wrong, so the mechanism must
be in either (a) survival classification accepting incorrect
sub-triangle sets, or (b) retessellation reintroducing
penetrations the boolean-LOD mesh didn't have. Either is
tractable to localize via instrumentation per
`feedback_anchor_before_fix.md`.

#### YB-11 (EULER-VIOLATION) — 6 cases

5 of 6 are χ off by exactly +2, consistent with one extra
disconnected component (likely an auto-union failure surfacing as
"merge incomplete"). F0044 is an outlier (χ=22).

### Tier C: low count, low tractability for Yang work

- **YB-06 (tri-pair-limit, 6 cases)**: capacity-bound, not a Yang correctness bug. Won't be moved by YB-01-class fixes.
- **YB-07 (bijective-oracle, 1 case)**: singleton.
- **YB-08 (revolve-SI, 1 case)**: meta-annotation issue.
- **YB-09 (TIMEOUT-30s, 2 cases)**: budget-bound.

### Ranking (count × tractability heuristic)

| Rank | Finding | Class | Cases | Tractability | Notes |
|---|---|---|---:|---|---|
| 1 | YB-01 | YANG-ERR-twin-validation | 92 | Moderate | Single error message, multiple upstream causes per YB-18 |
| 2 | YB-04 | WATERTIGHT-unpaired | 26 | Moderate-Low | Bimodal; needs sub-bucket split first |
| 3 | YB-05 | SELF-INTERSECT | 15 | Moderate | Tractable via instrumentation |
| 4 | YB-11 | EULER-VIOLATION | 6 | Moderate | 5/6 are auto-union side-effects |
| 5 | YB-06 | tri-pair-limit | 6 | Low | Capacity-bound |

**Per `feedback_no_last_bug.md`**, none of these rankings should be
read as "fixing rank-1 unlocks N cases." They estimate where
investigation has the highest expected information yield given a
fixed engineering effort, not how many cases will move to PASS.

---

## §7. What this slice did NOT cover

### Out of scope by brief

1. **Code-path root-cause tracing for individual cases.** I identified the
   error site (`validate_yang_result_topology` for YB-01) but did not
   trace the upstream cause of unpaired half-edges to specific
   subdivide_mesh_pair / label_cells / survival decisions. Per
   `feedback_anchor_before_fix.md`, that tracing requires per-case
   PR_DEBUG=1 instrumentation — out of scope for this characterization
   slice. Auditor C (Cherchi-port slice) is the natural extension.
2. **Fix proposals.** The brief explicitly forbade these.
3. **Audit of the 33 skipped timeout cases.** Those are excluded from
   yang_fast and from this audit. They may or may not share root causes
   with the yang_fast failures; not measured here.

### Limitations of the methodology

4. **Single-run baseline.** I ran yang_fast once and used its 8 PASSes
   as canonical. R0080 is known nondeterministic; R0018 is reportedly
   nondeterministic; either could flip on a re-run. A future audit
   should run yang_fast 3-5 times and report bucket-stable cases vs
   flapping cases separately.
5. **Detail-string nondeterminism.** Per YB-13, watertight unpaired
   counts and face-pair lists drift between runs. Bucket assignment is
   stable in the cases I've seen, but specific brief-detail values
   are not. The per-case table reflects this run's snapshot.
6. **First-failure point semantics.** Per the brief I classified by
   FIRST oracle-detected failure. Some cases have multiple stacked
   failures (e.g., R0033 has WATERTIGHT-unpaired + 10 inter-face
   penetrations + reversed normals + χ wrong); only the first
   (watertight) shapes the bucket. A different ordering (e.g.,
   YANG-ERR > SELF-INTERSECT > WATERTIGHT > EULER) would shift counts
   between Tier A/B classes.
7. **Bucket independence assumption.** §6 leverage analysis treats
   bucket counts as independent. They are not — fixing YB-01 likely
   exposes more SELF-INTERSECT failures (YB-10), so the bucket sums
   should not be summed across tiers.

### Areas adjacent but not measured

8. **Performance distribution within yang_fast.** Two cases (F0064,
   R0071) timed out at 30s; the rest completed. I did not measure
   per-case wall-clock time, so I can't characterize the
   tail-latency distribution near the budget cliff.
9. **Coplanar-preprocess effectiveness.** Yang §4.5.5 coplanar
   preprocessing is in the pipeline (`coplanar_preprocess.rs`). The
   F-series cases F0001-F0007 are dense with coplanar geometry; some
   pass, some fail at YB-01. I did not isolate whether the YB-01
   failures in F0002-F0006 happen *despite* coplanar preprocessing
   running successfully or *because* it failed to find the right
   pairs — that requires per-case [yang-diag] log inspection
   correlated with the meta files, which is auditor C's slice.
10. **Bijective tessellation gating.** Per `tessellation_pr3_corpus_dump.md`
    only 14/81 linear-bounded cases have non-bijective face pairs,
    yet 92 cases hit YANG-ERR-twin-validation. Bijectivity is
    necessary but not sufficient. The remaining (92 - ~14) ≈ 78
    cases have other upstream causes; those are not measured here.

---

## References

- `app/tests/cases/assay/results.json` — post-run baseline
- `app/tests/cases/assay/F*.meta.json`, `R*.meta.json` — case metadata
- `crates/test-harness/tests/assay_randomized.rs:596-669` — yang_fast definition
- `crates/test-harness/src/assay/randomized_runner.rs:140-241` — runner + per-case results.json updater
- `crates/kernel/src/boolean/yang_integration.rs:979-1080` — `validate_yang_result_topology`
- `crates/kernel/src/boolean/topology_extract.rs:351-925` — `flood_fill_patches` (twin pairing at 825-892)
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6 — Yang hybrid pipeline target
- `governance/ENGINEERING_CONSTITUTION.md` §P5, P8, P9, P10 — audit constraints
- `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §1 — auditor-implementer separation
- `docs/audits/cherchi_port_audit.md` — prior 4-auditor port audit (42 findings)
- `docs/references/yang2025_hybrid_boolean.txt` §4.4.2-4.4.3 — flood-fill patch segmentation, watertightness inheritance
- `/tmp/cherchi2022.txt` — full Cherchi 2022 paper (Algorithm 1 §5)
- `specs/tessellation_bounded_residuals.md` §§1-11 — bijective oracle work
- `specs/tessellation_pr3_corpus_dump.md` — per-case bijectivity catalog (cross-referenced for YB-03)
- Memory files: `feedback_no_regression_chasing.md`, `feedback_no_last_bug.md`, `feedback_yang_only.md`, `feedback_anchor_before_fix.md`, `feedback_validate_against_corpus.md`

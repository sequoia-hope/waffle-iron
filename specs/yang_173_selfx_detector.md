# #173 — N6 §4.5.4 illegal-self-intersection DETECTOR (increment 1: detect + loud STOP)

**Task:** #173 (endgame Phase 4, roadmap §0.0). **Deviation:** N6 (OPEN —
§4.5.4 detection/removal absent). **Design authority:** the 2026-07-17
junction research findings Q5 (`docs/yang_junction_research_findings.md:131`)
— design settled there; this spec records the wiring and the measurement gate.

## 1. Scope

Increment 1 is DETECTOR-FIRST: detect illegal self-intersections in the
boolean output shell and STOP LOUDLY. **No removal** — §4.5.4's "perform
local refinement" (removal) is increment 2 and routes into the #169
mesh-update loop when that machinery exists. A loud STOP is strictly better
than today's silent-wrong (P9/P10); C0116 is the designed red fixture.

Paper: `refs/text/yang2025_hybrid_boolean.txt:752-758` — the illegal
intersections are artifacts of surface discretization and mesh updating (the
input B-Rep is certified clean, so any output self-intersection is
pipeline-minted).

## 2. Detector semantics

**Exact triangle–triangle contact classification on non-index-adjacent pairs
of the OUTPUT mesh** (the Stage-4 f64 indexed mesh — `Mesh { verts, tris }`),
reusing the same `classify_pair` predicate stack the Stage-2 arrangement runs
(zero new numerics, conservative by construction).

Illegal = any pair of triangles that share **no vertex index** yet classify as
non-`Disjoint` (transversal contact, coplanar overlap, or touch), plus any
`Deferred` classification (loud, never silently dropped). Rationale for the
index-adjacency criterion: the detector runs AFTER the §4.4.3
`check_watertight_2manifold` gate, which enforces edge-2-cover **by index** —
post-gate, every legitimate adjacency in the mesh is index-mediated, so any
surviving contact between index-disjoint triangles is a genuine defect
(two-sheet penetration = C0116's class; coordinate-twin-mediated contact =
the #146 near-dup mint class; T-junction contact cannot survive the gate).

This is the same classification as `cherchi_rs::inputcheck::census` tier 3.
That module is stamped "DIAGNOSTIC ORACLE, NOT A GATE" — but its recorded
false-positive population (the N22 fold-sliver class,
`specs/yang_kept_mesh_manifold_gate.md` §2b) was measured on **chained INPUT
operands**, where collinear edge chains legitimately subdivide differently
across separately-authored meshes. The OUTPUT shell has no such legitimacy:
it is one arrangement-derived indexed mesh whose conformality contract is
index-level. §5 measures this claim corpus-wide before any STOP ships.

## 3. Implementation shape

1. **Primitive (cherchi-rs, `src/inputcheck.rs`):**
   `detect_improper_contacts(verts, tris) -> ImproperContacts` — the census
   tier-3 sweep extracted as a standalone pub fn (census delegates to it;
   behavior-identical). Adds sort-by-min-x sweep pruning over the per-triangle
   AABBs (the O(n²) AABB double loop is the recorded reason census is not a
   gate; the sweep makes the common disjoint case cheap). Output vectors
   sorted, deterministic.
2. **Probe (yang-rs, `reconstruct_topology_stage4` tail, after the §4.4.3
   gate):** env `YANG_SELFX_PROBE` → run the primitive, print offending pairs
   + coordinates + attribution. Byte-identical when unset.
3. **Gate (same site, always-on, only after §5 passes):** new
   `YangError::IllegalSelfIntersection { pairs }` (bounded pair list) — a
   typed P9/P10 loud STOP, surfaced through kernel-v2 as a typed
   `KernelError` like every other Stage-4 STOP.

## 4. What the probe must answer before the gate ships (P10 gate)

1. **Does C0116 fire at Stage 4?** The assay found 10 penetrations in the
   RENDER mesh (f32, per-face re-tessellation downstream in kernel-v2). If
   the Stage-4 f64 mesh is clean, the defect is minted downstream (Stage 5/6
   or render tessellation) and this wiring point is WRONG — abort, report,
   re-spec the placement (kernel-v2 output gate). Do not ship a gate that
   can't see the fixture.
2. **False positives:** zero CORRECT-case fires across the full 311-case
   corpus, or every fire individually shown to be a real (previously silent)
   defect. A false-positive population = the N22 story again = abort per P10.
3. **Cost:** per-case CPU delta acceptable under the assay budget (the
   detector runs on every boolean output in production). Measure on the
   biggest outputs (gear class). If the exact tier is too slow always-on,
   the fallback posture is: exact classification only on AABB-overlapping
   non-adjacent candidates (already the design); if STILL too slow, gate
   ships assay/debug-first and the roadmap records the production gap — NOT
   a float approximation (P9).

## 5. Exit criteria (increment 1 DONE)

- [ ] Primitive + unit tests in cherchi-rs (clean tet; piercing pair;
      coplanar overlap; index-shared adjacency skipped; twin-mediated
      contact flagged; sweep ≡ double-loop property).
- [ ] Probe measurement recorded here (§6): C0116 fires; per-case fire list
      over full corpus; timing.
- [ ] Always-on typed STOP wired; C0116 pin flips SUPPORTED_WRONG →
      Category::Error in `smoke_corpus_boundary_categories`.
- [ ] Full assay: C0116 WRONG→ERROR; **zero** CORRECT regressions; whole-run
      categories otherwise identical (any other WRONG→ERROR flip is a bonus
      exposure, recorded).
- [ ] `docs/yang_deviations.md` N6 updated (detection SHIPPED, removal =
      increment 2 → #169); triage ledger C0116 row; roadmap §0.0 Phase-4
      note.

## 6. Measurement record (2026-07-17, full 311-case probe sweep)

The §4 P10 gate ran and **REFUTED the single-layer design** on both counts:

1. **C0116 does NOT fire at Stage 4.** Its final mesh (68 tris) is exactly
   clean; the penetration (~5e-3, cyl×cyl graze) is **sub-sagitta** at the
   boolean mesh's resolution (12-segment cylinder ⇒ sagitta ≈ 8.6e-3) and
   only becomes observable at render resolution (sagitta = 1e-3·r ≈ 5e-4,
   assay threshold 2e-4·scale). Root: the trims come from the chord-accurate
   cyl×cyl path (M5 gap — no exact degree-4 curve), so the emitted trimmed
   surfaces genuinely interpenetrate at a depth the coarse mesh never
   samples. KV11_SI probe: the graze cylinder's lateral face spans its full
   length and the body wall keeps full-height generators — both trims wrong
   at the graze, faces 2×5, 10 penetrations at f32/render view.
2. **The exact detector fires on 53 cases — 33 SUPPORTED_CORRECT** (C0052–54,
   F0022/25/32/33/34/41/44/55/61/62/75/77–81/84/86–89, R0012/14/21/31/46/
   55/58/59/76 + 20 already-ERROR/TIMEOUT). Inspection (C0053): input-A ×
   input-B cylinder-lateral chords crossing near the intersection seam after
   Stage-4 relocation — the §4.5.4-expected "new intersections that may
   arise during mesh updating", whose paper remedy is REMOVAL by local
   refinement, not a STOP. Also cost-disqualified: up to 9.5s on 32k-tri
   meshes.

## 7. Revised design (shipped)

**Two layers, one STOP:**

- **Stage-4 exact mesh detector → banked as `YANG_SELFX_PROBE`**
  (yang-rs `stage5_topology.rs`, before `emit_topology`; byte-identical
  off). Its fire-list IS the §4.5.4-removal worklist for #169 increment 2
  (53 cases carry relocation/seam chord-crossings today). NOT wired as a
  STOP (33 CORRECT regressions = P10).
- **Render-resolution gate → the PRODUCTION loud STOP**
  (kernel-v2 `validate::selfx::validate_boolean_output_self_intersection`,
  called at the boolean assembly boundary beside the F1 planarity gate).
  Semantics-identical port of the corpus-calibrated assay oracle
  `check_no_self_intersection` (PR-TH1 normalized penetration depth,
  PR-KV11 quantized-shared-vertex adjacency skip, grazing band
  `max(max_abs·TAU_WELD_MAX, TAU_COINCIDENT)`) on the f64 render mesh —
  the mesh whose f32 cast every SUPPORTED_CORRECT case already passes in
  the assay, so the false-positive rate is measured (zero), not assumed.
  The band is a P10 safety net in the sanctioned direction only: it can
  only convert silent-wrong emissions into loud STOPs.
  New typed error `KernelV2Error::SelfIntersectingBooleanOutput`.

**Results:** C0116 WRONG→ERROR (`face_a=8, face_b=11, penetrations=40`);
**C0105 WRONG→ERROR as a bonus** (boolean_subtract rejected,
`face_a=11, face_b=13, penetrations=76`) — #177's loud-STOP increment is
delivered by the same gate (its watertightness-evasion residual remains
open). Cost: the gate tessellates the output once per boolean op; heavy
chained cases pay real time (C0053: 47s → 87s, still under budget) —
full-corpus timing verdict recorded below.

## 8. Full-assay verdict (2026-07-17, gate ON, release, 120s/case)

**248 C / 2 W / 54 E / 3 T / 1 EXPECTED_ERROR** (+3 UNSUPPORTED = 311).
Per-case diff vs the pre-gate baseline is EXACTLY:
- C0105 SUPPORTED_WRONG → ERROR (designed flip, gate)
- C0116 SUPPORTED_WRONG → ERROR (designed flip, gate)
- F0085 ERROR → TIMEOUT — borderline-budget artifact, not a verdict change:
  it ran at 116.2s of its 120s CPU budget pre-gate; the gate's per-op cost
  on its 20-extrude auto-union chain pushes it over. Both states are loud.

Remaining 2 WRONG = C0111/C0113 (#178, the sub-floor sliver dissolve — no
penetrations, out of this gate's scope by design).

**Perf note (the first cut was a real regression, caught by this run's
predecessor):** the initial scan re-quantized vertices per triangle PAIR and
had no per-triangle AABB pruning — 43.4s on R0054's 46,828-tri output,
flipping it SUPPORTED_CORRECT → TIMEOUT. Fix: per-triangle records
(positions/quantized verts/AABB) computed once + AABB overlap pruning before
the quantized-adjacency check and Möller test (pure pruning — a
beyond-threshold penetration implies overlapping AABBs). Scan: 43,445ms →
174ms (250×); R0054 back to 66s, CORRECT.

**Collateral exposure:** kernel-v2 integration test
`unequal_perpendicular_now_supported` (kv9_cyl_cyl_special) was a FALSE
GREEN — the M5 surface-pair-curve union completes watertight but its shell
carries 79 beyond-band penetrations (the fixture never ran under the
self-intersection oracle; the corpus' equal-radius Steinmetz cases pass
because their intersection degenerates to exact ellipses). Renamed
`unequal_perpendicular_walls_on_selfx_gate`, pins the typed STOP,
un-quarantine at [M5]/#172.

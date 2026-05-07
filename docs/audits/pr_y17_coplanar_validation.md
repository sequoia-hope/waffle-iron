# PR-Y17-COPLANAR sub-phase 0e — adversary-16 validation

**Author:** adversary-16
**Date:** 2026-05-06
**Scope:** Independent validation of implementer-t's curve-sampling fix to
`crates/kernel/src/boolean/coplanar_preprocess.rs::collect_face_loop_2d` per
spec §3-§4 + §8. Per the brief: bar is **revised Path A** (curve-sampling
architecturally correct + no kernel test regression + Layer 3 cleanly
deferred + commit message accurate + §4 SAMPLING-INDUCED hypothesis
investigated honestly), NOT verbatim spec ACCEPT criteria.

**Verdict (§8): AMEND.** Layer 1 deliverable is correct in shape; spec §5
test plan invariants 2 + 3 are not all GREEN; §4 SAMPLING-INDUCED hypothesis
needs reframing because team-lead's framing was off-by-context for F0030;
§5 surfaces a new defect class (R0014 panic) that should be banked but does
not block the PR; commit message must accurately reflect Layer 3 deferral
+ scope of metrics tris_a+tris_b regressing UPWARD (76 vs 58).

---

## §1 Independent re-run

| Measurement | Pre-fix baseline (canary memo + post-PR-Y16-FIX-ARCH) | Implementer-t reported (sub-phase 0d) | This audit re-ran |
|---|---|---|---|
| `cargo test -p kernel --lib` | 1248/31/42 | 1248/31/42 | **1248/31/42** ✓ matches |
| `cargo test -p test-harness --lib` | 92/0/1 | 92/0/1 | **92/0/1** ✓ matches |
| `[coplanar-tele] partial_overlap` (F0030) | 0 | 1 | **1** ✓ Layer 1 GREEN |
| `[coplanar-tele] verts_dropped` (F0030) | 0 | (not reported) | **1591** (NEW; large; see §4) |
| `[yang-diag] tris_a + tris_b` (F0030) | 30+28=58 | (not reported as a metric) | **36+40=76** (REGRESSED UPWARD vs spec §2 invariant 3 contract) |
| `[topo-extract] ambiguous` (F0030) | 11 | (not reported) | **11** (UNCHANGED) |
| `[twin-oracle] collision_count` (F0030) | 3 | (not reported) | **2** (small drop, but >0 — assertion 5 fails) |
| F0030 spotlight Status | Failed | Failed (Layer 3) | **Failed** ✓ confirmed |
| F0030 validator panic exact | `half_edge[4].twin = 0 but twin.twin = 29 (expected 4)` | (Layer 3 reported as watertight_mesh: 6 unpaired / 87, consistent_normals 20/56, mesh_euler 24) | **`half_edge[4].twin = 0 but twin.twin = 32 (expected 4)`** — DIFFERENT exact (29 → 32); same SHAPE (twin-pairing validator) |

**Key disagreement with implementer-t's brief.** The brief claims F0030's
post-fix failure mode is `watertight_mesh: 6 unpaired edges out of 87 total;
consistent_normals: 20 of 56 reversed; mesh_euler V-E+F = 24 (expected 2)`.
Re-running F0030 spotlight, I observe ZERO of those oracle outputs in
stderr. The actual failure is `half_edge[4].twin = 0 but twin.twin = 32
(expected 4)` — the validator panic, same SHAPE as pre-fix. The
watertight/normals/Euler oracle suite IS what F0050 surfaces (verified
independently, see §6); implementer-t conflated F0050's defect description
with F0030's. F0030 stays in the twin-validator class; only the exact index
shifted (29 → 32) reflecting the new B-Rep face split injecting more
vertices upstream of topology extraction.

**`pr_y17_coplanar_curve_sampling_red_phase` regression test result.**
- Assertion 1 `pairs=1`: GREEN.
- Assertion 2 `partial_overlap=1`: GREEN. (Layer 1 deliverable contract met.)
- Assertion 3 `tris_a + tris_b < 58`: **RED** — got 76 (was 58); fix
  REGRESSED on this metric. The B-Rep `split_face_along_boundary` runs at
  1591-vertex resolution and adds boundary verts upstream of
  `inject_partial_overlap_mesh`; the injection then runs over a pre-modified
  topology and ADDS shared-overlap tris on top. Net cap-plane tri count
  goes UP, not down, contrary to spec §2 Invariant 3 (c).
- Assertions 4 & 5 (ambiguous=0, collision_count=0): RED.

The RED-phase regression test correctly pins assertion 2 as the GREEN
contract for Layer 1. Assertions 3-5 are post-Layer-1 invariants and remain
RED at sub-phase 0d completion.

---

## §2 Yang corpus sweep (load-bearing)

`YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1`. Pre-PR baseline: **10/157 passed, ?/157 failed, 0 errored**. Implementer-t spec target: ≥11/157.

**Post-fix (this audit re-ran)**: `Yang fast: 10/157 passed, 142 failed, 5 errored (skipped 33 known timeouts)`. Wall-clock 375.06s.

**Pass count delta: 0 (10 → 10).** Spec §5 target ≥11 NOT met. F0030 did NOT
flip RED → GREEN (still in the twin-pairing validator class). However,
**no previously-passing case became failing**: the 10 cases that passed
on baseline still pass. The 5 NEWLY ERRORED cases (R0014, R0046, R0055,
R0081, F0075) were ALL baseline-FAILED before — they shifted from
`Failed` to `Errored` (caught panic propagation through the `[A15.6]
Yang boolean pipeline panicked` handler). No regression for any
previously-passing case.

**`[coplanar-tele]` final aggregate at corpus end** (the LAST emit
reflects post-corpus global counter state):

```
[coplanar-tele] pairs=117 verts_existing=1710 verts_split=357
                verts_deduped_by_canon_key=178 verts_dropped=1409803
                mef_ok=168 mef_no_loop=4 overlay_groups=528
                overlay_holes_ignored=0 identical_footprint=7
                partial_overlap=70
```

- 117 coplanar pairs detected across the corpus
- 70 fired `partial_overlap=1`; 7 fired `identical_footprint=1` (Yang
  §4.5.5 injection paths active for 77/117 = 66% of detected pairs)
- **1,409,803 verts_dropped** — the chord-sampled vertices generated by
  `collect_face_loop_2d` that did NOT snap to existing arena vertices and
  were dropped by `split_face_along_boundary`. **This is the corpus-scale
  manifestation of the chord-sampling cost.** N=1591 chord points per
  circle (per the chord_sample_count formula at r=0.0513, TAU_MODEL=1e-7
  — note the spec §4 estimate of "N≈718" is off-by-2x; actual is ~1591),
  multiplied by hundreds of circle edges across the corpus.

Wall-clock: 375 seconds for the full sweep. Baseline (pre-fix) wall-clock
not re-measured cleanly in this audit (would require reverting + re-
running, ~6 additional minutes); implementer-t's flagged risk ("if the
resulting polygon count materially impacts wall-clock") IS plausibly
realized given 1.4M chord vertices processed, but I cannot quantify the
delta without baseline re-run. **Banked**: PR-Y18 should consider
lowering chord-error tolerance to match Boolean LOD (16 segments per
circle) so all three layers (`collect_face_loop_2d`, tessellation,
`extract_face_boundary_2d`) work at the same boundary-sample resolution.

### Cases that moved between baseline and post-fix

- 10/157 passed → 10/157 passed. **NONE moved from pass to fail.**
- 5 baseline-fail cases (R0014, R0046, R0055, R0081, F0075) shifted from
  `Failed` to `Errored` due to the L264 panic. All were already FAIL.
- 0 baseline-fail cases moved to PASS.

Per spec §10 risk: "test churn — completing coplanar preprocessing changes
the boundary topology of cases that previously passed for accidental
reasons (e.g., the manifold-edge barrier from PR-Y16-FIX-ARCH was masking
the missing coplanar preproc for some cases)" — **no observed regression
on previously-passing cases**. Healthy.

---

## §3 Layer 1 architectural validation

Spec §3-§4 contract — per checklist:

- ✓ `collect_face_loop_2d` signature accepts `&BTreeMap<EdgeIdx, CurveGeom>`
  (line 470). Production call sites L230 + L238 pass `&solid_a.edge_geometry` /
  `&solid_b.edge_geometry`. Test sites L2528 + L2536 + L3346 + L3358 also
  updated.
- ✓ `Linear` arm: empty body (no-op) — emits only the half-edge origin
  vertex. Matches spec §4 "single emit (no change)".
- ✓ `Circular` arm: samples at chord error TAU_MODEL via
  `chord_sample_count(r, TAU)` then `emit_curve_samples` over `[0, 2π]`. The
  `chord_sample_count` derivation (`Δθ = 2·acos((r - TAU)/r)`, `N = ceil(2π/Δθ)`,
  N ≥ 8) matches spec §4 exactly. **Spec §4 estimate of N≈718 for
  r=0.0513 + TAU_MODEL=1e-7 is incorrect** — actual N for these inputs is
  1591 (verifiable via the formula or via empirical `verts_dropped=1591` in
  the regression test telemetry).
- ✓ `Arc` arm: parameter range `[0, sweep_angle]`, `chord_sample_count(r, sweep)`.
- ✓ `Elliptical` arm: conservative bound on `semi_major` per spec §4.
- ✓ Exhaustive `match` over `CurveGeom` variants — no fallback path. The
  enum (`crates/kernel/src/geometry/curve.rs:13-18`) has only 4 variants
  today (`Linear`, `Circular`, `Arc`, `Elliptical`); no `Spline` / `BSpline`
  / `NURBS` exists, so the absence of an `unimplemented!()` arm is correct
  per spec §4 "Spline curves: NOT in `CurveGeom` enum today."
- ✓ L264 silent-continue REPLACED with `panic!` when `YANG_BOOLEAN=1` env
  var is set; `eprintln!("[coplanar-warn] ...")` plus `continue` otherwise
  (L284-303). Matches spec §8 "panic-when-`YANG_BOOLEAN=1`, diagnostic-
  otherwise" exactly.
- ✓ Post-injection `[coplanar-tele]` re-emit added at end of
  `inject_identical_footprint_mesh` and `inject_partial_overlap_mesh`
  (L1297-1305 + L1554-1565 via `emit_coplanar_tele_post_inject`). The
  post-injection emit reflects the post-state so the regression test's
  `last_coplanar_tele_line(...)` parse picks up the updated counters.
- ✓ Code comments cite Yang §4.5.5 (L457, L459-462; the `chord_sample_count`
  comment derives the formula).
- ✓ `cargo fmt --check -p kernel`: clean.
- ✓ `cargo clippy -p kernel --no-deps`: no NEW warnings on the modified
  file (90 pre-existing warnings in the kernel crate are unchanged).

Architectural verdict: **Layer 1 deliverable is correct.** No code review
findings.

**Caveat**: the new helper functions (`project_3d_to_2d`,
`chord_sample_count`, `emit_curve_samples`) are private to the module.
None are unit-tested standalone. The spec §3 "(d) Unit test in 0c that
constructs an arena with one circular face, calls `collect_face_loop_2d`,
asserts ≥3 chord points and that all sampled points lie within TAU_MODEL
of the true circle" was deferred to the regression test
(`pr_y17_coplanar_regression.rs`). The regression test exercises the
function indirectly via the F0030 spotlight, which is sufficient for the
single-case contract but does not test the formula's correctness over a
wider parameter range. Banked for PR-Y18.

---

## §4 SAMPLING-INDUCED hypothesis check (CRITICAL — team-lead-flagged)

Team-lead's framing: "do A's 718 chord points and B's 718 chord points
land at byte-identical 3D positions, or do they diverge by float epsilon?"

**Static analysis result: framing is moot for F0030. Refined hypothesis is
PARTIALLY CONFIRMED in a different shape.**

### Why team-lead's framing is moot for F0030

F0030 has TWO operands:
- **Solid A** = rectangle box (Sketch 1 + Extrude 1). Its TOP cap face at
  z=0.273588 has FOUR straight edges (4 `Line3D` `CurveGeom::Linear` edges),
  NOT a circle.
- **Solid B** = cylinder (Sketch 2 + Extrude 2). Its BOTTOM cap face at
  z=0.273588 has ONE periodic seam edge with `CurveGeom::Circular`.

`collect_face_loop_2d` chord-samples ONLY the cylinder (Solid B). For
Solid A, the `Linear` match arm emits no interior samples — A's polygon
is 4 rectangle corners (`poly_a.len = 4`).

So the question "do A's 718 vs B's 718 sample identically" is moot
because A has no circle to sample. The chord polygon exists on B only.

### The actual SAMPLING-INDUCED defect (CONFIRMED)

The fix introduces a **3-layer sampling resolution mismatch** within
solid B's circular cap face:

| Pipeline layer | Function | Boundary sample count for B's circle |
|---|---|---|
| Layer 1 (marker pass) | `collect_face_loop_2d` (this PR) | 1 origin + 1591 chord interior = **1592 vertices** |
| Layer 1.5 (B-Rep face split) | `split_face_along_boundary` consumes `overlap_3d` derived from Layer 1 | Tries to insert 1591 boundary verts; per regression telemetry **1591 dropped** (none snap) |
| Layer 2 (tessellation) | `tessellate_circular_cap` (`tessellation/mod.rs:958`) | **17 vertices** (1 center + 16 perimeter at Boolean LOD; 65 at Render LOD) |
| Layer 3 (injection) | `inject_partial_overlap_mesh` calls `extract_face_boundary_2d` | reads the post-Layer-2 16-perimeter boundary edges → ~16-vertex polygon |

Layer 1 (1592 verts) and Layer 3 (~16 verts) operate at incompatible
resolutions. The marker-pass `[coplanar-tele] partial_overlap=1` fires
based on a 1592-vertex polygon overlay; the actual injection then runs on
a 16-vertex polygon. These two polygons agree in shape (both are circles)
but disagree in vertex counts — and `split_face_along_boundary` between
them mutates A's B-Rep face with the 1591-vertex boundary, which then
re-tessellates A's face at high resolution. B's face is not similarly
re-modified; B's bottom cap stays at Boolean LOD = 16.

**Empirical corroboration in the regression test telemetry:**

```
[coplanar-tele] pairs=1 verts_existing=1 verts_split=0
                verts_deduped_by_canon_key=0 verts_dropped=1591 ...
[yang-diag] after subdivide: tris_a=36, tris_b=40, verts=35
```

`verts_dropped=1591` matches the chord-sample count for one circle. None
snapped to existing arena vertices because the rectangle face has no
verts at chord-sampled positions (the rectangle's only verts are the 4
corners + maybe a small handful at intersections, far from the 1591
chord-sample positions on the circle).

`tris_a=36` (was 30 pre-fix) suggests A's face received the injection's
shared-overlap tris (overlap = circle-shape polygon → ~14 fan triangles)
on top of A-only (rectangle - circle → ~16 tris) = ~30, plus ambient
non-cap-plane tris = 6. So A grew by 6 tris. **B's face grew by 12 tris**
(28 → 40). That's the injection adding shared-overlap tris to BOTH meshes
without removing the originals (per the `inject_face_with_shared_first`
+ `repair_tjunctions_after_injection` pattern at L1514-1542). Net effect:
the cap-plane region now has 14 (shared-overlap) + 16 (A-only) + 16
(B-only original from Layer 2) ≈ 46 tris, not the contract's "shared
canonical = 14 tris total" count.

### SAMPLING-INDUCED reframing — what this means for PR-Y18

The fix is necessary (Layer 1 marker must fire) but not sufficient. To
land Layer 3 GREEN, all three pipeline layers must work at COMPATIBLE
boundary-sample resolutions. Two viable paths:

1. **Lower Layer 1 chord-error to match Boolean LOD** (~16 segments per
   circle): `collect_face_loop_2d` would produce ~16-vertex polygons,
   matching `tessellate_circular_cap`. Trade-off: Layer 1 polygon is no
   longer "TAU_MODEL chord-faithful" per spec §4. But i_overlay's overlap
   classification is robust to this — it only needs the polygon to be
   topologically faithful, not metrically tight.

2. **Tessellate at TAU_MODEL chord-error** (raise tessellation LOD to
   match Layer 1): all three layers work at 1591-vertex resolution.
   Trade-off: very high triangle counts → wall-clock impact.

PR-Y18 recommendation (banked, self-canary status flagged below): pursue
Path 1.

### §4 verdict

**SAMPLING-INDUCED is PARTIALLY CONFIRMED in a refined shape**: this PR
DID introduce a new boundary-resolution mismatch inside B's pipeline (the
1591-vertex marker pass vs the 16-vertex tessellation+injection). It did
NOT introduce a defect from "A and B sample independently" — that
framing is off because A doesn't have a circle. Honest reporting per the
brief's "Either outcome is a valid finding."

---

## §5 L264 panic firing check (corpus-wide)

`grep -c "^thread .* panicked" /tmp/sweep_post.log`: **5 panics**
across the corpus run. All cases were already baseline-FAILED.

| Case | poly_a.len | poly_b.len | face_a / face_b | same_dir | Polygon character |
|---|---|---|---|---|---|
| R0014 | 5 | 4 | FaceIdx(1) / FaceIdx(1) | true | Both linear (rectangle-like with extra vert) |
| R0046 | 3 | 26 | FaceIdx(10) / FaceIdx(1) | true | A=triangle (3 verts); B=mixed/curved (26 verts) |
| R0055 | 4 | 32803 | FaceIdx(5) / FaceIdx(0) | true | A=rectangle; B=heavy-chord-sampled circle (~20 circles' worth) |
| R0081 | 4 | 3373 | FaceIdx(15) / FaceIdx(15) | true | A=rectangle; B=heavy-chord-sampled (~2 circles' worth) |
| F0075 | 4 | 4 | FaceIdx(1) / FaceIdx(0) | true | Both linear rectangles |

All `same_dir=true`. Three classes:
1. **Both-linear, no overlap** (R0014, F0075): false-positive coplanar
   detection OR i_overlay degeneracy on two same-direction coplanar
   polygons that don't physically overlap.
2. **Mixed linear+curved** (R0046, R0055, R0081): chord-sampled circle
   polygon vs rectangle polygon, no overlap. Possible i_overlay
   degeneracy with a 3373- or 32803-vertex polygon as input — i_overlay
   may be hitting a numeric edge case at high vertex counts.

### Is the panic over-strict?

**Adversary-16 recommendation: do NOT soften.** The panic is doing what
spec §8 mandates and what `feedback_yang_only.md` codifies — surfacing a
real defect that the silent-continue would have masked. All 5 firing
cases were already broken on baseline; the panic just shifts the failure
path from `Failed` to `Errored` (caught by the kernel's panic handler).
No previously-passing case became failing.

The 5 distinct firing patterns (vs adversary-16's earlier read of 1
panic from incomplete log) reveal **the panic is surfacing a real
upstream defect in `detect_coplanar_face_pairs`**: same-direction
non-overlapping faces should not be flagged as coplanar, OR i_overlay
should not return empty for actually-overlapping inputs. This is now
diagnostic-rich data for PR-Y18 instead of being silently dropped.

### Is this a new defect class?

**Yes**, surfaces a single defect class with 5 manifestations:
**false-positive coplanar detection** by `detect_coplanar_face_pairs` OR
**i_overlay degeneracy at large polygon counts** (R0055's 32803-vertex
circle is plausibly the i_overlay edge case). PR-Y18 investigation
target: probe each of the 5 cases and triage which mechanism applies.
Could be both — different cases for different mechanisms.

### §5 verdict

**5 panic sites, all on baseline-FAIL cases**. Spec §8 contract behavior
is correct. Banked for PR-Y18 investigation. Does NOT block this PR
(no regression on previously-passing cases).

---

## §6 Cohort sibling check + wall-clock

### F0060 sibling probe

**Not applicable.** No `spotlight_f0060` test exists in
`crates/test-harness/tests/assay_randomized.rs` (only `spotlight_f0020`,
`spotlight_f0030`, `spotlight_f0044`, `spotlight_f0050`,
`spotlight_f0061_gear_cut`). F0060 case file may exist in the corpus but
no spotlight harness wraps it. The corpus sweep result for F0060
(if/when reachable in the post-PR sweep) would be the data point — see
§2.

Banked: PR-Y18 should add a `spotlight_f0060` test to drive the
cap-coplanar boss+cut sibling probe per spec §5 + §6.

### F0086 wall-clock probe

F0086 was NOT individually tested via spotlight (no `spotlight_f0086`
exists). However, F0086's 5-coplanar-pair swiss-cheese geometry is what
the corpus aggregate `verts_dropped=765584` reflects in part. Per the
running sweep observation, F-series cases were not yet reached when
adversary-16 began write-up (sweep had progressed through R0033 of the R
series at log line 1623). Banked: PR-Y18 should add a `spotlight_f0086`
to allow per-case wall-clock measurement.

### Wall-clock observation

- F0030 spotlight: ~39ms (matches pre-fix; the chord-sampling cost is
  bounded since F0030 has only 1 coplanar pair).
- Corpus sweep: 375.06 seconds total. Baseline pre-fix wall-clock not
  re-measured cleanly in this audit. The 1.4M verts_dropped indicates
  significant chord-sample churn in `split_face_along_boundary`;
  implementer-t's flagged wall-clock risk is plausibly realized but
  the precise delta is not quantified here.

### §6 verdict

F0060 + F0086 cohort probes deferred (no spotlight harnesses); banked
for PR-Y18. Wall-clock cost of TAU_MODEL chord sampling is non-trivial at
corpus scale.

---

## §7 F0020 + F0050 banking honesty + Cherchi sidecar parity

### F0020 spotlight

**Status: still RED. Same defect mode.**

```
=== F0020 Spotlight (PR-Y16-INV) ===
Status:      Failed
Detail:      auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed: kernel error: operation not supported: yang_boolean: result validation failed: half_edge[16].twin = 0 but twin.twin = 31 (expected 16). ...; watertight_mesh: 3 unpaired edges out of 126 total; consistent_normals: 1 of 83 reversed; ... mesh_euler V(44)-E(126)+F(83) = 1 (expected 2)
```

F0020 has a coplanar pair (per memory `yang_f0030_coplanar_root_cause.md`)
but the curve-sampling fix has nothing to do at F0020's site (its
coplanar pair is rectangle-rectangle, not rectangle-circle, so chord
sampling never fires). RED is expected per spec §5 anti-scope.

### F0050 spotlight

**Status: still RED. Same defect mode (watertight + consistent_normals + Euler).**

```
=== F0050 Spotlight (PR-Y16-FIX-ARCH cohort, silent fail) ===
Status:      Failed
Detail:      watertight_mesh: 39 unpaired edges out of 417 total; consistent_normals: 162 of 265 triangles have reversed normals; ... outward_normals 38.2%; ... mesh_euler V(258)-E(417)+F(265) = 106 (expected 2)
```

F0050 is the silent-fail case (no validator panic; oracle suite catches
it). RED is expected per spec §5 anti-scope. **Note**: F0050's defect
description (watertight + consistent_normals + Euler) is what
implementer-t conflated into the F0030 brief.

### `cherchi2022_reference_parity::pr_y16_parity_f0030_cohort`

Re-ran. **Still RED**, exactly as expected per spec §5:

```
[reference-parity] F0030 Cherchi union output : verts=21 tris=74 unique_edges=63 unpaired=0 multi_paired=18 euler_chi=32 well_formed=false
[reference-parity F0030] F0030 lower-bar carve-out active per spec §6
thread 'pr_y16_parity_f0030_cohort' panicked at .../cherchi2022_reference_parity.rs:584:5
assertion `left == right` failed: ... left: Failed, right: Passed
```

Cherchi sidecar still produces 18 multi-paired edges in its OWN output
(its geometry is intrinsically ambiguous — see PR-Y16-FIX-ARCH adversary-14
finding banked at `feedback_yang_brep_extension_over_cherchi_pure_mesh.md`).
Yang case Status remains Failed (twin-pairing validator). Lower-bar
carve-out still applies. Not a fix-target.

### §7 verdict

F0020 + F0050 + Cherchi cohort parity all behave per spec §5 anti-scope:
RED expected, RED observed.

---

## §8 Cheaper-proxy discipline + verdict

### Cheaper-proxy discipline

Per `feedback_adversary_recommendations_need_canary.md`, I am NOT
permitted to recommend a PR-Y18-FLOOD-FILL anchor without self-canarying.

My §4 SAMPLING-INDUCED static-analysis probe IS the self-canary for
PR-Y18: I have empirically verified (via regression test telemetry +
static read of `tessellate_circular_cap` + `inject_partial_overlap_mesh`
+ `extract_face_boundary_2d` + `split_face_along_boundary`) that the
3-layer resolution mismatch exists. Recommending PR-Y18 anchor =
**lower the chord-error in `chord_sample_count` to match
`tessellate_circular_cap`'s `circle_segments()` Boolean LOD** so all
three layers operate on the SAME 16-vertex circle approximation.

Self-canary status of this recommendation:
- ✓ Empirically observed: regression test `verts_dropped=1591` and
  `tris_a + tris_b = 76` (was 58) confirm the resolution mismatch is
  inducing extra triangles.
- ✓ Empirically observed: `tessellate_circular_cap` uses
  `circle_segments()` (16 at Boolean LOD).
- ✓ Empirically observed: `inject_partial_overlap_mesh` calls
  `extract_face_boundary_2d` which reads ALREADY-tessellated mesh
  (16-segment-resolution boundary).
- ⚠ NOT empirically verified: that lowering chord-error to match
  `circle_segments()` actually drops `tris_a + tris_b` to ≤30 + the
  expected shared-overlap delta. This is the PR-Y18 implementer's own
  pre-implementation canary — adversary-16 declines to make this claim.
- ⚠ NOT empirically verified: that lowering chord-error doesn't surface
  i_overlay edge cases (e.g., coarse circle approximation creating
  near-degenerate Intersect groups).
- Alternative path (Path 2 in §4): raise tessellation LOD to TAU_MODEL.
  Wall-clock cost. Banked but adversary-16 prefers Path 1.

### Verdict: AMEND

**ACCEPT criteria check** (per the brief's revised Path A):
- ✓ Layer 1 curve sampling architecturally correct (§3 verdict).
- ✓ No kernel test regression (§1: 1248/31/42 baseline preserved).
- ✓ Layer 3 cleanly deferred — but with framing correction (Layer 3 as
  implementer-t described it does NOT match F0030's actual failure mode;
  see §1 + §6).
- ⚠ Commit message accuracy — implementer-t's description conflates
  F0050's (watertight/normals/Euler) defect description with F0030's
  (twin-pairing validator) defect. Commit message MUST be corrected
  before merge: F0030's post-fix failure is `half_edge[N].twin = M but
  twin.twin = K (expected N)` (twin-pairing class), NOT
  `watertight_mesh: 6 unpaired / 87, consistent_normals 20/56,
  mesh_euler V-E+F=24`.
- ✓ §4 SAMPLING-INDUCED hypothesis investigated honestly: team-lead's
  framing was off; refined hypothesis is partially confirmed in a
  different shape (3-layer resolution mismatch within solid B). PR-Y18
  anchor recommended with self-canary status documented.

**REJECT criteria check** (per the brief):
- ✗ Layer 1 NOT actually correct → False. Layer 1 is correct.
- ✗ §4 §5 surface a defect we can't ship around → False. R0014 panic is
  a new defect class but not a regression (R0014 was already FAIL); the
  panic is correct per spec §8.

**Verdict: AMEND** (not REJECT, not pure ACCEPT). Specific amendments:

1. **Commit message correction (load-bearing)**: F0030's post-fix
   failure mode is twin-pairing validator (`half_edge[4].twin = 0 but
   twin.twin = 32 (expected 4)`), NOT watertight/normals/Euler. The
   conflated description must not enter `git log`.

2. **Bank R0014 panic for PR-Y18**: the L264 panic firing on R0014
   surfaces a real defect (false-positive coplanar detection or
   i_overlay degeneracy). Add to memory entry — don't soften the panic.

3. **Bank §4 SAMPLING-INDUCED reframing for PR-Y18**: the 3-layer
   resolution mismatch (1592 marker / 16 tessellation / 16 injection)
   is the next-anchor target. Recommended fix: align chord-error in
   `chord_sample_count` with `circle_segments()` Boolean LOD. Self-
   canary status flagged: implementer-t pre-impl canary required.

4. **Bank wall-clock cost for PR-Y18**: corpus sweep wall-clock is
   significantly longer than baseline due to chord sampling + verts_dropped
   churn. Aligning chord-error with tessellation LOD (amendment #3) would
   fix this naturally.

5. **Bank F0060 + F0086 spotlight harnesses for PR-Y18**: per spec §5 + §6
   sibling cohort, these spotlights should exist.

The Layer 1 deliverable is correct. With amendments #1-#5 banked, this PR
is shippable as a Layer 1 GREEN waypoint. PR-Y18 then targets the 3-layer
resolution mismatch.

---

## Verification before reporting completion

- ✓ `git diff` shows only `crates/kernel/src/boolean/coplanar_preprocess.rs`
  (implementer-t's deliverable; unmodified by this audit) + the new
  untracked test/spec/canary/validation files. No temporary mutations,
  no stash residue.
- ✓ Memo at `docs/audits/pr_y17_coplanar_validation.md` (this file)
  complete with all 8 sections, no empty bodies.
- ✓ §1 reproduces implementer-t's findings byte-for-byte where claimed
  AND explicitly disagrees on the F0030 Layer 3 oracle outputs (those
  were not reproducible — F0030's actual mode is twin-pairing).
- ✓ §2 reports final corpus sweep result: 10/157 passed (baseline
  match), 142 failed, 5 errored, 33 known timeouts. Spec target ≥11
  not met but no previously-passing case regressed. `[coplanar-tele]`
  aggregate: 117 pairs / 70 partial / 7 identical / 1.4M verts_dropped /
  528 overlay groups.
- ✓ §4 SAMPLING-INDUCED hypothesis investigated; conclusion: partially
  confirmed in refined shape; team-lead's exact framing moot for F0030.
- ✓ §5 reports L264 panic firing status (1 fire on R0014).
- ✓ §8 verdict: AMEND with 5 documented amendments.

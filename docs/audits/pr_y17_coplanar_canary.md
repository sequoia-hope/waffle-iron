# PR-Y17-COPLANAR sub-phase 0a — anchor pre-verification canary on F0030

**Author:** canary-runner-3
**Date:** 2026-05-06
**Scope:** READ-ONLY empirical probe of `coplanar_preprocess.rs` on F0030. The branch decision (GREEN / AMBER-1 / AMBER-2 / RED) sizes PR-Y17-COPLANAR's scope.

---

## §1 — F0030 coplanar detection state

**Detection fires.** `[coplanar-tele]` reports `pairs=1` for the F0030 spotlight run:

```
[coplanar-tele] pairs=1 verts_existing=0 verts_split=0 verts_deduped_by_canon_key=0
                verts_dropped=0 mef_ok=0 mef_no_loop=0 overlay_groups=0
                overlay_holes_ignored=0 identical_footprint=0 partial_overlap=0
```

`detect_coplanar_face_pairs` (coplanar_preprocess.rs:76) correctly identifies F0030's single coplanar pair. `split_brep_for_coplanar_pairs` (L181) enters the per-pair loop and increments `COPLANAR_PAIRS_PROCESSED`.

**Geometry framing (corrected vs PR-Y17-TWIN canary):** F0030's `Sketch 2` plane anchor is `EndCapPositive` index 0 of Extrude 1 (verified via `app/tests/cases/assay/F0030.waffle:154-180`), NOT the global z=0 plane that the meta JSON suggests. So the coplanar pair is:

- **Solid A (rectangle box):** TOP cap face at z=0.273588, outward normal +Z, footprint = rectangle 0.4246 × 0.3967 (full extents in xy)
- **Solid B (circle cylinder):** BOTTOM cap face at z=0.273588, outward normal −Z, footprint = circle radius 0.0513 at origin
- **`same_direction = false`** (anti-parallel)
- **B ⊂ A geometrically** (circle entirely contained inside rectangle)

So this is a Yang §4.5.5 **partial-overlap anti-parallel** case — overlap = circle, A-only = rectangle minus circle, B-only = ∅. The (correct) injection target is `inject_partial_overlap_mesh` per yang_integration.rs:725-737.

The verbose `[COPLANAR DETECT]` and `[COPLANAR SPLIT]` traces are gated by `#[cfg(test)]`, which is not active when `test-harness` builds the kernel as a library. Telemetry counters and post-call summary fire unconditionally; they are the load-bearing evidence here.

---

## §2 — F0030 injection state

**Both injection paths skipped.** `identical_footprint=0` AND `partial_overlap=0` confirms neither marker is set during `split_brep_for_coplanar_pairs`. Therefore:

- `inject_identical_footprint_mesh` (yang_integration.rs:708): runs but immediately returns because no pair has `is_identical_footprint == true`.
- `inject_partial_overlap_mesh` (yang_integration.rs:726): runs but immediately returns because no pair has `is_partial_overlap == true`.

Tessellation proceeds with stacked-cap meshes. No vertex splits, no `mef` calls, no shared triangulation.

**Where the per-pair loop bails (load-bearing finding).** Temporary unconditional probe added at L259-281 (now reverted; `git diff` clean — verified) captured:

```
[CANARY3] pair 0 same_dir=false normal=[0,0,1] off=0.273588
[CANARY3]   poly_a (4 pts):
[CANARY3]     A0: ( 0.212289, -0.198365)
[CANARY3]     A1: ( 0.212289,  0.198365)
[CANARY3]     A2: (-0.212289,  0.198365)
[CANARY3]     A3: (-0.212289, -0.198365)
[CANARY3]   poly_b (1 pts):
[CANARY3]     B0: (-0.051318,  0.000000)
[CANARY3]   overlay Intersect result: 0 groups; group0 len=0
```

`poly_b` has **1 vertex**. The cylinder bottom-cap face's B-Rep loop is a single periodic seam edge (the curve is parametric `CurveGeom::Circle`, the loop has one half-edge whose start vertex == end vertex). `collect_face_loop_2d` (L444) walks the loop reading only the half-edge `origin` vertex positions — it never consults `WaffleSolid::edge_geometry` (the `BTreeMap<EdgeIdx, CurveGeom>` at waffle_kernel.rs:35) and never samples the curve geometry. Output is a 1-point degenerate polygon.

i_overlay's `Intersect` over a 4-point convex rectangle and a 1-point set returns **0 groups**. The check `if overlap.is_empty() || overlap[0].is_empty()` at L264 fires the silent `continue`. `OVERLAY_GROUPS` is never incremented (matches `overlay_groups=0` in telemetry); marker flags are never set.

This is **NOT** a guard-fix-style AMBER-2 (predicate too tight); it is a **polygon-construction-incomplete** AMBER-2 — the function passes a degenerate input to i_overlay, and i_overlay does the right thing with what it was given.

---

## §3 — F0030 post-injection collision count

**Collision count: 11 (UNCHANGED from PR-Y17-TWIN baseline).** Captured from same run with `TWIN_DEBUG=1`:

```
[topo-extract] summary: paired=21, unpaired=4, ambiguous=11
[twin-oracle] total_directed_edges=79
[twin-oracle] unpaired_count=37
[twin-oracle] collision_count=3
```

`collision_count=3` from `[twin-oracle]` reports 3 distinct cluster offenders, but `ambiguous=11` from `[topo-extract]` reports the 11 individual collision-arm fires. Both are consistent with PR-Y17-TWIN canary §1 (which counted 11 collision cases and reported `collision_count=3`).

**Stage A confirms stacking** (`/tmp/viz/f0030_canary3/F0030/stage_A.obj` + labels CSV):

- Total: 29 vertices, 58 triangles
- Triangles labeled by origin: 30 from A, 28 from B
- 13 vertices live AT the cap plane z=0.273588 (4 rectangle corners, 6 circle perimeter samples, cylinder center, 2 extra subdivision verts)
- 28 triangles touch ONLY cap-plane vertices: 20 labeled A (top of box, fan-triangulated through circle perimeter) + 8 labeled B (bottom of cylinder cap)
- The 8 B cap-tris are NOT byte-identical to any A cap-tri; they are independently tessellated. This is the "stacked redundantly" failure mode that `inject_partial_overlap_mesh` is designed to eliminate (per Yang §4.5.5: "share identical sampling points on their boundaries" + Fig. 16).

The Yang-pipeline run terminates at:
```
[A15.6] Yang boolean pipeline failed (not falling through):
  half_edge[4].twin = 0 but twin.twin = 29 (expected 4)
```
Status: `Failed`, detail: `auto-union-failed`. Spotlight is RED. F0030's defect is NOT resolved by the existing 90% coplanar implementation.

---

## §4 — Branch decision

**Branch: AMBER-2** (preprocessing fires + classification short-circuits before injection markers are set).

The PR-Y17-TWIN abort document anticipated AMBER-2 as "preprocessing fires but injection silently bails on identical-footprint anti-parallel case → defect is in injection guard logic". My finding is the same shape but a different mechanism: the bail is in `split_brep_for_coplanar_pairs` (one level UP from `inject_*`), at the i_overlay-empty check, because the polygon passed to i_overlay is degenerate (1 vertex). The injection functions never see a marker because the upstream loop never reaches the marker-setting line.

Per the plan §Risks #1, all four branches have viable PR scopes. AMBER-2 here means:

- PR scope: small-to-moderate (~50-150 LOC)
- Touch points: `collect_face_loop_2d` (L444; needs to consult `WaffleSolid::edge_geometry` and sample `CurveGeom` variants into chord polygons), call sites at L230 / L232 / L2297 / L2304 / L3116 / L3118 (likely just need to thread the new signature through)
- The injection functions themselves (`inject_identical_footprint_mesh`, `inject_partial_overlap_mesh`) are likely correct — they just never got called. Sub-phase 0e adversary can verify that completing the polygon construction is sufficient to drive F0030 GREEN, or whether downstream gaps surface.

Not GREEN: collision count is unchanged from baseline; the existing implementation does not resolve F0030 today.

Not AMBER-1: the injection functions are not even invoked, so we cannot say "injection runs but has a gap". Once polygons are correct, they may run cleanly (sub-phase 0d will verify) or surface a downstream injection gap (sub-phase 0e adversary surfaces).

Not RED: detection is correct (`pairs=1`, plane normal/offset match the actual coplanar pair); the upstream call site (`yang_integration.rs:703-738`) is correct.

---

## §5 — Self-canaried recommendation for sub-phase 0d implementer

Per `feedback_adversary_recommendations_need_canary.md`: this section cites empirical observations from §1-§3, not inference.

**Empirically verified:**
- `collect_face_loop_2d` produces a 1-vertex `poly_b` for F0030's cylinder bottom-cap face. (CANARY3 probe output, cited in §2.)
- `WaffleSolid::edge_geometry` exists as `BTreeMap<EdgeIdx, CurveGeom>` (waffle_kernel.rs:35; verified via grep). It IS populated for cylinder cap faces (the F0030 test ran tessellation successfully and produced a meshed cylinder, so the curve geometry is available).
- The bail point is `if overlap.is_empty() || overlap[0].is_empty()` at L264. Probe-confirmed it fires for F0030's pair 0.
- Telemetry counters fire unconditionally; `#[cfg(test)]`-gated traces do not fire under `cargo test -p test-harness`.

**Empirically NOT verified (the implementer's pre-implementation canaries, per the feedback rule):**
- That `WaffleSolid::edge_geometry` for F0030's cylinder edge contains a `CurveGeom::Circle` variant (not a `Line` or `Spline` masquerading as the cylinder seam). Probe before fixing: enumerate `solid_b.edge_geometry` for `pair.face_b`'s loop edges; expect at least one `CurveGeom::Circle` (or whatever the cylinder cap's seam is represented as).
- That sampling the curve into a chord polygon at TAU_MODEL chord error produces a polygon where i_overlay returns `Intersect` non-empty against the rectangle. The implementer should add a temporary probe asserting `overlap.len() > 0 && overlap[0].len() >= 3` after the new sampling code, BEFORE writing the injection-completion path.
- That after the polygon construction is fixed, `inject_partial_overlap_mesh` actually fires (`partial_overlap` counter increments) and produces shared triangles at the cap plane (Stage A re-dump should show fewer total cap-plane tris and matching A/B triangle bytes in the overlap region).

**Anchor recommendation for sub-phase 0d:** modify `collect_face_loop_2d` to accept the relevant `&BTreeMap<EdgeIdx, CurveGeom>` and, for each half-edge, check the edge's `CurveGeom`. If the curve is non-linear (Circle, Ellipse, Spline, etc.), sample chord points at the configured tessellation tolerance (likely `TAU_MODEL` or the same tolerance that `tessellate` uses, for byte-identical sampling between the i_overlay polygon and the eventual cap mesh). For Lines, the existing single-vertex emit is correct. The 6 call sites (L230, L232, L2297, L2304, L3116, L3118) will need the additional argument (or a thin wrapper that pulls `&solid.edge_geometry`).

**Self-canary for THIS recommendation:** the recommendation is empirically supported by the CANARY3 probe (§2) — the `poly_b=1pt` evidence directly demonstrates that the loop-walk-only approach is incomplete for parametric edges. The recommendation does NOT depend on inferring what `inject_partial_overlap_mesh` does post-fix; sub-phase 0d's own canary verifies that.

**Anti-pattern guard (per `feedback_yang_only.md`):** do NOT add a fallback path for "i_overlay returned empty → skip silently". The current `continue` at L264 IS that fallback today. Per Yang §4.5.5, a coplanar pair detected at Stage 0a MUST be processed; an empty overlap means the polygon construction is wrong (per this canary) or the pair was a false positive (per detection logic). Both are bugs to surface, not silently skip. Sub-phase 0d should consider replacing the silent `continue` with a panic-or-error after the polygon-construction fix lands, to fail loudly on future false positives.

**Banked observations for adversary (sub-phase 0e):**
- The PR-Y17-TWIN canary memo's geometric framing of "F0030 = identical-footprint stacked caps" was wrong. F0030 is a **partial-overlap** case (B ⊂ A, anti-parallel). The previous diagnosis was correct on "coplanar-cap stacking" but wrong on "identical-footprint". `inject_identical_footprint_mesh` is NOT the F0030 fix anchor; `inject_partial_overlap_mesh` is.
- This contradicts the just-banked memory `yang_f0030_coplanar_root_cause.md` which says "identical-footprint coplanar-cap stacking". Sub-phase 0f team-lead should refresh that memory entry.

---

## Verification

- `git diff crates/kernel/src/boolean/coplanar_preprocess.rs` clean (CANARY3 probe added L259-281, fully reverted; verified by `git diff --stat` empty).
- `git status` only `docs/audits/pr_y17_coplanar_canary.md` (this file) + the unrelated untracked `output.obj` from earlier session state.
- §1-§5 each have non-empty bodies.
- §3 reports a specific collision count: 11 (matches PR-Y17-TWIN baseline; not "see notes").
- §4 picks ONE branch: AMBER-2 (not "GREEN-or-AMBER").
- §5 references empirical observation (`[CANARY3]` probe output cited in §2) and self-canaries the recommendation (notes which claims need pre-implementation verification).
- Probe ran in <1s spotlight time × 2 spotlight runs + ~3s rebuild between runs. No long-running operations.

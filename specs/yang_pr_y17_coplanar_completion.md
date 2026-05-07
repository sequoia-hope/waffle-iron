# PR-Y17-COPLANAR — Yang §4.5.5 cap-face overlap completion (curve-sampling fix)

**FIP:** §3 + §8 Bug Fix Variant. **Branch (per canary-runner-3 §4):** AMBER-2 — polygon-construction-incomplete. **Anchor case:** F0030. **Sibling probe (bonus):** F0060. **Banked separately:** F0020 (PR-Y18-DOWNSTREAM), F0050 (PR-Y19-NORMALS-EULER).

**This PR is COMPLETION, not from-scratch.** `crates/kernel/src/boolean/coplanar_preprocess.rs` is ~90% implemented per Yang 2025 §4.5.5: detection (`detect_coplanar_face_pairs`, L76), 2D Boolean orchestration (`split_brep_for_coplanar_pairs`, L181), identical-footprint injection (`inject_identical_footprint_mesh`, L957), partial-overlap injection (`inject_partial_overlap_mesh`, L1104). The single load-bearing gap is in `collect_face_loop_2d` (L420), which walks B-Rep half-edge `origin` vertices only and does not consult `WaffleSolid::edge_geometry`. For parametric edges (Circle, Arc, Ellipse), this produces degenerate 1-vertex polygons; i_overlay returns 0 overlap groups; `split_brep_for_coplanar_pairs` short-circuits silently at L264 before either injection marker is set. F0030's cylinder bottom-cap face is the canonical instance.

---

## §1 Goal

Complete `collect_face_loop_2d` so it samples non-linear `CurveGeom` variants into chord polygons before passing them to i_overlay. Result: F0030's coplanar pair classifies as `partial_overlap=1`, `inject_partial_overlap_mesh` fires, Stage A no longer stacks redundant cap-plane triangles, downstream `[topo-extract]` collision count drops from 11 to 0, and `spotlight_f0030` goes RED → GREEN.

This is the **completion of the existing 90% Yang §4.5.5 implementation**, not a from-scratch port. Yang 2025 §4.5.5 mandates: "the common part and the other two parts share identical sampling points on their boundaries" (Fig. 16). The current Rust path satisfies this for poly-line faces; this PR extends it to faces with parametric boundary curves.

---

## §2 Reference parity contract

Three invariants this PR commits to. Each cites (a) Yang §4.5.5, (b) current Rust behavior (today), (c) post-fix behavior, (d) covering test.

**Invariant 1 — 2D Boolean polygons accurately represent face boundaries.**
- (a) Yang 2025 §4.5.5 + Fig. 16: coplanar overlap detection requires polygons that correctly describe each face's boundary footprint in the shared 2D basis.
- (b) Current: `collect_face_loop_2d` (`coplanar_preprocess.rs:420`) emits one `(VertexIdx, [u, v])` pair per outer-loop half-edge `origin`. For F0030's circular cap (single periodic seam edge), this is 1 vertex.
- (c) Post-fix: for half-edges whose `EdgeIdx` resolves to a non-`Linear` `CurveGeom` (`Circular`, `Arc`, `Elliptical` per `geometry/curve.rs:13-18`), the function emits sampled chord points spanning the curve at TAU_MODEL chord error. `Linear` edges remain single-emit.
- (d) Unit test in 0c that constructs an arena with one circular face, calls `collect_face_loop_2d`, asserts ≥3 chord points and that all sampled points lie within TAU_MODEL of the true circle.

**Invariant 2 — F0030's coplanar pair classifies as `partial_overlap`.**
- (a) Yang §4.5.5: a coplanar pair detected at Stage 0a MUST be processed; both injection paths (identical_footprint, partial_overlap) follow the 2D Boolean classification (overlap polygon non-empty + at least one of A-only / B-only non-empty).
- (b) Current: `[coplanar-tele] pairs=1 ... overlay_groups=0 identical_footprint=0 partial_overlap=0` (canary §1). Both injection paths skipped.
- (c) Post-fix: `[coplanar-tele] pairs=1 ... overlay_groups≥1 partial_overlap=1` for F0030 (B-only is empty since circle ⊂ rectangle, A-only = rectangle minus circle, overlap = circle).
- (d) Regression test in 0c that runs the F0030 spotlight with diagnostic capture and asserts `partial_overlap=1` fires.

**Invariant 3 — Marker-set pair drives injection that eliminates cap-plane triangle stacking.**
- (a) Yang §4.5.5 + Fig. 16: identical sampling on the overlap region means meshes from A and B share triangulation bytes in the shared region.
- (b) Current (canary §3): Stage A has 28 cap-plane triangles (20 from A + 8 from B), independently tessellated, NOT byte-identical. `[topo-extract] summary: paired=21, unpaired=4, ambiguous=11`.
- (c) Post-fix: `inject_partial_overlap_mesh` (`coplanar_preprocess.rs:1104`) runs and produces a shared triangulation in the overlap region. `[topo-extract] summary` shows `ambiguous=0` (or substantially reduced; 0 is the contract; if downstream gaps surface, sub-phase 0e adversary surfaces them).
- (d) Regression test in 0c plus `cherchi2022_reference_parity::pr_y16_parity_f0030_cohort` (`crates/test-harness/tests/cherchi2022_reference_parity.rs:607`). Both currently RED; both must be GREEN post-fix.

---

## §3 Code site to fix

**Primary:** `collect_face_loop_2d` at `crates/kernel/src/boolean/coplanar_preprocess.rs:420`.

Modify the signature to accept the relevant edge geometry. Recommended (implementer chooses exact form):

```rust
fn collect_face_loop_2d(
    arena: &TopoArena,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,   // NEW
    face_idx: FaceIdx,
    origin: &[f64; 3],
    u_axis: &[f64; 3],
    v_axis: &[f64; 3],
) -> Vec<(VertexIdx, [f64; 2])>
```

For each half-edge in the outer loop:
1. Read `arena.half_edges[he.0].origin` (existing).
2. Read `arena.half_edges[he.0].edge` to get the `EdgeIdx`. Look up `edge_geometry.get(&edge_idx)`.
3. If absent or `CurveGeom::Linear(_)`: emit single `(VertexIdx, [u, v])` (existing path).
4. If `CurveGeom::Circular | Arc | Elliptical`: sample chord points per §4. The first sample MUST be the half-edge's `origin` vertex (for stable indexing into `boundary_verts` downstream); subsequent samples are interior chord points along the curve. Use `VertexIdx::INVALID` (or whatever sentinel `split_face_along_boundary` already handles for non-existing-vertex points; if no sentinel exists, the implementer adds one and `split_face_along_boundary` snaps such points to existing-or-split as it does today at L506-L529).

**Production call sites** (verified by grep):
- `coplanar_preprocess.rs:230` — `poly_a` for `face_a` in `split_brep_for_coplanar_pairs`
- `coplanar_preprocess.rs:232` — `poly_b` for `face_b` in `split_brep_for_coplanar_pairs`

Both sites already have `solid_a` and `solid_b` in scope; thread `&solid_a.edge_geometry` and `&solid_b.edge_geometry` respectively.

**Test call sites** (inside `#[cfg(test)] mod tests { ... }` starting at L1810):
- `coplanar_preprocess.rs:2273, 2280` — F0003 diagnostic test (debug-print of poly_a/poly_b)
- `coplanar_preprocess.rs:3092, 3094` — z=1 anti-parallel winding test

These must be updated to the new signature. **Note:** the canary memo §5 listed L3116 and L3118 as call sites — those are actually inside the `signed_area_2d` test helper (it processes the polygon AFTER `collect_face_loop_2d` returns) and do NOT call the function directly. Implementer-t verifies this in sub-phase 0d's anchor-pre-verification canary.

**Silent-continue site:** `coplanar_preprocess.rs:264`:
```rust
if overlap.is_empty() || overlap[0].is_empty() {
    #[cfg(test)] eprintln!("[COPLANAR SPLIT]   -> Skipped: no overlap");
    continue; // Coplanar but non-overlapping
}
```
Per §8 below, this `continue` becomes EXPLICIT after the polygon-construction fix lands. Spec mandates: a diagnostic counter increment + an `eprintln!` (not `#[cfg(test)]`-gated) reporting the offending pair's plane/normal/face indices, AND when `YANG_BOOLEAN=1` is set (already required for the entire pipeline), `panic!` with the polygon contents. Rationale: post-fix, an empty overlap on a detected coplanar pair is a real bug (false-positive detection, or an unhandled curve variant) — the silent skip masks it.

---

## §4 Curve sampling spec

**Tolerance.** Use `crate::units::TAU_MODEL` (1e-7 m) as chord error per CLAUDE.md units rules ("TAU_MODEL=1e-7"). Per Cherchi 2022 §3 input preconditions ("manifold, watertight, no self-intersections"), chord sampling must be conservative enough to preserve face boundary topology — under-sampling can produce self-intersecting polygons when adjacent curves share endpoints.

**Per-variant rules** (`CurveGeom` enum at `crates/kernel/src/geometry/curve.rs:13-18`):

- **`CurveGeom::Linear(Line3D)`**: single emit (no change). The line endpoint is the next half-edge's `origin`; no interior samples.

- **`CurveGeom::Circular(Circle3D)`** (full circle, single periodic seam edge — F0030's case): sample N chord points uniformly in parameter t ∈ [0, 2π). Choose N from the chord-error formula:
  ```
  N = ceil(2 * π / acos((r - TAU_MODEL) / r))
  ```
  Clamped to N ≥ 8 (degenerate floor for tiny radii where the formula returns near-2π). For F0030 (r = 0.0513, TAU_MODEL = 1e-7), N ≈ 718 — large but tractable for a one-shot 2D Boolean (i_overlay is O(N log N) per group). If the resulting polygon count materially impacts wall-clock, sub-phase 0d implementer reports; otherwise we accept the cost as the price of paper-faithful sampling.

- **`CurveGeom::Arc(Arc3D)`**: similar to Circular but parameter range t ∈ [0, sweep_angle]. N from the same chord-error formula scaled by sweep_angle / (2π). First sample at t=0 (the arc start point, which equals the half-edge's `origin`); last sample at t=sweep_angle (the arc end, which equals the next half-edge's `origin` — emit at index N-1 to ensure the next half-edge's `origin` is not duplicated).

- **`CurveGeom::Elliptical(Ellipse3D)`**: chord-error sampling against the larger semi-axis (semi_major), conservative bound:
  ```
  N = ceil(2 * π / acos((semi_major - TAU_MODEL) / semi_major))
  ```
  Same N ≥ 8 floor. (Tighter chord-error bounds exist for ellipses; the conservative bound is sufficient for correctness and the over-sampling cost is bounded by Circle case.)

**Spline curves:** NOT in `CurveGeom` enum today (per `geometry/curve.rs:13-18`). If a future curve variant is added (e.g., `BSpline`, `NURBS`), the spec for that variant lands in PR-Y17-COPLANAR-2; THIS PR's match arm panics with `unimplemented!("CurveGeom::<variant> sampling for coplanar preprocess — banked as PR-Y17-COPLANAR-2")` rather than silently emitting a single vertex. Per §8, no fallback.

**Sampling parametrization correctness probe (sub-phase 0d).** Before relying on the sampling, implementer-t adds a temporary unit canary asserting that for F0030's specific cylinder edge:
1. `solid_b.edge_geometry.get(&edge_idx)` returns `Some(CurveGeom::Circular(_))` (NOT `None`, NOT `Linear`).
2. After sampling at TAU_MODEL chord error, `overlap.len() ≥ 1 && overlap[0].len() ≥ 3` for the F0030 (rectangle ∩ circle) overlay.

These canaries are in line with `feedback_anchor_before_fix.md` and canary-runner-3 §5's "empirically NOT verified" list.

---

## §5 Test plan (per FIP §4.2)

- `spotlight_f0030` (`crates/test-harness/tests/assay_randomized.rs:323`) RED → GREEN. Currently fails at `[A15.6] Yang boolean pipeline failed ... half_edge[4].twin = 0 but twin.twin = 29 (expected 4)` per canary §3.
- New regression test (test-author-g writes in 0c): asserts on F0030 that (a) `[coplanar-tele] partial_overlap=1` fires (parse stderr), (b) `inject_partial_overlap_mesh` produces shared cap-plane triangles (probe Stage A triangle count and origin distribution at z=0.273588 — total cap-plane triangles drops from 28 to ≤20 with byte-identical A/B in the overlap region), (c) `[topo-extract]` collision count = 0.
- `cherchi2022_reference_parity::pr_y16_parity_f0030_cohort` (`cherchi2022_reference_parity.rs:607`) RED → GREEN.
- `spotlight_f0020`, `spotlight_f0050` STAY RED (different defect classes per `yang_f0030_coplanar_root_cause.md`; F0020 downstream of `flood_fill_patches`, F0050 normals + Euler). Confirmed expected.
- 953 kernel tests + 89 test-harness lib tests + 162 feature-engine tests baseline NO regression. The 4 in-tree test sites updated (L2273, L2280, L3092, L3094) must continue to pass.
- Yang fast subset baseline 10/157 → ≥11/157 (F0030 returns).
- Adversary 0e probes F0060 sibling (boss + cut, both circles, same plane); records help/no-help/regress.

---

## §6 Adversary-13/14 amendments addressed

- **F0051 latent exposure** (PR-Y16-FIX-ARCH adversary-14 §1): F0051 has the same Stage 6 collision pattern as F0030. Per the plan §Out of scope, F0051 may resolve as a side-effect when coplanar preprocessing completes for the cap-face case. Adversary-16 §3 reports observed effect (resolves / partially resolves / unaffected); not gating.
- **Cohort sibling check** (F0060, F0086): adversary-16 §3-§4 reports. F0060 is a plausible coplanar sibling (boss + cut, both circles, same plane); F0086 is multi-coplanar swiss-cheese, expected to be partially-affected at most. Not gating per plan §Risks #4.

---

## §7 Anti-scope (explicit OUT)

- F0020 downstream-of-flood_fill_patches defect → PR-Y18-DOWNSTREAM
- F0050 normals + Euler defect → PR-Y19-NORMALS-EULER
- Cherchi sidecar harness changes (sidecar parity F0030 lower-bar already passes; this PR is upstream of mesh boolean)
- `i_overlay` 4.4 library replacement (already production-grade; if it has a bug, file upstream)
- Spline / NURBS curve sampling (defer to PR-Y17-COPLANAR-2 only IF a future test case adds a new `CurveGeom` variant; today's enum only has Linear/Circular/Arc/Elliptical)
- ManifoldPatchGraph design changes
- PR-Y16-INV `[twin-oracle]` post-pairing block (regression canary; stays gated)
- Removing the unused `inject_conformal_coplanar_mesh` function (cleanup candidate; not in this PR)
- Removing deprecated S-H clipping pipeline (`clip.rs`)
- Performance optimizations for the curve-sampled polygon (caching, parallel i_overlay, etc.)
- Re-flipping the WASM gate (still intentional)
- Removing the `YANG_BOOLEAN` env-var gate for native (still intentional)
- Visualizing cap-face overlap in PR-VIZ pane (PR-VIZ-4 candidate)
- F0031–F0040 cylindrical quad-strip cohort (separate; PR-Y15c-fix-2 already addressed)
- R0020/R0021 render-LOD bijective failures
- R0071 kernel hang
- Mobile UI polish
- Fillet/chamfer/shell (DEFERRED INDEFINITELY per CLAUDE.md)

---

## §8 No-fallback commitment (per `feedback_yang_only.md`)

This PR does NOT add per-curve fallback paths. Concretely:

- **No "if curve sampling fails, emit single vertex"**: the `match` over `CurveGeom` variants is exhaustive. Any future variant (e.g., `BSpline`) hits the `unimplemented!()` arm, not a silent single-vertex emit.
- **No "if i_overlay returns empty, accept silently"**: the `coplanar_preprocess.rs:264` `continue` is replaced with an explicit diagnostic + (under `YANG_BOOLEAN=1`) panic. The current `continue` IS the fallback today; this PR removes it. Choice of behavior:

  **Spec choice: panic-when-`YANG_BOOLEAN=1`, diagnostic-otherwise.**

  Rationale: per `feedback_yang_only.md` "raise exception if unlabeled", and consistent with PR-Y15c-fix-2.2 (which promoted `unwrap_or_else` to `unwrap_or_else(|| panic!("A15.5 ..."))` after audit confirmed 0/190 fires — see `yang_pr_y14a_outcome.md`). After the polygon construction is fixed, an empty overlap on a detected coplanar pair indicates either (a) false-positive detection (bug), or (b) an unhandled curve variant (also bug). Both should fail loudly under the production gate. Outside `YANG_BOOLEAN=1` (e.g., MockKernel tests that never exercise this path), a diagnostic eprintln preserves test-suite ergonomics.

- **No tolerance widening**: chord-error is fixed at TAU_MODEL. If a future test surfaces a degenerate near-degenerate-circle case, that is a separate spec.
- **No "fall back to bbox" for tiny circles**: the N ≥ 8 floor is a degenerate-floor for the `acos` formula's near-2π output, not a precision fallback.

---

## §9 FIP role table

| Sub-phase | Agent | Role | Output |
|---|---|---|---|
| 0a Canary | canary-runner-3 (NEW) | Empirical pre-verification | `docs/audits/pr_y17_coplanar_canary.md` (DONE) |
| 0b Spec | spec-writer-p (NEW) | This document | `specs/yang_pr_y17_coplanar_completion.md` |
| 0c Tests | test-author-g (NEW) | RED tests per spec | New tests in `crates/test-harness/tests/` (likely `pr_y17_coplanar_regression.rs` or extension of `coplanar_curved.rs`) |
| 0d Implement | implementer-t (NEW) | `collect_face_loop_2d` curve sampling + L264 site replacement | `crates/kernel/src/boolean/coplanar_preprocess.rs` delta (~50-150 LOC) |
| 0e Adversary | adversary-16 (NEW) | Independent re-run + corpus sweep + cohort sibling check | `docs/audits/pr_y17_coplanar_validation.md` |
| 0f Close-out | team-lead | clippy/fmt/WASM/memory/commit | All deltas committed + WASM bundle in same commit |

---

## §10 Geometry framing note (correction to PR-Y17-TWIN canary memo)

For the implementer's reference (re-stating canary-runner-3 §1 + REFINEMENT 1+2 in `yang_f0030_coplanar_root_cause.md`):

- F0030 is **partial-overlap anti-parallel** (B ⊂ A: circle ⊂ rectangle; A-only = rectangle minus circle; B-only = ∅), NOT identical-footprint.
- Sketch 2's plane is `EndCapPositive` of Extrude 1 (z = 0.273588, +Z normal in A's frame, -Z normal in B's local frame), NOT global z=0.
- Fix anchor is `inject_partial_overlap_mesh` (`coplanar_preprocess.rs:1104`), NOT `inject_identical_footprint_mesh` (`coplanar_preprocess.rs:957`). Once `collect_face_loop_2d` produces a proper chord polygon for the circle, the L297 `a_only_empty && b_only_empty` check will be false (A-only = rectangle minus circle, non-empty; B-only = ∅, empty) and the L312 `is_partial_overlap = true` line will fire, routing F0030 through the partial-overlap injection path.
- The L297 check (identical-footprint detection) remains correct as written; F0030 simply doesn't take that branch. Identical-footprint cases (e.g., two coincident rectangles of the same dimensions) take the L297 branch; partial-overlap cases (F0030) take the L312 branch.

---

## Verification before reporting completion (sub-phase 0d)

- `git diff --stat` shows changes only in `crates/kernel/src/boolean/coplanar_preprocess.rs` (production code) and the new regression test file from 0c. The 4 in-tree test sites are updated to the new signature.
- `cargo test -p kernel` baseline (953 pass + 14 ignored) preserved.
- `cargo test -p test-harness` baseline (89 lib + 1 ignored) preserved + new regression test GREEN.
- `cargo clippy -p kernel` clean.
- `cargo fmt --check` clean.
- `spotlight_f0030` GREEN.
- `cherchi2022_reference_parity::pr_y16_parity_f0030_cohort` GREEN.
- Yang fast subset ≥11/157 (was 10).
- F0020 + F0050 spotlights stay RED (expected).

**Estimated production code delta:** 50-150 LOC in `coplanar_preprocess.rs` (signature change + curve-sampling helper + 6 call sites + L264 silent-continue replacement). Test delta: 30-50 LOC.

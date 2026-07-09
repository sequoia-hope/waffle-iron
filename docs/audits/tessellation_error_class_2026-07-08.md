# Assay ERROR class audit — `TessellationFailed` (render CDT), 2026-07-08

**Scope.** The #2 assay ERROR class after Stage-4 `LocalRefinementRequired`:
11 of 41 ERROR cases fail in **kernel-v2 render tessellation**
(`crates/kernel-v2/src/tessellate.rs`), *downstream* of a boolean that already
produced a solid. Baseline at audit: 208 CORRECT / 0 WRONG / 41 ERROR
(`target/assay_kv2_report.json`).

This audit dissects the class, assigns each case to a sub-mechanism, and locates
the root cause **per sub-family**. Conclusion up front: the class is **not one
bug** — it splits into three sub-mechanisms with two distinct root layers
(upstream yang vs in-crate unroll), none of which has a P9-clean quick fix. This
is a diagnosis (no behavior change), matching the Stage-4 LRR diagnosis commits
(d7da34ae, 92992e6b).

## Census (11 cases, 3 sub-mechanisms)

| Sub-mechanism (reason string) | Cases | n | Root layer |
|---|---|---|---|
| `ring rejected by CDT (degenerate/self-intersecting)` | F0045, R0011, R0016, R0072 | 4 | **upstream (yang)** — invalid planar loop |
| `planar/patch triangle collapsed at render precision` | F0078, R0012, R0098 (planar); R0088 (patch) | 4 | mixed — sub-f32 sliver in the face |
| `patch triangulation folded (inverted triangle) — KV9-F2` | R0034, R0054, R0065 | 3 | **in-crate** — unroll ear-clip fold at large `r_unroll` |

Site map (`tessellate.rs`): ring reject = `triangulate_ring` L1928 (floodfill
CDT `yang_rs::cdt_polygon_with_holes_floodfill` rejects the ring); planar
collapse = L3319; patch collapse = L3075/L3794; patch fold = L3118.

## Sub-family 1 — "ring rejected by CDT" = upstream self-intersecting planar loop

**Confirmed root cause (F0045, the canonical small case).** F0045 =
`extrude(circle)+extrude(circle)` — two parallel overlapping cylinders, boss
(union) — the simplest possible boolean. FaceId(9) is a **plane**
(`Surface::Plane`); its outer loop, sampled and projected onto its own plane,
**self-intersects** (2 crossings among 22 vertices). Because a planar loop's
projection onto its own plane is isometric, the 2D self-intersection is a
genuine 3D fold — an **invalid B-Rep face** the CDT correctly refuses.

Per-half-edge dump of FaceId(9)'s loop (14 half-edges, arcs + a 5-segment line
run) localizes the fold to a **junction cluster**: three *distinct* vertices sit
within 0.004–0.011 of each other at the corner (~0.37, −0.18) where two circle
arcs meet the boolean seam —

```
v71 arc-end   (0.373924, -0.182083)   ← loop order threads
v72 line      (0.370229, -0.189484)      v71 → v72 → v75, a zigzag
v75 line      (0.371809, -0.178401)      that crosses the incoming arc
```

They are **not** mergeable duplicates (0.4–1% of the radius apart — a real, if
thin, feature); the loop's `next`-pointer order through them is what
self-crosses. This is the **same near-coincident-junction-cluster wall as the
Stage-4 `LocalRefinementRequired` class** (`plane ∩ cylA ∩ cylB`), surfacing on
a *different* code path: instead of Stage-4 bailing, the vertices survive into
the B-Rep and the render CDT rejects the tangled loop. Fixing the upstream yang
junction handling would address both classes.

**Heterogeneity within the sub-family.** R0011 (the known "ellipse wall") is a
**397-vertex loop at 1e3–4e3 coordinates** — a large multi-lobe curve, not a
single 22-vertex junction spike. Its self-intersection is a separate
complex-loop sub-case, not yet dissected. R0016/R0072 not individually dumped
(R0072 = holed-disc, heavy). So sub-family 1 is *at least* two distinct upstream
mechanisms (junction cluster + complex loop).

**`validate_solid` gap (secondary finding).** `validate_planar_face`
(`validate.rs:551`) checks planarity, Newell/normal agreement, ring winding, and
endpoint-on-surface — but **not loop simplicity**. A self-intersecting planar
loop passes validation and only surfaces at render as the opaque "ring rejected
by CDT". Adding a planar-loop simplicity check would move the failure to a
precise, early, correctly-layered error — but it would **not convert** these
cases (root is upstream); it only re-labels ERROR→ERROR. Deferred (churn without
conversion; the real fix is the upstream junction handling).

## Sub-family 2 — "triangle collapsed at render precision"

The CDT/ear-clip *succeeds* but emits a triangle that is degenerate at **f32**
render precision (two verts bitwise-identical after f32 rounding, or an exactly
zero f32 cross product) — the always-on F0047-class gate (`f32_render_degenerate`,
never a skip/snap, P9). This means the face carries a genuinely sub-f32-thin
sliver region. Split 3 planar (F0078, R0012, R0098) + 1 patch (R0088). Not
dissected per-case this session; the open question is whether the sliver is a
*real* thin feature the boolean legitimately produced (then the case is
genuinely at the resolution floor) or a CDT-produced sliver that a flip pass
should have removed. R0088/R0098 are cases whose *boolean* was previously fixed
(M-C band-scale / non-star ring) — the failure has since moved downstream to
render, consistent with the sliver being upstream geometry.

## Sub-family 3 — "patch triangulation folded (inverted triangle)" = upstream coaxial-rim recovery gap → FIXED 2026-07-09

**⚠️ The original "in-crate unroll fold" diagnosis below was WRONG on two
counts** (corrected 2026-07-09 by full dissection of R0034):

1. **FaceId(299) is a CONE, not a "large cylinder"** — the fold probe reports
   `tan_a=1.010229` (half-angle ≈ 45.3°), so the outward normal is tilted ~45°
   from radial (`n̂ = r̂ − 1.01·â`).
2. **The root is UPSTREAM (yang boolean output), not the in-crate unroll.** The
   folded triangle's three vertices are `w1` (on-surface, r=519.04), `w18`
   (r=517.75 — a CHORD midpoint, 1.29 *inside* the surface), and `w12`
   (on-surface, r=518.86). The full boundary dump shows **all 12 edges of the
   cone patch are `LineSegment`** — the co-circular rim (constant r=519.0353,
   constant axial height) is stored as a coarse **5-chord polyline** (each chord
   spans ~8° at r=519 → sagitta ≈ 1.26, which *exceeds* the render chord
   tolerance ≈ 0.52). Refinement bisects those chords keeping the split point on
   the chord (necessarily — the neighbour face's copy of the chord stays
   straight, so an on-chord split preserves watertightness), 1.29 inside the
   surface, while the interior refinement points sit *on* the surface. In a band
   only 0.34 thick that mismatch tilts the facet inward → `dot = −0.596` → the
   KV9-F2 tripwire correctly fires. The tessellator is *right* to refuse it.

**Where the coarse polyline comes from.** R0034's 3rd op is `revolve(gear)` — a
partial revolve producing a nest of **560 coaxial cone bands** (the revolved
gear teeth). `build_partial_revolve` gives each band clean `Curve::Arc` swept
rims, but the union boolean re-tessellates everything and its mesh output
carries the co-circular cone∩cone / cone∩cylinder rims as untagged chord
polylines. `recover::recover_output_curves` (PR-KV7) is meant to retag such
chord runs back to exact circles — but its retag only fired for a curved lateral
meeting a **⊥ plane**; a rim between two coaxial *laterals* (neither a plane)
was never recovered.

**Fix (2026-07-09).** Extended the recover retag to the **curved ∩ curved
coaxial rim** case: two coaxial cylinder/cone laterals sharing a co-height,
co-radial chord run recover to their exact shared circle (A15 analytical
primacy). Guards: parallel axes, endpoints co-axial-height (excludes
rulings/seams), endpoints on the surface-0 rim, and the arc **midpoint on
surface 1** (excludes skew/offset pairs). Measured on R0034: 500+ genuine
candidates, max midpoint-on-surface-1 residual 1.9e-9, max radius disagreement
9.6e-12 — all ~500× within the scale-relative `band` (1.03e-6). After recovery
the rim is a shared `Arc`, sampled on-surface identically by both faces (no
depressed chord midpoint) → no fold. **R0034 & R0065 ERROR → CORRECT; R0054's
fold is likewise removed** (it now runs long enough to time out in-container —
it was already 131 s at baseline; a heavier band-count case). Fix in
`crates/kernel-v2/src/recover.rs`; regression test
`kv6c_partial_cone_boolean::coaxial_cone_cylinder_rim_recovers_through_boolean`.

---

### Original (incorrect) diagnosis, retained for the trail

**Claimed (R0034).** face 299, `r_unroll=519` (a large cylinder, coords
~500–590). The KV9-F2 fold tripwire (`tessellate.rs:3118`, unit dot < −0.1)
fires on a **thin sliver** in the *unrolled* development:

```
a  p2=(0.000000, 513.779593)   3D=(45.44, 118.08, 570.45)   dot = -0.596
b  p2=(-36.582297, 513.779593) 3D=(19.04, 100.62, 588.75)   (genuinely inverted)
c  p2=(-36.582297, 513.608651) 3D=(18.70, 101.60, 589.19)
```

Width ≈ 36.6, height ≈ 0.17 (aspect ~0.005). Claimed to be an **in-crate**
unroll-map precision fold — DISPROVEN above (the depressed vertex is an upstream
coarse-chord rim midpoint, not an unroll-map artifact). R0054, R0065 share the
reason string and the same coaxial-rim root.

## Recommended next steps (priority order)

1. ~~**Sub-family 3 (patch fold).**~~ **DONE 2026-07-09.** Root was the upstream
   coaxial-rim recovery gap (not an in-crate unroll fold); fixed by extending
   `recover::recover_output_curves` to curved∩curved coaxial rims. R0034 & R0065
   ERROR → CORRECT. See the corrected sub-family 3 section above.
2. **Sub-family 1 (ring reject) is the LRR wall in disguise** — the junction
   cluster (F0045) is `plane ∩ cylA ∩ cylB`, identical to the Stage-4
   `LocalRefinementRequired` root. Do NOT patch the tessellator to swallow the
   tangled loop (P9 — it is a genuinely invalid face). Fix belongs in yang
   junction handling; couple with the Stage-4 conic-triple-junction work
   (`specs/yang_stage4_conic_triple_junction.md`). R0011 (complex loop) is a
   separate upstream sub-case.
3. **Sub-family 2 (render collapse)** — dissect one case (F0078) to decide
   real-thin-feature vs removable-CDT-sliver before touching code.

## Reproduction

```
KV2_RING_REJECT_PROBE=1  ASSAY_CASE=F0045 ...   # dumps the rejected ring's 2D pts + CDT error
KV2_PATCH_FOLD_PROBE=1   ASSAY_CASE=R0034 ...   # dumps the folded triangle's work/3D coords
```
runner: `ASSAY_CASE=<id> ASSAY_CASE_TIMEOUT_SECS=120 ASSAY_JOBS=1 cargo test -p
test-harness --test assay_kv2 -- --exact single_case --ignored --nocapture`.

`KV2_RING_REJECT_PROBE` was added this session (`triangulate_ring`,
`tessellate.rs`) alongside the existing `KV2_PATCH_FOLD_PROBE` — both env-gated,
zero-cost off, matching the crate's `_PROBE` convention.

See also: `stage4_lrr_conic_triple_junction_diagnosis` (memory),
`specs/yang_stage4_conic_triple_junction.md`, `docs/yang_deviations.md` N2.

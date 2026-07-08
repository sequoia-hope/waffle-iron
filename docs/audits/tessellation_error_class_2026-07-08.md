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

## Sub-family 3 — "patch triangulation folded (inverted triangle)" = in-crate unroll fold

**Confirmed (R0034).** face 299, `r_unroll=519` (a large cylinder, coords
~500–590). The KV9-F2 fold tripwire (`tessellate.rs:3118`, unit dot < −0.1)
fires on a **thin sliver** in the *unrolled* development:

```
a  p2=(0.000000, 513.779593)   3D=(45.44, 118.08, 570.45)   dot = -0.596
b  p2=(-36.582297, 513.779593) 3D=(19.04, 100.62, 588.75)   (genuinely inverted)
c  p2=(-36.582297, 513.608651) 3D=(18.70, 101.60, 589.19)
```

Width ≈ 36.6, height ≈ 0.17 (aspect ~0.005) — a near-degenerate sliver at the
patch rim whose 3D winding faces *into* the surface. This is an **in-crate**
defect (the unrolled ear-clip/refinement pipeline, `tessellate.rs` ~2250–3160),
NOT an upstream boolean artifact — the tripwire is correctly loud (P9: better
than shipping inverted geometry), but the unroll triangulation should not
produce the fold. Plausible mechanism: precision loss in the unroll→3D map at
large `r_unroll`, or the refinement splitting a rim sliver against its own
winding. R0054, R0065 share the reason string (not individually dumped; all
three are heavy, ~20s/case). This sub-family is the most **in-scope for
kernel-v2** and the best candidate for a self-contained fix.

## Recommended next steps (priority order)

1. **Sub-family 3 (patch fold), in-crate, most tractable.** Investigate the
   unrolled ear-clip/refinement at large `r_unroll` (R0034 canonical). Determine
   whether the rim sliver is (a) an unroll-map precision fold — fix the map /
   split direction — or (b) a refinement inserting a point against the local
   winding. Red test: R0034 as an `#[ignore]` kernel-v2 tessellation unit that
   asserts no folded triangle. This stays entirely inside the sub-project.
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

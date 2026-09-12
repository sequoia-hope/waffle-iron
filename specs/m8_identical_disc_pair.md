# Spec: M8 identical disc pair — the flush same-radius cylinder stack (C0044)

**Status: LANDED always-on 2026-09-12** (`stage0::disc_pair::build_identical_discs`,
dispatched from `stage0_preprocess` as `DiscPair::Identical`). Vehicle: C0044
(`extrude(circle r=1, h=1)` + `extrude(circle r=1, h=1)` on its top cap +
`extrude(circle r=0.3, cut)` through both — the M8-annular "tube, χ=0" case),
ERROR at op 2 (`reassembled output would be non-2-manifold`, 0.2 s) since the
corpus was authored.

## 1. The defect, measured (2026-09-12, `YANG_STAGE0_DUMP_DIR` + `NONMANIFOLD_SITE_PROBE`)

- Op 2 is the union of two r = 1, h = 1 cylinders stacked cap-to-cap. Stage 0
  sees the pair (`pair_plane: face_a=1 face_b=0 opposite=true n=(0,0,1) d=-1
  band=1e-7`, `cyl_pairs: 1` for the shared lateral cylinder) and emits
  NOTHING: `000_union_a.obj` ≡ `000_union_a_pre.obj`, same for B.
- The two caps are the SAME circle (centre (0,0,1), r = 1, N = 13 each), yet
  their Stage-1 rims differ by ulps — Stage 1 samples each cap's rim with the
  cap's own in-plane basis and seam phase (`θ = φ₀ + k·2π/N`, seam at
  (0,−1,·) so φ₀ = −π/2 is inexact): A's top rim `0.4647231720437686` vs B's
  bottom rim `0.46472317204376845` (1.5e-16), `0.8229838658936564` vs
  `0.8229838658936571`. A's own bottom rim carries B's bits, not its top's.
- `build_disc_disc_containment` classified the pair by STRICT containment
  (`strictly_inside_convex`, exact) — false both ways for identical rims —
  then `convex_rings_overlap` (true: some ulp-inside vertices) → the "crossing
  rims / lens" branch → `DiscPair::Empty`, whose comment delegated the lens
  to cherchi's coplanar arrangement. The arrangement received two
  ulp-different 13-gons.
- Site: `i6-edge-overuse (14,15) fwd=1 rev=0` — output triangle 13 =
  `[(0,0,1), (0,−1,1), (0.4647,−0.8855,1)]` is A's INPUT triangle 13 (face
  1's first fan triangle), single-labelled `(A, 13)`, whole and unsplit; its
  rim edge (15,16) carries A's lateral tri 15 and B's lateral tri 56 (`fwd=2
  rev=1`); the rim vertex is the weld cluster `{14, 15, 18}` (A's sample, B's
  ulp-twin, a crossing mint). Every other cap triangle was dropped as the
  §4.5.5 sheet rule intends; this one survived the ulp-lens.

Harness reproduction (`tests/m8_disc_coplanar.rs`): the historical
`z_cylinder` fixture seams at +x, where every sample angle is an exact
multiple of 2π/N and the two caps' samples agree bit-for-bit — the pair
passed. Seaming the fixture at −y like the corpus (`z_cylinder_seam`)
reproduces it: union `NonManifoldOutput`, subtract `NonManifoldInput`.

## 2. The paper's rule — §4.5.5

"Identical meshes are generated for both models in this part" (the overlap)
and "the common part and the other two parts share identical sampling points
on their boundaries" (Fig. 16 caption; `refs/text/yang2025_hybrid_boolean.txt:717-731`).
For two identical discs the overlap is the WHOLE disc, its boundary is BOTH
rims, and the rims are shared with the laterals — so the identical mesh is
one fan over one ring, and that ring must be what both laterals sample.
The A-only and B-only parts are empty. Strict containment cannot express
this (there is no annulus), and a "lens" it is not.

## 3. The rule as built

`build_identical_discs(a, b, face_a, face_b, vb, ring_a, center_a, ring_b,
center_b, opposite) -> Option<DiscPair>`, tried before the strict
containment tests:

1. **Identity of the CIRCLES within rounding.** `|c_a − c_b| ≤ band` and
   `|r_a − r_b| ≤ band` with `band = TAU_WORK·(1 + scale)` (the KV10
   rounding band; scale = max(|centre coords|, r)) — never `TAU_MODEL`
   (the R0053 lesson: an absolute model band fuses real micro geometry).
   Otherwise `None` and the pair takes the historical paths.
2. **Ring merge about A's centre.** Both rings lie on the one circle within
   rounding, so the angularly nearest A sample is the 3D nearest. Each B
   sample is either
   - a rounding twin of an A sample (3D distance ≤ `band`) — **FUSED**, A's
     bits represent it (the rim build's slot merge will take those bits on
     B's edge; on A's edge they are bit-identical no-ops), or
   - a distinct sample — **INSERTED** into the merged ring (and thus into
     A's rim as an extra point; on B's edge it is B's own uniform sample,
     a bit-identical no-op).
   A B sample beyond `band` but inside the rim build's absolute
   `TAU_MODEL` merge ceiling would be merged AWAY by that build while the
   shared fan still carried it (a T-junction): refused loudly as
   `DiscPair::Wall("disc-identical-subres")` → the typed
   `CoplanarFacesUnsupported` residue. Under circle identity it cannot
   occur (samples are rounding twins or a full chord apart).
3. **Emission.** `tris_a = tris_b =` the fan `[c_a, m_i, m_{i+1}]` over the
   merged ring (frame-CCW = A's outward normal; B swapped iff `opposite`);
   `rim_overrides_a[e_a] = rim_overrides_b[e_b] =` the merged ring — the
   Stage-1 build threads them into every face using either circle edge
   (cap AND lateral), via the task-#143 slot merge for twins and the
   azimuth-merge strip for insertions.
   **Opposite rims.** A cylinder / torus lateral pairs its two rims 1:1
   (the azimuth-merge strip refuses `13 vs 26` — measured on the
   mismatched-seam fixture before this step), so every INSERTED sample
   (B-only on A's rim, A-only on B's) also gets its exact image on that
   lateral's opposite rim through the crossing path's `opposite_rim_image`
   (`lateral_for_cap`; cylinder AXIAL / torus POLOIDAL projection),
   registered as a rim override there (`opp_a` / `opp_b`). Refused loudly
   when the cap has no such lateral (`lateral_for_cap`'s tags), an image is
   on-axis, or two insertions collapse to one image — never a silent count
   deficit. No insertion ⇒ nothing propagates (C0044: fusion only).
4. **Seam weld.** If B's rim seam vertex (the circle edge's B-Rep vertex)
   fused with an A sample, `vb[seam_b] := that A sample` — the rim build
   refuses a seam slot whose bits differ from the B-Rep vertex ("B-Rep
   vertices are authoritative"), and the snap phase's cross-weld only
   catches bit-equal in-frame keys. A moves by nothing; B's vertex moves
   within the rounding band.

Downstream: cherchi prep welds the two identical fans into one multi-label
sheet; the `boolean()` §4.5.5 sheet rule resolves it by the pair's
`opposite` flag (union → drop, subtract → keep as A's cap, intersect →
drop); the two laterals share the merged ring bit-exactly and stitch.

## 4. Measured

- C0044 solo (release): ERROR 0.2 s → **SUPPORTED_CORRECT 0.9 s**, all three
  ops, every oracle (the tube's `euler_target 0`, volume 5.7177 within 5 %).
- Harness pins (`tests/m8_disc_coplanar.rs`, RED before / GREEN after,
  the 13 historical disc tests byte-identical via the `z_cylinder` wrapper):
  `flush_identical_cylinder_stack_union_succeeds` (−y seams: fusion + seam
  weld; no membrane on z = 1, z ∈ [0,2], volume within the 13-gon band of
  2π), `flush_identical_cylinder_stack_subtract_keeps_the_body` (the sheet
  kept as the cap), `flush_identical_stack_with_mismatched_seams_unions`
  (+x vs −y seams: zero coincidences, the INSERT half on both laterals
  with the opposite-rim images — RED as `azimuth-merge rims have
  mismatched / too-few samples (13 vs 26)` without them —, union and
  subtract).
- kernel-v2 `tests/m8_identical_disc_stack.rs::flush_same_radius_cylinder_stack_then_bore`
  (the real extrude path, C0044's three ops); test-harness
  `smoke_union_flush_same_radius_cylinder_stack` + the C0044 pin.
- Corpus (release, 8 jobs, 600 s; wall 742.3 s at load ≈ 4; F0085
  327.6 s, R0044 291.0 s): **286C / 0W / 19E / 4EE / 0T, 3
  UNSUPPORTED(coplanar-boolean)** — exactly ONE category move (C0044
  ERROR → SUPPORTED_CORRECT), ZERO detail moves (per-id category + detail
  diff against the committed `results.json`). Ledger: `docs/yang_tail_triage.md`
  "2026-09-12 (late) — C0044 CONVERTED".

## 5. Non-goals / known gaps (unchanged behaviour)

- Two discs whose circles differ by MORE than rounding but less than a
  chord (e.g. r and r + 1e-9) are not identical; strict containment
  classifies them and builds a femto annulus — the pre-existing
  disc∩disc containment behaviour, not this increment's.
- N-ary plane groups carrying a disc face keep the typed residue
  (`stage0::nary` scope), as before.

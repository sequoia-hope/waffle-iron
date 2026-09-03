# The torus BAND's side — a boolean-output torus patch spanning more than 180° was tessellated as its complement

Status: LANDED 2026-09-03 (the exact-membership oracle's class-A finding,
`docs/audits/exact_membership_sweep_2026_09_03.md`). Anchor and fix in
yang-rs `stage1_tessellate/patch_tessellators.rs::tessellate_torus_patch`
(the two-meridian-wrapping-loop "band" case), threaded through both
callers — kernel-v2's render path (`tessellate/surfaces/torus.rs`) and
yang-rs's own Stage-1 band tessellation (`tessellate_torus_band`).

## 1. The defect, as measured

R0091 (`revolve(circle, 219.43°) + extrude(box) + extrude(circle, cut)`):
the revolve is a separate body nothing else touches. Isolated, kernel-v2
builds it exactly (3.960e-12 against Pappus `π r² R θ` = 3.966e-12; the
sweep's 111 angular stations run 0° → 219.4°). After the union with the
disjoint box it is still intact (4.344e-12 for the two bodies against the
exact 4.35e-12; that union takes the arena-level disjoint-sum path, so the
modeling face and its structured seam-arc loop survive). After the CUT —
a real yang-rs boolean, from which the face comes back with polyline rims
and is rendered through the patch path — the body occupies the angular
stations 219.4° → 360°: the COMPLEMENTARY 140.6° wedge, volume 2.537e-12
against that wedge's Pappus 2.541e-12. The runner passed the case as
SUPPORTED_CORRECT: the wedge is watertight, genus 0, and the composition
oracle is skipped on cut chains.

R0045 (`revolve(circle, 289.7°) ∖ revolve(rect)`) and R0096
(`revolve(circle, 281.1°) ∖ revolve(circle)`) are the same defect: their
outputs occupy 280° → 360° and the tail of 281° → 360°. The audit first
filed them as a separate subtract class; the angular stations say
otherwise.

## 2. The anchor

`tessellate_torus_patch` inverts every loop into the `(u = meridian,
v = longitude)` chart with `atan2`, so each rim of a band lands at a
principal longitude in (−π, π]. Two rims bound TWO candidate bands (the
two arcs between them); `band_seam_bridge` laid the ribbon between the
two longitudes as they came, i.e. on the SHORTER arc. That is the band
when it spans less than 180°, its complement when it spans more, and a
coin toss at exactly 180°. Nothing in the input distinguishes the two
except the loops' orientation, which the consumer ignored.

## 3. The rule

The loops carry the answer. A B-Rep face's loops wind material-CCW about
the face's OUTWARD normal (yang-rs `BRepFace::outer_loop`: "CCW viewed from
outside along the face normal"; kernel-v2 `validate/faces.rs`, the
unrolled-winding rule for cylinder bands). In the consumer's chart, with
`(e1, e2, axis)` right-handed (`ortho_basis`: `e2 = axis × e1`),

    P(u, v) = c + (R + r cos u)(cos v e1 + sin v e2) + r sin u axis
    ∂P/∂u × ∂P/∂v = −(R + r cos u) · r · n̂_out

so the chart is NEGATIVELY oriented with respect to the torus's outward
normal: a material-CCW loop runs CW in `(u, v)`, and the material sits on
the traversal's RIGHT. For the rim that wraps the meridian `+1` (`pc`, u
increasing) the right-hand side is DECREASING v; for the `−1` rim (`mc`)
it is increasing v. Hence

    band = (v_pc − Δ, v_pc),  Δ = (v_pc − v_mc) mod 2π ∈ (0, 2π)

for an outward-facing face, and the mirror image — `(v_pc, v_pc + Δ')`,
`Δ' = (v_mc − v_pc) mod 2π` — for a `reversed` face (a bore: outward =
−n̂_torus). The consumer now takes `reversed` (a new parameter; both callers
pass the face's flag) and shifts `mc`'s longitudes by WHOLE periods onto
that side before the seam bridge — a band already on its side is untouched
bit-for-bit, so every sub-180° band the corpus renders today is
byte-identical. Coincident rim longitudes (a full-turn "band") decline
loudly as before.

## 4. Proofs

- yang-rs `torus_band_beyond_180_degrees_lies_on_the_oriented_side`: a
  220° band whose rims wind per the convention tessellates to the area
  `r·220°·2πR` with every triangle centroid inside [0°, 220°]; the same
  loops on a `reversed` face give the 140° complement. The three existing
  band tests and the Stage-1 poloidal band test had their rims wound the
  other way round (correct only under the shorter-arc behaviour) and are
  re-wound to the convention, with the reason recorded at each.
- R0091 end to end (`s453_r0053_output_obj`, then the angular stations):
  the revolve body occupies 0° → 219.4° again; the three-op result's
  volume moves from 1.957e-12 to 4.219e-12 against the exact chain's
  4.262e-12 (the remaining gap is the cut box's corner pillars at the
  lattice's resolution, not the sausage).
- The two-proof corpus (release, 8 jobs, 360 s): recorded in the roadmap
  entry — every sub-180° band case (C0066, R0087, R0059, R0074, R0062,
  R0028) byte-identical; the > 180° cases move only in the direction the
  exact volume certifies.

## 5. What it does not touch

Bounded (non-wrapping) torus patches — their single loop fixes the region.
The class-C finding of the same audit (a cut whose tool contains the whole
body removes nothing; R0034, R0007, R0027, R0088) is a different anchor in
yang-rs's classification of an arrangement without intersection curves,
and is open.

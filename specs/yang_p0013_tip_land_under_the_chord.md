# Spec: P0013 — a 9.28 µm land under TWO chord bands

Status: **anchored and half-landed 2026-10-03.** P1 (the exact cylinder
clearance) and P4 (the render loop-conformity density) are IN; P3 (flipping
the §4.3.3 derived-density guard ON by default) needs a full-corpus proof and
is the open step. P0013 is **SUPPORTED_CORRECT under `YANG_433_GUARD=1`** and
still ERROR on the default path.

Owner sections: Yang 2025 §4.3.3 (Case-IV rule-out,
`refs/text/yang2025_hybrid_boolean.txt:518-537`) and §4.5.2 (local
refinement, `:659-670`). Siblings:
`specs/yang_433_case_iv_corner_phantom.md` (the Case-IV certificate and its
inc-1 density guard) and `specs/kv2_cdt_triangulation_core.md` (the render
CDT).

---

## 1. The case

`app/tests/cases/assay/P0013.{waffle,meta.json}` — prospector seed 2 index 29,
minimised by TRUNCATION ONLY (4 ops → 2, `truncate to 2`; no geometry
rounding, so the minimum is a literal prefix of
`target/prospect/seed-2/candidates/X00000002-00029.*` and the anchor below
holds for both).

```
circle:boss  r = 2.1627665069443046e-2, z ∈ [0, 3.833917548816847e-2]
star7:cut    centre (7.110349561829165e-3, 8.563608711600106e-3),
             r_in 7.497105167552266e-3, r_out 1.0782731100676584e-2,
             sketch z = 2.103074485312741e-2, depth 5.197359847765712e-2
```

```
boolean_subtract failed: TessellationFailed {
    face: FaceId(19),
    reason: "ring rejected by CDT (degenerate/self-intersecting)" }
```

## 2. The anchor (measured 2026-10-03)

**The exact star is ENTIRELY INSIDE the exact cylinder.** All seven tips
clear the boss wall; the closest, the 7th, clears it by

```
tip T = (1.6307152271934494e-2, 1.4192650361907672e-2)   |T| = 2.1618384e-2
radius                                                   r   = 2.1627665069443046e-2
LAND  r − |T|                                                = 9.280774694829519e-6
```

So the correct answer is the cylinder with a star-shaped through-hole and a
9.28 µm land at that tip — **no intersection curve at the tip at all.** Two
independent chord bands each destroy it.

### 2a. The Stage-1 band — a Yang §4.3.3 Case IV

`YANG_FACE_CENSUS=1`: the boss operand is **52 triangles over 3 faces**
= `4N − 4` ⇒ **N = 14**, sagitta `r(1 − cos(π/14)) = 5.4220e-4` — **58×
the land.** The 14-gon therefore has the tip POKING THROUGH the boss, and
Stage 2 mints the crossing. This is the paper's Case IV verbatim: "the
meshes detect intersections that do not exist between the surfaces".

The minted wedge is a vertical sliver with four corners.
`YANG_433_PHANTOM=1` judges exactly half of them:

```
v=69 p=(0.016311,0.014203,0.021031) claim=B-edge x Cylinder e70:
      t=-0.002057(out) ... t=7.280482(out) -> PHANTOM-CLAIM clearance=Some(0.0)
v=71 p=(0.016321,0.014191,0.021031) claim=B-edge x Cylinder e66:
      t=-4.537546(out) ... t=1.002704(out) -> PHANTOM-CLAIM clearance=Some(0.0)
v=30 p=(0.016311,0.014203,0.000000) claim=A-edge x Plane e0:curved("Circle")
                                          -> CURVED-EDGE
v=37 p=(0.016321,0.014191,0.000000) claim=A-edge x Plane e0:curved("Circle")
                                          -> CURVED-EDGE
SUMMARY claims=17 valid=13 phantom=2 curved_edge=2 ...
```

Two findings, both one layer short of the certificate:

1. **`clearance=Some(0.0)` on the two refuted corners.** The clearance is a
   65-sample Lipschitz lower bound, `min_d − len/(2·64)`. The land is
   9.2807e-6 and the tip edge is 5.1769e-3 long, so the slack `len/128 =
   4.045e-5` SWAMPS it and the bound floors at 0 — the §4.5.2
   under-resolution demand is then dropped (`if g <= 0.0 { continue }`) and
   the ladder runs its blind default rungs (measured 52 → 72 → 100
   triangles, i.e. N = 14 → 19 → 26, never near the N ≥ 108 the land needs).
   The same slack, in the same formula, is why the §4.3.3 **density guard**
   under-derives: `YANG_433_GUARD=1` printed `req=Some(34)` — a demand from
   some OTHER cluster entirely, because the tip's own cluster derived
   nothing.
2. **The loop-level certificate cannot see this phantom at all.**
   `certify_phantom_loops` requires a CLOSED component of refuted corners.
   Here the wedge's two z = 0 corners sit on the boss's bottom cap circle and
   continue into the star outline's REAL intersection curve, so the component
   is open and no certificate is produced (no `[s433-ruleout]` line fires).
   Forcing `CURVED-EDGE → Phantom` as a diagnostic does **not** change that:
   the refuted set still touches genuine junctions. P0013 is therefore NOT
   the R0100/P0002 shape despite sharing the recipe family and the error
   text — it is **a phantom BUMP on a real intersection curve**, a shape the
   closed-loop certificate is structurally blind to.

Consequence: the output B-Rep's bottom cap (FaceId(19)) carries ONE merged
88-point loop (75 circle + 13 star vertices — the 14th star vertex, the tip,
is gone, consumed by the two minted crossings) instead of circle-outer +
star-hole; and the relocation lands the two minted vertices 1.57e-5 apart in
INVERTED angular order on the circle (p0 at 0.71569586 rad, p14 at
0.71642190, the arc attaching them reversed), so the merged ring is a
bow-tie. `KV2_RING_REJECT_PROBE=1` prints exactly that: four proper
self-crossings, edges 0/13, 0/14, 13/87, 14/87.

### 2b. The render band — the thin-land CDT class

Refine the operands enough (`YANG_452_ROUNDS=128` / `256`, both ADOPTED with
`unpaired=0 improper=0 fires=0`) and the output B-Rep becomes CORRECT —
`outer_len=71 holes=1`, the star a proper hole. **The CDT still refuses it.**

Because the render tessellation samples the circular boundary as the
INSCRIBED 71-gon (`RENDER_CHORD_TOLERANCE_REL = 1e-3`), whose sagitta is
`2.1169e-5` — again over the 9.28 µm land. The chord polygon cuts INSIDE the
hole, the two constraint rings cross, and the exact CDT is right to refuse.
Measured: the reject survives a d_ε/256 operand mesh, and `DIAG_NSEG=128`
(a throwaway density override) alone does not convert either — each band has
to be fixed on its own.

### 2c. The two-sided proof

| Stage-1 band | render band | verdict |
|---|---|---|
| natural (N=14) | 71 | ERROR (CDT reject, FaceId(19)) |
| natural (N=14) | 128 | ERROR (the merged bow-tie ring is wrong at ANY render density) |
| d_eps/128 | 71 | ERROR (the correct hole still pokes through the 71-gon) |
| d_eps/128 | 128 | **SUPPORTED_CORRECT (1.5 s), all checks passed** |

## 3. Why no band is tuned anywhere here

Both halves replace a WRONG QUANTITY with the right one; neither widens an
acceptance window:

* the Stage-1 half computes the segment-to-cylinder distance exactly instead
  of bounding it by quadrature;
* the render half derives `N` from the clearance the face actually has
  instead of a fixed global tolerance.

Both fail CLOSED: a clearance that is not strictly positive, or a demand
above the 4096 cap, derives NOTHING and keeps the loud reject.

## 4. The increments

### P1 — the exact cylinder clearance (LANDED)

`boolean::rim_junction::segment_cylinder_clearance`: the radial distance
`ρ(t) = ‖(p(t) − axis_point)⊥‖` is the norm of an affine function of `t`,
hence CONVEX, so its maximum on `[0,1]` is at an endpoint. With both
endpoints strictly inside, `ρ(t) ≤ max(ρ(0), ρ(1)) < radius` throughout and

```
min |ρ(t) − radius|  =  radius − max(ρ(0), ρ(1))
 t
```

— two evaluations, no quadrature, exact. Wired into
`segment_face_graze_n`'s clearance; every non-cylinder surface keeps the
sampled bound byte-identically (the R0100 pins are all CONE and are
unmoved). The guard then derives **N = 152** on P0013's tip cluster
(`sag(152) = 4.62e-6 < 9.28e-6`), measured: `[edge-graze-guard]
req=Some(152) natural=(13,…) gated=Some(152)`.

Pins (`crates/yang-rs/src/boolean/rim_junction.rs`, `edge_graze_tests`):
`cylinder_clearance_is_exact_not_sampled` (the measured land to five figures,
plus the premise that the Lipschitz bound goes NEGATIVE here),
`cylinder_clearance_declines_outside_and_non_cylinder`,
`p0013_tip_edge_derives_the_density_the_land_demands` (= 152). Mutation
check: with the sampled bound restored the last one fails with "a
non-piercing tip edge inside the band must derive a demand".

### P2 — a planar-face loop-simplicity postcondition (NOT LANDED)

The merged bow-tie cap ring is a B-Rep validity violation — a planar face's
boundary loop must be SIMPLE — and nothing in yang-rs checks it. It is
emitted as `Ok`, `output_improper_count` reads 0, and so
`natural_broken` is FALSE and no retry path engages; the failure is only
noticed one crate later, misnamed, by kernel-v2's tessellator. An exact
`orient2d` segment-crossing test over each planar face's projected loops
would name it where it is produced and give the detect-then-refine wrapper
a gate it currently lacks. Deferred: it re-classifies failures corpus-wide
and needs the full-corpus proof.

### P3 — flip the §4.3.3 density guard ON (NOT LANDED, the open step)

`edge_graze_min_rim_segments` is gated behind `YANG_433_GUARD=1|on` and is
applied EAGERLY on every pass of `boolean_once`, so flipping it changes
pass 1 for every case where it fires. Its 2026-08-27 broad-form flip was
REFUSED on corpus evidence (spec `yang_433_case_iv_corner_phantom.md` §7:
52 cases boosted, eight CORRECT regressed, R0011 turned silently WRONG);
the narrowed corner-cluster form in the tree has never been measured with a
CORRECT clearance behind it. **P1 changes what it derives, so the refused
measurement does not transfer in either direction** — it has to be re-run:
`ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=900 … full_corpus_categorized`, gate
off (byte-identical) then gate on.

A safer shape to measure alongside it, if the eager flip regresses again:
apply the guard on the REFINEMENT pass only, the `#195 inc-4` pattern
`rim_plane_graze_min_segments` already uses (pass 1 byte-identical, only an
already-broken op pays) — which needs P2 to make P0013's break detectable.

### P4 — the render loop-conformity density (LANDED)

`kernel_v2::tessellate::loop_conformity_segment_count` +
`loop_conformity_n_for`: for every planar face with inner loops and a
circular boundary edge of radius `r`, bound each inner loop's radial reach
about that circle's centre over EXACT geometry (its vertices, and for a
circular inner edge `|c_in − c| + r_in` — an upper bound, so the derivation
is conservative), take `clearance = r − reach`, and demand the smallest `N`
whose sagitta clears it:

```
N = ⌊π / arccos(1 − clearance/r)⌋ + 1
```

Measured on P0013: `clearance=9.280774694829519e-6 needed N=108`
(`KV2_LOOP_CONFORMITY_PROBE=1`).

Three deliberate choices:

* **global, not per-face** — the render mesh's watertightness depends on
  adjacent faces sampling a shared rim at the SAME `N`; a per-face density
  would tear the mesh at that rim.
* **self-limiting** — `None` unless some inner loop reaches within one
  sagitta of its own outer circle, so the canonical `N` stands everywhere
  else.
* **fail-closed** — `clearance ≤ 0` (a loop AT or ACROSS the circle: an
  invalid face, not an under-sampled one) and a demand above
  `LOOP_CONFORMITY_MAX_SEGMENTS = 4096` (true near-tangency) derive nothing
  and keep the loud reject.

Scope: planar faces. The curved-face holed variants (cylinder/cone/torus
patches) have their own routines and no measured customer yet.

Pins: `crates/kernel-v2/src/tessellate/loop_conformity_tests.rs` (six —
the measured demand, minimality of `N`, the premise that N = 71 does NOT
clear the land, non-positive clearance, a roomy clearance, the
near-tangency cap) and the end-to-end
`crates/test-harness/tests/p0013_tip_land_conformity.rs`. Mutation check:
with the derivation unwired the end-to-end pin fails with the verbatim
`TessellationFailed { face: FaceId(19), … "ring rejected by CDT …" }`.

## 5. What is NOT claimed

* **P0013 is not converted on the default path.** It needs P3.
* **No full-corpus measurement was taken** (another assay was running on the
  box). The evidence base for the landed halves is: the two crate suites,
  the new pins with their mutation checks, and the single-case re-runs
  recorded in the ledger row.
* The §4.3.3 closed-loop certificate's blindness to a phantom BUMP on a real
  curve (§2a finding 2) is recorded, not fixed. P3 side-steps it a priori;
  a downstream rule-out for it would need the §4.4.1 mesh-update candidate
  (b) of `yang_433_case_iv_corner_phantom.md` §3.

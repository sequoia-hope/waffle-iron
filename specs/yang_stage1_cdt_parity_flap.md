# #179 — Stage-1 planar CDT parity flap (finish the F0047 flood-fill migration)

Task #179, found while clearing the #146 P3a increment-3 gate (F0084
gate-ON regression, parent spec `yang_146_conformal_junction_sampling.md`
§4). Probe session 2026-07-18.

## 1. Measured problem

`tessellate_planar_cdt_face` (`stage1_tessellate.rs`, the all-segment
non-convex/holed planar face path) is the LAST production caller of
`cherchi_rs::cdt_polygon_with_holes` — the variant whose interior
classification is **f64 centroid parity** (even-odd ray test per
triangle centroid). On a boundary with a near-collinear triple, a
hair-sliver triangle's centroid sits within ~1e-16 of the boundary and
the parity test coin-flips:

- **keep an EXTERIOR sliver** → an extra (near-)zero-area "flap"
  triangle on top of a complete triangulation → the face's directed
  edges go asymmetric (fwd=1/rev=0 open + fwd=1/rev=2 over-used) → the
  operand mesh handed to the Cherchi arrangement is NON-2-MANIFOLD —
  an axiom-A1 violation minted at Stage 1;
- **drop an INTERIOR sliver** → the F0047 "parity slitting" class,
  already fixed for the curved-CDT path and kernel-v2 render by
  `cdt_polygon_with_holes_floodfill` (spec `kv2_cdt_triangulation_core`
  §6a/§6b).

Measured lead instance (F0084, fresh octagon-prism extrude, operand B of
its later auto-union): the tilted cap octagon has vertex 4 on the chord
between vertices 3 and 7 (|cross| ≈ 1.3e-16 of edge-length scale; the
profile notch 4→5→6→7 returns to the chord line). The cap emits 7
triangles instead of 6 — the flap `[3,7,4]` — and the prism mesh carries
edge (3,4) fwd=1/rev=2 + edge (3,7) fwd=1/rev=0. Captured bit-exact as
`tests_unit/stage1_cdt_flap.rs::f0084_octagon_prism` (two red fixtures).

Corpus reality (probe `NONMANIFOLD_SITE_PROBE` input-scan arm,
`boolean.rs`): flap-contaminated operand meshes occur in PRODUCTION
TODAY, gate-OFF and gate-ON — F0084 alone shows ~6 defective operands
across its op chain in BOTH gate states, byte-identical meshes. The
pipeline usually survives (the zero-area flap's directed-edge imbalance
dissolves in the arrangement/weld or stays outside the kept set);
F0084 gate-ON re-rolls the local triangulation so the imbalance lands in
the kept set and STOPs loudly at `s4-halfedge-pairing` (fwd=1 rev=2).
That STOP — the P3a "edge-level shadow" — is therefore a SYMPTOM of this
Stage-1 defect, not a weld-site collapse: the previous session's
"needs an edge-level wedge dedup at reassembly" framing is CORRECTED
(no such dedup is needed for F0084; the input mesh is simply invalid).

Junction sampling AMPLIFIES the class (every inserted pierce point is a
new collinear boundary triple — inserted mid-edge on a straight edge),
which is why the flap got loud gate-ON first.

## 2. Fix (structural, no tolerance)

Switch `tessellate_planar_cdt_face`'s triangulation call from
`cdt_polygon_with_holes` (parity) to
`cdt_polygon_with_holes_floodfill` (topological hull flood-fill) — the
same migration the curved-CDT path (`tessellate_planar_curved_cdt_face`)
and kernel-v2 render tessellation already completed for F0047.

Why this kills the flap: flood-fill classifies exterior faces as those
reachable from the convex hull crossing only NON-constraint edges. The
flap sliver's only constraint edge is the boundary segment (3,4); its
chord edges (3,7)/(4,7) are non-constraints spanning the notch mouth, so
the sliver is flooded from the hull and dropped — by graph topology, no
coordinate test, no band. Genuine interior slivers stay kept (the F0047
direction), preserving watertightness. Both coin-flip outcomes become
deterministic and correct.

Behavior deltas accepted by this spec:

- **Shared-vertex welding** (§6b M3b): the flood-fill variant welds
  bit-coincident loop vertices to one CDT vertex (keyhole/pinch faces)
  where the parity variant errored `DuplicateVertex`. Stage-1 planar
  faces gain the same keyhole tolerance kernel-v2 render already has; a
  degenerate consecutive-duplicate constraint still fails loudly.
- Triangulation deltas on already-misclassified faces ONLY: for every
  well-classified face the CDT is identical (same spade triangulation,
  same canonicalization) and the classifier agrees on every triangle
  whose centroid is not on a knife-edge.

`cdt_polygon_with_holes` (parity) remains in cherchi-rs for its test
oracles; no production caller remains after this change (enforced by a
grep in §4 — future callers must justify parity explicitly).

## 3. Non-goals

- Hole-loop classification inside the flood-fill variant stays per-hole
  centroid parity (upstream contract; a collinear HOLE run is a
  separate, unobserved class — add against a measured instance).
- No change to the fan path, the curved CDT, keep-interior CDT, or any
  Stage-4 site.
- No input-mesh manifoldness gate in `boolean()` this increment: the
  probe scan stays a probe. (A production A1 gate is worth weighing
  AFTER the corpus is flap-free, so it lands as a no-op ratchet, not a
  mass regression.)

## 4. Oracles & measurement

- Red→green: `stage1_cdt_flap.rs` both fixtures (2-manifold + no
  degenerate tris on the F0084 octagon prism).
- yang-rs lib suite + rewrite tier green.
- Full release assay gate-OFF vs committed baseline (250C/0W/55E ± the
  F0090 timeout flake): **0 WRONG** (hard abort otherwise), no C→E
  regression; E→C conversions are wins verified per-case.
- Full release assay gate-ON (`YANG_JUNCTION_SAMPLING_ENABLE=1`):
  expected F0084 C(baseline)→? — measured and recorded in the parent
  P3a ledger; the gate-ON regression set {F0084} should shrink to {}.
- Grep-lint: `grep -rn "cdt_polygon_with_holes(" crates/yang-rs/src/
  crates/kernel-v2/src/` returns no production call site.

## 5. Measured outcome (2026-07-18, SHIPPED)

- Red→green: both `stage1_cdt_flap` fixtures pass post-switch (the cap
  emits exactly 6 triangles; closed conformal 2-manifold). yang-rs lib
  382 green; rewrite tier green.
- F0084 single-case: gate-OFF CORRECT (unchanged) with the
  `i6-input-overuse` probe firing ZERO times across the whole op chain
  (production operands flap-free); gate-ON **SUPPORTED_CORRECT** — the
  last #146 P3a gate-ON regression fixed at the root. (Gate-ON the
  probe still fires on a DIFFERENT class — near-dup insertion
  conformality breaks, ~0.003-scale T-junction pairs in rebuilt
  operands — recorded in the parent spec §4 as the remaining inc-3
  blocker; it no longer fails any case.)
- Full release assay gate-OFF: **251C/0W/55E/2T** — per-case identical
  to the committed 250C baseline except (a) F0090 TIMEOUT→CORRECT (the
  known flake) and (b) F0082's Extrude-7 non-2-manifold auto-union
  failure GONE (this fix); F0082 stays ERROR at a later op
  (`TessellationFailed FaceId(3716): ring rejected by CDT` — loud,
  next defect layer). 0 WRONG — ratchet holds.
- Full release assay gate-ON: **251C/0W/55E/2T, category-identical
  per-case to gate-OFF** — the P3a gate-ON regression set shrinks
  {F0084} → **{}** (F0016 and F0084 both CORRECT gate-ON; F0082 fails
  identically in both states). 0 WRONG.
- Grep-lint: `cdt_polygon_with_holes(` has NO remaining caller in
  yang-rs or kernel-v2 (production or test); the parity variant
  survives only inside cherchi-rs (its own unit oracles).

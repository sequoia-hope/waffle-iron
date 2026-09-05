# M5 — Procedural Surface-Pair Curve (general degree-4 SSI, Option B)

**Status**: Implementation spec (governance gate CLEARED by user 2026-07-08 —
Option B; Constitution P8 degree-4 clarification merged at 35d2332f)
**References**: [#24] Yang et al. 2025 §4.1.2, §4.3, §4.4.1 (d_p = 1e-7);
[#1] Patrikalakis Ch.5 (degree-4 nature of quadric-pair intersections)
**Governance**: P8 (degree-4 procedural-curve clarification), A15.1
**Supersedes**: `specs/cyl_cyl_unequal_r_ssi.md`'s `Degree4CylCyl`
θ-parameterized representation (its branch table B1–B5 remains the case
analysis; the closed-form Levin-style variant is explicitly NOT built — the
paper is the spec and the paper carries refined point sequences, each point
Newton-projected onto BOTH surfaces).

---

## Goal

Represent general-position quadric-pair intersection curves (first producer:
unequal-radius / skew cylinder×cylinder) exactly and procedurally: the curve
is defined implicitly by its TWO analytic surfaces; every concrete point is
certified by Newton projection onto both surfaces (residual ≤ band). This
lifts the `AnalyticalSolutionNotAvailable` stop for general cyl×cyl
(ssi-rs lib.rs:1545) and gives kernel-v2 an output vocabulary for the
surviving quartic edge (the tunnel-mouth rim of C0052/C0053/C0054;
R0019/R0044 stops).

The curve arrives in kernel-v2 the same way circles/ellipses do: yang tags
per-mesh-edge output `BRepEdge`s with the shared analytic curve descriptor,
so a quartic loop is a chain of short surface-pair edges whose endpoints are
the certified point sequence (relocated by Stage 4). No point array is
carried on any curve variant — consecutive output vertices ARE the samples.

## Parameters (the new vocabulary, per crate)

| Crate | New variant | Fields |
|---|---|---|
| ssi-rs | `SsiCurve::SurfacePair` | `a: QuadricSurface, b: QuadricSurface` (Copy) |
| yang-rs | `Curve::SurfacePair` | `a: Surface, b: Surface` (yang's unsigned Copy surface descriptors) |
| kernel-v2 | `arena::Curve::SurfacePair` | `a: PairSurface, b: PairSurface` — Copy `PairSurface` enum in arena.rs (`#[non_exhaustive]`): `Cylinder { axis_point, axis_dir, radius }` (cyl×cyl) and `Cone { apex, axis_dir, half_angle }` (cone-pair producer, landed 2026-07-08) |

No `reversed`/cavity flags anywhere on the pair descriptors — the curve is a
point set; orientation comes from the half-edge traversal (origin → dest),
twins share IDENTICAL `a`, `b` (like `EllipseArc` twins share `major_axis`).

Ordering: `a`/`b` order is the ssi call's argument order, preserved verbatim
through yang → kernel-v2. Twin/pairing comparisons use exact equality on the
ordered pair (no canonicalization needed — both twins are minted from the
same descriptor).

## Branch table

### ssi-rs `cylinder_cylinder` (existing arms unchanged unless noted)

| # | Condition | Output |
|---|---|---|
| S1 | non-parallel, equal-R, coplanar axes | dual ellipses (existing, unchanged) |
| S2 | non-parallel, unequal-R (any coplanarity) | `vec![SurfacePair{a,b}]` (NEW — was ASNA) |
| S3 | non-parallel, equal-R, skew axes | `vec![SurfacePair{a,b}]` (NEW — was ASNA) |
| S4 | parallel arms (coincident/concentric/disjoint/tangent/secant) | existing, unchanged |
| S5 | degenerate inputs (E1) | `DegenerateInput` (existing, unchanged) |

ONE descriptor for the whole intersection set: branch/component separation is
topological (mesh connectivity of the point-sequence chains), not part of the
curve descriptor — per the paper, the curve is the zero set of both surfaces.
Disjoint-surface configurations return the descriptor too; Stage-3 membership
simply never matches it (no intersection mesh edges exist for that pair).

### yang-rs

| # | Site | Behavior |
|---|---|---|
| Y1 | `ssi_curve_to_curve` | `SsiCurve::SurfacePair` → `Curve::SurfacePair` (QuadricSurface → yang Surface, field-for-field; `Plane` operand → unreachable for the cyl×cyl producer, typed error if hit) |
| Y2 | `curve_contains_point` | max(residual to `a`, residual to `b`) ≤ tol |
| Y3 | Stage-3 tangent tie-break | tangent at p = normalize(n̂_a × n̂_b); parallel normals (tangency) → no tangent, candidate not tie-breakable (existing AmbiguousCurve stop stays loud) |
| Y4 | Stage-4 relocation | `relocate_onto_implicit_pair(p, a, b)` (EXISTING, tested — lib.rs:4552); `None` → existing loud `SsiRefinementFailed` path |
| Y5 | output emission | per-mesh-edge `BRepEdge` tagged `Curve::SurfacePair` exactly like circles/ellipses (`intersection_curves` map) |

### kernel-v2

| # | Site | Behavior |
|---|---|---|
| K1 | `classify_edge` (from_yang) | yang `Curve::SurfacePair` with both operands Cylinder → `EdgeKind::SurfacePair`; any non-Cylinder operand → `UnsupportedBooleanOutputCurve` (typed, until PairSurface grows) |
| K2 | `classify_edge`, `start == end` | `UnsupportedBooleanOutputCurve` ("closed surface-pair loop edge — no producer") |
| K3 | `classify_edge` endpoint check | each endpoint's residual to BOTH surfaces ≤ import band (1e-9 scaled, same shape as circle/ellipse bands) |
| K4 | `CurveKey` | new arm keyed on the ordered pair bits (distinct quartics on the same vertex pair pair separately) |
| K5 | `validate_solid` twin check | `curves_twin_consistent`: exact equality of `a`, `b` on both twins |
| K6 | `validate_solid` closed check | SurfacePair half-edge with origin == dest → `CurveTwinMismatch` (like Arc/EllipseArc) |
| K7 | `validate_solid` endpoint residual | per-point on-BOTH-surfaces residual ≤ validate band (the memory contract: validate = per-point residual) |
| K8 | `validate_solid` planar-face loop | SurfacePair edge on a `Plane` face → invalid (a transversal quadric-pair curve is never planar; degenerate configs produce conics upstream) |
| K9 | tessellate: edge samples | `surface_pair_interior_samples`: recursive chord midpoint → Newton projection onto both surfaces (Gauss-Newton, [#24] §4.3), split while sag > chord bound, depth-capped; non-convergence → typed error (loud, no chord fallback) |
| K10 | tessellate: cylinder patch boundary | SurfacePair boundary edges enter the unroll via their K9 samples (same role as `arc_interior_samples`) |
| K11 | re-entry `to_yang_brep` | ~~any SurfacePair edge → `UnsupportedCurvedBoolean { face }`~~ **inc-1 LANDED 2026-09-04** (section "K11 re-entry" below): a curved-lateral SurfacePair edge converts to ONE shared yang input `Curve::SurfacePair` (operands verbatim, endpoint-determined); yang Stage 1 builds its Newton-certified chain. **inc-2 LANDED 2026-09-05** (section "K11 inc-2" below): a chained cut whose plane CROSSES a pair chain lands the crossing on the exact `pair curve ∩ plane` junction. A SurfacePair on a PLANE loop stays typed (K8: never a valid solid) |

## Invariants

1. **On-both-surfaces**: every SurfacePair edge endpoint (and every render
   sample) satisfies `residual_a(p) ≤ band ∧ residual_b(p) ≤ band` where
   `residual` is the surface's implicit distance (|dist(p, axis) − r| for a
   cylinder) and band = 1e-9·(1 + max coordinate/radius magnitude) at import,
   matching the circle/ellipse import bands.
2. **Twin identity**: twins carry bit-identical `a`, `b`.
3. **Transversality at samples**: Newton projection succeeds only where
   surface normals are non-parallel; tangency points fail LOUD (upstream
   Stage-4 relocation already stops there — KV9-F1 class).
4. **No point arrays**: the certified sequence is the output vertex chain;
   `Curve` stays `Copy`.

## Oracles

| Oracle | Method |
|---|---|
| ssi S2/S3 descriptor | returned `SurfacePair` operands == inputs verbatim |
| membership | points constructed on both surfaces pass Y2 at tol; points off either surface by 10× tol fail |
| relocation | perturbed near-curve points relocate onto both surfaces (residuals ≤ 1e-13, existing `relocate_onto_implicit_pair` contract) |
| kernel validate | hand-built lens prism (two secant PARALLEL equal-R cylinders; tip edges are genuine SurfacePair lines — both residuals 0) passes `validate_solid`; mutated twin descriptor / off-surface endpoint / closed edge / planar-face placement each fail with the typed error |
| tessellation | K9 samples of a perpendicular unequal-R pair edge lie on both surfaces within band and satisfy the chord bound; lens prism tessellates with finite, NaN-free mesh |
| corpus (end-to-end) | C0052 (perp unequal-R), C0053 (45°), C0054 (skew) move ERROR → SUPPORTED_CORRECT with exact-volume oracles; R0019/R0044 stops lift; zero WRONG introduced |

## Failure modes

| Condition | Behavior |
|---|---|
| tangent pair (parallel normals at contact) | Stage-4 relocation `None` → existing loud `SsiRefinementFailed`; K9 sampling non-convergence → typed tessellation error |
| non-Cylinder operand reaching K1 | typed `UnsupportedBooleanOutputCurve` |
| closed single-edge quartic loop | K2/K6 typed rejection (no producer constructs them) |
| chained boolean on quartic-bounded body | K11 inc-1: RE-ENTERS Stage 1 (2026-09-04). K11 inc-2 (2026-09-05): a chained cut whose plane crosses a pair chain with a RULING × section-CIRCLE junction (plane ∥ one axis, ⟂ the other) lands the crossing exactly. A crossing whose two sections are OTHER in-plane conic pairs (ellipse × circle, ellipse × ellipse in ONE plane, ruling × ellipse) reaches the per-type junction arms, whose closed forms assume DISTINCT planes — unmeasured; the loud Stage-4 stops stand there |
| near-half ambiguity | N/A — no minor-arc derivation exists for SurfacePair (no normal to flip); traversal is endpoint-determined |

## Research basis

- [#24] Yang et al. 2025: §4.1.2 (intersection curves as refined point
  sequences), §4.3 (Newton projection onto both surfaces / local refinement),
  §4.4.1 (mesh updating with relocated = certified vertices), output
  tolerance d_p = 1e-7. The procedural surface-pair curve is the paper's
  representation; Levin pencil parameterization is deliberately avoided.
- [#1] Patrikalakis Ch.5: degree-4 algebraic nature of the general quadric
  pair (why no conic closed form exists here).
- Constitution P8 "Degree-4 procedural-curve clarification" (2026-07-08):
  a procedural curve whose defining surfaces are exact IS an analytical
  representation.

## Increments (P7)

1. **kernel-v2 vocabulary (gating)**: `PairSurface` + `Curve::SurfacePair`,
   K5–K11. Tests: hand-built lens-prism fixture + direct sampler tests.
   No producer yet — compile-time exhaustiveness sweeps every match site.
2. **ssi-rs**: S2/S3 arms return the descriptor (was ASNA).
3. **yang-rs**: Y1–Y5 plumbing (membership, tangent, relocation dispatch,
   emission).
4. **kernel-v2 from_yang**: K1–K4 acceptance; corpus measure (C0052/53/54,
   R0019/R0044) + assay gate.
5. **Cone-pair producer (landed 2026-07-08).** Extends the SurfacePair
   vocabulary from cyl×cyl to the cone-pair arms (cyl×cone, cone×cone):
   - ssi-rs: `cylinder_cone`/`cone_cone` NC (non-coaxial) arms return
     `SurfacePair` instead of ASNA (ssi8/ssi9 NC tests + adversary gate-boundary
     assertions flipped RED→GREEN; cylinder-first canonical order for cyl×cone,
     argument order preserved for the symmetric cone×cone).
   - yang-rs: `quadric_to_surface` Cone arm; Y2 membership + Y3 tangent
     generalized to Cone operands (`radial − |h|·tanα` residual; unit gradient
     `cosα·r̂ − sign(h)·sinα·â`). Y4 relocation already generic
     (`relocate_onto_implicit_pair`/`surface_value_and_normal` handle Cone).
   - kernel-v2: `PairSurface::Cone`; `pair_surface_residual_gradient` uses the
     TRUE signed-distance form `radial·cosα − |h|·sinα` (unit gradient ⇒ the
     shared Gauss-Newton step is exact, matching the cylinder's radial form);
     `pair_surface_scale` returns 0 for a cone (no constant radius; the point's
     own magnitude tracks local scale); `PairSurfaceKey`/`pair_surface_key` and
     `yang_surface_to_pair_surface` Cone arms.
   - Unit tests: ssi8/ssi9 flips, yang `m5_cone_pair_*` (Y1/Y2/Y3/Y4), kernel-v2
     `surface_pair_sampler_cone_cylinder`. All green; no regression on
     C0052/R0002. **Corpus note:** the R0008/R0003/R0019 cases previously
     tagged "cone-pair AmbiguousCurve class" were re-measured and are NOT
     cone-pair NC edges — R0003/R0008 are cone∩PLANE conic-selection walls
     (ellipse chord-band / apex-crossing generator-line pair) and R0019 is a
     Stage-4 `LocalRefinementRequired` (N2) wall — all orthogonal to the
     producer. The cone-pair NC producer ships as capability closure (per the
     roadmap "a faithful stage that lights up zero corpus cases still ships");
     a corpus case that exercises it end-to-end is not yet identified.

*Created: 2026-07-08*

## 2026-08-19 — the cone-pair walls were three PROSE-SHARED-RULE failures (R0032/R0044/R0053 + R0020)

The post-I5-2 Stage-4 STOP census (`yang_n2_stage4_cdt_mesh_updating.md`
§5c.13's `YANG_LRR_SITE` instrument) put R0032, R0044 and R0053 on ONE site:
`relocate_onto_implicit_pair` returning `None`. The new `YANG_PAIR_NEWTON_TRACE`
probe (per-iteration `f0 f1 det x` with the two surfaces) showed all three
DIVERGING on a **cone** partner with a geometric ratio that is exactly
`1 − sec α`: R0032 torus × cone α = 1.19 rad → −1.7; R0044 cyl × cone → −7.5;
R0053 cyl × cone → −2.6. Mechanism: the pair solver paired
`surface_value_and_normal`'s cone residual (the RADIAL form `l − |h|·tanα` =
distance × sec α) with the UNIT cone normal, so every Newton step was sec α
too long — convergent only below 60° (the existing 45° pin
`m5_cone_pair_relocation_onto_both` passed; the corpus cones are 61°–83°).
**KV16 had fixed exactly this in `relocate_onto_implicit_triple`** (R0017
v47, "at half-angle 60° the iteration bounces") and the pair sibling kept the
raw residual for five weeks — the same failure shape as the `8·ε·L` floor
(one rule stated in prose across two solvers). Fix: one shared helper
`surface_distance_and_normal` (cone `f·cosα`) drives BOTH solvers; pin
`m5_steep_cone_pair_relocation_converges` (α = 1.19/1.31/1.45 rad, both
argument orders; red-verified against the raw step).

Peeling R0044 then exposed two more, one per layer, each recorded here
because each is a rule this spec states in prose:

1. **Same-type SurfacePair junction (yang, KV16 precedent).** With the pair
   Newton converging, kernel-v2's K3 endpoint check rejected an output
   surface-pair edge whose endpoint sat 0.35 off its cone — because the
   vertex is the meeting point of cyl_A × cone_B1 and cyl_A × cone_B2 (the
   gear's tooth-flank crease circle: THREE surfaces), `vert_surface_pair`
   is a one-slot map (last pair wins), the triple block's `n_maps < 2`
   never saw it, and the pair loop relocated it onto (cyl, cone_B2) alone.
   Fix: detect a second DIFFERENT pair at insert time and route the vertex
   through `same_type_junction` to the triple pass (the KV16/KV16b
   pattern; 177 such junctions on R0044). Endpoint check now passes.
2. **kernel-v2 K9 sampler for cone pairs (two rules).** (a)
   `surface_pair_edge_samples` took its sag radius as
   `min(pair_surface_scale(a), pair_surface_scale(b))`, and
   `pair_surface_scale(Cone) = 0` (right for residual BANDS, spec §5) — so
   EVERY cyl×cone / cone×cone output edge dead-ended on "surface-pair
   refinement needs a positive finite chord tolerance" (R0020's recorded
   fatal wall, now R0044's too): the cone-pair producer that "ships as
   capability closure" was unreachable at render. Fix:
   `geom::pair_surface_local_scale` (cone: local radius `|h|·tanα`) and the
   sag radius = the smallest local radius over both surfaces at both
   endpoints (an apex-crossing edge still yields 0 and STOPs loudly). (b)
   `SURFACE_PAIR_PROJECT_TAU = 1e-13` "mirrors yang-rs's tested
   `relocate_onto_implicit_pair` contract" — the PRE-2026-07-28 contract;
   yang's is `max(1e-13, 8·ε·L)` since the R0025 anchor. At R0044's
   |x| ≈ 6e3 the projector could never converge. Fix: the same seed-scaled
   floor (`surface_pair_project_tau`); pin
   `surface_pair_sampler_cone_cylinder_at_large_magnitude` (red-verified).

Where the cases stand after the three: R0032 → Stage-6 reassembly
non-2-manifold; R0053 → Stage-6 reassembly non-2-manifold; R0044 → kernel-v2
render `ring rejected by CDT` on face 460 (MEASURED: a 184-node curved-patch
ring with a REVERSAL at idx 176→177→178 in the unrolled frame — vertex 177
~3.8 units behind 176 along the chain at scale 3e3, three distinct neighbour
edges — a §4.5.3 reversed intersection on a PROCEDURAL chain, which the
conic-loop `sweep_reversed_intersections` does not cover; the next
increment for this case, not a K9 sampling defect); R0020 → KV9-F2 `patch
triangulation folded` (the unrolled patch CDT). None is a §4.5.2 demand.
Corpus census: see the roadmap §0 record for this date.

## K11 re-entry — inc-1 LANDED 2026-09-04 (the last `UNSUPPORTED(curved-profile)` wall)

**Anchor.** R0044 (`revolve(rectangle) ∪ revolve(gear) − extrude(circle)`,
scale 4e3): after the corner-transit inc-3c flip its design boolean
completes and the circle cut refuses at `FaceId(458)` — a cylinder lateral
(r = 2327.8) whose 5-edge outer loop is `[Arc, SurfacePair ×4]`, every pair
this cylinder × one of three cones (the gear revolve's conical flanks). The
census (`yang_stage1_curved_holed_patch.md` "Re-census 2026-09-04") named
it M5 K11: no yang INPUT vocabulary for a degree-4 surface-pair edge.

**The paper's mechanism.** [#24] §4.1 samples every input edge into a shared
boundary chain that both incident faces splice (the bijective Stage-1
map); §4.3 refines any point of a quadric-pair curve by Newton projection
onto BOTH surfaces. A surface-pair input edge therefore needs no new
representation — its Stage-1 chain is the same recursive chord bisection
the hyperbola chain uses, with the closed-form evaluation replaced by the
projection Stage 4 already relocates with (`relocate_onto_implicit_pair`)
and kernel-v2 already renders with (`surface_pair_interior_samples`).

**Design (two sides, the KV14/KV16 pattern).**

| # | site | behaviour |
|---|---|---|
| R1 | kernel-v2 `to_yang.rs` `convert_lateral_edge` | `Curve::SurfacePair { a, b }` → ONE shared yang `BRepEdge { curve: Curve::SurfacePair { a, b } }` per twin pair (key `min(h, twin)`), endpoints from the first-encountered half-edge; operands map field-for-field (`pair_surface_to_yang`, the exact inverse of K1's `yang_surface_to_pair_surface`; Cylinder / Cone / Sphere). The M5 endpoint-determined convention: no directional normal, twins bit-identical, either side denotes the same point set. Reached by every KV14 patch-path lateral (holed, non-4-edge, 4-edge non-structured) on cylinder, cone and torus surfaces |
| R2 | kernel-v2 planar `convert_loop` | SurfacePair stays the typed `UnsupportedCurvedBoolean` (K8: a transversal quadric-pair curve is never planar; `validate_solid` rejects the solid first) |
| R3 | yang Stage-1 chain pre-pass (`stage1_tessellate.rs`, after the Hyperbola block) | per `Curve::SurfacePair` edge: `start == end` loud (no producer; K2/K6); each endpoint on BOTH surfaces at the K3/K7 band `1e-9·(1 + max(coord, local radius))` via `surface_distance_and_normal` (true distances, the cone's `·cos α`); chain = recursive chord-midpoint bisection, midpoint → `relocate_onto_implicit_pair`, split while the projection's sag > `d_ε`, depth cap 12; a projection that leaves the chord's neighbourhood (`sag ≥ chord`, a basin escape) or returns `None` (tangency / an axis / non-convergence) is loud — never a chord fallback (P9). Steiner vertices: `TessellationSource::BRepEdge { edge, t }`, `t` the bisection's ORDINAL parameter in (0, 1); `eval_source` documents that it cannot reproduce them (its only production caller is the sphere seam column). Shared chain in `rim_rings` |
| R4 | `d_ε` single source (`normals_chord_bounds.rs`) | `surface_pair_chain_bound(a, b, p0, p1) = chord_rel() × min local radius` over both operands at both endpoints (`surface_pair_local_scale`: cylinder / sphere radius, cone `|h|·tan α`) — the kernel-v2 render rule's scale under the ONE `chord_rel()` (A14.3, the `YANG_CHORD_REFINE` census knob covers it). `None` at a cone apex (degenerate) or for a non-pair operand — loud at R3 |
| R5 | `loop_polyline_attributed` | SurfacePair splices its chain exactly like an open conic arc |
| R6 | CDT admit lists | the cylinder holed-CDT gate and the cone Slice-E gate admit SurfacePair (torus dispatch has no curve gate) |
| R7 | Stage-3 owner band (`chord_tol_for_curved_owner`) | `None` fallback chain gains `surface_pair_chord_bound(owner)` — an owner bounded by pair edges ALONE (the vesica prism) carries the pair chains' own bound, not a producer-fault STOP |
| R8 | Stage-4 `input_curved_chord_bound` | folds in `surface_pair_chord_bound(brep)` (the max over pair edges) — the Slice F-3 lesson: a band must cover every chain the tessellation carries |

**Oracles (all green).**

- yang `tests_unit/m5_k11_pair_chain.rs`: a cylinder-A tube (r = 1) with a
  window of FOUR pair edges (A × cylinder-B, axis x, r = ½ — the closed
  saddle `sin²θ + z² = ¼` split at its two turning points and two poles so
  no chord midpoint sits on B's axis) re-enters Stage 1: every vertex on A,
  every pair-chain Steiner vertex ALSO on B (radial ½ ± 1e-9), ≥ 4 Steiner
  samples, ordinal `t ∈ (0, 1)`, the count-1 boundary = the two rims + the
  window (every window chord on B), wall area `4π − ∫ 2√(¼ − sin²θ) dθ`
  within 3 %. Chain bound = `chord_rel() × min local radius` (cone apex →
  `None`, plane operand → `None`; B-Rep-level max; `None` for a pair-free
  B-Rep). Loud: a closed pair edge; an endpoint off B; a chord whose
  midpoint lies on B's axis (`did not converge`).
- kernel-v2 `m5_surface_pair_curve.rs::surface_pair_reentry_enters_yang`
  (was `surface_pair_reentry_rejected`): the vesica prism converts (2 shared
  yang pair edges, operands verbatim) and a pocket in its top cap removes
  exactly 0.4 × 0.4 × 0.3 = 0.048 (within 1e-3).
- kernel-v2 `kv9_cyl_cyl_special.rs::unequal_perpendicular_union_reenters_
  with_far_pocket`: the unequal perpendicular cyl×cyl union (carries ≥ 2
  SurfacePair half-edges — the vocabulary pin) re-enters; a pocket in c1's
  top cap clear of the saddle (|z| ≤ 0.18) removes exactly 0.3 × 0.3 × 0.2 =
  0.018 (within 1e-3).

**Producer bound (code review 2026-09-04, measured).** Both samplers of a
pair edge — kernel-v2's K9 render sampler and yang's K11 Stage-1 chain —
are recursive chord-midpoint bisections: the chord's midpoint is Newton-
projected onto both surfaces and accepted when its sag is small, guarded
only by `sag ≥ chord` (basin escape) and non-convergence. That guard does
NOT catch a chord spanning more than 180° of turn about an operand: the
midpoint then projects onto the COMPLEMENTARY arc (a 240° arc's chord
midpoint lies at half the radius on the far side, sag 0.5 r < chord
1.73 r), and the chain silently traces the short way round. The
assumption holds today because the PRODUCER bounds it: kernel-v2 emits
every surface-pair edge as ONE arrangement chord (Y5, per-mesh-edge; no
chain merge in `from_yang`), measured on the unequal perpendicular cyl×cyl
union under four seam placements as ≤ 32° of turn about either operand
(104–112 pair edges per body). A future chain-merge of pair pieces (the
§4.4.2 conic seam-merge precedent) must keep each merged edge's turn well
under 180° about BOTH operands, or the samplers need a branch certificate
(the sweep cap `open_run_splits_by_sweep_cap` uses for conic runs is the
model). Not a defect today; recorded so the next producer change does not
turn it into one.

**Measured next wall (inc-2, quarantined probe
`unequal_perpendicular_union_reenters_with_crossing_cut`).** A chained cut
whose plane CROSSES the saddle (x = 0.27; the saddle spans x ∈ [0.24, 0.3])
STOPs at Stage-4 `LocalRefinementRequired` around the arrangement vertex the
plane mints on a pair CHORD (v28): the `pair curve ∩ plane` junction — a
three-surface point `{cyl_A, cyl_B, plane}` — has no relocation arm (the
crossing vertex enters `endpoints` through the plane × cylinder conic and
`vert_surface_pair` never sees it, since the pair curve is an INPUT edge,
not an intersection edge). inc-2 = the input-pair-chain junction: either a
Stage-1 override channel for pair owners (the P3a `rim_overrides` pattern,
mint = the triple Newton `relocate_onto_implicit_triple`) or the Stage-4
carried-crease transit (§4.5.1 inc-3c) recognising a pair crease. A plane
at x = 0.1 (missing the saddle) stops EARLIER at Stage-3 `AmbiguousCurve
{ candidates: 2, matched: 0 }` on a plane × c1 ruling edge (tol 1.5e-2) — a
separate, pre-existing Stage-3 class (R0026 / C0043 / C0056 / C0109), noted
for that family's census, not chased here.

**Corpus (release, 8 jobs, 360 s; F0085 313.6 s honest CORRECT):**
274C / 0W / 32E / 4EE / 0T — exactly ONE row moved against the apex-cone
canonical 274C/0W/31E/4EE/0T: R0044 `UNSUPPORTED(curved-profile)` → ERROR
at `face 166: holed lateral CDT failed` (the thin conical band above; see
`yang_stage1_curved_holed_patch.md`'s census row — CLOSED 2026-09-05 by
the thin-band chart guard; R0044 is CORRECT, genus 1 adjudicated). The
`UNSUPPORTED(curved-profile)` class is EMPTY; the only UNSUPPORTED rows
left are the two M8 coplanar cases (F0064, F0072).

## K11 inc-2 — LANDED 2026-09-05: the `pair curve ∩ plane` junction (ruling × section circle, coplanar)

**Anchor (the quarantined probe, now the pin).** kernel-v2
`kv9_cyl_cyl_special.rs::unequal_perpendicular_union_reenters_with_crossing_cut`:
the unequal perpendicular cyl×cyl union (c1 axis z r 0.3, c2 axis x r 0.18)
re-enters a box cut whose plane x = 0.27 crosses the saddle (x ∈ [0.24, 0.3])
at four points. Measured 2026-09-04: Stage-4 `LocalRefinementRequired` at
v28, the first crossing.

**Where the STOP actually was (one layer deeper than the inc-1 note).**
The inc-1 measurement read the wall as "the pair curve is an INPUT edge, so
no relocation arm sees the crossing". `YANG_LRR_SITE` localizes it to
`stage4_correct.rs`'s PR-F3 **line × circle junction arm**: the crossing
vertex is an endpoint of TWO intersection edges — plane_B × c1, a RULING
(`LineSegment`, the plane is parallel to c1's axis) and plane_B × c2, a
section CIRCLE (the plane is perpendicular to c2's axis) — so
`vert_line` ∩ `vert_circle` demotes it to `vert_junction`, whose closed form
is `line ∩ plane-of-circle` and requires the line TRANSVERSAL to the circle's
plane. Here both sections lie IN the cutting plane (`n · d = 0`), and the
arm STOPped "line parallel to the circle plane: no transversal junction".
The triple block never saw it either: a junction map counts as zero
curve-bearing maps (`n_maps`), the same accidental exclusion the KV16
same-type and R0044-bucket fixes each closed for their own map.

**The mechanism ([#24] §4.3, exact).** A point on the ruling is on B and on
c1; a point on the circle is on B and on c2; their in-plane crossing IS the
three-surface point {B, c1, c2} the pair chain passes through. The exact
junction is the in-plane line∩circle — the Task #146 closed form
`pp_line_circle_junction` (line∩sphere quadratic + circle-plane residual
certificate, "valid for the in-plane AND transversal configurations"), which
the circle × pp-line arm already used and this arm did not.

| # | site | behaviour |
|---|---|---|
| J1 | `stage4_relocate::ruling_circle_coplanar_junction` | `(line, circle, current, line_band, d_ε) → Ok((junction, gate)) \| Err(Miss \| Tangent)`. Junction = `pp_line_circle_junction` root nearest the vertex, certified against the circle plane at the scale-aware **`junction_certificate_band`** of that plane (ulp-order, the increment-3 exactness band) — the ruling and the circle are both EXACT sections of the same cutting plane, so a genuine in-plane crossing has ulp residual and a parallel-but-OFFSET ruling of any modelling size is a `Miss` (never the chord band: that would accept a non-junction and land the vertex on the line but off the circle by the offset). Gate = **`(line_band + d_ε) / sin θ`**, θ between the ruling and the circle's tangent at the junction — the pp-circle arm's Branch-6 crossing amplification with the ruling's OWN propagated band added (a ruling vertex, unlike a pp-line vertex, is not exact in the mesh). Derived, not a widening. `sin θ < TAU_MODEL` = a grazing contact → `Tangent` |
| J2 | `stage4_correct` line × circle arm, the `\|n·d\| < TAU_MODEL` branch | was an unconditional `LocalRefinementRequired`; now J1. `Ok` → the common gate / `project_onto_circle` (frame angle `t`) / retag path the transversal branch uses; `Err` → the loud stop stands (`YANG_LRR_PROBE` prints `site=line_circle_coplanar_decline` with the decline). The transversal branch is byte-identical |

**Oracles (all green).**

- yang `tests_unit/m5_k11_pair_chain.rs` (5 new): the inc-2 frame (plane
  x = 0.27, ruling y = −√(0.09 − 0.27²), circle r 0.18 about (0.27, 0, 0)):
  the measured v28 lands on the plane, the ruling AND the circle to 1e-15
  (⇒ on both cylinders as surfaces), with the derived gate
  `(band + d_ε)/(|z|/r)`; the root nearest the vertex wins; a ruling in the
  plane x = 0.2705 (parallel, offset 5e-4 — inside any chord band) is a
  `Miss` with an ulp-order certificate; a ruling tangent to the circle is a
  `Tangent`; a ruling in the plane that misses the circle is a `Miss`.
- kernel-v2 `unequal_perpendicular_union_reenters_with_crossing_cut`
  un-quarantined: all four crossings land on
  `(0.27, ±√(0.09 − 0.27²), ±√(0.18² − 0.09 + 0.27²))` (v28/v45/v87/v104,
  `[k11-inc2-junction]` under `YANG_LRR_PROBE`), the cut validates, and the
  removed volume matches the Simpson slab to **4.2e-4 relative** (pinned at
  2e-3).

**Corpus (release, 8 jobs, 420 s; wall 573.5 s, F0085 314.0 s honest
CORRECT): 275C / 0W / 31E / 4EE / 0T — ZERO rows moved against the
2026-09-04e canonical.** No corpus case has this junction type as its
current wall; the increment is pinned by the kernel-v2 probe alone.

**Not covered (recorded, not chased).** The junction's TYPE is set by the
cutting plane's two sections. Plane ∥ one axis and ⟂ the other gives ruling
× circle (this increment). An OBLIQUE plane gives ellipse × ellipse in ONE
plane — `vert_ell_junction`'s closed form is `(plane₁ ∩ plane₂) ∩ cylinder`
and degenerates when the two planes coincide; ellipse × circle and ruling ×
ellipse have no junction map at all (the single-map overwrite is the
`insert_ellipse_or_junction` class). None is measured by a probe yet; each
is the same shape as this fix (an in-plane conic∩conic closed form, or the
triple Newton with the junction maps counted as curve-bearing).


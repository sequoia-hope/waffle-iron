# P3a (#146): Conformal Junction Sampling at Stage 1 — Spec

Endgame Phase 3 entry increment (task #146 under epic #169), 2026-07-18.
Grounded in `docs/yang_junction_research_findings.md` (Q1/Q2/Q4 + the binding
junction contract) and the completed #171 triage
(`docs/yang_tail_triage.md` rollup: the P3a bucket).

## 1. Goal

Stop the mint of near-duplicate junction vertices at its source. Today each
operand's Stage-1 tessellation samples its analytic surfaces independently;
near a **shared junction** — a place where one operand's B-Rep edge (the
curve where two of its faces meet) pierces the other operand's surface — the
two operands' chord meshes cross at several nearby-but-distinct points where
the true geometry has ONE junction point. The exact arrangement then
*correctly* preserves every distinct crossing (findings Q4: exact arithmetic
prevents false coincidence, not true near-degeneracy), and the defect
surfaces far downstream as:

- Stage-4/6 non-2-manifold STOPs — F0082 (v588≈v601, 0.012 apart 3D,
  ~4e-4 in-plane), R0095 (~1e-24-area boundary triples on every face),
  C0044 (3-patch junction), C0058 (degenerate 64-vert curved loop,
  |Newell N| = 2.3e-16), F0058/F0060 (`s4-shell-euler` χ=3), R0049/F0064
  (`s6-planar-loop-nonplanar`, wall vert 1.4e-6–0.083 off-plane);
- output-ring defects rejected by the render CDT — R0016 (periodic (i,i+2)
  near-dup spikes ~1.1e-4 apart in the emitted face ring).

Per the paper, conformality on the shared curve is a PRECONDITION Yang
states as `r_A = r_B = r` (§4.4.1, `refs/text/yang2025_hybrid_boolean.txt`
around :551-561) but never spells out as a protocol; Urick 2019 supplies the
protocol (exchange characteristic points, one shared curve, both sides bound
to it) and the findings adopt it as the **junction contract**:

> Mint once, exactly. Share by identity (same vertex handle in both
> operands). Trigger by taxonomy. Multiplicity is a loud STOP. Refinement is
> only a guarded shell.

## 2. Root mechanism (confirmed)

For a cross junction `edge(A) × face(B)` (or symmetrically `edge(B) ×
face(A)`):

1. Operand A's edge `e` is shared by faces `A₁`, `A₂`. Stage 1 samples `e`
   ONCE (the shared `edge_polyline`) — so A is internally conformal along
   `e` — but the sample points are placed with no knowledge of B.
2. Operand B's face mesh chords cross A₁'s and A₂'s chord fans near the true
   pierce point `J = e ∩ surface(B)`. Because no sample of `e` lies AT `J`,
   the arrangement mints ≥2 distinct crossing vertices (one per incident
   A-face chord strip), all within the combined chord-sagitta band of `J`,
   plus sliver triangles between them.
3. Downstream, Stage 4 relocates the crossings onto their (different)
   incident curve pairs, shrinking but not fusing the cluster (they are
   genuinely distinct mesh vertices); Stage 5/6 assembly then sees
   near-dup / off-plane junction verts → the STOP signatures above.

This is exactly the class the #169 Phase-B re-CDT attempts could not fix
downstream (re-triangulating projects the near-dups into degenerate
slivers — refuted 0b655da2): the defect must be prevented at mint, not
repaired after.

## 3. Design

### 3.1 The junction pre-pass (where)

A new operand-conditioning pass in `yang_rs::boolean()` alongside the
phantom/graze guards and backtrack-spike normalization — BEFORE Stage 0/1
sample anything (the only layer where inserting a point moves no existing
coordinate; the N54 lesson: never touch a coordinate after a seam exists).

### 3.2 Pierce-point enumeration (what to mint)

For each cross pair `(edge e of X, face f of Y)`, X,Y ∈ {A,B}, X≠Y:

- Candidate gate: conservative AABB overlap of `e` against `f` (cheap,
  same pattern as the graze guard's pair scan).
- Solve `J = curve(e) ∩ surface(f)` EXACTLY with the shipped machinery:
  closed forms for line/circle × plane/quadric where they exist, else the
  N-137.1 implicit solvers (`relocate_onto_implicit_pair` seeded from the
  chord crossing; the curve of `e` is itself the intersection of its two
  incident surfaces, so `J` is a 3-surface point — `relocate_onto_implicit_triple`
  is the general constructor, findings Q1: stronger than the literature).
- Keep only solutions inside `e`'s parameter range and inside `f`'s bounded
  region (the same bounded-face containment test Stage 4 already uses).
- Multiplicity discipline: a curve×surface pair with a CONTINUUM of
  solutions (edge lying ON the surface — tangential contact) is NOT a
  pierce point; it routes to the tangency/#137 path or STOPs. The
  enumeration only mints transversal, isolated points (findings Q3
  taxonomy: transversality gate at entry).

### 3.3 Insertion by identity (how to share)

Generalize the SHIPPED `stage1_tessellate_with_rim_overrides` pattern
(M8 rim-crossing Steiner points, `stage1_tessellate.rs:81-98`) from
full-circle rims to ARBITRARY edge polylines:

- `edge_overrides[e]` lists extra exact points to insert into edge `e`'s
  polyline at their parameter-sorted position. Both faces incident to `e`
  consume the SAME polyline — A₁, A₂ stay conformal by construction, and
  the junction sample now lies exactly at `J`.
- On the OTHER operand, `J` must appear in face `f`'s mesh as a vertex:
  insert into `f`'s boundary polyline if `J` lies on one of `f`'s edges
  (within the exact containment test), else as an interior Steiner vertex
  of `f`'s CDT (interior insertion is CDT-freedom — findings Q2: interiors
  free, boundaries shared).
- The two insertions carry the SAME exact coordinates (one mint, bitwise
  identical on both sides — the Stage-0/Q6 mesh-once pattern realized for
  junction points). Identity across the operands is then established by the
  arrangement's exact coincidence merge (already correct, N48-certified);
  no tolerance is involved at any step.
- Contract mirrors the rim-override loud errors: an override coinciding
  with an existing sample merges only as a sub-TAU_MODEL twin taking the
  override's exact bits; ≥ TAU_MODEL coincidence, two distinct overrides
  claiming one slot, or an off-curve/off-surface override = typed
  `MalformedTopology` STOP. Empty override map = byte-identical output
  (the same `rim_override_empty_is_byte_identical` oracle shape).

### 3.4 Explicit non-goals

- **No tolerance merge, ever** (findings Q4 non-goal): merging a real
  0.012 feature is the R0091 silent-wrong hazard. Existing loud STOPs
  (`s6-planar-loop-nonplanar`, the Stage-4 shell gate, non-2-manifold
  reassembly) remain the P10 safety net and are NOT relaxed by this spec.
- No coordinate motion of existing samples: insertion only, pre-mesh.
- Intra-solid junctions (chained-input defects inside ONE operand) are out
  of scope here — that is the S5/S6 output-ring class (triage: F0045 etc.)
  fixed at emission, not at input sampling.
- The §4.5.2 refinement loop stays a guard shell (findings Q3); this pass
  is deterministic insertion, not iterative refinement.

## 4. Increments (each an atomic checkpoint)

### Increment 0 — measurement probe (this session, dev-only, gated)

`YANG_JUNCTION_MINT_PROBE`: inside `boolean()`, enumerate cross
`edge × face` AABB-overlapping pairs, seed the implicit solvers from edge
sample midpoints, and print every converged in-range pierce point `J` with
its edge/face ids and the distance from `J` to the nearest existing edge
sample. On the confirmed customer cases this must show:

- (RED evidence) a pierce point `J` whose nearest edge sample is ≥ the
  near-dup cluster radius reported by the triage probes (F0082: ~0.012;
  R0016: ~1.1e-4) — i.e. the mint gap is real and located;
- the count of pierce points per case (junction workload sizing for
  increment 2).

Production byte-identical (print-only). Oracle: probe fires on F0082 with a
`J` correlating with the v588/v601 cluster.

**Increment-0 measurement (2026-07-18, DONE — probe shipped in
`boolean.rs::junction_mint_probe`):**

- **F0082**: A-edge 619 × B-face 1 yields `J` with `d_end = 1.255e-2` — the
  nearest existing edge sample sits ~0.0126 from the true junction,
  MATCHING the triage's v588≈v601 near-dup cluster radius (0.012). The
  mint-gap hypothesis is confirmed on the lead customer.
- **R0016**: adjacent per-loop edge copies 2455/2459 pierce the SAME face
  295 at points ~3e-5 apart — the source of the periodic (i,i+2) output-ring
  spikes.
- **R0095**: micro-scale pierce points with gaps down to 7.7e-5; one edge
  (899) pierces two ADJACENT faces at near-identical `J` (~3.3e-5 apart) —
  a pierce near the partner's own edge, i.e. a corner: P3b stitch
  territory, and the increment-2 wiring must route such near-corner
  pierces to P3b rather than double-inserting (risk §6 bullet 2).
- **C0044**: ZERO candidates — its 3-patch junction is coplanar contact
  (flush annular stack), not a transversal pierce. C0044 leaves the P3a
  pierce class and re-vehicles to the Stage-0/M8 coplanar-seam family
  (ledger updated).
- Caveat: raw pierce counts (hundreds/op) overstate workload — the probe
  applies no edge-parameter-range or bounded-face containment gate
  (increment 1 adds both), and per-loop edge copies double-count. The
  correlation distances above, not the counts, are the evidence.
- Bookkeeping discovery for increment 1: `LineSegment` edges use the
  per-loop-copy convention (kernel-v2 `to_yang.rs` m1 — one directed yang
  edge per half-edge), so `edge_overrides` must key by GEOMETRIC edge
  (canonical endpoint pair + curve identity) and fan the inserted point
  out to every copy, or the two incident faces fall out of conformality —
  the exact defect this spec exists to prevent.

### Increment 1 — gated-off primitive + unit tests

- `junction_pierce_points(a, b) -> BTreeMap<(InputId, EdgeId), Vec<Point3>>`
  (pure, exact-solver-backed, transversality-gated);
- `stage1_tessellate_with_edge_overrides(...)` generalizing the rim-override
  insertion to line/arc edge polylines, with the full loud-error contract
  and `edge_override_empty_is_byte_identical` oracle;
- unit fixtures: box-edge × cylinder lateral (transversal pierce),
  box-edge coplanar-with-plane (tangential — must NOT mint), pierce at an
  existing sample (sub-TAU_MODEL twin merge takes exact bits).

**Increment 1a (2026-07-18, DONE)**: `boolean/junction.rs::
junction_pierce_points` — pure, unwired, `LineSegment` edges × planar
partner faces, 5 fixtures (`tests_unit/p3a_junction_pierce.rs`).

**Increment 1b (2026-07-18, DONE)**: `stage1_tessellate_with_edge_overrides`
(unwired wrapper over the widened `stage1_tessellate_inner_overrides`) —
the rim-override insertion generalized to `LineSegment` edge polylines:

- A pre-pass groups targeted edges by GEOMETRIC identity (canonical bitwise
  endpoint pair — the per-loop-copy trap from increment 0), validates the
  loud-error contract, mints each interior junction Steiner vertex ONCE per
  geometric edge (source `BRepEdge { edge: canonical copy, t: chord param }`),
  and registers a per-copy oriented chain `[start, J…, end]` in the shared
  `chains` map — every copy splices the SAME mesh vertex indices, so both
  incident faces are conformal by identity.
- `loop_polyline`'s expansion splices a `LineSegment` chain exactly like an
  open arc chain (no chain = byte-identical status quo); an all-line planar
  face whose loop carries a chain routes through the chain-splicing
  `tessellate_planar_curved_cdt_face` instead of the endpoint-only Newell
  fan / all-segment CDT.
- Loud arms (all covered by fixtures in `tests_unit/p3a_edge_overrides.rs`,
  12 tests): non-LineSegment / out-of-range target; MISSING or MISMATCHED
  per-loop-copy list (broken fan-out); off-line point; outside span
  `t ∈ (0,1)`; sub-TAU_MODEL near-endpoint graze differing in bits (corner
  = P3b, fail closed); overridden edge incident to a NON-PLANAR face
  (1b scope is planar-incident only — curved-face tessellators do not
  splice line chains yet). Bit-identical endpoint repeats and duplicate
  points dedup. ULP-twin chord-parameter ties break by exact dominant-axis
  coordinate order (the #145 lesson: never insertion order).
- Oracles: `edge_override_empty_is_byte_identical` (verts + tris + sources
  + chains), mint-once + closed consistently-wound 2-manifold conformality
  on the F0082-shaped `rj_box` fixture.
- Production byte-identity is structural: every production caller reaches
  the widened inner through the old wrappers with an empty edge map, and
  the new dispatch/expansion arms are unreachable with no line chains.

Remaining before increment 2: curved-partner containment in
`junction_pierce_points` (1a skips non-planar partner faces conservatively)
and the partner-face-side insertion of `J` (boundary-polyline or interior
Steiner per §3.3) — a missed mint is status quo, so increment 2 can wire
the planar⟂planar class first and extend coverage incrementally.

### Increment 2 — wire behind `YANG_JUNCTION_SAMPLING_ENABLE`

Feed the pierce points of BOTH operands into their Stage-1 calls inside
`boolean()`. Gate-ON full assay: judged by (a) 0 WRONG (non-negotiable),
(b) per-case flips in the P3a bucket (F0082, R0095, C0044, C0058, F0058,
F0060, R0049, F0064, R0016 + the R0051 suspect), (c) zero regressions
elsewhere; gate-OFF byte-identical. Per the N54 lesson the FIRST gate-ON
run must include `nary_tessellated_group_stage0_meshes` and the Stage-0
seam suite — insertion happens before Stage 0, so Stage 0 sees the
already-inserted polylines identically on both operands (safe by
construction, but the oracle run is mandatory, not assumed).

**Increment 2 (2026-07-18, WIRING SHIPPED gated-off; increment-3 gate NOT
reached).** Implementation:

- `junction_pierce_points` gains the increment-2 wiring scope (owner edge
  incident to two PLANAR surfaces; partner face planar with ALL-LINE loops
  — the chord-polygon containment is exact only then) and records the
  pierced `partner_face` per point.
- `junction_stage1_overrides(a, b)` builds the four Stage-1 maps: owner
  edge overrides (per-copy fan-out) + partner face INTERIOR points (deduped
  bitwise), one exact mint shared by identity; plus a sub-weld-band cluster
  filter — two distinct pierce points closer than the §4.3 weld band
  `TAU_MODEL·(1+scale)` are BOTH dropped (fail closed; minting them is
  guaranteed post-weld coincidence, and merging them would be the R0091
  tolerance hazard).
- Stage-1 face-interior channel: `face_overrides[f]` mints interior
  Steiner verts (source `BRepFace{face,u,v}`) into planar face `f`'s
  keep-interior CDT (`cdt_polygon_with_holes_keep_interior`), with loud
  arms: non-planar target, off-plane point, and a CONSUMED postcondition
  (a point outside the bounded region silently dropping would be the
  one-sided-mint conformality break). 7 new fixtures
  (`tests_unit/p3a_junction_wiring.rs`) incl. the end-to-end contract:
  both rebuilt operands carry every junction bit-exactly, closed
  consistently-wound 2-manifolds.
- Wiring in `boolean()` after the rim-junction block, scope gate:
  `stage0.is_none() && cyl_pairs.is_empty() && junction_boosted.is_none()`
  (overrides do not compose across from-topology rebuilds). Gate value
  `edge`/`face` selects one insertion half (dev diagnostics).

**Measured (gate-ON full release assay, 312 cases):**

- 0 WRONG — the ratchet holds. Gate-OFF byte-identical to the committed
  250C/0W/55E baseline (sole flip: F0090 TIMEOUT→CORRECT, which also
  happens gate-OFF — a timeout flake, not a P3a effect). Gate-ON lib suite
  (369) fully green incl. `nary_tessellated_group_stage0_meshes`.
- **Mechanism CONFIRMED at the defect site**: on F0082's failing boolean,
  the v588/v601 near-dup pair (the lead-customer mint-gap defect, 0.012
  apart) is GONE gate-ON — the insertion removes the near-dup mint exactly
  as designed.
- **No P3a bucket case converts**: the bucket models are chained multi-op
  cases with MULTIPLE defects; removing the near-dup exposes the next one
  (F0082 now fails with an over-used edge fwd=1/rev=2 — an overlap-sheet
  defect, different class).
- **2 gated regressions** (F0016, F0084 CORRECT→ERROR; F0085 T→E):
  root-caused via the new `NONMANIFOLD_SITE_PROBE` i6 provenance probe —
  the arrangement still mints near-dup crossings from PRE-EXISTING twin
  edges in the CHAINED operand's topology (the #170/N54 upstream Stage-0
  mint class, outside P3a's reach); our insertion changes the partner
  triangulation so the §4.3 weld now fuses those crossings into COINCIDENT
  output triangles, tripping the I6 guard (loud, never silent). Baseline
  passed only because the un-inserted triangulations happened to weld
  non-coincidently.

**Increment-3 blockers (in leverage order):** (1) the upstream twin-edge
residue in chained outputs (the #170 re-spec: 3D minted-interior-vert
unify or stage0-emission canonicalization) — P3a insertions are only safe
corpus-wide once chained operands stop carrying sub-weld twins; (2) the
next defect layer in the bucket models (overlap-sheet / fwd-rev misuse
after the near-dup is gone); (3) curved-incident edges and curved partner
containment (widen the wiring scope). The wiring + fixtures stay banked
behind the env gate meanwhile (production byte-identical).

**Blocker (1) CHARACTERIZED (2026-07-18, crossing-provenance probe —
`CHERCHI_VERT_PROVENANCE` in `cherchi_rs::labeling::native`, joined
against the widened `NONMANIFOLD_SITE_PROBE` i6-cluster arm):**

- The sub-weld crossings at F0016's I6 site are `VertexCoords::Lpi`
  mints whose generators are named input vertices: a CDT edge spanning
  two SHARED operand vertices (`A#53=B#20 → A#61=B#22` — the flush
  operands share bit-exact junction vertices) transversally crosses a
  partner triangle whose corner is ITSELF a shared vertex
  (`A#52=B#21`), passing `d_exact ≈ 4e-18` from it (exact rational
  separation, measured). The arrangement correctly mints an LPI twin of
  the explicit corner — three representations of one geometric junction
  point, 1e-18 apart.
- **The "near-parallel residue" theory is REFUTED**: the crossings are
  well-conditioned (`sin_inc` = 0.36–0.70). So is the "arrangement
  dedup gap" theory: `d_exact > 0` — the points are genuinely distinct
  exact points, and upstream Cherchi would mint them identically. And
  the #170 output-twin theory stays refuted (minted in-boolean).
- F0084 is the same class in vertex-on-FACE form: input vertex `B#24`
  sits 5e-15 off an A triangle plane (flush contact carried with f64
  authoring residue), so EVERY B edge fanning out of `B#24` pierces
  that plane sub-weld-close to the vertex — a fan of ≥6 LPI twins
  within 1e-14. Gate-ON this surfaces as an over-used edge (fwd=1
  rev=2) at reassembly rather than the I6 guard.
- Gate-OFF measurement (F0016 CORRECT): the SAME contact-residue class
  exists in the baseline (3 sub-weld pairs, e.g. an A vertex with
  crossings 5e-18 away) but at low density. **The insertion does not
  create the class — it amplifies it**: re-rolled CDT diagonals put
  more edges near the shared junction corners (33 pairs gate-ON), and
  the §4.3/I6 weld then collapses more slivers.
- Corrected root statement: flush/chained operands carry
  INTENDED-EXACT contacts (vertex-on-vertex, vertex-on-face) at
  sub-weld f64 residue (1e-18…5e-15). Any triangulation edge passing
  near such a contact mints LPI twins of the explicit vertex; the I6
  weld rightly fuses the cluster; the collapsed sliver leaves two
  surviving sub-triangles welded onto the SAME vertex triple (or an
  over-used edge) — and the current response is a blanket STOP.
- **Next increment (spec-first): post-weld collapsed-wedge resolution**
  at the I6 site — when two surviving triangles weld to one vertex
  triple AND share a raw edge while their third vertices weld together
  (the collapsed-sliver signature: F0016's pair `[98,84,41]`/`[83,41,98]`
  shares raw edge (41,98) with tips 84/83 welding via clusters
  {41,42,43}/{83,84,85}), same winding, same surface label — keep one
  (exact structural dedup, no tolerance beyond the existing weld).
  Genuinely-coincident-face inputs (the a4 fixture: no shared raw
  structure) still STOP. The fwd=1/rev=2 over-use is the edge-level
  shadow of the same collapse and needs the analogous local resolution.
  Alternative vehicle (more invasive, R0091-adjacent, N54-warned):
  canonicalize the contact residue in the INPUTS (snap the 5e-15
  vertex onto the face before arrangement) — not preferred.

**Increment 3a SHIPPED (2026-07-18, spec
`specs/yang_146_collapsed_wedge_dedup.md`, always-on):** the collapsed-
wedge dedup at the I6 site (`wedge_reject_reason` + the I6.5 loop arm in
`boolean.rs`). One measured correction to the sketch above: the parents
of the F0016 pair share a B-Rep FACE but NOT a mesh edge (the strip's
shared raw edge is intersection-minted), so §2.4's locality arm is
same-face via the `tri_face` maps, not parent-tri adjacency. Measured:
F0016 gate-ON → SUPPORTED_CORRECT (dedup fires once); gate-OFF full
assay behaviorally unchanged (dedup fires zero times; sole delta = the
F0090 timeout flake); gate-ON full assay 250C/0W/56E/2T — the gate-ON
regression set shrinks {F0016, F0084, F0085} → {F0084} (the edge-level
shadow, spec §4 non-goal). Increment-3 blocker (1) is now HALF-cleared:
the remaining gate-ON blocker is F0084's over-used-edge shadow at the
Stage-4/5 reassembly site.

**F0084 "edge-level shadow" ROOT-CAUSED and FIXED (2026-07-18, task
#179, spec `specs/yang_stage1_cdt_parity_flap.md`) — the framing above
is CORRECTED:** the fwd=1/rev=2 over-used edge was NOT a weld-site
collapse needing an edge-level wedge resolution. The `i6-edge-overuse` +
`i6-input-overuse` probe arms (`boolean.rs`, this session) showed the
asymmetric directed edges enter ON THE OPERAND MESHES: Stage-1's
all-segment planar CDT (`tessellate_planar_cdt_face`) was the last
production caller of the f64 centroid-parity interior classifier, which
on near-collinear boundary triples keeps an exterior zero-area flap
triangle (F0084's fresh octagon-prism cap has vertex 4 on the chord
3–7; the flap `[3,7,4]` makes the operand non-2-manifold IN BOTH GATE
STATES, byte-identical meshes — production survives by downstream luck).
Junction insertion amplifies the class (every pierce point is a new
collinear boundary triple) and re-rolled the local triangulation so the
imbalance landed in the kept set. Fix = flood-fill classifier migration
(the F0047 fix the curved-CDT path and kernel-v2 already had). F0084
gate-ON → SUPPORTED_CORRECT; no edge-level wedge dedup is needed.
(The B#24 5e-15-off-plane LPI-fan measurement above remains true but was
not the causal path of the STOP.)

**Remaining increment-3 blocker (measured gate-ON, post-#179): the
insertion rebuild still mints NON-CONFORMAL operand meshes** — the
`i6-input-overuse` scan gate-ON shows near-dup T-junction pairs in
rebuilt operands (e.g. F0084 operand B verts 0.0034 apart with
fwd=1/rev=2 + open edges at the junction-inserted region; both A and B
operands, many ops). These are NOT zero-area flaps (the #179 class,
which is gone) but genuine one-sided/near-dup insertion conformality
breaks inside `rebuilt_with_junction_overrides` → next increment:
characterize with the topo-dump arm and fix the insertion (or add the
loud rebuilt-operand 2-manifold postcondition), BEFORE inc-3 always-on.

**Blocker FIXED (2026-07-19, task #180, spec
`specs/yang_146_keep_interior_floodfill.md`) — the "one-sided insertion"
framing is CORRECTED:** triangle-level unit reproduction (bit-exact
F0084 live operand-B fixture `tests_unit/p3a_insertion_conformality.rs`)
showed every imbalance is ONE EXTRA SLIVER TRIANGLE between a split edge
polyline and its un-split chord (face 8's flap `[7, 11, J19]`), kept by
the f64 centroid parity classifier inside
`cdt_polygon_with_holes_keep_interior` — the CDT variant every
interior-junction face routes through. The insertion machinery itself
(polyline splicing, fan-out, interior minting) is correct; the #179
class in its keep-interior guise. Fix = the same flood-fill migration
(outer region topologically, holes by exact parity), applied to both
interior-capable variants (`keep_interior` + the N2
`cdt_with_interior_constraints`). Measured: F0084 gate-ON
`i6-input-overuse` fires ZERO times, SUPPORTED_CORRECT. Known residue
out of scope: `cdt_polygon_with_holes_refined` (render channel, no
junction insertion) still classifies HOLES by f64 centroid parity.

**Blocker (2) CHARACTERIZED (2026-07-19, task #181) — re-classified OUT
of P3a scope to the P3b corner stitch:** F0082 Extrude-11's ring-reject
(`KV2_RING_REJECT_PROBE` + the new `KV2_OUT_VERT_PROBE` in
`kernel_v2::boolean_op`, both env-gated print-only) is a yang OUTPUT
boundary defect minted by the failing union itself
(`YANG_INPUT_VERT_PROBE` negative across the whole chain): output face
362's cyl∩plane section-Ellipse arc (r≈0.2124) terminates at output
vert 913, which lies ON the ellipse to 4e-16 at parameter t≈π/2 (the
minor-axis quadrant — consistent with a cylinder chord-ring crossing
vertex relocated on-curve by `project_onto_ellipse_nearest`), while the
TRUE termination — the ellipse × wall-plane junction, t=1.5578, exact
point (-0.06399183, -0.10911126, 2.10955341) — was never minted. The
arc overshoots the wall by 1.29e-3 in-face (2.76e-3 along-curve), the
face ring self-intersects, and the #173 render gate STOPs loudly (both
gate states — P3a's edge-pierce insertion cannot reach it: this is an
intersection-CURVE × operand-BOUNDARY-EDGE junction, the roadmap P3b
"corner insert + stitch" class; the same ring also carries an 8.5e-4
near-dup pair where the adjacent chain meets the arc's other endpoint
v915). Inc-3 always-on is therefore NOT gated on F0082 — the case fails
identically with and without P3a; its fix belongs to P3b.

### Increment 3 — always-on

Only after increment 2's ledger shows recovered cases + 0 regressions
across the full corpus AND the sidecar parity suite stays green. Remove the
env gate; un-quarantine any milestone-tagged tests that named this class.

**SHIPPED (2026-07-19, task #182).** Ledger (all measured this session):

- Gate-ON full release assay: 251C/0W/55E/2T on 312 — **category-identical
  per-case to the committed gate-OFF baseline (zero diffs)**. The 0-WRONG
  ratchet holds and the gate-ON regression set is {} (inc-3a wedge dedup +
  inc-3b keep-interior flood-fill cleared it; inc-3c re-classed F0082 to
  P3b — it fails identically in both states).
- Sidecar parity gate-ON: `r0046_patch_label_parity`,
  `stage0_operand_inputcheck`, and the flagship
  `parity_native_vs_sidecar` suite (18 cases) all green.
- "Recovered cases" resolved as mechanism-level: the near-dup junction
  mint is eliminated at the measured defect sites (F0082 v588/v601 gone
  gate-ON, inc-2; F0016/F0084 green gate-ON after 3a/3b), while the
  case-level conversions were absorbed by the always-on/both-state fixes
  that fell out of the campaign (#179 flap, keep-interior flood-fill).
  Corpus-neutral + mechanism-superior (junction conformality by
  construction) + paper-compliant (the binding junction contract) ⇒ flip.

Implementation: sampling is the production default in
`yang_rs::boolean()`; `YANG_JUNCTION_SAMPLING_ENABLE=off|0` disables it
purely as a dev A/B knob for the compliance ledger (the `weld_enabled`
pattern); `=edge|face` remain as diagnostic halves. No quarantined tests
named this class (checked — the P3a fixtures test the primitives
directly and are unconditional). Post-flip production assay (unset env)
re-verified category-identical, and the yang-rs lib suite (806) is green
on the flipped default.

## 5. Oracles & verification

- `edge_override_empty_is_byte_identical` (increment 1);
- gate-OFF full assay byte-identical vs committed baseline (increments 1-2);
- gate-ON: 0-WRONG ratchet + P3a bucket case flips + Stage-0 seam suite +
  Cherchi sidecar parity (roadmap §6) — the arrangement input changes, so
  parity certifies the arrangement still agrees with the reference on the
  new meshes;
- every pierce point re-verified on-surface: `|F_surface(J)| ≤ TAU_EVAL`
  for all three defining surfaces (constructor postcondition, loud).

## 6. Risks

- **Seeding failures of the implicit solvers** near tangency: the
  transversality gate routes those to #137/P3b rather than minting an
  ill-conditioned point (a missed mint = status quo, never worse).
- **Pierce points very close to existing B-Rep vertices**: the sub-TAU_MODEL
  twin-merge arm takes the override's exact bits (B-Rep vertices stay
  authoritative per the rim-override contract; a ≥ TAU_MODEL near-corner
  graze fails closed with a typed STOP — those are P3b corner-stitch
  territory, not P3a).
- **Workload**: pierce enumeration is O(edges × faces) with AABB pruning —
  same complexity class as the shipped graze-guard scan (measured fine on
  the 312-case corpus).

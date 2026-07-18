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

### Increment 3 — always-on

Only after increment 2's ledger shows recovered cases + 0 regressions
across the full corpus AND the sidecar parity suite stays green. Remove the
env gate; un-quarantine any milestone-tagged tests that named this class.

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

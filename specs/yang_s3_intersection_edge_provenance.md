# SPEC — Stage-3 intersection-edge PROVENANCE (N10's durable target)

**Status: inc-1 LANDED 2026-07-30 (corpus byte-identical, 257C/0W/53E/0T,
zero deltas incl. detail strings). inc-2 next.**

## inc-1 result (2026-07-30)

`ArrangementSoup::intersection_edges` + `LabeledArrangement::intersection_edges`
(`BTreeSet<(u32, u32)>`, `(min, max)` pairs in `mesh.verts` space). Harvested in
`mesh_arrangement` step 9 per base triangle AFTER the emit branch — every
constr-marked live submesh edge, endpoints welded through the same §7 global
interner; harvesting after emission guarantees every endpoint is already
interned, so global-id assignment order is untouched (measured: full corpus
byte-identical). Remapped through `native_labeled_arrangement`'s
first-reference compaction (both-ends-referenced pairs only). The sidecar and
every hand-built fixture declare `Default::default()` (the documented
provenance-less contract, `source` precedent).

**The MEASURE-FIRST gate passed on the first run:** the constr marks DO
survive splits to assembly. Unit tests (`soup.rs`):
`intersection_edges_empty_for_disjoint_pair` and
`intersection_edges_trace_the_box_box_polyline` — the box×box harvest is
non-empty, every endpoint lies EXACTLY (RBig) on BOTH box surfaces, and the
edge graph is even-degree everywhere (closed loops — this assert would catch
any split that dropped its constr mark).

Deferred to inc-2 (with the consumption): a coplanar-overlap fixture pinning
that §4.5.5 overlap-boundary segments are harvested (they route through the
same enforcement, so they should be — verify, don't assume); and the F0083
producing-op measurement (are v118's edges provenance-confirmed?).
**Deviation:** N10 (`docs/yang_deviations.md`) — the on-both-surfaces gate.
**Customers (measured):** F0083 (`VertexOffSurface` face 388, v118 1.914e-3 off
`A:Cylinder`, exactly on `B:Plane`, both incidence edges skipped by the gate —
`specs/yang_s4_boundary_curve_relocation.md` §13-§14), R0099 (`cylpatch-vertex`
residual 8.651e-2 = 2.8% of r=3.125, probed 2026-07-29 — same family, producing
op unprobed). Both were SILENTLY WRONG until `kernel-v2/strict-validation`
(5b891ec2) made the on-surface tripwire visible in release.

## 1. The paper requirement (this is the spec)

Yang §4.2.3 (`refs/text/yang2025_hybrid_boolean.txt:511-515`): "After the mesh
intersection step, **each intersection curve is represented as a sequence of
vertices**. Owing to the implicit point representations used in [Cherchi et al.
2022], we can **directly map each intersection point back** to both NURBS
surfaces … **by querying the triangles that intersect at that point**."

The paper never *classifies* an edge as intersection-or-not from geometry: the
mesh intersection step (the arrangement) HANDS it the intersection curves with
per-point provenance. Our Stage 3 instead reconstructs that information from
patch-boundary adjacency (`compute_phase_a` pushes each patch's surface onto
every boundary edge of its cycle) and then gates the reconstruction with the
on-both-surfaces predicate (PR-YR18, deviation N10). N10's own ledger text
names the durable fix: "consume true mesh-level two-surface provenance from
the `LabeledArrangement` producer (the paper's intent) … the
producer-provenance route remains the durable target."

## 2. Why the gate cannot be repaired in place (six refuted discriminators)

`specs/yang_s4_boundary_curve_relocation.md` §14-§17 refuted five edge-local
repairs (asymmetric acceptance; far-endpoint-on-one-surface; magnitude ratio;
tangent alignment; per-instance chord bound): **the mis-seat contaminates every
predicate computed from the edge's own endpoints.**

The sixth — the §17 CHAIN lead ("classify each edge against the run of edges
sharing its surface pair") — is refuted at design review (2026-07-29, this
spec) by the PR-YR18 oracle fixture itself, BEFORE building:

| | chain | exact vertices | far vertex | required outcome |
|---|---|---|---|---|
| YR18 fixture (`tests/yr18_attribution.rs`) | 46-vertex seam ring | **45/46** | S0: on Plane, 1.005e-1 off Cylinder (2.90× tol), chain-interior | **skip** (oracle) |
| F0083 chain fragment (v116-v118-v80) | ≥3 vertices | **1/3** | v118: on Plane, 1.914e-3 off Cylinder (2.76× tol), chain-interior | **admit + relocate** |

The chain-exactness fraction points the WRONG WAY (the fixture's chain is far
"healthier" than F0083's, yet must be skipped), and every per-vertex property
matches across the two rows. Two further candidate rules also fail on
legitimate geometry: *admit-by-witness* would drag a mid-wall vertex (exactly
on its own cylinder, far off the plane — YR18's real-world motivating case)
onto the curve, and *which-surface-off* cannot distinguish a drifted curve
vertex from a legitimate plane-interior vertex adjacent to the seam (both:
on-plane, off-cylinder). **A true intersection edge with one drifted endpoint
and a mis-tagged single-surface edge are structurally identical to every
geometric observer. Only the producer knows which edges it minted as
intersection constraints.**

## 3. The design

### 3a. cherchi-rs: constrained-edge provenance survives to the output

The arrangement already computes the intersection segments explicitly:
`group_constraint_segments` (`arrangements/aux_structure.rs:400`) extracts one
`ConstraintSegment` per transversal pair per base triangle;
`enforce_constraints` (`arrangements/enforce.rs:138`) realizes them as
constrained submesh edges (`FastTrimesh::set_edge_constr`, the port of
upstream's `setEdgeConstr`). Increment 1 propagates those marks through
submesh→output assembly into a new field:

```rust
/// Undirected output-mesh edges minted by tri×tri intersection
/// constraints (Cherchi's constrained edges), as (min, max) vertex-index
/// pairs. The Stage-2 contract's per-EDGE provenance, complementing the
/// per-TRIANGLE `source`. Empty from a producer that does not track it
/// (the sidecar parity oracle); a native arrangement always populates it.
pub intersection_edges: BTreeSet<(u32, u32)>,
```

on `LabeledArrangement` — the exact `source` precedent (deviation N4's
resolution), including its empty-fallback contract.

### 3b. yang-rs Stage 3: provenance-first classification

In `build_intersection_curves`, an edge with two-`InputId` incidence is an
intersection edge **iff the producer minted it** (`intersection_edges`
membership — following the edge through any Stage-1.5/2.5 vertex remaps the
same way `source` is followed). The on-both gate remains ONLY as the fallback
for provenance-less producers (empty set) — the sidecar path keeps today's
behavior byte-identical.

### 3c. Selection contract for a provenance-confirmed edge with one off endpoint

Admission alone re-raises `AmbiguousCurve{matched:0}` (the §15 measurement:
selection requires BOTH endpoints within `tol`). For a provenance-CONFIRMED
edge, `matched == 0` because one endpoint drifted is no longer ambiguity — the
producer vouches for the edge. Select the unique curve through the
**witness** endpoint (the one within `tol` of both surfaces); the far endpoint
becomes a Stage-4 relocation obligation onto that curve (the machinery that
already exists — `relocate_onto_implicit_pair` / conic closed forms). A
provenance-confirmed edge with NO witness endpoint stays a loud
`AmbiguousCurve` (P9: never silently guess between candidate curves).

### 3d. YR18 oracles stay green by their own letter

`oracle1`/`oracle2` assert only that the fixture's boolean does **not** raise
`AmbiguousCurve { matched: 0 }` ("A success is fine; a NON-AmbiguousCurve
error is also acceptable"). The fixture hand-builds its `LabeledArrangement`,
so its `intersection_edges` is empty → provenance-less fallback → today's gate
→ skip → green, unchanged. No oracle renegotiation needed.

## 4. Increments

- **inc-0** — this spec.
- **inc-1 (cherchi-rs)** — propagate `set_edge_constr` marks through the
  submesh→global assembly; populate `LabeledArrangement::intersection_edges`;
  unit tests on a two-box crossing (the minted ring edges are exactly the
  constraint-segment edges; a glued-but-not-intersected seam contributes
  none). MEASURE FIRST: confirm the marks exist at assembly time on a real
  case (a probe counting constrained edges pre/post assembly), since the
  merge step may currently drop them.
- **inc-2 (yang-rs, gated `YANG_S3_EDGE_PROVENANCE_ENABLE`)** — 3b
  classification + 3c selection. First measurement ON F0083's producing op:
  are v118's edges (80,118)/(116,118) provenance-confirmed? If NOT, the
  vertex was minted upstream of the arrangement (Stage-0 overlay or inherited
  input defect) and this spec re-routes the same way §13 re-routed inc-4.
- **inc-3** — corpus measurement both gate states; flip per the
  zero-regression precedent (#169 P3b inc-5). Candidate conversions: F0083,
  R0099 (if its producing op shows the same class).

## 5. Non-goals

- No change to the incidence map's construction (`compute_phase_a`) — the
  provenance CONFIRMS or REFUTES an edge's tag; it does not replace the
  surface attribution that Stage 4 relocation needs.
- No relocation of provenance-less edges beyond today's behavior (sidecar
  parity stays byte-identical).
- The N10 "deferred follow-up" (oblique cone∩plane conics) is untouched —
  a provenance-confirmed conic edge still raises its deliberate loud
  `AmbiguousCurve` until analytic conic support lands (N7).

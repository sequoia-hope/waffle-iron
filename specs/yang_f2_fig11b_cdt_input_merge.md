# F2 — the Fig. 11(b) merge as input of the §4.4.1 update's CDT

Status: increment 0 DONE, increment 1 landed with it (2026-11-25); §8 (2026-11-26): the classes sorted by paper statement; the line pinch RULED (§8.3) and read per sheet by the harness; **increment 2a LANDED (§8.6): the N55 scan is deleted and the classes are merged in its place, all at once**; increment 3 (the sub-τ collapse re-censused) is recorded with it. Increment 4 (the classes merged ahead of the §4.5.3 sweep) is MEASURED and not landed (§8.7): it moves one case, P0053, to a loud ERROR that the next sub-step of the Stage-4 split answers. Then 2b (the classes as the update's CDT input, with N106). Roadmap: `docs/yang_faithfulness_roadmap.md`
F2, item "The Fig. 11(b) merge carries the paper's threshold". Ledger: N55
(the merge), N89 item 7 (threshold), N89 item 1 (the sub-τ collapse).
Matrix: rows 4.4-13, 4.3-17; finding 5.

## 1. The paper

§4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:546-571`): the intersection
curves are mapped to both meshes, `r_A = r_B = r`, and "through CDT we obtain
valid discretizations of the trimmed meshes". Fig. 11 (`:556-564`):
"(a) We locate the constrained edge containing q and split it using q.
(b) If an endpoint p of the split edge is too close to q, we merge p with q
as shown in (c) to improve the mesh quality." The text then recomputes d(T)
for the new boundary triangles (`:569-571`).

Read literally, the merge acts on the CDT's INPUT: p and q are points of the
constrained boundary, and the triangulation is built after they are one point.
Nothing is rewritten one pair at a time, so there is no merge order.

**Who p and q are (the figure, read 2026-11-26).** The caption says "the blue
and yellow mesh patches are the discretizations of two adjacent surfaces from
one B-Rep model … point q is an intersection point on the boundary curve".
In the drawing q sits on the edge between the blue and the yellow patch, and
p is the next mesh vertex along that same edge. So Fig. 11(b) merges a vertex
of ONE operand's mesh, lying on a boundary curve of that operand, into the
intersection point on that curve. Two other statements cover the other close
pairs, and they are not Fig. 11(b):

- §4.3.3 (`:535-536`): "During optimization, we remove a point if it is too
  close to another point on the same loop" — two intersection points.
- §4.4.1 (`:563-564`): "we remove a mesh vertex if it is too close to the
  intersection curve on the mesh" — a vertex inside a face (matrix 4.4-15).

§8 sorts the classes of §3 by these three statements.

**Threshold (owner ruling, 2026-11-25, A15.7 prose sign-off):** "too close"
is *the same point within floating-point roundoff*,
`TAU_WORK·(1 + scale)` (`stage4_correct/collapse.rs` `is_relocation_coincidence`).
The paper gives no number. A quality threshold is a separate, later
increment that needs its own census (N89 item 7).

## 2. What the code does today

Two passes stand in for Fig. 11(b) and §4.3.3's removal, both as index
rewrites made ahead of, or after, the update's CDT:

| pass | site | domain | order |
|---|---|---|---|
| same-point class merge (N55) | `stage4_correct.rs` "(3c)" block, before `update_patches_ex_with_bounds` | the classes of `stage4_fig11b::fig11b_vertex_classes`: vertices joined by a mesh edge, one end an intersection point, the same point within `TAU_WORK·(1 + scale)` | none — every class is rewritten onto its survivor at once (`collapse::merge_vertex_classes_with_bounds`), and the classes are formed again until none is left |
| #194 sub-τ collapse | `stage4_correct.rs` before the §4.4.3 gate; `stage5_topology.rs` after KV15b | any mesh edge | `BTreeSet<(u32,u32)>` order |

Measured 2026-11-26 with the class merge in place (§8.6):

- The class merge makes 296 merges on the corpus and 846 over the sweep's
  1,870 builds, and no class reaches the update.
- The sub-τ collapse's edges are four populations, named by number below:
  (1) two intersection points an ulp apart at Stage-4 entry; (2) two rim
  points that the boundary-curve relocation brings together after the
  update; (3) a rim vertex beside a vertex Stage 0 left inside a face
  (harmless: the body is the same without the pass); (4) two points of ONE
  operand's mesh (`pocket_operand_reenters_plain_boolean`), a Stage-1
  conformity defect that belongs to F5b and is out of this spec.
- Population (1) is taken by the class merge. The Stage-4 site of the sub-τ
  collapse takes nothing on the corpus or the sweep; its Stage-5 site takes
  five pairs in three rotated builds — population (2) in R0015 m1 and
  population (3) in C0043 m1 and m2. The pass is kept for population (4).

## 3. Design

### 3.1 Domain

A pair `(p, q)` is eligible when:

1. a mesh edge joins `p` and `q`, after the Fig. 11(a) splits are applied
   (§4.4.1: "an endpoint p of the split edge");
2. `q` is an intersection point, i.e. a vertex of both operands' meshes. This
   is read from `VertexParamMap` (`params[v][i].face.input` names A and B)
   together with the triangle star (`stage4_fig11b::operand_bits` takes the
   union of the two; a mesh that carries no parameters has the star alone).
   This is the stashed trial's rule (`merge_points_into_intersection_points`);
3. `|pq| < TAU_WORK·(1 + scale)`, where `scale` is the largest coordinate
   magnitude of the pair. Two indices at ONE position (`|pq| = 0`) are
   eligible: the N55 scan merges them today (11 of its pairs, census
   2026-11-25), and §4.4.1 sets `r_A = r_B = r`.

Two vertices of one operand's mesh, with no intersection point between them,
are never eligible (that is population 4; the paper's update never merges two
original mesh vertices).

### 3.2 Classes, not pairs

The eligible pairs are the edges of a graph, and the merge is decided per
connected component (union-find). A partition does not depend on the order
the pairs are read in; that is the property the index-order scan lacks.

- **Survivor** of a component: an intersection point. Among several, the
  existing exactness rank decides (closed-form two-curve junction > conic
  endpoint > other intersection point, as `sub_feature_merge_direction`),
  then the lexicographically least position, then the lowest index (the
  index only separates two indices at one position). Each is a function of
  the component, so the survivor's position does not depend on order or on
  the numbering of the vertices.
- **Diameter guard.** If the component's diameter exceeds the band (a chain
  of close vertices), it is not merged. It is counted by the census as
  `fig11b_class_too_wide`. Leaving it alone is today's behaviour for such a
  chain (the trial's resolve-and-re-measure), and the §4.4.3 gate stays loud
  on whatever that leaves.

### 3.3 Where it is applied

Inside `update_patches_ex_with_bounds`, after Phase A gives each patch its
boundary cycles and the Fig. 11(a) splits are known, and before
`split_cycle_with_hairpin_cancel`:

1. Build the classes over the whole mesh (both operands, every patch), so the
   two faces of a shared boundary edge see the same merge.
2. Map every boundary cycle through the class map, and drop consecutive
   repeats. A cycle that folds onto itself is read by the existing hairpin
   cancel, as in Fig. 11(a).
3. Mark every patch that holds a merged vertex (its cycle OR its interior
   triangles) for rebuild, along with the patches the update already
   rebuilds (`has_isect`, `has_split_bd`, `has_moved_degen`). A patch whose
   only change is the merge is rebuilt by the same CDT from the mapped
   cycles. No triangle is rewritten in place, and nothing goes through
   `collapse_vertex` or its membrane cancellation.
4. Recompute d(T) on the rebuilt triangles (the update already does this,
   `stage4_dt`).
5. Map the boundary a plan must reproduce through the same classes. The
   update accepts a plan only when `plan_matches_boundary(new_tris, want_bd)`
   holds, where `want_bd` is the patch's old directed boundary (or
   `expected_boundary_after_splits`). Unmapped, that check would refuse every
   merged patch; mapped, the two faces of a shared edge expect the same
   merged boundary, which is what keeps the result conformal.

Interior victims (read 2026-11-25): a victim may be an INTERIOR vertex of
its patch (a mesh vertex beside the curve, not on the cycle). The update's
rebuild carries a patch's interior vertices (`rebuild_patch_domain`, "the
interior vertices carried") and already takes an `excluded_interiors` set;
an interior victim goes in that set, so the CDT neither keeps it as a
Steiner point nor needs its index mapped.

If the CDT refuses a patch the merge marked (a crossing, or the d(T) budget),
the refusal is the update's existing typed failure for that patch. It is
never a fallback to the index rewrite.

### 3.4 What it retires

- The N55 scan (the "(3c)" block, `stage4_correct.rs`) is DELETED
  (increment 2a, §8.6). N55 stays open until the merge is the update's CDT
  input (increment 2b).
- Two intersection points an ulp apart at Stage-4 entry have left the
  sub-τ collapse (§8.6). That pass stays, with its Stage-5 site, for two
  points of one operand's mesh until F5b.
- Two rim points that the boundary-curve relocation brings together run
  after the update (R0015 m1). They belong to the roadmap's next item (split
  `stage4_relocate_and_correct_inner` into relocate → §4.3 dedup → §4.5.3 →
  update → validate) and stay with the sub-τ collapse until then.

Fig. 11(c)'s interior-point insertion ("If there are no other mesh vertices
within an intersection loop, we insert one point") belongs to the same
roadmap item but is a separate increment; this spec does not touch it.

## 4. Increments

Each is a checkpoint, and run in this order: **`cargo test -p yang-rs`
before any corpus, sweep or seed run** (2026-11-25 trap: the rewrite tier
held the only customer for population 4).

0. **Census, print-only** (`YANG_FIG11B_CENSUS`). At the update's input,
   log the classes §3.2 would form (size, diameter, survivor rank, patches
   touched), alongside the pairs the N55 scan and the sub-τ collapse take in
   the same build. Run it on the corpus and the 1,496 rigid-motion builds.
   Anchor R0015 m1: name the step that brings its two rim points together,
   and where it sits relative to the update. Expected: every N55 pair and
   every population-1 pair falls in a class, and no class is too wide. A
   pair outside every class names a domain gap, and that stops the plan.
1. **Primitive, not wired.** `fig11b_vertex_classes(mesh, vertex_params,
   attribution) -> Fig11bClasses`, with unit tests:
   - the partition and survivors are unchanged under a permutation of vertex
     indices (the property the scan fails);
   - a one-operand pair is refused (population 4's fixture from
     `pocket_operand_reenters_plain_boolean`);
   - a component wider than the band is refused;
   - an F0060-like pinch fixture: two pairs where one lies on a membrane a
     single-pair merge would cancel; both merge, in either index order, to
     the same cycles.
   The stashed trial's tests (`tests_unit/s441_fig11b_merge.rs`,
   `git stash` "trial 2026-11-25") are the starting point.
2. **Wire the classes and delete the N55 scan.** Taken in two steps (§8.6):
   **2a, DONE** — the classes merged in the scan's place, all at once;
   **2b** — the classes as input of the update's CDT (§3.3), which waits on
   N106. Gates of each step, in order:
   - `cargo test -p yang-rs`;
   - `./scripts/test.sh rewrite`;
   - the release corpus (WRONG = 0; any loss is a named dip, roadmap §1
     item 4), with F0060 and F0058 read individually;
   - the 1,870-line rigid-motion sweep diffed against the baseline field by
     field (`results.json` details, not only the line);
   - seed 6;
   - CI's always-on harness
     (`cargo test --release -p test-harness --no-fail-fast -- --skip smoke_corpus_boundary_categories`).
   The ledger (N55, N89 items 1 and 7) and matrix rows 4.4-13 and 4.3-17 move
   in the same commit.
3. **Re-census the sub-τ collapse. DONE with 2a (§8.6):** its Stage-4 site
   takes nothing on the corpus or the sweep; its Stage-5 site takes the
   boundary-relocation pair (R0015 m1) and the Stage-0 pair (C0043 m1, m2).
   Recorded in N89 item 1.

## 5. Risks

- **The CDT refuses where the rewrite succeeded.** A rebuilt patch can fail
  the update's d(T) budget or crossing check, where the in-place collapse
  passed. Each such case becomes a typed ERROR, named against F2, and is
  allowed only as a named dip; never a WRONG.
- **Pinch topology.** F0060 is the known case where the outcome depends on
  merge order. The class merge takes both of its pairs at once; whether the
  update's cycles then read the pinch correctly is the point increment 2
  measures, and that result is unknown until then.
- **Rotated-only customers.** The N55 scan's corner customers (P0027, R0050
  under rotation) appear only in the sweep, so the sweep is a gate, not an
  afterthought.

## 6. Increment 0 result (2026-11-25)

Census `YANG_FIG11B_CENSUS=<file>` (`stage4_fig11b::census`), release
corpus plus the 1,870-line rigid-motion sweep (374 cases × (ref + 4
motions)). The census is print-only: corpus 360C/0W/9E/5EE/0T and all 1,870
sweep lines byte-identical to the 4a8ec737 baseline.

- **Every N55 merge is in a class formed before the scan**: 294/294 on the
  corpus, 884/884 over the sweep. The first run found 12 outside; 11 were
  exact duplicates (`|pq| = 0`), which the first draft refused (copied from
  the sub-τ collapse's `d2 == 0` skip) and the N55 scan merges, so §3.1
  admits them. The 12th was a census record that two children's appends
  interleaved (`writeln!` on a `File` is several writes; fixed to one
  `write_all`).
- **No component is wider than the band** (0 `too_wide`).
- **Sub-τ collapse:** 37 of its 46 sweep merges are classes at the
  update's input (population 1, which N55's triangle-area filter does not
  take). The other 9: C0043 #1/#2 (population 3), R0015 #1 (population 2),
  and F0059 #2's four Stage-5 merges, made after the construct passes
  (1.7e-15…1.1e-14), the same shape as population 2. None is answered by a
  merge at the update's input.
- **Classes no pass merges today** (new merges once wired): F0060 ×2
  (1.5e-16, 9.2e-17 — the pairs that made F0060 WRONG under the ordered
  merge), P0041 #2 (5.7e-14), R0032 #0/#1 (1.8e-12, 2.2e-12). R0032's pairs
  are an order of magnitude above every pair merged today and inside the
  band; increment 2 reads them by name.

## 7. Experiment: every class at once, by index rewrite (2026-11-25)

Question (§5 "Pinch topology"): is F0060's failure the ORDER of the merges,
so that an order-free index rewrite of every class already holds it, or is
something else needed? Uncommitted switch in the (3c) block: build the
classes there (the junction / conic-endpoint rank of the scan), rewrite every
`victim -> survivor` at once, drop the degenerate triangles, cancel
opposite-winding duplicate pairs once, then let the scan run (it finds
nothing). One release binary, switch off vs on. The diff is not kept in the
tree.

| case | scan (today) | all classes at once |
|---|---|---|
| F0060 | CORRECT | **WRONG**: 2 non-manifold edges, χ = 6 (want 4) |
| F0058, R0032, P0041, P0005, P0017, P0027, P0040, P0059 | CORRECT | CORRECT |

Reading F0060 (authored equal-radius perpendicular cylinders; the two
intersection ellipses cross):

- The scan makes 34 merges and two membrane cancellations; the class rewrite
  makes 36 merges (22 classes) and no cancellation. After either, the mesh
  has 172 triangles, no open edge and no edge used more than twice
  (`YANG_NM_EDGE_PROBE`, `after-3c-merge` and `before-validate`). The fault
  appears later, at reassembly.
- The survivor rule is not the cause: picking the scan's survivor (lowest
  index at equal rank) is still WRONG, the same two edges.
- **The cause is two pairs the scan never merges**: 9/10 at
  (0, 0.3, −0.3), 1.5e-16 apart, and 21/24 at (0, −0.3, 0.3), 9.2e-17 apart
  — the points where the two ellipses cross. Each is an intersection point
  pair joined by a curve edge (one A and one B triangle on it). In the scan,
  merging a neighbouring class first turns the triangle on that edge into
  half of an opposite-winding duplicate pair, which is cancelled, so the edge
  is gone before it is read. Leaving out either pair gives 1 non-manifold
  edge; leaving out both gives CORRECT.

So F0060 is not decided by merge order once the merge is order-free: it is
decided by whether 9/10 and 21/24 become one vertex. The scan avoids it by
accident of its cancellations, not by a rule. Building the merge into the
update's CDT does not change this by itself: the same two pairs are in the
same classes.

**Correction (2026-11-26): the two pairs are not at a crossing of the two
ellipses.** F0060 is A, a cylinder of radius 0.3 about z with caps at
z = ±0.3, minus B, a cylinder of radius 0.3 about y. The two intersection
ellipses lie in the planes y = z and y = −z and cross at (±0.3, 0, 0). The
pairs are at (0, 0.3, −0.3) and (0, −0.3, 0.3): the ends of the two lines
x = 0, z = ∓0.3 along which B's lateral face is tangent to A's cap planes
(`specs/yang_tangency_pinch_split.md` §0a; the ten four-triangle edges the
probe prints at `s4-entry` lie on those lines). `A − B` is pinched along
both lines and is emitted as four shells; at each end of a line one ellipse
touches both rims, and the solid's sheets there each need their own vertex
(the Stage-4 entry edge-pinch split, N74/N78). Merging 9 with 10 joins two
sheets at that point. The pair's two points lie on one ellipse, an ulp
either side of x = 0, so the question is not one curve against two: it is
what a point merge may do at a tangential contact, which the paper's
"regular" intersection curves (`:561-562`) do not include. §8 reads the pair
by the faces its two points lie on.

## 8. The classes by paper statement, and F0060 read (2026-11-26)

### 8.1 Census

`stage4_fig11b::pair_kind` reads which statement of the paper a close pair
falls under from the faces each point carries parameters on
(`VertexParamMap`, §4.2.3) — no surface type and no curve type. The census
prints it for every mesh edge inside a class and for every pair the N55 scan
takes. One release binary, corpus plus the 1,870-line rigid-motion sweep;
print-only (corpus 360C/0W/9E/5EE/0T, `results.json` byte-identical, every
sweep line identical to the 4a8ec737 baseline).

| paper statement | kind | N55 merges, corpus (294) | N55 merges, sweep (884) |
|---|---|---|---|
| §4.3.3 `:535-536`, two points of one loop (same faces) | `same-curve` | 245 | 631 |
| §4.3.3, at the loop's end (one point on every face of the other, and more) | `curve-end` | 14 | 100 |
| Fig. 11(b) `:562-563`, operand vertex on a boundary curve beside the intersection point on it | `fig11b` | 33 | 147 |
| §4.4.1 `:563-564`, operand vertex inside a face beside the curve | `near-curve` | 2 | 3 |
| none: each point on a face the other is not recorded on | `different-faces` | 0 | 3 |

- **Five of six merges the scan makes are §4.3.3's removal, not Fig. 11(b)**
  (259 of 294 on the corpus, 731 of 884 over the sweep). N55 and matrix row
  4.4-13 name the scan as Fig. 11(b); rows 4.3-17 and 4.4-13 now carry the
  split.
- Every `fig11b` pair (30 class edges on the corpus, 147 over the sweep) is a
  relocated intersection point beside an operand vertex the relocation did
  not move — the figure's q and p.
- Of the `same-curve` class edges, 195 of 205 on the corpus and 592 of 648
  over the sweep join two intersection points NEITHER of which was relocated:
  the arrangement hands Stage 4 two points of one curve within roundoff of
  each other. What mints them is not read here.
- `different-faces`, nine class edges in three rotated builds: C0043 motion 0
  (one, merged by the scan), F0060 motion 2 (two, both merged by the scan) and
  C0043 motion 3 (three, an A rim vertex beside a point on A's face and B's
  rim; the scan does not take them and they reach the update — population 3
  of §2). Each is one corner of both operands: A's rim on a face of B beside
  B's rim on a face of A, 7e-17…6e-16 apart. Each vertex is recorded only on
  the faces of the triangles the arrangement cut it from, so the record of
  two vertices at one corner can differ. At this band they are one point
  (`r_A = r_B = r`, `:550-554`); the survivor must take the union of the
  faces (`VertexParamMap::merged_into`).
- The survivor of §3.2 (exactness rank first) is not always the member on
  the most faces: in 10 corpus classes and 99 over the sweep a curve's end
  (three faces, rank 0) merges into the curve point beside it (two faces,
  rank 1 as a conic endpoint) — R0021, R0046. Positions agree within the
  band, so the rule stands for the POSITION; the wiring must leave the
  survivor on every face of its class, or the curve's end loses its boundary
  curve.
- Left at `before-441-update` (the sub-τ collapse's population 1): none on
  the corpus; over the sweep 21 `same-curve`, 15 `curve-end`, 3
  `different-faces`.

### 8.2 What this changes in the design

Nothing in §3.3: in this representation every one of the five kinds takes
effect the same way — the survivor replaces the victim in the CDT's input —
and §3.1's domain (a mesh edge, an intersection point at one end, the same
point within roundoff) holds all of them. What changes is what the merge is
called and where it sits:

1. It is three statements of the paper, and by count it is §4.3.3 first.
   §4.3.3's "too close" has no number either; its recorded threshold is
   `TAU_MODEL·(1+scale)` for relocated pairs (N47, N56, both DELEGATED). Every
   pair measured here is inside the roundoff band, which is inside that one,
   so wiring the classes needs no new threshold. Widening the §4.3.3 part to
   N47's band is a separate question and is not part of this spec.
2. §4.3.3 removes points "during optimization", before §4.5.3 and §4.4.1
   (N81). The classes are formed after the §4.5.3 sweep, where the scan sits
   today. Whether the sweep reads a pair of one class as a reversal is the
   measurement the roadmap's sub-step split starts from (F2, "split
   `stage4_relocate_and_correct_inner`"); increment 2 does not move the site.
3. The N55 scan retires into this merge whole; its ledger entry and the
   matrix name both statements.

### 8.3 F0060: correct today on two vertices an ulp apart

F0060's `A − B` is pinched along two lines (§7 correction) and is emitted as
four closed solids. Read with `ASSAY_WATERTIGHT_PROBE` on the §7 experiment
(every class merged):

- The two edges the watertight oracle reports are the half of each pinch
  line from its end to the cap centre: (0, −0.3, 0.3)–(0, 0, 0.3) and
  (0, 0, −0.3)–(0, 0.3, −0.3). Each is used by four render triangles of four
  different faces: a cap piece and a piece of B's lateral on the x > 0 side,
  and the same two on the x < 0 side. That is two closed sheets sharing an
  edge, which is what a solid pinched along a line is.
- The Euler oracle's figure agrees: four closed shells (χ = 8) that share two
  edges count V − E + F = 6, the value it printed.
- With the scan, the same body differs in one thing: at the end of each line
  the two sheets hold two different vertices, 1.5e-16 and 9.2e-17 apart
  (9/10, 21/24), which the scan never merges because an earlier merge's
  membrane cancellation removes the two zero-area triangles that joined them.
  The oracle pairs edges by exact f32 bits, so the two sheets' edges do not
  meet and all four shells read closed.

So F0060 is CORRECT today because the kernel leaves one model point as two
vertices, and it turns WRONG the moment that point is one vertex — by any
mechanism, this merge included. The harness pins the reading that fails it:
`oracle.rs::euler_characteristic_two_shells_sharing_an_edge_still_fail`
(two shells sharing an edge are non-manifold for the watertight oracle),
written beside the test that credits two shells sharing a VERTEX
(`yang_tangency_pinch_split.md` §0c). The two cannot both hold for a line
pinch whose ends are exact.

The paper does not cover tangency along a curve: its intersection curves are
regular (`:561-562`) and its tangent case is an isolated point (§4.3.3). What
the kernel emits for a line pinch, and what the oracle accepts, is the
owner's ruling (governance A15.7; `docs/kernel_1_0.md` gate 3.7, "tangency
along a curve"). Increment 2 is not wired before it: under either answer
F0060 changes category.

**Ruled by the owner, 2026-11-26 (prose sign-off, A15.7: "approved, go
ahead with your recommendation").** A solid pinched along a line is emitted
the way a solid pinched at a point already is: each sheet holds its own copy
of the line's vertices and edges, at identical positions, and each sheet is
a closed manifold shell. The paper is silent (its intersection curves are
regular, `:561-562`; its tangent case is an isolated point, §4.3.3), so this
is the recorded choice for `docs/kernel_1_0.md` gate 3.7's "tangency along a
curve". The harness reads it by the B-Rep, not by position alone: a mesh
edge used four times is two sheets only where its four triangles lie in four
faces and pair, in exactly one way, into two pairs that cross the edge in
opposite directions and whose faces share a B-Rep edge
(`oracle::line_pinch_sheets`, `check_watertight_mesh_with_sheets`,
`check_mesh_euler_characteristic_with_sheets`). A four-use edge without
that backing still fails
(`oracle.rs::euler_characteristic_two_shells_sharing_an_edge_still_fail`,
`::a_four_use_edge_the_brep_does_not_back_still_fails`).

### 8.4 Increments, restated

2. Wire the classes and delete the N55 scan: 2a DONE, 2b open (§8.6).
3. Re-census the sub-τ collapse: DONE with 2a (§8.6).
4. New, with the roadmap's sub-step split: measure whether the §4.5.3 sweep
   removes or reorders members of a class, and move the §4.3.3 kinds ahead of
   it if it does (N81). Measured (§8.5), tried on the patch-loop walk
   (§8.7), DONE on the chain walk (§8.10).

### 8.5 Which pass takes each pair today (census, 2026-11-26)

The same census at two more sites — `after-reloc` (the relocation done,
nothing merged yet: where §4.3.3 removes its points, "during optimization")
and `before-453-sweep` — follows every same-point pair of a Stage-4 run to
the last site that still holds it. Counts are per run (the §4.5.2 loop's
re-runs included): 568 runs on the corpus, 2,210 over the sweep. Print-only;
corpus and sweep unchanged.

| kind at `after-reloc` | corpus | taken before the §4.5.3 sweep | by the sweep | by the scan or the N47 weld after it | reach the update |
|---|---|---|---|---|---|
| `same-curve` | 280 | 42 | 0 | 238 | 0 |
| `curve-end` | 14 | 0 | 0 | 14 | 0 |
| `fig11b` | 36 | 0 | 12 | 24 | 0 |
| `near-curve` | 10 | 0 | 0 | 10 | 0 |

| kind at `after-reloc` | sweep | before the §4.5.3 sweep | by the sweep | by the scan or the N47 weld | reach the update |
|---|---|---|---|---|---|
| `same-curve` | 880 | 250 | 5 (+7 in runs that stopped there) | 597 | 21 |
| `curve-end` | 121 | 0 | 1 | 105 | 15 |
| `fig11b` | 179 | 0 | 78 | 101 | 0 |
| `near-curve` | 11 | 0 | 0 | 11 | 0 |
| `different-faces` | 6 | 0 | 0 | 3 | 3 |

- **§4.3.3's removal is made in three places before the update:** the passes
  ahead of the sweep (the PR-KV9 junction-twin collapse, the moved × minted
  weld, the beyond-corner trim — 15 % of the same-curve pairs on the corpus,
  28 % over the sweep), the N55 scan and the N47 weld behind it, and the
  sub-τ collapse after the update for the pairs that reach it.
- **The §4.5.3 sweep takes a third of the Fig. 11(b) pairs on the corpus and
  44 % over the sweep, and almost no other kind.** An operand vertex and the
  relocated intersection point within roundoff of it have no order along
  their curve, and the sweep reads the pair as a reversal and removes a
  point. That is Fig. 11(b)'s merge made by §4.5.3's rule.
- Pairs also APPEAR after `after-reloc`: 8 `fig11b` and 8 `same-curve` on the
  corpus are first seen at `before-3c` (41 and 24 over the sweep) — a
  removal by the sweep makes two points neighbours that were not.

For the sub-step split (N81): the classes exist as soon as the relocation is
done, the sweep's reading of a same-point pair as a reversal is a reading of
roundoff, and the paper's order puts the removal first. Forming the classes
at `after-reloc` and letting the sweep read each class as one point is the
change to measure; it is its own increment (§8.4 item 4) and it moves which
pass takes 12 corpus pairs and 78 over the sweep, so it is not result-neutral
by construction.

### 8.6 Increment 2a: the classes merged in the scan's place (2026-11-26)

**What landed.** The N55 scan is deleted. At its site, before the update,
`fig11b_vertex_classes` forms the same-point classes and
`collapse::merge_vertex_classes_with_bounds` rewrites every victim onto its
class's survivor in one step: triangles that lose an edge are dropped, and
the opposite-winding pairs the rewrite leaves on one vertex triple are
cancelled once, after every class is applied (within one side label when the
arrangement is whole). A merge can make two survivors neighbours, so the
classes are formed again until none is left; each round removes a vertex.
The survivor is the class's (`reversal::merge_rank`, then position, then
index), so neither the partition nor the result depends on an order of
pairs. The scan's sub-`MIN_FEATURE_SIZE²` triangle test, its `moved` set and
its per-merge membrane cancellation are gone with it.

**Why two steps.** §3.3's form — the classes mapped through the cycles and
`want_bd` of every patch that holds a victim, each such patch rebuilt by the
CDT, a refusal loud — needs the update to accept those patches. Today it
refuses 1,281 of 16,368 patch updates on the d(T) budget (N106), and a
refused patch keeps its old triangles. Under 2b a class touching a refused
patch would stop the build, and a d(T) refusal is decided by rounding on
gear cone patches (N106). So 2a removes the ORDER (the property F0060 and
the 2026-11-25 trial failed on) with the rewrite the scan already used, and
2b replaces the rewrite when N106 is answered (F5b). What stays unfaithful
after 2a is recorded in N55: the merge is an index rewrite ahead of the CDT,
and a survivor keeps its own face record instead of the union of its
class's.

**Measured** (release, 900 s; baselines of commit `1951452d`):

| | before | after |
|---|---|---|
| corpus | 360C / 0W / 9E / 5EE / 0T | the same; `results.json` moves in one detail string (R0099, on-surface figures at 2e-15) |
| merges at the site, corpus | 294 (245 / 14 / 33 / 2) | 296 (247 `same-curve` / 14 `curve-end` / 33 `fig11b` / 2 `near-curve`) — the two more are F0060's pinch-line ends |
| classes reaching the update, sweep | 38 | 0 |
| sub-τ collapse, sweep | 46 pairs in 10 builds | 5 pairs in 3 builds, all at its Stage-5 site |
| sweep, 1,870 lines | 1,378 OK / 107 ERR | 1,379 OK / 106 ERR |
| seed 6 | 189C / 9E / 2T | the same, no case moved |

- **F0060** is CORRECT with each pinch-line end one point per sheet, read by
  the harness's sheet reading (§8.3). F0058, P0041, R0032 and C0056 are
  CORRECT.
- **Sweep, read line by line.** 41 lines differ. 39 differ in the mesh
  hash, the mesh volume's sixth digit or the distance to the reference
  only (C0044, C0047, C0064, F0056, F0057, F0059 m0/m1, F0060, P0005,
  P0017, P0027, P0037, P0041, P0059, R0050, R0099): bodies, χ and the exact
  volume are identical. The class's survivor is the other vertex of the pair
  in 81 builds, and seven builds gain the merges the scan left to the sub-τ
  collapse or to no pass (C0043 m3, F0055 m2, F0059 m0/m1, P0041 m0, R0032
  m0/m1). P0041 m2 now finishes on the first §4.5.2 run: the scan left one
  pair there, and the loop refined.
- **F0059 m2: ERROR → OK**, at the reference's exact volume.
- **F0059 m3: ERROR → ERROR, another refusal.** Before, the scan left
  three pairs, the run failed two `IllegalIntersection` sites and the loop
  ended unresolved. Now all 13 classes merge, the first run validates, and
  kernel-v2's render gate refuses the result
  (`TessellationFailed … "patch triangle collapsed at render precision"`).
  The output holds, at each of the two points where F0059's two ellipses
  CROSS (equal cylinders, tangent there), two vertices 4.5e-16 and 4.3e-16
  apart, each on two ellipse edges (`KV2_OUT_TWIN_PROBE`,
  `KV2_SUBRES_PROBE`): the two branches pass the point as two touching
  curves instead of crossing at one vertex. No census site holds either
  pair in a class and the sub-τ collapse does not take them, so no mesh
  edge joins them: they are two intersection points of two different
  curves at one position, which neither §4.3.3's same-loop removal nor
  Fig. 11(b) names. The other four builds of F0059 hold one vertex there.
  This is the singular point of a tangent crossing — a customer of the
  degenerate-contact matrix (`docs/kernel_1_0.md` gate 3.7), loud today.

Gates: `cargo test -p yang-rs` 1,528 passed; clippy and fmt clean; the
rewrite tier; CI's always-on harness 683 passed, none failed.

### 8.7 Increment 4 tried on the patch-loop walk (2026-11-26)

**The trial** (one binary, switch `YANG_FIG11B_EARLY`; kept in `git stash`,
"experiment 2026-11-26", not in the tree). The merge of §8.6 also runs at
`after-reloc`, ahead of the junction-twin collapse, the moved × minted weld,
the beyond-corner trim and the §4.5.3 sweep; the merge before the update
stays for the pairs that appear later. A Stage-1 minted junction outranks
every other member as survivor (its bits are its identity across the two
operands, N54).

**Measured against §8.6's state** (`f9da3ccc`):

| | switch off | switch on |
|---|---|---|
| `cargo test -p yang-rs` | 1,528 passed | 1,528 passed |
| corpus | 360C / 0W / 9E / 5EE / 0T | 359C / 0W / 10E / 5EE / 0T — **P0053 CORRECT → ERROR** (`NonManifoldOutput`); P0055 one detail string |
| sweep, 1,870 lines | 1,379 OK / 106 ERR / 11 DIFF | 1,375 / 109 / 12 — P0053's reference and three rotations fail, its fourth is compared with a failed reference; 7 other lines (F0059, P0005, P0055, R0050) differ in mesh hash only |
| merges, corpus | 296 before the update | 385 after the relocation, 18 before the update (P0017, P0020, P0059, R0032, P0055) |
| merges, sweep | 846 | 1,334 and 79 |
| classes at `before-453-sweep` | 284 corpus, 878 sweep | 3 and 19 (made by the passes between) |

So the sweep no longer reads a same-point pair as a reversal, and one case
moves.

**P0053, read.** A gear flank of coplanar faces (A287…A294) cut by a
cylinder (B2). Its natural run fails with the switch off too: the
patch-boundary walk ahead of the sweep stops at vertex 913
(`s6-wedge-walk-not-outgoing`; 913 is a member of a same-point pair at
distance 0), the failure names (B, 2), §4.5.2 refines the cylinder and then
every face, and the second refined run is adopted. With the switch on that
pair is merged and the walk passes. The sweep then removes three points as
reversals; removing 899 and reconnecting to 898 turns the triangle
(899, 900, 890) on A288 — all three vertices on the one intersection curve
— into (898, 900, 890), with 898 lying between 890 and 900 along the curve:
it now lies folded over its neighbour (301, 890, 898), and both carry the
directed edge 890 → 898. The sweep of that date re-derived its loops from
every patch's boundary cycles after each removal (since §8.9 it walks
chains and no patch loop); that walk stops
(`s6-boundary-walk-no-start: vertex 890`) and names (A, 288) alone. A288 is a plane, refining it changes nothing (the same 14 classes at
each of the four density rungs), and the loop ends unresolved. The
boundary-domain certificate names nothing in the build.

**Reading.** The fold is one the §4.4.1 update rebuilds from the patch's
boundary. The STOP came from the sweep's reading of its curves at that
date: the paper walks each intersection curve C (§4.5.3 `:679-689`), and
the sweep read C out of the boundary cycles of every patch, so a patch that
was mid-repair stopped it, and the STOP named only that patch's face. P0053
passed before by accident of which walk stopped first (the earlier STOP
happened to name a curved face). The answer is §8.9: the sweep walks chains
and leaves patch loops to the update. On that walk increment 4 holds every
case and is landed (§8.10).

### 8.8 The sweep's curves read as chains of intersection edges: census (2026-11-26)

**The reading** (`stage4_chains::IntersectionEdges`, print-only beside the
sweep; `YANG_CHAIN_CENSUS=<file>`). An intersection edge is a mesh edge with
a triangle of each operand on it. A curve C is the chain of such edges one
face of A and one face of B share; it ends where that pair has one edge at a
vertex, or more than two. The reading uses the triangles' face attribution
only: no patch, no boundary walk, no surface type. An edge of one operand
lying in a face of the other is an edge of two curves.

**Measured** (kernel of `f9da3ccc` plus the census; corpus `results.json`
and all 1,870 sweep lines identical to §8.6's):

| | corpus | rotation sweep |
|---|---|---|
| cases that reach the sweep / sweep calls | 179 / 564 | 180 (882 lines) / 2,184 |
| intersection edges in the sweep's loops / in the chains | 117,628 / 114,644 | 461,195 / 447,002 |
| edges only the loops hold / only the chains hold | 3,720 / 736, in 15 cases (24 calls) | 17,572 / 3,379, in 17 cases (72 calls) |
| removals | 2,355 | 11,914 |
| — at an interior point of a chain, joined along a chain edge | 2,272 | 11,500 |
| — the two overtake arms, on an edge of one operand | 83, in 18 cases | 383, in 26 cases |
| — a §4.5.3 test on an edge of one operand | 0 | 31 (C0043, two rotations) |
| reversals the 2·d_ε gate leaves, all at interior points of a chain | 48 | 220 |
| at the fixed point, positions the sweep reads as points between C's endpoints: chain interior / off the chains / at a branch | 175,378 / 5,888 / 0 | 661,400 / 27,803 / 2 (R0070) |
| positions the sweep reads as endpoints and the chains as interior | 63, in 7 cases | 1,206, in 21 cases |

**The disagreements, read** (census detail lines on the 15 corpus cases and
C0043; each names the surfaces Phase A carries on the edge and the face of
every triangle on it).

- *Edges only the loops hold* (13 cases): both triangles on the edge belong
  to B — the edge is a crease of B — and Phase A names it A × B. One of B's
  two faces there is a plane that `merge_same_plane_patches` (PR-YR27) has
  merged with A's coplanar face, and the merged patch carries A's name. The
  5,888 off-chain positions and C0043's 31 removals are on these edges.
- *Edges only the chains hold*: one triangle of A and one of B, and no
  Phase-A incidence at all (13 cases) — the seam inside such a merged plane,
  which the merge makes patch-interior; or Phase A names one operand alone
  (P0029, R0015); or two triangles of each operand with nothing dropped
  between them (P0020, one edge) — the faces touch along the edge without
  crossing.
- *Endpoints*: where the sweep ends C and the chain runs through, the vertex
  is a Stage-1 mint or carries a third surface (`curve_endpoint_ranked`).

So on every case with no coplanar merge the two readings hold the same
edges, and every §4.5.3 removal is at an interior point of a chain.

**What the sweep took from this (§8.9).** It does not walk the face-pair
chains. It walks chains of patch-boundary edges, which keep four things the
face-pair chains do not hold:

1. *The overtake arms* (matrix 4.5.3-12) act on an edge of ONE operand — a
   relocated junction that passed points of that operand's own boundary
   curve (18 corpus cases). Those edges bound a patch and are chained too.
2. *Endpoints*: a mint or a third surface ends C though the face pair does
   not change; the endpoint rule (`curve_endpoint_ranked`) runs on the
   chains unchanged.
3. *Incidence and curves*: the surfaces and the typed curve of each edge are
   Phase A's.
4. *The coplanar merge*: a merged plane is one patch, as in Phase A.

The face-pair chains stay as the census reading.

### 8.9 The sweep walks chains (landed 2026-11-28)

**What the sweep reads** (`stage4_correct/reversal.rs::curve_chains`). The
patches are Phase A's — the same flood fill and coplanar merge — and so are
the edge incidence and `build_intersection_curves`, so the merged planes of
§8.8 and the overtake arms keep what they had. An edge is on a patch's
boundary when the patch's triangles along it do not cancel in direction (one
triangle, or a fold's two on one side). A CHAIN is the run of boundary edges
one set of patches shares, between the vertices where that set has one edge
or more than two (`stage4_chains::chains_by`); a run that returns to its
start is a closed ring. A chain is read when one of its patches has an
intersection edge somewhere on its boundary, which brings in the operand's
own edges beside each curve, where the two overtake arms act. The chains are
read again after every removal. No patch boundary is walked as a loop; the
`compute_phase_a` call is gone from the sweep, on the kept mesh and on the
full arrangement (whose overlap-sheet incidence is
`reduce_overlap_incidence`, shared with Phase A).

**The test is per site.** A site whose two edges are one planar conic
(`geom::curve_plane`, no curve-variant list) or one straight run takes
Fig. 15's angle band. Before, a conic site took the band only on a patch
loop that was all conic and exact parameter order on any other loop, so one
site could take both. Parameter order stays on a conic stored with opposite
normals on the two edges and on surface-pair curves; the progression sign
stays on untyped pair chains (matrix 4.5.3-07).

**The removals, read call by call.** `YANG_CHAIN_CENSUS` prints the mesh's
identity per sweep call, so one call is compared across the two builds. On
the corpus (179 cases reach the sweep; 568 sweep calls):

- 138 cases make the same removals in the same order. 31 make the same
  removals in another order, or under the band where parameter order made
  them before.
- 5 cases make the same number at other points (P0029, P0044, R0016, R0050,
  R0070): the chain is walked from its other end, and the paper's "remove
  its next point" then takes the points on the other side of the reversal
  (R0070: a run of ten collapses onto its far end instead of its near one).
  The paper fixes no direction for the walk along C; an open chain is walked
  from its lowest-numbered end.
- 5 cases make more (P0020 15 → 73, P0053 5 → 28, R0061 2 → 20, P0055 3 → 6,
  P0007 4 → 8). In P0020, P0053, R0061 and P0055 the extra removals are all
  in one sweep call each, on a mesh the patch-loop walk refused outright
  (`NonManifoldOutput` from `patch_boundary_cycle`, before any point was
  tested): 58, 23, 18 and 3 reversed points the sweep now corrects. In the
  first three the build still fails later and §4.5.2 refines to the same
  next mesh as before; in P0055 the builds after it differ. P0007 is the
  walk direction (one curve entered from its other end), after which its
  builds differ.

**Measured** (kernel of this commit against the one before):

| | before | now |
|---|---|---|
| `cargo test -p yang-rs` | 1,537 passed | 1,544 passed (7 new) |
| corpus | 360C / 0W / 9E / 5EE / 0T | the same; three detail strings (F0064's stop lists 2 unresolved reversals for 4; P0007, P0055) |
| rotation sweep, 1,870 lines | 1,379 OK / 106 ERR / 11 DIFF | 1,383 / 102 / 11 — C0058 under one rotation and P0020 under three go ERR → OK; 11 lines differ in hash, mesh volume or deviation only (C0058 ×3, P0007 ×5, P0055 ×2, R0026); none is worse |
| test removals, corpus | 2,272: 50 by the angle band, 2,219 by parameter order, 3 by progression | 2,376: 812 by the angle band (613 on ellipses, 124 hyperbolas, 66 circles, 9 straight runs), 1,561 by parameter order — every one on a surface-pair curve — and 3 by progression |
| test removals, rotation sweep | 11,531: 185 by the angle band, 11,324 by parameter order, 22 by progression | 11,899: 6,024 by the angle band (4,664 on ellipses, 1,112 hyperbolas, 225 circles, 23 straight runs), 5,853 by parameter order — every one on a surface-pair curve — and 22 by progression |
| overtake removals | 83 corpus, 383 sweep | 85 (P0007, P0055) and 394 |
| curve points walked at the fixed points, corpus | 175,378 | 96,779: a point is walked once, not once per patch loop through it |
| prospector seed 6, 200 documents | 189C / 9E / 2T | the same, no document moves |

Every test removal is at an interior point of a face-pair chain of §8.8:
2,376 of 2,376 on the corpus, 11,868 of 11,899 over the rotation sweep (the
other 31 on C0043 under two rotations, as before).

**Pins.** `tests_unit/s453_curve_chains.rs`: a curve is one chain from end to
end; a patch whose boundary is not a loop is read like any other, and the
sweep corrects a reversal on it; a run that returns to its start is a closed
ring; `reduce_overlap_incidence`.

**Open here.** The walk direction of a chain (above). A conic stored with
opposite normals on a site's two edges still takes parameter order (no
corpus removal). The early merge of §8.7, whose P0053 stop was the
patch-loop walk, is the next step.

### 8.10 Increment 4: the classes merged right after the relocation (landed 2026-11-28)

**What runs** (`stage4_correct.rs::merge_same_point_classes`). §4.3.3 removes
too-close points "during optimization" (`:535-536`). The merge of §8.6 runs
as soon as the relocation is done — ahead of the junction-twin collapse, the
moved × minted weld, the beyond-corner trim and the §4.5.3 sweep — so the
sweep meets each class as one point and never as a reversal. It runs a
second time where it ran before, ahead of the update, for the classes the
passes in between leave: a collapse can join two points of one class by an
edge. A Stage-1 minted junction outranks every other member as survivor
(closed form, never moved; its bits are its identity across the two
operands, N54).

**Measured** (kernel of this commit against §8.9's):

| | §8.9 | now |
|---|---|---|
| `cargo test -p yang-rs` | 1,544 passed | 1,545 passed (1 new) |
| corpus | 360C / 0W / 9E / 5EE / 0T | the same; `results.json` byte-identical |
| rotation sweep, 1,870 lines | 1,383 OK / 102 ERR / 11 DIFF | the same, no line changes status; 6 differ in mesh hash only (F0059, P0005, P0020 ×3, R0050) |
| merges | 296 ahead of the update on the corpus, 846 over the sweep (measured at §8.6) | 334 after the relocation and 33 ahead of the update; 1,085 and 167 over the sweep |
| classes at `before-453-sweep` | 284 corpus, 878 sweep (§8.6) | 3 (P0017, P0055, R0061 — formed by the passes in between) and 19 |
| §4.5.3 removals | corpus 2,376 by the tests, 85 by the overtake arms; sweep 11,899 and 394 | corpus 2,375 and 71 (fewer in P0005, P0007, P0017, P0018, P0028, P0037, P0055, P0059; one or two more in P0020, R0061); sweep 11,836 and 306 |
| prospector seed 6 | 189C / 9E / 2T | the same, no document moves |

P0053, the case that stopped in §8.7, is CORRECT.

**What the passes ahead of the sweep still take** (`pass-took` lines of
`YANG_FIG11B_CENSUS`, corpus): the junction-twin collapse 8 vertices in 3 cases (F0058 2, F0059 4, F0060 2), the moved × minted weld 29 in 7 (P0005, P0007, P0017, P0020, P0053, P0055, R0061), the beyond-corner trim 464 in 14; over the rotation sweep 30, 147 and 2,693. A trial with the junction-twin
collapse and the moved × minted weld both left out (one binary, switches
since removed) failed `c0058_authored_geometry_union_mints_its_tangent_points`
and turned F0058 into a loud `NonManifold` ERROR; every other corpus case
held. So the junction-twin collapse keeps customers the classes do not
cover — exact twins of one junction in the tangent-crossing family (F0058,
F0059, F0060, C0058) — and the weld's band is wider than a class
(`TAU_MODEL·(1 + scale)`); neither is retired here. §8.11 reads the two apart.

**Open.** The two passes above; the second run of the merge (it takes what
later collapses join); the sweep's own removals of a Fig. 11(b) pair, now
only where a pass in between forms one.

### 8.11 The passes between the merge and the sweep, read apart; the junction-twin collapse retired (landed 2026-11-28)

**Method.** The junction-twin collapse and the moved × minted weld were
each switched off in one release binary (temporary switches, deleted), and
every pair a weld takes was printed (`weld-pair`: distance, one point
within roundoff or not, a mesh edge between the two or not), with the
vertex copies the Stage-4 entry split makes (`entry-pinch-split`).

| switched off | `cargo test -p yang-rs` | corpus | rotation sweep |
|---|---|---|---|
| the junction-twin collapse | 1,545 passed | F0058 → ERROR (kernel-v2 `NonManifoldVertex` at the pinch) | F0058: its reference and all four motions |
| the moved × minted weld | `c0058_authored_geometry_union_mints_its_tangent_points` fails | byte-identical | C0058: all four motions → ERR |
| the junction-twin collapse, with the entry split kept to line pinches | 1,545 passed | byte-identical | (the landed code, below) |

**The junction-twin collapse had two populations, and neither needs it.**

- *Point-pinch copies of the entry split* (F0058 2, F0060 2). The Stage-4
  entry split exists for the LINE pinch (spec `yang_tangency_pinch_split.md`
  §0a), but it also separated every vertex whose star is two closed fans.
  The junction arm relocated both copies to one point, the collapse made
  them one vertex again, and the (4a2) split separated them a second time.
  With the collapse off and nothing else changed the entry copies reach
  kernel-v2 as one non-manifold vertex (F0058). The entry call now splits
  a vertex only when one of its edges holds four triangles, so a point
  pinch is one vertex — one point of both curves through it — until (4a2).
  F0060's entry split makes 10 copies for 14.
- *Exact twins no mesh edge joins* (F0059, two pairs a run). The weld after
  the sweep takes them (`weld-pair`, d = 0).

**The moved × minted weld stays, and its customer is not its band.** Its 28
corpus pairs are two populations:

| pairs the weld takes (corpus) | ahead of the sweep | after the sweep |
|---|---|---|
| one point within roundoff, no mesh edge between them | 22 (17 onto a minted junction): P0005, P0020, P0053, P0055, R0061 | 8: F0059, P0061, P0062, R0050 |
| 1e-9 to 1e-7 apart, a mesh edge between them | 5, and 1 more with no edge: P0007, P0017 | 39: P0007, P0017, P0048, R0019, R0061, R0085 |

No pair is one point AND joined by an edge: the classes take all of those.
C0058 is the first row. At each of its two tangent points the relocation
puts a second vertex on the minted one (2.3e-16 and 2.1e-16 away) with no
mesh edge between them; left there, a removal of the sweep joins the two by
an edge, and that edge cannot be typed (`AmbiguousCurve`: both ellipses pass
through its one point).

**What this says about the classes.** §4.3.3 removes a point "too close to
another point on the same loop" (`:535-536`). The classes read the two ends
of a mesh edge (§3.1 item 1, Fig. 11(b)'s "endpoint p of the split edge"),
which is narrower: two points of one loop at one position with other points
of the loop between them are not read. That is the weld's first row, and it
is why the weld cannot retire into the classes as they stand.

**Landed** (kernel of this commit against §8.10's): the collapse deleted,
the entry call restricted.

| | §8.10 | now |
|---|---|---|
| `cargo test -p yang-rs` | 1,545 passed | 1,546 passed (1 new) |
| corpus | 360C / 0W / 9E / 5EE / 0T | the same; `results.json` byte-identical |
| rotation sweep, 1,870 lines | 1,383 OK / 102 ERR / 11 DIFF | 1,383 OK / 103 ERR / 10 DIFF: one line moves, F0060 under motion 2, from a quiet three bodies for four to a loud `LocalRefinementRequired` — what its other three motions already report |
| prospector seed 6 | 189C / 9E / 2T | the same; no document changes category or signature |

F0060 under motion 2 is the line pinch moved rigidly. Rotated, the pinch is
not handed over as §0a's chain of four-triangle edges: the entry call made
4 copies there, 3 of them at vertices with no four-triangle edge (of its
two such edges one carries the certificate and one fails it), and the build
came out as three
bodies, one cap a whole disc (`docs/yang_tail_triage.md`, 2026-11-15). With
the entry call kept to line pinches it makes 1 copy and the sweep does not
settle.
The ruled answer is four bodies (§8.3), so the quiet line was a wrong count
and the loud one names the open customer: a line pinch that is not handed
over as an exact chain (kernel 1.0 gate 3.7, tangency along a curve).

**Open.** The first row read by the paper's own statement: same point, same
loop, any distance apart along it (a census of those pairs by chain comes
first). The second row is N47's band and stays with the weld after the
sweep.

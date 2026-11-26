# F2 — the Fig. 11(b) merge as input of the §4.4.1 update's CDT

Status: increment 0 DONE, increment 1 landed with it (2026-11-25); §7 experiment: an order-free rewrite does NOT hold F0060; §8 (2026-11-26): the classes sorted by paper statement, and F0060 read — it passes today on two vertices an ulp apart at the ends of its pinch lines. Increment 2 waits on the owner's ruling for a line pinch (§8.3). Roadmap: `docs/yang_faithfulness_roadmap.md`
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

Two passes stand in for Fig. 11(b), both as `collapse_vertex` index rewrites
made ahead of, or after, the update's CDT:

| pass | site | domain | order |
|---|---|---|---|
| N55 §4.4.1(b) merge | `stage4_correct.rs` "(3c)" block, before `update_patches_ex_with_bounds` | shortest edge of a sub-`MIN_FEATURE_SIZE²` triangle touching a `moved` or conic-endpoint vertex | first degenerate triangle in `mesh.tris` order, one merge per scan |
| #194 sub-τ collapse | `stage4_correct.rs` before the §4.4.3 gate; `stage5_topology.rs` after KV15b | any mesh edge | `BTreeSet<(u32,u32)>` order |

Measured 2026-11-25 (N55 amendment, N89 item 1):

- Every N55 merge has an intersection point at one end (294 on the corpus,
  818 over the sweep's 1,870 builds).
- The sub-τ collapse acts on four populations: (1) two intersection points an
  ulp apart at Stage-4 entry (F0059 m0/m1, F0055, P0041); (2) two rim points
  that the boundary-curve relocation brings together after the update
  (R0015 m1); (3) a rim vertex beside a vertex Stage 0 left inside a face
  (C0043, harmless); (4) two points of ONE operand's mesh
  (`pocket_operand_reenters_plain_boolean`). Population (4) is a Stage-1
  conformity defect, moved to F5b, and is out of this spec.
- Taking the union of the N55 pairs and population (1) in index order makes
  F0060 WRONG and changes six corpus bodies, because the order of merges
  decides the result at a pinch.

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

- The N55 scan (the "(3c)" block, `stage4_correct.rs`) is DELETED in the
  same commit the merge is wired; N55 moves to RESOLVED-pending-owner with
  the paper cite.
- Population (1) leaves the Stage-4 sub-τ collapse. That pass stays (with
  its Stage-5 site) for population (4) until F5b, and its census must show
  it acting only on (4) and (3).
- Population (2) is NOT answered here unless increment 0 shows that its
  relocation runs before the update. If it runs after the update, it belongs
  to the roadmap's next item (split `stage4_relocate_and_correct_inner` into
  relocate → §4.3 dedup → §4.5.3 → update → validate), and it stays with the
  sub-τ collapse until then.

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
2. **Wire into the update and delete the N55 scan, in one commit.** Gates,
   in order:
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
3. **Re-census the sub-τ collapse.** After increment 2, confirm that it acts
   only on populations (3) and (4), plus (2) if increment 0 placed it after
   the update. Record that in N89 item 1.

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

### 8.4 Increments, restated

2. (unchanged in mechanism, §3.3–§4) Wire the classes into the update and
   delete the N55 scan in one commit — after §8.3 is ruled. The survivor is
   left on the union of its class's faces. F0060 and F0058 are read
   individually, and F0060's expected verdict is whatever the ruling makes
   it.
3. Re-census the sub-τ collapse (unchanged).
4. New, with the roadmap's sub-step split: measure whether the §4.5.3 sweep
   removes or reorders members of a class, and move the §4.3.3 kinds ahead of
   it if it does (N81).

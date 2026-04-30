# Tessellation Bounded-Path Residual Non-Bijectivity (PR3 investigation)

Investigation note from PR3 test-author. Purpose: document the **failed
hypothesis** that drove the original PR3 plan, the empirical evidence that
falsified it, and a revised diagnosis to anchor PR4.

This note exists because anchoring a fix without an empirical RED signal
violates `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §2 ("tests must fail
before the fix"). PR3 is being rescoped from "fix the dedup bug" to
"document what we now know is wrong with the dedup hypothesis and reset for
PR4".

## 1. The original PR3 hypothesis (now falsified)

From the Phase-1 explorer's report:

> `discretize_edges` (`crates/kernel/src/tessellation/mod.rs:3135+`) doesn't
> dedup B-Rep vertices: when edges E1 and E2 share an arena `VertexIdx` V,
> V's position is pushed to `disc.positions` TWICE at different indices.
> Faces touching E1 vs E2 then get different vertex IDs for the same
> position → oracle reports non-bijective.

The proposed fix was a `BTreeMap<VertexIdx, usize>` cache in
`discretize_edges` that emits each B-Rep vertex's position once and reuses
the pool index across edges. Implementer task T2 was scheduled to
implement this; adversary T3 to measure corpus impact.

## 2. Why the hypothesis fails

The bijective oracle (`crates/kernel/src/tessellation/bijective.rs:209-225`)
keys directed mesh edges on the **emitted f32→f64-cast position bit
pattern**, not on pool indices:

```rust
fn pos_key(p: [f64; 3]) -> [u64; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}
```

The `Linear` branch of `discretize_edges` (mod.rs:3197-3206) reads:

```rust
let p0 = arena.vertices[origin_v.0].position;
positions.push(p0);
```

Two edges sharing arena vertex V both read `arena.vertices[V].position` —
**byte-identical f64**. They land at different pool indices, but both
indices reference byte-identical f64 values. `tessellate_planar_face_bounded`
(mod.rs:3329-3335) then casts these byte-identical f64 to f32, producing
byte-identical f32 in the rendermesh. Oracle keys collide → matched →
bijective.

PR1's `test_cube_is_bijective` is GREEN for exactly this reason — the cube
exercises the supposed dedup gap in 8 corners (each shared by 3 edges) and
12 edges, yet the oracle reports 12/12 face pairs bijective.

The proposed dedup fix changes pool indices but not f32 positions. It
cannot change what the oracle measures.

## 3. Empirical evidence

Test-author wrote 12 fixtures, all routed through the bounded path
(`is_polygon_soup=false`, no arc edges, all primitive_params=None). All
tessellated successfully and produced 0 non-bijective pairs:

| # | Fixture | total_pairs | nb_pairs |
|---|---|---|---|
| 1 | Two cubes stacked (vertical union, F0001-style) | 20 | 0 |
| 2 | Two cubes lateral union (full-face contact) | 20 | 0 |
| 3 | Two cubes overlap union (50% overlap) | 20 | 0 |
| 4 | Asymmetric lateral (1×1×2 + 1×1×1) | 23 | 0 |
| 5 | Partial-overlap contact (1×2×1 + 1×1×1) | 26 | 0 |
| 6 | L-shape 3-box union | 28 | 0 |
| 7 | Plus-shape 5-box union | 44 | 0 |
| 8 | T-junction-inducing union (3×1×1 + 1×1×1 mid-top) | 26 | 0 |
| 9 | Box minus box intersect (went polygon_soup, fan) | 3 | 0 |
| 10 | Box with rectangular pocket (polygon_soup, fan) | 32 | 0 |
| 11 | Hex extrude | 18 | 0 |
| 12 | Hex U box union | 55 | 0 |

These were chosen to stress the suspected mechanism: shared corners,
T-junction-style boundaries, partial-overlap contacts, multi-face
adjacencies. None reproduced the 1.55% pair-non-bijectivity rate that the
post-PR2 corpus measurement (commit `c4f0fcb`) reports for the
linear-bounded class.

## 4. Revised diagnosis (to be empirically validated by PR4)

The corpus measurement commit `c4f0fcb` reports 350 nb pairs in the
linear-bounded class (1.55% of ~22k pairs across 13 of 99 cases). The
hand-built fixtures don't reproduce this rate, suggesting the residual
mechanism is sensitive to either:

- Specific assay-randomized coordinate values (non-canonical-axis
  geometry, large or small scales, oblique planes from `master_seed=42`)
- Compound-feature outputs (revolve+extrude+boolean chains) where the
  final boolean produces a B-Rep with **B-Rep T-junctions** — face A's
  outer_loop walks N edges along a shared boundary while face B's loop
  walks N−1 edges (one face has an extra mid-edge vertex the adjacent
  face doesn't have)

The collinear-vertex centroid-fan branch in `tessellate_planar_face_bounded`
(mod.rs:3357-3413) is keyed off a comment that says: *"This happens when
Yang coplanar merge keeps intersection-plane vertices on merged face
boundaries."* That comment is the closest existing-code acknowledgment of
the residual mechanism. If face A has a collinear vertex V_mid that face B
does not, A's boundary directed edge `V1→V_mid` has no `V_mid→V1` partner
on B → non-bijective.

This is a topology-level T-junction (B-Rep edges differ between adjacent
faces), NOT a pool-index dedup gap. The fix lives elsewhere — at the
boolean B-Rep assembly stage (`stitch.rs::build_brep_from_polygons_inner`,
or earlier in `analytical.rs::planar_planar_boolean`), not in
`discretize_edges`.

## 5. Constraints encountered

The test author was scoped to "only `bijective.rs` `mod tests`" per the
PR3 brief. Loading actual F-/R-series `.waffle` files requires
`feature-engine` + `test-harness::ModelBuilder::kernel().load(json)`,
which would pull cross-crate deps into kernel-internal tests — out of
scope for PR3. Hand-built fixtures in kernel-only scope cannot reach the
randomized topology+coordinate combinations that produce the residual
1.55% nb rate.

## 6. PR4 anchoring requirements

For PR4 to follow `feedback_yang_only.md` and FIP §2, the test-author
needs:

1. A specific assay case (or hand-built reproducer derived from one) that
   is in the linear-bounded class and produces ≥1 non-bij pair.
2. Oracle output identifying WHICH face pair is non-bij and what its
   unmatched directed edges are.
3. A diagnosis of the topology-level mechanism — likely *"face A's
   outer_loop has K vertices, face B's has K−1, missing midpoint M at
   position P"*, traced back to the B-Rep assembly stage.

Adversary T3 task: dump per-case nb counts on the corpus, identify the
13 linear-bounded cases that produce non-bij pairs, pick one with the
simplest topology, and capture its arena's outer_loop vertex sequences
for the offending face pair.

## 7. References

- `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §2 — red-before-green
- `governance/ENGINEERING_CONSTITUTION.md` P9–P10 — fix it right or don't
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6 — Yang hybrid pipeline
- `docs/audits/cherchi_port_audit.md` D-10 — `weld_mesh_vertices` violation
- `docs/references/yang2025_hybrid_boolean.txt` §4.1.1 — bijective contract
- `specs/tessellation_bounded_gate.md` — PR1 spec characterizing the gate
- PR1 oracle: commit `5f5423c` (`test(kernel/tessellation): bijective oracle + 4 face-pair shared-edge tests (PR1)`)
- PR1 corpus baselines: commits `d2eb72b`, `a445c18`
- PR2 fix: commit `f01dd68` (`fix(kernel/tessellation): share cap-to-lateral boundary vertex IDs in revolve primitive (PR2 v2)`)
- PR2 corpus delta: commit `c4f0fcb`
- `~/.claude/projects/-home-claude-workspace/memory/feedback_yang_only.md` —
  no shortcuts; faithful Yang implementation only
- `~/.claude/projects/-home-claude-workspace/memory/feedback_no_last_bug.md` —
  no claims of "the last gap"

## 8. PR4 empirical R0033 diagnostic dump

Captured stderr from
`crates/test-harness/tests/pr4_r0033_t_junction_diagnosis.rs::diagnose_r0033_t_junction_pattern`.

R0033 = 2-op partial-revolve gear: revolve(rectangle, boss, ~199°) then
revolve(gear, cut, ~74°) on a non-canonical-axis plane (per `R0033.meta.json`).
PR3 corpus dump ranks it 12 face pairs / 2 nb / 16.7% — smallest multi-nb
linear-bounded anchor. Tessellated under `YANG_BOOLEAN=1` at
`tess_tol = scale * 0.01 = 1.978e-4` (matches assay-runner).

### 8.1 First-call dump (canonical, matches PR3 corpus)

```
R0033 scale = 1.977872e-2, tess_tol = 1.977872e-4
LoadProject response variant: Discriminant(0)
engine_errors after load: 0 entries
tessellated mesh: 104 vertices, 252 indices (84 tris), 6 face_ranges
B-Rep arena: 48 vertices, 104 half_edges, 52 edges, 6 loops, 6 faces
bijective oracle: total_pairs_examined = 12, bijective_pairs = 10, non_bijective_pairs = 2

─── non-bijective pair #0 ───
face_a = FaceIdx(2), face_b = FaceIdx(3), edge = Some(EdgeIdx(6))
unmatched_a_count = 4, unmatched_b_count = 4
face_a outer_loop has 24 boundary vertices.
face_b outer_loop has 24 boundary vertices.
T-junction candidates: 12 vertex(es) on face_a but not face_b, 12 on face_b but not face_a
sample_unmatched_a (first 4):
  a-edge[0]: (1.089050e-2, 7.307105e-3, -1.771689e-2) → (8.900722e-3, 9.211298e-3, -1.779127e-2)
  a-edge[1]: (1.404203e-2, 4.291138e-3, -2.670923e-3) → (1.526877e-2, 3.117168e-3, -4.840629e-3)
  a-edge[2]: (1.238955e-2, 5.872539e-3, -1.134956e-3) → (1.404203e-2, 4.291138e-3, -2.670923e-3)
  a-edge[3]: (1.276546e-2, 5.512794e-3, -1.679186e-2) → (1.089050e-2, 7.307105e-3, -1.771689e-2)
sample_unmatched_b (first 4):
  b-edge[0]: (1.276546e-2, 5.512794e-3, -1.679186e-2) → (1.089050e-2, 7.307105e-3, -1.771689e-2)
  b-edge[1]: (1.238955e-2, 5.872539e-3, -1.134956e-3) → (1.404203e-2, 4.291138e-3, -2.670923e-3)
  b-edge[2]: (1.089050e-2, 7.307105e-3, -1.771689e-2) → (8.900722e-3, 9.211298e-3, -1.779127e-2)
  b-edge[3]: (1.404203e-2, 4.291138e-3, -2.670923e-3) → (1.526877e-2, 3.117168e-3, -4.840629e-3)

─── non-bijective pair #1 ───
face_a = FaceIdx(2), face_b = FaceIdx(5), edge = Some(EdgeIdx(7))
unmatched_a_count = 2, unmatched_b_count = 2
face_a outer_loop has 24 boundary vertices.
face_b outer_loop has 24 boundary vertices.
T-junction candidates: 12 vertex(es) on face_a but not face_b, 12 on face_b but not face_a
sample_unmatched_a (first 2):
  a-edge[0]: (2.188456e-2, 8.536718e-3, -1.018362e-2) → (2.181844e-2, 8.599987e-3, -7.430017e-3)
  a-edge[1]: (2.133029e-2, 9.067141e-3, -1.282978e-2) → (2.188456e-2, 8.536718e-3, -1.018362e-2)
sample_unmatched_b (first 2):
  b-edge[0]: (2.133029e-2, 9.067141e-3, -1.282978e-2) → (2.188456e-2, 8.536718e-3, -1.018362e-2)
  b-edge[1]: (2.188456e-2, 8.536718e-3, -1.018362e-2) → (2.181844e-2, 8.599987e-3, -7.430017e-3)
```

(Full per-vertex outer-loop sequences are available by re-running the
test with `--nocapture`. Trimmed here so the spec stays scannable; the
counts and sample edges are sufficient to characterize the mechanism.)

### 8.2 Second-call flap (recorded, not asserted)

The test runs the diagnosis twice in the same process. Across 5
invocations, **first-call always reports `non_bijective_pairs = 2`**
matching the PR3 corpus. **Second-call sometimes reports 3** (an extra
nb pair appears and existing pair `EdgeIdx` values shift). Observed
sequence across runs: `(2, 3), (2, 3), (2, 3), (2, 2), (2, 2)`.

State is fresh between calls (`WaffleKernel::new()` + `EngineState::new()`
each time), so the variance source is iteration-order non-determinism
inside the boolean pipeline (likely Rust's `HashMap` `RandomState`
reseeding between calls within a single thread). This matches the
corpus-dump's note that `R0080`/`R0018` "are nondeterministic and may
flip across runs".

The test asserts on the first-call value only. If PR5 fixes the
underlying topology defect, the flap should disappear too (a watertight
shared-boundary discretization has no per-call iteration sensitivity).

### 8.3 Analysis

Two empirical findings, one consistent and one **diverging from PR3's
T-junction hypothesis**:

**Finding A — boundary-loop vertex-set divergence (consistent with PR3).**
For both nb pairs, face_a's 24-vertex outer loop and face_b's 24-vertex
outer loop **share only 12 vertices** — the loop "corners" — while each
face has 12 *additional* vertices the other doesn't have. This is more
extreme than PR3's "N vs N−1" hypothesis: face A and face B aren't
walking N and N−1 edges along a shared B-Rep edge, they're walking two
*entirely separate* interior subdivisions on the same arena edge. Every
interior subdivision point exists on one face but not the other.

**Finding B — winding-orientation symmetry (NEW, not in PR3 hypothesis).**
The oracle's actual unmatched directed edges (after restrict-to-
shared-boundary) reveal a structural pattern PR3 didn't predict: the
unmatched edges from face_a and face_b are **the same edges in the same
forward direction**, not opposite directions. For pair #1:

```
a-edge[0]: (2.188e-2, 8.5e-3, -1.0e-2) → (2.181e-2, 8.6e-3, -7.4e-3)
b-edge[1]: (2.188e-2, 8.5e-3, -1.0e-2) → (2.181e-2, 8.6e-3, -7.4e-3)  ← SAME forward direction
a-edge[1]: (2.133e-2, 9.0e-3, -1.3e-2) → (2.188e-2, 8.5e-3, -1.0e-2)
b-edge[0]: (2.133e-2, 9.0e-3, -1.3e-2) → (2.188e-2, 8.5e-3, -1.0e-2)  ← SAME forward direction
```

Yang §4.1.1 requires the two faces sharing a B-Rep edge to emit
**reciprocal** directed edges — face A emits (P,Q) and face B emits
(Q,P). Here both emit (P,Q). Either:

1. The two faces are wound such that they are **co-oriented** along
   the shared boundary (both CCW from the same side) — i.e., the
   B-Rep edge's twin half-edges have the same orientation rather than
   opposite. This is a topological winding bug, not a missing-vertex
   bug.
2. OR these directed edges aren't actually on the shared boundary — the
   oracle's `restrict_to_shared_boundary` heuristic (undirected
   coincidence on either face's boundary set) is matching edges that
   are on the *interior* of each face's loop but happen to be position-
   coincident across faces.

Without per-edge B-Rep traversal, finding B is suggestive but not
definitive. Either way, the dominant signal is finding A: the two faces
have **disjoint** interior subdivisions of the shared boundary, which
is a stronger defect than a single missing midpoint.

### 8.4 PR5 anchor (corrected)

> Section 8.4 was rewritten in commit 2 of PR4. The original recommendation
> pointed at boolean B-Rep assembly code; that recommendation was wrong.
> See § 8.5 (Errata) below for the trace of what was wrong and why.

**The R0033 boolean does not actually run.** The diagnostic stderr
contains the line
`[yang-diag] AABB-disjoint short-circuit: skipping Cherchi for Subtract`,
emitted by `crates/kernel/src/boolean/topology_extract.rs:1515` when
the two operands' AABBs are separated by more than `TAU_MODEL`. R0033
is `revolve(rectangle, boss) + revolve(gear, cut)`; the rectangle and
gear revolves land at AABB-disjoint locations on the model's oblique
plane, so the `Subtract` short-circuits to
`yang_pipeline_result_for_disjoint(... op=Subtract ...)` (same file,
line 1361), which returns the first operand A unchanged.

The final solid stored under R0033's last feature ID is therefore the
output of `revolve(rectangle, boss)` — a partial-revolution (199°) of a
rectangle on the case's oblique plane. The arena dimensions
(6 faces / 48 vertices / 52 edges) are consistent with a revolved-rectangle
B-Rep: 2 caps + 4 lateral faces with 24 ring-vertices per swept boundary.
**Yang's Cherchi arrangement and B-Rep stitch never run for R0033.**

**Implication: the 2 nb pairs are produced by the revolve primitive
tessellator**, not by any boolean B-Rep assembly. The fix sites are:

1. **`crates/kernel/src/tessellation/mod.rs::tessellate_revolve_lateral`**
   (~line 1213). Discretizes each lateral face along the swept rectangle
   side. Emits a 24-vertex ring per lateral face for the 199° / oblique-axis
   geometry observed here.

2. **`crates/kernel/src/tessellation/mod.rs::tessellate_revolve_cap_polygon`**
   (~line 1800). Discretizes the two end-caps using a polygon
   triangulation that does NOT consume the same 24 ring-vertices the
   lateral faces emit. The 12 "only_in_a" + 12 "only_in_b" pattern is
   exactly this: lateral has its swept-ring interior verts; cap has its
   polygon-fan interior verts; the two sets are disjoint despite
   sharing the rectangle's 12 corner-equivalent points.

3. **The pool / boundary-sharing logic between lateral and cap**, the
   same site PR2's commit `f01dd68` modified for full-revolution cases
   ("share cap-to-lateral boundary vertex IDs in revolve primitive").
   PR2's fix worked for 360° revolutions where the sweep wraps, but
   R0033 (199°, non-canonical axis) stays nb-positive — indicating
   PR2's pool refactor was incomplete for partial-revolution and/or
   oblique-axis geometry.

PR5 should extend PR2's pool refactor to cover partial revolutions
(angle < 360°) and oblique sketch planes. The two non-bijective edge
indices reported by the oracle (`EdgeIdx(6)` and `EdgeIdx(7)`) are the
arena edges where lateral-cap discretizations diverge.

#### 8.4.1 Gate-class discrepancy (open question, not for PR5)

PR3's corpus dump (`specs/tessellation_pr3_corpus_dump.md`) classifies
R0033 as `linear-bounded` — meaning `is_polygon_soup=false`, no arc
edges, AND no primitive params (the dump's classifier puts any solid
with `revolve_params=Some(_)` into `primitive-dispatch`). But this
finding says R0033's final solid IS a revolve primitive output.

Either the AABB-disjoint short-circuit returns a `WaffleSolid` with
`revolve_params=None` (so the gate-classifier sees no primitive params
and falls through to `linear-bounded`), or the gate-classifier
inspects a different state than the actual stored solid. Worth a
follow-on investigation — but **not** PR5's scope. PR5 fixes the
underlying nb pairs; the gate classification is downstream of that.

### 8.5 Errata

This section records what was wrong in the original PR4 analysis so
future readers can see the trace.

**Original PR4 § 8.4 (commit `7ee4805`):** Recommended PR5 anchor on
`boolean/yang_integration.rs` (Yang Step 9 retessellation),
`boolean/topology_extract.rs` (flood_fill_patches / assemble_brep), or
`boolean/cherchi/` (twin construction).

**Why that was wrong:** R0033 never reaches the Cherchi /
`yang_boolean_pipeline` body. The AABB-disjoint short-circuit at
`topology_extract.rs:1515` fires before subdivision, returning the
first operand unchanged via `yang_pipeline_result_for_disjoint`.
PR4 author missed the short-circuit on initial trace despite the
stderr log line being present in the captured dump (visible at
the top of § 8.1).

**Why PR3's named candidates (`stitch.rs::build_brep_from_polygons_inner`,
`analytical.rs::planar_planar_boolean`) were also wrong:** Those are
S-H-clipping / legacy stack code; per A15.6 they are not on the
YANG_BOOLEAN=1 path at all. Even if R0033's boolean had run via
Cherchi, PR3's candidates would not have been the fix site.

**Confirmed-correct conclusion:** PR5 anchors on the revolve primitive
tessellator (`tessellation/mod.rs::tessellate_revolve_lateral` +
`tessellate_revolve_cap_polygon`) and extends PR2's `f01dd68` pool
refactor to handle partial revolutions and oblique-axis geometry.

**What's preserved unchanged:** § 8.1 (first-call dump), § 8.2 (flap),
§ 8.3 (analysis of vertex-set divergence and winding-orientation
hint). Those measurements are correct regardless of which code path
produced the solid.

This refines PR3's diagnosis: there ARE T-junction-style topology
mismatches between adjacent face boundaries, but the mechanism is
disjoint sibling discretizations from the revolve primitive, not
boolean B-Rep assembly.

## 9. PR5 empirical falsification — actual bug is upstream in `flood_fill_patches`

PR5 was scoped (per PR4 §8.4) to extend PR2's `RevolvePool` to
`tessellate_revolve_cap_polygon` for partial-revolve caps, then
pivoted (per implementer empirical investigation) to fix the per-face
Newell-reverse desync in `tessellate_planar_face_bounded`. **Both
hypotheses are wrong for R0033.** This section documents the trace so
future PRs do not repeat the mistake.

### 9.1 Hypothesis revision lineage

R0033 has been the canonical anchor since PR3's corpus dump. Five
hypotheses have now been proposed and falsified:

1. **PR3 dedup hypothesis.** `discretize_edges` doesn't dedup B-Rep
   vertices — falsified (oracle keys on f32 positions, not pool
   indices; byte-identical f64 produces byte-identical f32 regardless
   of pool placement).
2. **PR4 commit-1 anchor (`7ee4805`).** Fix in
   `boolean/yang_integration.rs` Step 9 retessellation /
   `boolean/topology_extract.rs::flood_fill_patches::assemble_brep` /
   `boolean/cherchi/` twin construction. Falsified by PR4 commit-2
   (`436ed37`): R0033 short-circuits via AABB-disjoint, never reaching
   the Cherchi body.
3. **PR4 commit-2 anchor (`436ed37`).** Solid is unchanged
   `revolve(rectangle, 199°)`; fix in revolve primitive
   tessellator (`tessellate_revolve_lateral` /
   `tessellate_revolve_cap_polygon`). Falsified by PR5 implementer:
   `yang_pipeline_result_for_disjoint` calls `flood_fill_patches`
   which rebuilds the arena; `result_topology_to_waffle_solid` then
   strips `revolve_params` (`yang_integration.rs:243`); R0033 routes
   via `tessellate_solid_bounded` (linear-bounded class), NOT the
   revolve primitive.
4. **PR5 brief (extend cap polygon `RevolvePool`).** Falsified
   immediately: `tessellate_revolve_cap_polygon` is dispatched at
   `mod.rs:476` only `if revolve_params.is_some()`. R0033's last
   solid has `revolve_params: None`, so the cap polygon function is
   never invoked.
5. **PR5 implementer pivot ("option 1": fix Newell-reverse desync in
   `tessellate_planar_face_bounded`).** Implemented and falsified
   empirically (this section).

### 9.2 PR5 option-1 implementation and falsification

The pivot mirrored PR2's `f01dd68` post-fix-normal-flip pattern:
walk arena natural without per-face Newell-reverse; conditionally flip
earcut output triangles when input is CW in the (u, v) basis derived
from `stored_normal`; post-fix flip stored normals when the polygon's
Newell normal disagrees with stored.

`PR5_DEBUG=1` instrumentation around the original
`tessellate_planar_face_bounded` confirmed empirically that for all 6
faces in R0033's post-flood-fill arena:

```
[pr5-dbg] face base=0  n=4  stored=(-0.652,0.624,0.431)  newell_norm=(-0.652,0.624,0.431)  dot_ns=1.0000
[pr5-dbg] face base=4  n=4  stored=(-0.717,0.687,0.118)  newell_norm=(-0.717,0.687,0.118)  dot_ns=1.0000
[pr5-dbg] face base=8  n=24 stored=(-0.694,0.664,0.278)  newell_norm=(-0.694,0.664,0.278)  dot_ns=1.0000
[pr5-dbg] face base=32 n=24 stored=(-0.691,-0.722,-0.000) newell_norm=(-0.691,-0.722,-0.000) dot_ns=1.0000
[pr5-dbg] face base=56 n=24 stored=(0.694,-0.664,-0.278)  newell_norm=(0.694,-0.664,-0.278)  dot_ns=1.0000
[pr5-dbg] face base=80 n=24 stored=(0.691,0.722,0.000)   newell_norm=(0.691,0.722,0.000)   dot_ns=1.0000
```

`dot_ns = 1.0000` for every face means `compute_newell_normal(arena_
natural_loop)` aligns exactly with `stored_normal`. This is forced by
`yang_integration.rs::result_topology_to_waffle_solid` lines 202-225:
each face's `stored_normal` is computed via `compute_newell_normal`
on the arena loop — guaranteeing per-face Newell-stored agreement.

Consequences:
- The original `reverse_outer = dot < 0.0` check at the old
  `mod.rs:3320` **never fires** for R0033.
- `signed_area_2d` in any (u, v, n) right-handed basis is positive →
  `input_is_cw_2d = false` → no earcut flip.
- The polygon-Newell post-fix-normal-flip never triggers (same
  `dot_ns > 0` test).
- The PR5 option-1 patch is a behavioral no-op for R0033. Test
  `pr4_r0033_t_junction_diagnosis` remains RED with `nb_count = 2`
  identical to the pre-PR5 baseline.

### 9.3 Where the bug actually lives

The bijective oracle's first-call dump for the offending pair
(`FaceIdx(2)`, `FaceIdx(3)`, shared `EdgeIdx(7)`) shows unmatched
directed edges where face_a's edge `P → Q` matches face_b's
edge `P → Q` in the SAME forward direction (Finding B from §8.3).

If both adjacent faces emit a shared B-Rep edge as a directed mesh
edge in the same forward 3D direction, their **arena loops both walk
the shared edge in the same 3D direction** — a half-edge twin
convention violation. Twin half-edges in a closed manifold MUST walk
their shared edge in opposite 3D directions; the tessellator
faithfully reproduces this and cannot correct an upstream malformed
arena.

Additional evidence: pair `(FaceIdx(2), FaceIdx(3))` reports 4
unmatched directed edges sharing only 1 B-Rep edge (`EdgeIdx(7)`).
The oracle's `restrict_to_shared_boundary` (`bijective.rs:334`) is
heuristic (undirected position-coincidence). 4 unmatched on 1 shared
edge means three of those edges lie on OTHER position-coincident
boundary segments that the heuristic includes — suggesting
`flood_fill_patches` is producing arena edges that share endpoints
with other arena edges but are not actually B-Rep adjacent.

### 9.4 PR6 anchor

The actual fix site is upstream of tessellation:

1. **`boolean/topology_extract.rs::flood_fill_patches`** (line 351+).
   Steps 5/5a/6 stitch surviving sub-triangles into B-Rep patches and
   build half-edge twin pairs. Investigate twin assignment for cases
   where a directed edge appears in multiple patches with the same
   source-face label (the AABB-disjoint Subtract path passes
   `verts_a, tris_a` and empty B, which may be a degenerate input
   path).
2. **`boolean/topology_extract.rs::yang_pipeline_result_for_disjoint`**
   (line 1361+). The disjoint short-circuit's flood-fill invocation
   may differ behaviorally from the normal `yang_boolean_pipeline`
   path (which runs a full subdivision/intersection cascade before
   flood-fill). A degenerate-but-valid input may not be exercising
   the same code paths in flood-fill.
3. **`boolean/yang_integration.rs::result_topology_to_waffle_solid`**
   (line 165+). Post-flood-fill arena finalization. Check whether
   half-edge twin pointers survive intact here, or whether the
   `face_geometry` reassignment (`compute_newell_normal` per face)
   masks an underlying twin-pair inversion.

PR6 should reproduce R0033 with a kernel-internal fixture that
exercises `flood_fill_patches` directly (without LoadProject), then
fix the twin-pairing bug at the actual source.

### 9.5 What PR5 ships

PR5 ships **documentation only**. The implementer reverted the
option-1 patch after empirically confirming it was a no-op. The
deliverable is:

- This §9 spec amendment.
- An updated docstring on the kernel-internal stub test
  (`bijective.rs::test_bounded_path_brep_t_junction_is_bijective`)
  re-targeting it from PR4/PR5 to PR6.

The PR4 RED diagnostic test
(`crates/test-harness/tests/pr4_r0033_t_junction_diagnosis.rs`)
**stays RED** — that's the canonical anchor for PR6.

This continues PR3's pivot pattern: a non-fix PR that documents what
was learned, preventing the next implementer from re-anchoring on a
falsified hypothesis. Per
`~/.claude/projects/-home-claude-workspace/memory/feedback_no_last_bug.md`:
we don't claim "the last gap"; we ship the empirical lesson.

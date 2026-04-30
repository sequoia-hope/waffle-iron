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

### 8.4 PR5 anchor

Per the data, the most likely fix sites are:

1. **`crates/kernel/src/boolean/yang_integration.rs` — B-Rep retessellation
   step (Step 9)**. The 6-face / 48-vertex / 52-edge arena post-boolean
   already encodes the topology — so the divergent loops emerge during
   per-face retessellation that walks the B-Rep half-edge chain
   independently for each face. If two adjacent faces walk *different*
   sub-edges of the same conceptual boundary, the retessellation can
   produce two disjoint interior chains. Inspect the half-edge twin
   relationships of `EdgeIdx(6)` and `EdgeIdx(7)` (the oracle's
   reported nb-edges) — particularly whether the twin half-edges sit on
   the same two-face boundary or whether they bridge to disjoint
   B-Rep regions due to a stitch-time topology defect.

2. **`crates/kernel/src/boolean/topology_extract.rs` (flood_fill_patches)
   or `assemble_brep`**. If flood-fill is grouping triangles into
   patches such that face_a and face_b end up with non-adjacent
   triangle sets along what *should* be a shared boundary (e.g.,
   per-sub-tri labeling assigns triangles to wrong faces), the
   resulting patches will have parallel-but-disjoint discretizations.

3. **B-Rep edge half-edge twin construction in
   `boolean/cherchi/` arrangement output → `assemble_brep` glue**. The
   "same forward direction" finding (B) is consistent with a twin
   half-edge being constructed on the wrong side of its B-Rep edge
   during stitch — both halves end up oriented the same way relative
   to a reference axis, instead of one being CW and one CCW around
   their respective faces.

PR3's named candidates were `stitch.rs::build_brep_from_polygons_inner`
and `analytical.rs::planar_planar_boolean`. **Neither of these are on
the YANG_BOOLEAN=1 path** — they are S-H-clipping / legacy stack code.
Per A15.6, the YANG path runs through `boolean/yang_integration.rs`
and the Cherchi cherchi modules. PR5 should anchor on the Yang
retessellation/topology-extract layer instead.

This refines (does not refute) PR3's diagnosis: T-junctions exist, but
the mechanism is **disjoint sibling chains**, not single missing
midpoints, and the relevant code path is Yang-side, not S-H side.

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

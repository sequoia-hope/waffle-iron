# PR-CR-AR2b — cherchi-rs arrangement: constraint segments + TPI

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution
on ported files; **reference parity is the correctness oracle**; NO `unsafe`; NO
`panic!` in production — all `Result<>`; single-threaded — port the SERIAL path,
the C++ `tbb` parallelism is a future feature flag; **exact arithmetic** —
`dashu`/existing exact predicates for explicit, the
`indirect-predicates-sidecar-rs` FFI for implicit/LPI/TPI; **predicates are
demand-driven**). Arrangement work stays behind the off-by-default
`indirect-predicates` feature — the DEFAULT crate build must remain FFI-free /
WASM-clean.

This is the **second half of M6 PR-CR-AR2** and the harder one. AR2a (point/edge
insertion) is done: `arrangements::retriangulate` + `aux_structure` build a
per-triangle submesh with every intersection POINT inserted. **AR2b enforces the
intersection SEGMENTS as mesh edges and constructs the deferred TPI points.** See
`docs/yang_functional_roadmap.md` §M6 (PR-CR-AR2b).

## Port source + spec
- **C++ source to port (MIT — attribute):**
  `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp`
  — the constraint-insertion machinery:
  `addConstraintSegmentsInSingleTriangle` (cpp:576), `addConstraintSegment`
  (cpp:597 — the core: edge-already-present → `setEdgeConstr`; else
  `findIntersectingElements` along the segment → at each crossing of an existing
  constraint, `createTPI` → `splitSegmentInSubSegments` → recurse via
  `sub_segs_map`), `createTPI` (cpp:1007 — the three-plane point), and the
  helpers it needs: `computeTriangleOfSegment` (cpp:1041),
  `segmentsIntersectInside` (cpp:1170), `pointInsideSegment` (cpp:1178),
  `splitSegmentInSubSegments` (cpp:1185), `findIntersectingElements`. Read these
  as the spec; reproduce faithfully (do NOT invent mechanism).
- **Paper context:** Cherchi 2020 §5 (constrained triangulation / segment
  insertion) + the cherchi2022 text.

## Build on what `cherchi-rs` already has (do NOT re-port)
- **AR2a** (`arrangements::retriangulate` + `aux_structure`) — the per-triangle
  submesh with points inserted; the intersection SEGMENTS (pairs of AR1 vertices)
  to enforce; the auxiliary structure / `sub_segs_map` analog.
- **CR12c `FastTrimesh`** — `edgeID`, `setEdgeConstr`, `splitTri`/`splitEdge`,
  edge/valence queries (add a thin accessor only if genuinely missing).
- Implicit predicates: `orient2d` (CR-IP6b, AR2a) + `orient3d` (CR-IP6) on
  implicit points (heed the `_II/_IIII`-segfault gotcha → `genericPoint::` static
  dispatch); `lambda3d_tpi_*` (CR-IP4) for `createTPI`.

## Scope (this PR only)
For each base triangle, after AR2a's point insertion, **enforce every
intersection segment** (each pair of the triangle's AR1 intersection vertices
that must be a mesh edge): if the edge already exists, flag it constraint; else
insert it, resolving crossings with existing constraint edges by **constructing a
TPI** (`ImplicitPoint3DTpi` via `lambda3d_tpi`) at each crossing, splitting both
segments into sub-segments, and recursing — exactly as `addConstraintSegment`
does. This **resolves the N13-deferred TPI construction** (the
single-coplanar-edge cases too, if `addConstraintSegment` handles them; if not,
keep their loud `Deferred(..)`). **Also: replace the N13 raw-`f64`
`point_in_segment` guard with the EXACT predicate** — CR1 `points_are_collinear_3d`
+ an exact between-ness check (no raw `f64` in the arrangement core; close that
deviation). **OUT of scope:** the global conforming soup / cross-triangle vertex
welding (AR3), boolean labeling (BL*), the C++ `tbb` parallel path.

## Oracle (full C++ corpus parity is AR3 — assert structure + exactness here)
1. **Constraints realized (load-bearing):** after AR2b, every intersection
   segment is present in the submesh as a chain of **constraint-flagged** mesh
   edges (`setEdgeConstr`), covering the segment end-to-end (no gap, no segment
   crossing a non-vertex edge interior).
2. **TPI exactness:** each constructed TPI point lies on ALL THREE supporting
   planes (the base triangle's + the two crossing segments' source triangles'),
   asserted via exact `orient3d == Zero` (NOT a float tolerance).
3. **Still a valid covering sub-triangulation** (the AR2a invariant holds
   post-constraint: exact `orient2d`, no gaps/overlaps, non-degenerate sub-tris).
4. **Exactness regression:** assert the new exact `point_in_segment` agrees with
   the removed `f64` stopgap on non-degenerate cases AND is correct on a
   collinear-but-just-outside case the `f64` version could misjudge.
5. Small hand-verified cases: two crossing constraint segments → exactly one TPI
   at the crossing on all three planes; a segment coinciding with an existing
   edge → flagged, no new vertex.
Document that cross-triangle parity vs the C++ binary is AR3.

## CI gate
`cargo test -p cherchi-rs` (DEFAULT — FFI-free/WASM-clean, prior unregressed) AND
`cargo test -p cherchi-rs --features indirect-predicates`; `cargo test
-p indirect-predicates-sidecar-rs` if a new IP wrapper is added; `cargo fmt
-p cherchi-rs -- --check`; `cargo clippy -p cherchi-rs --all-targets --features
indirect-predicates -- -D warnings` (and default). No `unsafe`, no `panic!` in
production. MIT attribution header. Single-threaded.

Role-separated FIP: Spec (you) → RED → GREEN → Adversary. If `addConstraintSegment`
recursion or `computeTriangleOfSegment` reveals a dependency on AR3-level
cross-triangle state (or a coplanar sub-case beyond a single triangle), **STOP
and report** — re-scope rather than improvise a Cherchi deviation.

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR2b
done; AR3 = conforming soup next; note the N13 TPI + `point_in_segment`
deviations RESOLVED) + `docs/yang_deviations.md` (move the N13 TPI/`f64`-guard
sub-notes to resolved) + the cherchi-rs `LICENSE-THIRD-PARTY.md`.

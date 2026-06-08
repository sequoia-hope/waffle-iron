# PR-CR-AR3a — cherchi-rs arrangement: constraint-edge enforcement + TPI-at-crossing

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution;
NO `unsafe`; NO `panic!` in production — all `Result<>`; single-threaded — serial
path; **exact arithmetic** — IP FFI for implicit/TPI; **predicates
demand-driven**). Arrangement work stays behind the off-by-default
`indirect-predicates` feature — DEFAULT build must remain FFI-free / WASM-clean.

This is **M6 PR-CR-AR3a** — the constraint-edge enforcement that AR2b Cycle C2
deferred to AR3 (it needs global cross-triangle state). With AR2b done (point
insertion, `ConstraintSegment` grouping, exact `point_in_segment`, the real
`ImplicitPoint3DTpi` handle + dispatch), AR3a builds the enforcement core.

## Parity-oracle note (read — it changes the oracle)
There is **NO standalone C++ arrangement binary** (the 2020 arrangement code is
embedded library-only; only `mesh_booleans`, the full boolean, is built). So AR3a
does **not** diff against a C++ arrangement. **Oracle = structural + EXACT
predicate invariants** (below). Full C++ reference parity engages at **BL3** (the
existing `mesh_booleans` binary transitively validates the arrangement). Do NOT
build a new C++ arrangement-dump sidecar for this PR.

## Port source + spec
- **C++ (MIT — attribute):**
  `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp`
  — `addConstraintSegment` (cpp:597), `createTPI` (cpp:1007),
  `findIntersectingElements`, `computeTriangleOfSegment` (cpp:1041),
  `segmentsIntersectInside` (cpp:1170), `splitSegmentInSubSegments` (cpp:1185).
  And the minimal `triangle_soup.h/.cpp` surface `createTPI`/`computeTriangleOfSegment`
  need (the `genericPoint` vertex/arena accessors — `vert`, `addImplVert`). Read
  as the spec; reproduce faithfully (do NOT invent mechanism).

## Build on (do NOT re-port)
- AR2b: `ConstraintSegment` + `group_constraint_segments`; exact
  `point_in_segment_3d` (collinearity); the real `ImplicitPoint3DTpi` routing +
  `Gp::T` predicate dispatch (Cycle C1).
- AR2a: `retriangulate`/`aux_structure` submesh; CR12c `FastTrimesh`
  (`edgeID`/`setEdgeConstr`/`splitTri`/`splitEdge`).
- IP FFI: `lambda3d_tpi` (createTPI), `orient2d`/`orient3d` on implicit points via
  `genericPoint::` static dispatch (CR-IP6 `_II/_IIII`-segfault gotcha).

## Scope (this PR)
Realize each AR2b `ConstraintSegment` as constraint-flagged mesh edge(s) in its
base triangle's submesh: if the edge already exists → `setEdgeConstr`; else insert
it via `addConstraintSegment` — find the elements it crosses, and **at each
crossing with an existing constraint edge construct a TPI** (`createTPI` → real
`ImplicitPoint3DTpi` via `lambda3d_tpi`, source planes from
`computeTriangleOfSegment`) → `splitSegmentInSubSegments` → recurse. Build the
**minimal `TriangleSoup`** that `createTPI`/`computeTriangleOfSegment` require (the
vertex/arena lookup of the segments' source triangles) — full global soup
assembly + cross-triangle dedup is **AR3b (out of scope here)**. Also out: boolean
labeling (BL*), the `tbb` parallel path.

## Oracle (structural + EXACT — no C++ arrangement binary; full parity at BL3)
1. **Constraints realized (load-bearing):** every `ConstraintSegment` is present
   as a chain of **constraint-flagged** edges (`setEdgeConstr`), end-to-end, no
   segment crossing a non-vertex edge interior.
2. **TPI exactness:** each `createTPI` point lies on ALL THREE supporting planes
   (base tri + the two crossing segments' source tris), via exact
   `orient3d == Zero` (NOT float tolerance).
3. **Valid conforming sub-triangulation:** post-enforcement the submesh still
   tiles the triangle (exact `orient2d`, no gaps/overlaps, non-degenerate).
4. **No spurious TPI:** a segment coincident with an existing edge → flagged, no
   new vertex; two crossing segments → exactly one TPI at the crossing.
5. Hand-verified: an X-crossing of two constraint segments (one TPI on 3 planes);
   a T-junction; a segment already an edge.

## CI gate
`cargo test -p cherchi-rs` (DEFAULT FFI-free/WASM-clean, prior unregressed) AND
`--features indirect-predicates`; `cargo test -p indirect-predicates-sidecar-rs`
if a wrapper is added; `cargo fmt -p cherchi-rs -- --check`; `cargo clippy
-p cherchi-rs --all-targets --features indirect-predicates -- -D warnings` (+
default). No `unsafe`/`panic!` in production. MIT attribution. Single-threaded.

Role-separated FIP: Spec → RED → GREEN → Adversary. **Commit + push at each green
sub-step** (lesson from the AR2b session-limit interruption — a cutoff must never
lose work); if `addConstraintSegment` recursion needs more global state than a
minimal `TriangleSoup` provides, it is acceptable to land the non-crossing
enforcement first and defer the crossing/TPI recursion to a sub-step — STOP and
report rather than improvise.

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR3a done;
AR3b = global soup next) + `docs/yang_deviations.md` (N13 TPI-deferral now fully
resolved — construction + enforcement) + the cherchi-rs `LICENSE-THIRD-PARTY.md`.

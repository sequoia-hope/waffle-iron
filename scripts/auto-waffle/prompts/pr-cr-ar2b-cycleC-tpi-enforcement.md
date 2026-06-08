# PR-CR-AR2b Cycle C — real TPI handle + constraint-edge enforcement (completes AR2b)

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution;
**reference parity is the correctness oracle**; NO `unsafe`; NO `panic!` in
production; single-threaded — serial path; **exact arithmetic** — IP FFI for
implicit/TPI; **predicates demand-driven**). Arrangement work stays behind the
off-by-default `indirect-predicates` feature — DEFAULT build must remain FFI-free
/ WASM-clean.

This **completes M6 PR-CR-AR2b**. AR2b was decomposed A/B/C: **Cycle A** (segment
predicates, ip-sidecar) and **Cycle B** (constraint-segment grouping +
`group_constraint_segments`/`ConstraintSegment`; exact `point_in_segment_3d` in
`predicates::collinearity` — which RESOLVED the N13 raw-`f64` guard; plus TPI
*plumbing* with a clearly-marked, never-exercised placeholder in
`arrangements::aux_structure`'s `backing`/`gp`) are **DONE + merged**. Cycle C
adds the two pieces the placeholder defers.

## Scope (this PR — the two deferred pieces + the AR2b adversary)
1. **Real `ImplicitPoint3DTpi` routing.** Replace the Cycle-B placeholder Tpi arms
   in `aux_structure` (`backing` returns `gens: vec![]`; `gp` returns the
   explicit-centroid stand-in) with the **real** TPI handle: construct
   `ImplicitPoint3DTpi` via `lambda3d_tpi_*` (CR-IP4), add the `Gp::T` arm, and
   expand the orient-predicate dispatch to cover TPI args (the "81-way" generic
   dispatch). **Use `genericPoint::` static dispatch** for the orient2d/orient3d
   predicates on implicit points — heed the CR-IP6 memory: the `_II/_IIII`
   variants segfault on explicit input; do NOT call them directly.
2. **`addConstraintSegment` edge enforcement.** Port
   `triangulation.cpp::addConstraintSegment` (cpp:597) + `createTPI` (cpp:1007) +
   the helpers (`findIntersectingElements`, `computeTriangleOfSegment` cpp:1041,
   `segmentsIntersectInside` cpp:1170, `splitSegmentInSubSegments` cpp:1185):
   for each Cycle-B `ConstraintSegment`, if the edge already exists →
   `setEdgeConstr`; else insert it, and where it crosses an existing constraint
   edge → `createTPI` (real handle from piece 1) → split into sub-segments →
   recurse. This is the actual enforcement that realizes intersection segments as
   constrained mesh edges.

**OUT of scope:** global conforming soup / cross-triangle welding (AR3); boolean
labeling (BL*); the C++ `tbb` parallel path. If `computeTriangleOfSegment` /
`addConstraintSegment` recursion reveals a dependency on AR3-level cross-triangle
state, **STOP and report** (re-scope, don't improvise a Cherchi deviation).

## Build on (do NOT re-port)
Cycle A segment predicates; Cycle B `ConstraintSegment`/`group_constraint_segments`
+ exact `point_in_segment_3d`; AR2a `retriangulate`/`aux_structure` submesh +
CR12c `FastTrimesh` (`edgeID`/`setEdgeConstr`/`splitTri`/`splitEdge`); the IP FFI
(`lambda3d_tpi`, `orient2d`/`orient3d` on implicit points via `genericPoint::`).

## Oracle (full C++ corpus parity is AR3 — exactness + structure here)
1. **Constraints realized:** every grouped intersection segment is present as a
   chain of **constraint-flagged** mesh edges, end-to-end, no segment crossing a
   non-vertex edge interior.
2. **TPI exactness (load-bearing):** each `createTPI` point lies on ALL THREE
   supporting planes (base triangle + the two crossing segments' source
   triangles), via exact `orient3d == Zero` (NOT float tolerance). The placeholder
   centroid is GONE — assert a real TPI vertex now flows through `backing`/`gp`.
3. **Valid covering sub-triangulation** preserved post-enforcement (exact
   `orient2d`, no gaps/overlaps, non-degenerate).
4. Hand cases: two crossing constraint segments → exactly one TPI on all 3 planes;
   segment coincident with an existing edge → flagged, no new vertex.
**Adversary (covers the whole AR2b):** TPI not silently misplaced; the exact
`point_in_segment` (Cycle B) holds on collinear-but-just-outside cases; no
regression to AR1/AR2a; default build still FFI-free.

## CI gate
`cargo test -p cherchi-rs` (DEFAULT FFI-free/WASM-clean, prior unregressed) AND
`--features indirect-predicates`; `cargo test -p indirect-predicates-sidecar-rs`
if a wrapper is added; `cargo fmt -p cherchi-rs -- --check`; `cargo clippy
-p cherchi-rs --all-targets --features indirect-predicates -- -D warnings` (+
default). No `unsafe`/`panic!` in production. MIT attribution. Single-threaded.

Role-separated FIP: Spec → RED → GREEN → Adversary. If piece 1 (the 81-way TPI
dispatch) alone is large enough to risk a session-limit cutoff, it is acceptable
to land piece 1 first (real TPI routing, its own RED/GREEN/adversary, committed +
pushed) and defer piece 2 (`addConstraintSegment`) to a Cycle C2 — **commit/push
at each green sub-step so a cutoff never loses work** (lesson from the AR2b
interruption).

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR2b DONE
— or note C1 done / C2 remaining if split; AR3 next) + `docs/yang_deviations.md`
(N13 TPI deferral RESOLVED; the N13 f64-guard already resolved in Cycle B) + the
cherchi-rs `LICENSE-THIRD-PARTY.md`.

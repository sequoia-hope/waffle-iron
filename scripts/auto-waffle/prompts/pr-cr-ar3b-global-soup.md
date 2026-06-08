# PR-CR-AR3b — cherchi-rs arrangement: global conforming soup + orchestration

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution;
NO `unsafe`; NO `panic!` in production — all `Result<>`; single-threaded — serial
path; **exact arithmetic** — IP FFI for implicit/TPI; **predicates
demand-driven**). Arrangement work behind the off-by-default `indirect-predicates`
feature — DEFAULT build must remain FFI-free / WASM-clean.

This is **M6 PR-CR-AR3b** — the global assembly that ties the finished per-stage
pieces into a **complete native mesh arrangement**. With AR1 (classify→points),
AR2a (point insertion), AR2b (grouping + exact `point_in_segment` + TPI handle),
and AR3a (constraint enforcement + X-crossing TPI) all done, AR3b ports the
orchestration + the global `TriangleSoup` that produces the conforming soup.

## Parity-oracle note (unchanged from AR3a)
No standalone C++ arrangement binary exists. **Oracle = structural + EXACT
predicate invariants** (below). Full C++ reference parity engages at **BL3**
(`mesh_booleans` transitively validates the arrangement). Do NOT build a C++
arrangement-dump sidecar.

## Port source + spec
- **C++ (MIT — attribute):**
  `arrangements/code/solve_intersections.cpp` — `meshArrangementPipeline`
  (cpp:40) / `solveIntersections` orchestration; `triangle_soup.h/.cpp` — the
  `TriangleSoup` container (`vert`/`addImplVert`/`vertOrigID`/`triLabel`/
  `appendJollyPoints`/`initJollyPoints`, the `multiplier` scaling); the input
  prep `mergeDuplicatedVertices` + `removeDegenerateAndDuplicatedTriangles`; and
  the `AuxiliaryStructure` (`intersectionList`/`initFromTriangleSoup`). The
  `new_tris`/`new_labels` assembly via `vertOrigID` (triangulation.cpp:126-129) is
  the merge/dedup. Read as the spec; reproduce faithfully.

## Build on (do NOT re-port)
- `processing::compute_multiplier`/`multiply_coordinates` (CR2/CR3).
- `arrangements::intersection_detection` (CR13 `detect_intersecting_pairs`).
- `arrangements::intersection_points` (AR1 classify→typed vertices).
- `arrangements::retriangulate`/`aux_structure`/`enforce`/`gp_dispatch`
  (AR2a/AR2b/AR3a — per-triangle point insertion + constraint enforcement + TPI).
- `FastTrimesh` (CR11/12) — the per-triangle submesh; `Mesh`.
- IP FFI: `init_fpu`, `lambda3d_lpi/tpi`, the implicit `orient` predicates.

## Scope (this PR)
Port `meshArrangementPipeline`: input coords/tris → multiplier → input-vertex
dedup (`mergeDuplicatedVertices`) + degenerate/dup-triangle removal → build the
global `TriangleSoup` → detect (CR13) → classify (AR1) → per-triangle
re-triangulate+enforce (AR2a+AR3a), assembling each submesh's output into the
**global** `out_tris`/`out_labels` via `vertOrigID` (new implicit points get a
deduped global id through `addImplVert`; shared vertices reuse ids) → append jolly
points. Emit the complete conforming triangle soup (global vertices + tris +
labels) — the input to BL*.

**Explicitly OUT of scope (deferred):** the **N16 deep-recursion / coplanar TPI**
(a constraint segment crossing MULTIPLE existing constraints → recursive
sub-segment TPI chains; the `seg2tris` global structure) — if an input needs it,
emit a classified loud marker (never silent) and defer to a sub-step / AR3b-2.
Also out: boolean labeling (BL*), the `tbb` parallel path. If the orchestration
reveals deep-recursion is needed for the *common* case (not just pathological
inputs), STOP and report.

## Oracle (structural + EXACT — full parity at BL3)
1. **Conforming soup (load-bearing):** the global output soup is
   **non-self-intersecting** — no two output triangles intersect in their
   interiors — checked via the EXACT tri-tri predicates (CR9), not float
   tolerance, on a small corpus.
2. **Every detected intersection realized:** each CR13 intersecting pair's
   intersection appears in the soup as shared/constraint edges (the inputs are
   conformed along their intersections).
3. **Topology sanity:** consistent shared-vertex ids (dedup works — coincident
   implicit points share one id); valid triangles (non-degenerate, exact area>0);
   Euler/edge-incidence sanity on closed inputs.
4. **Input prep correctness:** duplicated input vertices merged; degenerate/dup
   input triangles removed (exact).
5. Hand cases: two tetrahedra sharing a face/edge; a small two-box overlap
   (axis-aligned + a rotated case) → conforming soup with the expected shared
   intersection edges; a non-intersecting pair → soup == inputs (modulo prep).

## CI gate
`cargo test -p cherchi-rs` (DEFAULT FFI-free/WASM-clean, prior unregressed) AND
`--features indirect-predicates`; `cargo test -p indirect-predicates-sidecar-rs`
if a wrapper is added; `cargo fmt -p cherchi-rs -- --check`; `cargo clippy
-p cherchi-rs --all-targets --features indirect-predicates -- -D warnings` (+
default). No `unsafe`/`panic!` in production. MIT attribution. Single-threaded.

Role-separated FIP: Spec → RED → GREEN → Adversary. **Commit + push at each green
sub-step** (a session-limit cutoff must never lose work). If the global assembly
is too large for one cycle, land the orchestration + soup for the non-recursive
common case first (committed/pushed) and defer the rest — STOP and report rather
than improvise.

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR3b done
or note what's deferred; **the complete native arrangement now exists** → BL1
next) + `docs/yang_deviations.md` (N16 status) + the cherchi-rs
`LICENSE-THIRD-PARTY.md`.

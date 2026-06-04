# PR-CR-AR2a — cherchi-rs arrangement: per-triangle POINT/EDGE insertion

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution
header on ported files; **reference parity is the correctness oracle**; NO
`unsafe`; NO `panic!` in production — all `Result<>`; single-threaded; **exact
arithmetic** — `dashu`/existing exact predicates for explicit, the
`indirect-predicates-sidecar-rs` FFI for implicit/LPI/TPI; **predicates are
demand-driven**). The arrangement work is behind the off-by-default
`indirect-predicates` feature (AR1 convention) — the DEFAULT crate build must
stay FFI-free / WASM-clean.

This is the **first half of M6 PR-CR-AR2** (per-triangle constrained
re-triangulation). See `docs/yang_functional_roadmap.md` §M6. AR2 is split:
**AR2a = POINT/EDGE insertion (this PR)**; AR2b = constraint segments + TPI
(next). Do **not** port the spade NC1 CDT path here — it is f64-Delaunay and
cannot handle exact/implicit points; port Cherchi's incremental insertion on
implicit points via the CR12c `FastTrimesh` split API + exact predicates.

## Port source + spec
- **C++ source to port (MIT — attribute):**
  `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp`
  — specifically the POINT-insertion machinery:
  `triangulateSingleTriangle` (the point-collection + per-triangle submesh setup,
  cpp:52-102, **up to but NOT including** `addConstraintSegmentsInSingleTriangle`),
  `splitSingleTriangle` (interior-point insertion, cpp:189-223; the `…WithTree`
  variant cpp:413+ if the existing CR12c `Tree` makes it natural), and
  `splitSingleEdge` (on-edge insertion, cpp:501-575). Read these as the spec;
  reproduce faithfully (do NOT invent mechanism).
- **Paper context:** Cherchi 2020 §5 (arrangement / triangle subdivision) + the
  cherchi2022 text.

## Build on what `cherchi-rs` already has (do NOT re-port)
- **AR1** (`arrangements::intersection_points`) — the typed intersection vertices
  (explicit + LPI) per intersecting pair: these are the POINTS to insert.
- **CR12c `FastTrimesh`** — `splitTri` / `splitEdge` re-triangulation API + the
  `Tree` (point-location acceleration) + plane queries.
- Exact predicates: `orient2d` (CR10) / `orient3d` (CR6) and the IP `orient3d`
  (CR-IP6, with implicit points — heed the `_II/_IIII`-segfault gotcha: use
  `genericPoint::` static dispatch). Point-in-triangle / location uses EXACT
  predicates, no float tolerance.

## Scope (this PR only)
For each base triangle that AR1 marked as carrying intersection points: build the
per-triangle submesh and **insert every intersection POINT** — interior points
via `splitTri` (locate the containing sub-triangle exactly, then split), on-edge
points via `splitEdge` — producing a valid sub-triangulation of the original
triangle whose vertex set includes all its intersection points. **OUT of scope:**
enforcing the intersection SEGMENTS as edges (`addConstraintSegment` — AR2b), TPI
construction (`createTPI` — AR2b), the global conforming soup / cross-triangle
welding (AR3), boolean labeling (BL*). Points that would require a segment-segment
TPI to place are AR2b — if AR2a encounters one, defer it with a classified marker
(loud, never silent), mirroring AR1's `Deferred(..)`.

## Oracle (full C++ corpus parity is AR3 — at this stage assert structure)
1. **Valid covering sub-triangulation (load-bearing):** the inserted sub-triangles
   exactly tile the original triangle — no gaps, no overlaps, consistent winding —
   checked via EXACT `orient2d` (in the triangle's supporting plane), not float
   tolerance. Total sub-triangle area == original (exact/`dashu` where points are
   explicit; for implicit points assert via the exact predicates, not f64 area).
2. **Completeness:** every AR1 intersection point for the triangle is a vertex of
   the sub-triangulation; interior points are interior, on-edge points lie on the
   correct edge (exact incidence).
3. **Topology validity:** the resulting `FastTrimesh` submesh is a valid
   triangulation (every sub-tri non-degenerate; the CR11/CR12 invariants hold).
4. Small hand-verified cases (1 interior point; 1 on-edge point; 2 interior; a
   point coincident with a corner → no spurious split).
Document that segment-conformance + cross-triangle parity are AR2b/AR3.

## CI gate
`cargo test -p cherchi-rs` (DEFAULT — FFI-free/WASM-clean, prior tests
unregressed) AND `cargo test -p cherchi-rs --features indirect-predicates`
(exercises the AR1+AR2a path); `cargo fmt -p cherchi-rs -- --check`; `cargo clippy
-p cherchi-rs --all-targets --features indirect-predicates -- -D warnings` (and
default). No `unsafe`, no `panic!` in production. MIT attribution header.
Single-threaded (the C++ `tbb` parallelism is a future feature flag — port the
serial path).

Role-separated FIP: Spec (you) → RED → GREEN → Adversary. If the C++ reveals
point insertion is entangled with the constraint step in a way that can't be
cleanly split from AR2b, **STOP and report** (re-scope rather than improvise).

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR2a
done; AR2b = constraints+TPI next) + `docs/yang_deviations.md` if a new deviation
arises + the cherchi-rs `LICENSE-THIRD-PARTY.md` for the newly ported file.

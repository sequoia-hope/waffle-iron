# PR-CR-AR2a — per-triangle POINT/EDGE insertion (spec)

Manager spec for the approved PR-CR-AR2a plan. M6 native-arrangement slice:
for each base triangle carrying AR1 intersection points, build a per-triangle
submesh and **insert every intersection POINT** (interior → `split_tri`,
on-edge → `split_edge`), yielding a valid covering sub-triangulation whose
vertex set includes all the triangle's intersection points.

**Out of scope (AR2b/AR3/BL\*):** `addConstraintSegment` (enforcing
intersection *segments* as edges), TPI construction, cross-triangle welding,
boolean labeling.

The PR is split into three atomic, role-separated FIP cycles (Spec → RED →
GREEN → Adversary). Each cycle's crate gate must be clean before the next.

---

## Cycle 1 — CR-IP6b: implicit 2D predicates in `indirect-predicates-sidecar-rs`

Demand-driven (AR2a Cycle 3 is the caller). Mirror the existing CR-IP6
`orient3d` plumbing exactly: void\* generic static dispatch (NOT the `_II/_IIII`
variants — they segfault on explicit input), stub-mode fallback,
`AsGenericPoint` sealed trait, `Sign` return.

### C wrapper (`src/wrapper.h` + `src/wrapper.cpp`)
- `int ip_orient2d_xy(const void* a, const void* b, const void* c)` via
  `genericPoint::orient2Dxy`. Likewise `ip_orient2d_yz` (`orient2Dyz`) and
  `ip_orient2d_zx` (`orient2Dzx`).
- `int ip_point_in_triangle(const void* p, const void* a, const void* b,
  const void* c)` via the 4-arg `genericPoint::pointInTriangle` (bool→int,
  `0`/`1`).
- Stub build (`src/stub.cpp`): `ip_orient2d_*` return `2` (Undefined sentinel,
  matching the orient3d stub convention); `ip_point_in_triangle` returns `0`.

### Rust (`src/lib.rs`)
- `pub fn orient2d_xy(p1, p2, p3: &impl AsGenericPoint) -> Sign` (+ `_yz`,
  `_zx`), each `Sign::from_int` of the FFI int.
- `pub fn point_in_triangle(p, a, b, c: &impl AsGenericPoint) -> bool` —
  returns the FFI int `!= 0`. **Boundary semantics:** upstream
  `pointInTriangle` is inside-OR-boundary inclusive; we return that directly
  (do not remap). Stub mode returns `false`.
- `unsafe` only inside the one-line wrappers; `// Safety:` comments mirroring
  the orient3d wrappers.

### RED tests (`tests/smoke.rs`, appended)
- Fail-loud (panic, never silent skip) when `!AVAILABLE`, matching the AR1
  on-plane oracle precedent.
- `orient2d_xy/yz/zx` smoke with explicit handles: collinear→`Zero`, CCW→one
  sign, CW→the other.
- **Cross-check** `orient2d_xy` sign against `cherchi-rs`? No — the sidecar
  crate may not depend on cherchi-rs. Instead cross-check the three planes
  against each other / against hand-computed signs, and verify at least one
  call routes through an **LPI** handle (implicit dispatch path) without
  segfault, returning a defined `Sign`.
- `point_in_triangle`: explicit point strictly inside → `true`; on a vertex →
  `true`; on an edge → `true`; strictly outside → `false`; an LPI point known
  to lie inside a triangle → `true`.

### Gate
`cargo test -p indirect-predicates-sidecar-rs`,
`cargo fmt -p indirect-predicates-sidecar-rs -- --check`,
`cargo clippy -p indirect-predicates-sidecar-rs --all-targets -- -D warnings`.

---

## Cycle 2 — FastTrimesh typed (implicit-capable) vertices

`crates/cherchi-rs/src/arrangements/fast_trimesh.rs`. Generalize vertex storage
to a typed enum (file deviation #1 foreshadows this). **WASM-clean / FFI-free**:
`VertexCoords` stores only `Point3` generators, no FFI types.

```rust
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum VertexCoords {
    Explicit(Point3),
    Lpi { line: [Point3; 2], plane: [Point3; 3] },
}
```

- `Vertex.point: Point3` → `Vertex.coords: VertexCoords`.
- `from_soup` / `add_vert` / `add_vert_with_orig_id` wrap inputs as
  `VertexCoords::Explicit` — **existing public signatures unchanged** (CR11/CR12
  tests must stay green untouched).
- New: `pub fn add_vert_typed(&mut self, c: VertexCoords) -> u32`;
  `pub fn vert_coords(&self, v: u32) -> &VertexCoords`.
- `vert(v) -> Point3` and `tri_vert(t,off) -> Point3` keep signatures. For an
  `Explicit` vertex they return its point; for an `Lpi` vertex they return an
  **approx** Point3 (e.g. the line midpoint — document loudly: "approx only for
  Lpi; exact queries route through `vert_coords` + the indirect-predicates
  FFI"). Pick a deterministic, finite approx; it is never oracle-checked.
- `tri_orientation`: add `debug_assert!` that its 3 corners are
  `VertexCoords::Explicit` (base-triangle corners always are; AR2b's
  `triOrientation(0)` is unaffected) to catch misuse on implicit verts.
- `split_tri` / `split_edge` / `*_with_tree` / `add_tri` / removal: **unchanged**
  (index-only topology; they read no coordinates).

### RED tests
- Existing CR11/CR12 invariant suite stays green (no edits to those tests).
- `add_vert_typed` + `vert_coords` round-trip on both variants.
- `vert()` returns the documented approx for an `Lpi` vertex.
- Invariant sums (num_verts etc.) hold after adding typed verts.

### Gate
`cargo test -p cherchi-rs` (default) **and** `cargo test -p cherchi-rs
--features indirect-predicates`; `cargo fmt -p cherchi-rs -- --check`;
`cargo clippy -p cherchi-rs --all-targets -- -D warnings` (default) and
`--features indirect-predicates`.

---

## Cycle 3 — AR2a proper: grouping + insertion driver + oracle

`crates/cherchi-rs/`. All new code `#[cfg(feature = "indirect-predicates")]`,
MIT attribution header on ported files, exported from
`src/arrangements/mod.rs` under the cfg.

### 3a. Grouping — `src/arrangements/aux_structure.rs` (NEW)

Per-base-triangle buckets AR1 omitted, using **exact** predicates only.

```rust
pub struct TypedPoint { /* global id + VertexCoords + provenance */ }
pub struct TriangleAuxPoints { pub interior: Vec<u32>, pub edges: [Vec<u32>; 3] }
pub fn group_intersection_points(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
) -> (Vec<TypedPoint>, Vec<TriangleAuxPoints>) // TriangleAuxPoints indexed by base-tri id
```

- Dedup the global typed-vertex set: `Explicit` by exact `Point3` equality;
  `Lpi` by structural generator equality (`line`+`plane` slice equality).
  (Exact-coincident different-generator LPIs are a flagged follow-up; low
  likelihood for transversal inputs.)
- `Explicit { tri, corner, point }`: for its **owning** `tri` the point IS a
  corner → record nothing (never re-inserted — corner-coincident no-spurious-
  split case). For the **other** triangle of the pair, classify `point` via
  `point_in_triangle_3d` (CR8 exact): `StrictlyInside` → that tri's `interior`;
  `OnBoundary` → which edge (exact: the edge whose two corners are collinear
  with `point` — use orient/collinearity on exact coords) → `edges[i]`.
- `Lpi { line, plane }`: it lies on edge `line` of the triangle X that owns
  that edge → push to X's `edges[that edge]` (match `line` endpoints to X's
  corners by exact equality). It lies in the plane of triangle Y (= `plane`) →
  interior-or-edge of Y, decided by the **exact implicit** `point_in_triangle`
  FFI from Cycle 1, plus the three `orient2d_*` (in Y's ref plane) to find a
  zero edge → `interior` or `edges[i]`.
- `TriangleAuxPoints` must be returned for every base-tri id `0..num_tris`
  (empty buckets for non-intersecting tris) so Cycle 3b can index by tri id.

### 3b. Driver — `src/arrangements/retriangulate.rs` (NEW)

Port the readable `splitSingleTriangle` (triangulation.cpp:189-222) — linear-
scan locate (the active `WithStack` perf port is a later deviation; `WithTree`
is the alternative if reusing the CR12c `Tree`).

```rust
pub fn split_single_triangle(
    subm: &mut FastTrimesh,     // 1-triangle submesh of base tri t
    points: &[TypedPoint],
) -> Result<(), RetriangulateError>
```

Faithful loop: add the first point via `add_vert_typed`, `split_tri(0, v)`; for
each subsequent point → `add_vert_typed`, `find_containing_triangle` (private:
loop tris, implicit-aware `point_in_triangle` FFI), test the 3 edges with
`fast_point_on_line` (private: exact 2D `orient2d_*` == `Zero` in the submesh's
ref plane) → `split_edge` on the first on-edge hit, else `split_tri`. Route
**all** location / on-edge tests through the Cycle-1 FFI generic dispatch when
the feature is on (it handles explicit-explicit correctly too → byte-parity
with C++). No `panic!` — all error conditions are `Result<>`
(`RetriangulateError`, e.g. `NoContainingTriangle`).

The submesh is constructed by the caller (the test, in AR2a) from the base
triangle's 3 explicit corners as a 1-triangle `FastTrimesh` with the base's
ref plane.

### 3c. Oracle / tests (RED-first; the PR's required checks)

1. **Exact covering sub-triangulation** (load-bearing): sub-tris tile the base
   — every sub-tri shares the base's `tri_orientation` sign (exact); **area
   conservation** = sum of |sub-tri areas| == base |area| in exact rational
   (`dashu`), with LPI corner coords from `lambda3d_lpi_exact` (NOT `approx`);
   no-overlap follows from equal-sign + area-sum + shared-edge manifoldness.
2. **Completeness:** every input `TypedPoint` is a submesh vertex; interior
   points classify `StrictlyInside` (exact `point_in_triangle` FFI), on-edge
   points lie exactly on the correct edge (`orient2d == Zero` for that edge,
   `!= Zero` for the other two).
3. **Topology validity:** CR11/CR12 invariants hold post-insertion; exact
   sub-tri count per Euler.
4. **Four hand cases:** (a) 1 interior → 3 tris; (b) 1 on-edge → 2 tris (with
   an LPI on-edge variant exercising the implicit path); (c) 2 interior → 5
   tris; (d) corner-coincident → no new vertex / no split (`num_tris == 1`).

Document in the modules that segment-conformance + cross-triangle parity are
AR2b/AR3; full C++ differential corpus parity is AR3. A single base-triangle
differential vs the sidecar is a stretch goal, not a gate.

### Gate (Cycle 3)
`cargo test -p cherchi-rs` (default — FFI-free / WASM-clean, prior tests
unregressed) **and** `cargo test -p cherchi-rs --features indirect-predicates`;
`cargo fmt -p cherchi-rs -- --check`; `cargo clippy -p cherchi-rs --all-targets
--features indirect-predicates -- -D warnings` (and default). No `unsafe`, no
`panic!` in production.

---

## Process

Role-separated FIP (P5): distinct sub-agents per role — Spec (Manager) → RED
→ GREEN → Adversary. The Implementer never edits tests; the test author never
writes production code. Stay on `main`. Commit each phase with a conventional
message + the `Co-Authored-By: Claude Opus 4.8 (1M context)
<noreply@anthropic.com>` trailer; push at cycle end. No hack-to-green (P9/P10):
a genuine conflict ⇒ STOP and report. If a prior test's expectation is
obsoleted, migrate only the expected outcome (preserve structural assertions);
the Adversary verifies the migration was not weakened.

## Docs to update on completion
- `docs/yang_functional_roadmap.md` §M6 — mark PR-CR-AR2a done; AR2b next; note
  the CR-IP6b precursor.
- `docs/yang_deviations.md` — readable-`splitSingleTriangle`-over-`WithStack`
  deviation; LPI structural-equality dedup (first slice).
- `crates/cherchi-rs/LICENSE-THIRD-PARTY.md` — newly ported `retriangulate.rs`.
- Memory: `cherchi_rs_pr_cr_ar2a` topic file + MEMORY.md pointer.

# `cherchi-rs::predicates::triangle_intersects_triangle_3d` — Spike PR-CR9

## Goal

Classify the 3D-spatial relationship between two triangles `T1 = (a, b, c)`
and `T2 = (d, e, f)` as `Disjoint`, `Intersects`, or `Coplanar`. The
central algorithm of Cherchi 2022 §3 — the primitive that drives mesh
arrangement intersection detection.

This is the **algorithmic payoff PR** for the 8 preceding foundation
PRs. cherchi-rs can now answer the central question of mesh
arrangement: do two triangles intersect, and if so, how?

The implementation composes PR-CR7 (`triangles_are_coplanar`, the
branch point) and PR-CR8 (`segment_intersects_triangle_3d`, the
edge-triangle primitive) into the full Cherchi 2022 §3 algorithm.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `a, b, c` | `Point3` | Vertices of T1 |
| `d, e, f` | `Point3` | Vertices of T2 |

Returns: `TriangleIntersection` —
- `Disjoint` — triangles share no points
- `Intersects` — triangles share at least one point (confirmed via 3D test)
- `Coplanar` — caller must run 2D handler. Covers BOTH:
  - Full coplanar (both triangles in same plane)
  - Partial coplanar (an edge of one lies in the other's plane)

All inputs must be finite. NaN / infinite inputs are undefined.

## Branch table

| Input class | Output |
|-------------|--------|
| Triangles in same plane (full coplanar) | `Coplanar` |
| Non-coplanar, far apart | `Disjoint` |
| Non-coplanar, parallel planes | `Disjoint` |
| Non-coplanar, T1 entirely on one side of T2's plane | `Disjoint` |
| Non-coplanar, an edge of T1 crosses T2's interior | `Intersects` |
| Non-coplanar, an edge of T2 crosses T1's interior | `Intersects` |
| Non-coplanar, share a vertex with edges crossing | `Intersects` |
| Non-coplanar, share an edge (vertex coincidence) | `Intersects` (the shared edge IS the intersection; line tests via touching edges' vertex coincidence propagate Intersects) |
| Non-coplanar, edge of T1 lies in T2's plane but far from T2 | `Coplanar` (caller's 2D handler refines to Disjoint) |
| NaN / infinite inputs | Undefined |
| Degenerate triangle (collinear vertices) | Caller's responsibility |

## Invariants

1. **Symmetry**: swapping T1 with T2 preserves the result.
2. **Vertex-permutation invariance**: any permutation of `(a, b, c)`
   or `(d, e, f)` preserves the result (inherited from PR-CR7 and
   PR-CR8's invariances).
3. **Determinism**: same input → same output across runs and platforms
   (inherits from PR-CR6's `orient3d`).

## Oracles

1. **Canonical Disjoint** (3 cases): far apart, parallel planes, on
   same side of plane
2. **Canonical Intersects** (3 cases): perpendicular triangles crossing,
   shared vertex with edges crossing, edge of T1 through T2's interior
3. **Canonical Coplanar** (full case): two distinct triangles in z=0
4. **Canonical Coplanar** (full case): identical triangle
5. **Canonical Coplanar** (edge case): non-coplanar triangles sharing an edge
6. **Property: symmetry** — T1↔T2 swap preserves result
7. **Property: vertex-permutation invariance**
8. **Determinism** — 100 invocations identical

## Failure modes

- **NaN / infinite input**: undefined.
- **Degenerate triangle (collinear vertices)**: deterministic but may
  misclassify; caller's responsibility to filter via
  `points_are_collinear_3d` (PR-CR1).
- No error return; enum is the only output.

## Research basis

- **Cherchi 2022 §3** — full triangle-triangle intersection algorithm
- **Shewchuk 1997 §2.1** — `orient3d` foundation
- Composes PR-CR7 (coplanarity branch) + PR-CR8 (edge-triangle test)

## Method

Cherchi 2022 §3's branch-then-iterate algorithm:

1. Call `triangles_are_coplanar(a, b, c, d, e, f)` (PR-CR7).
   If true → return `Coplanar`.
2. Non-coplanar branch: iterate 6 edge-triangle pairs.
   - For each of T1's 3 edges: call `segment_intersects_triangle_3d`
     against T2 (PR-CR8)
   - For each of T2's 3 edges: call `segment_intersects_triangle_3d`
     against T1
3. Aggregate with priority **Intersects > Coplanar > Disjoint**:
   - If any returned `Intersects` → return `Intersects`
   - Else if any returned `Coplanar` (edge-in-other-plane) → return `Coplanar`
   - Else (all `Disjoint`) → return `Disjoint`

## Why `Coplanar` covers both full + partial cases

The 3-state enum's `Coplanar` is the "caller needs 2D refinement"
catch-all. Two situations land here:

1. **Full coplanar**: both triangles in the same plane (detected by
   PR-CR7). Caller runs 2D triangle-triangle overlap.

2. **Partial coplanar (edge in other's plane)**: triangles aren't in
   the same plane, but an edge of one lies in the other's plane.
   `segment_intersects_triangle_3d` returns `Coplanar` for that edge
   (since both endpoints are on the plane). Caller runs 2D segment-
   triangle test for the edge.

In both cases the caller's response is the same: "run 2D handler."
Using a single `Coplanar` variant keeps the API simple. The caller's
2D handler distinguishes the two situations internally.

Edge cases this correctly handles:
- **Shared edge between non-coplanar triangles** (vertex coincidence):
  The edge itself returns `Coplanar` from segment-triangle (both
  endpoints in the other plane). BUT the OTHER edges of T1 (which
  touch the shared edge's endpoints) hit vertex coincidence in their
  line tests, producing `Zero` orient3d results that combine with
  non-zero results to fall into the "all-same-sign-or-zeros" branch
  → segment-triangle returns `Intersects` for those edges. Aggregation
  with `Intersects > Coplanar` priority → result is `Intersects`.
  This is geometrically correct (the shared edge IS the intersection).
- **Edge in plane but doesn't touch the triangle** (no vertex
  coincidence): segment-triangle returns `Coplanar` for that edge;
  other edges return `Disjoint`. Aggregation → `Coplanar`. Caller's
  2D handler determines no actual intersection → Disjoint.

The implementation is "smart enough" to detect shared edges as
`Intersects` via the secondary propagation pathway, even though the
primary edge-in-plane edge returns `Coplanar`. This was discovered
mid-implementation during PR-CR9 GREEN — original spec wrongly claimed
shared edges return `Coplanar`. The corrected behavior is documented
here.

Cases that still return `Coplanar` require caller's 2D refinement:
- Full coplanar (both triangles in same plane)
- Edge in plane but no vertex coincidence (rare; requires the edge's
  line not to pass through any T2 interior even though it's coplanar
  with T2's plane)

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/triangle_intersect.rs` header (new file):

```rust
//! 3D triangle-triangle intersection classification — the algorithmic
//! payoff of PR-CR1 through PR-CR8's foundations.
//!
//! `triangle_intersects_triangle_3d` is the central algorithm of
//! Cherchi 2022 §3: mesh arrangement processes pairs of triangles via
//! this primitive. Branches on coplanarity (PR-CR7) and dispatches to
//! 6 edge-triangle tests (PR-CR8) in the non-coplanar case.
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; full algorithm).
//! Shewchuk 1997 §2.1 (orient3d as the foundational predicate).
//!
//! The 3-state enum (Disjoint / Intersects / Coplanar) collapses two
//! distinct geometric situations into the single `Coplanar` variant
//! (full coplanar AND partial-coplanar/edge-in-plane). Both require
//! caller's 2D refinement; documented in spec §"Why Coplanar covers
//! both cases".
```

(Plain attribution — no `Deliberate deviation`. The 3-state collapse
is documented as YAGNI / API-simplification in spec, not as a
behavioral deviation from upstream.)

## Scope discipline

This PR introduces:
- One `TriangleIntersection` enum (3 variants)
- One `triangle_intersects_triangle_3d` function

Does NOT add:
- 2D triangle-triangle overlap (the `Coplanar` case) — caller handles
- Explicit intersection-geometry computation (line/point/region)
- Granular Intersects sub-cases (interior/edge/vertex) — collapse to single Intersects per YAGNI
- AABB pre-filter / performance optimization

## Verification

```bash
# RED phase
cargo test -p cherchi-rs predicates::triangle_intersect
# expect: all tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 109 (PR-CR1-CR8) + 13 (PR-CR9) = 122 pass

cargo check --workspace
# expect: clean

cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```

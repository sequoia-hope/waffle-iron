# PR-CR-AR3a — Constraint-edge enforcement + TPI-at-crossing

Milestone **M6 / PR-CR-AR3a** of the clean-sheet `cherchi-rs` mesh-arrangement
port (Cherchi 2022 `triangulation.cpp`). This spec is the binding contract for
the role-separated FIP cycle (Spec → RED → GREEN → Adversary).

## Goal

AR2b left each base triangle's submesh with its *intersection points* inserted
as vertices (`split_single_triangle`) and a `Vec<ConstraintSegment>` describing
the transversal segments that must appear as constrained mesh edge(s). It
explicitly banked **enforcement** to AR3 because enforcement needs cross-element
state. AR3a delivers enforcement:

1. **Realize** every constraint segment as a chain of constraint-flagged edges,
   end to end, with no segment crossing the interior of a non-vertex edge.
2. At a crossing of two constraint segments, **construct the TPI** where the
   three supporting planes meet (base triangle + the two crossing segments'
   `source_tri`), insert it, split, flag, and recurse.

This is the native port of `triangulation.cpp::addConstraintSegment` (cpp:597),
`findIntersectingElements` (cpp:644), `boundaryWalker` (cpp:806), `earcutLinear`
(cpp:912), `createTPI` (cpp:1007), `segmentsIntersectInside` (cpp:1170),
`pointInsideSegment` (cpp:1178), `splitSegmentInSubSegments` (cpp:1185) — onto a
minimal per-segment source-triangle lookup (the `ConstraintSegment.source_tri`
AR2b already stores), single-threaded (no `tbb`), exact (IP FFI), `Result<>`-only.

## Parity-oracle note (binding — changes the oracle)

There is **no standalone C++ arrangement binary** (the 2020 arrangement is
library-only; only `mesh_booleans` is built). AR3a therefore does **NOT** diff
against a C++ arrangement and **must not** build a new arrangement-dump sidecar.
The oracle is **structural + EXACT predicate invariants** (§Oracle). Full C++
reference parity engages later (roadmap BL3).

## Files

- **New:** `crates/cherchi-rs/src/arrangements/enforce.rs` — the enforcement
  port, behind `#[cfg(feature = "indirect-predicates")]`, MIT attribution header.
  Holds the RED test module and (later) Adversary tests.
- **New (refactor):** `crates/cherchi-rs/src/arrangements/gp_dispatch.rs` — the
  shared `Gp`/`Backing`/`backing`/`gp`/`with_gp!`/`dispatch_orient2d`/
  `dispatch_point_in_triangle` machinery, moved verbatim out of
  `retriangulate.rs` (pure move, no behaviour change), `pub(crate)` to the
  `arrangements` module. MIT header (it is ported dispatch over ported code).
- **Edit:** `crates/cherchi-rs/src/arrangements/retriangulate.rs` — delete its
  private copies of the above and `use` them from `gp_dispatch` instead.
- **Edit:** `crates/cherchi-rs/src/arrangements/mod.rs` — `#[cfg(feature = ...)]`
  module decl + re-exports for the new public surface; module decl for
  `gp_dispatch` (feature-gated).
- **Edit (docs, close-out only):** `docs/yang_functional_roadmap.md` §M6,
  `docs/yang_deviations.md` (N13), cherchi-rs `LICENSE-THIRD-PARTY.md` ledger.

## Public surface (the contract RED tests against and GREEN implements)

All of the following live in `enforce.rs`, gated behind the
`indirect-predicates` feature, re-exported from `arrangements/mod.rs`.

```rust
/// A constraint segment expressed in SUBMESH-vertex-id terms (the form the
/// enforcement core consumes). `v0`/`v1` are submesh vertex ids; `source_tri`
/// is the segment's supporting plane (the OPPOSITE triangle's 3 corners — for
/// an original transversal segment this is exactly
/// `ConstraintSegment.source_tri`).
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentSpec {
    pub v0: u32,
    pub v1: u32,
    pub source_tri: [Point3; 3],
}

#[derive(Debug, PartialEq)]
pub enum EnforceError {
    /// A `ConstraintSegment` endpoint's interned coords are not present as a
    /// submesh vertex (the submesh was not produced by `split_single_triangle`
    /// over the same `points` set). `interned_id` is the offending endpoint.
    EndpointNotInSubmesh { interned_id: u32 },
    /// The topology walk could not locate the segment in the submesh (e.g. the
    /// endpoints are not both submesh vertices, or the fan is malformed). Wraps
    /// the offending `(v0, v1)` submesh vertex ids.
    SegmentNotLocatable { v0: u32, v1: u32 },
    /// A crossed constraint edge has no recorded supporting plane, so the TPI's
    /// third plane is unavailable. This is the AR3b global-state wall
    /// (`computeTriangleOfSegment`'s global `seg2tris` / coplanar `jollyPoint`):
    /// a sub-segment born mid-recursion that lost its directly-available
    /// `source_tri`. **STOP and report — do not improvise.** Deferred to AR3b.
    SourcePlaneUnavailable { v0: u32, v1: u32 },
    /// The three TPI supporting planes are not in general position (no single
    /// common intersection point — parallel / shared-line / coplanar). The
    /// coplanar `jollyPoint` fallback is AR3b. **STOP and report.**
    DegenerateTpi,
}

/// Enforce a list of constraint segments (submesh-vertex-id form) into the
/// submesh. Seeds an internal work-list from `specs`, then repeatedly pops a
/// work item and calls the `add_constraint_segment` port until the list is
/// empty. Each resulting constraint edge is flagged via `set_edge_constr`.
/// Orientation is computed once internally from the base corners (submesh
/// vertices 0,1,2 — always explicit, never removed).
pub fn enforce_constraint_segments(
    subm: &mut FastTrimesh,
    specs: &[SegmentSpec],
) -> Result<(), EnforceError>;

/// AR2b adapter: enforce `ConstraintSegment`s (interned-id endpoints) by
/// resolving each endpoint id → its `TypedPoint` coords → the submesh vertex
/// carrying those exact coords (structural `VertexCoords` equality, FFI-free),
/// building `SegmentSpec`s, and delegating to `enforce_constraint_segments`.
/// `points` MUST be the interned set the submesh was built from. Returns
/// `EndpointNotInSubmesh` if a resolution fails.
pub fn enforce_constraints(
    subm: &mut FastTrimesh,
    segments: &[ConstraintSegment],
    points: &[TypedPoint],
) -> Result<(), EnforceError>;
```

`add_constraint_segment`, `find_intersecting_elements`, `boundary_walker`,
`earcut_linear`, `create_tpi`, `segments_intersect_inside`,
`point_inside_segment`, `edge_opp_to_vert`, `tri_opp_to_edge`, and the
work-item type are **internal** (`fn` / `pub(crate)` at GREEN's discretion);
RED drives the two public entry points above. GREEN must NOT change the two
public signatures.

### Orientation (deviation from C++ caller)

C++ passes a precomputed `int orientation`. Here `enforce_constraint_segments`
computes it once via `dispatch_orient2d(ref_plane, v0, v1, v2)` on the base
corners (submesh vertex ids 0,1,2). Do **NOT** use `FastTrimesh::tri_orientation(0)`
— after splits the triangle at slot 0 may have a non-explicit corner, tripping
its `debug_assert`. The base corner *vertices* 0,1,2 are explicit and survive
all splits, so reading their `vert_coords` is safe.

### Source-plane bookkeeping (the minimal `TriangleSoup`)

The C++ `createTPI` calls `computeTriangleOfSegment` which sources a segment's
supporting triangle from the global `seg2tris` / `sub_segs_map`. AR3a replaces
that global state with a **per-work-item carried plane**: every work item is
`{ v0: u32, v1: u32, source_tri: [Point3; 3] }`. Sub-segments produced by a
split inherit their parent's `source_tri` (a collinear sub-piece has the same
supporting plane). The crossed constraint edge's plane is read from a
`constraint_planes: HashMap<(u32, u32), [Point3; 3]>` side map keyed by the
edge's **sorted vertex-id pair** (vertex ids are stable under `add_*`/`split_*`;
edge ids are not). When `set_edge_constr` is called during enforcement, the
owning segment's `source_tri` is recorded for that vertex pair (and, on a TPI
split, both halves of the crossed edge inherit the crossed edge's plane).

If a crossed constraint edge's vertex pair is **absent** from `constraint_planes`
→ `SourcePlaneUnavailable` (the AR3b wall). If `create_tpi`'s three planes are
not in general position → `DegenerateTpi`. Both are STOP-and-report.

## Branch table (`add_constraint_segment`, port of cpp:597)

Given a work item `(v0, v1, plane_seg)`:

| Condition | Action |
|---|---|
| `edge_id(v0, v1)` is `Some(e)` | `set_edge_constr(e)`; record `plane_seg` for `(v0,v1)`; **done** (no new vertex). |
| walk finds NO intersected edges (a `point_inside_segment` split happened) | flag the sub-edge, push the remaining sub-segment(s) with `plane_seg`; **return** (re-processed from work-list). |
| walk crosses only NON-constraint edges, reaches `v_stop` | `boundary_walker` ×2 → `earcut_linear` ×2 → `add_tri` new tris → `remove_tris` intersected → `set_edge_constr(edge_id(v_start, v_stop))`; record `plane_seg`. |
| walk meets an EXISTING constraint edge `e=(ev0,ev1)` | look up its plane `plane_e` from `constraint_planes` (else `SourcePlaneUnavailable`); `tpi = create_tpi(base_plane, plane_seg, plane_e)` (else `DegenerateTpi`); dedupe vs an existing submesh vertex with those exact `Tpi` coords; `split_edge(e, tpi_vid)`; flag both halves `(ev0,tpi)` and `(tpi,ev1)` recording `plane_e`; push `(v_start, tpi, plane_seg)` and `(tpi, v_stop, plane_seg)`; **return**. |

`v_start` = the lower-valence endpoint (cpp:609); `v_stop` the other.

## Reused machinery (do NOT re-port)

- `aux_structure.rs`: `ConstraintSegment`, `group_constraint_segments`, `TypedPoint`.
- `fast_trimesh.rs`: `edge_id`, `set_edge_constr`, `edge_is_constr`, `split_edge`,
  `split_tri`, `add_tri`, `remove_tris`, `vert_valence`, `adj_v2t`, `adj_e2t`,
  `tri_edges`, `tri_vert_id`, `tri_vert_offset`, `tri_vert_opposite_to`,
  `tri_contains_vert`, `edge_vert_id`, `vert_coords`, `add_vert_typed`,
  `ref_plane`. `edge_opp_to_vert` and `tri_opp_to_edge` are the two missing
  helpers — derive them privately in `enforce.rs` from the above (do NOT add new
  public `FastTrimesh` API): `edge_opp_to_vert(t,v)` = `edge_id` of the two
  corners of `t` other than `v`; `tri_opp_to_edge(e,t)` = the other entry of
  `adj_e2t(e)` (or `None` if boundary).
- `gp_dispatch.rs` (this PR's refactor): `Gp`, `backing`, `gp`, `with_gp!`,
  `dispatch_orient2d`, `dispatch_point_in_triangle`.
- `indirect-predicates-sidecar-rs`: `inner_segments_cross`,
  `point_in_inner_segment`, `orient2d_xy/yz/zx`, `orient3d`, `point_in_triangle`,
  `ImplicitPoint3DTpi`, `ExplicitPoint3D`. **No new FFI wrapper.** Use
  `genericPoint::` static dispatch only — never `_II`/`_IIII` (CR-IP6 segfault).

## Oracle (structural + EXACT — no C++ arrangement binary)

1. **Constraints realized (load-bearing):** every constraint segment is present
   as a chain of **constraint-flagged** edges (`edge_is_constr`), end-to-end
   between its endpoints, no segment crossing a non-vertex edge interior.
2. **TPI exactness:** each `create_tpi` point lies on ALL THREE supporting
   planes (base tri + the two crossing segments' source tris) via exact
   `orient3d == IpSign::Zero` (NOT float tolerance).
3. **Valid conforming sub-triangulation:** post-enforcement the submesh still
   tiles the base triangle exactly — every sub-tri shares the base winding sign
   and the exact (`RBig`) signed areas sum to the base's (pure-dashu oracle,
   independent of the FFI split path); no degenerate sub-tri.
4. **No spurious TPI:** a segment coincident with an existing edge → flagged,
   **no new vertex** (vertex count unchanged); two crossing segments → exactly
   **one** TPI at the crossing (one `Tpi` vertex added).
5. **Hand-verified cases:** (a) an X-crossing of two original transversal
   constraint segments → one TPI on 3 planes, both segments realized; (b) a
   T-junction (segment passing through an existing interior vertex) → split +
   both sub-edges flagged, no TPI; (c) a segment already an edge → flagged only.

## Scope boundary / STOP conditions (P9/P10 — do not improvise)

**In scope:** already-edge flagging; non-crossing enforcement (boundary walk +
earcut over non-constraint edges); the constraint-crossing `create_tpi` for the
**original-transversal X-crossing** where both crossing segments' `source_tri`
are directly available (carried by the work item / recorded in
`constraint_planes`).

**Out of scope → AR3b (STOP and report, land what is in scope):**
- Global conforming soup assembly; cross-triangle vertex weld/dedup.
- `computeTriangleOfSegment`'s global `seg2tris` sourcing and the coplanar
  `jollyPoint` fallback. If a sub-segment born mid-recursion loses its
  directly-available `source_tri` → `SourcePlaneUnavailable`. If the three TPI
  planes are not in general position → `DegenerateTpi`. Both halt the cycle for
  that input; the implementer reports rather than widening tolerance or adding a
  fallback path.
- Boolean labeling (BL*); the `tbb` parallel path.

## CI gate (all clean before done)

- `cargo test -p cherchi-rs` (DEFAULT, FFI-free / WASM-clean; prior tests
  unregressed) **and** `cargo test -p cherchi-rs --features indirect-predicates`.
- `cargo fmt -p cherchi-rs -- --check`.
- `cargo clippy -p cherchi-rs --all-targets --features indirect-predicates -- -D warnings`
  **and** the default-feature clippy.
- No `unsafe` / no `panic!` in production; MIT attribution header on new ported
  files; single-threaded.

## Governance

Role-separated FIP (Constitution P5): distinct sub-agents for RED (tests only) /
GREEN (production only — never edits tests) / Adversary. Commit + push at each
green sub-step. Stay on `main`. Conventional commits ending in the trailer
`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

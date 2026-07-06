//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR3b RED — global conforming soup + orchestration (tests only)
//!
//! This file is the **RED** slice of milestone M6 / PR-CR-AR3b: the failing
//! test module that pins the global-assembly public surface
//! (`ArrangementSoup`, `ArrangementError`, `Label`, `mesh_arrangement`, and the
//! `pub` prep helpers `merge_duplicated_vertices` /
//! `remove_degenerate_and_duplicated_triangles`) defined verbatim in
//! `specs/pr_cr_ar3b_global_soup.md`. It ports the C++
//! `meshArrangementPipeline` (`solve_intersections.cpp`), the input-prep
//! (`processing.cpp`), the triangle-soup container + jolly points
//! (`triangle_soup.{h,cpp}`), and the per-base-triangle assembly loop
//! (`triangulation.cpp`).
//!
//! **No production code is authored here** — the GREEN sub-agent adds the
//! orchestration port (and the `arrangements/mod.rs` + `lib.rs` re-exports) in
//! a later sub-step. The not-yet-written production symbols are referenced
//! through `crate::arrangements::{mesh_arrangement, ArrangementSoup,
//! ArrangementError, Label}` and `crate::arrangements::soup::{
//! merge_duplicated_vertices, remove_degenerate_and_duplicated_triangles}`, so
//! this module FAILS TO COMPILE under the `indirect-predicates` feature until
//! GREEN lands them (the intended RED state: unresolved-symbol errors for the
//! missing public items, nothing else).
//!
//! The tests cover all five spec §10 oracle invariants:
//! 1. **Conforming soup (load-bearing, EXACT):** no two output triangles
//!    overlap in their interiors — checked in pure-`dashu` rational arithmetic
//!    (every global vertex's exact coords; exact tri-tri interior-intersection
//!    test). The hand cases are designed to produce only `Explicit` + `Lpi`
//!    vertices (no constraint X-crossing → no TPI), so the exact oracle only
//!    needs explicit coords + line∩plane.
//! 2. **Every detected intersection realized:** each CR13 intersecting pair's
//!    intersection appears as shared/constraint edges; the LPI vertices are
//!    present with a SHARED global id.
//! 3. **Topology sanity:** coincident implicit points share one global id;
//!    every output triangle is non-degenerate (exact area > 0); edge-incidence
//!    / Euler sanity on a closed-input hand case.
//! 4. **Input-prep correctness:** duplicated input vertex merged + tris
//!    remapped; degenerate / duplicate input triangles removed (exact), labels
//!    OR-merged.
//! 5. **Hand cases:** two tetrahedra; axis-aligned two-box overlap; rotated
//!    two-box overlap; non-intersecting pair (soup == inputs modulo prep). Plus
//!    `jolly_count == 5` and the jolly tail present.
//! 6. **Loud deferral:** a coplanar-face pair → `Err(CoplanarPairDeferred)`
//!    (never silent / never a wrong soup).
//!
//! The pure-`dashu` exact helpers (`to_r`, `exact_coords`,
//! `exact_signed_area2`) are copied from `retriangulate.rs` / `enforce.rs` test
//! modules verbatim in style (test-only duplication is expected and fine), then
//! extended with an exact tri-tri interior-intersection test for invariant #1.

// The RED test module (authored by the FIP RED sub-agent and frozen — GREEN
// must not edit it) uses `input_corners.iter().any(|c| *c == xc)` over a
// `Vec<[RBig; 3]>`, which the `manual_contains` lint flags only now that GREEN
// makes the module compile. Scope the allow to this module so the gate stays
// clean without touching the frozen test code (semantics unchanged either way).
#![allow(clippy::manual_contains)]

use std::collections::{BTreeMap, BTreeSet};

use crate::arrangements::aux_structure::{
    group_constraint_segments, group_intersection_points, ConstraintSegmentError, TypedPoint,
    VisitedPocketRegistry,
};
use crate::arrangements::coplanar_propagate::{
    find_pockets_in_triangle, integrate_coplanar_into_arrangement, CoplanarIntegration,
};
use crate::arrangements::enforce::{enforce_constraints, EnforceError};
use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::intersection_detection::detect_intersecting_pairs;
use crate::arrangements::intersection_points::{classify_all, DeferReason, PairClassification};
use crate::arrangements::retriangulate::{split_single_triangle, RetriangulateError};
use crate::arrangements::{FastTrimesh, FastTrimeshError, Plane};
use crate::labeled_arrangement::InputId;
use crate::predicates::{max_component_in_triangle_normal, points_are_collinear_3d, Axis};
use crate::processing::multiplier::{compute_multiplier, multiply_coordinates};
use cad_primitives::Point3;
use dashu::float::FBig;
use dashu::rational::RBig;

/// Per-output-triangle "which input solid(s) it lies on" — the set-of-solids
/// label. Reuses the existing `InputId` newtype (labeled_arrangement.rs).
/// Stored as a sorted-unique `Vec<InputId>` (the C++ `std::bitset<NBIT>`,
/// OR-merged across duplicate input triangles in prep, carried verbatim onto
/// every output sub-triangle of a parent base triangle).
pub type Label = Vec<InputId>;

/// The complete conforming triangle soup produced by the native arrangement.
///
/// `verts` holds every global vertex as typed coordinates (Explicit input
/// corners + interned Lpi/Tpi implicit points), with the 5 jolly points
/// appended at the tail. `tris` indexes into `verts`; `labels` is 1:1 with
/// `tris`. This is the pre-in/out, pre-patch_id soup (BL1 consumes it).
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangementSoup {
    pub verts: Vec<VertexCoords>,
    pub tris: Vec<[u32; 3]>,
    pub labels: Vec<Label>,
    /// Per output triangle (parallel to `tris`): the BASE input-soup triangle
    /// index/indices it descends from. Every output triangle is a sub-triangle
    /// of exactly one base input triangle, so this is normally a single index;
    /// it is MULTI-VALUED only at a coplanar overlap, where the two coincident
    /// base triangles (one per input solid) merge into one emitted triangle
    /// (the §4.5.5 shared sheet). This is the per-triangle provenance the
    /// Stage-2 contract specifies (`source`) — consumed by yang-rs to attribute
    /// each output triangle to its B-Rep face via the Stage-1 map, replacing
    /// geometric centroid-proximity (deviation N4). Base indices are in the
    /// concatenated soup space; the consumer maps them to `(InputId, local)`.
    pub source: Vec<Vec<u32>>,
    /// Count of jolly points appended at the tail of `verts` (always 5). The
    /// real arrangement vertices are `verts[..verts.len() - jolly_count]`.
    pub jolly_count: u32,
    /// The PREPPED ORIGINAL input triangles over the same welded vertex
    /// array (post vertex-merge, post degenerate/duplicate removal, with
    /// removed duplicates RESTORED as winding-corrected single-label copies
    /// — the C++ `arr_in_tris` after `addDuplicateTrisInfoInStructures`,
    /// booleans.cpp:358-393). BL2 ray-casting tests in/out against these
    /// closed input shells, not against the cut output triangles.
    pub in_tris: Vec<[u32; 3]>,
    /// Per-`in_tris` labels — `arr_in_labels`. Single-input per entry after
    /// duplicate restoration (each solid's shell is closed under its own
    /// label); the OR-merged multi-label survives only on `labels` (the
    /// keep-rule surface labels), exactly as in the C++.
    pub in_labels: Vec<Label>,
    /// The `compute_multiplier` scale factor applied to ALL coordinates in
    /// `verts` (a power of two; `1.0` means unscaled). Output emission
    /// (`computeFinalExplicitResult`) divides by it to descale — the C++
    /// reads it back from the last jolly point's X coordinate.
    pub multiplier: f64,
}

/// Loud failure surface — never silent (P9/P10). Wraps the deferred walls.
#[derive(Debug, PartialEq)]
pub enum ArrangementError {
    /// A candidate pair is coplanar / single-coplanar-edge (AR1
    /// `Deferred(Coplanar | SingleCoplanarEdge)`) — Stage 0 / M8.
    CoplanarPairDeferred {
        ta: u32,
        tb: u32,
        reason: DeferReason,
    },
    /// AR1 flagged a degenerate configuration that slipped past prep.
    DegeneratePairDeferred { ta: u32, tb: u32 },
    /// Point insertion located a point outside its base triangle (AR2a
    /// `RetriangulateError::NoContainingTriangle`).
    Retriangulate { base_tri: u32, point_id: u32 },
    /// A `Transversal` pair's intersection vertices resolved to MORE than two
    /// distinct GEOMETRIC endpoints (AR3c,
    /// `ConstraintSegmentError::TooManyGeometricEndpoints`). Impossible for a
    /// valid non-coplanar transversal pair — its intersection is one segment
    /// with two endpoints (the C++ pipeline's `final_check` asserts this) —
    /// so it indicates an upstream classification bug, surfaced loudly
    /// instead of silently dropping the pair's constraint segment.
    TransversalEndpointOvercount { ta: u32, tb: u32, count: usize },
    /// Constraint enforcement hit the AR3a global-state wall: a crossed
    /// constraint edge has no recorded supporting plane / TPI planes not in
    /// general position. THIS is the N16 deep-recursion / coplanar-jollyPoint
    /// deferral. Wraps `EnforceError::{SourcePlaneUnavailable, DegenerateTpi,
    /// SegmentNotLocatable, EndpointNotInSubmesh}`.
    DeepRecursionRequired { base_tri: u32, detail: EnforceError },
    /// Malformed caller input (bad triangle index, count overflow) surfaced by
    /// the global-soup `FastTrimesh::from_soup`.
    Input(FastTrimeshError),
    /// `labels.len()` != input triangle count.
    LabelCountMismatch { tris: usize, labels: usize },
}

// =========================================================================
// Input-prep helpers (port of `processing.cpp`)
// =========================================================================

/// Insertion-ordered dedup of input vertices by exact `[f64; 3]` equality,
/// remapping each triangle's three indices to the deduped global ids.
///
/// Port of `processing.cpp:67-119` (serial branch). The C++
/// `flat_hash_map<array<double,3>, uint>` is realized here as a small linear
/// interner over the deduped vertex list (no `f64: Eq`/`Hash`). Only vertices
/// referenced by some triangle survive (matches C++, which iterates `in_tris`).
/// Coordinates are bit-exact (post-scale, no tolerance).
pub fn merge_duplicated_vertices(
    coords: &[f64],
    tris: &[[u32; 3]],
) -> (Vec<Point3>, Vec<[u32; 3]>) {
    let n_in = coords.len() / 3;
    let coord_at = |slot: u32| -> Point3 {
        let i = slot as usize;
        Point3::new(coords[3 * i], coords[3 * i + 1], coords[3 * i + 2])
    };

    let mut verts: Vec<Point3> = Vec::new();
    // `remap[old_slot] = Some(new_id)` once seen; lazily filled.
    let mut remap: Vec<Option<u32>> = vec![None; n_in];

    let intern = |verts: &mut Vec<Point3>, p: Point3| -> u32 {
        // Bit-exact linear interner over the deduped list (matches the C++
        // exact-array hash map, minus the hash).
        if let Some(i) = verts.iter().position(|&q| q == p) {
            return i as u32;
        }
        verts.push(p);
        (verts.len() - 1) as u32
    };

    let mut remapped: Vec<[u32; 3]> = Vec::with_capacity(tris.len());
    for tri in tris {
        let mut out = [0u32; 3];
        for (k, &old) in tri.iter().enumerate() {
            let new_id = match remap.get(old as usize).and_then(|o| *o) {
                Some(id) => id,
                None => {
                    let id = intern(&mut verts, coord_at(old));
                    if let Some(slot) = remap.get_mut(old as usize) {
                        *slot = Some(id);
                    }
                    id
                }
            };
            out[k] = new_id;
        }
        remapped.push(out);
    }

    (verts, remapped)
}

/// One removed duplicate input triangle, recorded so it can be RESTORED into
/// the ray-cast in/out substrate (`in_tris`/`in_labels`) before labeling.
///
/// Port of the C++ `DuplTriInfo { t_id, l_id, w }` (booleans.h) produced by
/// `customRemoveDegenerateAndDuplicatedTriangles` (booleans.cpp:179-313) and
/// consumed by `addDuplicateTrisInfoInStructures` (booleans.cpp:358-393).
/// Coplanar-overlap regions (Yang §4.5.5 Stage-0 emits IDENTICAL meshes on
/// the overlap for both solids) dedup into ONE arrangement triangle with an
/// OR-merged label — correct for the OUTPUT surface labels, but the in/out
/// ray cast needs each input as a CLOSED single-label shell with its OWN
/// winding, so the removed copy is restored there.
#[derive(Clone, Debug, PartialEq)]
pub struct DuplTriInfo {
    /// Index (into the kept triangle array) of the surviving first copy —
    /// the C++ `t_id`.
    pub t_off: usize,
    /// The DUPLICATE's own label (the C++ single-bit `l_id`; kept as a
    /// `Label` set since the port's labels are id-sets).
    pub label: Label,
    /// Winding of the duplicate relative to the survivor — the C++ `w`
    /// (`consistentWinding`, booleans.cpp:1530-1539): `true` = same
    /// cyclic order, `false` = opposite.
    pub w: bool,
}

/// Port of `consistentWinding` (booleans.cpp:1530-1539): do two triangles
/// over the SAME vertex set share cyclic winding order?
fn consistent_winding(t0: &[u32; 3], t1: &[u32; 3]) -> bool {
    let Some(j) = (0..3).find(|&j| t1[j] == t0[0]) else {
        unreachable!("consistent_winding: not the same triangle");
    };
    t0[1] == t1[(j + 1) % 3] && t0[2] == t1[(j + 2) % 3]
}

/// Drop exact-collinear (degenerate) triangles and dedup sorted-vertex
/// duplicates, OR-merging duplicate labels into the first survivor.
///
/// Port of `customRemoveDegenerateAndDuplicatedTriangles`
/// (booleans.cpp:179-313). For each triangle in order:
/// - **Degenerate**: if `points_are_collinear_3d(v0, v1, v2)` (CR1 — exact),
///   drop it (and its label).
/// - **Duplicate**: key by the **sorted** `[v0, v1, v2]`. First occurrence
///   keeps the triangle (original winding) + label; a later duplicate is
///   dropped but its label is OR-merged (sorted-unique set-union of `InputId`s)
///   into the first occurrence's label, AND a [`DuplTriInfo`] records the
///   duplicate's own label + relative winding so `mesh_arrangement` can
///   restore it into the in/out substrate (the C++ `dupl_triangles`
///   out-param, booleans.cpp:233-247).
///
/// Output `tris` preserves first-seen order; `labels` is 1:1 with it.
#[allow(clippy::type_complexity)] // (kept_tris, kept_labels, dupl, kept_source) local prep tuple
pub fn remove_degenerate_and_duplicated_triangles(
    verts: &[Point3],
    tris: &[[u32; 3]],
    labels: &[Label],
) -> (Vec<[u32; 3]>, Vec<Label>, Vec<DuplTriInfo>, Vec<Vec<u32>>) {
    let mut kept_tris: Vec<[u32; 3]> = Vec::new();
    let mut kept_labels: Vec<Label> = Vec::new();
    let mut dupl: Vec<DuplTriInfo> = Vec::new();
    // Per kept triangle: the ORIGINAL input triangle index/indices it
    // represents — the survivor's own index, plus every exact-coincident
    // duplicate OR-merged into it. This is the per-triangle PROVENANCE the
    // Stage-2 `source` contract needs: a coplanar overlap collapses the two
    // inputs' coincident triangles into one survivor here, so its provenance is
    // multi-valued. Indices are in the input (concatenated A++B) triangle space.
    let mut kept_source: Vec<Vec<u32>> = Vec::new();
    // Sorted-vertex key → output index of the first occurrence.
    let mut seen: Vec<([u32; 3], usize)> = Vec::new();

    for (ti, tri) in tris.iter().enumerate() {
        let v0 = verts[tri[0] as usize];
        let v1 = verts[tri[1] as usize];
        let v2 = verts[tri[2] as usize];
        // Degenerate (exact-collinear) → drop.
        if points_are_collinear_3d(v0, v1, v2) {
            continue;
        }
        let mut key = *tri;
        key.sort_unstable();
        let label = labels.get(ti).cloned().unwrap_or_default();

        if let Some(&(_, out_idx)) = seen.iter().find(|(k, _)| *k == key) {
            // Duplicate: OR-merge its label into the survivor (sorted-unique)
            // and record the restoration info (booleans.cpp:233-247).
            or_merge_label(&mut kept_labels[out_idx], &label);
            kept_source[out_idx].push(ti as u32);
            dupl.push(DuplTriInfo {
                t_off: out_idx,
                label: sorted_unique_label(&label),
                w: consistent_winding(tri, &kept_tris[out_idx]),
            });
        } else {
            let out_idx = kept_tris.len();
            kept_tris.push(*tri);
            kept_labels.push(sorted_unique_label(&label));
            kept_source.push(vec![ti as u32]);
            seen.push((key, out_idx));
        }
    }

    (kept_tris, kept_labels, dupl, kept_source)
}

/// Sorted-unique copy of a label (set of `InputId`s, ascending by raw id).
fn sorted_unique_label(label: &[InputId]) -> Label {
    let mut out: Label = label.to_vec();
    out.sort_by_key(|i| i.0);
    out.dedup();
    out
}

/// OR-merge `src` into `dst` (set-union, kept sorted-unique by raw id).
fn or_merge_label(dst: &mut Label, src: &[InputId]) {
    for &id in src {
        if !dst.contains(&id) {
            dst.push(id);
        }
    }
    dst.sort_by_key(|i| i.0);
    dst.dedup();
}

// =========================================================================
// Per-triangle reference plane (spec §6)
// =========================================================================

/// Reference projection plane for base triangle with corners `c0,c1,c2`: drop
/// the dominant-normal axis (`max_component_in_triangle_normal` → `Axis`).
/// `Axis::X → Plane::YZ`, `Axis::Y → Plane::ZX`, `Axis::Z → Plane::XY`.
fn triangle_plane(c0: Point3, c1: Point3, c2: Point3) -> Plane {
    match max_component_in_triangle_normal(c0, c1, c2) {
        Axis::X => Plane::YZ,
        Axis::Y => Plane::ZX,
        Axis::Z => Plane::XY,
    }
}

// =========================================================================
// Global-id interner (spec §7)
// =========================================================================

/// Linear interner over the growing global `verts` keyed by structural
/// `VertexCoords` equality (bit-exact, no tolerance — `VertexCoords` is
/// `f64`-bearing so we cannot use a `HashMap`). Input corners are NOT interned
/// here — they already hold their global id (`soup.tri(t)[k]`) and are placed in
/// `verts` up front; only new implicit / non-corner points flow through this.
struct GlobalVerts {
    verts: Vec<VertexCoords>,
}

impl GlobalVerts {
    /// Intern a `VertexCoords` by structural equality; append on first sight.
    fn intern(&mut self, c: VertexCoords) -> u32 {
        if let Some(i) = self.verts.iter().position(|v| *v == c) {
            return i as u32;
        }
        self.verts.push(c);
        (self.verts.len() - 1) as u32
    }
}

// =========================================================================
// Jolly points (spec §8)
// =========================================================================

/// The 5 jolly points (each component × multiplier `m`), as explicit
/// `VertexCoords`. Port of `triangle_soup.cpp` `initJollyPoints`.
fn jolly_points(m: f64) -> [VertexCoords; 5] {
    let p = |x: f64, y: f64, z: f64| VertexCoords::Explicit(Point3::new(x * m, y * m, z * m));
    [
        p(0.942_809_041_58, 0.0, -0.333_333_333),
        p(-0.471_404_520_79, 0.816_496_580_92, -0.333_333_333),
        p(-0.471_404_520_79, -0.816_496_580_92, -0.333_333_333),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 0.0),
    ]
}

// =========================================================================
// Exact coplanar interior-area-overlap test (deviation N17 — see step 7)
// =========================================================================
//
// AR1 returns `Deferred(Coplanar | SingleCoplanarEdge)` for EVERY coplanar
// triangle pair, because it does not port the C++
// `checkSingleCoplanarEdgeIntersections` path (deviation N13). But the C++
// processes coplanar pairs normally: a coplanar pair that shares ONLY an edge
// or vertex (no positive-area overlap — e.g. the two triangles of one cube
// face, or a closed solid's adjacent faces) produces NO new intersection
// geometry and passes straight through. The spec §4 step-7 "any coplanar →
// Err" rule, applied literally, would reject every closed solid (each flat quad
// face is two edge-sharing coplanar triangles), contradicting the spec's own
// hand corpus (closed cubes / tetrahedra).
//
// We therefore extend the governing principle (spec §0/§9: a *Stage-0 coplanar
// OVERLAP between the two solids* is the deferral): a coplanar pair is deferred
// ONLY when its two triangles overlap in POSITIVE AREA (exact 2D test below).
// Edge/vertex-only coplanar pairs are benign and skipped silently, exactly as
// the C++ reference does (they yield no intersection vertices / segments). This
// is EXACT (pure `dashu` rationals, no tolerance) — it never masks a real
// overlap, so it preserves the loud-deferral guarantee (P9/P10): the spec's
// `coplanar_pair_is_loudly_deferred` fixture (Tb shifted into Ta's interior)
// has positive-area overlap and is still loudly deferred.

fn to_r(x: f64) -> RBig {
    // Total on finite f64; the soup's coordinates are finite by construction.
    let fb: Option<FBig> = FBig::try_from(x).ok();
    match fb.and_then(|fb| RBig::try_from(fb).ok()) {
        Some(r) => r,
        None => RBig::ZERO,
    }
}

fn r3(p: Point3) -> [RBig; 3] {
    [to_r(p.x()), to_r(p.y()), to_r(p.z())]
}

/// Exact signed area (×2) of `(a, b, c)` projected by dropping `axis`
/// (0=x → YZ, 1=y → ZX, 2=z → XY). `(b-a) × (c-a)` 2D determinant.
fn signed_area2(axis: usize, a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
    let (i, j) = match axis {
        0 => (1usize, 2usize),
        1 => (2, 0),
        _ => (0, 1),
    };
    let bx = &b[i] - &a[i];
    let by = &b[j] - &a[j];
    let cx = &c[i] - &a[i];
    let cy = &c[j] - &a[j];
    &(&bx * &cy) - &(&by * &cx)
}

/// Dominant-axis of the exact normal of `(a, b, c)` (largest |component|).
fn dominant_axis(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> usize {
    let cross = |u: &[RBig; 3], v: &[RBig; 3]| -> [RBig; 3] {
        [
            &(&u[1] * &v[2]) - &(&u[2] * &v[1]),
            &(&u[2] * &v[0]) - &(&u[0] * &v[2]),
            &(&u[0] * &v[1]) - &(&u[1] * &v[0]),
        ]
    };
    let sub = |u: &[RBig; 3], v: &[RBig; 3]| -> [RBig; 3] {
        [&u[0] - &v[0], &u[1] - &v[1], &u[2] - &v[2]]
    };
    let n = cross(&sub(b, a), &sub(c, a));
    let abs = |r: &RBig| {
        if r < &RBig::ZERO {
            -r.clone()
        } else {
            r.clone()
        }
    };
    let nx = abs(&n[0]);
    let ny = abs(&n[1]);
    let nz = abs(&n[2]);
    if nx >= ny && nx >= nz {
        0
    } else if ny >= nz {
        1
    } else {
        2
    }
}

/// Exact 2D strictly-inside-triangle test (projected to `axis`).
fn strictly_in_tri2(
    axis: usize,
    p: &[RBig; 3],
    a: &[RBig; 3],
    b: &[RBig; 3],
    c: &[RBig; 3],
) -> bool {
    let d0 = signed_area2(axis, a, b, p);
    let d1 = signed_area2(axis, b, c, p);
    let d2 = signed_area2(axis, c, a, p);
    let pos = d0 > RBig::ZERO && d1 > RBig::ZERO && d2 > RBig::ZERO;
    let neg = d0 < RBig::ZERO && d1 < RBig::ZERO && d2 < RBig::ZERO;
    pos || neg
}

/// Exact 2D proper open-segment-crossing test (projected to `axis`).
fn segments_properly_cross2(
    axis: usize,
    p0: &[RBig; 3],
    p1: &[RBig; 3],
    q0: &[RBig; 3],
    q1: &[RBig; 3],
) -> bool {
    let o1 = signed_area2(axis, p0, p1, q0);
    let o2 = signed_area2(axis, p0, p1, q1);
    let o3 = signed_area2(axis, q0, q1, p0);
    let o4 = signed_area2(axis, q0, q1, p1);
    let opp = |a: &RBig, b: &RBig| {
        (a > &RBig::ZERO && b < &RBig::ZERO) || (a < &RBig::ZERO && b > &RBig::ZERO)
    };
    opp(&o1, &o2) && opp(&o3, &o4)
}

/// EXACT: do the two coplanar triangles overlap in POSITIVE area? Returns
/// `false` for edge/vertex-only sharing (the benign coplanar case). Assumes the
/// two triangles are coplanar (AR1 already established this via `orient3d`).
fn coplanar_tris_overlap(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
    let axis = dominant_axis(&t0[0], &t0[1], &t0[2]);
    for p in t1.iter() {
        if strictly_in_tri2(axis, p, &t0[0], &t0[1], &t0[2]) {
            return true;
        }
    }
    for p in t0.iter() {
        if strictly_in_tri2(axis, p, &t1[0], &t1[1], &t1[2]) {
            return true;
        }
    }
    let edges = [(0usize, 1usize), (1, 2), (2, 0)];
    for (a, b) in edges {
        for (c, d) in edges {
            if segments_properly_cross2(axis, &t0[a], &t0[b], &t1[c], &t1[d]) {
                return true;
            }
        }
    }
    false
}

/// Exact orient3d of `p` against the plane through `(r, s, t)`:
/// `(p-r) · ((s-r) × (t-r))`. Zero ⇔ coplanar.
fn orient3d_r(p: &[RBig; 3], r: &[RBig; 3], s: &[RBig; 3], t: &[RBig; 3]) -> RBig {
    let sub = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    };
    let cross = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    };
    let n = cross(&sub(s, r), &sub(t, r));
    let pr = sub(p, r);
    &(&(&pr[0] * &n[0]) + &(&pr[1] * &n[1])) + &(&pr[2] * &n[2])
}

/// EXACT: does a `SingleCoplanarEdge` pair `(t0, t1)` introduce real geometry?
/// The triangles are non-coplanar; at most an edge of one lies in the other's
/// plane. The pair is a real intersection iff that coplanar edge passes through
/// the OTHER triangle's STRICT interior (or properly crosses one of its edges)
/// in their common plane. Shared-edge / boundary-touch only → benign (false).
fn single_coplanar_edge_introduces_geometry(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
    // For each triangle as "the plane owner", find the OTHER triangle's edge
    // whose BOTH endpoints lie in the owner's plane (orient3d == 0), then test
    // whether that edge meets the owner's strict interior in the shared plane.
    let check = |owner: &[[RBig; 3]; 3], other: &[[RBig; 3]; 3]| -> bool {
        let on_plane = |p: &[RBig; 3]| -> bool {
            orient3d_r(p, &owner[0], &owner[1], &owner[2]) == RBig::ZERO
        };
        let axis = dominant_axis(&owner[0], &owner[1], &owner[2]);
        let edges = [(0usize, 1usize), (1, 2), (2, 0)];
        for (i, j) in edges {
            if !(on_plane(&other[i]) && on_plane(&other[j])) {
                continue;
            }
            // The coplanar edge (other[i], other[j]) lies in the owner's plane.
            // Real geometry iff either endpoint is strictly inside the owner,
            // or the edge properly crosses one of the owner's edges.
            if strictly_in_tri2(axis, &other[i], &owner[0], &owner[1], &owner[2])
                || strictly_in_tri2(axis, &other[j], &owner[0], &owner[1], &owner[2])
            {
                return true;
            }
            let oe = [(0usize, 1usize), (1, 2), (2, 0)];
            for (a, b) in oe {
                if segments_properly_cross2(axis, &other[i], &other[j], &owner[a], &owner[b]) {
                    return true;
                }
            }
        }
        false
    };
    check(t0, t1) || check(t1, t0)
}

/// Decide whether an AR1-`Deferred(Coplanar | SingleCoplanarEdge)` pair
/// represents a REAL intersection that must be loud-deferred (Stage-0 / M8), or
/// is BENIGN and may pass through silently (deviation N17). Pure-exact, no
/// tolerance.
///
/// - **Coplanar pair**: defer iff the two triangles overlap in POSITIVE AREA
///   (exact 2D test); edge/vertex-only touches (adjacent / co-face triangles
///   of a solid) → benign.
/// - **SingleCoplanarEdge pair** (non-coplanar): defer iff the coplanar edge
///   passes through the other triangle's strict interior or properly crosses an
///   edge of it; a boundary/shared-edge touch (a solid's adjacent faces) →
///   benign.
fn deferred_pair_must_defer(soup: &FastTrimesh, ta: u32, tb: u32, reason: DeferReason) -> bool {
    let t0 = [
        r3(soup.tri_vert(ta, 0)),
        r3(soup.tri_vert(ta, 1)),
        r3(soup.tri_vert(ta, 2)),
    ];
    let t1 = [
        r3(soup.tri_vert(tb, 0)),
        r3(soup.tri_vert(tb, 1)),
        r3(soup.tri_vert(tb, 2)),
    ];
    match reason {
        DeferReason::Coplanar => coplanar_tris_overlap(&t0, &t1),
        DeferReason::SingleCoplanarEdge => single_coplanar_edge_introduces_geometry(&t0, &t1),
        DeferReason::Degenerate => true,
    }
}

// =========================================================================
// Geometric point identity (deviation N18 — RESOLVED at source in AR3c)
// =========================================================================
//
// Two LPI/TPI points with DIFFERENT generator tuples can denote the SAME
// exact geometric point (e.g. the point where an A-edge meets a B-face ON
// one of A's own edges is reached both as `Lpi{ A-edge, B-plane }` and as
// `Lpi{ B-edge, A-plane }`, presentation-dependently — AR1's `li.size() > 1`
// early-out). N18 originally repaired this POST-HOC here (a
// `canonicalize_points` rewrite after grouping), which ran too late for
// `group_constraint_segments`: a pair over-counting to 3 structural ids had
// already silently dropped its constraint segment from both triangles
// (input-order-dependent fence gaps → BL1 flood leaks). PR-CR-AR3c moved
// geometric identity INTO the interner (`aux_structure::PointInterner`,
// keyed by exact rational coordinates, mirroring the C++
// `aux_structure.cpp:230 addVertexInSortedList` /
// `genericPoint::lessThan`), so the `points` set arriving here already has
// one id — one representative `VertexCoords` — per geometric point, and the
// downstream structural-equality consumers (`split_single_triangle` dedup,
// `enforce_constraints` endpoint resolution, the §7 global weld) coincide
// with geometric identity by construction.

// =========================================================================
// Orchestration (port of `meshArrangementPipeline`)
// =========================================================================

/// Build the native mesh arrangement for one triangle soup with per-triangle
/// input-solid labels.
///
/// `coords`: flat xyz triples (`len % 3 == 0`). `tris`: index triples into the
/// vertex list. `in_labels`: 1:1 with `tris`, each the set of input solids that
/// triangle belongs to (for a binary A∪B: A's tris carry `[InputId(0)]`, B's
/// `[InputId(1)]`).
///
/// Port of `solve_intersections.cpp::meshArrangementPipeline`, see
/// `specs/pr_cr_ar3b_global_soup.md` §4.
pub fn mesh_arrangement(
    coords: &[f64],
    tris: &[[u32; 3]],
    in_labels: &[Label],
) -> Result<ArrangementSoup, ArrangementError> {
    // 0. Loud caller-input check (count alignment).
    if in_labels.len() != tris.len() {
        return Err(ArrangementError::LabelCountMismatch {
            tris: tris.len(),
            labels: in_labels.len(),
        });
    }

    // 1. (Removed at PR-CR-M7c.) The pre-M7c FFI-stub refusal and `init_fpu`
    //    FPU-mode setup are gone: the clean-room native predicates are pure
    //    Rust — always available, no FPU rounding-mode requirement.

    // 2. Multiplier — scale a copy of the coordinates; all downstream geometry
    //    uses the scaled copy.
    let m = compute_multiplier(coords);
    let mut sc = coords.to_vec();
    multiply_coordinates(&mut sc, m);

    // 3. Merge duplicated input vertices (prep).
    let (verts, remapped_tris) = merge_duplicated_vertices(&sc, tris);

    // 4. Remove degenerate / duplicate input triangles (prep, labels
    //    OR-merged, duplicates recorded for the in/out restoration below).
    //    `kept_source[t]` carries the ORIGINAL input triangle index/indices each
    //    kept base triangle represents (multi-valued where a coplanar overlap
    //    collapsed the two inputs' coincident triangles) — the provenance the
    //    step-9 emit loop attaches to every output sub-triangle.
    let (kept_tris, kept_labels, dupl_triangles, kept_source) =
        remove_degenerate_and_duplicated_triangles(&verts, &remapped_tris, in_labels);

    // 5. Build the global soup. `from_soup` takes ONE plane; the per-triangle
    //    submeshes get their own correct plane (step 9), so any plane works for
    //    the global container.
    let soup =
        FastTrimesh::from_soup(&verts, &kept_tris, Plane::XY).map_err(ArrangementError::Input)?;

    // 6. Detect candidate intersecting pairs (CR13).
    let pairs = detect_intersecting_pairs(&soup);

    // 7. Classify each pair (AR1). Coplanar / degenerate → loud Err.
    let classified = classify_all(&soup, &pairs);
    for ((ta, tb), classification) in &classified {
        match classification {
            PairClassification::Deferred(reason) => match reason {
                DeferReason::Coplanar | DeferReason::SingleCoplanarEdge => {
                    // Deviation N17: defer ONLY a real intersection AR1 cannot
                    // construct (a Stage-0 / M8 case). Adjacent faces sharing an
                    // edge, and edge/vertex-only coplanar touches, are benign and
                    // pass through, matching the C++ reference. EXACT, no
                    // tolerance.
                    if deferred_pair_must_defer(&soup, *ta, *tb, *reason) {
                        return Err(ArrangementError::CoplanarPairDeferred {
                            ta: *ta,
                            tb: *tb,
                            reason: *reason,
                        });
                    }
                }
                DeferReason::Degenerate => {
                    return Err(ArrangementError::DegeneratePairDeferred { ta: *ta, tb: *tb });
                }
            },
            // PR-4: a fully-coplanar pair that AR1/AR2 CONSTRUCTED (vertices +
            // segments) is now wired into the split path + pocket dedup below
            // (no longer escalated to CoplanarPairDeferred). A coplanar pair
            // that genuinely could not be constructed is recorded as
            // `Deferred(Coplanar)` (handled above), not `Coplanar`, so it still
            // defers loudly — this arm only sees constructible pairs.
            PairClassification::Coplanar { .. } => {}
            PairClassification::Transversal { .. } | PairClassification::Disjoint => {}
        }
    }

    // 8. Group intersection points + constraint segments. Point identity is
    //    GEOMETRIC at the interner (AR3c, supersedes the post-hoc N18
    //    canonicalization): `points` carries one id — one representative
    //    `VertexCoords` — per exact geometric point, so coincident LPI/TPI
    //    points reached via different generator tuples already share an
    //    identity downstream (re-triangulate / enforce / global weld), and
    //    `group_constraint_segments` resolves endpoints by the same geometric
    //    keying. A pair resolving to >2 geometric endpoints is loud (the C++
    //    final_check assert), never a silently dropped segment.
    let (mut points, mut buckets) = group_intersection_points(&soup, &classified);
    let mut segments_per_tri = group_constraint_segments(&soup, &classified, &points).map_err(
        |ConstraintSegmentError::TooManyGeometricEndpoints { ta, tb, count }| {
            ArrangementError::TransversalEndpointOvercount { ta, tb, count }
        },
    )?;

    // 8b. (PR-4) Fold the fully-coplanar overlap geometry into the SAME
    //     buckets/segments the transversal path uses, so coplanar triangles
    //     get their overlap-boundary points + segments and enter the split
    //     path. `coplanar_tris` marks which base triangles route through the
    //     pocket-dedup emit (step 9). Geometric point identity is preserved by
    //     merging into the existing `points` interner by exact coords.
    let CoplanarIntegration {
        adjacency: _coplanar_adj,
        coplanar_tris,
    } = integrate_coplanar_into_arrangement(
        &soup,
        &classified,
        &mut points,
        &mut buckets,
        &mut segments_per_tri,
    );

    // The global vertex list seeds with the deduped input corners (Explicit),
    // in their existing global-id order; new implicit points append on demand.
    let mut globals = GlobalVerts {
        verts: verts.iter().map(|&p| VertexCoords::Explicit(p)).collect(),
    };
    let mut out_tris: Vec<[u32; 3]> = Vec::new();
    let mut out_labels: Vec<Label> = Vec::new();
    // Per emitted output triangle: the base input-soup triangle index/indices it
    // came from (parallel to `out_tris`). One index per triangle normally; a
    // coplanar overlap pocket gets the partner base triangle OR-appended below.
    let mut out_source: Vec<Vec<u32>> = Vec::new();

    // (PR-4) Global pocket dedup state, threaded across the whole step-9 loop
    // (one instance). `registry` maps a pocket's GLOBAL boundary-vertex SET to
    // the position at which its sub-triangles were first emitted; `pocket_pos`
    // records the actual out-position LIST for each pocket key, so a repeat
    // OR-merges the label into exactly those positions (more robust than the
    // C++ `size()-2` arithmetic — handles interior vertices in the pocket).
    let mut pocket_registry = VisitedPocketRegistry::new();
    let mut pocket_pos: BTreeMap<BTreeSet<u32>, Vec<usize>> = BTreeMap::new();

    // 9. Per base triangle: fast path (pass-through) or split + enforce.
    for t in 0..soup.num_tris() {
        let bucket = &buckets[t as usize];
        let no_points = bucket.interior.is_empty() && bucket.edges.iter().all(|e| e.is_empty());
        let no_segments = segments_per_tri[t as usize].is_empty();

        // Fast path: untouched base triangle → emit straight through.
        if no_points && no_segments {
            out_tris.push(soup.tri(t));
            out_labels.push(kept_labels[t as usize].clone());
            out_source.push(kept_source[t as usize].clone());
            continue;
        }

        // Split path: build a 1-triangle submesh with the correct plane.
        let c0 = soup.tri_vert(t, 0);
        let c1 = soup.tri_vert(t, 1);
        let c2 = soup.tri_vert(t, 2);
        let plane_t = triangle_plane(c0, c1, c2);
        let mut subm = FastTrimesh::from_soup(&[c0, c1, c2], &[[0u32, 1, 2]], plane_t)
            .map_err(ArrangementError::Input)?;

        // Flat point list for this base triangle: interior ++ each edge's
        // points, resolved from the geometrically-interned `points` by id
        // (AR2a fed interior + on-edge as ONE flat slice). Dedup by structural
        // `VertexCoords` equality — which now COINCIDES with geometric
        // identity (one representative per geometric point, AR3c) — so
        // coincident points insert as ONE submesh vertex.
        let mut flat: Vec<TypedPoint> = Vec::new();
        let push_unique = |flat: &mut Vec<TypedPoint>, tp: TypedPoint| {
            if !flat.iter().any(|e| e.coords == tp.coords) {
                flat.push(tp);
            }
        };
        for &id in &bucket.interior {
            push_unique(&mut flat, points[id as usize].clone());
        }
        for edge in &bucket.edges {
            for &id in edge {
                push_unique(&mut flat, points[id as usize].clone());
            }
        }

        split_single_triangle(&mut subm, &flat).map_err(|e| match e {
            RetriangulateError::NoContainingTriangle { point_id } => {
                ArrangementError::Retriangulate {
                    base_tri: t,
                    point_id,
                }
            }
        })?;

        // No degenerate point-segments can arrive here: with geometric
        // interning a tangential touch resolves to ONE endpoint id and emits
        // no segment at all in `group_constraint_segments` (the former
        // post-split coords filter is vacuous and was removed in AR3c —
        // matches the C++ which never emits a point-segment).
        enforce_constraints(&mut subm, &segments_per_tri[t as usize], &points).map_err(
            |detail| {
                // Diagnostic probe (env-gated, zero-cost off): dump the failing
                // base triangle, its submesh vertices, and the pending
                // constraint segments so an enforcement wall is localizable
                // without a debugger.
                if std::env::var_os("CHERCHI_ENFORCE_PROBE").is_some() {
                    eprintln!("[enforce-probe] base_tri={t} corners:");
                    eprintln!("  c0={c0:?}\n  c1={c1:?}\n  c2={c2:?}");
                    eprintln!("[enforce-probe] submesh verts ({}):", subm.num_verts());
                    for v in 0..subm.num_verts() {
                        eprintln!("  v{v}: {:?} ~ {:?}", subm.vert_coords(v), subm.vert(v));
                    }
                    eprintln!(
                        "[enforce-probe] segments ({}):",
                        segments_per_tri[t as usize].len()
                    );
                    for s in &segments_per_tri[t as usize] {
                        eprintln!(
                            "  seg endpoints=({},{}) coords=({:?}, {:?})",
                            s.endpoints.0,
                            s.endpoints.1,
                            points[s.endpoints.0 as usize].coords,
                            points[s.endpoints.1 as usize].coords,
                        );
                    }
                }
                ArrangementError::DeepRecursionRequired {
                    base_tri: t,
                    detail,
                }
            },
        )?;

        // Assemble: map each submesh vertex → a global id (§7 weld).
        let base_corners = [c0, c1, c2];
        let base_global_ids = soup.tri(t);
        let weld_local = |lv: u32, globals: &mut GlobalVerts| -> u32 {
            weld_vertex(
                subm.vert_coords(lv),
                &base_corners,
                &base_global_ids,
                globals,
            )
        };
        let label = kept_labels[t as usize].clone();

        if coplanar_tris.contains(&t) {
            // (PR-4) Pocket path — port of solvePocketsInCoplanarTriangle
            // (triangulation.cpp:1226). Flood-fill the submesh into pockets
            // bounded by constraint/border edges; key each pocket by its
            // GLOBAL boundary-vertex set. A new key emits the pocket's
            // sub-triangles (recording their out-positions); a SEEN key (the
            // coplanar partner's identical overlap pocket) is NOT re-emitted —
            // its label is OR-merged into the recorded positions. THIS is the
            // dedup that prevents the overlap double-count.
            for pocket in find_pockets_in_triangle(&subm) {
                // GLOBAL boundary-vertex set (welds both coplanar triangles'
                // shared overlap boundary to the SAME global ids → same key).
                let mut boundary_global: BTreeSet<u32> = BTreeSet::new();
                for &lv in &pocket.boundary_verts {
                    boundary_global.insert(weld_local(lv, &mut globals));
                }
                let boundary_vec: Vec<u32> = boundary_global.iter().copied().collect();

                match pocket_registry.add_visited_polygon_pocket(&boundary_vec, out_tris.len()) {
                    None => {
                        // New pocket → emit its sub-triangles, record positions.
                        let mut positions: Vec<usize> = Vec::with_capacity(pocket.sub_tris.len());
                        for &st in &pocket.sub_tris {
                            let local = subm.tri(st);
                            let global = [
                                weld_local(local[0], &mut globals),
                                weld_local(local[1], &mut globals),
                                weld_local(local[2], &mut globals),
                            ];
                            positions.push(out_tris.len());
                            out_tris.push(global);
                            out_labels.push(label.clone());
                            out_source.push(kept_source[t as usize].clone());
                        }
                        pocket_pos.insert(boundary_global, positions);
                    }
                    Some(_) => {
                        // Already emitted (the partner's identical overlap
                        // pocket) → OR-merge the label into every recorded
                        // out-position; do NOT re-emit (no double-count). The
                        // partner base triangle `t` (the OTHER input's coincident
                        // face) joins each merged triangle's provenance, making
                        // `source` multi-valued for the shared overlap sheet.
                        if let Some(positions) = pocket_pos.get(&boundary_global) {
                            for &p in positions {
                                or_merge_label(&mut out_labels[p], &label);
                                for &src in &kept_source[t as usize] {
                                    if !out_source[p].contains(&src) {
                                        out_source[p].push(src);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Non-coplanar triangle: plain per-sub-triangle emit (unchanged).
            for st in 0..subm.num_tris() {
                let local = subm.tri(st);
                let global = [
                    weld_local(local[0], &mut globals),
                    weld_local(local[1], &mut globals),
                    weld_local(local[2], &mut globals),
                ];
                out_tris.push(global);
                out_labels.push(label.clone());
                out_source.push(kept_source[t as usize].clone());
            }
        }
    }

    // 10. Append the 5 jolly points at the tail.
    let jolly = jolly_points(m);
    globals.verts.extend_from_slice(&jolly);

    // 11. Restore removed duplicates into the in/out substrate — the port of
    //     `addDuplicateTrisInfoInStructures` (booleans.cpp:358-393). The
    //     OUTPUT labels (`out_labels`) keep the OR-merged multi-label (the
    //     keep-rule input), but the BL2 ray cast needs each input as a
    //     CLOSED single-label shell: a merged {A,B} in-label is skipped by
    //     the prune for BOTH solids' patches (`tested_label ∩
    //     patch_surface_label`, booleans.cpp:680), leaving both shells open
    //     at the overlap, and the surviving copy carries only the FIRST
    //     solid's winding, breaking the back-face orientation verdict for
    //     the other. So: append a fresh copy with the duplicate's OWN label
    //     and `w`-corrected winding (cpp:375-386), and remove that label
    //     from the survivor (cpp:390).
    let mut arr_in_tris = kept_tris;
    let mut arr_in_labels = kept_labels;
    for d in &dupl_triangles {
        let [v0, v1, v2] = arr_in_tris[d.t_off];
        arr_in_tris.push(if d.w { [v0, v1, v2] } else { [v0, v2, v1] });
        arr_in_labels.push(d.label.clone());
        arr_in_labels[d.t_off].retain(|id| !d.label.contains(id));
    }

    // 12. Return the assembled soup.
    Ok(ArrangementSoup {
        verts: globals.verts,
        tris: out_tris,
        labels: out_labels,
        source: out_source,
        jolly_count: 5,
        in_tris: arr_in_tris,
        in_labels: arr_in_labels,
        multiplier: m,
    })
}

/// Map one submesh vertex's coordinates to a GLOBAL vertex id (spec §7 weld).
///
/// - An `Explicit(p)` coinciding (exact) with one of the base triangle's three
///   corners maps to that corner's pre-assigned global input id.
/// - Any other vertex (Lpi/Tpi or a non-corner Explicit) is interned into the
///   growing global `verts` by structural `VertexCoords` equality.
fn weld_vertex(
    coords: &VertexCoords,
    base_corners: &[Point3; 3],
    base_global_ids: &[u32; 3],
    globals: &mut GlobalVerts,
) -> u32 {
    if let VertexCoords::Explicit(p) = coords {
        for (k, &corner) in base_corners.iter().enumerate() {
            if *p == corner {
                return base_global_ids[k];
            }
        }
    }
    globals.intern(*coords)
}

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR3b (`mesh_arrangement` + `ArrangementSoup` +
    //! prep). These exercise the intended GREEN behaviour through the public
    //! surface the GREEN implementer WILL add — none of which exists yet, so
    //! this module currently FAILS TO COMPILE/RESOLVE against the not-yet-
    //! written API. No production code is authored in this PR.

    use crate::arrangements::fast_trimesh::VertexCoords;
    // Public surface the spec mandates GREEN re-exports from `arrangements`:
    use crate::arrangements::{mesh_arrangement, ArrangementError, ArrangementSoup, Label};
    // Prep helpers GREEN makes `pub` in this very module:
    use crate::arrangements::soup::{
        merge_duplicated_vertices, remove_degenerate_and_duplicated_triangles,
    };
    use crate::labeled_arrangement::InputId;
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // ════════════════════════════════════════════════════════════════
    // Exact-rational helpers (pure dashu — independent of the FFI).
    // Copied in style from retriangulate.rs / enforce.rs test modules.
    // ════════════════════════════════════════════════════════════════

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }

    fn sub3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    }
    fn cross3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    }
    fn dot3(a: &[RBig; 3], b: &[RBig; 3]) -> RBig {
        &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
    }

    /// Exact coordinates of a stored `VertexCoords`.
    ///
    /// `Explicit(p)` → exact rationals of p. `Lpi { line:[p,q], plane:[r,s,t] }`
    /// → the EXACT line-plane intersection (point on `p + u(q-p)` lying in the
    /// plane through `r,s,t`, `u = dot(r-p,n)/dot(q-p,n)`, `n = (s-r)×(t-r)`).
    /// `Tpi { v,w,u }` → the EXACT common intersection of the three supporting
    /// planes (Cramer's rule). All in `RBig`. The hand corpus is designed to
    /// avoid Tpi (so #1 needs only line∩plane), but the arm is included for
    /// completeness / safety.
    fn exact_coords(c: &VertexCoords) -> [RBig; 3] {
        let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
        match c {
            VertexCoords::Explicit(p) => to_r3(p),
            VertexCoords::Lpi { line, plane } => {
                let p = to_r3(&line[0]);
                let q = to_r3(&line[1]);
                let r = to_r3(&plane[0]);
                let s = to_r3(&plane[1]);
                let t = to_r3(&plane[2]);
                let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                let rp = sub3(&r, &p);
                let qp = sub3(&q, &p);
                let num = dot3(&rp, &n);
                let den = dot3(&qp, &n);
                assert!(
                    den != RBig::ZERO,
                    "exact_coords: LPI line parallel to plane (den == 0) — bad fixture"
                );
                let u = &num / &den;
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { v, w, u } => {
                let plane_eqn = |tri: &[Point3; 3]| -> ([RBig; 3], RBig) {
                    let r = to_r3(&tri[0]);
                    let s = to_r3(&tri[1]);
                    let t = to_r3(&tri[2]);
                    let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                    let d = dot3(&n, &r);
                    (n, d)
                };
                let (n0, d0) = plane_eqn(v);
                let (n1, d1) = plane_eqn(w);
                let (n2, d2) = plane_eqn(u);
                let det_rows = |r0: &[RBig; 3], r1: &[RBig; 3], r2: &[RBig; 3]| -> RBig {
                    dot3(r0, &cross3(r1, r2))
                };
                let det = det_rows(&n0, &n1, &n2);
                assert!(
                    det != RBig::ZERO,
                    "exact_coords: TPI planes not in general position (det == 0) — bad fixture"
                );
                let rhs = [d0, d1, d2];
                let sub_col = |k: usize| -> [[RBig; 3]; 3] {
                    let mut rows = [n0.clone(), n1.clone(), n2.clone()];
                    rows[0][k] = rhs[0].clone();
                    rows[1][k] = rhs[1].clone();
                    rows[2][k] = rhs[2].clone();
                    rows
                };
                let mx = sub_col(0);
                let my = sub_col(1);
                let mz = sub_col(2);
                [
                    &det_rows(&mx[0], &mx[1], &mx[2]) / &det,
                    &det_rows(&my[0], &my[1], &my[2]) / &det,
                    &det_rows(&mz[0], &mz[1], &mz[2]) / &det,
                ]
            }
        }
    }

    /// Exact signed area (× 2) of a triangle PROJECTED to the plane dropping the
    /// `axis`-th coordinate (0=x → YZ, 1=y → ZX, 2=z → XY). Returns the 2D
    /// determinant `(b-a) × (c-a)` in `RBig` (== twice the projected signed
    /// area). Used both for non-degeneracy (≠ 0 under the dominant axis) and for
    /// covering checks.
    fn exact_signed_area2(axis: usize, a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
        // The two surviving coordinate indices after dropping `axis`.
        let (i, j) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let bx = &b[i] - &a[i];
        let by = &b[j] - &a[j];
        let cx = &c[i] - &a[i];
        let cy = &c[j] - &a[j];
        &(&bx * &cy) - &(&by * &cx)
    }

    /// The dominant-axis of a triangle's exact normal (index of the largest
    /// |component|). 0=x,1=y,2=z. Used to pick a non-degenerate 2D projection.
    fn dominant_axis(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> usize {
        let n = cross3(&sub3(b, a), &sub3(c, a));
        let abs = |r: &RBig| {
            if r < &RBig::ZERO {
                -r.clone()
            } else {
                r.clone()
            }
        };
        let nx = abs(&n[0]);
        let ny = abs(&n[1]);
        let nz = abs(&n[2]);
        if nx >= ny && nx >= nz {
            0
        } else if ny >= nz {
            1
        } else {
            2
        }
    }

    /// Exact coords of every triangle of a soup, as `[[RBig;3];3]`.
    fn tri_exact(soup: &ArrangementSoup, t: usize) -> [[RBig; 3]; 3] {
        let [a, b, c] = soup.tris[t];
        [
            exact_coords(&soup.verts[a as usize]),
            exact_coords(&soup.verts[b as usize]),
            exact_coords(&soup.verts[c as usize]),
        ]
    }

    // ── Exact 3D tri-tri interior-intersection test (invariant #1) ────
    //
    // Two triangles "overlap in their interiors" iff they share a region of
    // positive area. We test this EXACTLY by: (1) if the two triangles are
    // coplanar, run an exact 2D overlap test (segment crossings + interior
    // containment) in their common dominant-axis projection; (2) if not
    // coplanar, they can only meet along a 1D segment (measure-zero, NOT an
    // interior area overlap), so they do NOT overlap. A conforming soup has NO
    // interior-overlapping pair.

    /// True iff all four points are coplanar (exact orient3d == 0).
    fn coplanar4(p: &[RBig; 3], a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> bool {
        // orient3d = (a-p)·((b-p)×(c-p)); zero ⇔ coplanar.
        let det = dot3(&sub3(a, p), &cross3(&sub3(b, p), &sub3(c, p)));
        det == RBig::ZERO
    }

    /// Are two triangles (each 3 exact pts) coplanar with each other?
    fn tris_coplanar(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        // Every vertex of t1 lies in the plane of t0.
        t1.iter().all(|p| coplanar4(&t0[0], &t0[1], &t0[2], p))
    }

    /// Exact 2D point-strictly-inside-triangle (projected to `axis`). Strict:
    /// returns false on the boundary.
    fn point_strictly_in_tri2(
        axis: usize,
        p: &[RBig; 3],
        a: &[RBig; 3],
        b: &[RBig; 3],
        c: &[RBig; 3],
    ) -> bool {
        let d0 = exact_signed_area2(axis, a, b, p);
        let d1 = exact_signed_area2(axis, b, c, p);
        let d2 = exact_signed_area2(axis, c, a, p);
        let pos = d0 > RBig::ZERO && d1 > RBig::ZERO && d2 > RBig::ZERO;
        let neg = d0 < RBig::ZERO && d1 < RBig::ZERO && d2 < RBig::ZERO;
        pos || neg
    }

    /// Exact 2D open-segment proper-crossing test (projected to `axis`): do the
    /// open segments (p0,p1) and (q0,q1) cross at a single interior point? Uses
    /// strict orientation sign opposition on both sides (proper crossing only —
    /// shared endpoints / collinear overlap are NOT a proper crossing, so a
    /// conforming soup's shared edges don't count as interior overlap).
    fn segments_properly_cross2(
        axis: usize,
        p0: &[RBig; 3],
        p1: &[RBig; 3],
        q0: &[RBig; 3],
        q1: &[RBig; 3],
    ) -> bool {
        let o1 = exact_signed_area2(axis, p0, p1, q0);
        let o2 = exact_signed_area2(axis, p0, p1, q1);
        let o3 = exact_signed_area2(axis, q0, q1, p0);
        let o4 = exact_signed_area2(axis, q0, q1, p1);
        let opp = |a: &RBig, b: &RBig| {
            (a > &RBig::ZERO && b < &RBig::ZERO) || (a < &RBig::ZERO && b > &RBig::ZERO)
        };
        opp(&o1, &o2) && opp(&o3, &o4)
    }

    /// EXACT test: do triangles `t0` and `t1` overlap in their INTERIORS
    /// (share positive area)? Used to assert the conforming-soup invariant #1.
    fn tris_interiors_overlap(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        // Non-coplanar triangles can only meet along a measure-zero segment.
        if !tris_coplanar(t0, t1) {
            return false;
        }
        // Coplanar: project to the common dominant axis and do an exact 2D
        // overlap test.
        let axis = dominant_axis(&t0[0], &t0[1], &t0[2]);
        // (a) any vertex of one strictly inside the other.
        for p in t1.iter() {
            if point_strictly_in_tri2(axis, p, &t0[0], &t0[1], &t0[2]) {
                return true;
            }
        }
        for p in t0.iter() {
            if point_strictly_in_tri2(axis, p, &t1[0], &t1[1], &t1[2]) {
                return true;
            }
        }
        // (b) any pair of edges properly crosses.
        let e0 = [(0usize, 1usize), (1, 2), (2, 0)];
        for (a, b) in e0 {
            for (c, d) in e0 {
                if segments_properly_cross2(axis, &t0[a], &t0[b], &t1[c], &t1[d]) {
                    return true;
                }
            }
        }
        false
    }

    /// Assert no two output triangles of `soup` overlap in their interiors
    /// (invariant #1). Real (non-jolly) triangles only.
    fn assert_conforming(soup: &ArrangementSoup) {
        let n = soup.tris.len();
        let exacts: Vec<[[RBig; 3]; 3]> = (0..n).map(|t| tri_exact(soup, t)).collect();
        for a in 0..n {
            for b in (a + 1)..n {
                assert!(
                    !tris_interiors_overlap(&exacts[a], &exacts[b]),
                    "output triangles {a} and {b} overlap in their interiors — soup not conforming"
                );
            }
        }
    }

    /// Assert every output triangle is non-degenerate (exact area ≠ 0 under its
    /// dominant axis) — invariant #3.
    fn assert_no_degenerate_tris(soup: &ArrangementSoup) {
        for t in 0..soup.tris.len() {
            let [a, b, c] = tri_exact(soup, t);
            let axis = dominant_axis(&a, &b, &c);
            assert!(
                exact_signed_area2(axis, &a, &b, &c) != RBig::ZERO,
                "output triangle {t} is degenerate (exact zero area)"
            );
        }
    }

    /// Number of distinct welded global ids actually referenced by triangles.
    fn referenced_vertex_count(soup: &ArrangementSoup) -> usize {
        let mut ids: Vec<u32> = soup.tris.iter().flatten().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// Assert the jolly tail: exactly 5 jolly points appended, NOT referenced by
    /// any triangle. Returns the count of real (non-jolly) verts.
    fn assert_jolly_tail(soup: &ArrangementSoup) -> usize {
        assert_eq!(soup.jolly_count, 5, "jolly_count must be exactly 5");
        let n = soup.verts.len();
        assert!(n >= 5, "verts must include the 5 jolly points");
        let real = n - 5;
        // No triangle references a jolly id (>= real).
        for tri in &soup.tris {
            for &id in tri {
                assert!(
                    (id as usize) < real,
                    "triangle references a jolly point id {id} (real verts = {real})"
                );
            }
        }
        real
    }

    /// Assert `labels` is 1:1 with `tris`.
    fn assert_label_alignment(soup: &ArrangementSoup) {
        assert_eq!(
            soup.tris.len(),
            soup.labels.len(),
            "labels must be 1:1 with tris"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Hand corpus — closed solids as triangle soups (flat coords + tris +
    // per-triangle labels). All transversal / non-coplanar so they avoid the
    // TPI path (no constraint X-crossing): a simple two-box / two-tetra overlap
    // whose per-face intersection is a single non-self-crossing segment yields
    // only Explicit + Lpi vertices.
    // ════════════════════════════════════════════════════════════════

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    /// Axis-aligned unit cube `[x0,x0+1]×[y0,y0+1]×[z0,z0+1]` as 8 corners + 12
    /// triangles, with every triangle carrying `label`. Returns (flat coords,
    /// tris, labels). Outward-CCW winding.
    fn cube(
        x0: f64,
        y0: f64,
        z0: f64,
        side: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let x1 = x0 + side;
        let y1 = y0 + side;
        let z1 = z0 + side;
        // 8 corners, ids 0..8.
        let corners = [
            (x0, y0, z0), // 0
            (x1, y0, z0), // 1
            (x1, y1, z0), // 2
            (x0, y1, z0), // 3
            (x0, y0, z1), // 4
            (x1, y0, z1), // 5
            (x1, y1, z1), // 6
            (x0, y1, z1), // 7
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        // 12 outward-facing triangles (2 per face).
        let tris = vec![
            // bottom z=z0 (normal -z)
            [0, 2, 1],
            [0, 3, 2],
            // top z=z1 (normal +z)
            [4, 5, 6],
            [4, 6, 7],
            // front y=y0 (normal -y)
            [0, 1, 5],
            [0, 5, 4],
            // back y=y1 (normal +y)
            [3, 7, 6],
            [3, 6, 2],
            // left x=x0 (normal -x)
            [0, 4, 7],
            [0, 7, 3],
            // right x=x1 (normal +x)
            [1, 2, 6],
            [1, 6, 5],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    /// A regular-ish tetrahedron: 4 corners + 4 triangles, all `label`.
    /// `o` is the apex-origin; spans roughly `[o, o+s]`.
    fn tetra(
        ox: f64,
        oy: f64,
        oz: f64,
        s: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let corners = [
            (ox, oy, oz),     // 0
            (ox + s, oy, oz), // 1
            (ox, oy + s, oz), // 2
            (ox, oy, oz + s), // 3
        ];
        let mut coords = Vec::with_capacity(12);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        // 4 faces (outward winding for an apex-at-origin tetra).
        let tris = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    /// Concatenate two solids' (coords, tris, labels) into one soup. Triangle
    /// indices of the second solid are offset by the first's vertex count.
    fn concat(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #4 — input-prep correctness (called DIRECTLY, no full run).
    // ════════════════════════════════════════════════════════════════

    /// `merge_duplicated_vertices`: a duplicated coordinate triple collapses to
    /// ONE global id and every referencing triangle index is remapped.
    ///
    /// Input: 4 vertex slots where slot 3 == slot 0 exactly. Two triangles:
    /// [0,1,2] and [3,1,2] (== [0,1,2] after merge). Expect 3 surviving verts;
    /// both tris remap to indices over {0,1,2} with slot-3 → 0.
    #[test]
    fn prep_merge_duplicated_vertices_collapses_and_remaps() {
        // slots: 0=(0,0,0) 1=(1,0,0) 2=(0,1,0) 3=(0,0,0) duplicate of 0.
        let coords = vec![
            0.0, 0.0, 0.0, // 0
            1.0, 0.0, 0.0, // 1
            0.0, 1.0, 0.0, // 2
            0.0, 0.0, 0.0, // 3 == 0
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 1, 2]];
        let (verts, remapped) = merge_duplicated_vertices(&coords, &tris);

        // Only the 3 distinct referenced coordinates survive (insertion-order).
        assert_eq!(verts.len(), 3, "duplicate vertex must collapse to 3 verts");
        assert_eq!(verts[0], Point3::new(0.0, 0.0, 0.0));
        assert_eq!(verts[1], Point3::new(1.0, 0.0, 0.0));
        assert_eq!(verts[2], Point3::new(0.0, 1.0, 0.0));

        // Both triangles remap to the same global ids (slot-3 dup → 0).
        assert_eq!(remapped[0], [0, 1, 2]);
        assert_eq!(
            remapped[1],
            [0, 1, 2],
            "duplicated vertex slot 3 remaps to 0"
        );
    }

    /// `remove_degenerate_and_duplicated_triangles`:
    /// (a) an exactly-collinear (degenerate) triangle is dropped;
    /// (b) a duplicate triangle (same sorted verts) with a DIFFERENT label is
    ///     dropped and its label OR-merged into the survivor.
    #[test]
    fn prep_remove_degenerate_and_dup_triangles() {
        // verts: 0,1,2 a real tri; 3 collinear with 0,1 (on x-axis) makes a
        // degenerate tri [0,1,3]. Tri [2,1,0] is [0,1,2] sorted == survivor's,
        // a duplicate with a different label.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0), // 0
            Point3::new(2.0, 0.0, 0.0), // 1
            Point3::new(0.0, 2.0, 0.0), // 2
            Point3::new(1.0, 0.0, 0.0), // 3 collinear with 0,1
        ];
        let tris = vec![
            [0u32, 1, 2], // survivor, label A
            [0u32, 1, 3], // degenerate (0,1,3 collinear) → dropped
            [2u32, 1, 0], // duplicate of [0,1,2] (sorted), label B → merged
        ];
        let labels = vec![vec![A], vec![A], vec![B]];

        let (kept_tris, kept_labels, _dupl, _src) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        assert_eq!(
            kept_tris.len(),
            1,
            "degenerate + duplicate triangles must both be removed → 1 survivor"
        );
        assert_eq!(kept_tris[0], [0, 1, 2], "survivor keeps first-seen winding");

        // Label OR-merged (sorted-unique union of A and B).
        let mut got = kept_labels[0].clone();
        got.sort_by_key(|i| i.0);
        assert_eq!(
            got,
            vec![A, B],
            "duplicate triangle's label must be OR-merged into the survivor"
        );
    }

    /// `remove_degenerate_and_duplicated_triangles`: a duplicate of the SAME
    /// label is still dropped but the survivor's label is unchanged (idempotent
    /// OR-merge).
    #[test]
    fn prep_duplicate_same_label_merges_idempotently() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(2.0, 0.0, 0.0),
            Point3::new(0.0, 2.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [1u32, 2, 0]]; // same sorted set
        let labels = vec![vec![A], vec![A]];
        let (kept_tris, kept_labels, _dupl, _src) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);
        assert_eq!(kept_tris.len(), 1, "duplicate dropped");
        assert_eq!(kept_labels[0], vec![A], "idempotent OR-merge keeps just A");
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(d) + jolly tail — a NON-intersecting pair: soup == inputs
    // modulo prep (same triangle count, no new vertices except the jolly tail).
    // ════════════════════════════════════════════════════════════════

    /// Two disjoint cubes (no overlap, no shared face). After `mesh_arrangement`
    /// the output triangle count equals the input (24, all pass-through), no new
    /// real vertices are introduced, and the 5-point jolly tail is appended.
    #[test]
    fn case_d_non_intersecting_pair_is_passthrough() {
        let a = cube(0.0, 0.0, 0.0, 1.0, A);
        let b = cube(5.0, 5.0, 5.0, 1.0, B); // far away, no intersection
        let (coords, tris, labels) = concat(a, b);
        let n_in_tris = tris.len();
        let n_in_verts = coords.len() / 3;

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("disjoint pair must not error");

        assert_label_alignment(&soup);
        let real = assert_jolly_tail(&soup);

        // No new real vertices beyond the (deduped) inputs; here all 16 corners
        // are distinct so real == 16.
        assert_eq!(
            real, n_in_verts,
            "disjoint pair introduces no new real vertices"
        );
        // No splits → same triangle count.
        assert_eq!(
            soup.tris.len(),
            n_in_tris,
            "disjoint pair is straight pass-through (no new triangles)"
        );
        assert_conforming(&soup);
        assert_no_degenerate_tris(&soup);
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(b) — axis-aligned two-box overlap → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Box A = [0,2]^3, Box B = [1,3]^3. They interpenetrate; each box face that
    /// the other box crosses is split along the intersection curve. The result
    /// must be a conforming soup (no interior overlaps), all non-degenerate,
    /// labels 1:1, jolly tail present.
    #[test]
    fn case_b_axis_aligned_box_overlap_conforming() {
        let a = cube(0.0, 0.0, 0.0, 2.0, A);
        let b = cube(1.0, 1.0, 1.0, 2.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("box overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        // Invariant #1 (load-bearing): no two output triangles overlap interiors.
        assert_conforming(&soup);

        // Invariant #2/#3: intersection introduced NEW real vertices (the soup
        // is not a pure pass-through — the boxes were cut).
        let real = soup.verts.len() - 5;
        assert!(
            real > 16,
            "interpenetrating boxes must introduce new intersection vertices \
             (real verts {real} should exceed the 16 input corners)"
        );
        assert!(
            soup.tris.len() > 24,
            "interpenetrating boxes must produce more than the 24 input triangles"
        );

        // Every label is a non-empty subset of {A, B} carried from a parent.
        for lab in &soup.labels {
            assert!(!lab.is_empty(), "every output label must be non-empty");
            for id in lab {
                assert!(*id == A || *id == B, "labels only over input solids A/B");
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #2 — every detected intersection realized (shared welded id).
    //
    // Uses a minimal two-triangle transversal pair (not two closed solids) so
    // the exact LPI intersection vertices are easy to hand-derive, and asserts
    // they appear with a SINGLE welded global id shared across BOTH triangles'
    // sub-triangulations (the load-bearing interner weld, spec §7).
    // ════════════════════════════════════════════════════════════════

    /// Triangle Ta in the z=0 plane and triangle Tb crossing it transversally.
    /// Tb pierces Ta's interior along a segment; the two piercing points are
    /// LPI vertices. After arrangement, each LPI vertex must be present EXACTLY
    /// ONCE in `verts` (welded id) and referenced by sub-triangles of BOTH base
    /// triangles.
    ///
    /// Ta: (0,0,0),(4,0,0),(0,4,0)  — z=0.
    /// Tb: (1,1,-1),(1,1,1),(3,0,1) crossing the z=0 plane.
    /// The exact z=0 crossings of Tb's edges with Ta lie on the segment Ta∩Tb.
    #[test]
    fn case_intersection_realized_welded_lpi_ids() {
        // Two single triangles forming a transversal X. Each triangle is its own
        // "solid" label for this structural check.
        let coords = vec![
            // Ta (z=0)
            0.0, 0.0, 0.0, // 0
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            // Tb (crosses z=0)
            1.0, 1.0, -1.0, // 3
            1.0, 1.0, 1.0, // 4
            3.0, 0.0, 1.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("transversal pair must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);

        // The intersection of Ta with Tb's edges produces LPI vertices. Collect
        // all real (non-jolly) vertices that are NOT plain input corners; each
        // such intersection vertex must appear EXACTLY once (welded) and be
        // referenced by triangles inheriting BOTH the A label and the B label.
        let real = soup.verts.len() - 5;
        let input_corners: Vec<[RBig; 3]> = (0..6)
            .map(|i| {
                exact_coords(&VertexCoords::Explicit(Point3::new(
                    coords[i * 3],
                    coords[i * 3 + 1],
                    coords[i * 3 + 2],
                )))
            })
            .collect();

        let mut intersection_vids: Vec<u32> = Vec::new();
        for v in 0..real as u32 {
            let xc = exact_coords(&soup.verts[v as usize]);
            let is_corner = input_corners.iter().any(|c| *c == xc);
            if !is_corner {
                intersection_vids.push(v);
            }
        }
        assert!(
            !intersection_vids.is_empty(),
            "the transversal crossing must realize at least one intersection vertex"
        );

        // Each intersection vertex's EXACT coords are unique across `verts`
        // (welded to ONE id — no duplicate implicit point).
        for &v in &intersection_vids {
            let xv = exact_coords(&soup.verts[v as usize]);
            let dup_count = (0..real as u32)
                .filter(|&w| exact_coords(&soup.verts[w as usize]) == xv)
                .count();
            assert_eq!(
                dup_count, 1,
                "intersection vertex {v} must be welded to a SINGLE global id (found {dup_count})"
            );
        }

        // At least one intersection vertex is shared by triangles carrying A and
        // by triangles carrying B (it lies on the conformed edge of both).
        let mut shared = false;
        for &v in &intersection_vids {
            let mut on_a = false;
            let mut on_b = false;
            for (t, tri) in soup.tris.iter().enumerate() {
                if tri.contains(&v) {
                    if soup.labels[t].contains(&A) {
                        on_a = true;
                    }
                    if soup.labels[t].contains(&B) {
                        on_b = true;
                    }
                }
            }
            if on_a && on_b {
                shared = true;
            }
        }
        assert!(
            shared,
            "an intersection vertex must be shared (welded) across both A and B sub-triangles"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(a) — two tetrahedra overlapping → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Two interpenetrating tetrahedra. Tetra A at origin, Tetra B offset so it
    /// overlaps A's interior but is not coplanar with any A face (transversal
    /// only → no TPI). Result must be a conforming soup.
    #[test]
    fn case_a_two_tetrahedra_overlap_conforming() {
        let a = tetra(0.0, 0.0, 0.0, 3.0, A);
        // B offset along the (1,1,1) diagonal so it pierces A transversally.
        let b = tetra(1.0, 1.0, 1.0, 3.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("tetra overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);

        // The tetra pair interpenetrates → soup is not a pure pass-through.
        assert!(
            soup.tris.len() > 8,
            "interpenetrating tetrahedra must produce more than the 8 input faces"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(c) — a ROTATED two-box overlap → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Box A axis-aligned [0,2]^3; Box B is a unit-ish box rotated 45° about the
    /// z-axis and shifted so it interpenetrates A while NOT sharing any coplanar
    /// face (the rotated side faces are oblique to A's). Transversal only → no
    /// TPI. Result must be conforming.
    #[test]
    fn case_c_rotated_box_overlap_conforming() {
        // Box B: a rectangular prism whose footprint is a 45°-rotated square
        // centred at (1,1, z) spanning z in [-1, 3] (so it pierces A top/bottom
        // and the oblique side walls cross A's vertical faces transversally).
        // Footprint square corners (rotated 45°, "radius" r = 1.2):
        //   (1+r, 1, *), (1, 1+r, *), (1-r, 1, *), (1, 1-r, *)
        let r = 1.2;
        let zlo = -1.0;
        let zhi = 3.0;
        let b_corners = [
            (1.0 + r, 1.0, zlo), // 0
            (1.0, 1.0 + r, zlo), // 1
            (1.0 - r, 1.0, zlo), // 2
            (1.0, 1.0 - r, zlo), // 3
            (1.0 + r, 1.0, zhi), // 4
            (1.0, 1.0 + r, zhi), // 5
            (1.0 - r, 1.0, zhi), // 6
            (1.0, 1.0 - r, zhi), // 7
        ];
        let mut bcoords = Vec::with_capacity(24);
        for (x, y, z) in b_corners {
            bcoords.push(x);
            bcoords.push(y);
            bcoords.push(z);
        }
        // 12 outward triangles: bottom (0,1,2,3), top (4,5,6,7), 4 sides.
        let btris = vec![
            // bottom z=zlo (normal -z): wind CW seen from below
            [0u32, 2, 1],
            [0, 3, 2],
            // top z=zhi (normal +z)
            [4u32, 5, 6],
            [4, 6, 7],
            // sides (each quad → 2 tris), outward
            [0u32, 1, 5],
            [0, 5, 4],
            [1u32, 2, 6],
            [1, 6, 5],
            [2u32, 3, 7],
            [2, 7, 6],
            [3u32, 0, 4],
            [3, 4, 7],
        ];
        let blabels = vec![vec![B]; btris.len()];

        let (coords, tris, labels) = concat(cube(0.0, 0.0, 0.0, 2.0, A), (bcoords, btris, blabels));

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("rotated box overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);
        assert!(
            soup.tris.len() > 24,
            "rotated interpenetrating box must produce more than the 24 input triangles"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #3 — Euler / edge-incidence sanity on a closed-input hand case.
    // ════════════════════════════════════════════════════════════════

    /// On the disjoint-cubes case (a clean closed input that passes straight
    /// through), the output triangle soup is a union of two closed 2-manifolds:
    /// every (undirected) edge is shared by exactly 2 triangles, and V−E+F = 2·2
    /// = 4 (two genus-0 components). This is the edge-incidence / Euler sanity
    /// check on closed input (invariant #3) — done on the pass-through case so
    /// the topology is exactly predictable (no cut-triangle skips).
    #[test]
    fn euler_edge_incidence_on_closed_passthrough() {
        let a = cube(0.0, 0.0, 0.0, 1.0, A);
        let b = cube(10.0, 0.0, 0.0, 1.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("closed input must not error");
        assert_jolly_tail(&soup);

        // Build undirected-edge incidence over real triangles.
        use std::collections::HashMap;
        let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in &soup.tris {
            for &(i, j) in &[(0usize, 1usize), (1, 2), (2, 0)] {
                let a = tri[i];
                let b = tri[j];
                let key = if a < b { (a, b) } else { (b, a) };
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        for (e, c) in &edge_count {
            assert_eq!(
                *c, 2,
                "edge {e:?} must be shared by exactly 2 triangles (closed 2-manifold)"
            );
        }

        // Euler: V − E + F = 4 for two disjoint genus-0 closed surfaces.
        let v = referenced_vertex_count(&soup) as i64;
        let e = edge_count.len() as i64;
        let f = soup.tris.len() as i64;
        assert_eq!(
            v - e + f,
            4,
            "two disjoint closed cubes: V−E+F must equal 4 (got V={v} E={e} F={f})"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #6 — loud deferral. A coplanar-face pair must return a
    // classified ArrangementError, NEVER a silent / wrong soup.
    // ════════════════════════════════════════════════════════════════

    /// Two triangles in the SAME plane (z=0) that overlap in positive area.
    /// As of PR-4 (coplanar pocket-dedup port) AR1's constructed `Coplanar`
    /// classification is wired into the split path: the pair CONSTRUCTS a
    /// conforming subdivision (A-only / overlap / B-only) instead of deferring.
    /// The overlap boundary appears as new (implicit) vertices, so the soup has
    /// strictly more vertices than the 6 inputs and more than 2 triangles —
    /// never a silent pass-through (which would emit exactly the 2 inputs).
    #[test]
    fn coplanar_overlap_pair_is_constructed() {
        // Ta and Tb both in z=0, overlapping (Tb shifted into Ta's interior).
        let coords = vec![
            // Ta
            0.0, 0.0, 0.0, // 0
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            // Tb (same plane z=0, overlapping Ta)
            1.0, 1.0, 0.0, // 3
            5.0, 1.0, 0.0, // 4
            1.0, 5.0, 0.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];

        let soup = mesh_arrangement(&coords, &tris, &labels)
            .expect("coplanar overlapping pair now CONSTRUCTS (PR-4 pocket dedup)");
        assert!(
            soup.tris.len() > 2,
            "the overlap must be subdivided (got {} tris, the 2 raw inputs = \
             silent pass-through)",
            soup.tris.len()
        );
        assert!(
            soup.verts.len() > 6,
            "overlap-boundary crossings must add new vertices (got {})",
            soup.verts.len()
        );

        // -- single-coplanar-edge, CONTAINED sub-config: now CLASSIFIED, not
        //    deferred (deviation N13, this PR). Tb's edge B0-B1 lies in Ta's
        //    plane (z=0) STRICTLY INSIDE Ta, Tb's third vertex off-plane. The
        //    arrangement must SUCCEED (no CoplanarPairDeferred) and the
        //    coplanar-edge endpoints must appear as explicit arrangement
        //    vertices, since the C++ checkSingleCoplanarEdgeIntersections
        //    places them as vertex-in-triangle + a symbolic segment.
        {
            let coords = vec![
                // Ta in z=0
                0.0, 0.0, 0.0, // 0
                10.0, 0.0, 0.0, // 1
                0.0, 10.0, 0.0, // 2
                // Tb: edge B0-B1 in z=0 strictly inside Ta; B2 off-plane.
                2.0, 2.0, 0.0, // 3
                4.0, 3.0, 0.0, // 4
                3.0, 3.0, 5.0, // 5
            ];
            let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
            let labels = vec![vec![A], vec![B]];
            let soup = mesh_arrangement(&coords, &tris, &labels).expect(
                "a contained single-coplanar-edge pair must classify, not defer \
                 (deviation N13)",
            );
            // The two coplanar-edge endpoints survive as explicit vertices in
            // the arrangement soup (scaled by the multiplier — match on the
            // explicit-kind count growing past the 6 inputs is the load-bearing
            // check; the arrangement must have at least the input vertices).
            assert!(
                soup.verts.len() >= 6,
                "arrangement soup must retain the input vertices, got {}",
                soup.verts.len()
            );
        }

        // NOTE: the N16 deep-recursion wall (DeepRecursionRequired) is NOT
        // readily constructible as a deterministic single hand case here — it
        // requires a constraint segment crossing MULTIPLE existing constraints
        // (the global seg2tris / coplanar jollyPoint path). Per the task brief,
        // that sub-case is skipped; the coplanar deferral above is the required
        // loud-deferral test. If/when a deterministic deep-recursion fixture is
        // found, add an analogous `expect_err(... DeepRecursionRequired ...)`.
    }

    // ════════════════════════════════════════════════════════════════
    // Error path — label/triangle count mismatch is loud.
    // ════════════════════════════════════════════════════════════════

    /// `in_labels` not 1:1 with `tris` → `LabelCountMismatch`, never a silent
    /// truncation.
    #[test]
    fn label_count_mismatch_is_loud() {
        let (coords, tris, mut labels) = cube(0.0, 0.0, 0.0, 1.0, A);
        labels.pop(); // now labels.len() == tris.len() - 1
        let err = mesh_arrangement(&coords, &tris, &labels)
            .expect_err("label/tri count mismatch must error");
        assert!(
            matches!(err, ArrangementError::LabelCountMismatch { .. }),
            "expected LabelCountMismatch, got {err:?}"
        );
    }
}

#[cfg(test)]
mod adversary_tests {
    //! PR-CR-AR3b ADVERSARY tests (sub-agent C). Authored SEPARATELY from the
    //! frozen RED `mod tests`; this module references neither the production
    //! internals nor the RED module's private helpers (it re-derives its own
    //! pure-`dashu` exact oracles in the SAME style). It adversarially stresses
    //! the just-landed `mesh_arrangement` orchestration to pin guarantees the
    //! RED tests leave un-pinned:
    //!
    //! 1. **Output invariance under pathological input orderings** — reversed
    //!    per-triangle winding, reversed triangle order, A↔B label swap: the soup
    //!    stays conforming, intersection-realized, topologically sane.
    //! 2. **Multiple independent crossings** on one base face — conforms exactly,
    //!    or a LOUD `DeepRecursionRequired` (never a silent / wrong soup). The
    //!    observed outcome is asserted + documented inline.
    //! 3. **Near-shared faces — loud deferral.** A near-coplanar pair whose
    //!    contact includes a coplanar EDGE crossing the other's interior, and a
    //!    genuinely coplanar overlap, both → loud `CoplanarPairDeferred` (never a
    //!    silent/wrong soup). Pins the `SingleCoplanarEdge` deferral branch
    //!    end-to-end. Exact rational offsets.
    //! 4. **Dedup is load-bearing** — N18 canonicalization welds COINCIDENT
    //!    implicit points to one id BUT keeps GENUINELY-DISTINCT intersection
    //!    points DISTINCT (anti-over-weld guard).
    //! 5. **Planar fast-path == inputs modulo prep** — disjoint pair: out_tris is
    //!    the prepped input set (same count, same resolved vertex POSITIONS up to
    //!    per-triangle rotation — prep renumbers ids), no new real verts,
    //!    jolly_count == 5, labels preserved per triangle.
    //! 6. **Loud-deferral never silent (P9/P10)** — every out-of-scope input
    //!    classified as `Err`, explicitly NOT `Ok`.

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::{mesh_arrangement, ArrangementError, ArrangementSoup, Label};
    use crate::labeled_arrangement::InputId;
    use crate::processing::multiplier::{compute_multiplier, multiply_coordinates};
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ════════════════════════════════════════════════════════════════
    // Exact-rational helpers (own copies, pure dashu — FFI-independent).
    // ════════════════════════════════════════════════════════════════

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }
    fn sub3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    }
    fn cross3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    }
    fn dot3(a: &[RBig; 3], b: &[RBig; 3]) -> RBig {
        &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
    }

    /// Exact coordinates of a stored `VertexCoords`. Mirrors the production
    /// `exact_vertex_coords` / RED `exact_coords` (line∩plane for Lpi; Cramer
    /// 3-plane solve for Tpi). Panics on a degenerate fixture (caught by the
    /// production canonicalizer in real runs; our fixtures are non-degenerate).
    fn exact_coords(c: &VertexCoords) -> [RBig; 3] {
        let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
        match c {
            VertexCoords::Explicit(p) => to_r3(p),
            VertexCoords::Lpi { line, plane } => {
                let p = to_r3(&line[0]);
                let q = to_r3(&line[1]);
                let r = to_r3(&plane[0]);
                let s = to_r3(&plane[1]);
                let t = to_r3(&plane[2]);
                let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                let num = dot3(&sub3(&r, &p), &n);
                let den = dot3(&sub3(&q, &p), &n);
                assert!(den != RBig::ZERO, "LPI parallel to plane — bad fixture");
                let u = &num / &den;
                let qp = sub3(&q, &p);
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { v, w, u } => {
                let plane_eqn = |tri: &[Point3; 3]| -> ([RBig; 3], RBig) {
                    let r = to_r3(&tri[0]);
                    let s = to_r3(&tri[1]);
                    let t = to_r3(&tri[2]);
                    let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                    let d = dot3(&n, &r);
                    (n, d)
                };
                let (n0, d0) = plane_eqn(v);
                let (n1, d1) = plane_eqn(w);
                let (n2, d2) = plane_eqn(u);
                let det_rows = |r0: &[RBig; 3], r1: &[RBig; 3], r2: &[RBig; 3]| -> RBig {
                    dot3(r0, &cross3(r1, r2))
                };
                let det = det_rows(&n0, &n1, &n2);
                assert!(det != RBig::ZERO, "TPI planes degenerate — bad fixture");
                let rhs = [d0, d1, d2];
                let sub_col = |k: usize| -> [[RBig; 3]; 3] {
                    let mut rows = [n0.clone(), n1.clone(), n2.clone()];
                    rows[0][k] = rhs[0].clone();
                    rows[1][k] = rhs[1].clone();
                    rows[2][k] = rhs[2].clone();
                    rows
                };
                let mx = sub_col(0);
                let my = sub_col(1);
                let mz = sub_col(2);
                [
                    &det_rows(&mx[0], &mx[1], &mx[2]) / &det,
                    &det_rows(&my[0], &my[1], &my[2]) / &det,
                    &det_rows(&mz[0], &mz[1], &mz[2]) / &det,
                ]
            }
        }
    }

    fn exact_signed_area2(axis: usize, a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
        let (i, j) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let bx = &b[i] - &a[i];
        let by = &b[j] - &a[j];
        let cx = &c[i] - &a[i];
        let cy = &c[j] - &a[j];
        &(&bx * &cy) - &(&by * &cx)
    }

    fn dominant_axis(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> usize {
        let n = cross3(&sub3(b, a), &sub3(c, a));
        let abs = |r: &RBig| {
            if r < &RBig::ZERO {
                -r.clone()
            } else {
                r.clone()
            }
        };
        let nx = abs(&n[0]);
        let ny = abs(&n[1]);
        let nz = abs(&n[2]);
        if nx >= ny && nx >= nz {
            0
        } else if ny >= nz {
            1
        } else {
            2
        }
    }

    fn tri_exact(soup: &ArrangementSoup, t: usize) -> [[RBig; 3]; 3] {
        let [a, b, c] = soup.tris[t];
        [
            exact_coords(&soup.verts[a as usize]),
            exact_coords(&soup.verts[b as usize]),
            exact_coords(&soup.verts[c as usize]),
        ]
    }

    fn coplanar4(p: &[RBig; 3], a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> bool {
        dot3(&sub3(a, p), &cross3(&sub3(b, p), &sub3(c, p))) == RBig::ZERO
    }
    fn tris_coplanar(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        t1.iter().all(|p| coplanar4(&t0[0], &t0[1], &t0[2], p))
    }
    fn point_strictly_in_tri2(
        axis: usize,
        p: &[RBig; 3],
        a: &[RBig; 3],
        b: &[RBig; 3],
        c: &[RBig; 3],
    ) -> bool {
        let d0 = exact_signed_area2(axis, a, b, p);
        let d1 = exact_signed_area2(axis, b, c, p);
        let d2 = exact_signed_area2(axis, c, a, p);
        let pos = d0 > RBig::ZERO && d1 > RBig::ZERO && d2 > RBig::ZERO;
        let neg = d0 < RBig::ZERO && d1 < RBig::ZERO && d2 < RBig::ZERO;
        pos || neg
    }
    fn segments_properly_cross2(
        axis: usize,
        p0: &[RBig; 3],
        p1: &[RBig; 3],
        q0: &[RBig; 3],
        q1: &[RBig; 3],
    ) -> bool {
        let o1 = exact_signed_area2(axis, p0, p1, q0);
        let o2 = exact_signed_area2(axis, p0, p1, q1);
        let o3 = exact_signed_area2(axis, q0, q1, p0);
        let o4 = exact_signed_area2(axis, q0, q1, p1);
        let opp = |a: &RBig, b: &RBig| {
            (a > &RBig::ZERO && b < &RBig::ZERO) || (a < &RBig::ZERO && b > &RBig::ZERO)
        };
        opp(&o1, &o2) && opp(&o3, &o4)
    }
    fn tris_interiors_overlap(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        if !tris_coplanar(t0, t1) {
            return false;
        }
        let axis = dominant_axis(&t0[0], &t0[1], &t0[2]);
        for p in t1.iter() {
            if point_strictly_in_tri2(axis, p, &t0[0], &t0[1], &t0[2]) {
                return true;
            }
        }
        for p in t0.iter() {
            if point_strictly_in_tri2(axis, p, &t1[0], &t1[1], &t1[2]) {
                return true;
            }
        }
        let e = [(0usize, 1usize), (1, 2), (2, 0)];
        for (a, b) in e {
            for (c, d) in e {
                if segments_properly_cross2(axis, &t0[a], &t0[b], &t1[c], &t1[d]) {
                    return true;
                }
            }
        }
        false
    }

    /// Invariant #1: no two output triangles overlap in their interiors.
    fn assert_conforming(soup: &ArrangementSoup) {
        let n = soup.tris.len();
        let exacts: Vec<[[RBig; 3]; 3]> = (0..n).map(|t| tri_exact(soup, t)).collect();
        for a in 0..n {
            for b in (a + 1)..n {
                assert!(
                    !tris_interiors_overlap(&exacts[a], &exacts[b]),
                    "output triangles {a} and {b} overlap interiors — soup not conforming"
                );
            }
        }
    }

    /// Invariant #3: every output triangle has exact non-zero area.
    fn assert_no_degenerate_tris(soup: &ArrangementSoup) {
        for t in 0..soup.tris.len() {
            let [a, b, c] = tri_exact(soup, t);
            let axis = dominant_axis(&a, &b, &c);
            assert!(
                exact_signed_area2(axis, &a, &b, &c) != RBig::ZERO,
                "output triangle {t} is degenerate (exact zero area)"
            );
        }
    }

    fn assert_jolly_tail(soup: &ArrangementSoup) -> usize {
        assert_eq!(soup.jolly_count, 5, "jolly_count must be exactly 5");
        let n = soup.verts.len();
        assert!(n >= 5, "verts must include the 5 jolly points");
        let real = n - 5;
        for tri in &soup.tris {
            for &id in tri {
                assert!(
                    (id as usize) < real,
                    "triangle references a jolly id {id} (real verts = {real})"
                );
            }
        }
        real
    }
    fn assert_label_alignment(soup: &ArrangementSoup) {
        assert_eq!(soup.tris.len(), soup.labels.len(), "labels 1:1 with tris");
    }

    /// All distinct exact intersection-vertex locations referenced by `soup`'s
    /// real triangles that are NOT one of `corners` (the input-corner coords).
    fn intersection_locations(soup: &ArrangementSoup, corners: &[[RBig; 3]]) -> Vec<[RBig; 3]> {
        let real = soup.verts.len() - 5;
        let mut out: Vec<[RBig; 3]> = Vec::new();
        for v in 0..real {
            let xc = exact_coords(&soup.verts[v]);
            if corners.iter().any(|c| *c == xc) {
                continue;
            }
            if !out.iter().any(|e| *e == xc) {
                out.push(xc);
            }
        }
        out
    }

    /// Assert each intersection vertex of `soup` is welded to a SINGLE global id
    /// (no two real verts share exact coords). Returns the welded id count.
    fn assert_implicit_points_welded(soup: &ArrangementSoup) -> usize {
        let real = soup.verts.len() - 5;
        let mut count = 0;
        for v in 0..real {
            let xv = exact_coords(&soup.verts[v]);
            let dups = (0..real)
                .filter(|&w| exact_coords(&soup.verts[w]) == xv)
                .count();
            assert_eq!(
                dups, 1,
                "vertex {v} must be welded to a SINGLE global id (found {dups})"
            );
            count += 1;
        }
        count
    }

    // ════════════════════════════════════════════════════════════════
    // Hand corpus builders (own copies; same style as RED).
    // ════════════════════════════════════════════════════════════════

    fn cube(
        x0: f64,
        y0: f64,
        z0: f64,
        side: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let x1 = x0 + side;
        let y1 = y0 + side;
        let z1 = z0 + side;
        let corners = [
            (x0, y0, z0),
            (x1, y0, z0),
            (x1, y1, z0),
            (x0, y1, z0),
            (x0, y0, z1),
            (x1, y0, z1),
            (x1, y1, z1),
            (x0, y1, z1),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    fn concat(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    // ════════════════════════════════════════════════════════════════
    // 1. Pathological orderings — output invariance.
    //
    // Same axis-aligned two-box overlap geometry (A=[0,2]^3, B=[1,3]^3) fed
    // under three input perturbations that must NOT change the arrangement's
    // correctness: (a) reversed per-triangle winding, (b) reversed triangle
    // order, (c) A↔B label swap. Each must still be conforming, non-degenerate,
    // welded, and intersection-realized (the boxes were cut → > 24 tris,
    // > 16 real verts). The arrangement must not depend on incidental input
    // ordering.
    // ════════════════════════════════════════════════════════════════

    /// Baseline geometry: interpenetrating axis-aligned boxes.
    fn two_box_overlap() -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B))
    }

    /// Assert a two-box-overlap soup is fully correct AND realized a cut.
    fn assert_two_box_overlap_correct(soup: &ArrangementSoup) {
        assert_label_alignment(soup);
        assert_jolly_tail(soup);
        assert_no_degenerate_tris(soup);
        assert_conforming(soup);
        assert_implicit_points_welded(soup);
        let real = soup.verts.len() - 5;
        assert!(
            real > 16,
            "interpenetrating boxes must introduce intersection verts (real = {real})"
        );
        assert!(
            soup.tris.len() > 24,
            "interpenetrating boxes must produce > 24 triangles (got {})",
            soup.tris.len()
        );
        for lab in &soup.labels {
            assert!(!lab.is_empty(), "every output label is non-empty");
            for id in lab {
                assert!(*id == A || *id == B, "labels only over A/B");
            }
        }
    }

    #[test]
    fn adversary_reversed_winding_is_invariant() {
        // Reverse every triangle's vertex order [a,b,c] → [c,b,a] (flips winding
        // / normal). The non-self-intersection + intersection-realization must be
        // unaffected — the arrangement is winding-agnostic for the conforming soup
        // (orientation matters only to later in/out, not here).
        let (coords, tris, labels) = two_box_overlap();
        let rev_tris: Vec<[u32; 3]> = tris.iter().map(|t| [t[2], t[1], t[0]]).collect();
        let soup = mesh_arrangement(&coords, &rev_tris, &labels)
            .expect("reversed-winding box overlap must not error");
        assert_two_box_overlap_correct(&soup);
    }

    #[test]
    fn adversary_reversed_triangle_order_is_invariant() {
        // Reverse the order triangles appear in the input arrays (and labels in
        // lockstep). Global vertex ids change but the soup must remain conforming
        // + realized; the per-triangle serial loop must not depend on input order.
        let (coords, tris, labels) = two_box_overlap();
        let rev_tris: Vec<[u32; 3]> = tris.iter().rev().copied().collect();
        let rev_labels: Vec<Label> = labels.iter().rev().cloned().collect();
        let soup = mesh_arrangement(&coords, &rev_tris, &rev_labels)
            .expect("reversed-tri-order box overlap must not error");
        assert_two_box_overlap_correct(&soup);
    }

    #[test]
    fn adversary_swapped_ab_labels_is_invariant() {
        // Swap the input-solid label assignment (A↔B). The geometry is identical;
        // only the carried labels swap. Soup must stay conforming + realized, and
        // every label is still a non-empty subset of {A,B}. (The label-set is
        // symmetric here, so this also guards that label routing is per-parent,
        // not order-coupled.)
        let swapped = |lab: InputId| if lab == A { B } else { A };
        let a = cube(0.0, 0.0, 0.0, 2.0, swapped(A)); // now B
        let b = cube(1.0, 1.0, 1.0, 2.0, swapped(B)); // now A
        let (coords, tris, labels) = concat(a, b);
        let soup = mesh_arrangement(&coords, &tris, &labels)
            .expect("A↔B-swapped box overlap must not error");
        assert_two_box_overlap_correct(&soup);
    }

    // ════════════════════════════════════════════════════════════════
    // 2. Multiple independent crossings on ONE base face.
    //
    // A large triangle Ta in the z=0 plane is pierced by TWO disjoint
    // protrusions of the other solid, so Ta's submesh receives several separate
    // constraint segments. Two small tetra-like "spikes" each puncture Ta in a
    // separate, well-separated region. Assert conforming + every crossing
    // realized — OR a LOUD DeepRecursionRequired (never silent / wrong). The
    // observed outcome is documented inline.
    // ════════════════════════════════════════════════════════════════

    /// A single closed triangular "spike" tetra that punctures the z=0 plane near
    /// `(cx, cy)`: base above the plane, apex below it, so two of its faces cross
    /// z=0 transversally, each contributing a constraint segment to the z=0 face.
    fn spike(cx: f64, cy: f64, label: InputId) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        // 3 top corners (z = +1) around (cx,cy), 1 apex (z = -1) at (cx,cy).
        let h = 0.6;
        let coords = vec![
            cx - h,
            cy - h,
            1.0, // 0 top
            cx + h,
            cy - h,
            1.0, // 1 top
            cx,
            cy + h,
            1.0, // 2 top
            cx,
            cy,
            -1.0, // 3 apex below plane
        ];
        // Outward-ish winding (immaterial to arrangement correctness).
        let tris = vec![[0u32, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    #[test]
    fn adversary_multiple_independent_crossings_on_one_face() {
        // Ta: a big z=0 triangle covering the region where both spikes land.
        let mut coords = vec![
            -5.0, -5.0, 0.0, // 0
            10.0, -5.0, 0.0, // 1
            -5.0, 10.0, 0.0, // 2
        ];
        let mut tris = vec![[0u32, 1, 2]];
        let mut labels: Vec<Label> = vec![vec![A]];

        // Two disjoint spikes piercing Ta in well-separated regions.
        for (cx, cy) in [(0.0_f64, 0.0_f64), (4.0, 4.0)] {
            let (sc, st, sl) = spike(cx, cy, B);
            let off = (coords.len() / 3) as u32;
            coords.extend_from_slice(&sc);
            for t in st {
                tris.push([t[0] + off, t[1] + off, t[2] + off]);
            }
            labels.extend(sl);
        }

        match mesh_arrangement(&coords, &tris, &labels) {
            Ok(soup) => {
                // OBSERVED OUTCOME: Ok — the big base face conforms exactly even
                // with multiple disjoint constraint segments from separate
                // protrusions. Pin the strong guarantees.
                assert_label_alignment(&soup);
                assert_jolly_tail(&soup);
                assert_no_degenerate_tris(&soup);
                assert_conforming(&soup);
                assert_implicit_points_welded(&soup);

                // Both crossings must be realized: the base face was split into
                // more than the 1 input triangle, and intersection vertices were
                // introduced for BOTH spike regions (x<2 region and x>2 region).
                assert!(
                    soup.tris.len() > 1 + 8,
                    "multi-crossing base face must split (got {} tris)",
                    soup.tris.len()
                );
                let corners: Vec<[RBig; 3]> = (0..coords.len() / 3)
                    .map(|i| {
                        [
                            to_r(coords[3 * i]),
                            to_r(coords[3 * i + 1]),
                            to_r(coords[3 * i + 2]),
                        ]
                    })
                    .collect();
                let locs = intersection_locations(&soup, &corners);
                let near_first = locs.iter().any(|p| p[0] < to_r(2.0));
                let near_second = locs.iter().any(|p| p[0] > to_r(2.0));
                assert!(
                    near_first && near_second,
                    "BOTH disjoint crossings must be realized (near_first={near_first}, \
                     near_second={near_second}); intersection locs = {}",
                    locs.len()
                );
            }
            Err(ArrangementError::DeepRecursionRequired { base_tri, detail }) => {
                // ACCEPTABLE LOUD OUTCOME: the multi-segment base face drove the
                // N16 deep-recursion wall. This is the contract's loud deferral —
                // never a silent / wrong soup. (Documented as a permissible
                // outcome by the brief.) Pin only that it is THIS error variant.
                let _ = (base_tri, detail);
            }
            Err(other) => {
                panic!(
                    "multi-crossing face must be Ok(conforming) OR loud \
                     DeepRecursionRequired — got {other:?}"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // 3. Near-shared faces — never a silent/wrong soup.
    //
    // (a) Two near-coplanar boxes whose contact includes a single coplanar EDGE
    //     lying in the other's face-plane and crossing its interior (plus an
    //     independent coplanar y-face overlap). As of the N13 tvX/edge-crossing
    //     slice this real positive-volume intersection is now CONSTRUCTED (a
    //     conforming subdivision), not deferred — reference-parity-verified in
    //     `single_coplanar_edge_parity.rs::tilted_slab_through_interior_*`. This
    //     pins that it is never a silent pass-through (P9/P10): the soup is
    //     subdivided with new geometry beyond the raw input.
    // (b) A genuinely coplanar OVERLAPPING pair → constructed (PR-4).
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn adversary_coplanar_edge_through_interior_is_constructed() {
        // Box A = axis-aligned [0,2]^3.
        // Box B = [1,3]x[0,2]x[0,2] but its z=0 / z=2 faces are TILTED by a small
        // exact slope (1/8 per unit x) so B is a transversal slab, NOT coplanar
        // with any A face. The tilt 1/8 = 0.125 is exact in f64 → no float fuzz.
        // B therefore interpenetrates A and the facing faces meet transversally.
        let s = 0.125_f64; // exact in binary.
                           // B's 8 corners: x in {1,3}, y in {0,2}, z tilted: z = base + s*(x-1).
        let bx = |xi: usize| if xi == 0 { 1.0 } else { 3.0 };
        let by = |yi: usize| if yi == 0 { 0.0 } else { 2.0 };
        // base z: bottom face base 0, top face base 2; tilt adds s*(x-1).
        let mut bcoords = Vec::with_capacity(24);
        let mut bidx = Vec::new();
        for zi in 0..2 {
            for yi in 0..2 {
                for xi in 0..2 {
                    let x = bx(xi);
                    let y = by(yi);
                    let zbase = if zi == 0 { 0.0 } else { 2.0 };
                    let z = zbase + s * (x - 1.0);
                    bcoords.push(x);
                    bcoords.push(y);
                    bcoords.push(z);
                    bidx.push((xi, yi, zi));
                }
            }
        }
        // Helper to map (xi,yi,zi) → linear corner id in the pushed order.
        let cid = |xi: usize, yi: usize, zi: usize| (zi * 4 + yi * 2 + xi) as u32;
        // 12 triangles of B (winding immaterial).
        let q = |a: u32, b: u32, c: u32, d: u32| vec![[a, b, c], [a, c, d]];
        let mut btris: Vec<[u32; 3]> = Vec::new();
        btris.extend(q(cid(0, 0, 0), cid(1, 0, 0), cid(1, 1, 0), cid(0, 1, 0))); // bottom
        btris.extend(q(cid(0, 0, 1), cid(1, 0, 1), cid(1, 1, 1), cid(0, 1, 1))); // top
        btris.extend(q(cid(0, 0, 0), cid(1, 0, 0), cid(1, 0, 1), cid(0, 0, 1))); // y=0
        btris.extend(q(cid(0, 1, 0), cid(1, 1, 0), cid(1, 1, 1), cid(0, 1, 1))); // y=2
        btris.extend(q(cid(0, 0, 0), cid(0, 1, 0), cid(0, 1, 1), cid(0, 0, 1))); // x=1
        btris.extend(q(cid(1, 0, 0), cid(1, 1, 0), cid(1, 1, 1), cid(1, 0, 1))); // x=3
        let blabels = vec![vec![B]; btris.len()];

        let (coords, tris, labels) = concat(cube(0.0, 0.0, 0.0, 2.0, A), (bcoords, btris, blabels));

        // The 1/8 tilt pivots at x=1, so B's bottom-face x=1 edge (1,0,0)→(1,2,0)
        // lies EXACTLY in A's z=0 plane and runs through the strict interior of
        // A's bottom face — a genuine SingleCoplanarEdge intersection (the
        // tvX/edge-crossing config). B's untilted y=0/y=2 faces additionally
        // overlap A's y-planes in positive area (an independent Coplanar
        // contact, constructed since PR-4). The whole positive-volume
        // interpenetration is now CONSTRUCTED as a conforming subdivision
        // (reference-parity-verified, see the parity suite) — it must never be a
        // silent pass-through nor a wrong soup (P9/P10): the result is a real
        // subdivision with new geometry beyond the raw input triangles.
        let n_input_tris = tris.len();
        let n_input_verts = coords.len() / 3;
        let soup = mesh_arrangement(&coords, &tris, &labels).expect(
            "coplanar-edge-through-interior is now CONSTRUCTED (N13 tvX slice), not deferred",
        );
        assert!(
            soup.tris.len() > n_input_tris && soup.verts.len() > n_input_verts,
            "interpenetration must be subdivided with new geometry, got {} tris / {} verts \
             (raw input {n_input_tris} tris / {n_input_verts} verts)",
            soup.tris.len(),
            soup.verts.len()
        );
    }

    #[test]
    fn adversary_genuinely_coplanar_overlap_is_constructed() {
        // Two triangles in the SAME plane z=0, overlapping in positive area.
        // As of PR-4 this CONSTRUCTS a conforming subdivision (pocket dedup),
        // it does not defer. The overlap is subdivided (more than the 2 raw
        // input triangles) and new boundary vertices appear — never a silent
        // pass-through, never a wrong soup.
        let coords = vec![
            0.0, 0.0, 0.0, // 0
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            1.0, 1.0, 0.0, // 3 (in Ta interior)
            5.0, 1.0, 0.0, // 4
            1.0, 5.0, 0.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];
        let soup = mesh_arrangement(&coords, &tris, &labels)
            .expect("coplanar positive-area overlap now CONSTRUCTS (PR-4)");
        assert!(
            soup.tris.len() > 2 && soup.verts.len() > 6,
            "overlap must be subdivided with new boundary vertices, got {} tris / {} verts",
            soup.tris.len(),
            soup.verts.len()
        );
    }

    // ════════════════════════════════════════════════════════════════
    // 4. Dedup is load-bearing (anti-over-weld + weld-happens).
    //
    // (a) Anti-over-weld: a transversal pair whose intersection produces TWO
    //     intersection vertices at DIFFERENT exact locations must keep them as
    //     DISTINCT global ids (canonicalization must not collapse distinct
    //     points). Compute both exact coords; assert they differ AND map to
    //     different ids.
    // (b) Weld-happens: a configuration where the SAME geometric intersection
    //     point is reachable from both triangles must yield exactly ONE welded
    //     id (coincident → one id). The transversal-X case's two pierce points
    //     are each shared by both base triangles' sub-triangulations.
    // ════════════════════════════════════════════════════════════════

    /// Transversal X: Ta in z=0, Tb crossing it, producing exactly TWO distinct
    /// LPI pierce points. Used by both the distinctness and the weld assertions.
    fn transversal_x() -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let coords = vec![
            0.0, 0.0, 0.0, // 0  Ta
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            1.0, 1.0, -1.0, // 3  Tb
            1.0, 1.0, 1.0, // 4
            3.0, 0.0, 1.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];
        (coords, tris, labels)
    }

    #[test]
    fn adversary_distinct_intersection_points_stay_distinct() {
        let (coords, tris, labels) = transversal_x();
        let soup = mesh_arrangement(&coords, &tris, &labels).expect("transversal X must not error");
        assert_conforming(&soup);
        assert_no_degenerate_tris(&soup);

        let corners: Vec<[RBig; 3]> = (0..6)
            .map(|i| {
                [
                    to_r(coords[3 * i]),
                    to_r(coords[3 * i + 1]),
                    to_r(coords[3 * i + 2]),
                ]
            })
            .collect();
        let locs = intersection_locations(&soup, &corners);

        // There must be at least TWO distinct intersection locations (the segment
        // Ta∩Tb has two endpoints at different exact coords). Anti-over-weld: if
        // canonicalization collapsed distinct points, we'd see < 2.
        assert!(
            locs.len() >= 2,
            "two distinct pierce points must remain DISTINCT global verts \
             (anti-over-weld); found {} intersection locations",
            locs.len()
        );

        // And those distinct locations really are pairwise different exact coords
        // mapping to different global ids.
        let real = soup.verts.len() - 5;
        let ids_for_loc = |target: &[RBig; 3]| -> Vec<usize> {
            (0..real)
                .filter(|&v| exact_coords(&soup.verts[v]) == *target)
                .collect()
        };
        let id0 = ids_for_loc(&locs[0]);
        let id1 = ids_for_loc(&locs[1]);
        assert_eq!(id0.len(), 1, "loc0 must weld to exactly one id");
        assert_eq!(id1.len(), 1, "loc1 must weld to exactly one id");
        assert_ne!(locs[0], locs[1], "the two pierce points differ exactly");
        assert_ne!(
            id0[0], id1[0],
            "distinct intersection points must map to DIFFERENT global ids"
        );
    }

    #[test]
    fn adversary_coincident_point_welds_to_one_id() {
        // The SAME transversal X: each pierce point lies on the conformed edge of
        // BOTH base triangles. The N18 weld must collapse each coincident
        // implicit point to exactly ONE global id (weld IS happening), and that
        // id is referenced by sub-triangles inheriting BOTH the A and the B label.
        let (coords, tris, labels) = transversal_x();
        let soup = mesh_arrangement(&coords, &tris, &labels).expect("transversal X must not error");

        // Every real implicit point appears exactly once (welded).
        assert_implicit_points_welded(&soup);

        let corners: Vec<[RBig; 3]> = (0..6)
            .map(|i| {
                [
                    to_r(coords[3 * i]),
                    to_r(coords[3 * i + 1]),
                    to_r(coords[3 * i + 2]),
                ]
            })
            .collect();
        let locs = intersection_locations(&soup, &corners);
        assert!(!locs.is_empty(), "the X must realize intersection points");

        // At least one welded intersection vertex is shared across A and B
        // sub-triangles — proving the weld unifies the two triangles' copies of
        // the coincident point into ONE id.
        let real = soup.verts.len() - 5;
        let mut shared = false;
        for loc in &locs {
            let vid = (0..real as u32).find(|&v| exact_coords(&soup.verts[v as usize]) == *loc);
            let Some(vid) = vid else { continue };
            let mut on_a = false;
            let mut on_b = false;
            for (t, tri) in soup.tris.iter().enumerate() {
                if tri.contains(&vid) {
                    if soup.labels[t].contains(&A) {
                        on_a = true;
                    }
                    if soup.labels[t].contains(&B) {
                        on_b = true;
                    }
                }
            }
            if on_a && on_b {
                shared = true;
            }
        }
        assert!(
            shared,
            "a coincident pierce point must weld to ONE id shared by both A and B \
             sub-triangles (weld is happening)"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // 5. Planar fast-path == inputs modulo prep.
    //
    // A disjoint pair: out_tris must be EXACTLY the prepped input triangle set —
    // same count, same global-id triples up to per-triangle vertex rotation —
    // no new real vertices (only the 5 jolly tail), jolly_count == 5, labels
    // preserved per triangle.
    // ════════════════════════════════════════════════════════════════

    /// A triple of exact positions in rotation-canonical form
    /// (lexicographically-smallest vertex first, cyclic order preserved — NOT
    /// reflection, since the fast path emits `soup.tri(t)` verbatim which
    /// preserves winding).
    fn rot_canon_pos(t: [[RBig; 3]; 3]) -> [[RBig; 3]; 3] {
        let min_at = (0..3).min_by_key(|&i| t[i].clone()).unwrap();
        [
            t[min_at].clone(),
            t[(min_at + 1) % 3].clone(),
            t[(min_at + 2) % 3].clone(),
        ]
    }

    #[test]
    fn adversary_disjoint_fast_path_is_prepped_inputs() {
        let a = cube(0.0, 0.0, 0.0, 1.0, A);
        let b = cube(5.0, 5.0, 5.0, 1.0, B);
        let (coords, tris, labels) = concat(a, b);
        let n_in_tris = tris.len();
        let n_in_verts = coords.len() / 3;

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("disjoint pair must not error");

        // jolly tail + no new real verts.
        let real = assert_jolly_tail(&soup);
        assert_eq!(soup.jolly_count, 5);
        assert_eq!(
            real, n_in_verts,
            "disjoint fast-path introduces no new real vertices"
        );
        assert_eq!(soup.tris.len(), n_in_tris, "fast-path: same triangle count");
        assert_label_alignment(&soup);

        // out_tris == prepped input tris, compared by resolved vertex POSITIONS
        // (not raw ids). `merge_duplicated_vertices` renumbers vertices by
        // first-appearance order, so a faithful fast-path passthrough legitimately
        // carries DIFFERENT global ids than the input coord-slot ids — comparing
        // ids would spuriously fail. Resolve every triangle's 3 ids to exact
        // coordinates instead. The pipeline scales coords by `compute_multiplier`
        // and the soup stores the SCALED corners, so scale a copy of the input the
        // same way before resolving; then compare multisets of rotation-canonical
        // position triples.
        let mut scaled_in = coords.clone();
        multiply_coordinates(&mut scaled_in, compute_multiplier(&coords));
        let in_pos = |id: u32| -> [RBig; 3] {
            let i = id as usize;
            [
                to_r(scaled_in[3 * i]),
                to_r(scaled_in[3 * i + 1]),
                to_r(scaled_in[3 * i + 2]),
            ]
        };
        let in_tri = |t: [u32; 3]| rot_canon_pos([in_pos(t[0]), in_pos(t[1]), in_pos(t[2])]);
        let soup_tri = |t: [u32; 3]| {
            rot_canon_pos([
                exact_coords(&soup.verts[t[0] as usize]),
                exact_coords(&soup.verts[t[1] as usize]),
                exact_coords(&soup.verts[t[2] as usize]),
            ])
        };

        let mut want: Vec<[[RBig; 3]; 3]> = tris.iter().map(|&t| in_tri(t)).collect();
        let mut got: Vec<[[RBig; 3]; 3]> = soup.tris.iter().map(|&t| soup_tri(t)).collect();
        want.sort();
        got.sort();
        assert_eq!(
            got, want,
            "fast-path out_tris must equal the prepped input geometry \
             (resolved vertex positions, up to per-tri rotation)"
        );

        // Labels preserved per triangle: each fast-path triangle keeps its
        // parent's label. Key by canonical position triple (ids are renumbered).
        let mut want_lbl: Vec<([[RBig; 3]; 3], Label)> = tris
            .iter()
            .zip(labels.iter())
            .map(|(&t, l)| (in_tri(t), l.clone()))
            .collect();
        let mut got_lbl: Vec<([[RBig; 3]; 3], Label)> = soup
            .tris
            .iter()
            .zip(soup.labels.iter())
            .map(|(&t, l)| (soup_tri(t), l.clone()))
            .collect();
        want_lbl.sort_by(|x, y| x.0.cmp(&y.0));
        got_lbl.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(
            got_lbl, want_lbl,
            "fast-path must preserve each triangle's parent label"
        );

        // No implicit (Lpi/Tpi) vertices among real verts — all Explicit.
        for v in 0..real {
            assert!(
                matches!(soup.verts[v], VertexCoords::Explicit(_)),
                "fast-path real vert {v} must be an Explicit input corner"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // 6. Loud-deferral never silent (the P9/P10 guarantee).
    //
    // For every out-of-scope input we can construct, assert a CLASSIFIED Err is
    // returned — and EXPLICITLY assert it is NOT Ok (no silently-wrong/empty
    // soup). Covers: coplanar overlap (CoplanarPairDeferred) and label/tri count
    // mismatch (LabelCountMismatch). (DeepRecursionRequired is covered as an
    // accepted outcome in test #2; degenerate-pair is not deterministically
    // reachable past prep, which removes degenerate tris first — documented.)
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn adversary_out_of_scope_inputs_are_never_silent_ok() {
        // (Coplanar positive-area overlap moved to
        // `adversary_genuinely_coplanar_overlap_is_constructed` — as of PR-4 it
        // is IN scope and CONSTRUCTS, no longer a deferral. The SingleCoplanarEdge
        // vertex-in-edge / through-interior sub-config is likewise now CONSTRUCTED
        // by the N13 tvX/edge-crossing slice — see
        // `adversary_coplanar_edge_through_interior_is_constructed`.)

        // Label/triangle count mismatch is still loud, never a silent
        // truncation.
        let (c2, t2, mut l2) = cube(0.0, 0.0, 0.0, 1.0, A);
        l2.pop();
        let r2 = mesh_arrangement(&c2, &t2, &l2);
        assert!(r2.is_err(), "count mismatch must NOT be Ok, got Ok");
        assert!(
            matches!(r2, Err(ArrangementError::LabelCountMismatch { .. })),
            "count mismatch must be the classified LabelCountMismatch, got {r2:?}"
        );
    }
}

#[cfg(test)]
mod ar3c_tests {
    //! RED oracles for PR-CR-AR3c — `mesh_arrangement` must be input-order-
    //! INDEPENDENT on closed intersection loops.
    //!
    //! Mechanism under test (see `aux_structure::ar3c_tests` for the minimal
    //! 2-triangle anchor): `group_intersection_points` interns intersection
    //! vertices by STRUCTURAL generator-tuple equality, while the C++
    //! reference (`aux_structure.cpp:230 addVertexInSortedList`, comparator
    //! `genericPoint::lessThan`) interns by EXACT GEOMETRY. When a pair's
    //! intersection-segment endpoint lies ON an edge of the pierced triangle,
    //! the swapped pair presentation re-derives that endpoint with different
    //! generators → 3 structural ids for 2 geometric points →
    //! `group_constraint_segments`' `ids.len() != 2` guard SILENTLY drops the
    //! pair's constraint segment from BOTH triangles → 4 of the through-cut's
    //! 16 intersection-loop fence edges go unrealized → BL1 flood-fill leaks
    //! and 6 patches collapse to 2 (the `#[ignore]`d
    //! `adversary_b_generated_ray_permutation_invariance` witness).
    //!
    //! Which presentation trips it depends only on the (lower-id, higher-id)
    //! pair order from `detect_intersecting_pairs`, which flips with the
    //! concat / triangle order — hence the presentation-invariance oracles.

    use std::collections::BTreeMap;

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::soup::{
        merge_duplicated_vertices, mesh_arrangement, remove_degenerate_and_duplicated_triangles,
        ArrangementSoup, Label,
    };
    use crate::arrangements::{
        classify_all, detect_intersecting_pairs, group_constraint_segments,
        group_intersection_points, FastTrimesh, Plane,
    };
    use crate::labeled_arrangement::InputId;
    use crate::labeling::compute_all_patches;
    use crate::processing::multiplier::{compute_multiplier, multiply_coordinates};
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    type Solid = (Vec<f64>, Vec<[u32; 3]>, Vec<Label>);

    // ── fixtures (local copies, mirroring labeling/inside_out fixtures) ──

    /// Axis-aligned box [o, o+s] (per-axis extents), 12 tris, outward winding.
    fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64, label: InputId) -> Solid {
        let p = |x: f64, y: f64, z: f64| (ox + x * sx, oy + y * sy, oz + z * sz);
        let corners = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2], // bottom (z=0)
            [4, 5, 6],
            [4, 6, 7], // top (z=1)
            [0, 1, 5],
            [0, 5, 4], // front (y=0)
            [2, 3, 7],
            [2, 7, 6], // back (y=1)
            [1, 2, 6],
            [1, 6, 5], // right (x=1)
            [3, 0, 4],
            [3, 4, 7], // left (x=0)
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    fn cube_solid(ox: f64, oy: f64, oz: f64, s: f64, label: InputId) -> Solid {
        boxx(ox, oy, oz, s, s, s, label)
    }

    fn concat(s0: Solid, s1: Solid) -> Solid {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    /// The BL1/BL2 through-cut: a square peg through the unit-2 cube. Its two
    /// closed intersection loops (z=0 and z=2 cube faces × peg walls) have
    /// curve endpoints ON cube-face edges — the structural-identity trap.
    fn through_cut() -> Solid {
        concat(
            cube_solid(0.0, 0.0, 0.0, 2.0, A),
            boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
        )
    }

    /// The three input presentations of the SAME geometry.
    fn presentations() -> [(&'static str, Solid); 3] {
        let fwd = through_cut();
        let (coords, tris, labels) = through_cut();
        let rev: Solid = (
            coords,
            tris.into_iter().rev().collect(),
            labels.into_iter().rev().collect(),
        );
        let swapped = concat(
            boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
            cube_solid(0.0, 0.0, 0.0, 2.0, A),
        );
        [
            ("forward", fwd),
            ("reversed-tris", rev),
            ("swapped-concat", swapped),
        ]
    }

    // ── pure-dashu exact coords (test-local copy, same style as `tests`) ──

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }
    fn sub3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    }
    fn cross3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    }
    fn dot3(a: &[RBig; 3], b: &[RBig; 3]) -> RBig {
        &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
    }

    /// Exact coordinates of `Explicit` / `Lpi` (the through-cut produces no
    /// Tpi: all constraint segments are single transversal crossings).
    fn exact_coords(c: &VertexCoords) -> [RBig; 3] {
        let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
        match c {
            VertexCoords::Explicit(p) => to_r3(p),
            VertexCoords::Lpi { line, plane } => {
                let p = to_r3(&line[0]);
                let q = to_r3(&line[1]);
                let r = to_r3(&plane[0]);
                let s = to_r3(&plane[1]);
                let t = to_r3(&plane[2]);
                let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                let num = dot3(&sub3(&r, &p), &n);
                let den = dot3(&sub3(&q, &p), &n);
                assert!(
                    den != RBig::ZERO,
                    "LPI line parallel to plane — bad fixture"
                );
                let u = &num / &den;
                let qp = sub3(&q, &p);
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { .. } => panic!("through-cut fixture must not produce Tpi"),
        }
    }

    // ── stage-level fingerprint: per-geometric-triangle constraint segments ──

    /// Sorted exact corner triple — a presentation-independent key for one
    /// base triangle's GEOMETRY (prep renumbers ids across presentations).
    type TriKey = Vec<[RBig; 3]>;
    /// Sorted list of segments, each the sorted pair of endpoint exact coords.
    type SegSet = Vec<Vec<[RBig; 3]>>;

    /// Run the `mesh_arrangement` pipeline PREFIX (multiplier → prep → CR13 →
    /// AR1 → grouping) and fingerprint `segments_per_tri` by base-triangle
    /// geometry: which constraint segments (as exact endpoint-coordinate
    /// pairs) does each GEOMETRIC base triangle receive?
    fn segments_fingerprint(solid: &Solid) -> BTreeMap<TriKey, SegSet> {
        let (coords, tris, labels) = solid;

        let m = compute_multiplier(coords);
        let mut sc = coords.clone();
        multiply_coordinates(&mut sc, m);
        let (verts, remapped) = merge_duplicated_vertices(&sc, tris);
        let (kept_tris, _kept_labels, _dupl, _src) =
            remove_degenerate_and_duplicated_triangles(&verts, &remapped, labels);
        let soup = FastTrimesh::from_soup(&verts, &kept_tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        let classified = classify_all(&soup, &pairs);
        let (points, _buckets) = group_intersection_points(&soup, &classified);
        let segments_per_tri = group_constraint_segments(&soup, &classified, &points)
            .expect("through-cut must not over-count endpoints");

        let mut out: BTreeMap<TriKey, SegSet> = BTreeMap::new();
        for (t, segs) in segments_per_tri.iter().enumerate() {
            let mut key: TriKey = (0..3)
                .map(|k| {
                    let p = soup.tri_vert(t as u32, k);
                    [to_r(p.x()), to_r(p.y()), to_r(p.z())]
                })
                .collect();
            key.sort();
            let mut set: SegSet = segs
                .iter()
                .map(|s| {
                    let mut pair = vec![
                        exact_coords(&points[s.endpoints.0 as usize].coords),
                        exact_coords(&points[s.endpoints.1 as usize].coords),
                    ];
                    pair.sort();
                    pair
                })
                .collect();
            set.sort();
            let prev = out.insert(key, set);
            assert!(
                prev.is_none(),
                "duplicate geometric base triangle in prepped soup"
            );
        }
        out
    }

    /// Stage-level presentation invariance: the multiset of constraint
    /// segments per GEOMETRIC base triangle is identical for the forward,
    /// reversed-triangle-order, and swapped-concat presentations.
    ///
    /// Pre-AR3c this FAILS: under the flipped pair order, 8 cube-face×peg-wall
    /// pairs (curve endpoints ON cube-face edges) over-count to 3 structural
    /// ids and their segments are silently dropped from both sides.
    #[test]
    fn through_cut_segments_per_tri_presentation_invariant() {
        let [(n0, s0), (n1, s1), (n2, s2)] = presentations();
        let f0 = segments_fingerprint(&s0);
        let f1 = segments_fingerprint(&s1);
        let f2 = segments_fingerprint(&s2);

        // Same geometric triangles in all presentations (prep is order-stable
        // on this fixture: no duplicates/degenerates).
        let keys = |f: &BTreeMap<TriKey, SegSet>| f.keys().cloned().collect::<Vec<_>>();
        assert_eq!(
            keys(&f0),
            keys(&f1),
            "{n0} vs {n1}: same geometric triangles"
        );
        assert_eq!(
            keys(&f0),
            keys(&f2),
            "{n0} vs {n2}: same geometric triangles"
        );

        for (key, set0) in &f0 {
            assert_eq!(
                set0, &f1[key],
                "{n1}: constraint-segment set differs from {n0} on base triangle {key:?}"
            );
            assert_eq!(
                set0, &f2[key],
                "{n2}: constraint-segment set differs from {n0} on base triangle {key:?}"
            );
        }
    }

    // ── end-to-end: fence-edge count + per-label patch counts ──

    /// Count of non-manifold (>2-incident) undirected edges in the output
    /// soup — the BL1 patch fences. The through-cut's two closed rectangular
    /// intersection loops are realized as 4+4 axis-aligned unit segments per
    /// loop = 16 fence edges (each shared by 4 triangles: 2 cube-face + 2
    /// peg-wall sub-triangles).
    fn non_manifold_edge_count(soup: &ArrangementSoup) -> usize {
        let mut e2n: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for tri in &soup.tris {
            for k in 0..3 {
                let (u, v) = (tri[k], tri[(k + 1) % 3]);
                *e2n.entry((u.min(v), u.max(v))).or_insert(0) += 1;
            }
        }
        e2n.values().filter(|&&n| n > 2).count()
    }

    /// Patch count per (canonicalized) surface label.
    fn per_label_patch_counts(soup: &ArrangementSoup) -> BTreeMap<Label, usize> {
        let patches = compute_all_patches(soup).expect("patches");
        let mut counts: BTreeMap<Label, usize> = BTreeMap::new();
        for patch in &patches.patches {
            let mut label = soup.labels[patch[0] as usize].clone();
            label.sort_unstable();
            *counts.entry(label).or_insert(0) += 1;
        }
        counts
    }

    /// End-to-end presentation invariance: all three presentations of the
    /// through-cut must yield 16 non-manifold fence edges and the same
    /// per-label patch counts (A: shell + 2 discs = 3; B: below/band/above
    /// = 3).
    ///
    /// Pre-AR3c the reversed / swapped presentations drop 4 fence segments
    /// (12 non-manifold edges) and the BL1 flood leaks: 1 patch per label.
    #[test]
    fn through_cut_end_to_end_presentation_invariant() {
        for (name, (coords, tris, labels)) in presentations() {
            let soup = mesh_arrangement(&coords, &tris, &labels)
                .unwrap_or_else(|e| panic!("{name}: through-cut must not error: {e:?}"));

            assert_eq!(
                non_manifold_edge_count(&soup),
                16,
                "{name}: the two closed intersection loops must be realized as \
                 16 non-manifold fence edges"
            );

            let counts = per_label_patch_counts(&soup);
            assert_eq!(
                counts.get(&vec![A]).copied(),
                Some(3),
                "{name}: solid A must split into 3 patches (shell + 2 discs), got {counts:?}"
            );
            assert_eq!(
                counts.get(&vec![B]).copied(),
                Some(3),
                "{name}: solid B must split into 3 patches (below/band/above), got {counts:?}"
            );
        }
    }
}

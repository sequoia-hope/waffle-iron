//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR1 — tri-tri intersection → typed intersection vertices
//!
//! First increment of M6 (native port of the Cherchi 2022 arrangement into
//! `cherchi-rs`). For each CR13 candidate intersecting pair, this module ports
//! the per-pair classification + intersection-point construction from
//! `arrangements/code/intersection_classification.cpp`
//! (`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`), emitting a
//! **typed intersection-vertex set per pair** (AR2 re-triangulates from it).
//!
//! **First FFI consumer inside `cherchi-rs`**: the LPI point construction uses
//! `indirect-predicates-sidecar-rs`, so the whole module is gated behind the
//! off-by-default `indirect-predicates` feature (WASM builds with it off).
//!
//! ## Scope (source-faithful — deviation N13 in `docs/yang_deviations.md`)
//!
//! The C++ `intersection_classification.cpp` constructs **only** explicit input
//! vertices + `implicitPoint3D_LPI` points (3 LPI call sites: cpp:290, cpp:324,
//! cpp:358). It builds **zero** TPI points — TPI (`implicitPoint3D_TPI`) is
//! created in `triangulation.cpp::createTPI`, the re-triangulation stage, which
//! the roadmap assigns to PR-CR-AR2. **AR1 therefore builds explicit + LPI only.**
//!
//! AR1 ports the generic **non-coplanar transversal crossing** (the clean core):
//! - `checkVtxInTriangleIntersection` (cpp:734-784): a triangle vertex lies in
//!   the other triangle → [`IntersectionVertex::Explicit`].
//! - `checkSingleNoCoplanarEdgeIntersection` (cpp:679-730): an edge pierces the
//!   other triangle's plane → [`IntersectionVertex::Lpi`].
//!
//! Fully-coplanar pairs (`allCoplanarEdges`, orBA `0 0 0`) and single-coplanar-edge
//! degeneracies (`singleCoplanarEdge`, orBA e.g. `1 0 0`, handled in C++ by
//! `checkSingleCoplanarEdgeIntersections` via jolly points + in-plane edge-edge
//! LPIs) are emitted with a loud [`PairClassification::Deferred`] marker — never
//! silently dropped — and deferred to a later slice.
//!
//! ## Sign-pattern decoders (cpp:834-925)
//!
//! Classification is driven by the `orBA` / `orAB` sign triples: the three
//! `orient3d` signs of one triangle's vertices against the other triangle's
//! supporting plane, normalized to {-1, 0, +1}. The decoders ported here are
//! `normalize_orientations`, `same_orientation`, `all_coplanar_edges`,
//! `single_coplanar_edge`, `vtx_in_plane_and_opposite_edge_on_same_side`,
//! `vtx_in_plane_and_opposite_edge_cross_plane`, and
//! `vtx_on_a_side_and_opposite_edge_on_the_other`. They operate on the EXACT
//! `predicates::orient3d` results on explicit coordinates (matching
//! `cinolib::orient3d` in the C++).

use crate::arrangements::FastTrimesh;
use cad_primitives::Point3;

/// One endpoint of a tri-tri intersection, correctly typed.
///
/// Mirrors the two point kinds the C++ arrangement constructs in
/// `intersection_classification.cpp`: an existing input vertex (explicit) or a
/// line-plane intersection (`implicitPoint3D_LPI`).
#[derive(Clone, Debug, PartialEq)]
pub enum IntersectionVertex {
    /// Coincides with an existing input vertex (explicit point). `tri` is the
    /// soup triangle index, `corner` the 0..=2 corner whose coordinates this
    /// vertex equals (exact equality, no tolerance).
    Explicit { tri: u32, corner: u8, point: Point3 },
    /// Edge of one triangle pierces the plane of the other (an LPI point).
    ///
    /// Stores the LPI *generators* — the two line endpoints (`line`) and the
    /// three plane points (`plane`) — exactly mirroring the C++
    /// `implicitPoint3D_LPI(p, q, r, s, t)` constructor, plus the approximate
    /// explicit coordinates (`approx`) read back via `lambda3d_lpi_*`. The
    /// generators are the load-bearing data (AR2 reconstructs the implicit
    /// point from them); `approx` is for spatial bookkeeping only.
    Lpi {
        line: [Point3; 2],
        plane: [Point3; 3],
        approx: Point3,
    },
}

/// Why a pair was deferred to a later slice (carried loudly, not dropped).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeferReason {
    /// Both triangles are coplanar (`allCoplanarEdges`, orBA `0 0 0`).
    Coplanar,
    /// A single edge of one triangle is coplanar with the other
    /// (`singleCoplanarEdge`, orBA e.g. `1 0 0`).
    SingleCoplanarEdge,
    /// A degenerate configuration AR1 does not handle (e.g. a degenerate
    /// triangle slipping past upstream validation).
    Degenerate,
}

/// Classification of a candidate pair's tri-tri relationship for this PR.
#[derive(Clone, Debug, PartialEq)]
pub enum PairClassification {
    /// Generic non-coplanar transversal crossing — `vertices` populated with
    /// the typed intersection endpoints.
    Transversal { vertices: Vec<IntersectionVertex> },
    /// Deferred to a later slice; carries the reason (loud, not dropped).
    Deferred(DeferReason),
    /// The sign patterns agree there is no real intersection.
    Disjoint,
}

/// Classify a single candidate pair `(ta, tb)` of soup triangles, constructing
/// its typed intersection-vertex set (explicit + LPI) for the transversal case.
///
/// Ports `checkTriangleTriangleIntersections` (cpp:119-280) restricted to the
/// non-coplanar transversal path (deviation N13). See the module docs.
pub fn classify_pair(soup: &FastTrimesh, ta: u32, tb: u32) -> PairClassification {
    // PR-CR-AR1 GREEN phase implements the ported mechanism here.
    let _ = (soup, ta, tb);
    PairClassification::Disjoint
}

/// Classify every candidate pair (e.g. the output of [`detect_intersecting_pairs`]).
///
/// [`detect_intersecting_pairs`]: crate::arrangements::detect_intersecting_pairs
pub fn classify_all(
    soup: &FastTrimesh,
    pairs: &[(u32, u32)],
) -> Vec<((u32, u32), PairClassification)> {
    pairs
        .iter()
        .map(|&(ta, tb)| ((ta, tb), classify_pair(soup, ta, tb)))
        .collect()
}

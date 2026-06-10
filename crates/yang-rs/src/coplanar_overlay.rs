//! Yang 2025 §4.5.5 Stage-0 coplanar overlay — the EXACT 2D Boolean engine
//! (PR-YR25, roadmap M8 slice a).
//!
//! ## Purpose (M8 context)
//!
//! When solids A and B have coplanar overlapping faces, Yang §4.5.5
//! (`refs/text/yang2025_hybrid_boolean.txt:717-731`) requires segmenting the
//! shared plane into A-only / B-only / overlap regions **before**
//! tessellation: "it is necessary to check coplanar planes and perform 2D
//! Boolean operations before mesh discretizations. Two coplanar planes will
//! be segmented into three parts after a Boolean operation in 2D" (Fig. 16,
//! `refs/text/yang2025_hybrid_boolean.txt:752-760`). The overlapping part is
//! replaced by a trimmed common planar surface and **identical meshes are
//! generated for both models** in that part; the boundaries of the common
//! surface become intersection curves between the two models.
//!
//! Downstream effect: exact-duplicate triangles on the overlap weld into
//! multi-label triangles at cherchi prep, so coplanarity vanishes from the
//! mesh arrangement entirely. This module is the geometric engine only —
//! **no pipeline wiring here**; M8 slice b wires it into [`crate::boolean`]
//! (plane detection, 3D→2D frame projection, mesh generation per region).
//!
//! ## Contract
//!
//! Input: two polygons-with-holes already projected into ONE shared 2D frame
//! (the frame choice is the M8b caller's job). Loops must be simple
//! (non-self-intersecting), holes strictly inside their outer loop and
//! mutually disjoint; outer/hole winding direction is irrelevant
//! (classification is parity-based). Violations surface as loud typed errors
//! ([`CoplanarOverlayError::CoverageMismatch`]) — never as silently wrong
//! regions.
//!
//! Output: ONE shared triangulation of the union of both polygons, every
//! triangle classified [`RegionClass::AOnly`] / [`RegionClass::BOnly`] /
//! [`RegionClass::Overlap`], plus derived queries: per-class EXACT rational
//! area ([`ClassifiedOverlay::area_exact`]) and region interface / boundary
//! polylines ([`ClassifiedOverlay::interface_polylines`],
//! [`ClassifiedOverlay::region_boundary_polylines`]) — the Overlap interfaces
//! become intersection curves in M8b.
//!
//! ## Method (exact, decision-procedure style)
//!
//! All geometry is computed over exact rationals (`dashu::rational::RBig`,
//! the same exact backend cherchi-rs uses):
//!
//! 1. **Exact arrangement of edges.** Collect all edges of A and B (outer +
//!    holes). Compute all pairwise intersections exactly: proper crossings,
//!    T-junctions (endpoint on edge interior), and collinear partial
//!    overlaps. Split every edge at every incident point; canonicalize and
//!    dedup the sub-segments (shared edges between A and B — the stacked-box
//!    common case — collapse to ONE sub-segment here).
//! 2. **Conforming triangulation via exact vertical decomposition.** Sweep
//!    the slab between each pair of consecutive event x-coordinates. Inside
//!    a slab no sub-segment endpoint or crossing exists, so the spanning
//!    sub-segments are interior-disjoint and totally ordered by their exact
//!    y at the slab midline. Each band between two vertically-consecutive
//!    sub-segments is a convex cell (trapezoid / triangle); its vertical
//!    sides are subdivided by EVERY arrangement point on the slab line so
//!    that adjacent slabs share vertices exactly (no T-junctions). Each cell
//!    is triangulated by an exact ear-clip (positive-rational-area ears
//!    only).
//!
//!    Why not `cherchi_rs::cdt_polygon_with_holes` (choice (i) of the M8a
//!    brief)? Its public contract takes ONE polygon-with-holes whose loops
//!    become the only constraints — it cannot accept the interior constraint
//!    segments of the overlay (B's edges crossing A's interior), and it is
//!    f64/spade-backed, so the exact coverage post-conditions could not be
//!    certified on the rational coordinates. The vertical decomposition is
//!    choice (ii) — "extract the subdivision cells as polygons then ear-clip
//!    each" — with trapezoidal cells instead of full DCEL faces: it needs no
//!    angular half-edge sorting and no hole-to-face assignment (an island
//!    region is just more bands), and every step is an exact rational
//!    decision procedure.
//! 3. **Exact classification.** Each cell's interior crosses no input edge,
//!    so its parity class is constant; classify ONCE per cell by an exact
//!    even-odd ray test of the cell's midline centroid against A and against
//!    B. In A ∧ in B → `Overlap`; in A only → `AOnly`; in B only → `BOnly`;
//!    in neither → the cell is outside the union and is dropped — guarded by
//!    the exact coverage identity below. Boundary-inclusion rules never
//!    arise: the test point is strictly interior to a cell, hence never ON
//!    any input edge (cell interiors are disjoint from all edges by
//!    construction). Region boundaries belong to the interfaces between
//!    regions, not to either side.
//! 4. **Exact coverage post-conditions (P9/P10 — fail loud).** Before
//!    returning, the engine asserts, in rational arithmetic:
//!    `area(AOnly) + area(Overlap) == area(A)` and
//!    `area(BOnly) + area(Overlap) == area(B)` (input areas by exact shoelace
//!    over the loops). Any mismatch → [`CoplanarOverlayError::CoverageMismatch`].
//!    Every emitted triangle has strictly positive exact area by
//!    construction.
//!
//! ## Rounding boundary
//!
//! Classification, triangulation and the post-conditions all run on the
//! EXACT rational coordinates ([`ClassifiedOverlay::exact_verts`], which the
//! output retains). The f64 [`ClassifiedOverlay::verts`] are derived LAST by
//! rounding each rational coordinate to the nearest f64 — they exist for the
//! downstream mesh, whose vertex coordinates are f64. If that rounding
//! collapses a (exactly positive) triangle to zero or negative f64 area —
//! e.g. two rational intersection points closer than half an ulp — the
//! engine fails LOUDLY with [`CoplanarOverlayError::RoundingCollapse`];
//! it never silently drops or flips a sliver.
//!
//! ## Determinism
//!
//! Single-threaded; all set/map state is `BTreeSet`/`BTreeMap` keyed on
//! exact rationals; sweeps and sorts use total exact orders. Two calls on
//! the same input produce bit-identical output.

use cad_primitives::Point2;
use dashu::rational::RBig;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ───────────────────────────── public types ─────────────────────────────

/// A simple polygon with optional holes, in the shared plane's 2D frame.
///
/// Loops are vertex rings (implicitly closed: last connects back to first).
/// Winding direction is irrelevant. See module docs for validity contract.
#[derive(Clone, Debug)]
pub struct PolygonWithHoles {
    /// Outer boundary loop (≥ 3 vertices).
    pub outer: Vec<Point2>,
    /// Hole loops, each strictly inside `outer` and mutually disjoint.
    pub holes: Vec<Vec<Point2>>,
}

/// Region classification of one output triangle (Yang §4.5.5's "three
/// parts", Fig. 16(c)).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegionClass {
    /// Inside A only.
    AOnly,
    /// Inside B only.
    BOnly,
    /// Inside both — the trimmed common surface region.
    Overlap,
}

/// An exact rational 2D point — the pre-rounding authority for every output
/// vertex. Ordered lexicographically (x, then y).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExactPoint2 {
    /// Exact x coordinate.
    pub x: RBig,
    /// Exact y coordinate.
    pub y: RBig,
}

/// One shared classified triangulation of the union of A and B.
///
/// `tris[i]` indexes into `verts` / `exact_verts`; `class[i]` is its region.
/// Triangles are CCW-positive in the EXACT coordinates.
#[derive(Clone, Debug)]
pub struct ClassifiedOverlay {
    /// Output vertices, rounded to nearest f64 (see "Rounding boundary" in
    /// the module docs). 1:1 with `exact_verts`.
    pub verts: Vec<Point2>,
    /// The exact rational vertex coordinates (classification / area
    /// authority). 1:1 with `verts`.
    pub exact_verts: Vec<ExactPoint2>,
    /// Triangles as index triples into `verts`.
    pub tris: Vec<[u32; 3]>,
    /// Per-triangle region class, 1:1 with `tris`.
    pub class: Vec<RegionClass>,
}

/// Typed errors of the coplanar overlay engine. Every failure is loud; no
/// silent fallback or tolerance path exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoplanarOverlayError {
    /// An input coordinate is NaN or infinite.
    NonFiniteInput,
    /// A loop has fewer than 3 vertices, a repeated consecutive vertex
    /// (zero-length edge), or zero area.
    DegenerateLoop(&'static str),
    /// Rounding the exact vertices to f64 collapsed a positively-oriented
    /// triangle to zero/negative f64 area (sliver collapse). The overlay is
    /// rejected rather than silently dropping or flipping the triangle.
    RoundingCollapse {
        /// The collapsed triangle (indices into the would-be vertex pool).
        tri: [u32; 3],
    },
    /// An internal triangulation invariant broke (cannot happen on
    /// contract-valid input; reported loudly rather than masked).
    TriangulationFailed(&'static str),
    /// The exact coverage identity `area(XOnly) + area(Overlap) == area(X)`
    /// failed for the named side — the input violated the
    /// simple-polygon-with-holes contract (or an internal bug surfaced).
    CoverageMismatch {
        /// `'A'` or `'B'`.
        side: char,
    },
}

impl fmt::Display for CoplanarOverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoplanarOverlayError::NonFiniteInput => {
                write!(f, "coplanar overlay input has NaN/infinite coordinate")
            }
            CoplanarOverlayError::DegenerateLoop(why) => {
                write!(f, "degenerate coplanar overlay input loop: {why}")
            }
            CoplanarOverlayError::RoundingCollapse { tri } => write!(
                f,
                "rounding exact overlay vertices to f64 collapsed triangle \
                 {tri:?} to non-positive area (sliver collapse)"
            ),
            CoplanarOverlayError::TriangulationFailed(why) => {
                write!(f, "coplanar overlay triangulation failed: {why}")
            }
            CoplanarOverlayError::CoverageMismatch { side } => write!(
                f,
                "exact coverage identity area({side}Only) + area(Overlap) == \
                 area({side}) failed — input violates the simple-polygon \
                 contract"
            ),
        }
    }
}

impl std::error::Error for CoplanarOverlayError {}

// ─────────────────────────── derived queries ────────────────────────────

impl ClassifiedOverlay {
    /// EXACT rational area of all triangles of `class` (shoelace over
    /// `exact_verts`). For oracles: `area_exact(AOnly) + area_exact(Overlap)`
    /// equals the exact input area of A (enforced at construction).
    pub fn area_exact(&self, class: RegionClass) -> RBig {
        let mut sum = RBig::ZERO;
        for (tri, c) in self.tris.iter().zip(&self.class) {
            if *c == class {
                let a = &self.exact_verts[tri[0] as usize];
                let b = &self.exact_verts[tri[1] as usize];
                let c3 = &self.exact_verts[tri[2] as usize];
                sum += cross_r(a, b, c3);
            }
        }
        // Shoelace doubles the area; triangles are CCW so each term is ≥ 0.
        sum / RBig::from(2)
    }

    /// Undirected edge → classes of its adjacent triangles (≤ 2 in a
    /// conforming triangulation). Deterministic (BTreeMap).
    fn edge_classes(&self) -> BTreeMap<[u32; 2], Vec<RegionClass>> {
        let mut map: BTreeMap<[u32; 2], Vec<RegionClass>> = BTreeMap::new();
        for (tri, c) in self.tris.iter().zip(&self.class) {
            for k in 0..3 {
                let (i, j) = (tri[k], tri[(k + 1) % 3]);
                let key = if i < j { [i, j] } else { [j, i] };
                map.entry(key).or_default().push(*c);
            }
        }
        map
    }

    /// Edges on the interface between regions `c1` and `c2` (exactly one
    /// adjacent triangle of each). These are the M8b intersection-curve
    /// segments when querying `(Overlap, AOnly)` / `(Overlap, BOnly)`.
    pub fn interface_edges(&self, c1: RegionClass, c2: RegionClass) -> Vec<[u32; 2]> {
        self.edge_classes()
            .into_iter()
            .filter(|(_, classes)| {
                classes.len() == 2
                    && ((classes[0] == c1 && classes[1] == c2)
                        || (classes[0] == c2 && classes[1] == c1))
                    && c1 != c2
            })
            .map(|(e, _)| e)
            .collect()
    }

    /// [`Self::interface_edges`] chained into polylines (vertex-index
    /// chains). Closed loops repeat their first vertex at the end.
    pub fn interface_polylines(&self, c1: RegionClass, c2: RegionClass) -> Vec<Vec<u32>> {
        chain_polylines(self.interface_edges(c1, c2))
    }

    /// All boundary edges of region `class`: edges with exactly one adjacent
    /// triangle of that class (the other side is a different class or
    /// nothing). The full region boundary = interfaces + outer rim.
    pub fn region_boundary_edges(&self, class: RegionClass) -> Vec<[u32; 2]> {
        self.edge_classes()
            .into_iter()
            .filter(|(_, classes)| classes.iter().filter(|c| **c == class).count() == 1)
            .map(|(e, _)| e)
            .collect()
    }

    /// [`Self::region_boundary_edges`] chained into polylines. Closed loops
    /// repeat their first vertex at the end.
    pub fn region_boundary_polylines(&self, class: RegionClass) -> Vec<Vec<u32>> {
        chain_polylines(self.region_boundary_edges(class))
    }
}

/// Chain undirected edges into polylines, deterministically: open chains are
/// walked first (started at the smallest odd-degree vertex), then loops
/// (started at the smallest remaining vertex); each step takes the smallest
/// remaining neighbor. Closed loops repeat their first vertex at the end.
fn chain_polylines(edges: Vec<[u32; 2]>) -> Vec<Vec<u32>> {
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for [i, j] in &edges {
        adj.entry(*i).or_default().insert(*j);
        adj.entry(*j).or_default().insert(*i);
    }
    let mut out = Vec::new();
    loop {
        // Smallest vertex with odd remaining degree (open chain end), else
        // smallest vertex with any remaining edge (loop).
        let start = adj
            .iter()
            .find(|(_, n)| n.len() % 2 == 1)
            .or_else(|| adj.iter().find(|(_, n)| !n.is_empty()))
            .map(|(v, _)| *v);
        let Some(start) = start else { break };
        let mut chain = vec![start];
        let mut cur = start;
        loop {
            let Some(next) = adj.get(&cur).and_then(|n| n.first().copied()) else {
                break;
            };
            adj.get_mut(&cur)
                .expect("adjacency entry exists")
                .remove(&next);
            adj.get_mut(&next)
                .expect("adjacency entry exists")
                .remove(&cur);
            chain.push(next);
            cur = next;
        }
        adj.retain(|_, n| !n.is_empty());
        out.push(chain);
    }
    out
}

// ───────────────────────────── construction ─────────────────────────────

/// Build the exact classified overlay of two polygons-with-holes on a
/// shared plane. See the module docs for the full contract, method, and
/// rounding-boundary design.
pub fn coplanar_overlay(
    a: &PolygonWithHoles,
    b: &PolygonWithHoles,
) -> Result<ClassifiedOverlay, CoplanarOverlayError> {
    // PR-YR25 RED stub — the exact engine lands in the GREEN commit.
    let _ = (a, b);
    Err(CoplanarOverlayError::TriangulationFailed(
        "PR-YR25 RED stub — implemented in GREEN",
    ))
}

// ───────────────────────────── exact helpers ────────────────────────────

/// `cross(b−a, c−a)` — twice the signed area of triangle (a, b, c), exact.
fn cross_r(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> RBig {
    (&b.x - &a.x) * (&c.y - &a.y) - (&b.y - &a.y) * (&c.x - &a.x)
}

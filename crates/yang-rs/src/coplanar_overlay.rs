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
use dashu::float::FBig;
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

impl ExactPoint2 {
    /// Exact lift of finite f64 coordinates (f64 → rational is exact).
    /// `None` on NaN / infinity. Used by the M8b Stage-0 wiring to compare
    /// input-corner coordinates against overlay `exact_verts`.
    pub fn from_f64(x: f64, y: f64) -> Option<Self> {
        Some(ExactPoint2 {
            x: rat(x).ok()?,
            y: rat(y).ok()?,
        })
    }
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
    /// Per-triangle index of the containing side-A input polygon
    /// (`u32::MAX` when the triangle is not inside A), 1:1 with `tris` —
    /// the M8 n-ary attribution ([`coplanar_overlay_multi`]). Always `0`
    /// for `AOnly`/`Overlap` triangles of a single-polygon overlay.
    pub poly_a: Vec<u32>,
    /// Side-B analog of `poly_a`.
    pub poly_b: Vec<u32>,
    /// Fusion record of the step-6 fused-emission repair (spec
    /// m8_overlay_fused_emission_collapse): loser overlay vertex index →
    /// surviving index, fully resolved; empty when no repair ran.
    pub fused: BTreeMap<u32, u32>,
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
        while let Some(next) = adj.get(&cur).and_then(|n| n.first().copied()) {
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
/// rounding-boundary design. Delegates to [`coplanar_overlay_multi`] with
/// one polygon per side (bit-identical output).
pub fn coplanar_overlay(
    a: &PolygonWithHoles,
    b: &PolygonWithHoles,
) -> Result<ClassifiedOverlay, CoplanarOverlayError> {
    coplanar_overlay_multi(std::slice::from_ref(a), std::slice::from_ref(b))
}

/// N-ary form of [`coplanar_overlay`] (M8 plane groups, spec
/// `m8_plane_group_nary_overlay`): each side is a set of
/// polygons-with-holes with pairwise-disjoint interiors (the faces of ONE
/// solid lying on one plane with a common orientation). Side membership is
/// exact even-odd parity over the side's combined edge set — the union of
/// disjoint simple regions — and every inside triangle is additionally
/// attributed to its (unique) containing polygon (`poly_a` / `poly_b`).
/// The exact coverage identity is enforced PER POLYGON:
/// `Σ area(tris attributed to polyᵢ, class ∈ {XOnly, Overlap}) == area(polyᵢ)`
/// — overlapping same-side inputs (a contract violation) fail loudly as
/// [`CoplanarOverlayError::CoverageMismatch`].
pub fn coplanar_overlay_multi(
    a: &[PolygonWithHoles],
    b: &[PolygonWithHoles],
) -> Result<ClassifiedOverlay, CoplanarOverlayError> {
    // ── 0. Validate + lift to exact rationals (per polygon). ────────────
    let polys_a: Vec<Vec<Vec<ExactPoint2>>> = a
        .iter()
        .map(exact_loops)
        .collect::<Result<_, CoplanarOverlayError>>()?;
    let polys_b: Vec<Vec<Vec<ExactPoint2>>> = b
        .iter()
        .map(exact_loops)
        .collect::<Result<_, CoplanarOverlayError>>()?;
    if polys_a.is_empty() || polys_b.is_empty() {
        return Err(CoplanarOverlayError::DegenerateLoop("empty polygon set"));
    }
    let per_poly_edges_a: Vec<Vec<(ExactPoint2, ExactPoint2)>> =
        polys_a.iter().map(|lp| loop_edges(lp)).collect();
    let per_poly_edges_b: Vec<Vec<(ExactPoint2, ExactPoint2)>> =
        polys_b.iter().map(|lp| loop_edges(lp)).collect();
    let edges_a: Vec<(ExactPoint2, ExactPoint2)> =
        per_poly_edges_a.iter().flatten().cloned().collect();
    let edges_b: Vec<(ExactPoint2, ExactPoint2)> =
        per_poly_edges_b.iter().flatten().cloned().collect();

    // ── 1. Exact arrangement: split all edges at all incidences. ────────
    let mut all_edges = edges_a.clone();
    all_edges.extend(edges_b.iter().cloned());
    let subs = split_all(&all_edges);

    // ── 2. Events (distinct endpoint x's) + per-event-line point sets. ──
    let xs: Vec<RBig> = subs
        .iter()
        .flat_map(|s| [s.a.x.clone(), s.b.x.clone()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // Every arrangement point ON the line x = xi: sub-segment endpoints at
    // xi plus the crossing of every sub-segment whose OPEN x-span contains
    // xi. Both vertical sides of every cell are subdivided by this full set,
    // so adjacent slabs share vertices exactly (conformity, no T-junctions).
    let line_pts: Vec<Vec<RBig>> = xs
        .iter()
        .map(|xi| {
            let mut set: BTreeSet<RBig> = BTreeSet::new();
            for s in &subs {
                if s.a.x == *xi {
                    set.insert(s.a.y.clone());
                }
                if s.b.x == *xi {
                    set.insert(s.b.y.clone());
                }
                if s.a.x < *xi && *xi < s.b.x {
                    set.insert(y_at(s, xi));
                }
            }
            set.into_iter().collect()
        })
        .collect();

    // ── 3. Sweep slabs: build, classify, and ear-clip each cell. ────────
    let mut pool: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut exact_verts: Vec<ExactPoint2> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut class: Vec<RegionClass> = Vec::new();
    let mut poly_a: Vec<u32> = Vec::new();
    let mut poly_b: Vec<u32> = Vec::new();
    let two = RBig::from(2);

    for w in 0..xs.len().saturating_sub(1) {
        let (xl, xr) = (&xs[w], &xs[w + 1]);
        let xm = (xl + xr) / &two;

        // Sub-segments spanning the whole slab (no event lies strictly
        // inside it, so partial overlap is impossible). Vertical
        // sub-segments lie ON an event line and bound no band.
        let mut active: Vec<(RBig, &Sub)> = subs
            .iter()
            .filter(|s| s.a.x != s.b.x && s.a.x <= *xl && s.b.x >= *xr)
            .map(|s| (y_at(s, &xm), s))
            .collect();
        // Interior-disjoint + deduped ⇒ midline y's are strictly distinct.
        active.sort_by(|p, q| p.0.cmp(&q.0));

        for pair in active.windows(2) {
            let (ylo_m, lo) = (&pair[0].0, pair[0].1);
            let (yhi_m, hi) = (&pair[1].0, pair[1].1);

            // Exact parity classification at the cell's midline centroid —
            // strictly interior to the cell, hence never ON any input edge.
            // Per-polygon parity gives both side membership (inside ANY of
            // the side's disjoint polygons) and the n-ary attribution index.
            let centroid = ExactPoint2 {
                x: xm.clone(),
                y: (ylo_m + yhi_m) / &two,
            };
            let pa = per_poly_edges_a
                .iter()
                .position(|es| point_in_even_odd(&centroid, es));
            let pb = per_poly_edges_b
                .iter()
                .position(|es| point_in_even_odd(&centroid, es));
            let cls = match (pa.is_some(), pb.is_some()) {
                (true, true) => RegionClass::Overlap,
                (true, false) => RegionClass::AOnly,
                (false, true) => RegionClass::BOnly,
                // Outside the union: dropped; guarded by the exact coverage
                // identity below.
                (false, false) => continue,
            };
            let (pa, pb) = (
                pa.map_or(u32::MAX, |i| i as u32),
                pb.map_or(u32::MAX, |i| i as u32),
            );

            // CCW cell ring: bottom-left → bottom-right → up the right side
            // (subdivided) → top-right → top-left → down the left side
            // (subdivided).
            let ybl = y_at(lo, xl);
            let ybr = y_at(lo, xr);
            let ytl = y_at(hi, xl);
            let ytr = y_at(hi, xr);
            let mut ring: Vec<ExactPoint2> = Vec::new();
            let push = |ring: &mut Vec<ExactPoint2>, x: &RBig, y: &RBig| {
                let p = ExactPoint2 {
                    x: x.clone(),
                    y: y.clone(),
                };
                if ring.last() != Some(&p) {
                    ring.push(p);
                }
            };
            push(&mut ring, xl, &ybl);
            push(&mut ring, xr, &ybr);
            for y in &line_pts[w + 1] {
                if *y > ybr && *y < ytr {
                    push(&mut ring, xr, y);
                }
            }
            push(&mut ring, xr, &ytr);
            push(&mut ring, xl, &ytl);
            for y in line_pts[w].iter().rev() {
                if *y > ybl && *y < ytl {
                    push(&mut ring, xl, y);
                }
            }
            if ring.last() == ring.first() {
                ring.pop();
            }
            if ring.len() < 3 {
                return Err(CoplanarOverlayError::TriangulationFailed(
                    "degenerate cell ring (duplicate sub-segment?)",
                ));
            }

            for t in ear_clip(&ring)? {
                let gi = [
                    intern(&mut pool, &mut exact_verts, &ring[t[0]]),
                    intern(&mut pool, &mut exact_verts, &ring[t[1]]),
                    intern(&mut pool, &mut exact_verts, &ring[t[2]]),
                ];
                tris.push(gi);
                class.push(cls);
                poly_a.push(pa);
                poly_b.push(pb);
            }
        }
    }

    // ── 4. Round to f64 LAST (see "Rounding boundary" in module docs). ──
    let mut verts: Vec<Point2> = Vec::with_capacity(exact_verts.len());
    for ev in &exact_verts {
        let (x, y) = (ev.x.to_f64().value(), ev.y.to_f64().value());
        if !x.is_finite() || !y.is_finite() {
            return Err(CoplanarOverlayError::TriangulationFailed(
                "exact vertex rounds outside the f64 range",
            ));
        }
        verts.push(Point2::new(x, y));
    }

    let mut overlay = ClassifiedOverlay {
        verts,
        exact_verts,
        tris,
        class,
        poly_a,
        poly_b,
        fused: BTreeMap::new(),
    };

    // ── 5. Exact coverage post-conditions (P9/P10 — loud). Run on the FULL
    // (pre-filter) overlay: the exact areas include every triangle, so this
    // validates the exact 2D boolean is correct BEFORE the f64-collapse filter
    // below ever drops anything. Enforced PER POLYGON: the area of the
    // triangles attributed to polyᵢ (class XOnly/Overlap) must equal polyᵢ's
    // exact input area — this is both the side identity (summed) and the
    // n-ary attribution oracle; overlapping same-side inputs fail here. ────
    for (side, polys, attribution, own_class) in [
        ('A', &polys_a, &overlay.poly_a, RegionClass::AOnly),
        ('B', &polys_b, &overlay.poly_b, RegionClass::BOnly),
    ] {
        for (pi, loops) in polys.iter().enumerate() {
            let mut sum = RBig::ZERO;
            for ((tri, c), &attr) in overlay.tris.iter().zip(&overlay.class).zip(attribution) {
                if attr == pi as u32 && (*c == own_class || *c == RegionClass::Overlap) {
                    let a2 = &overlay.exact_verts[tri[0] as usize];
                    let b2 = &overlay.exact_verts[tri[1] as usize];
                    let c2 = &overlay.exact_verts[tri[2] as usize];
                    sum += cross_r(a2, b2, c2);
                }
            }
            if sum / RBig::from(2) != input_area(loops) {
                return Err(CoplanarOverlayError::CoverageMismatch { side });
            }
        }
    }

    // ── 6. f64-emission gate (spec m8_overlay_fused_emission_collapse §3).
    // Every triangle is exactly CCW-positive by construction; it must also be
    // strictly positive in the ROUNDED f64 coordinates for the downstream
    // exact mesh boolean to consume it. A positive exact triangle can round
    // non-positive two ways (`rounded_tri_disposition`):
    //   * CoincidentNeedle — two verts round to the SAME f64 point (distinct
    //     exact verts a sub-ulp apart). Benign: the downstream coordinate
    //     interner welds f64-identical points to one index, so the needle's
    //     two non-zero edges are the SAME edge — dropping it leaves no f64 gap
    //     and no flip. The exact coverage above already proved the overlay
    //     correct.
    //   * CollinearSliver — three DISTINCT f64 verts rounded collinear (or
    //     order-inverted): its removal could gap, its retention flips a
    //     neighbour.
    //
    // Branch B2 — if NO triangle is a CollinearSliver: the legacy path (keep
    // Positive, drop CoincidentNeedle) is byte-identical and `fused` stays
    // empty. This preserves bit-identical output for every currently-passing
    // input (zero-regression requirement).
    //
    // Branch B3 — otherwise the fused-emission repair runs: sub-f64-resolution
    // degenerate complexes are collapsed (constrained edge collapse, [#51]
    // Hoppe validity gate in exact arithmetic) until the rounded image is
    // f64-emittable, or a real-scale (supra-TAU_MODEL) sliver keeps the loud
    // wall (B5/B6). Local T-subdivision over the FIXED rounded vertex set was
    // prototyped and REFUTED 2026-07-10 (spec `m8_overlay_femto_slab_emission`
    // §8, P10 abort): the corpus slivers are chord-collinear mint triples and
    // order-inverted twin mints for which no fixed-vertex-set triangulation is
    // positive — the repair removes that fixed-vertex premise.
    let any_sliver = overlay.tris.iter().any(|t| {
        rounded_tri_disposition(
            overlay.verts[t[0] as usize],
            overlay.verts[t[1] as usize],
            overlay.verts[t[2] as usize],
        ) == RoundedTri::CollinearSliver
    });

    if any_sliver {
        // B3: survivor preference set — an input-loop vertex outranks a minted
        // arrangement vertex (fusing a mint INTO existing input geometry
        // minimises downstream churn, spec §3). Re-derived from this overlay's
        // own exact inputs.
        let input_loop_verts: BTreeSet<ExactPoint2> = polys_a
            .iter()
            .chain(polys_b.iter())
            .flat_map(|poly| poly.iter().flatten().cloned())
            .collect();
        fused_emission_repair(&mut overlay, &input_loop_verts, &all_edges)?;
    } else {
        // B2 legacy path — byte-identical (zero-regression requirement). No
        // CollinearSliver exists in this branch, so that arm is unreachable.
        let mut kept_tris = Vec::with_capacity(overlay.tris.len());
        let mut kept_class = Vec::with_capacity(overlay.class.len());
        let mut kept_pa = Vec::with_capacity(overlay.poly_a.len());
        let mut kept_pb = Vec::with_capacity(overlay.poly_b.len());
        for (i, (tri, cls)) in overlay.tris.iter().zip(overlay.class.iter()).enumerate() {
            let a2 = overlay.verts[tri[0] as usize];
            let b2 = overlay.verts[tri[1] as usize];
            let c2 = overlay.verts[tri[2] as usize];
            match rounded_tri_disposition(a2, b2, c2) {
                RoundedTri::Positive => {
                    kept_tris.push(*tri);
                    kept_class.push(*cls);
                    kept_pa.push(overlay.poly_a[i]);
                    kept_pb.push(overlay.poly_b[i]);
                }
                RoundedTri::CoincidentNeedle => continue,
                RoundedTri::CollinearSliver => {
                    probe_sliver(&overlay, tri, &all_edges, "collapse");
                    return Err(CoplanarOverlayError::RoundingCollapse { tri: *tri });
                }
            }
        }
        overlay.tris = kept_tris;
        overlay.class = kept_class;
        overlay.poly_a = kept_pa;
        overlay.poly_b = kept_pb;
    }

    Ok(overlay)
}

/// Fused-emission repair at the step-6 rounding gate (spec
/// `m8_overlay_fused_emission_collapse` §3, branches B3–B8). Entered only when
/// ≥1 triangle rounds to a `CollinearSliver`. Collapses the edges of
/// sub-f64-resolution degenerate complexes (constrained edge collapse, [#51]
/// Hoppe with a validity gate in EXACT arithmetic) — recording each
/// `fused[loser] = survivor` — until no `CollinearSliver` remains, then drops
/// the resulting index-degenerate triangles and any residual benign needle.
/// A pass that commits nothing while a sliver remains is the honest loud wall
/// (B5). Mutates `overlay.tris`/`class`/`poly_a`/`poly_b`/`fused` in place;
/// `verts`/`exact_verts` are never compacted (spec §3).
fn fused_emission_repair(
    overlay: &mut ClassifiedOverlay,
    input_loop_verts: &BTreeSet<ExactPoint2>,
    all_edges: &[(ExactPoint2, ExactPoint2)],
) -> Result<(), CoplanarOverlayError> {
    // Eligibility ceiling: an edge is fusible only if its EXACT squared length
    // is < TAU_MODEL² (spec §2). TAU_MODEL comes from the centralized policy
    // (A14.3, no ad-hoc epsilon); it is the KV15b precedent constant — the
    // R0091 revert proved MIN_FEATURE_SIZE would wrongly fuse real micro-scale
    // geometry. This is a CEILING on what MAY fuse, not a trigger: the trigger
    // is exact f64 degeneracy of the rounded image, so real-scale
    // exactly-collinear slivers (edges ≥ TAU_MODEL) stay a loud wall (B6).
    let tau = rat(cad_primitives::TAU_MODEL)
        .map_err(|_| CoplanarOverlayError::TriangulationFailed("TAU_MODEL is not finite"))?;
    let tau2 = &tau * &tau;

    // A triangle is "dropped" once a collapse remaps two of its corners to the
    // same index — its sub-TAU_MODEL exact area is absorbed (spec I5). Rather
    // than compact mid-repair (which would invalidate the index-based
    // worklist, spec I6), degenerate triangles stay in `tris` and are simply
    // skipped everywhere until the final cleanup.
    let is_degenerate = |t: &[u32; 3]| t[0] == t[1] || t[1] == t[2] || t[0] == t[2];
    let disp = |ov: &ClassifiedOverlay, t: &[u32; 3]| {
        rounded_tri_disposition(
            ov.verts[t[0] as usize],
            ov.verts[t[1] as usize],
            ov.verts[t[2] as usize],
        )
    };

    loop {
        // Worklist: live triangles whose ROUNDED disposition is non-Positive
        // (needles AND slivers both fuse in repair mode), ascending index.
        let mut worklist: Vec<usize> = Vec::new();
        let mut first_sliver: Option<usize> = None;
        for (ti, t) in overlay.tris.iter().enumerate() {
            if is_degenerate(t) {
                continue;
            }
            match disp(overlay, t) {
                RoundedTri::Positive => {}
                RoundedTri::CoincidentNeedle => worklist.push(ti),
                RoundedTri::CollinearSliver => {
                    worklist.push(ti);
                    if first_sliver.is_none() {
                        first_sliver = Some(ti);
                    }
                }
            }
        }
        // Success: only Positive triangles and benign needles remain.
        let Some(stuck) = first_sliver else { break };

        let mut committed = false;
        'wl: for &ti in &worklist {
            let t = overlay.tris[ti];
            // Re-check: an earlier commit this pass may have dropped this
            // triangle or already made it Positive (a healthy triangle is
            // never fused — its edges are all supra-ceiling anyway).
            if is_degenerate(&t) || disp(overlay, &t) == RoundedTri::Positive {
                continue;
            }
            // Candidate edges in ascending EXACT squared length; deterministic
            // tie-break by the lexicographically smaller [min, max] index pair.
            let mut cands: Vec<([u32; 2], RBig)> = Vec::with_capacity(3);
            for k in 0..3 {
                let (i, j) = (t[k], t[(k + 1) % 3]);
                let (lo, hi) = if i < j { (i, j) } else { (j, i) };
                let dx = &overlay.exact_verts[lo as usize].x - &overlay.exact_verts[hi as usize].x;
                let dy = &overlay.exact_verts[lo as usize].y - &overlay.exact_verts[hi as usize].y;
                cands.push(([lo, hi], &dx * &dx + &dy * &dy));
            }
            cands.sort_by(|(e0, l0), (e1, l1)| l0.cmp(l1).then(e0.cmp(e1)));

            for ([lo, hi], len2) in &cands {
                // B6: real-scale edges are never fused (ceiling < TAU_MODEL²).
                if len2 >= &tau2 {
                    continue;
                }
                // Survivor selection (spec §3): an input-loop vertex outranks a
                // mint; both/neither → the smaller overlay index survives. The
                // survivor keeps its OWN exact bits (KV15b min-index precedent,
                // never an average).
                let lo_in = input_loop_verts.contains(&overlay.exact_verts[*lo as usize]);
                let hi_in = input_loop_verts.contains(&overlay.exact_verts[*hi as usize]);
                let (survivor, loser) = match (lo_in, hi_in) {
                    (true, false) => (*lo, *hi),
                    (false, true) => (*hi, *lo),
                    _ => (*lo, *hi),
                };

                // Validity gate ([#51] Hoppe link/fold condition, in EXACT
                // arithmetic — P9, no silent flip): tentatively remap
                // loser→survivor over every live triangle. Triangles that go
                // index-degenerate collapse away (absorbed, I5); every OTHER
                // remapped triangle must keep strictly positive EXACT area. Any
                // violation rejects this candidate; the triangle then tries its
                // next edge, or is left for a later pass (B7).
                let ok = overlay.tris.iter().all(|other| {
                    if is_degenerate(other) || !other.contains(&loser) {
                        return true;
                    }
                    let r = [
                        if other[0] == loser {
                            survivor
                        } else {
                            other[0]
                        },
                        if other[1] == loser {
                            survivor
                        } else {
                            other[1]
                        },
                        if other[2] == loser {
                            survivor
                        } else {
                            other[2]
                        },
                    ];
                    if is_degenerate(&r) {
                        return true; // collapses away — absorbed
                    }
                    cross_r(
                        &overlay.exact_verts[r[0] as usize],
                        &overlay.exact_verts[r[1] as usize],
                        &overlay.exact_verts[r[2] as usize],
                    ) > RBig::ZERO
                });
                if !ok {
                    continue;
                }

                // Commit: remap loser→survivor across all triangles, then
                // record the fusion path-compressed so `fused` stays fully
                // resolved — values are never keys (spec §3): every existing
                // entry that pointed at the loser is rewritten to the survivor.
                for tt in overlay.tris.iter_mut() {
                    for v in tt.iter_mut() {
                        if *v == loser {
                            *v = survivor;
                        }
                    }
                }
                for v in overlay.fused.values_mut() {
                    if *v == loser {
                        *v = survivor;
                    }
                }
                overlay.fused.insert(loser, survivor);
                committed = true;
                continue 'wl;
            }
            // No eligible candidate for this triangle this pass — leave it.
        }

        if !committed {
            // B5: a full pass committed nothing while a sliver remains — the
            // honest wall is preserved (loud), same probe hook before return.
            let stuck_tri = overlay.tris[stuck];
            probe_sliver(overlay, &stuck_tri, all_edges, "collapse");
            return Err(CoplanarOverlayError::RoundingCollapse { tri: stuck_tri });
        }
    }

    // Cleanup (spec §3 step 4): drop index-degenerate triangles and any
    // remaining CoincidentNeedle (benign weld, B8 — same argument as the B2
    // legacy path), keep everything else. No vertex compaction.
    let mut kept_tris = Vec::with_capacity(overlay.tris.len());
    let mut kept_class = Vec::with_capacity(overlay.class.len());
    let mut kept_pa = Vec::with_capacity(overlay.poly_a.len());
    let mut kept_pb = Vec::with_capacity(overlay.poly_b.len());
    for (i, t) in overlay.tris.iter().enumerate() {
        if is_degenerate(t) {
            continue;
        }
        match disp(overlay, t) {
            RoundedTri::Positive => {
                kept_tris.push(*t);
                kept_class.push(overlay.class[i]);
                kept_pa.push(overlay.poly_a[i]);
                kept_pb.push(overlay.poly_b[i]);
            }
            RoundedTri::CoincidentNeedle => continue,
            RoundedTri::CollinearSliver => {
                // Unreachable: the loop only exits with no sliver remaining.
                return Err(CoplanarOverlayError::TriangulationFailed(
                    "sliver survived fused-emission repair",
                ));
            }
        }
    }
    overlay.tris = kept_tris;
    overlay.class = kept_class;
    overlay.poly_a = kept_pa;
    overlay.poly_b = kept_pb;
    Ok(())
}

/// Diagnosis probe (read-only, env-gated on `YANG_POLY_PROBE`): report an
/// irreparable sliver's rounded coordinates plus a per-edge structure
/// census (on-input-segment flags, neighbour triangle + classes) so a
/// femto-slab collapse can be joined back to its minting event columns.
fn probe_sliver(
    overlay: &ClassifiedOverlay,
    tri: &[u32; 3],
    input_edges: &[(ExactPoint2, ExactPoint2)],
    why: &str,
) {
    if std::env::var_os("YANG_POLY_PROBE").is_none() {
        return;
    }
    let v = |i: u32| overlay.verts[i as usize];
    if let Some(pos) = overlay.tris.iter().position(|t| t == tri) {
        eprintln!("[sliver-probe] {why} self-class={:?}", overlay.class[pos]);
    }
    eprintln!(
        "[sliver-probe] {why} tri {tri:?} verts ({},{}) ({},{}) ({},{})",
        v(tri[0]).x(),
        v(tri[0]).y(),
        v(tri[1]).x(),
        v(tri[1]).y(),
        v(tri[2]).x(),
        v(tri[2]).y()
    );
    for k in 0..3 {
        let (i, j) = (tri[k], tri[(k + 1) % 3]);
        let (p, q) = (
            &overlay.exact_verts[i as usize],
            &overlay.exact_verts[j as usize],
        );
        let on_input = input_edges.iter().any(|(e0, e1)| {
            cross_r(e0, e1, p) == RBig::ZERO
                && cross_r(e0, e1, q) == RBig::ZERO
                && between_box(e0, e1, p)
                && between_box(e0, e1, q)
        });
        let nb = overlay
            .tris
            .iter()
            .zip(&overlay.class)
            .enumerate()
            .find(|(_, (t, _))| *t != tri && t.contains(&i) && t.contains(&j));
        eprintln!(
            "[sliver-probe]   edge ({i},{j}) on-input={on_input} neighbor={:?}",
            nb.map(|(ti, (_, c))| (ti, *c)),
        );
    }
    // Local-complex census: every triangle touching the sliver's vertices,
    // with rounded coords, class, and rounded disposition — joins a stuck
    // sliver back to its femto cluster (chord-collinear mint triples,
    // order-inverted twin pairs).
    for (j, (t, c)) in overlay.tris.iter().zip(&overlay.class).enumerate() {
        if t.iter().any(|vt| tri.contains(vt)) {
            let d = rounded_tri_disposition(v(t[0]), v(t[1]), v(t[2]));
            let vv = |i: u32| {
                let p = v(i);
                format!("{}@({},{})", i, p.x(), p.y())
            };
            eprintln!(
                "[pocket-probe] tri {j} {:?} {c:?} {d:?}: {} {} {}",
                t,
                vv(t[0]),
                vv(t[1]),
                vv(t[2])
            );
        }
    }
}

/// How a positively-oriented EXACT overlay triangle fares when its vertices are
/// rounded to f64 — the discrimination behind the step-6 sliver-collapse gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RoundedTri {
    /// Still strictly CCW-positive in f64 — keep it.
    Positive,
    /// Two of the three verts rounded to the SAME f64 point (a zero-EXTENT
    /// needle — distinct exact verts a sub-ulp apart). Benign: the downstream
    /// coordinate interner welds f64-identical points to one index, so the
    /// needle's two non-zero edges are the SAME edge — dropping it leaves no
    /// f64 gap and no flip.
    CoincidentNeedle,
    /// Three DISTINCT f64 verts that rounded into collinearity — a real sliver
    /// whose removal could leave a gap or whose retention flips a neighbour. A
    /// LOUD reject (P9 — never silently dropped).
    CollinearSliver,
}

fn rounded_tri_disposition(a: Point2, b: Point2, c: Point2) -> RoundedTri {
    let area2 = (b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x());
    // `partial_cmp` keeps this total over a (cannot-occur) NaN.
    if area2.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater) {
        return RoundedTri::Positive;
    }
    let same = |p: Point2, q: Point2| p.x() == q.x() && p.y() == q.y();
    if same(a, b) || same(b, c) || same(c, a) {
        RoundedTri::CoincidentNeedle
    } else {
        RoundedTri::CollinearSliver
    }
}

// ───────────────────────────── exact helpers ────────────────────────────

/// `cross(b−a, c−a)` — twice the signed area of triangle (a, b, c), exact.
/// (`pub(crate)` since PR-YR26 for the Stage-0 ring-orientation check.)
pub(crate) fn cross_r(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> RBig {
    (&b.x - &a.x) * (&c.y - &a.y) - (&b.y - &a.y) * (&c.x - &a.x)
}

/// `q` within the closed axis-aligned bounding box of `[a, b]` (used with
/// exactly collinear `q` for on-segment tests).
fn between_box(a: &ExactPoint2, b: &ExactPoint2, q: &ExactPoint2) -> bool {
    let (xlo, xhi) = if a.x <= b.x {
        (&a.x, &b.x)
    } else {
        (&b.x, &a.x)
    };
    let (ylo, yhi) = if a.y <= b.y {
        (&a.y, &b.y)
    } else {
        (&b.y, &a.y)
    };
    &q.x >= xlo && &q.x <= xhi && &q.y >= ylo && &q.y <= yhi
}

/// Exact f64 → RBig; fails on NaN / infinity. (`pub(crate)` since N2-3a for
/// the Stage-0 exact rim circle∩line quadratic.)
pub(crate) fn rat(x: f64) -> Result<RBig, CoplanarOverlayError> {
    let fb: FBig = FBig::try_from(x).map_err(|_| CoplanarOverlayError::NonFiniteInput)?;
    RBig::try_from(fb).map_err(|_| CoplanarOverlayError::NonFiniteInput)
}

/// Validate one input polygon and lift its loops to exact coordinates.
fn exact_loops(p: &PolygonWithHoles) -> Result<Vec<Vec<ExactPoint2>>, CoplanarOverlayError> {
    let mut out = Vec::with_capacity(1 + p.holes.len());
    for lp in std::iter::once(&p.outer).chain(p.holes.iter()) {
        if lp.len() < 3 {
            return Err(CoplanarOverlayError::DegenerateLoop(
                "loop has fewer than 3 vertices",
            ));
        }
        let exact: Vec<ExactPoint2> = lp
            .iter()
            .map(|v| {
                Ok(ExactPoint2 {
                    x: rat(v.x())?,
                    y: rat(v.y())?,
                })
            })
            .collect::<Result<_, CoplanarOverlayError>>()?;
        let n = exact.len();
        let mut area2 = RBig::ZERO;
        for i in 0..n {
            let (p0, p1) = (&exact[i], &exact[(i + 1) % n]);
            if p0 == p1 {
                return Err(CoplanarOverlayError::DegenerateLoop(
                    "zero-length edge (repeated consecutive vertex)",
                ));
            }
            area2 += &p0.x * &p1.y - &p1.x * &p0.y;
        }
        if area2 == RBig::ZERO {
            return Err(CoplanarOverlayError::DegenerateLoop("zero-area loop"));
        }
        out.push(exact);
    }
    Ok(out)
}

/// All directed edges of a loop set (closing edge included).
fn loop_edges(loops: &[Vec<ExactPoint2>]) -> Vec<(ExactPoint2, ExactPoint2)> {
    let mut edges = Vec::new();
    for lp in loops {
        for i in 0..lp.len() {
            edges.push((lp[i].clone(), lp[(i + 1) % lp.len()].clone()));
        }
    }
    edges
}

/// Exact area of a simple polygon-with-holes: |shoelace(outer)| − Σ
/// |shoelace(hole)|. The contract-side half of the coverage identity.
fn input_area(loops: &[Vec<ExactPoint2>]) -> RBig {
    let shoelace_abs = |lp: &[ExactPoint2]| -> RBig {
        let mut s = RBig::ZERO;
        for i in 0..lp.len() {
            let (p, q) = (&lp[i], &lp[(i + 1) % lp.len()]);
            s += &p.x * &q.y - &q.x * &p.y;
        }
        if s < RBig::ZERO {
            s = -s;
        }
        s / RBig::from(2)
    };
    let mut area = shoelace_abs(&loops[0]);
    for hole in &loops[1..] {
        area -= shoelace_abs(hole);
    }
    area
}

// ─────────────────────── arrangement (edge splitting) ───────────────────

/// One arrangement sub-segment, endpoints in canonical (lexicographic)
/// order: `a ≤ b`, so for non-vertical sub-segments `a.x < b.x`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Sub {
    a: ExactPoint2,
    b: ExactPoint2,
}

impl Sub {
    /// Canonicalize; `None` for a zero-length segment.
    fn new(p: ExactPoint2, q: ExactPoint2) -> Option<Self> {
        match p.cmp(&q) {
            std::cmp::Ordering::Less => Some(Sub { a: p, b: q }),
            std::cmp::Ordering::Greater => Some(Sub { a: q, b: p }),
            std::cmp::Ordering::Equal => None,
        }
    }
}

/// Exact y of a non-vertical sub-segment at x.
fn y_at(s: &Sub, x: &RBig) -> RBig {
    &s.a.y + (x - &s.a.x) * (&s.b.y - &s.a.y) / (&s.b.x - &s.a.x)
}

/// All pairwise split points between two edges: proper crossings,
/// T-junctions (endpoint on interior — `t∈{0,1}` on one side, `(0,1)` on
/// the other), and collinear partial overlaps (each segment split at the
/// other's interior endpoints). Returns (splits-for-s, splits-for-t).
#[allow(clippy::type_complexity)]
fn pair_splits(
    s: &(ExactPoint2, ExactPoint2),
    t: &(ExactPoint2, ExactPoint2),
) -> (Vec<ExactPoint2>, Vec<ExactPoint2>) {
    let d1 = (&s.1.x - &s.0.x, &s.1.y - &s.0.y);
    let d2 = (&t.1.x - &t.0.x, &t.1.y - &t.0.y);
    let w = (&t.0.x - &s.0.x, &t.0.y - &s.0.y);
    let denom = &d1.0 * &d2.1 - &d1.1 * &d2.0;

    let mut for_s = Vec::new();
    let mut for_t = Vec::new();

    if denom != RBig::ZERO {
        // s.0 + (tn/den)·d1 == t.0 + (un/den)·d2
        let mut tn = &w.0 * &d2.1 - &w.1 * &d2.0;
        let mut un = &w.0 * &d1.1 - &w.1 * &d1.0;
        let mut den = denom;
        if den < RBig::ZERO {
            tn = -tn;
            un = -un;
            den = -den;
        }
        let in_01 = |n: &RBig| *n >= RBig::ZERO && *n <= den;
        let strict_01 = |n: &RBig| *n > RBig::ZERO && *n < den;
        if in_01(&tn) && in_01(&un) {
            let p = ExactPoint2 {
                x: &s.0.x + d1.0 * &tn / &den,
                y: &s.0.y + d1.1 * &tn / &den,
            };
            if strict_01(&tn) {
                for_s.push(p.clone());
            }
            if strict_01(&un) {
                for_t.push(p);
            }
        }
    } else if &w.0 * &d1.1 - &w.1 * &d1.0 == RBig::ZERO {
        // Collinear: split each at the other's strictly-interior endpoints.
        for q in [&t.0, &t.1] {
            if strictly_inside_collinear(s, q) {
                for_s.push(q.clone());
            }
        }
        for q in [&s.0, &s.1] {
            if strictly_inside_collinear(t, q) {
                for_t.push(q.clone());
            }
        }
    }
    (for_s, for_t)
}

/// `q` strictly interior to segment `e` — `q` is already known collinear.
fn strictly_inside_collinear(e: &(ExactPoint2, ExactPoint2), q: &ExactPoint2) -> bool {
    let d = (&e.1.x - &e.0.x, &e.1.y - &e.0.y);
    let tq = (&q.x - &e.0.x) * &d.0 + (&q.y - &e.0.y) * &d.1;
    let dd = &d.0 * &d.0 + &d.1 * &d.1;
    tq > RBig::ZERO && tq < dd
}

/// Split every edge at every pairwise incidence, canonicalize, dedup.
/// Shared A/B edges and collinear overlaps collapse to single sub-segments.
fn split_all(edges: &[(ExactPoint2, ExactPoint2)]) -> Vec<Sub> {
    let n = edges.len();
    let mut splits: Vec<Vec<ExactPoint2>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in (i + 1)..n {
            let (si, sj) = pair_splits(&edges[i], &edges[j]);
            splits[i].extend(si);
            splits[j].extend(sj);
        }
    }
    let mut set: BTreeSet<Sub> = BTreeSet::new();
    for (e, pts) in edges.iter().zip(splits) {
        let d = (&e.1.x - &e.0.x, &e.1.y - &e.0.y);
        // Order split points along the edge by the (monotone) dot parameter.
        let mut keyed: Vec<(RBig, ExactPoint2)> = pts
            .into_iter()
            .map(|p| ((&p.x - &e.0.x) * &d.0 + (&p.y - &e.0.y) * &d.1, p))
            .collect();
        keyed.sort();
        keyed.dedup();
        let mut prev = e.0.clone();
        for (_, p) in keyed {
            if let Some(sub) = Sub::new(prev.clone(), p.clone()) {
                set.insert(sub);
            }
            prev = p;
        }
        if let Some(sub) = Sub::new(prev, e.1.clone()) {
            set.insert(sub);
        }
    }
    set.into_iter().collect()
}

// ─────────────────────────── classification ─────────────────────────────

/// Exact even-odd (parity) point-in-polygon over a loop-edge set, with the
/// standard half-open crossing rule (`a.y > p.y) != (b.y > p.y`) so rays
/// through vertices count consistently. Division-free: the crossing-side
/// comparison multiplies through by `(b.y − a.y)` with sign correction.
/// Overlay callers only pass points strictly interior to arrangement
/// cells, which are never ON an edge — so no boundary case arises there.
/// (`pub(crate)` since PR-YR27: the Stage-6 finite-extent containment
/// tie-break reuses it AFTER its own exact on-boundary rejection, so the
/// no-boundary precondition holds for that caller too.)
pub(crate) fn point_in_even_odd(p: &ExactPoint2, edges: &[(ExactPoint2, ExactPoint2)]) -> bool {
    let mut inside = false;
    for (a, b) in edges {
        if (a.y > p.y) != (b.y > p.y) {
            // (x_int − p.x) · (b.y − a.y), exact.
            let num = (&a.x - &p.x) * (&b.y - &a.y) + (&p.y - &a.y) * (&b.x - &a.x);
            let crosses_right = if b.y > a.y {
                num > RBig::ZERO
            } else {
                num < RBig::ZERO
            };
            if crosses_right {
                inside = !inside;
            }
        }
    }
    inside
}

// ─────────────────────────── triangulation ──────────────────────────────

/// Exact ear-clip of a simple CCW ring (cells are convex, possibly with
/// collinear runs on their vertical sides; the containment check keeps the
/// routine valid for any simple ring). Only strictly-positive-area ears are
/// clipped, so every emitted triangle is CCW with exact area > 0, and the
/// ears partition the ring exactly (area is conserved in rational
/// arithmetic).
///
/// (PR-YR27 Finding-8 hygiene: PR-YR26 made this `pub(crate)` claiming the
/// Stage-0 wiring would use it for subdivided-boundary re-triangulation,
/// but Stage 0 grew its own verified apex-fan instead —
/// `stage0::triangulate_ring`, whose docs explain why an ear-clip's long
/// diagonals are UNSAFE across the femto-crooked split chains. The
/// visibility is reverted to private; this routine is overlay-internal.)
pub(crate) fn ear_clip(ring: &[ExactPoint2]) -> Result<Vec<[usize; 3]>, CoplanarOverlayError> {
    let mut idx: Vec<usize> = (0..ring.len()).collect();
    let mut out = Vec::with_capacity(ring.len() - 2);
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let p = idx[(k + m - 1) % m];
            let c = idx[k];
            let nx = idx[(k + 1) % m];
            if cross_r(&ring[p], &ring[c], &ring[nx]) <= RBig::ZERO {
                continue; // reflex or collinear corner — not an ear
            }
            // No other remaining vertex inside-or-ON the closed ear (the
            // ON case would create a T-junction on the clipped diagonal).
            let blocked = idx.iter().any(|&q| {
                q != p
                    && q != c
                    && q != nx
                    && in_or_on_ccw_tri(&ring[p], &ring[c], &ring[nx], &ring[q])
            });
            if blocked {
                continue;
            }
            out.push([p, c, nx]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            return Err(CoplanarOverlayError::TriangulationFailed(
                "no clippable ear in cell ring",
            ));
        }
    }
    if cross_r(&ring[idx[0]], &ring[idx[1]], &ring[idx[2]]) <= RBig::ZERO {
        return Err(CoplanarOverlayError::TriangulationFailed(
            "final cell triangle has non-positive area",
        ));
    }
    out.push([idx[0], idx[1], idx[2]]);
    Ok(out)
}

/// `q` inside or on the closed CCW triangle (a, b, c), exact.
fn in_or_on_ccw_tri(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2, q: &ExactPoint2) -> bool {
    cross_r(a, b, q) >= RBig::ZERO
        && cross_r(b, c, q) >= RBig::ZERO
        && cross_r(c, a, q) >= RBig::ZERO
}

/// Get-or-insert an exact vertex in the shared pool. Insertion order is
/// deterministic (slab sweep order), so indices are reproducible.
fn intern(
    pool: &mut BTreeMap<ExactPoint2, u32>,
    verts: &mut Vec<ExactPoint2>,
    p: &ExactPoint2,
) -> u32 {
    if let Some(i) = pool.get(p) {
        return *i;
    }
    let i = u32::try_from(verts.len()).expect("vertex pool exceeds u32");
    pool.insert(p.clone(), i);
    verts.push(p.clone());
    i
}

#[cfg(test)]
mod sliver_gate_tests {
    use super::{rounded_tri_disposition, RoundedTri};
    use cad_primitives::Point2;

    #[test]
    fn positive_triangle_kept() {
        let d = rounded_tri_disposition(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        );
        assert_eq!(d, RoundedTri::Positive);
    }

    #[test]
    fn coincident_pair_needle_dropped() {
        // Two bit-identical verts (the R0015/46/81/98 signature: distinct exact
        // points that round to the same f64) — a zero-extent needle, benign.
        let p = Point2::new(2.613513332e-5, -1.588503209e-4);
        let q = Point2::new(2.613513332e-5, -1.513449247e-4);
        assert_eq!(
            rounded_tri_disposition(q, p, p),
            RoundedTri::CoincidentNeedle
        );
        assert_eq!(
            rounded_tri_disposition(p, q, p),
            RoundedTri::CoincidentNeedle
        );
        assert_eq!(
            rounded_tri_disposition(p, p, q),
            RoundedTri::CoincidentNeedle
        );
    }

    #[test]
    fn collinear_distinct_sliver_rejected() {
        // Three DISTINCT f64 verts rounded collinear — a real sliver, stays loud.
        let d = rounded_tri_disposition(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(2.0, 2.0),
        );
        assert_eq!(d, RoundedTri::CollinearSliver);
    }
}

#[cfg(test)]
mod b7_validity_gate_tests {
    //! Spec `m8_overlay_fused_emission_collapse.md` §4 B7 — the fused-emission
    //! repair's validity gate ([#51] Hoppe link/fold condition, in EXACT
    //! arithmetic) is load-bearing.
    //!
    //! The Adversary found that the "drop the validity gate (accept every
    //! candidate)" mutation is an EQUIVALENT mutant on every constructible
    //! public-API input: on all such inputs a flip-inducing collapse is never
    //! the only escape, so accepting it changes no observable result. Spec §6
    //! anticipated this and mandated a direct internal unit on the repair
    //! routine — this is that test.
    //!
    //! Hand-built triangle soup: a sub-`TAU_MODEL` sliver whose ONLY eligible
    //! collapse edge (AB) is straddled by two real triangles, so remapping the
    //! edge in EITHER survivor direction (B→A or A→B) exactly flips a real
    //! neighbour. The sliver's other two edges are real-scale (supra-`TAU_MODEL`,
    //! ineligible), so with the gate intact NOTHING can be committed and the
    //! honest `RoundingCollapse` wall stands with an EMPTY fusion record.
    //! Without the gate the repair commits the flipping AB collapse (leaking a
    //! `fused` entry and minting an exactly-flipped triangle) before stalling on
    //! the collapse it created — hence `fused.is_empty()` is the mutation-killer.
    use super::{
        fused_emission_repair, rat, ClassifiedOverlay, CoplanarOverlayError, ExactPoint2,
        RegionClass,
    };
    use cad_primitives::Point2;
    use dashu::rational::RBig;
    use std::collections::{BTreeMap, BTreeSet};

    fn ev(x: f64, y: RBig) -> ExactPoint2 {
        ExactPoint2 {
            x: rat(x).expect("finite"),
            y,
        }
    }

    /// B7: the exact validity gate rejects the sole eligible (sub-`TAU_MODEL`)
    /// collapse because it would flip a real triangle in either survivor
    /// direction; the repair returns the loud wall with nothing fused.
    #[test]
    fn b7_flip_inducing_collapse_is_rejected() {
        // A tiny rational far below the smallest f64 subnormal (2⁻¹⁰⁷⁴): its
        // rounded image is 0.0, so R stays f64-collinear with A and B while the
        // EXACT sliver area (1e-8 · d) is strictly positive.
        let d = RBig::from(1) / RBig::from(10u64).pow(400);

        // Verts (exact | rounded-f64):
        //   A = (0, 0)          idx 0   — left of the separating line x = 5e-9
        //   B = (1e-8, 0)       idx 1   — right of it; AB = 1e-8 < TAU_MODEL(1e-7)
        //   R = (1, d→0)        idx 2   — real-scale; AR, BR ≈ 1 ≥ TAU_MODEL
        //   P = (5e-9, -1)      idx 3   — on the separating line
        //   Q = (5e-9,  1)      idx 4   — on the separating line
        let exact_verts = vec![
            ev(0.0, rat(0.0).unwrap()),
            ev(1e-8, rat(0.0).unwrap()),
            ev(1.0, d),
            ev(5e-9, rat(-1.0).unwrap()),
            ev(5e-9, rat(1.0).unwrap()),
        ];
        let verts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1e-8, 0.0),
            Point2::new(1.0, 0.0), // R.y = d rounds to 0.0 → f64-collinear sliver
            Point2::new(5e-9, -1.0),
            Point2::new(5e-9, 1.0),
        ];

        // Triangles, all CCW-positive in EXACT coords (the repair's precondition):
        //   [0,1,2] (A,B,R) — the sliver (exact area 1e-8·d, rounds collinear).
        //   [1,4,3] (B,Q,P) real — flips to negative under B→A (idx1→idx0).
        //   [0,3,4] (A,P,Q) real — flips to negative under A→B (idx0→idx1).
        let mut overlay = ClassifiedOverlay {
            verts,
            exact_verts,
            tris: vec![[0, 1, 2], [1, 4, 3], [0, 3, 4]],
            class: vec![RegionClass::Overlap, RegionClass::AOnly, RegionClass::BOnly],
            poly_a: vec![0, 0, 0],
            poly_b: vec![0, 0, 0],
            fused: BTreeMap::new(),
        };

        // No input-loop verts → survivor is the min index (A over B); the gate
        // must still reject. `all_edges` is only read by the env-gated probe.
        let res = fused_emission_repair(&mut overlay, &BTreeSet::new(), &[]);

        assert!(
            matches!(res, Err(CoplanarOverlayError::RoundingCollapse { .. })),
            "the flip-inducing sole eligible collapse must leave the honest wall, got {res:?}"
        );
        // Mutation-killer: with the gate intact NOTHING is committed. A leaked
        // fusion entry means the gate accepted a flip (the always-accept mutant
        // commits AB before stalling on the sliver it minted).
        assert!(
            overlay.fused.is_empty(),
            "validity gate leaked a fusion — a flip-inducing collapse was accepted ({:?})",
            overlay.fused
        );
    }
}

#[cfg(test)]
mod event_column_merge_tests {
    //! #166 (deviations N49) — near-coincident sweep-event columns.
    //!
    //! When two GENUINELY-DISTINCT input corners project to nearly-equal sweep
    //! x (their 3D separation is orthogonal to the sweep direction, so `p·e₁`
    //! collapses them to within the coordinate-resolution floor), the exact
    //! plane-sweep opens TWO event columns a sub-`TAU_MODEL·(1+scale)` gap
    //! apart. Any crossing edge of the OTHER side is then lifted at BOTH
    //! columns, minting two arrangement vertices a like distance apart — a
    //! render-collapse "twin" that is exact-distinct (survives the f64 interner)
    //! yet below model resolution (this is the R0012/R0098 signature).
    //!
    //! This is a documented **pending RED oracle**, not yet green. N48's scoped
    //! fix (snap input x pre-`split_all`) is REFUTED (N49): the input-polygon
    //! corners are boundary-shared with the rest of the solid mesh — adjacent
    //! non-coplanar faces reuse those exact vertices — so moving ANY corner
    //! opens a watertightness seam (proven by
    //! `stage0::nary::nary_tessellated_group_stage0_meshes`, which a global
    //! x-snap tears: mesh_b → 15 boundary edges). The corrected fix must weld
    //! the INTERIOR twin lift-points only (never a boundary corner) and be
    //! certified against the R0091 green-but-wrong hazard — its own increment.
    //! Un-ignore this test when that fix lands.
    use super::{coplanar_overlay_multi, PolygonWithHoles};
    use cad_primitives::Point2;

    fn quad(v: [(f64, f64); 4]) -> PolygonWithHoles {
        PolygonWithHoles {
            outer: v.iter().map(|&(x, y)| Point2::new(x, y)).collect(),
            holes: vec![],
        }
    }

    /// A single overlay whose two B-corners share a sweep column to within
    /// ~1e-6 (real 3D separation ⟂ to the sweep direction). A's slanted top
    /// edge crosses the thin slab; the twin is the pair of near-duplicate lift
    /// vertices on that edge, one per column. Post-fix the columns unify and
    /// the edge is lifted once, so the minimum pairwise vertex gap is the real
    /// feature scale (tens of units), not ~1e-6.
    #[test]
    #[ignore = "#166 pending: input-column snap refuted (N49, boundary-shared \
                corners); needs interior-only twin weld — un-ignore when it lands"]
    fn near_coincident_event_columns_do_not_mint_twin() {
        // A: quad with a non-vertical top edge y = 60 − 0.2·x spanning x∈[0,100].
        let a = quad([(0.0, 0.0), (100.0, 0.0), (100.0, 40.0), (0.0, 60.0)]);
        // B ⊂ A: its left side runs from (50.0, 10) up to (50.000001, 30) — two
        // distinct corners 20 units apart in y whose x differ by only 1e-6.
        let b = quad([(50.0, 10.0), (80.0, 10.0), (80.0, 30.0), (50.000001, 30.0)]);

        let overlay = coplanar_overlay_multi(std::slice::from_ref(&a), std::slice::from_ref(&b))
            .expect("overlay must succeed");

        // No two DISTINCT output vertices may lie within the tolerance floor:
        // the twin (~1e-6 apart) violates this; the real geometry here is
        // separated by whole units. 1e-3 cleanly straddles the two regimes.
        let mut min_gap = f64::INFINITY;
        for (i, p) in overlay.verts.iter().enumerate() {
            for q in &overlay.verts[i + 1..] {
                let d = ((p.x() - q.x()).powi(2) + (p.y() - q.y()).powi(2)).sqrt();
                if d > 0.0 {
                    min_gap = min_gap.min(d);
                }
            }
        }
        assert!(
            min_gap > 1e-3,
            "near-coincident event columns minted a sub-tolerance twin: \
             min pairwise vertex gap = {min_gap:e} (expected ≫ 1e-3)"
        );
    }
}

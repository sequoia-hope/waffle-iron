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
/// rounding-boundary design.
pub fn coplanar_overlay(
    a: &PolygonWithHoles,
    b: &PolygonWithHoles,
) -> Result<ClassifiedOverlay, CoplanarOverlayError> {
    // ── 0. Validate + lift to exact rationals. ──────────────────────────
    let loops_a = exact_loops(a)?;
    let loops_b = exact_loops(b)?;
    let edges_a = loop_edges(&loops_a);
    let edges_b = loop_edges(&loops_b);

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
            let centroid = ExactPoint2 {
                x: xm.clone(),
                y: (ylo_m + yhi_m) / &two,
            };
            let in_a = point_in_even_odd(&centroid, &edges_a);
            let in_b = point_in_even_odd(&centroid, &edges_b);
            let cls = match (in_a, in_b) {
                (true, true) => RegionClass::Overlap,
                (true, false) => RegionClass::AOnly,
                (false, true) => RegionClass::BOnly,
                // Outside the union: dropped; guarded by the exact coverage
                // identity below.
                (false, false) => continue,
            };

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

    let overlay = ClassifiedOverlay {
        verts,
        exact_verts,
        tris,
        class,
    };

    // ── 5. Exact coverage post-conditions (P9/P10 — loud). ──────────────
    let area_a = overlay.area_exact(RegionClass::AOnly) + overlay.area_exact(RegionClass::Overlap);
    if area_a != input_area(&loops_a) {
        return Err(CoplanarOverlayError::CoverageMismatch { side: 'A' });
    }
    let area_b = overlay.area_exact(RegionClass::BOnly) + overlay.area_exact(RegionClass::Overlap);
    if area_b != input_area(&loops_b) {
        return Err(CoplanarOverlayError::CoverageMismatch { side: 'B' });
    }

    // ── 6. Sliver-collapse gate: every triangle (exactly CCW-positive by
    // construction) must stay positively oriented in the ROUNDED f64
    // coordinates. A collapse is rejected LOUDLY, never dropped silently.
    for tri in &overlay.tris {
        let a2 = overlay.verts[tri[0] as usize];
        let b2 = overlay.verts[tri[1] as usize];
        let c2 = overlay.verts[tri[2] as usize];
        let area2 = (b2.x() - a2.x()) * (c2.y() - a2.y()) - (b2.y() - a2.y()) * (c2.x() - a2.x());
        // `<=` would miss NaN (cannot occur — verts are finite — but be
        // total anyway): keep only strictly-positive areas.
        if area2.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(CoplanarOverlayError::RoundingCollapse { tri: *tri });
        }
    }

    Ok(overlay)
}

// ───────────────────────────── exact helpers ────────────────────────────

/// `cross(b−a, c−a)` — twice the signed area of triangle (a, b, c), exact.
/// (`pub(crate)` since PR-YR26 for the Stage-0 ring-orientation check.)
pub(crate) fn cross_r(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> RBig {
    (&b.x - &a.x) * (&c.y - &a.y) - (&b.y - &a.y) * (&c.x - &a.x)
}

/// Exact f64 → RBig; fails on NaN / infinity.
fn rat(x: f64) -> Result<RBig, CoplanarOverlayError> {
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
/// Callers only pass points strictly interior to arrangement cells, which
/// are never ON an edge — so no boundary case arises.
fn point_in_even_odd(p: &ExactPoint2, edges: &[(ExactPoint2, ExactPoint2)]) -> bool {
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
/// `pub(crate)` since PR-YR26: the Stage-0 wiring re-triangulates faces
/// whose boundary edges were subdivided by overlay points (the §4.5.5
/// "identical sampling points on their boundaries" propagation) — those
/// rings carry collinear boundary runs a fan cannot handle, exactly the
/// case this routine covers.
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

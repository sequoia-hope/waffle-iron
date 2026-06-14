//! Planar profile type for the primitive constructors (PR-KV2).
//!
//! A [`Profile`] is a closed planar region: an outer polygon plus zero or
//! more hole polygons, expressed in **2D plane coordinates** over an
//! explicit plane frame (origin + two basis vectors). The eventual consumer
//! (feature-engine sketch profiles) supplies exactly this shape: a sketch
//! plane and closed polylines solved in that plane.
//!
//! ## Planarity by construction
//!
//! Profile vertices are `Point2` in the plane's `(u, v)` coordinates; the
//! 3D embedding is `origin + x·u + y·v` ([`Profile::embed`]). A non-planar
//! profile is therefore **unrepresentable** — the constructor mandate
//! "reject non-planar profile input" is discharged at the type level rather
//! than by a tolerance test. (A caller holding 3D points must first project
//! them onto a plane; deciding *whether* loose 3D points are coplanar is an
//! exact-predicate question that belongs to the cherchi-rs layer, not here.)
//!
//! ## The simplicity-validation decision (documented per the KV2 mandate)
//!
//! Simple-polygon validation (no self-intersection) is implemented as an
//! **exact arithmetic check via `dashu` rationals**, not deferred and not
//! approximated:
//!
//! - **Why not f64 with epsilons:** a near-tangential crossing decided
//!   wrongly produces a topologically "valid" but geometrically garbage
//!   solid — the exact silent-wrong failure mode this rewrite exists to
//!   eliminate.
//! - **Why not a "caller guarantees simple" contract:** violations would
//!   surface (if at all) as downstream Euler/manifoldness errors far from
//!   the cause, or worse, not at all (a self-intersecting profile extrudes
//!   to a self-intersecting but Euler-clean solid).
//! - **Why dashu is acceptable here:** every `f64` is exactly representable
//!   as a rational, so `f64 → RBig` is lossless and sign evaluations of the
//!   2×2 orientation determinant are exact — this is a *decision procedure*,
//!   not an approximation. `dashu` is pure Rust and wasm-safe, is already in
//!   the workspace tree (cherchi-rs exact cascade), and kernel-v2's crate
//!   dep rule ("cad-primitives, yang-rs only") governs *workspace layering*,
//!   not external pure-Rust crates. The check is O(n²) in loop edges, which
//!   is irrelevant at sketch-profile scale.
//!
//! Checks performed by [`Profile::new`] (all loud, all typed):
//!
//! 1. all coordinates / frame components finite (`ProfileNotFinite`);
//! 2. `u × v` nonzero (`ProfileDegenerateBasis`);
//! 3. every loop has ≥ 3 vertices (`ProfileTooFewVertices`);
//! 4. no consecutively repeated vertex, including last == first
//!    (`ProfileRepeatedVertex`);
//! 5. every loop is a simple polygon — exact segment-pair crossing /
//!    touching / collinear-overlap test (`ProfileNotSimple`). A loop that
//!    passes this check provably encloses nonzero area, so there is no
//!    separate zero-area error: degenerate collinear loops fail here;
//! 6. distinct loops are pairwise disjoint — no crossing or touching
//!    (`ProfileLoopsIntersect`);
//! 7. every hole lies strictly inside the outer loop
//!    (`ProfileHoleNotInsideOuter`) and no hole lies inside another hole
//!    (`ProfileHolesNested`) — exact point-in-polygon on a witness vertex,
//!    sound because disjointness (6) is already established.
//!
//! ## Orientation normalization
//!
//! Loop input order is arbitrary; `Profile::new` computes each loop's exact
//! signed area (shoelace, rational) and stores **every loop CCW** in `(u,v)`
//! coordinates (positive area). Constructors choose winding per use site
//! (e.g. extrude reverses when the sweep direction opposes `u × v`), so a
//! normalized storage form keeps that logic in one place.

use crate::arena::UnitVector3;
use crate::error::KernelV2Error;
use cad_primitives::{Point2, Point3, Vector3};

/// `|u × v|²` floor below which the profile frame is rejected as
/// degenerate. Like `geom::NEWELL_MIN_SQ_NORM` this is effectively an
/// exact-zero test: it rejects frames whose basis vectors are exactly (or
/// catastrophically) parallel while accepting any genuinely plane-spanning
/// basis; a *skewed but spanning* basis is legal (the embedding is affine,
/// so a simple 2D polygon stays simple).
pub const BASIS_MIN_SQ_CROSS_NORM: f64 = 1e-60;

/// `|·|` tolerance on `|u| − 1`, `|v| − 1`, and `u · v` for a circle
/// profile's frame ([`Profile::circle`]). A circle in plane coordinates
/// embeds to a true 3D circle of the same radius **only** under an isometric
/// (orthonormal) frame; a skewed/scaled frame yields an ellipse — out of the
/// KV5a vocabulary, so it is rejected with
/// [`KernelV2Error::ProfileCircleFrameNotOrthonormal`] rather than silently
/// reshaped. The tolerance absorbs only unit-vector normalization rounding
/// (sketch planes supply normalized bases); it is far below any geometric
/// feature scale.
pub const CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE: f64 = 1e-9;

/// One boundary edge of an [`ProfileRegion::ArcPolygon`] loop, in `(u, v)`
/// plane coordinates. Edges chain head-to-tail (`edge[i].b == edge[i+1].a`,
/// the last closing onto the first). PR-KV12 Tier 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfileEdge {
    /// Straight segment from `a` to `b`.
    Line {
        /// Start point (plane coordinates).
        a: Point2,
        /// End point (plane coordinates).
        b: Point2,
    },
    /// Circular arc from `a` to `b` about `center` of `radius`. The kernel
    /// assembler derives the traversal sense from the geometry (the unique
    /// MINOR arc, sweep `∈ (0, π)`); `ccw` records the intended sense for the
    /// exact validator (E3) and is advisory at the E1/E2 assembler.
    Arc {
        /// Start point (on the circle).
        a: Point2,
        /// End point (on the circle).
        b: Point2,
        /// Circle center (plane coordinates).
        center: Point2,
        /// Circle radius (> 0).
        radius: f64,
        /// Intended CCW sense (around `+u × v`) of `a → b`.
        ccw: bool,
    },
}

/// The region a profile encloses, in `(u, v)` plane coordinates.
///
/// PR-KV2 had polygons-with-holes only; PR-KV5a adds the full-circle disk
/// (the assay corpus' 137 curved cases are all full circles → extruded
/// cylinders); PR-KV12 Tier 2 adds [`ProfileRegion::ArcPolygon`] (a loop of
/// mixed line + circular-arc edges → exact cylinder side patches). The
/// representation is deliberately an enum so they extend it without
/// reshaping `Profile`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ProfileRegion {
    /// Polygon with holes; loops stored CCW (see module docs).
    Polygon {
        /// Outer polygon, CCW in `(u, v)` coordinates.
        outer: Vec<Point2>,
        /// Hole polygons, each CCW in `(u, v)` coordinates, each strictly
        /// inside `outer`, pairwise disjoint and non-nested.
        holes: Vec<Vec<Point2>>,
    },
    /// Full-circle disk of `radius` about `center` (plane coordinates).
    /// Simplicity is trivial: any `radius > 0` circle is simple.
    Circle {
        /// Center in `(u, v)` plane coordinates.
        center: Point2,
        /// Radius (meters, > 0).
        radius: f64,
    },
    /// A closed loop of mixed [`ProfileEdge::Line`] / [`ProfileEdge::Arc`]
    /// edges, with zero or more hole loops (PR-KV12 Tier 2). Extrudes to a
    /// B-Rep with exact cylinder side patches for arc edges. Construction via
    /// [`Profile::arc_polygon`]; exact self-intersection validation is
    /// PR-KV12 increment E3.
    ArcPolygon {
        /// Outer boundary loop, edges chained head-to-tail.
        outer: Vec<ProfileEdge>,
        /// Hole loops (each chained head-to-tail).
        holes: Vec<Vec<ProfileEdge>>,
    },
}

/// A validated planar profile: plane frame + region.
///
/// Construction via [`Profile::new`] (polygon) or [`Profile::circle`] is the
/// only way to obtain one, so a `Profile` value **is** the evidence that all
/// validation in the module docs has passed.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    origin: Point3,
    u: Vector3,
    v: Vector3,
    region: ProfileRegion,
}

impl Profile {
    /// Validate and build a profile. See the module docs for the checks and
    /// the error contract. `outer` and each member of `holes` are closed
    /// polylines given WITHOUT the repeated closing vertex.
    pub fn new(
        origin: Point3,
        u: Vector3,
        v: Vector3,
        mut outer: Vec<Point2>,
        mut holes: Vec<Vec<Point2>>,
    ) -> Result<Self, KernelV2Error> {
        // 1. Frame finiteness + non-degeneracy. Finiteness FIRST: it is the
        //    precondition that makes every later f64 → RBig conversion total.
        let frame = [
            origin.x(),
            origin.y(),
            origin.z(),
            u.x(),
            u.y(),
            u.z(),
            v.x(),
            v.y(),
            v.z(),
        ];
        if frame.iter().any(|c| !c.is_finite()) {
            return Err(KernelV2Error::ProfileNotFinite);
        }
        let c = cross(u, v);
        let sq = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        // `sq` is finite (frame finiteness pre-checked), so a plain comparison
        // is total here.
        if sq < BASIS_MIN_SQ_CROSS_NORM {
            return Err(KernelV2Error::ProfileDegenerateBasis);
        }

        // 2–5. Per-loop validation + CCW normalization (loop_index 0 = outer,
        //      k + 1 = hole k).
        validate_and_normalize_loop(&mut outer, 0)?;
        for (k, hole) in holes.iter_mut().enumerate() {
            validate_and_normalize_loop(hole, k + 1)?;
        }

        // 6. Pairwise loop disjointness (exact; touching counts).
        let all: Vec<&[Point2]> = std::iter::once(outer.as_slice())
            .chain(holes.iter().map(|h| h.as_slice()))
            .collect();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                if loops_touch(all[i], all[j]) {
                    return Err(KernelV2Error::ProfileLoopsIntersect {
                        loop_a: i,
                        loop_b: j,
                    });
                }
            }
        }

        // 7. Hole containment (witness vertex; sound after disjointness).
        for (k, hole) in holes.iter().enumerate() {
            if !exact::point_strictly_inside(hole[0], &outer) {
                return Err(KernelV2Error::ProfileHoleNotInsideOuter { hole_index: k });
            }
        }
        for i in 0..holes.len() {
            for j in 0..holes.len() {
                if i != j && exact::point_strictly_inside(holes[j][0], &holes[i]) {
                    return Err(KernelV2Error::ProfileHolesNested {
                        outer_hole: i,
                        inner_hole: j,
                    });
                }
            }
        }

        Ok(Self {
            origin,
            u,
            v,
            region: ProfileRegion::Polygon { outer, holes },
        })
    }

    /// Validate and build a full-circle profile (PR-KV5a): a disk of
    /// `radius` about `center` in `(u, v)` plane coordinates.
    ///
    /// Checks (all loud, all typed):
    /// 1. all coordinates / frame components finite (`ProfileNotFinite`);
    /// 2. `u × v` nonzero (`ProfileDegenerateBasis`);
    /// 3. the frame is orthonormal within
    ///    [`CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE`]
    ///    (`ProfileCircleFrameNotOrthonormal` — see the constant's docs for
    ///    why a non-isometric frame is rejected rather than reshaped);
    /// 4. `radius` finite and strictly positive
    ///    (`ProfileCircleNonPositiveRadius`).
    pub fn circle(
        origin: Point3,
        u: Vector3,
        v: Vector3,
        center: Point2,
        radius: f64,
    ) -> Result<Self, KernelV2Error> {
        // 1. Finiteness FIRST (precondition for every later comparison).
        let frame = [
            origin.x(),
            origin.y(),
            origin.z(),
            u.x(),
            u.y(),
            u.z(),
            v.x(),
            v.y(),
            v.z(),
        ];
        if frame.iter().any(|c| !c.is_finite())
            || !center.x().is_finite()
            || !center.y().is_finite()
        {
            return Err(KernelV2Error::ProfileNotFinite);
        }
        // 2. Plane-spanning basis (same gate as the polygon form).
        let c = cross(u, v);
        let sq = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        if sq < BASIS_MIN_SQ_CROSS_NORM {
            return Err(KernelV2Error::ProfileDegenerateBasis);
        }
        // 3. Orthonormal frame: the embedding must be an isometry for the
        //    plane-coordinate circle to embed as a true circle (see
        //    CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE).
        let tol = CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE;
        let u_sq = u.x() * u.x() + u.y() * u.y() + u.z() * u.z();
        let v_sq = v.x() * v.x() + v.y() * v.y() + v.z() * v.z();
        let uv = u.x() * v.x() + u.y() * v.y() + u.z() * v.z();
        if (u_sq.sqrt() - 1.0).abs() > tol || (v_sq.sqrt() - 1.0).abs() > tol || uv.abs() > tol {
            return Err(KernelV2Error::ProfileCircleFrameNotOrthonormal);
        }
        // 4. Positive finite radius. (Simplicity is trivial: any r > 0
        //    circle is a simple closed curve.)
        if !radius.is_finite() || radius <= 0.0 {
            return Err(KernelV2Error::ProfileCircleNonPositiveRadius);
        }
        Ok(Self {
            origin,
            u,
            v,
            region: ProfileRegion::Circle { center, radius },
        })
    }

    /// Validate and build a mixed line/arc profile (PR-KV12 Tier 2): a
    /// closed loop of [`ProfileEdge`]s plus zero or more hole loops.
    ///
    /// E1 validation (loud, typed) — frame finiteness / non-degeneracy as for
    /// [`Profile::new`], plus per loop:
    /// 1. ≥ 2 edges (`ProfileTooFewVertices` with the loop index);
    /// 2. head-to-tail chain closure (`ProfileArcEdgeInvalid`);
    /// 3. every endpoint finite; each arc's `radius` finite/positive, its
    ///    endpoints on the circle within the import band, and its sweep a
    ///    MINOR arc `∈ (0, π)` (`ProfileArcEdgeInvalid`).
    ///
    /// Exact self-intersection / hole-containment validation (the analog of
    /// `Profile::new` steps 5–7, extended to arc edges) is PR-KV12 increment
    /// E3 — a value built here is NOT yet evidence of simplicity, only of
    /// well-formed edges. The E1/E2 assembler is exercised on profiles that
    /// are simple by construction (direct kernel tests).
    pub fn arc_polygon(
        origin: Point3,
        u: Vector3,
        v: Vector3,
        outer: Vec<ProfileEdge>,
        holes: Vec<Vec<ProfileEdge>>,
    ) -> Result<Self, KernelV2Error> {
        // Frame finiteness + non-degeneracy (same gate as `new` / `circle`).
        let frame = [
            origin.x(),
            origin.y(),
            origin.z(),
            u.x(),
            u.y(),
            u.z(),
            v.x(),
            v.y(),
            v.z(),
        ];
        if frame.iter().any(|c| !c.is_finite()) {
            return Err(KernelV2Error::ProfileNotFinite);
        }
        let c = cross(u, v);
        let sq = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        if sq < BASIS_MIN_SQ_CROSS_NORM {
            return Err(KernelV2Error::ProfileDegenerateBasis);
        }

        validate_arc_loop(&outer, 0)?;
        for (k, hole) in holes.iter().enumerate() {
            validate_arc_loop(hole, k + 1)?;
        }

        // Distinct loops must be pairwise non-touching (exact; the arc analog
        // of `Profile::new`'s `loops_touch`). Strict hole-inside-outer
        // containment with an arc-aware point-in-region is PR-KV12 increment
        // E4 (where holes are actually assembled); the extrude path rejects
        // `ArcPolygon` holes until then, so no unchecked hole reaches geometry.
        let all: Vec<&[ProfileEdge]> = std::iter::once(outer.as_slice())
            .chain(holes.iter().map(|h| h.as_slice()))
            .collect();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                if arc_loops_touch(all[i], all[j]) {
                    return Err(KernelV2Error::ProfileLoopsIntersect {
                        loop_a: i,
                        loop_b: j,
                    });
                }
            }
        }

        // Containment (E4b): every hole strictly inside the outer, and no hole
        // nested in another — exact arc-aware ray-cast (sound after
        // disjointness). An indeterminate witness (all rays degenerate) is
        // treated as "not confirmed inside" and rejected loudly, never
        // silently accepted.
        for (k, hole) in holes.iter().enumerate() {
            if loop_strictly_inside(hole, &outer) != Some(true) {
                return Err(KernelV2Error::ProfileHoleNotInsideOuter { hole_index: k });
            }
        }
        for i in 0..holes.len() {
            for j in 0..holes.len() {
                if i != j && loop_strictly_inside(&holes[j], &holes[i]) == Some(true) {
                    return Err(KernelV2Error::ProfileHolesNested {
                        outer_hole: i,
                        inner_hole: j,
                    });
                }
            }
        }

        Ok(Self {
            origin,
            u,
            v,
            region: ProfileRegion::ArcPolygon { outer, holes },
        })
    }

    /// Plane origin.
    pub fn origin(&self) -> Point3 {
        self.origin
    }

    /// First plane basis vector.
    pub fn u(&self) -> Vector3 {
        self.u
    }

    /// Second plane basis vector.
    pub fn v(&self) -> Vector3 {
        self.v
    }

    /// The validated region (polygon-with-holes or full-circle disk).
    pub fn region(&self) -> &ProfileRegion {
        &self.region
    }

    /// Embed a plane-coordinate point into 3D: `origin + x·u + y·v`.
    pub fn embed(&self, p: Point2) -> Point3 {
        Point3::new(
            self.origin.x() + p.x() * self.u.x() + p.y() * self.v.x(),
            self.origin.y() + p.x() * self.u.y() + p.y() * self.v.y(),
            self.origin.z() + p.x() * self.u.z() + p.y() * self.v.z(),
        )
    }

    /// Unit plane normal `normalize(u × v)`. Total for a validated profile
    /// (the constructor rejected degenerate bases).
    pub fn unit_normal(&self) -> UnitVector3 {
        let c = cross(self.u, self.v);
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        UnitVector3 {
            x: c[0] / len,
            y: c[1] / len,
            z: c[2] / len,
        }
    }
}

/// Cross product of the frame's basis vectors (component math local to this
/// crate; cad-primitives is types-only).
pub(crate) fn cross(a: Vector3, b: Vector3) -> [f64; 3] {
    [
        a.y() * b.z() - a.z() * b.y(),
        a.z() * b.x() - a.x() * b.z(),
        a.x() * b.y() - a.y() * b.x(),
    ]
}

/// Per-loop validation (steps 2–5 of the module-docs checklist), then
/// orientation normalization to CCW (positive exact shoelace area).
fn validate_and_normalize_loop(pts: &mut [Point2], loop_index: usize) -> Result<(), KernelV2Error> {
    if pts.len() < 3 {
        return Err(KernelV2Error::ProfileTooFewVertices { loop_index });
    }
    if pts.iter().any(|p| !p.x().is_finite() || !p.y().is_finite()) {
        return Err(KernelV2Error::ProfileNotFinite);
    }
    // Consecutive duplicates, including the implicit closing edge
    // (last == first). Exact f64 equality.
    let n = pts.len();
    for i in 0..n {
        if pts[i] == pts[(i + 1) % n] {
            return Err(KernelV2Error::ProfileRepeatedVertex { loop_index });
        }
    }
    // Spikes: a vertex whose two incident edges are collinear AND double
    // back (adjacent-segment overlap). Collinear straight-through vertices
    // (dot ≤ 0) are redundant but legal.
    for i in 0..n {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        if exact::doubles_back(a, b, c) {
            return Err(KernelV2Error::ProfileNotSimple { loop_index });
        }
    }
    // Non-adjacent segment pairs must be fully disjoint (crossing, touching,
    // and collinear overlap all reject) — exact.
    for i in 0..n {
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if adjacent {
                continue; // handled by the spike test above
            }
            if exact::closed_segments_intersect(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n])
            {
                return Err(KernelV2Error::ProfileNotSimple { loop_index });
            }
        }
    }
    // Orientation: exact shoelace sign. A simple polygon (established above)
    // encloses nonzero area; Equal is therefore unreachable, kept as a
    // defensive loud arm rather than a debug_assert.
    match exact::signed_area_sign(pts) {
        std::cmp::Ordering::Greater => Ok(()),
        std::cmp::Ordering::Less => {
            pts.reverse();
            Ok(())
        }
        std::cmp::Ordering::Equal => Err(KernelV2Error::ProfileNotSimple { loop_index }),
    }
}

impl ProfileEdge {
    /// The edge's start point (plane coordinates).
    pub fn start(&self) -> Point2 {
        match self {
            ProfileEdge::Line { a, .. } | ProfileEdge::Arc { a, .. } => *a,
        }
    }

    /// The edge's end point (plane coordinates).
    pub fn end(&self) -> Point2 {
        match self {
            ProfileEdge::Line { b, .. } | ProfileEdge::Arc { b, .. } => *b,
        }
    }
}

/// Well-formedness + EXACT simplicity of one [`ProfileEdge`] loop (PR-KV12
/// Tier 2). Pass 1 (E1): chain closure + per-arc validity. Pass 2 (E3): the
/// loop is a simple closed curve — no two edges share a point other than the
/// junction vertices consecutive edges legitimately share. `loop_index`
/// matches the `Profile::new` convention (0 = outer, k + 1 = hole k).
fn validate_arc_loop(edges: &[ProfileEdge], loop_index: usize) -> Result<(), KernelV2Error> {
    if edges.len() < 2 {
        return Err(KernelV2Error::ProfileTooFewVertices { loop_index });
    }
    let n = edges.len();
    for i in 0..n {
        // Head-to-tail chain closure (exact: the caller shares vertices).
        if edges[i].end() != edges[(i + 1) % n].start() {
            return Err(KernelV2Error::ProfileArcEdgeInvalid);
        }
        let (a, b) = (edges[i].start(), edges[i].end());
        if !a.x().is_finite() || !a.y().is_finite() || !b.x().is_finite() || !b.y().is_finite() {
            return Err(KernelV2Error::ProfileArcEdgeInvalid);
        }
        if a == b {
            return Err(KernelV2Error::ProfileArcEdgeInvalid);
        }
        if let ProfileEdge::Arc {
            a,
            b,
            center,
            radius,
            ..
        } = edges[i]
        {
            if !radius.is_finite() || radius <= 0.0 {
                return Err(KernelV2Error::ProfileArcEdgeInvalid);
            }
            // Endpoints on the circle (import band, scale-relative).
            let band = 1e-9 * radius.max(1.0);
            for p in [a, b] {
                let dr = ((p.x() - center.x()).powi(2) + (p.y() - center.y()).powi(2)).sqrt();
                if (dr - radius).abs() > band {
                    return Err(KernelV2Error::ProfileArcEdgeInvalid);
                }
            }
            // Minor arc: sweep angle ∈ (0, π). `atan2(|cross|, dot)` lands in
            // [0, π]; reject the degenerate (≈0) and the half-or-greater
            // (≥ π) ends with a small margin.
            let (da, db) = (
                [a.x() - center.x(), a.y() - center.y()],
                [b.x() - center.x(), b.y() - center.y()],
            );
            let cross2 = (da[0] * db[1] - da[1] * db[0]).abs();
            let dot = da[0] * db[0] + da[1] * db[1];
            let sweep = cross2.atan2(dot);
            const MARGIN: f64 = 1e-9;
            if !(MARGIN..=std::f64::consts::PI - MARGIN).contains(&sweep) {
                return Err(KernelV2Error::ProfileArcEdgeInvalid);
            }
        }
    }

    // A two-edge loop of two straight segments is a zero-area digon (the
    // segments coincide) — degenerate. (A digon with at least one arc, e.g.
    // a vesica lens or a circular sector closed by a chord, encloses area.)
    if n == 2 && edges.iter().all(|e| matches!(e, ProfileEdge::Line { .. })) {
        return Err(KernelV2Error::ProfileNotSimple { loop_index });
    }

    // Pass 2 (E3): exact pairwise simplicity. Consecutive edges legitimately
    // meet at their junction vertex (both junctions for a two-edge loop);
    // any OTHER shared point — a non-junction endpoint landing on an edge, or
    // two edge interiors crossing — makes the boundary non-simple.
    for i in 0..n {
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            let shared = if adjacent {
                shared_endpoints(&edges[i], &edges[j])
            } else {
                Vec::new()
            };
            if edges_meet_illegally(&edges[i], &edges[j], &shared) {
                return Err(KernelV2Error::ProfileNotSimple { loop_index });
            }
        }
    }
    Ok(())
}

/// The points that are endpoints of BOTH edges (exact equality) — the
/// junction(s) consecutive edges share (one for a `k ≥ 3` loop, both for a
/// two-edge loop).
fn shared_endpoints(e1: &ProfileEdge, e2: &ProfileEdge) -> Vec<Point2> {
    let e2v = [e2.start(), e2.end()];
    [e1.start(), e1.end()]
        .into_iter()
        .filter(|v| e2v.contains(v))
        .collect()
}

/// Is point `v` on the closed edge (line segment or minor arc)? Exact.
fn point_on_closed_edge(v: Point2, e: &ProfileEdge) -> bool {
    match *e {
        ProfileEdge::Line { a, b } => {
            exact::orient2d(a, b, v) == std::cmp::Ordering::Equal
                && exact::on_collinear_segment(a, b, v)
        }
        ProfileEdge::Arc {
            a,
            b,
            center,
            radius,
            ..
        } => exact::point_on_closed_arc(v, a, b, center, radius),
    }
}

/// Do the relative interiors of two edges cross? Exact dispatch over the
/// line/arc combinations (PR-KV12 E3 predicates).
fn interiors_cross(e1: &ProfileEdge, e2: &ProfileEdge) -> bool {
    match (*e1, *e2) {
        (ProfileEdge::Line { a: a1, b: b1 }, ProfileEdge::Line { a: a2, b: b2 }) => {
            exact::segments_properly_cross(a1, b1, a2, b2)
        }
        (
            ProfileEdge::Arc {
                a,
                b,
                center,
                radius,
                ..
            },
            ProfileEdge::Line { a: p, b: q },
        )
        | (
            ProfileEdge::Line { a: p, b: q },
            ProfileEdge::Arc {
                a,
                b,
                center,
                radius,
                ..
            },
        ) => exact::arc_segment_interior_cross(a, b, center, radius, p, q),
        (
            ProfileEdge::Arc {
                a: a1,
                b: b1,
                center: c1,
                radius: r1,
                ..
            },
            ProfileEdge::Arc {
                a: a2,
                b: b2,
                center: c2,
                radius: r2,
                ..
            },
        ) => exact::arc_arc_interior_cross(a1, b1, c1, r1, a2, b2, c2, r2),
    }
}

/// Do two edges share a point that is NOT a permitted junction? Either a
/// non-junction endpoint lying on the other closed edge, or the two relative
/// interiors crossing.
fn edges_meet_illegally(e1: &ProfileEdge, e2: &ProfileEdge, shared: &[Point2]) -> bool {
    for v in [e1.start(), e1.end()] {
        if !shared.contains(&v) && point_on_closed_edge(v, e2) {
            return true;
        }
    }
    for v in [e2.start(), e2.end()] {
        if !shared.contains(&v) && point_on_closed_edge(v, e1) {
            return true;
        }
    }
    interiors_cross(e1, e2)
}

/// Do two distinct [`ProfileEdge`] loops touch anywhere? (Exact; edge-pair
/// sweep with no permitted shared points — distinct loops must be fully
/// disjoint.)
fn arc_loops_touch(a: &[ProfileEdge], b: &[ProfileEdge]) -> bool {
    for ea in a {
        for eb in b {
            if edges_meet_illegally(ea, eb, &[]) {
                return true;
            }
        }
    }
    false
}

/// Is point `w` strictly inside the line/arc region bounded by `loop_edges`?
/// Exact +x-ray crossing parity (PR-KV12 E4b). `None` signals a ray
/// degeneracy (a boundary vertex on the ray, or an arc tangent to it) — the
/// caller retries with another witness. Caller guarantees `w` is not on the
/// boundary (loop disjointness is established first).
fn point_in_arc_region(w: Point2, loop_edges: &[ProfileEdge]) -> Option<bool> {
    // Far endpoint strictly beyond all geometry in +x → provably outside.
    let mut max_x = w.x();
    for e in loop_edges {
        max_x = max_x.max(e.start().x()).max(e.end().x());
        if let ProfileEdge::Arc { center, radius, .. } = e {
            max_x = max_x.max(center.x() + radius);
        }
    }
    let far = Point2::new(max_x + 1.0, w.y());

    // Degeneracy: a boundary vertex lying on the ray (same y, to the right).
    for e in loop_edges {
        let v = e.start();
        if v.y() == w.y() && v.x() >= w.x() {
            return None;
        }
    }

    let mut count = 0usize;
    for e in loop_edges {
        match *e {
            ProfileEdge::Line { a, b } => {
                if exact::segments_properly_cross(w, far, a, b) {
                    count += 1;
                }
            }
            ProfileEdge::Arc {
                a,
                b,
                center,
                radius,
                ..
            } => match exact::arc_segment_interior_crossings(a, b, center, radius, w, far) {
                Some(c) => count += c,
                None => return None,
            },
        }
    }
    Some(count % 2 == 1)
}

/// Is the `inner` loop strictly inside the `region` loop? Tries each `inner`
/// vertex as a ray-cast witness until one is non-degenerate (all `inner`
/// points share the same status once disjointness holds). `None` if every
/// witness is degenerate — the caller treats that as "cannot confirm".
fn loop_strictly_inside(inner: &[ProfileEdge], region: &[ProfileEdge]) -> Option<bool> {
    for e in inner {
        if let Some(inside) = point_in_arc_region(e.start(), region) {
            return Some(inside);
        }
    }
    None
}

/// Do two distinct loops intersect or touch anywhere? (Exact; segment-pair
/// sweep — O(n·m), fine at sketch scale.)
fn loops_touch(a: &[Point2], b: &[Point2]) -> bool {
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        for j in 0..nb {
            if exact::closed_segments_intersect(a[i], a[(i + 1) % na], b[j], b[(j + 1) % nb]) {
                return true;
            }
        }
    }
    false
}

/// Exact 2D predicates over `dashu` rationals — see [`crate::exact2d`]
/// (PR-KV3 promoted the predicates that originally lived here to a shared
/// crate-internal module so the tessellation pass uses the identical exact
/// arithmetic). Every finite `f64` converts losslessly to `RBig`, so these
/// are decision procedures (see the module docs' simplicity-validation
/// rationale). All callers guarantee finiteness before calling (checked in
/// `Profile::new` step 1 / loop validation), so the conversions are total.
use crate::exact2d as exact;

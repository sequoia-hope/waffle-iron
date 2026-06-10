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

/// A validated planar profile: plane frame + outer polygon + hole polygons.
///
/// Construction via [`Profile::new`] is the only way to obtain one, so a
/// `Profile` value **is** the evidence that all validation in the module
/// docs has passed. Loops are stored CCW in `(u, v)` coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    origin: Point3,
    u: Vector3,
    v: Vector3,
    /// Outer polygon, CCW in `(u, v)` coordinates.
    outer: Vec<Point2>,
    /// Hole polygons, each CCW in `(u, v)` coordinates, each strictly
    /// inside `outer`, pairwise disjoint and non-nested.
    holes: Vec<Vec<Point2>>,
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
            outer,
            holes,
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

    /// Outer polygon, CCW in `(u, v)` coordinates.
    pub fn outer(&self) -> &[Point2] {
        &self.outer
    }

    /// Hole polygons, each CCW in `(u, v)` coordinates.
    pub fn holes(&self) -> &[Vec<Point2>] {
        &self.holes
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

/// Exact 2D predicates over `dashu` rationals. Every finite `f64` converts
/// losslessly to `RBig`, so these are decision procedures (see the module
/// docs' simplicity-validation rationale). All callers guarantee finiteness
/// before calling (checked in `Profile::new` step 1 / loop validation), so
/// the conversions are total.
mod exact {
    use cad_primitives::Point2;
    use dashu::float::FBig;
    use dashu::rational::RBig;
    use std::cmp::Ordering;

    /// Lossless f64 → rational. Total for finite input (pre-checked).
    fn r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }

    /// Exact orientation of `c` relative to the directed line `a → b`:
    /// `Greater` = left, `Less` = right, `Equal` = collinear.
    pub(super) fn orient2d(a: Point2, b: Point2, c: Point2) -> Ordering {
        let det = (r(b.x()) - r(a.x())) * (r(c.y()) - r(a.y()))
            - (r(b.y()) - r(a.y())) * (r(c.x()) - r(a.x()));
        det.cmp(&RBig::ZERO)
    }

    /// Spike test at vertex `b` of the path `a → b → c`: the incident edges
    /// are collinear AND `c` heads back toward `a` (exact dot > 0).
    pub(super) fn doubles_back(a: Point2, b: Point2, c: Point2) -> bool {
        if orient2d(a, b, c) != Ordering::Equal {
            return false;
        }
        let dot = (r(c.x()) - r(b.x())) * (r(a.x()) - r(b.x()))
            + (r(c.y()) - r(b.y())) * (r(a.y()) - r(b.y()));
        dot.cmp(&RBig::ZERO) == Ordering::Greater
    }

    /// `q` is known collinear with `a`–`b`; is it within the segment's
    /// closed bounding box? (f64 comparisons are exact — no arithmetic.)
    fn on_collinear_segment(a: Point2, b: Point2, q: Point2) -> bool {
        q.x() >= a.x().min(b.x())
            && q.x() <= a.x().max(b.x())
            && q.y() >= a.y().min(b.y())
            && q.y() <= a.y().max(b.y())
    }

    /// Do CLOSED segments `p1p2` and `p3p4` share any point (proper
    /// crossing, endpoint touch, or collinear overlap)? Exact.
    pub(super) fn closed_segments_intersect(
        p1: Point2,
        p2: Point2,
        p3: Point2,
        p4: Point2,
    ) -> bool {
        let d1 = orient2d(p3, p4, p1);
        let d2 = orient2d(p3, p4, p2);
        let d3 = orient2d(p1, p2, p3);
        let d4 = orient2d(p1, p2, p4);
        if ((d1 == Ordering::Greater && d2 == Ordering::Less)
            || (d1 == Ordering::Less && d2 == Ordering::Greater))
            && ((d3 == Ordering::Greater && d4 == Ordering::Less)
                || (d3 == Ordering::Less && d4 == Ordering::Greater))
        {
            return true; // proper crossing
        }
        (d1 == Ordering::Equal && on_collinear_segment(p3, p4, p1))
            || (d2 == Ordering::Equal && on_collinear_segment(p3, p4, p2))
            || (d3 == Ordering::Equal && on_collinear_segment(p1, p2, p3))
            || (d4 == Ordering::Equal && on_collinear_segment(p1, p2, p4))
    }

    /// Exact sign of the loop's shoelace sum (`Greater` = CCW).
    pub(super) fn signed_area_sign(pts: &[Point2]) -> Ordering {
        let mut sum = RBig::ZERO;
        let n = pts.len();
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            sum += r(p.x()) * r(q.y()) - r(q.x()) * r(p.y());
        }
        sum.cmp(&RBig::ZERO)
    }

    /// Exact crossing-parity point-in-polygon, STRICT interior. The caller
    /// guarantees `q` does not lie on the boundary (loop disjointness is
    /// established first), which rules out the `orient == Equal` crossing
    /// ambiguity: a straddling edge with `q` on its supporting line would
    /// put `q` on the segment itself.
    pub(super) fn point_strictly_inside(q: Point2, pts: &[Point2]) -> bool {
        let mut inside = false;
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            // Half-open straddle test (exact f64 comparisons).
            if (a.y() > q.y()) == (b.y() > q.y()) {
                continue;
            }
            let upward = b.y() > a.y();
            let o = orient2d(a, b, q);
            // Ray +x from q crosses an upward edge iff q is strictly left
            // of it, a downward edge iff strictly right.
            if (upward && o == Ordering::Greater) || (!upward && o == Ordering::Less) {
                inside = !inside;
            }
        }
        inside
    }
}

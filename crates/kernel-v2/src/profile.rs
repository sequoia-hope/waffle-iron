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
        _origin: Point3,
        _u: Vector3,
        _v: Vector3,
        _outer: Vec<Point2>,
        _holes: Vec<Vec<Point2>>,
    ) -> Result<Self, KernelV2Error> {
        Err(KernelV2Error::NotImplemented("Profile::new (PR-KV2 RED)"))
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

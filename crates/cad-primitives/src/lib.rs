//! Shared geometric primitives and constants for the clean-sheet kernel rewrite.
//!
//! This crate is the foundation depended on by `cherchi-rs`, `ssi-rs`,
//! `yang-rs`, and `kernel-v2`. It holds **types only** — no algorithms.
//!
//! ## Scope discipline
//!
//! Things that belong here:
//! - Geometric primitive types (`Point3`, eventually `Vector3`)
//! - Distance/angle tolerance constants (`TAU_MODEL`, `MIN_FEATURE_SIZE`)
//! - Boolean operation enum (`BoolOp { Union, Intersect, Subtract, Xor }`)
//! - Cross-crate error type (`KernelError`)
//!
//! Things that do NOT belong here:
//! - Mesh data structures (live in `cherchi-rs` or `yang-rs`'s internal mesh)
//! - B-Rep data structures (live in `kernel-v2`)
//! - Any algorithm — predicates, intersections, tessellation, anything
//!
//! When in doubt: if it has a `fn` doing computation, it does not belong here.

/// Model-space distance tolerance: two coordinates closer than this are
/// considered coincident at modeling resolution. All distances are in meters.
pub const TAU_MODEL: f64 = 1e-7;

/// Minimum feature size: edges/faces/areas below this are treated as
/// degenerate (e.g. the zero-area-face threshold for Newell normals).
pub const MIN_FEATURE_SIZE: f64 = 1e-6;

/// Working / exact-arithmetic tolerance, tighter than `TAU_MODEL`, used for
/// numerically sensitive intermediate computations.
pub const TAU_WORK: f64 = 1e-12;

/// A point in 3D Euclidean space, stored as three `f64` coordinates.
///
/// Newtype wrapper around `[f64; 3]` — no algorithms, just storage +
/// accessors. Designed for use across the new tier crates (cherchi-rs,
/// yang-rs, ssi-rs, kernel-v2) without committing to a particular vector
/// library.
///
/// `Point3` is `Copy` and small (24 bytes). Pass by value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point3 {
    coords: [f64; 3],
}

impl Point3 {
    /// Construct a point from three coordinates.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { coords: [x, y, z] }
    }

    /// X coordinate.
    pub fn x(&self) -> f64 {
        self.coords[0]
    }

    /// Y coordinate.
    pub fn y(&self) -> f64 {
        self.coords[1]
    }

    /// Z coordinate.
    pub fn z(&self) -> f64 {
        self.coords[2]
    }

    /// Raw coordinate array.
    pub fn as_array(&self) -> [f64; 3] {
        self.coords
    }
}

impl From<[f64; 3]> for Point3 {
    fn from(coords: [f64; 3]) -> Self {
        Self { coords }
    }
}

impl From<Point3> for [f64; 3] {
    fn from(p: Point3) -> Self {
        p.coords
    }
}

/// A point in 2D Euclidean space, stored as two `f64` coordinates.
///
/// Newtype wrapper around `[f64; 2]` — no algorithms, just storage +
/// accessors. Mirrors `Point3` exactly, one dimension lower. Used by
/// 2D predicates in `cherchi-rs` (orient2d et al.) and by future 2D
/// refinement consumers in the Cherchi 2022 §4 coplanar handler.
///
/// `Point2` is `Copy` and small (16 bytes). Pass by value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point2 {
    coords: [f64; 2],
}

impl Point2 {
    /// Construct a point from two coordinates.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { coords: [x, y] }
    }

    /// X coordinate.
    pub fn x(&self) -> f64 {
        self.coords[0]
    }

    /// Y coordinate.
    pub fn y(&self) -> f64 {
        self.coords[1]
    }

    /// Raw coordinate array.
    pub fn as_array(&self) -> [f64; 2] {
        self.coords
    }
}

impl From<[f64; 2]> for Point2 {
    fn from(coords: [f64; 2]) -> Self {
        Self { coords }
    }
}

impl From<Point2> for [f64; 2] {
    fn from(p: Point2) -> Self {
        p.coords
    }
}

/// A vector in 3D Euclidean space, stored as three `f64` components.
///
/// Mirror of `Point3` (24 bytes, `Copy`) but a *direction/displacement*,
/// not a position. Used by `yang-rs::Surface::Plane` for outward normals,
/// eventually by `kernel-v2` for half-edge surface normals and by
/// `ssi-rs` for intersection-curve tangents.
///
/// No algorithms (`cross`, `dot`, `normalize`) — those belong in
/// consumer crates per cad-primitives' "types only" scope rule.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vector3 {
    coords: [f64; 3],
}

impl Vector3 {
    /// Construct a vector from three components.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { coords: [x, y, z] }
    }

    pub fn x(&self) -> f64 {
        self.coords[0]
    }

    pub fn y(&self) -> f64 {
        self.coords[1]
    }

    pub fn z(&self) -> f64 {
        self.coords[2]
    }

    pub fn as_array(&self) -> [f64; 3] {
        self.coords
    }
}

impl From<[f64; 3]> for Vector3 {
    fn from(coords: [f64; 3]) -> Self {
        Self { coords }
    }
}

impl From<Vector3> for [f64; 3] {
    fn from(v: Vector3) -> Self {
        v.coords
    }
}

/// Boolean operation between two meshes / solids.
///
/// Variant naming follows the workspace convention (`Intersect` /
/// `Subtract`, not `Intersection` / `Subtraction`). Crates that
/// interface with the upstream Cherchi 2022 binary map to the CLI
/// strings (`"intersection"`, `"subtraction"`) via a private
/// `cli_arg()` helper.
///
/// `Xor` is the symmetric difference: in A xor B iff in A xor in B.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Intersect,
    Subtract,
    Xor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_and_access() {
        let p = Point3::new(1.0, 2.0, 3.0);
        assert_eq!(p.x(), 1.0);
        assert_eq!(p.y(), 2.0);
        assert_eq!(p.z(), 3.0);
    }

    #[test]
    fn round_trip_array() {
        let arr = [1.5, 2.5, 3.5];
        let p: Point3 = arr.into();
        let back: [f64; 3] = p.into();
        assert_eq!(back, arr);
    }

    #[test]
    fn equality() {
        assert_eq!(Point3::new(1.0, 2.0, 3.0), Point3::new(1.0, 2.0, 3.0));
        assert_ne!(Point3::new(1.0, 2.0, 3.0), Point3::new(1.0, 2.0, 4.0));
    }

    #[test]
    fn point2_construct_and_access() {
        let p = Point2::new(1.0, 2.0);
        assert_eq!(p.x(), 1.0);
        assert_eq!(p.y(), 2.0);
    }

    #[test]
    fn point2_round_trip_array() {
        let arr = [1.5, 2.5];
        let p: Point2 = arr.into();
        let back: [f64; 2] = p.into();
        assert_eq!(back, arr);
    }

    #[test]
    fn point2_equality() {
        assert_eq!(Point2::new(1.0, 2.0), Point2::new(1.0, 2.0));
        assert_ne!(Point2::new(1.0, 2.0), Point2::new(1.0, 3.0));
    }

    // ----- BoolOp -----

    #[test]
    fn boolop_variants_distinct() {
        assert_ne!(BoolOp::Union, BoolOp::Intersect);
        assert_ne!(BoolOp::Union, BoolOp::Subtract);
        assert_ne!(BoolOp::Union, BoolOp::Xor);
        assert_ne!(BoolOp::Intersect, BoolOp::Subtract);
        assert_ne!(BoolOp::Intersect, BoolOp::Xor);
        assert_ne!(BoolOp::Subtract, BoolOp::Xor);
    }

    #[test]
    fn boolop_debug_names() {
        assert_eq!(format!("{:?}", BoolOp::Union), "Union");
        assert_eq!(format!("{:?}", BoolOp::Intersect), "Intersect");
        assert_eq!(format!("{:?}", BoolOp::Subtract), "Subtract");
        assert_eq!(format!("{:?}", BoolOp::Xor), "Xor");
    }

    #[test]
    fn boolop_copy() {
        let a = BoolOp::Union;
        let b = a; // Copy
        assert_eq!(a, b);
        // Confirm Clone is bound (cannot test via .clone() on a Copy
        // type per clippy; the trait bound itself is the contract).
        fn requires_clone<T: Clone>() {}
        requires_clone::<BoolOp>();
    }

    // ----- Vector3 -----

    #[test]
    fn vector3_construct_and_access() {
        let v = Vector3::new(1.0, 2.0, 3.0);
        assert_eq!(v.x(), 1.0);
        assert_eq!(v.y(), 2.0);
        assert_eq!(v.z(), 3.0);
    }

    #[test]
    fn vector3_round_trip_array() {
        let arr = [1.5, 2.5, 3.5];
        let v: Vector3 = arr.into();
        let back: [f64; 3] = v.into();
        assert_eq!(back, arr);
    }

    #[test]
    fn vector3_equality() {
        assert_eq!(Vector3::new(1.0, 2.0, 3.0), Vector3::new(1.0, 2.0, 3.0));
        assert_ne!(Vector3::new(1.0, 2.0, 3.0), Vector3::new(1.0, 2.0, 4.0));
    }

    // ----- M1: tolerance constants -----

    /// M1 spec §"Branch table" / I-invariants: the three tolerance
    /// constants must exist with the documented values. `MIN_FEATURE_SIZE`
    /// is the degenerate-face threshold (B3); `TAU_MODEL` / `TAU_WORK` are
    /// the model/work tolerances used downstream.
    #[test]
    fn tolerance_constants_have_expected_values() {
        assert_eq!(TAU_MODEL, 1e-7);
        assert_eq!(MIN_FEATURE_SIZE, 1e-6);
        assert_eq!(TAU_WORK, 1e-12);
    }
}

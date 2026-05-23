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
//! - Boolean operation enum (`BoolOp`)
//! - Cross-crate error type (`KernelError`)
//!
//! Things that do NOT belong here:
//! - Mesh data structures (live in `cherchi-rs` or `yang-rs`'s internal mesh)
//! - B-Rep data structures (live in `kernel-v2`)
//! - Any algorithm — predicates, intersections, tessellation, anything
//!
//! When in doubt: if it has a `fn` doing computation, it does not belong here.

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
}

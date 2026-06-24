//! KV6d increment 4a: yang `Surface::Torus` type + analytic `signed_distance`.
//!
//! The 2D bijective torus tessellation (Stage-1 ingestion) is the focused
//! follow-up 4b; this pins the analytic surface distance, which Stage-2 in/out
//! classification relies on.

use cad_primitives::{Point3, Vector3};
use yang_rs::{signed_distance_to_surface, Surface};

#[test]
fn torus_signed_distance_matches_tube_residual() {
    // Ring torus: center origin, axis +z, major R=3, minor r=1. The tube
    // surface is the set with √((ρ−3)²+τ²) = 1.
    let torus = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };
    let dist = |x: f64, y: f64, z: f64| {
        signed_distance_to_surface(torus, Point3::new(x, y, z)).expect("torus distance")
    };
    // On-surface points (≈ 0): outer/inner equator (ρ=4,2 at τ=0) and top/bottom
    // of the tube (ρ=3, τ=±1).
    assert!(dist(4.0, 0.0, 0.0).abs() < 1e-12);
    assert!(dist(2.0, 0.0, 0.0).abs() < 1e-12);
    assert!(dist(3.0, 0.0, 1.0).abs() < 1e-12);
    assert!(dist(3.0, 0.0, -1.0).abs() < 1e-12);
    // Tube center (ρ=3, τ=0) is inside → −1; far out (ρ=5) → +1.
    assert!((dist(3.0, 0.0, 0.0) - (-1.0)).abs() < 1e-12);
    assert!((dist(5.0, 0.0, 0.0) - 1.0).abs() < 1e-12);
    // Off-axis direction works too (ρ=4 along +y).
    assert!(dist(0.0, 4.0, 0.0).abs() < 1e-12);
}

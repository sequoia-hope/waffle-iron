//! N-mb-3b share-a-face default target (GEOMETRIC half) — pure predicate tests.
//! Spec §4.3(b): a body auto-merges when it has a planar face COINCIDENT with the
//! sketch plane AND OVERLAPPING the profile footprint, even on a datum sketch.
//!
//! These pin the two pure helpers the Implementer adds in the new PUBLIC module
//! `feature_engine::share_a_face`:
//!   - `plane_coincident(sketch_origin, sketch_normal, face_normal, face_point)`
//!   - `polygons_overlap_2d(a, b)`
//!
//! RED expectation: the module `feature_engine::share_a_face` and its functions
//! do not exist yet ⇒ this file fails to COMPILE (unresolved import / path).
//! That compile error IS the RED state for this FIP cycle.

use feature_engine::share_a_face::{plane_coincident, polygons_overlap_2d};

// ── plane_coincident ─────────────────────────────────────────────────────────

#[test]
fn coincident_same_plane_same_normal() {
    // Sketch plane z=5, +z; face on z=5 with +z normal ⇒ coincident.
    assert!(plane_coincident(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [2.0, 3.0, 5.0],
    ));
}

#[test]
fn coincident_same_plane_antiparallel_normal() {
    // A body's TOP face (normal +z) vs a DOWNWARD sketch normal (−z): same plane,
    // (anti)parallel normals ⇒ still coincident (|n̂·n̂_f| > 1 − TAU_MODEL).
    assert!(plane_coincident(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [2.0, 3.0, 5.0],
    ));
}

#[test]
fn not_coincident_parallel_but_offset() {
    // Parallel normals, but the face sits 0.001 above the plane ≫ TAU_MODEL (1e-7).
    assert!(!plane_coincident(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [2.0, 3.0, 5.001],
    ));
}

#[test]
fn not_coincident_nonparallel_normals() {
    // Face normal +x is perpendicular to the sketch normal +z ⇒ not coincident.
    assert!(!plane_coincident(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [2.0, 3.0, 5.0],
    ));
}

#[test]
fn coincident_within_tolerance_offset() {
    // Offset of 5e-8 < TAU_MODEL (1e-7) ⇒ still coincident.
    assert!(plane_coincident(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [2.0, 3.0, 5.0 + 5e-8],
    ));
}

// ── polygons_overlap_2d ──────────────────────────────────────────────────────

/// CCW unit-ish square [x0,y0]-[x1,y1].
fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

#[test]
fn overlap_two_overlapping_squares() {
    // [0,0]-[2,2] and [1,1]-[3,3] share the [1,1]-[2,2] region.
    assert!(polygons_overlap_2d(
        &square(0.0, 0.0, 2.0, 2.0),
        &square(1.0, 1.0, 3.0, 3.0),
    ));
}

#[test]
fn no_overlap_disjoint_squares() {
    assert!(!polygons_overlap_2d(
        &square(0.0, 0.0, 1.0, 1.0),
        &square(5.0, 5.0, 6.0, 6.0),
    ));
}

#[test]
fn overlap_one_fully_inside_other() {
    // [2,2]-[3,3] sits entirely within [0,0]-[10,10]; a vertex-inside catches it.
    assert!(polygons_overlap_2d(
        &square(0.0, 0.0, 10.0, 10.0),
        &square(2.0, 2.0, 3.0, 3.0),
    ));
}

#[test]
fn no_overlap_edge_touching_only() {
    // Sharing the x=1 edge only ⇒ zero-area overlap ⇒ false.
    assert!(!polygons_overlap_2d(
        &square(0.0, 0.0, 1.0, 1.0),
        &square(1.0, 0.0, 2.0, 1.0),
    ));
}

#[test]
fn overlap_identical_squares() {
    assert!(polygons_overlap_2d(
        &square(0.0, 0.0, 1.0, 1.0),
        &square(0.0, 0.0, 1.0, 1.0),
    ));
}

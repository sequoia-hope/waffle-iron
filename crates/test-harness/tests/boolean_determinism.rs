//! Boolean determinism tests.
//!
//! Verifies that boolean operations produce identical results across
//! multiple runs, regardless of memory allocation patterns or hash map
//! iteration order.

use test_harness::helpers::mesh_volume;
use test_harness::ModelBuilder;

/// Run a boss union boolean N times and verify all produce identical face counts
/// and volumes.
#[test]
fn boolean_union_deterministic_10x() {
    let mut face_counts = Vec::new();
    let mut volumes = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Boss on top face — triggers coplanar boolean
        m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
            .unwrap();
        m.extrude("boss", "boss_sk", 5.0).unwrap();

        let mesh = m.tessellate("boss").unwrap();
        let tri_count = mesh.indices.len() / 3;
        face_counts.push(tri_count);
        volumes.push(mesh_volume(&mesh));
    }

    let expected_faces = face_counts[0];
    let expected_vol: f64 = volumes[0];
    for (i, (&fc, &vol)) in face_counts.iter().zip(volumes.iter()).enumerate() {
        assert_eq!(
            fc, expected_faces,
            "Run {}: face count {} != expected {}",
            i, fc, expected_faces
        );
        assert!(
            (vol - expected_vol).abs() < 1.0,
            "Run {}: volume {} != expected {}",
            i,
            vol,
            expected_vol
        );
    }
}

/// Run a subtract boolean N times and verify deterministic results.
#[test]
fn boolean_subtract_deterministic_10x() {
    let mut face_counts = Vec::new();
    let mut volumes = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Cut pocket in center of top face
        m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
            .unwrap();
        m.extrude_cut("pocket", "cut_sk", 5.0).unwrap();

        let mesh = m.tessellate("pocket").unwrap();
        let tri_count = mesh.indices.len() / 3;
        face_counts.push(tri_count);
        volumes.push(mesh_volume(&mesh));
    }

    let expected_faces = face_counts[0];
    let expected_vol: f64 = volumes[0];
    for (i, (&fc, &vol)) in face_counts.iter().zip(volumes.iter()).enumerate() {
        assert_eq!(
            fc, expected_faces,
            "Run {}: face count {} != expected {}",
            i, fc, expected_faces
        );
        assert!(
            (vol - expected_vol).abs() < 1.0,
            "Run {}: volume {} != expected {}",
            i,
            vol,
            expected_vol
        );
    }
}

/// Chained boolean determinism: boss then cut, repeated.
#[test]
fn chained_boolean_deterministic_10x() {
    let mut face_counts = Vec::new();
    let mut volumes = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Two cuts on top face
        m.rect_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("cut1", "cut1_sk", 5.0).unwrap();

        m.rect_sketch("cut2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
            .unwrap();
        m.extrude_cut("cut2", "cut2_sk", 5.0).unwrap();

        let mesh = m.tessellate("cut2").unwrap();
        let tri_count = mesh.indices.len() / 3;
        face_counts.push(tri_count);
        volumes.push(mesh_volume(&mesh));
    }

    let expected_faces = face_counts[0];
    let expected_vol: f64 = volumes[0];
    for (i, (&fc, &vol)) in face_counts.iter().zip(volumes.iter()).enumerate() {
        assert_eq!(
            fc, expected_faces,
            "Run {}: face count {} != expected {}",
            i, fc, expected_faces
        );
        assert!(
            (vol - expected_vol).abs() < 1.0,
            "Run {}: volume {} != expected {}",
            i,
            vol,
            expected_vol
        );
    }
}

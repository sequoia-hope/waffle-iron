//! Adapter `Kernel::extract_edges` curved-edge export (task #140 follow-up,
//! user case step_extrude.waffle round 2).
//!
//! The trait-level edge export used to emit bare endpoint chords for EVERY
//! edge, so a rounded outline's arcs reached the app as straight lines: the
//! projected sketch boundary was a chord polygon and its offset lost the
//! corner radii. The export now reuses `introspect::edge_polyline` (the
//! chord-bound render sampling) and carries the analytic `EdgeCurve`
//! descriptor for circular edges so sketch projection can mint TRUE arcs.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::{EdgeCurve, Kernel};
use waffle_types::{CircleProfile, ClosedProfile};

fn circle_profile(radius: f64) -> ClosedProfile {
    ClosedProfile {
        entity_ids: vec![1],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: 0.0,
            center_v: 0.0,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    }
}

#[test]
fn cylinder_rims_export_sampled_closed_polylines_with_circle_descriptors() {
    let mut k = KernelV2Adapter::new();
    let faces = k
        .make_faces_from_profiles(
            &[circle_profile(0.01)],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .expect("circle profile face");
    let solid = k
        .extrude_face(faces[0], [0.0, 0.0, 1.0], 0.02)
        .expect("cylinder");

    let edges = k.extract_edges(&solid, 1e-4).expect("edge render data");

    let mut circles = 0;
    for range in &edges.edge_ranges {
        let n = (range.end_vertex - range.start_vertex) as usize;
        match &range.curve {
            Some(EdgeCurve::Circle { radius, normal, .. }) => {
                circles += 1;
                assert!(
                    n > 16,
                    "rim circle must be a sampled polyline, got {n} points"
                );
                // Closed: last point == first point (bitwise per contract).
                let s = range.start_vertex as usize * 3;
                let e = (range.end_vertex as usize - 1) * 3;
                assert_eq!(edges.vertices[s..s + 3], edges.vertices[e..e + 3]);
                assert!((radius - 0.01).abs() < 1e-12, "radius {radius}");
                assert!((normal[2].abs() - 1.0).abs() < 1e-12, "axis {normal:?}");
                // Every sample lies on the circle.
                for i in range.start_vertex as usize..range.end_vertex as usize {
                    let x = f64::from(edges.vertices[i * 3]);
                    let y = f64::from(edges.vertices[i * 3 + 1]);
                    let r = (x * x + y * y).sqrt();
                    assert!((r - 0.01).abs() < 1e-6, "sample off circle: r={r}");
                }
            }
            Some(EdgeCurve::Arc { .. }) => {
                panic!("cylinder has no partial-arc edges");
            }
            None => {
                // A seam edge (if present) stays a 2-point straight segment.
                assert_eq!(n, 2, "straight edges stay 2-point polylines");
            }
        }
    }
    assert_eq!(circles, 2, "top + bottom rim circles carry descriptors");
}

#[test]
fn box_edges_stay_two_point_segments_without_descriptors() {
    let mut k = KernelV2Adapter::new();
    let mut positions = HashMap::new();
    positions.insert(1, (-0.01, -0.01));
    positions.insert(2, (0.01, -0.01));
    positions.insert(3, (0.01, 0.01));
    positions.insert(4, (-0.01, 0.01));
    let profile = ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
        vertex_ids: vec![1, 2, 3, 4],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let faces = k
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rect face");
    let solid = k
        .extrude_face(faces[0], [0.0, 0.0, 1.0], 0.02)
        .expect("box");

    let edges = k.extract_edges(&solid, 1e-4).expect("edge render data");
    assert_eq!(edges.edge_ranges.len(), 12, "box has 12 edges");
    for range in &edges.edge_ranges {
        assert!(range.curve.is_none(), "straight edges carry no descriptor");
        assert_eq!(range.end_vertex - range.start_vertex, 2);
    }
}

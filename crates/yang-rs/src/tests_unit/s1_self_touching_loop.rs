//! Stage-1 self-touching boundary loop (N74, P0020).
//!
//! A curved face whose boundary loop VISITS ONE POSITION TWICE is the honest
//! B-Rep image of a PINCHED face — the Stage-4 edge-pinch split
//! (`specs/yang_tangency_pinch_split.md` §0a) gives each sheet of a tangential
//! contact its own vertex, and Stage 6 emits the sheets into one face. The
//! unrolled chart then has two bit-identical `(u, v)` entries and the
//! polygon-with-holes CDT has no representation for it.
//!
//! Before this gate the failure surfaced from `cherchi-rs` as
//! `MalformedTopology("face 0: holed lateral CDT failed: duplicate
//! (coincident) loop vertex in CDT input")` — same loudness, no locus: neither
//! the pair nor the pinch nor the producer was named. The gate refuses the
//! same input one layer earlier, typed.

use super::*;

/// A bounded cylinder patch (Slice A: azimuth [0, 1] rad, z ∈ [0, 1] on a
/// unit-radius cylinder about +Z) whose top boundary carries a generator
/// SPUR: out from `P` down to the tip `T` and back to `P2`.
///
/// `coincident` places `P2` on `P` exactly — the pinch. Otherwise `P2` sits a
/// hair further round in azimuth, which is an ordinary (if thin) notch and
/// must tessellate.
fn spur_patch(coincident: bool) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let on = |theta: f64, z: f64| Point3::new(theta.cos(), theta.sin(), z);
    let p_theta = 0.6_f64;
    let p2_theta = if coincident { p_theta } else { 0.5995 };
    let verts = [
        on(0.0, 0.0),      // V0
        on(1.0, 0.0),      // V1
        on(1.0, 1.0),      // V2
        on(p_theta, 1.0),  // V3 = P, the spur's first base
        on(p_theta, 0.7),  // V4 = T, the spur tip
        on(p2_theta, 1.0), // V5 = P2, the spur's second base
        on(0.0, 1.0),      // V6
    ]
    .into_iter()
    .map(|point| BRepVertex { point })
    .collect::<Vec<_>>();
    // The rim arc's NORMAL carries its traversal sense: the bottom rim runs
    // +theta (CCW about +Z), the top rim runs back in -theta, so the loop
    // nets ~0 winding and is a bounded partial patch (Slice A).
    let circ = |z: f64, sign: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, z),
        normal: Vector3::new(0.0, 0.0, sign),
        radius: 1.0,
    };
    let e = |start: u32, end: u32, curve: Curve| BRepEdge { start, end, curve };
    let edges = vec![
        e(0, 1, circ(0.0, 1.0)),     // bottom rim arc, +theta
        e(1, 2, Curve::LineSegment), // generator
        e(2, 3, circ(1.0, -1.0)),    // top rim arc back to P
        e(3, 4, Curve::LineSegment), // spur out
        e(4, 5, Curve::LineSegment), // spur back
        e(5, 6, circ(1.0, -1.0)),    // top rim arc on to V6
        e(6, 0, Curve::LineSegment), // generator
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop: (0..7).collect(),
        inner_loops: vec![],
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
fn a_loop_that_returns_to_one_point_is_a_typed_pinch_stop() {
    let (verts, edges, faces) = spur_patch(true);
    match stage1_tessellate(&verts, &edges, &faces) {
        Err(YangError::Stage1SelfTouchingLoop {
            face,
            vertices,
            point,
        }) => {
            assert_eq!(face, 0);
            // The two distinct boundary vertices the loop pinches at: P (3)
            // and P2 (5), in loop order.
            assert_eq!(vertices, (3, 5), "pinch pair");
            let p = verts[3].point.as_array();
            assert_eq!(
                point.map(f64::to_bits),
                p.map(f64::to_bits),
                "the reported point is the shared one, bit-exactly"
            );
        }
        Err(e) => panic!("expected Stage1SelfTouchingLoop, got {e:?}"),
        Ok(_) => panic!("expected Stage1SelfTouchingLoop, got a tessellation"),
    }
}

/// Mutation guard: the gate keys on BIT-EXACT world coincidence, not on
/// proximity. The same spur with its second base 5e-4 rad further round — a
/// thin notch, not a pinch — tessellates, so the gate cannot be a band in
/// disguise and cannot be what refuses an ordinary narrow feature.
#[test]
fn a_thin_notch_is_not_a_pinch_and_still_tessellates() {
    let (verts, edges, faces) = spur_patch(false);
    let t = stage1_tessellate(&verts, &edges, &faces).expect("a thin notch is not a pinch");
    assert!(!t.tris.is_empty(), "the notch patch triangulates");
    // Every one of the seven B-Rep vertices survives into the mesh (the
    // Stage-1 boundary bijection), including both spur bases.
    for (i, v) in verts.iter().enumerate() {
        let want = v.point.as_array();
        assert!(
            t.verts
                .iter()
                .any(|p| p.as_array().map(f64::to_bits) == want.map(f64::to_bits)),
            "boundary vertex {i} missing from the tessellation"
        );
    }
}

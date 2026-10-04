//! Stage-1 self-touching boundary loop (N74 → N78, P0020).
//!
//! A curved face whose boundary loop VISITS ONE POSITION TWICE can mean two
//! structurally different things, and the symptom alone does not say which:
//!
//! * a **two-region PINCH** — two closed regions meeting at a point, each
//!   enclosing real area. That is the per-SHEET face case of
//!   `specs/yang_tangency_pinch_split.md` §0b, and no corpus case exhibits it;
//! * a zero-width **SLIT** — a doubled polyline out and back along one curve,
//!   enclosing EXACTLY nothing, so the domain is ONE region with a hairline
//!   cut. That is P0020 (both of its contacts, nested, measured 2026-10-03),
//!   and it is minted by the Stage-4 edge-pinch split reading the §0a
//!   certificate at the `(4a2)` site on a pinch Stage 4 itself produced
//!   (deviation **N78**; the arrangement hands over a manifold mesh).
//!
//! Either way the unrolled chart has two bit-identical `(u, v)` entries and
//! the polygon-with-holes CDT has no representation for it, so the gate is
//! right to refuse — but the two must be told apart, because only one of them
//! is §0b's. `loop_self_contact` is that classifier and both branches are
//! pinned below.
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
            inner_edges,
            inner_area2,
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
            // N78: the contact is a SLIT, not two regions. The spur out to T
            // and back is a 2-edge sub-loop enclosing EXACTLY nothing, so
            // `yang_tangency_pinch_split` §0b (one face per SHEET) has no
            // second sheet here. This is the structure P0020 measures in the
            // corpus too (two contacts, 2- and 4-edge sub-loops, both `0e0`,
            // the complement carrying the whole 1.7395573469680094e-2).
            assert_eq!(inner_edges, 2, "the spur's sub-loop is two edges");
            assert_eq!(
                inner_area2, 0.0,
                "a slit encloses exactly zero chart area (got {inner_area2:e})"
            );
            assert!(
                format!(
                    "{}",
                    YangError::Stage1SelfTouchingLoop {
                        face,
                        vertices,
                        point,
                        inner_edges,
                        inner_area2
                    }
                )
                .contains("zero-width SLIT"),
                "the wall must name the SLIT, not §0b's per-sheet case"
            );
        }
        Err(e) => panic!("expected Stage1SelfTouchingLoop, got {e:?}"),
        Ok(_) => panic!("expected Stage1SelfTouchingLoop, got a tessellation"),
    }
}

// =========================================================================
// N78 (2026-10-03): the SLIT / two-region discriminator itself.
//
// The same symptom — a boundary loop visiting one position twice — means two
// structurally different things, and P0020's first anchor read the wrong one.
// `loop_self_contact` is the production classifier the wall's text and the
// §0b applicability both rest on, so both of its branches are pinned here on
// hand-built chart loops where the answer is known in closed form.
// =========================================================================

fn pt(x: f64, y: f64) -> cad_primitives::Point2 {
    cad_primitives::Point2::new(x, y)
}

/// A DOUBLED-POLYLINE excursion: the loop runs out from `P` (index 0) to a
/// tip and back to `P2` (index 2, the same chart point as 0). The sub-loop is
/// two edges and encloses exactly nothing — a SLIT.
#[test]
pub(crate) fn a_doubled_excursion_classifies_as_a_zero_area_slit() {
    // Chart: a unit square with a spur from (0.5, 1) down to (0.5, 0.4).
    let verts = vec![
        pt(0.5, 1.0), // 0 — P, first base
        pt(0.5, 0.4), // 1 — the tip
        pt(0.5, 1.0), // 2 — P2, second base (same point as 0)
        pt(0.0, 1.0), // 3
        pt(0.0, 0.0), // 4
        pt(1.0, 0.0), // 5
        pt(1.0, 1.0), // 6
    ];
    // Loop order: P → tip → P2 → … round the square … → P.
    let lp: Vec<u32> = vec![0, 1, 2, 6, 5, 4, 3];
    let (edges, area2) = super::super::stage1_tessellate::loop_self_contact(&lp, &verts, 0, 2)
        .expect("both vertices are on the loop");
    assert_eq!(edges, 2, "P → tip → P2 is a two-edge sub-loop");
    assert_eq!(
        area2, 0.0,
        "a slit encloses exactly zero area, got {area2:e}"
    );
    // The complement carries the WHOLE area: the unit square, doubled = 2.
    let outer = super::super::stage1_tessellate::sub_loop_area2(&lp, &verts, 2, 0);
    assert_eq!(outer.abs(), 2.0, "the complement is the whole square");
}

/// A FIGURE-EIGHT: two unit squares meeting at the single chart point
/// (1, 1), reached by two distinct loop vertices. BOTH sub-loops enclose a
/// full square, so this is the two-region pinch `yang_tangency_pinch_split`
/// §0b is written for — and it is what P0020 is NOT.
#[test]
pub(crate) fn a_figure_eight_classifies_as_a_two_region_pinch() {
    let verts = vec![
        pt(1.0, 1.0), // 0 — P
        pt(0.0, 1.0), // 1
        pt(0.0, 0.0), // 2
        pt(1.0, 0.0), // 3
        pt(1.0, 1.0), // 4 — P2, same point as 0
        pt(2.0, 1.0), // 5
        pt(2.0, 2.0), // 6
        pt(1.0, 2.0), // 7
    ];
    let lp: Vec<u32> = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let (edges, area2) = super::super::stage1_tessellate::loop_self_contact(&lp, &verts, 0, 4)
        .expect("both vertices are on the loop");
    assert_eq!(edges, 4, "the shorter sub-loop is the first square");
    assert_eq!(
        area2.abs(),
        2.0,
        "a lobe of the figure-eight encloses a whole unit square, got {area2:e}"
    );
    // Mutation guard: the discriminator is not a constant — the two shapes
    // give different answers from the SAME function.
    assert_ne!(area2, 0.0, "a two-region pinch is not a slit");
}

/// A vertex that is not on the loop has no contact to classify.
#[test]
pub(crate) fn a_vertex_off_the_loop_has_no_classification() {
    let verts = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(5.0, 5.0)];
    let lp: Vec<u32> = vec![0, 1, 2];
    assert!(
        super::super::stage1_tessellate::loop_self_contact(&lp, &verts, 0, 3).is_none(),
        "vertex 3 is not on this loop"
    );
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

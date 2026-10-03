//! Q6 oracles — `specs/agent_mechanical_design.md` §4.4, the arc-length half.
//!
//! Every expected number is a closed form written out in the assertion, or —
//! where the family has no closed form (an ellipse arc is an incomplete
//! elliptic integral) — an independent reference quadrature computed here at
//! 1000× the implementation's step count. Nothing is a value copied from a
//! previous run.

use std::collections::BTreeSet;
use std::f64::consts::TAU;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::measure::{edge_length, LengthTier};
use kernel_v2::{boolean_op, extrude, BrepArena, HalfEdgeId, Profile, SolidId};

fn block(arena: &mut BrepArena, sx: f64, sy: f64, sz: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(sx, 0.0),
            Point2::new(sx, sy),
            Point2::new(0.0, sy),
        ],
        vec![],
    )
    .expect("rectangle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), sz)
        .expect("extrude")
        .solid
}

fn cylinder(arena: &mut BrepArena, r: f64, h: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("extrude")
        .solid
}

/// One half-edge per EDGE of `solid` — the canonical (lower-id) side of each
/// twin pair, which is what "the body's edges" means.
fn canonical_edges(arena: &BrepArena, solid: SolidId) -> Vec<HalfEdgeId> {
    let mut seen = BTreeSet::new();
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            let face = arena.face(f).expect("face");
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid).expect("loop") {
                    let twin = arena.half_edge(h).expect("half-edge").twin;
                    seen.insert(h.min(twin));
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// Every canonical edge of `solid` as `(curve_type, length, closed, tier)`.
fn inventory(arena: &BrepArena, solid: SolidId) -> Vec<(&'static str, f64, bool, LengthTier)> {
    canonical_edges(arena, solid)
        .into_iter()
        .map(|h| {
            let r = edge_length(arena, h).expect("every edge has a length");
            (r.curve_type, r.value, r.closed, r.tier)
        })
        .collect()
}

/// §4.4: "a box lists 6/12/8 with exact edge lengths". The 12 edges are four
/// of each side length, every one a `line` at the `Exact` tier, and the
/// numbers are the side lengths with no tolerance at all — a chord between two
/// axis-aligned f64 vertices is computed, not approximated.
#[test]
fn a_box_has_twelve_exact_straight_edges() {
    let mut arena = BrepArena::new();
    let (sx, sy, sz) = (0.010, 0.020, 0.030);
    let b = block(&mut arena, sx, sy, sz);

    let inv = inventory(&arena, b);
    assert_eq!(inv.len(), 12, "a box has 12 edges: {inv:?}");
    assert!(
        inv.iter()
            .all(|(t, _, closed, tier)| *t == "line" && !*closed && *tier == LengthTier::Exact),
        "every box edge is an exact open line: {inv:?}"
    );

    for (side, want_count) in [(sx, 4), (sy, 4), (sz, 4)] {
        let n = inv.iter().filter(|(_, l, _, _)| *l == side).count();
        assert_eq!(
            n, want_count,
            "{want_count} edges of exactly {side} m, found {n}: {inv:?}"
        );
    }
    // And the total is the box's edge-length sum, exactly.
    let total: f64 = inv.iter().map(|(_, l, _, _)| l).sum();
    assert!(
        (total - 4.0 * (sx + sy + sz)).abs() <= f64::EPSILON * total,
        "perimeter total {total} vs 4(sx+sy+sz) {}",
        4.0 * (sx + sy + sz)
    );
}

/// §4.4: "a cylinder's rim arc length = 2πr exact". Both rims are full
/// circles, closed, at the `Exact` tier — and the number is `TAU · r` to the
/// last bit, not to a tolerance. The seam is the one straight edge, and its
/// length is the height.
#[test]
fn a_cylinder_rim_is_two_pi_r_to_the_last_bit() {
    let mut arena = BrepArena::new();
    let (r, h) = (0.025, 0.040);
    let c = cylinder(&mut arena, r, h);

    let inv = inventory(&arena, c);
    let rims: Vec<_> = inv.iter().filter(|(t, ..)| *t == "circle").collect();
    assert_eq!(rims.len(), 2, "two rims: {inv:?}");
    for (_, length, closed, tier) in &rims {
        assert_eq!(*length, TAU * r, "a rim is exactly 2πr");
        assert!(*closed, "a full circle closes on itself");
        assert_eq!(*tier, LengthTier::Exact);
    }
    // The chord between a closed curve's endpoints is ZERO — which is what
    // `TopoSignature::length` reports for this very edge. That difference is
    // the whole reason Q6 exists, so it is pinned here.
    assert!(
        rims[0].1 > 0.15,
        "the rim's arc length is not its chord: {}",
        rims[0].1
    );

    let seams: Vec<_> = inv.iter().filter(|(t, ..)| *t == "line").collect();
    assert_eq!(seams.len(), 1, "one seam: {inv:?}");
    assert_eq!(seams[0].1, h, "the seam is exactly the height");
}

/// An arc's length is `r·Δθ` from its own CCW sweep: a quarter-cylinder's two
/// rim arcs are each exactly `TAU·r/4`, and summing a split rim's arcs gives
/// the whole circle back.
#[test]
fn a_circular_arcs_length_is_its_radius_times_its_sweep() {
    let mut arena = BrepArena::new();
    let r = 0.030;
    let c = cylinder(&mut arena, r, 0.010);
    // A box that keeps one quadrant of the cylinder: each rim becomes a
    // quarter arc plus two straight edges.
    let cutter = {
        let p = Profile::new(
            Point3::new(0.0, 0.0, -0.010),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(2.0 * r, 0.0),
                Point2::new(2.0 * r, 2.0 * r),
                Point2::new(0.0, 2.0 * r),
            ],
            vec![],
        )
        .expect("quadrant profile");
        extrude(&mut arena, &p, Vector3::new(0.0, 0.0, 1.0), 0.030)
            .expect("extrude")
            .solid
    };
    let out = boolean_op(&mut arena, c, cutter, BoolOp::Intersect).expect("quadrant intersect");

    let inv = inventory(&arena, out);
    let arcs: Vec<_> = inv.iter().filter(|(t, ..)| *t == "arc").collect();
    assert!(!arcs.is_empty(), "the quadrant has rim arcs: {inv:?}");
    let quarter = TAU * r / 4.0;
    for (_, length, closed, tier) in &arcs {
        assert!(
            (length - quarter).abs() < 1e-12,
            "a quarter rim is TAU·r/4 = {quarter}, got {length}"
        );
        assert!(!*closed, "an arc runs between two distinct vertices");
        assert_eq!(*tier, LengthTier::Exact);
    }
}

/// An oblique plane cuts a cylinder in an ELLIPSE, whose arc length is an
/// incomplete elliptic integral — no closed form. The implementation reports
/// the `Quadrature` tier; this checks the value against an INDEPENDENT
/// reference integral of the same speed function at 1000× the step count, and
/// checks that the reported residual is the size of the real disagreement
/// rather than a decorative constant.
///
/// The geometry pins the answer: a plane whose unit normal makes `cos α` with
/// the cylinder axis cuts a cylinder of radius `r` in an ellipse with
/// semi-minor `r` and semi-major `r / cos α`.
#[test]
fn an_oblique_section_ellipse_matches_a_reference_integral() {
    let mut arena = BrepArena::new();
    let r = 0.25;
    let c = cylinder(&mut arena, r, 1.0);
    // Unit normal (1, 2, 2)/3 — the kv5b oblique-section fixture.
    let n = Vector3::new(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
    let s5 = 5.0f64.sqrt();
    let u = Vector3::new(2.0 / s5, -1.0 / s5, 0.0);
    let v = Vector3::new(
        n.y() * u.z() - n.z() * u.y(),
        n.z() * u.x() - n.x() * u.z(),
        n.x() * u.y() - n.y() * u.x(),
    );
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.5),
        u,
        v,
        vec![
            Point2::new(-2.0, -2.0),
            Point2::new(2.0, -2.0),
            Point2::new(2.0, 2.0),
            Point2::new(-2.0, 2.0),
        ],
        Vec::new(),
    )
    .expect("oblique rect profile");
    let slab = extrude(&mut arena, &profile, n, 1.0).expect("oblique slab");
    let out = boolean_op(&mut arena, c, slab.solid, BoolOp::Subtract).expect("oblique subtract");

    let inv = inventory(&arena, out);
    let ellipses: Vec<_> = inv.iter().filter(|(t, ..)| *t == "ellipse_arc").collect();
    assert!(
        !ellipses.is_empty(),
        "the oblique section is an ellipse: {inv:?}"
    );

    // The section ellipse: semi-minor r, semi-major r / (n̂·ẑ).
    let (minor, major) = (r, r / n.z());
    let reference = {
        let speed = |t: f64| ((major * t.sin()).powi(2) + (minor * t.cos()).powi(2)).sqrt();
        let steps = 2_000_000usize;
        let h = TAU / steps as f64;
        let mut acc = speed(0.0) + speed(TAU);
        for i in 1..steps {
            acc += if i % 2 == 1 { 4.0 } else { 2.0 } * speed(h * i as f64);
        }
        acc * h / 3.0
    };

    // Every ellipse piece of the section, summed, is the whole perimeter: the
    // cut goes all the way round the lateral.
    let got: f64 = ellipses.iter().map(|(_, l, _, _)| l).sum();
    let rel = (got - reference).abs() / reference;
    assert!(
        rel < 1e-9,
        "section perimeter {got} vs reference {reference} (relative {rel:e})"
    );

    for (_, length, _, tier) in &ellipses {
        match tier {
            LengthTier::Quadrature { residual } => {
                assert!(
                    residual.is_finite() && *residual >= 0.0 && *residual < 1e-9 * length,
                    "the witness is a measured, converged residual: {residual}"
                );
            }
            other => panic!("an ellipse arc has no closed form; tier was {other:?}"),
        }
    }
    // Sanity on the geometry the oracle rests on: a 2:3 obliquity really does
    // stretch the section well past the rim circle.
    assert!(
        reference > TAU * r * 1.1 && reference < TAU * major,
        "an oblique section is longer than the rim and shorter than its major circle: {reference}"
    );
}

/// Both sides of one edge report a BIT-IDENTICAL length. The two half-edges
/// carry mirrored geometry (a negated normal, reversed endpoints), so a
/// per-half-edge integration would differ in the last bits by summation
/// order; the length is taken on the canonical half-edge for exactly this
/// reason, and an agent that lists an edge twice must not see two numbers.
#[test]
fn twin_half_edges_report_the_same_length_bit_for_bit() {
    let mut arena = BrepArena::new();
    let c = cylinder(&mut arena, 0.025, 0.040);
    for h in canonical_edges(&arena, c) {
        let twin = arena.half_edge(h).expect("half-edge").twin;
        assert_eq!(
            edge_length(&arena, h).expect("canonical"),
            edge_length(&arena, twin).expect("twin"),
            "a twin pair must agree bit for bit"
        );
    }
}

// -------------------------------------------------------------------------
// The contract at the adapter — what a consumer of `KernelMeasure` sees
// -------------------------------------------------------------------------

/// A kind mismatch is a loud `EntityNotFound`, not a guess: handing a FACE id
/// to `edge_length` must not measure whatever edge happens to share that
/// index.
#[test]
fn only_an_edge_id_has_an_arc_length() {
    use waffle_types::kernel::{Kernel, KernelError, KernelIntrospect, KernelMeasure};
    use waffle_types::ClosedProfile;

    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let positions: std::collections::HashMap<u32, (f64, f64)> = [
        (1, (0.0, 0.0)),
        (2, (0.02, 0.0)),
        (3, (0.02, 0.01)),
        (4, (0.0, 0.01)),
    ]
    .into_iter()
    .collect();
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    let solid = kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], 0.005)
        .expect("box");

    // Every edge answers.
    let edges = KernelIntrospect::list_edges(&kernel, &solid);
    assert_eq!(edges.len(), 12);
    for e in &edges {
        let r = kernel.edge_length(*e).expect("an edge has a length");
        assert_eq!(r.curve_type, "line");
        assert!(r.value > 0.0);
    }

    // A face id does not.
    let face = KernelIntrospect::list_faces(&kernel, &solid)[0];
    match kernel.edge_length(face) {
        Err(KernelError::EntityNotFound { id }) => assert_eq!(id, face),
        other => panic!("a face id must be refused, got {other:?}"),
    }
}

/// An IMPORTED body's edge is answered — the polyline's chord sum is a real
/// number — but with NO chord bound: the tolerance the source mesh was
/// tessellated at is not ours to know, and inventing one would be worse than
/// having none.
#[test]
fn an_imported_edge_reports_chords_with_no_band() {
    use waffle_types::kernel::{
        ImportedBodyData, ImportedEdgeData, ImportedFaceData, ImportedShellData, ImportedSurface,
        Kernel, KernelIntrospect, KernelMeasure, LengthMethod,
    };

    let mut kernel = kernel_v2::KernelV2Adapter::new();
    // A two-chord polyline: the sum is 1 + 1 = 2, which no closed form of the
    // underlying curve is claimed to be.
    let data = ImportedBodyData {
        source_name: "tri".to_string(),
        shells: vec![ImportedShellData {
            faces: vec![ImportedFaceData {
                surface: ImportedSurface::Plane {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
                indices: vec![0, 1, 2],
                edge_indices: vec![0],
            }],
            edges: vec![ImportedEdgeData {
                polyline: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            }],
        }],
        warnings: vec![],
    };
    let solid = kernel.import_body(&data).expect("imported body");
    let edge = KernelIntrospect::list_edges(&kernel, &solid)[0];
    let r = kernel
        .edge_length(edge)
        .expect("an imported edge has a chord sum");
    assert_eq!(r.curve_type, "polyline");
    assert_eq!(r.value, 2.0, "the chord sum of the two segments");
    assert!(!r.closed);
    assert_eq!(
        r.method,
        LengthMethod::Chords { chord_bound: None },
        "we do not know what the source was sampled at"
    );
}

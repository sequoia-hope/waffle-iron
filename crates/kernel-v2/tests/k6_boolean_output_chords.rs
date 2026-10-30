//! K6 (remediation 2026-10-28, ledger N100 item 3): PRODUCTION gate on
//! straight edges of curved boolean-output faces.
//!
//! `recover::recover_output_curves` bails to the raw yang lists on any
//! anomaly, and on that path a curved rim that was emitted as `LineSegment`
//! chords used to reach the arena unchecked. The gate refuses any straight
//! edge of a curved face that leaves the face's surface between its
//! endpoints, and accepts the legitimate case — a ruling.
//!
//! Fixture: the quarter-disk sector wedge (`kv12_tier2_arc_extrude`), whose
//! one cylinder patch is bounded by two arcs and two rulings.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::arena::Curve;
use kernel_v2::{
    boolean_op, extrude, validate_boolean_output_chords, BrepArena, KernelV2Error, Profile,
    ProfileEdge, SolidId, Surface,
};

const R: f64 = 2.0;
const H: f64 = 3.0;

fn sector_wedge(a: &mut BrepArena) -> SolidId {
    let o = Point2::new(0.0, 0.0);
    let pa = Point2::new(R, 0.0);
    let pb = Point2::new(0.0, R);
    let p = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line { a: o, b: pa },
            ProfileEdge::Arc {
                a: pa,
                b: pb,
                center: o,
                radius: R,
                ccw: true,
            },
            ProfileEdge::Line { a: pb, b: o },
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), H)
        .unwrap()
        .solid
}

/// Turn one arc of the wedge's cylinder patch (both half-edges) into a
/// straight chord: the shape the recover bail used to ship.
fn chord_one_arc(a: &mut BrepArena, s: SolidId) {
    let shells = a.solid(s).unwrap().shells.clone();
    for sh in shells {
        for f in a.shell(sh).unwrap().faces.clone() {
            let face = a.face(f).unwrap();
            if !matches!(face.surface, Some(Surface::Cylinder { .. })) {
                continue;
            }
            for h in a.loop_half_edges(face.outer_loop).unwrap() {
                if matches!(a.half_edge(h).unwrap().curve, Curve::Arc { .. }) {
                    let tw = a.half_edge(h).unwrap().twin;
                    a.half_edge_mut(h).unwrap().curve = Curve::LineSegment;
                    a.half_edge_mut(tw).unwrap().curve = Curve::LineSegment;
                    return;
                }
            }
        }
    }
    panic!("the sector wedge has a cylinder patch bounded by an arc");
}

/// RED-PHASE PIN: a 90° arc of radius 2 shipped as its chord (sagitta
/// 2·(1 − cos 45°) ≈ 0.59 m) is refused with the typed wall.
#[test]
fn arc_shipped_as_a_chord_is_rejected() {
    let mut a = BrepArena::new();
    let s = sector_wedge(&mut a);
    chord_one_arc(&mut a, s);
    let err = validate_boolean_output_chords(&a, s).expect_err("a chord on a cylinder must reject");
    assert!(
        matches!(err, KernelV2Error::ChordOffCurvedSurface { .. }),
        "expected ChordOffCurvedSurface, got {err:?}"
    );
}

/// The wedge's two rulings are straight edges of the cylinder patch that lie
/// on it: the exactly constructed solid passes.
#[test]
fn rulings_of_a_cylinder_patch_pass() {
    let mut a = BrepArena::new();
    let s = sector_wedge(&mut a);
    validate_boolean_output_chords(&a, s).expect("rulings lie on their cylinder");
}

/// End-to-end: a boolean with a cylinder patch in its output passes the
/// wired gate.
#[test]
fn real_boolean_output_passes_the_wired_gate() {
    let mut a = BrepArena::new();
    let w = sector_wedge(&mut a);
    let p = Profile::new(
        Point3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(-1.0, -1.0),
            Point2::new(3.0, -1.0),
            Point2::new(3.0, 3.0),
            Point2::new(-1.0, 3.0),
        ],
        vec![],
    )
    .unwrap();
    let slab = extrude(&mut a, &p, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .unwrap()
        .solid;
    let out = boolean_op(&mut a, w, slab, BoolOp::Subtract).expect("wedge minus slab");
    validate_boolean_output_chords(&a, out).expect("boolean output passes");
}

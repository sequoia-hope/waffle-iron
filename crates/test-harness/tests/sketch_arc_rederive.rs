//! An arc in a stored sketch must still be an arc after the engine re-derives
//! the sketch's profiles (kernel-v2, exact volume).
//!
//! `Sketch::solved_profiles` is derived data: a writer may legitimately store
//! none, and several paths CLEAR it and re-derive — the rebuild's own
//! `recompute_derived_checked` (every extrude/revolve/pipe arm, `current_sketch`),
//! a reprojection, and `feature_engine::params` when a dimension expression
//! re-solves. If that re-derivation loses the arc record, the kernel receives
//! the loop as a polygon through its vertices and a rounded corner is built as
//! a single CHORD — a different solid, with no error and no warning.
//!
//! The oracle is closed form, not a recording: a 60 × 40 mm plate with one
//! corner rounded at r = 4 mm has cap area
//!
//!   0.060 × 0.040 − r²(1 − π/4) = 2.396566370614359e-3 m²
//!
//! (the rectangle less the corner square, plus back the quarter disc) while
//! the same loop with the arc read as its chord loses the corner triangle
//! r²/2 and has area 2.392e-3 — 1.9e-3 relatively smaller. The cap's area is
//! read off the built face, where it is the analytic one and exact to the last
//! bit, so the two readings are never confusable.
//!
//! The volume is checked too, but at its own tier: `mass_properties` reports
//! `Method::Mesh` for this body, because a PLANAR face with an arc in its rim
//! has no exact integration there and falls to the tessellation (measured on
//! this plate: 6.7e-6 relative under the analytic volume, inside the reported
//! chord bound). That is honest rather than wrong — the answer says which tier
//! it is — and it is why the face area, not the volume, is this test's sharp
//! instrument.

use std::f64::consts::PI;

use feature_engine::types::Operation;
use test_harness::workflow::ModelBuilder;
use uuid::Uuid;
use waffle_types::{kernel::measure::Method, SketchEntity, SolveStatus};

const W: f64 = 0.060;
const H: f64 = 0.040;
const R: f64 = 0.004;
const T: f64 = 0.006;

/// The plate's exact area: the rectangle less what the rounding removes
/// (the corner square minus the quarter disc).
fn exact_area() -> f64 {
    W * H - R * R * (1.0 - PI / 4.0)
}

/// The same loop with the arc read as one straight chord: the rectangle less
/// the right triangle the chord cuts off the corner.
fn chord_area() -> f64 {
    W * H - R * R / 2.0
}

/// A 60 × 40 plate with the corner at (W, H) rounded at radius R, stored the
/// way a script, an agent or the KiCad board writer stores a sketch: entities
/// and nothing derived.
///
/// ids: 1–5 points, 6 the arc centre, 10–14 edges.
fn rounded_plate_sketch() -> waffle_types::Sketch {
    let p = |id: u32, x: f64, y: f64| SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    };
    let line = |id: u32, start_id: u32, end_id: u32| SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    };
    waffle_types::Sketch {
        id: Uuid::new_v4(),
        plane: waffle_types::GeomRef {
            kind: waffle_types::TopoKind::Face,
            anchor: waffle_types::Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: waffle_types::Selector::Role {
                role: waffle_types::Role::EndCapPositive,
                index: 0,
            },
            policy: Default::default(),
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities: vec![
            p(1, 0.0, 0.0),
            p(2, W, 0.0),
            p(3, W, H - R), // tangent point on the right edge
            p(4, W - R, H), // tangent point on the top edge
            p(5, 0.0, H),
            p(6, W - R, H - R), // the arc's centre
            line(10, 1, 2),
            line(11, 2, 3),
            SketchEntity::Arc {
                id: 12,
                center_id: 6,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            line(13, 4, 5),
            line(14, 5, 1),
        ],
        constraints: Vec::new(),
        // Fully constrained by construction: the coordinates ARE the answer,
        // so no solve has to run for the geometry to be what it claims.
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: Default::default(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    }
}

/// What the built plate says about itself: the area of its largest planar
/// face (a cap), how many cylindrical faces it has, and its volume with the
/// tier that volume was computed at.
struct Built {
    cap_area: f64,
    cylinders: usize,
    faces: usize,
    volume: f64,
    method: Method,
}

fn build(sketch: waffle_types::Sketch) -> Built {
    let mut m = ModelBuilder::kernel_v2();
    m.add_operation("sk", Operation::Sketch { sketch })
        .expect("the sketch feature is added");
    m.extrude("plate", "sk", T).expect("the plate extrudes");
    m.assert_no_errors().expect("no engine errors");

    let signatures = m.face_signatures("plate").expect("face signatures");
    let kind = |s: &waffle_types::TopoSignature| s.surface_type.clone().unwrap_or_default();
    let cap_area = signatures
        .iter()
        .filter(|(_, s)| kind(s) == "planar")
        .filter_map(|(_, s)| s.area)
        .fold(0.0, f64::max);
    let cylinders = signatures
        .iter()
        .filter(|(_, s)| kind(s) == "cylindrical")
        .count();

    let handle = m.solid_handle("plate").expect("the plate has a solid");
    let mass = m
        .kernel_ref()
        .as_measure()
        .mass_properties(&handle, None)
        .expect("mass properties of the plate");

    Built {
        cap_area,
        cylinders,
        faces: signatures.len(),
        volume: mass.volume,
        method: mass.method,
    }
}

/// The plate's cap carries the arc, the body has the cylindrical wall the arc
/// stands for, and the volume agrees with the arithmetic inside its own tier's
/// band.
fn assert_rounded(built: &Built, what: &str) {
    let want = exact_area();
    let rel = (built.cap_area - want).abs() / want;
    assert!(
        rel < 1e-12,
        "{what}: the plate's cap measures {:e} m² where the arithmetic says \
         {want:e} ({rel:e} relative). The chord reading of the same loop is \
         {:e}, and {}",
        built.cap_area,
        chord_area(),
        if ((built.cap_area - chord_area()).abs() / chord_area()) < 1e-12 {
            "that is exactly what this measured: the arc reached the kernel as \
             one straight segment"
        } else {
            "this is neither"
        }
    );
    assert_eq!(
        built.cylinders, 1,
        "{what}: the rounded corner must be ONE cylindrical face (the body has \
         {} faces in total)",
        built.faces
    );

    // The volume is the same claim integrated: inside the band the answer
    // reports for itself, and nowhere near the chord reading, whose deficit is
    // 1.9e-3 relative — orders above any tessellation band on this plate.
    let band = match built.method {
        Method::Exact => 0.0,
        // The bound is a length; the volume it bounds is that length over the
        // curved boundary's swept area, so compare generously in the one
        // direction a chord can err: a tessellated arc is always INSIDE its
        // circle, so the mesh volume is low, never high.
        Method::Mesh { chord_bound } => chord_bound * (PI / 2.0 * R) * T / R,
    };
    let err = want * T - built.volume;
    assert!(
        err >= 0.0 && err <= band.max(1e-18),
        "{what}: the plate's volume is {:e} m³ where the cap area × thickness \
         says {:e} — off by {err:e}, outside the {:?} band {band:e}",
        built.volume,
        want * T,
        built.method
    );
}

/// The rebuild's own re-derivation: the stored sketch carries no derived data
/// at all, as a script-, agent- or KiCad-written sketch does, so every profile
/// the kernel sees was built by `recompute_derived_checked`.
#[test]
fn a_stored_arc_extrudes_as_an_arc_and_not_as_its_chord() {
    assert_rounded(&build(rounded_plate_sketch()), "a stored sketch");
}

/// `feature_engine::params::apply_sketch`'s re-derivation. A sketch stored
/// `Unsolved` — which v4 §2.10 explicitly allows a writer to do, and which an
/// agent's `feature_add` produces — is solved on the next rebuild, and that
/// path CLEARS the derived data and rebuilds it. It must rebuild the same
/// thing.
#[test]
fn an_unsolved_sketchs_first_solve_keeps_the_arc() {
    let mut sketch = rounded_plate_sketch();
    sketch.solve_status = SolveStatus::Unsolved;
    let built = build(sketch);
    assert_rounded(&built, "an unsolved sketch's first solve");
}

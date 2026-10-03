//! `Operation::Sweep` (spec `specs/b6_general_sweep.md` S6) on the REAL
//! kernel (kernel-v2), through the wasm-bridge dispatch the app and the
//! agent link use:
//!
//! 1. A rectangle round a CLOSED rounded-rectangle path is one genus-1 ring:
//!    no caps, one lateral per (segment, section edge), exact B-Rep volume
//!    `A · (straights + 2π R_c)` by Pappus (the section is centred on the
//!    path, so `R_c` is the corner radius); the render mesh is watertight
//!    with χ = 0.
//! 2. The same rectangle along the OPEN handlebar path (line → quarter bend
//!    → line) has both caps with their roles and exact volume `A · L`.
//!
//! Both paths start at the START of the FIRST listed entity, which is where
//! the section sketch is placed (the pierce rule).

use std::collections::HashMap;
use std::f64::consts::PI;

use feature_engine::types::*;
use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;
use waffle_types::kernel::KernelSolidHandle;
use waffle_types::*;

/// Section: `W × H` rectangle centred on the path.
const W: f64 = 0.04;
const H: f64 = 0.02;
/// Rounded rectangle: straights along x and y, corner radius.
const LX: f64 = 0.6;
const LY: f64 = 0.2;
const RC: f64 = 0.2;

fn plane_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn pt(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}
fn line(id: u32, s: u32, e: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: s,
        end_id: e,
        construction: true,
    }
}
fn arc(id: u32, c: u32, s: u32, e: u32) -> SketchEntity {
    SketchEntity::Arc {
        id,
        center_id: c,
        start_id: s,
        end_id: e,
        construction: true,
    }
}

fn sketch(
    origin: [f64; 3],
    normal: [f64; 3],
    x_axis: Option<[f64; 3]>,
    entities: Vec<SketchEntity>,
) -> Operation {
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: plane_ref(),
            plane_origin: origin,
            plane_normal: normal,
            plane_x_axis: x_axis,
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            projected: vec![],
            plane_face: None,
        },
    }
}

/// The section, drawn on the plane through `origin` with normal
/// `tangent` (the path's start tangent) and in-plane +u along world +y so
/// the rectangle's edges are parallel / perpendicular to the bend axis z.
fn section_sketch(origin: [f64; 3], tangent: [f64; 3]) -> Operation {
    sketch(
        origin,
        tangent,
        Some([0.0, 1.0, 0.0]),
        vec![
            pt(1, -W / 2.0, -H / 2.0),
            pt(2, W / 2.0, -H / 2.0),
            pt(3, W / 2.0, H / 2.0),
            pt(4, -W / 2.0, H / 2.0),
            SketchEntity::Line {
                id: 5,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 6,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 7,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 8,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
    )
}

/// Closed rounded rectangle on z = 0, CCW: the bottom straight from
/// `(RC, 0)` to `(RC + LX, 0)` is entity 10 (listed first ⇒ the path starts
/// at `(RC, 0)` heading +x), then quarter bends of radius `RC` alternate
/// with the other straights. Entities 10–17, arcs CCW about +z.
fn rounded_rect_sketch() -> Operation {
    let (x0, x1) = (RC, RC + LX);
    let (y0, y1) = (RC, RC + LY);
    sketch(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        // Explicit +x so sketch (u, v) IS world (x, y); the derived basis
        // for a +z normal points +u along world −y.
        Some([1.0, 0.0, 0.0]),
        vec![
            pt(1, x0, 0.0),
            pt(2, x1, 0.0),
            pt(3, x1, y0), // centre of corner 1
            pt(4, x1 + RC, y0),
            pt(5, x1 + RC, y1),
            pt(6, x1, y1), // centre of corner 2
            pt(7, x1, y1 + RC),
            pt(8, x0, y1 + RC),
            pt(9, x0, y1), // centre of corner 3
            pt(20, 0.0, y1),
            pt(21, 0.0, y0),
            pt(22, x0, y0), // centre of corner 4
            line(10, 1, 2),
            arc(11, 3, 2, 4),
            line(12, 4, 5),
            arc(13, 6, 5, 7),
            line(14, 7, 8),
            arc(15, 9, 8, 20),
            line(16, 20, 21),
            arc(17, 22, 21, 1),
        ],
    )
}

/// Handlebar: line (−1,0)→(0,0), CCW quarter about (0,0.3) to (0.3,0.3),
/// line to (0.3,1.3). Entities 10, 11, 12; length `2 + 0.3·π/2`.
fn handlebar_sketch() -> Operation {
    sketch(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        // Explicit +x so sketch (u, v) IS world (x, y); the derived basis
        // for a +z normal points +u along world −y.
        Some([1.0, 0.0, 0.0]),
        vec![
            pt(1, -1.0, 0.0),
            pt(2, 0.0, 0.0),
            pt(3, 0.0, 0.3),
            pt(4, 0.3, 0.3),
            pt(5, 0.3, 1.3),
            line(10, 1, 2),
            arc(11, 3, 2, 4),
            line(12, 4, 5),
        ],
    )
}

fn sweep_op(section: Uuid, path: Uuid, entity_ids: &[u32]) -> Operation {
    Operation::Sweep {
        params: SweepParams {
            sketch_id: section,
            profile_index: 0,
            profile_entity_ids: None,
            region: None,
            path: SweepPathRef::Sketch {
                sketch_id: path,
                entity_ids: entity_ids.to_vec(),
            },
            combine: None,
            targets: None,
        },
    }
}

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

fn exact_volume(b: &ModelBuilder, handle: &KernelSolidHandle) -> f64 {
    b.kernel_ref()
        .as_introspect()
        .solid_volume(handle)
        .expect("exact volume")
}

#[test]
fn rectangle_round_a_closed_rounded_rectangle_is_one_exact_ring() {
    let mut b = ModelBuilder::kernel_v2();
    let path = b.add_operation("Path", rounded_rect_sketch()).unwrap();
    // The section pierces at the start of entity 10, heading +x.
    let section = b
        .add_operation("Section", section_sketch([RC, 0.0, 0.0], [1.0, 0.0, 0.0]))
        .unwrap();
    // Listed out of order after the first: the chain is walked at rebuild.
    b.add_operation(
        "Ring",
        sweep_op(section, path, &[10, 17, 11, 16, 12, 15, 13, 14]),
    )
    .unwrap();
    assert_clean(&b, "ring");
    let handle = b.solid_handle("Ring").unwrap();
    let faces = b.kernel_ref().as_introspect().list_faces(&handle);
    assert_eq!(faces.len(), 8 * 4, "8 segments × 4 section edges, no caps");
    let v = exact_volume(&b, &handle);
    let expected = W * H * (2.0 * LX + 2.0 * LY + 2.0 * PI * RC);
    assert!(
        ((v - expected) / expected).abs() < 1e-9,
        "exact volume {v} ≠ A·(straights + 2πR_c) = {expected}"
    );
    let mesh = b.tessellate("Ring").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 0);
    assert!(chi.passed, "{}", chi.detail);
    assert!(mesh_signed_volume(&mesh) > 0.0, "outward orientation");
    let r = b.op_result("Ring").unwrap();
    assert!(
        r.provenance
            .role_assignments
            .iter()
            .all(|(_, role)| matches!(role, Role::SideFace { .. })),
        "a ring has no caps"
    );
}

#[test]
fn rectangle_along_the_open_handlebar_has_caps_and_exact_volume() {
    let mut b = ModelBuilder::kernel_v2();
    let path = b.add_operation("Path", handlebar_sketch()).unwrap();
    // Entity 10 first ⇒ the path starts at its free end (−1, 0) heading +x.
    let section = b
        .add_operation("Section", section_sketch([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]))
        .unwrap();
    b.add_operation("Bar", sweep_op(section, path, &[10, 11, 12]))
        .unwrap();
    assert_clean(&b, "bar");
    let handle = b.solid_handle("Bar").unwrap();
    let faces = b.kernel_ref().as_introspect().list_faces(&handle);
    assert_eq!(faces.len(), 3 * 4 + 2, "3 segments × 4 edges + 2 caps");
    let v = exact_volume(&b, &handle);
    let expected = W * H * (2.0 + 0.3 * PI / 2.0);
    assert!(
        ((v - expected) / expected).abs() < 1e-9,
        "exact volume {v} ≠ A·L = {expected}"
    );
    let mesh = b.tessellate("Bar").unwrap();
    assert!(oracle::check_watertight_mesh(&mesh).passed);
    assert!(oracle::check_mesh_euler_characteristic(&mesh, 2).passed);
    let r = b.op_result("Bar").unwrap();
    let roles: Vec<&Role> = r
        .provenance
        .role_assignments
        .iter()
        .map(|(_, role)| role)
        .collect();
    assert_eq!(
        roles
            .iter()
            .filter(|r| ***r == Role::EndCapNegative)
            .count(),
        1,
        "{roles:?}"
    );
    assert_eq!(
        roles
            .iter()
            .filter(|r| ***r == Role::EndCapPositive)
            .count(),
        1,
        "{roles:?}"
    );
}

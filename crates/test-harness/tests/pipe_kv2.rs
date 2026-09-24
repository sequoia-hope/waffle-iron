//! `Operation::Pipe` (spec `specs/b2_pipe_sweep.md` checkpoint 2) on the
//! REAL kernel (kernel-v2), through the wasm-bridge dispatch the app and
//! the agent link use:
//!
//! 1. A handlebar path (line → quarter bend → line) sweeps to one solid
//!    with two caps and one lateral per segment whose EXACT B-Rep volume is
//!    `π r² L`; the render mesh is watertight, χ = 2.
//! 2. A hollow pipe has the bore's laterals and annular caps: exact
//!    `π (r² − rᵢ²) L`, χ = 0 (a solid torus).
//! 3. The pipe is a boolean operand: a box cut through its straight lead-in
//!    removes exactly the swallowed length.

use std::collections::HashMap;
use std::f64::consts::PI;

use feature_engine::types::*;
use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;
use waffle_types::kernel::KernelSolidHandle;
use waffle_types::*;

const R: f64 = 0.05;
const RI: f64 = 0.03;
/// Handlebar length: two unit lines + a quarter circle of radius 0.3.
const LEN: f64 = 2.0 + 0.3 * PI / 2.0;

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

/// Handlebar path as construction geometry: line (−1,0)→(0,0), CCW quarter
/// about (0,0.3) to (0.3,0.3), line to (0.3,1.3). Entities 10, 11, 12.
fn handlebar_sketch() -> Operation {
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: plane_ref(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: vec![
                pt(1, -1.0, 0.0),
                pt(2, 0.0, 0.0),
                pt(3, 0.0, 0.3),
                pt(4, 0.3, 0.3),
                pt(5, 0.3, 1.3),
                SketchEntity::Line {
                    id: 10,
                    start_id: 1,
                    end_id: 2,
                    construction: true,
                },
                SketchEntity::Arc {
                    id: 11,
                    center_id: 3,
                    start_id: 2,
                    end_id: 4,
                    construction: true,
                },
                SketchEntity::Line {
                    id: 12,
                    start_id: 4,
                    end_id: 5,
                    construction: true,
                },
            ],
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            projected: vec![],
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
fn handlebar_pipe_is_one_exact_solid() {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Path", handlebar_sketch()).unwrap();
    b.pipe("Pipe", "Path", &[12, 10, 11], R, None).unwrap();
    assert_clean(&b, "pipe");
    let handle = b.solid_handle("Pipe").unwrap();
    let faces = b.kernel_ref().as_introspect().list_faces(&handle);
    assert_eq!(faces.len(), 5, "2 caps + 3 laterals");
    let v = exact_volume(&b, &handle);
    let expected = PI * R * R * LEN;
    assert!(
        ((v - expected) / expected).abs() < 1e-9,
        "exact volume {v} ≠ π r² L = {expected}"
    );
    let mesh = b.tessellate("Pipe").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);
    assert!(mesh_signed_volume(&mesh) > 0.0, "outward orientation");
    // Roles: both caps addressable.
    let r = b.op_result("Pipe").unwrap();
    let roles: Vec<&Role> = r
        .provenance
        .role_assignments
        .iter()
        .map(|(_, r)| r)
        .collect();
    assert!(roles.contains(&&Role::EndCapNegative), "{roles:?}");
    assert!(roles.contains(&&Role::EndCapPositive), "{roles:?}");
}

#[test]
fn hollow_handlebar_pipe_is_a_solid_torus() {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Path", handlebar_sketch()).unwrap();
    b.pipe("Pipe", "Path", &[10, 11, 12], R, Some(RI)).unwrap();
    assert_clean(&b, "hollow pipe");
    let handle = b.solid_handle("Pipe").unwrap();
    let faces = b.kernel_ref().as_introspect().list_faces(&handle);
    assert_eq!(faces.len(), 8, "2 annular caps + 3 outer + 3 bore laterals");
    let v = exact_volume(&b, &handle);
    let expected = PI * (R * R - RI * RI) * LEN;
    assert!(
        ((v - expected) / expected).abs() < 1e-9,
        "exact volume {v} ≠ π (r² − rᵢ²) L = {expected}"
    );
    let mesh = b.tessellate("Pipe").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 0);
    assert!(chi.passed, "{}", chi.detail);
}

#[test]
fn box_cut_through_the_lead_in_removes_exactly_its_length() {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Path", handlebar_sketch()).unwrap();
    b.pipe("Pipe", "Path", &[10, 11, 12], R, None).unwrap();
    assert_clean(&b, "pipe");
    // A box x ∈ [−1.5, −0.5], y ∈ [−0.5, 0.5], z ∈ [−0.5, 0.5]: swallows
    // the first 0.5 of the lead-in (the pipe starts at x = −1).
    b.rect_sketch(
        "Box sketch",
        [0.0, 0.0, -0.5],
        [0.0, 0.0, 1.0],
        -1.5,
        -0.5,
        1.0,
        1.0,
    )
    .unwrap();
    b.extrude_cut("Cut", "Box sketch", 1.0).unwrap();
    assert_clean(&b, "pipe − box");
    let mesh = b.tessellate("Cut").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    let v = mesh_signed_volume(&mesh);
    let expected = PI * R * R * (LEN - 0.5);
    assert!(
        ((v - expected) / expected).abs() < 1e-2,
        "mesh volume {v} vs π r² (L − 0.5) = {expected}"
    );
}

#[test]
fn malformed_path_is_a_loud_feature_error() {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Path", handlebar_sketch()).unwrap();
    // Two disconnected lines.
    b.pipe("Pipe", "Path", &[10, 12], R, None).unwrap();
    let errors = b.engine_errors().to_vec();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].1.contains("connected"), "{}", errors[0].1);
}

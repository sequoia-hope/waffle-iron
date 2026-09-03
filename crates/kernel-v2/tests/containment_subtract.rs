//! Class C of the exact-membership sweep (docs/audits/exact_membership_sweep_
//! 2026_09_03.md): a CUT whose tool contains the whole body must remove
//! everything — the honest answer is [`KernelV2Error::EmptyBooleanResult`]
//! (kernel-v2 has no empty solid). R0034's box came back intact from the
//! subtract of a cylinder that contains it, R0007 / R0027 / R0088 likewise,
//! and these pins were written expecting to go RED — they went green: the
//! kernel answers correctly, planar and curved. The resurrection lived in
//! the feature engine's most-recent-body walk (spec `cut_consumes_body` §7).
//! Kept as the kernel's pin of the containment answer, with the
//! contained-TOOL twin (a cavity) and the intersection as controls.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, KernelV2Error, Profile, RenderMesh,
};

fn box_solid(
    arena: &mut BrepArena,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> kernel_v2::SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box extrude")
        .solid
}

fn cylinder_solid(
    arena: &mut BrepArena,
    center: (f64, f64),
    radius: f64,
    z: (f64, f64),
) -> kernel_v2::SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(center.0, center.1),
        radius,
    )
    .expect("circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("cylinder extrude")
        .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Control: the tool INSIDE the body carves a cavity (a second shell) and
/// the volume drops by the tool's.
#[test]
fn contained_tool_carves_a_cavity() {
    let mut arena = BrepArena::new();
    let body = box_solid(&mut arena, (0.0, 4.0), (0.0, 4.0), (0.0, 4.0));
    let tool = box_solid(&mut arena, (1.0, 2.0), (1.0, 2.0), (1.0, 2.0));
    let out = boolean_op(&mut arena, body, tool, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("box − contained box: {e:?}"));
    let report = validate_solid(&arena, out).expect("cavity validates");
    assert_eq!(report.shells, 2, "the cavity is a second shell");
    let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate"));
    assert!((vol - 63.0).abs() < 1e-9, "64 − 1 = 63, got {vol}");
}

/// The body INSIDE a planar tool (A ⊂ B, all-planar arrangement, no
/// intersection curves): A ∖ B is empty.
#[test]
fn body_inside_a_planar_tool_is_empty_not_intact() {
    let mut arena = BrepArena::new();
    let body = box_solid(&mut arena, (1.0, 2.0), (1.0, 2.0), (1.0, 2.0));
    let tool = box_solid(&mut arena, (0.0, 4.0), (0.0, 4.0), (0.0, 4.0));
    match boolean_op(&mut arena, body, tool, BoolOp::Subtract) {
        Err(KernelV2Error::EmptyBooleanResult) => {}
        Err(e) => panic!("expected EmptyBooleanResult, got {e:?}"),
        Ok(out) => {
            let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate"));
            panic!("box ⊂ box subtract returned a solid of volume {vol} (the body is 1.0; the answer is empty)");
        }
    }
}

/// The body INSIDE a curved tool — R0034's shape: a box on the base plane
/// inside a cylinder of larger radius and greater height.
#[test]
fn body_inside_a_cylinder_tool_is_empty_not_intact() {
    let mut arena = BrepArena::new();
    let body = box_solid(&mut arena, (-1.0, 1.0), (-1.0, 1.0), (0.0, 3.0));
    let tool = cylinder_solid(&mut arena, (0.0, 0.0), 3.0, (0.0, 7.0));
    match boolean_op(&mut arena, body, tool, BoolOp::Subtract) {
        Err(KernelV2Error::EmptyBooleanResult) => {}
        Err(e) => panic!("expected EmptyBooleanResult, got {e:?}"),
        Ok(out) => {
            let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate"));
            panic!("box ⊂ cylinder subtract returned a solid of volume {vol} (the body is 12.0; the answer is empty)");
        }
    }
}

/// And the intersection of the same pair is the body itself.
#[test]
fn body_inside_a_tool_intersects_to_the_body() {
    let mut arena = BrepArena::new();
    let body = box_solid(&mut arena, (1.0, 2.0), (1.0, 2.0), (1.0, 2.0));
    let tool = box_solid(&mut arena, (0.0, 4.0), (0.0, 4.0), (0.0, 4.0));
    let out = boolean_op(&mut arena, body, tool, BoolOp::Intersect)
        .unwrap_or_else(|e| panic!("box ∩ containing box: {e:?}"));
    let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate"));
    assert!(
        (vol - 1.0).abs() < 1e-9,
        "the intersection is the body (1.0), got {vol}"
    );
}

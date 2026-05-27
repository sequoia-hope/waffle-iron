//! End-to-end integration test through the real `mesh_booleans` binary.
//!
//! Self-skips when `CHERCHI2022_BIN` env var doesn't resolve to an
//! existing file. Build per `docs/sidecar/cherchi2022_build_guide.md`.

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{boolean, BRep};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn unit_cube_at(origin: [f64; 3]) -> Mesh {
    let [x, y, z] = origin;
    let verts = vec![
        p(x, y, z),
        p(x + 1.0, y, z),
        p(x + 1.0, y + 1.0, z),
        p(x, y + 1.0, z),
        p(x, y, z + 1.0),
        p(x + 1.0, y, z + 1.0),
        p(x + 1.0, y + 1.0, z + 1.0),
        p(x, y + 1.0, z + 1.0),
    ];
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [0, 4, 7],
        [0, 7, 3],
    ];
    Mesh::new(verts, tris)
}

fn run_op_via_sidecar(op: BoolOp) {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs end_to_end] SKIP: sidecar binary not found");
        return;
    };
    let a = BRep::from_mesh(unit_cube_at([0.0, 0.0, 0.0]));
    let b = BRep::from_mesh(unit_cube_at([0.5, 0.0, 0.0]));
    let result = boolean(&a, &b, op, &sb).expect("yang-rs boolean failed");
    assert!(result.num_verts() > 0, "{op:?} produced 0-vertex BRep");
    assert!(result.num_tris() > 0, "{op:?} produced 0-triangle BRep");
}

#[test]
fn end_to_end_intersect_via_sidecar() {
    run_op_via_sidecar(BoolOp::Intersect);
}

#[test]
fn end_to_end_union_via_sidecar() {
    run_op_via_sidecar(BoolOp::Union);
}

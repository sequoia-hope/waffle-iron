//! End-to-end smoke tests against the real `mesh_booleans` binary.
//!
//! All op tests self-skip when the binary isn't built/available.
//! Set `CHERCHI2022_BIN` or build per `docs/sidecar/cherchi2022_build_guide.md`.

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;

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

fn run_op(op: BoolOp) {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[cherchi-sidecar-rs smoke] SKIP: binary not found; set CHERCHI2022_BIN");
        return;
    };
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.0, 0.0]);
    let result = sb.boolean(&a, &b, op).expect("boolean failed");
    assert!(
        result.num_verts() > 0,
        "{op:?} produced 0-vertex mesh"
    );
    assert!(
        result.num_tris() > 0,
        "{op:?} produced 0-triangle mesh"
    );
}

#[test]
fn smoke_intersection() {
    run_op(BoolOp::Intersect);
}

#[test]
fn smoke_union() {
    run_op(BoolOp::Union);
}

#[test]
fn smoke_subtraction() {
    run_op(BoolOp::Subtract);
}

#[test]
fn smoke_xor() {
    run_op(BoolOp::Xor);
}

#[test]
fn from_env_returns_err_when_binary_missing() {
    // Force a definitely-missing path.
    let saved = std::env::var("CHERCHI2022_BIN").ok();
    std::env::set_var("CHERCHI2022_BIN", "/definitely/not/a/path/that/exists");
    let result = SidecarBoolean::from_env();
    assert!(result.is_err());
    // Restore (best effort; tests don't run in parallel across processes
    // but other tests in this binary might race).
    match saved {
        Some(v) => std::env::set_var("CHERCHI2022_BIN", v),
        None => std::env::remove_var("CHERCHI2022_BIN"),
    }
}

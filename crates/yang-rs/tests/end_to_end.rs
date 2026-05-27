//! End-to-end integration test through the real `mesh_booleans` binary.
//!
//! Self-skips when `CHERCHI2022_BIN` env var doesn't resolve to an
//! existing file. Build per `docs/sidecar/cherchi2022_build_guide.md`.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

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

// ----- PR-YR4: sidecar integration tests for triangle attribution -----

/// Build a unit cube via BRep::new (with topology) at the given origin.
/// 8 vertices, 24 edges (4 per face), 6 quad faces. Each face has its
/// own dedicated edges so that `face.outer_loop` walks the 4 face
/// vertices via edge `start` fields.
fn unit_cube_brep_at(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    // 8 corners
    let verts = vec![
        BRepVertex {
            point: p(x, y, z),
        }, // 0: -x -y -z
        BRepVertex {
            point: p(x + 1.0, y, z),
        }, // 1: +x -y -z
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        }, // 2: +x +y -z
        BRepVertex {
            point: p(x, y + 1.0, z),
        }, // 3: -x +y -z
        BRepVertex {
            point: p(x, y, z + 1.0),
        }, // 4: -x -y +z
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        }, // 5: +x -y +z
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        }, // 6: +x +y +z
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
        }, // 7: -x +y +z
    ];
    // Each face has 4 dedicated edges. Closure builds 4 edges for verts
    // [a, b, c, d] walking a→b→c→d→a.
    let mut edges = Vec::with_capacity(24);
    let mut face_outer_loops = Vec::with_capacity(6);
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z)
        [4, 7, 6, 5], // F1 top (z+1)
        [0, 4, 5, 1], // F2 front (y)
        [1, 5, 6, 2], // F3 right (x+1)
        [2, 6, 7, 3], // F4 back (y+1)
        [3, 7, 4, 0], // F5 left (x)
    ];
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        face_outer_loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: 0.0,
            },
            outer_loop: face_outer_loops[i].clone(),
        })
        .collect();
    BRep::new(verts, edges, faces).expect("unit cube BRep::new failed")
}

#[test]
fn end_to_end_intersect_attribution_has_some_via_sidecar() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs end_to_end] SKIP: sidecar binary not found");
        return;
    };
    let a = unit_cube_brep_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_at([0.5, 0.0, 0.0]);
    let r = boolean(&a, &b, BoolOp::Intersect, &sb).expect("boolean failed");
    let attr = r.triangle_attribution();
    assert_eq!(
        attr.len(),
        r.num_tris(),
        "attribution length must match output triangle count"
    );
    let some_count = (0..attr.len() as u32)
        .filter(|i| attr.lookup(*i).is_some())
        .count();
    assert!(
        some_count > 0,
        "intersection of topologized cubes should yield at least one attributed triangle"
    );
}

#[test]
fn end_to_end_union_attribution_has_none_via_sidecar() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs end_to_end] SKIP: sidecar binary not found");
        return;
    };
    let a = unit_cube_brep_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_at([0.5, 0.0, 0.0]);
    let r = boolean(&a, &b, BoolOp::Union, &sb).expect("boolean failed");
    let attr = r.triangle_attribution();
    assert_eq!(attr.len(), r.num_tris());
    let none_count = (0..attr.len() as u32)
        .filter(|i| attr.lookup(*i).is_none())
        .count();
    assert!(
        none_count > 0,
        "union should yield at least one triangle with new (Intersection) verts → None attribution"
    );
}

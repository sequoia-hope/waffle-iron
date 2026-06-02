//! M1: Stage-1 orientation — the output mesh agrees with each face's
//! analytic outward normal and is `mesh_booleans_inputcheck`-clean.
//!
//! Spec: `specs/yang_m1_stage1_orientation.md`.
//!
//! - **I1 (orientation)**: every output triangle's geometric normal has
//!   positive dot with its source face's `surface.normal`. Pure-Rust.
//! - **I2 (inputcheck-clean)**: the canonical closed solids (unit cube,
//!   tetrahedron) pass all five `inputcheck` axioms. Reference-oracle;
//!   self-skips when the binary is absent.
//! - **I3 (Euler)**: `V − E + F = 2` over the output triangle mesh.
//!   Pure-Rust.
//! - **B3 (degenerate)**: a zero-area face → `YangError::DegenerateFace`.
//!
//! `BRep::new` fan-triangulates each face from its first loop vertex, in
//! face order. For the unit-cube fixture (6 quad faces) that means output
//! triangles `2*f` and `2*f+1` descend from face `f` — used by I1 to map
//! triangle → source face.

use std::time::Duration;

use cad_primitives::{Point3, Vector3};
use cherchi_sidecar_rs::{inputcheck, SidecarError};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Fixtures
// =========================================================================

/// Unit cube via `BRep::new` (with topology) at the given origin, with
/// correct per-face **outward** normals. 8 vertices, 24 edges (4 per
/// face), 6 quad faces. Mirrors the fixture in `end_to_end.rs`.
fn unit_cube_brep_at(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let verts = vec![
        BRepVertex { point: p(x, y, z) },
        BRepVertex {
            point: p(x + 1.0, y, z),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        },
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
        },
    ];
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
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).expect("unit cube BRep::new failed")
}

/// Tetrahedron BRep with vertices v0=(0,0,0), v1=(1,0,0), v2=(0,1,0),
/// v3=(0,0,1) and correct outward normals. Loop winding is intentionally
/// arbitrary (M1's fix canonicalizes it to match the analytic normal).
fn tetrahedron_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        }, // v0
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        }, // v1
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        }, // v2
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
        }, // v3
    ];
    // 4 triangular faces; each face gets 3 dedicated edges walking its
    // listed vertices a→b→c→a. Winding here is arbitrary; the analytic
    // normal is authoritative.
    let face_verts: [[u32; 3]; 4] = [
        [1, 2, 3], // slanted face, normal ∝ (1,1,1)
        [0, 3, 2], // x=0 face, normal (-1,0,0)
        [0, 1, 3], // y=0 face, normal (0,-1,0)
        [0, 2, 1], // z=0 face, normal (0,0,-1)
    ];
    let normals: [Vector3; 4] = [
        Vector3::new(1.0, 1.0, 1.0),
        Vector3::new(-1.0, 0.0, 0.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(0.0, 0.0, -1.0),
    ];
    let mut edges = Vec::with_capacity(12);
    let mut faces = Vec::with_capacity(4);
    for (f, vs) in face_verts.iter().enumerate() {
        let base = edges.len() as u32;
        for i in 0..3 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 3],
                curve: Curve::LineSegment,
            });
        }
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: normals[f],
                d: 0.0,
            },
            outer_loop: vec![base, base + 1, base + 2],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("tetrahedron BRep::new failed")
}

// =========================================================================
// Pure-Rust geometry helpers (test-only oracle math)
// =========================================================================

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn cross(u: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

fn dot(u: [f64; 3], v: [f64; 3]) -> f64 {
    u[0] * v[0] + u[1] * v[1] + u[2] * v[2]
}

/// Geometric normal of a mesh triangle (cross of its two edge vectors,
/// not normalized — only its sign relative to the face normal matters).
fn tri_normal(mesh: &cherchi_rs::Mesh, t: [u32; 3]) -> [f64; 3] {
    let a = mesh.verts[t[0] as usize];
    let b = mesh.verts[t[1] as usize];
    let c = mesh.verts[t[2] as usize];
    cross(sub(b, a), sub(c, a))
}

// =========================================================================
// I1 — orientation (pure-Rust)
// =========================================================================

#[test]
fn i1_cube_triangle_normals_agree_with_face_normals() {
    let b = unit_cube_brep_at([0.0, 0.0, 0.0]);
    let mesh = b.as_mesh();
    assert_eq!(b.num_tris(), 12, "cube fan-triangulates to 12 tris");
    // Fan-triangulation in face order: tris 2*f and 2*f+1 come from face f.
    for (ti, &tri) in mesh.tris.iter().enumerate() {
        let face_idx = ti / 2;
        let Surface::Plane { normal, .. } = b.faces()[face_idx].surface else {
            continue;
        };
        let n = tri_normal(mesh, tri);
        let d = dot(n, normal.as_array());
        assert!(
            d > 0.0,
            "tri {ti} (from face {face_idx}) normal {n:?} must agree with \
             outward face normal {:?}; dot = {d}",
            normal.as_array()
        );
    }
}

#[test]
fn i1_tetrahedron_triangle_normals_agree_with_face_normals() {
    let b = tetrahedron_brep();
    let mesh = b.as_mesh();
    assert_eq!(b.num_tris(), 4, "tetrahedron has 4 triangular faces");
    // One triangle per face, in face order.
    for (ti, &tri) in mesh.tris.iter().enumerate() {
        let Surface::Plane { normal, .. } = b.faces()[ti].surface else {
            continue;
        };
        let n = tri_normal(mesh, tri);
        let d = dot(n, normal.as_array());
        assert!(
            d > 0.0,
            "tetra tri {ti} normal {n:?} must agree with outward face \
             normal {:?}; dot = {d}",
            normal.as_array()
        );
    }
}

// =========================================================================
// I3 — Euler characteristic (pure-Rust)
// =========================================================================

#[test]
fn i3_cube_satisfies_euler_v_minus_e_plus_f() {
    let b = unit_cube_brep_at([0.0, 0.0, 0.0]);
    let mesh = b.as_mesh();
    use std::collections::BTreeSet;
    let mut undirected: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (a, c) = (tri[i], tri[j]);
            let key = if a < c { (a, c) } else { (c, a) };
            undirected.insert(key);
        }
    }
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let e = undirected.len() as i64;
    assert_eq!(v - e + f, 2, "Euler V-E+F: V={v} E={e} F={f}");
}

// =========================================================================
// I2 — inputcheck-clean (reference oracle; self-skips if binary absent)
// =========================================================================

#[test]
fn i2_cube_is_inputcheck_clean() {
    let b = unit_cube_brep_at([0.0, 0.0, 0.0]);
    let report = match inputcheck(b.as_mesh(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[yang-rs m1] SKIP: inputcheck binary not found");
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    assert!(
        report.all_pass(),
        "cube Stage-1 mesh must pass all inputcheck axioms; got {report:?}"
    );
}

#[test]
fn i2_tetrahedron_is_inputcheck_clean() {
    let b = tetrahedron_brep();
    let report = match inputcheck(b.as_mesh(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[yang-rs m1] SKIP: inputcheck binary not found");
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    assert!(
        report.all_pass(),
        "tetrahedron Stage-1 mesh must pass all inputcheck axioms; got {report:?}"
    );
}

// =========================================================================
// B3 — degenerate face → YangError::DegenerateFace
// =========================================================================

#[test]
fn b3_degenerate_face_returns_degenerate_face_error() {
    // Three collinear points → zero-area face → Newell normal magnitude 0.
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(2.0, 0.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Plane {
            // Some nominal normal; the face is geometrically degenerate.
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let err = BRep::new(verts, edges, faces).unwrap_err();
    match err {
        YangError::DegenerateFace { face } => {
            assert_eq!(face, 0, "the single degenerate face is index 0");
        }
        other => panic!("expected DegenerateFace, got {other:?}"),
    }
}

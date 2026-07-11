//! KV6d increment 5b2: a torus operand traverses the WHOLE yang pipeline
//! (Stage 1 tessellation → Stage 2 mesh boolean → Stage 5/6 reassembly) and the
//! surviving/trimmed torus face is reassembled into the output B-Rep. This is
//! the yang-side of torus output recovery; kernel-v2 owns the render-time
//! re-tessellation of the recovered patch.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// A 90° bent tube (partial torus): center origin, axis +z, major R=3, minor
/// r=1. Same fixture as `kv6d_torus_tessellation`. A closed solid (torus band +
/// 2 meridian-plane disk caps).
fn partial_torus_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(4.0, 0.0, 0.0),
        }, // V0 (θ=0, φ=0)
        BRepVertex {
            point: p(0.0, 4.0, 0.0),
        }, // Vα (θ=α, φ=0)
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(3.0, 0.0, 0.0),
                normal: v3(0.0, 1.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 3.0, 0.0),
                normal: v3(1.0, 0.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Torus {
                center: p(0.0, 0.0, 0.0),
                axis_dir: v3(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("partial torus B-Rep")
}

fn box_brep(origin: [f64; 3], size: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let [sx, sy, sz] = size;
    let pts = [
        [x, y, z],
        [x + sx, y, z],
        [x + sx, y + sy, z],
        [x, y + sy, z],
        [x, y, z + sz],
        [x + sx, y, z + sz],
        [x + sx, y + sy, z + sz],
        [x, y + sy, z + sz],
    ];
    let verts: Vec<BRepVertex> = pts
        .iter()
        .map(|&[a, b, c]| BRepVertex { point: p(a, b, c) })
        .collect();
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let normals = [
        v3(0.0, 0.0, -1.0),
        v3(0.0, 0.0, 1.0),
        v3(0.0, -1.0, 0.0),
        v3(1.0, 0.0, 0.0),
        v3(0.0, 1.0, 0.0),
        v3(-1.0, 0.0, 0.0),
    ];
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (i, vs) in face_verts.iter().enumerate() {
        let base = edges.len() as u32;
        for k in 0..4 {
            edges.push(BRepEdge {
                start: vs[k],
                end: vs[(k + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        let n = normals[i];
        let pv = pts[vs[0] as usize];
        let d = -(n.x() * pv[0] + n.y() * pv[1] + n.z() * pv[2]);
        faces.push(BRepFace {
            surface: Surface::Plane { normal: n, d },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: vec![],
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("box brep")
}

fn assert_watertight(mesh: &yang_rs::Mesh, what: &str) {
    let mut ec: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &mesh.tris {
        for (a, c) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let k = if a < c { (a, c) } else { (c, a) };
            *ec.entry(k).or_insert(0) += 1;
        }
    }
    for (e, n) in &ec {
        assert_eq!(*n, 2, "{what}: edge {e:?} shared by {n} tris (not 2)");
    }
}

fn signed_volume(mesh: &yang_rs::Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                + a.z() * (b.x() * c.y() - b.y() * c.x()))
                / 6.0
        })
        .sum()
}

fn has_torus_face(b: &BRep) -> bool {
    b.faces()
        .iter()
        .any(|f| matches!(f.surface, Surface::Torus { .. }))
}

/// A box that bites into the outer wall of the tube near θ=0 (centerline at
/// (3,0,0), outer vertex (4,0,0)) trims the torus lateral into an
/// arbitrary-boundary patch — exactly the cut/boss case that needs torus output
/// recovery.
#[test]
fn torus_minus_box_traverses_pipeline_and_reassembles_torus_face() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    let a = partial_torus_brep();
    let b = box_brep([3.4, -0.6, -0.6], [1.2, 1.2, 1.2]);

    let cut = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend)
        .unwrap_or_else(|e| panic!("torus − box: {e:?}"));
    assert_watertight(cut.as_mesh(), "torus − box");
    assert!(
        has_torus_face(&cut),
        "the trimmed torus lateral must survive as a Surface::Torus face"
    );

    let uni = yang_rs::boolean(&a, &b, BoolOp::Union, &backend)
        .unwrap_or_else(|e| panic!("torus ∪ box: {e:?}"));
    assert_watertight(uni.as_mesh(), "torus ∪ box");
    assert!(
        has_torus_face(&uni),
        "the torus lateral must survive the union as a Surface::Torus face"
    );
}

// M8 torus-profile rim crossing (task #131, spec
// `specs/m8_torus_profile_rim_crossing.md`): the bent tube's θ=0 seam disc
// is flush (same-normal) with a box face whose rectangle CROSSES the
// profile rim. Stage-0 must propagate the rim crossings into the torus
// lateral's profile rings (poloidal opposite-rim projection) — was the
// loud `rim-lateral-none` wall. The contained variant below pins the
// already-green baseline.
#[test]
fn flush_box_crossing_seam_disc_union() {
    let torus = partial_torus_brep();
    let bx = box_brep([2.5, 0.0, -0.4], [2.0, 2.0, 1.0]);
    let nb = yang_rs::native_backend().expect("native backend");
    let out = yang_rs::boolean(&torus, &bx, cad_primitives::BoolOp::Union, &nb)
        .expect("flush crossing union must be handled (task #131)");
    let mesh = out.as_mesh();
    assert_watertight(mesh, "flush crossing union");
    assert!(
        has_torus_face(&out),
        "the torus lateral must survive the union"
    );
    let vol = signed_volume(mesh);
    // 90° tube volume (Pappus): π r² R · π/2 = 3π²/2 ≈ 14.804; box = 4.
    let vt = 3.0 * std::f64::consts::PI * std::f64::consts::PI / 2.0;
    assert!(
        vol > vt && vol < vt + 4.0,
        "union volume {vol} must sit strictly between the tube ({vt}) and tube+box"
    );
}

/// Contained-variant canary: the flush box footprint strictly INSIDE the
/// seam disc (no rim crossing) was already green before task #131 — the
/// disc-pair containment path plus the KV6d downstream. Pins the baseline
/// the crossing slice builds on.
#[test]
fn flush_box_contained_in_seam_disc_union() {
    let torus = partial_torus_brep();
    let bx = box_brep([2.5, 0.0, -0.4], [0.9, 2.0, 0.9]);
    let nb = yang_rs::native_backend().expect("native backend");
    let out = yang_rs::boolean(&torus, &bx, cad_primitives::BoolOp::Union, &nb)
        .expect("contained flush union (pre-#131 baseline)");
    assert_watertight(out.as_mesh(), "contained flush union");
    assert!(has_torus_face(&out), "torus face survives");
}

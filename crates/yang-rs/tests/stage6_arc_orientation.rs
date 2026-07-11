//! Task #133 — Stage-6 arc-edge orientation convention (RED first).
//!
//! yang's Stage-1 input convention for a `Curve::Circle` ARC edge
//! (`start != end`) is "CCW around the stored `normal` from `start` to
//! `end`" (a kernel-v2-converted input always satisfies it as a MINOR arc).
//! `emit_topology` violated it: each directed per-face edge copy takes its
//! FACE-LOOP traversal as (start, end) but copies the intersection curve —
//! normal included — verbatim from the undirected mesh-edge map. The copy
//! whose traversal is CW around that normal then declares the COMPLEMENTARY
//! (≈ full-circle) arc, and a direct yang-chained boolean tessellates the
//! two copies of one geometric arc with wildly different chains → the
//! Stage-0/Stage-1 meshes tear (~90 unbalanced edges at the pocket floor)
//! and the next boolean dies `NonManifoldOutput`.
//!
//! Production is immune (kernel-v2's `from_yang_brep` re-derives minor
//! arcs), so this pins the YANG-LEVEL chain: a boolean output must be a
//! valid boolean INPUT (the m8/kv fixtures chain outputs directly).

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box B-Rep [lo, hi] (yr24/yr26 hexahedron convention).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex { point: p(x, y, z) };
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(s, e)| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("valid box B-Rep")
}

/// Upright cylinder, seam at +x (m8_disc_coplanar convention).
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let v0 = p(cx + radius, cy, base_z);
    let v1 = p(cx + radius, cy, base_z + height);
    let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(cx, cy, base_z),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(cx, cy, base_z + height),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(cx, cy, base_z),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: base_z,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -(base_z + height),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("z_cylinder BRep::new")
}

fn edge_stats(mesh: &Mesh) -> BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> {
    let key = |v: u32| {
        let p = mesh.verts[v as usize];
        [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
    };
    let mut m: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
            let (lo, hi, dir) = if a <= b { (a, b, 1) } else { (b, a, -1) };
            let e = m.entry((lo, hi)).or_insert((0, 0));
            e.0 += 1;
            e.1 += dir;
        }
    }
    m
}

fn assert_watertight(mesh: &Mesh, what: &str) {
    assert!(!mesh.tris.is_empty(), "{what}: output must be non-empty");
    for (edge, (count, balance)) in edge_stats(mesh) {
        assert_eq!(count, 2, "{what}: edge {edge:?} must have 2 incident tris");
        assert_eq!(balance, 0, "{what}: edge {edge:?} once per direction");
    }
}

fn run(a: &BRep, b: &BRep, op: BoolOp, what: &str) -> BRep {
    let nb = yang_rs::native_backend().expect("native backend");
    match boolean(a, b, op, &nb) {
        Ok(out) => out,
        Err(e) => panic!("{what}: boolean() failed: {e}"),
    }
}

/// The partial-depth pocket operand: cylinder r=2 h=2 minus a channel box
/// stopping at z=1 — leaves an interior floor whose r=2 arcs are split
/// across the floor plane and the notched (holed) lateral.
fn pocket_operand() -> BRep {
    let cyl = z_cylinder(0.0, 0.0, 0.0, 2.0, 2.0);
    let channel = box_brep([-0.5, -3.0, 1.0], [0.5, 3.0, 3.0]);
    run(&cyl, &channel, BoolOp::Subtract, "cyl − channel")
}

/// Structural pin: EVERY circle-arc edge of the boolean output satisfies
/// the input convention — CCW sweep around the stored normal from start to
/// end is the MINOR (< π) arc. (Each Stage-6 output arc spans one mesh
/// chord, so the geometric piece is always minor.)
#[test]
fn output_arc_edges_satisfy_ccw_minor_convention() {
    let out = pocket_operand();
    let mut arcs = 0usize;
    for (ei, e) in out.edges().iter().enumerate() {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            continue;
        };
        if e.start == e.end {
            continue;
        }
        arcs += 1;
        let n = normal.as_array();
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let nu = [n[0] / nl, n[1] / nl, n[2] / nl];
        // Deterministic in-plane frame: e1 = any ⟂, e2 = n × e1.
        let pick = if nu[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let d1 = [
            pick[1] * nu[2] - pick[2] * nu[1],
            pick[2] * nu[0] - pick[0] * nu[2],
            pick[0] * nu[1] - pick[1] * nu[0],
        ];
        let l1 = (d1[0] * d1[0] + d1[1] * d1[1] + d1[2] * d1[2]).sqrt();
        let e1 = [d1[0] / l1, d1[1] / l1, d1[2] / l1];
        let e2 = [
            nu[1] * e1[2] - nu[2] * e1[1],
            nu[2] * e1[0] - nu[0] * e1[2],
            nu[0] * e1[1] - nu[1] * e1[0],
        ];
        let c = center.as_array();
        let ang = |vi: u32| {
            let q = out.vertices()[vi as usize].point.as_array();
            let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
            let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
            let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
            y.atan2(x)
        };
        let sweep = (ang(e.end) - ang(e.start)).rem_euclid(2.0 * std::f64::consts::PI);
        assert!(
            sweep < std::f64::consts::PI,
            "edge {ei} (r={radius}): CCW sweep {sweep} ≥ π — arc convention violated \
             (start={:?} end={:?} normal={n:?})",
            out.vertices()[e.start as usize].point,
            out.vertices()[e.end as usize].point,
        );
    }
    assert!(arcs > 0, "operand must carry split arc edges");
}

/// e2e: the pocket operand re-enters a plain (coplanarity-free) boolean —
/// tool sunk strictly below the cap. Was `NonManifoldOutput` (the torn
/// Stage-1 chains), must build watertight.
#[test]
fn pocket_operand_reenters_plain_boolean() {
    let solid = pocket_operand();
    let tool = z_cylinder(0.0, 0.0, 1.3, 1.0, 0.5);
    let out = run(&solid, &tool, BoolOp::Subtract, "pocket − sunk tool");
    assert_watertight(out.as_mesh(), "pocket − sunk tool");
}

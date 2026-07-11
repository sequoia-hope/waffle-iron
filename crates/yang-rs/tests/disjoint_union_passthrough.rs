//! Task #134 — disjoint-union passthrough (spec
//! `specs/yang_disjoint_union_passthrough.md`). RED first.
//!
//! A union of two AABB-disjoint solids used to run the full mesh pipeline
//! and re-emit every untouched rim as a LineSegment chord polyline — the
//! output carried NO `Curve::Circle` vocabulary, so a LATER boolean with a
//! cylinder-owning intersection edge died at the Stage-3 producer fault
//! `AmbiguousCurve { candidates: 0, matched: 0 }` (no rim to derive the
//! chord bound from). The disjoint union is now the concatenated B-Rep —
//! every curve tag preserved bit-for-bit.

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Upright cylinder (axis +z), seam at +x (m8_disc_coplanar convention).
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let bottom = [cx, cy, base_z];
    let top = [cx, cy, base_z + height];
    let v0 = p(cx + radius, cy, base_z);
    let v1 = p(cx + radius, cy, base_z + height);
    let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom[0], bottom[1], bottom[2]),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top[0], top[1], top[2]),
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
                axis_point: p(bottom[0], bottom[1], bottom[2]),
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

#[test]
fn disjoint_union_preserves_circle_vocabulary() {
    let a = z_cylinder(-1.5, 0.0, 0.0, 1.0, 1.0);
    let b = z_cylinder(1.5, 0.0, 0.0, 1.0, 1.0);
    let nb = yang_rs::native_backend().expect("native backend");
    let out = boolean(&a, &b, BoolOp::Union, &nb).expect("disjoint union");
    let closed_circles = out
        .edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Circle { .. }) && e.start == e.end)
        .count();
    assert_eq!(
        closed_circles, 4,
        "disjoint union must keep BOTH inputs' exact rim circles (got {closed_circles})"
    );
    // Volume = exact sum of the two tessellated inputs (bit-identical
    // meshes through the passthrough).
    let vol = |m: &yang_rs::Mesh| -> f64 {
        m.tris
            .iter()
            .map(|t| {
                let a = m.verts[t[0] as usize];
                let b = m.verts[t[1] as usize];
                let c = m.verts[t[2] as usize];
                (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                    + a.z() * (b.x() * c.y() - b.y() * c.x()))
                    / 6.0
            })
            .sum()
    };
    // The concatenated solid re-tessellates at ITS OWN scale-relative
    // chord bound (a larger extent picks a coarser rim N), so compare
    // against the ANALYTIC sum within the chord band, not the inputs'
    // meshes bit-for-bit.
    let v = vol(out.as_mesh());
    let expect = 2.0 * std::f64::consts::PI; // two r=1 h=1 cylinders
    assert!(
        (v - expect).abs() <= expect * 0.08,
        "union volume {v} must be the analytic sum ~{expect} within the chord band"
    );
    // Watertight (both lumps closed).
    let mut ec: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
    for t in &out.as_mesh().tris {
        for (x, y) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let k = if x < y { (x, y) } else { (y, x) };
            *ec.entry(k).or_insert(0) += 1;
        }
    }
    assert!(ec.values().all(|&n| n == 2), "concatenated mesh watertight");
}

#[test]
fn disjoint_union_output_reenters_boolean() {
    let a = z_cylinder(-1.5, 0.0, 0.0, 1.0, 1.0);
    let b = z_cylinder(1.5, 0.0, 0.0, 1.0, 1.0);
    let nb = yang_rs::native_backend().expect("native backend");
    let lumps = boolean(&a, &b, BoolOp::Union, &nb).expect("disjoint union");
    // A third cylinder overlapping lump A — the chained boolean needs the
    // lumps' Circle rims for the Stage-3 chord bound (was the producer
    // fault AmbiguousCurve{0,0}).
    let c = z_cylinder(-1.5, 0.6, 0.5, 1.0, 1.0);
    let out =
        boolean(&lumps, &c, BoolOp::Union, &nb).expect("chained union on the disjoint-sum output");
    assert!(!out.as_mesh().tris.is_empty());
}

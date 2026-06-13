//! PR-KV11 — ellipse×ellipse JUNCTION at a box edge (cylinder+plane arm).
//!
//! An oblique cylinder piercing a box EDGE produces two cylinder∩plane
//! Ellipse sections (one per adjacent box face) that MEET at junction
//! vertices on that edge. Stage 4 must land each junction vertex on BOTH
//! ellipses — the exact junction is `(plane₁ ∩ plane₂) ∩ cylinder`, the
//! same closed form PR-KV9 added for the Steinmetz cyl×cyl crossing.
//!
//! The PR-KV9 junction detection lived only in the cylinder×cylinder arm of
//! the Stage-4 conic-endpoint scan; the cylinder+plane arm silently
//! OVERWROTE `vert_ellipse` when a vertex was the endpoint of two different
//! cylinder+plane ellipses, so the vertex was relocated onto the LAST
//! ellipse scanned and stayed off the first by the Stage-1 chord error
//! (~1e-4 at unit scale — five orders past kernel-v2's 1e-9 import band).
//! Corpus class: F0046–F0050, R0041, R0095, F0076 — "output ellipse-arc
//! endpoint does not lie on its ellipse".
//!
//! Fixture: unit box [0,1]³ ∪ cylinder (r=0.25, axis dir (1,1,3)/√11)
//! whose axis crosses the box's back-top edge (y=1, z=1) at (0.6, 1, 1).
//! Both adjacent faces cut the lateral in Ellipse sections; the two arcs
//! meet at two junction points on the edge, x = 0.6 ± s.

use std::collections::{HashMap, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3, TAU_MODEL};
use cherchi_rs::Mesh;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// Pure-Rust array math, re-declared verbatim from tests/yr11_stage4_ellipse.rs.
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// Cylinder B-Rep fixture (seam-edge encoding), re-declared from yr11.
fn cylinder_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let v0 = add(bottom_center, scale(e1, radius));
    let v1 = add(top_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];

    let neg_axis = scale(axis_unit, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let bottom_d = -dot(neg_axis, bottom_center);
    let top_d = -dot(axis_unit, top_center);

    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate the cylinder")
}

// Unit-cube fixture with true per-face plane offsets, re-declared from yr11.
fn unit_cube_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 1.0, 1.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 1.0),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let mut edges = Vec::with_capacity(24);
    let mut loops = Vec::with_capacity(6);
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [0.0, -1.0, 0.0, -1.0, -1.0, 0.0];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).expect("unit cube BRep::new failed")
}

// Mesh oracles, re-declared from yr11.
fn unpaired_half_edges(mesh: &Mesh) -> usize {
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let e = edges.len() as i64;
    v - e + f
}

/// Residual of `pt` against a stored Ellipse curve: out-of-plane distance
/// plus the in-plane implicit residual scaled back to length by the minor
/// radius — the SAME metric kernel-v2's import validation applies.
fn ellipse_residual(pt: [f64; 3], e: &BRepEdge) -> f64 {
    let Curve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = e.curve
    else {
        panic!("ellipse_residual on a non-Ellipse edge");
    };
    let n = unit(normal.as_array());
    let m = unit(major_axis.as_array());
    let w = cross(n, m);
    let d = sub(pt, center.as_array());
    let out_of_plane = dot(d, n);
    let u = dot(d, m) / major_radius;
    let v = dot(d, w) / minor_radius;
    out_of_plane.abs() + ((u.hypot(v) - 1.0).abs() * minor_radius)
}

/// Box ∪ oblique cylinder through the back-top edge: every output Ellipse
/// edge endpoint must lie on ITS OWN stored ellipse. The two junction
/// vertices on the edge (y=1, z=1) are endpoints of ellipses in BOTH the
/// top-face and back-face planes; before PR-KV11 the cylinder+plane arm
/// relocated them onto only the last-scanned ellipse.
#[test]
fn junction_endpoints_lie_on_both_ellipses() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[kv11] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    // Axis dir (1,2,3): a STEEP y-tilt so the lateral crosses the back face
    // (y=1) deeply (penetration ≫ the Stage-1 chord sagitta — a [1,1,3] tilt
    // grazes y=1 at chord depth, a tangency-class fixture, NOT this class).
    let u3 = unit([1.0, 2.0, 3.0]);
    // Axis crosses the (y=1, z=1) edge at (0.6, 1, 1); height 1.2 keeps the
    // bottom cap strictly inside the box and the top cap strictly outside.
    let axis_point = add([0.6, 1.0, 1.0], scale(u3, -0.9));
    let cyl = cylinder_brep(axis_point, [1.0, 2.0, 3.0], 0.25, 1.2);
    let bx = unit_cube_brep();

    let r = boolean(&bx, &cyl, BoolOp::Union, &sb).expect("kv11: box ∪ edge-piercing cylinder");

    // The output solid must include the BACK bulge (cylinder material beyond
    // y=1 below z=1) — the §4.5.3 junction false-reversal collapsed it away
    // entirely (watertight but missing material) before PR-KV11.
    {
        let m = r.as_mesh();
        let mut back_bulge = 0usize;
        for tri in &m.tris {
            let a3 = m.verts[tri[0] as usize].as_array();
            let b3 = m.verts[tri[1] as usize].as_array();
            let c3 = m.verts[tri[2] as usize].as_array();
            let cy = (a3[1] + b3[1] + c3[1]) / 3.0;
            let cz = (a3[2] + b3[2] + c3[2]) / 3.0;
            if cy > 1.0 + 1e-6 && cz < 1.0 - 1e-6 {
                back_bulge += 1;
            }
        }
        assert!(
            back_bulge > 0,
            "kv11: output mesh has NO triangles on the back bulge (y>1, z<1) — \
             the cylinder material beyond the back face was dropped"
        );
    }
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "kv11: output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "kv11: Euler must be 2"
    );

    // The fixture must actually exercise the junction: ellipse sections in
    // BOTH adjacent face planes (normals ±z and ±y).
    let verts = r.vertices();
    let mut planes_seen: HashSet<char> = HashSet::new();
    let mut checked = 0usize;
    let mut on_edge_endpoints: HashSet<u32> = HashSet::new();
    for e in r.edges() {
        let Curve::Ellipse { normal, .. } = e.curve else {
            continue;
        };
        let n = unit(normal.as_array());
        if n[2].abs() > 0.99 {
            planes_seen.insert('z');
        }
        if n[1].abs() > 0.99 {
            planes_seen.insert('y');
        }
        for v in [e.start, e.end] {
            let pt = verts[v as usize].point.as_array();
            let rho = ellipse_residual(pt, e);
            assert!(
                rho <= TAU_MODEL,
                "kv11: ellipse-edge endpoint v{v} {pt:?} off its ellipse by {rho:.3e} \
                 (curve {:?})",
                e.curve
            );
            if (pt[1] - 1.0).abs() <= TAU_MODEL && (pt[2] - 1.0).abs() <= TAU_MODEL {
                on_edge_endpoints.insert(v);
            }
            checked += 1;
        }
    }
    assert!(
        planes_seen.contains(&'z') && planes_seen.contains(&'y'),
        "kv11: expected ellipse sections in BOTH the top (z) and back (y) face \
         planes; saw {planes_seen:?} — a junction vertex relocated onto only one \
         ellipse swallows the other face's arc"
    );
    // The two true junctions (cylinder ∩ box-edge line) sit at y=1, z=1.
    assert!(
        on_edge_endpoints.len() >= 2,
        "kv11: expected ≥2 ellipse endpoints ON the box edge (y=1, z=1) — the \
         exact junctions (plane₁∩plane₂)∩cylinder; found {on_edge_endpoints:?}"
    );
    assert!(
        checked >= 4,
        "kv11 fixture: too few ellipse endpoints ({checked})"
    );
}

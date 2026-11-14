//! Yang §4.3 curve-interior domain certificate (trigger T9,
//! `stage4_correct::curve_domain_postcondition`) — the F0076 site, moved
//! rigidly (measured 2026-11-14, `docs/yang_tail_triage.md`).
//!
//! A cylinder stands on a box top, its axis 2.448° off the top's normal, its
//! radius 0.66 mm larger than the box's half-width. On the side where the
//! base disc dips below the box top, a 12°-wide arc of the base rim lies
//! outside the box's side wall: a sliver of the base disc (1.058e-5 m²) and a
//! strip of the lateral face are part of the union's boundary.
//!
//! The Stage-1 junction mints (ledger N82) place rim samples exactly where
//! the base circle meets the wall plane. When no regular rim sample falls
//! between those two mints, the chord joining them lies IN the wall plane:
//! the meshes touch there and never cross, the arrangement types the chord as
//! wall ∩ cylinder, and that arc runs 41 mm below the base rim, where the
//! cylinder face does not exist. Without the certificate the union came back
//! closed, on-surface and with nine faces — the sliver absent, and material
//! under it that neither operand has — for half of the rim's 30° phases.
//!
//! Integration-test files cannot share helpers; the constructors are copies
//! of the established hand-built patterns (`n2_rim_mint_adversary.rs`).

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
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

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let verts = vec![
        BRepVertex {
            point: p(x0, y0, z0),
        },
        BRepVertex {
            point: p(x1, y0, z0),
        },
        BRepVertex {
            point: p(x1, y1, z0),
        },
        BRepVertex {
            point: p(x0, y1, z0),
        },
        BRepVertex {
            point: p(x0, y0, z1),
        },
        BRepVertex {
            point: p(x1, y0, z1),
        },
        BRepVertex {
            point: p(x1, y1, z1),
        },
        BRepVertex {
            point: p(x0, y1, z1),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (−z)
        [4, 7, 6, 5], // top (+z)
        [0, 4, 5, 1], // front (−y)
        [1, 5, 6, 2], // right (+x)
        [2, 6, 7, 3], // back (+y)
        [3, 7, 4, 0], // left (−x)
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
    let offs = [z0, -z1, y0, -x1, -y1, x0];
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
    BRep::new(verts, edges, faces).expect("box_brep: BRep::new failed")
}

/// Closed solid cylinder (seam-edge encoding). Stage 1 takes the seam vertex
/// as its first rim sample, so `phase_deg` — the seam's azimuth about the
/// axis — sets where the regular rim samples fall.
fn cylinder_brep(
    base_center: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
    phase_deg: f64,
) -> BRep {
    let axis = unit(axis_dir);
    let top_center = add(base_center, scale(axis, height));
    let u = unit(cross(axis, [1.0, 0.0, 0.0]));
    let v = cross(axis, u);
    let (c, s) = (phase_deg.to_radians().cos(), phase_deg.to_radians().sin());
    let e1 = add(scale(u, c), scale(v, s));
    let v0 = add(base_center, scale(e1, radius));
    let v1 = add(top_center, scale(e1, radius));
    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];
    let neg = scale(axis, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(base_center[0], base_center[1], base_center[2]),
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis[0], axis[1], axis[2]),
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
                axis_point: p(base_center[0], base_center[1], base_center[2]),
                axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                d: -dot(neg, base_center),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis[0], axis[1], axis[2]),
                d: -dot(axis, top_center),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new failed")
}

// F0076's third operation in the second box's own frame (the box top at
// z = DEPTH, centred on the origin; lengths in metres, from the document).
const HALF_X: f64 = 0.114_629_883_864_957_29;
const HALF_Y: f64 = 0.126_494_443_168_207_38;
const DEPTH: f64 = 0.177_051_792_849_347_63;
const RADIUS: f64 = 0.115_293_319_946_664_7;
const HEIGHT: f64 = 0.299_407_598_498_290_8;

/// The cylinder's axis: 0.902° toward the +x wall, 2.276° toward +y (2.448°
/// off the top's normal in all).
fn axis() -> [f64; 3] {
    let (ax, ay) = (0.902_f64.to_radians().sin(), 2.276_f64.to_radians().sin());
    unit([ax, ay, (1.0 - ax * ax - ay * ay).sqrt()])
}

fn base_center() -> [f64; 3] {
    [0.0, 0.0, DEPTH]
}

fn union_at(phase_deg: f64) -> Result<BRep, YangError> {
    let a = box_brep([-HALF_X, -HALF_Y, 0.0], [HALF_X, HALF_Y, DEPTH]);
    let b = cylinder_brep(base_center(), axis(), RADIUS, HEIGHT, phase_deg);
    let backend = yang_rs::native_backend().expect("native backend");
    boolean(&a, &b, BoolOp::Union, &backend)
}

/// Height of `q` above the cylinder's base plane, along its axis.
fn above_base(q: Point3) -> f64 {
    let (q, c, n) = (q.as_array(), base_center(), axis());
    dot([q[0] - c[0], q[1] - c[1], q[2] - c[2]], n)
}

/// What the union must be at every rim phase: ten faces — the box's six, the
/// cylinder's lateral and top, and the base disc as TWO faces (the half
/// above the box top, and the sliver outside the wall) — and no edge typed
/// as a plane ∩ cylinder ellipse whose two ends both sit on the base rim
/// (between two rim points the boundary is the rim or the base chord; an
/// ellipse there is the arc that leaves the cylinder face).
fn assert_union_is_the_solid(out: &BRep, phase_deg: f64) {
    let n = axis();
    let base_faces = out
        .faces()
        .iter()
        .filter(|f| match f.surface {
            Surface::Plane { normal, d } => {
                let m = unit(normal.as_array());
                let c = base_center();
                (dot(m, n).abs() - 1.0).abs() < 1e-9
                    && (dot(m, c) + d / dot(normal.as_array(), normal.as_array()).sqrt()).abs()
                        < 1e-9
            }
            _ => false,
        })
        .count();
    assert_eq!(
        base_faces, 2,
        "phase {phase_deg}°: the base disc survives as the exposed half and the sliver outside \
         the wall"
    );
    assert_eq!(out.faces().len(), 10, "phase {phase_deg}°: face count");
    for e in out.edges() {
        if !matches!(e.curve, Curve::Ellipse { .. }) {
            continue;
        }
        let (s, t) = (
            above_base(out.vertices()[e.start as usize].point),
            above_base(out.vertices()[e.end as usize].point),
        );
        assert!(
            s.abs() > 1e-9 || t.abs() > 1e-9,
            "phase {phase_deg}°: an ellipse edge joins two base-rim points ({s:.3e}, {t:.3e})"
        );
    }
}

/// Every 5° of the rim's phase (the regular samples repeat every 30°; before
/// the certificate the phases with no regular sample in the 12° window —
/// half of them — returned the nine-face body).
#[test]
fn union_holds_the_sliver_at_every_rim_phase() {
    for k in 0..72 {
        let phase = 5.0 * f64::from(k);
        let out = union_at(phase).unwrap_or_else(|e| panic!("phase {phase}°: {e}"));
        assert_union_is_the_solid(&out, phase);
    }
}

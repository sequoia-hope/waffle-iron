//! N2/F0059 epic increment 2 — Stage-1 cap-rim junction insertion
//! (spec `specs/yang_rim_junction_insertion.md`, task #122).
//!
//! The truncated Steinmetz pair (equal-radius perpendicular cylinders with
//! h/2 < r — the F0059 shape) is the class fixture: the caps TRUNCATE the
//! intersection seam, so each cap disc keeps four circular-segment lobes
//! whose corners are exact rim junction points. Today the chord-sampled rim
//! bypasses those junctions and the op stops loudly in Stage 4 (the
//! over-determined ellipse×circle junction audit).
//!
//! RED (live pin): the union stops `Stage4RegionInvalid` /
//! `LocalRefinementRequired` — the documented wall. If this pin starts
//! failing, the wall MOVED: re-measure before touching the epic plan.
//! GREEN (ignored target): after increments 2 (rim junction insertion) and
//! 3 (Stage-4 exactness escape for over-determined junctions) land, the
//! union completes watertight with the exact truncated-Steinmetz volume.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
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
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// Cylinder B-Rep fixture (seam-edge encoding), re-declared verbatim from
// tests/kv9f1_tangency_junction.rs / kv11.
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

    BRep::new(verts, edges, faces).expect("cylinder fixture builds")
}

/// The F0059 shape: equal-radius perpendicular cylinders, axes crossing at
/// each other's midpoints, with h/2 < r so BOTH caps truncate the seam.
fn truncated_steinmetz_pair(r: f64, h: f64) -> (BRep, BRep) {
    assert!(h / 2.0 < r, "class fixture demands cap-truncated seam");
    let a = cylinder_brep([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h);
    let b = cylinder_brep([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h);
    (a, b)
}

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    let mut counts: std::collections::HashMap<(u32, u32), i64> = std::collections::HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0usize;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    let mut six_v = 0.0f64;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        six_v += dot(a, cross(b, c));
    }
    six_v / 6.0
}

/// Exact common volume of the box-truncated bicylinder: both cylinders
/// radius r, axes x and y, extents |x| ≤ h/2 and |y| ≤ h/2, with h/2 < r.
/// Slicing at height z: the section is the square
/// |x| ≤ min(√(r²−z²), h/2) × |y| ≤ min(√(r²−z²), h/2); the switch radius
/// is z0 = √(r² − h²/4).
fn truncated_bicylinder_volume(r: f64, h: f64) -> f64 {
    let z0 = (r * r - h * h / 4.0).sqrt();
    2.0 * z0 * h * h + 8.0 * (2.0 * r.powi(3) / 3.0 - r * r * z0 + z0.powi(3) / 3.0)
}

/// RED live pin: the documented wall TODAY is the Stage-4 over-determined
/// ellipse×circle junction audit (`LocalRefinementRequired`) — the seam
/// ellipses meet the cap rims at the four lobes' corners and Stage 4
/// refuses to silently pick a curve. If the failure ever changes shape,
/// this pin fails and the epic plan must be re-measured first (P10).
#[test]
fn truncated_steinmetz_union_stops_at_stage4_overdetermined_junction() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = truncated_steinmetz_pair(0.35, 0.5);
    match boolean(&a, &b, BoolOp::Union, &sb) {
        Ok(_) => panic!(
            "rim-junction: truncated Steinmetz union UNEXPECTEDLY completed — \
             the increment-2 wall moved; re-measure and update the spec/pins"
        ),
        Err(YangError::Stage4RegionInvalid { reason, .. }) => {
            assert_eq!(
                format!("{reason:?}"),
                "LocalRefinementRequired",
                "rim-junction: expected the over-determined junction STOP"
            );
        }
        Err(other) => panic!(
            "rim-junction: expected Stage4RegionInvalid(LocalRefinementRequired), \
             the wall moved to {other:?} — re-measure (P10)"
        ),
    }
}

/// GREEN target (un-ignore when increments 2+3 land): the union completes
/// watertight with the exact truncated-Steinmetz volume
/// V = 2·πr²h − V_common(r, h).
#[test]
#[ignore = "N2/F0059 epic increments 2+3 (rim junction insertion + Stage-4 exactness escape) — task #122"]
fn truncated_steinmetz_union_green_target() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r, h) = (0.35f64, 0.5f64);
    let (a, b) = truncated_steinmetz_pair(r, h);
    let out = boolean(&a, &b, BoolOp::Union, &sb)
        .unwrap_or_else(|e| panic!("rim-junction green target: union failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "rim-junction: union output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = 2.0 * std::f64::consts::PI * r * r * h - truncated_bicylinder_volume(r, h);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "rim-junction: union volume {vol} vs analytic {expect} (chord \
         under-fill band only)"
    );
}

//! KV9-F1 Increment 0c — Stage-4 tangency ellipse-junction relocation
//! (spec `specs/kv9_f1_tangency_inout_labels.md` §2c, branch rows J1/J2).
//!
//! The Steinmetz pair (equal-radius perpendicular intersecting-axes
//! cylinders) decomposes its intersection into TWO ellipses that cross at
//! the surface-tangency points (0, ±r, 0). The mesh vertex at each crossing
//! is the PINCH of the two faceted-surface intersection polylines; its
//! standoff from the exact junction is second-order-controlled
//! (√(2r·B), B = combined chord budget), NOT first-order — the
//! `vert_ell_junction` gate's 2·d_ε/|d̂·r̂| line metric (correct for the
//! KV11 box-edge class) rejects it as `OffCurveBeyondChordBand` even
//! though the vertex is exactly where Stage-1 chord error puts a tangency
//! pinch. Fix E-L2: gate same-pair cyl×cyl junctions against the derived
//! tangency band √(2rB) + B; relocation target stays the EXACT junction.
//!
//! RED (pre-fix): both ops stop at
//! `Stage4RegionInvalid { vertex: 41, OffCurveBeyondChordBand }`.
//! GREEN: subtract completes through yang-rs with the exact-volume oracle;
//! union progresses past Stage 4 (its own next wall is the NAMED Stage-6
//! boundary-walk item, spec §2c.5a — asserted to not be Stage-4).

use std::collections::HashMap;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// Pure-Rust array math, re-declared verbatim from tests/kv11_ellipse_edge_junction.rs.
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

// Cylinder B-Rep fixture (seam-edge encoding), re-declared from kv11.
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

/// Watertightness oracle, re-declared from kv11.
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

/// Signed volume of a closed triangle mesh (divergence theorem).
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

/// The two equal-radius perpendicular intersecting-axes cylinders of the
/// kernel-v2 kv9 SUBTRACT fixture (r = 0.2, h = 0.9, axes crossing at the
/// origin) — the same operands that stop RED at
/// `Stage4RegionInvalid { OffCurveBeyondChordBand }` today.
fn steinmetz_pair(r: f64, h: f64) -> (BRep, BRep) {
    let a = cylinder_brep([0.0, 0.0, -h / 2.0], [0.0, 0.0, 1.0], r, h);
    let b = cylinder_brep([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h);
    (a, b)
}

fn is_stage4_off_curve(e: &YangError) -> bool {
    matches!(
        e,
        YangError::Stage4RegionInvalid {
            reason: yang_rs::Stage4InvalidReason::OffCurveBeyondChordBand,
            ..
        }
    )
}

/// J1 GREEN contract (subtract): the op completes through yang-rs and the
/// output satisfies the exact Steinmetz volume oracle
/// V = πr²h − 16r³/3 (cylinder minus the bicylinder common volume).
#[test]
fn steinmetz_subtract_passes_stage4_with_volume_oracle() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[kv9f1] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r, h) = (0.2f64, 0.9f64);
    let (a, b) = steinmetz_pair(r, h);
    let out = boolean(&a, &b, BoolOp::Subtract, &sb).unwrap_or_else(|e| {
        panic!(
            "kv9f1: steinmetz subtract must clear yang-rs (Stage-4 tangency \
             junction, spec §2c row J1); failed with {e:?}"
        )
    });
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "kv9f1: subtract output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = std::f64::consts::PI * r * r * h - 16.0 * r * r * r / 3.0;
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "kv9f1: subtract volume {vol} vs analytic {expect} (chord under-fill \
         band only)"
    );
}

/// J1 GREEN contract (union): Stage 4 must accept the tangency junction —
/// the op must NOT stop `Stage4RegionInvalid`. Its own remaining wall is
/// the NAMED Stage-6 boundary-walk item (spec §2c.5a), asserted here as
/// "anything but a Stage-4 stop" so this test stays green when that
/// increment lands and turns the op Ok.
#[test]
fn steinmetz_union_progresses_past_stage4() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[kv9f1] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r, h) = (0.3f64, 1.2f64);
    let (a, b) = steinmetz_pair(r, h);
    match boolean(&a, &b, BoolOp::Union, &sb) {
        Ok(out) => {
            // Bonus: if the Stage-6 walk item has landed, hold the full oracle.
            assert_eq!(
                unpaired_half_edges(out.as_mesh()),
                0,
                "kv9f1: union output must be watertight"
            );
            let vol = mesh_signed_volume(out.as_mesh());
            let v_cyl = std::f64::consts::PI * r * r * h;
            let expect = 2.0 * v_cyl - 16.0 * r * r * r / 3.0;
            assert!(
                vol <= expect * 1.005 && vol >= 0.90 * expect,
                "kv9f1: union volume {vol} vs analytic {expect}"
            );
        }
        Err(e) => {
            assert!(
                !matches!(e, YangError::Stage4RegionInvalid { .. }),
                "kv9f1: steinmetz union must progress past Stage 4 (spec §2c \
                 row J1); stopped at {e:?}"
            );
        }
    }
}

/// J1 RED anchor (kept as the regression pin once GREEN): the specific
/// pre-fix stop was `OffCurveBeyondChordBand` at the tangency pinch. This
/// test documents that NEITHER op reports that reason ever again for the
/// Steinmetz operands.
#[test]
fn steinmetz_never_rejects_off_curve_beyond_chord_band() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[kv9f1] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    for (r, h, op) in [
        (0.2f64, 0.9f64, BoolOp::Subtract),
        (0.3f64, 1.2f64, BoolOp::Union),
    ] {
        let (a, b) = steinmetz_pair(r, h);
        if let Err(e) = boolean(&a, &b, op, &sb) {
            assert!(
                !is_stage4_off_curve(&e),
                "kv9f1: {op:?} (r={r}) rejected the tangency junction as \
                 OffCurveBeyondChordBand — the first-order line metric is the \
                 wrong gate at a same-pair cyl×cyl junction (spec §2c)"
            );
        }
    }
}

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

/// Regression pin (was the increment-2 RED pin): the Stage-4
/// over-determined junction STOP (`LocalRefinementRequired` at the lobe
/// corners) must NEVER return for the truncated-Steinmetz operands — the
/// increment-2 rim insertion + increment-3 exactness certificate resolve
/// those junctions structurally.
#[test]
fn truncated_steinmetz_union_never_stops_at_stage4_junction() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = truncated_steinmetz_pair(0.35, 0.5);
    if let Err(e) = boolean(&a, &b, BoolOp::Union, &sb) {
        assert!(
            !matches!(e, YangError::Stage4RegionInvalid { .. }),
            "rim-junction: the Stage-4 junction STOP returned: {e:?}"
        );
    }
}

// ── Increment 4: the cone-hyperbola junction class ───────────────────────
// Spec `specs/yang_rim_junction_insertion.md` §4 — coaxial cone-band rim
// circles crossing a PLANE face of the other operand (the
// R0004/R0017/R0019/R0044/R0047/R0049 shape).

/// Coaxial double-frustum lathe on the z-axis, uniformly scaled by `s`:
/// rims (0, s·r0), (s, s·r1), (2s, s·r2), two cone bands + planar caps.
fn lathe_brep(r0: f64, r1: f64, r2: f64, s: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(s * r0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(s * r1, 0.0, s),
        },
        BRepVertex {
            point: p(s * r2, 0.0, 2.0 * s),
        },
    ];
    let circle = |cz: f64, nz: f64, radius: f64| Curve::Circle {
        center: p(0.0, 0.0, cz),
        normal: Vector3::new(0.0, 0.0, nz),
        radius,
    };
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: circle(0.0, -1.0, s * r0),
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: circle(s, 1.0, s * r1),
        },
        BRepEdge {
            start: 2,
            end: 2,
            curve: circle(2.0 * s, 1.0, s * r2),
        },
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
    ];
    let cone = |ra: f64, za: f64, rb: f64, zb: f64| -> Surface {
        let slope = (rb - ra) / (zb - za);
        let z_apex = za - ra / slope;
        let dir = if slope > 0.0 { 1.0 } else { -1.0 };
        Surface::Cone {
            apex: p(0.0, 0.0, z_apex),
            axis_dir: Vector3::new(0.0, 0.0, dir),
            half_angle: slope.abs().atan(),
        }
    };
    let faces = vec![
        BRepFace {
            surface: cone(s * r0, 0.0, s * r1, s),
            outer_loop: vec![0, 3, 1, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: cone(s * r1, s, s * r2, 2.0 * s),
            outer_loop: vec![1, 4, 2, 4],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -2.0 * s,
            },
            outer_loop: vec![2],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("lathe fixture builds")
}

/// Axis-aligned box (the slab operand).
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
        .map(|&(start, end)| BRepEdge {
            start,
            end,
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
    BRep::new(vertices, edges, faces).expect("box fixture builds")
}

/// Lathe radii for the class pair: band 1 at half-angle 60°, band 2 at 30°
/// (descending), so a 45°-tilted plane sections band 1 in a HYPERBOLA and
/// band 2 in an ELLIPSE — the mixed-conic over-determined junction the
/// PR-YR21/23 audits stop on (the R0017 v43 shape).
fn class_radii() -> (f64, f64, f64) {
    let r0 = 1.0f64;
    let r1 = r0 + (std::f64::consts::PI / 3.0).tan(); // 60° band
    let r2 = r1 - (std::f64::consts::PI / 6.0).tan(); // 30° band, descending
    (r0, r1, r2)
}

/// Slab whose near face is the 45°-tilted plane x + z = c + s (normal
/// (1,0,1)/√2), covering the whole far side of the lathe: an axis-aligned
/// box in the (u,v,w) frame u=(x+z)/√2, v=y, w=(z−x)/√2, rotated back.
fn tilted_slab_brep(c: f64, s: f64) -> BRep {
    let iso = std::f64::consts::FRAC_1_SQRT_2;
    // Box in rotated frame: u ∈ [u0, u1], v ∈ [±v1], w ∈ [±w1].
    let u0 = (c + s) * iso;
    let (u1, v1, w1) = (8.0 * s * iso, 4.0 * s, 8.0 * s * iso);
    // Rotated-frame basis vectors in world coordinates.
    let bu = [iso, 0.0, iso];
    let bv = [0.0, 1.0, 0.0];
    let bw = [-iso, 0.0, iso];
    let corner = |u: f64, v: f64, w: f64| -> [f64; 3] {
        [
            u * bu[0] + v * bv[0] + w * bw[0],
            u * bu[1] + v * bv[1] + w * bw[1],
            u * bu[2] + v * bv[2] + w * bw[2],
        ]
    };
    let lo = [u0, -v1, -w1];
    let hi = [u1, v1, w1];
    let v = |uu: [f64; 3]| BRepVertex {
        point: p(uu[0], uu[1], uu[2]),
    };
    let vertices = vec![
        v(corner(lo[0], lo[1], lo[2])),
        v(corner(hi[0], lo[1], lo[2])),
        v(corner(hi[0], hi[1], lo[2])),
        v(corner(lo[0], hi[1], lo[2])),
        v(corner(hi[0], hi[1], hi[2])),
        v(corner(hi[0], lo[1], hi[2])),
        v(corner(lo[0], lo[1], hi[2])),
        v(corner(lo[0], hi[1], hi[2])),
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
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    // Face planes in the rotated frame (same order as box_brep's), with
    // n·p + d = 0 in world coordinates.
    let planes: [([f64; 3], f64); 6] = [
        ([-bw[0], -bw[1], -bw[2]], lo[2]),
        (bw, -hi[2]),
        (bu, -hi[0]),
        (bv, -hi[1]),
        ([-bu[0], -bu[1], -bu[2]], lo[0]),
        ([-bv[0], -bv[1], -bv[2]], lo[1]),
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
    BRep::new(vertices, edges, faces).expect("tilted slab fixture builds")
}

/// The class pair at scale `s`: 60°/30° lathe ∖ 45°-tilted slab through
/// x + z = (c + 1)·s with c = 2.0 — the shared rim (z = s, r = r1·s)
/// crosses the tilted face plane transversally at x = c·s.
fn lathe_tilted_slab_pair(s: f64) -> (BRep, BRep) {
    let (r0, r1, r2) = class_radii();
    (lathe_brep(r0, r1, r2, s), tilted_slab_brep(2.0 * s, s))
}

/// Analytic reference volume of lathe ∖ tilted slab at scale `s`.
///
/// V_lathe = Σ_bands (π·h/3)(R² + R·r + r²). The tilted plane x + z =
/// c + 1 removes, per z slice, the circular segment beyond the chord
/// x = (c+1) − z: A = r²·acos(q/r) − q·√(r² − q²) for q < r (q stays > 0
/// over the lathe's z-range, so the segment is always the minor one).
/// Composite Simpson at 4096 intervals per band — deterministic and
/// orders more accurate than the assertion band.
fn lathe_tilted_slab_reference_volume(s: f64) -> f64 {
    let (r0, r1, r2) = class_radii();
    let c = 2.0f64;
    let v_lathe = std::f64::consts::PI / 3.0
        * ((r0 * r0 + r0 * r1 + r1 * r1) + (r1 * r1 + r1 * r2 + r2 * r2));
    let seg = |r: f64, q: f64| -> f64 {
        if r <= q {
            0.0
        } else {
            r * r * (q / r).acos() - q * (r * r - q * q).sqrt()
        }
    };
    // Band from (ra @ za) to (rb @ za+1): r(z) linear, q(z) = (c+1) − z.
    let simpson = |ra: f64, rb: f64, za: f64| -> f64 {
        let n = 4096usize;
        let h = 1.0 / n as f64;
        let mut acc = 0.0f64;
        for k in 0..=n {
            let w = if k == 0 || k == n {
                1.0
            } else if k % 2 == 1 {
                4.0
            } else {
                2.0
            };
            let t = k as f64 * h;
            let r = ra + (rb - ra) * t;
            let q = (c + 1.0) - (za + t);
            acc += w * seg(r, q);
        }
        acc * h / 3.0
    };
    let v_cut = simpson(r0, r1, 0.0) + simpson(r1, r2, 1.0);
    (v_lathe - v_cut) * s * s * s
}

/// Regression pin (increment-4 RED): the Stage-4 over-determined junction
/// STOP (`LocalRefinementRequired` at the mixed hyperbola×ellipse cone-band
/// rim junctions) must never return for the lathe ∖ tilted-slab operands.
#[test]
fn lathe_tilted_slab_subtract_never_stops_at_stage4_junction() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = lathe_tilted_slab_pair(1.0);
    if let Err(e) = boolean(&a, &b, BoolOp::Subtract, &sb) {
        assert!(
            !matches!(e, YangError::Stage4RegionInvalid { .. }),
            "rim-junction incr 4: the Stage-4 junction STOP returned: {e:?}"
        );
    }
}

/// GREEN target (increment 4): the subtract completes watertight with the
/// analytic lathe-minus-tilted-slab volume at unit scale.
#[test]
fn lathe_tilted_slab_subtract_green_target() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = lathe_tilted_slab_pair(1.0);
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("incr-4 green target: subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "incr-4: subtract output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = lathe_tilted_slab_reference_volume(1.0);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "incr-4: subtract volume {vol} vs analytic {expect} (chord under-fill band only)"
    );
}

/// GREEN target at coordinate scale 4000 (the R0017/R0044 magnitude):
/// exercises the §4d scale-aware exactness certificate — the ABSOLUTE
/// 1e-12 band is ~2 ULP here and can never certify a junction.
#[test]
fn lathe_tilted_slab_subtract_green_target_large_scale() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let s = 4000.0f64;
    let (a, b) = lathe_tilted_slab_pair(s);
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("incr-4 large-scale green target: subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "incr-4 large-scale: subtract output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = lathe_tilted_slab_reference_volume(s);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "incr-4 large-scale: subtract volume {vol} vs analytic {expect}"
    );
}

// ── Increment 5: the prism-edge × cone-lateral junction (R0017 v101) ────
// Spec `specs/yang_stage4_conic_triple_junction.md` (wired): a box EDGE
// pierces a cone band's interior — the junction vertex sits exactly on
// both box planes but a facet-sagitta off the true cone, with NO rim to
// insert into. Stage 4 must relocate it onto all three surfaces.

/// 30° frustum band (apex z = −√3, r(0) = 1, r(2) = 2.155…) at scale `s`.
fn frustum30_brep(s: f64) -> BRep {
    let tan30 = (std::f64::consts::PI / 6.0).tan();
    let (r0, r1) = (1.0 * s, (2.0 + 1.0 / tan30) * tan30 * s);
    let verts = vec![
        BRepVertex {
            point: p(r0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(r1, 0.0, 2.0 * s),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 2.0 * s),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r1,
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
            surface: Surface::Cone {
                apex: p(0.0, 0.0, -s / tan30),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: std::f64::consts::PI / 6.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -2.0 * s,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("frustum30 fixture builds")
}

/// Corner-notch slab at scale `s`: {x + z ≥ 2s} ∩ {y ≥ 0.2s} — its
/// interior edge {x + z = 2s, y = 0.2s} pierces the 30° cone lateral near
/// z ≈ 0.63s. The tilted face sections the cone in an ELLIPSE (45° > 30°)
/// and the off-axis axis-parallel y-face in a HYPERBOLA — the mixed-conic
/// over-determined junction (PR-YR21/23 audit shape, the R0017 v101
/// class). The y-face deliberately misses the axis: a through-axis plane
/// would section generator LINES instead (a separate cone-bearing
/// line-edge vocabulary wall, out of this increment's scope).
fn corner_notch_brep(s: f64) -> BRep {
    let iso = std::f64::consts::FRAC_1_SQRT_2;
    let u0 = 2.0 * s * iso;
    let (u1, w1) = (8.0 * s * iso, 8.0 * s * iso);
    let bu = [iso, 0.0, iso];
    let bv = [0.0, 1.0, 0.0];
    let bw = [-iso, 0.0, iso];
    let corner = |u: f64, v: f64, w: f64| -> [f64; 3] {
        [
            u * bu[0] + v * bv[0] + w * bw[0],
            u * bu[1] + v * bv[1] + w * bw[1],
            u * bu[2] + v * bv[2] + w * bw[2],
        ]
    };
    let lo = [u0, 0.2 * s, -w1];
    let hi = [u1, 4.0 * s, w1];
    let v = |uu: [f64; 3]| BRepVertex {
        point: p(uu[0], uu[1], uu[2]),
    };
    let vertices = vec![
        v(corner(lo[0], lo[1], lo[2])),
        v(corner(hi[0], lo[1], lo[2])),
        v(corner(hi[0], hi[1], lo[2])),
        v(corner(lo[0], hi[1], lo[2])),
        v(corner(hi[0], hi[1], hi[2])),
        v(corner(hi[0], lo[1], hi[2])),
        v(corner(lo[0], lo[1], hi[2])),
        v(corner(lo[0], hi[1], hi[2])),
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
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([-bw[0], -bw[1], -bw[2]], lo[2]),
        (bw, -hi[2]),
        (bu, -hi[0]),
        (bv, -hi[1]),
        ([-bu[0], -bu[1], -bu[2]], lo[0]),
        ([-bv[0], -bv[1], -bv[2]], lo[1]),
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
    BRep::new(vertices, edges, faces).expect("corner notch fixture builds")
}

/// Analytic reference volume of frustum30 ∖ corner-notch at scale `s`:
/// per z slice the notch removes {x ≥ q(z) = 2 − z} ∩ {y ≥ y0 = 0.2} of
/// the r(z) disc — the corner integral
/// ∫_{y0}^{ymax} (√(r²−y²) − q) dy with ymax = √(r² − q²), via the
/// antiderivative G(y) = (y·√(r²−y²) + r²·asin(y/r))/2 − q·y. Composite
/// Simpson over z at 4096 intervals (deterministic, orders more accurate
/// than the assertion band).
fn frustum30_notch_reference_volume(s: f64) -> f64 {
    let tan30 = (std::f64::consts::PI / 6.0).tan();
    let r_of = |z: f64| (z + 1.0 / tan30) * tan30;
    let (ra, rb) = (r_of(0.0), r_of(2.0));
    let v_frustum = std::f64::consts::PI * 2.0 / 3.0 * (ra * ra + ra * rb + rb * rb);
    let y0 = 0.2f64;
    let corner_area = |r: f64, q: f64| -> f64 {
        if q >= r {
            return 0.0;
        }
        let ymax = (r * r - q * q).sqrt();
        if ymax <= y0 {
            return 0.0;
        }
        let g = |y: f64| (y * (r * r - y * y).sqrt() + r * r * (y / r).asin()) / 2.0 - q * y;
        g(ymax) - g(y0)
    };
    let n = 4096usize;
    let h = 2.0 / n as f64;
    let mut acc = 0.0f64;
    for k in 0..=n {
        let w = if k == 0 || k == n {
            1.0
        } else if k % 2 == 1 {
            4.0
        } else {
            2.0
        };
        let z = k as f64 * h;
        acc += w * corner_area(r_of(z), 2.0 - z);
    }
    let v_cut = acc * h / 3.0;
    (v_frustum - v_cut) * s * s * s
}

/// Regression pin (increment-5 RED): the Stage-4 over-determined junction
/// STOP at the pierced-lateral junction must never return.
#[test]
fn frustum_corner_notch_subtract_never_stops_at_stage4_junction() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = (frustum30_brep(1.0), corner_notch_brep(1.0));
    if let Err(e) = boolean(&a, &b, BoolOp::Subtract, &sb) {
        assert!(
            !matches!(e, YangError::Stage4RegionInvalid { .. }),
            "incr 5: the Stage-4 junction STOP returned: {e:?}"
        );
    }
}

/// GREEN target (increment 5): the subtract completes watertight with the
/// analytic notched-frustum volume — and the notch is REALLY cut (the
/// volume sits well below the un-notched frustum).
#[test]
fn frustum_corner_notch_subtract_green_target() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (a, b) = (frustum30_brep(1.0), corner_notch_brep(1.0));
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("incr-5 green target: subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "incr-5: subtract output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = frustum30_notch_reference_volume(1.0);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "incr-5: subtract volume {vol} vs analytic {expect} (chord under-fill band only)"
    );
    let tan30 = (std::f64::consts::PI / 6.0).tan();
    let (ra, rb) = (1.0, (2.0 + 1.0 / tan30) * tan30);
    let v_frustum = std::f64::consts::PI * 2.0 / 3.0 * (ra * ra + ra * rb + rb * rb);
    assert!(
        vol < 0.97 * v_frustum,
        "incr-5: the notch must actually be cut ({vol} vs full {v_frustum})"
    );
}

/// GREEN target at coordinate scale 4000 (the R0017 magnitude): exercises
/// the scale-aware Newton tolerance in the triple relocation.
#[test]
fn frustum_corner_notch_subtract_green_target_large_scale() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let s = 4000.0f64;
    let (a, b) = (frustum30_brep(s), corner_notch_brep(s));
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("incr-5 large-scale green target: subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "incr-5 large-scale: subtract output must be watertight"
    );
    let vol = mesh_signed_volume(out.as_mesh());
    let expect = frustum30_notch_reference_volume(s);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "incr-5 large-scale: subtract volume {vol} vs analytic {expect}"
    );
}

/// Same-type-junction sibling (hyperbola×hyperbola — the axis-parallel
/// slab): passes TODAY without insertion (the single-map junction never
/// trips the mixed-conic audits) and must KEEP passing once the plane arm
/// starts inserting these junctions too.
#[test]
fn lathe_axis_parallel_slab_subtract_stays_green() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = lathe_brep(1.0, 2.0, 0.8, 1.0);
    let b = box_brep([0.75, -4.0, -0.5], [4.0, 4.0, 2.5]);
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("axis-parallel sibling: subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "axis-parallel sibling: subtract output must be watertight"
    );
}

/// GREEN (increments 2+3 landed 2026-07-10): the union completes
/// watertight with the exact truncated-Steinmetz volume
/// V = 2·πr²h − V_common(r, h).
#[test]
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

// ── KV16: the SAME-TYPE hyperbola×hyperbola band junction (R0017) ────────
// Spec `specs/kv16_hyperbola_arc_vocabulary.md` + the
// `yang_stage4_conic_triple_junction` residue: an AXIS-PARALLEL slab face
// sections BOTH coaxial cone bands in hyperbolas; the two hyperbolas meet
// where the shared band rim crosses the plane — a {coneA, coneB, plane}
// junction. Both curves land in the SAME single-curve conic map
// (`vert_cone_hyperbola`), so the ≥2-maps triple trigger cannot see the
// junction: the second insert silently overwrites the first and the vertex
// is relocated onto only ONE band's hyperbola, leaving an off-curve
// endpoint on the other band's output edge.

/// First-order distance of `pt` from the `u > 0` branch of an output
/// `Curve::Hyperbola` (in-plane |g|/|∇g| + out-of-plane), mirroring the
/// kernel-v2 import certification.
fn hyperbola_endpoint_residual(curve: &Curve, pt: Point3) -> f64 {
    let Curve::Hyperbola {
        center,
        normal,
        major_axis,
        semi_transverse,
        semi_conjugate,
    } = curve
    else {
        panic!("not a hyperbola edge");
    };
    let n = normal.as_array();
    let m = major_axis.as_array();
    let w = [
        n[1] * m[2] - n[2] * m[1],
        n[2] * m[0] - n[0] * m[2],
        n[0] * m[1] - n[1] * m[0],
    ];
    let c = center.as_array();
    let pa = pt.as_array();
    let d = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
    let u = dot(d, m);
    let v = dot(d, w);
    let oop = dot(d, n);
    let (a, b) = (*semi_transverse, *semi_conjugate);
    let g = (u / a).powi(2) - (v / b).powi(2) - 1.0;
    let grad = 2.0 * (u / (a * a)).hypot(v / (b * b));
    let in_plane = if grad > 0.0 {
        (g / grad).abs()
    } else {
        f64::INFINITY
    };
    if u <= 0.0 {
        return f64::INFINITY;
    }
    in_plane.max(oop.abs())
}

/// GREEN target: lathe ∖ corner-notch box whose VERTICAL EDGE (x = 2.5,
/// y = 0.5, ∥ the axis) pierces BOTH cone bands' interiors. Each pierce
/// vertex sits on {planeX, planeY, coneN} with BOTH incident section
/// curves hyperbolas (axis-parallel planes) — the same-map collision. The
/// subtract must complete with EVERY output hyperbola edge's endpoints ON
/// their own branch (the pierce vertex relocated onto the exact triple
/// point, not onto just one plane's hyperbola).
#[test]
fn same_type_hyperbola_edge_pierce_endpoints_on_curve() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[rim-junction] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r0, r1, r2) = class_radii();
    let lathe = lathe_brep(r0, r1, r2, 1.0);
    // Notch corner edge at (2.5, 0.5): radial distance √6.5 ≈ 2.5495 < r1 ≈
    // 2.732 — pierces band 1 at z ≈ 0.895 and band 2 at z ≈ 1.316. The box
    // clears the caps (z ∈ [−0.5, 2.5]) and the far side (x, y to 4 > r1).
    let (xa, yb) = (2.5f64, 0.5f64);
    let slab = box_brep([xa, yb, -0.5], [4.0, 4.0, 2.5]);
    let out = boolean(&lathe, &slab, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("edge-pierce same-type subtract failed with {e:?}"));
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "edge-pierce same-type: subtract output must be watertight"
    );

    // Volume: V_lathe − ∫ A(r(z)) dz, where A(r) = disc ∩ {x ≥ xa, y ≥ yb}
    // (nonempty exactly when r ≥ √(xa²+yb²); closed form via the circular
    // antiderivative), piecewise per band; Simpson within each band.
    let tan60 = (std::f64::consts::PI / 3.0).tan();
    let tan30 = (std::f64::consts::PI / 6.0).tan();
    let v_lathe = std::f64::consts::PI / 3.0
        * ((r0 * r0 + r0 * r1 + r1 * r1) + (r1 * r1 + r1 * r2 + r2 * r2));
    let corner_r = (xa * xa + yb * yb).sqrt();
    let corner_area = |r: f64| {
        if r <= corner_r {
            return 0.0;
        }
        let y_max = (r * r - xa * xa).sqrt();
        let anti = |y: f64| (y * (r * r - y * y).sqrt() + r * r * (y / r).asin()) / 2.0 - xa * y;
        anti(y_max) - anti(yb)
    };
    let simpson = |f: &dyn Fn(f64) -> f64, lo: f64, hi: f64| {
        let n = 2000usize;
        let h = (hi - lo) / n as f64;
        let mut s = f(lo) + f(hi);
        for k in 1..n {
            s += if k % 2 == 1 { 4.0 } else { 2.0 } * f(lo + k as f64 * h);
        }
        s * h / 3.0
    };
    let z1_lo = (corner_r - r0) / tan60; // band-1 radius reaches the corner
    let z2_hi = 1.0 + (r1 - corner_r) / tan30; // band-2 falls back to it
    let cut = simpson(&|z| corner_area(r0 + tan60 * z), z1_lo, 1.0)
        + simpson(&|z| corner_area(r1 - tan30 * (z - 1.0)), 1.0, z2_hi);
    let expect = v_lathe - cut;
    let vol = mesh_signed_volume(out.as_mesh());
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "edge-pierce same-type: volume {vol} vs analytic {expect}"
    );

    // The discriminating oracle: every output hyperbola edge endpoint lies
    // on ITS OWN branch at the kernel import band (the R0017 auto-union
    // certification). A pierce vertex relocated onto only ONE plane's
    // hyperbola fails this by a facet-sagitta-scale residual.
    let verts = out.vertices();
    let mut hyperbola_edges = 0usize;
    for (i, e) in out.edges().iter().enumerate() {
        if !matches!(e.curve, Curve::Hyperbola { .. }) {
            continue;
        }
        hyperbola_edges += 1;
        let Curve::Hyperbola {
            semi_transverse,
            semi_conjugate,
            ..
        } = e.curve
        else {
            unreachable!();
        };
        let scale = semi_transverse.max(semi_conjugate);
        for v in [e.start, e.end] {
            let pt = verts[v as usize].point;
            let mag = pt.as_array().iter().fold(0.0f64, |acc, c| acc.max(c.abs()));
            let band = 1e-9 * (1.0 + scale.max(mag));
            let resid = hyperbola_endpoint_residual(&e.curve, pt);
            assert!(
                resid <= band,
                "hyperbola edge {i} endpoint {v} off its branch: residual {resid:.3e} \
                 > band {band:.3e}"
            );
        }
    }
    assert!(
        hyperbola_edges >= 2,
        "expected hyperbola output edges from both planes, found {hyperbola_edges}"
    );
}

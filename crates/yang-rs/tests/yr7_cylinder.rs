//! PR-YR7 (P2a) RED — curved Stage-1 tessellation oracle for the CYLINDER.
//!
//! Spec: `specs/yang_pr_yr7_cylinder_tessellation.md`.
//!
//! Asserts that a closed solid-cylinder B-Rep (encoded with a seam edge per
//! spec §1) tessellates via `BRep::new` into a watertight, error-bounded
//! triangle mesh with a correct, invertible `TessellationMap`. Four oracles:
//!
//! 1. **Surface-to-mesh distance ≤ d_ε** — no triangle bulges past the chord
//!    bound (`d_ε = 1e-2 × AABB_diagonal`, the diagonal computed ANALYTICALLY
//!    from the two rim circles' exact per-axis extents).
//! 2. **Watertight + 2-manifold** — every undirected edge shared by exactly
//!    two triangles. Plus an env-gated `inputcheck` arm (self-skips on
//!    `BinaryNotFound`).
//! 3. **Bijection round-trip** — `eval_source(map.lookup(v))` reproduces
//!    `mesh.verts[v]` for every mesh vertex (relies on the NEW production
//!    method `BRep::eval_source`).
//! 4. **Euler** — `V − E + F = 2`.
//!
//! Plus: a unit test of the NEW `yang_rs::signed_distance_to_surface`, Sphere/
//! Cone still reject at `BRep::new`, and the planar cube path is unchanged.
//!
//! RED status: this file references `BRep::eval_source` and
//! `signed_distance_to_surface`, which GREEN will add. Until then the crate's
//! test build fails to compile on exactly those two missing items.

use std::collections::BTreeMap;
use std::time::Duration;

use cad_primitives::{Point3, Vector3};
use cherchi_sidecar_rs::{inputcheck, SidecarError};
use yang_rs::{
    signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    TessellationSource, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives has no dot/cross/normalize helpers).
// =========================================================================

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

/// Perpendicular distance from point `x` to the infinite line through
/// `axis_point` with unit direction `axis_unit`.
fn dist_point_to_line(x: [f64; 3], axis_point: [f64; 3], axis_unit: [f64; 3]) -> f64 {
    let w = sub(x, axis_point);
    let along = dot(w, axis_unit);
    let proj = add(axis_point, scale(axis_unit, along));
    norm(sub(x, proj))
}

// =========================================================================
// Cylinder B-Rep fixture (spec §1 seam-edge encoding, defined locally:
// integration tests cannot see #[cfg(test)] items in lib.rs).
// =========================================================================

/// Build a closed solid-cylinder B-Rep per spec §1.
///
/// - `v0` = bottom-rim seam point (bottom circle at angle 0),
///   `v1` = top-rim seam point.
/// - `e0` bottom rim Circle (center bottom, normal −axis), start=end=v0.
/// - `e1` top rim Circle (center top, normal +axis), start=end=v1.
/// - `e2` seam LineSegment v0 → v1.
/// - `f0` lateral Cylinder, outer_loop = [e0, e2, e1, e2].
/// - `f1` bottom cap Plane (normal −axis_unit, d = −(normal·bottom_center)),
///   outer_loop = [e0].
/// - `f2` top cap Plane (normal +axis_unit, d = −(normal·top_center)),
///   outer_loop = [e1].
///
/// `axis_dir` is normalized inside the helper (so callers may pass a non-unit
/// tilted direction). The bottom seam point is at angle 0 around the axis,
/// using the SAME deterministic basis the production `ortho_basis` is required
/// to use; but since the round-trip oracle compares against whatever the
/// production tessellation emits, the fixture only needs a self-consistent
/// "angle 0" point on each rim. We pick angle 0 = `+r·e1` where `e1` is the
/// stablest-cross seed (matching the spec's `ortho_basis` contract): the unit
/// of `axis_unit × (the world axis least parallel to axis_unit)`.
fn cylinder_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));

    // Deterministic in-plane basis (e1, e2) for the rim circles. This MUST
    // match the production `ortho_basis(axis_dir)` contract (spec §4): pick the
    // world axis least parallel to `axis_unit`, cross to seed e1, then
    // e2 = axis_unit × e1. The fixture only needs the angle-0 *seam* point to
    // lie on the rim; the round-trip oracle compares emitted verts against
    // eval_source, both of which use the production basis, so the exact seam
    // angle the fixture chooses does not have to equal the production basis.
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    // Seam points at angle 0 = center + r·e1 on each rim.
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
        // e0 bottom rim
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        // e1 top rim
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        // e2 seam
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    // Cap plane d = -(normal · center).
    let bottom_d = -dot(neg_axis, bottom_center);
    let top_d = -dot(axis_unit, top_center);

    let faces = vec![
        // f0 lateral
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
        },
        // f1 bottom cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
        },
        // f2 top cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
        },
    ];

    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate the cylinder")
}

/// Analytic AABB diagonal from the two rim circles' exact extents (spec §3):
/// per axis `i`, a circle of center `c`, unit normal `n`, radius `r` spans
/// `c_i ± r·√(max(0, 1 − n_i²))`; combine min/max over both rims.
fn analytic_aabb_diagonal(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> f64 {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));
    // Both rim circles have unit normal = ±axis_unit (n_i² identical), radius r.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = radius * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    norm(sub(hi, lo))
}

// =========================================================================
// Cube fixture (copied from m1_inputcheck.rs `unit_cube_brep_at`) — guards
// the planar path against the curved dispatch refactor.
// =========================================================================

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
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
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
        })
        .collect();
    BRep::new(verts, edges, faces).expect("unit cube BRep::new failed")
}

// =========================================================================
// The cylinder corpus under test.
// =========================================================================

struct CylinderCase {
    name: &'static str,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
}

fn corpus() -> Vec<CylinderCase> {
    vec![
        CylinderCase {
            name: "z-up unit",
            axis_point: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 1.0,
            height: 1.0,
        },
        CylinderCase {
            name: "z-up wide-short",
            axis_point: [2.0, -1.0, 0.5],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 5.0,
            height: 0.5,
        },
        CylinderCase {
            name: "x-axis tall-thin",
            axis_point: [-3.0, 4.0, 1.0],
            axis_dir: [1.0, 0.0, 0.0],
            radius: 0.3,
            height: 7.0,
        },
        CylinderCase {
            // Off-axis, NON-UNIT axis_dir (magnitude 3, tilted) — the key
            // adversarial case: normalization + ortho_basis must be correct.
            name: "off-axis non-unit",
            axis_point: [1.0, 2.0, -1.0],
            axis_dir: [1.0, 2.0, 2.0],
            radius: 2.0,
            height: 4.0,
        },
    ]
}

/// Classify a triangle as lateral (Cylinder) vs a cap (Plane) by checking
/// which surface ALL of its 3 vertices lie near (within tol). We do NOT assume
/// an emission order the spec doesn't guarantee. Returns the face index of the
/// surface the triangle belongs to, or panics if it matches none/both.
///
/// `surfaces` is `(face_idx, Surface)` for the three cylinder faces.
fn classify_triangle(
    verts: &[Point3],
    tri: [u32; 3],
    surfaces: &[(usize, Surface)],
    tol: f64,
) -> usize {
    let pts: [[f64; 3]; 3] = [
        verts[tri[0] as usize].as_array(),
        verts[tri[1] as usize].as_array(),
        verts[tri[2] as usize].as_array(),
    ];
    let mut matches: Vec<usize> = Vec::new();
    for (fi, surf) in surfaces {
        let all_near = pts.iter().all(|&x| {
            let d = signed_distance_oracle(*surf, x).abs();
            d <= tol
        });
        if all_near {
            matches.push(*fi);
        }
    }
    assert_eq!(
        matches.len(),
        1,
        "triangle {tri:?} should lie near exactly one cylinder surface within \
         tol {tol}, matched faces {matches:?}"
    );
    matches[0]
}

/// Test-only oracle copy of the surface distance (we do NOT call the
/// production `signed_distance_to_surface` for classification, so that the
/// classification stands independently of the function under test). Plane:
/// `n_unit·x + d_scaled` using the stored normal/ d as a unit-plane (the
/// fixture builds unit normals). Cylinder: `dist(x, axis) − r`.
fn signed_distance_oracle(surface: Surface, x: [f64; 3]) -> f64 {
    match surface {
        Surface::Plane { normal, d } => dot(normal.as_array(), x) + d,
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let au = unit(axis_dir.as_array());
            dist_point_to_line(x, axis_point.as_array(), au) - radius
        }
        other => panic!("oracle: unexpected surface {other:?}"),
    }
}

// =========================================================================
// Oracle 1 — surface-to-mesh distance ≤ d_ε
// =========================================================================

#[test]
fn oracle1_mesh_within_chord_error_of_surface() {
    for case in corpus() {
        let b = cylinder_brep(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let d_eps =
            1e-2 * analytic_aabb_diagonal(case.axis_point, case.axis_dir, case.radius, case.height);
        assert!(d_eps > 0.0, "[{}] d_eps must be positive", case.name);

        // Reconstruct the three cylinder surfaces for classification.
        let axis_unit = unit(case.axis_dir);
        let bottom_center = case.axis_point;
        let top_center = add(bottom_center, scale(axis_unit, case.height));
        let neg = scale(axis_unit, -1.0);
        let surfaces: Vec<(usize, Surface)> = vec![
            (
                0,
                Surface::Cylinder {
                    axis_point: p(case.axis_point[0], case.axis_point[1], case.axis_point[2]),
                    axis_dir: Vector3::new(case.axis_dir[0], case.axis_dir[1], case.axis_dir[2]),
                    radius: case.radius,
                },
            ),
            (
                1,
                Surface::Plane {
                    normal: Vector3::new(neg[0], neg[1], neg[2]),
                    d: -dot(neg, bottom_center),
                },
            ),
            (
                2,
                Surface::Plane {
                    normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                    d: -dot(axis_unit, top_center),
                },
            ),
        ];

        for &tri in &mesh.tris {
            let fi = classify_triangle(&mesh.verts, tri, &surfaces, d_eps);
            let surf = surfaces[fi].1;
            // Sample centroid + 3 vertices.
            let a = mesh.verts[tri[0] as usize].as_array();
            let bb = mesh.verts[tri[1] as usize].as_array();
            let c = mesh.verts[tri[2] as usize].as_array();
            let centroid = scale(add(add(a, bb), c), 1.0 / 3.0);
            for sample in [a, bb, c, centroid] {
                let d = signed_distance_oracle(surf, sample).abs();
                assert!(
                    d <= d_eps,
                    "[{}] tri {tri:?} on face {fi}: sample {sample:?} distance {d} \
                     exceeds chord bound d_eps {d_eps}",
                    case.name
                );
            }
        }
    }
}

// =========================================================================
// Oracle 2 — watertight + 2-manifold (+ env-gated inputcheck)
// =========================================================================

#[test]
fn oracle2_watertight_two_manifold() {
    for case in corpus() {
        let b = cylinder_brep(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();

        // Every undirected edge shared by EXACTLY two triangles.
        let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for tri in &mesh.tris {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (a, c) = (tri[i], tri[j]);
                let key = if a < c { (a, c) } else { (c, a) };
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        for (edge, count) in &edge_count {
            assert_eq!(
                *count, 2,
                "[{}] undirected edge {edge:?} shared by {count} triangles \
                 (must be exactly 2 for watertight 2-manifold)",
                case.name
            );
        }
    }
}

#[test]
fn oracle2_inputcheck_clean_env_gated() {
    for case in corpus() {
        let b = cylinder_brep(case.axis_point, case.axis_dir, case.radius, case.height);
        let report = match inputcheck(b.as_mesh(), Duration::from_secs(30)) {
            Ok(r) => r,
            Err(SidecarError::BinaryNotFound { .. }) => {
                eprintln!("[yang-rs yr7] SKIP: inputcheck binary not found");
                return;
            }
            Err(e) => panic!("[{}] inputcheck failed unexpectedly: {e:?}", case.name),
        };
        assert!(
            report.all_pass(),
            "[{}] cylinder Stage-1 mesh must pass all inputcheck axioms; got {report:?}",
            case.name
        );
    }
}

// =========================================================================
// Oracle 3 — bijection round-trip (relies on BRep::eval_source — NEW)
// =========================================================================

#[test]
fn oracle3_eval_source_round_trip() {
    const TOL: f64 = 1e-9;
    for case in corpus() {
        let b = cylinder_brep(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let map = b.tessellation_map();
        assert_eq!(
            map.len(),
            mesh.num_verts(),
            "[{}] TessellationMap must cover every mesh vertex",
            case.name
        );
        for v in 0..mesh.num_verts() as u32 {
            let src = map.lookup(v);
            let recon = b.eval_source(src).as_array();
            let actual = mesh.verts[v as usize].as_array();
            let d = norm(sub(recon, actual));
            assert!(
                d <= TOL,
                "[{}] vertex {v} (source {src:?}): eval_source {recon:?} != \
                 mesh vert {actual:?} (dist {d})",
                case.name
            );
        }
    }
}

// =========================================================================
// Oracle 4 — Euler V − E + F = 2
// =========================================================================

#[test]
fn oracle4_euler_characteristic() {
    use std::collections::BTreeSet;
    for case in corpus() {
        let b = cylinder_brep(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
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
        assert_eq!(
            v - e + f,
            2,
            "[{}] Euler V-E+F: V={v} E={e} F={f}",
            case.name
        );
    }
}

// =========================================================================
// signed_distance_to_surface unit test (NEW pub fn)
// =========================================================================

#[test]
fn signed_distance_to_surface_cylinder_formula() {
    // Axis = z-axis through origin, radius 2; point (5,0,0) is 5 from the
    // axis → signed distance 5 − 2 = 3.
    let surf = Surface::Cylinder {
        axis_point: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let d =
        signed_distance_to_surface(surf, p(5.0, 0.0, 0.0)).expect("cylinder distance must be Ok");
    assert!(
        (d - 3.0).abs() < 1e-12,
        "expected dist_to_axis − radius = 3.0, got {d}"
    );

    // A point inside the cylinder → negative signed distance.
    let d_in =
        signed_distance_to_surface(surf, p(0.5, 0.0, 9.0)).expect("cylinder distance must be Ok");
    assert!(
        (d_in - (0.5 - 2.0)).abs() < 1e-12,
        "expected 0.5 − 2.0 = −1.5, got {d_in}"
    );
}

#[test]
fn signed_distance_to_surface_plane_formula() {
    // Plane z = 0 with outward +z: n·x + d at (3, 4, 7) = 7.
    let surf = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let d = signed_distance_to_surface(surf, p(3.0, 4.0, 7.0)).expect("plane distance must be Ok");
    assert!((d - 7.0).abs() < 1e-12, "expected 7.0, got {d}");
}

#[test]
fn signed_distance_to_surface_sphere_ok_cone_reject() {
    let sphere = Surface::Sphere {
        center: p(0.0, 0.0, 0.0),
        radius: 3.0,
    };
    // Signed distance to a sphere = |x − center| − radius. At (1,2,3) about a
    // sphere centered at the origin with r = 3: |(1,2,3)| − 3.
    let r = signed_distance_to_surface(sphere, p(1.0, 2.0, 3.0));
    let expected = norm([1.0, 2.0, 3.0]) - 3.0;
    assert!(
        (r.unwrap() - expected).abs() < 1e-12,
        "Sphere signed distance must be |x − center| − radius = {expected}"
    );

    let cone = Surface::Cone {
        apex: p(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    };
    let r = signed_distance_to_surface(cone, p(1.0, 0.0, 0.0));
    assert!(
        r.is_err(),
        "Cone must not be evaluable by signed_distance_to_surface, got {r:?}"
    );
}

// =========================================================================
// Sphere / Cone faces STILL reject at BRep::new (single-triangle fixture)
// =========================================================================

/// Single planar triangle in z=0 with a caller-chosen surface (independent of
/// the in-lib helper). Passes degeneracy/winding before the surface match.
fn one_triangle(surface: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(2.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 2.0, 0.0),
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
        surface,
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
    }];
    (verts, edges, faces)
}

#[test]
fn sphere_face_on_triangle_is_malformed() {
    // A one-triangle Sphere face lacks the meridian seam Circle the sphere
    // tessellation path requires (spec §1) → MalformedTopology, mirroring how a
    // cylinder-on-a-triangle is malformed rather than CurvedSurfaceNotYetSupported.
    let (v, e, f) = one_triangle(Surface::Sphere {
        center: p(0.0, 0.0, 0.0),
        radius: 1.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "sphere face 0 on a triangle (no seam Circle) must reject as MalformedTopology, got {r:?}"
    );
}

#[test]
fn cone_face_still_rejected() {
    let (v, e, f) = one_triangle(Surface::Cone {
        apex: p(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })),
        "cone face 0 must still reject as CurvedSurfaceNotYetSupported, got {r:?}"
    );
}

// =========================================================================
// Planar box UNCHANGED by the curved dispatch refactor
// =========================================================================

#[test]
fn planar_cube_tessellation_unchanged() {
    let b = unit_cube_brep_at([0.0, 0.0, 0.0]);
    assert_eq!(
        b.num_verts(),
        8,
        "cube still has 8 mesh verts (planar path)"
    );
    assert_eq!(b.num_tris(), 12, "cube still fan-triangulates to 12 tris");

    // The TessellationMap is the 1:1 BRepVertex bijection (planar path).
    let map = b.tessellation_map();
    assert_eq!(map.len(), 8, "cube map covers 8 verts");
    for v in 0..8u32 {
        assert_eq!(
            map.lookup(v),
            TessellationSource::BRepVertex(v),
            "cube vertex {v} source must be BRepVertex({v})"
        );
    }
}

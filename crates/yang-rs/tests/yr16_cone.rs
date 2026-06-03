//! PR-YR16 (P2c) RED — curved Stage-1 tessellation oracle for the CONE.
//!
//! Spec: `specs/yr16_cone_tessellation.md`.
//!
//! Asserts that a closed solid-cone B-Rep (one `Surface::Cone` lateral face +
//! one `Surface::Plane` base cap, sharing a single base-rim seam Circle per
//! spec §1 — NO seam LineSegment, the cone lateral is topologically a disk with
//! the apex as a single interior singular point) tessellates via `BRep::new`
//! into a watertight, error-bounded triangle mesh with a correct, invertible
//! `TessellationMap`. Four oracles, mirroring `yr7_cylinder.rs` (the cone, like
//! the cylinder, has a curved lateral + a planar cap, so it uses the cylinder's
//! surface-CLASSIFICATION oracle style — classify each triangle to the unique
//! surface all 3 of its vertices lie near, then bound its samples):
//!
//! 1. **Surface-to-mesh distance ≤ d_ε** — no triangle bulges past the chord
//!    bound (`d_ε = cone_chord_bound(height, half_angle) = 1e-2 ×
//!    √((2R)² + h²)`, computed test-side from params alone, identical literal to
//!    production per spec §3).
//! 2. **Watertight + 2-manifold** — every undirected edge shared by exactly two
//!    triangles (apex + base included). Plus an env-gated `inputcheck` arm
//!    (self-skips on `BinaryNotFound`).
//! 3. **Bijection round-trip** — `eval_source(map.lookup(v))` reproduces
//!    `mesh.verts[v]` for every mesh vertex (apex/base_seam → `BRepVertex`,
//!    Steiner rim → `BRepEdge`, cap center → `BRepFace`).
//! 4. **Euler** — `V − E + F = 2`.
//!
//! Plus: a `signed_distance_to_surface` cone unit test, and an `eval_source`
//! cone-FACE-arm unit test (the pure apex-fan emits no `BRepFace`-cone vertices,
//! so this arm needs its own focused coverage — asserted as a round-trip
//! property that does not need the crate-private `ortho_basis`).
//!
//! RED status: this file builds a `Surface::Cone` B-Rep through `BRep::new`,
//! which GREEN will teach to tessellate. Until then `cone_brep` panics on the
//! `.expect(...)` because production still returns `CurvedSurfaceNotYetSupported`,
//! and the `signed_distance_to_surface(Cone, ..)` / `eval_source(BRepFace cone)`
//! arms still reject. That runtime-RED is the expected FIP RED state.

use std::collections::BTreeMap;
use std::time::Duration;

use cad_primitives::{Point3, Vector3};
use cherchi_sidecar_rs::{inputcheck, SidecarError};
use yang_rs::{
    signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    TessellationSource,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives has no dot/cross/normalize helpers).
// Copied from yr7_cylinder.rs — integration tests cannot share a module.
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
#[allow(dead_code)]
fn dist_point_to_line(x: [f64; 3], axis_point: [f64; 3], axis_unit: [f64; 3]) -> f64 {
    let w = sub(x, axis_point);
    let along = dot(w, axis_unit);
    let proj = add(axis_point, scale(axis_unit, along));
    norm(sub(x, proj))
}

// =========================================================================
// Cone B-Rep fixture (spec §1: one Cone lateral + one Plane base cap, sharing
// a single base-rim seam Circle; NO seam LineSegment).
// =========================================================================

/// Build a closed solid-cone B-Rep per spec §1.
///
/// - `R = height·tan(half_angle)`; `base_center = apex + height·â`
///   (`â = unit(axis_dir)`).
/// - `verts = [apex (v0), base_seam (v1)]`. `base_seam` is any exact point on
///   the base rim (angle-0 convention): `base_center + R·e1` where `e1` is the
///   stablest-cross seed (the same deterministic basis seed the cylinder
///   fixture uses); the rim pre-pass recovers the azimuth, so the exact choice
///   is free.
/// - `e0` base rim `Curve::Circle { center: base_center, normal: â, radius: R }`,
///   `start = end = 1` (the base_seam vertex). One closed-loop Circle, SHARED by
///   the lateral + base cap (the watertightness mechanism).
/// - `f0` lateral `Surface::Cone { apex, axis_dir, half_angle }`,
///   `outer_loop = [e0]` (apex is interior — no edge references it).
/// - `f1` base cap `Surface::Plane { normal: â, d: −(â·base_center) }`,
///   `outer_loop = [e0]`. Both `reversed: false`.
///
/// `axis_dir` is normalized inside the helper (so callers may pass a non-unit
/// tilted direction).
fn cone_brep(apex: [f64; 3], axis_dir: [f64; 3], half_angle: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let radius = height * half_angle.tan();
    let base_center = add(apex, scale(axis_unit, height));

    // Deterministic in-plane seed e1 (same stablest-cross convention as the
    // cylinder fixture). The base_seam only needs to lie on the rim; the rim
    // pre-pass recovers its azimuth, so any on-rim point is acceptable.
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let base_seam = add(base_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(apex[0], apex[1], apex[2]),
        },
        BRepVertex {
            point: p(base_seam[0], base_seam[1], base_seam[2]),
        },
    ];

    let edges = vec![
        // e0 base rim Circle, shared by lateral + base cap; start = end = v1.
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(base_center[0], base_center[1], base_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
    ];

    // Cap plane d = -(normal · base_center) with outward normal = +axis_unit.
    let cap_d = -dot(axis_unit, base_center);

    let faces = vec![
        // f0 lateral cone
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                half_angle,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 base cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: cap_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("cone_brep: BRep::new should tessellate the cone")
}

/// Test-side chord bound `d_ε = cone_chord_bound(height, half_angle)`
/// = `1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)` (spec §3).
/// IDENTICAL literal to the production `cone_chord_bound`, computed test-side
/// from params alone (the only hard requirement is that the two agree).
fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

// =========================================================================
// The cone corpus under test (spec §6 table). Each half_angle = atan(R/height)
// so the listed R = height·tan(half_angle) holds.
// =========================================================================

struct ConeCase {
    name: &'static str,
    apex: [f64; 3],
    axis_dir: [f64; 3],
    half_angle: f64,
    height: f64,
}

fn corpus() -> Vec<ConeCase> {
    vec![
        ConeCase {
            // z-up unit: R = 1, height = 1 → 45°.
            name: "z-up unit",
            apex: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 0.0, 1.0],
            half_angle: (1.0_f64 / 1.0).atan(),
            height: 1.0,
        },
        ConeCase {
            // z-up wide-short: R = 5, height = 0.5 (h < 2R → §3 min bound is
            // load-bearing).
            name: "z-up wide-short",
            apex: [2.0, -1.0, 0.5],
            axis_dir: [0.0, 0.0, 1.0],
            half_angle: (5.0_f64 / 0.5).atan(),
            height: 0.5,
        },
        ConeCase {
            // x-axis tall-thin: R = 0.3, height = 7.0.
            name: "x-axis tall-thin",
            apex: [-3.0, 4.0, 1.0],
            axis_dir: [1.0, 0.0, 0.0],
            half_angle: (0.3_f64 / 7.0).atan(),
            height: 7.0,
        },
        ConeCase {
            // off-axis NON-UNIT axis_dir (‖(1,2,2)‖ = 3): R = 2, height = 4.0.
            // The key adversarial case: normalization + ortho_basis on a
            // non-unit tilted axis.
            name: "off-axis non-unit",
            apex: [1.0, 2.0, -1.0],
            axis_dir: [1.0, 2.0, 2.0],
            half_angle: (2.0_f64 / 4.0).atan(),
            height: 4.0,
        },
    ]
}

/// Test-only oracle copy of the surface distance (we do NOT call the production
/// `signed_distance_to_surface` for classification, so the classification stands
/// independently of the function under test). Plane: `n·x + d` (fixture builds
/// unit normals). Cone: `radial − |h_axial|·tan(α)` where
/// `radial = |(x − apex) − ((x − apex)·â)·â|` and `h_axial = (x − apex)·â`
/// (spec §1 / §6).
fn signed_distance_oracle(surface: Surface, x: [f64; 3]) -> f64 {
    match surface {
        Surface::Plane { normal, d } => dot(normal.as_array(), x) + d,
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let a = apex.as_array();
            let ax = unit(axis_dir.as_array());
            let w = sub(x, a);
            let h_axial = dot(w, ax);
            let radial = norm(sub(w, scale(ax, h_axial)));
            radial - h_axial.abs() * half_angle.tan()
        }
        other => panic!("oracle: unexpected surface {other:?}"),
    }
}

/// Classify a triangle as lateral (Cone) vs base cap (Plane) by checking which
/// surface ALL 3 of its vertices lie near (within tol). Returns the face index
/// of the matched surface, or panics if it matches none/both. Mirrors
/// `yr7_cylinder.rs::classify_triangle`. `surfaces` is `(face_idx, Surface)`.
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
        let all_near = pts
            .iter()
            .all(|&x| signed_distance_oracle(*surf, x).abs() <= tol);
        if all_near {
            matches.push(*fi);
        }
    }
    assert_eq!(
        matches.len(),
        1,
        "triangle {tri:?} should lie near exactly one cone surface within \
         tol {tol}, matched faces {matches:?}"
    );
    matches[0]
}

// =========================================================================
// Oracle 1 — surface-to-mesh distance ≤ d_ε
// =========================================================================

#[test]
fn oracle1_mesh_within_chord_error_of_surface() {
    for case in corpus() {
        let b = cone_brep(case.apex, case.axis_dir, case.half_angle, case.height);
        let mesh = b.as_mesh();
        let d_eps = cone_chord_bound(case.height, case.half_angle);
        assert!(d_eps > 0.0, "[{}] d_eps must be positive", case.name);

        // Reconstruct the two cone surfaces for classification.
        let axis_unit = unit(case.axis_dir);
        let base_center = add(case.apex, scale(axis_unit, case.height));
        let surfaces: Vec<(usize, Surface)> = vec![
            (
                0,
                Surface::Cone {
                    apex: p(case.apex[0], case.apex[1], case.apex[2]),
                    axis_dir: Vector3::new(case.axis_dir[0], case.axis_dir[1], case.axis_dir[2]),
                    half_angle: case.half_angle,
                },
            ),
            (
                1,
                Surface::Plane {
                    normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                    d: -dot(axis_unit, base_center),
                },
            ),
        ];

        for &tri in &mesh.tris {
            let fi = classify_triangle(&mesh.verts, tri, &surfaces, d_eps);
            let surf = surfaces[fi].1;
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
        let b = cone_brep(case.apex, case.axis_dir, case.half_angle, case.height);
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
        let b = cone_brep(case.apex, case.axis_dir, case.half_angle, case.height);
        let report = match inputcheck(b.as_mesh(), Duration::from_secs(30)) {
            Ok(r) => r,
            Err(SidecarError::BinaryNotFound { .. }) => {
                eprintln!("[yang-rs yr16] SKIP: inputcheck binary not found");
                return;
            }
            Err(e) => panic!("[{}] inputcheck failed unexpectedly: {e:?}", case.name),
        };
        assert!(
            report.all_pass(),
            "[{}] cone Stage-1 mesh must pass all inputcheck axioms; got {report:?}",
            case.name
        );
    }
}

// =========================================================================
// Oracle 3 — bijection round-trip (relies on BRep::eval_source)
// =========================================================================

#[test]
fn oracle3_eval_source_round_trip() {
    const TOL: f64 = 1e-9;
    for case in corpus() {
        let b = cone_brep(case.apex, case.axis_dir, case.half_angle, case.height);
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
        let b = cone_brep(case.apex, case.axis_dir, case.half_angle, case.height);
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
// signed_distance_to_surface cone unit test (NEW cone arm)
// =========================================================================

#[test]
fn signed_distance_to_surface_cone_formula() {
    // 45° cone: apex = origin, axis = +z, half_angle = π/4 (tanα = 1). The
    // signed radial residual is `radial − |h_axial|·tanα = radial − |z|`.
    let cone = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };

    // On-surface (1,0,1): radial = 1, |z| = 1 → 0.
    let on = signed_distance_to_surface(cone, p(1.0, 0.0, 1.0)).expect("cone distance must be Ok");
    assert!((on - 0.0).abs() < 1e-12, "on-surface must be ≈ 0, got {on}");

    // Outside (2,0,1): radial = 2, |z| = 1 → +1.
    let out = signed_distance_to_surface(cone, p(2.0, 0.0, 1.0)).expect("cone distance must be Ok");
    assert!(
        (out - 1.0).abs() < 1e-12,
        "outside cone point must give +1, got {out}"
    );

    // Inside (0.5,0,1): radial = 0.5, |z| = 1 → −0.5.
    let inside =
        signed_distance_to_surface(cone, p(0.5, 0.0, 1.0)).expect("cone distance must be Ok");
    assert!(
        (inside - (-0.5)).abs() < 1e-12,
        "inside cone point must give −0.5, got {inside}"
    );
}

// =========================================================================
// eval_source cone-FACE-arm unit test (the pure apex-fan emits no BRepFace
// cone vertices, so this is the arm's only coverage). `ortho_basis` is private
// to the crate, so instead of comparing against the §5.2 point formula directly
// (which needs the private basis), we assert a basis-INDEPENDENT round-trip
// property: the returned point lies EXACTLY on the analytic cone (its own
// radial residual ≈ 0) AND its axial height equals `v`. This pins the cone FACE
// arm honestly without needing the private basis.
// =========================================================================

#[test]
fn eval_source_cone_face_arm_round_trip() {
    const TOL: f64 = 1e-9;
    // Off-axis non-unit case exercises normalization + ortho_basis.
    let apex = [1.0, 2.0, -1.0];
    let axis_dir = [1.0, 2.0, 2.0];
    let half_angle = (2.0_f64 / 4.0).atan(); // R = 2, height = 4
    let height = 4.0;
    let b = cone_brep(apex, axis_dir, half_angle, height);

    let axis_unit = unit(axis_dir);

    // Pick a concrete (u, v): u = some angle, v = axial height (0 < v < height).
    let u = 0.9_f64;
    let v = 2.5_f64;
    let pt = b
        .eval_source(TessellationSource::BRepFace { face: 0, u, v })
        .as_array();

    // Residual on the analytic cone: radial − v·tan(half_angle) ≈ 0.
    let w = sub(pt, apex);
    let h_axial = dot(w, axis_unit);
    let radial = norm(sub(w, scale(axis_unit, h_axial)));
    assert!(
        (radial - v * half_angle.tan()).abs() < TOL,
        "eval_source cone FACE arm: radial {radial} must equal v·tanα = {}, point {pt:?}",
        v * half_angle.tan()
    );
    // Axial height (p − apex)·â ≈ v.
    assert!(
        (h_axial - v).abs() < TOL,
        "eval_source cone FACE arm: axial height {h_axial} must equal v = {v}, point {pt:?}"
    );
}

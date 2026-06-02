//! PR-YR12 (P2b) RED — curved Stage-1 tessellation oracle for the SPHERE.
//!
//! Spec: `specs/yang_pr_yr12_sphere_tessellation.md`.
//!
//! Asserts that a closed solid-sphere B-Rep (one `Surface::Sphere` face bounded
//! by a single meridian seam Circle per spec §1) tessellates via `BRep::new`
//! into a watertight, error-bounded triangle mesh with a correct, invertible
//! `TessellationMap`. Four oracles:
//!
//! 1. **Surface-to-mesh distance ≤ d_ε** — no triangle bulges past the chord
//!    bound (`d_ε = 1e-2 × 2r√3`, the AABB space diagonal of the sphere,
//!    computed test-side from `radius` alone).
//! 2. **Watertight + 2-manifold** — every undirected edge shared by exactly
//!    two triangles (poles included). Plus an env-gated `inputcheck` arm
//!    (self-skips on `BinaryNotFound`).
//! 3. **Bijection round-trip** — `eval_source(map.lookup(v))` reproduces
//!    `mesh.verts[v]` for every mesh vertex (pole, seam, interior).
//! 4. **Euler** — `V − E + F = 2`.
//!
//! RED status: this file builds a `Surface::Sphere` B-Rep through `BRep::new`,
//! which GREEN will teach to tessellate. Until then `sphere_brep` panics on the
//! `.expect(...)` because production still returns `CurvedSurfaceNotYetSupported`.

use std::collections::BTreeMap;
use std::time::Duration;

use cad_primitives::{Point3, Vector3};
use cherchi_sidecar_rs::{inputcheck, SidecarError};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

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

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

// =========================================================================
// Sphere B-Rep fixture (spec §1: one Sphere face + one meridian seam Circle).
// =========================================================================

/// Build a closed solid-sphere B-Rep per spec §1.
///
/// - `v0` = south pole `center + r·(0,0,-1)`, `v1` = north pole
///   `center + r·(0,0,+1)`.
/// - `e0` meridian seam `Curve::Circle { center, normal: (0,-1,0), radius: r }`,
///   `start = v0` (south), `end = v1` (north). The seam lies in the X–Z plane.
/// - `f0` `Surface::Sphere { center, radius }`, `outer_loop = [e0]`,
///   `inner_loops = []`.
fn sphere_brep(center: [f64; 3], radius: f64) -> BRep {
    let south = add(center, scale([0.0, 0.0, -1.0], radius));
    let north = add(center, scale([0.0, 0.0, 1.0], radius));

    let verts = vec![
        BRepVertex {
            point: p(south[0], south[1], south[2]),
        },
        BRepVertex {
            point: p(north[0], north[1], north[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Circle {
            center: p(center[0], center[1], center[2]),
            normal: Vector3::new(0.0, -1.0, 0.0),
            radius,
        },
    }];

    let faces = vec![BRepFace {
        surface: Surface::Sphere {
            center: p(center[0], center[1], center[2]),
            radius,
        },
        outer_loop: vec![0],
        inner_loops: Vec::new(),
    }];

    BRep::new(verts, edges, faces).expect("sphere_brep: BRep::new should tessellate the sphere")
}

/// Test-side chord bound `d_ε = 1e-2 × (AABB space diagonal)`. A sphere of
/// radius `r` has AABB cube `[c−r, c+r]³`, whose space diagonal is `2r√3`
/// (spec §3). Computed from `radius` alone, independent of any production fn,
/// but identical in value to the GREEN production sizing (the only hard
/// requirement is that the two agree).
fn chord_bound(radius: f64) -> f64 {
    1e-2 * 2.0 * radius * 3f64.sqrt()
}

// =========================================================================
// The sphere corpus under test (all z-up; centers / radii vary).
// =========================================================================

struct SphereCase {
    name: &'static str,
    center: [f64; 3],
    radius: f64,
}

fn corpus() -> Vec<SphereCase> {
    vec![
        SphereCase {
            name: "unit at origin",
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        },
        SphereCase {
            name: "offset large-radius",
            center: [2.0, -1.0, 0.5],
            radius: 5.0,
        },
        SphereCase {
            name: "offset small-radius",
            center: [-3.0, 4.0, 1.0],
            radius: 0.3,
        },
    ]
}

// =========================================================================
// Oracle 1 — surface-to-mesh distance ≤ d_ε
// =========================================================================

#[test]
fn oracle1_mesh_within_chord_error_of_surface() {
    for case in corpus() {
        let b = sphere_brep(case.center, case.radius);
        let mesh = b.as_mesh();
        let d_eps = chord_bound(case.radius);
        assert!(d_eps > 0.0, "[{}] d_eps must be positive", case.name);

        for &tri in &mesh.tris {
            let a = mesh.verts[tri[0] as usize].as_array();
            let bb = mesh.verts[tri[1] as usize].as_array();
            let c = mesh.verts[tri[2] as usize].as_array();
            let centroid = scale(add(add(a, bb), c), 1.0 / 3.0);
            for sample in [a, bb, c, centroid] {
                let d = (norm(sub(sample, case.center)) - case.radius).abs();
                assert!(
                    d <= d_eps,
                    "[{}] tri {tri:?}: sample {sample:?} radial deviation {d} \
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
        let b = sphere_brep(case.center, case.radius);
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
        let b = sphere_brep(case.center, case.radius);
        let report = match inputcheck(b.as_mesh(), Duration::from_secs(30)) {
            Ok(r) => r,
            Err(SidecarError::BinaryNotFound { .. }) => {
                eprintln!("[yang-rs yr12] SKIP: inputcheck binary not found");
                return;
            }
            Err(e) => panic!("[{}] inputcheck failed unexpectedly: {e:?}", case.name),
        };
        assert!(
            report.all_pass(),
            "[{}] sphere Stage-1 mesh must pass all inputcheck axioms; got {report:?}",
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
        let b = sphere_brep(case.center, case.radius);
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
        let b = sphere_brep(case.center, case.radius);
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

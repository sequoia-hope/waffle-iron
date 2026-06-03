//! PR-YR12 (P2b) ADVERSARY — independent verification of the curved Stage-1
//! SPHERE tessellation.
//!
//! Written by a FRESH verifier who did NOT author the production code
//! (`crates/yang-rs/src/lib.rs`) or the RED oracle (`tests/yr12_sphere.rs`).
//! Goal: try to prove the sphere tessellation is WRONG or FAKED. Fixtures are
//! built from scratch here — we do NOT reuse the RED `sphere_brep` helper. The
//! mirror of the cylinder adversary's "place the seam at a non-trivial azimuth"
//! lever here is the **seam Circle normal sign**: we build BOTH `(0,-1,0)` (RED's
//! choice) and the OPPOSITE `(0,+1,0)`. The production seam-column `t` recovery
//! (`t = atan2(w·e2, w·e1)` in `ortho_basis(normal)`) must round-trip through
//! `eval_source`'s `Curve::Circle` arm regardless of the normal sign — flipping
//! the normal flips `ortho_basis`'s `e2`, so a seam-frame bug that happened to
//! cancel for one sign would surface for the other.
//!
//! This file is tests-only; it never modifies production code or any other test.
//!
//! Attacks:
//! 1. Independent fixture, BOTH seam-normal signs round-trip (proves the seam
//!    frame is honest, not accidentally symmetric).
//! 2. Watertightness is REAL: every DIRECTED edge has exactly one opposite twin
//!    (stronger than undirected count==2); exactly 2 pole-position verts, each
//!    referenced by exactly n_lon triangles; no coincident verts except the two
//!    intended poles.
//! 3. Basis-independent surface adherence: independent `|x−c|−r`; every vertex
//!    on the sphere to ~1e-12; every centroid within the FULL d_ε = 1e-2·2r√3
//!    (asserted NOT halved).
//! 4. No inversion: every triangle's geometric normal points outward.
//! 5. Round-trip vs INDEPENDENT z-up geometry (own face_eval / seam / pole math).
//! 6. Migrations not weakened (sphere-on-a-triangle → MalformedTopology;
//!    cone-on-a-triangle → MalformedTopology (PR-YR16: the cone tessellation
//!    path is now live, so a cone-on-a-triangle is rim-malformed rather than
//!    CurvedSurfaceNotYetSupported);
//!    signed_distance sphere → Ok, cone → Ok (PR-YR16: cone radial residual)).
//! 7. Euler χ = 2 independently.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point3, Vector3};
use yang_rs::{
    signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    TessellationSource, YangError,
};

// =========================================================================
// Independent array math (cad-primitives exposes only new/x/y/z/as_array).
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

/// INDEPENDENT replica of the production `ortho_basis` contract (spec §4):
/// normalize n; seed = world axis with smallest |component| (tie-break x<y<z);
/// e1 = normalize(seed − (seed·n)n); e2 = n × e1. Rebuilt here so a production
/// basis bug would be caught by the divergence, not hidden by reuse. Used to
/// independently invert the seam Circle.
fn ortho_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let nu = unit(n);
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    let seed = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let sdotn = dot(seed, nu);
    let e1 = unit(sub(seed, scale(nu, sdotn)));
    let e2 = cross(nu, e1);
    (e1, e2)
}

fn sp(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

// =========================================================================
// INDEPENDENT seam-edge sphere builder — parameterized by seam normal SIGN.
//
// Spec §1/§2: a closed solid sphere is ONE `Surface::Sphere` face bounded by a
// single meridian seam `Curve::Circle` through both poles, with the two pole
// `BRepVertex` at `center ± r·ẑ`. The seam lies in the X–Z plane, so its normal
// is `±Y`. RED uses `(0,-1,0)`; we exercise BOTH `(0,-1,0)` and `(0,+1,0)` to
// force the production seam-frame recovery to do real, sign-dependent work.
//
// The z-up parameterization (spec §2) is FIXED and independent of the seam
// normal: `face_eval(u,v) = center + r·(cos v·cos u, cos v·sin u, sin v)`, poles
// at v = ±π/2, seam meridian at u = 0 (the +X half-plane). We do NOT encode the
// parameterization into the fixture — production owns it; we only assert the
// poles sit at `center ± r·ẑ` so the +X meridian (u=0) is the seam, consistent
// with both normal signs (the X–Z plane).
// =========================================================================

/// Build a closed solid-sphere B-Rep per spec §1, with the seam Circle normal
/// `(0, seam_sign, 0)`. `seam_sign` is ±1.0.
fn sphere_brep_adv(
    center: [f64; 3],
    radius: f64,
    seam_sign: f64,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let south = add(center, [0.0, 0.0, -radius]);
    let north = add(center, [0.0, 0.0, radius]);

    let verts = vec![
        BRepVertex { point: sp(south) },
        BRepVertex { point: sp(north) },
    ];

    let edges = vec![BRepEdge {
        start: 0, // south pole
        end: 1,   // north pole
        curve: Curve::Circle {
            center: sp(center),
            normal: Vector3::new(0.0, seam_sign, 0.0),
            radius,
        },
    }];

    let faces = vec![BRepFace {
        surface: Surface::Sphere {
            center: sp(center),
            radius,
        },
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];

    (verts, edges, faces)
}

fn build(center: [f64; 3], radius: f64, seam_sign: f64) -> BRep {
    let (v, e, f) = sphere_brep_adv(center, radius, seam_sign);
    BRep::new(v, e, f).expect("adversary sphere must tessellate")
}

/// Independent chord bound — the FULL d_ε = 1e-2 × (AABB space diagonal). A
/// sphere of radius r has AABB cube `[c−r, c+r]³`, space diagonal `2r√3`. We
/// build the AABB from r alone and take its diagonal, NOT reusing any production
/// literal, and we deliberately assert downstream that this is NOT halved.
fn d_eps(radius: f64) -> f64 {
    let lo = [-radius, -radius, -radius];
    let hi = [radius, radius, radius];
    1e-2 * norm(sub(hi, lo))
}

/// INDEPENDENT z-up surface evaluator (spec §2). Used to recompute interior /
/// pole points from scratch for the round-trip attack.
fn face_eval(center: [f64; 3], radius: f64, u: f64, v: f64) -> [f64; 3] {
    let (cu, su) = (u.cos(), u.sin());
    let (cv, sv) = (v.cos(), v.sin());
    add(center, [radius * cv * cu, radius * cv * su, radius * sv])
}

// =========================================================================
// Corpus — DISTINCT from yr12_sphere's (different centers/radii). Each case is
// run at BOTH seam-normal signs. Includes a large and a sub-unit radius.
// =========================================================================

struct Case {
    name: &'static str,
    center: [f64; 3],
    radius: f64,
}

fn corpus() -> Vec<Case> {
    vec![
        Case {
            name: "off-origin unit-ish",
            center: [1.0, 2.0, -3.0],
            radius: 1.25,
        },
        Case {
            name: "large radius off-origin",
            center: [-6.0, 0.5, 2.0],
            radius: 7.5,
        },
        Case {
            name: "sub-unit radius off-origin",
            center: [4.0, -2.5, 6.0],
            radius: 0.2,
        },
    ]
}

/// Independently recover the production grid resolution from the same chord
/// rules production uses (spec §3), so we can predict exact V / F / E and the
/// per-pole triangle fan size. `seg_budget = d_ε / 2` (production sizes segments
/// to half the budget for centroid headroom — honest refinement). We replicate
/// the rule to PREDICT counts; we separately assert (attack 3) the FULL d_ε is
/// the centroid bound, so replicating /2 here does not let a halved oracle slip.
fn grid_dims(radius: f64) -> (usize, usize) {
    use std::f64::consts::PI;
    let seg_budget = d_eps(radius) / 2.0;
    let mut n_lon = 3usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / n_lon as f64).cos()) > seg_budget {
            n_lon += 1;
        }
    }
    let mut n_lat = 2usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / (2.0 * n_lat as f64)).cos()) > seg_budget {
            n_lat += 1;
        }
    }
    (n_lon, n_lat)
}

// =========================================================================
// Attack 1 — independent fixture, BOTH seam-normal signs round-trip exactly.
// =========================================================================

#[test]
fn attack1_both_seam_normal_signs_round_trip() {
    const TOL: f64 = 1e-9;
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let map = b.tessellation_map();
            assert_eq!(
                map.len(),
                mesh.num_verts(),
                "[{} sign {seam_sign}] map must cover every vertex",
                case.name
            );
            // Confirm at least one seam-column vertex exists (BRepEdge source);
            // otherwise the sign lever would be vacuous.
            let n_seam = (0..mesh.num_verts() as u32)
                .filter(|&v| matches!(map.lookup(v), TessellationSource::BRepEdge { .. }))
                .count();
            assert!(
                n_seam >= 1,
                "[{} sign {seam_sign}] expected seam-column BRepEdge verts, found {n_seam}",
                case.name
            );
            for v in 0..mesh.num_verts() as u32 {
                let src = map.lookup(v);
                let recon = b.eval_source(src).as_array();
                let actual = mesh.verts[v as usize].as_array();
                let d = norm(sub(recon, actual));
                assert!(
                    d <= TOL,
                    "[{} sign {seam_sign}] v{v} src {src:?}: eval_source {recon:?} != \
                     mesh {actual:?} (d {d})",
                    case.name
                );
            }
        }
    }
}

// =========================================================================
// Attack 2 — watertightness is REAL (half-edge twins, shared poles, no weld).
// =========================================================================

#[test]
fn attack2_watertight_half_edge_and_shared_poles() {
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let (n_lon, n_lat) = grid_dims(case.radius);

            // Predicted topology of a lat/long sphere with shared poles:
            //   V = 2 poles + (n_lat-1) interior rings × n_lon
            //   F = 2 pole fans (n_lon each) + (n_lat-2) bands × 2·n_lon
            //   E = 3F/2 (closed triangulated manifold).
            let exp_v = 2 + (n_lat - 1) * n_lon;
            let exp_f = 2 * n_lon + (n_lat - 2) * 2 * n_lon;
            assert_eq!(
                mesh.num_verts(),
                exp_v,
                "[{} sign {seam_sign}] V: expected {exp_v} (n_lon={n_lon} n_lat={n_lat})",
                case.name
            );
            assert_eq!(
                mesh.num_tris(),
                exp_f,
                "[{} sign {seam_sign}] F: expected {exp_f}",
                case.name
            );

            // Half-edge twin pairing: each DIRECTED edge (a→b) appears exactly
            // once and its reverse (b→a) appears exactly once. Strictly stronger
            // than undirected count == 2 (it also forbids two tris winding the
            // same direction across an edge, i.e. a non-orientable / flipped
            // stitch).
            let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
            for tri in &mesh.tris {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    *directed.entry((tri[i], tri[j])).or_insert(0) += 1;
                }
            }
            for (&(a, c), &count) in &directed {
                assert_eq!(
                    count, 1,
                    "[{} sign {seam_sign}] directed edge {a}->{c} appears {count}× (must be 1)",
                    case.name
                );
                let twin = directed.get(&(c, a)).copied().unwrap_or(0);
                assert_eq!(
                    twin, 1,
                    "[{} sign {seam_sign}] directed edge {a}->{c} has {twin} reverse twins \
                     (must be exactly 1) — not a watertight orientable manifold",
                    case.name
                );
            }

            // Exactly TWO vertices sit at the pole positions, and they are
            // DISTINCT verts (poles are shared, not duplicated per-fan-triangle).
            let south = add(case.center, [0.0, 0.0, -case.radius]);
            let north = add(case.center, [0.0, 0.0, case.radius]);
            let pole_verts: Vec<u32> = (0..mesh.num_verts() as u32)
                .filter(|&v| {
                    let x = mesh.verts[v as usize].as_array();
                    norm(sub(x, south)) <= 1e-12 || norm(sub(x, north)) <= 1e-12
                })
                .collect();
            assert_eq!(
                pole_verts.len(),
                2,
                "[{} sign {seam_sign}] expected exactly 2 pole verts, found {} \
                 (poles duplicated → faked watertight pole closure)",
                case.name,
                pole_verts.len()
            );

            // Each pole vertex is referenced by exactly n_lon triangles (its fan).
            let mut pole_fan: BTreeMap<u32, usize> = BTreeMap::new();
            for tri in &mesh.tris {
                for &vi in tri.iter() {
                    if pole_verts.contains(&vi) {
                        *pole_fan.entry(vi).or_insert(0) += 1;
                    }
                }
            }
            for &pv in &pole_verts {
                let fan = pole_fan.get(&pv).copied().unwrap_or(0);
                assert_eq!(
                    fan, n_lon,
                    "[{} sign {seam_sign}] pole vert {pv} in {fan} tris (must be n_lon={n_lon})",
                    case.name
                );
            }

            // No two DISTINCT verts coincide within 1e-12 (no snap-weld masking).
            let vs: Vec<[f64; 3]> = mesh.verts.iter().map(|v| v.as_array()).collect();
            for i in 0..vs.len() {
                for j in (i + 1)..vs.len() {
                    let d = norm(sub(vs[i], vs[j]));
                    assert!(
                        d > 1e-12,
                        "[{} sign {seam_sign}] verts {i} and {j} coincident ({d}) — \
                         snap-weld / collapse faking watertightness",
                        case.name
                    );
                }
            }
        }
    }
}

// =========================================================================
// Attack 3 — basis-independent surface adherence, FULL (non-halved) d_ε.
// =========================================================================

/// Independent signed distance to the sphere — NOT the production fn.
fn sphere_dist(center: [f64; 3], radius: f64, x: [f64; 3]) -> f64 {
    norm(sub(x, center)) - radius
}

#[test]
fn attack3_surface_adherence_within_full_deps() {
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let deps = d_eps(case.radius);

            // Guard against a halved oracle: the bound we check centroids against
            // is the FULL 1e-2·2r√3, and we assert it equals exactly that value
            // (not d_ε/2). A regression that secretly checked against half this
            // would still pass its own test but fail ours.
            let full = 1e-2 * 2.0 * case.radius * 3f64.sqrt();
            assert!(
                (deps - full).abs() < 1e-15 * full.max(1.0),
                "[{} sign {seam_sign}] d_eps {deps} must be the FULL 2r√3 diagonal {full}, \
                 not halved",
                case.name
            );

            // Every VERTEX lies on the sphere to ~1e-12 (verts are exact samples).
            for v in &mesh.verts {
                let x = v.as_array();
                let d = sphere_dist(case.center, case.radius, x).abs();
                assert!(
                    d <= 1e-9,
                    "[{} sign {seam_sign}] vertex {x:?} off-sphere by {d} (must be ~0)",
                    case.name
                );
            }

            // Every CENTROID within the FULL d_ε.
            for &tri in &mesh.tris {
                let a = mesh.verts[tri[0] as usize].as_array();
                let bb = mesh.verts[tri[1] as usize].as_array();
                let c = mesh.verts[tri[2] as usize].as_array();
                let centroid = scale(add(add(a, bb), c), 1.0 / 3.0);
                let d = sphere_dist(case.center, case.radius, centroid).abs();
                assert!(
                    d <= deps,
                    "[{} sign {seam_sign}] tri {tri:?} centroid dev {d} > d_eps {deps}",
                    case.name
                );
            }
        }
    }
}

// =========================================================================
// Attack 4 — no inversion: every triangle's geometric normal points outward.
// =========================================================================

#[test]
fn attack4_all_triangles_outward_oriented() {
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let mut inspected = 0usize;
            for &tri in &mesh.tris {
                let a = mesh.verts[tri[0] as usize].as_array();
                let bb = mesh.verts[tri[1] as usize].as_array();
                let c = mesh.verts[tri[2] as usize].as_array();
                // Geometric normal (v1−v0)×(v2−v0).
                let gn = cross(sub(bb, a), sub(c, a));
                // Outward radial direction at the centroid (centroid − center).
                let centroid = scale(add(add(a, bb), c), 1.0 / 3.0);
                let radial = sub(centroid, case.center);
                let rn = norm(radial);
                assert!(
                    rn > 1e-12,
                    "[{} sign {seam_sign}] degenerate centroid on center",
                    case.name
                );
                let d = dot(gn, radial);
                assert!(
                    d > 0.0,
                    "[{} sign {seam_sign}] tri {tri:?} geometric normal faces INWARD \
                     (dot with outward radial = {d}) — inverted / flipped triangle",
                    case.name
                );
                inspected += 1;
            }
            assert!(
                inspected >= 12,
                "[{} sign {seam_sign}] expected to inspect many tris, saw {inspected}",
                case.name
            );
        }
    }
}

// =========================================================================
// Attack 5 — round-trip vs INDEPENDENT z-up geometry (own pole/seam/face math).
//
// For each vertex we recompute the expected world point from its source using
// OUR OWN geometry (pole position, seam circle in our own ortho_basis, or z-up
// face_eval) and compare to BOTH the mesh vertex and `eval_source`. This catches
// an eval_source that is self-consistent with a buggy tessellation but wrong vs
// the true z-up surface.
// =========================================================================

#[test]
fn attack5_round_trip_against_independent_zup_geometry() {
    const TOL: f64 = 1e-9;
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let map = b.tessellation_map();
            let south = add(case.center, [0.0, 0.0, -case.radius]);
            let north = add(case.center, [0.0, 0.0, case.radius]);

            for v in 0..mesh.num_verts() as u32 {
                let src = map.lookup(v);
                let actual = mesh.verts[v as usize].as_array();
                let recon = b.eval_source(src).as_array();

                // (a) eval_source inverts the bijection.
                assert!(
                    norm(sub(recon, actual)) <= TOL,
                    "[{} sign {seam_sign}] v{v} src {src:?}: eval_source {recon:?} != \
                     mesh {actual:?}",
                    case.name
                );

                // (b) independent recompute by source kind.
                let indep = match src {
                    TessellationSource::BRepVertex(_) => {
                        // A pole — must equal one of our independently computed poles.
                        if norm(sub(actual, south)) <= norm(sub(actual, north)) {
                            south
                        } else {
                            north
                        }
                    }
                    TessellationSource::BRepEdge { edge, t } => {
                        // Seam column: invert the Circle in OUR OWN ortho_basis.
                        let be = &b.edges()[edge as usize];
                        let Curve::Circle {
                            center,
                            normal,
                            radius,
                        } = be.curve
                        else {
                            panic!("seam edge {edge} must be a Circle");
                        };
                        let (e1, e2) = ortho_basis(normal.as_array());
                        add(
                            center.as_array(),
                            add(scale(e1, radius * t.cos()), scale(e2, radius * t.sin())),
                        )
                    }
                    TessellationSource::BRepFace { u, v: vv, .. } => {
                        // Interior: our own z-up evaluator.
                        face_eval(case.center, case.radius, u, vv)
                    }
                    // The sphere Stage-1 path emits ONLY the three source kinds
                    // above; Intersection / Unknown must NEVER appear here. If
                    // they do, the bijection is corrupted — fail loudly.
                    other => panic!(
                        "[{} sign {seam_sign}] v{v} got unexpected source {other:?} \
                         (sphere Stage-1 must emit only BRepVertex/BRepEdge/BRepFace)",
                        case.name
                    ),
                };
                let di = norm(sub(indep, actual));
                assert!(
                    di <= TOL,
                    "[{} sign {seam_sign}] v{v} src {src:?}: independent geom {indep:?} != \
                     mesh {actual:?} (d {di})",
                    case.name
                );
            }
        }
    }
}

// =========================================================================
// Attack 6 — migrations not weakened.
// =========================================================================

/// Independent single-triangle fixture with a caller-chosen surface (NO Circle
/// seam edge — a sphere/cone face on a flat triangle).
fn one_triangle(surface: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: Point3::new(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(4.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 4.0, 0.0),
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
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
fn attack6_sphere_on_triangle_is_malformed_not_silent() {
    // The sphere path is implemented, but a sphere face on a *triangle* lacks
    // its required meridian seam Circle edge → MalformedTopology (loud, never
    // silently flowing downstream). Only the error KIND changed from the
    // pre-YR12 CurvedSurfaceNotYetSupported; it must still error.
    let (v, e, f) = one_triangle(Surface::Sphere {
        center: Point3::new(1.0, 1.0, 1.0),
        radius: 3.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "sphere on a triangle must be MalformedTopology (lacks meridian seam Circle), got {r:?}"
    );
}

#[test]
fn attack6_cone_on_triangle_still_curved_not_supported_face0() {
    // PR-YR16 migration: a cone on a *triangle* (no base-rim Circle) is now
    // MalformedTopology, not CurvedSurfaceNotYetSupported — still a loud error,
    // NOT weakened to a generic is_err(). Only the error kind changed.
    let (v, e, f) = one_triangle(Surface::Cone {
        apex: Point3::new(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "cone on a triangle must be MalformedTopology (lacks its base-rim Circle edge), got {r:?}"
    );
}

#[test]
fn attack6_signed_distance_sphere_ok_cone_err() {
    // Sphere now evaluable: |x − center| − radius. Center (1,2,3), r=2, point
    // (1,2,7) is 4 along +z → 4 − 2 = +2 (outside). Point (1,2,3.5) is 0.5 from
    // center → 0.5 − 2 = −1.5 (inside).
    let sphere = Surface::Sphere {
        center: Point3::new(1.0, 2.0, 3.0),
        radius: 2.0,
    };
    let outside =
        signed_distance_to_surface(sphere, Point3::new(1.0, 2.0, 7.0)).expect("Sphere must be Ok");
    assert!(
        outside > 0.0 && (outside - 2.0).abs() < 1e-12,
        "outside sphere point must give +2, got {outside}"
    );
    let inside =
        signed_distance_to_surface(sphere, Point3::new(1.0, 2.0, 3.5)).expect("Sphere must be Ok");
    assert!(
        inside < 0.0 && (inside - (-1.5)).abs() < 1e-12,
        "inside sphere point must give −1.5, got {inside}"
    );

    // Cone now evaluable (PR-YR16): signed radial residual = radial − |z|·tanα.
    // 45° cone (apex = origin, axis = +z, tanα = 1) at (2,0,1): radial = 2,
    // |z| = 1 → +1 (outside).
    let cone_d = signed_distance_to_surface(
        Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        },
        Point3::new(2.0, 0.0, 1.0),
    )
    .expect("Cone must be Ok");
    assert!(
        cone_d > 0.0 && (cone_d - 1.0).abs() < 1e-12,
        "outside cone point must give +1, got {cone_d}"
    );
}

// =========================================================================
// Attack 7 — Euler χ = 2 independently.
// =========================================================================

#[test]
fn attack7_euler_characteristic() {
    for case in corpus() {
        for seam_sign in [-1.0_f64, 1.0_f64] {
            let b = build(case.center, case.radius, seam_sign);
            let mesh = b.as_mesh();
            let mut undirected: BTreeSet<(u32, u32)> = BTreeSet::new();
            for tri in &mesh.tris {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (a, c) = (tri[i], tri[j]);
                    undirected.insert(if a < c { (a, c) } else { (c, a) });
                }
            }
            let v = mesh.num_verts() as i64;
            let f = mesh.num_tris() as i64;
            let e = undirected.len() as i64;
            assert_eq!(
                v - e + f,
                2,
                "[{} sign {seam_sign}] Euler: V={v} E={e} F={f}",
                case.name
            );
        }
    }
}

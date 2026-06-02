//! PR-YR7 (P2a) ADVERSARY — independent verification of the curved Stage-1
//! cylinder tessellation.
//!
//! Written by a FRESH verifier who did NOT author the production code
//! (`crates/yang-rs/src/lib.rs`) or the RED oracle (`tests/yr7_cylinder.rs`).
//! Goal: try to prove the cylinder tessellation is WRONG or FAKED. Fixtures are
//! built from scratch here — we do NOT reuse the RED `cylinder_brep` helper, and
//! crucially we place the seam vertex at a DIFFERENT azimuth than RED (RED puts
//! angle-0 at `+r·e1`; we deliberately offset the seam by a non-trivial angle)
//! so the production seam-angle-recovery path (`phi0 = atan2`) is genuinely
//! exercised, not bypassed.
//!
//! This file is tests-only; it never modifies production code or the existing
//! RED / YR6 tests.
//!
//! Attacks:
//! 1. Watertightness is REAL, not snap-welded: exactly `2N+2` DISTINCT vertices,
//!    no two coincident, every undirected edge shared by exactly 2 tris, AND
//!    cap+lateral genuinely SHARE rim vertex indices.
//! 2. Off-axis, non-unit `axis_dir`, off-origin: surface distance ≤ d_ε (our own
//!    d_ε), 2-manifold, round-trip, Euler.
//! 3. Bijection round-trip via INDEPENDENT geometry (vertex lies on the analytic
//!    surface — basis-free — plus eval_source consistency).
//! 4. Surface adherence is basis-independent (every vertex on lateral OR a cap).
//! 5. No twist across the lateral (radial-outward geometric normals).
//! 6. Migrations not weakened (cylinder-on-a-triangle → MalformedTopology;
//!    wrong rim count → loud; Sphere/Cone → CurvedSurfaceNotYetSupported).
//! 7. signed_distance_to_surface sign sanity.

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

/// Perpendicular distance from `x` to the infinite line through `axis_point`
/// with unit direction `axis_unit`.
fn dist_point_to_line(x: [f64; 3], axis_point: [f64; 3], axis_unit: [f64; 3]) -> f64 {
    let w = sub(x, axis_point);
    let along = dot(w, axis_unit);
    norm(sub(w, scale(axis_unit, along)))
}

/// INDEPENDENT replica of the production `ortho_basis` contract (spec §4):
/// normalize n; seed = world axis with smallest |component| (tie-break x<y<z);
/// e1 = normalize(seed − (seed·n)n); e2 = n × e1. We rebuild it ourselves so a
/// production basis bug would be caught by the divergence, not hidden by reuse.
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

// =========================================================================
// INDEPENDENT seam-edge cylinder builder — DIFFERENT seam azimuth than RED.
//
// RED places the bottom/top seam at angle 0 of e1 (`+r·e1`). We deliberately
// place the seam at world azimuth `SEAM_AZIMUTH` (a non-zero, non-axis-aligned
// angle), so the production code's `phi0 = atan2(...)` seam-angle recovery is
// forced to do real work rather than landing on the trivial angle 0. The rim
// circle is parameterized in `ortho_basis(rim_normal)`; the bottom rim normal
// is −axis_unit, the top rim normal is +axis_unit (matching spec §1), so the
// two rims' e2 axes are opposite — exactly the twist the production (N−k)
// mapping must compensate.
//
// CRITICAL ENCODING INVARIANT (spec §1/§6): the two seam vertices both lie on
// the cylinder's single axial seam LINE, i.e. at the SAME WORLD azimuth. The
// production seam-alignment treats each rim's `ring[0]` (its seam) as the
// reference for the (N−k) opposite-rim mapping; that only aligns the quads if
// both seams share a world azimuth. RED satisfies this by placing both seams at
// `+r·e1` (world e1, azimuth 0 in BOTH frames since e1 is shared). We choose a
// DIFFERENT, non-zero world azimuth `SEAM_AZIMUTH` (so the production `phi0 =
// atan2` recovery does real work) but still keep BOTH seams on that one world
// direction: the bottom rim's normal is −axis, so the same world point sits at
// angle `−SEAM_AZIMUTH` in the bottom frame.
// =========================================================================

const SEAM_AZIMUTH: f64 = 1.0; // radians; arbitrary non-zero, non-π/2 world angle.

/// Point at azimuth `theta` on the circle (center, normal, radius) in
/// `ortho_basis(normal)`.
fn circle_point(center: [f64; 3], normal: [f64; 3], radius: f64, theta: f64) -> [f64; 3] {
    let (e1, e2) = ortho_basis(normal);
    add(
        center,
        add(
            scale(e1, radius * theta.cos()),
            scale(e2, radius * theta.sin()),
        ),
    )
}

fn cyl_p(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

/// Build a closed solid-cylinder B-Rep with both seam vertices on the SAME
/// world azimuth `SEAM_AZIMUTH` (measured in the top rim's frame). Because the
/// bottom rim's normal is the opposite of the top's, that identical world
/// direction is angle `−SEAM_AZIMUTH` in the bottom rim's own frame — production
/// recovers each angle independently via atan2, so both seams remain on one
/// axial line (the encoding contract) while sitting at a non-trivial azimuth.
fn adv_cylinder_brep(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));
    let neg_axis = scale(axis_unit, -1.0);

    // Top seam at SEAM_AZIMUTH in the top frame; bottom seam at the SAME world
    // direction, which is −SEAM_AZIMUTH in the bottom (opposite-normal) frame.
    let v1 = circle_point(top_center, axis_unit, radius, SEAM_AZIMUTH);
    let v0 = circle_point(bottom_center, neg_axis, radius, -SEAM_AZIMUTH);

    let verts = vec![
        BRepVertex { point: cyl_p(v0) },
        BRepVertex { point: cyl_p(v1) },
    ];

    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: cyl_p(bottom_center),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: cyl_p(top_center),
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
                axis_point: cyl_p(axis_point),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
        },
    ];

    (verts, edges, faces)
}

fn build(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let (v, e, f) = adv_cylinder_brep(axis_point, axis_dir, radius, height);
    BRep::new(v, e, f).expect("adversary cylinder must tessellate")
}

/// Independent analytic d_ε = 1e-2 × AABB diagonal, AABB from the two rim
/// circles' exact extents (spec §3): per axis a circle of center c, unit normal
/// n, radius r spans c_i ± r·√(max(0, 1 − n_i²)).
fn d_eps(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> f64 {
    let au = unit(axis_dir);
    let bottom = axis_point;
    let top = add(axis_point, scale(au, height));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in [bottom, top] {
        for i in 0..3 {
            let span = radius * (1.0 - au[i] * au[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(c[i] - span);
            hi[i] = hi[i].max(c[i] + span);
        }
    }
    1e-2 * norm(sub(hi, lo))
}

// =========================================================================
// Corpus — every case off-origin, including an off-axis NON-UNIT axis_dir.
// =========================================================================

struct Case {
    name: &'static str,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
}

fn corpus() -> Vec<Case> {
    vec![
        Case {
            name: "z-up off-origin",
            axis_point: [3.0, -2.0, 4.0],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 1.5,
            height: 2.0,
        },
        Case {
            // The headline adversarial case: tilted, NON-UNIT (magnitude 3)
            // axis_dir, off-origin axis_point.
            name: "off-axis non-unit (mag 3)",
            axis_point: [5.0, -3.0, 1.0],
            axis_dir: [2.0, -1.0, 2.0],
            radius: 2.0,
            height: 4.0,
        },
        Case {
            name: "y-axis thin-tall off-origin",
            axis_point: [-4.0, 1.0, 2.0],
            axis_dir: [0.0, 1.0, 0.0],
            radius: 0.4,
            height: 6.0,
        },
    ]
}

// =========================================================================
// Attack 1 — watertightness is REAL (2N+2 distinct, no weld, shared indices)
// =========================================================================

/// Recover N (the production segment count) independently from the chord-error
/// rule: smallest N ≥ 3 with r·(1 − cos(π/N)) ≤ d_ε.
fn expected_n(radius: f64, deps: f64) -> usize {
    let mut n = 3usize;
    if deps > 0.0 {
        while radius * (1.0 - (std::f64::consts::PI / n as f64).cos()) > deps {
            n += 1;
        }
    }
    n
}

#[test]
fn attack1_watertight_is_real_not_snap_welded() {
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let deps = d_eps(case.axis_point, case.axis_dir, case.radius, case.height);
        let n = expected_n(case.radius, deps);

        // Expected mesh totals: V = 2N + 2, F = 4N, E = 6N.
        assert_eq!(
            mesh.num_verts(),
            2 * n + 2,
            "[{}] expected 2N+2 = {} distinct verts (N={n})",
            case.name,
            2 * n + 2
        );
        assert_eq!(
            mesh.num_tris(),
            4 * n,
            "[{}] expected 4N = {} tris (N={n})",
            case.name,
            4 * n
        );

        // No two DISTINCT vertices are coincident — watertightness is NOT faked
        // by snap-welding two near-identical verts into one location.
        let vs: Vec<[f64; 3]> = mesh.verts.iter().map(|v| v.as_array()).collect();
        for i in 0..vs.len() {
            for j in (i + 1)..vs.len() {
                let d = norm(sub(vs[i], vs[j]));
                assert!(
                    d > 1e-9,
                    "[{}] verts {i} and {j} are coincident ({d}) — \
                     snap-weld / degenerate collapse faking watertightness",
                    case.name
                );
            }
        }

        // Every undirected edge shared by exactly 2 triangles.
        let mut ec: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for tri in &mesh.tris {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (a, c) = (tri[i], tri[j]);
                let key = if a < c { (a, c) } else { (c, a) };
                *ec.entry(key).or_insert(0) += 1;
            }
        }
        for (edge, count) in &ec {
            assert_eq!(
                *count, 2,
                "[{}] undirected edge {edge:?} shared by {count} tris (must be 2)",
                case.name
            );
        }

        // The watertight seam is via SHARED INDICES, not two coincident verts:
        // find rim vertices (source = BRepEdge), and confirm each is referenced
        // by BOTH a lateral triangle and a cap triangle. We classify each
        // triangle (lateral vs cap) basis-independently by surface adherence.
        let map = b.tessellation_map();
        let rim_verts: BTreeSet<u32> = (0..mesh.num_verts() as u32)
            .filter(|&v| matches!(map.lookup(v), TessellationSource::BRepEdge { .. }))
            .collect();
        // ring[0] (the seam) keeps a BRepVertex source but is still a rim vertex
        // geometrically; include the two BRepVertex seams as rim verts too.
        let seam_verts: BTreeSet<u32> = (0..mesh.num_verts() as u32)
            .filter(|&v| matches!(map.lookup(v), TessellationSource::BRepVertex(_)))
            .collect();
        let all_rim: BTreeSet<u32> = rim_verts.union(&seam_verts).copied().collect();
        // 2N rim verts total (N per rim, 2 of which are the BRepVertex seams).
        assert_eq!(
            all_rim.len(),
            2 * n,
            "[{}] expected 2N = {} rim verts, found {}",
            case.name,
            2 * n,
            all_rim.len()
        );

        let surfaces = surfaces_for(&case);
        let mut rim_in_lateral: BTreeSet<u32> = BTreeSet::new();
        let mut rim_in_cap: BTreeSet<u32> = BTreeSet::new();
        for tri in &mesh.tris {
            let fi = classify(&mesh.verts, *tri, &surfaces, deps);
            for &vi in tri.iter() {
                if all_rim.contains(&vi) {
                    if fi == 0 {
                        rim_in_lateral.insert(vi);
                    } else {
                        rim_in_cap.insert(vi);
                    }
                }
            }
        }
        // Every rim vertex must appear in BOTH a lateral and a cap triangle —
        // proving the shared-index seam (not coincident-but-distinct verts).
        for &vi in &all_rim {
            assert!(
                rim_in_lateral.contains(&vi),
                "[{}] rim vertex {vi} never used by a lateral triangle",
                case.name
            );
            assert!(
                rim_in_cap.contains(&vi),
                "[{}] rim vertex {vi} never used by a cap triangle — \
                 cap+lateral do NOT share this rim index",
                case.name
            );
        }
    }
}

// =========================================================================
// Shared classification (basis-independent surface adherence).
// =========================================================================

fn surfaces_for(case: &Case) -> Vec<(usize, Surface)> {
    let au = unit(case.axis_dir);
    let bottom = case.axis_point;
    let top = add(bottom, scale(au, case.height));
    let neg = scale(au, -1.0);
    vec![
        (
            0,
            Surface::Cylinder {
                axis_point: cyl_p(case.axis_point),
                axis_dir: Vector3::new(case.axis_dir[0], case.axis_dir[1], case.axis_dir[2]),
                radius: case.radius,
            },
        ),
        (
            1,
            Surface::Plane {
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                d: -dot(neg, bottom),
            },
        ),
        (
            2,
            Surface::Plane {
                normal: Vector3::new(au[0], au[1], au[2]),
                d: -dot(au, top),
            },
        ),
    ]
}

/// Independent surface distance (NOT the production fn): plane n·x+d (unit
/// normals in fixture); cylinder dist(x,axis)−r.
fn sdist(surface: Surface, x: [f64; 3]) -> f64 {
    match surface {
        Surface::Plane { normal, d } => dot(normal.as_array(), x) + d,
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => dist_point_to_line(x, axis_point.as_array(), unit(axis_dir.as_array())) - radius,
        other => panic!("unexpected surface {other:?}"),
    }
}

/// Classify a triangle to the unique surface ALL three verts lie near.
fn classify(verts: &[Point3], tri: [u32; 3], surfaces: &[(usize, Surface)], tol: f64) -> usize {
    let pts = [
        verts[tri[0] as usize].as_array(),
        verts[tri[1] as usize].as_array(),
        verts[tri[2] as usize].as_array(),
    ];
    let mut matched = Vec::new();
    for (fi, surf) in surfaces {
        if pts.iter().all(|&x| sdist(*surf, x).abs() <= tol) {
            matched.push(*fi);
        }
    }
    assert_eq!(
        matched.len(),
        1,
        "triangle {tri:?} must lie near exactly one surface (tol {tol}), matched {matched:?}"
    );
    matched[0]
}

// =========================================================================
// Attack 2 — off-axis/non-unit: distance ≤ d_ε, 2-manifold, round-trip, Euler.
// =========================================================================

#[test]
fn attack2_surface_distance_within_own_deps() {
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let deps = d_eps(case.axis_point, case.axis_dir, case.radius, case.height);
        assert!(deps > 0.0, "[{}] d_eps must be positive", case.name);
        let surfaces = surfaces_for(&case);

        for &tri in &mesh.tris {
            let fi = classify(&mesh.verts, tri, &surfaces, deps);
            let surf = surfaces[fi].1;
            let a = mesh.verts[tri[0] as usize].as_array();
            let bb = mesh.verts[tri[1] as usize].as_array();
            let c = mesh.verts[tri[2] as usize].as_array();
            let centroid = scale(add(add(a, bb), c), 1.0 / 3.0);
            for s in [a, bb, c, centroid] {
                let d = sdist(surf, s).abs();
                assert!(
                    d <= deps,
                    "[{}] tri {tri:?} face {fi}: sample {s:?} dist {d} > d_eps {deps}",
                    case.name
                );
            }
        }
    }
}

#[test]
fn attack2_euler_characteristic() {
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
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
        assert_eq!(v - e + f, 2, "[{}] Euler: V={v} E={e} F={f}", case.name);
    }
}

// =========================================================================
// Attack 3 — round-trip via INDEPENDENT geometry.
//
// For every vertex: (a) eval_source reproduces mesh.verts[v]; (b) the vertex
// lies ON the correct analytic surface (basis-free), which catches an
// eval_source that is self-consistent with a buggy tessellation but wrong vs
// true geometry. Where the source is a Circle BRepEdge, we ALSO recompute the
// point from our own independent ortho_basis + circle formula and check it.
// =========================================================================

#[test]
fn attack3_round_trip_independent_geometry() {
    const TOL: f64 = 1e-9;
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let map = b.tessellation_map();
        assert_eq!(
            map.len(),
            mesh.num_verts(),
            "[{}] map covers all",
            case.name
        );

        for v in 0..mesh.num_verts() as u32 {
            let src = map.lookup(v);
            let actual = mesh.verts[v as usize].as_array();

            // (a) eval_source is the inverse of the bijection.
            let recon = b.eval_source(src).as_array();
            let da = norm(sub(recon, actual));
            assert!(
                da <= TOL,
                "[{}] v{v} src {src:?}: eval_source {recon:?} != mesh {actual:?} (d {da})",
                case.name
            );

            // (b) independent recompute for Circle edge sources, using OUR OWN
            // ortho_basis + circle formula (must match production basis to
            // round-trip; if production basis differed, this would catch it).
            if let TessellationSource::BRepEdge { edge, t } = src {
                let be = &b.edges()[edge as usize];
                if let Curve::Circle {
                    center,
                    normal,
                    radius,
                } = be.curve
                {
                    let indep = circle_point(center.as_array(), normal.as_array(), radius, t);
                    let di = norm(sub(indep, actual));
                    assert!(
                        di <= TOL,
                        "[{}] v{v} circle edge {edge} t={t}: independent {indep:?} != \
                         mesh {actual:?} (d {di})",
                        case.name
                    );
                }
            }
        }
    }
}

// =========================================================================
// Attack 4 — basis-independent surface adherence (every vert on the cyl set).
// =========================================================================

#[test]
fn attack4_every_vertex_on_cylinder_surface_set() {
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let deps = d_eps(case.axis_point, case.axis_dir, case.radius, case.height);
        let au = unit(case.axis_dir);
        let bottom = case.axis_point;
        let top = add(bottom, scale(au, case.height));
        // Cap plane offsets along the axis.
        let proj = |x: [f64; 3]| dot(sub(x, case.axis_point), au);
        let bottom_off = proj(bottom);
        let top_off = proj(top);

        for v in &mesh.verts {
            let x = v.as_array();
            // On the lateral?
            let lat = (dist_point_to_line(x, case.axis_point, au) - case.radius).abs() <= deps;
            // On a cap plane? (signed axial offset matches a cap, AND within
            // radius+d_ε of the axis line so a far co-planar point is rejected).
            let off = proj(x);
            let radial = dist_point_to_line(x, case.axis_point, au);
            let on_bottom = (off - bottom_off).abs() <= deps && radial <= case.radius + deps;
            let on_top = (off - top_off).abs() <= deps && radial <= case.radius + deps;
            assert!(
                lat || on_bottom || on_top,
                "[{}] vertex {x:?} lies on neither lateral nor a cap (d_eps {deps})",
                case.name
            );
        }
    }
}

// =========================================================================
// Attack 5 — no twist across the lateral (radial-outward geometric normals).
// =========================================================================

#[test]
fn attack5_lateral_band_not_twisted() {
    for case in corpus() {
        let b = build(case.axis_point, case.axis_dir, case.radius, case.height);
        let mesh = b.as_mesh();
        let deps = d_eps(case.axis_point, case.axis_dir, case.radius, case.height);
        let au = unit(case.axis_dir);
        let surfaces = surfaces_for(&case);

        let mut lateral_seen = 0usize;
        for &tri in &mesh.tris {
            if classify(&mesh.verts, tri, &surfaces, deps) != 0 {
                continue; // only lateral triangles
            }
            lateral_seen += 1;
            let a = mesh.verts[tri[0] as usize].as_array();
            let bb = mesh.verts[tri[1] as usize].as_array();
            let c = mesh.verts[tri[2] as usize].as_array();
            // Geometric normal (v1−v0)×(v2−v0).
            let gn = cross(sub(bb, a), sub(c, a));
            // Radial-outward at centroid.
            let cen = scale(add(add(a, bb), c), 1.0 / 3.0);
            let w = sub(cen, case.axis_point);
            let radial = sub(w, scale(au, dot(w, au)));
            let rn = norm(radial);
            assert!(rn > 1e-12, "[{}] lateral centroid on axis?", case.name);
            let d = dot(gn, radial);
            assert!(
                d > 0.0,
                "[{}] lateral tri {tri:?} geometric normal faces INWARD \
                 (dot with radial-outward = {d}) — band is twisted",
                case.name
            );
        }
        // Sanity: we actually inspected lateral triangles (2N of them).
        assert!(
            lateral_seen >= 6,
            "[{}] expected to inspect lateral triangles, saw {lateral_seen}",
            case.name
        );
    }
}

// =========================================================================
// Attack 6 — migrations not weakened.
// =========================================================================

/// Independent single-triangle fixture with a caller-chosen surface.
fn one_triangle(surface: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: Point3::new(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(3.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 3.0, 0.0),
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
fn attack6_cylinder_on_triangle_is_malformed_not_silent() {
    let (v, e, f) = one_triangle(Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "cylinder-on-a-triangle must be MalformedTopology (loud), got {r:?}"
    );
}

#[test]
fn attack6_sphere_malformed_cone_still_curved_not_supported() {
    // PR-YR12 migration: a sphere face on a *triangle* (no meridian seam Circle)
    // is no longer CurvedSurfaceNotYetSupported — the sphere Stage-1 path is now
    // implemented, but this fixture lacks the sphere's required seam Circle edge,
    // so it is rejected as MalformedTopology (loud, never silent). Only the error
    // kind changed; the cone arm below is unchanged.
    let (v, e, f) = one_triangle(Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    });
    assert!(
        matches!(BRep::new(v, e, f), Err(YangError::MalformedTopology(_))),
        "sphere face 0 on a triangle must be MalformedTopology (lacks its meridian \
         seam Circle edge)"
    );

    let (v, e, f) = one_triangle(Surface::Cone {
        apex: Point3::new(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.5,
    });
    assert!(
        matches!(
            BRep::new(v, e, f),
            Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })
        ),
        "cone face 0 must still be CurvedSurfaceNotYetSupported"
    );
}

/// A cylinder lateral with the WRONG number of Circle rim edges (only 1 rim)
/// must be rejected loudly — the lateral needs exactly 2 Circle rims.
#[test]
fn attack6_cylinder_lateral_wrong_rim_count_rejected() {
    let axis_point = [0.0, 0.0, 0.0];
    let axis_dir = [0.0, 0.0, 1.0];
    let radius = 1.0;
    let bottom_center = axis_point;
    let neg = scale(unit(axis_dir), -1.0);
    let v0 = circle_point(bottom_center, neg, radius, SEAM_AZIMUTH);

    // Only ONE circle rim + a seam; lateral loop references [e0(circle), e1(seam)].
    let verts = vec![BRepVertex { point: cyl_p(v0) }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: cyl_p(bottom_center),
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: cyl_p(axis_point),
            axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
            radius,
        },
        outer_loop: vec![0, 1],
        inner_loops: Vec::new(),
    }];
    let r = BRep::new(verts, edges, faces);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "cylinder lateral with 1 Circle rim (not 2) must be MalformedTopology, got {r:?}"
    );
}

// =========================================================================
// Attack 7 — signed_distance_to_surface sign sanity.
// =========================================================================

#[test]
fn attack7_signed_distance_sign_sanity() {
    let surf = Surface::Cylinder {
        axis_point: Point3::new(1.0, 2.0, -1.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    // Outside: point 5 from axis → +3.
    let outside = signed_distance_to_surface(surf, Point3::new(6.0, 2.0, 10.0)).expect("ok");
    assert!(
        outside > 0.0 && (outside - 3.0).abs() < 1e-12,
        "outside point must give +3, got {outside}"
    );
    // Inside: point 0.5 from axis → −1.5.
    let inside = signed_distance_to_surface(surf, Point3::new(1.5, 2.0, 0.0)).expect("ok");
    assert!(
        inside < 0.0 && (inside - (-1.5)).abs() < 1e-12,
        "inside point must give −1.5, got {inside}"
    );

    // Sphere now evaluable (PR-YR12): signed distance = |x − center| − radius.
    // At (2,0,0) about a unit sphere centered at the origin: 2 − 1 = +1.
    let sphere_d = signed_distance_to_surface(
        Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        },
        Point3::new(2.0, 0.0, 0.0),
    )
    .expect("Sphere must be Ok");
    assert!(
        sphere_d > 0.0 && (sphere_d - 1.0).abs() < 1e-12,
        "outside sphere point must give +1, got {sphere_d}"
    );

    // Cone still rejects.
    assert!(
        signed_distance_to_surface(
            Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: 0.3,
            },
            Point3::new(1.0, 0.0, 1.0)
        )
        .is_err(),
        "Cone must Err"
    );
}

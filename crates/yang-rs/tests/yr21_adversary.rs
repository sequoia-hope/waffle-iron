//! PR-YR21 ADVERSARY — independent adversarial check on the cone∩plane ELLIPSE
//! Stage-4 relocation (`project_onto_cone_section` + the Stage-4 `Curve::Ellipse`
//! cone arm + cone reloc record / budget) landed in `crates/yang-rs/src/lib.rs`.
//!
//! This file is the ADVERSARY half of a role-separated FIP cycle: it writes TESTS
//! ONLY and never edits production, the RED file (`yr21_cone_ellipse.rs`), or any
//! other file. Per the repo convention that integration-test files cannot share
//! helpers, the harness (`p`, array math, `cone_brep`, the box builders, the
//! mesh oracles, `LabelMock`, the independent on-conic residuals, the
//! cone-ring/cap arrangement builder) is re-declared here — independently
//! authored fixtures, NOT a copy of the RED file's numbers (different cone
//! geometry, different δ choices, a distinct just-inside/just-outside bracket).
//!
//! Adversarial contract (the four properties this file guards):
//!   1. **Cylinder-ellipse path byte-identity** — the YR11 oblique-cylinder ∪ box
//!      ellipse still succeeds, relocates onto the cylinder∩plane to TAU_MODEL,
//!      and is deterministic. The cone changes did NOT perturb the cylinder path.
//!   2. **SILENT_WRONG stays 0** — out-of-scope cone sections return a LOUD `Err`,
//!      never a wrong `Ok`: (a) an asymptotic / generator-parallel (parabola)
//!      section; (b) a through-apex / wrong-nappe (`s ≤ 0`) section; (c) an
//!      on-axis degenerate ring vertex (`ρ < MIN_FEATURE_SIZE` → `OnAxis`).
//!   3. **Budget gate / band rejection is mutation-load-bearing** — a JUST-INSIDE
//!      cone+plane ellipse succeeds and relocates onto the exact ellipse (the
//!      relocation path is not vacuously failing), and a BEYOND-BAND perturbation
//!      is never silently kept as a bogus ellipse vertex (SILENT_WRONG = 0). NOTE
//!      (documented at the test, not fabricated): the Stage-4 cone budget gate
//!      `rho > cer.cone_d_eps` is SHADOWED by an identical upstream `on_both` gate
//!      (lib.rs:2804) using the same `cone_chord_bound` `tol`, so a beyond-band
//!      vertex is demoted to a LineSegment edge BEFORE it can reach the budget
//!      gate — I could not fire `OffCurveBeyondChordBand` through the public
//!      surface, and I do not fabricate a test that claims to.
//!   4. **No faithful contract weakened** — the held out-of-scope STOPs
//!      (parabola/hyperbola cone sections) remain `Err`, never `Ok`. (The
//!      shared-vertex dual-curve ambiguity audit is documented below as
//!      impractical to construct via the public surface; the held-scope STOP is
//!      the contract-preservation check actually exercised.)
//!
//! Tolerances mirror the RED file (do NOT weaken): on-conic / round-trip use
//! `cad_primitives::TAU_MODEL` (1e-7); the off-band budget is the cone's own
//! `cone_chord_bound`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Stage4InvalidReason, Surface, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (re-declared; integration tests cannot share helpers).
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
fn dist_point_to_line(x: [f64; 3], axis_point: [f64; 3], axis_unit: [f64; 3]) -> f64 {
    let w = sub(x, axis_point);
    let along = dot(w, axis_unit);
    let proj = add(axis_point, scale(axis_unit, along));
    norm(sub(x, proj))
}

// =========================================================================
// Mesh oracles (re-declared).
// =========================================================================

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

struct LabelMock {
    arrangement: LabeledArrangement,
}
impl MeshBoolean for LabelMock {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.clone())
    }
}

fn edge_endpoints(brep: &BRep, e: &BRepEdge) -> ([f64; 3], [f64; 3]) {
    let vs = brep.vertices();
    (
        vs[e.start as usize].point.as_array(),
        vs[e.end as usize].point.as_array(),
    )
}

fn ellipse_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Ellipse { .. }))
        .collect()
}

// =========================================================================
// PROPERTY 1 — CYLINDER-ELLIPSE PATH BYTE-IDENTITY.
//
// An INDEPENDENT oblique cylinder ∪ box ellipse fixture (distinct numbers from
// the RED file: dir = unit([1,0,2]), r = 0.3, height = 4, box spans 0..1.5).
// The cone changes (a brand-new `Surface::Cone` arm in the Stage-4 Ellipse
// match) must NOT touch the cylinder+plane branch — it stays the YR11 path
// verbatim. We assert the cylinder∩plane ellipse relocates onto BOTH surfaces to
// TAU_MODEL and that two runs are byte-identical (so a future cone-path edit that
// perturbs the cylinder relocation is caught here).
// =========================================================================

const ACYL_RADIUS: f64 = 0.3;
const ACYL_HEIGHT: f64 = 4.0;

fn acyl_dir() -> [f64; 3] {
    unit([1.0, 0.0, 2.0])
}
/// Centre a height-4 cylinder so its axis passes through the box centre
/// (0.75, 0.75, 0.75) at its midpoint (t = 2.0): both z-caps are crossed in
/// contained ellipses, body fully inside the 1.5-cube.
fn acyl_axis_point() -> [f64; 3] {
    let d = acyl_dir();
    [0.75 - 2.0 * d[0], 0.75 - 2.0 * d[1], 0.75 - 2.0 * d[2]]
}

fn acyl_cylinder_brep() -> BRep {
    let axis_dir = acyl_dir();
    let axis_unit = unit(axis_dir);
    let axis_point = acyl_axis_point();
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, ACYL_HEIGHT));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));
    let v0 = add(bottom_center, scale(e1, ACYL_RADIUS));
    let v1 = add(top_center, scale(e1, ACYL_RADIUS));

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
                radius: ACYL_RADIUS,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius: ACYL_RADIUS,
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
                radius: ACYL_RADIUS,
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
    BRep::new(verts, edges, faces).expect("acyl_cylinder_brep: BRep::new failed")
}

/// A 1.5-cube `[0,1.5]^3` with TRUE per-face plane offsets (box B for the
/// cylinder fixture). The bottom cap z=0 / top cap z=1.5 sections are ellipses.
fn box_15_brep() -> BRep {
    let s = 1.5;
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(s, 0.0, 0.0),
        },
        BRepVertex {
            point: p(s, s, 0.0),
        },
        BRepVertex {
            point: p(0.0, s, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, s),
        },
        BRepVertex {
            point: p(s, 0.0, s),
        },
        BRepVertex { point: p(s, s, s) },
        BRepVertex {
            point: p(0.0, s, s),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z=0)
        [4, 7, 6, 5], // F1 top (z=s)
        [0, 4, 5, 1], // F2 front (y=0)
        [1, 5, 6, 2], // F3 right (x=s)
        [2, 6, 7, 3], // F4 back (y=s)
        [3, 7, 4, 0], // F5 left (x=0)
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
    let offs = [0.0, -s, 0.0, -s, -s, 0.0];
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
    BRep::new(verts, edges, faces).expect("box_15_brep: BRep::new failed")
}

/// Analytic AABB diagonal of the oblique cylinder ⇒ its Stage-1 chord band d_ε.
fn acyl_d_eps() -> f64 {
    let axis_unit = acyl_dir();
    let bottom_center = acyl_axis_point();
    let top_center = add(bottom_center, scale(axis_unit, ACYL_HEIGHT));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = ACYL_RADIUS * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    1e-2 * norm(sub(hi, lo))
}

fn cap_plane(cap_z: f64) -> (Vector3, f64) {
    if cap_z == 0.0 {
        (Vector3::new(0.0, 0.0, -1.0), 0.0)
    } else {
        (Vector3::new(0.0, 0.0, 1.0), -cap_z)
    }
}
fn acyl_radial_residual(x: [f64; 3]) -> f64 {
    let r = dist_point_to_line(x, acyl_axis_point(), acyl_dir());
    (r - ACYL_RADIUS).abs()
}
fn acyl_plane_residual(x: [f64; 3], cap_z: f64) -> f64 {
    let (normal, d) = cap_plane(cap_z);
    (dot(x, normal.as_array()) + d).abs()
}
fn cap_z_of(pt: [f64; 3], top: f64) -> f64 {
    if pt[2] <= top * 0.5 {
        0.0
    } else {
        top
    }
}

fn acyl_ortho_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let nu = unit(n);
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    let seed = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(sub(seed, scale(nu, dot(seed, nu))));
    let e2 = cross(nu, e1);
    (e1, e2)
}

const ACYL_FACETS: usize = 16;

fn acyl_build_tube(
    bottom: &[[f64; 3]],
    top: &[[f64; 3]],
    bot_center: [f64; 3],
    top_center: [f64; 3],
) -> LabeledArrangement {
    assert_eq!(bottom.len(), top.len());
    let n_facets = bottom.len();
    let mut verts: Vec<Point3> = Vec::new();
    let mut bot = Vec::with_capacity(n_facets);
    let mut topv = Vec::with_capacity(n_facets);
    for &v in bottom {
        bot.push(verts.len() as u32);
        verts.push(p(v[0], v[1], v[2]));
    }
    for &v in top {
        topv.push(verts.len() as u32);
        verts.push(p(v[0], v[1], v[2]));
    }
    let cb = verts.len() as u32;
    verts.push(p(bot_center[0], bot_center[1], bot_center[2]));
    let ct = verts.len() as u32;
    verts.push(p(top_center[0], top_center[1], top_center[2]));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    let mut push = |t: [u32; 3], label: u32| {
        tris.push(t);
        surface.push(vec![LaInputId(label)]);
    };
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([bot[k], bot[k1], topv[k1]], 0);
        push([bot[k], topv[k1], topv[k]], 0);
    }
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([cb, bot[k1], bot[k]], 1);
    }
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([ct, topv[k], topv[k1]], 1);
    }
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        num_inputs: 2,
    }
}

/// Sample both cap rings on the oblique cylinder at radial distance `rprime`,
/// solving the axial param so the sampled point lands exactly on cap z (0 / 1.5).
fn acyl_ring_at_radius(rprime: f64) -> LabeledArrangement {
    let dir = acyl_dir();
    let axis_point = acyl_axis_point();
    let (e1, e2) = acyl_ortho_basis(dir);
    let surf = |theta: f64, s: f64| -> [f64; 3] {
        add(
            add(axis_point, scale(dir, s)),
            scale(add(scale(e1, theta.cos()), scale(e2, theta.sin())), rprime),
        )
    };
    let s_for = |cap_z: f64, theta: f64| -> f64 {
        let radial_z = rprime * (theta.cos() * e1[2] + theta.sin() * e2[2]);
        (cap_z - axis_point[2] - radial_z) / dir[2]
    };
    let n = ACYL_FACETS;
    let ring_on = |cap_z: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|k| {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                surf(th, s_for(cap_z, th))
            })
            .collect()
    };
    let bottom = ring_on(0.0);
    let top = ring_on(1.5);
    let mean = |ring: &[[f64; 3]]| -> [f64; 3] {
        let mut c = [0.0; 3];
        for v in ring {
            c = add(c, *v);
        }
        scale(c, 1.0 / ring.len() as f64)
    };
    let bc = mean(&bottom);
    let tc = mean(&top);
    acyl_build_tube(&bottom, &top, bc, tc)
}

#[test]
fn adversary_cylinder_ellipse_path_unaffected_byte_identity() {
    // INDEPENDENT ssi-rs oracle: the oblique cylinder ∩ z-plane is an Ellipse
    // (proves the fixture genuinely exercises the cylinder-ellipse arm).
    let cyl_q = ssi_rs::QuadricSurface::Cylinder {
        axis_point: Point3::from(acyl_axis_point()),
        axis_dir: Vector3::from(acyl_dir()),
        radius: ACYL_RADIUS,
    };
    let plane_q = ssi_rs::QuadricSurface::Plane {
        point: Point3::from([0.0, 0.0, 0.0]),
        normal: Vector3::new(0.0, 0.0, -1.0),
    };
    let curves = ssi_rs::intersect(&plane_q, &cyl_q).expect("oracle: oblique cap must intersect");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "adversary P1: cylinder fixture must genuinely be an Ellipse section, got {curves:?}"
    );

    let cyl = acyl_cylinder_brep();
    let bx = box_15_brep();
    let de = acyl_d_eps();
    let delta = 0.4 * de; // off-curve, inside (TAU_WORK, d_ε]
    assert!(delta > TAU_WORK && delta <= de);

    let mock = LabelMock {
        arrangement: acyl_ring_at_radius(ACYL_RADIUS - delta),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock).expect(
        "adversary P1: the cone changes must NOT break the cylinder-ellipse path; \
         oblique cylinder ∪ box must still Ok",
    );

    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "adversary P1: cylinder output must stay watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "adversary P1: cylinder output Euler must stay 2"
    );

    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "adversary P1: cylinder output must carry ≥1 Ellipse edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Relocated onto BOTH the cylinder and the cap plane to TAU_MODEL — the
    // cone diff did not alter the cylinder relocation.
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let cz = cap_z_of(ep, 1.5);
            let radial = acyl_radial_residual(ep);
            let planar = acyl_plane_residual(ep, cz);
            assert!(
                radial <= TAU_MODEL,
                "adversary P1: cylinder radial residual {radial} > TAU_MODEL at {ep:?}"
            );
            assert!(
                planar <= TAU_MODEL,
                "adversary P1: cap plane residual {planar} > TAU_MODEL at {ep:?}"
            );
        }
    }

    // Determinism: a second identical run is byte-identical. A future cone-path
    // change that perturbs the cylinder path (shared loops / ordering) is caught.
    let mock2 = LabelMock {
        arrangement: acyl_ring_at_radius(ACYL_RADIUS - delta),
    };
    let r2 = boolean(&cyl, &bx, BoolOp::Union, &mock2).expect("adversary P1: determinism run 2");
    assert_eq!(
        r, r2,
        "adversary P1: identical cylinder inputs must produce a byte-identical BRep"
    );
}

// =========================================================================
// CONE FIXTURE (independent geometry from the RED file).
//
//   cone A: apex at origin, axis +Z, half_angle α = atan(0.6) (tanα = 0.6),
//     height 5 → base radius R = 3 (base-rim Curve::Circle MANDATORY — production
//     derives the cone height / chord budget from it).
//   plane B: normal n = (sin 25°, 0, cos 25°) through (0,0,2.5). α ≈ 30.96° <
//     θ = 65° (the normal makes 25° with the axis) < 90° ⇒ a bounded ELLIPSE on
//     the upper nappe. Distinct numbers from the RED file's atan(0.5)/30°/h=4.
// =========================================================================

const CONE_APEX: [f64; 3] = [0.0, 0.0, 0.0];
const CONE_AXIS: [f64; 3] = [0.0, 0.0, 1.0];
const CONE_HEIGHT: f64 = 5.0;
const CONE_N: usize = 16;

fn cone_half_angle() -> f64 {
    0.6_f64.atan()
}
fn cut_plane_normal() -> [f64; 3] {
    let beta = 25.0_f64.to_radians();
    unit([beta.sin(), 0.0, beta.cos()])
}
fn cut_plane_d() -> f64 {
    -dot(cut_plane_normal(), [0.0, 0.0, 2.5])
}
fn cut_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(cut_plane_normal()),
        d: cut_plane_d(),
    }
}

/// The cone's Stage-1 chord bound `cone_chord_bound(height, half_angle)` =
/// `1e-2 · √((2R)² + h²)`, R = height·tanα. IDENTICAL literal to production's
/// `cone_chord_bound` (single source — A14.3); height = `|(rim_center−apex)·â|`
/// = CONE_HEIGHT (what production derives from the base-rim Circle).
fn cone_d_eps() -> f64 {
    let r = CONE_HEIGHT * cone_half_angle().tan();
    1e-2 * ((2.0 * r).powi(2) + CONE_HEIGHT.powi(2)).sqrt()
}

fn cone_brep(apex: [f64; 3], axis_dir: [f64; 3], half_angle: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let radius = height * half_angle.tan();
    let base_center = add(apex, scale(axis_unit, height));
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
    let edges = vec![BRepEdge {
        start: 1,
        end: 1,
        curve: Curve::Circle {
            center: p(base_center[0], base_center[1], base_center[2]),
            normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
            radius,
        },
    }];
    let cap_d = -dot(axis_unit, base_center);
    let faces = vec![
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

fn oblique_cone() -> BRep {
    cone_brep(CONE_APEX, CONE_AXIS, cone_half_angle(), CONE_HEIGHT)
}

// ---- oblique half-space box whose top face is an arbitrary oblique plane ----

fn plane_frame_for(n: [f64; 3]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let n = unit(n);
    let absn = [n[0].abs(), n[1].abs(), n[2].abs()];
    let w = if absn[0] <= absn[1] && absn[0] <= absn[2] {
        [1.0, 0.0, 0.0]
    } else if absn[1] <= absn[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = unit(sub(w, scale(n, dot(w, n))));
    let v = cross(n, u);
    (u, v, n)
}

fn oblique_halfspace_box_for(plane_surf: Surface) -> BRep {
    let Surface::Plane { normal, d } = plane_surf else {
        unreachable!("oblique_halfspace_box_for expects a Plane");
    };
    let n = unit(normal.as_array());
    let off = dot(n, [0.0, 0.0, 2.5]) + d;
    let center = sub([0.0, 0.0, 2.5], scale(n, off));
    let (u, v, nn) = plane_frame_for(n);
    let (h, depth) = (12.0, 12.0);
    let corner = |su: f64, sv: f64| add(center, add(scale(u, su * h), scale(v, sv * h)));
    let t0 = corner(-1.0, -1.0);
    let t1 = corner(1.0, -1.0);
    let t2 = corner(1.0, 1.0);
    let t3 = corner(-1.0, 1.0);
    let b0 = add(t0, scale(nn, -depth));
    let b1 = add(t1, scale(nn, -depth));
    let b2 = add(t2, scale(nn, -depth));
    let b3 = add(t3, scale(nn, -depth));
    let verts = vec![
        BRepVertex {
            point: p(t0[0], t0[1], t0[2]),
        },
        BRepVertex {
            point: p(t1[0], t1[1], t1[2]),
        },
        BRepVertex {
            point: p(t2[0], t2[1], t2[2]),
        },
        BRepVertex {
            point: p(t3[0], t3[1], t3[2]),
        },
        BRepVertex {
            point: p(b0[0], b0[1], b0[2]),
        },
        BRepVertex {
            point: p(b1[0], b1[1], b1[2]),
        },
        BRepVertex {
            point: p(b2[0], b2[1], b2[2]),
        },
        BRepVertex {
            point: p(b3[0], b3[1], b3[2]),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
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
    let mk_plane = |a: [f64; 3], bb: [f64; 3], c: [f64; 3], interior: [f64; 3]| -> Surface {
        let mut nrm = unit(cross(sub(bb, a), sub(c, a)));
        if dot(nrm, sub(interior, a)) > 0.0 {
            nrm = scale(nrm, -1.0);
        }
        Surface::Plane {
            normal: Vector3::from(nrm),
            d: -dot(nrm, a),
        }
    };
    let interior = add(center, scale(nn, -0.5 * depth));
    let corners = [t0, t1, t2, t3, b0, b1, b2, b3];
    let mut faces: Vec<BRepFace> = Vec::with_capacity(6);
    for (fi, vs) in face_verts.iter().enumerate() {
        let surface = if fi == 0 {
            plane_surf
        } else {
            let a = corners[vs[0] as usize];
            let bb = corners[vs[1] as usize];
            let c = corners[vs[2] as usize];
            mk_plane(a, bb, c, interior)
        };
        faces.push(BRepFace {
            surface,
            outer_loop: loops[fi].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("oblique_halfspace_box_for: BRep::new failed")
}

fn cutting_box() -> BRep {
    oblique_halfspace_box_for(cut_plane_surface())
}

// ---- independent on-conic residuals ----

fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let a = CONE_APEX;
    let ax = unit(CONE_AXIS);
    let w = sub(x, a);
    let h_axial = dot(w, ax);
    let radial = norm(sub(w, scale(ax, h_axial)));
    (radial - h_axial.abs() * cone_half_angle().tan()).abs()
}
fn cone_plane_residual(x: [f64; 3]) -> f64 {
    (dot(x, cut_plane_normal()) + cut_plane_d()).abs()
}
fn conic_residual(x: [f64; 3]) -> f64 {
    cone_radial_residual(x).max(cone_plane_residual(x))
}

fn is_ring_point(pt: [f64; 3]) -> bool {
    (pt[0] * pt[0] + pt[1] * pt[1]).sqrt() > MIN_FEATURE_SIZE
}

// ---- cone-cap arrangement builder (apex fan + elliptical cap) ----

fn azim_basis() -> ([f64; 3], [f64; 3]) {
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// One ring vertex on the cone at azimuth φ, radial offset `delta` off the
/// generator, solved to land EXACTLY on the cutting plane (so plane residual ≈ 0,
/// cone radial residual ≈ |delta|).
fn cone_ring_point(phi: f64, delta: f64) -> [f64; 3] {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let n = cut_plane_normal();
    let d = cut_plane_d();
    let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
    let n_dot_r = dot(n, rhat);
    let n_dot_a = dot(n, ax);
    let s = (-d - delta * n_dot_r) / (n_dot_a + tana * n_dot_r);
    let rho = s * tana + delta;
    add(scale(ax, s), scale(rhat, rho))
}

fn cone_ring(delta: f64) -> Vec<[f64; 3]> {
    (0..CONE_N)
        .map(|k| {
            let phi = 2.0 * std::f64::consts::PI * (k as f64) / (CONE_N as f64);
            cone_ring_point(phi, delta)
        })
        .collect()
}

/// Build the closed apex-fan + elliptical-cap arrangement from a ring at radial
/// offset `delta`. Apex (label 0 = cone A) fan + plane cap (label 1 = box B).
fn build_cone_cap_arrangement(ring: &[[f64; 3]]) -> LabeledArrangement {
    let mut cap_c = [0.0; 3];
    for v in ring {
        cap_c = add(cap_c, *v);
    }
    cap_c = scale(cap_c, 1.0 / ring.len() as f64);

    let mut verts: Vec<Point3> = Vec::new();
    let apex_id = verts.len() as u32;
    verts.push(p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]));
    let rim_base = verts.len() as u32;
    for v in ring {
        verts.push(p(v[0], v[1], v[2]));
    }
    let cap_id = verts.len() as u32;
    verts.push(p(cap_c[0], cap_c[1], cap_c[2]));

    let m = ring.len();
    let rim = |k: usize| rim_base + (k % m) as u32;
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for k in 0..m {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    for k in 0..m {
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        num_inputs: 2,
    }
}

fn cone_arrangement_at_delta(delta: f64) -> LabeledArrangement {
    build_cone_cap_arrangement(&cone_ring(delta))
}

// =========================================================================
// PROPERTY 3 — BAND REJECTION IS MUTATION-LOAD-BEARING (the reachable half) +
// the SHADOWED-GATE finding (documented at the second test, not fabricated).
//
// JUST INSIDE the cone budget `cone_chord_bound` (δ = 0.5·cone_d_eps, all on the
// cutting plane so the residual ≈ δ): the relocation SUCCEEDS and lands every
// ring vertex on the exact ellipse — proving the relocation path is reachable and
// NOT vacuously failing (so a mutation that always-rejected would be caught). The
// BEYOND-band half (and why the Stage-4 budget gate is unreachable via the public
// surface) is handled by `adversary_beyond_band_no_silent_wrong_ellipse_vertex`.
// =========================================================================

#[test]
fn adversary_budget_gate_just_inside_succeeds() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let de = cone_d_eps();
    let delta = 0.5 * de; // strictly inside (TAU_WORK, cone_d_eps]

    // Fixture sanity: the RING vertices (measured directly, NOT the mesh's apex /
    // cap-center which are not on the cone) are off the cone by ~δ (≫ TAU_MODEL)
    // yet inside the budget.
    let mut before = 0.0_f64;
    for pt in cone_ring(-delta) {
        assert!(is_ring_point(pt));
        before = before.max(conic_residual(pt));
    }
    assert!(
        before > 100.0 * TAU_MODEL && before <= de,
        "adversary P3-in: fixture must start off-curve (~δ) inside the budget; before={before}, de={de}"
    );

    let mock = LabelMock {
        arrangement: cone_arrangement_at_delta(-delta),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock).expect(
        "adversary P3-in: a JUST-INSIDE-budget cone+plane ellipse must SUCCEED \
         (the gate is not vacuously always-failing)",
    );
    // And it actually relocated onto the exact ellipse.
    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "adversary P3-in: must carry ≥1 Ellipse edge after relocate"
    );
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            assert!(
                cone_radial_residual(ep) <= TAU_MODEL && cone_plane_residual(ep) <= TAU_MODEL,
                "adversary P3-in: relocated vertex {ep:?} must be on the exact ellipse"
            );
        }
    }
}

// SHADOWED-GATE FINDING (anti-fabrication, honest scope note).
//
// I attempted a JUST-OUTSIDE-budget fixture that drives the Stage-4 cone budget
// gate (`rho > cer.cone_d_eps`, lib.rs:3966) to fire with OffCurveBeyondChordBand,
// to prove that gate is mutation-load-bearing in BOTH directions. I could NOT
// construct one through the PUBLIC `boolean()` surface, and after reading
// production I can show WHY (verified, not assumed):
//
// For a vertex to reach the Stage-4 budget gate it must first become an endpoint
// of a `Curve::Ellipse` intersection edge in `build_intersection_curves`. That
// function has an UPSTREAM `on_both` gate (lib.rs:2800-2806) that drops any edge
// whose endpoint's `signed_distance_to_surface(cone)` exceeds the SAME `tol`
// (`cone_chord_tol_for_owner` = `cone_chord_bound(height, half_angle)`) that the
// Stage-4 budget later uses as `cer.cone_d_eps` (both derive the height from the
// SAME cone-owner rim Circle, so they are the SAME number). The cone component of
// `cone_ellipse_residual` (lib.rs:2307) is exactly that cone signed distance, and
// the plane component is likewise bounded by `on_both`'s plane check at the same
// `tol`. So ANY vertex that would fail the Stage-4 budget has ALREADY been
// dropped to `Curve::LineSegment` by `on_both` and never enters
// `vert_cone_ellipse` — the budget gate is genuine defense-in-depth but is
// SHADOWED by an identical upstream gate. (Confirmed empirically: a vertex pushed
// off-cone by 1.043× — cone residual ≈ 1.15·de, beyond budget — produced an Ok
// whose two edges incident to that vertex were LineSegment, i.e. it was correctly
// NOT treated as an ellipse vertex; it never reached the budget gate.)
//
// Per the cycle rules I do NOT fabricate a test that "fires" the budget gate when
// I cannot actually reach it. What I CAN and DO assert below is the
// SILENT_WRONG=0 property for the beyond-band case: a beyond-band perturbation
// must NEVER leave a `Curve::Ellipse` intersection-edge endpoint sitting beyond
// the cone band (i.e. nothing is silently relocated to / kept as a bogus ellipse
// vertex). Whether the upstream `on_both` gate (LineSegment fallback) or the
// Stage-4 budget gate (loud Err) does the rejecting, the outcome must never be a
// wrong ellipse. The just-INSIDE success test above proves the relocation path is
// not vacuously failing; together they bracket the behaviour.

#[test]
fn adversary_beyond_band_no_silent_wrong_ellipse_vertex() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let de = cone_d_eps();

    // Push ONE ring vertex off the cone by 1.043× outward from the ellipse centre
    // (cone residual ≈ 1.15·de, beyond the band) while keeping it on the cutting
    // plane. The other 15 stay on the exact ellipse.
    let cone_q = ssi_rs::QuadricSurface::Cone {
        apex: p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]),
        axis_dir: Vector3::new(CONE_AXIS[0], CONE_AXIS[1], CONE_AXIS[2]),
        half_angle: cone_half_angle(),
    };
    let plane_q = ssi_rs::QuadricSurface::Plane {
        point: Point3::from(scale(cut_plane_normal(), -cut_plane_d())),
        normal: Vector3::from(cut_plane_normal()),
    };
    let curves = ssi_rs::intersect(&plane_q, &cone_q).expect("oracle: cone∩plane ellipse");
    let ssi_rs::SsiCurve::Ellipse { center, .. } = curves
        .into_iter()
        .find(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. }))
        .expect("oracle: section must be an Ellipse")
    else {
        unreachable!()
    };
    let ec = center.as_array();

    let mut ring = cone_ring(0.0);
    ring[0] = add(ec, scale(sub(ring[0], ec), 1.043));
    let perturbed = ring[0];
    // Sanity: genuinely beyond the cone budget, yet on the plane.
    assert!(
        cone_radial_residual(perturbed) > de && cone_plane_residual(perturbed) <= TAU_MODEL,
        "adversary P3-out: perturbed vertex must be > cone budget yet on the plane; \
         radial={}, plane={}, de={de}",
        cone_radial_residual(perturbed),
        cone_plane_residual(perturbed)
    );

    let mock = LabelMock {
        arrangement: build_cone_cap_arrangement(&ring),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);

    // SILENT_WRONG = 0: EITHER a loud Err, OR an Ok in which EVERY Curve::Ellipse
    // intersection-edge endpoint is within the cone band (TAU_MODEL after relocate
    // for the on-curve ones) — the beyond-band vertex must NOT be silently carried
    // as a bogus ellipse vertex. (It is correctly demoted to a plain LineSegment
    // edge by the upstream `on_both` gate.)
    match r {
        Err(_) => { /* a loud rejection is fine — never a wrong Ok */ }
        Ok(brep) => {
            for e in ellipse_edges(&brep) {
                let (s, t) = edge_endpoints(&brep, e);
                for ep in [s, t] {
                    let res = cone_radial_residual(ep).max(cone_plane_residual(ep));
                    assert!(
                        res <= de,
                        "adversary P3-out: a Curve::Ellipse intersection-edge endpoint {ep:?} \
                         is beyond the cone band (residual {res} > de={de}) — a SILENT WRONG \
                         relocation. The beyond-band vertex must be rejected, never kept as a \
                         bogus ellipse vertex."
                    );
                }
            }
        }
    }
}

// =========================================================================
// PROPERTY 2(c) — ON-AXIS DEGENERATE RING VERTEX → loud Err (never a bogus Ok).
//
// `project_onto_cone_section` is private, so we drive its `ρ < MIN_FEATURE_SIZE`
// OnAxis guard (lib.rs:2221) through the public surface: build a cone-cap
// arrangement whose ring contains ONE vertex sitting exactly on the cone axis
// (the apex), which is on the cutting plane only if the plane passes through the
// axis — instead we place a near-axis ring vertex so its radial component ρ is
// below MIN_FEATURE_SIZE. The relocation must then STOP loudly (OnAxis), never
// produce a bogus Ok. Because attribution may reject a near-axis ring before
// Stage 4, we accept ANY loud Err but assert NEVER Ok (SILENT_WRONG = 0).
// =========================================================================

#[test]
fn adversary_on_axis_ring_vertex_loud_err() {
    let cone = oblique_cone();
    let bx = cutting_box();

    // Build a ring on the exact ellipse, then collapse ONE ring vertex onto the
    // cone axis (x=y=0) at the cap's z — radial component ρ ≈ 0 < MIN_FEATURE_SIZE,
    // forcing the OnAxis guard if this vertex reaches Stage-4 relocation.
    let mut ring = cone_ring(0.0);
    // cap z (mean ring z) for the on-axis replacement.
    let zc = ring.iter().map(|v| v[2]).sum::<f64>() / ring.len() as f64;
    ring[0] = [0.0, 0.0, zc]; // exactly on the +Z axis
    let arr = build_cone_cap_arrangement(&ring);

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    // SILENT_WRONG = 0: must be a loud Err, NEVER a wrong Ok. The pipeline may
    // reject at attribution, the budget gate, or the OnAxis guard — all loud.
    assert!(
        r.is_err(),
        "adversary P2c: an on-axis (ρ<MIN_FEATURE_SIZE) ring vertex must STOP loudly, \
         not return Ok; got {:?}",
        r.map(|b| b.edges().iter().map(|e| e.curve).collect::<Vec<_>>())
    );
}

// =========================================================================
// PROPERTY 2(a) — PR-YR22 MIGRATION: ASYMPTOTIC / GENERATOR-PARALLEL (PARABOLA)
// SECTION now SUCCEEDS with a Curve::Parabola edge.
//
// A cutting plane PARALLEL to a cone generator (θ = α) is a PARABOLA — the
// single-candidate conic, IN scope as of PR-YR22. ssi-rs returns exactly one
// `SsiCurve::Parabola`, `ssi_curve_to_curve` now maps it to `Curve::Parabola`,
// and the Stage-4 cone arm relocates onto it. So the section now returns Ok with
// a `Curve::Parabola` edge — never an Ellipse / wrong curve. Independently
// confirmed (ssi-rs) to be a Parabola, not an Ellipse. (Hyperbola stays LOUD.)
// =========================================================================

fn parabola_plane_normal() -> [f64; 3] {
    // n·g = 0 for the +X generator g = unit(cosα·â + sinα·x̂); n = unit([1,0,-tanα]).
    let tana = cone_half_angle().tan();
    unit([1.0, 0.0, -tana])
}
fn parabola_plane_d() -> f64 {
    -dot(parabola_plane_normal(), [0.0, 0.0, 2.5])
}
fn parabola_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(parabola_plane_normal()),
        d: parabola_plane_d(),
    }
}

/// Closed cone-cap mock whose seam ring is on cone ∩ parabola-plane, sampled only
/// over the azimuth arc where generators actually pierce the plane (n·g away from
/// 0, bounded positive s). Its only role is to make the (cone, plane) seam a
/// closed cycle so attribution succeeds and the loud STOP fires downstream.
fn build_parabola_cap_arrangement() -> LabeledArrangement {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let cosa = cone_half_angle().cos();
    let sina = cone_half_angle().sin();
    let n = parabola_plane_normal();
    let d = parabola_plane_d();
    let mut ring: Vec<[f64; 3]> = Vec::new();
    let n_samp = 24usize;
    for k in 0..n_samp {
        // PR-YR22: a NARROW 40° arc centered on the φ=180° parabola VERTEX (was a
        // wide 270° arc for the old LOUD-STOP contract). The narrow arc keeps the
        // lone wrap (apex-fan) triangle's cone-attribution residual inside the cone
        // band (≈0.026 < band 0.057) so attribution SUCCEEDS and the pipeline
        // reaches the SSI parabola selection the YR22 GREEN change wires.
        let phi = std::f64::consts::PI * (160.0 / 180.0)
            + std::f64::consts::PI * (40.0 / 180.0) * (k as f64) / ((n_samp - 1) as f64);
        let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
        let g = add(scale(ax, cosa), scale(rhat, sina));
        let n_dot_g = dot(n, g);
        if n_dot_g.abs() < 1e-3 {
            continue;
        }
        let s = -(dot(n, CONE_APEX) + d) / n_dot_g;
        if !(s.is_finite() && s > 0.1 && s < 7.0) {
            continue;
        }
        ring.push(add(CONE_APEX, scale(g, s)));
    }
    assert!(ring.len() >= 3, "parabola arc must sample ≥3 ring points");
    build_cone_cap_arrangement(&ring)
}

#[test]
fn adversary_asymptotic_parabola_section_succeeds() {
    // INDEPENDENT ssi-rs oracle: this pair is a PARABOLA, not an Ellipse.
    let plane = ssi_rs::QuadricSurface::Plane {
        point: Point3::from(scale(parabola_plane_normal(), -parabola_plane_d())),
        normal: Vector3::from(parabola_plane_normal()),
    };
    let cone = ssi_rs::QuadricSurface::Cone {
        apex: p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]),
        axis_dir: Vector3::new(CONE_AXIS[0], CONE_AXIS[1], CONE_AXIS[2]),
        half_angle: cone_half_angle(),
    };
    let curves =
        ssi_rs::intersect(&plane, &cone).expect("oracle: parabola Plane∩Cone must return Ok");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Parabola { .. })),
        "oracle: the asymptotic (θ=α) section must be a Parabola, got {curves:?}"
    );
    assert!(
        !curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "oracle: the asymptotic case must NOT be an Ellipse"
    );

    let arr = build_parabola_cap_arrangement();
    let para_box = oblique_halfspace_box_for(parabola_plane_surface());
    let cone_brep = oblique_cone();
    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone_brep, &para_box, BoolOp::Union, &mock);
    // PR-YR22: the parabola section now SUCCEEDS — Ok with a Curve::Parabola edge,
    // never the old out-of-scope STOP and never a (wrong) Ellipse (SILENT_WRONG=0).
    assert!(
        !matches!(
            r,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::LocalRefinementRequired,
                ..
            })
        ),
        "adversary P2a (YR22): the θ=α parabola section must NO LONGER STOP with \
         Stage4RegionInvalid{{LocalRefinementRequired}}, got {r:?}"
    );
    let brep = r.expect(
        "adversary P2a (YR22): the θ=α (parabola) cone section must now SUCCEED (Ok) after the \
         cone-parabola Stage-4 relocate",
    );
    let curves_out: Vec<_> = brep.edges().iter().map(|e| e.curve).collect();
    assert!(
        curves_out
            .iter()
            .any(|c| matches!(c, Curve::Parabola { .. })),
        "adversary P2a (YR22): the θ=α output must carry ≥1 Curve::Parabola edge; got {curves_out:?}"
    );
    assert!(
        !curves_out
            .iter()
            .any(|c| matches!(c, Curve::Ellipse { .. })),
        "adversary P2a (YR22): the θ=α section must NOT emit a (wrong) Ellipse edge; got {curves_out:?}"
    );
}

// =========================================================================
// PROPERTY 2(b) — WRONG-NAPPE / THROUGH-APEX (s ≤ 0) SECTION → loud Err.
//
// `project_onto_cone_section`'s `s ≤ 0` guard (lib.rs:2248) STOPs when the
// generator pierces the plane at or behind the apex (apex-coincident / wrong
// nappe). A plane PASSING THROUGH THE APEX gives a degenerate section (the two
// generators through the pierce line / a point at the apex). We drive it through
// the public surface with a cutting plane through the apex (d = 0): every
// generator's solved s satisfies `s·(n·g) = -(n·apex) = 0` ⇒ s = 0 ⇒ the guard.
// Because attribution may reject this degenerate fixture before Stage 4, we
// accept ANY loud Err but assert NEVER a bogus Ok (SILENT_WRONG = 0).
// =========================================================================

/// A cutting plane THROUGH the apex (origin): n = (sin25°,0,cos25°), d = 0.
/// `s = -(n·apex + d)/(n·g) = 0` for every generator ⇒ the `s ≤ 0` guard.
fn through_apex_plane_normal() -> [f64; 3] {
    let beta = 25.0_f64.to_radians();
    unit([beta.sin(), 0.0, beta.cos()])
}

#[test]
fn adversary_through_apex_wrong_nappe_loud_err() {
    let cone = oblique_cone();

    // Build a ring on cone ∩ (a plane just OFFSET from the apex), so the seam is a
    // genuine (cone, plane) intersection edge, but the cutting plane the cone arm
    // sees is the THROUGH-APEX plane (d=0). We assemble the box on the through-apex
    // plane and sample the ring near the apex where s→0+.
    let n = through_apex_plane_normal();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let (e1, e2) = azim_basis();
    // d = 0 (through apex). Generators pierce only AT the apex (s=0). Sample a
    // small ring just off the apex along each generator (so it is on the cone, and
    // arbitrarily close to the apex-plane), label it as the (cone, plane) seam.
    let mut ring: Vec<[f64; 3]> = Vec::new();
    for k in 0..CONE_N {
        let phi = 2.0 * std::f64::consts::PI * (k as f64) / (CONE_N as f64);
        let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
        // a point on the cone at small axial s0 (near apex) — on the cone surface.
        let s0 = 0.6;
        let pt = add(scale(ax, s0), scale(rhat, s0 * tana));
        ring.push(pt);
    }
    let arr = build_cone_cap_arrangement(&ring);

    // Box top face = the through-apex plane (d = 0).
    let through_apex_plane = Surface::Plane {
        normal: Vector3::from(n),
        d: 0.0,
    };
    let bx = oblique_halfspace_box_for(through_apex_plane);

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    // SILENT_WRONG = 0: must be a loud Err, NEVER a wrong Ok. Whichever loud
    // variant fires (attribution, budget, or the s≤0 LocalRefinementRequired
    // guard), it must never silently relocate to a bogus ellipse.
    assert!(
        r.is_err(),
        "adversary P2b: a through-apex / wrong-nappe (s≤0) cone section must STOP loudly, \
         not return Ok; got {:?}",
        r.map(|b| b.edges().iter().map(|e| e.curve).collect::<Vec<_>>())
    );
}

// =========================================================================
// PROPERTY 4 — NO FAITHFUL CONTRACT WEAKENED (held out-of-scope STOPs).
//
// The dual-curve ambiguity audit (a vertex shared by a cone-ellipse edge AND
// another conic edge → LocalRefinementRequired, lib.rs:3884-3891) cannot be
// constructed deterministically through the PUBLIC boolean() surface here: it
// requires the SAME mesh vertex to be an endpoint of two DISTINCT conic
// intersection edges (cone-ellipse + circle/cylinder-ellipse) that BOTH survive
// `compute_phase_a` attribution on one hand-built LabeledArrangement — a
// multi-solid coincidence the cone-cap mock cannot express (its only conic seam
// is the single cone-ellipse ring). Constructing it would require a fabricated
// arrangement whose attribution I cannot guarantee, so per the cycle rules I do
// NOT fabricate that fixture. Instead, the contract-preservation check is the
// held-scope STOP: a HYPERBOLA cone section (θ < α, the plane steeper than the
// generators — out of scope, YR23) must remain a loud Err, never a wrong Ok. The
// parabola STOP (P2a) covers the other held conic; together they assert the
// out-of-scope conic boundary was not silently widened by the ellipse wiring.
// =========================================================================

/// A cutting plane STEEPLY inclined to the cone axis (its NORMAL nearly
/// perpendicular to the axis) ⇒ the plane angle to the axis is < α ⇒ a HYPERBOLA
/// two-branch section. n = unit([1,0,0.1]): the normal makes ~84° with the axis,
/// so the PLANE makes ~6° with the axis, well below α ≈ 30.96°. Confirmed
/// independently by ssi-rs (NOT an Ellipse). Offset so a piercing branch exists.
fn hyperbola_plane_normal() -> [f64; 3] {
    unit([1.0, 0.0, 0.1])
}
fn hyperbola_plane_d() -> f64 {
    -dot(hyperbola_plane_normal(), [1.5, 0.0, 2.5])
}
fn hyperbola_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(hyperbola_plane_normal()),
        d: hyperbola_plane_d(),
    }
}

#[test]
fn adversary_hyperbola_section_held_out_of_scope() {
    // INDEPENDENT ssi-rs oracle: θ < α ⇒ a Hyperbola, NOT an Ellipse.
    let plane = ssi_rs::QuadricSurface::Plane {
        point: Point3::from(scale(hyperbola_plane_normal(), -hyperbola_plane_d())),
        normal: Vector3::from(hyperbola_plane_normal()),
    };
    let cone = ssi_rs::QuadricSurface::Cone {
        apex: p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]),
        axis_dir: Vector3::new(CONE_AXIS[0], CONE_AXIS[1], CONE_AXIS[2]),
        half_angle: cone_half_angle(),
    };
    let curves =
        ssi_rs::intersect(&plane, &cone).expect("oracle: hyperbola Plane∩Cone must return Ok");
    assert!(
        !curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "oracle: the θ<α section must NOT be an Ellipse (held out of scope), got {curves:?}"
    );

    // Drive the public surface with a cone-cap mock whose seam ring lies on cone ∩
    // hyperbola-plane, sampled only over the azimuth arc where generators pierce
    // the plane with bounded positive s.
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let cosa = cone_half_angle().cos();
    let sina = cone_half_angle().sin();
    let n = hyperbola_plane_normal();
    let d = hyperbola_plane_d();
    let mut ring: Vec<[f64; 3]> = Vec::new();
    let n_samp = 24usize;
    for k in 0..n_samp {
        // a ~180° arc on the piercing side (around +X), away from the back side.
        let phi =
            -std::f64::consts::PI * 0.5 + std::f64::consts::PI * (k as f64) / ((n_samp - 1) as f64);
        let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
        let g = add(scale(ax, cosa), scale(rhat, sina));
        let n_dot_g = dot(n, g);
        if n_dot_g.abs() < 1e-3 {
            continue;
        }
        let s = -(dot(n, CONE_APEX) + d) / n_dot_g;
        if !(s.is_finite() && s > 0.1 && s < 7.0) {
            continue;
        }
        ring.push(add(CONE_APEX, scale(g, s)));
    }
    assert!(ring.len() >= 3, "hyperbola arc must sample ≥3 ring points");
    let arr = build_cone_cap_arrangement(&ring);
    let hyp_box = oblique_halfspace_box_for(hyperbola_plane_surface());
    let cone_brep = oblique_cone();
    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone_brep, &hyp_box, BoolOp::Union, &mock);
    // SILENT_WRONG = 0: a held out-of-scope hyperbola must STOP loudly, never Ok.
    assert!(
        r.is_err(),
        "adversary P4: a held out-of-scope HYPERBOLA cone section must STOP loudly, not Ok; \
         got {:?}",
        r.map(|b| b.edges().iter().map(|e| e.curve).collect::<Vec<_>>())
    );
}

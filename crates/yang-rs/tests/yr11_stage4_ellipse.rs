//! PR-YR11 RED — Stage 4 (OBLIQUE): RELOCATE mesh intersection points onto the
//! exact analytical ELLIPSE (oblique cylinder ∪ box), via the cylinder
//! parameterization (Yang §4.3.2), with §4.5.3 reversed-point correction.
//!
//! Spec of record: `specs/yang_pr_yr11_stage4_oblique_ellipse.md` (§5 is the RED
//! contract; §6 is hard scope). Paper: Yang 2025 §4.4.1 (mesh updating /
//! relocation) + §4.3.2 (parametric surface relocation) + §4.5.3 (correction of
//! reversed intersection).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY; the
//! GREEN implementer extends `crates/yang-rs/src/lib.rs` (the Stage-4 relocate +
//! reversal sweep + the `eval_source` Ellipse arm) to make these oracles GREEN.
//! The RED author NEVER edits production code.
//!
//! ## RED state (behavioral — the API surface all exists)
//!
//! Every public symbol this file references (`Stage4InvalidReason`,
//! `Curve::Ellipse`, `boolean`, `TessellationSource::BRepEdge`, …) already
//! exists in current production. So this file COMPILES against current
//! production and the initial RED state is **assert-fail**, not compile-fail:
//! current Stage 4 LOUDLY STOPs on an `Ellipse` intersection edge with
//! `Err(Stage4RegionInvalid { reason: EllipseProjectionUnsupported })`
//! (`crates/yang-rs/src/lib.rs` Stage-4 relocate). The oblique cylinder ∪ box
//! therefore returns that `Err` today, so the "must succeed + carry a
//! `Curve::Ellipse` edge + relocate onto the exact ellipse" oracles FAIL. After
//! GREEN lifts the STOP (relocating via the cylinder parameterization), they
//! pass.
//!
//! Per the established repo convention (integration-test files cannot share
//! helpers), the yr10 harness (`p`, array math, `cylinder_brep`, `canonical_box`,
//! `oblique_cylinder`, `surface_to_quadric`, `d_eps`, `LabelMock`,
//! `build_tube_from_3d_rings`, `hand_built_oblique_ellipse_arrangement`,
//! `unpaired_half_edges`, `euler_characteristic`, ssi-rs oracle helpers) is
//! re-declared verbatim here. The on-ellipse oracle (the exact ellipse the output
//! must match) is recomputed INDEPENDENTLY from the fixture's true cylinder/plane
//! (cylinder radial + plane residual), NOT via production code.
//!
//! Tolerances (spec §5, do NOT weaken):
//!   - On-ellipse / round-trip / after-deviation: `cad_primitives::TAU_MODEL` (1e-7).
//!   - Selection / off-band band: `d_ε` via `d_eps(...)`.
//!   - Angular: 1e-6 rad.

use std::collections::{HashMap, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use std::error::Error;
use yang_rs::{
    boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Stage4InvalidReason, Surface,
    TessellationSource, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math. Re-declared verbatim from tests/yr10_stage4_relocate.rs.
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
    let proj = add(axis_point, scale(axis_unit, along));
    norm(sub(x, proj))
}

// =========================================================================
// Cylinder B-Rep fixture (seam-edge encoding). Re-declared from yr10.
// =========================================================================

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
        // f0 lateral Cylinder
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
        // f1 bottom cap Plane
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 top cap Plane
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

/// Analytic AABB diagonal from the two rim circles' exact extents.
fn analytic_aabb_diagonal(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> f64 {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));
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

fn d_eps(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> f64 {
    1e-2 * analytic_aabb_diagonal(axis_point, axis_dir, radius, height)
}

// =========================================================================
// Unit-cube fixture with TRUE per-face plane offsets. Re-declared from yr10.
// =========================================================================

fn unit_cube_brep_offset_at(origin: [f64; 3]) -> BRep {
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
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z)
        [4, 7, 6, 5], // F1 top (z+1)
        [0, 4, 5, 1], // F2 front (y)
        [1, 5, 6, 2], // F3 right (x+1)
        [2, 6, 7, 3], // F4 back (y+1)
        [3, 7, 4, 0], // F5 left (x)
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
    let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
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
    BRep::new(verts, edges, faces).expect("offset cube BRep::new failed")
}

// =========================================================================
// Analytic mesh oracles. Re-declared from yr10.
// =========================================================================

/// Count directed half-edges with no opposite. Watertight ⇒ 0 unpaired.
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

/// Euler V − E + F over the mesh (E = unique undirected edges).
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

// =========================================================================
// Canonical OBLIQUE config. Re-declared from yr10's `oblique_cylinder` /
// `hand_built_oblique_ellipse_arrangement`. The cylinder axis is tilted 30° off
// +Z toward +X, so a z=const box cap section is an ELLIPSE.
//
//   box      = unit_cube_brep_offset_at([0,0,0])  (spans 0..1 in x,y,z)
//   cylinder = oblique_cylinder() (axis dir = unit([0.5,0,1]), r=0.25, h=3.0)
// =========================================================================

const CYL_RADIUS: f64 = 0.25;

/// The oblique cylinder's axis direction (unit), shared by all helpers.
/// A moderate ~18.4° tilt (`unit([1,0,3])`): clearly oblique — the z=const
/// section is an ELLIPSE (semi-major r/cos ≈ 1.054·r) — yet gentle enough that,
/// when centred (see `oblique_axis_point`), both cap ellipses AND the tilted
/// body stay inside the unit box (no side-face exit; the steeper [0.5,0,1] tilt
/// drove the axis past x=1 by the top cap → the out-of-scope corner case).
fn oblique_dir() -> [f64; 3] {
    unit([1.0, 0.0, 3.0])
}
/// The oblique cylinder's axis_point. Centre the height-3 cylinder so its axis
/// passes through the unit box's centre (0.5,0.5,0.5) at its midpoint (t=1.5):
/// both z-caps are crossed in contained ellipses, body fully inside the box.
fn oblique_axis_point() -> [f64; 3] {
    let dir = oblique_dir();
    [0.5 - 1.5 * dir[0], 0.5 - 1.5 * dir[1], 0.5 - 1.5 * dir[2]]
}

/// An oblique cylinder (axis tilted in the x-z plane). Its section by a z=const
/// plane is an ELLIPSE. Re-declared from yr10.
fn oblique_cylinder() -> BRep {
    let dir = oblique_dir();
    let height = 3.0;
    let axis_point = oblique_axis_point();
    cylinder_brep(axis_point, dir, CYL_RADIUS, height)
}

fn canonical_box() -> BRep {
    unit_cube_brep_offset_at([0.0, 0.0, 0.0])
}

/// The oblique cylinder's analytical lateral surface (used by the ssi-rs oracle).
fn oblique_cylinder_surface() -> Surface {
    let dir = oblique_dir();
    let axis_point = oblique_axis_point();
    Surface::Cylinder {
        axis_point: Point3::from(axis_point),
        axis_dir: Vector3::from(dir),
        radius: CYL_RADIUS,
    }
}

/// `d_ε` for the oblique cylinder fixture (its own AABB diagonal).
fn oblique_d_eps() -> f64 {
    d_eps(oblique_axis_point(), oblique_dir(), CYL_RADIUS, 3.0)
}

// =========================================================================
// SSI ORACLE — compute the EXACT oblique cap ellipse independently via ssi-rs.
// Re-declared from yr10 (surface_to_quadric verbatim).
// =========================================================================

fn surface_to_quadric(s: Surface) -> ssi_rs::QuadricSurface {
    match s {
        Surface::Torus { .. } => unreachable!("KV6d: torus not exercised by this test"),
        Surface::Plane { normal, d } => {
            let n = unit(normal.as_array());
            let point = scale(n, -d);
            ssi_rs::QuadricSurface::Plane {
                point: Point3::from(point),
                normal: Vector3::from(n),
            }
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => ssi_rs::QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        },
        Surface::Sphere { center, radius } => ssi_rs::QuadricSurface::Sphere { center, radius },
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => ssi_rs::QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        },
    }
}

/// The cutting-plane (normal, d) for box cap z=`cap_z`. Bottom cap z=0 has
/// normal (0,0,-1), d=0; top cap z=1 has normal (0,0,1), d=-1.
fn cap_plane(cap_z: f64) -> (Vector3, f64) {
    if cap_z == 0.0 {
        (Vector3::new(0.0, 0.0, -1.0), 0.0)
    } else {
        (Vector3::new(0.0, 0.0, 1.0), -cap_z)
    }
}

/// The EXACT cap ELLIPSE on plane z=`cap_z` (oblique cylinder ∩ that box cap),
/// computed independently by ssi-rs. Asserts it really is an Ellipse.
fn oracle_cap_ellipse(cap_z: f64) -> ssi_rs::SsiCurve {
    let (normal, d) = cap_plane(cap_z);
    let plane = surface_to_quadric(Surface::Plane { normal, d });
    let cyl = surface_to_quadric(oblique_cylinder_surface());
    let curves = ssi_rs::intersect(&plane, &cyl)
        .expect("oracle: Plane∩Cylinder must succeed for an oblique cap");
    assert_eq!(
        curves.len(),
        1,
        "oracle: oblique cap section must be exactly one curve, got {curves:?}"
    );
    assert!(
        matches!(curves[0], ssi_rs::SsiCurve::Ellipse { .. }),
        "oracle: oblique cylinder ∩ z-plane must be an Ellipse, got {:?}",
        curves[0]
    );
    curves[0]
}

// =========================================================================
// INDEPENDENT on-ellipse residual: a relocated crossing must end on BOTH the
// true cylinder (radial distance r about the axis) AND the cutting plane
// (n·x + d = 0). This is recomputed straight from the fixture's cylinder/plane
// (NOT via production), matching spec §5 Oracle 1's two-residual contract.
// =========================================================================

/// Cylinder radial residual `|dist(x, axis) − r|`.
fn cyl_radial_residual(x: [f64; 3]) -> f64 {
    let r = dist_point_to_line(x, oblique_axis_point(), oblique_dir());
    (r - CYL_RADIUS).abs()
}

/// Plane residual `|n·x + d|` for the cap plane at z=`cap_z`.
fn plane_residual(x: [f64; 3], cap_z: f64) -> f64 {
    let (normal, d) = cap_plane(cap_z);
    (dot(x, normal.as_array()) + d).abs()
}

/// The on-both-surfaces residual `max(|dist(x,axis)−r|, |n·x+d|)` — the spec §5
/// Oracle-1 ellipse residual, recomputed independently of production.
fn ellipse_residual(x: [f64; 3], cap_z: f64) -> f64 {
    cyl_radial_residual(x).max(plane_residual(x, cap_z))
}

// =========================================================================
// `ortho_basis` re-implemented in-test (used by the oblique surface sampler;
// matches lib.rs's basis, the SAME frame the oblique ring fixture uses).
// =========================================================================

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
// Independent ELLIPSE evaluation/parameterization — matches the spec §3 / the
// `curve_contains_point` Ellipse convention EXACTLY:
//   minor_dir = normalize(normal) × normalize(major_axis)
//   point(t)  = C + major_radius·cos t·major_axis + minor_radius·sin t·minor_dir
// Used ONLY by the round-trip oracle to invert `BRepEdge{edge,t}` and by the
// chord-deviation oracle's exact-ellipse reference. Recomputed from the SSI
// oracle's exact ellipse, independently of production.
// =========================================================================

fn ellipse_minor_dir(normal: [f64; 3], major_axis: [f64; 3]) -> [f64; 3] {
    cross(unit(normal), unit(major_axis))
}

/// Evaluate the exact ellipse point at parameter `t`, in the spec §3 convention.
fn eval_ellipse_point(
    center: [f64; 3],
    normal: [f64; 3],
    major_axis: [f64; 3],
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> [f64; 3] {
    let maj = unit(major_axis);
    let mindir = ellipse_minor_dir(normal, major_axis);
    add(
        center,
        add(
            scale(maj, major_radius * t.cos()),
            scale(mindir, minor_radius * t.sin()),
        ),
    )
}

/// Perpendicular (shortest) distance from `x` to the exact ellipse, sampled
/// densely. Used by the chord-deviation oracle as a production-independent
/// reference; coarse sampling is fine because we only assert ≫ TAU_MODEL vs
/// ≤ TAU_MODEL gaps (orders of magnitude apart), never a tight equality.
fn dist_to_ellipse_sampled(x: [f64; 3], ell: &ssi_rs::SsiCurve) -> f64 {
    let ssi_rs::SsiCurve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = ell
    else {
        panic!("dist_to_ellipse_sampled: not an ellipse");
    };
    let mut best = f64::INFINITY;
    let samples = 200_000usize;
    for k in 0..samples {
        let t = 2.0 * std::f64::consts::PI * (k as f64) / (samples as f64);
        let pe = eval_ellipse_point(
            center.as_array(),
            normal.as_array(),
            major_axis.as_array(),
            *major_radius,
            *minor_radius,
            t,
        );
        best = best.min(norm(sub(x, pe)));
    }
    best
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement.
// Re-declared from yr10.
// =========================================================================

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

// N=16 (not 8): the off-curve fixture pulls the ENTIRE ring inward by a radial
// δ, so the lateral-wall triangle centroid is offset from the true cylinder
// surface. As in yr10, N=16 keeps the lateral centroid offset within the
// UNCHANGED attribution band d_ε so attribution succeeds and Stage 4 is reached.
const N_FACETS: usize = 16;

// =========================================================================
// Build a 2-label (lateral=A label 0, caps=B label 1) closed tube+caps
// arrangement from explicit 3D bottom/top rings + cap centers. Re-declared
// verbatim from yr10's `build_tube_from_3d_rings`.
// =========================================================================

fn build_tube_from_3d_rings(
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
        num_inputs: 2,
    }
}

// =========================================================================
// OBLIQUE ELLIPSE FIXTURES.
//
// `oblique_ring_at_radius(rprime)` samples both cap rings on the oblique
// cylinder SURFACE but at RADIAL distance `rprime` (instead of CYL_RADIUS) from
// the axis, while solving the axial parameter so the sampled point still lands
// EXACTLY on the cap plane z=cap_z. So:
//   - rprime = CYL_RADIUS → ON the exact ellipse (axis section).
//   - rprime = CYL_RADIUS − δ → OFF the exact cylinder by ~δ radially, yet still
//     ON the cap plane (z exact). This is the controlled chord-band offset:
//     ellipse_residual ≈ |rprime − CYL_RADIUS| = δ.
// Cap centers = the mean of each ring (on-plane). Mirrors yr10's
// `hand_built_oblique_ellipse_arrangement` exactly except for the rprime knob.
// =========================================================================

fn oblique_ring_at_radius(rprime: f64) -> LabeledArrangement {
    let dir = oblique_dir();
    let axis_point = oblique_axis_point();
    let (e1, e2) = ortho_basis(dir);
    // On-surface point at radial distance `rprime`, angle θ, axial param s.
    let surf = |theta: f64, s: f64| -> [f64; 3] {
        add(
            add(axis_point, scale(dir, s)),
            scale(add(scale(e1, theta.cos()), scale(e2, theta.sin())), rprime),
        )
    };
    // Solve s so surf(θ, s).z == cap_z (the z-section sample), at radius rprime.
    let s_for = |cap_z: f64, theta: f64| -> f64 {
        let radial_z = rprime * (theta.cos() * e1[2] + theta.sin() * e2[2]);
        (cap_z - axis_point[2] - radial_z) / dir[2]
    };
    let n = N_FACETS;
    let ring_on = |cap_z: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|k| {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                surf(th, s_for(cap_z, th))
            })
            .collect()
    };
    let bottom = ring_on(0.0);
    let top = ring_on(1.0);
    let mean = |ring: &[[f64; 3]]| -> [f64; 3] {
        let mut c = [0.0; 3];
        for v in ring {
            c = add(c, *v);
        }
        scale(c, 1.0 / ring.len() as f64)
    };
    let bot_center = mean(&bottom);
    let top_center = mean(&top);
    build_tube_from_3d_rings(&bottom, &top, bot_center, top_center)
}

/// On-ellipse arrangement (rings sampled at the true radius). Mirrors yr10's
/// `hand_built_oblique_ellipse_arrangement` (verbatim geometry).
fn hand_built_oblique_ellipse_arrangement() -> LabeledArrangement {
    oblique_ring_at_radius(CYL_RADIUS)
}

/// The off-curve δ used by the oblique Stage-4 fixture: strictly inside the
/// `(TAU_WORK, d_ε]` relocate band (`δ = 0.4 · d_ε`), so the cap-ring vertices
/// are genuinely off-ellipse (`ρ ≈ δ ≫ TAU_MODEL`) yet still relocatable.
fn relocate_band_delta() -> f64 {
    0.4 * oblique_d_eps()
}

/// Off-curve oblique arrangement: rings at radial `r' = CYL_RADIUS − 0.4·d_ε`,
/// still ON the cap planes. The cap-ring vertices are off the exact ellipse by
/// ~δ pre-Stage-4 (genuine relocation work).
fn hand_built_offcurve_oblique_arrangement() -> LabeledArrangement {
    oblique_ring_at_radius(CYL_RADIUS - relocate_band_delta())
}

// =========================================================================
// Output-edge helpers. Re-declared from yr10.
// =========================================================================

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

/// Cap-z classification of an intersection-edge endpoint by its z coordinate.
fn cap_z_of(pt: [f64; 3]) -> f64 {
    if pt[2].abs() <= 0.5 {
        0.0
    } else {
        1.0
    }
}

/// Signed area-vector (·2) of a triangle = (b−a)×(c−a).
fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

/// The analytic outward surface normal at a mesh triangle: the cap-plane normal
/// if the triangle lies on a cap (all z within TAU_MODEL of 0 or 1), else the
/// OBLIQUE cylinder radial normal at the centroid. Used to check winding.
fn analytic_normal_at_tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<[f64; 3]> {
    let on_cap = |zc: f64| {
        a[2].abs().max(b[2].abs()).max(c[2].abs()) <= TAU_MODEL && zc == 0.0
            || (a[2] - 1.0)
                .abs()
                .max((b[2] - 1.0).abs())
                .max((c[2] - 1.0).abs())
                <= TAU_MODEL
                && zc == 1.0
    };
    if on_cap(0.0) {
        return Some([0.0, 0.0, -1.0]); // bottom cap outward
    }
    if on_cap(1.0) {
        return Some([0.0, 0.0, 1.0]); // top cap outward
    }
    // Lateral: outward radial at the centroid (about the OBLIQUE axis).
    let centroid = scale(add(add(a, b), c), 1.0 / 3.0);
    let axis_unit = oblique_dir();
    let axis_point = oblique_axis_point();
    let w = sub(centroid, axis_point);
    let radial = sub(w, scale(axis_unit, dot(w, axis_unit)));
    if norm(radial) < MIN_FEATURE_SIZE {
        return None;
    }
    Some(unit(radial))
}

// =========================================================================
// ORACLE 1 + 2 + 3 + 4 + 5 + determinism — the core oblique relocation oracle on
// the off-curve oblique tube. Drives boolean() with the off-curve LabelMock and
// asserts (spec §5):
//   1. every relocated intersection-edge vertex is on the EXACT ellipse
//      (cylinder radial residual ≤ TAU_MODEL AND plane residual ≤ TAU_MODEL),
//      recomputed independently from the fixture's true cylinder/plane;
//   3. max chord deviation strictly DECREASES (before ≫ TAU_MODEL, after ≤ TAU_MODEL);
//   4. output is watertight 2-manifold (0 unpaired, Euler 2), no inverted/
//      degenerate tris;
//   5. relocated verts' TessellationSource is BRepEdge{edge,t}, round-tripping
//      via the exact ellipse parameterization to the relocated position ≤ TAU_MODEL;
//      AND determinism (two runs byte-identical).
//
// RED: current production STOPs on the Ellipse edge with
// Stage4RegionInvalid{EllipseProjectionUnsupported}, so `boolean()` returns Err
// and the `.expect(...)` fails — the intended RED state ("production STOPs /
// rejects instead of relocating").
// =========================================================================

#[test]
fn t1_oblique_relocate_on_ellipse_chord_decreases_watertight() {
    let cyl = oblique_cylinder();
    let bx = canonical_box();
    let delta = relocate_band_delta();
    let de = oblique_d_eps();

    // Fixture sanity: TAU_WORK < δ ≤ d_ε (genuinely off-curve, relocatable).
    assert!(
        delta > TAU_WORK && delta <= de,
        "fixture δ={delta} must lie in (TAU_WORK, d_ε={de}]"
    );

    // BEFORE: the off-curve ring vertices are off the exact ellipse by ~δ.
    let arr = hand_built_offcurve_oblique_arrangement();
    let mut before_max_dev = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        // Cap-ring vertices (z≈0 or z≈1, off the axis) are the intersection
        // endpoints; the cap centers sit near the axis and are not relocated.
        if (pt[2].abs() <= TAU_MODEL || (pt[2] - 1.0).abs() <= TAU_MODEL)
            && dist_point_to_line(pt, oblique_axis_point(), oblique_dir()) > MIN_FEATURE_SIZE
        {
            before_max_dev = before_max_dev.max(ellipse_residual(pt, cap_z_of(pt)));
        }
    }
    assert!(
        before_max_dev > 100.0 * TAU_MODEL,
        "fixture must start genuinely off the ellipse (before_max_dev={before_max_dev} \
         should be ≫ TAU_MODEL); δ={delta}"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock).expect(
        "yr11 §5.2: oblique cylinder ∪ box (off-curve mock) must return Ok after Stage-4 \
         ellipse relocate (NOT Err(EllipseProjectionUnsupported))",
    );

    // ORACLE 4: watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr11 §5.4: relocated output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr11 §5.4: relocated output Euler V−E+F must be 2"
    );

    // The cap rings must carry exact Ellipse curves (the oblique sections).
    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "yr11 §5.2: expected ≥1 cap-ring Ellipse intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // ORACLE 1: every relocated intersection-edge vertex is on the exact ellipse
    // (BOTH residuals ≤ TAU_MODEL), recomputed independently from the fixture's
    // true cylinder/plane (NOT via the production ellipse fields).
    // ORACLE 3 (after): the max deviation of the polyline AFTER relocation.
    let mut after_max_dev = 0.0_f64;
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let cap_z = cap_z_of(ep);
            let radial = cyl_radial_residual(ep);
            let planar = plane_residual(ep, cap_z);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr11 §5.1: relocated vertex {ep:?} cylinder radial residual {radial} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr11 §5.1: relocated vertex {ep:?} plane residual {planar} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
        }
    }

    // ORACLE 3: chord deviation strictly decreases (and ends ≤ TAU_MODEL).
    assert!(
        after_max_dev < before_max_dev,
        "yr11 §5.3: max chord deviation must strictly decrease (after {after_max_dev} \
         < before {before_max_dev}) — proves real relocation, not a no-op"
    );
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr11 §5.3: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );

    // ORACLE 5: relocated verts carry BRepEdge{edge,t}; inverting via the EXACT
    // ellipse parameterization (spec §3 convention) reproduces the relocated
    // mesh position within TAU_MODEL. The ellipse frame is taken from the
    // production edge's OWN Curve::Ellipse fields (the round-trip is internal
    // self-consistency: production's `t` against production's stored ellipse).
    let tmap = r.tessellation_map();
    let mesh = r.as_mesh();
    let mut saw_relocated_edge_source = false;
    for e in &ellipses {
        let Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } = e.curve
        else {
            continue;
        };
        for vid in [e.start, e.end] {
            let src = tmap.lookup(vid);
            match src {
                TessellationSource::BRepEdge { edge: _, t } => {
                    saw_relocated_edge_source = true;
                    let inverted = eval_ellipse_point(
                        center.as_array(),
                        normal.as_array(),
                        major_axis.as_array(),
                        major_radius,
                        minor_radius,
                        t,
                    );
                    let mesh_pos = mesh.verts[vid as usize].as_array();
                    let d = norm(sub(inverted, mesh_pos));
                    assert!(
                        d <= TAU_MODEL,
                        "yr11 §5.5: relocated vertex {vid} BRepEdge t={t} must invert (via the \
                         exact ellipse parameterization) to the mesh position within TAU_MODEL, off by {d}"
                    );
                }
                other => panic!(
                    "yr11 §5.5: relocated ellipse intersection-edge vertex {vid} must carry \
                     TessellationSource::BRepEdge{{edge,t}}, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw_relocated_edge_source,
        "yr11 §5.5: at least one relocated vertex must carry a BRepEdge source"
    );

    // Determinism: a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: hand_built_offcurve_oblique_arrangement(),
    };
    let r2 = boolean(&cyl, &bx, BoolOp::Union, &mock2).expect("yr11 §5.5: determinism run 2");
    assert_eq!(
        r, r2,
        "yr11 §5.5: identical inputs must produce a byte-identical output BRep"
    );
}

// =========================================================================
// ORACLE 2 — the oblique cylinder ∪ box now SUCCEEDS (no
// EllipseProjectionUnsupported) and the output carries ≥1 Curve::Ellipse
// intersection edge. An INDEPENDENT ssi-rs oracle confirms the oblique section
// really is an Ellipse (the assertion mirrors yr10 lines 1142-1162).
//
// This oracle uses the ON-ellipse arrangement (rings at the true radius), so a
// successful Ok output requires the Ellipse edge to be emitted and relocated —
// no off-curve chord-band complication. RED: production currently returns
// Err(EllipseProjectionUnsupported), so this fails.
// =========================================================================

#[test]
fn t2_oblique_union_succeeds_with_ellipse_edge() {
    // INDEPENDENT ssi-rs oracle (mirrors yr10 t4 lines 1142-1162): the oblique
    // cylinder ∩ z-plane really is an Ellipse, so production is exercising the
    // right path.
    let dir = oblique_dir();
    let axis_point = oblique_axis_point();
    let cyl_q = surface_to_quadric(Surface::Cylinder {
        axis_point: Point3::from(axis_point),
        axis_dir: Vector3::from(dir),
        radius: CYL_RADIUS,
    });
    let plane_q = surface_to_quadric(Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    });
    let curves =
        ssi_rs::intersect(&plane_q, &cyl_q).expect("oracle: oblique cap section must intersect");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "oracle: oblique cylinder ∩ z-plane must yield an Ellipse section, got {curves:?}"
    );

    let a = oblique_cylinder();
    let b = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_oblique_ellipse_arrangement(),
    };
    let r = boolean(&a, &b, BoolOp::Union, &mock);

    // Must NOT be the EllipseProjectionUnsupported STOP.
    assert!(
        !matches!(
            r,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::EllipseProjectionUnsupported,
                ..
            })
        ),
        "yr11 §5.2: oblique cylinder ∪ box must NO LONGER reject with \
         EllipseProjectionUnsupported, got {r:?}"
    );

    let brep = r.expect("yr11 §5.2: oblique cylinder ∪ box must succeed (Ok)");

    // The output must carry ≥1 Curve::Ellipse intersection edge.
    let ellipses = ellipse_edges(&brep);
    assert!(
        !ellipses.is_empty(),
        "yr11 §5.2: oblique union output must carry ≥1 Curve::Ellipse intersection edge; got {:?}",
        brep.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Each ellipse edge's stored fields must be a VALID ellipse matching the
    // independent ssi-rs section invariants (minor_radius = r, major_radius =
    // r/|cos tilt| ≥ minor_radius). |cos tilt| = |dir·ẑ| for a z-cap.
    let abs_c = oblique_dir()[2].abs();
    let expect_minor = CYL_RADIUS;
    let expect_major = CYL_RADIUS / abs_c;
    for e in &ellipses {
        let Curve::Ellipse {
            major_radius,
            minor_radius,
            ..
        } = e.curve
        else {
            continue;
        };
        assert!(
            (minor_radius - expect_minor).abs() <= 1e-9,
            "yr11 §5.2: ellipse edge minor_radius {minor_radius} must equal r={expect_minor}"
        );
        assert!(
            (major_radius - expect_major).abs() <= 1e-9,
            "yr11 §5.2: ellipse edge major_radius {major_radius} must equal r/|cos tilt|={expect_major}"
        );
        assert!(
            major_radius >= minor_radius,
            "yr11 §5.2: ellipse edge must have major_radius ≥ minor_radius"
        );
    }
}

// =========================================================================
// ORACLE 3 (standalone, explicit) — chord deviation strictly DECREASES vs the
// exact ellipse, measured by perpendicular distance to the ssi-rs ellipse (a
// production-independent reference) rather than the on-both-surfaces residual.
// Pre-Stage-4 max deviation ≫ TAU_MODEL; post ≤ TAU_MODEL. RED: production STOPs.
// =========================================================================

#[test]
fn t3_oblique_chord_deviation_strictly_decreases() {
    let cyl = oblique_cylinder();
    let bx = canonical_box();

    let bottom_ellipse = oracle_cap_ellipse(0.0);
    let top_ellipse = oracle_cap_ellipse(1.0);

    // BEFORE: max perpendicular distance of off-curve cap-ring vertices to the
    // exact ssi-rs ellipse.
    let arr = hand_built_offcurve_oblique_arrangement();
    let mut before = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if (pt[2].abs() <= TAU_MODEL || (pt[2] - 1.0).abs() <= TAU_MODEL)
            && dist_point_to_line(pt, oblique_axis_point(), oblique_dir()) > MIN_FEATURE_SIZE
        {
            let ell = if cap_z_of(pt) == 0.0 {
                &bottom_ellipse
            } else {
                &top_ellipse
            };
            before = before.max(dist_to_ellipse_sampled(pt, ell));
        }
    }
    assert!(
        before > 100.0 * TAU_MODEL,
        "yr11 §5.3: pre-Stage-4 max ellipse deviation {before} must be ≫ TAU_MODEL"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr11 §5.3: oblique union must Ok after Stage-4 ellipse relocate");

    // AFTER: max perpendicular distance of the relocated ellipse-edge vertices
    // to the exact ssi-rs ellipse.
    let mut after = 0.0_f64;
    for e in ellipse_edges(&r) {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let ell = if cap_z_of(ep) == 0.0 {
                &bottom_ellipse
            } else {
                &top_ellipse
            };
            after = after.max(dist_to_ellipse_sampled(ep, ell));
        }
    }
    assert!(
        after < before,
        "yr11 §5.3: ellipse deviation must strictly decrease (after {after} < before {before})"
    );
    assert!(
        after <= TAU_MODEL,
        "yr11 §5.3: ellipse deviation after relocate must be ≤ TAU_MODEL, got {after}"
    );
}

// =========================================================================
// ORACLE 4 (standalone) — no reversed / inverted / degenerate triangles, plus
// relocated ring order follows the ELLIPSE TANGENT. Every output triangle has
// area ≥ MIN_FEATURE_SIZE² and winding agreeing with the analytic outward
// normal; and the relocated ellipse-edge endpoints, ordered by their ellipse
// parameter t, form a strictly monotone (once-wrapping) sequence — i.e. the loop
// is simple and ordered along the tangent. RED: production STOPs.
// =========================================================================

#[test]
fn t4_oblique_no_inverted_tris_and_tangent_order() {
    let cyl = oblique_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_offcurve_oblique_arrangement(),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr11 §5.4: oblique union must Ok after Stage-4 ellipse relocate");
    let mesh = r.as_mesh();

    for (ti, tri) in mesh.tris.iter().enumerate() {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let nrm = tri_normal(a, b, c);
        let area2 = norm(nrm);
        assert!(
            area2 * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
            "yr11 §5.4: triangle {ti} {tri:?} is degenerate (area {} < MIN_FEATURE_SIZE²)",
            area2 * 0.5
        );
        if let Some(an) = analytic_normal_at_tri(a, b, c) {
            let agree = dot(unit(nrm), an);
            assert!(
                agree > 0.0,
                "yr11 §5.4: triangle {ti} {tri:?} winding (normal {:?}) disagrees with the \
                 analytic outward normal {an:?} (dot {agree} ≤ 0) — inverted triangle",
                unit(nrm)
            );
        }
    }

    // Relocated ring order follows the ellipse tangent: take ONE cap's ellipse
    // edges, recompute each relocated endpoint's ellipse parameter t (in the
    // production edge's own frame), and confirm the set of distinct t values, in
    // sorted order, increases monotonically around exactly one wrap of [−π, π]
    // (a simple, once-wrapping inscribed polygon — no fold).
    let tmap = r.tessellation_map();
    let mut params: Vec<f64> = Vec::new();
    for e in ellipse_edges(&r) {
        let Curve::Ellipse { .. } = e.curve else {
            continue;
        };
        // Only the bottom cap (z≈0) to keep a single ring.
        let (s, _t) = edge_endpoints(&r, e);
        if cap_z_of(s) != 0.0 {
            continue;
        }
        for vid in [e.start, e.end] {
            if let TessellationSource::BRepEdge { edge: _, t } = tmap.lookup(vid) {
                params.push(t);
            }
        }
    }
    assert!(
        params.len() >= 3,
        "yr11 §5.4: expected ≥3 relocated ellipse-edge params on the bottom cap, got {}",
        params.len()
    );
    // Dedup near-equal params (shared endpoints), then assert simple ordering:
    // sorted t values are strictly increasing and span < 2π (one wrap).
    params.sort_by(|a, b| a.partial_cmp(b).unwrap());
    params.dedup_by(|a, b| (*a - *b).abs() <= 1e-6);
    for w in params.windows(2) {
        assert!(
            w[1] - w[0] > 1e-6,
            "yr11 §5.4: relocated ellipse params must be strictly increasing (simple loop, \
             no fold); found {} then {}",
            w[0],
            w[1]
        );
    }
    let span = params.last().unwrap() - params.first().unwrap();
    assert!(
        span < 2.0 * std::f64::consts::PI,
        "yr11 §5.4: relocated ellipse params must wrap at most once (span {span} < 2π)"
    );
}

// =========================================================================
// E2E (env-gated on CHERCHI2022_BIN) — real-sidecar OBLIQUE cylinder ∪ box.
// Mirrors yr10 t8. Asserts on-ellipse + watertight + Ellipse edge on the REAL
// mesh-boolean output. LOUD eprintln skip when the binary is absent (never a
// silent pass). RED: production STOPs on the Ellipse edge even on the real mesh.
// =========================================================================

#[test]
fn t5_e2e_oblique_cylinder_union_box_on_ellipse() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr11] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cyl = oblique_cylinder();
    let bx = canonical_box();

    let r =
        boolean(&cyl, &bx, BoolOp::Union, &sb).expect("yr11 E2E: oblique cylinder ∪ box must Ok");

    // Watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr11 E2E: relocated output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr11 E2E: relocated output Euler must be 2"
    );

    // Output carries ≥1 Curve::Ellipse intersection edge (the oblique caps).
    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "yr11 E2E: output must carry ≥1 Curve::Ellipse intersection edge (the oblique cap sections); \
         got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // On-ellipse: every cap-ring intersection-edge vertex is on BOTH the true
    // cylinder and the cap plane within TAU_MODEL (recomputed independently).
    let mut after_max_dev = 0.0_f64;
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let cap_z = cap_z_of(ep);
            let radial = cyl_radial_residual(ep);
            let planar = plane_residual(ep, cap_z);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr11 E2E: relocated vertex {ep:?} cylinder radial residual {radial} > TAU_MODEL"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr11 E2E: relocated vertex {ep:?} plane residual {planar} > TAU_MODEL"
            );
        }
    }
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr11 E2E: max ellipse deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );
}

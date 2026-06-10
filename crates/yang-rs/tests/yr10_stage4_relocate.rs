//! PR-YR10 RED — Stage 4: RELOCATE mesh intersection points onto the exact
//! analytical curve + §4.5.3 reversed-point correction.
//!
//! Spec of record: `specs/yang_pr_yr10_stage4_relocate.md` (§5 is the RED
//! contract). Paper: Yang 2025 §4.4.1 (mesh updating / relocation) + §4.5.3
//! (correction of reversed intersection) + §4.5.2 (local refinement = STOP).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY;
//! the GREEN implementer extends `crates/yang-rs/src/lib.rs` (the Stage-4
//! relocate + reversal sweep inside `reconstruct_topology` / `boolean()`) to
//! make these oracles GREEN. The RED author NEVER edits production code.
//!
//! ## RED state (two-stage, mirroring yr9_stage3_ssi.rs)
//!
//! 1. **Compile gate (the GREEN API surface).** This file references the public
//!    error surface the GREEN implementer ADDS (spec §4.2):
//!    `yang_rs::Stage4InvalidReason` and the `YangError::Stage4RegionInvalid` /
//!    `YangError::Stage4ReversalUnresolved` variants. Those do NOT exist yet, so
//!    **this file does not compile against current production**. That is the
//!    intended initial RED state: it turns from compile-fail to assert-fail the
//!    moment GREEN lands the error variants.
//!
//! 2. **Behavioral gate (off-curve mesh → on-curve mesh).** The fixtures place
//!    the cap-ring vertices genuinely OFF the exact `Curve::Circle` (on a chord
//!    inside the circle at radius `r' = CYL_RADIUS − δ`, `TAU_WORK < δ ≤ d_ε`).
//!    Pre-Stage-4 the output ring sits at `r'` (off-curve), so the on-curve
//!    (`ρ ≤ TAU_MODEL`) and chord-deviation-decrease assertions FAIL. After
//!    GREEN relocates them onto the exact circle, they pass.
//!
//! Per the established repo convention (integration test files cannot share
//! helpers), the yr9 harness (`p`, array math, `cylinder_brep`,
//! `unit_cube_brep_offset_at`, `d_eps`/`analytic_aabb_diagonal`, canonical
//! config, `LabelMock`, `hand_built_*_arrangement`, `unpaired_half_edges`,
//! `euler_characteristic`, `one_triangle`, the ssi-rs oracle helpers) is
//! re-declared verbatim here. The on-curve oracle (the exact circle the output
//! must match) is computed by calling `ssi_rs::intersect` DIRECTLY from the
//! test, independently of production.
//!
//! Tolerances (spec §5 / §"Tolerances", do NOT weaken):
//!   - On-curve / round-trip / after-deviation: `cad_primitives::TAU_MODEL` (1e-7).
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
// Pure-Rust array math (cad-primitives has no dot/cross/normalize helpers).
// Re-declared verbatim from tests/yr9_stage3_ssi.rs.
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
// Cylinder B-Rep fixture (seam-edge encoding). Re-declared from yr9.
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
// Unit-cube fixture with TRUE per-face plane offsets. Re-declared from yr9.
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
// Analytic mesh oracles. Re-declared from yr9.
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
// Canonical config: cylinder axis +Z through the unit cube at the origin.
//   box      = unit_cube_brep_offset_at([0,0,0])  (spans 0..1 in x,y,z)
//   cylinder = cylinder_brep([0.5,0.5,-0.5], +Z, r=0.25, h=2.0)
// The two box caps are z=0 (normal (0,0,-1), d=0) and z=1 (normal (0,0,1),
// d=-1). `ssi_rs::intersect(Plane(cap), Cylinder)` for each cap returns one
// `SsiCurve::Circle`: center (0.5,0.5,0)/(0.5,0.5,1), normal +Z, radius 0.25.
// =========================================================================

const CYL_AXIS_POINT: [f64; 3] = [0.5, 0.5, -0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_RADIUS: f64 = 0.25;
const CYL_HEIGHT: f64 = 2.0;

fn canonical_cylinder() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT)
}
fn canonical_box() -> BRep {
    unit_cube_brep_offset_at([0.0, 0.0, 0.0])
}

fn canonical_cylinder_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_RADIUS,
    }
}

// =========================================================================
// SSI ORACLE — compute the EXACT cap circle independently via ssi-rs.
// Re-declared from yr9.
// =========================================================================

fn surface_to_quadric(s: Surface) -> ssi_rs::QuadricSurface {
    match s {
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

/// The EXACT cap circle on plane z = `cap_z` (cylinder ∩ that box cap),
/// computed by ssi-rs. Bottom cap z=0 (normal (0,0,-1), d=0); top cap z=1
/// (normal (0,0,1), d=-1).
fn oracle_cap_circle(cap_z: f64) -> ssi_rs::SsiCurve {
    let (normal, d) = if cap_z == 0.0 {
        (Vector3::new(0.0, 0.0, -1.0), 0.0)
    } else {
        (Vector3::new(0.0, 0.0, 1.0), -cap_z)
    };
    let plane = surface_to_quadric(Surface::Plane { normal, d });
    let cyl = surface_to_quadric(canonical_cylinder_surface());
    let curves = ssi_rs::intersect(&plane, &cyl)
        .expect("oracle: Plane∩Cylinder must succeed for a perpendicular cap");
    assert_eq!(
        curves.len(),
        1,
        "oracle: perpendicular cap section must be exactly one curve, got {curves:?}"
    );
    curves[0]
}

/// Residual `ρ = max(|axial|, |radial − r|)` of `pt` to an exact `Circle`.
/// This IS the spec §4.5 residual the relocation must drive ≤ TAU_MODEL.
fn circle_residual(pt: [f64; 3], center: [f64; 3], normal: [f64; 3], radius: f64) -> f64 {
    let n = unit(normal);
    let w = sub(pt, center);
    let axial = dot(w, n).abs();
    let radial = norm(sub(w, scale(n, dot(w, n))));
    axial.max((radial - radius).abs())
}

/// Residual of `pt` to the cap circle on plane z=`cap_z` (the SSI oracle).
fn residual_to_cap_circle(pt: [f64; 3], cap_z: f64) -> f64 {
    let ssi_rs::SsiCurve::Circle {
        center,
        normal,
        radius,
    } = oracle_cap_circle(cap_z)
    else {
        panic!("oracle: cap section must be a circle");
    };
    circle_residual(pt, center.as_array(), normal.as_array(), radius)
}

// =========================================================================
// `ortho_basis` re-implemented in-test (spec §3 / lib.rs:816). The relocation
// `t` is `atan2(v, u)` in THIS frame, so the round-trip oracle must reproduce
// it bit-for-bit.
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

/// Reproduce `eval_source`'s Circle inversion: `center + r·(cos t·e1 + sin t·e2)`
/// with `ortho_basis(normal)` — the SAME frame production uses.
fn eval_circle_source(center: [f64; 3], normal: [f64; 3], radius: f64, t: f64) -> [f64; 3] {
    let (e1, e2) = ortho_basis(normal);
    add(
        center,
        scale(add(scale(e1, t.cos()), scale(e2, t.sin())), radius),
    )
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement.
// Re-declared from yr9.
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

// N=16 (not 8): the off-curve fixtures pull the ENTIRE ring (lateral walls
// included) inward by δ=0.4·d_ε, so the lateral-wall triangle centroid is
// offset from the true cylinder surface. At N=8 that offset (0.0248) exceeds
// the UNCHANGED attribution band d_ε=0.0212, so `boolean()` correctly returns
// FaceResolutionFailed BEFORE Stage 4 runs. At N=16 the lateral centroid offset
// is 0.0126 < d_ε (in band), so attribution succeeds and Stage 4 is reached.
// The crossing-vertex residual ρ=δ=0.0085 ≤ d_ε is unchanged by N.
const N_FACETS: usize = 16;

// =========================================================================
// Hand-built tube arrangement with the rim ring at radius `r' = r − δ` so the
// cap-ring intersection-edge endpoints sit OFF the exact circle (inside it) by
// ~δ pre-Stage-4. The exact SSI circle is computed from the INPUT cylinder
// (radius CYL_RADIUS) and is UNCHANGED. The cap planes z=0 / z=1 stay exact
// (rim verts at z=0 and z=1 exactly), so the caps remain on-plane.
//
//   - N=8 facets. 8 lateral QUADS (16 tris) → label InputId(0) = CYLINDER.
//   - bottom cap fan (8 tris) z=0 + top cap fan (8 tris) z=1 → InputId(1) = BOX.
//   - cap-ring boundary edges (lateral A ↔ cap-fan B) are the OUTPUT
//     INTERSECTION EDGES whose vertices must be relocated onto the cap Circle.
//   V=18, F=32. Watertight, Euler 2. `inside` all-false ⇒ Union keeps all.
// =========================================================================

fn hand_built_tube_arrangement_at_radius(rprime: f64) -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let (za, zb) = (0.0f64, 1.0f64); // box z extent (cap planes, kept exact)

    let ring: Vec<(f64, f64)> = (0..N_FACETS)
        .map(|k| {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            (cx + rprime * th.cos(), cy + rprime * th.sin())
        })
        .collect();

    build_tube_from_ring(&ring, za, zb, /*single_input=*/ false)
}

/// Build a 2-label (lateral=A, caps=B) closed tube+caps arrangement from a
/// pre-computed (x,y) ring and z extents. Factored so the synthetic
/// reversed-loop fixture can hand-place individual ring vertices.
fn build_tube_from_ring(
    ring: &[(f64, f64)],
    za: f64,
    zb: f64,
    single_input: bool,
) -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let n_facets = ring.len();

    let mut verts: Vec<Point3> = Vec::new();
    let mut bot = Vec::with_capacity(n_facets);
    let mut top = Vec::with_capacity(n_facets);
    for &(x, y) in ring {
        bot.push(verts.len() as u32);
        verts.push(p(x, y, za));
    }
    for &(x, y) in ring {
        top.push(verts.len() as u32);
        verts.push(p(x, y, zb));
    }
    let cb = verts.len() as u32;
    verts.push(p(cx, cy, za));
    let ct = verts.len() as u32;
    verts.push(p(cx, cy, zb));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    let cap_label = if single_input { 0 } else { 1 };
    let mut push = |t: [u32; 3], label: u32| {
        tris.push(t);
        surface.push(vec![LaInputId(label)]);
    };

    // Lateral walls → CYLINDER (label 0).
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([bot[k], bot[k1], top[k1]], 0);
        push([bot[k], top[k1], top[k]], 0);
    }
    // Bottom cap fan (z=za, outward −z) → cap label.
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([cb, bot[k1], bot[k]], cap_label);
    }
    // Top cap fan (z=zb, outward +z) → cap label.
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([ct, top[k], top[k1]], cap_label);
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

/// Build a 2-label closed tube+caps from explicit 3D bottom/top rings and cap
/// centers (lateral=A label 0, caps=B label 1). Unlike `build_tube_from_ring`
/// the rings need not be vertically stacked, so the lateral walls can track an
/// OBLIQUE cylinder surface (rings sampled on its two z-section ellipses) — this
/// is what lets the ellipse fixture clear face attribution and reach Stage 4.
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

/// The off-curve δ used by the canonical Stage-4 fixtures: strictly inside the
/// `(TAU_WORK, d_ε]` relocate band (`δ = 0.4 · d_ε`), so the cap-ring vertices
/// are genuinely off-curve (`ρ ≈ δ ≫ TAU_MODEL`) yet still relocatable.
fn relocate_band_delta() -> f64 {
    0.4 * d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT)
}

/// Canonical off-curve tube fixture: rim ring at `r' = CYL_RADIUS − 0.4·d_ε`.
fn hand_built_offcurve_tube_arrangement() -> LabeledArrangement {
    hand_built_tube_arrangement_at_radius(CYL_RADIUS - relocate_band_delta())
}

// =========================================================================
// Output-edge helpers. Re-declared from yr9.
// =========================================================================

fn edge_endpoints(brep: &BRep, e: &BRepEdge) -> ([f64; 3], [f64; 3]) {
    let vs = brep.vertices();
    (
        vs[e.start as usize].point.as_array(),
        vs[e.end as usize].point.as_array(),
    )
}

fn conic_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Circle { .. } | Curve::Ellipse { .. }))
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
/// cylinder radial normal at the centroid. Used to check winding agreement.
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
    // Lateral: outward radial at the centroid.
    let centroid = scale(add(add(a, b), c), 1.0 / 3.0);
    let axis_unit = unit(CYL_AXIS_DIR);
    let w = sub(centroid, CYL_AXIS_POINT);
    let radial = sub(w, scale(axis_unit, dot(w, axis_unit)));
    if norm(radial) < MIN_FEATURE_SIZE {
        return None;
    }
    Some(unit(radial))
}

// =========================================================================
// ORACLE 1 + 2 + 3 + 5 + 7 — the core relocation oracle on the canonical
// off-curve tube. Drives boolean() with the off-curve LabelMock and asserts:
//   1. every relocated intersection-edge vertex is on the exact circle ≤ TAU_MODEL,
//   2. max chord deviation strictly DECREASES (before > after, after ≤ TAU_MODEL),
//   3. output is watertight 2-manifold (0 unpaired, Euler 2),
//   5. relocated verts' TessellationSource is BRepEdge{edge,t}, round-tripping
//      through eval_circle_source to the relocated position within TAU_MODEL,
//   7. determinism (two runs byte-identical).
//
// RED: pre-Stage-4 production leaves the ring at r' (off-curve), so oracle 1/2
// fail; and the file does not compile until GREEN adds Stage4InvalidReason.
// =========================================================================

#[test]
fn t1_relocate_on_curve_chord_decreases_watertight() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let delta = relocate_band_delta();
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);

    // Sanity on the fixture: TAU_WORK < δ ≤ d_ε (genuinely off-curve, relocatable).
    assert!(
        delta > TAU_WORK && delta <= de,
        "fixture δ={delta} must lie in (TAU_WORK, d_ε={de}]"
    );

    // BEFORE: the hand-built ring vertices are off the exact cap circle by ~δ.
    let arr = hand_built_offcurve_tube_arrangement();
    let mut before_max_dev = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        // Only the cap-ring vertices (z≈0 or z≈1, off the axis) are intersection
        // endpoints; the cap centers sit on the axis and are not relocated.
        if (pt[2].abs() <= TAU_MODEL || (pt[2] - 1.0).abs() <= TAU_MODEL)
            && dist_point_to_line(pt, CYL_AXIS_POINT, unit(CYL_AXIS_DIR)) > MIN_FEATURE_SIZE
        {
            before_max_dev = before_max_dev.max(residual_to_cap_circle(pt, cap_z_of(pt)));
        }
    }
    assert!(
        before_max_dev > 100.0 * TAU_MODEL,
        "fixture must start genuinely off-curve (before_max_dev={before_max_dev} \
         should be ≫ TAU_MODEL); δ={delta}"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr10: cylinder ∪ box (off-curve mock) must return Ok after Stage-4 relocate");

    // ORACLE 3: watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr10 §5.3: relocated output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr10 §5.3: relocated output Euler V−E+F must be 2"
    );

    // The cap rings must still carry exact Circle curves (inherited from YR9).
    let conics = conic_edges(&r);
    assert!(
        !conics.is_empty(),
        "yr10: expected ≥1 cap-ring Circle intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // ORACLE 1: every relocated intersection-edge vertex's residual ≤ TAU_MODEL.
    // ORACLE 2 (after): the max deviation of the polyline AFTER.
    let mut after_max_dev = 0.0_f64;
    for e in &conics {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            continue;
        };
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let rho = circle_residual(ep, center.as_array(), normal.as_array(), radius);
            after_max_dev = after_max_dev.max(rho);
            assert!(
                rho <= TAU_MODEL,
                "yr10 §5.1: relocated intersection-edge vertex {ep:?} residual {rho} \
                 to the exact circle must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
        }
    }

    // ORACLE 2: chord deviation strictly decreases (and ends ≤ TAU_MODEL).
    assert!(
        after_max_dev < before_max_dev,
        "yr10 §5.2: max chord deviation must strictly decrease (after {after_max_dev} \
         < before {before_max_dev}) — proves real relocation, not a no-op"
    );
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr10 §5.2: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );

    // ORACLE 5: relocated verts carry BRepEdge{edge,t}; inverting via
    // eval_circle_source (the eval_source Circle formula) reproduces the
    // relocated mesh position within TAU_MODEL.
    let tmap = r.tessellation_map();
    let mesh = r.as_mesh();
    let mut saw_relocated_edge_source = false;
    for e in &conics {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            continue;
        };
        for vid in [e.start, e.end] {
            let src = tmap.lookup(vid);
            match src {
                TessellationSource::BRepEdge { edge: _, t } => {
                    saw_relocated_edge_source = true;
                    let inverted =
                        eval_circle_source(center.as_array(), normal.as_array(), radius, t);
                    let mesh_pos = mesh.verts[vid as usize].as_array();
                    let d = norm(sub(inverted, mesh_pos));
                    assert!(
                        d <= TAU_MODEL,
                        "yr10 §5.5: relocated vertex {vid} BRepEdge t={t} must invert (via the \
                         Circle eval_source formula) to the mesh position within TAU_MODEL, off by {d}"
                    );
                }
                other => panic!(
                    "yr10 §5.5: relocated intersection-edge vertex {vid} must carry \
                     TessellationSource::BRepEdge{{edge,t}}, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw_relocated_edge_source,
        "yr10 §5.5: at least one relocated vertex must carry a BRepEdge source"
    );

    // ORACLE 7: determinism — a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: hand_built_offcurve_tube_arrangement(),
    };
    let r2 = boolean(&cyl, &bx, BoolOp::Union, &mock2).expect("yr10: determinism run 2");
    assert_eq!(
        r, r2,
        "yr10 §5.7: identical inputs must produce a byte-identical output BRep"
    );
}

// =========================================================================
// ORACLE 4 — no reversed / inverted / degenerate triangles, on the canonical
// off-curve fixture: every output triangle has positive area, area ≥
// MIN_FEATURE_SIZE², and winding agrees with the analytic surface normal where
// determinable.
// =========================================================================

#[test]
fn t2_no_inverted_or_degenerate_triangles() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_offcurve_tube_arrangement(),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr10: off-curve union must Ok after Stage-4");
    let mesh = r.as_mesh();

    for (ti, tri) in mesh.tris.iter().enumerate() {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let nrm = tri_normal(a, b, c);
        let area2 = norm(nrm);
        // Positive area, ≥ MIN_FEATURE_SIZE² (the spec §4.5 degeneracy gate).
        assert!(
            area2 * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
            "yr10 §5.4: triangle {ti} {tri:?} is degenerate (area {} < MIN_FEATURE_SIZE²)",
            area2 * 0.5
        );
        // Winding agrees with the analytic outward normal where determinable.
        if let Some(an) = analytic_normal_at_tri(a, b, c) {
            let agree = dot(unit(nrm), an);
            assert!(
                agree > 0.0,
                "yr10 §5.4: triangle {ti} {tri:?} winding (normal {:?}) disagrees with the \
                 analytic outward surface normal {an:?} (dot {agree} ≤ 0) — inverted triangle",
                unit(nrm)
            );
        }
    }
}

// =========================================================================
// ORACLE 4 (reversal) — synthetic reversed-loop fixture exercising the §4.5.3
// collapse. We perturb ONE rim vertex's ANGLE past its neighbour so that, after
// radial projection onto the exact circle, the loop's angular order locally
// reverses, triggering the §4.5.3 reversed-intersection correction.
//
// REACHABILITY NOTE (verified): inducing a genuine angular reversal requires
// moving a shared ring vertex angularly PAST its neighbour, which (at any
// N) pushes the two lateral walls incident to that vertex off the input
// cylinder by > d_ε — so face attribution legitimately fails BEFORE Stage 4
// (the lateral walls and the cap-ring crossing share the same vertices). The
// reversed loop is therefore EITHER corrected by the §4.5.3 sweep (when the
// distortion stays in-band) producing a watertight inversion-free output, OR
// rejected by a loud P9/P10 STOP (Stage-4 reversal/collapse failure, OR the
// upstream attribution / SSI-selection gate). The invariant under test is the
// SAFETY property: a reversed loop is NEVER silently emitted as an inverted /
// non-watertight mesh.
// =========================================================================

/// Tube whose bottom+top rim rings have vertex k=2's ANGLE pushed PAST vertex
/// k=3's, while still inside the `(TAU_WORK, d_ε]` relocate band radially. After
/// radial projection onto the exact circle the angular order 1→2→3 reverses
/// locally at k=2, triggering the §4.5.3 reversed-intersection correction.
fn hand_built_reversed_loop_arrangement() -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let rprime = CYL_RADIUS - relocate_band_delta();
    let n = N_FACETS;
    let base = |k: usize| 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
    let ring: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            // Push k=2 angularly past k=3 (its successor) so the projected order
            // 1→2→3 reverses at k=2.
            let th = if k == 2 {
                base(3) + 0.25 * (base(4) - base(3))
            } else {
                base(k)
            };
            (cx + rprime * th.cos(), cy + rprime * th.sin())
        })
        .collect();
    build_tube_from_ring(&ring, 0.0, 1.0, /*single_input=*/ false)
}

#[test]
fn t3_reversed_loop_corrected_output_watertight() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_reversed_loop_arrangement(),
    };

    // The §4.5.3 sweep must EITHER resolve the reversal (collapse the offending
    // point, reconnect) and emit a watertight, inversion-free output, OR fail
    // LOUDLY with a Stage4* error — never silently emit an inverted mesh.
    match boolean(&cyl, &bx, BoolOp::Union, &mock) {
        Ok(r) => {
            // Resolved: output is watertight + has no inverted/degenerate tris.
            assert_eq!(
                unpaired_half_edges(r.as_mesh()),
                0,
                "yr10 §5.4: §4.5.3-corrected output must be watertight (0 unpaired)"
            );
            assert_eq!(
                euler_characteristic(r.as_mesh()),
                2,
                "yr10 §5.4: §4.5.3-corrected output Euler must be 2"
            );
            let mesh = r.as_mesh();
            for (ti, tri) in mesh.tris.iter().enumerate() {
                let a = mesh.verts[tri[0] as usize].as_array();
                let b = mesh.verts[tri[1] as usize].as_array();
                let c = mesh.verts[tri[2] as usize].as_array();
                let nrm = tri_normal(a, b, c);
                assert!(
                    norm(nrm) * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
                    "yr10 §5.4: §4.5.3-corrected triangle {ti} {tri:?} is degenerate"
                );
                if let Some(an) = analytic_normal_at_tri(a, b, c) {
                    assert!(
                        dot(unit(nrm), an) > 0.0,
                        "yr10 §5.4: §4.5.3-corrected triangle {ti} {tri:?} is inverted"
                    );
                }
            }
            // Every remaining intersection-edge vertex is on-curve.
            for e in conic_edges(&r) {
                let Curve::Circle {
                    center,
                    normal,
                    radius,
                } = e.curve
                else {
                    continue;
                };
                let (s, t) = edge_endpoints(&r, e);
                for ep in [s, t] {
                    let rho = circle_residual(ep, center.as_array(), normal.as_array(), radius);
                    assert!(
                        rho <= TAU_MODEL,
                        "yr10 §5.4: surviving relocated vertex {ep:?} residual {rho} > TAU_MODEL"
                    );
                }
            }
        }
        Err(YangError::Stage4ReversalUnresolved { .. })
        | Err(YangError::Stage4RegionInvalid { .. })
        | Err(YangError::SsiRefinementFailed { .. })
        | Err(YangError::FaceResolutionFailed { .. }) => {
            // Acceptable LOUD stop: the reversal could not be resolved by the
            // §4.5.3 collapse alone (genuine §4.5.2 local-refinement territory),
            // OR the angular distortion tripped the upstream attribution /
            // SSI-selection gate first. Either way it is a P9/P10-honest
            // failure, NOT a silently-emitted inverted / non-watertight mesh.
        }
        other => panic!(
            "yr10 §5.4: a reversed loop must EITHER produce a watertight inversion-free \
             output OR fail loudly (Stage4* / SsiRefinementFailed / FaceResolutionFailed), \
             never silently emit an inverted mesh; got {other:?}"
        ),
    }
}

// =========================================================================
// ORACLE 6a — LOUD ellipse rejection: an oblique cap section is an Ellipse, so
// Stage 4 must return Stage4RegionInvalid{reason: EllipseProjectionUnsupported}
// (no silent snap).
//
// Construction: an OBLIQUE cylinder whose axis is tilted relative to the box
// cap plane z=0 ⇒ the cap section is an ELLIPSE. We feed an off-curve elliptical
// ring through the LabelMock so the cap-ring intersection edge's incident
// surfaces (oblique Cylinder ∩ z-plane) force `ssi_rs::intersect` to yield an
// Ellipse curve — which the Circle-only Stage-4 projection must reject loudly.
// =========================================================================

/// An oblique cylinder (axis tilted in the x-z plane) of the same radius. Its
/// section by a z=const plane is an ELLIPSE. The lateral face is a
/// `Surface::Cylinder` with this tilted axis.
fn oblique_cylinder() -> BRep {
    // Axis direction tilted 30° off +Z toward +X; long enough to span z∈[−0.5,1.5].
    let dir = unit([0.5, 0.0, 1.0]);
    let height = 3.0;
    // Place the axis so it threads the unit box near (0.5,0.5).
    let axis_point = [0.5 - 0.5 * dir[0], 0.5 - 0.5 * dir[1], -0.5];
    cylinder_brep(axis_point, dir, CYL_RADIUS, height)
}

/// Build a tube whose lateral label is solid A (the OBLIQUE cylinder) and cap
/// fans are solid B (the box z=0/z=1 planes), with each ring sampled ON its
/// cap's elliptical z-section of the oblique cylinder. Sampling both rings on
/// the oblique SURFACE (rather than stacking a vertical ring) keeps the lateral
/// walls within the input cylinder's chord band so face attribution SUCCEEDS
/// and the pipeline reaches Stage 4 — where the cap edge's incident surfaces
/// (oblique Cylinder ∩ z-plane) yield an `Ellipse` curve that the Circle-only
/// Stage-4 projection must reject with `EllipseProjectionUnsupported`.
fn hand_built_oblique_ellipse_arrangement() -> LabeledArrangement {
    let dir = unit([0.5, 0.0, 1.0]);
    let axis_point = [0.5 - 0.5 * dir[0], 0.5 - 0.5 * dir[1], -0.5];
    let (e1, e2) = ortho_basis(dir);
    // On-surface point of the oblique cylinder at angle θ and axial param s.
    let surf = |theta: f64, s: f64| -> [f64; 3] {
        add(
            add(axis_point, scale(dir, s)),
            scale(
                add(scale(e1, theta.cos()), scale(e2, theta.sin())),
                CYL_RADIUS,
            ),
        )
    };
    // Axial s so that surf(θ, s).z == cap_z (the z-section ellipse sample).
    let s_for = |cap_z: f64, theta: f64| -> f64 {
        let radial_z = CYL_RADIUS * (theta.cos() * e1[2] + theta.sin() * e2[2]);
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
    // Cap centers = the z-section ellipse centers (mean of each ring), on-plane.
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

// PR-YR11 contract migration: this test originally asserted that an Ellipse
// intersection edge was REJECTED loudly (`Stage4RegionInvalid::
// EllipseProjectionUnsupported`) — the YR10-era scope where oblique relocation
// was unimplemented. PR-YR11 implements oblique-ellipse relocation (via the
// cylinder parameterization), so the oblique cylinder ∪ box now SUCCEEDS,
// carrying exact `Curve::Ellipse` cap edges. Faithful migration: the independent
// ssi-rs ellipse oracle and the `boolean()` call are unchanged; only the
// expected outcome flips Err → Ok-with-Ellipse.
#[test]
fn t4_ellipse_edge_relocates() {
    // Independent oracle: an oblique-cylinder ∩ z-plane really is an Ellipse.
    let dir = unit([0.5, 0.0, 1.0]);
    let axis_point = [0.5 - 0.5 * dir[0], 0.5 - 0.5 * dir[1], -0.5];
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
    // PR-YR11: oblique relocation is implemented — the Ellipse edge must now
    // RELOCATE (Ok), not reject; the output carries ≥1 exact Curve::Ellipse edge.
    let r = boolean(&a, &b, BoolOp::Union, &mock)
        .expect("yr11: oblique Ellipse edge must now relocate (Ok), not reject");
    let n_ellipse = r
        .edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Ellipse { .. }))
        .count();
    assert!(
        n_ellipse >= 1,
        "yr11: oblique cap section must be carried as a Curve::Ellipse edge, got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );
}

// =========================================================================
// ORACLE 6b — LOUD on-axis rejection. The Stage-4 `OnAxis` check lives in
// `project_onto_circle` (radial component < MIN_FEATURE_SIZE). It is a
// DEFENSIVE guard: an on-axis intersection-edge endpoint has circle residual
// ρ ≈ radius (0.25) ≫ d_ε, so the UPSTREAM `build_intersection_curves`
// selection — which requires `curve_contains_point(endpoint, d_ε)` for BOTH
// endpoints (lib.rs:1399, the SAME band) — provably rejects the edge with a
// loud `SsiRefinementFailed { matched: 0 }` BEFORE Stage 4 ever projects it.
// (Verified: the on-axis collapse also pushes the two adjacent lateral walls
// far off the cylinder, so `FaceResolutionFailed` may fire even earlier.)
//
// The oracle's SAFETY intent — an on-axis pathological crossing is rejected
// LOUDLY, never silently snapped onto the curve — holds regardless of WHICH
// honest P9/P10 gate fires. We therefore assert a loud `Err` of an honest STOP
// variant and NEVER `Ok` (a silent snap), plus an independent oracle confirming
// the on-axis point genuinely has ρ ≫ d_ε. Stage-4-internal `OnAxis`
// reachability would require a private-fn unit test (no public seam exists);
// that is the GREEN/adversary layer's concern, not this RED file's.
// =========================================================================

fn hand_built_onaxis_arrangement() -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let rprime = CYL_RADIUS - relocate_band_delta();
    let n = N_FACETS;
    let ring: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            if k == 0 {
                // Vertex 0 collapsed onto the axis (the cap center) → on-axis.
                (cx, cy)
            } else {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                (cx + rprime * th.cos(), cy + rprime * th.sin())
            }
        })
        .collect();
    build_tube_from_ring(&ring, 0.0, 1.0, /*single_input=*/ false)
}

#[test]
fn t5_on_axis_projection_rejected_loudly() {
    // Independent oracle: the on-axis point's residual to the exact cap circle
    // is ≈ radius ≫ d_ε (so the upstream selection band cannot contain it).
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);
    let on_axis_residual = residual_to_cap_circle([CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], 0.0], 0.0);
    assert!(
        on_axis_residual > de,
        "oracle: an on-axis crossing point's residual {on_axis_residual} must exceed d_ε {de}"
    );

    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_onaxis_arrangement(),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock);
    // LOUD rejection, never a silent snap (Ok). Accept the Stage-4 `OnAxis` guard
    // OR the upstream loud gate that provably fires first on this pathology.
    match r {
        Err(YangError::Stage4RegionInvalid {
            reason: Stage4InvalidReason::OnAxis,
            ..
        })
        | Err(YangError::SsiRefinementFailed { .. })
        | Err(YangError::FaceResolutionFailed { .. }) => {}
        Ok(_) => panic!(
            "yr10 §5.6b: an on-axis (degenerate radial) crossing must be rejected LOUDLY, \
             never silently snapped — got Ok"
        ),
        other => panic!(
            "yr10 §5.6b: an on-axis crossing must fail with a loud P9/P10 STOP \
             (Stage4 OnAxis, or the upstream SsiRefinementFailed / FaceResolutionFailed \
             gate that fires first on the same band), got {other:?}"
        ),
    }
}

// =========================================================================
// ORACLE 6c — LOUD off-band rejection. The Stage-4 `OffCurveBeyondChordBand`
// check (`circle_residual > d_ε`, lib.rs:2316) is, like 6b, a DEFENSIVE guard:
// the UPSTREAM `build_intersection_curves` selection requires
// `curve_contains_point(endpoint, d_ε)` — `|axial| ≤ d_ε ∧ |radial−r| ≤ d_ε`
// (lib.rs:1399) — which is bit-equivalent to `circle_residual ≤ d_ε`. So any
// endpoint with ρ > d_ε fails selection upstream with a loud
// `SsiRefinementFailed { matched: 0 }` BEFORE Stage 4 can apply its own band
// check. (And a uniformly off-band ring also fails `FaceResolutionFailed`
// first, since its lateral-wall centroids exceed the attribution band.)
//
// As in 6b, the oracle's SAFETY intent — an off-band crossing is rejected
// LOUDLY (never snapped, never tolerance-widened) — is preserved by asserting a
// loud `Err` of an honest STOP variant and never `Ok`, plus an independent
// oracle confirming the fixture is genuinely beyond the chord band.
// =========================================================================

fn hand_built_offband_arrangement() -> LabeledArrangement {
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);
    hand_built_tube_arrangement_at_radius(CYL_RADIUS - 2.0 * de)
}

#[test]
fn t6_off_band_residual_rejected_loudly() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);

    // Independent oracle: the fixture's off-curve residual is genuinely > d_ε.
    let arr = hand_built_offband_arrangement();
    let mut max_rho = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if (pt[2].abs() <= TAU_MODEL || (pt[2] - 1.0).abs() <= TAU_MODEL)
            && dist_point_to_line(pt, CYL_AXIS_POINT, unit(CYL_AXIS_DIR)) > MIN_FEATURE_SIZE
        {
            max_rho = max_rho.max(residual_to_cap_circle(pt, cap_z_of(pt)));
        }
    }
    assert!(
        max_rho > de,
        "fixture must start beyond the chord band (max_rho={max_rho} > d_ε={de})"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock);
    // LOUD rejection, never a silent snap (Ok). Accept the Stage-4
    // `OffCurveBeyondChordBand` guard OR the upstream loud gate that fires first.
    match r {
        Err(YangError::Stage4RegionInvalid {
            reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            ..
        })
        | Err(YangError::SsiRefinementFailed { .. })
        | Err(YangError::FaceResolutionFailed { .. }) => {}
        Ok(_) => panic!(
            "yr10 §5.6c: an off-band (ρ > d_ε) crossing must be rejected LOUDLY, \
             never silently snapped — got Ok"
        ),
        other => panic!(
            "yr10 §5.6c: an off-band crossing must fail with a loud P9/P10 STOP \
             (Stage4 OffCurveBeyondChordBand, or the upstream SsiRefinementFailed / \
             FaceResolutionFailed gate that fires first on the same band), got {other:?}"
        ),
    }
}

// =========================================================================
// ORACLE 8 — planar no-op: a planar box union (no Circle edges) is byte-identical
// across two identical runs, every edge stays LineSegment, and the verts are
// UNMOVED from the input arrangement positions. Stage 4 is a strict no-op when
// there are no Circle edges. (The full 900-case fuzz_boxes corpus is
// sidecar-gated in tests/fuzz_boxes.rs; this is the representative no-op guard.)
// =========================================================================

/// Re-declared from yr9: a single closed unit cube with alternating A/B face
/// labels, creating Plane∩Plane A↔B seams (all LineSegment edges).
fn hand_built_planar_box_arrangement() -> LabeledArrangement {
    let c = [
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(1.0, 1.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(0.0, 1.0, 1.0),
    ];
    let verts: Vec<Point3> = c.to_vec();
    let face_tris: [[[u32; 3]; 2]; 6] = [
        [[0, 2, 1], [0, 3, 2]], // bottom, outward −z
        [[4, 5, 6], [4, 6, 7]], // top, outward +z
        [[0, 1, 5], [0, 5, 4]], // front y=0, outward −y
        [[3, 7, 6], [3, 6, 2]], // back y=1, outward +y
        [[0, 4, 7], [0, 7, 3]], // left x=0, outward −x
        [[1, 2, 6], [1, 6, 5]], // right x=1, outward +x
    ];
    let face_label: [u32; 6] = [0, 1, 0, 1, 0, 1];
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for (f, pair) in face_tris.iter().enumerate() {
        for t in pair {
            tris.push(*t);
            surface.push(vec![LaInputId(face_label[f])]);
        }
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

/// Axis-aligned box B-Rep spanning `lo..hi` on every axis — the t7 fixture
/// needs DIFFERENT-extent A/B boxes (see t7's comment), which the unit-size
/// `unit_cube_brep_offset_at` cannot express. Same face order / outward
/// normals as that helper.
fn aa_box_brep(lo: f64, hi: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(lo, lo, lo),
        },
        BRepVertex {
            point: p(hi, lo, lo),
        },
        BRepVertex {
            point: p(hi, hi, lo),
        },
        BRepVertex {
            point: p(lo, hi, lo),
        },
        BRepVertex {
            point: p(lo, lo, hi),
        },
        BRepVertex {
            point: p(hi, lo, hi),
        },
        BRepVertex {
            point: p(hi, hi, hi),
        },
        BRepVertex {
            point: p(lo, hi, hi),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z=lo)
        [4, 7, 6, 5], // F1 top (z=hi)
        [0, 4, 5, 1], // F2 front (y=lo)
        [1, 5, 6, 2], // F3 right (x=hi)
        [2, 6, 7, 3], // F4 back (y=hi)
        [3, 7, 4, 0], // F5 left (x=lo)
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
    let offs = [lo, -hi, lo, -hi, -hi, lo];
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
    BRep::new(verts, edges, faces).expect("aa_box_brep BRep::new failed")
}

#[test]
fn t7_planar_box_union_stage4_noop() {
    // PR-YR24: the original fixture used two COINCIDENT unit cubes, which the
    // near-coplanar input gate now rejects before the (mock) backend runs.
    // The hand-built arrangement labels the unit cube's {z=0, y=0, x=0} faces
    // to A and {z=1, y=1, x=1} to B, so face resolution needs A to carry the
    // lo=0 planes and B the hi=1 planes — WITHOUT A and B sharing any face
    // plane. A = [0, 1.3]³ (planes {0, 1.3}) and B = [−0.3, 1]³ (planes
    // {−0.3, 1}) satisfy both: resolution still finds the unique labeled
    // plane for every tri, and no A-plane coincides with a B-plane.
    let a = aa_box_brep(0.0, 1.3);
    let b = aa_box_brep(-0.3, 1.0);
    let arr = hand_built_planar_box_arrangement();
    // Capture the input arrangement's vertex positions; Stage 4 must NOT move any.
    let input_positions: Vec<[f64; 3]> = arr.mesh.verts.iter().map(|v| v.as_array()).collect();

    let mock = LabelMock {
        arrangement: arr.clone(),
    };
    let r = boolean(&a, &b, BoolOp::Union, &mock).expect("yr10: planar union must Ok");

    // Every output edge stays LineSegment (no spurious Circle).
    for (i, e) in r.edges().iter().enumerate() {
        assert!(
            matches!(e.curve, Curve::LineSegment),
            "yr10 §5.8: planar edge {i} must stay LineSegment (Stage-4 no-op), got {:?}",
            e.curve
        );
    }
    assert_eq!(
        conic_edges(&r).len(),
        0,
        "yr10 §5.8: planar union must produce ZERO conic edges (Stage-4 strict no-op)"
    );

    // Verts unmoved from the input arrangement positions (within TAU_WORK).
    // POSITION-based, NOT index-based: `boolean()`'s kept-submesh compaction
    // renumbers vertices in first-encounter order (unchanged, correct), so the
    // output index order need not match the input. The no-op intent is "Stage 4
    // moved no vertex" → every output position must coincide with SOME input
    // position (no output vertex sits at a position absent from the input set),
    // at the same vertex count.
    let mesh = r.as_mesh();
    assert_eq!(
        mesh.verts.len(),
        input_positions.len(),
        "yr10 §5.8: planar no-op must not add/remove vertices"
    );
    for (i, mv) in mesh.verts.iter().enumerate() {
        let mp = mv.as_array();
        let matched = input_positions
            .iter()
            .any(|ip| norm(sub(mp, *ip)) <= TAU_WORK);
        assert!(
            matched,
            "yr10 §5.8: output vertex {i} at {mp:?} matches NO input arrangement position \
             within TAU_WORK — Stage 4 moved a vertex on the planar path"
        );
    }

    // Watertight (sanity).
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr10 §5.8: planar output watertight"
    );
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "yr10 §5.8: planar output Euler 2"
    );

    // Determinism: a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: hand_built_planar_box_arrangement(),
    };
    let r2 = boolean(&a, &b, BoolOp::Union, &mock2).expect("yr10: planar determinism run 2");
    assert_eq!(
        r, r2,
        "yr10 §5.8: planar union must be byte-identical across identical runs"
    );
}

// =========================================================================
// E2E (env-gated on CHERCHI2022_BIN) — real-sidecar cylinder ∪ box. Mirrors
// yr9 t6. Asserts on-curve + watertight + chord-deviation-drop on the REAL
// mesh-boolean output (whose cap-ring vertices are on chords inside the exact
// circle pre-Stage-4). LOUD eprintln skip when the binary is absent.
// =========================================================================

#[test]
fn t8_e2e_cylinder_union_box_relocated_on_curve() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr10] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cyl = canonical_cylinder();
    let bx = canonical_box();

    let r = boolean(&cyl, &bx, BoolOp::Union, &sb).expect("yr10 E2E: cylinder ∪ box must Ok");

    // Watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr10 E2E: relocated output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr10 E2E: relocated output Euler must be 2"
    );

    // On-curve: every cap-ring intersection-edge vertex is on the exact circle
    // within TAU_MODEL, and the max chord deviation is ≤ TAU_MODEL (Stage 4 did
    // real work on the faceted sidecar mesh).
    let conics = conic_edges(&r);
    assert!(
        !conics.is_empty(),
        "yr10 E2E: output must carry ≥1 Curve::Circle intersection edge (the cap rings)"
    );
    let mut after_max_dev = 0.0_f64;
    for e in &conics {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            continue;
        };
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let rho = circle_residual(ep, center.as_array(), normal.as_array(), radius);
            after_max_dev = after_max_dev.max(rho);
            assert!(
                rho <= TAU_MODEL,
                "yr10 E2E: relocated vertex {ep:?} residual {rho} > TAU_MODEL"
            );
        }
    }
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr10 E2E: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );
}

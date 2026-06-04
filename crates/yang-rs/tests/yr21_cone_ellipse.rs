//! PR-YR21 RED — Stage 4 (OBLIQUE CONE): RELOCATE mesh intersection points onto
//! the exact analytical ELLIPSE of an oblique `cone ∩ plane` cut, via the cone
//! generator parameterization (Yang §4.3.2), with the §4.5.3 reversed-point
//! correction. The cone analog of PR-YR11's oblique-CYLINDER ellipse.
//!
//! Spec of record: `specs/yr21_cone_section_relocation.md` (§4 is the RED
//! contract; §6 is hard scope). Paper: Yang 2025 §4.4.1 (mesh updating /
//! relocation) + §4.3.2 (parametric surface relocation).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY; the
//! GREEN implementer extends `crates/yang-rs/src/lib.rs` (a new
//! `project_onto_cone_section`, the Stage-4 `Curve::Ellipse` arm + a cone
//! relocation loop, the cone reloc record + budget). The RED author NEVER edits
//! production code.
//!
//! ## RED state (behavioral — the API surface all exists)
//!
//! Every public symbol this file references (`Stage4InvalidReason`,
//! `Curve::Ellipse`, `boolean`, `TessellationSource::BRepEdge`, …) already exists
//! in current production, so this file COMPILES against current production and
//! the initial RED state is **assert-fail**, not compile-fail. Current Stage 4's
//! `Curve::Ellipse` arm scans the edge incidence for a `Surface::Cylinder` +
//! `Surface::Plane` (YR11). A cone+plane ellipse edge has `(cyl = None, plane =
//! Some)` → the `let-else` (`crates/yang-rs/src/lib.rs:3611`) fails → returns
//! `Err(Stage4RegionInvalid { reason: LocalRefinementRequired, .. })`. So the
//! oblique cone ∪ box returns that `Err` today and the "must succeed + carry a
//! `Curve::Ellipse` edge + relocate onto the exact ellipse" oracles FAIL. After
//! GREEN wires the cone-ellipse relocation, they pass.
//!
//! Per the established repo convention (integration-test files cannot share
//! helpers), the yr11 / yr17 harness (`p`, array math, `cone_brep`, mesh oracles,
//! `LabelMock`, the off-curve relocate knob, the independent on-conic residual,
//! the exact-ellipse round-trip) is re-declared verbatim here. The on-conic
//! oracle (the exact ellipse the output must match) is recomputed INDEPENDENTLY
//! from the fixture's true cone/plane (cone radial residual + plane residual),
//! NOT via production code.
//!
//! Tolerances (spec §4, do NOT weaken):
//!   - On-conic / round-trip / after-deviation: `cad_primitives::TAU_MODEL` (1e-7).
//!   - Off-band band: the cone chord bound `cone_d_eps()`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{
    boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Stage4InvalidReason, Surface,
    TessellationSource, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math. Re-declared verbatim from yr11/yr17.
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

// =========================================================================
// Mesh oracles. Re-declared verbatim from yr11/yr17.
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

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

// =========================================================================
// Canonical OBLIQUE-CONE config.
//
//   cone A: apex at the origin, axis +Z, half_angle α = atan(0.5) (tanα = 0.5),
//     height 4 → base radius R = 2 at z = 4 (carries a base-rim Curve::Circle,
//     which production needs to derive the cone height / chord budget).
//   plane B (the cutting plane): normal n = (sin30°, 0, cos30°), through
//     (0,0,2). Its inclination to the cone axis is θ = 60° (the normal makes
//     30° with the axis). α ≈ 26.57° < θ = 60° < 90° ⇒ the section is a bounded
//     ELLIPSE on the upper nappe, wholly between apex (z=0) and base (z=4):
//     z-range ≈ [1.55, 2.81]. Independently confirmed `SsiCurve::Ellipse` by the
//     ssi-rs oracle below.
//   ELLIPSE: major_radius ≈ 1.2597, minor_radius ≈ 1.0445 (distinct by a clear
//     margin), center ≈ (-0.3149, 0, 2.1818).
// =========================================================================

const N_FACETS: usize = 16;
const CONE_APEX: [f64; 3] = [0.0, 0.0, 0.0];
const CONE_AXIS: [f64; 3] = [0.0, 0.0, 1.0];
const CONE_HEIGHT: f64 = 4.0;

/// half_angle = atan(0.5) ⇒ tanα = 0.5, base radius R = 4·0.5 = 2.0.
fn cone_half_angle() -> f64 {
    0.5_f64.atan()
}

fn cone_surface() -> Surface {
    Surface::Cone {
        apex: p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]),
        axis_dir: Vector3::new(CONE_AXIS[0], CONE_AXIS[1], CONE_AXIS[2]),
        half_angle: cone_half_angle(),
    }
}

/// The oblique cutting plane: normal at 30° from the cone axis toward +X,
/// through (0,0,2). `n·x + d = 0` with d = −n·(0,0,2).
fn cut_plane_normal() -> [f64; 3] {
    let beta = std::f64::consts::PI / 6.0; // 30°
    unit([beta.sin(), 0.0, beta.cos()])
}
fn cut_plane_d() -> f64 {
    let n = cut_plane_normal();
    -dot(n, [0.0, 0.0, 2.0])
}

fn cut_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(cut_plane_normal()),
        d: cut_plane_d(),
    }
}

/// The cone's Stage-1 chord bound `d_ε = cone_chord_bound(height, half_angle)`
/// = `1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)`. IDENTICAL literal
/// to the production `cone_chord_bound` (the single source — A14.3). `height` is
/// the same value production derives from the cone owner's base-rim `Curve::Circle`
/// (`|(rim_center − apex)·â| = CONE_HEIGHT`).
fn cone_d_eps() -> f64 {
    let r = CONE_HEIGHT * cone_half_angle().tan();
    1e-2 * ((2.0 * r).powi(2) + CONE_HEIGHT.powi(2)).sqrt()
}

// =========================================================================
// cone_brep — the cone B-Rep fixture (one Surface::Cone lateral face + one
// Surface::Plane base cap, sharing a single base-rim seam Circle). Re-declared
// VERBATIM from yr17::cone_brep. The base-rim Curve::Circle is MANDATORY:
// production derives the cone height / chord budget from it.
// =========================================================================

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

fn oblique_cone() -> BRep {
    cone_brep(CONE_APEX, CONE_AXIS, cone_half_angle(), CONE_HEIGHT)
}

// =========================================================================
// Oblique-plane half-space B-Rep (input B). A large tilted box whose ONE
// relevant face carries the oblique `Surface::Plane`; the other five faces are
// meters away from the ellipse cap so the cap centroids resolve uniquely to the
// oblique face (within the planar TAU_WORK band).
//
// Built in the plane frame (û, v̂ in-plane, n̂ out-of-plane). The "top" face lies
// on the cutting plane; the box extends a depth `D` along −n̂. Outward normal of
// the top face = +n̂ (the box interior is on the −n̂ side). All faces planar →
// `reversed: false`.
// =========================================================================

fn plane_frame() -> ([f64; 3], [f64; 3], [f64; 3]) {
    let n = cut_plane_normal();
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

/// A large tilted box whose top face is `plane_surf` (an oblique `Surface::Plane`),
/// centered (in plane) at `plane_center`, half-width `H`, depth `D` along −n̂.
fn oblique_halfspace_box(plane_surf: Surface, plane_center: [f64; 3], h: f64, d: f64) -> BRep {
    let (u, v, n) = plane_frame();
    let corner = |su: f64, sv: f64| add(plane_center, add(scale(u, su * h), scale(v, sv * h)));
    // top face corners (on the plane), CCW seen from +n̂ (outside).
    let t0 = corner(-1.0, -1.0);
    let t1 = corner(1.0, -1.0);
    let t2 = corner(1.0, 1.0);
    let t3 = corner(-1.0, 1.0);
    let b0 = add(t0, scale(n, -d));
    let b1 = add(t1, scale(n, -d));
    let b2 = add(t2, scale(n, -d));
    let b3 = add(t3, scale(n, -d));

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

    // 6 quad faces (vertex indices), wound CCW as seen from outside.
    let face_verts: [[u32; 4]; 6] = [
        [0, 3, 2, 1], // top (+n̂): on the cutting plane
        [4, 5, 6, 7], // bottom (−n̂)
        [0, 1, 5, 4], // side u-
        [1, 2, 6, 5], // side v? (lateral)
        [2, 3, 7, 6], // side u+
        [3, 0, 4, 7], // side v-
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

    // Outward normals + offsets per face. The top face is the oblique plane
    // (normal +n̂); the rest are derived from their own outward normals.
    let mk_plane = |a: [f64; 3], bb: [f64; 3], c: [f64; 3], interior: [f64; 3]| -> Surface {
        let mut nrm = unit(cross(sub(bb, a), sub(c, a)));
        // ensure outward (away from interior point)
        if dot(nrm, sub(interior, a)) > 0.0 {
            nrm = scale(nrm, -1.0);
        }
        Surface::Plane {
            normal: Vector3::from(nrm),
            d: -dot(nrm, a),
        }
    };
    // interior point: box center
    let interior = add(plane_center, scale(n, -0.5 * d));
    let corners = [t0, t1, t2, t3, b0, b1, b2, b3];
    let mut faces: Vec<BRepFace> = Vec::with_capacity(6);
    for (fi, vs) in face_verts.iter().enumerate() {
        let surface = if fi == 0 {
            plane_surf // the oblique cutting plane (exact production fields)
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

    BRep::new(verts, edges, faces).expect("oblique_halfspace_box: BRep::new failed")
}

fn cutting_box() -> BRep {
    oblique_halfspace_box(cut_plane_surface(), [0.0, 0.0, 2.0], 8.0, 8.0)
}

// =========================================================================
// SSI ORACLE — confirm INDEPENDENTLY (via ssi-rs) that the oblique cone ∩ plane
// section really is an ELLIPSE (proves the fixture is genuinely oblique, not
// accidentally a circle / parabola / hyperbola). `surface_to_quadric` re-declared
// verbatim from yr11.
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

/// The EXACT oblique cone ∩ plane section, computed independently by ssi-rs.
/// Asserts it really is exactly one Ellipse.
fn oracle_section_ellipse() -> ssi_rs::SsiCurve {
    let plane = surface_to_quadric(cut_plane_surface());
    let cone = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone)
        .expect("oracle: Plane∩Cone must succeed for an oblique cut");
    assert_eq!(
        curves.len(),
        1,
        "oracle: oblique cone section must be exactly one curve, got {curves:?}"
    );
    assert!(
        matches!(curves[0], ssi_rs::SsiCurve::Ellipse { .. }),
        "oracle: oblique cone ∩ plane must be an Ellipse (α<θ<90°), got {:?}",
        curves[0]
    );
    curves[0]
}

// =========================================================================
// INDEPENDENT on-conic residuals: a relocated crossing must end on BOTH the true
// cone (cone radial residual `|radial − |h_axial|·tanα|`) AND the cutting plane
// (`|n·x + d|`). Recomputed straight from the fixture's cone / plane (NOT via
// production). `cone_radial_residual` re-declared verbatim from yr17.
// =========================================================================

fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let a = CONE_APEX;
    let ax = unit(CONE_AXIS);
    let w = sub(x, a);
    let h_axial = dot(w, ax);
    let radial = norm(sub(w, scale(ax, h_axial)));
    (radial - h_axial.abs() * cone_half_angle().tan()).abs()
}

fn plane_residual(x: [f64; 3]) -> f64 {
    (dot(x, cut_plane_normal()) + cut_plane_d()).abs()
}

/// The on-both-surfaces residual `max(cone radial, plane)` — the spec §4 Oracle-2
/// ellipse residual, recomputed independently of production.
fn conic_residual(x: [f64; 3]) -> f64 {
    cone_radial_residual(x).max(plane_residual(x))
}

// =========================================================================
// Independent ELLIPSE evaluation / parameterization — matches the
// `curve_contains_point` / `ellipse_point` convention EXACTLY:
//   minor_dir = normalize(normal) × normalize(major_axis)
//   point(t)  = C + major_radius·cos t·major_axis + minor_radius·sin t·minor_dir
// Used ONLY by the round-trip oracle to invert `BRepEdge{edge,t}` and by the
// chord-deviation oracle's exact-ellipse reference. Re-declared verbatim from yr11.
// =========================================================================

fn ellipse_minor_dir(normal: [f64; 3], major_axis: [f64; 3]) -> [f64; 3] {
    cross(unit(normal), unit(major_axis))
}

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
/// reference; coarse sampling is fine because we only assert ≫ TAU_MODEL vs ≤
/// TAU_MODEL gaps (orders of magnitude apart), never a tight equality.
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

/// Resolution-INDEPENDENT perpendicular distance from `x` to the exact ellipse,
/// via a two-level sampler. The coarse 200k sweep above has a nearest-sample
/// floor of ~half the sample spacing (≈1.8e-5 for this ≈7.26-perimeter ellipse)
/// — about 180× TAU_MODEL — so it CANNOT certify a tight (≤ TAU_MODEL) on-ellipse
/// equality. This refinement first locates the nearest coarse sample param `t0`,
/// then does a LOCAL fine sweep of `FINE` samples over `[t0 − Δ, t0 + Δ]` (one
/// coarse step on each side). The fine spacing is `2·Δ / FINE ≈ 2π/(200000·50000)`
/// ≈ 6e-10, so its nearest-sample floor (~3e-10) is negligible vs TAU_MODEL.
/// Used ONLY for oracle3's tight after-relocate assertion; the coarse sampler is
/// retained for the orders-of-magnitude strict-decrease comparison.
fn dist_to_ellipse_refined(x: [f64; 3], ell: &ssi_rs::SsiCurve) -> f64 {
    let ssi_rs::SsiCurve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = ell
    else {
        panic!("dist_to_ellipse_refined: not an ellipse");
    };
    let center = center.as_array();
    let normal = normal.as_array();
    let major_axis = major_axis.as_array();

    // Coarse sweep: find nearest sample param t0.
    let coarse = 200_000usize;
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut best = f64::INFINITY;
    let mut t0 = 0.0_f64;
    for k in 0..coarse {
        let t = two_pi * (k as f64) / (coarse as f64);
        let pe = eval_ellipse_point(center, normal, major_axis, *major_radius, *minor_radius, t);
        let d = norm(sub(x, pe));
        if d < best {
            best = d;
            t0 = t;
        }
    }

    // Local fine sweep over one coarse step on each side of t0.
    let delta = two_pi / (coarse as f64);
    let fine = 100_000usize;
    let lo = t0 - delta;
    let span = 2.0 * delta;
    for k in 0..=fine {
        let t = lo + span * (k as f64) / (fine as f64);
        let pe = eval_ellipse_point(center, normal, major_axis, *major_radius, *minor_radius, t);
        best = best.min(norm(sub(x, pe)));
    }
    best
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement.
// Re-declared verbatim from yr11/yr17.
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

// =========================================================================
// OBLIQUE-CONE ELLIPSE FIXTURE BUILDER.
//
// The hand-built arrangement is the closed apex-side piece of the truncated
// oblique cone: an APEX FAN (apex → the ellipse ring, label 0 = the cone input A)
// + the elliptical CAP on the oblique plane (cap center → ellipse ring, label
// 1 = the plane/box input B). The seam where the fan meets the cap is the ELLIPSE
// ring — the intersection edge whose `Curve::Ellipse` Stage-4 relocates. The shell
// is genus 0, χ=2, outward-oriented (positive signed volume).
//
// `cone_ring_at_offset(delta)` samples the ring on the cone lateral at azimuth φ,
// solving the generator parameter `s` so the point stays EXACTLY on the cutting
// plane while sitting at radial distance `s·tanα + delta` from the axis. So:
//   - delta = 0 → ON the exact ellipse (cone ∩ plane).
//   - delta > 0 → OFF the cone by ~delta radially, yet still ON the cutting plane
//     (the controlled chord-band offset). conic_residual ≈ delta.
//
// Union keep-rule (Cherchi: keep iff inside is all-false): every triangle has
// `inside = [false, false]` so ALL are kept and NONE are flipped (`flip_for_op`
// for Union is always false) — the whole arrangement mesh is the output, exactly
// as yr11 does. The mandatory `mock_is_valid_genus0` self-check verifies the
// closed shell directly (no `boolean()` call).
// =========================================================================

/// In-plane azimuth basis perpendicular to the cone axis (+Z): e1=+X, e2=+Y.
fn azim_basis() -> ([f64; 3], [f64; 3]) {
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// One ring vertex on the cone at azimuth φ, radial offset `delta` off the
/// generator, solved to land EXACTLY on the cutting plane. Apex at origin.
fn cone_ring_point(phi: f64, delta: f64) -> [f64; 3] {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let n = cut_plane_normal();
    let d = cut_plane_d();
    let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
    let n_dot_r = dot(n, rhat);
    let n_dot_a = dot(n, ax);
    // n·(s·â + (s·tanα + delta)·r̂) + d = 0
    // ⇒ s·(n·â + tanα·n·r̂) = −d − delta·(n·r̂)
    let s = (-d - delta * n_dot_r) / (n_dot_a + tana * n_dot_r);
    let rho = s * tana + delta;
    add(scale(ax, s), scale(rhat, rho))
}

fn cone_ring(delta: f64) -> Vec<[f64; 3]> {
    (0..N_FACETS)
        .map(|k| {
            let phi = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            cone_ring_point(phi, delta)
        })
        .collect()
}

/// Build the closed apex-fan + elliptical-cap arrangement from a ring (offset
/// `delta`). Apex (label 0) fan + plane cap (label 1). Windings chosen so the
/// shell is outward-oriented (positive signed volume) — verified by the self-check.
fn build_cone_cap_arrangement(delta: f64) -> LabeledArrangement {
    let ring = cone_ring(delta);
    // cap center = mean of the ring (lies on the cutting plane).
    let mut cap_c = [0.0; 3];
    for v in &ring {
        cap_c = add(cap_c, *v);
    }
    cap_c = scale(cap_c, 1.0 / ring.len() as f64);

    let mut verts: Vec<Point3> = Vec::new();
    let apex_id = verts.len() as u32;
    verts.push(p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]));
    let rim_base = verts.len() as u32;
    for v in &ring {
        verts.push(p(v[0], v[1], v[2]));
    }
    let cap_id = verts.len() as u32;
    verts.push(p(cap_c[0], cap_c[1], cap_c[2]));

    let rim = |k: usize| rim_base + (k % N_FACETS) as u32;

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    // APEX FAN (label 0 = cone input A): apex → rim(k+1) → rim(k)
    // (outward-oriented away from the axis; self-check verifies positive volume).
    for k in 0..N_FACETS {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    // ELLIPTICAL CAP (label 1 = plane/box input B): cap_c → rim(k) → rim(k+1)
    // (opposite sense so the shared rim edges pair; outward = +n̂).
    for k in 0..N_FACETS {
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    // Union keep-rule: every triangle inside-count 0 ⇒ all kept, none flipped.
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

/// On-ellipse arrangement (ring on the exact cone ∩ plane).
fn on_ellipse_arrangement() -> LabeledArrangement {
    build_cone_cap_arrangement(0.0)
}

/// The off-curve δ: strictly inside the `(TAU_WORK, cone_d_eps]` relocate band
/// (`δ = 0.4 · cone_d_eps`), so the ring vertices are genuinely off the exact
/// ellipse (`conic_residual ≈ δ ≫ TAU_MODEL`) yet still relocatable.
fn relocate_band_delta() -> f64 {
    0.4 * cone_d_eps()
}

/// Off-curve arrangement: ring at radial `s·tanα − 0.4·cone_d_eps` (still ON the
/// cutting plane), off the exact cone by ~δ pre-Stage-4 (genuine relocation work).
fn off_curve_arrangement() -> LabeledArrangement {
    build_cone_cap_arrangement(-relocate_band_delta())
}

/// Simulate the Union keep-set on the arrangement mesh: every triangle is kept,
/// none flipped (Union). Used by the mandatory `mock_is_valid_genus0` self-check.
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    Mesh::new(arr.mesh.verts.clone(), arr.mesh.tris.clone())
}

// =========================================================================
// Output-edge helpers. Re-declared from yr11.
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

/// Is `pt` an intersection-edge (ring) endpoint? The ring vertices sit off the
/// axis; the apex and cap-center sit on/near the axis and are not relocated.
fn is_ring_point(pt: [f64; 3]) -> bool {
    // distance from the +Z axis through the apex (origin)
    let radial = (pt[0] * pt[0] + pt[1] * pt[1]).sqrt();
    radial > MIN_FEATURE_SIZE
}

fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

/// Analytic outward surface normal at a mesh triangle: the cutting-plane normal
/// if all 3 verts lie on the plane (the cap), else the cone outward normal at the
/// centroid (`n̂ = unit(r̂ − tanα·â)`). Used to check winding.
fn analytic_normal_at_tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<[f64; 3]> {
    let on_plane = plane_residual(a) <= TAU_MODEL
        && plane_residual(b) <= TAU_MODEL
        && plane_residual(c) <= TAU_MODEL;
    if on_plane {
        return Some(cut_plane_normal()); // cap outward = +n̂
    }
    // cone lateral: outward normal at the centroid.
    let centroid = scale(add(add(a, b), c), 1.0 / 3.0);
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let w = sub(centroid, CONE_APEX);
    let h_axial = dot(w, ax);
    let radial = sub(w, scale(ax, h_axial));
    if norm(radial) < MIN_FEATURE_SIZE {
        return None;
    }
    let rhat = unit(radial);
    Some(unit(sub(rhat, scale(ax, tana))))
}

// =========================================================================
// MANDATORY self-check (spec §4 oracle 7) — the authoritative fixture-validity
// gate. Builds the SIMULATED Union output (keep-all, no flip) directly, NO
// boolean() call, and asserts the mock is a valid genus-0 closed shell:
// watertight, χ=2, outward-oriented. This test PASSES today (no boolean() call →
// does not touch the not-yet-wired cone-ellipse Stage-4 path); the relocation /
// Ok oracles FAIL today (RED).
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    for arr in [on_ellipse_arrangement(), off_curve_arrangement()] {
        let sim = simulated_output_mesh(&arr);

        let unpaired = unpaired_half_edges(&sim);
        assert_eq!(
            unpaired, 0,
            "yr21 self-check: simulated cone-cap output mesh must be watertight \
             (0 unpaired half-edges); got {unpaired}. Iterate the mock windings."
        );

        let chi = euler_characteristic(&sim);
        assert_eq!(
            chi, 2,
            "yr21 self-check: simulated cone-cap output must be genus 0 (χ=2); got χ={chi}."
        );

        let vol = signed_volume(&sim);
        assert!(
            vol > 0.0,
            "yr21 self-check: simulated output must be OUTWARD-oriented (positive \
             signed volume); got {vol}. A negative volume means the mock is inside-out."
        );
    }
}

// =========================================================================
// Oracle 1 (spec §4) — the oblique cone ∪ box SUCCEEDS (no
// LocalRefinementRequired STOP) and the output carries ≥1 Curve::Ellipse
// intersection edge; an INDEPENDENT ssi-rs oracle confirms the section is an
// Ellipse; stored major_radius ≥ minor_radius.
//
// Uses the ON-ellipse arrangement (ring on the exact section), so a successful
// Ok requires the Ellipse edge to be emitted and (no-op) relocated.
//
// RED: production's Stage-4 Ellipse arm scans for Cylinder+Plane; a cone+plane
// ellipse hits the `let-else` (lib.rs:3611) → Err(Stage4RegionInvalid{
// LocalRefinementRequired }). So `boolean(...).expect(...)` panics today.
// =========================================================================

#[test]
fn oracle1_oblique_cone_union_succeeds_with_ellipse_edge() {
    // INDEPENDENT ssi-rs oracle: the oblique cone ∩ plane really is an Ellipse.
    let ell = oracle_section_ellipse();
    let ssi_rs::SsiCurve::Ellipse {
        major_radius: o_major,
        minor_radius: o_minor,
        ..
    } = ell
    else {
        unreachable!("oracle asserted Ellipse");
    };
    assert!(
        o_major >= o_minor && (o_major - o_minor).abs() > 1e-3,
        "yr21 O1: fixture must be a CLEARLY oblique ellipse (major {o_major} > minor \
         {o_minor} by a clear margin), not an accidental circle"
    );

    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: on_ellipse_arrangement(),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);

    // Must NOT be the LocalRefinementRequired STOP.
    assert!(
        !matches!(
            r,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::LocalRefinementRequired,
                ..
            })
        ),
        "yr21 O1: oblique cone ∪ box must NO LONGER reject a cone+plane ellipse with \
         Stage4RegionInvalid{{LocalRefinementRequired}}, got {r:?}"
    );

    let brep =
        r.expect("yr21 O1: oblique cone ∪ box must succeed (Ok) after cone-ellipse relocate");

    let ellipses = ellipse_edges(&brep);
    assert!(
        !ellipses.is_empty(),
        "yr21 O1: oblique cone union output must carry ≥1 Curve::Ellipse intersection \
         edge; got {:?}",
        brep.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Each ellipse edge's stored fields must be a VALID ellipse matching the
    // independent ssi-rs section invariants (major ≥ minor, and equal to the
    // ssi-rs section radii within a generous chord-derived tolerance).
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
            major_radius >= minor_radius,
            "yr21 O1: ellipse edge must have major_radius ≥ minor_radius"
        );
        assert!(
            (major_radius - o_major).abs() <= 1e-6,
            "yr21 O1: ellipse edge major_radius {major_radius} must match the ssi-rs \
             section major {o_major}"
        );
        assert!(
            (minor_radius - o_minor).abs() <= 1e-6,
            "yr21 O1: ellipse edge minor_radius {minor_radius} must match the ssi-rs \
             section minor {o_minor}"
        );
    }
}

// =========================================================================
// Oracle 2 + 3 + 4 + 5 — the core off-curve cone-ellipse relocation oracle.
// Drives boolean() with the OFF-curve LabelMock and asserts (spec §4):
//   2. every relocated intersection-edge vertex is on the EXACT ellipse (cone
//      radial residual ≤ TAU_MODEL AND plane residual ≤ TAU_MODEL), recomputed
//      independently; watertight (0 unpaired), Euler χ=2.
//   3. max chord deviation strictly DECREASES (before ≫ TAU_MODEL, after ≤ TAU_MODEL);
//   4. no inverted/degenerate triangles (area ≥ MIN_FEATURE_SIZE², winding agrees
//      with the analytic outward normal where defined);
//   5. relocated verts carry BRepEdge{edge,t}, round-tripping via the exact
//      ellipse parameterization to the relocated position ≤ TAU_MODEL; determinism.
//
// RED: production STOPs on the cone+plane Ellipse edge with
// Stage4RegionInvalid{LocalRefinementRequired}, so `.expect(...)` panics.
// =========================================================================

#[test]
fn oracle2_offcurve_relocate_on_ellipse_watertight() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let delta = relocate_band_delta();
    let de = cone_d_eps();

    // Fixture sanity: TAU_WORK < δ ≤ cone_d_eps (genuinely off-curve, relocatable).
    assert!(
        delta > TAU_WORK && delta <= de,
        "fixture δ={delta} must lie in (TAU_WORK, cone_d_eps={de}]"
    );

    // BEFORE: the off-curve ring vertices are off the exact ellipse by ~δ.
    let arr = off_curve_arrangement();
    let mut before_max_dev = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if is_ring_point(pt) {
            before_max_dev = before_max_dev.max(conic_residual(pt));
        }
    }
    assert!(
        before_max_dev > 100.0 * TAU_MODEL,
        "fixture must start genuinely off the ellipse (before_max_dev={before_max_dev} \
         should be ≫ TAU_MODEL); δ={delta}"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock).expect(
        "yr21 §4.2: oblique cone ∪ box (off-curve mock) must return Ok after cone Stage-4 \
         ellipse relocate (NOT Err(Stage4RegionInvalid{LocalRefinementRequired}))",
    );

    // Watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr21 §4.2: relocated output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr21 §4.2: relocated output Euler V−E+F must be 2"
    );

    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "yr21 §4.2: expected ≥1 Ellipse intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // ORACLE 2: every relocated intersection-edge vertex is on the exact ellipse
    // (BOTH residuals ≤ TAU_MODEL), recomputed independently from the fixture's
    // true cone/plane. ORACLE 3 (after): the max deviation AFTER relocation.
    let mut after_max_dev = 0.0_f64;
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr21 §4.2: relocated vertex {ep:?} cone radial residual {radial} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr21 §4.2: relocated vertex {ep:?} plane residual {planar} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
        }
    }

    // ORACLE 3: chord deviation strictly decreases (and ends ≤ TAU_MODEL).
    assert!(
        after_max_dev < before_max_dev,
        "yr21 §4.3: max chord deviation must strictly decrease (after {after_max_dev} \
         < before {before_max_dev}) — proves real relocation, not a no-op"
    );
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr21 §4.3: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );

    // ORACLE 5: relocated verts carry BRepEdge{edge,t}; inverting via the EXACT
    // ellipse parameterization reproduces the relocated mesh position within
    // TAU_MODEL (the ellipse frame is the production edge's OWN Curve::Ellipse
    // fields — an internal self-consistency round-trip).
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
            match tmap.lookup(vid) {
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
                    let dd = norm(sub(inverted, mesh_pos));
                    assert!(
                        dd <= TAU_MODEL,
                        "yr21 §4.5: relocated vertex {vid} BRepEdge t={t} must invert (via the \
                         exact ellipse parameterization) to the mesh position within TAU_MODEL, off by {dd}"
                    );
                }
                other => panic!(
                    "yr21 §4.5: relocated ellipse intersection-edge vertex {vid} must carry \
                     TessellationSource::BRepEdge{{edge,t}}, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw_relocated_edge_source,
        "yr21 §4.5: at least one relocated vertex must carry a BRepEdge source"
    );

    // Determinism: a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r2 = boolean(&cone, &bx, BoolOp::Union, &mock2).expect("yr21 §4.5: determinism run 2");
    assert_eq!(
        r, r2,
        "yr21 §4.5: identical inputs must produce a byte-identical output BRep"
    );
}

// =========================================================================
// Oracle 3 (standalone, explicit) — chord deviation strictly DECREASES vs the
// exact ellipse, measured by perpendicular distance to the ssi-rs ellipse (a
// production-independent reference). Pre max deviation ≫ TAU_MODEL; post ≤
// TAU_MODEL.
//
// The strict-decrease comparison uses the coarse `dist_to_ellipse_sampled` —
// both operands are orders of magnitude apart and well above its ≈1.8e-5
// nearest-sample floor, so the floor is harmless there. The tight after-relocate
// assertion (≤ TAU_MODEL) uses `dist_to_ellipse_refined` instead: a two-level
// sampler whose nearest-sample floor (~3e-10) is negligible vs TAU_MODEL (1e-7),
// so it can honor the tight bound that the coarse sampler's floor cannot. Both
// measure against the SAME ssi-rs ellipse, keeping this oracle independent of
// oracle2's cone/plane residual method. RED: production STOPs.
// =========================================================================

#[test]
fn oracle3_chord_deviation_strictly_decreases() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let ell = oracle_section_ellipse();

    let arr = off_curve_arrangement();
    let mut before = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if is_ring_point(pt) {
            before = before.max(dist_to_ellipse_sampled(pt, &ell));
        }
    }
    assert!(
        before > 100.0 * TAU_MODEL,
        "yr21 §4.3: pre-Stage-4 max ellipse deviation {before} must be ≫ TAU_MODEL"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr21 §4.3: oblique cone union must Ok after cone Stage-4 ellipse relocate");

    // Coarse measurement for the orders-of-magnitude strict-decrease check.
    let mut after = 0.0_f64;
    // Resolution-independent measurement for the tight ≤ TAU_MODEL check.
    let mut after_refined = 0.0_f64;
    for e in ellipse_edges(&r) {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            after = after.max(dist_to_ellipse_sampled(ep, &ell));
            after_refined = after_refined.max(dist_to_ellipse_refined(ep, &ell));
        }
    }
    assert!(
        after < before,
        "yr21 §4.3: ellipse deviation must strictly decrease (after {after} < before {before})"
    );
    assert!(
        after_refined <= TAU_MODEL,
        "yr21 §4.3: ellipse deviation after relocate must be ≤ TAU_MODEL, got {after_refined}"
    );
}

// =========================================================================
// Oracle 4 (standalone) — no reversed / inverted / degenerate triangles. Every
// output triangle has area ≥ MIN_FEATURE_SIZE² and winding agreeing with the
// analytic outward normal where defined. RED: production STOPs.
// =========================================================================

#[test]
fn oracle4_no_inverted_or_degenerate_tris() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr21 §4.4: oblique cone union must Ok after cone Stage-4 ellipse relocate");
    let mesh = r.as_mesh();

    for (ti, tri) in mesh.tris.iter().enumerate() {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let nrm = tri_normal(a, b, c);
        let area2 = norm(nrm);
        assert!(
            area2 * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
            "yr21 §4.4: triangle {ti} {tri:?} is degenerate (area {} < MIN_FEATURE_SIZE²)",
            area2 * 0.5
        );
        if let Some(an) = analytic_normal_at_tri(a, b, c) {
            let agree = dot(unit(nrm), an);
            assert!(
                agree > 0.0,
                "yr21 §4.4: triangle {ti} {tri:?} winding (normal {:?}) disagrees with the \
                 analytic outward normal {an:?} (dot {agree} ≤ 0) — inverted triangle",
                unit(nrm)
            );
        }
    }
}

// =========================================================================
// Oracle 6 (spec §4) — PR-YR22 MIGRATION: the asymptotic (θ = α) cone section is
// a PARABOLA, the single-candidate conic. As of PR-YR22 it is IN scope: ssi-rs
// returns exactly one `SsiCurve::Parabola` for this pair, which
// `ssi_curve_to_curve` now maps to `Curve::Parabola` and the Stage-4 cone arm
// relocates onto. So the θ=α section now SUCCEEDS with a `Curve::Parabola` edge.
//
// (The asymptotic fixture is still verified, INDEPENDENTLY via ssi-rs, to be a
// Parabola — NOT an Ellipse — so this is genuinely the parabola case; only the
// expected OUTCOME flipped from loud-Err to Ok+Parabola. Hyperbola stays LOUD.)
// =========================================================================

/// A cutting plane PARALLEL to the +X cone generator (n·g = 0 ⇒ θ = α ⇒
/// parabola). n = unit([1, 0, −tanα]) through (0,0,2).
fn parabola_plane_normal() -> [f64; 3] {
    let tana = cone_half_angle().tan();
    unit([1.0, 0.0, -tana])
}
fn parabola_plane_d() -> f64 {
    -dot(parabola_plane_normal(), [0.0, 0.0, 2.0])
}
fn parabola_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(parabola_plane_normal()),
        d: parabola_plane_d(),
    }
}

#[test]
fn oracle6_parabola_section_succeeds() {
    // INDEPENDENT ssi-rs oracle: this pair is a PARABOLA (θ = α), NOT an Ellipse.
    let plane = surface_to_quadric(parabola_plane_surface());
    let cone = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone)
        .expect("oracle: Plane∩Cone (parabola case) must return Ok");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Parabola { .. })),
        "oracle: the asymptotic (θ=α) cone section must be a Parabola, got {curves:?}"
    );
    assert!(
        !curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "oracle: the asymptotic case must NOT be an Ellipse"
    );

    // Build a closed cone-cap mock whose seam ring sits on cone ∩ parabola-plane
    // (sampled where generators actually pierce the plane), so the seam edge is a
    // genuine (cone, plane) intersection edge the SSI is invoked on.
    let arr = build_parabola_cap_arrangement();
    let para_box = oblique_halfspace_box_for(parabola_plane_surface());
    let cone_brep = oblique_cone();
    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone_brep, &para_box, BoolOp::Union, &mock);

    // PR-YR22: the θ=α parabola section now SUCCEEDS. It must NOT be the old
    // out-of-scope STOP (LocalRefinementRequired); it must return Ok and carry a
    // `Curve::Parabola` intersection edge — never an Ellipse / wrong curve.
    assert!(
        !matches!(
            r,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::LocalRefinementRequired,
                ..
            })
        ),
        "yr21 §4.6 (YR22): the θ=α parabola section must NO LONGER STOP with \
         Stage4RegionInvalid{{LocalRefinementRequired}}, got {r:?}"
    );
    let brep = r.expect(
        "yr21 §4.6 (YR22): the θ=α (parabola) cone section must now SUCCEED (Ok) after the \
         cone-parabola Stage-4 relocate",
    );
    let curves_out: Vec<_> = brep.edges().iter().map(|e| e.curve).collect();
    assert!(
        curves_out
            .iter()
            .any(|c| matches!(c, Curve::Parabola { .. })),
        "yr21 §4.6 (YR22): the θ=α section output must carry ≥1 Curve::Parabola edge; got \
         {curves_out:?}"
    );
    assert!(
        !curves_out
            .iter()
            .any(|c| matches!(c, Curve::Ellipse { .. })),
        "yr21 §4.6 (YR22): the θ=α section must NOT emit a (wrong) Ellipse edge; got {curves_out:?}"
    );
}

/// The plane frame for an ARBITRARY oblique `Surface::Plane` (mirrors
/// `plane_frame` but for the given plane's normal).
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

/// A large tilted box whose top face is the given oblique `Surface::Plane`,
/// centered on the plane near (0,0,2). For the parabola loud-STOP fixture.
fn oblique_halfspace_box_for(plane_surf: Surface) -> BRep {
    let Surface::Plane { normal, d } = plane_surf else {
        unreachable!("oblique_halfspace_box_for expects a Plane");
    };
    let n = unit(normal.as_array());
    // a point on the plane near the cone: foot of (0,0,2) onto the plane.
    let off = dot(n, [0.0, 0.0, 2.0]) + d;
    let center = sub([0.0, 0.0, 2.0], scale(n, off));
    oblique_halfspace_box_framed(plane_surf, center, 12.0, 12.0, plane_frame_for(n))
}

/// As `oblique_halfspace_box` but with an explicit plane frame (for arbitrary
/// plane normals).
fn oblique_halfspace_box_framed(
    plane_surf: Surface,
    plane_center: [f64; 3],
    h: f64,
    d: f64,
    frame: ([f64; 3], [f64; 3], [f64; 3]),
) -> BRep {
    let (u, v, n) = frame;
    let corner = |su: f64, sv: f64| add(plane_center, add(scale(u, su * h), scale(v, sv * h)));
    let t0 = corner(-1.0, -1.0);
    let t1 = corner(1.0, -1.0);
    let t2 = corner(1.0, 1.0);
    let t3 = corner(-1.0, 1.0);
    let b0 = add(t0, scale(n, -d));
    let b1 = add(t1, scale(n, -d));
    let b2 = add(t2, scale(n, -d));
    let b3 = add(t3, scale(n, -d));

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
    let interior = add(plane_center, scale(n, -0.5 * d));
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
    BRep::new(verts, edges, faces).expect("oblique_halfspace_box_framed: BRep::new failed")
}

/// Build a closed cone-cap mock whose seam ring is on cone ∩ parabola-plane,
/// sampled only over the azimuth subrange where generators actually pierce the
/// plane (n·g > 0), closed apex-fan + plane cap. Used ONLY by the loud-STOP
/// oracle (its closure validity is not asserted — only the loud STOP is).
fn build_parabola_cap_arrangement() -> LabeledArrangement {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let cosa = cone_half_angle().cos();
    let sina = cone_half_angle().sin();
    let n = parabola_plane_normal();
    let d = parabola_plane_d();
    // Sample only azimuths where the +nappe generator pierces the plane with a
    // bounded positive s (n·g away from 0 and the same sign as the parallel side).
    let mut ring: Vec<[f64; 3]> = Vec::new();
    let n_samp = 24usize;
    for k in 0..n_samp {
        // PR-YR22: restrict to a NARROW 40° arc centered on the φ=180° parabola
        // VERTEX, on the piercing side away from the φ=0 parallel-generator
        // azimuth. The narrow arc keeps the single wrap (apex-fan) triangle's
        // cone-attribution residual (chord sagitta) well inside the cone band
        // (≈0.021 < band 0.057), so Stage-6 attribution now SUCCEEDS and the
        // pipeline reaches the SSI parabola selection the YR22 GREEN change wires.
        // (The pre-YR22 LOUD-STOP contract used a wide 270° arc whose wrap chord
        // failed attribution; for a SUCCESS contract the seam must be attributable.)
        let phi = std::f64::consts::PI * (160.0 / 180.0)
            + std::f64::consts::PI * (40.0 / 180.0) * (k as f64) / ((n_samp - 1) as f64);
        let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
        let g = add(scale(ax, cosa), scale(rhat, sina)); // +nappe generator
        let n_dot_g = dot(n, g);
        if n_dot_g.abs() < 1e-3 {
            continue; // near-parallel: skip (unbounded)
        }
        let s = -(dot(n, CONE_APEX) + d) / n_dot_g;
        if !(s.is_finite() && s > 0.1 && s < 6.0) {
            continue;
        }
        let pt = add(CONE_APEX, scale(g, s));
        ring.push(pt);
    }
    // close with apex fan + a cap centroid on the plane.
    let mut cap_c = [0.0; 3];
    for v in &ring {
        cap_c = add(cap_c, *v);
    }
    cap_c = scale(cap_c, 1.0 / ring.len() as f64);

    let mut verts: Vec<Point3> = Vec::new();
    let apex_id = verts.len() as u32;
    verts.push(p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]));
    let rim_base = verts.len() as u32;
    for v in &ring {
        verts.push(p(v[0], v[1], v[2]));
    }
    let cap_id = verts.len() as u32;
    verts.push(p(cap_c[0], cap_c[1], cap_c[2]));

    let m = ring.len();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    // Treat the sampled arc as a CLOSED ring (wrap k+1 mod m). Every fan triangle
    // (apex → ring[k+1] → ring[k]) sits on the cone; every cap triangle
    // (cap_c → ring[k] → ring[k+1]) sits on the parabola plane — so all triangles
    // attribute cleanly (cone band / plane TAU_WORK) and the (cone, plane) seam
    // edge reaches `build_intersection_curves`, where ssi-rs returns a Parabola →
    // `AmbiguousCurve`. The wrap edge bridges the two arc ends (a long chord); it
    // is geometrically crude but its only role is to make the seam a closed cycle
    // so attribution succeeds — the loud STOP fires in `compute_phase_a` (SSI
    // selection) before any geometric gate scrutinizes the wrap.
    let rim = |k: usize| rim_base + (k % m) as u32;
    for k in 0..m {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }

    let nt = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; nt];
    let patch = vec![0u32; nt];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

// =========================================================================
// Oracle 8 (optional) — env-gated real-sidecar E2E. Mirrors yr11 t5 / yr17
// oracle5b. LOUD eprintln skip when the binary is absent. RED: production STOPs
// on the cone+plane Ellipse edge even on the real mesh.
// =========================================================================

#[test]
fn oracle8_e2e_oblique_cone_union_box_on_ellipse() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!(
            "[yr21] SKIPPED: sidecar binary not found — set CHERCHI2022_BIN to run the cone-ellipse E2E"
        );
        return;
    };
    let cone = oblique_cone();
    let bx = cutting_box();

    let r = boolean(&cone, &bx, BoolOp::Union, &sb)
        .expect("yr21 E2E: oblique cone ∪ box must Ok after cone Stage-4 ellipse relocate");

    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr21 E2E: relocated output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr21 E2E: relocated output Euler must be 2"
    );

    let ellipses = ellipse_edges(&r);
    assert!(
        !ellipses.is_empty(),
        "yr21 E2E: output must carry ≥1 Curve::Ellipse intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    let mut after_max_dev = 0.0_f64;
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr21 E2E: relocated vertex {ep:?} cone radial residual {radial} > TAU_MODEL"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr21 E2E: relocated vertex {ep:?} plane residual {planar} > TAU_MODEL"
            );
        }
    }
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr21 E2E: max ellipse deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );
}

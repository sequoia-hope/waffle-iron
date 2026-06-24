//! PR-YR22 RED — Stage 4 (OBLIQUE CONE): RELOCATE mesh intersection points onto
//! the exact analytical PARABOLA of a `cone ∩ plane` cut whose cutting plane is
//! parallel to EXACTLY ONE cone generator (θ = α), via the cone generator
//! parameterization (Yang §4.3.2). The single-candidate-conic sibling of
//! PR-YR21's cone ELLIPSE: `ssi-rs`'s `plane_cone` returns exactly one
//! `SsiCurve::Parabola` for this θ=α PARA case. Hyperbola is OUT of scope (stays
//! LOUD).
//!
//! Paper: Yang 2025 §4.4.1 (mesh updating / relocation) + §4.3.2 (parametric
//! surface relocation).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY; the
//! GREEN implementer extends `crates/yang-rs/src/lib.rs` (the new `Curve::Parabola`
//! variant, the `ssi_curve_to_curve` Parabola arm, the `curve_contains_point`
//! Parabola arm, the `eval_source` `Curve::Parabola` arm calling a new
//! `parabola_point`, and the Stage-4 relocation `Curve::Parabola` arm). The RED
//! author NEVER edits production code.
//!
//! ## RED state (compile-fail until GREEN)
//!
//! This file references the FINAL API — `Curve::Parabola { vertex, normal,
//! axis_dir, focal_length }` and the helper `parabola_point(vertex, normal,
//! axis_dir, focal_length, t)` — neither of which exists in current production.
//! So this file does NOT compile today; that is the expected RED state. After
//! GREEN adds the variant + helper + the relocation arm, it compiles and the
//! oracles pass.
//!
//! Per the established repo convention (integration-test files cannot share
//! helpers), the yr21 harness (`p`, array math, `cone_brep`, mesh oracles,
//! `LabelMock`, the on-conic residuals, the cone-ring/cap arrangement builder)
//! is re-declared here, retargeted to the θ=α PARABOLA fixture. The parabola
//! section the output must match is recomputed INDEPENDENTLY from the fixture's
//! true cone/plane (cone radial residual + plane residual + the exact `y²=4f·x`
//! in-plane relation), NOT via production code.
//!
//! Tolerances (mirror YR21, do NOT weaken):
//!   - On-conic / round-trip / after-deviation: `cad_primitives::TAU_MODEL` (1e-7).
//!   - Off-band band: the cone chord bound `cone_d_eps()`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, parabola_point, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Stage4InvalidReason,
    Surface, TessellationSource, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math. Re-declared verbatim from yr21.
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
// Mesh oracles. Re-declared verbatim from yr21.
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
// Canonical OBLIQUE-CONE / θ=α PARABOLA config.
//
//   cone A: apex at the origin, axis +Z, half_angle α = atan(0.5) (tanα = 0.5),
//     height 4 → base radius R = 2 at z = 4 (carries a base-rim Curve::Circle,
//     which production needs to derive the cone height / chord budget).
//   plane B (the cutting plane): normal n = unit([1, 0, −tanα]), through
//     (0,0,2.5). This plane is PARALLEL to EXACTLY ONE cone generator (the +X
//     generator g = unit(cosα·â + sinα·x̂): n·g = 0), so θ = α and the section
//     is a PARABOLA — a single unbounded arc on the upper nappe. Independently
//     confirmed `SsiCurve::Parabola` (NOT Ellipse/Hyperbola) by the ssi-rs oracle
//     below. ssi-rs reports vertex ≈ (−0.625, 0, 1.25), focal_length ≈ 0.2795.
//
//   The FIXTURE bounds the unbounded parabola by sampling the azimuth arc
//   φ ∈ [160°, 200°] — a 40° arc centered on the φ=180° parabola VERTEX (so the
//   relocation is genuinely exercised across the vertex), away from the φ=0
//   parallel-generator azimuth, where every generator pierces the plane at a
//   bounded s > 0 (between apex z=0 and base z=4). The sampled arc is closed into
//   a topological ring with a single wrap chord; the arc is deliberately narrow so
//   the lone wrap (apex-fan) triangle's cone-attribution residual (the chord
//   sagitta) stays comfortably INSIDE the cone band `cone_d_eps` even at the
//   off-curve δ offset (≈0.041 < band 0.057) — Stage-6 attribution runs at the
//   pre-relocation positions. The result is a genus-0 closed shell (apex-fan +
//   plane cap), verified by the self-check; the wrap triangle is interior scaffold
//   (NOT a parabola seam vertex — it is never relocated).
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

/// The θ=α cutting plane: normal n = unit([1, 0, −tanα]) (PARALLEL to the +X cone
/// generator), through (0,0,2.5). `n·x + d = 0` with d = −n·(0,0,2.5).
fn cut_plane_normal() -> [f64; 3] {
    let tana = cone_half_angle().tan();
    unit([1.0, 0.0, -tana])
}
fn cut_plane_d() -> f64 {
    let n = cut_plane_normal();
    -dot(n, [0.0, 0.0, 2.5])
}

fn cut_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(cut_plane_normal()),
        d: cut_plane_d(),
    }
}

/// The cone's Stage-1 chord bound `d_ε = cone_chord_bound(height, half_angle)`
/// = `1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)`. IDENTICAL literal
/// to the production `cone_chord_bound` (the single source — A14.3).
fn cone_d_eps() -> f64 {
    let r = CONE_HEIGHT * cone_half_angle().tan();
    1e-2 * ((2.0 * r).powi(2) + CONE_HEIGHT.powi(2)).sqrt()
}

// =========================================================================
// cone_brep — the cone B-Rep fixture (one Surface::Cone lateral face + one
// Surface::Plane base cap, sharing a single base-rim seam Circle). Re-declared
// VERBATIM from yr21::cone_brep. The base-rim Curve::Circle is MANDATORY:
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
// relevant face carries the θ=α `Surface::Plane`; the other five faces are
// meters away from the parabola cap so the cap centroids resolve uniquely to the
// oblique face (within the planar TAU_WORK band). Built in the plane frame.
// Re-declared (retargeted box center to the parabola plane near the cone).
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

/// A large tilted box whose top face is `plane_surf` (the θ=α `Surface::Plane`),
/// centered (in plane) at `plane_center`, half-width `H`, depth `D` along −n̂.
fn oblique_halfspace_box(plane_surf: Surface, plane_center: [f64; 3], h: f64, d: f64) -> BRep {
    let (u, v, n) = plane_frame();
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
        [0, 3, 2, 1], // top (+n̂): on the cutting plane
        [4, 5, 6, 7], // bottom (−n̂)
        [0, 1, 5, 4], // side
        [1, 2, 6, 5], // side
        [2, 3, 7, 6], // side
        [3, 0, 4, 7], // side
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
            plane_surf // the θ=α cutting plane (exact production fields)
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

/// The box top face is the θ=α plane; center it on the foot of the cone apex
/// region onto the plane so the parabola cap centroids resolve to it.
fn cutting_box() -> BRep {
    // foot of (0,0,1.8) (mid-arc-ish point) onto the plane.
    let n = cut_plane_normal();
    let d = cut_plane_d();
    let off = dot(n, [0.0, 0.0, 1.8]) + d;
    let center = sub([0.0, 0.0, 1.8], scale(n, off));
    oblique_halfspace_box(cut_plane_surface(), center, 10.0, 10.0)
}

// =========================================================================
// SSI ORACLE — confirm INDEPENDENTLY (via ssi-rs) that the θ=α cone ∩ plane
// section really is a PARABOLA (proves the fixture is genuinely the asymptotic
// para case, not accidentally an ellipse/hyperbola/circle). `surface_to_quadric`
// re-declared verbatim from yr21.
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

/// The EXACT θ=α cone ∩ plane section, computed independently by ssi-rs. Asserts
/// it really is exactly one Parabola (NOT Ellipse/Hyperbola).
fn oracle_section_parabola() -> ssi_rs::SsiCurve {
    let plane = surface_to_quadric(cut_plane_surface());
    let cone = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone)
        .expect("oracle: Plane∩Cone must succeed for the θ=α (parabola) cut");
    assert_eq!(
        curves.len(),
        1,
        "oracle: θ=α cone section must be exactly one curve, got {curves:?}"
    );
    assert!(
        matches!(curves[0], ssi_rs::SsiCurve::Parabola { .. }),
        "oracle: θ=α cone ∩ plane must be a Parabola (one generator ∥ plane), got {:?}",
        curves[0]
    );
    assert!(
        !matches!(curves[0], ssi_rs::SsiCurve::Ellipse { .. }),
        "oracle: the θ=α case must NOT be an Ellipse"
    );
    curves[0]
}

// =========================================================================
// INDEPENDENT on-conic residuals: a relocated crossing must end on BOTH the true
// cone (cone radial residual `|radial − |h_axial|·tanα|`) AND the cutting plane
// (`|n·x + d|`). Recomputed straight from the fixture's cone / plane (NOT via
// production). `cone_radial_residual` re-declared verbatim from yr21.
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

/// The on-both-surfaces residual `max(cone radial, plane)` — the cone∩plane
/// section residual, recomputed independently of production.
fn conic_residual(x: [f64; 3]) -> f64 {
    cone_radial_residual(x).max(plane_residual(x))
}

// =========================================================================
// INDEPENDENT exact-parabola in-plane test: a relocated crossing, projected into
// the parabola's in-plane (x = along +axis_dir from vertex, y = along the
// conjugate normal×axis_dir), must satisfy the exact relation `y² = 4f·x`. This
// is the production-independent "on the EXACT parabola" reference (distinct from
// the cone/plane residual method). Frame matches `SsiCurve::eval` for Parabola
// (and the GREEN `parabola_point`) field-for-field.
// =========================================================================

/// In-plane (x, y) coords of `pt` in the parabola frame `(vertex, axis_dir,
/// normal×axis_dir)`.
fn parabola_xy(pt: [f64; 3], ell: &ssi_rs::SsiCurve) -> (f64, f64) {
    let ssi_rs::SsiCurve::Parabola {
        vertex,
        normal,
        axis_dir,
        ..
    } = ell
    else {
        panic!("parabola_xy: not a parabola");
    };
    let v = vertex.as_array();
    let ax = unit(axis_dir.as_array());
    let conj = cross(unit(normal.as_array()), ax);
    let w = sub(pt, v);
    (dot(w, ax), dot(w, conj))
}

/// `|y² − 4f·x|` for the exact parabola — zero ⇒ on the exact parabola. Used by
/// oracle2 as the in-plane on-parabola residual.
fn parabola_inplane_residual(pt: [f64; 3], ell: &ssi_rs::SsiCurve) -> f64 {
    let ssi_rs::SsiCurve::Parabola { focal_length, .. } = ell else {
        panic!("parabola_inplane_residual: not a parabola");
    };
    let (x, y) = parabola_xy(pt, ell);
    (y * y - 4.0 * focal_length * x).abs()
}

// =========================================================================
// Independent PARABOLA evaluation / parameterization — matches the
// `SsiCurve::eval` (Parabola) / production `parabola_point` convention EXACTLY:
//   point(t) = vertex + (t²/(4·focal_length))·axis_dir + t·(normal × axis_dir)
// Used by the chord-deviation / round-trip oracles. Re-declared verbatim from the
// ssi-rs Parabola `eval`.
// =========================================================================

fn eval_parabola_point(
    vertex: [f64; 3],
    normal: [f64; 3],
    axis_dir: [f64; 3],
    focal_length: f64,
    t: f64,
) -> [f64; 3] {
    let ax = axis_dir;
    let conj = cross(normal, ax);
    add(
        vertex,
        add(scale(ax, t * t / (4.0 * focal_length)), scale(conj, t)),
    )
}

/// Resolution-INDEPENDENT perpendicular distance from `x` to the exact parabola,
/// via a two-level sampler over the bounded parameter window the fixture arc
/// occupies. A coarse sweep locates the nearest sample param `t0`, then a LOCAL
/// fine sweep over one coarse step on each side refines it. The fine spacing's
/// nearest-sample floor is negligible vs TAU_MODEL, so this can honor the tight
/// (≤ TAU_MODEL) after-relocate bound. Used by oracle3.
fn dist_to_parabola_refined(x: [f64; 3], ell: &ssi_rs::SsiCurve) -> f64 {
    let ssi_rs::SsiCurve::Parabola {
        vertex,
        normal,
        axis_dir,
        focal_length,
    } = ell
    else {
        panic!("dist_to_parabola_refined: not a parabola");
    };
    let vertex = vertex.as_array();
    let normal = normal.as_array();
    let axis_dir = axis_dir.as_array();
    // Parameter window: |t| ≤ TMAX comfortably covers the bounded fixture arc
    // (the arc's conjugate-coord magnitude stays ≪ 2; TMAX = 3 is generous).
    let tmax = 3.0_f64;
    let coarse = 600_000usize;
    let span = 2.0 * tmax;
    let mut best = f64::INFINITY;
    let mut t0 = 0.0_f64;
    for k in 0..=coarse {
        let t = -tmax + span * (k as f64) / (coarse as f64);
        let pe = eval_parabola_point(vertex, normal, axis_dir, *focal_length, t);
        let d = norm(sub(x, pe));
        if d < best {
            best = d;
            t0 = t;
        }
    }
    let step = span / (coarse as f64);
    let fine = 100_000usize;
    let lo = t0 - step;
    let fspan = 2.0 * step;
    for k in 0..=fine {
        let t = lo + fspan * (k as f64) / (fine as f64);
        let pe = eval_parabola_point(vertex, normal, axis_dir, *focal_length, t);
        best = best.min(norm(sub(x, pe)));
    }
    best
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement.
// Re-declared verbatim from yr21.
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
// θ=α PARABOLA FIXTURE BUILDER.
//
// The hand-built arrangement is the closed apex-side piece of the truncated cone
// over the bounded parabola arc: an APEX FAN (apex → the parabola ring, label 0
// = the cone input A) + the parabola CAP on the θ=α plane (cap center → parabola
// ring, label 1 = the plane/box input B). The seam where the fan meets the cap is
// the PARABOLA arc — the intersection edge whose `Curve::Parabola` Stage-4
// relocates. The arc is closed into a topological ring with a single wrap chord;
// the shell is genus 0, χ=2, outward-oriented (positive signed volume).
//
// `parabola_ring_point(phi, delta)` samples the ring on the cone lateral at
// azimuth φ (restricted to the piercing arc φ ∈ [π/2, 3π/2]), solving the
// generator parameter `s` so the point stays EXACTLY on the θ=α plane while
// sitting at radial distance `s·tanα + delta` from the axis. So:
//   - delta = 0 → ON the exact parabola (cone ∩ plane).
//   - delta < 0 → OFF the cone by ~|delta| radially, yet still ON the θ=α plane
//     (the controlled chord-band offset). conic_residual ≈ |delta|.
//
// Union keep-rule (Cherchi: keep iff inside is all-false): every triangle has
// `inside = [false, false]` so ALL are kept and NONE are flipped.
// =========================================================================

/// In-plane azimuth basis perpendicular to the cone axis (+Z): e1=+X, e2=+Y.
fn azim_basis() -> ([f64; 3], [f64; 3]) {
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// One ring vertex on the cone at azimuth φ, radial offset `delta` off the
/// generator, solved to land EXACTLY on the θ=α plane. Apex at origin.
fn parabola_ring_point(phi: f64, delta: f64) -> [f64; 3] {
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

/// The bounded parabola arc, sampled over φ ∈ [160°, 200°] (a narrow arc centered
/// on the φ=180° parabola vertex, on the piercing side away from the φ=0
/// parallel-generator azimuth), at radial offset `delta`. The arc is deliberately
/// narrow so the single wrap (apex-fan) triangle stays within the cone band.
fn parabola_ring(delta: f64) -> Vec<[f64; 3]> {
    let lo = 160.0_f64.to_radians();
    let hi = 200.0_f64.to_radians();
    (0..N_FACETS)
        .map(|k| {
            let phi = lo + (hi - lo) * (k as f64) / ((N_FACETS - 1) as f64);
            parabola_ring_point(phi, delta)
        })
        .collect()
}

/// Build the closed apex-fan + parabola-cap arrangement from a ring (offset
/// `delta`). Apex (label 0) fan + plane cap (label 1). The sampled arc is treated
/// as a CLOSED ring (wrap k+1 mod N_FACETS) so the shell is genus 0; windings
/// chosen so the shell is outward-oriented (positive signed volume) — verified by
/// the self-check.
fn build_parabola_cap_arrangement(delta: f64) -> LabeledArrangement {
    let ring = parabola_ring(delta);
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
    // APEX FAN (label 0 = cone input A): apex → rim(k+1) → rim(k).
    for k in 0..N_FACETS {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    // PARABOLA CAP (label 1 = plane/box input B): cap_c → rim(k) → rim(k+1)
    // (opposite sense so the shared rim edges pair; outward = +n̂).
    for k in 0..N_FACETS {
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
        num_inputs: 2,
    }
}

/// On-parabola arrangement (ring on the exact cone ∩ θ=α plane).
fn on_parabola_arrangement() -> LabeledArrangement {
    build_parabola_cap_arrangement(0.0)
}

/// The off-curve δ: strictly inside the `(TAU_WORK, cone_d_eps]` relocate band
/// (`δ = 0.4 · cone_d_eps`), so the ring vertices are genuinely off the exact
/// parabola (`conic_residual ≈ δ ≫ TAU_MODEL`) yet still relocatable.
fn relocate_band_delta() -> f64 {
    0.4 * cone_d_eps()
}

/// Off-curve arrangement: ring at radial `s·tanα − 0.4·cone_d_eps` (still ON the
/// θ=α plane), off the exact cone by ~δ pre-Stage-4 (genuine relocation work).
fn off_curve_arrangement() -> LabeledArrangement {
    build_parabola_cap_arrangement(-relocate_band_delta())
}

/// Simulate the Union keep-set on the arrangement mesh: every triangle kept, none
/// flipped (Union). Used by the mandatory `mock_is_valid_genus0` self-check.
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    Mesh::new(arr.mesh.verts.clone(), arr.mesh.tris.clone())
}

// =========================================================================
// Output-edge helpers. Re-declared from yr21 (Parabola-targeted).
// =========================================================================

fn edge_endpoints(brep: &BRep, e: &BRepEdge) -> ([f64; 3], [f64; 3]) {
    let vs = brep.vertices();
    (
        vs[e.start as usize].point.as_array(),
        vs[e.end as usize].point.as_array(),
    )
}

fn parabola_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Parabola { .. }))
        .collect()
}

/// Is `pt` an intersection-edge (ring) endpoint? The ring vertices sit off the
/// axis; the apex and cap-center sit on/near the axis and are not relocated.
fn is_ring_point(pt: [f64; 3]) -> bool {
    let radial = (pt[0] * pt[0] + pt[1] * pt[1]).sqrt();
    radial > MIN_FEATURE_SIZE
}

fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED Union output (keep-all, no flip) directly, NO boolean() call, and
// asserts the mock is a valid genus-0 closed shell: watertight, χ=2,
// outward-oriented. (Compiles + passes once GREEN adds the Parabola variant; it
// never reaches the not-yet-wired Stage-4 path.)
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    for arr in [on_parabola_arrangement(), off_curve_arrangement()] {
        let sim = simulated_output_mesh(&arr);

        let unpaired = unpaired_half_edges(&sim);
        assert_eq!(
            unpaired, 0,
            "yr22 self-check: simulated parabola-cap output mesh must be watertight \
             (0 unpaired half-edges); got {unpaired}."
        );

        let chi = euler_characteristic(&sim);
        assert_eq!(
            chi, 2,
            "yr22 self-check: simulated parabola-cap output must be genus 0 (χ=2); got χ={chi}."
        );

        let vol = signed_volume(&sim);
        assert!(
            vol > 0.0,
            "yr22 self-check: simulated output must be OUTWARD-oriented (positive \
             signed volume); got {vol}."
        );
    }
}

// =========================================================================
// Oracle 1 — the θ=α cone ∪ box SUCCEEDS and the output carries ≥1
// Curve::Parabola intersection edge; an INDEPENDENT ssi-rs oracle confirms the
// section is a Parabola (NOT Ellipse/Hyperbola); the stored focal_length /
// axis_dir match the ssi-rs section.
//
// Uses the ON-parabola arrangement (ring on the exact section), so a successful
// Ok requires the Parabola edge to be emitted and (no-op) relocated.
// =========================================================================

#[test]
fn oracle1_theta_eq_alpha_cone_union_succeeds_with_parabola_edge() {
    // INDEPENDENT ssi-rs oracle: the θ=α cone ∩ plane really is a Parabola.
    let para = oracle_section_parabola();
    let ssi_rs::SsiCurve::Parabola {
        axis_dir: o_axis,
        focal_length: o_focal,
        ..
    } = para
    else {
        unreachable!("oracle asserted Parabola");
    };
    assert!(
        o_focal > 1e-3,
        "yr22 O1: fixture must be a genuine parabola (focal_length {o_focal} > 0)"
    );

    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: on_parabola_arrangement(),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);

    // Must NOT be the LocalRefinementRequired STOP nor an AmbiguousCurve reject.
    assert!(
        !matches!(
            r,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::LocalRefinementRequired,
                ..
            })
        ),
        "yr22 O1: θ=α cone ∪ box must NO LONGER reject a cone+plane parabola with \
         Stage4RegionInvalid{{LocalRefinementRequired}}, got {r:?}"
    );

    let brep = r.expect("yr22 O1: θ=α cone ∪ box must succeed (Ok) after cone-parabola relocate");

    let parabolas = parabola_edges(&brep);
    assert!(
        !parabolas.is_empty(),
        "yr22 O1: θ=α cone union output must carry ≥1 Curve::Parabola intersection \
         edge; got {:?}",
        brep.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Each parabola edge's stored fields must match the independent ssi-rs section
    // within a generous (chord-derived) tolerance: GREEN stores the ssi-rs section
    // verbatim via `ssi_curve_to_curve`, so this is effectively byte-equality.
    for e in &parabolas {
        let Curve::Parabola {
            axis_dir,
            focal_length,
            ..
        } = e.curve
        else {
            continue;
        };
        assert!(
            (focal_length - o_focal).abs() <= 1e-6,
            "yr22 O1: parabola edge focal_length {focal_length} must match the ssi-rs \
             section focal_length {o_focal}"
        );
        let a = unit(axis_dir.as_array());
        let oa = unit(o_axis.as_array());
        let align = dot(a, oa);
        assert!(
            (align - 1.0).abs() <= 1e-6,
            "yr22 O1: parabola edge axis_dir {a:?} must match the ssi-rs section \
             axis_dir {oa:?} (dot {align} ≈ 1)"
        );
    }
}

// =========================================================================
// Oracle 2 (core) — the off-curve cone-parabola relocation oracle. Drives
// boolean() with the OFF-curve LabelMock and asserts:
//   - result is Ok, watertight (0 unpaired), Euler χ=2;
//   - every relocated intersection-edge vertex lies on the EXACT parabola
//     (`y²=4f·x` via in-plane x,y) AND on the cone AND on the plane to TAU_MODEL,
//     recomputed independently;
//   - max chord deviation strictly DECREASES across relocation and ends ≤ TAU_MODEL;
//   - determinism (a second boolean() run is byte-identical).
// =========================================================================

#[test]
fn oracle2_offcurve_relocate_on_parabola_watertight() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let delta = relocate_band_delta();
    let de = cone_d_eps();
    let para = oracle_section_parabola();

    // Fixture sanity: TAU_WORK < δ ≤ cone_d_eps (genuinely off-curve, relocatable).
    assert!(
        delta > TAU_WORK && delta <= de,
        "fixture δ={delta} must lie in (TAU_WORK, cone_d_eps={de}]"
    );

    // BEFORE: the off-curve ring vertices are off the exact parabola by ~δ.
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
        "fixture must start genuinely off the parabola (before_max_dev={before_max_dev} \
         should be ≫ TAU_MODEL); δ={delta}"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock).expect(
        "yr22: θ=α cone ∪ box (off-curve mock) must return Ok after cone Stage-4 parabola \
         relocate (NOT Err)",
    );

    // Watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr22: relocated output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr22: relocated output Euler V−E+F must be 2"
    );

    let parabolas = parabola_edges(&r);
    assert!(
        !parabolas.is_empty(),
        "yr22: expected ≥1 Parabola intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Every relocated intersection-edge vertex is on the exact parabola: cone
    // radial residual ≤ TAU_MODEL AND plane residual ≤ TAU_MODEL AND the in-plane
    // `y²=4f·x` relation ≤ TAU_MODEL. Track the max post-relocation deviation.
    let mut after_max_dev = 0.0_f64;
    for e in &parabolas {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr22: relocated vertex {ep:?} cone radial residual {radial} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr22: relocated vertex {ep:?} plane residual {planar} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            let inplane = parabola_inplane_residual(ep, &para);
            assert!(
                inplane <= TAU_MODEL,
                "yr22: relocated vertex {ep:?} exact-parabola residual |y²−4f·x| \
                 {inplane} must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
        }
    }

    // Chord deviation strictly decreases (and ends ≤ TAU_MODEL).
    assert!(
        after_max_dev < before_max_dev,
        "yr22: max chord deviation must strictly decrease (after {after_max_dev} \
         < before {before_max_dev}) — proves real relocation, not a no-op"
    );
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr22: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );

    // Determinism: a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r2 = boolean(&cone, &bx, BoolOp::Union, &mock2).expect("yr22: determinism run 2");
    assert_eq!(
        r, r2,
        "yr22: identical inputs must produce a byte-identical output BRep"
    );
}

// =========================================================================
// Oracle 3 (eval round-trip) — each relocated vertex carries
// TessellationSource::BRepEdge{edge,t}; `parabola_point(.., t)` reproduces the
// mesh position within TAU_MODEL. Also asserts (resolution-independently, via the
// two-level `dist_to_parabola_refined`) the relocated vertices lie on the exact
// ssi-rs parabola ≤ TAU_MODEL, while the pre-relocation ring sits ≫ TAU_MODEL off
// it (strict decrease, production-independent reference).
//
// The round-trip is the production edge's OWN Curve::Parabola fields fed to the
// production `parabola_point` (the SAME helper `eval_source`'s Curve::Parabola arm
// calls) — an internal self-consistency check mirroring YR21 oracle3.
// =========================================================================

#[test]
fn oracle3_parabola_eval_round_trip() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let ell = oracle_section_parabola();

    // Pre-relocation: the off-curve ring sits ≫ TAU_MODEL off the exact parabola.
    let arr = off_curve_arrangement();
    let mut before = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if is_ring_point(pt) {
            before = before.max(dist_to_parabola_refined(pt, &ell));
        }
    }
    assert!(
        before > 100.0 * TAU_MODEL,
        "yr22 O3: pre-Stage-4 max parabola deviation {before} must be ≫ TAU_MODEL"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr22 O3: θ=α cone union must Ok after cone Stage-4 parabola relocate");

    let tmap = r.tessellation_map();
    let mesh = r.as_mesh();
    let mut saw_relocated_edge_source = false;
    let mut after_refined = 0.0_f64;
    for e in parabola_edges(&r) {
        let Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } = e.curve
        else {
            continue;
        };
        for vid in [e.start, e.end] {
            // Resolution-independent on-parabola check against the ssi-rs section.
            let mesh_pos = mesh.verts[vid as usize].as_array();
            after_refined = after_refined.max(dist_to_parabola_refined(mesh_pos, &ell));

            match tmap.lookup(vid) {
                TessellationSource::BRepEdge { edge: _, t } => {
                    saw_relocated_edge_source = true;
                    // Round-trip through the PRODUCTION `parabola_point` (the same
                    // helper `eval_source`'s Curve::Parabola arm uses), with the
                    // edge's own stored fields.
                    let inverted =
                        parabola_point(vertex, normal, axis_dir, focal_length, t).as_array();
                    let dd = norm(sub(inverted, mesh_pos));
                    assert!(
                        dd <= TAU_MODEL,
                        "yr22 O3: relocated vertex {vid} BRepEdge t={t} must invert (via the \
                         exact parabola parameterization) to the mesh position within TAU_MODEL, \
                         off by {dd}"
                    );
                }
                other => panic!(
                    "yr22 O3: relocated parabola intersection-edge vertex {vid} must carry \
                     TessellationSource::BRepEdge{{edge,t}}, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw_relocated_edge_source,
        "yr22 O3: at least one relocated vertex must carry a BRepEdge source"
    );
    assert!(
        after_refined < before,
        "yr22 O3: parabola deviation must strictly decrease (after {after_refined} < before {before})"
    );
    assert!(
        after_refined <= TAU_MODEL,
        "yr22 O3: parabola deviation after relocate must be ≤ TAU_MODEL, got {after_refined}"
    );
}

// =========================================================================
// Oracle 4 — no degenerate triangles, and the boolean output is a consistently-
// oriented watertight 2-manifold. PR-YR22 (driver, post second-opinion review):
// this asserts the invariant production ACTUALLY enforces — non-degeneracy +
// inherited watertightness/local repair (Yang §4.4.1/§4.4.3, see lib.rs
// `validate_relocated_triangles`) — NOT a per-facet winding-vs-analytic-normal
// test. Production deliberately rejects that pointwise test because it
// false-positives on the fixture's ring-closure scaffold facets (the wrap
// "bridge" triangles that close the open 40° arc into a watertight ring).
// =========================================================================

#[test]
fn oracle4_no_inverted_or_degenerate_tris() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr22 O4: θ=α cone union must Ok after cone Stage-4 parabola relocate");
    let mesh = r.as_mesh();

    // (a) No DEGENERATE triangle (per-facet area floor).
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let area2 = norm(tri_normal(a, b, c));
        assert!(
            area2 * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
            "yr22 O4: triangle {ti} {tri:?} is degenerate (area {} < MIN_FEATURE_SIZE²)",
            area2 * 0.5
        );
    }

    // (b) No INVERTED orientation — the REAL post-relocation boolean output must
    // be a consistently-oriented watertight 2-manifold. A relocation that folds
    // or tears a facet breaks half-edge pairing (unpaired != 0); a global
    // inversion flips the signed volume. O2/O3 independently pin WHERE relocated
    // vertices land (on the exact cone+plane+parabola to TAU_MODEL). This is the
    // ONLY always-on check of signed-volume>0 on the real post-relocation output
    // (mock_is_valid_genus0 is no-pipeline; O2 omits volume; O8 is sidecar-gated).
    let unpaired = unpaired_half_edges(mesh);
    assert_eq!(
        unpaired, 0,
        "yr22 O4: boolean output must be watertight (0 unpaired half-edges); got {unpaired}"
    );
    let chi = euler_characteristic(mesh);
    assert_eq!(
        chi, 2,
        "yr22 O4: boolean output must be genus 0 (χ=2); got χ={chi}"
    );
    let vol = signed_volume(mesh);
    assert!(
        vol > 0.0,
        "yr22 O4: boolean output must be OUTWARD-oriented (signed volume > 0); got {vol}"
    );
}

// =========================================================================
// Oracle 8 (optional) — env-gated real-sidecar E2E. Mirrors yr21 oracle8. LOUD
// eprintln skip when the binary is absent.
// =========================================================================

#[test]
fn oracle8_e2e_theta_eq_alpha_cone_union_box_on_parabola() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr22] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cone = oblique_cone();
    let bx = cutting_box();

    let r = boolean(&cone, &bx, BoolOp::Union, &sb)
        .expect("yr22 E2E: θ=α cone ∪ box must Ok after cone Stage-4 parabola relocate");

    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr22 E2E: relocated output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr22 E2E: relocated output Euler must be 2"
    );

    let parabolas = parabola_edges(&r);
    assert!(
        !parabolas.is_empty(),
        "yr22 E2E: output must carry ≥1 Curve::Parabola intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    let mut after_max_dev = 0.0_f64;
    for e in &parabolas {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr22 E2E: relocated vertex {ep:?} cone radial residual {radial} > TAU_MODEL"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr22 E2E: relocated vertex {ep:?} plane residual {planar} > TAU_MODEL"
            );
        }
    }
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr22 E2E: max parabola deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );
}

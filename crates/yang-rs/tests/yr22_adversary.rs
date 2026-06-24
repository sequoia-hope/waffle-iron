//! PR-YR22 ADVERSARY — independent adversarial check on the cone∩plane PARABOLA
//! Stage-4 relocation (`Curve::Parabola` variant + `parabola_point` eval +
//! `ssi_curve_to_curve` / `curve_contains_point` Parabola arms + the Stage-4
//! `ConeParabolaReloc` relocation arm + `is_reversed` parabola tangent) landed in
//! `crates/yang-rs/src/lib.rs` by the GREEN subagent.
//!
//! This file is the ADVERSARY half of a role-separated FIP cycle: it writes TESTS
//! ONLY and never edits production, the RED file (`yr22_cone_parabola.rs`), the
//! sibling RED files (`yr21_*`), or any other file. Per the repo convention that
//! integration-test files cannot share helpers, the harness (`p`, array math,
//! `cone_brep`, the box builders, the mesh oracles, `LabelMock`, the independent
//! on-conic residuals, the cone-ring/cap arrangement builder) is re-declared here
//! — independently authored, NOT a verbatim copy of the RED file (distinct δ
//! choices, an independent off-parabola perturbation, an independently authored
//! `y²=4f·x` membership oracle, distinct ellipse/cylinder/circle regression
//! fixtures, and an orientation-fold mock for the oracle4-reframe probe).
//!
//! Adversarial contract (the five properties this file guards):
//!   1. **No over-acceptance / SILENT_WRONG=0** — a point genuinely OFF the
//!      parabola (beyond the cone chord band, on the plane) is NOT silently kept
//!      as an on-parabola edge endpoint; an independently authored `y²=4f·x`
//!      membership oracle confirms it is genuinely off; a successful `Ok` has
//!      EVERY relocated parabola-edge vertex on BOTH the cone and the plane (and
//!      on `y²=4f·x`) to TAU_MODEL.
//!   2. **eval round-trip** — `parabola_point(.., t)` at the relocation-tagged `t`
//!      reproduces the relocated 3D position to TAU_MODEL, cross-checked against an
//!      INDEPENDENTLY re-implemented parameterization.
//!   3. **No regression** — cone-ellipse (YR21), cylinder-ellipse (YR11), and
//!      circle (YR17) relocation paths still produce on-curve `Ok` results; the
//!      parabola edit did not perturb them.
//!   4. **Out-of-scope stays LOUD** — a HYPERBOLA cone section and an
//!      AXIS-PARALLEL/degenerate section return a classified `Err` (NOT `Ok`, NOT
//!      a panic).
//!   5. **oracle4-reframe soundness probe** — an explicit local orientation fold
//!      (a single flipped facet) that PRESERVES watertight half-edge pairing and
//!      χ is shown to either (a) be caught by the connectivity/volume invariant,
//!      or (b) fall to the on-curve residual oracles O2/O3 — quantifying the
//!      residual risk the reframe leaves.
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
    boolean, parabola_point, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    TessellationSource,
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

fn parabola_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Parabola { .. }))
        .collect()
}
fn ellipse_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Ellipse { .. }))
        .collect()
}
fn circle_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Circle { .. }))
        .collect()
}

// =========================================================================
// θ=α PARABOLA cone fixture. I reuse the RED file's cone geometry (apex at
// origin, axis +Z, tanα=0.5, height 4) because that is the geometry empirically
// proven to yield a single clean parabola arc; an adversary inventing a brand-new
// cone risks landing on an accidentally-ellipse/hyperbola fixture. My
// INDEPENDENCE is in the perturbations / δ choices / oracles below, NOT the cone
// numbers — and the ssi-rs oracle below independently re-confirms the section is
// a Parabola each run, so I never trust the geometry on faith.
// =========================================================================

const N_FACETS: usize = 16;
const CONE_APEX: [f64; 3] = [0.0, 0.0, 0.0];
const CONE_AXIS: [f64; 3] = [0.0, 0.0, 1.0];
const CONE_HEIGHT: f64 = 4.0;

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

/// θ=α cutting plane: n = unit([1,0,−tanα]) (∥ the +X cone generator), through
/// (0,0,2.5).
fn cut_plane_normal() -> [f64; 3] {
    let tana = cone_half_angle().tan();
    unit([1.0, 0.0, -tana])
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
/// `1e-2·√((2R)²+h²)`, R = height·tanα. IDENTICAL literal to production (A14.3).
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

/// A large tilted box whose top face is `plane_surf`, centered near (0,0,1.8)
/// (mid-arc), half-width 10, depth 10 along −n̂. Re-declared (matches the RED
/// fixture's siting so the parabola cap centroids resolve to the oblique face).
fn oblique_halfspace_box_for(plane_surf: Surface) -> BRep {
    let Surface::Plane { normal, d } = plane_surf else {
        unreachable!("oblique_halfspace_box_for expects a Plane");
    };
    let n = unit(normal.as_array());
    let off = dot(n, [0.0, 0.0, 1.8]) + d;
    let center = sub([0.0, 0.0, 1.8], scale(n, off));
    let (u, v, nn) = plane_frame_for(n);
    let (h, depth) = (10.0, 10.0);
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

// =========================================================================
// SSI ORACLE — independently confirm the θ=α cone∩plane section is a Parabola.
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

fn oracle_section_parabola() -> ssi_rs::SsiCurve {
    let plane = surface_to_quadric(cut_plane_surface());
    let cone = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone)
        .expect("oracle: Plane∩Cone must succeed for the θ=α (parabola) cut");
    assert_eq!(curves.len(), 1, "oracle: θ=α section must be one curve");
    assert!(
        matches!(curves[0], ssi_rs::SsiCurve::Parabola { .. }),
        "oracle: θ=α cone∩plane must be a Parabola, got {:?}",
        curves[0]
    );
    curves[0]
}

// =========================================================================
// INDEPENDENT on-conic residuals.
// =========================================================================

fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let a = CONE_APEX;
    let ax = unit(CONE_AXIS);
    let w = sub(x, a);
    let h_axial = dot(w, ax);
    let radial = norm(sub(w, scale(ax, h_axial)));
    (radial - h_axial.abs() * cone_half_angle().tan()).abs()
}
fn plane_residual_to(x: [f64; 3], n: [f64; 3], d: f64) -> f64 {
    (dot(x, n) + d).abs()
}
fn cut_plane_residual(x: [f64; 3]) -> f64 {
    plane_residual_to(x, cut_plane_normal(), cut_plane_d())
}

// =========================================================================
// INDEPENDENTLY-AUTHORED `y²=4f·x` membership oracle (Property 1). This is the
// adversary's OWN re-implementation of the in-plane parabola membership relation
// (distinct from the RED file's `parabola_inplane_residual` even though it must
// agree mathematically — both encode the same `y²=4f·x`). It converts the
// implicit residual to a geometric (length) residual via the in-plane gradient
// `|∇(y²−4f·x)| = 2√(4f²+y²)`, mirroring production's `curve_contains_point`
// scaling so I test the SAME membership semantics production uses.
// =========================================================================

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

/// Geometric (length) on-parabola residual `|y²−4f·x| / (2√(4f²+y²))`. Zero ⇒ on
/// the exact parabola. This mirrors production `curve_contains_point`'s parabola
/// arm scaling so a "membership" verdict here matches production's membership.
fn parabola_geo_residual(pt: [f64; 3], ell: &ssi_rs::SsiCurve) -> f64 {
    let ssi_rs::SsiCurve::Parabola { focal_length, .. } = ell else {
        panic!("parabola_geo_residual: not a parabola");
    };
    let (x, y) = parabola_xy(pt, ell);
    let implicit = (y * y - 4.0 * focal_length * x).abs();
    let grad = 2.0 * (4.0 * focal_length * focal_length + y * y).sqrt();
    if grad > MIN_FEATURE_SIZE {
        implicit / grad
    } else {
        implicit
    }
}

/// INDEPENDENT parabola evaluation matching `SsiCurve::eval` (Parabola) /
/// production `parabola_point` field-for-field:
///   point(t) = vertex + (t²/(4f))·axis_dir + t·(normal × axis_dir)
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

// =========================================================================
// θ=α PARABOLA cap arrangement builder. Re-declared (matches the RED fixture's
// narrow φ∈[160°,200°] piercing arc so attribution succeeds), parameterized by an
// arbitrary per-vertex radial offset so I can author my OWN off-curve / beyond-band
// perturbations distinct from the RED δ.
// =========================================================================

fn azim_basis() -> ([f64; 3], [f64; 3]) {
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// One ring vertex on the cone at azimuth φ, radial offset `delta` off the
/// generator, solved to land EXACTLY on the θ=α plane (plane residual ≈ 0, cone
/// radial residual ≈ |delta|).
fn parabola_ring_point(phi: f64, delta: f64) -> [f64; 3] {
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

/// The bounded parabola arc over φ∈[160°,200°] at radial offset `delta`.
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

/// Build the closed apex-fan + parabola-cap arrangement from a ring of points.
/// Apex (label 0 = cone A) fan + plane cap (label 1 = box B). Union keep-all.
fn build_parabola_cap_arrangement(ring: &[[f64; 3]]) -> LabeledArrangement {
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
        num_inputs: 2,
    }
}

/// The adversary's off-curve δ: distinct from the RED's 0.4·de — I use 0.6·de
/// (still strictly inside the relocate band), so the "before" residual differs.
fn adv_band_delta() -> f64 {
    0.6 * cone_d_eps()
}

// =========================================================================
// PROPERTY 1 — NO OVER-ACCEPTANCE / SILENT_WRONG = 0.
//
// (1a) An independently-authored `y²=4f·x` membership oracle: a point genuinely
//      OFF the parabola (pushed beyond the cone chord band, on the plane) has a
//      LARGE geometric membership residual (≫ TAU_MODEL) — i.e. it is genuinely
//      off, so a SILENT acceptance would be a real defect.
// (1b) When `boolean()` returns Ok on an off-curve (within-band) fixture, EVERY
//      relocated parabola-edge endpoint is on BOTH the cone and the plane AND on
//      `y²=4f·x` to TAU_MODEL — no vertex is silently kept off-curve.
// =========================================================================

#[test]
fn adversary_offcurve_acceptance_is_loud_not_silent() {
    let para = oracle_section_parabola();
    let de = cone_d_eps();

    // (1a) A point pushed BEYOND the cone band (radially off the cone by 2·de)
    // while staying on the plane is genuinely off the parabola: its independent
    // geometric `y²=4f·x` residual must be far above TAU_MODEL.
    let beyond = parabola_ring_point(180.0_f64.to_radians(), -2.0 * de);
    assert!(
        cut_plane_residual(beyond) <= TAU_MODEL,
        "fixture: the beyond-band probe must stay ON the cutting plane (plane residual {})",
        cut_plane_residual(beyond)
    );
    assert!(
        cone_radial_residual(beyond) > de,
        "fixture: the beyond-band probe must be off the cone by > cone_d_eps; got {}",
        cone_radial_residual(beyond)
    );
    let geo = parabola_geo_residual(beyond, &para);
    assert!(
        geo > 100.0 * TAU_MODEL,
        "adversary P1a: a genuinely off-parabola point must have a LARGE membership \
         residual (geo {geo} ≫ TAU_MODEL) — otherwise the membership test is vacuous"
    );

    // (1b) Drive boolean() with an off-curve (within-band) ring; if it succeeds,
    // every relocated parabola-edge vertex must be on the EXACT parabola.
    let delta = adv_band_delta();
    assert!(delta > TAU_WORK && delta <= de);
    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: build_parabola_cap_arrangement(&parabola_ring(-delta)),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    match r {
        Err(_) => { /* a loud rejection is acceptable — never a wrong Ok */ }
        Ok(brep) => {
            let parabolas = parabola_edges(&brep);
            assert!(
                !parabolas.is_empty(),
                "adversary P1b: an Ok output over a parabola seam must carry ≥1 Parabola edge"
            );
            for e in &parabolas {
                let (s, t) = edge_endpoints(&brep, e);
                for ep in [s, t] {
                    let radial = cone_radial_residual(ep);
                    let planar = cut_plane_residual(ep);
                    let geo = parabola_geo_residual(ep, &para);
                    assert!(
                        radial <= TAU_MODEL,
                        "adversary P1b: relocated parabola vertex {ep:?} cone radial residual \
                         {radial} > TAU_MODEL — SILENTLY accepted off-cone"
                    );
                    assert!(
                        planar <= TAU_MODEL,
                        "adversary P1b: relocated parabola vertex {ep:?} plane residual {planar} \
                         > TAU_MODEL — SILENTLY accepted off-plane"
                    );
                    assert!(
                        geo <= TAU_MODEL,
                        "adversary P1b: relocated parabola vertex {ep:?} `y²=4f·x` residual {geo} \
                         > TAU_MODEL — SILENTLY accepted off the exact parabola"
                    );
                }
            }
        }
    }
}

// PROPERTY 1 (continued) — BEYOND-BAND VERTEX IS NEVER KEPT AS A PARABOLA VERTEX.
//
// Push ONE ring vertex beyond the cone band (on the plane) and leave the other 15
// on the exact parabola. SILENT_WRONG = 0: EITHER a loud Err, OR an Ok in which
// NO Curve::Parabola intersection-edge endpoint is beyond the cone band. (Mirrors
// the YR21 adversary's beyond-band ellipse check; the same upstream `on_both` gate
// demotes a beyond-band vertex to a LineSegment before Stage-4, so the held
// property is "no bogus parabola vertex", whichever gate rejects it.)
#[test]
fn adversary_beyond_band_no_silent_wrong_parabola_vertex() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let de = cone_d_eps();

    let mut ring = parabola_ring(0.0);
    // Push the MIDDLE (vertex-of-parabola) ring point off the cone by ~1.2·de,
    // staying on the plane (so only the cone band is violated).
    let mid = N_FACETS / 2;
    ring[mid] = parabola_ring_point(180.0_f64.to_radians(), -1.2 * de);
    let perturbed = ring[mid];
    assert!(
        cone_radial_residual(perturbed) > de && cut_plane_residual(perturbed) <= TAU_MODEL,
        "fixture: perturbed vertex must be > cone band yet on the plane; radial={}, plane={}",
        cone_radial_residual(perturbed),
        cut_plane_residual(perturbed)
    );

    let mock = LabelMock {
        arrangement: build_parabola_cap_arrangement(&ring),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    match r {
        Err(_) => { /* loud rejection — fine */ }
        Ok(brep) => {
            for e in parabola_edges(&brep) {
                let (s, t) = edge_endpoints(&brep, e);
                for ep in [s, t] {
                    let res = cone_radial_residual(ep).max(cut_plane_residual(ep));
                    assert!(
                        res <= de,
                        "adversary P1c: a Curve::Parabola intersection-edge endpoint {ep:?} is \
                         beyond the cone band (residual {res} > de={de}) — a SILENT WRONG. The \
                         beyond-band vertex must be rejected, never kept as a bogus parabola vertex."
                    );
                }
            }
        }
    }
}

// =========================================================================
// PROPERTY 2 — EVAL ROUND-TRIP (production-independent reconstruction).
//
// Each relocated parabola vertex carries TessellationSource::BRepEdge{edge,t}.
// Feeding the edge's OWN stored Curve::Parabola fields + that `t` to BOTH the
// production `parabola_point` AND my independent `eval_parabola_point` must
// reproduce the mesh position to TAU_MODEL, and the two evaluators must agree.
// =========================================================================

#[test]
fn adversary_parabola_eval_round_trip_independent() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let delta = adv_band_delta();
    let mock = LabelMock {
        arrangement: build_parabola_cap_arrangement(&parabola_ring(-delta)),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("adversary P2: off-curve parabola union must Ok after relocate");
    let tmap = r.tessellation_map();
    let mesh = r.as_mesh();

    let mut saw = false;
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
            let mesh_pos = mesh.verts[vid as usize].as_array();
            match tmap.lookup(vid) {
                TessellationSource::BRepEdge { edge: _, t } => {
                    saw = true;
                    // Production evaluator.
                    let prod = parabola_point(vertex, normal, axis_dir, focal_length, t).as_array();
                    let dp = norm(sub(prod, mesh_pos));
                    assert!(
                        dp <= TAU_MODEL,
                        "adversary P2: production parabola_point(t={t}) must reproduce mesh pos \
                         within TAU_MODEL; off by {dp}"
                    );
                    // INDEPENDENT evaluator (my own reimplementation).
                    let mine = eval_parabola_point(
                        vertex.as_array(),
                        normal.as_array(),
                        axis_dir.as_array(),
                        focal_length,
                        t,
                    );
                    let dm = norm(sub(mine, mesh_pos));
                    assert!(
                        dm <= TAU_MODEL,
                        "adversary P2: INDEPENDENT eval(t={t}) must reproduce mesh pos within \
                         TAU_MODEL; off by {dm}"
                    );
                    // And the two evaluators must agree (catches a convention drift
                    // between production and the paper parameterization).
                    let dpm = norm(sub(prod, mine));
                    assert!(
                        dpm <= TAU_MODEL,
                        "adversary P2: production and independent parabola eval must agree at \
                         t={t}; differ by {dpm}"
                    );
                }
                other => panic!(
                    "adversary P2: relocated parabola vertex {vid} must carry \
                     TessellationSource::BRepEdge, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw,
        "adversary P2: at least one relocated parabola vertex must carry a BRepEdge source"
    );
}

// =========================================================================
// PROPERTY 3 — NO REGRESSION on the sibling relocation paths.
//
// The parabola edit must not perturb the cone-ELLIPSE (YR21), cylinder-ELLIPSE
// (YR11), or CIRCLE (YR17) relocation paths. Each gets a focused independent
// fixture asserting an on-curve Ok.
// =========================================================================

// ---- (3a) cone-ELLIPSE regression (θ < α, bounded ellipse) ----

fn cone_ellipse_plane_normal() -> [f64; 3] {
    // The normal makes 20° with the axis ⇒ plane angle 70° > α ≈ 26.57° ⇒ ellipse.
    let beta = 20.0_f64.to_radians();
    unit([beta.sin(), 0.0, beta.cos()])
}
fn cone_ellipse_plane_d() -> f64 {
    -dot(cone_ellipse_plane_normal(), [0.0, 0.0, 2.5])
}
fn cone_ellipse_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(cone_ellipse_plane_normal()),
        d: cone_ellipse_plane_d(),
    }
}

fn cone_ellipse_ring(delta: f64) -> Vec<[f64; 3]> {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let n = cone_ellipse_plane_normal();
    let d = cone_ellipse_plane_d();
    (0..N_FACETS)
        .map(|k| {
            let phi = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
            let n_dot_r = dot(n, rhat);
            let n_dot_a = dot(n, ax);
            let s = (-d - delta * n_dot_r) / (n_dot_a + tana * n_dot_r);
            let rho = s * tana + delta;
            add(scale(ax, s), scale(rhat, rho))
        })
        .collect()
}

fn build_cone_cap_arrangement(ring: &[[f64; 3]]) -> LabeledArrangement {
    // identical topology to the parabola cap builder (apex fan + plane cap).
    build_parabola_cap_arrangement(ring)
}

#[test]
fn adversary_regression_cone_ellipse_still_on_curve() {
    // INDEPENDENT ssi-rs oracle: this section is an Ellipse.
    let plane = surface_to_quadric(cone_ellipse_plane_surface());
    let cone_q = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone_q).expect("oracle: cone∩plane");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "regression fixture must be an Ellipse, got {curves:?}"
    );

    let cone = oblique_cone();
    let bx = oblique_halfspace_box_for(cone_ellipse_plane_surface());
    let de = cone_d_eps();
    let delta = 0.5 * de;
    let mock = LabelMock {
        arrangement: build_cone_cap_arrangement(&cone_ellipse_ring(-delta)),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("adversary P3a: cone-ellipse path must still Ok after the parabola edit");
    assert_eq!(unpaired_half_edges(r.as_mesh()), 0, "P3a watertight");
    assert_eq!(euler_characteristic(r.as_mesh()), 2, "P3a χ=2");
    let n = cone_ellipse_plane_normal();
    let d = cone_ellipse_plane_d();
    let ellipses = ellipse_edges(&r);
    assert!(!ellipses.is_empty(), "P3a must carry ≥1 Ellipse edge");
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            assert!(
                cone_radial_residual(ep) <= TAU_MODEL && plane_residual_to(ep, n, d) <= TAU_MODEL,
                "adversary P3a: cone-ellipse vertex {ep:?} must stay on the exact ellipse"
            );
        }
    }
}

// ---- (3b) cylinder-ELLIPSE regression (YR11 path) ----

const CYL_RADIUS: f64 = 0.4;
const CYL_HEIGHT: f64 = 4.0;
fn cyl_dir() -> [f64; 3] {
    unit([0.0, 1.0, 2.0])
}
fn cyl_axis_point() -> [f64; 3] {
    let d = cyl_dir();
    [0.75 - 2.0 * d[0], 0.75 - 2.0 * d[1], 0.75 - 2.0 * d[2]]
}
fn cyl_brep() -> BRep {
    let axis_dir = cyl_dir();
    let axis_unit = unit(axis_dir);
    let axis_point = cyl_axis_point();
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, CYL_HEIGHT));
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));
    let v0 = add(bottom_center, scale(e1, CYL_RADIUS));
    let v1 = add(top_center, scale(e1, CYL_RADIUS));
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
                radius: CYL_RADIUS,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius: CYL_RADIUS,
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
                radius: CYL_RADIUS,
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
    BRep::new(verts, edges, faces).expect("cyl_brep: BRep::new failed")
}

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
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
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

fn cyl_ortho_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
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

fn cyl_build_tube(
    bottom: &[[f64; 3]],
    top: &[[f64; 3]],
    bot_center: [f64; 3],
    top_center: [f64; 3],
) -> LabeledArrangement {
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

fn cyl_ring_at_radius(rprime: f64) -> LabeledArrangement {
    let dir = cyl_dir();
    let axis_point = cyl_axis_point();
    let (e1, e2) = cyl_ortho_basis(dir);
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
    cyl_build_tube(&bottom, &top, bc, tc)
}

fn cyl_d_eps() -> f64 {
    let axis_unit = cyl_dir();
    let bottom_center = cyl_axis_point();
    let top_center = add(bottom_center, scale(axis_unit, CYL_HEIGHT));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = CYL_RADIUS * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    1e-2 * norm(sub(hi, lo))
}

fn cyl_radial_residual(x: [f64; 3]) -> f64 {
    (dist_point_to_line(x, cyl_axis_point(), cyl_dir()) - CYL_RADIUS).abs()
}
fn cap_plane(cap_z: f64) -> (Vector3, f64) {
    if cap_z == 0.0 {
        (Vector3::new(0.0, 0.0, -1.0), 0.0)
    } else {
        (Vector3::new(0.0, 0.0, 1.0), -cap_z)
    }
}
fn cyl_plane_residual(x: [f64; 3], cap_z: f64) -> f64 {
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

#[test]
fn adversary_regression_cylinder_ellipse_still_on_curve() {
    // INDEPENDENT ssi-rs oracle.
    let cyl_q = ssi_rs::QuadricSurface::Cylinder {
        axis_point: Point3::from(cyl_axis_point()),
        axis_dir: Vector3::from(cyl_dir()),
        radius: CYL_RADIUS,
    };
    let plane_q = ssi_rs::QuadricSurface::Plane {
        point: Point3::from([0.0, 0.0, 0.0]),
        normal: Vector3::new(0.0, 0.0, -1.0),
    };
    let curves = ssi_rs::intersect(&plane_q, &cyl_q).expect("oracle: oblique cap intersect");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "cylinder regression fixture must be an Ellipse, got {curves:?}"
    );

    let cyl = cyl_brep();
    let bx = box_15_brep();
    let de = cyl_d_eps();
    let delta = 0.4 * de;
    assert!(delta > TAU_WORK && delta <= de);
    let mock = LabelMock {
        arrangement: cyl_ring_at_radius(CYL_RADIUS - delta),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("adversary P3b: cylinder-ellipse path must still Ok after the parabola edit");
    assert_eq!(unpaired_half_edges(r.as_mesh()), 0, "P3b watertight");
    assert_eq!(euler_characteristic(r.as_mesh()), 2, "P3b χ=2");
    let ellipses = ellipse_edges(&r);
    assert!(!ellipses.is_empty(), "P3b must carry ≥1 Ellipse edge");
    for e in &ellipses {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let cz = cap_z_of(ep, 1.5);
            assert!(
                cyl_radial_residual(ep) <= TAU_MODEL && cyl_plane_residual(ep, cz) <= TAU_MODEL,
                "adversary P3b: cylinder-ellipse vertex {ep:?} must stay on the exact ellipse"
            );
        }
    }
}

// ---- (3c) CIRCLE regression (perpendicular cap, YR10-class). Structurally the
// canonical Stage-4 circle fixture: an axis +Z cylinder through a unit cube, the
// two box z-caps (z=0, z=1) each a perpendicular Circle section, driven by a
// closed tube+caps LabeledArrangement whose lateral ring sits OFF the cylinder by
// δ. (An apex-fan single-section arrangement does NOT attribute, so we use the
// proven closed-tube structure.) ----

const CIRC_AXIS_POINT: [f64; 3] = [0.5, 0.5, -0.5];
const CIRC_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CIRC_RADIUS: f64 = 0.25;
const CIRC_HEIGHT: f64 = 2.0;

fn axis_cyl_brep() -> BRep {
    let axis_unit = unit(CIRC_AXIS_DIR);
    let bottom_center = CIRC_AXIS_POINT;
    let top_center = add(bottom_center, scale(axis_unit, CIRC_HEIGHT));
    let v0 = add(bottom_center, [CIRC_RADIUS, 0.0, 0.0]);
    let v1 = add(top_center, [CIRC_RADIUS, 0.0, 0.0]);
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
                radius: CIRC_RADIUS,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius: CIRC_RADIUS,
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
                axis_point: p(CIRC_AXIS_POINT[0], CIRC_AXIS_POINT[1], CIRC_AXIS_POINT[2]),
                axis_dir: Vector3::new(CIRC_AXIS_DIR[0], CIRC_AXIS_DIR[1], CIRC_AXIS_DIR[2]),
                radius: CIRC_RADIUS,
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
    BRep::new(verts, edges, faces).expect("axis_cyl_brep: BRep::new failed")
}

fn unit_cube_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 1.0, 1.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 1.0),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
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
    let offs = [0.0, -1.0, 0.0, -1.0, -1.0, 0.0];
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
    BRep::new(verts, edges, faces).expect("unit_cube_brep: BRep::new failed")
}

/// A closed tube+caps arrangement: lateral ring at radius `rprime` (label 0 =
/// cylinder A), two cap fans at z=0 / z=1 (label 1 = box B). Mirrors the YR10
/// canonical circle fixture.
fn circle_tube_arrangement(rprime: f64) -> LabeledArrangement {
    let cx = CIRC_AXIS_POINT[0];
    let cy = CIRC_AXIS_POINT[1];
    let (za, zb) = (0.0f64, 1.0f64);
    let ring: Vec<(f64, f64)> = (0..N_FACETS)
        .map(|k| {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            (cx + rprime * th.cos(), cy + rprime * th.sin())
        })
        .collect();

    let mut verts: Vec<Point3> = Vec::new();
    let mut bot = Vec::with_capacity(N_FACETS);
    let mut top = Vec::with_capacity(N_FACETS);
    for &(x, y) in &ring {
        bot.push(verts.len() as u32);
        verts.push(p(x, y, za));
    }
    for &(x, y) in &ring {
        top.push(verts.len() as u32);
        verts.push(p(x, y, zb));
    }
    let cb = verts.len() as u32;
    verts.push(p(cx, cy, za));
    let ct = verts.len() as u32;
    verts.push(p(cx, cy, zb));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    let mut push = |t: [u32; 3], label: u32| {
        tris.push(t);
        surface.push(vec![LaInputId(label)]);
    };
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([bot[k], bot[k1], top[k1]], 0);
        push([bot[k], top[k1], top[k]], 0);
    }
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([cb, bot[k1], bot[k]], 1);
    }
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([ct, top[k], top[k1]], 1);
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

fn circ_d_eps() -> f64 {
    let axis_unit = unit(CIRC_AXIS_DIR);
    let bottom = CIRC_AXIS_POINT;
    let top = add(bottom, scale(axis_unit, CIRC_HEIGHT));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in [bottom, top] {
        for i in 0..3 {
            let span = CIRC_RADIUS * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(c[i] - span);
            hi[i] = hi[i].max(c[i] + span);
        }
    }
    1e-2 * norm(sub(hi, lo))
}

/// Residual `max(|axial|, |radial − r|)` to the perpendicular cap circle on z.
fn circle_residual_at(pt: [f64; 3], cap_z: f64) -> f64 {
    let axial = (pt[2] - cap_z).abs();
    let radial =
        ((pt[0] - CIRC_AXIS_POINT[0]).powi(2) + (pt[1] - CIRC_AXIS_POINT[1]).powi(2)).sqrt();
    axial.max((radial - CIRC_RADIUS).abs())
}

#[test]
fn adversary_regression_circle_still_on_curve() {
    // INDEPENDENT ssi-rs oracle: the perpendicular cap section is a Circle.
    let plane_q = ssi_rs::QuadricSurface::Plane {
        point: Point3::from([0.0, 0.0, 0.0]),
        normal: Vector3::new(0.0, 0.0, -1.0),
    };
    let cyl_q = surface_to_quadric(Surface::Cylinder {
        axis_point: Point3::from(CIRC_AXIS_POINT),
        axis_dir: Vector3::from(CIRC_AXIS_DIR),
        radius: CIRC_RADIUS,
    });
    let curves = ssi_rs::intersect(&plane_q, &cyl_q).expect("oracle: cap intersect");
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Circle { .. })),
        "circle regression fixture must be a Circle, got {curves:?}"
    );

    let cyl = axis_cyl_brep();
    let bx = unit_cube_brep();
    let de = circ_d_eps();
    let delta = 0.4 * de;
    assert!(delta > TAU_WORK && delta <= de);
    let mock = LabelMock {
        arrangement: circle_tube_arrangement(CIRC_RADIUS - delta),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("adversary P3c: circle path must still Ok after the parabola edit");
    assert_eq!(unpaired_half_edges(r.as_mesh()), 0, "P3c watertight");
    assert_eq!(euler_characteristic(r.as_mesh()), 2, "P3c χ=2");
    let circles = circle_edges(&r);
    assert!(!circles.is_empty(), "P3c must carry ≥1 Circle edge");
    for e in &circles {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            // The intersection circles sit at z≈0 / z≈1; check whichever cap.
            let cap_z = if ep[2] < 0.5 { 0.0 } else { 1.0 };
            assert!(
                circle_residual_at(ep, cap_z) <= TAU_MODEL,
                "adversary P3c: circle vertex {ep:?} must stay on the exact z={cap_z} circle"
            );
        }
    }
}

// =========================================================================
// PROPERTY 4 — OUT-OF-SCOPE STAYS LOUD (Err, never Ok, never panic).
//
// (4a) HYPERBOLA cone section (θ < α) → loud Err.
// (4b) An axis/generator-parallel degenerate ring (every generator parallel to
//      the plane, so no bounded pierce) → loud Err (never a bogus Ok).
// =========================================================================

fn hyperbola_plane_normal() -> [f64; 3] {
    // normal nearly ⊥ to axis ⇒ plane angle < α ⇒ hyperbola (two branches).
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
fn adversary_hyperbola_section_stays_loud() {
    // INDEPENDENT ssi-rs oracle: NOT an Ellipse, NOT a Parabola.
    let plane = surface_to_quadric(hyperbola_plane_surface());
    let cone_q = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone_q).expect("oracle: hyperbola Plane∩Cone");
    assert!(
        !curves.iter().any(|c| matches!(
            c,
            ssi_rs::SsiCurve::Ellipse { .. } | ssi_rs::SsiCurve::Parabola { .. }
        )),
        "oracle: the θ<α section must be neither Ellipse nor Parabola, got {curves:?}"
    );

    // Drive the public surface with a cone-cap mock whose seam ring lies on
    // cone ∩ hyperbola-plane (piercing arc with bounded positive s).
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let cosa = cone_half_angle().cos();
    let sina = cone_half_angle().sin();
    let n = hyperbola_plane_normal();
    let d = hyperbola_plane_d();
    let mut ring: Vec<[f64; 3]> = Vec::new();
    let n_samp = 24usize;
    for k in 0..n_samp {
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
    let mock = LabelMock {
        arrangement: build_cone_cap_arrangement(&ring),
    };
    let bx = oblique_halfspace_box_for(hyperbola_plane_surface());
    let cone = oblique_cone();
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    assert!(
        r.is_err(),
        "adversary P4a: a held out-of-scope HYPERBOLA cone section must STOP loudly, not Ok; \
         got {:?}",
        r.map(|b| b.edges().iter().map(|e| e.curve).collect::<Vec<_>>())
    );
}

#[test]
fn adversary_degenerate_axis_parallel_stays_loud() {
    // A cutting plane whose normal is ALONG the cone axis would be a circle; the
    // genuinely degenerate / out-of-scope case for the cone arm is one where the
    // parabola section's generator pierces at s ≤ 0 (through-apex). Use a plane
    // THROUGH the apex (d=0): every generator pierces only AT the apex (s=0),
    // driving the `s ≤ 0` guard. SILENT_WRONG=0: must be a loud Err.
    let cone = oblique_cone();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let (e1, e2) = azim_basis();
    // through-apex plane parallel to the +X generator (θ=α through apex).
    let n = unit([1.0, 0.0, -tana]);
    let mut ring: Vec<[f64; 3]> = Vec::new();
    for k in 0..N_FACETS {
        let phi = std::f64::consts::PI * (160.0 / 180.0)
            + std::f64::consts::PI * (40.0 / 180.0) * (k as f64) / ((N_FACETS - 1) as f64);
        let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
        let s0 = 0.6;
        ring.push(add(scale(ax, s0), scale(rhat, s0 * tana)));
    }
    let through_apex_plane = Surface::Plane {
        normal: Vector3::from(n),
        d: 0.0,
    };
    let bx = oblique_halfspace_box_for(through_apex_plane);
    let mock = LabelMock {
        arrangement: build_cone_cap_arrangement(&ring),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);
    assert!(
        r.is_err(),
        "adversary P4b: a through-apex / degenerate (s≤0) cone-parabola section must STOP loudly, \
         not return Ok; got {:?}",
        r.map(|b| b.edges().iter().map(|e| e.curve).collect::<Vec<_>>())
    );
}

// =========================================================================
// PROPERTY 5 — ORACLE4-REFRAME SOUNDNESS PROBE.
//
// The reframed oracle4 checks the boolean OUTPUT is watertight (0 unpaired
// half-edges) + χ=2 + signed volume > 0, plus the per-facet area floor. The
// question: can a connectivity- and volume-preserving LOCAL orientation fold pass
// this while leaving relocated vertices in the wrong place? This probe builds, on
// the SIMULATED output mesh (no pipeline), a single-facet orientation flip and
// shows it BREAKS half-edge pairing (so oracle4's `unpaired == 0` catches it) — a
// local fold cannot be both topologically watertight AND inverted on one facet.
//
// It then argues (in code via the on-curve check) that the on-curve residual
// oracles O2/O3 are the load-bearing WHERE-checks: even a hypothetical fold that
// somehow preserved χ would still have to move vertices to pass O2/O3, which it
// cannot. The residual risk is stated honestly in the VERDICT.
// =========================================================================

/// Build the simulated Union output (keep-all, no flip) of the on-parabola cap.
fn simulated_on_parabola_output() -> Mesh {
    let arr = build_parabola_cap_arrangement(&parabola_ring(0.0));
    Mesh::new(arr.mesh.verts.clone(), arr.mesh.tris.clone())
}

#[test]
fn adversary_oracle4_reframe_local_fold_breaks_watertight() {
    let base = simulated_on_parabola_output();
    // Sanity: the unflipped mesh is the genus-0 closed shell oracle4 expects.
    assert_eq!(
        unpaired_half_edges(&base),
        0,
        "P5 sanity: baseline simulated output is watertight"
    );
    assert_eq!(euler_characteristic(&base), 2, "P5 sanity: χ=2");
    assert!(signed_volume(&base) > 0.0, "P5 sanity: outward");

    // Now FLIP a single facet's winding (a local orientation fold). This is the
    // minimal "localized orientation fold" the reframe is asked about.
    let mut folded_tris = base.tris.clone();
    let f = &mut folded_tris[0];
    f.swap(1, 2);
    let folded = Mesh::new(base.verts.clone(), folded_tris);

    // The fold leaves χ unchanged (same V, E, F)...
    assert_eq!(
        euler_characteristic(&folded),
        2,
        "P5: a single-facet flip preserves the Euler characteristic χ=2"
    );
    // ...but it BREAKS half-edge pairing: oracle4's watertight check catches it.
    let unpaired = unpaired_half_edges(&folded);
    assert!(
        unpaired > 0,
        "P5: a localized orientation fold MUST break half-edge pairing (unpaired {unpaired} > 0) \
         — this is WHY oracle4's `unpaired == 0` check is load-bearing and a connectivity-\
         preserving fold cannot silently pass. (If this ever became 0, the reframe would have a \
         real blind spot — see VERDICT residual-risk note.)"
    );
}

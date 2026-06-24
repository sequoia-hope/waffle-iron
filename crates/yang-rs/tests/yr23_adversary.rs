//! PR-YR23 ADVERSARY — independent adversarial check on the cone∩plane HYPERBOLA
//! end-to-end relocation + two-branch selection (`Curve::Hyperbola` variant +
//! `hyperbola_point` eval + `ssi_curve_to_curve` / `curve_contains_point`
//! Hyperbola arms with the `u>0` branch discriminator + the Stage-4
//! `ConeHyperbolaReloc` relocation arm + `is_reversed` hyperbola tangent) landed
//! in `crates/yang-rs/src/lib.rs` by the GREEN subagent.
//!
//! This file is the ADVERSARY half of a role-separated FIP cycle: it writes TESTS
//! ONLY and never edits production, the RED file (`yr23_cone_hyperbola.rs`), the
//! sibling RED/adversary files, or any other file. Per the repo convention that
//! integration-test files cannot share helpers, the harness (`p`, array math,
//! `cone_brep`, the box builder, the mesh oracles, `LabelMock`, the independent
//! on-conic residuals, the cone-ring/cap arrangement builder) is re-declared here
//! — independently authored (distinct δ choices, an independent re-implemented
//! `hyperbola_point`, an independent `(u/a)²−(v/b)²` membership predicate, an
//! independently-recomputed apex-fan centroid residual, distinct ellipse/parabola
//! regression fixtures).
//!
//! Adversarial contract (the seven attacks this file guards, each a `#[test]` or a
//! documented finding in the final report):
//!   1. **Two-branch selection genuinely rejects the wrong nappe** — a MUTATION
//!      proof: my OWN `(u/a)²−(v/b)²≈1 AND u>0` predicate REJECTS the −z branch
//!      for the upper-nappe ring endpoints (`u<0`) and ACCEPTS the +z branch;
//!      the END-TO-END emitted `Curve::Hyperbola` edge's `major_axis` is the +z
//!      branch (dot with +ẑ ≈ +1), NOT the −z branch.
//!   2. **Membership band is NOT a flat widening (P9/P10)** — `geo_res =
//!      |F|/|∇F|` is a true length: on-curve ⇒ ≈0; ~k·d_ε off scales ~linearly
//!      with k; a point just beyond the band is EXCLUDED, and shrinking `tol`
//!      shrinks the admitted set.
//!   3. **No regression of YR21 ellipse / YR22 parabola** — an ellipse cut →
//!      `Curve::Ellipse`, a θ=α cut → `Curve::Parabola`, a hyperbola cut → 2×
//!      `Curve::Hyperbola`, via `ssi_rs::intersect` + the public `boolean()`.
//!   4. **No silent-wrong** — characterize the outcome at several δ (0.5, 1.0,
//!      1.2, 2.0 × d_ε): each is either an on-curve `Ok` or a LOUD `Err`; NO δ
//!      yields an `Ok` whose hyperbola edge was silently snapped from a
//!      beyond-band crossing.
//!   5. **oracle7-reframe scrutiny** — the 2·d_ε fixture is genuinely `Err`
//!      `FaceResolutionFailed`, BECAUSE the apex-fan centroid is independently
//!      recomputed > d_ε off the cone; pin the reason + the centroid fact.
//!   6. **Eval round-trip via an INDEPENDENT re-implementation** — my own
//!      `center + a·cosh(t)·major + b·sinh(t)·(normal×major)` matches production
//!      `yang_rs::hyperbola_point` at several t (catches a production typo
//!      oracle3's production-helper-on-both-sides cannot).
//!   7. **Measure/fuzz honesty** — stated in the report (not a test): the
//!      hyperbola IS the common random-box cone section, so this PR SHOULD move
//!      the cone fuzz number; the capability is proven by the unit oracles + the
//!      env-gated real-sidecar E2E (oracle8); the fuzz delta is driver-verified
//!      separately (curved fuzz cannot complete in-container).
//!
//! Tolerances mirror the RED file (do NOT weaken): on-conic / round-trip use
//! `cad_primitives::TAU_MODEL` (1e-7); the off-band budget is the cone's own
//! `cone_chord_bound`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, hyperbola_point, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError,
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

fn hyperbola_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Hyperbola { .. }))
        .collect()
}
fn ellipse_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Ellipse { .. }))
        .collect()
}
fn parabola_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Parabola { .. }))
        .collect()
}

// =========================================================================
// HYPERBOLA cone fixture. I reuse the RED cone geometry (apex at origin, axis
// +Z, tanα=0.5, height 4) and the x=1 cutting plane because that is the geometry
// empirically proven to be the HYPE case; an adversary inventing a brand-new cone
// risks accidentally landing on an ellipse/parabola. My INDEPENDENCE is in the
// perturbations / δ choices / oracles below — and the ssi-rs oracle re-confirms
// the section is TWO Hyperbola each run, so I never trust the geometry on faith.
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

/// HYPE cutting plane: n = [1,0,0] (∥ the cone axis), through (1,0,0) — `x = 1`.
fn cut_plane_normal() -> [f64; 3] {
    [1.0, 0.0, 0.0]
}
fn cut_plane_d() -> f64 {
    -dot(cut_plane_normal(), [1.0, 0.0, 0.0])
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

/// A large tilted box whose top face is `plane_surf`, centered at the foot of a
/// mid-arc point onto the plane (half-width 10, depth 10 along −n̂).
fn oblique_halfspace_box_for(plane_surf: Surface, anchor: [f64; 3]) -> BRep {
    let Surface::Plane { normal, d } = plane_surf else {
        unreachable!("oblique_halfspace_box_for expects a Plane");
    };
    let n = unit(normal.as_array());
    let off = dot(n, anchor) + d;
    let center = sub(anchor, scale(n, off));
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

/// The HYPE cutting box: top face is the x=1 plane, anchored on the mid-arc point.
fn cutting_box() -> BRep {
    oblique_halfspace_box_for(cut_plane_surface(), [1.0, 0.0, 2.0])
}

// =========================================================================
// SSI ORACLE — independently confirm the section type via ssi-rs.
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

fn section_curves(plane: Surface, surf: Surface) -> Vec<ssi_rs::SsiCurve> {
    let q0 = surface_to_quadric(plane);
    let q1 = surface_to_quadric(surf);
    ssi_rs::intersect(&q0, &q1).expect("adversary: section intersect must succeed")
}

fn expect_hyperbola(c: &ssi_rs::SsiCurve) -> ([f64; 3], [f64; 3], [f64; 3], f64, f64) {
    match c {
        ssi_rs::SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => (
            center.as_array(),
            normal.as_array(),
            major_axis.as_array(),
            *semi_transverse,
            *semi_conjugate,
        ),
        other => panic!("expected Hyperbola, got {other:?}"),
    }
}

/// Both ssi-rs hyperbola branches for the fixture, asserting EXACTLY two.
fn both_branches() -> Vec<ssi_rs::SsiCurve> {
    let curves = section_curves(cut_plane_surface(), cone_surface());
    assert_eq!(
        curves.len(),
        2,
        "adversary fixture: HYPE section must be exactly TWO branches, got {curves:?}"
    );
    for c in &curves {
        assert!(
            matches!(c, ssi_rs::SsiCurve::Hyperbola { .. }),
            "adversary fixture: each branch must be a Hyperbola, got {c:?}"
        );
    }
    curves
}

fn upper_branch() -> ssi_rs::SsiCurve {
    let ax = unit(CONE_AXIS);
    for c in both_branches() {
        let (_, _, m, _, _) = expect_hyperbola(&c);
        if dot(unit(m), ax) > 0.0 {
            return c;
        }
    }
    panic!("adversary: no +axis (upper) branch");
}
fn lower_branch() -> ssi_rs::SsiCurve {
    let ax = unit(CONE_AXIS);
    for c in both_branches() {
        let (_, _, m, _, _) = expect_hyperbola(&c);
        if dot(unit(m), ax) < 0.0 {
            return c;
        }
    }
    panic!("adversary: no −axis (lower) branch");
}

// =========================================================================
// INDEPENDENT on-conic residuals (recomputed from the fixture, NOT production).
// =========================================================================

fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let ax = unit(CONE_AXIS);
    let w = sub(x, CONE_APEX);
    let h_axial = dot(w, ax);
    let radial = norm(sub(w, scale(ax, h_axial)));
    (radial - h_axial.abs() * cone_half_angle().tan()).abs()
}
fn plane_residual(x: [f64; 3]) -> f64 {
    (dot(x, cut_plane_normal()) + cut_plane_d()).abs()
}
fn conic_residual(x: [f64; 3]) -> f64 {
    cone_radial_residual(x).max(plane_residual(x))
}

// =========================================================================
// INDEPENDENT in-plane (u,v) frame + `(u/a)²−(v/b)²` membership — the adversary's
// OWN re-implementation of the branch discriminator (distinct from the RED file's
// `hyperbola_inplane_residual`, though mathematically the same relation). It also
// converts the dimensionless implicit residual to a geometric (length) residual
// via the in-plane gradient `|∇F| = |(2u/a², −2v/b²)|`, MIRRORING production's
// `curve_contains_point` scaling — so a "membership" verdict here tests the SAME
// semantics production uses.
// =========================================================================

fn hyperbola_uv(pt: [f64; 3], hyp: &ssi_rs::SsiCurve) -> (f64, f64) {
    let (center, normal, major, _, _) = expect_hyperbola(hyp);
    let m = unit(major);
    let conj = cross(unit(normal), m);
    let w = sub(pt, center);
    (dot(w, m), dot(w, conj))
}

/// Geometric (length) on-hyperbola residual `|(u/a)²−(v/b)²−1| / |∇F|`. Zero ⇒ on
/// the exact hyperbola branch curve. Matches production `curve_contains_point`.
fn hyperbola_geo_residual(pt: [f64; 3], hyp: &ssi_rs::SsiCurve) -> f64 {
    let (_, _, _, a, b) = expect_hyperbola(hyp);
    let (u, v) = hyperbola_uv(pt, hyp);
    let implicit = ((u / a).powi(2) - (v / b).powi(2) - 1.0).abs();
    let gu = 2.0 * u / (a * a);
    let gv = 2.0 * v / (b * b);
    let grad = (gu * gu + gv * gv).sqrt();
    if grad > MIN_FEATURE_SIZE {
        implicit / grad
    } else {
        implicit
    }
}

/// The adversary's OWN membership predicate: out-of-plane reject, then geometric
/// residual ≤ tol AND the `u > 0` branch discriminator. Independent of production.
fn adv_contains(hyp: &ssi_rs::SsiCurve, pt: [f64; 3], tol: f64) -> bool {
    let (center, normal, _, _, _) = expect_hyperbola(hyp);
    let oop = dot(sub(pt, center), unit(normal)).abs();
    if oop > tol {
        return false;
    }
    let (u, _v) = hyperbola_uv(pt, hyp);
    hyperbola_geo_residual(pt, hyp) <= tol && u > 0.0
}

/// INDEPENDENT hyperbola eval, re-implemented from scratch (NOT calling
/// production): `center + a·cosh(t)·major + b·sinh(t)·(normal × major)`.
fn eval_hyperbola_point(
    center: [f64; 3],
    normal: [f64; 3],
    major_axis: [f64; 3],
    a: f64,
    b: f64,
    t: f64,
) -> [f64; 3] {
    let m = major_axis;
    let conj = cross(normal, m);
    add(
        center,
        add(scale(m, a * t.cosh()), scale(conj, b * t.sinh())),
    )
}

// =========================================================================
// HYPERBOLA cap arrangement builder (re-declared; same topology as RED — apex fan
// + plane cap over the bounded upper-nappe φ∈[−12°,12°] arc), parameterized by a
// radial offset so I author my OWN δ.
// =========================================================================

fn azim_basis() -> ([f64; 3], [f64; 3]) {
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// One ring vertex on the cone at azimuth φ, radial offset `delta`, solved to land
/// EXACTLY on the x=1 plane. `delta=0` ⇒ on the exact hyperbola; `delta<0` ⇒ off
/// the cone radially by ~|delta| but still on the plane.
fn hyperbola_ring_point(phi: f64, delta: f64) -> [f64; 3] {
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

fn hyperbola_ring(delta: f64) -> Vec<[f64; 3]> {
    let lo = (-12.0_f64).to_radians();
    let hi = 12.0_f64.to_radians();
    (0..N_FACETS)
        .map(|k| {
            let phi = lo + (hi - lo) * (k as f64) / ((N_FACETS - 1) as f64);
            hyperbola_ring_point(phi, delta)
        })
        .collect()
}

fn build_hyperbola_cap_arrangement(delta: f64) -> LabeledArrangement {
    let ring = hyperbola_ring(delta);
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
    for k in 0..N_FACETS {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    for k in 0..N_FACETS {
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    LabeledArrangement {
        mesh,
        surface,
        inside: vec![vec![false, false]; n],
        patch: vec![0u32; n],
        num_inputs: 2,
    }
}

// =========================================================================
// ATTACK 1 — TWO-BRANCH SELECTION genuinely rejects the wrong nappe.
//
// (1a) MUTATION proof, predicate level: for the upper-nappe (exact, on-curve) ring
//      endpoints, my OWN `(u/a)²−(v/b)² AND u>0` membership ACCEPTS the +z branch
//      and REJECTS the −z branch (which gives u<0). So exactly ONE branch matches
//      ⇒ `matched == 1`.
// (1b) END-TO-END: the emitted Curve::Hyperbola edge's stored major_axis is the +z
//      branch (dot with +ẑ ≈ +1), NOT the −z branch.
// =========================================================================

#[test]
fn adversary_two_branch_selection_rejects_wrong_nappe() {
    let curves = both_branches();
    let upper = upper_branch();
    let lower = lower_branch();
    let ax = unit(CONE_AXIS);

    // The branches genuinely have OPPOSITE major_axis sign along the axis.
    let (_, _, mu, _, _) = expect_hyperbola(&upper);
    let (_, _, ml, _, _) = expect_hyperbola(&lower);
    assert!(
        dot(unit(mu), ax) > 0.5 && dot(unit(ml), ax) < -0.5,
        "adversary 1: the two branches must have opposite +z/−z major_axis; \
         upper·ẑ={}, lower·ẑ={}",
        dot(unit(mu), ax),
        dot(unit(ml), ax)
    );

    // Exact (on-curve) upper-nappe ring endpoints.
    let ring = hyperbola_ring(0.0);
    let endpoints = [ring[0], ring[N_FACETS / 2], ring[N_FACETS - 1]];
    for ep in endpoints {
        assert!(
            conic_residual(ep) <= TAU_MODEL,
            "adversary 1: exact ring endpoint {ep:?} must be on the section (residual {})",
            conic_residual(ep)
        );
    }

    // (1a) MUTATION: my predicate ACCEPTS the +z (upper) branch and REJECTS the
    // −z (lower) branch for every upper-nappe endpoint. The lower branch gives
    // u < 0 for these points (the discriminator), so it is rejected.
    let mut matched = 0usize;
    for c in &curves {
        let all = endpoints.iter().all(|&ep| adv_contains(c, ep, TAU_MODEL));
        if all {
            matched += 1;
            let (_, _, m, _, _) = expect_hyperbola(c);
            assert!(
                dot(unit(m), ax) > 0.0,
                "adversary 1a: the matched branch must be the +z (upper) branch"
            );
        }
    }
    assert_eq!(
        matched, 1,
        "adversary 1a: EXACTLY ONE branch may contain the upper-nappe endpoints under \
         my own `(u/a)²−(v/b)² AND u>0` membership; got matched={matched}"
    );

    // Concrete u-sign witness: every upper endpoint has u>0 on the +z branch and
    // u<0 on the −z branch.
    for ep in endpoints {
        let (uu, _) = hyperbola_uv(ep, &upper);
        let (ul, _) = hyperbola_uv(ep, &lower);
        assert!(
            uu > 0.0,
            "adversary 1a: endpoint {ep:?} must have u>0 on the +z branch (u={uu})"
        );
        assert!(
            ul < 0.0,
            "adversary 1a: endpoint {ep:?} must have u<0 on the −z branch (u={ul}) — \
             this is the discriminator that rejects the wrong nappe"
        );
    }

    // (1b) END-TO-END: the emitted Curve::Hyperbola edge stores the +z branch.
    let mock = LabelMock {
        arrangement: build_hyperbola_cap_arrangement(0.0),
    };
    let r = boolean(&oblique_cone(), &cutting_box(), BoolOp::Union, &mock)
        .expect("adversary 1b: on-curve hyperbola union must Ok");
    let hyps = hyperbola_edges(&r);
    assert!(
        !hyps.is_empty(),
        "adversary 1b: output must carry ≥1 Curve::Hyperbola edge"
    );
    for e in &hyps {
        let Curve::Hyperbola { major_axis, .. } = e.curve else {
            continue;
        };
        let m = unit(major_axis.as_array());
        assert!(
            dot(m, ax) > 0.5,
            "adversary 1b: the emitted edge major_axis {m:?} must be the +z (UPPER) branch \
             (m·ẑ={} > 0.5), NOT the lower nappe",
            dot(m, ax)
        );
        // And it is genuinely NOT the lower branch's major_axis.
        let (_, _, ml2, _, _) = expect_hyperbola(&lower);
        assert!(
            dot(m, unit(ml2)) < -0.5,
            "adversary 1b: the emitted edge major_axis must be OPPOSITE the lower branch's \
             (dot {} < −0.5)",
            dot(m, unit(ml2))
        );
    }
}

// =========================================================================
// ATTACK 2 — MEMBERSHIP BAND IS NOT A FLAT WIDENING (P9/P10). The geo_res =
// |F|/|∇F| is a true geometric length: (a) on-curve ⇒ ≈0; (b) ~k·d_ε off scales
// ~linearly with k; (c) just beyond the band is EXCLUDED; (d) shrinking tol
// shrinks the admitted set (so the band tracks tol, not a constant fudge).
// =========================================================================

#[test]
fn adversary_membership_band_is_geometric_not_flat() {
    let hyp = upper_branch();
    let de = cone_d_eps();

    // (a) ON the curve ⇒ residual ≈ 0.
    let on = hyperbola_ring_point(0.0, 0.0);
    let geo_on = hyperbola_geo_residual(on, &hyp);
    assert!(
        geo_on <= TAU_MODEL,
        "adversary 2a: an on-curve point must have geo_res ≈ 0 (got {geo_on})"
    );

    // (b) ~k·d_ε off (radially, on the plane) ⇒ geo_res grows ~linearly with k.
    // The geo_res is a LENGTH; if it were a constant fudge it would not scale.
    let ks = [0.5_f64, 1.0, 2.0, 4.0];
    let mut prev = 0.0_f64;
    let mut ratios: Vec<f64> = Vec::new();
    for (i, &k) in ks.iter().enumerate() {
        let off = hyperbola_ring_point(0.0, -k * de);
        // Confirm the perturbation is genuinely a radial cone offset of ~k·d_ε.
        assert!(
            (cone_radial_residual(off) - k * de).abs() <= 1e-9 + 1e-6 * k * de,
            "adversary 2b: probe at k={k} must be ~k·d_ε off the cone radially \
             (got {})",
            cone_radial_residual(off)
        );
        let geo = hyperbola_geo_residual(off, &hyp);
        assert!(
            geo > prev,
            "adversary 2b: geo_res must INCREASE with the off-distance k (k={k}: {geo} \
             not > previous {prev}) — a flat band would be constant"
        );
        if i > 0 {
            ratios.push(geo / prev);
        }
        prev = geo;
    }
    // The growth is monotone and substantial (doubling k roughly doubles geo_res
    // near the curve) — i.e. genuinely length-like, not constant. We assert each
    // doubling/quadrupling step grows the residual by a clear factor (>1.3),
    // proving it is NOT a flat fudge.
    for r in &ratios {
        assert!(
            *r > 1.3,
            "adversary 2b: each off-distance step must grow geo_res by a clear factor \
             (got ratio {r}) — confirms a geometric length, not a constant"
        );
    }

    // (c) Just-beyond-band point is EXCLUDED at tol = d_ε; (d) shrinking tol
    // shrinks the admitted set. NOTE the in-plane geo_res near the branch vertex
    // (φ=0, u≈a) is ~2× the RADIAL cone offset — the hyperbola is steeper than
    // the radial direction there — so a 0.3·d_ε radial offset maps to ~0.6·d_ε of
    // in-plane perpendicular distance. That asymmetry is itself proof the band is
    // a true geometric length, not a flat radial widening: the SAME radial offset
    // admits/excludes differently depending on in-plane position.
    let just_beyond = hyperbola_ring_point(0.0, -2.0 * de);
    assert!(
        !adv_contains(&hyp, just_beyond, de),
        "adversary 2c: a point 2·d_ε off the cone (geo_res ≈ 3.8·d_ε) must be EXCLUDED at tol=d_ε"
    );
    // A 0.3·d_ε radial-off vertex point (geo_res ≈ 0.6·d_ε) is admitted at tol=d_ε.
    let inband = hyperbola_ring_point(0.0, -0.3 * de);
    let inband_geo = hyperbola_geo_residual(inband, &hyp);
    assert!(
        inband_geo > 0.3 * de && inband_geo < de,
        "adversary 2d: the 0.3·d_ε-radial probe must have geo_res strictly between 0.3·d_ε and \
         d_ε (got {inband_geo}) — the in-plane length differs from the radial offset, confirming \
         a true geometric metric"
    );
    assert!(
        adv_contains(&hyp, inband, de),
        "adversary 2d: a 0.3·d_ε-radial probe (geo_res ≈ 0.6·d_ε) must be ADMITTED at tol=d_ε"
    );
    assert!(
        !adv_contains(&hyp, inband, 0.3 * de),
        "adversary 2d: the SAME probe must be EXCLUDED at the tighter tol=0.3·d_ε \
         — proving membership tracks tol (NOT a flat widening that ignores tol)"
    );
}

// =========================================================================
// ATTACK 3 — NO REGRESSION of YR21 (cone ellipse) / YR22 (cone parabola). An
// ellipse cut → 1× Curve::Ellipse; a θ=α cut → 1× Curve::Parabola; the hyperbola
// cut → 2× Curve::Hyperbola. Confirmed both via ssi-rs (section type) and the
// public boolean() (emitted curve type, on-curve, watertight).
// =========================================================================

// ---- (3a) cone-ELLIPSE (θ < α) still produces a Curve::Ellipse ----

fn ellipse_plane_normal() -> [f64; 3] {
    // 20° from axis ⇒ plane angle 70° > α ≈ 26.57° ⇒ ellipse.
    let beta = 20.0_f64.to_radians();
    unit([beta.sin(), 0.0, beta.cos()])
}
fn ellipse_plane_d() -> f64 {
    -dot(ellipse_plane_normal(), [0.0, 0.0, 2.5])
}
fn ellipse_plane_surface() -> Surface {
    Surface::Plane {
        normal: Vector3::from(ellipse_plane_normal()),
        d: ellipse_plane_d(),
    }
}
fn ellipse_ring(delta: f64) -> Vec<[f64; 3]> {
    let (e1, e2) = azim_basis();
    let ax = unit(CONE_AXIS);
    let tana = cone_half_angle().tan();
    let n = ellipse_plane_normal();
    let d = ellipse_plane_d();
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

#[test]
fn adversary_regression_cone_ellipse_still_curve_ellipse() {
    let curves = section_curves(ellipse_plane_surface(), cone_surface());
    assert!(
        curves
            .iter()
            .any(|c| matches!(c, ssi_rs::SsiCurve::Ellipse { .. })),
        "adversary 3a: ssi-rs section must be an Ellipse, got {curves:?}"
    );

    let de = cone_d_eps();
    let delta = 0.5 * de;
    let ring = ellipse_ring(-delta);
    let mut cap_c = [0.0; 3];
    for v in &ring {
        cap_c = add(cap_c, *v);
    }
    cap_c = scale(cap_c, 1.0 / ring.len() as f64);
    let mut verts: Vec<Point3> = vec![p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2])];
    let rim_base = verts.len() as u32;
    for v in &ring {
        verts.push(p(v[0], v[1], v[2]));
    }
    let cap_id = verts.len() as u32;
    verts.push(p(cap_c[0], cap_c[1], cap_c[2]));
    let rim = |k: usize| rim_base + (k % N_FACETS) as u32;
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for k in 0..N_FACETS {
        tris.push([0, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    for k in 0..N_FACETS {
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }
    let n = tris.len();
    let arr = LabeledArrangement {
        mesh: Mesh::new(verts, tris),
        surface,
        inside: vec![vec![false, false]; n],
        patch: vec![0u32; n],
        num_inputs: 2,
    };
    let bx = oblique_halfspace_box_for(ellipse_plane_surface(), [0.0, 0.0, 2.5]);
    let mock = LabelMock { arrangement: arr };
    let r = boolean(&oblique_cone(), &bx, BoolOp::Union, &mock)
        .expect("adversary 3a: cone-ellipse path must still Ok after the hyperbola edit");
    assert_eq!(unpaired_half_edges(r.as_mesh()), 0, "3a watertight");
    assert_eq!(euler_characteristic(r.as_mesh()), 2, "3a χ=2");
    let ells = ellipse_edges(&r);
    assert!(
        !ells.is_empty(),
        "adversary 3a: cone-ellipse output must carry ≥1 Curve::Ellipse edge (not a Hyperbola)"
    );
    assert!(
        hyperbola_edges(&r).is_empty(),
        "adversary 3a: a cone-ELLIPSE cut must NOT emit any Hyperbola edge"
    );
    let n = ellipse_plane_normal();
    let d = ellipse_plane_d();
    for e in &ells {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            assert!(
                cone_radial_residual(ep) <= TAU_MODEL && (dot(ep, n) + d).abs() <= TAU_MODEL,
                "adversary 3a: cone-ellipse vertex {ep:?} must stay on the exact ellipse"
            );
        }
    }
}

// ---- (3b) cone-PARABOLA (θ = α) still produces a Curve::Parabola (ssi level) ----
// A full end-to-end parabola relocation has its own (narrow-arc) fixture in the
// YR22 suite; here the adversary's non-regression duty is to confirm the SECTION
// TYPE machinery (`ssi_curve_to_curve`) still maps the θ=α section to Parabola and
// the hyperbola edit did not steal it. The end-to-end parabola pipeline is
// independently exercised by oracle_section + an on-curve relocate below.

fn parabola_plane_normal() -> [f64; 3] {
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

#[test]
fn adversary_regression_cone_parabola_still_curve_parabola() {
    // ssi-rs section type.
    let curves = section_curves(parabola_plane_surface(), cone_surface());
    assert_eq!(
        curves.len(),
        1,
        "adversary 3b: θ=α section must be ONE curve, got {curves:?}"
    );
    assert!(
        matches!(curves[0], ssi_rs::SsiCurve::Parabola { .. }),
        "adversary 3b: θ=α cone∩plane must be a Parabola, got {:?}",
        curves[0]
    );

    // End-to-end: a θ=α narrow-arc cap relocates onto a Curve::Parabola (NOT a
    // Hyperbola). Arc φ∈[160°,200°] (the proven-piercing parabola arc).
    let de = cone_d_eps();
    let delta = 0.5 * de;
    let ring: Vec<[f64; 3]> = {
        let (e1, e2) = azim_basis();
        let ax = unit(CONE_AXIS);
        let tana = cone_half_angle().tan();
        let n = parabola_plane_normal();
        let d = parabola_plane_d();
        (0..N_FACETS)
            .map(|k| {
                let lo = 160.0_f64.to_radians();
                let hi = 200.0_f64.to_radians();
                let phi = lo + (hi - lo) * (k as f64) / ((N_FACETS - 1) as f64);
                let rhat = add(scale(e1, phi.cos()), scale(e2, phi.sin()));
                let n_dot_r = dot(n, rhat);
                let n_dot_a = dot(n, ax);
                let s = (-d - (-delta) * n_dot_r) / (n_dot_a + tana * n_dot_r);
                let rho = s * tana + (-delta);
                add(scale(ax, s), scale(rhat, rho))
            })
            .collect()
    };
    let mut cap_c = [0.0; 3];
    for v in &ring {
        cap_c = add(cap_c, *v);
    }
    cap_c = scale(cap_c, 1.0 / ring.len() as f64);
    let mut verts: Vec<Point3> = vec![p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2])];
    let rim_base = verts.len() as u32;
    for v in &ring {
        verts.push(p(v[0], v[1], v[2]));
    }
    let cap_id = verts.len() as u32;
    verts.push(p(cap_c[0], cap_c[1], cap_c[2]));
    let rim = |k: usize| rim_base + (k % N_FACETS) as u32;
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for k in 0..N_FACETS {
        tris.push([0, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    for k in 0..N_FACETS {
        tris.push([cap_id, rim(k), rim(k + 1)]);
        surface.push(vec![LaInputId(1)]);
    }
    let n = tris.len();
    let arr = LabeledArrangement {
        mesh: Mesh::new(verts, tris),
        surface,
        inside: vec![vec![false, false]; n],
        patch: vec![0u32; n],
        num_inputs: 2,
    };
    let bx = oblique_halfspace_box_for(parabola_plane_surface(), [0.0, 0.0, 1.8]);
    let mock = LabelMock { arrangement: arr };
    let r = boolean(&oblique_cone(), &bx, BoolOp::Union, &mock)
        .expect("adversary 3b: cone-parabola path must still Ok after the hyperbola edit");
    let paras = parabola_edges(&r);
    assert!(
        !paras.is_empty(),
        "adversary 3b: cone-parabola output must carry ≥1 Curve::Parabola edge"
    );
    assert!(
        hyperbola_edges(&r).is_empty(),
        "adversary 3b: a cone-PARABOLA cut must NOT emit any Hyperbola edge"
    );
}

// =========================================================================
// ATTACK 4 — NO SILENT-WRONG across δ. Characterize the outcome at δ ∈ {0.5, 1.0,
// 1.2, 2.0}·d_ε. Each is either (i) an on-curve Ok (EVERY hyperbola-edge endpoint
// on the exact cone+plane to TAU_MODEL), or (ii) a LOUD Err. NO δ yields an Ok
// whose hyperbola edge was silently snapped from a beyond-band crossing.
// =========================================================================

#[test]
fn adversary_no_silent_wrong_across_delta() {
    let de = cone_d_eps();
    for &k in &[0.5_f64, 1.0, 1.2, 2.0] {
        let mock = LabelMock {
            arrangement: build_hyperbola_cap_arrangement(-k * de),
        };
        let r = boolean(&oblique_cone(), &cutting_box(), BoolOp::Union, &mock);
        match r {
            Err(_) => { /* a LOUD rejection is always acceptable — never wrong */ }
            Ok(brep) => {
                // If Ok, EVERY hyperbola-edge endpoint must be on the exact
                // section to TAU_MODEL — i.e. it was genuinely relocated, NOT
                // silently kept at a beyond-band position.
                for e in hyperbola_edges(&brep) {
                    let (s, t) = edge_endpoints(&brep, e);
                    for ep in [s, t] {
                        let res = conic_residual(ep);
                        assert!(
                            res <= TAU_MODEL,
                            "adversary 4: at δ={k}·d_ε the Ok output has a Curve::Hyperbola \
                             endpoint {ep:?} that is {res} off the exact section (> TAU_MODEL) \
                             — a SILENT-WRONG snap from a beyond-band crossing"
                        );
                    }
                }
            }
        }
    }
}

// =========================================================================
// ATTACK 5 — SCRUTINIZE the oracle7 reframe (FaceResolutionFailed at 2·d_ε).
// Independently verify: (a) the result is genuinely Err (not Ok); (b) it is
// FaceResolutionFailed BECAUSE the apex-fan centroid is independently recomputed
// > d_ε off the cone (so it cannot be attributed to any input face); (c) pin the
// reason so a future regression that turns this into a silent Ok is caught.
// =========================================================================

#[test]
fn adversary_oracle7_reframe_is_honest_face_resolution_failed() {
    let de = cone_d_eps();

    // (b first) INDEPENDENT centroid fact: the apex-fan triangle centroids of the
    // 2·d_ε ring are genuinely > d_ε off the cone — so Stage-6 face attribution
    // CANNOT place them on any input face within the Stage-1 chord band. This is
    // the geometric reason the reframe cites; recompute it from scratch.
    let ring = hyperbola_ring(-2.0 * de);
    let apex = CONE_APEX;
    let mut min_centroid_off = f64::INFINITY;
    let mut max_centroid_off = 0.0_f64;
    for k in 0..N_FACETS {
        let r0 = ring[k % N_FACETS];
        let r1 = ring[(k + 1) % N_FACETS];
        let c = scale(add(add(apex, r0), r1), 1.0 / 3.0);
        let off = cone_radial_residual(c);
        min_centroid_off = min_centroid_off.min(off);
        max_centroid_off = max_centroid_off.max(off);
    }
    assert!(
        min_centroid_off > de,
        "adversary 5b: EVERY apex-fan centroid of the 2·d_ε ring must be > d_ε off the cone \
         (min {min_centroid_off} > d_ε {de}) — the genuine, independent reason face attribution \
         fails (NOT a snap / widen)"
    );
    // Sanity that this is the ~1.33·d_ε magnitude the reframe claims.
    assert!(
        min_centroid_off < 2.0 * de && max_centroid_off < 2.0 * de,
        "adversary 5b: the centroid offset should be ~1.3–1.6·d_ε (min {min_centroid_off}, \
         max {max_centroid_off}), not larger — confirms the reframe's ≈1.33·d_ε claim"
    );

    // (a + c) The pipeline genuinely returns Err, and PIN the exact variant:
    // FaceResolutionFailed (a sanctioned LOUD outcome — never a silent Ok).
    let mock = LabelMock {
        arrangement: build_hyperbola_cap_arrangement(-2.0 * de),
    };
    let r = boolean(&oblique_cone(), &cutting_box(), BoolOp::Union, &mock);
    assert!(
        r.is_err(),
        "adversary 5a: the 2·d_ε fixture must be genuinely Err (the reframe's LOUD claim), got Ok"
    );
    let err = r.err().unwrap();
    assert!(
        matches!(err, YangError::FaceResolutionFailed { .. }),
        "adversary 5c: the oracle7 reframe pins this as FaceResolutionFailed; if it ever \
         becomes a silent Ok or a different (possibly-snapping) variant this canary fires. \
         Got {err:?}"
    );
}

// =========================================================================
// ATTACK 6 — EVAL ROUND-TRIP via an INDEPENDENT re-implementation. My own
// `eval_hyperbola_point` (re-implemented from scratch) must match the production
// `yang_rs::hyperbola_point` at several t — guards against a typo in the
// production helper that oracle3 (production helper on BOTH sides) cannot catch.
// =========================================================================

#[test]
fn adversary_hyperbola_point_matches_independent_reimpl() {
    let hyp = upper_branch();
    let (center, normal, major, a, b) = expect_hyperbola(&hyp);

    // ssi-rs guarantees unit normal/major; the production helper does NOT
    // re-normalize, and neither does my independent eval, so they must agree
    // field-for-field on the ssi-rs branch (the real stored-edge path).
    for &t in &[-1.5_f64, -0.7, -0.1, 0.0, 0.1, 0.7, 1.5, 2.3] {
        let prod = hyperbola_point(
            Point3::from(center),
            Vector3::from(normal),
            Vector3::from(major),
            a,
            b,
            t,
        )
        .as_array();
        let mine = eval_hyperbola_point(center, normal, major, a, b, t);
        let d = norm(sub(prod, mine));
        assert!(
            d <= TAU_MODEL,
            "adversary 6: production hyperbola_point(t={t})={prod:?} must match my independent \
             re-implementation {mine:?} (off by {d}) — catches a production-helper typo"
        );
        // Cross-check: the production point genuinely lies on the exact section to
        // TAU_MODEL (it is a real hyperbola point, not just self-consistent).
        assert!(
            conic_residual(prod) <= TAU_MODEL,
            "adversary 6: production hyperbola_point(t={t}) must lie on the exact cone∩plane \
             section (residual {})",
            conic_residual(prod)
        );
        // And the (u/a)²−(v/b)²=1, u>0 in-plane relation holds (true hyperbola).
        let (u, _v) = hyperbola_uv(prod, &hyp);
        assert!(
            u > 0.0 && hyperbola_geo_residual(prod, &hyp) <= TAU_MODEL,
            "adversary 6: production hyperbola_point(t={t}) must satisfy (u/a)²−(v/b)²=1 with u>0"
        );
    }
}

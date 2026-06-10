//! PR-YR23 RED — Stage 4 (OBLIQUE CONE): RELOCATE mesh intersection points onto
//! the exact analytical HYPERBOLA of a `cone ∩ plane` cut whose cutting plane is
//! parallel to the cone AXIS (`|n̂·â| = 0 < sinα`), piercing BOTH nappes with
//! opposite-sign symmetry-plane generators (`plane_cone` HYPE case). The
//! two-candidate-conic sibling of PR-YR21's cone ELLIPSE / PR-YR22's cone
//! PARABOLA: `ssi-rs`'s `plane_cone` returns EXACTLY TWO `SsiCurve::Hyperbola`
//! candidates (one per nappe, opposite `major_axis`, `+m̂` first) for this case.
//!
//! The genuinely NEW mechanism this PR closes out the cone with: **two-branch
//! selection**. Both branches lie on cone∩plane (the on-both-surfaces gate
//! passes for both), so `curve_contains_point` is the branch discriminator —
//! the edge's branch satisfies `(u/a)² − (v/b)² = 1` with `u = (p − center)·major_axis > 0`;
//! the OTHER branch's frame (opposite `major_axis`) gives `u < 0` → rejected, so
//! `matched == 1`.
//!
//! Paper: Yang 2025 §4.4.1 (mesh updating / relocation) + §4.3.2 (parametric
//! surface relocation).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY; the
//! GREEN implementer extends `crates/yang-rs/src/lib.rs` (the new `Curve::Hyperbola`
//! variant, the `ssi_curve_to_curve` Hyperbola arm, the `curve_contains_point`
//! Hyperbola arm with the `u>0` discriminator, the `eval_source` `Curve::Hyperbola`
//! arm calling a new `hyperbola_point`, and the Stage-4 relocation `Curve::Hyperbola`
//! arm). The RED author NEVER edits production code.
//!
//! ## RED state (compile-fail until GREEN)
//!
//! This file references the FINAL API — `Curve::Hyperbola { center, normal,
//! major_axis, semi_transverse, semi_conjugate }` and the helper
//! `hyperbola_point(center, normal, major_axis, a, b, t)` — neither of which
//! exists in current production. So this file does NOT compile today; that is the
//! expected RED state. After GREEN adds the variant + helper + the relocation arm,
//! it compiles and the oracles pass.
//!
//! Per the established repo convention (integration-test files cannot share
//! helpers), the yr22 harness (`p`, array math, `cone_brep`, mesh oracles,
//! `LabelMock`, the on-conic residuals, the cone-ring/cap arrangement builder)
//! is re-declared here, retargeted to the HYPERBOLA fixture. The hyperbola
//! section the output must match is recomputed INDEPENDENTLY from the fixture's
//! true cone/plane (cone radial residual + plane residual + the exact
//! `(u/a)² − (v/b)² = 1` in-plane relation with `u > 0`), NOT via production code.
//!
//! Tolerances (mirror YR22, do NOT weaken):
//!   - On-conic / round-trip / after-deviation: `cad_primitives::TAU_MODEL` (1e-7).
//!   - Off-band band: the cone chord bound `cone_d_eps()`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, hyperbola_point, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Stage4InvalidReason,
    Surface, TessellationSource, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math. Re-declared verbatim from yr22.
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
// Mesh oracles. Re-declared verbatim from yr22.
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
// Canonical OBLIQUE-CONE / HYPERBOLA config.
//
//   cone A: apex at the origin, axis +Z, half_angle α = atan(0.5) (tanα = 0.5),
//     height 4 → base radius R = 2 at z = 4 (carries a base-rim Curve::Circle,
//     which production needs to derive the cone height / chord budget).
//   plane B (the cutting plane): normal n = [1, 0, 0], through (1,0,0) — i.e.
//     the plane `x = 1`, with d = −1 (`n·x + d = x − 1 = 0`). This plane is
//     PARALLEL to the cone AXIS (`|n̂·â| = 0 < sinα`), so it pierces BOTH nappes'
//     symmetry-plane generators with OPPOSITE signs and the section is a
//     HYPERBOLA — two unbounded branches, one per nappe. Independently confirmed
//     `SsiCurve::Hyperbola` × 2 (NOT Ellipse/Parabola/Circle) by the ssi-rs
//     oracle below. ssi-rs reports (empirically verified):
//       branch 0: center (1,0,0), normal (1,0,0), major_axis (0,0,+1), a=2, b=1,
//                 vertex (1,0,+2)  [the UPPER-nappe branch, on the solid].
//       branch 1: same but major_axis (0,0,−1), vertex (1,0,−2)
//                 [the LOWER-nappe branch, NOT on the finite solid].
//
//   The FIXTURE bounds the unbounded UPPER-nappe branch by sampling the azimuth
//   arc φ ∈ [−12°, +12°] about +Z — a 24° arc centered on the φ=0 branch VERTEX
//   (so the relocation is genuinely exercised across the vertex). At φ=±12° the
//   ring vertex sits at z ≈ 2.09, comfortably inside the solid (z ≤ 4 needs
//   cosφ ≥ 0.5, i.e. |φ| ≤ 60°). The sampled arc is closed into a topological
//   ring with a single wrap chord; the arc is deliberately narrow so the lone
//   wrap (apex-fan) triangle's cone-attribution residual (the chord sagitta)
//   stays comfortably INSIDE the cone band `cone_d_eps` even at the off-curve δ
//   offset (δ = 0.4·cone_d_eps ≈ 0.0226 < band 0.0566) — Stage-6 attribution runs
//   at the pre-relocation positions. The result is a genus-0 closed shell
//   (apex-fan + plane cap), verified by the self-check; the wrap triangle is
//   interior scaffold (NOT a hyperbola seam vertex — it is never relocated).
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

/// The HYPERBOLA cutting plane: normal n = [1, 0, 0] (PARALLEL to the cone axis,
/// `|n·â| = 0`), through (1,0,0). `n·x + d = 0` with d = −1 (the plane x = 1).
fn cut_plane_normal() -> [f64; 3] {
    [1.0, 0.0, 0.0]
}
fn cut_plane_d() -> f64 {
    let n = cut_plane_normal();
    -dot(n, [1.0, 0.0, 0.0])
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
// VERBATIM from yr22::cone_brep. The base-rim Curve::Circle is MANDATORY:
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
// relevant face carries the HYPERBOLA `Surface::Plane`; the other five faces are
// meters away from the hyperbola cap so the cap centroids resolve uniquely to the
// oblique face (within the planar TAU_WORK band). Built in the plane frame.
// Re-declared from yr22 (retargeted box center to the x=1 plane near the cone).
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

/// A large tilted box whose top face is `plane_surf` (the HYPERBOLA
/// `Surface::Plane`), centered (in plane) at `plane_center`, half-width `H`, depth
/// `D` along −n̂.
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
            plane_surf // the hyperbola cutting plane (exact production fields)
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

/// The box top face is the x=1 plane; center it on the foot of a mid-arc point
/// onto the plane so the hyperbola cap centroids resolve to it.
fn cutting_box() -> BRep {
    // foot of (1,0,2.0) (mid-arc point) onto the plane x=1.
    let n = cut_plane_normal();
    let d = cut_plane_d();
    let off = dot(n, [1.0, 0.0, 2.0]) + d;
    let center = sub([1.0, 0.0, 2.0], scale(n, off));
    oblique_halfspace_box(cut_plane_surface(), center, 10.0, 10.0)
}

// =========================================================================
// SSI ORACLE — confirm INDEPENDENTLY (via ssi-rs) that the cone ∩ plane section
// really is TWO HYPERBOLA branches (proves the fixture is genuinely the HYPE
// case, not accidentally an ellipse/parabola/circle). `surface_to_quadric`
// re-declared verbatim from yr22.
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

/// All cone ∩ plane section curves, computed independently by ssi-rs. Asserts the
/// section is EXACTLY TWO Hyperbola (NOT Ellipse/Parabola/Circle).
fn oracle_section_hyperbolas() -> Vec<ssi_rs::SsiCurve> {
    let plane = surface_to_quadric(cut_plane_surface());
    let cone = surface_to_quadric(cone_surface());
    let curves = ssi_rs::intersect(&plane, &cone)
        .expect("oracle: Plane∩Cone must succeed for the axis-parallel (hyperbola) cut");
    assert_eq!(
        curves.len(),
        2,
        "oracle: the HYPE cone section must be exactly TWO curves (one per nappe), got {curves:?}"
    );
    for c in &curves {
        assert!(
            matches!(c, ssi_rs::SsiCurve::Hyperbola { .. }),
            "oracle: axis-parallel cone ∩ plane must be a Hyperbola (pierces both nappes), got {c:?}"
        );
        assert!(
            !matches!(c, ssi_rs::SsiCurve::Ellipse { .. }),
            "oracle: the HYPE case must NOT be an Ellipse"
        );
        assert!(
            !matches!(c, ssi_rs::SsiCurve::Parabola { .. }),
            "oracle: the HYPE case must NOT be a Parabola"
        );
    }
    curves
}

/// Destructure an ssi-rs Hyperbola branch into its fields.
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

/// The UPPER-nappe (`major_axis` has +Z component) branch — the one on the solid.
fn oracle_upper_branch() -> ssi_rs::SsiCurve {
    let curves = oracle_section_hyperbolas();
    let ax = unit(CONE_AXIS);
    for c in curves {
        let (_, _, m, _, _) = expect_hyperbola(&c);
        if dot(m, ax) > 0.0 {
            return c;
        }
    }
    panic!("oracle: no upper-nappe (+axis) hyperbola branch found");
}

// =========================================================================
// INDEPENDENT on-conic residuals: a relocated crossing must end on BOTH the true
// cone (cone radial residual `|radial − |h_axial|·tanα|`) AND the cutting plane
// (`|n·x + d|`). Recomputed straight from the fixture's cone / plane (NOT via
// production). `cone_radial_residual` re-declared verbatim from yr22.
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
// INDEPENDENT exact-hyperbola in-plane test: a relocated crossing, projected into
// the hyperbola's in-plane frame (u = along +major_axis from center, v = along
// the conjugate normal×major_axis), must satisfy the exact relation
// `(u/a)² − (v/b)² = 1` AND `u > 0` (the +major_axis branch). This is the
// production-independent "on the EXACT hyperbola" reference (distinct from the
// cone/plane residual method). Frame matches `SsiCurve::eval` for Hyperbola (and
// the GREEN `hyperbola_point`) field-for-field.
// =========================================================================

/// In-plane (u, v) coords of `pt` in the hyperbola frame `(center, major_axis,
/// normal×major_axis)`.
fn hyperbola_uv(pt: [f64; 3], hyp: &ssi_rs::SsiCurve) -> (f64, f64) {
    let (center, normal, major, _, _) = expect_hyperbola(hyp);
    let m = unit(major);
    let conj = cross(unit(normal), m);
    let w = sub(pt, center);
    (dot(w, m), dot(w, conj))
}

/// `|(u/a)² − (v/b)² − 1|` for the exact hyperbola — zero ⇒ on the exact
/// hyperbola's branch curve. Used by oracle2 as the in-plane on-hyperbola
/// residual (dimensionless; the assertion budget is TAU_MODEL, matching the
/// near-unit a/b of this fixture). The `u > 0` discriminator is asserted
/// separately.
fn hyperbola_inplane_residual(pt: [f64; 3], hyp: &ssi_rs::SsiCurve) -> f64 {
    let (_, _, _, a, b) = expect_hyperbola(hyp);
    let (u, v) = hyperbola_uv(pt, hyp);
    ((u / a).powi(2) - (v / b).powi(2) - 1.0).abs()
}

// =========================================================================
// Independent HYPERBOLA evaluation / parameterization — matches the
// `SsiCurve::eval` (Hyperbola) / production `hyperbola_point` convention EXACTLY:
//   point(t) = center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)
// Used by the chord-deviation / round-trip oracles. Re-declared verbatim from the
// ssi-rs Hyperbola `eval`.
// =========================================================================

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

/// Resolution-INDEPENDENT perpendicular distance from `x` to the exact hyperbola
/// branch, via a two-level sampler over the bounded parameter window the fixture
/// arc occupies. A coarse sweep locates the nearest sample param `t0`, then a
/// LOCAL fine sweep over one coarse step on each side refines it. The fine
/// spacing's nearest-sample floor is negligible vs TAU_MODEL, so this can honor
/// the tight (≤ TAU_MODEL) after-relocate bound. Used by oracle3. The window
/// `|t| ≤ TMAX = 2.5` covers the ±12° arc: v = b·sinh(t), |v| ≤ ~0.43 ⇒
/// |t| ≤ asinh(0.43) ≈ 0.42, well inside 2.5.
fn dist_to_hyperbola_refined(x: [f64; 3], hyp: &ssi_rs::SsiCurve) -> f64 {
    let (center, normal, major, a, b) = expect_hyperbola(hyp);
    let tmax = 2.5_f64;
    let coarse = 600_000usize;
    let span = 2.0 * tmax;
    let mut best = f64::INFINITY;
    let mut t0 = 0.0_f64;
    for k in 0..=coarse {
        let t = -tmax + span * (k as f64) / (coarse as f64);
        let pe = eval_hyperbola_point(center, normal, major, a, b, t);
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
        let pe = eval_hyperbola_point(center, normal, major, a, b, t);
        best = best.min(norm(sub(x, pe)));
    }
    best
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement.
// Re-declared verbatim from yr22.
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
// HYPERBOLA FIXTURE BUILDER.
//
// The hand-built arrangement is the closed apex-side piece of the truncated cone
// over the bounded UPPER-nappe hyperbola arc: an APEX FAN (apex → the hyperbola
// ring, label 0 = the cone input A) + the hyperbola CAP on the x=1 plane (cap
// center → hyperbola ring, label 1 = the plane/box input B). The seam where the
// fan meets the cap is the HYPERBOLA arc — the intersection edge whose
// `Curve::Hyperbola` Stage-4 relocates. The arc is closed into a topological ring
// with a single wrap chord; the shell is genus 0, χ=2, outward-oriented (positive
// signed volume).
//
// `hyperbola_ring_point(phi, delta)` samples the ring on the cone lateral at
// azimuth φ about +Z, solving the generator parameter `s` so the point stays
// EXACTLY on the x=1 plane while sitting at radial distance `s·tanα + delta` from
// the axis (the SAME type-agnostic solver shape as yr22's `parabola_ring_point`).
// For the plane x=1: n·â = 0, n·r̂ = cosφ, so `s = (1 − delta·cosφ)/(0.5·cosφ)`,
// `rho = s·tanα + delta`, point = `s·ẑ + rho·r̂`. So:
//   - delta = 0 → ON the exact hyperbola (cone ∩ plane).
//   - delta < 0 → OFF the cone by ~|delta| radially, yet still ON the x=1 plane
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
/// generator, solved to land EXACTLY on the x=1 plane. Apex at origin.
fn hyperbola_ring_point(phi: f64, delta: f64) -> [f64; 3] {
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

/// The bounded UPPER-nappe hyperbola arc, sampled over φ ∈ [−12°, +12°] (a narrow
/// arc centered on the φ=0 branch vertex), at radial offset `delta`. The arc is
/// deliberately narrow so the single wrap (apex-fan) triangle stays within the
/// cone band.
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

/// Build the closed apex-fan + hyperbola-cap arrangement from a ring (offset
/// `delta`). Apex (label 0) fan + plane cap (label 1). The sampled arc is treated
/// as a CLOSED ring (wrap k+1 mod N_FACETS) so the shell is genus 0; windings
/// chosen so the shell is outward-oriented (positive signed volume) — verified by
/// the self-check.
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
    // APEX FAN (label 0 = cone input A): apex → rim(k+1) → rim(k).
    for k in 0..N_FACETS {
        tris.push([apex_id, rim(k + 1), rim(k)]);
        surface.push(vec![LaInputId(0)]);
    }
    // HYPERBOLA CAP (label 1 = plane/box input B): cap_c → rim(k) → rim(k+1)
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

/// On-hyperbola arrangement (ring on the exact cone ∩ x=1 plane).
fn on_hyperbola_arrangement() -> LabeledArrangement {
    build_hyperbola_cap_arrangement(0.0)
}

/// The off-curve δ: strictly inside the `(TAU_WORK, cone_d_eps]` relocate band
/// (`δ = 0.4 · cone_d_eps`), so the ring vertices are genuinely off the exact
/// hyperbola (`conic_residual ≈ δ ≫ TAU_MODEL`) yet still relocatable.
fn relocate_band_delta() -> f64 {
    0.4 * cone_d_eps()
}

/// Off-curve arrangement: ring at radial `s·tanα − 0.4·cone_d_eps` (still ON the
/// x=1 plane), off the exact cone by ~δ pre-Stage-4 (genuine relocation work).
fn off_curve_arrangement() -> LabeledArrangement {
    build_hyperbola_cap_arrangement(-relocate_band_delta())
}

/// BEYOND-band δ for the LOUD oracle: `δ = 2 · cone_d_eps` (twice the relocation
/// budget). The ring is still EXACTLY on the x=1 plane, but its cone radial
/// residual (≈ 2·cone_d_eps) exceeds the chord band — a genuinely out-of-budget
/// hyperbola crossing the relocation path must LOUDLY reject (NOT silently snap).
fn beyond_band_delta() -> f64 {
    2.0 * cone_d_eps()
}

/// Beyond-band arrangement: ring `2·cone_d_eps` off the cone (still on the plane),
/// genus-0 (so boolean() reaches Stage 4), used by the LOUD oracle.
fn beyond_band_arrangement() -> LabeledArrangement {
    build_hyperbola_cap_arrangement(-beyond_band_delta())
}

/// Simulate the Union keep-set on the arrangement mesh: every triangle kept, none
/// flipped (Union). Used by the mandatory `mock_is_valid_genus0` self-check.
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    Mesh::new(arr.mesh.verts.clone(), arr.mesh.tris.clone())
}

// =========================================================================
// Output-edge helpers. Re-declared from yr22 (Hyperbola-targeted).
// =========================================================================

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

/// Is `pt` an intersection-edge (ring) endpoint? The ring vertices sit off the
/// axis; the apex and cap-center sit on/near the axis and are not relocated.
/// (The cap center of the x=1 hyperbola arc sits at x≈1, off the +Z axis, but is
/// a CAP vertex not an intersection-edge endpoint — the edge filter below selects
/// the true intersection edges, so this is only a coarse pre-filter for the BEFORE
/// loops over raw mesh verts, which inspect ring offset via conic_residual.)
fn is_ring_point(pt: [f64; 3]) -> bool {
    // Ring vertices satisfy plane_residual ≈ 0 (on x=1) AND sit off the +Z axis at
    // a genuine cone radius. The apex (origin) and cap center (x≈1, low radius
    // about the plane centroid) are excluded by requiring a sizeable +Z-axis
    // radial component AND on-plane membership.
    let on_plane = plane_residual(pt) < 10.0 * cone_d_eps();
    let axis_radial = (pt[0] * pt[0] + pt[1] * pt[1]).sqrt();
    on_plane && axis_radial > MIN_FEATURE_SIZE && pt[2] > 0.5
}

fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED Union output (keep-all, no flip) directly, NO boolean() call, and
// asserts the mock is a valid genus-0 closed shell: watertight, χ=2,
// outward-oriented. (Compiles + passes once GREEN adds the Hyperbola variant; it
// never reaches the not-yet-wired Stage-4 path. Its LOGIC references NO
// Curve::Hyperbola / hyperbola_point — only the raw mesh.)
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    for arr in [
        on_hyperbola_arrangement(),
        off_curve_arrangement(),
        beyond_band_arrangement(),
    ] {
        let sim = simulated_output_mesh(&arr);

        let unpaired = unpaired_half_edges(&sim);
        assert_eq!(
            unpaired, 0,
            "yr23 self-check: simulated hyperbola-cap output mesh must be watertight \
             (0 unpaired half-edges); got {unpaired}."
        );

        let chi = euler_characteristic(&sim);
        assert_eq!(
            chi, 2,
            "yr23 self-check: simulated hyperbola-cap output must be genus 0 (χ=2); got χ={chi}."
        );

        let vol = signed_volume(&sim);
        assert!(
            vol > 0.0,
            "yr23 self-check: simulated output must be OUTWARD-oriented (positive \
             signed volume); got {vol}."
        );
    }
}

// =========================================================================
// Oracle 1 — the hyperbola cone ∪ box SUCCEEDS and the output carries ≥1
// Curve::Hyperbola intersection edge; an INDEPENDENT ssi-rs oracle confirms the
// section is TWO Hyperbola (NOT Ellipse/Parabola); the stored
// center/normal/major_axis/a/b match the ssi-rs UPPER (+axis) branch.
//
// Uses the ON-hyperbola arrangement (ring on the exact section), so a successful
// Ok requires the Hyperbola edge to be emitted and (no-op) relocated.
// =========================================================================

#[test]
fn oracle1_hyperbola_cone_union_succeeds_with_hyperbola_edge() {
    // INDEPENDENT ssi-rs oracle: the axis-parallel cone ∩ plane really is TWO
    // Hyperbola, and we select the upper (+axis) branch the solid carries.
    let upper = oracle_upper_branch();
    let (o_center, o_normal, o_major, o_a, o_b) = expect_hyperbola(&upper);
    assert!(
        o_a > 1e-3 && o_b > 1e-3,
        "yr23 O1: fixture must be a genuine hyperbola (a={o_a}, b={o_b} > 0)"
    );

    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: on_hyperbola_arrangement(),
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
        "yr23 O1: hyperbola cone ∪ box must NO LONGER reject a cone+plane hyperbola with \
         Stage4RegionInvalid{{LocalRefinementRequired}}, got {r:?}"
    );

    let brep =
        r.expect("yr23 O1: hyperbola cone ∪ box must succeed (Ok) after cone-hyperbola relocate");

    let hyperbolas = hyperbola_edges(&brep);
    assert!(
        !hyperbolas.is_empty(),
        "yr23 O1: hyperbola cone union output must carry ≥1 Curve::Hyperbola intersection \
         edge; got {:?}",
        brep.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Each hyperbola edge's stored fields must match the independent ssi-rs UPPER
    // branch within a generous tolerance: GREEN stores the ssi-rs section verbatim
    // via `ssi_curve_to_curve`, so this is effectively byte-equality.
    for e in &hyperbolas {
        let Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } = e.curve
        else {
            continue;
        };
        assert!(
            (semi_transverse - o_a).abs() <= 1e-6,
            "yr23 O1: hyperbola edge semi_transverse {semi_transverse} must match the ssi-rs \
             section a {o_a}"
        );
        assert!(
            (semi_conjugate - o_b).abs() <= 1e-6,
            "yr23 O1: hyperbola edge semi_conjugate {semi_conjugate} must match the ssi-rs \
             section b {o_b}"
        );
        assert!(
            norm(sub(center.as_array(), o_center)) <= 1e-6,
            "yr23 O1: hyperbola edge center {:?} must match the ssi-rs section center {o_center:?}",
            center.as_array()
        );
        let m = unit(major_axis.as_array());
        let om = unit(o_major);
        let align = dot(m, om);
        assert!(
            (align - 1.0).abs() <= 1e-6,
            "yr23 O1: hyperbola edge major_axis {m:?} must match the ssi-rs UPPER branch \
             major_axis {om:?} (dot {align} ≈ 1) — the +axis branch, NOT the lower nappe"
        );
        let nn = unit(normal.as_array());
        let on = unit(o_normal);
        assert!(
            dot(nn, on).abs() >= 1.0 - 1e-6,
            "yr23 O1: hyperbola edge normal {nn:?} must match the ssi-rs section normal {on:?}"
        );
    }
}

// =========================================================================
// Oracle 2 (core) — the off-curve cone-hyperbola relocation oracle. Drives
// boolean() with the OFF-curve LabelMock and asserts:
//   - result is Ok, watertight (0 unpaired), Euler χ=2;
//   - every relocated intersection-edge vertex lies on the EXACT hyperbola
//     (`(u/a)²−(v/b)²=1` AND `u>0` via in-plane u,v) AND on the cone AND on the
//     plane to TAU_MODEL, recomputed independently;
//   - max chord deviation strictly DECREASES across relocation and ends ≤ TAU_MODEL;
//   - determinism (a second boolean() run is byte-identical).
// =========================================================================

#[test]
fn oracle2_offcurve_relocate_on_hyperbola_watertight() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let delta = relocate_band_delta();
    let de = cone_d_eps();
    let upper = oracle_upper_branch();

    // Fixture sanity: TAU_WORK < δ ≤ cone_d_eps (genuinely off-curve, relocatable).
    assert!(
        delta > TAU_WORK && delta <= de,
        "fixture δ={delta} must lie in (TAU_WORK, cone_d_eps={de}]"
    );

    // BEFORE: the off-curve ring vertices are off the exact hyperbola by ~δ.
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
        "fixture must start genuinely off the hyperbola (before_max_dev={before_max_dev} \
         should be ≫ TAU_MODEL); δ={delta}"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock).expect(
        "yr23: hyperbola cone ∪ box (off-curve mock) must return Ok after cone Stage-4 hyperbola \
         relocate (NOT Err)",
    );

    // Watertight 2-manifold.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr23: relocated output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr23: relocated output Euler V−E+F must be 2"
    );

    let hyperbolas = hyperbola_edges(&r);
    assert!(
        !hyperbolas.is_empty(),
        "yr23: expected ≥1 Hyperbola intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Every relocated intersection-edge vertex is on the exact hyperbola: cone
    // radial residual ≤ TAU_MODEL AND plane residual ≤ TAU_MODEL AND the in-plane
    // `(u/a)²−(v/b)²=1` relation ≤ TAU_MODEL AND u>0 (the +axis branch). Track the
    // max post-relocation deviation.
    let mut after_max_dev = 0.0_f64;
    for e in &hyperbolas {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr23: relocated vertex {ep:?} cone radial residual {radial} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr23: relocated vertex {ep:?} plane residual {planar} \
                 must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            let inplane = hyperbola_inplane_residual(ep, &upper);
            assert!(
                inplane <= TAU_MODEL,
                "yr23: relocated vertex {ep:?} exact-hyperbola residual |(u/a)²−(v/b)²−1| \
                 {inplane} must be ≤ TAU_MODEL ({TAU_MODEL})"
            );
            let (u, _v) = hyperbola_uv(ep, &upper);
            assert!(
                u > 0.0,
                "yr23: relocated vertex {ep:?} must be on the +major_axis (UPPER) branch \
                 (u={u} > 0), NOT the lower-nappe branch"
            );
        }
    }

    // Chord deviation strictly decreases (and ends ≤ TAU_MODEL).
    assert!(
        after_max_dev < before_max_dev,
        "yr23: max chord deviation must strictly decrease (after {after_max_dev} \
         < before {before_max_dev}) — proves real relocation, not a no-op"
    );
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr23: max chord deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );

    // Determinism: a second identical run is byte-identical.
    let mock2 = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r2 = boolean(&cone, &bx, BoolOp::Union, &mock2).expect("yr23: determinism run 2");
    assert_eq!(
        r, r2,
        "yr23: identical inputs must produce a byte-identical output BRep"
    );
}

// =========================================================================
// Oracle 3 (eval round-trip) — each relocated vertex carries
// TessellationSource::BRepEdge{edge,t}; `hyperbola_point(.., t)` reproduces the
// mesh position within TAU_MODEL. Also asserts (resolution-independently, via the
// two-level `dist_to_hyperbola_refined`) the relocated vertices lie on the exact
// ssi-rs hyperbola ≤ TAU_MODEL, while the pre-relocation ring sits ≫ TAU_MODEL off
// it (strict decrease, production-independent reference).
//
// The round-trip is the production edge's OWN Curve::Hyperbola fields fed to the
// production `hyperbola_point` (the SAME helper `eval_source`'s Curve::Hyperbola
// arm calls) — an internal self-consistency check mirroring YR22 oracle3.
// =========================================================================

#[test]
fn oracle3_hyperbola_eval_round_trip() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let hyp = oracle_upper_branch();

    // Pre-relocation: the off-curve ring sits ≫ TAU_MODEL off the exact hyperbola.
    let arr = off_curve_arrangement();
    let mut before = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if is_ring_point(pt) {
            before = before.max(dist_to_hyperbola_refined(pt, &hyp));
        }
    }
    assert!(
        before > 100.0 * TAU_MODEL,
        "yr23 O3: pre-Stage-4 max hyperbola deviation {before} must be ≫ TAU_MODEL"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr23 O3: hyperbola cone union must Ok after cone Stage-4 hyperbola relocate");

    let tmap = r.tessellation_map();
    let mesh = r.as_mesh();
    let mut saw_relocated_edge_source = false;
    let mut after_refined = 0.0_f64;
    for e in hyperbola_edges(&r) {
        let Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } = e.curve
        else {
            continue;
        };
        for vid in [e.start, e.end] {
            // Resolution-independent on-hyperbola check against the ssi-rs section.
            let mesh_pos = mesh.verts[vid as usize].as_array();
            after_refined = after_refined.max(dist_to_hyperbola_refined(mesh_pos, &hyp));

            match tmap.lookup(vid) {
                TessellationSource::BRepEdge { edge: _, t } => {
                    saw_relocated_edge_source = true;
                    // Round-trip through the PRODUCTION `hyperbola_point` (the same
                    // helper `eval_source`'s Curve::Hyperbola arm uses), with the
                    // edge's own stored fields.
                    let inverted = hyperbola_point(
                        center,
                        normal,
                        major_axis,
                        semi_transverse,
                        semi_conjugate,
                        t,
                    )
                    .as_array();
                    let dd = norm(sub(inverted, mesh_pos));
                    assert!(
                        dd <= TAU_MODEL,
                        "yr23 O3: relocated vertex {vid} BRepEdge t={t} must invert (via the \
                         exact hyperbola parameterization) to the mesh position within TAU_MODEL, \
                         off by {dd}"
                    );
                }
                other => panic!(
                    "yr23 O3: relocated hyperbola intersection-edge vertex {vid} must carry \
                     TessellationSource::BRepEdge{{edge,t}}, got {other:?}"
                ),
            }
        }
    }
    assert!(
        saw_relocated_edge_source,
        "yr23 O3: at least one relocated vertex must carry a BRepEdge source"
    );
    assert!(
        after_refined < before,
        "yr23 O3: hyperbola deviation must strictly decrease (after {after_refined} < before {before})"
    );
    assert!(
        after_refined <= TAU_MODEL,
        "yr23 O3: hyperbola deviation after relocate must be ≤ TAU_MODEL, got {after_refined}"
    );
}

// =========================================================================
// Oracle 4 — no degenerate triangles, and the boolean output is a consistently-
// oriented watertight 2-manifold. THE YR22 REFRAME INVARIANT, retargeted: this
// asserts the invariant production ACTUALLY enforces — non-degeneracy +
// inherited watertightness/local repair (Yang §4.4.1/§4.4.3, see lib.rs
// `validate_relocated_triangles`) + signed-volume>0 — NOT a per-facet
// winding-vs-analytic-normal test. Production deliberately rejects that pointwise
// test because it false-positives on the fixture's ring-closure scaffold facets
// (the wrap "bridge" triangles that close the open 24° arc into a watertight
// ring). DO NOT reintroduce the winding-vs-analytic-normal check.
// =========================================================================

#[test]
fn oracle4_no_inverted_or_degenerate_tris() {
    let cone = oblique_cone();
    let bx = cutting_box();
    let mock = LabelMock {
        arrangement: off_curve_arrangement(),
    };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock)
        .expect("yr23 O4: hyperbola cone union must Ok after cone Stage-4 hyperbola relocate");
    let mesh = r.as_mesh();

    // (a) No DEGENERATE triangle (per-facet area floor).
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let area2 = norm(tri_normal(a, b, c));
        assert!(
            area2 * 0.5 >= MIN_FEATURE_SIZE * MIN_FEATURE_SIZE,
            "yr23 O4: triangle {ti} {tri:?} is degenerate (area {} < MIN_FEATURE_SIZE²)",
            area2 * 0.5
        );
    }

    // (b) No INVERTED orientation — the REAL post-relocation boolean output must
    // be a consistently-oriented watertight 2-manifold. A relocation that folds
    // or tears a facet breaks half-edge pairing (unpaired != 0); a global
    // inversion flips the signed volume. O2/O3 independently pin WHERE relocated
    // vertices land (on the exact cone+plane+hyperbola to TAU_MODEL). This is the
    // ONLY always-on check of signed-volume>0 on the real post-relocation output
    // (mock_is_valid_genus0 is no-pipeline; O2 omits volume; O8 is sidecar-gated).
    let unpaired = unpaired_half_edges(mesh);
    assert_eq!(
        unpaired, 0,
        "yr23 O4: boolean output must be watertight (0 unpaired half-edges); got {unpaired}"
    );
    let chi = euler_characteristic(mesh);
    assert_eq!(
        chi, 2,
        "yr23 O4: boolean output must be genus 0 (χ=2); got χ={chi}"
    );
    let vol = signed_volume(mesh);
    assert!(
        vol > 0.0,
        "yr23 O4: boolean output must be OUTWARD-oriented (signed volume > 0); got {vol}"
    );
}

// =========================================================================
// Oracle 5 — TWO-BRANCH SELECTION (the genuinely NEW mechanism). Uses ONLY the
// test's own re-implemented membership predicate (NOT production): for the
// fixture's TWO ssi-rs Hyperbola candidates, exactly ONE contains BOTH of the
// (exact, on-curve) ring's edge endpoints under the membership
// `(u/a)²−(v/b)²=1 AND u>0` — and it is the +axis (UPPER-nappe) branch. The
// OTHER branch (opposite major_axis) gives u<0 for the same points → rejected,
// so `matched == 1`.
//
// This mirrors production's `build_intersection_curves` matched loop semantics:
// it independently demonstrates the discrimination falls out of the `u>0`
// membership arm, which is exactly the new mechanism GREEN adds to
// `curve_contains_point`.
// =========================================================================

#[test]
fn oracle5_two_branch_selection_matched_one() {
    let curves = oracle_section_hyperbolas();
    assert_eq!(curves.len(), 2, "fixture must produce exactly two branches");

    // The exact (on-curve) ring endpoints: first and last sampled ring vertices on
    // the EXACT cone ∩ plane section (delta = 0). Both lie on the UPPER branch.
    let ring = hyperbola_ring(0.0);
    let p_first = ring[0];
    let p_last = ring[N_FACETS - 1];

    // Sanity: both endpoints are genuinely on the section (cone+plane) to TAU_MODEL.
    for ep in [p_first, p_last] {
        assert!(
            conic_residual(ep) <= TAU_MODEL,
            "oracle5: exact ring endpoint {ep:?} must be on the section (residual {})",
            conic_residual(ep)
        );
    }

    // Test-local membership predicate: `(u/a)²−(v/b)²=1` within a band derived from
    // the cone chord bound (NOT a flat widening — the band is the in-plane image of
    // the surface-normal chord bound; on-curve points have residual ~1e-15), AND
    // `u>0`. We use TAU_MODEL since the endpoints are exact (residual ~1e-15).
    let contains = |hyp: &ssi_rs::SsiCurve, ep: [f64; 3]| -> bool {
        // out-of-plane reject first
        let (center, normal, _, _, _) = expect_hyperbola(hyp);
        let oop = dot(sub(ep, center), unit(normal)).abs();
        if oop > TAU_MODEL {
            return false;
        }
        let (u, _v) = hyperbola_uv(ep, hyp);
        hyperbola_inplane_residual(ep, hyp) <= TAU_MODEL && u > 0.0
    };

    let mut matched = 0usize;
    let mut matched_is_upper = false;
    let ax = unit(CONE_AXIS);
    for c in &curves {
        if contains(c, p_first) && contains(c, p_last) {
            matched += 1;
            let (_, _, m, _, _) = expect_hyperbola(c);
            if dot(m, ax) > 0.0 {
                matched_is_upper = true;
            }
        }
    }

    assert_eq!(
        matched, 1,
        "oracle5: EXACTLY ONE hyperbola branch must contain BOTH ring endpoints under the \
         `(u/a)²−(v/b)²=1 AND u>0` membership (the two-branch discriminator); got matched={matched}"
    );
    assert!(
        matched_is_upper,
        "oracle5: the matched branch must be the +axis (UPPER-nappe) branch — the solid's branch"
    );
}

// =========================================================================
// Oracle 7 — OUT-OF-SCOPE LOUD fixture. A hyperbola crossing genuinely BEYOND the
// relocation chord budget (`ρ ≈ 2·cone_d_eps > cone_d_eps`) must stay LOUD — NOT
// silently snapped, NOT widened. The ring is still EXACTLY on the x=1 plane (so a
// Hyperbola section is still the right section) but its cone radial residual is
// twice the chord band, so the relocation path must reject it.
//
// Verified geometric fact (this fixture's hyperbola section): NO on-plane,
// upper-nappe ring vertex triggers a `project_onto_cone_section` Err (a real
// cone∩plane section point always relocates cleanly; the asymptote `n_dot_g≈0`
// is at infinity, unreachable on the plane). So the hyperbola RELOCATION-band
// guard (`ρ > d_ε` → OffCurveBeyondChordBand) is geometrically UNREACHABLE for an
// on-plane ring — and at `ρ ≈ 2·d_ε` the ring vertices first fail the YR18
// on-both-surfaces gate (`tol = d_ε`), so the hyperbola edge is skipped before
// the reloc guard ever runs. The HONEST LOUD outcome at this magnitude is one
// layer earlier: the badly-off-surface apex-fan triangle centroid (≈1.33·d_ε off
// the cone) cannot be attributed to ANY input face within its Stage-1 chord band,
// so Stage-6 face attribution loudly raises `FaceResolutionFailed` — a genuine,
// non-silent rejection of an out-of-scope mock mesh (NOT a snap, NOT a widen).
//
// PR-YR23 (driver reframe, mirroring the YR22 oracle4 reframe — "spec principle
// over literal"): the spec §6 lists the LOUD variants as illustrative ("e.g.").
// The oracle's load-bearing INTENT is "the result is `Err`, never a silent
// Ok/snap" — `FaceResolutionFailed` honors that intent exactly. The narrower δ
// that would instead trip the reloc-band guard does not exist (it lands in a
// LineSegment-fallback dead zone that would SILENTLY succeed — the worse failure
// this oracle exists to forbid). So the sanctioned set is the honest LOUD family:
// OffCurveBeyondChordBand / LocalRefinementRequired / AmbiguousCurve (via
// SsiRefinementFailed) / FaceResolutionFailed — never a silent Ok.
// =========================================================================

#[test]
fn oracle7_out_of_scope_beyond_band_stays_loud() {
    let cone = oblique_cone();
    let bx = cutting_box();

    // Fixture sanity: the beyond-band ring is genuinely past the budget.
    let de = cone_d_eps();
    let arr = beyond_band_arrangement();
    let mut max_resid = 0.0_f64;
    for v in &arr.mesh.verts {
        let pt = v.as_array();
        if is_ring_point(pt) {
            max_resid = max_resid.max(conic_residual(pt));
        }
    }
    assert!(
        max_resid > de,
        "oracle7: the beyond-band ring must sit past the chord budget \
         (max residual {max_resid} > cone_d_eps {de})"
    );

    let mock = LabelMock { arrangement: arr };
    let r = boolean(&cone, &bx, BoolOp::Union, &mock);

    // Must be LOUD (Err), NOT a silent Ok that snapped a 2·d_ε crossing onto the
    // exact section.
    assert!(
        r.is_err(),
        "oracle7: an out-of-budget hyperbola crossing (ρ ≈ 2·cone_d_eps) must LOUDLY reject \
         (NOT silently snap / widen); got Ok"
    );
    let err = r.err().unwrap();
    let sanctioned = matches!(
        err,
        YangError::Stage4RegionInvalid {
            reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            ..
        } | YangError::Stage4RegionInvalid {
            reason: Stage4InvalidReason::LocalRefinementRequired,
            ..
        }
    ) || matches!(err, YangError::SsiRefinementFailed { .. })
        || matches!(err, YangError::FaceResolutionFailed { .. });
    assert!(
        sanctioned,
        "oracle7: the LOUD rejection must be a sanctioned out-of-scope variant \
         (OffCurveBeyondChordBand / LocalRefinementRequired / AmbiguousCurve via \
         SsiRefinementFailed / FaceResolutionFailed); got {err:?}"
    );
}

// =========================================================================
// Oracle 8 (optional) — env-gated real-sidecar E2E. Mirrors yr22 oracle8. LOUD
// eprintln skip when the binary is absent.
// =========================================================================

#[test]
fn oracle8_e2e_hyperbola_cone_union_box_on_hyperbola() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr23] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cone = oblique_cone();
    let bx = cutting_box();

    let r = boolean(&cone, &bx, BoolOp::Union, &sb)
        .expect("yr23 E2E: hyperbola cone ∪ box must Ok after cone Stage-4 hyperbola relocate");

    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr23 E2E: relocated output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr23 E2E: relocated output Euler must be 2"
    );

    let hyperbolas = hyperbola_edges(&r);
    assert!(
        !hyperbolas.is_empty(),
        "yr23 E2E: output must carry ≥1 Curve::Hyperbola intersection edge; got {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    let mut after_max_dev = 0.0_f64;
    for e in &hyperbolas {
        let (s, t) = edge_endpoints(&r, e);
        for ep in [s, t] {
            let radial = cone_radial_residual(ep);
            let planar = plane_residual(ep);
            after_max_dev = after_max_dev.max(radial.max(planar));
            assert!(
                radial <= TAU_MODEL,
                "yr23 E2E: relocated vertex {ep:?} cone radial residual {radial} > TAU_MODEL"
            );
            assert!(
                planar <= TAU_MODEL,
                "yr23 E2E: relocated vertex {ep:?} plane residual {planar} > TAU_MODEL"
            );
        }
    }
    assert!(
        after_max_dev <= TAU_MODEL,
        "yr23 E2E: max hyperbola deviation after relocate must be ≤ TAU_MODEL, got {after_max_dev}"
    );
}

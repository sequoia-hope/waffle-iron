//! M5 torus arm, increment 3 — torus × PLANE joins the surface-pair
//! vocabulary (P0032's shape, 2026-10-10; spec `specs/m5_surface_pair_curve.md`
//! "Torus × plane").
//!
//! A pentagon prism (circumradius 0.02 in the y–z plane, extruded 0.03 along
//! +x) is cut by a 266° revolve of a circle (r 0.0194) about an axis parallel
//! to z through (0.0337, −0.00644, 0.000355): a ring torus (R 0.02504) whose
//! tube crosses the prism's end cap x = 0.03. The cap∩torus curve is a
//! SPIRIC section — degree 4, no conic form — and until this increment Stage
//! 3 left every torus × plane edge as `LineSegment` chords of the boolean
//! mesh. The cap face was then bounded by a 9.2e-3 straight chord sitting
//! 7e-4 inside the true curve while the torus face's render triangles
//! followed the true surface, and the two faces overlapped by that sliver:
//! `SelfIntersectingBooleanOutput { FaceId(10) PLANE × FaceId(13) TORUS }`.
//!
//! Now the oblique section emits as `Curve::SurfacePair { torus, plane }`,
//! kernel-v2 carries the plane as a pair operand (`PairSurface::Plane`), and
//! the K8 placement rule reads "a surface-pair edge may bound a planar face
//! only when one operand IS that plane" — the planar face then samples the
//! same twin-canonical certified points as its torus neighbour.
//!
//! The Stage-3 emission is GATED OFF by default (checkpoint 1: the arm-on
//! corpus exposed three further sites, spec "Torus × plane" §Measured), so
//! this pin arms it with `YANG_TORUS_PLANE_PAIR=1` — the one test in this
//! binary, so the process-global knob is safe. RED without it
//! (mutation-checked 2026-10-10: `SelfIntersectingBooleanOutput`), GREEN
//! with it. The volume is pinned to
//! a 2e8-sample Monte-Carlo over the exact membership — prism minus the
//! counter-clockwise 266° wedge — 2.12567e-5 ± 1.7e-9 (the kernel's own
//! 2.125826e-5 named that wedge convention; the clockwise wedge would read
//! 8.98e-6).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Curve, Profile,
    RenderMesh, SolidId, Surface,
};

const R_PENT: f64 = 0.02;
const L_PRISM: f64 = 0.03;
const TORUS_C: [f64; 3] = [0.0337, -0.00644, 0.000355];
const R_MAJOR: f64 = 0.02504;
const R_MINOR: f64 = 0.0194;
const ANGLE_DEG: f64 = 266.0;
const MC_VOLUME: f64 = 2.12567e-5;

fn prism(a: &mut BrepArena) -> SolidId {
    // The sketch basis the engine derives for a +x plane: u = ŷ, v = ẑ.
    let pts: Vec<Point2> = (0..5)
        .map(|k| {
            let t = 2.0 * std::f64::consts::PI * k as f64 / 5.0;
            Point2::new(R_PENT * t.cos(), R_PENT * t.sin())
        })
        .collect();
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        pts,
        vec![],
    )
    .expect("pentagon profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), L_PRISM)
        .expect("prism extrude")
        .solid
}

fn cutter(a: &mut BrepArena) -> SolidId {
    // The circle sits in the plane y = C_y (sketch normal +y: u = −x̂, v = ẑ),
    // centred R_MAJOR from the axis on its −x side.
    let o = Point3::new(TORUS_C[0] - R_MAJOR, TORUS_C[1], TORUS_C[2]);
    let p = Profile::circle(
        o,
        Vector3::new(-1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, 0.0),
        R_MINOR,
    )
    .expect("circle profile");
    revolve(
        a,
        &p,
        Point3::new(TORUS_C[0], TORUS_C[1], TORUS_C[2]),
        Vector3::new(0.0, 0.0, 1.0),
        ANGLE_DEG.to_radians(),
    )
    .expect("torus revolve")
    .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Is `p`'s azimuth about the axis inside the revolve's CCW sweep from the
/// profile's direction (−x̂, azimuth π)?
fn in_wedge(p: &[f64]) -> bool {
    let th = (p[1] - TORUS_C[1]).atan2(p[0] - TORUS_C[0]);
    let d = (th - std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI);
    d <= ANGLE_DEG.to_radians()
}

/// Signed tube distance to the torus.
fn torus_residual(p: &[f64]) -> f64 {
    let (dx, dy, dz) = (p[0] - TORUS_C[0], p[1] - TORUS_C[1], p[2] - TORUS_C[2]);
    let rho = dx.hypot(dy);
    ((rho - R_MAJOR).powi(2) + dz * dz).sqrt() - R_MINOR
}

#[test]
fn a_torus_crossing_a_planar_cap_leaves_the_spiric_as_a_shared_pair_edge() {
    // Checkpoint 1: arm the gated Stage-3 emission (see the module docs).
    std::env::set_var("YANG_TORUS_PLANE_PAIR", "1");
    let mut a = BrepArena::new();
    let p = prism(&mut a);
    let v_prism = mesh_signed_volume(&tessellate(&a, p).expect("prism tessellates"));
    let c = cutter(&mut a);
    let out = boolean_op(&mut a, p, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("prism − torus wedge failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(vol > 0.0 && vol < v_prism);
    assert!(
        ((vol - MC_VOLUME) / MC_VOLUME).abs() < 3e-3,
        "volume {vol:.6e} vs the exact-membership Monte-Carlo {MC_VOLUME:.6e}"
    );

    // The vocabulary: the end cap x = L_PRISM is bounded by surface-pair
    // edges whose operands are the torus and that very plane.
    let mut cap_pair_edges = 0usize;
    let mut torus_faces = 0usize;
    for sh in &a.solid(out).unwrap().shells {
        for &f in &a.shell(*sh).unwrap().faces {
            let face = a.face(f).unwrap();
            match face.surface {
                Some(Surface::Torus { .. }) => torus_faces += 1,
                Some(Surface::Plane(pl)) if (pl.point.x() - L_PRISM).abs() < 1e-12 => {
                    for h in a.loop_half_edges(face.outer_loop).unwrap() {
                        let he = a.half_edge(h).unwrap();
                        if let Curve::SurfacePair { a: sa, b: sb } = he.curve {
                            let kinds = [sa, sb].map(|s| match s {
                                kernel_v2::PairSurface::Torus { .. } => 't',
                                kernel_v2::PairSurface::Plane { .. } => 'p',
                                _ => '?',
                            });
                            assert!(
                                kinds.contains(&'t') && kinds.contains(&'p'),
                                "a cap pair edge is {{torus, plane}}, got {kinds:?}"
                            );
                            cap_pair_edges += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    assert!(torus_faces >= 1, "the cut leaves a torus face");
    assert!(
        cap_pair_edges >= 2,
        "the end cap is bounded by spiric pair edges (got {cap_pair_edges})"
    );

    // The render: the cap's boundary samples sit ON the torus (each is
    // Newton-projected onto both surfaces), and there are many more of them
    // than the boolean mesh's chord endpoints — the refinement the chord
    // polyline could never carry.
    let on_cap_and_torus = mesh
        .positions
        .chunks_exact(3)
        .filter(|q| (q[0] - L_PRISM).abs() <= 1e-9 && torus_residual(q).abs() <= 1e-7)
        .count();
    assert!(
        on_cap_and_torus >= 12,
        "the cap carries the sampled spiric (got {on_cap_and_torus} vertices on both)"
    );
    // And no cap vertex inside the wedge's azimuth range lies strictly
    // inside the tube (the sliver the straight chord used to leave behind;
    // the 94° gap of the revolve is solid prism, where the tube distance
    // says nothing).
    for q in mesh.positions.chunks_exact(3) {
        if (q[0] - L_PRISM).abs() > 1e-9 || !in_wedge(q) {
            continue;
        }
        assert!(
            torus_residual(q) >= -1e-7,
            "cap vertex {q:?} sits {:.3e} INSIDE the cut torus",
            torus_residual(q)
        );
    }
}

//! M5 increment 1 — the procedural surface-pair curve vocabulary
//! (`specs/m5_surface_pair_curve.md`, K5–K11).
//!
//! The curve of a general-position quadric pair is represented implicitly
//! by its TWO analytic surfaces ([#24] Yang et al. 2025 §4.1.2/§4.3;
//! Constitution P8 degree-4 clarification). No producer exists yet in this
//! increment — fixtures retag hand-picked edges of `extrude` outputs whose
//! true geometry genuinely lies on both surfaces:
//!
//! - The kv12 vesica-lens prism's two vertical tip edges lie on BOTH
//!   supporting cylinders (centers (0,±1), r=√2) — they are exactly the
//!   secant-parallel cylinder×cylinder intersection lines, the degenerate
//!   member of the surface-pair family with zero residual.
//! - A unit-cube vertical edge lies on two parallel r=1 cylinders placed
//!   symmetrically around it (for the planar-face rejection, where the
//!   endpoint residuals are exactly zero so ONLY the placement rule fires).
//!
//! Oracle groups (spec §Oracles):
//! 1. validate: fixture passes; twin-descriptor mutation, off-surface
//!    endpoint, closed loop, planar-face placement each fail TYPED.
//! 2. tessellate: fixture meshes to the exact vesica prism volume band;
//!    the public sampler refines a genuinely curved perpendicular
//!    unequal-R pair edge with every sample on BOTH surfaces.
//! 3. re-entry: `to_yang_brep` rejects surface-pair edges loudly (K11).

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    extrude, surface_pair_interior_samples, tessellate, to_yang_brep, validate_solid, BrepArena,
    Curve, KernelV2Error, PairSurface, Profile, ProfileEdge, RenderMesh, SolidId, UnitVector3,
};

const H: f64 = 3.0;

fn up() -> UnitVector3 {
    UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }
}

/// The two supporting cylinders of the kv12 vesica lens: centers (0, ±1),
/// radius √2, axes along +ẑ. The lens tips (±1, 0) lie on BOTH.
fn vesica_pair() -> (PairSurface, PairSurface) {
    let r2 = 2.0_f64.sqrt();
    (
        PairSurface::Cylinder {
            axis_point: Point3::new(0.0, 1.0, 0.0),
            axis_dir: up(),
            radius: r2,
        },
        PairSurface::Cylinder {
            axis_point: Point3::new(0.0, -1.0, 0.0),
            axis_dir: up(),
            radius: r2,
        },
    )
}

/// Extrude the kv12 vesica lens (two consecutive arcs, k=2, tips (±1, 0))
/// by `H` along +ẑ, then retag its two vertical tip edges (all four
/// half-edges) as the surface-pair curve of the two supporting cylinders.
fn vesica_prism_with_surface_pair_tips() -> (BrepArena, SolidId) {
    let r2 = 2.0_f64.sqrt();
    let a = Point2::new(-1.0, 0.0);
    let b = Point2::new(1.0, 0.0);
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Arc {
                a,
                b,
                center: Point2::new(0.0, 1.0),
                radius: r2,
                ccw: true,
            },
            ProfileEdge::Arc {
                a: b,
                b: a,
                center: Point2::new(0.0, -1.0),
                radius: r2,
                ccw: true,
            },
        ],
        vec![],
    )
    .expect("valid vesica lens profile");
    let mut arena = BrepArena::new();
    let res = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
        .expect("vesica extrude succeeds");
    validate_solid(&arena, res.solid).expect("vesica prism valid before retag");

    let (ca, cb) = vesica_pair();
    let retagged = retag_vertical_segments(&mut arena, ca, cb);
    assert_eq!(
        retagged, 4,
        "the vesica prism has exactly two vertical tip edges (4 half-edges)"
    );
    (arena, res.solid)
}

/// Retag every vertical `LineSegment` half-edge (the tip rulings) with the
/// surface-pair curve; returns how many half-edges were retagged.
fn retag_vertical_segments(arena: &mut BrepArena, a: PairSurface, b: PairSurface) -> usize {
    let mut hits = 0;
    for slot in 0..arena.half_edges.len() {
        let Some(he) = arena.half_edges[slot] else {
            continue;
        };
        if !matches!(he.curve, Curve::LineSegment) {
            continue;
        }
        let p0 = arena.vertices[he.origin.0 as usize].unwrap().point;
        let next = arena.half_edges[he.next.0 as usize].unwrap();
        let p1 = arena.vertices[next.origin.0 as usize].unwrap().point;
        // Vertical = the tip rulings (the caps' arc edges are the only other
        // curves; every LineSegment in this solid is a vertical ruling, but
        // assert it anyway so the fixture stays honest).
        assert!(
            (p0.x() - p1.x()).abs() < 1e-15 && (p0.y() - p1.y()).abs() < 1e-15,
            "vesica prism LineSegments are vertical rulings"
        );
        arena.half_edges[slot].as_mut().unwrap().curve = Curve::SurfacePair { a, b };
        hits += 1;
    }
    hits
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
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

// ---------------------------------------------------------------------------
// Oracle group 1 — validate_solid
// ---------------------------------------------------------------------------

/// K5/K7 green path: the retagged prism is a valid solid — twins carry
/// identical descriptors and every endpoint is on BOTH cylinders (residual
/// exactly 0 for the tips).
#[test]
fn vesica_prism_with_surface_pair_tip_edges_validates() {
    let (arena, solid) = vesica_prism_with_surface_pair_tips();
    let report = validate_solid(&arena, solid).expect("surface-pair tip edges validate");
    assert_eq!(report.faces, 4, "2 caps + 2 cylinder walls");
    assert_eq!(report.edges, 6, "2+2 cap arcs + 2 tip edges");
}

/// K5: twins must carry bit-identical descriptors — perturbing ONE
/// half-edge's pair radius is a typed mismatch.
#[test]
fn surface_pair_twin_descriptor_mismatch_rejected() {
    let (mut arena, solid) = vesica_prism_with_surface_pair_tips();
    let idx = (0..arena.half_edges.len())
        .find(|&i| {
            matches!(
                arena.half_edges[i].map(|he| he.curve),
                Some(Curve::SurfacePair { .. })
            )
        })
        .expect("a surface-pair half-edge exists");
    let Some(Curve::SurfacePair { a, b }) = arena.half_edges[idx].map(|he| he.curve) else {
        unreachable!();
    };
    let PairSurface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    } = a
    else {
        unreachable!("vesica pair is cylinders");
    };
    arena.half_edges[idx].as_mut().unwrap().curve = Curve::SurfacePair {
        a: PairSurface::Cylinder {
            axis_point,
            axis_dir,
            radius: radius + 1e-9,
        },
        b,
    };
    let err = validate_solid(&arena, solid).expect_err("twin descriptor mismatch is rejected");
    assert!(
        matches!(err, KernelV2Error::CurveTwinMismatch { .. }),
        "expected CurveTwinMismatch, got {err:?}"
    );
}

/// K7: a surface-pair endpoint off either defining surface is a typed
/// off-surface finding (per-point on-BOTH-surfaces residual).
#[test]
fn surface_pair_endpoint_off_surface_rejected() {
    let (mut arena, solid) = vesica_prism_with_surface_pair_tips();
    // Move the (+1, 0, 0) tip vertex radially outward off both cylinders by
    // far more than the import band (1e-6 ≫ 1e-9·scale).
    let vid = (0..arena.vertices.len())
        .find(|&i| {
            arena.vertices[i].is_some_and(|v| {
                (v.point.x() - 1.0).abs() < 1e-12
                    && v.point.y().abs() < 1e-12
                    && v.point.z().abs() < 1e-12
            })
        })
        .expect("tip vertex at (1, 0, 0)");
    arena.vertices[vid].as_mut().unwrap().point = Point3::new(1.0 + 1e-6, 0.0, 0.0);
    let err = validate_solid(&arena, solid).expect_err("off-surface endpoint is rejected");
    assert!(
        matches!(
            err,
            KernelV2Error::VertexOffSurface { .. } | KernelV2Error::CurvedGeometryMismatch { .. }
        ),
        "expected a typed off-surface finding, got {err:?}"
    );
}

/// K6: a CLOSED surface-pair half-edge (origin == destination) has no
/// producer and is rejected like a closed Arc.
#[test]
fn closed_surface_pair_edge_rejected() {
    // A full-circle cap edge closes on its own origin — extrude a disc and
    // retag the closed rims; the closed-edge rule must fire regardless of
    // the descriptors' residuals (checked before them).
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        1.0,
    )
    .expect("unit disc profile");
    let mut arena = BrepArena::new();
    let res = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .expect("cylinder extrude succeeds");
    validate_solid(&arena, res.solid).expect("canonical cylinder valid");
    // Both cylinders of the pair contain the base rim circle: the solid's
    // own lateral (axis ẑ through origin, r=1) and a torus-free stand-in —
    // a second cylinder through the rim does not exist (a circle lies on
    // infinitely many quadrics, but on only ONE ẑ-axis cylinder), so the
    // closed-edge check must fire BEFORE any residual check. Use the same
    // cylinder twice; the closed shape is what is being rejected.
    let cyl = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let mut retagged = 0;
    for slot in 0..arena.half_edges.len() {
        let Some(he) = arena.half_edges[slot] else {
            continue;
        };
        if matches!(he.curve, Curve::Circle { .. }) {
            arena.half_edges[slot].as_mut().unwrap().curve = Curve::SurfacePair { a: cyl, b: cyl };
            retagged += 1;
        }
    }
    assert!(retagged >= 2, "cylinder rims retagged");
    let err = validate_solid(&arena, res.solid).expect_err("closed surface-pair edge rejected");
    assert!(
        matches!(err, KernelV2Error::CurveTwinMismatch { .. }),
        "expected CurveTwinMismatch (closed-edge rule), got {err:?}"
    );
}

/// K8: a surface-pair edge on a PLANAR face is rejected — a transversal
/// quadric-pair curve is never planar (degenerate configs produce conics
/// upstream). The retagged cube edge lies exactly on both cylinders, so
/// only the placement rule can fire.
#[test]
fn surface_pair_edge_on_planar_face_rejected() {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("unit square profile");
    let mut arena = BrepArena::new();
    let res = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .expect("cube extrude succeeds");
    // The vertical edge at (1, 0): both endpoints (and the whole line) lie
    // on the two parallel unit cylinders with axes through (0,0) and (2,0).
    let ca = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let cb = PairSurface::Cylinder {
        axis_point: Point3::new(2.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let mut retagged = 0;
    for slot in 0..arena.half_edges.len() {
        let Some(he) = arena.half_edges[slot] else {
            continue;
        };
        let p0 = arena.vertices[he.origin.0 as usize].unwrap().point;
        let next = arena.half_edges[he.next.0 as usize].unwrap();
        let p1 = arena.vertices[next.origin.0 as usize].unwrap().point;
        let on_line = |p: Point3| (p.x() - 1.0).abs() < 1e-12 && p.y().abs() < 1e-12;
        if on_line(p0) && on_line(p1) {
            arena.half_edges[slot].as_mut().unwrap().curve = Curve::SurfacePair { a: ca, b: cb };
            retagged += 1;
        }
    }
    assert_eq!(retagged, 2, "one vertical cube edge (twin pair) retagged");
    let err = validate_solid(&arena, res.solid).expect_err("planar placement rejected");
    assert!(
        matches!(err, KernelV2Error::CurvedGeometryMismatch { .. }),
        "expected CurvedGeometryMismatch (planar placement), got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Oracle group 2 — tessellation
// ---------------------------------------------------------------------------

/// The retagged prism tessellates: finite, NaN-free, watertight-volume in
/// the chord band of the exact vesica prism volume (π − 2)·H.
#[test]
fn vesica_prism_with_surface_pair_tips_tessellates() {
    let (arena, solid) = vesica_prism_with_surface_pair_tips();
    let mesh = tessellate(&arena, solid).expect("surface-pair prism tessellates");
    assert!(!mesh.indices.is_empty());
    assert!(
        mesh.positions.iter().all(|c| c.is_finite()),
        "no NaN/inf positions"
    );
    let exact = (PI - 2.0) * H;
    let vol = mesh_signed_volume(&mesh);
    // Inscribed chord mesh: volume below exact, within a few percent at the
    // default chord tolerance (same band the kv12 vesica assertion uses).
    assert!(
        vol > 0.0 && vol <= exact + 1e-9 && (exact - vol) / exact < 0.05,
        "volume {vol} vs exact {exact}"
    );
}

/// K9: the public sampler refines a genuinely CURVED quartic piece — the
/// perpendicular unequal-R pair x²+y²=1 ∧ x²+z²=¼ between two on-curve
/// points — and every returned sample lies on BOTH surfaces.
#[test]
fn surface_pair_sampler_perpendicular_unequal_cylinders() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let b = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: UnitVector3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
        radius: 0.5,
    };
    // Curve param: x = ½ sin φ, z = ½ cos φ, y = √(1 − x²).
    let p_of = |phi: f64| {
        let x = 0.5 * phi.sin();
        Point3::new(x, (1.0 - x * x).sqrt(), 0.5 * phi.cos())
    };
    let start = p_of(0.0); // (0, 1, ½)
    let end = p_of(PI / 2.0); // (½, √¾, 0)
    let tol = 1e-4;
    let samples =
        surface_pair_interior_samples(&a, &b, start, end, tol).expect("sampler converges");
    assert!(
        samples.len() >= 3,
        "a quarter-turn quartic piece needs interior refinement at tol {tol}, got {}",
        samples.len()
    );
    for s in &samples {
        let ra = (s.x() * s.x() + s.y() * s.y()).sqrt();
        let rb = (s.x() * s.x() + s.z() * s.z()).sqrt();
        assert!(
            (ra - 1.0).abs() < 1e-9 && (rb - 0.5).abs() < 1e-9,
            "sample {s:?} on both surfaces (ra={ra}, rb={rb})"
        );
    }
    // Samples advance monotonically in the curve parameter φ = atan2(x, z)·…
    // (strictly increasing from start to end — no back-tracking).
    let phi = |p: &Point3| (2.0 * p.x()).atan2(2.0 * p.z());
    let mut prev = phi(&start);
    for s in &samples {
        let cur = phi(s);
        assert!(cur > prev, "samples advance monotonically along the curve");
        prev = cur;
    }
    assert!(phi(&end) > prev);
    // Chord-bound satisfied: for each adjacent pair the midpoint's distance
    // to the curve (via the sampler's own certification) is within tol —
    // spot-check with a dense polyline: max deviation of the true curve
    // from each chord stays under 2·tol.
    let mut chain = vec![start];
    chain.extend(samples.iter().copied());
    chain.push(end);
    for w in chain.windows(2) {
        let (p0, p1) = (w[0], w[1]);
        let (f0, f1) = (phi(&p0), phi(&p1));
        for k in 1..8 {
            let t = f0 + (f1 - f0) * (k as f64) / 8.0;
            let q = p_of(t);
            // Distance from q to the chord p0→p1.
            let d = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
            let v = [q.x() - p0.x(), q.y() - p0.y(), q.z() - p0.z()];
            let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let t_proj = (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / dd;
            let c = [
                v[0] - t_proj * d[0],
                v[1] - t_proj * d[1],
                v[2] - t_proj * d[2],
            ];
            let dist = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            assert!(
                dist < 2.0 * tol,
                "chord sag {dist} exceeds 2·tol at φ={t} between {p0:?} and {p1:?}"
            );
        }
    }
}

/// K9 with a CONE operand (M5 cone-pair producer): the sampler's Newton
/// projector drives the `PairSurface::Cone` residual/gradient. A π/4 cone about
/// +z (`x²+y² = z²`) meets a z-parallel unit cylinder centred at (2,0,·) in a
/// degree-4 arc. Sampled from (1,0,1) [x=1, y=0] to (2,1,√5) [x=2, y=1] — a
/// quarter of the y-bulge, whose chord clears both axes. Every sample must
/// satisfy BOTH implicit residuals tightly, and the bulging arc needs interior
/// refinement.
#[test]
fn surface_pair_sampler_cone_cylinder() {
    let cone = PairSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        half_angle: PI / 4.0,
    };
    let cyl = PairSurface::Cylinder {
        axis_point: Point3::new(2.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let start = Point3::new(1.0, 0.0, 1.0); // x=1,y=0: cone 1=1, cyl (1−2)²=1
    let end = Point3::new(2.0, 1.0, 5.0_f64.sqrt()); // x=2,y=1: cone 5=5, cyl 0+1=1
    let tol = 1e-4;
    let samples =
        surface_pair_interior_samples(&cone, &cyl, start, end, tol).expect("sampler converges");
    assert!(
        !samples.is_empty(),
        "the y-bulging cone∩cyl arc needs interior refinement at tol {tol}"
    );
    for s in &samples {
        let radial = (s.x() * s.x() + s.y() * s.y()).sqrt();
        let cone_res = (radial - s.z().abs()).abs(); // tan(π/4) = 1
        let cyl_res = (((s.x() - 2.0).powi(2) + s.y() * s.y()).sqrt() - 1.0).abs();
        assert!(
            cone_res < 1e-9 && cyl_res < 1e-9,
            "sample {s:?} on both surfaces (cone_res={cone_res}, cyl_res={cyl_res})"
        );
    }
}

/// K9 at LARGE coordinate magnitude (2026-08-19, R0044 anchor): the same
/// cone∩cylinder arc translated to |x| ≈ 6e3. Every pair residual is a
/// LENGTH, so at that magnitude nothing evaluates below ~8·ε·L ≈ 1e-11; the
/// projector's bare 1e-13 acceptance (mirroring yang-rs's PRE-2026-07-28
/// contract) could never be met and a fully converged root reported
/// "did not converge". The floor is now `max(1e-13, 8·ε·L)` (yang-rs's
/// amended contract); meters-scale behavior is unchanged.
#[test]
fn surface_pair_sampler_cone_cylinder_at_large_magnitude() {
    let o = [6000.0, -6000.0, 6000.0];
    let cone = PairSurface::Cone {
        apex: Point3::new(o[0], o[1], o[2]),
        axis_dir: up(),
        half_angle: PI / 4.0,
    };
    let cyl = PairSurface::Cylinder {
        axis_point: Point3::new(o[0] + 2.0, o[1], o[2]),
        axis_dir: up(),
        radius: 1.0,
    };
    let start = Point3::new(o[0] + 1.0, o[1], o[2] + 1.0);
    let end = Point3::new(o[0] + 2.0, o[1] + 1.0, o[2] + 5.0_f64.sqrt());
    let tol = 1e-4;
    let samples = surface_pair_interior_samples(&cone, &cyl, start, end, tol)
        .expect("a converged root at |x| ~ 6e3 must be ACCEPTED — 1e-13 is below one ULP there");
    assert!(!samples.is_empty());
    let band = 8.0 * f64::EPSILON * 6000.0 * 4.0;
    for s in &samples {
        let radial = ((s.x() - o[0]).powi(2) + (s.y() - o[1]).powi(2)).sqrt();
        let cone_res = (radial - (s.z() - o[2]).abs()).abs();
        let cyl_res = (((s.x() - o[0] - 2.0).powi(2) + (s.y() - o[1]).powi(2)).sqrt() - 1.0).abs();
        assert!(
            cone_res < band && cyl_res < band,
            "sample {s:?} on both surfaces within the evaluation floor \
             (cone_res={cone_res:e}, cyl_res={cyl_res:e}, band={band:e})"
        );
    }
}

/// K9 failure mode: a tangent pair (parallel normals along the contact
/// line) cannot be Newton-refined — typed, loud, no chord fallback.
#[test]
fn surface_pair_sampler_tangent_pair_fails_loud() {
    // Externally tangent unit cylinders: axes through (0,0) and (2,0),
    // touching along the line x=1, y=0. Normals are ±x̂ for both surfaces
    // everywhere on the contact line — the Gauss-Newton system is rank-1.
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let b = PairSurface::Cylinder {
        axis_point: Point3::new(2.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let start = Point3::new(1.0, 0.0, 0.0);
    let end = Point3::new(1.0, 0.0, 1.0);
    // The chord IS the exact curve here, so a zero-work success (no interior
    // samples needed: midpoint already certified on both surfaces) is the
    // one acceptable outcome besides the typed tangency stop.
    match surface_pair_interior_samples(&a, &b, start, end, 1e-4) {
        Ok(samples) => {
            for s in &samples {
                let ra = (s.x() * s.x() + s.y() * s.y()).sqrt();
                let rb = ((s.x() - 2.0) * (s.x() - 2.0) + s.y() * s.y()).sqrt();
                assert!(
                    (ra - 1.0).abs() < 1e-9 && (rb - 1.0).abs() < 1e-9,
                    "tangent-line samples must stay certified, got {s:?}"
                );
            }
        }
        Err(reason) => {
            assert!(
                !reason.is_empty(),
                "tangency failure carries a named reason"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Oracle group 3 — boolean re-entry wall (K11)
// ---------------------------------------------------------------------------

/// K11 re-entry (2026-09-04, spec "K11 re-entry"): a solid carrying
/// surface-pair edges RE-ENTERS yang Stage 1 — each twin pair converts to ONE
/// shared yang `Curve::SurfacePair` input edge carrying both operands
/// operand-for-operand, and a chained boolean on the quartic-bounded body
/// succeeds with an exact planar decrement (a pocket in the top cap, clear of
/// the lateral walls: |x|, |y| ≤ 0.2 lies inside the lens, whose boundary at
/// |x| ≤ 0.2 is at |y| ≥ 0.4). Was `surface_pair_reentry_rejected`, the
/// typed K11 wall.
#[test]
fn surface_pair_reentry_enters_yang() {
    let (mut arena, solid) = vesica_prism_with_surface_pair_tips();
    let yb = to_yang_brep(&arena, solid).expect("surface-pair edges convert to yang input");
    let pair_edges = yb
        .edges()
        .iter()
        .filter(|e| matches!(e.curve, yang_rs::Curve::SurfacePair { .. }))
        .count();
    assert_eq!(
        pair_edges, 2,
        "the two tip edges (four half-edges) must convert to two SHARED yang edges"
    );
    for e in yb.edges() {
        if let yang_rs::Curve::SurfacePair { a, b } = e.curve {
            assert!(
                matches!(a, yang_rs::Surface::Cylinder { radius, .. } if (radius - 2.0_f64.sqrt()).abs() < 1e-15)
                    && matches!(b, yang_rs::Surface::Cylinder { radius, .. } if (radius - 2.0_f64.sqrt()).abs() < 1e-15),
                "operands carried verbatim: {a:?} / {b:?}"
            );
        }
    }

    let v1 = mesh_signed_volume(&tessellate(&arena, solid).expect("prism tessellates"));
    let pocket = Profile::new(
        Point3::new(0.0, 0.0, H - 0.3),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(-0.2, -0.2),
            Point2::new(0.2, -0.2),
            Point2::new(0.2, 0.2),
            Point2::new(-0.2, 0.2),
        ],
        vec![],
    )
    .unwrap();
    let tool = extrude(&mut arena, &pocket, Vector3::new(0.0, 0.0, 1.0), 0.6)
        .unwrap()
        .solid;
    let out = kernel_v2::boolean_op(&mut arena, solid, tool, cad_primitives::BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("re-enter the quartic-bounded prism: {e:?}"));
    validate_solid(&arena, out).expect("re-entered result validates");
    let v2 = mesh_signed_volume(&tessellate(&arena, out).expect("result tessellates"));
    // 0.4 × 0.4 × 0.3 = 0.048 removed from the planar top cap.
    assert!(
        (v1 - v2 - 0.048).abs() < 1e-3,
        "pocket decrement {} must be ≈0.048: v1={v1} v2={v2}",
        v1 - v2
    );
}

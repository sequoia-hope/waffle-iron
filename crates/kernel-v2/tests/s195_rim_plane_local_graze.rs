//! #195 rim×plane graze, LOCAL form (P0029's shape, 2026-10-09).
//!
//! A square prism (|x|, |y| ≤ 30 on z = 0, height 34 along +z) is cut by a
//! TILTED cylinder (r 24) whose bottom cap sits inside the prism and whose
//! bottom cap RIM dips just 1.45e-2 below the prism's own bottom cap plane
//! z = 0 — a relative graze of 6.0e-4 on the radius, far below the arm's
//! render-observability line.
//!
//! The rim crosses z = 0 twice, and both crossings are exact
//! {bottom-cap plane, cutter cap plane, cutter cylinder} corners that
//! Stage 1 mints. They are 3.0 apart against a natural rim chord of ~16, so
//! they arrive as ADJACENT rim vertices — and a straight edge between two
//! points that each lie in BOTH planes lies in both planes. The chord
//! polyline therefore does not cross z = 0 at all: it runs along it. The
//! cutter's cap sheet and lateral sheet meet the prism's bottom cap sheet
//! along one segment, the exact arrangement faithfully reports a doubled
//! directed edge (fwd=2 rev=2, one page per operand), and Stage 4's
//! watertightness gate STOPs `reassembled output would be non-2-manifold`.
//!
//! The lens is REAL — on P0029's own numbers the sliver the solids trade is
//! 2.37 × 3.37e-2, some 3.4e4 × `MIN_FEATURE_SIZE` — so the STOP is a
//! capability gap, not a loud-by-design wall. The body-wide rim-N floor that
//! would resolve it is 128 against a natural 9; the LOCAL form pays the same
//! sagitta demand as three extra samples on the grazed arc of the rim's own
//! closure.
//!
//! RED with `YANG_195_LOCAL=off`. Measured over a 60-fixture sweep of this
//! family (3 radii × 5 lens depths × 5 tilts, every one RED at the natural
//! density): 53 build with the rule on. The residual 7 all sit on the two
//! steepest tilts and STOP at a DIFFERENT Stage-4 site —
//! `YANG_LRR_STOP site=split_cycle` (`stage4_correct.rs:14628`), the named
//! follow-up in `docs/yang_tail_triage.md`.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

/// The prism: the square |x|, |y| ≤ R_BOX on z = 0, extruded H_BOX along +z.
const R_BOX: f64 = 30.0;
const H_BOX: f64 = 34.0;
/// The cutter's radius and its axis as authored (NOT unit length; the engine
/// normalizes).
const R_CUT: f64 = 24.0;
const N_CUT: [f64; 3] = [0.0, 0.3, 0.95];
const DEPTH_CUT: f64 = 120.0;
/// How far the rim's lowest point dips below the prism's bottom cap. Chosen
/// below the arm's render line (2e-3·r = 4.8e-2) and far above the #178
/// authoring-noise line, so the site is exactly the §5k population.
const LENS_DEPTH: f64 = 1.45e-2;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The cutter's cap-plane frame, exactly as `SketchPlaneBasis::from_origin_normal`
/// derives it: reference ẑ (|n̂·ẑ| < 0.99), x = ẑ × n̂, y = n̂ × x.
fn cut_frame(axis: [f64; 3]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let n = norm(axis);
    let x = norm(cross([0.0, 0.0, 1.0], n));
    let y = norm(cross(n, x));
    (n, x, y)
}

/// The cap-rim centre: on the axis, placed so the rim's LOWEST point sits
/// `LENS_DEPTH` below z = 0. The rim's z oscillates about the centre with
/// amplitude `r·√(1−n̂_z²)` (because `e1_z² + e2_z² + n̂_z² = 1`).
fn cut_origin(axis: [f64; 3]) -> [f64; 3] {
    let (n, _, _) = cut_frame(axis);
    let amp = R_CUT * (1.0 - n[2] * n[2]).max(0.0).sqrt();
    [0.0, 0.0, amp - LENS_DEPTH]
}

fn prism(a: &mut BrepArena) -> SolidId {
    // The sketch basis the engine derives for a +z plane: x = −ŷ, y = x̂.
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        vec![
            Point2::new(-R_BOX, -R_BOX),
            Point2::new(R_BOX, -R_BOX),
            Point2::new(R_BOX, R_BOX),
            Point2::new(-R_BOX, R_BOX),
        ],
        vec![],
    )
    .expect("square profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), H_BOX)
        .expect("prism extrude")
        .solid
}

fn cutter(a: &mut BrepArena, axis: [f64; 3]) -> SolidId {
    let (n, x, y) = cut_frame(axis);
    let o = cut_origin(axis);
    let p = Profile::circle(
        Point3::new(o[0], o[1], o[2]),
        Vector3::new(x[0], x[1], x[2]),
        Vector3::new(y[0], y[1], y[2]),
        Point2::new(0.0, 0.0),
        R_CUT,
    )
    .expect("cutter profile");
    // Sweep UP the axis, so the cap plane — and its grazing rim — stays put.
    extrude(a, &p, Vector3::new(n[0], n[1], n[2]), DEPTH_CUT)
        .expect("cutter extrude")
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

/// Radial residual against the cutter's lateral surface.
fn cyl_residual(p: [f64; 3], axis: [f64; 3]) -> f64 {
    let (n, _, _) = cut_frame(axis);
    let o = cut_origin(axis);
    let v = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
    let t = dot(v, n);
    let q = [v[0] - t * n[0], v[1] - t * n[1], v[2] - t * n[2]];
    dot(q, q).sqrt() - R_CUT
}

/// The two exact crossings of the cap rim with z = 0, in closed form: the
/// rim's z is `o_z + r(cos t·e1_z + sin t·e2_z)`, so `z = 0` is one
/// `A cos t + B sin t = −o_z` and the roots are `φ ± ψ`.
fn exact_crossings(axis: [f64; 3]) -> [[f64; 3]; 2] {
    let (_, x, y) = cut_frame(axis);
    let o = cut_origin(axis);
    let (ca, cb) = (R_CUT * x[2], R_CUT * y[2]);
    let amp = ca.hypot(cb);
    let phi = cb.atan2(ca);
    let psi = (-o[2] / amp).clamp(-1.0, 1.0).acos();
    let at = |t: f64| {
        let (s, c) = t.sin_cos();
        [
            o[0] + R_CUT * (c * x[0] + s * y[0]),
            o[1] + R_CUT * (c * x[1] + s * y[1]),
            o[2] + R_CUT * (c * x[2] + s * y[2]),
        ]
    };
    [at(phi - psi), at(phi + psi)]
}

#[test]
fn a_rim_grazing_a_cap_plane_crosses_it_instead_of_lying_in_it() {
    // The fixture is the site: both crossings on z = 0, on the cutter, and
    // close enough together to be adjacent rim samples.
    let xs = exact_crossings(N_CUT);
    for p in xs {
        assert!(p[2].abs() < 1e-12, "a crossing lies on z = 0: {p:?}");
        assert!(
            cyl_residual(p, N_CUT).abs() < 1e-9,
            "a crossing lies on the cutter: residual {:.3e}",
            cyl_residual(p, N_CUT)
        );
    }
    let chord = {
        let d = [
            xs[1][0] - xs[0][0],
            xs[1][1] - xs[0][1],
            xs[1][2] - xs[0][2],
        ];
        dot(d, d).sqrt()
    };
    assert!(
        chord > 1.0 && chord < 4.0,
        "the crossings are one rim chord apart (got {chord}), which is what \
         flattens the crossing into a line contact"
    );

    let mut a = BrepArena::new();
    let p = prism(&mut a);
    let v_prism = mesh_signed_volume(&tessellate(&a, p).expect("prism tessellates"));
    let c = cutter(&mut a, N_CUT);
    let out = boolean_op(&mut a, p, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("prism − grazing cylinder failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_prism,
        "the pocket removes a proper part of the prism: {v_prism} → {vol}"
    );

    // The defect's own quantity: the prism's bottom cap must carry the
    // NOTCH the lens cuts out of it — at least one output vertex on z = 0
    // that lies on the cutter's lateral surface. With the rim flattened
    // into the plane there is no notch at all (and no output: the gate
    // STOPs), so this is the assertion the fix turns green.
    let on_cap_and_cutter = mesh
        .positions
        .chunks_exact(3)
        .filter(|p| p[2].abs() <= 1e-9 && cyl_residual([p[0], p[1], p[2]], N_CUT).abs() <= 1e-6)
        .count();
    assert!(
        on_cap_and_cutter >= 2,
        "the bottom cap keeps the lens notch — vertices both on z = 0 and on \
         the cutter (got {on_cap_and_cutter})"
    );

    // And the notch is only a notch: every bottom-cap vertex stays inside
    // the prism's footprint, and none sits INSIDE the cutter (which would
    // mean the sliver was left attached, the silent-wrong this STOP guards).
    for q in mesh.positions.chunks_exact(3) {
        let q = [q[0], q[1], q[2]];
        if q[2].abs() > 1e-9 {
            continue;
        }
        assert!(
            q[0].abs() <= R_BOX + 1e-9 && q[1].abs() <= R_BOX + 1e-9,
            "bottom-cap vertex {q:?} escaped the prism footprint"
        );
        assert!(
            cyl_residual(q, N_CUT) >= -1e-6,
            "bottom-cap vertex {q:?} sits {:.3e} INSIDE the cutter",
            cyl_residual(q, N_CUT)
        );
    }
}

/// The §5k residual (2026-10-09): 7 of the 60 swept fixtures still STOP,
/// all on the two steepest tilts. The arm fires and the doubled edge is
/// GONE — the refinement reaches Stage 4 and STOPs there instead, at
/// `YANG_LRR_STOP site=split_cycle` / `stage4_correct.rs:14628`
/// `LocalRefinementRequired` — so a SECOND wall sits behind this one, on a
/// different site. This is the same axis as P0029's own
/// (`(0.2, 0.45, 0.87)`) at P0029's own lens depth.
///
/// Un-quarantine in the commit that lands that wall.
#[test]
#[ignore = "§5k residual: the refined mesh STOPs at Stage-4 split_cycle \
            (stage4_correct.rs:14628) — a different site, named follow-up \
            in docs/yang_tail_triage.md §2026-10-09"]
fn a_steeply_tilted_grazing_rim_still_stops_at_split_cycle() {
    const STEEP: [f64; 3] = [0.2, 0.45, 0.87];
    let mut a = BrepArena::new();
    let p = prism(&mut a);
    let c = cutter(&mut a, STEEP);
    let out = boolean_op(&mut a, p, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("prism − steeply tilted grazing cylinder failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1);
    let mesh = tessellate(&a, out).expect("output tessellates");
    let notch = mesh
        .positions
        .chunks_exact(3)
        .filter(|q| q[2].abs() <= 1e-9 && cyl_residual([q[0], q[1], q[2]], STEEP).abs() <= 1e-6)
        .count();
    assert!(
        notch >= 2,
        "the bottom cap keeps the lens notch (got {notch})"
    );
}

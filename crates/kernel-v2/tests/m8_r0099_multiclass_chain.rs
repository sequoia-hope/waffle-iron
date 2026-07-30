//! M8 Stage-0 multi-class cavity arm — the R0099 conversion pin
//! (spec `specs/m8_stage0_multiclass_cavity_arm.md` §4 inc-1, amendment 12
//! of the fold-gate series).
//!
//! R0099's op 3 (revolve cut, rectangle profile) contacts the tube ONLY
//! coplanarly: its θ=0/θ=180 profile rectangles lie exactly in the
//! bottom-cap plane, zero transversal intersections ⇒ `has_conic = false`
//! ⇒ Stage 4 never runs — whatever Stage 0 emits is final. The overlay
//! mints exact on-rim-circle vertices where the wedge rectangles cross the
//! cap's rim chords (u ≈ ±3.1205 vs r = 3.1251 — the crossings hug the
//! rim); moving those mints chord→circle folds local slivers, every arm of
//! the repair ladder rejects (`multi-class cavity with constraint-blocked
//! fan` — the mint sits ON the intersection polyline, so its cavity spans
//! two region classes BY CONSTRUCTION), and the amendment-2 fallback
//! REVERTS the mints to chord lifts. Three chord-position vertices survive
//! into the outer-cylinder face boundary 6.1e-2..9.1e-2 inside r —
//! `VertexOffSurface`, caught by the debug/strict validation tripwire.
//!
//! CONVERTED 2026-07-30 (amendment 14, spec §11): the vertex-inserting
//! split repairs vert 9 — q_a/q_b split B's real edge where the moved
//! chain crosses it, the cavity re-cuts into remnants + material + the
//! AOnly bulge, and the chain subdivision propagates through BOTH
//! inputs' boundary chains (`collect_edge_splits` + the rim-override
//! extras). Both tests below are GREEN pins of the converted state.
//!
//! Historical arc — MEASURED 2026-07-30 at inc-1 (and unchanged by the
//! inc-2 always-on flip, which the corpus passed with zero category
//! changes): the wedge
//! decomposition fires at all four multi-class mints (verts 4/9/116/182 —
//! vert 9 the closed interior form) and fans their valid wedges, but
//! every FOLDED wedge polygon is exactly NON-SIMPLE — the
//! interacting-mints signature (vert 4's ring: a neighbor mint's
//! collapsed chord passes through v's minted position). The NonSimple
//! propagates crossing-narrowed seeds into the amendment-6 joint path
//! (region attempts [178,182], [115,116,120,126]) whose sub-region
//! guards reject: `crossing edges ungrowable (region polygon not simple)`
//! / `region too small` — exactly the spec's inc-3 region-form-parity
//! scope, now census-armed. In-chain reverts drop 6 → 3; the residual
//! leak (vert 9) was the inc-1-era quarantine reason, since lifted by
//! the amendment-14 split above.
//!
//! Chain values are R0099's bit-exact parameters (master seed 42, case
//! seed 9039304369631583684), replayed through direct constructors in the
//! PRODUCTION sketch frame — `tangent_x_from_normal` replicated
//! digit-for-digit from feature-engine `rebuild.rs`, y = n̂ × x̂ per the
//! adapter's `make_faces_from_profiles` — the `m8_swiss_cheese_chain.rs`
//! pattern.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
    SolidId,
};

/// All three sketches share this plane (the case's datum).
const PLANE_O: [f64; 3] = [-0.984136019356967, -4.672440505341375, 7.920748897180792];
const PLANE_N: [f64; 3] = [0.619532616596952, -0.3130983129596498, -0.7198255228833966];

/// Op 1: extrude circle, boss.
const R_BOSS: f64 = 3.12510105566724;
const D_BOSS: f64 = 1.9457327344208741;
/// Op 2: extrude circle, cut (through — depth > boss height; no reverse:
/// the target body lies on the +n̂ side of the sketch plane, `cut_eps` is 0
/// since B23, so the tool is the plain +n̂ cylinder).
const R_CUT: f64 = 2.2396140720438398;
const D_CUT: f64 = 4.6123958261502676;
/// Op 3: revolve rectangle (sketch UV half-extents), cut. The axis is
/// in-plane; the angle reaches the kernel in degrees and is converted at
/// the adapter boundary.
const RECT_HU: f64 = 3.120459344391322;
const RECT_HV: f64 = 1.143228783649328;
const AXIS_O: [f64; 3] = [-2.896139917617264, -13.563135193291696, 10.142277141133917];
const AXIS_D: [f64; 3] = [0.7579338931491432, -0.0, 0.6523313679532691];
const ANGLE_DEG: f64 = 323.7129961571792;

/// feature-engine `rebuild.rs::tangent_x_from_normal`, digit-for-digit:
/// ref = Z (|n_z| < 0.99 here), x̂ = normalize(Z × n).
fn tangent_x_from_normal(n: [f64; 3]) -> [f64; 3] {
    let ref_vec = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let cx = [
        ref_vec[1] * n[2] - ref_vec[2] * n[1],
        ref_vec[2] * n[0] - ref_vec[0] * n[2],
        ref_vec[0] * n[1] - ref_vec[1] * n[0],
    ];
    let len = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if len < 1e-12 {
        return [1.0, 0.0, 0.0];
    }
    [cx[0] / len, cx[1] / len, cx[2] / len]
}

/// The production sketch frame: (origin, x̂, ŷ = n × x̂) — the adapter's
/// `make_faces_from_profiles` convention.
fn sketch_frame() -> (Point3, Vector3, Vector3) {
    let n = PLANE_N;
    let x = tangent_x_from_normal(n);
    let y = [
        n[1] * x[2] - n[2] * x[1],
        n[2] * x[0] - n[0] * x[2],
        n[0] * x[1] - n[1] * x[0],
    ];
    (
        Point3::new(PLANE_O[0], PLANE_O[1], PLANE_O[2]),
        Vector3::new(x[0], x[1], x[2]),
        Vector3::new(y[0], y[1], y[2]),
    )
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

fn volume_of(a: &BrepArena, s: SolidId, what: &str) -> f64 {
    let mesh = tessellate(a, s).unwrap_or_else(|e| panic!("tessellate {what}: {e:?}"));
    mesh_signed_volume(&mesh)
}

/// The 3-op chain. Returns the per-op volumes with the final solid, or the
/// first failing step's error (Debug-formatted, step-tagged).
fn run_chain() -> Result<(BrepArena, SolidId, [f64; 3]), String> {
    let (origin, ux, vy) = sketch_frame();
    let n_dir = Vector3::new(PLANE_N[0], PLANE_N[1], PLANE_N[2]);
    let mut a = BrepArena::new();

    // Op 1 — extrude circle, boss.
    let p1 = Profile::circle(origin, ux, vy, Point2::new(0.0, 0.0), R_BOSS)
        .map_err(|e| format!("op1 profile: {e:?}"))?;
    let mut body = extrude(&mut a, &p1, n_dir, D_BOSS)
        .map_err(|e| format!("op1 extrude: {e:?}"))?
        .solid;
    validate_solid(&a, body).map_err(|e| format!("op1 validate: {e:?}"))?;
    let v1 = volume_of(&a, body, "op1");

    // Op 2 — extrude circle, cut (through).
    let p2 = Profile::circle(origin, ux, vy, Point2::new(0.0, 0.0), R_CUT)
        .map_err(|e| format!("op2 profile: {e:?}"))?;
    let tool2 = extrude(&mut a, &p2, n_dir, D_CUT)
        .map_err(|e| format!("op2 extrude: {e:?}"))?
        .solid;
    body = boolean_op(&mut a, body, tool2, BoolOp::Subtract)
        .map_err(|e| format!("op2 boolean: {e:?}"))?;
    validate_solid(&a, body).map_err(|e| format!("op2 validate: {e:?}"))?;
    let v2 = volume_of(&a, body, "op2");

    // Op 3 — revolve rectangle, cut. The wedge's θ=0/θ=180 rectangles lie
    // exactly in the tube's bottom-cap plane: a pure Stage-0 op.
    let rect = vec![
        Point2::new(-RECT_HU, -RECT_HV),
        Point2::new(RECT_HU, -RECT_HV),
        Point2::new(RECT_HU, RECT_HV),
        Point2::new(-RECT_HU, RECT_HV),
    ];
    let p3 = Profile::new(origin, ux, vy, rect, Vec::new())
        .map_err(|e| format!("op3 profile: {e:?}"))?;
    let tool3 = revolve(
        &mut a,
        &p3,
        Point3::new(AXIS_O[0], AXIS_O[1], AXIS_O[2]),
        Vector3::new(AXIS_D[0], AXIS_D[1], AXIS_D[2]),
        ANGLE_DEG.to_radians(),
    )
    .map_err(|e| format!("op3 revolve: {e:?}"))?
    .solid;
    body = boolean_op(&mut a, body, tool3, BoolOp::Subtract)
        .map_err(|e| format!("op3 boolean: {e:?}"))?;
    validate_solid(&a, body).map_err(|e| format!("op3 validate: {e:?}"))?;
    let v3 = volume_of(&a, body, "op3");

    Ok((a, body, [v1, v2, v3]))
}

/// GREEN pin (flipped 2026-07-30 at the amendment-14 inc-3.2d corpus
/// flip, per this pin's original instruction): the historical RED form
/// asserted the `VertexOffSurface` revert leak at op 3. The
/// vertex-inserting split (spec §11) repairs vert 9's fold truthfully —
/// q_a/q_b split B's real edge, the chain subdivision propagates through
/// BOTH inputs' boundary chains — so the chain now completes THROUGH the
/// on-surface tripwire (debug_assertions / `strict-validation`), which
/// remains armed: a regression re-introducing the leak fails here loudly.
#[test]
fn r0099_chain_completes_through_on_surface_tripwire() {
    let (a, body, _) = run_chain().unwrap_or_else(|e| {
        panic!(
            "R0099 chain must complete since the amendment-14 split \
             (historical wall: op3 VertexOffSurface): {e}"
        )
    });
    validate_solid(&a, body).expect("final solid must validate");
}

/// The conversion oracle: the chain completes, validates (including the
/// debug/strict on-surface tripwire — the historical leak site), and the
/// volumes agree with the analytics.
///
/// **Op 3 is a MEASURE-ZERO cut** (derived at amendment-14 inc-3.2,
/// 2026-07-30, correcting this oracle's original strict `v3 < v2`): the
/// revolve axis is in-plane and perpendicular to the tube's axis
/// (n̂·AXIS_D ≈ 1e-4), with PLANE_O at the axis foot (offset ≈ 9.362
/// along the in-plane perpendicular). The 36.31° UNSWEPT sector hugs the
/// tube's side of the plane: a tube point at height h is uncovered when
/// h < tan(36.31°)·ρ⊥ and ρ⊥ ≥ 6.24 everywhere on the tube, so the bound
/// (≥ 4.58) exceeds the tube's full height (1.95) — the wedge's material
/// NEVER enters the tube; contact is exactly the coplanar faces (the
/// module docs' "only coplanarly", now quantified). TRUE Δv = 0, so the
/// truthful assertion is v3 in the SAME analytic annulus band as v2 —
/// and v3 may legitimately EXCEED v2: the split keeps rim mints ON their
/// circles, restoring chord-cut sliver volume (measured +0.069%, toward
/// the analytic). The corpus meta's "decrease" entry is the op-type
/// default; its evaluator is the weak `vol_r ≤ vol_a + tol` form, which
/// this satisfies.
///
/// Un-quarantined 2026-07-30: the amendment-14 split is ALWAYS-ON since
/// the inc-3.2d corpus flip (the only category change was this case's
/// ERROR → SUPPORTED_CORRECT conversion).
#[test]
fn r0099_chain_conversion_oracle() {
    let (a, body, [v1, v2, v3]) = run_chain()
        .unwrap_or_else(|e| panic!("R0099 chain must complete once the split lands: {e}"));
    validate_solid(&a, body).expect("final solid must validate");
    // Op-1 boss is a plain cylinder: 1.5% chord-deficit band around the
    // analytic volume (the swiss-cheese band rationale).
    let analytic1 = std::f64::consts::PI * R_BOSS * R_BOSS * D_BOSS;
    assert!(
        (v1 - analytic1).abs() / analytic1 < 0.015,
        "boss volume {v1} outside band of analytic {analytic1}"
    );
    // Op-2 cut is concentric and through: the annulus.
    let analytic2 = std::f64::consts::PI * (R_BOSS * R_BOSS - R_CUT * R_CUT) * D_BOSS;
    assert!(
        (v2 - analytic2).abs() / analytic2 < 0.015,
        "annulus volume {v2} outside band of analytic {analytic2}"
    );
    // Op-3 is the measure-zero cut: the annulus band again, positive.
    assert!(v3 > 0.0, "final volume must be positive: {v3}");
    assert!(
        (v3 - analytic2).abs() / analytic2 < 0.015,
        "post-cut volume {v3} outside band of analytic {analytic2} \
         (true Δv = 0 — see the oracle docs)"
    );
}

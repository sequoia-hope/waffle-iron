//! Regression (PR-5, coplanar cylinder membrane): the user's real spur-gear
//! unioned with a coaxial flange whose outer wall is the SAME cylinder as the
//! gear's central bore. cherchi (PRs 1-4) constructs the coincident-cylinder
//! overlap region with a MULTI-SOLID label; yang Stage-6 must resolve that
//! internal shared sheet via a coincident-cylinder pair detector
//! (`stage0::detect_coincident_cylinder_pairs`) the same way it resolves a
//! planar coplanar overlap "membrane".
//!
//! Before this fix the union failed at Stage-6 reassembly with:
//!   "Auto-union failed: yang-rs: geometric face resolution failed for kept
//!   triangle N (… a multi-solid label with no matching Stage-0 pair plane)."
//!
//! THE GATE (P9 discipline — a 0-error build is NOT success): the combined
//! union must pass the FULL oracle suite (no_self_intersection, watertight,
//! Euler, positive volume) AND span the gear's full z-extent z≈[-0.005, 0.005]
//! (NOT just the flange's [-0.002, 0.002] — that = the gear body silently
//! dropped = FAIL).
use test_harness::{oracle, ModelBuilder};

/// err.waffle is 1.2 MB — too large to vendor into fixtures, so it is loaded
/// by path. It lives at the repo root; from this test's manifest dir that is
/// `../../err.waffle`. When run inside a git worktree the repo root is the
/// worktree root, which also has a copy.
fn err_waffle_json() -> Option<String> {
    let candidates = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../err.waffle").to_string(),
        "/home/claude/workspace/err.waffle".to_string(),
    ];
    for c in candidates {
        if let Ok(s) = std::fs::read_to_string(&c) {
            return Some(s);
        }
    }
    None
}

// QUARANTINED (RED) — progress across PR-5 and PR-6, gear still not clean:
//
//   • PR-5 landed the Stage-6 coincident-cylinder MEMBRANE resolution (the
//     original "no matching Stage-0 pair plane" face-resolution failure).
//   • PR-6 fixed DEFECT 1 conformally: the "Stage-4 relocation region around
//     vertex 4497 is invalid: DegenerateTriangle" sliver. Confirmed cause —
//     31 cap-rim points (cap plane z=±0.002 ∩ the coincident bore/flange
//     cylinder) were minted redundantly by cherchi's exact arrangement,
//     clustered ~1 ULP apart (max sep ~9e-19 at coord scale 5e-3) and left
//     DISTINCT by the curved-input bit-exact weld, so a kept triangle carried
//     two copies of one geometric rim point. PR-6 welds those redundant
//     reconstructions by EXACT IDENTITY, gated on analytic on-cylinder
//     membership (radial dist ~1e-19) + a sub-feature-size cluster band — a
//     conformal merge, NOT a tolerance bucket (touches no planar case, cannot
//     reintroduce the reverted F0057 planar-weld masking).
//
//   • DEFECT 2 remains (the present wall): "reassembled output would be
//     non-2-manifold". Single shell with χ = −1 and a PINCH at vertex 4492
//     (z=0, on the bore cylinder, mid-plug), where 7 gear-bore-wall triangles
//     (InputId A) and 2 flange-wall triangles (InputId B) meet at one shared
//     vertex. ROOT CAUSE is in cherchi-rs (Stage 2), NOT yang-rs: over the
//     overlap band z∈[−0.002,0.002] the gear bore wall and the flange outer
//     wall are the SAME (coincident, opposite-sense) cylinder, yet cherchi's
//     ray-cast in/out labels the bore-wall band inside=[false,false] (kept by
//     the union) instead of inside=[_, true] (interior to the flange, dropped).
//     Both walls' fans therefore survive and pinch. Fixing this requires
//     cherchi to multi-label / correctly classify the coincident-cylinder
//     WALL overlap (not just the cap-rim membrane) — the conformal curved
//     Stage-0/labeling work, out of yang-rs scope. Forcing past it is
//     silent-wrong (P9). Un-ignore when that lands.
#[ignore = "M8-cyl: PR-6 fixed the Stage-4 DegenerateTriangle (cap-rim conformal \
            weld); the gear now reaches reassembly but a coincident-cylinder \
            WALL labeling defect in cherchi-rs (bore-wall band mis-classified \
            inside=[false,false]) pinches the seam (χ=-1). Needs cherchi \
            coincident-cylinder in/out labeling — out of yang-rs scope"]
#[test]
fn gear_flange_union_builds_full_height() {
    let Some(json) = err_waffle_json() else {
        eprintln!("err.waffle not found on any candidate path; skipping");
        return;
    };
    let mut b = ModelBuilder::kernel_v2();
    b.load(&json).expect("load err.waffle");

    let errs: Vec<String> = b
        .engine_errors()
        .iter()
        .map(|(i, m)| format!("{i}: {m}"))
        .collect();
    let warns: Vec<String> = b.engine_warnings().to_vec();
    eprintln!("engine_errors: {errs:?}");
    eprintln!("engine_warnings: {warns:?}");
    assert!(errs.is_empty(), "gear union produced errors: {errs:?}");
    assert!(
        !warns.iter().any(|w| w.contains("Auto-union failed")),
        "gear union deferred to standalone (Auto-union failed): {warns:?}"
    );

    let mesh = b
        .tessellate_combined("Extrude")
        .expect("tessellate combined gear union");

    // Combined bbox z-extent — must span the FULL gear height, not just flange.
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in mesh.vertices.chunks_exact(3) {
        let z = v[2] as f64;
        zmin = zmin.min(z);
        zmax = zmax.max(z);
    }
    eprintln!("combined bbox z = [{zmin}, {zmax}]");

    // Full per-oracle verdicts.
    let verdicts = oracle::run_all_mesh_checks(&mesh);
    for v in &verdicts {
        eprintln!("oracle {}: pass={} — {}", v.oracle_name, v.passed, v.detail);
    }

    // The gear spans z≈[-0.005, 0.005]; the flange alone is [-0.002, 0.002].
    assert!(
        zmin <= -0.0045 && zmax >= 0.0045,
        "combined union does not span gear full height (z=[{zmin}, {zmax}]); \
         gear body likely dropped"
    );

    for v in &verdicts {
        assert!(v.passed, "oracle {} FAILED: {}", v.oracle_name, v.detail);
    }
}

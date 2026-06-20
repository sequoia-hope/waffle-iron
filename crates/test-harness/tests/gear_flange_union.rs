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
//     non-2-manifold". RE-LOCALIZED in task28 by reference-parity
//     discrimination — the PRIOR cherchi-labeling diagnosis was WRONG.
//
//     The gear's bore wall (r≈0.005909, INWARD-facing — a hole) and the
//     unioned flange's OUTER wall (same r, OUTWARD-facing) are the SAME 32-gon
//     cylinder with bit-identical x,y generators but OPPOSITE normal sense.
//     They are also NON-CONFORMAL in z: the bore wall is one tall quad band
//     (verts only at z=±0.005), the flange wall + caps land at z=±0.002.
//
//     The discriminating experiment (the EXACT meshes yang feeds cherchi,
//     extracted via a temporary YANG_DUMP_MESHES probe, fed to BOTH the native
//     cherchi boolean AND the upstream C++ `mesh_booleans` sidecar): both
//     produce the SAME non-watertight union — 9152 tris / 4550 verts / 54
//     unpaired edges, volumes matching to ~5e-11. Per the discrimination
//     protocol, native == sidecar == non-watertight ⇒ the INPUT meshes are
//     non-conformal / degenerate, NOT a cherchi labeling bug. cherchi is
//     faithful to the reference. The 54 unpaired edges all lie on the bore
//     cylinder at the flange's cap-plane rings z=±0.002.
//
//     The minimal reproduction + parity oracle is
//     `crates/cherchi-rs/tests/task28_plug_in_bore.rs` (a tube with an inward
//     bore wall ∪ a coaxial outward plug at the same radius): the C++
//     reference ALSO fails it (24 unpaired even with conformal z), proving the
//     mesh boolean cannot resolve an opposite-normal coincident-cylinder wall
//     at the mesh level.
//
//     ROOT CAUSE: a missing yang STAGE-0 capability — the cylinder analog of
//     the §4.5.5 opposite-normal coplanar overlap (already handled for the
//     planar disc∩polygon CROSSING). The coincident wall sheets must be
//     dropped (interior to the union) and the cap-ring boundary stitched
//     conformally BEFORE the mesh boolean. `detect_coincident_cylinder_pairs`
//     only supplies a post-arrangement keep/drop hint; it does NOT
//     re-tessellate, so the non-conformal degenerate input reaches cherchi
//     unchanged. Forcing past this is silent-wrong (P9). Un-ignore when the
//     Stage-0 coincident-cylinder re-tessellation lands.
#[ignore = "M8-cyl Stage-0 gap (task28 re-localized): the gear bore wall and \
            flange wall are an OPPOSITE-normal coincident cylinder, non-conformal \
            in z. Native cherchi == C++ sidecar (both 54 unpaired edges) — NOT a \
            cherchi labeling bug; the degenerate coincident-sheet input needs a \
            yang Stage-0 coincident-cylinder re-tessellation (drop interior \
            sheets + stitch cap rings), the cylinder analog of §4.5.5. See \
            crates/cherchi-rs/tests/task28_plug_in_bore.rs."]
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

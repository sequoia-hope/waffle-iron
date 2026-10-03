//! P0013 — the star tip that clears the boss cylinder by 9.28 µm, under a
//! 14-gon Stage-1 mesh whose sagitta is 542 µm.
//!
//! Spec: `specs/yang_p0013_tip_land_under_the_chord.md`. Two independent
//! gaps stack on this document (both measured, §2):
//!
//! * **P1 / P3 (yang-rs)** — the exact star is ENTIRELY inside the exact
//!   cylinder, so the mesh-level tip crossing is a Yang §4.3.3 Case IV
//!   ("the meshes detect intersections that do not exist between the
//!   surfaces"). The §4.3.3 density guard recognises the configuration but
//!   its clearance was a 65-sample Lipschitz lower bound, `min_d − len/128`,
//!   which collapses to "touching" for a 9.28 µm land on a 5.18 mm edge —
//!   so it derived nothing. With the EXACT cylinder clearance (P1) it
//!   derives N = 152, the density the land demands.
//! * **P4 (kernel-v2)** — the output cap's star HOLE clears its circular
//!   boundary by that same 9.28 µm, which is under the canonical render
//!   sagitta (N = 71 ⇒ 21.2 µm): the inscribed chord polygon cuts INSIDE
//!   the hole, the two constraint rings cross, and the exact CDT rightly
//!   refuses the ring. The render density is now derived from the face's own
//!   loop clearance (N = 108 here).
//!
//! P1 + P4 are landed; the remaining step is flipping the §4.3.3 guard ON by
//! default, which needs the full-corpus proof its 2026-08-27 broad-form flip
//! was refused on (spec §5). So this pin runs the guard through its dev gate
//! and asserts the conversion the two landed halves deliver.
//!
//! Run: `cargo test -p test-harness --release --test p0013_tip_land_conformity`

use std::fs;
use std::path::PathBuf;

use test_harness::ModelBuilder;

/// The campaign wall: kernel-v2's exact planar CDT refusing the output ring.
const CDT_REJECT_WALL: &str = "ring rejected by CDT";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// Replays P0013 with the §4.3.3 Case-IV density guard on its dev gate, and
/// pins that neither the boolean nor the render tessellation walls.
///
/// RED before this increment on BOTH halves, each on its own:
/// * without P1 the guard derives nothing (it read the land as "touching")
///   and `boolean_subtract` fails with the CDT reject on FaceId(19);
/// * without P4 the refined body's cap hole still pokes through the
///   71-gon render chord polygon and the SAME reject fires one stage later
///   (measured: it survives even a d_ε/256 operand mesh).
#[test]
fn p0013_tip_land_survives_both_chord_bands() {
    // Dev gate for the §4.3.3 Case-IV derived-density guard (spec
    // `yang_433_case_iv_corner_phantom.md` inc-1). Set for this binary only;
    // this file holds exactly one test so no other test observes it.
    //
    // SAFETY: single-threaded point in this test binary's lifetime — one
    // test, set before any engine call, never cleared.
    unsafe { std::env::set_var("YANG_433_GUARD", "1") };

    let waffle_json = fs::read_to_string(assay_dir().join("P0013.waffle"))
        .expect("P0013.waffle must be readable");
    let mut builder = ModelBuilder::kernel_v2();
    builder
        .load(&waffle_json)
        .expect("LoadProject must succeed");

    let failures: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect();
    assert!(
        failures.is_empty(),
        "P0013 must rebuild clean with the §4.3.3 guard on; got:\n  {}",
        failures.join("\n  ")
    );

    let tess = builder.tessellate_last_with_tol(0.01);
    let err = tess.as_ref().err().map(|e| e.to_string());
    assert!(
        !err.as_deref().unwrap_or("").contains(CDT_REJECT_WALL),
        "the output cap ring must not be refused by the CDT: {err:?}"
    );
    let mesh = tess.expect("the converted body must tessellate");
    assert!(
        !mesh.indices.is_empty(),
        "the converted body must carry triangles"
    );
}

//! RED oracles for M8 Stage-0 inputcheck-clean overlap emission
//! (spec `specs/m8_stage0_inputcheck_clean_emission.md` §5/§6).
//!
//! The fixtures are the EXACT operand meshes yang-rs Stage-0 handed to the
//! native boolean for the acceptance cases' defective ops (banked from the
//! Increment-0 diagnosis, `YANG_STAGE0_DUMP_DIR`):
//!
//! - `r0046_stage0_emission_{a,b}.obj` — the disc×polygon crossing pair
//!   (mechanism M-B resolution collapse: 4/10 `[u,u,v]` degenerate tris,
//!   plus the small M-A split drop: 10 boundary edges + a pinch vertex on B).
//! - `f0063_stage0_emission_{a,b}.obj` — the gear-outline pair (mechanism
//!   M-A cluster-domain split drop at scale: 567 boundary edges + 391
//!   improper T-junction contacts on B; A is clean).
//!
//! RED today: the census finds the measured defects. GREEN when the Stage-0
//! emission fix re-banks these fixtures from the corrected emission and both
//! operands of each pair are five-axiom clean. (Re-banking from the fixed
//! emission is the spec's §6 fixture-refresh procedure — a fixture refresh,
//! never a tolerance change; the E2E campaign trackers keep the end-to-end
//! walls honest independently of the bank.)
//!
//! The `#[ignore]`d sidecar tests are the binding reference (oracle-vs-
//! oracle, spec §2 calibration): the native census verdict must agree with
//! `mesh_booleans_inputcheck` at the clean/dirty level on the same meshes.

use std::path::PathBuf;
use std::time::Duration;

use cherchi_rs::inputcheck::{census, NativeInputCheck};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::{obj, SidecarError};

fn fixture(name: &str) -> Mesh {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    obj::read_obj(&path).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn assert_operand_clean(name: &str, c: &NativeInputCheck) {
    assert!(
        c.clean(),
        "M8 Stage-0 RED — emitted operand `{name}` violates the Cherchi input \
         contract (spec m8_stage0_inputcheck_clean_emission §5 I1):\n{}",
        c.summary()
    );
}

/// M-B + small M-A pair (R0046 defective subtract). RED: A carries 4
/// index-degenerate tris; B carries 10 + 10 boundary edges + 1 pinch vertex.
#[test]
fn native_census_r0046_emission_operands() {
    for side in ["a", "b"] {
        let name = format!("r0046_stage0_emission_{side}.obj");
        let m = fixture(&name);
        let c = census(&m.verts, &m.tris);
        assert_operand_clean(&name, &c);
    }
}

/// M-A-at-scale pair (F0063 defective union op0). RED: B carries 567
/// boundary edges + 391 improper pairs + 49 pinch vertices; A is clean
/// (also asserted, as a regression guard).
#[test]
fn native_census_f0063_emission_operands() {
    for side in ["a", "b"] {
        let name = format!("f0063_stage0_emission_{side}.obj");
        let m = fixture(&name);
        let c = census(&m.verts, &m.tris);
        assert_operand_clean(&name, &c);
    }
}

/// Oracle-vs-oracle (spec §2 calibration, binding reference): the sidecar
/// `mesh_booleans_inputcheck` verdict must agree with the native census at
/// the clean/dirty level on every banked operand — and after GREEN, both
/// must report all-pass. Loud-panics if the binary is missing (a silently
/// skipped reference oracle is a false GREEN, P9).
#[test]
#[ignore = "requires the C++ mesh_booleans_inputcheck sidecar (CHERCHI2022_INPUTCHECK_BIN \
            or scripts/build_sidecars.sh); binding reference for the census verdicts"]
fn sidecar_inputcheck_agrees_on_banked_operands() {
    for name in [
        "r0046_stage0_emission_a.obj",
        "r0046_stage0_emission_b.obj",
        "f0063_stage0_emission_a.obj",
        "f0063_stage0_emission_b.obj",
    ] {
        let m = fixture(name);
        let native = census(&m.verts, &m.tris);
        let side = match cherchi_sidecar_rs::inputcheck(&m, Duration::from_secs(60)) {
            Ok(r) => r,
            Err(SidecarError::BinaryNotFound { path }) => panic!(
                "inputcheck oracle unavailable: binary not found at {} \
                 (set CHERCHI2022_INPUTCHECK_BIN or build per \
                 docs/sidecar/cherchi2022_build_guide.md). Refusing to skip.",
                path.display()
            ),
            Err(e) => panic!("sidecar inputcheck failed on {name}: {e}"),
        };
        assert_eq!(
            side.all_pass(),
            native.clean(),
            "oracle disagreement on {name}: sidecar {side:?} vs native\n{}",
            native.summary()
        );
        // GREEN target (spec §5 I1): both oracles all-pass on the re-banked
        // fixed emission. RED today by the clean/dirty agreement above.
        assert!(
            side.all_pass(),
            "M8 Stage-0 RED — sidecar reports {name} violates the input contract: {side:?}"
        );
    }
}

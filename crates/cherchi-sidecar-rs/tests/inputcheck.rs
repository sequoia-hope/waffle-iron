//! M1: reference-oracle harness against `mesh_booleans_inputcheck`.
//!
//! These tests exercise the new `cherchi_sidecar_rs::inputcheck` function,
//! which runs the upstream `mesh_booleans_inputcheck` C++ binary as a
//! reference oracle for the Cherchi 2022 §3 input axioms (manifold,
//! watertight, local orientation, global orientation, intersection-free).
//!
//! Self-skip when the binary isn't built/available. Set
//! `CHERCHI2022_INPUTCHECK_BIN` or build per
//! `docs/sidecar/cherchi2022_build_guide.md`.
//!
//! IMPORTANT (verified CLI facts): the binary prints its 5-line verdict to
//! **stdout** (not stderr) and exits **0 regardless of pass/fail**, so the
//! verdict must be parsed from stdout, not the exit code.

use std::time::Duration;

use cad_primitives::Point3;
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::{inputcheck, SidecarError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A unit cube at the origin with correct **outward** triangle winding
/// (CCW viewed from outside). Identical to the smoke-test fixture which is
/// already accepted by the boolean binary.
fn outward_unit_cube() -> Mesh {
    let verts = vec![
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(1.0, 1.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(0.0, 1.0, 1.0),
    ];
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [0, 4, 7],
        [0, 7, 3],
    ];
    Mesh::new(verts, tris)
}

/// The same cube with every triangle's winding reversed: still a closed,
/// manifold, watertight, locally-consistent mesh — but globally inverted
/// (inside-out). This is exactly the failure mode M1 fixes in yang-rs.
fn inside_out_unit_cube() -> Mesh {
    let m = outward_unit_cube();
    let tris = m
        .tris
        .iter()
        .map(|t| [t[0], t[2], t[1]])
        .collect::<Vec<_>>();
    Mesh::new(m.verts, tris)
}

#[test]
fn outward_cube_passes_all_inputcheck_axioms() {
    let report = match inputcheck(&outward_unit_cube(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!(
                "[cherchi-sidecar-rs inputcheck] SKIP: inputcheck binary not found; \
                 set CHERCHI2022_INPUTCHECK_BIN"
            );
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    assert!(report.manifold, "outward cube should be manifold");
    assert!(report.watertight, "outward cube should be watertight");
    assert!(
        report.local_orientation,
        "outward cube should pass local orientation"
    );
    assert!(
        report.global_orientation,
        "outward cube should pass global orientation"
    );
    assert!(
        report.intersection_free,
        "outward cube should be intersection-free"
    );
    assert!(report.all_pass(), "outward cube should pass ALL axioms");
}

#[test]
fn inside_out_cube_fails_global_orientation() {
    let report = match inputcheck(&inside_out_unit_cube(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!(
                "[cherchi-sidecar-rs inputcheck] SKIP: inputcheck binary not found; \
                 set CHERCHI2022_INPUTCHECK_BIN"
            );
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    // Reversing every winding keeps it manifold/watertight but flips the
    // global orientation: the lone axiom that must now fail.
    assert!(
        !report.global_orientation,
        "inside-out cube must FAIL global orientation"
    );
    assert!(
        !report.all_pass(),
        "inside-out cube must not pass all axioms"
    );
}

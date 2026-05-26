//! End-to-end smoke test for the Cherchi 2022 `mesh_booleans` sidecar.
//!
//! Builds two overlapping unit cubes in memory, writes them to OBJ,
//! invokes the C++ binary on `intersection`, parses the output, and
//! asserts the result is structurally well-formed (non-empty verts +
//! tris). Does NOT assert byte equality — Cherchi 2022 may use TBB
//! internally; we avoid flakes by not asserting determinism we don't
//! yet need.
//!
//! Self-skips (passes silently) when the binary isn't built. Set
//! `CHERCHI2022_BIN` or build per `docs/sidecar/cherchi2022_build_guide.md`
//! to make the test run for real.

mod common;

use std::process::Command;
use std::time::Duration;

use common::obj::{self, TriMesh};
use common::sidecar::{cherchi_bin, run_with_timeout, TimedRun};

/// Build a unit cube with the SW-bottom corner at `origin`. 8 vertices,
/// 12 triangles, all CCW with outward-pointing normals (right-hand rule).
fn unit_cube_at(origin: [f64; 3]) -> TriMesh {
    let [x, y, z] = origin;
    let verts = vec![
        [x, y, z],                   // 0: bottom-SW
        [x + 1.0, y, z],             // 1: bottom-SE
        [x + 1.0, y + 1.0, z],       // 2: bottom-NE
        [x, y + 1.0, z],             // 3: bottom-NW
        [x, y, z + 1.0],             // 4: top-SW
        [x + 1.0, y, z + 1.0],       // 5: top-SE
        [x + 1.0, y + 1.0, z + 1.0], // 6: top-NE
        [x, y + 1.0, z + 1.0],       // 7: top-NW
    ];
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1], // bottom (-Z)
        [4, 5, 6],
        [4, 6, 7], // top    (+Z)
        [0, 1, 5],
        [0, 5, 4], // south  (-Y)
        [2, 3, 7],
        [2, 7, 6], // north  (+Y)
        [1, 2, 6],
        [1, 6, 5], // east   (+X)
        [0, 4, 7],
        [0, 7, 3], // west   (-X)
    ];
    (verts, tris)
}

/// Deterministic per-test temp dir under the OS tempdir. Wiped on entry
/// so leftover files from a prior run don't bleed into the next.
fn fresh_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cherchi-rs-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cherchi_sidecar_round_trip_intersection() {
    let Some(bin) = cherchi_bin() else {
        return;
    };

    let (verts_a, tris_a) = unit_cube_at([0.0, 0.0, 0.0]);
    let (verts_b, tris_b) = unit_cube_at([0.5, 0.0, 0.0]);

    let tmp = fresh_temp_dir("smoke_intersection");
    let a = tmp.join("a.obj");
    let b = tmp.join("b.obj");
    let out = tmp.join("out.obj");

    obj::write_obj(&a, &verts_a, &tris_a).unwrap();
    obj::write_obj(&b, &verts_b, &tris_b).unwrap();

    let mut cmd = Command::new(&bin);
    cmd.arg("intersection").arg(&a).arg(&b).arg(&out);

    match run_with_timeout(cmd, Duration::from_secs(30)) {
        TimedRun::Completed(o) if o.status.success() => {
            let (out_verts, out_tris) = obj::read_obj(&out).unwrap();
            assert!(
                !out_verts.is_empty() && !out_tris.is_empty(),
                "intersection of overlapping unit cubes produced empty mesh: \
                 {} verts, {} tris (stderr=\n{})",
                out_verts.len(),
                out_tris.len(),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        TimedRun::Completed(o) => panic!(
            "mesh_booleans exited non-zero ({:?}); stderr=\n{}",
            o.status,
            String::from_utf8_lossy(&o.stderr)
        ),
        TimedRun::TimedOut => panic!("mesh_booleans timed out (30s) on unit-cube intersection"),
        TimedRun::SpawnFailed(e) => panic!("spawn failed: {e}"),
    }
}

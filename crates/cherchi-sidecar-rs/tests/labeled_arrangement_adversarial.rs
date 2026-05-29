//! Adversarial M2 audit: try to BREAK the patched `LabeledArrangement`
//! producer. Structural assertions only; no production code is modified.
//!
//! See `specs/yang_m2_labeled_arrangement.md`. These tests extend the C1-C5
//! coverage in `labeled_arrangement.rs` with the attacks the Adversary role is
//! responsible for:
//!
//!   A1 — label<->triangle ALIGNMENT, verified GEOMETRICALLY (independent of
//!        the C++ dump's own 1:1 claim): every solid-A-only triangle must lie
//!        on a face plane of cube A, every solid-B-only triangle on B.
//!   A2 — in/out keep-rule oracle for ALL FOUR ops (the existing test covers
//!        only Union + Subtract); plus a non-triviality check that Subtract
//!        differs from Union.
//!   A3 — coplanar multi-attribution is REAL: every surface.len()==2 triangle
//!        in the shared-face case lies on the shared plane x==1.
//!   A4 — determinism robustness: 3x repeat of labeled_arrangement AND 3x of
//!        the stock boolean(), all byte-identical.
//!   A6 — producer error handling: a fake binary that emits a malformed
//!        .labels file must yield SidecarError::LabelsParse, never a panic or
//!        silent wrong result; missing binary -> BinaryNotFound.
//!
//! All real-binary tests self-skip when the binary isn't built/available.

use std::sync::Mutex;
use std::time::Duration;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::{labeled_arrangement, InputId, SidecarBoolean, SidecarError};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Process-wide guard for the `CHERCHI2022_BIN` env var. `std::env::set_var`
/// mutates PROCESS-GLOBAL state, and cargo runs a test binary's tests on
/// multiple threads by default. Without serialization, an A6 test that points
/// the env at a fake/missing binary races the A1-A4 tests that resolve the real
/// binary at call time. Every test that reads OR writes `CHERCHI2022_BIN` takes
/// this lock for its whole duration, making the suite race-free under default
/// `cargo test` parallelism (no `--test-threads=1` needed).
///
/// NOTE (finding, not fixed here — production untouched): the sibling
/// `labeled_arrangement.rs` C5 test mutates `CHERCHI2022_BIN` with no such
/// guard. It only avoids flaking because it is that file's sole env-mutator;
/// adding any future env-mutating test there would resurrect the same race.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned unit cube with min-corner at `origin`. 8 verts, 12 outward
/// tris. Mirrors the fixture in `labeled_arrangement.rs`.
fn unit_cube_at(origin: [f64; 3]) -> Mesh {
    let [x, y, z] = origin;
    let verts = vec![
        p(x, y, z),
        p(x + 1.0, y, z),
        p(x + 1.0, y + 1.0, z),
        p(x, y + 1.0, z),
        p(x, y, z + 1.0),
        p(x + 1.0, y, z + 1.0),
        p(x + 1.0, y + 1.0, z + 1.0),
        p(x, y + 1.0, z + 1.0),
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

/// Acquire the process-wide env guard for the duration of a test. Tolerates a
/// poisoned mutex (a prior test panicked while holding it): the env state is
/// always restored by the holders below, so the data is not logically corrupt.
fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn try_or_skip<T>(r: Result<T, SidecarError>, ctx: &str) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[adversarial {ctx}] SKIP: binary not found; set CHERCHI2022_BIN");
            None
        }
        Err(e) => panic!("labeled_arrangement ({ctx}) failed unexpectedly: {e:?}"),
    }
}

// ---- canonicalization helpers (shared shape with labeled_arrangement.rs) ----

fn canon_tri(mesh: &Mesh, tri: [u32; 3]) -> [[u64; 3]; 3] {
    let bits = |pt: &Point3| [pt.x().to_bits(), pt.y().to_bits(), pt.z().to_bits()];
    let mut coords = [
        bits(&mesh.verts[tri[0] as usize]),
        bits(&mesh.verts[tri[1] as usize]),
        bits(&mesh.verts[tri[2] as usize]),
    ];
    coords.sort();
    coords
}

fn canon_multiset(mesh: &Mesh) -> Vec<[[u64; 3]; 3]> {
    let mut v: Vec<_> = mesh.tris.iter().map(|&t| canon_tri(mesh, t)).collect();
    v.sort();
    v
}

// ========================================================================
// A1 — Label<->triangle ALIGNMENT, verified geometrically.
// ========================================================================

/// A triangle "lies in one of cube X's 6 face planes" iff all three of its
/// vertices share a single axis-plane coordinate: there is an axis `ax` and a
/// plane value `pv in {lo[ax], lo[ax]+1}` such that every vertex has
/// `coord[ax] == pv`. (Being individually on *some* plane is too weak — a
/// sub-triangle of an interpenetration could touch different planes per vertex.
/// A genuine face-plane triangle is coplanar with exactly one axis plane.)
fn tri_in_a_cube_plane(mesh: &Mesh, tri: [u32; 3], lo: [f64; 3]) -> bool {
    let vs = [
        &mesh.verts[tri[0] as usize],
        &mesh.verts[tri[1] as usize],
        &mesh.verts[tri[2] as usize],
    ];
    let coord = |v: &Point3, ax: usize| match ax {
        0 => v.x(),
        1 => v.y(),
        _ => v.z(),
    };
    for (ax, &l) in lo.iter().enumerate() {
        for pv in [l, l + 1.0] {
            if vs.iter().all(|v| coord(v, ax) == pv) {
                return true;
            }
        }
    }
    false
}

/// A1: for two interpenetrating cubes A@[0,0,0], B@[0.5,0.5,0.5], every
/// triangle the labels attribute to *solid A only* (surface == [InputId(0)])
/// must be coplanar with one of A's 6 face planes, and every *solid B only*
/// triangle coplanar with one of B's planes. If a triangle's label says A but
/// its geometry lies only on B (or neither), the `.labels` line is misaligned
/// with the `.obj` triangle (off-by-one / reorder) — the highest-risk claim.
#[test]
fn a1_label_matches_geometry_for_each_solid() {
    let _g = env_guard();
    let lo_a = [0.0, 0.0, 0.0];
    let lo_b = [0.5, 0.5, 0.5];
    let a = unit_cube_at(lo_a);
    let b = unit_cube_at(lo_b);
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "a1") else {
        return;
    };

    let n = la.mesh.tris.len();
    let mut checked_a = 0usize;
    let mut checked_b = 0usize;
    for t in 0..n {
        let surf = &la.surface[t];
        if surf.len() != 1 {
            // Multi-attributed tris (none expected here, but don't assume):
            // skip — A1 targets single-solid attribution alignment.
            continue;
        }
        let tri = la.mesh.tris[t];
        match surf[0] {
            InputId(0) => {
                assert!(
                    tri_in_a_cube_plane(&la.mesh, tri, lo_a),
                    "tri {t} labeled solid A but not coplanar with any A face plane; \
                     verts {:?}",
                    tri.map(|i| {
                        let v = &la.mesh.verts[i as usize];
                        (v.x(), v.y(), v.z())
                    })
                );
                checked_a += 1;
            }
            InputId(1) => {
                assert!(
                    tri_in_a_cube_plane(&la.mesh, tri, lo_b),
                    "tri {t} labeled solid B but not coplanar with any B face plane; \
                     verts {:?}",
                    tri.map(|i| {
                        let v = &la.mesh.verts[i as usize];
                        (v.x(), v.y(), v.z())
                    })
                );
                checked_b += 1;
            }
            InputId(other) => panic!("tri {t}: unexpected solid id {other} (num_inputs=2)"),
        }
    }
    // Both solids must contribute single-attributed tris, else the test is vacuous.
    assert!(
        checked_a > 0 && checked_b > 0,
        "expected single-solid tris from BOTH A ({checked_a}) and B ({checked_b})"
    );
}

// ========================================================================
// A2 — keep-rule oracle for ALL FOUR ops + Subtract != Union non-triviality.
// ========================================================================

/// A2: the existing C2 test covers only Union + Subtract. Extend the
/// keep_set-vs-stock-boolean canonical-multiset oracle to Intersect AND Xor as
/// well (and re-cover Union + Subtract for completeness). If keep_set(op)
/// disagrees with the stock op for ANY op, it is a keep-rule bug in `keep_set`
/// or a mislabeled in/out.
#[test]
fn a2_keep_set_matches_stock_boolean_all_four_ops() {
    let _g = env_guard();
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);

    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[adversarial a2] SKIP: binary not found; set CHERCHI2022_BIN");
        return;
    };
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "a2") else {
        return;
    };

    for op in [
        BoolOp::Union,
        BoolOp::Intersect,
        BoolOp::Subtract,
        BoolOp::Xor,
    ] {
        let stock = sb.boolean(&a, &b, op).expect("stock boolean failed");
        let stock_set = canon_multiset(&stock);

        let keep = la.keep_set(op);
        let mut kept: Vec<_> = keep
            .iter()
            .map(|&i| canon_tri(&la.mesh, la.mesh.tris[i]))
            .collect();
        kept.sort();

        assert_eq!(
            kept.len(),
            stock_set.len(),
            "{op:?}: keep_set tri count ({}) != stock result tri count ({})",
            kept.len(),
            stock_set.len()
        );
        assert_eq!(
            kept, stock_set,
            "{op:?}: keep_set canonical triangle multiset must equal stock boolean result"
        );
    }
}

/// A2 non-triviality: Subtract must select a DIFFERENT triangle set than Union
/// for two overlapping cubes — otherwise the keep rules might be collapsing to
/// a trivial "keep all surface" with no in/out discrimination.
#[test]
fn a2_subtract_differs_from_union() {
    let _g = env_guard();
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.0, 0.0]); // offset in x only
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "a2-diff") else {
        return;
    };

    let union = la.keep_set(BoolOp::Union);
    let subtract = la.keep_set(BoolOp::Subtract);
    assert_ne!(
        union, subtract,
        "Subtract and Union must select different triangle sets (non-trivial in/out)"
    );
    // Both must be non-empty (a degenerate empty keep set would also "differ").
    assert!(!union.is_empty(), "union keep set must be non-empty");
    assert!(!subtract.is_empty(), "subtract keep set must be non-empty");
}

// ========================================================================
// A3 — coplanar multi-attribution is REAL (lies on the shared plane).
// ========================================================================

/// A3: cubes A@[0,0,0] and B@[1,0,0] share the x==1 plane. Every triangle the
/// labels mark multi-attributed (surface.len()==2) must have all three vertices
/// on x==1. A multi-bit label on a triangle NOT at x==1 is a labeling bug.
#[test]
fn a3_coplanar_multi_attribution_lies_on_shared_plane() {
    let _g = env_guard();
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([1.0, 0.0, 0.0]);
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "a3") else {
        return;
    };

    let n = la.mesh.tris.len();
    let mut multi = 0usize;
    for t in 0..n {
        if la.surface[t].len() < 2 {
            continue;
        }
        multi += 1;
        // Multi-attributed surface must reference exactly the two distinct
        // solids 0 and 1.
        let mut ids: Vec<u32> = la.surface[t].iter().map(|&InputId(i)| i).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids,
            vec![0, 1],
            "tri {t}: multi-attribution must name exactly solids 0 and 1, got {:?}",
            la.surface[t]
        );
        let tri = la.mesh.tris[t];
        for &vi in &tri {
            let v = &la.mesh.verts[vi as usize];
            assert_eq!(
                v.x(),
                1.0,
                "tri {t}: multi-attributed but vertex {vi} not on shared plane x==1 \
                 (got x={})",
                v.x()
            );
        }
    }
    assert!(
        multi >= 1,
        "expected >=1 multi-attributed tri on the shared face"
    );
}

// ========================================================================
// A4 — determinism robustness (3x, and stock boolean 3x).
// ========================================================================

/// A4: run labeled_arrangement 3x on the overlapping-cubes case; all results
/// (mesh + every label vec) must be byte-identical. Single-thread TBB pinning
/// should make this hold not by luck.
#[test]
fn a4_labeled_arrangement_deterministic_over_three_runs() {
    let _g = env_guard();
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);

    let Some(first) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "a4#1") else {
        return;
    };
    for run in 2..=3 {
        let next = labeled_arrangement(&a, &b, TIMEOUT)
            .unwrap_or_else(|e| panic!("run {run} failed after run 1 succeeded: {e:?}"));
        assert_eq!(first.mesh.verts, next.mesh.verts, "run {run}: verts differ");
        assert_eq!(first.mesh.tris, next.mesh.tris, "run {run}: tris differ");
        assert_eq!(first.surface, next.surface, "run {run}: surface differs");
        assert_eq!(first.inside, next.inside, "run {run}: inside differs");
        assert_eq!(first.patch, next.patch, "run {run}: patch differs");
        assert_eq!(
            first.num_inputs, next.num_inputs,
            "run {run}: num_inputs differs"
        );
    }
}

/// A4: the stock `boolean()` must ALSO be deterministic now (same TBB fix), so
/// downstream parity comparisons are stable. 3x identical canonical multisets.
#[test]
fn a4_stock_boolean_deterministic_over_three_runs() {
    let _g = env_guard();
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);

    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[adversarial a4-stock] SKIP: binary not found; set CHERCHI2022_BIN");
        return;
    };
    let first = sb.boolean(&a, &b, BoolOp::Union).expect("run 1 failed");
    let first_set = canon_multiset(&first);
    for run in 2..=3 {
        let next = sb
            .boolean(&a, &b, BoolOp::Union)
            .unwrap_or_else(|e| panic!("stock run {run} failed: {e:?}"));
        assert_eq!(
            canon_multiset(&next),
            first_set,
            "stock boolean run {run}: canonical multiset differs (non-deterministic)"
        );
    }
}

// ========================================================================
// A6 — producer error handling via a fake binary.
// ========================================================================

/// Build a fake "mesh_booleans" shell script at `script_path` that, when run
/// with `<op> a.obj b.obj out.obj` and CHERCHI_DUMP_LABELS=<base>, writes:
///   - out.obj      (a trivial valid OBJ; the producer also reads arr.obj)
///   - <base>.obj   (a valid 1-triangle arrangement OBJ)
///   - <base>.labels (CONTENT controlled by `labels_body`)
/// and exits 0. This lets us drive `labeled_arrangement`'s parser with a
/// malformed `.labels` without needing the real C++ binary.
fn write_fake_binary(script_path: &std::path::Path, labels_body: &str) {
    use std::os::unix::fs::PermissionsExt;
    // $4 is out.obj; CHERCHI_DUMP_LABELS is the arr base.
    let arr_obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    let script = format!(
        r#"#!/usr/bin/env bash
set -e
out="$4"
base="$CHERCHI_DUMP_LABELS"
printf '%s' '{arr_obj}' > "$out"
printf '%s' '{arr_obj}' > "${{base}}.obj"
cat > "${{base}}.labels" <<'LABELS_EOF'
{labels_body}
LABELS_EOF
exit 0
"#
    );
    std::fs::write(script_path, script).expect("write fake binary");
    let mut perms = std::fs::metadata(script_path)
        .expect("stat fake binary")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(script_path, perms).expect("chmod fake binary");
}

fn with_fake_bin<R>(labels_body: &str, f: impl FnOnce() -> R) -> R {
    let dir = std::env::temp_dir().join(format!(
        "cherchi-adversarial-fakebin-{}-{}",
        std::process::id(),
        labels_body.len()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir fakebin");
    let script = dir.join("mesh_booleans");
    write_fake_binary(&script, labels_body);

    // Hold the process-wide env lock across the whole mutate-run-restore window
    // so concurrent real-binary tests never observe the fake path.
    let _g = env_guard();
    let saved = std::env::var("CHERCHI2022_BIN").ok();
    std::env::set_var("CHERCHI2022_BIN", &script);
    let r = f();
    match saved {
        Some(v) => std::env::set_var("CHERCHI2022_BIN", v),
        None => std::env::remove_var("CHERCHI2022_BIN"),
    }
    r
}

/// A6: a malformed `.labels` (header claims 1 tri, but the bit token is not a
/// number) must yield `SidecarError::LabelsParse`, never a panic or silent
/// success. Drives the private parser through the public producer via a fake
/// binary. NOTE: env mutation is process-global; these A6 tests are marked
/// `#[ignore]`-free but kept self-contained (save/restore CHERCHI2022_BIN).
#[test]
fn a6_malformed_labels_yields_labels_parse() {
    // header: "1 2" (1 tri, 2 inputs). tri line: surface bit "x" is not numeric.
    let body = "1 2\nx | 0 | 0";
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let result = with_fake_bin(body, || labeled_arrangement(&a, &b, TIMEOUT));
    match result {
        Err(SidecarError::LabelsParse { .. }) => {}
        other => panic!("expected LabelsParse for malformed labels, got {other:?}"),
    }
}

/// A6: a `.labels` whose tri-line count disagrees with the header num_tris
/// (header says 2, only 1 line present) must also be `LabelsParse` — never a
/// truncated/silently-shorter LabeledArrangement.
#[test]
fn a6_label_line_count_mismatch_yields_labels_parse() {
    // header claims 2 tris but only one tri line follows.
    let body = "2 2\n0 | 0 | 0";
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let result = with_fake_bin(body, || labeled_arrangement(&a, &b, TIMEOUT));
    match result {
        Err(SidecarError::LabelsParse { .. }) => {}
        other => panic!("expected LabelsParse for line-count mismatch, got {other:?}"),
    }
}

/// A6: an `inside` bit index >= num_inputs is out of range and must be rejected
/// as `LabelsParse`, never panic via out-of-bounds indexing.
#[test]
fn a6_inside_bit_out_of_range_yields_labels_parse() {
    // num_inputs=2 but inside lists bit 5.
    let body = "1 2\n0 | 5 | 0";
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let result = with_fake_bin(body, || labeled_arrangement(&a, &b, TIMEOUT));
    match result {
        Err(SidecarError::LabelsParse { .. }) => {}
        other => panic!("expected LabelsParse for out-of-range inside bit, got {other:?}"),
    }
}

/// A6: missing binary -> BinaryNotFound (confirms the self-skip idiom guards
/// the real failure mode, complementing C5).
#[test]
fn a6_missing_binary_yields_binary_not_found() {
    let _g = env_guard();
    let saved = std::env::var("CHERCHI2022_BIN").ok();
    std::env::set_var("CHERCHI2022_BIN", "/definitely/not/a/real/binary/anywhere");
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let result = labeled_arrangement(&a, &b, TIMEOUT);
    match saved {
        Some(v) => std::env::set_var("CHERCHI2022_BIN", v),
        None => std::env::remove_var("CHERCHI2022_BIN"),
    }
    match result {
        Err(SidecarError::BinaryNotFound { .. }) => {}
        other => panic!("expected BinaryNotFound, got {other:?}"),
    }
}

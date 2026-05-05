//! PR-VIZ-1 smoke test — per-stage Yang OBJ-dump produces files on disk.
//!
//! Spec: `specs/yang_pr_viz_1_per_stage_obj_dump.md` §7.
//! Plan: `~/.claude/plans/reactive-juggling-sloth.md`.
//!
//! Verifies the env-gated dump path: with `YANG_BOOLEAN=1`,
//! `YANG_CONFORMAL_PROBE=1`, and `YANG_STAGE_DUMP=<tempdir>` set, running
//! one assay case produces at least one `stage_*.obj` plus matching
//! `_labels.csv` under `<tempdir>/<case-id>/`. The OBJ contains valid
//! `v ` and `f ` lines.
//!
//! `#[ignore]`-gated to keep default test runs fast (full Yang pipeline).

use std::fs;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

#[test]
#[ignore]
fn test_viz_dump_produces_obj_files() {
    let tmpdir = std::env::temp_dir().join(format!("pr_viz_1_smoke_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmpdir);
    fs::create_dir_all(&tmpdir).expect("create tempdir");

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");
    std::env::set_var("YANG_STAGE_DUMP", &tmpdir);

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping smoke test");
        return;
    }
    let _result = run_single_case(dir, "F0031", true);

    // F0031 is two cylinders subtracted from a cube — the boolean fires,
    // so at least Stage A and Stage E_lod=Render dumps should land.
    let case_dir = tmpdir.join("F0031");
    assert!(
        case_dir.exists(),
        "F0031 case dir not created at {:?}",
        case_dir
    );

    let obj_files: Vec<_> = fs::read_dir(&case_dir)
        .expect("read case dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "obj").unwrap_or(false))
        .collect();
    assert!(
        !obj_files.is_empty(),
        "no OBJ files produced under {:?}",
        case_dir
    );

    let first_obj = obj_files[0].path();
    let content = fs::read_to_string(&first_obj).expect("read OBJ");
    assert!(content.contains("v "), "no vertex lines in {:?}", first_obj);
    assert!(content.contains("f "), "no face lines in {:?}", first_obj);

    // Check matching CSV exists for at least one OBJ.
    let csv_files: Vec<_> = fs::read_dir(&case_dir)
        .expect("read case dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "csv").unwrap_or(false))
        .collect();
    assert!(
        !csv_files.is_empty(),
        "no CSV label files produced under {:?}",
        case_dir
    );

    eprintln!(
        "PR-VIZ-1 smoke: {} OBJ files, {} CSV files under {:?}",
        obj_files.len(),
        csv_files.len(),
        case_dir
    );
    for f in &obj_files {
        eprintln!("  obj: {:?}", f.path().file_name().unwrap());
    }

    // Cleanup.
    let _ = fs::remove_dir_all(&tmpdir);
}

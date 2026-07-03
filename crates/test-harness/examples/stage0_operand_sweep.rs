//! Corpus-wide Stage-0 operand five-axiom sweep
//! (spec `specs/m8_stage0_inputcheck_clean_emission.md` §6, dev-only).
//!
//! For every assay case: replay through kernel-v2 with
//! `YANG_STAGE0_DUMP_DIR`, then run the native census
//! (`cherchi_rs::inputcheck::census`) on every dumped operand and its
//! pre-Stage-0 mesh, and emit one TSV row per operand quantifying the
//! introduced-vs-inherited residue corpus-wide. The sidecar binary is NOT
//! invoked here (a subprocess per operand across ~200 cases is the
//! per-fixture oracle's job, not the sweep's).
//!
//! Run (single process, sequential — env mutation is process-global):
//!
//! ```text
//! cargo run -p test-harness --release --example stage0_operand_sweep \
//!     > docs/audits/stage0_operand_inputcheck_sweep_<date>.tsv
//! ```
//!
//! Columns: case, op, operand, verts, tris, clean, then per-defect-class
//! `<post>/<pre>` count pairs (a post>pre pair = Stage-0-INTRODUCED).
//! Progress + per-case status go to stderr; the TSV goes to stdout.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use cherchi_rs::inputcheck::{census, NativeInputCheck};
use cherchi_sidecar_rs::obj::read_obj;
use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn dump_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/stage0_sweep")
}

fn replay(case_id: &str, timeout: Duration) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let dir = assay_dir();
    let id = case_id.to_string();
    std::thread::spawn(move || {
        let r = (|| -> Result<(), String> {
            let waffle_json = fs::read_to_string(dir.join(format!("{id}.waffle")))
                .map_err(|e| format!("read: {e}"))?;
            let mut builder = ModelBuilder::kernel_v2();
            builder
                .load(&waffle_json)
                .map_err(|e| format!("load: {e}"))?;
            Ok(())
        })();
        let _ = tx.send(r);
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(_) => Err(format!("timeout {}s (worker orphaned)", timeout.as_secs())),
    }
}

fn counts(c: &NativeInputCheck) -> [usize; 11] {
    [
        c.nonmanifold_edges.len(),
        c.nonmanifold_verts.len(),
        c.boundary_edges.len(),
        c.misoriented_pairs.len(),
        c.improper_pairs.len(),
        c.unresolved_pairs.len(),
        c.duplicate_tris.len(),
        c.index_degenerate_tris.len(),
        c.collinear_degenerate_tris.len(),
        c.coincident_vert_twins.len(),
        c.unreferenced_verts.len(),
    ]
}

const CLASS_NAMES: [&str; 11] = [
    "nonmanifold_edges",
    "nonmanifold_verts",
    "boundary_edges",
    "misoriented_pairs",
    "improper_pairs",
    "unresolved_pairs",
    "duplicate_tris",
    "index_degenerate",
    "collinear_degenerate",
    "vertex_twins",
    "unreferenced_verts",
];

fn main() {
    let mut cases: Vec<String> = fs::read_dir(assay_dir())
        .expect("assay dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().into_string().ok()?;
            n.strip_suffix(".waffle").map(str::to_string)
        })
        .collect();
    cases.sort();

    print!("case\top\toperand\tverts\ttris\tclean\tintroduced");
    for n in CLASS_NAMES {
        print!("\t{n}");
    }
    println!();

    let mut total_ops = 0usize;
    let mut dirty_ops = 0usize;
    let mut introduced_ops = 0usize;
    for case in &cases {
        let dump = dump_root().join(case);
        let _ = fs::remove_dir_all(&dump);
        fs::create_dir_all(&dump).expect("dump dir");
        std::env::set_var("YANG_STAGE0_DUMP_DIR", &dump);
        let status = replay(case, Duration::from_secs(60));
        std::env::remove_var("YANG_STAGE0_DUMP_DIR");
        if let Err(e) = &status {
            eprintln!("[sweep] {case}: replay {e} (operands dumped so far still censused)");
        }

        let mut stems: Vec<String> = fs::read_dir(&dump)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let n = e.file_name().into_string().ok()?;
                        n.strip_suffix("_meta.txt").map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        stems.sort();

        for stem in &stems {
            let meta =
                fs::read_to_string(dump.join(format!("{stem}_meta.txt"))).unwrap_or_default();
            if !meta.contains("stage0: true") {
                continue; // Stage-1 passthrough — not this sweep's artifact
            }
            for side in ["a", "b"] {
                let Ok(post) = read_obj(&dump.join(format!("{stem}_{side}.obj"))) else {
                    eprintln!("[sweep] {case} {stem} {side}: unreadable post obj");
                    continue;
                };
                let Ok(pre) = read_obj(&dump.join(format!("{stem}_{side}_pre.obj"))) else {
                    eprintln!("[sweep] {case} {stem} {side}: unreadable pre obj");
                    continue;
                };
                let c_post = census(&post.verts, &post.tris);
                let c_pre = census(&pre.verts, &pre.tris);
                let (n_post, n_pre) = (counts(&c_post), counts(&c_pre));
                let introduced = n_post.iter().zip(&n_pre).any(|(p, q)| p > q);
                total_ops += 1;
                if !c_post.clean() {
                    dirty_ops += 1;
                }
                if introduced {
                    introduced_ops += 1;
                }
                print!(
                    "{case}\t{stem}\t{side}\t{}\t{}\t{}\t{}",
                    post.verts.len(),
                    post.tris.len(),
                    c_post.clean(),
                    introduced
                );
                for (p, q) in n_post.iter().zip(&n_pre) {
                    print!("\t{p}/{q}");
                }
                println!();
            }
        }
        let _ = fs::remove_dir_all(&dump);
    }
    eprintln!(
        "[sweep] done: {total_ops} Stage-0 operands censused, {dirty_ops} dirty, \
         {introduced_ops} with Stage-0-introduced defects"
    );
}

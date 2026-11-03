//! Exact-chain COVERAGE census (kernel 1.0 gate 1: every oracle in force as a
//! verdict). A case the exact chain declines is a case it does not judge, so
//! an oracle change that clears a WRONG by declining must show its cost here
//! (`agy/oracle-extent` review, 2026-11-03: 355 → 290 corpus cases covered).
//! Coverage is decided from the chain alone. `exact_volume_verdict` returns `NotCovered` from the
//! chain (indeterminate / no box / empty / too coarse) before it ever reads the
//! kernel volume; with a NaN kernel volume a covered chain reads `Flag`
//! (NaN never satisfies the band), so COVERED = Agree|Flag.
//!
//! `COVERAGE_DIR` (default app/tests/cases/assay) — every `*.waffle` in it.
//! Threads = `RESERVED_CORES` (default 1).

use std::path::PathBuf;
use std::sync::Mutex;
use test_harness::assay::exact_membership::{exact_volume_verdict, ExactChain, ExactVolumeVerdict};

fn line_for(path: &PathBuf) -> String {
    let id = path.file_stem().unwrap().to_string_lossy().to_string();
    let Ok(text) = std::fs::read_to_string(path) else {
        return format!("{id}\tUNREADABLE");
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return format!("{id}\tUNPARSEABLE");
    };
    let t0 = std::time::Instant::now();
    let chain = match ExactChain::from_waffle(&doc) {
        Ok(c) => c,
        Err(e) => return format!("{id}\tNOCHAIN\t{e:?}"),
    };
    let v = exact_volume_verdict(&chain, f64::NAN);
    let dt = t0.elapsed().as_secs_f64();
    match v {
        ExactVolumeVerdict::Agree { .. } | ExactVolumeVerdict::Flag { .. } => {
            format!("{id}\tCOVERED\t{dt:.1}s")
        }
        ExactVolumeVerdict::NotCovered(why) => {
            let why = why.replace(['\n', '\t'], " ");
            format!("{id}\tNOTCOVERED\t{why}\t{dt:.1}s")
        }
    }
}

#[test]
#[ignore]
fn exact_coverage_census() {
    let dir = std::env::var("COVERAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("app/tests/cases/assay")
        });
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "waffle"))
        .collect();
    paths.sort();
    let jobs: usize = std::env::var("RESERVED_CORES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    let next = Mutex::new(0usize);
    let out = Mutex::new(vec![String::new(); paths.len()]);
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = {
                    let mut n = next.lock().unwrap();
                    let i = *n;
                    *n += 1;
                    i
                };
                if i >= paths.len() {
                    break;
                }
                let l = line_for(&paths[i]);
                eprintln!("{l}");
                out.lock().unwrap()[i] = l;
            });
        }
    });
    let out = out.into_inner().unwrap();
    let (mut cov, mut nc, mut nochain, mut other) = (0, 0, 0, 0);
    for l in &out {
        println!("CENSUS\t{l}");
        match l.split('\t').nth(1) {
            Some("COVERED") => cov += 1,
            Some("NOTCOVERED") => nc += 1,
            Some("NOCHAIN") => nochain += 1,
            _ => other += 1,
        }
    }
    println!(
        "TOTALS\t{}\tcases {} covered {cov} notcovered {nc} nochain {nochain} other {other}",
        dir.display(),
        out.len()
    );
}

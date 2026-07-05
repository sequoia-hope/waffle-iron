//! Dev-only: time the three N24-assay TIMEOUT-flipped cases individually
//! (load-vs-cost discrimination; spec kv9_f1_tangency_inout_labels §2b).
use std::path::PathBuf;
use std::time::Instant;
use test_harness::assay::randomized_runner::run_single_case;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay");
    for id in ["R0001", "R0013", "R0056"] {
        let t0 = Instant::now();
        let r = run_single_case(&dir, id, true);
        let dt = t0.elapsed().as_secs_f64();
        match r {
            Some(r) => println!(
                "{id}: {:?} in {dt:.1}s | {}",
                r.status,
                &r.detail[..r.detail.len().min(80)]
            ),
            None => println!("{id}: not found"),
        }
    }
}

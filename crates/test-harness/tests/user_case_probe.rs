//! Scratch probe for user-reported cases (not committed).
use test_harness::ModelBuilder;

#[test]
#[ignore]
fn replay_error_coplanar_waffle() {
    let json = std::fs::read_to_string("/home/claude/workspace/error_coplanar.waffle")
        .expect("read waffle");
    let mut b = ModelBuilder::kernel_v2();
    match b.load(&json) {
        Ok(_) => println!("load OK"),
        Err(e) => println!("LoadProject FAILED: {e}"),
    }
    for (id, msg) in b.engine_errors() {
        println!("ENGINE ERROR {id}: {msg}");
    }
    for w in b.engine_warnings() {
        println!("WARNING: {w}");
    }
    match b.tessellate_last_with_tol(0.01) {
        Ok(m) => println!("tessellates: {} tris", m.indices.len() / 3),
        Err(e) => println!("tessellate FAILED: {e:?}"),
    }
}

/// Replay ANY `.waffle` named by `WAFFLE_PATH` through kernel-v2 and print
/// every engine error / warning (manual: `WAFFLE_PATH=… cargo test -p
/// test-harness --release --test user_case_probe replay_waffle_env -- --ignored --nocapture`).
#[test]
#[ignore]
fn replay_waffle_env() {
    let Ok(path) = std::env::var("WAFFLE_PATH") else {
        println!("WAFFLE_PATH not set — nothing to do");
        return;
    };
    let json = std::fs::read_to_string(&path).expect("read waffle");
    let mut b = ModelBuilder::kernel_v2();
    match b.load(&json) {
        Ok(_) => println!("load OK"),
        Err(e) => println!("LoadProject FAILED: {e}"),
    }
    for (id, msg) in b.engine_errors() {
        println!("ENGINE ERROR {id}: {msg}");
    }
    for w in b.engine_warnings() {
        println!("WARNING: {w}");
    }
    match b.tessellate_last_with_tol(0.01) {
        Ok(m) => {
            println!("tessellates: {} tris", m.indices.len() / 3);
            println!(
                "mesh volume: {:.12e}",
                test_harness::helpers::mesh_volume(&m)
            );
            for v in test_harness::oracle::run_all_mesh_checks(&m) {
                println!(
                    "oracle {}: {} {}",
                    v.oracle_name,
                    if v.passed { "PASS" } else { "FAIL" },
                    v.detail
                );
            }
        }
        Err(e) => println!("tessellate FAILED: {e:?}"),
    }
}

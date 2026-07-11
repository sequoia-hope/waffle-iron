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

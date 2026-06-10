//! Regenerate `crates/cherchi-rs/src/predicates/indirect/generated.rs`.
//!
//! Usage: `cargo run -p predicate-gen`

use std::path::Path;

fn main() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(predicate_gen::OUTPUT_RELATIVE);
    let contents = predicate_gen::orient3d::generate_file();
    std::fs::write(&out, &contents).expect("write generated.rs");
    println!("wrote {} ({} bytes)", out.display(), contents.len());
    for (suffix, delta, degree) in predicate_gen::orient3d::instance_table() {
        println!("  orient3d_{suffix}: delta = {delta:e}, degree = {degree}");
    }
}

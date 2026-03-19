//! Render a sketch to SVG.
//!
//! Usage:
//!   cargo run --example render_sketch --features render -- tests/golden/scenarios/rectangle.json
//!   echo '<sketch json>' | cargo run --example render_sketch --features render
//!
//! Outputs SVG to stdout.

use sketch_solver::{render_sketch_svg, solve_sketch, Sketch};
use std::io::Read;

fn main() {
    let json = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {path}: {e}")),
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .expect("Failed to read stdin");
            buf
        }
    };

    let sketch: Sketch = serde_json::from_str(&json).expect("Failed to parse Sketch JSON");
    let solved = solve_sketch(&sketch);

    eprintln!("Status: {:?}", solved.status);
    eprintln!("Points: {}", solved.positions.len());
    eprintln!("Profiles: {}", solved.profiles.len());

    let svg = render_sketch_svg(&sketch, &solved);
    print!("{svg}");
}

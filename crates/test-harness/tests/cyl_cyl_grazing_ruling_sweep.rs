//! Measurement driver (assertion-free) for the §13 crossing-ruling class:
//! sweep the axis offset of the `cyl_cyl_grazing_ruling_kv2` pair and print
//! each cut's outcome. Run with `YANG_433_TANGENT_INSERT=off` for the A/B.
//!
//! ```text
//! cargo test -p test-harness --test cyl_cyl_grazing_ruling_sweep --release -- --ignored --nocapture
//! ```

use test_harness::ModelBuilder;

const R_A: f64 = 1.0;
const R_B: f64 = 1.15;

fn outcome(delta: f64) -> String {
    let mut b = ModelBuilder::kernel_v2();
    b.true_circle_sketch("a_sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, R_A)
        .unwrap();
    b.extrude("a", "a_sk", 1.0).unwrap();
    b.true_circle_sketch("b_sk", [delta, 0.0, -0.2], [0.0, 0.0, 1.0], 0.0, 0.0, R_B)
        .unwrap();
    b.extrude_cut("b", "b_sk", 1.4).unwrap();
    let errors = b.engine_errors().to_vec();
    let warnings = b.engine_warnings().to_vec();
    if !errors.is_empty() {
        return format!("ERROR {}", errors[0].1);
    }
    if !warnings.is_empty() {
        return format!("WARN {}", warnings[0]);
    }
    let handle = b.solid_handle("b").expect("cut body");
    let exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&handle)
        .expect("exact volume");
    format!("OK volume {exact:.9}")
}

#[test]
#[ignore = "measurement driver"]
fn sweep_axis_offset() {
    for delta in std::env::var("SWEEP_DELTAS")
        .map(|s| {
            s.split(',')
                .map(|x| x.parse::<f64>().unwrap())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![0.16, 0.18, 0.20, 0.22, 0.25, 0.30, 0.40, 0.50])
    {
        let x = (R_A * R_A - R_B * R_B + delta * delta) / (2.0 * delta);
        let y = (R_A * R_A - x * x).sqrt();
        let na = [x / R_A, y / R_A];
        let nb = [(x - delta) / R_B, y / R_B];
        let angle = (na[0] * nb[0] + na[1] * nb[1]).acos().to_degrees();
        let thickness = delta + R_A - R_B;
        println!(
            "delta={delta:.2} crossing={angle:.2}deg crescent={thickness:.3}: {}",
            outcome(delta)
        );
    }
}

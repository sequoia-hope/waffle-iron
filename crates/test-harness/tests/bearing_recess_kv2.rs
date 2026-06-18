//! Bearing-recess regression (M8 disc∩disc coplanar containment via the real
//! kernel-v2 extrude→cut pipeline, the app's path): a dia-10 cylinder with a
//! concentric dia-5 circle cut N-deep into the top face. Pins the
//! sketch→extrude→sketch-on-face→extrude-cut flow a user drives in the GUI.
use test_harness::ModelBuilder;

fn recess(bodyr: f64, len: f64, cutr: f64, depth: f64, cx: f64, cy: f64) -> Vec<String> {
    let mut b = ModelBuilder::kernel_v2();
    b.true_circle_sketch("base", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, bodyr)
        .expect("base sketch");
    b.extrude("body", "base", len).expect("extrude body");
    b.true_circle_sketch("cutsk", [0.0, 0.0, len], [0.0, 0.0, 1.0], cx, cy, cutr)
        .expect("cut sketch");
    let _ = b.extrude_cut("cut", "cutsk", depth);
    b.engine_errors().iter().map(|(_, m)| m.clone()).collect()
}

#[test]
fn dia10_len10_dia5_cut2_concentric() {
    let errs = recess(5.0, 10.0, 2.5, 2.0, 0.0, 0.0);
    assert!(
        errs.is_empty(),
        "concentric bearing recess failed: {errs:?}"
    );
}

#[test]
fn dia10_len10_dia5_cut2_offcenter() {
    let errs = recess(5.0, 10.0, 2.5, 2.0, 1.5, 1.0);
    assert!(
        errs.is_empty(),
        "off-center bearing recess failed: {errs:?}"
    );
}

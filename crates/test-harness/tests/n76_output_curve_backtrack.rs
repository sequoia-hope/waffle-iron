//! N76 — a boolean OUTPUT loop must not traverse one intersection curve twice.
//!
//! Deviation `docs/yang_deviations.md` N76; ledger `docs/yang_tail_triage.md`
//! 2026-10-03 (night).
//!
//! P0017's `FaceId(28)` is a cone sliver whose outer loop is
//! `Arc(136) + SurfacePair(137) + SurfacePair(138)`, where 137 and 138 carry
//! the SAME `{Cylinder r = 8.322345964738464e-4 axis +ŷ, Cone α =
//! 0.8757228702119423 axis +x̂}` pair. On the cone's upper nappe that curve is
//! a single-valued graph `h₊(θ)` (the other root of the quadratic sits at
//! `h₋ ≈ −7.03e-4`, the far nappe), and the two half-edges' spans are
//! `[0.1420979, 0.2999498]` and `[0, 0.2999498]` — the second CONTAINS the
//! first. The excursion `node3 → node5 → node3` is therefore a zero-width spur
//! pointing OUT of the material: `h₊(θ) − h_arc` has exact zeros at both of the
//! rim arc's endpoints (`h₊(θ₃) = 3.606374095272e-4 = h_arc` to the bit) and
//! dips to −5.0010e-7 between them, so the face's material region is the LENS
//! between rim and curve, chart area ≈ 7.3e-8.
//!
//! The render CDT was right to refuse the nine-point chart ring it was handed:
//! it carries four proper self-crossings (`1–2 × 7–8`, `3–4 × 6–7`,
//! `3–4 × 7–8`, `4–5 × 6–7`) and EVERY one of its nine points lies on the cone
//! to machine precision (residual 0, −1.08e-19, at worst 6.93e-16), so the
//! crossing is in the declared geometry and no sampling density can remove it.
//! Measured: forced rim N = 35 passes, N = 50 fails DIFFERENTLY
//! (`reassembled output would be non-2-manifold`), N = 66/71/100/200 pass —
//! a resolution lottery, not a convergence.
//!
//! `BRep::normalize_output_curve_backtracks` merges the spur — the curved twin
//! of the straight backtrack spike `normalized_without_backtrack_spikes`
//! already removes on the INPUT side (task #146, F0064) — leaving the
//! exactly-correct two-edge lens. Mutation check: with
//! `is_curve_backtrack_pair` forced to `false`, this test fails with the
//! verbatim `TessellationFailed { face: FaceId(28), reason: "ring rejected by
//! CDT (degenerate/self-intersecting)" }`.
//!
//! Run: `cargo test -p test-harness --release --test n76_output_curve_backtrack`

use std::fs;
use std::path::PathBuf;

use test_harness::ModelBuilder;

/// The wall the spur used to raise, one crate downstream of where it was made.
const CDT_REJECT_WALL: &str = "ring rejected by CDT";

/// The wall the guard relaxation removed from the developable arms. A
/// two-edge LENS on distinct curves bounds a real area, and `from_yang_brep`'s
/// `lens_bigon` arm is what admits it.
const BIGON_WALL: &str = "fewer than 3 edges";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn load(case: &str) -> ModelBuilder {
    let waffle_json = fs::read_to_string(assay_dir().join(format!("{case}.waffle")))
        .unwrap_or_else(|e| panic!("{case}.waffle must be readable: {e}"));
    let mut builder = ModelBuilder::kernel_v2();
    builder
        .load(&waffle_json)
        .expect("LoadProject must succeed");
    builder
}

fn engine_errors(builder: &ModelBuilder) -> Vec<String> {
    builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect()
}

/// P0017 rebuilds clean: the output spur is normalized away, the cone sliver's
/// loop is the two-edge lens, and both developable arms accept it.
#[test]
fn p0017_output_curve_spur_is_normalized_away() {
    let mut builder = load("P0017");
    let failures = engine_errors(&builder);
    assert!(
        !failures.iter().any(|f| f.contains(CDT_REJECT_WALL)),
        "FaceId(28)'s chart ring self-crosses only because half-edges 137 and \
         138 double-cover one cyl×cone curve; with the spur merged there is no \
         crossing left to refuse. Got:\n  {}",
        failures.join("\n  ")
    );
    assert!(
        !failures.iter().any(|f| f.contains(BIGON_WALL)),
        "the merged loop is a two-edge LENS (rim Arc + cyl×cone SurfacePair, \
         DISTINCT curves) and bounds a real area; got:\n  {}",
        failures.join("\n  ")
    );
    assert!(
        failures.is_empty(),
        "P0017 must rebuild clean; got:\n  {}",
        failures.join("\n  ")
    );

    // The circle cut SEVERS the body. The exact-membership lattice is stable at
    // every rung and both phases — `components = 2`, bodies 6.4059e-11 and
    // 8.0718e-11, 1024-cell two-phase mean total 1.4476685e-10 — and the
    // kernel's render mesh sums to 1.446021896e-10, one inscribed-mesh chord
    // deficit (rel −1.14e-3) below it.
    let meshes = builder
        .tessellate_live_with_tol(1e-3)
        .expect("the converted model must tessellate");
    assert_eq!(
        meshes.len(),
        2,
        "the cut severs the body into exactly two solids"
    );
    let vol: f64 = meshes
        .iter()
        .map(test_harness::helpers::mesh_signed_volume)
        .sum();
    let expected = 1.4476685e-10;
    assert!(
        (vol - expected).abs() <= 3e-3 * expected,
        "tessellated total {vol:.9e} must match the adjudicated 1.4476685e-10 \
         within the 3e-3 chord-deficit band"
    );
}

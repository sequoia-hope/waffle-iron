//! Generator-level oracles: error-analysis sanity, emission snapshot,
//! determinism, and regeneration-matches-checked-in.

use predicate_gen::ir::Program;
use predicate_gen::orient3d::{
    generate_file, instance_table, lpi_lambda_spec, patterns, tpi_lambda_spec,
};

// ---------------------------------------------------------------------
// FPG forward error analysis
// ---------------------------------------------------------------------

/// Hand verification on the classic 2D-orientation determinant
/// `(qx−px)(ry−py) − (rx−px)(qy−py)` over translation differences:
///
/// - each difference: bound 1, error h = ulp(1)/2 ≈ 1.1107e−16
///   (f64::EPSILON/2 plus the FPG x87 bonus /2¹¹);
/// - each product: bound ≈ 1 (+ half-ulp), error ≈ ulp(1)/2 + h² + h + h
///   ≈ 3.332e−16;
/// - the difference of the two products: bound ≈ 2, error ≈
///   ulp(2)/2 + 2·3.332e−16 ≈ 2.221e−16 + 6.664e−16 ≈ 8.886e−16.
///
/// FPG's own published constant for this expression is
/// 8.88720573725927976811e−16 (`meyer_pion2008_fpg.txt:345`); ours must
/// land in the same narrow band (slightly larger: our `next_up`
/// emulation of round-toward-+∞ is a touch more conservative).
#[test]
fn det2x2_delta_matches_fpg_published_constant() {
    let mut p = Program::default();
    let a = p.diff_factor("qx - px", "iv", "ex");
    let b = p.diff_factor("ry - py", "iv", "ex");
    let c = p.diff_factor("rx - px", "iv", "ex");
    let d = p.diff_factor("qy - py", "iv", "ex");
    let ab = p.mul(a, b);
    let cd = p.mul(c, d);
    let det = p.sub(ab, cd);
    let (sfe, degree) = p.analyze(det);
    assert_eq!(degree, 2);
    assert!(
        sfe.error > 8.8e-16 && sfe.error < 9.0e-16,
        "det2x2 error {:e} outside the hand-verified band around FPG's \
         8.887e-16",
        sfe.error
    );
    // δ folds in (1+ε)^(k+2) for the runtime threshold computation —
    // a relative change ≪ 1e-10, still inside the band.
    let delta = sfe.delta(degree);
    assert!(delta > 8.8e-16 && delta < 9.0e-16);
    assert!(delta >= sfe.error, "delta must not shrink the error");
}

/// δ strictly grows with expression depth (more roundoff accumulates).
#[test]
fn delta_grows_with_depth() {
    let mut p = Program::default();
    let a = p.raw_factor("a", "iv", "ex");
    let b = p.raw_factor("b", "iv", "ex");
    let c = p.raw_factor("c", "iv", "ex");
    let ab = p.mul(a, b);
    let abc = p.mul(ab, c);
    let (s1, d1) = p.analyze(ab);
    let (s2, d2) = p.analyze(abc);
    assert_eq!((d1, d2), (2, 3));
    assert!(
        s2.error > s1.error,
        "deeper expression must accumulate more error: {:e} vs {:e}",
        s2.error,
        s1.error
    );
    // Tiny hand case: a·b of two exact inputs has error exactly the
    // half-ulp of its bound-1 product, ≈ 1.11e-16.
    assert!(s1.error > 1.0e-16 && s1.error < 1.2e-16);
}

/// Homogeneity is enforced: adding operands of different degree is a
/// generator bug, not a warning.
#[test]
#[should_panic(expected = "inhomogeneous")]
fn inhomogeneous_sum_panics() {
    let mut p = Program::default();
    let a = p.raw_factor("a", "iv", "ex");
    let b = p.raw_factor("b", "iv", "ex");
    let ab = p.mul(a, b); // degree 2
    let bad = p.add(ab, a); // degree 2 + degree 1
    p.analyze(bad);
}

// ---------------------------------------------------------------------
// Lambda filter constants vs Cherchi 2020's published values
// ---------------------------------------------------------------------

/// Cherchi 2020 §4.2.2 publishes the semi-static filter constants for
/// the same lambda denominators (`mesh_arrangement.txt:349, 370`):
///
/// - `εdL = 4.884981308350689e−15 · δL³`
/// - `εdT = 8.704148513061234e−14 · δT⁶`
///
/// Our independently-derived constants must match in degree exactly and
/// in magnitude closely (ours slightly larger — conservative `next_up`
/// rounding plus the runtime-scaling fold). This is a strong clean-room
/// cross-check: same formulas, independently analyzed.
#[test]
fn lambda_d_deltas_match_cherchi_published_band() {
    let lpi = lpi_lambda_spec();
    assert_eq!(lpi.d_deg, 3, "dL degree (Cherchi: δL³)");
    assert!(
        lpi.d_delta > 4.884981308350689e-15 && lpi.d_delta < 4.884981308350689e-15 * 2.0,
        "DELTA_LPI_D {:e} not in [1, 2]× Cherchi's 4.885e-15",
        lpi.d_delta
    );

    let tpi = tpi_lambda_spec();
    assert_eq!(tpi.d_deg, 6, "dT degree (Cherchi: δT⁶)");
    assert!(
        tpi.d_delta > 8.704148513061234e-14 && tpi.d_delta < 8.704148513061234e-14 * 2.0,
        "DELTA_TPI_D {:e} not in [1, 2]× Cherchi's 8.704e-14",
        tpi.d_delta
    );
}

/// Lambda degrees per the paper formulas: LPI λ = d·p − n·(p−q) is
/// degree 4 (d, n are degree-3 determinants); TPI λ is degree 7 and dT
/// degree 6 (normals are degree 2).
#[test]
fn lambda_degrees_match_paper_formulas() {
    let lpi = lpi_lambda_spec();
    assert_eq!((lpi.l_deg, lpi.d_deg), (4, 3));
    let tpi = tpi_lambda_spec();
    assert_eq!((tpi.l_deg, tpi.d_deg), (7, 6));
}

// ---------------------------------------------------------------------
// Instance table
// ---------------------------------------------------------------------

/// 15 sorted patterns, 14 generated instances (EEEE delegates), with
/// hand-derived degrees: with explicit p4 the rows have degree 7 (T),
/// 4 (L), 1 (E); all-implicit rows have degree d4·λi (e.g. TTTT:
/// 6+7 = 13 per row → 39).
#[test]
fn instance_degrees_match_hand_derivation() {
    assert_eq!(patterns().len(), 15);
    let table = instance_table();
    assert_eq!(table.len(), 14);
    let degree_of = |sfx: &str| {
        table
            .iter()
            .find(|(s, _, _)| s == sfx)
            .unwrap_or_else(|| panic!("missing instance {sfx}"))
            .2
    };
    assert_eq!(degree_of("leee"), 6); // 4+1+1
    assert_eq!(degree_of("teee"), 9); // 7+1+1
    assert_eq!(degree_of("llee"), 9); // 4+4+1
    assert_eq!(degree_of("llll"), 21); // 3·(3+4)
    assert_eq!(degree_of("tttt"), 39); // 3·(6+7)
    assert_eq!(degree_of("tttl"), 30); // 2·(3+7) + (6+4)... rows: d4(L,3)+λT(7)=10 ×3
    for (sfx, delta, degree) in &table {
        assert!(
            *delta > 0.0 && *delta < 1.0,
            "{sfx}: delta {delta:e} out of range"
        );
        assert!(*degree >= 6 && *degree <= 39, "{sfx}: degree {degree}");
    }
}

// ---------------------------------------------------------------------
// Emission: snapshot, determinism, checked-in file freshness
// ---------------------------------------------------------------------

#[test]
fn emission_snapshot_contains_expected_functions() {
    let f = generate_file();
    assert!(f.starts_with("//! GENERATED by predicate-gen"));
    for needle in [
        "pub(super) fn lpi_lambda_f64(",
        "pub(super) fn lpi_lambda_iv(",
        "pub(super) fn lpi_lambda_exact(",
        "pub(super) fn tpi_lambda_f64(",
        "pub(super) fn tpi_lambda_iv(",
        "pub(super) fn tpi_lambda_exact(",
        "pub(super) const DELTA_LPI_D:",
        "pub(super) const DELTA_TPI_D:",
        "pub(super) fn dispatch_canonical(",
        "pub(super) fn dispatch_filtered_canonical(",
        "pub(super) fn dispatch_exact_canonical(",
    ] {
        assert!(f.contains(needle), "generated file missing `{needle}`");
    }
    // Every instance gets 5 functions + 2 constants.
    for (sfx, _, _) in instance_table() {
        for tier in ["_filtered", "_interval", "_exact", "_inexact", ""] {
            let needle = format!("pub(super) fn orient3d_{sfx}{tier}(");
            assert!(f.contains(&needle), "generated file missing `{needle}`");
        }
        let needle = format!("const DELTA_{}:", sfx.to_uppercase());
        assert!(f.contains(&needle), "generated file missing `{needle}`");
    }
    // No division in CODE (denominators are handled by sign parity, never
    // evaluated); comment lines may mention λ/d fractions.
    for line in f.lines() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        assert!(
            !code.contains(" / "),
            "generated code must not divide: `{line}`"
        );
    }
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(
        generate_file(),
        generate_file(),
        "two runs must be byte-identical"
    );
}

/// The checked-in `generated.rs` must be exactly what the current
/// generator produces — guards against hand-edits and stale output.
#[test]
fn checked_in_file_is_fresh() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(predicate_gen::OUTPUT_RELATIVE);
    let checked_in = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert!(
        checked_in == generate_file(),
        "{} is stale or hand-edited — regenerate with `cargo run -p predicate-gen`",
        path.display()
    );
}

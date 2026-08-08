//! Increment 1 of `specs/assay_independent_volume_oracle.md` — the engine plus
//! its calibration.
//!
//! The oracle rebuilds each operation's solid **in isolation** from the case's
//! own `.waffle` (its verbatim solved sketch, never a profile re-derived from
//! `profile_size`), composes them set-theoretically, and compares the result to
//! what the kernel's boolean produced. The boolean is under test; the primitive
//! constructors are not.
//!
//! **Scope of this increment: all-BOSS cases only** (123 of the 261
//! SUPPORTED_CORRECT, of which 90 are F/R — precisely the population that
//! carries no absolute geometric oracle today). A `cut` operation is NOT
//! re-authored: `feature-engine`'s `rebuild.rs` picks a cut's sweep direction
//! from *the accumulated target body's* position and extends it by `cut_eps`,
//! so an independently reconstructed cut tool would risk a FALSE WRONG — the
//! one failure mode this oracle must never have. Cut cases are reported
//! NOT-COVERED, never silently passed (spec §5).
//!
//! Run the sweep:
//! ```text
//! cargo test -p test-harness --test assay_volume_oracle --release \
//!     -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::volume_oracle::{composed_volume, scan_volume, SolidScan};
use test_harness::workflow::ModelBuilder;

const CORPUS: &str = "../../app/tests/cases/assay";

/// The oracle's own tessellation tolerance — far finer than the corpus render
/// tolerance (`clamp(scale·0.01, 1e-9, 0.1)`), which admits ~1 % chord error on
/// curved profiles and would swamp the comparison.
fn oracle_tol(scale: f64) -> f64 {
    let k: f64 = std::env::var("ORACLE_TOL_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    (scale * 1e-4 * k).clamp(1e-15, 1e-3)
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

/// One operation of a case, as read from its meta.
struct OpMeta {
    is_cut: bool,
}

fn read_case(id: &str) -> Option<(serde_json::Value, serde_json::Value, Vec<OpMeta>, f64)> {
    let d = corpus_dir();
    let waffle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.waffle"))).ok()?).ok()?;
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.meta.json"))).ok()?).ok()?;
    let ops = meta
        .get("operations")?
        .as_array()?
        .iter()
        .map(|o| OpMeta {
            is_cut: o.get("is_cut").and_then(serde_json::Value::as_bool) == Some(true),
        })
        .collect();
    let scale = meta.get("scale").and_then(serde_json::Value::as_f64)?;
    Some((waffle.clone(), meta, ops, scale))
}

/// Build a single-feature document: operation `k` plus **only** the sketch it
/// references, taken verbatim from the case's own `.waffle`.
///
/// Returns `None` when the op is a cut (see the module note) or the document
/// shape is not the one the corpus emits.
fn isolate_operation(waffle: &serde_json::Value, k: usize) -> Option<String> {
    let feats = waffle
        .get("tabs")?
        .as_array()?
        .first()?
        .get("kind")?
        .get("features")?
        .get("features")?
        .as_array()?;

    // Ops in feature order (everything that is not a Sketch).
    let op_positions: Vec<usize> = feats
        .iter()
        .enumerate()
        .filter(|(_, f)| f.get("operation").and_then(|o| o.get("type")) != Some(&"Sketch".into()))
        .map(|(i, _)| i)
        .collect();
    let pos = *op_positions.get(k)?;
    let op = &feats[pos];
    let params = op.get("operation")?.get("params")?;
    if params.get("cut").and_then(serde_json::Value::as_bool) == Some(true) {
        return None; // not re-authored — see the module note
    }
    // `merge: false` is a NewBody op: the ops do NOT compose into one solid, so
    // "union of the operands" is the wrong expectation. Measured 2026-08-08:
    // C0082/C0083 ("combine modes: NewBody") flagged at rel 0.46/0.54 purely
    // because of this — a FALSE WRONG, caught before it was believed.
    if params.get("merge").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    let sketch_id = params.get("sketch_id")?.as_str()?;
    let sketch = feats.iter().find(|f| {
        f.get("operation")
            .and_then(|o| o.get("sketch"))
            .and_then(|s| s.get("id"))
            .and_then(serde_json::Value::as_str)
            == Some(sketch_id)
    })?;

    let mut doc = waffle.clone();
    // The sketch is taken VERBATIM — including its `plane` record. Every corpus
    // sketch also carries explicit `plane_origin` / `plane_normal`, which is
    // what the rebuild uses for the sweep frame; rewriting the `plane`
    // selector to an invented "World" variant is what made every operand fail
    // to load on the first attempt (2026-08-08).
    let sketch = sketch.clone();
    let list = doc
        .get_mut("tabs")?
        .as_array_mut()?
        .first_mut()?
        .get_mut("kind")?
        .get_mut("features")?
        .get_mut("features")?
        .as_array_mut()?;
    *list = vec![sketch, op.clone()];
    serde_json::to_string(&doc).ok()
}

/// The `sketch_id` each operation is driven by, in op order.
fn sketch_ids(waffle: &serde_json::Value) -> Option<Vec<String>> {
    let feats = waffle
        .get("tabs")?
        .as_array()?
        .first()?
        .get("kind")?
        .get("features")?
        .get("features")?
        .as_array()?;
    Some(
        feats
            .iter()
            .filter_map(|f| f.get("operation")?.get("params")?.get("sketch_id")?.as_str())
            .map(str::to_string)
            .collect(),
    )
}

/// Build one operand solid and scan it.
fn operand_scan(waffle: &serde_json::Value, k: usize, tol: f64) -> Option<SolidScan> {
    let json = isolate_operation(waffle, k)?;
    let mut b = ModelBuilder::kernel_v2();
    if let Err(e) = b.load(&json) {
        if std::env::var_os("ORACLE_DEBUG").is_some() {
            eprintln!("[oracle] op {k}: load failed: {e}");
        }
        return None;
    }
    if !b.engine_errors().is_empty() {
        if std::env::var_os("ORACLE_DEBUG").is_some() {
            eprintln!("[oracle] op {k}: engine errors: {:?}", b.engine_errors());
        }
        return None;
    }
    let mesh = match b.tessellate_last_with_tol(tol) {
        Ok(m) => m,
        Err(e) => {
            if std::env::var_os("ORACLE_DEBUG").is_some() {
                eprintln!("[oracle] op {k}: tessellate failed: {e}");
            }
            return None;
        }
    };
    SolidScan::from_render_mesh(&mesh)
}

/// The kernel's own boolean output, scanned through the SAME code path.
///
/// ALL live bodies, concatenated into one soup — not `tessellate_last`. A model
/// can legitimately end with several bodies, and the composed operand set is
/// the union of all of them; taking only the last would understate the output
/// and manufacture a discrepancy. (The winding sweep resolves a multi-shell
/// soup correctly — see `disjoint_shells_along_z_give_two_intervals`.)
fn output_scan(waffle: &serde_json::Value, tol: f64) -> Option<SolidScan> {
    let json = serde_json::to_string(waffle).ok()?;
    let mut b = ModelBuilder::kernel_v2();
    b.load(&json).ok()?;
    if !b.engine_errors().is_empty() {
        return None;
    }
    let meshes = b.tessellate_live_with_tol(tol).ok()?;
    let mut all = meshes.first()?.clone();
    for m in meshes.iter().skip(1) {
        let base = (all.vertices.len() / 3) as u32;
        all.vertices.extend_from_slice(&m.vertices);
        all.indices.extend(m.indices.iter().map(|i| i + base));
    }
    SolidScan::from_render_mesh(&all)
}

/// Verdict for one case.
#[derive(Debug)]
enum Verdict {
    /// Discrepancy within the oracle's own measured error.
    Agree { rel: f64, band: f64 },
    /// Exceeds it — a candidate silent-wrong.
    Flag { rel: f64, band: f64 },
    NotCovered(&'static str),
}

fn evaluate(id: &str, grid: usize) -> Verdict {
    let Some((waffle, _meta, ops, scale)) = read_case(id) else {
        return Verdict::NotCovered("unreadable case");
    };
    if ops.iter().any(|o| o.is_cut) {
        return Verdict::NotCovered("has a cut op (tool not re-authored)");
    }
    // Two ops driven by ONE sketch is the holed-profile class (C0094: "one
    // sketch, two extrudes") — profile 0 is the outer boundary and profile 1
    // the hole, so the result is not the union of two independently built
    // solids. Measured 2026-08-08: this flagged at rel 0.42 as a FALSE WRONG.
    if let Some(ids) = sketch_ids(&waffle) {
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != ids.len() {
            return Verdict::NotCovered("ops share a sketch (holed profile)");
        }
    }
    let tol = oracle_tol(scale);
    let mut scans = Vec::new();
    for k in 0..ops.len() {
        match operand_scan(&waffle, k, tol) {
            Some(s) => scans.push(s),
            None => return Verdict::NotCovered("operand build failed"),
        }
    }
    let Some(out) = output_scan(&waffle, tol) else {
        return Verdict::NotCovered("output build failed");
    };
    let refs: Vec<&SolidScan> = scans.iter().collect();
    let cuts = vec![false; refs.len()];
    let expected = composed_volume(&refs, &cuts, grid);
    let actual = scan_volume(&out, grid);

    let denom = expected.volume.abs().max(actual.volume.abs()).max(1e-300);
    let rel = (expected.volume - actual.volume).abs() / denom;
    // The band is MEASURED, never chosen: both sides' own grid residuals,
    // relative, plus a small floor for f32 render-vertex quantisation.
    let band = ((expected.residual + actual.residual) / denom) * 4.0 + 1e-5;
    if rel <= band {
        Verdict::Agree { rel, band }
    } else {
        Verdict::Flag { rel, band }
    }
}

/// CALIBRATION (spec §3.4). The 14 all-axis-aligned-rectangle F cases have an
/// EXACT expected volume from the corpus generator's own unit-tested box-CSG
/// sweep. The oracle must reproduce it before any verdict it emits elsewhere is
/// trusted. These cases are deliberately NOT the deliverable — all 14 are
/// two-op box unions, and proving them correct proves nothing about the kernel.
/// Their job is to prove the ORACLE.
#[test]
#[ignore = "slow (builds 3 solids per case); run with --ignored"]
fn calibration_against_exact_box_csg() {
    const CASES: [&str; 14] = [
        "F0001", "F0002", "F0003", "F0004", "F0005", "F0006", "F0007", "F0008", "F0009", "F0010",
        "F0051", "F0053", "F0091", "F0093",
    ];
    let mut failures = Vec::new();
    let mut covered = 0;
    for id in CASES {
        match evaluate(id, 96) {
            Verdict::Agree { rel, band } => {
                covered += 1;
                println!("{id}: AGREE rel={rel:.3e} band={band:.3e}");
            }
            Verdict::Flag { rel, band } => {
                covered += 1;
                failures.push(format!("{id}: rel={rel:.3e} > band={band:.3e}"));
            }
            Verdict::NotCovered(why) => println!("{id}: NOT-COVERED ({why})"),
        }
    }
    println!("calibration: {covered}/14 covered, {} flagged", failures.len());
    assert!(
        failures.is_empty(),
        "the ORACLE is wrong on its calibration set — no sweep verdict is \
         trustworthy until this is green:\n{}",
        failures.join("\n")
    );
    assert!(covered >= 10, "calibration coverage too thin: {covered}/14");
}

/// INCREMENT 2 — sweep every all-boss SUPPORTED_CORRECT case and report the
/// discrepancy distribution. Read-only: the assay's own verdicts do not change.
#[test]
#[ignore = "full sweep; run with --ignored --nocapture"]
fn sweep_all_boss_correct_cases() {
    let dir = corpus_dir();
    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("results.json")).expect("results.json"))
            .expect("parse results.json");
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    for r in results["results"].as_array().expect("results array") {
        status.insert(
            r["id"].as_str().unwrap_or_default().to_string(),
            r["category"].as_str().unwrap_or_default().to_string(),
        );
    }
    let only: Option<String> = std::env::var("ORACLE_CASE").ok();
    let grid: usize = std::env::var("ORACLE_GRID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);

    let (mut agree, mut flagged, mut skipped) = (0usize, Vec::new(), 0usize);
    for (id, cat) in &status {
        if cat != "SUPPORTED_CORRECT" {
            continue;
        }
        if let Some(want) = &only {
            if id != want {
                continue;
            }
        }
        match evaluate(id, grid) {
            Verdict::Agree { rel, band } => {
                agree += 1;
                println!("{id}: AGREE rel={rel:.3e} band={band:.3e}");
            }
            Verdict::Flag { rel, band } => {
                println!("{id}: **FLAG** rel={rel:.3e} band={band:.3e}");
                flagged.push(format!("{id} (rel={rel:.3e}, band={band:.3e})"));
            }
            Verdict::NotCovered(why) => {
                skipped += 1;
                println!("{id}: not-covered ({why})");
            }
        }
    }
    // Coverage is REPORTED, never implied (spec §5).
    println!(
        "\nORACLE SWEEP: agree={agree} flagged={} not-covered={skipped}",
        flagged.len()
    );
    if !flagged.is_empty() {
        println!("FLAGGED:\n  {}", flagged.join("\n  "));
    }
}

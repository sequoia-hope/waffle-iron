//! Document-level driver for the independent volume oracle: rebuild each
//! operation of a `.waffle` **in isolation**, compose the operand solids
//! set-theoretically, and compare against the kernel's own output.
//!
//! Extracted from `tests/assay_volume_oracle.rs` (increment 1) so the
//! categorized assay runner can apply the SAME check in-line: after the
//! 2026-08-08 anchoring (`docs/audits/volume_oracle_flags_anchored.md`),
//! volume composition — not body count — is the discriminator between a
//! legitimate disjoint-boss multi-body output (a generator-sanctioned case
//! shape, see `gen.rs` "free-space cut" note: only NO-OP shapes are repaired)
//! and a union that lost material (R0090/R0030 base-drop) or double-counted
//! an unfused overlap.
//!
//! Scope: all-BOSS chains only. A `cut` tool is NOT re-authored — the
//! engine's cut sweep direction depends on the accumulated target body
//! (`rebuild.rs`, `cut_eps`), so an independently rebuilt cut risks a FALSE
//! WRONG, the one failure mode an oracle must never have. Cut cases report
//! [`CompositionVerdict::NotCovered`], never a silent pass.

use super::volume_oracle::{composed_volume, scan_volume, SolidScan};
use crate::workflow::ModelBuilder;

/// The oracle's own tessellation tolerance — far finer than the corpus render
/// tolerance (`clamp(scale·0.01, 1e-9, 0.1)`), which admits ~1 % chord error
/// on curved profiles and would swamp the comparison. `ORACLE_TOL_SCALE`
/// scales it (dev knob for the tessellation-invariance discriminator).
pub fn oracle_tol(scale: f64) -> f64 {
    let k: f64 = std::env::var("ORACLE_TOL_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    (scale * 1e-4 * k).clamp(1e-15, 1e-3)
}

/// Verdict for one case.
#[derive(Debug)]
pub enum CompositionVerdict {
    /// Discrepancy within the oracle's own measured error band.
    Agree { rel: f64, band: f64 },
    /// Exceeds it — the output set differs from the union of the operands.
    Flag { rel: f64, band: f64 },
    /// The oracle cannot honestly author an expectation for this case.
    NotCovered(&'static str),
}

fn features(waffle: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    waffle
        .get("tabs")?
        .as_array()?
        .first()?
        .get("kind")?
        .get("features")?
        .get("features")?
        .as_array()
}

/// Build a single-feature document: operation `k` plus **only** the sketch it
/// references, taken verbatim from the case's own `.waffle`.
///
/// Returns `None` when the op is a cut / NewBody, when the sketch is not
/// datum-anchored (a face-anchored plane cannot be isolated), or when the
/// document shape is not the one the corpus emits.
pub fn isolate_operation(waffle: &serde_json::Value, k: usize) -> Option<String> {
    let feats = features(waffle)?;

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
    // "union of the operands" is the wrong expectation (measured 2026-08-08:
    // C0082/C0083 flagged at rel 0.46/0.54 purely because of this).
    if params.get("merge").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    let sketch_id = params.get("sketch_id")?.as_str()?;
    // A sketch whose plane anchors to a previous FEATURE's face cannot be
    // isolated. Measured 2026-08-08: never fires on today's corpus (all 1101
    // sketches anchor to a DATUM); the guard keeps the oracle honest if the
    // corpus ever grows a feature-anchored sketch.
    if !sketch_is_datum_anchored(feats, sketch_id) {
        return None;
    }
    let sketch = feats.iter().find(|f| {
        f.get("operation")
            .and_then(|o| o.get("sketch"))
            .and_then(|s| s.get("id"))
            .and_then(serde_json::Value::as_str)
            == Some(sketch_id)
    })?;

    let mut doc = waffle.clone();
    // The sketch is taken VERBATIM — including its `plane` record and the
    // explicit `plane_origin` / `plane_normal` the rebuild uses.
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

/// Does this sketch's plane resolve against a DATUM (context-free, so the
/// sketch can be isolated) rather than a previous feature's face?
pub fn sketch_is_datum_anchored(feats: &[serde_json::Value], sketch_id: &str) -> bool {
    feats
        .iter()
        .filter_map(|f| f.get("operation")?.get("sketch"))
        .find(|s| s.get("id").and_then(serde_json::Value::as_str) == Some(sketch_id))
        .and_then(|s| s.get("plane")?.get("anchor")?.get("type")?.as_str())
        .is_some_and(|t| t == "Datum")
}

/// The `sketch_id` each operation is driven by, in op order.
pub fn sketch_ids(waffle: &serde_json::Value) -> Option<Vec<String>> {
    let feats = features(waffle)?;
    Some(
        feats
            .iter()
            .filter_map(|f| {
                f.get("operation")?
                    .get("params")?
                    .get("sketch_id")?
                    .as_str()
            })
            .map(str::to_string)
            .collect(),
    )
}

/// Build one operand solid and scan it.
pub fn operand_scan(waffle: &serde_json::Value, k: usize, tol: f64) -> Option<SolidScan> {
    let json = isolate_operation(waffle, k)?;
    let mut b = ModelBuilder::kernel_v2();
    if b.load(&json).is_err() || !b.engine_errors().is_empty() {
        return None;
    }
    let mesh = b.tessellate_last_with_tol(tol).ok()?;
    SolidScan::from_render_mesh(&mesh)
}

/// The kernel's own boolean output, scanned through the SAME code path.
///
/// ALL live bodies, concatenated into one soup — not `tessellate_last`: a
/// model can legitimately end with several bodies (disjoint-boss lumps), and
/// the composed operand set is the union of all of them.
pub fn output_scan(waffle: &serde_json::Value, tol: f64) -> Option<SolidScan> {
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

/// Evaluate one case document against the composed union of its operands.
///
/// `is_cut` flags come from the case meta (one per op, op order); `scale` is
/// the meta's model scale. `id` labels ORACLE_DEBUG output only.
pub fn evaluate_composition(
    id: &str,
    waffle: &serde_json::Value,
    is_cut: &[bool],
    scale: f64,
    grid: usize,
) -> CompositionVerdict {
    if is_cut.iter().any(|&c| c) {
        return CompositionVerdict::NotCovered("has a cut op (tool not re-authored)");
    }
    // Two ops driven by ONE sketch is the holed-profile class (C0094): profile
    // 0 is the outer boundary and profile 1 the hole — not a union of two
    // independent solids. Measured 2026-08-08 as a FALSE WRONG (rel 0.42).
    if let Some(ids) = sketch_ids(waffle) {
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != ids.len() {
            return CompositionVerdict::NotCovered("ops share a sketch (holed profile)");
        }
    }
    let tol = oracle_tol(scale);
    let mut scans = Vec::new();
    for k in 0..is_cut.len() {
        match operand_scan(waffle, k, tol) {
            Some(s) => scans.push(s),
            None => return CompositionVerdict::NotCovered("operand build failed"),
        }
    }
    let Some(out) = output_scan(waffle, tol) else {
        return CompositionVerdict::NotCovered("output build failed");
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
    if std::env::var_os("ORACLE_DEBUG").is_some() {
        for (k, s) in scans.iter().enumerate() {
            let v = scan_volume(s, grid);
            eprintln!(
                "[oracle] {id} operand {k}: vol={:.6e} resid={:.2e}",
                v.volume, v.residual
            );
        }
        eprintln!(
            "[oracle] {id} composed={:.6e} (resid {:.2e})  output={:.6e} (resid {:.2e})",
            expected.volume, expected.residual, actual.volume, actual.residual
        );
    }
    if rel <= band {
        CompositionVerdict::Agree { rel, band }
    } else {
        CompositionVerdict::Flag { rel, band }
    }
}

/// The document with every feature from the `keep_ops`-th operation on
/// removed — the sketches that only the dropped ops referenced go with them,
/// and the rollback index is cleared. `None` when the document shape is not
/// the corpus's.
///
/// For probing a PREFIX of a chain in isolation: R0044's union completes
/// under the corner-transit gate set and the case then stops at its cut's
/// typed NotSupported, so the categorized runner never validates the union
/// (spec `yang_451_corner_transit.md`, inc-2c-3b-12b-11).
pub fn truncate_ops(waffle: &serde_json::Value, keep_ops: usize) -> Option<serde_json::Value> {
    let mut doc = waffle.clone();
    let list = doc
        .pointer_mut("/tabs/0/kind/features/features")?
        .as_array_mut()?;
    let is_sketch = |f: &serde_json::Value| f.pointer("/operation/type") == Some(&"Sketch".into());
    let mut kept: Vec<serde_json::Value> = Vec::new();
    let mut ops_seen = 0usize;
    for f in list.iter() {
        if !is_sketch(f) {
            if ops_seen == keep_ops {
                break;
            }
            ops_seen += 1;
        }
        kept.push(f.clone());
    }
    // Drop sketches no kept op references.
    let referenced: Vec<String> = kept
        .iter()
        .filter_map(|f| {
            f.pointer("/operation/params/sketch_id")?
                .as_str()
                .map(str::to_string)
        })
        .collect();
    kept.retain(|f| {
        !is_sketch(f)
            || f.pointer("/operation/sketch/id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| referenced.iter().any(|r| r == id))
    });
    *list = kept;
    if let Some(ai) = doc.pointer_mut("/tabs/0/kind/features/active_index") {
        *ai = serde_json::Value::Null;
    }
    Some(doc)
}

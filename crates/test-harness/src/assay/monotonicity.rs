//! Volume monotonicity per operation (remediation plan O5, review
//! 2026-10-28 §2.1): every corpus meta authors `volume_monotonicity` — one
//! `"increase"` / `"decrease"` per operation, the direction the model's total
//! live volume must move at that step — and until now nothing read it on the
//! corpus path. A cut that removes nothing (a silent no-op) or a union that
//! loses material is exactly what it states.
//!
//! One replay, no prefix re-runs: feature-engine keeps every feature's result
//! and who consumed whom (`EngineState::consumed_by`), so the live set after
//! step k is "outputs of features ≤ k not consumed by a feature ≤ k". Volumes
//! are the kernel's EXACT B-Rep volumes; a solid it cannot integrate (a
//! mesh-backed import, a surface the integrator lacks) makes that step
//! not covered rather than guessed.
//!
//! RECORD-ONLY until a census shows what the authored expectations look like
//! against today's kernel (same rule as `assay::on_surface`).

use crate::workflow::ModelBuilder;
use feature_engine::types::Operation;
use std::collections::HashSet;

/// Relative change below which a step counts as "no change" — far above the
/// exact integrator's rounding (bit-level on re-integration of the same
/// B-Rep), far below any authored feature.
pub const MONOTONIC_REL_EPS: f64 = 1e-9;

/// The outcome over a whole document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MonotonicityReading {
    /// Steps checked against an authored direction.
    pub checked: usize,
    /// Steps that could not be measured (an unintegrable solid).
    pub not_covered: usize,
    /// `(step, expected, before, after)` for every step that moved the wrong
    /// way or not at all.
    pub violations: Vec<(usize, String, f64, f64)>,
    /// Set when the meta's list and the document's operations disagree in
    /// length, so nothing could be paired.
    pub unpaired: Option<(usize, usize)>,
}

impl MonotonicityReading {
    /// Stable summary: `monotonicity: <checked> checked, <n> violation(s)...`.
    pub fn summary(&self) -> String {
        if let Some((authored, ops)) = self.unpaired {
            return format!("monotonicity: unpaired ({authored} authored vs {ops} ops)");
        }
        let mut s = format!(
            "monotonicity: {} checked, {} not covered, {} violation(s)",
            self.checked,
            self.not_covered,
            self.violations.len()
        );
        for (k, exp, a, b) in &self.violations {
            s.push_str(&format!(" [op {k}: expected {exp}, {a:.6e} -> {b:.6e}]"));
        }
        s
    }
}

/// Pair `authored` with the document's solid-producing operations and read
/// the total live volume after each one.
pub fn measure(builder: &ModelBuilder, authored: &[String]) -> MonotonicityReading {
    let engine = &builder.state.engine;
    let tree = &engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    let ops: Vec<_> = tree.features[..limit]
        .iter()
        .filter(|f| {
            !f.suppressed
                && !matches!(
                    f.operation,
                    Operation::Sketch { .. } | Operation::DatumPlane { .. }
                )
        })
        .collect();
    let mut r = MonotonicityReading::default();
    if authored.is_empty() {
        return r;
    }
    if authored.len() != ops.len() {
        r.unpaired = Some((authored.len(), ops.len()));
        return r;
    }
    let kernel = builder.kernel_ref().as_introspect();
    let mut before = 0.0f64;
    let mut before_known = true;
    for (k, f) in ops.iter().enumerate() {
        // Live after step k: outputs of ops[..=k] not consumed by ops[..=k].
        let upto: HashSet<_> = ops[..=k].iter().map(|g| g.id).collect();
        let consumed: HashSet<_> = engine
            .consumed_by
            .iter()
            .filter(|(consumer, _)| upto.contains(consumer))
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        let mut after = 0.0f64;
        let mut known = true;
        for g in &ops[..=k] {
            if consumed.contains(&g.id) {
                continue;
            }
            if let Some(res) = engine.get_result(g.id) {
                for (_, body) in &res.outputs {
                    match kernel.solid_volume(&body.handle) {
                        Ok(v) => after += v,
                        Err(_) => known = false,
                    }
                }
            }
        }
        let _ = f;
        if known && before_known {
            r.checked += 1;
            let scale = before.abs().max(after.abs()).max(f64::MIN_POSITIVE);
            let moved = (after - before) / scale;
            let ok = match authored[k].as_str() {
                "increase" => moved > MONOTONIC_REL_EPS,
                "decrease" => moved < -MONOTONIC_REL_EPS,
                _ => true,
            };
            if !ok {
                r.violations.push((k, authored[k].clone(), before, after));
            }
        } else {
            r.not_covered += 1;
        }
        before = after;
        before_known = known;
    }
    r
}

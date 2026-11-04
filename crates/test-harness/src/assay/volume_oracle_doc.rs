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

/// The Sketch FEATURE an operation's `sketch_id` names — resolved the way
/// `feature_engine::rebuild` resolves it (`find_sketch_in_tree` matches the
/// FEATURE id), with the inner `sketch.id` as the fallback. The engine-authored
/// corpus mints the two ids equal, so keying by the inner id alone coincided
/// there; the prospector's gen3 documents (P-series) mint them apart, and the
/// inner-id lookup read every one as "operand build failed" — the composition
/// oracle silently NotCovered and the topology instrument unable to isolate
/// operand 0 (2026-09-28, P0005's meta adjudication; the same trap the
/// exact-membership reader had, fixed 2026-09-28 for P0004).
fn sketch_feature<'a>(
    feats: &'a [serde_json::Value],
    sketch_id: &str,
) -> Option<&'a serde_json::Value> {
    let is_sketch = |f: &serde_json::Value| {
        f.get("operation").and_then(|o| o.get("type")) == Some(&"Sketch".into())
    };
    feats
        .iter()
        .find(|f| {
            is_sketch(f) && f.get("id").and_then(serde_json::Value::as_str) == Some(sketch_id)
        })
        .or_else(|| {
            feats.iter().find(|f| {
                f.get("operation")
                    .and_then(|o| o.get("sketch"))
                    .and_then(|s| s.get("id"))
                    .and_then(serde_json::Value::as_str)
                    == Some(sketch_id)
            })
        })
}

/// Build a single-feature document: operation `k` plus **only** the sketch it
/// references, taken verbatim from the case's own `.waffle`.
///
/// Returns `None` when the op is a cut / NewBody, when the sketch is not
/// datum-anchored (a face-anchored plane cannot be isolated), or when the
/// document shape is not the one the corpus emits.
pub fn isolate_operation(waffle: &serde_json::Value, k: usize) -> Option<String> {
    isolate_operation_inner(waffle, k, false)
}

/// [`isolate_operation`] for the TOPOLOGY instrument: a `merge: false`
/// (NewBody) op is still an operand solid of the set union of the live
/// bodies — the kernel's later auto-unions merge it into whatever they touch
/// (P0005: the revolve is a NewBody the gear then unions with), and the
/// output side is read as the concatenation of ALL live bodies. The
/// composition (volume) oracle keeps refusing NewBody ops, because its
/// expectation is a single composed solid.
pub fn isolate_operation_any_body(waffle: &serde_json::Value, k: usize) -> Option<String> {
    isolate_operation_inner(waffle, k, true)
}

fn isolate_operation_inner(
    waffle: &serde_json::Value,
    k: usize,
    allow_new_body: bool,
) -> Option<String> {
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
    if !allow_new_body && params.get("merge").and_then(serde_json::Value::as_bool) == Some(false) {
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
    let sketch = sketch_feature(feats, sketch_id)?;

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

/// Is solid-bearing op `k` an explicit UNION combine of existing bodies?
/// It contributes no operand of its own to the set union of the live
/// bodies — both of its inputs are already operands — so the topology
/// instrument skips it rather than trying to build it standalone (P0007,
/// 2026-09-29: "operand 4 failed to build").
pub fn op_is_union_combine(waffle: &serde_json::Value, k: usize) -> bool {
    let Some(feats) = features(waffle) else {
        return false;
    };
    let mut ops = feats
        .iter()
        .filter(|f| f.get("operation").and_then(|o| o.get("type")) != Some(&"Sketch".into()));
    let Some(op) = ops.nth(k) else {
        return false;
    };
    op.pointer("/operation/type")
        .and_then(serde_json::Value::as_str)
        == Some("BooleanCombine")
        && op
            .pointer("/operation/params/operation/type")
            .and_then(serde_json::Value::as_str)
            == Some("Union")
}

/// Does this sketch's plane resolve against a DATUM (context-free, so the
/// sketch can be isolated) rather than a previous feature's face?
pub fn sketch_is_datum_anchored(feats: &[serde_json::Value], sketch_id: &str) -> bool {
    sketch_feature(feats, sketch_id)
        .and_then(|f| f.get("operation")?.get("sketch"))
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
    scan_isolated(&isolate_operation(waffle, k)?, tol)
}

/// [`operand_scan`] through [`isolate_operation_any_body`] (the topology
/// instrument's operand set includes NewBody ops).
pub fn operand_scan_any_body(waffle: &serde_json::Value, k: usize, tol: f64) -> Option<SolidScan> {
    scan_isolated(&isolate_operation_any_body(waffle, k)?, tol)
}

fn scan_isolated(json: &str, tol: f64) -> Option<SolidScan> {
    let mut b = ModelBuilder::kernel_v2();
    if b.load(json).is_err() || !b.engine_errors().is_empty() {
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
    evaluate_composed_scans(id, &scans, &out, grid)
}

/// Compare the set union of pre-built operand scans against the output scan at
/// resolution `grid`. Separated from [`evaluate_composition`] so unit tests can
/// exercise the composition band on synthetic meshes without invoking the kernel.
pub fn evaluate_composed_scans(
    id: &str,
    scans: &[SolidScan],
    out: &SolidScan,
    grid: usize,
) -> CompositionVerdict {
    let refs: Vec<&SolidScan> = scans.iter().collect();
    let cuts = vec![false; refs.len()];
    let operand_vols: Vec<_> = scans.iter().map(|s| scan_volume(s, grid)).collect();
    let max_operand_residual = operand_vols
        .iter()
        .map(|v| v.residual)
        .fold(0.0_f64, f64::max);
    let expected = composed_volume(&refs, &cuts, grid);
    let actual = scan_volume(out, grid);

    let denom = expected.volume.abs().max(actual.volume.abs()).max(1e-300);
    let rel = (expected.volume - actual.volume).abs() / denom;
    // The band is MEASURED, never chosen: both sides' own grid residuals,
    // relative, plus a small floor for f32 render-vertex quantisation.
    //
    // `composed_volume`'s single scalar `|V(2n) - V(n)|` can under-report a
    // non-cancelling discretisation bias in `V(2n)` when step-wall aliasing on
    // the union XY bbox happens to align between `n` and `2n` (`X00000006-00100`:
    // `|V(128) - V(64)| = 8.82e-2` while `|V(256) - V(128)| = 1.24` and operand 0
    // `star6`'s own residual is `4.09e-1`), while boolean intersection on curved
    // operands breaks exact XY common-mode alignment with `out`.
    //
    // Conversely, summing operand residuals grows linearly with overlapping
    // operands, `max_k resid_k` alone still imports curved boundaries buried
    // inside another operand (`iv_union`), and `|V(4n) - V(2n)|` alone imports
    // common-mode XY step cliffs of axis-aligned boxes (`C0089`, where each
    // operand's own residual is zero and `V_comp(2n) == V_out(2n)`). Taking
    // `min(|V(4n) - V(2n)|, max_k resid_k)` carries only intrinsic operand
    // discretisation error that remains exposed on the composed union, without
    // growing with overlap or loosening buried/common-mode cases.
    let fine_composed_residual =
        if max_operand_residual > expected.residual || std::env::var_os("ORACLE_DEBUG").is_some() {
            composed_volume(&refs, &cuts, 2 * grid).residual
        } else {
            0.0
        };
    let expected_residual = expected
        .residual
        .max(fine_composed_residual.min(max_operand_residual));
    let band = ((expected_residual + actual.residual) / denom) * 4.0 + 1e-5;
    if std::env::var_os("ORACLE_DEBUG").is_some() {
        for (k, v) in operand_vols.iter().enumerate() {
            eprintln!(
                "[oracle] {id} operand {k}: vol={:.6e} resid={:.2e}",
                v.volume, v.residual
            );
        }
        eprintln!(
            "[oracle] {id} composed={:.6e} (resid {:.2e}, resid4n {:.2e}, band_resid {:.2e})  output={:.6e} (resid {:.2e})",
            expected.volume, expected.residual, fine_composed_residual, expected_residual, actual.volume, actual.residual
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
    // An op's `sketch_id` names the Sketch FEATURE (the way `rebuild.rs`
    // resolves it); prospector documents carry a different inner
    // `sketch.id`, so both are honoured (2026-09-29: P0007's truncation
    // dropped every sketch and the tool built "no active features").
    kept.retain(|f| {
        !is_sketch(f)
            || [f.pointer("/id"), f.pointer("/operation/sketch/id")]
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .any(|id| referenced.iter().any(|r| r == id))
    });
    *list = kept;
    if let Some(ai) = doc.pointer_mut("/tabs/0/kind/features/active_index") {
        *ai = serde_json::Value::Null;
    }
    Some(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waffle_types::kernel::RenderMesh;

    /// Extrude a star-shaped 2D profile `(u, v)` around `(0, 0)` from `0` to
    /// `depth` along `origin + u*ax_u + v*ax_v + w*axis` into a closed,
    /// outward-oriented `RenderMesh` (where `(ax_u, ax_v, axis)` is right-handed).
    fn extruded_star_mesh(
        origin: [f64; 3],
        ax_u: [f64; 3],
        ax_v: [f64; 3],
        axis: [f64; 3],
        ring_ccw: &[(f64, f64)],
        depth: f64,
    ) -> RenderMesh {
        let map_pt = |u: f64, v: f64, w: f64| -> [f32; 3] {
            [
                (origin[0] + u * ax_u[0] + v * ax_v[0] + w * axis[0]) as f32,
                (origin[1] + u * ax_u[1] + v * ax_v[1] + w * axis[1]) as f32,
                (origin[2] + u * ax_u[2] + v * ax_v[2] + w * axis[2]) as f32,
            ]
        };
        let m = ring_ccw.len();
        let mut verts: Vec<[f32; 3]> = Vec::with_capacity(2 * m + 2);
        let c_bot = 0u32;
        verts.push(map_pt(0.0, 0.0, 0.0));
        let c_top = 1u32;
        verts.push(map_pt(0.0, 0.0, depth));
        for &(u, v) in ring_ccw {
            verts.push(map_pt(u, v, 0.0));
        }
        for &(u, v) in ring_ccw {
            verts.push(map_pt(u, v, depth));
        }
        let bot = |i: usize| (2 + i) as u32;
        let top = |i: usize| (2 + m + i) as u32;
        let mut indices = Vec::with_capacity(12 * m);
        for i in 0..m {
            let next = (i + 1) % m;
            // Bottom cap (outward normal = -axis).
            indices.extend_from_slice(&[c_bot, bot(next), bot(i)]);
            // Top cap (outward normal = +axis).
            indices.extend_from_slice(&[c_top, top(i), top(next)]);
            // Side quad (outward normal = (dv, -du, 0)).
            indices.extend_from_slice(&[bot(i), bot(next), top(next), bot(i), top(next), top(i)]);
        }
        let mut vertices = Vec::with_capacity(verts.len() * 3);
        for p in verts {
            vertices.extend_from_slice(&p);
        }
        RenderMesh {
            vertices,
            normals: Vec::new(),
            indices,
            face_ranges: Vec::new(),
        }
    }

    fn concat_meshes(a: &RenderMesh, b: &RenderMesh) -> RenderMesh {
        let mut out = a.clone();
        let base = (out.vertices.len() / 3) as u32;
        out.vertices.extend_from_slice(&b.vertices);
        out.indices.extend(b.indices.iter().map(|i| i + base));
        out
    }

    /// Defect 3 regression (mirrors prospector seed-6 `X00000006-00100`):
    /// Operand 0 is a sharp 6-pointed star prism extruded along +Y whose own
    /// grid residual at `grid = 64` is ~0.40, while ~75% of its volume sits
    /// inside Operand 1 (an oblique cylinder prism) so `composed_volume`'s
    /// scalar residual `|V(2n) - V(n)|` is masked/cancelled ~5x smaller.
    /// When boolean intersection refines Operand 1's curved wall along the
    /// star-cylinder intersection arcs, the output scan shifts by ~0.78 (~3.5e-4
    /// relative) — well inside Operand 0's measured grid uncertainty, yet
    /// falsely flagged when the composition band drops operand residuals.
    #[test]
    fn composition_band_carries_dominant_operand_residual() {
        let mut star_ring = Vec::with_capacity(12);
        for k in 0..12 {
            let theta = (k as f64) * std::f64::consts::PI / 6.0 + 0.827;
            let r = if k % 2 == 0 { 8.9027 } else { 1.2353 };
            star_ring.push((r * theta.cos(), r * theta.sin()));
        }
        // Right-handed frame `(ax_u, ax_v, axis) = (+Z, +X, +Y)` extrudes the
        // XZ star along +Y from y = 0 to y = 9.628.
        let m_star = extruded_star_mesh(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            &star_ring,
            9.6282,
        );
        let s0 = SolidScan::from_render_mesh(&m_star).expect("star scan");
        let exact_star_vol = 3.0 * 8.9027 * 1.2353 * 9.6282;
        let s0_fine = scan_volume(&s0, 256);
        assert!(
            (s0_fine.volume - exact_star_vol).abs() < 0.05,
            "star mesh orientation/volume sanity: got {}, expected {}",
            s0_fine.volume,
            exact_star_vol
        );

        let cyl_mesh = |facets: usize| -> RenderMesh {
            let n_raw: [f64; 3] = [0.154024644, 0.787970659, 0.596136436];
            let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
            let axis = [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len];
            // Orthonormal basis perpendicular to `axis`: ax_u = normalize((1,0,0) x axis),
            // ax_v = axis x ax_u so (ax_u, ax_v, axis) is right-handed.
            let ux_raw: [f64; 3] = [0.0, -axis[2], axis[1]];
            let ux_len = (ux_raw[1] * ux_raw[1] + ux_raw[2] * ux_raw[2]).sqrt();
            let ax_u = [0.0, ux_raw[1] / ux_len, ux_raw[2] / ux_len];
            let ax_v = [
                axis[1] * ax_u[2] - axis[2] * ax_u[1],
                axis[2] * ax_u[0] - axis[0] * ax_u[2],
                axis[0] * ax_u[1] - axis[1] * ax_u[0],
            ];
            let ring: Vec<(f64, f64)> = (0..facets)
                .map(|k| {
                    let t = 2.0 * std::f64::consts::PI * (k as f64) / (facets as f64);
                    (8.829 * t.cos(), 8.829 * t.sin())
                })
                .collect();
            extruded_star_mesh([-2.8154, -0.7255, 1.9206], ax_u, ax_v, axis, &ring, 8.7060)
        };

        let m_cyl_isolated = cyl_mesh(72);
        let s1 = SolidScan::from_render_mesh(&m_cyl_isolated).expect("cyl scan");

        // Verify the exact failure precondition: operand 0's own residual
        // dominates `composed_volume`'s masked/cancelled scalar residual.
        let op0_v = scan_volume(&s0, 64);
        let comp_v = composed_volume(&[&s0, &s1], &[false, false], 64);
        assert!(
            op0_v.residual > 3.0 * comp_v.residual,
            "precondition: operand 0 residual ({:.3e}) must dominate composed residual ({:.3e})",
            op0_v.residual,
            comp_v.residual
        );

        // Output mesh represents the exact same union where boolean splitting
        // inserted additional on-surface vertices along the cylinder rim.
        let m_out = concat_meshes(&m_star, &cyl_mesh(96));
        let out = SolidScan::from_render_mesh(&m_out).expect("out scan");
        let s0_dup1 = SolidScan::from_render_mesh(&m_star).expect("star dup1");
        let s0_dup2 = SolidScan::from_render_mesh(&m_star).expect("star dup2");
        let s1_dup = SolidScan::from_render_mesh(&m_cyl_isolated).expect("cyl dup");
        let operands = [s0, s1];

        let pair_band = match evaluate_composed_scans("synth-00100", &operands, &out, 64) {
            CompositionVerdict::Agree { rel, band } => {
                assert!(rel <= band, "rel={rel:.3e} <= band={band:.3e}");
                band
            }
            other => panic!("expected Agree once operand residuals are carried, got {other:?}"),
        };

        // Repeating overlapping operands must NOT inflate the composition band.
        let overlapped_operands = [
            SolidScan::from_render_mesh(&m_star).expect("star a"),
            s0_dup1,
            s0_dup2,
            s1_dup,
        ];
        match evaluate_composed_scans("synth-overlap-no-sum", &overlapped_operands, &out, 64) {
            CompositionVerdict::Agree { band, .. } => {
                assert!(
                    (band - pair_band).abs() <= 1e-12,
                    "overlapping operands inflated band: {band:.6e} vs {pair_band:.6e}"
                );
            }
            other => panic!("expected Agree on repeated overlapping operands, got {other:?}"),
        }

        // Burying a rough curved/star operand completely inside a larger outer
        // axis-aligned box must NOT import the buried operand's isolated residual.
        let box_ring = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
        let m_outer_box = extruded_star_mesh(
            [0.0, 0.0, -20.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            &box_ring,
            40.0,
        );
        let s_outer = SolidScan::from_render_mesh(&m_outer_box).expect("outer box");
        let s_buried = SolidScan::from_render_mesh(&m_star).expect("buried star");
        let out_box = SolidScan::from_render_mesh(&m_outer_box).expect("outer box out");
        match evaluate_composed_scans("synth-buried", &[s_outer, s_buried], &out_box, 64) {
            CompositionVerdict::Agree { band, .. } => {
                assert!(
                    band <= 1.0001e-5,
                    "buried operand inflated enclosing box band: {band:.6e}"
                );
            }
            other => panic!("expected Agree on enclosing box, got {other:?}"),
        }

        // Guard the opposite direction: a genuine set-level deficit (dropping
        // operand 0's protruding spikes so output = cylinder alone, ~3.7% loss)
        // must still be flagged loudly.
        let out_dropped = SolidScan::from_render_mesh(&cyl_mesh(96)).expect("dropped scan");
        assert!(
            matches!(
                evaluate_composed_scans("synth-dropped", &operands, &out_dropped, 64),
                CompositionVerdict::Flag { .. }
            ),
            "dropping operand 0's protruding spikes must still Flag"
        );
    }
}

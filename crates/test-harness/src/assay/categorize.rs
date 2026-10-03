//! The corpus categorizer — one document in, one verdict out.
//!
//! Lifted verbatim from `tests/assay_kv2.rs` (2026-09-27, prospector P1) so
//! the assay prospector can judge a candidate document with the SAME code
//! that scores the corpus. The test binary keeps its timeout / subprocess /
//! parallel / results.json machinery and calls [`categorize`].
//!
//! Categories (see `docs/TESTING.md` §"Running the categorized assay"):
//! `SUPPORTED_CORRECT`, `SUPPORTED_WRONG`, `UNSUPPORTED(reason)`,
//! `EXPECTED_ERROR`, `ERROR`, `TIMEOUT`, `SKIPPED_SLOW`.

use crate::assay::gen::AssayMeta;
use crate::assay::volume_oracle_doc;
use crate::helpers::mesh_bounding_box;
use crate::oracle;
use crate::ModelBuilder;

// ── Categories ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnsupportedReason {
    Revolve,
    CurvedProfile,
    CoplanarBoolean,
    FilletChamferShell,
    MultiShell,
    Other,
}

impl UnsupportedReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Revolve => "revolve",
            Self::CurvedProfile => "curved-profile",
            Self::CoplanarBoolean => "coplanar-boolean",
            Self::FilletChamferShell => "fillet-chamfer-shell",
            Self::MultiShell => "multi-shell",
            Self::Other => "other",
        }
    }

    /// Inverse of [`label`] — the subprocess outcome-line codec (ASSAY_JOBS).
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "revolve" => Self::Revolve,
            "curved-profile" => Self::CurvedProfile,
            "coplanar-boolean" => Self::CoplanarBoolean,
            "fillet-chamfer-shell" => Self::FilletChamferShell,
            "multi-shell" => Self::MultiShell,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    SupportedCorrect,
    SupportedWrong,
    Unsupported(UnsupportedReason),
    /// The meta EXPECTS a rebuild error and the engine raised one (the
    /// self-intersection canaries F0073/F0074). The canary fired correctly
    /// — but PASS is reserved for fully-supported WORKING geometry, so this
    /// reports as an error status with the expectation as context.
    ExpectedError,
    Error,
    /// The case exceeded the per-case timeout. A distinct category — NOT
    /// `Error` (it's "too slow to judge here," usually the heavy exact-
    /// arithmetic gear arrangements), and NOT silently dropped. Feeds the
    /// auto slow-list that `ASSAY_FAST=1` skips for a quick baseline.
    Timeout,
    /// Skipped because it is on the slow-list and `ASSAY_FAST=1` was set.
    SkippedSlow,
}

impl Category {
    pub fn label(&self) -> String {
        match self {
            Self::SupportedCorrect => "SUPPORTED_CORRECT".to_string(),
            Self::SupportedWrong => "SUPPORTED_WRONG".to_string(),
            Self::Unsupported(r) => format!("UNSUPPORTED({})", r.label()),
            Self::ExpectedError => "EXPECTED_ERROR".to_string(),
            Self::Error => "ERROR".to_string(),
            Self::Timeout => "TIMEOUT".to_string(),
            Self::SkippedSlow => "SKIPPED_SLOW".to_string(),
        }
    }

    /// Inverse of [`label`] — the subprocess outcome-line codec (ASSAY_JOBS).
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "SUPPORTED_CORRECT" => Self::SupportedCorrect,
            "SUPPORTED_WRONG" => Self::SupportedWrong,
            "EXPECTED_ERROR" => Self::ExpectedError,
            "ERROR" => Self::Error,
            "TIMEOUT" => Self::Timeout,
            "SKIPPED_SLOW" => Self::SkippedSlow,
            other => {
                let inner = other.strip_prefix("UNSUPPORTED(")?.strip_suffix(')')?;
                Self::Unsupported(UnsupportedReason::from_label(inner)?)
            }
        })
    }
}

pub struct CaseOutcome {
    pub id: String,
    pub category: Category,
    pub detail: String,
}

/// Classify a `NotSupported` message (engine error or auto-union warning
/// text) into the adapter's declared unsupported boundaries.
///
/// Classification runs on the text AFTER the `operation not supported:`
/// marker — the adapter's typed reason — never on the failing feature's
/// name. An auto-union warning reads "Revolve 3: Auto-union failed: …
/// operation not supported: boolean_union: coplanar input face pair …";
/// matching the whole message keyed on "Revolve" and mislabeled coplanar
/// walls as UNSUPPORTED(revolve) (R0015/R0053; the same mislabel hid
/// R0085's coplanar wall until task #131).
pub fn unsupported_reason(msg: &str) -> UnsupportedReason {
    let reason = msg
        .split_once(NOT_SUPPORTED_MARKER)
        .map_or(msg, |(_, after)| after);
    let m = reason.to_lowercase();
    if m.contains("revolve") {
        UnsupportedReason::Revolve
    } else if m.contains("circle") || m.contains("curved") || m.contains("arc") {
        UnsupportedReason::CurvedProfile
    } else if m.contains("coplanar") {
        UnsupportedReason::CoplanarBoolean
    } else if m.contains("multi-shell") {
        // PR-KV7: the multi-shell operand wall (internal voids / disjoint
        // bodies cannot re-enter yang). Checked BEFORE the fillet/chamfer/
        // shell bucket so "multi-shell" does not pattern-match "shell".
        UnsupportedReason::MultiShell
    } else if m.contains("fillet") || m.contains("chamfer") || m.contains("shell") {
        UnsupportedReason::FilletChamferShell
    } else {
        UnsupportedReason::Other
    }
}

pub const NOT_SUPPORTED_MARKER: &str = "operation not supported:";

/// Replay one document through feature-engine + `KernelV2Adapter` and
/// categorize the outcome — THE verdict of the corpus score, lifted from
/// `tests/assay_kv2.rs::replay_case` on 2026-09-27 (assay prospector P1,
/// `specs/assay_prospector.md` §3.1) so a candidate document that is not a
/// corpus file can be judged by the same code path. Mirrors the legacy
/// randomized runner's replay shape (load → engine errors → tessellate
/// last → mesh oracles) but with the NotSupported-boundary categorization
/// in front.
///
/// `id` is only used for messages and the composition oracle's label.
pub fn categorize(id: &str, waffle_json: &str, meta: &AssayMeta) -> CaseOutcome {
    let err_outcome = |detail: String| CaseOutcome {
        id: id.to_string(),
        category: Category::Error,
        detail,
    };

    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(waffle_json) {
        return err_outcome(format!("LoadProject failed: {e}"));
    }

    let engine_errors: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("{id}: {msg}"))
        .collect();
    let warnings: Vec<String> = builder.engine_warnings().to_vec();

    // 1. NotSupported boundary? Check engine errors first (rebuild failures),
    //    then warnings (the merge=true auto-union path downgrades a boolean
    //    error to an "Auto-union failed: …" warning).
    let not_supported_msgs: Vec<&String> = engine_errors
        .iter()
        .chain(warnings.iter())
        .filter(|m| m.contains(NOT_SUPPORTED_MARKER))
        .collect();
    if let Some(first) = not_supported_msgs.first() {
        return CaseOutcome {
            id: id.to_string(),
            category: Category::Unsupported(unsupported_reason(first)),
            detail: format!(
                "{} NotSupported boundary(ies); first: {}",
                not_supported_msgs.len(),
                first
            ),
        };
    }

    // 2. Cases whose meta EXPECTS a rebuild error (legacy: disjoint-operand
    //    unions). If kernel-v2 also errors (for a non-NotSupported reason),
    //    that is the expected behavior; if it succeeds, fall through to
    //    normal mesh validation — succeeding with a valid (multi-shell)
    //    result is not wrong for the new kernel.
    if meta.oracles.expect_rebuild_error && !engine_errors.is_empty() {
        return CaseOutcome {
            id: id.to_string(),
            category: Category::ExpectedError,
            detail: format!("expected rebuild error: {}", engine_errors.join("; ")),
        };
    }

    // 3. Any other engine error is an unexpected failure.
    if !engine_errors.is_empty() {
        return err_outcome(format!(
            "{} engine error(s): {}",
            engine_errors.len(),
            engine_errors.join("; ")
        ));
    }

    // 3b. An auto-union failure that is NOT a declared NotSupported boundary
    //     is an unexpected boolean failure (the merge=true path downgrades
    //     it to a warning and leaves separate bodies, so without this check
    //     it would masquerade as a merge-incomplete SUPPORTED_WRONG).
    let union_failures: Vec<&String> = warnings
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .collect();
    if !union_failures.is_empty() {
        return err_outcome(format!(
            "{} auto-union failure(s): {}",
            union_failures.len(),
            union_failures
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    // 4. Tessellate the last solid (scale-adaptive tolerance like the legacy
    //    runner; the adapter's planar tessellation is exact and ignores it).
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let bodies = match builder.tessellate_last_bodies_with_tol(tess_tol) {
        Ok(m) => m,
        Err(e) => return err_outcome(format!("no solid / tessellation failed: {e}")),
    };
    let mesh = crate::workflow::merge_meshes(bodies.clone());

    // 5. Validation: the legacy replay's mesh oracles + meta expectations.
    let mut failures: Vec<String> = Vec::new();
    for v in oracle::run_all_mesh_checks(&mesh) {
        // `no_self_intersection` asks whether ONE solid penetrates itself, so
        // it is judged per BODY below. On the merge of a multi-body result it
        // would report two separate live bodies that legitimately overlap —
        // bodies the document never unioned — as a kernel defect (P0010,
        // 2026-10-03: the carried first boss overlaps the union result by
        // 8.2e-8 m³ and each body is watertight and penetration-free).
        if v.oracle_name == "no_self_intersection" {
            continue;
        }
        if !v.passed {
            failures.push(format!("{}: {}", v.oracle_name, v.detail));
        }
    }
    for (i, body) in bodies.iter().enumerate() {
        let v = oracle::check_no_self_intersection(body);
        if !v.passed {
            failures.push(format!("no_self_intersection (body {i}): {}", v.detail));
        }
    }
    if mesh.indices.is_empty() {
        failures.push("empty mesh: no triangles".to_string());
    }
    // Spec `cut_consumes_body`: when the engine explicitly reports a boolean
    // that CONSUMED a body (an engulfing cut / empty intersect), the ops that
    // built that body contribute no final triangles, so the op-derived
    // minimum is not a valid expectation — the volume/euler/watertight
    // oracles still validate the survivors. Only the engine's typed
    // consumption warning skips the check; never relaxed on error paths.
    let body_consumed = builder
        .engine_warnings()
        .iter()
        .any(|w| w.contains("consumed the entire target body") || w.contains("no material"));
    if !body_consumed && !meta.oracles.derived_meta {
        let ops: Vec<(String, String)> = meta
            .operations
            .iter()
            .map(|o| (o.kind.clone(), o.profile_type.clone()))
            .collect();
        let v = oracle::check_minimum_triangle_count(&mesh, &ops);
        if !v.passed {
            failures.push(format!("minimum_triangle_count: {}", v.detail));
        }
    }
    if !mesh.vertices.is_empty() {
        let v = oracle::check_volume_magnitude(&mesh, meta.scale);
        if !v.passed {
            failures.push(format!("volume_magnitude: {}", v.detail));
        }
        // C-series exact-volume oracle: the meta carries an analytic volume
        // computed at generation time from kernel-independent arithmetic.
        // Multi-body cases must sum ALL bodies, so tessellate the whole model.
        if let Some(expected) = meta.oracles.expected_volume {
            let tol_rel = meta.oracles.expected_volume_tol_rel.unwrap_or(1e-3);
            let vol = builder
                .tessellate_live_with_tol(tess_tol)
                .map(|meshes| {
                    meshes
                        .iter()
                        .map(crate::helpers::mesh_signed_volume)
                        .sum::<f64>()
                })
                .unwrap_or_else(|_| crate::helpers::mesh_signed_volume(&mesh));
            if (vol - expected).abs() > tol_rel * expected.abs() {
                failures.push(format!(
                    "expected_volume: {vol:.9e} vs expected {expected:.9e} (rel tol {tol_rel:.1e})"
                ));
            }
        }
        let v = oracle::check_mesh_euler_characteristic_with_shells(
            &mesh,
            meta.oracles.euler_target,
            meta.oracles.expected_shell_count,
        );
        if meta.oracles.derived_meta {
            // Prospector candidate: no adjudicated target — χ must be even.
            match v.value {
                Some(chi) if (chi as i64) % 2 != 0 => failures.push(format!(
                    "mesh_euler_characteristic: χ = {} is ODD ({})",
                    chi as i64, v.detail
                )),
                _ => {}
            }
        } else if !v.passed {
            failures.push(format!("mesh_euler_characteristic: {}", v.detail));
        }
        if let Ok(dir) = std::env::var("ASSAY_DUMP_STL") {
            if let Ok(bytes) = crate::stl::export_binary_stl(&mesh, &meta.id) {
                let path = format!("{dir}/{}.stl", meta.id);
                let _ = std::fs::write(&path, bytes);
                eprintln!("[assay] dumped final mesh to {path}");
            }
        }
        // `ASSAY_DUMP_OBJ=<dir>`: the same final mesh as a Wavefront OBJ with
        // one `g face_<id>` group per kernel face and the f32 positions
        // written at full round-trip precision — so a non-manifold residue
        // can be attributed to the face whose tessellation produced it
        // (the STL dump carries neither). Read-only, off unless set.
        if let Ok(dir) = std::env::var("ASSAY_DUMP_OBJ") {
            use std::fmt::Write as _;
            let mut obj = String::new();
            for v in mesh.vertices.chunks_exact(3) {
                let _ = writeln!(obj, "v {:?} {:?} {:?}", v[0], v[1], v[2]);
            }
            let mut ranges: Vec<&waffle_types::kernel::FaceRange> =
                mesh.face_ranges.iter().collect();
            ranges.sort_by_key(|r| r.start_index);
            let mut tri = 0usize;
            let ntri = mesh.indices.len() / 3;
            for r in ranges {
                let _ = writeln!(obj, "g face_{}", r.face_id.0);
                let end = (r.end_index as usize / 3).min(ntri);
                while tri < end {
                    let i = tri * 3;
                    let _ = writeln!(
                        obj,
                        "f {} {} {}",
                        mesh.indices[i] + 1,
                        mesh.indices[i + 1] + 1,
                        mesh.indices[i + 2] + 1
                    );
                    tri += 1;
                }
            }
            if tri < ntri {
                let _ = writeln!(obj, "g face_unranged");
                while tri < ntri {
                    let i = tri * 3;
                    let _ = writeln!(
                        obj,
                        "f {} {} {}",
                        mesh.indices[i] + 1,
                        mesh.indices[i + 1] + 1,
                        mesh.indices[i + 2] + 1
                    );
                    tri += 1;
                }
            }
            let path = format!("{dir}/{}.obj", meta.id);
            let _ = std::fs::write(&path, obj);
            eprintln!("[assay] dumped final mesh to {path}");
        }
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        let dx = (bb_max[0] - bb_min[0]) as f64;
        let dy = (bb_max[1] - bb_min[1]) as f64;
        let dz = (bb_max[2] - bb_min[2]) as f64;
        let diagonal = (dx * dx + dy * dy + dz * dz).sqrt();
        if diagonal > meta.oracles.max_bbox_extent {
            failures.push(format!(
                "bbox diagonal {:.3e} exceeds max {:.3e}",
                diagonal, meta.oracles.max_bbox_extent
            ));
        }
    }
    // Multi-op cases must end as a single merged body (legacy runner check) —
    // unless the meta declares a deliberate multi-body count (C-series 3a).
    if let Some(expected_solids) = meta.oracles.expected_solid_count {
        let solid_count = builder.distinct_solid_count();
        if solid_count != expected_solids {
            failures.push(format!(
                "solid count: {solid_count} bodies (meta expects {expected_solids})"
            ));
        }
    } else if meta.operations.len() > 1 {
        // Volume composition (the independent oracle, in-line): the output
        // must equal the SET UNION of the operations' isolated solids. This
        // REPLACES the body-count "merge incomplete" check (2026-08-08): a
        // free-space disjoint boss is a generator-sanctioned case shape
        // (`assay/gen.rs` repairs only NO-OP shapes — swallowed boss,
        // free-space cut), and the engine's two-body disjoint-merge output is
        // spec'd behavior (`disjoint_merge_bodies.rs`, the F0015-class fix) —
        // so body count cannot distinguish that legitimate shape from a union
        // that LOST material (R0090/R0030 base-drop) or kept an unfused
        // overlap. Volume composition distinguishes all three, and also
        // catches the single-body deficit class (R0040/R0057/R0059) no count
        // could see. Cut chains are NOT-COVERED (a cut tool is not
        // re-authorable in isolation) — a recorded gap, never a silent pass.
        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(waffle_json) {
            let cuts: Vec<bool> = meta.operations.iter().map(|o| o.is_cut).collect();
            match volume_oracle_doc::evaluate_composition(id, &doc, &cuts, meta.scale, 64) {
                volume_oracle_doc::CompositionVerdict::Flag { rel, band } => {
                    failures.push(format!(
                        "volume_composition: output differs from the union of the \
                         operations' isolated solids (rel={rel:.3e} > band={band:.3e})"
                    ));
                }
                volume_oracle_doc::CompositionVerdict::Agree { .. }
                | volume_oracle_doc::CompositionVerdict::NotCovered(_) => {}
            }
        }
    }

    // Exact-membership volume (the analytic oracle, in-line, 2026-09-04):
    // the kernel's result volume against the document's closed-form solid
    // on a lattice (`assay::exact_membership`) — no mesh on the reference
    // side, so it sees what no mesh-borne oracle can: a cut that removes
    // nothing, a wedge tessellated as its complement (the 2026-09-03
    // sweep's classes A–C, all SUPPORTED_CORRECT until then). Cut chains
    // are covered here, unlike the composition oracle. The kernel side is
    // tessellated at the volume oracle's own tolerance (`oracle_tol`, the
    // sweep's), the band is the lattice's own convergence plus a
    // tessellation floor, and an unconverged lattice or an uncovered
    // document is NotCovered, never a verdict. `ASSAY_EXACT_VOLUME=0|off`
    // is the dev A/B knob.
    let exact_on = !matches!(
        std::env::var("ASSAY_EXACT_VOLUME").as_deref(),
        Ok("0") | Ok("off")
    );
    if exact_on {
        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(waffle_json) {
            if let Ok(chain) = crate::assay::exact_membership::ExactChain::from_waffle(&doc) {
                let vol_tol = volume_oracle_doc::oracle_tol(meta.scale);
                let kernel_vol = builder
                    .tessellate_live_with_tol(vol_tol)
                    .map(|meshes| {
                        meshes
                            .iter()
                            .map(crate::helpers::mesh_signed_volume)
                            .sum::<f64>()
                    })
                    .unwrap_or_else(|_| crate::helpers::mesh_signed_volume(&mesh));
                use crate::assay::exact_membership::ExactVolumeVerdict;
                match crate::assay::exact_membership::exact_volume_verdict(
                    &chain, kernel_vol,
                ) {
                    ExactVolumeVerdict::Flag {
                        rel,
                        band,
                        exact,
                        kernel,
                    } => failures.push(format!(
                        "exact_volume: kernel {kernel:.6e} vs exact {exact:.6e} (rel {rel:+.3e} outside band {band:.3e})"
                    )),
                    ExactVolumeVerdict::Agree { .. } | ExactVolumeVerdict::NotCovered(_) => {}
                }
            }
        }
    }

    if failures.is_empty() {
        CaseOutcome {
            id: id.to_string(),
            category: Category::SupportedCorrect,
            detail: "all checks passed".to_string(),
        }
    } else {
        CaseOutcome {
            id: id.to_string(),
            category: Category::SupportedWrong,
            detail: failures.join("; "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The auto-union message shape embeds the FAILING FEATURE's name before
    /// the marker; the reason bucket must come from the adapter's typed text
    /// after it (R0015/R0053 were mislabeled UNSUPPORTED(revolve) for their
    /// coplanar wall).
    #[test]
    fn unsupported_reason_ignores_feature_name_prefix() {
        assert_eq!(
            unsupported_reason(
                "Revolve 3: Auto-union failed: kernel error: operation not supported: \
                 boolean_union: coplanar input face pair (Yang Stage 0 coplanar \
                 preprocessing — roadmap M8 — not yet implemented)"
            ),
            UnsupportedReason::CoplanarBoolean
        );
        // A genuine revolve wall still classifies as revolve (marker present).
        assert_eq!(
            unsupported_reason(
                "abc-123: operation error: kernel error: operation not supported: \
                 revolve_face: full-turn circle profile sweeps a CLOSED torus \
                 (kernel-v2 roadmap KV6d; PARTIAL-turn circle revolve → torus is supported)"
            ),
            UnsupportedReason::Revolve
        );
        // Marker-less text (defensive): falls back to whole-message matching.
        assert_eq!(
            unsupported_reason("coplanar input face pair"),
            UnsupportedReason::CoplanarBoolean
        );
    }
}

//! Assay prospector — the search for the corpus's next failing case.
//!
//! Plan of record: `specs/assay_prospector.md`. This module holds the parts
//! that are library code: deriving a candidate's meta FROM its document
//! (§3.2), so any `.waffle` — generated, mutated or harvested — can be judged
//! by [`crate::assay::categorize::categorize`] with the same code path that
//! scores the corpus.

pub mod gen3;
pub mod metamorphic;
pub mod minimize;
pub mod mutate;
pub mod promote;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assay::gen::{AssayMeta, OpMeta, OracleExpectations};

/// The generator version stamped on prospector-derived metas.
pub const PROSPECT_META_VERSION: u32 = 100;

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// One parsed sketch: its plane and the extent / kind of its entities.
struct SketchInfo {
    origin: [f64; 3],
    normal: [f64; 3],
    /// Radius of the smallest origin-centred disc holding every point.
    radius: f64,
    /// Largest |u| or |v| over the sketch's points (for the scale estimate).
    max_abs: f64,
    profile_type: String,
}

fn sketch_info(sk: &Value) -> Option<SketchInfo> {
    let origin = sk.get("plane_origin").and_then(vec3)?;
    let normal = sk.get("plane_normal").and_then(vec3)?;
    let entities = sk.get("entities").and_then(Value::as_array)?;
    let mut radius: f64 = 0.0;
    let mut max_abs: f64 = 0.0;
    let mut lines = 0usize;
    let mut circles = 0usize;
    let mut arcs = 0usize;
    for e in entities {
        match e.get("type").and_then(Value::as_str).unwrap_or("") {
            "Point" => {
                let x = e.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = e.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                radius = radius.max((x * x + y * y).sqrt());
                max_abs = max_abs.max(x.abs()).max(y.abs());
            }
            "Line" => lines += 1,
            "Circle" => {
                circles += 1;
                let r = e.get("radius").and_then(Value::as_f64).unwrap_or(0.0);
                radius = radius.max(r);
                max_abs = max_abs.max(r);
            }
            "Arc" => arcs += 1,
            _ => {}
        }
    }
    // The categorizer's `check_minimum_triangle_count` knows "rectangle",
    // "circle" and "gear"; everything else falls to the polygon floor.
    let profile_type = if circles > 0 && lines == 0 && arcs == 0 {
        "circle"
    } else if arcs > 0 {
        "arc-polygon"
    } else if lines == 4 {
        "rectangle"
    } else {
        "polygon"
    }
    .to_string();
    Some(SketchInfo {
        origin,
        normal,
        radius,
        max_abs,
        profile_type,
    })
}

/// Derive an [`AssayMeta`] for a document the corpus generator did not
/// write: operations, planes, profile classes and the scale come from the
/// document itself; the Euler target is marked UNKNOWN (χ must merely be
/// even); no exact volume is authored (the in-line exact-membership oracle
/// judges the volume from the same document).
///
/// Fails only on a document with no tabs / feature list; an operation the
/// walker does not model is recorded with its type tag as `kind` and no
/// profile, never dropped (the categorizer's op-count checks must see it).
pub fn derive_meta(id: &str, doc: &Value) -> Result<AssayMeta, String> {
    let feats = doc
        .get("tabs")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(|t| t.pointer("/kind/features/features"))
        .and_then(Value::as_array)
        .ok_or_else(|| "document has no feature list".to_string())?;

    let mut sketches: std::collections::HashMap<String, SketchInfo> =
        std::collections::HashMap::new();
    let mut operations: Vec<OpMeta> = Vec::new();
    let mut extent: f64 = 0.0;
    let mut first_plane: Option<([f64; 3], [f64; 3])> = None;

    for f in feats {
        if f.get("suppressed").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(op) = f.get("operation") else {
            continue;
        };
        let ty = op.get("type").and_then(Value::as_str).unwrap_or("");
        let params = op.get("params");
        match ty {
            "Sketch" => {
                if let Some(sk) = op.get("sketch") {
                    if let (Some(id), Some(info)) =
                        (sk.get("id").and_then(Value::as_str), sketch_info(sk))
                    {
                        extent = extent
                            .max(info.max_abs)
                            .max(info.origin.iter().map(|c| c.abs()).fold(0.0, f64::max));
                        if first_plane.is_none() {
                            first_plane = Some((info.origin, info.normal));
                        }
                        sketches.insert(id.to_string(), info);
                    }
                }
            }
            "Extrude" | "Revolve" => {
                let sk = params
                    .and_then(|p| p.get("sketch_id"))
                    .and_then(Value::as_str)
                    .and_then(|sid| sketches.get(sid));
                let is_cut = params
                    .and_then(|p| p.get("cut"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let depth_or_angle = if ty == "Extrude" {
                    let d = params
                        .and_then(|p| p.get("depth"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    // A ThroughAll / UpTo extrude carries a placeholder depth
                    // (1.0) the sweep never uses; only a Blind depth is a
                    // length of the model. (Read as 1.0 m it made a 1.3 mm
                    // part report scale 1 and fail `volume_magnitude`.)
                    let blind = params
                        .and_then(|p| p.pointer("/depth_mode/type"))
                        .and_then(Value::as_str)
                        .is_none_or(|m| m == "Blind");
                    if blind {
                        extent = extent.max(d.abs());
                    }
                    d
                } else {
                    params
                        .and_then(|p| p.get("angle"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                };
                operations.push(OpMeta {
                    kind: ty.to_lowercase(),
                    profile_type: sk
                        .map(|s| s.profile_type.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    profile_size: sk.map(|s| s.radius).unwrap_or(0.0),
                    depth_or_angle,
                    is_cut,
                    plane_origin: sk.map(|s| s.origin),
                    plane_normal: sk.map(|s| s.normal),
                });
            }
            "BooleanCombine" => {
                // `BooleanOp` is `#[serde(tag = "type")]`: `{"type": "Union"}`.
                let verb = params
                    .and_then(|p| p.get("operation"))
                    .and_then(|o| o.get("type").and_then(Value::as_str).or(o.as_str()))
                    .unwrap_or("");
                operations.push(OpMeta {
                    kind: format!("boolean-{}", verb.to_lowercase()),
                    profile_type: "none".to_string(),
                    profile_size: 0.0,
                    depth_or_angle: 0.0,
                    is_cut: matches!(verb, "Subtract" | "Intersect"),
                    plane_origin: None,
                    plane_normal: None,
                });
            }
            other => operations.push(OpMeta {
                kind: other.to_lowercase(),
                profile_type: "unknown".to_string(),
                profile_size: 0.0,
                depth_or_angle: 0.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            }),
        }
    }
    if operations.is_empty() {
        return Err("document has no solid-bearing operation".to_string());
    }

    let scale = extent.max(1e-6);
    let (plane_origin, plane_normal) = first_plane.unwrap_or(([0.0; 3], [0.0, 0.0, 1.0]));
    let summary = operations
        .iter()
        .map(|o| {
            format!(
                "{}({},{})",
                o.kind,
                o.profile_type,
                if o.is_cut { "cut" } else { "boss" }
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    Ok(AssayMeta {
        id: id.to_string(),
        description: format!(
            "{} ops, scale={scale:.2e}, {summary} — prospector candidate (meta derived from the document)",
            operations.len()
        ),
        master_seed: 0,
        test_seed: 0,
        scale,
        log_scale: scale.log10(),
        plane_origin,
        plane_normal,
        operations,
        oracles: OracleExpectations {
            euler_target: 2,
            expect_watertight: true,
            // Catastrophic-explosion guard only: a legitimate diagonal is at
            // most a few times the extent.
            max_bbox_extent: scale * 100.0,
            expect_positive_volume: true,
            volume_monotonicity: vec![],
            expect_rebuild_error: false,
            expected_volume: None,
            expected_volume_tol_rel: None,
            expected_solid_count: None,
            expected_shell_count: None,
            derived_meta: true,
        },
        generator_version: PROSPECT_META_VERSION,
        featured: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_meta_reads_a_corpus_document_like_its_own_meta() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay");
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("C0059.waffle")).unwrap())
                .unwrap();
        let own: AssayMeta =
            serde_json::from_str(&std::fs::read_to_string(dir.join("C0059.meta.json")).unwrap())
                .unwrap();
        let derived = derive_meta("X", &doc).unwrap();
        assert_eq!(derived.operations.len(), own.operations.len());
        for (d, o) in derived.operations.iter().zip(&own.operations) {
            assert_eq!(d.kind, o.kind);
            assert_eq!(d.is_cut, o.is_cut);
            assert!((d.depth_or_angle - o.depth_or_angle).abs() < 1e-12);
        }
        assert!(derived.oracles.derived_meta);
        assert!(derived.scale > 0.0);
    }

    #[test]
    fn derive_meta_refuses_a_document_without_features() {
        let doc: Value = serde_json::json!({ "tabs": [] });
        assert!(derive_meta("X", &doc).is_err());
    }
}

// ── Signatures (spec §3.4) ──────────────────────────────────────────────

/// Collapse the numbers a message carries so two findings of the same kind
/// share one key: digits (incl. floats / exponents) → `#`, uuid-ish runs
/// → `<id>`.
fn strip_numbers(s: &str) -> String {
    // uuids first (8-4-4-4-12 hex tokens) → `<id>`; then digit runs → `#`.
    let s: String = s
        .split(' ')
        .map(|tok| {
            let core = tok.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '-');
            let is_uuid = core.len() == 36
                && core.split('-').map(str::len).collect::<Vec<_>>() == [8, 4, 4, 4, 12]
                && core.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
            if is_uuid {
                tok.replace(core, "<id>")
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::with_capacity(s.len());
    let mut in_num = false;
    let mut hex_run = 0usize;
    for c in s.chars() {
        let numeric = c.is_ascii_digit() || (in_num && matches!(c, '.' | 'e' | 'E' | '-' | '+'));
        if numeric {
            if !in_num {
                out.push('#');
                in_num = true;
            }
            continue;
        }
        in_num = false;
        if c.is_ascii_hexdigit() || c == '-' {
            hex_run += 1;
        } else {
            hex_run = 0;
        }
        out.push(c);
        // a 36-char uuid renders as `#`-runs with dashes; good enough.
        let _ = hex_run;
    }
    out
}

/// A finding's signature — what the minimizer preserves and the deduper
/// keys on (`specs/assay_prospector.md` §3.4).
pub fn signature(category: &crate::assay::categorize::Category, detail: &str) -> String {
    use crate::assay::categorize::Category as C;
    match category {
        C::SupportedCorrect => "correct".to_string(),
        C::SupportedWrong => {
            // "oracle: detail; oracle: detail" → the sorted set of oracle names.
            let mut names: Vec<&str> = detail
                .split("; ")
                .filter_map(|f| f.split_once(':').map(|(n, _)| n.trim()))
                .collect();
            names.sort_unstable();
            names.dedup();
            format!("wrong[{}]", names.join(","))
        }
        C::Unsupported(r) => format!("unsupported({})", r.label()),
        C::ExpectedError => "expected-error".to_string(),
        C::Error => {
            // Keep the typed tail: after the last "kernel error:" if present,
            // else after the last "error:" marker, else the whole detail.
            let tail = detail
                .rsplit_once("kernel error:")
                .or_else(|| detail.rsplit_once("error:"))
                .map(|(_, t)| t.trim())
                .unwrap_or(detail);
            let tail = strip_numbers(tail);
            // FaceId(#) etc. are fine; cap the length.
            let tail: String = tail.chars().take(160).collect();
            format!("error[{tail}]")
        }
        C::Timeout => "timeout".to_string(),
        C::SkippedSlow => "skipped".to_string(),
    }
}

// ── Verdict subprocess plumbing (spec §3.3) ─────────────────────────────

/// One line of the prospector's `report.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportLine {
    pub id: String,
    pub seed: u64,
    pub index: u64,
    pub category: String,
    pub signature: String,
    pub detail: String,
    /// `Recipe::summary` — the step list at a glance.
    pub summary: String,
    pub steps: usize,
    pub scale: f64,
    /// The harness error that stopped the build, if any.
    pub build_stopped_by: Option<String>,
    /// CPU seconds the verdict subprocess used.
    pub cpu_secs: f64,
    /// Mutants only: the parent document's id and the knob turned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knob: Option<String>,
}

/// The line a verdict child prints; the parent parses nothing else.
pub const OUTCOME_PREFIX: &str = "PROSPECT_OUTCOME\t";

/// Summed user+system CPU seconds of a process (Linux `/proc/<pid>/stat`).
#[cfg(target_os = "linux")]
pub fn process_cpu_secs(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = f.get(11)?.parse().ok()?;
    let stime: u64 = f.get(12)?.parse().ok()?;
    Some((utime + stime) as f64 / 100.0)
}

#[cfg(not(target_os = "linux"))]
pub fn process_cpu_secs(_pid: u32) -> Option<f64> {
    None
}

/// Generate, build, save and judge ONE candidate — the body of the verdict
/// child. Files land as `<out>/candidates/<id>.{waffle,meta.json,lineage.json}`.
/// Returns the outcome (category, detail) plus the build report.
pub fn judge_generated(
    out: &std::path::Path,
    seed: u64,
    index: u64,
) -> Result<
    (
        crate::assay::categorize::CaseOutcome,
        gen3::Recipe,
        gen3::BuildReport,
    ),
    String,
> {
    let recipe = gen3::generate(seed, index);
    let id = recipe.id();
    let (mut builder, report) = gen3::build(&recipe);
    let waffle = builder.save().map_err(|e| format!("save: {e}"))?;
    let doc: Value = serde_json::from_str(&waffle).map_err(|e| format!("parse: {e}"))?;
    let meta = derive_meta(&id, &doc)?;
    let dir = out.join("candidates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{id}.waffle")), &waffle).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!("{id}.meta.json")),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!("{id}.lineage.json")),
        serde_json::to_string_pretty(&serde_json::json!({
            "kind": "generated",
            "recipe": recipe,
            "build": report,
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let outcome = crate::assay::categorize::categorize(&id, &waffle, &meta);
    Ok((outcome, recipe, report))
}

/// Judge an existing candidate on disk (`<stem>.waffle` + `<stem>.meta.json`).
pub fn judge_document(
    stem: &std::path::Path,
) -> Result<crate::assay::categorize::CaseOutcome, String> {
    let id = stem
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("candidate")
        .to_string();
    let waffle =
        std::fs::read_to_string(stem.with_extension("waffle")).map_err(|e| e.to_string())?;
    let meta_path = stem.with_extension("meta.json");
    let meta: AssayMeta = match std::fs::read_to_string(&meta_path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string())?,
        Err(_) => {
            let doc: Value = serde_json::from_str(&waffle).map_err(|e| e.to_string())?;
            derive_meta(&id, &doc)?
        }
    };
    Ok(crate::assay::categorize::categorize(&id, &waffle, &meta))
}

/// Build, save and judge a recipe file (`<stem>.recipe.json`) — the verdict
/// child's third mode, used by the minimizer. Writes `<stem>.waffle` and
/// `<stem>.meta.json` next to it.
pub fn judge_recipe(
    stem: &std::path::Path,
) -> Result<(crate::assay::categorize::CaseOutcome, gen3::BuildReport), String> {
    let recipe: gen3::Recipe = serde_json::from_str(
        &std::fs::read_to_string(stem.with_extension("recipe.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("recipe: {e}"))?;
    let id = stem
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("candidate")
        .to_string();
    let (mut builder, report) = gen3::build(&recipe);
    let waffle = builder.save().map_err(|e| format!("save: {e}"))?;
    let doc: Value = serde_json::from_str(&waffle).map_err(|e| format!("parse: {e}"))?;
    let meta = derive_meta(&id, &doc)?;
    std::fs::write(stem.with_extension("waffle"), &waffle).map_err(|e| e.to_string())?;
    std::fs::write(
        stem.with_extension("meta.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        crate::assay::categorize::categorize(&id, &waffle, &meta),
        report,
    ))
}

/// A filesystem-safe slug for a signature (directory name under `findings/`).
pub fn signature_slug(sig: &str) -> String {
    let mut out = String::new();
    for c in sig.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 72 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    // A short hash keeps two long signatures with the same prefix apart.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in sig.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{trimmed}-{:04x}", h & 0xffff)
}

/// Candidate id of a mutant: `M<seed hex>-<index>`.
pub fn mutant_id(seed: u64, index: u64) -> String {
    format!("M{:08x}-{:05}", seed & 0xFFFF_FFFF, index)
}

/// Mutate `parent` (a `.waffle` path) with the `(seed, index)` stream, save
/// the mutant under `<out>/candidates/`, derive its meta and judge it — the
/// verdict child's mutation mode. Returns the outcome and the knob label.
pub fn judge_mutant(
    out: &std::path::Path,
    parent: &std::path::Path,
    seed: u64,
    index: u64,
) -> Result<(crate::assay::categorize::CaseOutcome, String), String> {
    let id = mutant_id(seed, index);
    let text = std::fs::read_to_string(parent).map_err(|e| format!("parent: {e}"))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("parent json: {e}"))?;
    let mut rng = gen3::Rng::new(seed ^ index.wrapping_mul(0xA24B_AED4_963E_E407));
    let (mutant, knob) =
        mutate::mutate(&doc, &mut rng).ok_or_else(|| "nothing to mutate".to_string())?;
    let waffle = serde_json::to_string(&mutant).map_err(|e| e.to_string())?;
    let meta = derive_meta(&id, &mutant)?;
    let dir = out.join("candidates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{id}.waffle")), &waffle).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!("{id}.meta.json")),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!("{id}.lineage.json")),
        serde_json::to_string_pretty(&serde_json::json!({
            "kind": "mutated",
            "parent": parent.display().to_string(),
            "knob": knob.label(),
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        crate::assay::categorize::categorize(&id, &waffle, &meta),
        knob.label(),
    ))
}

/// Judge AND measure a document: the categorizer's verdict plus the live
/// bodies' summed signed volume and exact-bit χ (the quantities the
/// metamorphic identities compare). Tessellation at the exact oracle's
/// tolerance for the meta's scale.
pub fn measure_document(waffle: &str, meta: &AssayMeta) -> metamorphic::Measurement {
    let outcome = crate::assay::categorize::categorize(&meta.id, waffle, meta);
    let mut m = metamorphic::Measurement {
        category: outcome.category.label(),
        detail: outcome.detail,
        volume: None,
        chi: None,
        bodies: 0,
    };
    let mut builder = crate::ModelBuilder::kernel_v2();
    if builder.load(waffle).is_err() {
        return m;
    }
    let tol = crate::assay::volume_oracle_doc::oracle_tol(meta.scale);
    if let Ok(meshes) = builder.tessellate_live_with_tol(tol) {
        let meshes: Vec<_> = meshes
            .into_iter()
            .filter(|mm| !mm.indices.is_empty())
            .collect();
        m.bodies = meshes.len();
        if !meshes.is_empty() {
            m.volume = Some(
                meshes
                    .iter()
                    .map(crate::helpers::mesh_signed_volume)
                    .sum::<f64>(),
            );
            let chis: Vec<Option<i64>> = meshes
                .iter()
                .map(|mm| {
                    crate::oracle::check_mesh_euler_characteristic_with_shells(mm, 2, None)
                        .value
                        .map(|c| c as i64)
                })
                .collect();
            m.chi = chis.iter().copied().sum::<Option<i64>>();
        }
    }
    m
}

/// Measure a candidate on disk (`<stem>.waffle`, meta derived if absent).
pub fn measure_stem(stem: &std::path::Path) -> Result<metamorphic::Measurement, String> {
    let id = stem
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("candidate")
        .to_string();
    let waffle =
        std::fs::read_to_string(stem.with_extension("waffle")).map_err(|e| e.to_string())?;
    let meta_path = stem.with_extension("meta.json");
    let meta: AssayMeta = match std::fs::read_to_string(&meta_path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string())?,
        Err(_) => {
            let doc: Value = serde_json::from_str(&waffle).map_err(|e| e.to_string())?;
            derive_meta(&id, &doc)?
        }
    };
    Ok(measure_document(&waffle, &meta))
}

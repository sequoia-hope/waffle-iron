//! Promotion — a finding becomes a corpus case (`specs/assay_prospector.md` §8).
//!
//! Writes `P00NN.waffle` + `P00NN.meta.json` into the corpus directory and
//! appends the manifest entry. The category pin in `tests/assay_kv2.rs` and
//! the `results.json` refresh are the promoting session's own steps: a
//! promotion is curated, never automatic.

use std::path::Path;

use crate::assay::gen::{AssayMeta, CorpusManifest, ManifestEntry};

/// The next free `P` id in the corpus manifest.
pub fn next_p_id(corpus_dir: &Path) -> Result<String, String> {
    let manifest = read_manifest(corpus_dir)?;
    let n = manifest
        .cases
        .iter()
        .filter_map(|c| c.id.strip_prefix('P'))
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    Ok(format!("P{:04}", n + 1))
}

fn read_manifest(corpus_dir: &Path) -> Result<CorpusManifest, String> {
    let s = std::fs::read_to_string(corpus_dir.join("manifest.json"))
        .map_err(|e| format!("manifest: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("manifest: {e}"))
}

/// Write the case files and the manifest entry. `meta.id` is rewritten to
/// `id`; the description is `description` verbatim (put the signature and
/// the lineage there). The manifest's `count` follows its `cases`.
pub fn write_case(
    corpus_dir: &Path,
    id: &str,
    waffle_json: &str,
    meta: &AssayMeta,
    description: &str,
) -> Result<(), String> {
    let mut manifest = read_manifest(corpus_dir)?;
    if manifest.cases.iter().any(|c| c.id == id) {
        return Err(format!("{id} already in the manifest"));
    }
    let mut meta = meta.clone();
    meta.id = id.to_string();
    meta.description = description.to_string();
    meta.featured = true;
    let waffle_filename = format!("{id}.waffle");
    let meta_filename = format!("{id}.meta.json");
    std::fs::write(corpus_dir.join(&waffle_filename), waffle_json).map_err(|e| e.to_string())?;
    std::fs::write(
        corpus_dir.join(&meta_filename),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    manifest.cases.push(ManifestEntry {
        id: id.to_string(),
        filename: waffle_filename,
        meta_filename,
        description: description.to_string(),
        featured: true,
    });
    manifest.count = manifest.cases.len();
    std::fs::write(
        corpus_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_p_id_counts_from_the_manifest() {
        let dir = std::env::temp_dir().join(format!("prospect-promote-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = CorpusManifest {
            master_seed: 0,
            count: 1,
            generator_version: 0,
            cases: vec![ManifestEntry {
                id: "P0007".into(),
                filename: "P0007.waffle".into(),
                meta_filename: "P0007.meta.json".into(),
                description: String::new(),
                featured: true,
            }],
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        assert_eq!(next_p_id(&dir).unwrap(), "P0008");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

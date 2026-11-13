//! EXPERIMENT ONLY (branch `exp/paper-skeleton`; not for main).
//!
//! `YANG_SKELETON` switches mechanisms OFF, in release builds too, so the
//! corpus can be run with them absent. A comma-separated list of groups, or
//! `all`. A group name also switches off every site under it (`s1` covers
//! `s1.rim`, `s1.junction`, `s1.p3a`, `s1.eip`).
//!
//! - `s1.rim`       `phantom_min_rim_segments` (operand-wide rim boost).
//! - `s1.junction`  `rim_junction_overrides` (rim-junction insertion).
//! - `s1.p3a`       `junction_stage1_overrides` (edge x face pierce mints).
//! - `s1.eip`       `stage0::edge_in_plane::conformity_overrides` (§4.5.5 one
//!                  dimension down: an edge lying in a partner planar face).
//! - `s4`           Stage-4/5 post-hoc repairs: fold-merge / absorb / rehome,
//!                  `collapse_subtauwork_mesh_edges` (both sites),
//!                  `repair_fan_cluster`, the late pinch-vertex split.
//! - `transit`      the crease transit (`transit.crease`) and the corner
//!                  transit (`transit.corner`).
//! - `s1fold`       Stage 1's cross-patch folds (N88): the cylinder-pair
//!                  phantom fold and the thin-band rim fold.
//!
//! `YANG_SKELETON_LOG=<file>`: each process appends the name of every site
//! it reached with its mechanism switched off, once per site, followed by
//! the corpus case (`ASSAY_CASE`). One corpus
//! case is one process, so the line count per site is the number of cases
//! in which that mechanism's call site was reached.
//!
//! With the mechanisms on, the same log records `transit.crease.called` /
//! `transit.crease.solved` (the crease transit was attempted / returned a
//! junction) and `transit.corner.mutated` (the corner transit changed the
//! mesh), again once per process.
//!
//! Left ON: the construct pass (it carries §4.3.4 / §4.4.1 today),
//! `update_patches`, the §4.3 dedup collapses, the §4.5.2 loop, Stage 0
//! (including its relocation), and Stage 1's own-validity folds.
use std::sync::{Mutex, OnceLock};

pub(crate) fn off(site: &str) -> bool {
    static SET: OnceLock<Vec<String>> = OnceLock::new();
    let set = SET.get_or_init(|| {
        std::env::var("YANG_SKELETON")
            .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
            .unwrap_or_default()
    });
    let hit = set.iter().any(|s| {
        s == "all"
            || s == site
            || (site.len() > s.len()
                && site.starts_with(s.as_str())
                && site.as_bytes()[s.len()] == b'.')
    });
    if hit {
        log_once(site);
    }
    hit
}

/// Census for a run with the mechanisms ON: record that `event` happened in
/// this process (once), in the same log.
pub(crate) fn note(event: &str) {
    log_once(event);
}

fn log_once(site: &str) {
    static SEEN: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    let Ok(path) = std::env::var("YANG_SKELETON_LOG") else {
        return;
    };
    let mut seen = SEEN.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    if seen.iter().any(|s| s == site) {
        return;
    }
    seen.push(site.to_string());
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
        let _ = writeln!(f, "{site} {case}");
    }
}

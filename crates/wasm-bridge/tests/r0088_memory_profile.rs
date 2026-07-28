//! Peak-RSS profiler for a replayed assay document.
//!
//! `ASSAY_CASE=<id>` selects the case (default R0088); `R0088_PREFIX=n`
//! truncates the feature list to attribute cost to a single feature.
//!
//!   for n in 1 2 3 4 5 6; do R0088_PREFIX=$n cargo test -p wasm-bridge --release \
//!     --test r0088_memory_profile -- --ignored --nocapture; done
//!
//! ONE PREFIX PER PROCESS is not a detail — it is the whole methodology. Within
//! a single process the allocator's high-water mark never falls, so every
//! prefix after the first is measured against an already-inflated baseline. The
//! first version of this file looped in-process and reported ~800 MiB for
//! R0088's prefixes 5 and 6; re-measured one-per-process those are +0.0 MiB and
//! +0.5 MiB. **Those 800 MiB extrudes never existed.** A profiler that
//! manufactures work is worse than none.
//!
//! ## Findings (2026-07-28, after the c932e45c octree duplication budget)
//!
//! R0088, per prefix: sketches and Extrude 1 ~5 MiB; Extrude 2 **92.9 MiB**
//! (was 6,950); Sketch 3 +0.0; Extrude 3 **+0.5**. Stage breakdown of Extrude
//! 2: arrangement +70.5 MiB, patches +0.0, `compute_inside_out` **+0.1 MiB**
//! (was +6,036).
//!
//! Sweep over the twelve heaviest corpus cases (by assay wall time) plus
//! R0088 — peak RSS, whole document:
//!
//!     R0019  866 MiB    R0085  493    R0081  373    R0047  335    F0065  312
//!     F0072  237 MiB    R0054  217    F0070  137    F0069  120    F0090  101
//!     F0085   93 MiB    R0088   93    F0088   78
//!
//! All are an order of magnitude under the wasm32 4 GiB ceiling, so no other
//! corpus case is at OOM risk today. Note the sweep was ordered by TIME, which
//! is only a proxy for memory: the slowest case (F0072, 135 s) is 237 MiB while
//! the heaviest (R0019) is fourth by time. A memory-ordered sweep would need
//! every case measured, which this tool can do but nothing yet demands.
//!
//! **R0019, the worst, is NOT a pathology — it is a big model.** Its Revolve 2
//! feeds the arrangement **207,400 input triangles** (R0088: 7,506) and gets
//! 212,876 out, x1.03. The arrangement costs +668 MiB there, i.e. ~3.4 KB per
//! triangle — actually BETTER per-triangle than R0088's ~10 KB, which carries
//! fixed overhead over far fewer triangles. And `compute_inside_out` adds
//! **+0.0 MiB at 207K triangles**, so the octree fix holds at 28x the scale it
//! was diagnosed on.
//!
//! Consequence for anyone picking this up: the remaining lever is **Stage-1
//! tessellation density** (why does that revolve emit 207K triangles?), NOT
//! arrangement efficiency. That is a fidelity/performance trade-off and a
//! design decision, not a defect — which is why it is written down here rather
//! than quietly changed.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use wasm_bridge::{dispatch, EngineState, UiToEngine};

/// Resident set size in bytes, from `/proc/self/statm` (field 2 = resident
/// pages). Linux-only, which is fine — this is a dev diagnostic.
fn rss_bytes() -> u64 {
    let Ok(s) = fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let mut it = s.split_whitespace();
    let _size = it.next();
    let resident: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    resident * 4096
}

struct PeakSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl PeakSampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let (s, p) = (stop.clone(), peak.clone());
        let handle = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                let r = rss_bytes();
                p.fetch_max(r, Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        });
        Self {
            stop,
            peak,
            handle: Some(handle),
        }
    }

    fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.peak.load(Ordering::Relaxed)
    }
}

fn case_json(id: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../app/tests/cases/assay/{id}.waffle",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).expect("case parses")
}

/// Return the document with only the first `n` features retained.
fn truncated(mut doc: serde_json::Value, n: usize) -> String {
    let feats = doc["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .expect("feature array")
        .iter()
        .take(n)
        .cloned()
        .collect::<Vec<_>>();
    doc["tabs"][0]["kind"]["features"]["features"] = serde_json::Value::Array(feats);
    serde_json::to_string(&doc).expect("re-encode")
}

fn replay(json: &str) -> (u64, std::time::Duration, &'static str) {
    let msg: UiToEngine = serde_json::from_str(&format!(
        r#"{{"type":"LoadProject","data":{}}}"#,
        serde_json::to_string(json).expect("encode")
    ))
    .expect("LoadProject deserializes");

    let sampler = PeakSampler::start();
    let t0 = std::time::Instant::now();

    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let resp = dispatch(&mut state, msg, &mut kernel);

    let elapsed = t0.elapsed();
    let peak = sampler.finish();
    let kind = match resp {
        wasm_bridge::EngineToUi::Error { .. } => "Error",
        wasm_bridge::EngineToUi::ModelUpdated { .. } => "ModelUpdated",
        _ => "other",
    };
    (peak, elapsed, kind)
}

/// Replay ONE prefix, selected by `R0088_PREFIX`. One prefix per PROCESS is the
/// only honest way to read peak RSS: within a single process the allocator's
/// high-water mark never falls, so every prefix after the first is measured
/// against an already-inflated baseline and its "delta" is meaningless. The
/// pre-fix run reported ~800 MiB for prefixes 5 and 6 that way; those were
/// artifacts of prefix 4's 6.9 GiB peak, not costs of their own.
///
/// Driver:
///   for n in 1 2 3 4 5 6; do R0088_PREFIX=$n cargo test -p wasm-bridge --release \
///     --test r0088_memory_profile -- --ignored --nocapture; done
#[test]
#[ignore = "diagnostic: set R0088_PREFIX=n; one prefix per process (see doc comment)"]
fn r0088_peak_memory_by_feature_prefix() {
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "R0088".to_string());
    let doc = case_json(&case);
    let names: Vec<String> = doc["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .expect("feature array")
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("?").to_string())
        .collect();

    // Whole document by default; `R0088_PREFIX=n` truncates for attribution.
    let n: usize = match std::env::var("R0088_PREFIX") {
        Ok(v) => v.parse().expect("R0088_PREFIX must be an integer"),
        Err(_) => names.len(),
    };
    assert!((1..=names.len()).contains(&n), "prefix out of range");

    let json = truncated(doc, n);
    let (peak, elapsed, kind) = replay(&json);
    eprintln!(
        "PEAK {case} {n}/{} through {:<12} peak_rss={:>8.2} MiB  {:>7.1}s  {kind}",
        names.len(),
        names[n - 1],
        peak as f64 / 1024.0 / 1024.0,
        elapsed.as_secs_f64(),
    );
}

//! Where does assay R0088's ~6.8 GiB peak actually go?
//!
//! R0088 is SIX features — three sketches and three extrudes — at coordinate
//! magnitude ~700. That is wildly disproportionate to 6.8 GiB, so the first
//! question is not "how do we allocate less" but "which operation allocates
//! this much". This truncates the document's feature list and replays each
//! prefix, sampling RSS throughout, so the cost lands on a specific feature.
//!
//! Run explicitly (it is slow and memory-hungry by construction):
//!   cargo test -p wasm-bridge --release --test r0088_memory_profile -- --ignored --nocapture

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

    let base = rss_bytes();
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
    (peak.saturating_sub(base), elapsed, kind)
}

#[test]
#[ignore = "diagnostic: replays R0088 prefixes; multi-GiB and minutes by design"]
fn r0088_peak_memory_by_feature_prefix() {
    let doc = case_json("R0088");
    let names: Vec<String> = doc["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .expect("feature array")
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("?").to_string())
        .collect();
    eprintln!("R0088 features: {names:?}");

    for n in 1..=names.len() {
        let json = truncated(doc.clone(), n);
        let (peak, elapsed, kind) = replay(&json);
        eprintln!(
            "  prefix {n}/{} (through {:<10}) peak_delta={:>8.2} MiB  {:>7.1}s  {kind}",
            names.len(),
            names[n - 1],
            peak as f64 / 1024.0 / 1024.0,
            elapsed.as_secs_f64(),
        );
    }
}

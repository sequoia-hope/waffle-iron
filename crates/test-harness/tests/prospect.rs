//! Assay prospector — entry points and acceptance cases.
//!
//! Spec: `specs/assay_prospector.md`. P1: a document that is not a corpus
//! file is judged by the corpus categorizer through a meta derived from
//! the document itself.

use test_harness::assay::categorize::{categorize, Category};
use test_harness::assay::prospect::derive_meta;
use test_harness::ModelBuilder;

/// Star polygon vertices (alternating outer / inner radius), sketch-local.
fn star(points: u32, inner_r: f64, outer_r: f64) -> Vec<(f64, f64)> {
    let n = points * 2;
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * (k as f64) / (n as f64);
            let r = if k % 2 == 0 { outer_r } else { inner_r };
            (r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The first finding of the loud generative runners (2026-09-27, spec §8):
/// an octagon prism on the Y plane unioned with a 4-point needle star
/// (r_in = 2, r_out ≈ 22) on the X plane, both at y = 22.42. The union
/// STOPs loudly in tessellation ("planar triangle collapsed at render
/// precision"), and the categorizer must report exactly that as ERROR
/// from the saved document — the P1 acceptance case.
fn needle_star_document() -> String {
    let mut b = ModelBuilder::kernel_v2();
    let y = 22.41755130980609;
    b.polygon_sketch(
        "sk_0",
        [0.0, y, 0.0],
        [0.0, 1.0, 0.0],
        &[
            (10.01142855584012, -30.39476694290923),
            (12.494294560269523, -24.569000763226),
            (10.130507177517845, -18.693910603455162),
            (4.304740997834618, -16.21104459902576),
            (-1.5703491619362175, -18.574831981777436),
            (-4.0532151663656215, -24.40059816146067),
            (-1.6894277836139446, -30.2756883212315),
            (4.136338396069275, -32.7585543256609),
        ],
    )
    .unwrap();
    b.extrude("ext_0", "sk_0", 9.13931723988505).unwrap();
    b.polygon_sketch(
        "sk_1",
        [0.0, y, 0.0],
        [1.0, 0.0, 0.0],
        &star(4, 2.0, 22.0331315381885),
    )
    .unwrap();
    b.extrude_no_merge("ext_1", "sk_1", 7.854423622158365)
        .unwrap();
    // The union fails loudly; the feature stays in the tree with its error.
    let union = b.boolean_union("bool_0", "ext_0", "ext_1");
    assert!(union.is_err(), "the needle-star union was expected to STOP");
    b.save().unwrap()
}

#[test]
fn needle_star_union_categorizes_as_the_recorded_error() {
    let waffle = needle_star_document();
    let doc: serde_json::Value = serde_json::from_str(&waffle).unwrap();
    let meta = derive_meta("X-needle-star", &doc).unwrap();
    assert_eq!(meta.operations.len(), 3, "{:?}", meta.operations);
    assert_eq!(meta.operations[2].kind, "boolean-union");
    let outcome = categorize("X-needle-star", &waffle, &meta);
    assert_eq!(
        outcome.category,
        Category::Error,
        "category {:?}: {}",
        outcome.category,
        outcome.detail
    );
    assert!(
        outcome
            .detail
            .contains("planar triangle collapsed at render precision"),
        "signature not found in: {}",
        outcome.detail
    );
}

/// A plain corpus-shaped document (one box) is SUPPORTED_CORRECT under a
/// derived meta — the derivation adds no false expectation.
#[test]
fn derived_meta_keeps_a_correct_document_correct() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 2.0, 1.0)
        .unwrap();
    b.extrude("box", "sk", 0.5).unwrap();
    let waffle = b.save().unwrap();
    let doc: serde_json::Value = serde_json::from_str(&waffle).unwrap();
    let meta = derive_meta("X-box", &doc).unwrap();
    let outcome = categorize("X-box", &waffle, &meta);
    assert_eq!(
        outcome.category,
        Category::SupportedCorrect,
        "{}",
        outcome.detail
    );
}

// ── The search loop (spec §3.3, §9) ──────────────────────────────────────

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use test_harness::assay::prospect::{
    self, gen3, judge_document, judge_generated, process_cpu_secs, signature, ReportLine,
    OUTCOME_PREFIX,
};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn out_dir(seed: u64) -> PathBuf {
    match std::env::var("PROSPECT_OUT") {
        Ok(p) => PathBuf::from(p),
        Err(_) => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/prospect")
            .join(format!("seed-{seed}")),
    }
}

/// Verdict CHILD: `PROSPECT_OUT` + `PROSPECT_SEED` + `PROSPECT_INDEX` ⇒
/// generate + build + judge; or `PROSPECT_CANDIDATE=<stem>` ⇒ judge a
/// document on disk. Prints exactly one `PROSPECT_OUTCOME\t{json}` line.
#[test]
#[ignore = "verdict subprocess entry (spawned by prospect_run); not a test"]
fn prospect_judge() {
    let json = if let Ok(stem) = std::env::var("PROSPECT_CANDIDATE") {
        match judge_document(Path::new(&stem)) {
            Ok(o) => serde_json::json!({
                "category": o.category.label(),
                "detail": o.detail,
            }),
            Err(e) => serde_json::json!({ "category": "ERROR", "detail": format!("judge: {e}") }),
        }
    } else {
        let seed = env_u64("PROSPECT_SEED", 1);
        let index = env_u64("PROSPECT_INDEX", 0);
        let out = out_dir(seed);
        match judge_generated(&out, seed, index) {
            Ok((o, recipe, build)) => serde_json::json!({
                "category": o.category.label(),
                "detail": o.detail,
                "summary": recipe.summary(),
                "steps": recipe.steps.len(),
                "scale": recipe.scale,
                "build_stopped_by": build.stopped_by,
            }),
            Err(e) => {
                serde_json::json!({ "category": "ERROR", "detail": format!("generate: {e}") })
            }
        }
    };
    println!("{OUTCOME_PREFIX}{json}");
}

/// Spawn the verdict child for `(seed, index)` under a CPU budget.
fn spawn_verdict(seed: u64, index: u64, out: &Path, budget: Duration) -> ReportLine {
    use std::process::{Command, Stdio};
    let id = gen3::candidate_id(seed, index);
    let mut line = ReportLine {
        id: id.clone(),
        seed,
        index,
        category: "ERROR".into(),
        signature: String::new(),
        detail: String::new(),
        summary: String::new(),
        steps: 0,
        scale: 0.0,
        build_stopped_by: None,
        cpu_secs: 0.0,
    };
    let exe = std::env::current_exe().expect("current_exe");
    let wall_cap = Duration::from_secs_f64((budget.as_secs_f64() * 4.0).max(120.0));
    let mut child = match Command::new(&exe)
        .args(["--exact", "prospect_judge", "--ignored", "--nocapture"])
        .env("PROSPECT_SEED", seed.to_string())
        .env("PROSPECT_INDEX", index.to_string())
        .env("PROSPECT_OUT", out)
        .env_remove("PROSPECT_CANDIDATE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            line.detail = format!("driver: cannot spawn: {e}");
            return line;
        }
    };
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let pid = child.id();
    let start = Instant::now();
    let mut last_cpu = 0.0;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => {
                line.detail = format!("driver: wait failed: {e}");
                return line;
            }
        }
        if let Some(cpu) = process_cpu_secs(pid) {
            last_cpu = cpu;
            if cpu > budget.as_secs_f64() {
                timed_out = true;
            }
        }
        if start.elapsed() > wall_cap {
            timed_out = true;
        }
        if timed_out {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    line.cpu_secs = last_cpu;
    let output = reader.join().unwrap_or_default();
    if timed_out {
        line.category = "TIMEOUT".into();
        line.detail = format!("cpu {last_cpu:.1}s > budget {:.0}s", budget.as_secs_f64());
        line.signature = "timeout".into();
        return line;
    }
    let Some(json_line) = output.lines().find_map(|l| l.strip_prefix(OUTCOME_PREFIX)) else {
        line.detail = format!(
            "driver: child printed no outcome (panic/abort?); tail: {}",
            output
                .chars()
                .rev()
                .take(300)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
        line.signature = "panic".into();
        return line;
    };
    let v: serde_json::Value = match serde_json::from_str(json_line) {
        Ok(v) => v,
        Err(e) => {
            line.detail = format!("driver: bad outcome json: {e}");
            return line;
        }
    };
    line.category = v["category"].as_str().unwrap_or("ERROR").to_string();
    line.detail = v["detail"].as_str().unwrap_or("").to_string();
    line.summary = v["summary"].as_str().unwrap_or("").to_string();
    line.steps = v["steps"].as_u64().unwrap_or(0) as usize;
    line.scale = v["scale"].as_f64().unwrap_or(0.0);
    line.build_stopped_by = v["build_stopped_by"].as_str().map(str::to_string);
    let cat = test_harness::assay::categorize::Category::from_label(&line.category)
        .unwrap_or(test_harness::assay::categorize::Category::Error);
    line.signature = signature(&cat, &line.detail);
    line
}

/// The prospector's search: `PROSPECT_SEED` (1), `PROSPECT_COUNT` (200),
/// `PROSPECT_JOBS` (8), `PROSPECT_BUDGET_SECS` (600), `PROSPECT_OUT`
/// (`target/prospect/seed-<seed>`). Resumable: ids already in
/// `report.jsonl` are skipped. Never run during a full assay.
#[test]
#[ignore = "the prospector search (long, release); run by hand — see specs/assay_prospector.md §9"]
fn prospect_run() {
    let seed = env_u64("PROSPECT_SEED", 1);
    let count = env_u64("PROSPECT_COUNT", 200);
    let jobs = env_u64("PROSPECT_JOBS", 8).max(1) as usize;
    let budget = Duration::from_secs(env_u64("PROSPECT_BUDGET_SECS", 600));
    let out = out_dir(seed);
    std::fs::create_dir_all(&out).expect("out dir");
    let report_path = out.join("report.jsonl");

    // Resume: ids already judged.
    let done: std::collections::HashSet<String> = std::fs::read_to_string(&report_path)
        .map(|s| {
            s.lines()
                .filter_map(|l| serde_json::from_str::<ReportLine>(l).ok())
                .map(|r| r.id)
                .collect()
        })
        .unwrap_or_default();
    eprintln!(
        "[prospect] seed={seed} count={count} jobs={jobs} budget={}s out={} (resuming {} done)",
        budget.as_secs(),
        out.display(),
        done.len()
    );

    let report = Arc::new(Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&report_path)
            .expect("report.jsonl"),
    ));
    let next = Arc::new(AtomicU64::new(0));
    let done = Arc::new(done);
    let started = Instant::now();
    let workers: Vec<_> = (0..jobs)
        .map(|_| {
            let next = Arc::clone(&next);
            let done = Arc::clone(&done);
            let report = Arc::clone(&report);
            let out = out.clone();
            std::thread::spawn(move || loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                if index >= count {
                    break;
                }
                let id = gen3::candidate_id(seed, index);
                if done.contains(&id) {
                    continue;
                }
                let line = spawn_verdict(seed, index, &out, budget);
                eprintln!(
                    "[prospect] {} {:<18} {:>6.1}s  {}  {}",
                    line.id, line.category, line.cpu_secs, line.summary, line.signature
                );
                let mut f = report.lock().unwrap();
                let _ = writeln!(f, "{}", serde_json::to_string(&line).unwrap());
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }

    // Summary.
    let lines: Vec<ReportLine> = std::fs::read_to_string(&report_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let mut hist: std::collections::BTreeMap<String, usize> = Default::default();
    let mut findings: std::collections::BTreeMap<String, Vec<&ReportLine>> = Default::default();
    for l in &lines {
        *hist.entry(l.category.clone()).or_default() += 1;
        let interesting =
            !(l.category == "SUPPORTED_CORRECT" || l.category.starts_with("UNSUPPORTED"));
        if interesting {
            findings.entry(l.signature.clone()).or_default().push(l);
        }
    }
    eprintln!(
        "[prospect] done: {} candidates in {:.0}s",
        lines.len(),
        started.elapsed().as_secs_f64()
    );
    for (k, v) in &hist {
        eprintln!("[prospect]   {k:<24} {v}");
    }
    eprintln!("[prospect] {} finding signature(s):", findings.len());
    for (sig, ls) in &findings {
        let smallest = ls.iter().min_by_key(|l| l.steps).unwrap();
        eprintln!(
            "[prospect]   ×{:<3} {}  e.g. {} ({} steps): {}",
            ls.len(),
            sig,
            smallest.id,
            smallest.steps,
            smallest.summary
        );
    }
    let _ = prospect::PROSPECT_META_VERSION;
}

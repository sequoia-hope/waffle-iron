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
    self, gen3, judge_generated, judge_mutant, judge_recipe, metamorphic, minimize,
    process_cpu_secs, promote, signature, signature_slug, ReportLine, OUTCOME_PREFIX,
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
    let json = if let Ok(parent) = std::env::var("PROSPECT_MUTATE_PARENT") {
        let seed = env_u64("PROSPECT_SEED", 1);
        let index = env_u64("PROSPECT_INDEX", 0);
        let out = PathBuf::from(std::env::var("PROSPECT_OUT").expect("PROSPECT_OUT"));
        match judge_mutant(&out, Path::new(&parent), seed, index) {
            Ok((o, knob)) => serde_json::json!({
                "category": o.category.label(),
                "detail": o.detail,
                "knob": knob,
            }),
            Err(e) => serde_json::json!({ "category": "ERROR", "detail": format!("mutate: {e}") }),
        }
    } else if let Ok(stem) = std::env::var("PROSPECT_RECIPE") {
        match judge_recipe(Path::new(&stem)) {
            Ok((o, build)) => serde_json::json!({
                "category": o.category.label(),
                "detail": o.detail,
                "build_stopped_by": build.stopped_by,
            }),
            Err(e) => serde_json::json!({ "category": "ERROR", "detail": format!("recipe: {e}") }),
        }
    } else if let Ok(stem) = std::env::var("PROSPECT_CANDIDATE") {
        // Judge + measure (volume, χ, bodies) — the metamorphic driver reads
        // the measurement; a plain judge reads category/detail.
        match prospect::measure_stem(Path::new(&stem)) {
            Ok(m) => serde_json::json!({
                "category": m.category,
                "detail": m.detail,
                "measurement": m,
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

/// Run one verdict child with the given environment under a CPU budget.
/// Returns (category, detail, extra json, cpu_secs) or the driver's own
/// failure as an ERROR/TIMEOUT outcome.
fn run_child(
    envs: &[(&str, String)],
    budget: Duration,
) -> (String, String, serde_json::Value, f64) {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().expect("current_exe");
    let wall_cap = Duration::from_secs_f64((budget.as_secs_f64() * 4.0).max(120.0));
    let mut cmd = Command::new(&exe);
    cmd.args(["--exact", "prospect_judge", "--ignored", "--nocapture"])
        .env_remove("PROSPECT_CANDIDATE")
        .env_remove("PROSPECT_RECIPE")
        .env_remove("PROSPECT_MUTATE_PARENT")
        .env_remove("PROSPECT_INDEX")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                "ERROR".into(),
                format!("driver: cannot spawn: {e}"),
                serde_json::Value::Null,
                0.0,
            )
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
                return (
                    "ERROR".into(),
                    format!("driver: wait failed: {e}"),
                    serde_json::Value::Null,
                    last_cpu,
                )
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
    let output = reader.join().unwrap_or_default();
    if timed_out {
        return (
            "TIMEOUT".into(),
            format!("cpu {last_cpu:.1}s > budget {:.0}s", budget.as_secs_f64()),
            serde_json::Value::Null,
            last_cpu,
        );
    }
    let Some(json_line) = output.lines().find_map(|l| l.strip_prefix(OUTCOME_PREFIX)) else {
        let tail: String = output
            .chars()
            .rev()
            .take(300)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return (
            "ERROR".into(),
            format!("driver: child printed no outcome (panic/abort?); tail: {tail}"),
            serde_json::json!({ "panic": true }),
            last_cpu,
        );
    };
    match serde_json::from_str::<serde_json::Value>(json_line) {
        Ok(v) => (
            v["category"].as_str().unwrap_or("ERROR").to_string(),
            v["detail"].as_str().unwrap_or("").to_string(),
            v,
            last_cpu,
        ),
        Err(e) => (
            "ERROR".into(),
            format!("driver: bad outcome json: {e}"),
            serde_json::Value::Null,
            last_cpu,
        ),
    }
}

fn signature_of(category: &str, detail: &str, extra: &serde_json::Value) -> String {
    if category == "TIMEOUT" {
        return "timeout".into();
    }
    if extra.get("panic").is_some() {
        return "panic".into();
    }
    let cat = test_harness::assay::categorize::Category::from_label(category)
        .unwrap_or(test_harness::assay::categorize::Category::Error);
    signature(&cat, detail)
}

/// Spawn the verdict child for `(seed, index)` under a CPU budget.
fn spawn_verdict(seed: u64, index: u64, out: &Path, budget: Duration) -> ReportLine {
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
        parent: None,
        knob: None,
    };
    let (category, detail, v, cpu) = run_child(
        &[
            ("PROSPECT_SEED", seed.to_string()),
            ("PROSPECT_INDEX", index.to_string()),
            ("PROSPECT_OUT", out.display().to_string()),
        ],
        budget,
    );
    line.cpu_secs = cpu;
    line.signature = signature_of(&category, &detail, &v);
    line.category = category;
    line.detail = detail;
    line.summary = v["summary"].as_str().unwrap_or("").to_string();
    line.steps = v["steps"].as_u64().unwrap_or(0) as usize;
    line.scale = v["scale"].as_f64().unwrap_or(0.0);
    line.build_stopped_by = v["build_stopped_by"].as_str().map(str::to_string);
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

/// Judge a recipe through the child: write it to `<stem>.recipe.json`,
/// run, return the signature.
fn judge_recipe_via_child(recipe: &gen3::Recipe, stem: &Path, budget: Duration) -> Option<String> {
    std::fs::write(
        stem.with_extension("recipe.json"),
        serde_json::to_string_pretty(recipe).ok()?,
    )
    .ok()?;
    let (category, detail, v, _) =
        run_child(&[("PROSPECT_RECIPE", stem.display().to_string())], budget);
    Some(signature_of(&category, &detail, &v))
}

/// P3: minimize every finding of a report (one per signature, the example
/// with the fewest steps), write `findings/<slug>/` with the minimal
/// recipe, its document and meta, and a README. Env: `PROSPECT_SEED` /
/// `PROSPECT_OUT` (which report), `PROSPECT_BUDGET_SECS` (per verdict),
/// `PROSPECT_MIN_STEPS` (verdicts per finding, default 60).
#[test]
#[ignore = "the prospector minimizer (long, release); run by hand after prospect_run — spec §7"]
fn prospect_minimize() {
    let seed = env_u64("PROSPECT_SEED", 1);
    let budget = Duration::from_secs(env_u64("PROSPECT_BUDGET_SECS", 600));
    let min_steps = env_u64("PROSPECT_MIN_STEPS", 60) as usize;
    let out = out_dir(seed);
    let report_path = out.join("report.jsonl");
    let lines: Vec<ReportLine> = std::fs::read_to_string(&report_path)
        .expect("report.jsonl — run prospect_run first")
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let mut by_sig: std::collections::BTreeMap<String, Vec<&ReportLine>> = Default::default();
    for l in &lines {
        let interesting =
            !(l.category == "SUPPORTED_CORRECT" || l.category.starts_with("UNSUPPORTED"));
        if interesting {
            by_sig.entry(l.signature.clone()).or_default().push(l);
        }
    }
    eprintln!(
        "[minimize] {} signature(s) in {}",
        by_sig.len(),
        report_path.display()
    );
    for (sig, ls) in &by_sig {
        let example = ls.iter().min_by_key(|l| l.steps).unwrap();
        let lineage_path = out
            .join("candidates")
            .join(format!("{}.lineage.json", example.id));
        let Ok(lineage) = std::fs::read_to_string(&lineage_path) else {
            eprintln!("[minimize] {sig}: no lineage for {} — skipped", example.id);
            continue;
        };
        let lineage: serde_json::Value = serde_json::from_str(&lineage).unwrap();
        let Ok(recipe) = serde_json::from_value::<gen3::Recipe>(lineage["recipe"].clone()) else {
            eprintln!("[minimize] {sig}: {} has no recipe — skipped", example.id);
            continue;
        };
        let dir = out.join("findings").join(signature_slug(sig));
        std::fs::create_dir_all(&dir).unwrap();
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let mut n = 0usize;
        let mut judge = |r: &gen3::Recipe| -> Option<String> {
            n += 1;
            let stem = work.join(format!("try{n:03}"));
            let s = judge_recipe_via_child(r, &stem, budget);
            eprintln!(
                "[minimize]   {} try{n:03} ({} steps) → {}",
                example.id,
                r.steps.len(),
                s.as_deref().unwrap_or("?")
            );
            s
        };
        let (min, rep) = minimize::minimize(&recipe, sig, &mut judge, min_steps);
        // Materialize the minimal recipe as the finding's document.
        let stem = dir.join(format!("{}-min", example.id));
        let final_sig = judge_recipe_via_child(&min, &stem, budget);
        let readme = format!(
            "# {sig}\n\nseed {} index {} ({}) — {} example(s) in the report\n\n\
             steps {} → {} after {} verdict(s){}\n\nkept reductions:\n{}\n\n\
             minimal recipe: `{}`\nfinal signature: {}\n\nsummary: {}\n",
            example.seed,
            example.index,
            example.id,
            ls.len(),
            rep.steps_before,
            rep.steps_after,
            rep.verdicts_used,
            if rep.budget_exhausted {
                " (budget exhausted)"
            } else {
                ""
            },
            rep.kept
                .iter()
                .map(|k| format!("- {k}"))
                .collect::<Vec<_>>()
                .join("\n"),
            min.summary(),
            final_sig.as_deref().unwrap_or("?"),
            example.detail,
        );
        std::fs::write(dir.join("README.md"), readme).unwrap();
        eprintln!(
            "[minimize] {sig}\n           {} steps → {} ({} verdicts): {}",
            rep.steps_before,
            rep.steps_after,
            rep.verdicts_used,
            min.summary()
        );
    }
}

// ── Promotion (spec §8) ──────────────────────────────────────────────────

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay")
}

/// Promote a document on disk (`PROSPECT_FROM=<stem>` with `.waffle` +
/// `.meta.json`, e.g. a `findings/<slug>/<id>.min` stem) into the corpus as
/// the next `P` case, with `PROSPECT_DESCRIPTION`. The category pin in
/// `assay_kv2.rs` and the results.json refresh are the caller's next steps.
#[test]
#[ignore = "corpus promotion (writes app/tests/cases/assay); run by hand — spec §8"]
fn prospect_promote() {
    let stem = PathBuf::from(std::env::var("PROSPECT_FROM").expect("PROSPECT_FROM=<stem>"));
    let description = std::env::var("PROSPECT_DESCRIPTION").expect("PROSPECT_DESCRIPTION");
    let waffle = std::fs::read_to_string(stem.with_extension("waffle")).expect("waffle");
    let meta: test_harness::assay::gen::AssayMeta = serde_json::from_str(
        &std::fs::read_to_string(stem.with_extension("meta.json")).expect("meta"),
    )
    .expect("meta json");
    let corpus = corpus_dir();
    let id = promote::next_p_id(&corpus).unwrap();
    // Re-judge first: a promotion records a verdict the promoter has seen.
    let outcome = categorize(&id, &waffle, &meta);
    promote::write_case(&corpus, &id, &waffle, &meta, &description).unwrap();
    eprintln!(
        "[promote] {id} ← {} : {} — {}",
        stem.display(),
        outcome.category.label(),
        outcome.detail
    );
}

/// Re-derive `PROSPECT_CANDIDATE=<stem>`'s `.meta.json` FROM its
/// `.waffle` (spec §3.2: a candidate's meta is derived from its document,
/// never copied). Needed whenever a finding's document is edited by hand —
/// a chain PREFIX, say, whose inherited meta still lists the full op list
/// and the full envelope. Writes the stem's `.meta.json` in place.
#[test]
#[ignore = "manual instrument: rewrites <stem>.meta.json from <stem>.waffle — spec §3.2"]
fn prospect_derive_meta() {
    let stem = PathBuf::from(std::env::var("PROSPECT_CANDIDATE").expect("PROSPECT_CANDIDATE"));
    let id = stem
        .file_name()
        .and_then(|s| s.to_str())
        .expect("stem name")
        .to_string();
    let waffle = std::fs::read_to_string(stem.with_extension("waffle")).expect("waffle");
    let doc: serde_json::Value = serde_json::from_str(&waffle).expect("waffle json");
    let meta = derive_meta(&id, &doc).expect("derivable meta");
    std::fs::write(
        stem.with_extension("meta.json"),
        serde_json::to_string_pretty(&meta).expect("meta json"),
    )
    .expect("write meta");
    eprintln!(
        "[derive-meta] {id}: {} op(s), scale {:.6e}, max_bbox_extent {:.6e}",
        meta.operations.len(),
        meta.scale,
        meta.oracles.max_bbox_extent
    );
}

/// Promote the needle star (the first loud generative finding, spec §8) as
/// a `P` case from the acceptance document.
#[test]
#[ignore = "corpus promotion (writes app/tests/cases/assay); run by hand — spec §8"]
fn prospect_promote_needle_star() {
    let waffle = needle_star_document();
    let doc: serde_json::Value = serde_json::from_str(&waffle).unwrap();
    let corpus = corpus_dir();
    let id = promote::next_p_id(&corpus).unwrap();
    let meta = derive_meta(&id, &doc).unwrap();
    let outcome = categorize(&id, &waffle, &meta);
    assert_eq!(outcome.category, Category::Error, "{}", outcome.detail);
    promote::write_case(
        &corpus,
        &id,
        &waffle,
        &meta,
        "3 ops, scale=3.28e1, extrude(polygon,boss)+extrude(polygon,boss)+boolean-union — \
         prospector P0: octagon prism (Y plane) ∪ 4-point needle star r_in=2 r_out=22 (X plane) \
         ⇒ boolean_union TessellationFailed \"planar triangle collapsed at render precision\" \
         (found by the first loud generative_chain run, 2026-09-27; ERROR-class pin, \
         derived_meta: expectations unadjudicated until conversion)",
    )
    .unwrap();
    eprintln!("[promote] {id} ← needle star: {}", outcome.detail);
}

// ── Mutation search (spec §5) ────────────────────────────────────────────

/// Parents for a mutation run: the `.waffle` files of `dir`, restricted to
/// the ids `results.json` (if present) marks SUPPORTED_CORRECT.
fn mutation_parents(dir: &Path) -> Vec<PathBuf> {
    let correct: Option<std::collections::HashSet<String>> =
        std::fs::read_to_string(dir.join("results.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("results")?.as_array().map(|rows| {
                    rows.iter()
                        .filter(|r| r["category"].as_str() == Some("SUPPORTED_CORRECT"))
                        .filter_map(|r| r["id"].as_str().map(str::to_string))
                        .collect()
                })
            });
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("parent dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("waffle"))
        .filter(|p| match &correct {
            Some(ok) => p
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| ok.contains(s))
                .unwrap_or(false),
            None => true,
        })
        .collect();
    files.sort();
    files
}

/// The mutation search: `PROSPECT_MUTATE=<dir>` (default: the corpus),
/// `PROSPECT_SEED`, `PROSPECT_COUNT`, `PROSPECT_JOBS`, `PROSPECT_BUDGET_SECS`;
/// output under `target/prospect/mutate-<seed>` (or `PROSPECT_OUT`). Each
/// mutant is one knob on one CORRECT parent; a non-CORRECT verdict is a
/// finding whose known-good sibling is one edit away.
#[test]
#[ignore = "the prospector mutation search (long, release); run by hand — spec §5"]
fn prospect_mutate() {
    let seed = env_u64("PROSPECT_SEED", 1);
    let count = env_u64("PROSPECT_COUNT", 200);
    let jobs = env_u64("PROSPECT_JOBS", 8).max(1) as usize;
    let budget = Duration::from_secs(env_u64("PROSPECT_BUDGET_SECS", 600));
    let parent_dir = std::env::var("PROSPECT_MUTATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| corpus_dir());
    let out = match std::env::var("PROSPECT_OUT") {
        Ok(p) => PathBuf::from(p),
        Err(_) => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/prospect")
            .join(format!("mutate-{seed}")),
    };
    std::fs::create_dir_all(&out).expect("out dir");
    let parents = mutation_parents(&parent_dir);
    assert!(
        !parents.is_empty(),
        "no parents in {}",
        parent_dir.display()
    );
    let report_path = out.join("report.jsonl");
    let done: std::collections::HashSet<String> = std::fs::read_to_string(&report_path)
        .map(|s| {
            s.lines()
                .filter_map(|l| serde_json::from_str::<ReportLine>(l).ok())
                .map(|r| r.id)
                .collect()
        })
        .unwrap_or_default();
    eprintln!(
        "[mutate] seed={seed} count={count} jobs={jobs} parents={} from {} out={} (resuming {} done)",
        parents.len(),
        parent_dir.display(),
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
    let parents = Arc::new(parents);
    let started = Instant::now();
    let workers: Vec<_> = (0..jobs)
        .map(|_| {
            let next = Arc::clone(&next);
            let done = Arc::clone(&done);
            let report = Arc::clone(&report);
            let parents = Arc::clone(&parents);
            let out = out.clone();
            std::thread::spawn(move || loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                if index >= count {
                    break;
                }
                let id = prospect::mutant_id(seed, index);
                if done.contains(&id) {
                    continue;
                }
                // Parent choice is part of the seeded stream.
                let pick = gen3::Rng::new(seed ^ index.wrapping_mul(0x2545_F491_4F6C_DD1D))
                    .below(parents.len() as u64) as usize;
                let parent = &parents[pick];
                let parent_id = parent
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let (category, detail, v, cpu) = run_child(
                    &[
                        ("PROSPECT_MUTATE_PARENT", parent.display().to_string()),
                        ("PROSPECT_SEED", seed.to_string()),
                        ("PROSPECT_INDEX", index.to_string()),
                        ("PROSPECT_OUT", out.display().to_string()),
                    ],
                    budget,
                );
                let line = ReportLine {
                    id: id.clone(),
                    seed,
                    index,
                    signature: signature_of(&category, &detail, &v),
                    category,
                    detail,
                    summary: format!("{parent_id} + {}", v["knob"].as_str().unwrap_or("?")),
                    steps: 0,
                    scale: 0.0,
                    build_stopped_by: None,
                    cpu_secs: cpu,
                    parent: Some(parent_id),
                    knob: v["knob"].as_str().map(str::to_string),
                };
                eprintln!(
                    "[mutate] {} {:<18} {:>6.1}s  {}  {}",
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
    let lines: Vec<ReportLine> = std::fs::read_to_string(&report_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let mut hist: std::collections::BTreeMap<String, usize> = Default::default();
    let mut findings: std::collections::BTreeMap<String, Vec<&ReportLine>> = Default::default();
    for l in &lines {
        *hist.entry(l.category.clone()).or_default() += 1;
        if !(l.category == "SUPPORTED_CORRECT" || l.category.starts_with("UNSUPPORTED")) {
            findings.entry(l.signature.clone()).or_default().push(l);
        }
    }
    eprintln!(
        "[mutate] done: {} mutants in {:.0}s",
        lines.len(),
        started.elapsed().as_secs_f64()
    );
    for (k, v) in &hist {
        eprintln!("[mutate]   {k:<24} {v}");
    }
    eprintln!("[mutate] {} finding signature(s):", findings.len());
    for (sig, ls) in &findings {
        eprintln!(
            "[mutate]   ×{:<3} {}  e.g. {}: {}",
            ls.len(),
            sig,
            ls[0].id,
            ls[0].summary
        );
    }
}

// ── Metamorphic identities (spec §6) ─────────────────────────────────────

/// Measure a document written to `<stem>.waffle` through the child.
fn measure_via_child(
    doc: &serde_json::Value,
    stem: &Path,
    budget: Duration,
) -> Option<metamorphic::Measurement> {
    std::fs::write(
        stem.with_extension("waffle"),
        serde_json::to_string(doc).ok()?,
    )
    .ok()?;
    let (category, detail, v, _) = run_child(
        &[("PROSPECT_CANDIDATE", stem.display().to_string())],
        budget,
    );
    match serde_json::from_value::<metamorphic::Measurement>(v["measurement"].clone()) {
        Ok(m) => Some(m),
        Err(_) => Some(metamorphic::Measurement {
            category,
            detail,
            volume: None,
            chi: None,
            bodies: 0,
        }),
    }
}

/// The metamorphic search: for every parent (`PROSPECT_MUTATE=<dir>`,
/// default the corpus; CORRECT ids only when `results.json` is present)
/// build the explicit-axis reference, a seeded rigid motion of it, and a
/// uniform scaling; measure all four through the child and compare.
/// `PROSPECT_SEED`, `PROSPECT_COUNT` (parents to take, default all),
/// `PROSPECT_JOBS`, `PROSPECT_BUDGET_SECS`; output under
/// `target/prospect/metamorphic-<seed>`.
#[test]
#[ignore = "the prospector metamorphic search (long, release); run by hand — spec §6"]
fn prospect_metamorphic() {
    let seed = env_u64("PROSPECT_SEED", 1);
    let count = env_u64("PROSPECT_COUNT", u64::MAX);
    let jobs = env_u64("PROSPECT_JOBS", 8).max(1) as usize;
    let budget = Duration::from_secs(env_u64("PROSPECT_BUDGET_SECS", 600));
    let parent_dir = std::env::var("PROSPECT_MUTATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| corpus_dir());
    let out = match std::env::var("PROSPECT_OUT") {
        Ok(p) => PathBuf::from(p),
        Err(_) => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/prospect")
            .join(format!("metamorphic-{seed}")),
    };
    std::fs::create_dir_all(out.join("candidates")).expect("out dir");
    let mut parents = mutation_parents(&parent_dir);
    parents.truncate(count.min(parents.len() as u64) as usize);
    assert!(
        !parents.is_empty(),
        "no parents in {}",
        parent_dir.display()
    );
    let report_path = out.join("report.jsonl");
    let done: std::collections::HashSet<String> = std::fs::read_to_string(&report_path)
        .map(|s| {
            s.lines()
                .filter_map(|l| serde_json::from_str::<ReportLine>(l).ok())
                .map(|r| r.id)
                .collect()
        })
        .unwrap_or_default();
    eprintln!(
        "[metamorphic] seed={seed} parents={} jobs={jobs} out={} (resuming {} done)",
        parents.len(),
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
    let parents = Arc::new(parents);
    let started = Instant::now();
    // Relative band on volumes: the tessellation tolerance is the exact
    // oracle's for the document's scale (`oracle_tol`), so two tessellations
    // of the same geometry differ by chord error only; 1e-3 is generous.
    const REL_BAND: f64 = 1e-3;
    let workers: Vec<_> = (0..jobs)
        .map(|_| {
            let next = Arc::clone(&next);
            let done = Arc::clone(&done);
            let report = Arc::clone(&report);
            let parents = Arc::clone(&parents);
            let out = out.clone();
            std::thread::spawn(move || loop {
                let index = next.fetch_add(1, Ordering::SeqCst) as usize;
                if index >= parents.len() {
                    break;
                }
                let parent = &parents[index];
                let parent_id = parent
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let id = format!("T{:08x}-{parent_id}", seed & 0xFFFF_FFFF);
                if done.contains(&id) {
                    continue;
                }
                let doc: serde_json::Value = match std::fs::read_to_string(parent)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                {
                    Some(d) => d,
                    None => continue,
                };
                let scale = test_harness::assay::prospect::mutate::characteristic_length(&doc);
                let cdir = out.join("candidates");
                let original = measure_via_child(&doc, &cdir.join(format!("{id}-orig")), budget);
                let reference_doc = metamorphic::with_explicit_x_axes(&doc);
                let mut problems: Vec<String> = Vec::new();
                let mut cpu = 0.0;
                if let (Some(orig), Some(ref_doc)) = (original.as_ref(), reference_doc.as_ref()) {
                    let reference =
                        measure_via_child(ref_doc, &cdir.join(format!("{id}-ref")), budget);
                    if let Some(reference) = reference.as_ref() {
                        if let Some(p) =
                            metamorphic::compare("axes", orig, reference, 1.0, REL_BAND)
                        {
                            problems.push(p);
                        }
                        let mut rng = gen3::Rng::new(
                            seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
                        );
                        let motion = metamorphic::RigidMotion::draw(&mut rng, scale);
                        if let Some(rot) = metamorphic::rigid_motion(ref_doc, &motion) {
                            if let Some(moved) =
                                measure_via_child(&rot, &cdir.join(format!("{id}-rigid")), budget)
                            {
                                if let Some(p) =
                                    metamorphic::compare("rigid", reference, &moved, 1.0, REL_BAND)
                                {
                                    problems.push(p);
                                }
                            }
                        }
                        let factor = if rng.chance(0.5) { 1e-3 } else { 1e3 };
                        if let Some(sc) =
                            test_harness::assay::prospect::mutate::scale_document(ref_doc, factor)
                        {
                            if let Some(scaled) =
                                measure_via_child(&sc, &cdir.join(format!("{id}-scale")), budget)
                            {
                                if let Some(p) = metamorphic::compare(
                                    &format!("scale×{factor:.0e}"),
                                    reference,
                                    &scaled,
                                    factor * factor * factor,
                                    REL_BAND,
                                ) {
                                    problems.push(p);
                                }
                            }
                        }
                    }
                    cpu = 0.0;
                }
                let (category, signature) = if problems.is_empty() {
                    ("SUPPORTED_CORRECT".to_string(), "correct".to_string())
                } else if problems.iter().all(|p| p.starts_with("oracle[")) {
                    (
                        "ORACLE".to_string(),
                        problems
                            .iter()
                            .map(|p| p.split(':').next().unwrap_or("oracle").to_string())
                            .collect::<Vec<_>>()
                            .join("+"),
                    )
                } else {
                    (
                        "METAMORPHIC".to_string(),
                        problems
                            .iter()
                            .map(|p| p.split(':').next().unwrap_or("metamorphic").to_string())
                            .collect::<Vec<_>>()
                            .join("+"),
                    )
                };
                let line = ReportLine {
                    id: id.clone(),
                    seed,
                    index: index as u64,
                    category,
                    signature,
                    detail: problems.join("; "),
                    summary: format!(
                        "{parent_id}: {}",
                        original
                            .as_ref()
                            .map(|m| m.category.clone())
                            .unwrap_or_else(|| "?".into())
                    ),
                    steps: 0,
                    scale,
                    build_stopped_by: None,
                    cpu_secs: cpu,
                    parent: Some(parent_id),
                    knob: None,
                };
                eprintln!(
                    "[metamorphic] {} {:<18} {}  {}",
                    line.id, line.category, line.summary, line.detail
                );
                let mut f = report.lock().unwrap();
                let _ = writeln!(f, "{}", serde_json::to_string(&line).unwrap());
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
    let lines: Vec<ReportLine> = std::fs::read_to_string(&report_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let bad: Vec<&ReportLine> = lines
        .iter()
        .filter(|l| l.category == "METAMORPHIC" || l.category == "ORACLE")
        .collect();
    eprintln!(
        "[metamorphic] done: {} parents in {:.0}s; {} disagreement(s) ({} kernel, {} oracle-only)",
        lines.len(),
        started.elapsed().as_secs_f64(),
        bad.len(),
        bad.iter().filter(|l| l.category == "METAMORPHIC").count(),
        bad.iter().filter(|l| l.category == "ORACLE").count()
    );
    for l in &bad {
        eprintln!("[metamorphic]   {} {}: {}", l.id, l.signature, l.detail);
    }
}

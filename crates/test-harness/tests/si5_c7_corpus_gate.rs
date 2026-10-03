//! SI5 checkpoint **C7** — the corpus gate
//! (`specs/step_import_si5_exact_analytic_ingestion.md` §7;
//! `specs/boolean_hardening_external_corpus.md` §1.1's pin pattern).
//!
//! The censuses in `si5_analytic.rs` MEASURE the exact tier's reach over the
//! ABC corpus and print a table nobody reads twice. This binary PINS it: a
//! fixed, stride-sampled subset of ABC chunk 0000 is ingested through the
//! SI5 tier (`step_import::parse_step_analytic` → `kernel_v2::ingest_analytic`),
//! and every model's verdict — per shell: exact with its volume, surface
//! area and validated topology, or refused with the refusal's CLASS — is
//! compared against `corpora/abc_0000_si5_gate.json`, committed. Any
//! difference is a red test:
//!
//! - an exact shell that now refuses, or whose volume / area / topology
//!   moved: a **REGRESSION** in ingestion — the thing C7 exists to catch;
//! - a refused shell that now ingests: **PROGRESS**, still red, because the
//!   pin is the record — regenerate it (`UPDATE_SI5_GATE_PIN=1`) and commit
//!   the conversion with the change that earned it, the way the assay smoke
//!   pins move with a conversion;
//! - a refusal whose class changed: the wall MOVED — also recorded, same way;
//! - a model whose bytes differ from the pin: the LOCAL corpus is not the
//!   one pinned (a corrupt or re-extracted chunk), never a kernel verdict.
//!
//! The corpus itself is license-restricted and never committed: the pin
//! names each model by its ABC id, byte count and FNV-1a hash so a finding
//! stays reproducible, and the gate SKIPS with a note when no chunk is on
//! disk (CI). It runs wherever `scripts/fetch-abc-corpus.sh` has run.
//!
//! Every import runs in a KILLABLE child with a CPU budget: truck's reader
//! is not resource-bounded (41 GB RSS on one model, hardening spec §10) and
//! panics rather than erroring on some real input, so an in-process loop is
//! one bad model away from a dead run. The child/driver split and CPU-time
//! budget are the assay runner's (`assay_kv2::replay_case_subprocess`).
//!
//!     # the gate (release — corpus work is release-only policy)
//!     cargo test -p test-harness --test si5_c7_corpus_gate --release -- --nocapture
//!     # move the pin after a conversion (prints the diff it records)
//!     UPDATE_SI5_GATE_PIN=1 cargo test -p test-harness --test si5_c7_corpus_gate --release
//!
//! Knobs: `ABC_DIR` (else `$WAFFLE_CORPUS_DIR/abc/step/0000`, else
//! `~/.cache/waffle-iron/corpus/abc/step/0000`), `SI5_GATE_JOBS`,
//! `SI5_GATE_CASE_CPU_SECS` (default 120), `SI5_GATE_ALLOW_DEBUG=1`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use test_harness::assay::prospect::process_cpu_secs;

const OUTCOME_PREFIX: &str = "SI5_GATE_OUTCOME\t";
const PIN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/corpora/abc_0000_si5_gate.json"
);
const CHUNK: &str = "0000";
/// The sample: this many models, stride-spaced over the chunk's eligible
/// files (sorted by path), so the Onshape-document clustering of adjacent
/// ids does not make the sample forty copies of ten designs.
const SAMPLE_N: usize = 400;
/// Real STEP has a 540 MB tail; the SI5 censuses cap at 2 MB and so does the
/// pin, so their reach figures and this gate describe the same population.
const MAX_BYTES: u64 = 2_000_000;
/// Relative band on a pinned exact volume / area. The integrals are closed
/// forms over an exact-π coefficient and f64 sums; a change beyond this is a
/// geometry change, not a rounding one.
const SCALAR_REL_BAND: f64 = 1e-9;

// =========================================================================
// The record — what the pin stores per model
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Pin {
    chunk: String,
    sample_n: usize,
    max_bytes: u64,
    /// ISO date the pin was (re)generated — provenance for the reader, never
    /// compared.
    generated: String,
    models: Vec<ModelRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct ModelRecord {
    /// The ABC file stem, e.g. `00000000_290a9120f9f249a7a05cfe9c_step_000`.
    id: String,
    bytes: u64,
    /// FNV-1a 64 of the file bytes, hex. Guards the LOCAL copy, not the
    /// corpus: the id is the authoritative name.
    fnv1a64: String,
    /// `ok` (shells below carry the verdict), `parse error: …`, `panic`,
    /// `timeout: …` or `driver: …`.
    outcome: String,
    shells: Vec<ShellRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct ShellRecord {
    /// `exact` or `mesh`.
    tier: String,
    /// The refusal's class (digits collapsed to `#`) for a `mesh` shell; for
    /// an `exact` shell, a note when a post-ingest measurement was refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    volume_m3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    area_m2: Option<f64>,
    /// `[V, E, F, rings, shells, genus]` from `validate_solid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    topology: Option<[usize; 6]>,
}

// =========================================================================
// Pure pieces (unit-tested below, no corpus needed)
// =========================================================================

/// FNV-1a, 64-bit. Not cryptographic — a fingerprint of the local bytes.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Collapse every digit so refusal strings group by CLASS, not by index.
fn collapse_digits(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

/// `n` indices stride-spaced over `len` (all of them when `len <= n`),
/// deterministic and monotone.
fn stride_sample(len: usize, n: usize) -> Vec<usize> {
    if len <= n {
        return (0..len).collect();
    }
    (0..n).map(|i| i * len / n).collect()
}

/// Every `.step` under `dir` at or under `max_bytes`, sorted by path.
fn eligible_files(dir: &Path, max_bytes: u64) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "step")
                && std::fs::metadata(&p).is_ok_and(|m| m.len() <= max_bytes)
            {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

fn model_id(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// The local file is not the pinned one — no verdict was compared.
    Corrupt,
    /// An exact shell refused / measured differently; a model that stopped
    /// parsing; a new timeout. The gate's reason to exist.
    Regression,
    /// A refused shell now ingests, or a model that failed now parses.
    Progress,
    /// Still refused, by a different wall.
    ClassMoved,
}

#[derive(Debug, Clone, PartialEq)]
struct Finding {
    id: String,
    kind: Kind,
    detail: String,
}

fn rel_moved(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(f64::MIN_POSITIVE);
    (a - b).abs() / scale > SCALAR_REL_BAND
}

/// Compare one model's current record against its pin. Empty = identical
/// to the pin's resolution.
fn diff_model(pinned: &ModelRecord, now: &ModelRecord) -> Vec<Finding> {
    let mut out = Vec::new();
    let f = |kind: Kind, detail: String| Finding {
        id: now.id.clone(),
        kind,
        detail,
    };
    if pinned.bytes != now.bytes || pinned.fnv1a64 != now.fnv1a64 {
        out.push(f(
            Kind::Corrupt,
            format!(
                "local file is not the pinned one ({} bytes, fnv {}) — pinned {} bytes, fnv {}",
                now.bytes, now.fnv1a64, pinned.bytes, pinned.fnv1a64
            ),
        ));
        return out;
    }
    if pinned.outcome != now.outcome {
        let kind = if pinned.outcome == "ok" {
            Kind::Regression
        } else if now.outcome == "ok" {
            Kind::Progress
        } else {
            Kind::ClassMoved
        };
        out.push(f(
            kind,
            format!("outcome `{}` → `{}`", pinned.outcome, now.outcome),
        ));
        return out;
    }
    if pinned.shells.len() != now.shells.len() {
        out.push(f(
            Kind::Regression,
            format!(
                "shell count {} → {} (the canonical shell list changed)",
                pinned.shells.len(),
                now.shells.len()
            ),
        ));
        return out;
    }
    for (i, (p, n)) in pinned.shells.iter().zip(&now.shells).enumerate() {
        match (p.tier.as_str(), n.tier.as_str()) {
            ("exact", "mesh") => out.push(f(
                Kind::Regression,
                format!(
                    "shell {i}: was exact, now refused — {}",
                    n.why.as_deref().unwrap_or("?")
                ),
            )),
            ("mesh", "exact") => out.push(f(
                Kind::Progress,
                format!(
                    "shell {i}: was refused ({}), now exact",
                    p.why.as_deref().unwrap_or("?")
                ),
            )),
            ("mesh", "mesh") => {
                if p.why != n.why {
                    out.push(f(
                        Kind::ClassMoved,
                        format!(
                            "shell {i}: refusal `{}` → `{}`",
                            p.why.as_deref().unwrap_or("?"),
                            n.why.as_deref().unwrap_or("?")
                        ),
                    ));
                }
            }
            ("exact", "exact") => {
                let before = out.len();
                // A measurement the kernel refused on the pinned run (its
                // `why` names the closed form it lacks) and now delivers is
                // PROGRESS — the ratchet — while one it delivered and now
                // refuses is a regression, like a moved value.
                let scalar = |name: &str, a: Option<f64>, b: Option<f64>| match (a, b) {
                    (Some(a), Some(b)) if rel_moved(a, b) => {
                        Some((Kind::Regression, format!("shell {i}: {name} {a:e} → {b:e}")))
                    }
                    (Some(_), None) => Some((
                        Kind::Regression,
                        format!(
                            "shell {i}: {name} no longer measured — {}",
                            n.why.as_deref().unwrap_or("?")
                        ),
                    )),
                    (None, Some(b)) => Some((
                        Kind::Progress,
                        format!(
                            "shell {i}: {name} now measured ({b:e}); was refused — {}",
                            p.why.as_deref().unwrap_or("?")
                        ),
                    )),
                    _ => None,
                };
                for (kind, detail) in [
                    scalar("volume", p.volume_m3, n.volume_m3),
                    scalar("area", p.area_m2, n.area_m2),
                ]
                .into_iter()
                .flatten()
                {
                    out.push(f(kind, detail));
                }
                match (p.topology, n.topology) {
                    (Some(a), Some(b)) if a != b => out.push(f(
                        Kind::Regression,
                        format!("shell {i}: topology [V,E,F,R,S,G] {a:?} → {b:?}"),
                    )),
                    (Some(_), None) => out.push(f(
                        Kind::Regression,
                        format!(
                            "shell {i}: no longer validates — {}",
                            n.why.as_deref().unwrap_or("?")
                        ),
                    )),
                    (None, Some(b)) => out.push(f(
                        Kind::Progress,
                        format!("shell {i}: now validates ({b:?})"),
                    )),
                    _ => {}
                }
                // Both measured, both validated, same values: a changed `why`
                // can only be a different refusal text on the same missing
                // measurement — a class move, recorded.
                if out.len() == before && p.why != n.why {
                    out.push(f(
                        Kind::ClassMoved,
                        format!(
                            "shell {i}: measurement refusal `{}` → `{}`",
                            p.why.as_deref().unwrap_or("-"),
                            n.why.as_deref().unwrap_or("-")
                        ),
                    ));
                }
            }
            (a, b) => out.push(f(
                Kind::Regression,
                format!("shell {i}: unknown tiers `{a}` → `{b}`"),
            )),
        }
    }
    out
}

// =========================================================================
// The child — one model, in process, typed verdict
// =========================================================================

fn extractor_class(e: &step_import::Ineligible) -> String {
    use step_import::Ineligible as I;
    let s = match e {
        I::Surface { entity, .. } => {
            format!("extractor: surface {entity} has no exact representation")
        }
        I::Curve { entity, .. } => format!("extractor: curve {entity} has no exact representation"),
        I::OutOfRange { entity, reason, .. } => {
            format!("extractor: {entity} out of range — {reason}")
        }
        I::Topology { reason, .. } => format!("extractor: topology — {reason}"),
        I::BadIndex { table, .. } => format!("extractor: {table} index out of range"),
        I::SilentlyDroppedTopology { entity } => {
            format!("extractor: source declares {entity}, which the reader silently drops")
        }
        I::Empty => "extractor: shell has no faces".to_string(),
        I::Voids { .. } => "extractor: one boundary of a solid with voids".to_string(),
    };
    collapse_digits(&s)
}

fn kernel_class(e: &kernel_v2::KernelV2Error) -> String {
    use kernel_v2::KernelV2Error as E;
    let s = match e {
        E::AnalyticIngestUnsupportedSurface { surface, .. } => {
            format!("kernel: unsupported surface {surface}")
        }
        E::AnalyticIngestUnsupportedCurve { curve, .. } => {
            format!("kernel: unsupported curve {curve}")
        }
        E::AnalyticIngestUnsupported(r) => format!("kernel: unsupported — {r}"),
        E::InvalidAnalyticShell(r) => format!("kernel: invalid shell — {r}"),
        E::AnalyticVertexOffSurface { .. } => "kernel: vertex off surface".to_string(),
        // Named, not bucketed: the curved orientation/consistency walls carry
        // the violated condition in their payload, and a C7 corpus finding
        // lands on one (the apex-cone patch of `00005451_…_step_005`, whose
        // verdict is "cone patch vertex lies on the axis"). A `{other:?}`
        // line would have grouped it with every unrelated validation error.
        E::CurvedGeometryMismatch { reason, .. } => format!("kernel: curved geometry — {reason}"),
        other => format!("kernel: validation — {other:?}"),
    };
    collapse_digits(&s)
}

/// Ingest one file through the SI5 tier and record every shell's verdict.
/// Panics inside truck are caught here (the process boundary catches what
/// this cannot — an abort, a runaway).
fn judge_file(path: &Path) -> ModelRecord {
    let id = model_id(path);
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut rec = ModelRecord {
        id: id.clone(),
        bytes: bytes.len() as u64,
        fnv1a64: format!("{:016x}", fnv1a64(&bytes)),
        outcome: "ok".to_string(),
        shells: Vec::new(),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        rec.outcome = "parse error: not UTF-8".to_string();
        return rec;
    };

    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        step_import::parse_step_analytic(&text, &id)
    }));
    let import = match parsed {
        Err(_) => {
            rec.outcome = "panic".to_string();
            return rec;
        }
        Ok(Err(e)) => {
            rec.outcome = format!("parse error: {}", collapse_digits(&e.to_string()));
            return rec;
        }
        Ok(Ok(i)) => i,
    };

    let mut arena = kernel_v2::BrepArena::new();
    for shell in &import.shells {
        let shell = match shell {
            Err(e) => {
                rec.shells.push(ShellRecord {
                    tier: "mesh".into(),
                    why: Some(extractor_class(e)),
                    volume_m3: None,
                    area_m2: None,
                    topology: None,
                });
                continue;
            }
            Ok(s) => s,
        };
        let ingested = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            kernel_v2::ingest_analytic(&mut arena, shell)
        }));
        let sid = match ingested {
            Err(_) => {
                rec.shells.push(ShellRecord {
                    tier: "mesh".into(),
                    why: Some("kernel: PANIC".into()),
                    volume_m3: None,
                    area_m2: None,
                    topology: None,
                });
                continue;
            }
            Ok(Err(e)) => {
                rec.shells.push(ShellRecord {
                    tier: "mesh".into(),
                    why: Some(kernel_class(&e)),
                    volume_m3: None,
                    area_m2: None,
                    topology: None,
                });
                continue;
            }
            Ok(Ok(sid)) => sid,
        };
        let mut why = Vec::new();
        let volume_m3 = match kernel_v2::geom::signed_volume(&arena, sid) {
            Ok(v) => Some(v),
            Err(e) => {
                why.push(format!("volume: {}", collapse_digits(&format!("{e:?}"))));
                None
            }
        };
        let area_m2 = match kernel_v2::surface_area(&arena, sid) {
            Ok(a) => Some(a),
            Err(e) => {
                why.push(format!("area: {}", collapse_digits(&format!("{e:?}"))));
                None
            }
        };
        let topology = match kernel_v2::validate_solid(&arena, sid) {
            Ok(t) => Some([t.vertices, t.edges, t.faces, t.rings, t.shells, t.genus]),
            Err(e) => {
                why.push(format!("validate: {}", collapse_digits(&format!("{e:?}"))));
                None
            }
        };
        rec.shells.push(ShellRecord {
            tier: "exact".into(),
            why: (!why.is_empty()).then(|| why.join("; ")),
            volume_m3,
            area_m2,
            topology,
        });
    }
    rec
}

/// Verdict CHILD: `SI5_GATE_CASE=<path>` ⇒ judge that file and print exactly
/// one `SI5_GATE_OUTCOME\t{json}` line. Driven by the gate; manual form:
///
///     SI5_GATE_CASE=/path/to/model.step cargo test -p test-harness \
///       --test si5_c7_corpus_gate --release -- --ignored --nocapture --exact si5_gate_child
#[test]
#[ignore = "child: driven by the gate via SI5_GATE_CASE"]
fn si5_gate_child() {
    let Ok(case) = std::env::var("SI5_GATE_CASE") else {
        eprintln!("si5_gate_child: SI5_GATE_CASE unset — nothing to do");
        return;
    };
    // truck's panic message is noise on the driver's nulled stderr; keep the
    // manual form readable.
    std::panic::set_hook(Box::new(|_| {}));
    let rec = judge_file(Path::new(&case));
    println!(
        "{OUTCOME_PREFIX}{}",
        serde_json::to_string(&rec).expect("record serializes")
    );
}

// =========================================================================
// The driver — budgeted children, a pool, the pin
// =========================================================================

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Judge one file in a killable child with a CPU-time budget (wall cap as
/// the stall safety net). The child's record, or a `timeout: …` /
/// `driver: …` outcome carrying the local fingerprint so the pin still
/// identifies the file.
fn judge_in_child(path: &Path, cpu_budget: Duration) -> ModelRecord {
    use std::io::Read as _;
    use std::process::{Command, Stdio};

    let fallback = |outcome: String| {
        let bytes = std::fs::read(path).unwrap_or_default();
        ModelRecord {
            id: model_id(path),
            bytes: bytes.len() as u64,
            fnv1a64: format!("{:016x}", fnv1a64(&bytes)),
            outcome,
            shells: Vec::new(),
        }
    };
    let cpu = cpu_budget.as_secs_f64();
    let wall_cap = Duration::from_secs_f64((cpu * 4.0).max(cpu + 120.0));
    let exe = std::env::current_exe().expect("current_exe of the test binary");
    let mut child = match Command::new(&exe)
        .args(["--exact", "si5_gate_child", "--ignored", "--nocapture"])
        .env("SI5_GATE_CASE", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fallback(format!("driver: cannot spawn child: {e}")),
    };
    let mut stdout = child.stdout.take().expect("piped child stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let pid = child.id();
    let cpu_probe = process_cpu_secs(pid).is_some();
    let wall_deadline = Instant::now() + if cpu_probe { wall_cap } else { cpu_budget };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                let over_cpu = cpu_probe && process_cpu_secs(pid).is_some_and(|c| c >= cpu);
                let over_wall = Instant::now() >= wall_deadline;
                if over_cpu || over_wall {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(if over_cpu {
                        format!("timeout: {cpu:.0}s CPU (child killed)")
                    } else {
                        format!(
                            "timeout: wall cap {}s, child stalled below its CPU budget (killed)",
                            wall_deadline.duration_since(Instant::now()).as_secs()
                        )
                    });
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("driver: wait failed: {e}"));
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    match status {
        Err(detail) => fallback(detail),
        Ok(status) => {
            let line = out.lines().find_map(|l| l.strip_prefix(OUTCOME_PREFIX));
            match line.map(serde_json::from_str::<ModelRecord>) {
                Some(Ok(rec)) => rec,
                Some(Err(e)) => fallback(format!("driver: unreadable child record: {e}")),
                None => fallback(format!(
                    "driver: child exited {} without a verdict (abort / OOM kill)",
                    status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "by signal".to_string())
                )),
            }
        }
    }
}

/// Run the whole sample through `jobs` concurrent children; records in
/// sample order.
fn judge_sample(files: &[PathBuf], jobs: usize, cpu_budget: Duration) -> Vec<ModelRecord> {
    let queue: Mutex<VecDeque<(usize, &PathBuf)>> = Mutex::new(files.iter().enumerate().collect());
    let done: Mutex<Vec<Option<ModelRecord>>> = Mutex::new(vec![None; files.len()]);
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| loop {
                let next = queue.lock().expect("queue").pop_front();
                let Some((i, path)) = next else { break };
                let rec = judge_in_child(path, cpu_budget);
                done.lock().expect("done")[i] = Some(rec);
            });
        }
    });
    done.into_inner()
        .expect("done")
        .into_iter()
        .map(|r| r.expect("every file judged"))
        .collect()
}

fn corpus_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("ABC_DIR") {
        return Some(PathBuf::from(d));
    }
    let root = std::env::var("WAFFLE_CORPUS_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache/waffle-iron/corpus"))
        })
        .ok()?;
    let dir = root.join("abc/step").join(CHUNK);
    dir.is_dir().then_some(dir)
}

fn summarize(models: &[ModelRecord]) -> String {
    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    let (mut exact, mut mixed, mut mesh) = (0usize, 0usize, 0usize);
    let (mut exact_shells, mut mesh_shells) = (0usize, 0usize);
    let mut walls: BTreeMap<String, usize> = BTreeMap::new();
    for m in models {
        *outcomes
            .entry(collapse_digits(
                m.outcome
                    .split_once(':')
                    .map(|(k, _)| k)
                    .unwrap_or(&m.outcome),
            ))
            .or_default() += 1;
        if m.outcome != "ok" {
            continue;
        }
        let e = m.shells.iter().filter(|s| s.tier == "exact").count();
        exact_shells += e;
        mesh_shells += m.shells.len() - e;
        for s in &m.shells {
            if let (true, Some(w)) = (s.tier == "mesh", &s.why) {
                *walls.entry(w.clone()).or_default() += 1;
            }
        }
        if e == m.shells.len() && e > 0 {
            exact += 1;
        } else if e == 0 {
            mesh += 1;
        } else {
            mixed += 1;
        }
    }
    let mut s = format!(
        "SI5 C7 gate over {} models: {exact} fully exact ({:.1} %), {mixed} mixed, {mesh} all \
         mesh-tier; {exact_shells} exact shells, {mesh_shells} refused\n  outcomes: {}\n  walls:",
        models.len(),
        100.0 * exact as f64 / models.len().max(1) as f64,
        outcomes
            .iter()
            .map(|(k, v)| format!("{k} ×{v}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let mut rows: Vec<_> = walls.iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (w, c) in rows {
        s.push_str(&format!("\n    x{c:<5} {w}"));
    }
    s
}

/// **The gate.** Skips (with a note) when no corpus is on disk or in a debug
/// build; otherwise every sampled model's verdict must equal its pin.
#[test]
fn abc_sample_ingests_exactly_as_pinned() {
    if cfg!(debug_assertions) && std::env::var("SI5_GATE_ALLOW_DEBUG").is_err() {
        eprintln!(
            "si5 corpus gate: debug build — corpus work is release-only policy \
             (docs/TESTING.md); run with --release or SI5_GATE_ALLOW_DEBUG=1"
        );
        return;
    }
    let Some(dir) = corpus_dir() else {
        eprintln!(
            "si5 corpus gate: no ABC chunk {CHUNK} on disk (ABC_DIR / WAFFLE_CORPUS_DIR / \
             ~/.cache/waffle-iron/corpus) — skipping; scripts/fetch-abc-corpus.sh fetches it"
        );
        return;
    };
    let update = std::env::var("UPDATE_SI5_GATE_PIN").is_ok();
    let jobs: usize = env_or(
        "SI5_GATE_JOBS",
        std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4),
    );
    let cpu_budget = Duration::from_secs(env_or("SI5_GATE_CASE_CPU_SECS", 120));

    let eligible = eligible_files(&dir, MAX_BYTES);
    assert!(
        !eligible.is_empty(),
        "no .step files at or under {MAX_BYTES} bytes under {}",
        dir.display()
    );
    let files: Vec<PathBuf> = stride_sample(eligible.len(), SAMPLE_N)
        .into_iter()
        .map(|i| eligible[i].clone())
        .collect();

    let started = Instant::now();
    let models = judge_sample(&files, jobs, cpu_budget);
    let wall = started.elapsed();
    eprintln!(
        "\n{}\n  {} eligible files in the chunk, {jobs} jobs, {:.0} s wall",
        summarize(&models),
        eligible.len(),
        wall.as_secs_f64()
    );

    let now = Pin {
        chunk: CHUNK.to_string(),
        sample_n: SAMPLE_N,
        max_bytes: MAX_BYTES,
        generated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        models,
    };

    let pinned: Option<Pin> = std::fs::read_to_string(PIN_PATH)
        .ok()
        .map(|t| serde_json::from_str(&t).unwrap_or_else(|e| panic!("{PIN_PATH}: {e}")));

    let mut findings: Vec<Finding> = Vec::new();
    if let Some(pinned) = &pinned {
        let by_id: BTreeMap<&str, &ModelRecord> =
            pinned.models.iter().map(|m| (m.id.as_str(), m)).collect();
        let now_ids: std::collections::BTreeSet<&str> =
            now.models.iter().map(|m| m.id.as_str()).collect();
        for m in &now.models {
            match by_id.get(m.id.as_str()) {
                Some(p) => findings.extend(diff_model(p, m)),
                None => findings.push(Finding {
                    id: m.id.clone(),
                    kind: Kind::Corrupt,
                    detail: "sampled from the local chunk but absent from the pin — the chunk \
                             on disk is not the one pinned"
                        .into(),
                }),
            }
        }
        for p in &pinned.models {
            if !now_ids.contains(p.id.as_str()) {
                findings.push(Finding {
                    id: p.id.clone(),
                    kind: Kind::Corrupt,
                    detail: "pinned but not sampled from the local chunk".into(),
                });
            }
        }
    }

    if update || pinned.is_none() {
        if !update {
            panic!(
                "no pin at {PIN_PATH}; generate it deliberately with UPDATE_SI5_GATE_PIN=1 and \
                 commit it"
            );
        }
        if let Some(dir) = Path::new(PIN_PATH).parent() {
            std::fs::create_dir_all(dir).expect("corpora dir");
        }
        let mut json = serde_json::to_string_pretty(&now).expect("pin serializes");
        json.push('\n');
        std::fs::write(PIN_PATH, json).unwrap_or_else(|e| panic!("{PIN_PATH}: {e}"));
        eprintln!(
            "\n  pin written: {PIN_PATH} ({} models){}",
            now.models.len(),
            if findings.is_empty() {
                String::new()
            } else {
                format!("\n  it records {} change(s):", findings.len())
            }
        );
        for f in &findings {
            eprintln!("    {:?} {}: {}", f.kind, f.id, f.detail);
        }
        return;
    }

    if findings.is_empty() {
        return;
    }
    findings.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
    let mut report = String::new();
    for f in &findings {
        report.push_str(&format!(
            "\n  {:<10} {}: {}",
            format!("{:?}", f.kind),
            f.id,
            f.detail
        ));
    }
    let n = |k: Kind| findings.iter().filter(|f| f.kind == k).count();
    panic!(
        "SI5 corpus gate: {} finding(s) against {PIN_PATH} — {} regression, {} progress, {} \
         class moved, {} corrupt.{report}\n\n  A REGRESSION is an exact ingestion that no \
         longer holds: fix it (the pin is right). PROGRESS / CLASS MOVED are the ratchet: \
         regenerate with UPDATE_SI5_GATE_PIN=1 and commit the pin with the change that moved \
         it. CORRUPT means the local chunk is not the pinned one — re-extract it, do not \
         touch the pin.",
        findings.len(),
        n(Kind::Regression),
        n(Kind::Progress),
        n(Kind::ClassMoved),
        n(Kind::Corrupt),
    );
}

// =========================================================================
// Always-on unit tests of the pure pieces
// =========================================================================

fn exact(v: f64) -> ShellRecord {
    ShellRecord {
        tier: "exact".into(),
        why: None,
        volume_m3: Some(v),
        area_m2: Some(6.0),
        topology: Some([8, 12, 6, 0, 1, 0]),
    }
}

fn refused(why: &str) -> ShellRecord {
    ShellRecord {
        tier: "mesh".into(),
        why: Some(why.into()),
        volume_m3: None,
        area_m2: None,
        topology: None,
    }
}

fn model(shells: Vec<ShellRecord>) -> ModelRecord {
    ModelRecord {
        id: "m".into(),
        bytes: 3,
        fnv1a64: "abc".into(),
        outcome: "ok".into(),
        shells,
    }
}

#[test]
fn fnv1a64_matches_the_reference_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
}

#[test]
fn stride_sample_is_deterministic_monotone_and_spans_the_range() {
    assert_eq!(stride_sample(5, 10), vec![0, 1, 2, 3, 4]);
    let s = stride_sample(9164, 400);
    assert_eq!(s.len(), 400);
    assert_eq!(s[0], 0);
    assert!(s.windows(2).all(|w| w[0] < w[1]), "strictly increasing");
    // The last index is `399 * 9164 / 400` = 9141: within one stride of the end.
    assert!(
        s[399] >= 9164 - 2 * (9164 / 400),
        "reaches the end of the chunk"
    );
    assert_eq!(s, stride_sample(9164, 400));
}

#[test]
fn collapse_digits_groups_by_class() {
    assert_eq!(
        collapse_digits("face 12: B_SPLINE_SURFACE has no exact representation"),
        "face ##: B_SPLINE_SURFACE has no exact representation"
    );
}

#[test]
fn an_identical_record_has_no_findings() {
    let a = model(vec![exact(1.0), refused("extractor: surface x")]);
    assert!(diff_model(&a, &a).is_empty());
    // Within the band is identical.
    let b = model(vec![exact(1.0 + 1e-12), refused("extractor: surface x")]);
    assert!(diff_model(&a, &b).is_empty());
}

#[test]
fn exact_to_refused_and_moved_measurements_are_regressions() {
    let p = model(vec![exact(1.0)]);
    let n = model(vec![refused("kernel: invalid shell — open loop")]);
    let f = diff_model(&p, &n);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, Kind::Regression);
    assert!(f[0].detail.contains("now refused"), "{}", f[0].detail);

    let n = model(vec![exact(1.0 + 1e-6)]);
    let f = diff_model(&p, &n);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, Kind::Regression);
    assert!(f[0].detail.contains("volume"), "{}", f[0].detail);

    let mut t = exact(1.0);
    t.topology = Some([8, 12, 6, 1, 1, 0]);
    let f = diff_model(&p, &model(vec![t]));
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].detail.contains("topology"), "{}", f[0].detail);

    // A model that stopped parsing.
    let mut dead = model(vec![]);
    dead.outcome = "panic".into();
    let f = diff_model(&p, &dead);
    assert_eq!(f[0].kind, Kind::Regression);
}

#[test]
fn refused_to_exact_is_progress_and_a_new_wall_is_a_class_move() {
    let p = model(vec![refused(
        "extractor: surface B_SPLINE_SURFACE has no exact representation",
    )]);
    let f = diff_model(&p, &model(vec![exact(2.0)]));
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, Kind::Progress);

    let f = diff_model(
        &p,
        &model(vec![refused("kernel: invalid shell — open loop")]),
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, Kind::ClassMoved);

    let mut parses_now = model(vec![exact(1.0)]);
    let mut p2 = parses_now.clone();
    p2.outcome = "parse error: STEP file contains no solids".into();
    p2.shells.clear();
    parses_now.outcome = "ok".into();
    assert_eq!(diff_model(&p2, &parses_now)[0].kind, Kind::Progress);
}

#[test]
fn a_measurement_becoming_available_is_progress_and_losing_one_is_a_regression() {
    let mut no_area = exact(1.0);
    no_area.area_m2 = None;
    no_area.why = Some("area: CurvedGeometryMismatch — not implemented".into());
    let p = model(vec![no_area.clone()]);

    let f = diff_model(&p, &model(vec![exact(1.0)]));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, Kind::Progress);
    assert!(f[0].detail.contains("area now measured"), "{}", f[0].detail);

    let f = diff_model(&model(vec![exact(1.0)]), &p);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, Kind::Regression);
    assert!(
        f[0].detail.contains("area no longer measured"),
        "{}",
        f[0].detail
    );

    let mut other_text = no_area.clone();
    other_text.why = Some("area: CurvedGeometryMismatch — a different wall".into());
    let f = diff_model(&p, &model(vec![other_text]));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, Kind::ClassMoved);

    let mut unvalidated = exact(1.0);
    unvalidated.topology = None;
    unvalidated.why = Some("validate: open loop".into());
    let f = diff_model(&model(vec![exact(1.0)]), &model(vec![unvalidated]));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, Kind::Regression);
    assert!(
        f[0].detail.contains("no longer validates"),
        "{}",
        f[0].detail
    );
}

#[test]
fn a_changed_file_is_corrupt_and_nothing_else_is_compared() {
    let p = model(vec![exact(1.0)]);
    let mut n = model(vec![refused("kernel: PANIC")]);
    n.fnv1a64 = "def".into();
    let f = diff_model(&p, &n);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, Kind::Corrupt);
}

#[test]
fn the_pin_round_trips_through_json_byte_for_byte() {
    let pin = Pin {
        chunk: "0000".into(),
        sample_n: 2,
        max_bytes: 10,
        generated: "2026-10-03".into(),
        models: vec![
            model(vec![
                exact(0.1 + 0.2),
                refused("extractor: shell has no faces"),
            ]),
            {
                let mut m = model(vec![]);
                m.outcome = "timeout: 120s CPU (child killed)".into();
                m
            },
        ],
    };
    let json = serde_json::to_string_pretty(&pin).unwrap();
    let back: Pin = serde_json::from_str(&json).unwrap();
    assert_eq!(back, pin);
    assert_eq!(serde_json::to_string_pretty(&back).unwrap(), json);
    assert!(!json.contains("\"why\": null"), "absent fields stay absent");
}

#[test]
fn the_summary_counts_models_by_tier_and_walls_by_class() {
    let mut dead = model(vec![]);
    dead.outcome = "parse error: no solids".into();
    let s = summarize(&[
        model(vec![exact(1.0)]),
        model(vec![exact(1.0), refused("extractor: surface x")]),
        model(vec![refused("extractor: surface x")]),
        dead,
    ]);
    assert!(
        s.contains("1 fully exact (25.0 %), 1 mixed, 1 all mesh-tier"),
        "{s}"
    );
    assert!(s.contains("ok ×3, parse error ×1"), "{s}");
    assert!(s.contains("x2     extractor: surface x"), "{s}");
}

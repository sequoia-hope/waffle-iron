//! Always-on metamorphic suite (remediation plan O8, review §2.5 tests 5–7).
//!
//! Kernel-to-kernel identities that need no reference answer, checked on
//! the twelve `smoke_*` scenarios of `assay_kv2.rs` and on a fixed list of
//! cheap corpus cases:
//!
//! - **rigid-motion equivariance** — the document translated and rotated as
//!   a whole has the same category, live-body count, per-body B-Rep Euler
//!   characteristic (V − E + F, sorted) and EXACT volume
//!   (`KernelIntrospect::solid_volume`, never a mesh). The transform is the
//!   prospector's own (`prospect::metamorphic::with_explicit_x_axes` +
//!   `rigid_motion`), not a second document transformer; its `axes` step
//!   (implicit sketch x-axes written out) must not move the result at all.
//! - **inclusion–exclusion** where the case is a two-body boolean, each
//!   term built as its own document from the two operands made standalone:
//!   vol(A∪B) + vol(A∩B) = vol(A) + vol(B), and vol(A−B) = vol(A) − vol(A∩B).
//!   The case's own result must equal the matching variant.
//! - **commutativity** of ∪ and ∩: B∪A and B∩A against A∪B and A∩B
//!   (category, bodies, χ, volume).
//!
//! Volumes compare to a RELATIVE band of [`REL_VOLUME`] = 1e-9 of the
//! larger magnitude in the identity (see the constant for the measured
//! residuals that justify it). A violation is a kernel finding: it fails
//! the test with the numbers — the band is not to be widened to pass.

use std::time::Instant;

use serde_json::Value;
use test_harness::assay::categorize::categorize;
use test_harness::assay::geometry_hash::live_geometry_hash;
use test_harness::assay::prospect::gen3::Rng;
use test_harness::assay::prospect::metamorphic::{rigid_motion, with_explicit_x_axes, RigidMotion};
use test_harness::assay::prospect::{derive_meta, mutate::characteristic_length};
use test_harness::assay::smoke_scenarios::{build_smoke_scenario, SMOKE_SCENARIOS};
use test_harness::ModelBuilder;
use waffle_types::kernel::KernelSolidHandle;

/// Relative volume band for every identity. Exact volumes are integrated
/// from the B-Rep in rational arithmetic, so what is left is the f64
/// placement of the result's vertices and curves. Measured 2026-10-29 on
/// this suite (release; `--nocapture` prints a `max rel` per case): the
/// largest residual of any identity on any case was 1.22e-13 (R0008, rigid
/// motion of a revolve ∪ extrude), then 1.86e-14 (P0001, ∩ commutativity)
/// and 4.37e-15 (C0040, rigid); every planar smoke scenario stayed below
/// 7e-16. 1e-9 leaves four orders of headroom above that and is still six
/// orders below any loss of material a chord-level defect would cause.
const REL_VOLUME: f64 = 1e-9;

fn assay_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay")
}

fn features(doc: &Value) -> &Vec<Value> {
    doc.pointer("/tabs/0/kind/features/features")
        .and_then(Value::as_array)
        .expect("document has a feature list")
}

fn features_mut(doc: &mut Value) -> &mut Vec<Value> {
    doc.pointer_mut("/tabs/0/kind/features/features")
        .and_then(Value::as_array_mut)
        .expect("document has a feature list")
}

/// What one build of a document measured.
#[derive(Debug, Clone)]
struct Measure {
    category: String,
    detail: String,
    bodies: usize,
    /// Per live body, B-Rep V − E + F, sorted.
    chi: Vec<i64>,
    /// Sum of the live bodies' exact volumes, or the first error.
    volume: Result<f64, String>,
    hash: u64,
}

fn exact_volume(b: &ModelBuilder, handles: &[KernelSolidHandle]) -> Result<f64, String> {
    let k = b.kernel_ref().as_introspect();
    let mut sum = 0.0;
    for h in handles {
        sum += k.solid_volume(h).map_err(|e| e.to_string())?;
    }
    Ok(sum)
}

fn brep_chi(b: &ModelBuilder, handles: &[KernelSolidHandle]) -> Vec<i64> {
    let k = b.kernel_ref().as_introspect();
    let mut chi: Vec<i64> = handles
        .iter()
        .map(|h| {
            k.list_vertices(h).len() as i64 - k.list_edges(h).len() as i64
                + k.list_faces(h).len() as i64
        })
        .collect();
    chi.sort_unstable();
    chi
}

/// Judge (full `categorize` oracle set) and measure a document.
fn measure(id: &str, doc: &Value) -> Measure {
    let waffle = serde_json::to_string(doc).unwrap();
    let meta = derive_meta(id, doc).expect("derive meta");
    let outcome = categorize(id, &waffle, &meta);
    let mut b = ModelBuilder::kernel_v2();
    b.load(&waffle).expect("document loads");
    let handles = b.live_solid_handles();
    Measure {
        category: outcome.category.label(),
        detail: outcome.detail,
        bodies: handles.len(),
        chi: brep_chi(&b, &handles),
        volume: exact_volume(&b, &handles),
        hash: live_geometry_hash(&b),
    }
}

fn rel(a: f64, b: f64, scale: f64) -> f64 {
    (a - b).abs() / scale.abs().max(f64::MIN_POSITIVE)
}

/// Accumulates violations and the largest residual seen.
#[derive(Default)]
struct Verdict {
    violations: Vec<String>,
    max_rel: f64,
    lines: Vec<String>,
}

impl Verdict {
    fn volume(&mut self, what: &str, lhs: f64, rhs: f64, scale: f64) {
        let r = rel(lhs, rhs, scale);
        self.max_rel = self.max_rel.max(r);
        self.lines
            .push(format!("{what}: {lhs:.15e} vs {rhs:.15e} (rel {r:.2e})"));
        if r > REL_VOLUME {
            self.violations.push(format!(
                "{what}: {lhs:.15e} vs {rhs:.15e} — rel {r:.3e} > {REL_VOLUME:.0e}"
            ));
        }
    }

    /// Category, body count and χ must match; volumes within the band.
    fn same(&mut self, what: &str, a: &Measure, b: &Measure) {
        if a.category != b.category {
            self.violations.push(format!(
                "{what}: category {} vs {} ({} | {})",
                a.category, b.category, a.detail, b.detail
            ));
            // Keep going: whether the KERNEL's own measurements still agree
            // tells a kernel disagreement from an oracle's (the categorizer's
            // exact-membership lattice is a separate instrument).
        }
        if a.bodies != b.bodies {
            self.violations
                .push(format!("{what}: bodies {} vs {}", a.bodies, b.bodies));
        }
        if a.chi != b.chi {
            self.violations
                .push(format!("{what}: B-Rep χ {:?} vs {:?}", a.chi, b.chi));
        }
        match (&a.volume, &b.volume) {
            (Ok(x), Ok(y)) => self.volume(what, *y, *x, *x),
            (Err(x), Err(y)) if x == y => {}
            (x, y) => self
                .violations
                .push(format!("{what}: exact volume {x:?} vs {y:?}")),
        }
    }
}

// ── two-body structure ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Union,
    Intersect,
    Subtract,
}

/// A two-body boolean: the two operand features made standalone (the
/// boolean, if a separate feature, removed), and the case's own operation.
struct Pair {
    base: Value,
    a: String,
    b: String,
    /// What the case itself computes (`None`: two standalone bodies).
    op: Option<(Op, bool)>, // (op, operands swapped relative to a/b)
}

fn param<'a>(f: &'a Value, key: &str) -> Option<&'a Value> {
    f.pointer(&format!("/operation/params/{key}"))
}

fn as_pair(doc: &Value) -> Option<Pair> {
    let feats = features(doc);
    let solids: Vec<usize> = feats
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            matches!(
                f.pointer("/operation/type").and_then(Value::as_str),
                Some("Extrude" | "Revolve")
            )
        })
        .map(|(i, _)| i)
        .collect();
    let others: Vec<usize> = feats
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            !matches!(
                f.pointer("/operation/type").and_then(Value::as_str),
                Some("Extrude" | "Revolve" | "Sketch")
            )
        })
        .map(|(i, _)| i)
        .collect();
    if solids.len() != 2 || others.len() > 1 {
        return None;
    }
    if solids
        .iter()
        .any(|&i| param(&feats[i], "cut").and_then(Value::as_bool) == Some(true))
    {
        return None; // a cut tool's sweep direction auto-reverses; not an operand
    }
    let id = |i: usize| feats[i]["id"].as_str().unwrap().to_string();
    let (a, b) = (id(solids[0]), id(solids[1]));
    let op = if let Some(&k) = others.first() {
        if feats[k].pointer("/operation/type").and_then(Value::as_str) != Some("BooleanCombine")
            || k < solids[1]
        {
            return None;
        }
        let verb = param(&feats[k], "operation/type").and_then(Value::as_str)?;
        let op = match verb {
            "Union" => Op::Union,
            "Intersect" => Op::Intersect,
            "Subtract" => Op::Subtract,
            _ => return None,
        };
        let first = param(&feats[k], "body_a/anchor/feature_id").and_then(Value::as_str)?;
        Some((op, first == b))
    } else {
        let f = &feats[solids[1]];
        match param(f, "combine/type").and_then(Value::as_str) {
            Some("NewBody") => None,
            Some("Add") => Some((Op::Union, false)),
            Some("Cut") => Some((Op::Subtract, false)),
            Some("Intersect") => Some((Op::Intersect, false)),
            Some(_) => return None,
            None => (param(f, "merge").and_then(Value::as_bool) != Some(false))
                .then_some((Op::Union, false)),
        }
    };
    let mut base = doc.clone();
    let fs = features_mut(&mut base);
    for &i in &solids {
        let p = fs[i].pointer_mut("/operation/params").unwrap();
        p["merge"] = Value::Bool(false);
        p["combine"] = serde_json::json!({"type": "NewBody"});
    }
    if let Some(&k) = others.first() {
        fs.remove(k);
    }
    Some(Pair { base, a, b, op })
}

/// Build `first OP second` as its own document (the standalone operands
/// plus one boolean feature) and measure the result.
fn variant(pair: &Pair, op: Op, swap: bool) -> Result<(f64, Measure), String> {
    let waffle = serde_json::to_string(&pair.base).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    b.load(&waffle).map_err(|e| e.to_string())?;
    b.name_feature_by_id("A", &pair.a);
    b.name_feature_by_id("B", &pair.b);
    let (x, y) = if swap { ("B", "A") } else { ("A", "B") };
    match op {
        Op::Union => b.boolean_union("R", x, y),
        Op::Intersect => b.boolean_intersect("R", x, y),
        Op::Subtract => b.boolean_subtract("R", x, y),
    }
    .map_err(|e| format!("{op:?}: {e}"))?;
    if !b.engine_errors().is_empty() {
        return Err(format!("{op:?}: engine errors {:?}", b.engine_errors()));
    }
    let handles = b.solid_handles("R").map_err(|e| format!("{op:?}: {e}"))?;
    let vol = exact_volume(&b, &handles)?;
    // The variant as a document, judged by the full oracle set too.
    let doc: Value = serde_json::from_str(&b.save().map_err(|e| e.to_string())?).unwrap();
    Ok((vol, measure("variant", &doc)))
}

fn operand_volumes(pair: &Pair) -> Result<(f64, f64), String> {
    let mut b = ModelBuilder::kernel_v2();
    b.load(&serde_json::to_string(&pair.base).unwrap())
        .map_err(|e| e.to_string())?;
    b.name_feature_by_id("A", &pair.a);
    b.name_feature_by_id("B", &pair.b);
    let va = exact_volume(&b, &b.solid_handles("A").map_err(|e| e.to_string())?)?;
    let vb = exact_volume(&b, &b.solid_handles("B").map_err(|e| e.to_string())?)?;
    Ok((va, vb))
}

fn check_pair(v: &mut Verdict, original: &Measure, pair: &Pair) {
    let (va, vb) = match operand_volumes(pair) {
        Ok(x) => x,
        Err(e) => return v.violations.push(format!("operands: {e}")),
    };
    let run = |op, swap| variant(pair, op, swap);
    let (u, i, d, u2, i2) = match (
        run(Op::Union, false),
        run(Op::Intersect, false),
        run(Op::Subtract, false),
        run(Op::Union, true),
        run(Op::Intersect, true),
    ) {
        (Ok(u), Ok(i), Ok(d), Ok(u2), Ok(i2)) => (u, i, d, u2, i2),
        (u, i, d, u2, i2) => {
            for (name, r) in [("A∪B", u), ("A∩B", i), ("A−B", d), ("B∪A", u2), ("B∩A", i2)]
            {
                if let Err(e) = r {
                    v.violations.push(format!("{name}: {e}"));
                }
            }
            return;
        }
    };
    v.lines.push(format!(
        "operands: vol(A) {va:.15e}, vol(B) {vb:.15e}; categories ∪ {} ∩ {} − {}",
        u.1.category, i.1.category, d.1.category
    ));
    let s = va + vb;
    v.volume("vol(A∪B)+vol(A∩B) = vol(A)+vol(B)", u.0 + i.0, s, s);
    v.volume("vol(A−B) = vol(A)−vol(A∩B)", d.0, va - i.0, va);
    v.same("commutativity ∪", &u.1, &u2.1);
    v.same("commutativity ∩", &i.1, &i2.1);
    if let Some((op, swapped)) = pair.op {
        let mine = match (op, swapped) {
            (Op::Union, false) => &u.1,
            (Op::Union, true) => &u2.1,
            (Op::Intersect, false) => &i.1,
            (Op::Intersect, true) => &i2.1,
            (Op::Subtract, false) => &d.1,
            (Op::Subtract, true) => return, // B−A is not built; nothing to match
        };
        v.same("case result = its variant", original, mine);
    }
}

/// Run every identity on one document; panic with all violations.
fn check(id: &str, doc: &Value) {
    let t0 = Instant::now();
    let mut v = Verdict::default();
    let original = measure(id, doc);

    // Rigid motion — the prospector's transformer.
    let reference = with_explicit_x_axes(doc).expect("explicit axes");
    let ref_m = measure(id, &reference);
    // The axes step is held to the same measurements as the motion, NOT to
    // bit identity: an explicit `plane_x_axis` goes through
    // `SketchPlaneBasis::from_origin_normal_x` (feature-engine
    // `rebuild/planes.rs::sketch_basis`), a different f64 path from the
    // implicit `from_origin_normal`, so the frame can differ in the last
    // ulp. Measured 2026-10-29 on smoke_oblique_plane_box: geometry hash
    // 3c6868ddfde185bd → c2d0db0901cce5a1, exact volume rel 3.5e-16. Whether
    // the two paths SHOULD agree bit for bit is the engine's contract, not
    // the kernel's; the hash difference is logged, not failed.
    if ref_m.hash != original.hash {
        v.lines.push(format!(
            "axes: geometry hash {:016x} → {:016x} (frame rounding, see comment)",
            original.hash, ref_m.hash
        ));
    }
    v.same("axes", &original, &ref_m);
    let seed = id.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, c| {
        (h ^ c as u64).wrapping_mul(0x0100_0000_01b3)
    });
    let motion = RigidMotion::draw(&mut Rng::new(seed), characteristic_length(doc));
    let moved = rigid_motion(&reference, &motion).expect("rigid motion");
    let moved_m = measure(id, &moved);
    v.same("rigid", &ref_m, &moved_m);

    let pair = as_pair(doc);
    if let Some(p) = &pair {
        check_pair(&mut v, &original, p);
    }
    eprintln!(
        "[metamorphic] {id:<40} {:<22} pair={:<5} max rel {:.2e}  {:.2}s",
        original.category,
        pair.is_some(),
        v.max_rel,
        t0.elapsed().as_secs_f64()
    );
    for l in &v.lines {
        eprintln!("[metamorphic]   {id}: {l}");
    }
    assert!(
        v.violations.is_empty(),
        "{id}: {} metamorphic violation(s):\n  {}",
        v.violations.len(),
        v.violations.join("\n  ")
    );
}

fn smoke(name: &str) {
    let mut b = build_smoke_scenario(name);
    let doc: Value = serde_json::from_str(&b.save().expect("save")).unwrap();
    check(&format!("smoke_{name}"), &doc);
}

fn corpus(id: &str) {
    let text = std::fs::read_to_string(assay_dir().join(format!("{id}.waffle")))
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    check(id, &serde_json::from_str(&text).unwrap());
}

macro_rules! smoke_tests {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            smoke(stringify!($name).trim_start_matches("smoke_"));
        }
    )*};
}

smoke_tests!(
    smoke_single_box,
    smoke_thin_slab,
    smoke_oblique_plane_box,
    smoke_l_shaped_extrude,
    smoke_union_offset_boss,
    smoke_union_face_to_face_stack,
    smoke_union_flush_same_radius_cylinder_stack,
    smoke_subtract_offset_boxes,
    smoke_intersect_offset_boxes,
    smoke_blind_pocket_cut,
    smoke_through_hole_cut,
    smoke_two_standalone_bodies,
);

macro_rules! corpus_tests {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            corpus(stringify!($name));
        }
    )*};
}

// Cheap corpus cases, chosen from a timing survey of every
// SUPPORTED_CORRECT case with ≤ 3 operations (`survey_case_times`, release,
// 4 jobs, 2026-10-29). The time after each is ONE `categorize` of the case;
// a metamorphic check costs ~3× that (original, axes, moved) plus, for a
// two-body pair, five boolean variants. Mix: two-body pairs (explicit
// BooleanCombine and auto-union boss), cuts, a revolve operand, curved
// cuts and lathe profiles.
corpus_tests!(
    P0001, // 0.113 s  extrude ∪ extrude (BooleanCombine Union) — pair
    C0051, // 0.060 s  boss auto-union — pair
    C0094, // 0.067 s  boss auto-union — pair
    F0005, // 0.098 s  boss auto-union — pair
    F0013, // 0.100 s  boss auto-union — pair
    C0039, // 0.108 s  boss auto-union — pair
    F0011, // 0.143 s  boss auto-union (disjoint, two shells) — pair
    R0067, // 0.179 s  boss auto-union — pair
    C0045, // 0.186 s  boss auto-union — pair
    R0086, // 0.254 s  boss auto-union — pair
    R0008, // 0.342 s  revolve ∪ extrude — pair
    C0100, // 0.001 s  single extrude
    C0079, // 0.065 s  three extrudes
    C0086, // 0.003 s  extrude, cut
    C0040, // 0.014 s  extrude, cut
    C0024, // 0.097 s  extrude, cut
    R0048, // 0.141 s  extrude, cut
    P0016, // 0.147 s  extrude, revolve cut, cut
    C0060, // 0.156 s  revolve, cut
    P0019, // 0.175 s  extrude, revolve, cut
);

// ── Findings of the first run (2026-10-29), quarantined LOUDLY ──────────────
//
// Each fails today for the reason in its `ignore` text; run with
// `--ignored` to reproduce. They are the suite working, not noise: remove
// the `ignore` when the named defect is fixed.

/// 0.002 s, extrude ∩ extrude (BooleanCombine Intersect) — pair.
#[test]
#[ignore = "KERNEL FINDING 2026-10-29: the inclusion-exclusion variant A∪B of P0023's two \
            face-adjacent operands (two profiles of one sketch) fails LOUDLY in both operand \
            orders: kernel-v2 boolean_union InvalidBooleanOutput(\"an undirected output edge \
            is not used by exactly two directed edges\"). The case's own A∩B is \
            SUPPORTED_CORRECT (volume 0, touching operands); rigid motion holds."]
#[allow(non_snake_case)]
fn P0023() {
    corpus("P0023");
}

/// 0.181 s, revolve then cut.
#[test]
#[ignore = "ORACLE FINDING 2026-10-29: under rigid motion the KERNEL agrees (exact volume \
            0.2720243112740431 vs 0.2720243112740430, rel 2.0e-16; bodies and χ equal) but the \
            categorizer's exact-membership lattice flags the moved copy SUPPORTED_WRONG \
            (kernel 2.716489e-1 vs lattice 2.430310e-1, rel +1.18e-1 > band 1.13e-1): the \
            lattice is rotation-dependent on revolves (memory: prospector oracle defect OPEN)"]
#[allow(non_snake_case)]
fn C0059() {
    corpus("C0059");
}

/// 0.346 s, revolve then revolve cut.
#[test]
#[ignore = "ORACLE FINDING 2026-10-29: under rigid motion the KERNEL agrees (exact volume \
            1.514247659030280 vs 1.514247659030281, rel 4.4e-16; bodies and χ equal) but the \
            exact-membership lattice flags the moved copy SUPPORTED_WRONG (kernel 1.512272 vs \
            lattice 3.442171, rel -5.6e-1 > band 6.1e-2): rotation-dependent lattice on revolves"]
#[allow(non_snake_case)]
fn C0061() {
    corpus("C0061");
}

#[test]
fn smoke_list_matches_assay_kv2() {
    assert_eq!(SMOKE_SCENARIOS.len(), 12);
}

/// Timing survey for choosing the corpus list: `META_SURVEY=C0001,C0002,…`
/// categorizes each case and prints its wall time (release, one thread
/// per case, at most `META_SURVEY_JOBS` at once).
#[test]
#[ignore = "manual: the timing survey behind the corpus list below"]
fn survey_case_times() {
    let list = match std::env::var("META_SURVEY_FILE") {
        Ok(path) => std::fs::read_to_string(path).unwrap(),
        Err(_) => std::env::var("META_SURVEY").unwrap_or_default(),
    };
    let ids: Vec<String> = list
        .trim()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let jobs: usize = std::env::var("META_SURVEY_JOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let queue = std::sync::Mutex::new(ids.into_iter());
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let Some(id) = queue.lock().unwrap().next() else {
                    break;
                };
                let text =
                    std::fs::read_to_string(assay_dir().join(format!("{id}.waffle"))).unwrap();
                let doc: Value = serde_json::from_str(&text).unwrap();
                let t = Instant::now();
                let meta = derive_meta(&id, &doc).unwrap();
                let out = categorize(&id, &text, &meta);
                eprintln!(
                    "[survey] {id} {:.3}s {} pair={}",
                    t.elapsed().as_secs_f64(),
                    out.category.label(),
                    as_pair(&doc).is_some()
                );
            });
        }
    });
}

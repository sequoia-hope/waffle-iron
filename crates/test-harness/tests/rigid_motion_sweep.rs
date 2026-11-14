//! Rigid-motion sweep of one corpus case: does the kernel give the same
//! result when the whole document is rotated and translated?
//!
//! The corpus, the prospector seeds and the suites hold each case in the
//! orientation it was authored in. A mechanism built against rounding, or a
//! defect that only rounding exposes, is invisible there: an axis-aligned
//! plane holds a coincident edge exactly, an oblique one cannot. This probe
//! moves a case by `SWEEP_N` seeded rigid motions (the prospector's own
//! transformer, `prospect::metamorphic`) and reads each moved build against
//! the unmoved one.
//!
//! The verdict is the KERNEL's, not the categorizer's (whose exact-membership
//! lattice is rotation-dependent on revolves). Per motion, one line:
//!
//! - `OK`: no engine error, no failed auto-union, and the same body count,
//!   per-body B-Rep Euler characteristic and volume as the unmoved build.
//!   Volume is exact where the kernel has a closed form (compared at 1e-9
//!   relative) and the render mesh's otherwise (compared at 1e-2, the mesh's
//!   own orientation noise).
//! - `ERR`: an engine error, or an auto-union that failed and left its
//!   operands as separate bodies (a warning in the app). Loud.
//! - `DIFF`: built without complaint and differs. Read these first: a
//!   different volume is a wrong model.
//!
//! ```text
//! SWEEP_CASE=P0054 SWEEP_N=4 cargo test -p test-harness \
//!   --test rigid_motion_sweep --release sweep -- --ignored --nocapture
//! ```
//!
//! `steps` builds the document truncated after each operation, unmoved and
//! under motion `SWEEP_K`, to find the first operation whose result differs.
//!
//! Use: before removing a kernel mechanism, run the cases it acts on (or the
//! whole corpus: about ten minutes at 14 jobs, four motions each) from a
//! tree with the mechanism and a tree without, and compare line by line.
//! Recorded findings: `docs/yang_tail_triage.md`, 2026-11-13.

use serde_json::Value;
use test_harness::assay::prospect::gen3::Rng;
use test_harness::assay::prospect::metamorphic::{rigid_motion, with_explicit_x_axes, RigidMotion};
use test_harness::assay::prospect::mutate::characteristic_length;
use test_harness::assay::volume_oracle_doc::truncate_ops;
use test_harness::ModelBuilder;

/// What one build measured.
struct Measure {
    /// Engine errors, then failed auto-unions.
    errs: Vec<String>,
    bodies: usize,
    /// Per live body, B-Rep V − E + F, sorted.
    chi: Vec<i64>,
    exact_volume: Result<f64, String>,
    mesh_volume: Result<f64, String>,
}

fn measure(doc: &Value) -> Measure {
    let waffle = serde_json::to_string(doc).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    if let Err(e) = b.load(&waffle) {
        return Measure {
            errs: vec![format!("load: {e}")],
            bodies: 0,
            chi: vec![],
            exact_volume: Err("load".into()),
            mesh_volume: Err("load".into()),
        };
    }
    let handles = b.live_solid_handles();
    let k = b.kernel_ref().as_introspect();
    let mut chi: Vec<i64> = handles
        .iter()
        .map(|h| {
            k.list_vertices(h).len() as i64 - k.list_edges(h).len() as i64
                + k.list_faces(h).len() as i64
        })
        .collect();
    chi.sort_unstable();
    let exact_volume = handles
        .iter()
        .map(|h| k.solid_volume(h).map_err(|e| e.to_string()))
        .sum::<Result<f64, String>>();
    let mut errs: Vec<String> = b.engine_errors().iter().map(|e| format!("{e:?}")).collect();
    errs.extend(
        b.engine_warnings()
            .iter()
            .filter(|w| w.contains("Auto-union failed"))
            .map(|w| format!("WARNING {w}")),
    );
    let bodies = handles.len();
    let mesh_volume = b
        .tessellate_live_with_tol(characteristic_length(doc) * 1e-4)
        .map(|ms| {
            ms.iter()
                .map(test_harness::helpers::mesh_signed_volume)
                .sum::<f64>()
        })
        .map_err(|e| e.to_string());
    Measure {
        errs,
        bodies,
        chi,
        exact_volume,
        mesh_volume,
    }
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / a.abs().max(f64::MIN_POSITIVE)).abs()
}

fn verdict(reference: &Measure, moved: &Measure) -> &'static str {
    if !moved.errs.is_empty() {
        return "ERR";
    }
    let same_volume = match (
        &reference.exact_volume,
        &moved.exact_volume,
        &reference.mesh_volume,
        &moved.mesh_volume,
    ) {
        (Ok(a), Ok(b), _, _) => rel(*a, *b) < 1e-9,
        (Err(_), Err(_), Ok(a), Ok(b)) => rel(*a, *b) < 1e-2,
        _ => false,
    };
    if moved.bodies == reference.bodies && moved.chi == reference.chi && same_volume {
        "OK"
    } else {
        "DIFF"
    }
}

fn show(m: &Measure) -> String {
    let exact = match &m.exact_volume {
        Ok(v) => format!("{v:.12e}"),
        Err(_) => "n/a".into(),
    };
    let mesh = match &m.mesh_volume {
        Ok(v) => format!("{v:.6e}"),
        Err(e) => format!("ERR({})", e.chars().take(80).collect::<String>()),
    };
    format!(
        "bodies={} chi={:?} vol={exact} mesh={mesh}",
        m.bodies, m.chi
    )
}

fn case_document() -> (String, Value) {
    let id = std::env::var("SWEEP_CASE").expect("set SWEEP_CASE to a corpus case id");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay");
    let text = std::fs::read_to_string(dir.join(format!("{id}.waffle")))
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    (id, serde_json::from_str(&text).unwrap())
}

/// Motion `k` of a case: seeded by the case id, so two trees draw the same.
fn motion(id: &str, doc: &Value, k: u64) -> RigidMotion {
    let base = id.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, c| {
        (h ^ c as u64).wrapping_mul(0x0100_0000_01b3)
    });
    let seed = base ^ (k + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    RigidMotion::draw(&mut Rng::new(seed), characteristic_length(doc))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "probe: set SWEEP_CASE (and SWEEP_N); prints one SWEEP line per motion"]
fn sweep() {
    let (id, doc) = case_document();
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    let m0 = measure(&reference);
    println!("SWEEP {id} ref errs={} {}", m0.errs.len(), show(&m0));
    for k in 0..env_u64("SWEEP_N", 4) {
        let moved = rigid_motion(&reference, &motion(&id, &doc, k)).expect("rigid motion");
        let m = measure(&moved);
        let first = m
            .errs
            .first()
            .map(|s| s.chars().take(260).collect::<String>())
            .unwrap_or_default();
        println!("SWEEP {id} {k} {} {} {first}", verdict(&m0, &m), show(&m));
    }
}

#[test]
#[ignore = "probe: set SWEEP_CASE (and SWEEP_K); prints one STEPS line per operation"]
fn steps() {
    let (id, doc) = case_document();
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    let moved =
        rigid_motion(&reference, &motion(&id, &doc, env_u64("SWEEP_K", 0))).expect("rigid motion");
    let mut last = (0usize, 0usize);
    for n in 1.. {
        let (Some(a), Some(b)) = (truncate_ops(&reference, n), truncate_ops(&moved, n)) else {
            break;
        };
        // `truncate_ops` returns the whole document once `n` passes its
        // operation count; stop when a step adds nothing.
        let size = (
            serde_json::to_string(&a).unwrap().len(),
            serde_json::to_string(&b).unwrap().len(),
        );
        if size == last {
            break;
        }
        last = size;
        let (ma, mb) = (measure(&a), measure(&b));
        println!(
            "STEPS {id} ops={n} {} | unmoved: errs={} {} | moved: errs={} {}",
            verdict(&ma, &mb),
            ma.errs.len(),
            show(&ma),
            mb.errs.len(),
            show(&mb)
        );
    }
}

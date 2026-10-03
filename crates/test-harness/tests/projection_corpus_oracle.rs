//! The corpus-wide half of the **projection oracle**
//! (`specs/drawings_and_mbd.md` §5.3), over every assay case.
//!
//! The per-primitive half lives in `kernel_v2::projection::tests` and runs in
//! the inner loop. This sweep is the same two checks over the real documents
//! of `app/tests/cases/assay` — gear unions, chained booleans, revolves, the
//! cases that found most of the kernel's tail — and it is `#[ignore]`d because
//! REBUILDING those documents is the cost of the assay itself.
//!
//! It stride-samples by default for the same reason (see [`stride`]); the
//! exhaustive pass is one variable away.
//!
//! ```text
//! cargo test -p test-harness --test projection_corpus_oracle --release \
//!     -- --ignored --nocapture
//! PROJECTION_ORACLE_STRIDE=1 cargo test -p test-harness \
//!     --test projection_corpus_oracle --release -- --ignored --nocapture
//! ```
//!
//! ## What is asserted, and what §5.3 cannot assert yet
//!
//! For every case that rebuilds, and every one of the six axis directions:
//!
//! 1. **The projected bbox lies inside the AABB's projection.** §5.3 states
//!    this as an EQUALITY, which is a D1b statement: at D1a a view holds edges
//!    only, and a curved solid's extreme points are generally on a silhouette.
//!    Containment is the half that holds at D1a, and it is the half that
//!    catches a wrong basis, a swapped axis or a mis-signed depth. (The
//!    kernel's `solid_aabb` is also documented CONSERVATIVE on curved edges
//!    and answers `None` for a solid carrying a surface-pair curve, so some
//!    cases are not boundable at all — counted and reported, never silently
//!    skipped.)
//! 2. **Total visible length is invariant under a 180° rotation about the
//!    view axis.** That rotation is an isometry of the view plane, so every
//!    analytic reconstruction the projection performs — the ellipse's
//!    principal axes and parameter range, the circular and edge-on special
//!    cases — must come out the same length. It holds for every solid at D1a,
//!    and it is the check that would catch a sign error in the reconstruction
//!    on geometry no hand-written fixture contains.
//!
//! A case the KERNEL cannot build is not a projection failure: those are the
//! assay's own business (`assay_kv2.rs` scores them) and are counted here as
//! `not_built`. A projection that FAILS on a solid the kernel did build is a
//! failure of this oracle, and is reported with the case id.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use test_harness::workflow::ModelBuilder;
use waffle_types::kernel::projection::{Aabb2, ProjectOpts, ViewFrame};
use waffle_types::kernel::{ProjectionBody, Visibility};

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

/// How many cases to skip between samples.
///
/// The sweep's cost is the ASSAY's cost — rebuilding a corpus case is the
/// expensive part, and the heaviest 20-op chained-boolean cases run for
/// several CPU-minutes each, so a full single-threaded pass is hours. The
/// default therefore STRIDE-SAMPLES, the same shape as the SI5 C7 corpus gate,
/// and the exhaustive sweep is one environment variable away:
/// `PROJECTION_ORACLE_STRIDE=1`. The sample is deterministic (sorted ids, a
/// fixed stride), so a failure is reproducible by id.
///
/// Measured 2026-10-03 at the default stride 8, in `--release`: 41 of 321
/// cases, **381 s single-threaded** (6 m 21 s wall), 40 projected in all six
/// directions with 0 failures; 1 not built (C0113, one of the corpus's seven
/// loud-by-design C-series walls) and 36 of the 246 (case, direction) pairs
/// not boundable, which is 6 cases carrying a surface-pair curve.
fn stride() -> usize {
    std::env::var("PROJECTION_ORACLE_STRIDE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(8)
}

/// Every case id in the corpus, sorted.
fn all_case_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(corpus_dir())
        .unwrap_or_else(|e| panic!("{}: {e}", corpus_dir().display()))
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".waffle"))
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

/// The six axis directions §5.3 names.
fn axis_views() -> Vec<(&'static str, ViewFrame)> {
    vec![
        ("+x", ViewFrame::looking_along([1.0, 0.0, 0.0])),
        ("-x", ViewFrame::looking_along([-1.0, 0.0, 0.0])),
        ("+y", ViewFrame::looking_along([0.0, 1.0, 0.0])),
        ("-y", ViewFrame::looking_along([0.0, -1.0, 0.0])),
        ("+z", ViewFrame::looking_along([0.0, 0.0, 1.0])),
        ("-z", ViewFrame::looking_along([0.0, 0.0, -1.0])),
    ]
}

/// A half turn about the view axis: the same line of sight, `up` reversed.
fn half_turned(frame: &ViewFrame) -> ViewFrame {
    ViewFrame {
        origin: frame.origin,
        dir: frame.dir,
        up: [-frame.up[0], -frame.up[1], -frame.up[2]],
    }
}

fn aabb_projection(
    basis: &waffle_types::kernel::projection::ViewBasis,
    lo: [f64; 3],
    hi: [f64; 3],
) -> Aabb2 {
    let mut bb: Option<Aabb2> = None;
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let (uv, _) = basis.project(p);
        bb = Some(match bb {
            None => Aabb2::point(uv),
            Some(b) => b.united_point(uv),
        });
    }
    bb.expect("eight corners")
}

#[derive(Default)]
struct Tally {
    /// Cases whose document would not rebuild — the assay's business.
    not_built: Vec<String>,
    /// Cases that built and projected in all six directions.
    projected: usize,
    /// `(case, direction)` the kernel could not bound (surface-pair curves),
    /// where the containment half did not run.
    unbounded: usize,
    /// `(case, direction)` the containment half DID check. Asserted below:
    /// without it, a regression that made every solid unboundable would turn
    /// check 1 vacuous while the sweep still passed.
    bounded: usize,
    /// Failures, by case id.
    failures: BTreeMap<String, Vec<String>>,
}

#[test]
#[ignore = "corpus-wide: rebuilds every assay case; run with --release --ignored (minutes)"]
fn the_projection_oracle_holds_over_the_whole_assay_corpus() {
    // The model scale varies across the corpus (metres to millimetres), so
    // every tolerance is RELATIVE to the case's own projected extent. An
    // absolute slack would be meaningless on both ends of that range.
    const REL_SLACK: f64 = 1e-9;

    let all = all_case_ids();
    assert!(
        all.len() > 100,
        "expected the assay corpus, found {} cases in {}",
        all.len(),
        corpus_dir().display()
    );
    let stride = stride();
    let ids: Vec<String> = all.iter().step_by(stride).cloned().collect();
    println!(
        "projection oracle: {} of {} corpus cases (stride {stride}; \
         PROJECTION_ORACLE_STRIDE=1 for all of them)",
        ids.len(),
        all.len()
    );

    let mut tally = Tally::default();
    for id in &ids {
        let path = corpus_dir().join(format!("{id}.waffle"));
        let json = match fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) => {
                tally.not_built.push(format!("{id} (unreadable: {e})"));
                continue;
            }
        };
        let mut builder = ModelBuilder::kernel_v2();
        if builder.load(&json).is_err() || !builder.engine_errors().is_empty() {
            tally.not_built.push(id.clone());
            continue;
        }
        let handles = builder.live_solid_handles();
        if handles.is_empty() {
            tally.not_built.push(format!("{id} (no live body)"));
            continue;
        }
        let bodies: Vec<ProjectionBody> = handles
            .iter()
            .map(|h| ProjectionBody::solo(h.clone()))
            .collect();

        // The conservative AABB of the union of the live bodies, when the
        // kernel can bound every one of them.
        let mut bounds: Option<([f64; 3], [f64; 3])> = Some(([f64::MAX; 3], [f64::MIN; 3]));
        for h in &handles {
            match (builder.kernel_mut().as_introspect().solid_aabb(h), bounds) {
                (Some((lo, hi)), Some((mut l, mut u))) => {
                    for k in 0..3 {
                        l[k] = l[k].min(lo[k]);
                        u[k] = u[k].max(hi[k]);
                    }
                    bounds = Some((l, u));
                }
                _ => bounds = None,
            }
        }

        let mut problems = Vec::new();
        for (name, frame) in axis_views() {
            let basis = frame.basis().expect("an axis view has a basis");
            let opts = ProjectOpts::default();
            let view = match builder.kernel_mut().project_bodies(&bodies, &frame, &opts) {
                Ok(v) => v,
                Err(e) => {
                    problems.push(format!("{name}: project_bodies failed: {e}"));
                    continue;
                }
            };
            let Some(got) = view.bbox else {
                problems.push(format!("{name}: a built solid projected no curves"));
                continue;
            };
            let extent = (got.max.x() - got.min.x())
                .abs()
                .max((got.max.y() - got.min.y()).abs())
                .max(1.0);

            // 1. Containment in the AABB's projection.
            if let Some((lo, hi)) = bounds {
                tally.bounded += 1;
                let want = aabb_projection(&basis, lo, hi);
                if !got.within(&want, REL_SLACK * extent) {
                    problems.push(format!(
                        "{name}: projected {got:?} escapes the AABB projection {want:?}"
                    ));
                }
            } else {
                tally.unbounded += 1;
            }

            // 2. The half-turn length invariant.
            let turned = half_turned(&frame);
            match builder.kernel_mut().project_bodies(&bodies, &turned, &opts) {
                Err(e) => problems.push(format!("{name}: the half turn failed: {e}")),
                Ok(other) => {
                    let a = view.total_length(Visibility::Visible);
                    let b = other.total_length(Visibility::Visible);
                    if a <= 0.0 {
                        problems.push(format!("{name}: zero visible length"));
                    } else if (a - b).abs() > REL_SLACK * a {
                        problems.push(format!(
                            "{name}: visible length {a} becomes {b} after a half turn"
                        ));
                    }
                }
            }
        }
        if problems.is_empty() {
            tally.projected += 1;
        } else {
            tally.failures.insert(id.clone(), problems);
        }
    }

    println!(
        "projection oracle over {} cases: {} projected in all six directions, \
         {} not built (the assay's own business), {} of {} (case, direction) pairs \
         bounded and containment-checked, {} failing cases",
        ids.len(),
        tally.projected,
        tally.not_built.len(),
        tally.bounded,
        tally.bounded + tally.unbounded,
        tally.failures.len()
    );
    if !tally.not_built.is_empty() {
        println!("not built: {}", tally.not_built.join(", "));
    }
    for (id, problems) in &tally.failures {
        println!("FAIL {id}:");
        for p in problems {
            println!("    {p}");
        }
    }

    // A sweep that built nothing would pass vacuously, which is the one way
    // this oracle could lie.
    assert!(
        tally.projected * 2 > ids.len(),
        "only {} of {} sampled cases projected — the sweep proved nothing",
        tally.projected,
        ids.len()
    );
    // And so would a sweep where nothing could be BOUNDED: the containment
    // half runs only on a case `solid_aabb` answers for, and it records no
    // problem for one it skips, so a regression that made every solid
    // unboundable would silence check 1 without failing anything. At stride 8
    // the sweep's own measurement is 36 of 246 pairs unbounded, so 210 bounded.
    assert!(
        tally.bounded > tally.unbounded,
        "only {} of {} (case, direction) pairs could be bounded — the \
         containment half of the oracle barely ran",
        tally.bounded,
        tally.bounded + tally.unbounded
    );
    assert!(
        tally.failures.is_empty(),
        "{} case(s) fail the projection oracle (listed above)",
        tally.failures.len()
    );
}

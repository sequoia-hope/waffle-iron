//! The corpus-wide **section oracle** (`specs/drawings_and_mbd.md` §5.3), over
//! every assay case: cut each live body at its own AABB mid-plane in all three
//! axes and check what comes back.
//!
//! The per-primitive half lives in `kernel_v2::projection::section::tests` and
//! runs in the inner loop with closed-form areas (a box's cross-section,
//! `π·a·b` for an oblique cylinder, `π·r²` for a bore). This sweep is the
//! property over the real documents of `app/tests/cases/assay` — gear unions,
//! chained booleans, revolves, the cases that found most of the kernel's tail.
//!
//! ```text
//! cargo test -p test-harness --test section_corpus_oracle --release \
//!     -- --ignored --nocapture
//! SECTION_ORACLE_STRIDE=1 cargo test -p test-harness \
//!     --test section_corpus_oracle --release -- --ignored --nocapture
//! ```
//!
//! ## What §5.3 asks for, and what is asserted instead
//!
//! §5.3 states the section oracle as "the cap area must equal the area of the
//! stencil-cap polygon the app computes today for the same plane". That
//! comparison cannot be made from here and should not be: the app's stencil cap
//! is a RENDER artifact — a screen-space stencil pass over the tessellation,
//! inside the Svelte/three.js half of the tree, with no numeric area to read
//! and no way to invoke it from a Rust test. Measuring the kernel against a
//! rasteriser's silhouette would also be the wrong direction of trust: the
//! stencil is an approximation of the cap, not the other way round.
//!
//! So the corpus half asserts the properties a cap must have, each of which is
//! independent of the implementation that produced it:
//!
//! 1. **The cap area is bounded by the AABB's cross-section.** The solid lies
//!    inside its own (conservative) axis-aligned box, so a planar section of
//!    the solid lies inside the same plane's section of that box — which for an
//!    axis-aligned cut plane is exactly the product of the other two extents. A
//!    conservative bound, so the inequality is a PROOF; a cap that escapes it is
//!    not a section of this solid. This is the check that would have caught the
//!    deviation-N69 class (an `Intersect` answering with a copy of an operand)
//!    had the containment net inside `section_with_plane` not caught it first.
//! 2. **Every loop is closed and does not cross another loop curve.** A cap
//!    loop comes out of a 2-manifold face, so it is closed and simple BY
//!    CONSTRUCTION: measuring it is how a defect in the PROJECTION of that loop
//!    — a wrong parameter range, a mis-signed arc sense, a dropped curve — is
//!    caught, because the topology would still look right. Measured with
//!    `kernel_v2::projection::section::loop_defects`, which runs D1c's own
//!    crossing search so the oracle and the visibility split cannot disagree
//!    about whether two curves meet.
//! 3. **An outer loop's signed area is positive and a hole's negative**, and
//!    the net is positive: a cap with more hole than material is not a cap.
//! 4. **The cut solid projects.** Handing it back is the whole point (§5.2:
//!    "the caller projects the cut solid with D1a–c"), so a cut body that
//!    cannot be projected is a failure of this increment and not of D1a.
//!
//! ## Declines are counted, by kind
//!
//! Every outcome that is NOT an asserted cap is tallied and printed: a case the
//! kernel cannot build (the assay's own business), a solid it cannot bound, a
//! typed capability refusal (the Stage-0 coplanar wall, a curved partial-patch
//! operand that cannot re-enter yang Stage 1), any other pipeline STOP, an empty
//! cap, and the §4.5.5 Stage-0 shared-cap path. Counted rather than skipped for
//! the reason D1c put its declines on the contract: a regression that started
//! declining everything would otherwise look like a clean sweep.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kernel_v2::projection::section::loop_defects;
use test_harness::workflow::ModelBuilder;
use waffle_types::kernel::projection::{ProjectOpts, ViewFrame};

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

/// How many cases to skip between samples.
///
/// The sweep's cost is the ASSAY's cost plus three booleans per live body:
/// rebuilding a corpus case is already minutes for the heaviest chained-boolean
/// stacks, and each section runs a real Intersect on top. The default therefore
/// STRIDE-SAMPLES, the same shape as `projection_corpus_oracle`, and the
/// exhaustive sweep is one environment variable away: `SECTION_ORACLE_STRIDE=1`.
/// The sample is deterministic (sorted ids, a fixed stride), so a failure is
/// reproducible by id.
///
/// The default is COARSER than the projection oracle's 8, and measurement is
/// why: that sweep does one rebuild and six projections per case and ran 42
/// cases in 263 s, while this one does a rebuild and three real BOOLEANS per
/// live body. Measured 2026-10-03 in `--release` at stride 64 (the sample is
/// C0001, C0065, F0011, F0075, R0021, R0085): **6 cases, 305 s and 340 s over
/// two runs**, 21
/// `(body, axis)` cuts — 16 capped and asserted, 3 not boundable (a body
/// carrying a surface-pair or hyperbola edge, which `solid_aabb` declines to
/// bound), 1 other pipeline STOP (C0065's torus patch UV-CDT, the standing
/// KV9-F2 family), 1 empty cap, 1 through the §4.5.5 Stage-0 shared cap, 0
/// sampled loops, 0 cases not built, 0 failures. The fullest cap fills
/// 1.000000 of its AABB cross-section (C0001 along `x` — a prismatic body, so
/// the containment bound is TIGHT there, which is what makes it a check and
/// not a formality).
fn stride() -> usize {
    std::env::var("SECTION_ORACLE_STRIDE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(64)
}

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

#[derive(Default)]
struct Tally {
    /// Cases whose document would not rebuild — the assay's business.
    not_built: Vec<String>,
    /// `(body, axis)` triples attempted.
    attempted: usize,
    /// …that came back with a non-empty cap and were fully asserted.
    capped: usize,
    /// …whose solid the kernel could not BOUND (a surface-pair or hyperbola
    /// edge), so there is no scale to derive the box's margin from.
    unbounded: usize,
    /// …refused as a typed capability wall: the Stage-0 coplanar residue, or a
    /// curved partial-patch operand that cannot re-enter yang Stage 1.
    not_supported: usize,
    /// …refused by any other pipeline STOP (a Stage-3/4/5 wall, a containment
    /// net). Each is named in the report.
    boolean_stop: usize,
    /// …that produced NO cap: the mid-plane of the conservative AABB can lie
    /// off a thin or non-convex body's real middle.
    empty_cap: usize,
    /// …whose cap came back through the §4.5.5 Stage-0 overlay, attributed to
    /// the model rather than to the cutting box.
    shared_with_model: usize,
    /// …whose cap carried a sampled (non-analytic) loop, so its area is low by
    /// that polyline's sagitta deficit.
    inexact_cap: usize,
    /// Worst (cap area) / (AABB cross-section) ratio seen, and where. Always
    /// ≤ 1 by the containment proof; a solid that FILLS its box reaches 1.
    worst_fill: f64,
    worst_fill_at: String,
    /// The distinct STOP texts with the `(case, axis)` that hit each, so the
    /// report NAMES the walls rather than counting them: a decline nobody can
    /// attribute is a decline nobody will convert.
    stops: BTreeMap<String, Vec<String>>,
    /// Failures, by case id.
    failures: BTreeMap<String, Vec<String>>,
}

#[test]
#[ignore = "corpus-wide: rebuilds every assay case and runs three booleans per body; \
            run with --release --ignored (minutes)"]
fn the_section_oracle_holds_over_the_whole_assay_corpus() {
    // Every tolerance is RELATIVE to the case's own extent: the corpus spans
    // metres to millimetres and an absolute slack is meaningless at both ends.
    const REL_SLACK: f64 = 1e-9;
    /// The kernel's canonical relative chord tolerance — the band a cap edge
    /// that could not stay analytic carries.
    const CHORD_REL: f64 = 1e-3;

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
        "section oracle: {} of {} corpus cases (stride {stride}; \
         SECTION_ORACLE_STRIDE=1 for all of them)",
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

        let mut problems = Vec::new();
        for h in &handles {
            // Each body is cut at its OWN box's middle, and bounded by its own
            // box's cross-section. A union box would make the bound looser for
            // no reason.
            let Some((lo, hi)) = builder.kernel_mut().as_introspect().solid_aabb(h) else {
                // The section declines these by name; count the three axes it
                // would have been asked about and move on.
                tally.attempted += 3;
                tally.unbounded += 3;
                continue;
            };
            let mid = [
                0.5 * (lo[0] + hi[0]),
                0.5 * (lo[1] + hi[1]),
                0.5 * (lo[2] + hi[2]),
            ];
            let extent = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            let diag = (extent[0] * extent[0] + extent[1] * extent[1] + extent[2] * extent[2])
                .sqrt()
                .max(f64::MIN_POSITIVE);

            for (k, name) in [(0usize, "x"), (1, "y"), (2, "z")] {
                let mut normal = [0.0; 3];
                normal[k] = 1.0;
                // The AABB's own cross-section at an axis-aligned plane: the
                // product of the other two extents. The solid is inside the
                // box, so its section is inside the box's section — a
                // containment PROOF, and conservative because `solid_aabb` is.
                let cross = extent[(k + 1) % 3] * extent[(k + 2) % 3];
                tally.attempted += 1;

                let cut = match builder.kernel_mut().section_with_plane(h, mid, normal) {
                    Ok(cut) => cut,
                    Err(waffle_types::kernel::KernelError::NotSupported { operation }) => {
                        tally.not_supported += 1;
                        tally
                            .stops
                            .entry(operation)
                            .or_default()
                            .push(format!("{id} {name}"));
                        continue;
                    }
                    Err(e) => {
                        tally.boolean_stop += 1;
                        tally
                            .stops
                            .entry(format!("{e}"))
                            .or_default()
                            .push(format!("{id} {name}"));
                        continue;
                    }
                };

                if cut.cap_shared_with_model {
                    tally.shared_with_model += 1;
                }
                if cut.cap_loops.is_empty() {
                    tally.empty_cap += 1;
                    continue;
                }
                if !cut.exact() {
                    tally.inexact_cap += 1;
                }
                tally.capped += 1;

                // 1. The containment bound. The slack is the chord band of the
                //    cap's own linear size where a loop is sampled, and float
                //    noise where every loop is analytic: a sampled cap edge
                //    rides a chord polyline INSIDE the true curve, so its area
                //    is low rather than high and the inequality is only ever
                //    helped — but a boolean-output face's boundary is itself a
                //    chord approximation of its intersection curve, which can
                //    sit a sagitta OUTSIDE the exact solid.
                let band = if cut.exact() { REL_SLACK } else { CHORD_REL };
                let net = cut.cap_area();
                if net > cross + band * diag * diag {
                    problems.push(format!(
                        "{name}: cap area {net:e} exceeds the AABB cross-section {cross:e} \
                         (extent {extent:?}) — this is not a section of this solid"
                    ));
                }
                if cross > 0.0 {
                    let fill = net / cross;
                    if fill > tally.worst_fill {
                        tally.worst_fill = fill;
                        tally.worst_fill_at = format!("{id} {name}");
                    }
                }

                // 3. Signs: at least one outer loop, a positive net.
                if net <= 0.0 {
                    problems.push(format!(
                        "{name}: net cap area {net:e} is not positive — more hole than material"
                    ));
                }
                if !cut.cap_loops.iter().any(|l| l.signed_area > 0.0) {
                    problems.push(format!("{name}: no cap loop has a positive area"));
                }

                // 2. Closure and simplicity, per loop.
                for (li, l) in cut.cap_loops.iter().enumerate() {
                    let d = loop_defects(&l.curves, CHORD_REL * diag);
                    if d.unmatched_ends > 0 {
                        problems.push(format!(
                            "{name}: cap loop {li} ({} curve(s), exact={}) has {} endpoint(s) \
                             with no partner — the chain is OPEN",
                            l.curves.len(),
                            l.exact,
                            d.unmatched_ends
                        ));
                    }
                    if d.budget_exhausted {
                        problems.push(format!(
                            "{name}: cap loop {li} ({} curve(s)) exhausted the crossing \
                             search's budget — its simplicity was not measured",
                            l.curves.len()
                        ));
                    }
                    if d.self_crossings > 0 {
                        problems.push(format!(
                            "{name}: cap loop {li} ({} curve(s), exact={}) crosses itself {} \
                             time(s) away from a shared endpoint ({} tangential contact(s) \
                             declined)",
                            l.curves.len(),
                            l.exact,
                            d.self_crossings,
                            d.tangency_declines
                        ));
                    }
                }

                // 4. The cut solid projects — the reason it is handed back.
                let Some(body) = cut.cut_solid else {
                    problems.push(format!(
                        "{name}: a non-empty cap with no cut solid to project"
                    ));
                    continue;
                };
                match builder.kernel_mut().project(
                    &body,
                    &ViewFrame::looking_along(normal),
                    &ProjectOpts::default(),
                ) {
                    Err(e) => problems.push(format!("{name}: the cut solid will not project: {e}")),
                    Ok(view) if view.curves.is_empty() => {
                        problems.push(format!("{name}: the cut solid projected no curves"));
                    }
                    Ok(_) => {}
                }
            }
        }
        if !problems.is_empty() {
            tally.failures.insert(id.clone(), problems);
        }
    }

    println!(
        "section oracle over {} cases: {} (body, axis) cuts attempted, {} capped and \
         asserted, {} not boundable, {} typed capability refusals, {} other pipeline \
         STOPs, {} empty caps, {} through the Stage-0 shared cap, {} with a sampled \
         (inexact) loop; {} cases not built (the assay's own business); the fullest \
         cap fills {:.6} of its AABB cross-section ({}); {} failing cases",
        ids.len(),
        tally.attempted,
        tally.capped,
        tally.unbounded,
        tally.not_supported,
        tally.boolean_stop,
        tally.empty_cap,
        tally.shared_with_model,
        tally.inexact_cap,
        tally.not_built.len(),
        tally.worst_fill,
        if tally.worst_fill_at.is_empty() {
            "none"
        } else {
            &tally.worst_fill_at
        },
        tally.failures.len()
    );
    if !tally.not_built.is_empty() {
        println!("not built: {}", tally.not_built.join(", "));
    }
    for (what, who) in &tally.stops {
        println!("declined ×{} at [{}]: {what}", who.len(), who.join(", "));
    }
    for (id, problems) in &tally.failures {
        println!("FAIL {id}:");
        for p in problems {
            println!("    {p}");
        }
    }

    // A sweep that capped nothing would pass vacuously, which is the one way
    // this oracle could lie: every assertion above lives inside the capped
    // branch.
    assert!(
        tally.capped * 3 > tally.attempted,
        "only {} of {} (body, axis) cuts produced a cap — the sweep proved nothing",
        tally.capped,
        tally.attempted
    );
    // And the declines are PINNED as a fraction, not merely printed. Each kind
    // is an honest outcome on its own — a body the kernel cannot bound, a wall
    // it names — but a regression that turned most cuts into one of them would
    // leave every assertion above running on a handful of cases while the sweep
    // still passed. Measured at the default stride: 5 of 21.
    let declined = tally.unbounded + tally.not_supported + tally.boolean_stop + tally.empty_cap;
    assert!(
        declined * 2 < tally.attempted,
        "{declined} of {} (body, axis) cuts declined ({} unboundable, {} typed walls, \
         {} STOPs, {} empty) — most of the sweep is no longer measuring a cap",
        tally.attempted,
        tally.unbounded,
        tally.not_supported,
        tally.boolean_stop,
        tally.empty_cap
    );
    assert!(
        tally.failures.is_empty(),
        "{} case(s) fail the section oracle (listed above)",
        tally.failures.len()
    );
}

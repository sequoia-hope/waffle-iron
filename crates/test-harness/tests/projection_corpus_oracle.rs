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
//! ## What is asserted — the §5.3 bbox equality, SANDWICHED
//!
//! For every case that rebuilds, and every one of the six axis directions:
//!
//! 1. **The projected bbox lies inside the AABB's projection.** The upper
//!    half of §5.3's equality. (The kernel's `solid_aabb` answers `None` for a
//!    solid carrying a surface-pair curve, so some cases are not boundable at
//!    all — counted and reported, never silently skipped.)
//! 2. **The projected bbox CONTAINS the render tessellation's projection.**
//!    The lower half, and new at D1b. The render mesh is INSCRIBED — every
//!    vertex lies on the solid's boundary — so its box is a sound lower bound
//!    on the solid's own, and the projection must contain it. At D1a a curved
//!    solid FAILED this by its whole radial bulge: a cylinder's projected
//!    edges reach `±R` only on the two rims, while the mesh reaches it
//!    everywhere. It is the silhouette that closes the gap, so this is the
//!    check D1b exists to pass. The slack is the CHORD BAND of the view's own
//!    size, because a boolean-output face is bounded by a chord polyline
//!    approximating its true intersection curve and a mesh vertex on such a
//!    boundary can sit a sagitta outside the exact solid.
//!
//!    Together, 1 and 2 pin the projected bbox between two independent
//!    bounds. Where the AABB is TIGHT the sandwich IS §5.3's literal
//!    equality; where the AABB is conservative — it bounds a circular edge by
//!    the box of its whole circle, and a torus by the cube
//!    `centre ± (R + r)` — the tessellation still pins the projection from
//!    below. Which it is, is MEASURED rather than assumed: a pair whose AABB
//!    and tessellation boxes agree has both bounds tight, so §5.3's literal
//!    equality is asserted there, beside the same count for the EDGE curves
//!    alone (what D1a reached, computed in the same pass); a pair where they
//!    do not is a typed decline (`aabb_conservative`), counted and reported
//!    rather than asserted away.
//! 3. **Total visible length is invariant under a 180° rotation about the
//!    view axis.** That rotation is an isometry of the view plane, so every
//!    analytic reconstruction the projection performs — the ellipse's
//!    principal axes and parameter range, the circular and edge-on special
//!    cases, the silhouette circle of a sphere — must come out the same
//!    length, or the reconstruction is wrong.
//!
//! A case the KERNEL cannot build is not a projection failure: those are the
//! assay's own business (`assay_kv2.rs` scores them) and are counted here as
//! `not_built`. A projection that FAILS on a solid the kernel did build is a
//! failure of this oracle, and is reported with the case id.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use test_harness::workflow::ModelBuilder;
use waffle_types::kernel::projection::{Aabb2, CurveKind, ProjectOpts, ViewBasis, ViewFrame};
use waffle_types::kernel::{ProjectionBody, Visibility};

const CORPUS: &str = "../../app/tests/cases/assay";

/// The kernel's canonical relative chord tolerance — the density both the
/// render mesh and a default-options projection use, so the two agree on where
/// a sampled curve is.
const CHORD_REL: f64 = 1e-3;

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
/// loud-by-design C-series walls) and 36 of the 240 (case, direction) pairs
/// not boundable, which is 6 cases carrying a surface-pair curve. (240, not
/// 246: the case that did not build contributes no pair. Re-measured
/// 2026-10-03 at 433 s with the bounded tally below — 204 of 240 bounded.)
///
/// Re-measured at D1b, 2026-10-03, on the grown 334-case corpus: **42 cases,
/// 263 s**, 39 projected in all six directions, 3 not built (C0113, P0013,
/// R0007), 216 of 234 (case, direction) pairs bounded, all 234 pinned from
/// below by the render tessellation, 0 failures. The AABB is TIGHT on 136 of
/// the bounded pairs and conservative on 80; on all 136 tight ones §5.3's
/// literal bbox equality HOLDS, against 134 with the edge curves alone.
///
/// Two things are worth reading off those numbers. The equality itself moves
/// only from 134 to 136, because a tight AABB and a curved extreme rarely
/// coincide: most corpus cases are prismatic outlines with interior curved
/// features, whose global bbox the edges already reached. The check D1b
/// actually turns green is the TESSELLATION containment — measured on the
/// same sample projected with `project_edges` instead of `project_solid`, 2
/// of the 42 cases fail it (the ones whose outline IS a silhouette: a torus,
/// a bored revolve), and at D1b all 42 pass.
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

fn aabb_projection(basis: &ViewBasis, lo: [f64; 3], hi: [f64; 3]) -> Aabb2 {
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

/// The projection of a render mesh's vertices — a LOWER bound on the projected
/// solid, since every mesh vertex lies on the solid's boundary (up to the chord
/// band on a boolean output's chord-polyline boundary, which is what the
/// caller's slack carries).
fn mesh_projection(basis: &ViewBasis, mesh: &waffle_types::kernel::RenderMesh) -> Option<Aabb2> {
    let mut bb: Option<Aabb2> = None;
    for v in mesh.vertices.chunks_exact(3) {
        let (uv, _) = basis.project([f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]);
        bb = Some(match bb {
            None => Aabb2::point(uv),
            Some(b) => b.united_point(uv),
        });
    }
    bb
}

/// Whether two boxes agree on all four sides within `slack`.
fn boxes_agree(a: &Aabb2, b: &Aabb2, slack: f64) -> bool {
    (a.min.x() - b.min.x()).abs() <= slack
        && (a.min.y() - b.min.y()).abs() <= slack
        && (a.max.x() - b.max.x()).abs() <= slack
        && (a.max.y() - b.max.y()).abs() <= slack
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
    /// `(case, direction)` whose projected bbox was pinned from BELOW by the
    /// render tessellation — check 2, the D1b half.
    tess_pinned: usize,
    /// Pairs where the AABB is TIGHT — its projection agrees with the
    /// tessellation's, so §5.3's equality is a statement that can be made at
    /// all there. (Both bounds sandwich the solid, so agreeing means both are
    /// tight.) The complement is `aabb_conservative`.
    aabb_tight: usize,
    /// And the pairs where the AABB is measurably conservative (a circular
    /// edge's whole-circle box, a torus's cube), so §5.3's equality cannot be
    /// asserted against it. The typed decline.
    aabb_conservative: usize,
    /// Of the `aabb_tight` pairs, how many the FULL projection's bbox
    /// actually equals the AABB's projection on all four sides — §5.3's
    /// literal claim, asserted.
    equality: usize,
    /// And how many the EDGE-only bbox equals — what D1a reached on the same
    /// sample, computed in the same pass from the `CurveKind::Edge` curves, so
    /// the before/after is one measurement and not two runs.
    equality_edges_only: usize,
    /// `(case, direction)` pairs where one of the two half-turn views reported
    /// no VISIBLE curve at all — every curve hidden. Reported, not asserted:
    /// it is a statement about D1c's split and the §5.3 visibility oracle is
    /// what judges that.
    no_visible_curve: usize,
    /// Worst relative disagreement between the two half-turn views' VISIBLE
    /// length, and where.
    worst_visible_swing: f64,
    worst_visible_swing_at: String,
    /// And over BOTH visibilities, which the coincidence merge also moves.
    worst_total_swing: f64,
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
            // The view's own size, and the same floored at 1 for the
            // AABB-containment slack (which `REL_SLACK` makes a float-noise
            // band, not a geometric one).
            let size = (got.max.x() - got.min.x())
                .abs()
                .max((got.max.y() - got.min.y()).abs());
            let extent = size.max(1.0);

            // 1. Containment in the AABB's projection — the upper half.
            let aabb_box = bounds.map(|(lo, hi)| aabb_projection(&basis, lo, hi));
            if let Some(want) = aabb_box {
                tally.bounded += 1;
                if !got.within(&want, REL_SLACK * extent) {
                    problems.push(format!(
                        "{name}: projected {got:?} escapes the AABB projection {want:?}"
                    ));
                }
            } else {
                tally.unbounded += 1;
            }

            // The bbox D1a would have reported: the EDGE curves alone. Free
            // here, and it makes the before/after of §5.3's equality one
            // measurement instead of two runs of two different trees.
            let edges_only = view
                .curves
                .iter()
                .filter(|c| c.kind == CurveKind::Edge)
                .map(|c| c.geometry.bbox())
                .reduce(|a, b| a.united(b));

            // 2. The render tessellation's projection, contained — the LOWER
            // half, and the one only silhouettes can satisfy on a curved
            // solid.
            let mut mesh_box: Option<Aabb2> = None;
            for h in &handles {
                match builder.kernel_mut().tessellate(h, CHORD_REL) {
                    Err(e) => {
                        mesh_box = None;
                        problems.push(format!("{name}: tessellation failed: {e}"));
                        break;
                    }
                    Ok(mesh) => {
                        if let Some(b) = mesh_projection(&basis, &mesh) {
                            mesh_box = Some(match mesh_box {
                                None => b,
                                Some(prev) => prev.united(b),
                            });
                        }
                    }
                }
            }
            if let Some(inner) = mesh_box {
                tally.tess_pinned += 1;
                // The CHORD BAND of the view's own size. The mesh is
                // inscribed in the solid's analytic surfaces, but a
                // boolean-output face is bounded by a chord polyline that
                // approximates its true intersection curve, so a mesh vertex
                // on such a boundary can sit a sagitta OUTSIDE the exact
                // solid — the documented propagation of the render band
                // (`memory` `yang_chord_band_propagates_into_section_metric`).
                // Measured 2026-10-03: the worst over the stride-8 sample is
                // R0015 at 1.9e-8 on a 1.7e-4 view (1.1e-4 relative), which
                // an f32-ulp slack would have called a failure and a 1e-3
                // band calls what it is.
                let slack = CHORD_REL * size;
                if !inner.within(&got, slack) {
                    problems.push(format!(
                        "{name}: the render mesh's projection {inner:?} escapes the projected \
                         bbox {got:?} — a silhouette is missing"
                    ));
                }
                // Where the AABB's projection and the tessellation's AGREE,
                // both sandwich bounds are tight and §5.3's equality is a
                // claim that can be made at all — so it is ASSERTED there,
                // and the edge-only bbox is counted beside it so the report
                // says what the silhouettes bought.
                if let Some(outer) = aabb_box {
                    if boxes_agree(&outer, &inner, slack) {
                        tally.aabb_tight += 1;
                        if boxes_agree(&got, &outer, slack) {
                            tally.equality += 1;
                        } else {
                            problems.push(format!(
                                "{name}: the AABB projection {outer:?} is TIGHT (the \
                                 tessellation reaches it) and the projected bbox {got:?} \
                                 does not equal it"
                            ));
                        }
                        if edges_only.is_some_and(|e| boxes_agree(&e, &outer, slack)) {
                            tally.equality_edges_only += 1;
                        }
                    } else {
                        tally.aabb_conservative += 1;
                    }
                }
            }

            // 3. The half-turn SYMMETRY of the projected extremes.
            //
            //    A half turn about the view axis maps `(u, v)` to `(−u, −v)`,
            //    so the turned view's bounding box must be the negation of the
            //    original's. Which is what this check was always for: every
            //    analytic reconstruction the projection performs — the
            //    ellipse's principal axes and parameter range, the circular
            //    and edge-on special cases, a sphere's silhouette circle —
            //    reaches its own extremes, and a mistake in any of them moves
            //    one.
            //
            //    It used to be stated as the invariance of the total VISIBLE
            //    length, and that worked only because D1a and D1b tagged every
            //    curve visible, which made it the length of the whole point
            //    set. D1c breaks it twice over, and neither break is a defect:
            //    the visible SUBSET is not symmetric under a coordinate
            //    negation, because the crossing roots and the coincidence
            //    sweep's `u` ordering are computed on negated coordinates and a
            //    marginal piece changes side; and the total over both
            //    visibilities is not either, because the coincidence MERGE
            //    drops a curve that reproduces another and the near-coincidence
            //    decision flips the same way. Measured over this sample: the
            //    total swings by up to 1.4 % (P0005) and the visible subset by
            //    13 % (F0043), with one direction of C0065 reporting no visible
            //    curve at all.
            //
            //    The BBOX has neither problem. A dropped duplicate's points are
            //    also in the curve that kept it, and a split tiles its parent,
            //    so the extremes are exactly what they were. The lengths are
            //    MEASURED and printed instead, and what judges the split itself
            //    is `projection_visibility_oracle`, against the surface rather
            //    than against a rotation of itself.
            let turned = half_turned(&frame);
            match builder.kernel_mut().project_bodies(&bodies, &turned, &opts) {
                Err(e) => problems.push(format!("{name}: the half turn failed: {e}")),
                Ok(other) => {
                    let Some(mirrored) = other.bbox else {
                        problems.push(format!("{name}: the half turn projected no curves"));
                        continue;
                    };
                    let want = Aabb2 {
                        min: cad_primitives::Point2::new(-mirrored.max.x(), -mirrored.max.y()),
                        max: cad_primitives::Point2::new(-mirrored.min.x(), -mirrored.min.y()),
                    };
                    if !boxes_agree(&got, &want, REL_SLACK * extent) {
                        problems.push(format!(
                            "{name}: the projected bbox {got:?} is not the negation of the \
                             half-turned view's {mirrored:?}"
                        ));
                    }
                    // The lengths, measured rather than asserted (see above).
                    let whole = |v: &waffle_types::kernel::projection::ViewGeometry| {
                        v.total_length(Visibility::Visible) + v.total_length(Visibility::Hidden)
                    };
                    let (a, b) = (whole(&view), whole(&other));
                    if a <= 0.0 {
                        problems.push(format!("{name}: zero projected length"));
                    } else {
                        let swing = (a - b).abs() / a.max(b);
                        if swing > tally.worst_total_swing {
                            tally.worst_total_swing = swing;
                        }
                    }
                    let (va, vb) = (
                        view.total_length(Visibility::Visible),
                        other.total_length(Visibility::Visible),
                    );
                    if va <= 0.0 || vb <= 0.0 {
                        tally.no_visible_curve += 1;
                    } else {
                        let swing = (va - vb).abs() / va.max(vb);
                        if swing > tally.worst_visible_swing {
                            tally.worst_visible_swing = swing;
                            tally.worst_visible_swing_at = format!("{id} {name}");
                        }
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
         bounded and containment-checked, {} pinned from below by the render \
         tessellation; the AABB is TIGHT on {} of them (conservative on {}), and \
         there §5.3's literal bbox equality holds for {} — against {} with the \
         EDGE curves alone, which is what D1a reached; the visible subset of \
         the half-turn pair swings in length by at most {:.3e} ({}) and the \
         whole point set by {:.3e}, with {} pair(s) reporting no visible curve \
         at all; {} failing cases",
        ids.len(),
        tally.projected,
        tally.not_built.len(),
        tally.bounded,
        tally.bounded + tally.unbounded,
        tally.tess_pinned,
        tally.aabb_tight,
        tally.aabb_conservative,
        tally.equality,
        tally.equality_edges_only,
        tally.worst_visible_swing,
        if tally.worst_visible_swing_at.is_empty() {
            "none"
        } else {
            &tally.worst_visible_swing_at
        },
        tally.worst_total_swing,
        tally.no_visible_curve,
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
    // measured 2026-10-03 at stride 8: 204 of 240 pairs bounded, 36 not.
    assert!(
        tally.bounded > tally.unbounded,
        "only {} of {} (case, direction) pairs could be bounded — the \
         containment half of the oracle barely ran",
        tally.bounded,
        tally.bounded + tally.unbounded
    );
    // And so would a sweep where the D1b half never ran. The tessellation pin
    // is the check that a silhouette is present at all, and unlike the AABB it
    // is available for EVERY solid the kernel can tessellate, so it must cover
    // every pair that got as far as a projection.
    assert!(
        tally.tess_pinned >= tally.bounded + tally.unbounded,
        "only {} of {} (case, direction) pairs were pinned from below by the \
         render tessellation — the D1b half of the oracle barely ran",
        tally.tess_pinned,
        tally.bounded + tally.unbounded
    );
    // And so would a sweep with no TIGHT-AABB pair left: §5.3's literal
    // equality is asserted only there, so a regression that loosened every
    // AABB would turn that assertion vacuous.
    assert!(
        tally.aabb_tight * 4 >= tally.bounded,
        "only {} of {} bounded pairs have a tight AABB — §5.3's literal \
         equality is barely asserted anywhere",
        tally.aabb_tight,
        tally.bounded
    );
    assert!(
        tally.failures.is_empty(),
        "{} case(s) fail the projection oracle (listed above)",
        tally.failures.len()
    );
}

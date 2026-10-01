//! SI5: analytic STEP fixtures, and the export → extract round trip.
//!
//! `specs/step_import_si5_exact_analytic_ingestion.md` checkpoint C2.
//!
//! **Why the fixtures are generated here.** step-import's committed fixtures
//! were written by truck's own `out` module, which emits `SURFACE_OF_REVOLUTION`
//! and b-spline curves — so `tests/fixtures/cylinder.step` contains no
//! `CYLINDRICAL_SURFACE` at all and cannot exercise the analytic path (spec
//! §4.3). Our own `kernel_v2::step_export` writes analytic AP214 with all five
//! elementary surfaces, so it can produce license-clean fixtures that actually
//! contain the entities SI5 extracts.
//!
//! kernel-v2 may not depend on step-import and vice versa, so test-harness —
//! the cross-crate integration crate — is where both are visible.
//!
//! The fixtures are committed and this test is their golden check: it rebuilds
//! each one and compares byte-for-byte, so a change in the exporter cannot
//! silently drift the inputs SI5 is tested against. `write_step` output is
//! deterministic (no timestamp in the header), which is what makes that sound.
//!
//! Regenerate deliberately:
//!     UPDATE_SI5_FIXTURES=1 cargo test -p test-harness --test si5_analytic

use std::f64::consts::PI;
use std::path::PathBuf;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, revolve, write_step, BrepArena, Profile, SolidId, StepSolid};
use waffle_types::kernel::RigidPlacement;

/// Fixtures live beside step-import's truck-written ones, in their own
/// directory so the provenance difference stays visible.
fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../step-import/tests/fixtures/analytic"
    ))
}

fn fixture_path(name: &str) -> PathBuf {
    fixture_dir().join(name)
}

// Millimetre-scale geometry in metres, so the exported (millimetre) numbers
// come out round and a human can read the fixture.
const R: f64 = 0.005; // 5 mm
const H: f64 = 0.012; // 12 mm

/// Right cylinder: CYLINDRICAL_SURFACE lateral in the 4-edge CCLL form (two
/// CIRCLE rims + the LINE seam twice) plus two PLANE caps.
fn cylinder(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        R,
    )
    .expect("circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
        .expect("cylinder extrude")
        .solid
}

/// Solid cone from an on-axis apex-triangle full-turn revolve: a
/// CONICAL_SURFACE lateral over a single CIRCLE base rim, the apex a singular
/// surface point.
fn cone(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(R, 0.0),
            Point2::new(0.0, H),
        ],
        vec![],
    )
    .expect("apex triangle profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .expect("full-turn apex cone")
    .solid
}

/// Closed sphere (V=2, E=1, F=1): one SPHERICAL_SURFACE, one meridian.
fn sphere(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        R,
    )
    .expect("on-axis circle profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed sphere revolve")
    .solid
}

/// Closed ring torus (V=1, E=2, F=1, genus 1): one TOROIDAL_SURFACE.
fn torus(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 3.0 * R),
        R,
    )
    .expect("off-axis circle profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed torus revolve")
    .solid
}

/// Block with a through bore: PLANE faces carrying RING loops (FACE_BOUND)
/// plus a CYLINDRICAL_SURFACE bore wall. 79 % of the corpus has inner loops,
/// so this is the fixture for that.
fn drilled_block(arena: &mut BrepArena) -> SolidId {
    let side = 0.020;
    let block = {
        let profile = Profile::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(-side / 2.0, -side / 2.0),
                Point2::new(side / 2.0, -side / 2.0),
                Point2::new(side / 2.0, side / 2.0),
                Point2::new(-side / 2.0, side / 2.0),
            ],
            vec![],
        )
        .expect("square profile");
        extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
            .expect("block extrude")
            .solid
    };
    // A drill that pokes out both ends, so the bore is a through hole and both
    // caps get a ring rather than one cap keeping a disc.
    let drill = {
        let profile = Profile::circle(
            Point3::new(0.0, 0.0, -H),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Point2::new(0.0, 0.0),
            R / 2.0,
        )
        .expect("drill profile");
        extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), 3.0 * H)
            .expect("drill extrude")
            .solid
    };
    boolean_op(arena, block, drill, BoolOp::Subtract).expect("through bore")
}

fn export(name: &str, build: Build) -> String {
    let mut arena = BrepArena::new();
    let solid = build(&mut arena);
    write_step(
        &arena,
        &[StepSolid {
            solid,
            name: name.to_string(),
            placement: RigidPlacement::IDENTITY,
        }],
        &format!("{name}.step"),
    )
    .unwrap_or_else(|e| panic!("{name} export failed: {e:?}"))
}

/// A fixture's builder: fills a fresh arena and returns the solid to export.
type Build = fn(&mut BrepArena) -> SolidId;

const FIXTURES: &[(&str, Build)] = &[
    ("cylinder", cylinder),
    ("cone", cone),
    ("sphere", sphere),
    ("torus", torus),
    ("drilled_block", drilled_block),
];

#[test]
fn analytic_fixtures_match_the_exporter() {
    let update = std::env::var("UPDATE_SI5_FIXTURES").is_ok();
    if update {
        std::fs::create_dir_all(fixture_dir()).expect("fixture directory");
    }

    let mut drift = Vec::new();
    for (name, build) in FIXTURES {
        let text = export(name, *build);
        let path = fixture_path(&format!("{name}.step"));
        if update {
            std::fs::write(&path, &text).expect("write fixture");
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(committed) if committed == text => {}
            Ok(_) => drift.push(format!(
                "{name}: committed fixture differs from the exporter"
            )),
            Err(e) => drift.push(format!("{name}: {e} ({})", path.display())),
        }
    }
    assert!(
        drift.is_empty(),
        "analytic fixtures are stale — regenerate deliberately with \
         UPDATE_SI5_FIXTURES=1 and review the diff:\n  {}",
        drift.join("\n  ")
    );
}

/// The fixtures exist to carry entities the truck-written ones do not. If this
/// ever stops holding, the fixtures have lost their purpose and the analytic
/// tests downstream are testing nothing.
#[test]
fn analytic_fixtures_carry_the_entities_si5_extracts() {
    let want: &[(&str, &[&str])] = &[
        (
            "cylinder",
            &["CYLINDRICAL_SURFACE", "PLANE", "CIRCLE", "LINE"],
        ),
        ("cone", &["CONICAL_SURFACE", "CIRCLE"]),
        ("sphere", &["SPHERICAL_SURFACE"]),
        ("torus", &["TOROIDAL_SURFACE"]),
        (
            "drilled_block",
            &["CYLINDRICAL_SURFACE", "PLANE", "FACE_BOUND"],
        ),
    ];
    for (name, entities) in want {
        let text = std::fs::read_to_string(fixture_path(&format!("{name}.step")))
            .unwrap_or_else(|e| panic!("{name}.step: {e} — regenerate with UPDATE_SI5_FIXTURES=1"));
        for entity in *entities {
            assert!(text.contains(entity), "{name}.step must contain {entity}");
        }
        for banned in [
            "B_SPLINE",
            "SURFACE_OF_REVOLUTION",
            "SURFACE_OF_LINEAR_EXTRUSION",
        ] {
            assert!(
                !text.contains(banned),
                "{name}.step must be free of {banned} — it is the ANALYTIC fixture"
            );
        }
    }
}

// =========================================================================
// Corpus cross-check (#[ignore]d — needs the ABC corpus)
// =========================================================================

/// Does the EXTRACTOR's eligibility agree with the text census's?
///
/// `scripts/si5_census.py` gates on the raw exchange file; `extract_analytic`
/// gates on what truck actually parsed. They are independent measurements of
/// the same quantity, so a disagreement localizes a bug in one of them — this
/// is the cheapest external-coherence check available for C2, and it is how
/// the census's own two scanner bugs were caught (spec §2.4).
///
///     ABC_DIR=/tmp/abc/chunk0000 ABC_N=300 \
///       cargo test -p test-harness --test si5_analytic --release \
///       -- --ignored --nocapture extractor_eligibility
#[test]
#[ignore = "corpus: needs ABC_DIR (scripts/fetch-abc-corpus.sh)"]
fn extractor_eligibility_against_the_text_census() {
    use std::collections::BTreeMap;

    let Ok(dir) = std::env::var("ABC_DIR") else {
        eprintln!("ABC_DIR unset — skipping");
        return;
    };
    let n: usize = std::env::var("ABC_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    // Real STEP has a 540 MB tail and truck's import is not resource-bounded
    // (41 GB RSS measured on one model, 2026-09-30), so cap by size.
    let max_bytes: u64 = std::env::var("ABC_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);

    // The census's gate, as a text predicate — kept deliberately identical to
    // scripts/si5_census.py: TOKENS, not `= NAME(` heads, because a rational
    // b-spline appears only inside a complex instance.
    let census_eligible = |t: &str| {
        const OUT_OF_VOCAB: &[&str] = &[
            "B_SPLINE_SURFACE",
            "RATIONAL_B_SPLINE_SURFACE",
            "BEZIER_SURFACE",
            "SURFACE_OF_REVOLUTION",
            "SURFACE_OF_LINEAR_EXTRUSION",
            "OFFSET_SURFACE",
            "CURVE_BOUNDED_SURFACE",
            "RECTANGULAR_TRIMMED_SURFACE",
            "B_SPLINE_CURVE",
            "RATIONAL_B_SPLINE_CURVE",
            "BEZIER_CURVE",
            "TRIMMED_CURVE",
            "COMPOSITE_CURVE",
            "POLYLINE",
            "PARABOLA",
            "HYPERBOLA",
        ];
        !OUT_OF_VOCAB.iter().any(|e| t.contains(e))
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![PathBuf::from(&dir)];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "step") {
                files.push(p);
            }
        }
    }
    files.sort();

    let mut census_yes_extract_yes = 0usize;
    let mut census_yes_extract_no = 0usize;
    let mut census_no_extract_yes = 0usize;
    let mut census_no_extract_no = 0usize;
    let mut rejections: BTreeMap<String, usize> = BTreeMap::new();
    let mut disagreements: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    for path in &files {
        if scanned >= n {
            break;
        }
        let Ok(md) = std::fs::metadata(path) else {
            continue;
        };
        if md.len() > max_bytes {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        scanned += 1;
        let want = census_eligible(&text);

        // truck panics rather than erroring on some real input, so this is a
        // catch_unwind boundary: a panic is a finding, not a dead run.
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            step_import::parse_step_analytic(&text, "probe")
        }));
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let (eligible, why) = match got {
            Err(_) => (false, "truck PANIC".to_string()),
            Ok(Err(e)) => (false, format!("{e}")),
            Ok(Ok(import)) => {
                let all = import.fully_eligible();
                let why = import.rejections().first().cloned().unwrap_or_default();
                (all, why)
            }
        };
        if !eligible && !why.is_empty() {
            // Collapse indices so signatures group.
            let sig: String = why
                .chars()
                .map(|c| if c.is_ascii_digit() { '#' } else { c })
                .collect();
            *rejections.entry(sig).or_default() += 1;
        }
        match (want, eligible) {
            (true, true) => census_yes_extract_yes += 1,
            (true, false) => {
                census_yes_extract_no += 1;
                if disagreements.len() < 12 {
                    disagreements.push(format!("{id}: census YES, extract NO — {why}"));
                }
            }
            (false, true) => {
                census_no_extract_yes += 1;
                if disagreements.len() < 12 {
                    disagreements.push(format!("{id}: census NO, extract YES"));
                }
            }
            (false, false) => census_no_extract_no += 1,
        }
    }
    std::panic::set_hook(prev_hook);

    let agree = census_yes_extract_yes + census_no_extract_no;
    eprintln!("\nSI5 EXTRACTOR vs TEXT CENSUS over {scanned} models");
    eprintln!("  both eligible      {census_yes_extract_yes}");
    eprintln!("  both ineligible    {census_no_extract_no}");
    eprintln!("  census YES / extract NO  {census_yes_extract_no}");
    eprintln!("  census NO  / extract YES {census_no_extract_yes}");
    eprintln!(
        "  agreement {:.1}%",
        100.0 * agree as f64 / scanned.max(1) as f64
    );
    eprintln!("\n  rejection signatures:");
    let mut sigs: Vec<_> = rejections.iter().collect();
    sigs.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (sig, count) in sigs.iter().take(15) {
        eprintln!("    x{count:<5} {sig}");
    }
    if !disagreements.is_empty() {
        eprintln!("\n  disagreements (first {}):", disagreements.len());
        for d in &disagreements {
            eprintln!("    {d}");
        }
    }
    assert!(scanned > 0, "no models scanned under {dir}");
}

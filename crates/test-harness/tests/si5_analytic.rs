//! SI5: analytic STEP fixtures, and the export → extract round trip.
//!
//! `specs/step_import_si5_exact_analytic_ingestion.md` checkpoints C2 (the
//! fixtures and the extraction) and C3 (arena ingestion of the planar tier).
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

/// The C3 beachhead shape: a plain block — six PLANE faces, twelve LINE
/// edges, no curve anywhere. 15.7 % of the ingestible corpus is polyhedral
/// like this (spec §2.1), and it is the only tier that needs neither a minted
/// seam nor a curved orientation law.
fn block(arena: &mut BrepArena) -> SolidId {
    let side = 0.020;
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
}

/// Polyhedral and genus 1: a block with a SQUARE through hole. Planar
/// throughout, so C3 ingests it — which makes it the fixture for the two
/// things a box cannot exercise: `FACE_BOUND` ring loops with the exact
/// opposite-winding rule, and the genus back-solve (`V − E + F − R = 0`,
/// since STEP states no genus at all).
fn slotted_block(arena: &mut BrepArena) -> SolidId {
    let side = 0.020;
    let hole = 0.006;
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
        vec![vec![
            Point2::new(-hole / 2.0, -hole / 2.0),
            Point2::new(-hole / 2.0, hole / 2.0),
            Point2::new(hole / 2.0, hole / 2.0),
            Point2::new(hole / 2.0, -hole / 2.0),
        ]],
    )
    .expect("holed square profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
        .expect("slotted block extrude")
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
    ("block", block),
    ("slotted_block", slotted_block),
];

/// The fixtures C3 ingests (planar vocabulary), with their builders and their
/// `(V, E, F, R, S, G)` — so an ingestion test can compare the ingested solid
/// against the constructed one it was exported from, and the comparison
/// cannot pass vacuously if both paths lose the same structure.
const PLANAR_FIXTURES: &[(&str, Build, Counts)] = &[
    ("block", block, (8, 12, 6, 0, 1, 0)),
    // Two ring loops (the hole's mouth on each cap) and genus 1, back-solved
    // from the Euler characteristic since STEP states no genus.
    ("slotted_block", slotted_block, (16, 24, 10, 2, 1, 1)),
];

/// The fixtures C3 must REFUSE, with the vocabulary member that walls each.
/// A silent partial ingest of one of these would be the whole point of SI5
/// missed, so the refusal is pinned as tightly as the acceptance.
const CURVED_FIXTURES: &[(&str, &str)] = &[
    ("cylinder", "cylindrical"),
    ("cone", "conical"),
    ("sphere", "spherical"),
    ("torus", "toroidal"),
    ("drilled_block", "cylindrical"),
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
        ("block", &["PLANE", "LINE"]),
        ("slotted_block", &["PLANE", "LINE", "FACE_BOUND"]),
    ];
    // The two planar fixtures earn their place by carrying NO curve and NO
    // curved surface — that is what makes them the C3 tier's inputs rather
    // than a second copy of `drilled_block`.
    for (name, ..) in PLANAR_FIXTURES {
        let text = std::fs::read_to_string(fixture_path(&format!("{name}.step")))
            .unwrap_or_else(|e| panic!("{name}.step: {e} — regenerate with UPDATE_SI5_FIXTURES=1"));
        for banned in [
            "CYLINDRICAL_SURFACE",
            "CONICAL_SURFACE",
            "SPHERICAL_SURFACE",
            "TOROIDAL_SURFACE",
            "CIRCLE",
        ] {
            assert!(
                !text.contains(banned),
                "{name}.step must be free of {banned} — it is the PLANAR fixture"
            );
        }
    }
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
// C3 — exact arena ingestion, planar tier
// =========================================================================

/// Parse a fixture and return its single eligible analytic shell.
fn analytic_shell(name: &str) -> waffle_types::kernel::AnalyticShellData {
    let text = std::fs::read_to_string(fixture_path(&format!("{name}.step")))
        .unwrap_or_else(|e| panic!("{name}.step: {e} — regenerate with UPDATE_SI5_FIXTURES=1"));
    let import = step_import::parse_step_analytic(&text, name)
        .unwrap_or_else(|e| panic!("{name}: analytic extraction failed: {e}"));
    assert_eq!(
        import.shells.len(),
        1,
        "{name}: expected one shell, got {:?}",
        import.shells.len()
    );
    import.shells[0]
        .clone()
        .unwrap_or_else(|e| panic!("{name}: shell is ineligible: {e}"))
}

/// `(V, E, F, R, S, G)` — a solid's element counts, for comparing two arenas.
type Counts = (usize, usize, usize, usize, usize, usize);

/// Element counts of a solid, for comparing two arenas' topology.
fn counts(arena: &BrepArena, solid: SolidId) -> Counts {
    let r = kernel_v2::validate_solid(arena, solid).expect("validates");
    (r.vertices, r.edges, r.faces, r.rings, r.shells, r.genus)
}

/// The C3 acceptance oracle, and a genuine differential one: the solid that
/// WROTE the fixture and the solid ingested back from it must agree on
/// topology exactly and on volume to the millimetre↔metre round trip of the
/// exchange file's decimals.
///
/// This is the oracle the spec's §8 list calls "round-trip through
/// `kernel_v2::step_export`" (7) joined to "volume agreement" (5), and it is
/// the strongest one available without a second kernel: the constructed solid
/// is Euler-operator built and constructor-validated, so an ingestion that
/// agrees with it on `(V, E, F, R, S, G)` and volume cannot be quietly
/// mis-assembled.
#[test]
fn planar_fixtures_ingest_and_match_the_solid_they_came_from() {
    for (name, build, want_counts) in PLANAR_FIXTURES {
        let mut built = BrepArena::new();
        let built_solid = build(&mut built);

        let mut arena = BrepArena::new();
        let shell = analytic_shell(name);
        let ingested = kernel_v2::ingest_analytic(&mut arena, &shell)
            .unwrap_or_else(|e| panic!("{name}: ingestion refused: {e:?}"));

        assert_eq!(
            counts(&arena, ingested),
            counts(&built, built_solid),
            "{name}: ingested topology differs from the solid it was exported from"
        );
        assert_eq!(
            counts(&arena, ingested),
            *want_counts,
            "{name}: (V, E, F, R, S, G) is not the shape this fixture exists to carry"
        );

        let want = kernel_v2::geom::signed_volume(&built, built_solid).expect("built volume");
        let got = kernel_v2::geom::signed_volume(&arena, ingested).expect("ingested volume");
        let rel = (got - want).abs() / want.abs();
        assert!(
            rel <= 1e-12,
            "{name}: volume {got:.17e} vs constructed {want:.17e} (rel {rel:.3e}) — \
             the only legitimate difference is the exporter's mm decimals"
        );
    }
}

/// Ingestion is a FIXED POINT: export the ingested solid, ingest that, and
/// the second pass must reproduce the first bit-for-bit (volume) and exactly
/// (topology). A sense, winding or ordering corruption that happened to
/// survive the first pass would move on the second.
#[test]
fn ingesting_an_exported_ingested_solid_is_a_fixed_point() {
    for (name, ..) in PLANAR_FIXTURES {
        let mut first = BrepArena::new();
        let a = kernel_v2::ingest_analytic(&mut first, &analytic_shell(name))
            .unwrap_or_else(|e| panic!("{name}: first ingestion refused: {e:?}"));

        let text = write_step(
            &first,
            &[StepSolid {
                solid: a,
                name: (*name).to_string(),
                placement: RigidPlacement::IDENTITY,
            }],
            &format!("{name}-reexport.step"),
        )
        .unwrap_or_else(|e| panic!("{name}: re-export failed: {e:?}"));
        let import = step_import::parse_step_analytic(&text, name)
            .unwrap_or_else(|e| panic!("{name}: re-extraction failed: {e}"));
        let shell = import.shells[0]
            .clone()
            .unwrap_or_else(|e| panic!("{name}: re-extracted shell ineligible: {e}"));

        let mut second = BrepArena::new();
        let b = kernel_v2::ingest_analytic(&mut second, &shell)
            .unwrap_or_else(|e| panic!("{name}: second ingestion refused: {e:?}"));

        assert_eq!(
            counts(&second, b),
            counts(&first, a),
            "{name}: topology drifted"
        );
        // Bitwise, deliberately: the two solids have the same faces in the
        // same order, so the volume sum has the same summation order. If this
        // ever flakes, the finding is that the EXTRACTION order moved (truck's
        // tables are HashMaps and its shell order is not canonical — spec §5.7),
        // not that the kernel lost precision.
        assert_eq!(
            kernel_v2::geom::signed_volume(&second, b).unwrap(),
            kernel_v2::geom::signed_volume(&first, a).unwrap(),
            "{name}: volume drifted across the round trip"
        );
    }
}

/// The one case the loop-sign determination cannot see on its own, pinned on
/// real parsed geometry: invert a RINGED face's sense and both signs flip, so
/// the hole reads as the perimeter and the perimeter as a hole — a swap that
/// is internally consistent and that `validate_solid` would also accept.
/// Containment closes it exactly (the face's net signed area must be
/// positive), and this is the test that proves the hole is closed rather than
/// merely commented.
#[test]
fn a_ringed_face_with_an_inverted_sense_is_refused_as_a_net_area_violation() {
    let mut shell = analytic_shell("slotted_block");
    let fi = shell
        .faces
        .iter()
        .position(|f| f.has_rings())
        .expect("slotted_block has a face with a ring");
    shell.faces[fi].same_sense = !shell.faces[fi].same_sense;
    let mut arena = BrepArena::new();
    assert_eq!(
        kernel_v2::ingest_analytic(&mut arena, &shell),
        Err(kernel_v2::KernelV2Error::InvalidAnalyticShell(
            "a face's loops do not enclose positive area (its rings do not lie inside its \
                 outer boundary, or its declared sense is inverted)"
        ))
    );
}

/// Every curved fixture must hit the C3 capability wall by NAME. A silent
/// partial ingest here — a cylinder quietly assembled as something planar —
/// is the silent-wrong class SI5 exists to avoid, so the refusal is pinned as
/// tightly as the acceptance. When C4/C5 land, these move to acceptance in
/// the same commit (a stale wall is itself a defect).
#[test]
fn curved_fixtures_hit_the_c3_capability_wall_by_name() {
    for (name, surface) in CURVED_FIXTURES {
        let shell = analytic_shell(name);
        let mut arena = BrepArena::new();
        match kernel_v2::ingest_analytic(&mut arena, &shell) {
            Err(kernel_v2::KernelV2Error::AnalyticIngestUnsupportedSurface {
                surface: got,
                ..
            }) => assert_eq!(got, *surface, "{name}: walled on {got}, expected {surface}"),
            other => panic!("{name}: expected the planar-tier surface wall, got {other:?}"),
        }
    }
}

// =========================================================================
// Corpus cross-check (#[ignore]d — needs the ABC corpus)
// =========================================================================

/// Every `.step` under `dir`, sorted — so a capped run scans the same models
/// every time (determinism; two runs' numbers are comparable).
fn corpus_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![PathBuf::from(dir)];
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
    files
}

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

    let files = corpus_files(&dir);

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

/// What share of real models does C3 actually ingest, and what walls the
/// rest — measured, per spec §8's "categorized" posture rather than asserted.
///
///     ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 \
///         cargo test -p test-harness --test si5_analytic --release \
///         -- --ignored --nocapture c3_planar_ingestion
///
/// Two numbers matter and they answer different questions:
///
/// 1. **Reach**: models where every shell became an arena solid. Compare it
///    to the census's polyhedral share (15.7 % of the ingestible subset,
///    spec §2.1) — C3 cannot exceed that, and falling short of it localizes
///    a gap in the assembler rather than in the vocabulary.
/// 2. **In-vocabulary success**: of the models whose extracted shells are
///    ALREADY planes-and-lines, how many ingest. This one should be 100 %,
///    and every miss is a finding with a named cause — a structural claim in
///    the file that the arena's law refuses (P9/P10), not a tolerance to
///    widen.
#[test]
#[ignore = "corpus: needs ABC_DIR (scripts/fetch-abc-corpus.sh)"]
fn c3_planar_ingestion_over_the_corpus() {
    use std::collections::BTreeMap;
    use waffle_types::kernel::{AnalyticCurve, AnalyticShellData, AnalyticSurface};

    let Ok(dir) = std::env::var("ABC_DIR") else {
        eprintln!("ABC_DIR unset — skipping");
        return;
    };
    let n: usize = std::env::var("ABC_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let max_bytes: u64 = std::env::var("ABC_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);

    /// Is this shell already inside C3's vocabulary? (Planes and lines, as
    /// EXTRACTED — independent of whether the assembler accepts it, which is
    /// the whole point of measuring the two separately.)
    fn planar(shell: &AnalyticShellData) -> bool {
        shell
            .faces
            .iter()
            .all(|f| matches!(f.surface, AnalyticSurface::Plane { .. }))
            && shell
                .edges
                .iter()
                .all(|e| matches!(e.curve, AnalyticCurve::Line))
    }

    /// The census's own planes-only predicate, as a text scan (tokens, not
    /// `= NAME(` heads — spec §2.4's scanner trap). This is the independent
    /// upper bound C3's reach is compared against, so the gap between it and
    /// `in_vocab` is exactly what the extractor's file-wide refusals cost.
    fn text_polyhedral(t: &str) -> bool {
        const NON_PLANE: &[&str] = &[
            "CYLINDRICAL_SURFACE",
            "CONICAL_SURFACE",
            "SPHERICAL_SURFACE",
            "TOROIDAL_SURFACE",
            "B_SPLINE_SURFACE",
            "BEZIER_SURFACE",
            "SURFACE_OF_REVOLUTION",
            "SURFACE_OF_LINEAR_EXTRUSION",
            "OFFSET_SURFACE",
            "B_SPLINE_CURVE",
            "BEZIER_CURVE",
            "TRIMMED_CURVE",
            "POLYLINE",
            "CIRCLE",
            "ELLIPSE",
            "PARABOLA",
            "HYPERBOLA",
        ];
        !NON_PLANE.iter().any(|e| t.contains(e))
    }

    let mut scanned = 0usize;
    let mut text_poly = 0usize;
    let mut ingested = 0usize;
    let mut in_vocab = 0usize;
    let mut in_vocab_ingested = 0usize;
    let mut solids = 0usize;
    let mut faces = 0usize;
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    let mut in_vocab_failures: Vec<String> = Vec::new();

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    for path in corpus_files(&dir) {
        if scanned >= n {
            break;
        }
        let Ok(md) = std::fs::metadata(&path) else {
            continue;
        };
        if md.len() > max_bytes {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        scanned += 1;
        if text_polyhedral(&text) {
            text_poly += 1;
        }
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // truck panics rather than erroring on some real input (spec §5.6).
        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            step_import::parse_step_analytic(&text, &id)
        }));
        let import = match parsed {
            Err(_) => {
                *buckets.entry("truck PANIC".into()).or_default() += 1;
                continue;
            }
            Ok(Err(e)) => {
                *buckets.entry(format!("parse: {e}")).or_default() += 1;
                continue;
            }
            Ok(Ok(i)) => i,
        };
        let Some(shells) = import
            .shells
            .iter()
            .map(|s| s.as_ref().ok())
            .collect::<Option<Vec<_>>>()
        else {
            *buckets
                .entry("C2 extraction ineligible".into())
                .or_default() += 1;
            continue;
        };
        let all_planar = shells.iter().all(|s| planar(s));
        if all_planar {
            in_vocab += 1;
        }

        let mut arena = BrepArena::new();
        let mut ok = true;
        let mut why = String::new();
        let mut here = Vec::new();
        for shell in &shells {
            match kernel_v2::ingest_analytic(&mut arena, shell) {
                Ok(solid) => here.push((solid, shell.faces.len())),
                Err(e) => {
                    ok = false;
                    why = match e {
                        kernel_v2::KernelV2Error::AnalyticIngestUnsupportedSurface {
                            surface,
                            ..
                        } => format!("unsupported surface: {surface}"),
                        kernel_v2::KernelV2Error::AnalyticIngestUnsupportedCurve {
                            curve, ..
                        } => format!("unsupported curve: {curve}"),
                        kernel_v2::KernelV2Error::AnalyticIngestUnsupported(r) => {
                            format!("unsupported: {r}")
                        }
                        kernel_v2::KernelV2Error::InvalidAnalyticShell(r) => {
                            format!("invalid shell: {r}")
                        }
                        kernel_v2::KernelV2Error::AnalyticVertexOffSurface { .. } => {
                            "vertex off surface".into()
                        }
                        other => format!("validation: {other:?}"),
                    };
                    break;
                }
            }
        }
        if ok {
            ingested += 1;
            solids += here.len();
            faces += here.iter().map(|&(_, f)| f).sum::<usize>();
            if all_planar {
                in_vocab_ingested += 1;
            }
        } else {
            *buckets.entry(why.clone()).or_default() += 1;
            if all_planar && in_vocab_failures.len() < 20 {
                in_vocab_failures.push(format!("{id}: {why}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    eprintln!("\nSI5 C3 PLANAR INGESTION over {scanned} models (<= {max_bytes} bytes)");
    eprintln!(
        "  ingested            {ingested}  ({:.1} %)  -> {solids} solids, {faces} faces",
        100.0 * ingested as f64 / scanned.max(1) as f64
    );
    eprintln!(
        "  already in vocabulary {in_vocab}, of which ingested {in_vocab_ingested} ({:.1} %)",
        100.0 * in_vocab_ingested as f64 / in_vocab.max(1) as f64
    );
    eprintln!(
        "  text census says polyhedral {text_poly} -> the {} model(s) between that and \
         `in vocabulary` are the extractor's file-wide refusals",
        text_poly.saturating_sub(in_vocab)
    );
    eprintln!("\n  what walls the rest:");
    let mut rows: Vec<_> = buckets.iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (why, count) in rows.iter().take(20) {
        eprintln!("    x{count:<5} {why}");
    }
    if !in_vocab_failures.is_empty() {
        eprintln!("\n  IN-VOCABULARY FAILURES (each one is a finding):");
        for f in &in_vocab_failures {
            eprintln!("    {f}");
        }
    }
    assert!(scanned > 0, "no models scanned under {dir}");
}

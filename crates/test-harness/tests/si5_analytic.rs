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

/// Cone FRUSTUM: a CONICAL_SURFACE band between two full-circle rims, plus two
/// PLANE caps — the form 413 of the corpus's conical faces arrive in (spec
/// §5.1), and the one C4a ingests. (The apex `cone` above is the other form:
/// its lateral has a single rim and a singular point, which C4a refuses.)
fn frustum(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(R, 0.0),
            Point2::new(R / 2.0, H),
            Point2::new(0.0, H),
        ],
        vec![],
    )
    .expect("frustum profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .expect("full-turn frustum")
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
    ("frustum", frustum),
    ("sphere", sphere),
    ("torus", torus),
    ("drilled_block", drilled_block),
    ("block", block),
    ("slotted_block", slotted_block),
];

/// The fixtures the ingest path accepts, with their builders and their
/// `(V, E, F, R, S, G)` — so an ingestion test can compare the ingested solid
/// against the constructed one it was exported from, and the comparison
/// cannot pass vacuously if both paths lose the same structure.
const INGESTED_FIXTURES: &[(&str, Build, Counts)] = &[
    // C3, the planar tier.
    ("block", block, (8, 12, 6, 0, 1, 0)),
    // Two ring loops (the hole's mouth on each cap) and genus 1, back-solved
    // from the Euler characteristic since STEP states no genus.
    ("slotted_block", slotted_block, (16, 24, 10, 2, 1, 1)),
    // C4a, full curved bands. Stroud's single-fake-edge cylinder: two seam
    // anchors, two closed rims plus the seam, two caps plus the lateral.
    ("cylinder", cylinder, (2, 3, 3, 0, 1, 0)),
    ("frustum", frustum, (2, 3, 3, 0, 1, 0)),
    // A block with a through bore: the bore's two rims are RINGS of the caps,
    // which is the `reversed` cavity orientation case a cap cannot exercise.
    ("drilled_block", drilled_block, (10, 15, 7, 2, 1, 1)),
];

/// The fixtures the ingest path must REFUSE, with the reason that walls each.
/// A silent partial ingest of one of these would be the whole point of SI5
/// missed, so the refusal is pinned as tightly as the acceptance — and when a
/// later checkpoint lands the capability, the row MOVES to
/// [`INGESTED_FIXTURES`] in the same commit (a stale wall is itself a defect).
const UNSUPPORTED_FIXTURES: &[(&str, &str)] = &[
    ("sphere", "surface: spherical"),
    ("torus", "surface: toroidal"),
    // The apex cone's lateral has ONE rim and a singular point, not a band.
    // The corpus writes that shape with a `VERTEX_LOOP`, which C2 refuses
    // file-wide (spec §5.3), so it has no customer before C5.
    (
        "cone",
        "a curved face is not a full band of two closed rims (C4b partial patch)",
    ),
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
        ("frustum", &["CONICAL_SURFACE", "PLANE", "CIRCLE", "LINE"]),
        ("sphere", &["SPHERICAL_SURFACE"]),
        ("torus", &["TOROIDAL_SURFACE"]),
        (
            "drilled_block",
            &["CYLINDRICAL_SURFACE", "PLANE", "FACE_BOUND"],
        ),
        ("block", &["PLANE", "LINE"]),
        ("slotted_block", &["PLANE", "LINE", "FACE_BOUND"]),
    ];
    // `block` and `slotted_block` earn their place by carrying NO curve and NO
    // curved surface — that is what makes them the C3 tier's inputs rather
    // than a second copy of `drilled_block`.
    for name in ["block", "slotted_block"] {
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
    for (name, build, want_counts) in INGESTED_FIXTURES {
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
    for (name, ..) in INGESTED_FIXTURES {
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

/// Every out-of-vocabulary fixture must hit its capability wall by NAME. A
/// silent partial ingest here — a sphere quietly assembled as something else —
/// is the silent-wrong class SI5 exists to avoid, so the refusal is pinned as
/// tightly as the acceptance. When a later checkpoint lands the capability,
/// the row moves to `INGESTED_FIXTURES` in the same commit (a stale wall is
/// itself a defect).
#[test]
fn unsupported_fixtures_hit_their_capability_wall_by_name() {
    for (name, reason) in UNSUPPORTED_FIXTURES {
        let shell = analytic_shell(name);
        let mut arena = BrepArena::new();
        let got = match kernel_v2::ingest_analytic(&mut arena, &shell) {
            Err(kernel_v2::KernelV2Error::AnalyticIngestUnsupportedSurface { surface, .. }) => {
                format!("surface: {surface}")
            }
            Err(kernel_v2::KernelV2Error::AnalyticIngestUnsupported(r)) => r.to_string(),
            other => panic!("{name}: expected a typed capability wall, got {other:?}"),
        };
        assert_eq!(&got, reason, "{name}: walled on the wrong thing");
    }
}

// =========================================================================
// The geometry-provenance tier (spec `si5_geometry_provenance_tier.md`)
// =========================================================================

/// ABC `00000007` in miniature: the file's `CIRCLE` radius record and its own
/// anchor vertex are two independent roundings of one quantity, so they
/// disagree — by 4.6e-11 in that model, against a 1e-12 construction band.
/// Nudging the anchor RADIALLY reproduces it from the vertex side (§5.5: neither
/// number is "the wrong one"), and it is the shape that isolates the claim: the
/// anchor leaves its circle while every other record stays untouched.
fn anchor_pushed_off_its_circle_by(
    shell: &waffle_types::kernel::AnalyticShellData,
    delta: f64,
) -> waffle_types::kernel::AnalyticShellData {
    use waffle_types::kernel::AnalyticCurve;
    let mut out = shell.clone();
    let anchors: Vec<u32> = out
        .edges
        .iter()
        .filter(|e| matches!(e.curve, AnalyticCurve::Circle { .. }) && e.is_closed())
        .map(|e| e.start)
        .collect();
    assert!(!anchors.is_empty(), "fixture carries no closed CIRCLE edge");
    for v in anchors {
        let p = out.vertices[v as usize];
        // The fixture's rims are about the z axis, so "radial" is the xy ray.
        let r = p.x().hypot(p.y());
        let s = (r + delta) / r;
        out.vertices[v as usize] = Point3::new(p.x() * s, p.y() * s, p.z());
    }
    out
}

/// Coarsen the `CIRCLE` radius RECORD instead — the other side of the same
/// disagreement, and the one the production gates own (see the bracket test).
fn circle_radius_coarsened_by(
    shell: &waffle_types::kernel::AnalyticShellData,
    delta: f64,
) -> waffle_types::kernel::AnalyticShellData {
    use waffle_types::kernel::AnalyticCurve;
    let mut out = shell.clone();
    for e in &mut out.edges {
        if let AnalyticCurve::Circle { radius, .. } = &mut e.curve {
            *radius += delta;
        }
    }
    out
}

/// An `Asserted` solid is held to the import tier, so a 4e-11 self-disagreement
/// ingests — the whole of the 11-model class (§1.1).
#[test]
fn an_asserted_anchor_off_its_own_circle_ingests() {
    let shell = anchor_pushed_off_its_circle_by(&analytic_shell("cylinder"), 4.0e-11);
    let mut arena = BrepArena::new();
    let solid = kernel_v2::ingest_analytic(&mut arena, &shell)
        .expect("a 4e-11 record disagreement is inside the import tier");
    assert_eq!(
        arena.solid(solid).expect("live").provenance,
        kernel_v2::GeometryProvenance::Asserted,
        "an ingested solid's coordinates are the file's, not the kernel's"
    );
}

/// The other half of that claim, and the one that makes it a tier assignment
/// rather than a band widening: the SAME geometry, called `Constructed`, is
/// still refused by the construction tripwire. Nothing was loosened for any
/// producer that places its own coordinates.
#[test]
fn the_same_defect_on_a_constructed_solid_is_still_refused() {
    let shell = anchor_pushed_off_its_circle_by(&analytic_shell("cylinder"), 4.0e-11);
    let mut arena = BrepArena::new();
    let solid = kernel_v2::ingest_analytic(&mut arena, &shell).expect("ingests as asserted");

    arena.solids[solid.index()]
        .as_mut()
        .expect("live")
        .provenance = kernel_v2::GeometryProvenance::Constructed;

    match kernel_v2::validate_solid(&arena, solid) {
        Err(kernel_v2::KernelV2Error::VertexOffSurface { .. }) => {}
        other => panic!(
            "the construction tripwire must still refuse 4e-11 on geometry claimed as \
             constructed, got {other:?}"
        ),
    }
}

/// The oracle for "no production coverage was given up" (§4). Relaxing a
/// debug-tier tripwire is only honest if the claim it was making is still held
/// somewhere that always compiles, and for C4a's vocabulary three PRODUCTION
/// gates bracket it. This test is the measured bracket, so a later loosening of
/// any of them turns red here rather than silently opening the window the
/// tripwire used to cover.
///
/// Both columns of the sweep that established it (cylinder fixture, r = 5 mm):
/// a coarsened radius record is owned by the rim-radius agreement from 1e-11,
/// and both shapes are owned by the seam-anchor reconciliation from 5e-9.
#[test]
fn production_gates_bracket_the_on_curve_claim() {
    let base = analytic_shell("cylinder");
    for (delta, want) in [
        (1.0e-11, "rim circle radius disagrees with the surface"),
        (1.0e-9, "rim circle radius disagrees with the surface"),
    ] {
        let mut arena = BrepArena::new();
        match kernel_v2::ingest_analytic(&mut arena, &circle_radius_coarsened_by(&base, delta)) {
            Err(kernel_v2::KernelV2Error::CurvedGeometryMismatch { reason, .. }) => {
                assert_eq!(reason, want, "radius record off by {delta:.0e}")
            }
            other => panic!("a radius record off by {delta:.0e} must be refused, got {other:?}"),
        }
    }
    for shape in [anchor_pushed_off_its_circle_by, circle_radius_coarsened_by] {
        let mut arena = BrepArena::new();
        match kernel_v2::ingest_analytic(&mut arena, &shape(&base, 5.0e-9)) {
            Err(kernel_v2::KernelV2Error::AnalyticIngestUnsupported(reason)) => assert_eq!(
                reason,
                "a rim needing a re-anchored seam shares its anchor vertex with another edge",
                "5e-9 is past the import tier and the seam-anchor gate owns it"
            ),
            other => panic!("5e-9 off the circle must be refused in production, got {other:?}"),
        }
    }
}

/// A block overlapping the ingested cylinder with no coplanar face pair, so
/// the join can be measured through a real boolean rather than asserted.
fn offset_block(arena: &mut BrepArena) -> SolidId {
    let side = 0.020;
    let profile = Profile::new(
        Point3::new(0.004, 0.0, 0.006),
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
        .expect("offset block extrude")
        .solid
}

/// The lattice: a boolean carries its operands' faces through, so one asserted
/// operand makes the output asserted (§3.3). Without this the ingested
/// geometry's own roundings would meet the construction tripwire one operation
/// later — a loud refusal, which is the direction a lost tier fails in.
#[test]
fn provenance_joins_through_a_boolean() {
    let mut arena = BrepArena::new();
    let ingested =
        kernel_v2::ingest_analytic(&mut arena, &analytic_shell("cylinder")).expect("ingests");
    let built = offset_block(&mut arena);
    let joined = boolean_op(&mut arena, ingested, built, BoolOp::Union).expect("union");
    assert_eq!(
        arena.solid(joined).expect("live").provenance,
        kernel_v2::GeometryProvenance::Asserted,
        "a union with an ingested operand carries the file's tier"
    );

    let mut only_built = BrepArena::new();
    let a = cylinder(&mut only_built);
    let b = offset_block(&mut only_built);
    let c = boolean_op(&mut only_built, a, b, BoolOp::Union).expect("union");
    assert_eq!(
        only_built.solid(c).expect("live").provenance,
        kernel_v2::GeometryProvenance::Constructed,
        "a union of constructed operands stays at the construction tier"
    );
}

/// A rigid placement moves coordinates without placing them, so the copy keeps
/// the source's tier — and the copy is validated, which it could not survive
/// if the tier were dropped.
#[test]
fn provenance_survives_a_transform() {
    let shell = anchor_pushed_off_its_circle_by(&analytic_shell("cylinder"), 4.0e-11);
    let mut arena = BrepArena::new();
    let src = kernel_v2::ingest_analytic(&mut arena, &shell).expect("ingests");
    let moved = kernel_v2::transform_solid(
        &mut arena,
        src,
        &RigidPlacement {
            translation: [0.05, 0.0, 0.0],
            ..RigidPlacement::IDENTITY
        },
    )
    .expect("a moved copy of an ingested solid validates at its own tier");
    assert_eq!(
        arena.solid(moved).expect("live").provenance,
        kernel_v2::GeometryProvenance::Asserted
    );
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

/// **C4b-M, the measurement checkpoint of `specs/si5_c4b_arc_patch_tier.md`.**
/// Four numbers the C4b design needs and does not have, two of which can change
/// the plan:
///
/// 1. what the CONE-section-ellipse wall (KV16b) costs in model reach;
/// 2. whether multi-loop arc patches exist at all — if they do not, C4b's only
///    genuinely new logic (outer-loop ranking in the unrolled domain) has no
///    customer yet and must not be written as general machinery;
/// 3. the projected per-model reach of the whole tier;
/// 4. the share of arcs at or near a half turn, which is the one place a C4b
///    mapping can silently build the complementary arc.
///
///     ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 \
///         cargo test -p test-harness --test si5_analytic --release \
///         -- --ignored --nocapture c4b_arc_patch_census
#[test]
#[ignore = "corpus: needs ABC_DIR (scripts/fetch-abc-corpus.sh)"]
fn c4b_arc_patch_census() {
    use std::collections::BTreeMap;
    use waffle_types::kernel::{AnalyticCurve, AnalyticLoop, AnalyticShellData, AnalyticSurface};

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

    /// C4b's surface/curve vocabulary: planes, cylinders, cones; lines,
    /// circles (open or closed), ellipses. Spheres and tori are C5.
    fn c4b_vocab(shell: &AnalyticShellData) -> bool {
        shell.faces.iter().all(|f| {
            matches!(
                f.surface,
                AnalyticSurface::Plane { .. }
                    | AnalyticSurface::Cylinder { .. }
                    | AnalyticSurface::Cone { .. }
            )
        }) && shell
            .faces
            .iter()
            .all(|f| f.loops.iter().all(|l| matches!(l, AnalyticLoop::Edges(_))))
    }

    let mut scanned = 0usize;
    let mut vocab = 0usize;
    let mut vocab_with_cone_ellipse = 0usize;
    let mut projected_reach = 0usize;
    let mut arc_patch_faces = 0usize;
    let mut loops_per_arc_patch: BTreeMap<usize, usize> = BTreeMap::new();
    let mut ellipse_on: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut arcs = 0usize;
    let mut arcs_half_turn = 0usize;
    let mut arcs_near_half = 0usize;

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
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            step_import::parse_step_analytic(&text, &id)
        }));
        let Ok(Ok(import)) = parsed else { continue };
        let Some(shells) = import
            .shells
            .iter()
            .map(|s| s.as_ref().ok())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        if !shells.iter().all(|s| c4b_vocab(s)) {
            continue;
        }
        vocab += 1;

        let mut cone_ellipse = false;
        for shell in &shells {
            // Which faces use which edges, so an ellipse can be attributed to
            // the surface it actually bounds.
            for (fi, face) in shell.faces.iter().enumerate() {
                let kind = face.surface.surface_type_str();
                let mut closed_circles = 0usize;
                let mut loops_here = 0usize;
                for lp in &face.loops {
                    let AnalyticLoop::Edges(os) = lp else {
                        continue;
                    };
                    loops_here += 1;
                    for o in os {
                        let e = &shell.edges[o.edge as usize];
                        match e.curve {
                            AnalyticCurve::Circle { .. } if e.start == e.end => closed_circles += 1,
                            AnalyticCurve::Ellipse { .. } => {
                                *ellipse_on.entry(kind).or_default() += 1;
                                if matches!(face.surface, AnalyticSurface::Cone { .. }) {
                                    cone_ellipse = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let curved = !matches!(face.surface, AnalyticSurface::Plane { .. });
                if curved && closed_circles == 0 {
                    arc_patch_faces += 1;
                    *loops_per_arc_patch.entry(loops_here).or_default() += 1;
                }
                let _ = fi;
            }
            // Arc sweeps, from the file's own `interior` point (never derived
            // from the endpoints — spec §3.1).
            for e in &shell.edges {
                if let AnalyticCurve::Circle {
                    center,
                    radius,
                    interior,
                    ..
                } = e.curve
                {
                    if e.start == e.end {
                        continue;
                    }
                    arcs += 1;
                    let p = shell.vertices[e.start as usize];
                    let q = shell.vertices[e.end as usize];
                    // Chord/radius geometry: the sweep of the arc THROUGH
                    // `interior` is > π iff `interior` is on the far side.
                    let v =
                        |a: Point3| [a.x() - center.x(), a.y() - center.y(), a.z() - center.z()];
                    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                    let (vp, vq, vi) = (v(p), v(q), v(interior));
                    let cos_pq = (dot(vp, vq) / (radius * radius)).clamp(-1.0, 1.0);
                    let minor = cos_pq.acos();
                    // `interior` nearer the midpoint direction of the MINOR arc
                    // means the arc is the minor one.
                    let mid = [vp[0] + vq[0], vp[1] + vq[1], vp[2] + vq[2]];
                    let sweep = if dot(mid, vi) >= 0.0 {
                        minor
                    } else {
                        2.0 * std::f64::consts::PI - minor
                    };
                    let half = std::f64::consts::PI;
                    if (sweep - half).abs() <= 1e-9 {
                        arcs_half_turn += 1;
                    } else if (sweep - half).abs() <= half / 180.0 {
                        arcs_near_half += 1;
                    }
                }
            }
        }
        if cone_ellipse {
            vocab_with_cone_ellipse += 1;
        } else {
            projected_reach += 1;
        }
    }
    std::panic::set_hook(prev_hook);

    eprintln!("\nC4b CENSUS over {scanned} models (<= {max_bytes} bytes)");
    eprintln!(
        "  in C4b surface/curve vocabulary   {vocab}  ({:.1} %)",
        100.0 * vocab as f64 / scanned.max(1) as f64
    );
    eprintln!(
        "    of those, walled by a CONE-section ellipse (KV16b)  {vocab_with_cone_ellipse}  \
         ({:.1} % of vocab)",
        100.0 * vocab_with_cone_ellipse as f64 / vocab.max(1) as f64
    );
    eprintln!(
        "  PROJECTED C4b reach               {projected_reach}  ({:.1} % of scanned)",
        100.0 * projected_reach as f64 / scanned.max(1) as f64
    );
    eprintln!("  arc-patch faces                   {arc_patch_faces}");
    eprintln!("    loops per arc patch: {loops_per_arc_patch:?}");
    eprintln!("  ellipse edge uses by face kind: {ellipse_on:?}");
    eprintln!(
        "  open arcs {arcs}: exactly a half turn {arcs_half_turn}, within 1° of one \
         {arcs_near_half}"
    );
    assert!(scanned > 0, "no models scanned under {dir}");
}

/// What share of real models does the ingest path actually take, and what
/// walls the rest — measured, per spec §8's "categorized" posture rather than
/// asserted.
///
///     ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 \
///         cargo test -p test-harness --test si5_analytic --release \
///         -- --ignored --nocapture ingestion_over_the_corpus
///
/// Three numbers matter and they answer different questions:
///
/// 1. **Reach**: models where every shell became an arena solid.
/// 2. **In-vocabulary success**: of the models whose extracted shells are
///    ALREADY inside the current checkpoint's vocabulary, how many ingest.
///    This one should be 100 %, and every miss is a finding with a named
///    cause — a structural claim in the file that the arena's law refuses
///    (P9/P10), not a tolerance to widen.
/// 3. **The planar sub-tier**, kept separately so a C4a regression that only
///    affects curved bands is visible as a divergence between the two rather
///    than as one number moving.
#[test]
#[ignore = "corpus: needs ABC_DIR (scripts/fetch-abc-corpus.sh)"]
fn ingestion_over_the_corpus() {
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

    /// Is this shell already inside the planar C3 vocabulary? (Planes and
    /// lines, as EXTRACTED — independent of whether the assembler accepts it,
    /// which is the whole point of measuring the two separately.)
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

    /// Is this shell inside C4a's vocabulary? Planes, cylinders and cones;
    /// lines and CLOSED circles; every curved face a full band of two closed
    /// rims, every planar loop either a chain of chords or one closed circle.
    /// The form test is the part a surface/curve census cannot do — it is
    /// exactly what spec §5.1's re-measurement showed matters.
    fn c4a(shell: &AnalyticShellData) -> bool {
        use waffle_types::kernel::AnalyticLoop;
        let closed_circle = |e: u32| {
            let e = &shell.edges[e as usize];
            e.start == e.end && matches!(e.curve, AnalyticCurve::Circle { .. })
        };
        if !shell.edges.iter().all(|e| match e.curve {
            AnalyticCurve::Line => e.start != e.end,
            AnalyticCurve::Circle { .. } => e.start == e.end,
            AnalyticCurve::Ellipse { .. } => false,
        }) {
            return false;
        }
        shell.faces.iter().all(|f| {
            let loops: Option<Vec<&Vec<_>>> = f
                .loops
                .iter()
                .map(|l| match l {
                    AnalyticLoop::Edges(os) => Some(os),
                    AnalyticLoop::Vertex(_) => None,
                })
                .collect();
            let Some(loops) = loops else { return false };
            let rim = |os: &Vec<waffle_types::kernel::OrientedEdge>| {
                os.len() == 1 && closed_circle(os[0].edge)
            };
            let chords = |os: &Vec<waffle_types::kernel::OrientedEdge>| {
                os.len() >= 3 && os.iter().all(|o| !closed_circle(o.edge))
            };
            match f.surface {
                AnalyticSurface::Plane { .. } => loops.iter().all(|os| rim(os) || chords(os)),
                AnalyticSurface::Cylinder { .. } | AnalyticSurface::Cone { .. } => {
                    // Two single-rim loops, or the canonical seamed lateral.
                    (loops.len() == 2 && loops.iter().all(|os| rim(os)))
                        || (loops.len() == 1
                            && loops[0].len() == 4
                            && loops[0].iter().filter(|o| closed_circle(o.edge)).count() == 2)
                }
                _ => false,
            }
        })
    }

    /// The census's own predicates, as text scans (tokens, not `= NAME(`
    /// heads — spec §2.4's scanner trap). These are the INDEPENDENT upper
    /// bounds reach is compared against, so a gap between one of them and the
    /// corresponding `in_vocab` count is exactly what the extractor's
    /// file-wide refusals plus the form gate cost.
    const OUT_OF_C4A: &[&str] = &[
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
        "ELLIPSE",
        "PARABOLA",
        "HYPERBOLA",
    ];
    fn text_polyhedral(t: &str) -> bool {
        !OUT_OF_C4A.iter().any(|e| t.contains(e))
            && !["CYLINDRICAL_SURFACE", "CONICAL_SURFACE", "CIRCLE"]
                .iter()
                .any(|e| t.contains(e))
    }
    /// The C4a SURFACE+CURVE gate only: it cannot see the band/patch form, so
    /// it is a genuine upper bound rather than a prediction.
    fn text_c4a(t: &str) -> bool {
        !OUT_OF_C4A.iter().any(|e| t.contains(e))
    }

    let mut scanned = 0usize;
    let mut text_poly = 0usize;
    let mut text_curved = 0usize;
    let mut ingested = 0usize;
    let mut planar_ingested = 0usize;
    let mut in_vocab = 0usize;
    let mut in_vocab_ingested = 0usize;
    let mut vocab_planar = 0usize;
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
        if text_c4a(&text) {
            text_curved += 1;
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
        let all_c4a = shells.iter().all(|s| c4a(s));
        if all_c4a {
            in_vocab += 1;
            if all_planar {
                vocab_planar += 1;
            }
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
            if all_c4a {
                in_vocab_ingested += 1;
            }
            if all_planar {
                planar_ingested += 1;
            }
        } else {
            *buckets.entry(why.clone()).or_default() += 1;
            if all_c4a && in_vocab_failures.len() < 20 {
                in_vocab_failures.push(format!("{id}: {why}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    eprintln!("\nSI5 C3+C4a INGESTION over {scanned} models (<= {max_bytes} bytes)");
    eprintln!(
        "  ingested              {ingested}  ({:.1} %)  -> {solids} solids, {faces} faces",
        100.0 * ingested as f64 / scanned.max(1) as f64
    );
    eprintln!(
        "  already in vocabulary {in_vocab}, of which ingested {in_vocab_ingested} ({:.1} %)",
        100.0 * in_vocab_ingested as f64 / in_vocab.max(1) as f64
    );
    eprintln!(
        "    of those, planar only {vocab_planar} (ingested {planar_ingested}) -> C4a's own \
         contribution is {} model(s)",
        ingested.saturating_sub(planar_ingested)
    );
    eprintln!(
        "  text census upper bounds: polyhedral {text_poly}, C4a surface+curve gate \
         {text_curved}"
    );
    eprintln!(
        "    the {} model(s) between the C4a gate and `in vocabulary` are the extractor's \
         file-wide refusals plus the BAND/PATCH form gate the text cannot see",
        text_curved.saturating_sub(in_vocab)
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
    // The doc comment above has claimed since C3 that in-vocabulary success
    // "should be 100 %, and every miss is a finding". Since the provenance
    // tier (spec `si5_geometry_provenance_tier.md`) it IS 100 %, so assert it:
    // the next miss should be a red test, not a line of output nobody reads.
    assert_eq!(
        in_vocab_ingested,
        in_vocab,
        "{} in-vocabulary model(s) refused — each one is a finding, not a \
         tolerance to widen",
        in_vocab - in_vocab_ingested
    );
}

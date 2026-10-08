//! The sketch corpus (`specs/agent_mechanical_design.md` §10.4, S4).
//!
//! `app/tests/cases/sketch/<ID>.waffle` + `<ID>.meta.json`: one real document
//! per case, holding one `Sketch` feature, beside the answer that case is
//! supposed to have. The kernel has had a 351-case corpus and a categorized
//! assay since 2026; the sketch system had **no** fixture-driven test at all —
//! every sketch in `sketch-solver`'s 282 tests is hand-built in Rust, and the
//! 1,172 sketches in the kernel corpus carry zero constraints because nothing
//! ever authored one (`crates/wasm-bridge/tests/sketch_constraint_persistence.rs`
//! measured that it is an authoring gap, not a persistence bug).
//!
//! ## The expectations are AUTHORED, not captured
//!
//! This is the whole design, and the reason the generator is a separate binary.
//! A corpus whose expectations come from running the solver is a recording:
//! it detects change, which is useful, but it cannot detect that the answer was
//! wrong all along — and this session found two defects of exactly that kind
//! (a fillet silently extruding as a chamfer, a point-line dimension mirroring
//! its point) which a captured corpus would have enshrined.
//!
//! So every number in a `.meta.json` is written by hand in
//! `src/bin/sketch_corpus_gen.rs`, with its arithmetic in a comment, and the
//! generator REFUSES to write a case whose expectations the solver does not
//! meet. Generating the corpus is therefore itself the first assertion, and a
//! case that disagrees is a finding to adjudicate before it is committed —
//! never a number to update.
//!
//! A case pinning a KNOWN defect says so in `defect`, which keeps the wrong
//! answer loud instead of silently blessing it.
//!
//! ## Three tiers
//!
//! `tests/sketch_corpus.rs` replays each case through:
//!
//! 1. **pure** — `sketch_solver::solve_sketch` on the sketch read from the file;
//! 2. **engine** — `sketch_create` and `sketch_solve_state`, the S3 agent door,
//!    through `wasm_bridge::execute_tool` with a `MockKernel`;
//! 3. **oracle** — [`crate::sketch_rank`], the independent structural
//!    computation (finite differences + SVD).
//!
//! All three must agree with the authored answer, and with each other.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use waffle_types::{Sketch, SolveStatus};

/// One closed region a case expects, by the entity ids that bound it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedRegion {
    /// The region's BOUNDARY entity ids, ascending (the comparison sorts both
    /// sides — a boundary is an order-insensitive identity, which is what
    /// `resolve_region_by_identity` matches on).
    ///
    /// `boundary_entity_ids` rather than `profile_entity_ids` because only the
    /// boundary exists for every region: a plate with a hole reports its
    /// ANNULUS with `profile_entity_ids: None` (it is not one whole loop) and
    /// `boundary_entity_ids: Some([5, 6, 7, 8, 10])` — the outline AND the
    /// hole. Keying on the profile ids would have silently dropped that region
    /// from the comparison instead of checking it.
    pub entity_ids: Vec<u32>,
    /// Whether this region can be handed to an Extrude — i.e. whether it
    /// carries `profile_entity_ids` because it IS one whole loop.
    ///
    /// Worth its own field: it is the difference between a region a feature can
    /// consume and one that only exists on screen, and the annulus of a holed
    /// plate is the second kind.
    pub extrudable: bool,
    /// The region's area in sketch units², as ARITHMETIC gives it.
    ///
    /// Compared relatively (`area_rel_tol`), never absolutely. Two floors make
    /// an absolute tolerance here a tolerance on the sketch's units instead:
    /// `compute_regions` measures on the slicer's snapped float grid, so an
    /// EXACT 0.06 × 0.04 rectangle reads 2.4000000044703484e-3 (out by 2^-29
    /// relative), and a solved coordinate carries LM's convergence tail. A loop
    /// containing an arc is additionally LOW by its chords
    /// (`DEFAULT_CHORD_TOLERANCE` is 1e-3, relative), which is why a curved
    /// case sets its own looser `area_rel_tol` and says so.
    pub area: f64,
}

/// What a case's answer is supposed to be (§10.4's `SketchOracleExpectations`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchOracleExpectations {
    /// The `SolveStatus` serde tag: `FullyConstrained`, `UnderConstrained`,
    /// `OverConstrained` or `SolveFailed`.
    pub status: String,
    /// Degrees of freedom left. Present for EVERY verdict — the S2 report
    /// carries a number even where `SolveStatus` does not.
    pub dof: u32,
    /// Solver parameters: 2 per point, 1 per circle radius.
    pub params: u32,
    /// Residual ROWS handed to the solver (a multi-row constraint contributes
    /// several). `rank < rows` on a satisfied system is the redundancy
    /// condition, so a case asserting `redundant` has to pin these two.
    pub rows: u32,
    /// Rank of the final constraint Jacobian.
    pub rank: u32,
    /// Driving constraints expected to exceed tolerance, as indices into the
    /// sketch's FULL constraint array (reference dimensions included — the S2
    /// index space). Order-insensitive in the comparison: which of two mutually
    /// contradictory constraints is "worst" is a least-squares accident.
    pub conflicts: Vec<u32>,
    /// Dependent driving constraints on a satisfied, over-determined system.
    pub redundant: Vec<u32>,
    /// Every closed region the sketch is expected to have.
    pub regions: Vec<ExpectedRegion>,
    /// Solved coordinates the case knows in closed form, by point id. A case
    /// may pin only the points whose position is arithmetic.
    #[serde(default)]
    pub positions: BTreeMap<u32, [f64; 2]>,
    /// Absolute tolerance for `positions`, in sketch units.
    pub positions_tol: f64,
    /// Solved radii the case knows in closed form, by entity id.
    ///
    /// Separate from `positions` because a radius is its own parameter and
    /// never travels through them — and separate from `regions[].area` because
    /// the area of a curved loop is a CHORD POLYGON's and so cannot carry a
    /// tight tolerance, while the radius can. A circle case therefore pins its
    /// radius exactly and its area loosely, instead of using a loose area as a
    /// weak proxy for both.
    #[serde(default)]
    pub radii: BTreeMap<u32, f64>,
    /// Absolute tolerance for `radii`, in sketch units.
    #[serde(default = "default_radii_tol")]
    pub radii_tol: f64,
    /// Relative tolerance for every `regions[].area`.
    pub area_rel_tol: f64,
    /// Set when the case pins a KNOWN DEFECT rather than the right answer: what
    /// is wrong, so the next reader finds the explanation here instead of
    /// trusting the number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defect: Option<String>,
}

fn default_radii_tol() -> f64 {
    1e-12
}

/// A corpus case: its identity, and the answer it should give.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchCaseMeta {
    pub id: String,
    /// What the case is and why it is in the corpus.
    pub description: String,
    /// What the case exercises, for the runner's coverage summary: entity
    /// kinds, constraint kinds, and free-text tags like `"redundant"`.
    pub exercises: Vec<String>,
    pub expectations: SketchOracleExpectations,
    /// Bumped when the generator's authored answers change shape, so a stale
    /// file is visible rather than silently mixed in.
    pub generator_version: u32,
}

/// The generator version this build writes and the runner requires.
pub const GENERATOR_VERSION: u32 = 1;

/// The corpus directory, relative to the repo root.
pub const CORPUS_DIR: &str = "app/tests/cases/sketch";

/// The repo root, from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// A loaded case: its metadata, and the sketch out of its document.
pub struct LoadedCase {
    pub meta: SketchCaseMeta,
    pub sketch: Sketch,
}

/// Load every case in `dir`, by ascending id.
///
/// A `.waffle` with no `.meta.json`, or a case whose document holds no single
/// `Sketch` feature, is a hard error: a corpus that silently skips a case it
/// cannot read reports a green run over fewer cases than it has.
pub fn load_corpus(dir: &Path) -> Vec<LoadedCase> {
    let mut ids: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|e| e.to_str()) == Some("waffle"))
                .then(|| path.file_stem()?.to_str().map(str::to_string))
                .flatten()
        })
        .collect();
    ids.sort();

    ids.into_iter()
        .map(|id| {
            let meta_path = dir.join(format!("{id}.meta.json"));
            let meta_text = std::fs::read_to_string(&meta_path)
                .unwrap_or_else(|e| panic!("{}: {e}", meta_path.display()));
            let meta: SketchCaseMeta = serde_json::from_str(&meta_text)
                .unwrap_or_else(|e| panic!("{}: {e}", meta_path.display()));
            assert_eq!(
                meta.generator_version, GENERATOR_VERSION,
                "{id}: generator_version {} but this build writes {GENERATOR_VERSION}; \
                 regenerate with `cargo run -p test-harness --bin sketch_corpus_gen`",
                meta.generator_version,
            );
            let waffle_path = dir.join(format!("{id}.waffle"));
            let text = std::fs::read_to_string(&waffle_path)
                .unwrap_or_else(|e| panic!("{}: {e}", waffle_path.display()));
            let sketch = sketch_of(&text, &id);
            LoadedCase { meta, sketch }
        })
        .collect()
}

/// The one `Sketch` in a case document.
pub fn sketch_of(waffle_json: &str, id: &str) -> Sketch {
    let (tree, _meta) = file_format::load_project(waffle_json)
        .unwrap_or_else(|e| panic!("{id}: the document does not load: {e:?}"));
    let sketches: Vec<Sketch> = tree
        .features
        .iter()
        .filter_map(|f| match &f.operation {
            feature_engine::types::Operation::Sketch { sketch } => Some(sketch.clone()),
            _ => None,
        })
        .collect();
    match sketches.len() {
        1 => sketches.into_iter().next().unwrap(),
        n => panic!("{id}: expected exactly one Sketch feature, found {n}"),
    }
}

/// The serde tag of a solve status, as the expectations spell it.
pub fn status_tag(status: &SolveStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
        .unwrap_or_else(|| "Unsolved".to_string())
}

/// One way an answer differed from what the case expects.
#[derive(Debug, Clone)]
pub struct Mismatch {
    /// Which field: `"dof"`, `"regions[5,6,7,8].area"`, …
    pub field: String,
    pub expected: String,
    pub actual: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: expected {}, got {}",
            self.field, self.expected, self.actual
        )
    }
}

/// Everything a tier can report about one sketch, in the shape the
/// expectations are written in — so the three tiers are compared field by
/// field against one authored answer rather than against each other's shapes.
#[derive(Debug, Clone)]
pub struct TierAnswer {
    pub status: String,
    pub dof: u32,
    pub params: u32,
    pub rows: u32,
    pub rank: u32,
    pub conflicts: Vec<u32>,
    pub redundant: Vec<u32>,
    /// `(sorted boundary ids, extrudable, area)` per region, sorted by the ids.
    pub regions: Vec<(Vec<u32>, bool, f64)>,
    pub positions: BTreeMap<u32, [f64; 2]>,
    pub radii: BTreeMap<u32, f64>,
}

fn sorted(mut v: Vec<u32>) -> Vec<u32> {
    v.sort_unstable();
    v
}

/// Compare a tier's answer with the case's authored expectations.
pub fn check(expect: &SketchOracleExpectations, got: &TierAnswer) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let mut eq = |field: &str, e: String, a: String| {
        if e != a {
            out.push(Mismatch {
                field: field.to_string(),
                expected: e,
                actual: a,
            });
        }
    };

    eq("status", expect.status.clone(), got.status.clone());
    eq("dof", expect.dof.to_string(), got.dof.to_string());
    eq("params", expect.params.to_string(), got.params.to_string());
    eq("rows", expect.rows.to_string(), got.rows.to_string());
    eq("rank", expect.rank.to_string(), got.rank.to_string());
    eq(
        "conflicts",
        format!("{:?}", sorted(expect.conflicts.clone())),
        format!("{:?}", sorted(got.conflicts.clone())),
    );
    eq(
        "redundant",
        format!("{:?}", sorted(expect.redundant.clone())),
        format!("{:?}", sorted(got.redundant.clone())),
    );

    // `free.len() == dof` is the S2 contract and is checked by the runner
    // against the report directly; here only the fields a case authors.

    let want: Vec<(Vec<u32>, bool, f64)> = expect
        .regions
        .iter()
        .map(|r| (sorted(r.entity_ids.clone()), r.extrudable, r.area))
        .collect();
    eq(
        "regions.count",
        want.len().to_string(),
        got.regions.len().to_string(),
    );
    for (ids, extrudable, area) in &want {
        match got.regions.iter().find(|(got_ids, _, _)| got_ids == ids) {
            None => out.push(Mismatch {
                field: format!("regions{ids:?}"),
                expected: format!("a region of area {area:e}"),
                actual: format!(
                    "no region with that boundary; got {:?}",
                    got.regions.iter().map(|(i, _, _)| i).collect::<Vec<_>>()
                ),
            }),
            Some((_, got_extrudable, got_area)) => {
                if got_extrudable != extrudable {
                    out.push(Mismatch {
                        field: format!("regions{ids:?}.extrudable"),
                        expected: extrudable.to_string(),
                        actual: got_extrudable.to_string(),
                    });
                }
                let tol = expect.area_rel_tol * area.abs();
                if (got_area - area).abs() > tol {
                    out.push(Mismatch {
                        field: format!("regions{ids:?}.area"),
                        expected: format!("{area:.17e} ± {:.1e} rel", expect.area_rel_tol),
                        actual: format!(
                            "{got_area:.17e} (off by {:.3e} rel)",
                            (got_area - area) / area
                        ),
                    });
                }
            }
        }
    }

    for (id, want) in &expect.radii {
        match got.radii.get(id) {
            None => out.push(Mismatch {
                field: format!("radii[{id}]"),
                expected: format!("{want:e}"),
                actual: "no radius for that entity".to_string(),
            }),
            Some(got) => {
                if (got - want).abs() > expect.radii_tol {
                    out.push(Mismatch {
                        field: format!("radii[{id}]"),
                        expected: format!("{want:e} ± {:e}", expect.radii_tol),
                        actual: format!("{got:e} ({:e} away)", (got - want).abs()),
                    });
                }
            }
        }
    }

    for (id, want) in &expect.positions {
        match got.positions.get(id) {
            None => out.push(Mismatch {
                field: format!("positions[{id}]"),
                expected: format!("{want:?}"),
                actual: "no position for that point".to_string(),
            }),
            Some(got) => {
                let off = ((got[0] - want[0]).powi(2) + (got[1] - want[1]).powi(2)).sqrt();
                if off > expect.positions_tol {
                    out.push(Mismatch {
                        field: format!("positions[{id}]"),
                        expected: format!("{want:?} ± {:e}", expect.positions_tol),
                        actual: format!("{got:?} ({off:e} away)"),
                    });
                }
            }
        }
    }
    out
}

/// A tier's answer from a `SolvedSketch` plus the regions computed from it.
pub fn answer_from_solved(
    solved: &waffle_types::SolvedSketch,
    regions: &[waffle_types::regions::Region],
) -> TierAnswer {
    TierAnswer {
        status: status_tag(&solved.status),
        dof: solved.report.dof,
        params: solved.report.params,
        rows: solved.report.rows,
        rank: solved.report.rank,
        conflicts: solved.report.conflicts.clone(),
        redundant: solved.report.redundant.clone(),
        regions: regions_as_pairs(regions),
        positions: solved
            .positions
            .iter()
            .map(|(id, (x, y))| (*id, [*x, *y]))
            .collect(),
        radii: solved.radii.iter().map(|(id, r)| (*id, *r)).collect(),
    }
}

/// Regions as `(sorted boundary ids, extrudable, area)`.
///
/// EVERY region, keyed on its boundary: a region with no boundary identity at
/// all would be unauthorable, and none has been seen — if one appears it is
/// reported with an empty id list rather than dropped, because a corpus that
/// silently omits a region reports a green run over fewer regions than the
/// sketch has.
pub fn regions_as_pairs(regions: &[waffle_types::regions::Region]) -> Vec<(Vec<u32>, bool, f64)> {
    let mut out: Vec<(Vec<u32>, bool, f64)> = regions
        .iter()
        .map(|r| {
            (
                sorted(r.boundary_entity_ids.clone().unwrap_or_default()),
                r.profile_entity_ids.is_some(),
                r.area,
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The regions of a solved sketch, the way every sketch tool computes them.
pub fn regions_of(
    sketch: &Sketch,
    positions: &std::collections::HashMap<u32, (f64, f64)>,
) -> Vec<waffle_types::regions::Region> {
    waffle_types::compute_regions(
        &sketch.entities,
        positions,
        waffle_types::regions::DEFAULT_CHORD_TOLERANCE,
    )
}

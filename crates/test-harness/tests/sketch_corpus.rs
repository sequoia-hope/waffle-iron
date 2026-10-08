//! Replay the sketch corpus through three independent computations
//! (`specs/agent_mechanical_design.md` §10.4, S4).
//!
//! Each case in `app/tests/cases/sketch/` carries an answer someone DERIVED
//! (see `examples/sketch_corpus_gen.rs` — the generator refuses to write a case
//! whose expectations the solver does not meet, so a committed case is one
//! whose numbers were adjudicated). This runner holds three tiers to that one
//! answer:
//!
//! 1. **pure** — `sketch_solver::solve_sketch` on the sketch read out of the
//!    case document, plus `compute_regions` on what it solved;
//! 2. **engine** — `sketch_create` and `sketch_solve_state`, the S3 agent door,
//!    through `wasm_bridge::execute_tool` with a `MockKernel`. A case that the
//!    solver gets right and the door gets wrong is a door defect, and before S3
//!    there was no way to see one;
//! 3. **oracle** — `test_harness::sketch_rank`, the independent structural
//!    computation (residuals from the published equations, finite differences,
//!    SVD), which is the only tier that can contradict the solver's own report
//!    about `rank` and `dof`.
//!
//! The tiers must agree with the authored answer AND with each other. An
//! agreement between two implementations of the same algebra is weak evidence;
//! an agreement between the authored arithmetic, the solver and the oracle is
//! the strong form, and that is why the corpus exists rather than another
//! hand-built test module.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use test_harness::sketch_corpus::{
    answer_from_solved, check, load_corpus, regions_as_pairs, regions_of, repo_root, status_tag,
    LoadedCase, Mismatch, TierAnswer, CORPUS_DIR,
};
use test_harness::sketch_rank::analyze_at;
use waffle_types::kernel::MockKernel;
use waffle_types::{Sketch, SolveStatus};

fn corpus() -> Vec<LoadedCase> {
    let dir = repo_root().join(CORPUS_DIR);
    let cases = load_corpus(&dir);
    assert!(
        !cases.is_empty(),
        "{} holds no cases; generate them with \
         `cargo run -p test-harness --example sketch_corpus_gen`",
        dir.display()
    );
    cases
}

// ── Tier 1: the solver ──────────────────────────────────────────────────────

/// What the solver makes of a case, with the regions of what it solved.
fn pure_tier(sketch: &Sketch) -> (waffle_types::SolvedSketch, TierAnswer) {
    let solved = sketch_solver::solve_sketch(sketch);
    let regions = regions_of(sketch, &solved.positions);
    let answer = answer_from_solved(&solved, &regions);
    (solved, answer)
}

#[test]
fn every_case_answers_what_it_was_authored_to_answer() {
    let mut failures: Vec<String> = Vec::new();
    for case in corpus() {
        let (solved, answer) = pure_tier(&case.sketch);
        let mismatches = check(&case.meta.expectations, &answer);

        // A case pinning a known defect is EXPECTED to disagree; a case that
        // claims a defect and then agrees is a defect that was fixed without
        // the pin being removed, which is just as much a stale test.
        match &case.meta.expectations.defect {
            Some(reason) => {
                if mismatches.is_empty() {
                    failures.push(format!(
                        "{}: pinned as a defect ({reason}) but the answer now MATCHES — \
                         the defect is fixed; delete the pin and author the right answer",
                        case.meta.id
                    ));
                }
            }
            None => {
                for m in &mismatches {
                    failures.push(format!("{}: {m}", case.meta.id));
                }
            }
        }

        // `free.len() == dof` is the S2 contract, in every case, free of charge.
        if solved.report.free.len() as u32 != solved.report.dof {
            failures.push(format!(
                "{}: free.len() is {} but dof is {}",
                case.meta.id,
                solved.report.free.len(),
                solved.report.dof,
            ));
        }
        // And every residual row indexes a constraint the sketch actually has.
        for row in &solved.report.residuals {
            if row.index as usize >= case.sketch.constraints.len() {
                failures.push(format!(
                    "{}: residual index {} is past the sketch's {} constraints",
                    case.meta.id,
                    row.index,
                    case.sketch.constraints.len(),
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// ── Tier 2: the agent door ──────────────────────────────────────────────────

/// Author the case's sketch through `sketch_create` and read it back through
/// `sketch_solve_state` — the S3 door, end to end, with no kernel.
fn engine_tier(case: &LoadedCase) -> Result<(Value, Value), String> {
    let mut state = wasm_bridge::EngineState::new();
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": "sketch-corpus" });

    let entities = serde_json::to_value(&case.sketch.entities).expect("entities serialize");
    let constraints =
        serde_json::to_value(&case.sketch.constraints).expect("constraints serialize");
    let args = json!({
        "plane": {
            "origin": case.sketch.plane_origin,
            "normal": case.sketch.plane_normal,
            "x_axis": case.sketch.plane_x_axis,
        },
        "entities": entities,
        "constraints": constraints,
        // A case that does not solve must still COMMIT, or there is nothing to
        // read back; the corpus holds over-constrained cases on purpose.
        "on_error": "keep",
    });
    let created = wasm_bridge::execute_tool(
        &mut state,
        &mut kernel,
        "sketch_create",
        &args,
        Some(&context),
    );
    if created.is_error {
        return Err(format!(
            "sketch_create refused: {}",
            created.structured_content["error"]
        ));
    }
    let feature_id = created.structured_content["feature_id"].clone();
    let read = wasm_bridge::execute_tool(
        &mut state,
        &mut kernel,
        "sketch_solve_state",
        &json!({ "feature_id": feature_id }),
        Some(&context),
    );
    if read.is_error {
        return Err(format!(
            "sketch_solve_state refused: {}",
            read.structured_content["error"]
        ));
    }
    Ok((created.structured_content, read.structured_content))
}

/// The door's `state` object, in the shape the expectations are written in.
fn answer_from_state(state: &Value, regions: &Value) -> TierAnswer {
    let u32s = |v: &Value| -> Vec<u32> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64().map(|n| n as u32))
                    .collect()
            })
            .unwrap_or_default()
    };
    let num = |key: &str| state[key].as_u64().unwrap_or_default() as u32;
    let mut positions = BTreeMap::new();
    if let Some(map) = state["positions"].as_object() {
        for (id, xy) in map {
            if let (Ok(id), Some(a)) = (id.parse::<u32>(), xy.as_array()) {
                if a.len() == 2 {
                    positions.insert(
                        id,
                        [
                            a[0].as_f64().unwrap_or_default(),
                            a[1].as_f64().unwrap_or_default(),
                        ],
                    );
                }
            }
        }
    }
    let mut radii = BTreeMap::new();
    if let Some(map) = state["radii"].as_object() {
        for (id, r) in map {
            if let (Ok(id), Some(r)) = (id.parse::<u32>(), r.as_f64()) {
                radii.insert(id, r);
            }
        }
    }
    // The door reports only `profile_entity_ids` per region (that is the
    // contract an agent extrudes against), so a sub-region arrives with a null
    // id list. The comparison below therefore uses the door's regions only for
    // the extrudable ones; the boundary identities are the pure tier's job.
    let mut out_regions: Vec<(Vec<u32>, bool, f64)> = regions
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| {
                    let ids = u32s(&r["profile_entity_ids"]);
                    let extrudable = r["profile_entity_ids"].is_array();
                    let mut ids = ids;
                    ids.sort_unstable();
                    (ids, extrudable, r["area_m2"].as_f64().unwrap_or_default())
                })
                .collect()
        })
        .unwrap_or_default();
    out_regions.sort_by(|a: &(Vec<u32>, bool, f64), b| a.0.cmp(&b.0));

    TierAnswer {
        status: state["status"].as_str().unwrap_or("Unsolved").to_string(),
        dof: num("dof"),
        params: num("params"),
        rows: num("rows"),
        rank: num("rank"),
        conflicts: u32s(&state["conflicts"]),
        redundant: u32s(&state["redundant"]),
        regions: out_regions,
        positions,
        radii,
    }
}

#[test]
fn the_agent_door_answers_what_the_solver_answers() {
    let mut failures: Vec<String> = Vec::new();
    for case in corpus() {
        let (created, read) = match engine_tier(&case) {
            Ok(pair) => pair,
            Err(e) => {
                failures.push(format!("{}: {e}", case.meta.id));
                continue;
            }
        };

        // `sketch_create` and `sketch_solve_state` must agree with each other:
        // one authors and the other re-solves the stored result, so a
        // difference means committing a sketch changed it.
        if created["state"] != read["state"] {
            failures.push(format!(
                "{}: sketch_create's state and sketch_solve_state's differ; \
                 committing the sketch changed it",
                case.meta.id
            ));
        }

        let expect = &case.meta.expectations;
        let answer = answer_from_state(&read["state"], &read["regions"]);

        // The structural numbers, against the authored answer.
        for (field, want, got) in [
            ("status", expect.status.clone(), answer.status.clone()),
            ("dof", expect.dof.to_string(), answer.dof.to_string()),
            (
                "params",
                expect.params.to_string(),
                answer.params.to_string(),
            ),
            ("rows", expect.rows.to_string(), answer.rows.to_string()),
            ("rank", expect.rank.to_string(), answer.rank.to_string()),
        ] {
            if want != got && expect.defect.is_none() {
                failures.push(format!(
                    "{}: door {field}: expected {want}, got {got}",
                    case.meta.id
                ));
            }
        }

        // Every EXTRUDABLE region the case authors must come back through the
        // door with the same ids and area, because that is what an agent passes
        // to a feature.
        for want in expect.regions.iter().filter(|r| r.extrudable) {
            let mut ids = want.entity_ids.clone();
            ids.sort_unstable();
            match answer.regions.iter().find(|(got, _, _)| *got == ids) {
                None => failures.push(format!(
                    "{}: the door reports no extrudable region {ids:?}; got {:?}",
                    case.meta.id,
                    answer.regions.iter().map(|(i, _, _)| i).collect::<Vec<_>>()
                )),
                Some((_, _, area)) => {
                    if (area - want.area).abs() > expect.area_rel_tol * want.area.abs() {
                        failures.push(format!(
                            "{}: the door's region {ids:?} has area {area:e}, expected {:e}",
                            case.meta.id, want.area
                        ));
                    }
                }
            }
        }

        // And the positions the case pins.
        for (id, want) in &expect.positions {
            match answer.positions.get(id) {
                None => failures.push(format!(
                    "{}: the door reports no position for point {id}",
                    case.meta.id
                )),
                Some(got) => {
                    let off = ((got[0] - want[0]).powi(2) + (got[1] - want[1]).powi(2)).sqrt();
                    if off > expect.positions_tol {
                        failures.push(format!(
                            "{}: the door's point {id} is at {got:?}, expected {want:?}",
                            case.meta.id
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// ── Tier 3: the independent oracle ──────────────────────────────────────────

#[test]
fn the_independent_oracle_agrees_about_every_case() {
    let mut failures: Vec<String> = Vec::new();
    let mut indeterminate: Vec<String> = Vec::new();

    for case in corpus() {
        let (solved, _) = pure_tier(&case.sketch);
        let oracle = analyze_at(&case.sketch, &solved.positions, &solved.radii);

        if !oracle.refused.is_empty() {
            failures.push(format!(
                "{}: the oracle could not compile {:?}, so it is describing a \
                 different system",
                case.meta.id, oracle.refused
            ));
            continue;
        }

        let expect = &case.meta.expectations;
        if oracle.params != expect.params {
            failures.push(format!(
                "{}: oracle params {} vs authored {}",
                case.meta.id, oracle.params, expect.params
            ));
        }
        if oracle.rows != expect.rows {
            failures.push(format!(
                "{}: oracle rows {} vs authored {}",
                case.meta.id, oracle.rows, expect.rows
            ));
        }
        // The oracle declines to decide a rank whose smallest retained singular
        // value sits within a decade of its threshold. That is reported, never
        // counted as a disagreement: an SVD threshold is a judgement call and
        // the oracle says so rather than pretending.
        match (oracle.rank.decided(), oracle.dof) {
            (Some(rank), Some(dof)) => {
                if rank != expect.rank {
                    failures.push(format!(
                        "{}: oracle rank {rank} vs authored {}",
                        case.meta.id, expect.rank
                    ));
                }
                if dof != expect.dof {
                    failures.push(format!(
                        "{}: oracle dof {dof} vs authored {}",
                        case.meta.id, expect.dof
                    ));
                }
            }
            _ => indeterminate.push(case.meta.id.clone()),
        }
    }

    if !indeterminate.is_empty() {
        eprintln!(
            "oracle returned Indeterminate for {} case(s): {}",
            indeterminate.len(),
            indeterminate.join(", ")
        );
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// ── The corpus itself ───────────────────────────────────────────────────────

#[test]
fn the_committed_corpus_is_what_the_generator_writes() {
    // The equivalent of the manifest-freshness check on the agent tools: a
    // case file edited by hand, or left behind when the generator changed, is
    // a case whose answer nobody derived.
    //
    // Re-derives each case's answer from its own stored sketch rather than
    // re-running the generator (which would need the generator's source as a
    // library): if the committed `.waffle` and the committed `.meta.json`
    // disagree, one of them was edited.
    for case in corpus() {
        let (_, answer) = pure_tier(&case.sketch);
        let mismatches = check(&case.meta.expectations, &answer);
        let expected_to_differ = case.meta.expectations.defect.is_some();
        assert_eq!(
            mismatches.is_empty(),
            !expected_to_differ,
            "{}: the committed document and metadata disagree{}: {}",
            case.meta.id,
            if expected_to_differ {
                " (and the case claims a defect, so they were supposed to)"
            } else {
                ""
            },
            mismatches
                .iter()
                .map(Mismatch::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
}

#[test]
fn the_corpus_covers_the_shapes_s4_asks_for() {
    // §10.4's named shapes. A corpus that drifts away from them is a corpus
    // that stopped testing what it was built for, and nothing else would say
    // so — the cases would simply all pass.
    let cases = corpus();
    let tags: Vec<String> = cases
        .iter()
        .flat_map(|c| c.meta.exercises.iter().cloned())
        .collect();
    for required in [
        "under-constrained",
        "fully-constrained",
        "over-constrained",
        "redundant",
        "reference-dimension",
        "scale",
        "curved",
        "two-regions",
        "conflicts",
        "index-space",
    ] {
        assert!(
            tags.iter().any(|t| t == required),
            "no case exercises {required:?}; the corpus covers {:?}",
            {
                let mut seen: Vec<&String> = tags.iter().collect();
                seen.sort();
                seen.dedup();
                seen
            }
        );
    }

    // Every verdict the solver can reach appears at least once, so the runner
    // exercises each branch of its own comparison.
    let statuses: Vec<&str> = cases
        .iter()
        .map(|c| c.meta.expectations.status.as_str())
        .collect();
    for verdict in ["UnderConstrained", "FullyConstrained", "OverConstrained"] {
        assert!(
            statuses.contains(&verdict),
            "no case ends {verdict}; got {statuses:?}"
        );
    }
}

#[test]
fn a_case_document_round_trips_through_the_file_format() {
    // The case files are real documents, and the app's dev API serves them into
    // the page (`app/tests/cases/manifest.json` names them
    // `sketch/<ID>.waffle`). A case the loader cannot read back as the same
    // sketch is a case the page would open as something else.
    //
    // Compared as DATA, not as bytes: `save_project` mints a document id and a
    // tab id and stamps `Utc::now()` on every call, and `solved_positions`
    // serializes a `HashMap` whose key order varies run to run. The generator
    // normalizes all four so the committed files are reproducible
    // (`stabilize`), but a plain re-save is not byte-identical and asserting
    // that it is only tests the clock.
    for case in corpus() {
        let path = repo_root()
            .join(CORPUS_DIR)
            .join(format!("{}.waffle", case.meta.id));
        let text = std::fs::read_to_string(&path).expect("the case document");
        let (tree, meta) = file_format::load_project(&text).expect("it loads");
        let again = file_format::save_project(&tree, &meta);
        let (tree2, _) = file_format::load_project(&again).expect("it loads again");

        assert_eq!(
            tree.features.len(),
            tree2.features.len(),
            "{}: feature count changed on re-save",
            case.meta.id
        );
        let sketch_again = match &tree2.features[0].operation {
            feature_engine::types::Operation::Sketch { sketch } => sketch.clone(),
            other => panic!("{}: not a Sketch after re-save: {other:?}", case.meta.id),
        };
        assert_eq!(
            serde_json::to_value(&sketch_again).unwrap(),
            serde_json::to_value(&case.sketch).unwrap(),
            "{}: the sketch did not survive a save/load round trip",
            case.meta.id
        );

        // And the answer survives with it: a document that loads to the same
        // sketch must still solve to the same verdict.
        let (_, answer) = pure_tier(&sketch_again);
        let mismatches = check(&case.meta.expectations, &answer);
        assert_eq!(
            mismatches.is_empty(),
            case.meta.expectations.defect.is_none(),
            "{}: after a round trip: {}",
            case.meta.id,
            mismatches
                .iter()
                .map(Mismatch::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

#[test]
fn the_committed_case_files_are_byte_reproducible() {
    // The generator normalizes the three values `save_project` draws from the
    // environment and sorts every key, so running it twice writes identical
    // bytes. Checked here against the COMMITTED files: re-stabilizing a case's
    // own loaded content must reproduce the file on disk exactly. A case that
    // fails this was hand-edited, or the generator's normalization drifted —
    // either way the file is no longer what the generator would write.
    for case in corpus() {
        let path = repo_root()
            .join(CORPUS_DIR)
            .join(format!("{}.waffle", case.meta.id));
        let committed = std::fs::read_to_string(&path).expect("the case document");
        let (tree, meta) = file_format::load_project(&committed).expect("it loads");
        let resaved = file_format::save_project(&tree, &meta);
        // The same normalization the generator applies, by hand: only the four
        // environment-derived fields and the key order may differ.
        let mut a: serde_json::Value = serde_json::from_str(&committed).unwrap();
        let mut b: serde_json::Value = serde_json::from_str(&resaved).unwrap();
        for doc in [&mut a, &mut b] {
            doc["document"]["id"] = serde_json::json!("");
            doc["document"]["created"] = serde_json::json!("");
            doc["document"]["modified"] = serde_json::json!("");
            doc["tabs"][0]["id"] = serde_json::json!("");
            doc["active_tab"] = serde_json::json!("");
        }
        assert_eq!(
            serde_json::to_string_pretty(&a).unwrap(),
            serde_json::to_string_pretty(&b).unwrap(),
            "{}: the committed file is not what saving its own content produces",
            case.meta.id
        );
    }
}

#[test]
fn the_corpus_summary() {
    // Not an assertion — the census. Printed with `--nocapture`, so a reader
    // can see what the corpus covers and what each case's verdict is without
    // opening thirteen files.
    let cases = corpus();
    println!("\n{:-<96}", "");
    println!(
        "{:<7} {:<18} {:>5} {:>7} {:>5} {:>5} {:>8}  exercises",
        "case", "status", "dof", "params", "rows", "rank", "regions"
    );
    println!("{:-<96}", "");
    let mut defects = 0;
    for case in &cases {
        let e = &case.meta.expectations;
        if e.defect.is_some() {
            defects += 1;
        }
        println!(
            "{:<7} {:<18} {:>5} {:>7} {:>5} {:>5} {:>8}  {}",
            case.meta.id,
            e.status,
            e.dof,
            e.params,
            e.rows,
            e.rank,
            e.regions.len(),
            case.meta.exercises.join(", "),
        );
    }
    println!("{:-<96}", "");
    println!(
        "{} cases, {} pinning a known defect\n",
        cases.len(),
        defects
    );

    // The three tiers, counted, so the summary says what actually ran.
    let mut pure_ok = 0;
    let mut oracle_ok = 0;
    for case in &cases {
        let (solved, answer) = pure_tier(&case.sketch);
        if check(&case.meta.expectations, &answer).is_empty()
            != case.meta.expectations.defect.is_some()
        {
            pure_ok += 1;
        }
        let oracle = analyze_at(&case.sketch, &solved.positions, &solved.radii);
        if oracle.rank.decided() == Some(case.meta.expectations.rank) {
            oracle_ok += 1;
        }
        // Keep the status tag honest about what the solver said, not what the
        // file claims.
        assert_eq!(
            status_tag(&solved.status),
            if case.meta.expectations.defect.is_some() {
                status_tag(&solved.status)
            } else {
                case.meta.expectations.status.clone()
            },
            "{}: status",
            case.meta.id
        );
    }
    println!("pure tier agreed on {pure_ok}/{} cases", cases.len());
    println!("oracle agreed on   {oracle_ok}/{} cases", cases.len());

    // The region census, which is what a profile defect would move.
    let total_regions: usize = cases
        .iter()
        .map(|c| c.meta.expectations.regions.len())
        .sum();
    let extrudable: usize = cases
        .iter()
        .flat_map(|c| c.meta.expectations.regions.iter())
        .filter(|r| r.extrudable)
        .count();
    println!("{total_regions} regions authored, {extrudable} of them extrudable\n");

    // And a last cross-check that the pure tier's region identities are the
    // ones the case authored, independent of the comparison above.
    for case in &cases {
        let (solved, _) = pure_tier(&case.sketch);
        let regions = regions_of(&case.sketch, &solved.positions);
        let got = regions_as_pairs(&regions);
        assert_eq!(
            got.len(),
            case.meta.expectations.regions.len(),
            "{}: region count",
            case.meta.id
        );
    }

    // The one shape that cannot be authored: a sketch with no driving
    // constraint must report `NotRun`, because no LM run happened.
    let free = cases
        .iter()
        .find(|c| c.sketch.constraints.is_empty())
        .expect("S0001 has no constraints");
    let solved = sketch_solver::solve_sketch(&free.sketch);
    assert!(
        matches!(solved.status, SolveStatus::UnderConstrained { .. }),
        "a sketch with no constraints is under-constrained"
    );
    assert_eq!(
        solved.report.convergence.termination,
        waffle_types::sketch_state::Termination::NotRun,
        "and no LM run happened"
    );
}

//! Determinism verification for boolean operations.
//!
//! Runs the same scenario multiple times and compares the result's
//! canonical, bit-exact B-Rep hash ([`super::geometry_hash`]) — and, as a
//! readable summary, its (V, E, F) counts. Counts alone were the original
//! check; two different solids with the same face count passed it.

use super::strategies::{BoolOp, BooleanScenario, SketchProfile, SolidBodySpec};
use crate::workflow::ModelBuilder;

/// Result of a determinism check.
#[derive(Debug, Clone)]
pub struct DeterminismResult {
    /// Whether all runs produced identical topology.
    pub is_deterministic: bool,
    /// Topology counts (V, E, F) from each run.
    pub run_topologies: Vec<(usize, usize, usize)>,
    /// Canonical geometry hash of the result from each successful run.
    pub run_hashes: Vec<u64>,
    /// Indices of runs that diverged from run 0.
    pub divergent_runs: Vec<usize>,
    /// Human-readable summary.
    pub detail: String,
}

/// Build a single body from a SolidBodySpec in the given ModelBuilder.
fn build_body(
    builder: &mut ModelBuilder,
    spec: &SolidBodySpec,
    sketch_name: &str,
    body_name: &str,
) -> Result<(), String> {
    match &spec.profile {
        SketchProfile::Rect(r) => {
            builder
                .rect_sketch(sketch_name, spec.origin, spec.normal, r.x, r.y, r.w, r.h)
                .map_err(|e| e.to_string())?;
        }
        SketchProfile::Circle(c) => {
            builder
                .circle_sketch(sketch_name, spec.origin, spec.normal, c.cx, c.cy, c.r)
                .map_err(|e| e.to_string())?;
        }
    }
    builder
        .extrude_no_merge(body_name, sketch_name, spec.depth)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Execute a BooleanScenario in a FRESH adapter and return the result's
/// topology counts and canonical geometry hash.
pub fn run_scenario(scenario: &BooleanScenario) -> Result<((usize, usize, usize), u64), String> {
    let mut builder = ModelBuilder::kernel_v2();

    build_body(&mut builder, &scenario.body_a, "sk_a", "body_a")?;
    build_body(&mut builder, &scenario.body_b, "sk_b", "body_b")?;

    let bool_result = match scenario.op {
        BoolOp::Union => builder.boolean_union("result", "body_a", "body_b"),
        BoolOp::Subtract => builder.boolean_subtract("result", "body_a", "body_b"),
        BoolOp::Intersect => builder.boolean_intersect("result", "body_a", "body_b"),
    };
    bool_result.map_err(|e| e.to_string())?;

    let topo = builder
        .topology_counts("result")
        .map_err(|e| e.to_string())?;
    let hash = super::geometry_hash::feature_geometry_hash(&builder, "result")
        .map_err(|e| e.to_string())?;
    Ok((topo, hash))
}

/// Run a BooleanScenario `runs` times and check that topology is deterministic.
pub fn check_determinism(scenario: &BooleanScenario, runs: usize) -> DeterminismResult {
    let mut topologies = Vec::with_capacity(runs);
    let mut hashes = Vec::with_capacity(runs);
    let mut errors = Vec::new();

    for i in 0..runs {
        match run_scenario(scenario) {
            Ok((topo, hash)) => {
                topologies.push(topo);
                hashes.push(hash);
            }
            Err(e) => {
                errors.push((i, e));
            }
        }
    }

    // If all runs errored, report as non-deterministic only if errors differ
    if topologies.is_empty() {
        return DeterminismResult {
            is_deterministic: errors.len() <= 1 || errors.windows(2).all(|w| w[0].1 == w[1].1),
            run_topologies: vec![],
            run_hashes: vec![],
            divergent_runs: vec![],
            detail: format!("All {} runs failed: {:?}", runs, errors),
        };
    }

    let reference = topologies[0];
    let reference_hash = hashes[0];
    let mut divergent = Vec::new();

    for (i, (topo, hash)) in topologies.iter().zip(&hashes).enumerate().skip(1) {
        if *topo != reference || *hash != reference_hash {
            divergent.push(i);
        }
    }

    let is_det = divergent.is_empty() && errors.is_empty();
    let detail = if is_det {
        format!(
            "Deterministic: V={} E={} F={} hash={:016x} across {} runs",
            reference.0, reference.1, reference.2, reference_hash, runs
        )
    } else {
        format!(
            "Non-deterministic! Reference V={} E={} F={}, hashes {:016x?}, topologies {:?}, divergent runs: {:?}, errors: {:?}",
            reference.0, reference.1, reference.2, hashes, topologies, divergent, errors
        )
    };

    DeterminismResult {
        is_deterministic: is_det,
        run_topologies: topologies,
        run_hashes: hashes,
        divergent_runs: divergent,
        detail,
    }
}

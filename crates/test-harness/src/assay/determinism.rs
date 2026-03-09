//! Determinism verification for boolean operations.
//!
//! Runs the same scenario multiple times and compares topology counts
//! to detect non-deterministic behavior.

use super::strategies::{BoolOp, BooleanScenario, SketchProfile, SolidBodySpec};
use crate::workflow::ModelBuilder;

/// Result of a determinism check.
#[derive(Debug, Clone)]
pub struct DeterminismResult {
    /// Whether all runs produced identical topology.
    pub is_deterministic: bool,
    /// Topology counts (V, E, F) from each run.
    pub run_topologies: Vec<(usize, usize, usize)>,
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

/// Execute a BooleanScenario and return topology counts of the result.
fn run_scenario(scenario: &BooleanScenario) -> Result<(usize, usize, usize), String> {
    let mut builder = ModelBuilder::kernel();

    build_body(&mut builder, &scenario.body_a, "sk_a", "body_a")?;
    build_body(&mut builder, &scenario.body_b, "sk_b", "body_b")?;

    let bool_result = match scenario.op {
        BoolOp::Union => builder.boolean_union("result", "body_a", "body_b"),
        BoolOp::Subtract => builder.boolean_subtract("result", "body_a", "body_b"),
        BoolOp::Intersect => builder.boolean_intersect("result", "body_a", "body_b"),
    };
    bool_result.map_err(|e| e.to_string())?;

    builder.topology_counts("result").map_err(|e| e.to_string())
}

/// Run a BooleanScenario `runs` times and check that topology is deterministic.
pub fn check_determinism(scenario: &BooleanScenario, runs: usize) -> DeterminismResult {
    let mut topologies = Vec::with_capacity(runs);
    let mut errors = Vec::new();

    for i in 0..runs {
        match run_scenario(scenario) {
            Ok(topo) => topologies.push(topo),
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
            divergent_runs: vec![],
            detail: format!("All {} runs failed: {:?}", runs, errors),
        };
    }

    let reference = topologies[0];
    let mut divergent = Vec::new();

    for (i, topo) in topologies.iter().enumerate().skip(1) {
        if *topo != reference {
            divergent.push(i);
        }
    }

    let is_det = divergent.is_empty() && errors.is_empty();
    let detail = if is_det {
        format!(
            "Deterministic: V={} E={} F={} across {} runs",
            reference.0, reference.1, reference.2, runs
        )
    } else {
        format!(
            "Non-deterministic! Reference V={} E={} F={}, divergent runs: {:?}, errors: {:?}",
            reference.0, reference.1, reference.2, divergent, errors
        )
    };

    DeterminismResult {
        is_deterministic: is_det,
        run_topologies: topologies,
        divergent_runs: divergent,
        detail,
    }
}

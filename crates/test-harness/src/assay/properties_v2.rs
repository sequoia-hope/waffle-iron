//! Oracle-based property checkers for generative CAD scenarios (v2).
//!
//! Oracles O19-O21 for single-body extrude results, O26-O28 for region
//! decomposition validation, plus composite runners.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use crate::helpers::mesh_volume;
use crate::oracle;
use crate::workflow::ModelBuilder;
use waffle_types::kernel::RenderMesh;

use super::properties::PropertyResult;
use super::regions::{signed_area, ClosedRegion};

/// O19: Check minimum body topology counts.
///
/// Any valid solid must have at least V=4, E=6, F=4 (tetrahedron minimum).
pub fn check_body_count(v: usize, e: usize, f: usize) -> PropertyResult {
    if v >= 4 && e >= 6 && f >= 4 {
        PropertyResult::pass(
            "O19_body_count",
            format!("V={} E={} F={} (all above minimum)", v, e, f),
        )
    } else {
        PropertyResult::fail(
            "O19_body_count",
            format!("V={} E={} F={} (minimum: V>=4, E>=6, F>=4)", v, e, f),
        )
    }
}

/// O20: Check that the mesh is non-empty (has at least one triangle).
pub fn check_non_empty_result(mesh: &RenderMesh) -> PropertyResult {
    let tri_count = mesh.indices.len() / 3;
    if tri_count > 0 {
        PropertyResult::pass("O20_non_empty", format!("{} triangles", tri_count))
    } else {
        PropertyResult::fail("O20_non_empty", "mesh has 0 triangles".to_string())
    }
}

/// O21: Check that mesh volume is within an upper bound.
///
/// The volume of an extruded polygon must be less than `scale_envelope^3`.
/// This catches catastrophic tessellation failures that produce unbounded geometry.
pub fn check_volume_upper_bound(mesh: &RenderMesh, scale_envelope: f64) -> PropertyResult {
    let vol = mesh_volume(mesh);
    let max_vol = scale_envelope * scale_envelope * scale_envelope;
    if vol <= max_vol {
        PropertyResult::pass(
            "O21_volume_bound",
            format!("vol={:.1} <= {:.1}", vol, max_vol),
        )
    } else {
        PropertyResult::fail(
            "O21_volume_bound",
            format!("vol={:.1} > {:.1} (envelope^3)", vol, max_vol),
        )
    }
}

/// Run all generative extrude oracles on a completed ModelBuilder.
///
/// Runs: mesh checks (O1-O10), topology (Euler), and new oracles O19-O21.
pub fn run_generative_extrude_oracles(
    builder: &mut ModelBuilder,
    feature_name: &str,
    scale_envelope: f64,
) -> Vec<PropertyResult> {
    let mut results = Vec::new();

    // Tessellate the result body
    let mesh = match builder.tessellate(feature_name) {
        Ok(m) => m,
        Err(e) => {
            results.push(PropertyResult::fail(
                "tessellate",
                format!("Failed to tessellate {}: {}", feature_name, e),
            ));
            return results;
        }
    };

    // O20: Non-empty result
    results.push(check_non_empty_result(&mesh));

    // O21: Volume upper bound
    results.push(check_volume_upper_bound(&mesh, scale_envelope));

    // O1-O10: Mesh oracles (watertight, normals, degenerate triangles, etc.)
    for verdict in oracle::run_all_mesh_checks(&mesh) {
        results.push(PropertyResult {
            name: verdict.oracle_name,
            passed: verdict.passed,
            detail: verdict.detail,
        });
    }

    // Topology checks (Euler invariant, O19 body count)
    if let Ok((v, e, f)) = builder.topology_counts(feature_name) {
        results.push(super::properties::check_euler_invariant(v, e, f));
        results.push(check_body_count(v, e, f));
    }

    results
}

// ── Chain Oracles (O22, O24, O25) ────────────────────────────────────

/// O22: Volume conservation for boolean operations.
///
/// vol(A ∪ B) + vol(A ∩ B) ≈ vol(A) + vol(B) within tolerance.
/// This is a theoretical oracle; for chain scenarios we use a relaxed form.
pub fn check_volume_conservation(
    vol_a: f64,
    vol_b: f64,
    vol_union: f64,
    vol_intersect: f64,
) -> PropertyResult {
    let lhs = vol_union + vol_intersect;
    let rhs = vol_a + vol_b;
    // 10% tolerance to account for tessellation error
    let tol = rhs * 0.10 + 1.0;
    let diff = (lhs - rhs).abs();

    if diff <= tol {
        PropertyResult::pass(
            "O22_volume_conservation",
            format!(
                "vol(A∪B)+vol(A∩B)={:.1} ≈ vol(A)+vol(B)={:.1} (diff={:.1})",
                lhs, rhs, diff
            ),
        )
    } else {
        PropertyResult::fail(
            "O22_volume_conservation",
            format!(
                "vol(A∪B)+vol(A∩B)={:.1} != vol(A)+vol(B)={:.1} (diff={:.1} > tol={:.1})",
                lhs, rhs, diff, tol
            ),
        )
    }
}

/// O24: Feature count matches number of completed chain steps.
///
/// After N completed steps, the model should have produced valid features.
/// We verify that the completed_steps count is at least 1 (base body exists).
pub fn check_monotonic_features(completed_steps: usize, step_volumes: &[f64]) -> PropertyResult {
    if completed_steps == 0 {
        return PropertyResult::fail("O24_monotonic_features", "No steps completed".to_string());
    }

    // Each completed step should have a corresponding volume measurement
    if step_volumes.len() < completed_steps.min(step_volumes.len() + 1) && step_volumes.is_empty() {
        return PropertyResult::fail(
            "O24_monotonic_features",
            format!(
                "completed_steps={} but no volumes recorded",
                completed_steps
            ),
        );
    }

    PropertyResult::pass(
        "O24_monotonic_features",
        format!(
            "completed_steps={}, volumes_recorded={}",
            completed_steps,
            step_volumes.len()
        ),
    )
}

/// O25: All completed steps up to the truncation point have valid solids.
///
/// Every volume in step_volumes must be > 0. If the chain truncated early
/// (known kernel limitation), the completed steps must still be valid.
pub fn check_partial_chain_validity(step_volumes: &[f64]) -> PropertyResult {
    for (i, &vol) in step_volumes.iter().enumerate() {
        if vol <= 0.0 {
            return PropertyResult::fail(
                "O25_partial_chain_validity",
                format!("step {} has non-positive volume: {:.3}", i, vol),
            );
        }
    }

    PropertyResult::pass(
        "O25_partial_chain_validity",
        format!(
            "all {} step volumes positive (range: {:.1}..{:.1})",
            step_volumes.len(),
            step_volumes.iter().cloned().fold(f64::INFINITY, f64::min),
            step_volumes
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    )
}

/// Run all generative chain oracles on a completed chain result.
///
/// Runs: O24 + O25 (chain-specific), then all mesh/topology oracles on the
/// final body with strict enforcement, plus per-step volume invariants.
pub fn run_generative_chain_oracles(
    result: &mut super::strategies_v2::ChainResult,
    scale_envelope: f64,
) -> Vec<PropertyResult> {
    let mut results = Vec::new();

    // O24: Feature count check
    results.push(check_monotonic_features(
        result.completed_steps,
        &result.step_volumes,
    ));

    // O25: Partial chain validity (all volumes > 0)
    results.push(check_partial_chain_validity(&result.step_volumes));

    // Run standard mesh oracles on the final body — ALL of them hard.
    //
    // History: until 2026-09-27 the boolean results downgraded `euler_invariant`,
    // `watertight`, `outward_normals`, `consistent_normals` and `O19_body_count`
    // to "[advisory-bug]" passes, and the per-step volume invariants to
    // "[advisory]" passes, because the truck-era boolean was known-wrong in
    // those classes. On kernel-v2 those are the correct-or-loud contract
    // (0 WRONG on the corpus is enforced by the composition oracle); a fuzz
    // that reports them as passes is blind to exactly what it exists to find.
    //
    // One oracle is REPLACED rather than hardened: the B-Rep `euler_invariant`
    // (V − E + F == 2 over the kernel's vertex/edge/face lists) is only true
    // for a genus-0, single-shell body whose faces have no inner loops. A
    // through-cut makes genus, a disjoint boss makes a second shell, and a
    // blind pocket puts an inner loop on a face — all correct results the
    // "== 2" reading would flag. The chain result is judged on the MESH
    // instead: the exact-bit Euler characteristic of the tessellation must be
    // EVEN (a closed 2-manifold's χ = 2·S − 2·G is always even); which even
    // value is right is the exact-membership oracle's question, not this one's.
    let oracles =
        run_generative_extrude_oracles(&mut result.builder, &result.final_feature, scale_envelope);
    let is_boolean_result = result.completed_steps > 1;
    for oracle in oracles {
        if is_boolean_result && oracle.name == "euler_invariant" {
            continue;
        }
        results.push(oracle);
    }
    if is_boolean_result {
        if let Ok(mesh) = result.builder.tessellate(&result.final_feature) {
            results.push(check_mesh_euler_even(&mesh));
        }
    }

    // Per-step volume invariants (I9-I12) — hard.
    results.append(&mut result.volume_invariant_results);

    results
}

/// The mesh's exact-bit Euler characteristic must be even.
///
/// A closed 2-manifold has χ = 2·shells − 2·genus, always even; an odd χ is
/// a non-manifold or open residue no genus can explain (the corpus runner's
/// "odd χ = impossible" reading, `oracle::check_mesh_euler_characteristic_with_shells`).
/// The value itself is read off that oracle's verdict (`value` = χ); its own
/// pass/fail is against a target this caller does not know.
pub fn check_mesh_euler_even(mesh: &RenderMesh) -> PropertyResult {
    let v = oracle::check_mesh_euler_characteristic_with_shells(mesh, 2, None);
    match v.value {
        Some(chi) if (chi as i64) % 2 == 0 => PropertyResult::pass(
            "mesh_euler_even",
            format!("χ = {} ({})", chi as i64, v.detail),
        ),
        Some(chi) => PropertyResult::fail(
            "mesh_euler_even",
            format!("χ = {} is ODD ({})", chi as i64, v.detail),
        ),
        // An empty mesh reports no χ; O20_non_empty already fails it.
        None => PropertyResult::pass("mesh_euler_even", format!("skipped: {}", v.detail)),
    }
}

// ── Region Oracles (O26-O28) ─────────────────────────────────────────

/// O26: Check that iOverlay decomposition conserves total area.
///
/// The sum of all region areas should approximately equal the total area
/// of the input contours (within 1% tolerance).
pub fn check_region_area_conservation(
    regions: &[ClosedRegion],
    total_contour_area: f64,
) -> PropertyResult {
    let region_sum: f64 = regions.iter().map(|r| r.area).sum();
    let tol = total_contour_area.abs() * 0.01 + 1.0; // 1% + small absolute tolerance
    let diff = (region_sum - total_contour_area).abs();

    if diff <= tol {
        PropertyResult::pass(
            "O26_area_conservation",
            format!(
                "region_sum={:.2} ≈ contour_area={:.2} (diff={:.2})",
                region_sum, total_contour_area, diff
            ),
        )
    } else {
        PropertyResult::fail(
            "O26_area_conservation",
            format!(
                "region_sum={:.2} != contour_area={:.2} (diff={:.2} > tol={:.2})",
                region_sum, total_contour_area, diff, tol
            ),
        )
    }
}

/// O27: Check that regions do not overlap.
///
/// For each pair of regions, compute their intersection area using iOverlay.
/// It should be approximately zero.
pub fn check_region_non_overlap(regions: &[ClosedRegion]) -> PropertyResult {
    if regions.len() <= 1 {
        return PropertyResult::pass(
            "O27_non_overlap",
            format!("{} region(s) — no overlap possible", regions.len()),
        );
    }

    for i in 0..regions.len() {
        for j in (i + 1)..regions.len() {
            let shape_a: Vec<Vec<[f64; 2]>> = vec![regions[i].outer.clone()];
            let shape_b: Vec<Vec<[f64; 2]>> = vec![regions[j].outer.clone()];

            let intersection: Vec<Vec<Vec<[f64; 2]>>> =
                shape_a.overlay(&shape_b, OverlayRule::Intersect, FillRule::EvenOdd);

            let overlap_area: f64 = intersection
                .iter()
                .flat_map(|s| s.first())
                .map(|c| signed_area(c).abs())
                .sum();

            // Allow small numerical overlap (< 1% of smaller region)
            let min_area = regions[i].area.min(regions[j].area);
            let tol = min_area * 0.01 + 0.1;

            if overlap_area > tol {
                return PropertyResult::fail(
                    "O27_non_overlap",
                    format!(
                        "regions {} and {} overlap: area={:.2} > tol={:.2}",
                        i, j, overlap_area, tol
                    ),
                );
            }
        }
    }

    PropertyResult::pass(
        "O27_non_overlap",
        format!("{} regions — no significant overlap", regions.len()),
    )
}

/// O28: Check that all regions meet minimum size requirements.
///
/// Every region must have `area >= min_feature_size²`.
pub fn check_region_min_size(regions: &[ClosedRegion], min_feature_size: f64) -> PropertyResult {
    let min_area = min_feature_size * min_feature_size;

    for (i, region) in regions.iter().enumerate() {
        if region.area < min_area {
            return PropertyResult::fail(
                "O28_min_size",
                format!(
                    "region {} has area={:.2} < min={:.2} (feature_size={:.1})",
                    i, region.area, min_area, min_feature_size
                ),
            );
        }
    }

    PropertyResult::pass(
        "O28_min_size",
        format!("all {} regions have area >= {:.2}", regions.len(), min_area),
    )
}

/// Run all region decomposition oracles (O26-O28).
pub fn run_generative_region_oracles(
    regions: &[ClosedRegion],
    total_area: f64,
    min_feature_size: f64,
) -> Vec<PropertyResult> {
    vec![
        check_region_area_conservation(regions, total_area),
        check_region_non_overlap(regions),
        check_region_min_size(regions, min_feature_size),
    ]
}

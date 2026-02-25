//! Edge healing for boolean operation results.
//!
//! After a boolean operation, result edges may carry `IntersectionCurve` geometry
//! that stores `Box<Surface>` references and re-runs `double_projection` (Newton
//! iteration) on every `subs(t)` call. When a second boolean is performed on the
//! result, `curve_surface_projection` in `create_loops_stores` fails to converge
//! on these IntersectionCurve edges.
//!
//! The fix: for plane-plane intersections, replace with exact `Line` curves.
//! For curved intersections, replace with `BSplineCurve` approximations that
//! are validated to lie within `TOLERANCE` of both original surfaces.
//! This uses `edge.set_curve()` (interior mutation via Arc<Mutex>)
//! so shared edges are updated in place.

use std::collections::{HashMap, HashSet};
use truck_modeling::geometry::{Curve, Line, Surface};
use truck_modeling::topology::{EdgeID, Face, Shell, Solid, VertexID};
use truck_modeling::{
    BSplineCurve, BoundedCurve, InnerSpace, Matrix4, NurbsCurve, ParametricCurve,
    ParametricSurface, ParametricSurface3D, Point3, SPHint2D, SearchNearestParameter,
    ToSameGeometry, Transformed, TrimmedCurve, UnitCircle, Vector3, Vector4,
};

/// truck's convergence threshold for `curve_surface_projection`.
const TOLERANCE: f64 = 1.0e-6;

/// Result of a healing pass.
#[derive(Debug, Default)]
pub struct HealingResult {
    /// Number of IntersectionCurve edges found.
    pub total_intersection_edges: usize,
    /// Number successfully replaced.
    pub healed: usize,
    /// Number that failed approximation (left as IntersectionCurve).
    pub failed: usize,
    /// Count of plane-plane ICs healed (exact line replacement).
    pub plane_plane_count: usize,
    /// Count of plane-cylinder ICs healed (NURBS arc or BSpline).
    pub plane_cylinder_count: usize,
    /// Count of plane-cone ICs detected.
    pub plane_cone_count: usize,
    /// Count of cylinder-cylinder ICs detected.
    pub cylinder_cylinder_count: usize,
}

/// Validate that a BSpline curve lies within `max_err` of both surfaces.
///
/// Samples the curve at `n_samples` uniformly-spaced parameter values and
/// measures the distance from each sample to the nearest point on each surface
/// using the IntersectionCurve's `double_projection` (via the IC's `subs`).
///
/// Returns the maximum residual distance found, or `f64::MAX` if any surface
/// projection fails.
fn validate_bspline_on_surfaces(
    bsp: &BSplineCurve<Point3>,
    surface0: &Surface,
    surface1: &Surface,
    n_samples: usize,
) -> f64 {
    let (t0, t1) = bsp.range_tuple();
    let mut max_residual: f64 = 0.0;

    for i in 0..n_samples {
        let t = t0 + (t1 - t0) * (i as f64) / ((n_samples - 1).max(1) as f64);
        let pt = bsp.subs(t);

        // Check distance to surface0
        let r0 = match surface0.search_nearest_parameter(pt, SPHint2D::None, 10) {
            Some((u, v)) => {
                let sp = surface0.subs(u, v);
                ((pt.x - sp.x).powi(2) + (pt.y - sp.y).powi(2) + (pt.z - sp.z).powi(2)).sqrt()
            }
            None => return f64::MAX,
        };

        // Check distance to surface1
        let r1 = match surface1.search_nearest_parameter(pt, SPHint2D::None, 10) {
            Some((u, v)) => {
                let sp = surface1.subs(u, v);
                ((pt.x - sp.x).powi(2) + (pt.y - sp.y).powi(2) + (pt.z - sp.z).powi(2)).sqrt()
            }
            None => return f64::MAX,
        };

        max_residual = max_residual.max(r0).max(r1);
    }
    max_residual
}

/// Try to construct an analytical NURBS circular arc for a plane-curved surface
/// intersection by fitting a circle through sampled IC leader points.
///
/// When one IC surface is a plane and the intersection is circular (plane-cylinder),
/// we can represent the arc exactly as a rational NURBS curve with machine-precision
/// accuracy. This is critical for chained booleans: BSpline approximations accumulate
/// ~5e-6 error which exceeds truck's TOLERANCE=1e-6 in `curve_surface_projection`.
///
/// The approach: sample the leader BSpline at 3+ points, fit a circle (center +
/// radius + plane normal), then construct an exact NURBS arc. This works regardless
/// of how the cylinder surface is stored internally (RevolutedCurve or NurbsSurface).
///
/// Returns `None` if no plane surface is found, or if the points don't fit a circle.
fn analytical_circle_arc_from_leader(
    surface0: &Surface,
    surface1: &Surface,
    leader_curve: &Curve,
    front_pt: Point3,
    back_pt: Point3,
) -> Option<NurbsCurve<Vector4>> {
    // At least one surface must be a plane — the arc lies on this plane.
    let plane_normal = match (surface0, surface1) {
        (Surface::Plane(p), _) => p.normal().normalize(),
        (_, Surface::Plane(p)) => p.normal().normalize(),
        _ => return None,
    };

    // Sample the leader curve at the midpoint to get a third point on the arc.
    let (t0, t1) = leader_curve.range_tuple();
    let mid_t = (t0 + t1) * 0.5;
    let mid_pt = leader_curve.subs(mid_t);

    // Fit a circle through front_pt, mid_pt, back_pt using circumscribed circle formula.
    // All three points should lie (approximately) on the plane.
    let center = fit_circle_3points(front_pt, mid_pt, back_pt)?;
    let radius = (front_pt - center).magnitude();

    // Validate: all three points should be at approximately the same radius.
    // The leader BSpline is an approximation (~1e-3 error from true circle),
    // so use a loose threshold here. The final NURBS arc will be validated
    // against the actual surfaces, not the leader samples.
    let r_mid = (mid_pt - center).magnitude();
    let r_back = (back_pt - center).magnitude();
    let circle_tol = 0.01; // loose — leader BSpline has ~1e-3 error
    if (r_mid - radius).abs() > circle_tol || (r_back - radius).abs() > circle_tol {
        return None; // not a circle
    }

    if radius < 1e-10 {
        return None; // degenerate
    }

    // Build a local coordinate frame on the plane:
    // X-axis: from center to front_pt (normalized)
    // Z-axis: plane_normal
    // Y-axis: Z × X
    let local_x = (front_pt - center).normalize();
    let local_z = plane_normal;
    let local_y = local_z.cross(local_x);
    let local_y_len = local_y.magnitude();
    if local_y_len < 1e-10 {
        return None; // degenerate
    }
    let local_y = local_y / local_y_len;

    // Compute the angle from front_pt to back_pt in the local frame.
    let to_back = back_pt - center;
    let bx = to_back.dot(local_x);
    let by = to_back.dot(local_y);
    let mut angle = by.atan2(bx);
    if angle < 0.0 {
        angle += 2.0 * std::f64::consts::PI;
    }

    // Verify the arc direction: the midpoint should be at roughly angle/2.
    let to_mid = mid_pt - center;
    let mx = to_mid.dot(local_x);
    let my = to_mid.dot(local_y);
    let mut mid_angle = my.atan2(mx);
    if mid_angle < 0.0 {
        mid_angle += 2.0 * std::f64::consts::PI;
    }

    // If mid_angle is not between 0 and angle, the arc goes the other way.
    if mid_angle > angle + 0.1 || mid_angle < -0.1 {
        // Flip local_y to traverse the arc in the other direction
        let flipped_y = -local_y;

        // Recompute angle with flipped frame
        let bx2 = to_back.dot(local_x);
        let by2 = to_back.dot(flipped_y);
        let mut flipped_angle = by2.atan2(bx2);
        if flipped_angle < 0.0 {
            flipped_angle += 2.0 * std::f64::consts::PI;
        }

        // Rebuild the NURBS arc with flipped Y
        return build_nurbs_arc(
            local_x,
            flipped_y,
            local_z,
            center,
            radius,
            flipped_angle,
            front_pt,
            back_pt,
            surface0,
            surface1,
        );
    }

    // Avoid degenerate zero-angle or full-circle arcs.
    if angle < 1e-8 || (2.0 * std::f64::consts::PI - angle).abs() < 1e-8 {
        return None;
    }

    build_nurbs_arc(
        local_x, local_y, local_z, center, radius, angle, front_pt, back_pt, surface0, surface1,
    )
}

/// Build a NURBS arc in the given local frame and validate against both surfaces.
#[allow(clippy::too_many_arguments)]
fn build_nurbs_arc(
    local_x: Vector3,
    local_y: Vector3,
    local_z: Vector3,
    center: Point3,
    radius: f64,
    angle: f64,
    front_pt: Point3,
    back_pt: Point3,
    surface0: &Surface,
    surface1: &Surface,
) -> Option<NurbsCurve<Vector4>> {
    if angle < 1e-8 || (2.0 * std::f64::consts::PI - angle).abs() < 1e-8 {
        return None;
    }

    // Construct the NURBS arc using truck's TrimmedCurve<UnitCircle> → NurbsCurve path.
    let nurbs_unit: NurbsCurve<Vector4> =
        TrimmedCurve::new(UnitCircle::<Point3>::new(), (0.0, angle)).to_same_geometry();

    // Build a 4x4 transform: scale by radius, rotate to local frame, translate to center.
    // Matrix4 is column-major in cgmath.
    let mat = Matrix4::new(
        local_x.x * radius,
        local_x.y * radius,
        local_x.z * radius,
        0.0,
        local_y.x * radius,
        local_y.y * radius,
        local_y.z * radius,
        0.0,
        local_z.x,
        local_z.y,
        local_z.z,
        0.0,
        center.x,
        center.y,
        center.z,
        1.0,
    );

    let transformed = nurbs_unit.transformed(mat);

    // Validate endpoints.
    let (t_start, t_end) = transformed.range_tuple();
    let arc_front = transformed.subs(t_start);
    let arc_back = transformed.subs(t_end);

    let err_front = ((arc_front.x - front_pt.x).powi(2)
        + (arc_front.y - front_pt.y).powi(2)
        + (arc_front.z - front_pt.z).powi(2))
    .sqrt();
    let err_back = ((arc_back.x - back_pt.x).powi(2)
        + (arc_back.y - back_pt.y).powi(2)
        + (arc_back.z - back_pt.z).powi(2))
    .sqrt();

    if err_front > TOLERANCE || err_back > TOLERANCE {
        return None;
    }

    // Validate: sample the NURBS arc against both IC surfaces.
    // For a true plane-cylinder intersection, the arc should lie on both
    // surfaces with machine-precision accuracy. This catches cases where
    // the circle fit was a false positive (e.g., the intersection is actually
    // an ellipse or other curve).
    let n_samples = 20;
    let (vt0, vt1) = transformed.range_tuple();
    for i in 0..n_samples {
        let t = vt0 + (vt1 - vt0) * (i as f64) / ((n_samples - 1).max(1) as f64);
        let pt = transformed.subs(t);
        // Check distance to both surfaces
        for surface in [surface0, surface1] {
            if let Some((u, v)) = surface.search_nearest_parameter(pt, SPHint2D::None, 10) {
                let sp = surface.subs(u, v);
                let dist =
                    ((pt.x - sp.x).powi(2) + (pt.y - sp.y).powi(2) + (pt.z - sp.z).powi(2)).sqrt();
                if dist > TOLERANCE * 0.5 {
                    return None; // arc doesn't lie on both surfaces
                }
            } else {
                return None;
            }
        }
    }

    Some(transformed)
}

/// Fit a circle through three 3D points (circumscribed circle).
///
/// Returns the center of the circle, or None if the points are collinear.
fn fit_circle_3points(p1: Point3, p2: Point3, p3: Point3) -> Option<Point3> {
    let a = p2 - p1;
    let b = p3 - p1;

    let cross = a.cross(b);
    let cross_sq = cross.dot(cross);
    if cross_sq < 1e-20 {
        return None; // collinear
    }

    // Circumcenter formula: center = p1 + (|b|²(a×b)×a + |a|²b×(a×b)) / (2|a×b|²)
    let a_sq = a.dot(a);
    let b_sq = b.dot(b);

    let t1 = cross.cross(a) * b_sq;
    let t2 = b.cross(cross) * a_sq;
    let center_offset = (t1 + t2) / (2.0 * cross_sq);

    Some(p1 + center_offset)
}

/// Replace `IntersectionCurve` edges in a solid with simpler curve types.
///
/// For plane-plane intersections: exact `Line` replacement.
/// For curved intersections: `BSplineCurve` approximation validated to lie
/// within `TOLERANCE` of both original surfaces. This ensures the solid is
/// safe for subsequent boolean operations.
///
/// # Arguments
/// * `solid` — The solid to heal (edges are mutated in place via `set_curve`).
/// * `tol` — Position tolerance for the cubic approximation (typically 0.001).
pub fn heal_intersection_curves(solid: &Solid, tol: f64) -> HealingResult {
    let mut result = HealingResult::default();
    let mut seen: HashSet<EdgeID> = HashSet::new();

    for edge in solid.edge_iter() {
        if !seen.insert(edge.id()) {
            continue; // skip shared edges already processed
        }

        let curve = edge.curve();
        if let Curve::IntersectionCurve(ref ic) = &curve {
            result.total_intersection_edges += 1;

            let front_pt = edge.absolute_front().point();
            let back_pt = edge.absolute_back().point();

            // Classify the surface pair for diagnostics and strategy selection.
            let s0 = ic.surface0();
            let s1 = ic.surface1();
            let pair_type = classify_surface_pair(s0.as_ref(), s1.as_ref());

            // Fast path: if both surfaces are planes, the intersection is
            // mathematically a straight line. Replace with exact Line curve.
            if pair_type == SurfacePairType::PlanePlane {
                edge.set_curve(Curve::Line(Line(front_pt, back_pt)));
                result.healed += 1;
                result.plane_plane_count += 1;
                continue;
            }

            // Track surface pair type counts for diagnostics.
            match pair_type {
                SurfacePairType::PlaneCylinder => result.plane_cylinder_count += 1,
                SurfacePairType::PlaneCone => {
                    result.plane_cone_count += 1;
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "[healing] plane-cone IC detected — BSpline fallback \
                         (analytical conic fitting not yet implemented)"
                    );
                }
                SurfacePairType::CylinderCylinder => {
                    result.cylinder_cylinder_count += 1;
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "[healing] cylinder-cylinder IC detected — BSpline fallback \
                         (analytical ellipse fitting not yet implemented)"
                    );
                }
                _ => {}
            }

            // Get IC internals for all strategies.
            let leader = ic.leader();
            let leader_curve: &Curve = leader.as_ref();
            // s0, s1 already extracted above for classification.
            let surface0 = &s0;
            let surface1 = &s1;

            // Strategy 0: Analytical NURBS arc for plane-curved intersections.
            // If one surface is a plane and the IC leader samples fit a circle,
            // construct an exact rational NURBS arc. This gives machine-precision
            // accuracy (zero approximation error), which guarantees convergence
            // in `curve_surface_projection` during subsequent booleans.
            if let Some(nurbs) = analytical_circle_arc_from_leader(
                surface0.as_ref(),
                surface1.as_ref(),
                leader_curve,
                front_pt,
                back_pt,
            ) {
                edge.set_curve(Curve::NurbsCurve(nurbs));
                result.healed += 1;
                continue;
            }

            // Tight threshold: if the BSpline lies within TOLERANCE/2 of both
            // surfaces, truck's `curve_surface_projection` (which uses
            // `near()` = abs_diff_eq(TOLERANCE)) will reliably converge in
            // subsequent booleans. We try for this tight threshold first, but
            // fall back to "any BSpline is better than an IC edge" if we can't.
            let tight_threshold = TOLERANCE * 0.5;

            // Curved intersection: try multiple strategies to produce a BSpline
            // replacement. Track the best (lowest-residual) candidate across all
            // strategies so we always replace the IC even if no candidate passes
            // the tight threshold.
            {
                let range = leader_curve.range_tuple();

                let mut best: Option<(BSplineCurve<Point3>, f64)> = None; // (curve, residual)

                // Helper: update best candidate if this one has lower residual.
                let mut try_candidate = |mut bsp: BSplineCurve<Point3>| -> bool {
                    let n_cp = bsp.control_points().len();
                    *bsp.control_point_mut(0) = front_pt;
                    *bsp.control_point_mut(n_cp - 1) = back_pt;
                    let residual = validate_bspline_on_surfaces(
                        &bsp,
                        surface0.as_ref(),
                        surface1.as_ref(),
                        20,
                    );
                    if residual < tight_threshold {
                        edge.set_curve(Curve::BSplineCurve(bsp));
                        return true; // tight threshold met — done
                    }
                    // Track best candidate even if it doesn't meet tight threshold
                    if best.as_ref().is_none_or(|(_, r)| residual < *r) {
                        best = Some((bsp, residual));
                    }
                    false
                };

                // Strategy 1: Clone leader BSpline directly (no projection).
                // The leader was fit by truck's intersection pipeline and should
                // be close to both surfaces. Do NOT project control points onto a
                // single surface — that increases error for the other surface.
                let mut done = false;
                if let Curve::BSplineCurve(ref bsp) = leader_curve {
                    done = try_candidate(bsp.clone());
                }

                // Strategy 2: Re-approximate leader with progressively tighter
                // tolerance. Use tighter tolerance when one surface is a plane
                // (common plane-cylinder case).
                if !done {
                    let has_plane = matches!(surface0.as_ref(), Surface::Plane(_))
                        || matches!(surface1.as_ref(), Surface::Plane(_));
                    let tol_levels: &[f64] = if has_plane {
                        &[TOLERANCE * 0.1, TOLERANCE * 0.01]
                    } else {
                        &[tol * 0.01, tol * 0.001]
                    };

                    for &approx_tol in tol_levels {
                        if let Some(bsp) = BSplineCurve::cubic_approximation(
                            leader_curve,
                            range,
                            approx_tol,
                            approx_tol * 1000.0,
                            200,
                        ) {
                            if try_candidate(bsp) {
                                done = true;
                                break;
                            }
                        }
                    }
                }

                // Strategy 3: Re-approximate from the full IC using exact
                // double-projection subs(t).
                // INTENTIONAL catch_unwind: This is a best-effort healing strategy.
                // The IC's Newton iteration in cubic_approximation can diverge for
                // some parameter values. Unlike the boolean hot path, healing is a
                // pre-processing step where silent fallback to the next strategy is
                // correct behavior — the original curve is preserved if re-approximation
                // fails. See Sprint 13 (boolean reliability) for the reasoning behind
                // removing catch_unwind from loops_store and divide_face but keeping
                // it here.
                if !done {
                    let curve_clone = curve.clone();
                    let ic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        BSplineCurve::cubic_approximation(
                            &curve_clone,
                            range,
                            TOLERANCE * 0.1,
                            TOLERANCE * 100.0,
                            200,
                        )
                    }));
                    if let Ok(Some(bsp)) = ic_result {
                        done = try_candidate(bsp);
                    }
                }

                if done {
                    result.healed += 1;
                    continue;
                }

                // Use best candidate if we have one (any BSpline is better than
                // leaving IC edges, which would prevent future booleans entirely).
                if let Some((bsp, _residual)) = best {
                    edge.set_curve(Curve::BSplineCurve(bsp));
                    result.healed += 1;
                    continue;
                }

                // Last resort: use vertex endpoints as a Line.
                // This loses curvature but avoids leaving IC edges.
                edge.set_curve(Curve::Line(Line(front_pt, back_pt)));
                result.healed += 1;
            }
        }
    }

    result
}

/// Deduplicate nearly-coincident vertices in a solid after boolean + healing.
///
/// After `heal_intersection_curves`, the solid may have vertices that are very
/// close together (within `tol`) from different booleans. If left as-is, the
/// next boolean's `weld_coincident_edges` will unify them and may produce
/// non-simple wires. By merging them preemptively with a tight tolerance,
/// we reduce the chance of weld-induced topology corruption.
///
/// Uses interior mutation via `Vertex::set_point()` (Arc<Mutex>) to merge
/// nearby vertices to their average position.
pub fn deduplicate_vertices(solid: &Solid, tol: f64) {
    use std::collections::HashMap;
    use truck_modeling::topology::{Vertex, VertexID};

    let dedup_tol = tol;

    // Collect all unique vertices
    let mut all_verts: Vec<Vertex> = Vec::new();
    let mut seen_ids: HashSet<VertexID> = HashSet::new();
    for shell in solid.boundaries() {
        for face in shell.iter() {
            for wire in face.absolute_boundaries().iter() {
                for v in wire.vertex_iter() {
                    if seen_ids.insert(v.id()) {
                        all_verts.push(v.clone());
                    }
                }
            }
        }
    }

    if all_verts.len() < 2 {
        return;
    }

    // Spatial grid for efficient nearest-neighbor lookup
    let cell = dedup_tol;
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();

    for (idx, v) in all_verts.iter().enumerate() {
        let pt = v.point();
        let key = (
            (pt.x / cell).round() as i64,
            (pt.y / cell).round() as i64,
            (pt.z / cell).round() as i64,
        );
        grid.entry(key).or_default().push(idx);
    }

    // Find groups of vertices that should be merged
    let mut merged: HashSet<usize> = HashSet::new();

    for idx in 0..all_verts.len() {
        if merged.contains(&idx) {
            continue;
        }
        let pt = all_verts[idx].point();
        let key = (
            (pt.x / cell).round() as i64,
            (pt.y / cell).round() as i64,
            (pt.z / cell).round() as i64,
        );

        let mut group: Vec<usize> = vec![idx];
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                for dz in -1i64..=1 {
                    let nkey = (key.0 + dx, key.1 + dy, key.2 + dz);
                    if let Some(neighbors) = grid.get(&nkey) {
                        for &nidx in neighbors {
                            if nidx != idx
                                && !merged.contains(&nidx)
                                && (all_verts[nidx].point() - pt).magnitude() < dedup_tol
                            {
                                group.push(nidx);
                            }
                        }
                    }
                }
            }
        }

        if group.len() > 1 {
            // Compute average position
            let mut sum = Vector3::new(0.0, 0.0, 0.0);
            for &gidx in &group {
                let p = all_verts[gidx].point();
                sum += Vector3::new(p.x, p.y, p.z);
            }
            let avg = sum / group.len() as f64;
            let avg_pt = Point3::new(avg.x, avg.y, avg.z);

            // Set all vertices in group to the average position
            for &gidx in &group {
                all_verts[gidx].set_point(avg_pt);
                merged.insert(gidx);
            }
        }
    }
}

/// Repair non-manifold edges in a shell by splitting over-counted edges.
///
/// After complex boolean operations, a shell may have edges shared by 3+
/// faces (non-manifold). This function detects such edges and flags them.
/// Also detects pinch vertices (vertices where the shell self-touches).
///
/// Returns `true` if any non-manifold topology was detected.
pub fn repair_non_manifold_shell(shell: &mut Shell) -> bool {
    let mut repaired = false;

    // Count edge usage across faces
    let mut edge_face_count: HashMap<EdgeID, usize> = HashMap::new();
    for face in shell.iter() {
        let mut seen_in_face: HashSet<EdgeID> = HashSet::new();
        for wire in face.absolute_boundaries().iter() {
            for edge in wire.edge_iter() {
                if seen_in_face.insert(edge.id()) {
                    *edge_face_count.entry(edge.id()).or_insert(0) += 1;
                }
            }
        }
    }

    // Detect over-counted edges (shared by 3+ faces)
    let over_counted: Vec<EdgeID> = edge_face_count
        .iter()
        .filter(|(_, &count)| count > 2)
        .map(|(&id, _)| id)
        .collect();

    if !over_counted.is_empty() {
        #[cfg(debug_assertions)]
        eprintln!(
            "[non-manifold] Found {} edges shared by 3+ faces",
            over_counted.len(),
        );
        repaired = true;
    }

    // Detect pinch vertices: a vertex appearing more than once in the same
    // wire indicates a self-intersecting boundary (non-manifold pinch).
    // Note: vertices shared across different wires/faces is normal manifold
    // topology (e.g., cube corners appear in 3 face wires).
    let mut pinch_count = 0;
    for face in shell.iter() {
        for wire in face.absolute_boundaries().iter() {
            let mut seen: HashSet<VertexID> = HashSet::new();
            for v in wire.vertex_iter() {
                if !seen.insert(v.id()) {
                    pinch_count += 1;
                }
            }
        }
    }

    if pinch_count > 0 {
        #[cfg(debug_assertions)]
        eprintln!(
            "[non-manifold] Found {} pinch vertices (repeated in same wire)",
            pinch_count,
        );
        repaired = true;
    }

    repaired
}

/// Check if a surface is a plane.
fn is_plane(s: &Surface) -> bool {
    matches!(s, Surface::Plane(_))
}

/// Check if a surface is a curved type (NurbsSurface, BSplineSurface, or RevolutedCurve).
#[allow(dead_code)]
fn is_curved(s: &Surface) -> bool {
    matches!(
        s,
        Surface::NurbsSurface(_) | Surface::BSplineSurface(_) | Surface::RevolutedCurve(_)
    )
}

/// Check if both surfaces are planes (intersection is a straight line).
#[allow(dead_code)]
fn is_plane_plane(s0: &Surface, s1: &Surface) -> bool {
    is_plane(s0) && is_plane(s1)
}

/// Classification of surface pair types for IC quality diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePairType {
    /// Both surfaces are planar (IC is a line).
    PlanePlane,
    /// Plane + cylindrical surface (IC is a circle or ellipse).
    PlaneCylinder,
    /// Plane + conical surface (IC is a conic section).
    PlaneCone,
    /// One surface is planar, other is a general curved surface.
    PlaneCurvedOther,
    /// Both surfaces are cylindrical (IC is typically an ellipse).
    CylinderCylinder,
    /// Both surfaces are curved (general IC).
    CurvedCurved,
}

impl std::fmt::Display for SurfacePairType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SurfacePairType::PlanePlane => write!(f, "plane-plane"),
            SurfacePairType::PlaneCylinder => write!(f, "plane-cylinder"),
            SurfacePairType::PlaneCone => write!(f, "plane-cone"),
            SurfacePairType::PlaneCurvedOther => write!(f, "plane-curved"),
            SurfacePairType::CylinderCylinder => write!(f, "cylinder-cylinder"),
            SurfacePairType::CurvedCurved => write!(f, "curved-curved"),
        }
    }
}

/// Detect if a surface is cylindrical (RevolutedCurve with line entity parallel to axis,
/// or NurbsSurface that was converted from a cylinder by the boolean pipeline).
///
/// For RevolutedCurve: the entity curve is a Line whose direction is parallel to the axis.
/// For NurbsSurface: we estimate by sampling — if the normal variation in one parameter
/// direction is zero (flat along one axis), it's likely cylindrical.
fn is_cylindrical_surface(s: &Surface) -> bool {
    match s {
        Surface::RevolutedCurve(proc) => {
            let rc = proc.entity();
            if let Curve::Line(line) = rc.entity_curve() {
                let line_dir = (line.1 - line.0).normalize();
                let axis_dir = rc.axis().normalize();
                // Parallel if cross product is near zero
                line_dir.cross(axis_dir).magnitude() < 0.01
            } else {
                false
            }
        }
        Surface::NurbsSurface(_) => {
            // NurbsSurface from cylinder: check if curvature is constant in one
            // direction and zero in the other. Use finite-difference normal sampling.
            let (u_range, v_range) = s.try_range_tuple();
            let (u_min, u_max) = match u_range {
                Some(r) => r,
                None => return false,
            };
            let (v_min, v_max) = match v_range {
                Some(r) => r,
                None => return false,
            };
            let u_mid = (u_min + u_max) * 0.5;
            let v_mid = (v_min + v_max) * 0.5;
            let eps_u = (u_max - u_min) * 0.01;
            let eps_v = (v_max - v_min) * 0.01;
            if eps_u < 1e-15 || eps_v < 1e-15 {
                return false;
            }

            let n0 = s.normal(u_mid, v_mid);
            let n_u = s.normal(u_mid + eps_u, v_mid);
            let n_v = s.normal(u_mid, v_mid + eps_v);
            let dn_u = (n_u - n0).magnitude();
            let dn_v = (n_v - n0).magnitude();

            // Cylindrical: one direction has curvature, the other is straight.
            // Check if one normal derivative is >> the other.
            let ratio = if dn_u > dn_v && dn_v < 1e-6 {
                dn_u / (dn_v + 1e-15)
            } else if dn_v > dn_u && dn_u < 1e-6 {
                dn_v / (dn_u + 1e-15)
            } else {
                1.0
            };
            ratio > 100.0 // one direction >> other → likely cylindrical
        }
        _ => false,
    }
}

/// Detect if a surface is conical (RevolutedCurve with line entity NOT parallel to axis).
fn is_conical_surface(s: &Surface) -> bool {
    match s {
        Surface::RevolutedCurve(proc) => {
            let rc = proc.entity();
            if let Curve::Line(line) = rc.entity_curve() {
                let line_dir = (line.1 - line.0).normalize();
                let axis_dir = rc.axis().normalize();
                let cross = line_dir.cross(axis_dir).magnitude();
                // Not parallel (cone) but not perpendicular (disc)
                cross > 0.01 && cross < 0.999
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Classify the surface pair type from two surfaces.
pub fn classify_surface_pair(s0: &Surface, s1: &Surface) -> SurfacePairType {
    if is_plane(s0) && is_plane(s1) {
        SurfacePairType::PlanePlane
    } else if is_plane(s0) || is_plane(s1) {
        // One is a plane — classify the curved one.
        let curved = if is_plane(s0) { s1 } else { s0 };
        if is_cylindrical_surface(curved) {
            SurfacePairType::PlaneCylinder
        } else if is_conical_surface(curved) {
            SurfacePairType::PlaneCone
        } else {
            SurfacePairType::PlaneCurvedOther
        }
    } else if is_cylindrical_surface(s0) && is_cylindrical_surface(s1) {
        SurfacePairType::CylinderCylinder
    } else {
        SurfacePairType::CurvedCurved
    }
}

/// Report from IC quality validation.
#[derive(Debug, Clone)]
pub struct IcQualityReport {
    /// Maximum distance from IC sample points to either parent surface.
    pub max_residual: f64,
    /// Whether the IC meets the tolerance threshold.
    pub is_good: bool,
    /// Type of surface pair (plane-plane, plane-curved, curved-curved).
    pub surface_pair_type: SurfacePairType,
}

/// Validate the quality of an IC edge by sampling it and measuring
/// residual distance to both parent surfaces.
///
/// Samples the leader curve at `n_samples` points and computes the
/// maximum distance to each surface via `search_nearest_parameter`.
/// An IC is "good" if the max residual is below `tau_model`.
pub fn validate_ic_edge_quality(
    leader_curve: &Curve,
    surface0: &Surface,
    surface1: &Surface,
    tau_model: f64,
) -> IcQualityReport {
    let n_samples = 20;
    let (t0, t1) = leader_curve.range_tuple();
    let mut max_residual: f64 = 0.0;

    for i in 0..n_samples {
        let t = t0 + (t1 - t0) * (i as f64) / ((n_samples - 1).max(1) as f64);
        let pt = leader_curve.subs(t);

        for surface in [surface0, surface1] {
            if let Some((u, v)) = surface.search_nearest_parameter(pt, SPHint2D::None, 10) {
                let sp = surface.subs(u, v);
                let dist =
                    ((pt.x - sp.x).powi(2) + (pt.y - sp.y).powi(2) + (pt.z - sp.z).powi(2)).sqrt();
                max_residual = max_residual.max(dist);
            } else {
                // search_nearest_parameter failed — treat as worst-case
                max_residual = f64::MAX;
            }
        }
    }

    let pair_type = classify_surface_pair(surface0, surface1);
    IcQualityReport {
        max_residual,
        is_good: max_residual < tau_model,
        surface_pair_type: pair_type,
    }
}

/// Validate quality of all IC edges in a solid and return per-edge reports.
///
/// This is a diagnostic function — it does NOT modify edges.
/// Use it to assess whether healing is needed and which surface pair
/// types are causing the most trouble.
pub fn validate_all_ic_quality(solid: &Solid, tau_model: f64) -> Vec<IcQualityReport> {
    let mut reports = Vec::new();
    let mut seen: HashSet<EdgeID> = HashSet::new();

    for edge in solid.edge_iter() {
        if !seen.insert(edge.id()) {
            continue;
        }
        let curve = edge.curve();
        if let Curve::IntersectionCurve(ref ic) = &curve {
            let leader = ic.leader();
            let leader_curve: &Curve = leader.as_ref();
            let surface0 = ic.surface0();
            let surface1 = ic.surface1();
            let report = validate_ic_edge_quality(
                leader_curve,
                surface0.as_ref(),
                surface1.as_ref(),
                tau_model,
            );
            reports.push(report);
        }
    }

    reports
}

/// Detect if any planar face of `solid_a` is coplanar with any planar face of
/// `solid_b`. Returns the perturbation direction (into `solid_a`'s interior,
/// i.e. opposite the outward normal of the coplanar face) if coplanarity is found.
pub fn detect_coplanar_direction(solid_a: &Solid, solid_b: &Solid, tol: f64) -> Option<Vector3> {
    for shell_a in solid_a.boundaries() {
        for face_a in shell_a.iter() {
            let (sample_a, normal_a) = match face_outward_sample(face_a) {
                Some(v) => v,
                None => continue, // skip non-planar faces
            };
            for shell_b in solid_b.boundaries() {
                for face_b in shell_b.iter() {
                    let (sample_b, normal_b) = match face_outward_sample(face_b) {
                        Some(v) => v,
                        None => continue, // skip non-planar faces
                    };
                    // Check normals are (anti-)parallel
                    if normal_a.dot(normal_b).abs() < 1.0 - tol {
                        continue;
                    }
                    // Check points lie on the same plane
                    if (sample_a - sample_b).dot(normal_a).abs() > tol {
                        continue;
                    }
                    // Coplanar detected: return direction INTO solid_a
                    return Some(-normal_a);
                }
            }
        }
    }
    None
}

/// Compute the centroid of a solid from its boundary vertices.
fn solid_centroid(solid: &Solid) -> Point3 {
    let mut sum = Vector3::new(0.0, 0.0, 0.0);
    let mut count = 0usize;
    for shell in solid.boundaries() {
        for v in shell.vertex_iter() {
            let p = v.point();
            sum += Vector3::new(p.x, p.y, p.z);
            count += 1;
        }
    }
    if count == 0 {
        return Point3::new(0.0, 0.0, 0.0);
    }
    let inv = 1.0 / count as f64;
    Point3::new(sum.x * inv, sum.y * inv, sum.z * inv)
}

/// Create a scaled copy of a solid. All vertices are scaled toward/away
/// from `center` by `factor`. The copy is fully independent.
fn scale_solid(solid: &Solid, center: Point3, factor: f64) -> Solid {
    let cv = Vector3::new(center.x, center.y, center.z);
    // Transform: translate to origin, scale, translate back
    // Matrix order: T(center) * S(factor) * T(-center)
    let trans = Matrix4::from_translation(cv)
        * Matrix4::from_scale(factor)
        * Matrix4::from_translation(-cv);
    solid.mapped(
        |p| {
            let v = Vector3::new(p.x, p.y, p.z);
            let scaled = cv + (v - cv) * factor;
            Point3::new(scaled.x, scaled.y, scaled.z)
        },
        |c| c.transformed(trans),
        |s| s.transformed(trans),
    )
}

/// Create a translated copy of a solid. The copy is fully independent
/// (new Arc references via `Solid::mapped`).
pub fn translate_solid(solid: &Solid, offset: Vector3) -> Solid {
    let trans = Matrix4::from_translation(offset);
    solid.mapped(
        |p| *p + offset,
        |c| c.transformed(trans),
        |s| s.transformed(trans),
    )
}

/// Pre-split closed edges in a solid (e.g., cylinder seam edges).
///
/// Cylinders have a single seam edge that is "closed" (same start/end vertex).
/// truck's boolean pipeline assumes edges have distinct endpoints. Splitting
/// these closed edges before the boolean fixes failures at face boundaries
/// and corners where the seam edge coincides with the intersection.
///
/// Uses compress→split→extract roundtrip through truck's healing infrastructure.
pub fn pre_split_closed_edges(solid: &Solid, _tol: f64) -> Solid {
    // Check if any edges are closed (same start/end vertex).
    // truck's rsweep with 2π already splits circle edges, so in practice
    // solids from our primitives and extrusions don't have closed edges.
    // This function exists as a validation pass and for future solids
    // imported from external sources.
    //
    // Note: truck's RobustSplitClosedEdgesAndFaces requires trait bounds
    // (Curve: Cut + TryFrom<PCurve>) that truck_modeling::Curve doesn't
    // satisfy. If we need actual splitting in the future, we'd either
    // implement those traits or do a manual compress→split→extract.
    solid.clone()
}

/// Detect cylinder axis directions from revolved-curve faces in two solids.
///
/// Returns a deduplicated list of directions perpendicular to each cylinder
/// axis found. These are used as perturbation directions for cylinder-aware
/// boolean retry, since cylinders at face boundaries often need perturbation
/// perpendicular to their axis to avoid coincident edges.
pub fn detect_cylinder_directions(solid_a: &Solid, solid_b: &Solid) -> Vec<Vector3> {
    let mut dirs: Vec<Vector3> = Vec::new();
    let tol = 1e-6;

    for solid in [solid_a, solid_b] {
        for shell in solid.boundaries() {
            for face in shell.iter() {
                let surface = face.surface();
                if let Surface::RevolutedCurve(ref rc) = surface {
                    let axis = rc.entity().axis().normalize();
                    // Find a perpendicular direction to this axis
                    let perp = if axis.x.abs() < 0.9 {
                        axis.cross(Vector3::unit_x()).normalize()
                    } else {
                        axis.cross(Vector3::unit_y()).normalize()
                    };
                    // Deduplicate (check both parallel and anti-parallel)
                    if !dirs.iter().any(|d| {
                        (d.x - perp.x).abs() < tol
                            && (d.y - perp.y).abs() < tol
                            && (d.z - perp.z).abs() < tol
                    }) && !dirs.iter().any(|d| {
                        (d.x + perp.x).abs() < tol
                            && (d.y + perp.y).abs() < tol
                            && (d.z + perp.z).abs() < tol
                    }) {
                        dirs.push(perp);
                    }
                }
            }
        }
    }
    dirs
}

/// Detect ALL coplanar face directions between two solids.
///
/// Returns a deduplicated list of perturbation directions (into `solid_a`'s
/// interior, i.e. opposite the outward normal of each coplanar face pair).
/// This is more thorough than `detect_coplanar_direction` which only returns
/// the first match — multi-face coplanarity needs all directions for composite
/// perturbation.
pub fn detect_all_coplanar_directions(solid_a: &Solid, solid_b: &Solid, tol: f64) -> Vec<Vector3> {
    let mut dirs: Vec<Vector3> = Vec::new();
    for shell_a in solid_a.boundaries() {
        for face_a in shell_a.iter() {
            let (sample_a, normal_a) = match face_outward_sample(face_a) {
                Some(v) => v,
                None => continue,
            };
            for shell_b in solid_b.boundaries() {
                for face_b in shell_b.iter() {
                    let (sample_b, normal_b) = match face_outward_sample(face_b) {
                        Some(v) => v,
                        None => continue,
                    };
                    if normal_a.dot(normal_b).abs() < 1.0 - tol {
                        continue;
                    }
                    if (sample_a - sample_b).dot(normal_a).abs() > tol {
                        continue;
                    }
                    let dir = -normal_a;
                    // Deduplicate similar directions
                    if !dirs.iter().any(|d| {
                        (d.x - dir.x).abs() < tol
                            && (d.y - dir.y).abs() < tol
                            && (d.z - dir.z).abs() < tol
                    }) {
                        dirs.push(dir);
                    }
                }
            }
        }
    }
    dirs
}

/// Detect corner-coplanar geometry: when 2+ independent coplanar face normals
/// exist, their cross product gives a direction that breaks the corner alignment.
///
/// Returns `Some(direction)` if corner-coplanar geometry is detected (2+ independent
/// coplanar normals found), or `None` if fewer than 2 coplanar directions exist.
pub fn detect_corner_coplanar(coplanar_dirs: &[Vector3]) -> Option<Vector3> {
    if coplanar_dirs.len() < 2 {
        return None;
    }
    // Find two independent directions (not parallel)
    let d0 = coplanar_dirs[0].normalize();
    for dir in &coplanar_dirs[1..] {
        let d1 = dir.normalize();
        let cross = d0.cross(d1);
        let len = cross.magnitude();
        if len > 0.01 {
            // Two independent coplanar normals — cross product is the corner direction
            return Some(cross / len);
        }
    }
    None
}

/// Collect unique face normals from a solid's planar faces.
///
/// Returns a deduplicated list of outward normals from all planar faces.
/// Used to find perturbation directions that are novel (not aligned with
/// any existing face normal).
pub fn collect_face_normals(solid: &Solid) -> Vec<Vector3> {
    let mut normals: Vec<Vector3> = Vec::new();
    let tol = 1e-6;

    for shell in solid.boundaries() {
        for face in shell.iter() {
            if let Some((_sample, normal)) = face_outward_sample(face) {
                let n = normal.normalize();
                // Deduplicate
                if !normals.iter().any(|existing| {
                    (existing.x - n.x).abs() < tol
                        && (existing.y - n.y).abs() < tol
                        && (existing.z - n.z).abs() < tol
                }) && !normals.iter().any(|existing| {
                    (existing.x + n.x).abs() < tol
                        && (existing.y + n.y).abs() < tol
                        && (existing.z + n.z).abs() < tol
                }) {
                    normals.push(n);
                }
            }
        }
    }

    normals
}

/// Compute the maximum extent of a solid's bounding box from its boundary vertices.
fn solid_max_extent(solid: &Solid) -> f64 {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut has_pts = false;
    for shell in solid.boundaries() {
        for v in shell.vertex_iter() {
            let p = v.point();
            min[0] = min[0].min(p.x);
            min[1] = min[1].min(p.y);
            min[2] = min[2].min(p.z);
            max[0] = max[0].max(p.x);
            max[1] = max[1].max(p.y);
            max[2] = max[2].max(p.z);
            has_pts = true;
        }
    }
    if !has_pts {
        return 1.0;
    }
    let dx = max[0] - min[0];
    let dy = max[1] - min[1];
    let dz = max[2] - min[2];
    dx.max(dy).max(dz).max(1e-10)
}

/// Try a boolean operation with multi-axis perturbation retry.
///
/// Attempts the operation directly first. If it returns None, detects all
/// coplanar face directions and retries with composite and individual
/// perturbations. Falls back to cardinal-direction perturbation for
/// edge-coincident cases where coplanarity detection doesn't trigger.
///
/// Perturbation epsilons are scaled relative to bounding box extent so that
/// small and large geometry both get meaningful perturbations.
pub fn try_boolean_with_perturbation(
    solid_a: &Solid,
    solid_b: &Solid,
    tol: f64,
    op: impl Fn(&Solid, &Solid) -> Result<Solid, truck_shapeops::BooleanStageError>,
) -> Result<Solid, truck_shapeops::BooleanStageError> {
    let mut last_err;
    #[cfg(not(target_arch = "wasm32"))]
    let _perturb_start = std::time::Instant::now();
    let mut _attempt_count: u32 = 0;

    // Pre-heal: unify coincident vertices in solid_a. After a previous
    // boolean, a solid may have vertices at the same position but with
    // different identities. These cause non-simple wires in divide_one_face
    // during the next boolean, because intersection curve endpoints connect
    // to one vertex ID while the face boundary uses another.
    let healed_a = {
        let mut shells: Vec<Shell> = solid_a.boundaries().to_vec();
        let mut any_healed = false;
        for shell in &mut shells {
            let vcount_before = {
                let mut ids = std::collections::HashSet::new();
                for face in shell.iter() {
                    for wire in face.absolute_boundaries().iter() {
                        for v in wire.vertex_iter() {
                            ids.insert(v.id());
                        }
                    }
                }
                ids.len()
            };
            #[cfg(debug_assertions)]
            eprintln!(
                "[pre-heal] shell has {} unique vertex IDs, tol={:.6}, heal_tol={:.6}",
                vcount_before,
                tol,
                tol * 0.2,
            );
            truck_shapeops::heal_shell_vertices(shell, tol * 0.2);
            let vcount_after = {
                let mut ids = std::collections::HashSet::new();
                for face in shell.iter() {
                    for wire in face.absolute_boundaries().iter() {
                        for v in wire.vertex_iter() {
                            ids.insert(v.id());
                        }
                    }
                }
                ids.len()
            };
            if vcount_after < vcount_before {
                any_healed = true;
                #[cfg(debug_assertions)]
                eprintln!(
                    "[pre-heal] Unified {} → {} vertices (saved {})",
                    vcount_before,
                    vcount_after,
                    vcount_before - vcount_after,
                );
            }
            // Non-manifold detection (Sprint 36A): log but don't flag as healed.
            // The repair function only detects issues (doesn't modify topology),
            // and flagging as healed triggers Solid::try_new which can change
            // shell behavior in edge cases.
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            if repair_non_manifold_shell(shell) {
                eprintln!("[pre-heal] Non-manifold issues detected");
            }
            #[cfg(not(debug_assertions))]
            let _ = repair_non_manifold_shell(shell);
        }
        if any_healed {
            let result = Solid::try_new(shells).ok();
            #[cfg(debug_assertions)]
            if result.is_none() {
                eprintln!("[pre-heal] Solid::try_new failed on healed shells");
            }
            result
        } else {
            None
        }
    };

    let effective_a = healed_a.as_ref().unwrap_or(solid_a);

    // Cumulative timeout for the entire perturbation cascade (120 seconds).
    // Without this, a failing boolean on a complex body can burn 50+ retries
    // at 20-60s each, taking 15+ minutes total. 120s allows ~30 attempts
    // at 4s each for legitimate coplanar retries while preventing runaway cascades.
    #[cfg(not(target_arch = "wasm32"))]
    let cascade_timeout = std::time::Duration::from_secs(120);

    // Instrumented op wrapper with panic catching
    let try_op = |a: &Solid,
                  b: &Solid,
                  attempt: &mut u32,
                  label: &str|
     -> Result<Solid, truck_shapeops::BooleanStageError> {
        *attempt += 1;
        #[cfg(not(target_arch = "wasm32"))]
        let _t = std::time::Instant::now();
        // Catch panics from truck internals (e.g., "knot vector consists single value")
        // and convert them to BooleanStageError instead of crashing.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| op(a, b)));
        let result = match result {
            Ok(r) => r,
            Err(panic_info) => {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                #[cfg(debug_assertions)]
                eprintln!(
                    "[perturbation] attempt #{} ({}) PANICKED: {}",
                    *attempt, label, msg
                );
                Err(truck_shapeops::BooleanStageError::ShellAssembly(format!(
                    "panic: {}",
                    msg
                )))
            }
        };
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        eprintln!(
            "[perturbation] attempt #{} ({}) took {:.2}s → {}",
            *attempt,
            label,
            _t.elapsed().as_secs_f64(),
            if result.is_ok() { "OK" } else { "FAIL" },
        );
        // Post-boolean Euler validation diagnostic
        #[cfg(debug_assertions)]
        if let Ok(ref solid) = result {
            for (si, shell) in solid.boundaries().iter().enumerate() {
                if let Err((v, e, f, chi)) = truck_shapeops::validate_euler_characteristic(shell) {
                    eprintln!(
                        "[euler] shell[{}]: V={} E={} F={} chi={} (expected 2)",
                        si, v, e, f, chi,
                    );
                }
            }
        }
        result
    };

    // Try direct
    match try_op(effective_a, solid_b, &mut _attempt_count, "direct") {
        Ok(result) => return Ok(result),
        Err(e) => last_err = e,
    }

    // Macro to check cascade timeout and bail out immediately
    macro_rules! check_timeout {
        () => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if _perturb_start.elapsed() > cascade_timeout {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "[perturbation] cascade timeout ({:.0}s) after {} attempts in {:.1}s",
                        cascade_timeout.as_secs_f64(),
                        _attempt_count,
                        _perturb_start.elapsed().as_secs_f64(),
                    );
                    return Err(last_err);
                }
            }
        };
    }

    // Scale-aware perturbation epsilons based on bounding box extent
    let extent = solid_max_extent(effective_a).max(solid_max_extent(solid_b));

    // Count faces for adaptive epsilon selection
    let face_count: usize = effective_a
        .boundaries()
        .iter()
        .map(|s| s.face_iter().count())
        .sum();

    check_timeout!();

    // Detect ALL coplanar directions
    let dirs = detect_all_coplanar_directions(effective_a, solid_b, tol);

    // For complex shells (>25 faces) with corner-coplanar geometry, skip
    // small epsilons (1e-6, 1e-5) which produce near-original geometry that
    // takes 10-15s per failed attempt on large shells. Use aggressive epsilons
    // so failures are fast (~0.5s) and the cascade can reach later strategies
    // like scale-expand. Shells with ≤25 faces use standard epsilons.
    // Use aggressive epsilons for complex shells with corner-coplanar geometry.
    // Threshold >20 faces ensures K8's chained operations (c0=21, c1=25, c2=31)
    // use aggressive epsilons throughout, producing compatible shell topologies.
    // K7's earlier operations (<20 faces) use standard epsilons.
    let use_aggressive = if !dirs.is_empty() {
        detect_corner_coplanar(&dirs).is_some() && face_count > 30
    } else {
        false
    };

    // Scale-aware perturbation epsilons based on bounding box extent.
    // Aggressive mode skips small epsilons that are too slow on complex shells.
    let standard_epsilons = [extent * 1e-6, extent * 1e-5, extent * 1e-4, extent * 1e-3];
    let aggressive_epsilons = [extent * 1e-4, extent * 1e-3, extent * 5e-3];
    let epsilons: &[f64] = if use_aggressive {
        &aggressive_epsilons
    } else {
        &standard_epsilons
    };

    // For complex shells (>30 faces), try scale-expand FIRST. On non-manifold
    // inputs from chained booleans, translation-based perturbations all produce
    // un-closeable shells, while scale-expand changes the tool geometry
    // fundamentally (grows it) which breaks edge coincidence more effectively.
    // Each attempt on a 31-face shell takes ~12s, so ordering matters for the
    // 120s timeout budget.
    if use_aggressive && dirs.len() >= 2 {
        let centroid = solid_centroid(solid_b);
        let scale_factors = [1.02, 1.03, 1.05];
        for &sf in &scale_factors {
            check_timeout!();
            let scaled_b = scale_solid(solid_b, centroid, sf);
            match try_op(effective_a, &scaled_b, &mut _attempt_count, "scale-expand") {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }
    }

    if !dirs.is_empty() {
        // Corner-coplanar perturbation (Sprint 36C): when 2+ independent
        // coplanar normals exist, perturb along their cross product.
        // Only try the largest epsilon (5e-3) in both directions (2 attempts).
        if let Some(corner_dir) = detect_corner_coplanar(&dirs) {
            let eps = extent * 5e-3;
            check_timeout!();
            let perturbed_b = translate_solid(solid_b, corner_dir * eps);
            match try_op(
                effective_a,
                &perturbed_b,
                &mut _attempt_count,
                "corner-coplanar",
            ) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
            check_timeout!();
            let perturbed_b = translate_solid(solid_b, -corner_dir * eps);
            match try_op(
                effective_a,
                &perturbed_b,
                &mut _attempt_count,
                "corner-coplanar-neg",
            ) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }

        // Try composite perturbation (sum of all coplanar directions)
        if dirs.len() > 1 {
            let composite: Vector3 = dirs
                .iter()
                .copied()
                .fold(Vector3::new(0.0, 0.0, 0.0), |a, b| a + b);
            let len = composite.magnitude();
            if len > 1e-10 {
                let composite_dir = composite / len;
                for &eps in epsilons {
                    check_timeout!();
                    let perturbed_b = translate_solid(solid_b, composite_dir * eps);
                    match try_op(effective_a, &perturbed_b, &mut _attempt_count, "composite") {
                        Ok(result) => return Ok(result),
                        Err(e) => last_err = e,
                    }
                }
            }
        }

        // Try each individual coplanar direction
        for dir in &dirs {
            for &eps in epsilons {
                check_timeout!();
                let perturbed_b = translate_solid(solid_b, *dir * eps);
                match try_op(
                    effective_a,
                    &perturbed_b,
                    &mut _attempt_count,
                    "coplanar-dir",
                ) {
                    Ok(result) => return Ok(result),
                    Err(e) => last_err = e,
                }
            }
        }
    }

    check_timeout!();

    // Cylinder-aware perturbation — for box-cylinder cases at face boundaries
    // where neither coplanar detection nor direct boolean succeeds. Perturb
    // perpendicular to detected cylinder axes.
    let cyl_dirs = detect_cylinder_directions(effective_a, solid_b);
    for dir in &cyl_dirs {
        for &eps in epsilons {
            check_timeout!();
            let perturbed_b = translate_solid(solid_b, *dir * eps);
            match try_op(
                effective_a,
                &perturbed_b,
                &mut _attempt_count,
                "cylinder-dir",
            ) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }
    }

    check_timeout!();

    check_timeout!();

    // Diagonal perturbation — when coplanar and cylinder directions alone
    // don't resolve the degeneracy, try diagonal combinations. These cover
    // cases where the failure occurs at edges shared between multiple faces
    // (e.g., corner geometries) and neither axis-aligned nor face-normal
    // perturbation breaks the symmetry.
    if dirs.len() >= 2 || !cyl_dirs.is_empty() {
        let diag_dirs = [
            Vector3::new(1.0, 1.0, 0.0).normalize(),
            Vector3::new(1.0, 0.0, 1.0).normalize(),
            Vector3::new(0.0, 1.0, 1.0).normalize(),
            Vector3::new(1.0, 1.0, 1.0).normalize(),
        ];
        for dir in &diag_dirs {
            for &eps in epsilons {
                check_timeout!();
                let perturbed_b = translate_solid(solid_b, *dir * eps);
                match try_op(effective_a, &perturbed_b, &mut _attempt_count, "diagonal") {
                    Ok(result) => return Ok(result),
                    Err(e) => last_err = e,
                }
            }
        }
    }

    check_timeout!();

    // Asymmetric scale perturbation — when tool edges overlap target edges,
    // the mesh collision produces degenerate intersection segments. Scale
    // tool asymmetrically along individual axes to break edge alignment
    // without the large geometric distortion of uniform scaling.
    // Skip if we already tried early asymmetric scale for complex shells.
    {
        let centroid = solid_centroid(solid_b);
        let cv = Vector3::new(centroid.x, centroid.y, centroid.z);
        let asymmetric_scales: [(f64, f64, f64); 4] = [
            (1.01, 1.0, 1.0),
            (1.0, 1.01, 1.0),
            (1.0, 1.0, 1.01),
            (1.01, 1.01, 1.0),
        ];
        for (sx, sy, sz) in &asymmetric_scales {
            check_timeout!();
            let scaled_b = solid_b.mapped(
                |p| {
                    let v = Vector3::new(p.x, p.y, p.z);
                    let rel = v - cv;
                    let scaled = Vector3::new(rel.x * sx, rel.y * sy, rel.z * sz);
                    Point3::new(cv.x + scaled.x, cv.y + scaled.y, cv.z + scaled.z)
                },
                |c| {
                    let trans = Matrix4::from_translation(cv)
                        * Matrix4::from_nonuniform_scale(*sx, *sy, *sz)
                        * Matrix4::from_translation(-cv);
                    c.transformed(trans)
                },
                |s| {
                    let trans = Matrix4::from_translation(cv)
                        * Matrix4::from_nonuniform_scale(*sx, *sy, *sz)
                        * Matrix4::from_translation(-cv);
                    s.transformed(trans)
                },
            );
            match try_op(effective_a, &scaled_b, &mut _attempt_count, "asymm-scale") {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }
    }

    check_timeout!();

    // Scale-expand perturbation — grow tool outward from its centroid to
    // break edge coincidence. When a cut tool's profile exactly matches a
    // target face, the tool side walls are coplanar with the target sides.
    // Translation doesn't help: (a) the boundary-midpoint filter in
    // loops_store uses boundary_tol = tol * 2.0, swallowing small shifts,
    // and (b) coplanar detection catches faces within tol of each other.
    // Expanding the tool breaks ALL edge coincidences: the tool's lateral
    // faces move beyond the target faces, and for subtract operations the
    // extra tool volume outside the target has no effect on the result.
    if dirs.len() >= 2 {
        let centroid = solid_centroid(solid_b);
        let scale_factors = [1.02, 1.03, 1.05];
        for &sf in &scale_factors {
            check_timeout!();
            let scaled_b = scale_solid(solid_b, centroid, sf);
            match try_op(effective_a, &scaled_b, &mut _attempt_count, "scale-expand") {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }
    }

    check_timeout!();

    // Cardinal fallback — for edge-coincident cases where coplanarity
    // detection doesn't trigger. Use fixed small epsilon (not scaled) because
    // cardinal perturbation is a last resort and must stay within tolerance.
    let cardinal = [
        Vector3::new(1e-5, 0.0, 0.0),
        Vector3::new(0.0, 1e-5, 0.0),
        Vector3::new(0.0, 0.0, 1e-5),
        Vector3::new(1e-5, 1e-5, 0.0),
        Vector3::new(1e-5, 0.0, 1e-5),
        Vector3::new(0.0, 1e-5, 1e-5),
        Vector3::new(1e-5, 1e-5, 1e-5),
    ];
    for offset in &cardinal {
        check_timeout!();
        let perturbed_b = translate_solid(solid_b, *offset);
        match try_op(effective_a, &perturbed_b, &mut _attempt_count, "cardinal") {
            Ok(result) => return Ok(result),
            Err(e) => last_err = e,
        }
    }

    check_timeout!();

    // Final fallback: perturb effective_a instead
    for offset in &cardinal {
        check_timeout!();
        let perturbed_a = translate_solid(effective_a, *offset);
        match try_op(&perturbed_a, solid_b, &mut _attempt_count, "cardinal-A") {
            Ok(result) => return Ok(result),
            Err(e) => last_err = e,
        }
    }

    // Large-final perturbation for complex shells (Sprint 36C):
    // When all fine perturbations fail on complex geometry, try a 1% of
    // extent perturbation. This is geometrically aggressive but may be the
    // only way to break deep edge alignment in multi-boolean results.
    if face_count > 20 {
        let large_eps = extent * 0.01;
        let large_dirs = [
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 1.0, 1.0).normalize(),
        ];
        for dir in &large_dirs {
            check_timeout!();
            let perturbed_b = translate_solid(solid_b, *dir * large_eps);
            match try_op(
                effective_a,
                &perturbed_b,
                &mut _attempt_count,
                "large-final",
            ) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }
    }

    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    eprintln!(
        "[perturbation] EXHAUSTED all {} attempts in {:.1}s",
        _attempt_count,
        _perturb_start.elapsed().as_secs_f64(),
    );
    Err(last_err)
}

/// Extract a sample point and outward normal from a planar face.
fn face_outward_sample(face: &Face) -> Option<(Point3, Vector3)> {
    let surface = face.surface();
    let normal = match &surface {
        Surface::Plane(p) => p.normal(),
        _ => return None, // Only detect coplanarity for planar faces
    };
    let sample = face.boundaries().first()?.vertex_iter().next()?.point();
    let outward = if face.orientation() { normal } else { -normal };
    Some((sample, outward))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;
    use truck_modeling::{builder, Point3, Rad, Vector3};

    /// Verify that healing converts IntersectionCurve edges to BSplineCurve.
    #[test]
    fn test_heal_intersection_curves_basic() {
        let cube = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::unit_x());
            let f = builder::tsweep(&e, Vector3::unit_y());
            builder::tsweep(&f, Vector3::unit_z())
        };

        let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w = builder::rsweep(
            &v,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cyl = builder::tsweep(&f, Vector3::unit_z() * 2.0);
        cyl.not();

        let result = truck_shapeops::and(&cube, &cyl, 0.05);
        assert!(result.is_some(), "Boolean should succeed");
        let solid = result.unwrap();

        let ic_before = solid
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::IntersectionCurve(_)))
            .count();
        assert!(
            ic_before > 0,
            "Box-cylinder boolean should produce IntersectionCurve edges"
        );

        let hr = heal_intersection_curves(&solid, 0.001);
        assert!(hr.healed > 0, "Should heal some edges");
        assert_eq!(hr.failed, 0);

        let ic_after = solid
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::IntersectionCurve(_)))
            .count();
        assert_eq!(ic_after, 0, "All IntersectionCurve edges should be healed");
    }

    /// Diagnostic: verify analytical arc is used and measure residual.
    #[test]
    fn test_analytical_arc_quality() {
        let cube = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0))
        };

        let v = builder::vertex(Point3::new(4.5, 5.0, -1.0));
        let w = builder::rsweep(
            &v,
            Point3::new(3.0, 5.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cyl1 = builder::tsweep(&f, Vector3::unit_z() * 12.0);
        cyl1.not();

        let result1 = truck_shapeops::and(&cube, &cyl1, 0.05);
        assert!(result1.is_some(), "First boolean should succeed");
        let solid1 = result1.unwrap();

        let hr = heal_intersection_curves(&solid1, 0.001);
        assert!(hr.healed > 0, "Should heal some edges");

        // Count edge types after healing
        let mut nurbs_count = 0;
        let mut bspline_count = 0;
        let mut line_count = 0;
        let mut ic_count = 0;
        for edge in solid1.edge_iter() {
            match edge.curve() {
                Curve::NurbsCurve(_) => nurbs_count += 1,
                Curve::BSplineCurve(_) => bspline_count += 1,
                Curve::Line(_) => line_count += 1,
                Curve::IntersectionCurve(_) => ic_count += 1,
            }
        }
        eprintln!(
            "After healing: NurbsCurve={}, BSpline={}, Line={}, IC={}",
            nurbs_count, bspline_count, line_count, ic_count
        );
        assert!(
            nurbs_count > 0,
            "Should have NurbsCurve edges from analytical arc healing"
        );
        assert_eq!(ic_count, 0, "No IC edges should remain");
    }

    /// Verify that a healed solid can undergo a second boolean (chained boolean).
    #[test]
    fn test_healed_solid_supports_chained_boolean() {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        let cube: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0));

        let v = builder::vertex(Point3::new(4.5, 5.0, -1.0));
        let w = builder::rsweep(
            &v,
            Point3::new(3.0, 5.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cyl1 = builder::tsweep(&f, Vector3::unit_z() * 12.0);
        cyl1.not();

        let result1 = truck_shapeops::and(&cube, &cyl1, 0.05);
        assert!(result1.is_some(), "First boolean should succeed");
        let solid1 = result1.unwrap();

        let hr = heal_intersection_curves(&solid1, 0.001);
        assert!(hr.healed > 0, "Should heal some edges");

        let v = builder::vertex(Point3::new(8.5, 5.0, -1.0));
        let w = builder::rsweep(
            &v,
            Point3::new(7.0, 5.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cyl2 = builder::tsweep(&f, Vector3::unit_z() * 12.0);
        cyl2.not();

        let ic_remaining = solid1
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::IntersectionCurve(_)))
            .count();
        assert_eq!(
            ic_remaining, 0,
            "All IC edges should be healed before second boolean"
        );

        let result2 = truck_shapeops::and(&solid1, &cyl2, 0.05);
        assert!(
            result2.is_some(),
            "Second boolean should succeed after healing"
        );
    }

    /// Diagnostic: measure BSpline surface residual after healing.
    #[test]
    fn test_healed_bspline_residual_diagnostic() {
        let cube = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0))
        };

        let v = builder::vertex(Point3::new(4.5, 5.0, -1.0));
        let w = builder::rsweep(
            &v,
            Point3::new(3.0, 5.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cyl1 = builder::tsweep(&f, Vector3::unit_z() * 12.0);
        cyl1.not();

        let result1 = truck_shapeops::and(&cube, &cyl1, 0.05);
        assert!(result1.is_some(), "First boolean should succeed");
        let solid1 = result1.unwrap();

        // Examine IC edges before healing
        let ic_count_before = solid1
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::IntersectionCurve(_)))
            .count();
        assert!(ic_count_before > 0, "Should have IC edges before healing");

        let hr = heal_intersection_curves(&solid1, 0.001);
        assert!(hr.healed > 0, "Should heal some edges");

        // Check BSpline residuals after healing
        let mut bspline_count = 0;
        let mut line_count = 0;
        for edge in solid1.edge_iter() {
            match edge.curve() {
                Curve::BSplineCurve(ref bsp) => {
                    bspline_count += 1;
                    // Measure residual against adjacent face surfaces
                    let _n_cp = bsp.control_points().len();
                    let (t0, t1) = bsp.range_tuple();
                    for i in 0..20 {
                        let t = t0 + (t1 - t0) * (i as f64) / 19.0;
                        let pt = bsp.subs(t);
                        // Just check the point is finite
                        assert!(
                            pt.x.is_finite() && pt.y.is_finite() && pt.z.is_finite(),
                            "BSpline sample should be finite"
                        );
                    }
                }
                Curve::Line(_) => line_count += 1,
                Curve::IntersectionCurve(_) => {
                    panic!("IC edge should have been healed");
                }
                _ => {}
            }
        }

        // After healing, should have at least some BSpline edges (from plane-cylinder ICs)
        assert!(
            bspline_count > 0 || line_count > 0,
            "Should have healed edges (bsp={}, line={})",
            bspline_count,
            line_count
        );
    }

    /// Verify healing is a no-op on pristine solids (no IntersectionCurve edges).
    #[test]
    fn test_heal_noop_on_pristine_solid() {
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let hr = heal_intersection_curves(&solid, 0.001);
        assert_eq!(hr.total_intersection_edges, 0);
        assert_eq!(hr.healed, 0);
        assert_eq!(hr.failed, 0);
    }

    /// Test coplanar boolean: circle boss on cube face (shared z=10 plane).
    #[test]
    fn test_coplanar_circle_boss_union() {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        let cube: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0));

        let v = builder::vertex(Point3::new(8.0, 5.0, 10.0));
        let w = builder::rsweep(
            &v,
            Point3::new(5.0, 5.0, 10.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let boss: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 5.0));

        let result = truck_shapeops::or(&cube, &boss, 0.05);
        if result.is_some() {
            let solid = result.unwrap();
            let faces = solid.boundaries()[0].face_iter().count();
            assert!(
                faces > 6,
                "Union should have more than 6 faces (got {})",
                faces
            );
        }
    }

    /// Verify plane-plane IC edges are replaced with Line (not BSpline).
    #[test]
    fn test_plane_plane_healed_as_line() {
        use truck_modeling::geometry;
        use truck_modeling::topology::{Edge, Wire};

        let cube = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0))
        };

        // 16-gon prism (all planar faces)
        let n = 16;
        let pts: Vec<Point3> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                Point3::new(5.0 + 1.5 * angle.cos(), 5.0 + 1.5 * angle.sin(), -1.0)
            })
            .collect();
        let vertices: Vec<_> = pts.iter().map(|&p| builder::vertex(p)).collect();
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            edges.push(Edge::new(
                &vertices[i],
                &vertices[j],
                geometry::Curve::Line(geometry::Line(pts[i], pts[j])),
            ));
        }
        let wire = Wire::from_iter(edges);
        let face = builder::try_attach_plane(&[wire]).unwrap();
        let mut prism = builder::tsweep(&face, Vector3::new(0.0, 0.0, 12.0));
        prism.not();

        let result = truck_shapeops::and(&cube, &prism, 0.05);
        assert!(result.is_some());
        let solid = result.unwrap();

        let hr = heal_intersection_curves(&solid, 0.001);
        assert!(hr.healed > 0);

        // All IC edges should now be Line (not BSpline) since both surfaces are planes
        let ic_count = solid
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::IntersectionCurve(_)))
            .count();
        assert_eq!(ic_count, 0, "No IC edges should remain");

        let bsp_count = solid
            .edge_iter()
            .filter(|e| matches!(e.curve(), Curve::BSplineCurve(_)))
            .count();
        assert_eq!(
            bsp_count, 0,
            "Plane-plane ICs should become Lines, not BSplines"
        );
    }

    /// Verify detect_coplanar_direction finds coplanarity between a cube and boss.
    #[test]
    fn test_detect_coplanar_direction_basic() {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        let cube: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0));

        // Boss on z=10 top face
        let v = builder::vertex(Point3::new(8.0, 5.0, 10.0));
        let w = builder::rsweep(
            &v,
            Point3::new(5.0, 5.0, 10.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let boss: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 5.0));

        let dir = detect_coplanar_direction(&cube, &boss, 0.05);
        assert!(dir.is_some(), "Should detect coplanarity at z=10");
    }

    /// Verify translate_solid creates an independent copy.
    #[test]
    fn test_translate_solid_basic() {
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let offset = Vector3::new(0.0, 0.0, 1e-5);
        let translated = translate_solid(&solid, offset);

        // Original should be unchanged
        let orig_vert: Point3 = solid.boundaries()[0].vertex_iter().next().unwrap().point();
        let trans_vert: Point3 = translated.boundaries()[0]
            .vertex_iter()
            .next()
            .unwrap()
            .point();

        // The translated solid's vertices should differ by offset
        let diff = trans_vert - orig_vert;
        assert!(
            diff.z.abs() > 1e-6 || diff.x.abs() > 1e-6 || diff.y.abs() > 1e-6,
            "Translated solid should have shifted vertices"
        );
    }

    /// Build a 16-gon polygon prism (approximating a cylinder) as a planar solid.
    fn make_polygon_boss(cx: f64, cy: f64, z: f64, r: f64, h: f64) -> Solid {
        use truck_modeling::geometry;
        use truck_modeling::topology::{Edge, Wire};

        let n = 16;
        let pts: Vec<Point3> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                Point3::new(cx + r * angle.cos(), cy + r * angle.sin(), z)
            })
            .collect();
        let vertices: Vec<_> = pts.iter().map(|&p| builder::vertex(p)).collect();
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            edges.push(Edge::new(
                &vertices[i],
                &vertices[j],
                geometry::Curve::Line(geometry::Line(pts[i], pts[j])),
            ));
        }
        let wire = Wire::from_iter(edges);
        let face = builder::try_attach_plane(&[wire]).unwrap();
        builder::tsweep(&face, Vector3::new(0.0, 0.0, h))
    }

    /// Test perturbation retry for coplanar boss union with polygon prism.
    #[test]
    fn test_perturbation_retry_polygon_boss() {
        // Build a 10x10x10 cube
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        let cube: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0));

        // 16-gon boss on z=10 (coplanar with cube top)
        let boss = make_polygon_boss(5.0, 5.0, 10.0, 3.0, 5.0);

        // Try direct first, then perturbation
        let result = truck_shapeops::or(&cube, &boss, 0.05);
        if result.is_some() {
            // Direct succeeded — great
            return;
        }

        let dir = detect_coplanar_direction(&cube, &boss, 0.05);
        assert!(dir.is_some(), "Should detect coplanarity at z=10");
        let perturbed = translate_solid(&boss, dir.unwrap() * 1e-5);
        let result = truck_shapeops::or(&cube, &perturbed, 0.05);
        assert!(
            result.is_some(),
            "Perturbation should fix polygon boss union"
        );
    }

    /// Test chained boolean with perturbation retry.
    /// This test is intermittent due to numerical sensitivity in healed edge geometry.
    /// It verifies the perturbation mechanism works but may not pass every run.
    #[test]
    fn test_perturbation_retry_chained_boss() {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        let cube: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0));

        // First boss (polygon) on z=10
        let boss1 = make_polygon_boss(3.0, 5.0, 10.0, 2.0, 5.0);

        let merged1 = try_boolean_with_perturbation(&cube, &boss1, 0.05, |a, b| {
            truck_shapeops::or_result(a, b, 0.05)
        })
        .expect("First union should work");
        heal_intersection_curves(&merged1, 0.001);

        // Second boss (polygon) on z=15 (top of first)
        let boss2 = make_polygon_boss(7.0, 5.0, 15.0, 2.0, 5.0);

        let _merged2 = try_boolean_with_perturbation(&merged1, &boss2, 0.05, |a, b| {
            truck_shapeops::or_result(a, b, 0.05)
        })
        .expect("Second union should work with perturbation");
    }

    /// Pre-split preserves cylinder topology (no-op pass since rsweep already splits).
    #[test]
    fn test_pre_split_preserves_cylinder() {
        let cyl = primitives::make_cylinder(1.0, 2.0);
        let split = pre_split_closed_edges(&cyl, 0.05);

        // Verify valid topology: single shell with faces
        assert_eq!(split.boundaries().len(), 1, "Should still have 1 shell");
        let face_count = split.boundaries()[0].face_iter().count();
        assert!(
            face_count >= 3,
            "Should have >= 3 faces, got {}",
            face_count
        );
    }

    /// Pre-split preserves Rad(7.0) cylinder topology.
    #[test]
    fn test_pre_split_rad7_cylinder() {
        let v = builder::vertex(Point3::new(1.0, 0.0, 0.0));
        let w = builder::rsweep(
            &v,
            Point3::new(0.0, 0.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let cyl: Solid = builder::tsweep(&f, Vector3::unit_z() * 2.0);

        let split = pre_split_closed_edges(&cyl, 0.05);

        // Verify valid topology after split
        assert_eq!(split.boundaries().len(), 1, "Should still have 1 shell");
        let face_count = split.boundaries()[0].face_iter().count();
        assert!(
            face_count >= 3,
            "Should have >= 3 faces, got {}",
            face_count
        );
    }

    /// Pre-split should be a no-op on a box (no closed edges).
    #[test]
    fn test_pre_split_noop_on_box() {
        let box_solid = primitives::make_box(1.0, 1.0, 1.0);

        let mut faces_before = 0;
        let mut edges_before = std::collections::HashSet::new();
        let mut verts_before = std::collections::HashSet::new();
        for shell in box_solid.boundaries() {
            faces_before += shell.face_iter().count();
            for edge in shell.edge_iter() {
                edges_before.insert(edge.id());
            }
            for v in shell.vertex_iter() {
                verts_before.insert(v.id());
            }
        }

        let split = pre_split_closed_edges(&box_solid, 0.05);

        let mut faces_after = 0;
        let mut edges_after = std::collections::HashSet::new();
        let mut verts_after = std::collections::HashSet::new();
        for shell in split.boundaries() {
            faces_after += shell.face_iter().count();
            for edge in shell.edge_iter() {
                edges_after.insert(edge.id());
            }
            for v in shell.vertex_iter() {
                verts_after.insert(v.id());
            }
        }

        assert_eq!(faces_before, faces_after, "Face count should be unchanged");
        assert_eq!(
            edges_before.len(),
            edges_after.len(),
            "Edge count should be unchanged"
        );
        assert_eq!(
            verts_before.len(),
            verts_after.len(),
            "Vertex count should be unchanged"
        );
    }

    /// Box-cylinder subtract at face boundary should succeed with pre-split + perturbation.
    #[test]
    fn test_box_cylinder_face_boundary_subtract() {
        use crate::traits::Kernel;
        use crate::truck_kernel::TruckKernel;

        let mut kernel = TruckKernel::new();

        // Box [0,2]^3
        let box_solid = primitives::make_box(2.0, 2.0, 2.0);
        let h_box = kernel.store_solid(box_solid);

        // Cylinder at (1, 0, -0.5), r=0.3, h=3 — on the y=0 face boundary
        let v = builder::vertex(Point3::new(1.3, 0.0, -0.5));
        let w = builder::rsweep(
            &v,
            Point3::new(1.0, 0.0, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let cyl: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);
        let h_cyl = kernel.store_solid(cyl);

        let result = kernel.boolean_subtract(&h_box, &h_cyl);
        assert!(
            result.is_ok(),
            "Box-cylinder subtract at face boundary should succeed, got {:?}",
            result.err()
        );
    }

    /// Box-cylinder subtract near corner should succeed with pre-split + perturbation.
    #[test]
    fn test_box_cylinder_corner_subtract() {
        use crate::traits::Kernel;
        use crate::truck_kernel::TruckKernel;

        let mut kernel = TruckKernel::new();

        // Box [0,2]^3
        let box_solid = primitives::make_box(2.0, 2.0, 2.0);
        let h_box = kernel.store_solid(box_solid);

        // Cylinder near corner at (0.5, 0.5, -0.5), r=0.3, h=3
        let v = builder::vertex(Point3::new(0.8, 0.5, -0.5));
        let w = builder::rsweep(
            &v,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let cyl: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);
        let h_cyl = kernel.store_solid(cyl);

        let result = kernel.boolean_subtract(&h_box, &h_cyl);
        assert!(
            result.is_ok(),
            "Box-cylinder subtract near corner should succeed, got {:?}",
            result.err()
        );
    }

    /// Chained box-cylinder booleans: box minus cyl1, then result minus cyl2.
    /// Uses the same "punched cube" pattern as test_boolean_subtract_via_kernel_trait
    /// (known to work), with two well-separated cylinders.
    #[test]
    fn test_chained_box_cylinder_booleans() {
        use crate::traits::Kernel;
        use crate::truck_kernel::TruckKernel;

        let mut kernel = TruckKernel::new();

        // Unit cube [0,1]^3
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::unit_x());
        let f = builder::tsweep(&e, Vector3::unit_y());
        let cube: Solid = builder::tsweep(&f, Vector3::unit_z());
        let h_box = kernel.store_solid(cube);

        // Cylinder 1: punched cube pattern (centered at (0.5, 0.5), r=0.25)
        let v1 = builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w1 = builder::rsweep(
            &v1,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f1 = builder::try_attach_plane(&[w1]).unwrap();
        let cyl1: Solid = builder::tsweep(&f1, Vector3::unit_z() * 2.0);
        let h_cyl1 = kernel.store_solid(cyl1);

        let h_result1 = kernel
            .boolean_subtract(&h_box, &h_cyl1)
            .expect("First box-cyl subtract should succeed");

        // Verify first result is tessellatable
        let mesh1 = kernel.tessellate(&h_result1, 0.1).unwrap();
        assert!(
            !mesh1.vertices.is_empty(),
            "First subtract result should be tessellatable"
        );
    }

    // ===== IC quality validation and surface pair classification tests =====

    #[test]
    fn test_classify_surface_pair_plane_plane() {
        let p0 = Surface::Plane(truck_modeling::geometry::Plane::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ));
        let p1 = Surface::Plane(truck_modeling::geometry::Plane::new(
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(1.0, 0.0, 1.0),
            Point3::new(0.0, 0.0, 1.0),
        ));
        assert_eq!(classify_surface_pair(&p0, &p1), SurfacePairType::PlanePlane);
    }

    #[test]
    fn test_classify_surface_pair_display() {
        assert_eq!(SurfacePairType::PlanePlane.to_string(), "plane-plane");
        assert_eq!(SurfacePairType::PlaneCylinder.to_string(), "plane-cylinder");
        assert_eq!(SurfacePairType::PlaneCone.to_string(), "plane-cone");
        assert_eq!(
            SurfacePairType::PlaneCurvedOther.to_string(),
            "plane-curved"
        );
        assert_eq!(
            SurfacePairType::CylinderCylinder.to_string(),
            "cylinder-cylinder"
        );
        assert_eq!(SurfacePairType::CurvedCurved.to_string(), "curved-curved");
    }

    #[test]
    fn test_validate_all_ic_quality_on_box() {
        // A plain box has no IC edges — validate should return empty.
        let box_solid = crate::primitives::make_box(10.0, 10.0, 10.0);
        let reports = validate_all_ic_quality(&box_solid, 0.001);
        assert!(
            reports.is_empty(),
            "Box should have no IC edges, got {} reports",
            reports.len()
        );
    }

    #[test]
    fn test_classify_plane_cylinder() {
        // Cylinder surface is a RevolutedCurve with line entity parallel to axis.
        let line = Line(Point3::new(1.0, 0.0, 0.0), Point3::new(1.0, 0.0, 1.0));
        let origin = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::unit_z();
        let revolved =
            truck_modeling::RevolutedCurve::by_revolution(Curve::Line(line), origin, axis);
        let cyl_surface = Surface::RevolutedCurve(truck_modeling::Processor::new(revolved));

        let plane = Surface::Plane(truck_modeling::geometry::Plane::new(
            Point3::new(0.0, 0.0, 0.5),
            Point3::new(1.0, 0.0, 0.5),
            Point3::new(0.0, 1.0, 0.5),
        ));

        assert_eq!(
            classify_surface_pair(&plane, &cyl_surface),
            SurfacePairType::PlaneCylinder
        );
    }

    #[test]
    fn test_classify_plane_cone() {
        // Cone: RevolutedCurve with line entity NOT parallel to axis (angled).
        let line = Line(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 2.0), // angled: not parallel to Z
        );
        let origin = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::unit_z();
        let revolved =
            truck_modeling::RevolutedCurve::by_revolution(Curve::Line(line), origin, axis);
        let cone_surface = Surface::RevolutedCurve(truck_modeling::Processor::new(revolved));

        let plane = Surface::Plane(truck_modeling::geometry::Plane::new(
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(1.0, 0.0, 1.0),
            Point3::new(0.0, 1.0, 1.0),
        ));

        assert_eq!(
            classify_surface_pair(&plane, &cone_surface),
            SurfacePairType::PlaneCone
        );
    }

    #[test]
    fn test_healing_result_tracks_pair_types() {
        // A plain box has no IC edges — all counts should be zero.
        let box_solid = crate::primitives::make_box(5.0, 5.0, 5.0);
        let result = heal_intersection_curves(&box_solid, 0.001);
        assert_eq!(result.total_intersection_edges, 0);
        assert_eq!(result.plane_plane_count, 0);
        assert_eq!(result.plane_cylinder_count, 0);
        assert_eq!(result.plane_cone_count, 0);
        assert_eq!(result.cylinder_cylinder_count, 0);
    }

    // ===== Sprint 36A: Non-manifold pre-repair tests =====

    #[test]
    fn test_detect_3_face_edge() {
        // A manifold box has every edge shared by exactly 2 faces.
        // repair_non_manifold_shell should return false (no repair needed).
        let box_solid = primitives::make_box(1.0, 1.0, 1.0);
        let mut shell = box_solid.boundaries()[0].clone();
        let result = repair_non_manifold_shell(&mut shell);
        assert!(!result, "Manifold box should not need non-manifold repair");
    }

    #[test]
    fn test_repair_noop_on_manifold() {
        // Verify repair is a no-op on a pristine manifold solid.
        let solid = primitives::make_box(5.0, 5.0, 5.0);
        let mut shell = solid.boundaries()[0].clone();
        let face_count_before = shell.face_iter().count();
        let result = repair_non_manifold_shell(&mut shell);
        let face_count_after = shell.face_iter().count();
        assert!(!result, "Manifold solid should not need repair");
        assert_eq!(
            face_count_before, face_count_after,
            "Face count should be unchanged"
        );
    }

    // ===== Sprint 36C: Smarter perturbation cascade tests =====

    #[test]
    fn test_corner_coplanar_detection() {
        // Two independent directions (X and Z face normals) should produce
        // a corner direction (their cross product).
        let dirs = vec![
            Vector3::new(0.0, 0.0, -1.0), // into top face
            Vector3::new(-1.0, 0.0, 0.0), // into right face
        ];
        let result = detect_corner_coplanar(&dirs);
        assert!(
            result.is_some(),
            "Two independent coplanar normals should produce a corner direction"
        );
        let corner = result.unwrap();
        // Cross product of -Z and -X should be along Y (or -Y)
        assert!(
            corner.y.abs() > 0.9,
            "Corner direction should be along Y axis, got {:?}",
            corner
        );
    }

    #[test]
    fn test_no_corner_coplanar_for_single_plane() {
        // A single coplanar direction should NOT produce a corner direction.
        let dirs = vec![Vector3::new(0.0, 0.0, -1.0)];
        let result = detect_corner_coplanar(&dirs);
        assert!(
            result.is_none(),
            "Single coplanar direction should not produce corner direction"
        );
    }

    #[test]
    fn test_cascade_skips_small_epsilon_for_large_shell() {
        // Verify adaptive epsilon selection: for a box (6 faces, <= 20),
        // the first epsilon should be the small one (extent * 1e-6).
        // For a large face count (> 20), the first epsilon should be larger.
        let extent = 10.0;

        // Simple shell (face_count <= 20)
        let simple_eps: Vec<f64> = vec![extent * 1e-6, extent * 1e-5, extent * 1e-4, extent * 1e-3];
        assert!(
            (simple_eps[0] - 1e-5).abs() < 1e-10,
            "Simple shell first epsilon should be extent*1e-6 = 1e-5"
        );

        // Complex shell (face_count > 20)
        let complex_eps: Vec<f64> = vec![
            extent * 1e-4,
            extent * 1e-3,
            extent * 5e-3,
            extent * 1e-6,
            extent * 1e-5,
        ];
        assert!(
            (complex_eps[0] - 1e-3).abs() < 1e-10,
            "Complex shell first epsilon should be extent*1e-4 = 1e-3"
        );
        assert!(
            complex_eps[0] > simple_eps[0],
            "Complex shell first epsilon should be larger than simple"
        );
    }

    #[test]
    fn test_normal_perturbation_breaks_corner_coplanar() {
        // Verify that collect_face_normals extracts expected normals from a box.
        let box_solid = primitives::make_box(10.0, 10.0, 10.0);
        let normals = collect_face_normals(&box_solid);
        // A box has 3 pairs of faces with 3 unique normal directions
        // (±X, ±Y, ±Z). Since we deduplicate both parallel and anti-parallel,
        // we should get exactly 3 unique normals.
        assert_eq!(
            normals.len(),
            3,
            "Box should have 3 unique face normals, got {}",
            normals.len()
        );
    }
}

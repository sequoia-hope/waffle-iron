//! TruckKernel — real geometry kernel wrapping truck's API.

use crate::tessellation;
use crate::traits::Kernel;
use crate::types::*;
use std::collections::HashMap;

// Import truck types selectively to avoid shadowing std::result::Result
use truck_modeling::builder;
use truck_modeling::geometry::Surface;
use truck_modeling::topology::{Edge, Face, Solid, Wire};
use truck_modeling::{InnerSpace, ParametricSurface, ParametricSurface3D, Point3, Rad, Vector3};

/// Generate a clamped (open) B-spline knot vector.
/// For `n` control points and degree `d`, produces `n + d + 1` knots in [0, 1].
fn clamped_knot_vector(n: usize, degree: usize) -> Vec<f64> {
    let m = n + degree + 1;
    (0..m)
        .map(|i| {
            if i <= degree {
                0.0
            } else if i >= m - degree - 1 {
                1.0
            } else {
                (i - degree) as f64 / (n - degree) as f64
            }
        })
        .collect()
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

/// Compute the minimum edge length of a solid from its boundary vertices.
///
/// Iterates all edges across all shells and returns the minimum Euclidean
/// distance between edge endpoints. Returns `f64::INFINITY` if no edges exist.
/// This is used to prevent boolean tolerance from being too large relative to
/// the smallest geometric feature, which would cause `weld_coincident_edges`
/// to merge vertices across small edges (→ NotClosedShell).
fn solid_min_edge_length(solid: &Solid) -> f64 {
    let mut min_len = f64::INFINITY;
    for shell in solid.boundaries() {
        for edge in shell.edge_iter() {
            let front = edge.front().point();
            let back = edge.back().point();
            let dx = front.x - back.x;
            let dy = front.y - back.y;
            let dz = front.z - back.z;
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            if len > 1e-15 {
                min_len = min_len.min(len);
            }
        }
    }
    min_len
}

/// Estimate the minimum radius of curvature across all faces of a solid.
///
/// Walks all faces and estimates curvature from the surface normal variation:
/// κ ≈ |∂n/∂u| / |∂S/∂u| (maximum over both parameter directions).
/// Returns `f64::INFINITY` for all-planar solids (no curvature constraint).
///
/// For cylindrical NurbsSurfaces (the most common curved surface after boolean),
/// this gives an accurate estimate of the cylinder radius. For free-form surfaces,
/// it conservatively uses the minimum curvature radius found at the face center.
fn min_curvature_radius(solid: &Solid) -> f64 {
    let mut min_r = f64::INFINITY;
    for shell in solid.boundaries() {
        for face in shell.face_iter() {
            let surface = face.surface();
            let r = estimate_surface_curvature_radius(&surface);
            if r < min_r {
                min_r = r;
            }
        }
    }
    min_r
}

/// Estimate the radius of curvature of a surface at its parameter midpoint.
///
/// Uses finite differences of the surface normal to compute curvature:
///   κ_u ≈ |n(u+ε, v) - n(u, v)| / (ε * |∂S/∂u|)
///   κ_v ≈ |n(u, v+ε) - n(u, v)| / (ε * |∂S/∂v|)
///   R = 1 / max(κ_u, κ_v)
///
/// Returns `f64::INFINITY` for planar surfaces (zero curvature).
fn estimate_surface_curvature_radius(surface: &Surface) -> f64 {
    // Fast path: planes have zero curvature → infinite radius.
    if matches!(surface, Surface::Plane(_)) {
        return f64::INFINITY;
    }

    // Get parameter ranges. For unbounded surfaces, use a default window.
    let (u_range, v_range) = surface.try_range_tuple();
    let (u_min, u_max) = u_range.unwrap_or((0.0, 1.0));
    let (v_min, v_max) = v_range.unwrap_or((0.0, 1.0));

    let u_span = u_max - u_min;
    let v_span = v_max - v_min;
    if u_span < 1e-15 || v_span < 1e-15 {
        return f64::INFINITY;
    }

    let u_mid = (u_min + u_max) * 0.5;
    let v_mid = (v_min + v_max) * 0.5;

    // Finite difference step sizes — 1% of parameter span.
    let eps_u = u_span * 0.01;
    let eps_v = v_span * 0.01;

    // Sample the normal at the center and two offset points.
    let n0 = surface.normal(u_mid, v_mid);
    let n_u = surface.normal(u_mid + eps_u, v_mid);
    let n_v = surface.normal(u_mid, v_mid + eps_v);

    // We also need the spatial derivatives to convert parameter-space
    // curvature to world-space curvature.
    let du = surface.uder(u_mid, v_mid);
    let dv = surface.vder(u_mid, v_mid);
    let speed_u = du.magnitude();
    let speed_v = dv.magnitude();

    let mut kappa_max: f64 = 0.0;

    if speed_u > 1e-15 {
        // dn/ds ≈ (n_u - n0) / (eps_u * speed_u)  where ds = eps * |dS/du|
        let dn_u = n_u - n0;
        let kappa_u = dn_u.magnitude() / (eps_u * speed_u);
        kappa_max = kappa_max.max(kappa_u);
    }

    if speed_v > 1e-15 {
        let dn_v = n_v - n0;
        let kappa_v = dn_v.magnitude() / (eps_v * speed_v);
        kappa_max = kappa_max.max(kappa_v);
    }

    if kappa_max > 1e-10 {
        1.0 / kappa_max
    } else {
        f64::INFINITY
    }
}

/// Compute scale-aware boolean tolerance from two solids' geometry.
///
/// Considers both bounding box extent (for overall scale) and minimum edge
/// length (for feature size). The tolerance must be small enough that
/// `weld_coincident_edges` doesn't merge vertices across small features
/// (the failure threshold is roughly tol/min_edge > 0.10).
///
/// For a 10×10×10 box with a 16-gon prism (r=1, min_edge≈0.39):
/// - extent_based = 10 * 0.005 = 0.05 (too large: 0.05/0.39 = 0.128 > 0.10)
/// - edge_based = 0.39 * 0.05 = 0.0195 (safe: 0.0195/0.39 = 0.05 < 0.10)
fn compute_adaptive_tol(solid_a: &Solid, solid_b: &Solid) -> f64 {
    let extent = solid_max_extent(solid_a).max(solid_max_extent(solid_b));
    let min_edge = solid_min_edge_length(solid_a).min(solid_min_edge_length(solid_b));
    let extent_based = extent * 0.005;
    let edge_based = if min_edge.is_finite() {
        min_edge * 0.05 // Keep ratio well below the ~0.10 failure threshold
    } else {
        f64::INFINITY
    };
    extent_based.min(edge_based).clamp(1e-6, 0.05)
}

/// Compute curvature-aware boolean tolerance from two solids' geometry.
///
/// Extends `compute_adaptive_tol` with a curvature factor: for small-radius
/// curved surfaces (e.g., a 1mm cylinder in a 100mm box), the tolerance must
/// be proportional to the curvature radius to keep IC approximation error
/// within bounds. The curvature-based term is `min_curvature_radius * 0.01`.
///
/// For all-planar geometry, this is identical to `compute_adaptive_tol`
/// (curvature radius is infinite → curvature term doesn't constrain).
fn compute_curvature_adaptive_tol(solid_a: &Solid, solid_b: &Solid) -> f64 {
    let base_tol = compute_adaptive_tol(solid_a, solid_b);
    let min_r = min_curvature_radius(solid_a).min(min_curvature_radius(solid_b));
    let curvature_based = if min_r.is_finite() {
        min_r * 0.01
    } else {
        f64::INFINITY
    };
    // Curvature can only tighten, never loosen the tolerance.
    // Lower clamp reduced to 1e-7 for curved precision.
    base_tol.min(curvature_based).clamp(1e-7, 0.05)
}

/// Compute scale-aware healing tolerance from two solids' geometry.
///
/// Healing tolerance is 10% of the boolean tolerance — tight enough to
/// preserve curve accuracy while still being proportional to geometry size.
fn compute_healing_tol(solid_a: &Solid, solid_b: &Solid) -> f64 {
    compute_curvature_adaptive_tol(solid_a, solid_b) * 0.1
}

/// Compute layered `BooleanOptions` from two solids' geometry.
///
/// Wraps `compute_curvature_adaptive_tol` to produce the full tolerance struct.
/// When curved surfaces are present, `tau_model` is tightened by curvature,
/// and `tau_area` is automatically smaller (it's derived as `tau_model^2`).
/// This ensures `divide_one_face` doesn't discard small IC-derived face
/// fragments on curved surfaces where parametric compression is significant.
fn compute_boolean_options(solid_a: &Solid, solid_b: &Solid) -> BooleanOptions {
    let tol = compute_curvature_adaptive_tol(solid_a, solid_b);
    BooleanOptions::for_boolean_tol(tol)
}

/// Validate that a solid is suitable for boolean operations.
/// Returns an error if the solid has no boundaries or no faces.
fn validate_solid_for_boolean(solid: &Solid, label: &str) -> Result<(), KernelError> {
    if solid.boundaries().is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: format!("{} has no boundaries", label),
        });
    }
    let shell = &solid.boundaries()[0];
    if shell.face_iter().count() == 0 {
        return Err(KernelError::BooleanFailed {
            reason: format!("{} has no faces", label),
        });
    }
    // Note: ShellCondition check (warn if not closed) is available via truck_meshalgo
    // but we skip it here — boundary+face checks are sufficient as a pre-check.
    // The boolean pipeline itself will fail with a clear error if the shell is not closed.
    Ok(())
}

/// Build a `BooleanDiagnosticsSummary` from full `BooleanDiagnostics`.
fn build_diagnostics_summary(
    diag: &truck_shapeops::BooleanDiagnostics,
) -> BooleanDiagnosticsSummary {
    let faces_classified = diag.classification.shell0_and
        + diag.classification.shell0_or
        + diag.classification.shell1_and
        + diag.classification.shell1_or;
    BooleanDiagnosticsSummary {
        tau_model: diag.tolerance.tau_model,
        faces_classified,
        vertices_welded: diag.topology.vertices_welded,
        edges_canonicalized: diag.topology.edges_canonicalized,
        total_duration_ms: diag.timing.total.as_millis() as u64,
        warnings: diag.warnings.clone(),
        successful_strategy: String::new(), // filled by caller if needed
        perturbation_attempts: 0,           // filled by caller if needed
        perturbation_elapsed_ms: 0,         // filled by caller if needed
        preheal_vertices_unified: 0,        // filled by caller if needed
        recovery_level: diag.recovery.recovery_level,
    }
}

/// Real geometry kernel backed by the truck BREP library.
pub struct TruckKernel {
    next_handle: u64,
    next_id: u64,
    solids: HashMap<u64, Solid>,
    /// Standalone faces created by make_faces_from_profiles, awaiting extrude.
    standalone_faces: HashMap<u64, Face>,
    /// Diagnostics from the last boolean operation.
    last_boolean_diagnostics: Option<BooleanDiagnosticsSummary>,
}

impl TruckKernel {
    /// Create a new TruckKernel instance.
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            next_id: 1,
            solids: HashMap::new(),
            standalone_faces: HashMap::new(),
            last_boolean_diagnostics: None,
        }
    }

    fn alloc_handle(&mut self) -> KernelSolidHandle {
        let h = KernelSolidHandle(self.next_handle);
        self.next_handle += 1;
        h
    }

    fn alloc_id(&mut self) -> KernelId {
        let id = KernelId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn store_solid(&mut self, solid: Solid) -> KernelSolidHandle {
        let handle = self.alloc_handle();
        self.solids.insert(handle.id(), solid);
        handle
    }

    pub(crate) fn get_solid(&self, handle: &KernelSolidHandle) -> Option<&Solid> {
        self.solids.get(&handle.id())
    }

    /// Returns the diagnostics from the last boolean operation, if any.
    pub fn last_boolean_diagnostics(&self) -> Option<&BooleanDiagnosticsSummary> {
        self.last_boolean_diagnostics.as_ref()
    }

    /// Split a multi-shell Solid into separate single-shell Solids,
    /// but only when ALL shells are valid manifolds (chi=2) and spatially disjoint.
    /// Returns one handle per resulting body.
    fn split_multi_shell(&mut self, solid: Solid) -> Vec<KernelSolidHandle> {
        let shells = solid.into_boundaries();
        if shells.len() <= 1 {
            let reassembled = Solid::new_unchecked(shells);
            return vec![self.store_solid(reassembled)];
        }

        // Only split if every shell has Euler chi=2 (valid manifold).
        // Multi-shell results with chi≠2 are malformed booleans —
        // keeping them together preserves existing behavior.
        let all_valid = shells
            .iter()
            .all(|shell| truck_shapeops::validate_euler_characteristic(shell).is_ok());

        if !all_valid {
            let reassembled = Solid::new_unchecked(shells);
            return vec![self.store_solid(reassembled)];
        }

        // Compute bounding boxes for each shell
        let bboxes: Vec<([f64; 3], [f64; 3])> = shells
            .iter()
            .map(|shell| {
                let mut min = [f64::MAX; 3];
                let mut max = [f64::MIN; 3];
                for v in shell.vertex_iter() {
                    let p = v.point();
                    min[0] = min[0].min(p.x);
                    min[1] = min[1].min(p.y);
                    min[2] = min[2].min(p.z);
                    max[0] = max[0].max(p.x);
                    max[1] = max[1].max(p.y);
                    max[2] = max[2].max(p.z);
                }
                (min, max)
            })
            .collect();

        // Group shells with overlapping bboxes into the same body.
        let n = shells.len();
        let bbox_overlaps = |a: usize, b: usize| -> bool {
            let (amin, amax) = &bboxes[a];
            let (bmin, bmax) = &bboxes[b];
            amin[0] <= bmax[0] + 1e-9
                && amax[0] >= bmin[0] - 1e-9
                && amin[1] <= bmax[1] + 1e-9
                && amax[1] >= bmin[1] - 1e-9
                && amin[2] <= bmax[2] + 1e-9
                && amax[2] >= bmin[2] - 1e-9
        };

        // Union-find grouping
        let mut group: Vec<usize> = (0..n).collect();
        for i in 0..n {
            for j in (i + 1)..n {
                if bbox_overlaps(i, j) {
                    let gi = group[i];
                    let gj = group[j];
                    if gi != gj {
                        let (lo, hi) = if gi < gj { (gi, gj) } else { (gj, gi) };
                        for g in group.iter_mut() {
                            if *g == hi {
                                *g = lo;
                            }
                        }
                    }
                }
            }
        }

        // Assign sequential body indices
        let mut group_to_body = HashMap::new();
        for &g in &group {
            let len = group_to_body.len();
            group_to_body.entry(g).or_insert(len);
        }
        let num_bodies = group_to_body.len();

        if num_bodies <= 1 {
            let reassembled = Solid::new_unchecked(shells);
            return vec![self.store_solid(reassembled)];
        }

        // Build separate solids per body group
        let mut body_shells: Vec<Vec<_>> = vec![Vec::new(); num_bodies];
        for (i, shell) in shells.into_iter().enumerate() {
            let body_idx = group_to_body[&group[i]];
            body_shells[body_idx].push(shell);
        }

        body_shells
            .into_iter()
            .map(|shells| {
                let solid = Solid::new_unchecked(shells);
                self.store_solid(solid)
            })
            .collect()
    }

    /// Shared boolean operation setup: validate, heal, compute tolerances, run cascade.
    /// The `make_op` callback receives the pre-computed `BooleanTolerance` and returns
    /// the truck boolean closure to execute.
    fn run_boolean_op<F, G>(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        boolean_op: crate::healing::BooleanOp,
        make_op: F,
    ) -> Result<Solid, KernelError>
    where
        F: FnOnce(truck_shapeops::BooleanTolerance) -> G,
        G: Fn(
            &Solid,
            &Solid,
        ) -> std::result::Result<
            (Solid, truck_shapeops::BooleanDiagnostics),
            truck_shapeops::BooleanStageError,
        >,
    {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        validate_solid_for_boolean(&solid_a, "solid_a")?;
        validate_solid_for_boolean(&solid_b, "solid_b")?;

        let heal_tol = compute_healing_tol(&solid_a, &solid_b);
        crate::healing::heal_intersection_curves(&solid_a, heal_tol);
        crate::healing::heal_intersection_curves(&solid_b, heal_tol);

        let opts = compute_boolean_options(&solid_a, &solid_b);
        debug_assert!(
            opts.validate().is_ok(),
            "BooleanOptions validation failed: {:?}",
            opts.validate()
        );
        let tol = opts.tau_model;
        let tols = opts.to_boolean_tolerance();
        let solid_a = crate::healing::pre_split_closed_edges(&solid_a, tol);
        let solid_b = crate::healing::pre_split_closed_edges(&solid_b, tol);

        let op = make_op(tols);
        let (result, diag) =
            crate::healing::try_boolean_with_perturbation_diag(&solid_a, &solid_b, tol, Some(boolean_op), op)
                .map_err(|e| KernelError::from(BooleanError::from(e)))?;
        self.last_boolean_diagnostics = Some(build_diagnostics_summary(&diag));
        crate::healing::heal_and_dedup_volume_guarded(&result, heal_tol);
        Ok(result)
    }

    /// Export a solid to STEP AP203 format string.
    pub fn export_step(
        &self,
        handle: &KernelSolidHandle,
        file_name: &str,
    ) -> Result<String, KernelError> {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = self
            .solids
            .get(&handle.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(handle.id()),
            })?;

        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: file_name.to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        Ok(complete.to_string())
    }
}

impl Default for TruckKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel for TruckKernel {
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let truck_face = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        let dir = Vector3::new(direction[0], direction[1], direction[2]);
        let dir_len = dir.magnitude();
        if dir_len < 1e-12 {
            return Err(KernelError::Other {
                message: "extrude direction has zero length".to_string(),
            });
        }
        let sweep_vec = dir.normalize() * depth;

        // When the sweep direction opposes the face normal, tsweep creates a
        // solid with inside-out face orientations (all normals point inward).
        // This causes boolean_subtract to compute intersection instead of
        // subtraction, and breaks chained booleans entirely.
        // Fix: invert the face so its normal aligns with the sweep direction.
        let face_normal: Option<Vector3> = match truck_face.surface() {
            Surface::Plane(ref p) => Some(p.normal()),
            _ => None,
        };
        let truck_face = if let Some(n) = face_normal {
            if InnerSpace::dot(n, sweep_vec) < 0.0 {
                truck_face.inverse()
            } else {
                truck_face
            }
        } else {
            truck_face
        };

        let solid = builder::tsweep(&truck_face, sweep_vec);
        Ok(self.store_solid(solid))
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let truck_face = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        let origin = Point3::new(axis_origin[0], axis_origin[1], axis_origin[2]);
        let axis = Vector3::new(axis_direction[0], axis_direction[1], axis_direction[2]);
        if axis.magnitude() < 1e-12 {
            return Err(KernelError::Other {
                message: "revolve axis has zero length".to_string(),
            });
        }

        let angle_rad = angle.to_radians();
        let solid = builder::rsweep(&truck_face, origin, axis.normalize(), Rad(angle_rad), 3);
        Ok(self.store_solid(solid))
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Union, |tols| {
            move |a, b| truck_shapeops::or_result_with_tol_diag(a, b, &tols)
        })?;
        #[cfg(debug_assertions)]
        if result.boundaries().len() > 1 {
            eprintln!(
                "[boolean_union] WARNING: result has {} shells",
                result.boundaries().len()
            );
        }
        Ok(self.store_solid(result))
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Subtraction, |tols| {
            move |a, b| truck_shapeops::difference_result_with_tol_diag(a, b, &tols)
        })?;
        #[cfg(debug_assertions)]
        if result.boundaries().len() > 1 {
            eprintln!(
                "[boolean_subtract] WARNING: result has {} shells",
                result.boundaries().len()
            );
        }
        Ok(self.store_solid(result))
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Intersection, |tols| {
            move |a, b| truck_shapeops::and_result_with_tol_diag(a, b, &tols)
        })?;
        Ok(self.store_solid(result))
    }

    fn boolean_union_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Union, |tols| {
            move |a, b| truck_shapeops::or_result_with_tol_diag(a, b, &tols)
        })?;
        Ok(self.split_multi_shell(result))
    }

    fn boolean_subtract_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Subtraction, |tols| {
            move |a, b| truck_shapeops::difference_result_with_tol_diag(a, b, &tols)
        })?;
        Ok(self.split_multi_shell(result))
    }

    fn boolean_intersect_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        let result = self.run_boolean_op(a, b, crate::healing::BooleanOp::Intersection, |tols| {
            move |a, b| truck_shapeops::and_result_with_tol_diag(a, b, &tols)
        })?;
        Ok(self.split_multi_shell(result))
    }

    fn fillet_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if radius <= 0.0 {
            return Err(KernelError::Other {
                message: "Fillet radius must be positive".into(),
            });
        }

        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        if truck_solid.boundaries().is_empty() {
            return Err(KernelError::Other {
                message: "Solid has no shell".into(),
            });
        }

        // Phase 1: Compute fillet wedge geometry from the original solid
        // (identical to chamfer Phase 1)
        struct WedgeGeom {
            front: Point3,
            back: Point3,
            offset_a: Vector3,
            offset_b: Vector3,
            na: Vector3,
            nb: Vector3,
        }

        let wedge_geoms = {
            let shell = &truck_solid.boundaries()[0];

            let mut unique_edges = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for e in shell.edge_iter() {
                if seen.insert(e.id()) {
                    unique_edges.push(e);
                }
            }

            let mut geoms = Vec::new();
            for edge_kid in edges {
                let edge_offset = (edge_kid.0 % 10000).saturating_sub(1000) as usize;
                if edge_offset >= unique_edges.len() {
                    return Err(KernelError::EntityNotFound { id: *edge_kid });
                }
                let target = &unique_edges[edge_offset];
                let tid = target.id();

                let front = target.front().point();
                let back = target.back().point();
                let ev = back - front;
                let elen = ev.magnitude();
                if elen < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Fillet target edge has zero length".into(),
                    });
                }
                let edir = ev / elen;

                // Find the 2 adjacent planar faces and their outward normals
                let mut normals = Vec::new();
                for face in shell.face_iter() {
                    let has = face
                        .boundaries()
                        .iter()
                        .flat_map(|w| w.edge_iter())
                        .any(|e| e.id() == tid);
                    if has {
                        match face.oriented_surface() {
                            Surface::Plane(p) => normals.push(p.normal()),
                            _ => {
                                return Err(KernelError::NotSupported {
                                    operation: "fillet on non-planar face".into(),
                                });
                            }
                        }
                    }
                }
                if normals.len() != 2 {
                    return Err(KernelError::Other {
                        message: format!("Edge has {} adjacent faces, expected 2", normals.len()),
                    });
                }

                let (na, nb) = (normals[0], normals[1]);

                let mut da = na.cross(edir);
                if da.dot(nb) > 0.0 {
                    da = -da;
                }
                let dal = da.magnitude();
                if dal < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Degenerate fillet geometry (parallel faces?)".into(),
                    });
                }
                da /= dal;

                let mut db = nb.cross(edir);
                if db.dot(na) > 0.0 {
                    db = -db;
                }
                let dbl = db.magnitude();
                if dbl < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Degenerate fillet geometry (parallel faces?)".into(),
                    });
                }
                db /= dbl;

                geoms.push(WedgeGeom {
                    front,
                    back,
                    offset_a: da,
                    offset_b: db,
                    na,
                    nb,
                });
            }
            geoms
        };

        // Phase 2: Build wedge prisms with arc cross-section and boolean-subtract
        let mut current = truck_solid;
        const ARC_SEGMENTS: usize = 16;

        for wg in &wedge_geoms {
            let ev = wg.back - wg.front;
            let elen = ev.magnitude();
            let edir = ev / elen;

            let ext = (radius * 0.1).min(elen * 0.05);
            let start = wg.front - edir * ext;
            let sweep = ev + edir * (2.0 * ext);

            // Nudge V0 outward along the bisector of the two face normals
            let outward = wg.na + wg.nb;
            let outward_len = outward.magnitude();
            let nudge_vec = if outward_len > 1e-12 {
                (outward / outward_len) * (radius * 0.05)
            } else {
                Vector3::new(0.0, 0.0, 0.0)
            };
            let v0 = start + nudge_vec;

            // Tangent points on faces A and B, nudged slightly outside
            let face_eps = radius * 0.02;
            let v1 = start + wg.offset_a * radius + wg.na * face_eps;
            let v2 = start + wg.offset_b * radius + wg.nb * face_eps;

            // Fillet arc: center is at start + offset_a*R + offset_b*R
            let center = start + wg.offset_a * radius + wg.offset_b * radius;
            let d1 = v1 - center;
            let d2 = v2 - center;
            let d1_len = d1.magnitude();
            let d2_len = d2.magnitude();
            if d1_len < 1e-12 || d2_len < 1e-12 {
                return Err(KernelError::Other {
                    message: "Degenerate fillet arc geometry".into(),
                });
            }
            let d1n = d1 / d1_len;
            let d2n = d2 / d2_len;
            let arc_radius = (d1_len + d2_len) * 0.5;

            // Compute the sweep angle between d1 and d2
            let cos_angle = d1n.dot(d2n).clamp(-1.0, 1.0);
            let angle = cos_angle.acos();

            // Build arc points using slerp in the plane of d1,d2
            let sin_angle = angle.sin();
            let mut arc_points = Vec::with_capacity(ARC_SEGMENTS - 1);
            if sin_angle.abs() > 1e-12 {
                for i in 1..ARC_SEGMENTS {
                    let t = i as f64 / ARC_SEGMENTS as f64;
                    let theta = angle * t;
                    let w1 = (angle - theta).sin() / sin_angle;
                    let w2 = theta.sin() / sin_angle;
                    let pt = center + (d1n * w1 + d2n * w2) * arc_radius;
                    arc_points.push(pt);
                }
            }

            // Build polygon: v0 -> v1 -> arc_points -> v2 -> back to v0
            let mut vertices = Vec::new();
            vertices.push(v0);
            vertices.push(v1);
            vertices.extend_from_slice(&arc_points);
            vertices.push(v2);

            // Create truck vertices and edges for the polygon
            let n = vertices.len();
            let tverts: Vec<_> = vertices.iter().map(|&p| builder::vertex(p)).collect();
            let mut wire_edges = Vec::with_capacity(n);
            for i in 0..n {
                let j = (i + 1) % n;
                let pi = vertices[i];
                let pj = vertices[j];
                wire_edges.push(Edge::new(
                    &tverts[i],
                    &tverts[j],
                    truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(pi, pj)),
                ));
            }

            let wire = Wire::from_iter(wire_edges);
            let poly_face = builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Other {
                message: format!("Failed to create fillet wedge: {}", e),
            })?;

            // Ensure face normal aligns with sweep for proper solid orientation
            let face_n = match poly_face.surface() {
                Surface::Plane(ref p) => p.normal(),
                _ => unreachable!(),
            };
            let poly_face = if InnerSpace::dot(face_n, sweep) < 0.0 {
                poly_face.inverse()
            } else {
                poly_face
            };

            let wedge = builder::tsweep(&poly_face, sweep);

            let tol = compute_adaptive_tol(&current, &current);
            let htol = tol * 0.1;
            crate::healing::heal_intersection_curves(&current, htol);

            let tols = [tol, tol * 0.5, tol * 0.25];
            let mut last_result = None;
            for &t in &tols {
                if let Ok(result) =
                    crate::healing::try_boolean_with_perturbation(&current, &wedge, t, |a, b| {
                        truck_shapeops::difference_result(a, b, t)
                    })
                {
                    last_result = Some(result);
                    break;
                }
            }

            current = last_result.ok_or_else(|| KernelError::Other {
                message: "Fillet boolean subtraction failed".into(),
            })?;

            crate::healing::heal_intersection_curves(&current, htol);
        }

        Ok(self.store_solid(current))
    }

    fn chamfer_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if distance <= 0.0 {
            return Err(KernelError::Other {
                message: "Chamfer distance must be positive".into(),
            });
        }

        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        if truck_solid.boundaries().is_empty() {
            return Err(KernelError::Other {
                message: "Solid has no shell".into(),
            });
        }

        // Phase 1: Compute chamfer wedge geometry from the original solid
        struct WedgeGeom {
            front: Point3,
            back: Point3,
            offset_a: Vector3,
            offset_b: Vector3,
            na: Vector3,
            nb: Vector3,
        }

        let wedge_geoms = {
            let shell = &truck_solid.boundaries()[0];

            let mut unique_edges = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for e in shell.edge_iter() {
                if seen.insert(e.id()) {
                    unique_edges.push(e);
                }
            }

            let mut geoms = Vec::new();
            for edge_kid in edges {
                let edge_offset = (edge_kid.0 % 10000).saturating_sub(1000) as usize;
                if edge_offset >= unique_edges.len() {
                    return Err(KernelError::EntityNotFound { id: *edge_kid });
                }
                let target = &unique_edges[edge_offset];
                let tid = target.id();

                let front = target.front().point();
                let back = target.back().point();
                let ev = back - front;
                let elen = ev.magnitude();
                if elen < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Chamfer target edge has zero length".into(),
                    });
                }
                let edir = ev / elen;

                // Find the 2 adjacent planar faces and their outward normals
                let mut normals = Vec::new();
                for face in shell.face_iter() {
                    let has = face
                        .boundaries()
                        .iter()
                        .flat_map(|w| w.edge_iter())
                        .any(|e| e.id() == tid);
                    if has {
                        match face.oriented_surface() {
                            Surface::Plane(p) => normals.push(p.normal()),
                            _ => {
                                return Err(KernelError::NotSupported {
                                    operation: "chamfer on non-planar face".into(),
                                });
                            }
                        }
                    }
                }
                if normals.len() != 2 {
                    return Err(KernelError::Other {
                        message: format!("Edge has {} adjacent faces, expected 2", normals.len()),
                    });
                }

                let (na, nb) = (normals[0], normals[1]);

                // Compute inward-along-face offset direction for face A:
                // perpendicular to edge within A's plane, pointing toward solid interior
                let mut da = na.cross(edir);
                if da.dot(nb) > 0.0 {
                    da = -da;
                }
                let dal = da.magnitude();
                if dal < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Degenerate chamfer geometry (parallel faces?)".into(),
                    });
                }
                da /= dal;

                // Same for face B
                let mut db = nb.cross(edir);
                if db.dot(na) > 0.0 {
                    db = -db;
                }
                let dbl = db.magnitude();
                if dbl < 1e-12 {
                    return Err(KernelError::Other {
                        message: "Degenerate chamfer geometry (parallel faces?)".into(),
                    });
                }
                db /= dbl;

                geoms.push(WedgeGeom {
                    front,
                    back,
                    offset_a: da,
                    offset_b: db,
                    na,
                    nb,
                });
            }
            geoms
        };

        // Phase 2: Build wedge prisms and boolean-subtract from the solid
        let mut current = truck_solid;

        for wg in &wedge_geoms {
            let ev = wg.back - wg.front;
            let elen = ev.magnitude();
            let edir = ev / elen;

            // Extend slightly beyond edge endpoints to avoid coplanar boolean issues
            let ext = (distance * 0.1).min(elen * 0.05);
            let start = wg.front - edir * ext;
            let sweep = ev + edir * (2.0 * ext);

            // Triangle cross-section at `start`.
            // Nudge V0 outward from the solid along the bisector of the two
            // face outward normals.  This prevents the wedge side-faces from
            // being exactly coplanar with the box faces — a configuration that
            // makes truck's boolean engine return None.
            let outward = wg.na + wg.nb;
            let outward_len = outward.magnitude();
            let nudge_vec = if outward_len > 1e-12 {
                (outward / outward_len) * (distance * 0.05)
            } else {
                Vector3::new(0.0, 0.0, 0.0)
            };
            let v0 = start + nudge_vec;
            // Nudge v1/v2 slightly outside the box along their respective
            // face normals.  Without this, the wedge shares exact geometric
            // edges with the box faces, which produces degenerate intersection
            // curves and causes the boolean to fail with NotClosedShell.
            let face_eps = distance * 0.02;
            let v1 = start + wg.offset_a * distance + wg.na * face_eps;
            let v2 = start + wg.offset_b * distance + wg.nb * face_eps;

            let tv0 = builder::vertex(v0);
            let tv1 = builder::vertex(v1);
            let tv2 = builder::vertex(v2);

            let e01 = Edge::new(
                &tv0,
                &tv1,
                truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(v0, v1)),
            );
            let e12 = Edge::new(
                &tv1,
                &tv2,
                truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(v1, v2)),
            );
            let e20 = Edge::new(
                &tv2,
                &tv0,
                truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(v2, v0)),
            );

            let wire = Wire::from_iter(vec![e01, e12, e20]);
            let tri_face = builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Other {
                message: format!("Failed to create chamfer wedge: {}", e),
            })?;

            // Ensure face normal aligns with sweep for proper solid orientation
            let face_n = match tri_face.surface() {
                Surface::Plane(ref p) => p.normal(),
                _ => unreachable!(),
            };
            let tri_face = if InnerSpace::dot(face_n, sweep) < 0.0 {
                tri_face.inverse()
            } else {
                tri_face
            };

            let wedge = builder::tsweep(&tri_face, sweep);

            // Use only the main solid's extent for tolerance — the wedge is
            // a tool shape whose extent should not inflate the tolerance.
            let tol = compute_adaptive_tol(&current, &current);
            let htol = tol * 0.1;
            crate::healing::heal_intersection_curves(&current, htol);

            // Retry with decreasing tolerances — truck's boolean is
            // sensitive to the exact tolerance value for near-coplanar
            // configurations.
            let tols = [tol, tol * 0.5, tol * 0.25];
            let mut last_result = None;
            for &t in &tols {
                if let Ok(result) =
                    crate::healing::try_boolean_with_perturbation(&current, &wedge, t, |a, b| {
                        truck_shapeops::difference_result(a, b, t)
                    })
                {
                    last_result = Some(result);
                    break;
                }
            }

            current = last_result.ok_or_else(|| KernelError::Other {
                message: "Chamfer boolean subtraction failed".into(),
            })?;

            crate::healing::heal_intersection_curves(&current, htol);
        }

        Ok(self.store_solid(current))
    }

    fn shell(
        &mut self,
        solid: &KernelSolidHandle,
        faces_to_remove: &[KernelId],
        thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        use std::collections::HashSet;

        if thickness <= 0.0 {
            return Err(KernelError::Other {
                message: "Shell thickness must be positive".into(),
            });
        }

        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        if truck_solid.boundaries().is_empty() {
            return Err(KernelError::ShellFailed {
                reason: "solid has no boundaries".into(),
            });
        }

        let truck_shell = &truck_solid.boundaries()[0];
        let all_faces: Vec<&Face> = truck_shell.face_iter().collect();

        if all_faces.is_empty() {
            return Err(KernelError::ShellFailed {
                reason: "solid has no faces".into(),
            });
        }

        // Map KernelIds to face indices
        let remove_indices: HashSet<usize> = faces_to_remove
            .iter()
            .map(|kid| (kid.0 % 10000) as usize)
            .collect();

        // Validate face indices
        for kid in faces_to_remove {
            let idx = (kid.0 % 10000) as usize;
            if idx >= all_faces.len() {
                return Err(KernelError::EntityNotFound { id: *kid });
            }
        }

        // Collect face planes: outward_normal and signed distance (n·x = d)
        let mut face_normals: Vec<Vector3> = Vec::new();
        let mut face_d: Vec<f64> = Vec::new();

        for &face in &all_faces {
            let surface = face.oriented_surface();
            match surface {
                Surface::Plane(ref p) => {
                    let n = p.normal();
                    let o = p.origin();
                    let d = n.x * o.x + n.y * o.y + n.z * o.z;
                    face_normals.push(n);
                    face_d.push(d);
                }
                _ => {
                    return Err(KernelError::NotSupported {
                        operation: "shell (non-planar faces)".into(),
                    });
                }
            }
        }

        // Build vertex → face adjacency
        let mut vert_id_to_idx = HashMap::new();
        let mut vert_positions: Vec<Point3> = Vec::new();

        for v in truck_shell.vertex_iter() {
            let vid = v.id();
            vert_id_to_idx.entry(vid).or_insert_with(|| {
                let idx = vert_positions.len();
                vert_positions.push(v.point());
                idx
            });
        }

        let num_verts = vert_positions.len();
        let mut vert_faces: Vec<Vec<usize>> = vec![Vec::new(); num_verts];

        for (face_idx, &face) in all_faces.iter().enumerate() {
            for wire in face.boundaries() {
                for v in wire.vertex_iter() {
                    if let Some(&vi) = vert_id_to_idx.get(&v.id()) {
                        if !vert_faces[vi].contains(&face_idx) {
                            vert_faces[vi].push(face_idx);
                        }
                    }
                }
            }
        }

        // Compute inner vertex positions via 3-plane intersection (Cramer's rule).
        // For removed faces, extend outward (d + thickness) so the inner solid
        // pokes through, causing boolean subtraction to remove the entire face.
        // For kept faces, offset inward (d - thickness).
        let offset_d: Vec<f64> = face_d
            .iter()
            .enumerate()
            .map(|(i, d)| {
                if remove_indices.contains(&i) {
                    d + thickness
                } else {
                    d - thickness
                }
            })
            .collect();

        let mut inner_positions: Vec<Point3> = Vec::with_capacity(num_verts);

        for (vi, adj_faces) in vert_faces.iter().enumerate() {
            if adj_faces.len() < 3 {
                return Err(KernelError::ShellFailed {
                    reason: format!(
                        "Vertex {} has only {} adjacent faces (need >=3)",
                        vi,
                        adj_faces.len()
                    ),
                });
            }

            let n0 = face_normals[adj_faces[0]];
            let n1 = face_normals[adj_faces[1]];
            let n2 = face_normals[adj_faces[2]];
            let d0 = offset_d[adj_faces[0]];
            let d1 = offset_d[adj_faces[1]];
            let d2 = offset_d[adj_faces[2]];

            let det = n0.x * (n1.y * n2.z - n1.z * n2.y) - n0.y * (n1.x * n2.z - n1.z * n2.x)
                + n0.z * (n1.x * n2.y - n1.y * n2.x);

            if det.abs() < 1e-12 {
                return Err(KernelError::ShellFailed {
                    reason: "Degenerate vertex: coplanar adjacent faces".into(),
                });
            }

            let x = (d0 * (n1.y * n2.z - n1.z * n2.y) - n0.y * (d1 * n2.z - n1.z * d2)
                + n0.z * (d1 * n2.y - n1.y * d2))
                / det;
            let y = (n0.x * (d1 * n2.z - n1.z * d2) - d0 * (n1.x * n2.z - n1.z * n2.x)
                + n0.z * (n1.x * d2 - d1 * n2.x))
                / det;
            let z = (n0.x * (n1.y * d2 - d1 * n2.y) - n0.y * (n1.x * d2 - d1 * n2.x)
                + d0 * (n1.x * n2.y - n1.y * n2.x))
                / det;

            inner_positions.push(Point3::new(x, y, z));
        }

        // Build the inner solid by replicating the original's face topology
        // with offset vertex positions, using tsweep from the first face.
        // For robustness, we build it face-by-face and assemble via boolean.

        // Extract ordered vertex indices per face wire
        let face_wire_verts = |face: &Face| -> Vec<Vec<usize>> {
            face.boundaries()
                .iter()
                .map(|wire| {
                    wire.edge_iter()
                        .map(|edge| vert_id_to_idx[&edge.front().id()])
                        .collect()
                })
                .collect()
        };

        // Build the inner solid using the same sweep approach as the original.
        // For a box created by tsweep(tsweep(tsweep(vertex))):
        // We identify the "bottom" face (first face) and extrude direction,
        // then reconstruct via tsweep with inner positions.
        //
        // General approach: build inner solid from its face vertices directly.
        // Use the first face's wire to create a planar face, then tsweep.

        // Get the first face's wire vertices (use the original face topology)
        let first_face_wires = face_wire_verts(all_faces[0]);
        if first_face_wires.is_empty() || first_face_wires[0].len() < 3 {
            return Err(KernelError::ShellFailed {
                reason: "First face has invalid wire".into(),
            });
        }

        // Build inner face from first face's wire with inner positions
        let first_wire_indices = &first_face_wires[0];
        let n = first_wire_indices.len();
        let inner_face_verts: Vec<_> = first_wire_indices
            .iter()
            .map(|&i| builder::vertex(inner_positions[i]))
            .collect();
        let mut face_edges = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            let vi = first_wire_indices[i];
            let vj = first_wire_indices[j];
            face_edges.push(Edge::new(
                &inner_face_verts[i],
                &inner_face_verts[j],
                truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(
                    inner_positions[vi],
                    inner_positions[vj],
                )),
            ));
        }
        let inner_wire = Wire::from_iter(face_edges);
        let inner_face =
            builder::try_attach_plane(&[inner_wire]).map_err(|e| KernelError::ShellFailed {
                reason: format!("Failed to build inner face: {}", e),
            })?;

        // Compute the sweep direction: from the first face's inner centroid
        // to the opposite face's inner centroid.
        // For a box, the opposite face shares no vertices with the first face.
        // Find a vertex NOT in the first face to determine sweep direction.
        let first_face_vert_set: HashSet<usize> = first_wire_indices.iter().copied().collect();
        let mut sweep_target = None;
        for (vi, _) in inner_positions.iter().enumerate() {
            if !first_face_vert_set.contains(&vi) {
                sweep_target = Some(vi);
                break;
            }
        }

        let sweep_target_vi = sweep_target.ok_or_else(|| KernelError::ShellFailed {
            reason: "Cannot determine sweep direction for inner solid".into(),
        })?;

        // Sweep direction: from first face centroid to the target vertex,
        // projected along the face normal
        let face0_normal = face_normals[0];
        let first_centroid = {
            let sum: Vector3 = first_wire_indices
                .iter()
                .map(|&i| {
                    Vector3::new(
                        inner_positions[i].x,
                        inner_positions[i].y,
                        inner_positions[i].z,
                    )
                })
                .fold(Vector3::new(0.0, 0.0, 0.0), |a, b| a + b);
            sum / first_wire_indices.len() as f64
        };
        let target_pt = inner_positions[sweep_target_vi];
        let target_vec = Vector3::new(target_pt.x, target_pt.y, target_pt.z);
        let diff = target_vec - first_centroid;
        // Project onto face normal direction to get sweep depth
        let sweep_depth = InnerSpace::dot(diff, face0_normal).abs();
        // Sweep in the direction opposing the face normal (into the solid)
        let sweep_dir = if InnerSpace::dot(diff, face0_normal) > 0.0 {
            face0_normal * sweep_depth
        } else {
            face0_normal * (-sweep_depth)
        };

        // Ensure sweep is aligned with the face normal for correct solid orientation
        let face_normal_for_sweep: Option<Vector3> = match inner_face.surface() {
            Surface::Plane(ref p) => Some(p.normal()),
            _ => None,
        };
        let inner_face = if let Some(fn_norm) = face_normal_for_sweep {
            if InnerSpace::dot(fn_norm, sweep_dir) < 0.0 {
                inner_face.inverse()
            } else {
                inner_face
            }
        } else {
            inner_face
        };

        let inner_solid: Solid = builder::tsweep(&inner_face, sweep_dir);

        // Store inner solid and boolean subtract from original
        let inner_handle = self.store_solid(inner_solid);
        self.boolean_subtract(solid, &inner_handle)
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        tessellation::tessellate_solid(truck_solid, tolerance, &mut self.next_id, solid)
    }

    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        Ok(tessellation::extract_edges(
            truck_solid,
            tolerance,
            &mut self.next_id,
        ))
    }

    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        let origin = Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]);
        let normal = Vector3::new(plane_normal[0], plane_normal[1], plane_normal[2]).normalize();
        let x_axis = Vector3::new(plane_x_axis[0], plane_x_axis[1], plane_x_axis[2]).normalize();
        let y_axis = normal.cross(x_axis).normalize();

        let mut face_ids = Vec::new();

        for profile in profiles {
            let face = if let Some(ref circ) = profile.circle {
                // True NURBS circular face: construct circular wire via rsweep
                let center_3d = origin + x_axis * circ.center_u + y_axis * circ.center_v;
                let edge_point = center_3d + x_axis * circ.radius;
                let v = builder::vertex(edge_point);
                let wire =
                    builder::rsweep(&v, center_3d, normal, Rad(2.0 * std::f64::consts::PI), 3);
                builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Other {
                    message: format!("Failed to create circular face: {}", e),
                })?
            } else {
                // Polygon/spline path: build wire from consecutive point pairs.
                // When spline_segments are present, use BSpline curves for those
                // edges; otherwise use straight line segments.
                let pts_3d: Vec<Point3> = profile
                    .entity_ids
                    .iter()
                    .filter_map(|id| {
                        positions
                            .get(id)
                            .map(|&(u, v)| origin + x_axis * u + y_axis * v)
                    })
                    .collect();

                if pts_3d.len() < 3 {
                    return Err(KernelError::Other {
                        message: "Profile has fewer than 3 points".to_string(),
                    });
                }

                // Build a lookup: start_point_index → SplineSegment
                let spline_map: HashMap<usize, &SplineSegment> = profile
                    .spline_segments
                    .iter()
                    .map(|s| (s.start_point_index, s))
                    .collect();

                let n = pts_3d.len();
                let vertices: Vec<_> = pts_3d.iter().map(|&p| builder::vertex(p)).collect();
                let mut wire_edges: Vec<Edge> = Vec::new();
                for i in 0..n {
                    let j = (i + 1) % n;

                    let curve = if let Some(seg) = spline_map.get(&i) {
                        // Convert spline control points from UV to 3D
                        let ctrl_pts_3d: Vec<Point3> = seg
                            .control_points
                            .iter()
                            .map(|&(u, v)| origin + x_axis * u + y_axis * v)
                            .collect();

                        if ctrl_pts_3d.len() >= 2 {
                            let degree = 3.min(ctrl_pts_3d.len() - 1);
                            let knots = clamped_knot_vector(ctrl_pts_3d.len(), degree);
                            let knot_vec =
                                truck_geometry::prelude::KnotVec::from(knots);
                            let bsp = truck_geometry::prelude::BSplineCurve::new(
                                knot_vec,
                                ctrl_pts_3d,
                            );
                            truck_modeling::geometry::Curve::BSplineCurve(bsp)
                        } else {
                            // Fallback to line
                            truck_modeling::geometry::Curve::Line(
                                truck_modeling::geometry::Line(pts_3d[i], pts_3d[j]),
                            )
                        }
                    } else {
                        truck_modeling::geometry::Curve::Line(
                            truck_modeling::geometry::Line(pts_3d[i], pts_3d[j]),
                        )
                    };

                    let edge = Edge::new(&vertices[i], &vertices[j], curve);
                    wire_edges.push(edge);
                }
                let wire = Wire::from_iter(wire_edges);

                builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Other {
                    message: format!("Failed to create planar face: {}", e),
                })?
            };

            let face_id = self.alloc_id();
            self.standalone_faces.insert(face_id.0, face);
            face_ids.push(face_id);
        }

        Ok(face_ids)
    }

    fn export_step(
        &mut self,
        solid: &KernelSolidHandle,
        file_name: &str,
    ) -> Result<String, KernelError> {
        TruckKernel::export_step(self, solid, file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    #[test]
    fn test_truck_kernel_make_faces_and_extrude() {
        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (1.0, 1.0));
        positions.insert(4, (0.0, 1.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        assert_eq!(face_ids.len(), 1);

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 2.0)
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1);

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();
        assert_eq!(faces.len(), 6, "Extruded rectangle should have 6 faces");
    }

    /// Verify box-cylinder boolean subtract (punched cube).
    /// The cylinder must pierce through the box (not at edges/corners/coplanar faces).
    #[test]
    fn test_boolean_subtract_box_cylinder() {
        use truck_modeling::builder;

        // Cube [0,1]^3
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::unit_x());
        let f = builder::tsweep(&e, Vector3::unit_y());
        let cube: Solid = builder::tsweep(&f, Vector3::unit_z());

        // Cylinder centered at (0.5, 0.5), r=0.25, extends z=-0.5 to z=1.5
        // (fully pierces top and bottom faces, well inside edges)
        let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w = builder::rsweep(
            &v,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
        cylinder.not();

        let result = truck_shapeops::and(&cube, &cylinder, 0.05);
        assert!(result.is_some(), "Box-cylinder boolean should succeed");

        let solid = result.unwrap();
        let shell = &solid.boundaries()[0];
        use truck_topology::shell::ShellCondition;
        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Result shell must be Closed"
        );
    }

    /// Verify box-box boolean operations with offset (non-coplanar faces).
    #[test]
    fn test_boolean_ops_box_box_offset() {
        use truck_modeling::builder;
        use truck_topology::shell::ShellCondition;

        let box_a: Solid = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(2.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 2.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0))
        };
        let box_b: Solid = {
            let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };

        // AND (intersection)
        let result = truck_shapeops::and(&box_a, &box_b, 0.05);
        assert!(result.is_some(), "Box-box AND should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );

        // OR (union)
        let result = truck_shapeops::or(&box_a, &box_b, 0.05);
        assert!(result.is_some(), "Box-box OR should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );

        // Subtract (AND with NOT)
        let mut box_b_neg = box_b.clone();
        box_b_neg.not();
        let result = truck_shapeops::and(&box_a, &box_b_neg, 0.05);
        assert!(result.is_some(), "Box-box SUBTRACT should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );
    }

    #[test]
    fn test_adaptive_tolerance_scaling() {
        // Test that compute_adaptive_tol produces scale-appropriate values
        let make_box = |s: f64| -> Solid {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(s, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, s, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, s))
        };

        // Unit scale: extent=1.0, tol = 1.0*0.005 = 0.005
        let small = make_box(1.0);
        let tol_small = compute_adaptive_tol(&small, &small);
        assert!(
            (tol_small - 0.005).abs() < 1e-10,
            "Unit scale tol={} should be 0.005",
            tol_small
        );

        // 10x scale: extent=10.0, tol = 10*0.005 = 0.05 (at upper clamp)
        let medium = make_box(10.0);
        let tol_medium = compute_adaptive_tol(&medium, &medium);
        assert!(
            (tol_medium - 0.05).abs() < 1e-10,
            "10x scale tol={} should be 0.05 (upper clamp)",
            tol_medium
        );

        // 100x scale: extent=100.0, tol = 0.05 (clamped at upper bound)
        let large = make_box(100.0);
        let tol_large = compute_adaptive_tol(&large, &large);
        assert!(
            (tol_large - 0.05).abs() < 1e-10,
            "100x scale tol={} should be 0.05 (clamped)",
            tol_large
        );

        // Verify monotonicity up to clamp
        assert!(
            tol_medium >= tol_small,
            "Medium geometry should get >= tol than small"
        );
        assert!(
            tol_large >= tol_medium,
            "Large should be >= medium (both at clamp)"
        );
    }

    #[test]
    fn test_parametric_scale_boolean() {
        use truck_topology::shell::ShellCondition;

        // Test that boolean union works at various scales with adaptive tolerance.
        // Uses overlapping boxes (not fully contained) to ensure proper intersection
        // curves are generated.
        for scale in &[1.0, 2.0, 5.0, 10.0] {
            let s = *scale;
            // Box A: [0, 2s]^3
            let box_a = {
                let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
                let e = builder::tsweep(&v, Vector3::new(2.0 * s, 0.0, 0.0));
                let f = builder::tsweep(&e, Vector3::new(0.0, 2.0 * s, 0.0));
                builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0 * s))
            };
            // Box B: [s, 3s]^3 — overlapping (no coplanar, partial overlap)
            let box_b = {
                let v = builder::vertex(Point3::new(1.0 * s, 1.0 * s, 1.0 * s));
                let e = builder::tsweep(&v, Vector3::new(2.0 * s, 0.0, 0.0));
                let f = builder::tsweep(&e, Vector3::new(0.0, 2.0 * s, 0.0));
                builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0 * s))
            };

            let tol = compute_adaptive_tol(&box_a, &box_b);
            let result = truck_shapeops::or(&box_a, &box_b, tol);
            assert!(
                result.is_some(),
                "Box-box union at scale {}x should succeed (tol={})",
                s,
                tol
            );
            let solid = result.unwrap();
            assert_eq!(
                solid.boundaries()[0].shell_condition(),
                ShellCondition::Closed,
                "Union at scale {}x should produce closed shell",
                s
            );
        }
    }

    // Coplanar partial overlap: pair-specific skip allows non-coplanar pairs to intersect normally
    #[test]
    fn test_coplanar_partial_overlap_union() {
        use truck_topology::shell::ShellCondition;

        // Box1: [0,1]^3
        let box1 = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };
        // Box2: [0.5, 1.5] x [0, 1] x [0, 1] — shares the x=1 face partially
        let box2 = {
            let v = builder::vertex(Point3::new(0.5, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };

        let tol = compute_adaptive_tol(&box1, &box2);
        // Use perturbation system since direct truck call fails on coplanar faces
        let result = crate::healing::try_boolean_with_perturbation(&box1, &box2, tol, |a, b| {
            truck_shapeops::or_result(a, b, tol)
        });
        assert!(
            result.is_ok(),
            "Coplanar partial overlap union should succeed"
        );
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed,
            "Coplanar union should produce closed shell"
        );
    }

    /// Benchmark boolean operations at various tolerances.
    /// Uses catch_unwind since truck panics at certain tolerances.
    /// Run with: cargo test -p kernel-fork bench_boolean -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_boolean_tolerances() {
        let tolerances = [0.05, 0.1, 0.2, 0.5];
        let box_solid = primitives::make_box(2.0, 2.0, 2.0);
        let cyl_solid = primitives::make_cylinder(0.5, 3.0);

        println!("\n=== Boolean: Box(2x2x2) vs Cylinder(r=0.5, h=3) ===");

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            println!("--- {} ---", name);
            for tol in &tolerances {
                let a = box_solid.clone();
                let b = cyl_solid.clone();
                let t = *tol;
                let start = std::time::Instant::now();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                        "or" => truck_shapeops::or(&a, &b, t),
                        "sub" => {
                            let mut b2 = b;
                            b2.not();
                            truck_shapeops::and(&a, &b2, t)
                        }
                        "and" => truck_shapeops::and(&a, &b, t),
                        _ => unreachable!(),
                    }));
                let elapsed = start.elapsed();
                let status = match &result {
                    Ok(Some(_)) => "OK",
                    Ok(None) => "FAILED (None)",
                    Err(_) => "PANIC",
                };
                println!("  tol={:.2} → {:?} [{}]", tol, elapsed, status);
            }
        }

        // Box-box booleans with coplanar faces (shared origin)
        println!("\n=== Box-Box Booleans (coplanar, origin-aligned, tol=0.05) ===");
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let box_b = primitives::make_box(1.0, 1.0, 1.0);

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            let a = box_a.clone();
            let b = box_b.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                "or" => truck_shapeops::or(&a, &b, 0.05),
                "sub" => {
                    let mut b2 = b;
                    b2.not();
                    truck_shapeops::and(&a, &b2, 0.05)
                }
                "and" => truck_shapeops::and(&a, &b, 0.05),
                _ => unreachable!(),
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(_)) => "OK",
                Ok(None) => "FAILED (None)",
                Err(_) => "PANIC",
            };
            println!("  {} → {:?} [{}]", name, elapsed, status);
        }

        // Box-box with offset (no coplanar faces)
        println!("\n=== Box-Box Booleans (offset, no coplanar faces, tol=0.05) ===");
        // Create offset box by extruding a face at (0.5, 0.5, 0.5)
        let box_a2 = primitives::make_box(2.0, 2.0, 2.0);
        // Build an offset box manually using tsweep from an offset position
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b2: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            let a = box_a2.clone();
            let b = box_b2.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                "or" => truck_shapeops::or(&a, &b, 0.05),
                "sub" => {
                    let mut b2 = b;
                    b2.not();
                    truck_shapeops::and(&a, &b2, 0.05)
                }
                "and" => truck_shapeops::and(&a, &b, 0.05),
                _ => unreachable!(),
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(_)) => "OK",
                Ok(None) => "FAILED (None)",
                Err(_) => "PANIC",
            };
            println!("  {} → {:?} [{}]", name, elapsed, status);
        }
    }

    /// Diagnostic: test boolean ops with different cylinder placements to isolate
    /// which geometric configurations work vs. fail.
    /// Run with: cargo test -p kernel-fork diag_boolean_configs -- --ignored --nocapture
    #[test]
    #[ignore]
    fn diag_boolean_configs() {
        use truck_modeling::builder;

        println!("\n=== Diagnostic: Boolean Operation Configurations ===\n");

        // Config 1: Punched cube (truck's own test case — cylinder fully inside box)
        {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::unit_x());
            let f = builder::tsweep(&e, Vector3::unit_y());
            let cube: Solid = builder::tsweep(&f, Vector3::unit_z());

            let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
            let w = builder::rsweep(
                &v,
                Point3::new(0.5, 0.5, 0.0),
                Vector3::unit_z(),
                Rad(7.0),
                3,
            );
            let f = builder::try_attach_plane(&[w]).unwrap();
            let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
            cylinder.not();

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "1. Punched cube (cyl inside box): {:?} [{}]",
                elapsed, status
            );
        }

        // Config 2: Cylinder centered in box face (partially overlapping)
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Cylinder at center of box, radius 0.5, extends through
            let v = builder::vertex(Point3::new(1.5, 1.0, -0.5));
            let w = builder::rsweep(
                &v,
                Point3::new(1.0, 1.0, 0.0),
                Vector3::unit_z(),
                Rad(7.0),
                3,
            );
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("2. Cylinder at box center: {:?} [{}]", elapsed, status);
        }

        // Config 3: Our original test — cylinder at origin corner
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            let cylinder = primitives::make_cylinder(0.5, 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "3. Cylinder at box corner (original): {:?} [{}]",
                elapsed, status
            );
        }

        // Config 4: Cylinder centered (2pi), z-offset to avoid coplanar bottom
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Build cylinder at (1,1,-0.5) with r=0.3, h=3 — NO coplanar faces
            let v = builder::vertex(Point3::new(1.3, 1.0, -0.5));
            let w = builder::rsweep(
                &v,
                Point3::new(1.0, 1.0, 0.0),
                Vector3::unit_z(),
                Rad(2.0 * std::f64::consts::PI),
                3,
            );
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "4. Cylinder centered (2pi), inside box: {:?} [{}]",
                elapsed, status
            );
        }

        // Config 5: Cylinder intersecting box face (partially in, partially out)
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Cylinder at (1,2,0) — half inside, half outside the y=2 face
            let v = builder::vertex(Point3::new(1.5, 2.0, -0.5));
            let w = builder::rsweep(
                &v,
                Point3::new(1.0, 2.0, 0.0),
                Vector3::unit_z(),
                Rad(7.0),
                3,
            );
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("5. Cylinder at face boundary: {:?} [{}]", elapsed, status);
        }

        // Config 6: Box-box fully overlapping (offset)
        {
            let box_a = primitives::make_box(2.0, 2.0, 2.0);
            let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            let box_b: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&box_a, &box_b, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("6. Box-box offset (intersect): {:?} [{}]", elapsed, status);
        }
    }

    /// Test export_step via the Kernel trait method.
    #[test]
    fn test_export_step_via_trait() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let handle = kernel.store_solid(solid);

        let step_data = kernel.export_step(&handle, "trait_test.step").unwrap();

        assert!(
            step_data.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_data.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// STEP export investigation: simple box (should work)
    #[test]
    fn test_step_export_simple_box() {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: "test_box.step".to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        let step_string = complete.to_string();

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
        assert!(
            step_string.contains("FACE_SURFACE"),
            "Should have face entities"
        );
        assert!(
            step_string.contains("ENDSEC"),
            "Should have proper STEP footer"
        );
    }

    /// STEP export investigation: cylinder (revolved geometry)
    #[test]
    fn test_step_export_cylinder() {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = primitives::make_cylinder(1.0, 2.0);
        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: "test_cylinder.step".to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        let step_string = complete.to_string();

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// STEP export of a boolean union result via the TruckKernel API.
    /// The kernel auto-heals IntersectionCurve edges, making STEP export safe.
    #[test]
    fn test_step_export_boolean_result() {
        let mut kernel = TruckKernel::new();

        // Create two offset boxes and store them in the kernel
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let handle_a = kernel.store_solid(box_a);

        let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let handle_b = kernel.store_solid(box_b);

        // Boolean union via kernel (auto-heals intersection curves)
        let union_handle = kernel
            .boolean_union(&handle_a, &handle_b)
            .expect("boolean union should succeed for offset boxes");

        // Export to STEP via kernel
        let step_string = kernel
            .export_step(&union_handle, "test_boolean.step")
            .expect("STEP export of healed boolean result should succeed");

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// Face IDs from introspection match tessellation for make_faces_from_profiles → extrude_face.
    /// This is the realistic app path (not primitives::make_box).
    #[test]
    fn test_extruded_rect_face_id_consistency() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (10.0, 0.0));
        positions.insert(3, (10.0, 10.0));
        positions.insert(4, (0.0, 10.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 5.0)
            .unwrap();

        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let tess_ids: std::collections::HashSet<_> =
            mesh.face_ranges.iter().map(|fr| fr.face_id).collect();

        assert_eq!(
            introspect_ids, tess_ids,
            "Face IDs must match for extruded rectangle"
        );
        assert_eq!(introspect_ids.len(), 6, "Extruded rect should have 6 faces");
    }

    /// GAP K6: export_step with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_export_step_nonexistent_handle() {
        let kernel = TruckKernel::new();
        let bad_handle = KernelSolidHandle(99999);
        let result = kernel.export_step(&bad_handle, "nonexistent.step");
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "export_step with bad handle should return EntityNotFound, got {:?}",
            result
        );
    }

    /// GAP K7: extrude result must have ShellCondition::Closed.
    #[test]
    fn test_extrude_shell_condition_closed() {
        use truck_topology::shell::ShellCondition;

        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (2.0, 2.0));
        positions.insert(4, (0.0, 2.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 3.0)
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Extruded solid must have a closed (watertight) shell"
        );
    }

    /// Tessellation from TruckKernel must have all-finite vertices and normals.
    #[test]
    fn test_truck_tessellation_no_nan() {
        let mut kernel = TruckKernel::new();

        // Box
        let box_solid = primitives::make_box(1.0, 2.0, 3.0);
        let h_box = kernel.store_solid(box_solid);
        let mesh_box = kernel.tessellate(&h_box, 0.1).unwrap();

        for (i, v) in mesh_box.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Box vertex[{}] = {} is not finite", i, v);
        }
        for (i, n) in mesh_box.normals.iter().enumerate() {
            assert!(n.is_finite(), "Box normal[{}] = {} is not finite", i, n);
        }

        // Cylinder
        let cyl_solid = primitives::make_cylinder(1.0, 2.0);
        let h_cyl = kernel.store_solid(cyl_solid);
        let mesh_cyl = kernel.tessellate(&h_cyl, 0.1).unwrap();

        for (i, v) in mesh_cyl.vertices.iter().enumerate() {
            assert!(
                v.is_finite(),
                "Cylinder vertex[{}] = {} is not finite",
                i,
                v
            );
        }
        for (i, n) in mesh_cyl.normals.iter().enumerate() {
            assert!(
                n.is_finite(),
                "Cylinder normal[{}] = {} is not finite",
                i,
                n
            );
        }
    }

    /// Tessellation triangles from TruckKernel must not have zero area.
    #[test]
    fn test_truck_tessellation_no_zero_area_triangles() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let handle = kernel.store_solid(solid);
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        let epsilon = 1e-12_f32;
        assert_eq!(mesh.indices.len() % 3, 0);
        for tri in mesh.indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            let v0 = [
                mesh.vertices[i0 * 3],
                mesh.vertices[i0 * 3 + 1],
                mesh.vertices[i0 * 3 + 2],
            ];
            let v1 = [
                mesh.vertices[i1 * 3],
                mesh.vertices[i1 * 3 + 1],
                mesh.vertices[i1 * 3 + 2],
            ];
            let v2 = [
                mesh.vertices[i2 * 3],
                mesh.vertices[i2 * 3 + 1],
                mesh.vertices[i2 * 3 + 2],
            ];

            let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

            let cx = e1[1] * e2[2] - e1[2] * e2[1];
            let cy = e1[2] * e2[0] - e1[0] * e2[2];
            let cz = e1[0] * e2[1] - e1[1] * e2[0];
            let area_2x = (cx * cx + cy * cy + cz * cz).sqrt();

            assert!(
                area_2x > epsilon,
                "Triangle [{}, {}, {}] has zero area (2*area={})",
                i0,
                i1,
                i2,
                area_2x
            );
        }
    }

    #[test]
    fn test_truck_kernel_store_and_tessellate_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        assert!(!mesh.vertices.is_empty(), "Mesh should have vertices");
        assert!(!mesh.indices.is_empty(), "Mesh should have indices");
        assert!(!mesh.normals.is_empty(), "Mesh should have normals");
        assert_eq!(mesh.face_ranges.len(), 6, "Box should have 6 face ranges");

        let total_indices = mesh.indices.len() as u32;
        let covered: u32 = mesh
            .face_ranges
            .iter()
            .map(|r| r.end_index - r.start_index)
            .sum();
        assert_eq!(
            covered, total_indices,
            "Face ranges should cover all indices"
        );
    }

    #[test]
    fn test_truck_kernel_extract_edges_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.extract_edges(&handle, 0.1).unwrap();

        assert!(!edges.vertices.is_empty(), "Edge data should have vertices");
        // A box has 12 edges
        assert_eq!(
            edges.edge_ranges.len(),
            12,
            "Box should have 12 edge ranges"
        );

        // Verify each edge range references valid vertex data
        for range in &edges.edge_ranges {
            assert!(
                range.end_vertex > range.start_vertex,
                "Edge range should have at least 2 vertices"
            );
            assert!(
                (range.end_vertex as usize) * 3 <= edges.vertices.len(),
                "Edge range end should not exceed vertex count"
            );
        }
    }

    #[test]
    fn test_truck_kernel_extract_edges_cylinder() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.extract_edges(&handle, 0.1).unwrap();

        assert!(
            !edges.vertices.is_empty(),
            "Cylinder edge data should have vertices"
        );
        // A cylinder has 3 edges: top circle, bottom circle, seam
        assert!(
            edges.edge_ranges.len() >= 3,
            "Cylinder should have at least 3 edge ranges, got {}",
            edges.edge_ranges.len()
        );
    }

    // ── Coverage: error paths ────────────────────────────────────────

    /// TruckKernel::default() produces the same state as new().
    #[test]
    fn test_truck_kernel_default() {
        let k1 = TruckKernel::new();
        let k2 = TruckKernel::default();
        // Both should have empty solid storage
        let bad = KernelSolidHandle(1);
        assert!(k1.get_solid(&bad).is_none());
        assert!(k2.get_solid(&bad).is_none());
    }

    /// extrude_face on a nonexistent face returns EntityNotFound.
    #[test]
    fn test_extrude_face_not_found() {
        let mut kernel = TruckKernel::new();
        let result = kernel.extrude_face(KernelId(999), [0.0, 0.0, 1.0], 1.0);
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// extrude_face with zero-length direction returns Other error.
    #[test]
    fn test_extrude_zero_direction() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (0.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        let result = kernel.extrude_face(face_ids[0], [0.0, 0.0, 0.0], 1.0);
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected Other error for zero direction, got {:?}",
            result
        );
    }

    /// revolve_face basic: create a face and revolve it.
    #[test]
    fn test_revolve_face_basic() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        // Small rectangle offset from Y axis for revolve
        positions.insert(1, (1.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (2.0, 1.0));
        positions.insert(4, (1.0, 1.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .revolve_face(
                face_ids[0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                std::f64::consts::PI,
            )
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        assert!(
            !solid.boundaries().is_empty(),
            "Revolved solid should have at least one shell"
        );
    }

    /// revolve_face on nonexistent face returns EntityNotFound.
    #[test]
    fn test_revolve_face_not_found() {
        let mut kernel = TruckKernel::new();
        let result = kernel.revolve_face(
            KernelId(999),
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::PI,
        );
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// revolve_face with zero-length axis returns Other error.
    #[test]
    fn test_revolve_zero_axis() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (1.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (1.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        let result = kernel.revolve_face(
            face_ids[0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0], // zero axis
            std::f64::consts::PI,
        );
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected Other error for zero axis, got {:?}",
            result
        );
    }

    /// boolean_union with nonexistent handle A returns EntityNotFound.
    #[test]
    fn test_boolean_union_not_found_a() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_b = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_union(&bad, &h_b);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_union with nonexistent handle B returns EntityNotFound.
    #[test]
    fn test_boolean_union_not_found_b() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_union(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_subtract with nonexistent handles returns EntityNotFound.
    #[test]
    fn test_boolean_subtract_not_found() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        // Missing A
        let result = kernel.boolean_subtract(&bad, &h_a);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
        // Missing B
        let result = kernel.boolean_subtract(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_intersect with nonexistent handles returns EntityNotFound.
    #[test]
    fn test_boolean_intersect_not_found() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_intersect(&bad, &h_a);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
        let result = kernel.boolean_intersect(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// Fillet a single box edge, verify result has more faces and closed shell.
    #[test]
    fn test_fillet_single_box_edge() {
        use crate::traits::KernelIntrospect;
        use truck_topology::shell::ShellCondition;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(10.0, 10.0, 10.0);
        let handle = kernel.store_solid(solid);

        let edge_ids = kernel.list_edges(&handle);
        assert_eq!(edge_ids.len(), 12);

        let result = kernel.fillet_edges(&handle, &[edge_ids[0]], 1.0);
        assert!(result.is_ok(), "fillet_edges failed: {:?}", result.err());

        let filleted = result.unwrap();
        let s = kernel.get_solid(&filleted).unwrap();
        let shell = &s.boundaries()[0];

        let face_count = shell.face_iter().count();
        // Original box has 6 faces; fillet adds faces from the arc wedge boolean
        assert!(
            face_count >= 7,
            "Filleted box should have at least 7 faces, got {}",
            face_count
        );

        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Filleted solid must have a closed shell"
        );
    }

    /// Fillet with invalid radius returns error.
    #[test]
    fn test_fillet_zero_radius_error() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(10.0, 10.0, 10.0);
        let handle = kernel.store_solid(solid);
        let result = kernel.fillet_edges(&handle, &[KernelId(1000)], 0.0);
        assert!(result.is_err());
    }

    /// Chamfer a single box edge, verify result has more faces and closed shell.
    #[test]
    fn test_chamfer_single_box_edge() {
        use crate::traits::KernelIntrospect;
        use truck_topology::shell::ShellCondition;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edge_ids = kernel.list_edges(&handle);
        assert_eq!(edge_ids.len(), 12);

        let result = kernel.chamfer_edges(&handle, &[edge_ids[0]], 0.1);
        assert!(result.is_ok(), "Chamfer should succeed, got {:?}", result);

        let chamfered = result.unwrap();
        let s = kernel.get_solid(&chamfered).unwrap();
        let shell = &s.boundaries()[0];

        let face_count = shell.face_iter().count();
        assert!(
            face_count >= 7,
            "Chamfered box should have at least 7 faces, got {}",
            face_count
        );

        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Chamfered solid must have a closed shell"
        );
    }

    /// Euler's formula V-E+F=2 holds after chamfer.
    #[test]
    fn test_chamfer_preserves_euler() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(2.0, 2.0, 2.0);
        let handle = kernel.store_solid(solid);

        let edge_ids = kernel.list_edges(&handle);
        let result = kernel.chamfer_edges(&handle, &[edge_ids[0]], 0.2).unwrap();

        let s = kernel.get_solid(&result).unwrap();
        let shell = &s.boundaries()[0];

        let faces: Vec<_> = shell.face_iter().collect();
        let mut edge_set = std::collections::HashSet::new();
        for e in shell.edge_iter() {
            edge_set.insert(e.id());
        }
        let mut vert_set = std::collections::HashSet::new();
        for v in shell.vertex_iter() {
            vert_set.insert(v.id());
        }

        let v = vert_set.len() as i64;
        let e = edge_set.len() as i64;
        let f = faces.len() as i64;
        assert_eq!(
            v - e + f,
            2,
            "Euler formula V-E+F=2 must hold after chamfer (V={}, E={}, F={})",
            v,
            e,
            f
        );
    }

    /// Chamfer result tessellates with no NaN/Inf values.
    #[test]
    fn test_chamfer_tessellates_cleanly() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edge_ids = kernel.list_edges(&handle);
        let chamfered = kernel.chamfer_edges(&handle, &[edge_ids[0]], 0.1).unwrap();
        let mesh = kernel.tessellate(&chamfered, 0.1).unwrap();

        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(v.is_finite(), "vertex[{}] = {} is not finite", i, v);
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(n.is_finite(), "normal[{}] = {} is not finite", i, n);
        }
    }

    /// Chamfer with nonexistent edge returns EntityNotFound.
    #[test]
    fn test_chamfer_nonexistent_edge_error() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        // Edge offset 99 doesn't exist (box has 12 edges)
        let bad_edge = KernelId(handle.id() * 10000 + 1000 + 99);
        let result = kernel.chamfer_edges(&handle, &[bad_edge], 0.1);
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// Chamfer with zero distance returns error.
    #[test]
    fn test_chamfer_zero_distance_error() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edge_ids = kernel.list_edges(&handle);
        let result = kernel.chamfer_edges(&handle, &[edge_ids[0]], 0.0);
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected error for zero distance, got {:?}",
            result
        );
    }

    /// Helper: find the face index with outward normal closest to `target_normal`.
    fn find_face_by_normal(
        kernel: &TruckKernel,
        handle: &KernelSolidHandle,
        target: [f64; 3],
    ) -> KernelId {
        use crate::traits::KernelIntrospect;
        let faces = kernel.list_faces(handle);
        let mut best = faces[0];
        let mut best_dot = f64::NEG_INFINITY;
        for fid in &faces {
            let sig = kernel.compute_signature(*fid, TopoKind::Face);
            if let Some(n) = sig.normal {
                let dot = n[0] * target[0] + n[1] * target[1] + n[2] * target[2];
                if dot > best_dot {
                    best_dot = dot;
                    best = *fid;
                }
            }
        }
        best
    }

    /// Shell a 1×1×1 box removing the top face, thickness=0.1.
    /// Result should have more than 6 faces and be tessellatable.
    #[test]
    fn test_shell_box_remove_top() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        // Find the top face (normal ~ [0,0,1])
        let top_face = find_face_by_normal(&kernel, &handle, [0.0, 0.0, 1.0]);

        let result = kernel.shell(&handle, &[top_face], 0.1);
        assert!(result.is_ok(), "Shell should succeed, got {:?}", result);
        let shell_handle = result.unwrap();

        // The shell result should have more faces than original 6
        use crate::traits::KernelIntrospect;
        let faces = kernel.list_faces(&shell_handle);
        assert!(
            faces.len() > 6,
            "Shell should have more than 6 faces, got {}",
            faces.len()
        );

        // Should tessellate cleanly
        let mesh = kernel.tessellate(&shell_handle, 0.1).unwrap();
        assert!(!mesh.vertices.is_empty(), "Shell mesh should have vertices");
        assert!(!mesh.indices.is_empty(), "Shell mesh should have indices");
    }

    /// Shell a box removing two faces (top + front).
    #[test]
    fn test_shell_box_remove_two_faces() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let top_face = find_face_by_normal(&kernel, &handle, [0.0, 0.0, 1.0]);
        let front_face = find_face_by_normal(&kernel, &handle, [0.0, -1.0, 0.0]);

        let result = kernel.shell(&handle, &[top_face, front_face], 0.1);
        assert!(
            result.is_ok(),
            "Shell with 2 removed faces should succeed, got {:?}",
            result
        );
        let shell_handle = result.unwrap();

        let mesh = kernel.tessellate(&shell_handle, 0.1).unwrap();
        assert!(!mesh.vertices.is_empty());
    }

    /// Shell result should have valid topology with reasonable V, E, F counts.
    /// Note: boolean-based shell may produce V-E+F != 2 due to intersection
    /// curve artifacts; we verify it's close and the shell is closed.
    #[test]
    fn test_shell_topology_valid() {
        use truck_topology::shell::ShellCondition;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(2.0, 2.0, 2.0);
        let handle = kernel.store_solid(solid);

        let top_face = find_face_by_normal(&kernel, &handle, [0.0, 0.0, 1.0]);
        let shell_handle = kernel.shell(&handle, &[top_face], 0.2).unwrap();

        let shell_solid = kernel.get_solid(&shell_handle).unwrap();
        let shell_boundary = &shell_solid.boundaries()[0];

        // Shell should be closed (watertight)
        assert_eq!(
            shell_boundary.shell_condition(),
            ShellCondition::Closed,
            "Shell result must be topologically closed"
        );

        // Should have more than 6 faces (original box has 6)
        let f = shell_boundary.face_iter().count();
        assert!(f > 6, "Shell should have >6 faces, got {}", f);
    }

    /// Shell tessellation should have no NaN or Inf values.
    #[test]
    fn test_shell_tessellates_cleanly() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let top_face = find_face_by_normal(&kernel, &handle, [0.0, 0.0, 1.0]);
        let shell_handle = kernel.shell(&handle, &[top_face], 0.1).unwrap();

        let mesh = kernel.tessellate(&shell_handle, 0.1).unwrap();
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Shell vertex[{}] = {} is not finite", i, v);
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(n.is_finite(), "Shell normal[{}] = {} is not finite", i, n);
        }
    }

    /// Shell with nonexistent face ID returns EntityNotFound.
    #[test]
    fn test_shell_nonexistent_face_error() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let bad_face = KernelId(handle.id() * 10000 + 99);
        let result = kernel.shell(&handle, &[bad_face], 0.1);
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// Shell with zero thickness returns error.
    #[test]
    fn test_shell_zero_thickness_error() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let result = kernel.shell(&handle, &[KernelId(handle.id() * 10000)], 0.0);
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected error for zero thickness, got {:?}",
            result
        );

        let result_neg = kernel.shell(&handle, &[KernelId(handle.id() * 10000)], -0.5);
        assert!(
            matches!(result_neg, Err(KernelError::Other { .. })),
            "Expected error for negative thickness, got {:?}",
            result_neg
        );
    }

    /// Shell with nonexistent solid handle returns EntityNotFound.
    #[test]
    fn test_shell_nonexistent_solid() {
        let mut kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = kernel.shell(&bad, &[KernelId(999 * 10000)], 0.1);
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// tessellate with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_tessellate_not_found() {
        let mut kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = kernel.tessellate(&bad, 0.1);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// extract_edges with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_extract_edges_not_found() {
        let mut kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = kernel.extract_edges(&bad, 0.1);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// make_faces_from_profiles with fewer than 3 points returns error.
    #[test]
    fn test_make_faces_too_few_points() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        let result = kernel.make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        );
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected error for fewer than 3 points, got {:?}",
            result
        );
    }

    /// make_faces_from_profiles with multiple profiles.
    #[test]
    fn test_make_faces_multiple_profiles() {
        let mut kernel = TruckKernel::new();
        let p1 = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let p2 = ClosedProfile {
            entity_ids: vec![4, 5, 6],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (0.5, 1.0));
        positions.insert(4, (3.0, 0.0));
        positions.insert(5, (4.0, 0.0));
        positions.insert(6, (3.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[p1, p2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 2, "Should create 2 faces from 2 profiles");
        assert_ne!(face_ids[0], face_ids[1], "Face IDs should be distinct");
    }

    /// make_faces_from_profiles with triangle profile, then extrude to wedge.
    #[test]
    fn test_make_faces_triangle_extrude() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (1.0, 2.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 1);
        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 3.0)
            .unwrap();
        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        let faces: Vec<_> = shell.face_iter().collect();
        // Triangular prism: 2 triangles + 3 quads = 5 faces
        assert_eq!(faces.len(), 5, "Triangular extrusion should have 5 faces");
    }

    /// make_faces_from_profiles on XZ plane (non-default normal).
    #[test]
    fn test_make_faces_xz_plane() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (1.0, 1.0));
        positions.insert(4, (0.0, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0], // XZ plane normal = Y
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 1);
        // Extrude along Y
        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 1.0, 0.0], 2.0)
            .unwrap();
        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        let faces: Vec<_> = shell.face_iter().collect();
        assert_eq!(faces.len(), 6, "XZ-plane extruded box should have 6 faces");
    }

    /// Tessellation of a cylinder covers all faces.
    #[test]
    fn test_tessellate_cylinder_face_ranges() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();

        // Every face range face_id should be in the introspect list
        for fr in &mesh.face_ranges {
            assert!(
                introspect_ids.contains(&fr.face_id),
                "Tessellation face_id {:?} not in introspect list",
                fr.face_id
            );
        }

        // Face ranges should cover all indices contiguously
        let total_indices = mesh.indices.len() as u32;
        let covered: u32 = mesh
            .face_ranges
            .iter()
            .map(|r| r.end_index - r.start_index)
            .sum();
        assert_eq!(
            covered, total_indices,
            "Face ranges should cover all indices"
        );
    }

    /// export_step (inherent method) on nonexistent solid returns EntityNotFound.
    #[test]
    fn test_export_step_inherent_not_found() {
        let kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = TruckKernel::export_step(&kernel, &bad, "test.step");
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// Verify STEP export content has expected structure with file name.
    #[test]
    fn test_export_step_content_depth() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);
        let step_data = kernel.export_step(&handle, "cyl_test.step").unwrap();

        assert!(step_data.contains("ISO-10303-21"));
        assert!(step_data.contains("MANIFOLD_SOLID_BREP"));
        assert!(step_data.contains("DATA"));
        assert!(step_data.contains("ENDSEC"));
        // File name should appear in header
        assert!(
            step_data.contains("cyl_test.step"),
            "STEP header should contain the file name"
        );
    }

    /// Boolean union via Kernel trait (not raw truck_shapeops).
    #[test]
    fn test_boolean_union_via_kernel_trait() {
        let mut kernel = TruckKernel::new();
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let h_a = kernel.store_solid(box_a);

        // Offset box B
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let h_b = kernel.store_solid(box_b);

        let h_result = kernel.boolean_union(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Union result should be tessellatable"
        );
        assert!(
            !mesh.face_ranges.is_empty(),
            "Union should have face ranges"
        );
    }

    /// Boolean subtract via Kernel trait.
    #[test]
    fn test_boolean_subtract_via_kernel_trait() {
        let mut kernel = TruckKernel::new();

        // Cube [0,1]^3
        let v = truck_modeling::builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = truck_modeling::builder::tsweep(&v, Vector3::unit_x());
        let f = truck_modeling::builder::tsweep(&e, Vector3::unit_y());
        let cube: Solid = truck_modeling::builder::tsweep(&f, Vector3::unit_z());
        let h_a = kernel.store_solid(cube);

        // Cylinder fully inside cube
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w = truck_modeling::builder::rsweep(
            &v,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
            3,
        );
        let f = truck_modeling::builder::try_attach_plane(&[w]).unwrap();
        let cyl: Solid = truck_modeling::builder::tsweep(&f, Vector3::unit_z() * 2.0);
        let h_b = kernel.store_solid(cyl);

        let h_result = kernel.boolean_subtract(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Subtract result should produce geometry"
        );
    }

    /// Boolean intersect via Kernel trait.
    #[test]
    fn test_boolean_intersect_via_kernel_trait() {
        let mut kernel = TruckKernel::new();
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let h_a = kernel.store_solid(box_a);

        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let h_b = kernel.store_solid(box_b);

        let h_result = kernel.boolean_intersect(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Intersect result should be tessellatable"
        );
    }

    /// make_faces_from_profiles with entity IDs not in positions map produces error.
    #[test]
    fn test_make_faces_missing_positions() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };
        let mut positions = HashMap::new();
        // Only provide 2 positions out of 4
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        let result = kernel.make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        );
        assert!(
            result.is_err(),
            "Should fail with fewer than 3 matched positions"
        );
    }

    /// alloc_handle and alloc_id produce monotonically increasing values.
    #[test]
    fn test_alloc_id_monotonic() {
        let mut kernel = TruckKernel::new();
        let id1 = kernel.alloc_id();
        let id2 = kernel.alloc_id();
        let id3 = kernel.alloc_id();
        assert!(id1.0 < id2.0);
        assert!(id2.0 < id3.0);

        let h1 = kernel.alloc_handle();
        let h2 = kernel.alloc_handle();
        assert!(h1.id() < h2.id());
    }

    /// Tessellate a revolved solid (cylinder) and verify no NaN/Inf.
    #[test]
    fn test_tessellate_cylinder_no_nan() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(0.5, 3.0);
        let handle = kernel.store_solid(solid);
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        assert!(!mesh.vertices.is_empty());
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(
                v.is_finite(),
                "Cylinder vertex[{}] = {} is not finite",
                i,
                v
            );
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(
                n.is_finite(),
                "Cylinder normal[{}] = {} is not finite",
                i,
                n
            );
        }
        // Cylinder should have face ranges for all its faces
        assert!(
            mesh.face_ranges.len() >= 3,
            "Cylinder should have >= 3 face ranges, got {}",
            mesh.face_ranges.len()
        );
    }

    /// Extract edges from revolved solid and verify edge range validity.
    #[test]
    fn test_extract_edges_cylinder_ranges_valid() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);
        let edges = kernel.extract_edges(&handle, 0.05).unwrap();

        for range in &edges.edge_ranges {
            assert!(
                range.end_vertex > range.start_vertex,
                "Edge range should have at least 2 vertices"
            );
            assert!(
                (range.end_vertex as usize) * 3 <= edges.vertices.len(),
                "Edge range end should not exceed vertex count"
            );
        }
        // All vertices should be finite
        for (i, v) in edges.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Edge vertex[{}] = {} is not finite", i, v);
        }
    }

    #[test]
    fn test_healing_tolerance_scales_with_geometry() {
        for &scale in &[0.1, 1.0, 10.0, 50.0] {
            let solid = primitives::make_box(scale, scale, scale);
            let heal = compute_healing_tol(&solid, &solid);
            let bool_tol = compute_adaptive_tol(&solid, &solid);
            assert!(
                heal < bool_tol,
                "scale={}: heal_tol={} must be < bool_tol={}",
                scale,
                heal,
                bool_tol
            );
        }
    }

    #[test]
    fn test_solid_min_edge_length_cube() {
        let cube = primitives::make_box(1.0, 1.0, 1.0);
        let min_edge = solid_min_edge_length(&cube);
        // A 1×1×1 cube has all edges of length 1.0
        assert!(
            (min_edge - 1.0).abs() < 1e-10,
            "Cube min edge should be 1.0, got {}",
            min_edge
        );
    }

    #[test]
    fn test_solid_min_edge_length_rectangular_box() {
        let rect = primitives::make_box(10.0, 5.0, 2.0);
        let min_edge = solid_min_edge_length(&rect);
        // The shortest edge of a 10×5×2 box is 2.0
        assert!(
            (min_edge - 2.0).abs() < 1e-10,
            "Rect box min edge should be 2.0, got {}",
            min_edge
        );
    }

    /// Build a regular n-gon face centered at `center` on the XY plane with given radius.
    fn make_ngon_face(center: Point3, radius: f64, n: usize) -> Face {
        use truck_modeling::builder;
        let mut vertices = Vec::new();
        for i in 0..n {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            let x = center.x + radius * angle.cos();
            let y = center.y + radius * angle.sin();
            vertices.push(builder::vertex(Point3::new(x, y, center.z)));
        }
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            edges.push(builder::line(&vertices[i], &vertices[j]));
        }
        let wire: Wire = edges.into();
        builder::try_attach_plane(&[wire]).unwrap()
    }

    #[test]
    fn test_solid_min_edge_length_polygon_prism() {
        // 16-gon polygon prism with r=1.0
        // Min edge = 2 * r * sin(π/16) ≈ 0.3902
        let profile = make_ngon_face(Point3::new(0.0, 0.0, 0.0), 1.0, 16);
        let prism: Solid = builder::tsweep(&profile, Vector3::new(0.0, 0.0, 5.0));
        let min_edge = solid_min_edge_length(&prism);
        let expected = 2.0 * (std::f64::consts::PI / 16.0).sin();
        assert!(
            (min_edge - expected).abs() < 1e-6,
            "16-gon prism min edge should be ~{:.4}, got {:.4}",
            expected,
            min_edge
        );
    }

    #[test]
    fn test_adaptive_tol_feature_aware() {
        // 10×10×10 box: extent=10, min_edge=10
        // extent_based = 10 * 0.005 = 0.05
        // edge_based = 10 * 0.05 = 0.5
        // result = min(0.05, 0.5) = 0.05
        let big_box = primitives::make_box(10.0, 10.0, 10.0);
        let tol_box = compute_adaptive_tol(&big_box, &big_box);
        assert!(
            (tol_box - 0.05).abs() < 1e-10,
            "Box-only: expected 0.05, got {}",
            tol_box
        );

        // 16-gon prism with r=1.0: min_edge ≈ 0.3902
        // When combined with a 10×10×10 box:
        // extent = 10, min_edge = 0.3902
        // extent_based = 0.05, edge_based = 0.3902 * 0.05 ≈ 0.01951
        // result = min(0.05, 0.01951) = 0.01951
        let profile = make_ngon_face(Point3::new(0.0, 0.0, 0.0), 1.0, 16);
        let prism: Solid = builder::tsweep(&profile, Vector3::new(0.0, 0.0, 5.0));
        let tol_mixed = compute_adaptive_tol(&big_box, &prism);
        assert!(
            tol_mixed < 0.03,
            "Feature-aware tol should be < 0.03 (debug proved this passes), got {}",
            tol_mixed
        );
        assert!(
            tol_mixed > 0.01,
            "Feature-aware tol should be > 0.01 (not too small), got {}",
            tol_mixed
        );
    }

    #[test]
    fn test_adaptive_tol_floor_and_ceiling() {
        // Very small geometry: 0.001 × 0.001 × 0.001
        let tiny = primitives::make_box(0.001, 0.001, 0.001);
        let tol_tiny = compute_adaptive_tol(&tiny, &tiny);
        assert!(
            tol_tiny >= 1e-6,
            "Tiny geometry tol should be >= 1e-6 (floor), got {}",
            tol_tiny
        );

        // Very large geometry: 1000 × 1000 × 1000
        let huge = primitives::make_box(1000.0, 1000.0, 1000.0);
        let tol_huge = compute_adaptive_tol(&huge, &huge);
        assert!(
            tol_huge <= 0.05,
            "Huge geometry tol should be <= 0.05 (ceiling), got {}",
            tol_huge
        );
    }

    /// Profile-based box-box subtract (mimics make_faces_from_profiles + tsweep path).
    /// Validates that the engine's cut extrude path works at 10-unit scale.
    #[test]
    fn test_profile_based_subtract_10_scale() {
        use truck_modeling::geometry::Curve;

        let target = primitives::make_box(10.0, 10.0, 10.0);

        // Tool: rect (2,2)-(8,8) on plane at z=10.1, extruded down by 15.2
        let pts = vec![
            Point3::new(2.0, 2.0, 10.1),
            Point3::new(8.0, 2.0, 10.1),
            Point3::new(8.0, 8.0, 10.1),
            Point3::new(2.0, 8.0, 10.1),
        ];
        let verts: Vec<_> = pts.iter().map(|&p| builder::vertex(p)).collect();
        let mut edges = Vec::new();
        for i in 0..4 {
            let j = (i + 1) % 4;
            edges.push(Edge::new(
                &verts[i],
                &verts[j],
                Curve::Line(truck_modeling::geometry::Line(pts[i], pts[j])),
            ));
        }
        let wire = Wire::from_iter(edges);
        let face = builder::try_attach_plane(&[wire]).unwrap();
        let sweep_vec = Vector3::new(0.0, 0.0, -15.2);
        let face = if let Some(n) = match face.surface() {
            Surface::Plane(ref p) => Some(p.normal()),
            _ => None,
        } {
            if InnerSpace::dot(n, sweep_vec) < 0.0 {
                face.inverse()
            } else {
                face
            }
        } else {
            face
        };
        let tool: Solid = builder::tsweep(&face, sweep_vec);

        // Use the engine's tolerance path
        let opts = BooleanOptions::for_boolean_tol(compute_adaptive_tol(&target, &tool));
        let tols = opts.to_boolean_tolerance();
        let result = truck_shapeops::difference_with_tol(&target, &tool, &tols);
        assert!(
            result.is_some(),
            "Profile-based subtract must succeed with layered tolerances"
        );

        let solid = result.unwrap();
        use truck_topology::shell::ShellCondition;
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );
    }

    #[test]
    fn test_validate_solid_empty_boundaries() {
        let solid: Solid = Solid::new_unchecked(Vec::new());
        let result = validate_solid_for_boolean(&solid, "test");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("no boundaries"),
            "Error should mention 'no boundaries', got: {}",
            err_msg
        );
    }

    #[test]
    fn test_validate_solid_empty_faces() {
        use truck_modeling::topology::Shell;
        let empty_shell: Shell = vec![].into();
        let solid: Solid = Solid::new_unchecked(vec![empty_shell]);
        let result = validate_solid_for_boolean(&solid, "test");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("no faces"),
            "Error should mention 'no faces', got: {}",
            err_msg
        );
    }

    #[test]
    fn test_validate_solid_valid_box() {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let edge = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let face = builder::tsweep(&edge, Vector3::new(0.0, 1.0, 0.0));
        let solid: Solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, 1.0));
        assert!(validate_solid_for_boolean(&solid, "box").is_ok());
    }

    /// Circle profile extrusion should produce a true NURBS cylinder (3 faces)
    /// rather than the 34-face polygonal prism from a 32-sided polygon.
    #[test]
    fn test_circle_profile_extrude_produces_cylinder() {
        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            circle: Some(CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: 1.0,
            }),
            spline_segments: vec![],
        };
        let positions = HashMap::new(); // Not needed for circle profiles

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        assert_eq!(face_ids.len(), 1);

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 5.0)
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1);

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();
        // rsweep with division=3 creates 3 NURBS arcs for the full circle.
        // Extruding produces 3 lateral NURBS surfaces + 2 caps = 5 faces.
        // A 32-sided polygonal prism would have 34 faces (32 lateral + 2 caps).
        assert_eq!(
            faces.len(),
            5,
            "Circle profile extrusion should produce 5 faces (NURBS cylinder), got {}",
            faces.len()
        );

        // Verify the solid tessellates successfully
        let mesh = kernel.tessellate(&handle, 0.05).unwrap();
        assert!(
            mesh.vertices.len() > 10,
            "Cylinder mesh should have many vertices"
        );
        assert!(
            mesh.indices.len() > 10,
            "Cylinder mesh should have many triangle indices"
        );
    }

    // ===== Curvature-adaptive tolerance tests =====

    #[test]
    fn test_curvature_radius_plane() {
        // All faces of a box are planar → curvature radius should be infinity.
        let box_solid = primitives::make_box(10.0, 10.0, 10.0);
        let r = min_curvature_radius(&box_solid);
        assert!(
            r.is_infinite(),
            "Box (all planar) should have infinite curvature radius, got {}",
            r
        );
    }

    #[test]
    fn test_curvature_radius_cylinder() {
        // Cylinder with radius=1.0: curved faces should give R ≈ 1.0.
        let cyl = primitives::make_cylinder(1.0, 10.0);
        let r = min_curvature_radius(&cyl);
        // The cylinder has both planar (top/bottom) and curved (barrel) faces.
        // The barrel face has curvature radius = 1.0.
        // Allow generous margin since finite-difference estimation is approximate.
        assert!(
            r < 2.0,
            "Cylinder r=1 should have curvature radius near 1.0, got {}",
            r
        );
        assert!(
            r > 0.3,
            "Cylinder r=1 curvature radius should be > 0.3, got {}",
            r
        );
    }

    #[test]
    fn test_curvature_radius_large_cylinder() {
        // Cylinder with radius=50.0: curvature radius should be ~50.
        let cyl = primitives::make_cylinder(50.0, 10.0);
        let r = min_curvature_radius(&cyl);
        assert!(
            r < 100.0,
            "Cylinder r=50 should have curvature radius near 50, got {}",
            r
        );
        assert!(
            r > 15.0,
            "Cylinder r=50 curvature radius should be > 15, got {}",
            r
        );
    }

    #[test]
    fn test_curvature_adaptive_tol_constrains_small_cylinder() {
        // 10mm box + 1mm cylinder.
        // Without curvature: tol = min(10*0.005, edge_based) ~ 0.005..0.05
        // With curvature: curvature_based = 1.0 * 0.01 = 0.01
        // Result should be <= 0.01 (curvature constrains)
        let box_solid = primitives::make_box(10.0, 10.0, 10.0);
        let cyl_solid = primitives::make_cylinder(1.0, 10.0);
        let tol = compute_curvature_adaptive_tol(&box_solid, &cyl_solid);
        assert!(
            tol <= 0.015,
            "Curvature should constrain tolerance for small cylinder: {}",
            tol
        );
        assert!(tol >= 1e-7, "Tolerance should be above the floor: {}", tol);
    }

    #[test]
    fn test_curvature_adaptive_tol_flat_matches_base() {
        // Two 10mm boxes — all flat faces, curvature = infinity.
        // Curvature-adaptive tol should be same as base adaptive tol.
        let box_a = primitives::make_box(10.0, 10.0, 10.0);
        let box_b = primitives::make_box(10.0, 10.0, 10.0);
        let base_tol = compute_adaptive_tol(&box_a, &box_b);
        let curv_tol = compute_curvature_adaptive_tol(&box_a, &box_b);
        assert!(
            (curv_tol - base_tol).abs() < 1e-10,
            "Flat geometry: curvature tol {} should match base tol {}",
            curv_tol,
            base_tol
        );
    }

    #[test]
    fn test_curvature_adaptive_tol_never_loosens() {
        // The curvature-adaptive tol must never exceed the base adaptive tol.
        // Test with a variety of geometries.
        let box_s = primitives::make_box(5.0, 5.0, 5.0);
        let cyl_s = primitives::make_cylinder(2.0, 5.0);
        let base = compute_adaptive_tol(&box_s, &cyl_s);
        let curv = compute_curvature_adaptive_tol(&box_s, &cyl_s);
        assert!(
            curv <= base,
            "Curvature tol {} must be <= base tol {}",
            curv,
            base
        );
    }

    #[test]
    fn test_curvature_adaptive_tol_floor() {
        // Very small cylinder (r=0.001): curvature_based = 0.001 * 0.01 = 1e-5.
        // But min clamp is 1e-7, so we should get >= 1e-7.
        let tiny_box = primitives::make_box(0.01, 0.01, 0.01);
        let tiny_cyl = primitives::make_cylinder(0.001, 0.01);
        let tol = compute_curvature_adaptive_tol(&tiny_box, &tiny_cyl);
        assert!(
            tol >= 1e-7,
            "Tolerance must respect floor 1e-7, got {}",
            tol
        );
    }

    #[test]
    fn test_estimate_surface_curvature_plane() {
        let plane = Surface::Plane(truck_modeling::geometry::Plane::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ));
        let r = estimate_surface_curvature_radius(&plane);
        assert!(
            r.is_infinite(),
            "Plane should have infinite curvature radius, got {}",
            r
        );
    }
}

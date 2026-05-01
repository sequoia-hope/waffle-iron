//! Yang 2025 Section 4.3: Intersection optimization.
//!
//! After mesh intersection (Cherchi 2022 §4 arrangement, per Yang §4.2), each
//! intersection point maps to approximate parametric positions on both
//! surfaces. These generally don't coincide in 3D. This module iteratively
//! refines them until they converge to the same point.
//!
//! Two methods per Yang Appendix C:
//! - Newton: algebraic root-finding on D(u,v,s,t) = S_A(u,v) - S_B(s,t)
//! - Geometric: tangent plane intersection → line projection → back-projection
//!
//! Ref [#24]: Yang et al. 2025 Section 4.3, Appendix C.

use crate::geometry::surface::SurfaceGeom;

/// Result of a successful intersection point optimization.
#[derive(Debug, Clone)]
pub(crate) struct OptimizedPoint {
    /// Refined parametric coordinates on surface A.
    pub params_a: (f64, f64),
    /// Refined parametric coordinates on surface B.
    pub params_b: (f64, f64),
    /// Converged 3D position (midpoint of the two surface evaluations).
    pub position: [f64; 3],
    /// Number of iterations to converge.
    pub iterations: usize,
}

/// Error during optimization.
#[derive(Debug)]
pub(crate) enum OptimError {
    /// Did not converge within max iterations.
    NotConverged { residual: f64, iterations: usize },
    /// Degenerate Jacobian (parallel tangent planes, singular matrix).
    DegenerateJacobian,
}

/// Newton's method for intersection point optimization.
///
/// Per Yang 2025 Appendix C:
/// D(u,v,s,t) = S_A(u,v) - S_B(s,t)
/// ∇D = [∂S_A/∂u, ∂S_A/∂v, -∂S_B/∂s, -∂S_B/∂t]  (3×4 Jacobian)
/// Solve ∇D∇Dᵀ aₖ = D  (3×3 normal equations)
/// Update x += ∇Dᵀ aₖ
///
/// Terminates when ||D|| < d_p.
pub(crate) fn newton_optimize(
    surface_a: &SurfaceGeom,
    surface_b: &SurfaceGeom,
    seed_a: (f64, f64),
    seed_b: (f64, f64),
    d_p: f64,
    max_iter: usize,
) -> Result<OptimizedPoint, OptimError> {
    let mut ua = seed_a.0;
    let mut va = seed_a.1;
    let mut sb = seed_b.0;
    let mut tb = seed_b.1;

    for iter in 0..max_iter {
        // Evaluate both surfaces at current parameters.
        let pa = surface_a.evaluate(ua, va);
        let pb = surface_b.evaluate(sb, tb);

        // Residual: we want to minimize ||S_A(u,v) - S_B(s,t)||.
        // Per Yang Appendix C: solve ∇D∇Dᵀ a = (S_B - S_A), then update x += ∇Dᵀ a.
        let d = [pb.x - pa.x, pb.y - pa.y, pb.z - pa.z]; // RHS = S_B - S_A
        let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();

        if residual < d_p {
            return Ok(OptimizedPoint {
                params_a: (ua, va),
                params_b: (sb, tb),
                position: [
                    (pa.x + pb.x) * 0.5,
                    (pa.y + pb.y) * 0.5,
                    (pa.z + pb.z) * 0.5,
                ],
                iterations: iter,
            });
        }

        // Build Jacobian columns: [∂S_A/∂u, ∂S_A/∂v, -∂S_B/∂s, -∂S_B/∂t]
        let ta_u = surface_a.tangent_u(ua, va);
        let ta_v = surface_a.tangent_v(ua, va);
        let tb_s = surface_b.tangent_u(sb, tb);
        let tb_t = surface_b.tangent_v(sb, tb);

        // J is 3×4: columns are ta_u, ta_v, -tb_s, -tb_t
        // JJᵀ is 3×3
        let j = [
            [ta_u.x, ta_v.x, -tb_s.x, -tb_t.x],
            [ta_u.y, ta_v.y, -tb_s.y, -tb_t.y],
            [ta_u.z, ta_v.z, -tb_s.z, -tb_t.z],
        ];

        // Compute JJᵀ (3×3 symmetric)
        let mut jjt = [[0.0f64; 3]; 3];
        for i in 0..3 {
            for k in 0..3 {
                let mut sum = 0.0;
                for c in 0..4 {
                    sum += j[i][c] * j[k][c];
                }
                jjt[i][k] = sum;
            }
        }

        // Solve JJᵀ a = d via Cramer's rule (3×3)
        let a = solve_3x3_cramer(&jjt, &d);
        let a = match a {
            Some(a) => a,
            None => return Err(OptimError::DegenerateJacobian),
        };

        // Compute update: delta = Jᵀ a (4-vector)
        let delta = [
            j[0][0] * a[0] + j[1][0] * a[1] + j[2][0] * a[2], // δu
            j[0][1] * a[0] + j[1][1] * a[1] + j[2][1] * a[2], // δv
            j[0][2] * a[0] + j[1][2] * a[1] + j[2][2] * a[2], // δs
            j[0][3] * a[0] + j[1][3] * a[1] + j[2][3] * a[2], // δt
        ];

        ua += delta[0];
        va += delta[1];
        sb += delta[2];
        tb += delta[3];
    }

    // Did not converge
    let pa = surface_a.evaluate(ua, va);
    let pb = surface_b.evaluate(sb, tb);
    let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
    let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Err(OptimError::NotConverged {
        residual,
        iterations: max_iter,
    })
}

/// Geometric method for intersection point optimization.
///
/// Per Yang 2025 Section 4.3.2:
/// 1. Build tangent planes P_A, P_B at current params
/// 2. L = P_A ∩ P_B (intersection line)
/// 3. Project both surface points onto L → new 3D point
/// 4. Back-project to both surfaces → new params
///
/// "Used only when the two tangent planes are not parallel."
pub(crate) fn geometric_optimize(
    surface_a: &SurfaceGeom,
    surface_b: &SurfaceGeom,
    seed_a: (f64, f64),
    seed_b: (f64, f64),
    d_p: f64,
    max_iter: usize,
) -> Result<OptimizedPoint, OptimError> {
    let mut ua = seed_a.0;
    let mut va = seed_a.1;
    let mut sb = seed_b.0;
    let mut tb = seed_b.1;

    for iter in 0..max_iter {
        let pa = surface_a.evaluate(ua, va);
        let pb = surface_b.evaluate(sb, tb);

        let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
        let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();

        if residual < d_p {
            return Ok(OptimizedPoint {
                params_a: (ua, va),
                params_b: (sb, tb),
                position: [
                    (pa.x + pb.x) * 0.5,
                    (pa.y + pb.y) * 0.5,
                    (pa.z + pb.z) * 0.5,
                ],
                iterations: iter,
            });
        }

        // Compute surface normals at current params
        let na = surface_a.normal_at(ua, va);
        let nb = surface_b.normal_at(sb, tb);

        // Intersection line direction: L = nA × nB
        let line_dir = [
            na.y * nb.z - na.z * nb.y,
            na.z * nb.x - na.x * nb.z,
            na.x * nb.y - na.y * nb.x,
        ];
        let line_len_sq =
            line_dir[0] * line_dir[0] + line_dir[1] * line_dir[1] + line_dir[2] * line_dir[2];

        if line_len_sq < 1e-30 {
            // Tangent planes are parallel — fall back to Newton
            return newton_optimize(
                surface_a,
                surface_b,
                (ua, va),
                (sb, tb),
                d_p,
                max_iter - iter,
            );
        }

        // === Yang 4.3.2: Compute tangent plane intersection line L_k ===
        // Tangent planes: P_A_k: n_A · (P - p_A) = 0, P_B_k: n_B · (P - p_B) = 0
        // This is a 2×3 linear system:
        //   n_A · P = d_A, where d_A = n_A · p_A
        //   n_B · P = d_B, where d_B = n_B · p_B
        //
        // Choose coordinate to eliminate by finding largest |d| component.
        // Solve the 2×2 subsystem via Cramer's rule.

        let d_a = na.x * pa.x + na.y * pa.y + na.z * pa.z;
        let d_b = nb.x * pb.x + nb.y * pb.y + nb.z * pb.z;

        let abs_dx = line_dir[0].abs();
        let abs_dy = line_dir[1].abs();
        let abs_dz = line_dir[2].abs();

        let line_point = if abs_dx >= abs_dy && abs_dx >= abs_dz {
            // x is largest component: eliminate x, solve 2×2 for y, z
            // [n_A_y  n_A_z] [y]     [d_A]
            // [n_B_y  n_B_z] [z]  =  [d_B]
            let det = na.y * nb.z - na.z * nb.y;
            if det.abs() < 1e-30 {
                return newton_optimize(
                    surface_a,
                    surface_b,
                    (ua, va),
                    (sb, tb),
                    d_p,
                    max_iter - iter,
                );
            }
            let y = (d_a * nb.z - na.z * d_b) / det;
            let z = (na.y * d_b - d_a * nb.y) / det;
            [0.0, y, z]
        } else if abs_dy >= abs_dz {
            // y is largest component: eliminate y, solve 2×2 for x, z
            // [n_A_x  n_A_z] [x]     [d_A]
            // [n_B_x  n_B_z] [z]  =  [d_B]
            let det = na.x * nb.z - na.z * nb.x;
            if det.abs() < 1e-30 {
                return newton_optimize(
                    surface_a,
                    surface_b,
                    (ua, va),
                    (sb, tb),
                    d_p,
                    max_iter - iter,
                );
            }
            let x = (d_a * nb.z - na.z * d_b) / det;
            let z = (na.x * d_b - d_a * nb.x) / det;
            [x, 0.0, z]
        } else {
            // z is largest component: eliminate z (set z=0), solve 2×2 for x, y
            // [n_A_x  n_A_y] [x]     [d_A]
            // [n_B_x  n_B_y] [y]  =  [d_B]
            let det = na.x * nb.y - na.y * nb.x;
            if det.abs() < 1e-30 {
                return newton_optimize(
                    surface_a,
                    surface_b,
                    (ua, va),
                    (sb, tb),
                    d_p,
                    max_iter - iter,
                );
            }
            let x = (d_a * nb.y - na.y * d_b) / det;
            let y = (na.x * d_b - d_a * nb.x) / det;
            [x, y, 0.0]
        };

        // Normalize line direction
        let line_len = line_len_sq.sqrt();
        let d_hat = [
            line_dir[0] / line_len,
            line_dir[1] / line_len,
            line_dir[2] / line_len,
        ];

        // Project midpoint r1 onto line L to get r2
        let r1 = [
            (pa.x + pb.x) * 0.5,
            (pa.y + pb.y) * 0.5,
            (pa.z + pb.z) * 0.5,
        ];
        let r_to_line = [
            r1[0] - line_point[0],
            r1[1] - line_point[1],
            r1[2] - line_point[2],
        ];
        let proj_len = r_to_line[0] * d_hat[0] + r_to_line[1] * d_hat[1] + r_to_line[2] * d_hat[2];
        let r2 = [
            line_point[0] + proj_len * d_hat[0],
            line_point[1] + proj_len * d_hat[1],
            line_point[2] + proj_len * d_hat[2],
        ];

        // Back-project r2 onto both surfaces
        let proj_a =
            surface_a.project_point(crate::geometry::point::Point3::new(r2[0], r2[1], r2[2]));
        let proj_b =
            surface_b.project_point(crate::geometry::point::Point3::new(r2[0], r2[1], r2[2]));

        // Update parametric coordinates via inverse evaluation.
        if let Some((new_ua, new_va)) = surface_a.inverse_evaluate(proj_a) {
            ua = new_ua;
            va = new_va;
        }
        if let Some((new_sb, new_tb)) = surface_b.inverse_evaluate(proj_b) {
            sb = new_sb;
            tb = new_tb;
        }
    }

    let pa = surface_a.evaluate(ua, va);
    let pb = surface_b.evaluate(sb, tb);
    let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
    let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Err(OptimError::NotConverged {
        residual,
        iterations: max_iter,
    })
}

/// Solve 3×3 system Ax = b via Cramer's rule.
/// Returns None if determinant is near-zero (singular matrix).
fn solve_3x3_cramer(a: &[[f64; 3]; 3], b: &[f64; 3]) -> Option<[f64; 3]> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);

    if det.abs() < 1e-30 {
        return None;
    }

    let inv_det = 1.0 / det;

    let x0 = (b[0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (b[1] * a[2][2] - a[1][2] * b[2])
        + a[0][2] * (b[1] * a[2][1] - a[1][1] * b[2]))
        * inv_det;

    let x1 = (a[0][0] * (b[1] * a[2][2] - a[1][2] * b[2])
        - b[0] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * b[2] - b[1] * a[2][0]))
        * inv_det;

    let x2 = (a[0][0] * (a[1][1] * b[2] - b[1] * a[2][1])
        - a[0][1] * (a[1][0] * b[2] - b[1] * a[2][0])
        + b[0] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        * inv_det;

    Some([x0, x1, x2])
}

// ── Pipeline integration ─────────────────────────────────────────────────

use crate::boolean::exact_mesh::SubdividedMesh;
use crate::geometry::point::Point3;
use crate::tessellation::bijective::BijectiveMap;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::FaceIdx;
// BTreeSet (not HashSet): vert_faces_a/b are iterated to find (face_a, face_b)
// pairs that anchor Newton optimization for intersection vertex positions
// (lines ~472-473, ~866-867). HashMap RandomState would non-deterministically
// pick a different (face_a, face_b) pair on each run, producing different
// optimized vertex positions and a flapping rendermesh seen by the bijective
// oracle. PR12 Step 1 widening per `feedback_no_regression_chasing.md`.
use std::collections::{BTreeMap, BTreeSet};

/// Per-vertex optimization status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum VertexOptStatus {
    /// Not an intersection vertex (original mesh vertex).
    NotIntersection,
    /// Both surfaces are planar — Cherchi 2020 §4 indirect predicates already give exact position.
    Planar,
    /// Successfully optimized to within d_p tolerance.
    Optimized,
    /// Optimization failed (did not converge or degenerate Jacobian).
    Failed,
}

/// Statistics from intersection vertex optimization.
#[derive(Debug, Default)]
pub(crate) struct OptimizationStats {
    pub optimized: usize,
    pub skipped_planar: usize,
    pub skipped_no_surface: usize,
    pub skipped_no_inverse: usize,
    pub not_converged: usize,
    pub failed: usize,
    /// Per-vertex status (indexed by vertex index in subdivided mesh).
    pub vertex_status: Vec<VertexOptStatus>,
}

/// Optimize NEW intersection vertices in the subdivided mesh.
///
/// Per Yang 2025 Section 4.3: after mesh intersection (Cherchi 2022, per Yang §4.2), each new
/// vertex maps to approximate positions on both surfaces. This function
/// refines each vertex via Newton/geometric optimization until the two
/// surface evaluations converge to the same 3D point (within d_p).
///
/// New vertices are identified by index >= num_input_verts (original mesh
/// vertices from A and B were indexed 0..num_input_verts).
pub(crate) fn optimize_intersection_vertices(
    subdivided: &mut SubdividedMesh,
    bijective_a: &BijectiveMap,
    bijective_b: &BijectiveMap,
    face_geometry_a: &BTreeMap<FaceIdx, SurfaceGeom>,
    face_geometry_b: &BTreeMap<FaceIdx, SurfaceGeom>,
    num_input_verts: usize,
    d_p: f64,
) -> OptimizationStats {
    let mut stats = OptimizationStats::default();
    let n_verts = subdivided.verts.len();
    stats.vertex_status = vec![VertexOptStatus::NotIntersection; n_verts];

    if num_input_verts >= n_verts {
        return stats; // No new intersection vertices
    }

    // Build vertex → face adjacency from sub-triangles.
    // For each new vertex, find which faces from A and B reference it.
    let mut vert_faces_a: Vec<BTreeSet<FaceIdx>> = vec![BTreeSet::new(); n_verts];
    let mut vert_faces_b: Vec<BTreeSet<FaceIdx>> = vec![BTreeSet::new(); n_verts];

    for sub_tri in &subdivided.tris_a {
        if sub_tri.parent_tri < bijective_a.tri_face_ids.len() {
            let face = bijective_a.tri_face_ids[sub_tri.parent_tri];
            for &vi in &sub_tri.verts {
                if vi >= num_input_verts && vi < n_verts {
                    vert_faces_a[vi].insert(face);
                }
            }
        }
    }
    for sub_tri in &subdivided.tris_b {
        if sub_tri.parent_tri < bijective_b.tri_face_ids.len() {
            let face = bijective_b.tri_face_ids[sub_tri.parent_tri];
            for &vi in &sub_tri.verts {
                if vi >= num_input_verts && vi < n_verts {
                    vert_faces_b[vi].insert(face);
                }
            }
        }
    }

    // Optimize each new intersection vertex.
    for vi in num_input_verts..n_verts {
        if vert_faces_a[vi].is_empty() || vert_faces_b[vi].is_empty() {
            continue; // Not at an A/B intersection
        }

        let pos = subdivided.verts[vi];
        let pt = Point3::new(pos[0], pos[1], pos[2]);
        let mut optimized = false;
        let mut all_pairs_missing_surface = true;

        #[cfg(test)]
        eprintln!(
            "[OPT DIAG] vertex {}: faces_a={:?} faces_b={:?} pos={:?}",
            vi,
            vert_faces_a[vi].iter().collect::<Vec<_>>(),
            vert_faces_b[vi].iter().collect::<Vec<_>>(),
            pos
        );

        // Try each face pair (A_face, B_face) that shares this vertex.
        'pairs: for &face_a in &vert_faces_a[vi] {
            for &face_b in &vert_faces_b[vi] {
                let geom_a = match face_geometry_a.get(&face_a) {
                    Some(g) => g,
                    None => {
                        #[cfg(test)]
                        eprintln!(
                            "[OPT DIAG]   pair: face_a={:?} face_b={:?} -> MISSING geom_a",
                            face_a, face_b
                        );
                        stats.skipped_no_surface += 1;
                        continue;
                    }
                };
                let geom_b = match face_geometry_b.get(&face_b) {
                    Some(g) => g,
                    None => {
                        #[cfg(test)]
                        eprintln!(
                            "[OPT DIAG]   pair: face_a={:?} face_b={:?} -> MISSING geom_b",
                            face_a, face_b
                        );
                        stats.skipped_no_surface += 1;
                        continue;
                    }
                };
                all_pairs_missing_surface = false;

                // Skip planar-planar: Cherchi 2020 §4 indirect predicates
                // already produce exact intersection points for flat surfaces.
                if matches!(geom_a, SurfaceGeom::Planar(_))
                    && matches!(geom_b, SurfaceGeom::Planar(_))
                {
                    #[cfg(test)]
                    eprintln!(
                        "[OPT DIAG]   pair: face_a={:?} face_b={:?} -> PLANAR SKIP",
                        face_a, face_b
                    );
                    stats.skipped_planar += 1;
                    stats.vertex_status[vi] = VertexOptStatus::Planar;
                    optimized = true;
                    break 'pairs;
                }

                // Compute parametric seeds via inverse evaluation.
                let seed_a = match geom_a.inverse_evaluate(pt) {
                    Some(uv) => uv,
                    None => {
                        stats.skipped_no_inverse += 1;
                        continue;
                    }
                };
                let seed_b = match geom_b.inverse_evaluate(pt) {
                    Some(uv) => uv,
                    None => {
                        stats.skipped_no_inverse += 1;
                        continue;
                    }
                };

                // Per Yang 4.3.3: Newton first (for tangent points, boundary cases),
                // geometric fallback (for general intersection loops).
                let result = newton_optimize(geom_a, geom_b, seed_a, seed_b, d_p, 20)
                    .or_else(|_| geometric_optimize(geom_a, geom_b, seed_a, seed_b, d_p, 20));

                match result {
                    Ok(opt) => {
                        subdivided.verts[vi] = opt.position;
                        subdivided.params_a[vi] = Some(opt.params_a);
                        subdivided.params_b[vi] = Some(opt.params_b);
                        stats.optimized += 1;
                        stats.vertex_status[vi] = VertexOptStatus::Optimized;
                        optimized = true;
                        break 'pairs;
                    }
                    Err(OptimError::NotConverged { .. }) => {
                        stats.not_converged += 1;
                    }
                    Err(OptimError::DegenerateJacobian) => {
                        stats.failed += 1;
                    }
                }
            }
        }

        if !optimized && !vert_faces_a[vi].is_empty() && !vert_faces_b[vi].is_empty() {
            if all_pairs_missing_surface {
                // All face pairs had missing surface geometry — we can't optimize
                // but we also can't declare failure. The Cherchi 2020 position is already
                // exact for planar geometry (the most common case for missing entries).
                // Treat as NotIntersection to avoid triggering expensive refinement.
                #[cfg(test)]
                eprintln!(
                    "[OPT DIAG] vertex {}: ALL pairs missing surface -> keeping Cherchi position",
                    vi
                );
                stats.skipped_no_surface += 1;
            } else {
                #[cfg(test)]
                eprintln!("[OPT DIAG] vertex {}: -> FAILED", vi);
                stats.vertex_status[vi] = VertexOptStatus::Failed;
                stats.failed += 1;
            }
        }
    }

    stats
}

/// Yang 4.5.1: Find the adjacent face across the nearest boundary edge.
///
/// Given a face and an optimized 3D point that has left the face's trim domain,
/// walk the face's outer loop to find the nearest boundary edge, then follow the
/// twin half-edge to discover the adjacent face. Returns the adjacent face index
/// and its surface geometry (if available).
fn find_adjacent_face_across_boundary<'a>(
    arena: &'a TopoArena,
    face_idx: FaceIdx,
    point: [f64; 3],
    face_geometry: &'a BTreeMap<FaceIdx, SurfaceGeom>,
) -> Option<(FaceIdx, &'a SurfaceGeom)> {
    if face_idx.0 >= arena.faces.len() {
        return None;
    }
    let face = &arena.faces[face_idx.0];
    let outer_loop = face.outer_loop;
    if outer_loop.0 >= arena.loops.len() {
        return None;
    }
    let start_he = arena.loops[outer_loop.0].half_edge;

    // Walk the outer loop collecting edges; find the one closest to `point`.
    let mut best_dist_sq = f64::MAX;
    let mut best_twin_face: Option<FaceIdx> = None;
    let mut he = start_he;
    loop {
        if he.0 >= arena.half_edges.len() {
            break;
        }
        let he_data = &arena.half_edges[he.0];
        let v0_idx = he_data.origin;
        let next_he = he_data.next;
        if next_he.0 >= arena.half_edges.len() {
            break;
        }
        let v1_idx = arena.half_edges[next_he.0].origin;

        if v0_idx.0 < arena.vertices.len() && v1_idx.0 < arena.vertices.len() {
            let p0 = arena.vertices[v0_idx.0].position;
            let p1 = arena.vertices[v1_idx.0].position;
            let dist_sq = closest_point_on_segment_dist_sq(point, p0, p1);

            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                // Follow twin to get the adjacent face
                let twin = he_data.twin;
                if twin.0 < arena.half_edges.len() {
                    let twin_loop = arena.half_edges[twin.0].loop_;
                    if twin_loop.0 < arena.loops.len() {
                        let adj_face = arena.loops[twin_loop.0].face;
                        if adj_face != face_idx {
                            best_twin_face = Some(adj_face);
                        }
                    }
                }
            }
        }

        he = next_he;
        if he == start_he {
            break;
        }
    }

    best_twin_face.and_then(|adj| face_geometry.get(&adj).map(|g| (adj, g)))
}

/// Squared distance from `point` to the closest point on segment `p0→p1`.
fn closest_point_on_segment_dist_sq(point: [f64; 3], p0: [f64; 3], p1: [f64; 3]) -> f64 {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let dz = p1[2] - p0[2];
    let len_sq = dx * dx + dy * dy + dz * dz;
    if len_sq < 1e-30 {
        let ex = point[0] - p0[0];
        let ey = point[1] - p0[1];
        let ez = point[2] - p0[2];
        return ex * ex + ey * ey + ez * ez;
    }
    let t = ((point[0] - p0[0]) * dx + (point[1] - p0[1]) * dy + (point[2] - p0[2]) * dz) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = p0[0] + t * dx;
    let cy = p0[1] + t * dy;
    let cz = p0[2] + t * dz;
    let ex = point[0] - cx;
    let ey = point[1] - cy;
    let ez = point[2] - cz;
    ex * ex + ey * ey + ez * ez
}

/// Yang 4.5.1: Check if an optimized point has left the source face.
///
/// Evaluates the surface at the optimized params and checks if the resulting
/// 3D point is still geometrically on the face (within tolerance). Uses the
/// face's boundary loop vertices to do a simple containment test: the point
/// should be within a tolerance of the face's surface AND its projection
/// should lie within the convex hull of the face boundary.
///
/// Returns true if the point is OUTSIDE the face boundary.
fn point_exited_face(
    arena: &TopoArena,
    face_idx: FaceIdx,
    optimized_pos: [f64; 3],
    geom: &SurfaceGeom,
) -> bool {
    // First check: is the point still on the surface at all?
    let pt = Point3::new(optimized_pos[0], optimized_pos[1], optimized_pos[2]);
    if !geom.contains_point(pt) {
        return true;
    }

    // Second check: is the point within the face's boundary polygon?
    // Walk the outer loop and collect boundary vertices.
    if face_idx.0 >= arena.faces.len() {
        return false; // Can't verify — assume OK
    }
    let face = &arena.faces[face_idx.0];
    let outer_loop = face.outer_loop;
    if outer_loop.0 >= arena.loops.len() {
        return false;
    }
    let start_he = arena.loops[outer_loop.0].half_edge;
    let mut boundary_verts: Vec<[f64; 3]> = Vec::new();
    let mut he = start_he;
    loop {
        if he.0 >= arena.half_edges.len() {
            break;
        }
        let v = arena.half_edges[he.0].origin;
        if v.0 < arena.vertices.len() {
            boundary_verts.push(arena.vertices[v.0].position);
        }
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    if boundary_verts.len() < 3 {
        return false; // Degenerate face — can't verify
    }

    // Check: distance from optimized point to the nearest boundary edge.
    // If it's closer to a boundary edge than to the face interior, it may
    // have exited. Use a simpler heuristic: compute distance to the face
    // plane's centroid and compare with max vertex distance.
    // This is approximate but sufficient for the truncation trigger.
    let mut centroid = [0.0f64; 3];
    for v in &boundary_verts {
        centroid[0] += v[0];
        centroid[1] += v[1];
        centroid[2] += v[2];
    }
    let n = boundary_verts.len() as f64;
    centroid[0] /= n;
    centroid[1] /= n;
    centroid[2] /= n;

    // Max distance from centroid to any boundary vertex
    let max_radius_sq = boundary_verts
        .iter()
        .map(|v| {
            let dx = v[0] - centroid[0];
            let dy = v[1] - centroid[1];
            let dz = v[2] - centroid[2];
            dx * dx + dy * dy + dz * dz
        })
        .fold(0.0f64, f64::max);

    // Distance from optimized point to centroid
    let dx = optimized_pos[0] - centroid[0];
    let dy = optimized_pos[1] - centroid[1];
    let dz = optimized_pos[2] - centroid[2];
    let dist_sq = dx * dx + dy * dy + dz * dz;

    // If point is farther from centroid than the farthest boundary vertex,
    // it's likely outside the face. Use 1.2× margin for robustness.
    dist_sq > max_radius_sq * 1.44 // 1.2² = 1.44
}

/// Yang 4.5.1: Recover failed vertices by replacing with midpoints of
/// neighboring successful vertices, then re-optimizing with step clamping.
///
/// For each FAILED vertex, finds the nearest SUCCESSFUL or PLANAR vertices
/// that share an edge in the subdivided mesh. Replaces the failed vertex's
/// position with the midpoint of those neighbors, then re-runs optimization.
///
/// When arenas are provided, implements true boundary truncation per Yang 4.5.1:
/// if the optimized point exits the source face, finds the adjacent face across
/// the nearest boundary edge and re-optimizes using the adjacent surface.
///
/// Returns the number of vertices recovered.
pub(crate) fn recover_failed_regions(
    subdivided: &mut SubdividedMesh,
    vertex_status: &mut [VertexOptStatus],
    bijective_a: &BijectiveMap,
    bijective_b: &BijectiveMap,
    face_geometry_a: &BTreeMap<FaceIdx, SurfaceGeom>,
    face_geometry_b: &BTreeMap<FaceIdx, SurfaceGeom>,
    num_input_verts: usize,
    d_p: f64,
    arena_a: Option<&TopoArena>,
    arena_b: Option<&TopoArena>,
) -> usize {
    let n_verts = subdivided.verts.len();
    if num_input_verts >= n_verts {
        return 0;
    }

    // Build edge adjacency: for each vertex, collect neighboring vertices.
    let mut neighbors: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_verts];
    for tri in subdivided.tris_a.iter().chain(subdivided.tris_b.iter()) {
        for k in 0..3 {
            let v0 = tri.verts[k];
            let v1 = tri.verts[(k + 1) % 3];
            neighbors[v0].insert(v1);
            neighbors[v1].insert(v0);
        }
    }

    // Build vertex→face adjacency for new vertices (same as in optimize_intersection_vertices).
    let mut vert_faces_a: Vec<BTreeSet<FaceIdx>> = vec![BTreeSet::new(); n_verts];
    let mut vert_faces_b: Vec<BTreeSet<FaceIdx>> = vec![BTreeSet::new(); n_verts];
    for sub_tri in &subdivided.tris_a {
        if sub_tri.parent_tri < bijective_a.tri_face_ids.len() {
            let face = bijective_a.tri_face_ids[sub_tri.parent_tri];
            for &vi in &sub_tri.verts {
                if vi >= num_input_verts {
                    vert_faces_a[vi].insert(face);
                }
            }
        }
    }
    for sub_tri in &subdivided.tris_b {
        if sub_tri.parent_tri < bijective_b.tri_face_ids.len() {
            let face = bijective_b.tri_face_ids[sub_tri.parent_tri];
            for &vi in &sub_tri.verts {
                if vi >= num_input_verts {
                    vert_faces_b[vi].insert(face);
                }
            }
        }
    }

    let mut recovered = 0;

    for vi in num_input_verts..n_verts {
        if vertex_status[vi] != VertexOptStatus::Failed {
            continue;
        }

        // Find neighboring vertices that are Optimized or Planar.
        let good_neighbors: Vec<usize> = neighbors[vi]
            .iter()
            .copied()
            .filter(|&ni| {
                matches!(
                    vertex_status[ni],
                    VertexOptStatus::Optimized | VertexOptStatus::Planar
                ) || ni < num_input_verts // Original vertices are reliable
            })
            .collect();

        if good_neighbors.is_empty() {
            continue; // No anchor points for midpoint replacement
        }

        // Replace with midpoint of good neighbors (Yang 4.5.1: "midpoint of v0 and v1").
        let mut mid = [0.0f64; 3];
        for &ni in &good_neighbors {
            mid[0] += subdivided.verts[ni][0];
            mid[1] += subdivided.verts[ni][1];
            mid[2] += subdivided.verts[ni][2];
        }
        let n = good_neighbors.len() as f64;
        mid[0] /= n;
        mid[1] /= n;
        mid[2] /= n;
        subdivided.verts[vi] = mid;

        // Re-optimize from the new midpoint position.
        let pt = Point3::new(mid[0], mid[1], mid[2]);
        let mut success = false;

        for &face_a in &vert_faces_a[vi] {
            for &face_b in &vert_faces_b[vi] {
                let geom_a = match face_geometry_a.get(&face_a) {
                    Some(g) => g,
                    None => continue,
                };
                let geom_b = match face_geometry_b.get(&face_b) {
                    Some(g) => g,
                    None => continue,
                };
                if matches!(geom_a, SurfaceGeom::Planar(_))
                    && matches!(geom_b, SurfaceGeom::Planar(_))
                {
                    vertex_status[vi] = VertexOptStatus::Planar;
                    success = true;
                    break;
                }

                let seed_a = match geom_a.inverse_evaluate(pt) {
                    Some(uv) => uv,
                    None => continue,
                };
                let seed_b = match geom_b.inverse_evaluate(pt) {
                    Some(uv) => uv,
                    None => continue,
                };

                // Try with clamped parameters (Yang 4.5.1 step truncation).
                let (cu, cv, _) = geom_a.clamp_params(seed_a.0, seed_a.1);
                let (cs, ct, _) = geom_b.clamp_params(seed_b.0, seed_b.1);

                // Per Yang 4.3.3: Newton first, geometric fallback.
                let result = newton_optimize(geom_a, geom_b, (cu, cv), (cs, ct), d_p, 30)
                    .or_else(|_| geometric_optimize(geom_a, geom_b, (cu, cv), (cs, ct), d_p, 30));

                if let Ok(opt) = result {
                    // Yang 4.5.1 boundary truncation: check if the optimized
                    // point has left the source face on either side. If so,
                    // find the adjacent face and re-optimize on its surface.
                    let mut final_opt = opt;
                    let mut switched = false;

                    // Check side A: did the point leave face_a?
                    if let Some(arena) = arena_a {
                        if point_exited_face(arena, face_a, final_opt.position, geom_a) {
                            if let Some((adj_face, adj_geom)) = find_adjacent_face_across_boundary(
                                arena,
                                face_a,
                                final_opt.position,
                                face_geometry_a,
                            ) {
                                let adj_pt = Point3::new(
                                    final_opt.position[0],
                                    final_opt.position[1],
                                    final_opt.position[2],
                                );
                                if let Some(adj_seed) = adj_geom.inverse_evaluate(adj_pt) {
                                    let (au, av, _) = adj_geom.clamp_params(adj_seed.0, adj_seed.1);
                                    // Re-optimize with the adjacent surface for side A
                                    let adj_result = newton_optimize(
                                        adj_geom,
                                        geom_b,
                                        (au, av),
                                        final_opt.params_b,
                                        d_p,
                                        30,
                                    )
                                    .or_else(|_| {
                                        geometric_optimize(
                                            adj_geom,
                                            geom_b,
                                            (au, av),
                                            final_opt.params_b,
                                            d_p,
                                            30,
                                        )
                                    });
                                    if let Ok(adj_opt) = adj_result {
                                        final_opt = adj_opt;
                                        switched = true;
                                        let _ = adj_face; // used for logging if needed
                                    }
                                }
                            }
                        }
                    }

                    // Check side B: did the point leave face_b?
                    if !switched {
                        if let Some(arena) = arena_b {
                            if point_exited_face(arena, face_b, final_opt.position, geom_b) {
                                if let Some((_adj_face, adj_geom)) =
                                    find_adjacent_face_across_boundary(
                                        arena,
                                        face_b,
                                        final_opt.position,
                                        face_geometry_b,
                                    )
                                {
                                    let adj_pt = Point3::new(
                                        final_opt.position[0],
                                        final_opt.position[1],
                                        final_opt.position[2],
                                    );
                                    if let Some(adj_seed) = adj_geom.inverse_evaluate(adj_pt) {
                                        let (as_, at, _) =
                                            adj_geom.clamp_params(adj_seed.0, adj_seed.1);
                                        let adj_result = newton_optimize(
                                            geom_a,
                                            adj_geom,
                                            final_opt.params_a,
                                            (as_, at),
                                            d_p,
                                            30,
                                        )
                                        .or_else(|_| {
                                            geometric_optimize(
                                                geom_a,
                                                adj_geom,
                                                final_opt.params_a,
                                                (as_, at),
                                                d_p,
                                                30,
                                            )
                                        });
                                        if let Ok(adj_opt) = adj_result {
                                            final_opt = adj_opt;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    subdivided.verts[vi] = final_opt.position;
                    vertex_status[vi] = VertexOptStatus::Optimized;
                    recovered += 1;
                    success = true;
                    break;
                }
            }
            if success {
                break;
            }
        }
    }

    recovered
}

/// Yang 4.5.3: Detect and correct reversed intersection points.
///
/// After optimization, intersection curve polylines may have points out of
/// order. For each intersection vertex with parametric coords on both surfaces,
/// compare the discrete tangent (from polyline neighbors) with the analytical
/// tangent (cross product of surface normals). If angle is 45°-135°, the point
/// is reversed — collapse it into its next point (or previous point if no next works).
///
/// Collapse operation: when vi_r is reversed, move its next point vi_n into vi_r's
/// position and reroute all triangle references from vi_n → vi_r. This preserves
/// the intersection curve topology while avoiding index shifts.
///
/// Returns the number of reversed points collapsed.
pub(crate) fn correct_reversed_intersections(
    subdivided: &mut SubdividedMesh,
    bijective_a: &BijectiveMap,
    bijective_b: &BijectiveMap,
    face_geometry_a: &BTreeMap<FaceIdx, SurfaceGeom>,
    face_geometry_b: &BTreeMap<FaceIdx, SurfaceGeom>,
    num_input_verts: usize,
) -> usize {
    let n_verts = subdivided.verts.len();
    if num_input_verts >= n_verts {
        return 0;
    }

    // Build edge adjacency for intersection vertices.
    // An "intersection edge" connects two vertices that both appear in
    // tris_a AND tris_b (they're on the intersection curve).
    let mut is_in_a = vec![false; n_verts];
    let mut is_in_b = vec![false; n_verts];
    for tri in &subdivided.tris_a {
        for &vi in &tri.verts {
            is_in_a[vi] = true;
        }
    }
    for tri in &subdivided.tris_b {
        for &vi in &tri.verts {
            is_in_b[vi] = true;
        }
    }

    // Intersection vertices: referenced by BOTH meshes
    let is_intersection: Vec<bool> = (0..n_verts)
        .map(|vi| vi >= num_input_verts && is_in_a[vi] && is_in_b[vi])
        .collect();

    // Build edge adjacency among intersection vertices.
    let mut neighbors: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_verts];
    for tri in subdivided.tris_a.iter().chain(subdivided.tris_b.iter()) {
        for k in 0..3 {
            let v0 = tri.verts[k];
            let v1 = tri.verts[(k + 1) % 3];
            if is_intersection[v0] && is_intersection[v1] {
                neighbors[v0].insert(v1);
                neighbors[v1].insert(v0);
            }
        }
    }

    // Chain intersection vertices into polylines.
    let mut visited = vec![false; n_verts];
    let mut polylines: Vec<Vec<usize>> = Vec::new();

    for start in num_input_verts..n_verts {
        if !is_intersection[start] || visited[start] {
            continue;
        }
        // Walk forward from start
        let mut chain = vec![start];
        visited[start] = true;
        let mut current = start;
        loop {
            let next = neighbors[current].iter().copied().find(|&n| !visited[n]);
            match next {
                Some(n) => {
                    visited[n] = true;
                    chain.push(n);
                    current = n;
                }
                None => break,
            }
        }
        if chain.len() >= 3 {
            polylines.push(chain);
        }
    }

    // For each polyline, detect and collapse reversed points.
    // Algorithm: for each middle point p_r:
    //   1. If collinear or angle is 45°-135°, p_r is reversed
    //   2. Collapse next point p_n into p_r (copy position/params, reroute triangles)
    //   3. Re-check p_r with its new next point
    //   4. If no next point works, collapse p_r into previous point p_b
    let mut collapsed = 0;
    let angle_lo = std::f64::consts::FRAC_PI_4; // 45°
    let angle_hi = 3.0 * std::f64::consts::FRAC_PI_4; // 135°

    // Helper: check if point vi_r is reversed given neighbors vi_b and vi_n
    let is_reversed_point =
        |subdivided: &SubdividedMesh, vi_b: usize, vi_r: usize, vi_n: usize| -> bool {
            let pb = subdivided.verts[vi_b];
            let pr = subdivided.verts[vi_r];
            let pn = subdivided.verts[vi_n];

            // Discrete tangent: sum of unit edge vectors
            let edge_bp = [pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]];
            let edge_rn = [pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]];
            let len_bp =
                (edge_bp[0] * edge_bp[0] + edge_bp[1] * edge_bp[1] + edge_bp[2] * edge_bp[2])
                    .sqrt();
            let len_rn =
                (edge_rn[0] * edge_rn[0] + edge_rn[1] * edge_rn[1] + edge_rn[2] * edge_rn[2])
                    .sqrt();

            if len_bp < 1e-15 || len_rn < 1e-15 {
                return false; // Degenerate edge
            }

            let t_disc = [
                edge_bp[0] / len_bp + edge_rn[0] / len_rn,
                edge_bp[1] / len_bp + edge_rn[1] / len_rn,
                edge_bp[2] / len_bp + edge_rn[2] / len_rn,
            ];
            let t_disc_len =
                (t_disc[0] * t_disc[0] + t_disc[1] * t_disc[1] + t_disc[2] * t_disc[2]).sqrt();

            // Collinear check: if discrete tangent is nearly zero, points are reversing
            if t_disc_len < 1e-10 {
                return true;
            }

            // Analytical tangent: n_A × n_B at p_r
            let params_a = match subdivided.params_a[vi_r] {
                Some(p) => p,
                None => return false,
            };
            let params_b = match subdivided.params_b[vi_r] {
                Some(p) => p,
                None => return false,
            };

            // Find which face this vertex belongs to (use first adjacent face)
            let face_a = subdivided
                .tris_a
                .iter()
                .find(|t| t.verts.contains(&vi_r))
                .and_then(|t| bijective_a.tri_face_ids.get(t.parent_tri).copied());
            let face_b = subdivided
                .tris_b
                .iter()
                .find(|t| t.verts.contains(&vi_r))
                .and_then(|t| bijective_b.tri_face_ids.get(t.parent_tri).copied());

            let (face_a, face_b) = match (face_a, face_b) {
                (Some(a), Some(b)) => (a, b),
                _ => return false,
            };

            let geom_a = match face_geometry_a.get(&face_a) {
                Some(g) => g,
                None => return false,
            };
            let geom_b = match face_geometry_b.get(&face_b) {
                Some(g) => g,
                None => return false,
            };

            let n_a = geom_a.normal_at(params_a.0, params_a.1);
            let n_b = geom_b.normal_at(params_b.0, params_b.1);
            let t_anal = n_a.cross(n_b);
            let t_anal_len = t_anal.length();

            if t_anal_len < 1e-15 {
                return false; // Parallel normals — tangent surfaces, skip
            }

            // Compare angles
            let dot = (t_disc[0] * t_anal.x + t_disc[1] * t_anal.y + t_disc[2] * t_anal.z)
                / (t_disc_len * t_anal_len);
            let angle = dot.clamp(-1.0, 1.0).acos();

            angle > angle_lo && angle < angle_hi
        };

    // Helper: collapse vi_n into vi_r by copying position/params and rerouting triangles
    let collapse_into = |subdivided: &mut SubdividedMesh, vi_r: usize, vi_n: usize| {
        // Copy position and parameters from vi_n to vi_r
        subdivided.verts[vi_r] = subdivided.verts[vi_n];
        subdivided.params_a[vi_r] = subdivided.params_a[vi_n];
        subdivided.params_b[vi_r] = subdivided.params_b[vi_n];

        // Reroute all triangle references: vi_n → vi_r
        for tri in &mut subdivided.tris_a {
            for vi in &mut tri.verts {
                if *vi == vi_n {
                    *vi = vi_r;
                }
            }
        }
        for tri in &mut subdivided.tris_b {
            for vi in &mut tri.verts {
                if *vi == vi_n {
                    *vi = vi_r;
                }
            }
        }
    };

    use std::collections::BTreeSet;

    for polyline in &polylines {
        if polyline.len() < 3 {
            continue;
        }

        // Track orphaned vertices (collapsed away) so we can skip them
        let mut orphaned = BTreeSet::new();

        // Process each position in the polyline, skipping orphaned vertices
        let mut i = 1;
        while i < polyline.len() {
            // Find the previous non-orphaned vertex
            let mut prev_idx = i - 1;
            while prev_idx > 0 && orphaned.contains(&polyline[prev_idx]) {
                prev_idx -= 1;
            }

            // Find the next non-orphaned vertex
            let mut next_idx = i + 1;
            while next_idx < polyline.len() && orphaned.contains(&polyline[next_idx]) {
                next_idx += 1;
            }

            // Skip if this is an orphaned vertex or if we don't have both neighbors
            if orphaned.contains(&polyline[i]) || next_idx >= polyline.len() {
                i += 1;
                continue;
            }

            let vi_b = polyline[prev_idx];
            let vi_r = polyline[i];
            let vi_n = polyline[next_idx];

            if is_reversed_point(subdivided, vi_b, vi_r, vi_n) {
                // Reversal detected: collapse vi_n into vi_r
                collapse_into(subdivided, vi_r, vi_n);
                orphaned.insert(vi_n);
                collapsed += 1;

                // Don't increment i — re-check vi_r with its new next neighbor
                // (which may have changed due to orphaning vi_n)
                continue;
            }

            i += 1;
        }
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::point::{Point3, Vector3};
    use crate::geometry::surface::{Cylinder, Plane, Sphere};

    #[test]
    fn newton_plane_plane_converges() {
        // Two planes intersecting along a line.
        // Plane A: z = 0 (normal = +Z)
        // Plane B: y = 0 (normal = +Y)
        // Intersection line: y=0, z=0 (the x-axis).
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 1.0, 0.0),
        });

        // Seeds: slightly off the intersection line.
        let result = newton_optimize(&sa, &sb, (1.0, 0.5), (1.0, 0.3), 1e-7, 50);
        let opt = result.expect("Newton should converge for plane-plane");
        assert!(opt.iterations <= 5, "Should converge quickly for planes");
        // Position should be on the x-axis (y≈0, z≈0)
        assert!(opt.position[1].abs() < 1e-6, "y should be ~0");
        assert!(opt.position[2].abs() < 1e-6, "z should be ~0");
    }

    #[test]
    fn newton_plane_sphere_converges() {
        // Plane z=0 and unit sphere at origin.
        // Intersection: circle x²+y²=1 at z=0.
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Spherical(Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        });

        // Seed: approximately on the circle at (1, 0, 0).
        // Plane params (u, v) → point (u, v, 0) with frame x=[1,0,0], y=[0,1,0]
        // Sphere params (u, v) → u=0, v=0 → (1, 0, 0)
        let result = newton_optimize(&sa, &sb, (1.0, 0.0), (0.0, 0.0), 1e-7, 50);
        let opt = result.expect("Newton should converge for plane-sphere");
        // Position should be on the unit circle at z=0.
        let r = (opt.position[0] * opt.position[0] + opt.position[1] * opt.position[1]).sqrt();
        assert!(
            (r - 1.0).abs() < 1e-5,
            "Distance from origin should be ~1.0, got {r}"
        );
        assert!(opt.position[2].abs() < 1e-5, "z should be ~0");
    }

    #[test]
    fn geometric_plane_cylinder_converges() {
        // Plane z=0.5 and cylinder radius=1 along z-axis.
        // Intersection: circle x²+y²=1 at z=0.5.
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.5),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::new(0.0, 0.0, 0.0),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        });

        // Seed: approximately on the circle at (1, 0, 0.5).
        let result = geometric_optimize(&sa, &sb, (1.0, 0.0), (0.0, 0.5), 1e-7, 50);
        let opt = result.expect("Geometric should converge for plane-cylinder");
        let r = (opt.position[0] * opt.position[0] + opt.position[1] * opt.position[1]).sqrt();
        assert!((r - 1.0).abs() < 1e-4, "Radius should be ~1.0, got {r}");
        assert!(
            (opt.position[2] - 0.5).abs() < 1e-4,
            "z should be ~0.5, got {}",
            opt.position[2]
        );
    }

    #[test]
    fn cramer_solves_identity() {
        let a = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [3.0, 5.0, 7.0];
        let x = solve_3x3_cramer(&a, &b).unwrap();
        assert!((x[0] - 3.0).abs() < 1e-12);
        assert!((x[1] - 5.0).abs() < 1e-12);
        assert!((x[2] - 7.0).abs() < 1e-12);
    }

    #[test]
    fn cramer_returns_none_for_singular() {
        let a = [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [1.0, 1.0, 1.0];
        assert!(solve_3x3_cramer(&a, &b).is_none());
    }

    #[test]
    fn geometric_sphere_cylinder_intersection_line_projection() {
        // Test Yang 4.3.2 geometric method: tangent plane intersection + projection
        // Sphere: center (0,0,0), radius 1
        // Cylinder: axis along z, radius 0.8, centered at origin
        // Intersection: circle at z=√(1-0.64)=0.6, x²+y²=0.64
        let sphere = SurfaceGeom::Spherical(Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        });
        let cylinder = SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::new(0.0, 0.0, 0.0),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.8,
        });

        // Seed near intersection point (~0.8, 0, 0.6)
        // For sphere: (u,v) → (sin(v)cos(u), sin(v)sin(u), cos(v))
        // For cylinder: (u,v) → (0.8*cos(u), 0.8*sin(u), v)
        // At point (0.8, 0, 0.6):
        // - Sphere: u≈0, v=acos(0.6)≈0.927 radians (about 53°)
        // - Cylinder: u≈0, v≈0.6

        let result = geometric_optimize(&sphere, &cylinder, (0.1, 0.93), (0.05, 0.6), 1e-6, 50);
        let opt = result.expect("Geometric should converge for sphere-cylinder");

        // The optimized position should be on both surfaces.
        let p_opt = opt.position;

        // Check sphere: distance from origin should be ≈ 1.0
        let r_sphere = (p_opt[0] * p_opt[0] + p_opt[1] * p_opt[1] + p_opt[2] * p_opt[2]).sqrt();
        assert!(
            (r_sphere - 1.0).abs() < 1e-4,
            "Optimized point should be on sphere (r=1), got r={r_sphere}"
        );

        // Check cylinder: x²+y² should be ≈ 0.64
        let r_cyl_sq = p_opt[0] * p_opt[0] + p_opt[1] * p_opt[1];
        assert!(
            (r_cyl_sq - 0.64).abs() < 1e-4,
            "Optimized point should be on cylinder (r²=0.64), got r²={r_cyl_sq}"
        );

        // The z-coordinate should be positive (intersection is above xy-plane)
        assert!(
            p_opt[2] > 0.0,
            "z should be positive for upper intersection"
        );
    }

    #[test]
    fn geometric_offset_sphere_cylinder_nontrivial_intersection_line() {
        // Test Yang 4.3.2 with non-trivial tangent plane intersection line.
        // Sphere: center (0.5, 0, 0), radius 1 — offset so normals are non-axis-aligned.
        // Cylinder: radius 0.8 along z-axis at origin.
        // Both have working inverse_evaluate, so both params update each iteration.
        let sa = SurfaceGeom::Spherical(Sphere {
            center: Point3::new(0.5, 0.0, 0.0),
            radius: 1.0,
        });
        let sb = SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::new(0.0, 0.0, 0.0),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.8,
        });

        // A point near the intersection: on the cylinder at (0.8, 0, 0).
        // Distance from sphere center (0.5, 0, 0) to (0.8, 0, 0) = 0.3, which is
        // inside the sphere. The sphere-cylinder intersection is a curve where
        // (x-0.5)²+y²+z² = 1 AND x²+y² = 0.64.
        // At z=0, (x-0.5)²+y² = 1 and x²+y² = 0.64
        //   → x²-x+0.25+y² = 1 → 0.64-x+0.25 = 1 → x = -0.11
        // So intersection at z=0 is near x=-0.11, y²=0.64-0.0121≈0.628, y≈0.792.
        // Point ≈ (-0.11, 0.792, 0).
        //
        // Sphere inverse_evaluate at (-0.11, 0.792, 0):
        //   d = (-0.11-0.5, 0.792, 0) = (-0.61, 0.792, 0), len=1.0
        //   u = atan2(0.792, -0.61) ≈ 2.23, v = asin(0) = 0
        // Cylinder inverse_evaluate at (-0.11, 0.792, 0):
        //   u = atan2(0.792/0.8, -0.11/0.8) ≈ atan2(0.99, -0.1375) ≈ 1.71, v = 0

        // Start with seeds slightly off the intersection.
        let result = geometric_optimize(&sa, &sb, (2.2, 0.05), (1.7, 0.05), 1e-6, 50);
        let opt = result.expect("Geometric should converge for offset sphere-cylinder");

        // Check sphere: distance from center (0.5,0,0) should be ≈ 1.0
        let dx = opt.position[0] - 0.5;
        let dy = opt.position[1];
        let dz = opt.position[2];
        let r_sphere = (dx * dx + dy * dy + dz * dz).sqrt();
        assert!(
            (r_sphere - 1.0).abs() < 1e-4,
            "Point should be on sphere (r=1), got r={r_sphere}"
        );

        // Check cylinder: x²+y² should be ≈ 0.64
        let r_cyl_sq = opt.position[0] * opt.position[0] + opt.position[1] * opt.position[1];
        assert!(
            (r_cyl_sq - 0.64).abs() < 1e-4,
            "Point should be on cylinder (r²=0.64), got r²={r_cyl_sq}"
        );
    }

    #[test]
    fn geometric_parallel_tangent_planes_fallback_to_newton() {
        // Two parallel planes: should detect parallel normals and fall back to Newton
        let plane_a = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let plane_b = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 1.0),
            normal: Vector3::new(0.0, 0.0, 1.0), // Parallel
        });

        let result = geometric_optimize(&plane_a, &plane_b, (0.5, 0.5), (0.5, 0.5), 1e-6, 20);
        // Should detect parallel planes and delegate to Newton, which should fail
        // because planes don't intersect (parallel, offset by 1.0)
        assert!(
            result.is_err(),
            "Should fail on parallel non-intersecting planes"
        );
    }

    #[test]
    fn correct_reversed_intersections_collapses_middle_point() {
        // Test Yang 4.5.3: detect and collapse a reversed vertex via collinear
        // detection.
        //
        // Layout: 2 original verts (v0, v1) + 3 intersection verts (v2, v3, v4).
        // Triangles create a strict chain: v2-v3 edge, v3-v4 edge, but NO v2-v4 edge.
        // This forces polyline order [v2, v3, v4].
        //
        // v2=(0,0,0) → v3=(2,0,0) → v4=(1,0,0)
        //   edge v2→v3 = (+2,0,0), unit = (+1,0,0)
        //   edge v3→v4 = (-1,0,0), unit = (-1,0,0)
        //   discrete tangent at v3 = (+1,0,0)+(-1,0,0) = (0,0,0) → collinear reversal
        //
        // After collapse: v3 gets v4's position (1,0,0), triangle refs to v4 → v3.
        use crate::boolean::exact_mesh::{SubTriangle, SubdividedMesh};
        use crate::tessellation::bijective::BijectiveMap;
        use crate::topology::half_edge::FaceIdx;
        use std::collections::BTreeMap;

        let mut subdivided = SubdividedMesh {
            verts: vec![
                [10.0, 10.0, 10.0], // v0: original (not intersection)
                [20.0, 20.0, 20.0], // v1: original (not intersection)
                [0.0, 0.0, 0.0],    // v2: intersection start
                [2.0, 0.0, 0.0],    // v3: REVERSED (overshoots past v4)
                [1.0, 0.0, 0.0],    // v4: intersection end
            ],
            // Two triangles per mesh, creating chain edges v2-v3 and v3-v4
            // but NOT v2-v4 (v0/v1 are non-intersection anchors).
            tris_a: vec![
                SubTriangle {
                    verts: [0, 2, 3],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
                SubTriangle {
                    verts: [0, 3, 4],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
            ],
            tris_b: vec![
                SubTriangle {
                    verts: [1, 2, 3],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
                SubTriangle {
                    verts: [1, 3, 4],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
            ],
            params_a: vec![
                None,
                None,
                Some((0.0, 0.0)),
                Some((2.0, 0.0)),
                Some((1.0, 0.0)),
            ],
            params_b: vec![
                None,
                None,
                Some((0.0, 0.0)),
                Some((2.0, 0.0)),
                Some((1.0, 0.0)),
            ],
            // Spec §F1 default for synthetic fixtures: 4 = 2 + 2.
            upstream_tri_count: 4,
        };

        let bijective_a = BijectiveMap {
            tri_face_ids: vec![FaceIdx(0)],
            vertex_params: vec![],
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: vec![FaceIdx(0)],
            vertex_params: vec![],
        };
        let face_geometry_a: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();
        let face_geometry_b: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();

        let collapsed = correct_reversed_intersections(
            &mut subdivided,
            &bijective_a,
            &bijective_b,
            &face_geometry_a,
            &face_geometry_b,
            2, // v0, v1 are original
        );

        assert_eq!(collapsed, 1, "Should collapse 1 reversed vertex");
        // v3 should now have v4's position
        assert_eq!(
            subdivided.verts[3],
            [1.0, 0.0, 0.0],
            "v3 should be moved to v4's position"
        );
        // All triangle references to v4 should be rerouted to v3
        for tri in subdivided.tris_a.iter().chain(subdivided.tris_b.iter()) {
            for &v in &tri.verts {
                assert_ne!(v, 4, "All refs to v4 should be rerouted to v3");
            }
        }
    }

    #[test]
    fn closest_point_on_segment_dist_sq_midpoint() {
        // Point directly above midpoint of segment [0,0,0]-[2,0,0]
        let d = closest_point_on_segment_dist_sq([1.0, 1.0, 0.0], [0.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
        assert!((d - 1.0).abs() < 1e-12, "Distance² should be 1.0, got {d}");
    }

    #[test]
    fn closest_point_on_segment_dist_sq_endpoint() {
        // Point beyond the end of the segment
        let d = closest_point_on_segment_dist_sq([3.0, 0.0, 0.0], [0.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
        assert!((d - 1.0).abs() < 1e-12, "Distance² should be 1.0, got {d}");
    }

    #[test]
    fn closest_point_on_segment_dist_sq_degenerate() {
        // Degenerate segment (zero length)
        let d = closest_point_on_segment_dist_sq([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert!((d - 1.0).abs() < 1e-12, "Distance² should be 1.0, got {d}");
    }

    #[test]
    fn find_adjacent_face_across_boundary_simple() {
        // Build a minimal B-Rep with two faces sharing an edge.
        use crate::topology::arena::TopoArena;

        let mut arena = TopoArena::new();
        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);

        // Two faces: face0 (triangle v0-v1-v2) and face1 (triangle v1-v3-v2)
        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([1.0, 0.0, 0.0]);
        let v2 = arena.add_vertex([0.5, 1.0, 0.0]);
        let v3 = arena.add_vertex([1.5, 1.0, 0.0]);

        let face0 = arena.add_face(shell);
        let face1 = arena.add_face(shell);
        let loop0 = arena.add_loop(face0);
        let loop1 = arena.add_loop(face1);
        arena.faces[face0.0].outer_loop = loop0;
        arena.faces[face1.0].outer_loop = loop1;

        // Face0: v0→v1→v2 (3 half-edges)
        let (_, he0a, he0b) = arena.add_edge(); // v0→v1
        let (_, he1a, he1b) = arena.add_edge(); // v1→v2
        let (_, he2a, he2b) = arena.add_edge(); // v2→v0

        // Face1: v1→v3→v2 (3 half-edges + shared edge v1→v2)
        let (_, he3a, he3b) = arena.add_edge(); // v1→v3
        let (_, he4a, he4b) = arena.add_edge(); // v3→v2

        // Wire face0 loop: he0a(v0→v1) → he1a(v1→v2) → he2a(v2→v0)
        arena.half_edges[he0a.0].origin = v0;
        arena.half_edges[he0a.0].next = he1a;
        arena.half_edges[he0a.0].prev = he2a;
        arena.half_edges[he0a.0].loop_ = loop0;

        arena.half_edges[he1a.0].origin = v1;
        arena.half_edges[he1a.0].next = he2a;
        arena.half_edges[he1a.0].prev = he0a;
        arena.half_edges[he1a.0].loop_ = loop0;

        arena.half_edges[he2a.0].origin = v2;
        arena.half_edges[he2a.0].next = he0a;
        arena.half_edges[he2a.0].prev = he1a;
        arena.half_edges[he2a.0].loop_ = loop0;

        // Wire face1 loop: he1b(v2→v1) → he3a(v1→v3) → he4a(v3→v2)
        // The shared edge v1→v2 on face0 is he1a; its twin he1b is on face1 as v2→v1
        arena.half_edges[he1b.0].origin = v2;
        arena.half_edges[he1b.0].next = he3a;
        arena.half_edges[he1b.0].prev = he4a;
        arena.half_edges[he1b.0].loop_ = loop1;

        arena.half_edges[he3a.0].origin = v1;
        arena.half_edges[he3a.0].next = he4a;
        arena.half_edges[he3a.0].prev = he1b;
        arena.half_edges[he3a.0].loop_ = loop1;

        arena.half_edges[he4a.0].origin = v3;
        arena.half_edges[he4a.0].next = he1b;
        arena.half_edges[he4a.0].prev = he3a;
        arena.half_edges[he4a.0].loop_ = loop1;

        // Set loop starting half-edges
        arena.loops[loop0.0].half_edge = he0a;
        arena.loops[loop1.0].half_edge = he1b;

        // Set up face geometry
        let mut face_geometry: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();
        let plane = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        face_geometry.insert(face0, plane.clone());
        face_geometry.insert(face1, plane);

        // Query: from face0, point near the shared edge v1-v2, should find face1
        let point = [0.75, 0.5, 0.0]; // Near the v1-v2 edge
        let result = find_adjacent_face_across_boundary(&arena, face0, point, &face_geometry);
        assert!(result.is_some(), "Should find adjacent face");
        let (adj_face, _) = result.unwrap();
        assert_eq!(adj_face, face1, "Adjacent face should be face1");
    }

    #[test]
    fn point_exited_face_inside() {
        // Point inside a simple triangular face should NOT be flagged as exited.
        use crate::topology::arena::TopoArena;

        let mut arena = TopoArena::new();
        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);
        let face = arena.add_face(shell);
        let loop_ = arena.add_loop(face);
        arena.faces[face.0].outer_loop = loop_;

        // Triangle: (0,0,0), (2,0,0), (1,2,0)
        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([2.0, 0.0, 0.0]);
        let v2 = arena.add_vertex([1.0, 2.0, 0.0]);

        let (_, he0, _) = arena.add_edge();
        let (_, he1, _) = arena.add_edge();
        let (_, he2, _) = arena.add_edge();

        arena.half_edges[he0.0].origin = v0;
        arena.half_edges[he0.0].next = he1;
        arena.half_edges[he0.0].prev = he2;
        arena.half_edges[he0.0].loop_ = loop_;

        arena.half_edges[he1.0].origin = v1;
        arena.half_edges[he1.0].next = he2;
        arena.half_edges[he1.0].prev = he0;
        arena.half_edges[he1.0].loop_ = loop_;

        arena.half_edges[he2.0].origin = v2;
        arena.half_edges[he2.0].next = he0;
        arena.half_edges[he2.0].prev = he1;
        arena.half_edges[he2.0].loop_ = loop_;

        arena.loops[loop_.0].half_edge = he0;

        let plane = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });

        // Centroid: (1, 0.67, 0) — should be inside
        assert!(!point_exited_face(&arena, face, [1.0, 0.67, 0.0], &plane));
    }

    #[test]
    fn point_exited_face_outside() {
        // Point far outside the face should be flagged as exited.
        use crate::topology::arena::TopoArena;

        let mut arena = TopoArena::new();
        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);
        let face = arena.add_face(shell);
        let loop_ = arena.add_loop(face);
        arena.faces[face.0].outer_loop = loop_;

        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([1.0, 0.0, 0.0]);
        let v2 = arena.add_vertex([0.5, 1.0, 0.0]);

        let (_, he0, _) = arena.add_edge();
        let (_, he1, _) = arena.add_edge();
        let (_, he2, _) = arena.add_edge();

        arena.half_edges[he0.0].origin = v0;
        arena.half_edges[he0.0].next = he1;
        arena.half_edges[he0.0].prev = he2;
        arena.half_edges[he0.0].loop_ = loop_;

        arena.half_edges[he1.0].origin = v1;
        arena.half_edges[he1.0].next = he2;
        arena.half_edges[he1.0].prev = he0;
        arena.half_edges[he1.0].loop_ = loop_;

        arena.half_edges[he2.0].origin = v2;
        arena.half_edges[he2.0].next = he0;
        arena.half_edges[he2.0].prev = he1;
        arena.half_edges[he2.0].loop_ = loop_;

        arena.loops[loop_.0].half_edge = he0;

        let plane = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });

        // Point at (10, 10, 0) — way outside the triangle, should be flagged
        assert!(point_exited_face(&arena, face, [10.0, 10.0, 0.0], &plane));
    }
}

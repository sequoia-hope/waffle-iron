use crate::sketch::Sketch;
use crate::constraint::{Constraint, SketchEntity};
use thiserror::Error;

/// Result of running the constraint solver.
#[derive(Debug)]
pub struct SolverResult {
    pub converged: bool,
    pub iterations: usize,
    pub final_residual: f64,
    pub params: Vec<f64>,
}

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("Solver did not converge after {max_iterations} iterations (residual: {residual})")]
    DidNotConverge {
        max_iterations: usize,
        residual: f64,
    },
    #[error("Under-constrained system: {dof} degrees of freedom remain")]
    UnderConstrained { dof: i64 },
    #[error("Over-constrained system: constraints are contradictory")]
    OverConstrained,
}

/// Configuration for the Gauss-Newton / Levenberg-Marquardt solver.
#[derive(Debug, Clone)]
pub struct SolverConfig {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub lambda_initial: f64,
    pub lambda_factor: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-10,
            lambda_initial: 1e-3,
            lambda_factor: 10.0,
        }
    }
}

/// Solve sketch constraints using Gauss-Newton with Levenberg-Marquardt damping.
///
/// Each constraint produces one or more scalar residual equations r_i(x) where r_i = 0
/// when satisfied. We build the Jacobian J analytically and solve:
///   (J^T J + lambda * I) * dx = -J^T * r
/// for the parameter update dx.
pub fn solve_sketch(sketch: &mut Sketch, config: &SolverConfig) -> Result<SolverResult, SolverError> {
    let mut params = sketch.params.clone();
    let n = params.len();

    if n == 0 || sketch.constraints.is_empty() {
        return Ok(SolverResult {
            converged: true,
            iterations: 0,
            final_residual: 0.0,
            params: params.clone(),
        });
    }

    let mut lambda = config.lambda_initial;

    for iteration in 0..config.max_iterations {
        // Collect all residuals
        let residuals = collect_residuals(sketch, &params);
        let m = residuals.len();

        let total_sq: f64 = residuals.iter().map(|r| r * r).sum();
        if total_sq < config.tolerance {
            sketch.params = params.clone();
            return Ok(SolverResult {
                converged: true,
                iterations: iteration,
                final_residual: total_sq,
                params,
            });
        }

        // Build Jacobian analytically
        let jacobian = build_jacobian(sketch, &params, m, n);

        // Compute J^T * r
        let mut jtr = vec![0.0; n];
        for j in 0..n {
            for i in 0..m {
                jtr[j] += jacobian[i * n + j] * residuals[i];
            }
        }

        // Compute J^T * J
        let mut jtj = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                let mut sum = 0.0;
                for k in 0..m {
                    sum += jacobian[k * n + i] * jacobian[k * n + j];
                }
                jtj[i * n + j] = sum;
            }
        }

        // Try Levenberg-Marquardt step with adaptive damping
        let mut found_better = false;
        for _ in 0..10 {
            // Add damping: (J^T J + lambda * I)
            let mut damped = jtj.clone();
            for i in 0..n {
                damped[i * n + i] += lambda;
            }

            // Solve damped * dx = -jtr
            if let Some(dx) = solve_linear_system(&damped, &jtr, n) {
                let mut new_params = params.clone();
                for i in 0..n {
                    new_params[i] -= dx[i];
                }

                let new_residuals = collect_residuals(sketch, &new_params);
                let new_sq: f64 = new_residuals.iter().map(|r| r * r).sum();

                if new_sq < total_sq {
                    params = new_params;
                    lambda = (lambda / config.lambda_factor).max(1e-15);
                    found_better = true;
                    break;
                }
            }
            lambda *= config.lambda_factor;
        }

        if !found_better {
            // If LM step failed, try a small gradient descent step as fallback
            let grad_norm_sq: f64 = jtr.iter().map(|g| g * g).sum();
            if grad_norm_sq > 1e-20 {
                let step = 0.01 / grad_norm_sq.sqrt();
                for i in 0..n {
                    params[i] -= step * jtr[i];
                }
            }
            lambda *= config.lambda_factor;
        }
    }

    let residuals = collect_residuals(sketch, &params);
    let final_residual: f64 = residuals.iter().map(|r| r * r).sum();
    sketch.params = params.clone();

    if final_residual < config.tolerance {
        Ok(SolverResult {
            converged: true,
            iterations: config.max_iterations,
            final_residual,
            params,
        })
    } else {
        Err(SolverError::DidNotConverge {
            max_iterations: config.max_iterations,
            residual: final_residual,
        })
    }
}

/// Collect individual (non-squared) residuals from all constraints.
/// Each constraint may produce one or more residual equations.
fn collect_residuals(sketch: &Sketch, params: &[f64]) -> Vec<f64> {
    let mut residuals = Vec::new();
    for c in &sketch.constraints {
        constraint_residuals(c, params, &sketch.entities, &mut residuals);
    }
    residuals
}

/// Produce individual residuals for a constraint (r_i = 0 when satisfied).
fn constraint_residuals(c: &Constraint, params: &[f64], entities: &[SketchEntity], out: &mut Vec<f64>) {
    match c {
        Constraint::Coincident { point_a, point_b } => {
            let (ax, ay) = entity_point(entities, *point_a, params);
            let (bx, by) = entity_point(entities, *point_b, params);
            out.push(ax - bx);
            out.push(ay - by);
        }
        Constraint::Distance { point_a, point_b, value } => {
            let (ax, ay) = entity_point(entities, *point_a, params);
            let (bx, by) = entity_point(entities, *point_b, params);
            let dist_sq = (ax - bx).powi(2) + (ay - by).powi(2);
            // Use squared distance residual to avoid sqrt derivative singularity at 0
            out.push(dist_sq - value * value);
        }
        Constraint::Horizontal { line } => {
            if let SketchEntity::Line { start_param, end_param } = &entities[*line] {
                out.push(params[start_param + 1] - params[end_param + 1]);
            }
        }
        Constraint::Vertical { line } => {
            if let SketchEntity::Line { start_param, end_param } = &entities[*line] {
                out.push(params[*start_param] - params[*end_param]);
            }
        }
        Constraint::Fixed { point, x, y } => {
            let (px, py) = entity_point(entities, *point, params);
            out.push(px - x);
            out.push(py - y);
        }
        Constraint::Radius { entity, value } => {
            if let SketchEntity::Circle { radius_param, .. } = &entities[*entity] {
                out.push(params[*radius_param] - value);
            }
        }
        Constraint::Parallel { line_a, line_b } => {
            let (dx_a, dy_a) = line_direction(entities, *line_a, params);
            let (dx_b, dy_b) = line_direction(entities, *line_b, params);
            out.push(dx_a * dy_b - dy_a * dx_b);
        }
        Constraint::Perpendicular { line_a, line_b } => {
            let (dx_a, dy_a) = line_direction(entities, *line_a, params);
            let (dx_b, dy_b) = line_direction(entities, *line_b, params);
            out.push(dx_a * dx_b + dy_a * dy_b);
        }
        Constraint::Angle { line_a, line_b, value } => {
            let (dx_a, dy_a) = line_direction(entities, *line_a, params);
            let (dx_b, dy_b) = line_direction(entities, *line_b, params);
            let cross = dx_a * dy_b - dy_a * dx_b;
            let dot = dx_a * dx_b + dy_a * dy_b;
            out.push(cross - dot * value.tan());
        }
        Constraint::Equal { entity_a, entity_b } => {
            let len_a = entity_length(entities, *entity_a, params);
            let len_b = entity_length(entities, *entity_b, params);
            out.push(len_a - len_b);
        }
        Constraint::PointOnEntity { point, entity } => {
            point_on_entity_residual(entities, *point, *entity, params, out);
        }
        Constraint::Symmetric { point_a, point_b, axis } => {
            symmetric_residual(entities, *point_a, *point_b, *axis, params, out);
        }
        Constraint::Tangent { entity_a, entity_b } => {
            tangent_residual_vec(entities, *entity_a, *entity_b, params, out);
        }
    }
}

/// Build the Jacobian matrix analytically. Returns flat row-major [m x n] array.
fn build_jacobian(sketch: &Sketch, params: &[f64], m: usize, n: usize) -> Vec<f64> {
    let mut jac = vec![0.0; m * n];
    let mut row = 0;

    for c in &sketch.constraints {
        let row_count = constraint_jacobian(c, params, &sketch.entities, n, &mut jac, row);
        row += row_count;
    }

    debug_assert_eq!(row, m);
    jac
}

/// Compute Jacobian rows for a single constraint. Returns number of rows written.
fn constraint_jacobian(
    c: &Constraint,
    params: &[f64],
    entities: &[SketchEntity],
    n: usize,
    jac: &mut [f64],
    start_row: usize,
) -> usize {
    match c {
        Constraint::Coincident { point_a, point_b } => {
            let pi_a = param_indices_for_point(entities, *point_a);
            let pi_b = param_indices_for_point(entities, *point_b);
            if let (Some((ax, ay)), Some((bx, by))) = (pi_a, pi_b) {
                // r0 = params[ax] - params[bx], r1 = params[ay] - params[by]
                jac[start_row * n + ax] = 1.0;
                jac[start_row * n + bx] = -1.0;
                jac[(start_row + 1) * n + ay] = 1.0;
                jac[(start_row + 1) * n + by] = -1.0;
            }
            2
        }
        Constraint::Distance { point_a, point_b, .. } => {
            let pi_a = param_indices_for_point(entities, *point_a);
            let pi_b = param_indices_for_point(entities, *point_b);
            if let (Some((ax_i, ay_i)), Some((bx_i, by_i))) = (pi_a, pi_b) {
                let ax = params[ax_i];
                let ay = params[ay_i];
                let bx = params[bx_i];
                let by = params[by_i];
                // r = (ax-bx)^2 + (ay-by)^2 - d^2
                // dr/d(ax) = 2*(ax-bx), etc.
                let r = start_row;
                jac[r * n + ax_i] = 2.0 * (ax - bx);
                jac[r * n + ay_i] = 2.0 * (ay - by);
                jac[r * n + bx_i] = -2.0 * (ax - bx);
                jac[r * n + by_i] = -2.0 * (ay - by);
            }
            1
        }
        Constraint::Horizontal { line } => {
            if let SketchEntity::Line { start_param, end_param } = &entities[*line] {
                // r = y1 - y2
                jac[start_row * n + start_param + 1] = 1.0;
                jac[start_row * n + end_param + 1] = -1.0;
            }
            1
        }
        Constraint::Vertical { line } => {
            if let SketchEntity::Line { start_param, end_param } = &entities[*line] {
                // r = x1 - x2
                jac[start_row * n + *start_param] = 1.0;
                jac[start_row * n + *end_param] = -1.0;
            }
            1
        }
        Constraint::Fixed { point, .. } => {
            if let Some((px, py)) = param_indices_for_point(entities, *point) {
                jac[start_row * n + px] = 1.0;
                jac[(start_row + 1) * n + py] = 1.0;
            }
            2
        }
        Constraint::Radius { entity, .. } => {
            if let SketchEntity::Circle { radius_param, .. } = &entities[*entity] {
                jac[start_row * n + *radius_param] = 1.0;
            }
            1
        }
        Constraint::Parallel { line_a, line_b } => {
            // r = dx_a * dy_b - dy_a * dx_b
            if let (
                SketchEntity::Line { start_param: sa, end_param: ea },
                SketchEntity::Line { start_param: sb, end_param: eb },
            ) = (&entities[*line_a], &entities[*line_b]) {
                let dx_a = params[*ea] - params[*sa];
                let dy_a = params[ea + 1] - params[sa + 1];
                let dx_b = params[*eb] - params[*sb];
                let dy_b = params[eb + 1] - params[sb + 1];
                let r = start_row;
                // dr/d(sa_x) = -dy_b, dr/d(ea_x) = dy_b
                // dr/d(sa_y) = dx_b,  dr/d(ea_y) = -dx_b
                // dr/d(sb_x) = dy_a,  dr/d(eb_x) = -dy_a
                // dr/d(sb_y) = -dx_a, dr/d(eb_y) = dx_a
                jac[r * n + *sa] += -dy_b;
                jac[r * n + *ea] += dy_b;
                jac[r * n + sa + 1] += dx_b;
                jac[r * n + ea + 1] += -dx_b;
                jac[r * n + *sb] += dy_a;
                jac[r * n + *eb] += -dy_a;
                jac[r * n + sb + 1] += -dx_a;
                jac[r * n + eb + 1] += dx_a;
            }
            1
        }
        Constraint::Perpendicular { line_a, line_b } => {
            // r = dx_a * dx_b + dy_a * dy_b
            if let (
                SketchEntity::Line { start_param: sa, end_param: ea },
                SketchEntity::Line { start_param: sb, end_param: eb },
            ) = (&entities[*line_a], &entities[*line_b]) {
                let dx_a = params[*ea] - params[*sa];
                let dy_a = params[ea + 1] - params[sa + 1];
                let dx_b = params[*eb] - params[*sb];
                let dy_b = params[eb + 1] - params[sb + 1];
                let r = start_row;
                jac[r * n + *sa] += -dx_b;
                jac[r * n + *ea] += dx_b;
                jac[r * n + sa + 1] += -dy_b;
                jac[r * n + ea + 1] += dy_b;
                jac[r * n + *sb] += -dx_a;
                jac[r * n + *eb] += dx_a;
                jac[r * n + sb + 1] += -dy_a;
                jac[r * n + eb + 1] += dy_a;
            }
            1
        }
        Constraint::Angle { .. } => {
            // Finite difference fallback for angle
            let h = 1e-8;
            let mut r_base = Vec::new();
            constraint_residuals(c, params, entities, &mut r_base);
            let r_idx = start_row;
            for j in 0..n {
                let mut p_plus = params.to_vec();
                p_plus[j] += h;
                let mut r_plus = Vec::new();
                constraint_residuals(c, &p_plus, entities, &mut r_plus);
                if !r_plus.is_empty() && !r_base.is_empty() {
                    jac[r_idx * n + j] = (r_plus[0] - r_base[0]) / h;
                }
            }
            1
        }
        Constraint::Equal { .. } => {
            // Finite difference
            let h = 1e-8;
            let mut r_base = Vec::new();
            constraint_residuals(c, params, entities, &mut r_base);
            for j in 0..n {
                let mut p_plus = params.to_vec();
                p_plus[j] += h;
                let mut r_plus = Vec::new();
                constraint_residuals(c, &p_plus, entities, &mut r_plus);
                if !r_plus.is_empty() && !r_base.is_empty() {
                    jac[start_row * n + j] = (r_plus[0] - r_base[0]) / h;
                }
            }
            1
        }
        Constraint::PointOnEntity { .. } => {
            let h = 1e-8;
            let mut r_base = Vec::new();
            constraint_residuals(c, params, entities, &mut r_base);
            let num = r_base.len();
            for ri in 0..num {
                for j in 0..n {
                    let mut p_plus = params.to_vec();
                    p_plus[j] += h;
                    let mut r_plus = Vec::new();
                    constraint_residuals(c, &p_plus, entities, &mut r_plus);
                    if ri < r_plus.len() && ri < r_base.len() {
                        jac[(start_row + ri) * n + j] = (r_plus[ri] - r_base[ri]) / h;
                    }
                }
            }
            num
        }
        Constraint::Symmetric { .. } => {
            let h = 1e-8;
            let mut r_base = Vec::new();
            constraint_residuals(c, params, entities, &mut r_base);
            let num = r_base.len();
            for ri in 0..num {
                for j in 0..n {
                    let mut p_plus = params.to_vec();
                    p_plus[j] += h;
                    let mut r_plus = Vec::new();
                    constraint_residuals(c, &p_plus, entities, &mut r_plus);
                    if ri < r_plus.len() && ri < r_base.len() {
                        jac[(start_row + ri) * n + j] = (r_plus[ri] - r_base[ri]) / h;
                    }
                }
            }
            num
        }
        Constraint::Tangent { .. } => {
            // Finite difference for tangent Jacobian
            let h = 1e-8;
            let mut r_base = Vec::new();
            constraint_residuals(c, params, entities, &mut r_base);
            let num = r_base.len();
            for ri in 0..num {
                for j in 0..n {
                    let mut p_plus = params.to_vec();
                    p_plus[j] += h;
                    let mut r_plus = Vec::new();
                    constraint_residuals(c, &p_plus, entities, &mut r_plus);
                    if ri < r_plus.len() && ri < r_base.len() {
                        jac[(start_row + ri) * n + j] = (r_plus[ri] - r_base[ri]) / h;
                    }
                }
            }
            num
        }
    }
}

fn entity_point(entities: &[SketchEntity], idx: usize, params: &[f64]) -> (f64, f64) {
    match &entities[idx] {
        SketchEntity::Point { param_index } => (params[*param_index], params[param_index + 1]),
        SketchEntity::Circle { center_param, .. } => {
            (params[*center_param], params[center_param + 1])
        }
        _ => (0.0, 0.0),
    }
}

fn param_indices_for_point(entities: &[SketchEntity], idx: usize) -> Option<(usize, usize)> {
    match &entities[idx] {
        SketchEntity::Point { param_index } => Some((*param_index, param_index + 1)),
        SketchEntity::Circle { center_param, .. } => Some((*center_param, center_param + 1)),
        _ => None,
    }
}

fn line_direction(entities: &[SketchEntity], idx: usize, params: &[f64]) -> (f64, f64) {
    if let SketchEntity::Line { start_param, end_param } = &entities[idx] {
        (params[*end_param] - params[*start_param], params[end_param + 1] - params[start_param + 1])
    } else {
        (1.0, 0.0)
    }
}

fn entity_length(entities: &[SketchEntity], idx: usize, params: &[f64]) -> f64 {
    match &entities[idx] {
        SketchEntity::Line { start_param, end_param } => {
            let dx = params[*end_param] - params[*start_param];
            let dy = params[end_param + 1] - params[start_param + 1];
            (dx * dx + dy * dy).sqrt()
        }
        SketchEntity::Circle { radius_param, .. } => params[*radius_param],
        _ => 0.0,
    }
}

fn point_on_entity_residual(
    entities: &[SketchEntity],
    point_idx: usize,
    entity_idx: usize,
    params: &[f64],
    out: &mut Vec<f64>,
) {
    let (px, py) = entity_point(entities, point_idx, params);
    match &entities[entity_idx] {
        SketchEntity::Line { start_param, end_param } => {
            // Point on line: cross product of (P - A) x (B - A) = 0
            let ax = params[*start_param];
            let ay = params[start_param + 1];
            let bx = params[*end_param];
            let by = params[end_param + 1];
            out.push((px - ax) * (by - ay) - (py - ay) * (bx - ax));
        }
        SketchEntity::Circle { center_param, radius_param } => {
            // Distance from center equals radius
            let cx = params[*center_param];
            let cy = params[center_param + 1];
            let r = params[*radius_param];
            out.push((px - cx).powi(2) + (py - cy).powi(2) - r * r);
        }
        _ => {}
    }
}

fn symmetric_residual(
    entities: &[SketchEntity],
    point_a: usize,
    point_b: usize,
    axis: usize,
    params: &[f64],
    out: &mut Vec<f64>,
) {
    let (ax, ay) = entity_point(entities, point_a, params);
    let (bx, by) = entity_point(entities, point_b, params);

    if let SketchEntity::Line { start_param, end_param } = &entities[axis] {
        let lx0 = params[*start_param];
        let ly0 = params[start_param + 1];
        let lx1 = params[*end_param];
        let ly1 = params[end_param + 1];
        let dx = lx1 - lx0;
        let dy = ly1 - ly0;
        let len_sq = dx * dx + dy * dy;
        if len_sq > 1e-20 {
            // Midpoint of A and B should lie on the axis line
            let mx = (ax + bx) / 2.0;
            let my = (ay + by) / 2.0;
            let cross = (mx - lx0) * dy - (my - ly0) * dx;
            out.push(cross);
            // Vector A->B should be perpendicular to axis
            let dot = (bx - ax) * dx + (by - ay) * dy;
            out.push(dot);
        }
    }
}

fn tangent_residual_vec(
    entities: &[SketchEntity],
    a: usize,
    b: usize,
    params: &[f64],
    out: &mut Vec<f64>,
) {
    match (&entities[a], &entities[b]) {
        (SketchEntity::Line { start_param, end_param }, SketchEntity::Circle { center_param, radius_param }) |
        (SketchEntity::Circle { center_param, radius_param }, SketchEntity::Line { start_param, end_param }) => {
            let ax = params[*start_param];
            let ay = params[start_param + 1];
            let bx = params[*end_param];
            let by = params[end_param + 1];
            let cx = params[*center_param];
            let cy = params[center_param + 1];
            let r = params[*radius_param];
            let dx = bx - ax;
            let dy = by - ay;
            let len_sq = dx * dx + dy * dy;
            // Residual: cross_product^2 / len_sq - r^2 = 0
            // Using: (distance_to_line)^2 = cross^2 / len_sq
            let cross = (cx - ax) * dy - (cy - ay) * dx;
            out.push(cross * cross / len_sq.max(1e-20) - r * r);
        }
        (SketchEntity::Circle { center_param: ca, radius_param: ra }, SketchEntity::Circle { center_param: cb, radius_param: rb }) => {
            let ax = params[*ca];
            let ay = params[ca + 1];
            let bx = params[*cb];
            let by = params[cb + 1];
            let r_a = params[*ra];
            let r_b = params[*rb];
            let dist_sq = (ax - bx).powi(2) + (ay - by).powi(2);
            // External tangency: dist^2 - (ra + rb)^2 = 0
            out.push(dist_sq - (r_a + r_b).powi(2));
        }
        _ => {}
    }
}

/// Solve a dense linear system A*x = b using Gaussian elimination with partial pivoting.
/// A is n x n in row-major. Returns None if singular.
fn solve_linear_system(a: &[f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    // Augmented matrix [A|b]
    let mut aug = vec![0.0; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            aug[i * (n + 1) + j] = a[i * n + j];
        }
        aug[i * (n + 1) + n] = b[i];
    }

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_val = aug[col * (n + 1) + col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            let val = aug[row * (n + 1) + col].abs();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }

        if max_val < 1e-15 {
            return None; // singular
        }

        // Swap rows
        if max_row != col {
            for j in 0..=n {
                let tmp = aug[col * (n + 1) + j];
                aug[col * (n + 1) + j] = aug[max_row * (n + 1) + j];
                aug[max_row * (n + 1) + j] = tmp;
            }
        }

        // Eliminate below
        let pivot = aug[col * (n + 1) + col];
        for row in (col + 1)..n {
            let factor = aug[row * (n + 1) + col] / pivot;
            for j in col..=n {
                aug[row * (n + 1) + j] -= factor * aug[col * (n + 1) + j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i * (n + 1) + n];
        for j in (i + 1)..n {
            sum -= aug[i * (n + 1) + j] * x[j];
        }
        let diag = aug[i * (n + 1) + i];
        if diag.abs() < 1e-15 {
            return None;
        }
        x[i] = sum / diag;
    }

    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Constraint;
    use crate::sketch::Sketch;

    #[test]
    fn test_solve_horizontal_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 5.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Horizontal { line });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let result = result.unwrap();
        assert!(result.converged);
        assert!(result.final_residual < 1e-8);

        let (_, y1) = sketch.point_position(p1);
        let (_, y2) = sketch.point_position(p2);
        assert!((y1 - y2).abs() < 1e-6, "Line not horizontal: y1={}, y2={}", y1, y2);
    }

    #[test]
    fn test_solve_distance_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(3.0, 4.0);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Distance { point_a: p1, point_b: p2, value: 10.0 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver error: {:?}", result.err());

        let (x2, y2) = sketch.point_position(p2);
        let dist = (x2 * x2 + y2 * y2).sqrt();
        assert!((dist - 10.0).abs() < 0.01, "Distance is {} instead of 10.0", dist);
    }

    #[test]
    fn test_solve_already_satisfied() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Horizontal { line });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config).unwrap();
        assert!(result.converged);
        assert_eq!(result.iterations, 0);
    }

    #[test]
    fn test_solve_vertical_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(3.0, 10.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Vertical { line });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (x1, _) = sketch.point_position(p1);
        let (x2, _) = sketch.point_position(p2);
        assert!((x1 - x2).abs() < 1e-6, "Line not vertical: x1={}, x2={}", x1, x2);
    }

    #[test]
    fn test_solve_perpendicular_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let p3 = sketch.add_point(0.0, 0.0);
        let p4 = sketch.add_point(3.0, 5.0);
        let l1 = sketch.add_line(p1, p2);
        let l2 = sketch.add_line(p3, p4);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p2, x: 10.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p3, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Perpendicular { line_a: l1, line_b: l2 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (x4, _y4) = sketch.point_position(p4);
        // p4 should have moved so that l2 is vertical (perpendicular to horizontal l1)
        assert!(x4.abs() < 0.1, "Expected x4 near 0, got {}", x4);
    }

    #[test]
    fn test_solve_coincident_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(5.0, 3.0);
        let p2 = sketch.add_point(8.0, 7.0);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 5.0, y: 3.0 });
        sketch.add_constraint(Constraint::Coincident { point_a: p1, point_b: p2 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (x1, y1) = sketch.point_position(p1);
        let (x2, y2) = sketch.point_position(p2);
        assert!((x1 - x2).abs() < 1e-6);
        assert!((y1 - y2).abs() < 1e-6);
    }

    #[test]
    fn test_solve_parallel_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let p3 = sketch.add_point(0.0, 5.0);
        let p4 = sketch.add_point(7.0, 8.0); // not parallel initially
        let l1 = sketch.add_line(p1, p2);
        let l2 = sketch.add_line(p3, p4);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p2, x: 10.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p3, x: 0.0, y: 5.0 });
        sketch.add_constraint(Constraint::Parallel { line_a: l1, line_b: l2 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        // l2 should now be horizontal (parallel to l1)
        let (_, y3) = sketch.point_position(p3);
        let (_, y4) = sketch.point_position(p4);
        assert!((y3 - y4).abs() < 0.1, "Lines not parallel: y3={}, y4={}", y3, y4);
    }

    #[test]
    fn test_solve_multi_constraint_rectangle() {
        // Build a fully-constrained rectangle
        let mut sketch = Sketch::new();
        let p0 = sketch.add_point(0.0, 0.0);
        let p1 = sketch.add_point(9.0, 0.5);   // should become (10, 0)
        let p2 = sketch.add_point(9.5, 4.5);   // should become (10, 5)
        let p3 = sketch.add_point(0.5, 5.5);   // should become (0, 5)

        let l0 = sketch.add_line(p0, p1); // bottom
        let l1 = sketch.add_line(p1, p2); // right
        let l2 = sketch.add_line(p2, p3); // top
        let l3 = sketch.add_line(p3, p0); // left

        // Fix origin
        sketch.add_constraint(Constraint::Fixed { point: p0, x: 0.0, y: 0.0 });
        // Horizontal/vertical sides
        sketch.add_constraint(Constraint::Horizontal { line: l0 });
        sketch.add_constraint(Constraint::Horizontal { line: l2 });
        sketch.add_constraint(Constraint::Vertical { line: l1 });
        sketch.add_constraint(Constraint::Vertical { line: l3 });
        // Dimensions
        sketch.add_constraint(Constraint::Distance { point_a: p0, point_b: p1, value: 10.0 });
        sketch.add_constraint(Constraint::Distance { point_a: p1, point_b: p2, value: 5.0 });

        let config = SolverConfig { max_iterations: 200, ..SolverConfig::default() };
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (x0, y0) = sketch.point_position(p0);
        let (x1, y1) = sketch.point_position(p1);
        let (_x2, y2) = sketch.point_position(p2);
        let (_x3, _y3) = sketch.point_position(p3);

        assert!((x0 - 0.0).abs() < 0.1, "p0.x = {}", x0);
        assert!((y0 - 0.0).abs() < 0.1, "p0.y = {}", y0);
        assert!((x1 - 10.0).abs() < 0.5, "p1.x = {}", x1);
        assert!((y1 - 0.0).abs() < 0.1, "p1.y = {}", y1);
        assert!((y2 - 5.0).abs() < 0.5, "p2.y = {}", y2);
    }

    #[test]
    fn test_solve_radius_constraint() {
        let mut sketch = Sketch::new();
        let c = sketch.add_circle(0.0, 0.0, 3.0);

        sketch.add_constraint(Constraint::Radius { entity: c, value: 10.0 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok());

        if let SketchEntity::Circle { radius_param, .. } = &sketch.entities[c] {
            assert!((sketch.params[*radius_param] - 10.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_solve_angle_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let p3 = sketch.add_point(0.0, 0.0);
        let p4 = sketch.add_point(5.0, 5.0);
        let l1 = sketch.add_line(p1, p2);
        let l2 = sketch.add_line(p3, p4);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p2, x: 10.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p3, x: 0.0, y: 0.0 });
        // 45 degrees
        sketch.add_constraint(Constraint::Angle {
            line_a: l1,
            line_b: l2,
            value: std::f64::consts::FRAC_PI_4,
        });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (x4, y4) = sketch.point_position(p4);
        // At 45 degrees, x4 should approximately equal y4
        assert!((x4 - y4).abs() < 0.5, "Expected 45-degree line: x4={x4}, y4={y4}");
    }

    #[test]
    fn test_solve_equal_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let p3 = sketch.add_point(0.0, 5.0);
        let p4 = sketch.add_point(3.0, 5.0);
        let l1 = sketch.add_line(p1, p2);
        let l2 = sketch.add_line(p3, p4);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p2, x: 10.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p3, x: 0.0, y: 5.0 });
        sketch.add_constraint(Constraint::Equal { entity_a: l1, entity_b: l2 });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        // l2 should now have length 10 (same as l1)
        let (x3, y3) = sketch.point_position(p3);
        let (x4, y4) = sketch.point_position(p4);
        let len = ((x4 - x3).powi(2) + (y4 - y3).powi(2)).sqrt();
        assert!((len - 10.0).abs() < 0.5, "Expected equal length 10, got {len}");
    }

    #[test]
    fn test_solve_point_on_line() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(p1, p2);
        let p3 = sketch.add_point(5.0, 3.0); // off the line

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: p2, x: 10.0, y: 0.0 });
        sketch.add_constraint(Constraint::PointOnEntity { point: p3, entity: line });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (_, y3) = sketch.point_position(p3);
        assert!(y3.abs() < 0.1, "Point should be on horizontal line, y3={y3}");
    }

    #[test]
    fn test_solve_point_on_circle() {
        let mut sketch = Sketch::new();
        let c = sketch.add_circle(0.0, 0.0, 5.0);
        let p = sketch.add_point(3.0, 1.0); // not on circle

        // Fix circle center and radius, let point move onto circle
        sketch.add_constraint(Constraint::Fixed { point: c, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Radius { entity: c, value: 5.0 });
        sketch.add_constraint(Constraint::PointOnEntity { point: p, entity: c });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (px, py) = sketch.point_position(p);
        let dist = (px * px + py * py).sqrt();
        assert!((dist - 5.0).abs() < 0.5, "Point should be on circle r=5, dist={dist}");
    }

    #[test]
    fn test_solve_symmetric_constraint() {
        let mut sketch = Sketch::new();
        // Axis: vertical line x=5
        let a1 = sketch.add_point(5.0, 0.0);
        let a2 = sketch.add_point(5.0, 10.0);
        let axis = sketch.add_line(a1, a2);

        let pa = sketch.add_point(2.0, 3.0);
        let pb = sketch.add_point(6.0, 3.0); // should become (8, 3) for symmetry about x=5

        sketch.add_constraint(Constraint::Fixed { point: a1, x: 5.0, y: 0.0 });
        sketch.add_constraint(Constraint::Fixed { point: a2, x: 5.0, y: 10.0 });
        sketch.add_constraint(Constraint::Fixed { point: pa, x: 2.0, y: 3.0 });
        sketch.add_constraint(Constraint::Symmetric { point_a: pa, point_b: pb, axis });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (xb, yb) = sketch.point_position(pb);
        assert!((xb - 8.0).abs() < 0.5, "Expected xb=8, got {xb}");
        assert!((yb - 3.0).abs() < 0.5, "Expected yb=3, got {yb}");
    }

    #[test]
    fn test_solve_tangent_line_circle() {
        let mut sketch = Sketch::new();
        let c = sketch.add_circle(0.0, 0.0, 5.0);
        let p1 = sketch.add_point(-10.0, 5.5);
        let p2 = sketch.add_point(10.0, 5.5);
        let line = sketch.add_line(p1, p2);

        // Fix circle and fix p1.x, let p1.y and p2.y be free to achieve tangency
        sketch.add_constraint(Constraint::Radius { entity: c, value: 5.0 });
        sketch.add_constraint(Constraint::Horizontal { line });
        sketch.add_constraint(Constraint::Tangent { entity_a: line, entity_b: c });

        let config = SolverConfig { max_iterations: 200, ..SolverConfig::default() };
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let (_, y1) = sketch.point_position(p1);
        // Line should be tangent: y = 5 or y = -5
        assert!((y1.abs() - 5.0).abs() < 0.5, "Expected tangent at y=+/-5, got y1={y1}");
    }

    #[test]
    fn test_linear_system_solve() {
        // 2x + y = 5
        // x + 3y = 7
        // Solution: x = 8/5, y = 9/5
        let a = vec![2.0, 1.0, 1.0, 3.0];
        let b = vec![5.0, 7.0];
        let x = solve_linear_system(&a, &b, 2).unwrap();
        assert!((x[0] - 1.6).abs() < 1e-10);
        assert!((x[1] - 1.8).abs() < 1e-10);
    }
}

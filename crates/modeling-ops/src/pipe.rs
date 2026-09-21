//! Pipe sweep operation (`specs/b2_pipe_sweep.md` checkpoint 2): a circle
//! swept along an open sketch chain, built by `Kernel::pipe` as one solid.
//! Roles: the start cap is `EndCapNegative` (its outward normal opposes the
//! path's start tangent), the end cap `EndCapPositive`, every lateral
//! `SideFace { index }` in kernel face order.

use waffle_types::kernel::units::TAU_WORK;
use waffle_types::kernel::{KernelId, KernelSolidHandle, PipePathSegment};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a pipe sweep. `path` is in the sketch plane's `(u, v)`
/// coordinates; the plane frame is the one `make_faces_from_profiles` takes.
#[allow(clippy::too_many_arguments)]
pub fn execute_pipe(
    kb: &mut dyn KernelBundle,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    plane_x_axis: [f64; 3],
    path: &[PipePathSegment],
    radius: f64,
    inner_radius: Option<f64>,
    before_snapshot: Option<&TopoSnapshot>,
) -> Result<OpResult, OpError> {
    if path.is_empty() {
        return Err(OpError::InvalidParameter {
            reason: "pipe path has no segments".to_string(),
        });
    }
    if !(radius.is_finite() && radius > 0.0) {
        return Err(OpError::InvalidParameter {
            reason: format!("pipe radius must be positive, got {radius}"),
        });
    }
    if let Some(ri) = inner_radius {
        if !(ri.is_finite() && ri > 0.0 && ri < radius) {
            return Err(OpError::InvalidParameter {
                reason: format!("pipe inner radius {ri} must be in (0, {radius})"),
            });
        }
    }

    let handle = kb.pipe(
        plane_origin,
        plane_normal,
        plane_x_axis,
        path,
        radius,
        inner_radius,
    )?;

    let after = diff::snapshot(kb.as_introspect(), &handle);
    let empty_snap = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let before = before_snapshot.unwrap_or(&empty_snap);
    let diff_result = diff::diff(before, &after);

    let (t_start, t_end) = end_tangents_world(plane_normal, plane_x_axis, path);
    let role_assignments = assign_pipe_roles(kb.as_introspect(), &handle, t_start, t_end);

    let provenance = Provenance {
        created: diff_result.created,
        deleted: diff_result.deleted,
        modified: Vec::new(),
        role_assignments,
    };

    Ok(OpResult {
        outputs: vec![(
            OutputKey::Main,
            BodyOutput {
                handle,
                mesh: None,
                edges: None,
            },
        )],
        provenance,
        diagnostics: Diagnostics::default(),
    })
}

/// Unit start / end tangents of the path in world space.
fn end_tangents_world(
    plane_normal: [f64; 3],
    plane_x_axis: [f64; 3],
    path: &[PipePathSegment],
) -> ([f64; 3], [f64; 3]) {
    let n = plane_normal;
    let x = plane_x_axis;
    let y = [
        n[1] * x[2] - n[2] * x[1],
        n[2] * x[0] - n[0] * x[2],
        n[0] * x[1] - n[1] * x[0],
    ];
    let embed = |t: (f64, f64)| -> [f64; 3] {
        let v = [
            t.0 * x[0] + t.1 * y[0],
            t.0 * x[1] + t.1 * y[1],
            t.0 * x[2] + t.1 * y[2],
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > TAU_WORK {
            [v[0] / l, v[1] / l, v[2] / l]
        } else {
            [0.0, 0.0, 0.0]
        }
    };
    let tangent_at = |seg: &PipePathSegment, at_start: bool| -> (f64, f64) {
        match *seg {
            PipePathSegment::Line { a, b } => {
                let d = (b.0 - a.0, b.1 - a.1);
                let l = (d.0 * d.0 + d.1 * d.1).sqrt().max(TAU_WORK);
                (d.0 / l, d.1 / l)
            }
            PipePathSegment::Arc {
                a, b, center, ccw, ..
            } => {
                let p = if at_start { a } else { b };
                let r = (p.0 - center.0, p.1 - center.1);
                let l = (r.0 * r.0 + r.1 * r.1).sqrt().max(TAU_WORK);
                let r = (r.0 / l, r.1 / l);
                if ccw {
                    (-r.1, r.0)
                } else {
                    (r.1, -r.0)
                }
            }
        }
    };
    (
        embed(tangent_at(&path[0], true)),
        embed(tangent_at(&path[path.len() - 1], false)),
    )
}

/// Caps by outward-normal alignment with the end tangents (the start cap's
/// outward normal is `−t_start`, the end cap's `+t_end`); everything else a
/// side face in kernel order.
fn assign_pipe_roles(
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
    solid: &KernelSolidHandle,
    t_start: [f64; 3],
    t_end: [f64; 3],
) -> Vec<(KernelId, Role)> {
    let faces = introspect.list_faces(solid);
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let normal_of = |f: KernelId| introspect.compute_signature(f, TopoKind::Face).normal;
    let best = |want: [f64; 3]| -> Option<KernelId> {
        faces
            .iter()
            .filter_map(|&f| normal_of(f).map(|n| (f, dot(n, want))))
            .filter(|(_, d)| *d > 0.9)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(f, _)| f)
    };
    let start_cap = best([-t_start[0], -t_start[1], -t_start[2]]);
    let end_cap = best(t_end).filter(|f| Some(*f) != start_cap);
    let mut assignments = Vec::with_capacity(faces.len());
    let mut side = 0usize;
    for &f in &faces {
        if Some(f) == start_cap {
            assignments.push((f, Role::EndCapNegative));
        } else if Some(f) == end_cap {
            assignments.push((f, Role::EndCapPositive));
        } else {
            assignments.push((f, Role::SideFace { index: side }));
            side += 1;
        }
    }
    assignments
}

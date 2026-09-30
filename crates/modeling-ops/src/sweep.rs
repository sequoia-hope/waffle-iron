//! General sweep operation (`specs/b6_general_sweep.md` increment S6): a
//! planar polygon section swept along a world-space chain of lines and
//! arcs, built by `Kernel::sweep` as ONE solid (rims shared, no boolean).
//!
//! Roles, as for the pipe: on an OPEN path the cap whose outward normal
//! opposes the path's start tangent is `EndCapNegative`, the cap along the
//! end tangent is `EndCapPositive`, and every lateral is `SideFace { index }`
//! in kernel face order. A CLOSED path is a ring with no caps, so every face
//! is a side face — a consumer that asks a ring for a cap gets nothing,
//! which is the truth rather than a guess.

use waffle_types::kernel::{KernelId, KernelSolidHandle, SweepSection};
use waffle_types::sketch3d::Chain3d;
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a sweep. `section` is a polygon in its own plane frame's `(u, v)`
/// (the frame `make_faces_from_profiles` takes); `path` is in world
/// coordinates and may be open or closed. The pierce rule (section plane ⟂
/// start tangent, start point in the plane) and every corner rule are the
/// kernel's, reported through its typed errors.
pub fn execute_sweep(
    kb: &mut dyn KernelBundle,
    section: &SweepSection,
    path: &Chain3d,
    before_snapshot: Option<&TopoSnapshot>,
) -> Result<OpResult, OpError> {
    if section.outer.len() < 3 {
        return Err(OpError::InvalidParameter {
            reason: format!(
                "sweep section needs at least 3 vertices, got {}",
                section.outer.len()
            ),
        });
    }
    if section
        .outer
        .iter()
        .chain(section.holes.iter().flatten())
        .any(|(u, v)| !(u.is_finite() && v.is_finite()))
    {
        return Err(OpError::InvalidParameter {
            reason: "sweep section has a non-finite vertex".to_string(),
        });
    }
    if path.edges.is_empty() {
        return Err(OpError::InvalidParameter {
            reason: "sweep path has no segments".to_string(),
        });
    }

    let handle = kb.sweep(section, path)?;

    let after = diff::snapshot(kb.as_introspect(), &handle);
    let empty_snap = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let before = before_snapshot.unwrap_or(&empty_snap);
    let diff_result = diff::diff(before, &after);

    let caps = if path.closed {
        None
    } else {
        Some((
            path.edges[0].start_tangent(),
            path.edges[path.edges.len() - 1].end_tangent(),
        ))
    };
    let role_assignments = assign_sweep_roles(kb.as_introspect(), &handle, caps);

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

/// Caps by outward-normal alignment with the end tangents (`Some((t_start,
/// t_end))`: the start cap's outward normal is `−t_start`, the end cap's
/// `+t_end`); `None` for a closed path, which has no caps. Everything else
/// is a side face in kernel order.
fn assign_sweep_roles(
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
    solid: &KernelSolidHandle,
    caps: Option<([f64; 3], [f64; 3])>,
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
    let (start_cap, end_cap) = match caps {
        Some((t_start, t_end)) => {
            let start_cap = best([-t_start[0], -t_start[1], -t_start[2]]);
            let end_cap = best(t_end).filter(|f| Some(*f) != start_cap);
            (start_cap, end_cap)
        }
        None => (None, None),
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use waffle_types::kernel::MockKernel;
    use waffle_types::sketch3d::{Edge3d, Edge3dKind};

    fn square() -> SweepSection {
        SweepSection {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [1.0, 0.0, 0.0],
            plane_x_axis: [0.0, 1.0, 0.0],
            outer: vec![(-0.1, -0.1), (0.1, -0.1), (0.1, 0.1), (-0.1, 0.1)],
            holes: Vec::new(),
        }
    }

    fn line(id: u32, a: [f64; 3], b: [f64; 3]) -> Edge3d {
        Edge3d {
            entity_id: id,
            kind: Edge3dKind::Line,
            a,
            b,
        }
    }

    /// An open L along +x then +z.
    fn open_l() -> Chain3d {
        Chain3d {
            edges: vec![
                line(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
                line(2, [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            ],
            closed: false,
            g1: vec![false],
        }
    }

    /// A closed unit square ring in the xz plane starting along +x.
    fn ring() -> Chain3d {
        Chain3d {
            edges: vec![
                line(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
                line(2, [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
                line(3, [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
                line(4, [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]),
            ],
            closed: true,
            g1: vec![false; 4],
        }
    }

    fn roles(r: &OpResult) -> Vec<Role> {
        r.provenance
            .role_assignments
            .iter()
            .map(|(_, role)| role.clone())
            .collect()
    }

    #[test]
    fn open_path_gets_both_cap_roles_and_side_faces() {
        let mut k = MockKernel::new();
        let r = execute_sweep(&mut k, &square(), &open_l(), None).unwrap();
        assert_eq!(r.outputs.len(), 1);
        assert_eq!(r.outputs[0].0, OutputKey::Main);
        let roles = roles(&r);
        // The mock's body is a unit box; its −x face is the start cap
        // (outward −t_start = −x) and its +z face the end cap.
        assert_eq!(
            roles.iter().filter(|r| **r == Role::EndCapNegative).count(),
            1,
            "{roles:?}"
        );
        assert_eq!(
            roles.iter().filter(|r| **r == Role::EndCapPositive).count(),
            1,
            "{roles:?}"
        );
        assert_eq!(
            roles
                .iter()
                .filter(|r| matches!(r, Role::SideFace { .. }))
                .count(),
            roles.len() - 2
        );
        assert!(!r.provenance.created.is_empty());
    }

    #[test]
    fn closed_path_has_no_caps() {
        let mut k = MockKernel::new();
        let r = execute_sweep(&mut k, &square(), &ring(), None).unwrap();
        let roles = roles(&r);
        assert!(!roles.is_empty());
        assert!(
            roles.iter().all(|r| matches!(r, Role::SideFace { .. })),
            "{roles:?}"
        );
        // Side indices are dense from zero.
        let mut idx: Vec<usize> = roles
            .iter()
            .map(|r| match r {
                Role::SideFace { index } => *index,
                _ => unreachable!(),
            })
            .collect();
        idx.sort_unstable();
        assert_eq!(idx, (0..roles.len()).collect::<Vec<_>>());
    }

    #[test]
    fn argument_refusals_are_typed() {
        let mut k = MockKernel::new();
        let mut thin = square();
        thin.outer.truncate(2);
        assert!(matches!(
            execute_sweep(&mut k, &thin, &open_l(), None),
            Err(OpError::InvalidParameter { .. })
        ));
        let mut nan = square();
        nan.outer[0].0 = f64::NAN;
        assert!(matches!(
            execute_sweep(&mut k, &nan, &open_l(), None),
            Err(OpError::InvalidParameter { .. })
        ));
        let empty = Chain3d {
            edges: Vec::new(),
            closed: false,
            g1: Vec::new(),
        };
        assert!(matches!(
            execute_sweep(&mut k, &square(), &empty, None),
            Err(OpError::InvalidParameter { .. })
        ));
        // A holed section is the kernel's typed capability wall, passed
        // through untouched.
        let mut holed = square();
        holed.holes = vec![vec![
            (-0.05, -0.05),
            (0.05, -0.05),
            (0.05, 0.05),
            (-0.05, 0.05),
        ]];
        assert!(matches!(
            execute_sweep(&mut k, &holed, &open_l(), None),
            Err(OpError::Kernel(
                waffle_types::kernel::KernelError::NotSupported { .. }
            ))
        ));
    }
}

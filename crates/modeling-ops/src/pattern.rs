//! Pattern instancing: copies of seed bodies at a list of placements
//! (spec `specs/custom_features_and_modeling_roadmap.md` §B1).
//!
//! A pattern does NOT re-execute the seed's operations; it copies the seed
//! body through `Kernel::transform_body` (or `Kernel::mirror_body` for a
//! mirror instance), which is exact on analytic geometry. Instance 0 is the seed body itself (identity placement — the
//! seed is NOT copied, its handle is re-emitted so the pattern feature owns
//! every instance); instances 1.. are copies. Any boolean against targets is
//! the feature engine's job (its combine dispatch), not this executor's.
//!
//! Provenance: every copied face and edge is `created`; each instance's
//! faces carry `Role::PatternInstance { index }` so a downstream reference
//! can name "instance 7's k-th face" (`Selector::Role { role, index: k }`,
//! `k` in the kernel's face-list order, which the rigid copy preserves).

use waffle_types::kernel::{KernelSolidHandle, MirrorPlane, RigidPlacement};
use waffle_types::{OutputKey, Role};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// One seed body to instance.
#[derive(Debug, Clone)]
pub struct PatternSeed {
    /// The seed body.
    pub handle: KernelSolidHandle,
}

/// What one instance of a pattern is: the seed itself, a rigid copy, or a
/// mirrored copy. Every pattern's instance 0 is [`Instance::Seed`] — the seed
/// body is re-emitted, never copied, so the feature owns it.
#[derive(Debug, Clone)]
pub enum Instance {
    /// Instance 0: the seed body, re-emitted as it is.
    Seed,
    /// A rigid copy (`Kernel::transform_body`).
    Rigid(RigidPlacement),
    /// A reflected copy (`Kernel::mirror_body`). Improper, so the kernel
    /// reverses the copy's loops — see `MirrorPlane`.
    Mirror(MirrorPlane),
}

/// Instance `seeds` at `instances`. `instances[0]` must be [`Instance::Seed`]
/// (the seed is re-emitted, not copied); every later entry produces one copy
/// per seed. Outputs are ordered instance-major: for instance `i`, the seeds
/// in order — so with S seeds the body for (instance i, seed s) is output
/// `i·S + s`; output 0 is `Main`, the rest `Body { index }`.
///
/// `skip` lists instance indices (≥ 1) to omit; skipped instances produce
/// no body and no output slot (indices of later outputs shift down).
pub fn execute_pattern_instances(
    kb: &mut dyn KernelBundle,
    seeds: &[PatternSeed],
    instances: &[Instance],
    skip: &[u32],
) -> Result<OpResult, OpError> {
    if seeds.is_empty() {
        return Err(OpError::InvalidParameter {
            reason: "pattern has no seed bodies".into(),
        });
    }
    if instances.len() < 2 {
        return Err(OpError::InvalidParameter {
            reason: format!(
                "pattern needs at least 2 instances (the seed plus one copy); got {}",
                instances.len()
            ),
        });
    }
    if !matches!(instances[0], Instance::Seed) {
        return Err(OpError::InvalidParameter {
            reason: "pattern instance 0 must be the seed itself".into(),
        });
    }
    if instances[1..].iter().any(|i| matches!(i, Instance::Seed)) {
        return Err(OpError::InvalidParameter {
            reason: "only instance 0 is the seed; every later instance is a copy".into(),
        });
    }
    for &s in skip {
        if s == 0 {
            return Err(OpError::InvalidParameter {
                reason: "pattern instance 0 is the seed and cannot be skipped".into(),
            });
        }
        if s as usize >= instances.len() {
            return Err(OpError::InvalidParameter {
                reason: format!(
                    "pattern skip index {s} is out of range (instances 1..{})",
                    instances.len() - 1
                ),
            });
        }
    }

    let mut outputs: Vec<(OutputKey, BodyOutput)> = Vec::new();
    let mut role_assignments = Vec::new();
    let mut before = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let mut after = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };

    for (i, instance) in instances.iter().enumerate() {
        if skip.contains(&(i as u32)) {
            continue;
        }
        for seed in seeds {
            let handle = match instance {
                Instance::Seed => {
                    let snap = diff::snapshot(kb.as_introspect(), &seed.handle);
                    before.faces.extend(snap.faces.clone());
                    before.edges.extend(snap.edges.clone());
                    before.vertices.extend(snap.vertices.clone());
                    after.faces.extend(snap.faces);
                    after.edges.extend(snap.edges);
                    after.vertices.extend(snap.vertices);
                    seed.handle.clone()
                }
                Instance::Rigid(placement) => {
                    let copy = kb.transform_body(&seed.handle, placement)?;
                    let snap = diff::snapshot(kb.as_introspect(), &copy);
                    after.faces.extend(snap.faces);
                    after.edges.extend(snap.edges);
                    after.vertices.extend(snap.vertices);
                    copy
                }
                Instance::Mirror(plane) => {
                    let copy = kb.mirror_body(&seed.handle, plane)?;
                    let snap = diff::snapshot(kb.as_introspect(), &copy);
                    after.faces.extend(snap.faces);
                    after.edges.extend(snap.edges);
                    after.vertices.extend(snap.vertices);
                    copy
                }
            };
            for face in kb.as_introspect().list_faces(&handle) {
                role_assignments.push((face, Role::PatternInstance { index: i }));
            }
            let key = if outputs.is_empty() {
                OutputKey::Main
            } else {
                OutputKey::Body {
                    index: outputs.len(),
                }
            };
            outputs.push((
                key,
                BodyOutput {
                    handle,
                    mesh: None,
                    edges: None,
                },
            ));
        }
    }

    let d = diff::diff(&before, &after);
    Ok(OpResult {
        outputs,
        provenance: Provenance {
            created: d.created,
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments,
        },
        diagnostics: Diagnostics::default(),
    })
}

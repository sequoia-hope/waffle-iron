//! Stage 0 oracle — `CoplanarMeshIdenticalOracle` (Yang 2025 §4.5.5).
//!
//! ## Contract
//!
//! Yang 2025 §4.5.5 (p. 1281–1292):
//!
//! > "When conducting Boolean operations on B-Rep models, the coplanarity
//! > between the surfaces of the two models is a commonly encountered
//! > degenerative case. As our discretization method does not maintain
//! > coplanarity in triangle meshes because of floating-point error
//! > introduced in discretization, it may lead to incorrect Boolean
//! > operation results. Therefore, it is necessary to check coplanar
//! > planes and perform 2D Boolean operations before mesh discretizations.
//! > [...] The overlapping part is replaced by a trimmed common planar
//! > surface, and identical meshes are generated for both models in this
//! > part."
//!
//! Operational definition: for each detected coplanar overlap region, the
//! triangles emitted by Stage 0's mesh injection
//! (`coplanar_preprocess::inject_identical_footprint_mesh` and
//! `inject_partial_overlap_mesh`) must be byte-identical between operand A
//! and operand B over the overlap region. Identity is checked at f32
//! precision because that is the precision retained in the `RenderMesh`
//! that downstream stages consume; any divergence at f32 is a downstream
//! conformality hazard.
//!
//! ## Audit anchors
//!
//! - YA-15 (Auditor A, `docs/audits/yang_audit_a_yang_pipeline.md`):
//!   coplanar B-Rep splitting is pre-tess but mesh injection is post-tess
//!   — a hybrid that admits divergence the canonical Yang pipeline avoids
//!   by construction.
//! - YA-26 (Auditor A): the d_epsilon retessellation refinement loop in
//!   `yang_integration.rs` re-tessellates without re-running coplanar
//!   injection. Any retry path that reaches Stage 1 with a fresh
//!   tessellation but no fresh injection violates the contract this
//!   oracle checks.
//!
//! ## Snapshot semantics
//!
//! The oracle reads `PipelineState::stage_0_coplanar`
//! (`CoplanarPreprocessSnapshot`). The snapshot's `mesh_a`/`mesh_b` fields
//! are intended to carry the AFTER-injection tessellation: callers that
//! want to verify Yang §4.5.5 must populate them with the meshes produced
//! by `inject_identical_footprint_mesh` / `inject_partial_overlap_mesh`,
//! not the raw `tessellate_waffle_solid` output.
//!
//! ## Coverage
//!
//! - `is_identical_footprint == true`: PRESENT — multiset of plane
//!   triangles in mesh_a vs mesh_b checked under canonical-form keys
//!   (sorted f32 vertex bits, order-independent so the winding flip for
//!   anti-parallel pairs doesn't perturb identity).
//! - `is_partial_overlap == true && is_identical_footprint == false`:
//!   STUB — restricting candidate triangles to the overlap region
//!   requires re-running i_overlay's Intersect/Difference; that is in
//!   the production-pipeline scope, not the diagnostic oracle's. The
//!   oracle reports `ViolationKind::OracleStub` so the corpus runner
//!   can distinguish "passing" from "not yet checked".

use std::collections::BTreeMap;

use crate::boolean::pipeline_oracles::{
    OracleViolation, PipelineState, StageOracle, ViolationKind, YangStage,
};
use crate::types::RenderMesh;

/// Stage 0 oracle: byte-identical mesh emission over coplanar overlap
/// region (Yang §4.5.5).
pub(crate) struct CoplanarMeshIdenticalOracle;

impl StageOracle for CoplanarMeshIdenticalOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage0Coplanar
    }
    fn name(&self) -> &'static str {
        "CoplanarMeshIdenticalOracle"
    }
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let snap = match state.stage_0_coplanar.as_ref() {
            Some(s) => s,
            None => return Ok(()), // Skip when not snapshotted.
        };

        // Pairs empty → no Stage 0 contract to enforce.
        if snap.pairs.is_empty() {
            return Ok(());
        }

        // Pairs present but meshes missing → snapshot incomplete; the
        // oracle's contract requires the post-injection meshes to
        // verify byte-identity.
        let (mesh_a, mesh_b) = match (snap.mesh_a.as_ref(), snap.mesh_b.as_ref()) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "{} coplanar pair(s) detected but mesh_a/mesh_b not snapshotted; \
                         oracle requires both meshes to verify §4.5.5 identity",
                        snap.pairs.len(),
                    ),
                    kind: ViolationKind::StateMissing,
                });
            }
        };

        // Walk pairs in registration order (CoplanarFacePair list is a
        // Vec, so iteration is deterministic). For each
        // identical-footprint pair, compare plane-coincident triangle
        // multisets between A and B.
        let mut stub_pairs: Vec<usize> = Vec::new();
        for (idx, pair) in snap.pairs.iter().enumerate() {
            if pair.is_identical_footprint {
                let normal = [
                    pair.plane_normal[0] as f32,
                    pair.plane_normal[1] as f32,
                    pair.plane_normal[2] as f32,
                ];
                let offset = pair.plane_offset as f32;
                let tris_on_plane_a = collect_plane_triangle_keys(mesh_a, &normal, offset);
                let tris_on_plane_b = collect_plane_triangle_keys(mesh_b, &normal, offset);
                if tris_on_plane_a != tris_on_plane_b {
                    let (n_a, n_b) = (
                        tris_on_plane_a.values().sum::<usize>(),
                        tris_on_plane_b.values().sum::<usize>(),
                    );
                    return Err(OracleViolation {
                        stage: self.stage(),
                        oracle_name: self.name(),
                        message: format!(
                            "pair {idx} (face_a={:?}, face_b={:?}, identical-footprint): \
                             plane-triangle multisets differ — A has {n_a} tri(s) on plane, \
                             B has {n_b} tri(s); Yang §4.5.5 requires byte-identical emission",
                            pair.face_a, pair.face_b,
                        ),
                        kind: ViolationKind::ContractViolated,
                    });
                }
                continue;
            }
            if pair.is_partial_overlap {
                stub_pairs.push(idx);
            }
        }

        if !stub_pairs.is_empty() {
            return Err(OracleViolation {
                stage: self.stage(),
                oracle_name: self.name(),
                message: format!(
                    "{} partial-overlap pair(s) not checked: indices {:?}; \
                     overlap-region restriction not yet implemented (requires re-running \
                     i_overlay Intersect to identify the canonical region)",
                    stub_pairs.len(),
                    stub_pairs,
                ),
                kind: ViolationKind::OracleStub,
            });
        }

        Ok(())
    }
}

// ── Plane-triangle key extraction ───────────────────────────────────────

/// Canonical key for a triangle: the three vertex positions in f32 bits,
/// sorted so the key is winding- and rotation-independent. Returning a
/// `[[u32; 3]; 3]` keeps each vertex's components grouped while the
/// outer-array sort orders the three vertices.
type TriKey = [[u32; 3]; 3];

/// Build a `BTreeMap<TriKey, count>` of all triangles in `mesh` whose
/// three vertex positions all lie on the plane `(normal, offset)`. Plane
/// distance is computed in f32 to mirror the precision the renderer
/// actually consumes.
///
/// `BTreeMap` (not `HashMap`) so the multiset equality check has
/// deterministic iteration order — required by team-feedback memory
/// `feedback_no_regression_chasing.md` (BTreeMap when iteration order
/// matters).
fn collect_plane_triangle_keys(
    mesh: &RenderMesh,
    normal: &[f32; 3],
    offset: f32,
) -> BTreeMap<TriKey, usize> {
    // f32 plane-distance tolerance: be generous because `RenderMesh`
    // positions are downcast from f64. 1e-4 m is two orders of
    // magnitude tighter than the assay corpus's smallest feature
    // (MIN_FEATURE_SIZE = 1e-6 mostly via dedup at 1e-9). Past that,
    // we accept any vertex on the plane regardless of round-off.
    //
    // The BYTE-IDENTITY check is via the triangle key, not the plane
    // tolerance — the tolerance only filters which triangles are
    // CANDIDATES for being on the coplanar plane. Once they are
    // candidates, their vertex-bit-equality is what's compared.
    const PLANE_TOL: f32 = 1.0e-4;

    let mut out: BTreeMap<TriKey, usize> = BTreeMap::new();
    let n_indices = mesh.indices.len();
    if !n_indices.is_multiple_of(3) {
        return out; // Malformed mesh — return empty.
    }
    let n_tris = n_indices / 3;
    for t in 0..n_tris {
        let i0 = mesh.indices[t * 3] as usize;
        let i1 = mesh.indices[t * 3 + 1] as usize;
        let i2 = mesh.indices[t * 3 + 2] as usize;
        let p0 = match read_vertex(mesh, i0) {
            Some(p) => p,
            None => continue,
        };
        let p1 = match read_vertex(mesh, i1) {
            Some(p) => p,
            None => continue,
        };
        let p2 = match read_vertex(mesh, i2) {
            Some(p) => p,
            None => continue,
        };
        if !on_plane(&p0, normal, offset, PLANE_TOL)
            || !on_plane(&p1, normal, offset, PLANE_TOL)
            || !on_plane(&p2, normal, offset, PLANE_TOL)
        {
            continue;
        }
        let key = canonical_tri_key(&p0, &p1, &p2);
        *out.entry(key).or_insert(0) += 1;
    }
    out
}

/// Read vertex `i` from the mesh's flat `vertices: Vec<f32>` array.
/// Returns `None` if `i` is out of range.
fn read_vertex(mesh: &RenderMesh, i: usize) -> Option<[f32; 3]> {
    if i * 3 + 2 >= mesh.vertices.len() {
        return None;
    }
    Some([
        mesh.vertices[i * 3],
        mesh.vertices[i * 3 + 1],
        mesh.vertices[i * 3 + 2],
    ])
}

#[inline]
fn on_plane(p: &[f32; 3], normal: &[f32; 3], offset: f32, tol: f32) -> bool {
    let dot = p[0] * normal[0] + p[1] * normal[1] + p[2] * normal[2];
    (dot - offset).abs() <= tol
}

/// Canonical-form triangle key — the three vertex bit-triples sorted in
/// ascending order so winding and rotation don't perturb identity.
#[inline]
fn canonical_tri_key(p0: &[f32; 3], p1: &[f32; 3], p2: &[f32; 3]) -> TriKey {
    let mut keys: [[u32; 3]; 3] = [pos_key_f32(p0), pos_key_f32(p1), pos_key_f32(p2)];
    keys.sort();
    keys
}

#[inline]
fn pos_key_f32(p: &[f32; 3]) -> [u32; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::coplanar_preprocess::CoplanarFacePair;
    use crate::boolean::pipeline_oracles::CoplanarPreprocessSnapshot;
    use crate::topology::half_edge::FaceIdx;
    use crate::types::{FaceRange, KernelId, RenderMesh};

    /// Build a minimal `RenderMesh` from a flat list of `[x, y, z]`
    /// vertices and a flat list of `[i0, i1, i2]` triangles. `face_ranges`
    /// is left empty since the oracle does not consume it (it works
    /// purely from geometry).
    fn make_mesh(verts: &[[f32; 3]], tris: &[[u32; 3]]) -> RenderMesh {
        let mut vertices = Vec::with_capacity(verts.len() * 3);
        for v in verts {
            vertices.extend_from_slice(v);
        }
        let normals = vec![0.0; vertices.len()];
        let mut indices = Vec::with_capacity(tris.len() * 3);
        for t in tris {
            indices.extend_from_slice(t);
        }
        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges: vec![FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: tris.len() as u32 * 3,
            }],
        }
    }

    /// Build a `CoplanarFacePair` for the z=0 plane with both flags as
    /// requested.
    fn pair_z0(identical: bool, partial: bool, same_direction: bool) -> CoplanarFacePair {
        CoplanarFacePair {
            face_a: FaceIdx(0),
            face_b: FaceIdx(1),
            plane_normal: [0.0, 0.0, 1.0],
            plane_offset: 0.0,
            same_direction,
            is_identical_footprint: identical,
            is_partial_overlap: partial,
        }
    }

    // ── KNOWN-PASS: identical-footprint, byte-equal triangles ───────────

    #[test]
    fn identical_footprint_byte_equal_triangles_pass() {
        // Two triangles on z=0 in mesh A, byte-identical (modulo winding
        // direction) in mesh B. Canonical key sorts vertex bit-triples,
        // so the (0,1,2) and (0,2,1) windings collide as expected.
        let verts_a = [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let tris_a = [[0_u32, 1, 2], [1, 3, 2]];
        let mesh_a = make_mesh(&verts_a, &tris_a);

        // Same vertex positions, opposite winding (anti-parallel pair).
        let tris_b = [[0_u32, 2, 1], [1, 2, 3]];
        let mesh_b = make_mesh(&verts_a, &tris_b);

        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, false, false)],
            mesh_a: Some(mesh_a),
            mesh_b: Some(mesh_b),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        let oracle = CoplanarMeshIdenticalOracle;
        let verdict = oracle.check(&state);
        assert!(verdict.is_ok(), "expected Ok, got {verdict:?}");
    }

    // ── KNOWN-FAIL: identical-footprint, B missing a triangle ───────────

    #[test]
    fn identical_footprint_missing_triangle_in_b_fails() {
        // Mesh A has both triangles on z=0; mesh B has only one.
        // Multiset comparison must reject.
        let verts = [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let mesh_a = make_mesh(&verts, &[[0, 1, 2], [1, 3, 2]]);
        let mesh_b = make_mesh(&verts, &[[0, 1, 2]]);
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, false, true)],
            mesh_a: Some(mesh_a),
            mesh_b: Some(mesh_b),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        let oracle = CoplanarMeshIdenticalOracle;
        let violation = oracle.check(&state).expect_err("expected violation");
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.stage, YangStage::Stage0Coplanar);
        assert!(
            violation.message.contains("multiset") || violation.message.contains("differ"),
            "unexpected message: {}",
            violation.message
        );
    }

    // ── KNOWN-FAIL: identical-footprint, B has shifted vertex ───────────

    #[test]
    fn identical_footprint_shifted_vertex_in_b_fails() {
        // Mesh A and B have a triangle each on z=0, but a single vertex
        // bit-shifted (different f32 bits) → canonical keys diverge.
        let verts_a = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Bit-perturb the y-component of vertex 2: 1.0 → next-up-from-1.0.
        let one_perturbed = f32::from_bits(1.0_f32.to_bits() + 1);
        let verts_b = [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, one_perturbed, 0.0],
        ];
        let mesh_a = make_mesh(&verts_a, &[[0, 1, 2]]);
        let mesh_b = make_mesh(&verts_b, &[[0, 1, 2]]);
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, false, true)],
            mesh_a: Some(mesh_a),
            mesh_b: Some(mesh_b),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        let verdict = CoplanarMeshIdenticalOracle.check(&state);
        let violation = verdict.expect_err("expected violation");
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
    }

    // ── StateMissing: pairs present but meshes None ─────────────────────

    #[test]
    fn pairs_present_but_meshes_missing_reports_state_missing() {
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, false, true)],
            mesh_a: None,
            mesh_b: None,
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        let violation = CoplanarMeshIdenticalOracle
            .check(&state)
            .expect_err("expected violation");
        assert_eq!(violation.kind, ViolationKind::StateMissing);
        assert!(
            violation.message.contains("not snapshotted"),
            "unexpected: {}",
            violation.message
        );
    }

    // ── Skip: snapshot None ─────────────────────────────────────────────

    #[test]
    fn snapshot_none_self_skips() {
        // No Stage 0 snapshot at all → oracle silently passes.
        let state = PipelineState::empty();
        assert!(CoplanarMeshIdenticalOracle.check(&state).is_ok());
    }

    // ── Skip: pairs empty ───────────────────────────────────────────────

    #[test]
    fn empty_pairs_passes() {
        // Snapshot present but no coplanar pairs detected → no contract
        // to enforce.
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![],
            mesh_a: None,
            mesh_b: None,
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        assert!(CoplanarMeshIdenticalOracle.check(&state).is_ok());
    }

    // ── Stub: partial-overlap not yet checked ───────────────────────────

    #[test]
    fn partial_overlap_only_pair_reports_oracle_stub() {
        // Pair flagged is_partial_overlap (and not is_identical_footprint)
        // → oracle reports OracleStub so corpus runner distinguishes
        // "passing" from "not yet checked".
        let mesh = make_mesh(&[[0.0_f32, 0.0, 0.0]], &[]);
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(false, true, false)],
            mesh_a: Some(mesh.clone()),
            mesh_b: Some(mesh),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        let violation = CoplanarMeshIdenticalOracle
            .check(&state)
            .expect_err("expected stub");
        assert_eq!(violation.kind, ViolationKind::OracleStub);
        assert!(
            violation.message.contains("partial-overlap"),
            "unexpected: {}",
            violation.message
        );
    }

    // ── Identity-footprint ALSO partial-overlap: identical takes priority ─

    #[test]
    fn pair_with_both_flags_uses_identical_path() {
        // Per `inject_partial_overlap_mesh` L1067, identical-footprint
        // takes precedence over partial-overlap when both are set. The
        // oracle mirrors that: this pair is checked under the
        // identical-footprint contract, so byte-equal triangles pass.
        let verts = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mesh_a = make_mesh(&verts, &[[0, 1, 2]]);
        let mesh_b = make_mesh(&verts, &[[0, 1, 2]]);
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, true, true)],
            mesh_a: Some(mesh_a),
            mesh_b: Some(mesh_b),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        assert!(CoplanarMeshIdenticalOracle.check(&state).is_ok());
    }

    // ── Off-plane triangles are filtered out ────────────────────────────

    #[test]
    fn triangles_off_plane_are_ignored() {
        // Mesh A has one triangle on z=0 plus one off-plane triangle;
        // mesh B has the same on-plane triangle and a DIFFERENT off-plane
        // triangle. The oracle should only compare on-plane triangles
        // and pass.
        let verts_a = [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 5.0], // off-plane (z=5)
            [1.0, 0.0, 5.0],
            [0.0, 1.0, 5.0],
        ];
        let tris_a = [[0, 1, 2], [3, 4, 5]];
        let verts_b = [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 7.0], // different off-plane vertex
            [3.0, 0.0, 7.0],
            [2.0, 1.0, 7.0],
        ];
        let tris_b = [[0, 1, 2], [3, 4, 5]];
        let mesh_a = make_mesh(&verts_a, &tris_a);
        let mesh_b = make_mesh(&verts_b, &tris_b);
        let snap = CoplanarPreprocessSnapshot {
            pairs: vec![pair_z0(true, false, true)],
            mesh_a: Some(mesh_a),
            mesh_b: Some(mesh_b),
        };
        let mut state = PipelineState::empty();
        state.stage_0_coplanar = Some(snap);
        assert!(CoplanarMeshIdenticalOracle.check(&state).is_ok());
    }
}

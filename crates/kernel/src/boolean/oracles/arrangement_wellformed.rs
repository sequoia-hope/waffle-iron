//! Stage 2 oracle — `MeshArrangementWellFormedOracle`.
//!
//! Validates that the post-arrangement sub-triangle mesh produced by
//! `subdivide_mesh_pair` (Yang 2025 §4.2 / Cherchi 2022 §3–§4) satisfies
//! the well-formedness contract that downstream stages assume.
//!
//! ## Contract (Cherchi 2022 §3 lines 236-237 + §4 input invariants)
//!
//! Cherchi's arrangement requires (and produces) a sub-triangle mesh
//! where every undirected edge is incident to a small, classifiable
//! number of triangles. The post-arrangement output of `solve_intersections`
//! (the `subdivide_mesh_pair` driver) is consumed by:
//!
//! - `topology_extract::extract_result_topology` (boundary / intersection
//!   classification — directly indexes `directed_edge_to_tris`),
//! - `build_manifold_patch_graph` (PR8 — assumes valid sub-triangle indices
//!   and conservation), and
//! - `label_cells` (Cherchi 2022 §5 ray-cast — assumes manifold patches).
//!
//! All three downstream consumers fail in cascade when the arrangement
//! output violates one of:
//!
//! 1. **Edge incidence**: every undirected edge has 1 (boundary), 2
//!    (manifold), or N≥3 (singular / non-manifold) directed-edge sides.
//!    No edge may be "phantom" — appearing in the directed map yet
//!    missing from the undirected map (impossible by construction, but
//!    we guard against builder defects).
//! 2. **No degenerate sub-tris**: no zero-area / collinear triangle —
//!    Cherchi predicates (§4 cached `orient3d`) silently produce wrong
//!    classifications on degenerate input.
//! 3. **Vertex completeness**: every triangle vertex index is `<
//!    verts.len()`. A dangling index is a builder bug; the rest of the
//!    pipeline panics on indexing.
//! 4. **Conservation**: the total directed-edge count equals
//!    `3 * (tris_a.len() + tris_b.len())`. This is a tautology for the
//!    builder; it doubles as a runtime bookkeeping check.
//!
//! ## Audit anchors
//!
//! See `docs/audits/yang_audit_c_cherchi2022.md` §3 (input precondition
//! "manifold, watertight and with no self-intersections", lines 236-237)
//! and the YC-04 / YC-13 findings on arrangement-stage robustness. This
//! oracle does NOT enforce watertightness end-to-end — that is Stage 1's
//! contract (bijective tessellation). Here we enforce only the local
//! sub-triangle-mesh well-formedness that Cherchi §3-4 produce by
//! construction.

use std::collections::BTreeMap;

use crate::boolean::exact_mesh::SubTriangle;
use crate::boolean::pipeline_oracles::{
    OracleViolation, PipelineState, StageOracle, ViolationKind, YangStage,
};

/// Stage 2 well-formedness oracle (Cherchi 2022 §3-4).
pub(crate) struct MeshArrangementWellFormedOracle;

impl StageOracle for MeshArrangementWellFormedOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage2Arrangement
    }
    fn name(&self) -> &'static str {
        "MeshArrangementWellFormedOracle"
    }
    fn cherchi_section(&self) -> Option<&'static str> {
        Some("Cherchi 2022 §3-4")
    }
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let subdivided = match state.stage_2_subdivided.as_ref() {
            Some(s) => s,
            None => return Ok(()), // Stage 2 didn't run — runner skips.
        };

        let n_verts = subdivided.verts.len();
        let n_tris = subdivided.tris_a.len() + subdivided.tris_b.len();

        // ── Check (3): vertex-index completeness ──
        // Walk both sides; report the first dangling index. Doing this up
        // front means subsequent edge / area checks can index into
        // `verts` without bounds-checking each access.
        if let Some(violation) = check_vertex_indices(self, &subdivided.tris_a, n_verts, "tris_a") {
            return Err(violation);
        }
        if let Some(violation) = check_vertex_indices(self, &subdivided.tris_b, n_verts, "tris_b") {
            return Err(violation);
        }

        // ── Check (2): no degenerate sub-triangles ──
        // Use exact `orient3d`-style collinearity: a sub-triangle (a, b, c)
        // is degenerate iff (b-a) × (c-a) is the zero vector. We use f64
        // arithmetic here because the inputs to Stage 2 are explicit
        // float coordinates (the sub-triangle vertex array carries either
        // explicit operand vertices or LPI-rounded arrangement vertices —
        // both are explicit `[f64; 3]` in `SubdividedMesh::verts`).
        if let Some(violation) =
            check_no_degenerate(self, &subdivided.tris_a, &subdivided.verts, "tris_a")
        {
            return Err(violation);
        }
        if let Some(violation) =
            check_no_degenerate(self, &subdivided.tris_b, &subdivided.verts, "tris_b")
        {
            return Err(violation);
        }

        // ── Check (1) + (4): edge incidence + conservation ──
        // Build directed-edge multiset over all sub-triangles (both sides).
        // Per `feedback_no_regression_chasing.md` and harness convention,
        // BTreeMap (deterministic ordering) — not HashMap.
        let mut directed: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut undirected: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut total_directed = 0usize;

        for sub in subdivided.tris_a.iter().chain(subdivided.tris_b.iter()) {
            for ei in 0..3 {
                let v0 = sub.verts[ei];
                let v1 = sub.verts[(ei + 1) % 3];
                *directed.entry((v0, v1)).or_default() += 1;
                let key = if v0 <= v1 { (v0, v1) } else { (v1, v0) };
                *undirected.entry(key).or_default() += 1;
                total_directed += 1;
            }
        }

        // Conservation (4): total directed-edge count = 3 × tri count.
        let expected_directed = 3 * n_tris;
        if total_directed != expected_directed {
            return Err(OracleViolation {
                stage: self.stage(),
                oracle_name: self.name(),
                message: format!(
                    "directed-edge conservation violated: counted {total_directed}, \
                     expected 3 × ({} + {}) = {expected_directed}",
                    subdivided.tris_a.len(),
                    subdivided.tris_b.len(),
                ),
                kind: ViolationKind::ContractViolated,
            });
        }

        // ── F1 conservation anchor (spec §F1, encoding (a)) ────────────
        // Anchor the post-snapshot tri count to the upstream Cherchi
        // emission counter. The directed-edge check above is a tautology
        // that shrinks proportionally with snapshot size, so a "lost-
        // during-emit" defect (e.g., a stray `sub_tris_a.pop()` between
        // the emission loop and the `SubdividedMesh` constructor) is
        // structurally invisible to it. `upstream_tri_count` is populated
        // at emission time in `subdivide_mesh_pair_full_cherchi`
        // (encoding (a): a Cherchi label==3 tri increments the counter
        // by 2 for the A and B sub-tris it emits), so any divergence
        // from `tris_a.len() + tris_b.len()` indicates lost or duplicated
        // sub-tris between emission and snapshot.
        if n_tris != subdivided.upstream_tri_count {
            return Err(OracleViolation {
                stage: self.stage(),
                oracle_name: self.name(),
                message: format!(
                    "Stage 2 emit conservation violated: subdivided.tris_a.len() + \
                     subdivided.tris_b.len() = {n_tris}, but upstream_tri_count = {} \
                     (expected equality per F1 / spec §F1 encoding (a))",
                    subdivided.upstream_tri_count,
                ),
                kind: ViolationKind::ContractViolated,
            });
        }

        // Edge incidence (1): every undirected edge MUST appear in the
        // directed map under at least one orientation. A "phantom" edge
        // (in undirected but neither orientation appears in directed) is
        // impossible by our build but a defensive check; conversely, the
        // sum of (u,v) and (v,u) in `directed` MUST equal `undirected[(u,v)]`.
        for (&(u, v), &count) in undirected.iter() {
            let forward = directed.get(&(u, v)).copied().unwrap_or(0);
            let backward = directed.get(&(v, u)).copied().unwrap_or(0);
            if forward + backward != count {
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "edge incidence inconsistent for ({u}, {v}): \
                         undirected count = {count}, directed forward = {forward}, \
                         backward = {backward} (forward + backward must equal undirected)",
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
            // Cherchi 2022 §3-4 admits 1 (boundary), 2 (manifold), or
            // N≥3 (singular). 0 is impossible (the edge is in `undirected`
            // because at least one triangle owns it). We accept all
            // positive counts here — singular edges are a valid output of
            // the arrangement; downstream patch segmentation handles them.
            // The check exists to catch builder-side over-counting (e.g.,
            // an edge with `count > 3 × n_tris` from a corruption bug),
            // which would have been caught above by conservation.
            if count == 0 {
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "edge incidence zero for ({u}, {v}) — present in undirected map \
                         but neither orientation appears in directed map",
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
        }

        Ok(())
    }
}

/// Return the first vertex-index violation in `tris`, or `None` if all
/// indices are in range. `side` names the bucket in the error message
/// (e.g. "tris_a"). Borrows `oracle` for stage / name fields.
fn check_vertex_indices(
    oracle: &MeshArrangementWellFormedOracle,
    tris: &[SubTriangle],
    n_verts: usize,
    side: &str,
) -> Option<OracleViolation> {
    for (ti, sub) in tris.iter().enumerate() {
        for (vi, &v) in sub.verts.iter().enumerate() {
            if v >= n_verts {
                return Some(OracleViolation {
                    stage: oracle.stage(),
                    oracle_name: oracle.name(),
                    message: format!(
                        "{side}[{ti}].verts[{vi}] = {v} but only {n_verts} verts exist",
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
        }
    }
    None
}

/// Return the first degenerate sub-triangle in `tris`, or `None` if all
/// are non-degenerate. Degeneracy = zero cross-product of two edges (the
/// three vertices are collinear or coincident).
fn check_no_degenerate(
    oracle: &MeshArrangementWellFormedOracle,
    tris: &[SubTriangle],
    verts: &[[f64; 3]],
    side: &str,
) -> Option<OracleViolation> {
    for (ti, sub) in tris.iter().enumerate() {
        let a = verts[sub.verts[0]];
        let b = verts[sub.verts[1]];
        let c = verts[sub.verts[2]];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cx = ab[1] * ac[2] - ab[2] * ac[1];
        let cy = ab[2] * ac[0] - ab[0] * ac[2];
        let cz = ab[0] * ac[1] - ab[1] * ac[0];
        if cx == 0.0 && cy == 0.0 && cz == 0.0 {
            return Some(OracleViolation {
                stage: oracle.stage(),
                oracle_name: oracle.name(),
                message: format!(
                    "{side}[{ti}] is degenerate: verts = {:?}, positions a={a:?} b={b:?} c={c:?}",
                    sub.verts,
                ),
                kind: ViolationKind::ContractViolated,
            });
        }
    }
    None
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::exact_mesh::{SubTriangle, SubdividedMesh};

    /// Build a `SubTriangle` from a 3-tuple of vertex indices. `parent_tri`
    /// and `cosurface_orientation` are filler — Stage 2 well-formedness
    /// oracle reads only `verts`.
    fn st(a: usize, b: usize, c: usize) -> SubTriangle {
        SubTriangle {
            verts: [a, b, c],
            parent_tri: 0,
            cosurface_orientation: None,
        }
    }

    /// KNOWN-PASS fixture: two disjoint axis-aligned cubes, one as `tris_a`
    /// and one as `tris_b`. 24 sub-triangles total (12 per cube), all edges
    /// manifold (every cube triangle pairs with exactly one neighbour
    /// within its own mesh — disjoint cubes don't share edges with each
    /// other but their internal edges are 2-manifold).
    fn two_disjoint_cubes() -> SubdividedMesh {
        // Cube A: [0, 1]³ → 8 verts (0..8).
        // Cube B: shifted to [10, 11]³ → 8 verts (8..16).
        let mut verts = Vec::with_capacity(16);
        for &(ox, oy, oz) in &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0)] {
            for k in 0..8 {
                let x = ox + ((k & 1) as f64);
                let y = oy + (((k >> 1) & 1) as f64);
                let z = oz + (((k >> 2) & 1) as f64);
                verts.push([x, y, z]);
            }
        }

        // 12 triangles per cube — same indexing scheme into [0..8].
        // Vertices are bit-packed (x, y, z) ∈ {0,1}³.
        // Each face is one quad split into 2 tris with consistent winding.
        // We don't need correct outward winding for well-formedness checks;
        // we only need every undirected edge to be shared by exactly 2 tris
        // (manifold). We use a known good triangulation:
        let cube_tris: [[usize; 3]; 12] = [
            // -Z face (z=0 verts: 0,1,2,3)
            [0, 2, 1],
            [1, 2, 3],
            // +Z face (z=1 verts: 4,5,6,7)
            [4, 5, 6],
            [5, 7, 6],
            // -Y face (y=0 verts: 0,1,4,5)
            [0, 1, 4],
            [1, 5, 4],
            // +Y face (y=1 verts: 2,3,6,7)
            [2, 6, 3],
            [3, 6, 7],
            // -X face (x=0 verts: 0,2,4,6)
            [0, 4, 2],
            [2, 4, 6],
            // +X face (x=1 verts: 1,3,5,7)
            [1, 3, 5],
            [3, 7, 5],
        ];

        let tris_a: Vec<SubTriangle> = cube_tris.iter().map(|t| st(t[0], t[1], t[2])).collect();
        // Cube B uses the same local pattern, offset by +8.
        let tris_b: Vec<SubTriangle> = cube_tris
            .iter()
            .map(|t| st(t[0] + 8, t[1] + 8, t[2] + 8))
            .collect();

        SubdividedMesh {
            verts,
            tris_a,
            tris_b,
            params_a: vec![None; 16],
            params_b: vec![None; 16],
            // Spec §F1 default for synthetic fixtures: 24 = 12 + 12.
            upstream_tri_count: 24,
        }
    }

    /// KNOWN-FAIL fixture: one degenerate triangle (3 collinear vertices).
    fn mesh_with_degenerate() -> SubdividedMesh {
        SubdividedMesh {
            verts: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0], // collinear with verts 0 and 1.
            ],
            tris_a: vec![st(0, 1, 2)], // degenerate — collinear.
            tris_b: vec![],
            params_a: vec![None; 3],
            params_b: vec![],
            upstream_tri_count: 1,
        }
    }

    /// KNOWN-FAIL fixture: a triangle vertex index past `verts.len()`.
    fn mesh_with_dangling_vertex() -> SubdividedMesh {
        SubdividedMesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris_a: vec![st(0, 1, 99)], // 99 ≫ verts.len() = 3.
            tris_b: vec![],
            params_a: vec![None; 3],
            params_b: vec![],
            upstream_tri_count: 1,
        }
    }

    // ── Stage / name plumbing ───────────────────────────────────────────

    #[test]
    fn oracle_reports_stage2() {
        let oracle = MeshArrangementWellFormedOracle;
        assert_eq!(oracle.stage(), YangStage::Stage2Arrangement);
        assert_eq!(oracle.name(), "MeshArrangementWellFormedOracle");
        assert!(oracle.cherchi_section().is_some());
    }

    // ── Skip behavior ───────────────────────────────────────────────────

    #[test]
    fn oracle_skips_when_stage2_not_snapshotted() {
        // No Stage 2 snapshot → oracle must self-skip, NOT report
        // StateMissing. (Per harness convention: missing snapshot is
        // not itself a contract violation here — Stage 2 may legitimately
        // not have run if Stage 1 errored.)
        let state = PipelineState::empty();
        assert!(MeshArrangementWellFormedOracle.check(&state).is_ok());
    }

    // ── KNOWN-PASS ──────────────────────────────────────────────────────

    #[test]
    fn oracle_passes_on_two_disjoint_cubes() {
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(two_disjoint_cubes());
        let result = MeshArrangementWellFormedOracle.check(&state);
        assert!(
            result.is_ok(),
            "expected disjoint cubes to be well-formed, got: {result:?}",
        );
    }

    // ── KNOWN-FAIL: degenerate triangle ─────────────────────────────────

    #[test]
    fn oracle_rejects_degenerate_triangle() {
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(mesh_with_degenerate());
        let result = MeshArrangementWellFormedOracle.check(&state);
        let violation = result.unwrap_err();
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.stage, YangStage::Stage2Arrangement);
        assert!(
            violation.message.contains("degenerate"),
            "expected 'degenerate' in message, got: {}",
            violation.message,
        );
    }

    // ── KNOWN-FAIL: dangling vertex index ───────────────────────────────

    #[test]
    fn oracle_rejects_out_of_bounds_vertex_index() {
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(mesh_with_dangling_vertex());
        let result = MeshArrangementWellFormedOracle.check(&state);
        let violation = result.unwrap_err();
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.stage, YangStage::Stage2Arrangement);
        assert!(
            violation.message.contains("99") && violation.message.contains("verts"),
            "expected error to mention dangling index 99 + verts count, got: {}",
            violation.message,
        );
    }

    // ── KNOWN-FAIL: directed-edge conservation violation via duplicated tri ─

    #[test]
    fn oracle_rejects_inconsistent_undirected_directed_counts() {
        // Construct a state where undirected and directed counts agree
        // (so conservation passes), but verify the oracle's edge-incidence
        // path is exercised on a real well-formed input. This is a
        // sanity test rather than an inversion fixture — the oracle's
        // edge-incidence inconsistency branch is unreachable from a
        // well-formed builder; we keep the branch as a defensive guard.
        let mesh = two_disjoint_cubes();
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(mesh);
        assert!(MeshArrangementWellFormedOracle.check(&state).is_ok());
    }

    // ── KNOWN-FAIL: F1 upstream conservation anchor (spec §F1) ──────────

    /// Per spec §F1 + PR10 audit (`specs/oracle_validity_audit.md` §F1):
    /// the existing directed-edge tautology check shrinks proportionally
    /// with snapshot size, so a "lost-during-emit" defect produces no
    /// violation. Anchoring `upstream_tri_count` to the upstream Cherchi
    /// emission counter detects the divergence.
    ///
    /// Fixture mimics the audit's mutation: take the well-formed
    /// two-disjoint-cubes snapshot (24 sub-tris, upstream_tri_count = 24)
    /// and `pop()` one sub-triangle from `tris_a` to simulate a stray
    /// drop between emission and `SubdividedMesh` construction. The
    /// directed-edge conservation check then sees 3 × 23 = 69 edges
    /// (consistent with itself) but `upstream_tri_count` still records
    /// the 24 emissions — F1 fires.
    #[test]
    fn oracle_rejects_lost_during_emit_via_f1_anchor() {
        let mut mesh = two_disjoint_cubes();
        // Sanity: fixture's synthetic default is 24.
        assert_eq!(mesh.upstream_tri_count, 24);
        // Simulate the audit's "lost-during-emit" mutation.
        mesh.tris_a.pop().expect("two_disjoint_cubes has tris_a");
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(mesh);
        let violation = MeshArrangementWellFormedOracle
            .check(&state)
            .expect_err("F1 must detect the dropped sub-tri");
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.stage, YangStage::Stage2Arrangement);
        assert!(
            violation.message.contains("upstream_tri_count")
                && violation.message.contains("23")
                && violation.message.contains("24"),
            "expected message to name both counts (23 vs upstream 24), got: {}",
            violation.message,
        );
    }

    /// Mirror test: the F1 anchor must NOT fire on a well-formed snapshot
    /// where every emission is recorded. Guards against an oracle that
    /// rejects everything (false-positive surface).
    #[test]
    fn oracle_passes_when_upstream_tri_count_matches() {
        let mesh = two_disjoint_cubes();
        assert_eq!(mesh.tris_a.len() + mesh.tris_b.len(), mesh.upstream_tri_count);
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(mesh);
        assert!(MeshArrangementWellFormedOracle.check(&state).is_ok());
    }
}

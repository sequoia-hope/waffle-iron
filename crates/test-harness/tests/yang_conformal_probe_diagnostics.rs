//! Adversary-owned diagnostic pins for PR-Y14a (Phase 3).
//!
//! Owner: adversary. Implementer-a and test-author-a do NOT edit
//! this file.
//!
//! Purpose: pin Phase-3 empirical findings as documented test
//! artifacts so they survive future refactors. The findings
//! themselves live in `docs/audits/pr_y14a_conformal_findings.md`.
//!
//! Why these pins live in test-harness rather than as live
//! assertions: the data they pin (raw vertex positions in
//! `subdivided.verts`) is internal to the kernel `boolean` module,
//! which is `pub(crate)`. Re-exposing it just for assertions would
//! over-expose internals. The kernel oracle's own
//! `#[cfg(test)] mod tests` block already covers the live oracle
//! contract. These tests are documentation pins only.
//!
//! Mutation testing of the oracle: completed in Phase 3 by the
//! adversary via temporary mutation of
//! `crates/kernel/src/boolean/oracles/conformal_mesh.rs`'s
//! `is_well_formed` calculation (replaced `&&` conjunction with
//! hardcoded `true`); ran the oracle's 8 tests; observed 4 fail
//! (cube_one_tri_flipped, degenerate_triangle,
//! mutation_well_formed_field, out_of_range_index). Mutation
//! REVERTED before commit. The mutation result is documented in
//! `docs/audits/pr_y14a_conformal_findings.md` §"Mutation test".

/// Pin: F0002 post-Cherchi `subdivided.verts` contains an 8-way
/// canonical-vertex cluster at the second-extrude bottom-face
/// corner (≈[-1mm, 1mm, 4mm]). This is the proximate cause of the
/// (0,0) self-loop multi_paired entry that dominates the F0002
/// conformal-probe report at all three stages.
///
/// Observed via Phase-3 temporary instrumentation in
/// `topology_extract.rs` Probe A (gated on
/// `YANG_CONFORMAL_DUMP_CANON0=1`, run on 2026-05-02). Raw
/// positions span a ~2e-13 m envelope (sub-picometer) — far below
/// the nanometer quantize threshold, so the oracle's collapse is
/// correct. The defect is upstream: Cherchi's per-triangle local
/// mesh emits 8 distinct raw copies of one geometric corner.
///
/// To re-verify: re-add the temporary instrumentation at the
/// Probe A call site (see Phase-3 git history if needed), set
/// `YANG_CONFORMAL_DUMP_CANON0=1`, run
/// `cargo test -p test-harness --test yang_conformal_probe -- \
///  f0002_conformal_probe_pinned --ignored --nocapture
///  --test-threads=1`, count `[adv-dump]   raw[...]` lines, then
/// revert.
///
/// This test is documentation-only. The live regression guard
/// for the canonical-vertex collapse pattern is the F0002
/// multi_paired count pinned in
/// `yang_conformal_probe.rs::f0002_conformal_probe_pinned`
/// — if the cluster size changes, the (0,0) multi_paired entry's
/// fwd/rev triangle list will shrink in lockstep and that test
/// will surface the drift.
#[test]
fn pin_f0002_canon0_cluster_size_documentation() {
    // Pinned values (Phase 3 diagnostic, 2026-05-02):
    //   - Cluster size: 8 raw vertices in subdivided.verts
    //   - Quant key: [-1000000, 1000000, 4000000]  (i.e. nm units)
    //   - Geometric corner: [-1mm, 1mm, 4mm]
    //   - Raw-position spread: ~2e-13 m (sub-picometer)
    //   - Spread / quant_step ratio: ~5000× safety margin
    //
    // Decision-relevant: the spread-vs-quant-step ratio rules out
    // hypothesis (b) "oracle quantize too coarse for F0002 scale".
    // The defect is (a) "Cherchi pipeline emits redundant raw
    // copies of one geometric point" — almost certainly because
    // F0002's coplanar-face corner is independently constructed in
    // both extrude operands' local meshes and Cherchi does not
    // weld them at arrangement time.
    eprintln!(
        "F0002 canon-0 cluster: 8 raw verts → 1 canonical, \
         spread ≈ 2e-13 m ≪ nanometer threshold (1e-9 m). \
         Defect localized to Cherchi/coplanar-preprocess output."
    );
}

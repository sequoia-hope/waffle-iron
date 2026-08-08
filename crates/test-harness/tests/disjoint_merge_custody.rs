//! Regression: the consumption-custody invariant for legacy auto-union
//! (2026-08-08, the R0090/R0030 base-drop —
//! `docs/audits/volume_oracle_flags_anchored.md`).
//!
//! Three `merge=true` extrudes on far-apart planes. Op2 leaves op1's tool as a
//! standalone leftover (`Body{1}` of E2, spec'd disjoint-merge behavior). Op3
//! then targets "the most recent solid": before the fix that collected only
//! E2's MAIN output while consuming E2 WHOLE — deleting the leftover base body
//! without any boolean touching it (live volume dropped by exactly the first
//! prism). The fix collects ALL of E2's live bodies as fold lumps, so
//! consumption takes custody of everything it hides.

use test_harness::helpers::mesh_signed_volume;
use test_harness::ModelBuilder;

#[test]
fn third_disjoint_merge_extrude_conserves_all_bodies() {
    let mut b = ModelBuilder::kernel_v2();

    // Three far-apart unit cubes, each a merge=true extrude (legacy auto path).
    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E1", "S1", 1.0).unwrap();

    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E2", "S2", 1.0).unwrap();

    b.rect_sketch("S3", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], -5.0, -5.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E3", "S3", 1.0).unwrap();

    let meshes = b.tessellate_live_with_tol(1e-3).unwrap();
    let total: f64 = meshes.iter().map(mesh_signed_volume).sum();

    // Custody: every prism survives. Before the fix this was 2 bodies /
    // volume 2.0 — E1's cube deleted by E3's consumption of E2.
    assert_eq!(
        meshes.len(),
        3,
        "three disjoint merge extrudes must yield three live bodies, got {} \
         (a lost body means consumption took custody of bodies it never held)",
        meshes.len()
    );
    assert!(
        (total - 3.0).abs() < 1e-6,
        "total live volume must be 3 unit cubes, got {total}"
    );
}

/// The two-op shape stays byte-identical (spec'd disjoint-merge behavior,
/// `disjoint_merge_bodies.rs`) — the fix only changes N-body custody.
#[test]
fn second_disjoint_merge_extrude_unchanged() {
    let mut b = ModelBuilder::kernel_v2();

    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E1", "S1", 1.0).unwrap();

    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E2", "S2", 1.0).unwrap();

    let meshes = b.tessellate_live_with_tol(1e-3).unwrap();
    let total: f64 = meshes.iter().map(mesh_signed_volume).sum();
    assert_eq!(meshes.len(), 2);
    assert!((total - 2.0).abs() < 1e-6, "got {total}");
}

/// A tool that TOUCHES one of the two disjoint lumps merges with exactly that
/// lump and re-emits the other — custody without over-merging.
#[test]
fn third_extrude_merges_touched_lump_and_keeps_other() {
    let mut b = ModelBuilder::kernel_v2();

    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E1", "S1", 1.0).unwrap();

    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E2", "S2", 1.0).unwrap();

    // Overlaps E1's cube (centered on it, half-embedded), far from E2's.
    b.rect_sketch("S3", [0.0, 0.0, 0.5], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E3", "S3", 1.0).unwrap();

    let meshes = b.tessellate_live_with_tol(1e-3).unwrap();
    let total: f64 = meshes.iter().map(mesh_signed_volume).sum();
    // E1 ∪ E3 = 1.0 + 1.0 − 0.5 overlap = 1.5; E2 = 1.0.
    assert_eq!(
        meshes.len(),
        2,
        "tool must fuse with the lump it touches and keep the other, got {}",
        meshes.len()
    );
    assert!(
        (total - 2.5).abs() < 1e-6,
        "expected 1.5 (merged) + 1.0 (kept), got {total}"
    );
}

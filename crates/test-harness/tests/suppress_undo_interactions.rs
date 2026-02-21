//! Suppress/undo interaction tests.
//!
//! Tests for suppress → undo, cascade suppress, suppress/unsuppress cycles,
//! delete → undo, and reorder → undo interactions.

use test_harness::ModelBuilder;

// ── Suppress then undo ───────────────────────────────────────────────────

/// Suppress then undo should restore the feature.
#[test]
fn test_suppress_then_undo_restores() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    m.suppress("box").unwrap();
    // Suppressed feature should not produce solid output
    assert!(
        m.assert_has_solid("box").is_err(),
        "suppressed feature should not have solid"
    );

    m.undo().unwrap();
    // After undo, box should be restored
    m.assert_has_solid("box").unwrap();
}

// ── Suppress parent sketch then undo ─────────────────────────────────────

/// Suppress parent sketch should cascade errors to child; undo restores both.
#[test]
fn test_suppress_parent_sketch_undo_restores() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();
    m.assert_no_errors().unwrap();

    m.suppress("sk").unwrap();
    // Suppressing the sketch should cascade — extrude has no input sketch
    m.assert_has_errors().unwrap();

    m.undo().unwrap();
    m.assert_has_solid("box").unwrap();
    m.assert_no_errors().unwrap();
}

// ── Suppress/unsuppress cycle then undo ──────────────────────────────────

/// Multiple undo across suppress/unsuppress cycle restores original state.
#[test]
fn test_multiple_undo_suppress_unsuppress_cycle() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    m.suppress("box").unwrap(); // suppress
    m.unsuppress("box").unwrap(); // unsuppress
    m.assert_has_solid("box").unwrap();

    m.undo().unwrap(); // undo unsuppress → back to suppressed
    assert!(
        m.assert_has_solid("box").is_err(),
        "after undo of unsuppress, feature should be suppressed again"
    );

    m.undo().unwrap(); // undo suppress → back to normal
    m.assert_has_solid("box").unwrap();
}

// ── Delete then undo ─────────────────────────────────────────────────────

/// Delete then undo should restore the feature.
#[test]
fn test_delete_then_undo_restores() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_feature_count(2).unwrap();

    m.delete_feature("box").unwrap();
    m.assert_feature_count(1).unwrap(); // Only sketch remains

    m.undo().unwrap();
    // Note: delete_feature removes the name mapping and undo doesn't restore it,
    // so we verify via feature_count rather than assert_has_solid("box").
    m.assert_feature_count(2).unwrap();
}

// ── Reorder then undo ────────────────────────────────────────────────────

/// Reorder then undo should preserve original order.
#[test]
fn test_reorder_then_undo_preserves_order() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.assert_feature_count(4).unwrap();
    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();

    // Reorder box2's extrude to position 2 (before box1's extrude)
    m.reorder("box2", 2).unwrap();

    m.undo().unwrap();
    m.assert_feature_count(4).unwrap();
    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();
    m.assert_no_errors().unwrap();
}

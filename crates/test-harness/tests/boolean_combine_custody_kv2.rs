//! Real-geometry custody of a `BooleanCombine`'s unnamed bodies — the assay
//! rows P0010 and P0011 (2026-10-03, `docs/yang_tail_triage.md`).
//!
//! The op names ONE output per operand (`body_a`/`body_b` carry an
//! `OutputKey`) but consumes both operand FEATURES whole, so a multi-output
//! operand lost every body the boolean never named: no error, no warning, each
//! lost body watertight. The live volume DROPPED across a union, which is
//! arithmetically impossible.
//!
//! The two shapes the corpus rows carry, with the two ways a feature comes to
//! hold several bodies:
//!   * P0010 — the operand is a `merge=true` extrude whose tool was disjoint
//!     from its target, so it re-emitted the target as `Body{1}`.
//!   * P0011 — the operand is itself a `BooleanCombine` of two DISJOINT bodies,
//!     so the kernel's own boolean returned two shells.
//!
//! `feature-engine/tests/boolean_combine_custody.rs` pins the bookkeeping
//! invariant; these pin that real bodies and real volume survive.

use test_harness::helpers::mesh_signed_volume;
use test_harness::ModelBuilder;

/// Three far-apart unit cubes as three `merge=false` extrudes.
fn three_cubes() -> ModelBuilder {
    let mut b = ModelBuilder::kernel_v2();
    for (i, (x, y)) in [(0.0, 0.0), (5.0, 5.0), (-5.0, -5.0)].iter().enumerate() {
        let s = format!("S{i}");
        let e = format!("E{i}");
        b.rect_sketch(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], *x, *y, 1.0, 1.0)
            .unwrap();
        b.extrude_no_merge(&e, &s, 1.0).unwrap();
    }
    b
}

fn live_total(b: &mut ModelBuilder) -> (usize, f64) {
    let meshes = b.tessellate_live_with_tol(1e-3).unwrap();
    let total = meshes.iter().map(mesh_signed_volume).sum();
    (meshes.len(), total)
}

/// P0010's shape: union two of three disjoint cubes where the first operand's
/// feature carries a SECOND body (a disjoint `merge=true` leftover). Before the
/// fix the leftover vanished and the live volume fell from 3 to 2 unit cubes.
#[test]
fn union_of_a_disjoint_merge_leftover_keeps_the_base() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E1", "S1", 1.0).unwrap();
    // merge=true but disjoint ⇒ E2 holds Main (its own prism) + Body{1} (E1's).
    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E2", "S2", 1.0).unwrap();
    assert_eq!(b.op_result("E2").unwrap().outputs.len(), 2, "E2 setup");
    b.rect_sketch("S3", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], -5.0, -5.0, 1.0, 1.0)
        .unwrap();
    b.extrude_no_merge("E3", "S3", 1.0).unwrap();
    assert_eq!(live_total(&mut b), (3, 3.0), "setup: three unit cubes");

    // Union E2's Main with E3 — E2's Body{1} is in nobody's operand list.
    b.boolean_union("U", "E2", "E3").unwrap();
    assert!(b.engine_errors().is_empty(), "{:?}", b.engine_errors());

    let (n, total) = live_total(&mut b);
    assert_eq!(
        n, 3,
        "the two disjoint union operands plus the carried leftover (got {n})"
    );
    assert!(
        (total - 3.0).abs() < 1e-6,
        "a union cannot reduce total volume: got {total}, want 3 unit cubes"
    );
    assert!(
        b.engine_warnings()
            .iter()
            .any(|w| w.contains("not targeted")),
        "carrying a body the boolean never named is reported: {:?}",
        b.engine_warnings()
    );
}

/// P0011's shape: the first union of two DISJOINT cubes yields a two-body
/// feature; the second union names only its `Main`. Before the fix the first
/// union's second body was dropped (P0011 lost 3468.21 of 14146.78).
#[test]
fn chained_union_keeps_the_first_unions_second_body() {
    let mut b = three_cubes();
    assert_eq!(live_total(&mut b), (3, 3.0), "setup: three unit cubes");

    b.boolean_union("U1", "E0", "E1").unwrap();
    assert!(b.engine_errors().is_empty(), "{:?}", b.engine_errors());
    assert_eq!(
        b.op_result("U1").unwrap().outputs.len(),
        2,
        "a union of two disjoint bodies yields two shells"
    );
    assert_eq!(
        live_total(&mut b),
        (3, 3.0),
        "the first union conserves all 3"
    );

    b.boolean_union("U2", "U1", "E2").unwrap();
    assert!(b.engine_errors().is_empty(), "{:?}", b.engine_errors());
    let (n, total) = live_total(&mut b);
    assert_eq!(n, 3, "the chained union must not drop U1's second body");
    assert!(
        (total - 3.0).abs() < 1e-6,
        "a chained union cannot reduce total volume: got {total}, want 3"
    );
}

/// The invariant over the whole family: for every pair of the three disjoint
/// cubes, and for union and subtract, the live set is
/// `(inputs − named operands) ∪ result` and no live material is hidden.
/// A disjoint Subtract leaves its target untouched.
///
/// INVARIANCE pin, not a RED→GREEN one: every operand here is single-output,
/// which is exactly the shape the pre-fix code handled correctly. It guards the
/// custody fix from over-carrying (a body counted twice) as the two pins above
/// guard it from under-carrying.
#[test]
fn boolean_output_set_conserves_the_unnamed_bodies() {
    for (a, c) in [("E0", "E1"), ("E0", "E2"), ("E1", "E2")] {
        // Union of a disjoint pair: both operands survive, plus the third cube.
        let mut b = three_cubes();
        b.boolean_union("U", a, c).unwrap();
        assert!(
            b.engine_errors().is_empty(),
            "{a}∪{c}: {:?}",
            b.engine_errors()
        );
        let (n, total) = live_total(&mut b);
        assert_eq!((n, (total * 1e6).round()), (3, 3e6), "{a} ∪ {c}");

        // Subtract of a disjoint pair: the tool removes nothing, so the target
        // survives whole and the untouched third cube with it.
        let mut b = three_cubes();
        b.boolean_subtract("D", a, c).unwrap();
        assert!(
            b.engine_errors().is_empty(),
            "{a}−{c}: {:?}",
            b.engine_errors()
        );
        let (n, total) = live_total(&mut b);
        assert_eq!(
            (n, (total * 1e6).round()),
            (2, 2e6),
            "{a} − {c}: the target and the unnamed third cube"
        );
    }
}

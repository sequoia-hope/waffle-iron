//! Spec `kv2_multishell_boolean_operands`: a multi-shell solid whose shells
//! are spatially DISJOINT LUMPS re-enters `boolean_op` as an operand (the
//! KV7-F2 disjoint-lump slice). Shell clusters with nested/overlapping AABBs
//! (potential voids) keep the typed `UnsupportedMultiShellBoolean` wall.
//!
//! All fixtures are axis-aligned boxes → exact analytic volumes (1e-9).
use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::*;

fn make_box(arena: &mut BrepArena, lo: [f64; 3], hi: [f64; 3]) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, lo[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(lo[0], lo[1]),
            Point2::new(hi[0], lo[1]),
            Point2::new(hi[0], hi[1]),
            Point2::new(lo[0], hi[1]),
        ],
        vec![],
    )
    .unwrap();
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), hi[2] - lo[2])
        .unwrap()
        .solid
}

/// Two-lump operand: unit cube at the origin + unit cube at (5,5,0). The
/// disjoint union is the production path that mints multi-shell solids.
fn two_lump_body(arena: &mut BrepArena) -> SolidId {
    let a = make_box(arena, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = make_box(arena, [5.0, 5.0, 0.0], [6.0, 6.0, 1.0]);
    let s = boolean_op(arena, a, b, BoolOp::Union).unwrap();
    assert_eq!(
        arena.solid(s).unwrap().shells.len(),
        2,
        "fixture: disjoint union must be a 2-shell solid"
    );
    s
}

fn volume(arena: &BrepArena, s: SolidId) -> f64 {
    geom::signed_volume(arena, s).unwrap()
}

#[test]
fn union_bridges_one_lump_of_two_lump_operand() {
    let mut arena = BrepArena::new();
    let a = two_lump_body(&mut arena);
    // Tool overlaps lump 1 only: x ∈ [0.5, 1.5] → merged lump vol 1.5.
    let tool = make_box(&mut arena, [0.5, 0.0, 0.0], [1.5, 1.0, 1.0]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Union).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 2.5).abs() < 1e-9,
        "union volume: expected 2.5 (1.5 merged + 1.0 far lump), got {v}"
    );
    let bodies = split_solid_into_bodies(&mut arena, out).unwrap();
    assert_eq!(bodies.len(), 2, "bridged lump + far lump = 2 bodies");
}

#[test]
fn subtract_cuts_one_lump_of_two_lump_operand() {
    let mut arena = BrepArena::new();
    let a = two_lump_body(&mut arena);
    // Tool eats the x ∈ [0.5, 1] half of lump 1; lump 2 untouched.
    let tool = make_box(&mut arena, [0.5, -0.1, -0.1], [1.1, 1.1, 1.1]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Subtract).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 1.5).abs() < 1e-9,
        "subtract volume: expected 1.5 (0.5 cut lump + 1.0 far lump), got {v}"
    );
}

#[test]
fn subtract_consuming_one_lump_keeps_the_other() {
    let mut arena = BrepArena::new();
    let a = two_lump_body(&mut arena);
    // Tool engulfs lump 2 entirely.
    let tool = make_box(&mut arena, [4.9, 4.9, -0.1], [6.1, 6.1, 1.1]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Subtract).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 1.0).abs() < 1e-9,
        "subtract volume: expected 1.0 (lump 1 survives alone), got {v}"
    );
    let bodies = split_solid_into_bodies(&mut arena, out).unwrap();
    assert_eq!(bodies.len(), 1, "surviving lump is a single body");
}

#[test]
fn intersect_selects_one_lump_of_two_lump_operand() {
    let mut arena = BrepArena::new();
    let a = two_lump_body(&mut arena);
    // Tool overlaps the x ∈ [0, 0.5] slice of lump 1 only.
    let tool = make_box(&mut arena, [-0.1, -0.1, -0.1], [0.5, 1.1, 1.1]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Intersect).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 0.5).abs() < 1e-9,
        "intersect volume: expected 0.5 (lump 1 slice), got {v}"
    );
}

#[test]
fn both_operands_multi_lump_union() {
    let mut arena = BrepArena::new();
    let a = two_lump_body(&mut arena);
    // Second 2-lump body, each lump overlapping the corresponding A lump:
    // near lumps merge to 1.5, far lumps merge to 1.5 → total 3.0.
    let c = make_box(&mut arena, [0.5, 0.0, 0.0], [1.5, 1.0, 1.0]);
    let d = make_box(&mut arena, [5.5, 5.0, 0.0], [6.5, 6.0, 1.0]);
    let b = boolean_op(&mut arena, c, d, BoolOp::Union).unwrap();
    assert_eq!(arena.solid(b).unwrap().shells.len(), 2);
    let out = boolean_op(&mut arena, a, b, BoolOp::Union).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 3.0).abs() < 1e-9,
        "union volume: expected 3.0 (two 1.5 merged lumps), got {v}"
    );
    let bodies = split_solid_into_bodies(&mut arena, out).unwrap();
    assert_eq!(bodies.len(), 2, "two merged lumps stay two bodies");
}

// =========================================================================
// Amendment 1: VOID operands (nested shells from a fully-enclosed subtract)
// re-enter booleans too — the Cherchi 2022 parity in/out labeling is
// component- and cavity-agnostic (spec §Amendment 1).
// =========================================================================

/// Voided operand: box [0,4]³ minus interior box [1,3]³ → outer shell +
/// cavity shell, volume 64 − 8 = 56.
fn voided_box(arena: &mut BrepArena) -> SolidId {
    let outer = make_box(arena, [0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
    let inner = make_box(arena, [1.0, 1.0, 1.0], [3.0, 3.0, 3.0]);
    let s = boolean_op(arena, outer, inner, BoolOp::Subtract).unwrap();
    assert_eq!(
        arena.solid(s).unwrap().shells.len(),
        2,
        "fixture: fully-enclosed subtract must yield outer + void shells"
    );
    let v = geom::signed_volume(arena, s).unwrap();
    assert!((v - 56.0).abs() < 1e-9, "fixture volume 56, got {v}");
    s
}

#[test]
fn union_with_voided_operand_preserves_void() {
    let mut arena = BrepArena::new();
    let a = voided_box(&mut arena);
    // Side-overlapping box adds the x ∈ [4, 5] slab (1·16 = 16).
    let tool = make_box(&mut arena, [3.5, 0.0, 0.0], [5.0, 4.0, 4.0]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Union).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 72.0).abs() < 1e-9,
        "union volume: expected 72 (56 + 24 − 8 overlap), got {v}"
    );
}

#[test]
fn subtract_corner_from_voided_operand_keeps_void() {
    let mut arena = BrepArena::new();
    let a = voided_box(&mut arena);
    // Corner cut [0, 0.9]³ is solid material, clear of the cavity.
    let tool = make_box(&mut arena, [-0.5, -0.5, -0.5], [0.9, 0.9, 0.9]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Subtract).unwrap();
    let v = volume(&arena, out);
    let expect = 56.0 - 0.9_f64.powi(3);
    assert!(
        (v - expect).abs() < 1e-9,
        "subtract volume: expected {expect}, got {v}"
    );
}

#[test]
fn subtract_tunnel_opening_the_cavity() {
    let mut arena = BrepArena::new();
    let a = voided_box(&mut arena);
    // x-through tunnel 0.6×0.6 pierces both outer walls AND the cavity:
    // removed material = (4 − 2)·0.36 = 0.72; the void merges into the
    // tunnel (void topology correctly destroyed).
    let tool = make_box(&mut arena, [-0.5, 1.7, 1.7], [4.5, 2.3, 2.3]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Subtract).unwrap();
    let v = volume(&arena, out);
    let expect = 56.0 - 0.72;
    assert!(
        (v - expect).abs() < 1e-9,
        "subtract volume: expected {expect}, got {v}"
    );
}

#[test]
fn intersect_slab_straddling_the_cavity_wall() {
    let mut arena = BrepArena::new();
    let a = voided_box(&mut arena);
    // Slab x ∈ [2.5, 4.5] ∩ solid = 1.5·16 − cavity slice 0.5·4 = 22.
    let tool = make_box(&mut arena, [2.5, -0.5, -0.5], [4.5, 4.5, 4.5]);
    let out = boolean_op(&mut arena, a, tool, BoolOp::Intersect).unwrap();
    let v = volume(&arena, out);
    assert!(
        (v - 22.0).abs() < 1e-9,
        "intersect volume: expected 22, got {v}"
    );
}

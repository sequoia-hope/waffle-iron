//! The body splitter keeps a cavity with the lump that encloses it — for a
//! CURVED lump too (P0041's shape, 2026-10-12).
//!
//! `split_solid_into_bodies` clusters a solid's shells by axis-aligned box
//! overlap: a void's box nests inside its lump's, far-apart lumps' boxes do
//! not meet. The box used to be the hull of the shell's B-Rep VERTICES, and a
//! plain cylinder has exactly two of those, both on its seam — its "box" was
//! a line segment on the lateral. A square cavity sunk into a cylinder boss
//! (an engine cut whose direction-less sweep ran INTO the body, from a plane
//! 10 below the top cap) therefore clustered with nothing, the splitter
//! returned it as a second "body", and the next cut — a wider prism sharing
//! the cavity's floor and ceiling planes — ran as two subtracts, one against
//! the inside-out cavity shell alone, whose output kept the cavity's floor and
//! ceiling squares with their edges used once:
//! `InvalidBooleanOutput("an undirected output edge is not used by exactly
//! two directed edges")`.
//!
//! The splitter now clusters by the shell's CONSERVATIVE box
//! (`introspect::conservative_shell_aabb` — the bound of the surface, the one
//! `conservative_aabb` already computes per solid), so the void stays with
//! its boss, the second cut sees the 2-shell operand, and yang's pipeline
//! opens the cavity into the cut (its in/out labelling is cavity-agnostic,
//! spec `kv2_multishell_boolean_operands` Amendment 1).
//!
//! RED before the fix: the first test's split returned 2 bodies; the second
//! test is the P0041 chain and STOPped with the pairing text above.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, geom, split_solid_into_bodies, validate_solid, BrepArena, Profile, SolidId,
};

/// A +x plane's sketch frame as the engine derives it: u = ẑ × x̂ = ŷ,
/// v = x̂ × ŷ = ẑ; a CCW polygon in (u, v) extrudes along +x.
fn plus_x_frame() -> (Vector3, Vector3) {
    (Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0))
}

/// The boss: a circle of radius 110 about the x axis, x ∈ [0, 210].
fn boss(a: &mut BrepArena) -> SolidId {
    let (u, v) = plus_x_frame();
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        u,
        v,
        Point2::new(0.0, 0.0),
        110.0,
    )
    .expect("boss profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), 210.0)
        .expect("boss extrude")
        .solid
}

/// A right prism over the plane x = `x0` with (y, z) polygon `pts` (CCW),
/// x ∈ [x0, x0 + depth].
fn prism(a: &mut BrepArena, x0: f64, pts: &[(f64, f64)], depth: f64) -> SolidId {
    let (u, v) = plus_x_frame();
    let p = Profile::new(
        Point3::new(x0, 0.0, 0.0),
        u,
        v,
        pts.iter().map(|&(y, z)| Point2::new(y, z)).collect(),
        vec![],
    )
    .expect("prism profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), depth)
        .expect("prism extrude")
        .solid
}

/// P0041's first cut: a square of half-diagonal 40 about (y, z) = (6, 70),
/// swept x ∈ [100, 200] — wholly inside the boss, so the subtract mints a
/// cavity.
fn cavity_tool(a: &mut BrepArena) -> SolidId {
    let sq = [
        (6.0 + 21.61209223472559, 70.0 + 33.65883939231586),
        (6.0 - 33.65883939231586, 70.0 + 21.61209223472559),
        (6.0 - 21.61209223472559, 70.0 - 33.658839392315855),
        (6.0 + 33.658839392315855, 70.0 - 21.612092234725594),
    ];
    prism(a, 100.0, &sq, 100.0)
}

/// P0041's second cut: a diamond of half-diagonal 200 about (y, z) =
/// (10, 80), swept x ∈ [100, 200] — the cavity's own x-slab, so its start
/// cap lies IN the cavity's floor plane and its end cap in the ceiling
/// plane, and it reaches past the boss's lateral on three sides.
fn opening_tool(a: &mut BrepArena) -> SolidId {
    let dia = [(210.0, 80.0), (10.0, 280.0), (-190.0, 80.0), (10.0, -120.0)];
    prism(a, 100.0, &dia, 100.0)
}

#[test]
fn a_cavity_in_a_cylinder_boss_is_not_a_body_of_its_own() {
    let mut a = BrepArena::new();
    let b = boss(&mut a);
    let t = cavity_tool(&mut a);
    let voided = boolean_op(&mut a, b, t, BoolOp::Subtract).expect("cavity subtract");
    assert_eq!(
        a.solid(voided).unwrap().shells.len(),
        2,
        "fixture: a wholly enclosed subtract yields the boss shell + the cavity shell"
    );
    let bodies = split_solid_into_bodies(&mut a, voided).expect("split");
    assert_eq!(
        bodies.len(),
        1,
        "the cavity's box nests inside the boss's box: ONE body"
    );
    assert_eq!(a.solid(bodies[0]).unwrap().shells.len(), 2);
}

#[test]
fn a_cut_sharing_the_cavity_floor_and_ceiling_planes_opens_the_cavity() {
    let mut a = BrepArena::new();
    let b = boss(&mut a);
    let t = cavity_tool(&mut a);
    let voided = boolean_op(&mut a, b, t, BoolOp::Subtract).expect("cavity subtract");
    let voided = split_solid_into_bodies(&mut a, voided).expect("split")[0];
    let d = opening_tool(&mut a);
    // P0041's STOP, verbatim, before the fix.
    let out = boolean_op(&mut a, voided, d, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("opening subtract failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(
        report.shells, 1,
        "the cavity opened into the cut: one shell"
    );
    // Exact: π·110²·210 − 100·area(diamond ∩ disc); the area by a 6000-column
    // midpoint integration of the exact per-column overlap, 33078.129.
    let v = geom::signed_volume(&a, out).expect("volume");
    let expect = std::f64::consts::PI * 110.0 * 110.0 * 210.0 - 100.0 * 33_078.129_138_198_68;
    assert!(
        ((v - expect) / expect).abs() < 5.0e-4,
        "volume {v} vs reference {expect}"
    );
}

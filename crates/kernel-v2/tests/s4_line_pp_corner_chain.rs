//! Stage-4 line × plane-pair CORNER and generator PIERCE junctions on a
//! chained boolean (R0070's shape at unit scale; spec
//! `yang_stage4_conic_triple_junction.md`, "Junction-map candidates — the
//! line × plane-pair corner" and "Junction-line amendment — the line-curve
//! carriers", 2026-09-17).
//!
//! A z-cylinder boss gets a tilted slot cut along x (the slot's flank
//! planes contain x and are NOT perpendicular to z), then a cylinder is
//! drilled along x into the slot's back wall so that its flat BOTTOM plane
//! (x = X_BOTTOM) crosses the boss lateral. Bottom ∩ lateral is a pair of
//! GENERATORS (the bottom plane is parallel to the boss axis); each
//! generator
//! - ENDS where it meets a slot flank: {lateral, flank, bottom} — a Line
//!   endpoint that also terminates the flank∩bottom plane∩plane segment.
//!   Before the fix that vertex fell through the triple block (the
//!   plane∩plane map counted zero toward `n_maps`) to the Line arm's
//!   perpendicular FOOT, which lands on the generator but OFF the tilted
//!   flank by the foot's along-line error (R0070 op 3: 8.5e-7 at 1.7e-2
//!   scale; here ≈ sagitta × 0.15) and Stage 6 STOPped
//!   `s6-planar-loop-nonplanar`. RED without the `pp_line_corner` candidate
//!   (mutation-checked 2026-09-17), GREEN with it;
//! - EXITS the hole where it pierces the hole's lateral: {lateral, bottom,
//!   hole lateral} — the line-curve junction whose displacement is bounded
//!   by the LINE corridor `2·d_ε/|L̂·n_hole|` (the R0070 v88 class), not the
//!   surface-pair corridor.
//!
//! Every one of those junctions is an exact output vertex.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

/// Boss: z-cylinder, radius 2, z ∈ [0, 2.5].
const R_BOSS: f64 = 2.0;
/// Slot (extruded along +x from x = 0.8 to 3): y ∈ [−0.7, 0.7], flanks
/// z = 0.905 + 0.15·y (bottom) and z = 1.505 + 0.15·y (top).
const SLOT_HALF_W: f64 = 0.7;
const SLOPE: f64 = 0.15;
const Z_LO_AT_MINUS_W: f64 = 0.8;
const Z_HI_AT_MINUS_W: f64 = 1.4;
/// Hole: x-cylinder, radius 0.6, axis through (y, z) = (0, 1.3), from the
/// bottom plane x = 1.95 out through the boss to x = 3.
const X_BOTTOM: f64 = 1.95;
const HOLE_R: f64 = 0.6;
const HOLE_Z: f64 = 1.3;

fn z_lo(y: f64) -> f64 {
    Z_LO_AT_MINUS_W + SLOPE * (y + SLOT_HALF_W)
}
fn z_hi(y: f64) -> f64 {
    Z_HI_AT_MINUS_W + SLOPE * (y + SLOT_HALF_W)
}
fn in_slot(y: f64, z: f64) -> bool {
    y.abs() <= SLOT_HALF_W && z >= z_lo(y) && z <= z_hi(y)
}

fn boss(a: &mut BrepArena) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        R_BOSS,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), 2.5)
        .unwrap()
        .solid
}

/// The slot cutter: a parallelogram in the (y, z) plane extruded along +x.
fn slot(a: &mut BrepArena) -> kernel_v2::SolidId {
    let w = SLOT_HALF_W;
    let p = Profile::new(
        Point3::new(0.8, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(-w, z_lo(-w)),
            Point2::new(w, z_lo(w)),
            Point2::new(w, z_hi(w)),
            Point2::new(-w, z_hi(-w)),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), 3.0 - 0.8)
        .unwrap()
        .solid
}

/// The hole cutter: a circle in the plane x = X_BOTTOM extruded along +x.
fn hole(a: &mut BrepArena) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(X_BOTTOM, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, HOLE_Z),
        HOLE_R,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), 3.0 - X_BOTTOM)
        .unwrap()
        .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

fn has_vertex(mesh: &RenderMesh, c: [f64; 3]) -> bool {
    mesh.positions.chunks_exact(3).any(|p| {
        ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt() <= 1e-9
    })
}

/// The scoop the hole removes from the slotted boss: over the hole's disc,
/// the x-extent between the bottom plane and the boss lateral, outside the
/// slot (midpoint rule, 600²).
fn analytic_scoop() -> f64 {
    let n = 600;
    let h = 2.0 * HOLE_R / n as f64;
    let mut v = 0.0;
    for i in 0..n {
        for j in 0..n {
            let y = -HOLE_R + (i as f64 + 0.5) * h;
            let z = HOLE_Z - HOLE_R + (j as f64 + 0.5) * h;
            if y * y + (z - HOLE_Z).powi(2) > HOLE_R * HOLE_R || in_slot(y, z) {
                continue;
            }
            let x_boss = (R_BOSS * R_BOSS - y * y).sqrt();
            if x_boss > X_BOTTOM {
                v += (x_boss - X_BOTTOM) * h * h;
            }
        }
    }
    v
}

#[test]
fn slotted_boss_drilled_hole_resolves_line_pp_corners_and_generator_pierces() {
    let mut a = BrepArena::new();
    let b = boss(&mut a);
    let s = slot(&mut a);
    let slotted = boolean_op(&mut a, b, s, BoolOp::Subtract).expect("boss − slot");
    validate_solid(&a, slotted).expect("slotted boss validates");
    let v_slotted = mesh_signed_volume(&tessellate(&a, slotted).expect("slotted tessellates"));

    let h = hole(&mut a);
    let out = boolean_op(&mut a, slotted, h, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("slotted boss − hole failed: {e:?}"));
    let report = validate_solid(&a, out).expect("drilled slotted boss validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);

    // The generators bottom ∩ lateral sit at y = ±y0.
    let y0 = (R_BOSS * R_BOSS - X_BOTTOM * X_BOTTOM).sqrt();
    let inside_hole = |y: f64, z: f64| y * y + (z - HOLE_Z).powi(2) < HOLE_R * HOLE_R;

    // Line × plane-pair CORNERS: each generator ends on a slot flank inside
    // the hole disc — {lateral, flank, bottom}.
    let mut corners = 0;
    for y in [y0, -y0] {
        for z in [z_lo(y), z_hi(y)] {
            if inside_hole(y, z) {
                corners += 1;
                let c = [X_BOTTOM, y, z];
                assert!(
                    has_vertex(&mesh, c),
                    "corner {c:?} is not an exact output vertex"
                );
            }
        }
    }
    assert_eq!(
        corners, 3,
        "the fixture is authored with three flank corners in the hole"
    );

    // Generator PIERCES: each generator exits the hole through its lateral —
    // {lateral, bottom, hole lateral}, the line-curve corridor's shape.
    let dz = (HOLE_R * HOLE_R - y0 * y0).sqrt();
    let mut pierces = 0;
    for y in [y0, -y0] {
        for z in [HOLE_Z + dz, HOLE_Z - dz] {
            if in_slot(y, z) {
                continue; // in air: the generator is not an edge there
            }
            pierces += 1;
            let c = [X_BOTTOM, y, z];
            assert!(
                has_vertex(&mesh, c),
                "pierce {c:?} is not an exact output vertex"
            );
        }
    }
    assert_eq!(pierces, 3, "the fixture is authored with three pierces");

    // Volume: the hole removes the analytic scoop. Both meshes facet the
    // boss lateral with their own vertex sets, so the difference carries
    // facet noise of the order of one sagitta over the scoop's footprint —
    // the band is loose, the direction is not.
    let scoop = analytic_scoop();
    let removed = v_slotted - vol;
    assert!(removed > 0.0, "the hole removes material: {removed}");
    assert!(
        (removed - scoop).abs() <= 0.5 * scoop,
        "removed {removed} vs analytic scoop {scoop}"
    );
}

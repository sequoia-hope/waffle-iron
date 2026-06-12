//! PR-F3 RED — the plane-PARALLEL-to-axis × cylinder SSI LINE case
//! (KV6b-F3): booleans where a planar face slices a cylinder lateral
//! along ruling lines.
//!
//! ssi-rs already solves this pair exactly (plane_cylinder C3a/C3b: one or
//! two `Line`s); the gap is yang Stage 4 — `LineSegment` intersection
//! edges get NO relocation (their arrangement points sit on facet chords,
//! off the exact line AND off the cylinder by up to the sagitta), and a
//! TRIPLE point shared by a line edge and a circle edge is relocated onto
//! the circle only, leaving it off the cutting plane (the off-plane vertex
//! the KV6b probe caught: radius exactly r, z off by the sagitta).
//!
//! This is NOT revolve-specific: a plain KV5a cylinder unioned with a box
//! that overlaps it SIDEWAYS hits it — the most ordinary CAD gesture.
//!
//! ## Fixture (exact volumes)
//!
//! Cylinder: r = 1, axis +z through the origin, z ∈ [0, 1].
//! Box: x ∈ [0.6, 2], y ∈ [−2, 2], z ∈ [0, 1] — its x = 0.6 plane is
//! PARALLEL to the axis and slices the lateral wall along two ruling
//! lines (the y = ±2 planes clear the cylinder; z-caps are coplanar with
//! the cylinder caps — wait, they ARE: z∈[0,1] matches. Use z ∈ [−0.2,
//! 1.3] so the box caps clear the cylinder caps and ONLY the x = 0.6
//! plane introduces the line case... that makes the overlap a full
//! z-through prism of the circular segment x > 0.6 over z ∈ [0, 1]).
//!
//! Circular segment area (d = 0.6, r = 1):
//! `A_seg = r²·acos(d/r) − d·√(r² − d²)`.
//! Union volume = π·r²·h + V_box − A_seg·h (exact).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
};

fn cylinder(arena: &mut BrepArena) -> kernel_v2::SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        1.0,
    )
    .expect("circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .expect("cylinder")
        .solid
}

fn box_solid(
    arena: &mut BrepArena,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> kernel_v2::SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box")
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

/// Cylinder caps are at z ∈ [0, 1]; the box spans z ∈ [−0.2, 1.3] so the
/// caps are NOT coplanar (avoids the M8 wall) and the overlap prism is the
/// full-height circular segment x > 0.6.
const BX: (f64, f64) = (0.6, 2.0);
const BY: (f64, f64) = (-2.0, 2.0);
const BZ: (f64, f64) = (-0.2, 1.3);

fn segment_area(d: f64, r: f64) -> f64 {
    r * r * (d / r).acos() - d * (r * r - d * d).sqrt()
}

#[test]
fn cylinder_union_sideways_box_exact_volume() {
    let mut arena = BrepArena::new();
    let c = cylinder(&mut arena);
    let b = box_solid(&mut arena, BX, BY, BZ);
    let out = boolean_op(&mut arena, c, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("cylinder ∪ sideways box: {e:?}"));
    validate_solid(&arena, out).expect("union validates");
    let mesh = tessellate(&arena, out).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    // Overlap = segment(x > 0.6) × cylinder height 1.
    let overlap = segment_area(0.6, 1.0) * 1.0;
    let box_vol = (BX.1 - BX.0) * (BY.1 - BY.0) * (BZ.1 - BZ.0);
    let expect = std::f64::consts::PI + box_vol - overlap;
    // Chord band: the cylinder's mesh under-fills; box is exact.
    assert!(
        vol <= expect * 1.001 && vol >= expect - 0.05 * std::f64::consts::PI,
        "union volume {vol} vs {expect}"
    );
}

#[test]
fn cylinder_subtract_sideways_box_exact_volume() {
    let mut arena = BrepArena::new();
    let c = cylinder(&mut arena);
    let b = box_solid(&mut arena, BX, BY, BZ);
    let out = boolean_op(&mut arena, c, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cylinder − sideways box: {e:?}"));
    validate_solid(&arena, out).expect("cut validates");
    let mesh = tessellate(&arena, out).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let expect = std::f64::consts::PI - segment_area(0.6, 1.0);
    assert!(
        vol <= expect * 1.001 && vol >= 0.93 * expect,
        "cut volume {vol} vs {expect}"
    );
}

/// Every output vertex that lies near the cylinder surface must be ON it
/// (the relocation contract): radius within a tight band of 1 OR clearly
/// interior/exterior box geometry. Catches the chord points the missing
/// line relocation leaves behind.
#[test]
fn line_edge_points_relocated_onto_cylinder_and_plane() {
    let mut arena = BrepArena::new();
    let c = cylinder(&mut arena);
    let b = box_solid(&mut arena, BX, BY, BZ);
    let _out = boolean_op(&mut arena, c, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cylinder − box: {e:?}"));
    // The cut face at x = 0.6 inside the cylinder: every boundary vertex of
    // the cut region that lies on the lateral wall must satisfy BOTH
    // surfaces: x == 0.6 (exact plane) and radius == 1 (cylinder) — the
    // intersection LINES at y = ±√(1 − 0.36) = ±0.8.
    let mut line_pts = 0;
    for slot in &arena.vertices {
        let Some(v) = slot else { continue };
        let p = v.point;
        let r = (p.x() * p.x() + p.y() * p.y()).sqrt();
        // Points on the x = 0.6 plane AND near the lateral wall:
        if (p.x() - 0.6).abs() < 1e-6 && (r - 1.0).abs() < 0.05 {
            line_pts += 1;
            assert!(
                (r - 1.0).abs() <= 1e-6,
                "line-edge vertex {p:?} off the cylinder: r = {r}"
            );
            assert!(
                (p.y().abs() - 0.8).abs() <= 1e-6,
                "line-edge vertex {p:?} off the intersection line y = ±0.8"
            );
        }
    }
    assert!(
        line_pts >= 2,
        "expected relocated line-edge vertices on the x = 0.6 plane, found {line_pts}"
    );
}

/// The KV6b probe geometry: 270° revolve ∪ a box crossing its outer wall.
/// Failed with InvalidBooleanOutput (a triple point relocated onto the
/// box-side circle but left off the box-top plane).
#[test]
fn revolve_union_crossing_box_succeeds() {
    let mut arena = BrepArena::new();
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(3.0, 2.0),
            Point2::new(0.0, 2.0),
        ],
        vec![],
    )
    .unwrap();
    let r = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        270.0_f64.to_radians(),
    )
    .unwrap();
    let b = box_solid(&mut arena, (1.0, 2.0), (1.5, 2.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("revolve ∪ crossing box: {e:?}"));
    validate_solid(&arena, out).expect("validates");
    let mesh = tessellate(&arena, out).expect("tessellate");
    assert!(mesh_signed_volume(&mesh) > 0.0);
}

#[test]
fn sideways_boolean_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let c = cylinder(&mut arena);
        let b = box_solid(&mut arena, BX, BY, BZ);
        let out = boolean_op(&mut arena, c, b, BoolOp::Subtract).expect("cut");
        let mesh = tessellate(&arena, out).expect("tessellate");
        (arena, mesh)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2);
    assert_eq!(m1, m2);
}

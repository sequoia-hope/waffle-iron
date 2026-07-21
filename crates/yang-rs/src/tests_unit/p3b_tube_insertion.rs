//! P3b increment-2 unit fixtures: the cylinder-lateral interior junction
//! insertion channel (spec `yang_169_p3b_curved_partner_pierce.md` §3.3) —
//! the face-override pre-pass cylinder arm + the containing-triangle 3-fan
//! splice, driven through `rebuilt_with_junction_overrides` exactly the way
//! the increment-3 wiring will call it.

use super::n2_junction::{rj_box, rj_cylinder};
use super::p3a_edge_overrides::closed_conformal_2_manifold;
use crate::boolean::line_edge_cylinder_face_pierce;
use crate::*;
use std::collections::BTreeMap;

fn bits(p: Point3) -> [u64; 3] {
    [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
}

/// The z-axis tube r=0.25, v∈[0,1] and its lateral-face index.
fn tube() -> (BRep, u32) {
    let b = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25, 1.0);
    let f = b
        .faces()
        .iter()
        .position(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .expect("fixture has a lateral face") as u32;
    (b, f)
}

/// One on-surface interior point splices as a local 3-fan: exact bits in
/// the mesh, +2 triangles, closed conformal 2-manifold, and the minted
/// vertex's `BRepFace` source round-trips through `eval_source` onto the
/// same point (the Stage-1 bijection contract).
#[test]
fn tube_interior_mint_splices_exact_bits_and_stays_manifold() {
    let (tube, lat) = tube();
    let th = 0.7f64;
    let j = Point3::new(0.25 * th.cos(), 0.25 * th.sin(), 0.5);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j]);
    let rebuilt = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("cylinder interior splice");
    let mesh = rebuilt.as_mesh();
    assert_eq!(
        mesh.tris.len(),
        tube.as_mesh().tris.len() + 2,
        "one containing triangle became a 3-fan"
    );
    let vi = mesh
        .verts
        .iter()
        .position(|p| bits(*p) == bits(j))
        .expect("the junction's EXACT bits are a mesh vertex");
    assert!(
        closed_conformal_2_manifold(&mesh.tris),
        "splice preserves the closed conformal 2-manifold"
    );
    // Bijection: the minted vertex carries a BRepFace source on the lateral
    // that evaluates back onto the junction point.
    let src = rebuilt.tessellation_map().sources[vi];
    let TessellationSource::BRepFace { face, .. } = src else {
        panic!("minted vertex must carry a BRepFace source, got {src:?}");
    };
    assert_eq!(face, lat);
    let back = rebuilt.eval_source(src).as_array();
    let ja = j.as_array();
    let err =
        ((back[0] - ja[0]).powi(2) + (back[1] - ja[1]).powi(2) + (back[2] - ja[2]).powi(2)).sqrt();
    assert!(err < 1e-12, "source round-trip within fp noise, got {err}");
}

/// Two interior points on the same lateral splice independently (+4 tris),
/// both bit-exact, still manifold — the sequential fan handles multiple
/// mints per face.
#[test]
fn two_interior_mints_on_one_lateral() {
    let (tube, lat) = tube();
    let j1 = Point3::new(0.25 * 0.7f64.cos(), 0.25 * 0.7f64.sin(), 0.4);
    let j2 = Point3::new(0.25 * (-1.9f64).cos(), 0.25 * (-1.9f64).sin(), 0.7);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j1, j2]);
    let rebuilt = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("double splice");
    let mesh = rebuilt.as_mesh();
    assert_eq!(mesh.tris.len(), tube.as_mesh().tris.len() + 4);
    for j in [j1, j2] {
        assert!(
            mesh.verts.iter().any(|p| bits(*p) == bits(j)),
            "junction {j:?} minted bit-exactly"
        );
    }
    assert!(closed_conformal_2_manifold(&mesh.tris));
}

/// An off-surface point is a producer fault — loud error, never a
/// projection or a silent drop.
#[test]
fn off_surface_interior_point_errors_loudly() {
    let (tube, lat) = tube();
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![Point3::new(0.3, 0.0, 0.5)]); // r=0.3 ≠ 0.25
    let err = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect_err("off-surface must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("off the cylinder surface"),
        "names the defect: {msg}"
    );
}

/// inc-4e (spec §3.3 second arm, the C0103 class): a point EXACTLY on a
/// tube grid edge (the seam ruling) splits the edge's two incident
/// triangles into a 2+2 fan — bit-exact mint, +2 triangles, still a closed
/// conformal 2-manifold. (Until inc-4e this placement was the deferred
/// fail-closed loud error; rim-ring insertions made it a live class.)
#[test]
fn on_grid_edge_interior_point_splits_2_plus_2() {
    let (tube, lat) = tube();
    // The seam ruling runs through (0.25, 0, v) — a mid-height point on it
    // sits exactly on a grid edge in the chart.
    let j = Point3::new(0.25, 0.0, 0.5);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j]);
    let rebuilt = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("on-grid-edge placement splits 2+2");
    let mesh = rebuilt.as_mesh();
    assert_eq!(
        mesh.tris.len(),
        tube.as_mesh().tris.len() + 2,
        "two incident triangles became four"
    );
    assert!(
        mesh.verts.iter().any(|p| bits(*p) == bits(j)),
        "the junction's EXACT bits are a mesh vertex"
    );
    assert!(
        closed_conformal_2_manifold(&mesh.tris),
        "2+2 split preserves the closed conformal 2-manifold"
    );
}

/// inc-4e (the C0102 class): a point strictly inside a triangle but within
/// the weld band of a grid ruling (post-composition shift, e.g. 5.55e-17
/// off) routes to the SAME 2+2 edge-split — never the guaranteed-sliver
/// 3-fan, never a loud error.
#[test]
fn near_grid_edge_interior_point_splits_2_plus_2() {
    let (tube, lat) = tube();
    // A hair off the seam ruling in azimuth: chart distance r·θ ≈ 1e-8,
    // far below the weld band TAU_MODEL·(1+scale) ≈ 1.25e-7 yet nonzero.
    let th = 4.0e-8f64;
    let j = Point3::new(0.25 * th.cos(), 0.25 * th.sin(), 0.5);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j]);
    let rebuilt = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("near-grid-edge placement splits 2+2");
    let mesh = rebuilt.as_mesh();
    assert_eq!(mesh.tris.len(), tube.as_mesh().tris.len() + 2);
    assert!(mesh.verts.iter().any(|p| bits(*p) == bits(j)));
    assert!(closed_conformal_2_manifold(&mesh.tris));
}

/// A point within the weld band of an EXISTING mesh vertex (here: the
/// previous mint in the same override list) is still the loud sub-band
/// vertex error — that arm is the wiring pre-filters' skip-on-both-sides
/// multiplicity guard, never a splice.
#[test]
fn sub_band_duplicate_of_prior_mint_errors_loudly() {
    let (tube, lat) = tube();
    let th = 0.7f64;
    let j1 = Point3::new(0.25 * th.cos(), 0.25 * th.sin(), 0.5);
    let j2 = Point3::new(j1.x(), j1.y(), j1.z() + 1.0e-9); // well inside the band
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j1, j2]);
    let err = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect_err("sub-band duplicate must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("weld band of an existing grid vertex"),
        "names the guard: {msg}"
    );
}

/// The cross-operand conformality contract (spec §4, the inc-3 shape): the
/// inc-1 pierce primitive's OWN output J, inserted via the owner EDGE
/// channel into the box and the partner FACE channel into the tube, lands
/// bit-exactly in BOTH rebuilt Stage-1 meshes as closed 2-manifolds.
#[test]
fn both_operands_carry_the_pierce_bit_exactly() {
    let (tube, lat) = tube();
    // Box whose bottom-front edge is the segment (-1,0.1,0.5)→(1,0.1,0.5).
    let bx = rj_box([-1.0, 0.1, 0.5], [1.0, 1.1, 1.5]);
    // Locate that geometric edge and its incident surfaces.
    let target = |p: Point3| {
        (p.y() - 0.1).abs() < 1e-15 && (p.z() - 0.5).abs() < 1e-15 && p.x().abs() >= 1.0 - 1e-15
    };
    let copies: Vec<u32> = (0..bx.edges().len() as u32)
        .filter(|&ei| {
            let e = &bx.edges()[ei as usize];
            e.curve == Curve::LineSegment
                && target(bx.vertices()[e.start as usize].point)
                && target(bx.vertices()[e.end as usize].point)
        })
        .collect();
    assert_eq!(copies.len(), 2, "per-loop copies of the bottom-front edge");
    let e0 = &bx.edges()[copies[0] as usize];
    let (p0, p1) = (
        bx.vertices()[e0.start as usize].point,
        bx.vertices()[e0.end as usize].point,
    );
    let surfs: Vec<Surface> = bx
        .faces()
        .iter()
        .filter(|f| f.outer_loop.iter().any(|ei| copies.contains(ei)))
        .map(|f| f.surface)
        .collect();
    assert_eq!(surfs.len(), 2, "edge incident to two faces");
    // The inc-1 primitive mints the junctions.
    let pierces = line_edge_cylinder_face_pierce(
        p0,
        p1,
        surfs[0],
        surfs[1],
        lat,
        &tube.faces()[lat as usize],
        &tube,
    );
    assert_eq!(pierces.len(), 2, "the chord crosses the tube twice");
    let pts: Vec<Point3> = pierces.iter().map(|pp| pp.point).collect();
    // Owner side: the box edge polylines (all copies, the P3a fan-out).
    let mut eo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    for &ei in &copies {
        eo.insert(ei, pts.clone());
    }
    let box_rebuilt = bx
        .rebuilt_with_junction_overrides(&eo, &BTreeMap::new())
        .expect("owner-side splice");
    // Partner side: the tube lateral interior.
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, pts.clone());
    let tube_rebuilt = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("partner-side splice");
    for j in &pts {
        for (mesh, side) in [
            (box_rebuilt.as_mesh(), "owner"),
            (tube_rebuilt.as_mesh(), "partner"),
        ] {
            assert!(
                mesh.verts.iter().any(|p| bits(*p) == bits(*j)),
                "{side} mesh carries {j:?} bit-exactly"
            );
        }
    }
    assert!(closed_conformal_2_manifold(&box_rebuilt.as_mesh().tris));
    assert!(closed_conformal_2_manifold(&tube_rebuilt.as_mesh().tris));
}

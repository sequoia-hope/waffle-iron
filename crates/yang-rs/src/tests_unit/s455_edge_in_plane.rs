// The star fixture carries the corpus case's authored digits (`cos 45°`
// rounded by the generator), not the constant.
#![allow(clippy::approx_constant)]
#[allow(unused_imports)]
use super::*;

// ====================================================================
// §4.5.5 one dimension down — edge-in-plane identification + conformity
// (spec `specs/yang_455_edge_in_plane_conformity.md`, P0001).
//
// A needle star's tip edge authored 4e-15 below the octagon prism's cap
// plane, running inside the cap region: nothing identified the edge into
// the plane, so the exact arrangement kept the 4e-15-wide wedge of the
// upper flank as "outside A" and kernel-v2's G1 gate refused the collapsed
// ear. Arm 1 moves the tip vertices onto the plane; arm 2 makes the
// sub-segment inside the cap one identically-sampled mesh edge in BOTH
// operands (a crossing minted into every copy of both crossed edges, the
// inside endpoint an interior Steiner point, the sub-segment a CDT
// constraint), so the result does not depend on exact coplanarity.
// ====================================================================

use crate::stage0::edge_in_plane::{conformity_overrides, identify_vertices};

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    scale(a, 1.0 / l)
}

/// A prism: the CCW polygon `poly` in the frame (`origin`, `u`, `v`) with
/// `n = u × v` (unit), extruded by `depth` along `n`. Bottom cap normal `−n`,
/// top cap `+n`, one side face per polygon edge, one edge copy per face loop
/// (the per-loop-copy convention). A CW polygon is reversed.
fn prism(poly: &[[f64; 2]], origin: [f64; 3], u: [f64; 3], v: [f64; 3], depth: f64) -> BRep {
    let mut poly: Vec<[f64; 2]> = poly.to_vec();
    let area2: f64 = (0..poly.len())
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if area2 < 0.0 {
        poly.reverse();
    }
    let n = unit(cross(u, v));
    let k = poly.len();
    let bottom: Vec<[f64; 3]> = poly
        .iter()
        .map(|q| add(add(origin, scale(u, q[0])), scale(v, q[1])))
        .collect();
    let top: Vec<[f64; 3]> = bottom.iter().map(|b| add(*b, scale(n, depth))).collect();
    let mut verts: Vec<BRepVertex> = Vec::new();
    for b in &bottom {
        verts.push(BRepVertex { point: p3(*b) });
    }
    for t in &top {
        verts.push(BRepVertex { point: p3(*t) });
    }
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    let mut face = |loop_verts: Vec<u32>, normal: [f64; 3], on: [f64; 3]| {
        let base = edges.len() as u32;
        let m = loop_verts.len();
        for i in 0..m {
            edges.push(BRepEdge {
                start: loop_verts[i],
                end: loop_verts[(i + 1) % m],
                curve: Curve::LineSegment,
            });
        }
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(normal[0], normal[1], normal[2]),
                d: -dot(normal, on),
            },
            outer_loop: (base..base + m as u32).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        });
    };
    // Bottom (viewed from −n the CCW polygon reads CW → reverse).
    face((0..k as u32).rev().collect(), scale(n, -1.0), bottom[0]);
    // Top.
    face((k as u32..2 * k as u32).collect(), n, top[0]);
    // Sides: b_i → t_i → t_{i+1} → b_{i+1} (the `box_brep` convention).
    for i in 0..k {
        let j = (i + 1) % k;
        let dir = unit(add(bottom[j], scale(bottom[i], -1.0)));
        let outward = unit(cross(dir, n));
        face(
            vec![i as u32, (k + i) as u32, (k + j) as u32, j as u32],
            outward,
            bottom[i],
        );
    }
    BRep::new(verts, edges, faces).expect("prism topology")
}

/// Y-plane origin shared by both P0001 sketches.
const Y0: f64 = 22.41755130980609;

/// P0001's octagon (world (x, z) coordinates), extruded 9.139 along +Y from
/// the plane `y = Y0`.
fn octagon_prism() -> BRep {
    let xz = [
        [-10.01142855584012, -30.39476694290923],
        [-12.494294560269523, -24.569000763226],
        [-10.130507177517845, -18.693910603455162],
        [-4.304740997834618, -16.21104459902576],
        [1.5703491619362175, -18.574831981777436],
        [4.0532151663656215, -24.40059816146067],
        [1.6894277836139446, -30.2756883212315],
        [-4.136338396069275, -32.7585543256609],
    ];
    // Frame u = +z, v = +x: u × v = +y.
    let poly: Vec<[f64; 2]> = xz.iter().map(|q| [q[1], q[0]]).collect();
    prism(
        &poly,
        [0.0, Y0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        9.13931723988505,
    )
}

/// P0001's 4-point needle star (sketch (u, v) → world (y = Y0 + u, z = v)),
/// extruded 7.854 along +X. `tip_u` is the authored u of the two axis tips:
/// the generator's `cos 270°` (−4.047e-15) in the corpus case, 0 exactly in
/// the control.
fn star_prism(tip_u: f64, other_u: f64) -> BRep {
    let uv = [
        [22.0331315381885, 0.0],
        [1.4142135623730951, 1.414213562373095],
        [other_u, 22.0331315381885],
        [-1.414213562373095, 1.4142135623730951],
        [-22.0331315381885, 2.6982804013435146e-15],
        [-1.4142135623730954, -1.414213562373095],
        [tip_u, -22.0331315381885],
        [1.4142135623730947, -1.4142135623730954],
    ];
    // Frame u = +y, v = +z: u × v = +x.
    prism(
        &uv,
        [0.0, Y0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        7.854423622158365,
    )
}

const TIP_U: f64 = -4.047420602015271e-15;
const OTHER_U: f64 = 1.3491402006717573e-15;

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut v = 0.0;
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        v += dot(a, cross(b, c));
    }
    v / 6.0
}

fn closed_2_manifold(tris: &[[u32; 3]]) -> bool {
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in tris {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !counts.is_empty() && counts.values().all(|&c| c == 2)
}

fn union(a: &BRep, b: &BRep) -> Result<BRep, YangError> {
    let nb = crate::native_backend().expect("native backend");
    boolean(a, b, BoolOp::Union, &nb)
}

/// Control: with the tips authored exactly on the star's axis the tip edge
/// is bit-exactly in the cap plane and the union completes.
fn exact_union_volume() -> f64 {
    let out = union(&octagon_prism(), &star_prism(0.0, 0.0)).expect("exact-tip control unions");
    assert!(closed_2_manifold(&out.as_mesh().tris));
    signed_volume(out.as_mesh())
}

/// The P0001 class: the femto-off tip edge unions, closed 2-manifold, at the
/// exact-tip control's volume.
#[test]
pub(crate) fn s455_femto_off_tip_edge_unions_at_the_control_volume() {
    let control = exact_union_volume();
    let out = union(&octagon_prism(), &star_prism(TIP_U, OTHER_U))
        .expect("the femto-off tip edge must union");
    assert!(
        closed_2_manifold(&out.as_mesh().tris),
        "output must be a closed 2-manifold"
    );
    let vol = signed_volume(out.as_mesh());
    assert!(vol > 0.0);
    assert!(
        ((vol - control) / control).abs() < 1e-9,
        "volume {vol} vs control {control}"
    );
}

/// Arm 1 moves exactly the two axis-tip vertices of the star (and nothing of
/// the octagon), each by the authored 3.55e-15 onto `y = Y0`.
#[test]
pub(crate) fn s455_identification_moves_only_the_two_tip_vertices() {
    let a = octagon_prism();
    let b = star_prism(TIP_U, OTHER_U);
    let (na, nb) = identify_vertices(&a, &b)
        .expect("identify")
        .expect("the femto-off tips must be identified");
    assert_eq!(na.vertices(), a.vertices(), "the octagon is untouched");
    let mut moved: Vec<usize> = Vec::new();
    for (i, (before, after)) in b.vertices().iter().zip(nb.vertices()).enumerate() {
        if before != after {
            moved.push(i);
            assert_eq!(
                after.point.y(),
                Y0,
                "vertex {i} lands bit-exactly on the plane"
            );
            assert_eq!(after.point.x(), before.point.x());
            assert_eq!(after.point.z(), before.point.z());
        }
    }
    // The −z tip (index 6) on both caps; the +z tip (index 2, 1.3e-15 off)
    // lies OUTSIDE the octagon's region and is not identified.
    assert_eq!(moved, vec![6, 14], "only the in-region tip pair moves");
}

/// Arm 2 on the identified pair: the tip edge lies in the cap, crosses its
/// boundary once (at x ≈ 3.044 on the pt5–pt6 edge), the x = 0 tip is inside
/// the cap — one mint into every copy of both crossed edges, one interior
/// point, one constraint segment.
#[test]
pub(crate) fn s455_conformity_mints_one_crossing_and_one_constraint() {
    let a = octagon_prism();
    let b = star_prism(TIP_U, OTHER_U);
    let (na, nb) = identify_vertices(&a, &b)
        .expect("identify")
        .expect("identified");
    let eip = conformity_overrides(&na, &nb);
    assert_eq!(eip.mints.len(), 1, "one crossing: {eip:?}");
    let m = eip.mints[0];
    assert!(
        (m.x() - 3.044231589914659).abs() < 1e-9,
        "crossing x = {}",
        m.x()
    );
    assert_eq!(m.y(), Y0);
    // B's tip edge has two per-loop copies (both flanks); each gets the mint.
    assert_eq!(eip.edge_b.len(), 2);
    assert!(eip.edge_b.values().all(|v| v == &vec![m]));
    // A's crossed cap edge has two copies (the cap and the side face).
    assert_eq!(eip.edge_a.len(), 2);
    assert!(eip.edge_a.values().all(|v| v == &vec![m]));
    // The x = 0 tip is an interior point of A's cap (face 0 = bottom).
    assert_eq!(eip.face_a.len(), 1);
    let (cap, pts) = eip.face_a.iter().next().unwrap();
    assert_eq!(*cap, 0);
    assert_eq!(pts.len(), 1);
    assert_eq!(pts[0].x(), 0.0);
    // One constraint on that cap: (x = 0 tip) → (crossing).
    let cons = eip.cons_a.get(&0).expect("constraint on the cap");
    assert_eq!(cons.len(), 1);
    assert!(cons[0].contains(&pts[0]) && cons[0].contains(&m));
    assert!(eip.face_b.is_empty() && eip.cons_b.is_empty());
    assert_eq!(eip.declined, 0);
}

/// Rigid motion of the femto-off pair: the cap plane is oblique, bit-exact
/// coplanarity is unattainable, and the conformity arm still completes the
/// union at the same volume.
#[test]
pub(crate) fn s455_oblique_rigid_motion_unions_at_the_same_volume() {
    let control = exact_union_volume();
    // Rodrigues rotation about (1, 2, 3)/‖·‖ by 0.7 rad, then a translation.
    let k = unit([1.0, 2.0, 3.0]);
    let (s, c) = 0.7f64.sin_cos();
    let rot = |v: [f64; 3]| -> [f64; 3] {
        let kv = cross(k, v);
        let kd = dot(k, v);
        [
            v[0] * c + kv[0] * s + k[0] * kd * (1.0 - c),
            v[1] * c + kv[1] * s + k[1] * kd * (1.0 - c),
            v[2] * c + kv[2] * s + k[2] * kd * (1.0 - c),
        ]
    };
    let t = [0.3, -1.1, 2.2];
    let origin = add(rot([0.0, Y0, 0.0]), t);
    let xz = [
        [-10.01142855584012, -30.39476694290923],
        [-12.494294560269523, -24.569000763226],
        [-10.130507177517845, -18.693910603455162],
        [-4.304740997834618, -16.21104459902576],
        [1.5703491619362175, -18.574831981777436],
        [4.0532151663656215, -24.40059816146067],
        [1.6894277836139446, -30.2756883212315],
        [-4.136338396069275, -32.7585543256609],
    ];
    let poly: Vec<[f64; 2]> = xz.iter().map(|q| [q[1], q[0]]).collect();
    let a = prism(
        &poly,
        origin,
        rot([0.0, 0.0, 1.0]),
        rot([1.0, 0.0, 0.0]),
        9.13931723988505,
    );
    let uv = [
        [22.0331315381885, 0.0],
        [1.4142135623730951, 1.414213562373095],
        [OTHER_U, 22.0331315381885],
        [-1.414213562373095, 1.4142135623730951],
        [-22.0331315381885, 2.6982804013435146e-15],
        [-1.4142135623730954, -1.414213562373095],
        [TIP_U, -22.0331315381885],
        [1.4142135623730947, -1.4142135623730954],
    ];
    let b = prism(
        &uv,
        origin,
        rot([0.0, 1.0, 0.0]),
        rot([0.0, 0.0, 1.0]),
        7.854423622158365,
    );
    let out = union(&a, &b).expect("the oblique femto-off pair must union");
    assert!(closed_2_manifold(&out.as_mesh().tris));
    let vol = signed_volume(out.as_mesh());
    assert!(
        ((vol - control) / control).abs() < 1e-9,
        "oblique volume {vol} vs control {control}"
    );
}

/// An edge lying ENTIRELY inside a partner face (both endpoints inside, no
/// boundary crossing): a diamond-section prism whose ridge runs inside a
/// box's top face, half in the box, half above, reaching past the box's
/// side. The ridge 4e-15 above the top plane unions at the exact ridge's
/// volume.
#[test]
pub(crate) fn s455_ridge_edge_fully_inside_a_partner_face() {
    let cube = prism(
        &[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        4.0,
    );
    // Diamond in (y, z) with its left vertex (the ridge) at (2, z_ridge),
    // extruded along +x from x = 1 to x = 3.
    let diamond = |z_ridge: f64| -> BRep {
        prism(
            &[
                [2.0, z_ridge],
                [3.0, z_ridge + 0.5],
                [6.0, z_ridge],
                [3.0, z_ridge - 0.5],
            ],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            2.0,
        )
    };
    let exact = union(&cube, &diamond(4.0)).expect("exact ridge unions");
    let control = signed_volume(exact.as_mesh());
    let femto = 4.0 + 4.0 * f64::EPSILON;
    let a = cube.clone();
    let b = diamond(femto);
    let eip = {
        let (na, nb) = identify_vertices(&a, &b)
            .expect("identify")
            .expect("identified");
        conformity_overrides(&na, &nb)
    };
    assert!(eip.mints.is_empty(), "no boundary crossing");
    let cons = eip
        .cons_a
        .get(&1)
        .expect("constraint on the cube's top face");
    assert_eq!(cons.len(), 1);
    assert_eq!(
        eip.face_a.get(&1).map(Vec::len),
        Some(2),
        "both ridge endpoints interior"
    );
    let out = union(&a, &b).expect("the femto-off ridge must union");
    assert!(closed_2_manifold(&out.as_mesh().tris));
    let vol = signed_volume(out.as_mesh());
    assert!(
        ((vol - control) / control).abs() < 1e-9,
        "ridge volume {vol} vs control {control}"
    );
}

/// Identity: a plain crossing pair (no vertex near a partner plane) — arm 1
/// returns `None` (no rebuild), arm 2 is empty.
#[test]
pub(crate) fn s455_plain_crossing_pair_is_the_identity() {
    let a = prism(
        &[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        4.0,
    );
    let b = prism(
        &[[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]],
        [0.0, 0.0, 2.5],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        4.0,
    );
    assert!(identify_vertices(&a, &b).expect("identify").is_none());
    assert!(conformity_overrides(&a, &b).is_empty());
    union(&a, &b).expect("crossing boxes union");
}

/// Stage 1: an interior constraint segment between two interior points of a
/// planar face is an EDGE of the emitted triangulation.
#[test]
pub(crate) fn s455_stage1_face_constraint_is_an_edge_of_the_cdt() {
    let cube = prism(
        &[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        4.0,
    );
    let p = Point3::new(1.0, 1.5, 4.0);
    let q = Point3::new(3.0, 2.5, 4.0);
    let mut face_overrides = std::collections::BTreeMap::new();
    face_overrides.insert(1u32, vec![p, q]);
    let mut cons = crate::stage1_tessellate::FaceConstraints::new();
    cons.insert(1u32, vec![[p, q]]);
    let rebuilt = cube
        .rebuilt_with_all_overrides_and_constraints(
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            &face_overrides,
            &cons,
        )
        .expect("rebuild with a constraint");
    let mesh = rebuilt.as_mesh();
    let idx = |pt: Point3| {
        mesh.verts
            .iter()
            .position(|v| *v == pt)
            .expect("point in mesh") as u32
    };
    let (ip, iq) = (idx(p), idx(q));
    let has_edge = mesh.tris.iter().any(|t| {
        (0..3).any(|i| {
            let (u, v) = (t[i], t[(i + 1) % 3]);
            (u == ip && v == iq) || (u == iq && v == ip)
        })
    });
    assert!(
        has_edge,
        "the constraint must be an edge of the top face's CDT"
    );
    assert!(closed_2_manifold(&mesh.tris));
    // The standing constraint survives a from-topology rebuild.
    assert_eq!(
        rebuilt.standing_face_constraints().get(&1).map(Vec::len),
        Some(1)
    );
    let again = rebuilt
        .retessellated_at_current_d_eps()
        .expect("re-tessellate");
    assert_eq!(again.as_mesh().tris.len(), mesh.tris.len());
}

/// F0055 / `mixed_cap_flush_stack_union`: a cylinder whose SEAM line lies in a
/// box face — the seam is incident to the curved lateral, so arm 2 declines
/// the contact (the edge-override channel is planar-incident only) and the
/// subtract completes as before.
#[test]
pub(crate) fn s455_curved_incident_seam_in_a_partner_plane_is_declined() {
    let cube = prism(
        &[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        2.0,
    );
    // Column at the corner (0, 0), r = 1, z ∈ [−0.5, 2.5]; its seam vertex
    // sits at azimuth 0 → (1, 0, z), on the box's y = 0 face plane.
    let (cv, ce, cf) = super::boolean_functional::rt_cylinder(-0.5, 3.0, 1.0);
    let column = BRep::new(cv, ce, cf).expect("column");
    let eip = conformity_overrides(&cube, &column);
    assert!(
        eip.is_empty(),
        "no override on a curved-incident seam: {eip:?}"
    );
    assert!(eip.declined >= 1);
    let nb = crate::native_backend().expect("native backend");
    boolean(&cube, &column, BoolOp::Subtract, &nb).expect("box − corner column");
}

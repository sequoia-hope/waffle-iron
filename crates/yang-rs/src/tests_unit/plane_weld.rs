#[allow(unused_imports)]
use super::*;

// ====================================================================
// §4.5.5 shared plane at the B-Rep level — `stage0::plane_weld`
// (spec `specs/yang_455_coplanar_plane_weld.md`, measured on
// `error_oct4.waffle` 2026-10-04: a through-cut cylinder whose cap was
// authored 2.235e-10 below the frame's top face by an f32-rounded sketch
// origin).
//
// The scan WELDS such a pair (gap ≤ band/100) and Stage 0 snaps the loop
// vertices onto A's plane — but B's stored cap plane and rim circle kept
// their own plane, so every uniform rim sample and every opposite-rim image
// lived 2.235e-10 below every snapped vertex and overlay mint. The weld
// rewrites the participating faces' planes, curved-edge anchors and loop
// vertices onto the canonical plane BEFORE Stage 0, and is the identity for
// bit-exact coplanar input.
// ====================================================================

/// Axis-aligned box BRep spanning `lo..hi` with outward face normals
/// (the `n178_subres_coplanar` fixture, duplicated — it is private there).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let verts = vec![
        BRepVertex {
            point: p(x0, y0, z0),
        },
        BRepVertex {
            point: p(x1, y0, z0),
        },
        BRepVertex {
            point: p(x1, y1, z0),
        },
        BRepVertex {
            point: p(x0, y1, z0),
        },
        BRepVertex {
            point: p(x0, y0, z1),
        },
        BRepVertex {
            point: p(x1, y0, z1),
        },
        BRepVertex {
            point: p(x1, y1, z1),
        },
        BRepVertex {
            point: p(x0, y1, z1),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let mut edges = Vec::new();
    let mut loops = Vec::new();
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [z0, -z1, y0, -x1, -y1, x0];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).unwrap()
}

/// The unit box and a through-cut cylinder of radius 0.3 on the box's axis
/// whose TOP cap sits `residual` below the box's top face `z = 1` (its
/// bottom cap is well clear of the box, at `z = -0.5`). `residual == 0.0`
/// is the bit-exact coplanar pair.
fn box_and_cutter(residual: f64) -> (BRep, BRep) {
    let a = box_brep([-0.5, -0.5, 0.0], [0.5, 0.5, 1.0]);
    let (verts, edges, faces) = rt_cylinder(-0.5, 1.5 - residual, 0.3);
    let b = BRep::new(verts, edges, faces).unwrap();
    (a, b)
}

/// The measured producer residual — f32(0.01) is 2.235e-10 below 0.01.
const RESIDUAL: f64 = 2.235e-10;

/// I1: a bit-exact coplanar pair is left untouched — `None`, no rebuild.
#[test]
pub(crate) fn plane_weld_bit_exact_pair_is_identity() {
    let (a, b) = box_and_cutter(0.0);
    let welded = crate::stage0::plane_weld::weld_coplanar_planes(&a, &b).expect("weld");
    assert!(
        welded.is_none(),
        "a bit-exact coplanar pair carries the canonical plane already — nothing to weld"
    );
}

/// No near-coplanar pair at all (the cap clear of every box face) — `None`.
#[test]
pub(crate) fn plane_weld_without_a_pair_is_identity() {
    let (a, b) = box_and_cutter(0.25);
    let welded = crate::stage0::plane_weld::weld_coplanar_planes(&a, &b).expect("weld");
    assert!(welded.is_none(), "no cross pair ⇒ nothing to weld");
}

/// The residual cap is rewritten onto the box's plane: its `Surface::Plane`
/// (orientation kept), its rim circle's center and its seam vertex all land
/// at `z = 1` bit for bit; the box (the canonical side) is byte-identical.
#[test]
pub(crate) fn plane_weld_rewrites_residual_cap_onto_the_canonical_plane() {
    let (a, b) = box_and_cutter(RESIDUAL);
    let Surface::Plane { d: d_before, .. } = b.faces()[2].surface else {
        panic!("fixture: face 2 is the top cap");
    };
    assert_ne!(d_before, -1.0, "fixture: the cap starts off the plane");

    let (na, nb) = crate::stage0::plane_weld::weld_coplanar_planes(&a, &b)
        .expect("weld")
        .expect("a welded pair must rewrite");

    // A is the canonical side: untouched bit for bit.
    for (fa, fb) in a.faces().iter().zip(na.faces()) {
        assert_eq!(fa.surface, fb.surface, "A face plane changed");
    }
    for (va, vb) in a.vertices().iter().zip(na.vertices()) {
        assert_eq!(va.point, vb.point, "A vertex moved");
    }

    // B's cap plane: same outward orientation, the canonical offset.
    let Surface::Plane { normal, d } = nb.faces()[2].surface else {
        panic!("cap stays planar");
    };
    assert_eq!(normal.as_array(), [0.0, 0.0, 1.0]);
    assert_eq!(d, -1.0, "cap offset must be the box top's, bit for bit");
    // The rim circle's anchor and the seam vertex follow.
    let Curve::Circle { center, .. } = nb.edges()[1].curve else {
        panic!("rim stays a circle");
    };
    assert_eq!(center.as_array()[2], 1.0, "rim circle center on the plane");
    assert_eq!(
        nb.vertices()[1].point.as_array()[2],
        1.0,
        "seam vertex on the plane"
    );
    // The bottom cap (clear of the box) is not a pair: untouched.
    assert_eq!(nb.faces()[1].surface, b.faces()[1].surface);
    assert_eq!(nb.edges()[0].curve, b.edges()[0].curve);
    assert_eq!(nb.vertices()[0].point, b.vertices()[0].point);
}

/// End to end: the residual-cap through-cut builds, and every output vertex
/// on the top face sits at `z = 1` exactly — no 2.235e-10 skin, no twin.
/// Compared against the bit-exact cut, which it must match in face count.
#[test]
pub(crate) fn plane_weld_residual_through_cut_matches_the_exact_cut() {
    let nb = crate::native_backend().expect("native backend");
    let (a, b) = box_and_cutter(0.0);
    let exact = boolean(&a, &b, BoolOp::Subtract, &nb).expect("exact through-cut");
    let (a, b) = box_and_cutter(RESIDUAL);
    let welded = boolean(&a, &b, BoolOp::Subtract, &nb).expect("residual through-cut");
    assert_eq!(
        welded.faces().len(),
        exact.faces().len(),
        "the residual cut must be the same solid as the exact cut"
    );
    for v in welded.vertices() {
        let z = v.point.as_array()[2];
        if z > 0.5 {
            assert_eq!(z, 1.0, "top-face vertex {:?} off the welded plane", v.point);
        }
    }
}

//! Task #179 — Stage-1 planar CDT parity-flap red fixture (spec
//! `specs/yang_stage1_cdt_parity_flap.md`).
//!
//! Captured verbatim from F0084 (probe session 2026-07-18): a fresh
//! octagon-prism extrude whose tilted cap polygon carries a NEAR-COLLINEAR
//! boundary triple — vertex 4 lies on the chord between vertices 3 and 7
//! (collinear to ~1e-16 after plane projection; the profile has a notch
//! 4→5→6→7 that returns to the chord line). The cap routes to
//! `tessellate_planar_cdt_face` (all-line, non-convex, no chains), whose
//! f64 centroid-parity interior classification keeps the exterior
//! hair-sliver triangle (3,7,4): the cap emits 7 triangles instead of 6,
//! and the extra zero-area flap leaves directed edges (3,7)/(7,4) unpaired
//! and (4,3) doubled — a NON-2-MANIFOLD operand mesh handed to the
//! arrangement in production (both junction-sampling gate states; F0084
//! gate-ON dies downstream at `s4-halfedge-pairing` fwd=1 rev=2).
//!
//! The fix (spec §3) finishes the F0047 flood-fill migration: the
//! all-segment planar CDT classifies interiors topologically
//! (`cdt_polygon_with_holes_floodfill`), never by f64 centroid parity.

use super::p3a_edge_overrides::closed_conformal_2_manifold;
use crate::*;

/// The F0084 operand-B topology, bit-exact: 16 vertices, 48 per-loop-copy
/// `LineSegment` edges, 10 faces (2 octagon caps + 8 side quads).
pub(crate) fn f0084_octagon_prism() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let pts: [[f64; 3]; 16] = [
        [0.3784462719369921, -0.07370607694753468, 3.9276283795958458],
        [-0.30607146311, 0.0720720917322487, 3.927628379595846],
        [-0.4291041252382407, -0.5056415671752061, 3.8218232550202074],
        [0.25541360980875066, -0.6514197358549887, 3.821823255020207],
        [0.3011028121541197, -0.43688096376865077, 3.861114868013792],
        [0.1858856458657696, -0.4123437653265438, 3.861114868013792],
        [0.21753990330327305, -0.2637076505917648, 3.888336766602261],
        [0.33275706959162316, -0.2882448490338718, 3.888336766602261],
        [-0.4230984530386351, -0.47744126003224163, 3.660861330644907],
        [-0.30006579091039437, 0.10027239887521328, 3.766666455220546],
        [
            0.3844519441365978,
            -0.045505769804570065,
            3.7666664552205456,
        ],
        [0.33876274179122884, -0.26004454189090714, 3.727374842226961],
        [0.22354557550287882, -0.23550734344880014, 3.727374842226961],
        [0.19189131806537535, -0.38414345818357926, 3.700152943638492],
        [0.3071084843537254, -0.40868065662568615, 3.7001529436384915],
        [0.2614192820083563, -0.6232194287120242, 3.6608613306449067],
    ];
    let verts: Vec<BRepVertex> = pts
        .iter()
        .map(|p| BRepVertex {
            point: Point3::new(p[0], p[1], p[2]),
        })
        .collect();
    let seg = |start: u32, end: u32| BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    };
    let edge_pairs: [(u32, u32); 48] = [
        // 0-7: top cap octagon 0→1→…→7→0
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 0),
        // 8-15: bottom cap octagon 8→9→…→15→8
        (8, 9),
        (9, 10),
        (10, 11),
        (11, 12),
        (12, 13),
        (13, 14),
        (14, 15),
        (15, 8),
        // 16-47: side quads, 4 directed per-loop copies each
        (2, 1),
        (1, 9),
        (9, 8),
        (8, 2),
        (3, 2),
        (2, 8),
        (8, 15),
        (15, 3),
        (4, 3),
        (3, 15),
        (15, 14),
        (14, 4),
        (5, 4),
        (4, 14),
        (14, 13),
        (13, 5),
        (6, 5),
        (5, 13),
        (13, 12),
        (12, 6),
        (7, 6),
        (6, 12),
        (12, 11),
        (11, 7),
        (0, 7),
        (7, 11),
        (11, 10),
        (10, 0),
        (1, 0),
        (0, 10),
        (10, 9),
        (9, 1),
    ];
    let edges: Vec<BRepEdge> = edge_pairs.iter().map(|&(s, e)| seg(s, e)).collect();
    let plane = |n: [f64; 3], d: f64| Surface::Plane {
        normal: Vector3::new(n[0], n[1], n[2]),
        d,
    };
    let face = |surface: Surface, outer: Vec<u32>| BRepFace {
        surface,
        outer_loop: outer,
        inner_loops: Vec::new(),
        reversed: false,
    };
    let faces = vec![
        face(
            plane(
                [-0.0367265710194908, -0.17245373184463034, 0.984332702572667],
                -3.864904911734558,
            ),
            (0..8).collect(),
        ),
        face(
            plane(
                [0.03672657101949038, 0.1724537318446299, -0.9843327025726671],
                3.7013810097503943,
            ),
            (8..16).collect(),
        ),
        face(
            plane(
                [-0.9780663284171093, 0.20829368021299924, 0.0],
                -0.3143703533848094,
            ),
            vec![16, 17, 18, 19],
        ),
        face(
            plane(
                [
                    -0.20503028117286906,
                    -0.9627426723461391,
                    -0.17632110096635906,
                ],
                0.09908603094241475,
            ),
            vec![20, 21, 22, 23],
        ),
        face(
            plane(
                [
                    0.978066328417109,
                    -0.2082936802130007,
                    -3.8084076806265987e-16,
                ],
                -0.3854980657180198,
            ),
            vec![24, 25, 26, 27],
        ),
        face(
            plane(
                [0.20503028117286823, 0.9627426723461392, 0.1763211009663599],
                -0.32192727216788597,
            ),
            vec![28, 29, 30, 31],
        ),
        face(
            plane(
                [
                    0.9780663284171089,
                    -0.20829368021300088,
                    1.3742472831446884e-16,
                ],
                -0.26769709155012855,
            ),
            vec![32, 33, 34, 35],
        ),
        face(
            plane(
                [
                    -0.20503028117286878,
                    -0.9627426723461389,
                    -0.17632110096636017,
                ],
                0.47631547890703896,
            ),
            vec![36, 37, 38, 39],
        ),
        face(
            plane(
                [
                    0.978066328417109,
                    -0.2082936802130007,
                    -3.8084076806265987e-16,
                ],
                -0.3854980657180198,
            ),
            vec![40, 41, 42, 43],
        ),
        face(
            plane(
                [0.20503028117286998, 0.9627426723461389, 0.17632110096635964],
                -0.6991567201325048,
            ),
            vec![44, 45, 46, 47],
        ),
    ];
    (verts, edges, faces)
}

/// RED (pre-fix): Stage-1 tessellation of a valid 2-manifold octagon prism
/// must emit a closed conformal 2-manifold mesh. The parity classifier
/// keeps the (3,7,4) exterior hair-sliver → 29 triangles, unpaired
/// directed edges.
#[test]
fn octagon_prism_notch_stage1_mesh_is_2_manifold() {
    let (verts, edges, faces) = f0084_octagon_prism();
    let tess = stage1_tessellate(&verts, &edges, &faces).expect("stage1");
    assert!(
        closed_conformal_2_manifold(&tess.tris),
        "Stage-1 mesh of a valid octagon prism must be a closed conformal \
         2-manifold; got {} tris (28 expected — an extra triangle is the \
         parity flap)",
        tess.tris.len()
    );
}

/// The flap is zero-area: no emitted triangle may have (near-)zero area
/// relative to its edge lengths. Guards the same defect from the metric
/// side — a degenerate sliver in the operand mesh survives the arrangement
/// and poisons downstream half-edge pairing regardless of which boundary
/// configuration minted it.
#[test]
fn octagon_prism_notch_stage1_mesh_has_no_degenerate_tris() {
    let (verts, edges, faces) = f0084_octagon_prism();
    let tess = stage1_tessellate(&verts, &edges, &faces).expect("stage1");
    for (i, t) in tess.tris.iter().enumerate() {
        let p = |k: usize| tess.verts[t[k] as usize].as_array();
        let (a, b, c) = (p(0), p(1), p(2));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let area2 = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let lu = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        let lv = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        assert!(
            area2 > 1e-9 * lu * lv,
            "tri {i} {t:?} is a (near-)zero-area sliver (|cross| {area2:.3e} vs \
             edge lengths {lu:.3e}/{lv:.3e})"
        );
    }
}

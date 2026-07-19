//! Task #180 — #146 inc-3b: junction-insertion rebuild conformality red
//! fixture (spec `yang_146_conformal_junction_sampling.md`, "Remaining
//! increment-3 blocker").
//!
//! Captured verbatim from F0084 gate-ON (probe session 2026-07-18/19,
//! `NONMANIFOLD_SITE_PROBE` `i6-input-overuse` + `YANG_JUNCTION_MINT_PROBE=v`):
//! the fresh octagon-prism operand B (16 verts / 48 per-loop-copy edges /
//! 10 faces — the same topology shape as the #179 fixture, different chain
//! step) rebuilt through `stage1_tessellate_with_edge_overrides` with the
//! op's actual junction payload (14 edge overrides + 4 face-interior
//! overrides) emits a NON-2-MANIFOLD mesh: an extra sliver triangle over
//! three consecutive points of a split edge polyline (e.g. B-Rep verts
//! {7,11} split by a pierce point 0.0034 from vert 11 → extra triangle
//! [7, 11, J] with directed edges (7,11) fwd=1/rev=0, (7,J) fwd=1/rev=2,
//! (11,J) fwd=2/rev=1). Production survives only by downstream luck.

use super::p3a_edge_overrides::closed_conformal_2_manifold;
use crate::*;
use std::collections::BTreeMap;

/// The F0084 operand-B topology at the offending chain step, bit-exact:
/// 16 vertices, 48 per-loop-copy `LineSegment` edges, 10 planar faces.
pub(crate) fn f0084_live_b_octagon_prism() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let pts: [[f64; 3]; 16] = [
        [0.17821361567347324, 0.2512682224941413, 1.338121791977246],
        [
            -0.3547855941529841,
            -0.055854501690931584,
            1.338121791977246,
        ],
        [-0.11843381648117289, -0.4660335630886844, 1.264827377330174],
        [
            0.41456539334528514,
            -0.15891083890361143,
            1.2648273773301737,
        ],
        [0.3371699834110933, -0.024594199037325966, 1.288828259927188],
        [0.254974826511139, -0.07195637150687396, 1.288828259927188],
        [0.17341386870771086, 0.0695894101583078, 1.3141209093802315],
        [0.2556090256076652, 0.11695158262785568, 1.3141209093802315],
        [-0.13782411931752303, -0.432382467691906, 1.013977331547628],
        [-0.3741758969893343, -0.02220340629415321, 1.0872717461947],
        [0.1588233128371231, 0.2849193178909197, 1.0872717461947001],
        [0.2362187227713151, 0.15060267802463398, 1.0632708635976857],
        [0.15402356587136073, 0.10324050555508611, 1.0632708635976857],
        [
            0.23558452367478883,
            -0.03830527611009561,
            1.0379782141446423,
        ],
        [0.3177796805747431, 0.009056896359452383, 1.0379782141446423],
        [0.395175090508935, -0.1252597435068331, 1.013977331547628],
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
                [0.07638826085308693, -0.132568772899324, 0.988225861863475],
                -1.302669669392006,
            ),
            (0..8).collect(),
        ),
        face(
            plane(
                [
                    -0.07638826085308671,
                    0.13256877289932392,
                    -0.988225861863475,
                ],
                1.0488308907655048,
            ),
            (8..16).collect(),
        ),
        face(
            plane(
                [
                    -0.8664506137022646,
                    -0.49926279053717687,
                    1.1412710877550702e-16,
                ],
                -0.3352902701648538,
            ),
            vec![16, 17, 18, 19],
        ),
        face(
            plane(
                [
                    0.4933844014749654,
                    -0.8562489044880573,
                    -0.15300211091416915,
                ],
                -0.14708607151684364,
            ),
            vec![20, 21, 22, 23],
        ),
        face(
            plane(
                [0.866450613702264, 0.4992627905371779, 0.0],
                -0.2798621706061218,
            ),
            vec![24, 25, 26, 27],
        ),
        face(
            plane(
                [-0.4933844014749662, 0.856248904488057, 0.1530021109141693],
                -0.009780277931617315,
            ),
            vec![28, 29, 30, 31],
        ),
        face(
            plane(
                [
                    0.8664506137022641,
                    0.4992627905371778,
                    -4.1340467556557955e-17,
                ],
                -0.18499795607375286,
            ),
            vec![32, 33, 34, 35],
        ),
        face(
            plane(
                [0.493384401474965, -0.8562489044880577, -0.15300211091416904],
                0.17508943152383197,
            ),
            vec![36, 37, 38, 39],
        ),
        face(
            plane(
                [0.866450613702264, 0.4992627905371779, 0.0],
                -0.2798621706061218,
            ),
            vec![40, 41, 42, 43],
        ),
        face(
            plane(
                [-0.4933844014749658, 0.8562489044880571, 0.15300211091416924],
                -0.33195578097229317,
            ),
            vec![44, 45, 46, 47],
        ),
    ];
    (verts, edges, faces)
}

/// The op's actual junction override payload (edge pierce points fanned to
/// both per-loop copies + face-interior partner points), captured verbatim
/// via `YANG_JUNCTION_MINT_PROBE=v`.
#[allow(clippy::type_complexity)]
pub(crate) fn f0084_live_b_overrides() -> (BTreeMap<u32, Vec<Point3>>, BTreeMap<u32, Vec<Point3>>) {
    let mut eo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    eo.insert(
        10,
        vec![Point3::new(
            0.2140340651344644,
            0.18910326446637987,
            1.0701504881626276,
        )],
    );
    eo.insert(
        14,
        vec![Point3::new(
            0.3310130422763537,
            -0.01390907383706022,
            1.0338744519081706,
        )],
    );
    let e15 = vec![
        Point3::new(0.33086971611907556, -0.16231353532912313, 1.013977331547628),
        Point3::new(0.26216353692342975, -0.20190314171856782, 1.013977331547628),
        Point3::new(0.23335438250033808, -0.21850344022702006, 1.013977331547628),
        Point3::new(0.031550073105964305, -0.3347863316663289, 1.013977331547628),
    ];
    eo.insert(15, e15.clone());
    eo.insert(22, e15);
    eo.insert(
        26,
        vec![Point3::new(
            0.3310130422763537,
            -0.01390907383706022,
            1.0338744519081706,
        )],
    );
    let e27 = vec![Point3::new(
        0.31863820400429704,
        0.007566963269076388,
        1.049084829829121,
    )];
    eo.insert(27, e27.clone());
    eo.insert(29, e27);
    let e31 = vec![Point3::new(
        0.23627127197838133,
        -0.039497100336504484,
        1.0468625954176523,
    )];
    eo.insert(31, e31.clone());
    eo.insert(33, e31);
    let e35 = vec![Point3::new(
        0.1541071529735702,
        0.10309544348080445,
        1.0643522200155322,
    )];
    eo.insert(35, e35.clone());
    eo.insert(37, e35);
    let e39 = vec![Point3::new(
        0.236474084999486,
        0.1501595070863852,
        1.0665744544270008,
    )];
    eo.insert(39, e39.clone());
    eo.insert(41, e39);
    eo.insert(
        42,
        vec![Point3::new(
            0.2140340651344644,
            0.18910326446637987,
            1.0701504881626276,
        )],
    );
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(
        1,
        vec![
            Point3::new(0.18713382926150413, 0.18889005052231614, 1.072201230611446),
            Point3::new(
                -0.16613463574192383,
                -0.33635320032330285,
                1.0290478471308835,
            ),
            Point3::new(
                0.23337019433802744,
                -0.20213136061465503,
                1.0161723951954755,
            ),
        ],
    );
    fo.insert(
        3,
        vec![
            Point3::new(0.3314160813615079, -0.16499216237008968, 1.0307296465462215),
            Point3::new(
                0.032677247145866395,
                -0.3356900765592237,
                1.0226697649759433,
            ),
            Point3::new(0.2338148660814896, -0.22076102119410745, 1.028096395873146),
            Point3::new(
                0.26410035347204464,
                -0.20345604127295874,
                1.0289134889782554,
            ),
        ],
    );
    fo.insert(
        4,
        vec![Point3::new(
            0.331416081361508,
            -0.014608532056852974,
            1.0463649043818308,
        )],
    );
    fo.insert(
        8,
        vec![Point3::new(
            0.2141050855656329,
            0.1889800113473058,
            1.0713359659902826,
        )],
    );
    (eo, fo)
}

/// RED (pre-fix): the junction-inserted rebuild of a valid 2-manifold
/// octagon prism must emit a closed conformal 2-manifold mesh. The live
/// payload mints an extra near-collinear sliver triangle over a split
/// polyline (directed-edge imbalance fwd=1/rev=2 + open edges).
#[test]
fn junction_inserted_octagon_prism_stage1_mesh_is_2_manifold() {
    let (verts, edges, faces) = f0084_live_b_octagon_prism();
    let (eo, fo) = f0084_live_b_overrides();
    let tess = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &eo, &fo, None)
        .expect("stage1 with junction overrides");
    // Diagnostic dump on failure: every imbalanced directed edge + the
    // owning face of each triangle using it.
    if !closed_conformal_2_manifold(&tess.tris) {
        let mut dir: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in &tess.tris {
            for k in 0..3 {
                *dir.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        for (&(s, e), &fwd) in &dir {
            if s > e {
                continue;
            }
            let rev = dir.get(&(e, s)).copied().unwrap_or(0);
            if fwd != rev {
                eprintln!("imbalanced ({s},{e}) fwd={fwd} rev={rev}");
                for (ti, t) in tess.tris.iter().enumerate() {
                    if t.contains(&s) && t.contains(&e) {
                        let face = tess
                            .face_tri_ranges
                            .iter()
                            .position(|r| r.contains(&ti))
                            .map(|f| f as i64)
                            .unwrap_or(-1);
                        eprintln!("  tri {ti} {t:?} face {face}");
                    }
                }
            }
        }
        panic!(
            "junction-inserted Stage-1 mesh must be a closed conformal 2-manifold; \
             got {} tris",
            tess.tris.len()
        );
    }
}

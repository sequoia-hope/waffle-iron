//! §4.4.1 mesh-level curve-chain decimation (`stage4_chain_decimate`, F2 / N81;
//! Yang et al. 2025 §4.3.4 / §4.4.1,
//! `refs/text/yang2025_hybrid_boolean.txt:536-571`): redundant interior
//! vertices of `LineSegment` / `SurfacePair` intersection runs (and
//! sub-resolution collinear straight-edge subdivisions) are dropped inside
//! Stage 4 with conformal per-patch CDT updates; bent/coarse runs and
//! junctions stay untouched; no cycle falls below three edges; kept piece
//! boundaries are invariant under `u32` vertex renumbering.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;

use crate::brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationSource, TriangleAttribution,
    TriangleAttributionMap, TriangleSideLabel, TriangleSideLabelMap,
};
use crate::geom::{Curve, Surface};
use crate::stage4_chain_decimate::decimate_patch_chains_441;
use crate::stage4_construct::directed_boundary;
use crate::stage4_correct::{
    compact_unreferenced_verts, compute_phase_a, compute_phase_a_with_side_labels,
};
use crate::stage5_seam_merge::point_key;
use crate::stage5_topology::emit_topology;

fn single_plane_brep(normal: [f64; 3], d: f64, tri_pts: [[f64; 3]; 3]) -> BRep {
    let surface = Surface::Plane {
        normal: Vector3::new(normal[0], normal[1], normal[2]),
        d,
    };
    BRep::new(
        tri_pts
            .into_iter()
            .map(|c| BRepVertex {
                point: Point3::new(c[0], c[1], c[2]),
            })
            .collect(),
        vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ],
        vec![BRepFace {
            surface,
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }],
    )
    .expect("valid single-plane brep")
}

fn two_perp_plane_brep() -> BRep {
    let z_plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let y_plane = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    BRep::new(
        vec![
            BRepVertex {
                point: Point3::new(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 0.0, -1.0),
            },
        ],
        vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ],
        vec![
            BRepFace {
                surface: z_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: y_plane,
                outer_loop: vec![0, 3, 4],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ],
    )
    .expect("valid two-plane brep")
}

/// Build two perpendicular mesh patches sharing a dense run `0..n` along `+x`:
/// - Patch 0 lies in `z = 0` (`y >= 0`) with far corners `n` `(x_end, 1, 0)` and `n+1` `(0, 1, 0)`.
/// - Patch 1 lies in `y = 0` (`z <= 0`) with far corners `n+2` `(x_end, 0, -1)` and `n+3` `(0, 0, -1)`.
///
/// When `cross_input` is `true`, Patch 0 is `(A, 0)` and Patch 1 is `(B, 0)` (an `A×B`
/// intersection run); when `false`, Patch 0 is `(A, 0)` and Patch 1 is `(A, 1)` (a
/// same-operand straight crease).
#[allow(clippy::type_complexity)]
fn shared_mesh_run(
    n: usize,
    s: f64,
    cross_input: bool,
) -> (
    Mesh,
    TriangleAttributionMap,
    TriangleSideLabelMap,
    BRep,
    BRep,
) {
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| Point3::new(s * i as f64, 0.0, 0.0))
        .collect();
    let x_end = s * (n - 1) as f64;
    let c_a0 = n as u32;
    let c_a1 = (n + 1) as u32;
    let c_b0 = (n + 2) as u32;
    let c_b1 = (n + 3) as u32;
    verts.push(Point3::new(x_end, 1.0, 0.0));
    verts.push(Point3::new(0.0, 1.0, 0.0));
    verts.push(Point3::new(x_end, 0.0, -1.0));
    verts.push(Point3::new(0.0, 0.0, -1.0));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut attrs: Vec<Option<TriangleAttribution>> = Vec::new();
    let mut labels: Vec<TriangleSideLabel> = Vec::new();

    let attr_p0 = Some(TriangleAttribution {
        input: InputId::A,
        face: 0,
    });
    let attr_p1 = if cross_input {
        Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        })
    } else {
        Some(TriangleAttribution {
            input: InputId::A,
            face: 1,
        })
    };
    let lbl_p0 = TriangleSideLabel {
        cherchi_patch: 0,
        inside: [false, false],
        surface: [true, false],
        sheet_opposite: None,
    };
    let lbl_p1 = if cross_input {
        TriangleSideLabel {
            cherchi_patch: 1,
            inside: [false, false],
            surface: [false, true],
            sheet_opposite: None,
        }
    } else {
        TriangleSideLabel {
            cherchi_patch: 1,
            inside: [false, false],
            surface: [true, false],
            sheet_opposite: None,
        }
    };

    // Patch 0 (`z = 0`, outward `+z`): cycle `0 -> 1 -> ... -> n-1 -> c_a0 -> c_a1 -> 0`.
    for i in 0..(n - 1) as u32 {
        tris.push([i, i + 1, c_a1]);
        attrs.push(attr_p0);
        labels.push(lbl_p0);
    }
    tris.push([(n - 1) as u32, c_a0, c_a1]);
    attrs.push(attr_p0);
    labels.push(lbl_p0);

    // Patch 1 (`y = 0`): cycle `(n-1) -> ... -> 0 -> c_b1 -> c_b0 -> (n-1)` so its
    // shared boundary edge `(i+1 -> i)` pairs anti-parallel with Patch 0's `(i -> i+1)`.
    for i in 0..(n - 1) as u32 {
        tris.push([i + 1, i, c_b1]);
        attrs.push(attr_p1);
        labels.push(lbl_p1);
    }
    tris.push([c_b0, (n - 1) as u32, c_b1]);
    attrs.push(attr_p1);
    labels.push(lbl_p1);

    let mesh = Mesh::new(verts, tris);
    let mut attribution = TriangleAttributionMap::empty();
    attribution.attributions = attrs;
    let side_labels = TriangleSideLabelMap { labels };

    let (brep_a, brep_b) = if cross_input {
        (
            single_plane_brep(
                [0.0, 0.0, 1.0],
                0.0,
                [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            ),
            single_plane_brep(
                [0.0, 1.0, 0.0],
                0.0,
                [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
            ),
        )
    } else {
        (
            two_perp_plane_brep(),
            single_plane_brep(
                [1.0, 0.0, 0.0],
                5.0,
                [[5.0, 0.0, 0.0], [5.0, 1.0, 0.0], [5.0, 0.0, 1.0]],
            ),
        )
    };
    (mesh, attribution, side_labels, brep_a, brep_b)
}

fn loop_chains(edges: &[BRepEdge], lp: &[u32]) {
    for (i, &ei) in lp.iter().enumerate() {
        let e = &edges[ei as usize];
        let nx = &edges[lp[(i + 1) % lp.len()] as usize];
        assert_eq!(e.end, nx.start, "loop must chain start->end");
    }
}

fn undirected_point_pieces(
    verts: &[Point3],
    edges: &[BRepEdge],
    lp: &[u32],
) -> Vec<([u64; 3], [u64; 3])> {
    let mut out: Vec<([u64; 3], [u64; 3])> = lp
        .iter()
        .map(|&ei| {
            let e = &edges[ei as usize];
            let (ka, kb) = (
                point_key(verts[e.start as usize]),
                point_key(verts[e.end as usize]),
            );
            if ka <= kb {
                (ka, kb)
            } else {
                (kb, ka)
            }
        })
        .collect();
    out.sort_unstable();
    out
}

/// A 40-vertex `A×B` intersection run at `5e-5` spacing (`2e-3` long; R0085
/// density) decimates inside §4.4.1: `mesh.tris` updates conformally on both
/// owner patches, unreferenced dropped vertices compact cleanly, both emitted
/// faces share identical run pieces with interior spacing `>= d_p * 10^3`, and
/// every surviving vertex carries a valid `TessellationSource`.
#[test]
fn dense_line_run_decimates_twin_conformant() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(40, 5e-5, true);
    let outer_before = directed_boundary(&mesh.tris);
    let (infos, _inc, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert!(st.verts_dropped > 0, "dense run must drop vertices: {st:?}");
    assert_eq!(st.declined_floor, 0);
    assert_eq!(
        directed_boundary(&mesh.tris),
        outer_before,
        "conformal outer boundary must hold"
    );

    let mut relocations = Vec::new();
    compact_unreferenced_verts(&mut mesh, &mut relocations, &side_labels.labels);
    assert!(
        mesh.verts.len() < 44,
        "dropped vertices must compact out of mesh"
    );

    let (infos2, _inc2, curves2) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a post-decimate");
    let (bverts, edges, faces, sources, _) = emit_topology(
        &mesh,
        &infos2,
        &curves2,
        &relocations,
        BoolOp::Union,
        (&a.vertices, &a.faces, &a.edges),
        (&b.vertices, &b.faces, &b.edges),
    )
    .expect("emit_topology");

    loop_chains(&edges, &faces[0].outer_loop);
    loop_chains(&edges, &faces[1].outer_loop);

    let on_axis = |k: [u64; 3]| k[1] == 0.0f64.to_bits() && k[2] == 0.0f64.to_bits();
    let pa: Vec<_> = undirected_point_pieces(&mesh.verts, &edges, &faces[0].outer_loop)
        .into_iter()
        .filter(|&(u, v)| on_axis(u) && on_axis(v))
        .collect();
    let pb: Vec<_> = undirected_point_pieces(&mesh.verts, &edges, &faces[1].outer_loop)
        .into_iter()
        .filter(|&(u, v)| on_axis(u) && on_axis(v))
        .collect();
    assert_eq!(pa, pb, "both owners must emit identical decimated pieces");

    let mut xs: Vec<f64> = pa
        .iter()
        .flat_map(|&(u, v)| [f64::from_bits(u[0]), f64::from_bits(v[0])])
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|x, y| (*x - *y).abs() < 1e-15);
    assert!((xs[0] - 0.0).abs() < 1e-15);
    assert!((*xs.last().unwrap() - 39.0 * 5e-5).abs() < 1e-15);
    let dp = cad_primitives::TAU_MODEL * (1.0 + 2e-3);
    for w in xs.windows(2) {
        if (w[1] - 39.0 * 5e-5).abs() > 1e-12 {
            assert!(
                w[1] - w[0] >= dp * 1e3,
                "kept spacing {:e} below d_p*10^3",
                w[1] - w[0]
            );
        }
    }
    assert_eq!(bverts.len(), mesh.verts.len());
    assert_eq!(sources.len(), mesh.verts.len());
    for (vi, src) in sources.iter().enumerate() {
        assert_eq!(*src, TessellationSource::BRepVertex(vi as u32));
    }
}

/// Same-operand straight crease subdivision run (`intersection_curves` empty)
/// decimates via the strict `TAU_WORK` collinearity class and updates both
/// adjacent patches conformally.
#[test]
fn collinear_operand_subdivision_run_decimates() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(40, 5e-5, false);
    let outer_before = directed_boundary(&mesh.tris);
    let (infos, _inc, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    assert!(curves.is_empty(), "same-operand crease has no A×B curves");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert!(st.verts_dropped > 0, "{st:?}");
    assert_eq!(directed_boundary(&mesh.tris), outer_before);
}

/// An operand run with a real `1e-9` bend (above `TAU_WORK`) is untouched
/// byte-for-byte.
#[test]
fn bent_operand_run_is_untouched() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(40, 5e-5, false);
    for (i, v) in mesh.verts.iter_mut().enumerate().take(39).skip(1) {
        let bump = if i % 2 == 0 { 1e-9 } else { -1e-9 };
        *v = Point3::new(v.x(), 0.0, bump);
    }
    let tris0 = mesh.tris.clone();
    let (infos, _inc, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// A collinear operand run sparser than `d_p * 10^3` (`1e-2` spacing) is untouched.
#[test]
fn coarse_operand_run_is_untouched() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(12, 1e-2, false);
    let tris0 = mesh.tris.clone();
    let (infos, _inc, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// A sparse `A×B` intersection run (`1e-2` spacing) is a byte-identical no-op.
#[test]
fn sparse_run_is_byte_identical() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(12, 1e-2, true);
    let tris0 = mesh.tris.clone();
    let (infos, _inc, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// Loop floor: a patch bounded by a 2-chord dense run `[0, 1, 2]` and ONE
/// closing edge `2 -> 0` (a 3-edge cycle) declines dropping `1`, keeping both
/// owner cycles at `>= 3` edges.
#[test]
fn loop_floor_keeps_three_edges() {
    let s = 5e-5;
    let verts = vec![
        Point3::new(0.0, 0.0, 0.0),     // 0
        Point3::new(s, 1e-13, 0.0),     // 1 (tiny in-plane offset so tri [0,1,2] is non-degenerate)
        Point3::new(2.0 * s, 0.0, 0.0), // 2
        Point3::new(s, 0.0, -1.0),      // 3 (Patch B's far corner -> 4-edge cycle on B)
    ];
    let tris = vec![
        [0, 1, 2], // Patch A: 3-edge cycle 0->1->2->0
        [1, 0, 3], // Patch B: 4-edge cycle 2->1->0->3->2
        [2, 1, 3],
    ];
    let mut mesh = Mesh::new(verts, tris);
    let tris0 = mesh.tris.clone();
    let mut attribution = TriangleAttributionMap::empty();
    attribution.attributions = vec![
        Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        }),
        Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        }),
        Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        }),
    ];
    let mut side_labels = TriangleSideLabelMap {
        labels: vec![
            TriangleSideLabel {
                cherchi_patch: 0,
                inside: [false, false],
                surface: [true, false],
                sheet_opposite: None,
            },
            TriangleSideLabel {
                cherchi_patch: 1,
                inside: [false, false],
                surface: [false, true],
                sheet_opposite: None,
            },
            TriangleSideLabel {
                cherchi_patch: 1,
                inside: [false, false],
                surface: [false, true],
                sheet_opposite: None,
            },
        ],
    };
    let a = single_plane_brep(
        [0.0, 0.0, 1.0],
        0.0,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    );
    let b = single_plane_brep(
        [0.0, 1.0, 0.0],
        0.0,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
    );
    let mut curves = BTreeMap::new();
    curves.insert((0, 1), Curve::LineSegment);
    curves.insert((1, 2), Curve::LineSegment);
    let (infos, _, _) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    );
    assert_eq!(
        st.verts_dropped, 0,
        "3-edge cycle must decline dropping v1: {st:?}"
    );
    assert!(st.declined_floor >= 1, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// F2 / N81: kept piece boundaries are invariant under arbitrary `u32` vertex
/// renumbering, AND 4-owner full-arrangement `A×B` seams decimate conformally
/// across all four sub-patches (`A_out`, `A_in`, `B_out`, `B_in`).
#[test]
fn decimation_is_invariant_under_vertex_renumbering_and_supports_4_owner_full_arrangement() {
    let n = 30usize;
    let s = 5e-5;
    let x_end = s * (n - 1) as f64;
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| Point3::new(s * i as f64, 0.0, 0.0))
        .collect();
    // 4 sub-patch far-corner pairs: A_pos (+y), A_neg (-y), B_pos (+z), B_neg (-z).
    let base = n as u32;
    verts.extend([
        Point3::new(x_end, 1.0, 0.0),  // base+0: A_out
        Point3::new(0.0, 1.0, 0.0),    // base+1: A_out
        Point3::new(x_end, -1.0, 0.0), // base+2: A_in
        Point3::new(0.0, -1.0, 0.0),   // base+3: A_in
        Point3::new(x_end, 0.0, 1.0),  // base+4: B_out
        Point3::new(0.0, 0.0, 1.0),    // base+5: B_out
        Point3::new(x_end, 0.0, -1.0), // base+6: B_in
        Point3::new(0.0, 0.0, -1.0),   // base+7: B_in
    ]);

    let mut tris = Vec::new();
    let mut attrs = Vec::new();
    let mut labels = Vec::new();
    let push_strip = |tris: &mut Vec<[u32; 3]>,
                      attrs: &mut Vec<Option<TriangleAttribution>>,
                      labels: &mut Vec<TriangleSideLabel>,
                      forward: bool,
                      c_end: u32,
                      c_start: u32,
                      att: TriangleAttribution,
                      lbl: TriangleSideLabel| {
        for i in 0..(n - 1) as u32 {
            let tri = if forward {
                [i, i + 1, c_start]
            } else {
                [i + 1, i, c_start]
            };
            tris.push(tri);
            attrs.push(Some(att));
            labels.push(lbl);
        }
        let close = if forward {
            [(n - 1) as u32, c_end, c_start]
        } else {
            [c_end, (n - 1) as u32, c_start]
        };
        tris.push(close);
        attrs.push(Some(att));
        labels.push(lbl);
    };

    // Operand A (`z=0`, normal `+z`): A_out (`y>=0`, forward `i -> i+1`), A_in (`y<=0`, backward `i+1 -> i`).
    push_strip(
        &mut tris,
        &mut attrs,
        &mut labels,
        true,
        base,
        base + 1,
        TriangleAttribution {
            input: InputId::A,
            face: 0,
        },
        TriangleSideLabel {
            cherchi_patch: 0,
            inside: [false, false],
            surface: [true, false],
            sheet_opposite: None,
        },
    );
    push_strip(
        &mut tris,
        &mut attrs,
        &mut labels,
        false,
        base + 2,
        base + 3,
        TriangleAttribution {
            input: InputId::A,
            face: 0,
        },
        TriangleSideLabel {
            cherchi_patch: 1,
            inside: [false, true],
            surface: [true, false],
            sheet_opposite: None,
        },
    );
    // Operand B (`y=0`, normal `+y`): B_out (`z<=0`, backward `i+1 -> i`), B_in (`z>=0`, forward `i -> i+1`).
    push_strip(
        &mut tris,
        &mut attrs,
        &mut labels,
        true,
        base + 4,
        base + 5,
        TriangleAttribution {
            input: InputId::B,
            face: 0,
        },
        TriangleSideLabel {
            cherchi_patch: 2,
            inside: [true, false],
            surface: [false, true],
            sheet_opposite: None,
        },
    );
    push_strip(
        &mut tris,
        &mut attrs,
        &mut labels,
        false,
        base + 6,
        base + 7,
        TriangleAttribution {
            input: InputId::B,
            face: 0,
        },
        TriangleSideLabel {
            cherchi_patch: 3,
            inside: [false, false],
            surface: [false, true],
            sheet_opposite: None,
        },
    );

    let a = single_plane_brep(
        [0.0, 0.0, 1.0],
        0.0,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    );
    let b = single_plane_brep(
        [0.0, 1.0, 0.0],
        0.0,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
    );

    // Build a vertex-renumbered twin with reversed/scrambled IDs.
    let nv = verts.len();
    let perm: Vec<u32> = (0..nv).map(|i| ((nv - 1 - i + 7) % nv) as u32).collect();
    let mut pverts = vec![Point3::new(0.0, 0.0, 0.0); nv];
    for (old, &new) in perm.iter().enumerate() {
        pverts[new as usize] = verts[old];
    }
    let ptris: Vec<[u32; 3]> = tris
        .iter()
        .map(|t| {
            [
                perm[t[0] as usize],
                perm[t[1] as usize],
                perm[t[2] as usize],
            ]
        })
        .collect();

    let mut mesh1 = Mesh::new(verts, tris);
    let mut attr1 = TriangleAttributionMap::empty();
    attr1.attributions = attrs.clone();
    let mut sl1 = TriangleSideLabelMap {
        labels: labels.clone(),
    };

    let mut mesh2 = Mesh::new(pverts, ptris);
    let mut attr2 = TriangleAttributionMap::empty();
    attr2.attributions = attrs;
    let mut sl2 = TriangleSideLabelMap { labels };

    let (infos1, _, curves1) = compute_phase_a_with_side_labels(
        &mesh1,
        &attr1,
        Some(&sl1),
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a 1");
    let (infos2, _, curves2) = compute_phase_a_with_side_labels(
        &mesh2,
        &attr2,
        Some(&sl2),
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a 2");

    let st1 = decimate_patch_chains_441(
        &mut mesh1, &mut attr1, &mut sl1, &infos1, &curves1, &a, &b, true,
    );
    let st2 = decimate_patch_chains_441(
        &mut mesh2, &mut attr2, &mut sl2, &infos2, &curves2, &a, &b, true,
    );
    assert!(st1.verts_dropped > 0, "{st1:?}");
    assert_eq!(
        st1.verts_dropped, st2.verts_dropped,
        "drop count must match across permutation"
    );

    let surviving_axis_pts = |m: &Mesh| -> BTreeSet<[u64; 3]> {
        m.tris
            .iter()
            .flatten()
            .map(|&v| point_key(m.verts[v as usize]))
            .filter(|k| k[1] == 0.0f64.to_bits() && k[2] == 0.0f64.to_bits())
            .collect()
    };
    assert_eq!(
        surviving_axis_pts(&mesh1),
        surviving_axis_pts(&mesh2),
        "kept 3D vertices along the 4-owner seam must be identical regardless of vertex numbering"
    );
}

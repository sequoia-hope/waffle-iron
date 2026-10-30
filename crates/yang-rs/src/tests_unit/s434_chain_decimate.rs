//! §4.4.1 mesh-level curve-chain decimation (`stage4_chain_decimate`, F2 / N81;
//! Yang et al. 2025 §4.3.4 / §4.4.1,
//! `refs/text/yang2025_hybrid_boolean.txt:575-593, 605-609, 546-572`):
//! redundant interior vertices of `LineSegment` / `SurfacePair` intersection
//! runs (and sub-resolution collinear straight-edge subdivisions, ledger N105)
//! are dropped inside Stage 4 via forward §4.3.4 with conformal per-patch CDT
//! updates; bent/coarse runs and junctions stay untouched; no cycle falls below
//! three edges; kept piece boundaries are invariant under `u32` vertex
//! renumbering (and walk direction / world-frame rotation when operand-vertex
//! anchored); `Surface::Sphere` declines are counted explicitly; broken mesh
//! invariants fail with a typed `YangError::Stage4ChainDecimateFailed`.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;

use crate::brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationSource, TriangleAttribution,
    TriangleAttributionMap, TriangleSideLabel, TriangleSideLabelMap,
};
use crate::errors::{ChainDecimateError, YangError};
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
/// density) decimates inside §4.4.1 via forward §4.3.4: `mesh.tris` updates
/// conformally on both owner patches, unreferenced dropped vertices compact
/// cleanly, both emitted faces share identical run pieces with interior spacing
/// `>= d_p * 10^3`, every emitted piece is still `Curve::LineSegment`, and every
/// surviving vertex carries a valid `TessellationSource`.
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
    let before_edges = 2 * 42usize;
    let st = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos,
        &curves,
        &a,
        &b,
        false,
    )
    .expect("decimate");
    assert!(st.verts_dropped > 0, "dense run must drop vertices: {st:?}");
    assert_eq!(st.declined_loop_floor, 0);
    assert!(st.edges_after < st.edges_before);
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

    assert!(edges.len() < before_edges);
    loop_chains(&edges, &faces[0].outer_loop);
    loop_chains(&edges, &faces[1].outer_loop);

    // Every emitted piece is still a LineSegment.
    for e in &edges {
        assert_eq!(e.curve, Curve::LineSegment);
    }

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
/// decimates via the strict `TAU_WORK` collinearity class (ledger N105),
/// updates both adjacent patches conformally, and emits identical run pieces on
/// both owner faces.
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
    )
    .expect("decimate");
    assert!(st.verts_dropped > 0, "{st:?}");
    assert_eq!(directed_boundary(&mesh.tris), outer_before);

    let mut relocations = Vec::new();
    compact_unreferenced_verts(&mut mesh, &mut relocations, &side_labels.labels);
    let (infos2, _inc2, curves2) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a post-decimate");
    let (_bverts, edges, faces, _sources, _) = emit_topology(
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
    for e in &edges {
        assert_eq!(e.curve, Curve::LineSegment);
    }
    let on_axis = |k: [u64; 3]| k[1] == 0.0f64.to_bits() && k[2] == 0.0f64.to_bits();
    let pa: Vec<_> = undirected_point_pieces(&mesh.verts, &edges, &faces[0].outer_loop)
        .into_iter()
        .filter(|&(u, v)| on_axis(u) && on_axis(v))
        .collect();
    let pb: Vec<_> = undirected_point_pieces(&mesh.verts, &edges, &faces[1].outer_loop)
        .into_iter()
        .filter(|&(u, v)| on_axis(u) && on_axis(v))
        .collect();
    assert!(!pa.is_empty() && pa.len() < 39);
    assert_eq!(pa, pb, "both owners must emit identical decimated pieces");
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
    )
    .expect("decimate");
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
    )
    .expect("decimate");
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
    )
    .expect("decimate");
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// Build a 2-stretch lens patch (Patch A) bounded by an `(n - 1)`-edge dense
/// bowed run `0 -> 1 -> ... -> n-1` (`y_i = -bulge * sin(pi * i / (n - 1)) <= 0`
/// in `z = 0`) and ONE straight closing edge `(n - 1) -> 0`, paired along the
/// bowed run with collar Patch B below (`y <= y_i`, `z = 0`, distinct operand).
#[allow(clippy::type_complexity)]
fn lens_run_mesh(
    n: usize,
    s: f64,
    bulge: f64,
) -> (
    Mesh,
    TriangleAttributionMap,
    TriangleSideLabelMap,
    BRep,
    BRep,
    BTreeMap<(u32, u32), Curve>,
) {
    let x_end = s * (n - 1) as f64;
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| {
            let t = std::f64::consts::PI * (i as f64) / ((n - 1) as f64);
            Point3::new(s * i as f64, -bulge * t.sin(), 0.0)
        })
        .collect();
    let c_b0 = n as u32;
    let c_b1 = (n + 1) as u32;
    verts.push(Point3::new(x_end, -1.0, 0.0));
    verts.push(Point3::new(0.0, -1.0, 0.0));

    let mut tris = Vec::new();
    let mut attrs = Vec::new();
    let mut labels = Vec::new();

    let attr_a = Some(TriangleAttribution {
        input: InputId::A,
        face: 0,
    });
    let attr_b = Some(TriangleAttribution {
        input: InputId::B,
        face: 0,
    });
    let lbl_a = TriangleSideLabel {
        cherchi_patch: 0,
        inside: [false, false],
        surface: [true, false],
        sheet_opposite: None,
    };
    let lbl_b = TriangleSideLabel {
        cherchi_patch: 1,
        inside: [false, false],
        surface: [false, true],
        sheet_opposite: None,
    };

    // Patch A: convex lens fan `[0, i, i + 1]` for `i = 1..(n - 1)`.
    // Boundary cycle: `0 -> 1 -> ... -> (n - 1) -> 0` (the dense run + 1 closing edge).
    for i in 1..(n - 1) as u32 {
        tris.push([0, i, i + 1]);
        attrs.push(attr_a);
        labels.push(lbl_a);
    }

    // Patch B: collar below the bow (`[i + 1, i, c_b1]` + `[c_b0, n - 1, c_b1]`).
    for i in 0..(n - 1) as u32 {
        tris.push([i + 1, i, c_b1]);
        attrs.push(attr_b);
        labels.push(lbl_b);
    }
    tris.push([c_b0, (n - 1) as u32, c_b1]);
    attrs.push(attr_b);
    labels.push(lbl_b);

    let mesh = Mesh::new(verts, tris);
    let mut attribution = TriangleAttributionMap::empty();
    attribution.attributions = attrs;
    let side_labels = TriangleSideLabelMap { labels };

    let a = single_plane_brep(
        [0.0, 0.0, 1.0],
        0.0,
        [[0.0, 0.0, 0.0], [x_end, 0.0, 0.0], [0.0, 1.0, 0.0]],
    );
    // Opposite plane normal `[0, 0, -1]` (`reversed: false`) so `a` and `b` both lie on
    // `z = 0` without merging under `unit_planes_coincide` when `use_side_labels = false`.
    let b = single_plane_brep(
        [0.0, 0.0, -1.0],
        0.0,
        [[0.0, 0.0, 0.0], [x_end, -1.0, 0.0], [0.0, -1.0, 0.0]],
    );
    let curves: BTreeMap<(u32, u32), Curve> = (0..(n - 1) as u32)
        .map(|i| ((i, i + 1), Curve::LineSegment))
        .collect();
    (mesh, attribution, side_labels, a, b, curves)
}

/// Loop floor on a 2-stretch lens face (bounded by a dense run and ONE closing
/// edge):
/// 1. `40`-vertex run at `5e-5` spacing shrinks to `< 39` and `>= 2` run pieces
///    (so both owner loops stay `>= 3` edges and chain cleanly);
/// 2. `6`-vertex run at `5e-5` spacing shrinks down to **2** run pieces = **3**
///    loop edges on Patch A (the exact floor);
/// 3. `40`-vertex sub-band run at `2e-6` spacing (where forward §4.3.4 proposes
///    collapsing the entire 39-edge run into **1** chord, which would drop
///    Patch A's loop to `2` edges) is declined with `declined_loop_floor >= 1`,
///    never shrinking the run to `1` piece.
#[test]
fn loop_floor_keeps_three_edges() {
    // (1) 40-vertex run at 5e-5 spacing: shrinks to >= 2 run pieces, loop >= 3 edges.
    {
        let (mut mesh, mut attribution, mut side_labels, a, b, curves) =
            lens_run_mesh(40, 5e-5, 1e-6);
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
        )
        .expect("decimate");
        assert!(
            st.verts_dropped > 0,
            "40-vertex lens should decimate: {st:?}"
        );
        let mut relocations = Vec::new();
        compact_unreferenced_verts(&mut mesh, &mut relocations, &side_labels.labels);
        let (infos2, _, _) = compute_phase_a(
            &mesh,
            &attribution,
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 2");
        let (_, edges, faces, _, _) = emit_topology(
            &mesh,
            &infos2,
            &BTreeMap::new(),
            &relocations,
            BoolOp::Union,
            (&a.vertices, &a.faces, &a.edges),
            (&b.vertices, &b.faces, &b.edges),
        )
        .expect("emit");
        let mut lens = [faces[0].outer_loop.len(), faces[1].outer_loop.len()];
        lens.sort_unstable();
        assert!(
            lens[0] >= 3 && lens[0] < 40,
            "lens face loop must stay >= 3 and shrink from 40: {lens:?}"
        );
        assert!(
            lens[1] >= 3,
            "collar face loop fell below 3 edges: {lens:?}"
        );
        loop_chains(&edges, &faces[0].outer_loop);
        loop_chains(&edges, &faces[1].outer_loop);
    }

    // (2) 6-vertex run at 5e-5 spacing: shrinks to 2 run pieces = exact 3-edge floor on Patch A.
    {
        let (mut mesh, mut attribution, mut side_labels, a, b, curves) =
            lens_run_mesh(6, 5e-5, 1e-6);
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
        )
        .expect("decimate");
        assert_eq!(
            st.verts_dropped, 3,
            "6-vertex run shrinks to 2 pieces: {st:?}"
        );
        let mut relocations = Vec::new();
        compact_unreferenced_verts(&mut mesh, &mut relocations, &side_labels.labels);
        let (infos2, _, _) = compute_phase_a(
            &mesh,
            &attribution,
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 2");
        let (_, edges, faces, _, _) = emit_topology(
            &mesh,
            &infos2,
            &BTreeMap::new(),
            &relocations,
            BoolOp::Union,
            (&a.vertices, &a.faces, &a.edges),
            (&b.vertices, &b.faces, &b.edges),
        )
        .expect("emit");
        let mut lens = [faces[0].outer_loop.len(), faces[1].outer_loop.len()];
        lens.sort_unstable();
        assert_eq!(
            lens,
            [3, 5],
            "lens face shrinks to 2 run pieces + 1 closing edge = 3-edge floor (collar = 5)"
        );
        loop_chains(&edges, &faces[0].outer_loop);
        loop_chains(&edges, &faces[1].outer_loop);
    }

    // (3) 40-vertex sub-band run at 2e-6 spacing: entire 39-edge run is redundant
    // on a single chord `0 -> 39`, which would reduce Patch A's loop to 2 edges;
    // loop floor declines the 1-piece collapse (`declined_loop_floor >= 1`).
    {
        let (mut mesh, mut attribution, mut side_labels, a, b, curves) =
            lens_run_mesh(40, 2e-6, 1e-8);
        let tris0 = mesh.tris.clone();
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
        )
        .expect("decimate");
        assert_eq!(
            st.verts_dropped, 0,
            "40-vertex lens must never collapse run to 1 piece: {st:?}"
        );
        assert!(st.declined_loop_floor >= 1, "{st:?}");
        assert_eq!(mesh.tris, tris0);
    }
}

/// Loop floor on a minimal 3-edge triangle cycle (`[0, 1, 2]` + closing edge
/// `2 -> 0`): declines dropping `1`.
#[test]
fn loop_floor_three_edge_triangle_stays_three_edges() {
    let s = 5e-5;
    let verts = vec![
        Point3::new(0.0, 0.0, 0.0),     // 0
        Point3::new(s, 1e-13, 0.0),     // 1
        Point3::new(2.0 * s, 0.0, 0.0), // 2
        Point3::new(s, 0.0, -1.0),      // 3
    ];
    let tris = vec![[0, 1, 2], [1, 0, 3], [2, 1, 3]];
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
    )
    .expect("decimate");
    assert_eq!(
        st.verts_dropped, 0,
        "3-edge cycle must decline dropping v1: {st:?}"
    );
    assert!(st.declined_loop_floor >= 1, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// Build an `m`-edge (`m` divisible by 4) closed `SurfacePair` loop of radius
/// `radius` in `z = 0` shared between inner disk Patch A (`0..m` around center
/// `m`) and holed square collar Patch B (`(-0.1,-0.1)..(0.1,0.1)`).
/// If `anchor_ring_idx` is `Some(k)`, Operand A's `BRep.vertices` includes the
/// ring vertex `p_k` as an authored operand `BRepVertex`.
#[allow(clippy::type_complexity)]
fn closed_ring_mesh(
    m: usize,
    radius: f64,
    anchor_ring_idx: Option<usize>,
) -> (
    Mesh,
    TriangleAttributionMap,
    TriangleSideLabelMap,
    BRep,
    BRep,
    BTreeMap<(u32, u32), Curve>,
) {
    assert_eq!(m % 4, 0);
    let q = m / 4;
    let mut verts: Vec<Point3> = (0..m)
        .map(|k| {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (m as f64);
            Point3::new(radius * th.cos(), radius * th.sin(), 0.0)
        })
        .collect();
    let center = m as u32;
    verts.push(Point3::new(0.0, 0.0, 0.0));
    let c0 = (m + 1) as u32;
    let c1 = (m + 2) as u32;
    let c2 = (m + 3) as u32;
    let c3 = (m + 4) as u32;
    verts.push(Point3::new(0.1, 0.1, 0.0));
    verts.push(Point3::new(-0.1, 0.1, 0.0));
    verts.push(Point3::new(-0.1, -0.1, 0.0));
    verts.push(Point3::new(0.1, -0.1, 0.0));

    let mut tris = Vec::new();
    let mut attrs = Vec::new();
    let mut labels = Vec::new();

    let attr_a = Some(TriangleAttribution {
        input: InputId::A,
        face: 0,
    });
    // Both patches belong to `(A, 0)` (`Plane +z`) split across the closed torus
    // intersection ring (`lbl_a` outside B, `lbl_b` inside B), so both patches
    // carry `+z` outward normal and `InputId::A`'s outer square boundary cancels
    // across the closed inner ring.
    let attr_b = attr_a;
    let lbl_a = TriangleSideLabel {
        cherchi_patch: 0,
        inside: [false, false],
        surface: [true, false],
        sheet_opposite: None,
    };
    let lbl_b = TriangleSideLabel {
        cherchi_patch: 1,
        inside: [false, true],
        surface: [true, false],
        sheet_opposite: None,
    };

    // Patch A: inner disk fan `[center, k, (k + 1) % m]`.
    for k in 0..m {
        tris.push([center, k as u32, ((k + 1) % m) as u32]);
        attrs.push(attr_a);
        labels.push(lbl_a);
    }

    // Patch B: 4-quadrant holed square collar (`[(k + 1) % m, k, c_quad]` + 4 corner bridges).
    let corners = [c0, c1, c2, c3];
    for quad in 0..4 {
        let cq = corners[quad];
        let c_next = corners[(quad + 1) % 4];
        let k_start = quad * q;
        let k_end = (quad + 1) * q;
        for k in k_start..k_end {
            tris.push([((k + 1) % m) as u32, k as u32, cq]);
            attrs.push(attr_b);
            labels.push(lbl_b);
        }
        tris.push([cq, c_next, (k_end % m) as u32]);
        attrs.push(attr_b);
        labels.push(lbl_b);
    }

    let mesh = Mesh::new(verts.clone(), tris);
    let mut attribution = TriangleAttributionMap::empty();
    attribution.attributions = attrs;
    let side_labels = TriangleSideLabelMap { labels };

    let a_tri = if let Some(ak) = anchor_ring_idx {
        let p = verts[ak];
        [[p.x(), p.y(), p.z()], [0.2, -0.2, 0.0], [-0.2, 0.2, 0.0]]
    } else {
        [[0.2, 0.0, 0.0], [0.0, 0.2, 0.0], [-0.2, -0.2, 0.0]]
    };
    let a = single_plane_brep([0.0, 0.0, 1.0], 0.0, a_tri);
    let b = single_plane_brep(
        [0.0, 0.0, 1.0],
        5.0,
        [[0.0, 0.0, 5.0], [1.0, 0.0, 5.0], [0.0, 1.0, 5.0]],
    );

    let pair_curve = Curve::SurfacePair {
        a: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        b: Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 0.5,
            minor_radius: 0.5 - radius,
        },
    };
    let curves: BTreeMap<(u32, u32), Curve> = (0..m)
        .map(|k| {
            let (u, v) = (k as u32, ((k + 1) % m) as u32);
            ((u.min(v), u.max(v)), pair_curve)
        })
        .collect();

    (mesh, attribution, side_labels, a, b, curves)
}

/// Closed `SurfacePair` loop (`72` edges at `R = 2e-4`) between inner disk
/// Patch A and holed collar Patch B decimates conformally (`72 -> 24` chords,
/// `48` vertices dropped) and emits twin-conformant closed loops on both faces.
#[test]
fn closed_chain_decimates_conformally() {
    let (mut mesh, mut attribution, mut side_labels, a, b, curves) =
        closed_ring_mesh(72, 2e-4, None);
    let outer_before = directed_boundary(&mesh.tris);
    let (infos, _, _) = compute_phase_a_with_side_labels(
        &mesh,
        &attribution,
        Some(&side_labels),
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
        true,
    )
    .expect("decimate");
    assert_eq!(st.runs_decimated, 1, "{st:?}");
    assert_eq!(
        st.verts_dropped, 48,
        "72-step circle at 5 deg steps must decimate to 24 chords (15 deg < 20 deg): {st:?}"
    );
    assert_eq!(
        directed_boundary(&mesh.tris),
        outer_before,
        "outer collar boundary must stay conformal"
    );

    let mut relocations = Vec::new();
    let remap = compact_unreferenced_verts(&mut mesh, &mut relocations, &side_labels.labels);
    let _ = remap;
    let (infos2, _, _) = compute_phase_a_with_side_labels(
        &mesh,
        &attribution,
        Some(&side_labels),
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a post-decimate");
    // Rebuildremapped `SurfacePair` curve map on surviving ring chords for `emit_topology`.
    let pair_curve = *curves.values().next().unwrap();
    let mut curves2 = BTreeMap::new();
    for info in &infos2 {
        for cyc in &info.cycles {
            for &(u, v) in cyc {
                let (pu, pv) = (mesh.verts[u as usize], mesh.verts[v as usize]);
                if pu.x().hypot(pu.y()) < 1e-2 && pv.x().hypot(pv.y()) < 1e-2 {
                    curves2.insert((u.min(v), u.max(v)), pair_curve);
                }
            }
        }
    }
    let (_, edges, faces, _, _) = emit_topology(
        &mesh,
        &infos2,
        &curves2,
        &relocations,
        BoolOp::Union,
        (&a.vertices, &a.faces, &a.edges),
        (&b.vertices, &b.faces, &b.edges),
    )
    .expect("emit_topology");

    let disk_fi = faces
        .iter()
        .position(|f| f.inner_loops.is_empty())
        .expect("disk face");
    let collar_fi = 1 - disk_fi;
    assert_eq!(faces[disk_fi].outer_loop.len(), 24);
    assert_eq!(faces[collar_fi].inner_loops.len(), 1);
    assert_eq!(faces[collar_fi].inner_loops[0].len(), 24);
    loop_chains(&edges, &faces[disk_fi].outer_loop);
    loop_chains(&edges, &faces[collar_fi].outer_loop);
    loop_chains(&edges, &faces[collar_fi].inner_loops[0]);
    assert_eq!(
        undirected_point_pieces(&mesh.verts, &edges, &faces[disk_fi].outer_loop),
        undirected_point_pieces(&mesh.verts, &edges, &faces[collar_fi].inner_loops[0]),
        "disk outer loop and collar inner hole loop must share identical decimated chords"
    );
}

/// Closed-chain decimation is invariant under arbitrary `u32` vertex
/// renumbering (both unanchored and operand-vertex-anchored), AND when anchored
/// at a surviving authored operand `BRepVertex` it is also invariant under 90°
/// world-frame rotation `(x, y, z) -> (-y, x, z)`.
#[test]
fn closed_chain_is_invariant_under_vertex_renumbering() {
    let ring_surviving_pts = |m: &Mesh| -> BTreeSet<[u64; 3]> {
        m.tris
            .iter()
            .flatten()
            .map(|&v| m.verts[v as usize])
            .filter(|p| {
                let r = p.x().hypot(p.y());
                r > 1e-5 && r < 1e-2
            })
            .map(point_key)
            .collect()
    };

    let permute_case = |mesh: &Mesh,
                        curves: &BTreeMap<(u32, u32), Curve>|
     -> (Mesh, BTreeMap<(u32, u32), Curve>) {
        let nv = mesh.verts.len();
        // Coprime affine permutation `(37 * i + 19) % 77` (gcd(37, 77) == 1).
        let perm: Vec<u32> = (0..nv).map(|i| ((37 * i + 19) % nv) as u32).collect();
        let mut pverts = vec![Point3::new(0.0, 0.0, 0.0); nv];
        for (old, &new) in perm.iter().enumerate() {
            pverts[new as usize] = mesh.verts[old];
        }
        let ptris: Vec<[u32; 3]> = mesh
            .tris
            .iter()
            .map(|t| {
                [
                    perm[t[0] as usize],
                    perm[t[1] as usize],
                    perm[t[2] as usize],
                ]
            })
            .collect();
        let pcurves: BTreeMap<(u32, u32), Curve> = curves
            .iter()
            .map(|(&(u, v), &c)| {
                let (pu, pv) = (perm[u as usize], perm[v as usize]);
                ((pu.min(pv), pu.max(pv)), c)
            })
            .collect();
        (Mesh::new(pverts, ptris), pcurves)
    };

    // (a) Unanchored closed ring (`0` surviving operand BRepVertices on ring):
    // vertex-renumbering-invariant.
    {
        let (mut m1, mut attr1, mut sl1, a, b, c1) = closed_ring_mesh(72, 2e-4, None);
        let (mut m2, c2) = permute_case(&m1, &c1);
        let mut attr2 = attr1.clone();
        let mut sl2 = sl1.clone();

        let (infos1, _, _) = compute_phase_a_with_side_labels(
            &m1,
            &attr1,
            Some(&sl1),
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 1");
        let (infos2, _, _) = compute_phase_a_with_side_labels(
            &m2,
            &attr2,
            Some(&sl2),
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 2");

        let st1 =
            decimate_patch_chains_441(&mut m1, &mut attr1, &mut sl1, &infos1, &c1, &a, &b, true)
                .expect("dec 1");
        let st2 =
            decimate_patch_chains_441(&mut m2, &mut attr2, &mut sl2, &infos2, &c2, &a, &b, true)
                .expect("dec 2");
        assert_eq!(st1.verts_dropped, 48);
        assert_eq!(st1.verts_dropped, st2.verts_dropped);
        assert_eq!(
            ring_surviving_pts(&m1),
            ring_surviving_pts(&m2),
            "unanchored closed ring kept 3D vertices must be invariant under vertex renumbering"
        );
    }

    // (b) Operand-vertex-anchored closed ring (`anchor_ring_idx = Some(11)`):
    // invariant under BOTH vertex renumbering AND 90-degree world-frame rotation.
    {
        let (mut m1, mut attr1, mut sl1, a, b, c1) = closed_ring_mesh(72, 2e-4, Some(11));
        let anchor_pt = m1.verts[11];
        let (mut m2, c2) = permute_case(&m1, &c1);
        let mut attr2 = attr1.clone();
        let mut sl2 = sl1.clone();

        // Build a +90-degree Z-rotated copy `(x, y, z) -> (-y, x, z)` (exact IEEE-754
        // bit-preserving sign/swap) of `m2` + operands `a`, `b`.
        let rot90 = |p: Point3| Point3::new(-p.y(), p.x(), p.z());
        let mut m3 = Mesh::new(
            m2.verts.iter().copied().map(rot90).collect(),
            m2.tris.clone(),
        );
        let mut attr3 = attr1.clone();
        let mut sl3 = sl1.clone();
        let a_rot = BRep::new(
            a.vertices
                .iter()
                .map(|v| BRepVertex {
                    point: rot90(v.point),
                })
                .collect(),
            a.edges.clone(),
            a.faces.clone(),
        )
        .expect("rotated a");
        let b_rot = BRep::new(
            b.vertices
                .iter()
                .map(|v| BRepVertex {
                    point: rot90(v.point),
                })
                .collect(),
            b.edges.clone(),
            b.faces.clone(),
        )
        .expect("rotated b");
        let c3 = c2.clone();

        let (infos1, _, _) = compute_phase_a_with_side_labels(
            &m1,
            &attr1,
            Some(&sl1),
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 1");
        let (infos2, _, _) = compute_phase_a_with_side_labels(
            &m2,
            &attr2,
            Some(&sl2),
            &a,
            &b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 2");
        let (infos3, _, _) = compute_phase_a_with_side_labels(
            &m3,
            &attr3,
            Some(&sl3),
            &a_rot,
            &b_rot,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("phase a 3");

        let st1 =
            decimate_patch_chains_441(&mut m1, &mut attr1, &mut sl1, &infos1, &c1, &a, &b, true)
                .expect("dec 1");
        let st2 =
            decimate_patch_chains_441(&mut m2, &mut attr2, &mut sl2, &infos2, &c2, &a, &b, true)
                .expect("dec 2");
        let st3 = decimate_patch_chains_441(
            &mut m3, &mut attr3, &mut sl3, &infos3, &c3, &a_rot, &b_rot, true,
        )
        .expect("dec 3");

        assert_eq!(st1.verts_dropped, 48);
        assert_eq!(st1.verts_dropped, st2.verts_dropped);
        assert_eq!(st1.verts_dropped, st3.verts_dropped);

        let kept1 = ring_surviving_pts(&m1);
        let kept2 = ring_surviving_pts(&m2);
        assert!(
            kept1.contains(&point_key(anchor_pt)),
            "authored operand BRepVertex must survive on closed ring"
        );
        assert_eq!(
            kept1, kept2,
            "operand-anchored closed ring must be invariant under vertex renumbering"
        );

        let kept1_rot90: BTreeSet<[u64; 3]> = m1
            .tris
            .iter()
            .flatten()
            .map(|&v| m1.verts[v as usize])
            .filter(|p| {
                let r = p.x().hypot(p.y());
                r > 1e-5 && r < 1e-2
            })
            .map(|p| point_key(rot90(p)))
            .collect();
        assert_eq!(
            kept1_rot90,
            ring_surviving_pts(&m3),
            "operand-anchored closed ring must be invariant under 90-deg world-frame rotation"
        );
    }
}

/// Forward §4.3.4 on an asymmetric open chain keeps identical physical chords
/// when reflected `(x, y, z) -> (-x, -y, z)` so `point_cmp` orders the opposite
/// endpoint first.
#[test]
fn open_chain_is_invariant_under_walk_direction_and_frame_reflection() {
    let xs = [0.0, 4e-5, 8e-5, 1.3e-4, 2.0e-4, 2.5e-4, 3.1e-4];
    let n = xs.len();
    let (mut m1, mut attr1, mut sl1, a, b) = shared_mesh_run(n, 5e-5, true);
    for (i, &x) in xs.iter().enumerate() {
        m1.verts[i] = Point3::new(x, 0.0, 0.0);
    }
    let x_end = *xs.last().unwrap();
    m1.verts[n] = Point3::new(x_end, 1.0, 0.0);
    m1.verts[n + 2] = Point3::new(x_end, 0.0, -1.0);

    // 180-deg rotation in XY `(x, y, z) -> (-x, -y, z)`: preserves `+z` triangle
    // orientation on Patch 0 while flipping the `x` order of the chain endpoints.
    let flip = |p: Point3| Point3::new(-p.x(), -p.y(), p.z());
    let mut m2 = Mesh::new(
        m1.verts.iter().copied().map(flip).collect(),
        m1.tris.clone(),
    );
    let mut attr2 = attr1.clone();
    let mut sl2 = sl1.clone();
    let a2 = single_plane_brep(
        [0.0, 0.0, 1.0],
        0.0,
        [[0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]],
    );
    let b2 = single_plane_brep(
        [0.0, -1.0, 0.0],
        0.0,
        [[0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
    );

    let (infos1, _, curves1) =
        compute_phase_a(&m1, &attr1, &a, &b, &crate::stage3_ssi::NO_EDGE_PROVENANCE)
            .expect("phase a 1");
    let (infos2, _, curves2) = compute_phase_a(
        &m2,
        &attr2,
        &a2,
        &b2,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a 2");

    let st1 = decimate_patch_chains_441(
        &mut m1, &mut attr1, &mut sl1, &infos1, &curves1, &a, &b, false,
    )
    .expect("dec 1");
    let st2 = decimate_patch_chains_441(
        &mut m2, &mut attr2, &mut sl2, &infos2, &curves2, &a2, &b2, false,
    )
    .expect("dec 2");
    assert!(st1.verts_dropped > 0);
    assert_eq!(st1.verts_dropped, st2.verts_dropped);

    let axis_pts = |m: &Mesh, negate_x: bool| -> Vec<u64> {
        let mut out: Vec<u64> = m
            .tris
            .iter()
            .flatten()
            .map(|&v| m.verts[v as usize])
            .filter(|p| p.y() == 0.0 && p.z() == 0.0)
            .map(|p| (if negate_x { -p.x() } else { p.x() }).to_bits())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    assert_eq!(
        axis_pts(&m1, false),
        axis_pts(&m2, true),
        "forward §4.3.4 must select identical physical vertices regardless of walk direction"
    );
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
    )
    .expect("dec 1");
    let st2 = decimate_patch_chains_441(
        &mut mesh2, &mut attr2, &mut sl2, &infos2, &curves2, &a, &b, true,
    )
    .expect("dec 2");
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

/// Chains owned by a `Surface::Sphere` patch decline decimation cleanly and
/// increment `DecimateStats::declined_sphere_unsupported` (since `SurfaceChart`
/// has no sphere chart).
#[test]
fn sphere_owner_patch_counts_declined_sphere_unsupported() {
    let n = 10usize;
    let r = 1.0f64;
    // Dense equatorial arc in `z = 0` (`theta_i = i * 5e-5`): lies on both `z = 0`
    // and unit sphere centered at origin.
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| {
            let th = 5e-5 * (i as f64);
            Point3::new(r * th.cos(), r * th.sin(), 0.0)
        })
        .collect();
    let c_plane = n as u32;
    let c_sphere = (n + 1) as u32;
    verts.push(Point3::new(0.0, 0.0, 0.0)); // center in z = 0
    verts.push(Point3::new(0.0, 0.0, 1.0)); // north pole on unit sphere

    let mut tris = Vec::new();
    let mut attrs = Vec::new();
    let mut labels = Vec::new();
    for i in 0..(n - 1) as u32 {
        tris.push([c_plane, i, i + 1]);
        attrs.push(Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        }));
        labels.push(TriangleSideLabel {
            cherchi_patch: 0,
            inside: [false, false],
            surface: [true, false],
            sheet_opposite: None,
        });
    }
    for i in 0..(n - 1) as u32 {
        tris.push([i + 1, i, c_sphere]);
        attrs.push(Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        }));
        labels.push(TriangleSideLabel {
            cherchi_patch: 1,
            inside: [false, false],
            surface: [false, true],
            sheet_opposite: None,
        });
    }

    let mut mesh = Mesh::new(verts, tris);
    let tris0 = mesh.tris.clone();
    let mut attribution = TriangleAttributionMap::empty();
    attribution.attributions = attrs;
    let mut side_labels = TriangleSideLabelMap { labels };

    let a = single_plane_brep(
        [0.0, 0.0, 1.0],
        0.0,
        [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
    );
    let sphere_surf = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: r,
    };
    let b = BRep::new(
        vec![
            BRepVertex {
                point: Point3::new(0.0, 0.0, -r),
            },
            BRepVertex {
                point: Point3::new(0.0, 0.0, r),
            },
        ],
        vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, -1.0, 0.0),
                radius: r,
            },
        }],
        vec![BRepFace {
            surface: sphere_surf,
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        }],
    )
    .expect("sphere brep");

    let pair_curve = Curve::SurfacePair {
        a: a.faces[0].surface,
        b: sphere_surf,
    };
    let curves: BTreeMap<(u32, u32), Curve> = (0..(n - 1) as u32)
        .map(|i| ((i, i + 1), pair_curve))
        .collect();
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
    )
    .expect("decimate");
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(st.declined_sphere_unsupported, 1, "{st:?}");
    assert_eq!(mesh.tris, tris0);
}

/// Passing an `infos` slice whose length disagrees with the flood-filled patch
/// partition returns a loud typed `YangError::Stage4ChainDecimateFailed`
/// (`ChainDecimateError::PatchPartitionMismatch`), never silently returning default stats.
#[test]
fn patch_partition_mismatch_returns_loud_typed_error() {
    let (mut mesh, mut attribution, mut side_labels, a, b) = shared_mesh_run(10, 5e-5, true);
    let (infos, _, curves) = compute_phase_a(
        &mesh,
        &attribution,
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("phase a");
    assert_eq!(infos.len(), 2);
    let err = decimate_patch_chains_441(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &infos[..1],
        &curves,
        &a,
        &b,
        false,
    )
    .expect_err("truncated infos must return typed error");
    let YangError::Stage4ChainDecimateFailed { reason } = err else {
        panic!("expected Stage4ChainDecimateFailed, got {err:?}");
    };
    assert_eq!(
        reason,
        ChainDecimateError::PatchPartitionMismatch {
            raw_patches: 2,
            info_patches: 1,
        }
    );
}

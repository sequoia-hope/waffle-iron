//! §4.3.4 Stage-5 chain decimation (`stage5_chain_decimate`, R0085
//! 2026-09-22): the interior vertices of a LineSegment / SurfacePair
//! INTERSECTION run that the paper's refinement acceptance calls redundant
//! are dropped identically on both owner faces; operand runs and junctions
//! are never touched; a loop never falls below three edges.

use std::collections::BTreeMap;

use cad_primitives::{Point3, Vector3};

use crate::brep::{BRepEdge, BRepFace, TessellationSource};
use crate::geom::{Curve, Surface};
use crate::stage5_chain_decimate::decimate_intersection_runs;

/// Two faces sharing a straight run of `n` vertices along +x at spacing
/// `s` (vertices `0..n`), each closed by its own two far corners: face A
/// walks the run forward and closes above (`n`, `n+1`), face B walks it
/// backward and closes below (`n+2`, `n+3`). Returns (verts, edges, faces,
/// run edge keys).
#[allow(clippy::type_complexity)]
fn shared_run(
    n: usize,
    s: f64,
) -> (
    Vec<Point3>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    BTreeMap<(u32, u32), Curve>,
) {
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| Point3::new(s * i as f64, 0.0, 0.0))
        .collect();
    let x_end = s * (n - 1) as f64;
    verts.push(Point3::new(x_end, 1.0, 0.0)); // n
    verts.push(Point3::new(0.0, 1.0, 0.0)); // n+1
    verts.push(Point3::new(x_end, -1.0, 0.0)); // n+2
    verts.push(Point3::new(0.0, -1.0, 0.0)); // n+3
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut push = |s: usize, e: usize| -> u32 {
        edges.push(BRepEdge {
            start: s as u32,
            end: e as u32,
            curve: Curve::LineSegment,
        });
        (edges.len() - 1) as u32
    };
    let mut a_loop: Vec<u32> = (0..n - 1).map(|i| push(i, i + 1)).collect();
    a_loop.push(push(n - 1, n));
    a_loop.push(push(n, n + 1));
    a_loop.push(push(n + 1, 0));
    let mut b_loop: Vec<u32> = (0..n - 1).map(|i| push(n - 1 - i, n - 2 - i)).collect();
    b_loop.push(push(0, n + 3));
    b_loop.push(push(n + 3, n + 2));
    b_loop.push(push(n + 2, n - 1));
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let faces = vec![
        BRepFace {
            surface: plane,
            outer_loop: a_loop,
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: b_loop,
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    let keys: BTreeMap<(u32, u32), Curve> = (0..n - 1)
        .map(|i| ((i as u32, (i + 1) as u32), Curve::LineSegment))
        .collect();
    (verts, edges, faces, keys)
}

fn undirected_pieces(edges: &[BRepEdge], lp: &[u32]) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = lp
        .iter()
        .map(|&ei| {
            let e = &edges[ei as usize];
            (e.start.min(e.end), e.start.max(e.end))
        })
        .collect();
    v.sort_unstable();
    v
}

fn loop_chains(edges: &[BRepEdge], lp: &[u32]) {
    for (i, &ei) in lp.iter().enumerate() {
        let e = &edges[ei as usize];
        let nx = &edges[lp[(i + 1) % lp.len()] as usize];
        assert_eq!(e.end, nx.start, "loop must chain start→end");
    }
}

/// A 40-vertex run at 5e-5 spacing (2e-3 long; the R0085 density) between
/// two faces: both faces keep the SAME pieces, every kept interior spacing
/// is at least d_p·10³ = 1e-4·(1+scale), the endpoints and the closing
/// corners survive, both loops still chain, and a dropped vertex's source
/// evaluates back onto its own position through the covering piece.
#[test]
fn dense_line_run_decimates_twin_conformant() {
    let (verts, mut edges, mut faces, keys) = shared_run(40, 5e-5);
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let before = edges.len();
    let st = decimate_intersection_runs(&verts, &mut edges, &mut faces, &mut sources, &keys);
    assert!(
        st.verts_dropped > 0,
        "the dense run must lose vertices: {st:?}"
    );
    assert!(edges.len() < before);
    assert_eq!(st.declined_floor, 0);
    loop_chains(&edges, &faces[0].outer_loop);
    loop_chains(&edges, &faces[1].outer_loop);
    // Twin conformance: the run pieces are identical undirected pairs on
    // both faces (the closing corners differ by construction).
    let run_pieces = |lp: &[u32]| -> Vec<(u32, u32)> {
        undirected_pieces(&edges, lp)
            .into_iter()
            .filter(|&(a, b)| a < 40 && b < 40)
            .collect()
    };
    let pa = run_pieces(&faces[0].outer_loop);
    let pb = run_pieces(&faces[1].outer_loop);
    assert_eq!(pa, pb, "both owners must emit identical pieces");
    // Endpoints kept; interior kept spacing ≥ d_p·10³ (scale ≤ 2 here).
    let mut kept: Vec<u32> = pa.iter().flat_map(|&(a, b)| [a, b]).collect();
    kept.sort_unstable();
    kept.dedup();
    assert_eq!(kept[0], 0);
    assert_eq!(*kept.last().unwrap(), 39);
    let dp = cad_primitives::TAU_MODEL * (1.0 + 2e-3);
    for w in kept.windows(2) {
        let d = (verts[w[1] as usize].x() - verts[w[0] as usize].x()).abs();
        // The final piece may be shorter (its end is the fixed endpoint).
        if w[1] != 39 {
            assert!(d >= dp * 1e3, "kept spacing {d:e} below d_p·10³");
        }
    }
    // Every piece is still a LineSegment.
    for e in &edges {
        assert_eq!(e.curve, Curve::LineSegment);
    }
    // Sources: a dropped vertex evaluates back onto itself via the lerp.
    for v in 1..39u32 {
        if kept.contains(&v) {
            continue;
        }
        let TessellationSource::BRepEdge { edge, t } = sources[v as usize] else {
            panic!("dropped vertex {v} must carry an edge source");
        };
        let e = &edges[edge as usize];
        let s = verts[e.start as usize];
        let en = verts[e.end as usize];
        let p = Point3::new(
            s.x() + t * (en.x() - s.x()),
            s.y() + t * (en.y() - s.y()),
            s.z() + t * (en.z() - s.z()),
        );
        let d = ((p.x() - verts[v as usize].x()).powi(2)
            + (p.y() - verts[v as usize].y()).powi(2)
            + (p.z() - verts[v as usize].z()).powi(2))
        .sqrt();
        assert!(d < 1e-12, "source of dropped vertex {v} off by {d:e}");
    }
}

/// The same run WITHOUT intersection provenance — the subdivision points of
/// a straight operand edge, exactly collinear: the strict class. They are
/// dropped like the intersection run (twin-conformant, endpoints kept).
#[test]
fn collinear_operand_subdivision_run_decimates() {
    let (verts, mut edges, mut faces, _keys) = shared_run(40, 5e-5);
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let st = decimate_intersection_runs(
        &verts,
        &mut edges,
        &mut faces,
        &mut sources,
        &BTreeMap::new(),
    );
    assert!(st.verts_dropped > 0, "{st:?}");
    loop_chains(&edges, &faces[0].outer_loop);
    loop_chains(&edges, &faces[1].outer_loop);
    let run_pieces = |lp: &[u32]| -> Vec<(u32, u32)> {
        undirected_pieces(&edges, lp)
            .into_iter()
            .filter(|&(a, b)| a < 40 && b < 40)
            .collect()
    };
    assert_eq!(
        run_pieces(&faces[0].outer_loop),
        run_pieces(&faces[1].outer_loop)
    );
}

/// An operand run with a REAL bend (each interior vertex 1e-9 off the line
/// through its neighbours — far above working precision) is geometry, not
/// subdivision: byte-identical edges and loops, whatever its spacing.
#[test]
fn bent_operand_run_is_untouched() {
    let (mut verts, mut edges, mut faces, _keys) = shared_run(40, 5e-5);
    for (i, v) in verts.iter_mut().enumerate().take(39).skip(1) {
        let bump = if i % 2 == 0 { 1e-9 } else { -1e-9 };
        *v = Point3::new(v.x(), bump, 0.0);
    }
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let edges0 = edges.clone();
    let loops0 = (faces[0].outer_loop.clone(), faces[1].outer_loop.clone());
    let st = decimate_intersection_runs(
        &verts,
        &mut edges,
        &mut faces,
        &mut sources,
        &BTreeMap::new(),
    );
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(edges, edges0);
    assert_eq!(
        (faces[0].outer_loop.clone(), faces[1].outer_loop.clone()),
        loops0
    );
}

/// A collinear operand run sparser than the paper's chord bound (1e-2 ≫
/// d_p·10³) is left alone: the pass moves only the sub-resolution class.
#[test]
fn coarse_operand_run_is_untouched() {
    let (verts, mut edges, mut faces, _keys) = shared_run(12, 1e-2);
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let edges0 = edges.clone();
    let st = decimate_intersection_runs(
        &verts,
        &mut edges,
        &mut faces,
        &mut sources,
        &BTreeMap::new(),
    );
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(edges, edges0);
}

/// A run whose vertices are already sparser than the criterion (spacing
/// 1e-2 ≫ d_p·10³) is a no-op — byte-identical.
#[test]
fn sparse_run_is_byte_identical() {
    let (verts, mut edges, mut faces, keys) = shared_run(12, 1e-2);
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let edges0 = edges.clone();
    let loops0 = (faces[0].outer_loop.clone(), faces[1].outer_loop.clone());
    let st = decimate_intersection_runs(&verts, &mut edges, &mut faces, &mut sources, &keys);
    assert_eq!(st.verts_dropped, 0, "{st:?}");
    assert_eq!(edges, edges0);
    assert_eq!(
        (faces[0].outer_loop.clone(), faces[1].outer_loop.clone()),
        loops0
    );
}

/// Loop floor: a face bounded by the dense run and ONE closing edge (a
/// two-stretch loop) must keep at least three edges — the run may only
/// decimate to two pieces or stay verbatim, never to one.
#[test]
fn loop_floor_keeps_three_edges() {
    let n = 40usize;
    let s = 5e-5;
    let mut verts: Vec<Point3> = (0..n)
        .map(|i| Point3::new(s * i as f64, 0.0, 0.0))
        .collect();
    // Face A closes the run with a single bulging segment through a far
    // vertex? No — a single edge n-1 → 0 (a lens-like 2-stretch loop).
    verts.push(Point3::new(0.0, 1.0, 0.0)); // n (face B's corner)
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut push = |s: usize, e: usize| -> u32 {
        edges.push(BRepEdge {
            start: s as u32,
            end: e as u32,
            curve: Curve::LineSegment,
        });
        (edges.len() - 1) as u32
    };
    let mut a_loop: Vec<u32> = (0..n - 1).map(|i| push(i, i + 1)).collect();
    a_loop.push(push(n - 1, 0));
    let mut b_loop: Vec<u32> = (0..n - 1).map(|i| push(n - 1 - i, n - 2 - i)).collect();
    b_loop.push(push(0, n));
    b_loop.push(push(n, n - 1));
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let mut faces = vec![
        BRepFace {
            surface: plane,
            outer_loop: a_loop,
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: b_loop,
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    let keys: BTreeMap<(u32, u32), Curve> = (0..n - 1)
        .map(|i| ((i as u32, (i + 1) as u32), Curve::LineSegment))
        .collect();
    let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    decimate_intersection_runs(&verts, &mut edges, &mut faces, &mut sources, &keys);
    assert!(
        faces[0].outer_loop.len() >= 3,
        "face A loop fell below 3 edges"
    );
    assert!(
        faces[1].outer_loop.len() >= 3,
        "face B loop fell below 3 edges"
    );
    loop_chains(&edges, &faces[0].outer_loop);
    loop_chains(&edges, &faces[1].outer_loop);
}

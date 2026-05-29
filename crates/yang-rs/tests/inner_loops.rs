//! PR-YR5c — holed-face (inner-loop) tests for `yang_rs::boolean()`.
//!
//! When one convex solid pierces a hole *through* another solid's face, the
//! result face is an **annulus**: an outer boundary loop plus one or more
//! inner loops (the holes). Before YR5c, `reconstruct_topology` rejected any
//! patch with more than one boundary cycle and returned
//! `Err(YangError::NonManifoldOutput)` — the sole M3 fuzz failure bucket. YR5c
//! adds inner-loop support so those booleans succeed with a correct holed-face
//! B-Rep.
//!
//! This file exercises the spec branch table (`specs/yang_pr_yr5c_inner_loops.md`):
//!   - `cube_minus_interior_rod`  → L1 (one outer + inner loops), I2, I3, I5
//!   - `cube_minus_blind_rod`     → L1 mixed with L0 (one holed + one plain)
//!   - `corner_clip_has_no_holes` → L0 regression (no spurious inner loops)
//!
//! Self-skips cleanly when the C++ sidecar binary is absent
//! (`SidecarBoolean::from_env()` → `Err`).

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarBoolean;
use std::collections::HashMap;
use yang_rs::{BRep, BRepEdge, BRepFace, Curve, Surface, YangError};

// =========================================================================
// Axis-aligned box → topologized BRep.
//
// Same 8-vert / 24-edge / 6-quad-face template as `fuzz_boxes::OrientedBox`,
// specialized to axis-aligned (no rotation): outward normals are the ±unit
// axes; plane offset `d = −n·(a vertex on that face)`; per-face loops are the
// CCW (from outside) quad cycles. yang Stage 1 (`BRep::new`) canonicalizes
// triangle winding to the stated normal.
// =========================================================================

struct AaBox {
    center: [f64; 3],
    half: [f64; 3],
}

impl AaBox {
    fn corner(&self, sx: f64, sy: f64, sz: f64) -> [f64; 3] {
        [
            self.center[0] + sx * self.half[0],
            self.center[1] + sy * self.half[1],
            self.center[2] + sz * self.half[2],
        ]
    }

    fn to_brep(&self) -> Result<BRep, YangError> {
        // Corner s-order matches fuzz_boxes::OrientedBox / m3_adversary::cube:
        //   0:(−−−) 1:(+−−) 2:(++−) 3:(−+−) 4:(−−+) 5:(+−+) 6:(+++) 7:(−++)
        let signs: [[f64; 3]; 8] = [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let verts: Vec<yang_rs::BRepVertex> = signs
            .iter()
            .map(|s| {
                let c = self.corner(s[0], s[1], s[2]);
                yang_rs::BRepVertex {
                    point: Point3::new(c[0], c[1], c[2]),
                }
            })
            .collect();

        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3], // −z
            [4, 7, 6, 5], // +z
            [0, 4, 5, 1], // −y
            [1, 5, 6, 2], // +x
            [2, 6, 7, 3], // +y
            [3, 7, 4, 0], // −x
        ];
        let normals: [[f64; 3]; 6] = [
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];

        let mut edges = Vec::with_capacity(24);
        let mut loops = Vec::with_capacity(6);
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

        let faces: Vec<BRepFace> = (0..6)
            .map(|i| {
                let n = normals[i];
                let v0 = verts[face_verts[i][0] as usize].point.as_array();
                let d = -(n[0] * v0[0] + n[1] * v0[1] + n[2] * v0[2]);
                BRepFace {
                    surface: Surface::Plane {
                        normal: Vector3::new(n[0], n[1], n[2]),
                        d,
                    },
                    outer_loop: loops[i].clone(),
                    inner_loops: Vec::new(),
                }
            })
            .collect();

        BRep::new(verts, edges, faces)
    }
}

// =========================================================================
// Mesh audit helpers (self-contained; mirror fuzz_boxes / m3_adversary).
// =========================================================================

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

// =========================================================================
// B-Rep loop audit helpers (the structural oracles new in YR5c).
// =========================================================================

/// Signed area of a directed B-Rep loop measured along `normal`:
/// `(Σ v_i × v_{i+1}) · n̂` (Newell area-vector dotted with the face normal).
/// `> 0` ⇒ CCW-from-outside (outer); `< 0` ⇒ CW-from-outside (a hole).
///
/// `loop_edge_indices` are indices into `brep.edges()`; the loop's ordered
/// vertices are obtained by walking `edges[idx].start`.
fn loop_signed_area(brep: &BRep, loop_edge_indices: &[u32], normal: Vector3) -> f64 {
    let edges = brep.edges();
    let verts = brep.vertices();
    let pts: Vec<[f64; 3]> = loop_edge_indices
        .iter()
        .map(|&ei| verts[edges[ei as usize].start as usize].point.as_array())
        .collect();
    let n = pts.len();
    // Newell area-vector: Σ v_i × v_{i+1}.
    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        nx += a[1] * b[2] - a[2] * b[1];
        ny += a[2] * b[0] - a[0] * b[2];
        nz += a[0] * b[1] - a[1] * b[0];
    }
    let nrm = normal.as_array();
    // 2·area·n̂ dotted with face normal; sign + relative magnitude is what we
    // assert, so the factor of 2 is irrelevant.
    0.5 * (nx * nrm[0] + ny * nrm[1] + nz * nrm[2])
}

/// Collect every directed loop edge `(start, end)` across ALL faces of `brep`
/// (outer loop + every inner loop), resolved through `brep.edges()`.
fn all_directed_loop_edges(brep: &BRep) -> Vec<(u32, u32)> {
    let edges = brep.edges();
    let mut out = Vec::new();
    for f in brep.faces() {
        for &ei in &f.outer_loop {
            let e = &edges[ei as usize];
            out.push((e.start, e.end));
        }
        for inner in &f.inner_loops {
            for &ei in inner {
                let e = &edges[ei as usize];
                out.push((e.start, e.end));
            }
        }
    }
    out
}

/// I3: every directed loop edge `(a,b)` has exactly one reverse `(b,a)`.
/// Returns the number of edges whose forward/reverse counts are unbalanced
/// (0 ⇒ a closed 2-manifold B-Rep stitched by its loops).
fn brep_unpaired_loop_edges(brep: &BRep) -> usize {
    let dirs = all_directed_loop_edges(brep);
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for &(a, b) in &dirs {
        *counts.entry((a, b)).or_insert(0) += 1;
    }
    let mut unpaired = 0;
    for (&(a, b), &fwd) in &counts {
        let rev = counts.get(&(b, a)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn face_normal(f: &BRepFace) -> Vector3 {
    match f.surface {
        Surface::Plane { normal, .. } => normal,
    }
}

const VOL_TOL: f64 = 1e-6;

// =========================================================================
// Tests
// =========================================================================

/// Canonical case (spec L1 + I2 + I3 + I5): a unit cube minus an interior rod
/// that pierces clean through. The z=0 and z=1 faces become annuli.
///
/// Before YR5c this returned `Err(NonManifoldOutput)`; the headline assert is
/// `r.is_ok()`.
#[test]
fn cube_minus_interior_rod() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[inner_loops] SKIP: sidecar binary not found");
        return;
    };

    // A = unit cube [0,1]^3.
    let a = AaBox {
        center: [0.5, 0.5, 0.5],
        half: [0.5, 0.5, 0.5],
    }
    .to_brep()
    .expect("A BRep::new failed");
    // B = rod [0.3,0.7] x [0.3,0.7] x [-0.5,1.5] (pierces through z).
    let b = AaBox {
        center: [0.5, 0.5, 0.5],
        half: [0.2, 0.2, 1.0],
    }
    .to_brep()
    .expect("B BRep::new failed");

    let r = yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb);

    // --- Headline (L1): the boolean now succeeds. ---
    let r = match r {
        Ok(brep) => brep,
        Err(e) => panic!("cube_minus_interior_rod expected Ok, got Err({e:?})"),
    };

    // --- Exactly 2 holed faces (the z=0 and z=1 annuli). ---
    let holed = r
        .faces()
        .iter()
        .filter(|f| f.inner_loops.len() == 1)
        .count();
    assert_eq!(
        holed, 2,
        "expected exactly 2 faces with one inner loop (z=0 and z=1 annuli), got {holed}"
    );

    // --- I2: loop orientation per face (outer > 0, each inner < 0). ---
    for (fi, f) in r.faces().iter().enumerate() {
        let n = face_normal(f);
        let outer = loop_signed_area(&r, &f.outer_loop, n);
        assert!(
            outer > 0.0,
            "face {fi}: outer loop signed area {outer} should be > 0 (CCW from outside)"
        );
        for (li, inner) in f.inner_loops.iter().enumerate() {
            let area = loop_signed_area(&r, inner, n);
            assert!(
                area < 0.0,
                "face {fi} inner loop {li}: signed area {area} should be < 0 (hole, CW from outside)"
            );
        }
    }

    // --- I3: B-Rep is manifold — every directed loop edge has one reverse. ---
    let unpaired = brep_unpaired_loop_edges(&r);
    assert_eq!(
        unpaired, 0,
        "B-Rep loop edges not manifold: {unpaired} directed edges lack a unique reverse"
    );

    // --- I5: mesh volume + watertight. Subtract = 1 - 0.16 = 0.84. ---
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.84).abs() < VOL_TOL,
        "subtract volume {vol} != 0.84 (overlap 0.16)"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "output mesh not watertight"
    );
}

/// Edge case (spec L1 + L0 mix): a rod that enters the z=0 face and stops
/// inside A — a blind square pit. The z=0 face is holed; the z=1 face is plain.
#[test]
fn cube_minus_blind_rod() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[inner_loops] SKIP: sidecar binary not found");
        return;
    };

    // A = unit cube [0,1]^3.
    let a = AaBox {
        center: [0.5, 0.5, 0.5],
        half: [0.5, 0.5, 0.5],
    }
    .to_brep()
    .expect("A BRep::new failed");
    // B = blind rod [0.3,0.7] x [0.3,0.7] x [-0.5,0.5]: enters z=0, stops at z=0.5.
    let b = AaBox {
        center: [0.5, 0.5, 0.0],
        half: [0.2, 0.2, 0.5],
    }
    .to_brep()
    .expect("B BRep::new failed");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("cube_minus_blind_rod expected Ok, got Err({e:?})"),
    };

    // --- Exactly 1 holed face (z=0). z=1 stays plain. ---
    let holed = r
        .faces()
        .iter()
        .filter(|f| f.inner_loops.len() == 1)
        .count();
    assert_eq!(
        holed, 1,
        "expected exactly 1 holed face (z=0 blind pit), got {holed}"
    );

    // --- I2: orientation. ---
    for (fi, f) in r.faces().iter().enumerate() {
        let n = face_normal(f);
        assert!(
            loop_signed_area(&r, &f.outer_loop, n) > 0.0,
            "face {fi}: outer loop should be CCW from outside"
        );
        for (li, inner) in f.inner_loops.iter().enumerate() {
            assert!(
                loop_signed_area(&r, inner, n) < 0.0,
                "face {fi} inner loop {li}: hole should be CW from outside"
            );
        }
    }

    // --- I3: B-Rep manifold. ---
    assert_eq!(
        brep_unpaired_loop_edges(&r),
        0,
        "blind-pit B-Rep loop edges not manifold"
    );

    // --- I5: mesh volume + watertight. overlap = 0.4*0.4*0.5 = 0.08 ⇒ 0.92. ---
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.92).abs() < VOL_TOL,
        "blind subtract volume {vol} != 0.92 (overlap 0.08)"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "blind-pit output mesh not watertight"
    );
}

/// Regression (spec L0): a corner clip must NOT produce spurious inner loops.
/// A = unit cube; B = unit cube centered at (1,1,1) clipping A's +++ corner
/// (the M3 diagonal case). Every output face is simple.
#[test]
fn corner_clip_has_no_holes() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[inner_loops] SKIP: sidecar binary not found");
        return;
    };

    // A = unit cube [0,1]^3.
    let a = AaBox {
        center: [0.5, 0.5, 0.5],
        half: [0.5, 0.5, 0.5],
    }
    .to_brep()
    .expect("A BRep::new failed");
    // B = unit cube [0.5,1.5]^3 (clips A's +++ corner).
    let b = AaBox {
        center: [1.0, 1.0, 1.0],
        half: [0.5, 0.5, 0.5],
    }
    .to_brep()
    .expect("B BRep::new failed");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("corner_clip expected Ok, got Err({e:?})"),
    };

    // --- L0 regression: NO face has an inner loop. ---
    let holed = r
        .faces()
        .iter()
        .filter(|f| !f.inner_loops.is_empty())
        .count();
    assert_eq!(
        holed, 0,
        "corner clip should yield only simple faces, but {holed} face(s) have inner loops"
    );

    // --- I3 still holds for the simple-face case. ---
    assert_eq!(
        brep_unpaired_loop_edges(&r),
        0,
        "corner-clip B-Rep loop edges not manifold"
    );

    // --- I5: overlap = [0.5,1]^3 = 0.125 ⇒ subtract 0.875. ---
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.875).abs() < VOL_TOL,
        "corner-clip subtract volume {vol} != 0.875 (overlap 0.125)"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "corner-clip output mesh not watertight"
    );
}

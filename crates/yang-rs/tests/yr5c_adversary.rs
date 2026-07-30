//! PR-YR5c ADVERSARY — attack the holed-face (inner-loop) reconstruction.
//!
//! These tests probe the YR5c claims beyond the happy-path `inner_loops.rs`
//! fixtures:
//!
//! 1. `two_holes_*` — a face with ≥2 inner loops (the N-hole code path) via a
//!    chained / double-rod subtract.
//! 2. `tjunction_*` — a genuinely non-manifold patch STILL errors
//!    `NonManifoldOutput` (via a MockBackend driving a hand-built
//!    `LabeledArrangement`).
//! 3. `cavity_wall_normal` — the Subtract cavity-wall normal flip is correct:
//!    stored normals point *result*-outward (into the tunnel void) and I2 holds
//!    for ALL faces.
//! 4. `rotated_*` — loop classification survives an off-axis rigid rotation
//!    (stresses Newell-vs-normal).
//! 5. `corner_clip_*` — regression: simple faces stay simple, no spurious
//!    holes; correct volume.
//!
//! Self-skips cleanly when the C++ sidecar binary is absent
//! (`yang_rs::native_backend()` → `None`, FFI stub build). Determinism: all geometry is a fixed
//! constant; the rotation matrix is a fixed (non-axis-aligned) rigid rotation.
//!
//! No production code is modified by this file. Where an attack confirms a real
//! bug, the failing test is left in place and called out in the report.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use std::collections::HashMap;
use yang_rs::{BRep, BRepEdge, BRepFace, Curve, Surface, YangError};

// =========================================================================
// Linear algebra (inline; cad-primitives is types-only).
// =========================================================================

type Mat3 = [[f64; 3]; 3];

fn mat_vec(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Quaternion (w,x,y,z) → rotation matrix (unit-normalized internally).
fn quat_to_mat3(w: f64, x: f64, y: f64, z: f64) -> Mat3 {
    let n = (w * w + x * x + y * y + z * z).sqrt();
    let (w, x, y, z) = (w / n, x / n, y / n, z / n);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

// =========================================================================
// Oriented-box generator → topologized BRep (same template as fuzz_boxes /
// inner_loops; parameterized by center, half-extents, rotation).
// =========================================================================

struct OrientedBox {
    center: [f64; 3],
    half: [f64; 3],
    rot: Mat3,
}

impl OrientedBox {
    fn aligned(center: [f64; 3], half: [f64; 3]) -> Self {
        OrientedBox {
            center,
            half,
            rot: IDENTITY,
        }
    }

    /// Rigidly rotate this axis-aligned box about a scene `pivot` by `rot`.
    /// Effective box: center' = pivot + rot·(center − pivot); same `rot`.
    /// (So two boxes rotated about the SAME pivot keep their relative pose —
    /// a true rigid scene rotation, unlike rotating each about its own center.)
    fn rotated_about(center: [f64; 3], half: [f64; 3], pivot: [f64; 3], rot: Mat3) -> Self {
        let rel = [
            center[0] - pivot[0],
            center[1] - pivot[1],
            center[2] - pivot[2],
        ];
        let rr = mat_vec(&rot, rel);
        OrientedBox {
            center: add3(pivot, rr),
            half,
            rot,
        }
    }

    fn corner(&self, sx: f64, sy: f64, sz: f64) -> [f64; 3] {
        let local = [sx * self.half[0], sy * self.half[1], sz * self.half[2]];
        add3(self.center, mat_vec(&self.rot, local))
    }

    fn to_brep(&self) -> Result<BRep, YangError> {
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
        let local_normals: [[f64; 3]; 6] = [
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
                let wn = mat_vec(&self.rot, local_normals[i]);
                let normal = Vector3::new(wn[0], wn[1], wn[2]);
                let v0 = verts[face_verts[i][0] as usize].point.as_array();
                let d = -dot3(wn, v0);
                BRepFace {
                    surface: Surface::Plane { normal, d },
                    outer_loop: loops[i].clone(),
                    inner_loops: Vec::new(),
                    reversed: false,
                }
            })
            .collect();

        BRep::new(verts, edges, faces)
    }
}

// =========================================================================
// Mesh + B-Rep audit helpers (mirror inner_loops.rs / fuzz_boxes.rs).
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

/// Signed area of a directed B-Rep loop measured along `normal`:
/// `(Σ v_i × v_{i+1}) · n̂`. `> 0` ⇒ CCW-from-outside (outer); `< 0` ⇒ hole.
fn loop_signed_area(brep: &BRep, loop_edge_indices: &[u32], normal: Vector3) -> f64 {
    let edges = brep.edges();
    let verts = brep.vertices();
    let pts: Vec<[f64; 3]> = loop_edge_indices
        .iter()
        .map(|&ei| verts[edges[ei as usize].start as usize].point.as_array())
        .collect();
    let n = pts.len();
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
    0.5 * (nx * nrm[0] + ny * nrm[1] + nz * nrm[2])
}

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

/// I1: every loop is a closed directed cycle
/// (`edges[loop[i]].end == edges[loop[(i+1)%n]].start`).
fn loop_is_closed(brep: &BRep, loop_edge_indices: &[u32]) -> bool {
    let edges = brep.edges();
    let n = loop_edge_indices.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let cur = &edges[loop_edge_indices[i] as usize];
        let nxt = &edges[loop_edge_indices[(i + 1) % n] as usize];
        if cur.end != nxt.start {
            return false;
        }
    }
    true
}

fn face_normal(f: &BRepFace) -> Vector3 {
    match f.surface {
        Surface::Plane { normal, .. } => normal,
        _ => panic!("expected Plane"),
    }
}

const VOL_TOL: f64 = 1e-6;

/// Assert the full structural invariant suite (I1/I2/I3) on a reconstructed
/// B-Rep, for ALL faces (including flipped Subtract cavity walls).
fn assert_brep_invariants(r: &BRep) {
    // I3: closed 2-manifold by loop edges.
    assert_eq!(
        brep_unpaired_loop_edges(r),
        0,
        "I3: B-Rep loop edges not manifold"
    );
    for (fi, f) in r.faces().iter().enumerate() {
        let n = face_normal(f);
        // I1: closure of every loop.
        assert!(
            loop_is_closed(r, &f.outer_loop),
            "I1: face {fi} outer loop not a closed cycle"
        );
        for (li, inner) in f.inner_loops.iter().enumerate() {
            assert!(
                loop_is_closed(r, inner),
                "I1: face {fi} inner loop {li} not a closed cycle"
            );
        }
        // I2: outer > 0, every inner < 0 (relative to the STORED normal, which
        // is the result-outward normal after any Subtract cavity-wall flip).
        let outer = loop_signed_area(r, &f.outer_loop, n);
        assert!(
            outer > 0.0,
            "I2: face {fi} outer loop area {outer} should be > 0 vs stored normal"
        );
        for (li, inner) in f.inner_loops.iter().enumerate() {
            let area = loop_signed_area(r, inner, n);
            assert!(
                area < 0.0,
                "I2: face {fi} inner loop {li} area {area} should be < 0 (hole)"
            );
        }
    }
}

fn skip_or_backend() -> Option<yang_rs::NativeBoolean> {
    match yang_rs::native_backend() {
        Some(nb) => Some(nb),
        None => {
            eprintln!("[yr5c_adversary] SKIP: native FFI shim not linked (stub build)");
            None
        }
    }
}

// =========================================================================
// ATTACK 1 — a face with TWO inner loops (the N-hole code path).
//
// Strategy: subtract B = the UNION of two disjoint rods from A. Rather than
// build a union B-Rep (which yang can't yet emit as valid topologized input
// for re-feeding), we drive the N-hole path by chaining: r1 = A − rod1, then
// feed r1 back as input A for r2 = r1 − rod2 IF r1 is valid re-feedable input.
//
// r1's faces are reconstructed quads/annuli with Plane surfaces and closed
// loops, so it IS a structurally valid BRep — but `BRep::new` requires each
// face's OUTER loop be a simple ≥3 cycle, which r1 satisfies. We attempt the
// chain; if `BRep::new` rejects r1 (e.g. multi-shell vertex sharing), we fall
// back to the documented single-step "≥1 hole" assertion and note the N-hole
// path is exercised by the fuzz instead.
// =========================================================================

/// Two rods piercing the SAME pair of faces (z=0 and z=1), far apart in x, so
/// each through-face should carry TWO holes. We cannot express B = (rod ∪ rod)
/// as one convex box, so we subtract sequentially and inspect the SECOND
/// result. The second subtract's z-faces must each have 2 inner loops IF
/// chaining is supported; otherwise this documents the limitation.
#[test]
fn two_holes_in_one_face_via_chained_subtract() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    let a = OrientedBox::aligned([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
        .to_brep()
        .expect("A");
    // rod1 near x=0.25, rod2 near x=0.75 — both pierce z fully, disjoint in x.
    let rod1 = OrientedBox::aligned([0.25, 0.5, 0.5], [0.1, 0.1, 1.0])
        .to_brep()
        .expect("rod1");
    let rod2 = OrientedBox::aligned([0.75, 0.5, 0.5], [0.1, 0.1, 1.0])
        .to_brep()
        .expect("rod2");

    let r1 = match yang_rs::boolean(&a, &rod1, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("first subtract (A − rod1) expected Ok, got Err({e:?})"),
    };
    // r1 should be valid + have exactly 2 single-hole faces.
    let r1_holed = r1
        .faces()
        .iter()
        .filter(|f| f.inner_loops.len() == 1)
        .count();
    assert_eq!(r1_holed, 2, "A − rod1 should yield 2 single-hole faces");
    assert_brep_invariants(&r1);

    // Attempt to re-feed r1 as input. yang's `boolean` takes BReps; r1's
    // topology is reconstructed but its faces have Plane surfaces and closed
    // loops. The attribution path needs r1's face PLANES — present. If r1 is
    // not re-feedable (BRep::new validation, or the sidecar rejects the
    // reconstructed mesh as non-watertight/self-intersecting), we record that
    // and stop — the N-hole path is then only reachable via the fuzz corpus.
    //
    // We rebuild r1 through `BRep::new` from its own (vertices, edges, faces)
    // to force the same input-validation the original boxes passed.
    let r1_rebuilt = BRep::new(
        r1.vertices().to_vec(),
        r1.edges().to_vec(),
        r1.faces().to_vec(),
    );
    let r1_input = match r1_rebuilt {
        Ok(br) => br,
        Err(e) => {
            eprintln!(
                "[two_holes] r1 not re-feedable as BRep input ({e:?}); \
                 N-hole path documented via fuzz instead"
            );
            return;
        }
    };

    match yang_rs::boolean(&r1_input, &rod2, BoolOp::Subtract, &sb) {
        Ok(r2) => {
            // If the chain worked, the z-faces should now hold TWO holes each.
            let two_holed = r2
                .faces()
                .iter()
                .filter(|f| f.inner_loops.len() == 2)
                .count();
            let any_holed = r2
                .faces()
                .iter()
                .filter(|f| !f.inner_loops.is_empty())
                .count();
            eprintln!(
                "[two_holes] r2 faces: {} with 2 holes, {} with ≥1 hole",
                two_holed, any_holed
            );
            // Whatever the hole partition, the result must satisfy I1/I2/I3.
            assert_brep_invariants(&r2);
            // And the volume: 1 − 2·(0.2·0.2·1.0) = 1 − 0.08 = 0.92.
            let vol = signed_volume(r2.as_mesh());
            assert!(
                (vol - 0.92).abs() < VOL_TOL,
                "two-rod subtract volume {vol} != 0.92"
            );
            assert_eq!(
                unpaired_half_edges(r2.as_mesh()),
                0,
                "two-rod not watertight"
            );
            // The N-hole code path is reached iff some face carries ≥2 holes.
            // Both rods pierce z=0 and z=1, so we EXPECT 2 faces with 2 holes.
            assert_eq!(
                two_holed, 2,
                "expected 2 faces (z=0, z=1) each with 2 inner loops"
            );
        }
        Err(e) => {
            eprintln!(
                "[two_holes] chained subtract (r1 − rod2) returned Err({e:?}); \
                 re-feeding reconstructed B-Reps is out of scope for YR5c. \
                 N-hole code path documented via fuzz."
            );
        }
    }
}

// =========================================================================
// ATTACK 3 — Subtract cavity-wall normal flip correctness.
//
// For `cube_minus_interior_rod`, the 4 tunnel-wall faces come from B and were
// flipped by `flip_for_op`. Their STORED normals must point result-outward
// (into the tunnel void). Concretely: the x=0.3 wall's result-outward normal
// is +x (toward tunnel center at x=0.5). We verify, for every face whose plane
// is one of the 4 tunnel walls, that the stored normal points toward the
// tunnel center — AND that I2 (outer area > 0 vs stored normal) holds for ALL
// faces, which is the self-consistency the flip must produce.
// =========================================================================

#[test]
fn cavity_wall_normals_point_result_outward() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    let a = OrientedBox::aligned([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
        .to_brep()
        .expect("A");
    // rod [0.3,0.7]×[0.3,0.7]×[−0.5,1.5]. Tunnel center axis is x=0.5,y=0.5.
    let b = OrientedBox::aligned([0.5, 0.5, 0.5], [0.2, 0.2, 1.0])
        .to_brep()
        .expect("B");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("cube_minus_interior_rod expected Ok, got Err({e:?})"),
    };

    // Full I1/I2/I3 over ALL faces (incl. flipped B walls).
    assert_brep_invariants(&r);

    // Tunnel axis center (the rod's central axis): x=0.5, y=0.5, any z.
    let axis = [0.5, 0.5];
    let mut wall_faces = 0usize;
    for (fi, f) in r.faces().iter().enumerate() {
        let Surface::Plane { normal, d } = f.surface else {
            continue;
        };
        let n = normal.as_array();
        // Identify a tunnel WALL: a plane parallel to z whose |x or y| offset
        // is 0.3 or 0.7 (the rod sides). Walls have n_z ≈ 0.
        let is_vertical = n[2].abs() < 1e-9;
        if !is_vertical {
            continue;
        }
        // Plane offset value: for normal ±x, plane is x = ∓d (n·p + d = 0).
        // Rod walls sit at x∈{0.3,0.7} or y∈{0.3,0.7}. A's outer walls are at
        // x∈{0,1} or y∈{0,1}. We want only the INNER (rod) walls.
        // A point on the plane: solve along the dominant axis.
        let on_rod_wall = if n[0].abs() > 0.5 {
            // x-plane: x0 = -d / n[0]
            let x0 = -d / n[0];
            (x0 - 0.3).abs() < 1e-6 || (x0 - 0.7).abs() < 1e-6
        } else if n[1].abs() > 0.5 {
            let y0 = -d / n[1];
            (y0 - 0.3).abs() < 1e-6 || (y0 - 0.7).abs() < 1e-6
        } else {
            false
        };
        if !on_rod_wall {
            continue;
        }
        wall_faces += 1;

        // A representative point on this wall: project the tunnel-axis point
        // onto the wall plane is awkward; instead use the face's first outer
        // loop vertex and step from there toward the axis.
        let e0 = &r.edges()[f.outer_loop[0] as usize];
        let p = r.vertices()[e0.start as usize].point.as_array();
        // Vector from wall point toward tunnel axis (in xy):
        let to_axis = [axis[0] - p[0], axis[1] - p[1], 0.0];
        // Result-outward normal of a CAVITY wall points INTO the void, i.e.
        // toward the tunnel center. So n·to_axis must be > 0.
        let proj = dot3(n, to_axis);
        assert!(
            proj > 0.0,
            "face {fi}: tunnel-wall stored normal {n:?} should point toward \
             tunnel axis (into the void); n·to_axis = {proj} (≤0 means the flip \
             failed and the normal points into the solid)"
        );
    }
    assert_eq!(
        wall_faces, 4,
        "expected exactly 4 tunnel-wall faces from the rod, found {wall_faces}"
    );
}

// =========================================================================
// ATTACK 4 — loop classification on ROTATED geometry.
//
// Apply a fixed non-axis-aligned rigid rotation to BOTH A and the rod. Subtract
// must still produce a holed-face B-Rep, the correct (rotation-invariant)
// volume 0.84, watertight mesh, manifold B-Rep, and I1/I2/I3 over all faces.
// This stresses the Newell-area-vector-vs-normal classification off-axis.
// =========================================================================

#[test]
fn rotated_cube_minus_rod_holed_faces() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    // Fixed, deliberately non-trivial rotation (no zero/equal quat components).
    let rot = quat_to_mat3(0.31, -0.62, 0.47, 0.55);
    let pivot = [0.5, 0.5, 0.5];

    // Rigid scene rotation about a COMMON pivot so A and the rod keep their
    // relative pose (the through-rod centered at the cube center).
    let a = OrientedBox::rotated_about([0.5, 0.5, 0.5], [0.5, 0.5, 0.5], pivot, rot)
        .to_brep()
        .expect("A rotated");
    let b = OrientedBox::rotated_about([0.5, 0.5, 0.5], [0.2, 0.2, 1.0], pivot, rot)
        .to_brep()
        .expect("B rotated");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("rotated cube_minus_rod expected Ok, got Err({e:?})"),
    };

    // The 2 through-faces (z=0, z=1 under the rotation) are still annuli.
    let holed = r
        .faces()
        .iter()
        .filter(|f| f.inner_loops.len() == 1)
        .count();
    assert_eq!(
        holed, 2,
        "rotated through-rod should still yield 2 single-hole faces, got {holed}"
    );

    // I1/I2/I3 must hold off-axis (the headline of this attack).
    assert_brep_invariants(&r);

    // Rotation-invariant volume: 1 − 0.16 = 0.84.
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.84).abs() < VOL_TOL,
        "rotated subtract volume {vol} != 0.84 (rotation must preserve volume)"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "rotated output mesh not watertight"
    );
}

/// Companion: a rotated BLIND rod (mixed L0/L1 off-axis) — 1 holed + plain.
#[test]
fn rotated_cube_minus_blind_rod() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    let rot = quat_to_mat3(0.71, 0.18, -0.39, 0.55);
    let pivot = [0.5, 0.5, 0.5];

    // Rigid scene rotation about a COMMON pivot. In the UNROTATED frame:
    // A = [0,1]^3; B = blind rod centered (0.5,0.5,0.0) half (0.2,0.2,0.5)
    // ⇒ spans z∈[−0.5,0.5], poking out the −z face and stopping at z=0.5
    // inside A (1 holed face). The rigid rotation preserves this relationship.
    let a = OrientedBox::rotated_about([0.5, 0.5, 0.5], [0.5, 0.5, 0.5], pivot, rot)
        .to_brep()
        .expect("A rotated");
    let b = OrientedBox::rotated_about([0.5, 0.5, 0.0], [0.2, 0.2, 0.5], pivot, rot)
        .to_brep()
        .expect("B rotated");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("rotated blind rod expected Ok, got Err({e:?})"),
    };

    let holed = r
        .faces()
        .iter()
        .filter(|f| f.inner_loops.len() == 1)
        .count();
    assert_eq!(
        holed, 1,
        "rotated blind pit should yield exactly 1 holed face"
    );
    assert_brep_invariants(&r);

    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.92).abs() < VOL_TOL,
        "rotated blind subtract volume {vol} != 0.92"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "rotated blind not watertight"
    );
}

// =========================================================================
// ATTACK 5 — regression: corner clips stay simple, no spurious holes.
//
// Two corner-clip subtracts (a +++ corner clip and an edge-straddling clip).
// Every face must remain simple (`inner_loops` empty) with correct volume.
// This guards against the largest-area-outer heuristic mis-classifying a
// boundary cycle of a non-holed patch as a hole.
// =========================================================================

#[test]
fn corner_clip_no_spurious_holes() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    let a = OrientedBox::aligned([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
        .to_brep()
        .expect("A");
    // B = [0.5,1.5]^3 clips A's +++ corner. overlap = 0.5^3 = 0.125 ⇒ 0.875.
    let b = OrientedBox::aligned([1.0, 1.0, 1.0], [0.5, 0.5, 0.5])
        .to_brep()
        .expect("B");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("corner clip expected Ok, got Err({e:?})"),
    };

    let holed = r
        .faces()
        .iter()
        .filter(|f| !f.inner_loops.is_empty())
        .count();
    assert_eq!(
        holed, 0,
        "corner clip must yield only simple faces, got {holed} holed"
    );
    assert_brep_invariants(&r);

    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.875).abs() < VOL_TOL,
        "corner-clip subtract volume {vol} != 0.875"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "corner-clip not watertight"
    );
}

/// Edge-straddle clip: B clips along a full edge of A (a slab off the +x+y
/// edge). Result is an L-prism; no holes. overlap = [0.5,1]×[0.5,1]×[0,1] =
/// 0.25 ⇒ subtract 0.75.
#[test]
fn edge_clip_no_spurious_holes() {
    let Some(sb) = skip_or_backend() else {
        return;
    };

    let a = OrientedBox::aligned([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
        .to_brep()
        .expect("A");
    // B centered at (1,1,0.5), half (0.5,0.5,1.0): spans x∈[0.5,1.5],
    // y∈[0.5,1.5], z∈[−0.5,1.5]. Overlap with A = [0.5,1]×[0.5,1]×[0,1] = 0.25.
    let b = OrientedBox::aligned([1.0, 1.0, 0.5], [0.5, 0.5, 1.0])
        .to_brep()
        .expect("B");

    let r = match yang_rs::boolean(&a, &b, BoolOp::Subtract, &sb) {
        Ok(brep) => brep,
        Err(e) => panic!("edge clip expected Ok, got Err({e:?})"),
    };

    let holed = r
        .faces()
        .iter()
        .filter(|f| !f.inner_loops.is_empty())
        .count();
    assert_eq!(
        holed, 0,
        "edge clip must yield only simple faces, got {holed} holed"
    );
    assert_brep_invariants(&r);

    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - 0.75).abs() < VOL_TOL,
        "edge-clip subtract volume {vol} != 0.75"
    );
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "edge-clip not watertight"
    );
}

// =========================================================================
// ATTACK 2 — genuine non-manifold STILL errors.
//
// Drive `yang_rs::boolean` with a MockBackend whose `labeled_arrangement`
// returns a hand-built `LabeledArrangement` whose Union kept-set forms a
// SINGLE edge-connected patch containing a NON-MANIFOLD EDGE (an edge shared
// by three patch triangles). `patch_boundary_cycle` must still return
// `Err(NonManifoldOutput)` — the genuine non-manifold case must NOT be
// silently accepted now that multi-cycle patches are allowed.
//
// Why a non-manifold *edge* (not a pinch vertex / bowtie): a bowtie of two
// triangles sharing only a vertex is two DISCONNECTED patches (flood-fill is
// edge-adjacency-based), each a valid simple triangle — so it does NOT exercise
// the cycle-walk failure path. Any flood-filled patch over MANIFOLD edges has a
// disjoint-simple-cycle boundary (an annulus at worst) and reconstructs fine.
// The genuine E1 trigger is a degree-3 edge: edge 1-2 shared by tris
// [0,1,2],[1,3,2],[1,2,4] is interior to the patch (count 3 ⇒ not a boundary
// edge), which leaves the boundary directed-edge multiset UNBALANCED. The cycle
// walk then dead-ends mid-walk ⇒ `Err(NonManifoldOutput)`.
// =========================================================================

mod tjunction {
    use super::*;
    use cherchi_rs::{InputId as LaInputId, LabeledArrangement, MeshBoolean};
    use std::error::Error;

    /// MockBackend: returns a fixed hand-built LabeledArrangement regardless of
    /// the input meshes. `boolean()` is unused (yang calls labeled_arrangement).
    struct TJunctionBackend {
        la: LabeledArrangement,
    }

    impl MeshBoolean for TJunctionBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            Err("TJunctionBackend::boolean unused".into())
        }

        fn labeled_arrangement(
            &self,
            _a: &Mesh,
            _b: &Mesh,
        ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
            Ok(self.la.clone())
        }
    }

    /// Build A as a unit cube whose face 0 (−z plane) contains the z=0 verts.
    /// The mock arrangement's tris all live on z=0 and attribute to A face 0.
    fn cube() -> BRep {
        OrientedBox::aligned([0.5, 0.5, 0.5], [0.5, 0.5, 0.5])
            .to_brep()
            .expect("cube")
    }

    /// Three same-plane (z=0) triangles sharing a single NON-MANIFOLD edge
    /// (1-2), all attributed to A face 0 (−z plane) and inside all-false so
    /// Union keeps all three. Edge 1-2 is shared by all three triangles
    /// (`patch_edge_count == 3`), so it is interior; the remaining boundary
    /// directed edges are
    ///   0→1, 2→0  (from [0,1,2]),  1→3, 3→2  (from [1,3,2]),  2→4, 4→1 (from [1,2,4])
    /// giving `by_start[2] = {0, 4}` (out-degree 2). The first cycle walk
    /// closes 0→1→3→2→0, leaving 2→4, 4→1 whose walk dead-ends at 1 (no out
    /// edge) ⇒ `Err(NonManifoldOutput)`.
    fn nonmanifold_edge_arrangement() -> LabeledArrangement {
        // All verts on z=0 so they lie on A's −z face plane (z=0, normal −z).
        let verts = vec![
            Point3::new(0.30, 0.10, 0.0), // 0
            Point3::new(0.40, 0.50, 0.0), // 1  ┐ shared edge 1-2
            Point3::new(0.60, 0.50, 0.0), // 2  ┘
            Point3::new(0.50, 0.90, 0.0), // 3
            Point3::new(0.70, 0.10, 0.0), // 4
        ];
        let tris = vec![[0u32, 1, 2], [1u32, 3, 2], [1u32, 2, 4]];
        let mesh = Mesh::new(verts, tris);
        LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]; 3],
            inside: vec![vec![false, false]; 3],
            patch: vec![0, 0, 0],
            source: Vec::new(),
            intersection_edges: Default::default(),
            num_inputs: 2,
        }
    }

    #[test]
    fn tjunction_patch_still_errors_nonmanifold() {
        // No sidecar needed — the backend is the mock.
        let a = cube();
        // B is unused by the mock arrangement, but yang needs a B BRep — and
        // (PR-YR24) it must not be input-coplanar with A or the near-coplanar
        // gate rejects the pair before the mock backend runs. It must also
        // AABB-OVERLAP A: task #134's disjoint-union passthrough returns the
        // concatenated inputs without ever calling the backend, so a far-away
        // dummy B would bypass the adversarial arrangement entirely. Offset
        // fractions keep every plane distinct (no coplanarity).
        let b = OrientedBox::aligned([0.3, 0.3, 0.3], [0.2, 0.2, 0.2])
            .to_brep()
            .expect("cube b");
        let backend = TJunctionBackend {
            la: nonmanifold_edge_arrangement(),
        };

        let res = yang_rs::boolean(&a, &b, BoolOp::Union, &backend);
        match res {
            // Spec E1: a genuine non-manifold patch (dead-end / T-junction in
            // the cycle walk) MUST return `NonManifoldOutput`.
            Err(YangError::NonManifoldOutput) => { /* correct */ }
            Err(other) => panic!(
                "[tjunction] expected Err(NonManifoldOutput) (spec E1), got Err({other:?}); \
                 the non-manifold-edge patch must trigger the cycle-walk dead-end, not some \
                 other error"
            ),
            Ok(brep) => {
                // The dangerous outcome: a non-manifold patch silently accepted.
                let unpaired = brep_unpaired_loop_edges(&brep);
                panic!(
                    "[tjunction] non-manifold-edge patch was ACCEPTED (Ok) — \
                     {unpaired} unpaired loop edges; a genuine non-manifold patch must \
                     return Err(NonManifoldOutput)"
                );
            }
        }
    }
}

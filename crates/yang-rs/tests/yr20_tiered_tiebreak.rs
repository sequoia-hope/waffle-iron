//! PR-YR20 RED — Stage-6 tiered face-resolution tie-break.
//!
//! Spec of record: `specs/yr20_face_resolution_tiered_tiebreak.md` §7.
//!
//! These tests are written by the RED test-author sub-agent and run against the
//! UNMODIFIED `crates/yang-rs/src` so the cap-vs-curved tie case currently
//! FAILS with `FaceResolutionFailed`. The GREEN sub-agent's §5 change (tiered
//! exact-over-band tie-break) flips it to a clean cap attribution.
//!
//! ## Test 1 — `cap_vs_curved_tie_resolves_to_cap` (currently RED)
//!
//! A **closed cylinder** boolean (Union, A=A so the result is the cylinder)
//! driven by a hand-built [`LabeledArrangement`] mock. The cylinder's top cap is
//! deliberately triangulated as a two-ring strip (rim ring at `r = R` + an inner
//! ring at `r = R_INNER`, both in the `z = H` plane, fanned to the top centre).
//! The strip triangles with **two rim verts + one inner vert** have a centroid
//! that is:
//!
//!   * EXACTLY on the top-cap plane `z = H` (`|c.z − H| < TAU_WORK = 1e-12`), and
//!   * within the cylinder lateral's Stage-1 chord band `d_ε = 1e-2·diag = 0.03`
//!     (the centroid radius `r_c ≈ (R + R + R_INNER)/3 = 0.983`, so
//!     `|r_c − R| ≈ 0.017 < d_ε`).
//!
//! That is an `n_hits == 2` cap-vs-curved tie. Today Stage-6 raises
//! `FaceResolutionFailed` on the first such triangle. Post-fix the EXACT-tier cap
//! hit (`dist < TAU_WORK`) dominates the BAND-tier lateral hit, so the triangle
//! attributes to the top-cap plane and the boolean succeeds.
//!
//! The tie MAGNITUDE is load-bearing and asserted directly in the test (cap dist
//! `< TAU_WORK` AND lateral dist in `[TAU_WORK, d_ε)`), so the fixture cannot
//! silently stop being a genuine tie.
//!
//! Post-fix assertions (these FAIL today):
//!   1. `boolean()` returns `Ok`.
//!   2. The identified near-rim tie triangle attributes to the **top-cap plane**
//!      face (index 2), NOT the cylinder lateral (index 0).
//!   3. Output is watertight 2-manifold; `χ = 2 − 2g = 2` (genus 0);
//!      `signed_volume > 0` (orientation witness — a hand-built mock can pass
//!      watertight + χ while globally inside-out, memory
//!      `yang_mock_orientation_witness`).
//!
//! ## Test 2 — `all_planar_coplanar_tie_still_fails_resolution` (passes today AND
//! after fix)
//!
//! The safety canary, modelled on
//! `m3_adversary.rs::a6_equidistant_two_planes_tie_fails_resolution`: a genuine
//! all-planar coplanar tie (centroid equidistant — distance 0 — to TWO of solid
//! A's face planes, both well within `TAU_WORK`). Both hits are EXACT-tier, so
//! `n_exact ≥ 2` ⇒ the tie path still raises `FaceResolutionFailed`. This proves
//! the fix only re-ranks a MIXED exact-planar-vs-curved-band tie and leaves the
//! genuine multi-plane EXACT tie intact (spec §4.1 byte-identity).

use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, InputId, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Shared mock backend (identical shape to m3_adversary / yr13 LabelMock):
// boolean() consumes the hand-built arrangement directly as both the output
// mesh and the labeled arrangement.
// =========================================================================

struct LabelMock {
    arrangement: LabeledArrangement,
}
impl MeshBoolean for LabelMock {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.clone())
    }
}

// =========================================================================
// Tiny array helpers + invariant oracles (independent of production math).
// =========================================================================

fn centroid(verts: &[Point3], tri: [u32; 3]) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ]
}

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
    use std::collections::HashMap;
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

fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::HashSet;
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    v - edges.len() as i64 + f
}

// =========================================================================
// TEST 1 fixture: a hand-built closed cylinder mesh + matching BRep.
//
// Cylinder: axis +z through the origin, radius R, height H, N segments.
//   faces: 0 = lateral (Cylinder), 1 = bottom cap (Plane −z), 2 = top cap
//   (Plane +z).
//
// N is chosen so the flat lateral triangles' centroids stay inside the
// cylinder's own Stage-1 chord band d_ε; the top-cap two-ring strip introduces
// the cap-vs-curved tie.
// =========================================================================

const N: usize = 24;
const R: f64 = 1.0;
const H: f64 = 1.0;
/// Top-cap inner-ring radius. Picked so the 2-rim+1-inner strip triangle's
/// centroid radius `(R + R + R_INNER)/3 = 0.983` lands inside the lateral band
/// `d_ε = 0.03` ⇒ genuine cap-vs-curved tie.
const R_INNER: f64 = 0.95;
/// `d_ε = 1e-2 · AABB-diagonal`. For R=1, H=1 the rim-AABB spans 2R in x and y
/// and H in z ⇒ diag = √(4 + 4 + 1) = 3 ⇒ d_ε = 0.03. (This MUST equal the
/// production `curved_chord_bound` over the two rim circles — asserted in the
/// test by the tie-magnitude check.)
const D_EPS: f64 = 0.03;

const FACE_LATERAL: u32 = 0;
const FACE_BOT_CAP: u32 = 1;
const FACE_TOP_CAP: u32 = 2;

/// Build the closed cylinder mesh. Returns `(verts, tris, patch)` where `patch`
/// groups triangles by their cylinder face (lateral / bottom cap / top cap), so
/// Stage-6 flood-fill reconstructs three patches → three B-Rep faces.
fn build_cylinder_mesh() -> (Vec<Point3>, Vec<[u32; 3]>, Vec<u32>) {
    let mut verts: Vec<Point3> = Vec::new();
    // bottom rim   0 ..  N   (r = R, z = 0)
    for i in 0..N {
        let a = 2.0 * std::f64::consts::PI * (i as f64) / (N as f64);
        verts.push(p(R * a.cos(), R * a.sin(), 0.0));
    }
    // top rim      N .. 2N   (r = R, z = H)
    for i in 0..N {
        let a = 2.0 * std::f64::consts::PI * (i as f64) / (N as f64);
        verts.push(p(R * a.cos(), R * a.sin(), H));
    }
    // top inner   2N .. 3N   (r = R_INNER, z = H)
    for i in 0..N {
        let a = 2.0 * std::f64::consts::PI * (i as f64) / (N as f64);
        verts.push(p(R_INNER * a.cos(), R_INNER * a.sin(), H));
    }
    let bot_center = verts.len() as u32; // 3N
    verts.push(p(0.0, 0.0, 0.0));
    let top_center = verts.len() as u32; // 3N + 1
    verts.push(p(0.0, 0.0, H));

    let br = |i: usize| (i % N) as u32; // bottom rim index
    let tr = |i: usize| (N + (i % N)) as u32; // top rim index
    let ir = |i: usize| (2 * N + (i % N)) as u32; // top inner-ring index

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut patch: Vec<u32> = Vec::new();

    // Lateral barrel: each quad (br i, br i+1, tr i+1, tr i) → 2 outward tris.
    for i in 0..N {
        tris.push([br(i), br(i + 1), tr(i + 1)]);
        patch.push(FACE_LATERAL);
        tris.push([br(i), tr(i + 1), tr(i)]);
        patch.push(FACE_LATERAL);
    }
    // Bottom cap: outward (−z) centre fan.
    for i in 0..N {
        tris.push([bot_center, br(i + 1), br(i)]);
        patch.push(FACE_BOT_CAP);
    }
    // Top cap: rim↔inner strip (the tie-bearing rows) + inner↔centre fan,
    // all outward (+z).
    for i in 0..N {
        // 2 rim + 1 inner  → centroid r ≈ 0.983 ⇒ cap-vs-curved TIE.
        tris.push([tr(i), tr(i + 1), ir(i + 1)]);
        patch.push(FACE_TOP_CAP);
        // 1 rim + 2 inner  → centroid r ≈ 0.967 (just outside the band) ⇒ cap.
        tris.push([tr(i), ir(i + 1), ir(i)]);
        patch.push(FACE_TOP_CAP);
    }
    for i in 0..N {
        tris.push([top_center, ir(i), ir(i + 1)]);
        patch.push(FACE_TOP_CAP);
    }

    (verts, tris, patch)
}

/// The cylinder B-Rep that matches `build_cylinder_mesh` (axis +z, R, H). Face
/// order matches the mesh's `patch` ids: 0 lateral, 1 bottom cap, 2 top cap.
/// (Seam-edge encoding per the yr7/yr13 `cylinder_brep` convention; this is the
/// `a`/`b` argument to `boolean()` — its `as_mesh()` is ignored by `LabelMock`,
/// but `boolean()` reads its `faces()`/`edges()` for Stage-6 resolution.)
fn cyl_brep_at(z0: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(R, 0.0, z0),
        },
        BRepVertex {
            point: p(R, 0.0, z0 + H),
        },
    ];
    let edges = vec![
        // e0 bottom rim Circle
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: R,
            },
        },
        // e1 top rim Circle
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z0 + H),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: R,
            },
        },
        // e2 seam
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        // f0 lateral Cylinder
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, z0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: R,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 bottom cap Plane (−z), n·x + d = 0 at z = z0 ⇒ d = z0
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 top cap Plane (+z), n·x + d = 0 at z = z0 + H ⇒ d = −(z0 + H)
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -(z0 + H),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("cyl_brep: BRep::new must tessellate")
}

#[test]
fn cap_vs_curved_tie_resolves_to_cap() {
    let (verts, tris, patch) = build_cylinder_mesh();

    // --- (a) The fixture mesh is genuinely watertight (sanity; not the SUT). ---
    {
        let mesh = Mesh::new(verts.clone(), tris.clone());
        assert_eq!(
            unpaired_half_edges(&mesh),
            0,
            "fixture cylinder mesh must be watertight before driving boolean()"
        );
    }

    // --- (b) The cap-vs-curved tie MAGNITUDE is load-bearing: locate the first
    //         top-cap triangle whose centroid is EXACTLY on the top-cap plane
    //         (dist < TAU_WORK) AND inside the lateral band (dist in
    //         [TAU_WORK, d_ε)). This is the n_hits == 2 tie. ---
    let tau_work = cad_primitives::TAU_WORK;
    let mut tie_tri: Option<usize> = None;
    for (ti, &tri) in tris.iter().enumerate() {
        if patch[ti] != FACE_TOP_CAP {
            continue;
        }
        let c = centroid(&verts, tri);
        let cap_dist = (c[2] - H).abs(); // distance to top-cap plane z = H
        let r_c = (c[0] * c[0] + c[1] * c[1]).sqrt();
        let lat_dist = (r_c - R).abs(); // distance to the cylinder lateral
        let exact_on_cap = cap_dist < tau_work;
        let band_on_lateral = lat_dist >= tau_work && lat_dist < D_EPS;
        if exact_on_cap && band_on_lateral {
            tie_tri = Some(ti);
            break;
        }
    }
    let tie_tri = tie_tri.expect(
        "fixture must contain a cap-vs-curved tie triangle (EXACT on cap, BAND on lateral)",
    );

    // Re-assert the tie magnitude explicitly on the identified triangle so the
    // load-bearing condition is visible in the test body.
    {
        let c = centroid(&verts, tris[tie_tri]);
        let cap_dist = (c[2] - H).abs();
        let r_c = (c[0] * c[0] + c[1] * c[1]).sqrt();
        let lat_dist = (r_c - R).abs();
        assert!(
            cap_dist < tau_work,
            "tie tri {tie_tri}: cap distance {cap_dist:.3e} must be < TAU_WORK {tau_work:.1e} \
             (EXACT-tier cap hit)"
        );
        assert!(
            lat_dist >= tau_work && lat_dist < D_EPS,
            "tie tri {tie_tri}: lateral distance {lat_dist:.3e} must be in \
             [TAU_WORK {tau_work:.1e}, d_ε {D_EPS}) (BAND-tier lateral hit)"
        );
    }

    // --- (c) Drive boolean() through the mock. Union with all tris labelled on
    //         solid A and inside neither solid keeps every triangle unflipped,
    //         so the result is the cylinder itself. ---
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let arrangement = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]; n], // every tri on solid A's surface
        inside: vec![vec![false, false]; n],  // outside both solids
        patch,
        num_inputs: 2,
    };
    let backend = LabelMock { arrangement };
    let a = cyl_brep_at(0.0);
    // PR-YR24: B was a COINCIDENT copy of A, whose planar cap pairs the
    // near-coplanar input gate now rejects before the (mock) backend runs.
    // The arrangement labels every tri to solid A, so B's geometry is unused
    // by resolution — shift it well clear of A's cap planes.
    let b = cyl_brep_at(3.0 * H);

    let r = boolean(&a, &b, BoolOp::Union, &backend);

    // 1. Post-fix: Ok (today: Err FaceResolutionFailed on the tie tri).
    let out = match r {
        Ok(out) => out,
        Err(e) => {
            panic!("cap-vs-curved tie must resolve (Ok) post-fix; tie tri = {tie_tri}, got {e:?}")
        }
    };

    // 2. The tie triangle attributes to the TOP-CAP plane (face 2), not the
    //    cylinder lateral (face 0). `boolean()` may reorder triangles when it
    //    compacts the kept sub-mesh, but Union keeps all tris in order and drops
    //    none here, so the output tri index equals the input index. Verify by
    //    geometry to be order-independent: find the output tri with the same
    //    centroid as the input tie tri and assert its attribution.
    let attr = out.triangle_attribution();
    assert_eq!(
        attr.len(),
        out.num_tris(),
        "attribution must cover every output triangle"
    );
    // Match the OUTPUT tri by reconstructing the input tie centroid
    // deterministically.
    let tie_centroid = {
        let (v, t, _) = build_cylinder_mesh();
        centroid(&v, t[tie_tri])
    };
    let mut matched: Option<u32> = None;
    for t in 0..out.num_tris() as u32 {
        let c = centroid(&out.as_mesh().verts, out.as_mesh().tris[t as usize]);
        let d = ((c[0] - tie_centroid[0]).powi(2)
            + (c[1] - tie_centroid[1]).powi(2)
            + (c[2] - tie_centroid[2]).powi(2))
        .sqrt();
        if d < 1e-9 {
            matched = Some(t);
            break;
        }
    }
    let out_tie = matched.expect("the tie triangle must survive into the output mesh (Union)");
    let a_tie = attr
        .lookup(out_tie)
        .expect("the tie triangle must be attributed (no skeleton None)");
    assert_eq!(
        a_tie.input,
        InputId::A,
        "tie tri must be attributed to solid A"
    );
    assert_eq!(
        a_tie.face, FACE_TOP_CAP,
        "tie tri must attribute to the TOP-CAP plane (face {FACE_TOP_CAP}), \
         NOT the cylinder lateral (face {FACE_LATERAL}); got face {}",
        a_tie.face
    );

    // 3. Output is watertight 2-manifold; χ = 2 − 2g = 2 (genus 0);
    //    signed_volume > 0 (orientation witness).
    assert_eq!(
        unpaired_half_edges(out.as_mesh()),
        0,
        "output must be watertight (no unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(out.as_mesh()),
        2,
        "output Euler χ must be 2 (genus 0 closed shell)"
    );
    let vol = signed_volume(out.as_mesh());
    assert!(
        vol > 0.0,
        "output signed volume must be positive (outward orientation witness); got {vol}"
    );
}

// =========================================================================
// TEST 2 — safety canary: a genuine all-planar coplanar tie must STILL
// FaceResolutionFailed. Self-contained adaptation of
// m3_adversary.rs::a6_equidistant_two_planes_tie_fails_resolution.
// =========================================================================

/// Axis-aligned unit cube B-Rep at `origin` (outward normals, correct plane
/// offsets `n·x + d = 0`). Self-contained (no call into m3_adversary).
fn cube(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let verts = vec![
        BRepVertex { point: p(x, y, z) },
        BRepVertex {
            point: p(x + 1.0, y, z),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        },
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
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
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
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
    BRep::new(verts, edges, faces).expect("cube BRep::new failed")
}

#[test]
fn all_planar_coplanar_tie_still_fails_resolution() {
    // A real (positive-area) triangle in plane z = 0.5 whose centroid is
    // (0, 0, 0.5): distance 0 to plane x = 0 AND to plane y = 0 (both ≪
    // TAU_WORK) — two EXACT-tier hits. Every other A plane is ≥ 0.5 away. Two
    // planes tie at distance 0 ⇒ n_exact ≥ 2 ⇒ still FaceResolutionFailed.
    //
    // This is the spec §4.1 byte-identity guarantee: for all-planar inputs every
    // hit is EXACT (planar tol == TAU_WORK), so the BAND tier is empty and the
    // tiered rule degenerates to the old "exactly one face within TAU_WORK"
    // rule — a genuine ≥2-EXACT tie is unchanged.
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    let verts = vec![p(-0.5, -0.5, 0.5), p(0.5, -0.5, 0.5), p(0.0, 1.0, 0.5)];
    let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
    let arrangement = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]],
        inside: vec![vec![false, false]],
        patch: vec![0],
        num_inputs: 2,
    };
    let backend = LabelMock { arrangement };
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::FaceResolutionFailed { tri }) => assert_eq!(tri, 0),
        other => panic!("expected FaceResolutionFailed (all-planar EXACT tie), got {other:?}"),
    }
}

// =========================================================================
// TEST 3 (ADVERSARY, PR-YR20 §9) — a genuine 0-EXACT + 2-BAND CURVED tie must
// STILL raise FaceResolutionFailed under the new tiered rule.
//
// The GREEN match has FOUR live arms; the RED file exercised:
//   (1, Some, _, _)        — unique exact hit       (cap_vs_curved_tie...)
//   (≥2 exact)             — genuine EXACT tie       (all_planar_coplanar...)
// and implicitly the no-match arm. It did NOT exercise the
//   (0, _, ≥2, Some) → _   — genuine BAND tie
// arm with two real band hits. This adversary fixture drives exactly that arm:
// a kept triangle whose centroid is within the SHARED Stage-1 chord band of TWO
// distinct coaxial cylinder faces and EXACTLY on NEITHER (0 exact, 2 band) ⇒
// the tie path must still refuse (FaceResolutionFailed), proving the fix did
// NOT silently start attributing curved ties.
//
// Construction (all arithmetic deterministic, no rand / time / FS):
//   Input B-Rep = two coaxial cylinders (axis +z) of radii R1=1.00, R2=1.04,
//   each with two rim circles at z=0 and z=H=1 (4 Circle edges total). The
//   production band is `curved_chord_bound(edges)` = 1e-2 · diag(AABB of all 4
//   rims). AABB spans 2·R2 in x and y and H in z ⇒ diag = √(2.08² + 2.08² + 1²)
//   ≈ 3.1069 ⇒ band ≈ 0.031069. Both cylinder faces share this single band.
//
//   The single kept triangle's centroid sits at radius r_c = 1.02 (exactly
//   midway between the two cylinders), z = 0.5:
//     dist to cyl R1 = |1.02 − 1.00| = 0.02 ∈ [TAU_WORK, band)  → BAND hit
//     dist to cyl R2 = |1.02 − 1.04| = 0.02 ∈ [TAU_WORK, band)  → BAND hit
//     neither < TAU_WORK ⇒ 0 EXACT, 2 BAND ⇒ (0,_,2,Some) ⇒ F3.
//
// If a future change made the band tier pick a "closest" band hit (the ratio
// variant the spec §3 explicitly rejected), this would flip to Ok and this test
// would fail — that is its guard value.
// =========================================================================

/// Two coaxial cylinder faces (axis +z) of radii `R1`/`R2`, each face bounded
/// by its own bottom+top rim Circle edges. No cap faces (not needed — the mock
/// supplies the output mesh; `boolean()` only reads `faces()`/`edges()` here).
fn two_cylinder_brep() -> BRep {
    const R1: f64 = 1.00;
    const R2: f64 = 1.04;
    const HH: f64 = 1.0;
    // Seam vertices (one per rim, reused as ring[0] by the lateral tessellation).
    let verts = vec![
        BRepVertex {
            point: p(R1, 0.0, 0.0),
        }, // v0 cyl1 bottom seam
        BRepVertex {
            point: p(R1, 0.0, HH),
        }, // v1 cyl1 top seam
        BRepVertex {
            point: p(R2, 0.0, 0.0),
        }, // v2 cyl2 bottom seam
        BRepVertex {
            point: p(R2, 0.0, HH),
        }, // v3 cyl2 top seam
    ];
    let edges = vec![
        // e0 cyl1 bottom rim
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: R1,
            },
        },
        // e1 cyl1 top rim
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, HH),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: R1,
            },
        },
        // e2 cyl1 seam
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        // e3 cyl2 bottom rim
        BRepEdge {
            start: 2,
            end: 2,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: R2,
            },
        },
        // e4 cyl2 top rim
        BRepEdge {
            start: 3,
            end: 3,
            curve: Curve::Circle {
                center: p(0.0, 0.0, HH),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: R2,
            },
        },
        // e5 cyl2 seam
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        // f0 cyl1 lateral
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: R1,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 cyl2 lateral
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: R2,
            },
            outer_loop: vec![3, 5, 4, 5],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("two_cylinder_brep: BRep::new must tessellate")
}

#[test]
fn curved_band_tie_two_cylinders_still_fails_resolution() {
    // A real (positive-area) triangle whose centroid is at radius 1.02, z=0.5:
    // 0.02 from BOTH cylinder surfaces (R1=1.00, R2=1.04), exactly on neither.
    // Pick three verts whose mean is (1.02, 0.0, 0.5).
    let verts = vec![
        p(1.02, -0.10, 0.40),
        p(1.02, 0.10, 0.40),
        p(1.02, 0.00, 0.70),
    ];
    // Centroid = (1.02, 0.0, 0.5); r_c = 1.02 (the x/y mean lies on the x-axis).
    let cx = (verts[0].as_array()[0] + verts[1].as_array()[0] + verts[2].as_array()[0]) / 3.0;
    let cy = (verts[0].as_array()[1] + verts[1].as_array()[1] + verts[2].as_array()[1]) / 3.0;
    let r_c = (cx * cx + cy * cy).sqrt();

    // Load-bearing tie magnitude: assert 0-EXACT + 2-BAND BEFORE driving the SUT
    // so the fixture cannot silently degrade to a non-tie. Band must equal the
    // production `curved_chord_bound` over the four rim circles.
    let band = {
        // Reproduce curved_chord_bound's AABB over the 4 rims (R2 dominates xy).
        let span_xy = 2.0 * 1.04_f64;
        let diag = (span_xy * span_xy + span_xy * span_xy + 1.0_f64).sqrt();
        1e-2 * diag
    };
    let tau = cad_primitives::TAU_WORK;
    let d1 = (r_c - 1.00).abs();
    let d2 = (r_c - 1.04).abs();
    assert!(
        d1 >= tau && d1 < band,
        "cyl1 dist {d1:.4e} must be in [TAU_WORK {tau:.1e}, band {band:.4e}) (BAND-tier, not EXACT)"
    );
    assert!(
        d2 >= tau && d2 < band,
        "cyl2 dist {d2:.4e} must be in [TAU_WORK {tau:.1e}, band {band:.4e}) (BAND-tier, not EXACT)"
    );

    let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
    let arrangement = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]], // on solid A's surface
        inside: vec![vec![false, false]],  // kept by Union
        patch: vec![0],
        num_inputs: 2,
    };
    let backend = LabelMock { arrangement };
    let a = two_cylinder_brep();
    let b = two_cylinder_brep();

    // 0 EXACT + 2 BAND ⇒ the (0,_,≥2,_) arm ⇒ still FaceResolutionFailed.
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::FaceResolutionFailed { tri }) => assert_eq!(tri, 0),
        other => panic!(
            "expected FaceResolutionFailed (genuine 0-EXACT/2-BAND curved tie); got {other:?}"
        ),
    }
}

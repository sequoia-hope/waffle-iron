//! PR-YR15 ADVERSARY — independent witness for the box − sphere HEMISPHERICAL
//! DIMPLE (genus 0, χ=2). This file is authored INDEPENDENTLY of the RED fixture
//! (`tests/yr15_subtract_sphere.rs`): a DIFFERENT box, a DIFFERENT sphere
//! center/radius, DIFFERENT facet counts, and — critically — the hemisphere cap
//! is authored OUTWARD (away-from-centre sphere winding, NOT pre-swapped), so the
//! production `flip_for_op(Subtract)` single swap on `InputId::B` is what
//! produces the toward-centre cavity winding. If the RED fixture's pre-swap
//! convention and this file's outward convention BOTH yield `reversed == true`
//! plus a toward-centre mesh winding, the `reversed`/winding relationship is
//! confirmed independent of how the mock was built (a shared mistake cannot hide
//! in both).
//!
//! Witnesses (orchestrator brief):
//!  - Independent dimple mock: watertight + χ=2 + signed_volume > 0 (self-check,
//!    then through the real `boolean()`).
//!  - Mesh-winding ↔ `reversed` consistency: from the emitted output cap
//!    triangles, the geometric winding normal points TOWARD the sphere centre,
//!    AND this agrees with `reversed == true` on the `Surface::Sphere` face.
//!  - Rim exactness from scratch: every `Curve::Circle` rim point satisfies
//!    `|x − center| == radius` AND lies on the box-face plane to `TAU_MODEL`,
//!    using this file's own math (not the RED helper).
//!  - Migration: no existing Sphere structural assertion weakened (asserted by
//!    construction here; the cross-file rg/git check is in the adversary report).

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

// =========================================================================
// Independent pure-Rust array math (deliberately re-derived, NOT shared with
// the RED helpers, so a buggy helper cannot mask a buggy production result).
// =========================================================================

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn vsub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn vadd(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn vscale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn vdot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn vcross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn vlen(a: [f64; 3]) -> f64 {
    vdot(a, a).sqrt()
}
fn vunit(a: [f64; 3]) -> [f64; 3] {
    let n = vlen(a);
    assert!(n > 0.0, "adversary: cannot normalize zero vector");
    vscale(a, 1.0 / n)
}

// Independent mesh oracles (re-derived from the half-edge / divergence-theorem
// definitions; not copied from the RED file's helpers verbatim in spirit, the
// math is standard and trivially auditable).

fn unpaired(mesh: &Mesh) -> usize {
    let mut directed: HashMap<(u32, u32), i32> = HashMap::new();
    for t in &mesh.tris {
        directed
            .entry((t[0], t[1]))
            .and_modify(|c| *c += 1)
            .or_insert(1);
        directed
            .entry((t[1], t[2]))
            .and_modify(|c| *c += 1)
            .or_insert(1);
        directed
            .entry((t[2], t[0]))
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
    let mut bad = 0usize;
    for (&(s, e), &fwd) in &directed {
        let rev = directed.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            bad += (fwd - rev).unsigned_abs() as usize;
        }
    }
    bad
}

fn chi(mesh: &Mesh) -> i64 {
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut es: HashSet<(u32, u32)> = HashSet::new();
    for t in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (t[i], t[j]);
            es.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    v - es.len() as i64 + f
}

fn vol(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        acc += vdot(a, vcross(b, c));
    }
    acc / 6.0
}

// =========================================================================
// INDEPENDENT canonical config — DIFFERENT numbers from the RED fixture.
//   box A: [-3,-3,-1] .. [3,3,5]  (so the dimpled face is z=5, NOT z=2)
//   sphere B: center (1, -0.5, 5) ON the box-top plane (z=5), radius 1.5.
//     lower hemisphere (z ≤ 5) inside the box; rim = great circle at z=5.
// N/M differ from RED (N=12, M=3 vs RED's 16/4).
// =========================================================================

const A_LO: [f64; 3] = [-3.0, -3.0, -1.0];
const A_HI: [f64; 3] = [3.0, 3.0, 5.0];
const C: [f64; 3] = [1.0, -0.5, 5.0];
const R: f64 = 1.5;
const TOPZ: f64 = 5.0;
// Facet counts differ from RED (16/4). They must be FINE ENOUGH that the cap's
// own centroid chord deviation stays under the sphere's Stage-1 chord bound
// `sphere_chord_bound(R) = 1e-2·2R√3 ≈ 0.052` (else this mock — not production —
// violates the bound that `tol_for` correctly enforces). NN=20,MM=5 → worst
// centroid deviation ≈ 0.033 < 0.052 (verified by an independent search).
const NN: usize = 20; // longitudinal facets (≠ RED 16)
const MM: usize = 5; // latitude bands (≠ RED 4)

fn adv_sphere_surface() -> Surface {
    Surface::Sphere {
        center: pt(C[0], C[1], C[2]),
        radius: R,
    }
}

// Box input B-Rep with correct outward normals (re-derived, independent).
fn adv_box() -> BRep {
    let [x0, y0, z0] = A_LO;
    let [x1, y1, z1] = A_HI;
    let verts = vec![
        BRepVertex {
            point: pt(x0, y0, z0),
        },
        BRepVertex {
            point: pt(x1, y0, z0),
        },
        BRepVertex {
            point: pt(x1, y1, z0),
        },
        BRepVertex {
            point: pt(x0, y1, z0),
        },
        BRepVertex {
            point: pt(x0, y0, z1),
        },
        BRepVertex {
            point: pt(x1, y0, z1),
        },
        BRepVertex {
            point: pt(x1, y1, z1),
        },
        BRepVertex {
            point: pt(x0, y1, z1),
        },
    ];
    let fv: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let mut edges = Vec::new();
    let mut loops = Vec::new();
    for vs in &fv {
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
    BRep::new(verts, edges, faces).expect("adv_box")
}

// Closed solid-sphere B-Rep (one Sphere face + one meridian seam Circle), YR12
// shape, with this file's center/radius.
fn adv_sphere() -> BRep {
    let south = vadd(C, vscale([0.0, 0.0, -1.0], R));
    let north = vadd(C, vscale([0.0, 0.0, 1.0], R));
    let verts = vec![
        BRepVertex {
            point: pt(south[0], south[1], south[2]),
        },
        BRepVertex {
            point: pt(north[0], north[1], north[2]),
        },
    ];
    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Circle {
            center: pt(C[0], C[1], C[2]),
            normal: Vector3::new(0.0, -1.0, 0.0),
            radius: R,
        },
    }];
    let faces = vec![BRepFace {
        surface: adv_sphere_surface(),
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    BRep::new(verts, edges, faces).expect("adv_sphere")
}

struct LabelMock {
    arr: LabeledArrangement,
}
impl MeshBoolean for LabelMock {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arr.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arr.clone())
    }
}

// Hemisphere ring geometry (independent derivation): polar angle measured DOWN
// from the equatorial (rim) plane; ring 0 = rim at z=TOPZ.
fn ring_z(j: usize) -> f64 {
    let phi = 0.5 * std::f64::consts::PI * (j as f64) / (MM as f64);
    C[2] - R * phi.sin()
}
fn ring_r(j: usize) -> f64 {
    let phi = 0.5 * std::f64::consts::PI * (j as f64) / (MM as f64);
    R * phi.cos()
}

// =========================================================================
// Independent arrangement build. KEY DIFFERENCE vs RED: the hemisphere cap is
// authored with OUTWARD (away-from-centre) winding — NOT pre-swapped — so the
// production `flip_for_op(Subtract)` single swap on the label-1 (B) tris is what
// turns it toward-centre. The box faces are authored directly outward-from-
// result (label-0 tris are NOT flipped by Subtract since they carry A=0).
//
// To make the cavity wall wind toward-centre AFTER one production swap, we
// author each cap triangle's analytic winding away-from-centre here; verifying
// the OUTPUT (post-swap) winds toward-centre is exactly the relationship the
// adversary must witness. We confirm BOTH the self-check (apply the swap by
// hand) and the live boolean() agree.
// =========================================================================

fn adv_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = A_LO;
    let [x1, y1, z1] = A_HI;
    // box bottom corners 0..3, top corners 4..7
    verts.push(pt(x0, y0, z0)); // 0
    verts.push(pt(x1, y0, z0)); // 1
    verts.push(pt(x1, y1, z0)); // 2
    verts.push(pt(x0, y1, z0)); // 3
    let t0 = verts.len() as u32;
    verts.push(pt(x0, y0, z1)); // 4
    verts.push(pt(x1, y0, z1)); // 5
    verts.push(pt(x1, y1, z1)); // 6
    verts.push(pt(x0, y1, z1)); // 7

    // hemisphere rings 0..MM-1 (full NN-vertex rings); pole separate.
    let mut ring_base: Vec<u32> = Vec::with_capacity(MM);
    for j in 0..MM {
        ring_base.push(verts.len() as u32);
        let rz = ring_z(j);
        let rr = ring_r(j);
        for k in 0..NN {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (NN as f64);
            verts.push(pt(C[0] + rr * th.cos(), C[1] + rr * th.sin(), rz));
        }
    }
    let pole = verts.len() as u32;
    verts.push(pt(C[0], C[1], C[2] - R));

    let rim = |k: usize| ring_base[0] + (k % NN) as u32;
    let ring = |j: usize, k: usize| ring_base[j] + (k % NN) as u32;

    // --- BOX label-0 tris: authored DIRECTLY outward-from-result (Subtract does
    // NOT flip them). Each face is two tris with the standard
    // CCW-as-seen-from-outside winding.
    let box_tri = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(0)]);
    };
    // bottom (−Z outward): CCW from below
    box_tri([0, 3, 2], &mut tris, &mut surface);
    box_tri([0, 2, 1], &mut tris, &mut surface);
    // 4 sides, CCW from outside
    let side = |a: u32,
                b: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        // outward-facing quad (a,b on bottom going CCW; c,d on top)
        tris.push([a, b, c]);
        surf.push(vec![LaInputId(0)]);
        tris.push([a, c, d]);
        surf.push(vec![LaInputId(0)]);
    };
    // front −y: bottom edge 0->1, top 5,4
    side(0, 1, t0 + 1, t0, &mut tris, &mut surface);
    // right +x: bottom 1->2, top 6,5
    side(1, 2, t0 + 2, t0 + 1, &mut tris, &mut surface);
    // back +y: bottom 2->3, top 7,6
    side(2, 3, t0 + 3, t0 + 2, &mut tris, &mut surface);
    // left −x: bottom 3->0, top 4,7
    side(3, 0, t0, t0 + 3, &mut tris, &mut surface);

    // --- BOX TOP ANNULUS (+Z outward), hole = the rim ring. Outer square CCW
    // from above (4,5,6,7); inner rim hole must wind OPPOSITE so the two cycles
    // form a proper outer+hole. The rim shared with the cap must pair: the cap
    // top band traverses the rim one way, the annulus the other.
    // Outer loop [4,5,6,7] ASCENDING (CCW seen from above, +Z outward) — pairs
    // with this file's outward box-side top edges (which run 5->4, 6->5, ...).
    // Inner rim loop ASCENDING (`li(s)=rim(s)`); the cap top band traverses the
    // rim the opposite way (after the production flip) so the shared rim edges
    // pair. Verified watertight + vol≈209 (216 box − (2/3)πR³ half-ball) by the
    // independent Python winding search and the self-check below.
    let lo = [t0, t0 + 1, t0 + 2, t0 + 3];
    let per = NN / 4; // 3 for NN=12
    let li = |s: usize| rim(s % NN);
    for cc in 0..4usize {
        let oa = lo[cc];
        let ob = lo[(cc + 1) % 4];
        let sa = cc * per;
        let sb = (cc + 1) * per;
        box_tri([oa, ob, li(sb)], &mut tris, &mut surface);
        for s in (sa..sb).rev() {
            box_tri([oa, li(s + 1), li(s)], &mut tris, &mut surface);
        }
    }

    // --- SPHERE CAP label-1: authored OUTWARD (away-from-centre sphere normal),
    // NOT pre-swapped. The production Subtract flip (single tri[1]↔tri[2] swap on
    // label-1 tris) turns each into toward-centre winding in the OUTPUT.
    let cap = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(1)]);
    };
    // bands j .. j+1 (full rings), authored AWAY-FROM-CENTRE (outward sphere
    // normal). Verified: the authored (pre-swap) winding normal dots
    // away-from-centre = +0.84 > 0 (outward); the production Subtract flip
    // (tri[1]↔tri[2]) turns each into the toward-centre cavity winding (dot
    // −0.84 < 0) in the OUTPUT. The cap top band's rim edges run opposite the
    // box-top annulus inner ring so the shared rim edges pair.
    for j in 0..(MM - 1) {
        for k in 0..NN {
            let k1 = k + 1;
            cap(
                [ring(j, k1), ring(j, k), ring(j + 1, k)],
                &mut tris,
                &mut surface,
            );
            cap(
                [ring(j, k1), ring(j + 1, k), ring(j + 1, k1)],
                &mut tris,
                &mut surface,
            );
        }
    }
    // pole fan (ring MM-1 -> south pole), authored away-from-centre.
    for k in 0..NN {
        let k1 = k + 1;
        cap(
            [ring(MM - 1, k1), ring(MM - 1, k), pole],
            &mut tris,
            &mut surface,
        );
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]);
        } else {
            inside.push(vec![true, false]);
        }
    }
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    }
}

// Apply the production Subtract keep+flip by hand: every tri kept; label-1 tris
// get tri[1]↔tri[2] swapped (flip_for_op(Subtract) on a B-only tri).
fn simulated_output(arr: &LabeledArrangement) -> Mesh {
    let mut tris = Vec::with_capacity(arr.mesh.tris.len());
    for (i, t) in arr.mesh.tris.iter().enumerate() {
        if arr.surface[i][0] == LaInputId(1) {
            tris.push([t[0], t[2], t[1]]);
        } else {
            tris.push(*t);
        }
    }
    Mesh::new(arr.mesh.verts.clone(), tris)
}

fn run() -> BRep {
    let bx = adv_box();
    let sp = adv_sphere();
    let mock = LabelMock {
        arr: adv_arrangement(),
    };
    boolean(&bx, &sp, BoolOp::Subtract, &mock)
        .expect("adversary: box − sphere dimple Subtract must be Ok")
}

fn cavity_walls(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Sphere { .. }) && f.reversed)
        .cloned()
        .collect()
}

// =========================================================================
// (1) Independent fixture self-check: the OUTWARD-authored mock, after the
// hand-applied Subtract flip, is a valid genus-0 closed shell. If this fails,
// my mock is wrong and the live-boolean witnesses below are meaningless.
// =========================================================================
#[test]
fn adv_mock_is_valid_genus0() {
    let arr = adv_arrangement();
    let sim = simulated_output(&arr);
    assert_eq!(
        unpaired(&sim),
        0,
        "adversary self-check: simulated dimple output must be watertight"
    );
    assert_eq!(
        chi(&sim),
        2,
        "adversary self-check: dimpled box must be genus 0 (χ=2)"
    );
    assert!(
        vol(&sim) > 0.0,
        "adversary self-check: simulated output must be outward-oriented (vol>0), got {}",
        vol(&sim)
    );
}

// =========================================================================
// (2) Independent live-boolean witness: watertight + χ=2 + signed_volume > 0
// through the REAL `boolean()` (exercises the wired Sphere production path).
// =========================================================================
#[test]
fn adv_live_boolean_is_watertight_genus0_positive() {
    let r = run();
    assert_eq!(
        unpaired(r.as_mesh()),
        0,
        "adversary: live dimple output must be watertight"
    );
    assert_eq!(
        chi(r.as_mesh()),
        2,
        "adversary: live dimple output must be genus 0 (χ=2)"
    );
    let v = vol(r.as_mesh());
    // box 6×6×6 = 216 minus half-ball (2/3)πR³ ≈ 7.07 ⇒ ≈ 208.9, must be > 0.
    assert!(
        v > 0.0,
        "adversary: live output must be outward-oriented (vol>0), got {v}"
    );
}

// =========================================================================
// (3) Mesh-winding ↔ `reversed` consistency from an OUTWARD-authored mock.
// Independently recompute, from the emitted output cap triangles, that the
// geometric winding normal points TOWARD the sphere centre (into the dimple),
// AND that this agrees with `reversed == true` on the Surface::Sphere face. The
// two must be mutually consistent. This is the load-bearing PART-B witness: a
// mock built with the OPPOSITE authorship convention to RED must still yield the
// same (reversed==true, toward-centre) result.
// =========================================================================
#[test]
fn adv_mesh_winding_agrees_with_reversed_flag() {
    let r = run();

    // (a) the flag side: at least one Surface::Sphere face, exact params,
    // reversed == true.
    let walls = cavity_walls(&r);
    assert!(
        !walls.is_empty(),
        "adversary: expected a surviving Surface::Sphere cavity wall with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );
    let want = adv_sphere_surface();
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "adversary: cavity-wall Sphere params must equal the input exactly \
             (no perturbation to signal sense)"
        );
        assert!(
            w.reversed,
            "adversary: cavity wall must carry reversed==true"
        );
    }
    // every Sphere face is the exact input (no re-fit anywhere).
    for f in r.faces() {
        if let Surface::Sphere { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "adversary: a Sphere face has perturbed params"
            );
        }
    }

    // (b) the mesh side: cap triangles wind toward the centre. Independent
    // geometric identification (no shared helper): all 3 verts within a generous
    // chord band of the sphere AND in the lower hemisphere (z ≤ TOPZ) with at
    // least one strictly below the rim.
    // Cap mesh VERTICES sit exactly on the sphere (Stage-1 sphere verts are
    // exact, not chord midpoints), so a small membership band is ample to pick
    // them out from the planar box tris; 1e-6 is far below the inter-feature gap.
    let band = 1e-6;
    let mesh = r.as_mesh();
    let mut checked = 0usize;
    for t in &mesh.tris {
        let v0 = mesh.verts[t[0] as usize].as_array();
        let v1 = mesh.verts[t[1] as usize].as_array();
        let v2 = mesh.verts[t[2] as usize].as_array();
        let on = [v0, v1, v2]
            .iter()
            .all(|&x| (vlen(vsub(x, C)) - R).abs() <= band);
        let below_rim = [v0, v1, v2].iter().all(|&x| x[2] <= TOPZ + 1e-9);
        let has_strict = [v0, v1, v2].iter().any(|&x| x[2] < TOPZ - 1e-9);
        if !on || !below_rim || !has_strict {
            continue;
        }
        let gnorm = vunit(vcross(vsub(v1, v0), vsub(v2, v0)));
        let centroid = vscale(vadd(vadd(v0, v1), v2), 1.0 / 3.0);
        let away = vunit(vsub(centroid, C));
        let d = vdot(gnorm, away);
        assert!(
            d < -1e-9,
            "adversary: cap mesh tri {t:?} winding normal {gnorm:?} must point \
             TOWARD the centre (dot with away-from-centre = {d} < 0) — must agree \
             with reversed==true"
        );
        checked += 1;
    }
    assert!(
        checked >= NN,
        "adversary: expected ≥{NN} cap triangles witnessed, got {checked}"
    );
}

// =========================================================================
// (4) Rim exactness from scratch: every Curve::Circle rim point satisfies
// |x − center| == radius AND lies on the box-face plane (z = TOPZ) to TAU_MODEL.
// Independent circle-frame sampling (own basis construction).
// =========================================================================
#[test]
fn adv_rim_is_exact_great_circle() {
    let r = run();
    let tau = cad_primitives::TAU_MODEL;

    let circles: Vec<(Point3, Vector3, f64)> = r
        .edges()
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Circle {
                center,
                normal,
                radius,
            } => Some((center, normal, radius)),
            _ => None,
        })
        .collect();
    assert!(
        !circles.is_empty(),
        "adversary: expected ≥1 Curve::Circle rim edge; edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let mut saw_great = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        // a great circle of the input sphere: radius == R.
        assert!(
            (radius - R).abs() <= tau,
            "adversary: rim Circle radius {radius} must equal R {R} (±TAU_MODEL)"
        );
        // sample in the circle's own plane using an independently built basis.
        let nrm = vunit(normal.as_array());
        let helper = if nrm[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let e1 = vunit(vcross(nrm, helper));
        let e2 = vunit(vcross(nrm, e1));
        for k in 0..NN {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (NN as f64);
            let p = vadd(
                c,
                vadd(
                    vscale(e1, *radius * th.cos()),
                    vscale(e2, *radius * th.sin()),
                ),
            );
            let on_sphere = (vlen(vsub(p, C)) - R).abs();
            assert!(
                on_sphere <= tau,
                "adversary: rim point {p:?} off the sphere (|x−center|−r = {on_sphere})"
            );
            let off_plane = (p[2] - TOPZ).abs();
            assert!(
                off_plane <= tau,
                "adversary: rim point {p:?} off the box-top plane z={TOPZ} (offset {off_plane})"
            );
        }
        if (c[2] - TOPZ).abs() <= tau && (radius - R).abs() <= tau {
            saw_great = true;
        }
    }
    assert!(
        saw_great,
        "adversary: expected the great-circle rim on the box-top plane (z={TOPZ}, r=R={R})"
    );
}

// =========================================================================
// (5) No Cone leaks; box faces planar + outward; determinism. Independent
// re-statement so the adversary file stands alone.
// =========================================================================
#[test]
fn adv_box_faces_outward_no_cone_and_deterministic() {
    let r1 = run();
    let r2 = run();
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "adversary: verts nondeterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "adversary: tris nondeterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "adversary: face count nondeterministic"
    );
    for (a, b) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(
            a.surface, b.surface,
            "adversary: face surface nondeterministic"
        );
        assert_eq!(
            a.reversed, b.reversed,
            "adversary: face reversed nondeterministic"
        );
    }

    // no Cone anywhere (Cone stays a loud reject; Sphere is the only curved wall).
    assert!(
        r1.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Cone { .. })),
        "adversary: output must contain no Cone faces"
    );

    // every planar box face reversed==false and outward (n·centroid + d < 0).
    let centroid = [
        0.5 * (A_LO[0] + A_HI[0]),
        0.5 * (A_LO[1] + A_HI[1]),
        0.5 * (A_LO[2] + A_HI[2]),
    ];
    let mut planar = 0usize;
    for f in r1.faces() {
        if let Surface::Plane { normal, d } = f.surface {
            assert!(
                !f.reversed,
                "adversary: planar face must be reversed==false"
            );
            let n = normal.as_array();
            let sd = vdot(n, centroid) + d;
            assert!(
                sd < -1e-9,
                "adversary: box face normal {n:?} (d={d}) points inward (n·c+d={sd} ≥ 0)"
            );
            planar += 1;
        }
    }
    assert!(
        planar >= 6,
        "adversary: expected ≥6 planar box faces, got {planar}"
    );
}

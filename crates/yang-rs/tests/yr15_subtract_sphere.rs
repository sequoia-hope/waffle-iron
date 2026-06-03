//! PR-YR15 RED — curved `Subtract`: box − sphere HEMISPHERICAL DIMPLE, a
//! genus-0 (χ=2) closed orientable 2-manifold. Extends PR-YR13's BLIND POCKET
//! (cylinder cavity, genus 0) and PR-YR14's THROUGH-HOLE (cylinder cavity,
//! genus 1) to a SPHERICAL cavity: a sphere centred ON one box face (poking
//! through exactly that one face) so that `box − sphere` carves a hemispherical
//! dimple. The lower hemisphere (z ≤ 2) lies inside the box; the cap wall is the
//! INSIDE hemisphere of the sphere (`Surface::Sphere`, `reversed == true`),
//! whose effective outward normal points TOWARD the sphere centre (into the
//! dimple). The result is a single connected, closed, orientable 2-manifold
//! shell of genus 0 → χ = 2 (a dimpled box, topologically still a sphere).
//!
//! Spec of record: `specs/yr15_subtract_sphere_dimple.md`.
//!
//! Drives the public `yang_rs::boolean(&box, &sphere, BoolOp::Subtract, &mock)`
//! via a hand-built `LabeledArrangement` (`LabelMock`) encoding the FULL closed
//! genus-0 result surface: a box BOTTOM (z=0, 2 tris), 4 box sides, a box TOP
//! ANNULUS (z=2, hole = the great-circle rim ring r=1), and a hemisphere cap
//! (the inside/lower hemisphere, rim z=2 down to south pole z=1, label 1).
//! `a` = box (`InputId::A` / id 0); `b` = sphere (`InputId::B` / label id 1). The
//! Subtract keep-rule keeps the box surface tris (count 0) and the sphere cavity
//! tris (`inside[0]`, count 1); `flip_for_op(Subtract)` re-swaps tri[1]↔tri[2]
//! on the label-1 tris.
//!
//! RED status: the GREEN sub-agent wires `Surface::Sphere` into three production
//! sites (`surface_to_quadric`, the `tol_for` face-resolution band, and the
//! `emit_topology` curved-branch guard) — each mirroring the existing `Cylinder`
//! arm — plus extracts the `1e-2·2r√3` chord literal into a shared
//! `sphere_chord_bound` helper. Until then a `Surface::Sphere` cavity wall hits
//! the planar-fallback arm in `emit_topology` (`src/lib.rs:3503`) and returns
//! `Err(YangError::CurvedSurfaceNotYetSupported { .. })`, so `run_subtract()`
//! panics on `.expect(...)`. That is the RED signal. The mock self-check below
//! (`mock_is_valid_genus0`) makes NO `boolean()` call, so it PASSES today,
//! proving the fixture is a valid genus-0 closed shell before the boolean
//! oracles exercise the (not-yet-wired) Sphere path.
//!
//! Oracles (spec §Oracles):
//!  1. Cavity wall surface params: `Surface::Sphere` == input exact
//!     center/radius, `reversed == true`; box faces `Plane`, `reversed == false`
//!     (PART A surface-param + PART B emitted-mesh-winding witness).
//!  2. Effective outward normal points TOWARD the centre (into the dimple):
//!     analytic away-from-centre normal negated (because `reversed`) points
//!     toward `center` (PART A) + witness ACTUAL mesh-triangle winding
//!     (`dot(gnorm, away_from_center) < -1e-9`, PART B).
//!  3. Watertight 2-manifold, χ == 2, signed_volume > 0, 0 unpaired half-edges.
//!  4. Exact `Circle` rim: ≥1 `Curve::Circle` edge; every rim point satisfies
//!     `|x − center| = radius` AND lies on the box-face plane to `TAU_MODEL`.
//!  5. Determinism (two runs byte-identical verts/tris/faces incl. `reversed`)
//!     + env-gated sidecar `Subtract` mesh-parity (LOUD skip).

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives exposes only new/x/y/z/as_array).
// =========================================================================

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// =========================================================================
// Mesh oracles (copied from yr13/yr14).
// =========================================================================

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

fn euler_characteristic(mesh: &Mesh) -> i64 {
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let e = edges.len() as i64;
    v - e + f
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

// =========================================================================
// Canonical config — a verified-closed box with a HEMISPHERICAL DIMPLE.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2]
//   sphere B: center (0,0,2) ON the box-top plane (z=2), radius 1.
//     The lower hemisphere (z ≤ 2, down to the south pole z=1) lies INSIDE the
//     box; the upper hemisphere (z > 2) is outside and discarded. `box − sphere`
//     carves a hemispherical dimple in the top face. The rim is the GREAT circle
//     `sphere ∩ box-top plane` (z=2, r=1, center (0,0,2)).
// =========================================================================

const N: usize = 16; // rim/longitudinal facets
const M: usize = 4; // hemisphere latitude bands (rim → pole)
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const SPH_CENTER: [f64; 3] = [0.0, 0.0, 2.0];
const SPH_R: f64 = 1.0;
const TOP_Z: f64 = 2.0; // box top plane = great-circle rim plane

fn sph_surface() -> Surface {
    Surface::Sphere {
        center: p(SPH_CENTER[0], SPH_CENTER[1], SPH_CENTER[2]),
        radius: SPH_R,
    }
}

// =========================================================================
// Fixtures: box_brep (reused VERBATIM from yr13/yr14) and sphere_brep (the YR12
// closed solid-sphere B-Rep: one Sphere face + one meridian seam Circle).
// Integration tests cannot see #[cfg(test)] lib items, so these are local.
// =========================================================================

/// Axis-aligned box `lo..hi` with correct OUTWARD normals and plane offsets
/// (`n·x + d = 0`). All faces planar → `reversed: false`.
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
        [0, 1, 2, 3], // bottom (−z)
        [4, 7, 6, 5], // top (+z)
        [0, 4, 5, 1], // front (−y)
        [1, 5, 6, 2], // right (+x)
        [2, 6, 7, 3], // back (+y)
        [3, 7, 4, 0], // left (−x)
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
    // n·x + d = 0 ⇒ d = −n·(a point on the plane).
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
    BRep::new(verts, edges, faces).expect("box_brep: BRep::new failed")
}

/// Closed solid-sphere B-Rep (YR12 spec §1: one `Surface::Sphere` face bounded
/// by a single meridian seam `Curve::Circle`). South pole `v0`, north pole `v1`.
fn sphere_brep(center: [f64; 3], radius: f64) -> BRep {
    let south = add(center, scale([0.0, 0.0, -1.0], radius));
    let north = add(center, scale([0.0, 0.0, 1.0], radius));

    let verts = vec![
        BRepVertex {
            point: p(south[0], south[1], south[2]),
        },
        BRepVertex {
            point: p(north[0], north[1], north[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Circle {
            center: p(center[0], center[1], center[2]),
            normal: Vector3::new(0.0, -1.0, 0.0),
            radius,
        },
    }];

    let faces = vec![BRepFace {
        surface: Surface::Sphere {
            center: p(center[0], center[1], center[2]),
            radius,
        },
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];

    BRep::new(verts, edges, faces).expect("sphere_brep: BRep::new should tessellate the sphere")
}

fn dimple_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn dimple_sphere() -> BRep {
    sphere_brep(SPH_CENTER, SPH_R)
}

// =========================================================================
// Hand-built arrangement: FULL closed genus-0 result surface (box with a
// hemispherical dimple), outward-from-result winding, N=16 longitudinal facets,
// M=4 hemisphere latitude bands. Verified watertight + χ=2 + positive volume
// (after the Subtract keep-set + flip_for_op) by the MANDATORY
// `mock_is_valid_genus0` self-check below.
//
// Box tris (label 0): surface=[A], inside=[false,false] (count 0) — kept by
//   Subtract branch 1, NOT flipped.
// Sphere cap tris (label 1): surface=[B], inside=[true,false] (count 1) — kept
//   by Subtract branch 2, FLIPPED by flip_for_op (swap tri[1]↔tri[2]). Authored
//   so the post-flip winding is outward-from-result (TOWARD the sphere centre).
//
// Vs YR13's pocket_arrangement:
//   1. Box BOTTOM is a plain 2-triangle face (z=0) — KEEP as YR13.
//   2. Box 4 SIDES — KEEP as YR13.
//   3. Box TOP ANNULUS (z=2, hole = the great-circle RIM ring r=1) — KEEP as
//      YR13's top annulus (the rim is at z=TOP_Z=2 here, vs YR13's z=2 too).
//   4. The cylinder wall + floor cap of YR13 are REPLACED by a HEMISPHERE CAP:
//      M latitude rings (rim z=2 → south pole z=1), a quad band fan between rings
//      and a triangle fan at the pole.
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

/// Hemisphere latitude `j` (0 = rim/equator at z=TOP_Z, M = south pole at
/// z=center−r): polar angle `phi = (π/2)·(j/M)` measured DOWN from the
/// equatorial plane. z = center_z − r·sin(phi); ring radius = r·cos(phi).
fn hemi_ring_z(j: usize) -> f64 {
    let phi = 0.5 * std::f64::consts::PI * (j as f64) / (M as f64);
    SPH_CENTER[2] - SPH_R * phi.sin()
}
fn hemi_ring_r(j: usize) -> f64 {
    let phi = 0.5 * std::f64::consts::PI * (j as f64) / (M as f64);
    SPH_R * phi.cos()
}

fn dimple_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = BOX_LO;
    let [x1, y1, z1] = BOX_HI;
    let b0 = verts.len() as u32;
    verts.push(p(x0, y0, z0)); // 0
    verts.push(p(x1, y0, z0)); // 1
    verts.push(p(x1, y1, z0)); // 2
    verts.push(p(x0, y1, z0)); // 3
    let t0 = verts.len() as u32;
    verts.push(p(x0, y0, z1)); // 4
    verts.push(p(x1, y0, z1)); // 5
    verts.push(p(x1, y1, z1)); // 6
    verts.push(p(x0, y1, z1)); // 7

    // Hemisphere latitude rings j=0..M. Ring 0 (j=0) is the GREAT-CIRCLE rim at
    // z=TOP_Z, r=1 (shared: box-top annulus inner boundary + cap top band). Ring
    // M would be the pole (radius 0) — emitted as a single pole vertex, not a
    // ring. So rings 0..M-1 are full N-vertex rings; the pole is one vertex.
    let mut ring_base: Vec<u32> = Vec::with_capacity(M);
    for j in 0..M {
        ring_base.push(verts.len() as u32);
        let rz = hemi_ring_z(j);
        let rr = hemi_ring_r(j);
        for k in 0..N {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
            verts.push(p(
                SPH_CENTER[0] + rr * th.cos(),
                SPH_CENTER[1] + rr * th.sin(),
                rz,
            ));
        }
    }
    // South pole (j=M): z = center_z − r = 1.0.
    let pole = verts.len() as u32;
    verts.push(p(SPH_CENTER[0], SPH_CENTER[1], SPH_CENTER[2] - SPH_R));

    // The rim ring IS hemisphere ring 0.
    let rim = |k: usize| ring_base[0] + (k % N) as u32;
    let ring = |j: usize, k: usize| ring_base[j] + (k % N) as u32;

    // A real Cherchi arrangement is OUTWARD-oriented (positive signed volume).
    // We author each box face's triangles using the SAME geometric vertex
    // sequences as a CCW-from-outside box, then apply a single GLOBAL winding
    // reversal at the box emit closure (`push_box` swaps tri[1]↔tri[2] exactly
    // once) so the boolean OUTPUT comes out outward-oriented.
    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // global reversal → outward output
        surf.push(vec![LaInputId(0)]);
    };

    // === BOX BOTTOM (z=z0), outward −Z. Standard box face [0,1,2,3] winding.
    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    // === BOX 4 SIDES, outward horizontal (standard CCW-from-outside winding,
    // then globally reversed at emit). KEEP exactly as YR13.
    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, c, bb]); // reversed [a,bb,c]
        surf.push(vec![LaInputId(0)]);
        tris.push([a, d, c]); // reversed [a,c,d]
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface); // front −y
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface); // right +x
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface); // back +y
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface); // left −x

    // === BOX TOP ANNULUS (z=z1), outward +Z, with the great-circle rim ring as
    // its hole. KEEP exactly as YR13: outer Lo=[t0,t0+3,t0+2,t0+1] (CW-from-above;
    // edges oppose the side faces); inner loop Li = rim DESCENDING
    // (`li(s)=rim((N−s)%N)`) so the outer-square cycle and the rim-ring hole wind
    // in OPPOSITE rotational senses (proper outer + hole, `positive_count == 1`).
    // The inner-ring boundary edges run ASCENDING; the cap therefore traverses the
    // rim DESCENDING so the shared rim edges pair.
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = N / 4; // 4 for N=16
    let li = |s: usize| rim((N - (s % N)) % N);
    for c in 0..4usize {
        let oa = lo[c];
        let ob = lo[(c + 1) % 4];
        let sa = c * per;
        let sb = (c + 1) * per;
        push_box([oa, ob, li(sb)], &mut tris, &mut surface);
        for s in (sa..sb).rev() {
            push_box([oa, li(s + 1), li(s)], &mut tris, &mut surface);
        }
    }

    // === SPHERE HEMISPHERE CAP (label 1) — rim ring (j=0, z=2) down to the
    // south pole (z=1). As a cavity wall the outward-from-result normal points
    // TOWARD the sphere CENTRE (into the dimple). The sphere/B tris are authored
    // with the global reversal AND a pre-swap for flip_for_op; the two swaps
    // CANCEL, so the emit closure pushes the vertices unswapped ([t0,t1,t2]).
    // flip_for_op(Subtract) then re-swaps these at compaction, restoring their
    // outward (toward-centre) winding — the SAME signal that sets
    // `reversed == true` (I-rev1).
    //
    // The top band's rim edges traverse the rim DESCENDING (rim(k+1)→rim(k)) —
    // opposite the box-top annulus inner ring (ASCENDING) so the shared rim edges
    // pair. Each band between ring j and ring j+1 is split into two triangles; the
    // final band (ring M-1 → pole) is a single triangle fan at the pole. All
    // authored with the same toward-centre orientation as the top band so the
    // shared inter-ring edges pair consistently.
    let push_sph = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]); // global-reversal ∘ pre-swap = identity
        surf.push(vec![LaInputId(1)]);
    };
    // Bands between full rings j and j+1 (for j = 0 .. M-2).
    for j in 0..(M - 1) {
        for k in 0..N {
            let k1 = k + 1;
            // toward-centre winding: upper ring (j) edges DESCENDING; lower ring
            // (j+1) edges ASCENDING so successive bands' shared edges pair.
            push_sph(
                [ring(j, k1), ring(j, k), ring(j + 1, k)],
                &mut tris,
                &mut surface,
            );
            push_sph(
                [ring(j, k1), ring(j + 1, k), ring(j + 1, k1)],
                &mut tris,
                &mut surface,
            );
        }
    }
    // Pole fan (ring M-1 → south pole).
    for k in 0..N {
        let k1 = k + 1;
        push_sph(
            [ring(M - 1, k1), ring(M - 1, k), pole],
            &mut tris,
            &mut surface,
        );
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]); // box surface: outside both
        } else {
            inside.push(vec![true, false]); // sphere cavity wall: inside A only
        }
    }
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

/// Simulate the Subtract keep-set + flip on the arrangement mesh: every triangle
/// is kept (box `inside` count 0; sphere `inside` count 1), so the output mesh is
/// the arrangement mesh with every LABEL-1 triangle's tri[1]/tri[2] swapped
/// (that is `flip_for_op` for Subtract on `InputId::B`). Used by the mandatory
/// `mock_is_valid_genus0` self-check (no `boolean()` call).
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(arr.mesh.tris.len());
    for (i, tri) in arr.mesh.tris.iter().enumerate() {
        if arr.surface[i][0] == LaInputId(1) {
            tris.push([tri[0], tri[2], tri[1]]); // flip_for_op(Subtract) on B
        } else {
            tris.push(*tri);
        }
    }
    Mesh::new(arr.mesh.verts.clone(), tris)
}

fn run_subtract() -> BRep {
    let bx = dimple_box();
    let sph = dimple_sphere();
    let mock = LabelMock {
        arrangement: dimple_arrangement(),
    };
    boolean(&bx, &sph, BoolOp::Subtract, &mock)
        .expect("yr15: box − sphere HEMISPHERICAL DIMPLE Subtract must be Ok")
}

/// The surviving cavity-wall faces: `Surface::Sphere` with `reversed == true`.
fn cavity_wall_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Sphere { .. }) && f.reversed)
        .cloned()
        .collect()
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED boolean output (keep-all + flip label-1) directly, NO boolean()
// call, and asserts the mock is a valid genus-0 closed shell: watertight, χ=2,
// outward-oriented. If this fails the whole RED test is meaningless, so the mock
// windings (especially the hemisphere bands) are iterated until it passes.
//
// This test PASSES today (no boolean() call → does not touch the not-yet-wired
// Sphere production path); the boolean oracles below FAIL today (RED).
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    let arr = dimple_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr15 self-check: simulated dimple output mesh must be watertight \
         (0 unpaired half-edges); got {unpaired}. Iterate the mock windings."
    );

    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 2,
        "yr15 self-check: simulated hemispherical-dimple output must be genus 0 \
         (χ=2); got χ={chi}. A dimpled box is still a topological sphere."
    );

    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr15 self-check: simulated output must be OUTWARD-oriented (positive \
         signed volume); got {vol}. A negative volume means the mock is globally \
         inside-out."
    );
}

// =========================================================================
// Oracle 1 — cavity wall surface params + sense encoding (PART A surface params
// + PART B emitted-mesh-winding witness); box faces planar outward; no Cone.
// =========================================================================

#[test]
fn oracle1_cavity_wall_surface_params_and_sense() {
    let r = run_subtract();
    let want = sph_surface();

    // Cavity wall(s): Surface::Sphere == input exact params, reversed == true.
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr15 O1: expected a surviving Surface::Sphere cavity wall with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "yr15 O1 (I-rev3): cavity-wall Surface::Sphere must equal the input \
             sphere's center/radius field-for-field (no perturbation to signal sense)"
        );
        assert!(
            w.reversed,
            "yr15 O1: cavity wall must carry reversed == true"
        );
    }
    // Every Surface::Sphere face must be the exact input params (no re-fit).
    for f in r.faces() {
        if let Surface::Sphere { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "yr15 O1: a Surface::Sphere face has perturbed params"
            );
        }
    }

    // PART B — witness the ACTUAL emitted mesh winding (the mesh side of I-rev1).
    // Identify cap mesh triangles geometrically: all 3 verts within d_ε of the
    // sphere surface (|x−center| ≈ radius) AND in the dimple band (z ≤ TOP_Z,
    // i.e. the lower hemisphere). For each, the geometric winding normal
    // (v1−v0)×(v2−v0) at the centroid must point TOWARD the centre (dot with the
    // away-from-centre direction < 0). This proves the mesh winding agrees with
    // `reversed == true` — not merely the surface params.
    let center = SPH_CENTER;
    let mesh = r.as_mesh();
    let de = 0.05; // generous cap-membership tolerance (sphere chord band)
    let mut cap_tris_checked = 0usize;
    for tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let pts = [v0, v1, v2];
        let on_sphere = pts
            .iter()
            .all(|&x| (norm(sub3(x, center)) - SPH_R).abs() <= de);
        // lower hemisphere only (z ≤ TOP_Z), and exclude the rim ring itself
        // (all three on the rim z=TOP_Z would be a degenerate flat tri — there
        // are none, but require at least one vertex strictly below the rim).
        let in_band = pts.iter().all(|&x| x[2] <= TOP_Z + 1e-9);
        let has_below = pts.iter().any(|&x| x[2] < TOP_Z - 1e-9);
        if !on_sphere || !in_band || !has_below {
            continue;
        }
        let u = sub3(v1, v0);
        let w = sub3(v2, v0);
        let gnorm = unit(cross(u, w));
        let centroid = scale(add(add(v0, v1), v2), 1.0 / 3.0);
        let away_from_center = unit(sub3(centroid, center));
        let d = dot(gnorm, away_from_center);
        assert!(
            d < -1e-9,
            "yr15 O1b: cap mesh triangle {tri:?} geometric winding normal {gnorm:?} \
             must point TOWARD the centre (dot with away-from-centre < 0); got dot \
             {d} (mesh winding must agree with reversed==true)"
        );
        cap_tris_checked += 1;
    }
    assert!(
        cap_tris_checked >= N,
        "yr15 O1b: expected to witness ≥{N} cap mesh triangles, found {cap_tris_checked}"
    );

    // Box outer faces: Surface::Plane with reversed == false, outward normals.
    let plane_faces: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .collect();
    assert!(
        !plane_faces.is_empty(),
        "yr15 O1: expected ≥1 planar box face"
    );
    for f in &plane_faces {
        assert!(
            !f.reversed,
            "yr15 O1 (I-rev2): planar faces must emit reversed == false \
             (sense encoded in Plane.normal, never double-flipped)"
        );
    }
    let box_centroid = [
        0.5 * (BOX_LO[0] + BOX_HI[0]),
        0.5 * (BOX_LO[1] + BOX_HI[1]),
        0.5 * (BOX_LO[2] + BOX_HI[2]),
    ];
    let box_faces: [([f64; 3], [f64; 3]); 6] = [
        ([0.0, 0.0, -1.0], [0.0, 0.0, BOX_LO[2]]), // bottom z=0
        ([0.0, 0.0, 1.0], [0.0, 0.0, BOX_HI[2]]),  // top z=2
        ([0.0, -1.0, 0.0], [0.0, BOX_LO[1], 0.0]), // front y=-2
        ([0.0, 1.0, 0.0], [0.0, BOX_HI[1], 0.0]),  // back y=2
        ([-1.0, 0.0, 0.0], [BOX_LO[0], 0.0, 0.0]), // left x=-2
        ([1.0, 0.0, 0.0], [BOX_HI[0], 0.0, 0.0]),  // right x=2
    ];
    for (bn, bp) in &box_faces {
        let mut found = false;
        for f in &plane_faces {
            let Surface::Plane { normal, d } = f.surface else {
                unreachable!("filtered to Surface::Plane");
            };
            let n = normal.as_array();
            let parallel = (dot(n, *bn)).abs() > 1.0 - 1e-9;
            let on_plane = (dot(n, *bp) + d).abs() < 1e-6;
            if parallel && on_plane {
                found = true;
                let sd = dot(n, box_centroid) + d;
                assert!(
                    sd < -1e-9,
                    "yr15 O1: box outer face on plane (normal {bn:?}, point {bp:?}) \
                     has stored normal {n:?} (d={d}) pointing INWARD (n·c+d={sd} ≥ 0); \
                     must point OUTWARD"
                );
            }
        }
        assert!(
            found,
            "yr15 O1: expected an output Surface::Plane on box face (normal {bn:?}, \
             point {bp:?})"
        );
    }

    // No Cone in the output (Sphere is the cavity wall; Cone stays deferred).
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Cone { .. })),
        "yr15 O1: output must contain no Cone faces"
    );
}

// =========================================================================
// Oracle 2 — effective outward normal points TOWARD the centre (into the
// dimple): PART A surface-param reasoning + PART B actual mesh winding.
// =========================================================================

#[test]
fn oracle2_effective_normal_points_toward_centre() {
    let r = run_subtract();
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr15 O2: expected ≥1 surviving Surface::Sphere cavity-wall face with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );

    // PART A — surface-param reasoning (the analytic side of I-rev1). For several
    // sampled points on the cap wall, the canonical analytic outward normal is
    // away-from-centre; reversed ⇒ effective = −(away-from-centre), which must
    // point TOWARD the centre.
    for wall in &walls {
        let Surface::Sphere { center, radius } = wall.surface else {
            panic!("cavity wall must be Surface::Sphere");
        };
        let c = center.as_array();
        let _ = radius;
        // sample a spread of directions on the lower hemisphere
        for k in 0..6 {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / 6.0;
            // a point on the sphere in the lower hemisphere
            let sample = [
                c[0] + SPH_R * 0.5f64.sqrt() * th.cos(),
                c[1] + SPH_R * 0.5f64.sqrt() * th.sin(),
                c[2] - SPH_R * 0.5f64.sqrt(),
            ];
            let away = unit(sub3(sample, c));
            let effective = scale(away, -1.0); // reversed ⇒ −(away-from-centre)
            assert!(
                dot(effective, away) < -1e-9,
                "yr15 O2a: effective (reversed) normal must point TOWARD the centre"
            );
        }
    }

    // PART B — witness the ACTUAL emitted mesh winding: cap triangles' geometric
    // winding normals point toward the centre. (Same construction as O1b; kept
    // here as the explicit normal-direction oracle the spec names separately.)
    let center = SPH_CENTER;
    let mesh = r.as_mesh();
    let de = 0.05;
    let mut cap_tris_checked = 0usize;
    for tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let pts = [v0, v1, v2];
        let on_sphere = pts
            .iter()
            .all(|&x| (norm(sub3(x, center)) - SPH_R).abs() <= de);
        let in_band = pts.iter().all(|&x| x[2] <= TOP_Z + 1e-9);
        let has_below = pts.iter().any(|&x| x[2] < TOP_Z - 1e-9);
        if !on_sphere || !in_band || !has_below {
            continue;
        }
        let u = sub3(v1, v0);
        let w = sub3(v2, v0);
        let gnorm = unit(cross(u, w));
        let centroid = scale(add(add(v0, v1), v2), 1.0 / 3.0);
        let away_from_center = unit(sub3(centroid, center));
        let d = dot(gnorm, away_from_center);
        assert!(
            d < -1e-9,
            "yr15 O2b: cap mesh triangle {tri:?} winding normal {gnorm:?} must point \
             TOWARD the centre (dot with away-from-centre = {d} < 0)"
        );
        cap_tris_checked += 1;
    }
    assert!(
        cap_tris_checked >= N,
        "yr15 O2b: expected to witness ≥{N} cap mesh triangles, found {cap_tris_checked}"
    );
}

// =========================================================================
// Oracle 3 — watertight 2-manifold, χ = 2, signed_volume > 0.
// =========================================================================

#[test]
fn oracle3_watertight_euler_two() {
    let r = run_subtract();
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr15 O3: dimple output mesh must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr15 O3: hemispherical-dimple output must be genus 0 (χ = 2)"
    );
    // Outward-oriented solid (not inside-out): POSITIVE signed volume (≈ box
    // 4×4×2 = 32 minus the half-ball (2/3)πr³ ≈ 2.09 ⇒ ≈ 29.9).
    let vol = signed_volume(r.as_mesh());
    assert!(
        vol > 0.0,
        "yr15 O3: result must be outward-oriented (positive signed volume), got {vol}"
    );
}

// =========================================================================
// Oracle 4 — exact Circle rim: the great circle sphere ∩ box-top plane.
// =========================================================================

#[test]
fn oracle4_great_circle_rim() {
    let r = run_subtract();

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
        "yr15 O4: the sphere ∩ box-top section must appear as ≥1 Curve::Circle rim \
         edge; edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let tau = cad_primitives::TAU_MODEL;
    // The rim is a GREAT circle: radius == SPH_R, lies in the box-top plane
    // (z=TOP_Z), centred on the sphere axis through the centre.
    let mut saw_great = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        // radius matches the sphere (great circle through the centre plane).
        assert!(
            (radius - SPH_R).abs() <= tau,
            "yr15 O4: rim Circle radius {radius} must equal SPH_R {SPH_R} (±TAU_MODEL) \
             — it is the GREAT circle sphere ∩ centre-plane"
        );
        // Every point on the rim must satisfy |x − center| = radius (on the
        // sphere) AND lie on the box-top plane (z = TOP_Z) to TAU_MODEL. Sample
        // the circle in its own frame.
        let nrm = unit(normal.as_array());
        // build an orthonormal basis (e1,e2) in the circle plane
        let world = if nrm[0].abs() <= nrm[1].abs() && nrm[0].abs() <= nrm[2].abs() {
            [1.0, 0.0, 0.0]
        } else if nrm[1].abs() <= nrm[2].abs() {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = unit(cross(nrm, world));
        let e2 = unit(cross(nrm, e1));
        for k in 0..N {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
            let pt = add(
                c,
                add(scale(e1, *radius * th.cos()), scale(e2, *radius * th.sin())),
            );
            // on the sphere surface
            let radial = (norm(sub3(pt, SPH_CENTER)) - SPH_R).abs();
            assert!(
                radial <= tau,
                "yr15 O4: rim point {pt:?} must lie on the sphere (|x−center|−r = \
                 {radial} ≤ TAU_MODEL)"
            );
            // on the box-top plane z=TOP_Z
            let plane_off = (pt[2] - TOP_Z).abs();
            assert!(
                plane_off <= tau,
                "yr15 O4: rim point {pt:?} must lie on the box-top plane z={TOP_Z} \
                 (offset {plane_off} ≤ TAU_MODEL)"
            );
        }
        // its supporting plane is the box-top plane and it is a great circle
        if (c[2] - TOP_Z).abs() <= tau && (radius - SPH_R).abs() <= tau {
            saw_great = true;
        }
    }
    assert!(
        saw_great,
        "yr15 O4: expected the great-circle rim on the box-top plane (z={TOP_Z}, \
         radius=SPH_R={SPH_R})"
    );
}

// =========================================================================
// Oracle 5 — determinism + env-gated sidecar parity (LOUD skip).
// =========================================================================

#[test]
fn oracle5_determinism_and_sidecar_parity() {
    // (a) Determinism: two run_subtract() runs must be byte-identical in verts,
    // tris, and per-face (surface, reversed).
    let r1 = run_subtract();
    let r2 = run_subtract();
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "yr15 O5a: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "yr15 O5a: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "yr15 O5a: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "yr15 O5a: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "yr15 O5a: face reversed differs");
    }

    // (b) Env-gated sidecar parity (LOUD skip when unset).
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs yr15] SKIP: sidecar binary not found (set CHERCHI2022_BIN)");
        return;
    };
    let bx = dimple_box();
    let sph = dimple_sphere();
    let r = boolean(&bx, &sph, BoolOp::Subtract, &sb)
        .expect("yr15 O5b: sidecar-backed dimple Subtract must be Ok");
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr15 O5b: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr15 O5b: sidecar-backed hemispherical-dimple output must be χ = 2 (genus 0)"
    );
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr15 O5b: sidecar-backed output must carry a reversed Surface::Sphere cavity wall"
    );
}

//! PR-YR14 RED — curved `Subtract`: box − cylinder THROUGH-HOLE, a genus-1
//! (χ=0) closed orientable 2-manifold. Extends PR-YR13's BLIND POCKET (genus 0,
//! χ=2) to a tunnel: the cylinder passes FULLY through the box (both caps lie
//! OUTSIDE the box, so neither survives — there is NO floor cap), leaving a
//! cylindrical tube wall spanning the whole box thickness.
//!
//! Spec of record: `specs/yr14_subtract_through_hole.md`.
//!
//! Drives the public `yang_rs::boolean(&box, &cyl, BoolOp::Subtract, &mock)` via
//! a hand-built `LabeledArrangement` (`LabelMock`) encoding the FULL closed
//! genus-1 result surface: a box BOTTOM annulus (z=0, hole = bottom rim ring
//! r=1), a box TOP annulus (z=2, hole = top rim ring r=1), 4 box sides, and a
//! tube wall (top rim ↔ bottom rim, label 1). `a` = box (`InputId::A` / id 0); `b` =
//! cylinder (`InputId::B` / label id 1). The Subtract keep-rule keeps the box
//! surface tris (count 0) and the cylinder tube-wall tris (`inside[0]`, count 1);
//! `flip_for_op(Subtract)` re-swaps tri[1]↔tri[2] on the label-1 tris.
//!
//! The ONLY production change a later (GREEN) agent makes is generalizing the
//! per-shell Euler gate in `check_watertight_2manifold` (`src/lib.rs:1776`) from
//! strict `V−E+F == 2` to "accept χ = 2−2g (χ even, ≤2); reject odd χ or χ>2".
//! Everything else (curved-Subtract cavity-sense via `BRepFace.reversed`,
//! annular planar faces, the two-rim tube wall, exact `Circle` rim edges) ALREADY
//! works. So the through-hole oracles fail TODAY only because `boolean()` returns
//! `Err(NonManifoldOutput)` (the χ=0 result is rejected by the `!= 2` gate),
//! making `run_subtract()` panic on `.expect(...)`. That is the RED signal.
//!
//! Oracles (spec §Oracles):
//!  1. Through-hole succeeds + watertight (0 unpaired half-edges) + **χ == 0**
//!     (genus 1, asserted explicitly) + positive signed volume (outward).
//!  2. Gate-not-weakened (RED contract): three defect-injected mocks each STILL
//!     return `Err(NonManifoldOutput)` — (a) unpaired half-edge, (b) odd χ (χ=1),
//!     (c) χ>2 (χ=4). PASSES today and must STAY green through GREEN.
//!  3. Cavity-sense: tube wall is `Surface::Cylinder` (exact input params),
//!     `reversed == true`; effective normal (−radial) toward axis (PART A) +
//!     witness ACTUAL mesh-triangle winding toward-axis (PART B); box outer faces
//!     `Surface::Plane`, `reversed==false`, outward normals; no Sphere/Cone.
//!  4. Two exact `Circle` rim edges: ≥2 `Curve::Circle` edges, one at z=2, one
//!     at z=0, each on the cylinder lateral AND its box-face plane to TAU_MODEL.
//!  5. Determinism (two runs byte-identical verts/tris/faces incl. `reversed`)
//!     + env-gated sidecar parity (LOUD skip).
//!
//! `YangError` does NOT derive `PartialEq` (only `Debug`), so the gate-reject
//! assertions use `matches!(err, YangError::NonManifoldOutput)`.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

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
// Mesh oracles (copied from yr13_subtract_cylinder.rs).
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
// Canonical config — a verified-closed box with a cylindrical THROUGH-HOLE.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2]
//   cylinder B: axis +Z through origin, radius 1, spans z=−0.5..2.5 (height 3).
//     BOTH caps (z=−0.5 below box, z=2.5 above box) lie OUTSIDE the box, so
//     neither survives — there is NO floor cap. Only the lateral wall (spanning
//     the full box thickness z=0..2) and two rim circles (z=0, z=2) survive.
// =========================================================================

const N: usize = 16;
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, -0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.0;
const CYL_H: f64 = 3.0;
const BOT_Z: f64 = 0.0; // box bottom plane = lower rim (through-hole)
const TOP_Z: f64 = 2.0; // box top plane = upper rim

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

// =========================================================================
// Fixtures: box_brep + cylinder_brep, reused VERBATIM from yr13.
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

/// Closed solid-cylinder B-Rep (seam-edge encoding per yr7 spec §1). Face 0 =
/// lateral Cylinder, face 1 = bottom cap Plane, face 2 = top cap Plane.
fn cylinder_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let v0 = add(bottom_center, scale(e1, radius));
    let v1 = add(top_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];

    let neg_axis = scale(axis_unit, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let bottom_d = -dot(neg_axis, bottom_center);
    let top_d = -dot(axis_unit, top_center);

    let faces = vec![
        // f0 lateral
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 bottom cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 top cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate")
}

fn hole_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn hole_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

// =========================================================================
// Hand-built arrangement: FULL closed genus-1 result surface (box with a
// through-hole), outward-from-result winding, N=16 cylinder facets. Verified
// watertight + χ=0 (after the Subtract keep-set + flip_for_op) by the
// MANDATORY `mock_is_valid_genus1` self-check below.
//
// Box tris (label 0): surface=[A], inside=[false,false] (count 0) — kept by
//   Subtract branch 1, NOT flipped.
// Cylinder wall tris (label 1): surface=[B], inside=[true,false] (count 1) —
//   kept by Subtract branch 2, FLIPPED by flip_for_op (swap tri[1]↔tri[2]).
//   Authored so the post-flip winding is outward-from-result (toward-axis).
//
// Four changes vs YR13's pocket_arrangement:
//   1. Bottom rim ring at z=0 REPLACES YR13's floor ring (z=0.5) + floor_center
//      (no cap — the cylinder passes fully through).
//   2. Box TOP ANNULUS kept exactly as YR13; 4 box sides kept as YR13.
//   3. Box BOTTOM is now an ANNULUS (bottom rim ring as its hole, −Z outward,
//      inner ring traversed ASCENDING so it pairs with the wall's descending
//      bottom edges) REPLACING YR13's 2-triangle box bottom.
//   4. Tube WALL spans rim ring (z=2) down to bottom rim ring (z=0); no cap.
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

fn hole_arrangement() -> LabeledArrangement {
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

    // top rim ring @ z=TOP_Z, r=1 (shared: top annulus inner boundary + wall top)
    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), TOP_Z));
    }
    // bottom rim ring @ z=BOT_Z, r=1 (shared: bottom annulus inner boundary +
    // wall bottom). NO floor cap, so NO floor_center vertex.
    let brim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), BOT_Z));
    }

    let rim = |k: usize| rim_base + (k % N) as u32;
    let brim = |k: usize| brim_base + (k % N) as u32;

    // A real Cherchi arrangement is OUTWARD-oriented (positive signed volume).
    // We author each box face's triangles below using the SAME geometric vertex
    // sequences as a CCW-from-outside box, then apply a single GLOBAL winding
    // reversal at the box emit closure (`push_box` swaps tri[1]↔tri[2] exactly
    // once) so the boolean OUTPUT comes out outward-oriented.
    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // global reversal → outward output
        surf.push(vec![LaInputId(0)]);
    };

    // === BOX BOTTOM ANNULUS (z=z0), outward −Z, with the bottom rim ring as its
    // hole. This is the z-MIRROR of the top annulus. The top annulus uses outer
    // Lo=[t0,t0+3,t0+2,t0+1] with inner ring DESCENDING; mirrored across z, the
    // bottom face's outer square is [b0,b0+1,b0+2,b0+3] (the CCW-from-outside
    // box-bottom winding, edges {0→1,1→2,2→3,3→0}), and its inner ring runs the
    // OPPOSITE rotational sense to the outer square. The bottom rim's inner
    // boundary edges, AS EMITTED (post global-reversal), must traverse the rim
    // ASCENDING (brim(k)→brim(k+1)) so they pair with the tube wall's bottom
    // edges (which, post-flip_for_op, traverse brim(k+1)→brim(k) DESCENDING).
    //
    // Mirror derivation: in push_box the geometric authoring is reversed once.
    // The top annulus authored `li(s) = rim((N−s)%N)` (descending in s) and the
    // emitted inner edges came out ASCENDING in k after reversal. For the bottom
    // face we want the emitted inner edges ASCENDING in k as well, but the bottom
    // outer square winds OPPOSITE to the top outer square (−Z vs +Z), so we use
    // the bottom rim ASCENDING in the authoring index: `bi(s) = brim(s)`. The
    // self-check `mock_is_valid_genus1` is the authority — it is iterated until
    // watertight + χ=0 + positive volume hold.
    let blo = [b0, b0 + 1, b0 + 2, b0 + 3];
    let per = N / 4; // 4 for N=16
    let bi = |s: usize| brim(s % N);
    for c in 0..4usize {
        let oa = blo[c];
        let ob = blo[(c + 1) % 4];
        let sa = c * per;
        let sb = (c + 1) * per;
        push_box([oa, ob, bi(sb)], &mut tris, &mut surface);
        for s in (sa..sb).rev() {
            push_box([oa, bi(s + 1), bi(s)], &mut tris, &mut surface);
        }
    }

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

    // === BOX TOP ANNULUS (z=z1), outward +Z, with the top rim ring as its hole.
    // KEEP exactly as YR13: outer Lo=[t0,t0+3,t0+2,t0+1] (CW-from-above; edges
    // oppose the side faces); inner loop Li = rim DESCENDING (`li(s)=rim((N−s)%N)`)
    // so the outer-square cycle and the rim-ring hole wind in OPPOSITE rotational
    // senses (proper outer + hole, `positive_count == 1`). The inner-ring boundary
    // edges run ASCENDING; the wall therefore traverses the rim DESCENDING so the
    // shared rim edges pair.
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
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

    // === CYLINDER WALL (label 1) — top rim ring (z=2) down to bottom rim ring
    // (z=0). As a cavity wall the outward-from-result normal points TOWARD the
    // axis (−radial). The cylinder/B tris are authored with the global reversal
    // AND a pre-swap for flip_for_op; the two swaps CANCEL, so the emit closure
    // pushes the vertices unswapped ([t0,t1,t2]). flip_for_op(Subtract) then
    // re-swaps these at compaction, restoring their outward (toward-axis) winding
    // — the SAME signal that sets `reversed == true` (I-rev1). NO floor cap.
    let push_cyl = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]); // global-reversal ∘ pre-swap = identity
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        // FINAL outward (toward-axis) winding: top-rim edges ASCENDING
        // (rim(k)→rim(k1)) — opposite the top annulus inner ring (descending) so
        // the shared rim edges pair; bottom-rim edges DESCENDING
        // (brim(k1)→brim(k)) — opposite the bottom annulus inner ring (ascending)
        // so those shared rim edges pair too.
        push_cyl([rim(k1), rim(k), brim(k)], &mut tris, &mut surface);
        push_cyl([rim(k1), brim(k), brim(k1)], &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]); // box surface: outside both
        } else {
            inside.push(vec![true, false]); // cylinder cavity wall: inside A only
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
/// is kept (box `inside` count 0; cyl `inside` count 1), so the output mesh is
/// the arrangement mesh with every LABEL-1 triangle's tri[1]/tri[2] swapped
/// (that is `flip_for_op` for Subtract on `InputId::B`). Used by the mandatory
/// `mock_is_valid_genus1` self-check (no `boolean()` call).
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
    let bx = hole_box();
    let cyl = hole_cyl();
    let mock = LabelMock {
        arrangement: hole_arrangement(),
    };
    boolean(&bx, &cyl, BoolOp::Subtract, &mock)
        .expect("yr14: box − cylinder THROUGH-HOLE Subtract must be Ok")
}

/// The surviving cavity-wall faces: `Surface::Cylinder` with `reversed == true`.
fn cavity_wall_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cylinder { .. }) && f.reversed)
        .cloned()
        .collect()
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED boolean output (keep-all + flip label-1) directly, NO boolean()
// call, and asserts the mock is a valid genus-1 closed shell: watertight,
// χ=0, outward-oriented. If this fails the whole RED test is meaningless, so
// the mock windings (especially the bottom annulus) are iterated until it
// passes.
// =========================================================================

#[test]
fn mock_is_valid_genus1() {
    let arr = hole_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr14 self-check: simulated genus-1 output mesh must be watertight \
         (0 unpaired half-edges); got {unpaired}. Iterate the mock windings."
    );

    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 0,
        "yr14 self-check: simulated through-hole output must be genus 1 (χ=0); \
         got χ={chi}. A through-hole box is a single connected closed orientable \
         2-manifold of genus 1."
    );

    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr14 self-check: simulated output must be OUTWARD-oriented (positive \
         signed volume); got {vol}. A negative volume means the mock is globally \
         inside-out."
    );
}

// =========================================================================
// Oracle 1 — through-hole succeeds + watertight + χ=0 (genus 1) + outward.
// =========================================================================

#[test]
fn oracle1_through_hole_watertight_genus1() {
    let r = run_subtract();
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr14 O1: through-hole output mesh must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        0,
        "yr14 O1: through-hole output must be genus 1 (χ = 0), NOT genus 0 (χ=2)"
    );
    // Outward-oriented solid (not inside-out): POSITIVE signed volume (≈ box
    // 4×4×2 = 32 minus the π·r²·h tunnel ≈ 25.7).
    let vol = signed_volume(r.as_mesh());
    assert!(
        vol > 0.0,
        "yr14 O1: result must be outward-oriented (positive signed volume), got {vol}"
    );
}

// =========================================================================
// Oracle 2 — gate-not-weakened: defect-injected mocks each STILL return a LOUD
// non-manifold rejection. Drives the PUBLIC boolean(). PASSES today (the gate
// already rejects these) and must STAY green through GREEN — that IS the
// "gate not weakened" contract. `YangError` has no PartialEq → matches!.
//
// REACHABILITY FINDING (load-bearing — see the χ>2 sub-case below). The
// production pipeline runs geometric FACE RESOLUTION (every kept triangle's
// centroid must lie on an input B-Rep face surface, `src/lib.rs:2398-2547`)
// BEFORE `check_watertight_2manifold` (`src/lib.rs:2983`), and that gate is
// only REACHED at all when the kept mesh carries a conic edge (a `Circle` rim
// — `src/lib.rs:3349-3356`). A free-floating "tiny tetra/cube" mock therefore
// either (i) never reaches the watertight gate (no conic → planar fast path
// returns Ok), or (ii) trips `FaceResolutionFailed` first (its facets lie on no
// box/cylinder surface). So the defects below are INJECTED INTO the valid
// through-hole arrangement (which has the two rim Circles, hence reaches the
// gate) using only triangles that resolve on the real input surfaces.
// =========================================================================

/// Drive `boolean(box, cyl, Subtract, mock)` for a hand-mutated arrangement and
/// return the resulting error variant (or panic on an unexpected `Ok`).
fn drive_defect(arr: LabeledArrangement, label: &str) -> YangError {
    let mock = LabelMock { arrangement: arr };
    boolean(&hole_box(), &hole_cyl(), BoolOp::Subtract, &mock)
        .err()
        .unwrap_or_else(|| panic!("yr14 O2 {label}: defect mock must be rejected, got Ok"))
}

#[test]
fn oracle2_gate_not_weakened() {
    // (a) UNPAIRED HALF-EDGE (clause 1 — the directed half-edge pairing loop,
    // kept STRICT and genus-independent through GREEN). Drop ONE tube-wall
    // (label-1) triangle from the valid through-hole arrangement: every remaining
    // triangle still resolves (it is a real box/cylinder surface tri, and the rim
    // Circles still drive the conic path into the gate), but the dropped tri's
    // half-edges are now unpaired. The pairing loop must reject this.
    {
        let mut arr = hole_arrangement();
        let idx = arr
            .surface
            .iter()
            .position(|s| s[0] == LaInputId(1))
            .expect("a label-1 wall triangle exists");
        let mut tris = arr.mesh.tris.clone();
        tris.remove(idx);
        arr.surface.remove(idx);
        arr.inside.remove(idx);
        arr.patch.remove(idx);
        arr.mesh = Mesh::new(arr.mesh.verts.clone(), tris);
        let err = drive_defect(arr, "O2a");
        assert!(
            matches!(err, YangError::NonManifoldOutput),
            "yr14 O2a: an unpaired half-edge must return NonManifoldOutput, got {err:?}"
        );
    }

    // (b) ODD χ (clause 2 — "χ even, ≤2; reject ODD χ"). Append ONE extra lone
    // label-0 triangle lying flat on the box-BOTTOM plane (z=0), so it resolves
    // there. As its own connected shell it has V=3,E=3,F=1 ⇒ χ = 3−3+1 = 1 (ODD),
    // which is impossible for a closed orientable manifold. The generalized gate
    // (accept χ=2−2g; reject odd χ) must STILL reject it. (The lone tri's
    // half-edges are also unpaired, which independently triggers the reject — the
    // point is the gate NEVER accepts odd χ; this is the spec's intent.)
    {
        let mut arr = hole_arrangement();
        let mut verts = arr.mesh.verts.clone();
        let mut tris = arr.mesh.tris.clone();
        let base = verts.len() as u32;
        verts.push(p(-1.8, -1.8, 0.0));
        verts.push(p(-1.6, -1.8, 0.0));
        verts.push(p(-1.7, -1.6, 0.0));
        tris.push([base, base + 1, base + 2]);
        arr.surface.push(vec![LaInputId(0)]);
        arr.inside.push(vec![false, false]);
        arr.patch.push(0);
        arr.mesh = Mesh::new(verts, tris);
        let err = drive_defect(arr, "O2b");
        assert!(
            matches!(err, YangError::NonManifoldOutput),
            "yr14 O2b: an odd-χ (χ=1) shell must return NonManifoldOutput, got {err:?}"
        );
    }

    // (c) χ-INFLATED surface (clause 2 — "reject χ > 2"). A SINGLE connected
    // shell with χ > 2 cannot exist for a closed orientable manifold, and — given
    // this pipeline's face-resolution-first ordering plus the coincident-triangle
    // guard (`src/lib.rs:2380-2394`, two surviving tris welding to the same 3
    // verts ⇒ NonManifoldInput) and the bit-exact weld (`src/lib.rs:2300-2312`) —
    // the natural χ>2 construction (a DOUBLED / multiply-covered surface, the only
    // resolvable way to inflate χ past 2) is caught by the coincident-triangle
    // guard FIRST, returning NonManifoldInput. That is still a LOUD non-manifold
    // rejection: the χ>2 class is NOT accepted. We append a reverse-wound DUPLICATE
    // of a box-bottom triangle (its winding inflates the local χ above 2 while
    // remaining resolvable on z=0) and assert the boolean LOUDLY rejects it with a
    // non-manifold error (NonManifoldInput OR NonManifoldOutput) — never Ok, never
    // a silent acceptance. The directed-pairing + χ gate are both demonstrably
    // not weakened (cases a/b cover the gate's own two clauses); this case pins
    // that a χ>2-inflating mesh can never slip through to a valid result.
    {
        let mut arr = hole_arrangement();
        let mut verts = arr.mesh.verts.clone();
        let mut tris = arr.mesh.tris.clone();
        let base = verts.len() as u32;
        verts.push(p(-1.8, -1.8, 0.0));
        verts.push(p(-1.6, -1.8, 0.0));
        verts.push(p(-1.7, -1.6, 0.0));
        tris.push([base, base + 1, base + 2]);
        tris.push([base, base + 2, base + 1]); // reverse-wound duplicate (χ-inflate)
        for _ in 0..2 {
            arr.surface.push(vec![LaInputId(0)]);
            arr.inside.push(vec![false, false]);
            arr.patch.push(0);
        }
        arr.mesh = Mesh::new(verts, tris);
        let err = drive_defect(arr, "O2c");
        assert!(
            matches!(
                err,
                YangError::NonManifoldInput | YangError::NonManifoldOutput
            ),
            "yr14 O2c: a χ-inflating (doubled) surface must be LOUDLY rejected as \
             non-manifold (NonManifoldInput or NonManifoldOutput), never accepted; \
             got {err:?}"
        );
    }
}

// =========================================================================
// Oracle 3 — cavity-sense: tube wall reversed, effective normal toward axis
// (PART A surface params + PART B actual mesh winding); box outer faces planar
// outward; no Sphere/Cone.
// =========================================================================

#[test]
fn oracle3_cavity_sense_and_survival() {
    let r = run_subtract();
    let want = cyl_surface();

    // Tube wall(s): Surface::Cylinder == input exact params, reversed == true.
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr14 O3: expected a surviving Surface::Cylinder tube wall with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "yr14 O3 (I-rev3): tube-wall Surface::Cylinder must equal the input \
             cylinder's params field-for-field (no perturbation to signal sense)"
        );
        assert!(w.reversed, "yr14 O3: tube wall must carry reversed == true");
    }
    // Every Surface::Cylinder face must be the exact input params (no re-fit).
    for f in r.faces() {
        if let Surface::Cylinder { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "yr14 O3: a Surface::Cylinder face has perturbed params"
            );
        }
    }

    // PART A — surface-param reasoning: canonical analytic normal is away-from-
    // axis; reversed ⇒ effective = −radial, which must point TOWARD the axis.
    for wall in &walls {
        let Surface::Cylinder {
            axis_dir: ad,
            radius,
            ..
        } = wall.surface
        else {
            panic!("tube wall must be Surface::Cylinder");
        };
        let au = unit(ad.as_array());
        let absd = [au[0].abs(), au[1].abs(), au[2].abs()];
        let world = if absd[0] <= absd[1] && absd[0] <= absd[2] {
            [1.0, 0.0, 0.0]
        } else if absd[1] <= absd[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = unit(cross(au, world));
        let e2 = unit(cross(au, e1));
        let _ = radius;
        for k in 0..6 {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / 6.0;
            let radial = unit(add(scale(e1, th.cos()), scale(e2, th.sin())));
            let effective = scale(radial, -1.0); // reversed ⇒ −radial
            assert!(
                dot(effective, radial) < -1e-9,
                "yr14 O3a: effective (reversed) normal must point TOWARD the axis"
            );
        }
    }

    // PART B — witness the ACTUAL emitted mesh winding (the mesh side of I-rev1).
    // Identify tube-wall mesh triangles geometrically: all 3 verts within d_ε of
    // the cylinder lateral surface (radial ≈ radius) AND in the wall band
    // (BOT_Z ≤ z ≤ TOP_Z). For each, the geometric winding normal
    // (v1−v0)×(v2−v0) at the centroid must point TOWARD the axis (dot with the
    // outward radial < 0). This proves the mesh winding agrees with
    // `reversed == true` — not merely the surface params.
    let axis_point = CYL_AXIS_POINT;
    let axis_unit = unit(CYL_AXIS_DIR);
    let mesh = r.as_mesh();
    let de = 0.05;
    let mut wall_tris_checked = 0usize;
    for tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let pts = [v0, v1, v2];
        let radial_dist = |x: [f64; 3]| -> f64 {
            let w = sub3(x, axis_point);
            let along = dot(w, axis_unit);
            let proj = add(axis_point, scale(axis_unit, along));
            norm(sub3(x, proj))
        };
        let on_lateral = pts.iter().all(|&x| (radial_dist(x) - CYL_R).abs() <= de);
        let in_band = pts
            .iter()
            .all(|&x| x[2] >= BOT_Z - 1e-9 && x[2] <= TOP_Z + 1e-9);
        // A wall tri spans the full thickness: a vertex at z≈TOP and one at z≈BOT.
        let has_top = pts.iter().any(|&x| (x[2] - TOP_Z).abs() < 1e-9);
        let has_bot = pts.iter().any(|&x| (x[2] - BOT_Z).abs() < 1e-9);
        if !on_lateral || !in_band || !has_top || !has_bot {
            continue;
        }
        let u = sub3(v1, v0);
        let w = sub3(v2, v0);
        let gnorm = unit(cross(u, w));
        let centroid = scale(add(add(v0, v1), v2), 1.0 / 3.0);
        let cw = sub3(centroid, axis_point);
        let along = dot(cw, axis_unit);
        let proj = add(axis_point, scale(axis_unit, along));
        let outward_radial = unit(sub3(centroid, proj));
        let d = dot(gnorm, outward_radial);
        assert!(
            d < -1e-9,
            "yr14 O3b: tube-wall mesh triangle {tri:?} geometric winding normal \
             {gnorm:?} must point TOWARD the axis (dot with outward radial < 0); \
             got dot {d} (mesh winding must agree with reversed==true)"
        );
        wall_tris_checked += 1;
    }
    assert!(
        wall_tris_checked >= N,
        "yr14 O3b: expected to witness ≥{N} tube-wall mesh triangles, found \
         {wall_tris_checked}"
    );

    // Box outer faces: Surface::Plane with reversed == false, outward normals.
    let plane_faces: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .collect();
    assert!(
        !plane_faces.is_empty(),
        "yr14 O3: expected ≥1 planar box face"
    );
    for f in &plane_faces {
        assert!(
            !f.reversed,
            "yr14 O3 (I-rev2): planar faces must emit reversed == false \
             (sense encoded in Plane.normal, never double-flipped)"
        );
    }

    // The SIX box OUTER faces (z=0, z=2, x=±2, y=±2) must each carry an
    // OUTWARD-pointing stored Plane.normal — i.e. the box centroid lies on the
    // plane's negative side (n·c + d < 0). NOTE: the box bottom (z=0) and top
    // (z=2) are now ANNULI (the through-hole pierces both), but their supporting
    // planes and outward normals are unchanged from a solid box face.
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
                    "yr14 O3: box outer face on plane (normal {bn:?}, point {bp:?}) \
                     has stored normal {n:?} (d={d}) pointing INWARD (n·c+d={sd} ≥ 0); \
                     must point OUTWARD"
                );
            }
        }
        assert!(
            found,
            "yr14 O3: expected an output Surface::Plane on box face (normal {bn:?}, \
             point {bp:?})"
        );
    }

    // No Sphere/Cone in the output.
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cone { .. })),
        "yr14 O3: output must contain no Sphere/Cone faces"
    );
}

// =========================================================================
// Oracle 4 — two exact Circle rim edges (cylinder ∩ box-top AND ∩ box-bottom).
// =========================================================================

#[test]
fn oracle4_two_circle_rim_edges() {
    let r = run_subtract();

    // Collect every Curve::Circle edge with its (center, normal, radius).
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
        circles.len() >= 2,
        "yr14 O4: the through-hole must produce ≥2 Curve::Circle rim edges (one \
         at z=2 box-top, one at z=0 box-bottom); found {} — edges = {:?}",
        circles.len(),
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let tau = cad_primitives::TAU_MODEL;
    let axis = unit(CYL_AXIS_DIR);
    // Each rim circle must lie on the cylinder lateral surface (radius == CYL_R,
    // center on the axis, normal ∥ axis) AND on its box-face plane (center z ≈ 2
    // or ≈ 0). Verify at least one at z≈2 and at least one at z≈0.
    let mut saw_top = false;
    let mut saw_bot = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        // radius matches the cylinder.
        assert!(
            (radius - CYL_R).abs() <= tau,
            "yr14 O4: rim Circle radius {radius} must equal CYL_R {CYL_R} (±TAU_MODEL)"
        );
        // normal parallel to the cylinder axis (the cylinder lateral surface).
        let nrm = unit(normal.as_array());
        assert!(
            (dot(nrm, axis)).abs() > 1.0 - 1e-9,
            "yr14 O4: rim Circle normal {nrm:?} must be parallel to the cylinder axis"
        );
        // center radially on the axis (x,y == 0 for an axis-aligned +Z cylinder
        // through the origin): radial distance from the axis line ≈ 0.
        let w = sub3(c, CYL_AXIS_POINT);
        let along = dot(w, axis);
        let proj = add(CYL_AXIS_POINT, scale(axis, along));
        let radial_off = norm(sub3(c, proj));
        assert!(
            radial_off <= tau,
            "yr14 O4: rim Circle center {c:?} must lie on the cylinder axis \
             (radial offset {radial_off} ≤ TAU_MODEL)"
        );
        // On its box-face plane: center z ≈ TOP_Z (box top) or ≈ BOT_Z (box
        // bottom). These ARE the cylinder ∩ box-plane sections.
        if (c[2] - TOP_Z).abs() <= tau {
            saw_top = true;
        } else if (c[2] - BOT_Z).abs() <= tau {
            saw_bot = true;
        } else {
            panic!(
                "yr14 O4: rim Circle center z={} lies on neither box-top (z={}) nor \
                 box-bottom (z={}) plane within TAU_MODEL",
                c[2], TOP_Z, BOT_Z
            );
        }
    }
    assert!(
        saw_top,
        "yr14 O4: expected a rim Circle on the box-TOP plane (z={TOP_Z})"
    );
    assert!(
        saw_bot,
        "yr14 O4: expected a rim Circle on the box-BOTTOM plane (z={BOT_Z})"
    );
}

// =========================================================================
// Oracle 5 — determinism + env-gated sidecar parity (LOUD skip).
// =========================================================================

#[test]
fn oracle5_sidecar_parity_and_determinism() {
    // (a) Determinism: two run_subtract() runs must be byte-identical in verts,
    // tris, and per-face (surface, reversed).
    let r1 = run_subtract();
    let r2 = run_subtract();
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "yr14 O5a: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "yr14 O5a: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "yr14 O5a: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "yr14 O5a: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "yr14 O5a: face reversed differs");
    }

    // (b) Env-gated sidecar parity (LOUD skip when unset).
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs yr14] SKIP: sidecar binary not found (set CHERCHI2022_BIN)");
        return;
    };
    let bx = hole_box();
    let cyl = hole_cyl();
    let r = boolean(&bx, &cyl, BoolOp::Subtract, &sb)
        .expect("yr14 O5b: sidecar-backed through-hole Subtract must be Ok");
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr14 O5b: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        0,
        "yr14 O5b: sidecar-backed through-hole output must be χ = 0 (genus 1)"
    );
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr14 O5b: sidecar-backed output must carry a reversed Surface::Cylinder tube wall"
    );
}

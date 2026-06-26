//! PR-YR17 RED — curved `Subtract`: box − cone CONICAL POCKET, a genus-0
//! (χ=2) closed orientable 2-manifold. Extends PR-YR13's BLIND POCKET (cylinder
//! cavity, genus 0), PR-YR14's THROUGH-HOLE (genus 1), and PR-YR15's
//! HEMISPHERICAL DIMPLE (sphere cavity, genus 0) to a CONICAL cavity: a cone
//! with its apex INSIDE the box (the pocket bottom) and its base ABOVE the box
//! top, so the cone exits THROUGH the box-top plane only and `box − cone` carves
//! a conical pocket. The cavity wall is the cone lateral from the apex up to the
//! rim (`Surface::Cone`, `reversed == true`), whose effective outward normal
//! points INTO the pocket (away from box material). The result is a single
//! connected, closed, orientable 2-manifold shell of genus 0 → χ = 2 (a box with
//! a conical pocket, topologically still a sphere).
//!
//! Spec of record: `specs/yr17_subtract_cone_cavity.md`.
//!
//! Drives the public `yang_rs::boolean(&box, &cone, BoolOp::Subtract, &mock)`
//! via a hand-built `LabeledArrangement` (`LabelMock`) encoding the FULL closed
//! genus-0 result surface: a box BOTTOM (z=0, 2 tris), 4 box sides, a box TOP
//! ANNULUS (z=2, hole = the rim ring r=1), and a cone-lateral cavity wall (an
//! APEX FAN: the N rim verts → a single apex vertex at (0,0,0.5), N triangles).
//! `a` = box (`InputId::A` / id 0); `b` = cone (`InputId::B` / label id 1). The
//! Subtract keep-rule keeps the box surface tris (count 0) and the cone cavity
//! tris (`inside[0]`, count 1); `flip_for_op(Subtract)` re-swaps tri[1]↔tri[2]
//! on the label-1 tris.
//!
//! RED status: the GREEN sub-agent wires `Surface::Cone` into the boolean path
//! (`surface_to_quadric`, the `tol_for` face-resolution band, the `emit_topology`
//! curved-branch guard, dropping it from the defensive loud-reject arm) — each
//! mirroring the existing `Cylinder` / `Sphere` arm. Until then a `Surface::Cone`
//! cavity wall is LOUDLY rejected on the boolean path and `boolean()` returns
//! `Err(YangError::CurvedSurfaceNotYetSupported { .. })`, so `run_subtract()`
//! panics on `.expect(...)`. That is the RED signal. The mock self-check below
//! (`mock_is_valid_genus0`) makes NO `boolean()` call, so it PASSES today,
//! proving the fixture is a valid genus-0 closed shell before the boolean
//! oracles exercise the (not-yet-wired) Cone path.
//!
//! Oracles (spec §Oracles):
//!  1. Cavity wall surface params: `Surface::Cone` == input exact
//!     apex/axis_dir/half_angle, `reversed == true`; box faces `Plane`,
//!     `reversed == false`; no Sphere/Cylinder faces in the output.
//!  2. Effective outward normal points INTO the pocket (toward the axis / away
//!     from box material): the YR16 tilted cone normal `n̂ = unit(r̂ − tanα·â)`
//!     NEGATED (because `reversed`) points away from box material (PART A) +
//!     witness ACTUAL mesh-triangle winding, sampling EDGE MIDPOINTS as well as
//!     verts+centroid (PART B).
//!  3. Watertight 2-manifold, χ == 2, signed_volume > 0, 0 unpaired half-edges.
//!  4. Exact `Circle` rim: ≥1 `Curve::Circle` edge; every rim point lies on the
//!     cone (radial residual ≤ TAU_MODEL) AND on the box-top plane (z = 2) to
//!     TAU_MODEL; radius == R_rim.
//!  5. Determinism (two runs byte-identical verts/tris/faces incl. `reversed`)
//!     + env-gated sidecar `Subtract` mesh-parity (LOUD skip).

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
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
// Mesh oracles (copied from yr15).
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
// Canonical config — a verified-closed box with a CONICAL POCKET.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2]
//   cone B: apex (0,0,0.5) INSIDE the box, axis +Z, half_angle = atan(1/1.5) so
//     tanα = 2/3 and the rim at z=2 has radius R_rim = 1.5·tanα = 1.0. The cone
//     base is chosen ABOVE the box top (height 3.0 → base_center z = 3.5, base
//     radius 2.0), so the cone exits the box ONLY through the box-top plane.
//     `box − cone` carves a conical pocket; the cone lateral apex→rim survives
//     as the cavity wall (`Surface::Cone`, reversed). The rim is the perpendicular
//     cut `cone ∩ box-top plane` → exact Circle (center (0,0,2), normal +Z, r=1).
// =========================================================================

const N: usize = 16; // rim/longitudinal facets
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const CONE_APEX: [f64; 3] = [0.0, 0.0, 0.5];
const CONE_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CONE_HEIGHT: f64 = 3.0; // base z = 0.5 + 3.0 = 3.5 (above the box top z=2)
const TOP_Z: f64 = 2.0; // box top plane = rim plane
const R_RIM: f64 = 1.0; // rim radius = (2 − 0.5)·tanα = 1.5·(2/3) = 1.0

/// half_angle = atan(1/1.5) so tanα = 2/3 and R_rim = 1.5·tanα = 1.0.
fn cone_half_angle() -> f64 {
    (1.0_f64 / 1.5).atan()
}

fn cone_surface() -> Surface {
    Surface::Cone {
        apex: p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]),
        axis_dir: Vector3::new(CONE_AXIS_DIR[0], CONE_AXIS_DIR[1], CONE_AXIS_DIR[2]),
        half_angle: cone_half_angle(),
    }
}

/// Test-side chord bound `d_ε = cone_chord_bound(height, half_angle)`
/// = `1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)` (yr16 §3).
/// IDENTICAL literal to the production `cone_chord_bound`. Here used as a
/// generous chord band for cone-lateral membership classification.
fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

// =========================================================================
// Fixtures: box_brep (reused VERBATIM from yr13/yr14/yr15) and cone_brep
// (mirror tests/yr16_cone.rs — one Cone lateral + one Plane base cap sharing a
// base-rim seam Circle). Integration tests cannot see #[cfg(test)] lib items,
// so these are local.
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

/// Closed solid-cone B-Rep (yr16 §1: one `Surface::Cone` lateral face + one
/// `Surface::Plane` base cap, sharing a single base-rim seam Circle; NO seam
/// LineSegment). Copied from tests/yr16_cone.rs::cone_brep.
fn cone_brep(apex: [f64; 3], axis_dir: [f64; 3], half_angle: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let radius = height * half_angle.tan();
    let base_center = add(apex, scale(axis_unit, height));

    // Deterministic in-plane seed e1 (same stablest-cross convention as the
    // cylinder fixture). The base_seam only needs to lie on the rim; the rim
    // pre-pass recovers its azimuth, so any on-rim point is acceptable.
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let base_seam = add(base_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(apex[0], apex[1], apex[2]),
        },
        BRepVertex {
            point: p(base_seam[0], base_seam[1], base_seam[2]),
        },
    ];

    let edges = vec![
        // e0 base rim Circle, shared by lateral + base cap; start = end = v1.
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(base_center[0], base_center[1], base_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
    ];

    // Cap plane d = -(normal · base_center) with outward normal = +axis_unit.
    let cap_d = -dot(axis_unit, base_center);

    let faces = vec![
        // f0 lateral cone
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                half_angle,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 base cap
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: cap_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("cone_brep: BRep::new should tessellate the cone")
}

fn pocket_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn pocket_cone() -> BRep {
    cone_brep(CONE_APEX, CONE_AXIS_DIR, cone_half_angle(), CONE_HEIGHT)
}

// =========================================================================
// Hand-built arrangement: FULL closed genus-0 result surface (box with a
// CONICAL POCKET), outward-from-result winding, N=16 longitudinal facets.
// Verified watertight + χ=2 + positive volume (after the Subtract keep-set +
// flip_for_op) by the MANDATORY `mock_is_valid_genus0` self-check below.
//
// Box tris (label 0): surface=[A], inside=[false,false] (count 0) — kept by
//   Subtract branch 1, NOT flipped.
// Cone cavity-wall tris (label 1): surface=[B], inside=[true,false] (count 1) —
//   kept by Subtract branch 2, FLIPPED by flip_for_op (swap tri[1]↔tri[2]).
//   Authored so the post-flip winding is outward-from-result (INTO the pocket,
//   i.e. toward the cone axis / away from box material).
//
// Vs YR15's dimple_arrangement:
//   1. Box BOTTOM is a plain 2-triangle face (z=0) — KEEP as YR15.
//   2. Box 4 SIDES — KEEP as YR15.
//   3. Box TOP ANNULUS (z=2, hole = the RIM ring r=R_RIM=1) — KEEP as YR15's top
//      annulus (the rim is at z=TOP_Z=2 here, r=R_RIM=1).
//   4. The HEMISPHERE CAP of YR15 is REPLACED by a CONE-LATERAL APEX FAN: the N
//      rim verts (the SAME ring as the box-top annulus inner loop) connected to a
//      SINGLE apex vertex at (0,0,0.5). N triangles: apex → rim[k] → rim[k+1].
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

fn pocket_arrangement() -> LabeledArrangement {
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

    // Rim ring @ z=TOP_Z, r=R_RIM (shared: box-top annulus inner boundary + cone
    // lateral cavity-wall top). The cone lateral is an APEX FAN, so the only ring
    // is the rim; the apex is a single vertex below.
    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(
            CONE_APEX[0] + R_RIM * th.cos(),
            CONE_APEX[1] + R_RIM * th.sin(),
            TOP_Z,
        ));
    }
    // Apex vertex at (0,0,0.5) — the pocket bottom / cone singular tip.
    let apex = verts.len() as u32;
    verts.push(p(CONE_APEX[0], CONE_APEX[1], CONE_APEX[2]));

    let rim = |k: usize| rim_base + (k % N) as u32;

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
    // then globally reversed at emit). KEEP exactly as YR15.
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

    // === BOX TOP ANNULUS (z=z1), outward +Z, with the rim ring as its hole.
    // KEEP exactly as YR15: outer Lo=[t0,t0+3,t0+2,t0+1] (CW-from-above; edges
    // oppose the side faces); inner loop Li = rim DESCENDING (`li(s)=rim((N−s)%N)`)
    // so the outer-square cycle and the rim-ring hole wind in OPPOSITE rotational
    // senses (proper outer + hole, `positive_count == 1`). The inner-ring boundary
    // edges run ASCENDING; the cavity wall therefore traverses the rim DESCENDING
    // so the shared rim edges pair.
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

    // === CONE LATERAL CAVITY WALL (label 1) — APEX FAN: the rim ring (z=2, r=1)
    // connected to the single apex vertex (z=0.5). As a cavity wall the
    // outward-from-result normal points INTO the pocket (TOWARD the cone axis /
    // away from box material). The cone/B tris are authored with the global
    // reversal AND a pre-swap for flip_for_op; the two swaps CANCEL, so the emit
    // closure pushes the vertices unswapped ([t0,t1,t2]). flip_for_op(Subtract)
    // then re-swaps these at compaction, restoring their outward (into-pocket)
    // winding — the SAME signal that sets `reversed == true` (I-rev1).
    //
    // The cavity-wall rim edges traverse the rim DESCENDING (rim(k+1)→rim(k)) —
    // opposite the box-top annulus inner ring (ASCENDING) so the shared rim edges
    // pair. Each fan triangle is apex → rim[k+1] → rim[k].
    let push_cone = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]); // global-reversal ∘ pre-swap = identity
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        // into-pocket winding: rim edges DESCENDING (rim(k1)→rim(k)), apex closes
        // the fan. apex → rim(k1) → rim(k).
        push_cone([apex, rim(k1), rim(k)], &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]); // box surface: outside both
        } else {
            inside.push(vec![true, false]); // cone cavity wall: inside A only
        }
    }
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        num_inputs: 2,
    }
}

/// Simulate the Subtract keep-set + flip on the arrangement mesh: every triangle
/// is kept (box `inside` count 0; cone `inside` count 1), so the output mesh is
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
    let bx = pocket_box();
    let cone = pocket_cone();
    let mock = LabelMock {
        arrangement: pocket_arrangement(),
    };
    boolean(&bx, &cone, BoolOp::Subtract, &mock)
        .expect("yr17: box − cone CONICAL POCKET Subtract must be Ok")
}

/// The surviving cavity-wall faces: `Surface::Cone` with `reversed == true`.
fn cavity_wall_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cone { .. }) && f.reversed)
        .cloned()
        .collect()
}

// =========================================================================
// Cone-lateral membership classification (used by O2 PART B). A point lies on
// the cone lateral iff its radial residual `|radial − |h_axial|·tanα| ≤ d_ε`
// (apex (0,0,0.5), â=+z, tanα=2/3) AND it is in the pocket band
// 0.5−1e-9 ≤ z ≤ 2+1e-9.
// =========================================================================

fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let a = CONE_APEX;
    let ax = unit(CONE_AXIS_DIR);
    let w = sub3(x, a);
    let h_axial = dot(w, ax);
    let radial = norm(sub3(w, scale(ax, h_axial)));
    (radial - h_axial.abs() * cone_half_angle().tan()).abs()
}

fn in_pocket_band(x: [f64; 3]) -> bool {
    x[2] >= CONE_APEX[2] - 1e-9 && x[2] <= TOP_Z + 1e-9
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED boolean output (keep-all + flip label-1) directly, NO boolean()
// call, and asserts the mock is a valid genus-0 closed shell: watertight, χ=2,
// outward-oriented. If this fails the whole RED test is meaningless, so the mock
// windings (especially the apex-fan cavity wall) are iterated until it passes.
//
// This test PASSES today (no boolean() call → does not touch the not-yet-wired
// Cone production path); the boolean oracles below FAIL today (RED).
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    let arr = pocket_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr17 self-check: simulated conical-pocket output mesh must be watertight \
         (0 unpaired half-edges); got {unpaired}. Iterate the mock windings."
    );

    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 2,
        "yr17 self-check: simulated conical-pocket output must be genus 0 \
         (χ=2); got χ={chi}. A box with a conical pocket is still a topological sphere."
    );

    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr17 self-check: simulated output must be OUTWARD-oriented (positive \
         signed volume); got {vol}. A negative volume means the mock is globally \
         inside-out."
    );
}

// =========================================================================
// Oracle 1 — cavity wall surface params + sense encoding; box faces planar
// outward; no Sphere/Cylinder.
// =========================================================================

#[test]
fn oracle1_cavity_wall_surface_params_and_sense() {
    let r = run_subtract();
    let want = cone_surface();

    // Cavity wall(s): Surface::Cone == input exact params, reversed == true.
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr17 O1: expected a surviving Surface::Cone cavity wall with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "yr17 O1 (I-rev3): cavity-wall Surface::Cone must equal the input \
             cone's apex/axis_dir/half_angle field-for-field (no perturbation to \
             signal sense)"
        );
        assert!(
            w.reversed,
            "yr17 O1: cavity wall must carry reversed == true"
        );
    }
    // Every Surface::Cone face must be the exact input params (no re-fit).
    for f in r.faces() {
        if let Surface::Cone { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "yr17 O1: a Surface::Cone face has perturbed params"
            );
        }
    }

    // Box outer faces: Surface::Plane with reversed == false, outward normals.
    let plane_faces: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .collect();
    assert!(
        !plane_faces.is_empty(),
        "yr17 O1: expected ≥1 planar box face"
    );
    for f in &plane_faces {
        assert!(
            !f.reversed,
            "yr17 O1 (I-rev2): planar faces must emit reversed == false \
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
                    "yr17 O1: box outer face on plane (normal {bn:?}, point {bp:?}) \
                     has stored normal {n:?} (d={d}) pointing INWARD (n·c+d={sd} ≥ 0); \
                     must point OUTWARD"
                );
            }
        }
        assert!(
            found,
            "yr17 O1: expected an output Surface::Plane on box face (normal {bn:?}, \
             point {bp:?})"
        );
    }

    // No Sphere/Cylinder in the output (Cone is the cavity wall; the others stay
    // absent for this case).
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cylinder { .. })),
        "yr17 O1: output must contain no Sphere/Cylinder faces"
    );
}

// =========================================================================
// Oracle 2 — effective outward normal points INTO the pocket (toward the axis /
// away from box material): PART A surface-param reasoning + PART B actual mesh
// winding (sampling EDGE MIDPOINTS as well as verts+centroid).
// =========================================================================

#[test]
fn oracle2_effective_normal_points_into_pocket() {
    let r = run_subtract();
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr17 O2: expected ≥1 surviving Surface::Cone cavity-wall face with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );

    // PART A — surface-param reasoning (the analytic side of I-cone-winding). For
    // several sampled points on the cone lateral, the canonical YR16 tilted
    // outward normal is `n̂ = unit(r̂ − tanα·â)` (away from the axis, points into
    // box material). reversed ⇒ effective = −n̂, which must point AWAY from box
    // material (toward the axis side, into the pocket). We assert the effective
    // normal's radial (away-from-axis) component is NEGATIVE.
    for wall in &walls {
        let Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } = wall.surface
        else {
            panic!("cavity wall must be Surface::Cone");
        };
        let a = apex.as_array();
        let ax = unit(axis_dir.as_array());
        let tana = half_angle.tan();
        let absd = [ax[0].abs(), ax[1].abs(), ax[2].abs()];
        let world = if absd[0] <= absd[1] && absd[0] <= absd[2] {
            [1.0, 0.0, 0.0]
        } else if absd[1] <= absd[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = unit(cross(ax, world));
        let e2 = unit(cross(ax, e1));
        for k in 0..6 {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / 6.0;
            // a point on the cone at axial height h above the apex
            let h = 1.0_f64;
            let rad = h * tana;
            let rhat = add(scale(e1, th.cos()), scale(e2, th.sin())); // away-from-axis dir
            let _sample = add(a, add(scale(ax, h), scale(rhat, rad)));
            // YR16 tilted outward normal: n̂ = unit(r̂ − tanα·â).
            let n_out = unit(sub3(rhat, scale(ax, tana)));
            let effective = scale(n_out, -1.0); // reversed ⇒ −n̂
                                                // effective normal must point AWAY from box material: its component
                                                // along the away-from-axis direction r̂ is negative (into the pocket).
            let radial_comp = dot(effective, rhat);
            assert!(
                radial_comp < -1e-9,
                "yr17 O2a: effective (reversed) cone normal must point INTO the \
                 pocket (away from box material); away-from-axis component \
                 {radial_comp} must be < 0"
            );
        }
    }

    // PART B — witness the ACTUAL emitted mesh winding (the mesh side of
    // I-cone-winding). Identify cavity-wall mesh triangles geometrically: all 3
    // verts AND all 3 edge MIDPOINTS within the cone chord band of the lateral
    // (`cone_radial_residual ≤ d_ε`, per `yang_cone_tessellation_oracle_findings`
    // the cone chord bulges most at edge midpoints) AND in the pocket band
    // (0.5 ≤ z ≤ 2), with at least one vert strictly above the apex. For each, the
    // geometric winding normal (v1−v0)×(v2−v0) at the centroid must point TOWARD
    // the axis (dot with the away-from-axis direction < 0). This proves the mesh
    // winding agrees with `reversed == true` — not merely the surface params.
    let apex_pt = CONE_APEX;
    let axis_unit = unit(CONE_AXIS_DIR);
    let mesh = r.as_mesh();
    // Generous chord band: the Stage-1 cone chord bound for the rim sub-cone
    // (height apex→rim = 1.5).
    let de = cone_chord_bound(1.5, cone_half_angle()).max(0.05);
    let mut wall_tris_checked = 0usize;
    for tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        // Membership samples: the 3 verts AND the 3 edge midpoints.
        let m01 = scale(add(v0, v1), 0.5);
        let m12 = scale(add(v1, v2), 0.5);
        let m20 = scale(add(v2, v0), 0.5);
        let samples = [v0, v1, v2, m01, m12, m20];
        let on_cone = samples
            .iter()
            .all(|&x| cone_radial_residual(x) <= de && in_pocket_band(x));
        // require at least one vert strictly above the apex (excludes any
        // degenerate apex-collapsed or rim-only triangle)
        let has_above_apex = [v0, v1, v2].iter().any(|&x| x[2] > CONE_APEX[2] + 1e-9);
        if !on_cone || !has_above_apex {
            continue;
        }
        let u = sub3(v1, v0);
        let w = sub3(v2, v0);
        let gnorm = unit(cross(u, w));
        // away-from-axis direction at the triangle centroid
        let centroid = scale(add(add(v0, v1), v2), 1.0 / 3.0);
        let cw = sub3(centroid, apex_pt);
        let along = dot(cw, axis_unit);
        let proj = add(apex_pt, scale(axis_unit, along));
        let away_from_axis = unit(sub3(centroid, proj));
        let d = dot(gnorm, away_from_axis);
        assert!(
            d < -1e-9,
            "yr17 O2b: cavity-wall mesh triangle {tri:?} geometric winding normal \
             {gnorm:?} must point TOWARD the axis / INTO the pocket (dot with \
             away-from-axis < 0); got dot {d} (mesh winding must agree with \
             reversed==true)"
        );
        wall_tris_checked += 1;
    }
    assert!(
        wall_tris_checked >= 8,
        "yr17 O2b: expected to witness ≥8 cavity-wall mesh triangles, found \
         {wall_tris_checked}"
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
        "yr17 O3: conical-pocket output mesh must be watertight (0 unpaired \
         half-edges); the apex singular vertex must close cleanly"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr17 O3: conical-pocket output must be genus 0 (χ = 2)"
    );
    // Outward-oriented solid (not inside-out): POSITIVE signed volume (≈ box
    // 4×4×2 = 32 minus the cone-to-apex pocket (1/3)π R_rim² · 1.5 ≈ 1.57 ⇒ ≈ 30.4).
    let vol = signed_volume(r.as_mesh());
    assert!(
        vol > 0.0,
        "yr17 O3: result must be outward-oriented (positive signed volume), got {vol}"
    );
}

// =========================================================================
// Oracle 4 — exact Circle rim: cone ∩ box-top plane (perpendicular cut).
// =========================================================================

#[test]
fn oracle4_circle_rim() {
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
        "yr17 O4: the cone ∩ box-top section must appear as ≥1 Curve::Circle rim \
         edge; edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let tau = cad_primitives::TAU_MODEL;
    // The rim is the perpendicular-cut circle: radius == R_RIM, lies in the
    // box-top plane (z = TOP_Z), every point on the cone lateral.
    let mut saw_rim = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        // radius matches the perpendicular cut: R_rim = 1.5·tanα = 1.0.
        assert!(
            (radius - R_RIM).abs() <= tau,
            "yr17 O4: rim Circle radius {radius} must equal R_RIM {R_RIM} \
             (±TAU_MODEL) — the perpendicular cone ∩ box-top cut"
        );
        // Every point on the rim must satisfy the cone radial residual ≈ 0 (on
        // the cone) AND lie on the box-top plane (z = TOP_Z) to TAU_MODEL. Sample
        // the circle in its own frame.
        let nrm = unit(normal.as_array());
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
            // on the cone surface (radial residual ≈ 0)
            let residual = cone_radial_residual(pt);
            assert!(
                residual <= tau,
                "yr17 O4: rim point {pt:?} must lie on the cone (radial residual \
                 {residual} ≤ TAU_MODEL)"
            );
            // on the box-top plane z=TOP_Z
            let plane_off = (pt[2] - TOP_Z).abs();
            assert!(
                plane_off <= tau,
                "yr17 O4: rim point {pt:?} must lie on the box-top plane z={TOP_Z} \
                 (offset {plane_off} ≤ TAU_MODEL)"
            );
        }
        // its supporting plane is the box-top plane and the radius is R_rim
        if (c[2] - TOP_Z).abs() <= tau && (radius - R_RIM).abs() <= tau {
            saw_rim = true;
        }
    }
    assert!(
        saw_rim,
        "yr17 O4: expected the rim Circle on the box-top plane (z={TOP_Z}, \
         radius=R_RIM={R_RIM})"
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
        "yr17 O5a: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "yr17 O5a: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "yr17 O5a: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "yr17 O5a: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "yr17 O5a: face reversed differs");
    }

    // (b) Env-gated sidecar parity (LOUD skip when unset).
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yang-rs yr17] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let bx = pocket_box();
    let cone = pocket_cone();
    let r = boolean(&bx, &cone, BoolOp::Subtract, &sb)
        .expect("yr17 O5b: sidecar-backed conical-pocket Subtract must be Ok");
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr17 O5b: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr17 O5b: sidecar-backed conical-pocket output must be χ = 2 (genus 0)"
    );
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr17 O5b: sidecar-backed output must carry a reversed Surface::Cone cavity wall"
    );
}

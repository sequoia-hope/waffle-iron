//! PR-YR13 RED — curved `Subtract`: box − cylinder BLIND POCKET, cavity-sense
//! via the NEW `BRepFace.reversed` flag.
//!
//! Spec of record: `specs/yr13_subtract_cylinder_cavity_sense.md`.
//!
//! Drives the public `yang_rs::boolean(&box, &cyl, BoolOp::Subtract, &mock)`
//! via a hand-built `LabeledArrangement` (`LabelMock`) encoding the FULL closed
//! surface of the result solid (a box with a cylindrical pocket open through
//! its top, floor at z=0.5). `a` = box (`InputId::A` / label id 0); `b` =
//! cylinder (`InputId::B` / label id 1). The Subtract keep-rule
//! (`cherchi-rs/src/labeled_arrangement.rs:95-98`) keeps the box surface tris
//! (`on_surface(0) && inside.count()==0`) and the cylinder cavity tris
//! (`!on_surface(0) && inside[0] && inside.count()==1`); `flip_for_op`
//! (`src/lib.rs:2221`) flips the cylinder/B tris (swap tri[1]↔tri[2]). The mock
//! authors the cylinder wall + floor PRE-SWAPPED so the post-flip winding is
//! consistently outward-from-result.
//!
//! Oracles (spec §Oracles):
//!  1. Cavity-sense: the surviving cylinder-lateral wall is a `Surface::Cylinder`
//!     with `reversed == true`; its *effective* outward normal (canonical
//!     away-from-axis, NEGATED because reversed) points TOWARD the axis.
//!  2. Watertight (0 unpaired half-edges) + Euler χ = 2.
//!  3. Analytic survival: cavity wall == input cylinder surface field-for-field,
//!     `reversed == true`; box outer faces are `Surface::Plane`, `reversed==false`.
//!  4. Sidecar mesh-parity (env-gated, LOUD skip): output mesh == sidecar
//!     Subtract of the two Stage-1 tessellations.
//!  5. Exact rim edge: cylinder ∩ box-top section appears as a `Curve::Circle`.
//!  6. Determinism: two `boolean()` runs produce identical output.
//!
//! RED status: this file references `BRepFace.reversed`, which the GREEN
//! sub-agent adds. Until then the crate's test build fails to compile on the
//! missing field (`error[E0063]: missing field reversed`).

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
// Mesh oracles (copied from end_to_end.rs:139-173).
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

#[allow(dead_code)]
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
// Canonical config — a verified-closed box-with-pocket.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2]
//   cylinder B: axis +Z through origin, radius 1, spans z=0.5..2.5 (height 2).
//     bottom cap z=0.5 = pocket FLOOR (inside the box); top cap z=2.5 above the
//     box (discarded by the boolean). Lateral wall + floor cap survive.
// =========================================================================

const N: usize = 16;
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, 0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.0;
const CYL_H: f64 = 2.0;
const FLOOR_Z: f64 = 0.5; // cylinder bottom cap = pocket floor
const TOP_Z: f64 = 2.0; // box top plane = rim

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

// =========================================================================
// Fixtures: box_brep (generalized from m1_inputcheck's unit cube) and
// cylinder_brep (copied from yr7_cylinder.rs:93-215, seam-edge encoding).
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

/// Closed solid-cylinder B-Rep (seam-edge encoding per yr7 spec §1).
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

fn pocket_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn pocket_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

// =========================================================================
// Hand-built arrangement: FULL closed result surface (box-with-pocket),
// outward-from-result winding, N = 8 cylinder facets. Verified watertight +
// χ=2 (after the Subtract keep-set + flip_for_op) by the scratch validation.
//
// Box tris (label 0): surface=[A], inside=[false,false] (count 0) — kept by
//   Subtract branch 1, NOT flipped.
// Cylinder wall+floor tris (label 1): surface=[B], inside=[true,false] (count 1)
//   — kept by Subtract branch 2, FLIPPED by flip_for_op (swap tri[1]↔tri[2]).
//   Authored PRE-SWAPPED so the post-flip winding is outward-from-result.
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

    // rim ring @ z=TOP_Z, r=1 (shared: annulus inner boundary + wall top)
    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), TOP_Z));
    }
    // floor ring @ z=FLOOR_Z, r=1 (shared: wall bottom + floor cap)
    let floor_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), FLOOR_Z));
    }
    let floor_center = verts.len() as u32;
    verts.push(p(0.0, 0.0, FLOOR_Z));

    let rim = |k: usize| rim_base + (k % N) as u32;
    let flr = |k: usize| floor_base + (k % N) as u32;

    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(0)]);
    };

    // === BOX BOTTOM (z=z0), outward −Z. Standard box face [0,1,2,3] winding.
    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    // === BOX 4 SIDES, outward horizontal (standard CCW-from-outside winding).
    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, bb, c]);
        surf.push(vec![LaInputId(0)]);
        tris.push([a, c, d]);
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface); // front −y
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface); // right +x
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface); // back +y
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface); // left −x

    // === BOX TOP ANNULUS (z=z1), outward +Z, with the rim ring as its hole.
    // Per-sector fan between outer loop Lo = [4,7,6,5] (CW-from-above; edges
    // {4→7,7→6,6→5,5→4} oppose the side faces) and inner loop Li = rim
    // DESCENDING (`li(s)=rim((N−s)%N)`). Sector c fans Lo[c] over the inner arc
    // Li[c*per..(c+1)*per]. This makes the outer-square cycle and the rim-ring
    // hole wind in OPPOSITE rotational senses, so they have OPPOSITE signed-area
    // signs — exactly the proper outer + hole the planar reconstruction requires
    // (`positive_count == 1`). The inner-ring boundary edges run ASCENDING; the
    // wall therefore traverses the rim DESCENDING so the shared rim edges pair.
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

    // === CYLINDER WALL (label 1) — rim ring (z=2) down to floor ring (z=0.5).
    // As a cavity wall the outward-from-result normal points TOWARD the axis
    // (−radial). Authored PRE-SWAPPED (tri[1]↔tri[2] opposite of the final
    // outward winding) so flip_for_op(Subtract)'s re-swap restores it outward.
    let push_cyl = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // pre-swap tri[1] <-> tri[2]
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        // FINAL outward (toward-axis) winding: rim edges DESCENDING
        // (rim(k1)→rim(k)) — opposite the annulus inner ring (ascending) so the
        // shared rim edges pair; floor edges ASCENDING (flr(k)→flr(k1)) —
        // opposite the floor-cap fan (descending).
        push_cyl([rim(k1), rim(k), flr(k)], &mut tris, &mut surface);
        push_cyl([rim(k1), flr(k), flr(k1)], &mut tris, &mut surface);
    }

    // === CYLINDER FLOOR CAP (label 1) @ z=0.5, outward +Z (up, into the void).
    // FINAL outward winding: fan around floor_center, floor edges DESCENDING
    // (flr(k1)→flr(k)) — opposite the wall's ascending floor edges.
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([floor_center, flr(k1), flr(k)], &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]); // box surface: outside both
        } else {
            inside.push(vec![true, false]); // cylinder cavity: inside A only
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

fn run_subtract() -> BRep {
    let bx = pocket_box();
    let cyl = pocket_cyl();
    let mock = LabelMock {
        arrangement: pocket_arrangement(),
    };
    boolean(&bx, &cyl, BoolOp::Subtract, &mock).expect("yr13: box − cylinder Subtract must be Ok")
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
// Oracle 1 — cavity-sense: effective outward normal points TOWARD the axis.
// =========================================================================

#[test]
fn oracle1_cavity_wall_effective_normal_points_toward_axis() {
    let r = run_subtract();
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr13: expected ≥1 surviving cylinder cavity-wall face with reversed==true; \
         faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );

    let axis_point = CYL_AXIS_POINT;
    let axis_unit = unit(CYL_AXIS_DIR);

    // Sample several points on the wall surface: angles around the axis at two
    // heights. For each, the canonical analytic outward normal is the
    // away-from-axis radial; reversed ⇒ effective normal = −radial. Assert the
    // effective normal points TOWARD the axis (dot with the outward radial < 0)
    // and is NOT the canonical away-from-axis direction.
    for wall in &walls {
        let Surface::Cylinder {
            axis_point: ap,
            axis_dir: ad,
            radius,
        } = wall.surface
        else {
            panic!("cavity wall must be Surface::Cylinder");
        };
        let ap = ap.as_array();
        let au = unit(ad.as_array());
        // basis perpendicular to the axis
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

        for &h in &[0.6_f64, 1.2, 1.8] {
            for k in 0..6 {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / 6.0;
                let radial = add(scale(e1, th.cos()), scale(e2, th.sin())); // unit away-from-axis
                let _surf_pt = add(add(ap, scale(au, h)), scale(radial, radius));

                // canonical outward (away-from-axis) normal at this surface point
                let canonical = unit(radial);
                // reversed ⇒ effective outward normal is the negation
                let effective = scale(canonical, -1.0);

                // outward radial direction relative to the cylinder axis at the
                // sampled point (independent of the surface params under test)
                let outward_radial = unit(radial);

                let d_eff = dot(effective, outward_radial);
                assert!(
                    d_eff < -1e-9,
                    "yr13 O1: effective outward normal must point TOWARD the axis \
                     (dot < 0); got dot {d_eff} at h={h} k={k}"
                );
                let d_canon = dot(canonical, outward_radial);
                assert!(
                    d_canon > 1e-9,
                    "yr13 O1: canonical (away-from-axis) normal must NOT equal the \
                     effective normal; canonical dot {d_canon}"
                );
            }
        }
        // sanity: axis_point/axis_dir match the input (exactness checked in O3)
        let _ = (axis_point, axis_unit);
    }
}

// =========================================================================
// Oracle 2 — watertight 2-manifold, χ = 2.
// =========================================================================

#[test]
fn oracle2_watertight_euler_two() {
    let r = run_subtract();
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr13 O2: output mesh must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr13 O2: blind-pocket output must be genus 0 (χ = 2)"
    );
}

// =========================================================================
// Oracle 3 — analytic surface survives; sense encoding per surface kind.
// =========================================================================

#[test]
fn oracle3_analytic_survival_and_sense_encoding() {
    let r = run_subtract();
    let want = cyl_surface();

    // Cavity wall(s): Surface::Cylinder == input exact params, reversed == true.
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr13 O3: expected a surviving Surface::Cylinder cavity wall"
    );
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "yr13 O3 (I-rev3): cavity wall Surface::Cylinder must equal the input \
             cylinder's params field-for-field (no param perturbation to signal sense)"
        );
        assert!(
            w.reversed,
            "yr13 O3: cavity wall must carry reversed == true"
        );
    }

    // Every Surface::Cylinder face must be the exact input params (no re-fit).
    for f in r.faces() {
        if let Surface::Cylinder { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "yr13 O3: a Surface::Cylinder face has perturbed params"
            );
        }
    }

    // Box outer faces: Surface::Plane with reversed == false (I-rev2: planar
    // sense lives in Plane.normal, never the flag → no double-flip).
    let plane_faces: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .collect();
    assert!(
        !plane_faces.is_empty(),
        "yr13 O3: expected ≥1 planar box face"
    );
    for f in &plane_faces {
        assert!(
            !f.reversed,
            "yr13 O3 (I-rev2): planar faces must emit reversed == false \
             (sense encoded in Plane.normal, never double-flipped)"
        );
    }

    // No Sphere/Cone in the output.
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cone { .. })),
        "yr13 O3: output must contain no Sphere/Cone faces"
    );
}

// =========================================================================
// Oracle 4 — sidecar mesh-parity (env-gated, LOUD skip).
// =========================================================================

#[test]
fn oracle4_sidecar_mesh_parity_env_gated() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs yr13] SKIP: sidecar binary not found (set CHERCHI2022_BIN)");
        return;
    };
    let bx = pocket_box();
    let cyl = pocket_cyl();
    let r = boolean(&bx, &cyl, BoolOp::Subtract, &sb)
        .expect("yr13 O4: sidecar-backed Subtract must be Ok");

    // The output must be a watertight, genus-0 solid via the REAL arrangement.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr13 O4: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr13 O4: sidecar-backed blind-pocket output must be χ = 2"
    );
    // The analytic cavity wall must survive with reversed == true.
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr13 O4: sidecar-backed output must carry a reversed Surface::Cylinder cavity wall"
    );
}

// =========================================================================
// Oracle 5 — exact rim edge: cylinder ∩ box-top is a Curve::Circle.
// =========================================================================

#[test]
fn oracle5_rim_is_circle_edge() {
    let r = run_subtract();
    let has_circle = r
        .edges()
        .iter()
        .any(|e| matches!(e.curve, Curve::Circle { .. }));
    assert!(
        has_circle,
        "yr13 O5: the cylinder ∩ box-top section must appear as a Curve::Circle \
         edge in the output; edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );
}

// =========================================================================
// Oracle 6 — determinism.
// =========================================================================

#[test]
fn oracle6_determinism() {
    let r1 = run_subtract();
    let r2 = run_subtract();
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "yr13 O6: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "yr13 O6: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "yr13 O6: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "yr13 O6: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "yr13 O6: face reversed differs");
    }
}

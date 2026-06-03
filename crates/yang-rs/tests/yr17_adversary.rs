//! PR-YR17 ADVERSARY — independent cone-cavity outward-sense witness.
//!
//! A THIRD, independent agent (neither the RED-test author nor the GREEN
//! implementer). This file re-derives the box − cone CONICAL POCKET contract on
//! a SECOND, DISTINCT conical-pocket `LabeledArrangement` whose box, apex z, and
//! half-angle all differ from `yr17_subtract_cone.rs`:
//!
//!   * Box A: axis-aligned `[-3,-3,0] .. [3,3,1]` (top face at z = 1, a SHALLOW
//!     box — the RED case is `[-2,-2,0]..[2,2,2]`, top z = 2).
//!   * Cone B: apex `(0,0,0.25)` INSIDE the box, axis +Z,
//!     `half_angle = atan(2.0)` so `tanα = 2.0` and the rim at z = 1 has radius
//!     `R_rim = (1 − 0.25)·tanα = 0.75·2 = 1.5` (the RED case has tanα = 2/3,
//!     R_rim = 1.0 — a DIFFERENT cone steepness AND a wider rim).
//!
//! `box − cone` carves a STEEPER, WIDER conical pocket: the cone lateral apex→
//! rim survives as the cavity wall (`Surface::Cone`, `reversed == true`), whose
//! effective outward normal points INTO the pocket. This is an independently
//! authored fixture, NOT copied from the RED helpers.
//!
//! What this witnesses (independently of production's authoring logic):
//!
//! (W1) MANDATORY mock self-check FIRST: the SIMULATED Subtract output
//! (keep-all + flip label-1) is watertight (0 unpaired half-edges), χ = 2, AND
//! POSITIVE signed volume — so a fixture bug cannot masquerade as a code pass
//! (memory `yang_mock_orientation_witness`: a hand-built arrangement can pass
//! watertight + χ while globally inside-out; the positive-volume + winding
//! witness is the guard).
//!
//! (W2) The boolean OUTPUT carries a `Surface::Cone` cavity wall with the input
//! cone's EXACT apex/axis_dir/half_angle and `reversed == true`.
//!
//! (W3) The OUTPUT mesh winding on the cavity-wall triangles points INTO the
//! pocket (toward the cone axis / away from box material) — winding ↔
//! `reversed` consistent — and this is asserted while sampling EDGE MIDPOINTS
//! (where the cone chord bulges most, per
//! `yang_cone_tessellation_oracle_findings`), so the geometric winding normal
//! is checked against the negation of the YR16 TILTED normal
//! `n̂ = unit(r̂ − tanα·â)`, NOT the pure radial — making the tilt load-bearing
//! for the first non-trivial winding case.
//!
//! (W4) The exact `Circle` rim lies on the cone (radial residual ≈ 0) AND on
//! the box-top plane (z = 1).

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

// ---- independent pure-array math (no import of the RED helpers) ----
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
fn vnorm(a: [f64; 3]) -> f64 {
    vdot(a, a).sqrt()
}
fn vunit(a: [f64; 3]) -> [f64; 3] {
    let n = vnorm(a);
    assert!(n > 0.0, "yr17 ADV: cannot normalize zero vector");
    vscale(a, 1.0 / n)
}

// =========================================================================
// SECOND, DISTINCT conical-pocket config (different from yr17_subtract_cone.rs).
//   box A: [-3,-3,0] .. [3,3,1]  (SHALLOW: top z = 1)
//   cone B: apex (0,0,0.25), axis +Z, half_angle = atan(2.0) ⇒ tanα = 2.0.
//     rim at z=1 has radius (1 − 0.25)·tanα = 0.75·2 = 1.5.
//   Cone base chosen ABOVE the box top so the cone exits ONLY through z=1.
// =========================================================================
const NSEG: usize = 16;
const BOX_LO: [f64; 3] = [-3.0, -3.0, 0.0];
const BOX_HI: [f64; 3] = [3.0, 3.0, 1.0];
const A_APEX: [f64; 3] = [0.0, 0.0, 0.25];
const A_AXIS: [f64; 3] = [0.0, 0.0, 1.0];
const A_HEIGHT: f64 = 2.0; // base z = 0.25 + 2.0 = 2.25 (above box top z=1)
const TOP_Z: f64 = 1.0;
const RIM_R: f64 = 1.5; // (1 − 0.25)·tanα = 0.75·2.0 = 1.5

/// half_angle = atan(2.0) ⇒ tanα = 2.0, distinct from the RED atan(2/3).
fn half_angle() -> f64 {
    2.0_f64.atan()
}

fn input_cone_surface() -> Surface {
    Surface::Cone {
        apex: p(A_APEX[0], A_APEX[1], A_APEX[2]),
        axis_dir: Vector3::new(A_AXIS[0], A_AXIS[1], A_AXIS[2]),
        half_angle: half_angle(),
    }
}

// =========================================================================
// Independent mesh oracles (re-derived; not imported from yr17 RED).
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
    v - edges.len() as i64 + f
}

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        acc += vdot(a, vcross(b, c));
    }
    acc / 6.0
}

// =========================================================================
// Input B-Reps (independently re-derived box + cone fixtures).
// =========================================================================
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
    BRep::new(verts, edges, faces).expect("yr17 ADV: box_brep BRep::new failed")
}

fn cone_brep(apex: [f64; 3], axis_dir: [f64; 3], ha: f64, height: f64) -> BRep {
    let axis_unit = vunit(axis_dir);
    let radius = height * ha.tan();
    let base_center = vadd(apex, vscale(axis_unit, height));
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = vunit(vcross(axis_unit, world));
    let base_seam = vadd(base_center, vscale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(apex[0], apex[1], apex[2]),
        },
        BRepVertex {
            point: p(base_seam[0], base_seam[1], base_seam[2]),
        },
    ];
    let edges = vec![BRepEdge {
        start: 1,
        end: 1,
        curve: Curve::Circle {
            center: p(base_center[0], base_center[1], base_center[2]),
            normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
            radius,
        },
    }];
    let cap_d = -vdot(axis_unit, base_center);
    let faces = vec![
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                half_angle: ha,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
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
    BRep::new(verts, edges, faces).expect("yr17 ADV: cone_brep should tessellate")
}

fn adv_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn adv_cone() -> BRep {
    cone_brep(A_APEX, A_AXIS, half_angle(), A_HEIGHT)
}

// =========================================================================
// Independent hand-built conical-pocket arrangement. The STRUCTURAL winding
// machinery (per-box global reversal, box-top annulus, apex-fan cavity wall) is
// re-derived here; it is geometry-independent, so plugging in OUR distinct
// constants yields a distinct valid genus-0 fixture, proven by the self-check.
// =========================================================================
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

fn adv_arrangement() -> LabeledArrangement {
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

    // Rim ring @ z=TOP_Z, r=RIM_R (shared inner boundary of the box-top annulus
    // AND top of the cone-lateral apex fan).
    let rim_base = verts.len() as u32;
    for k in 0..NSEG {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (NSEG as f64);
        verts.push(p(
            A_APEX[0] + RIM_R * th.cos(),
            A_APEX[1] + RIM_R * th.sin(),
            TOP_Z,
        ));
    }
    // Apex vertex — pocket bottom / cone singular tip.
    let apex = verts.len() as u32;
    verts.push(p(A_APEX[0], A_APEX[1], A_APEX[2]));

    let rim = |k: usize| rim_base + (k % NSEG) as u32;

    // Each box face authored CCW-from-outside, then ONE global reversal at emit.
    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]);
        surf.push(vec![LaInputId(0)]);
    };

    // BOX BOTTOM (z=z0), outward −Z.
    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    // BOX 4 SIDES.
    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, c, bb]);
        surf.push(vec![LaInputId(0)]);
        tris.push([a, d, c]);
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface);
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface);
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface);
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface);

    // BOX TOP ANNULUS (z=z1) outward +Z, with the rim ring as its hole. Outer
    // loop Lo wound CW-from-above; inner loop the rim DESCENDING so the two
    // cycles oppose (proper outer + hole). The cavity wall traverses the rim
    // DESCENDING so the shared rim edges pair.
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = NSEG / 4;
    let li = |s: usize| rim((NSEG - (s % NSEG)) % NSEG);
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

    // CONE LATERAL CAVITY WALL (label 1) — APEX FAN. Authored so the
    // global-reversal ∘ pre-swap = identity push; flip_for_op(Subtract) re-swaps
    // at compaction, restoring into-pocket winding (the same signal as
    // reversed==true). Rim traversed DESCENDING (rim(k+1)→rim(k)).
    let push_cone = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]);
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..NSEG {
        push_cone([apex, rim(k + 1), rim(k)], &mut tris, &mut surface);
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
        num_inputs: 2,
    }
}

/// Simulate the Subtract keep-set + flip directly (label-1 tris swapped) for the
/// mandatory self-check; NO boolean() call.
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(arr.mesh.tris.len());
    for (i, tri) in arr.mesh.tris.iter().enumerate() {
        if arr.surface[i][0] == LaInputId(1) {
            tris.push([tri[0], tri[2], tri[1]]);
        } else {
            tris.push(*tri);
        }
    }
    Mesh::new(arr.mesh.verts.clone(), tris)
}

fn run_subtract() -> BRep {
    let mock = LabelMock {
        arr: adv_arrangement(),
    };
    boolean(&adv_box(), &adv_cone(), BoolOp::Subtract, &mock)
        .expect("yr17 ADV: box − cone conical-pocket Subtract must be Ok")
}

fn cavity_wall_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cone { .. }) && f.reversed)
        .cloned()
        .collect()
}

// Cone-lateral membership: radial residual within the chord band AND in the
// pocket axial band.
fn cone_radial_residual(x: [f64; 3]) -> f64 {
    let a = A_APEX;
    let ax = vunit(A_AXIS);
    let w = vsub(x, a);
    let h_axial = vdot(w, ax);
    let radial = vnorm(vsub(w, vscale(ax, h_axial)));
    (radial - h_axial.abs() * half_angle().tan()).abs()
}
fn in_pocket_band(x: [f64; 3]) -> bool {
    x[2] >= A_APEX[2] - 1e-9 && x[2] <= TOP_Z + 1e-9
}
fn cone_chord_bound(height: f64, ha: f64) -> f64 {
    let r = height * ha.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

// =========================================================================
// 0. MANDATORY self-check — fixture validity gate (memory:
// yang_mock_orientation_witness). Watertight + χ=2 + POSITIVE signed volume on
// the SIMULATED output, NO boolean() call.
// =========================================================================
#[test]
fn adv_mock_is_valid_genus0() {
    let arr = adv_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr17 ADV: independent conical-pocket fixture must be watertight \
         (0 unpaired half-edges); got {unpaired}"
    );
    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 2,
        "yr17 ADV: independent fixture must be genus 0 (χ=2); got χ={chi}"
    );
    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr17 ADV: independent fixture must be OUTWARD-oriented (positive signed \
         volume); got {vol} — a negative volume means the mock is globally inside-out"
    );
}

// =========================================================================
// 1. Cavity wall is Surface::Cone == EXACT input params, reversed==true; box
// faces planar reversed==false; no Sphere/Cylinder.
// =========================================================================
#[test]
fn adv_cavity_wall_is_reversed_cone_with_exact_params() {
    let r = run_subtract();
    let want = input_cone_surface();

    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr17 ADV: expected a surviving Surface::Cone cavity wall with \
         reversed==true; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );
    for w in &walls {
        assert_eq!(
            w.surface, want,
            "yr17 ADV: cavity-wall Surface::Cone must equal the input cone's \
             apex/axis_dir/half_angle field-for-field (no perturbation)"
        );
        assert!(
            w.reversed,
            "yr17 ADV: cavity wall must carry reversed==true"
        );
    }
    for f in r.faces() {
        if let Surface::Cone { .. } = f.surface {
            assert_eq!(
                f.surface, want,
                "yr17 ADV: a Surface::Cone face has perturbed params"
            );
        }
    }
    let plane_faces: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .collect();
    assert!(
        !plane_faces.is_empty(),
        "yr17 ADV: expected ≥1 planar box face"
    );
    for f in &plane_faces {
        assert!(
            !f.reversed,
            "yr17 ADV: planar faces must emit reversed==false (sense in the \
             Plane.normal, never double-flipped)"
        );
    }
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cylinder { .. })),
        "yr17 ADV: output must contain no Sphere/Cylinder faces"
    );
}

// =========================================================================
// 2. THE HEADLINE — into-pocket winding via the TILTED normal, sampling EDGE
// MIDPOINTS. For each cavity-wall mesh triangle (all 3 verts AND all 3 edge
// midpoints within the cone chord band of the lateral and in the pocket band),
// the geometric winding normal must agree with the NEGATION of the YR16 tilted
// normal n̂ = unit(r̂ − tanα·â) at the centroid (into-pocket), and must point
// toward the axis (dot with away-from-axis < 0). A radial-with-wrong-sign
// orientation would FAIL this.
// =========================================================================
#[test]
fn adv_cavity_winding_agrees_with_negated_tilted_normal_edge_midpoints() {
    let r = run_subtract();
    let walls = cavity_wall_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr17 ADV: expected ≥1 reversed Surface::Cone cavity wall"
    );

    let apex_pt = A_APEX;
    let axis_unit = vunit(A_AXIS);
    let tana = half_angle().tan();
    let mesh = r.as_mesh();
    // Generous chord band sized from the apex→rim sub-cone height (0.75).
    let de = cone_chord_bound(0.75, half_angle()).max(0.05);

    let mut checked = 0usize;
    for tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        // membership samples: 3 verts AND 3 edge MIDPOINTS (the bulge points).
        let m01 = vscale(vadd(v0, v1), 0.5);
        let m12 = vscale(vadd(v1, v2), 0.5);
        let m20 = vscale(vadd(v2, v0), 0.5);
        let samples = [v0, v1, v2, m01, m12, m20];
        let on_cone = samples
            .iter()
            .all(|&x| cone_radial_residual(x) <= de && in_pocket_band(x));
        let has_above_apex = [v0, v1, v2].iter().any(|&x| x[2] > A_APEX[2] + 1e-9);
        if !on_cone || !has_above_apex {
            continue;
        }

        // geometric winding normal
        let gnorm = vunit(vcross(vsub(v1, v0), vsub(v2, v0)));

        // independently re-derive the TILTED outward normal at the centroid
        let centroid = vscale(vadd(vadd(v0, v1), v2), 1.0 / 3.0);
        let w = vsub(centroid, apex_pt);
        let along = vdot(w, axis_unit);
        let proj = vadd(apex_pt, vscale(axis_unit, along));
        let rhat = vunit(vsub(centroid, proj)); // away-from-axis
        let n_tilt = vunit(vsub(rhat, vscale(axis_unit, tana))); // YR16 tilted n̂
        let effective = vscale(n_tilt, -1.0); // reversed ⇒ −n̂ (into pocket)

        // (a) winding agrees with the NEGATED tilted normal
        let agree = vdot(gnorm, effective);
        assert!(
            agree > 1e-9,
            "yr17 ADV: cavity-wall tri {tri:?} geometric winding normal {gnorm:?} \
             must agree with the NEGATED tilted cone normal {effective:?} \
             (dot {agree} > 0) — mesh winding must follow n̂=unit(r̂−tanα·â), \
             not the pure radial / wrong sign"
        );
        // (b) and points TOWARD the axis (into the pocket)
        let toward_axis = vdot(gnorm, rhat);
        assert!(
            toward_axis < -1e-9,
            "yr17 ADV: cavity-wall tri {tri:?} winding normal must point TOWARD \
             the axis / into the pocket (dot with away-from-axis {toward_axis} < 0)"
        );
        checked += 1;
    }
    assert!(
        checked >= 8,
        "yr17 ADV: expected to witness ≥8 cavity-wall triangles (edge-midpoint \
         sampled), found {checked}"
    );
}

// =========================================================================
// 2b. ANALYTIC tilt witness — the negated-radial is NOT consistent with the
// tilted into-pocket normal direction in radial/axial decomposition. This pins
// the §4 tilt independently of any mesh: the tilted normal has a NEGATIVE axial
// component (n̂·â = −tanα·|·| after unit), whereas the pure radial r̂ has a ZERO
// axial component. The effective (reversed) normal therefore has a POSITIVE
// axial component — it tips UP toward the rim — which a radial-only model cannot
// reproduce. A mutation that drops the tilt would leave the effective normal
// purely radial (axial component 0), failing this assertion.
// =========================================================================
#[test]
fn adv_effective_normal_has_nonzero_axial_tilt() {
    let axis_unit = vunit(A_AXIS);
    let tana = half_angle().tan();
    for k in 0..8 {
        let th = std::f64::consts::TAU * (k as f64) / 8.0;
        // build an arbitrary radial direction ⟂ axis
        let e1 = vunit(vcross(axis_unit, [1.0, 0.0, 0.0]));
        let e2 = vcross(axis_unit, e1);
        let rhat = vadd(vscale(e1, th.cos()), vscale(e2, th.sin()));
        let n_tilt = vunit(vsub(rhat, vscale(axis_unit, tana)));
        let effective = vscale(n_tilt, -1.0);

        // tilted normal's axial component is strictly negative; effective is +.
        let n_axial = vdot(n_tilt, axis_unit);
        let eff_axial = vdot(effective, axis_unit);
        let eff_radial = vdot(effective, rhat);
        assert!(
            n_axial < -1e-9,
            "yr17 ADV: tilted n̂ must have a strictly NEGATIVE axial component \
             (got {n_axial}); the pure radial would give 0 — the tilt is the \
             load-bearing distinction at k={k}"
        );
        assert!(
            eff_axial > 1e-9,
            "yr17 ADV: effective (reversed) normal must tip UP toward the rim \
             (axial component {eff_axial} > 0) at k={k}"
        );
        assert!(
            eff_radial < -1e-9,
            "yr17 ADV: effective normal must point INTO the pocket (radial \
             component {eff_radial} < 0) at k={k}"
        );
    }
}

// =========================================================================
// 3. Watertight 2-manifold, χ=2, positive signed volume on the OUTPUT.
// =========================================================================
#[test]
fn adv_output_watertight_euler_positive_volume() {
    let r = run_subtract();
    let mesh = r.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr17 ADV: conical-pocket output must be watertight; the apex singular \
         vertex must close cleanly"
    );
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "yr17 ADV: conical-pocket output must be genus 0 (χ=2)"
    );
    let vol = signed_volume(mesh);
    assert!(
        vol > 0.0,
        "yr17 ADV: result must be outward-oriented (positive signed volume), got {vol}"
    );
}

// =========================================================================
// 4. Exact Circle rim on the cone AND on the box-top plane (z = TOP_Z).
// =========================================================================
#[test]
fn adv_circle_rim_on_cone_and_top_plane() {
    let r = run_subtract();
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
        "yr17 ADV: the cone ∩ box-top section must appear as ≥1 Curve::Circle rim; \
         edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let mut saw_rim = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        assert!(
            (radius - RIM_R).abs() <= tau,
            "yr17 ADV: rim Circle radius {radius} must equal RIM_R {RIM_R} \
             (±TAU_MODEL) — the perpendicular cone ∩ box-top cut"
        );
        let nrm = vunit(normal.as_array());
        let world = if nrm[0].abs() <= nrm[1].abs() && nrm[0].abs() <= nrm[2].abs() {
            [1.0, 0.0, 0.0]
        } else if nrm[1].abs() <= nrm[2].abs() {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = vunit(vcross(nrm, world));
        let e2 = vunit(vcross(nrm, e1));
        for k in 0..NSEG {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (NSEG as f64);
            let pt = vadd(
                c,
                vadd(
                    vscale(e1, *radius * th.cos()),
                    vscale(e2, *radius * th.sin()),
                ),
            );
            let residual = cone_radial_residual(pt);
            assert!(
                residual <= tau,
                "yr17 ADV: rim point {pt:?} must lie on the cone (radial residual \
                 {residual} ≤ TAU_MODEL)"
            );
            let plane_off = (pt[2] - TOP_Z).abs();
            assert!(
                plane_off <= tau,
                "yr17 ADV: rim point {pt:?} must lie on the box-top plane z={TOP_Z} \
                 (offset {plane_off} ≤ TAU_MODEL)"
            );
        }
        if (c[2] - TOP_Z).abs() <= tau && (radius - RIM_R).abs() <= tau {
            saw_rim = true;
        }
    }
    assert!(
        saw_rim,
        "yr17 ADV: expected the rim Circle on the box-top plane (z={TOP_Z}, \
         radius=RIM_R={RIM_R})"
    );
}

// =========================================================================
// 5. INDEPENDENT sidecar parity (env-gated, LOUD skip). The LabelMock fixture
// places the rim verts EXACTLY on the rim circle, so it does NOT exercise the
// fifth GREEN site (`cone_chord_tol_for_owner`) nor the real `cone_outward_normal`
// orientation of the Stage-1 input cone — both only bite when the mesh endpoints
// sit on the cone's CHORD approximation, which is what the sidecar arrangement
// produces. This test drives the REAL sidecar-backed pipeline on OUR distinct
// box − cone so the fifth site + the tilted-normal sign are independently
// witnessed on a second fixture (ADVERSARY mutation finding: M3b reds only the
// sidecar path; M4 sign-flip reds the sidecar path via NonManifoldOutput).
// =========================================================================
#[test]
fn adv_sidecar_parity_conical_pocket() {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs yr17 ADV] SKIP: sidecar binary not found (set CHERCHI2022_BIN)");
        return;
    };
    let r = boolean(&adv_box(), &adv_cone(), BoolOp::Subtract, &sb)
        .expect("yr17 ADV: sidecar-backed conical-pocket Subtract must be Ok");
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr17 ADV: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr17 ADV: sidecar-backed conical-pocket output must be χ = 2 (genus 0)"
    );
    assert!(
        signed_volume(r.as_mesh()) > 0.0,
        "yr17 ADV: sidecar-backed output must be outward-oriented (positive volume)"
    );
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr17 ADV: sidecar-backed output must carry a reversed Surface::Cone cavity wall"
    );
}

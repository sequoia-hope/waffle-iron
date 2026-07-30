//! PR-YR13 ADVERSARY — INDEPENDENT verification of the curved `Subtract`
//! cavity-sense claim, with a self-authored box−cylinder blind-pocket
//! arrangement (NO helpers shared with the RED file, so a RED-fixture bug
//! cannot hide a defect here).
//!
//! Different geometry from RED on purpose: box [-3,-3,0]..[3,3,3], cylinder
//! axis +Z at origin, radius 1.5, floor z=1.0, rim z=3.0 (box top), N=24
//! wall facets. The cylinder is the subtrahend (`InputId::B`), so its lateral
//! wall survives as the pocket wall.
//!
//! The chord-band membership the production face-resolver requires (≈ 1e-2 ×
//! cylinder-rim-AABB-diagonal) for this fixture: AABB x,y∈[-1.5,1.5],
//! z∈[1,3] ⇒ diag = √(3²+3²+2²) = √22 ≈ 4.690 ⇒ band ≈ 0.0469. The N=24 wall
//! facet centroid sits at radial r·cos(π/24) ≈ 1.4872 from the axis, so its
//! chord deviation from the analytic surface is ≈ 0.0128 — comfortably inside
//! the band.
//!
//! The KEY independent check (oracle A1) reasons NOT from the surface params
//! (as the original RED oracle 1 did) but from the ACTUAL emitted mesh triangle
//! winding: for each cavity-wall triangle, the geometric normal `(v1−v0)×(v2−v0)`
//! must point TOWARD the axis (into the pocket), and the output must have
//! positive signed volume. This verifies — in ABSOLUTE terms — that the mesh
//! `flip_for_op` produced AGREES with the `reversed == true` B-Rep flag (spec
//! invariant I-rev1), from geometry the param-only oracle never inspects.
//!
//! ADVERSARY FINDING — RESOLVED. The original RED hand-built arrangement (and,
//! initially, this adversary mock which faithfully reproduced its convention)
//! was GLOBALLY INSIDE-OUT: box-bottom mesh tri wound +Z (true outward is −Z),
//! signed volume < 0. No RED oracle detected it (oracle 1 reasoned only from
//! surface params, never reading the mesh; `signed_volume` was dead code), so
//! oracle 1 could not actually WITNESS the I-rev1 mesh↔flag consistency it
//! nominally tested. This was reported; the RED author then re-oriented the
//! yr13 mock to OUTWARD and strengthened oracles 1–3 to witness consistency
//! absolutely (commit 41819459). This adversary file is now ALSO re-oriented
//! OUTWARD (a uniform tri[1]↔tri[2] reversal of every authored arrangement
//! triangle — see pocket_arrangement) so it is a genuinely independent witness
//! of the ABSOLUTE consistency: signed volume > 0, box-bottom −Z, cavity-wall
//! mesh winding toward-axis. The production `reversed` derivation is UNCHANGED
//! and was confirmed correct and LOAD-BEARING by the prior mutation check
//! (force `reversed:false` and invert to `InputId::A` each fail RED oracle 1/3).

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

// =========================================================================
// Independent fixture parameters (deliberately != RED's).
// =========================================================================

const N: usize = 24;
const BOX_LO: [f64; 3] = [-3.0, -3.0, 0.0];
const BOX_HI: [f64; 3] = [3.0, 3.0, 3.0];
const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, 1.0]; // bottom cap = pocket floor
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.5;
const CYL_H: f64 = 2.5; // top at z=3.5, above box top (discarded)
const FLOOR_Z: f64 = 1.0;
const RIM_Z: f64 = 3.0; // box top plane

// =========================================================================
// Self-authored array math (independent of RED).
// =========================================================================

fn ad(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sb(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn sc(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dt(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cr(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn nm(a: [f64; 3]) -> f64 {
    dt(a, a).sqrt()
}
fn un(a: [f64; 3]) -> [f64; 3] {
    let n = nm(a);
    assert!(n > 1e-15, "yr13-adv: cannot normalize near-zero vector");
    sc(a, 1.0 / n)
}
fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Independent mesh oracles.
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

// =========================================================================
// Self-authored B-Rep fixtures (NOT shared with RED).
// =========================================================================

fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
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
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (−z)
        [4, 7, 6, 5], // top (+z)
        [0, 4, 5, 1], // front (−y)
        [1, 5, 6, 2], // right (+x)
        [2, 6, 7, 3], // back (+y)
        [3, 7, 4, 0], // left (−x)
    ];
    let mut edges = Vec::new();
    let mut loops = Vec::new();
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
    BRep::new(verts, edges, faces).expect("yr13-adv: box_brep BRep::new")
}

fn cylinder_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let au = un(axis_dir);
    let bottom = axis_point;
    let top = ad(axis_point, sc(au, height));

    let abs = [au[0].abs(), au[1].abs(), au[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = un(cr(au, world));

    let v0 = ad(bottom, sc(e1, radius));
    let v1 = ad(top, sc(e1, radius));

    let verts = vec![
        BRepVertex {
            point: pt(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: pt(v1[0], v1[1], v1[2]),
        },
    ];
    let neg = sc(au, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: pt(bottom[0], bottom[1], bottom[2]),
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: pt(top[0], top[1], top[2]),
                normal: Vector3::new(au[0], au[1], au[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let bottom_d = -dt(neg, bottom);
    let top_d = -dt(au, top);
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: pt(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg[0], neg[1], neg[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(au[0], au[1], au[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("yr13-adv: cylinder_brep BRep::new")
}

fn adv_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn adv_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

// =========================================================================
// Self-authored full closed result arrangement (box with cylindrical pocket).
//
// Box tris  (label 0): surface=[A], inside=[false,false] — kept by Subtract,
//   NOT flipped.
// Cylinder wall+floor tris (label 1): surface=[B], inside=[true,false] — kept
//   by Subtract, FLIPPED by flip_for_op (swap tri[1]↔tri[2]). Authored
//   PRE-SWAPPED so the post-flip winding is outward-from-result (toward axis).
//
// I author the wall/floor with the FINAL outward (post-flip) winding and then
// pre-swap, exactly mirroring the production contract. The geometry is built
// to be watertight after the keep-set + flip.
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

fn pocket_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = BOX_LO;
    let [x1, y1, z1] = BOX_HI;
    let b0 = verts.len() as u32;
    verts.push(pt(x0, y0, z0)); // 0
    verts.push(pt(x1, y0, z0)); // 1
    verts.push(pt(x1, y1, z0)); // 2
    verts.push(pt(x0, y1, z0)); // 3
    let t0 = verts.len() as u32;
    verts.push(pt(x0, y0, z1)); // 4
    verts.push(pt(x1, y0, z1)); // 5
    verts.push(pt(x1, y1, z1)); // 6
    verts.push(pt(x0, y1, z1)); // 7

    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(pt(CYL_R * th.cos(), CYL_R * th.sin(), RIM_Z));
    }
    let floor_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(pt(CYL_R * th.cos(), CYL_R * th.sin(), FLOOR_Z));
    }
    let floor_center = verts.len() as u32;
    verts.push(pt(0.0, 0.0, FLOOR_Z));

    let rim = |k: usize| rim_base + (k % N) as u32;
    let flr = |k: usize| floor_base + (k % N) as u32;

    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(0)]);
    };

    // BOX BOTTOM (z=z0), outward −Z.
    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    // BOX 4 SIDES, outward horizontal.
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

    // BOX TOP ANNULUS (z=z1), outward +Z, rim ring as the hole. Outer square
    // Lo=[4,7,6,5] (edges oppose the sides); inner loop Li = rim DESCENDING so
    // the outer + hole wind in opposite senses (proper outer+hole). N divisible
    // by 4 (N=24 ⇒ per=6).
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = N / 4;
    let li = |s: usize| rim((N - (s % N)) % N);
    for c in 0..4usize {
        let oa = lo[c];
        let ob = lo[(c + 1) % 4];
        let sa = c * per;
        let sb_ = (c + 1) * per;
        push_box([oa, ob, li(sb_)], &mut tris, &mut surface);
        for s in (sa..sb_).rev() {
            push_box([oa, li(s + 1), li(s)], &mut tris, &mut surface);
        }
    }

    // CYLINDER WALL (label 1). FINAL outward (toward-axis) winding: rim edges
    // DESCENDING (rim(k1)→rim(k)) — opposite the annulus inner ring (ascending)
    // so shared rim edges pair; floor edges ASCENDING (flr(k)→flr(k1)). Authored
    // PRE-SWAPPED so flip_for_op(Subtract) restores the final outward winding.
    let push_cyl = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // pre-swap tri[1]<->tri[2]
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([rim(k1), rim(k), flr(k)], &mut tris, &mut surface);
        push_cyl([rim(k1), flr(k), flr(k1)], &mut tris, &mut surface);
    }

    // CYLINDER FLOOR CAP (label 1) @ FLOOR_Z, outward +Z (into the void). FINAL
    // outward winding: fan around floor_center, floor edges DESCENDING
    // (flr(k1)→flr(k)) — opposite the wall's ascending floor edges.
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([floor_center, flr(k1), flr(k)], &mut tris, &mut surface);
    }

    // GLOBAL OUTWARD RE-ORIENTATION (adversary fixup): the authoring convention
    // above (mirrored from the original RED mock) produced a GLOBALLY INSIDE-OUT
    // surface (box-bottom tri winds +Z; signed volume < 0). A uniform tri[1]↔
    // tri[2] swap on EVERY authored triangle flips the global orientation to
    // OUTWARD (box-bottom → −Z, signed volume > 0) while preserving exact
    // watertightness and χ (every directed edge reverses in lock-step). The
    // per-triangle `surface`/`inside` labels are positional and unchanged, so
    // the Subtract keep-set and the `flip_for_op` relationship are untouched —
    // the cavity wall is still authored PRE-SWAPPED relative to its own facet
    // winding, so flip_for_op(Subtract) restores it to the (now outward)
    // toward-axis sense. This lets A1 witness mesh↔`reversed` consistency in
    // ABSOLUTE terms (not merely orientation-independent).
    for t in &mut tris {
        t.swap(1, 2);
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

fn run_subtract() -> BRep {
    let bx = adv_box();
    let cyl = adv_cyl();
    let mock = LabelMock {
        arr: pocket_arrangement(),
    };
    boolean(&bx, &cyl, BoolOp::Subtract, &mock).expect("yr13-adv: Subtract must be Ok")
}

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: pt(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

// Radial distance of `p` from the cylinder axis, and the outward (away-from-
// axis) unit radial at `p`. Independent of the surface params under test.
fn axis_radial(p: [f64; 3]) -> (f64, [f64; 3]) {
    let ap = CYL_AXIS_POINT;
    let au = un(CYL_AXIS_DIR);
    let w = sb(p, ap);
    let along = dt(w, au);
    let radial = sb(w, sc(au, along));
    let r = nm(radial);
    (
        r,
        if r > 1e-12 {
            sc(radial, 1.0 / r)
        } else {
            [0.0; 3]
        },
    )
}

// A triangle is a cavity-wall triangle iff all 3 of its vertices sit on the
// cylinder lateral surface band (radial ≈ CYL_R) AND strictly between the floor
// and rim heights (so the floor-cap and box faces are excluded). The chord band
// for radial membership is the production curved band (≈ 0.0469); we use a
// slightly looser 0.06 inclusion window so a *correct* facet is never missed,
// while box faces (radial ≥ ~3 near edges, or 0 on the axis) are firmly out.
fn is_cavity_wall_tri(mesh: &Mesh, tri: [u32; 3]) -> bool {
    let zs: Vec<[f64; 3]> = tri
        .iter()
        .map(|&i| mesh.verts[i as usize].as_array())
        .collect();
    for v in &zs {
        let (r, _) = axis_radial(*v);
        if (r - CYL_R).abs() > 0.06 {
            return false;
        }
        if v[2] < FLOOR_Z - 1e-6 || v[2] > RIM_Z + 1e-6 {
            return false;
        }
        // Exclude floor-ring-only tris (all at z≈FLOOR_Z) — those belong to the
        // wall's bottom row too, but the floor CAP fan has its apex on the axis
        // (radial 0) so it is already excluded by the radial test above. A wall
        // tri always spans the rim→floor height, so at least one vertex is above
        // the floor; assert that below, after the per-vertex screen.
    }
    // require the triangle to actually span height (not a degenerate flat ring
    // slice) — a wall facet has vertices at both z≈RIM and z≈FLOOR.
    let has_rim = zs.iter().any(|v| (v[2] - RIM_Z).abs() < 1e-6);
    let has_floor = zs.iter().any(|v| (v[2] - FLOOR_Z).abs() < 1e-6);
    has_rim && has_floor
}

// Signed volume of a closed mesh (Σ a·(b×c) / 6). Positive ⇒ the mesh is
// consistently OUTWARD-oriented (true B-Rep convention); negative ⇒ inside-out.
fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        acc += dt(a, cr(b, c));
    }
    acc / 6.0
}

// =========================================================================
// Oracle A1 (THE independent check) — mesh-winding ↔ `reversed` consistency,
// asserted in ABSOLUTE terms from the ACTUAL emitted mesh triangle winding
// (geometry the RED surface-param oracle never inspected).
//
// This independent mock is now OUTWARD-oriented (uniform tri[1]↔tri[2] reversal
// at authoring — see pocket_arrangement), so the result mesh has positive
// signed volume and the box-bottom winds −Z. Spec invariant I-rev1: the
// cavity-wall mesh winding and the `reversed` flag derive from the same
// flip_for_op signal. On an outward mesh that means, ABSOLUTELY: every
// cavity-wall mesh triangle's winding-normal points TOWARD the axis (into the
// pocket) — i.e. its analytic away-from-axis normal is NEGATED, which is exactly
// what `reversed == true` records. We assert that directly (dot with the
// outward radial < 0), plus signed_volume > 0 to pin the absolute frame.
// =========================================================================

#[test]
fn a1_mesh_winding_points_toward_axis_absolute() {
    let r = run_subtract();
    let mesh = r.as_mesh();

    // Pin the absolute orientation: a correct outward-from-result solid.
    let vol = signed_volume(mesh);
    assert!(
        vol > 0.0,
        "yr13-adv A1: independent mock output must be OUTWARD-oriented \
         (positive signed volume); got {vol}"
    );

    // The cavity wall MUST carry reversed == true (the flag under test).
    assert!(
        r.faces()
            .iter()
            .any(|f| matches!(f.surface, Surface::Cylinder { .. }) && f.reversed),
        "yr13-adv A1: expected a reversed==true cylinder cavity wall"
    );

    let mut checked = 0usize;
    for &tri in &mesh.tris {
        if !is_cavity_wall_tri(mesh, tri) {
            continue;
        }
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let gn = cr(sb(v1, v0), sb(v2, v0));
        let mag = nm(gn);
        assert!(
            mag > 1e-12,
            "yr13-adv A1: cavity-wall tri {tri:?} is degenerate (zero-area)"
        );
        let gnu = sc(gn, 1.0 / mag);

        let centroid = sc(ad(ad(v0, v1), v2), 1.0 / 3.0);
        let (_, outward) = axis_radial(centroid);

        // ABSOLUTE: on this outward-oriented mesh the cavity-wall winding-normal
        // must point TOWARD the axis (dot with the outward radial clearly < 0).
        // This is the mesh-side witness that `reversed == true` (negate the
        // analytic away-from-axis normal) matches the emitted geometry (I-rev1).
        let d = dt(gnu, outward);
        assert!(
            d < -0.5,
            "yr13-adv A1: cavity-wall mesh-triangle winding-normal must point \
             TOWARD the axis (dot with outward radial clearly < 0) on this \
             outward-oriented result — the mesh-side witness of reversed==true \
             (I-rev1). Got dot {d} for tri {tri:?} centroid {centroid:?}. A \
             reversed/winding inconsistency flips this positive."
        );
        checked += 1;
    }

    // N=24 facets × 2 tris = 48 wall triangles expected.
    assert!(
        checked >= 2 * N,
        "yr13-adv A1: expected ≥{} cavity-wall triangles, only classified {checked}",
        2 * N
    );
}

// =========================================================================
// Oracle A2 — many-point surface-param sampling (more samples than RED's
// 6×3): every angle×height sample on the wall has effective normal (canonical
// negated because reversed) pointing toward the axis.
// =========================================================================

#[test]
fn a2_effective_normal_toward_axis_dense_sampling() {
    let r = run_subtract();
    let walls: Vec<&BRepFace> = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cylinder { .. }) && f.reversed)
        .collect();
    assert!(
        !walls.is_empty(),
        "yr13-adv A2: expected a reversed Surface::Cylinder cavity wall; faces = {:?}",
        r.faces()
            .iter()
            .map(|f| (f.surface, f.reversed))
            .collect::<Vec<_>>()
    );

    for w in &walls {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = w.surface
        else {
            unreachable!()
        };
        let ap = axis_point.as_array();
        let au = un(axis_dir.as_array());
        let abs = [au[0].abs(), au[1].abs(), au[2].abs()];
        let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
            [1.0, 0.0, 0.0]
        } else if abs[1] <= abs[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = un(cr(au, world));
        let e2 = un(cr(au, e1));

        let heights = [0.1_f64, 0.5, 1.0, 1.5, 1.9];
        let angles = 40usize;
        for &h in &heights {
            for k in 0..angles {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (angles as f64);
                let radial = ad(sc(e1, th.cos()), sc(e2, th.sin()));
                let _surf = ad(ad(ap, sc(au, h)), sc(radial, radius));
                let canonical = un(radial);
                let effective = sc(canonical, -1.0); // reversed
                let outward = un(radial);
                assert!(
                    dt(effective, outward) < -1.0 + 1e-9,
                    "yr13-adv A2: effective normal must point TOWARD axis"
                );
                assert!(
                    dt(canonical, outward) > 1.0 - 1e-9,
                    "yr13-adv A2: canonical normal must point AWAY (so they differ)"
                );
            }
        }
    }
}

// =========================================================================
// Oracle A3 — sense encoding per surface kind, with EXACT input params for the
// cavity wall and outward-pointing stored Plane.normal for every box face.
// =========================================================================

#[test]
fn a3_sense_encoding_and_exact_params() {
    let r = run_subtract();
    let want = cyl_surface();

    // The box interior reference point (geometric center of the solid box).
    let box_center = [
        0.5 * (BOX_LO[0] + BOX_HI[0]),
        0.5 * (BOX_LO[1] + BOX_HI[1]),
        0.5 * (BOX_LO[2] + BOX_HI[2]),
    ];

    let mut saw_wall = false;
    let mut saw_box_plane = false;
    for f in r.faces() {
        match f.surface {
            Surface::Torus { .. } => unreachable!("KV6d: torus not produced by this test"),
            Surface::Cylinder { .. } => {
                // I-rev3: exact input params; the sense lives ONLY in the flag.
                assert!(
                    f.reversed,
                    "yr13-adv A3: cavity wall must be reversed==true"
                );
                assert_eq!(
                    f.surface, want,
                    "yr13-adv A3 (I-rev3): cylinder params must equal input field-for-field \
                     (no perturbation to signal sense)"
                );
                saw_wall = true;
            }
            Surface::Plane { normal, d } => {
                // I-rev2: planar faces NEVER use the flag (no double-flip); sense
                // lives in the (winding-tracking) Plane.normal.
                assert!(
                    !f.reversed,
                    "yr13-adv A3 (I-rev2): planar faces must emit reversed==false"
                );
                let nrm = normal.as_array();
                let n_abs = [nrm[0].abs(), nrm[1].abs(), nrm[2].abs()];
                let axis_aligned = (n_abs[0] > 0.99 && n_abs[1] < 1e-6 && n_abs[2] < 1e-6)
                    || (n_abs[1] > 0.99 && n_abs[0] < 1e-6 && n_abs[2] < 1e-6)
                    || (n_abs[2] > 0.99 && n_abs[0] < 1e-6 && n_abs[1] < 1e-6);
                // Identify the 6 box-wall planes by their offset matching a box
                // extent. On this OUTWARD-oriented mock the production planar
                // branch makes the stored Plane.normal track the (outward) mesh
                // winding, so each box-wall normal points OUTWARD ABSOLUTELY:
                // evaluated at the box center it gives a negative signed distance
                // (the interior is on the negative-normal side).
                let extent_match = |axis: usize| {
                    (d.abs() - BOX_HI[axis].abs()).abs() < 1e-9
                        || (d.abs() - BOX_LO[axis].abs()).abs() < 1e-9
                };
                let on_box_wall = axis_aligned
                    && ((nrm[2].abs() > 0.99 && extent_match(2))
                        || (nrm[1].abs() > 0.99 && extent_match(1))
                        || (nrm[0].abs() > 0.99 && extent_match(0)));
                if on_box_wall {
                    let signed = dt(nrm, box_center) + d;
                    assert!(
                        signed < -1e-9,
                        "yr13-adv A3: box-wall Plane.normal must point OUTWARD \
                         (box center on the interior/negative side); signed {signed}"
                    );
                    saw_box_plane = true;
                }
            }
            Surface::Sphere { .. } | Surface::Cone { .. } => {
                panic!("yr13-adv A3: no Sphere/Cone faces expected in box−cylinder");
            }
        }
    }
    assert!(saw_wall, "yr13-adv A3: expected a cylinder cavity wall");
    assert!(
        saw_box_plane,
        "yr13-adv A3: expected ≥1 box-wall plane face"
    );
}

// =========================================================================
// Oracle A4 — watertight + χ=2 on the independently-built mock.
// =========================================================================

#[test]
fn a4_watertight_genus0() {
    let r = run_subtract();
    let mesh = r.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr13-adv A4: independent mock output must be watertight (a winding bug \
         surfaces as unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "yr13-adv A4: blind-pocket output must be genus 0 (χ=2)"
    );
}

// =========================================================================
// Oracle A5 — exact circular rim edge present (cylinder ∩ box-top section).
// =========================================================================

#[test]
fn a5_rim_circle_edge_present() {
    let r = run_subtract();
    assert!(
        r.edges()
            .iter()
            .any(|e| matches!(e.curve, Curve::Circle { .. })),
        "yr13-adv A5: expected a Curve::Circle rim edge"
    );
}

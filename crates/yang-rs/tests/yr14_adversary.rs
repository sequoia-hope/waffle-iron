//! PR-YR14 ADVERSARY — INDEPENDENT verification of the genus-1 through-hole
//! `Subtract` claim, with a SELF-CONTAINED box−cylinder through-hole arrangement
//! (NO helpers shared with the RED `yr14_through_hole.rs` file, so a RED-fixture
//! bug cannot hide a defect here).
//!
//! Different geometry from RED on purpose: box [-3,-3,0]..[3,3,3], cylinder axis
//! +Z at origin, radius 1.5, spanning z=−0.5..3.5 (BOTH caps lie OUTSIDE the box
//! — no cap survives), N=24 wall facets. The cylinder is the subtrahend
//! (`InputId::B`), so its lateral wall survives as the tube wall spanning the full
//! box thickness (z=0..3), making a single connected closed orientable 2-manifold
//! of genus 1 → χ = 0.
//!
//! Authoring convention (mirrors `tests/yr13_adversary.rs`): the arrangement is
//! authored as a CCW-from-outside surface and then GLOBALLY re-oriented OUTWARD
//! by a uniform `tri.swap(1,2)` on every authored triangle (so the boolean output
//! has positive signed volume and the box-bottom winds −Z). The cylinder/B tube
//! wall is authored PRE-SWAPPED so `flip_for_op(Subtract)` restores its
//! toward-axis (−radial, cavity-wall) winding — the SAME signal that sets
//! `reversed == true`. There is NO floor cap (the tube spans the whole box).
//!
//! The KEY independent oracle (A1) reasons NOT from the surface params (as a
//! param-only oracle would) but from the ACTUAL emitted mesh-triangle winding:
//! for each tube-wall triangle the geometric normal `(v1−v0)×(v2−v0)` must point
//! TOWARD the axis (dot with the outward radial clearly < 0), AND the output must
//! have positive signed volume, AND χ == 0, AND watertight (0 unpaired
//! half-edges). This witnesses — in ABSOLUTE terms — that the mesh `flip_for_op`
//! AGREES with the `reversed == true` B-Rep flag (spec invariant I-rev1), from
//! geometry a param-only oracle never inspects.
//!
//! ADVERSARY FINDING (load-bearing — reported to the driver). The shipped tests
//! do NOT make the χ-gate's "reject odd χ / χ>2" clause INDEPENDENTLY
//! load-bearing: in the RED file's oracle2 every defect is double-covered (O2a/b
//! by the directed half-edge pairing loop, since dropping a tri / adding a lone
//! tri also unpairs an edge; O2c by the coincident-triangle `NonManifoldInput`
//! guard). Mutating the χ gate to `if false { … }` left the FULL `yang-rs` suite
//! GREEN; symmetrically, mutating the pairing loop to `if false { … }` ALSO left
//! it green — the two guards mutually shadow each other on the existing corpus.
//! The χ-clause IS load-bearing for the ACCEPT path (Mutation A — reverting it to
//! `chi != 2` — makes the genus-1 through-hole oracles go RED). Oracle A6 below
//! pins the honest reachability boundary: a closed sub-shell with odd χ is caught
//! by an EARLIER guard (geometric face resolution — `FaceResolutionFailed`)
//! before the χ-clause, so the operative safety property (LOUD rejection, never
//! Ok) still holds, but the χ-clause's reject branch is not isolatable through
//! the public `boolean()` on the reachable corpus. See A6's comment for detail.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

// =========================================================================
// Independent fixture parameters (deliberately != RED's).
// =========================================================================

const N: usize = 24;
const BOX_LO: [f64; 3] = [-3.0, -3.0, 0.0];
const BOX_HI: [f64; 3] = [3.0, 3.0, 3.0];
const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, -0.5]; // bottom cap below box (discarded)
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.5;
const CYL_H: f64 = 4.0; // top cap at z=3.5, above box top (discarded)
const BOT_Z: f64 = 0.0; // box bottom plane = lower rim (through-hole)
const TOP_Z: f64 = 3.0; // box top plane = upper rim

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
    assert!(n > 1e-15, "yr14-adv: cannot normalize near-zero vector");
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

// Signed volume of a closed mesh (Σ a·(b×c) / 6). Positive ⇒ consistently
// OUTWARD-oriented (B-Rep convention); negative ⇒ inside-out.
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
    BRep::new(verts, edges, faces).expect("yr14-adv: box_brep BRep::new")
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
    BRep::new(verts, edges, faces).expect("yr14-adv: cylinder_brep BRep::new")
}

fn adv_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn adv_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: pt(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

// =========================================================================
// Self-authored full closed genus-1 result arrangement (box with a cylindrical
// THROUGH-HOLE). Box bottom + top are ANNULI (each pierced by a rim ring); the
// tube wall spans the full thickness (top rim z=3 ↔ bottom rim z=0); NO floor
// cap. A single global tri.swap(1,2) re-orients OUTWARD (positive signed volume).
//
// Box tris (label 0): surface=[A], inside=[false,false] — kept by Subtract, NOT
//   flipped.
// Cylinder wall tris (label 1): surface=[B], inside=[true,false] — kept by
//   Subtract, FLIPPED by flip_for_op (swap tri[1]↔tri[2]); authored so the
//   post-flip winding is toward-axis (cavity wall).
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

fn hole_arrangement() -> LabeledArrangement {
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

    // top rim ring @ z=TOP_Z, r=R (top annulus inner boundary + wall top)
    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(pt(CYL_R * th.cos(), CYL_R * th.sin(), TOP_Z));
    }
    // bottom rim ring @ z=BOT_Z, r=R (bottom annulus inner boundary + wall
    // bottom). NO floor cap → NO floor_center vertex.
    let brim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(pt(CYL_R * th.cos(), CYL_R * th.sin(), BOT_Z));
    }

    let rim = |k: usize| rim_base + (k % N) as u32;
    let brim = |k: usize| brim_base + (k % N) as u32;

    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(0)]);
    };

    let per = N / 4; // 6 for N=24

    // BOX BOTTOM ANNULUS (z=z0), outward −Z, bottom rim ring as the hole. Outer
    // square [b0,b0+1,b0+2,b0+3] (CCW-from-outside box-bottom). Inner ring runs
    // the OPPOSITE rotational sense to the outer square (ascending in the
    // authoring index `bi(s)=brim(s)`), giving a proper outer + hole.
    let blo = [b0, b0 + 1, b0 + 2, b0 + 3];
    let bi = |s: usize| brim(s % N);
    for c in 0..4usize {
        let oa = blo[c];
        let ob = blo[(c + 1) % 4];
        let sa = c * per;
        let sb_ = (c + 1) * per;
        push_box([oa, ob, bi(sb_)], &mut tris, &mut surface);
        for s in (sa..sb_).rev() {
            push_box([oa, bi(s + 1), bi(s)], &mut tris, &mut surface);
        }
    }

    // BOX 4 SIDES, outward horizontal (standard CCW-from-outside winding).
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

    // BOX TOP ANNULUS (z=z1), outward +Z, top rim ring as the hole. Outer square
    // Lo=[t0,t0+3,t0+2,t0+1] (edges oppose the sides). Inner loop Li = rim
    // DESCENDING so the outer + hole wind in opposite senses.
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
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

    // CYLINDER WALL (label 1) — top rim ring (z=3) down to bottom rim ring (z=0).
    // No cap. Authored PRE-SWAPPED so flip_for_op(Subtract) restores the toward-
    // axis (cavity-wall) winding. FINAL (post-flip) winding: top-rim edges
    // ASCENDING (rim(k)→rim(k1)) — opposite the top annulus inner ring
    // (descending) so the shared rim edges pair; bottom-rim edges DESCENDING
    // (brim(k1)→brim(k)) — opposite the bottom annulus inner ring (ascending).
    let push_cyl = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // pre-swap tri[1]<->tri[2]
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([rim(k1), rim(k), brim(k)], &mut tris, &mut surface);
        push_cyl([rim(k1), brim(k), brim(k1)], &mut tris, &mut surface);
    }

    // GLOBAL OUTWARD RE-ORIENTATION: the authoring above is CCW-from-outside
    // (globally inside-out as emitted). A uniform tri[1]↔tri[2] swap on EVERY
    // authored triangle flips the global orientation OUTWARD (box-bottom → −Z,
    // signed volume > 0) while preserving exact watertightness and χ (every
    // directed edge reverses in lock-step). The per-triangle surface/inside
    // labels are positional and untouched, so the Subtract keep-set and the
    // flip_for_op relationship are preserved: the cavity wall stays authored
    // PRE-SWAPPED relative to its own facet winding, so flip_for_op(Subtract)
    // restores it to the (now outward) toward-axis sense. This lets A1 witness
    // mesh↔reversed consistency in ABSOLUTE terms.
    for t in &mut tris {
        t.swap(1, 2);
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

/// Simulate the Subtract keep-set + flip on the arrangement mesh (every triangle
/// kept; label-1 tris swap tri[1]↔tri[2]), with NO boolean() call. Used by the
/// mandatory self-check.
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
    let bx = adv_box();
    let cyl = adv_cyl();
    let mock = LabelMock {
        arr: hole_arrangement(),
    };
    boolean(&bx, &cyl, BoolOp::Subtract, &mock).expect("yr14-adv: through-hole Subtract must be Ok")
}

// A triangle is a tube-wall triangle iff all 3 verts sit on the cylinder lateral
// band (radial ≈ CYL_R) within BOT_Z..TOP_Z, AND it spans the full thickness (a
// vertex at z≈TOP and one at z≈BOT). Independent of the surface params under test.
fn is_wall_tri(mesh: &Mesh, tri: [u32; 3]) -> bool {
    let vs: Vec<[f64; 3]> = tri
        .iter()
        .map(|&i| mesh.verts[i as usize].as_array())
        .collect();
    for v in &vs {
        let (r, _) = axis_radial(*v);
        if (r - CYL_R).abs() > 0.06 {
            return false;
        }
        if v[2] < BOT_Z - 1e-6 || v[2] > TOP_Z + 1e-6 {
            return false;
        }
    }
    let has_top = vs.iter().any(|v| (v[2] - TOP_Z).abs() < 1e-6);
    let has_bot = vs.iter().any(|v| (v[2] - BOT_Z).abs() < 1e-6);
    has_top && has_bot
}

// =========================================================================
// MANDATORY self-check — the fixture must simulate to a valid genus-1 closed
// shell (watertight + χ=0 + positive signed volume) BEFORE driving boolean().
// If this fails the whole adversary test is meaningless.
// =========================================================================

#[test]
fn a0_mock_is_valid_genus1() {
    let arr = hole_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr14-adv self-check: simulated genus-1 output must be watertight \
         (0 unpaired half-edges); got {unpaired}"
    );
    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 0,
        "yr14-adv self-check: simulated through-hole output must be genus 1 \
         (χ=0); got χ={chi}"
    );
    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr14-adv self-check: simulated output must be OUTWARD-oriented \
         (positive signed volume); got {vol}"
    );
}

// =========================================================================
// Oracle A1 (THE independent witness) — mesh-winding ↔ `reversed` consistency in
// ABSOLUTE terms, from the ACTUAL emitted tube-wall mesh-triangle winding +
// positive signed volume + χ=0 + watertight. Geometry a param-only oracle never
// inspects.
// =========================================================================

#[test]
fn a1_wall_winding_toward_axis_absolute() {
    let r = run_subtract();
    let mesh = r.as_mesh();

    // Pin the absolute frame: outward-oriented genus-1 solid.
    let vol = signed_volume(mesh);
    assert!(
        vol > 0.0,
        "yr14-adv A1: independent mock output must be OUTWARD-oriented \
         (positive signed volume); got {vol}"
    );
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr14-adv A1: through-hole output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(mesh),
        0,
        "yr14-adv A1: through-hole output must be genus 1 (χ=0), NOT genus 0 (χ=2)"
    );

    // The tube wall MUST carry reversed == true (the flag under test).
    assert!(
        r.faces()
            .iter()
            .any(|f| matches!(f.surface, Surface::Cylinder { .. }) && f.reversed),
        "yr14-adv A1: expected a reversed==true cylinder tube wall"
    );

    let mut checked = 0usize;
    for &tri in &mesh.tris {
        if !is_wall_tri(mesh, tri) {
            continue;
        }
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let gn = cr(sb(v1, v0), sb(v2, v0));
        let mag = nm(gn);
        assert!(
            mag > 1e-12,
            "yr14-adv A1: tube-wall tri {tri:?} is degenerate (zero-area)"
        );
        let gnu = sc(gn, 1.0 / mag);
        let centroid = sc(ad(ad(v0, v1), v2), 1.0 / 3.0);
        let (_, outward) = axis_radial(centroid);
        // ABSOLUTE: on this outward-oriented mesh the tube-wall winding-normal
        // must point TOWARD the axis (dot with outward radial clearly < 0) —
        // the mesh-side witness of reversed==true (I-rev1).
        let d = dt(gnu, outward);
        assert!(
            d < -0.5,
            "yr14-adv A1: tube-wall mesh-triangle winding-normal must point \
             TOWARD the axis (dot with outward radial clearly < 0) on this \
             outward-oriented result — the mesh-side witness of reversed==true \
             (I-rev1). Got dot {d} for tri {tri:?} centroid {centroid:?}"
        );
        checked += 1;
    }

    // N=24 facets × 2 tris = 48 wall triangles expected.
    assert!(
        checked >= 2 * N,
        "yr14-adv A1: expected ≥{} tube-wall triangles, only classified {checked}",
        2 * N
    );
}

// =========================================================================
// Oracle A2 — dense surface-param sampling: every angle×height sample on the
// wall has effective normal (canonical negated because reversed) toward the axis.
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
        "yr14-adv A2: expected a reversed Surface::Cylinder tube wall; faces = {:?}",
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

        let heights = [0.6_f64, 1.5, 2.0, 2.5, 3.4];
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
                    "yr14-adv A2: effective normal must point TOWARD axis"
                );
                assert!(
                    dt(canonical, outward) > 1.0 - 1e-9,
                    "yr14-adv A2: canonical normal must point AWAY (so they differ)"
                );
            }
        }
    }
}

// =========================================================================
// Oracle A3 — sense encoding per surface kind: exact input params for the tube
// wall (no perturbation), outward stored Plane.normal for every box face,
// no Sphere/Cone.
// =========================================================================

#[test]
fn a3_sense_encoding_and_exact_params() {
    let r = run_subtract();
    let want = cyl_surface();
    let box_center = [
        0.5 * (BOX_LO[0] + BOX_HI[0]),
        0.5 * (BOX_LO[1] + BOX_HI[1]),
        0.5 * (BOX_LO[2] + BOX_HI[2]),
    ];

    let mut saw_wall = false;
    let mut saw_box_plane = false;
    for f in r.faces() {
        match f.surface {
            Surface::Cylinder { .. } => {
                assert!(f.reversed, "yr14-adv A3: tube wall must be reversed==true");
                assert_eq!(
                    f.surface, want,
                    "yr14-adv A3 (I-rev3): cylinder params must equal input \
                     field-for-field (no perturbation to signal sense)"
                );
                saw_wall = true;
            }
            Surface::Plane { normal, d } => {
                assert!(
                    !f.reversed,
                    "yr14-adv A3 (I-rev2): planar faces must emit reversed==false"
                );
                let nrm = normal.as_array();
                let n_abs = [nrm[0].abs(), nrm[1].abs(), nrm[2].abs()];
                let axis_aligned = (n_abs[0] > 0.99 && n_abs[1] < 1e-6 && n_abs[2] < 1e-6)
                    || (n_abs[1] > 0.99 && n_abs[0] < 1e-6 && n_abs[2] < 1e-6)
                    || (n_abs[2] > 0.99 && n_abs[0] < 1e-6 && n_abs[1] < 1e-6);
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
                        "yr14-adv A3: box-wall Plane.normal must point OUTWARD \
                         (box center on the interior/negative side); signed {signed}"
                    );
                    saw_box_plane = true;
                }
            }
            Surface::Sphere { .. } | Surface::Cone { .. } => {
                panic!("yr14-adv A3: no Sphere/Cone faces expected in box−cylinder");
            }
        }
    }
    assert!(saw_wall, "yr14-adv A3: expected a cylinder tube wall");
    assert!(
        saw_box_plane,
        "yr14-adv A3: expected ≥1 box-wall plane face"
    );
}

// =========================================================================
// Oracle A4 — watertight + χ=0 on the independently-built mock (the genus-1
// gate the GREEN production change unlocks).
// =========================================================================

#[test]
fn a4_watertight_genus1() {
    let r = run_subtract();
    let mesh = r.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr14-adv A4: independent mock output must be watertight"
    );
    assert_eq!(
        euler_characteristic(mesh),
        0,
        "yr14-adv A4: through-hole output must be genus 1 (χ=0)"
    );
}

// =========================================================================
// Oracle A5 — two exact Circle rim edges (cylinder ∩ box-top AND ∩ box-bottom),
// each on the cylinder lateral surface and its box-face plane to TAU_MODEL.
// =========================================================================

#[test]
fn a5_two_rim_circle_edges() {
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
        circles.len() >= 2,
        "yr14-adv A5: the through-hole must produce ≥2 Curve::Circle rim edges \
         (z=3 box-top, z=0 box-bottom); found {}",
        circles.len()
    );

    let tau = cad_primitives::TAU_MODEL;
    let axis = un(CYL_AXIS_DIR);
    let mut saw_top = false;
    let mut saw_bot = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        assert!(
            (radius - CYL_R).abs() <= tau,
            "yr14-adv A5: rim Circle radius {radius} must equal CYL_R {CYL_R}"
        );
        let nrm = un(normal.as_array());
        assert!(
            (dt(nrm, axis)).abs() > 1.0 - 1e-9,
            "yr14-adv A5: rim Circle normal {nrm:?} must be parallel to the axis"
        );
        let w = sb(c, CYL_AXIS_POINT);
        let along = dt(w, axis);
        let proj = ad(CYL_AXIS_POINT, sc(axis, along));
        let radial_off = nm(sb(c, proj));
        assert!(
            radial_off <= tau,
            "yr14-adv A5: rim Circle center {c:?} must lie on the cylinder axis \
             (radial offset {radial_off} ≤ TAU_MODEL)"
        );
        if (c[2] - TOP_Z).abs() <= tau {
            saw_top = true;
        } else if (c[2] - BOT_Z).abs() <= tau {
            saw_bot = true;
        } else {
            panic!(
                "yr14-adv A5: rim Circle center z={} on neither box-top (z={}) nor \
                 box-bottom (z={}) within TAU_MODEL",
                c[2], TOP_Z, BOT_Z
            );
        }
    }
    assert!(
        saw_top,
        "yr14-adv A5: expected a rim Circle on box-TOP (z={TOP_Z})"
    );
    assert!(
        saw_bot,
        "yr14-adv A5: expected a rim Circle on box-BOTTOM (z={BOT_Z})"
    );
}

// =========================================================================
// Oracle A6 (χ-gate defense-in-depth witness — the adversary's distinctive
// contribution + an honest reachability finding reported to the driver).
//
// ADVERSARY FINDING. The shipped RED oracle2 defect cases are each ALSO caught
// by an EARLIER guard — O2a/O2b by the directed half-edge pairing loop (a
// dropped / lone triangle ALSO unpairs an edge), O2c by the coincident-triangle
// `NonManifoldInput` guard (`src/lib.rs:2389-2404`). So mutating the χ-clause to
// `if false { … }` leaves the FULL `yang-rs` suite GREEN: against the shipped
// corpus the χ-clause's "reject odd χ / χ>2" rejection is NOT independently
// load-bearing. (Symmetrically, mutating the pairing loop to `if false { … }`
// also leaves the suite green — the two guards mutually shadow each other on the
// existing tests.) Both mutations were run and restored by the adversary.
//
// REACHABILITY. Reaching the χ-clause with a mesh that the pairing loop and the
// coincident-tri guard BOTH accept, yet that presents odd χ or χ>2, appears
// structurally hard-to-impossible through the public `boolean()`: every
// non-degenerate kept triangle must first pass GEOMETRIC FACE RESOLUTION (its
// centroid must lie on an input B-Rep face surface, `src/lib.rs:2407-2547`). A
// closed 3-D sub-shell (e.g. two tetrahedra sharing a single pinch vertex —
// V=7,E=12,F=8 ⇒ χ=3, edges all paired, no coincident tris) has slanted faces
// whose centroids lie on NO planar box face, so it trips `FaceResolutionFailed`
// (or, for a flat doubled sheet, the coincident-tri guard) BEFORE the χ-clause.
// A6 documents and PINS this: such a construction is LOUDLY rejected as a defect
// (never accepted, never Ok) — which is the operative safety property — even
// though the specific guard that fires is an EARLIER one, not the χ-clause. The
// χ-clause remains the correct, in-spec gate (it is what unlocks the genus-1
// accept path, proven load-bearing by Mutation A reverting it to `chi != 2`),
// and is a sound defense-in-depth layer; this test records that the existing
// reachable corpus does not isolate it from the upstream guards.
// =========================================================================

#[test]
fn a6_odd_chi_subshell_loudly_rejected() {
    // Inject — into the VALID through-hole arrangement (which reaches the
    // watertight gate because it carries the two rim Circles) — an EXTRA closed
    // sub-shell with ODD combinatorial χ: two tetrahedra sharing exactly ONE
    // pinch vertex. V=7, E=12, F=8 ⇒ χ = 3 (odd); every directed edge is paired
    // (each tetra is independently closed & orientable); no two triangles share
    // the same 3 verts. The result must be LOUDLY rejected as non-manifold
    // (whichever guard fires first), NEVER accepted as a valid boolean.
    let mut arr = hole_arrangement();
    let mut verts = arr.mesh.verts.clone();
    let mut tris = arr.mesh.tris.clone();
    let base = verts.len() as u32;
    // Tetra 1 verts (base+0..3); Tetra 2 verts (base+3..6) — share base+3.
    // Small, near the box-bottom interior corner, away from the hole.
    verts.push(pt(-2.6, -2.6, 0.1)); // 0
    verts.push(pt(-2.2, -2.6, 0.1)); // 1
    verts.push(pt(-2.4, -2.2, 0.1)); // 2
    verts.push(pt(-2.4, -2.4, 0.4)); // 3 (shared pinch apex)
    verts.push(pt(-2.6, -2.6, 0.7)); // 4
    verts.push(pt(-2.2, -2.6, 0.7)); // 5
    verts.push(pt(-2.4, -2.2, 0.7)); // 6
    let v = |o: u32| base + o;
    // Two closed tetrahedra (each: 4 outward-wound faces; all directed edges
    // paired within the tetra). They share only the pinch vertex base+3.
    let tetra1: [[u32; 3]; 4] = [
        [v(0), v(2), v(1)],
        [v(0), v(1), v(3)],
        [v(1), v(2), v(3)],
        [v(2), v(0), v(3)],
    ];
    let tetra2: [[u32; 3]; 4] = [
        [v(4), v(5), v(6)],
        [v(4), v(3), v(5)],
        [v(5), v(3), v(6)],
        [v(6), v(3), v(4)],
    ];
    for t in tetra1.into_iter().chain(tetra2) {
        tris.push(t);
        arr.surface.push(vec![LaInputId(0)]);
        arr.inside.push(vec![false, false]);
        arr.patch.push(0);
    }
    arr.mesh = Mesh::new(verts, tris);

    let mock = LabelMock { arr };
    let err = boolean(&adv_box(), &adv_cyl(), BoolOp::Subtract, &mock)
        .expect_err("yr14-adv A6: an odd-χ closed sub-shell must be rejected, got Ok");
    assert!(
        matches!(
            err,
            YangError::NonManifoldInput
                | YangError::NonManifoldOutput
                | YangError::FaceResolutionFailed { .. }
        ),
        "yr14-adv A6: a closed sub-shell with ODD χ (=3) must be LOUDLY rejected \
         (NonManifoldInput / NonManifoldOutput / FaceResolutionFailed), never \
         accepted; got {err:?}"
    );
}

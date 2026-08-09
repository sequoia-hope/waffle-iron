//! M8 slice g — disc / annular / mixed faces in n-ary plane groups (task
//! #132, spec `specs/m8_nary_tessellated_faces.md`). RED oracles.
//!
//! Driver: assay case R0046 — a subtract whose Stage-0 multi-pair plane
//! group holds two MIXED cap pieces (side A) and a DISC (side B). Slice f
//! walls any such group at `nary-face-unsupported`; this slice wires the
//! 1×1 tessellated machinery (exact rim rings, on-circle chord mints,
//! lateral crossing propagation) per face of the group.
//!
//! Canonical fixture (R0046's shape with a CYLINDER tool, so no torus
//! lateral is involved): cylinder r=2 h=2, minus a channel box through the
//! top (splitting the cap into two chord+arc segment pieces), minus a flush
//! coaxial cylinder r=1 whose top cap lands exactly on the cap plane —
//! the final subtract's group = {mixed, mixed} × {disc}.

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

// ════════════════════════════════════════════════════════════════════
// fixtures (yr24/yr26 conventions, shared with m8_disc_coplanar.rs)
// ════════════════════════════════════════════════════════════════════

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box B-Rep [lo, hi] (8 verts / 24 edges / 6 quad faces).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex { point: p(x, y, z) };
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(s, e)| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("valid box B-Rep")
}

/// Upright cylinder (axis +z), seam at +x (m8_disc_coplanar convention).
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let bottom = [cx, cy, base_z];
    let top = [cx, cy, base_z + height];
    let v0 = p(cx + radius, cy, base_z);
    let v1 = p(cx + radius, cy, base_z + height);
    let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom[0], bottom[1], bottom[2]),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top[0], top[1], top[2]),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(bottom[0], bottom[1], bottom[2]),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: base_z,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -(base_z + height),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("z_cylinder BRep::new")
}

/// TWO disjoint upright cylinders (radius `r`, height `h`, centers at
/// (cx1, 0) and (cx2, 0), base z=0) as ONE two-lump B-Rep — the z_cylinder
/// topology duplicated with index offsets.
fn two_z_cylinders(cx1: f64, cx2: f64, r: f64, h: f64) -> BRep {
    let mut verts: Vec<BRepVertex> = Vec::new();
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    for cx in [cx1, cx2] {
        let vo = verts.len() as u32;
        let eo = edges.len() as u32;
        verts.push(BRepVertex {
            point: p(cx + r, 0.0, 0.0),
        });
        verts.push(BRepVertex {
            point: p(cx + r, 0.0, h),
        });
        edges.push(BRepEdge {
            start: vo,
            end: vo,
            curve: Curve::Circle {
                center: p(cx, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        });
        edges.push(BRepEdge {
            start: vo + 1,
            end: vo + 1,
            curve: Curve::Circle {
                center: p(cx, 0.0, h),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        });
        edges.push(BRepEdge {
            start: vo,
            end: vo + 1,
            curve: Curve::LineSegment,
        });
        faces.push(BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(cx, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![eo, eo + 2, eo + 1, eo + 2],
            inner_loops: Vec::new(),
            reversed: false,
        });
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![eo],
            inner_loops: Vec::new(),
            reversed: false,
        });
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -h,
            },
            outer_loop: vec![eo + 1],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("two_z_cylinders BRep::new")
}

// ════════════════════════════════════════════════════════════════════
// mesh oracles (yr26 pattern)
// ════════════════════════════════════════════════════════════════════

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                + a.z() * (b.x() * c.y() - b.y() * c.x()))
                / 6.0
        })
        .sum()
}

fn edge_stats(mesh: &Mesh) -> BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> {
    let key = |v: u32| {
        let p = mesh.verts[v as usize];
        [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
    };
    let mut m: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
            let (lo, hi, dir) = if a <= b { (a, b, 1) } else { (b, a, -1) };
            let e = m.entry((lo, hi)).or_insert((0, 0));
            e.0 += 1;
            e.1 += dir;
        }
    }
    m
}

fn assert_watertight(mesh: &Mesh, what: &str) {
    assert!(!mesh.tris.is_empty(), "{what}: output must be non-empty");
    for (edge, (count, balance)) in edge_stats(mesh) {
        assert_eq!(
            count, 2,
            "{what}: edge {edge:?} must have exactly 2 incident tris"
        );
        assert_eq!(balance, 0, "{what}: edge {edge:?} once per direction");
    }
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::BTreeSet;
    let mut vs: BTreeSet<[u64; 3]> = BTreeSet::new();
    for t in &mesh.tris {
        for &v in t {
            let p = mesh.verts[v as usize];
            vs.insert([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]);
        }
    }
    vs.len() as i64 - edge_stats(mesh).len() as i64 + mesh.tris.len() as i64
}

fn run(a: &BRep, b: &BRep, op: BoolOp, what: &str) -> BRep {
    let nb = yang_rs::native_backend().expect("native backend always available");
    match boolean(a, b, op, &nb) {
        Ok(out) => out,
        Err(e) => panic!("{what}: boolean() failed: {e}"),
    }
}

/// Closed genus-0 solid oracle. `vol` is the analytic target; a boolean
/// output's planar/cylindrical faces are tessellated with chord sag, so the
/// mesh volume approaches `vol` only in the chord count — assert within
/// `rel` instead of exactly.
#[allow(dead_code)] // retained oracle: n-ary closed-solid cases will want it
fn assert_closed(out: &BRep, vol: f64, rel: f64, what: &str) {
    let mesh = out.as_mesh();
    assert_watertight(mesh, what);
    assert_eq!(euler_characteristic(mesh), 2, "{what}: χ must be 2");
    let v = signed_volume(mesh);
    assert!(v > 0.0, "{what}: outward orientation (positive volume)");
    let tol = vol.abs() * rel;
    assert!(
        (v - vol).abs() <= tol,
        "{what}: volume {v} != expected {vol} (tol {tol})"
    );
}

// ════════════════════════════════════════════════════════════════════
// fixture chain: the pocketed cylinder (R0046's shape, cylinder tool)
// ════════════════════════════════════════════════════════════════════

const R_BIG: f64 = 2.0;
const H: f64 = 2.0;
const W: f64 = 0.5; // channel half-width
const R_TOOL: f64 = 1.0;
const POCKET_DEPTH: f64 = 0.5;

/// Area of the vertical strip |x| ≤ w intersected with the disc r.
fn strip_area(r: f64, w: f64) -> f64 {
    2.0 * w * (r * r - w * w).sqrt() + 2.0 * r * r * (w / r).asin()
}

/// Cylinder minus a FULL-HEIGHT channel: two half-moon lumps whose cap
/// plane z=H carries TWO mixed chord+arc segment faces (x ≥ W and x ≤ −W).
/// (A partial-depth channel leaves an interior floor whose arc chains hit a
/// PRE-EXISTING Stage-1 conformality gap — the plain non-coplanar subtract
/// of such an operand already fails NonManifoldOutput; out of slice-g
/// scope, see the task-#132 ledger.)
fn channel_cut_cylinder() -> BRep {
    let cyl = z_cylinder(0.0, 0.0, 0.0, R_BIG, H);
    let channel = box_brep([-W, -3.0, -1.0], [W, 3.0, H + 1.0]);
    run(&cyl, &channel, BoolOp::Subtract, "cyl − channel")
}

fn flush_tool() -> BRep {
    z_cylinder(0.0, 0.0, H - POCKET_DEPTH, R_TOOL, POCKET_DEPTH)
}

/// Mesh-volume of a chained yang output at the same chord resolution the
/// oracle mesh uses (`as_mesh`), for volume-delta targets that cancel the
/// tessellation sag of untouched geometry.
fn mesh_volume(brep: &BRep) -> f64 {
    signed_volume(brep.as_mesh())
}

// ════════════════════════════════════════════════════════════════════
// Oracle #1 — canonical subtract: flush coaxial pocket over two mixed
// cap pieces (the {mixed, mixed} × {disc} plane group).
// ════════════════════════════════════════════════════════════════════
#[test]
fn flush_pocket_subtract_and_union_partition() {
    let solid = channel_cut_cylinder();
    let vol_before = mesh_volume(&solid);

    // Subtract: removes the tool minus its slice already inside the
    // channel — Δ⁻ ≈ (π r² − strip_area(r, w)) · depth.
    let sub = run(&solid, &flush_tool(), BoolOp::Subtract, "flush pocket");
    let sub_mesh = sub.as_mesh();
    assert_watertight(sub_mesh, "flush pocket subtract");
    // The channel splits the solid into TWO lumps (two genus-0 shells).
    assert_eq!(
        euler_characteristic(sub_mesh),
        4,
        "flush pocket subtract: χ must be 4 (two lumps)"
    );
    let v_sub = signed_volume(sub_mesh);
    assert!(v_sub > 0.0, "flush pocket subtract: positive volume");
    let removed = vol_before - v_sub;

    // Union: the flush tool pokes into the channel void — genuinely adds
    // the strip slice, bridging the lumps into ONE arch (genus 0).
    let uni = run(&solid, &flush_tool(), BoolOp::Union, "flush tool union");
    let uni_mesh = uni.as_mesh();
    assert_watertight(uni_mesh, "flush tool union");
    assert_eq!(
        euler_characteristic(uni_mesh),
        2,
        "flush tool union: χ must be 2 (bridged arch)"
    );
    let added = signed_volume(uni_mesh) - vol_before;

    // Partition cross-oracle: removed + added is EXACTLY the tool region
    // as tessellated in the outputs — off the analytic π r² d only by the
    // tool rim's own chord sag (16-gon area deficit ≈ 2.6%; band 5%).
    let tool_vol = std::f64::consts::PI * R_TOOL * R_TOOL * POCKET_DEPTH;
    assert!(
        (removed + added - tool_vol).abs() <= tool_vol * 5.0e-2,
        "partition: removed {removed} + added {added} != tool ~{tool_vol}"
    );
    // Per-op sanity bands: the strip split amplifies rim sag (the chord at
    // the 16-gon's ±90° vertex sags ~0.076·r across the whole strip), so
    // the analytic strip areas hold only to ~10% at Stage-1 resolution.
    let d_sub = (std::f64::consts::PI * R_TOOL * R_TOOL - strip_area(R_TOOL, W)) * POCKET_DEPTH;
    let d_uni = strip_area(R_TOOL, W) * POCKET_DEPTH;
    assert!(
        (removed - d_sub).abs() <= d_sub * 0.15,
        "subtract: removed {removed}, analytic ~{d_sub}"
    );
    assert!(
        (added - d_uni).abs() <= d_uni * 0.15,
        "union: added {added}, analytic ~{d_uni}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #3 — sanity: the plain (channel-free) flush pocket is the
// already-supported 1×1 disc∩disc containment class and must keep
// working bit-for-bit through this slice (I9 regression canary at the
// e2e level; exact volume π(R² − r²·d/H… computed directly)).
// ════════════════════════════════════════════════════════════════════
#[test]
fn plain_flush_pocket_still_succeeds() {
    let cyl = z_cylinder(0.0, 0.0, 0.0, R_BIG, H);
    let vol_before = mesh_volume(&cyl);
    let out = run(&cyl, &flush_tool(), BoolOp::Subtract, "plain flush pocket");
    let delta = std::f64::consts::PI * R_TOOL * R_TOOL * POCKET_DEPTH;
    let mesh = out.as_mesh();
    assert_watertight(mesh, "plain flush pocket");
    let v = signed_volume(mesh);
    let got_delta = vol_before - v;
    assert!(
        (got_delta - delta).abs() <= delta * 6.0e-2,
        "plain flush pocket: removed {got_delta}, expected ~{delta}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #4 — disc×disc in a GROUP, one pair crossing rims + one pair
// containment. The 1×1 path already resolves crossing coplanar rims
// (m8_disc_coplanar::disc_disc_crossing_union_succeeds — cherchi's
// coplanar arrangement); the n-ary group must too. Two boss cylinders
// sunk INTO a base (no incidental z=0 pair); the tool's bottom disc is
// flush with both boss tops, crossing boss A's rim and containing
// boss B's.
// ════════════════════════════════════════════════════════════════════
#[test]
fn group_with_crossing_and_contained_rims_succeeds() {
    // Two disjoint pillars as ONE two-lump solid (KV7-F2: multi-lump
    // operands enter booleans), built DIRECTLY so the rims keep their exact
    // `Curve::Circle` vocabulary (a chained disjoint union re-emits rims as
    // segment polylines — a separate producer gap, not this slice).
    let s2 = two_z_cylinders(-1.5, 1.5, 1.0, 1.0);
    let vol_before = mesh_volume(&s2);
    // Tool bottom disc flush with both boss tops at z=1. ASYMMETRIC center
    // (the m8_disc_coplanar crossing-test convention — symmetric rims mint
    // mirrored sweep-event columns whose slivers RoundingCollapse). Boss A
    // rims cross: d(centers)≈1.71, radii 2.2/1.0 → 1.2 < d < 3.2 ✓. Boss B
    // rim strictly inside: d≈1.31, d + 1.0 < 2.2 ✓. Both pairs share the
    // tool bottom face → one 2-pair plane group with three DISC faces.
    let tool = z_cylinder(0.2, 0.13, 1.0, 2.2, 0.5);
    let out = run(&s2, &tool, BoolOp::Union, "flush tool over both bosses");
    let mesh = out.as_mesh();
    assert_watertight(mesh, "crossing+contained group union");
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "crossing+contained group union: χ must be 2"
    );
    // The tool sits entirely ON the z=1 plane (no interpenetration):
    // added volume = the full tool cylinder.
    let delta = std::f64::consts::PI * 2.2 * 2.2 * 0.5;
    let got_delta = signed_volume(mesh) - vol_before;
    assert!(
        (got_delta - delta).abs() <= delta * 6.0e-2,
        "crossing+contained group union: added {got_delta}, expected ~{delta}"
    );
}

// 1×1 canary: oracle #4's crossing geometry against a SINGLE boss runs the
// singleton-group (1×1) path — pins that the n-ary machinery did not
// regress the pairwise crossing class it generalizes (spec I9's e2e face).
#[test]
fn single_boss_crossing_1x1_regression() {
    let boss_a = z_cylinder(-1.5, 0.0, 0.0, 1.0, 1.0);
    let tool = z_cylinder(0.2, 0.13, 1.0, 2.2, 0.5);
    let out = run(&boss_a, &tool, BoolOp::Union, "single boss crossing 1x1");
    assert_watertight(out.as_mesh(), "single boss crossing 1x1");
}

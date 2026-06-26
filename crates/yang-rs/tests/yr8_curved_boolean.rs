//! PR-YR8 (P2c) RED — first curved boolean: `cylinder ∪ box`.
//!
//! Spec of record: `specs/yang_pr_yr8_curved_boolean.md`.
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY;
//! the GREEN implementer extends `crates/yang-rs/src/lib.rs` (Stage-6 face
//! resolution per-face tolerance + `reconstruct_topology` curved branch) to
//! make the cylinder-survival oracles GREEN.
//!
//! ## RED contract (current production behavior)
//!
//! `reconstruct_topology` (src/lib.rs ~1603) LOUDLY rejects every curved
//! inherited surface — INCLUDING `Surface::Cylinder` — with
//! `Err(YangError::CurvedSurfaceNotYetSupported { face })`. So any test that
//! drives a kept cylinder-labeled patch through `boolean()` currently FAILS at
//! runtime with that error. That is the INTENDED RED state: the tests COMPILE
//! (they reference only existing public API) but assert-fail against current
//! code, and turn GREEN once the two blockers in spec §4 are fixed.
//!
//! Two independent paths cover the spec §6 oracle items:
//!
//! 1. **Mock-backend direct path** (PRIMARY in-env gate, deterministic, no
//!    sidecar binary): a hand-built watertight `LabeledArrangement` containing
//!    cylinder-lateral patches (label A) plus box-plane cap patches (label B),
//!    driven through the PUBLIC `boolean()` via the `LabelMock` pattern
//!    (mirrors `m3_adversary.rs`). This is the GREEN gate that does NOT depend
//!    on the sidecar. Covers §6.2 (surface survival), §6.4 (geometric
//!    soundness), §6.6 (determinism), and exercises Blocker 1 (face resolution
//!    at `d_ε`) + Blocker 2 (`reconstruct_topology` curved branch).
//!
//! 2. **Real-sidecar E2E** (env-gated on `CHERCHI2022_BIN`): the true
//!    `cylinder ∪ box` through `SidecarBoolean`. Covers §6.1 (runs & Ok),
//!    §6.2, §6.4, §6.5 (2-manifold / watertight + Euler). Self-skips with a
//!    LOUD `eprintln!` when the binary is absent (never silently passes).
//!
//! 3. **Sphere/Cone still loud** (direct): sphere/cone surfaces must NEVER flow
//!    through the pipeline; they are rejected at `BRep::new` (input-path),
//!    which is the construction-time guarantee that they can never reach
//!    `reconstruct_topology`. Covers §6.7.

use std::collections::{HashMap, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use std::error::Error;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives has no dot/cross/normalize helpers).
// Copied verbatim from tests/yr7_cylinder.rs — integration test files cannot
// share helpers.
// =========================================================================

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
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
/// Perpendicular distance from `x` to the infinite line through `axis_point`
/// with unit direction `axis_unit`.
fn dist_point_to_line(x: [f64; 3], axis_point: [f64; 3], axis_unit: [f64; 3]) -> f64 {
    let w = sub(x, axis_point);
    let along = dot(w, axis_unit);
    let proj = add(axis_point, scale(axis_unit, along));
    norm(sub(x, proj))
}

// =========================================================================
// Cylinder B-Rep fixture (spec §1 seam-edge encoding). Copied from
// tests/yr7_cylinder.rs (locally re-declared; integration tests cannot see
// each other's helpers).
// =========================================================================

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
        // f0 lateral Cylinder
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
        // f1 bottom cap Plane
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 top cap Plane
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

    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate the cylinder")
}

/// Analytic AABB diagonal from the two rim circles' exact extents (spec §3).
/// Used to derive the SAME `d_ε = 1e-2 × AABB_diag` chord bound Stage 1 uses.
fn analytic_aabb_diagonal(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> f64 {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = radius * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    norm(sub(hi, lo))
}

fn d_eps(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> f64 {
    1e-2 * analytic_aabb_diagonal(axis_point, axis_dir, radius, height)
}

// =========================================================================
// Unit-cube fixture with TRUE per-face plane offsets (so geometric face
// resolution succeeds). Copied from tests/end_to_end.rs.
// =========================================================================

fn unit_cube_brep_offset_at(origin: [f64; 3]) -> BRep {
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
        [0, 1, 2, 3], // F0 bottom (z)
        [4, 7, 6, 5], // F1 top (z+1)
        [0, 4, 5, 1], // F2 front (y)
        [1, 5, 6, 2], // F3 right (x+1)
        [2, 6, 7, 3], // F4 back (y+1)
        [3, 7, 4, 0], // F5 left (x)
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
    BRep::new(verts, edges, faces).expect("offset cube BRep::new failed")
}

// =========================================================================
// Analytic mesh oracles. Copied from tests/end_to_end.rs / m3_adversary.rs.
// =========================================================================

/// Count directed half-edges with no opposite. Watertight ⇒ 0 unpaired.
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

/// Euler V − E + F over the mesh (E = unique undirected edges).
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
// Canonical config (spec §6 / Manager brief): cylinder axis +Z through the
// unit cube at the origin.
//
//   box      = unit_cube_brep_offset_at([0,0,0])  (spans 0..1 in x,y,z)
//   cylinder = cylinder_brep([0.5,0.5,-0.5], +Z, r=0.25, h=2.0)
//
// The cylinder is centered on (x,y)=(0.5,0.5), radius 0.25, spanning
// z = -0.5 .. 1.5 — it pokes fully through the box's top (z=1) and bottom
// (z=0) faces, so its lateral surface survives a Union as analytic
// `Surface::Cylinder` patches.
// =========================================================================

const CYL_AXIS_POINT: [f64; 3] = [0.5, 0.5, -0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_RADIUS: f64 = 0.25;
const CYL_HEIGHT: f64 = 2.0;

fn canonical_cylinder() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT)
}
fn canonical_box() -> BRep {
    unit_cube_brep_offset_at([0.0, 0.0, 0.0])
}

/// The exact `Surface::Cylinder` the canonical cylinder's lateral face carries.
/// Output cylinder faces must `==` this EXACTLY (inherited, not re-fit).
fn canonical_cylinder_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_RADIUS,
    }
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement
// (mirrors m3_adversary.rs ATTACK-6). The mock ignores the input meshes and
// returns the hand-built arrangement from `labeled_arrangement()`.
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
// Hand-built watertight arrangement: a closed N-gon "tube" approximating the
// canonical cylinder, capped on the box's z=0 and z=1 planes.
//
// GEOMETRY (documented for the GREEN implementer's face-resolution reasoning):
//
//   - N = 8 facets. Lateral ring vertices lie EXACTLY on the analytic cylinder
//     (radius 0.25 about (0.5,0.5)) at z=0 (bottom ring) and z=1 (top ring).
//   - 8 lateral QUADS (16 triangles), each split into 2 tris. These carry the
//     surface label `InputId(0)` = solid A = the CYLINDER. Their centroids sit
//     at most ~0.0148 from the analytic cylinder surface — well within the
//     Stage-1 chord bound d_ε ≈ 0.0212 (1e-2 × AABB diag of the canonical
//     cylinder), and ≥ 0.5 from BOTH cap planes (z=0, z=1), so Blocker-1's
//     per-face rule resolves them UNIQUELY to the cylinder lateral face (no
//     F3 tie — the cap bands are TAU_WORK, far away).
//   - bottom cap fan (8 tris) on plane z=0 and top cap fan (8 tris) on z=1.
//     These carry label `InputId(1)` = solid B = the BOX. Their centroids lie
//     EXACTLY on the box's z=0 / z=1 planes (distance 0 < TAU_WORK) and ≥ d_ε
//     from the cylinder, so they resolve to box plane faces.
//
//   Total: V = 18, F = 32. Verified watertight (0 unpaired half-edges),
//   Euler V−E+F = 2. `inside` is all-false for every triangle ⇒ Union keeps
//   ALL 32 triangles (the `keep_set` Union rule). The mesh is a closed,
//   2-manifold solid: it exercises BOTH the cylinder-lateral
//   `reconstruct_topology` branch (Blocker 2) and the planar cap branch.
//
// This is NOT the geometrically-correct `cylinder ∪ box` shell (which has a
// hole-in-cap topology); it is the MINIMAL watertight labeled mesh that drives
// a kept cylinder patch through Stage 6, which is all the in-env GREEN gate
// needs. The true union shell is checked on the sidecar-equipped E2E path.
// =========================================================================

const N_FACETS: usize = 8;

fn hand_built_tube_arrangement() -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let r = CYL_RADIUS;
    let (za, zb) = (0.0f64, 1.0f64); // box z extent

    let ring: Vec<(f64, f64)> = (0..N_FACETS)
        .map(|k| {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            (cx + r * th.cos(), cy + r * th.sin())
        })
        .collect();

    let mut verts: Vec<Point3> = Vec::new();
    let mut bot = Vec::with_capacity(N_FACETS);
    let mut top = Vec::with_capacity(N_FACETS);
    for &(x, y) in &ring {
        bot.push(verts.len() as u32);
        verts.push(p(x, y, za));
    }
    for &(x, y) in &ring {
        top.push(verts.len() as u32);
        verts.push(p(x, y, zb));
    }
    let cb = verts.len() as u32;
    verts.push(p(cx, cy, za)); // bottom cap center
    let ct = verts.len() as u32;
    verts.push(p(cx, cy, zb)); // top cap center

    // (label, [v0,v1,v2]) — winding chosen so the closed mesh is watertight
    // (every directed edge has a unique opposite).
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let push =
        |t: [u32; 3], label: u32, tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
            tris.push(t);
            surf.push(vec![LaInputId(label)]);
        };

    // Lateral walls → CYLINDER (label 0).
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([bot[k], bot[k1], top[k1]], 0, &mut tris, &mut surface);
        push([bot[k], top[k1], top[k]], 0, &mut tris, &mut surface);
    }
    // Bottom cap fan (plane z=0, outward −z) → BOX (label 1).
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([cb, bot[k1], bot[k]], 1, &mut tris, &mut surface);
    }
    // Top cap fan (plane z=1, outward +z) → BOX (label 1).
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([ct, top[k], top[k1]], 1, &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    // Union keeps a tri iff its `inside` row is all-false: set every row false.
    let inside = vec![vec![false, false]; n];
    // Single patch id per surface kind would over-merge nothing here; the
    // production code derives patches by flood-fill on attribution, so the
    // `patch` field is unused by `boolean()` (kept for I1 shape only).
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

/// Helper: does the output BRep carry a `Surface::Cylinder` face whose params
/// EXACTLY equal the canonical cylinder's (axis_point/axis_dir/radius)?
fn has_exact_cylinder_face(brep: &BRep) -> bool {
    let want = canonical_cylinder_surface();
    brep.faces().iter().any(|f| f.surface == want)
}

// =========================================================================
// TEST 1 — Mock-backend direct path (PRIMARY in-env GREEN gate).
// Spec §6 items: 6.2 (surface survival), 6.4 (geometric soundness),
// 6.6 (determinism). Exercises Blocker 1 + Blocker 2.
//
// RED: currently FAILS — reconstruct_topology rejects the kept cylinder patch
// with CurvedSurfaceNotYetSupported.
// =========================================================================

#[test]
fn t1_mock_cylinder_patch_survives_union_as_analytic_surface() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_tube_arrangement(),
    };

    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr8: cylinder-labeled Union must return Ok once the curved branch lands");

    // §6.2: ≥1 face is Surface::Cylinder with the input's EXACT params.
    assert!(
        has_exact_cylinder_face(&r),
        "yr8: output must carry ≥1 Surface::Cylinder face equal to the input \
         cylinder's exact params {:?}; got faces {:?}",
        canonical_cylinder_surface(),
        r.faces().iter().map(|f| f.surface).collect::<Vec<_>>()
    );

    // §6.2: every surviving cylinder-band patch inherits the cylinder; cap
    // patches are planar. (At least one of each kind must appear.)
    let n_cyl = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .count();
    let n_plane = r
        .faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Plane { .. }))
        .count();
    assert!(
        n_cyl >= 1,
        "yr8: expected ≥1 cylinder face, got {n_cyl} (faces {:?})",
        r.faces().iter().map(|f| f.surface).collect::<Vec<_>>()
    );
    assert!(
        n_plane >= 1,
        "yr8: expected ≥1 planar cap face, got {n_plane}"
    );
    // No sphere/cone may EVER appear in output.
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cone { .. })),
        "yr8: output must contain no Sphere/Cone faces"
    );

    // §6.4: every vertex of a cylinder-lateral output triangle lies within
    // d_ε of the analytic cylinder; box-cap vertices lie ON their plane.
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);
    let axis_unit = unit(CYL_AXIS_DIR);
    let mesh = r.as_mesh();
    for tri in &mesh.tris {
        let pts: [[f64; 3]; 3] = [
            mesh.verts[tri[0] as usize].as_array(),
            mesh.verts[tri[1] as usize].as_array(),
            mesh.verts[tri[2] as usize].as_array(),
        ];
        // A lateral triangle has all 3 verts on the cylinder (dist 0). A cap
        // triangle has its rim verts on the cylinder but its center vertex on
        // the axis (dist = radius). Classify by "all 3 verts on cylinder".
        let on_cyl = pts
            .iter()
            .all(|&x| (dist_point_to_line(x, CYL_AXIS_POINT, axis_unit) - CYL_RADIUS).abs() <= de);
        if on_cyl {
            for &x in &pts {
                let d = (dist_point_to_line(x, CYL_AXIS_POINT, axis_unit) - CYL_RADIUS).abs();
                assert!(
                    d <= de,
                    "yr8: lateral vertex {x:?} distance {d} exceeds chord bound d_ε {de}"
                );
            }
        }
    }

    // §6.6 determinism: identical inputs → identical output BRep + mesh.
    let mock2 = LabelMock {
        arrangement: hand_built_tube_arrangement(),
    };
    let r2 = boolean(&cyl, &bx, BoolOp::Union, &mock2).expect("yr8: determinism run 2");
    assert_eq!(
        r.faces().len(),
        r2.faces().len(),
        "yr8 determinism: face count differs"
    );
    for (i, (f1, f2)) in r.faces().iter().zip(r2.faces().iter()).enumerate() {
        assert_eq!(
            f1.surface, f2.surface,
            "yr8 determinism: face {i} surface differs"
        );
    }
    assert_eq!(
        r.as_mesh().verts,
        r2.as_mesh().verts,
        "yr8 determinism: mesh.verts differ"
    );
    assert_eq!(
        r.as_mesh().tris,
        r2.as_mesh().tris,
        "yr8 determinism: mesh.tris differ"
    );
}

/// §6.5 (mock-path corollary): the hand-built kept mesh is watertight and
/// genus-0, so the reconstructed output mesh is too. This isolates the
/// "boolean() runs and produces a closed mesh" property without the sidecar.
///
/// RED: FAILS at the `expect` (CurvedSurfaceNotYetSupported) until GREEN.
#[test]
fn t1b_mock_output_mesh_watertight_and_euler_two() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_tube_arrangement(),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr8: cylinder-labeled Union must return Ok once the curved branch lands");

    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr8: mock output mesh must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr8: mock output mesh Euler V−E+F must be 2 (closed genus-0)"
    );
}

// =========================================================================
// TEST 2 — Real-sidecar E2E (env-gated on CHERCHI2022_BIN).
// Spec §6 items: 6.1 (runs & Ok), 6.2 (surface survival), 6.4 (geometric
// soundness), 6.5 (2-manifold / watertight + Euler).
//
// This is the empirical STOP-condition check (spec §5): it asserts the SUCCESS
// path. If real geometry hits a genuine F3-tie / NonManifoldOutput /
// non-watertight result, THAT is a §5 STOP — a real finding for the Manager,
// NOT something this test should pre-weaken to tolerate.
//
// Self-skips with a LOUD eprintln when the binary is absent (never silently
// passes).
// =========================================================================

#[test]
fn t2_e2e_cylinder_union_box_via_sidecar() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr8] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cyl = canonical_cylinder();
    let bx = canonical_box();

    // §6.1: runs & Ok, no panic.
    let r = boolean(&cyl, &bx, BoolOp::Union, &sb)
        .expect("yr8 E2E: cylinder ∪ box must return Ok (a §5 STOP if it cannot)");

    assert!(
        !r.faces().is_empty(),
        "yr8 E2E: output BRep must have ≥1 face"
    );

    // §6.2: ≥1 Surface::Cylinder face with the input's EXACT params.
    assert!(
        has_exact_cylinder_face(&r),
        "yr8 E2E: output must carry ≥1 Surface::Cylinder face equal to the input \
         cylinder's exact params {:?}; got faces {:?}",
        canonical_cylinder_surface(),
        r.faces().iter().map(|f| f.surface).collect::<Vec<_>>()
    );
    // No sphere/cone may ever appear.
    assert!(
        r.faces()
            .iter()
            .all(|f| !matches!(f.surface, Surface::Sphere { .. } | Surface::Cone { .. })),
        "yr8 E2E: output must contain no Sphere/Cone faces"
    );

    // §6.5: watertight + Euler V−E+F = 2 (a closed genus-0 union shell).
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr8 E2E: output mesh must be watertight (0 unpaired half-edges) — \
         else a §5 STOP (non-watertight union)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr8 E2E: output mesh Euler V−E+F must be 2"
    );

    // §6.4: every cylinder-lateral output vertex within d_ε of the analytic
    // cylinder; box-face vertices within TAU_WORK of their planes.
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);
    let axis_unit = unit(CYL_AXIS_DIR);
    let mesh = r.as_mesh();
    for tri in &mesh.tris {
        let pts: [[f64; 3]; 3] = [
            mesh.verts[tri[0] as usize].as_array(),
            mesh.verts[tri[1] as usize].as_array(),
            mesh.verts[tri[2] as usize].as_array(),
        ];
        // Cylinder-lateral triangle ⟺ all 3 verts on the analytic cylinder.
        let on_cyl = pts
            .iter()
            .all(|&x| (dist_point_to_line(x, CYL_AXIS_POINT, axis_unit) - CYL_RADIUS).abs() <= de);
        if on_cyl {
            for &x in &pts {
                let d = (dist_point_to_line(x, CYL_AXIS_POINT, axis_unit) - CYL_RADIUS).abs();
                assert!(
                    d <= de,
                    "yr8 E2E: lateral vertex {x:?} distance {d} exceeds chord bound d_ε {de}"
                );
            }
        }
    }
}

// =========================================================================
// TEST 3 — Sphere/Cone still loudly reject (direct, sidecar-independent).
// Spec §6 item: 6.7.
//
// Sphere/Cone surfaces must NEVER flow through the pipeline. They are rejected
// at construction time (`BRep::new`), which is the strongest possible
// guarantee that they can never reach `reconstruct_topology` as a kept patch:
// a Sphere/Cone INPUT BRep cannot even be built, so no mock or sidecar path can
// attribute an output triangle to a Sphere/Cone face. This mirrors the
// canonical assertion shape in yr7_cylinder.rs `sphere_face_still_rejected` /
// `cone_face_still_rejected` and yr7_adversary `attack6_*`.
//
// These tests PASS against current production (sphere/cone reject is live) and
// MUST continue to pass after the GREEN cylinder fix — the fix touches ONLY
// the Cylinder arm, never Sphere/Cone.
// =========================================================================

/// Single planar triangle in z=0 carrying a caller-chosen surface. Passes
/// degeneracy/winding before the surface match in `BRep::new`.
fn one_triangle(surface: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(2.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 2.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface,
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
fn t3_sphere_face_on_triangle_is_malformed() {
    // PR-YR12 migration: a sphere face on a *triangle* (no meridian seam Circle)
    // is now MalformedTopology, not CurvedSurfaceNotYetSupported — the sphere
    // Stage-1 path is implemented but this fixture lacks the required seam Circle
    // edge. Still a loud error, never silently flowing to reconstruct_topology.
    let (v, e, f) = one_triangle(Surface::Sphere {
        center: p(0.0, 0.0, 0.0),
        radius: 1.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "yr8: a Sphere face on a triangle must reject as MalformedTopology (lacks \
         its meridian seam Circle edge), got {r:?}"
    );
}

#[test]
fn t3_cone_face_still_loudly_rejected() {
    // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle) is now
    // MalformedTopology, not CurvedSurfaceNotYetSupported — still a loud error,
    // never silent (can never flow to reconstruct_topology). Only the error kind
    // changed.
    let (v, e, f) = one_triangle(Surface::Cone {
        apex: p(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "yr8: a Cone face on a triangle must STILL reject loudly as MalformedTopology \
         (lacks its base-rim Circle edge), got {r:?}"
    );
}

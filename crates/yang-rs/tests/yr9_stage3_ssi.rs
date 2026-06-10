//! PR-YR9 (P3) RED — Stage 3 SSI wiring: EXACT intersection edges for
//! `cylinder ∪ box`.
//!
//! Spec of record: `specs/yang_pr_yr9_stage3_ssi.md` (§7 is the RED contract).
//!
//! This is the RED half of a role-separated FIP cycle. It writes TESTS ONLY;
//! the GREEN implementer extends `crates/yang-rs/src/lib.rs` (Stage-3 ssi-rs
//! wiring in `reconstruct_topology`) to make the EXACT-conic oracles GREEN.
//! The RED author NEVER edits production code.
//!
//! ## RED contract (two-stage)
//!
//! 1. **Compile gate (the GREEN API surface).** This file references two
//!    error-type members the GREEN implementer ADDS to the public surface:
//!    `yang_rs::YangError::SsiRefinementFailed { edge, reason }` and the public
//!    sibling enum `yang_rs::SsiRefinementError` (spec §5.1). Those do NOT exist
//!    yet, so **this file does not compile against current production**. That is
//!    the intended initial RED state: the file turns from compile-fail to
//!    assert-fail the moment GREEN lands the error variants.
//!
//! 2. **Behavioral gate (LineSegment → exact conic).** Once the API surface
//!    exists, the bulk of these tests assert that the `cylinder ∪ box`
//!    intersection edges (the two cap rings) carry the EXACT `Curve::Circle`
//!    from `ssi_rs::intersect(Plane, Cylinder)` — NOT the P2c
//!    `Curve::LineSegment`. Until GREEN wires Stage 3, production still emits
//!    `LineSegment`, so every "exact" assertion FAILS. That is the RED state.
//!
//! Per the YR8 RED file's precedent, integration test files cannot share
//! helpers, so the YR8 harness (`p`, array math, `cylinder_brep`,
//! `unit_cube_brep_offset_at`, `d_eps`, canonical config, `LabelMock`,
//! `hand_built_tube_arrangement`, `one_triangle`, etc.) is re-declared verbatim
//! here. The oracle (the EXACT circle the output must match) is computed by
//! calling `ssi_rs::intersect` DIRECTLY from the test — the test reproduces the
//! analytic ground truth independently of production.
//!
//! Tolerances (spec §7, do not weaken):
//!   - **Exact-on-surface** uses `cad_primitives::TAU_MODEL` (strictly stronger
//!     than `d_ε`): a densely-sampled exact curve lies on BOTH incident analytic
//!     surfaces within `TAU_MODEL`.
//!   - **Endpoint / consistency** uses `d_ε` (= `1e-2 × analytic AABB diag`),
//!     the same Stage-1 chord bound the YR8 oracle recomputes.

use std::collections::{HashMap, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3, TAU_MODEL};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use std::error::Error;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives has no dot/cross/normalize helpers).
// Re-declared verbatim from tests/yr8_curved_boolean.rs — integration test
// files cannot share helpers.
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
// Cylinder B-Rep fixture (spec §1 seam-edge encoding). Re-declared from
// tests/yr8_curved_boolean.rs.
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
// Unit-cube fixture with TRUE per-face plane offsets. Re-declared from
// tests/yr8_curved_boolean.rs.
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
// Analytic mesh oracles. Re-declared from tests/yr8_curved_boolean.rs.
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
// Canonical config (spec §7 / Manager brief): cylinder axis +Z through the
// unit cube at the origin.
//
//   box      = unit_cube_brep_offset_at([0,0,0])  (spans 0..1 in x,y,z)
//   cylinder = cylinder_brep([0.5,0.5,-0.5], +Z, r=0.25, h=2.0)
//
// The two box caps are the planes z=0 (normal (0,0,-1), d=0) and z=1
// (normal (0,0,1), d=-1). `ssi_rs::intersect(Plane(cap), Cylinder)` for each
// cap returns exactly ONE `SsiCurve::Circle` (axis ⟂ cap → C1): center
// (0.5,0.5,0) / (0.5,0.5,1), normal = +Z, radius 0.25.
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
fn canonical_cylinder_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_RADIUS,
    }
}

// =========================================================================
// SSI ORACLE — compute the EXACT cap circle independently via ssi-rs.
//
// The cylinder owns `Surface::Cylinder`; each box cap is a `Surface::Plane`
// `n·x + d = 0`. Per spec §3, a yang `Surface::Plane { normal, d }` maps to
// `QuadricSurface::Plane { point = -d·normal, normal }`. The cylinder maps
// field-for-field to `QuadricSurface::Cylinder`. `ssi_rs::intersect` of those
// two is the C1 perpendicular case → exactly one `SsiCurve::Circle`.
// =========================================================================

/// yang `Surface` → `ssi_rs::QuadricSurface` (the test's independent oracle
/// conversion; production gets its own `surface_to_quadric`).
fn surface_to_quadric(s: Surface) -> ssi_rs::QuadricSurface {
    match s {
        Surface::Plane { normal, d } => {
            let n = unit(normal.as_array());
            let point = scale(n, -d);
            ssi_rs::QuadricSurface::Plane {
                point: Point3::from(point),
                normal: Vector3::from(n),
            }
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => ssi_rs::QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        },
        Surface::Sphere { center, radius } => ssi_rs::QuadricSurface::Sphere { center, radius },
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => ssi_rs::QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        },
    }
}

/// The EXACT cap circle on plane z = `cap_z` (cylinder ∩ that box cap),
/// computed by ssi-rs. The bottom cap is z=0 (yang normal (0,0,-1), d=0);
/// the top cap is z=1 (yang normal (0,0,1), d=-1).
fn oracle_cap_circle(cap_z: f64) -> ssi_rs::SsiCurve {
    let (normal, d) = if cap_z == 0.0 {
        (Vector3::new(0.0, 0.0, -1.0), 0.0)
    } else {
        (Vector3::new(0.0, 0.0, 1.0), -cap_z)
    };
    let plane = surface_to_quadric(Surface::Plane { normal, d });
    let cyl = surface_to_quadric(canonical_cylinder_surface());
    let curves = ssi_rs::intersect(&plane, &cyl)
        .expect("oracle: Plane∩Cylinder must succeed for a perpendicular cap");
    assert_eq!(
        curves.len(),
        1,
        "oracle: perpendicular cap section must be exactly one curve, got {curves:?}"
    );
    curves[0]
}

/// Implicit on-curve test (mirrors production's `curve_contains_point`, spec
/// §5.4) for the curve families this PR produces.
fn ssi_curve_contains_point(c: &ssi_rs::SsiCurve, pt: [f64; 3], tol: f64) -> bool {
    match c {
        ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = unit(normal.as_array());
            let w = sub(pt, center.as_array());
            let axial = dot(w, n).abs();
            let radial = norm(sub(w, scale(n, dot(w, n))));
            axial <= tol && (radial - radius).abs() <= tol
        }
        ssi_rs::SsiCurve::Line { point, dir } => {
            let d = unit(dir.as_array());
            dist_point_to_line(pt, point.as_array(), d) <= tol
        }
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = unit(normal.as_array());
            let maj = unit(major_axis.as_array());
            let min_axis = cross(n, maj);
            let w = sub(pt, center.as_array());
            if dot(w, n).abs() > tol {
                return false;
            }
            let u = dot(w, maj);
            let v = dot(w, min_axis);
            let residual = ((u / major_radius).powi(2) + (v / minor_radius).powi(2)).sqrt() - 1.0;
            (residual.abs() * major_radius.min(*minor_radius)) <= tol
        }
        _ => false,
    }
}

/// Convert an output `Curve::{Circle,Ellipse}` back into the corresponding
/// `ssi_rs::SsiCurve`, so the test can densely sample it via `.eval(t)`. Only
/// the conic families this PR emits are handled; a `LineSegment` has no closed
/// `SsiCurve` form here (its samples come from the edge endpoints).
fn curve_to_ssi(c: &Curve) -> Option<ssi_rs::SsiCurve> {
    match *c {
        Curve::Circle {
            center,
            normal,
            radius,
        } => Some(ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        }),
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => Some(ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        }),
        Curve::LineSegment => None,
        // PR-YR22: this yr9 helper only samples the circle/ellipse families it
        // emits; a Parabola has no closed `SsiCurve` form here (same as
        // LineSegment). Exhaustiveness arm forced by the new enum variant.
        Curve::Parabola { .. } => None,
        // PR-YR23: likewise a Hyperbola has no closed `SsiCurve` form in this
        // yr9 helper; exhaustiveness arm forced by the new enum variant.
        Curve::Hyperbola { .. } => None,
    }
}

// =========================================================================
// `LabelMock`: drive the PUBLIC boolean() with a HAND-BUILT LabeledArrangement
// (mirrors yr8_curved_boolean.rs / m3_adversary.rs).
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
// Hand-built watertight arrangement (re-declared from yr8_curved_boolean.rs).
//
//   - N = 8 facets. Lateral ring vertices lie EXACTLY on the analytic cylinder
//     (radius 0.25 about (0.5,0.5)) at z=0 (bottom ring) and z=1 (top ring).
//   - 8 lateral QUADS (16 triangles) → label InputId(0) = solid A = CYLINDER.
//   - bottom cap fan (8 tris) on plane z=0 + top cap fan (8 tris) on z=1 →
//     label InputId(1) = solid B = BOX.
//
//   The cap-ring boundary edges (lateral wall ↔ cap fan, i.e. a label-0 tri
//   adjacent to a label-1 tri along a shared mesh edge) are the OUTPUT
//   INTERSECTION EDGES whose `Curve` must become the EXACT cap `Circle`.
//
//   Total: V = 18, F = 32. Watertight (0 unpaired half-edges), Euler 2.
//   `inside` all-false ⇒ Union keeps ALL 32 triangles.
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
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];

    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

// =========================================================================
// Output-edge helpers (vertex lookup + curve classification).
// =========================================================================

/// Resolve an output `BRepEdge`'s start/end mesh-vertex positions. The output
/// `BRepVertex` list is 1:1 with `as_mesh().verts` (PR-YR5), and an edge's
/// `start`/`end` index into `vertices()`.
fn edge_endpoints(brep: &BRep, e: &BRepEdge) -> ([f64; 3], [f64; 3]) {
    let vs = brep.vertices();
    (
        vs[e.start as usize].point.as_array(),
        vs[e.end as usize].point.as_array(),
    )
}

/// Every output edge whose `Curve` is a conic (Circle/Ellipse). These are the
/// EXACT intersection edges this PR introduces.
fn conic_edges(brep: &BRep) -> Vec<&BRepEdge> {
    brep.edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Circle { .. } | Curve::Ellipse { .. }))
        .collect()
}

// =========================================================================
// TEST 1 — Has EXACT edges (spec §7.4) + Exact on BOTH surfaces (§7.1) +
// Endpoints (§7.2) + Consistency (§7.3) + determinism (§7.5). Mock direct path.
//
// RED: production still emits `Curve::LineSegment` on the cap rings, so
// `conic_edges` is EMPTY and the §7.4 assertion fails. (And the file only
// compiles once GREEN adds SsiRefinementError / SsiRefinementFailed.)
// =========================================================================

#[test]
fn t1_cap_rings_carry_exact_ssi_circles() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_tube_arrangement(),
    };

    let r = boolean(&cyl, &bx, BoolOp::Union, &mock)
        .expect("yr9: cylinder ∪ box (mock) must return Ok");

    // §7.4: ≥1 intersection edge carries Curve::Circle.
    let conics = conic_edges(&r);
    assert!(
        !conics.is_empty(),
        "yr9: expected ≥1 intersection edge with Curve::Circle (the cap rings); \
         got edge curves {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // §7.4: ALL conic intersection edges are Circles (the perpendicular cap
    // case — no ellipses for this canonical config).
    for e in &conics {
        assert!(
            matches!(e.curve, Curve::Circle { .. }),
            "yr9: cap-ring intersection edge must be a Circle (perpendicular cap), \
             got {:?}",
            e.curve
        );
    }

    // §7.4: both cap rings present — distinct centers at z≈0 and z≈1, each
    // matching the ssi-rs oracle for that cap (center / normal / radius within
    // TAU_MODEL).
    let oracle_bottom = oracle_cap_circle(0.0);
    let oracle_top = oracle_cap_circle(1.0);
    let (
        ssi_rs::SsiCurve::Circle {
            center: ocb,
            normal: onb,
            radius: orb,
        },
        ssi_rs::SsiCurve::Circle {
            center: oct,
            normal: ont,
            radius: ort,
        },
    ) = (oracle_bottom, oracle_top)
    else {
        panic!("oracle: cap sections must be circles");
    };
    // Oracle sanity (independent of production): the ssi-rs circles are what we
    // documented.
    assert!((orb - CYL_RADIUS).abs() <= TAU_MODEL && (ort - CYL_RADIUS).abs() <= TAU_MODEL);
    assert!((ocb.z() - 0.0).abs() <= TAU_MODEL && (oct.z() - 1.0).abs() <= TAU_MODEL);

    let mut saw_bottom = false;
    let mut saw_top = false;
    for e in &conics {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            continue;
        };
        // Which cap? Classify by z of the circle center.
        let (oc, on, orr) = if center.z().abs() <= 0.5 {
            saw_bottom = true;
            (ocb, onb, orb)
        } else {
            saw_top = true;
            (oct, ont, ort)
        };
        assert!(
            norm(sub(center.as_array(), oc.as_array())) <= TAU_MODEL,
            "yr9: cap-ring Circle center {:?} must equal ssi-rs oracle {:?} within TAU_MODEL",
            center,
            oc
        );
        // Normal must be parallel to the oracle normal (sign-invariant: the
        // conic is sign-invariant per spec §5; allow either orientation).
        let dotn = dot(unit(normal.as_array()), unit(on.as_array())).abs();
        assert!(
            (dotn - 1.0).abs() <= TAU_MODEL,
            "yr9: cap-ring Circle normal {:?} must be parallel to oracle normal {:?}",
            normal,
            on
        );
        assert!(
            (radius - orr).abs() <= TAU_MODEL,
            "yr9: cap-ring Circle radius {radius} must equal oracle radius {orr} within TAU_MODEL"
        );
    }
    assert!(
        saw_bottom && saw_top,
        "yr9: BOTH cap rings must be present (bottom z≈0 and top z≈1); \
         saw_bottom={saw_bottom} saw_top={saw_top}"
    );

    // §7.1 Exact on BOTH surfaces: densely sample each assigned exact curve
    // (≥32 pts over [0, 2π)) and assert every sample lies on the analytic
    // cylinder (|dist_to_axis − r| ≤ TAU_MODEL) AND on the incident cap plane
    // (|n·x + d| ≤ TAU_MODEL).
    let axis_unit = unit(CYL_AXIS_DIR);
    let n_samples = 64usize;
    for e in &conics {
        let Some(ssi) = curve_to_ssi(&e.curve) else {
            continue;
        };
        // The incident cap plane: pick z=0 or z=1 by the curve's center z.
        let Curve::Circle { center, .. } = e.curve else {
            continue;
        };
        let cap_z = if center.z().abs() <= 0.5 { 0.0 } else { 1.0 };
        // Plane n·x + d = 0: for z=0 cap → n=(0,0,-1), d=0; z=1 → n=(0,0,1), d=-1.
        let (pn, pd) = if cap_z == 0.0 {
            ([0.0, 0.0, -1.0], 0.0)
        } else {
            ([0.0, 0.0, 1.0], -cap_z)
        };
        for i in 0..n_samples {
            let t = 2.0 * std::f64::consts::PI * (i as f64) / (n_samples as f64);
            let x = ssi.eval(t).as_array();
            let d_cyl = (dist_point_to_line(x, CYL_AXIS_POINT, axis_unit) - CYL_RADIUS).abs();
            assert!(
                d_cyl <= TAU_MODEL,
                "yr9 §7.1: sample {x:?} of exact cap curve is off the cylinder by {d_cyl} > TAU_MODEL"
            );
            let d_plane = (dot(pn, x) + pd).abs();
            assert!(
                d_plane <= TAU_MODEL,
                "yr9 §7.1: sample {x:?} of exact cap curve is off cap plane (cap_z={cap_z}) \
                 by {d_plane} > TAU_MODEL"
            );
        }
    }

    // §7.2 Endpoints: the exact curve passes through the edge's start/end mesh
    // vertices within d_ε.
    let de = d_eps(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT);
    for e in &conics {
        let Some(ssi) = curve_to_ssi(&e.curve) else {
            continue;
        };
        let (s, t) = edge_endpoints(&r, e);
        assert!(
            ssi_curve_contains_point(&ssi, s, de),
            "yr9 §7.2: exact curve must pass through edge start {s:?} within d_ε {de}"
        );
        assert!(
            ssi_curve_contains_point(&ssi, t, de),
            "yr9 §7.2: exact curve must pass through edge end {t:?} within d_ε {de}"
        );
    }

    // §7.3 Consistency: the exact curve stays within d_ε of the straight P2c
    // polyline chord (the segment between the edge's two mesh vertices) it
    // replaces — i.e. the selected conic hugs the mesh edge, not a far circle.
    // We sample the chord's mid-arc on the exact curve and confirm both
    // endpoints are on the curve AND the chord midpoint is within d_ε of the
    // curve (so the conic is the RIGHT one near this edge).
    for e in &conics {
        let Some(ssi) = curve_to_ssi(&e.curve) else {
            continue;
        };
        let (s, t) = edge_endpoints(&r, e);
        let chord_mid = scale(add(s, t), 0.5);
        // The chord midpoint of a small arc is just inside the circle; its
        // distance to the curve is bounded by the chord's sagitta ≤ d_ε for
        // the canonical N=8 facet ring. Check via the implicit on-curve metric
        // against a slightly relaxed in-plane test: the midpoint must be within
        // d_ε of SOME point on the curve. Sample densely and take the min.
        let mut best = f64::INFINITY;
        for i in 0..256 {
            let th = 2.0 * std::f64::consts::PI * (i as f64) / 256.0;
            let x = ssi.eval(th).as_array();
            best = best.min(norm(sub(x, chord_mid)));
        }
        assert!(
            best <= de,
            "yr9 §7.3: exact curve strays {best} > d_ε {de} from the mesh chord midpoint \
             {chord_mid:?} (wrong conic selected?)"
        );
    }

    // §7.5 determinism: identical inputs → identical output edge curves.
    let mock2 = LabelMock {
        arrangement: hand_built_tube_arrangement(),
    };
    let r2 = boolean(&cyl, &bx, BoolOp::Union, &mock2).expect("yr9: determinism run 2");
    assert_eq!(
        r.edges().len(),
        r2.edges().len(),
        "yr9 determinism: edge count differs"
    );
    for (i, (e1, e2)) in r.edges().iter().zip(r2.edges().iter()).enumerate() {
        assert_eq!(
            e1.curve, e2.curve,
            "yr9 determinism: edge {i} curve differs ({:?} vs {:?})",
            e1.curve, e2.curve
        );
    }
}

// =========================================================================
// TEST 2 — Scope held: SAME-INPUT boundaries never become conics (spec §7.5).
//
// PREMISE CORRECTION (RED follow-up #2): the canonical tube mock
// (`hand_built_tube_arrangement`) has NO same-input boundary edge — its closed
// lateral tube (label A) is bounded only by the top+bottom rings, and both cap
// fans are label B, so EVERY emitted edge is an A↔B ring (correctly all become
// Circles via t1). There is no LineSegment to "survive" there.
//
// To actually test the over-reach guard we need a genuine SAME-INPUT seam:
// build a closed tube-WITH-CAPS where ALL triangles carry label 0 (= solid A =
// the cylinder), with input `a` a cylinder whose caps lie EXACTLY on the mesh
// ring planes (z=0 and z=2). Then:
//   - lateral walls resolve to A's `Surface::Cylinder` face (centroids within
//     the Stage-1 curved chord band d_ε of the analytic cylinder),
//   - cap fans resolve to A's z=0 / z=2 `Surface::Plane` cap faces (centroids
//     exactly ON those planes, within TAU_WORK),
//   - the two rings are now lateral-patch(A) ↔ cap-patch(A): SAME InputId ⇒ NO
//     ssi entry ⇒ every output edge stays `Curve::LineSegment`.
//
// NOTE on height: the N=8 facet ring has a chord sagitta r·(1−cos(π/8)) ≈
// 0.0190; the cylinder face's Stage-1 band is d_ε = 1e-2 × AABB-diag, which for
// a height-2 cylinder is ≈ 0.0212 > 0.0190 (lateral centroids resolve), but for
// a height-1 cylinder is ≈ 0.0122 < 0.0190 (would F3-fail face resolution). So
// the same-input tube uses height 2 (z = 0..2), matching the canonical
// cylinder's geometry exactly.
//
// This is the STRONGEST form of "same-input edges never become conics": EVERY
// output edge must be a LineSegment (no Circle/Ellipse anywhere), and ≥1 edge
// must exist.
//
// RED: production emits all LineSegment today, so this PASSES trivially before
// GREEN; after GREEN it guards that the conic conversion did not over-reach to
// same-input edges. (The file still only compiles once GREEN lands the error
// API.)
// =========================================================================

/// Height-2 cylinder with caps EXACTLY at z=0 and z=2 (axis +Z through
/// (0.5,0.5)), so a z=0/z=2 cap-fan mesh resolves to this cylinder's OWN cap
/// `Surface::Plane` faces. Same radius / d_ε as the canonical cylinder, so the
/// N=8 lateral facets resolve to the `Surface::Cylinder` face.
fn single_input_cylinder() -> BRep {
    cylinder_brep([0.5, 0.5, 0.0], CYL_AXIS_DIR, CYL_RADIUS, 2.0)
}

/// Closed tube-WITH-CAPS, EVERY triangle labelled InputId(0) (= solid A). The
/// lateral walls + both cap fans all belong to A, so the top/bottom rings are
/// SAME-INPUT (A↔A) boundary edges — they must stay `Curve::LineSegment`.
/// Geometry matches `single_input_cylinder` (rings on the analytic cylinder at
/// z=0 / z=2; cap centers on the z=0 / z=2 planes).
fn hand_built_single_input_tube() -> LabeledArrangement {
    let cx = 0.5;
    let cy = 0.5;
    let r = CYL_RADIUS;
    let (za, zb) = (0.0f64, 2.0f64);

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
    verts.push(p(cx, cy, za));
    let ct = verts.len() as u32;
    verts.push(p(cx, cy, zb));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    let push = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push(t);
        surf.push(vec![LaInputId(0)]); // EVERY triangle is solid A.
    };
    // Lateral walls.
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([bot[k], bot[k1], top[k1]], &mut tris, &mut surface);
        push([bot[k], top[k1], top[k]], &mut tris, &mut surface);
    }
    // Bottom cap fan (z=0, outward −z).
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([cb, bot[k1], bot[k]], &mut tris, &mut surface);
    }
    // Top cap fan (z=1, outward +z).
    for k in 0..N_FACETS {
        let k1 = (k + 1) % N_FACETS;
        push([ct, top[k], top[k1]], &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

#[test]
fn t2_same_input_edges_stay_line_segments() {
    let a = single_input_cylinder();
    // Input B is irrelevant to the all-A arrangement; a far-away box keeps the
    // pipeline's B branch valid without contributing any kept triangle.
    let b = unit_cube_brep_offset_at([100.0, 100.0, 100.0]);
    let mock = LabelMock {
        arrangement: hand_built_single_input_tube(),
    };
    let r = boolean(&a, &b, BoolOp::Union, &mock).expect("yr9: single-input mock union must Ok");

    assert!(!r.edges().is_empty(), "yr9: expected ≥1 output edge");
    // SAME-INPUT (A↔A) edges must NEVER become conics: EVERY edge is LineSegment.
    for (i, e) in r.edges().iter().enumerate() {
        assert!(
            matches!(e.curve, Curve::LineSegment),
            "yr9 §7.5: same-input boundary edge {i} must stay Curve::LineSegment (no SSI \
             entry for A↔A edges), got {:?}",
            e.curve
        );
    }
    // And no Circle/Ellipse anywhere in the output (over-reach guard).
    assert_eq!(
        conic_edges(&r).len(),
        0,
        "yr9 §7.5: a single-input solid must produce ZERO conic edges; the SSI conversion \
         over-reached to same-input boundaries"
    );
}

// =========================================================================
// TEST 3 — Scope held: planar two-box union → every edge is LineSegment
// (spec §7.5, Plane∩Plane → Line → LineSegment). Sidecar-free, deterministic.
//
// Two offset unit cubes overlapping in x produce only Plane∩Plane intersection
// edges. We drive a planar hand-built arrangement (all planar labels) through
// boolean() and assert EVERY output edge `Curve` is LineSegment — the planar
// `fuzz_boxes`-style corpus must NOT regress to conics.
//
// RED: production emits LineSegment today (passes); after GREEN this guards
// that Plane∩Plane edges convert to a Line → LineSegment (the map entry equals
// the fallback), NOT a spurious circle.
// =========================================================================

/// A minimal watertight two-label planar box: a unit cube whose 6 faces carry
/// alternating A/B labels so adjacent faces with different InputId produce
/// Plane∩Plane intersection edges. (Geometry is a single closed cube; the
/// labels are what create A↔B planar seams.)
fn hand_built_planar_box_arrangement() -> LabeledArrangement {
    // Unit cube 0..1 corners.
    let c = [
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(1.0, 1.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(0.0, 1.0, 1.0),
    ];
    let verts: Vec<Point3> = c.to_vec();
    // 12 triangles (2 per face), outward-facing winding. Label each face's two
    // tris; alternate labels per face so adjacent faces differ in InputId,
    // creating Plane∩Plane A↔B seams along the cube's edges.
    // Faces: bottom(z=0), top(z=1), front(y=0), back(y=1), left(x=0), right(x=1)
    let face_tris: [[[u32; 3]; 2]; 6] = [
        [[0, 2, 1], [0, 3, 2]], // bottom, outward −z
        [[4, 5, 6], [4, 6, 7]], // top, outward +z
        [[0, 1, 5], [0, 5, 4]], // front y=0, outward −y
        [[3, 7, 6], [3, 6, 2]], // back y=1, outward +y
        [[0, 4, 7], [0, 7, 3]], // left x=0, outward −x
        [[1, 2, 6], [1, 6, 5]], // right x=1, outward +x
    ];
    // Alternate labels: bottom/front/left = A(0), top/back/right = B(1).
    let face_label: [u32; 6] = [0, 1, 0, 1, 0, 1];

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for (f, pair) in face_tris.iter().enumerate() {
        for t in pair {
            tris.push(*t);
            surface.push(vec![LaInputId(face_label[f])]);
        }
    }
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

/// Axis-aligned box B-Rep spanning `lo..hi` on every axis — the t3 fixture
/// (PR-YR24) needs DIFFERENT-extent A/B boxes so the inputs are not
/// coplanar; same face order / outward normals as `unit_cube_brep_offset_at`.
fn aa_box_brep(lo: f64, hi: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(lo, lo, lo),
        },
        BRepVertex {
            point: p(hi, lo, lo),
        },
        BRepVertex {
            point: p(hi, hi, lo),
        },
        BRepVertex {
            point: p(lo, hi, lo),
        },
        BRepVertex {
            point: p(lo, lo, hi),
        },
        BRepVertex {
            point: p(hi, lo, hi),
        },
        BRepVertex {
            point: p(hi, hi, hi),
        },
        BRepVertex {
            point: p(lo, hi, hi),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z=lo)
        [4, 7, 6, 5], // F1 top (z=hi)
        [0, 4, 5, 1], // F2 front (y=lo)
        [1, 5, 6, 2], // F3 right (x=hi)
        [2, 6, 7, 3], // F4 back (y=hi)
        [3, 7, 4, 0], // F5 left (x=lo)
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
    let offs = [lo, -hi, lo, -hi, -hi, lo];
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
    BRep::new(verts, edges, faces).expect("aa_box_brep BRep::new failed")
}

#[test]
fn t3_planar_box_union_all_line_segments() {
    // Two planar boxes (A and B inputs). The hand-built arrangement provides
    // the labeling: the unit cube's {z=0, y=0, x=0} faces label to A and
    // {z=1, y=1, x=1} to B. PR-YR24: the original COINCIDENT unit cubes are
    // rejected by the near-coplanar input gate before the (mock) backend
    // runs; A = [0, 1.3]³ (planes {0, 1.3}) and B = [−0.3, 1]³ (planes
    // {−0.3, 1}) still give resolution a unique labeled plane for every tri
    // while sharing NO face plane.
    let a = aa_box_brep(0.0, 1.3);
    let b = aa_box_brep(-0.3, 1.0);
    let mock = LabelMock {
        arrangement: hand_built_planar_box_arrangement(),
    };
    let r = boolean(&a, &b, BoolOp::Union, &mock)
        .expect("yr9: planar two-box mock union must return Ok");

    // Every output edge MUST be a LineSegment (Plane∩Plane → Line → LineSegment).
    for (i, e) in r.edges().iter().enumerate() {
        assert!(
            matches!(e.curve, Curve::LineSegment),
            "yr9 §7.5: planar union edge {i} must be LineSegment (Plane∩Plane), got {:?} \
             — the planar corpus must not regress to conics",
            e.curve
        );
    }
    // Watertight + closed (sanity: the construction is a valid closed cube).
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr9: planar box mock output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr9: planar box mock output Euler V−E+F must be 2"
    );
}

// =========================================================================
// TEST 4 — Scope held: sphere/cone still loudly reject at BRep::new
// (spec §7.5). Copied from yr8_curved_boolean.rs t3. PASSES today and MUST
// keep passing after GREEN (the SSI wiring touches only the Cylinder∩Plane
// path, never Sphere/Cone construction-time rejection).
// =========================================================================

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
fn t4_sphere_face_on_triangle_is_malformed() {
    // PR-YR12 migration: a sphere face on a *triangle* (no meridian seam Circle)
    // is now MalformedTopology, not CurvedSurfaceNotYetSupported — the sphere
    // Stage-1 path is implemented but this fixture lacks the required seam Circle
    // edge. Still a loud error, never silent.
    let (v, e, f) = one_triangle(Surface::Sphere {
        center: p(0.0, 0.0, 0.0),
        radius: 1.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "yr9: a Sphere face on a triangle must reject as MalformedTopology (lacks \
         its meridian seam Circle edge), got {r:?}"
    );
}

#[test]
fn t4_cone_face_still_loudly_rejected() {
    // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle) is now
    // MalformedTopology, not CurvedSurfaceNotYetSupported — still a loud error,
    // never silent. Only the error kind changed.
    let (v, e, f) = one_triangle(Surface::Cone {
        apex: p(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    });
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "yr9: a Cone face on a triangle must STILL reject loudly as MalformedTopology \
         (lacks its base-rim Circle edge), got {r:?}"
    );
}

// =========================================================================
// TEST 5 — STOP path (spec §7.6): a selection failure returns
// `Err(YangError::SsiRefinementFailed { .. })` — NOT a silent LineSegment
// fallback and NOT a panic.
//
// REACHABILITY CORRECTION (RED follow-up #3). The face-resolution gate (step 5
// of `boolean()`) and the SSI selection (step 5/§5.5 `build_intersection_curves`)
// use COMPATIBLE tolerances: any mesh/surface inconsistency that breaks SSI
// breaks face resolution FIRST, so the previous "displaced cap box" construction
// errored with `FaceResolutionFailed` (centroids off every box plane) and never
// reached the SSI path. To reach the SSI STOP we need geometry that PASSES face
// resolution but THEN fails `ssi_rs::intersect` or selection.
//
// CONSTRUCTION — coincident planes → `IntersectFailed(DegenerateInput)`:
//   - A closed unit cube mesh whose BOTTOM face (z=0) is split by the diagonal
//     0–2 into two triangles: tri[0,2,1] labelled InputId(0) (= A) and
//     tri[0,3,2] labelled InputId(1) (= B). The other 5 faces (10 tris) are all
//     label A.
//   - Input `a` and input `b` are BOTH unit cubes at the origin, so each has a
//     z=0 bottom `Surface::Plane` face. Both bottom triangles' centroids lie
//     EXACTLY on z=0, so face resolution succeeds: the A triangle resolves to
//     A's z=0 face, the B triangle to B's z=0 face (each within TAU_WORK).
//   - Flood-fill puts the A and B bottom triangles in DIFFERENT patches; their
//     shared diagonal edge 0–2 is an A↔B boundary edge with incidence
//     [(A, Plane z=0), (B, Plane z=0)]. `ssi_rs::intersect` of two COINCIDENT
//     planes returns `Err(SsiError::DegenerateInput)` (verified below by reading
//     ssi-rs `plane_plane`: coincident ⇒ Err) ⇒ the production path returns
//     `SsiRefinementFailed { reason: IntersectFailed(..) }`.
//   - The two bottom triangles are DISJOINT halves of one planar face (not
//     coincident triangles), so there is NO non-manifold / coincident-triangle
//     rejection.
//
// RED: this test only COMPILES once GREEN adds `SsiRefinementFailed` /
// `SsiRefinementError`. Behaviorally, before the Stage-3 wiring exists,
// production emits LineSegment (Ok), so the error `match` FAILS — the intended
// RED. After GREEN, the loud STOP must fire.
// =========================================================================

/// Closed unit cube whose bottom (z=0) face is split A/B along the diagonal
/// 0–2; all other faces are label A. The A↔B diagonal is a coincident-plane
/// (both z=0) intersection edge → forces the SSI `IntersectFailed` STOP.
fn hand_built_coincident_plane_arrangement() -> LabeledArrangement {
    // Unit cube corners.
    let verts = vec![
        p(0.0, 0.0, 0.0), // 0
        p(1.0, 0.0, 0.0), // 1
        p(1.0, 1.0, 0.0), // 2
        p(0.0, 1.0, 0.0), // 3
        p(0.0, 0.0, 1.0), // 4
        p(1.0, 0.0, 1.0), // 5
        p(1.0, 1.0, 1.0), // 6
        p(0.0, 1.0, 1.0), // 7
    ];
    // (tri, label). Bottom face split: [0,2,1]→A, [0,3,2]→B (shared diag 0–2).
    // Remaining 5 faces all label A. Windings are outward-facing for a closed,
    // watertight cube.
    let entries: &[([u32; 3], u32)] = &[
        ([0, 2, 1], 0), // bottom A
        ([0, 3, 2], 1), // bottom B  (coincident z=0 plane with the A tri)
        ([4, 5, 6], 0), // top
        ([4, 6, 7], 0),
        ([0, 1, 5], 0), // front y=0
        ([0, 5, 4], 0),
        ([3, 7, 6], 0), // back y=1
        ([3, 6, 2], 0),
        ([0, 4, 7], 0), // left x=0
        ([0, 7, 3], 0),
        ([1, 2, 6], 0), // right x=1
        ([1, 6, 5], 0),
    ];
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    for &(t, label) in entries {
        tris.push(t);
        surface.push(vec![LaInputId(label)]);
    }
    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

#[test]
fn t5_stop_path_coincident_planes_is_loud() {
    // CONFIRM ssi-rs behavior on coincident planes (the STOP trigger): two
    // bit-identical z=0 planes → Err (DegenerateInput per ssi-rs plane_plane).
    let plane_a = surface_to_quadric(Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    });
    let plane_b = surface_to_quadric(Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    });
    assert!(
        ssi_rs::intersect(&plane_a, &plane_b).is_err(),
        "oracle: ssi_rs::intersect of two coincident planes must Err (the STOP trigger)"
    );

    // A is a unit cube at the origin; B is a unit cube at x = 5 — its bottom
    // face lies on the SAME infinite z=0 plane (so the B-labeled bottom tri
    // still resolves to B's bottom face) but the two faces' AABBs are
    // DISJOINT, so the PR-YR24 near-coplanar scan deliberately does NOT
    // flag the pair (far-apart same-plane faces cannot interact — the
    // documented over-deferral avoidance).
    //
    // PR-YR26 (M8 slice b) CONTRACT CHANGE: a Plane∩Plane intersection edge
    // no longer consults `ssi_rs::intersect` at all. The YR18 on-both gate
    // already verifies both endpoints lie on both planes within TAU_WORK,
    // and the unique line through two distinct points on both planes is the
    // edge's own line — `Curve::LineSegment` EXACTLY (zero chord error),
    // byte-equivalent to the SSI route for transversal planes. For the
    // §4.5.5 coplanar seams the boundary of the trimmed common surface IS
    // the intersection curve ("The boundaries of the common surface are
    // regarded as intersection curves between the two models",
    // refs/text/yang2025_hybrid_boolean.txt:728-730) and comes from the 2D
    // overlay, not from SSI — so the former coincident-plane SSI STOP is no
    // longer reachable for planes (the ssi-rs oracle above still pins that
    // ssi_rs itself refuses the pair). The hand-built coincident-plane
    // arrangement must now reassemble cleanly with LineSegment edges.
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([5.0, 0.0, 0.0]);
    let mock = LabelMock {
        arrangement: hand_built_coincident_plane_arrangement(),
    };
    let r = boolean(&a, &b, BoolOp::Union, &mock)
        .expect("yr9 §7.6 (PR-YR26): a Plane∩Plane edge resolves to LineSegment, no SSI STOP");
    assert!(
        r.edges()
            .iter()
            .all(|e| matches!(e.curve, Curve::LineSegment)),
        "yr9 §7.6 (PR-YR26): all-planar output edges must be LineSegment"
    );
}

// =========================================================================
// TEST 6 — Real-sidecar E2E (env-gated on CHERCHI2022_BIN). Spec §7.7.
//
// Mirrors yr8_curved_boolean.rs t2, but ADDITIONALLY asserts ≥1 output edge is
// Curve::Circle matching the cap-ring oracle. Self-skips with a LOUD eprintln
// when the binary is absent.
// =========================================================================

#[test]
fn t6_e2e_cylinder_union_box_has_exact_cap_circle() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr9] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let cyl = canonical_cylinder();
    let bx = canonical_box();

    let r = boolean(&cyl, &bx, BoolOp::Union, &sb).expect("yr9 E2E: cylinder ∪ box must return Ok");

    assert!(!r.faces().is_empty(), "yr9 E2E: output must have ≥1 face");

    // ≥1 intersection edge carries Curve::Circle matching the cap-ring oracle.
    let conics = conic_edges(&r);
    assert!(
        !conics.is_empty(),
        "yr9 E2E: output must carry ≥1 Curve::Circle intersection edge (the cap rings); \
         got curves {:?}",
        r.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    let oracle_bottom = oracle_cap_circle(0.0);
    let oracle_top = oracle_cap_circle(1.0);
    let (
        ssi_rs::SsiCurve::Circle {
            center: ocb,
            radius: orb,
            ..
        },
        ssi_rs::SsiCurve::Circle {
            center: oct,
            radius: ort,
            ..
        },
    ) = (oracle_bottom, oracle_top)
    else {
        panic!("oracle circles");
    };

    // Each Circle must match one of the cap-ring oracles (center + radius)
    // within TAU_MODEL.
    for e in &conics {
        let Curve::Circle { center, radius, .. } = e.curve else {
            continue;
        };
        let near_bottom = norm(sub(center.as_array(), ocb.as_array())) <= TAU_MODEL
            && (radius - orb).abs() <= TAU_MODEL;
        let near_top = norm(sub(center.as_array(), oct.as_array())) <= TAU_MODEL
            && (radius - ort).abs() <= TAU_MODEL;
        assert!(
            near_bottom || near_top,
            "yr9 E2E: Circle (center {center:?}, r {radius}) must match a cap-ring oracle \
             (bottom {ocb:?}/{orb} or top {oct:?}/{ort}) within TAU_MODEL"
        );
    }

    // Watertight + Euler 2 (the union shell, mirroring YR8 t2).
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr9 E2E: output mesh must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr9 E2E: output mesh Euler V−E+F must be 2"
    );
}

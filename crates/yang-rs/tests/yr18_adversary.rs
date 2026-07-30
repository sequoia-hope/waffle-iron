//! PR-YR18 ADVERSARY — independent audit of the on-both-surfaces gate.
//!
//! Spec of record: `docs/specs/yr18_intersection_edge_attribution.md`.
//!
//! The RED oracles (`yr18_attribution.rs`) only assert the NEGATIVE: that the
//! mis-classified seam edge no longer raises `AmbiguousCurve { matched: 0 }`.
//! They accept ANY success or ANY non-AmbiguousCurve error — so a degenerate
//! "fix" that skipped EVERY intersection edge (silently downgrading all of them
//! to `Curve::LineSegment`) would ALSO pass them. The danger that leaves
//! unpinned is **over-skipping**: the gate silencing a GENUINE intersection edge
//! whose both endpoints really are on both surfaces.
//!
//! This adversary file pins the two properties the RED test does not:
//!
//!   1. `genuine_intersection_edges_survive_the_gate_and_emit_circles` — the
//!      canonical CLOSED cylinder∪box tube (the yr9 `t6` geometry: cylinder
//!      lateral wall = label 0, the two box caps = label 1 planes z=0 / z=1).
//!      The two cap rings are GENUINE cylinder∩plane intersection edges whose
//!      every endpoint lies on BOTH surfaces, so they MUST pass the
//!      on-both-surfaces gate, reach `ssi_rs::intersect`, and emit
//!      `Curve::Circle` edges. A gate that over-skipped genuine edges would
//!      silently downgrade them to `Curve::LineSegment` and NO circle would
//!      survive — caught here. (yr9 `t1`/`t6` already prove this on HEAD; this
//!      restates it as a focused YR18 over-skip guard so a future regression to
//!      the gate is attributed to YR18, not buried in yr9.)
//!
//!   2. `gate_is_a_necessary_condition_of_selection` — a unit-level pin of the
//!      no-regression invariant (spec §4): every point ON the selected
//!      intersection circle (the curve `curve_contains_point` matches against)
//!      is within `tol` of BOTH surfaces — indeed within round-off. Pinned via
//!      the crate's public `signed_distance_to_surface`, the SAME predicate the
//!      gate uses. This proves the gate cannot reject an edge the selection
//!      would accept (the gate is a necessary condition of `matched == 1`),
//!      which the RED test never exercises.
//!
//! Deterministic, sidecar-free (hand-built `LabeledArrangement` + `LabelMock`,
//! the same pattern yr9 uses). NO production code is modified by this file.

use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (mirrors the yr9 fixture).
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

// =========================================================================
// Canonical config — IDENTICAL to yr9 t6 (the proven-closed tube whose cap
// rings carry exact SSI circles on HEAD).
//   box      = unit cube spanning 0..1 (caps z=0 and z=1)
//   cylinder = axis +Z through (0.5,0.5,-0.5), r=0.25, h=2.0
// =========================================================================

const CYL_AXIS_POINT: [f64; 3] = [0.5, 0.5, -0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_RADIUS: f64 = 0.25;
const CYL_HEIGHT: f64 = 2.0;
const N_FACETS: usize = 8;

fn canonical_cylinder_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_RADIUS,
    }
}

/// The box cap planes the cap rings lie on (z=0 outward −Z, z=1 outward +Z).
fn bottom_cap_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    }
}
fn top_cap_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.0,
    }
}

/// The cylinder's Stage-1 chord band, computed from its rim circles' AABB
/// exactly as `curved_chord_bound` / yr9 `d_eps` does.
fn cyl_chord_tol() -> f64 {
    let axis_unit = unit(CYL_AXIS_DIR);
    let bottom_center = CYL_AXIS_POINT;
    let top_center = add(bottom_center, scale(axis_unit, CYL_HEIGHT));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = CYL_RADIUS * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    1e-2 * norm(sub(hi, lo))
}

// =========================================================================
// Input B-Reps (cylinder + unit box), copied from the yr9 fixture so the
// production `tol` (recomputed from the cylinder B-Rep) matches.
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
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
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

/// Unit cube at `origin` spanning origin..origin+1, outward normals + offsets.
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

fn canonical_cylinder() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT)
}
fn canonical_box() -> BRep {
    unit_cube_brep_offset_at([0.0, 0.0, 0.0])
}

// =========================================================================
// LabelMock + the GENUINE CLOSED tube arrangement (yr9 t6 geometry): lateral
// walls labelled CYLINDER (id 0), both cap fans labelled BOX (id 1). The cap
// rings (z=0 and z=1) are GENUINE cylinder∩plane intersection edges — every
// endpoint exactly on radius 0.25 on the cap plane → on BOTH surfaces.
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

fn genuine_tube_arrangement() -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let r = CYL_RADIUS;
    let (za, zb) = (0.0f64, 1.0f64); // box z extent → the two cap rings

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
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    }
}

fn run_union() -> Result<BRep, YangError> {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: genuine_tube_arrangement(),
    };
    // a = cylinder (id 0), b = box (id 1) — matches the label ids above.
    boolean(&cyl, &bx, BoolOp::Union, &mock)
}

// =========================================================================
// ADVERSARY 1 — NO OVER-SKIP. The two genuine cylinder∩plane cap rings (every
// endpoint on BOTH surfaces) MUST pass the on-both-surfaces gate, reach
// `ssi_rs::intersect`, and emit `Curve::Circle` edges. A gate that over-skipped
// would silently downgrade them to `Curve::LineSegment` → NO circle survives.
// =========================================================================

#[test]
fn genuine_intersection_edges_survive_the_gate_and_emit_circles() {
    let brep = run_union().expect(
        "yr18 ADV1: the canonical genuine cylinder∪box (all cap-ring endpoints on both \
         surfaces) must return Ok — the gate must NOT skip genuine intersection edges",
    );

    // Fixture sanity: every cap-ring vertex really is on BOTH surfaces within
    // tol (so the gate passes for every cap-ring edge). If this failed the test
    // would be vacuous.
    let tol = cyl_chord_tol();
    let cyl = canonical_cylinder_surface();
    for (z, plane) in [(0.0, bottom_cap_plane()), (1.0, top_cap_plane())] {
        for k in 0..N_FACETS {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N_FACETS as f64);
            let pt = p(
                CYL_AXIS_POINT[0] + CYL_RADIUS * th.cos(),
                CYL_AXIS_POINT[1] + CYL_RADIUS * th.sin(),
                z,
            );
            let sc = signed_distance_to_surface(cyl, pt).expect("cyl sd").abs();
            let sp = signed_distance_to_surface(plane, pt)
                .expect("plane sd")
                .abs();
            assert!(
                sc <= tol && sp <= tol,
                "yr18 ADV1 (fixture sanity): cap ring vertex (z={z}, k={k}) must be on \
                 BOTH surfaces within tol (cyl |sd|={sc}, plane |sd|={sp}, tol={tol})"
            );
        }
    }

    // Decisive pin: at least one Curve::Circle survived (over-skip would leave
    // ONLY LineSegments).
    let circles: Vec<&BRepEdge> = brep
        .edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::Circle { .. }))
        .collect();
    assert!(
        !circles.is_empty(),
        "yr18 ADV1: the genuine cap rings produced NO Curve::Circle edge — the \
         on-both-surfaces gate OVER-SKIPPED a genuine intersection edge, silently \
         downgrading it to a LineSegment (a hack-to-green that swallows real geometry). \
         Emitted curves: {:?}",
        brep.edges().iter().map(|e| e.curve).collect::<Vec<_>>()
    );

    // Each surviving circle is a cap ring (z≈0 or z≈1, r≈0.25, normal ±Z),
    // proving the gate let the genuine SSI curve through, not a fallback artifact.
    for e in &circles {
        let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        else {
            unreachable!()
        };
        let z = center.z();
        assert!(
            (z.abs() <= tol || (z - 1.0).abs() <= tol)
                && (radius - CYL_RADIUS).abs() <= tol
                && normal.as_array()[2].abs() >= 1.0 - 1e-9,
            "yr18 ADV1: a surviving Circle is not a cap ring: center={center:?} \
             normal={normal:?} radius={radius} (expected z≈0/1, r≈{CYL_RADIUS}, ±Z)"
        );
    }
}

// =========================================================================
// ADVERSARY 2 — the no-regression invariant (spec §4), pinned at unit level via
// the SAME predicate the gate uses (`signed_distance_to_surface`). Every point
// on the selected intersection circle (the curve `curve_contains_point` matches
// against in `build_intersection_curves`) is within `tol` of BOTH surfaces —
// indeed within round-off. Therefore the gate is a NECESSARY condition of
// `matched == 1`: it can never reject an edge the selection would accept. The
// RED test never exercises this (it only perturbs ONE endpoint OFF a surface).
// =========================================================================

#[test]
fn gate_is_a_necessary_condition_of_selection() {
    let tol = cyl_chord_tol();
    let cyl = canonical_cylinder_surface();

    for (z, plane) in [(0.0, bottom_cap_plane()), (1.0, top_cap_plane())] {
        // Dense sampling AROUND each cap ring (4× the facet count), exactly on
        // the analytic intersection circle — the curve the SSI returns and the
        // selection matches against.
        for k in 0..(4 * N_FACETS) {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / ((4 * N_FACETS) as f64);
            let on_curve = p(
                CYL_AXIS_POINT[0] + CYL_RADIUS * th.cos(),
                CYL_AXIS_POINT[1] + CYL_RADIUS * th.sin(),
                z,
            );
            let sc = signed_distance_to_surface(cyl, on_curve)
                .expect("cyl sd")
                .abs();
            let sp = signed_distance_to_surface(plane, on_curve)
                .expect("plane sd")
                .abs();
            // Within tol (the gate's band) — comfortably, since the curve lies
            // on both surfaces. This is the geometric fact the invariant rests
            // on: a matched point is on both surfaces, so the gate's per-surface
            // test is implied by the selection's on-curve test.
            assert!(
                sc <= tol && sp <= tol,
                "yr18 ADV2: a point on the selected intersection circle (z={z}) is off a \
                 surface beyond tol (cyl |sd|={sc}, plane |sd|={sp}, tol={tol}) — would \
                 break the no-regression invariant (the gate could reject a matched edge)"
            );
            // Residual is essentially exact, not merely within the chord band:
            // the curve genuinely lies on both quadrics.
            assert!(
                sc <= 1e-9 && sp <= 1e-9,
                "yr18 ADV2: on-curve residual unexpectedly large (cyl={sc}, plane={sp})"
            );
        }
    }
}

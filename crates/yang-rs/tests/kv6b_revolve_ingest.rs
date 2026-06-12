//! PR-KV6b-1 RED — Stage-1 ingestion of the kernel-v2 REVOLVE vocabulary.
//!
//! Two new input shapes (the PR-KV6a revolve outputs, hand-built here in
//! yang-rs's own types per the crate scope rules):
//!
//! - **Partial revolve** (rectangle r1=1, r2=2, h=3 about the x-axis, sweep
//!   α ∈ {90°, 180°, 350°}): 2 planar quad caps, 2 planar annular-sector
//!   walls (loop [seg, arc, seg, arc]), 2 partial-cylinder walls (loop
//!   [seg, arc, seg, arc]; the inner one `reversed: true`), 4 directed
//!   sweep arcs.
//! - **Washer** (the same rectangle at α = 360°): 2 annular caps (full-
//!   circle outer loop + full-circle RING), outer + inner canonical
//!   cylinders (inner `reversed: true`, rims mirrored), genus 1.
//!
//! ## The input-arc convention (NEW, pinned here)
//!
//! An input `BRepEdge` with `curve: Circle` and `start != end` denotes the
//! CCW sweep around `curve.normal` from `vertices[start]` to
//! `vertices[end]` — unique in (0, 2π), so the exactly-π (180°) and
//! major-arc (350°) cases are unambiguous. `start == end` remains the full
//! circle. (Outputs are unaffected: yang output Circle edges stay
//! SSI-derived sub-arcs at mesh granularity.)
//!
//! ## Oracle groups
//!
//! 1. RED pins: today `BRep::new` rejects both fixtures loudly.
//! 2. (GREEN) Stage-1 mesh: watertight + 2-manifold; Euler χ = 2 for the
//!    partial solids, χ = 0 for the washer (torus).
//! 3. (GREEN) Bijection: `eval_source(map.lookup(v))` reproduces every mesh
//!    vertex (yr7 oracle 3); arc-sourced Steiner verts lie exactly on
//!    their circles.
//! 4. (GREEN) Orientation/volume: positive signed mesh volume within a 3%
//!    band of Pappus `α(r₂²−r₁²)h/2` (catches reversed-wall winding).
//! 5. (GREEN) End-to-end booleans vs a box (Union and Subtract) through
//!    the native backend: watertight 2-manifold output, sandwich volume
//!    bounds, cavity walls carry `reversed: true` (Stage-6 propagation).
//! 6. Adversary: arc endpoint off its circle → loud; reversed planar
//!    face → loud.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

const R1: f64 = 1.0;
const R2: f64 = 2.0;
const H: f64 = 3.0;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// Rotate (y, z) by α about the +x axis (right-handed).
fn rot(yv: f64, zv: f64, a: f64) -> (f64, f64) {
    (yv * a.cos() - zv * a.sin(), yv * a.sin() + zv * a.cos())
}

fn pappus(angle: f64) -> f64 {
    angle * (R2 * R2 - R1 * R1) * H / 2.0
}

// =========================================================================
// Fixtures
// =========================================================================

/// Partial revolve of the rectangle x∈[0,H], y∈[R1,R2] (z = 0 plane) about
/// the +x axis by `angle` ∈ (0, 2π). Mirrors the PR-KV6a kernel-v2 output
/// shape exactly. Vertex layout: ring0 = [v0..v3] (profile CCW:
/// (0,R1) (H,R1) (H,R2) (0,R2)), ring1 = rotated copies [v4..v7].
/// Edge layout: s0..s3 = ring0 profile segs (12 o'clock order), t0..t3 =
/// ring1 segs, a0..a3 = sweep arcs vᵢ→vᵢ₊₄ with normal +x̂ (the CCW sweep
/// from ring0 to ring1 IS α — the input-arc convention).
fn partial_revolve_brep(angle: f64) -> Result<BRep, yang_rs::YangError> {
    let prof = [(0.0, R1), (H, R1), (H, R2), (0.0, R2)];
    let mut verts: Vec<BRepVertex> = prof
        .iter()
        .map(|&(x, y)| BRepVertex {
            point: p(x, y, 0.0),
        })
        .collect();
    for &(x, y) in &prof {
        let (yy, zz) = rot(y, 0.0, angle);
        verts.push(BRepVertex {
            point: p(x, yy, zz),
        });
    }

    let seg = |a: u32, b: u32| BRepEdge {
        start: a,
        end: b,
        curve: Curve::LineSegment,
    };
    let mut edges = vec![
        seg(0, 1), // s0 bottom (y = R1)
        seg(1, 2), // s1 right (x = H)
        seg(2, 3), // s2 top (y = R2)
        seg(3, 0), // s3 left (x = 0)
        seg(4, 5), // t0
        seg(5, 6), // t1
        seg(6, 7), // t2
        seg(7, 4), // t3
    ];
    for i in 0..4u32 {
        let (x, y) = prof[i as usize];
        edges.push(BRepEdge {
            start: i,
            end: i + 4,
            curve: Curve::Circle {
                center: p(x, 0.0, 0.0),
                normal: v3(1.0, 0.0, 0.0),
                radius: y,
            },
        });
    }
    let (a0, a1, a2, a3) = (8u32, 9u32, 10u32, 11u32);

    let (cos_a, sin_a) = (angle.cos(), angle.sin());
    let end_normal = v3(0.0, -sin_a, cos_a); // R_x(α)·ẑ
    let end_d = -(end_normal.y() * verts[4].point.y() + end_normal.z() * verts[4].point.z());

    let faces = vec![
        // start cap (z = 0 plane, outward −ẑ: material sweeps toward +z)
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![],
            reversed: false,
        },
        // end cap
        BRepFace {
            surface: Surface::Plane {
                normal: end_normal,
                d: end_d,
            },
            outer_loop: vec![4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        },
        // inner cylinder wall (bottom profile edge, radius R1, cavity sense)
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: v3(1.0, 0.0, 0.0),
                radius: R1,
            },
            outer_loop: vec![0, a1, 4, a0],
            inner_loops: vec![],
            reversed: true,
        },
        // right annular sector (x = H plane, outward +x̂)
        BRepFace {
            surface: Surface::Plane {
                normal: v3(1.0, 0.0, 0.0),
                d: -H,
            },
            outer_loop: vec![1, a2, 5, a1],
            inner_loops: vec![],
            reversed: false,
        },
        // outer cylinder wall (top profile edge, radius R2)
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: v3(1.0, 0.0, 0.0),
                radius: R2,
            },
            outer_loop: vec![2, a3, 6, a2],
            inner_loops: vec![],
            reversed: false,
        },
        // left annular sector (x = 0 plane, outward −x̂)
        BRepFace {
            surface: Surface::Plane {
                normal: v3(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![3, a0, 7, a3],
            inner_loops: vec![],
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces)
}

/// The 360° washer: same rectangle, full turn. Rims carry the CAP-side
/// directional normal (the `to_yang_brep` shared-edge convention): each
/// annulus' outer circle traverses CCW around the cap's outward normal,
/// its ring CCW around the negation; the cylinders see the exact negations
/// (outer tube rims point toward each other in lateral sense; the reversed
/// inner tube's rims point away — the kernel-v2 mirrored rim rule).
fn washer_brep() -> Result<BRep, yang_rs::YangError> {
    let verts = vec![
        BRepVertex {
            point: p(0.0, R1, 0.0),
        }, // v0
        BRepVertex {
            point: p(H, R1, 0.0),
        }, // v1
        BRepVertex {
            point: p(H, R2, 0.0),
        }, // v2
        BRepVertex {
            point: p(0.0, R2, 0.0),
        }, // v3
    ];
    let circ = |v: u32, x: f64, r: f64, nx: f64| BRepEdge {
        start: v,
        end: v,
        curve: Curve::Circle {
            center: p(x, 0.0, 0.0),
            normal: v3(nx, 0.0, 0.0),
            radius: r,
        },
    };
    let edges = vec![
        circ(0, 0.0, R1, 1.0),  // c0: ring of the x=0 cap (CCW around +x̂ = −(−x̂))
        circ(1, H, R1, -1.0),   // c1: ring of the x=H cap
        circ(2, H, R2, 1.0),    // c2: outer circle of the x=H cap (CCW around +x̂)
        circ(3, 0.0, R2, -1.0), // c3: outer circle of the x=0 cap
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        }, // s_in (4)
        BRepEdge {
            start: 3,
            end: 2,
            curve: Curve::LineSegment,
        }, // s_out (5)
    ];

    let faces = vec![
        // x = 0 annular cap (outward −x̂): outer = c3, ring = c0
        BRepFace {
            surface: Surface::Plane {
                normal: v3(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![3],
            inner_loops: vec![vec![0]],
            reversed: false,
        },
        // x = H annular cap (outward +x̂): outer = c2, ring = c1
        BRepFace {
            surface: Surface::Plane {
                normal: v3(1.0, 0.0, 0.0),
                d: -H,
            },
            outer_loop: vec![2],
            inner_loops: vec![vec![1]],
            reversed: false,
        },
        // outer tube (canonical lateral)
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: v3(1.0, 0.0, 0.0),
                radius: R2,
            },
            outer_loop: vec![3, 5, 2, 5],
            inner_loops: vec![],
            reversed: false,
        },
        // inner tube (cavity sense, mirrored rims)
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: v3(1.0, 0.0, 0.0),
                radius: R1,
            },
            outer_loop: vec![0, 4, 1, 4],
            inner_loops: vec![],
            reversed: true,
        },
    ];

    BRep::new(verts, edges, faces)
}

/// Axis-aligned box B-Rep (the m1 unit-cube convention: per-face DIRECTED
/// edge copies, all loop edges forward).
fn box_brep(origin: [f64; 3], size: [f64; 3]) -> BRep {
    let (verts, edges, faces) = box_parts(origin, size);
    BRep::new(verts, edges, faces).expect("box brep")
}

fn box_parts(origin: [f64; 3], size: [f64; 3]) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let [x, y, z] = origin;
    let [sx, sy, sz] = size;
    let pts = [
        [x, y, z],
        [x + sx, y, z],
        [x + sx, y + sy, z],
        [x, y + sy, z],
        [x, y, z + sz],
        [x + sx, y, z + sz],
        [x + sx, y + sy, z + sz],
        [x, y + sy, z + sz],
    ];
    let verts: Vec<BRepVertex> = pts
        .iter()
        .map(|&[a, b, c]| BRepVertex { point: p(a, b, c) })
        .collect();
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (−z)
        [4, 7, 6, 5], // top (+z)
        [0, 4, 5, 1], // front (−y)
        [1, 5, 6, 2], // right (+x)
        [2, 6, 7, 3], // back (+y)
        [3, 7, 4, 0], // left (−x)
    ];
    let normals = [
        v3(0.0, 0.0, -1.0),
        v3(0.0, 0.0, 1.0),
        v3(0.0, -1.0, 0.0),
        v3(1.0, 0.0, 0.0),
        v3(0.0, 1.0, 0.0),
        v3(-1.0, 0.0, 0.0),
    ];
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (i, vs) in face_verts.iter().enumerate() {
        let base = edges.len() as u32;
        for k in 0..4 {
            edges.push(BRepEdge {
                start: vs[k],
                end: vs[(k + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        // Plane d via a vertex on the face: n·x + d = 0.
        let n = normals[i];
        let pv = pts[vs[0] as usize];
        let d = -(n.x() * pv[0] + n.y() * pv[1] + n.z() * pv[2]);
        faces.push(BRepFace {
            surface: Surface::Plane { normal: n, d },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: vec![],
            reversed: false,
        });
    }
    (verts, edges, faces)
}

// =========================================================================
// Oracle helpers
// =========================================================================

fn mesh_signed_volume(mesh: &yang_rs::Mesh) -> f64 {
    let mut six_v = 0.0;
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Watertight 2-manifold: every undirected edge used by exactly two
/// triangles, in opposite directions.
fn assert_watertight(mesh: &yang_rs::Mesh, what: &str) {
    use std::collections::BTreeMap;
    let mut dir: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    for t in &mesh.tris {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            *dir.entry((t[i], t[j])).or_insert(0) += 1;
            *dir.entry((t[j], t[i])).or_insert(0) -= 1;
        }
    }
    let unpaired = dir.values().filter(|&&c| c != 0).count();
    assert_eq!(unpaired, 0, "{what}: {unpaired} unpaired directed edges");
}

fn euler(mesh: &yang_rs::Mesh) -> i64 {
    let mut undirected: BTreeSet<(u32, u32)> = BTreeSet::new();
    for t in &mesh.tris {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (a, c) = (t[i].min(t[j]), t[i].max(t[j]));
            undirected.insert((a, c));
        }
    }
    mesh.num_verts() as i64 - undirected.len() as i64 + mesh.tris.len() as i64
}

// =========================================================================
// 1+2+3+4. Stage-1 ingestion oracles (RED: construction fails today)
// =========================================================================

#[test]
fn partial_revolve_tessellates_watertight_with_pappus_volume() {
    for angle in [PI / 2.0, PI, 350.0_f64.to_radians()] {
        let b = partial_revolve_brep(angle)
            .unwrap_or_else(|e| panic!("partial revolve ({angle}) BRep::new: {e:?}"));
        let mesh = b.as_mesh();
        assert!(!mesh.tris.is_empty());
        assert_watertight(mesh, "partial revolve mesh");
        assert_eq!(euler(mesh), 2, "partial revolve is a sphere-like shell");

        let vol = mesh_signed_volume(mesh);
        assert!(vol > 0.0, "outward orientation at {angle}");
        // Band calibration: inscribed chords under-estimate by ≈ 1 − sin δ/δ
        // per chord angle δ = 2π/N; at the spec's d_ε = 1e-2·AABB-diag the
        // honest deficit for these proportions is ~3.3% — 5% bounds it
        // without admitting a winding/orientation defect (which would show
        // as a SIGN flip or a ≫10% loss).
        let expect = pappus(angle);
        assert!(
            vol <= expect * 1.001 && vol >= 0.95 * expect,
            "volume {vol} vs Pappus {expect} at {angle}"
        );
    }
}

#[test]
fn washer_tessellates_watertight_genus_one() {
    let b = washer_brep().unwrap_or_else(|e| panic!("washer BRep::new: {e:?}"));
    let mesh = b.as_mesh();
    assert_watertight(mesh, "washer mesh");
    assert_eq!(euler(mesh), 0, "washer mesh is a torus (χ = 0)");
    let vol = mesh_signed_volume(mesh);
    let expect = pappus(2.0 * PI);
    assert!(vol > 0.0);
    // Same inscribed-chord band as the partial oracle (see there).
    assert!(
        vol <= expect * 1.001 && vol >= 0.95 * expect,
        "washer volume {vol} vs {expect}"
    );
}

#[test]
fn bijection_round_trip_covers_arc_steiner_vertices() {
    const TOL: f64 = 1e-9;
    let b = partial_revolve_brep(PI).expect("180° revolve");
    let mesh = b.as_mesh();
    let map = b.tessellation_map();
    assert_eq!(map.len(), mesh.num_verts());
    let mut arc_sourced = 0usize;
    for v in 0..mesh.num_verts() as u32 {
        let src = map.lookup(v);
        let recon = b.eval_source(src).as_array();
        let actual = mesh.verts[v as usize].as_array();
        let d = ((recon[0] - actual[0]).powi(2)
            + (recon[1] - actual[1]).powi(2)
            + (recon[2] - actual[2]).powi(2))
        .sqrt();
        assert!(d <= TOL, "vertex {v} ({src:?}): {d}");
        if let yang_rs::TessellationSource::BRepEdge { edge, .. } = src {
            if edge >= 8 {
                arc_sourced += 1;
                // Arc Steiner verts lie exactly on their circle.
                let radius = if edge == 8 || edge == 9 { R1 } else { R2 };
                let r = (actual[1] * actual[1] + actual[2] * actual[2]).sqrt();
                assert!(
                    (r - radius).abs() <= 1e-9,
                    "arc Steiner vertex {v} off circle: r = {r}"
                );
            }
        }
    }
    assert!(
        arc_sourced >= 4,
        "expected arc-sourced Steiner vertices, got {arc_sourced}"
    );
}

// =========================================================================
// 5. End-to-end booleans (native backend; skip when FFI stub)
// =========================================================================

#[test]
fn partial_revolve_union_and_subtract_box() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    let a = partial_revolve_brep(PI / 2.0).expect("90° revolve");
    let va = mesh_signed_volume(a.as_mesh());
    // Box overlapping the outer wall region of the first quadrant sweep.
    let b = box_brep([1.0, 1.5, -0.5], [1.0, 1.0, 1.0]);
    let vb = 1.0;

    let union = yang_rs::boolean(&a, &b, BoolOp::Union, &backend)
        .unwrap_or_else(|e| panic!("revolve ∪ box: {e:?}"));
    let mu = union.as_mesh();
    assert_watertight(mu, "revolve ∪ box");
    let vu = mesh_signed_volume(mu);
    assert!(
        vu > va.max(vb) - 1e-9 && vu < va + vb + 1e-9,
        "union volume {vu} outside sandwich ({va}, {vb})"
    );

    let cut = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend)
        .unwrap_or_else(|e| panic!("revolve − box: {e:?}"));
    let mc = cut.as_mesh();
    assert_watertight(mc, "revolve − box");
    let vc = mesh_signed_volume(mc);
    assert!(vc > 0.0 && vc < va, "cut volume {vc} vs operand {va}");
}

#[test]
fn pi_arc_fixture_through_full_pipeline() {
    // The exactly-π input arcs (180° revolve) must flow through unambiguously.
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    let a = partial_revolve_brep(PI).expect("180° revolve");
    let b = box_brep([1.0, 1.5, -0.5], [1.0, 1.0, 1.0]);
    let out = yang_rs::boolean(&a, &b, BoolOp::Union, &backend)
        .unwrap_or_else(|e| panic!("π-arc union: {e:?}"));
    assert_watertight(out.as_mesh(), "π-arc union");
}

#[test]
fn washer_subtract_box_propagates_reversed_cavity_walls() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    let a = washer_brep().expect("washer");
    // Box that bites into the washer from outside (crosses the outer tube).
    let b = box_brep([1.0, 1.5, -0.5], [1.0, 1.0, 1.0]);
    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend)
        .unwrap_or_else(|e| panic!("washer − box: {e:?}"));
    assert_watertight(out.as_mesh(), "washer − box");
    // The washer's own inner tube survives the cut and must KEEP its
    // cavity sense in the output (Stage-6 reversed propagation:
    // input.reversed XOR subtract-B, here input A's reversed=true rides
    // through). Without the fix it comes back reversed: false.
    let kept_reversed_cylinders = out
        .faces()
        .iter()
        .filter(
            |f| matches!(f.surface, Surface::Cylinder { radius, .. } if (radius - R1).abs() < 1e-9),
        )
        .filter(|f| f.reversed)
        .count();
    assert!(
        kept_reversed_cylinders > 0,
        "washer inner-tube output patches must keep reversed: true"
    );
}

// =========================================================================
// 6. Adversary: loud rejections
// =========================================================================

#[test]
fn arc_endpoint_off_circle_is_loud() {
    // Same 90° fixture but vertex 4 (ring1[0]) is nudged off the circle.
    let angle = PI / 2.0;
    let prof = [(0.0, R1), (H, R1), (H, R2), (0.0, R2)];
    let mut verts: Vec<BRepVertex> = prof
        .iter()
        .map(|&(x, y)| BRepVertex {
            point: p(x, y, 0.0),
        })
        .collect();
    for &(x, y) in &prof {
        let (yy, zz) = rot(y, 0.0, angle);
        verts.push(BRepVertex {
            point: p(x, yy, zz),
        });
    }
    verts[4].point = p(0.0, 0.1, 1.2); // r ≈ 1.204 ≠ R1

    let edges = vec![BRepEdge {
        start: 0,
        end: 4,
        curve: Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: v3(1.0, 0.0, 0.0),
            radius: R1,
        },
    }];
    // A face referencing the bad arc — the endpoint-on-circle validation
    // must fire loudly at BRep::new regardless of the face's other defects.
    let faces = vec![BRepFace {
        surface: Surface::Plane {
            normal: v3(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: vec![0],
        inner_loops: vec![],
        reversed: false,
    }];
    let err = BRep::new(verts, edges, faces).expect_err("off-circle arc endpoint");
    assert!(
        matches!(err, yang_rs::YangError::MalformedTopology(_)),
        "loud typed rejection, got {err:?}"
    );
}

#[test]
fn reversed_planar_face_is_loud() {
    let mut b = box_brep_parts();
    b.2[0].reversed = true; // a planar face with reversed=true is malformed
    let err = BRep::new(b.0, b.1, b.2).expect_err("reversed planar face");
    assert!(
        matches!(err, yang_rs::YangError::MalformedTopology(_)),
        "loud typed rejection, got {err:?}"
    );
}

/// The box fixture's raw parts (for adversarial mutation before BRep::new).
fn box_brep_parts() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    box_parts([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
}

//! N2-3a ADVERSARY — independent adversarial probes on the Stage-0 exact
//! rim mint (spec `specs/n2_stage4_junction_cluster_merge.md` §3 branch table
//! + fold-validity gate; FIP §6 validation phase).
//!
//! This file is the ADVERSARY half of a role-separated FIP cycle: TESTS ONLY —
//! it never edits production code or the acceptance suite
//! (`n2_junction_cluster.rs`). Per the repo convention that integration-test
//! files cannot share helpers, the BRep constructors and mesh oracles are
//! independently-typed copies of the established hand-built patterns
//! (yr13 / n2_junction_cluster / m8_disc_coplanar), not imports.
//!
//! ## Adversarial contract
//!
//! Every probe asserts EITHER-VALID-OR-LOUD: an `Ok` output must be
//! watertight, have the pinned Euler characteristic, positive volume, retain
//! ≥ 1 analytic `Surface::Cylinder` face, and carry NO loop vertex off its
//! face's analytic surface beyond the kernel import band
//! `1e-9·(1 + max(r, ‖p‖∞))` — the exact predicate kernel-v2's debug
//! tripwire measures, i.e. what "silent-wrong" means for this increment. An
//! `Err` must be a typed `YangError` (loud). Panics are never acceptable.
//!
//! ## Probes (all values MEASURED at 15d00e7f before pinning)
//!
//! 1. **Near-tangency sweep** (`near_tangency_sweep_valid_or_loud`): the
//!    acceptance fixture's tangency penetration δ at 1e-2·r / 1e-4·r /
//!    1e-6·r. At 1e-2·r the box side plane crosses rim chords (single body,
//!    χ = 2, junction vertex exact to 1e-20). At 1e-4·r and below the
//!    penetration is smaller than the N=13 chord sagitta r·(1−cos(π/13)) ≈
//!    6.2e-6 AND smaller than r−max(sample·x̂) ≈ 1.54e-6, so the tessellated
//!    solids are disjoint: the union is honestly two watertight shells
//!    (χ = 4) — inherent Stage-1 chord-tolerance behavior, NOT an N2-3a
//!    defect (any sub-sagitta penetration is below the pipeline's documented
//!    d_ε). All three: fully on-band.
//! 2. **Exact tangency + just-past** (`exact_tangency_and_just_past`): side
//!    plane exactly AT x = r (any claimed crossing would have exact
//!    discriminant 0) and at r + 1e-6·r (disjoint): two watertight shells,
//!    χ = 4, on-band.
//! 3. **Crossing through an existing rim sample**
//!    (`crossing_through_existing_rim_sample`): self-calibrated — a first
//!    union locates a REAL Stage-1 rim-ring vertex (az ≈ 31.62° under the
//!    always-on rim refinement, 2026-08-14 flip census; ring samples are
//!    pair-dependent since the flip), then the box side plane is rebuilt
//!    bit-exactly through its x coordinate. Measured at the flip census:
//!    the coincidence class dead-ends in a LOUD typed
//!    `Stage4RegionInvalid`/`LocalRefinementRequired` (pre-flip: Ok with
//!    the sample surviving) — pinned as valid-or-loud with the Ok arm's
//!    full oracle retained so a capability gain flips the pin loudly. The
//!    1-ULP-inside variant now builds VALID (pre-flip it was a loud
//!    `NonManifoldOutput`) — a flip capability gain.
//! 4. **Fold-gate fixture** (`fold_gate_revert_is_contained_and_deterministic`):
//!    coarse rim (r = 1, h = 10 → Stage-1 picks N = 7) with a prism edge
//!    inside the pre-flip chord↔arc band — the measured R0013 folding
//!    mechanism. Since the 2026-08-14 flip the refined ring removes the
//!    coarse sagitta and ZERO fold-gate events fire here (the gate + repair
//!    ladder remain corpus-exercised, e.g. F0067's flush pairs); the pins
//!    that survive the mechanism's retirement: fully-valid output, the
//!    prism design corner bit-verbatim, bitwise determinism.
//! 5. **Extreme magnitudes** (`extreme_magnitudes_valid_or_loud`): the
//!    acceptance fixture ×1e6 (valid: χ = 2, worst residual 2.8e-14 ≪ band
//!    2.1e-7) and ×1e-2 (LOUD today: `NonManifoldOutput` — a scale-dependent
//!    wall, acceptable per P9 but pinned so it can never go silent-wrong).
//!    At ×1e-4 the BOX INPUT itself is rejected loudly by `BRep::new`
//!    (`DegenerateFace { face: 0 }`, extents ~2e-8) — pinned: the wall is in
//!    input tessellation, before the boolean.
//! 6. **Multiple crossings on one chord** (`multiple_crossings_same_chord`):
//!    box narrower (0.2) than the N=13 rim chord span (~0.46), both side
//!    planes crossing the SAME chord (az 90°→117.7°); all four circle∩line
//!    junctions must be present exactly.
//! 7. **No-panic + determinism sweep** (`no_panic_and_determinism_sweep`):
//!    every fixture above run twice — bitwise-identical outputs (or
//!    identical typed errors), never a panic.
//!
//! ## Mutation sanity (FIP §6.3 — executed before finalizing, all reverted)
//!
//! (a) fold gate disabled, (b) crossing branch → plain radial projection,
//! (c) minted-index tracking replaced by coordinate-comparison inference,
//! (d) rim-chord classification disabled. Kill matrix in the cycle report;
//! no mutation survives the union of this suite, the acceptance suite, the
//! m8 campaign pins, and m8_disc_coplanar.

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::{HashMap, HashSet};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface, YangError};

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
// Mesh oracles (pattern of n2_junction_cluster.rs).
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

/// The kernel-v2 import band (`crates/kernel-v2/src/validate.rs::import_band`):
/// `1e-9 · (1 + max(scale_feature, ‖p‖∞))`. `scale_feature` is the surface's
/// own size parameter (cylinder radius; 0 for a plane).
fn import_band(scale_feature: f64, pt: [f64; 3]) -> f64 {
    let m = pt[0].abs().max(pt[1].abs()).max(pt[2].abs());
    1e-9 * (1.0 + scale_feature.max(m))
}

/// Off-surface scan over EVERY output face (the acceptance suite's i1
/// pattern, generalized from cylinder-only to every fixture surface): each
/// boundary-loop vertex must lie on its face's analytic `Surface` within the
/// kernel import band. Returns
/// `(off_band_count, max_residual, worst_point, band_at_worst)`. Panics on a
/// surface type these fixtures cannot produce (planes + Z cylinders only) —
/// an unexpected output surface IS an adversarial finding.
fn all_faces_residual_survey(out: &BRep) -> (usize, f64, [f64; 3], f64) {
    let mut worst = (0.0_f64, [0.0; 3], f64::INFINITY);
    let mut off_count = 0usize;
    for f in out.faces() {
        let mut seen: HashSet<u32> = HashSet::new();
        for &e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let e = &out.edges()[e_idx as usize];
            for v in [e.start, e.end] {
                if !seen.insert(v) {
                    continue;
                }
                let pt = out.vertices()[v as usize].point.as_array();
                let (resid, band) = match f.surface {
                    Surface::Plane { normal, d } => {
                        let n = normal.as_array();
                        ((dot(n, pt) + d).abs() / norm(n), import_band(0.0, pt))
                    }
                    Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                    } => {
                        let ap = axis_point.as_array();
                        let au = unit(axis_dir.as_array());
                        let w = sub3(pt, ap);
                        let along = dot(w, au);
                        let radial = sub3(w, scale(au, along));
                        ((norm(radial) - radius).abs(), import_band(radius, pt))
                    }
                    other => panic!(
                        "n2 rim-mint adversary: unexpected output surface {other:?} \
                         (fixtures build only planes and Z cylinders)"
                    ),
                };
                if resid > band {
                    off_count += 1;
                    if std::env::var_os("ADV_DIAG").is_some() {
                        eprintln!("[adv-diag] off-band v{v} p={pt:?} resid={resid:.3e} face surface={:?} edge {e_idx} curve={:?}", f.surface, e.curve);
                    }
                }
                if resid > worst.0 {
                    worst = (resid, pt, band);
                }
            }
        }
    }
    (off_count, worst.0, worst.1, worst.2)
}

// =========================================================================
// BRep constructors (copied from the yr13 / m8_disc_coplanar patterns).
// =========================================================================

/// Axis-aligned box `lo..hi` with correct OUTWARD normals and plane offsets
/// (`n·x + d = 0`), returning `BRep::new`'s `Result` (probe 5 pins the loud
/// input-tessellation wall at tiny absolute scale).
fn try_box_brep(lo: [f64; 3], hi: [f64; 3]) -> Result<BRep, YangError> {
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
    BRep::new(verts, edges, faces)
}

fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    try_box_brep(lo, hi).expect("box_brep: BRep::new failed")
}

/// Arbitrary convex-CCW-profile prism `z0..z1` (copied from the
/// m8_disc_coplanar pattern) — for the fold-gate band-edge footprint, which
/// is not axis-aligned.
fn polygon_prism(profile: &[[f64; 2]], z0: f64, z1: f64) -> BRep {
    let n = profile.len();
    let mut verts: Vec<BRepVertex> = Vec::with_capacity(2 * n);
    for &[x, y] in profile {
        verts.push(BRepVertex { point: p(x, y, z0) });
    }
    for &[x, y] in profile {
        verts.push(BRepVertex { point: p(x, y, z1) });
    }
    let line = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();

    // Bottom cap (normal −z): profile reversed so it reads CCW from below.
    let bottom_order: Vec<u32> = std::iter::once(0u32).chain((1..n as u32).rev()).collect();
    let bb = edges.len() as u32;
    for i in 0..n {
        edges.push(line(bottom_order[i], bottom_order[(i + 1) % n]));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: z0,
        },
        outer_loop: (bb..bb + n as u32).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });
    // Top cap (normal +z): forward.
    let tb = edges.len() as u32;
    for i in 0..n as u32 {
        edges.push(line(n as u32 + i, n as u32 + (i + 1) % n as u32));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -z1,
        },
        outer_loop: (tb..tb + n as u32).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });
    // Side walls.
    for i in 0..n as u32 {
        let (bi, bj) = (i, (i + 1) % n as u32);
        let (ti, tj) = (n as u32 + i, n as u32 + (i + 1) % n as u32);
        let base = edges.len() as u32;
        edges.push(line(bi, bj));
        edges.push(line(bj, tj));
        edges.push(line(tj, ti));
        edges.push(line(ti, bi));
        let a = profile[i as usize];
        let b = profile[((i + 1) % n as u32) as usize];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = (dy / len, -dx / len);
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(nx, ny, 0.0),
                d: -(nx * a[0] + ny * a[1]),
            },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("polygon_prism BRep::new")
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

// =========================================================================
// Fixture geometry — the acceptance fixture's R0072-class values, made
// re-parameterizable in scale `s` and tangency penetration `delta`.
// =========================================================================

/// R0072's cylinder radius / depth, bit-for-bit (acceptance fixture).
const R: f64 = 2.1339062731488812e-4;
const H: f64 = 2.0891191078398327e-4;
/// The acceptance fixture's measured tangency penetration.
const DELTA: f64 = 1.607e-6;
/// Box extents at scale 1 (acceptance fixture values).
const BOX_W: f64 = 2.0e-4;
const BOX_HALF_Y: f64 = 1.0e-4;
/// B's height (shorter than A → its top cap crosses the cylinder interior).
const H_B: f64 = 7.657508571136625e-5;

/// Fixture A at scale `s`: the R0072-class cylinder, axis +Z through the
/// origin, bottom cap on the shared z = 0 sketch plane.
fn cyl_a(s: f64) -> BRep {
    cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], R * s, H * s)
}

/// Fixture B at scale `s`: box with its −x side plane at `x = s·R − delta`
/// (`delta` in ABSOLUTE units: exact tangency is `delta = 0.0`, just-past
/// tangency `delta < 0.0`), BOTTOM cap coplanar same-normal with A's (the
/// shared z = 0 sketch plane), SHORTER than A — the R0072 configuration.
fn box_b_coplanar(s: f64, delta: f64) -> BRep {
    let x_lo = R * s - delta;
    box_brep(
        [x_lo, -BOX_HALF_Y * s, 0.0],
        [x_lo + BOX_W * s, BOX_HALF_Y * s, H_B * s],
    )
}

/// Fold-gate fixture (probe 4): coarse-rim tall cylinder — Stage-1 derives
/// N from the rim-circle AABB diagonal (d_ε = 1e-2·diag ≈ 0.104 at
/// h = 10·r), giving N = 7 and chord sagitta 1−cos(π/7) ≈ 9.7e-2.
fn tall_cyl() -> BRep {
    cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 10.0)
}

/// Fold-gate fixture B: convex prism whose edge (0.98,−0.10)→(0.82,0.52)
/// lies INSIDE the chord↔arc band of the az 38.57°→−12.86° rim chord
/// (endpoint radii 0.985 / 0.971 vs chord radius ≤ 0.951 at those azimuths,
/// arc radius 1), with B-material between the chord and the edge. The
/// x = 0.82 event mint's radial path crosses the edge → BOnly triangle
/// inversion → gate revert (instrumented ground truth: 3 reverts fire).
fn band_edge_prism() -> BRep {
    polygon_prism(
        &[[0.98, -0.10], [0.82, 0.52], [-0.5, 0.3], [-0.5, -0.3]],
        0.0,
        0.4,
    )
}

/// Corner-in-band fixture: box corners (0.94, ±0.3) sit inside the rim
/// circle but OUTSIDE the pre-flip N=7 rim polygon (in the chord↔arc
/// band); the −x edge's circle∩line roots (0.94, ±0.3412) lie OUTSIDE the
/// box's own edge segment. Since the 2026-08-14 flip, membership
/// refinement classifies the corners INSIDE (Overlap) — no crescent, no
/// mints, no phantom junction (see the probe's doc for the census).
fn corner_in_band_box() -> BRep {
    box_brep([0.94, -0.3, 0.0], [3.0, 0.3, 0.4])
}

/// Multiple-crossings fixture: unit cylinder (N = 13, chord span ≈ 0.46) and
/// a 0.2-wide box strip whose BOTH side planes cross the SAME rim chord
/// (az 90° → 117.69°).
fn unit_cyl() -> BRep {
    cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0)
}
fn narrow_strip_box() -> BRep {
    box_brep([-0.3, -1.5, 0.0], [-0.1, 1.5, 0.4])
}

/// Union through the PUBLIC pipeline with the PRODUCTION native backend,
/// returning the `Result` (errors are DATA under the valid-or-loud contract).
fn try_union(a: BRep, b: BRep) -> Result<BRep, YangError> {
    let backend = yang_rs::native_backend()
        .expect("native backend is always Some since PR-CR-M7c (pure Rust, WASM-clean)");
    boolean(&a, &b, BoolOp::Union, &backend)
}

/// The either-valid-or-loud contract (see file header). `chi_ok` pins the
/// Euler characteristic (2 = one shell; 4 = two watertight shells, legal
/// only for the documented sub-sagitta / disjoint fixtures).
fn assert_valid_or_loud(
    r: Result<BRep, YangError>,
    chi_ok: &[i64],
    label: &str,
) -> Result<BRep, YangError> {
    // A loud typed error satisfies the contract as-is; only Ok is audited.
    let out = r?;
    let mesh = out.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "{label}: Ok output must be watertight"
    );
    let chi = euler_characteristic(mesh);
    assert!(
        chi_ok.contains(&chi),
        "{label}: Ok output Euler characteristic {chi} not in {chi_ok:?}"
    );
    assert!(
        signed_volume(mesh) > 0.0,
        "{label}: Ok output volume must be positive"
    );
    assert!(
        out.faces()
            .iter()
            .any(|f| matches!(f.surface, Surface::Cylinder { .. })),
        "{label}: union output must retain ≥1 analytic Surface::Cylinder face"
    );
    let (off_count, max_resid, worst_pt, band) = all_faces_residual_survey(&out);
    assert!(
        off_count == 0,
        "{label}: SILENT-WRONG — {off_count} loop vertices off their face's \
         analytic surface on an Ok output (worst residual {max_resid:.6e} > \
         band {band:.6e} at p = {worst_pt:?}); the contract is fully-valid or \
         loud typed error, never off-band geometry on Ok"
    );
    Ok(out)
}

/// Assert an output vertex exists within the import band of analytic point
/// `q` (the i2 pattern: exact junctions must survive as vertices).
fn assert_vertex_at(out: &BRep, q: [f64; 3], feature: f64, label: &str) {
    let band_q = import_band(feature, q);
    let dmin = out
        .vertices()
        .iter()
        .map(|v| norm(sub3(v.point.as_array(), q)))
        .fold(f64::INFINITY, f64::min);
    assert!(
        dmin <= band_q,
        "{label}: no output vertex at the analytic junction {q:?} \
         (nearest {dmin:.3e} > band {band_q:.3e})"
    );
}

/// Byte-level output signature for determinism comparisons (mesh vertex
/// bits + triangle indices + face surfaces; error Debug string for `Err`).
fn signature(r: &Result<BRep, YangError>) -> String {
    match r {
        Ok(out) => {
            let mesh = out.as_mesh();
            let mut s = String::new();
            for v in &mesh.verts {
                let a = v.as_array();
                s.push_str(&format!(
                    "v {:016x} {:016x} {:016x};",
                    a[0].to_bits(),
                    a[1].to_bits(),
                    a[2].to_bits()
                ));
            }
            for t in &mesh.tris {
                s.push_str(&format!("t {} {} {};", t[0], t[1], t[2]));
            }
            for f in out.faces() {
                s.push_str(&format!("f {:?} {};", f.surface, f.reversed));
            }
            s
        }
        Err(e) => format!("ERR {e:?}"),
    }
}

/// Self-calibration for probe 3: locate a REAL bottom-rim ring sample of the
/// R0072-class cylinder from a baseline union output (z = 0, +y side,
/// x > 0, on the rim circle within the import band; the max-x such vertex).
/// Under the always-on M8 rim membership refinement (2026-08-14 corpus
/// flip) the ring carries exact on-circle refinement samples in addition to
/// the N = 13 uniform ones; the max-x +y sample is now the az ≈ 31.62°
/// refinement sample — measured
/// (1.8170833687781708e-4, 1.1188226014404256e-4, 0). Deterministic: the
/// ring sampling depends only on A's own rim circles and the pair's
/// refinement demands.
fn calibrated_rim_sample() -> [f64; 3] {
    let out = try_union(cyl_a(1.0), box_b_coplanar(1.0, DELTA))
        .expect("baseline acceptance-class union must build (acceptance suite is green)");
    let mut sample = None::<[f64; 3]>;
    for v in out.vertices() {
        let a = v.point.as_array();
        if a[2] == 0.0 && a[0] > 0.0 && a[1] > 0.5 * R {
            let rad = (a[0] * a[0] + a[1] * a[1]).sqrt();
            if (rad - R).abs() < import_band(R, a) && sample.map(|s| a[0] > s[0]).unwrap_or(true) {
                sample = Some(a);
            }
        }
    }
    let s = sample.expect("baseline output must contain a +y bottom-rim ring sample");
    // Sanity: the calibrated sample is the az ≈ 31.62° refinement vertex
    // (measured at the 2026-08-14 flip census; the pre-flip N = 13 uniform
    // sample was az ≈ 34.62°).
    let az = s[1].atan2(s[0]).to_degrees();
    assert!(
        (az - 31.62).abs() < 0.1,
        "calibration drifted: expected the az≈31.62° ring sample, got az={az:.2}° at {s:?}"
    );
    s
}

// =========================================================================
// Probe 1 — near-tangency sweep.
// =========================================================================

/// δ = 1e-2·r / 1e-4·r / 1e-6·r relative tangency penetrations: each either
/// fully oracle-valid or loud. Measured at HEAD: all three Ok and fully
/// on-band; 1e-2·r is a single χ=2 body with the rim∩plane junction vertex
/// EXACT (1e-20); 1e-4·r and 1e-6·r are honestly-disjoint two-shell unions
/// (χ = 4) because the penetration is below both the N=13 chord sagitta
/// (≈ 2.9e-2·r) and the polygon max-extent gap r·(1−cos(6.92°)) ≈ 7.3e-3·r —
/// sub-tessellation penetration is the pipeline's documented d_ε regime.
#[test]
fn near_tangency_sweep_valid_or_loud() {
    for (delta_rel, chi_ok) in [(1e-2, &[2i64][..]), (1e-4, &[4][..]), (1e-6, &[4][..])] {
        let delta = delta_rel * R;
        let label = format!("near-tangency δ={delta_rel:.0e}·r");
        let r = try_union(cyl_a(1.0), box_b_coplanar(1.0, delta));
        let out = assert_valid_or_loud(r, chi_ok, &label);
        if delta_rel == 1e-2 {
            // Crossing regime: the exact rim∩plane junction must exist
            // (the circle∩line branch, spec §3 row 4 / invariant I2).
            let out = out.expect(
                "δ=1e-2·r built Ok at HEAD (measured) — a new loud error here \
                                  means the crossing regime regressed",
            );
            let x_j = R - delta;
            let y_star = (R * R - x_j * x_j).sqrt();
            assert_vertex_at(&out, [x_j, y_star, 0.0], R, &label);
        }
    }
}

// =========================================================================
// Probe 2 — exact tangency (discriminant 0) and just-past tangency.
// =========================================================================

/// Side plane exactly AT x = r (any circle∩line test on that line has exact
/// discriminant 0 — the double-root boundary of spec §6) and at r + 1e-6·r
/// (fully disjoint): measured two watertight shells (χ = 4), on-band, no
/// error — and, by contract, a loud typed error would also be acceptable.
/// Never a panic, never silent off-band geometry.
#[test]
fn exact_tangency_and_just_past() {
    for (delta, label) in [
        (0.0, "exact tangency δ=0 (discriminant 0)"),
        (-1e-6 * R, "just-past tangency δ=-1e-6·r"),
    ] {
        let r = try_union(cyl_a(1.0), box_b_coplanar(1.0, delta));
        let _ = assert_valid_or_loud(r, &[4], label);
    }
}

// =========================================================================
// Probe 3 — crossing through an existing rim sample.
// =========================================================================

/// The box side plane rebuilt bit-exactly through a REAL rim-ring sample's x
/// coordinate (self-calibrated az ≈ 34.62° vertex): the overlay's
/// B-edge×chord intersection lands exactly ON the chord endpoint, so the
/// exact-key rim branch and the crossing branch collide. Measured: single
/// body, on-band, and NO duplicate or near-duplicate vertices (min pairwise
/// mesh-vertex distance 1.099e-6 ≫ the 1e-12 crack floor — a duplicate from
/// double-minting would sit within ULPs).
#[test]
fn crossing_through_existing_rim_sample() {
    let s = calibrated_rim_sample();
    let bx = box_brep([s[0], -BOX_HALF_Y, 0.0], [s[0] + BOX_W, BOX_HALF_Y, H_B]);
    // 2026-08-14 flip census (always-on rim refinement): ring samples are
    // now PAIR-DEPENDENT (refinement re-anchors the ring per pair — the
    // pre-flip N = 13 uniform az 34.62° sample no longer exists), so this
    // probe's plane passes bit-exactly through the CALIBRATION pair's
    // sample position — a near-coincidence adversary for THIS pair. At the
    // 08-14 census the outcome was a loud Stage-4 `LocalRefinementRequired`
    // dead-end; 2026-08-19 (spec `yang_n2_stage4_cdt_mesh_updating.md`
    // §5c.13) anchored that STOP as the §4.4.1(a) unzip loop's ABSOLUTE
    // `MIN_FEATURE_SIZE²` degeneracy floor mis-firing at this fixture's
    // 2e-4 scale, and the scale-free identity fix flipped the pin to Ok —
    // which is the contract's other leg: fully oracle-valid, no cracks,
    // and the DESIGN junctions present. Note the geometry: at x = s[0]
    // the circle's |y| = 1.119e-4 EXCEEDS the box's half-width 1e-4, so
    // the side plane never meets the rim inside the box; the design
    // junctions are the box's y = ±BOX_HALF_Y faces × the rim circle at
    // x_j = sqrt(R² − BOX_HALF_Y²) (the sample itself was only ever a
    // pre-refinement RING vertex on the retained arc, not a junction).
    match assert_valid_or_loud(
        try_union(cyl_a(1.0), bx),
        &[2],
        "crossing through rim sample (exact)",
    ) {
        Err(e) => panic!(
            "crossing through rim sample: built Ok since 2026-08-19 (§5c.13 \
             degeneracy identity); a loud error here is a regression: {e:?}"
        ),
        Ok(out) => {
            // The design junctions (box y-faces × rim circle) must exist as
            // vertices (the i2 pattern: exact junctions survive).
            let x_j = (R * R - BOX_HALF_Y * BOX_HALF_Y).sqrt();
            for y in [BOX_HALF_Y, -BOX_HALF_Y] {
                assert_vertex_at(
                    &out,
                    [x_j, y, 0.0],
                    R,
                    "crossing through rim sample (exact): box y-face × rim junction",
                );
            }
            // No cracks from a near-duplicate mint: min pairwise distance
            // stays macroscopic (a double-mint would be < 1e-12).
            let vs: Vec<[f64; 3]> = out.as_mesh().verts.iter().map(|v| v.as_array()).collect();
            let mut dmin = f64::INFINITY;
            for i in 0..vs.len() {
                for j in (i + 1)..vs.len() {
                    dmin = dmin.min(norm(sub3(vs[i], vs[j])));
                }
            }
            assert!(
                dmin > 1e-12,
                "crossing through rim sample: near-duplicate vertices (dmin = {dmin:.3e}) — \
                 the exact-key and crossing branches minted the same point twice"
            );
        }
    }
}

/// The same plane 1 ULP inside the sample: the crossing points land within
/// ULPs of the ring sample. The overlay mints femto-twin split pairs on
/// every chord both event columns cross; Stage-0's sub-floor shared-mint
/// collapse (increment 4, spec m8_holed_disc_coplanar_overlay §8) resolves
/// both twins to ONE shared on-circle target, dissolving the fold and the
/// twin before the arrangement. Measured: single body, χ = 2, fully
/// on-band. (Before increment 4 this was quarantined: the twin wedge was
/// fold-gate reverted and a chord-position vertex reached a cylinder-face
/// loop off-band by the chord sagitta, 6e-6.)
#[test]
fn crossing_one_ulp_inside_rim_sample() {
    let s = calibrated_rim_sample();
    let x_lo = f64::next_down(s[0]);
    let bx = box_brep([x_lo, -BOX_HALF_Y, 0.0], [x_lo + BOX_W, BOX_HALF_Y, H_B]);
    let _ = assert_valid_or_loud(
        try_union(cyl_a(1.0), bx),
        &[2, 4],
        "crossing 1 ULP inside rim sample",
    );
}

// =========================================================================
// Probe 4 — fold-gate exercise on a coarse rim.
// =========================================================================

/// The R0013 folding mechanism reproduced in a controlled fixture (see
/// `band_edge_prism`): the gate MUST fire (instrumented ground truth: 3
/// reverts), the output must stay watertight / χ = 2 / fully on-band (the
/// reverts are contained — no off-band vertex reaches a boundary loop), and
/// repeat runs must be bitwise identical.
///
/// The positive revert detector: the x = 0.82 event split of the
/// az 38.57°→−12.86° rim chord survives in the output mesh AT ITS CHORD
/// POSITION (measured (0.82, 0.4562626001437506, 0), radius ≈ 0.9385) —
/// exact minting would have placed it ON the circle (radius 1), so finding
/// it strictly inside, collinear with the chord, proves the gate reverted
/// the mint. Tolerances: the x-event coordinate is exactly the prism
/// corner's 0.82 (rational split of f64 chord endpoints → ≤ 1 ULP in y);
/// 1e-9 absolute at unit scale ≫ ULP yet ≪ the 6e-2 mint displacement.
#[test]
fn fold_gate_revert_is_contained_and_deterministic() {
    // 2026-08-14 flip census (always-on rim refinement): the coarse-chord
    // fold mechanism this fixture was built to trigger NO LONGER ARISES —
    // the refined ring's sub-chord sagitta is too small to invert anything,
    // and zero fold-gate events fire (measured with YANG_SPLIT_PROBE; the
    // pre-flip run reverted 3 mints here). The prism corner (0.82, 0.52),
    // rad ≈ 0.971 inside the exact circle, now classifies Overlap (interior
    // content) instead of landing outside the coarse polygon in a sag
    // crescent. The gate + repair ladder remain corpus-exercised (F0067's
    // flush pairs). Pins that survive the mechanism's retirement: the
    // output is fully valid, the prism's design corner is IMMOVABLE
    // (survives verbatim — never welded, merged, or relocated), and the
    // result is bitwise deterministic.
    let r1 = try_union(tall_cyl(), band_edge_prism());
    let out = assert_valid_or_loud(r1, &[2], "fold-gate band-edge prism")
        .expect("fold-gate fixture built Ok at HEAD (measured)");

    let corner_exact = out
        .as_mesh()
        .verts
        .iter()
        .map(|v| v.as_array())
        .any(|a| a == [0.82, 0.52, 0.0]);
    assert!(
        corner_exact,
        "fold-gate fixture: the prism design corner (0.82, 0.52, 0) must \
         survive bit-verbatim (design vertices are immovable)"
    );

    // Determinism of the result (A4.2 / spec I6): bitwise identical.
    let r2 = try_union(tall_cyl(), band_edge_prism());
    assert_eq!(
        signature(&Ok(out)),
        signature(&r2),
        "fold-gate fixture: repeat run differs — Stage-0 resolution is not \
         deterministic"
    );
}

// =========================================================================
// Probe 4b — corner-in-band: circle∩line roots outside the B edge segment.
// =========================================================================

/// Box corners (0.94, ±0.3) inside the circle but outside the rim polygon
/// (the chord↔arc sag-crescent class). 2026-08-14 flip census (always-on
/// rim refinement): membership refinement subdivides the ring until the
/// polygon agrees with the exact circle, so the corners classify INSIDE
/// (Overlap) and the pre-flip behavior — crossing-branch mints at the
/// circle∩line roots OUTSIDE the B edge's own segment, gate-reverted, with
/// Stage-4's line relocation installing the UNBOUNDED junction
/// (0.94, −√(1−0.94²), 0) as an output vertex — no longer occurs. That
/// extrapolated junction was the F0067 wheel-corner defect class (spec
/// `yang_441_trim_cdt_construction` §4-I1d/J1: a junction outside the kept
/// footprint); its absence is the cure, not a loss. The REAL in-segment
/// junctions — the y = ±0.3 edge lines crossing the circle at
/// x = √(1−0.09) — must be output vertices, the box corner survives
/// verbatim (design vertices are immovable), and the output is fully
/// valid. Measured: χ = 2, on-band, volume 3.0508e1 (the refined cap
/// rounds closer to the true circle than the pre-flip 2.9833e1).
#[test]
fn corner_in_band_refines_membership_no_phantom_junction() {
    let out = assert_valid_or_loud(
        try_union(tall_cyl(), corner_in_band_box()),
        &[2],
        "corner-in-band",
    )
    .expect("corner-in-band fixture built Ok at HEAD (measured)");
    let x_j = (1.0_f64 - 0.3 * 0.3).sqrt();
    assert_vertex_at(&out, [x_j, -0.3, 0.0], 1.0, "corner-in-band -y junction");
    assert_vertex_at(&out, [x_j, 0.3, 0.0], 1.0, "corner-in-band +y junction");
    assert_vertex_at(&out, [0.94, -0.3, 0.0], 1.0, "corner-in-band box corner");
    let vol = signed_volume(out.as_mesh());
    assert!(
        (30.0..31.0).contains(&vol),
        "corner-in-band: volume {vol:.4} outside the measured plausibility band \
         (30.0..31.0; the refined bottom cap rounds toward π·r²·h + the box tail)"
    );
}

// =========================================================================
// Probe 5 — extreme magnitudes.
// =========================================================================

/// The acceptance-class fixture far from unit scale (the repo has a history
/// of absolute-epsilon traps — TAU_WORK class):
/// - ×1e6: valid single body, fully on-band (measured worst residual
///   2.8e-14 vs band 2.1e-7);
/// - ×1e-2: a LOUD `NonManifoldOutput` today (scale-dependent wall,
///   acceptable per P9; pinned so it can never become silent-wrong);
/// - ×1e-4: the box INPUT is rejected loudly by `BRep::new`
///   (`DegenerateFace`, extents ~2e-8) — the wall is input tessellation,
///   before the boolean; the cylinder at the same scale constructs.
#[test]
fn extreme_magnitudes_valid_or_loud() {
    // ×1e6
    let s = 1e6;
    let r = try_union(cyl_a(s), box_b_coplanar(s, DELTA * s));
    let out = assert_valid_or_loud(r, &[2], "scale ×1e6");
    let out = out.expect("×1e6 built Ok at HEAD (measured)");
    let x_j = R * s - DELTA * s;
    let y_star = ((R * s) * (R * s) - x_j * x_j).sqrt();
    assert_vertex_at(&out, [x_j, y_star, 0.0], R * s, "scale ×1e6 junction");

    // ×1e-2
    let s = 1e-2;
    let _ = assert_valid_or_loud(
        try_union(cyl_a(s), box_b_coplanar(s, DELTA * s)),
        &[2, 4],
        "scale ×1e-2",
    );

    // ×1e-4: loud input-construction wall (box); cylinder constructs.
    let s = 1e-4;
    let x_lo = R * s - DELTA * s;
    let box_result = try_box_brep(
        [x_lo, -BOX_HALF_Y * s, 0.0],
        [x_lo + BOX_W * s, BOX_HALF_Y * s, H_B * s],
    );
    assert!(
        box_result.is_err(),
        "scale ×1e-4: the ~2e-8-extent box unexpectedly constructed — the \
         BRep::new degenerate-face wall moved; re-measure the boolean at this \
         scale and extend the sweep"
    );
    let _ = cyl_a(s); // must still construct (panics loudly if not)
}

// =========================================================================
// Probe 6 — multiple crossings on one chord.
// =========================================================================

/// Both side planes of a 0.2-wide strip cross the SAME N=13 rim chord
/// (az 90°→117.69°, x-span ≈ 0.46): all four circle∩line junctions
/// (x = −0.1 and x = −0.3, ±y) must be present exactly, output valid.
#[test]
fn multiple_crossings_same_chord() {
    let out = assert_valid_or_loud(
        try_union(unit_cyl(), narrow_strip_box()),
        &[2],
        "multiple crossings on one chord",
    )
    .expect("narrow-strip fixture built Ok at HEAD (measured)");
    for x in [-0.1, -0.3] {
        let y = (1.0_f64 - x * x).sqrt();
        for yy in [y, -y] {
            assert_vertex_at(
                &out,
                [x, yy, 0.0],
                1.0,
                &format!("same-chord junction at x={x}, y={yy:.4}"),
            );
        }
    }
}

// =========================================================================
// Probe 7 — no-panic + determinism sweep over every fixture.
// =========================================================================

/// Every fixture above, run twice through the public pipeline: bitwise
/// identical outputs (or identical typed errors). A panic anywhere fails
/// the test. (spade/cherchi are deterministic; the fold gate iterates to a
/// fixpoint in a fixed triangle order — spec I6.)
#[test]
fn no_panic_and_determinism_sweep() {
    let sample_x = calibrated_rim_sample()[0];
    type FixturePair = Box<dyn Fn() -> (BRep, BRep)>;
    let fixtures: Vec<(&str, FixturePair)> = vec![
        (
            "δ=1e-2·r",
            Box::new(|| (cyl_a(1.0), box_b_coplanar(1.0, 1e-2 * R))),
        ),
        (
            "δ=1e-4·r",
            Box::new(|| (cyl_a(1.0), box_b_coplanar(1.0, 1e-4 * R))),
        ),
        (
            "δ=1e-6·r",
            Box::new(|| (cyl_a(1.0), box_b_coplanar(1.0, 1e-6 * R))),
        ),
        ("δ=0", Box::new(|| (cyl_a(1.0), box_b_coplanar(1.0, 0.0)))),
        (
            "δ=-1e-6·r",
            Box::new(|| (cyl_a(1.0), box_b_coplanar(1.0, -1e-6 * R))),
        ),
        (
            "×1e6",
            Box::new(|| (cyl_a(1e6), box_b_coplanar(1e6, DELTA * 1e6))),
        ),
        (
            "×1e-2",
            Box::new(|| (cyl_a(1e-2), box_b_coplanar(1e-2, DELTA * 1e-2))),
        ),
        ("fold-gate", Box::new(|| (tall_cyl(), band_edge_prism()))),
        (
            "corner-in-band",
            Box::new(|| (tall_cyl(), corner_in_band_box())),
        ),
        ("same-chord", Box::new(|| (unit_cyl(), narrow_strip_box()))),
        (
            "sample-exact",
            Box::new(move || {
                (
                    cyl_a(1.0),
                    box_brep(
                        [sample_x, -BOX_HALF_Y, 0.0],
                        [sample_x + BOX_W, BOX_HALF_Y, H_B],
                    ),
                )
            }),
        ),
        (
            "sample-1ulp",
            Box::new(move || {
                let x = f64::next_down(sample_x);
                (
                    cyl_a(1.0),
                    box_brep([x, -BOX_HALF_Y, 0.0], [x + BOX_W, BOX_HALF_Y, H_B]),
                )
            }),
        ),
    ];
    for (label, build) in fixtures {
        let (a1, b1) = build();
        let (a2, b2) = build();
        let s1 = signature(&try_union(a1, b1));
        let s2 = signature(&try_union(a2, b2));
        assert_eq!(s1, s2, "{label}: repeat runs are not bitwise identical");
    }
}

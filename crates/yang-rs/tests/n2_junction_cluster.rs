//! N2-3a RED — Stage-4 Fig-11(b) junction-cluster merge onto the relocated
//! point q (spec: `specs/n2_stage4_junction_cluster_merge.md`).
//!
//! Mechanism-level reproduction of the R0072 class through the PUBLIC
//! `yang_rs::boolean()` with the production native backend, at the REAL
//! defect scale (reproduce-at-actual-scale rule; corpus is unit-ish and
//! masks scale-dependent bands). The fixture mirrors R0072's meta exactly:
//!
//! - Solid A: cylinder, r = 2.1339062731488812e-4, depth 2.0891191078398327e-4
//!   (R0072 op 1), axis +Z through the origin.
//! - Solid B: box (R0072 op 2's rectangle boss) whose −x side plane is
//!   NEAR-TANGENT to the cylinder (plane x = r − δ, δ = 1.607e-6 — the
//!   penetration the diagnostic measured; tangency amplification along the
//!   cap trace y*/δ ≈ 16, the ~12.8×-class regime), whose BOTTOM cap is
//!   COPLANAR SAME-NORMAL with A's bottom cap (the shared z=0 sketch plane,
//!   bit-identical fields), and which is SHORTER than A (depth
//!   7.657508571136625e-5, R0072 op 2).
//!
//! ## Measured RED population (Test-Author phase, 2026-07-02)
//!
//! Today's union output carries **11 off-surface cylinder-face boundary-loop
//! vertices** (residuals 5.5e-6..6.2e-6 ≈ the Stage-1 chord sagitta for the
//! N=13 rim, vs an import band ≈ 1.0e-9), ALL on the SHARED z=0 plane's rim
//! and spread around the ENTIRE rim (azimuths 16°..−32°) — they are
//! Stage-0-coplanar-overlay–inserted rim vertices left at their CHORD
//! positions, never relocated onto the rim circle (the rim's mesh edges are
//! same-input `input0 == input1` boundaries that `build_intersection_curves`
//! skips, so Stage 4 never claims their vertices). The REAL R0072 replay
//! shows the same signature: 12 off-surface loop vertices across both rims
//! (11 at chord-sagitta residuals 5.4e-6..7.3e-6 + the diagnostic's v7
//! tangency-cluster member at 1.607e-6).
//!
//! **Spec-scope note (must-read for the Implementer):** the spec §3 branch
//! table (merge clusters within `band(q)` of a relocated LINE endpoint)
//! reaches the v7-class tangency members but NOT this dominant whole-rim
//! chord-vertex population — no relocated line endpoint is near azimuth
//! −90°. These tests assert the spec's ACCEPTANCE INVARIANT I1 (§4: every
//! loop vertex on its face's surface within the kernel import band — the
//! exact kernel-v2 tripwire predicate, and what R0072-green requires), not
//! the branch table. Making them green requires handling the overlay-rim
//! class as well (e.g. relocating Stage-0-inserted rim vertices onto the
//! rim circle — same Fig-11 "boundary curves map to boundary curves"
//! requirement), or a re-scoped spec.
//!
//! Oracles (spec §4/§5):
//! - I1 `i1_cylinder_face_loop_vertices_on_surface` — **RED today** (11
//!   off-band vertices, worst 6.2e-6 > band 1.0e-9).
//! - I2 `i2_exact_junction_vertex_survives` — GREEN today; the rim∩plane
//!   junction at (r−δ, +y*, 0) exists as an output vertex EXACTLY on the
//!   analytic triple point (measured 0.0 distance); the fix must keep it
//!   (the merge is ONTO q, never away from it). (The −y* twin has NO nearby
//!   vertex today — nearest 5.1e-5 — an asymmetry of the N=13 rim sampling
//!   vs the crossings; its post-fix form is covered by I1, not pinned here.)
//! - I3 `pins_watertight_euler_volume` — GREEN today; pins that the fix
//!   cannot regress watertightness / χ=2 / plausible volume.
//! - I4 `i4_locality_noncoplanar_tangent_all_on_surface` — GREEN today; the
//!   SAME near-tangent side plane with NO coplanar cap pair (B protrudes
//!   past both caps; cylinder depth 5.7655e-4 — the one clearance geometry
//!   measured to build; see `H_CLEAR`) yields a FULLY exact output (max
//!   residual 2.7e-20). Pins the no-op path: every rim crossing there is a
//!   classified relocation endpoint and must stay untouched (spec I4/I5).
//! - I6 `i6_determinism` — GREEN today; two runs of the RED-class fixture
//!   are byte-identical (A4.2).
//!
//! BRep constructors copied from the established hand-built patterns
//! (`yr13_subtract_cylinder.rs` box_brep / cylinder_brep; integration tests
//! cannot see `#[cfg(test)]` lib items, so they are local).

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::{HashMap, HashSet};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

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
// Mesh oracles (pattern of end_to_end.rs / yr13).
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

// =========================================================================
// Fixture geometry — the R0072 class at its REAL scale.
// =========================================================================

/// R0072's cylinder radius, bit-for-bit (assay meta `profile_size` op 1 /
/// diagnostic 2026-07-02).
const R: f64 = 2.1339062731488812e-4;
/// R0072's cylinder extrude depth (assay meta `depth_or_angle` op 1).
const H: f64 = 2.0891191078398327e-4;
/// Near-tangency penetration: the box side plane sits at x = R − DELTA.
/// This is the measured R0072 penetration (cap-corner pp line 1.607e-6
/// inside the cylinder), which produced the ~12.8× tangency amplification.
const DELTA: f64 = 1.607e-6;
/// Box extents: x-width and y-half-width. The y half-width must exceed the
/// tangency half-chord y* = sqrt(R² − (R−δ)²) ≈ 2.6e-5 so the plane region
/// spans the whole tangency zone; 1e-4 ≫ 2.6e-5. The +x face at
/// R − δ + 2e-4 ≈ 4.1e-4 > R stays clear of the cylinder.
const BOX_W: f64 = 2.0e-4;
const BOX_HALF_Y: f64 = 1.0e-4;
/// B's z-range: the R0072 configuration — BOTTOM cap coplanar same-normal
/// with A's bottom cap (both extruded from the shared z=0 sketch plane,
/// bit-identical plane fields) and B SHORTER than A (R0072's rectangle boss
/// depth 7.657508571136625e-5 < A's 2.089e-4), so B's top cap crosses the
/// cylinder interior in a small arc. The junction cluster the diagnostic
/// measured (v7/v8/v11) lives at the SHARED bottom plane's rim crossing.
const Z_B_LO: f64 = 0.0;
const H_B: f64 = 7.657508571136625e-5;
/// I4-companion box z-range: protrudes past both caps (no coplanar pair).
const CLEAR_Z_LO: f64 = -1.0e-4;
const CLEAR_Z_HI: f64 = 8.0e-4;
/// I4-companion cylinder depth. The companion pins the no-op path for the
/// SAME (R, δ); its depth is the one clearance geometry measured to BUILD —
/// with A at R0072's own depth (2.089e-4, and 3e-4/4e-4) the non-coplanar
/// tangent union dies in a PRE-EXISTING loud `NonManifoldOutput` before
/// Stage 4 (measured 2026-07-02; a different wall, out of this increment's
/// scope). At 5.7655e-4 it builds and is fully exact (max residual 2.7e-20).
const H_CLEAR: f64 = 5.7655e-4;

/// The kernel-v2 import band (`crates/kernel-v2/src/validate.rs::import_band`)
/// — the acceptance band the output loop vertices must meet so kernel-v2's
/// vertex-on-surface tripwire (and its release-mode absence) is satisfied by
/// construction: `1e-9 · (1 + max(radius, ‖p‖∞))`.
fn import_band(radius: f64, pt: [f64; 3]) -> f64 {
    let m = pt[0].abs().max(pt[1].abs()).max(pt[2].abs());
    1e-9 * (1.0 + radius.max(m))
}

/// Axis-aligned box `lo..hi` with correct OUTWARD normals and plane offsets
/// (`n·x + d = 0`). Copied from yr13_subtract_cylinder.rs.
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

/// Closed solid-cylinder B-Rep (seam-edge encoding per yr7 spec §1). Copied
/// from yr13_subtract_cylinder.rs.
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

/// Fixture A: the R0072-scale cylinder, axis +Z through the origin.
fn cyl_a() -> BRep {
    cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], R, H)
}

/// Fixture B (RED class): box with its −x side plane at `x = R − delta`,
/// BOTTOM cap coplanar same-normal with A's (z = 0, the shared sketch
/// plane), SHORTER than A (see H_B doc) — the faithful R0072 configuration.
fn box_b_coplanar(delta: f64) -> BRep {
    let x_lo = R - delta;
    box_brep([x_lo, -BOX_HALF_Y, Z_B_LO], [x_lo + BOX_W, BOX_HALF_Y, H_B])
}

/// Fixture B (I4 companion): the SAME near-tangent side plane, but the caps
/// (z ∈ [CLEAR_Z_LO, CLEAR_Z_HI]) protrude past BOTH of A's caps — no
/// coplanar pair. Every cap∩plane trace is then an ordinary A×B intersection
/// curve and every rim-crossing vertex is a classified relocation endpoint,
/// so the output is fully on-surface TODAY (measured) — the no-op path.
fn box_b_clear(delta: f64) -> BRep {
    let x_lo = R - delta;
    box_brep(
        [x_lo, -BOX_HALF_Y, CLEAR_Z_LO],
        [x_lo + BOX_W, BOX_HALF_Y, CLEAR_Z_HI],
    )
}

/// Union through the PUBLIC pipeline with the PRODUCTION native backend.
fn run_union_with(a: BRep, b: BRep) -> BRep {
    let backend = yang_rs::native_backend()
        .expect("native backend is always Some since PR-CR-M7c (pure Rust, WASM-clean)");
    boolean(&a, &b, BoolOp::Union, &backend)
        .expect("n2 junction-cluster fixture: cylinder ∪ near-tangent box union must be Ok")
}

fn run_union(b: BRep) -> BRep {
    run_union_with(cyl_a(), b)
}

/// Survey the boundary-loop vertices of every output `Surface::Cylinder`
/// face against the analytic cylinder — the EXACT quantity kernel-v2's
/// vertex-on-surface tripwire measures on the imported loops. Returns
/// `(off_band_vertex_count, max_residual, worst_point, band_at_worst)`.
fn cylinder_loop_residual_survey(out: &BRep) -> (usize, f64, [f64; 3], f64) {
    let mut worst = (0.0_f64, [0.0; 3], f64::INFINITY);
    let mut off_count = 0usize;
    let mut cyl_faces = 0usize;
    for f in out.faces() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        cyl_faces += 1;
        let ap = axis_point.as_array();
        let au = unit(axis_dir.as_array());
        let mut seen: HashSet<u32> = HashSet::new();
        for &e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let e = &out.edges()[e_idx as usize];
            for v in [e.start, e.end] {
                if !seen.insert(v) {
                    continue;
                }
                let pt = out.vertices()[v as usize].point.as_array();
                let w = sub3(pt, ap);
                let along = dot(w, au);
                let radial = sub3(w, scale(au, along));
                let resid = (norm(radial) - radius).abs();
                if resid > import_band(radius, pt) {
                    off_count += 1;
                }
                if resid > worst.0 {
                    worst = (resid, pt, import_band(radius, pt));
                }
            }
        }
    }
    assert!(
        cyl_faces > 0,
        "n2 fixture: the union output must retain ≥1 Surface::Cylinder face \
         (analytic survival); faces = {:?}",
        out.faces().iter().map(|f| f.surface).collect::<Vec<_>>()
    );
    (off_count, worst.0, worst.1, worst.2)
}

// =========================================================================
// I1 + I2 — RED today: on-surface output loops; junction vertex at q.
// =========================================================================

/// **RED (the N2-3a defect).** Every boundary-loop vertex of every output
/// `Surface::Cylinder` face lies on the cylinder within the kernel import
/// band — Yang §4.4.1 Fig 11(b): the near-duplicate junction-cluster members
/// must be merged ONTO the relocated on-curve point q, so no vertex survives
/// at a cluster member's off-curve (chord) position. Today the (3c)
/// sub-feature merge collapses the cluster onto its lowest-index OFF-curve
/// member and this assert fails with a ~1e-6 residual (~the Stage-1 chord
/// sagitta) against a ~1e-9 band.
#[test]
fn i1_cylinder_face_loop_vertices_on_surface() {
    let out = run_union(box_b_coplanar(DELTA));
    let (off_count, max_resid, worst_pt, band) = cylinder_loop_residual_survey(&out);
    assert!(
        off_count == 0,
        "N2-3a I1: {off_count} cylinder-face boundary-loop vertices are OFF the \
         analytic cylinder (worst residual {max_resid:.6e} > import band \
         {band:.6e} at p = {worst_pt:?}, r = {R:.17e}). This is the R0072 \
         class: coplanar-overlay rim vertices survive at their Stage-1 CHORD \
         positions instead of being merged onto the relocated on-curve junction \
         point q / the exact rim (Yang §4.4.1 Fig 11(b)). The real R0072 \
         exhibits 12 such vertices across both rims (diagnostic 2026-07-02)."
    );
}

// =========================================================================
// I2 — GREEN pin: the exact junction vertex exists and must survive.
// =========================================================================

/// GREEN today — the rim∩plane junction on the shared plane at
/// (R−δ, +y*, 0) — the point simultaneously on the generator line, the cap
/// plane, and the cylinder — exists as an output vertex EXACTLY on the
/// analytic triple point (measured distance 0.0). Spec I2: the Fig-11(b)
/// merge is ONTO q — it must keep this exact junction vertex, never move it
/// or absorb it into an off-curve neighbor. (The −y* twin junction has no
/// nearby vertex today — nearest 5.1e-5, an N=13 sampling asymmetry; its
/// post-fix form is governed by I1, not pinned here.)
#[test]
fn i2_exact_junction_vertex_survives() {
    let out = run_union(box_b_coplanar(DELTA));
    let x_j = R - DELTA;
    let y_star = (R * R - x_j * x_j).sqrt();
    let q = [x_j, y_star, 0.0];
    let band_q = import_band(R, q);
    let dmin = out
        .vertices()
        .iter()
        .map(|v| norm(sub3(v.point.as_array(), q)))
        .fold(f64::INFINITY, f64::min);
    assert!(
        dmin <= band_q,
        "N2-3a I2: the exact rim∩plane junction vertex q = {q:?} must exist in \
         the output (nearest vertex {dmin:.3e} > band {band_q:.3e}) — the \
         Fig-11(b) merge target is q itself and must never be displaced"
    );
}

// =========================================================================
// I3 — GREEN pin: watertight, χ = 2, positive volume.
// =========================================================================

/// GREEN today — pins that the junction-cluster merge cannot regress the
/// structural gates (R0072's yang output already passes its own watertight
/// gate; the defect is geometric, not topological).
#[test]
fn pins_watertight_euler_volume() {
    let out = run_union(box_b_coplanar(DELTA));
    let mesh = out.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "N2-3a I3: union output must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "N2-3a I3: union output must be genus 0 (χ = 2)"
    );
    let vol = signed_volume(mesh);
    // Lower bound: the union contains the full cylinder (π R² H ≈ 8.25e-11)
    // and the box (2e-4 × 2e-4 × H ≈ 2.31e-11) minus their overlap (< the
    // thin tangency sliver, ≪ either volume). Upper bound: sum of both.
    let v_cyl = std::f64::consts::PI * R * R * H;
    let v_box = BOX_W * (2.0 * BOX_HALF_Y) * (H_B - Z_B_LO);
    assert!(
        vol > 0.9 * v_cyl.max(v_box) && vol < 1.1 * (v_cyl + v_box),
        "N2-3a I3: union volume {vol:.6e} implausible (cyl {v_cyl:.6e}, box {v_box:.6e})"
    );
}

// =========================================================================
// I4 — GREEN pin: locality / no-cluster companion.
// =========================================================================

/// GREEN today — the SAME near-tangent side plane (same δ), but with B's
/// caps clearing A's caps (NO coplanar pair). Every cap∩plane trace is an
/// ordinary A×B intersection curve, every rim crossing is a classified
/// relocation endpoint, and the output is fully on-surface TODAY (measured:
/// off-band count 0). Pins the no-op path: the cluster merge must not
/// disturb a near-tangent input whose vertices are all already exactly
/// relocated (spec I4/I5 — no over-eating within the same δ regime).
#[test]
fn i4_locality_noncoplanar_tangent_all_on_surface() {
    let a = cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], R, H_CLEAR);
    let out = run_union_with(a, box_b_clear(DELTA));
    let (off_count, max_resid, worst_pt, band) = cylinder_loop_residual_survey(&out);
    assert!(
        off_count == 0,
        "N2-3a I4: the non-coplanar near-tangent companion must be on-surface \
         today (and stay so): {off_count} off-band vertices, worst residual \
         {max_resid:.6e} > band {band:.6e} at p = {worst_pt:?}"
    );
}

// =========================================================================
// I6 — GREEN pin: determinism of the RED-class fixture.
// =========================================================================

/// GREEN today — two runs of the RED-class union are byte-identical
/// (A4.2; spade/cherchi are deterministic, Stage-4 iterates in sorted order).
#[test]
fn i6_determinism() {
    let r1 = run_union(box_b_coplanar(DELTA));
    let r2 = run_union(box_b_coplanar(DELTA));
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "N2-3a I6: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "N2-3a I6: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "N2-3a I6: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "N2-3a I6: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "N2-3a I6: face reversed differs");
    }
}

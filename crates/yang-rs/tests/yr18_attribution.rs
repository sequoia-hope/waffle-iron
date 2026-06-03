//! PR-YR18 RED — Stage-5 intersection-edge attribution fix.
//!
//! Spec of record: `docs/specs/yr18_intersection_edge_attribution.md`.
//!
//! Reproduces the **mis-classified intersection edge** the curved fuzz surfaces
//! as the dominant loud `SsiRefinementError::AmbiguousCurve { matched: 0 }`:
//! an undirected mesh boundary edge whose incidence carries two entries of
//! DIFFERENT `InputId` (so `build_intersection_curves` treats it as a
//! `(surf0, surf1)` intersection edge and hands it to `ssi_rs::intersect`), but
//! where ONE endpoint lies off ONE of the two attributed surfaces by MORE than
//! that edge's Stage-1 chord band `tol`. No returned curve passes through both
//! endpoints → `matched == 0` → loud `AmbiguousCurve`.
//!
//! Mechanism (spec §1): `compute_phase_a` pushes a patch's single inherited face
//! surface onto EVERY boundary edge of the patch cycle, so a seam edge shared by
//! a cylinder-wall patch (label B) and a box-top plane patch (label A) becomes a
//! `(Cylinder, Plane)` intersection edge — even when one endpoint is a pure
//! cylinder-wall tessellation vertex genuinely off the cylinder. The defect is
//! the CLASSIFICATION, not the SSI math (`ssi_rs::intersect` returns the correct
//! `z = 2` circle).
//!
//! Fixture geometry (deterministic, sidecar-free): a hand-built
//! `LabeledArrangement` whose surviving triangles are
//!   - a cylinder WALL band (label B, `Surface::Cylinder` axis +Z r=1, seam ring
//!     at z=2 → top ring at z=2.5), and
//!   - a box-TOP plane disk fan (label A, `Surface::Plane` z=2),
//! glued along the seam ring at z=2 (every seam edge is shared → an intersection
//! edge). All wall-triangle centroids lie within the cylinder's Stage-1 chord
//! band, and all plane-triangle centroids lie exactly on z=2, so both patches
//! attribute cleanly. ONE seam vertex (`S0`, the radius-`1+off` point at angle 0,
//! z=2) is pushed OUT radially so it lies ON the box-top plane (the plane patch
//! is happy) but `off ≈ 2.9 × tol` OFF the cylinder. The two seam edges incident
//! to `S0` — the BTreeMap-first intersection edges `(0,1)` and `(0,46)` — are the
//! MIS-CLASSIFIED edges: `(Cylinder, Plane)` edges whose `S0` endpoint is
//! `2.9 × tol` off the cylinder, so `curve_contains_point` rejects it and
//! `matched == 0`.
//!
//! The decisive real-world case (spec §1): a cylinder∩plane edge with
//! `tol ≈ 3.1e-2`, one endpoint on both surfaces, the other `~8.9e-2` off the
//! plane (~2.9× the band). This fixture reproduces that class numerically
//! (`tol ≈ 3.46e-2`, off endpoint `~1.0e-1` off the cylinder, exactly 2.9×).
//!
//! RED status: with the current (unfixed) `build_intersection_curves` the first
//! intersection edge it processes — the canonical-smallest key `(0, 1)`, which is
//! incident to `S0` — hands `(Cylinder, Plane)` to `ssi_rs::intersect`, the
//! returned `z=2` circle does not contain `S0` (radius `1+off`), so the boolean
//! returns `Err(YangError::SsiRefinementFailed { reason: AmbiguousCurve {
//! matched: 0, .. }, .. })`. The POST-FIX gate must skip this mis-classified edge
//! (it falls through to the `Curve::LineSegment` fallback), so the boolean must
//! NOT raise `AmbiguousCurve { matched: 0 }` for it. The two oracles below assert
//! that POST-FIX behaviour and therefore FAIL today (RED).

use std::collections::BTreeMap;
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{
    boolean, signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve,
    SsiRefinementError, Surface, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math.
// =========================================================================

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
// Canonical config.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2] — its TOP face is the plane z=2.
//   cylinder B: axis +Z through origin, radius 1, rims at z=0.5 and z=2.5 (the
//     SAME geometry yr13 uses, so the Stage-1 chord band `tol` is the cylinder's
//     `curved_chord_bound` ≈ 3.46e-2).
//   The arrangement glues a cylinder WALL band (seam ring at z=2 → top ring at
//     z=2.5, label B) to a box-TOP plane disk fan (label A) along the seam ring.
//   ONE seam vertex `S0` is pushed out radially to radius `1+off`, lying ON the
//     box-top plane but `off = 2.9·tol` OFF the cylinder.
// =========================================================================

const N: usize = 24; // seam / top-ring facets (fine enough wall centroids stay in band)
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, 0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.0;
const CYL_H: f64 = 2.0;
const SEAM_Z: f64 = 2.0; // seam ring lies ON the box-top plane
const TOP_Z: f64 = 2.5; // cylinder top ring (= cylinder top cap z)

/// The cylinder's Stage-1 chord band, computed from its rim `Curve::Circle`
/// edges' AABB exactly as `curved_chord_bound` does: rims at z=0.5 and z=2.5,
/// r=1 → AABB diag √(2² + 2² + 2²) = 2√3 → `tol = 1e-2 · 2√3 ≈ 3.464e-2`.
/// SINGLE source for the fixture's `off`; the production `tol` is recomputed
/// independently from the same cylinder B-Rep, so they agree by construction.
fn cyl_chord_tol() -> f64 {
    let lo = [-CYL_R, -CYL_R, 0.5];
    let hi = [CYL_R, CYL_R, 2.5];
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    1e-2 * diag
}

/// Radial off-cylinder displacement of the mis-classified seam vertex `S0`:
/// exactly `2.9 × tol`, mirroring the decisive investigation case (off endpoint
/// ~2.9× the chord band).
fn off_dist() -> f64 {
    2.9 * cyl_chord_tol()
}

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

/// The box-TOP supporting plane (normal +Z, `n·x + d = 0` ⇒ d = −2).
fn box_top_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -BOX_HI[2],
    }
}

// =========================================================================
// Input B-Reps (box + cylinder), copied from the yr13 fixture so the
// production `tol` (from the cylinder B-Rep) matches `cyl_chord_tol()`.
// Integration tests cannot see #[cfg(test)] lib items, so these are local.
// =========================================================================

/// Axis-aligned box `lo..hi` with OUTWARD normals and plane offsets. The TOP
/// face (index 1, normal +Z, d=−hi.z) is the plane the seam ring lies on.
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

/// Closed solid-cylinder B-Rep (yr7/yr13 seam-edge encoding). Its rim
/// `Curve::Circle` edges drive the production `tol` via `curved_chord_bound`.
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

fn input_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn input_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

// =========================================================================
// Hand-built arrangement: a cylinder WALL band (label B) glued to a box-TOP
// plane disk fan (label A) along the seam ring at z=2. Every triangle has
// `inside == [false, false]` so the Union keep-rule (inside.count() == 0) keeps
// ALL of them and `flip_for_op(Union)` leaves winding untouched. Both patches
// therefore survive and `compute_phase_a` runs over the full seam.
//
// `S0` (seam vertex k=0) is pushed out to radius `1+off`: ON the box-top plane,
// `off ≈ 2.9·tol` OFF the cylinder. The seam edges incident to `S0` are the
// mis-classified `(Cylinder, Plane)` edges.
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

/// Seam-ring vertex `k` at z=2. All on radius 1 (on the cylinder) EXCEPT k≡0
/// which is pushed out to radius `1+off` (off the cylinder, still on z=2).
fn seam_pt(k: usize) -> Point3 {
    let th = 2.0 * std::f64::consts::PI * ((k % N) as f64) / (N as f64);
    let rad = if k % N == 0 {
        CYL_R + off_dist()
    } else {
        CYL_R
    };
    p(rad * th.cos(), rad * th.sin(), SEAM_Z)
}

/// Cylinder top-ring vertex `k` at z=2.5, radius 1 (exact, on the cylinder).
fn top_pt(k: usize) -> Point3 {
    let th = 2.0 * std::f64::consts::PI * ((k % N) as f64) / (N as f64);
    p(CYL_R * th.cos(), CYL_R * th.sin(), TOP_Z)
}

fn misclassified_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut idx: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut push_v = |pt: Point3| -> u32 {
        let key = [pt.x().to_bits(), pt.y().to_bits(), pt.z().to_bits()];
        if let Some(&i) = idx.get(&key) {
            return i;
        }
        let i = verts.len() as u32;
        verts.push(pt);
        idx.insert(key, i);
        i
    };

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    // === CYLINDER WALL band (label B = id 1): seam ring (z=2) → top ring (z=2.5).
    // A real Cherchi arrangement is OUTWARD-oriented; the precise winding does
    // not matter for this RED fixture (we only reach `build_intersection_curves`,
    // which classifies by incidence, before any winding-dependent emission), so
    // the band is wound consistently CCW-from-outside.
    for k in 0..N {
        let s0 = push_v(seam_pt(k));
        let s1 = push_v(seam_pt(k + 1));
        let t1 = push_v(top_pt(k + 1));
        let t0 = push_v(top_pt(k));
        tris.push([s0, s1, t1]);
        surface.push(vec![LaInputId(1)]);
        tris.push([s0, t1, t0]);
        surface.push(vec![LaInputId(1)]);
    }

    // === BOX-TOP plane disk fan (label A = id 0): center (0,0,2) → seam ring.
    // Center and all seam-ring vertices except S0 are at z=2; the fan triangles'
    // centroids land exactly on z=2 (S0 contributes only to its two triangles,
    // whose centroid z = (2 + 2 + 2)/3 = 2 because S0 itself is at z=2 — it is
    // pushed RADIALLY, not vertically — so EVERY plane triangle is on the plane).
    let center = push_v(p(0.0, 0.0, SEAM_Z));
    for k in 0..N {
        let s0 = push_v(seam_pt(k));
        let s1 = push_v(seam_pt(k + 1));
        tris.push([center, s0, s1]);
        surface.push(vec![LaInputId(0)]);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    // Every triangle outside both solids → Union keeps ALL of them.
    let inside: Vec<Vec<bool>> = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        num_inputs: 2,
    }
}

fn run_union() -> Result<BRep, YangError> {
    let bx = input_box();
    let cyl = input_cyl();
    let mock = LabelMock {
        arrangement: misclassified_arrangement(),
    };
    // a = box (InputId::A / id 0), b = cylinder (InputId::B / id 1).
    boolean(&bx, &cyl, BoolOp::Union, &mock)
}

/// True iff `e` is a loud `AmbiguousCurve { matched: 0 }` SSI refinement failure.
fn is_ambiguous_matched_zero(e: &YangError) -> bool {
    matches!(
        e,
        YangError::SsiRefinementFailed {
            reason: SsiRefinementError::AmbiguousCurve { matched: 0, .. },
            ..
        }
    )
}

// =========================================================================
// UNIT GUARD — pin the mechanism directly via `signed_distance_to_surface`
// (the SAME predicate the post-fix gate uses). Documents that the mis-classified
// seam vertex `S0` is > tol off the cylinder while the kept rim endpoints are
// <= tol from both surfaces. This test PASSES today (no `boolean()` call) — it
// proves the fixture geometry is what the RED oracles claim.
// =========================================================================

#[test]
fn unit_guard_off_endpoint_exceeds_band() {
    let tol = cyl_chord_tol();
    let cyl = cyl_surface();
    let plane = box_top_plane();

    // S0 (the radius-(1+off) seam vertex): ON the plane, OFF the cylinder by ~2.9·tol.
    let s0 = seam_pt(0);
    let sd_cyl_s0 = signed_distance_to_surface(cyl, s0).expect("cyl sd");
    let sd_plane_s0 = signed_distance_to_surface(plane, s0).expect("plane sd");
    assert!(
        sd_plane_s0.abs() <= tol,
        "yr18 guard: S0 must lie ON the box-top plane (|sd|={} ≤ tol={})",
        sd_plane_s0.abs(),
        tol
    );
    assert!(
        sd_cyl_s0.abs() > tol,
        "yr18 guard: S0 must lie OFF the cylinder beyond the chord band \
         (|sd|={} > tol={})",
        sd_cyl_s0.abs(),
        tol
    );
    // It is ~2.9× the band — the decisive investigation ratio.
    let ratio = sd_cyl_s0.abs() / tol;
    assert!(
        (2.0..=4.0).contains(&ratio),
        "yr18 guard: S0 off-cylinder ratio {ratio} should be ~2.9× the band"
    );

    // The kept rim endpoints S1..S_{N-1}: ON BOTH surfaces within tol.
    for k in 1..N {
        let pt = seam_pt(k);
        let sc = signed_distance_to_surface(cyl, pt).expect("cyl sd").abs();
        let sp = signed_distance_to_surface(plane, pt)
            .expect("plane sd")
            .abs();
        assert!(
            sc <= tol && sp <= tol,
            "yr18 guard: kept rim vertex {k} must lie on BOTH surfaces within tol \
             (cyl |sd|={sc}, plane |sd|={sp}, tol={tol})"
        );
    }
}

// =========================================================================
// Oracle 1 (RED) — the boolean must NOT raise `AmbiguousCurve { matched: 0 }`
// caused by the mis-classified seam edge. POST-FIX the gate skips it (falls
// through to `Curve::LineSegment`), so the boolean either succeeds OR refuses for
// an unrelated, correctly-classified reason.
// =========================================================================

#[test]
fn oracle1_misclassified_edge_does_not_raise_ambiguous_matched_zero() {
    let result = run_union();
    if let Err(e) = &result {
        assert!(
            !is_ambiguous_matched_zero(e),
            "yr18 O1: the boolean raised AmbiguousCurve {{ matched: 0 }} from the \
             mis-classified cylinder∩plane seam edge (one endpoint {:.4}× the chord \
             band off the cylinder). POST-FIX the on-both-surfaces gate must skip \
             this edge (→ Curve::LineSegment), never hand it to ssi_rs::intersect. \
             Got: {e:?}",
            off_dist() / cyl_chord_tol()
        );
    }
    // A success is fine; a NON-AmbiguousCurve error is also acceptable (the
    // fixture's purpose is solely to prove this edge is no longer mis-classified).
}

// =========================================================================
// Oracle 2 (RED) — exercise the exact error-enum shape the spec names, and the
// off-endpoint magnitude, so the failure message documents the mechanism
// verbatim. Identical assertion target as O1; kept separate to surface the
// numeric evidence (tol, off, ratio) in the panic.
// =========================================================================

#[test]
fn oracle2_no_ambiguous_from_off_cylinder_endpoint() {
    let tol = cyl_chord_tol();
    let off = off_dist();
    let result = run_union();
    match result {
        Ok(_) => { /* POST-FIX success — gate skipped the mis-classified edge. */ }
        Err(e) => {
            assert!(
                !is_ambiguous_matched_zero(&e),
                "yr18 O2: a (Cylinder, Plane) seam edge incident to S0 was handed to \
                 ssi_rs::intersect though S0 is {:.4} off the cylinder (tol={:.4}, \
                 ratio={:.2}×). The z=2 circle the SSI returns cannot pass through S0 \
                 → matched==0 → AmbiguousCurve. POST-FIX this edge must be classified \
                 as a single-surface internal edge and skipped. Got: {e:?}",
                off,
                tol,
                off / tol
            );
        }
    }
}

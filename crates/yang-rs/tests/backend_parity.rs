//! PR-CR-BL3c — yang-rs BACKEND PARITY: the production NATIVE backend
//! (`yang_rs::native_backend()` → `cherchi_rs::NativeBoolean`) vs the C++
//! subprocess sidecar (`cherchi_sidecar_rs::SidecarBoolean`), through the
//! FULL yang-rs `boolean()` pipeline end to end.
//!
//! cherchi-rs's own `tests/parity_native_vs_sidecar.rs` (PR-CR-BL3b) is the
//! M6 reference-parity gate at the MESH level. This module is the yang-level
//! complement: it asserts that swapping the backend under the `MeshBoolean`
//! seam does not change the *B-Rep pipeline's* output — Stage 5/6 attribution,
//! topology reconstruction, and Stage 3/4 refinement included.
//!
//! ## Parity metric (triangulation-independent)
//!
//! The two backends may triangulate the same arrangement differently, so the
//! diff never compares triangle lists. Per case:
//!
//! 1. Both backends return `Ok`.
//! 2. Both outputs watertight (0 unpaired half-edges) with equal Euler
//!    characteristic.
//! 3. Signed volume equal within 1e-9 relative — for PLANAR cases. For
//!    curved cases the output volume is triangulation-DEPENDENT even with
//!    identical vertex sets: Stage 4 relocates near-curve vertices onto the
//!    exact analytic curve, after which the lateral surface is non-planar
//!    quads whose enclosed volume depends on each backend's (legitimately
//!    different) diagonal choices. Empirically (this fixture): mesh-LEVEL
//!    backend outputs are equal to 4e-16 and the yang-level vertex sets,
//!    χ, watertightness, and surfaces all match exactly — only the volume
//!    differs (~2e-4), purely from triangulation. Curved cases therefore
//!    use a chord-band volume tolerance d_ε × A_lateral (the project-wide
//!    chord-band metric, cf. yr19), NOT a widened universal tolerance —
//!    the equality content for curved cases is carried by metrics 1/2/4/5.
//! 4. Vertex-set Hausdorff-0: every native output vertex has a sidecar
//!    output vertex within 1e-6 absolute and vice versa (Cherchi
//!    arrangements add no Steiner points beyond input verts + LPI/TPI
//!    intersection points, and yang's Stage-4 relocation is a deterministic
//!    function of those points + the analytic curves — so the SETS must
//!    match; the slack absorbs each side's lazy-exact → f64 emission
//!    rounding).
//! 5. B-Rep face-surface multiset equal EXACTLY (surfaces are inherited from
//!    the input faces' analytic params, which both pipelines pass through
//!    bit-identically).
//!
//! NO tolerance widening to make a case pass (P9/P10): a mismatch is a real
//! finding (native bug, sidecar-coupled pipeline assumption, or harness bug).
//!
//! ## Skip behavior
//!
//! Self-skips (loud eprintln) when either backend is unavailable: the
//! sidecar binary missing (`CHERCHI2022_BIN`), or the indirect-predicates
//! FFI stub build (`yang_rs::native_backend()` → `None`).

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{
    boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, MeshBoolean, Surface, YangError,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Fixtures (suite-conventional copies — see end_to_end.rs / yr8_curved_boolean.rs)
// =========================================================================

/// Unit cube BRep at `origin` with outward normals and true plane offsets.
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

/// Axis-aligned cylinder BRep (z-axis), matching yr8's canonical fixture:
/// axis through (0.5, 0.5, ·), r = 0.25, z ∈ [−0.5, 1.5] — pierces the unit
/// cube at [0,1]³ through both caps with NO coplanar face pairs.
fn canonical_cylinder() -> BRep {
    let (cx, cy) = (0.5, 0.5);
    let (z0, z1) = (-0.5, 1.5);
    let radius = 0.25;
    let verts = vec![
        BRepVertex {
            point: p(cx + radius, cy, z0),
        },
        BRepVertex {
            point: p(cx + radius, cy, z1),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(cx, cy, z0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(cx, cy, z1),
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
                axis_point: p(cx, cy, z0),
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
                d: z0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -z1,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("canonical_cylinder BRep::new failed")
}

// =========================================================================
// Mesh metrics (suite-conventional copies)
// =========================================================================

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

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    use std::collections::HashMap;
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

/// Euler V − E + F over the mesh, after an exact-coordinate vertex weld
/// (so duplicated emission vertices don't skew V or E).
fn euler_characteristic_welded(mesh: &Mesh) -> i64 {
    use std::collections::{HashMap, HashSet};
    let mut first: HashMap<[u64; 3], u32> = HashMap::new();
    let weld: Vec<u32> = mesh
        .verts
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
            *first.entry(key).or_insert(i as u32)
        })
        .collect();
    let v = first.len() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (weld[tri[i] as usize], weld[tri[j] as usize]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    v - edges.len() as i64 + f
}

/// Hausdorff-0 vertex-set match: every vertex of `a` has a vertex of `b`
/// within `tol` (call twice for symmetry).
fn vertex_set_covered(a: &Mesh, b: &Mesh, tol: f64) -> Option<Point3> {
    for va in &a.verts {
        let covered = b.verts.iter().any(|vb| {
            (va.x() - vb.x()).abs() <= tol
                && (va.y() - vb.y()).abs() <= tol
                && (va.z() - vb.z()).abs() <= tol
        });
        if !covered {
            return Some(*va);
        }
    }
    None
}

/// Canonical sorted listing of a BRep's face surfaces (Debug form — Surface
/// has no Ord). Surfaces are inherited bit-identically from input faces, so
/// the multiset must match exactly across backends.
fn surface_multiset(brep: &BRep) -> Vec<String> {
    let mut s: Vec<String> = brep
        .faces()
        .iter()
        .map(|f| format!("{:?}", f.surface))
        .collect();
    s.sort();
    s
}

// =========================================================================
// The parity harness
// =========================================================================

const VOL_REL_TOL: f64 = 1e-9;
const VERT_TOL: f64 = 1e-6;

/// Volume parity arm (see module docs, metric 3).
enum VolTol {
    /// Planar case: triangulations of the same piecewise-planar complex
    /// enclose the same volume ⇒ strict 1e-9 relative.
    StrictRel,
    /// Curved case: triangulation-dependent chord volume ⇒ absolute
    /// chord-band bound d_ε × A_lateral, computed per fixture.
    ChordBand(f64),
}

fn assert_backend_parity(case: &str, a: &BRep, b: &BRep, op: BoolOp, vol_tol: VolTol) {
    let Ok(sidecar) = SidecarBoolean::from_env() else {
        eprintln!("[backend_parity] SKIP {case}: sidecar binary not found (set CHERCHI2022_BIN)");
        return;
    };
    let Some(native) = yang_rs::native_backend() else {
        eprintln!("[backend_parity] SKIP {case}: native FFI shim not linked (stub build)");
        return;
    };

    let run = |backend: &dyn MeshBoolean, name: &str| -> Result<BRep, YangError> {
        let r = boolean(a, b, op, backend);
        if let Err(e) = &r {
            eprintln!("[backend_parity] {case}/{name} FAILED: {e:?}");
        }
        r
    };
    let rn = run(&native, "native").expect("native backend must succeed");
    let rs = run(&sidecar, "sidecar").expect("sidecar backend must succeed");
    let (mn, ms) = (rn.as_mesh(), rs.as_mesh());

    // (2) watertight + Euler.
    assert_eq!(
        unpaired_half_edges(mn),
        0,
        "{case}: native output not watertight"
    );
    assert_eq!(
        unpaired_half_edges(ms),
        0,
        "{case}: sidecar output not watertight"
    );
    assert_eq!(
        euler_characteristic_welded(mn),
        euler_characteristic_welded(ms),
        "{case}: Euler characteristic differs (native vs sidecar)"
    );

    // (3) signed volume (see module docs for the two arms).
    let (vn, vs) = (signed_volume(mn), signed_volume(ms));
    match vol_tol {
        VolTol::StrictRel => {
            let scale = vs.abs().max(1e-12);
            assert!(
                ((vn - vs) / scale).abs() <= VOL_REL_TOL,
                "{case}: signed volume diverges: native {vn} vs sidecar {vs}"
            );
        }
        VolTol::ChordBand(band) => {
            assert!(
                (vn - vs).abs() <= band,
                "{case}: signed volume diverges beyond the chord band {band}: \
                 native {vn} vs sidecar {vs}"
            );
        }
    }

    // (4) vertex-set Hausdorff-0, both directions.
    if let Some(v) = vertex_set_covered(mn, ms, VERT_TOL) {
        panic!("{case}: native vertex {v:?} has no sidecar vertex within {VERT_TOL}");
    }
    if let Some(v) = vertex_set_covered(ms, mn, VERT_TOL) {
        panic!("{case}: sidecar vertex {v:?} has no native vertex within {VERT_TOL}");
    }

    // (5) B-Rep face-surface multiset, exact.
    assert_eq!(
        surface_multiset(&rn),
        surface_multiset(&rs),
        "{case}: B-Rep face-surface multiset differs (native vs sidecar)"
    );
}

// =========================================================================
// Cases: planar diagonal cubes × {Union, Intersect, Subtract} (XOR is
// YangError::UnsupportedOp by spec) + one curved case (cylinder ∪ box,
// the yr8 canonical fixture).
// =========================================================================

#[test]
fn parity_planar_diagonal_cubes_union() {
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([0.5, 0.5, 0.5]);
    assert_backend_parity("diag_cubes_union", &a, &b, BoolOp::Union, VolTol::StrictRel);
}

#[test]
fn parity_planar_diagonal_cubes_intersect() {
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([0.5, 0.5, 0.5]);
    assert_backend_parity(
        "diag_cubes_intersect",
        &a,
        &b,
        BoolOp::Intersect,
        VolTol::StrictRel,
    );
}

#[test]
fn parity_planar_diagonal_cubes_subtract() {
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([0.5, 0.5, 0.5]);
    assert_backend_parity(
        "diag_cubes_subtract",
        &a,
        &b,
        BoolOp::Subtract,
        VolTol::StrictRel,
    );
}

#[test]
fn parity_planar_corner_clip_subtract() {
    // Asymmetric clip: B bites one corner of A (generic position, no
    // coplanar pairs).
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([0.7, 0.3, 0.4]);
    assert_backend_parity(
        "corner_clip_subtract",
        &a,
        &b,
        BoolOp::Subtract,
        VolTol::StrictRel,
    );
}

#[test]
fn parity_curved_cylinder_union_box() {
    // yr8 canonical curved case: through-piercing cylinder ∪ unit box.
    let cyl = canonical_cylinder();
    let bx = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    // Chord band: d_ε = 1e-2 × analytic AABB diag of the combined inputs
    // (Stage 1's chord rule); A_lateral = 2πrh. The bound is the maximum
    // volume freedom two valid triangulations of the same relocated vertex
    // set have over the curved region (observed: ~2e-4, band ≈ 0.077).
    let d_eps = 1e-2 * (1.0f64 + 1.0 + 4.0).sqrt();
    let band = d_eps * (2.0 * std::f64::consts::PI * 0.25 * 2.0);
    assert_backend_parity(
        "cylinder_union_box",
        &cyl,
        &bx,
        BoolOp::Union,
        VolTol::ChordBand(band),
    );
}

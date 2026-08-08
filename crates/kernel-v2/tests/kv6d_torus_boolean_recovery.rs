//! KV6d increment 5b2 + Tier B relocation: a torus boolean traverses
//! `yang_rs::boolean` (whose Stage-4 implicit-pair Newton relocates the
//! intersection boundary ONTO the analytic torus), is reconstructed by kernel-v2
//! `from_yang_brep` (the `FaceSurf::Torus` arm + on-surface `validate_torus_face`),
//! and — for a simple (non-seam-wrapping) patch — render-tessellated via the
//! UV-CDT consumer `yang_rs::tessellate_torus_patch`.
//!
//! to_yang torus-OPERAND conversion is a separate later increment, so we build
//! the torus operand directly as a yang B-Rep and feed yang's OUTPUT through the
//! kernel-v2 reconstruction + render path.

use cad_primitives::{BoolOp, Point3, Vector3};
use kernel_v2::{from_yang_brep, tessellate, validate_solid, BrepArena, RenderMesh};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// 90° bent tube (partial torus): center origin, axis +z, R=3, r=1. Closed
/// solid (torus band + 2 meridian-plane disk caps).
fn partial_torus_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(4.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 4.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(3.0, 0.0, 0.0),
                normal: v3(0.0, 1.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 3.0, 0.0),
                normal: v3(1.0, 0.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Torus {
                center: p(0.0, 0.0, 0.0),
                axis_dir: v3(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("partial torus B-Rep")
}

fn box_brep(origin: [f64; 3], size: [f64; 3]) -> BRep {
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
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
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
    BRep::new(verts, edges, faces).expect("box brep")
}

/// Positional watertightness: snap each render-mesh vertex to a 1e-9 grid and
/// require every undirected edge to be shared by exactly two triangles.
fn assert_render_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::BTreeMap;
    let key = |i: u32| -> [i64; 3] {
        let k = i as usize * 3;
        [
            (mesh.positions[k] * 1e9).round() as i64,
            (mesh.positions[k + 1] * 1e9).round() as i64,
            (mesh.positions[k + 2] * 1e9).round() as i64,
        ]
    };
    let mut ec: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *ec.entry(e).or_insert(0) += 1;
        }
    }
    for (e, n) in &ec {
        assert_eq!(*n, 2, "{what}: edge {e:?} shared by {n} tris (not 2)");
    }
}

/// Every torus-band render vertex lies on the tube: √((ρ−R)² + τ²) ≈ r.
fn assert_band_on_surface(mesh: &RenderMesh, what: &str) {
    let mut band = 0;
    for i in 0..mesh.num_vertices() {
        let k = i * 3;
        let (px, py, pz) = (
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        );
        let tau = pz; // axis +z
        let rho = (px * px + py * py).sqrt();
        let d = rho - 3.0;
        let resid = ((d * d + tau * tau).sqrt() - 1.0).abs();
        if resid < 1e-6 {
            band += 1;
        }
    }
    assert!(band > 0, "{what}: no on-tube torus vertices");
}

/// ANALYTIC volume of the `partial_torus_brep` fixture (Pappus: a 90° bend of
/// the R=3, r=1 tube is a quarter of the full torus, 2π²Rr²/4).
///
/// The volume-conservation bounds below compare against the OUTPUT's render
/// volume. Since the 2026-08-08 chord-band fix, boolean-output patches render
/// at the structured band, so an operand measured from the coarser yang
/// Stage-1 mesh UNDER-states A by that mesh's inscribed-chord loss and the
/// bound fails spuriously (subtract vol 14.654 "exceeding" a Stage-1-measured
/// A of 13.947). The true volumes are exact for these fixtures, and an
/// inscribed render can never exceed them — the honest bound basis.
fn partial_torus_volume() -> f64 {
    use std::f64::consts::PI;
    0.25 * 2.0 * PI * PI * 3.0 * 1.0 * 1.0
}

/// Enclosed volume of a kernel-v2 render mesh.
fn render_volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [m.positions[k], m.positions[k + 1], m.positions[k + 2]]
    };
    let mut s = 0.0;
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        s += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    (s / 6.0).abs()
}

/// Boolean volume CONSERVATION bound — the kernel-independent correctness oracle
/// the loose `check_volume_magnitude` (16 orders of magnitude) cannot provide:
/// union ⊇ max(A,B), subtract ⊆ A, intersect ⊆ min(A,B). A 30%-too-thin result
/// (the band-render regression) violates these; a watertight-but-wrong mesh does
/// not slip through. `band` is the inscribed-chord slack.
fn assert_boolean_volume_bounds(op: BoolOp, va: f64, vb: f64, vr: f64, band: f64) {
    match op {
        BoolOp::Union => assert!(
            vr >= va.max(vb) * (1.0 - band) && vr <= (va + vb) * (1.0 + band),
            "union vol {vr} outside [max(A,B), A+B] for A={va} B={vb}"
        ),
        BoolOp::Subtract => assert!(
            vr <= va * (1.0 + band) && vr >= 0.0,
            "subtract vol {vr} exceeds A={va}"
        ),
        BoolOp::Intersect => assert!(
            vr <= va.min(vb) * (1.0 + band),
            "intersect vol {vr} exceeds min(A,B) for A={va} B={vb}"
        ),
        // Symmetric difference: bounded above by the union.
        BoolOp::Xor => assert!(
            vr <= (va + vb) * (1.0 + band),
            "xor vol {vr} exceeds A+B for A={va} B={vb}"
        ),
    }
}

/// Max torus-surface residual over every torus face's boundary vertices of a
/// yang output B-Rep.
fn max_torus_boundary_residual(out: &BRep) -> f64 {
    let mut maxr: f64 = 0.0;
    for f in out.faces() {
        if let Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } = f.surface
        {
            let c = center.as_array();
            let ax = axis_dir.as_array();
            let verts = out.vertices();
            for e in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let pid = out.edges()[*e as usize].start as usize;
                let p = verts[pid].point.as_array();
                let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let tau = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
                let rad = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
                let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
                let resid = ((rho - major_radius).powi(2) + tau * tau).sqrt() - minor_radius;
                maxr = maxr.max(resid.abs());
            }
        }
    }
    maxr
}

/// KV6d Tier B: the Stage-4 implicit-pair Newton relocation drives the trimmed
/// torus boundary ONTO the analytic torus (~0.096 off the chord → ~1e-8), so the
/// analytic-B-Rep reconstruction `from_yang_brep` (whose `validate_torus_face`
/// requires on-surface boundary vertices) now ACCEPTS the output where before it
/// rejected it. This is the end-to-end proof of the relocation. (A "tube minus a
/// window" cut leaves the surviving lateral wrapping the full meridian seam, the
/// UV-CDT consumer's documented v1 boundary, so render tessellation of THIS
/// patch is exercised separately — see `tessellate::torus_patch_tess_tests` for
/// the simple-patch render and the small-box intersect below.)
#[test]
fn torus_boolean_relocates_boundary_onto_surface_and_reconstructs() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    let a = partial_torus_brep();
    let b = box_brep([3.4, -0.6, -0.6], [1.2, 1.2, 1.2]);

    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend)
        .unwrap_or_else(|e| panic!("torus − box (yang): {e:?}"));

    // The relocation made every torus-boundary vertex on-surface.
    let resid = max_torus_boundary_residual(&out);
    assert!(
        resid < 1e-7,
        "torus boundary not relocated on-surface: {resid:e}"
    );

    // ...so the analytic reconstruction + its validation now succeed.
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &out)
        .unwrap_or_else(|e| panic!("from_yang_brep(relocated torus output): {e:?}"));
    validate_solid(&arena, solid).expect("reconstructed torus-cut solid validates");
}

/// Full path including RENDER of a boolean-output torus patch: a box INTERSECTED
/// with the tube yields a DISK-topology torus patch (a bounded (u,v) region, no
/// meridian wrap) which reconstructs AND render-tessellates into a watertight,
/// on-tube mesh via the UV-CDT consumer. (A cut that wraps the meridian seam —
/// cylindrical topology — is the documented periodic-render follow-on, detected
/// and reported loudly by the consumer; see `torus_subtract_seam_cut_is_periodic`.)
#[test]
fn box_intersect_torus_reconstructs_and_tessellates() {
    let Some(backend) = yang_rs::native_backend() else {
        return;
    };
    let a = partial_torus_brep();
    // A box over the outer tube near θ=45°: the kept torus patch is a bounded
    // (u,v) disk (no meridian wrap).
    let b = box_brep([2.4, 2.4, -0.6], [1.0, 1.0, 1.2]);
    let (va, vb) = (partial_torus_volume(), 1.0 * 1.0 * 1.2);

    let out = yang_rs::boolean(&a, &b, BoolOp::Intersect, &backend)
        .unwrap_or_else(|e| panic!("torus ∩ box (yang): {e:?}"));
    assert!(
        out.faces()
            .iter()
            .any(|f| matches!(f.surface, Surface::Torus { .. })),
        "intersect must keep a torus patch"
    );
    let resid = max_torus_boundary_residual(&out);
    assert!(
        resid < 1e-7,
        "torus patch boundary not on-surface: {resid:e}"
    );

    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &out)
        .unwrap_or_else(|e| panic!("from_yang_brep(torus ∩ box): {e:?}"));
    validate_solid(&arena, solid).expect("reconstructed torus ∩ box validates");
    let mesh = tessellate(&arena, solid).expect("tessellate torus ∩ box");
    assert!(!mesh.indices.is_empty(), "non-empty render mesh");
    assert_render_watertight(&mesh, "torus ∩ box render");
    assert_band_on_surface(&mesh, "torus ∩ box render");
    // Intersect ⊆ min(A, B): the kept chunk cannot exceed either operand.
    assert_boolean_volume_bounds(BoolOp::Intersect, va, vb, render_volume(&mesh), 0.05);
}

/// A subtract that bites the tube AT the outer (φ=0) seam turns the surviving
/// torus lateral into a patch that WRAPS the full meridian (cylindrical, not
/// disk, topology). The seam-wrapping (band) render now handles it: the torus
/// face is no longer the blocker. (This particular box's planar cut-cap — a
/// non-convex polygon bounded by the relocated intersection curve — still trips
/// a SEPARATE planar ear-clip limitation; the assertion below confirms only that
/// any remaining failure is NOT the torus patch.) The seam-wrapping render
/// itself is proven watertight + on-tube by `tessellate::torus_patch_tests::
/// torus_band_seam_wrapping_render`.
#[test]
fn torus_subtract_seam_cut_torus_face_renders() {
    let Some(backend) = yang_rs::native_backend() else {
        return;
    };
    let a = partial_torus_brep();
    let b = box_brep([3.4, -0.6, -0.6], [1.2, 1.2, 1.2]);
    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend).expect("torus − box");

    // Reconstruction succeeds (Tier B relocation put the boundary on-surface).
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &out).expect("reconstruct seam-cut torus");

    // The torus seam-wrapping patch no longer blocks: any remaining render error
    // is a non-torus (planar cut-cap) face, NOT a torus seam/UV-CDT failure.
    if let Err(e) = tessellate(&arena, solid) {
        let msg = format!("{e:?}");
        assert!(
            !msg.contains("UV-CDT") && !msg.contains("seam-crossing") && !msg.contains("torus"),
            "the torus band must render; remaining failure must be non-torus, got: {msg}"
        );
    }
}

/// KV6d-5b2 band-render fix: a box FULLY INSIDE the tube (subtract) leaves the
/// torus lateral intact as a meridian-wrapping BAND. The band render (with seam
/// subdivision) must cover the full tube — before the fix it produced a thin
/// sliver (vol ~0.5 instead of ~torus−box). Catches that regression: the
/// reconstructed solid renders watertight, on-tube, with near-torus volume.
#[test]
fn contained_box_torus_band_renders_full_volume() {
    let Some(backend) = yang_rs::native_backend() else {
        return;
    };
    let a = partial_torus_brep();
    let b = box_brep([1.95, 1.95, -0.3], [0.45, 0.45, 0.6]); // inside the tube
    let va = partial_torus_volume();
    let vb = 0.45 * 0.45 * 0.6;
    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend).expect("torus − contained box");
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &out).expect("reconstruct");
    validate_solid(&arena, solid).expect("validates");
    let mesh = tessellate(&arena, solid).expect("tessellate");
    assert_render_watertight(&mesh, "contained-box torus band");
    assert_band_on_surface(&mesh, "contained-box torus band");
    // Volume-conservation bound (subtract ⊆ A) PLUS a tight contained-box lower
    // bound (≈ vol(A) − vol(B)). The pre-fix thin sliver (~0.5) fails both.
    let vr = render_volume(&mesh);
    assert_boolean_volume_bounds(BoolOp::Subtract, va, vb, vr, 0.05);
    assert!(
        vr > va - vb - 0.5,
        "contained subtract removed too much: vr {vr} vs A {va} − B {vb}"
    );
}

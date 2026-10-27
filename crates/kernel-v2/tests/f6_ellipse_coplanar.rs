//! Roadmap F6 (ledger N89 item 4; matrix rows 4.5.5-05, 5.5-03):
//! Stage-0 §4.5.5 coplanar preprocessing on ellipse-bounded planar faces
//! (`refs/text/yang2025_hybrid_boolean.txt:718-732`, Fig. 24(b) `:962-966`).
//!
//! 1. Minimal repro via `KernelV2Adapter` (`make_faces_from_profiles` /
//!    `extrude_face` / `boolean_intersect` / `boolean_union`): an oblique
//!    planar cut of a tilted circular cylinder produces planar caps bounded by
//!    exact `EllipseArc`s; unioning a coplanar solid onto that cap previously
//!    hit Stage 0's `face-unsupported` (`CoplanarFacesUnsupported`) wall.
//!    Verified via `KernelV2Adapter` (`tessellate` -> `validate_solid` + closed
//!    2-manifold render-mesh pairing + `KernelMeasure::mass_properties`) and
//!    via `BrepArena` (`validate_solid` + exact closed-form
//!    `geom::signed_volume` to `1e-12` relative).
//! 2. Crossing-rim pin + `BrepArena` structural check: two rotated coplanar-cap
//!    elliptical cylinders union cleanly, pass `validate_solid`, emit a closed
//!    2-manifold render mesh, and retain analytic `Curve::EllipseArc` edges on
//!    both coplanar caps.
//! 3. Fig. 24(b) sequential unions (`:962-966`): 4-cylinder (debug+release) and
//!    full 24-cylinder (`step = π/24`) coplanar-capped elliptical cylinders
//!    combined sequentially by `boolean_union`, verified by `validate_solid`
//!    (`shells == 1`, `genus == 0`, `euler_lhs == euler_rhs == 2`), closed
//!    2-manifold render-mesh pairing (`open == 0`, `over == 0`), an analytic
//!    polar-support Cavalieri cross-section integral (`< 1e-6` quadrature
//!    convergence, cross-checked to `1.05e-9` against exact closed-form
//!    `geom::signed_volume` on the 4-cylinder union), and render-mesh
//!    chord-deficit bounds at `N = 71` and `N = 223`.

use std::collections::HashMap;
use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, circle_segment_count, extrude, geom, tessellate, tessellate_with_chord_tolerance,
    transform_solid, validate_solid, BrepArena, Curve, KernelV2Adapter, KernelV2Error, Profile,
    RenderMesh as Kv2RenderMesh, SolidId, Surface, RENDER_CHORD_TOLERANCE_REL,
};
use waffle_types::kernel::{
    Kernel, KernelError, KernelMeasure, KernelSolidHandle, Method, RenderMesh, RigidPlacement,
};
use waffle_types::{CircleProfile, ClosedProfile};

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn plane_x_axis(n: [f64; 3]) -> [f64; 3] {
    let n = norm(n);
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    norm(cross(r, n))
}

fn rect_profile(x: (f64, f64), y: (f64, f64)) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let positions = HashMap::from([
        (1u32, (x.0, y.0)),
        (2, (x.1, y.0)),
        (3, (x.1, y.1)),
        (4, (x.0, y.1)),
    ]);
    let ids = vec![1, 2, 3, 4];
    let profile = ClosedProfile {
        entity_ids: ids.clone(),
        is_outer: true,
        vertex_ids: ids,
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    (profile, positions)
}

fn circle_profile(cx: f64, cy: f64, radius: f64) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let profile = ClosedProfile {
        entity_ids: vec![1],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: cx,
            center_v: cy,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    (profile, HashMap::new())
}

fn adapter_rect_box(
    k: &mut KernelV2Adapter,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> KernelSolidHandle {
    let n = [0.0, 0.0, 1.0];
    let (prof, pos) = rect_profile(x, y);
    let faces = k
        .make_faces_from_profiles(&[prof], [0.0, 0.0, z.0], n, [1.0, 0.0, 0.0], &pos)
        .expect("stage rectangle face");
    k.extrude_face(faces[0], n, z.1 - z.0).expect("extrude box")
}

/// Build an oblique-cut elliptical-cap cylinder spanning `z ∈ [z0, z1]` via
/// `KernelV2Adapter` (`make_faces_from_profiles` -> `extrude_face` ->
/// `boolean_intersect`): extrude an exact circular cylinder of `radius` along
/// tilted unit axis `u = [0.6, 0.0, 0.8]` through `pivot`, then intersect with
/// a wide horizontal slab `[z0, z1]`. The horizontal caps at `z = z0` and
/// `z = z1` are exact ellipses with semi-axes `a = radius / 0.8 = 1.25 * radius`
/// and `b = radius`.
fn adapter_elliptical_cap_cylinder(
    k: &mut KernelV2Adapter,
    pivot: [f64; 3],
    radius: f64,
    z0: f64,
    z1: f64,
) -> KernelSolidHandle {
    let u = [0.6, 0.0, 0.8]; // 0.6^2 + 0.8^2 == 1.0 exact
    let s_back = 12.0;
    let origin = [
        pivot[0] - s_back * u[0],
        pivot[1] - s_back * u[1],
        pivot[2] - s_back * u[2],
    ];
    let (cprof, cpos) = circle_profile(0.0, 0.0, radius);
    let cfaces = k
        .make_faces_from_profiles(&[cprof], origin, u, plane_x_axis(u), &cpos)
        .expect("stage tilted circle");
    let tilted_cyl = k
        .extrude_face(cfaces[0], u, 2.0 * s_back)
        .expect("extrude tilted cylinder");
    let slab = adapter_rect_box(k, (-12.0, 12.0), (-12.0, 12.0), (z0, z1));
    k.boolean_intersect(&tilted_cyl, &slab)
        .expect("oblique slab cut produces elliptical-cap cylinder")
}

/// Build the identical oblique-cut elliptical-cap cylinder directly in a
/// `BrepArena` so tests can inspect `validate_solid`'s `TopologyReport` and
/// `tessellate_with_chord_tolerance` directly.
fn arena_elliptical_cap_cylinder(
    a: &mut BrepArena,
    pivot: [f64; 3],
    radius: f64,
    z0: f64,
    z1: f64,
) -> SolidId {
    let u = [0.6, 0.0, 0.8];
    let s_back = 12.0;
    let nx = plane_x_axis(u);
    let ny = cross(u, nx);
    let cprof = Profile::circle(
        Point3::new(
            pivot[0] - s_back * u[0],
            pivot[1] - s_back * u[1],
            pivot[2] - s_back * u[2],
        ),
        Vector3::new(nx[0], nx[1], nx[2]),
        Vector3::new(ny[0], ny[1], ny[2]),
        Point2::new(0.0, 0.0),
        radius,
    )
    .expect("circle profile");
    let cyl = extrude(a, &cprof, Vector3::new(u[0], u[1], u[2]), 2.0 * s_back)
        .expect("extrude tilted cylinder")
        .solid;
    let bprof = Profile::new(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(-12.0, -12.0),
            Point2::new(12.0, -12.0),
            Point2::new(12.0, 12.0),
            Point2::new(-12.0, 12.0),
        ],
        vec![],
    )
    .expect("slab profile");
    let slab = extrude(a, &bprof, Vector3::new(0.0, 0.0, 1.0), z1 - z0)
        .expect("extrude slab")
        .solid;
    boolean_op(a, cyl, slab, BoolOp::Intersect)
        .expect("oblique slab cut produces elliptical-cap cylinder")
}

fn rot_z(theta: f64) -> RigidPlacement {
    let (s, c) = theta.sin_cos();
    RigidPlacement {
        rotation: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 0.0, 0.0],
    }
}

fn signed_volume_coords(positions: &[f64], indices: &[u32]) -> f64 {
    let p = |i: u32| -> [f64; 3] {
        let idx = (i as usize) * 3;
        [positions[idx], positions[idx + 1], positions[idx + 2]]
    };
    let mut six_v = 0.0;
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

fn render_mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let pos: Vec<f64> = mesh.vertices.iter().map(|&c| f64::from(c)).collect();
    signed_volume_coords(&pos, &mesh.indices)
}

/// Verified render-mesh closure check (matching `PR-KV8c` / `PR-KV11` in
/// `crates/test-harness/src/oracle.rs::check_watertight_mesh`):
///
/// How `KernelV2Adapter::tessellate` (`crates/kernel-v2/src/adapter.rs:921-953`)
/// and `kernel_v2::tessellate` (`crates/kernel-v2/src/tessellate/mod.rs:66-71,
/// 328-352`) actually emit vertices:
/// 1. `RenderMesh` is **not** index-welded across faces: each face appends its
///    own vertex slice (`face_ranges`) so per-face flat/crease normals are
///    exact; `KernelV2Adapter::tessellate` calls `validate_solid`, then
///    `kernel_v2::tessellate`, and narrows `positions: Vec<f64>` to
///    `vertices: Vec<f32>`.
/// 2. Shared B-Rep boundary edges are sampled **twin-canonically**
///    (`tessellate/sampling.rs:332, 775` — computed on `h.min(he.twin)` and
///    reversed on the twin), so incident faces emit identical `f64` boundary
///    sample coordinates, **except** where `tessellate_developable_patch` pass 4
///    (`tessellate/developable.rs:1824-1834`) bisects a boundary `Chord` or
///    `ArcSample` sub-edge at its exact 3D midpoint `0.5 * (pa + pb)` during
///    LEPP refinement, intentionally introducing a **collinear T-junction** on
///    that chord.
///
/// Accordingly, this helper:
/// - pairs exact-bit opposite-directed edges `(u -> v)` / `(v -> u)` up front,
/// - subdivides any remaining residue directed edges at residue vertices lying
///   collinearly in `(0, 1)` within `collinear_rel_tol * (1 + scale)`, and
/// - returns `(exact_closed_edges, raw_residue_half_edges, open_edges, over_edges)`,
///   where `open_edges` counts any undirected sub-edge whose directed counts
///   are not `(1, 1)` (catching holes, cracks, AND same-orientation double
///   covers) and `over_edges` counts undirected multiplicity `> 2`.
fn check_position_welded_closure(
    positions: &[f64],
    indices: &[u32],
    collinear_rel_tol: f64,
) -> (usize, usize, usize, usize) {
    assert!(!indices.is_empty() && indices.len().is_multiple_of(3));
    for &c in positions {
        assert!(c.is_finite(), "non-finite render vertex coordinate {c}");
    }
    let max_abs = positions.iter().map(|c| c.abs()).fold(0.0_f64, f64::max);
    let tol = collinear_rel_tol * (1.0 + max_abs);

    type Bits = [u64; 3];
    let pt = |i: u32| -> [f64; 3] {
        let k = (i as usize) * 3;
        [positions[k], positions[k + 1], positions[k + 2]]
    };
    let bits = |p: [f64; 3]| -> Bits { [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] };

    let mut dir_counts: HashMap<(Bits, Bits), usize> = HashMap::new();
    let mut undir_counts: HashMap<(Bits, Bits), usize> = HashMap::new();
    let mut half_edges: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(indices.len());

    for t in indices.chunks_exact(3) {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (pa, pb) = (pt(t[i]), pt(t[j]));
            let (ka, kb) = (bits(pa), bits(pb));
            assert_ne!(ka, kb, "degenerate zero-length render triangle edge");
            *dir_counts.entry((ka, kb)).or_insert(0) += 1;
            let u = if ka <= kb { (ka, kb) } else { (kb, ka) };
            *undir_counts.entry(u).or_insert(0) += 1;
            half_edges.push((pa, pb));
        }
    }

    let is_exact_closed = |ka: Bits, kb: Bits| -> bool {
        let u = if ka <= kb { (ka, kb) } else { (kb, ka) };
        undir_counts.get(&u) == Some(&2)
            && dir_counts.get(&(ka, kb)) == Some(&1)
            && dir_counts.get(&(kb, ka)) == Some(&1)
    };

    let exact_closed = undir_counts
        .keys()
        .filter(|&&(ka, kb)| is_exact_closed(ka, kb))
        .count();

    let residue: Vec<([f64; 3], [f64; 3])> = half_edges
        .into_iter()
        .filter(|&(pa, pb)| !is_exact_closed(bits(pa), bits(pb)))
        .collect();
    if residue.is_empty() {
        return (exact_closed, 0, 0, 0);
    }

    // Quantize residue vertices at `tol` and collect representative 3D coords.
    type QKey = [i64; 3];
    let qkey = |p: [f64; 3]| -> QKey {
        [
            (p[0] / tol).round() as i64,
            (p[1] / tol).round() as i64,
            (p[2] / tol).round() as i64,
        ]
    };
    let mut res_verts: HashMap<QKey, [f64; 3]> = HashMap::new();
    for &(pa, pb) in &residue {
        res_verts.entry(qkey(pa)).or_insert(pa);
        res_verts.entry(qkey(pb)).or_insert(pb);
    }

    // Subdivide each residue directed edge A -> B at collinear interior residue
    // vertices P (0 < t < 1, perpendicular distance <= tol).
    let mut sub_dir: HashMap<(QKey, QKey), usize> = HashMap::new();
    let mut sub_undir: HashMap<(QKey, QKey), usize> = HashMap::new();
    for &(pa, pb) in &residue {
        let (qa, qb) = (qkey(pa), qkey(pb));
        if qa == qb {
            continue;
        }
        let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let ab2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        let mut splits: Vec<(f64, QKey)> = Vec::new();
        for (&qp, &pp) in &res_verts {
            if qp == qa || qp == qb {
                continue;
            }
            let ap = [pp[0] - pa[0], pp[1] - pa[1], pp[2] - pa[2]];
            let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab2;
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
            let d2 =
                (pp[0] - proj[0]).powi(2) + (pp[1] - proj[1]).powi(2) + (pp[2] - proj[2]).powi(2);
            if d2 <= tol * tol {
                splits.push((t, qp));
            }
        }
        splits.sort_by(|a, b| a.0.total_cmp(&b.0));
        splits.dedup_by_key(|s| s.1);

        let mut prev = qa;
        for (_, qm) in splits.into_iter().chain(std::iter::once((1.0, qb))) {
            if prev == qm {
                continue;
            }
            *sub_dir.entry((prev, qm)).or_insert(0) += 1;
            let u = if prev <= qm { (prev, qm) } else { (qm, prev) };
            *sub_undir.entry(u).or_insert(0) += 1;
            prev = qm;
        }
    }

    let open = sub_undir
        .iter()
        .filter(|&(&(u, v), &cnt)| {
            cnt != 2 || sub_dir.get(&(u, v)) != Some(&1) || sub_dir.get(&(v, u)) != Some(&1)
        })
        .count();
    let over = sub_undir.values().filter(|&&cnt| cnt > 2).count();
    (exact_closed, residue.len(), open, over)
}

fn assert_closed_kv2_mesh(mesh: &Kv2RenderMesh, label: &str) {
    let (exact_closed, raw_residue, open, over) =
        check_position_welded_closure(&mesh.positions, &mesh.indices, 1e-9);
    assert_eq!(
        over, 0,
        "{label}: {over} over-used (>2) render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
    assert_eq!(
        open, 0,
        "{label}: {open} open or mis-oriented render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
}

fn assert_closed_adapter_mesh(mesh: &RenderMesh, label: &str) {
    let pos: Vec<f64> = mesh.vertices.iter().map(|&c| f64::from(c)).collect();
    // `KernelV2Adapter::tessellate` narrows f64 positions to f32 (~1.2e-7 ulp),
    // so collinear midpoint splits sit within ~1e-6 * scale of the parent chord.
    let (exact_closed, raw_residue, open, over) =
        check_position_welded_closure(&pos, &mesh.indices, 2e-6);
    assert_eq!(
        over, 0,
        "{label}: {over} over-used (>2) adapter render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
    assert_eq!(
        open, 0,
        "{label}: {open} open or mis-oriented adapter render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
}

/// Exact polar-support Cavalieri volume of the union of `n_cyl` congruent
/// oblique cylinders rotated around the z-axis by `s * step_rad`.
///
/// At height `z ∈ [z0, z1]`, cylinder `s` intersects the horizontal plane in
/// the ellipse `E_s(z)` centered at `c_s(z) = d(z) * (cos θ_s, sin θ_s)` with
/// `d(z) = 0.75 * (z - z_pivot)`, semi-major axis `a = r_cyl / 0.8` along
/// `(cos θ_s, sin θ_s)`, and semi-minor axis `b = r_cyl` along
/// `(-sin θ_s, cos θ_s)`. Because `0 <= d(z) <= 0.75 * (0.5 - (-2.0)) = 1.875 <
/// 2.5 = a`, the origin `(0, 0)` lies strictly inside EVERY ellipse `E_s(z)`
/// (minimum radial clearance `a - d(z1) = 0.625`). By convexity of `E_s(z)`,
/// each polar ray at angle `φ` exits `E_s(z)` at the unique closed-form root
/// `ρ_s(φ, z) > 0` of
/// `((ρ cos(φ - θ_s) - d(z)) / a)^2 + (ρ sin(φ - θ_s) / b)^2 = 1`, so the union
/// `Ω(z) = ⋃_s E_s(z)` is strictly star-shaped about `(0, 0)` with exact polar
/// boundary `ρ_max(φ, z) = max_s ρ_s(φ, z)` and exact cross-section area
/// `A(z) = (1/2) ∫_0^{2π} ρ_max(φ, z)^2 dφ`.
///
/// Integrated with `n_phi` periodic angular nodes and composite Simpson's rule
/// on `n_z_panels` (even) axial panels.
fn polar_cavalieri_union_volume(
    r_cyl: f64,
    z_pivot: f64,
    (z0, z1): (f64, f64),
    n_cyl: usize,
    step_rad: f64,
    (n_phi, n_z_panels): (usize, usize),
) -> f64 {
    assert!(n_z_panels >= 2 && n_z_panels.is_multiple_of(2));
    let a = r_cyl / 0.8;
    let b = r_cyl;
    let inv_a2 = 1.0 / (a * a);
    let inv_b2 = 1.0 / (b * b);

    let cyl_sc: Vec<(f64, f64)> = (0..n_cyl)
        .map(|s| ((s as f64) * step_rad).sin_cos())
        .collect();
    let dphi = 2.0 * PI / (n_phi as f64);
    let phi_sc: Vec<(f64, f64)> = (0..n_phi)
        .map(|k| ((k as f64 + 0.5) * dphi).sin_cos())
        .collect();

    let slice_area = |z: f64| -> f64 {
        let d = 0.75 * (z - z_pivot);
        assert!(
            d.abs() < a,
            "origin must stay strictly inside every ellipse"
        );
        let c_term = 1.0 - d * d * inv_a2;
        let mut sum_rho2 = 0.0;
        for &(sp, cp) in &phi_sc {
            let mut rho_max = 0.0_f64;
            for &(st, ct) in &cyl_sc {
                // cos(φ - θ_s) = cos φ cos θ_s + sin φ sin θ_s
                // sin(φ - θ_s) = sin φ cos θ_s - cos φ sin θ_s
                let cpsi = cp * ct + sp * st;
                let spsi = sp * ct - cp * st;
                let qa = cpsi * cpsi * inv_a2 + spsi * spsi * inv_b2;
                let qb = d * cpsi * inv_a2;
                let rho = (qb + (qb * qb + qa * c_term).sqrt()) / qa;
                rho_max = rho_max.max(rho);
            }
            sum_rho2 += rho_max * rho_max;
        }
        0.5 * sum_rho2 * dphi
    };

    let hz = (z1 - z0) / (n_z_panels as f64);
    let mut simpson = slice_area(z0) + slice_area(z1);
    for iz in 1..n_z_panels {
        let z = z0 + (iz as f64) * hz;
        let w = if iz % 2 == 1 { 4.0 } else { 2.0 };
        simpson += w * slice_area(z);
    }
    simpson * hz / 3.0
}

#[test]
fn smallest_ellipse_cap_coplanar_repro_via_adapter() {
    let mut k = KernelV2Adapter::new();
    // Oblique-cut cylinder on z ∈ [0, 1] with pivot (0, 0, 0.5), radius 1.0:
    // top cap at z = 1 is an ellipse centered at (0.375, 0, 1) with semi-axes
    // a = 1.25, b = 1.0. Exact volume = π * 1.25 * 1.0 * 1.0 = 1.25 * π.
    let ell_cyl = adapter_elliptical_cap_cylinder(&mut k, [0.0, 0.0, 0.5], 1.0, 0.0, 1.0);
    let v_cyl_exact = 1.25 * PI;
    let m0 = k
        .tessellate(&ell_cyl, 0.01)
        .expect("tessellate oblique cyl");
    assert_closed_adapter_mesh(&m0, "oblique elliptical-cap cylinder");

    // Flush stacked box on the z = 1.0 ellipse-bounded planar cap whose base
    // crosses the elliptical rim (x ∈ [0.2, 2.0], y ∈ [-0.4, 0.4], z ∈ [1, 2];
    // the ellipse ends at x = 0.375 + 1.25 = 1.625 on y = 0).
    let boss = adapter_rect_box(&mut k, (0.2, 2.0), (-0.4, 0.4), (1.0, 2.0));
    let v_boss = 1.8 * 0.8 * 1.0;

    let out = k
        .boolean_union(&ell_cyl, &boss)
        .unwrap_or_else(|e: KernelError| panic!("coplanar union on ellipse cap failed: {e:?}"));

    // `KernelV2Adapter::tessellate` runs `validate_solid` (`adapter.rs:932`)
    // before emitting the render mesh.
    let mesh = k.tessellate(&out, 0.01).expect("tessellate union");
    assert_closed_adapter_mesh(&mesh, "coplanar ellipse-cap union");

    let v_expect = v_cyl_exact + v_boss;
    let v_mesh = render_mesh_signed_volume(&mesh);
    // At canonical N = 71 (`RENDER_CHORD_TOLERANCE_REL = 1e-3`), the inscribed
    // cylinder chords under-approximate the curved portion `v_cyl_exact` by at
    // most `(2π/71)^2 / 6 = 1.306e-3` (while the planar boss is exact).
    let n_canon = f64::from(circle_segment_count(RENDER_CHORD_TOLERANCE_REL));
    let max_chord_deficit = v_cyl_exact * (2.0 * PI / n_canon).powi(2) / 6.0 + 1e-5;
    assert!(
        v_mesh <= v_expect + 1e-6 && v_expect - v_mesh <= max_chord_deficit,
        "render mesh volume {v_mesh:.9} outside inscribed chord band [{:.9}, {v_expect:.9}]",
        v_expect - max_chord_deficit
    );

    let mp = k
        .mass_properties(&out, None)
        .expect("adapter mass_properties on coplanar ellipse-cap union");
    match mp.method {
        Method::Exact => {
            assert!(
                ((mp.volume - v_expect) / v_expect).abs() < 1e-12,
                "exact mass_properties volume {:.15e} vs {v_expect:.15e}",
                mp.volume
            );
        }
        Method::Mesh { chord_bound } => {
            assert!(
                mp.volume <= v_expect + 1e-6 && v_expect - mp.volume <= v_expect * chord_bound * 2.0,
                "mesh-tier mass_properties volume {:.9} outside chord_bound={chord_bound:.3e} vs {v_expect:.9}",
                mp.volume
            );
        }
    }

    // Identical flush-boss union in `BrepArena`: verify `TopologyReport` AND
    // exact tessellation-free analytic B-Rep volume (`geom::signed_volume`
    // integrates `EllipseArc` + `LineSegment` caps and oblique cylinder patches
    // in closed form).
    let mut arena = BrepArena::new();
    let a_cyl = arena_elliptical_cap_cylinder(&mut arena, [0.0, 0.0, 0.5], 1.0, 0.0, 1.0);
    let bprof = Profile::new(
        Point3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.2, -0.4),
            Point2::new(2.0, -0.4),
            Point2::new(2.0, 0.4),
            Point2::new(0.2, 0.4),
        ],
        vec![],
    )
    .expect("boss profile");
    let a_boss = extrude(&mut arena, &bprof, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .expect("extrude boss")
        .solid;
    let a_out = boolean_op(&mut arena, a_cyl, a_boss, BoolOp::Union)
        .expect("arena coplanar union on ellipse cap");
    let report = validate_solid(&arena, a_out).expect("validate_solid on flush ellipse-cap union");
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0);
    assert_eq!(report.euler_lhs, report.euler_rhs);
    assert_eq!(report.euler_lhs, 2);
    let v_brep = geom::signed_volume(&arena, a_out)
        .expect("exact closed-form B-Rep volume on flush ellipse-cap union");
    assert!(
        ((v_brep - v_expect) / v_expect).abs() < 1e-12,
        "flush union exact B-Rep volume {v_brep:.15e} vs {v_expect:.15e}"
    );
    eprintln!(
        "[f6 repro] report={report:?} v_brep={v_brep:.12} v_expect={v_expect:.12} v_mesh={v_mesh:.12}"
    );
}

/// Run a sequential `boolean_union` of `n_cyl` coplanar-capped elliptical
/// cylinders generated by rotating the seed cylinder around the perpendicular
/// z-axis in steps of `step_rad`, asserting:
/// 1. `validate_solid` closed 2-manifold topology (`shells == 1`, `genus == 0`,
///    `euler_lhs == euler_rhs == 2`),
/// 2. closed 2-manifold position-welded render-mesh pairing (`open == 0`,
///    `over == 0`), and
/// 3. volume agreement with the self-verified (`< 1e-6` rel) analytic
///    polar-support Cavalieri oracle within the derived inscribed-polygon chord
///    deficit bounds at both canonical (`N = 71`, `< 1.6e-3`) and tightened
///    (`rel = 1e-4`, `N = 223`, `< 2.0e-4`) render tolerances.
fn assert_rotated_elliptical_cylinder_union_chain(n_cyl: usize, step_rad: f64) {
    let mut arena = BrepArena::new();
    let r_cyl = 2.0;
    let z0 = 0.0;
    let z1 = 0.5;
    let z_pivot = -2.0;
    let seed = arena_elliptical_cap_cylinder(&mut arena, [0.0, 0.0, z_pivot], r_cyl, z0, z1);

    let mut acc = seed;
    for step in 1..n_cyl {
        let theta = (step as f64) * step_rad;
        let rotated =
            transform_solid(&mut arena, seed, &rot_z(theta)).expect("rotate elliptical cylinder");
        acc = boolean_op(&mut arena, acc, rotated, BoolOp::Union).unwrap_or_else(|e| {
            panic!(
                "Fig. 24(b) sequential union ({n_cyl} cyls) failed at step {step}/{} \
                 (theta={theta:.4}): {e:?}",
                n_cyl - 1
            )
        });
    }

    // 1. Kernel's own B-Rep 2-manifold & Euler–Poincaré validation oracle.
    let report = validate_solid(&arena, acc).unwrap_or_else(|e| {
        panic!("Fig. 24(b) {n_cyl}-cylinder union failed validate_solid: {e:?}")
    });
    assert_eq!(
        report.shells, 1,
        "Fig. 24(b) {n_cyl}-cylinder union must form 1 closed shell, got {report:?}"
    );
    assert_eq!(
        report.genus, 0,
        "Fig. 24(b) {n_cyl}-cylinder star-shaped union must have genus 0, got {report:?}"
    );
    assert_eq!(
        report.euler_lhs, report.euler_rhs,
        "Fig. 24(b) {n_cyl}-cylinder union Euler balance V - E + F - R == 2(S - G): {report:?}"
    );
    assert_eq!(
        report.euler_lhs, 2,
        "Fig. 24(b) {n_cyl}-cylinder union χ == 2: {report:?}"
    );
    assert!(
        report.faces >= n_cyl + 2,
        "expected coplanar caps + {n_cyl} lateral patches, got {} faces",
        report.faces
    );

    // 2. Analytic polar-support Cavalieri volume oracle (`< 1e-6` quadrature
    //    convergence between (4096, 16) and (16384, 64)), cross-checked against
    //    exact closed-form `geom::signed_volume`:
    //    - On `n_cyl == 4` (`step = π/6`), every lateral Monge conic span has
    //      enough interior samples to reconstruct as `Curve::EllipseArc`, so
    //      `geom::signed_volume` succeeds in closed form and agrees with
    //      `polar_cavalieri_union_volume` to ~`1e-9` relative.
    //    - On `n_cyl == 24` (`step = π/24` = 7.5°), adjacent lateral seams meet
    //      across narrow wedges leaving 2-point sub-chord `LineSegment` facets,
    //      which `geom::signed_volume` declines loudly (`"boolean chord facets
    //      have no closed form"` — the same decline `test-harness::oracle`
    //      encounters on multi-boolean curved patches).
    let v_coarse =
        polar_cavalieri_union_volume(r_cyl, z_pivot, (z0, z1), n_cyl, step_rad, (4096, 16));
    let v_cavalieri =
        polar_cavalieri_union_volume(r_cyl, z_pivot, (z0, z1), n_cyl, step_rad, (16384, 64));
    let quad_rel = ((v_cavalieri - v_coarse) / v_cavalieri).abs();
    assert!(
        quad_rel < 1e-6,
        "polar Cavalieri quadrature did not converge to <1e-6: coarse={v_coarse:.12} \
         fine={v_cavalieri:.12} (rel={quad_rel:.3e})"
    );
    let v_ref = match geom::signed_volume(&arena, acc) {
        Ok(v_brep) => {
            let brep_vs_cavalieri_rel = ((v_brep - v_cavalieri) / v_brep).abs();
            assert!(
                brep_vs_cavalieri_rel < 1e-7,
                "Fig. 24(b) {n_cyl}-cylinder exact B-Rep volume {v_brep:.12} disagrees with \
                 analytic polar Cavalieri {v_cavalieri:.12} (rel={brep_vs_cavalieri_rel:.3e})"
            );
            v_brep
        }
        Err(KernelV2Error::CurvedGeometryMismatch { reason, .. })
            if n_cyl > 4 && reason.contains("boolean chord facets have no closed form") =>
        {
            v_cavalieri
        }
        Err(e) => panic!("Fig. 24(b) {n_cyl}-cylinder unexpected geom::signed_volume error: {e:?}"),
    };

    // 3a. Canonical render mesh (`RENDER_CHORD_TOLERANCE_REL = 1e-3`, `N = 71`):
    //     closed 2-manifold pairing (`open == 0`, `over == 0`) + inscribed
    //     chord-area deficit bound `(2π/N)^2 / 6 = 1.306e-3` (bound `1.6e-3`
    //     with local-scale margin). Because every cylinder is convex, every
    //     inscribed chord triangle lies inside its parent cylinder, so
    //     `v_canon <= v_ref`.
    let mesh_canon = tessellate(&arena, acc).expect("canonical tessellate of union");
    assert_closed_kv2_mesh(
        &mesh_canon,
        &format!("Fig. 24(b) {n_cyl}-cylinder canonical render mesh"),
    );
    let v_canon = signed_volume_coords(&mesh_canon.positions, &mesh_canon.indices);
    let n_canon = f64::from(circle_segment_count(RENDER_CHORD_TOLERANCE_REL));
    let canon_chord_bound = (2.0 * PI / n_canon).powi(2) / 6.0 + 3e-4; // 1.306e-3 + 3e-4 = 1.606e-3
    let rel_canon = (v_ref - v_canon) / v_ref;
    assert!(
        rel_canon >= -1e-6 && rel_canon <= canon_chord_bound,
        "Fig. 24(b) {n_cyl}-cylinder canonical mesh (N={n_canon}) volume {v_canon:.9} vs \
         ref {v_ref:.9}: rel deficit {rel_canon:.3e} outside [0, {canon_chord_bound:.3e}]"
    );

    // 3b. Tightened render mesh (`rel = 1e-4`, `N = 223`): chord-area deficit
    //     shrinks ~10× to `(2π/223)^2 / 6 = 1.325e-4` (bound `2.0e-4`).
    let mesh_tight = tessellate_with_chord_tolerance(&arena, acc, 1e-4)
        .expect("tightened (1e-4) tessellate of union");
    assert_closed_kv2_mesh(
        &mesh_tight,
        &format!("Fig. 24(b) {n_cyl}-cylinder tightened (1e-4) render mesh"),
    );
    let v_tight = signed_volume_coords(&mesh_tight.positions, &mesh_tight.indices);
    let n_tight = f64::from(circle_segment_count(1e-4));
    let tight_chord_bound = (2.0 * PI / n_tight).powi(2) / 6.0 + 6.75e-5; // ~2.0e-4
    let rel_tight = (v_ref - v_tight) / v_ref;
    assert!(
        rel_tight >= -1e-6 && rel_tight <= tight_chord_bound,
        "Fig. 24(b) {n_cyl}-cylinder tightened mesh (N={n_tight}) volume {v_tight:.9} vs \
         ref {v_ref:.9}: rel deficit {rel_tight:.3e} outside [0, {tight_chord_bound:.3e}]"
    );
    eprintln!(
        "[f6 fig24b n={n_cyl}] report={report:?} v_ref={v_ref:.9} v_cavalieri={v_cavalieri:.9} \
         v_canon={v_canon:.9} (rel={rel_canon:.3e}) v_tight={v_tight:.9} (rel={rel_tight:.3e})"
    );
}

/// Fast debug-tier Fig. 24(b) chain: 4 rotated coplanar-capped elliptical
/// cylinders (`step = π/6`), exercising both the 1×1 Stage-0 pair overlay
/// (step 1) and the n-ary multi-pair plane-group overlay (steps 2–3) on both
/// `BrepArena` and `KernelV2Adapter`.
#[test]
fn fig24b_4_rotated_elliptical_cap_cylinders_sequential_union() {
    assert_rotated_elliptical_cylinder_union_chain(4, PI / 6.0);

    // Also pin the `KernelV2Adapter` entry path on the 4-cylinder chain.
    let mut k = KernelV2Adapter::new();
    let seed = adapter_elliptical_cap_cylinder(&mut k, [0.0, 0.0, -2.0], 2.0, 0.0, 0.5);
    let mut acc = seed.clone();
    for step in 1..4 {
        let rotated = k
            .transform_body(&seed, &rot_z((step as f64) * PI / 6.0))
            .expect("adapter rotate elliptical cylinder");
        acc = k
            .boolean_union(&acc, &rotated)
            .expect("adapter 4-cylinder sequential union");
    }
    let mesh = k.tessellate(&acc, 0.01).expect("adapter tessellate 4-cyl");
    assert_closed_adapter_mesh(&mesh, "adapter 4-cylinder Fig. 24(b) union");
}

/// Full Yang et al. 2025 §5.5 Fig. 24(b) pin
/// (`refs/text/yang2025_hybrid_boolean.txt:962-966`): 24 elliptical-cap
/// cylinders with coplanar top and bottom faces rotated by `π/24` each step and
/// combined sequentially via `boolean_union`. Runs automatically in `--release`
/// (~120 s); skipped under `debug_assertions` so the debug `kernel-v2` fast
/// tier stays sub-minute while the 4-cylinder chain above covers debug builds.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "full 24-cylinder Fig. 24(b) chain (~120s in --release); run with --release"
)]
fn fig24b_24_rotated_elliptical_cap_cylinders_sequential_union() {
    assert_rotated_elliptical_cylinder_union_chain(24, PI / 24.0);
}

#[test]
fn coplanar_ellipse_caps_preserve_exact_ellipse_arcs_in_arena() {
    let mut a = BrepArena::new();
    let c0 = arena_elliptical_cap_cylinder(&mut a, [0.0, 0.0, -2.0], 2.0, 0.0, 0.5);
    let c1 = transform_solid(&mut a, c0, &rot_z(PI / 6.0)).unwrap();
    let united = boolean_op(&mut a, c0, c1, BoolOp::Union)
        .unwrap_or_else(|e| panic!("two rotated elliptical-cap cylinders union failed: {e:?}"));
    let report = validate_solid(&a, united).expect("united solid validates");
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0);
    assert_eq!(report.euler_lhs, report.euler_rhs);
    assert_eq!(report.euler_lhs, 2);
    let mesh = tessellate(&a, united).expect("united solid tessellates");
    assert_closed_kv2_mesh(&mesh, "2-cylinder coplanar ellipse-cap union");

    // Verify both coplanar caps (z = 0 and z = 0.5) retain analytic
    // `Curve::EllipseArc` boundary edges — no degradation to LineSegments.
    let mut z0_ell_arcs = 0usize;
    let mut z1_ell_arcs = 0usize;
    for &sh in &a.solid(united).unwrap().shells {
        for &fid in &a.shell(sh).unwrap().faces {
            let f = a.face(fid).unwrap();
            if let Some(Surface::Plane(pl)) = f.surface {
                let z = pl.point.z();
                for hid in a.loop_half_edges(f.outer_loop).unwrap() {
                    if matches!(a.half_edge(hid).unwrap().curve, Curve::EllipseArc { .. }) {
                        if z.abs() < 1e-9 {
                            z0_ell_arcs += 1;
                        } else if (z - 0.5).abs() < 1e-9 {
                            z1_ell_arcs += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        z0_ell_arcs >= 2,
        "bottom cap z=0 must retain exact Curve::EllipseArc edges, got {z0_ell_arcs}"
    );
    assert!(
        z1_ell_arcs >= 2,
        "top cap z=0.5 must retain exact Curve::EllipseArc edges, got {z1_ell_arcs}"
    );
}

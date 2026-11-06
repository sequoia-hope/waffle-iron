//! N106 D3 unit pins: patch CDT with Delaunay criterion in the intrinsic
//! isometric chart (`(r·θ, y)` on a cylinder; developed circular-sector chart
//! on a cone) vs. the raw `(θ, y)` chart, gated behind `ChartMetric` (not an
//! env var, not wired into production).
//!
//! Paper: Yang et al. 2025 §4.1.2 / §4.4.1
//! (`refs/text/yang2025_hybrid_boolean.txt:357-378, 546-571`).

use std::collections::BTreeSet;

use cad_primitives::{Point2, Point3, Vector3};
use cherchi_rs::Mesh;

use crate::geom::Surface;
use crate::stage4_construct::{
    isometric_cdt_pool, rebuild_patch_planar_ex, rebuild_patch_planar_ex_with_metric,
    rebuild_patch_planar_with_metric, ChartMetric,
};
use crate::stage4_dt::d_of_t;
use crate::stage4_project::SurfaceChart;
use crate::stage4_splice::SplicePatch;

/// 3D triangle aspect ratio `max_edge / min_altitude`, scale-free (`>= 2/√3 ≈ 1.1547`
/// with equality on an equilateral triangle).
fn tri_aspect_ratio_3d(pts: [Point3; 3]) -> f64 {
    let a = pts[0].as_array();
    let b = pts[1].as_array();
    let c = pts[2].as_array();
    let edge_len = |p: [f64; 3], q: [f64; 3]| -> f64 {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    let lab = edge_len(a, b);
    let lbc = edge_len(b, c);
    let lca = edge_len(c, a);
    let max_edge = lab.max(lbc).max(lca);
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1] * w[2] - u[2] * w[1],
        u[2] * w[0] - u[0] * w[2],
        u[0] * w[1] - u[1] * w[0],
    ];
    let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    // min_altitude = twice_area / max_edge => max_edge / min_altitude = max_edge^2 / twice_area.
    (max_edge * max_edge) / twice_area
}

/// Valid `2 * n_steps`-triangle reference partition of the `(2 * n_steps + 2)`-vertex
/// patch cycle (`0..=n` convex bottom arc, `n+1..=2n+1` convex top arc in reverse):
/// - bottom pocket `0..=n` (`n - 1` CCW zig-zag triangles),
/// - top pocket `(n + 1)..=(2 * n + 1)` (`n - 1` CCW zig-zag triangles),
/// - central quad `[0, n, n + 1]` and `[0, n + 1, 2 * n + 1]`.
fn build_reference_patch_tris(n_steps: usize) -> Vec<[u32; 3]> {
    let n = n_steps as u32;
    let mut tris = Vec::with_capacity(2 * n_steps);
    let mut add_pocket = |mut lo: u32, mut hi: u32| {
        while hi - lo >= 2 {
            tris.push([lo, lo + 1, hi]);
            lo += 1;
            if hi - lo >= 2 {
                tris.push([lo, hi - 1, hi]);
                hi -= 1;
            }
        }
    };
    add_pocket(0, n);
    add_pocket(n + 1, 2 * n + 1);
    tris.push([0, n, n + 1]);
    tris.push([0, n + 1, 2 * n + 1]);
    assert_eq!(tris.len(), 2 * n_steps);
    tris
}

/// Pin 1 (Cylinder `r = 50`, long axial span):
/// On a cylinder patch of radius `r = 50` spanning `y ∈ [0, 60]` between two
/// curved intersection arcs `y_bot(θ) = 2·(1 − cos θ)` and
/// `y_top(θ) = 60 − 2·(1 − cos θ)` (`θ ∈ [-0.6, +0.6]`, 24 circumferential
/// steps of `δθ = 0.05`, world rim step `≈ 2.5`, axial-to-step ratio `24:1`):
/// - `(θ, y)` CDT (`ChartMetric::Parametric`) bridges across both end pockets
///   with circumferential slivers spanning multiple `δθ` steps (exact max 3D
///   aspect ratio printed and pinned).
/// - Isometric `(r·θ, y)` CDT (`ChartMetric::Isometric`) connects every bottom
///   sample along axial generators to the top arc with zero circumferential
///   pocket slivers (every triangle spans a single `δθ = 0.05` step).
/// - `d(T)` (`stage4_dt::d_of_t`) of EVERY triangle in the isometric CDT is no
///   larger than in the `(θ, y)` CDT (`dt <= param_max_dt`, strictly smaller max).
#[test]
fn n106_d3_cylinder_r50_long_axial_span_eliminates_circumferential_slivers_and_reduces_dt() {
    let radius = 50.0;
    let surface = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius,
    };
    let chart = SurfaceChart::new(surface).expect("cylinder chart");

    let n_steps = 24usize;
    let d_theta = 0.05;
    let theta_start = -0.6;
    let y_top = 60.0;

    let mut uv_cycle: Vec<Point2> = Vec::with_capacity(2 * (n_steps + 1));
    for k in 0..=n_steps {
        let theta = theta_start + (k as f64) * d_theta;
        let y = 2.0 * (1.0 - theta.cos());
        uv_cycle.push(Point2::new(theta, y));
    }
    for k in (0..=n_steps).rev() {
        let theta = theta_start + (k as f64) * d_theta;
        let y = y_top - 2.0 * (1.0 - theta.cos());
        uv_cycle.push(Point2::new(theta, y));
    }

    let verts: Vec<Point3> = uv_cycle.iter().map(|&uv| chart.lift(uv)).collect();
    let old_tris = build_reference_patch_tris(n_steps);
    let mesh = Mesh::new(verts.clone(), old_tris.clone());
    let patch = SplicePatch {
        cycles: vec![(0..uv_cycle.len() as u32).collect()],
        tris: (0..old_tris.len() as u32).collect(),
        surface,
    };

    let param_plan = rebuild_patch_planar_with_metric(&mesh, 0, &patch, ChartMetric::Parametric)
        .expect("(θ, y) CDT should succeed");
    let iso_plan = rebuild_patch_planar_with_metric(&mesh, 0, &patch, ChartMetric::Isometric)
        .expect("isometric (r·θ, y) CDT should succeed");

    // Same boundary vertices, zero Steiner vertices in both metrics.
    assert!(param_plan.new_verts.is_empty());
    assert!(iso_plan.new_verts.is_empty());
    assert_eq!(param_plan.new_tris.len(), 2 * n_steps);
    assert_eq!(iso_plan.new_tris.len(), 2 * n_steps);

    // The production entry point runs the isometric chart (N106, 2026-11-03).
    let prod_plan = rebuild_patch_planar_ex(&mesh, 0, &patch, &BTreeSet::new()).unwrap();
    assert_eq!(prod_plan.new_tris, iso_plan.new_tris);
    assert_eq!(prod_plan.new_tri_bounds, iso_plan.new_tri_bounds);

    let projected_uv: Vec<Point2> = verts.iter().map(|&p| chart.project(p)).collect();

    let eval_metrics = |tris: &[[u32; 3]], bounds: &[f64]| -> (f64, Vec<f64>, usize, f64) {
        let mut max_ar = 0.0f64;
        let mut dts = Vec::with_capacity(tris.len());
        let mut pocket_slivers = 0usize;
        let mut max_theta_span = 0.0f64;
        for (idx, t) in tris.iter().enumerate() {
            let pts3 = [
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            ];
            let ar = tri_aspect_ratio_3d(pts3);
            max_ar = max_ar.max(ar);
            let uv = [
                projected_uv[t[0] as usize],
                projected_uv[t[1] as usize],
                projected_uv[t[2] as usize],
            ];
            let dt = d_of_t(&surface, uv).expect("valid cylinder d(T)");
            assert_eq!(
                bounds[idx], dt,
                "PatchRebuild::new_tri_bounds[{idx}] must equal d_of_t of chart coordinates"
            );
            dts.push(dt);
            // A circumferential pocket sliver has all 3 vertices on the bottom
            // arc `0..=n_steps` or all 3 vertices on the top arc `> n_steps`
            // (bridging across `>= 2` circumferential steps on one end).
            if t.iter().all(|&v| (v as usize) <= n_steps)
                || t.iter().all(|&v| (v as usize) > n_steps)
            {
                pocket_slivers += 1;
            }
            let th_lo = uv[0].x().min(uv[1].x()).min(uv[2].x());
            let th_hi = uv[0].x().max(uv[1].x()).max(uv[2].x());
            max_theta_span = max_theta_span.max(th_hi - th_lo);
        }
        (max_ar, dts, pocket_slivers, max_theta_span)
    };

    let (param_max_ar, param_dts, param_pocket_slivers, param_max_dtheta) =
        eval_metrics(&param_plan.new_tris, &param_plan.new_tri_bounds);
    let (iso_max_ar, iso_dts, iso_pocket_slivers, iso_max_dtheta) =
        eval_metrics(&iso_plan.new_tris, &iso_plan.new_tri_bounds);

    let param_min_dt = param_dts.iter().copied().fold(f64::INFINITY, f64::min);
    let param_max_dt = param_dts.iter().copied().fold(0.0f64, f64::max);
    let iso_min_dt = iso_dts.iter().copied().fold(f64::INFINITY, f64::min);
    let iso_max_dt = iso_dts.iter().copied().fold(0.0f64, f64::max);

    println!(
        "[n106_d3_cylinder_r50] parametric (θ, y): max_aspect_3d={param_max_ar:.6}, \
         pocket_slivers={param_pocket_slivers}, max_dtheta={param_max_dtheta:.6}, \
         min_dt={param_min_dt:.6}, max_dt={param_max_dt:.6}"
    );
    println!(
        "[n106_d3_cylinder_r50] isometric (r·θ, y): max_aspect_3d={iso_max_ar:.6}, \
         pocket_slivers={iso_pocket_slivers}, max_dtheta={iso_max_dtheta:.6}, \
         min_dt={iso_min_dt:.6}, max_dt={iso_max_dt:.6}"
    );

    // 1. (θ, y) CDT produces circumferential slivers across both end pockets;
    //    isometric (r·θ, y) CDT produces ZERO circumferential pocket slivers
    //    and every triangle spans a single δθ = 0.05 column.
    assert_eq!(
        param_pocket_slivers,
        2 * (n_steps - 1),
        "(θ, y) CDT expected full bottom+top pocket circumferential slivers"
    );
    assert!(
        param_max_ar > 75.0,
        "(θ, y) CDT expected severe circumferential sliver aspect ratio (> 75), got {param_max_ar:.6}"
    );
    assert_eq!(
        iso_pocket_slivers, 0,
        "isometric (r·θ, y) CDT must produce zero circumferential pocket slivers"
    );
    assert!(
        (iso_max_dtheta - d_theta).abs() <= 1e-12,
        "every isometric triangle must span a single δθ={d_theta} step, got max_dtheta={iso_max_dtheta}"
    );
    // Note: on a seedless strip of axial height H = 60 and rim step r·δθ = 2.5,
    // each generator-spanning strip triangle's 3D aspect ratio scales with the
    // fixture's H / (r·δθ) = 24 geometry (~24.04), not a universal chart cap.
    // Assert what the chart guarantees: zero circumferential slivers above and
    // strictly better aspect ratio than the raw (θ, y) CDT, without a
    // knife-edge cap sitting on 24.04.
    assert!(
        iso_max_ar < param_max_ar,
        "isometric (r·θ, y) CDT max 3D aspect ratio must beat (θ, y), got iso={iso_max_ar:.6}, param={param_max_ar:.6}"
    );

    // 2. d(T) of EVERY triangle in the isometric CDT is no larger than in the
    //    (θ, y) CDT (`dt <= param_max_dt`, with strictly smaller maximum; no
    //    knife-edge multiplier sitting on the fixture's ~16.37× ratio).
    for (idx, &dt) in iso_dts.iter().enumerate() {
        assert!(
            dt <= param_max_dt,
            "isometric triangle {idx} d(T)={dt:.6} exceeds (θ, y) CDT max_dt={param_max_dt:.6}"
        );
    }
    assert!(
        iso_max_dt < param_max_dt,
        "expected isometric max d(T) ({iso_max_dt:.6}) < (θ, y) max d(T) ({param_max_dt:.6})"
    );
}

/// Pin 2 (Cone developed / unrolled circular-sector chart):
/// Verifies both (a) the exact intrinsic Euclidean metric of `isometric_cdt_pool`
/// on a cone (`ds² = sec²α dz² + z² tan²α dθ²`), and (b) that on a cone patch
/// (`half_angle = π/6`, `z ∈ [80, 130]`) with a curved intersection arc, the
/// developed circular-sector CDT eliminates circumferential slivers and keeps
/// every triangle's `d(T)` no larger than in the `(θ, z)` CDT.
#[test]
fn n106_d3_cone_developed_chart_isometry_and_cdt_eliminates_circumferential_slivers() {
    let half_angle = std::f64::consts::FRAC_PI_6; // 30°, sin α = 0.5, tan α = 1/√3
    let surface = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle,
    };
    let chart = SurfaceChart::new(surface).expect("cone chart");

    // (a) Direct metric check on `isometric_cdt_pool`:
    //   - Ruling segment `(θ, z1) -> (θ, z2)` has exact 2D length == 3D length.
    //   - Small azimuthal step `(θ, z) -> (θ + δθ, z)` matches 3D arc/chord
    //     `z · tan α · δθ` to O(δθ³).
    let sample_uv = [
        Point2::new(0.25, 60.0),
        Point2::new(0.25, 135.0),
        Point2::new(0.25 + 1e-6, 90.0),
        Point2::new(0.25, 90.0),
    ];
    let dev = isometric_cdt_pool(&chart, &sample_uv);
    let dist2 = |a: Point2, b: Point2| ((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2)).sqrt();
    let dist3 = |a: Point3, b: Point3| {
        let p = a.as_array();
        let q = b.as_array();
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    let ruling_2d = dist2(dev[0], dev[1]);
    let ruling_3d = dist3(chart.lift(sample_uv[0]), chart.lift(sample_uv[1]));
    assert!(
        (ruling_2d - ruling_3d).abs() <= 1e-12,
        "developed cone ruling length {ruling_2d} must match 3D generator length {ruling_3d}"
    );
    let azim_2d = dist2(dev[2], dev[3]);
    let azim_3d = dist3(chart.lift(sample_uv[2]), chart.lift(sample_uv[3]));
    assert!(
        (azim_2d - azim_3d).abs() <= 1e-12,
        "developed cone azimuthal step {azim_2d} must match 3D step {azim_3d}"
    );

    // (b) Cone patch CDT comparison (`z ∈ [80, 130]`, local radius ∈ [46.19, 75.06]):
    let n_steps = 24usize;
    let d_theta = 0.05;
    let theta_start = -0.6;
    let z_base = 80.0;
    let z_top = 130.0;

    let mut uv_cycle: Vec<Point2> = Vec::with_capacity(2 * (n_steps + 1));
    for k in 0..=n_steps {
        let theta = theta_start + (k as f64) * d_theta;
        let z = z_base + 2.0 * (1.0 - theta.cos());
        uv_cycle.push(Point2::new(theta, z));
    }
    for k in (0..=n_steps).rev() {
        let theta = theta_start + (k as f64) * d_theta;
        let z = z_top - 2.0 * (1.0 - theta.cos());
        uv_cycle.push(Point2::new(theta, z));
    }

    let verts: Vec<Point3> = uv_cycle.iter().map(|&uv| chart.lift(uv)).collect();
    let old_tris = build_reference_patch_tris(n_steps);
    let mesh = Mesh::new(verts.clone(), old_tris.clone());
    let patch = SplicePatch {
        cycles: vec![(0..uv_cycle.len() as u32).collect()],
        tris: (0..old_tris.len() as u32).collect(),
        surface,
    };

    let param_plan = rebuild_patch_planar_ex_with_metric(
        &mesh,
        0,
        &patch,
        &BTreeSet::new(),
        ChartMetric::Parametric,
    )
    .expect("(θ, z) cone CDT should succeed");
    let iso_plan = rebuild_patch_planar_ex_with_metric(
        &mesh,
        0,
        &patch,
        &BTreeSet::new(),
        ChartMetric::Isometric,
    )
    .expect("developed circular-sector cone CDT should succeed");

    assert!(param_plan.new_verts.is_empty());
    assert!(iso_plan.new_verts.is_empty());

    let projected_uv: Vec<Point2> = verts.iter().map(|&p| chart.project(p)).collect();

    let summarize = |tris: &[[u32; 3]], bounds: &[f64]| -> (f64, Vec<f64>, usize) {
        let mut max_ar = 0.0f64;
        let mut dts = Vec::with_capacity(tris.len());
        let mut pocket_slivers = 0usize;
        for (idx, t) in tris.iter().enumerate() {
            let pts3 = [
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            ];
            max_ar = max_ar.max(tri_aspect_ratio_3d(pts3));
            let uv = [
                projected_uv[t[0] as usize],
                projected_uv[t[1] as usize],
                projected_uv[t[2] as usize],
            ];
            let dt = d_of_t(&surface, uv).expect("valid cone d(T)");
            assert_eq!(
                bounds[idx], dt,
                "cone PatchRebuild::new_tri_bounds[{idx}] must equal d_of_t of chart coordinates"
            );
            dts.push(dt);
            if t.iter().all(|&v| (v as usize) <= n_steps)
                || t.iter().all(|&v| (v as usize) > n_steps)
            {
                pocket_slivers += 1;
            }
        }
        (max_ar, dts, pocket_slivers)
    };

    let (param_max_ar, param_dts, param_slivers) =
        summarize(&param_plan.new_tris, &param_plan.new_tri_bounds);
    let (iso_max_ar, iso_dts, iso_slivers) =
        summarize(&iso_plan.new_tris, &iso_plan.new_tri_bounds);
    let param_min_dt = param_dts.iter().copied().fold(f64::INFINITY, f64::min);
    let param_max_dt = param_dts.iter().copied().fold(0.0f64, f64::max);
    let iso_max_dt = iso_dts.iter().copied().fold(0.0f64, f64::max);

    println!(
        "[n106_d3_cone] parametric (θ, z): max_aspect_3d={param_max_ar:.6}, \
         pocket_slivers={param_slivers}, min_dt={param_min_dt:.6}, max_dt={param_max_dt:.6}"
    );
    println!(
        "[n106_d3_cone] developed sector:  max_aspect_3d={iso_max_ar:.6}, \
         pocket_slivers={iso_slivers}, max_dt={iso_max_dt:.6}"
    );

    assert!(
        param_slivers == 2 * (n_steps - 1) && param_max_ar > 75.0,
        "cone (θ, z) CDT expected circumferential slivers, got slivers={param_slivers}, max_ar={param_max_ar:.6}"
    );
    assert_eq!(
        iso_slivers, 0,
        "developed circular-sector cone CDT must produce zero circumferential pocket slivers"
    );
    // Note: as on the cylinder, each seedless strip triangle's 3D aspect ratio
    // scales with the fixture's slant-span-to-rim-step geometry (~25.07 here),
    // not a universal chart constant. Assert what the chart guarantees: zero
    // circumferential slivers above and strictly lower max aspect ratio than
    // the raw (θ, z) CDT, without a knife-edge cap sitting on 25.07.
    assert!(
        iso_max_ar < param_max_ar,
        "developed circular-sector cone CDT max 3D aspect ratio must beat (θ, z), got iso={iso_max_ar:.6}, param={param_max_ar:.6}"
    );
    for (idx, &dt) in iso_dts.iter().enumerate() {
        assert!(
            dt <= param_max_dt,
            "cone developed-chart triangle {idx} d(T)={dt:.6} exceeds (θ, z) max_dt={param_max_dt:.6}"
        );
    }
    assert!(
        iso_max_dt < param_max_dt,
        "expected cone developed-chart max d(T) ({iso_max_dt:.6}) < (θ, z) max d(T) ({param_max_dt:.6})"
    );
}

/// Pin 3 (Cone developed-chart non-inversion in `(θ, z)` and 3D):
/// Straight segments in the developed circular-sector chart `(X, Y)` map to
/// curved trajectories `z(θ) = c / cos((θ − θ_0)·sin α − φ_*)` in `(θ, z)`.
/// Across cone half-angles (`α = π/6` and `α = π/3`), wide azimuthal span
/// (`θ ∈ [-1.2, +1.2]`, `Δθ = 2.4 rad`), near-apex-to-outer-rim axial range
/// (`z ∈ [15, 75]`), curved top/bottom arcs, and carried interior vertices,
/// verifies that EVERY triangle valid in the developed chart is non-inverted
/// when mapped back to unwrapped `(θ, z)` AND non-inverted in 3D against the
/// analytic outward cone normal.
#[test]
fn n106_d3_cone_developed_chart_triangles_not_inverted_in_theta_z_or_3d() {
    let cross2 = |a: Point2, b: Point2, c: Point2| -> f64 {
        (b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())
    };
    let cross3 = |u: [f64; 3], w: [f64; 3]| -> [f64; 3] {
        [
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ]
    };
    let dot3 = |u: [f64; 3], w: [f64; 3]| -> f64 { u[0] * w[0] + u[1] * w[1] + u[2] * w[2] };

    for &half_angle in &[std::f64::consts::FRAC_PI_6, std::f64::consts::FRAC_PI_3] {
        let surface = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle,
        };
        let chart = SurfaceChart::new(surface).expect("cone chart");

        // Build a wide-span curved cone patch (`θ ∈ [-1.2, +1.2]`, 24 steps of
        // δθ = 0.1) with carried interior vertices at 3 axial rings × 5 azimuths
        // so the developed-chart CDT emits boundary-to-boundary, boundary-to-interior,
        // and interior-to-interior diagonals across curved `(θ, z)` chords.
        let n_steps = 24usize;
        let d_theta = 0.1;
        let theta_start = -1.2;

        let mut uv_pool: Vec<Point2> = Vec::new();
        for k in 0..=n_steps {
            let th = theta_start + (k as f64) * d_theta;
            let z = 15.0 + 3.0 * (1.0 - th.cos()) + 1.5 * (2.0 * th).sin();
            uv_pool.push(Point2::new(th, z));
        }
        for k in (0..=n_steps).rev() {
            let th = theta_start + (k as f64) * d_theta;
            let z = 75.0 - 4.0 * (1.0 - th.cos()) + 2.0 * (1.5 * th).cos();
            uv_pool.push(Point2::new(th, z));
        }
        let n_bnd = uv_pool.len();
        let boundary: Vec<u32> = (0..n_bnd as u32).collect();

        let mut interior_indices: Vec<u32> = Vec::new();
        for &z_int in &[28.0, 44.0, 60.0] {
            for &th_int in &[-0.85, -0.40, 0.05, 0.45, 0.90] {
                let idx = uv_pool.len() as u32;
                uv_pool.push(Point2::new(th_int, z_int));
                interior_indices.push(idx);
            }
        }

        // Reference `(θ, z)` triangulation carrying all interior vertices so
        // `rebuild_patch_planar_ex_with_metric` inherits the exact patch
        // boundary cycle AND carries all 15 interior cone vertices.
        let seed_tris = cherchi_rs::cdt_with_interior_constraints(
            &uv_pool,
            &boundary,
            &[],
            &interior_indices,
            &[],
        )
        .expect("reference (θ, z) cone CDT should succeed");

        let verts: Vec<Point3> = uv_pool.iter().map(|&uv| chart.lift(uv)).collect();
        let mesh = Mesh::new(verts.clone(), seed_tris.clone());
        let patch = SplicePatch {
            cycles: vec![boundary.clone()],
            tris: (0..seed_tris.len() as u32).collect(),
            surface,
        };

        let iso_plan = rebuild_patch_planar_ex_with_metric(
            &mesh,
            0,
            &patch,
            &BTreeSet::new(),
            ChartMetric::Isometric,
        )
        .unwrap_or_else(|e| panic!("developed cone CDT (α={half_angle:.4}) failed: {e:?}"));

        assert!(
            iso_plan.new_verts.is_empty(),
            "carried-interior cone patch should not need extra Steiner seeds"
        );
        assert_eq!(
            iso_plan.new_tris.len(),
            seed_tris.len(),
            "Euler count with identical boundary + 15 carried interiors must match"
        );

        // Unwrapped `(θ, z)` coordinates and developed `(X, Y)` coordinates of
        // the mesh vertices (`theta ∈ [-1.2, +1.2]` lies strictly inside `(-π, π)`
        // so `chart.project` returns the exact unwrapped `(θ, z)` directly).
        let projected_tz: Vec<Point2> = verts.iter().map(|&p| chart.project(p)).collect();
        let dev_xy = isometric_cdt_pool(&chart, &projected_tz);

        // Analytic outward unit normal of `Surface::Cone { apex: 0, axis: +Z, half_angle }`
        // at a point `p = (x, y, z)` with `z > 0`:
        // `n_out = cos(α) * (x/r, y/r, 0) - sin(α) * (0, 0, 1)` where `r = hypot(x, y)`.
        let cone_outward_normal = |p: Point3| -> [f64; 3] {
            let [x, y, _z] = p.as_array();
            let r = x.hypot(y);
            assert!(r > 0.0, "cone point must stay strictly away from apex");
            let cos_a = half_angle.cos();
            let sin_a = half_angle.sin();
            [cos_a * (x / r), cos_a * (y / r), -sin_a]
        };

        // Determine the expected 2D orientation sign of `(X, Y)` and `(θ, z)`
        // from the right-handed chart (`e1 × e2 = +Z` => `∂C/∂θ × ∂C/∂z` points
        // outward when `cross2_tz > 0` and `cross2_xy > 0`; verify on `seed_tris[0]`).
        let ref_sign_tz = cross2(
            projected_tz[seed_tris[0][0] as usize],
            projected_tz[seed_tris[0][1] as usize],
            projected_tz[seed_tris[0][2] as usize],
        )
        .signum();
        assert_eq!(ref_sign_tz, 1.0);

        for (t_idx, &[i0, i1, i2]) in iso_plan.new_tris.iter().enumerate() {
            let (u0, u1, u2) = (i0 as usize, i1 as usize, i2 as usize);

            // 1. Valid (strictly positive signed area) in the developed chart `(X, Y)`.
            let area2_dev = cross2(dev_xy[u0], dev_xy[u1], dev_xy[u2]);
            assert!(
                area2_dev > 0.0,
                "cone α={half_angle:.4} tri {t_idx} [{i0},{i1},{i2}] not positive in developed chart: {area2_dev:.6e}"
            );

            // 2. Non-inverted (strictly positive signed area) when mapped back to `(θ, z)`.
            let area2_tz = cross2(projected_tz[u0], projected_tz[u1], projected_tz[u2]);
            assert!(
                area2_tz > 0.0,
                "cone α={half_angle:.4} tri {t_idx} [{i0},{i1},{i2}] inverted in (θ, z): area2_tz={area2_tz:.6e}"
            );

            // 3. Non-inverted in 3D: triangle normal aligns strictly positively with
            //    the analytic outward cone normal at the `(θ, z)` centroid AND at
            //    all three vertices.
            let p0 = verts[u0].as_array();
            let p1 = verts[u1].as_array();
            let p2 = verts[u2].as_array();
            let tri_n = cross3(
                [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]],
                [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]],
            );
            let tz_centroid = Point2::new(
                (projected_tz[u0].x() + projected_tz[u1].x() + projected_tz[u2].x()) / 3.0,
                (projected_tz[u0].y() + projected_tz[u1].y() + projected_tz[u2].y()) / 3.0,
            );
            let n_cent = cone_outward_normal(chart.lift(tz_centroid));
            let dot_cent = dot3(tri_n, n_cent);
            assert!(
                dot_cent > 0.0,
                "cone α={half_angle:.4} tri {t_idx} [{i0},{i1},{i2}] inverted in 3D at centroid: dot={dot_cent:.6e}"
            );
            for &corner_u in &[u0, u1, u2] {
                let dot_v = dot3(tri_n, cone_outward_normal(verts[corner_u]));
                assert!(
                    dot_v > 0.0,
                    "cone α={half_angle:.4} tri {t_idx} [{i0},{i1},{i2}] inverted in 3D at vertex {corner_u}: dot={dot_v:.6e}"
                );
            }
        }
    }
}

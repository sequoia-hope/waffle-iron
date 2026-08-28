//! Grid-aligned arc sampling — the KV9-F2a/R0054 conformality contract
//! (spec `yang_434_output_chord_refinement.md` §3 inc-3, flip blocker
//! R0054): interior samples of an `Arc` half-edge land on the GLOBAL
//! azimuth grid of the circle's axis, not on a per-arc uniform subdivision
//! anchored at the arc's own start vertex.
//!
//! Why: a boolean output can bound one developable face with two coaxial
//! rim arcs a fraction of a chord-sagitta apart (R0054 FaceId(548): a
//! 0.0089-wide cone strip between restored revolve rims, chord sag 0.052).
//! Per-arc anchored sampling gives the two rims incommensurate grids, so
//! the strip's CDT builds needle triangles whose apex sits mid-chord where
//! the chord sags below the surface — the emitted normal tilts past the
//! −0.1 fold margin and the patch fails loudly. The mesh polylines the
//! §4.4.2 restoration replaced were phase-locked for free (Stage-1 revolve
//! tessellation used one shared azimuth grid); the global grid restores
//! that conformality analytically, for every coaxial family at once.

use super::sampling::arc_grid_samples;
use super::{tessellate_cone_patch, RenderMesh};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use cad_primitives::Point3;
use std::f64::consts::PI;

const UP: UnitVector3 = UnitVector3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};
const DOWN: UnitVector3 = UnitVector3 {
    x: 0.0,
    y: 0.0,
    z: -1.0,
};

fn azimuth(p: Point3) -> f64 {
    p.y().atan2(p.x())
}

/// Angular distance on the circle (result in [0, π]).
fn ang_dist(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(2.0 * PI);
    d.min(2.0 * PI - d)
}

fn cone_point(theta: f64, v: f64, tan_a: f64) -> Point3 {
    let r = v * tan_a;
    Point3::new(r * theta.cos(), r * theta.sin(), v)
}

// ---------------------------------------------------------------- pure grid

#[test]
fn coaxial_arcs_sample_on_one_azimuth_grid() {
    // Two coaxial circles at different stations and radii, stored with
    // OPPOSITE normals (the twin/orientation reality of stacked rims),
    // starting at unrelated azimuths. Every pair of samples in the shared
    // azimuth range must align exactly on one grid.
    let n_seg = 72u32;
    let (r_a, r_b) = (54.0, 53.973);
    let start_a = Point3::new(r_a * 0.3f64.cos(), r_a * 0.3f64.sin(), 18.0);
    // B walks CCW around −z: azimuth DECREASES from 2.1.
    let start_b = Point3::new(r_b * 2.1f64.cos(), r_b * 2.1f64.sin(), 17.991);
    let sa = arc_grid_samples(
        Point3::new(0.0, 0.0, 18.0),
        UP,
        r_a,
        start_a,
        2.0,
        n_seg,
        &[],
    );
    let sb = arc_grid_samples(
        Point3::new(0.0, 0.0, 17.991),
        DOWN,
        r_b,
        start_b,
        1.5,
        n_seg,
        &[],
    );
    assert!(
        sa.len() >= 20 && sb.len() >= 15,
        "dense enough to mean much"
    );
    // A covers θ ∈ (0.3, 2.3); B covers θ ∈ (0.6, 2.1). In the overlap
    // every A sample azimuth must have an exactly matching B azimuth.
    let mut matched = 0usize;
    for (_, p) in &sa {
        let th = azimuth(*p);
        if th <= 0.6 + 1e-9 || th >= 2.1 - 1e-9 {
            continue;
        }
        let best = sb
            .iter()
            .map(|(_, q)| ang_dist(th, azimuth(*q)))
            .fold(f64::INFINITY, f64::min);
        assert!(
            best < 1e-12,
            "A sample at θ={th:.9} has no aligned B sample (nearest {best:.3e})"
        );
        matched += 1;
    }
    assert!(matched >= 15, "overlap must contain many aligned pairs");
}

#[test]
fn grid_step_keeps_the_chord_bound() {
    let n_seg = 72u32;
    let r = 54.0;
    let start = Point3::new(r * 0.271f64.cos(), r * 0.271f64.sin(), 18.0);
    let sweep = 2.4369;
    let s = arc_grid_samples(Point3::new(0.0, 0.0, 18.0), UP, r, start, sweep, n_seg, &[]);
    let delta = 2.0 * PI / f64::from(n_seg);
    assert!(!s.is_empty());
    // Walk-ordered fracs, strictly inside (0, 1).
    let mut prev_t = 0.0f64; // start
    for (frac, _) in &s {
        assert!(*frac > 0.0 && *frac < 1.0, "interior only, got {frac}");
        assert!(*frac > prev_t, "walk-ordered");
        prev_t = *frac;
    }
    // Angular gaps (start→first, consecutive, last→end) all ≤ Δ (+ slack
    // for the f32 end-guard's dropped point).
    let mut ts: Vec<f64> = s.iter().map(|(f, _)| f * sweep).collect();
    ts.insert(0, 0.0);
    ts.push(sweep);
    for w in ts.windows(2) {
        assert!(
            w[1] - w[0] <= delta + 1e-4,
            "angular step {:.6} exceeds the chord bound {delta:.6}",
            w[1] - w[0]
        );
    }
    // Interior samples sit ON the global grid: azimuth ≡ 0 (mod Δ).
    for (_, p) in &s {
        let th = azimuth(*p).rem_euclid(2.0 * PI);
        let off = (th / delta - (th / delta).round()).abs() * delta;
        assert!(off < 1e-12, "sample off the global grid by {off:.3e} rad");
    }
}

#[test]
fn normal_sign_gives_the_same_azimuth_set() {
    // The same arc stored with ±ν (twin copies, or two edges minted by
    // different pipeline stages) must land on the same grid azimuths.
    let n_seg = 64u32;
    let r = 7.5;
    let start = Point3::new(r * 1.0f64.cos(), r * 1.0f64.sin(), 3.0);
    let end_az: f64 = 1.0 + 1.9; // CCW-around-+z end
    let end = Point3::new(r * end_az.cos(), r * end_az.sin(), 3.0);
    let c = Point3::new(0.0, 0.0, 3.0);
    let s_up = arc_grid_samples(c, UP, r, start, 1.9, n_seg, &[]);
    // Around −z the SAME circular segment is walked end→start with the
    // same sweep.
    let s_dn = arc_grid_samples(c, DOWN, r, end, 1.9, n_seg, &[]);
    assert_eq!(s_up.len(), s_dn.len());
    for (i, (_, p)) in s_up.iter().enumerate() {
        let (_, q) = &s_dn[s_dn.len() - 1 - i];
        let d = ang_dist(azimuth(*p), azimuth(*q));
        assert!(d < 1e-12, "±ν sample {i} misaligned by {d:.3e} rad");
    }
}

#[test]
fn end_guard_drops_sub_render_slivers() {
    // Start the arc EXACTLY on a grid azimuth: the coincident grid point
    // must be dropped (a sample there would mint a boundary sub-edge below
    // f32 render resolution against the start vertex).
    let n_seg = 64u32;
    let r = 54.0;
    let delta = 2.0 * PI / f64::from(n_seg);
    let th0 = 5.0 * delta;
    let start = Point3::new(r * th0.cos(), r * th0.sin(), 18.0);
    let s = arc_grid_samples(Point3::new(0.0, 0.0, 18.0), UP, r, start, 1.3, n_seg, &[]);
    for (_, p) in &s {
        let d3 = ((p.x() - start.x()).powi(2)
            + (p.y() - start.y()).powi(2)
            + (p.z() - start.z()).powi(2))
        .sqrt();
        assert!(
            d3 > 1e-5,
            "sample {d3:.3e} from the start vertex — below render resolution"
        );
    }
}

// ------------------------------------------------------- face-level (R0054)

/// Wire one loop over existing vertices with per-edge curves.
fn add_loop(
    arena: &mut BrepArena,
    fid: FaceId,
    he_base: usize,
    edges: &[(u32, Curve)],
    kind: LoopKind,
) -> LoopId {
    let lid = LoopId(arena.loops.len() as u32);
    let n = edges.len();
    for (i, (v, curve)) in edges.iter().enumerate() {
        arena.half_edges.push(Some(HalfEdge {
            twin: HalfEdgeId((he_base + i) as u32),
            next: HalfEdgeId((he_base + (i + 1) % n) as u32),
            prev: HalfEdgeId((he_base + (i + n - 1) % n) as u32),
            origin: VertexId(*v),
            loop_id: lid,
            curve: *curve,
        }));
    }
    arena.loops.push(Some(Loop {
        face: fid,
        boundary: LoopBoundary::Edges(HalfEdgeId(he_base as u32)),
        kind,
    }));
    lid
}

/// The R0054 FaceId(548) class, scaled to be decisively inside the fold
/// margin: a thin cone strip between two coaxial rim arcs whose azimuth
/// spans are SPLIT differently, so per-arc anchored sampling (the former
/// scheme) grids the rims incommensurately and a lower-rim sample lands
/// mid-chord of an upper-rim chord that sags far deeper than the strip is
/// wide. Grid-aligned sampling pairs the rims into ladder rungs and the
/// patch tessellates.
#[test]
fn thin_coaxial_rim_strip_tessellates_without_folding() {
    let tan_a: f64 = 3.0;
    let half_angle = tan_a.atan();
    let (v_hi, v_lo) = (18.0, 18.0 - 0.002);
    let (r_hi, r_lo) = (v_hi * tan_a, v_lo * tan_a);
    let theta_end = 2.4;
    let theta_split = 1.27; // lower rim's interior junction vertex

    let mut arena = BrepArena::new();
    let vid = |arena: &mut BrepArena, p: Point3| -> u32 {
        arena.vertices.push(Some(Vertex { point: p }));
        (arena.vertices.len() - 1) as u32
    };
    let lo0 = vid(&mut arena, cone_point(0.0, v_lo, tan_a));
    let lo1 = vid(&mut arena, cone_point(theta_split, v_lo, tan_a));
    let lo2 = vid(&mut arena, cone_point(theta_end, v_lo, tan_a));
    let hi2 = vid(&mut arena, cone_point(theta_end, v_hi, tan_a));
    let hi0 = vid(&mut arena, cone_point(0.0, v_hi, tan_a));

    let arc_lo = Curve::Arc {
        center: Point3::new(0.0, 0.0, v_lo),
        normal: UP,
        radius: r_lo,
    };
    let arc_hi = Curve::Arc {
        center: Point3::new(0.0, 0.0, v_hi),
        normal: DOWN, // walked θ-decreasing (CCW around −z)
        radius: r_hi,
    };
    // In-chart CCW (u = θ·r_unroll, v axial): along the LOW rim with θ
    // increasing, up, back along the HIGH rim, down.
    let fid = FaceId(0);
    let outer = add_loop(
        &mut arena,
        fid,
        0,
        &[
            (lo0, arc_lo),
            (lo1, arc_lo),
            (lo2, Curve::LineSegment),
            (hi2, arc_hi),
            (hi0, Curve::LineSegment),
        ],
        LoopKind::Outer,
    );
    arena.faces.push(Some(Face {
        surface: Some(Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UP,
            half_angle,
            reversed: false,
        }),
        outer_loop: outer,
        inner_loops: vec![],
        shell: ShellId(0),
    }));
    arena.shells.push(Some(Shell {
        solid: SolidId(0),
        faces: vec![fid],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![ShellId(0)],
    }));

    let mut mesh = RenderMesh::default();
    tessellate_cone_patch(&arena, fid, 72, &mut mesh).expect(
        "R0054 class: a thin strip between coaxial rim arcs must tessellate — \
         misaligned per-arc sampling folds it (apex mid-chord under the sag)",
    );
    assert!(mesh.indices.len() >= 3, "non-empty triangulation");
}

// ------------------------------------------------- ellipse sag contract
// inc-8 (spec `yang_434_output_chord_refinement.md`, R0003 FaceId(577)):
// `ellipse_interior_samples` must honor the circle-step sag contract at the
// INCIDENT SURFACES' local scale. The uniform-parameter grid alone bounds
// chord sag only at the ellipse's own scale (≈ R_maj·(1−cos(π/n_seg))); a
// steep plane×cone section has R_maj far above the cone's local radius, so
// its chords cut deeper into the face than any on-surface feature and the
// patch CDT folds against them.

use super::sampling::ellipse_interior_samples;

const X_AXIS: UnitVector3 = UnitVector3 {
    x: 1.0,
    y: 0.0,
    z: 0.0,
};

/// Fixture: one EllipseArc half-edge (plus a LineSegment closer) on a face
/// with the given surface. Returns the arena; the ellipse half-edge is id 0.
fn ellipse_edge_arena(
    surface: Option<Surface>,
    major_radius: f64,
    minor_radius: f64,
    t_start: f64,
    t_end: f64,
) -> (BrepArena, Curve) {
    let center = Point3::new(0.0, 0.0, 20.0);
    let nu = [0.0, 0.0, 1.0];
    let mr = [1.0, 0.0, 0.0];
    let ell = Curve::EllipseArc {
        center,
        normal: UP,
        major_axis: X_AXIS,
        major_radius,
        minor_radius,
    };
    let p0 = crate::geom::ellipse_point_at(center, nu, mr, major_radius, minor_radius, t_start);
    let p1 = crate::geom::ellipse_point_at(center, nu, mr, major_radius, minor_radius, t_end);
    let mut arena = BrepArena::new();
    arena.vertices.push(Some(Vertex { point: p0 }));
    arena.vertices.push(Some(Vertex { point: p1 }));
    let fid = FaceId(0);
    let outer = add_loop(
        &mut arena,
        fid,
        0,
        &[(0, ell), (1, Curve::LineSegment)],
        LoopKind::Outer,
    );
    arena.faces.push(Some(Face {
        surface,
        outer_loop: outer,
        inner_loops: vec![],
        shell: ShellId(0),
    }));
    (arena, ell)
}

/// The measured certificate the sampler enforces: for every adjacent pair
/// of the emitted polyline, the ellipse point at the parametric midpoint
/// deviates from the chord midpoint by at most `tol`.
fn worst_mid_sag(polyline: &[Point3], ell: &Curve) -> f64 {
    let Curve::EllipseArc {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = *ell
    else {
        unreachable!()
    };
    let nu = [normal.x, normal.y, normal.z];
    let mr = [major_axis.x, major_axis.y, major_axis.z];
    let mut worst = 0.0f64;
    for w in polyline.windows(2) {
        let ta = crate::geom::ellipse_param(center, nu, mr, major_radius, minor_radius, w[0])
            .expect("on-ellipse");
        let tb = crate::geom::ellipse_param(center, nu, mr, major_radius, minor_radius, w[1])
            .expect("on-ellipse");
        let pm = crate::geom::ellipse_point_at(
            center,
            nu,
            mr,
            major_radius,
            minor_radius,
            0.5 * (ta + tb),
        );
        let mid = [
            0.5 * (w[0].x() + w[1].x()),
            0.5 * (w[0].y() + w[1].y()),
            0.5 * (w[0].z() + w[1].z()),
        ];
        let sag =
            ((pm.x() - mid[0]).powi(2) + (pm.y() - mid[1]).powi(2) + (pm.z() - mid[2]).powi(2))
                .sqrt();
        worst = worst.max(sag);
    }
    worst
}

#[test]
fn eccentric_ellipse_sampling_is_sag_bound_at_the_surface_scale() {
    // Cone local radius at the endpoints (z = 20, tan α = 0.2): r_local = 4;
    // R_maj = 100 — the R0003 f577 shape (steep plane×cone section).
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: UP,
        half_angle: 0.2f64.atan(),
        reversed: false,
    };
    let (arena, ell) = ellipse_edge_arena(Some(cone), 100.0, 4.0, 0.02, 0.30);
    let n_seg = 71u32;
    let tol = 4.0 * (1.0 - (PI / f64::from(n_seg)).cos());

    // The pre-fix uniform k-grid violates the surface-scale contract: its
    // spans sag at the ellipse's OWN scale (documents the red state).
    let k = (0.28f64 / (2.0 * PI / f64::from(n_seg))).ceil();
    let grid: Vec<Point3> = (0..=k as u32)
        .map(|j| {
            crate::geom::ellipse_point_at(
                Point3::new(0.0, 0.0, 20.0),
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                100.0,
                4.0,
                0.02 + 0.28 * f64::from(j) / k,
            )
        })
        .collect();
    assert!(
        worst_mid_sag(&grid, &ell) > 10.0 * tol,
        "fixture must make the uniform grid violate the surface-scale band \
         decisively (else the test proves nothing)"
    );

    let samples = ellipse_interior_samples(&arena, HalfEdgeId(0), n_seg).expect("samples");
    assert!(
        samples.len() > k as usize - 1,
        "bisection must add samples beyond the uniform grid ({} ≤ {})",
        samples.len(),
        k as usize - 1
    );
    let mut poly = vec![arena.vertex(VertexId(0)).unwrap().point];
    poly.extend(samples);
    poly.push(arena.vertex(VertexId(1)).unwrap().point);
    let worst = worst_mid_sag(&poly, &ell);
    assert!(
        worst <= tol * (1.0 + 1e-9),
        "every sub-chord must meet the surface-scale sag band: worst {worst:.3e} > tol {tol:.3e}"
    );
}

#[test]
fn near_circular_ellipse_keeps_the_uniform_grid() {
    // R_maj = R_min = r on a cylinder of the same radius: the uniform grid
    // already meets the surface-scale band — the samples must be exactly
    // the grid points (no densification of healthy sections).
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: UP,
        radius: 30.0,
        reversed: false,
    };
    let (arena, _) = ellipse_edge_arena(Some(cyl), 30.0, 30.0, 0.02, 0.17);
    let n_seg = 71u32;
    let samples = ellipse_interior_samples(&arena, HalfEdgeId(0), n_seg).expect("samples");
    // sweep 0.15 at grid step 2π/71 → k = 2 → exactly one interior sample,
    // bitwise at the uniform-grid parameter (t0/sweep recovered from the
    // endpoint POSITIONS, exactly as the sampler does).
    assert_eq!(samples.len(), 1);
    for (j, s) in samples.iter().enumerate() {
        let expect = grid_expect(&arena, 30.0, 30.0, 2.0, j);
        assert_eq!(s.as_array(), expect.as_array());
    }
}

#[test]
fn scaleless_faces_fall_back_to_the_ellipse_scale() {
    // No incident surface contributes a local radius (surface: None here;
    // a plane behaves the same): the ellipse's own scale stands and the
    // eccentric arc keeps the pure uniform grid.
    let (arena, _) = ellipse_edge_arena(None, 100.0, 4.0, 0.02, 0.30);
    let n_seg = 71u32;
    let k = (0.28f64 / (2.0 * PI / f64::from(n_seg))).ceil();
    let samples = ellipse_interior_samples(&arena, HalfEdgeId(0), n_seg).expect("samples");
    assert_eq!(samples.len(), k as usize - 1);
    for (j, s) in samples.iter().enumerate() {
        let expect = grid_expect(&arena, 100.0, 4.0, k, j);
        assert_eq!(s.as_array(), expect.as_array());
    }
}

/// The pre-fix uniform-grid point `j` (0-based interior index) of the
/// fixture's ellipse edge, with `t0`/`sweep` recovered from the endpoint
/// POSITIONS exactly the way the sampler recovers them.
fn grid_expect(arena: &BrepArena, a: f64, b: f64, k: f64, j: usize) -> Point3 {
    let center = Point3::new(0.0, 0.0, 20.0);
    let nu = [0.0, 0.0, 1.0];
    let mr = [1.0, 0.0, 0.0];
    let p0 = arena.vertex(VertexId(0)).unwrap().point;
    let p1 = arena.vertex(VertexId(1)).unwrap().point;
    let t0 = crate::geom::ellipse_param(center, nu, mr, a, b, p0).unwrap();
    let sweep = crate::geom::ellipse_ccw_sweep(center, nu, mr, a, b, p0, p1).unwrap();
    crate::geom::ellipse_point_at(center, nu, mr, a, b, t0 + sweep * (j as f64 + 1.0) / k)
}

#[test]
fn ellipse_at_the_apex_scale_fails_loud() {
    // An endpoint at the cone apex has local radius 0 → tol 0: the
    // refinement cannot terminate and must fail with its typed reason
    // (P9 — a boundary conic through the apex is degenerate), matching
    // the surface-pair sampler's apex behavior.
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 20.0), // apex in the ellipse plane
        axis_dir: UP,
        half_angle: 0.2f64.atan(),
        reversed: false,
    };
    let (arena, _) = ellipse_edge_arena(Some(cone), 100.0, 4.0, 0.02, 0.30);
    let err = ellipse_interior_samples(&arena, HalfEdgeId(0), 71).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("depth cap exceeded"),
        "expected the typed depth-cap failure, got: {msg}"
    );
}

//! M5 K11 re-entry (spec `m5_surface_pair_curve.md` "K11 re-entry"): a
//! procedural `Curve::SurfacePair` INPUT edge builds its Stage-1 boundary
//! chain by chord-midpoint bisection with Newton projection onto both
//! surfaces, and the holed-lateral CDT splices it like a conic arc.

use super::*;

/// Cylinder A: axis z, radius 1. Cylinder B: axis x, radius 1/2 — B pierces A
/// in one closed degree-4 curve `{(cos θ, sin θ, ±√(¼ − sin² θ))}`, θ ∈
/// [−30°, 30°], which this fixture splits into FOUR pair edges at the two
/// θ = ±30° turning points and the two θ = 0 poles, so no chord midpoint sits
/// on B's axis.
fn cyl_a() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    }
}
fn cyl_b() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.5,
    }
}

/// The A tube (z ∈ [−1, 1], two full rims) with the B window as an inner loop
/// of four surface-pair edges: `(verts, edges, faces)`.
fn tube_with_pair_window() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let s30 = 0.5_f64;
    let c30 = (1.0 - s30 * s30).sqrt();
    let verts = vec![
        BRepVertex {
            point: Point3::new(1.0, 0.0, -1.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, 1.0),
        },
        BRepVertex {
            point: Point3::new(c30, -s30, 0.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, 0.5),
        },
        BRepVertex {
            point: Point3::new(c30, s30, 0.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, -0.5),
        },
    ];
    let pair = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::SurfacePair {
            a: cyl_a(),
            b: cyl_b(),
        },
    };
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, -1.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 1.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        },
        pair(2, 3),
        pair(3, 4),
        pair(4, 5),
        pair(5, 2),
    ];
    let faces = vec![BRepFace {
        surface: cyl_a(),
        outer_loop: vec![0],
        inner_loops: vec![vec![1], vec![2, 3, 4, 5]],
        reversed: false,
    }];
    (verts, edges, faces)
}

fn dist_to_axis(p: Point3, axis: [f64; 3]) -> f64 {
    let pa = p.as_array();
    let along = pa[0] * axis[0] + pa[1] * axis[1] + pa[2] * axis[2];
    let r = [
        pa[0] - along * axis[0],
        pa[1] - along * axis[1],
        pa[2] - along * axis[2],
    ];
    (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
}

/// The window's area ON the cylinder A (`r = 1`, so `dA = dθ dz`):
/// `∫_{−π/6}^{π/6} 2·√(¼ − sin² θ) dθ` by composite Simpson.
fn window_area_on_a() -> f64 {
    let n = 4000usize;
    let (t0, t1) = (-std::f64::consts::FRAC_PI_6, std::f64::consts::FRAC_PI_6);
    let h = (t1 - t0) / n as f64;
    let f = |t: f64| 2.0 * (0.25 - t.sin().powi(2)).max(0.0).sqrt();
    let mut s = f(t0) + f(t1);
    for k in 1..n {
        let w = if k % 2 == 1 { 4.0 } else { 2.0 };
        s += w * f(t0 + h * k as f64);
    }
    s * h / 3.0
}

/// The tube with a surface-pair window re-enters Stage 1: every vertex on A,
/// every pair-chain vertex ALSO on B, the chains actually sampled (Steiner
/// vertices exist), the window boundary is the count-1 edge set beside the
/// two rims, and the wall area is `2π·2 − window` within the chord budget.
#[test]
fn cylinder_tube_with_surface_pair_window_reenters_stage1() {
    let (verts, edges, faces) = tube_with_pair_window();
    let t = stage1_tessellate(&verts, &edges, &faces).expect("pair-windowed tube");
    assert!(!t.tris.is_empty(), "must produce triangles");

    // Oracle 1: on A everywhere; pair-chain samples on B too.
    let mut steiner_on_pair = 0usize;
    for (i, v) in t.verts.iter().enumerate() {
        let ra = dist_to_axis(*v, [0.0, 0.0, 1.0]);
        assert!(
            (ra - 1.0).abs() < 1e-9,
            "vertex {i} off cylinder A: radial {ra}"
        );
        if let TessellationSource::BRepEdge { edge, t: tt } = t.sources[i] {
            if (2..=5).contains(&edge) {
                steiner_on_pair += 1;
                assert!(
                    tt > 0.0 && tt < 1.0,
                    "ordinal chain parameter {tt} outside (0, 1)"
                );
                let rb = dist_to_axis(*v, [1.0, 0.0, 0.0]);
                assert!(
                    (rb - 0.5).abs() < 1e-9,
                    "pair-chain vertex {i} off cylinder B: radial {rb}"
                );
            }
        }
    }
    assert!(
        steiner_on_pair >= 4,
        "the pair chains must carry Newton-certified Steiner samples (found {steiner_on_pair})"
    );

    // Oracle 2: count-1 edges are exactly the two rims + the window; every
    // window boundary vertex lies on B; no edge is used more than twice.
    let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    let mut window_boundary = 0usize;
    for (&(x, y), &c) in &undirected {
        assert!(c <= 2, "edge ({x},{y}) used {c} times");
        if c == 1 {
            let (px, py) = (t.verts[x as usize], t.verts[y as usize]);
            let on_rim = |p: Point3| (p.as_array()[2].abs() - 1.0).abs() < 1e-9;
            let on_b = |p: Point3| (dist_to_axis(p, [1.0, 0.0, 0.0]) - 0.5).abs() < 1e-9;
            if on_rim(px) && on_rim(py) {
                continue;
            }
            assert!(
                on_b(px) && on_b(py),
                "boundary edge ({x},{y}) is neither a rim chord nor on the window curve"
            );
            window_boundary += 1;
        }
    }
    assert!(
        window_boundary >= 4,
        "the window must be a boundary loop (found {window_boundary} boundary chords)"
    );

    // Oracle 3: area = 4π − window, within the 1e-2 chord budget (the tube's
    // inscribed chords lose area; the window's inscribed polygon gains it).
    let analytic = 4.0 * std::f64::consts::PI - window_area_on_a();
    let area: f64 = t
        .tris
        .iter()
        .map(|tri| {
            let p0 = t.verts[tri[0] as usize].as_array();
            let p1 = t.verts[tri[1] as usize].as_array();
            let p2 = t.verts[tri[2] as usize].as_array();
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        })
        .sum();
    assert!(
        (area - analytic).abs() <= 0.03 * analytic,
        "wall area {area} vs analytic {analytic}"
    );
}

/// The chain bound is `chord_rel()` × the pair's smallest local radius; a
/// cone apex endpoint or a non-pair operand yields no bound; the B-Rep-level
/// bound is the largest chain bound and `None` for a pair-free B-Rep.
#[test]
fn surface_pair_chain_bound_takes_the_smallest_local_radius() {
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    // Cylinder radius 1 vs the cone's local radius tan(45°)·h at h = 0.3 / 2.
    let p0 = Point3::new(0.3, 0.0, 0.3);
    let p1 = Point3::new(0.0, 2.0, 2.0);
    let b = surface_pair_chain_bound(cyl_a(), cone, p0, p1).expect("bound");
    assert!((b - chord_rel() * 0.3).abs() < 1e-15, "bound {b}");
    // An endpoint AT the apex: local radius 0 → no bound.
    assert!(surface_pair_chain_bound(cyl_a(), cone, Point3::new(0.0, 0.0, 0.0), p1).is_none());
    // A plane operand is not a pair surface.
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    assert!(surface_pair_chain_bound(cyl_a(), plane, p0, p1).is_none());

    let (verts, edges, faces) = tube_with_pair_window();
    let brep = BRep::new(verts, edges, faces).expect("brep");
    let bb = surface_pair_chord_bound(&brep).expect("pair bound");
    assert!((bb - chord_rel() * 0.5).abs() < 1e-15, "brep bound {bb}");
    let (verts, mut edges, mut faces) = tube_with_pair_window();
    edges.truncate(2);
    faces[0].inner_loops.truncate(1);
    let plain = BRep::new(verts, edges, faces).expect("plain tube");
    assert_eq!(surface_pair_chord_bound(&plain), None);
}

/// A closed single-edge surface-pair loop has no producer — loud.
#[test]
fn closed_surface_pair_edge_is_loud() {
    let (verts, mut edges, faces) = tube_with_pair_window();
    edges[2].end = 2;
    let err = stage1_tessellate(&verts, &edges, &faces)
        .err()
        .expect("closed pair edge");
    assert!(
        format!("{err:?}").contains("closed single-edge surface-pair loop"),
        "{err:?}"
    );
}

/// An endpoint off one operand surface is loud at the import band.
#[test]
fn off_surface_pair_endpoint_is_loud() {
    let (mut verts, edges, faces) = tube_with_pair_window();
    // Still on A (radial 1) but off B: slide vertex 3 up the ruling.
    verts[3].point = Point3::new(1.0, 0.0, 0.6);
    let err = stage1_tessellate(&verts, &edges, &faces)
        .err()
        .expect("off-B endpoint");
    assert!(format!("{err:?}").contains("is not on"), "{err:?}");
}

/// A chord whose midpoint sits on an operand's axis cannot be projected
/// (the residual gradient is undefined there) — loud, never a chord fallback.
#[test]
fn pair_chain_projection_failure_is_loud() {
    let (verts, mut edges, mut faces) = tube_with_pair_window();
    // One edge straight across the window from θ = −30° to θ = +30° at z = 0:
    // its chord midpoint (cos 30°, 0, 0) lies ON cylinder B's axis.
    edges[2].end = 4;
    edges.truncate(3);
    faces[0].inner_loops[1] = vec![2];
    let err = stage1_tessellate(&verts, &edges, &faces)
        .err()
        .expect("axis midpoint");
    let msg = format!("{err:?}");
    assert!(msg.contains("did not converge"), "{msg}");
}

// ── M5 K11 inc-2: the `pair curve ∩ plane` junction (ruling × section circle, coplanar) ──

/// The inc-2 configuration in its own frame: cutting plane x = 0.27 through
/// the unequal perpendicular union (c1 axis z, r 0.3; c2 axis x, r 0.18). The
/// plane's section of c1 is the ruling y = −√(0.09 − 0.27²), its section of
/// c2 is the circle y² + z² = 0.18² centred on (0.27, 0, 0) — both in the
/// plane. The arrangement vertex sits on the pair CHORD, off both curves by
/// the chord sag; the junction is the in-plane crossing, exact on both
/// cylinders and the plane.
fn inc2_frame() -> (Point3, Vector3, Point3, Vector3, f64) {
    let y = -(0.09_f64 - 0.27 * 0.27).sqrt();
    (
        Point3::new(0.27, y, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point3::new(0.27, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        0.18,
    )
}

#[test]
fn coplanar_ruling_circle_junction_lands_on_all_three_surfaces() {
    let (lp, ld, c, n, r) = inc2_frame();
    // The measured v28 of the kernel-v2 pin: on the plane, off the ruling by
    // ~1e-3 and off the circle by ~1e-3 (the pair chord's sag).
    let current = Point3::new(0.27, -0.12979374035881622, -0.12295904152943612);
    let (j, gate) =
        ruling_circle_coplanar_junction((lp, ld), (c, n, r), current, 4.347e-2, 1.536e-2)
            .expect("in-plane ruling × circle crossing is a junction");
    let ja = j.as_array();
    let y = -(0.09_f64 - 0.27 * 0.27).sqrt();
    let z = -(0.18_f64 * 0.18 - y * y).sqrt();
    assert!((ja[0] - 0.27).abs() < 1e-15, "on the cutting plane: {ja:?}");
    assert!((ja[1] - y).abs() < 1e-15, "on the ruling (⇒ on c1): {ja:?}");
    assert!((ja[2] - z).abs() < 1e-15, "on the circle (⇒ on c2): {ja:?}");
    // Exact on both cylinders as surfaces, not only on their sections.
    assert!((ja[0] * ja[0] + ja[1] * ja[1] - 0.09).abs() < 1e-15);
    assert!((ja[1] * ja[1] + ja[2] * ja[2] - 0.0324).abs() < 1e-15);
    // The derived gate `(band + d_ε)/sin θ` at this crossing angle: the
    // ruling (ẑ) against the circle tangent at j, `x̂ × (0, y, z)/r =
    // (0, −z, y)/r`, so sin θ = |ẑ × tangent| = |z|/r = 0.6872.
    let sin_theta = z.abs() / r;
    assert!(
        ((4.347e-2 + 1.536e-2) / sin_theta - gate).abs() < 1e-12,
        "gate {gate}"
    );
    // The measured vertex is well inside it.
    let ca = current.as_array();
    let rho = ((ja[0] - ca[0]).powi(2) + (ja[1] - ca[1]).powi(2) + (ja[2] - ca[2]).powi(2)).sqrt();
    assert!(rho < gate && rho < 2e-3, "rho {rho} gate {gate}");
}

#[test]
fn coplanar_junction_picks_the_root_nearest_the_vertex() {
    let (lp, ld, c, n, r) = inc2_frame();
    let y = -(0.09_f64 - 0.27 * 0.27).sqrt();
    let z = (0.18_f64 * 0.18 - y * y).sqrt();
    // The ruling crosses the circle twice (z = ±0.1237); a vertex near the
    // +z crossing must land there, never on the −z root.
    let current = Point3::new(0.27, y + 1e-3, z - 1e-3);
    let (j, _) = ruling_circle_coplanar_junction((lp, ld), (c, n, r), current, 1e-2, 1e-2)
        .expect("junction");
    assert!((j.as_array()[2] - z).abs() < 1e-15, "{:?}", j.as_array());
}

#[test]
fn a_ruling_parallel_but_offset_from_the_circle_plane_declines() {
    let (_, ld, c, n, r) = inc2_frame();
    // A ruling of c1 in the plane x = 0.2705 — parallel to the circle's plane
    // (x = 0.27) and offset by 5e-4, well inside any chord band: the chains
    // could cross in the MESH, but the curves do not meet. The certificate is
    // the ulp-order plane band, so this is a decline, never an acceptance
    // that would land the vertex off the circle by the offset.
    let y = -(0.09_f64 - 0.2705 * 0.2705).sqrt();
    let lp = Point3::new(0.2705, y, -1.0);
    let current = Point3::new(0.2702, y, -0.1237);
    let got = ruling_circle_coplanar_junction((lp, ld), (c, n, r), current, 1e-2, 1e-2);
    match got {
        Err(CoplanarJunctionDecline::Miss { plane_band }) => {
            assert!(
                plane_band < 1e-11,
                "ulp-order certificate, got {plane_band:.3e}"
            );
        }
        other => panic!("expected a Miss decline, got {other:?}"),
    }
}

#[test]
fn a_ruling_tangent_to_the_circle_declines_as_grazing() {
    let (_, ld, c, n, r) = inc2_frame();
    // A ruling at y = −r touches the circle at z = 0: a grazing contact.
    let lp = Point3::new(0.27, -r, -1.0);
    let current = Point3::new(0.27, -r + 1e-6, 1e-4);
    let got = ruling_circle_coplanar_junction((lp, ld), (c, n, r), current, 1e-2, 1e-2);
    assert!(
        matches!(got, Err(CoplanarJunctionDecline::Tangent { .. })),
        "expected a Tangent decline, got {got:?}"
    );
}

#[test]
fn a_ruling_in_the_plane_that_misses_the_circle_declines() {
    let (_, ld, c, n, r) = inc2_frame();
    let lp = Point3::new(0.27, -0.25, -1.0);
    let current = Point3::new(0.27, -0.25, 0.0);
    let got = ruling_circle_coplanar_junction((lp, ld), (c, n, r), current, 1e-2, 1e-2);
    assert!(
        matches!(got, Err(CoplanarJunctionDecline::Miss { .. })),
        "expected a Miss decline, got {got:?}"
    );
}

#[allow(unused_imports)]
use super::*;

// ── M5 surface-pair plumbing (Y1–Y3) ─────────────────────────────────

pub(crate) fn qcyl(ap: [f64; 3], ad: [f64; 3], r: f64) -> ssi_rs::QuadricSurface {
    ssi_rs::QuadricSurface::Cylinder {
        axis_point: Point3::new(ap[0], ap[1], ap[2]),
        axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
        radius: r,
    }
}

/// Y1: `SsiCurve::SurfacePair` maps to `Curve::SurfacePair` carrying both
/// operands field-for-field as yang `Surface::Cylinder`s.
#[test]
pub(crate) fn m5_ssi_surface_pair_maps_to_curve_surface_pair() {
    let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = qcyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.5);
    let curve = ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
        .expect("cyl×cyl surface pair maps");
    match curve {
        Curve::SurfacePair {
            a: Surface::Cylinder { radius: ra, .. },
            b: Surface::Cylinder { radius: rb, .. },
        } => {
            assert_eq!(ra, 1.0);
            assert_eq!(rb, 0.5);
        }
        other => panic!("expected Curve::SurfacePair of two cylinders, got {other:?}"),
    }
}

/// Y1: a non-cylinder operand (no producer yet) rejects loudly.
#[test]
pub(crate) fn m5_surface_pair_non_cylinder_operand_rejected() {
    let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let plane = ssi_rs::QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    assert!(ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: plane }).is_err());
}

/// Y2: on-both-surfaces membership — a point exactly on the perpendicular
/// unequal-R curve passes; a point off either cylinder by ≫ tol fails.
#[test]
pub(crate) fn m5_surface_pair_membership() {
    // x²+y²=1 ∧ x²+z²=¼ : point (0, 1, ½) lies on both.
    let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
    let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
    assert!(curve_contains_point(
        &sp,
        Point3::new(0.0, 1.0, 0.5),
        1e-9,
        None
    ));
    // Off cylinder b radially by 0.1 ≫ tol.
    assert!(!curve_contains_point(
        &sp,
        Point3::new(0.0, 1.0, 0.6),
        1e-9,
        None
    ));
}

/// Y3: the surface-pair tangent at a point is `n̂_a × n̂_b`. At (0, 1, ½)
/// the cylinder-a radial normal is +ŷ and cylinder-b radial normal is +ẑ,
/// so the tangent is ±x̂.
#[test]
pub(crate) fn m5_surface_pair_tangent_is_normal_cross() {
    let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
    let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
    let t = curve_tangent_at(&sp, Point3::new(0.0, 1.0, 0.5)).expect("transversal ⇒ tangent");
    assert!(t[0].abs() > 0.999, "tangent should be ±x̂, got {t:?}");
    assert!(t[1].abs() < 1e-9 && t[2].abs() < 1e-9);
}

/// Y3/Y4 failure mode: tangent (parallel normals) → no tangent (None), so
/// the candidate stays non-tie-breakable and the loud stop stands.
#[test]
pub(crate) fn m5_surface_pair_tangent_none_at_tangency() {
    // Externally tangent unit cylinders touch along x=1,y=0: both normals
    // are ±x̂ on the contact line ⇒ parallel ⇒ no finite tangent.
    let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = qcyl([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
    assert!(curve_tangent_at(&sp, Point3::new(1.0, 0.0, 0.0)).is_none());
}

// ── M5 cone-pair producer (Y1–Y3 with Cone operands) ─────────────────

pub(crate) fn qcone(apex: [f64; 3], ad: [f64; 3], alpha: f64) -> ssi_rs::QuadricSurface {
    ssi_rs::QuadricSurface::Cone {
        apex: Point3::new(apex[0], apex[1], apex[2]),
        axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
        half_angle: alpha,
    }
}

/// Y1: a cone-pair `SsiCurve::SurfacePair` maps to `Curve::SurfacePair`
/// carrying both `Surface::Cone` operands field-for-field (cone-pair
/// producer). A cyl×cone mixed pair maps too.
#[test]
pub(crate) fn m5_cone_pair_maps_to_curve_surface_pair() {
    let a = qcone(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        std::f64::consts::FRAC_PI_4,
    );
    let b = qcone([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0_f64.atan());
    match ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
        .expect("cone×cone surface pair maps")
    {
        Curve::SurfacePair {
            a: Surface::Cone { half_angle: ha, .. },
            b: Surface::Cone { half_angle: hb, .. },
        } => {
            assert_eq!(ha, std::f64::consts::FRAC_PI_4);
            assert_eq!(hb, 3.0_f64.atan());
        }
        other => panic!("expected Curve::SurfacePair of two cones, got {other:?}"),
    }
    // Mixed cyl×cone also maps (both operand kinds supported).
    let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let cone = qcone(
        [0.0, 0.0, 5.0],
        [0.0, 0.0, 1.0],
        std::f64::consts::FRAC_PI_4,
    );
    assert!(matches!(
        ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: cone }),
        Ok(Curve::SurfacePair {
            a: Surface::Cylinder { .. },
            b: Surface::Cone { .. }
        })
    ));
}

/// Y2: on-both-surfaces membership for a cone∩cylinder curve. The z-axis
/// cone `radial = |h|·tan(π/4) = |h|` meets the z-axis cylinder `radial = 1`
/// on the circle `radial = 1, h = ±1`; the point (1, 0, 1) lies on both.
#[test]
pub(crate) fn m5_cone_pair_membership() {
    let cone = qcone(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        std::f64::consts::FRAC_PI_4,
    );
    let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
    assert!(curve_contains_point(
        &sp,
        Point3::new(1.0, 0.0, 1.0),
        1e-9,
        None
    ));
    // Off the cone (h=1 needs radial=1, but radial here is 1.2) by ≫ tol.
    assert!(!curve_contains_point(
        &sp,
        Point3::new(1.2, 0.0, 1.0),
        1e-9,
        None
    ));
}

/// Y3: the cone-pair tangent at a transversal point is `n̂_a × n̂_b`. At
/// (1, 0, 1) the π/4 cone normal is `(x̂ − ẑ)/√2` and the cylinder radial
/// normal is `x̂`; their cross is ∓ŷ.
#[test]
pub(crate) fn m5_cone_pair_tangent_is_normal_cross() {
    let cone = qcone(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        std::f64::consts::FRAC_PI_4,
    );
    let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
    let t = curve_tangent_at(&sp, Point3::new(1.0, 0.0, 1.0)).expect("transversal ⇒ tangent");
    assert!(t[1].abs() > 0.999, "tangent should be ±ŷ, got {t:?}");
    assert!(t[0].abs() < 1e-9 && t[2].abs() < 1e-9);
}

/// Y4: a perturbed near-curve point relocates onto both surfaces of a
/// cone∩cylinder pair (the generic Newton engine handles Cone operands).
#[test]
pub(crate) fn m5_cone_pair_relocation_onto_both() {
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    // Perturb the true curve point (1,0,1) off both surfaces.
    let p = relocate_onto_implicit_pair(Point3::new(1.02, 0.03, 0.98), cone, cyl)
        .expect("near-curve point relocates");
    assert!(signed_distance_to_surface(cone, p).unwrap().abs() < 1e-9);
    assert!(signed_distance_to_surface(cyl, p).unwrap().abs() < 1e-9);
}

/// The pair Newton must CONVERGE on a STEEP cone (half-angle > 60°).
///
/// `relocate_onto_implicit_pair` paired `surface_value_and_normal`'s cone
/// residual — the radial form `l − |h|·tanα` = distance × sec α — with the
/// UNIT cone normal, so each Newton step was sec α times too long: the
/// error multiplies by `(1 − sec α)` per step, which is < 1 in magnitude
/// only below 60°. `m5_cone_pair_relocation_onto_both` (45°, sec α = 1.41)
/// converged; the corpus cones did not: R0032 (torus × cone α = 1.19 rad,
/// measured ratio −1.7 = 1 − sec 68°), R0044 (cyl × cone, −7.5) and R0053
/// (cyl × cone, −2.6) all diverged geometrically to the `MAX_ITERS` STOP
/// (`YANG_PAIR_NEWTON_TRACE`, 2026-08-19). The triple solver had carried the
/// `cos α` rescale since KV16; the pair sibling had not — the SAME
/// prose-shared-rule failure as the `8·ε·L` floor below. Both solvers now
/// go through `surface_distance_and_normal`.
#[test]
pub(crate) fn m5_steep_cone_pair_relocation_converges() {
    for half_angle in [1.19_f64, 1.31, 1.45] {
        // Cylinder r=1 about z; cone about z from the origin. True curve:
        // the circle r=1 at height h = 1/tanα.
        let cone = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle,
        };
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let h = 1.0 / half_angle.tan();
        for (s0, s1) in [(cone, cyl), (cyl, cone)] {
            let p = relocate_onto_implicit_pair(Point3::new(1.02, 0.03, h - 0.02), s0, s1)
                .unwrap_or_else(|| {
                    panic!("steep cone α={half_angle}: near-curve point must relocate")
                });
            assert!(signed_distance_to_surface(cone, p).unwrap().abs() < 1e-9);
            assert!(signed_distance_to_surface(cyl, p).unwrap().abs() < 1e-9);
        }
    }
}

/// The pair Newton's convergence floor must be REACHABLE at the coordinate
/// magnitude it is asked to work at.
///
/// `relocate_onto_implicit_pair` used a bare absolute `tau = 1e-13`. Every
/// `surface_value_and_normal` residual is a LENGTH, so at coordinate magnitude
/// `L` no residual can be evaluated below ~`8·ε·L`; at R0044's scale
/// (L ≈ 6.2e3) that floor is ~1.1e-11, more than 100× ABOVE the demanded
/// 1e-13. A fully converged root therefore ran out of iterations and returned
/// `None` — a loud STOP for a point that was already exactly on both surfaces.
/// `relocate_onto_implicit_triple` had carried the `8·ε·L` amendment since
/// increment 5; the pair sibling had not, so the two shared a metric and
/// disagreed about it.
///
/// The numbers are R0025's LIVE PROBE VALUES — a torus x plane pair and the
/// seed recorded at one of its `relocate_onto_implicit_pair` calls. Real
/// coordinates are REQUIRED here, and two earlier attempts at this test were
/// wrong for instructive reasons:
///
///  - a synthetic large-coordinate fixture with tidy axis-aligned values
///    (6000, 100, 42) cancels exactly, both residuals reach 0.0, and the test
///    passes even on the broken build; and
///  - R0044's probed vertex is a THREE-surface junction, so its cylinder x cone
///    pair has no root near that seed and returns `None` in both builds.
///
/// This witness was captured from the `MAX_ITERS` exit itself on a build pinned
/// back to the old constant: it stalls at `f0 = 1.14e-13`, `f1 = -5.68e-14` —
/// f1 already converged, f0 parked just ABOVE the old absolute 1e-13 and far
/// BELOW this seed's `8*eps*L` floor of ~2.4e-12. Newton cannot close that last
/// factor of 1.1 because it is below one ULP at |x| ~ 1.3e3, so the old build
/// burned all 32 iterations on an already-converged root and returned `None`.
///
/// Note it is specifically a STALLED seed, not merely a large-coordinate one:
/// an earlier attempt used a seed that converges by iteration 3 on BOTH builds
/// and so passed without the fix. A witness has to come from the failing exit.
#[test]
pub(crate) fn pair_newton_converges_at_large_coordinate_magnitude() {
    let torus = Surface::Torus {
        center: Point3::new(
            -1168.0344115266691,
            -337.362_669_297_692_83,
            -504.810_811_776_372,
        ),
        axis_dir: Vector3::new(0.0, 0.347_027_534_189_695_1, -0.937_854_941_083_225_3),
        major_radius: 494.229_467_044_109_13,
        minor_radius: 329.486_311_362_739_46,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(
            -0.500_859_978_671_093_7,
            -0.373_465_296_917_343_9,
            0.780_809_166_034_846_1,
        ),
        d: -463.872_489_046_618_45,
    };
    let seed = Point3::new(
        -1339.5010951476447,
        -357.541_286_003_184_7,
        -436.161_969_211_728_3,
    );

    let p = relocate_onto_implicit_pair(seed, torus, plane)
        .expect("a converged root at |x| ~ 1.3e3 must be ACCEPTED — 1e-13 is below one ULP there");

    // Assert against what is representable at this magnitude, rather than
    // restating the old impossible demand.
    let l = seed.x().abs().max(seed.y().abs()).max(seed.z().abs());
    let floor = 8.0 * f64::EPSILON * l;
    assert!(
        signed_distance_to_surface(torus, p).unwrap().abs() <= floor,
        "torus residual must reach the {floor:.3e} evaluation floor"
    );
    assert!(
        signed_distance_to_surface(plane, p).unwrap().abs() <= floor,
        "plane residual must reach the {floor:.3e} evaluation floor"
    );

    // (The floor is SEED-scaled, never iterate-scaled, so a diverging iterate
    // cannot inflate its own acceptance threshold. That property is asserted
    // where it is observable — on the corpus, which stays 0-WRONG — not here.)

    // Unit-scale behaviour is unchanged: 8*eps*L there is ~5e-15, well under
    // the 1e-13 floor, so the constant still governs and the shipped path is
    // byte-identical.
    let unit_cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let unit_plane = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    let q = relocate_onto_implicit_pair(Point3::new(1.02, 0.03, 0.5), unit_cyl, unit_plane)
        .expect("unit-scale relocation is unaffected");
    assert!(signed_distance_to_surface(unit_cyl, q).unwrap().abs() <= 1e-13);
    assert!(signed_distance_to_surface(unit_plane, q).unwrap().abs() <= 1e-13);
}

// ── Case-IV phantom guard (spec `yang_case_iv_phantom_guard`) ────────

/// Minimal solid cylinder B-Rep (two rims + seam) for the guard tests.
pub(crate) fn guard_cyl(cx: f64, cy: f64, r: f64, h: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: Point3::new(cx + r, cy, 0.0),
        },
        BRepVertex {
            point: Point3::new(cx + r, cy, h),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(cx, cy, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(cx, cy, h),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
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
                axis_point: Point3::new(cx, cy, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -h,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("guard cylinder")
}

/// The measured F0088 pair: a nested-disjoint tool inside the plate
/// cylinder with gap 0.0115 < the natural N=14 sagitta — the guard must
/// demand a finer N (34 at these radii: the smallest N with
/// sag(R,N)+sag(r,N) ≤ gap/2).
#[test]
pub(crate) fn phantom_guard_nested_disjoint_demands_finer_n() {
    let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
    let tool = guard_cyl(1.2243, 0.0, 0.042871795720997065, 0.23);
    let n = phantom_min_rim_segments(&plate, &tool).expect("guard must fire");
    let gap = 1.2787008340600021 - 1.2243 - 0.042871795720997065;
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    assert!(
        sag(1.2787008340600021, n) + sag(0.042871795720997065, n) <= gap / 2.0,
        "derived N={n} must clear the analytic gap with the factor-2 margin"
    );
    assert!(
        sag(1.2787008340600021, n - 1) + sag(0.042871795720997065, n - 1) > gap / 2.0,
        "derived N={n} must be MINIMAL (no over-refinement)"
    );
}

/// A crossing pair (the tool overlaps the plate wall) has no analytic
/// gap — a real intersection curve exists and SSI refines it. No boost.
#[test]
pub(crate) fn phantom_guard_crossing_pair_is_silent() {
    let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
    let tool = guard_cyl(1.26, 0.0, 0.042871795720997065, 0.23);
    assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
}

/// A far-disjoint pair derives a tiny N that both solids' natural
/// Stage-1 N already satisfies — the self-limiting gate drops it.
#[test]
pub(crate) fn phantom_guard_far_pair_is_silent() {
    let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
    let tool = guard_cyl(0.3, 0.1, 0.042871795720997065, 0.23);
    assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
}

/// Build one B-Rep carrying TWO cylinders (a plate wall + a hole at
/// `(hx, hy)` with radius `hr`).
pub(crate) fn two_cyl_brep(hx: f64, hy: f64, hr: f64) -> BRep {
    let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
    let tool = guard_cyl(hx, hy, hr, 0.23);
    let mut verts = plate.vertices.clone();
    let mut edges = plate.edges.clone();
    let mut faces = plate.faces.clone();
    let (vo, eo) = (verts.len() as u32, edges.len() as u32);
    verts.extend(tool.vertices.iter().cloned());
    for e in &tool.edges {
        edges.push(BRepEdge {
            start: e.start + vo,
            end: e.end + vo,
            curve: e.curve,
        });
    }
    for f in &tool.faces {
        faces.push(BRepFace {
            surface: f.surface,
            outer_loop: f.outer_loop.iter().map(|&e| e + eo).collect(),
            inner_loops: Vec::new(),
            reversed: f.reversed,
        });
    }
    BRep::new(verts, edges, faces).expect("combined solid")
}

/// INTRA-solid pair (the chained F0088 output: hole 4's lateral 0.0115
/// from the plate wall inside ONE solid): STAGE 1's own N selection must
/// fold the pair's derived N in — otherwise ANY tessellation of the
/// solid (input conversion included) puts the cap's outer-rim chords
/// across the hole rim and the planar CDT gets crossing constraints
/// (measured corpus F0088 ops 7/15, `CDT triangulation failed`). The
/// near-rim solid must tessellate strictly denser than the same solid
/// with its hole far from the wall.
#[test]
pub(crate) fn stage1_intra_solid_phantom_fold_densifies_rims() {
    let near = two_cyl_brep(1.2243, 0.0, 0.042871795720997065);
    let far = two_cyl_brep(0.3, 0.1, 0.042871795720997065);
    assert!(
        near.as_mesh().num_verts() > far.as_mesh().num_verts(),
        "near-rim solid must tessellate denser (near {} verts vs far {})",
        near.as_mesh().num_verts(),
        far.as_mesh().num_verts()
    );
    // And the cross-pair guard is silent for it — the intra fold lives
    // in Stage 1, not in the pair analysis.
    let partner = guard_cyl(10.0, 10.0, 0.1, 0.23);
    assert_eq!(phantom_min_rim_segments(&near, &partner), None);
}

/// An operand without B-Rep faces (the `from_mesh` chained-output
/// degenerate) has no cylinder faces to scan — byte-identical path.
#[test]
pub(crate) fn phantom_guard_faceless_operand_is_silent() {
    let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
    let raw = BRep::from_mesh(plate.as_mesh().clone());
    assert_eq!(phantom_min_rim_segments(&plate, &raw), None);
    assert_eq!(phantom_min_rim_segments(&raw, &plate), None);
}

// R0072: position tie-break for near-coincident PARALLEL line candidates
// (`select_disjoint_parallel_line`). Mirrors the instrumented R0072 edge
// (2,143): two parallel generators whose endpoint-distance intervals are
// disjoint → the lower (nearer) one is selected. The numbers are the live
// probe values (cand0 ≈ 2.0e-5, cand1 ≈ 3.3e-5).
#[test]
pub(crate) fn r0072_parallel_line_position_tiebreak() {
    let dir = Vector3::new(
        0.539_214_627_766_961_7,
        -0.348_918_218_865_836_5,
        -0.766_487_874_493_543,
    );
    // Two parallel lines offset along a perpendicular `n̂` (⟂ dir), 2e-5 and
    // 3.3e-5 from the edge endpoints which sit on the origin segment.
    let n = {
        // any unit vector ⟂ dir
        let d = normalize3(dir.as_array());
        let t = [1.0, 0.0, 0.0];
        let dot = t[0] * d[0] + t[1] * d[1] + t[2] * d[2];
        let p = [t[0] - dot * d[0], t[1] - dot * d[1], t[2] - dot * d[2]];
        normalize3(p)
    };
    let line_at = |off: f64| (Point3::new(off * n[0], off * n[1], off * n[2]), dir);
    let cand0 = line_at(2.0e-5);
    let cand1 = line_at(3.3e-5);
    let p_s = Point3::new(0.0, 0.0, 0.0);
    let p_e = Point3::new(
        d_scale(dir, 1e-4)[0],
        d_scale(dir, 1e-4)[1],
        d_scale(dir, 1e-4)[2],
    );

    // Disjoint intervals → the nearer line (index 0) wins regardless of order.
    assert_eq!(
        select_disjoint_parallel_line(&[cand0, cand1], p_s, p_e),
        Some(0)
    );
    assert_eq!(
        select_disjoint_parallel_line(&[cand1, cand0], p_s, p_e),
        Some(1)
    );

    // OVERLAPPING intervals (generators merged below resolution) → no clear
    // winner → None (the caller keeps its loud `AmbiguousCurve`). Put the two
    // lines symmetrically about the segment so each endpoint is equidistant.
    let near_a = line_at(2.0e-5);
    let near_b = line_at(-2.0e-5);
    assert_eq!(
        select_disjoint_parallel_line(&[near_a, near_b], p_s, p_e),
        None
    );

    // NON-parallel candidates → None (the tangent discriminator's job).
    let crossing = (Point3::new(0.0, 0.0, 0.0), Vector3::new(n[0], n[1], n[2]));
    assert_eq!(
        select_disjoint_parallel_line(&[cand0, crossing], p_s, p_e),
        None
    );

    // Fewer than two candidates → None.
    assert_eq!(select_disjoint_parallel_line(&[cand0], p_s, p_e), None);
}

// R0008: position tie-break for CROSSING generator lines through a cone apex.
// The two candidates are the live probe values from R0008's edge (43,44): a
// plane through the cone apex sections it into two generators that CROSS at the
// apex (NOT parallel — so `select_disjoint_parallel_line` bails) and are nearly
// aligned with the edge (so the tangent discriminator's 0.1 margin never
// fires). The edge lies on the HORIZONTAL generator (dir z-component ≈ 0, same
// z as the endpoints); the tilted generator is admitted only by the large cone
// chord band. The general `select_disjoint_line_by_distance` resolves it by
// disjoint perpendicular-distance interval; the parallel wrapper still returns
// None (its contract — the crossing case is not its job).
#[test]
pub(crate) fn r0008_cone_apex_crossing_generators_position_tiebreak() {
    let apex = Point3::new(
        39.562_058_563_451_22,
        -187.104_703_586_691_47,
        -27.121_056_731_101_312,
    );
    // cand 0: TILTED generator (z-component 0.0361) — the false match.
    let tilted = (
        apex,
        Vector3::new(
            0.100_642_603_516_822_69,
            -0.994_268_050_926_091_2,
            0.036_084_751_142_105_825,
        ),
    );
    // cand 1: HORIZONTAL generator (z-component ≈ 0) — the true edge curve.
    let horizontal = (
        apex,
        Vector3::new(
            -0.106_916_055_424_955_14,
            0.994_268_050_926_091_2,
            -1.144_917_494_144_692_7e-16,
        ),
    );
    let p_s = Point3::new(
        32.001_345_361_241_51,
        -116.793_696_046_306_76,
        -27.121_056_731_101_312,
    );
    let p_e = Point3::new(
        32.055_545_644_578_29,
        -117.297_732_693_176_21,
        -27.121_056_731_101_312,
    );

    // General position test resolves the crossing pair (order-independent).
    assert_eq!(
        select_disjoint_line_by_distance(&[tilted, horizontal], p_s, p_e),
        Some(1)
    );
    assert_eq!(
        select_disjoint_line_by_distance(&[horizontal, tilted], p_s, p_e),
        Some(0)
    );

    // The parallel wrapper still bails (crossing lines are not its case) — its
    // R0072 contract is preserved.
    assert_eq!(
        select_disjoint_parallel_line(&[tilted, horizontal], p_s, p_e),
        None
    );
}

pub(crate) fn d_scale(v: Vector3, s: f64) -> [f64; 3] {
    let d = normalize3(v.as_array());
    [d[0] * s, d[1] * s, d[2] * s]
}

// PR-YR10 N3 regression (Yang §4.5.3): a U-turn at p_r — consecutive points
// double back so v1 ≈ −v2 ⇒ |t̃| ≈ 0 — IS a reversal. The paper places the
// collinear/degenerate-t̃ case WITHIN the reversal subset ("directly detect
// the reversal, avoiding the angle comparisons"). p_b=(0,0,0) → p_r=(1,0,0)
// → p_n=(0.5,0,0) reverses direction (v1=+x, v2=−x, t̃=0). The degenerate
// branch must report a reversal. (Was the N3 logic inversion: returned
// `false` = "healthy", silently failing to correct the very reversal §4.5.3
// exists for; reachable whenever relocation produces an out-of-order point.)

// PR-6 (coincident-cylinder rim conformal weld). Locks the two invariants
// that make the curved-input rim weld a conformal exact-identity merge of
// redundant reconstructions — NOT a tolerance bucket that could mask
// unpaired edges (the reverted F0057 hazard):
//   (1) two sub-ULP rim duplicates of one analytic point are BOTH on the
//       cylinder (within the analytic band) AND within the cluster band,
//       so they fuse;
//   (2) two GENUINELY distinct rim points (≥ MIN_FEATURE_SIZE apart, here
//       the ~1e-4 chord spacing) are on the cylinder but FAR outside the
//       cluster band, so they never fuse.
#[test]
pub(crate) fn pr6_rim_weld_fuses_only_sub_ulp_duplicates() {
    let cyl = stage0::PairCylinder {
        axis_point: [0.0, 0.0, 0.0],
        axis_dir: [0.0, 0.0, 1.0],
        radius: 1.0,
        band: 1e-7,
        opposite: true,
    };
    let base = [1.0, 0.0, 0.3];
    // (1) A sub-ULP duplicate: perturb the in-plane coord by ~2 ULPs.
    let twin = [1.0 + 2.0 * f64::EPSILON, 0.0, 0.3];
    let scale = base
        .iter()
        .chain(twin.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
    assert!(
        centroid_on_cylinder(base, &cyl) <= cyl.band,
        "base rim point must be on the cylinder"
    );
    assert!(
        centroid_on_cylinder(twin, &cyl) <= cyl.band,
        "sub-ULP twin must still be on the cylinder"
    );
    assert!(
        (0..3).all(|k| (base[k] - twin[k]).abs() <= cluster_band),
        "sub-ULP twin must be within the cluster band ⇒ fuses"
    );
    // (2) A genuinely distinct rim point ~1e-4 away along the rim: on the
    // cylinder, but FAR outside the cluster band ⇒ never fused.
    let theta = 1e-4_f64;
    let distinct = [theta.cos(), theta.sin(), 0.3];
    assert!(
        centroid_on_cylinder(distinct, &cyl) <= cyl.band,
        "the distinct rim point is also exactly on the cylinder"
    );
    assert!(
        (0..3).any(|k| (base[k] - distinct[k]).abs() > cluster_band),
        "a genuinely distinct rim point (≥ chord spacing) must lie OUTSIDE \
             the cluster band so the conformal weld never fuses it (no \
             tolerance-bucket masking)"
    );
}

// KV15 (spec `kv15_mixed_operand_planar_near_weld` §4): the mixed-operand
// per-vertex near-weld. W2 — a planar-only femto pair (2 ULPs) fuses to
// the min index; W3 — a curved-adjacent root never near-welds (kv9
// junction-duplicate protection); W5 — genuinely distinct features
// (≥ MIN_FEATURE_SIZE) sit far outside the band and never fuse.
#[test]
pub(crate) fn kv15_planar_femto_pair_welds_to_min_index() {
    let base = p(1.0, 0.0, 0.3);
    let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
    let verts = vec![base, twin];
    let mut weld = vec![0u32, 1u32];
    kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
    assert_eq!(
        weld,
        vec![0, 0],
        "W2: a planar femto pair fuses, min-index survivor"
    );
}

#[test]
pub(crate) fn kv15_curved_adjacent_root_never_near_welds() {
    let base = p(1.0, 0.0, 0.3);
    let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
    let verts = vec![base, twin];
    for flags in [[true, false], [false, true], [true, true]] {
        let mut weld = vec![0u32, 1u32];
        kv15_near_weld_pass(&verts, &mut weld, &flags);
        assert_eq!(
            weld,
            vec![0, 1],
            "W3: a curved-adjacent root (flags {flags:?}) must keep bit-exact \
                 identity — Stage-4 owns junction-duplicate collapse"
        );
    }
}

#[test]
pub(crate) fn kv15_distinct_features_never_fuse() {
    // 1e-4 apart at coordinate scale ~1 — eight orders beyond the
    // TAU_WORK·(1+scale) band; the pair must never fuse (no
    // tolerance-bucket masking, the reverted-F0057 hazard).
    let verts = vec![p(1.0, 0.0, 0.3), p(1.0 + 1.0e-4, 0.0, 0.3)];
    let mut weld = vec![0u32, 1u32];
    kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
    assert_eq!(
        weld,
        vec![0, 1],
        "W5: sub-floor is the mint-site's job; ≥-floor never fuses"
    );
}

/// KV15 spec W4 + §3 eligibility: only positively-proven all-planar
/// descent yields an eligible (non-curved) vertex. Empty provenance,
/// sentinel / out-of-range `tri_face` entries, an unknown face, and a
/// non-planar face all mark every vertex of the triangle curved.
#[test]
pub(crate) fn kv15_eligibility_is_conservative() {
    let tris = vec![[0u32, 1, 2]];
    let planar_a = |k: u32, fi: u32| (k == 0 && fi == 7).then_some(true);
    // Positively proven planar descent → eligible.
    let src = vec![vec![(LaInputId(0), 0u32)]];
    assert_eq!(
        kv15_curved_touch(3, &tris, &src, &[7], &[], planar_a),
        vec![false; 3],
        "proven planar descent is eligible"
    );
    // Empty provenance (sidecar producer) → curved.
    assert_eq!(
        kv15_curved_touch(3, &tris, &[Vec::new()], &[7], &[], planar_a),
        vec![true; 3],
        "W4: empty provenance stays bit-exact"
    );
    // Sentinel tri_face entry → curved.
    assert_eq!(
        kv15_curved_touch(3, &tris, &src, &[u32::MAX], &[], planar_a),
        vec![true; 3],
        "sentinel face map entry stays bit-exact"
    );
    // Out-of-range local tri index → curved.
    let src_oob = vec![vec![(LaInputId(0), 9u32)]];
    assert_eq!(
        kv15_curved_touch(3, &tris, &src_oob, &[7], &[], planar_a),
        vec![true; 3],
        "out-of-range provenance stays bit-exact"
    );
    // Non-planar face → curved; input B routes through tri_face_b.
    let cyl_b = |k: u32, fi: u32| (k == 1 && fi == 3).then_some(false);
    let src_b = vec![vec![(LaInputId(1), 0u32)]];
    assert_eq!(
        kv15_curved_touch(3, &tris, &src_b, &[], &[3], cyl_b),
        vec![true; 3],
        "a curved-face descendant marks its vertices ineligible"
    );
    // Multi-parent (coplanar overlap): ONE curved parent poisons the tri.
    let mixed = vec![vec![(LaInputId(0), 0u32), (LaInputId(1), 0u32)]];
    let planar_a_cyl_b = |k: u32, fi: u32| ((k, fi) == (0, 7)).then_some(true).or(Some(false));
    assert_eq!(
        kv15_curved_touch(3, &tris, &mixed, &[7], &[3], planar_a_cyl_b),
        vec![true; 3],
        "any curved parent of a multi-parent tri stays bit-exact"
    );
}

// KV15b (spec `kv15b_mint_site_subresolution_collapse` §7): the
// emission collapse of sub-`TAU_MODEL` intersection segments.
pub(crate) fn kv15b_map(segs: &[(u32, u32)]) -> std::collections::BTreeMap<(u32, u32), Curve> {
    segs.iter()
        .map(|&(a, b)| ((a.min(b), a.max(b)), Curve::LineSegment))
        .collect()
}

#[test]
pub(crate) fn kv15b_subresolution_intersection_segment_collapses() {
    // B1/I1: a 5e-8 intersection segment (0,1) collapses; min index
    // survives with its original bits; the degenerate tri drops.
    let twin = p(5.0e-8, 0.0, 0.0);
    let mut mesh = Mesh::new(
        vec![p(0.0, 0.0, 0.0), twin, p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    let mut attr = vec![None; 2];
    let map = kv15b_map(&[(0, 1)]);
    assert!(collapse_subresolution_intersection_segments(
        &mut mesh,
        &mut attr,
        &map,
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
    ));
    assert_eq!(
        mesh.tris,
        vec![[0, 2, 3]],
        "degenerate tri dropped, twin remapped"
    );
    assert_eq!(
        mesh.verts[0],
        p(0.0, 0.0, 0.0),
        "I1: the survivor keeps its own exact coordinates"
    );
    assert_eq!(attr.len(), 1, "attribution stays in lockstep with tris");
}

#[test]
pub(crate) fn kv15b_supraresolution_segment_untouched() {
    // B2/I2: 2e-7 ≥ TAU_MODEL — never collapses (a mutation widening the
    // band to MIN_FEATURE_SIZE must fail here: 2e-7 < 1e-6).
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(2.0e-7, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    let mut attr = vec![None; 2];
    let map = kv15b_map(&[(0, 1)]);
    assert!(!collapse_subresolution_intersection_segments(
        &mut mesh,
        &mut attr,
        &map,
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
    ));
    assert_eq!(
        mesh.tris,
        vec![[0, 1, 3], [1, 2, 3]],
        "B2: ≥ TAU_MODEL stays"
    );
}

#[test]
pub(crate) fn kv15b_non_intersection_edge_untouched() {
    // B4/I3: the sub-TAU pair (0,1) is NOT an intersection segment —
    // inherited operand geometry (micro-profile corners) never collapses
    // (a mutation dropping the intersection-membership gate fails here).
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(5.0e-8, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 3], [1, 2, 3]],
    );
    let mut attr = vec![None; 2];
    let map = kv15b_map(&[(1, 2)]); // only the LONG edge is intersection
    assert!(!collapse_subresolution_intersection_segments(
        &mut mesh,
        &mut attr,
        &map,
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
    ));
    assert_eq!(
        mesh.tris,
        vec![[0, 1, 3], [1, 2, 3]],
        "B4: a sub-TAU NON-intersection edge is inherited geometry — untouched"
    );
}

#[test]
pub(crate) fn kv15b_twin_chain_resolves_to_single_survivor() {
    // B5: chain 0–1–2 with both links sub-TAU (5e-8 + 4e-8): both
    // collapse onto vertex 0 through the redirect (no chain drift beyond
    // the original twin cluster; exact-zero pairs B3 are never touched).
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(5.0e-8, 0.0, 0.0),
            p(9.0e-8, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
    );
    let mut attr = vec![None; 3];
    let map = kv15b_map(&[(0, 1), (1, 2)]);
    assert!(collapse_subresolution_intersection_segments(
        &mut mesh,
        &mut attr,
        &map,
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
    ));
    assert_eq!(
        mesh.tris,
        vec![[0, 3, 4]],
        "B5: both twins collapse onto the min index; degenerate tris drop"
    );
}

// Spec `yang_stage6_sliver_topology` amendment 1 (S7): the
// certainly-fatal chord split + null-excursion cancellation.
pub(crate) fn s7_info(cycles: Vec<Vec<(u32, u32)>>) -> PatchInfo {
    PatchInfo {
        cycles,
        input: InputId::A,
        inherited: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        face_idx: 0,
        input_reversed: false,
        had_fold_sliver: false,
    }
}

pub(crate) fn s7_mesh() -> Mesh {
    Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),   // 0: chord start
            p(0.374, 0.0, 0.0), // 1: on the chord (exact)
            p(1.0, 0.0, 0.0),   // 2: chord end
            p(0.5, 1.0, 0.0),   // 3: apex of loop A
            p(0.5, -1.0, 0.0),  // 4: apex of loop B
            p(0.2, -1.0, 0.0),  // 5: apex of the second chord user (benign T)
        ],
        vec![[0, 2, 3], [1, 2, 4]],
    )
}

#[test]
pub(crate) fn s7_fatal_chord_splits_and_spur_cancels() {
    // Loop A walks a spur (1→0) + the chord (0,2) over vertex 1; loop B
    // walks (2→1). Chord use-count 1, complementary {0,1}/{1,2} both
    // present → split at 1; the spur then cancels (amendment 1a) and A
    // emerges as the clean triangle 1→2→3→1.
    let infos = vec![
        s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
        s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
    ];
    let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
    assert_eq!(
        out[0][0],
        vec![(1, 2), (2, 3), (3, 1)],
        "S7: chord split at the on-segment vertex, spur cancelled"
    );
    assert_eq!(out[1][0], infos[1].cycles[0], "loop B untouched");
}

#[test]
pub(crate) fn s7_benign_t_junction_untouched() {
    // The chord (0,2) is walked by TWO loops (use 2) while the
    // complementary chain {0,1}/{1,2} ALSO exists (loops A + C) — this
    // isolates the use==1 gate: a mutation dropping it splits here and
    // fails (the reference-parity guard for benign T-junctions).
    let infos = vec![
        s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
        s7_info(vec![vec![(2, 0), (0, 5), (5, 2)]]),
        s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
    ];
    let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
    assert_eq!(out[0][0], infos[0].cycles[0], "use-2 chord never splits");
    assert_eq!(out[1][0], infos[1].cycles[0]);
}

#[test]
pub(crate) fn s7_missing_complementary_chain_untouched() {
    // No loop walks {1,2}: the complementary chain is absent, so the
    // split cannot certify a repair — S6 residue, unchanged.
    let infos = vec![
        s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
        s7_info(vec![vec![(0, 1), (1, 4), (4, 0)]]),
    ];
    let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
    assert_eq!(out[0][0], infos[0].cycles[0]);
}

#[test]
pub(crate) fn s7_off_band_vertex_untouched() {
    // Vertex 1 lifted 1e-9 off the segment (> TAU_WORK): outside the
    // last-ulp band — no split (a mutation widening the band fails here).
    let mut mesh = s7_mesh();
    mesh.verts[1] = p(0.374, 1.0e-9, 0.0);
    let infos = vec![
        s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
        s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
    ];
    let out = subdivide_loops_at_shared_vertices(&infos, &mesh);
    assert_eq!(out[0][0], infos[0].cycles[0]);
}

// Spec `yang_s3_ellipse_rim_chord_bound` §7: the Stage-3 fallback bound
// for ellipse-rim-only curved owners.
#[test]
pub(crate) fn s3_ellipse_rim_bound_is_max_major_radius_scaled() {
    // T2: mixed seg/ellipse edge list → 1e-2 · MAX major_radius (the
    // largest Stage-1 chain bound; a mutation picking min or the
    // minor_radius must fail).
    let ell = |a: f64, b: f64| BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Ellipse {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            major_radius: a,
            minor_radius: b,
        },
    };
    let seg = BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    };
    let edges = vec![seg.clone(), ell(0.25, 0.2), ell(0.5, 0.1), seg];
    assert_eq!(
        ellipse_rim_chord_bound(&edges),
        Some(1e-2 * 0.5),
        "T2: the fallback is the LARGEST ellipse-chain bound"
    );
}

#[test]
pub(crate) fn s3_ellipse_rim_bound_none_without_ellipses() {
    // T3: a seg-only owner has no fallback — the loud producer fault
    // stands (a mutation returning Some(TAU_WORK) here must fail).
    let seg = BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    };
    assert_eq!(
        ellipse_rim_chord_bound(&[seg]),
        None,
        "T3: no Circle and no Ellipse → producer fault preserved"
    );
}

// Deviation N38: the cone-owning Stage-3 selection band is bound to the EDGE's
// OWN cone band (exact `Surface` match), NOT an arbitrary first cone face. The
// pre-fix bug paired one band's apex with another band's rim on a multi-band
// gear revolve, minting a nonsense height and a too-tight band that spuriously
// rejected legitimate chord-error endpoints (R0003).
#[test]
pub(crate) fn n38_cone_band_bound_binds_to_matching_band() {
    use std::f64::consts::FRAC_PI_4;
    let circle = |cz: f64, r: f64| Curve::Circle {
        center: p(0.0, 0.0, cz),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    };
    // Band A: apex origin, rim at height 10 (r = 10·tan45° = 10).
    // Band B: apex z=50, rim at height 2 from its OWN apex (r = 2).
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: circle(10.0, 10.0),
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: circle(52.0, 2.0),
        },
    ];
    let cone_surf = |apex_z: f64| Surface::Cone {
        apex: p(0.0, 0.0, apex_z),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: FRAC_PI_4,
    };
    let band_a = BRepFace {
        surface: cone_surf(0.0),
        outer_loop: vec![0],
        inner_loops: vec![],
        reversed: false,
    };
    let band_b = BRepFace {
        surface: cone_surf(50.0),
        outer_loop: vec![1],
        inner_loops: vec![],
        reversed: false,
    };
    // Band B listed FIRST — a "first cone face" bug would return Band B's
    // bound for an edge that names Band A.
    let faces = vec![band_b, band_a];

    // An edge on Band A must select Band A's OWN bound (height 10), NOT the
    // first-listed Band B's (height 2).
    let got_a = cone_band_chord_bound(cone_surf(0.0), &faces, &edges).expect("band A matches");
    assert!(
        (got_a - cone_chord_bound(10.0, FRAC_PI_4)).abs() < 1e-15,
        "edge on Band A gets Band A's own chord bound"
    );
    // An edge on Band B selects Band B's bound.
    let got_b = cone_band_chord_bound(cone_surf(50.0), &faces, &edges).expect("band B matches");
    assert!(
        (got_b - cone_chord_bound(2.0, FRAC_PI_4)).abs() < 1e-15,
        "edge on Band B gets Band B's own chord bound"
    );
    assert!(got_a > got_b, "the two bands have distinct bounds");
}

#[test]
pub(crate) fn n38_cone_band_bound_max_height_rim() {
    use std::f64::consts::FRAC_PI_4;
    // A frustum band bounded by TWO rims (heights 4 and 10). The bound must
    // use the MAX-height rim (larger radius ⇒ larger circumferential sagitta).
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 4.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 10.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 10.0,
            },
        },
    ];
    let band = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: FRAC_PI_4,
    };
    let faces = vec![BRepFace {
        surface: band,
        outer_loop: vec![0, 1],
        inner_loops: vec![],
        reversed: false,
    }];
    let got = cone_band_chord_bound(band, &faces, &edges).unwrap();
    assert!(
        (got - cone_chord_bound(10.0, FRAC_PI_4)).abs() < 1e-15,
        "max-height (h=10) rim drives the band bound, not h=4"
    );
}

#[test]
pub(crate) fn n38_cone_band_bound_none_without_circle_rims() {
    // A cone band with only a LineSegment edge has no Circle rim → None,
    // preserving the loud producer-fault path (a mutation returning
    // Some(TAU_WORK) must fail).
    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    }];
    let band = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let faces = vec![BRepFace {
        surface: band,
        outer_loop: vec![0],
        inner_loops: vec![],
        reversed: false,
    }];
    assert_eq!(cone_band_chord_bound(band, &faces, &edges), None);
}

// N39 (task #161): the cone∩plane conic curve-distance amplification factor.
// A GRAZING plane-∥-axis hyperbola places a legitimate mesh chord point (off
// the cone within its Stage-1 chord band) FURTHER from the exact curve than
// the raw cone chord sagitta — by 1/sin α, α = angle(cone normal, plane
// normal). Without the factor the flat band under-admits (spurious
// AmbiguousCurve); with it the point is correctly matched.
#[test]
pub(crate) fn n39_cone_plane_hyperbola_amplification_is_load_bearing() {
    // yr23 config: cone apex-origin, axis +Z, tanα = 0.5 (half_angle = atan 0.5),
    // height 4. Cutting plane x = 1 (normal +X, d = −1) is PARALLEL to the axis
    // → the section is a hyperbola. At the branch vertex the cone normal makes
    // angle α_cone = 90°−atan(0.5) with the axis, so sin(angle to the plane
    // normal) = sin(atan 0.5) and the amplification is 1/sin(atan 0.5) ≈ 2.236.
    let half_angle = 0.5_f64.atan();
    let cone = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.0,
    };
    // The real ssi-rs section (independent of the amplification code).
    let q_plane = surface_to_quadric(plane).expect("plane quadric");
    let q_cone = surface_to_quadric(cone).expect("cone quadric");
    let curves = ssi_rs::intersect(&q_plane, &q_cone).expect("cone∩plane hyperbola");
    // Upper nappe: major_axis has +Z component (the branch on the +Z solid).
    let upper = curves
        .iter()
        .find(|c| match c {
            ssi_rs::SsiCurve::Hyperbola { major_axis, .. } => major_axis.as_array()[2] > 0.0,
            _ => false,
        })
        .expect("an upper-nappe hyperbola branch");
    let (center, major, a) = match upper {
        ssi_rs::SsiCurve::Hyperbola {
            center,
            major_axis,
            semi_transverse,
            ..
        } => (center.as_array(), major_axis.as_array(), *semi_transverse),
        _ => unreachable!(),
    };
    // Branch vertex V = center + a·major_axis (t = 0). ≈ (1, 0, 2).
    let v = Point3::new(
        center[0] + a * major[0],
        center[1] + a * major[1],
        center[2] + a * major[2],
    );

    // The amplification at the vertex is the grazing 1/sin α ≈ 2.236.
    let amp = surface_pair_point_amplification(v, cone, plane).expect("finite grazing amp");
    assert!(
        amp > 2.0,
        "a plane-∥-axis hyperbola vertex is grazing → amp 1/sinα ≈ 2.236, got {amp}"
    );

    // A legitimate mesh chord point: on the plane (x=1), off the cone axially by
    // ε along +Z from the vertex — off the cone RADIALLY by ε/2 (within the
    // chord band), but off the exact hyperbola by ≈ε (the amplified distance).
    let flat = cone_chord_bound(4.0, half_angle); // the cone's Stage-1 band ≈ 0.0566
    let eps = 0.08;
    let q = Point3::new(v.x(), v.y(), v.z() + eps);
    let cone_off = signed_distance_to_surface(cone, q).unwrap().abs();
    assert!(
        cone_off <= flat,
        "the probe point must be a LEGITIMATE mesh point (off the cone {cone_off} \
         within its chord band {flat})"
    );
    // The plane is exact: the point is ON it.
    assert!(signed_distance_to_surface(plane, q).unwrap().abs() <= cad_primitives::TAU_WORK);

    // RED (flat band): the legitimate point is WRONGLY rejected off the curve.
    assert!(
        !curve_contains_point(upper, q, flat, None),
        "flat cone band under-admits the grazing-hyperbola chord point (the N39 bug)"
    );
    // GREEN (amplified band): the point is correctly matched.
    assert!(
        curve_contains_point(upper, q, flat * amp, None),
        "the amplified band flat·(1/sinα) correctly admits the chord point"
    );
}

#[test]
pub(crate) fn n39_amplification_matches_gradient_cross_product() {
    // The factor is exactly 1/‖n̂₀ × n̂₁‖ of the two surface gradients — a
    // mutation using the dot product, sin↔cos, or one surface must fail.
    let cone = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.5_f64.atan(),
    };
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.0,
    };
    let x = Point3::new(1.0, 0.0, 2.0);
    let g0 = surface_normal_at(cone, x).unwrap();
    let g1 = surface_normal_at(plane, x).unwrap();
    let cx = [
        g0[1] * g1[2] - g0[2] * g1[1],
        g0[2] * g1[0] - g0[0] * g1[2],
        g0[0] * g1[1] - g0[1] * g1[0],
    ];
    let sin_a = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    let got = surface_pair_point_amplification(x, cone, plane).unwrap();
    assert!((got - 1.0 / sin_a).abs() < 1e-12, "amp = 1/‖ĝ₀×ĝ₁‖");
}

#[test]
pub(crate) fn n39_amplification_none_at_cone_apex() {
    // At the apex the cone gradient is singular → None (the caller keeps the
    // flat band + tangent discriminator; never a silent everything-matches).
    let cone = Surface::Cone {
        apex: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.5_f64.atan(),
    };
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 0.0,
    };
    assert_eq!(
        surface_pair_point_amplification(Point3::new(0.0, 0.0, 0.0), cone, plane),
        None
    );
}

// N46 (task #164): the EXACT cylinder∩plane generator-line band. `line_amp`'s
// constant `R/√(R²−d²)` is the tangent slope of the concave η(radial)=√(radial²−d²)
// at radial=R; for finite `tol` near tangency it UNDER-predicts the perpendicular
// distance, so a legitimate mesh chord point is wrongly rejected off BOTH
// generators (R0026's `AmbiguousCurve{2,0}`). The exact worst-case band
// `√(B_in²+tol²)` admits it while the wrong (11×-farther) generator stays out.
// Geometry is R0026's edge (131,197), probed from the assay.
fn r0026_cyl_plane_fixture() -> (Surface, Surface, Point3, Point3, f64) {
    let cyl = Surface::Cylinder {
        axis_point: p(
            -0.03525890036742006,
            0.06844222368655112,
            -0.057767985112573875,
        ),
        axis_dir: Vector3::new(
            -0.6082295054207996,
            -0.31717801479140706,
            0.7276365683969928,
        ),
        radius: 0.03575800166968048,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(0.6451807609275815, 0.3364472639173315, 0.6859628447164238),
        d: 0.008601756033135673,
    };
    let p_s = p(
        -0.05406432762021697,
        0.07585316408040951,
        0.00110635509114421,
    );
    let p_e = p(
        -0.053983474941050165,
        0.07590435591733039,
        0.0010052010023050533,
    );
    // The cylinder's Stage-1 chord band for this edge (probed `tol`).
    let tol = 1.498e-3;
    (cyl, plane, p_s, p_e, tol)
}

#[test]
pub(crate) fn n46_cyl_plane_generator_band_exceeds_linearization() {
    // O1: the exact band strictly exceeds its first-order linearization for a
    // finite `tol` with `d < R` (concavity of √(radial²−d²)).
    let (cyl, plane, _, _, tol) = r0026_cyl_plane_fixture();
    let exact = cyl_plane_generator_band(cyl, plane, tol).expect("d<R, R-tol>d → Some");
    let linear = line_band_amplification(cyl, plane).expect("cyl∩plane → Some amp") * tol;
    assert!(
        exact > linear,
        "exact band {exact} must exceed the linearization {linear} (concave η)"
    );
    // And the gap is the R0026-relevant ~7%+ (not a rounding wobble).
    assert!(
        exact > linear * 1.05,
        "exact/linear = {} — the near-tangency gap must be material",
        exact / linear
    );
}

#[test]
pub(crate) fn n46_cyl_plane_generator_band_is_load_bearing() {
    // O2/O3: with R0026's geometry the LINEAR band rejects the legitimate chord
    // endpoints off the correct generator (the bug), the EXACT band admits them,
    // and the wrong generator stays rejected under the exact band.
    let (cyl, plane, p_s, p_e, tol) = r0026_cyl_plane_fixture();
    let q_cyl = surface_to_quadric(cyl).expect("cyl quadric");
    let q_plane = surface_to_quadric(plane).expect("plane quadric");
    let curves = ssi_rs::intersect(&q_cyl, &q_plane).expect("cyl∩plane 2 generators");
    let lines: Vec<&ssi_rs::SsiCurve> = curves
        .iter()
        .filter(|c| matches!(c, ssi_rs::SsiCurve::Line { .. }))
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "a plane ∥ axis sections the cylinder in 2 generators"
    );

    let perp = |c: &ssi_rs::SsiCurve, pt: Point3| -> f64 {
        let ssi_rs::SsiCurve::Line { point, dir } = c else {
            unreachable!()
        };
        let d = {
            let a = dir.as_array();
            let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            [a[0] / n, a[1] / n, a[2] / n]
        };
        let pa = point.as_array();
        let w = [pt.x() - pa[0], pt.y() - pa[1], pt.z() - pa[2]];
        let h = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let r = [w[0] - h * d[0], w[1] - h * d[1], w[2] - h * d[2]];
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    // Correct generator = the one nearer to p_s; wrong = the other.
    let (correct, wrong) = if perp(lines[0], p_s) < perp(lines[1], p_s) {
        (lines[0], lines[1])
    } else {
        (lines[1], lines[0])
    };
    // Sanity: the wrong generator is an order of magnitude farther.
    assert!(
        perp(wrong, p_s) > perp(correct, p_s) * 5.0,
        "the two generators must be clearly distinguishable by position"
    );

    let linear = line_band_amplification(cyl, plane).unwrap() * tol;
    let exact = cyl_plane_generator_band(cyl, plane, tol).unwrap();

    for pt in [p_s, p_e] {
        // RED: the linear band under-admits the legitimate endpoint.
        assert!(
            !curve_contains_point(correct, pt, linear, None),
            "linear band {linear} wrongly rejects endpoint perp {} (the N46 bug)",
            perp(correct, pt)
        );
        // GREEN: the exact band admits it.
        assert!(
            curve_contains_point(correct, pt, exact, None),
            "exact band {exact} must admit endpoint perp {}",
            perp(correct, pt)
        );
        // O3: the wrong generator is NOT admitted (no false positive).
        assert!(
            !curve_contains_point(wrong, pt, exact, None),
            "the wrong generator (perp {}) must stay rejected under the exact band {exact}",
            perp(wrong, pt)
        );
    }
}

#[test]
pub(crate) fn n46_cyl_plane_generator_band_none_guards() {
    // O4: None for a non-cyl/plane pair, for d ≥ R (plane misses), and for
    // R − tol ≤ d (merged-generator near-tangency → the loud stop stands).
    let (cyl, _, _, _, tol) = r0026_cyl_plane_fixture();
    // Non-matching pair.
    assert_eq!(cyl_plane_generator_band(cyl, cyl, tol), None);
    // Plane far outside the cylinder (d ≫ R).
    let far = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -10.0,
    };
    let axis_cyl = Surface::Cylinder {
        axis_point: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(cyl_plane_generator_band(axis_cyl, far, tol), None);
    // Near-tangency: plane at d = 0.999·R, so R − tol < d.
    let tangent_pl = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -0.999,
    };
    assert_eq!(
        cyl_plane_generator_band(axis_cyl, tangent_pl, 0.01),
        None,
        "R − tol ≤ d must return None (merged generators, loud stop stands)"
    );
    // Sanity that the SAME axis cylinder with a comfortably-secant plane IS Some.
    let secant = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -0.5,
    };
    assert!(cyl_plane_generator_band(axis_cyl, secant, 0.01).is_some());
}

#[test]
pub(crate) fn kv15b_resolved_length_regrows_past_band_stays() {
    // B5 second half: after 1→0, segment (1,2) resolves to (0,2) at
    // 1.2e-7 ≥ TAU_MODEL — it must NOT collapse (single-sweep, no drift).
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(5.0e-8, 0.0, 0.0),
            p(1.2e-7, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
    );
    let mut attr = vec![None; 3];
    let map = kv15b_map(&[(0, 1), (1, 2)]);
    assert!(collapse_subresolution_intersection_segments(
        &mut mesh,
        &mut attr,
        &map,
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
        &guard_cyl(0.0, 0.0, 1.0, 1.0),
    ));
    assert_eq!(
        mesh.tris,
        vec![[0, 2, 4], [2, 3, 4]],
        "a segment whose RESOLVED length is ≥ TAU_MODEL stays (I2)"
    );
}

// Spec `yang_453_junction_protected_collapse` §3: the §4.5.3 collapse
// victim is `p_n` on a same-curve run, but `p_r` when `p_n` is a curve
// junction (the loop's curve changes at `p_n`).
#[test]
pub(crate) fn s453_collapse_removes_p_n_on_same_curve_run() {
    let circle = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
        std::collections::BTreeMap::new();
    curves.insert((1, 2), circle);
    curves.insert((2, 3), circle);
    let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    assert_eq!(
        reversal_collapse_direction(&curves, &inc, 1, 2, 3),
        (2, 1),
        "same curve beyond p_n ⇒ paper default: p_n is the victim"
    );
}

#[test]
pub(crate) fn s453_collapse_protects_junction_p_n() {
    let circle = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let other = Curve::Circle {
        center: p(5.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
        std::collections::BTreeMap::new();
    curves.insert((1, 2), circle);
    curves.insert((2, 3), other);
    let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    assert_eq!(
        reversal_collapse_direction(&curves, &inc, 1, 2, 3),
        (1, 2),
        "curve changes at p_n ⇒ p_n is an exact curve-junction endpoint \
             and must survive; the overshooting p_r is the victim"
    );
    // Canonical-key robustness: descending vertex ids on both edges.
    let mut curves_rev: std::collections::BTreeMap<(u32, u32), Curve> =
        std::collections::BTreeMap::new();
    curves_rev.insert((7, 9), circle);
    curves_rev.insert((3, 7), other);
    assert_eq!(
        reversal_collapse_direction(&curves_rev, &inc, 9, 7, 3),
        (9, 7),
        "junction protection must hold under canonical (min,max) edge keys"
    );
}

// Spec §3c: straight-run reversal — branch table 4–7 on synthetic
// curve + incidence maps. The seam runs along +x; vertex 1 (p_r) doubles
// back to vertex 2 (p_n) at 0.5 (a U-turn on the run).
#[test]
pub(crate) fn s453c_line_run_reversal_branches() {
    use std::collections::BTreeMap;
    let mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.5, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
        ],
        vec![],
    );
    let lo = std::f64::consts::FRAC_PI_4;
    let hi = 3.0 * std::f64::consts::FRAC_PI_4;
    let plane_a = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let plane_b = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    let plane_c = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 1.0),
        d: 0.0,
    };
    let mut curves: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
    curves.insert((0, 1), Curve::LineSegment);
    curves.insert((1, 2), Curve::LineSegment);
    curves.insert((2, 3), Curve::LineSegment);
    let pair = vec![(InputId::A, plane_a), (InputId::B, plane_b)];
    let pair_swapped = vec![(InputId::B, plane_b), (InputId::A, plane_a)];
    let pair_other = vec![(InputId::A, plane_a), (InputId::B, plane_c)];

    // Branch 7/6 precondition: same run through p_r (pair equality is
    // unordered), U-turn detected.
    let mut inc: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
    inc.insert((0, 1), pair.clone());
    inc.insert((1, 2), pair_swapped.clone());
    inc.insert((2, 3), pair.clone());
    assert!(
        is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
        "a U-turn on ONE straight seam run (unordered-equal pairs) is a \
             §4.5.3 reversal"
    );
    // Branch 7: same pair continues past p_n → paper default victim p_n.
    assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (2, 1));
    // Branch 6: pair changes at p_n → p_n is the run junction; p_r is
    // the victim.
    inc.insert((2, 3), pair_other.clone());
    assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (1, 2));

    // Branch 4: pair changes AT p_r → corner, never tested as a reversal
    // (even though the polyline doubles back).
    let mut inc4: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
    inc4.insert((0, 1), pair.clone());
    inc4.insert((1, 2), pair_other.clone());
    assert!(
        !is_reversed(&mesh, &curves, &inc4, 0, 1, 2, lo, hi),
        "a surface-pair change at p_r is a genuine corner, not a reversal"
    );

    // Branch 5: tangent/parallel pair (n_A × n_B ≈ 0) — cannot diagnose.
    // Use NON-doubling geometry so the U-turn arm doesn't fire first.
    let mesh5 = Mesh::new(
        vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0)],
        vec![],
    );
    let coincident = vec![(InputId::A, plane_a), (InputId::B, plane_a)];
    let mut inc5: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
    inc5.insert((0, 1), coincident.clone());
    inc5.insert((1, 2), coincident.clone());
    assert!(
        !is_reversed(&mesh5, &curves, &inc5, 0, 1, 2, lo, hi),
        "a coincident-plane seam (§4.5.5) has no cross-product tangent — \
             healthy skip"
    );

    // Per-site eligibility: a run boundary (missing curve entry on one
    // side) is never a reversal site.
    let mut curves_gap: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
    curves_gap.insert((1, 2), Curve::LineSegment);
    assert!(
        !is_reversed(&mesh, &curves_gap, &inc, 0, 1, 2, lo, hi),
        "p_r with a curve-less incident edge is a run boundary, not a site"
    );
    // Run END at p_n: curve(p_r,p_n) exists, curve(p_n,p_after) doesn't —
    // p_n survives, p_r is the victim.
    assert_eq!(
        reversal_collapse_direction(&curves_gap, &inc, 1, 2, 3),
        (1, 2),
        "the run's exact endpoint (no intersection edge beyond) survives"
    );
}

// ── Task #145: spec `yang_453_mixed_cycle_conic_backtrack` branches 9–12 ──

/// Branch 9/10: exact conic parameter-order reversal on a shared circle.
/// A backtrack (t runs 10° → 5° → 20°) is a reversal; a monotone coarse
/// 7-gon corner (51.4° chords — the `corner_in_band` P10 shape) is healthy.
#[test]
pub(crate) fn s453d_shared_circle_backtrack_reversed() {
    let circle = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let at = |deg: f64| {
        let t = deg.to_radians();
        p(t.cos(), t.sin(), 0.0)
    };
    assert!(
        conic_param_reversed(&circle, at(10.0), at(5.0), at(20.0)),
        "parameter runs backward then forward — a §4.5.3 reversal (branch 9)"
    );
    assert!(
        !conic_param_reversed(&circle, at(0.0), at(51.4), at(102.9)),
        "coarse 7-gon chords progress monotonically — healthy (branch 10, I3)"
    );
    // Branch-cut robustness: the atan2 parameter wraps at ±π; monotone
    // progression across the cut must stay healthy, a backtrack across it
    // must still be detected.
    assert!(
        !conic_param_reversed(&circle, at(170.0), at(180.0), at(-170.0)),
        "monotone progression across the ±π branch cut is healthy"
    );
    assert!(
        conic_param_reversed(&circle, at(175.0), at(180.0), at(178.0)),
        "a backtrack straddling the branch cut is still a reversal"
    );
    // Branches 9a/9b: the deltas drive the survivor choice — the collapse
    // must target the parameter-NEARER neighbor (the one p_r overshot), so
    // the 2·d_ε gate bounds the ACTUAL overshoot, never a whole arc.
    let (d1, d2) = conic_param_deltas(&circle, at(10.0), at(5.0), at(20.0))
        .expect("parameters defined on the circle");
    assert!(
        d1 < 0.0 && d2 > 0.0 && d1.abs() < d2.abs(),
        "backward overshoot past p_b: |d1| (5°) is the overshoot, p_b survives (9a)"
    );
    let (d1, d2) = conic_param_deltas(&circle, at(0.0), at(22.0), at(20.0))
        .expect("parameters defined on the circle");
    assert!(
        d1 > 0.0 && d2 < 0.0 && d2.abs() < d1.abs(),
        "forward overshoot past p_n: |d2| (2°) is the overshoot, p_n survives (9b)"
    );
}

/// I2 adversary: a near-tangent plane∩cylinder ellipse (a = 2.4, b = 0.02 —
/// the R0061 scale) turns nearly 180° in 3D at its major-axis tip even for
/// a LEGIT monotone traversal. The discriminator must be parameter order,
/// never the 3D turn angle — this test kills a `v1·v2 < 0` mutant.
#[test]
pub(crate) fn s453d_steep_ellipse_peak_monotone_is_healthy() {
    let ell = Curve::Ellipse {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.4,
        minor_radius: 0.02,
    };
    let at = |deg: f64| {
        let t = deg.to_radians();
        p(2.4 * t.cos(), 0.02 * t.sin(), 0.0)
    };
    let (pb, pr, pn) = (at(-5.0), at(0.0), at(5.0));
    // Precondition: the 3D turn at the tip genuinely exceeds 90° (otherwise
    // this adversary would not discriminate the mutant).
    let v1 = [pr.x() - pb.x(), pr.y() - pb.y(), pr.z() - pb.z()];
    let v2 = [pn.x() - pr.x(), pn.y() - pr.y(), pn.z() - pr.z()];
    assert!(
        v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2] < 0.0,
        "fixture precondition: the tip traversal turns more than 90° in 3D"
    );
    assert!(
        !conic_param_reversed(&ell, pb, pr, pn),
        "monotone parameter progression around a steep tip is healthy (I2)"
    );
    // And the mirrored genuine backtrack on the SAME ellipse is detected.
    assert!(
        conic_param_reversed(&ell, at(2.0), at(-1.0), at(4.0)),
        "a genuine parameter backtrack on the eccentric ellipse is a reversal"
    );
}

/// I5: conic identity up to the stored normal's SIGN (a frame choice, not
/// geometry) — exact field comparison, never tolerance-based.
#[test]
pub(crate) fn s453d_conic_identity_up_to_normal_sign() {
    let e = Curve::Ellipse {
        center: p(1.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 0.5,
    };
    let e_flip = Curve::Ellipse {
        center: p(1.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, -1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 0.5,
    };
    let e_other = Curve::Ellipse {
        center: p(9.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 0.5,
    };
    assert!(conics_equal_up_to_normal_sign(&e, &e));
    assert!(conics_equal_up_to_normal_sign(&e, &e_flip));
    assert!(!conics_equal_up_to_normal_sign(&e, &e_other));
    let c = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 1.0, 0.0),
        radius: 1.0,
    };
    let c_flip = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, -1.0, 0.0),
        radius: 1.0,
    };
    let c_r2 = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 1.0, 0.0),
        radius: 2.0,
    };
    assert!(conics_equal_up_to_normal_sign(&c, &c_flip));
    assert!(!conics_equal_up_to_normal_sign(&c, &c_r2));
    assert!(!conics_equal_up_to_normal_sign(&c, &e));
    assert!(!conics_equal_up_to_normal_sign(
        &Curve::LineSegment,
        &Curve::LineSegment
    ));
}

/// Branch 12 eligibility: a mixed-cycle site is a shared-conic site iff BOTH
/// incident edges carry the same conic (exact or up-to-normal-sign). A
/// junction (different conics), a conic/LineSegment boundary, and a straight
/// run (the §3c arm's turf) are all ineligible.
#[test]
pub(crate) fn s453d_shared_conic_site_eligibility() {
    use std::collections::BTreeMap;
    let e = Curve::Ellipse {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 0.5,
    };
    let e_flip = Curve::Ellipse {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, -1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 0.5,
    };
    let other = Curve::Circle {
        center: p(5.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let mut curves: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
    curves.insert((1, 2), e);
    curves.insert((2, 3), e);
    assert_eq!(mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)), Some(e));
    curves.insert((2, 3), e_flip);
    assert_eq!(
        mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)),
        Some(e),
        "sign-flipped storage of the same conic is ONE curve (I5)"
    );
    curves.insert((2, 3), other);
    assert_eq!(
        mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)),
        None,
        "a junction between two different conics is not a site"
    );
    curves.insert((2, 3), Curve::LineSegment);
    assert_eq!(
        mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)),
        None,
        "a conic/LineSegment boundary is not a site"
    );
    let mut lines: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
    lines.insert((1, 2), Curve::LineSegment);
    lines.insert((2, 3), Curve::LineSegment);
    assert_eq!(
        mixed_cycle_shared_conic(&lines, (1, 2), (2, 3)),
        None,
        "straight runs belong to the §3c both-line arm, not this one"
    );
}

// ── Task #146: spec `yang_stage4_circle_pp_line_junction` branches 1–5 ──

/// Branch 4/5: the line∩circle junction closed form — in-plane crossing
/// (the F0064 configuration: the pp-line lies IN the circle's plane),
/// transversal crossing, and a clean miss.
#[test]
pub(crate) fn s146_pp_line_circle_junction_closed_form() {
    let center = p(0.0, 0.0, 0.0);
    let normal = Vector3::new(0.0, 0.0, 1.0);
    let r = 1.0;
    // In-plane line y = 0.6, z = 0 crosses the unit circle at x = ±0.8; the
    // current position near (0.79, 0.61) must pick the +x root.
    let j = pp_line_circle_junction(
        p(0.0, 0.6, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        center,
        normal,
        r,
        p(0.79, 0.61, 0.0),
        1.0e-9,
    )
    .expect("in-plane crossing resolves");
    assert!((j.x() - 0.8).abs() < 1.0e-12 && (j.y() - 0.6).abs() < 1.0e-12);
    // Transversal line through (0.8, 0.6, −1) along +z pierces the circle
    // plane exactly on the circle.
    let j2 = pp_line_circle_junction(
        p(0.8, 0.6, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        center,
        normal,
        r,
        p(0.8, 0.6, -0.001),
        1.0e-9,
    )
    .expect("transversal crossing resolves");
    assert!((j2.z()).abs() < 1.0e-12 && (j2.x() - 0.8).abs() < 1.0e-12);
    // A line missing the circle (y = 2) has no junction (branch 5).
    assert!(pp_line_circle_junction(
        p(0.0, 2.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        center,
        normal,
        r,
        p(0.0, 2.0, 0.0),
        1.0e-9,
    )
    .is_none());
    // A transversal line piercing the plane INSIDE the circle: both sphere
    // roots are off the plane at real scale — no junction (branch 5).
    assert!(pp_line_circle_junction(
        p(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        center,
        normal,
        r,
        p(0.0, 0.0, 0.0),
        1.0e-9,
    )
    .is_none());
}

/// The pp-line closed form: point on both planes, direction along their
/// cross product; parallel planes have no unique line.
#[test]
pub(crate) fn s146_pp_line_closed_form() {
    let (pt, dir) = pp_line(
        Vector3::new(0.0, 0.0, 1.0),
        -0.5,
        Vector3::new(0.0, 1.0, 0.0),
        -0.25,
    )
    .expect("transversal planes intersect in a line");
    assert!((pt.z() - 0.5).abs() < 1.0e-12 && (pt.y() - 0.25).abs() < 1.0e-12);
    let d = normalize3(dir.as_array());
    assert!(d[0].abs() > 0.999, "line runs along x");
    assert!(pp_line(
        Vector3::new(0.0, 0.0, 1.0),
        -0.5,
        Vector3::new(0.0, 0.0, 1.0),
        -0.75,
    )
    .is_none());
}

/// Branches 1–3: entry dedup — duplicated entries (either plane order)
/// collapse to one line; two DISTINCT lines refuse.
#[test]
pub(crate) fn s146_dedup_single_pp_line() {
    let za = (Vector3::new(0.0, 0.0, 1.0), -0.5);
    let ya = (Vector3::new(0.0, 1.0, 0.0), -0.25);
    let yb = (Vector3::new(0.0, 1.0, 0.0), -0.75);
    let e1 = (za.0, za.1, ya.0, ya.1);
    let e1_swapped = (ya.0, ya.1, za.0, za.1);
    let e2 = (za.0, za.1, yb.0, yb.1);
    assert!(dedup_single_pp_line(&[e1, e1, e1_swapped]).is_some());
    assert!(
        dedup_single_pp_line(&[e1, e2]).is_none(),
        "two distinct pp-lines are over-determined (branch 3)"
    );
}

/// Spec `yang_453_mixed_cycle_conic_backtrack` §3b (mechanism 2): on a
/// near-tangent section the azimuth-preserving projection slides a vertex
/// ~1/(n·â) ALONG the ellipse; the in-plane nearest-point projection must
/// stay within a small multiple of the actual off-curve residual (I6).
#[test]
pub(crate) fn s453e_near_tangent_ellipse_nearest_projection_bounded() {
    // Cylinder: axis z through origin, r = 0.02. Plane: n ≈ (0.99995, 0, 0.01),
    // n·x = 0.01 → |n·â| = 0.01, section ellipse a = r/|n·â| = 2, b = r.
    let r = 0.02_f64;
    let n_dot_a = 0.01_f64;
    let nx = (1.0 - n_dot_a * n_dot_a).sqrt();
    let plane_n = Vector3::new(nx, 0.0, n_dot_a);
    let plane_d = -0.01_f64; // n·x + d = 0 passes 0.01/nx ≈ 0.01 from the axis
                             // Ellipse frame of this section (center = plane ∩ axis, major along the
                             // in-plane steepest direction = normalize(â − (n·â)n)).
    let cz = -plane_d / n_dot_a; // z where the axis meets the plane
    let center = p(0.0, 0.0, cz);
    let maj = normalize3([-n_dot_a * nx, 0.0, 1.0 - n_dot_a * n_dot_a]);
    let major_axis = Vector3::new(maj[0], maj[1], maj[2]);
    let er = EllipseReloc {
        axis_point: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
        plane_n,
        plane_d,
        center,
        normal: plane_n,
        major_axis,
        major_radius: r / n_dot_a,
        minor_radius: r,
        second_cyl: None,
    };
    // Exact curve point at azimuth θ = π/2: (0, r, z(θ)) with
    // z = −(d + nx·r·cosθ)/n_dot_a; displace it AZIMUTHALLY by 1e-4.
    let theta = std::f64::consts::FRAC_PI_2;
    let exact = p(
        r * theta.cos(),
        r * theta.sin(),
        -(plane_d + nx * r * theta.cos()) / n_dot_a,
    );
    let delta = 1.0e-4_f64;
    let displaced = p(exact.x() - delta, exact.y(), exact.z());
    let rho = ellipse_residual(displaced, &er);
    assert!(
        rho < 2.0 * delta,
        "fixture: residual is O(delta), got {rho:.3e}"
    );
    // The azimuth projection slides ~delta/n_dot_a along the curve — the
    // documented mechanism-2 defect magnitude.
    let (az_proj, _) =
        project_onto_ellipse_via_cylinder(displaced, &er).expect("azimuth projection");
    let az_move = {
        let a = az_proj.as_array();
        let b = displaced.as_array();
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    assert!(
        az_move > 50.0 * delta,
        "fixture: the azimuth slide is macro (≈100·delta), got {az_move:.3e}"
    );
    // The nearest-point projection stays local and lands ON the ellipse.
    let (near_proj, t) =
        project_onto_ellipse_nearest(displaced, &er).expect("nearest projection converges");
    let near_move = {
        let a = near_proj.as_array();
        let b = displaced.as_array();
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    assert!(
        near_move < 4.0 * delta,
        "I6: nearest-point move is a small multiple of the residual, got {near_move:.3e}"
    );
    assert!(
        ellipse_residual(near_proj, &er) < 1.0e-9,
        "the nearest projection lands on the exact ellipse"
    );
    // The returned parameter matches the projected point's own frame angle.
    let t_check = ellipse_param(
        near_proj,
        er.center,
        er.normal,
        er.major_axis,
        er.major_radius,
        er.minor_radius,
    );
    assert!(
        (t - t_check).abs() < 1.0e-9,
        "returned parameter is the projected point's frame angle"
    );
    // Degenerate seed: the ellipse CENTER has no local nearest point within
    // any band — the projection lands a macro distance away (≥ b), which the
    // relocation loop's R3 gate must reject.
    if let Ok((c_proj, _)) = project_onto_ellipse_nearest(center, &er) {
        let c_move = {
            let a = c_proj.as_array();
            let b = center.as_array();
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        assert!(
            c_move >= 0.9 * r,
            "center projection moves ~the minor radius (R3 territory)"
        );
    }
}

#[test]
pub(crate) fn s453c_surface_normal_at_canonical() {
    let n = surface_normal_at(
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 2.0),
            d: 1.0,
        },
        p(5.0, 5.0, 5.0),
    )
    .expect("plane normal");
    assert!((n[2] - 1.0).abs() < 1e-15, "plane normal unit-normalized");

    let n = surface_normal_at(
        Surface::Cylinder {
            axis_point: p(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        },
        p(2.0, 0.0, 7.0),
    )
    .expect("cylinder normal");
    assert!((n[0] - 1.0).abs() < 1e-15 && n[2].abs() < 1e-15);
    assert!(
        surface_normal_at(
            Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
            p(0.0, 0.0, 3.0),
        )
        .is_none(),
        "on-axis point has no radial direction"
    );

    let n = surface_normal_at(
        Surface::Sphere {
            center: p(1.0, 0.0, 0.0),
            radius: 5.0,
        },
        p(1.0, 3.0, 0.0),
    )
    .expect("sphere normal");
    assert!((n[1] - 1.0).abs() < 1e-15);

    // 45° cone: at a lateral point the normal is perpendicular to the
    // ruling direction and tilted 45° from the axis.
    let n = surface_normal_at(
        Surface::Cone {
            apex: p(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        },
        p(1.0, 0.0, 1.0),
    )
    .expect("cone normal");
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert!((n[0] - s).abs() < 1e-12 && (n[2] + s).abs() < 1e-12);
}

// Spec §3b: §4.4.1(b) merge survivor ranking — junction > conic endpoint
// > plain vertex; equal ranks keep the lower-index rule.
#[test]
pub(crate) fn s453_merge_survivor_prefers_exact_vertex() {
    use std::collections::BTreeSet;
    let junction: BTreeSet<u32> = [15u32].into_iter().collect();
    let conic: BTreeSet<u32> = [15u32, 20u32].into_iter().collect();

    // Conic endpoint (higher index) survives over a plain vertex — the
    // R0091 configuration, in BOTH argument orders.
    assert_eq!(
        sub_feature_merge_direction(&junction, &conic, 8, 20),
        (8, 20)
    );
    assert_eq!(
        sub_feature_merge_direction(&junction, &conic, 20, 8),
        (8, 20)
    );

    // Junction survives over a plain single-curve conic endpoint.
    assert_eq!(
        sub_feature_merge_direction(&junction, &conic, 20, 15),
        (20, 15)
    );
    assert_eq!(
        sub_feature_merge_direction(&junction, &conic, 15, 20),
        (20, 15)
    );

    // Equal rank (both plain): lower index survives — byte-identical to
    // the pre-fix behavior.
    assert_eq!(sub_feature_merge_direction(&junction, &conic, 4, 9), (9, 4));
    assert_eq!(sub_feature_merge_direction(&junction, &conic, 9, 4), (9, 4));
}

#[test]
pub(crate) fn n3_degenerate_tangent_is_reversal() {
    let mesh = Mesh::new(
        vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.5, 0.0, 0.0)],
        vec![],
    );
    // Spec §3c per-site eligibility: p_r is a §4.5.3 site only when both
    // incident edges are intersection edges — give both a Circle entry on
    // the SAME curve (the original N3 fixture predates the site guard).
    let circle = Curve::Circle {
        center: p(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
        std::collections::BTreeMap::new();
    curves.insert((0, 1), circle);
    curves.insert((1, 2), circle);
    let lo = std::f64::consts::FRAC_PI_4;
    let hi = 3.0 * std::f64::consts::FRAC_PI_4;
    let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    assert!(
        is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
        "a 180° U-turn (degenerate t̃, Yang §4.5.3 collinear case) must be \
             detected as a reversal, not treated as healthy"
    );
}

/// Spec `kv15b_mint_site_subresolution_collapse` I1b, generalized to ALL
/// surfaces (2026-08-19, R0047 anchor): when a sub-resolution intersection
/// pair joins a certified plane∩cone∩cone crease junction J (3 carried
/// surfaces) with its cone∩plane interior neighbour S (2 surfaces), the
/// min-index survivor S keeps its INDEX but adopts J's COORDINATES — so the
/// merged vertex stays on BOTH cones' section ellipses. Under the planar-only
/// count (1 plane each → tie) the survivor kept its own position and the
/// emitted vertex sat 1.4e-9 off cone-2's ellipse (kernel-v2 "output
/// ellipse-arc endpoint does not lie on its ellipse"). RED under the
/// planar-only rule, GREEN under the surface-incidence rule.
#[test]
pub(crate) fn kv15b_i1b_adopts_surface_incidence_richer_junction_coordinates() {
    // A: a planar z=0 face (triangle loop).
    let a = {
        let verts = vec![
            BRepVertex {
                point: p(-2.0, -2.0, 0.0),
            },
            BRepVertex {
                point: p(2.0, -2.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 2.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("planar A")
    };
    // B: two NON-coaxial cone faces (each a frustum lateral with its own
    // rim circles), both passing through J = (0.5, 0, 0) at z = 0.
    //   cone-1: apex (0,0,-1), +z, tan α1 = 0.5
    //   cone-2: apex (0.3,0.1,-2), +z, tan α2 = √0.05 / 2
    let j = p(0.5, 0.0, 0.0);
    let cone1 = Surface::Cone {
        apex: p(0.0, 0.0, -1.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.5f64.atan(),
    };
    let cone2 = Surface::Cone {
        apex: p(0.3, 0.1, -2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: (0.05f64.sqrt() / 2.0).atan(),
    };
    let b = {
        let z = Vector3::new(0.0, 0.0, 1.0);
        let nz = Vector3::new(0.0, 0.0, -1.0);
        let r1 = |h: f64| h * 0.5; // cone-1 radius at height h above its apex
        let r2 = |h: f64| h * (0.05f64.sqrt() / 2.0);
        let verts = vec![
            BRepVertex {
                point: p(r1(0.5), 0.0, -0.5),
            },
            BRepVertex {
                point: p(r1(1.5), 0.0, 0.5),
            },
            BRepVertex {
                point: p(0.3 + r2(1.5), 0.1, -0.5),
            },
            BRepVertex {
                point: p(0.3 + r2(2.5), 0.1, 0.5),
            },
        ];
        let circ = |c: Point3, n: Vector3, r: f64| Curve::Circle {
            center: c,
            normal: n,
            radius: r,
        };
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: circ(p(0.0, 0.0, -0.5), z, r1(0.5)),
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: circ(p(0.0, 0.0, 0.5), nz, r1(1.5)),
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 2,
                curve: circ(p(0.3, 0.1, -0.5), z, r2(1.5)),
            },
            BRepEdge {
                start: 3,
                end: 3,
                curve: circ(p(0.3, 0.1, 0.5), nz, r2(2.5)),
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: cone1,
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: cone2,
                outer_loop: vec![3, 5, 4, 5],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("two-cone B")
    };
    // Certify the fixture: J is on the plane and on BOTH cones; S (J rotated
    // 1e-7 rad about cone-1's axis) is on the plane and cone-1 only, 5e-8
    // from J — a sub-resolution intersection pair.
    let theta = 1e-7f64;
    let s = p(0.5 * theta.cos(), 0.5 * theta.sin(), 0.0);
    let on = |surf: Surface, q: Point3| {
        surface_distance_and_normal(surf, q.as_array())
            .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(q.as_array(), surf))
    };
    assert!(on(cone1, j) && on(cone2, j) && on(cone1, s) && !on(cone2, s));
    let dist = {
        let (x, y) = (s.as_array(), j.as_array());
        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
    };
    assert!(
        dist > 1e-9 && dist < cad_primitives::TAU_MODEL,
        "dist {dist:e}"
    );

    // Mesh: 0 = S (min index ⇒ topological survivor), 1 = J.
    let mut mesh = Mesh::new(
        vec![s, j, p(1.0, 0.0, 0.0), p(0.5, 0.0, -1.0), p(0.5, 0.2, -1.0)],
        vec![[0, 1, 2], [1, 0, 3], [1, 2, 4]],
    );
    let att = |input: InputId, face: u32| Some(TriangleAttribution { input, face });
    let mut attr = vec![att(InputId::A, 0), att(InputId::B, 0), att(InputId::B, 1)];
    let map = kv15b_map(&[(0, 1)]);
    assert!(collapse_subresolution_intersection_segments(
        &mut mesh, &mut attr, &map, &a, &b
    ));
    assert_eq!(
        mesh.verts[0], j,
        "the survivor must ADOPT the surface-incidence-richer junction's coordinates \
         (3 surfaces vs 2), not keep its own (planar-only count was a 1–1 tie)"
    );
    assert!(on(cone1, mesh.verts[0]) && on(cone2, mesh.verts[0]));
}

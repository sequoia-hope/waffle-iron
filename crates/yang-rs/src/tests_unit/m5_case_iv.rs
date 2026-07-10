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
        &mut mesh, &mut attr, &map
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
        &mut mesh, &mut attr, &map
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
        &mut mesh, &mut attr, &map
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
        &mut mesh, &mut attr, &map
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
        &mut mesh, &mut attr, &map
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

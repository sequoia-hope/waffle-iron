//! Stage-4 §4.4.1 boundary-curve relocation, inc-1 primitive
//! (spec `specs/yang_s4_boundary_curve_relocation.md` §6).
//!
//! The five oracles the spec names, built on the MEASURED geometry of the two
//! known instances rather than on invented numbers: the n2 I1 fixture's rim
//! (R = 2.1339062731488812e-4, base N = 37 ⇒ a 360/37 = 9.7297° span) and the
//! R0063 sub-class (a vertex perturbed OFF the chord, not a pristine chord
//! point).

use super::*;
use crate::geom::Curve;
use crate::stage4_boundary_curve::{
    boundary_relocation_for_vertex, plan_boundary_relocations, project_onto_curve,
};
use cad_primitives::{Point3, Vector3, TAU_WORK};

/// R0072's cylinder radius — the real scale the defect was measured at.
const R: f64 = 2.1339062731488812e-4;
/// 360/37: the boosted-rim span whose chord carries the defective vertex.
const SPAN_DEG: f64 = 360.0 / 37.0;

fn unit_z_circle() -> Curve {
    Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    }
}

fn on_circle(deg: f64) -> Point3 {
    let t = deg.to_radians();
    Point3::new(R * t.cos(), R * t.sin(), 0.0)
}

fn dist(a: Point3, b: Point3) -> f64 {
    let (x, y) = (a.as_array(), b.as_array());
    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// The Stage-1 chord bound for this rim: the max sagitta of a `SPAN_DEG` chord.
/// This is the bound Stage 1 already guarantees — NOT a widened tolerance.
fn chord_bound() -> f64 {
    R * (1.0 - (SPAN_DEG / 2.0).to_radians().cos())
}

/// A point at parameter `t` along the chord joining two on-circle rim samples —
/// the pristine artifact (n2 `v6`, measured EXACTLY on its chord, perp 7.14e-22).
fn chord_point(deg_a: f64, deg_b: f64, t: f64) -> Point3 {
    let (a, b) = (on_circle(deg_a).as_array(), on_circle(deg_b).as_array());
    Point3::new(
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    )
}

// -------------------------------------------------------------------------
// (i) a pristine chord-position vertex relocates EXACTLY onto the circle
// -------------------------------------------------------------------------

#[test]
fn s4bc_i_chord_vertex_relocates_onto_the_circle() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.5); // the chord midpoint: max sagitta
    let resid = (p.as_array()[0].hypot(p.as_array()[1]) - R).abs();
    assert!(
        resid > 1e-9 * (1.0 + R),
        "fixture must actually be off-band before the fix (resid {resid:.3e})"
    );

    let q = boundary_relocation_for_vertex(7, p, &curve, chord_bound())
        .expect("within the chord bound ⇒ never a STOP")
        .expect("an off-curve chord vertex must be relocated");

    let qa = q.as_array();
    let radial = qa[0].hypot(qa[1]);
    assert!(
        (radial - R).abs() <= TAU_WORK * (1.0 + R),
        "relocated vertex must land ON the analytic circle, got residual {:.3e}",
        (radial - R).abs()
    );
    // §4.4.1 relocation moves ONLY off the chord — it must not slide the vertex
    // around the rim, so its azimuth is preserved.
    assert!(
        (qa[1].atan2(qa[0]) - p.as_array()[1].atan2(p.as_array()[0])).abs() <= 1e-15,
        "projection must preserve azimuth (radial projection, not a re-sample)"
    );
}

// -------------------------------------------------------------------------
// (ii) the R0063 sub-class: perturbed OFF the chord, still relocates
// -------------------------------------------------------------------------

#[test]
fn s4bc_ii_vertex_perturbed_off_the_chord_still_relocates() {
    let curve = unit_z_circle();
    // R0063's vertex sits 0.77 % of the sagitta off its chord — the fix must
    // project from wherever the vertex is, not assume a pristine chord point.
    let base = chord_point(0.0, SPAN_DEG, 0.475239168);
    let nudge = 0.0077 * chord_bound();
    let p = Point3::new(base.as_array()[0], base.as_array()[1] + nudge, 0.0);

    let q = boundary_relocation_for_vertex(9, p, &curve, chord_bound())
        .expect("still within the chord bound")
        .expect("a perturbed chord vertex must also be relocated");
    let qa = q.as_array();
    assert!(
        (qa[0].hypot(qa[1]) - R).abs() <= TAU_WORK * (1.0 + R),
        "perturbed vertex must also land exactly on the circle"
    );
}

// -------------------------------------------------------------------------
// (iii) a junction vertex is NOT moved
// -------------------------------------------------------------------------

#[test]
fn s4bc_iii_cross_curve_junction_vertex_is_never_moved() {
    let curve = unit_z_circle();
    let mut mesh = Mesh::empty();
    // v0/v1 exact rim samples; v2 the chord artifact; v3 an A×B junction that
    // also happens to sit off the circle — it must survive untouched, because
    // it is required to lie on BOTH curves (I1's `v5` is the real instance).
    mesh.verts.push(on_circle(0.0));
    mesh.verts.push(on_circle(SPAN_DEG));
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.5));
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.25));

    let mut rim: std::collections::BTreeMap<(u32, u32), Curve> = Default::default();
    rim.insert((0, 2), curve);
    rim.insert((2, 1), curve);
    rim.insert((0, 3), curve);

    let mut excluded: std::collections::BTreeSet<u32> = Default::default();
    excluded.insert(3);

    let moves =
        plan_boundary_relocations(&mesh, &rim, &Default::default(), &excluded, chord_bound());
    let moved: Vec<u32> = moves.iter().map(|(v, _)| *v).collect();
    assert_eq!(moved, vec![2], "only the unclaimed chord vertex may move");
}

// -------------------------------------------------------------------------
// (iv) beyond the chord bound ⇒ LOUD STOP, never a snap
// -------------------------------------------------------------------------

#[test]
fn s4bc_iv_beyond_chord_bound_is_a_loud_stop() {
    let curve = unit_z_circle();
    // Well inside the rim — a real defect of a different class, which this pass
    // must refuse rather than quietly drag onto the circle.
    let p = Point3::new(R * 0.5, 0.0, 0.0);
    let err = boundary_relocation_for_vertex(11, p, &curve, chord_bound())
        .expect_err("a displacement beyond the chord bound must STOP");
    assert!(
        matches!(
            err,
            YangError::Stage4RegionInvalid {
                vertex: 11,
                reason: Stage4InvalidReason::LocalRefinementRequired
            }
        ),
        "expected the loud Stage-4 STOP, got {err:?}"
    );
}

// -------------------------------------------------------------------------
// (v) an already-exact vertex is a bit-exact no-op (byte-identical corpus)
// -------------------------------------------------------------------------

#[test]
fn s4bc_v_on_curve_vertex_is_a_no_op() {
    let curve = unit_z_circle();
    for deg in [0.0, SPAN_DEG, 90.0, -137.5] {
        let p = on_circle(deg);
        let got = boundary_relocation_for_vertex(3, p, &curve, chord_bound())
            .expect("an on-curve vertex is never a STOP");
        assert!(
            got.is_none(),
            "a vertex already on the curve must not be rewritten (deg {deg})"
        );
    }
}

/// The projection itself is exact and idempotent: projecting a projected point
/// returns it unchanged, so a second Stage-4 pass cannot drift the mesh.
#[test]
fn s4bc_projection_is_idempotent() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.31);
    let q = project_onto_curve(p, &curve).expect("circle projection exists");
    let q2 = project_onto_curve(q, &curve).expect("re-projection exists");
    assert!(
        dist(q, q2) <= TAU_WORK * (1.0 + R),
        "projection must be idempotent, drifted {:.3e}",
        dist(q, q2)
    );
}

/// A point ON the axis has no unique closest point — the pass must skip it,
/// not guess an azimuth.
#[test]
fn s4bc_axis_point_is_skipped_not_guessed() {
    let curve = unit_z_circle();
    assert!(project_onto_curve(Point3::new(0.0, 0.0, 0.0), &curve).is_none());
    assert!(project_onto_curve(Point3::new(0.0, 0.0, 5.0), &curve).is_none());
}

/// Out-of-scope curve kinds are skipped, never snapped (spec §4 step 1).
#[test]
fn s4bc_non_circle_curve_is_skipped() {
    let p = Point3::new(1.0, 1.0, 1.0);
    assert!(project_onto_curve(p, &Curve::LineSegment).is_none());
}

// =========================================================================
// inc-2 CENSUS (spec §8): do the defective rim vertices still carry a
// `TessellationSource::BRepEdge { edge, t }` tag in the OUTPUT mesh?
//
// Arm (a) of inc-2 (exact re-evaluation at the recorded parameter) is only
// buildable for tagged vertices; untagged ones need arm (b), inc-1's guarded
// projection. Measure before building — do not assume.
// =========================================================================

/// Reproduces the n2 I1 geometry in-crate (the integration fixture lives in
/// `tests/n2_junction_cluster.rs`, which cannot see `pub(crate)` items).
fn i1_operands() -> (BRep, BRep) {
    const H: f64 = 2.0891191078398327e-4;
    const DELTA: f64 = 1.607e-6;
    const BOX_HALF_Y: f64 = 1.0e-4;
    const BOX_W: f64 = 2.0e-4;
    const H_B: f64 = 7.657508571136625e-5;
    let a = super::n2_junction::rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], R, H);
    let x_lo = R - DELTA;
    let b = super::n2_junction::rj_box([x_lo, -BOX_HALF_Y, 0.0], [x_lo + BOX_W, BOX_HALF_Y, H_B]);
    (a, b)
}

#[test]
fn s4bc_census_tessellation_source_of_off_curve_rim_vertices() {
    let (a, b) = i1_operands();
    let backend = match crate::native_backend() {
        Some(be) => be,
        None => return,
    };
    // The defect only appears with the #195 rim boost on; without it the
    // fixture is clean and the census is vacuous (which is itself the control).
    let Ok(out) = crate::boolean(&a, &b, cad_primitives::BoolOp::Union, &backend) else {
        return;
    };

    let mut off: Vec<(u32, f64, TessellationSource)> = Vec::new();
    let mut tagged_rim = 0usize;
    let mut census: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for f in out.faces() {
        let Surface::Cylinder { radius, .. } = f.surface else {
            continue;
        };
        for &e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let e = &out.edges()[e_idx as usize];
            for v in [e.start, e.end] {
                let p = out.vertices()[v as usize].point.as_array();
                let src = out.tessellation.lookup(v);
                let kind = match src {
                    TessellationSource::BRepVertex(_) => "BRepVertex",
                    TessellationSource::BRepEdge { .. } => "BRepEdge",
                    TessellationSource::BRepFace { .. } => "BRepFace",
                    TessellationSource::Intersection => "Intersection",
                    TessellationSource::Unknown => "Unknown",
                };
                *census.entry(kind).or_default() += 1;
                if matches!(src, TessellationSource::BRepEdge { .. }) {
                    tagged_rim += 1;
                }
                let resid = (p[0].hypot(p[1]) - radius).abs();
                if resid > 1e-9 * (1.0 + radius) {
                    off.push((v, resid, src));
                }
            }
        }
    }
    eprintln!(
        "[s4bc-census] cylinder-loop vertex sources: {census:?} (BRepEdge-tagged {tagged_rim})"
    );
    for (v, resid, src) in &off {
        eprintln!("[s4bc-census] OFF-CURVE v={v} resid={resid:.6e} source={src:?}");
    }
    // No assertion on the count: this test is a CENSUS, and it must stay green
    // in both gate states (the fixture is clean with the #195 arm off). What it
    // pins is that the lookup is total — every cylinder-loop vertex has a
    // source — so inc-2 can branch on it.
    assert!(
        census.values().sum::<usize>() > 0,
        "the union must retain at least one cylinder face to census"
    );
}

/// inc-2 (MEASURED): a same-input `Cylinder`+`Plane` adjacency does NOT imply
/// the shared edge lies on cylinder∩plane — after a boolean, a cylinder patch
/// can be adjacent to a plane patch along a trimming boundary nowhere near the
/// rim (`m8_nary_tessellated_overlay::flush_pocket_subtract_and_union_partition`
/// STOPped when the candidate curve was trusted blindly). Membership is
/// therefore VERIFIED per edge: if either endpoint is beyond the bound, the
/// whole edge is abandoned — including its OTHER, in-band endpoint, which must
/// NOT be quietly snapped to a curve it may not belong to.
#[test]
fn s4bc_vi_edge_with_an_out_of_band_endpoint_is_abandoned_whole() {
    let curve = unit_z_circle();
    let mut mesh = Mesh::empty();
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.5)); // v0: in-band, off-curve
    mesh.verts.push(Point3::new(R * 0.5, 0.0, 0.0)); // v1: far off — not this rim
    mesh.verts.push(chord_point(0.0, SPAN_DEG, 0.5)); // v2: in-band, off-curve
    mesh.verts.push(on_circle(SPAN_DEG)); // v3: exactly on

    let mut rim: std::collections::BTreeMap<(u32, u32), Curve> = Default::default();
    rim.insert((0, 1), curve); // rejected edge
    rim.insert((2, 3), curve); // accepted edge

    let moves = plan_boundary_relocations(
        &mesh,
        &rim,
        &Default::default(),
        &Default::default(),
        chord_bound(),
    );
    let moved: Vec<u32> = moves.iter().map(|(v, _)| *v).collect();
    assert_eq!(
        moved,
        vec![2],
        "only the verified edge may relocate; v0 rides a rejected edge and must stay put"
    );
}

// =========================================================================
// inc-3 — the Fig-11 point q as a TRIPLE POINT (spec §11)
// =========================================================================

use crate::geom::Surface;
use crate::stage4_boundary_curve::{circle_plane_nearest_root, satisfies_all_surfaces};

/// The solve is exact and picks the root NEAREST the current seat — the far
/// root is a real intersection too, and choosing it would teleport the vertex
/// across the rim.
#[test]
fn s4bc_iii_triple_point_picks_the_nearest_root() {
    let curve = unit_z_circle();
    // Plane x = R/2 cuts the unit-z circle at azimuth ±60°.
    let n = Vector3::new(1.0, 0.0, 0.0);
    let d = -R / 2.0;
    for (seed_deg, want_deg) in [(50.0, 60.0), (70.0, 60.0), (-50.0, -60.0), (-160.0, -60.0)] {
        let q = circle_plane_nearest_root(&curve, n, d, on_circle(seed_deg))
            .expect("the plane cuts this circle");
        let qa = q.as_array();
        assert!(
            (qa[0].hypot(qa[1]) - R).abs() <= TAU_WORK * (1.0 + R),
            "root must be ON the circle"
        );
        assert!(
            (qa[0] - R / 2.0).abs() <= TAU_WORK * (1.0 + R),
            "root must be ON the plane"
        );
        let got = qa[1].atan2(qa[0]).to_degrees();
        assert!(
            (got - want_deg).abs() < 1e-6,
            "seed {seed_deg}° should pick {want_deg}°, got {got}°"
        );
    }
}

/// A rim that never reaches the plane yields NO claim — the pass must not
/// invent a nearest point.
#[test]
fn s4bc_iii_triple_point_skips_when_the_rim_never_reaches_the_plane() {
    let curve = unit_z_circle();
    // x = 2R is outside the circle entirely.
    assert!(circle_plane_nearest_root(
        &curve,
        Vector3::new(1.0, 0.0, 0.0),
        -2.0 * R,
        on_circle(0.0)
    )
    .is_none());
    // A plane parallel to the circle's own plane is ambiguous (none, or the
    // whole circle) — also no claim.
    assert!(
        circle_plane_nearest_root(&curve, Vector3::new(0.0, 0.0, 1.0), 0.0, on_circle(0.0))
            .is_none()
    );
}

/// The certificate REFUSES a point that misses any of its surfaces — this is
/// what replaces a displacement band for this class.
#[test]
fn s4bc_iii_certificate_refuses_a_point_off_any_surface() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    };
    let cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let cut = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -R / 2.0,
    };
    // The true triple point: on the cylinder, on z=0, on x=R/2.
    let good = Point3::new(R / 2.0, (R * R - R * R / 4.0).sqrt(), 0.0);
    assert!(satisfies_all_surfaces(good, &[cyl, cap, cut]));
    // Same azimuth but pulled inside the cylinder — must be refused.
    let bad = Point3::new(R / 2.0 * 0.9, (R * R - R * R / 4.0).sqrt() * 0.9, 0.0);
    assert!(!satisfies_all_surfaces(bad, &[cyl, cap, cut]));
    // On the cylinder and cap but off the cutting plane — refused.
    let off_cut = on_circle(10.0);
    assert!(!satisfies_all_surfaces(off_cut, &[cyl, cap, cut]));
}

// =========================================================================
// §20 census — the flush-junction duplicate-plane identification
// =========================================================================

/// F0067's measured pair: the rim's own bottom cap and the OTHER operand's top
/// cap, 5e-16 apart with OPPOSITE normals. `let [other] = ...` counted these as
/// two constraints; they are one surface.
#[test]
fn s4bc_dup_planes_flush_junction_pair_is_identified() {
    let z = 1.7518978673859231_f64;
    let own_cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -z,
    };
    // The other operand's cap: outward the other way, and 5e-16 offset.
    let other_cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 1.7518978673859236,
    };
    assert!(crate::stage4_boundary_curve::planes_are_duplicates(
        own_cap, other_cap
    ));
    assert!(crate::stage4_boundary_curve::planes_are_duplicates(
        other_cap, own_cap
    ));
}

/// The real flank plane at that same junction must NOT be identified away — it
/// is the constraint inc-2 dropped, and the census exists to count it.
#[test]
fn s4bc_dup_planes_refuses_a_genuinely_different_plane() {
    let cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.7518978673859231,
    };
    let flank = Surface::Plane {
        normal: Vector3::new(0.856701, -0.515813, 0.0),
        d: -0.0069008,
    };
    assert!(!crate::stage4_boundary_curve::planes_are_duplicates(
        cap, flank
    ));
    // Parallel but genuinely offset (1e-9 ≫ TAU_WORK) is also NOT a duplicate.
    let shifted = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.7518978673859231 - 1e-9,
    };
    assert!(!crate::stage4_boundary_curve::planes_are_duplicates(
        cap, shifted
    ));
}

/// A non-plane pair is never a duplicate — the identification is measured for
/// planes only and must not be extrapolated to a kind nothing has exercised.
#[test]
fn s4bc_dup_planes_non_plane_pairs_are_never_duplicates() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    assert!(!crate::stage4_boundary_curve::planes_are_duplicates(
        cyl, cyl
    ));
    assert!(!crate::stage4_boundary_curve::planes_are_duplicates(
        cyl, plane
    ));
    // A zero-length normal cannot be compared: refuse, never divide by zero.
    let degenerate = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 0.0),
        d: 0.0,
    };
    assert!(!crate::stage4_boundary_curve::planes_are_duplicates(
        degenerate, plane
    ));
}

// =========================================================================
// inc-6 (spec §20) — seating a rim vertex that carries an UNCONSUMED surface
// =========================================================================

use crate::stage4_boundary_curve::{seat_against_unconsumed, unconsumed_surfaces_for_vertex};

/// A plane `x = R·cos(deg)`, whose two roots on the unit-z rim are `±deg`.
fn cutting_plane_at(deg: f64) -> Surface {
    Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -R * deg.to_radians().cos(),
    }
}

/// No unconsumed surface ⇒ the rim projection stands, bit-for-bit. This is the
/// measured majority (89 of inc-2's 101 corpus snaps) and must not move.
#[test]
fn s4bc_vi_no_unconsumed_surface_keeps_the_projection_bit_exact() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.5);
    let q = project_onto_curve(p, &curve).expect("chord point projects");
    let seat = seat_against_unconsumed(p, q, &curve, &[], chord_bound())
        .expect("no unconsumed surface ⇒ the projection is the answer");
    assert_eq!(
        seat.as_array(),
        q.as_array(),
        "the empty case must be bit-identical, not merely close"
    );
}

/// ONE unconsumed plane ⇒ the seat is the `Circle ∩ Plane` certificate: on the
/// rim AND on the plane the projection would have dropped. This is F0067's
/// class, at F0067's scale (the seat differs from the projection by 1.9e-7,
/// three orders above `TAU_WORK`).
#[test]
fn s4bc_vi_one_unconsumed_plane_seats_at_the_certificate() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.25);
    let q = project_onto_curve(p, &curve).expect("chord point projects");
    // A plane cutting the rim 0.05° past the projection's angle — inside the
    // chord budget, but a DIFFERENT point.
    let target_deg = 2.4780438484538493;
    let plane = cutting_plane_at(target_deg);
    let seat = seat_against_unconsumed(p, q, &curve, &[plane], chord_bound())
        .expect("root within the chord bound ⇒ seated");

    assert!(
        dist(seat, q) > TAU_WORK,
        "the certificate must actually differ from the projection ({:.3e})",
        dist(seat, q)
    );
    // ON the rim.
    let sa = seat.as_array();
    assert!(
        (sa[0].hypot(sa[1]) - R).abs() <= TAU_WORK * (1.0 + R),
        "seat must lie on the rim circle"
    );
    // ON the plane the projection would have dropped — the whole point.
    let Surface::Plane { normal, d } = plane else {
        unreachable!()
    };
    let n = normal.as_array();
    let resid = n[0] * sa[0] + n[1] * sa[1] + n[2] * sa[2] + d;
    assert!(
        resid.abs() <= TAU_WORK * (1.0 + R),
        "seat must satisfy the unconsumed plane (resid {resid:.3e})"
    );
}

/// The certificate is still subject to the pass's `bound`: a root further from
/// the vertex than the owner's Stage-1 chord guarantee is not this class, and
/// the pass makes NO claim rather than dragging the vertex there.
#[test]
fn s4bc_vi_certificate_beyond_the_chord_bound_makes_no_claim() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.25);
    let q = project_onto_curve(p, &curve).expect("chord point projects");
    // 0.3° past the projection: 1.26e-6 away, against a 7.69e-7 bound.
    let plane = cutting_plane_at(2.7280438484538493);
    assert!(seat_against_unconsumed(p, q, &curve, &[plane], chord_bound()).is_none());
}

/// A non-plane unconsumed surface, or more than one, has no closed-form seat
/// here. The pass must make no claim — never fall back to the rim projection,
/// which is the measured F0067 defect.
#[test]
fn s4bc_vi_underivable_seats_make_no_claim() {
    let curve = unit_z_circle();
    let p = chord_point(0.0, SPAN_DEG, 0.25);
    let q = project_onto_curve(p, &curve).expect("chord point projects");
    let sphere = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: R,
    };
    assert!(
        seat_against_unconsumed(p, q, &curve, &[sphere], chord_bound()).is_none(),
        "a non-plane unconsumed surface has no closed-form seat in this increment"
    );
    let two = [cutting_plane_at(2.478), cutting_plane_at(2.6)];
    assert!(
        seat_against_unconsumed(p, q, &curve, &two, chord_bound()).is_none(),
        "an over-determined seat must refuse, not pick one"
    );
}

/// The unconsumed set: own surfaces are consumed, a coplanar duplicate of an own
/// surface is NOT a constraint (F0067's flush cap), and a genuine third surface
/// is reported.
#[test]
fn s4bc_vi_unconsumed_set_filters_own_and_duplicate_surfaces() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    };
    let cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    // The other operand's cap at the same flush junction: opposite normal, 5e-16
    // offset — an identical surface wearing a different label.
    let dup_cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 5e-16,
    };
    let flank = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -R / 2.0,
    };
    let own = [(InputId::B, cyl), (InputId::B, cap)];
    let mut incidence: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        Default::default();
    incidence.insert((0, 1), vec![(InputId::B, cyl), (InputId::B, cap)]);
    incidence.insert((1, 2), vec![(InputId::A, dup_cap), (InputId::A, flank)]);
    // An edge not touching vertex 1 must contribute nothing.
    incidence.insert(
        (3, 4),
        vec![(
            InputId::A,
            Surface::Sphere {
                center: Point3::new(0.0, 0.0, 0.0),
                radius: 1.0,
            },
        )],
    );

    let got = unconsumed_surfaces_for_vertex(1, &own, &incidence);
    assert_eq!(
        got,
        vec![flank],
        "only the genuine third surface survives: own pair consumed, flush \
         duplicate identified away, distant edge irrelevant"
    );
}

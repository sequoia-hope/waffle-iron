//! PR-KV6a RED oracles — revolve: polygon profiles with axis-aligned edges
//! swept about an in-plane axis, partial angles (0, 2π) AND full 360°.
//!
//! ## Scope (corpus-derived + user decision 2026-06-11)
//!
//! The 38 assay revolve cases generate: axis = the sketch-plane x-basis,
//! offset 1.5×profile_size along the in-plane perpendicular (profile
//! strictly on one side), rectangle profiles whose edges are exactly
//! parallel/perpendicular to the axis, partial angles 30–360° exclusive.
//! Full 360° is additionally in scope because the app's revolve dialog
//! defaults to it. Out of scope, typed and loud: oblique edges (cones —
//! KV6c), circle profiles (torus — KV6d), holed profiles, axis touching or
//! crossing the profile (an ERROR, not NotSupported — F0073/F0074 pin
//! `expect_rebuild_error`).
//!
//! ## Output vocabulary
//!
//! Partial revolve of an offset rectangle = exactly the KV5b partial-patch
//! vocabulary: 2 planar end caps, 2 partial-cylinder laterals (outer
//! `reversed: false`, inner `reversed: true`), 2 planar annular-sector
//! faces, 4 sweep [`Curve::Arc`] edges (forward traversal CCW around +â).
//! V=2k, E=3k, F=k+2, χ=2 (k = profile edge count).
//!
//! Full 360° of an offset rectangle is a genus-1 washer in the KV5a
//! canonical vocabulary: 2 annular caps (outer circle loop + circle ring),
//! outer + inner full cylinders, 4 full-circle rims; V=4, E=6, F=4, R=2,
//! G=1 ⇒ V−E+F−R = 0 = 2(S−G).
//!
//! ## Oracle groups
//!
//! 1. Topology + validation census (partial + full)
//! 2. Analytic volume: Pappus `V = α·R̄·A = α(r₂²−r₁²)h/2` via
//!    `geom::signed_volume` (tessellation-independent) — bitwise-adjacent
//!    (≤1e-12 rel) for partial; bitwise `9π` for the exact 360 fixture
//! 3. Tessellation: watertight position-paired mesh, positive volume in the
//!    chord band, quadratic convergence, at 90°, 350° (major-arc sectors),
//!    and 360° (annular caps)
//! 4. Rejections: typed, loud, pre-mutation (arena untouched)
//! 5. Determinism: bit-identical arenas and meshes
//! 6. KV6b wall: revolve outputs do NOT enter yang booleans yet (typed)
//! 7. Legacy-trait adapter: revolve_face end-to-end, error mapping
//!    (axis-through-profile must NOT carry the NotSupported marker)

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    geom, revolve, tessellate, tessellate_with_chord_tolerance, to_yang_brep, validate_solid,
    BrepArena, Curve, KernelV2Error, Profile, RenderMesh, RevolveResult, Surface,
};

// =========================================================================
// Fixtures
// =========================================================================

/// Inner radius, outer radius, axial extent of the offset rectangle.
const R1: f64 = 1.0;
const R2: f64 = 2.0;
const H: f64 = 3.0;

/// Rectangle x∈[0,3], y∈[1,2] in the XY plane (CCW), revolved about the
/// world x-axis (in-plane, profile strictly on the +y side).
fn rect_profile() -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, R1),
            Point2::new(H, R1),
            Point2::new(H, R2),
            Point2::new(0.0, R2),
        ],
        vec![],
    )
    .expect("rect profile")
}

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

fn revolve_rect(arena: &mut BrepArena, angle: f64) -> RevolveResult {
    let profile = rect_profile();
    revolve(arena, &profile, AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("revolve({angle}) failed: {e:?}"))
}

/// Pappus: V = α·R̄·A with R̄·A = (r₂²−r₁²)·h/2.
fn pappus_volume(angle: f64) -> f64 {
    angle * (R2 * R2 - R1 * R1) * H / 2.0
}

// =========================================================================
// Shared oracle helpers
// =========================================================================

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Watertightness by POSITION-keyed directed-edge pairing (faces emit
/// duplicated vertices). Shared ARCS agree bitwise (the KV5b twin-symmetric
/// sampling), but full-circle rim junctions are sampled CCW around OPPOSITE
/// axes by their two faces, so they agree only within trig rounding
/// (~1e-16) — keys are therefore quantized at 1e-9 absolute, far below any
/// feature scale and far above the rounding band.
fn assert_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::HashMap;
    let q = |x: f64| (x / 1e-9).round() as i64;
    let key = |i: u32| {
        let k = (i as usize) * 3;
        (
            q(mesh.positions[k]),
            q(mesh.positions[k + 1]),
            q(mesh.positions[k + 2]),
        )
    };
    let mut count: HashMap<_, i64> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue; // degenerate edge keys are caught by the area check
            }
            *count.entry((ka, kb)).or_insert(0) += 1;
            *count.entry((kb, ka)).or_insert(0) -= 1;
        }
    }
    let unpaired = count.values().filter(|&&c| c != 0).count();
    assert_eq!(unpaired, 0, "{what}: {unpaired} unpaired directed edges");
}

fn assert_mesh_sane(mesh: &RenderMesh, what: &str) {
    assert!(!mesh.indices.is_empty(), "{what}: empty mesh");
    for v in &mesh.positions {
        assert!(v.is_finite(), "{what}: non-finite position");
    }
    for chunk in mesh.normals.chunks_exact(3) {
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-9,
            "{what}: non-unit normal {chunk:?}"
        );
    }
}

fn assert_rel_eq(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs();
    assert!(
        err <= rel * expected.abs(),
        "{what}: |{actual} - {expected}| = {err} > {rel} * {}",
        expected.abs()
    );
}

/// Census of typed surfaces: (planes, cylinders, reversed cylinders).
fn surface_census(
    arena: &BrepArena,
    faces: impl IntoIterator<Item = kernel_v2::FaceId>,
) -> (usize, usize, usize) {
    let (mut planes, mut cyls, mut rev) = (0, 0, 0);
    for f in faces {
        match arena.face(f).expect("face").surface {
            Some(Surface::Plane(_)) => planes += 1,
            Some(Surface::Cylinder { reversed, .. }) => {
                cyls += 1;
                if reversed {
                    rev += 1;
                }
            }
            other => panic!("untyped/unknown surface {other:?}"),
        }
    }
    (planes, cyls, rev)
}

fn all_faces(_arena: &BrepArena, r: &RevolveResult) -> Vec<kernel_v2::FaceId> {
    let mut v = vec![r.start_cap, r.end_cap];
    v.extend(r.walls.iter().copied());
    v
}

// =========================================================================
// 1. Topology + validation census
// =========================================================================

#[test]
fn partial_revolve_topology_census() {
    for angle in [PI / 2.0, PI, 1.5 * PI] {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);

        let report = validate_solid(&arena, r.solid)
            .unwrap_or_else(|e| panic!("revolve({angle}) validates: {e:?}"));
        assert_eq!(report.vertices, 8, "V at {angle}");
        assert_eq!(report.edges, 12, "E at {angle}");
        assert_eq!(report.faces, 6, "F at {angle}");
        assert_eq!(report.rings, 0, "R at {angle}");
        assert_eq!(report.shells, 1);
        assert_eq!(report.genus, 0, "partial revolve is genus 0");
        assert_eq!(report.euler_lhs, 2);
        assert_eq!(report.euler_rhs, 2);

        // 4 walls (one per profile edge) + 2 caps; 4 planes + 2 cylinders,
        // exactly one of them the inner cavity-sense wall.
        assert_eq!(r.walls.len(), 4, "one wall per profile edge");
        let (planes, cyls, rev) = surface_census(&arena, all_faces(&arena, &r));
        assert_eq!((planes, cyls, rev), (4, 2, 1), "census at {angle}");

        // Cylinder walls carry the axis and the profile-edge radii.
        for f in &r.walls {
            if let Some(Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
                reversed,
            }) = arena.face(*f).expect("wall").surface
            {
                assert!(
                    radius == R1 || radius == R2,
                    "wall radius {radius} not a profile-edge radius"
                );
                assert_eq!(
                    reversed,
                    radius == R1,
                    "inner wall (r={R1}) is the cavity sense, outer is not"
                );
                assert_eq!(
                    (axis_dir.x, axis_dir.y, axis_dir.z),
                    (1.0, 0.0, 0.0),
                    "cylinder axis is the revolve axis"
                );
                // axis_point on the revolve axis (y = z = 0).
                assert_eq!((axis_point.y(), axis_point.z()), (0.0, 0.0));
            }
        }

        // Sweep arcs: 4 undirected arc edges (8 half-edges), forward
        // traversal CCW around +â = +x̂, twins exactly negated; radii are
        // the profile-vertex radii, centers on the axis.
        let mut arc_hes = 0;
        let mut plus = 0;
        let mut minus = 0;
        for slot in &arena.half_edges {
            let Some(he) = slot else { continue };
            if let Curve::Arc {
                center,
                normal,
                radius,
            } = he.curve
            {
                arc_hes += 1;
                assert!(
                    radius == R1 || radius == R2,
                    "arc radius {radius} not a profile-vertex radius"
                );
                assert_eq!((center.y(), center.z()), (0.0, 0.0), "arc center on axis");
                match (normal.x, normal.y, normal.z) {
                    (1.0, 0.0, 0.0) => plus += 1,
                    (-1.0, 0.0, 0.0) => minus += 1,
                    other => panic!("arc normal {other:?} not ±â"),
                }
            }
        }
        assert_eq!(arc_hes, 8, "8 arc half-edges (4 sweep edges)");
        assert_eq!(
            (plus, minus),
            (4, 4),
            "each arc edge has a +â and a −â side"
        );

        // Cap planes: start cap outward normal opposes the sweep velocity
        // (−ẑ for this fixture); end cap normal = R_x(angle)·(+ẑ).
        let Some(Surface::Plane(p_start)) = arena.face(r.start_cap).expect("start").surface else {
            panic!("start cap not planar");
        };
        assert_eq!(
            (p_start.normal.x, p_start.normal.y, p_start.normal.z),
            (0.0, 0.0, -1.0),
            "start cap outward normal at {angle}"
        );
        let Some(Surface::Plane(p_end)) = arena.face(r.end_cap).expect("end").surface else {
            panic!("end cap not planar");
        };
        let expect = (0.0, -angle.sin(), angle.cos());
        assert!(
            (p_end.normal.x - expect.0).abs() < 1e-12
                && (p_end.normal.y - expect.1).abs() < 1e-12
                && (p_end.normal.z - expect.2).abs() < 1e-12,
            "end cap outward normal at {angle}: got ({}, {}, {}), want {expect:?}",
            p_end.normal.x,
            p_end.normal.y,
            p_end.normal.z
        );
    }
}

#[test]
fn full_revolve_topology_census_genus_one_washer() {
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, 2.0 * PI);

    let report = validate_solid(&arena, r.solid).expect("360° washer validates");
    assert_eq!(report.vertices, 4, "one seam vertex per rim circle");
    assert_eq!(report.edges, 6, "4 rim circles + 2 seam rulings");
    assert_eq!(report.faces, 4, "2 annular caps + 2 cylinders");
    assert_eq!(report.rings, 2, "one ring per annular cap");
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 1, "a washer is a torus topologically");
    assert_eq!(report.euler_lhs, 0);
    assert_eq!(report.euler_rhs, 0);

    let (planes, cyls, rev) = surface_census(&arena, all_faces(&arena, &r));
    assert_eq!(
        (planes, cyls, rev),
        (2, 2, 1),
        "2 caps + outer/inner cylinder"
    );

    // No arcs anywhere: the 360° branch is in the full-circle vocabulary.
    for slot in &arena.half_edges {
        let Some(he) = slot else { continue };
        assert!(
            !matches!(he.curve, Curve::Arc { .. }),
            "full revolve must not contain Arc edges"
        );
    }

    // Caps are the annuli at the axial extremes, normals exactly ∓â.
    let mut cap_normals: Vec<(f64, f64, f64)> = [r.start_cap, r.end_cap]
        .iter()
        .map(|&f| {
            let face = arena.face(f).expect("cap");
            assert_eq!(face.inner_loops.len(), 1, "annular cap has one ring");
            let Some(Surface::Plane(p)) = face.surface else {
                panic!("cap not planar");
            };
            (p.normal.x, p.normal.y, p.normal.z)
        })
        .collect();
    cap_normals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(cap_normals, vec![(-1.0, 0.0, 0.0), (1.0, 0.0, 0.0)]);
}

// =========================================================================
// 2. Analytic volume (tessellation-independent)
// =========================================================================

#[test]
fn partial_volume_matches_pappus_analytically() {
    for angle in [PI / 3.0, PI / 2.0, PI, 1.75 * PI] {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);
        let vol = geom::signed_volume(&arena, r.solid)
            .unwrap_or_else(|e| panic!("analytic volume at {angle}: {e:?}"));
        assert!(vol > 0.0, "outward orientation at {angle}");
        assert_rel_eq(vol, pappus_volume(angle), 1e-12, "Pappus volume");
    }
}

#[test]
fn full_volume_is_exactly_pi_r2sq_minus_r1sq_h() {
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, 2.0 * PI);
    let vol = geom::signed_volume(&arena, r.solid).expect("analytic washer volume");
    // (r₂² − r₁²)·h = 9 exactly; the rational π-coefficient path must
    // surface it bitwise (disk + ring-disk + two canonical laterals).
    assert_eq!(vol, 9.0 * PI, "washer volume == π(r₂²−r₁²)h bitwise");
}

// =========================================================================
// 3. Tessellation
// =========================================================================

#[test]
fn partial_meshes_watertight_with_volume_in_band() {
    // 350° exercises the major-arc annular sector (non-convex, sweep > π).
    for angle in [PI / 2.0, 350.0_f64.to_radians()] {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);
        let mesh =
            tessellate(&arena, r.solid).unwrap_or_else(|e| panic!("tessellate at {angle}: {e:?}"));
        assert_mesh_sane(&mesh, "partial revolve mesh");
        assert_watertight(&mesh, "partial revolve mesh");
        let v = mesh_signed_volume(&mesh);
        assert!(v > 0.0, "positive mesh volume at {angle}");
        // Inscribed chords under-estimate; 2% band at the default chord
        // tolerance is generous and catches gross winding defects.
        assert_rel_eq(v, pappus_volume(angle), 2e-2, "mesh volume band");
    }
}

#[test]
fn full_mesh_watertight_with_annular_caps() {
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, 2.0 * PI);
    let mesh = tessellate(&arena, r.solid).expect("tessellate 360° washer");
    assert_mesh_sane(&mesh, "washer mesh");
    assert_watertight(&mesh, "washer mesh");
    let v = mesh_signed_volume(&mesh);
    assert!(v > 0.0);
    assert_rel_eq(v, 9.0 * PI, 2e-2, "washer mesh volume band");
}

#[test]
fn mesh_volume_converges_quadratically() {
    let angle = 1.25 * PI;
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, angle);
    let analytic = pappus_volume(angle);
    let mut defects = Vec::new();
    for tol in [1e-2, 2.5e-3, 6.25e-4] {
        let mesh =
            tessellate_with_chord_tolerance(&arena, r.solid, tol).expect("tessellate at tol");
        assert_watertight(&mesh, "tightened mesh");
        defects.push((analytic - mesh_signed_volume(&mesh)).abs());
    }
    assert!(
        defects[1] < defects[0] / 3.0 && defects[2] < defects[1] / 3.0,
        "quadratic convergence expected, got {defects:?}"
    );
}

// =========================================================================
// 4. Rejections (typed, loud, pre-mutation)
// =========================================================================

#[test]
fn invalid_angles_rejected_typed() {
    for bad in [0.0, -1.0, 2.0 * PI + 1e-6, f64::NAN, f64::INFINITY] {
        let mut arena = BrepArena::new();
        let profile = rect_profile();
        let err = revolve(&mut arena, &profile, AXIS_O, AXIS_D, bad)
            .expect_err("invalid angle must be rejected");
        assert_eq!(err, KernelV2Error::RevolveInvalidAngle, "angle {bad}");
        assert_eq!(arena, BrepArena::new(), "arena untouched after Err");
    }
}

#[test]
fn axis_out_of_plane_rejected_typed() {
    let profile = rect_profile();
    // Origin off the profile plane.
    let mut arena = BrepArena::new();
    let err = revolve(&mut arena, &profile, Point3::new(0.0, 0.0, 0.5), AXIS_D, PI)
        .expect_err("off-plane axis origin");
    assert_eq!(err, KernelV2Error::RevolveAxisNotInPlane);
    // Direction out of the plane (along the normal).
    let err = revolve(
        &mut arena,
        &profile,
        AXIS_O,
        Vector3::new(0.0, 0.0, 1.0),
        PI,
    )
    .expect_err("normal-direction axis");
    assert_eq!(err, KernelV2Error::RevolveAxisNotInPlane);
    // Degenerate direction.
    let err = revolve(
        &mut arena,
        &profile,
        AXIS_O,
        Vector3::new(0.0, 0.0, 0.0),
        PI,
    )
    .expect_err("zero axis direction");
    assert_eq!(err, KernelV2Error::RevolveAxisNotInPlane);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

#[test]
fn axis_crossing_or_touching_profile_rejected_as_error() {
    // Crossing: rectangle straddles the axis (the F0073/F0074 class).
    let crossing = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, -1.0),
            Point2::new(3.0, -1.0),
            Point2::new(3.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("crossing profile");
    let mut arena = BrepArena::new();
    let err = revolve(&mut arena, &crossing, AXIS_O, AXIS_D, PI)
        .expect_err("axis through the profile interior");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);

    // Touching: bottom edge exactly on the axis (KV6a requires strict
    // clearance; the on-axis solid-of-revolution is a later capability).
    let touching = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(3.0, 0.0),
            Point2::new(3.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("touching profile");
    let err = revolve(&mut arena, &touching, AXIS_O, AXIS_D, PI)
        .expect_err("axis touching the profile boundary");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

#[test]
fn oblique_edges_circle_profiles_and_holes_rejected_typed() {
    let mut arena = BrepArena::new();

    // Oblique edge (3,1)→(4,2): neither parallel nor perpendicular to x̂.
    let oblique = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(4.0, 2.0),
            Point2::new(0.0, 2.0),
        ],
        vec![],
    )
    .expect("oblique profile");
    let err =
        revolve(&mut arena, &oblique, AXIS_O, AXIS_D, PI).expect_err("oblique edge → cone (KV6c)");
    assert_eq!(err, KernelV2Error::RevolveObliqueEdgeUnsupported);

    // FULL-turn circle profile → closed torus (KV6d full-turn, still walled;
    // partial-turn circle revolve → torus is supported, tested separately).
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(1.5, 5.0),
        0.5,
    )
    .expect("circle profile");
    let err = revolve(&mut arena, &circle, AXIS_O, AXIS_D, 2.0 * PI)
        .expect_err("full-turn circle profile → closed torus");
    assert_eq!(err, KernelV2Error::RevolveCircleProfileUnsupported);

    // Holed polygon.
    let holed = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(3.0, 2.0),
            Point2::new(0.0, 2.0),
        ],
        vec![vec![
            Point2::new(1.0, 1.25),
            Point2::new(2.0, 1.25),
            Point2::new(2.0, 1.75),
            Point2::new(1.0, 1.75),
        ]],
    )
    .expect("holed profile");
    let err = revolve(&mut arena, &holed, AXIS_O, AXIS_D, PI).expect_err("holed profile");
    assert_eq!(err, KernelV2Error::RevolveProfileHolesUnsupported);

    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

// =========================================================================
// KV6c: full-turn cone (oblique edge → frustum band)
// =========================================================================

/// A conical washer — inner cylinder r=1, two annular caps, and an OUTER cone
/// frustum (radius 2 at axial 0 rising to 3 at axial 3) — revolved 360° about
/// the x-axis. The oblique edge (3,3)→(0,2) sweeps a `Surface::Cone` band
/// (KV6c increment 4). The solid-of-revolution volume is
/// ∫₀³ π((2 + x/3)² − 1) dx = 16π.
#[test]
fn full_turn_oblique_edge_builds_cone_frustum() {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(3.0, 3.0),
            Point2::new(0.0, 2.0),
        ],
        vec![],
    )
    .expect("conical-washer profile");
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, 2.0 * PI)
        .expect("full-turn oblique revolve builds a cone frustum");

    // `validate_solid` already ran inside `revolve` (finalize_solid). Confirm
    // exactly one cone lateral was built.
    let cone_walls = r
        .walls
        .iter()
        .filter(|&&w| matches!(arena.face(w).unwrap().surface, Some(Surface::Cone { .. })))
        .count();
    assert_eq!(cone_walls, 1, "exactly one cone frustum wall");

    // Analytic solid-of-revolution volume = 16π. This is the orientation
    // oracle: a wrong cone normal / reversed sense would shift the total.
    let v = geom::signed_volume(&arena, r.solid).expect("volume");
    assert!(
        (v - 16.0 * PI).abs() < 1e-9,
        "volume {v}, want {}",
        16.0 * PI
    );

    // Tessellation succeeds end to end (exercises tessellate_cone_lateral).
    let mesh = tessellate(&arena, r.solid).expect("frustum solid tessellates");
    let nv = mesh.num_vertices();
    assert!(!mesh.indices.is_empty(), "non-empty mesh");
    assert!(
        mesh.indices.iter().all(|&i| (i as usize) < nv),
        "all triangle indices in range"
    );
}

// =========================================================================
// 5. Determinism
// =========================================================================

#[test]
fn revolve_construction_and_tessellation_deterministic() {
    let build = |angle: f64| {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);
        let mesh = tessellate(&arena, r.solid).expect("tessellate");
        (arena, mesh)
    };
    for angle in [1.2, 2.0 * PI] {
        let (a1, m1) = build(angle);
        let (a2, m2) = build(angle);
        assert_eq!(a1, a2, "bit-identical arenas at {angle}");
        assert_eq!(m1, m2, "bit-identical meshes at {angle}");
    }
}

// =========================================================================
// 6. KV6b: revolve operands CONVERT (the wall narrowed to OUTPUT re-entry —
//    see kv6b_revolve_boolean.rs for the end-to-end boolean suite)
// =========================================================================

#[test]
fn revolve_output_boolean_reentry_is_typed_wall() {
    // PR-KV6b-2 flipped the operand-level wall: revolve solids (partial AND
    // washer) now convert into yang BReps. What remains walled is boolean
    // OUTPUT re-entry (chord-polyline curved boundaries), pinned in
    // kv6b_revolve_boolean.rs::revolve_boolean_output_reentry_stays_typed_wall.
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, PI / 2.0);
    to_yang_brep(&arena, r.solid).expect("partial revolve operand converts since KV6b");

    let mut arena2 = BrepArena::new();
    let r2 = revolve_rect(&mut arena2, 2.0 * PI);
    to_yang_brep(&arena2, r2.solid).expect("washer operand converts since KV6b");
}

// =========================================================================
// 7. Legacy-trait adapter
// =========================================================================

mod adapter {
    use super::*;
    use kernel_v2::KernelV2Adapter;
    use std::collections::HashMap;
    use waffle_types::kernel::{ClosedProfile, Kernel, KernelError, KernelIntrospect};

    fn rect_closed_profile() -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, R1));
        positions.insert(2, (H, R1));
        positions.insert(3, (H, R2));
        positions.insert(4, (0.0, R2));
        (profile, positions)
    }

    fn staged_face(k: &mut KernelV2Adapter) -> waffle_types::kernel::KernelId {
        let (profile, positions) = rect_closed_profile();
        let faces = k
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("stage rect profile");
        faces[0]
    }

    #[test]
    fn revolve_face_partial_and_full_end_to_end() {
        for (angle_deg, expect_faces) in [(180.0, 6usize), (360.0, 4usize)] {
            let mut k = KernelV2Adapter::new();
            let face = staged_face(&mut k);
            let handle = k
                .revolve_face(face, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], angle_deg)
                .unwrap_or_else(|e| panic!("revolve_face({angle_deg}°): {e:?}"));

            assert_eq!(
                k.list_faces(&handle).len(),
                expect_faces,
                "face count at {angle_deg}°"
            );
            let mesh = k
                .tessellate(&handle, 0.001)
                .unwrap_or_else(|e| panic!("adapter tessellate({angle_deg}°): {e:?}"));
            assert!(!mesh.indices.is_empty(), "non-empty mesh at {angle_deg}°");

            // Mesh volume in the Pappus band.
            let analytic = super::pappus_volume(angle_deg.to_radians());
            let mut six_v = 0.0f64;
            for t in mesh.indices.chunks_exact(3) {
                let p = |i: u32| {
                    let j = (i as usize) * 3;
                    [
                        mesh.vertices[j] as f64,
                        mesh.vertices[j + 1] as f64,
                        mesh.vertices[j + 2] as f64,
                    ]
                };
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]);
            }
            let v = six_v / 6.0;
            assert!(
                (v - analytic).abs() < 0.03 * analytic,
                "adapter mesh volume at {angle_deg}°: {v} vs {analytic}"
            );
        }
    }

    /// F0073/F0074 semantics: the axis-through-profile rejection must reach
    /// the engine as a plain rebuild ERROR — its message must NOT carry the
    /// `"operation not supported:"` marker, or the assay would categorize
    /// the case UNSUPPORTED instead of the meta-expected rebuild error.
    #[test]
    fn axis_through_profile_is_error_not_notsupported() {
        let mut k = KernelV2Adapter::new();
        let (profile, mut positions) = rect_closed_profile();
        // Straddle the axis: y ∈ [−1, 1].
        positions.insert(1, (0.0, -1.0));
        positions.insert(2, (H, -1.0));
        let faces = k
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("stage straddling profile");
        let err = k
            .revolve_face(faces[0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 180.0)
            .expect_err("axis through profile");
        let msg = format!("{err}");
        assert!(
            !msg.contains("operation not supported:"),
            "axis-through-profile must be an ERROR, not NotSupported: {msg}"
        );
        assert!(
            matches!(err, KernelError::Other { .. }),
            "mapped to KernelError::Other, got {err:?}"
        );
    }

    /// Capability walls keep the NotSupported marker (assay UNSUPPORTED).
    #[test]
    fn capability_walls_keep_notsupported_marker() {
        // Out-of-range angle is invalid input → Other (not a capability).
        let mut k = KernelV2Adapter::new();
        let face = staged_face(&mut k);
        let err = k
            .revolve_face(face, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 400.0)
            .expect_err(">360° angle");
        assert!(matches!(err, KernelError::Other { .. }), "got {err:?}");

        // PR-KV6b-2: booleans over revolve operands now RUN (see
        // kv6b_revolve_boolean.rs::adapter for the end-to-end positive
        // suite). The disjoint union here exercises the adapter path.
        let mut k2 = KernelV2Adapter::new();
        let face2 = staged_face(&mut k2);
        let rev = k2
            .revolve_face(face2, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0)
            .expect("revolve");
        let (profile, positions) = rect_closed_profile();
        let f3 = k2
            .make_faces_from_profiles(
                &[profile],
                [10.0, 10.0, 10.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("stage box profile");
        let bx = k2.extrude_face(f3[0], [0.0, 0.0, 1.0], 1.0).expect("box");
        let out = k2
            .boolean_union(&rev, &bx)
            .expect("disjoint revolve ∪ box runs since KV6b");
        assert!(!k2.list_faces(&out).is_empty());
    }
}

// =========================================================================
// KV6d: partial torus (revolve a circle profile)
// =========================================================================

/// Revolving a circle profile by a partial angle builds a bent solid tube
/// (partial torus): 2 disk caps + 1 `Surface::Torus` lateral with longitude
/// arc seams. Validates as a genus-0 solid (V=2, E=3, F=3).
#[test]
fn kv6d_partial_torus_revolve_validates() {
    // Circle in the XY plane, center at radial 3 from the x-axis, minor r=1.
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 3.0),
        1.0,
    )
    .expect("circle profile");
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, PI / 2.0)
        .expect("partial torus revolve builds");
    let report = validate_solid(&arena, r.solid).expect("partial torus validates");
    assert_eq!(report.faces, 3, "2 caps + 1 toroidal lateral");
    assert_eq!(report.vertices, 2, "2 seam vertices");
    assert_eq!(report.edges, 3, "2 profile circles + 1 seam");
    assert_eq!(report.genus, 0);
    assert!(
        r.walls
            .iter()
            .any(|&w| matches!(arena.face(w).unwrap().surface, Some(Surface::Torus { .. }))),
        "a Surface::Torus lateral was built"
    );

    // Tessellates watertight, and the mesh volume approaches the analytic
    // partial-torus volume (Pappus): V = α · R · π r² (here α=π/2, R=3, r=1).
    let mesh = tessellate(&arena, r.solid).expect("partial torus tessellates");
    assert_mesh_sane(&mesh, "partial torus");
    assert_watertight(&mesh, "partial torus");
    let exact = (PI / 2.0) * 3.0 * PI * 1.0 * 1.0;
    let vol = mesh_signed_volume(&mesh).abs();
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "partial torus mesh volume {vol} vs analytic {exact} (5% facet band)"
    );
}

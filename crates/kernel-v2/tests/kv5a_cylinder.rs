//! PR-KV5a RED oracles — curved-geometry core: circle profiles → extruded
//! right-circular-cylinder solids, curved-aware validation, render
//! tessellation, analytic introspection.
//!
//! ## Oracle groups
//!
//! 1. **Topology + validation**: a cylinder extrude is `validate_solid`
//!    green under the documented curved Euler accounting (V=2, E=3, F=3,
//!    R=0, S=1, G=0 — Stroud 2006 §3.1.4 single-fake-edge representation,
//!    fig. 6.58; see `arena` module docs), with typed cap/lateral surfaces.
//! 2. **Analytic introspection**: signed volume == π·r²·h and surface area
//!    == 2πr(h + r), asserted **bitwise** on a fixture whose π-coefficients
//!    (r²h = 4.5, 2r(h+r) = 10.5) are exactly representable — the
//!    implementation accumulates the exact rational π-coefficient of the
//!    surface integral and rounds once — and to ≤ 1e-12 relative on a
//!    general off-origin, antiparallel-sweep fixture.
//! 3. **Tessellation**: mesh signed volume equals the inscribed-N-gon prism
//!    volume `(1/2)·N·r²·sin(2π/N)·h` to 1e-12 relative (THAT formula is
//!    the oracle band — exact for vertices on the circle; the f64 cos/sin
//!    sampling perturbs at ~1e-16 relative), and converges to π·r²·h
//!    quadratically as the chord tolerance shrinks (N doubles ⇒ error ÷ ~4).
//! 4. **Winding/orientation**: positive mesh volume; lateral per-vertex
//!    normals are exact outward radial directions at the quad corners; cap
//!    normals are exactly ∓axis.
//! 5. **Rejections**: oblique circle extrude (elliptic cylinder — out of
//!    vocabulary), in-plane direction, non-positive/non-finite radius,
//!    non-orthonormal circle frame, curved boolean input (KV5b boundary).
//! 6. **Determinism**: identical construction ⇒ bit-identical arenas,
//!    meshes, and edge polylines.
//!
//! NEGATIVE oracle: the existing planar suites (kv1/kv2/kv3 — 55 tests)
//! must stay green untouched.

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    circle_segment_count, extract_edges, extract_edges_with_chord_tolerance, extrude, geom,
    make_face_from_profile, surface_area, tessellate, tessellate_with_chord_tolerance,
    to_yang_brep, validate_solid, BrepArena, Curve, ExtrudeResult, FaceId, KernelV2Error, Profile,
    RenderMesh, Surface, RENDER_CHORD_TOLERANCE_REL,
};

// =========================================================================
// Fixtures
// =========================================================================

/// Exact-coefficient fixture: r = 1.5, h = 2.0, axis +z through the origin.
/// r²h = 4.5 and 2r(h+r) = 10.5 are exactly representable, so the analytic
/// oracles can assert bitwise equality with `4.5 * PI` / `10.5 * PI`.
const R_EXACT: f64 = 1.5;
const H_EXACT: f64 = 2.0;

fn exact_profile() -> Profile {
    Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        R_EXACT,
    )
    .expect("exact circle profile")
}

fn exact_cylinder(arena: &mut BrepArena) -> ExtrudeResult {
    let profile = exact_profile();
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), H_EXACT).expect("exact cylinder extrude")
}

/// General fixture: assay-scale radius/height, off-origin plane, plane
/// normal +y, swept ANTIPARALLEL to the normal (the `cosine < 0` branch).
const R_GEN: f64 = 0.397_896_053_546_063_94;
const H_GEN: f64 = 0.574_275_025_183_695_7;

fn general_profile() -> Profile {
    // u × v = (1,0,0) × (0,0,-1) = (0, 1, 0).
    Profile::circle(
        Point3::new(0.1, 0.2, 0.3),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, -1.0),
        Point2::new(0.3, -0.2),
        R_GEN,
    )
    .expect("general circle profile")
}

fn general_cylinder(arena: &mut BrepArena) -> ExtrudeResult {
    let profile = general_profile();
    extrude(arena, &profile, Vector3::new(0.0, -1.0, 0.0), H_GEN)
        .expect("general cylinder extrude (antiparallel sweep)")
}

// =========================================================================
// Shared oracle helpers
// =========================================================================

/// Signed volume of a triangle mesh: `Σ det[p0, p1, p2] / 6`.
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

/// Inscribed regular-N-gon prism volume: `(1/2)·N·r²·sin(2π/N)·h` — EXACT
/// for an N-gon inscribed in the circle of radius `r`, extruded by `h`.
fn inscribed_prism_volume(n: u32, r: f64, h: f64) -> f64 {
    0.5 * (n as f64) * r * r * (2.0 * PI / (n as f64)).sin() * h
}

/// Vertex indices referenced by one face's index range, deduplicated.
fn face_vertex_indices(mesh: &RenderMesh, face: FaceId) -> Vec<u32> {
    let range = mesh
        .face_ranges
        .iter()
        .find(|fr| fr.face == face)
        .unwrap_or_else(|| panic!("face {face:?} has a range"));
    let mut idx: Vec<u32> =
        mesh.indices[range.start as usize..(range.start + range.count) as usize].to_vec();
    idx.sort_unstable();
    idx.dedup();
    idx
}

fn assert_rel_eq(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs();
    assert!(
        err <= rel * expected.abs(),
        "{what}: |{actual} - {expected}| = {err} > {rel} * {}",
        expected.abs()
    );
}

// =========================================================================
// 1. Topology + validation
// =========================================================================

#[test]
fn cylinder_extrude_validates_with_curved_euler_accounting() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);

    // Curved Euler accounting (arena module docs): V=2 (one seam vertex per
    // rim), E=3 (two closed rim circles + one seam ruling), F=3 (two caps +
    // lateral), R=0, S=1, G=0 ⇒ V−E+F−R = 2 = 2(S−G).
    let report = validate_solid(&arena, result.solid).expect("cylinder validates");
    assert_eq!(report.vertices, 2, "two seam vertices");
    assert_eq!(report.edges, 3, "two rim circles + one seam");
    assert_eq!(report.faces, 3, "two caps + lateral");
    assert_eq!(report.rings, 0);
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0);
    assert_eq!(report.euler_lhs, 2);
    assert_eq!(report.euler_rhs, 2);

    // Result shape: one lateral wall, no hole walls.
    assert_eq!(result.walls.len(), 1, "one cylindrical lateral face");
    assert!(result.hole_walls.is_empty());

    // Typed surfaces. Cap normals are exactly ∓axis (axis = +z here);
    // the base (in the profile plane) opposes the sweep.
    let base = arena.face(result.base).expect("base face");
    let Some(Surface::Plane(base_plane)) = base.surface else {
        panic!("base must be planar, got {:?}", base.surface);
    };
    assert_eq!(
        (
            base_plane.normal.x,
            base_plane.normal.y,
            base_plane.normal.z
        ),
        (0.0, 0.0, -1.0),
        "base cap outward normal is exactly −axis"
    );
    let top = arena.face(result.top).expect("top face");
    let Some(Surface::Plane(top_plane)) = top.surface else {
        panic!("top must be planar, got {:?}", top.surface);
    };
    assert_eq!(
        (top_plane.normal.x, top_plane.normal.y, top_plane.normal.z),
        (0.0, 0.0, 1.0),
        "top cap outward normal is exactly +axis"
    );
    let lateral = arena.face(result.walls[0]).expect("lateral face");
    let Some(Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    }) = lateral.surface
    else {
        panic!("lateral must be a cylinder, got {:?}", lateral.surface);
    };
    assert_eq!(radius, R_EXACT);
    assert_eq!((axis_dir.x, axis_dir.y, axis_dir.z), (0.0, 0.0, 1.0));
    assert_eq!(
        (axis_point.x(), axis_point.y(), axis_point.z()),
        (0.0, 0.0, 0.0),
        "axis through the base-rim center"
    );

    // Cap boundary representation: each cap's outer loop is a SINGLE
    // closed circle half-edge (next(h) == h) whose directional normal
    // equals the cap's outward normal; the lateral loop has 4 half-edges
    // (rim, seam up, rim, seam down).
    let base_hes = arena
        .loop_half_edges(base.outer_loop)
        .expect("base loop walk");
    assert_eq!(base_hes.len(), 1, "cap outer loop = one circle half-edge");
    let base_he = arena.half_edge(base_hes[0]).expect("base he");
    assert_eq!(base_he.next, base_hes[0], "closed: next(h) == h");
    let Curve::Circle { normal, radius, .. } = base_he.curve else {
        panic!("cap boundary must be a Circle, got {:?}", base_he.curve);
    };
    assert_eq!(radius, R_EXACT);
    assert_eq!(
        (normal.x, normal.y, normal.z),
        (0.0, 0.0, -1.0),
        "cap circle traversal CCW around the cap's outward normal"
    );
    let lat_hes = arena
        .loop_half_edges(lateral.outer_loop)
        .expect("lateral loop walk");
    assert_eq!(
        lat_hes.len(),
        4,
        "lateral loop: rim, seam up, rim, seam down"
    );

    // General fixture (off-origin, antiparallel sweep) validates too.
    let mut arena2 = BrepArena::new();
    let result2 = general_cylinder(&mut arena2);
    let report2 = validate_solid(&arena2, result2.solid).expect("general cylinder validates");
    assert_eq!((report2.vertices, report2.edges, report2.faces), (2, 3, 3));
}

// =========================================================================
// 2. Analytic introspection (volume / area independent of tessellation)
// =========================================================================

#[test]
fn analytic_volume_is_exactly_pi_r_squared_h() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);
    let vol = geom::signed_volume(&arena, result.solid).expect("signed volume");
    // r²h = 1.5² · 2 = 4.5 exactly; the exact rational π-coefficient
    // accumulation must surface it bitwise.
    assert_eq!(vol, 4.5 * PI, "analytic volume == π·r²·h bitwise");

    let mut arena2 = BrepArena::new();
    let result2 = general_cylinder(&mut arena2);
    let vol2 = geom::signed_volume(&arena2, result2.solid).expect("signed volume");
    assert!(vol2 > 0.0, "outward orientation: positive volume");
    assert_rel_eq(
        vol2,
        PI * R_GEN * R_GEN * H_GEN,
        1e-12,
        "general analytic volume",
    );
}

#[test]
fn analytic_area_is_exactly_two_pi_r_h_plus_r() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);
    let area = surface_area(&arena, result.solid).expect("surface area");
    // 2r(h + r) = 2·1.5·3.5 = 10.5 exactly.
    assert_eq!(area, 10.5 * PI, "analytic area == 2πr(h+r) bitwise");

    let mut arena2 = BrepArena::new();
    let result2 = general_cylinder(&mut arena2);
    let area2 = surface_area(&arena2, result2.solid).expect("surface area");
    assert_rel_eq(
        area2,
        2.0 * PI * R_GEN * (H_GEN + R_GEN),
        1e-12,
        "general analytic area",
    );
}

// =========================================================================
// 3. Tessellation: inscribed-prism exactness + convergence
// =========================================================================

#[test]
fn mesh_volume_equals_inscribed_prism_formula() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);
    let mesh = tessellate(&arena, result.solid).expect("tessellate cylinder");
    let n = circle_segment_count(RENDER_CHORD_TOLERANCE_REL);
    let expected = inscribed_prism_volume(n, R_EXACT, H_EXACT);
    assert_rel_eq(
        mesh_signed_volume(&mesh),
        expected,
        1e-12,
        "mesh volume vs inscribed N-gon prism (the analytic chord-band oracle)",
    );

    // Structural counts: caps fan to N−2 triangles each, lateral is N
    // quad-pairs (2N triangles).
    let cap_verts = face_vertex_indices(&mesh, result.base).len(); // N rim verts
    assert_eq!(cap_verts as u32, n, "cap emits N rim vertices");
    let lat_range = mesh
        .face_ranges
        .iter()
        .find(|fr| fr.face == result.walls[0])
        .expect("lateral range");
    assert_eq!(
        lat_range.count,
        6 * n,
        "lateral = N quad-pairs = 2N triangles"
    );
    for fr in &mesh.face_ranges {
        if fr.face == result.base || fr.face == result.top {
            assert_eq!(fr.count, 3 * (n - 2), "cap fan = N−2 triangles");
        }
    }
}

#[test]
fn mesh_volume_converges_to_analytic_as_chord_band_tightens() {
    let mut arena = BrepArena::new();
    let result = general_cylinder(&mut arena);
    let analytic = PI * R_GEN * R_GEN * H_GEN;

    // Sagitta band ∝ 1/N²: shrinking the tolerance 4× doubles N and must
    // cut the volume defect ~4× (quadratic convergence).
    let tols = [1e-2, 2.5e-3, 6.25e-4];
    let mut errors = Vec::new();
    for &tol in &tols {
        let n = circle_segment_count(tol);
        let mesh = tessellate_with_chord_tolerance(&arena, result.solid, tol).expect("tessellate");
        // Each mesh matches ITS inscribed-prism volume to 1e-12 …
        assert_rel_eq(
            mesh_signed_volume(&mesh),
            inscribed_prism_volume(n, R_GEN, H_GEN),
            1e-12,
            "inscribed prism at tightened band",
        );
        errors.push((analytic - mesh_signed_volume(&mesh)).abs());
    }
    // … and the defect against the analytic volume shrinks quadratically.
    assert!(
        errors[1] < errors[0] / 3.0 && errors[2] < errors[1] / 3.0,
        "quadratic convergence expected, got defects {errors:?}"
    );
    assert!(errors[2] < errors[0] / 8.0, "overall ≥8× reduction");
}

// =========================================================================
// 4. Winding / orientation
// =========================================================================

#[test]
fn winding_outward_lateral_normals_exactly_radial_at_corners() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);
    let mesh = tessellate(&arena, result.solid).expect("tessellate");

    assert!(
        mesh_signed_volume(&mesh) > 0.0,
        "outward-wound mesh: positive signed volume"
    );

    // Lateral: every vertex normal is the exact outward radial direction at
    // that quad corner — unit, perpendicular to the axis, aligned with the
    // corner's radial offset (axis = +z through origin for this fixture).
    for i in face_vertex_indices(&mesh, result.walls[0]) {
        let k = (i as usize) * 3;
        let (px, py) = (mesh.positions[k], mesh.positions[k + 1]);
        let (nx, ny, nz) = (mesh.normals[k], mesh.normals[k + 1], mesh.normals[k + 2]);
        assert!(
            nz.abs() <= 1e-12,
            "lateral normal ⊥ axis at corner {i}: nz = {nz}"
        );
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        assert!((len - 1.0).abs() <= 1e-12, "unit normal at corner {i}");
        // Outward radial: n · (p − axis)/r == 1 up to sampling rounding.
        let dot = (nx * px + ny * py) / R_EXACT;
        assert!(
            (dot - 1.0).abs() <= 1e-9,
            "normal not the outward radial at corner {i}: dot = {dot}"
        );
    }

    // Caps: flat-shaded with exactly ∓axis.
    for (face, expected) in [(result.base, -1.0), (result.top, 1.0)] {
        for i in face_vertex_indices(&mesh, face) {
            let k = (i as usize) * 3;
            assert_eq!(
                (mesh.normals[k], mesh.normals[k + 1], mesh.normals[k + 2]),
                (0.0, 0.0, expected),
                "cap normal exactly ±axis at vertex {i}"
            );
        }
    }
}

// =========================================================================
// 5. Rejections (typed, loud, pre-mutation)
// =========================================================================

#[test]
fn oblique_circle_extrude_rejected_typed() {
    let mut arena = BrepArena::new();
    let profile = exact_profile();
    // 45° off the normal: legal for polygons (sheared prism), but a circle
    // would become an elliptic cylinder — out of the KV5a vocabulary.
    let err = extrude(&mut arena, &profile, Vector3::new(0.0, 0.7, 0.7), 1.0)
        .expect_err("oblique circle extrude must be rejected");
    assert_eq!(err, KernelV2Error::ExtrudeObliqueCircleUnsupported);
    assert_eq!(
        arena,
        BrepArena::new(),
        "rejection leaves the arena untouched"
    );

    // In-plane direction keeps the SHARED typed error.
    let err = extrude(&mut arena, &profile, Vector3::new(1.0, 0.0, 0.0), 1.0)
        .expect_err("in-plane direction");
    assert_eq!(err, KernelV2Error::ExtrudeDirectionInPlane);

    // Shared argument validation also applies to the circle path.
    let err =
        extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 0.0).expect_err("zero distance");
    assert_eq!(err, KernelV2Error::ExtrudeNonPositiveDistance);
    let err = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 0.0), 1.0)
        .expect_err("zero direction");
    assert_eq!(err, KernelV2Error::ExtrudeDegenerateDirection);
}

#[test]
fn invalid_circle_profiles_rejected_typed() {
    let o = Point3::new(0.0, 0.0, 0.0);
    let ex = Vector3::new(1.0, 0.0, 0.0);
    let ey = Vector3::new(0.0, 1.0, 0.0);
    let c = Point2::new(0.0, 0.0);

    for bad_r in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let err = Profile::circle(o, ex, ey, c, bad_r)
            .expect_err("non-positive/non-finite radius must be rejected");
        assert_eq!(
            err,
            KernelV2Error::ProfileCircleNonPositiveRadius,
            "r = {bad_r}"
        );
    }

    // Skewed frame (u·v ≠ 0): the embedded "circle" would be an ellipse.
    let err =
        Profile::circle(o, ex, Vector3::new(0.6, 0.8, 0.0), c, 1.0).expect_err("skewed frame");
    assert_eq!(err, KernelV2Error::ProfileCircleFrameNotOrthonormal);

    // Scaled frame (|u| ≠ 1): plane coordinates would not be metric.
    let err = Profile::circle(
        o,
        Vector3::new(2.0, 0.0, 0.0),
        Vector3::new(0.0, 2.0, 0.0),
        c,
        1.0,
    )
    .expect_err("scaled frame");
    assert_eq!(err, KernelV2Error::ProfileCircleFrameNotOrthonormal);

    // Degenerate basis stays the shared typed error.
    let err = Profile::circle(o, ex, ex, c, 1.0).expect_err("parallel basis");
    assert_eq!(err, KernelV2Error::ProfileDegenerateBasis);

    // Non-finite center.
    let err = Profile::circle(o, ex, ey, Point2::new(f64::NAN, 0.0), 1.0).expect_err("NaN center");
    assert_eq!(err, KernelV2Error::ProfileNotFinite);
}

#[test]
fn curved_boolean_input_rejected_typed_until_kv5b() {
    let mut arena = BrepArena::new();
    let result = exact_cylinder(&mut arena);
    let err = to_yang_brep(&arena, result.solid)
        .expect_err("curved boolean conversion is PR-KV5b; must be loud, not mistranslated");
    assert!(
        matches!(err, KernelV2Error::UnsupportedCurvedBoolean { .. }),
        "typed curved-boolean rejection, got {err:?}"
    );
}

// =========================================================================
// 6. Edge extraction
// =========================================================================

#[test]
fn extract_edges_emits_circle_polylines_at_tessellation_n() {
    let mut arena = BrepArena::new();
    let result = general_cylinder(&mut arena);
    let n = circle_segment_count(RENDER_CHORD_TOLERANCE_REL) as usize;

    let polylines = extract_edges(&arena, result.solid).expect("extract_edges");
    assert_eq!(polylines.len(), 3, "two rim circles + one seam");

    let mut circles = 0;
    let mut seams = 0;
    // Axis for the general fixture: −y (sweep was antiparallel to +y).
    let axis = [0.0, -1.0, 0.0];
    for pl in &polylines {
        if pl.len() == 2 {
            seams += 1;
            let d = [
                pl[1].x() - pl[0].x(),
                pl[1].y() - pl[0].y(),
                pl[1].z() - pl[0].z(),
            ];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert_rel_eq(len, H_GEN, 1e-12, "seam length == height");
            let along = (d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2]).abs();
            assert_rel_eq(along, len, 1e-12, "seam parallel to the axis");
        } else {
            circles += 1;
            assert_eq!(pl.len(), n + 1, "closed circle polyline at tessellation N");
            assert_eq!(pl[0], pl[n], "explicit closure: last == first");
            // Every sample lies on the rim circle (radius from the axis).
            for p in &pl[..n] {
                let radial = ((p.x() - 0.4).powi(2) + (p.z() - 0.5).powi(2)).sqrt();
                assert!(
                    (radial - R_GEN).abs() <= 1e-9 * R_GEN.max(1.0),
                    "rim sample off circle: radial = {radial}"
                );
            }
        }
    }
    assert_eq!((circles, seams), (2, 1));

    // Same N plumbing as tessellation: a tightened band changes the
    // polyline density identically.
    let tight =
        extract_edges_with_chord_tolerance(&arena, result.solid, 6.25e-4).expect("tight extract");
    let n_tight = circle_segment_count(6.25e-4) as usize;
    assert!(n_tight > n, "tightened band increases N");
    let mut tight_circle_lens: Vec<usize> =
        tight.iter().map(Vec::len).filter(|&l| l != 2).collect();
    tight_circle_lens.sort_unstable();
    assert_eq!(tight_circle_lens, vec![n_tight + 1, n_tight + 1]);
}

// =========================================================================
// 7. Circle lamina (the zero-height analog — implemented at GREEN so
//    make_face_from_profile stays total over the new Profile variant)
// =========================================================================

#[test]
fn circle_lamina_validates_zero_volume_double_disk_area() {
    let mut arena = BrepArena::new();
    let profile = exact_profile();
    let lam = make_face_from_profile(&mut arena, &profile).expect("circle lamina");
    // One seam vertex, one closed circle edge, two disk faces:
    // V − E + F − R = 1 − 1 + 2 = 2.
    let report = validate_solid(&arena, lam.solid).expect("lamina validates");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.rings),
        (1, 1, 2, 0)
    );
    assert_eq!(report.euler_lhs, 2);
    assert_eq!(report.euler_rhs, 2);
    // Zero enclosed volume (the disk terms cancel exactly in rational
    // arithmetic); area = two disks = 2πr² = 4.5π bitwise.
    assert_eq!(geom::signed_volume(&arena, lam.solid).expect("volume"), 0.0);
    assert_eq!(surface_area(&arena, lam.solid).expect("area"), 4.5 * PI);
}

// =========================================================================
// 8. Determinism
// =========================================================================

#[test]
fn cylinder_construction_and_tessellation_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let result = general_cylinder(&mut arena);
        let mesh = tessellate(&arena, result.solid).expect("tessellate");
        let edges = extract_edges(&arena, result.solid).expect("edges");
        (arena, mesh, edges)
    };
    let (a1, m1, e1) = build();
    let (a2, m2, e2) = build();
    assert_eq!(a1, a2, "bit-identical arenas");
    assert_eq!(m1, m2, "bit-identical meshes");
    assert_eq!(e1, e2, "bit-identical edge polylines");
}

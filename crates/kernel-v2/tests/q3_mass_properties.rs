//! Q3 oracles — `specs/agent_mechanical_design.md` §4.4 "Mass".
//!
//! Every expected number here is a closed form written out in the assertion,
//! never a value copied from a previous run.
//!
//! What the tiers claim, and what each case proves:
//!
//! | case | tier | why |
//! |---|---|---|
//! | box | Exact | planar faces, straight loops |
//! | cylinder | Exact | disk caps + the lateral's `(θ, t)` chart |
//! | cone (apex form) | Exact | one disk cap + the lateral's `(θ, τ)` chart |
//! | plate with a round hole | Exact | a BOOLEAN result: caps with a circular inner loop and a `reversed` bore wall |
//! | sphere | Mesh | no closed-form chart yet — and the volume deficit stays inside the band |
//!
//! Plus the two invariances the spec asks for: a translation moves the
//! centroid by exactly the translation and leaves the inertia alone, and a
//! rotation leaves the principal moments alone.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::mass::{mass_properties, MassResult};
use kernel_v2::{boolean_op, extrude, revolve, BrepArena, Profile, SolidId};
use waffle_types::kernel::RigidPlacement;

// -------------------------------------------------------------------------
// Fixtures (meters, as the kernel works in)
// -------------------------------------------------------------------------

fn block(arena: &mut BrepArena, sx: f64, sy: f64, sz: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(sx, 0.0),
            Point2::new(sx, sy),
            Point2::new(0.0, sy),
        ],
        vec![],
    )
    .expect("rectangle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), sz)
        .expect("extrude")
        .solid
}

fn cylinder(arena: &mut BrepArena, cx: f64, cy: f64, z0: f64, r: f64, h: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("extrude")
        .solid
}

/// Relative closeness, so the assertions read in the units of the answer.
#[track_caller]
fn close(got: f64, want: f64, rel: f64, what: &str) {
    let scale = want.abs().max(got.abs()).max(f64::MIN_POSITIVE);
    assert!(
        (got - want).abs() <= rel * scale,
        "{what}: got {got:e}, want {want:e} (relative {:e} > {rel:e})",
        (got - want).abs() / scale
    );
}

// -------------------------------------------------------------------------
// Exact tier
// -------------------------------------------------------------------------

#[test]
fn a_box_is_exact_in_every_quantity() {
    // 20 mm × 10 mm × 5 mm, near corner at the origin.
    let (sx, sy, sz) = (0.02, 0.01, 0.005);
    let mut arena = BrepArena::new();
    let s = block(&mut arena, sx, sy, sz);
    let m = mass_properties(&arena, s, 1.0).expect("a box integrates");

    assert!(m.exact, "a planar solid is the exact tier");
    assert_eq!(m.chord_bound, 0.0, "an exact answer carries no band");
    close(m.volume, sx * sy * sz, 1e-14, "volume");
    close(
        m.surface_area,
        2.0 * (sx * sy + sy * sz + sz * sx),
        1e-14,
        "area",
    );
    for (i, want) in [sx / 2.0, sy / 2.0, sz / 2.0].into_iter().enumerate() {
        close(m.centroid[i], want, 1e-14, &format!("centroid[{i}]"));
    }
    // The closed form: I_xx = m(b² + c²)/12 about the centroid.
    let v = sx * sy * sz;
    for (i, (a, b)) in [(sy, sz), (sz, sx), (sx, sy)].into_iter().enumerate() {
        close(
            m.inertia_at_centroid[i][i],
            v * (a * a + b * b) / 12.0,
            1e-13,
            &format!("I[{i}][{i}]"),
        );
    }
    // A box's axes are the world axes, so every off-diagonal term vanishes.
    for i in 0..3 {
        for j in 0..3 {
            if i != j {
                assert!(
                    m.inertia_at_centroid[i][j].abs() <= 1e-16 * v * sx * sx,
                    "off-diagonal I[{i}][{j}] = {:e}",
                    m.inertia_at_centroid[i][j]
                );
            }
        }
    }
    // Density scales mass and inertia linearly and nothing else.
    let steel = mass_properties(&arena, s, 7850.0).expect("a box integrates");
    close(steel.volume, m.volume, 1e-15, "volume is density-free");
    close(steel.mass, 7850.0 * m.volume, 1e-14, "mass");
    close(
        steel.inertia_at_centroid[0][0],
        7850.0 * m.inertia_at_centroid[0][0],
        1e-14,
        "inertia scales with density",
    );
}

#[test]
fn a_cylinder_is_exact_not_chordal() {
    // r = 5 mm, h = 10 mm, axis = z through the origin.
    let (r, h) = (0.005, 0.01);
    let mut arena = BrepArena::new();
    let s = cylinder(&mut arena, 0.0, 0.0, 0.0, r, h);
    let m = mass_properties(&arena, s, 1.0).expect("a cylinder integrates");

    assert!(m.exact, "the lateral's own chart is exact");
    let v = PI * r * r * h;
    close(m.volume, v, 1e-14, "volume");
    close(m.surface_area, 2.0 * PI * r * (r + h), 1e-14, "area");
    close(m.centroid[2], h / 2.0, 1e-13, "centroid z");
    for i in 0..2 {
        assert!(
            m.centroid[i].abs() <= 1e-15 * r,
            "centroid[{i}] is on the axis: {:e}",
            m.centroid[i]
        );
    }
    // Closed forms: I_zz = m r²/2, I_xx = I_yy = m(3r² + h²)/12.
    close(m.inertia_at_centroid[2][2], v * r * r / 2.0, 1e-13, "I_zz");
    for i in 0..2 {
        close(
            m.inertia_at_centroid[i][i],
            v * (3.0 * r * r + h * h) / 12.0,
            1e-13,
            &format!("I[{i}][{i}]"),
        );
    }
    // The spectrum is degenerate (two equal transverse moments); the ascending
    // order puts the axial one first here because r²/2 < (3r² + h²)/12 for
    // this h.
    close(m.principal_moments[0], v * r * r / 2.0, 1e-13, "λ₀");
    close(
        m.principal_moments[2],
        v * (3.0 * r * r + h * h) / 12.0,
        1e-13,
        "λ₂",
    );
}

#[test]
fn a_cone_is_exact_including_its_apex_arm() {
    // A right triangle revolved about the x axis: apex at the origin, base
    // radius 4 mm at x = 10 mm.
    let (r, h) = (0.004, 0.01);
    let mut arena = BrepArena::new();
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(h, 0.0),
            Point2::new(h, r),
        ],
        vec![],
    )
    .expect("triangle profile");
    let s = revolve(
        &mut arena,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("cone revolve")
    .solid;
    let m = mass_properties(&arena, s, 1.0).expect("a cone integrates");

    assert!(m.exact, "the apex form is an exact arm");
    let v = PI * r * r * h / 3.0;
    close(m.volume, v, 1e-13, "volume");
    // Slant + base: π r (r + √(r² + h²)).
    close(
        m.surface_area,
        PI * r * (r + (r * r + h * h).sqrt()),
        1e-13,
        "area",
    );
    // The centroid of a cone sits a quarter of the height up from the base,
    // i.e. 3h/4 from the apex.
    close(m.centroid[0], 0.75 * h, 1e-13, "centroid along the axis");
    // I about the axis = 3 m r² / 10.
    close(
        m.inertia_at_centroid[0][0],
        0.3 * v * r * r,
        1e-12,
        "I_axial",
    );
}

#[test]
fn a_boolean_result_with_a_bore_wall_is_exact() {
    // A 20 mm plate with a 6 mm through hole: the §4.4 case that exercises the
    // signs no primitive does — a planar cap with a CIRCULAR inner loop (the
    // disk arm at sign −1) and a `reversed` cylinder bore wall. A sign error
    // in either shows up here and nowhere else, and the `signed_volume`
    // cross-check inside `mass_properties` would STOP first.
    let (s, r) = (0.02, 0.003);
    let mut arena = BrepArena::new();
    let plate = block(&mut arena, s, s, s);
    // The tool protrudes both ways so no cap of it is coplanar with a cap of
    // the plate (the Stage-0 wall is not what this case is about).
    let tool = cylinder(&mut arena, s / 2.0, s / 2.0, -0.005, r, s + 0.01);
    let holed = boolean_op(&mut arena, plate, tool, BoolOp::Subtract).expect("through cut");
    let m = mass_properties(&arena, holed, 1.0).expect("the holed plate integrates");

    assert!(m.exact, "a bore wall is a full cylinder band: exact");
    close(m.volume, s * s * s - PI * r * r * s, 1e-13, "volume");
    // Outer box faces, minus the two hole disks, plus the bore wall.
    close(
        m.surface_area,
        6.0 * s * s - 2.0 * PI * r * r + 2.0 * PI * r * s,
        1e-13,
        "area",
    );
    for i in 0..3 {
        close(m.centroid[i], s / 2.0, 1e-13, &format!("centroid[{i}]"));
    }
}

// -------------------------------------------------------------------------
// Mesh tier
// -------------------------------------------------------------------------

#[test]
fn a_sphere_is_the_mesh_tier_and_stays_inside_its_own_band() {
    let r = 0.004;
    let mut arena = BrepArena::new();
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("on-axis circle");
    let s = revolve(
        &mut arena,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("sphere revolve")
    .solid;
    let m = mass_properties(&arena, s, 1.0).expect("a sphere integrates");

    assert!(
        !m.exact,
        "a sphere face has no closed-form chart arm yet, so the tier is honest"
    );
    assert!(m.chord_bound > 0.0, "the mesh tier carries its band");

    // The band on a VOLUME derived from a chord band `ε`: every render
    // triangle is inscribed, so the solid is short by at most the area times
    // the band. That is the bound a consumer must carry, and the answer must
    // be inside it — and SHORT, never long, because the mesh is inscribed.
    let truth = 4.0 / 3.0 * PI * r * r * r;
    let bound = m.surface_area * m.chord_bound;
    assert!(
        m.volume < truth,
        "an inscribed mesh under-reports: {} vs {truth}",
        m.volume
    );
    assert!(
        truth - m.volume <= bound,
        "volume deficit {:e} exceeds area × band {:e}",
        truth - m.volume,
        bound
    );
    assert!(
        (m.surface_area - 4.0 * PI * r * r).abs() <= 4.0 * PI * r * r * 1e-2,
        "area {} vs {}",
        m.surface_area,
        4.0 * PI * r * r
    );
    for i in 0..3 {
        assert!(
            m.centroid[i].abs() <= 1e-12 * r,
            "a sphere's centroid is its center: centroid[{i}] = {:e}",
            m.centroid[i]
        );
    }
}

// -------------------------------------------------------------------------
// Invariances
// -------------------------------------------------------------------------

/// The quantities a rigid motion must move, and the ones it must not.
fn placed(arena: &mut BrepArena, s: SolidId, p: &RigidPlacement) -> MassResult {
    let moved = kernel_v2::transform::transform_solid(arena, s, p).expect("rigid copy");
    mass_properties(arena, moved, 1.0).expect("the copy integrates")
}

#[test]
fn a_translation_moves_the_centroid_and_nothing_else() {
    let mut arena = BrepArena::new();
    let s = block(&mut arena, 0.02, 0.01, 0.005);
    let home = mass_properties(&arena, s, 1.0).expect("integrates");
    let shift = [0.3, -0.7, 1.1];
    let moved = placed(
        &mut arena,
        s,
        &RigidPlacement {
            translation: shift,
            rotation: RigidPlacement::IDENTITY.rotation,
        },
    );

    close(moved.volume, home.volume, 1e-14, "volume");
    close(moved.surface_area, home.surface_area, 1e-14, "area");
    for (i, d) in shift.into_iter().enumerate() {
        close(
            moved.centroid[i],
            home.centroid[i] + d,
            1e-12,
            &format!("centroid[{i}]"),
        );
        // The inertia is ABOUT the centroid, so it is translation-invariant.
        // The tolerance is relative to the tensor, not to the (much larger)
        // second moments about the origin the shift puts through the
        // subtraction — which is exactly why that subtraction is the term to
        // watch here.
        close(
            moved.inertia_at_centroid[i][i],
            home.inertia_at_centroid[i][i],
            1e-6,
            &format!("I[{i}][{i}] is translation-invariant"),
        );
    }
}

#[test]
fn a_rotation_leaves_the_principal_moments_alone() {
    let mut arena = BrepArena::new();
    let s = block(&mut arena, 0.02, 0.01, 0.005);
    let home = mass_properties(&arena, s, 1.0).expect("integrates");
    // 30° about z composed with 40° about x — nothing axis-aligned.
    let (s1, c1) = (30f64.to_radians()).sin_cos();
    let (s2, c2) = (40f64.to_radians()).sin_cos();
    let rz = [[c1, -s1, 0.0], [s1, c1, 0.0], [0.0, 0.0, 1.0]];
    let rx = [[1.0, 0.0, 0.0], [0.0, c2, -s2], [0.0, s2, c2]];
    let mut rotation = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            rotation[i][j] = (0..3).map(|k| rz[i][k] * rx[k][j]).sum();
        }
    }
    let moved = placed(
        &mut arena,
        s,
        &RigidPlacement {
            translation: [0.0; 3],
            rotation,
        },
    );

    close(moved.volume, home.volume, 1e-14, "volume");
    close(moved.surface_area, home.surface_area, 1e-13, "area");
    for i in 0..3 {
        close(
            moved.principal_moments[i],
            home.principal_moments[i],
            1e-11,
            &format!("principal moment {i} is rotation-invariant"),
        );
    }
    // The axes themselves DID move: the rotated box's principal axes are the
    // rotated world axes, which is the other half of the invariance.
    let rotated_x = [rotation[0][0], rotation[1][0], rotation[2][0]];
    let aligned = (0..3).any(|k| {
        let dot: f64 = (0..3)
            .map(|i| moved.principal_axes[k][i] * rotated_x[i])
            .sum();
        dot.abs() > 1.0 - 1e-9
    });
    assert!(
        aligned,
        "one principal axis is the rotated x axis: {:?}",
        moved.principal_axes
    );
}

#[test]
fn a_unions_volume_is_at_most_the_sum_of_its_operands() {
    let mut arena = BrepArena::new();
    let a = block(&mut arena, 0.01, 0.01, 0.01);
    let b = cylinder(&mut arena, 0.005, 0.005, 0.005, 0.004, 0.02);
    let va = mass_properties(&arena, a, 1.0).expect("a").volume;
    let vb = mass_properties(&arena, b, 1.0).expect("b").volume;
    let union = boolean_op(&mut arena, a, b, BoolOp::Union).expect("union");
    let vu = mass_properties(&arena, union, 1.0).expect("union").volume;
    assert!(
        vu <= va + vb + 1e-15 * (va + vb),
        "union volume {vu:e} exceeds {va:e} + {vb:e}"
    );
    // …and it is strictly less, because these two overlap.
    assert!(
        vu < va + vb,
        "these operands overlap, so the union is smaller"
    );
}

#[test]
fn density_must_be_positive_and_finite() {
    let mut arena = BrepArena::new();
    let s = block(&mut arena, 0.01, 0.01, 0.01);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            mass_properties(&arena, s, bad).is_err(),
            "density {bad} must be refused, not scaled by"
        );
    }
}

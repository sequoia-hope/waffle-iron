//! KV6d closed-torus RED oracles — full-turn revolve of a circle profile
//! (spec `specs/kv6d_closed_torus_revolve.md`).
//!
//! A circle profile strictly off-axis revolved exactly 360° builds the
//! CLOSED ring torus: the minimal CW structure of T² (Stroud 2006 §3.1.4
//! seam representation; Mäntylä Euler–Poincaré with genus):
//!
//! - V = 1 — seam anchor at the outer equator (θ = 0, φ = 0)
//! - E = 2 — poloidal PROFILE circle (radius r) + toroidal OUTER-EQUATOR
//!   circle (radius R + r), both closed through the anchor
//! - F = 1 — one `Surface::Torus` face, outer loop = the aba⁻¹b⁻¹ square
//!   (4 half-edges; BOTH twin pairs internal to the loop)
//! - χ = V − E + F − R = 0 = 2(S − G) with G = 1
//!
//! Oracle groups: topology census, Pappus volume 2π²Rr² via the render
//! mesh, watertightness, determinism, rejection branches (on-axis sphere
//! wall typed; off-center crossing stays an ERROR), and boolean re-entry
//! (meridian-plane half-cut: volume exactly halves, ring severed to χ = 2).

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Curve, KernelV2Error,
    Profile, RenderMesh, RevolveResult, Surface,
};

// =========================================================================
// Fixtures
// =========================================================================

/// Major radius (axis → tube center) and minor (tube) radius.
const R_MAJ: f64 = 3.0;
const R_MIN: f64 = 1.0;

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// Circle in the XY plane, center at radial 3 from the x-axis, minor r=1 —
/// the same tube as `kv6d_partial_torus_revolve_validates`, closed.
fn circle_profile() -> Profile {
    Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, R_MAJ),
        R_MIN,
    )
    .expect("circle profile")
}

fn revolve_closed_torus(arena: &mut BrepArena) -> RevolveResult {
    let profile = circle_profile();
    revolve(arena, &profile, AXIS_O, AXIS_D, 2.0 * PI)
        .unwrap_or_else(|e| panic!("closed-torus revolve failed: {e:?}"))
}

// =========================================================================
// Shared oracle helpers (same conventions as kv6a_revolve.rs)
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
                continue;
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

// =========================================================================
// 1. Topology + validation census
// =========================================================================

#[test]
fn closed_torus_topology_census() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_torus(&mut arena);

    let report = validate_solid(&arena, r.solid).expect("closed torus validates");
    assert_eq!(report.vertices, 1, "one seam anchor vertex");
    assert_eq!(
        report.edges, 2,
        "poloidal profile circle + toroidal equator"
    );
    assert_eq!(report.faces, 1, "one closed torus face");
    assert_eq!(report.rings, 0);
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 1, "the closed ring is genus 1");
    assert_eq!(report.euler_lhs, 0, "V−E+F−R = 0");
    assert_eq!(report.euler_rhs, 0, "2(S−G) = 0");

    // Result shape: no caps (nothing planar to name), one torus wall.
    assert!(r.start_cap.is_none(), "closed torus has no start cap");
    assert!(r.end_cap.is_none(), "closed torus has no end cap");
    assert_eq!(r.walls.len(), 1, "one lateral face");

    // Surface parameters.
    let face = arena.face(r.walls[0]).expect("torus face");
    let Some(Surface::Torus {
        center,
        axis_dir,
        major_radius,
        minor_radius,
        reversed,
    }) = face.surface
    else {
        panic!("wall is not a Surface::Torus: {:?}", face.surface);
    };
    assert_eq!((center.x(), center.y(), center.z()), (0.0, 0.0, 0.0));
    assert_eq!((axis_dir.x, axis_dir.y, axis_dir.z), (1.0, 0.0, 0.0));
    assert_eq!(major_radius, R_MAJ);
    assert_eq!(minor_radius, R_MIN);
    assert!(!reversed, "constructor torus is the solid sense");

    // The two seam circles: profile circle (radius r, center = embedded
    // profile center) + outer equator (radius R+r, center = torus center).
    let hes = arena.loop_half_edges(face.outer_loop).expect("loop");
    assert_eq!(hes.len(), 4, "aba⁻¹b⁻¹ square: 4 half-edges");
    let mut prof = 0usize;
    let mut equator = 0usize;
    for &h in &hes {
        let he = arena.half_edge(h).expect("half-edge");
        // Every half-edge starts (and ends) at the single anchor vertex.
        let v = arena.vertex(he.origin).expect("anchor").point;
        assert_eq!(
            (v.x(), v.y(), v.z()),
            (0.0, R_MAJ + R_MIN, 0.0),
            "anchor at the outer equator, θ=0 φ=0"
        );
        match he.curve {
            Curve::Circle { radius, center, .. } if radius == R_MIN => {
                prof += 1;
                assert_eq!(
                    (center.x(), center.y(), center.z()),
                    (0.0, R_MAJ, 0.0),
                    "profile circle centered on the tube center"
                );
            }
            Curve::Circle { radius, center, .. } if radius == R_MAJ + R_MIN => {
                equator += 1;
                assert_eq!(
                    (center.x(), center.y(), center.z()),
                    (0.0, 0.0, 0.0),
                    "equator circle centered on the axis"
                );
            }
            other => panic!("unexpected seam curve {other:?}"),
        }
    }
    assert_eq!((prof, equator), (2, 2), "each seam circle traversed twice");

    // Both twin pairs are internal to the single loop.
    for &h in &hes {
        let he = arena.half_edge(h).expect("half-edge");
        assert!(
            hes.contains(&he.twin),
            "twin of a seam half-edge lives in the same loop"
        );
        assert_ne!(he.twin, h, "twin is a distinct half-edge");
    }
}

// =========================================================================
// 2. Render mesh: watertight + Pappus volume
// =========================================================================

#[test]
fn closed_torus_mesh_watertight_with_pappus_volume() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_torus(&mut arena);

    let mesh = tessellate(&arena, r.solid).expect("closed torus tessellates");
    assert_mesh_sane(&mesh, "closed torus");
    assert_watertight(&mesh, "closed torus");

    // Pappus: V = 2π²·R·r².
    let exact = 2.0 * PI * PI * R_MAJ * R_MIN * R_MIN;
    let vol = mesh_signed_volume(&mesh);
    assert!(vol > 0.0, "outward orientation (positive signed volume)");
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "closed torus mesh volume {vol} vs analytic {exact} (5% facet band)"
    );
}

// =========================================================================
// 3. Determinism
// =========================================================================

#[test]
fn closed_torus_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let r = revolve_closed_torus(&mut arena);
        let mesh = tessellate(&arena, r.solid).expect("tessellates");
        (arena, mesh.positions, mesh.indices)
    };
    let (a1, p1, i1) = build();
    let (a2, p2, i2) = build();
    assert_eq!(a1, a2, "arena bit-identical across builds");
    assert_eq!(p1, p2, "mesh positions bit-identical");
    assert_eq!(i1, i2, "mesh indices bit-identical");
}

// =========================================================================
// 4. Rejection branches
// =========================================================================

/// A full-turn circle CENTERED ON the axis sweeps a SPHERE — KV6d
/// increment 2, still a typed capability wall (C0067), distinct from the
/// retired closed-torus wall.
#[test]
fn on_axis_circle_full_turn_stays_walled_sphere() {
    let mut arena = BrepArena::new();
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0), // center ON the x-axis
        1.0,
    )
    .expect("on-axis circle profile");
    let err = revolve(&mut arena, &circle, AXIS_O, AXIS_D, 2.0 * PI)
        .expect_err("on-axis full-turn circle → sphere, walled");
    assert_eq!(err, KernelV2Error::RevolveOnAxisCircleUnsupported);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

/// A full-turn circle CROSSING the axis off-center stays invalid input
/// (self-intersecting sweep), exactly like the partial-angle branch.
#[test]
fn crossing_circle_full_turn_rejected_as_error() {
    let mut arena = BrepArena::new();
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.5), // center 0.5 off-axis, radius 1 → crossing
        1.0,
    )
    .expect("crossing circle profile");
    let err = revolve(&mut arena, &circle, AXIS_O, AXIS_D, 2.0 * PI)
        .expect_err("crossing circle → invalid input");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

/// The partial-angle circle path is untouched by the full-turn work: same
/// topology as `kv6d_partial_torus_revolve_validates` pins.
#[test]
fn partial_torus_path_unchanged() {
    let mut arena = BrepArena::new();
    let profile = circle_profile();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, PI / 2.0)
        .expect("partial torus still builds");
    let report = validate_solid(&arena, r.solid).expect("partial torus validates");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.genus),
        (2, 3, 3, 0)
    );
}

// =========================================================================
// 5. Boolean re-entry: meridian-plane half cut (mini-C0065)
// =========================================================================

/// Subtracting a half-space box bounded by the meridian plane y = 0 (a
/// plane CONTAINING the torus axis) removes exactly half the ring: the
/// intersection curves are two poloidal circles (analytic), the result is
/// a C-shaped bar (genus 0, χ = 2) of exactly half the Pappus volume.
#[test]
fn closed_torus_boolean_meridian_half_cut() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_torus(&mut arena);

    // Cutter: box x∈[−6,6], y∈[−6,0], z∈[−6,6] (contains the y ≤ 0 half).
    let cutter_profile = Profile::new(
        Point3::new(0.0, 0.0, -6.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(-6.0, -6.0),
            Point2::new(6.0, -6.0),
            Point2::new(6.0, 0.0),
            Point2::new(-6.0, 0.0),
        ],
        vec![],
    )
    .expect("cutter profile");
    let cutter = extrude(
        &mut arena,
        &cutter_profile,
        Vector3::new(0.0, 0.0, 12.0),
        12.0,
    )
    .expect("cutter box");

    let out = boolean_op(&mut arena, r.solid, cutter.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("torus − half-space subtract failed: {e:?}"));

    let report = validate_solid(&arena, out).expect("half ring validates");
    assert_eq!(report.shells, 1, "one connected half ring");
    assert_eq!(report.genus, 0, "severed ring is genus 0");
    assert_eq!(report.euler_lhs, report.euler_rhs);
    assert_eq!(report.euler_lhs, 2, "χ = 2");

    let mesh = tessellate(&arena, out).expect("half ring tessellates");
    assert_mesh_sane(&mesh, "half ring");
    assert_watertight(&mesh, "half ring");
    let exact = PI * PI * R_MAJ * R_MIN * R_MIN; // half of 2π²Rr²
    let vol = mesh_signed_volume(&mesh);
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "half-ring mesh volume {vol} vs analytic {exact} (5% facet band)"
    );
}

// =========================================================================
// 6. Adversary: near-tangent narrow shaft stays a LOUD typed stop (C0065)
// =========================================================================

/// The C0065 configuration: a vertical square shaft whose outer wall
/// (x = 1.45) is near-tangent to the outer equator (ρ = 1.5) — the gap
/// (0.05) is comparable to the Stage-1 chord sagitta, so the inscribed
/// mesh's intersection oval closes EARLY (entirely inside the bounded
/// wall) and Stage-4 implicit-pair relocation would drag it onto the
/// infinite-surface curve OUTSIDE the wall face, minting a phantom
/// overlapping lens shell (silent WRONG geometry). The bounded-face
/// containment guard must stop this typed — never emit the double cover.
/// (The honest conversion is the §4.3.3 near-tangency increment.)
#[test]
fn closed_torus_near_tangent_shaft_stays_loud() {
    let mut arena = BrepArena::new();
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(0.5, -1.2),
        0.3,
    )
    .expect("circle profile");
    let r = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .expect("closed torus builds");
    let shaft_profile = Profile::new(
        Point3::new(0.0, 0.0, -1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.95, -0.25),
            Point2::new(1.45, -0.25),
            Point2::new(1.45, 0.25),
            Point2::new(0.95, 0.25),
        ],
        vec![],
    )
    .expect("shaft profile");
    let shaft =
        extrude(&mut arena, &shaft_profile, Vector3::new(0.0, 0.0, 3.0), 3.0).expect("shaft box");
    let err = boolean_op(&mut arena, r.solid, shaft.solid, BoolOp::Subtract)
        .expect_err("near-tangent narrow shaft must stop typed, not emit a double cover");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("OffCurveBeyondChordBand") || msg.contains("LocalRefinementRequired"),
        "expected a Stage-4 typed stop, got {msg}"
    );
}

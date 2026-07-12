//! KV6d increment 2 oracles — full-turn revolve of an ON-AXIS circle
//! profile builds the CLOSED sphere (spec `specs/kv6d_sphere_revolve.md`).
//!
//! Minimal seam structure of S² (the PR-YR12 yang contract mirrored into
//! the arena; Stroud 2006 §3.1.4 seam representation):
//!
//! - V = 2 — south/north poles at `center ∓ r·ẑ` (CANONICAL world z-up,
//!   regardless of the revolve axis — the sphere is isotropic)
//! - E = 1 — one meridian seam `Curve::Arc` twin pair on the X–Z great
//!   circle through `center + r·x̂`
//! - F = 1 — one `Surface::Sphere` face, outer loop = the twin pair
//! - χ = V − E + F − R = 2 = 2(S − G) with G = 0
//!
//! Oracle groups: topology census, volume 4/3·π·r³ via the render mesh,
//! watertightness, determinism, rejection branches (partial-angle on-axis
//! and off-center crossing stay ERRORS), and boolean re-entry (equatorial
//! half-cut: volume exactly halves; the surviving sphere face is a
//! longitude-WRAPPING pole-cap patch — the C0067 render mechanism).

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Curve, KernelV2Error,
    Profile, RenderMesh, RevolveResult, Surface,
};

// =========================================================================
// Fixtures
// =========================================================================

/// Sphere radius; the profile circle sits centered ON the revolve axis.
const R: f64 = 1.0;
/// Sphere center: on the x-axis at x = 5 (poles at z = ±1 — canonical
/// z-up even though the revolve axis is X).
const CX: f64 = 5.0;

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// Circle in the XY plane, centered at (5, 0, 0) ON the x-axis, radius 1.
fn on_axis_circle() -> Profile {
    Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(CX, 0.0),
        R,
    )
    .expect("on-axis circle profile")
}

fn revolve_closed_sphere(arena: &mut BrepArena) -> RevolveResult {
    let profile = on_axis_circle();
    revolve(arena, &profile, AXIS_O, AXIS_D, 2.0 * PI)
        .unwrap_or_else(|e| panic!("closed-sphere revolve failed: {e:?}"))
}

// =========================================================================
// Shared oracle helpers (same conventions as kv6d_closed_torus.rs)
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
fn closed_sphere_topology_census() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_sphere(&mut arena);

    let report = validate_solid(&arena, r.solid).expect("closed sphere validates");
    assert_eq!(report.vertices, 2, "two pole vertices");
    assert_eq!(report.edges, 1, "one meridian seam edge");
    assert_eq!(report.faces, 1, "one closed sphere face");
    assert_eq!(report.rings, 0);
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0, "the ball is genus 0");
    assert_eq!(report.euler_lhs, 2, "V−E+F−R = 2");
    assert_eq!(report.euler_rhs, 2, "2(S−G) = 2");

    // Result shape: no caps, one sphere wall.
    assert!(r.start_cap.is_none(), "closed sphere has no start cap");
    assert!(r.end_cap.is_none(), "closed sphere has no end cap");
    assert_eq!(r.walls.len(), 1, "one lateral face");

    // Surface parameters: center snapped ONTO the axis.
    let face = arena.face(r.walls[0]).expect("sphere face");
    let Some(Surface::Sphere {
        center,
        radius,
        reversed,
    }) = face.surface
    else {
        panic!("wall is not a Surface::Sphere: {:?}", face.surface);
    };
    assert_eq!((center.x(), center.y(), center.z()), (CX, 0.0, 0.0));
    assert_eq!(radius, R);
    assert!(!reversed, "constructor sphere is the solid sense");

    // Seam: one Arc twin pair, poles at center ± r·ẑ (canonical z-up even
    // though the revolve axis is X), normals exactly ∓ŷ.
    let hes = arena.loop_half_edges(face.outer_loop).expect("loop");
    assert_eq!(hes.len(), 2, "seam twin pair: 2 half-edges");
    let h0 = arena.half_edge(hes[0]).expect("seam_fwd");
    let h1 = arena.half_edge(hes[1]).expect("seam_back");
    assert_eq!(h0.twin, hes[1], "the pair are twins");
    assert_eq!(h1.twin, hes[0]);
    let p0 = arena.vertex(h0.origin).expect("south").point;
    let p1 = arena.vertex(h1.origin).expect("north").point;
    assert_eq!((p0.x(), p0.y(), p0.z()), (CX, 0.0, -R), "south pole");
    assert_eq!((p1.x(), p1.y(), p1.z()), (CX, 0.0, R), "north pole");
    for (h, want_ny) in [(h0, -1.0), (h1, 1.0)] {
        let Curve::Arc {
            center: ac,
            normal,
            radius: ar,
        } = h.curve
        else {
            panic!("seam half-edge is not an Arc: {:?}", h.curve);
        };
        assert_eq!((ac.x(), ac.y(), ac.z()), (CX, 0.0, 0.0));
        assert_eq!(ar, R);
        assert_eq!(
            (normal.x, normal.y, normal.z),
            (0.0, want_ny, 0.0),
            "meridian seam normal ∓ŷ (twin-negated)"
        );
    }
}

// =========================================================================
// 2. Render mesh: watertight + ball volume
// =========================================================================

#[test]
fn closed_sphere_mesh_watertight_with_ball_volume() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_sphere(&mut arena);

    let mesh = tessellate(&arena, r.solid).expect("closed sphere tessellates");
    assert_mesh_sane(&mesh, "closed sphere");
    assert_watertight(&mesh, "closed sphere");

    let exact = 4.0 / 3.0 * PI * R * R * R;
    let vol = mesh_signed_volume(&mesh);
    assert!(vol > 0.0, "outward orientation (positive signed volume)");
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "closed sphere mesh volume {vol} vs analytic {exact} (5% facet band)"
    );
    // Every render vertex is exactly on the sphere.
    for p in mesh.positions.chunks_exact(3) {
        let d = ((p[0] - CX) * (p[0] - CX) + p[1] * p[1] + p[2] * p[2]).sqrt();
        assert!(
            (d - R).abs() < 1e-12,
            "render vertex off the sphere: {p:?} (|d−r| = {})",
            (d - R).abs()
        );
    }
}

// =========================================================================
// 3. Determinism
// =========================================================================

#[test]
fn closed_sphere_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let r = revolve_closed_sphere(&mut arena);
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
// 4. Rejection branches (unchanged semantics)
// =========================================================================

/// A PARTIAL-angle revolve of an on-axis circle is a self-intersecting
/// sweep — INVALID INPUT, exactly as before this increment.
#[test]
fn partial_on_axis_circle_still_rejected() {
    let mut arena = BrepArena::new();
    let profile = on_axis_circle();
    let err = revolve(&mut arena, &profile, AXIS_O, AXIS_D, PI / 2.0)
        .expect_err("partial on-axis circle revolve is invalid input");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

/// A full-turn circle CROSSING the axis off-center stays invalid input.
#[test]
fn crossing_circle_full_turn_still_rejected() {
    let mut arena = BrepArena::new();
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.5), // center 0.5 off-axis, radius 1 → crossing
        R,
    )
    .expect("crossing circle profile");
    let err = revolve(&mut arena, &circle, AXIS_O, AXIS_D, 2.0 * PI)
        .expect_err("crossing circle full-turn revolve is invalid input");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

// =========================================================================
// 5. Boolean re-entry: equatorial half-cut (the C0067 mechanism)
// =========================================================================

/// Sphere − lower half-space box: the volume exactly halves (2/3·π·r³) and
/// the surviving sphere face is a longitude-WRAPPING pole-cap patch (its
/// boundary is the equator; the region contains the north pole) — the
/// to_yang closed-sphere emission, plane×sphere Stage 3, from_yang
/// `FaceSurf::Sphere`, and the pole-bridged UV-CDT render all in one chain.
#[test]
fn closed_sphere_boolean_equatorial_half_cut() {
    let mut arena = BrepArena::new();
    let r = revolve_closed_sphere(&mut arena);

    // Cutter: box x∈[3,7], y∈[−2,2], z∈[−2,0] (contains the z ≤ 0 half).
    let cutter_profile = Profile::new(
        Point3::new(0.0, 0.0, -2.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(3.0, -2.0),
            Point2::new(7.0, -2.0),
            Point2::new(7.0, 2.0),
            Point2::new(3.0, 2.0),
        ],
        vec![],
    )
    .expect("cutter profile");
    let cutter = extrude(
        &mut arena,
        &cutter_profile,
        Vector3::new(0.0, 0.0, 2.0),
        2.0,
    )
    .expect("cutter box");

    let out = boolean_op(&mut arena, r.solid, cutter.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("sphere − half-space subtract failed: {e:?}"));

    let report = validate_solid(&arena, out).expect("hemisphere validates");
    assert_eq!(report.shells, 1, "one connected hemisphere");
    assert_eq!(report.genus, 0, "hemisphere is genus 0");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&arena, out).expect("hemisphere tessellates");
    assert_mesh_sane(&mesh, "hemisphere");
    assert_watertight(&mesh, "hemisphere");
    let exact = 2.0 / 3.0 * PI * R * R * R;
    let vol = mesh_signed_volume(&mesh);
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "hemisphere mesh volume {vol} vs analytic {exact} (5% facet band)"
    );
}

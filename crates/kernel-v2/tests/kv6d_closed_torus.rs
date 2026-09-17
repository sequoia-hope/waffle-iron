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

/// T-junction-aware watertightness (the assay `watertight_mesh` rule): edges
/// that pair exactly are closed; the residue is subdivided at every residue
/// vertex lying ON a residue edge (within 1e-9), then must cancel.
fn assert_watertight_tjunction_aware(mesh: &RenderMesh, what: &str) {
    use std::collections::{BTreeMap, BTreeSet};
    let q = |x: f64| (x / 1e-9).round() as i64;
    let pos = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let key = |p: [f64; 3]| (q(p[0]), q(p[1]), q(p[2]));
    let mut count: BTreeMap<_, i64> = BTreeMap::new();
    let mut at: BTreeMap<_, [f64; 3]> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (pa, pb) = (pos(a), pos(b));
            let (ka, kb) = (key(pa), key(pb));
            if ka == kb {
                continue;
            }
            at.entry(ka).or_insert(pa);
            at.entry(kb).or_insert(pb);
            *count.entry((ka, kb)).or_insert(0) += 1;
            *count.entry((kb, ka)).or_insert(0) -= 1;
        }
    }
    let residue: Vec<_> = count
        .iter()
        .filter(|(_, &c)| c != 0)
        .map(|(e, &c)| (*e, c))
        .collect();
    let verts: BTreeSet<_> = residue.iter().flat_map(|((a, b), _)| [*a, *b]).collect();
    let mut sub: BTreeMap<_, i64> = BTreeMap::new();
    for ((ka, kb), c) in residue {
        let (pa, pb) = (at[&ka], at[&kb]);
        let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let mut on: Vec<(f64, _)> = vec![(0.0, ka), (1.0, kb)];
        for &kv in &verts {
            if kv == ka || kv == kb {
                continue;
            }
            let p = at[&kv];
            let w = [p[0] - pa[0], p[1] - pa[1], p[2] - pa[2]];
            let t = (w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / len2;
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let perp = [w[0] - t * d[0], w[1] - t * d[1], w[2] - t * d[2]];
            if (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= 1e-9 {
                on.push((t, kv));
            }
        }
        on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        for w in on.windows(2) {
            *sub.entry((w[0].1, w[1].1)).or_insert(0) += c;
            *sub.entry((w[1].1, w[0].1)).or_insert(0) -= c;
        }
    }
    let open = sub.values().filter(|&&c| c != 0).count();
    assert_eq!(
        open, 0,
        "{what}: {open} unpaired directed edges after T-junction subdivision"
    );
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

/// A full-turn circle CENTERED ON the axis sweeps a SPHERE — supported
/// since KV6d increment 2 (spec `kv6d_sphere_revolve.md`, detailed census
/// in `tests/kv6d_sphere_revolve.rs`); here just pin that the branch
/// BUILDS and validates (it was this suite's typed wall before).
#[test]
fn on_axis_circle_full_turn_builds_sphere() {
    let mut arena = BrepArena::new();
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0), // center ON the x-axis
        1.0,
    )
    .expect("on-axis circle profile");
    let r = revolve(&mut arena, &circle, AXIS_O, AXIS_D, 2.0 * PI)
        .expect("on-axis full-turn circle builds a closed sphere");
    assert_eq!(r.walls.len(), 1, "one sphere face");
    assert!(matches!(
        arena.face(r.walls[0]).unwrap().surface,
        Some(Surface::Sphere { .. })
    ));
    validate_solid(&arena, r.solid).expect("closed sphere validates");
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
// 6. The near-tangent narrow shaft (C0065) CONVERTS: §4.5.2 + Slice F-4
// =========================================================================

/// The C0065 configuration: a vertical square shaft whose outer wall
/// (x = 1.45) grazes the outer equator (ρ = 1.5) 0.05 deep — comparable to
/// the Stage-1 chord sagitta, so the inscribed mesh's intersection oval
/// closes EARLY (entirely inside the bounded wall) and the Stage-4 relocation
/// leaves the wall face: `OffCurveBeyondChordBand`, the typed STOP this test
/// pinned until 2026-09-17. That STOP is Yang §4.5.2's own trigger; the
/// always-on op-level refinement pass (`yang-rs::boolean::refine_452`)
/// re-tessellates at d_ε/4, the loop reaches the clip walls, and the output
/// is the genus-2 through-slot. Full oracle: `tests/kv6d_c0065_through_slot.rs`;
/// here only that the boolean no longer stops and validates as one shell.
#[test]
fn closed_torus_near_tangent_shaft_converts_through_452() {
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
    let out = boolean_op(&mut arena, r.solid, shaft.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("the §4.5.2 pass converts the grazing shaft: {e:?}"));
    let report = validate_solid(&arena, out).expect("slotted torus validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.faces, 5, "torus + the four shaft walls");
    assert_eq!(
        report.rings, 1,
        "the torus face carries its second window as a ring"
    );
}

// =========================================================================
// 7. Boolean re-entry — the R0050 torus∩conic corner (2026-09-12)
// =========================================================================

/// R0050's op-2 shape: a PARTIAL torus (two meridian cap planes) cut out of
/// a cylinder whose axis is parallel to the torus axis. Each cap plane ∥
/// the cylinder axis meets the lateral in a ruling LINE (a conic-arm edge),
/// and that line meets the torus∩cylinder pair curve at a {cylinder, cap
/// plane, torus} corner. yang's torus block used to STOP on that vertex as
/// a "torus-edge endpoint that is also a conic endpoint" — the general
/// triple block never scanned torus edges (they populate no conic map) —
/// while with exactly three incident surfaces it is the plain three-surface
/// corner both blocks already solve (spec `yang_stage4_conic_triple_junction`,
/// "Junction-map candidates").
///
/// Geometry: tube centre radius 3, tube radius 1 about the x-axis, swept
/// 40° between the azimuths 160° and 200° (measured from +y toward +z); the
/// cylinder r = 2 has its axis at (y, z) = (−4.4, 0), so the tube lies
/// inside it except a sliver (its near wall at radial 2.4 at φ = 180°) and
/// each cap plane — 1.505 from the cylinder axis — cuts the lateral in a
/// ruling at radial s = 2.817 (the root of |s·û − C|² = 4 inside [2, 4]).
#[test]
fn partial_torus_cap_rulings_meet_the_tube_on_a_cylinder() {
    let mut arena = BrepArena::new();
    const PHI0: f64 = 160.0 * PI / 180.0;
    const SWEEP: f64 = 40.0 * PI / 180.0;
    const RC: f64 = 2.0;
    const CY: f64 = -4.4;
    // Torus segment: the profile circle sits in the meridian plane at
    // azimuth φ₀ (u = x̂, v = the radial direction at φ₀), swept by +40°
    // about +x (from φ₀ toward φ₀ + 40°).
    let v0 = Vector3::new(0.0, PHI0.cos(), PHI0.sin());
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        v0,
        Point2::new(0.0, R_MAJ),
        R_MIN,
    )
    .expect("tube profile at φ₀");
    let seg = revolve(&mut arena, &profile, AXIS_O, AXIS_D, SWEEP).expect("40° torus segment");
    let seg_report = validate_solid(&arena, seg.solid).expect("segment validates");
    assert_eq!(seg_report.faces, 3, "tube + two caps");

    // Cylinder r = 2, axis x through (y, z) = (CY, 0), x ∈ [−3, 3].
    let cyl_profile = Profile::circle(
        Point3::new(-3.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(CY, 0.0),
        RC,
    )
    .expect("cylinder profile");
    let cyl =
        extrude(&mut arena, &cyl_profile, Vector3::new(1.0, 0.0, 0.0), 6.0).expect("cylinder");
    let cyl_mesh = tessellate(&arena, cyl.solid).expect("cylinder tessellates");
    let cyl_vol = mesh_signed_volume(&cyl_mesh);

    let out = boolean_op(&mut arena, cyl.solid, seg.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cylinder − torus segment subtract failed: {e:?}"));

    let report = validate_solid(&arena, out).expect("bitten cylinder validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.genus, 0, "a pocket through the wall is genus 0");
    assert_eq!(report.euler_lhs, report.euler_rhs);
    assert_eq!(
        report.faces, 6,
        "lateral (windowed) + two rims + tube + two cap faces"
    );

    let mesh = tessellate(&arena, out).expect("bitten cylinder tessellates");
    assert_mesh_sane(&mesh, "bitten cylinder");
    // The windowed lateral's chart refinement subdivides its boundary edges
    // (the ruling into 8, each pair-curve chord into 4) while the cap plane
    // and the tube keep the raw polyline — kernel-v2's documented one-sided
    // collinear subdivision (PR-TH1), so the pairing is T-junction-aware
    // like the assay's `watertight_mesh` oracle: a residue edge is split at
    // every residue vertex lying on it before directed edges must cancel.
    assert_watertight_tjunction_aware(&mesh, "bitten cylinder");

    // The four exact corners are output vertices: on each cap plane the
    // ruling at radial s solves s² − 2s(û·C) + |C|² − RC² = 0 (the root in
    // [2, 4]); the tube meets it at x = ±√(R_MIN² − (s − R_MAJ)²).
    for phi in [PHI0, PHI0 + SWEEP] {
        let u = [phi.cos(), phi.sin()];
        let uc = u[0] * CY; // û·C with C = (CY, 0)
        let disc = uc * uc - (CY * CY - RC * RC);
        assert!(disc > 0.0, "the cap plane at {phi} must cut the lateral");
        let s = uc - disc.sqrt();
        assert!(
            (R_MAJ - R_MIN..=R_MAJ + R_MIN).contains(&s),
            "ruling s = {s}"
        );
        let x = (R_MIN * R_MIN - (s - R_MAJ).powi(2)).sqrt();
        for sx in [1.0, -1.0] {
            let c = [sx * x, s * u[0], s * u[1]];
            let hit = mesh.positions.chunks_exact(3).any(|p| {
                ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()
                    <= 1e-9
            });
            assert!(hit, "exact corner {c:?} is not an output vertex");
        }
    }

    // Volume: the bite removes the part of the segment inside the cylinder —
    // between half and all of its Pappus volume (40/360 · 2π²Rr²).
    let segment = SWEEP / (2.0 * PI) * 2.0 * PI * PI * R_MAJ * R_MIN * R_MIN;
    let vol = mesh_signed_volume(&mesh);
    let removed = cyl_vol - vol;
    assert!(
        removed >= 0.5 * segment && removed <= segment,
        "removed {removed} vs segment Pappus {segment} (cylinder {cyl_vol}, out {vol})"
    );
}

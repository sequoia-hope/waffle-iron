//! The GENERAL on-axis lathe: a full-turn revolve of any simple polygon with
//! one on-axis edge (`docs/notes/eiffel/FEATURE_NOTES.md` §1).
//!
//! Before it, `on_axis_revolve` took a 3-gon (apex cone) or a 4-gon
//! (cylinder / frustum) and nothing else, so the most natural lathe shape
//! there is — a dome polyline touching the axis at both ends — was a typed
//! `RevolveAxisIntersectsProfile` and had to ship as stacked frusta.
//!
//! Oracles. Every shape here is a solid of revolution whose volume is known
//! in closed form (Pappus, band by band), so the oracle is the VOLUME, not a
//! census:
//!
//! 1. Shapes the old builders refused now build, with the analytic volume,
//!    a watertight mesh and χ = 2: a 6-band dome, a stepped shaft, the
//!    bicone, the "pencil" (cylinder + tip), a cup with a BLIND BORE (whose
//!    bore wall faces the axis — the `reversed` cavity sense).
//! 2. Orientation: every mesh integrates to a POSITIVE volume. An
//!    inside-out lathe satisfies the per-face invariant and fails this.
//! 3. The census is V − E + F = 2 at every band count.
//! 4. The shapes the SPECIFIC builders own are untouched — same face counts,
//!    same volumes — so this is a widening, not a rewrite.
//! 5. What is still refused is refused loudly: a partial sweep of a
//!    many-band profile, a profile touching the axis in two places, a
//!    zero-area profile.

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    revolve, tessellate, validate_solid, BrepArena, KernelV2Error, Profile, RenderMesh, SolidId,
};

/// Revolve a profile given in the (radius, axial) half-plane: `s` along +X,
/// `t` along +Z, about the Z axis through the origin, a full turn.
fn lathe(arena: &mut BrepArena, ring: &[(f64, f64)]) -> Result<SolidId, KernelV2Error> {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        ring.iter().map(|&(s, t)| Point2::new(s, t)).collect(),
        vec![],
    )
    .expect("profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .map(|r| r.solid)
}

fn mesh_volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize;
        [
            m.positions[3 * i],
            m.positions[3 * i + 1],
            m.positions[3 * i + 2],
        ]
    };
    m.indices
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
        })
        .sum()
}

/// Every edge of the mesh is used exactly twice, in opposite directions.
fn watertight(m: &RenderMesh) -> bool {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    let q = |i: u32| -> [i64; 3] {
        let i = i as usize;
        let r = |x: f64| (x * 1e9).round() as i64;
        [
            r(m.positions[3 * i]),
            r(m.positions[3 * i + 1]),
            r(m.positions[3 * i + 2]),
        ]
    };
    // Weld by position: a seam vertex may be duplicated per face.
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut id_of = |i: u32| -> u32 {
        let key = q(i);
        let next = ids.len() as u32;
        *ids.entry(key).or_insert(next)
    };
    for tri in m.indices.chunks(3) {
        let v: Vec<u32> = tri.iter().map(|&i| id_of(i)).collect();
        for k in 0..3 {
            let (a, b) = (v[k], v[(k + 1) % 3]);
            if a == b {
                continue; // degenerate sliver at a pole
            }
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += if a < b { 1 } else { -1 };
        }
    }
    counts.values().all(|&c| c == 0)
}

/// The analytic volume of a solid of revolution from its (radius, axial)
/// profile: Pappus band by band, ∫πs²dt over the closed polygon.
fn pappus(ring: &[(f64, f64)]) -> f64 {
    let n = ring.len();
    let mut v = 0.0;
    for i in 0..n {
        let (s0, t0) = ring[i];
        let (s1, t1) = ring[(i + 1) % n];
        // ∫ over the edge of π s² dt with s linear in t.
        v += PI * (s0 * s0 + s0 * s1 + s1 * s1) / 3.0 * (t1 - t0);
    }
    v.abs()
}

fn check(name: &str, ring: &[(f64, f64)], faces: usize) {
    let mut arena = BrepArena::new();
    let solid = lathe(&mut arena, ring).unwrap_or_else(|e| panic!("{name}: {e}"));
    validate_solid(&arena, solid).unwrap_or_else(|e| panic!("{name} validate: {e}"));
    let shells = &arena.solid(solid).expect("solid").shells;
    assert_eq!(shells.len(), 1, "{name}: one shell");
    let got_faces = arena.shell(shells[0]).expect("shell").faces.len();
    assert_eq!(got_faces, faces, "{name}: face count");
    let mesh = tessellate(&arena, solid).unwrap_or_else(|e| panic!("{name} tessellate: {e}"));
    assert!(watertight(&mesh), "{name}: mesh is not watertight");
    let (got, want) = (mesh_volume(&mesh), pappus(ring));
    assert!(got > 0.0, "{name}: volume {got} — the lathe is inside out");
    // Chord error on the swept circles: the mesh under-reports a curved
    // solid, so the band is the faceting, not the geometry.
    assert!(
        (got - want).abs() < 2e-3 * want,
        "{name}: volume {got} vs {want}"
    );
}

/// A dome: a quarter-ellipse polyline from the bottom rim up to the apex,
/// touching the axis at both ends. THE shape §1 is about.
fn dome(bands: usize, r: f64, h: f64) -> Vec<(f64, f64)> {
    let mut ring = vec![(0.0, 0.0), (r, 0.0)];
    for i in 1..=bands {
        let a = (i as f64 / bands as f64) * PI / 2.0;
        ring.push((r * a.cos(), h * a.sin()));
    }
    ring
}

#[test]
fn a_dome_polyline_revolves_as_one_solid() {
    // 6 bands: the bottom disc, 5 frusta and the apex cone = 7 faces.
    check("dome", &dome(6, 0.4, 0.6), 7);
    // And at other band counts, to pin that nothing is special about six.
    check("dome-3", &dome(3, 0.4, 0.6), 4);
    check("dome-12", &dome(12, 1.0, 1.0), 13);
}

#[test]
fn a_stepped_shaft_revolves_with_its_annulus_bands() {
    // Two cylinders of different radii, so the step between them is a
    // planar ANNULUS band — a middle band with two rims and no slant.
    let ring = &[
        (0.0, 0.0),
        (0.3, 0.0),
        (0.3, 0.5),
        (0.15, 0.5),
        (0.15, 0.9),
        (0.0, 0.9),
    ];
    // disc, cylinder, annulus, cylinder, disc.
    check("stepped shaft", ring, 5);
}

#[test]
fn a_cup_with_a_blind_bore_faces_its_bore_wall_inward() {
    // A(0,0) → rim → up the outside → across the top → DOWN the bore → the
    // bore floor. The bore wall's outward normal points AT the axis: the
    // `reversed` cavity sense, derived from the profile's winding.
    let (r_out, r_in, h, floor) = (0.3, 0.2, 0.8, 0.2);
    let ring = &[
        (0.0, 0.0),
        (r_out, 0.0),
        (r_out, h),
        (r_in, h),
        (r_in, floor),
        (0.0, floor),
    ];
    check("cup", ring, 5);

    // The bore wall is a cylinder of the bore radius, marked reversed.
    let mut arena = BrepArena::new();
    let solid = lathe(&mut arena, ring).expect("cup");
    let bore = arena
        .solid(solid)
        .expect("solid")
        .shells
        .iter()
        .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
        .filter_map(|f| match arena.face(f).expect("face").surface {
            Some(kernel_v2::Surface::Cylinder {
                radius, reversed, ..
            }) => Some((radius, reversed)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bore.len(), 2, "outer wall and bore wall");
    assert!(
        bore.contains(&(r_out, false)) && bore.contains(&(r_in, true)),
        "{bore:?}: the outer wall faces out, the bore faces the axis"
    );
}

#[test]
fn the_bicone_and_the_pencil_build() {
    // Bicone: a triangle whose BOTH connector edges are oblique — two apex
    // cones sharing one rim. The 3-gon builder turned this down.
    check("bicone", &[(0.0, -0.5), (0.4, 0.0), (0.0, 0.7)], 2);
    // Pencil: a cylinder with a conical tip — a 4-gon with one oblique cap
    // edge, which the 4-gon builder turned down.
    check(
        "pencil",
        &[(0.0, 0.0), (0.25, 0.0), (0.25, 0.6), (0.0, 0.9)],
        3,
    );
}

#[test]
fn the_census_is_a_ball_at_every_band_count() {
    let shaft = vec![
        (0.0, 0.0),
        (0.3, 0.0),
        (0.3, 0.5),
        (0.15, 0.5),
        (0.15, 0.9),
        (0.0, 0.9),
    ];
    for bands in [1usize, 2, 3, 5, 9, 0] {
        // `bands == 0` stands for the stepped shaft, whose middle ANNULUS
        // band carries an inner ring, so its census is 2 + R, not 2.
        let ring = if bands == 0 {
            shaft.clone()
        } else {
            dome(bands, 0.5, 0.5)
        };
        let mut arena = BrepArena::new();
        let solid = lathe(&mut arena, &ring).unwrap_or_else(|e| panic!("{bands} bands: {e}"));
        let shell = arena.solid(solid).expect("solid").shells[0];
        let faces = arena.shell(shell).expect("shell").faces.clone();
        let p = ring.len() - 2; // off-axis profile vertices
        let mut half_edges = 0usize;
        let mut rings = 0usize;
        let mut vertices = std::collections::BTreeSet::new();
        for &f in &faces {
            let face = arena.face(f).expect("face");
            rings += face.inner_loops.len();
            let loops: Vec<_> = std::iter::once(face.outer_loop)
                .chain(face.inner_loops.iter().copied())
                .collect();
            for l in loops {
                for h in arena.loop_half_edges(l).expect("loop") {
                    half_edges += 1;
                    vertices.insert(arena.half_edge(h).expect("he").origin);
                }
            }
        }
        let (v, e, f) = (vertices.len(), half_edges / 2, faces.len());
        // Euler–Poincaré for one shell of genus 0 with R rings:
        // V − E + F − R = 2. A washer band trades its seam ruling for a ring,
        // so both sides move together.
        assert_eq!(v, p, "{bands}: one vertex per off-axis profile vertex");
        assert_eq!(f, p + 1, "{bands}: one face per profile edge swept");
        assert_eq!(e, 2 * p - 1 - rings, "{bands}: rims + seam rulings");
        assert_eq!(
            v as i64 - e as i64 + f as i64 - rings as i64,
            2,
            "{bands} bands: χ"
        );
    }
}

#[test]
fn the_shapes_the_specific_builders_own_are_unchanged() {
    // Cylinder (4-gon, equal radii): the extrude-of-circle delegation —
    // 3 faces, exact volume.
    check(
        "cylinder",
        &[(0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (0.0, 1.0)],
        3,
    );
    // Frustum (4-gon, oblique): 3 faces.
    check(
        "frustum",
        &[(0.0, 0.0), (0.5, 0.0), (0.3, 1.0), (0.0, 1.0)],
        3,
    );
    // Apex cone (3-gon, one perpendicular cap): 2 faces.
    check("cone", &[(0.0, 0.0), (0.5, 0.0), (0.0, 1.0)], 2);
}

#[test]
fn what_is_still_refused_is_refused_loudly() {
    // A partial sweep of a many-band profile: the wedge vocabulary (two pie
    // caps + a swept face per edge) is not built, and is not guessed.
    let ring = dome(5, 0.4, 0.6);
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        ring.iter().map(|&(s, t)| Point2::new(s, t)).collect(),
        vec![],
    )
    .expect("profile");
    let mut arena = BrepArena::new();
    let before = arena.clone();
    let err = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        PI / 2.0,
    )
    .unwrap_err();
    assert!(
        matches!(err, KernelV2Error::RevolveAxisIntersectsProfile),
        "{err:?}"
    );
    assert_eq!(arena, before, "refused before anything was written");

    // Two separate axis touches (a dart whose tip and tail both sit ON the
    // axis) is not a single-on-axis-edge lathe: the two on-axis vertices are
    // not adjacent, so there is no on-axis EDGE.
    let mut arena = BrepArena::new();
    let err = lathe(
        &mut arena,
        &[(0.0, 0.0), (0.4, 0.5), (0.0, 1.0), (0.1, 0.5)],
    )
    .unwrap_err();
    assert!(
        matches!(err, KernelV2Error::RevolveAxisIntersectsProfile),
        "{err:?}"
    );
}

//! PR-KV2 RED oracles: planar profile → solid constructors.
//!
//! Property tests over the KV2 constructor surface (`Profile`,
//! `make_face_from_profile`, `extrude`, `geom::signed_volume`):
//!
//! - square → box: exact V/E/F/R/S/G table, exact volume `w·d·h`, 6 faces
//!   with outward normals (centroid test + exact axis set), `validate_solid`
//!   green;
//! - downward extrude (direction `−n`): the flip path still yields positive
//!   signed volume and outward base/top normals;
//! - L-shaped (concave) profile: exact hand-computed volume, planar walls,
//!   Euler holds (concave ≠ non-simple — must work);
//! - profile with one rectangular hole → through-hole: `G = 1`, ring counts,
//!   Euler LHS = RHS, exact volume `outer − hole`, hole-wall normals point
//!   INTO the hole (outward from material);
//! - rotated/oblique plane profile: same invariants — no axis-aligned
//!   shortcuts (orthonormal but irrational frame);
//! - lamina (`make_face_from_profile`): opposite-normal face pair, zero
//!   volume, holed lamina genus bookkeeping `(V,E,F,R,S,G) = (8,8,2,2,1,1)`;
//! - error paths: < 3 vertices, repeated consecutive vertex, non-simple
//!   polygon (the documented loud contract: EXACT dashu-rational check ⇒
//!   `ProfileNotSimple`), hole placement errors, degenerate basis, zero /
//!   negative distance, in-plane / zero direction — argument errors leave
//!   the arena untouched;
//! - determinism: identical inputs ⇒ identical arenas.
//!
//! Expected element counts for a prism over a `k`-gon: `V = 2k`, `E = 3k`,
//! `F = k + 2` ⇒ Euler LHS `2k − 3k + (k + 2) = 2` = RHS. With a through-hole
//! of `m` vertices: `V = 2(k+m)`, `E = 3(k+m)`, `F = k + m + 2`, `R = 2`,
//! `G = 1` ⇒ LHS `0` = RHS.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::geom::{face_centroid, signed_volume};
use kernel_v2::*;

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// Identity frame: profile plane = XY, normal +Z.
fn xy_frame() -> (Point3, Vector3, Vector3) {
    (
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
    )
}

fn square_profile(w: f64, d: f64) -> Profile {
    let (o, u, v) = xy_frame();
    Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(w, 0.0), p2(w, d), p2(0.0, d)],
        vec![],
    )
    .expect("square profile is valid")
}

#[track_caller]
fn assert_counts(arena: &BrepArena, solid: SolidId, expect: (i64, i64, i64, i64, i64, i64)) {
    let c = arena.euler_counts(solid).expect("euler_counts");
    assert_eq!(
        (c.v, c.e, c.f, c.r, c.s, c.g),
        expect,
        "element counts (V,E,F,R,S,G)"
    );
    assert!(
        c.holds(),
        "Euler–Poincaré formula violated: {} != {}",
        c.lhs(),
        c.rhs()
    );
}

/// Mean of all live vertices in the arena (tests build one solid per arena).
fn solid_centroid(arena: &BrepArena) -> Point3 {
    let mut s = [0.0f64; 3];
    let mut n = 0usize;
    for v in arena.vertices.iter().flatten() {
        let a = v.point.as_array();
        s[0] += a[0];
        s[1] += a[1];
        s[2] += a[2];
        n += 1;
    }
    Point3::new(s[0] / n as f64, s[1] / n as f64, s[2] / n as f64)
}

fn face_plane(arena: &BrepArena, face: FaceId) -> Plane {
    let f = arena.face(face).expect("face alive");
    let Some(Surface::Plane(plane)) = f.surface else {
        panic!("face {face:?} has no plane: {:?}", f.surface);
    };
    plane
}

/// Outward-normal oracle for convex solids: every face normal points away
/// from the solid centroid.
#[track_caller]
fn assert_outward_normals(arena: &BrepArena, solid: SolidId) {
    let c = solid_centroid(arena);
    let solid_ref = arena.solid(solid).expect("solid alive");
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            let plane = face_plane(arena, f);
            let fc = face_centroid(arena, f).expect("face centroid");
            let d = plane.normal.x * (fc.x() - c.x())
                + plane.normal.y * (fc.y() - c.y())
                + plane.normal.z * (fc.z() - c.z());
            assert!(
                d > 0.0,
                "face {f:?} normal {:?} not outward (dot {d})",
                plane.normal
            );
        }
    }
}

/// All loop vertices of `face` within `tol` of its stored plane (explicit
/// release-mode check; `validate_solid` only checks planarity in debug).
#[track_caller]
fn assert_face_planar(arena: &BrepArena, face: FaceId, tol: f64) {
    let plane = face_plane(arena, face);
    let f = arena.face(face).expect("face");
    let mut loops = vec![f.outer_loop];
    loops.extend(f.inner_loops.iter().copied());
    for lid in loops {
        for p in arena.loop_points(lid).expect("loop points") {
            let d = (p.x() - plane.point.x()) * plane.normal.x
                + (p.y() - plane.point.y()) * plane.normal.y
                + (p.z() - plane.point.z()) * plane.normal.z;
            assert!(
                d.abs() <= tol,
                "face {face:?} vertex {p:?} off-plane by {d}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Lamina (make_face_from_profile)
// ---------------------------------------------------------------------------

/// Square lamina: 1 mvfs + 1 mev_lone + 2 mev + 1 mef ⇒ (4,4,2,0,1,0).
/// Front face normal +Z (profile u×v), back face −Z; volume 0.
#[test]
fn lamina_square_face_pair() {
    let mut arena = BrepArena::new();
    let profile = square_profile(2.0, 3.0);
    let lam = make_face_from_profile(&mut arena, &profile).expect("lamina");
    assert_counts(&arena, lam.solid, (4, 4, 2, 0, 1, 0));

    let report = validate_solid(&arena, lam.solid).expect("lamina validates");
    assert_eq!((report.faces, report.rings, report.genus), (2, 0, 0));

    let nf = face_plane(&arena, lam.front).normal;
    let nb = face_plane(&arena, lam.back).normal;
    assert!(
        (nf.x, nf.y, nf.z) == (0.0, 0.0, 1.0),
        "front +Z, got {nf:?}"
    );
    assert!(
        (nb.x, nb.y, nb.z) == (0.0, 0.0, -1.0),
        "back −Z, got {nb:?}"
    );

    let vol = signed_volume(&arena, lam.solid).expect("volume");
    assert_eq!(vol, 0.0, "lamina encloses no volume");
}

/// Holed lamina is torus-like: one ring on EACH face, genus 1:
/// (V,E,F,R,S,G) = (8,8,2,2,1,1), LHS 0 = RHS.
#[test]
fn lamina_with_hole_genus_bookkeeping() {
    let (o, u, v) = xy_frame();
    let profile = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)],
        vec![vec![p2(1.0, 1.0), p2(2.0, 1.0), p2(2.0, 2.0), p2(1.0, 2.0)]],
    )
    .expect("holed profile is valid");

    let mut arena = BrepArena::new();
    let lam = make_face_from_profile(&mut arena, &profile).expect("holed lamina");
    assert_counts(&arena, lam.solid, (8, 8, 2, 2, 1, 1));

    let report = validate_solid(&arena, lam.solid).expect("holed lamina validates");
    assert_eq!((report.euler_lhs, report.euler_rhs), (0, 0));
    assert_eq!(
        arena.face(lam.front).expect("front").inner_loops.len(),
        1,
        "front face carries the hole ring"
    );
    assert_eq!(
        arena.face(lam.back).expect("back").inner_loops.len(),
        1,
        "back face carries the hole ring"
    );
}

// ---------------------------------------------------------------------------
// Box (square extrude)
// ---------------------------------------------------------------------------

/// 2 × 3 square extruded 4 along +Z ⇒ exact box table (8,12,6,0,1,0),
/// volume exactly 24, outward normals = the six axis directions.
#[test]
fn box_from_square_profile() {
    let mut arena = BrepArena::new();
    let profile = square_profile(2.0, 3.0);
    let ext = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 4.0).expect("box");

    assert_counts(&arena, ext.solid, (8, 12, 6, 0, 1, 0));
    let report = validate_solid(&arena, ext.solid).expect("box validates");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.rings),
        (8, 12, 6, 0)
    );
    assert_eq!((report.euler_lhs, report.euler_rhs), (2, 2));

    // Exact volume: all determinants are integers ⇒ the f64 sum is exact.
    let vol = signed_volume(&arena, ext.solid).expect("volume");
    assert_eq!(vol, 24.0, "volume must be exactly w·d·h");

    assert_outward_normals(&arena, ext.solid);

    // Base −Z, top +Z, and the full normal set is exactly the ±axis set.
    let nb = face_plane(&arena, ext.base).normal;
    let nt = face_plane(&arena, ext.top).normal;
    assert_eq!((nb.x, nb.y, nb.z), (0.0, 0.0, -1.0));
    assert_eq!((nt.x, nt.y, nt.z), (0.0, 0.0, 1.0));
    assert_eq!(ext.walls.len(), 4, "one wall per outer edge");
    let mut normals: Vec<[f64; 3]> = Vec::new();
    for &f in ext.walls.iter().chain([&ext.base, &ext.top]) {
        let n = face_plane(&arena, f).normal;
        normals.push([n.x, n.y, n.z]);
    }
    for axis in [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ] {
        assert!(
            normals.contains(&axis),
            "missing outward normal {axis:?} in {normals:?}"
        );
    }

    // Base and top are parallel (translated copies).
    assert!((kernel_v2::geom::dot(nb, nt) + 1.0).abs() < 1e-15);
}

/// Sweep direction OPPOSING the profile normal (−Z over a +Z-normal
/// profile): the constructor must reverse the working orientation so the
/// result is still outward (positive volume), with the base face (in the
/// profile plane) now facing +Z.
#[test]
fn box_extrude_against_profile_normal() {
    let mut arena = BrepArena::new();
    let profile = square_profile(2.0, 3.0);
    let ext =
        extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, -1.0), 4.0).expect("downward box");

    assert_counts(&arena, ext.solid, (8, 12, 6, 0, 1, 0));
    validate_solid(&arena, ext.solid).expect("downward box validates");

    let vol = signed_volume(&arena, ext.solid).expect("volume");
    assert_eq!(vol, 24.0, "flip path must still be outward-oriented");

    let nb = face_plane(&arena, ext.base).normal;
    let nt = face_plane(&arena, ext.top).normal;
    assert_eq!((nb.x, nb.y, nb.z), (0.0, 0.0, 1.0), "base faces up");
    assert_eq!((nt.x, nt.y, nt.z), (0.0, 0.0, -1.0), "top faces down");
    assert_outward_normals(&arena, ext.solid);
}

// ---------------------------------------------------------------------------
// L-shaped (concave) profile
// ---------------------------------------------------------------------------

/// Concave L profile (area 4 − 1 = 3) extruded 0.5 ⇒ volume exactly 1.5;
/// 6-gon prism table (12,18,8,0,1,0); every wall planar. Concavity is NOT
/// non-simplicity — `Profile::new` must accept it.
#[test]
fn l_profile_extrude() {
    let (o, u, v) = xy_frame();
    let profile = Profile::new(
        o,
        u,
        v,
        vec![
            p2(0.0, 0.0),
            p2(2.0, 0.0),
            p2(2.0, 1.0),
            p2(1.0, 1.0),
            p2(1.0, 2.0),
            p2(0.0, 2.0),
        ],
        vec![],
    )
    .expect("concave L profile is valid (concave != non-simple)");

    let mut arena = BrepArena::new();
    let ext = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 0.5).expect("L prism");

    assert_counts(&arena, ext.solid, (12, 18, 8, 0, 1, 0));
    let report = validate_solid(&arena, ext.solid).expect("L prism validates");
    assert_eq!((report.euler_lhs, report.euler_rhs), (2, 2));

    // Hand-computed: area(L) = 2·1 + 1·1 = 3; volume = 3 · 0.5 = 1.5.
    // All inputs dyadic ⇒ exact in f64.
    let vol = signed_volume(&arena, ext.solid).expect("volume");
    assert_eq!(vol, 1.5);

    assert_eq!(ext.walls.len(), 6);
    for &w in &ext.walls {
        assert_face_planar(&arena, w, 1e-12);
    }
    // The reflex-corner walls face AWAY from the material: the wall on the
    // inner x=1 segment must have normal +X... it bounds material at x<1?
    // Material near that wall is the column x∈[0,1], y∈[1,2] ⇒ outward +X.
    let count_px = ext
        .walls
        .iter()
        .filter(|&&w| {
            let n = face_plane(&arena, w).normal;
            (n.x, n.y, n.z) == (1.0, 0.0, 0.0)
        })
        .count();
    assert_eq!(
        count_px, 2,
        "x=2 outer wall AND x=1 reflex wall both face +X"
    );
}

// ---------------------------------------------------------------------------
// Through-hole extrude
// ---------------------------------------------------------------------------

/// 4×4 outer with 1×1 hole, extruded 2: genus-1 table (16,24,10,2,1,1),
/// LHS 0 = RHS, volume exactly (16 − 1) · 2 = 30, hole walls' outward
/// normals point INTO the hole void, base and top each carry one ring.
#[test]
fn holed_extrude_through_hole() {
    let (o, u, v) = xy_frame();
    let profile = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)],
        vec![vec![p2(1.0, 1.0), p2(2.0, 1.0), p2(2.0, 2.0), p2(1.0, 2.0)]],
    )
    .expect("holed profile valid");

    let mut arena = BrepArena::new();
    let ext = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("holed box");

    assert_counts(&arena, ext.solid, (16, 24, 10, 2, 1, 1));
    let report = validate_solid(&arena, ext.solid).expect("holed box validates");
    assert_eq!((report.euler_lhs, report.euler_rhs), (0, 0));
    assert_eq!((report.rings, report.genus), (2, 1));

    let vol = signed_volume(&arena, ext.solid).expect("volume");
    assert_eq!(vol, 30.0, "volume = outer − hole, exactly");

    // Base and top each carry exactly one ring (the hole mouth).
    assert_eq!(arena.face(ext.base).expect("base").inner_loops.len(), 1);
    assert_eq!(arena.face(ext.top).expect("top").inner_loops.len(), 1);

    // Hole walls: 4 planar quads whose outward normals point INTO the hole
    // void (toward the hole axis at (1.5, 1.5)).
    assert_eq!(ext.hole_walls.len(), 1);
    assert_eq!(ext.hole_walls[0].len(), 4);
    for &w in &ext.hole_walls[0] {
        assert_face_planar(&arena, w, 1e-12);
        let n = face_plane(&arena, w).normal;
        let c = face_centroid(&arena, w).expect("wall centroid");
        let d = n.x * (1.5 - c.x()) + n.y * (1.5 - c.y());
        assert!(
            d > 0.0,
            "hole wall normal {n:?} at {c:?} must point into the hole"
        );
    }
}

// ---------------------------------------------------------------------------
// Oblique plane (no axis-aligned shortcuts)
// ---------------------------------------------------------------------------

/// 1×2 rectangle on an oblique (orthonormal but irrational-direction)
/// plane, extruded 0.7 along the plane normal: box table, volume 1·2·0.7,
/// outward normals, top parallel to base.
#[test]
fn oblique_plane_box() {
    // Orthonormal frame: u = (1,2,2)/3, v = (−2,−1,2)/3, n = u×v = (2,−2,1)/3.
    let o = Point3::new(0.5, -1.0, 2.0);
    let u = Vector3::new(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
    let v = Vector3::new(-2.0 / 3.0, -1.0 / 3.0, 2.0 / 3.0);
    let profile = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 2.0), p2(0.0, 2.0)],
        vec![],
    )
    .expect("oblique profile valid");

    let mut arena = BrepArena::new();
    // Direction deliberately unnormalized (the constructor normalizes).
    let ext =
        extrude(&mut arena, &profile, Vector3::new(2.0, -2.0, 1.0), 0.7).expect("oblique box");

    assert_counts(&arena, ext.solid, (8, 12, 6, 0, 1, 0));
    validate_solid(&arena, ext.solid).expect("oblique box validates");

    let vol = signed_volume(&arena, ext.solid).expect("volume");
    assert!((vol - 1.4).abs() < 1e-12, "volume 1·2·0.7 = 1.4, got {vol}");

    assert_outward_normals(&arena, ext.solid);

    // Top parallel to base (antiparallel normals), and base normal = −n.
    let nb = face_plane(&arena, ext.base).normal;
    let nt = face_plane(&arena, ext.top).normal;
    assert!((kernel_v2::geom::dot(nb, nt) + 1.0).abs() < 1e-12);
    let n_expect = [2.0 / 3.0, -2.0 / 3.0, 1.0 / 3.0];
    let err = (nb.x + n_expect[0]).abs() + (nb.y + n_expect[1]).abs() + (nb.z + n_expect[2]).abs();
    assert!(err < 1e-12, "base normal must be −n, got {nb:?}");

    // Walls planar (release-mode explicit check).
    for &w in &ext.walls {
        assert_face_planar(&arena, w, 1e-12);
    }
}

// ---------------------------------------------------------------------------
// Profile error paths
// ---------------------------------------------------------------------------

#[test]
fn profile_rejects_too_few_vertices() {
    let (o, u, v) = xy_frame();
    let err = Profile::new(o, u, v, vec![p2(0.0, 0.0), p2(1.0, 0.0)], vec![]).unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileTooFewVertices { loop_index: 0 });

    // Same arm for holes (loop_index = hole k + 1).
    let err = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)],
        vec![vec![p2(1.0, 1.0), p2(2.0, 2.0)]],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileTooFewVertices { loop_index: 1 });
}

#[test]
fn profile_rejects_repeated_consecutive_vertex() {
    let (o, u, v) = xy_frame();
    let err = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 0.0), p2(0.0, 1.0)],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileRepeatedVertex { loop_index: 0 });

    // Closing edge: last == first is the same defect.
    let err = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 1.0), p2(0.0, 0.0)],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileRepeatedVertex { loop_index: 0 });
}

/// The documented loud contract for non-simple polygons: the EXACT
/// (dashu-rational) simplicity check rejects them with `ProfileNotSimple`.
#[test]
fn profile_rejects_non_simple_polygons() {
    let (o, u, v) = xy_frame();

    // Bowtie: edges (0,0)–(1,1) and (1,0)–(0,1) cross properly.
    let err = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(1.0, 1.0), p2(1.0, 0.0), p2(0.0, 1.0)],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 });

    // Collinear "polygon" (zero area): closing adjacency doubles back.
    let err = Profile::new(
        o,
        u,
        v,
        vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.0)],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 });

    // Touching (non-adjacent vertex ON an edge): exact contact must reject.
    let err = Profile::new(
        o,
        u,
        v,
        vec![
            p2(0.0, 0.0),
            p2(2.0, 0.0),
            p2(2.0, 2.0),
            p2(1.0, 0.0), // lands exactly on edge (0,0)–(2,0)
            p2(0.0, 2.0),
        ],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 });
}

#[test]
fn profile_rejects_bad_hole_placement() {
    let (o, u, v) = xy_frame();
    let outer = vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)];

    // Hole entirely outside the outer loop.
    let err = Profile::new(
        o,
        u,
        v,
        outer.clone(),
        vec![vec![p2(5.0, 5.0), p2(6.0, 5.0), p2(6.0, 6.0), p2(5.0, 6.0)]],
    )
    .unwrap_err();
    assert_eq!(
        err,
        KernelV2Error::ProfileHoleNotInsideOuter { hole_index: 0 }
    );

    // Hole crossing the outer boundary.
    let err = Profile::new(
        o,
        u,
        v,
        outer.clone(),
        vec![vec![p2(3.0, 1.0), p2(5.0, 1.0), p2(5.0, 2.0), p2(3.0, 2.0)]],
    )
    .unwrap_err();
    assert_eq!(
        err,
        KernelV2Error::ProfileLoopsIntersect {
            loop_a: 0,
            loop_b: 1
        }
    );

    // Nested holes.
    let err = Profile::new(
        o,
        u,
        v,
        outer,
        vec![
            vec![p2(1.0, 1.0), p2(3.0, 1.0), p2(3.0, 3.0), p2(1.0, 3.0)],
            vec![p2(1.5, 1.5), p2(2.5, 1.5), p2(2.5, 2.5), p2(1.5, 2.5)],
        ],
    )
    .unwrap_err();
    assert_eq!(
        err,
        KernelV2Error::ProfileHolesNested {
            outer_hole: 0,
            inner_hole: 1
        }
    );
}

#[test]
fn profile_rejects_degenerate_frame() {
    let square = vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 1.0), p2(0.0, 1.0)];
    let o = Point3::new(0.0, 0.0, 0.0);

    // Parallel basis vectors: u × v = 0.
    let err = Profile::new(
        o,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(2.0, 0.0, 0.0),
        square.clone(),
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileDegenerateBasis);

    // Non-finite coordinate.
    let err = Profile::new(
        o,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![p2(0.0, 0.0), p2(f64::NAN, 0.0), p2(1.0, 1.0)],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ProfileNotFinite);
}

// ---------------------------------------------------------------------------
// Extrude error paths (argument errors leave the arena untouched)
// ---------------------------------------------------------------------------

#[test]
fn extrude_rejects_bad_distance() {
    let profile = square_profile(1.0, 1.0);
    let mut arena = BrepArena::new();
    let up = Vector3::new(0.0, 0.0, 1.0);

    for (dist, label) in [(0.0, "zero"), (-1.0, "negative"), (f64::NAN, "NaN")] {
        let err = extrude(&mut arena, &profile, up, dist).unwrap_err();
        assert_eq!(
            err,
            KernelV2Error::ExtrudeNonPositiveDistance,
            "{label} distance"
        );
    }
    assert_eq!(arena, BrepArena::new(), "failed extrude must not mutate");
}

#[test]
fn extrude_rejects_bad_direction() {
    let profile = square_profile(1.0, 1.0);
    let mut arena = BrepArena::new();

    // In-plane direction (profile plane is XY).
    let err = extrude(&mut arena, &profile, Vector3::new(1.0, 0.0, 0.0), 1.0).unwrap_err();
    assert_eq!(err, KernelV2Error::ExtrudeDirectionInPlane);
    let err = extrude(&mut arena, &profile, Vector3::new(0.7, -0.3, 0.0), 1.0).unwrap_err();
    assert_eq!(err, KernelV2Error::ExtrudeDirectionInPlane);

    // Zero / non-finite direction.
    let err = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 0.0), 1.0).unwrap_err();
    assert_eq!(err, KernelV2Error::ExtrudeDegenerateDirection);
    let err = extrude(
        &mut arena,
        &profile,
        Vector3::new(0.0, 0.0, f64::INFINITY),
        1.0,
    )
    .unwrap_err();
    assert_eq!(err, KernelV2Error::ExtrudeDegenerateDirection);

    assert_eq!(arena, BrepArena::new(), "failed extrude must not mutate");
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn identical_inputs_produce_identical_arenas() {
    let (o, u, v) = xy_frame();
    let build = || {
        let profile = Profile::new(
            o,
            u,
            v,
            vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)],
            vec![vec![p2(1.0, 1.0), p2(2.0, 1.0), p2(2.0, 2.0), p2(1.0, 2.0)]],
        )
        .expect("profile valid");
        let mut arena = BrepArena::new();
        let ext =
            extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("holed box");
        (arena, ext)
    };
    let (a1, e1) = build();
    let (a2, e2) = build();
    assert_eq!(e1, e2, "identical inputs ⇒ identical handle sets");
    assert_eq!(a1, a2, "identical inputs ⇒ identical arenas");
    assert_eq!(format!("{a1:?}"), format!("{a2:?}"));
}

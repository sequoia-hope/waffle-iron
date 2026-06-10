//! PR-KV1 RED oracles: half-edge B-Rep arena + Euler operators.
//!
//! Property tests over pure Euler-operator construction sequences
//! (Stroud 2006, ch. 4 + appendix F):
//!
//! - unit cube: 1 mvfs + 7 mev + 5 mef (Stroud §4.1's spanning-set
//!   decomposition of the cube, element vector (8,12,6,0,0,1) ⇒ primitive
//!   vector (7,5,1,0,0): seven MEV, five MFE, one MBFV), with the
//!   Euler–Poincaré formula and the Newell invariant asserted at EVERY step;
//! - triangular prism: 1 mvfs + 5 mev + 4 mef;
//! - through-hole (square tunnel through the cube) exercising kemr + kfmrh
//!   genus bookkeeping: V − E + F − R = 0 = 2(S − G) for the torus-like
//!   result;
//! - operator error paths (mef across loops, degenerate edges, kemr on a
//!   face-separating edge, kfmrh misuse, stale ids) with atomicity
//!   (Err ⇒ arena unmodified);
//! - determinism: identical construction sequences ⇒ identical arenas;
//! - `validate_solid` detection arms via deliberate raw-field corruption.
//!
//! ## Why there is no "Newell violation" error-path test
//!
//! The `face.normal ≡ Newell(outer_loop)` invariant (crate hard rule 2)
//! cannot be violated through the public operator API: the stored normal is
//! *derived from* the loop walk at every operator exit, and the only way a
//! face could fail to satisfy the invariant is by having no orientation at
//! all — which `mef` rejects up front with `Err(DegenerateFaceNormal)`.
//! This is the "impossible-by-construction" case the KV1 mandate asks to
//! document; the corruption test `validate_detects_newell_mismatch` shows
//! the invariant IS still checked (defense in depth), it just takes raw
//! field access to break it.

use cad_primitives::Point3;
use kernel_v2::*;

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Assert the Euler–Poincaré element counts of `solid` and that the formula
/// holds. `expect` is (V, E, F, R, S, G).
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
        "Euler–Poincaré formula V−E+F−R = 2(S−G) violated: {} != {}",
        c.lhs(),
        c.rhs()
    );
}

/// Assert the Newell invariant on every live face of the arena:
/// orientable outer loop ⇔ `surface == Some(plane)` with
/// `plane.normal ≡ normalize(Newell(outer_loop))`.
#[track_caller]
fn assert_newell_invariant(arena: &BrepArena) {
    for (i, slot) in arena.faces.iter().enumerate() {
        let Some(face) = slot else { continue };
        let pts = arena
            .loop_points(face.outer_loop)
            .expect("outer loop walkable");
        match kernel_v2::geom::newell_unit(&pts) {
            Some(u) => {
                let Some(Surface::Plane(plane)) = face.surface else {
                    panic!("face {i}: orientable loop but surface = {:?}", face.surface);
                };
                let d = kernel_v2::geom::dot(plane.normal, u);
                assert!(
                    d > 1.0 - 1e-9,
                    "face {i}: stored normal {:?} != Newell {u:?} (dot {d})",
                    plane.normal
                );
            }
            None => assert!(
                face.surface.is_none(),
                "face {i}: degenerate loop must carry surface None, got {:?}",
                face.surface
            ),
        }
    }
}

#[track_caller]
fn assert_face_normal(arena: &BrepArena, face: FaceId, expect: (f64, f64, f64)) {
    let f = arena.face(face).expect("face alive");
    let Some(Surface::Plane(plane)) = f.surface else {
        panic!("face {face:?} has no plane: {:?}", f.surface);
    };
    let n = plane.normal;
    let err = (n.x - expect.0).abs() + (n.y - expect.1).abs() + (n.z - expect.2).abs();
    assert!(
        err < 1e-12,
        "face {face:?} normal {n:?}, expected {expect:?}"
    );
}

// ---------------------------------------------------------------------------
// Cube construction (hand-derived; every step's expected V/E/F/R/S/G noted)
// ---------------------------------------------------------------------------

/// Handles needed by downstream tests.
struct Cube {
    solid: SolidId,
    /// f1 from mvfs — ends as the TOP face (+Z).
    top: FaceId,
    /// New face of the bottom-closing mef (−Z).
    bottom: FaceId,
    /// Bottom loop representative `(v1→v4)`, lives in the bottom loop.
    bottom_he: HalfEdgeId,
    /// Top loop half-edge `v5→v6` (origin at v5 = (0,0,1)).
    top_he_v5_v6: HalfEdgeId,
    /// Half-edge `v1→v2` (bottom-rim edge; its twin lives in another loop).
    rim_he_v1_v2: HalfEdgeId,
    /// A side face (the −Y one).
    side_a: FaceId,
}

/// Build the unit cube [0,1]^3 with outward normals via
/// 1 mvfs + 1 mev_lone + 6 mev + 5 mef.
///
/// Vertex map: bottom v1(0,0,0) v2(1,0,0) v3(1,1,0) v4(0,1,0);
///             top    v5(0,0,1) v6(1,0,1) v7(1,1,1) v8(0,1,1).
///
/// Step table (V,E,F,R,S,G after each op):
///
/// | # | op | result | counts |
/// |---|----|--------|--------|
/// | 1 | mvfs(v1)                          | solid, f1, l1=Lone(v1) | (1,0,1,0,1,0) |
/// | 2 | mev_lone(l1, v2)                  | edge v1–v2             | (2,1,1,0,1,0) |
/// | 3 | mev(he(v2→v1), v3)                | spur v2–v3             | (3,2,1,0,1,0) |
/// | 4 | mev(he(v3→v2), v4)                | spur v3–v4             | (4,3,1,0,1,0) |
/// | 5 | mef(he(v4→v3), he(v1→v2))         | bottom = v4v3v2v1 (−Z) | (4,4,2,0,1,0) |
/// | 6 | mev(he(v1→v2), v5)                | post v1–v5             | (5,5,2,0,1,0) |
/// | 7 | mev(he(v2→v3), v6)                | post v2–v6             | (6,6,2,0,1,0) |
/// | 8 | mev(he(v3→v4), v7)                | post v3–v7             | (7,7,2,0,1,0) |
/// | 9 | mev(he(v4→v1), v8)                | post v4–v8             | (8,8,2,0,1,0) |
/// |10 | mef(he(v5→v1), he(v6→v2))         | side v5v1v2v6 (−Y)     | (8,9,3,0,1,0) |
/// |11 | mef(he(v6→v2), he(v7→v3))         | side v6v2v3v7 (+X)     | (8,10,4,0,1,0) |
/// |12 | mef(he(v7→v3), he(v8→v4))         | side v7v3v4v8 (+Y)     | (8,11,5,0,1,0) |
/// |13 | mef(he(v8→v4), he(v5→v6))         | side v8v4v1v5 (−X);    | (8,12,6,0,1,0) |
/// |   |                                   | f1's residual loop is the top v5v6v7v8 (+Z) | |
fn build_cube(arena: &mut BrepArena) -> Cube {
    // Step 1: mvfs — Stroud §F.8.
    let m = mvfs(arena, pt(0.0, 0.0, 0.0)).expect("mvfs");
    assert_counts(arena, m.solid, (1, 0, 1, 0, 1, 0));
    assert_newell_invariant(arena);

    // Step 2: first edge v1–v2.
    let e12 = mev_lone(arena, m.outer_loop, pt(1.0, 0.0, 0.0)).expect("mev_lone");
    assert_counts(arena, m.solid, (2, 1, 1, 0, 1, 0));
    assert_newell_invariant(arena);

    // Step 3: spur v2–v3, anchored at he(v2→v1).
    let e23 = mev(arena, e12.he_in, pt(1.0, 1.0, 0.0)).expect("mev v3");
    assert_counts(arena, m.solid, (3, 2, 1, 0, 1, 0));
    assert_newell_invariant(arena);

    // Step 4: spur v3–v4, anchored at he(v3→v2).
    let e34 = mev(arena, e23.he_in, pt(0.0, 1.0, 0.0)).expect("mev v4");
    assert_counts(arena, m.solid, (4, 3, 1, 0, 1, 0));
    assert_newell_invariant(arena);

    // Step 5: close the bottom. New face takes the cycle containing he_from
    // = he(v4→v3): v4→v3→v2→v1, Newell −Z = outward bottom.
    let bottom = mef(arena, e34.he_in, e12.he_out).expect("mef bottom");
    assert_counts(arena, m.solid, (4, 4, 2, 0, 1, 0));
    assert_newell_invariant(arena);
    assert_face_normal(arena, bottom.face, (0.0, 0.0, -1.0));

    // Steps 6–9: sweep the four posts up. Anchors are the four bottom-square
    // half-edges remaining in f1's loop (v1→v2, v2→v3, v3→v4, v4→v1).
    let up1 = mev(arena, e12.he_out, pt(0.0, 0.0, 1.0)).expect("mev v5");
    assert_counts(arena, m.solid, (5, 5, 2, 0, 1, 0));
    let up2 = mev(arena, e23.he_out, pt(1.0, 0.0, 1.0)).expect("mev v6");
    assert_counts(arena, m.solid, (6, 6, 2, 0, 1, 0));
    let up3 = mev(arena, e34.he_out, pt(1.0, 1.0, 1.0)).expect("mev v7");
    assert_counts(arena, m.solid, (7, 7, 2, 0, 1, 0));
    let up4 = mev(arena, bottom.he_old_side, pt(0.0, 1.0, 1.0)).expect("mev v8");
    assert_counts(arena, m.solid, (8, 8, 2, 0, 1, 0));
    assert_newell_invariant(arena);

    // Steps 10–13: the four side faces. Each mef rim edge connects two
    // adjacent top vertices; the new face takes the U-shaped post-down,
    // rim-across, post-up cycle.
    let side_a = mef(arena, up1.he_in, up2.he_in).expect("mef side -Y");
    assert_counts(arena, m.solid, (8, 9, 3, 0, 1, 0));
    assert_face_normal(arena, side_a.face, (0.0, -1.0, 0.0));

    let side_b = mef(arena, up2.he_in, up3.he_in).expect("mef side +X");
    assert_counts(arena, m.solid, (8, 10, 4, 0, 1, 0));
    assert_face_normal(arena, side_b.face, (1.0, 0.0, 0.0));

    let side_c = mef(arena, up3.he_in, up4.he_in).expect("mef side +Y");
    assert_counts(arena, m.solid, (8, 11, 5, 0, 1, 0));
    assert_face_normal(arena, side_c.face, (0.0, 1.0, 0.0));

    // he_to must have origin v5: that is side_a's old-side edge v5→v6.
    let side_d = mef(arena, up4.he_in, side_a.he_old_side).expect("mef side -X");
    assert_counts(arena, m.solid, (8, 12, 6, 0, 1, 0));
    assert_face_normal(arena, side_d.face, (-1.0, 0.0, 0.0));
    assert_newell_invariant(arena);

    // f1's residual loop is the top square v5→v6→v7→v8 (+Z).
    assert_face_normal(arena, m.face, (0.0, 0.0, 1.0));

    Cube {
        solid: m.solid,
        top: m.face,
        bottom: bottom.face,
        bottom_he: bottom.he_new_side,
        top_he_v5_v6: side_a.he_old_side,
        rim_he_v1_v2: e12.he_out,
        side_a: side_a.face,
    }
}

#[test]
fn cube_construction_step_table() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    assert_counts(&arena, cube.solid, (8, 12, 6, 0, 1, 0));
}

#[test]
fn cube_validates() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let report = validate_solid(&arena, cube.solid).expect("cube must validate");
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (8, 12, 6, 0, 1, 0)
    );
    assert_eq!(report.euler_lhs, 2);
    assert_eq!(report.euler_rhs, 2);
}

// ---------------------------------------------------------------------------
// Triangular prism
// ---------------------------------------------------------------------------

/// Build a triangular prism (right triangle base, height 1) via
/// 1 mvfs + 1 mev_lone + 4 mev + 4 mef.
///
/// Vertex map: bottom v1(0,0,0) v2(1,0,0) v3(0,1,0); top v4 v5 v6 above.
///
/// Step table:
///
/// | # | op | result | counts |
/// |---|----|--------|--------|
/// | 1 | mvfs(v1)                   |               | (1,0,1,0,1,0) |
/// | 2 | mev_lone(l1, v2)           | edge v1–v2    | (2,1,1,0,1,0) |
/// | 3 | mev(he(v2→v1), v3)         | spur v2–v3    | (3,2,1,0,1,0) |
/// | 4 | mef(he(v3→v2), he(v1→v2))  | bottom (−Z)   | (3,3,2,0,1,0) |
/// | 5 | mev(he(v1→v2), v4)         | post v1–v4    | (4,4,2,0,1,0) |
/// | 6 | mev(he(v2→v3), v5)         | post v2–v5    | (5,5,2,0,1,0) |
/// | 7 | mev(he(v3→v1), v6)         | post v3–v6    | (6,6,2,0,1,0) |
/// | 8 | mef(he(v4→v1), he(v5→v2))  | side −Y       | (6,7,3,0,1,0) |
/// | 9 | mef(he(v5→v2), he(v6→v3))  | side +X+Y     | (6,8,4,0,1,0) |
/// |10 | mef(he(v6→v3), he(v4→v5))  | side −X; residual top (+Z) | (6,9,5,0,1,0) |
#[test]
fn prism_construction_step_table_and_validates() {
    let mut arena = BrepArena::new();

    let m = mvfs(&mut arena, pt(0.0, 0.0, 0.0)).expect("mvfs");
    assert_counts(&arena, m.solid, (1, 0, 1, 0, 1, 0));

    let e12 = mev_lone(&mut arena, m.outer_loop, pt(1.0, 0.0, 0.0)).expect("mev_lone");
    assert_counts(&arena, m.solid, (2, 1, 1, 0, 1, 0));

    let e23 = mev(&mut arena, e12.he_in, pt(0.0, 1.0, 0.0)).expect("mev v3");
    assert_counts(&arena, m.solid, (3, 2, 1, 0, 1, 0));

    let bottom = mef(&mut arena, e23.he_in, e12.he_out).expect("mef bottom");
    assert_counts(&arena, m.solid, (3, 3, 2, 0, 1, 0));
    assert_face_normal(&arena, bottom.face, (0.0, 0.0, -1.0));

    let up1 = mev(&mut arena, e12.he_out, pt(0.0, 0.0, 1.0)).expect("mev v4");
    assert_counts(&arena, m.solid, (4, 4, 2, 0, 1, 0));
    let up2 = mev(&mut arena, e23.he_out, pt(1.0, 0.0, 1.0)).expect("mev v5");
    assert_counts(&arena, m.solid, (5, 5, 2, 0, 1, 0));
    let up3 = mev(&mut arena, bottom.he_old_side, pt(0.0, 1.0, 1.0)).expect("mev v6");
    assert_counts(&arena, m.solid, (6, 6, 2, 0, 1, 0));
    assert_newell_invariant(&arena);

    let side_a = mef(&mut arena, up1.he_in, up2.he_in).expect("mef side -Y");
    assert_counts(&arena, m.solid, (6, 7, 3, 0, 1, 0));
    assert_face_normal(&arena, side_a.face, (0.0, -1.0, 0.0));

    let side_b = mef(&mut arena, up2.he_in, up3.he_in).expect("mef hypotenuse side");
    assert_counts(&arena, m.solid, (6, 8, 4, 0, 1, 0));
    let s = 1.0 / 2.0f64.sqrt();
    assert_face_normal(&arena, side_b.face, (s, s, 0.0));

    let side_c = mef(&mut arena, up3.he_in, side_a.he_old_side).expect("mef side -X");
    assert_counts(&arena, m.solid, (6, 9, 5, 0, 1, 0));
    assert_face_normal(&arena, side_c.face, (-1.0, 0.0, 0.0));
    assert_face_normal(&arena, m.face, (0.0, 0.0, 1.0));
    assert_newell_invariant(&arena);

    let report = validate_solid(&arena, m.solid).expect("prism must validate");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.rings),
        (6, 9, 5, 0)
    );
}

// ---------------------------------------------------------------------------
// Through-hole: kemr + kfmrh genus bookkeeping
// ---------------------------------------------------------------------------

/// Drill a square through-hole (0.25..0.75)² down the cube's Z axis.
///
/// Hole corners: top h1(.25,.25,1) h2(.75,.25,1) h3(.75,.75,1) h4(.25,.75,1);
/// bottom b1..b4 below them at z=0.
///
/// Step table (continuing from the cube at (8,12,6,0,1,0)):
///
/// | # | op | result | counts |
/// |---|----|--------|--------|
/// | 1 | mev(he(v5→v6), h1)         | bridge v5–h1 in top loop  | ( 9,13,6,0,1,0) |
/// | 2 | mev(he(h1→v5), h2)         | spur h1–h2                | (10,14,6,0,1,0) |
/// | 3 | mev(he(h2→h1), h3)         | spur h2–h3                | (11,15,6,0,1,0) |
/// | 4 | mev(he(h3→h2), h4)         | spur h3–h4                | (12,16,6,0,1,0) |
/// | 5 | mef(he(h1→h2), he(h4→h3))  | lid h1h2h3h4 (+Z)         | (12,17,7,0,1,0) |
/// | 6 | kemr(he(v5→h1))            | lid square becomes a ring of the top face (winds −Z, opposite the top's +Z) | (12,16,7,1,1,0) |
/// | 7–10 | mev ×4 on the lid loop  | posts h_i–b_i             | (16,20,7,1,1,0) |
/// | 11–14 | mef ×4                 | hole walls (+Y,−X,−Y,+X); lid's residual loop is the bottom membrane b1b2b3b4 (+Z) | (16,24,11,1,1,0) |
/// | 15 | kfmrh(lid_face, bottom)   | membrane killed; its loop becomes a ring of the bottom face; genus 1 | (16,24,10,2,1,1) |
///
/// Final Euler–Poincaré: 16 − 24 + 10 − 2 = 0 = 2(1 − 1)  (torus-like).
#[test]
fn through_hole_genus_bookkeeping() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let s = cube.solid;

    // 1: bridge from v5 into the hole region.
    let bridge = mev(&mut arena, cube.top_he_v5_v6, pt(0.25, 0.25, 1.0)).expect("bridge");
    assert_counts(&arena, s, (9, 13, 6, 0, 1, 0));

    // 2–4: spur chain h1→h2→h3→h4.
    let s12 = mev(&mut arena, bridge.he_in, pt(0.75, 0.25, 1.0)).expect("h2");
    assert_counts(&arena, s, (10, 14, 6, 0, 1, 0));
    let s23 = mev(&mut arena, s12.he_in, pt(0.75, 0.75, 1.0)).expect("h3");
    assert_counts(&arena, s, (11, 15, 6, 0, 1, 0));
    let s34 = mev(&mut arena, s23.he_in, pt(0.25, 0.75, 1.0)).expect("h4");
    assert_counts(&arena, s, (12, 16, 6, 0, 1, 0));

    // 5: close the lid. New face = h1→h2→h3→h4, Newell +Z (same as top —
    // the lid is coplanar exterior surface until the hole is opened).
    let lid = mef(&mut arena, s12.he_out, s34.he_in).expect("lid");
    assert_counts(&arena, s, (12, 17, 7, 0, 1, 0));
    assert_face_normal(&arena, lid.face, (0.0, 0.0, 1.0));
    assert_newell_invariant(&arena);

    // 6: kill the bridge; the lid square (walked h1→h4→h3→h2 on the top
    // face's side, Newell −Z) becomes a ring of the top face.
    let ring_top = kemr(&mut arena, bridge.he_out).expect("kemr bridge");
    assert_counts(&arena, s, (12, 16, 7, 1, 1, 0));
    let ring = arena.loop_(ring_top.ring).expect("ring alive");
    assert_eq!(ring.kind, LoopKind::Inner);
    assert_eq!(ring.face, cube.top);
    assert!(arena
        .face(cube.top)
        .expect("top")
        .inner_loops
        .contains(&ring_top.ring));
    // Ring winds opposite to the top face (+Z): Newell must be −Z.
    let ring_pts = arena.loop_points(ring_top.ring).expect("ring pts");
    let rn = kernel_v2::geom::newell_unit(&ring_pts).expect("ring orientable");
    assert!(rn.z < -0.999_999_999, "ring must wind −Z, got {rn:?}");
    assert_newell_invariant(&arena);

    // 7–10: sweep the lid down to z=0.
    let dn1 = mev(&mut arena, s12.he_out, pt(0.25, 0.25, 0.0)).expect("b1");
    let dn2 = mev(&mut arena, s23.he_out, pt(0.75, 0.25, 0.0)).expect("b2");
    let dn3 = mev(&mut arena, s34.he_out, pt(0.75, 0.75, 0.0)).expect("b3");
    let dn4 = mev(&mut arena, lid.he_new_side, pt(0.25, 0.75, 0.0)).expect("b4");
    assert_counts(&arena, s, (16, 20, 7, 1, 1, 0));

    // 11–14: hole walls. Outward normals point INTO the hole void.
    let wall_a = mef(&mut arena, dn1.he_in, dn2.he_in).expect("wall +Y");
    assert_face_normal(&arena, wall_a.face, (0.0, 1.0, 0.0));
    let wall_b = mef(&mut arena, dn2.he_in, dn3.he_in).expect("wall -X");
    assert_face_normal(&arena, wall_b.face, (-1.0, 0.0, 0.0));
    let wall_c = mef(&mut arena, dn3.he_in, dn4.he_in).expect("wall -Y");
    assert_face_normal(&arena, wall_c.face, (0.0, -1.0, 0.0));
    let wall_d = mef(&mut arena, dn4.he_in, wall_a.he_old_side).expect("wall +X");
    assert_face_normal(&arena, wall_d.face, (1.0, 0.0, 0.0));
    assert_counts(&arena, s, (16, 24, 11, 1, 1, 0));
    // The lid face is now the hole-bottom membrane b1b2b3b4 (+Z).
    assert_face_normal(&arena, lid.face, (0.0, 0.0, 1.0));
    assert_newell_invariant(&arena);

    // 15: open the hole — kill the membrane, its loop becomes a ring of the
    // bottom face, genus increments.
    let ring_bot = kfmrh(&mut arena, lid.face, cube.bottom).expect("kfmrh");
    assert_counts(&arena, s, (16, 24, 10, 2, 1, 1));
    assert!(arena.face(lid.face).is_err(), "killed face must be dead");
    let rb = arena.loop_(ring_bot).expect("bottom ring alive");
    assert_eq!(rb.kind, LoopKind::Inner);
    assert_eq!(rb.face, cube.bottom);
    // Bottom face is −Z; its ring must wind +Z.
    let rb_pts = arena.loop_points(ring_bot).expect("rb pts");
    let rbn = kernel_v2::geom::newell_unit(&rb_pts).expect("rb orientable");
    assert!(
        rbn.z > 0.999_999_999,
        "bottom ring must wind +Z, got {rbn:?}"
    );
    assert_newell_invariant(&arena);

    // Full validation of the genus-1 solid.
    let report = validate_solid(&arena, s).expect("holed cube must validate");
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (16, 24, 10, 2, 1, 1)
    );
    assert_eq!(report.euler_lhs, 0);
    assert_eq!(report.euler_rhs, 0);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn identical_sequences_produce_identical_arenas() {
    let mut a1 = BrepArena::new();
    let mut a2 = BrepArena::new();
    let c1 = build_cube(&mut a1);
    let c2 = build_cube(&mut a2);
    assert_eq!(c1.solid, c2.solid);
    assert_eq!(a1, a2, "identical sequences must produce identical arenas");
    // And the textual debug layout too (catches PartialEq laxity).
    assert_eq!(format!("{a1:?}"), format!("{a2:?}"));
}

// ---------------------------------------------------------------------------
// Operator error paths (each also asserts atomicity: Err ⇒ arena unchanged)
// ---------------------------------------------------------------------------

#[test]
fn mef_across_loops_is_rejected() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let before = arena.clone();
    // Top-loop half-edge vs bottom-loop half-edge: not cofacial.
    let err = mef(&mut arena, cube.top_he_v5_v6, cube.bottom_he).unwrap_err();
    assert_eq!(err, KernelV2Error::MefDifferentLoops);
    assert_eq!(arena, before, "failed op must not mutate the arena");
}

#[test]
fn mef_degenerate_edges_are_rejected() {
    let mut arena = BrepArena::new();
    // Spur path v1—v2—v3: loop walks v1→v2, v2→v3, v3→v2, v2→v1.
    let m = mvfs(&mut arena, pt(0.0, 0.0, 0.0)).expect("mvfs");
    let e12 = mev_lone(&mut arena, m.outer_loop, pt(1.0, 0.0, 0.0)).expect("mev_lone");
    let e23 = mev(&mut arena, e12.he_in, pt(2.0, 0.0, 0.0)).expect("mev");
    let before = arena.clone();

    // Same half-edge twice.
    let err = mef(&mut arena, e12.he_out, e12.he_out).unwrap_err();
    assert_eq!(err, KernelV2Error::DegenerateEdge);
    assert_eq!(arena, before);

    // Distinct half-edges, same origin (both leave v2): a v2–v2 self-edge.
    let err = mef(&mut arena, e12.he_in, e23.he_out).unwrap_err();
    assert_eq!(err, KernelV2Error::DegenerateEdge);
    assert_eq!(arena, before);
}

#[test]
fn kemr_on_face_separating_edge_is_rejected() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let before = arena.clone();
    // The rim edge v1–v2 separates the bottom face from a side face: its
    // halves live in different loops. Killing it would merge faces (KEF),
    // i.e. violate the 2-manifold/ring contract of kemr.
    let err = kemr(&mut arena, cube.rim_he_v1_v2).unwrap_err();
    assert!(
        matches!(err, KernelV2Error::NonManifoldTopology(_)),
        "expected NonManifoldTopology, got {err:?}"
    );
    assert_eq!(arena, before);
}

#[test]
fn kfmrh_error_paths() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let before = arena.clone();

    // Same face on both sides.
    let err = kfmrh(&mut arena, cube.top, cube.top).unwrap_err();
    assert_eq!(err, KernelV2Error::KfmrhSameFace);
    assert_eq!(arena, before);

    // Different shells (a second solid in the same arena).
    let other = mvfs(&mut arena, pt(5.0, 5.0, 5.0)).expect("second solid");
    let before2 = arena.clone();
    let err = kfmrh(&mut arena, cube.top, other.face).unwrap_err();
    assert_eq!(err, KernelV2Error::KfmrhDifferentShells);
    assert_eq!(arena, before2);
}

#[test]
fn kfmrh_rejects_kill_face_with_rings() {
    // Build a cube with a ring on the top face (bridge + square + kemr),
    // then try to kill the ringed top face. Stroud §F.9: error condition.
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let bridge = mev(&mut arena, cube.top_he_v5_v6, pt(0.25, 0.25, 1.0)).expect("bridge");
    let s12 = mev(&mut arena, bridge.he_in, pt(0.75, 0.25, 1.0)).expect("h2");
    let s23 = mev(&mut arena, s12.he_in, pt(0.75, 0.75, 1.0)).expect("h3");
    let s34 = mev(&mut arena, s23.he_in, pt(0.25, 0.75, 1.0)).expect("h4");
    let _lid = mef(&mut arena, s12.he_out, s34.he_in).expect("lid");
    let _ring = kemr(&mut arena, bridge.he_out).expect("kemr");

    let before = arena.clone();
    let err = kfmrh(&mut arena, cube.top, cube.bottom).unwrap_err();
    assert_eq!(err, KernelV2Error::KfmrhFaceHasRings);
    assert_eq!(arena, before);
}

#[test]
fn mev_lone_on_edged_loop_is_rejected() {
    let mut arena = BrepArena::new();
    let m = mvfs(&mut arena, pt(0.0, 0.0, 0.0)).expect("mvfs");
    let _e = mev_lone(&mut arena, m.outer_loop, pt(1.0, 0.0, 0.0)).expect("first edge");
    let before = arena.clone();
    let err = mev_lone(&mut arena, m.outer_loop, pt(2.0, 0.0, 0.0)).unwrap_err();
    assert_eq!(err, KernelV2Error::LoopNotLone);
    assert_eq!(arena, before);
}

#[test]
fn stale_and_invalid_ids_are_rejected() {
    let mut arena = BrepArena::new();
    let _ = mvfs(&mut arena, pt(0.0, 0.0, 0.0)).expect("mvfs");

    // Out-of-range ids.
    let err = mev(&mut arena, HalfEdgeId(999), pt(1.0, 0.0, 0.0)).unwrap_err();
    assert!(matches!(err, KernelV2Error::InvalidId { .. }));
    let err = validate_solid(&arena, SolidId(7)).unwrap_err();
    assert!(matches!(err, KernelV2Error::InvalidId { .. }));

    // Stale id: a face killed by kfmrh must be rejected afterwards.
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let bridge = mev(&mut arena, cube.top_he_v5_v6, pt(0.25, 0.25, 1.0)).expect("bridge");
    let s12 = mev(&mut arena, bridge.he_in, pt(0.75, 0.25, 1.0)).expect("h2");
    let s23 = mev(&mut arena, s12.he_in, pt(0.75, 0.75, 1.0)).expect("h3");
    let s34 = mev(&mut arena, s23.he_in, pt(0.25, 0.75, 1.0)).expect("h4");
    let lid = mef(&mut arena, s12.he_out, s34.he_in).expect("lid");
    let _ring = kemr(&mut arena, bridge.he_out).expect("kemr");
    let dn1 = mev(&mut arena, s12.he_out, pt(0.25, 0.25, 0.0)).expect("b1");
    let dn2 = mev(&mut arena, s23.he_out, pt(0.75, 0.25, 0.0)).expect("b2");
    let dn3 = mev(&mut arena, s34.he_out, pt(0.75, 0.75, 0.0)).expect("b3");
    let dn4 = mev(&mut arena, lid.he_new_side, pt(0.25, 0.75, 0.0)).expect("b4");
    let wall_a = mef(&mut arena, dn1.he_in, dn2.he_in).expect("wall");
    let _ = mef(&mut arena, dn2.he_in, dn3.he_in).expect("wall");
    let _ = mef(&mut arena, dn3.he_in, dn4.he_in).expect("wall");
    let _ = mef(&mut arena, dn4.he_in, wall_a.he_old_side).expect("wall");
    let _ = kfmrh(&mut arena, lid.face, cube.bottom).expect("kfmrh");

    let err = kfmrh(&mut arena, lid.face, cube.top).unwrap_err();
    assert!(matches!(err, KernelV2Error::InvalidId { .. }));
}

// ---------------------------------------------------------------------------
// validate_solid detection arms (deliberate raw-field corruption)
// ---------------------------------------------------------------------------

#[test]
fn validate_detects_broken_twin_pairing() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    let h = cube.rim_he_v1_v2;
    arena.half_edges[h.index()].as_mut().unwrap().twin = h; // self-twin
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert!(
        matches!(err, KernelV2Error::TwinPairingBroken { .. }),
        "got {err:?}"
    );
}

#[test]
fn validate_detects_newell_mismatch() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    // Flip the stored top-face normal. The loop walk is unchanged, so the
    // plane still contains the loop (planarity passes) but the normal no
    // longer matches Newell.
    let face = arena.faces[cube.top.index()].as_mut().unwrap();
    let Some(Surface::Plane(ref mut plane)) = face.surface else {
        panic!("top face must have a plane");
    };
    plane.normal = UnitVector3 {
        x: -plane.normal.x,
        y: -plane.normal.y,
        z: -plane.normal.z,
    };
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert_eq!(err, KernelV2Error::NewellMismatch { face: cube.top });
}

#[test]
fn validate_detects_missing_surface() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    arena.faces[cube.side_a.index()].as_mut().unwrap().surface = None;
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert_eq!(err, KernelV2Error::FaceWithoutSurface { face: cube.side_a });
}

#[test]
fn validate_detects_euler_violation() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    // Lie about the genus: counts no longer satisfy V−E+F−R = 2(S−G).
    let shell_id = arena.solid(cube.solid).expect("solid").shells[0];
    arena.shells[shell_id.index()].as_mut().unwrap().genus = 3;
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert!(
        matches!(err, KernelV2Error::EulerFormulaViolation { .. }),
        "got {err:?}"
    );
}

#[test]
fn validate_detects_non_manifold_vertex() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    // Merge two opposite corners of the cube: redirect every half-edge
    // leaving v7 (=(1,1,1)) to leave v1 (=(0,0,0)) instead. Twin/next origin
    // consistency is preserved (all references rewritten), but v1 now has
    // TWO disjoint radial fans — the classic non-manifold "hourglass" vertex.
    let v1 = arena.half_edge(cube.rim_he_v1_v2).expect("rim he").origin;
    // Find v7 by coordinates.
    let v7 = VertexId(
        arena
            .vertices
            .iter()
            .position(|v| v.map(|v| v.point.as_array()) == Some([1.0, 1.0, 1.0]))
            .expect("v7 exists") as u32,
    );
    for slot in arena.half_edges.iter_mut() {
        if let Some(he) = slot.as_mut() {
            if he.origin == v7 {
                he.origin = v1;
            }
        }
    }
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert_eq!(err, KernelV2Error::NonManifoldVertex { vertex: v1 });
}

#[cfg(debug_assertions)]
#[test]
fn validate_detects_non_planar_face_in_debug() {
    let mut arena = BrepArena::new();
    let cube = build_cube(&mut arena);
    // Push one bottom vertex out of plane by 1 µm (= MIN_FEATURE_SIZE):
    // far above the debug planarity tripwire (1e-12 m).
    let v1 = arena.half_edge(cube.rim_he_v1_v2).expect("rim he").origin;
    arena.vertices[v1.index()].as_mut().unwrap().point = pt(0.0, 0.0, -1.0e-6);
    let err = validate_solid(&arena, cube.solid).unwrap_err();
    assert!(
        matches!(
            err,
            KernelV2Error::NonPlanarFace { .. } | KernelV2Error::NewellMismatch { .. }
        ),
        "got {err:?}"
    );
}

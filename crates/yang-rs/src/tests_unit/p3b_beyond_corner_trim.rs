//! P3b inc-4b unit fixtures: the beyond-corner conformal trim
//! (`trim_beyond_corner_phantoms`, spec `yang_169_p3b_curved_partner_pierce.md`
//! §5 inc-4b) and the pierce-time trim provenance (`owner_planes` on
//! [`PiercePoint`]).
//!
//! The trim collapses a RELOCATED section-curve sample that lies beyond a
//! minted corner junction's convex owner plane (zero kept content) onto the
//! mint. Every eligibility leg fails closed — these fixtures pin each leg
//! red/green: fire, patch-subset guard, reflex/default provenance, corridor
//! cap, off-plane rejection, and sub-band (weld-territory) rejection.

use super::n2_junction::rj_box;
#[allow(unused_imports)]
use super::*;
use crate::boolean::{junction_pierce_points, resolve_trim_beyond, MintProvenance, MintTrimPlane};
use std::collections::{BTreeMap, HashSet};

/// Owner planes of the fixture mint: the "wall" x=0 and the "face" z=0,
/// both convex at the pierce (beyond either = outside the owner).
fn convex_provenance() -> MintProvenance {
    MintProvenance {
        owner_planes: [
            MintTrimPlane {
                n: [1.0, 0.0, 0.0],
                d: 0.0,
                trim_beyond: true,
            },
            MintTrimPlane {
                n: [0.0, 0.0, 1.0],
                d: 0.0,
                trim_beyond: true,
            },
        ],
    }
}

/// Fixture mesh: mint 0 at the origin, candidate 1 at `phantom`, a pleat
/// pair over edge (0,1) that annihilates on collapse, and a witness tri
/// anchored at the candidate that must come out re-anchored at the mint.
fn trim_fixture(phantom: Point3) -> (Mesh, Vec<Option<TriangleAttribution>>) {
    let verts = vec![
        Point3::new(0.0, 0.0, 0.0),  // 0 mint
        phantom,                     // 1 candidate
        Point3::new(0.0, 1.0, 0.0),  // 2
        Point3::new(0.0, -1.0, 0.0), // 3
        Point3::new(0.5, -1.0, 0.0), // 4 witness
        Point3::new(1.5, -1.0, 0.0), // 5 witness
    ];
    let tris = vec![[0, 1, 2], [1, 0, 3], [1, 4, 5]];
    let attribution: Vec<Option<TriangleAttribution>> = (0..3)
        .map(|_| {
            Some(TriangleAttribution {
                input: InputId::A,
                face: 7,
            })
        })
        .collect();
    (Mesh::new(verts, tris), attribution)
}

fn minted_map() -> BTreeMap<u32, MintProvenance> {
    [(0u32, convex_provenance())].into_iter().collect()
}

/// GREEN: a moved sample 1e-3 beyond the convex wall, exactly on the other
/// owner plane, within the corridor — trims onto the mint. The pleat
/// annihilates; the witness tri is re-anchored at the mint.
#[test]
fn beyond_corner_phantom_trims_onto_mint() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-3);
    assert!(trimmed, "the beyond-corner phantom must trim");
    assert_eq!(
        mesh.tris,
        vec![[0, 4, 5]],
        "pleat annihilates; witness re-anchored at the mint (survivor=0)"
    );
    assert_eq!(mesh.tris.len(), attribution.len(), "attribution lockstep");
}

/// PATCH-SUBSET GUARD (the F0082 cap-ring lesson): the candidate carries a
/// patch the mint does not touch — collapsing would drag that face's ring
/// onto a foreign point. No fire, mesh byte-identical.
#[test]
fn candidate_with_foreign_patch_is_not_trimmed() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    // The witness tri (candidate-incident, NOT mint-incident) belongs to a
    // different face — the F0082 cap-ring shape.
    attribution[2] = Some(TriangleAttribution {
        input: InputId::B,
        face: 0,
    });
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-3);
    assert!(!trimmed, "a foreign-patch candidate must NOT trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// REFLEX / AMBIGUOUS provenance fails closed: `trim_beyond == false` on the
/// crossed plane means beyond it may be genuine material. No fire.
#[test]
fn reflex_owner_plane_is_not_trimmed() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    let mut prov = convex_provenance();
    prov.owner_planes[0].trim_beyond = false;
    let minted: BTreeMap<u32, MintProvenance> = [(0u32, prov)].into_iter().collect();
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed = trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted, 1e-3);
    assert!(!trimmed, "reflex/ambiguous incidence must NOT trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// DEFAULT (degenerate) provenance — the fail-closed placeholder — can never
/// fire: every signed distance evaluates to 0.
#[test]
fn default_provenance_is_inert() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    let minted: BTreeMap<u32, MintProvenance> = [(
        0u32,
        MintProvenance {
            owner_planes: [MintTrimPlane::default(); 2],
        },
    )]
    .into_iter()
    .collect();
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed = trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted, 1e-3);
    assert!(!trimmed, "degenerate placeholder provenance must be inert");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// CORRIDOR CAP: the same phantom with a chord budget too small to explain
/// the displacement (`2·d_ε/sinθ < dist`) may be LEGITIMATE far-side
/// geometry — no fire (status quo; downstream gates stay loud).
#[test]
fn over_corridor_candidate_is_not_trimmed() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    // sinθ = 1 here, so the corridor is 2·d_ε = 2e-4 < dist 1e-3.
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-4);
    assert!(!trimmed, "an over-corridor candidate must NOT trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// ON-THE-OTHER-PLANE leg: a candidate beyond the wall but OFF the second
/// owner plane is not a section-curve sample of (partner × that plane) —
/// the segment to the mint does not leave the face at the corner. No fire.
#[test]
fn off_other_plane_candidate_is_not_trimmed() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 1e-3));
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-3);
    assert!(!trimmed, "an off-plane candidate must NOT trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// SUB-BAND: within the `TAU_MODEL·(1+scale)` coincidence band the pair is
/// the §4.3 WELD's territory, never the trim's.
#[test]
fn sub_band_candidate_is_left_to_the_weld() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(5e-8, 0.0, 0.0));
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = [1u32].into_iter().collect();
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-3);
    assert!(!trimmed, "a sub-band pair is weld territory, not trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// UN-MOVED candidates are never trimmed (the R0091 restriction: the pass
/// never touches un-relocated arrangement geometry).
#[test]
fn unmoved_candidate_is_not_trimmed() {
    let (mut mesh, mut attribution) = trim_fixture(Point3::new(1e-3, 0.0, 0.0));
    let before = mesh.tris.clone();
    let moved: HashSet<u32> = HashSet::new();
    let trimmed =
        trim_beyond_corner_phantoms(&mut mesh, &mut attribution, &moved, &minted_map(), 1e-3);
    assert!(!trimmed, "an un-relocated candidate must NOT trim");
    assert_eq!(mesh.tris, before, "mesh byte-identical");
}

/// PIERCE-TIME PROVENANCE (inc-4b threading): every mint from the convex
/// box×box pierce fixture carries unit-normal owner planes that evaluate to
/// ~0 at the pierce point, with `material_beyond == Some(false)` on BOTH (a
/// box edge is convex — material does NOT extend beyond either incident
/// plane). This pin fixes the material-direction sign empirically: the
/// material-LEFT draft of `owner_trim_planes` read every convex box edge as
/// reflex and this assertion caught it (the loop convention is
/// material-RIGHT of travel, u = t̂ × n̂).
#[test]
fn box_pierce_provenance_is_convex_and_on_plane() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.3, 0.3, 0.5], [0.7, 0.7, 1.5]);
    let out = junction_pierce_points(&a, &b);
    let mut seen = 0usize;
    for pierces in out.values() {
        for pp in pierces {
            seen += 1;
            let j = pp.point.as_array();
            for plane in pp.owner_planes {
                let n_len =
                    (plane.n[0] * plane.n[0] + plane.n[1] * plane.n[1] + plane.n[2] * plane.n[2])
                        .sqrt();
                assert!(
                    (n_len - 1.0).abs() < 1e-12,
                    "owner plane normal must be unit, got {n_len}"
                );
                let dist = plane.n[0] * j[0] + plane.n[1] * j[1] + plane.n[2] * j[2] + plane.d;
                assert!(
                    dist.abs() < 1e-9,
                    "pierce point must lie on its owner plane, got {dist}"
                );
                assert_eq!(
                    plane.material_beyond,
                    Some(false),
                    "box edges are convex — material must NOT extend beyond"
                );
            }
        }
    }
    assert!(seen > 0, "fixture must produce pierces");
}

/// OP RESOLUTION table (inc-4b): zero-content-beyond depends on the op and
/// the owner side. Pin every combination — the measured live fires are
/// Union+reflex (F0082's rising-wall corners) and Subtract+tool-owner+reflex
/// (R0061's zigzag tool corners); Subtract+base-owner and Intersect trim on
/// CONVEX planes (beyond = outside the result); XOR and undetermined
/// geometry never trim.
#[test]
fn resolve_trim_beyond_pins_the_op_owner_table() {
    use cad_primitives::BoolOp::*;
    let reflex = Some(true);
    let convex = Some(false);
    // (op, owner, material_beyond) -> trim
    let cases = [
        (Union, InputId::A, reflex, true),
        (Union, InputId::B, reflex, true),
        (Union, InputId::A, convex, false),
        (Union, InputId::B, convex, false),
        (Subtract, InputId::A, reflex, false),
        (Subtract, InputId::A, convex, true),
        (Subtract, InputId::B, reflex, true),
        (Subtract, InputId::B, convex, false),
        (Intersect, InputId::A, reflex, false),
        (Intersect, InputId::A, convex, true),
        (Intersect, InputId::B, convex, true),
        (Xor, InputId::A, reflex, false),
        (Xor, InputId::B, convex, false),
        (Union, InputId::A, None, false),
        (Subtract, InputId::B, None, false),
    ];
    for (op, owner, mb, expect) in cases {
        assert_eq!(
            resolve_trim_beyond(op, owner, mb),
            expect,
            "resolve({op:?}, {owner:?}, {mb:?})"
        );
    }
}

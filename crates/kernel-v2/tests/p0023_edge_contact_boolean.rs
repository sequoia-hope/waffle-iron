//! P0023 / P0024 (2026-10-03): two cubes meeting along ONE EDGE, flush in
//! the third axis — a SILENT WRONG in both `Intersect` and `Union`.
//!
//! Shape. `A = [0, s]³` and `B = A + d` where `d` has a full side `s` in two
//! axes and zero in the third (`s = 0.01`, the 10 mm cube). The two solids
//! then share exactly one EDGE — the segment where their two offset axes
//! both touch — and nothing else: their interiors are disjoint, their
//! contact set is one-dimensional, and the pair of faces normal to the
//! zero-offset axis is COPLANAR (two coplanar input face pairs, one at each
//! end of the flush axis). Stage 0 of Yang §4.5.5 therefore has real work
//! to do on this input.
//!
//! The honest answers:
//!
//! - `Intersect` ⇒ no solid. `A ∩ B` is a one-dimensional segment; a
//!   boolean that returns solids has nothing to return, so the right answer
//!   is an EMPTY result (the engine's "a disjoint Intersect leaves no solid"
//!   path), typed and loud.
//! - `Union` ⇒ a NON-MANIFOLD solid, which this kernel does not represent.
//!   `A ∪ B` has a non-manifold edge: four faces meet along the shared
//!   segment and the solid pinches to a line there. The right answer is a
//!   typed non-manifold refusal (or two bodies, if the product's data model
//!   says so) — never a silent operand drop. Memory records the same call
//!   for two bosses meeting along an edge.
//!
//! What the kernel measured instead (2026-10-03): `Intersect` returns a
//! COPY OF OPERAND A — volume `s³`, A's centroid, A's bounding box — and
//! `Union` returns a copy of ONE OPERAND, dropping the other. No STOP, no
//! warning. Two of the three orientations fail.
//!
//! ## 2026-10-03 (late): deviation N69 RESOLVED — the HONEST answer is pinned
//!
//! Graze-aware §5 ray selection (`cherchi_rs::labeling::inside_out`, the N69
//! remediation) rejects a candidate ray whose supporting line is exactly
//! coplanar with a foreign candidate triangle and advances to the next
//! non-border origin, then to Y and Z. On this configuration the first +X
//! ray from A's corner `(0, s, s)` ran straight along B's `y = s ∩ z = s`
//! EDGE; the ladder rejects it and finds an origin whose ray crosses, so all
//! three orientations now produce the labels the one lucky orientation
//! always had — and with them the honest answers, measured identical across
//! all three:
//!
//! - `Intersect` ⇒ `KernelV2Error::EmptyBooleanResult`
//! - `Union` ⇒ `InvalidBooleanOutput("an undirected output edge is not used
//!   by exactly two directed edges")` — the non-manifold edge said out loud
//!
//! So this file pins the honest refusal, not merely "a loud refusal": each
//! test asserts the named answer AND that the P0023 containment net
//! (`InnerLabelOutsideInputBounds`) stayed SILENT — the net is the fallback
//! for an unsound label, and a sound label must not need it. Running with
//! `CHERCHI_GRAZE_AWARE_RAY=0` (the N69 kill switch) puts the net back in
//! the firing line, which is the mutation check for this pin.
//!
//! Each assertion is on a POSITIVE quantity (the returned volume, or the
//! named content of the refusal), never on the absence of a panic.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, geom::signed_volume, BrepArena, Profile, SolidId};

/// The 10 mm cube — the scale the defect was measured at.
const S: f64 = 0.01;

/// A `side` cube with its near corner at `at`.
fn corner_cube(arena: &mut BrepArena, at: [f64; 3], side: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, at[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(at[0], at[1]),
            Point2::new(at[0] + side, at[1]),
            Point2::new(at[0] + side, at[1] + side),
            Point2::new(at[0], at[1] + side),
        ],
        vec![],
    )
    .expect("cube profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), side)
        .expect("extrude")
        .solid
}

/// Every acceptable answer here is a REFUSAL, and the refusal has to name
/// something. Two shapes qualify:
///
/// - the P0023 containment net
///   (`InnerLabelOutsideInputBounds` — a patch labeled inside an input it
///   does not even meet), which is what fires today on the two broken
///   orientations; or
/// - a named reassembly wall the honest answer routes through. Measured
///   2026-10-03: the ONE orientation whose labels are already correct
///   ("shared edge along x") reaches reassembly and STOPs with
///   `InvalidBooleanOutput("an undirected output edge is not used by
///   exactly two directed edges")` — which IS the non-manifold union said
///   out loud, four faces along the shared segment. That is the right
///   answer for `Union` on this configuration, and it shows the kernel
///   already refuses the non-manifold union whenever Stage 2 hands it
///   correct labels.
/// - an empty Intersect result, once graze-aware ray selection (deviation
///   N69) makes every orientation's labels correct.
///
/// A bare "boolean failed" with no named cause would NOT be loud enough to
/// tell those apart, so the assertion is on the named content.
fn assert_refusal_is_loud(e: &kernel_v2::KernelV2Error, label: &str) {
    let text = e.to_string();
    let named = [
        "InnerLabelOutsideInputBounds",
        "not inside",
        "empty",
        "Empty",
        "manifold",
        "Manifold",
        "not used by exactly two directed edges",
    ];
    assert!(
        named.iter().any(|n| text.contains(n)),
        "{label}: the refusal must NAME its cause, got {text:?}"
    );
}

/// N69: with graze-aware ray selection the §5 labels on this configuration
/// are SOUND, so the P0023 containment net must stay silent. The net firing
/// means a patch is still being labeled inside an input it does not meet —
/// the defect, caught one layer late. (Mutation check: with
/// `CHERCHI_GRAZE_AWARE_RAY=0` this assertion fails on two of the three
/// orientations, which is exactly the pre-N69 state.)
fn assert_the_net_stayed_silent(e: &kernel_v2::KernelV2Error, label: &str) {
    let text = e.to_string();
    assert!(
        !text.contains("InnerLabelOutsideInputBounds"),
        "{label}: the P0023 containment net fired — the §5 labels are still \
         unsound, so graze-aware ray selection (N69) is not doing its job. \
         Got {text:?}"
    );
}

/// The three orientations of the same configuration: the offset is a full
/// side in two axes and zero (flush) in the third, so the named axis is the
/// direction of the shared edge.
fn orientations() -> [([f64; 3], &'static str); 3] {
    [
        ([S, S, 0.0], "shared edge along z"),
        ([S, 0.0, S], "shared edge along y"),
        ([0.0, S, S], "shared edge along x"),
    ]
}

/// P0023: the `Intersect` of two cubes that share one edge has NO solid in
/// it. Returning a solid at all is wrong; returning operand A is the
/// measured silent wrong.
#[test]
fn edge_contact_intersect_is_empty_never_an_operand() {
    for (d, label) in orientations() {
        let mut arena = BrepArena::new();
        let a = corner_cube(&mut arena, [0.0, 0.0, 0.0], S);
        let b = corner_cube(&mut arena, d, S);
        let va = signed_volume(&arena, a).expect("operand A integrates");

        match boolean_op(&mut arena, a, b, BoolOp::Intersect) {
            Err(e) => {
                assert_refusal_is_loud(&e, label);
                assert_the_net_stayed_silent(&e, label);
                // N69: the honest answer, identical on all three
                // orientations — the regularized intersection is empty.
                assert!(
                    matches!(e, kernel_v2::KernelV2Error::EmptyBooleanResult),
                    "{label}: Intersect of two edge-touching cubes must refuse with \
                     EmptyBooleanResult (no solid in a one-dimensional contact set), \
                     got {e}"
                );
            }
            Ok(out) => {
                let v = signed_volume(&arena, out).expect("the result integrates");
                panic!(
                    "{label}: Intersect of two edge-touching cubes returned a solid of \
                     volume {v:e} m³ (operand A is {va:e} m³; ratio {:.6}) — the \
                     intersection of two solids that meet in a SEGMENT has no volume",
                    v / va
                );
            }
        }
    }
}

/// P0024: the `Union` of two cubes that share one edge is non-manifold. A
/// kernel that represents only 2-manifold solids must REFUSE it; what it
/// must never do is return one operand and drop the other, which is what it
/// was measured doing.
#[test]
fn edge_contact_union_never_drops_an_operand() {
    for (d, label) in orientations() {
        let mut arena = BrepArena::new();
        let a = corner_cube(&mut arena, [0.0, 0.0, 0.0], S);
        let b = corner_cube(&mut arena, d, S);
        let va = signed_volume(&arena, a).expect("operand A integrates");
        let vb = signed_volume(&arena, b).expect("operand B integrates");
        // The two cubes meet in a set of measure zero, so the union's volume
        // is the SUM — whatever topology carries it.
        let want = va + vb;

        match boolean_op(&mut arena, a, b, BoolOp::Union) {
            Err(e) => {
                assert_refusal_is_loud(&e, label);
                assert_the_net_stayed_silent(&e, label);
                // N69: the honest answer, identical on all three
                // orientations — four faces meet along the shared segment,
                // and kernel-v2 represents only 2-manifold solids, so the
                // reassembly refuses by NAMING the non-manifold edge.
                // (Two bodies would also be honest; this kernel's data
                // model says refuse, and that is what it does.)
                let text = e.to_string();
                assert!(
                    text.contains("not used by exactly two directed edges"),
                    "{label}: Union of two edge-touching cubes must refuse by naming \
                     the non-manifold edge, got {text:?}"
                );
            }
            Ok(out) => {
                let v = signed_volume(&arena, out).expect("the result integrates");
                // Exact arithmetic on axis-aligned boxes: the sum is exact,
                // so the only honest band is zero. A dropped operand is off
                // by a full half.
                assert!(
                    (v - want).abs() <= 1e-18,
                    "{label}: Union of two edge-touching cubes has volume {want:e} m³ \
                     (A = {va:e} + B = {vb:e}); got {v:e} m³, which is {:.6} of the \
                     sum — an operand was DROPPED",
                    v / want
                );
            }
        }
    }
}

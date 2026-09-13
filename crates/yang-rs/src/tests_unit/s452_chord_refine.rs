//! §4.5.2 local refinement — the `d_ε` refinement rung primitive
//! (spec `specs/yang_452_local_refinement.md` §6, increment 1).
//!
//! `with_refined_chord(f, …)` is Yang §4.5.2's "increase the mesh resolution
//! of the parametric surfaces associated with the erroneous regions"
//! (`refs/text/yang2025_hybrid_boolean.txt:659-670`) expressed as the ONE
//! quantity the paper uses: `d_ε`. These tests pin the rung's algebra — it
//! divides every chord bound, composes on nesting, restores on the way out
//! (including on unwind), and never COARSENS.

use crate::stage1_tessellate::{
    chord_refine_scale, sphere_chord_bound, torus_chord_bound, with_refined_chord,
};

/// Natural density is the paper's `d_ε = 1e-2` at the surface's own scale.
#[test]
fn natural_rung_is_the_paper_d_eps() {
    assert_eq!(chord_refine_scale(), 1.0);
    assert_eq!(torus_chord_bound(3.0, 1.0), 4e-2);
    assert_eq!(sphere_chord_bound(1.0), 1e-2 * 2.0 * 3f64.sqrt());
}

/// The rung DIVIDES every bound — one lever, every surface kind, so the
/// derived Stage-3/4/6 membership bands move with the mesh
/// (`fix_all_gates_sharing_a_metric`).
#[test]
fn rung_divides_every_chord_bound() {
    with_refined_chord(4.0, || {
        assert_eq!(chord_refine_scale(), 4.0);
        assert_eq!(torus_chord_bound(3.0, 1.0), 1e-2);
        assert_eq!(sphere_chord_bound(1.0), 1e-2 * 2.0 * 3f64.sqrt() / 4.0);
    });
    assert_eq!(chord_refine_scale(), 1.0, "the rung is restored");
    assert_eq!(torus_chord_bound(3.0, 1.0), 4e-2);
}

/// Nesting COMPOSES (the paper's loop "is repeated if optimization failure
/// persists" — successive rounds refine further, they do not reset).
#[test]
fn nested_rungs_compose_and_unwind_in_order() {
    with_refined_chord(2.0, || {
        assert_eq!(chord_refine_scale(), 2.0);
        with_refined_chord(3.0, || assert_eq!(chord_refine_scale(), 6.0));
        assert_eq!(chord_refine_scale(), 2.0, "inner rung restored");
    });
    assert_eq!(chord_refine_scale(), 1.0);
}

/// Refinement ONLY. A factor below 1 would coarsen the mesh while loosening
/// every derived band — tolerance widening through the back door (P9) — so it
/// is clamped to the natural rung, never applied.
#[test]
fn factors_below_one_and_non_finite_are_clamped_to_natural() {
    for f in [0.5, 1.0, 0.0, -3.0, f64::NAN, f64::INFINITY] {
        with_refined_chord(f, || {
            assert_eq!(
                chord_refine_scale(),
                1.0,
                "factor {f} must not change the rung"
            );
        });
    }
    // …and a non-finite factor inside a live rung leaves that rung alone.
    with_refined_chord(4.0, || {
        with_refined_chord(f64::NAN, || assert_eq!(chord_refine_scale(), 4.0));
    });
}

/// The rung is restored even when the body unwinds — a refinement pass that
/// panics must not leave the process tessellating at the refined density.
#[test]
fn rung_is_restored_on_unwind() {
    let caught = std::panic::catch_unwind(|| {
        with_refined_chord(8.0, || {
            assert_eq!(chord_refine_scale(), 8.0);
            panic!("refinement pass blew up");
        })
    });
    assert!(caught.is_err(), "the panic propagates");
    assert_eq!(chord_refine_scale(), 1.0, "the rung is restored on unwind");
}

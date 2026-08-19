//! Stage-4 relocated-triangle validation: unnormalized area-vector and the
//! post-relocation triangle sanity gate. Extracted move-only from
//! stage4_correct.rs (#159 F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// The Stage-4 DEGENERACY IDENTITY band: a relocated triangle is degenerate
/// (Yang Fig. 11(a) — the relocated point lies ON the constrained edge, a
/// zero-area triangle) when its smallest height is ≤ this fraction of its
/// longest edge. It is the SAME 1e-9 relative identity measure as
/// `stage4_construct::chain_straightness` / `on_segment_interior` — six
/// orders from f64 relocation noise (~1e-15 relative) and six orders from
/// real geometry — and it is SCALE-FREE.
///
/// History (2026-08-19, R0009/R0047 anchor): the previous test was the
/// ABSOLUTE area floor `MIN_FEATURE_SIZE²` (1e-12 m²), which at micro model
/// scale (1e-4 m: R0009, R0047, R0091, R0063, R0072) flagged HEALTHY
/// triangles (h/l 0.007–0.4, area 1e-13..1e-12) as degenerate. The
/// §4.4.1(a) unzip then acted on real triangles — an edge flip on a curved
/// surface (silent geometry change; R0091 ×1, R0072 ×6, R0063 ×68 flips
/// on CORRECT verdicts) — and on R0009 ping-ponged a 4-action cycle to the
/// `split_max_passes` cap (R0047: 5168 actions, 62 s). Feature size is a
/// property of the MODEL; a mesh triangle is not a feature.
pub(crate) const DEGENERACY_IDENTITY_REL: f64 = 1e-9;

/// Scale-free triangle degeneracy ratio: `min_height / max_edge`
/// (= `2·area / max_edge²`), or `0.0` when all three points coincide (no
/// extent at all — degenerate by definition). Ratio 0 = exactly collinear;
/// ~0.87 = equilateral.
pub(crate) fn tri_degeneracy_ratio(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> f64 {
    let d2 = |u: [f64; 3], v: [f64; 3]| {
        let e = [v[0] - u[0], v[1] - u[1], v[2] - u[2]];
        e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
    };
    let l2 = d2(p0, p1).max(d2(p1, p2)).max(d2(p2, p0));
    if l2 == 0.0 || !l2.is_finite() {
        return 0.0;
    }
    let av = tri_area_vector(p0, p1, p2);
    let twice_area = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt();
    twice_area / l2
}

/// Yang Fig. 11(a) degeneracy identity: is the triangle numerically
/// zero-area RELATIVE TO ITS OWN EXTENT (`tri_degeneracy_ratio` ≤
/// [`DEGENERACY_IDENTITY_REL`])? Shared by every Stage-4 degeneracy gate
/// (`validate_relocated_triangles`, the §4.4.1(a) unzip loop's `is_degen`,
/// the mutual-pair arm, the gated cylinder-strip replan) — one metric, one
/// definition (a gate family sharing a metric must move together).
pub(crate) fn tri_is_degenerate(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> bool {
    tri_degeneracy_ratio(p0, p1, p2) <= DEGENERACY_IDENTITY_REL
}

/// Unnormalized triangle area-vector `(p1−p0) × (p2−p0)` (= 2·area·n̂).
pub(crate) fn tri_area_vector(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> [f64; 3] {
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ]
}

/// PR-YR10 (Yang §4.4.1 / §4.4.3 / §4.5 step 4): validate every RELOCATED
/// triangle (one touching a `moved` vertex) for **non-degeneracy** — it must
/// not be numerically collinear ([`tri_is_degenerate`], the scale-free
/// [`DEGENERACY_IDENTITY_REL`] identity; formerly the absolute
/// `MIN_FEATURE_SIZE²` area floor, which mis-fired at micro model scale),
/// else `DegenerateTriangle`. Triangles untouched by relocation are skipped:
/// `boolean()` legitimately keeps near-zero-area arrangement slivers for
/// watertightness, which Stage 4 must not re-litigate.
///
/// **Why there is no per-facet absolute "winding vs analytic normal" gate.**
/// Yang §4.4.1 states plainly that relocating the discrete crossing points onto
/// the exact curve "essentially breaks bijectivity, causing gaps or
/// self-intersections," and that **watertightness is inherited from the
/// mesh-boolean output and repaired locally** (§4.4.3) — it is NOT re-derived
/// per facet. The genuine *reversed-intersection* defect (§4.5.3) is a
/// non-monotonic ordering of points ALONG an intersection curve; that is
/// detected and corrected by the polyline-tangent sweep
/// (`sweep_reversed_intersections`) on the ordered conic loops, which either
/// fixes it (edge-collapse) or STOPs loudly (`Stage4ReversalUnresolved` /
/// `LocalRefinementRequired`). What remains after a monotonic-loop sweep is the
/// benign in-surface self-intersection Yang accepts: e.g. a planar cap-fan
/// triangle bridging the relocated ring to a fixed box corner can locally fold
/// WITHIN its (unchanged) supporting plane when a ring vertex moves outward onto
/// the true circle. That fold does NOT move the cap off its exact `Plane`, does
/// NOT reverse the intersection curve, and does NOT break watertightness (pure
/// relocation leaves mesh connectivity — hence half-edge pairing and χ —
/// untouched). An absolute pointwise `dot(winding, surface_normal) > 0` test
/// false-positives on exactly these facets (verified: the cap facet's kept
/// winding is opposite the box's stored cap normal before
/// `reconstruct_topology`'s Newell orientation pass reconciles it; and a
/// faceted cylinder's facet normal legitimately deviates from the pointwise
/// centroid radial by up to the facet half-angle). The faithful output
/// invariant is therefore: non-degenerate relocated facets + the §4.5.3 sweep +
/// the global `check_watertight_2manifold` gate (§4.4.3) — not a per-facet
/// winding sign.
pub(crate) fn validate_relocated_triangles(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    moved: &std::collections::HashSet<u32>,
) -> Result<(), YangError> {
    let _ = attribution; // attribution no longer consulted (no per-facet normal gate)
    for tri in &mesh.tris {
        // Only triangles incident to a relocated (moved) vertex are validated.
        if !tri.iter().any(|v| moved.contains(v)) {
            continue;
        }
        let p0 = mesh.verts[tri[0] as usize].as_array();
        let p1 = mesh.verts[tri[1] as usize].as_array();
        let p2 = mesh.verts[tri[2] as usize].as_array();
        if tri_is_degenerate(p0, p1, p2) {
            if std::env::var_os("YANG_RELOC_PROBE").is_some() {
                eprintln!(
                    "[reloc-degen] tri={tri:?} moved={:?} p0={p0:?} p1={p1:?} p2={p2:?} ratio={:e}",
                    tri.iter().map(|v| moved.contains(v)).collect::<Vec<_>>(),
                    tri_degeneracy_ratio(p0, p1, p2)
                );
            }
            return Err(YangError::stage4_region_invalid(
                tri[0],
                Stage4InvalidReason::DegenerateTriangle,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod degeneracy_identity_tests {
    //! Pins for the scale-free §4.4.1(a) degeneracy identity (2026-08-19,
    //! R0009/R0047 anchor). Each fixture is red under the retired absolute
    //! `MIN_FEATURE_SIZE²` area floor and green under the identity, or the
    //! reverse — so the pair of tests documents the metric change itself.
    use super::*;

    fn moved_first(mesh: &Mesh) -> std::collections::HashSet<u32> {
        let mut m = std::collections::HashSet::new();
        m.insert(0);
        let _ = mesh;
        m
    }

    /// A HEALTHY triangle at micro model scale (R0009-class: edges ~1e-6,
    /// h/l ≈ 0.4, area 2.4e-13 < the old 1e-12 floor) is NOT degenerate,
    /// and a relocated vertex on it must pass the validation gate.
    #[test]
    fn healthy_micro_scale_triangle_is_not_degenerate() {
        let p0 = [0.0, 0.0, 0.0];
        let p1 = [1.1e-6, 0.0, 0.0];
        let p2 = [0.5e-6, 4.4e-7, 0.0];
        let ratio = tri_degeneracy_ratio(p0, p1, p2);
        assert!(ratio > 0.3, "ratio {ratio:e}");
        assert!(!tri_is_degenerate(p0, p1, p2));
        // Old floor would have flagged it:
        let av = tri_area_vector(p0, p1, p2);
        let area = 0.5 * (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt();
        assert!(area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE);
        let mesh = Mesh::new(
            vec![
                Point3::new(p0[0], p0[1], p0[2]),
                Point3::new(p1[0], p1[1], p1[2]),
                Point3::new(p2[0], p2[1], p2[2]),
            ],
            vec![[0, 1, 2]],
        );
        let attribution = TriangleAttributionMap::empty();
        validate_relocated_triangles(&mesh, &attribution, &moved_first(&mesh))
            .expect("a healthy micro-scale relocated triangle is valid");
    }

    /// A COLLINEAR triangle at macro scale (long edge 1000 m, off-vertex
    /// 1e-7 off the line: area 5e-5 ≫ the old floor) IS degenerate under
    /// the identity (h/l = 1e-10) — the case the absolute floor missed.
    #[test]
    fn collinear_macro_scale_triangle_is_degenerate() {
        let p0 = [0.0, 0.0, 0.0];
        let p1 = [1000.0, 0.0, 0.0];
        let p2 = [400.0, 1e-7, 0.0];
        assert!(tri_is_degenerate(p0, p1, p2));
        let av = tri_area_vector(p0, p1, p2);
        let area = 0.5 * (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt();
        assert!(area > cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE);
        let mesh = Mesh::new(
            vec![
                Point3::new(p0[0], p0[1], p0[2]),
                Point3::new(p1[0], p1[1], p1[2]),
                Point3::new(p2[0], p2[1], p2[2]),
            ],
            vec![[0, 1, 2]],
        );
        let attribution = TriangleAttributionMap::empty();
        let err = validate_relocated_triangles(&mesh, &attribution, &moved_first(&mesh))
            .expect_err("a collinear relocated triangle STOPs");
        assert!(matches!(
            err,
            YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::DegenerateTriangle,
                ..
            }
        ));
    }

    /// The identity band sits between relocation noise and real geometry:
    /// f64-noise collinearity (1e-15 relative) is degenerate, a 1e-8
    /// relative offset is not; fully coincident points are degenerate.
    #[test]
    fn identity_band_separates_noise_from_geometry() {
        assert!(tri_is_degenerate(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.3, 1e-15, 0.0]
        ));
        assert!(!tri_is_degenerate(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.3, 1e-8, 0.0]
        ));
        assert!(tri_is_degenerate(
            [2.0, 3.0, 4.0],
            [2.0, 3.0, 4.0],
            [2.0, 3.0, 4.0]
        ));
        // Needle: two coincident vertices, zero area, positive extent.
        assert!(tri_is_degenerate(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0]
        ));
    }
}

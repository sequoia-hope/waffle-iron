//! Stage-4 relocated-triangle validation: unnormalized area-vector and the
//! post-relocation triangle sanity gate. Extracted move-only from
//! stage4_correct.rs (#159 F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

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
/// triangle (one touching a `moved` vertex) for **non-degeneracy** — its
/// post-relocation area must stay ≥ `MIN_FEATURE_SIZE²`, else
/// `DegenerateTriangle`. Triangles untouched by relocation are skipped:
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
        let nrm = tri_area_vector(p0, p1, p2);
        let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if twice_area * 0.5 < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
            if std::env::var_os("YANG_RELOC_PROBE").is_some() {
                eprintln!(
                    "[reloc-degen] tri={tri:?} moved={:?} p0={p0:?} p1={p1:?} p2={p2:?} 2A={twice_area}",
                    tri.iter().map(|v| moved.contains(v)).collect::<Vec<_>>()
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: tri[0],
                reason: Stage4InvalidReason::DegenerateTriangle,
            });
        }
    }
    Ok(())
}

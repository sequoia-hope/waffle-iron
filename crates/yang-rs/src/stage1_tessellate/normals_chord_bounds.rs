//! Stage-1 tessellation helpers: outward-normal computation (sphere/
//! radial/cone) + triangle orientation + per-surface chord bounds.
//! Extracted move-only from stage1_tessellate.rs (#159 F9 decomposition).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// PR-YR12 (P2b): full outward radial normal of a sphere face at the centroid of
/// `tri` — `normalize(centroid − center)`. The analog of `radial_outward_normal`
/// but with no axis projection (a sphere is isotropic). Used to orient sphere
/// triangle winding via `orient_tri`.
pub(crate) fn sphere_outward_normal(verts: &[Point3], tri: &[u32; 3], center: Point3) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ctr = center.as_array();
    normalize3([cen[0] - ctr[0], cen[1] - ctr[1], cen[2] - ctr[2]])
}

/// PR-YR7: outward radial normal of the cylinder surface at the centroid of
/// `tri` — the component of `(centroid − axis_point)` perpendicular to the
/// axis, normalized. Used to orient lateral triangle winding (governance
/// A15.5). Falls back to the raw radial vector if it is (near-)axial.
pub(crate) fn radial_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    axis_point: [f64; 3],
    axis_unit: [f64; 3],
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let w = [
        cen[0] - axis_point[0],
        cen[1] - axis_point[1],
        cen[2] - axis_point[2],
    ];
    let along = w[0] * axis_unit[0] + w[1] * axis_unit[1] + w[2] * axis_unit[2];
    let radial = [
        w[0] - along * axis_unit[0],
        w[1] - along * axis_unit[1],
        w[2] - along * axis_unit[2],
    ];
    normalize3(radial)
}

/// PR-YR16 (spec §4): outward normal of a cone lateral at the centroid of `tri`.
///
/// The cone normal is TILTED ⟂ the generator (NOT purely radial like the
/// cylinder). A cone point is `P = apex + s·â + s·tanα·r̂` with generator
/// `g = â + tanα·r̂`; the surface normal lies in `span{â, r̂}` ⟂ `g`. Imposing
/// `n·g = 0` on `n = a·r̂ + b·â` gives `b = −a·tanα`, so the outward
/// (positive-radial) normal is `n̂ = unit(r̂ − tanα·â)`. The analog of
/// `radial_outward_normal` / `sphere_outward_normal`, feeding `orient_tri`. The
/// fan-triangle centroid sits at ≈ 2/3 of the way to the rim, so its radial
/// component is never degenerate near the apex.
pub(crate) fn cone_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ax = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    let w = [cen[0] - ap[0], cen[1] - ap[1], cen[2] - ap[2]];
    let along = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
    let radial_vec = [
        w[0] - along * ax[0],
        w[1] - along * ax[1],
        w[2] - along * ax[2],
    ];
    let rhat = normalize3(radial_vec);
    let t = half_angle.tan();
    normalize3([
        rhat[0] - t * ax[0],
        rhat[1] - t * ax[1],
        rhat[2] - t * ax[2],
    ])
}

/// PR-YR7: flip `tri`'s winding (swap last two verts) if its geometric normal
/// `(v1−v0)×(v2−v0)` opposes the analytic outward normal `target`.
pub(crate) fn orient_tri(verts: &[Point3], tri: &mut [u32; 3], target: [f64; 3]) {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let dot = cross[0] * target[0] + cross[1] * target[1] + cross[2] * target[2];
    if dot < 0.0 {
        tri.swap(1, 2);
    }
}

/// The relative chord-bound base `1e-2` — the ONE place the constant lives
/// (governance A14.3; every `*_chord_bound` below multiplies its own scale by
/// this). Debug builds honor the §4.5.2 census knob `YANG_CHORD_REFINE=<f>`
/// (f ≥ 1): every chord bound divides by `f`, uniformly refining ALL curved
/// tessellation densities (≈ √f more segments per full turn) while the
/// derived Stage-3/4/6 membership bands tighten CONSISTENTLY (they call the
/// same functions — `fix_all_gates_sharing_a_metric`). This is the
/// density-ladder lever for adjudicating "is this failure density-limited?"
/// across every surface type (the older `YANG_NSEG_FLOOR` floors only the
/// circle-chain branch, so sphere/cone/torus cases never feel it). Release
/// builds compile the knob out — production density is not configurable.
pub(crate) fn chord_rel() -> f64 {
    const BASE: f64 = 1e-2;
    #[cfg(debug_assertions)]
    {
        if let Some(f) = std::env::var("YANG_CHORD_REFINE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|f| f.is_finite() && *f >= 1.0)
        {
            return BASE / f;
        }
    }
    BASE
}

/// Stage-1 chord bound for an ELLIPSE rim chain (KV14 ellipse-arc re-entry):
/// `d_ε = 1e-2 · major_radius` — the circle chord rule applied at the
/// ellipse's worst-case curvature scale. SINGLE SOURCE (A14.3, spec
/// `yang_s3_ellipse_rim_chord_bound` I3): the Stage-1 ellipse chain pre-pass
/// derives its sampling from this, and Stage-3's
/// `chord_tol_for_curved_owner` fallback reuses the SAME bound for owners
/// whose only curved rims are ellipses.
pub(crate) fn ellipse_chord_bound(major_radius: f64) -> f64 {
    chord_rel() * major_radius
}

/// Stage-3 fallback bound for a curved-owning input with NO Circle rim: the
/// largest Stage-1 ellipse-chain bound over the owner's `Curve::Ellipse`
/// edges (spec `yang_s3_ellipse_rim_chord_bound` T2 — an obliquely-trimmed
/// cylinder re-entering from a prior boolean carries ellipse rims only).
/// `None` when the owner has no ellipse edge either (T3 — the loud producer
/// fault stands).
pub(crate) fn ellipse_rim_chord_bound(edges: &[BRepEdge]) -> Option<f64> {
    edges
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Ellipse { major_radius, .. } => Some(ellipse_chord_bound(major_radius)),
            // KV16: hyperbola rims use the same 1e-2 rule at the conic's
            // scale (the S3 tol-lookup vocabulary lesson — a curved owner
            // whose only curved rims are hyperbolas must resolve a bound).
            Curve::Hyperbola {
                semi_transverse,
                semi_conjugate,
                ..
            } => Some(ellipse_chord_bound(semi_transverse.max(semi_conjugate))),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        })
}

/// PR-YR8 (P2c): the Stage-1 chord-error bound `d_ε = 1e-2 × analytic-AABB-diag`
/// for a solid, derived from its `Curve::Circle` rim edges (spec §4 Blocker 1).
///
/// This is the **single source** (governance A14.3) of the `1e-2` chord-bound
/// constant: both `BRep::new` (which derives the cylinder tessellation `n_seg`
/// from it) and Stage-6 face resolution (which uses it as the per-curved-face
/// membership tolerance, degenerate and non-degenerate alike) call this — there
/// is no second copy of the math or the literal anywhere in the crate.
///
/// Per axis a circle of center `c`, unit normal `n`, radius `r` spans
/// `c_i ± r·√(max(0, 1 − n_i²))`; the AABB is the union of those spans over all
/// rim circles. Returns:
/// - `Some(1e-2 × diag)` when the solid has ≥1 `Curve::Circle` rim (it has a
///   tessellated curved face, so it exposes a chord band), or
/// - `None` when there are no circle rims (an all-planar solid has zero chord
///   error; its faces resolve at `TAU_WORK`, not at a curved band).
pub(crate) fn curved_chord_bound(edges: &[BRepEdge]) -> Option<f64> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for e in edges {
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        {
            any = true;
            let nu = normalize3(normal.as_array());
            let c = center.as_array();
            for i in 0..3 {
                let span = radius * (1.0 - nu[i] * nu[i]).max(0.0).sqrt();
                lo[i] = lo[i].min(c[i] - span);
                hi[i] = hi[i].max(c[i] + span);
            }
        }
    }
    if !any {
        return None;
    }
    let dx = hi[0] - lo[0];
    let dy = hi[1] - lo[1];
    let dz = hi[2] - lo[2];
    let diag = (dx * dx + dy * dy + dz * dz).sqrt();
    Some(chord_rel() * diag)
}

/// PR-YR15: the Stage-1 chord bound for a `Surface::Sphere` tessellation,
/// `d_ε = 1e-2 · 2r√3` (the sphere's bounding-cube diagonal × 1e-2). SINGLE
/// SOURCE OF TRUTH (A14.3): both `tessellate_sphere_face` (which derives the
/// tessellation `n_lon`/`n_lat` from it) and Stage-6 face resolution (`tol_for`,
/// which uses it as the per-sphere-face membership tolerance) call this — there
/// is no second copy of the literal anywhere in the crate.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim circle's AABB diagonal is `2r√2`, which UNDERESTIMATES the sphere's own
/// `2r√3` chord error, so a sphere face must use its own bound here — not the
/// rim band. This is A14.3/A15, not tolerance widening.
pub(crate) fn sphere_chord_bound(radius: f64) -> f64 {
    chord_rel() * 2.0 * radius * 3f64.sqrt()
}

/// KV14 Slice F-3: the Stage-1 chord bound of a `Surface::Torus` PATCH
/// tessellation — `d_ε = 1e-2 · (R + r)`, the budget `tessellate_torus_band`
/// hands the UV-CDT (its meridian step is `√(8·d_ε/r)`, capped at 0.5 rad, so
/// the tube chords sag ≤ d_ε; the toroidal step at the structured spacing
/// sags ≤ d_ε·r/(R + r)). SINGLE SOURCE (A14.3): the band/disk tessellator
/// derives its density from this, and `input_curved_chord_bound` folds it in
/// for an input whose torus faces re-enter through the PATCH path — a lone-loop
/// torus disk (a torus∩cone chord polyline) carries no `Curve::Circle` rim at
/// all, so without its own bound such an input reports NO chord band and
/// Stage 4 refuses to relocate onto its conic edges (`chord_band_none`).
/// STRUCTURED torus laterals (profile circles + seam arc) sample at the rim
/// density and are covered by the rim band; they do not fold this in. This
/// is A14.3/A15, not tolerance widening.
pub(crate) fn torus_chord_bound(major: f64, minor: f64) -> f64 {
    chord_rel() * (major + minor)
}

/// PR-YR16 (spec §3): the Stage-1 chord bound for a `Surface::Cone`
/// tessellation, `d_ε = 1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)`.
/// SINGLE SOURCE OF TRUTH (A14.3) of the cone's `1e-2` literal: both the
/// pre-pass N-sizing (folded in via `min()` whenever a cone face is present)
/// and the test-side oracle compute this exact value, so they agree by
/// construction.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim's AABB diagonal `2R√2` IGNORES the cone height and can EXCEED the cone's
/// honest bound for wide-short cones (`h < 2R`), so a cone face must fold in its
/// own bound — not rely on the rim band alone. This is A14.3/A15, not tolerance
/// widening.
pub(crate) fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    chord_rel() * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

/// The Stage-1 chord bound of ONE specific cone band — the selection band for
/// a cone-owning intersection edge (Stage-3 `cone_chord_tol_for_owner`).
///
/// `band` is the edge's own `Surface::Cone` (as tagged on the arrangement
/// edge). We find the owner face carrying exactly that surface and return its
/// `cone_chord_bound(height, half_angle)` computed from the band's OWN rim(s),
/// taking the MAX-height rim (a frustum has two — the larger radius carries
/// the larger circumferential sagitta, so it is the band's chord bound).
///
/// **Deviation N38.** The pre-fix `cone_chord_tol_for_owner` paired the edge
/// band's apex/half_angle with the FIRST cone face's FIRST rim — an unrelated
/// band on a multi-band gear revolve (R0003: apex of the h≈50 band mixed with
/// a h≈3 band's rim → a nonsense `height` → a 6× too-tight band). The mesh
/// endpoints, legitimately on the h≈50 band's chord (well within its true
/// 0.63 sagitta), then fell outside the bogus 0.10 band and Stage-3 raised a
/// spurious `AmbiguousCurve`. Binding the band to the edge's OWN cone (exact
/// `Surface` match — the surface is a same-source copy off the face) fixes the
/// mismatch while leaving every single-cone case byte-identical (the matched
/// face is the only cone face).
///
/// Returns `None` when no matching cone face carries a `Curve::Circle` rim —
/// the caller keeps its loud producer-fault path.
pub(crate) fn cone_band_chord_bound(
    band: Surface,
    faces: &[BRepFace],
    edges: &[BRepEdge],
) -> Option<f64> {
    let Surface::Cone {
        apex,
        axis_dir,
        half_angle,
    } = band
    else {
        return None;
    };
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    let mut max_h: Option<f64> = None;
    for f in faces {
        if f.surface != band {
            continue;
        }
        for &e_idx in &f.outer_loop {
            if let Curve::Circle { center, .. } = edges[e_idx as usize].curve {
                let c = center.as_array();
                let h = ((c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2])
                    .abs();
                max_h = Some(max_h.map_or(h, |m: f64| m.max(h)));
            }
        }
    }
    max_h.map(|h| cone_chord_bound(h, half_angle))
}

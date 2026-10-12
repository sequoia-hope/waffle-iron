//! Stage-1 operand SELF-CONTACT guard (2026-09-07, R0032 op 3; spec
//! `specs/yang_stage1_self_contact_guard.md`).
//!
//! Yang §4.1.1 states the Stage-1 contract: the discretization of a closed
//! B-Rep model is "a closed, watertight manifold" within the chord bound
//! `d_ε` of the surfaces. A chord band that reaches ANOTHER face of the same
//! solid breaks it — the operand's own mesh self-intersects although its
//! surfaces do not, §4.2.1 Case IV ("the meshes detect intersections that do
//! not exist in surfaces") stated INSIDE one operand. The arrangement then
//! resolves that false crossing exactly and both sheets survive the labeling
//! (they are the same solid's boundary), so the boolean output carries a
//! balanced double cover the Stage-4/6 watertight gates stop on — far from
//! the cause. R0032: the op-2 body's torus face 0 is a 0.62-unit skin over a
//! buried gear-tooth tip (the tooth's rim circle dips inside the torus
//! between two triple junctions); the skin's chord triangles (12-unit chords,
//! sag ≈ 0.6–1.3 at the torus patch budget) pierced the cone bands under it.
//!
//! | | |
//! |---|---|
//! | detect | [`cherchi_rs::detect_improper_contacts`] over the operand mesh — the SAME exact tri–tri classification the arrangement runs, so "improper" means precisely "the arrangement would construct intersection structure for this pair". Index-sharing pairs are outside its sweep (a crossing that also shares a vertex stays the downstream gate's tripwire) |
//! | derive | at a contact point `p` (on both triangles) the true gap between the two surfaces is EXACTLY the sum of the two triangles' deviations there, `τ = dev_a + dev_b`. The rim-pair guard's half-gap margin (`sag ≤ gap/2`) then asks the DOMINANT face — the one whose chord band reached farther — to halve its chord bound: the paper's uniform 2× refinement, derived, applied to the face that caused the contact. A planar face never deviates, so it is never refined; two planar faces in contact are a genuine B-Rep self-intersection (no demand: loud) |
//! | channel | a torus face on the PATCH path (`tessellate_torus_band`, density from `torus_chord_bound`) takes a per-face chord bound; every rim-sampled face (cylinder / cone laterals and their caps) takes the shared rim N — the smallest N whose sagitta at the face's largest rim radius is half the current one (≈ √2·N) |
//! | refine | the Stage-1 driver re-runs the whole pass with the tightened demands, at most [`SELF_CONTACT_ROUNDS`] times |
//! | report | typed [`YangError::Stage1SelfContact`] when the rounds are spent, no face of any contact pair has a channel, or the demand does not tighten — never a silently self-intersecting operand |

use super::*;
use std::collections::BTreeMap;

/// Bounded refinement rounds (each halves the dominant faces' chord bounds,
/// so four rounds are a 16× tighter bound / 4× finer rim N) before an
/// operand whose chord bands keep reaching its own faces becomes the loud
/// stop it is.
pub(crate) const SELF_CONTACT_ROUNDS: usize = 4;

/// What one guard pass found in an operand mesh, and the refinement it
/// derives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelfContactReport {
    /// Improper (exactly classified as intersecting / touching) pairs.
    pub(crate) pairs: usize,
    /// Pairs the exact classifier deferred (degenerate configuration).
    pub(crate) unresolved: usize,
    /// Faces of the first reported pair, `(min, max)`.
    pub(crate) first: (usize, usize),
    /// Shared rim N the rim-sampled dominant faces ask for (`None` when no
    /// contact names a rim-sampled face).
    pub(crate) demand_n: Option<usize>,
    /// Per-face torus PATCH chord bounds (each half its current one).
    pub(crate) face_bounds: BTreeMap<u32, f64>,
}

/// Is the chart-bound channel of a CHART-path cylinder lateral on (spec §2
/// "channel", P0037, 2026-10-12)? `YANG_S1_SELF_CONTACT_CHART=0|off` keeps
/// the pre-P0037 rule (the shared rim N alone) for dev A/B.
pub(crate) fn chart_channel_on() -> bool {
    !matches!(
        std::env::var("YANG_S1_SELF_CONTACT_CHART").as_deref(),
        Ok("0") | Ok("off")
    )
}

/// Exact self-contact scan of one Stage-1 tessellation plus the derived
/// refinement. `None` when the mesh is clean (no improper or unresolved pair
/// anywhere — the fast path, byte-identical for every operand that already
/// meets §4.1.1). `n_used` is the shared rim N the pass chose;
/// `current_face_bounds` the torus patch bounds already in force (a face
/// absent there is at its `torus_chord_bound`).
pub(crate) fn scan_self_contacts(
    tess: &Stage1Tess,
    edges: &[BRepEdge],
    faces: &[BRepFace],
    n_used: Option<usize>,
    current_face_bounds: &BTreeMap<u32, f64>,
) -> Option<SelfContactReport> {
    let contacts = cherchi_rs::detect_improper_contacts(&tess.verts, &tess.tris);
    if contacts.is_clean() {
        return None;
    }
    // Triangle → producing face (the pass records each face's tri range).
    let mut tri_face: Vec<u32> = vec![u32::MAX; tess.tris.len()];
    for (f, r) in tess.face_tri_ranges.iter().enumerate() {
        for slot in &mut tri_face[r.clone()] {
            *slot = f as u32;
        }
    }
    let mut report = SelfContactReport {
        pairs: contacts.improper_pairs.len(),
        unresolved: contacts.unresolved_pairs.len(),
        first: (usize::MAX, usize::MAX),
        demand_n: None,
        face_bounds: BTreeMap::new(),
    };
    // The operand's chord budget as the cylinder chart CDT reads it
    // (`stage1_tessellate_once`'s `operand_chord_budget`, the bound a
    // chart-path lateral's interior census enforces): the "own" bound the
    // chart channel halves.
    let operand_budget: Option<f64> =
        curved_chord_bound(edges).or_else(|| ellipse_rim_chord_bound(edges));
    let mut shown = 0usize;
    let mut relevant = 0usize;
    let tri_pts = |t: u32| -> [Point3; 3] {
        let tri = tess.tris[t as usize];
        [
            tess.verts[tri[0] as usize],
            tess.verts[tri[1] as usize],
            tess.verts[tri[2] as usize],
        ]
    };
    for &(ta, tb) in contacts
        .improper_pairs
        .iter()
        .chain(contacts.unresolved_pairs.iter())
    {
        let (Some(&fa), Some(&fb)) = (tri_face.get(ta as usize), tri_face.get(tb as usize)) else {
            continue;
        };
        if fa == u32::MAX || fb == u32::MAX {
            continue;
        }
        let (fa, fb) = (fa as usize, fb as usize);
        // Two COPLANAR planar faces of one solid are Stage 0's case (§4.5.5
        // — a stepped body's shared plane, the intra-solid flush pairs the
        // coplanar overlay handles with its own exact gates), not a chord
        // artifact: planar triangles have no chord band. Left to Stage 0.
        if coplanar_planar_pair(&faces[fa], &faces[fb]) {
            continue;
        }
        let (pa, pb) = (tri_pts(ta), tri_pts(tb));
        // Adjacency by POSITION: a re-entering lineage-less output tessellates
        // its cap and lateral rims with positional twins (bit-identical or a
        // few ulp apart) instead of one shared index (measured: the
        // `stage6_arc_orientation` pocket operand, 96 → 305 such pairs over
        // four rounds), and the exact sweep — which excludes index-sharing
        // pairs — reports every such touch. The index rule, by position: the
        // arrangement welds twins; they are not a chord band's business.
        if positionally_adjacent(&pa, &pb) {
            continue;
        }
        // A contact point must come from a PROPER crossing (an edge passing
        // through the other triangle's plane strictly inside it); a touch at
        // an edge endpoint or along a coplanar edge is a shared-boundary
        // configuration (a T-junction, a twin seam), not a chord band
        // reaching another face — `None`, skipped.
        let Some(p) = contact_point(pa, pb) else {
            continue;
        };
        relevant += 1;
        if report.first == (usize::MAX, usize::MAX) {
            report.first = (fa.min(fb), fa.max(fb));
        }
        let dev = |f: usize| -> f64 {
            signed_distance_to_surface(faces[f].surface, p)
                .map(f64::abs)
                .unwrap_or(0.0)
        };
        let (da, db) = (dev(fa), dev(fb));
        if std::env::var_os("YANG_SPLIT_PROBE").is_some() && shown < 4 {
            shown += 1;
            eprintln!(
                "[stage1-self-contact-pair] tris ({ta},{tb}) faces ({fa}:{}, {fb}:{}) p={:?} \
                 dev=({da:.3e},{db:.3e}) ta={:?} tb={:?}",
                crate::stage4_correct::surface_kind_name(faces[fa].surface),
                crate::stage4_correct::surface_kind_name(faces[fb].surface),
                p.as_array(),
                tri_pts(ta).map(|q| q.as_array()),
                tri_pts(tb).map(|q| q.as_array())
            );
        }
        let tol = 1e-9 * da.max(db);
        let dominant: Vec<usize> = if da > db + tol {
            vec![fa]
        } else if db > da + tol {
            vec![fb]
        } else if fa == fb {
            vec![fa]
        } else {
            vec![fa, fb]
        };
        for f in dominant {
            refine_face(
                f,
                faces,
                edges,
                n_used,
                operand_budget,
                current_face_bounds,
                &mut report.demand_n,
                &mut report.face_bounds,
            );
        }
    }
    // Every contact was a coplanar planar pair, a positional-twin adjacency
    // or a boundary touch: nothing for this guard.
    if relevant == 0 {
        return None;
    }
    report.pairs = relevant;
    Some(report)
}

/// Do the two triangles share a vertex POSITION within the working rounding
/// band `TAU_WORK · (1 + scale)` — adjacency the index rule cannot see?
fn positionally_adjacent(a: &[Point3; 3], b: &[Point3; 3]) -> bool {
    let scale = a
        .iter()
        .chain(b.iter())
        .map(|p| {
            let q = p.as_array();
            q[0].abs().max(q[1].abs()).max(q[2].abs())
        })
        .fold(0.0f64, f64::max);
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    a.iter().any(|p| {
        b.iter().any(|q| {
            let (p, q) = (p.as_array(), q.as_array());
            (p[0] - q[0]).abs() <= band
                && (p[1] - q[1]).abs() <= band
                && (p[2] - q[2]).abs() <= band
        })
    })
}

/// Both faces planar and on the SAME plane (parallel normals, coincident
/// offsets, either orientation) — the intra-solid coplanar configuration
/// Stage 0 owns.
fn coplanar_planar_pair(a: &BRepFace, b: &BRepFace) -> bool {
    let (Surface::Plane { normal: na, d: da }, Surface::Plane { normal: nb, d: db }) =
        (a.surface, b.surface)
    else {
        return false;
    };
    let (na, nb) = (na.as_array(), nb.as_array());
    let la = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
    let lb = (nb[0] * nb[0] + nb[1] * nb[1] + nb[2] * nb[2]).sqrt();
    if la.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        || lb.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
    {
        return false;
    }
    let cx = na[1] * nb[2] - na[2] * nb[1];
    let cy = na[2] * nb[0] - na[0] * nb[2];
    let cz = na[0] * nb[1] - na[1] * nb[0];
    let sin = (cx * cx + cy * cy + cz * cz).sqrt() / (la * lb);
    if sin > 1e-12 {
        return false;
    }
    let dot = na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2];
    // n·x + d = 0 on both: same plane iff d_a/|n_a| = ±d_b/|n_b| with the
    // sign of the normals' agreement.
    let (oa, ob) = (da / la, db / lb);
    let ob = if dot < 0.0 { -ob } else { ob };
    (oa - ob).abs() <= 1e-12 * (1.0 + oa.abs().max(ob.abs()))
}

/// The refinement channel of one dominant face: a torus PATCH face halves
/// its per-face chord bound; a planar face has no chord band (never
/// refined); every other face asks the shared rim N to halve the sagitta at
/// its largest rim radius (a face with no rim circle has no density channel
/// — nothing is demanded, the driver's loud stop stands).
///
/// A CYLINDER lateral additionally halves its per-face CHART bound (P0037,
/// 2026-10-12): on the seeded chart path (`tessellate_lateral_holed_cdt`)
/// the face's chord band is governed by `ChartBudget`'s `sag_bound =
/// min(budget, demand)` — the bound the interior-edge census enforces — and
/// the rim N only CAPS the seed-grid step. P0037's boss lateral meets a cut
/// cylinder across a 0.1-unit lens on a 36.1 radius with the operand budget
/// at 1.496 (the rim AABB × 1e-2): four rounds raised N 11 → 47 while the
/// chord from the lens corner to the nearest grid row kept spanning 8.9°
/// – 15° (the grid's phase against the corner, never the halving), and the
/// contact survived every round. Halving the bound the census enforces is
/// the spec's derived rule applied to the channel this path actually reads;
/// the tube path ignores the chart budget, so the demand is inert there.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine_face(
    f: usize,
    faces: &[BRepFace],
    edges: &[BRepEdge],
    n_used: Option<usize>,
    operand_budget: Option<f64>,
    current_face_bounds: &BTreeMap<u32, f64>,
    demand_n: &mut Option<usize>,
    face_bounds: &mut BTreeMap<u32, f64>,
) {
    let face = &faces[f];
    let halve_face_bound = |own: Option<f64>, face_bounds: &mut BTreeMap<u32, f64>| {
        let cur = current_face_bounds.get(&(f as u32)).copied().or(own);
        let Some(cur) = cur.filter(|c| c.is_finite() && *c > 0.0) else {
            return;
        };
        let half = cur / 2.0;
        face_bounds
            .entry(f as u32)
            .and_modify(|b| *b = b.min(half))
            .or_insert(half);
    };
    let demand_rim_n = |demand_n: &mut Option<usize>| {
        let r_max = face
            .outer_loop
            .iter()
            .chain(face.inner_loops.iter().flatten())
            .filter_map(|&e| match edges[e as usize].curve {
                Curve::Circle { radius, .. } => Some(radius),
                _ => None,
            })
            .fold(0.0f64, f64::max);
        if let (true, Some(n)) = (r_max > 0.0, n_used) {
            if let Some(want) = rim_n_halving_sagitta(r_max, n) {
                *demand_n = Some(demand_n.map_or(want, |d| d.max(want)));
            }
        }
    };
    match face.surface {
        Surface::Plane { .. } => {}
        Surface::Torus {
            major_radius,
            minor_radius,
            ..
        } if torus_face_takes_patch_path(face, edges, major_radius, minor_radius) => {
            halve_face_bound(
                Some(torus_chord_bound(major_radius, minor_radius)),
                face_bounds,
            );
        }
        Surface::Cylinder { .. } => {
            demand_rim_n(demand_n);
            if chart_channel_on() {
                halve_face_bound(operand_budget, face_bounds);
            }
        }
        _ => demand_rim_n(demand_n),
    }
}

/// The smallest rim segment count above `n_cur` whose sagitta at radius `r`
/// is at most HALF the current one (`sag(r, N) = r(1 − cos(π/N))`): the
/// factor-2 margin of the rim-pair guard, as an N. `None` for a degenerate
/// radius / N, or past 4096 (a true near-tangency the loud stop keeps).
pub(crate) fn rim_n_halving_sagitta(r: f64, n_cur: usize) -> Option<usize> {
    if r.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) || n_cur < 3 {
        return None;
    }
    let sag = |n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let target = sag(n_cur) / 2.0;
    let mut n = n_cur + 1;
    while sag(n) > target {
        n += 1;
        if n > 4096 {
            return None;
        }
    }
    Some(n)
}

/// A point of the contact between two triangles: the mean of every PROPER
/// edge-of-one × plane-of-the-other crossing (strictly inside the edge) that
/// lands inside the other triangle (both directions). `None` when no edge
/// crosses the other's plane inside it — a coplanar, edge-touching or
/// vertex-touching contact is a shared-boundary configuration, not a chord
/// band reaching another face. The exact classification is the
/// certificate; this point only locates where each face's chord band was
/// measured.
pub(crate) fn contact_point(a: [Point3; 3], b: [Point3; 3]) -> Option<Point3> {
    let sub = |p: [f64; 3], q: [f64; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let cross = |p: [f64; 3], q: [f64; 3]| {
        [
            p[1] * q[2] - p[2] * q[1],
            p[2] * q[0] - p[0] * q[2],
            p[0] * q[1] - p[1] * q[0],
        ]
    };
    let norm = |p: [f64; 3]| dot(p, p).sqrt();
    let mut acc = [0.0f64; 3];
    let mut k = 0usize;
    for (t, o) in [(a, b), (b, a)] {
        let o0 = o[0].as_array();
        let o1 = o[1].as_array();
        let o2 = o[2].as_array();
        let n = cross(sub(o1, o0), sub(o2, o0));
        let nn = norm(n);
        if nn.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let diam = norm(sub(o1, o0))
            .max(norm(sub(o2, o1)))
            .max(norm(sub(o0, o2)));
        for i in 0..3 {
            let p = t[i].as_array();
            let q = t[(i + 1) % 3].as_array();
            let sp = dot(n, sub(p, o0));
            let sq = dot(n, sub(q, o0));
            if sp == sq {
                continue; // parallel to the plane (coplanar edge or no crossing)
            }
            if (sp > 0.0 && sq > 0.0) || (sp < 0.0 && sq < 0.0) {
                continue;
            }
            let tt = sp / (sp - sq);
            // PROPER crossing only: an endpoint on the plane is a touch (a
            // shared vertex / T-junction), not a crossing.
            if !(1e-9..=1.0 - 1e-9).contains(&tt) {
                continue;
            }
            let x = [
                p[0] + tt * (q[0] - p[0]),
                p[1] + tt * (q[1] - p[1]),
                p[2] + tt * (q[2] - p[2]),
            ];
            // Inside `o` (edge-function signs against its normal), with a
            // relative slack so a crossing ON an edge of `o` still counts.
            let inside = [(o0, o1), (o1, o2), (o2, o0)].iter().all(|&(u, v)| {
                let e = sub(v, u);
                dot(cross(e, sub(x, u)), n) >= -1e-9 * norm(e) * nn * diam
            });
            if inside {
                acc = [acc[0] + x[0], acc[1] + x[1], acc[2] + x[2]];
                k += 1;
            }
        }
    }
    (k > 0).then(|| Point3::new(acc[0] / k as f64, acc[1] / k as f64, acc[2] / k as f64))
}

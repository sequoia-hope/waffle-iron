//! Stage-0 disc∩convex-polygon containment builder (PR-M8-disc):
//! DiscPair detection, exact convex containment, fan/earclip/annulus
//! triangulations + annulus_tests (extracted verbatim from
//! stage0/mod.rs — spec `specs/stage0_decomposition.md`, increment 4).

#[allow(clippy::wildcard_imports)]
use super::*;

// ════════════════════════════════════════════════════════════════════════
// PR-M8-disc — direct disc∩convex-polygon containment builder
// ════════════════════════════════════════════════════════════════════════

/// Outcome of the direct disc-pair construction.
pub(crate) enum DiscPair {
    /// Handled: final per-face override triangles (face B already winding-
    /// swapped iff `opposite`).
    Handled {
        tris_a: Vec<[Point3; 3]>,
        tris_b: Vec<[Point3; 3]>,
    },
    /// Coplanar disc pair that is disjoint in-plane — benign, no override.
    Empty,
    /// Outside increment 1's scope — the caller raises the loud residue. The
    /// `&str` is the probe sub-tag.
    Wall(&'static str),
}

/// A disc-loop vertex carrying its in-frame 2D position (exact, for
/// orientation/containment; f64, for angular sorting) and its resolved 3D
/// point (shared between both solids).
pub(crate) struct V2 {
    pub(crate) e: ExactPoint2,
    pub(crate) u: f64,
    pub(crate) v: f64,
    pub(crate) p: Point3,
}

/// Build the override triangles for a near-coplanar pair in which exactly one
/// face is a flat circular disc and the other a convex polygon, when one
/// strictly contains the other (§4.5.5, the dominant M8 sub-class).
///
/// The disc keeps its exact Stage-1 rim ring (so the override is conformal
/// with the cylinder lateral that shares it); the contained region is a
/// shared rim/boundary triangulation emitted IDENTICALLY to both solids, and
/// the surrounding region is an angular-merge annulus on the larger face.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_disc_pair(
    a: &BRep,
    b: &BRep,
    face_a: usize,
    face_b: usize,
    va: &[Point3],
    vb: &[Point3],
    frame: &Frame,
    opposite: bool,
) -> DiscPair {
    let da = disc_circle_edge(a, face_a);
    let db = disc_circle_edge(b, face_b);
    // disc∩disc (e.g. a bearing recess: a small cylinder cap coplanar with a
    // larger cylinder cap) — both faces keep their exact Stage-1 rim rings, so
    // the containment build stays conformal with BOTH cylinder laterals.
    if da.is_some() && db.is_some() {
        return build_disc_disc_containment(a, b, face_a, face_b, va, vb, frame, opposite);
    }
    let disc_is_a = da.is_some();
    let (disc_brep, disc_fi, disc_coords) = if disc_is_a {
        (a, face_a, va)
    } else {
        (b, face_b, vb)
    };
    let (poly_brep, poly_fi, poly_coords) = if disc_is_a {
        (b, face_b, vb)
    } else {
        (a, face_a, va)
    };

    // Disc rim (exact Stage-1 ring, CCW in frame) + centre as 2D/3D verts.
    let Some(rim3) = disc_rim_ring(disc_brep, disc_fi, disc_coords, frame) else {
        return DiscPair::Wall("disc-rim");
    };
    let circle_e = da.or(db).expect("one disc");
    let Curve::Circle { center, .. } = disc_brep.edges()[circle_e as usize].curve else {
        return DiscPair::Wall("disc-rim");
    };
    let disc: Vec<V2> = match rim3.iter().map(|&p| mk_v2(p, frame)).collect() {
        Some(v) => v,
        None => return DiscPair::Wall("disc-rim"),
    };
    let center_v = match mk_v2(center, frame) {
        Some(v) => v,
        None => return DiscPair::Wall("disc-rim"),
    };

    // Convex polygon corners (must be hole-free; CCW in frame).
    if !poly_brep.faces()[poly_fi].inner_loops.is_empty() {
        return DiscPair::Wall("disc-poly-holed");
    }
    let Some(poly_ring) =
        loop_vertex_ring(poly_brep.edges(), &poly_brep.faces()[poly_fi].outer_loop)
    else {
        return DiscPair::Wall("disc-poly-loop");
    };
    let poly: Vec<V2> = match poly_ring
        .iter()
        .map(|&vi| mk_v2(poly_coords[vi as usize], frame))
        .collect()
    {
        Some(v) => v,
        None => return DiscPair::Wall("disc-poly-loop"),
    };
    let Some(poly) = orient_ccw(poly) else {
        return DiscPair::Wall("disc-poly-degenerate");
    };
    if !is_strictly_convex(&poly) {
        return DiscPair::Wall("disc-poly-nonconvex");
    }
    // `disc` is convex by construction but re-orient defensively (the rim is
    // already CCW in frame).
    let Some(disc) = orient_ccw(disc) else {
        return DiscPair::Wall("disc-degenerate");
    };

    // Containment: which shape is strictly inside the other? (Strict — a
    // tangency or crossing falls through to the loud residue.)
    let disc_in_poly = disc.iter().all(|v| strictly_inside_convex(&poly, &v.e));
    let poly_in_disc = poly.iter().all(|v| strictly_inside_convex(&disc, &v.e));

    let (inner, outer, center_opt): (&[V2], &[V2], Option<&V2>) = if disc_in_poly {
        (&disc, &poly, Some(&center_v))
    } else if poly_in_disc {
        (&poly, &disc, None)
    } else if convex_rings_overlap(&disc, &poly) {
        // Partial overlap: a circle×segment crossing (irrational on the
        // sampled ring) plus boundary-split propagation — a deferred slice.
        return DiscPair::Wall("disc-crossing");
    } else {
        // Coplanar but disjoint in-plane (the scan's AABBs overlap, the
        // shapes do not): benign — the exact arrangement passes the coplanar
        // non-overlap through (deviation N17). Nothing to override.
        return DiscPair::Empty;
    };

    // OVERLAP = the inner region; emitted to BOTH faces. A disc inner uses a
    // rim fan about its centre; a polygon inner uses an ear-clip.
    let Some(overlap) = (match center_opt {
        Some(c) => fan_tris(c, inner),
        None => earclip_tris(inner),
    }) else {
        return DiscPair::Wall("disc-overlap-tri");
    };
    // OUTER-only = `outer` with `inner` as a hole; emitted to the larger face.
    let Some(annulus) = annulus_tris(outer, inner) else {
        return DiscPair::Wall("disc-annulus-tri");
    };

    // The larger face owns the annulus; both faces own the overlap. Triangles
    // are frame-CCW (normal = +n̂ = face A's outward normal): face A keeps
    // them, face B swaps iff opposite.
    let outer_is_disc = poly_in_disc; // when poly⊆disc, the disc is larger
    let mut disc_face_tris = overlap.clone();
    let mut poly_face_tris = overlap;
    if outer_is_disc {
        disc_face_tris.extend(annulus);
    } else {
        poly_face_tris.extend(annulus);
    }
    let (tris_a, mut tris_b) = if disc_is_a {
        (disc_face_tris, poly_face_tris)
    } else {
        (poly_face_tris, disc_face_tris)
    };
    if opposite {
        for t in &mut tris_b {
            t.swap(1, 2);
        }
    }
    DiscPair::Handled { tris_a, tris_b }
}

/// A disc face's exact Stage-1 rim ring (frame coords) plus its centre, both as
/// `V2`. The rim is bit-identical to what the cylinder lateral sharing it gets,
/// so any override built from it stays conformal.
pub(crate) fn disc_ring_and_center(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(Vec<V2>, V2)> {
    let circle_e = disc_circle_edge(brep, fi)?;
    let Curve::Circle { center, .. } = brep.edges()[circle_e as usize].curve else {
        return None;
    };
    let rim3 = disc_rim_ring(brep, fi, coords, frame)?;
    let ring: Vec<V2> = rim3
        .iter()
        .map(|&p| mk_v2(p, frame))
        .collect::<Option<_>>()?;
    let center_v = mk_v2(center, frame)?;
    Some((ring, center_v))
}

/// Build override triangles for a near-coplanar pair where BOTH faces are flat
/// circular discs and one rim strictly contains the other (the §4.5.5
/// disc∩disc containment sub-class — a bearing recess / coaxial cap-on-cap).
///
/// Mirrors [`build_disc_pair`]'s containment build: the OVERLAP is the inner
/// disc fanned about its own centre (emitted identically to both solids), and
/// the larger disc additionally owns the angular-merge ANNULUS between the two
/// rims. Both rims are kept exactly (each shared with its cylinder lateral).
/// Crossing rims defer to Increment 2 (`Wall("disc-disc-crossing")`); a benign
/// disjoint coplanar pair returns `Empty`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_disc_disc_containment(
    a: &BRep,
    b: &BRep,
    face_a: usize,
    face_b: usize,
    va: &[Point3],
    vb: &[Point3],
    frame: &Frame,
    opposite: bool,
) -> DiscPair {
    let (Some((ring_a, center_a)), Some((ring_b, center_b))) = (
        disc_ring_and_center(a, face_a, va, frame),
        disc_ring_and_center(b, face_b, vb, frame),
    ) else {
        return DiscPair::Wall("disc-rim");
    };
    let (Some(ring_a), Some(ring_b)) = (orient_ccw(ring_a), orient_ccw(ring_b)) else {
        return DiscPair::Wall("disc-degenerate");
    };

    // Strict containment (a tangency or crossing falls through, as in the
    // disc∩polygon path).
    let a_in_b = ring_a.iter().all(|v| strictly_inside_convex(&ring_b, &v.e));
    let b_in_a = ring_b.iter().all(|v| strictly_inside_convex(&ring_a, &v.e));
    // (inner, outer, inner-centre, inner_is_a)
    let (inner, outer, inner_center, inner_is_a) = if a_in_b {
        (&ring_a, &ring_b, &center_a, true)
    } else if b_in_a {
        (&ring_b, &ring_a, &center_b, false)
    } else if convex_rings_overlap(&ring_a, &ring_b) {
        // CROSSING rims (a lens overlap, neither contained). No Stage-0
        // override: the two caps keep their default conformal Stage-1 fans and
        // cherchi's coplanar arrangement (single-coplanar-edge N13 + the
        // fully-coplanar PRs 1-4 pocket dedup) resolves the coplanar lens
        // directly — the explicit two-disc lens construction the overlay would
        // need is unnecessary now that cherchi handles coplanar overlap. (A
        // genuine disjoint pair returns `Empty` below; a crossing produces a
        // real coplanar overlap cherchi must arrange, but the keep/drop is the
        // same `Empty` no-override path.)
        return DiscPair::Empty;
    } else {
        return DiscPair::Empty;
    };

    let Some(overlap) = fan_tris(inner_center, inner) else {
        return DiscPair::Wall("disc-overlap-tri");
    };
    let Some(annulus) = annulus_tris(outer, inner) else {
        return DiscPair::Wall("disc-annulus-tri");
    };

    // Triangles are frame-CCW (= face A's outward normal): the inner face owns
    // the overlap, the outer face owns overlap + annulus. Face A keeps frame-CCW
    // winding; face B swaps iff its outward normal opposes the canonical one.
    let (tris_a, mut tris_b) = if inner_is_a {
        let mut outer_t = overlap.clone();
        outer_t.extend(annulus);
        (overlap, outer_t)
    } else {
        let mut outer_t = overlap.clone();
        outer_t.extend(annulus);
        (outer_t, overlap)
    };
    if opposite {
        for t in &mut tris_b {
            t.swap(1, 2);
        }
    }
    DiscPair::Handled { tris_a, tris_b }
}

/// Lift a 3D point to a `V2` (in-frame 2D + the original 3D point).
pub(crate) fn mk_v2(p: Point3, frame: &Frame) -> Option<V2> {
    let (u, v) = frame.project(p);
    Some(V2 {
        e: ExactPoint2::from_f64(u, v)?,
        u,
        v,
        p,
    })
}

/// Re-orient a ring CCW in the frame (exact shoelace); `None` if degenerate.
pub(crate) fn orient_ccw(ring: Vec<V2>) -> Option<Vec<V2>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut area2 = RBig::ZERO;
    for i in 1..n - 1 {
        area2 += cross_r(&ring[0].e, &ring[i].e, &ring[i + 1].e);
    }
    if area2 == RBig::ZERO {
        return None;
    }
    if area2 > RBig::ZERO {
        Some(ring)
    } else {
        Some(ring.into_iter().rev().collect())
    }
}

/// Strictly convex CCW polygon: every corner turns strictly left.
pub(crate) fn is_strictly_convex(ring: &[V2]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    (0..n).all(|i| cross_r(&ring[(i + n - 1) % n].e, &ring[i].e, &ring[(i + 1) % n].e) > RBig::ZERO)
}

/// Is `q` strictly inside the convex CCW polygon `ring`?
pub(crate) fn strictly_inside_convex(ring: &[V2], q: &ExactPoint2) -> bool {
    let n = ring.len();
    (0..n).all(|i| cross_r(&ring[i].e, &ring[(i + 1) % n].e, q) > RBig::ZERO)
}

/// Do two convex CCW rings overlap with positive area? A vertex of one
/// strictly inside the other, or a proper edge crossing (the rotated-rectangle
/// case with no vertex inside). Exact. Used only to tell a benign disjoint
/// coplanar pair from a partial-overlap (crossing) one.
pub(crate) fn convex_rings_overlap(a: &[V2], b: &[V2]) -> bool {
    if a.iter().any(|v| strictly_inside_convex(b, &v.e))
        || b.iter().any(|v| strictly_inside_convex(a, &v.e))
    {
        return true;
    }
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        let (a0, a1) = (&a[i].e, &a[(i + 1) % na].e);
        for j in 0..nb {
            let (b0, b1) = (&b[j].e, &b[(j + 1) % nb].e);
            if segs_properly_cross(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    false
}

/// Do open segments `p0p1` and `q0q1` cross at a single interior point? (Both
/// endpoints of each strictly straddle the other's supporting line.)
pub(crate) fn segs_properly_cross(
    p0: &ExactPoint2,
    p1: &ExactPoint2,
    q0: &ExactPoint2,
    q1: &ExactPoint2,
) -> bool {
    let d1 = cross_r(p0, p1, q0);
    let d2 = cross_r(p0, p1, q1);
    let d3 = cross_r(q0, q1, p0);
    let d4 = cross_r(q0, q1, p1);
    ((d1 > RBig::ZERO) != (d2 > RBig::ZERO))
        && (d1 != RBig::ZERO && d2 != RBig::ZERO)
        && ((d3 > RBig::ZERO) != (d4 > RBig::ZERO))
        && (d3 != RBig::ZERO && d4 != RBig::ZERO)
}

/// Fan a convex CCW ring about an interior apex (the disc centre).
pub(crate) fn fan_tris(apex: &V2, ring: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push([apex.p, ring[i].p, ring[(i + 1) % n].p]);
    }
    Some(out)
}

/// Ear-clip a simple CCW ring into frame-CCW triangles.
pub(crate) fn earclip_tris(ring: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let pts: Vec<ExactPoint2> = ring.iter().map(|v| v.e.clone()).collect();
    let idx = crate::coplanar_overlay::ear_clip(&pts).ok()?;
    Some(
        idx.into_iter()
            .map(|[i, j, k]| [ring[i].p, ring[j].p, ring[k].p])
            .collect(),
    )
}

/// Triangulate `outer` (convex CCW) minus `inner` (convex CCW, strictly
/// inside) — the annular region between two nested convex rings.
///
/// Both rings are star-shaped about the inner ring's centroid `O` (interior to
/// `inner` by convexity, hence to `outer` since `inner ⊆ outer`), so their
/// vertices are angularly monotone about `O`. The annulus is the strip between
/// the two monotone chains; it triangulates by an angular merge (advance the
/// chain whose next vertex comes first in angle), each triangle oriented
/// frame-CCW exactly. No keyhole, no Steiner points — every boundary vertex of
/// both rings is preserved, so the inner ring stays bit-shared with the
/// overlap fan and the cylinder lateral.
pub(crate) fn annulus_tris(outer: &[V2], inner: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let (ni, no) = (inner.len(), outer.len());
    if ni < 3 || no < 3 {
        return None;
    }
    let ox: f64 = inner.iter().map(|v| v.u).sum::<f64>() / ni as f64;
    let oy: f64 = inner.iter().map(|v| v.v).sum::<f64>() / ni as f64;
    let ang = |v: &V2| (v.v - oy).atan2(v.u - ox);

    // A ring → an ascending-unwrapped angle chain starting at its min-angle
    // vertex, with the start vertex appended again (closing the loop at
    // angle a0 + 2π).
    let chain = |ring: &[V2]| -> (Vec<usize>, Vec<f64>) {
        let n = ring.len();
        let start = (0..n)
            .min_by(|&a, &b| ang(&ring[a]).partial_cmp(&ang(&ring[b])).unwrap())
            .unwrap();
        let mut order = Vec::with_capacity(n + 1);
        let mut angs = Vec::with_capacity(n + 1);
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=n {
            let idx = (start + k) % n;
            let mut a = ang(&ring[idx]);
            while a <= prev {
                a += std::f64::consts::TAU;
            }
            prev = a;
            order.push(idx);
            angs.push(a);
        }
        (order, angs)
    };
    let (io, ia) = chain(inner);
    let (oo, oa) = chain(outer);

    // Exact centroid for the half-plane visibility guards (spec
    // `m8_stage0_fold_pair_emission` E-F1..E-F3): strictly interior to the
    // convex inner ring, so it decides which side of a chord's supporting
    // line is "inner". Exact sign tests only — no tolerances (A14.3).
    let o_exact = ExactPoint2::from_f64(ox, oy)?;

    // Merge the two monotone chains into a strip triangulation.
    let tri = |a: &V2, b: &V2, c: &V2| -> [Point3; 3] {
        if cross_r(&a.e, &b.e, &c.e) >= RBig::ZERO {
            [a.p, b.p, c.p]
        } else {
            [a.p, c.p, b.p]
        }
    };
    // E-F1/E-F2: an inner-advance triangle (chord c1→c2 fanned to outer P)
    // is valid iff P lies STRICTLY on the opposite side of the chord's
    // supporting line from O. Angular monotonicity alone does not imply
    // this (measured, F0027: a far square corner falls on the CENTER side
    // of a distant chord's line → the fan double-covers the disc pocket —
    // the fold-pair census class). Returns the triangle's exact area (×2)
    // for the E-F4 certificate.
    let inner_valid = |i: usize, j: usize| -> Option<RBig> {
        let (c1, c2) = (&inner[io[i]], &inner[io[i + 1]]);
        let s_p = cross_r(&c1.e, &c2.e, &outer[oo[j]].e);
        let s_o = cross_r(&c1.e, &c2.e, &o_exact);
        if s_p == RBig::ZERO || s_o == RBig::ZERO {
            return None;
        }
        if (s_p > RBig::ZERO) == (s_o > RBig::ZERO) {
            return None;
        }
        Some(if s_p > RBig::ZERO { s_p } else { -s_p })
    };
    // E-F3: an outer-advance triangle (outer edge o1→o2 with inner apex Q)
    // is valid iff Q lies STRICTLY on O's side of the outer edge's line
    // (guaranteed by convex nesting; a violation is a loud E-F5).
    let outer_valid = |i: usize, j: usize| -> Option<RBig> {
        let (o1, o2) = (&outer[oo[j]], &outer[oo[j + 1]]);
        let s_q = cross_r(&o1.e, &o2.e, &inner[io[i]].e);
        let s_o = cross_r(&o1.e, &o2.e, &o_exact);
        if s_q == RBig::ZERO || s_o == RBig::ZERO {
            return None;
        }
        if (s_q > RBig::ZERO) != (s_o > RBig::ZERO) {
            return None;
        }
        Some(if s_q > RBig::ZERO { s_q } else { -s_q })
    };
    let mut out: Vec<[Point3; 3]> = Vec::with_capacity(ni + no);
    let mut covered2 = RBig::ZERO;
    let (mut i, mut j) = (0usize, 0usize);
    let mut guard = 0usize;
    while i < ni || j < no {
        guard += 1;
        if guard > ni + no + 8 {
            return None;
        }
        // Angle preference as before; validity redirects an invalid
        // preferred advance to the other chain (E-F2), and a step where
        // NEITHER advance is valid is a loud `None` (E-F5) — never a
        // silently-flipped or invisible fan.
        let prefer_inner = if i >= ni {
            false
        } else if j >= no {
            true
        } else {
            ia[i + 1] <= oa[j + 1]
        };
        let inner_ok = if i < ni && j < no {
            inner_valid(i, j)
        } else if i < ni && j >= no {
            // Outer chain exhausted: the closing outer vertex is oo[no]
            // (== oo[0]); the guard still applies against it.
            inner_valid(i, no)
        } else {
            None
        };
        let outer_ok = if j < no && i < ni {
            outer_valid(i, j)
        } else if j < no && i >= ni {
            outer_valid(ni, j)
        } else {
            None
        };
        let advance_inner = match (prefer_inner, &inner_ok, &outer_ok) {
            (true, Some(_), _) => true,
            (true, None, Some(_)) => false,
            (false, _, Some(_)) => false,
            (false, Some(_), None) => true,
            (_, None, None) => return None,
        };
        if advance_inner {
            let jj = if j < no { j } else { no };
            let t = tri(&inner[io[i]], &inner[io[i + 1]], &outer[oo[jj]]);
            covered2 += inner_ok.expect("validated");
            if !degenerate(&t) {
                out.push(t);
            }
            i += 1;
        } else {
            let ii = if i < ni { i } else { ni };
            let t = tri(&outer[oo[j]], &outer[oo[j + 1]], &inner[io[ii]]);
            covered2 += outer_ok.expect("validated");
            if !degenerate(&t) {
                out.push(t);
            }
            j += 1;
        }
    }

    // E-F4 coverage certificate (I2, the `triangulate_ring` P9-gate
    // pattern): the emitted strip covers EXACTLY the region between the
    // rings — no pleat, no gap. Exact shoelace over the same coordinates
    // the triangles use.
    let shoelace2 = |ring: &[V2]| -> RBig {
        let n = ring.len();
        let mut a = RBig::ZERO;
        for k in 0..n {
            let p = &ring[k].e;
            let q = &ring[(k + 1) % n].e;
            a += &p.x * &q.y - &q.x * &p.y;
        }
        if a > RBig::ZERO {
            a
        } else {
            -a
        }
    };
    if covered2 != shoelace2(outer) - shoelace2(inner) {
        return None;
    }
    Some(out)
}

/// A triangle with two coincident vertices (zero geometric extent).
pub(crate) fn degenerate(t: &[Point3; 3]) -> bool {
    t[0] == t[1] || t[1] == t[2] || t[2] == t[0]
}

#[cfg(test)]
mod annulus_tests {
    //! Fold-pair emission RED oracle (spec `m8_stage0_fold_pair_emission`
    //! §6): F0027's measured configuration — a square outer ring whose
    //! corners fall on the CENTER side of distant inner chords' supporting
    //! lines. The angle-only merge fans those chords to invisible corners,
    //! double-covering part of the disc (the misoriented+improper census
    //! class). The exact coverage certificate (I2/E-F4) is the assertion:
    //! Σ triangle areas == area(outer) − area(inner), rational shoelace.

    use super::{annulus_tris, V2};
    use crate::coplanar_overlay::ExactPoint2;
    use cad_primitives::Point3;
    use dashu::rational::RBig;

    const Z: f64 = 0.236530362945883;

    fn v2(u: f64, v: f64) -> V2 {
        V2 {
            e: ExactPoint2::from_f64(u, v).expect("finite"),
            u,
            v,
            p: Point3::new(u, v, Z),
        }
    }

    /// The F0027 rings, verbatim from the dumped defective operand
    /// (square corners CCW; 11-gon rim CCW by ascending azimuth).
    fn f0027_rings() -> (Vec<V2>, Vec<V2>) {
        let outer = [
            (0.24933140012920343, -0.18511094772209571),
            (0.24933140012920343, 0.18511094772209571),
            (-0.24933140012920343, 0.18511094772209571),
            (-0.24933140012920343, -0.18511094772209571),
        ];
        let inner = [
            (-0.10624127713105047, -0.048518765551481664),
            (-0.06314462464930325, -0.09825495384444957),
            (0.0, -0.11679588852813404),
            (0.06314462464930323, -0.09825495384444959),
            (0.10624127713105048, -0.04851876555148163),
            (0.11560707478868232, 0.016621787986866053),
            (0.08826844304146464, 0.07648504128332562),
            (0.032905204303597765, 0.1120648343898067),
            (-0.032905204303597814, 0.11206483438980669),
            (-0.08826844304146471, 0.07648504128332555),
            (-0.11560707478868233, 0.016621787986866008),
        ];
        (
            outer.iter().map(|&(u, v)| v2(u, v)).collect(),
            inner.iter().map(|&(u, v)| v2(u, v)).collect(),
        )
    }

    /// Exact CCW shoelace area (×2) of a ring.
    fn ring_area2(ring: &[V2]) -> RBig {
        let n = ring.len();
        let mut a = RBig::ZERO;
        for i in 0..n {
            let p = &ring[i].e;
            let q = &ring[(i + 1) % n].e;
            a += &p.x * &q.y - &q.x * &p.y;
        }
        a
    }

    /// Exact area (×2) of an emitted triangle, from its (u,v) = (x,y)
    /// in-plane coordinates (the test plane is z=const with normal +z).
    fn tri_area2(t: &[Point3; 3]) -> RBig {
        let e: Vec<ExactPoint2> = t
            .iter()
            .map(|p| ExactPoint2::from_f64(p.x(), p.y()).expect("finite"))
            .collect();
        let dx1 = &e[1].x - &e[0].x;
        let dy1 = &e[1].y - &e[0].y;
        let dx2 = &e[2].x - &e[0].x;
        let dy2 = &e[2].y - &e[0].y;
        &dx1 * &dy2 - &dy1 * &dx2
    }

    /// RED (spec §6): the F0027 annulus must cover EXACTLY the region
    /// between the rings. Today the angle-only merge double-covers two
    /// pockets (fold pairs at corners 1 and 3), so Σ areas exceeds the
    /// annulus area and this certificate fails.
    #[test]
    fn f0027_annulus_coverage_certificate() {
        let (outer, inner) = f0027_rings();
        let tris = annulus_tris(&outer, &inner).expect("annulus must build");
        let annulus2 = ring_area2(&outer) - ring_area2(&inner);
        let mut covered2 = RBig::ZERO;
        let mut folded = 0usize;
        for t in &tris {
            let a2 = tri_area2(t);
            if a2 <= RBig::ZERO {
                folded += 1;
            }
            covered2 += a2;
        }
        assert_eq!(folded, 0, "annulus emitted non-positive-area triangles");
        assert_eq!(
            covered2,
            annulus2,
            "fold-pair RED — annulus triangulation does not cover the region \
             between the rings exactly (spec m8_stage0_fold_pair_emission I2): \
             covered {} vs annulus {} (×2, exact); the surplus is the measured \
             double-cover pleat at the invisible corners",
            covered2.to_f64().value(),
            annulus2.to_f64().value()
        );
    }
}

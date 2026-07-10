//! Stage-0 canonical frame + face→polygon extraction: Frame,
//! overlay gates, loop/ring helpers, 2D polygon construction,
//! in-frame coordinate clustering + frame_cluster_tests (extracted
//! verbatim from stage0/mod.rs — spec `specs/stage0_decomposition.md`,
//! increment 7).

#[allow(clippy::wildcard_imports)]
use super::*;

// ════════════════════════════════════════════════════════════════════════
// canonical frame
// ════════════════════════════════════════════════════════════════════════

/// The pair's canonical shared plane + deterministic 2D frame: face A's
/// unit normal `n` with unit offset `d` (`n·x + d = 0`), an on-plane origin
/// `o = −d·n`, and the in-plane axes `(e1, e2) = ortho_basis(n)`
/// (right-handed: `e1 × e2 = n`).
pub(crate) struct Frame {
    pub(crate) n: [f64; 3],
    pub(crate) d: f64,
    pub(crate) o: [f64; 3],
    pub(crate) e1: [f64; 3],
    pub(crate) e2: [f64; 3],
}

impl Frame {
    /// Project `p` onto the canonical plane along `n` — the §4.5.5 snap.
    /// Exactly the identity for points already on the plane (`t == 0.0`).
    pub(crate) fn snap(&self, p: Point3) -> Point3 {
        let pa = p.as_array();
        let t = self.n[0] * pa[0] + self.n[1] * pa[1] + self.n[2] * pa[2] + self.d;
        Point3::new(
            pa[0] - t * self.n[0],
            pa[1] - t * self.n[1],
            pa[2] - t * self.n[2],
        )
    }

    /// In-plane coordinates of (the plane projection of) `p`.
    pub(crate) fn project(&self, p: Point3) -> (f64, f64) {
        let pa = p.as_array();
        let w = [pa[0] - self.o[0], pa[1] - self.o[1], pa[2] - self.o[2]];
        (
            w[0] * self.e1[0] + w[1] * self.e1[1] + w[2] * self.e1[2],
            w[0] * self.e2[0] + w[1] * self.e2[1] + w[2] * self.e2[2],
        )
    }

    /// The 3D lift `o + u·e1 + v·e2` — the shared coordinate of every NEW
    /// overlay vertex (computed once per overlay vertex, used by BOTH
    /// solids' meshes).
    pub(crate) fn lift(&self, u: f64, v: f64) -> Point3 {
        Point3::new(
            self.o[0] + u * self.e1[0] + v * self.e2[0],
            self.o[1] + u * self.e1[1] + v * self.e2[1],
            self.o[2] + u * self.e1[2] + v * self.e2[2],
        )
    }
}

/// Build the canonical frame from face A's stored plane. `None` for a
/// degenerate normal (rejected loudly by the caller as unsupported).
pub(crate) fn canonical_frame(a: &BRep, face_a: usize) -> Option<Frame> {
    let Surface::Plane { normal, d } = a.faces()[face_a].surface else {
        return None;
    };
    let na = normal.as_array();
    let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
    if len < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let n = [na[0] / len, na[1] / len, na[2] / len];
    let du = d / len;
    let o = [-du * n[0], -du * n[1], -du * n[2]];
    let (e1, e2) = ortho_basis(normal);
    Some(Frame {
        n,
        d: du,
        o,
        e1: e1.as_array(),
        e2: e2.as_array(),
    })
}

// ════════════════════════════════════════════════════════════════════════
// face → polygon helpers
// ════════════════════════════════════════════════════════════════════════

/// Is this face overlay-supported: a planar surface that is EITHER an
/// all-`LineSegment` polygon OR a full-circle disc (PR-M8-disc — the single
/// dominant M8 coplanar sub-class: a cylinder end-cap flush against another
/// planar face). The disc is handled by sampling its rim into the SAME ring
/// Stage 1 uses, then routing it through the existing polygon overlay.
pub(crate) fn overlay_face_supported(brep: &BRep, fi: usize) -> bool {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) {
        return false;
    }
    if disc_circle_edge(brep, fi).is_some() {
        return true;
    }
    // M8 holed-disc (spec `m8_holed_disc_coplanar_overlay`): a planar ANNULAR
    // face — single-circle outer loop + each inner loop a single closed circle
    // — is overlay-eligible (its outer + hole rims sample into the exact
    // `PolygonWithHoles` the overlay already consumes).
    if annular_disc_face(brep, fi).is_some() {
        return true;
    }
    // M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): a planar face whose
    // loops mix `LineSegment` and `Circle`/`Ellipse` edges samples its loops
    // from the face's own Stage-1 chains. (Curved-chord subdivision by the
    // overlap boundary walls later, at the slice-1 gate.)
    if mixed_planar_face(brep, fi) {
        return true;
    }
    std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
        .all(|&e| matches!(brep.edges()[e as usize].curve, Curve::LineSegment))
}

/// If `fi` is a flat circular disc — planar surface, no holes, a single
/// outer-loop edge that is a closed `Curve::Circle` (`start == end`) — return
/// that circle edge's index. Else `None`.
pub(crate) fn disc_circle_edge(brep: &BRep, fi: usize) -> Option<u32> {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) || !f.inner_loops.is_empty() {
        return None;
    }
    if f.outer_loop.len() != 1 {
        return None;
    }
    let e = f.outer_loop[0];
    let edge = &brep.edges()[e as usize];
    matches!(edge.curve, Curve::Circle { .. } if edge.start == edge.end).then_some(e)
}

/// If `fi` is a flat ANNULAR disc — planar surface, a single closed-`Curve::Circle`
/// outer loop, and ≥1 inner loop each a single closed `Curve::Circle` (a bore /
/// swiss-cheese hole) — return `(outer_circle_edge, [hole_circle_edges])`. Else
/// `None`. The holes need not be concentric (each is classified by its own
/// circle geometry downstream). Spec `m8_holed_disc_coplanar_overlay` §1.
pub(crate) fn annular_disc_face(brep: &BRep, fi: usize) -> Option<(u32, Vec<u32>)> {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) || f.inner_loops.is_empty() {
        return None;
    }
    let is_full_circle = |loop_edges: &[u32]| -> Option<u32> {
        if loop_edges.len() != 1 {
            return None;
        }
        let e = loop_edges[0];
        let edge = &brep.edges()[e as usize];
        matches!(edge.curve, Curve::Circle { .. } if edge.start == edge.end).then_some(e)
    };
    let outer = is_full_circle(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(is_full_circle(lp)?);
    }
    Some((outer, holes))
}

/// Extract a disc face's rim ring (ordered CCW in the pair `frame`) by
/// re-running Stage 1 on this solid with the current (snapped) `coords` and
/// reading the cap fan's vertices. Returns the ring as ordered 3D points.
///
/// Pulling the ring from Stage 1's OWN output (rather than re-deriving it)
/// makes the disc mesh bit-identical to the cap/lateral tessellation
/// `build_stage0_mesh` produces for every non-overridden face — the
/// conformality the §4.5.5 shared-mesh guarantee rests on.
pub(crate) fn disc_rim_ring(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<Vec<Point3>> {
    let circle_e = disc_circle_edge(brep, fi)?;
    let Curve::Circle { center, .. } = brep.edges()[circle_e as usize].curve else {
        return None;
    };
    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let range = tess.face_tri_ranges.get(fi)?.clone();

    // Unique vertices of the cap fan = the rim ring + the one center Steiner
    // vertex. Drop the vertex nearest the circle centre; the rest are the rim.
    let c = center.as_array();
    let mut seen = std::collections::BTreeSet::new();
    let mut rim: Vec<Point3> = Vec::new();
    for tri in &tess.tris[range] {
        for &v in tri {
            if seen.insert(v) {
                let p = tess.verts[v as usize];
                let pa = p.as_array();
                let dr = ((pa[0] - c[0]).powi(2) + (pa[1] - c[1]).powi(2) + (pa[2] - c[2]).powi(2))
                    .sqrt();
                rim.push(p);
                let _ = dr;
            }
        }
    }
    if rim.len() < 4 {
        return None;
    }
    // Identify and drop the centre vertex (strictly closest to `center`).
    let center_idx = (0..rim.len()).min_by(|&i, &j| {
        let di = dist2(rim[i].as_array(), c);
        let dj = dist2(rim[j].as_array(), c);
        di.partial_cmp(&dj).unwrap()
    })?;
    rim.remove(center_idx);
    if rim.len() < 3 {
        return None;
    }
    // Order CCW by the in-frame angle about the circle centre.
    rim.sort_by(|p, q| {
        let ang = |x: &Point3| {
            let (u, v) = frame.project(*x);
            let (cu, cv) = frame.project(Point3::new(c[0], c[1], c[2]));
            (v - cv).atan2(u - cu)
        };
        ang(p).partial_cmp(&ang(q)).unwrap()
    });
    Some(rim)
}

pub(crate) fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Extract the outer rim ring AND each hole rim ring of an ANNULAR disc face
/// (spec `m8_holed_disc_coplanar_overlay`). Like [`disc_rim_ring`], pulls the
/// rings from Stage 1's OWN output so the overlay mesh is bit-identical to the
/// cap/lateral tessellation (§4.5.5 conformality). The planar-curved CDT emits
/// NO interior Steiner points, so every unique face-triangle vertex lies on the
/// outer circle or one hole circle; each vertex is classified to the ring whose
/// circle it lies on (`||p − centerᵢ| − rᵢ|` minimal — robust for off-centre
/// holes). Outer ring ordered CCW, holes ordered CW (opposite sense) in the
/// pair `frame`. Returns `(outer_ring, [hole_rings])`.
pub(crate) fn annular_rim_rings(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(Vec<Point3>, Vec<Vec<Point3>>)> {
    let (outer_e, hole_es) = annular_disc_face(brep, fi)?;
    let circle_geo = |e: u32| -> Option<([f64; 3], f64)> {
        match brep.edges()[e as usize].curve {
            Curve::Circle { center, radius, .. } => Some((center.as_array(), radius)),
            _ => None,
        }
    };
    let (oc, or) = circle_geo(outer_e)?;
    let mut holes_geo: Vec<([f64; 3], f64)> = Vec::with_capacity(hole_es.len());
    for &e in &hole_es {
        holes_geo.push(circle_geo(e)?);
    }

    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let range = tess.face_tri_ranges.get(fi)?.clone();

    // Unique face-triangle vertices (all on a rim — no Steiner center here).
    let mut seen = std::collections::BTreeSet::new();
    let mut pts: Vec<Point3> = Vec::new();
    for tri in &tess.tris[range] {
        for &v in tri {
            if seen.insert(v) {
                pts.push(tess.verts[v as usize]);
            }
        }
    }

    // In-frame radial residual of a point against a circle (center, r).
    let residual = |p: &Point3, center: &[f64; 3], r: f64| -> f64 {
        let (pu, pv) = frame.project(*p);
        let (cu, cv) = frame.project(Point3::new(center[0], center[1], center[2]));
        (((pu - cu).powi(2) + (pv - cv).powi(2)).sqrt() - r).abs()
    };

    // Classify each vertex to the ring (outer=0, hole k → k+1) it lies on.
    let mut outer: Vec<Point3> = Vec::new();
    let mut holes: Vec<Vec<Point3>> = vec![Vec::new(); holes_geo.len()];
    for p in &pts {
        let mut best = (residual(p, &oc, or), 0usize);
        for (k, (hc, hr)) in holes_geo.iter().enumerate() {
            let d = residual(p, hc, *hr);
            if d < best.0 {
                best = (d, k + 1);
            }
        }
        if best.1 == 0 {
            outer.push(*p);
        } else {
            holes[best.1 - 1].push(*p);
        }
    }
    if outer.len() < 3 || holes.iter().any(|h| h.len() < 3) {
        return None;
    }

    // Order a ring by in-frame angle about `center`; `ccw` selects the sense.
    let order = |ring: &mut Vec<Point3>, center: &[f64; 3], ccw: bool| {
        let (cu, cv) = frame.project(Point3::new(center[0], center[1], center[2]));
        ring.sort_by(|p, q| {
            let ang = |x: &Point3| {
                let (u, v) = frame.project(*x);
                (v - cv).atan2(u - cu)
            };
            let (ap, aq) = (ang(p), ang(q));
            if ccw {
                ap.partial_cmp(&aq).unwrap()
            } else {
                aq.partial_cmp(&ap).unwrap()
            }
        });
    };
    order(&mut outer, &oc, true);
    for (k, h) in holes.iter_mut().enumerate() {
        order(h, &holes_geo[k].0, false);
    }
    Some((outer, holes))
}

/// Diagnostic-only: histogram of a face's loop-edge curve types + structure,
/// for the M8 residue survey (`YANG_COPLANAR_PROBE`). Not on any hot path.
pub(crate) fn face_curve_histogram(brep: &BRep, fi: usize) -> String {
    let f = &brep.faces()[fi];
    let mut seg = 0;
    let mut circle = 0;
    let mut ellipse = 0;
    let mut other = 0;
    for &e in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        match brep.edges()[e as usize].curve {
            Curve::LineSegment => seg += 1,
            Curve::Circle { .. } => circle += 1,
            Curve::Ellipse { .. } => ellipse += 1,
            _ => other += 1,
        }
    }
    let surf = match f.surface {
        Surface::Plane { .. } => "plane",
        _ => "nonplane",
    };
    format!(
        "surf={surf} outer={} holes={} seg={seg} circle={circle} ellipse={ellipse} other={other}",
        f.outer_loop.len(),
        f.inner_loops.len(),
    )
}

/// All loop vertex indices of a face (outer + holes), deduped.
pub(crate) fn face_loop_verts(brep: &BRep, fi: usize) -> Vec<u32> {
    let f = &brep.faces()[fi];
    let mut out: Vec<u32> = std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
        .flat_map(|&e| {
            let edge = &brep.edges()[e as usize];
            [edge.start, edge.end]
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Ordered vertex ring of one loop, taking each edge's `start` (the loop
/// continuity convention the Stage-1 fan path uses). `None` if the loop is
/// not continuous (`edges[loop[i]].end != edges[loop[i+1]].start`).
pub(crate) fn loop_vertex_ring(edges: &[BRepEdge], lp: &[u32]) -> Option<Vec<u32>> {
    let n = lp.len();
    if n < 3 {
        return None;
    }
    for i in 0..n {
        let e = &edges[lp[i] as usize];
        let next = &edges[lp[(i + 1) % n] as usize];
        if e.end != next.start {
            return None;
        }
    }
    Some(lp.iter().map(|&e| edges[e as usize].start).collect())
}

/// The face as a [`PolygonWithHoles`] in the pair frame, plus the exact
/// (u,v) → vertex-index map of its loop corners (for overlay-vertex
/// resolution). `None` on a non-continuous loop or non-finite coordinates.
pub(crate) fn face_polygon_2d(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(PolygonWithHoles, BTreeMap<ExactPoint2, u32>)> {
    let f = &brep.faces()[fi];
    let mut corners: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut project_ring = |lp: &[u32]| -> Option<Vec<Point2>> {
        let ring = loop_vertex_ring(brep.edges(), lp)?;
        let mut out = Vec::with_capacity(ring.len());
        for vi in ring {
            let (u, v) = frame.project(coords[vi as usize]);
            corners.insert(ExactPoint2::from_f64(u, v)?, vi);
            out.push(Point2::new(u, v));
        }
        Some(out)
    };
    let outer = project_ring(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(project_ring(lp)?);
    }
    Some((PolygonWithHoles { outer, holes }, corners))
}

/// Does any overlay vertex lie STRICTLY interior to one of `poly`'s outer
/// sub-chords (a rim edge)? True ⇒ the overlap boundary crosses the rim, so the
/// rim is subdivided and the cylinder lateral must absorb the split (the
/// crossing increment). Exact (rational), endpoints excluded.
pub(crate) fn rim_subdivided(poly: &PolygonWithHoles, overlay: &ClassifiedOverlay) -> bool {
    let ring = &poly.outer;
    let n = ring.len();
    if n < 2 {
        return false;
    }
    // Exact rim-edge keys (one per sub-chord), to skip overlay verts that ARE
    // rim vertices (endpoints) cheaply.
    for i in 0..n {
        let s = &ring[i];
        let e = &ring[(i + 1) % n];
        let (sx, sy) = (s.x(), s.y());
        let (ex, ey) = (e.x(), e.y());
        let (Some(s2), Some(e2)) = (ExactPoint2::from_f64(sx, sy), ExactPoint2::from_f64(ex, ey))
        else {
            continue;
        };
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        for q in &overlay.exact_verts {
            let wx = &q.x - &s2.x;
            let wy = &q.y - &s2.y;
            // On the sub-chord's supporting line?
            if &dx * &wy - &dy * &wx != RBig::ZERO {
                continue;
            }
            // Strictly interior, away from BOTH endpoints by a margin? A vertex
            // a few ULPs off a rim sample (t≈0 or t≈1) is that sample
            // reconstructed by the overlay — the rim-snap reconciles it, so it
            // is NOT a crossing. A genuine crossing sits macroscopically
            // mid-chord. The 1e-6 margin cleanly separates the two.
            let t = (&dx * &wx + &dy * &wy) / &len2;
            let tf = t.to_f64().value();
            if tf > 1.0e-6 && tf < 1.0 - 1.0e-6 {
                if std::env::var_os("RIM_SUBDIV_PROBE").is_some() {
                    eprintln!(
                        "[rim-subdiv] sub-chord {i} ({sx},{sy})->({ex},{ey}) interior vert ({},{}) t={tf}",
                        q.x.to_f64().value(),
                        q.y.to_f64().value(),
                    );
                }
                return true;
            }
        }
    }
    false
}

/// M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): does any overlay vertex
/// lie strictly interior to a CURVED sub-chord of this mixed face — outer
/// ring or hole rings, segments selected by `masks` (the
/// [`face_polygon_2d_tessellated`] mixed-arm attribution)? Same exact
/// predicate as [`rim_subdivided`] (rational collinearity + interior
/// parameter with the ULP-reconstruction margin), restricted to curved
/// segments: straight-edge subdivision is legitimate `collect_edge_splits`
/// traffic. True triggers [`collect_mixed_crossings`] propagation.
pub(crate) fn curved_chords_subdivided(
    poly: &PolygonWithHoles,
    masks: &[Vec<Option<u32>>],
    overlay: &ClassifiedOverlay,
) -> bool {
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(masks)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        for i in 0..n {
            if mask[i].is_none() {
                continue;
            }
            let s = &ring[i];
            let e = &ring[(i + 1) % n];
            let (Some(s2), Some(e2)) = (
                ExactPoint2::from_f64(s.x(), s.y()),
                ExactPoint2::from_f64(e.x(), e.y()),
            ) else {
                continue;
            };
            let dx = &e2.x - &s2.x;
            let dy = &e2.y - &s2.y;
            let len2 = &dx * &dx + &dy * &dy;
            if len2 == RBig::ZERO {
                continue;
            }
            for q in &overlay.exact_verts {
                let wx = &q.x - &s2.x;
                let wy = &q.y - &s2.y;
                if &dx * &wy - &dy * &wx != RBig::ZERO {
                    continue;
                }
                // Strictly interior with the same margin as `rim_subdivided`:
                // a vertex a few ULPs off a chain sample is that sample
                // reconstructed by the overlay, not a crossing.
                let t = ((&dx * &wx + &dy * &wy) / &len2).to_f64().value();
                if t > 1.0e-6 && t < 1.0 - 1.0e-6 {
                    return true;
                }
            }
        }
    }
    false
}

/// The fold gate's 3D-bit-degeneracy test (the M-B emission-drop class): a
/// triangle whose RESOLVED image carries a bit-duplicate vertex is never
/// emitted, so its 2D state must not drive gate decisions.
pub(crate) fn gate_tri_degenerate(t: &[u32; 3], coords: &[Point3]) -> bool {
    let bits = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let b = [
        bits(coords[t[0] as usize]),
        bits(coords[t[1] as usize]),
        bits(coords[t[2] as usize]),
    ];
    b[0] == b[1] || b[1] == b[2] || b[0] == b[2]
}

/// The fold gate's 2D signed area under the CURRENT resolved coordinates,
/// projected into the pair frame.
pub(crate) fn gate_tri_area(t: &[u32; 3], coords: &[Point3], frame: &Frame) -> f64 {
    let p0 = frame.project(coords[t[0] as usize]);
    let p1 = frame.project(coords[t[1] as usize]);
    let p2 = frame.project(coords[t[2] as usize]);
    (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)
}

/// A triangle is valid under the current resolved coordinates if it winds
/// material-CCW (positive area) or its 3D image is bit-degenerate (the M-B
/// emission-drop class). The single validity contract shared by the
/// amendment-4 flips and the amendment-5 cavity relocation.
pub(crate) fn gate_tri_valid(t: &[u32; 3], coords: &[Point3], frame: &Frame) -> bool {
    gate_tri_degenerate(t, coords) || gate_tri_area(t, coords, frame) > 0.0
}

/// Exact orientation sign of the 2D triple (a, b, c) — rational arithmetic
/// over the raw f64 frame projections (P9: no tolerance). `None` on
/// non-finite input.
pub(crate) fn orient_sign_exact(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Option<i8> {
    use crate::coplanar_overlay::rat;
    let (ax, ay) = (rat(a.0).ok()?, rat(a.1).ok()?);
    let (bx, by) = (rat(b.0).ok()?, rat(b.1).ok()?);
    let (cx, cy) = (rat(c.0).ok()?, rat(c.1).ok()?);
    let det = (&bx - &ax) * (&cy - &ay) - (&by - &ay) * (&cx - &ax);
    Some(match det.cmp(&RBig::ZERO) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
    })
}

/// Like [`face_polygon_2d`], but a flat circular DISC face is tessellated to its
/// Result of [`face_polygon_2d_tessellated`]: the in-frame 2D polygon, a
/// corner→vertex-index key map, a rim-key→3D-point map (empty for line
/// loops), and — for a MIXED Line+Arc face only (spec
/// `m8_mixed_loop_coplanar_overlay`) — per-ring sub-chord edge attribution
/// (`segs[0]` = outer, `segs[1..]` = holes; `segs[r][i] = Some(e)` ⇔ the
/// segment ring[i]→ring[i+1] lies on curved B-Rep edge `e`, `None` ⇔ a
/// straight edge). Empty ⇔ not a mixed face — disc / annular / all-segment
/// faces keep their existing paths.
pub(crate) type TessellatedFacePolygon = (
    PolygonWithHoles,
    BTreeMap<ExactPoint2, u32>,
    BTreeMap<ExactPoint2, Point3>,
    Vec<Vec<Option<u32>>>,
);

/// §2b in-frame coordinate clustering (spec `m8_shared_boundary_identity`
/// C1-C3, I7/I8): snap projected u values (and, independently, v values)
/// that agree within `band` — across ALL the pair's polygons — to the
/// cluster's FIRST-SEEN representative (an original projected value, never
/// an average). The f64 frame projection rounds each vertex independently,
/// so an OBLIQUE solid's intended-frame-vertical edge lands ~1e-16 off
/// vertical even when its world coordinates are consistent; the exact
/// overlay then faithfully builds femto sweep slabs → needle cells →
/// `RoundingCollapse` / femto-twin split points. Clustering makes
/// intended-equal frame coordinates BIT-equal across the pair (§4.5.5
/// identical boundary sampling in the overlay's own domain).
///
/// Deterministic order: `polys` in slice order, each polygon's `outer` then
/// `holes`, vertices in loop order. Clusters are isolated (real features
/// are ≥ MIN_FEATURE_SIZE apart, six orders above the band — the KV10
/// margin), so greedy first-seen matching cannot chain-drift.
///
/// Test-only since §2c: production wires `cluster_frame_coords_rim_aware`
/// directly (pure-polygon pairs pass empty `rim_excluded`). This wrapper is
/// retained as the §2b reference path the C4d guard compares against.
#[cfg(test)]
pub(crate) fn cluster_frame_coords(polys: &mut [&mut PolygonWithHoles], band: f64) {
    // §2b behavior = §2c rim-aware clustering with NO excluded rim coordinates;
    // delegating keeps the two paths byte-identical for pure-polygon pairs (the
    // C4d guard is the arbiter).
    cluster_frame_coords_rim_aware(polys, &[], band);
}

/// §2c rim-aware variant of `cluster_frame_coords`
/// (spec `m8_shared_boundary_identity` C4a–C4d, invariant I9). The cluster
/// DOMAIN is the polygon-chain coordinates only: rim sample coordinates
/// (`rim_excluded`, per polygon) are neither cluster members nor seeds, and a
/// polygon coordinate within `band` of a rim sample only is left UNTOUCHED (no
/// cross-domain welding). This structurally avoids both §2b-reverted failure
/// modes (welding rim samples; snapping polygon corners onto rims). With every
/// `rim_excluded` slice empty it is byte-identical to `cluster_frame_coords`.
pub(crate) fn cluster_frame_coords_rim_aware(
    polys: &mut [&mut PolygonWithHoles],
    rim_excluded: &[&[Point2]],
    band: f64,
) {
    for axis in 0..2 {
        // Rim sample coordinate values on this axis — excluded from the cluster
        // domain (C4b): never members, never seeds. A polygon coord within band
        // of any of these is left untouched (C4c).
        let rim_coords: Vec<f64> = rim_excluded
            .iter()
            .flat_map(|rim| rim.iter())
            .map(|pt| if axis == 0 { pt.x() } else { pt.y() })
            .collect();
        let near_rim = |c: f64| rim_coords.iter().any(|r| (*r - c).abs() <= band);

        let mut reps: Vec<f64> = Vec::new();
        for poly in polys.iter_mut() {
            for lp in std::iter::once(&mut poly.outer).chain(poly.holes.iter_mut()) {
                for q in lp.iter_mut() {
                    let c = if axis == 0 { q.x() } else { q.y() };
                    // C4b/C4c: a coordinate within band of a rim sample is
                    // neither snapped nor a seed — left exactly as-is.
                    if near_rim(c) {
                        continue;
                    }
                    match reps.iter().find(|r| (**r - c).abs() <= band) {
                        Some(&r) => {
                            *q = if axis == 0 {
                                Point2::new(r, q.y())
                            } else {
                                Point2::new(q.x(), r)
                            };
                        }
                        None => reps.push(c),
                    }
                }
            }
        }
    }
}

/// exact Stage-1 rim ring. The third return value maps each rim vertex's exact
/// 2D key to its bit-identical 3D rim point (for overlay-vertex → 3D
/// resolution; the cylinder lateral shares that exact ring, keeping the overlap
/// mesh conformal). Line-loop faces return an empty rim map.
pub(crate) fn face_polygon_2d_tessellated(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<TessellatedFacePolygon> {
    if disc_circle_edge(brep, fi).is_some() {
        let rim = disc_rim_ring(brep, fi, coords, frame)?;
        let mut outer = Vec::with_capacity(rim.len());
        let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        for &pt in &rim {
            let (u, v) = frame.project(pt);
            let ex = ExactPoint2::from_f64(u, v)?;
            rim_map.insert(ex, pt);
            outer.push(Point2::new(u, v));
        }
        return Some((
            PolygonWithHoles {
                outer,
                holes: Vec::new(),
            },
            BTreeMap::new(),
            rim_map,
            Vec::new(),
        ));
    }
    // M8 holed-disc (spec `m8_holed_disc_coplanar_overlay`): an ANNULAR cap —
    // outer + hole rims sampled from Stage 1's own tessellation into a
    // `PolygonWithHoles`, with every rim point registered in `rim_map` so the
    // overlay-vertex → exact 3D rim point resolution is T-junction-free.
    if annular_disc_face(brep, fi).is_some() {
        let (outer_ring, hole_rings) = annular_rim_rings(brep, fi, coords, frame)?;
        let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        let mut project_ring = |ring: &[Point3]| -> Option<Vec<Point2>> {
            let mut out = Vec::with_capacity(ring.len());
            for &pt in ring {
                let (u, v) = frame.project(pt);
                let ex = ExactPoint2::from_f64(u, v)?;
                rim_map.insert(ex, pt);
                out.push(Point2::new(u, v));
            }
            Some(out)
        };
        let outer = project_ring(&outer_ring)?;
        let mut holes = Vec::with_capacity(hole_rings.len());
        for hr in &hole_rings {
            holes.push(project_ring(hr)?);
        }
        return Some((
            PolygonWithHoles { outer, holes },
            BTreeMap::new(),
            rim_map,
            Vec::new(),
        ));
    }
    // M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): a planar face whose
    // loops mix `LineSegment` and `Circle`/`Ellipse` edges (and full-circle
    // loops in non-annular configurations). Splice each loop from Stage 1's
    // OWN per-edge sample chains (§4.5.5 conformality with the adjacent
    // curved laterals): polyline vertices that are B-Rep vertices → `corners`
    // (resolved to the pair's snapped/welded coordinates); chain Steiner
    // samples → `rim_map` (exact 3D points, bit-shared with the laterals).
    // Per-ring masks mark which sub-chords lie on curved edges — the caller's
    // slice-1 gate walls the pair if the overlap boundary subdivides one.
    if mixed_planar_face(brep, fi) {
        return mixed_face_polygon_2d(brep, fi, coords, frame);
    }
    let (poly, corners) = face_polygon_2d(brep, fi, coords, frame)?;
    Some((poly, corners, BTreeMap::new(), Vec::new()))
}

/// Is `fi` a MIXED planar face (spec `m8_mixed_loop_coplanar_overlay` §2):
/// `Surface::Plane`, not a disc, not annular, every loop edge's curve ∈
/// {`LineSegment`, `Circle`}, at least one `Circle`? Ellipse edges stay the
/// `face-unsupported` wall — chord-interior overlay vertices are minted onto
/// the exact CIRCLE ([`RimChordCtx`]); there is no ellipse mint.
pub(crate) fn mixed_planar_face(brep: &BRep, fi: usize) -> bool {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) {
        return false;
    }
    if disc_circle_edge(brep, fi).is_some() || annular_disc_face(brep, fi).is_some() {
        return false;
    }
    let mut any_curved = false;
    for &e in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        match brep.edges()[e as usize].curve {
            Curve::LineSegment => {}
            Curve::Circle { .. } => any_curved = true,
            _ => return false,
        }
    }
    any_curved
}

/// The MIXED-face arm of [`face_polygon_2d_tessellated`]: loop polylines
/// spliced from the face's own Stage-1 tessellation chains.
pub(crate) fn mixed_face_polygon_2d(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<TessellatedFacePolygon> {
    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let n_brep_verts = brep.vertices().len() as u32;
    let f = &brep.faces()[fi];
    let is_curved = |e_idx: u32| matches!(brep.edges()[e_idx as usize].curve, Curve::Circle { .. });

    let mut corners: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
    let mut masks: Vec<Vec<Option<u32>>> = Vec::with_capacity(1 + f.inner_loops.len());
    let mut project_loop = |lp: &[u32]| -> Option<Vec<Point2>> {
        let attributed =
            crate::loop_polyline_attributed(fi, lp, brep.edges(), &tess.chains).ok()?;
        let mut ring = Vec::with_capacity(attributed.len());
        let mut mask = Vec::with_capacity(attributed.len());
        for &(g, e_idx) in &attributed {
            // Chain Steiner samples live in the tessellation pool; B-Rep
            // vertices resolve through the pair's snapped `coords` (identical
            // values — the tessellation ran on those same coordinates).
            let pt = tess.verts.get(g as usize).copied()?;
            let (u, v) = frame.project(pt);
            let ex = ExactPoint2::from_f64(u, v)?;
            if g < n_brep_verts {
                corners.insert(ex, g);
            } else {
                rim_map.insert(ex, pt);
            }
            ring.push(Point2::new(u, v));
            // The segment STARTING at this vertex lies on its emitting edge.
            mask.push(is_curved(e_idx).then_some(e_idx));
        }
        masks.push(mask);
        Some(ring)
    };

    let outer = project_loop(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(project_loop(lp)?);
    }
    Some((PolygonWithHoles { outer, holes }, corners, rim_map, masks))
}

// ════════════════════════════════════════════════════════════════════════
// M8-vertex-canon §2b: in-frame coordinate clustering
// (spec `specs/m8_shared_boundary_identity.md` §2b, FIP Phase 2, RED).
//
// The world-space vertex pass leaves an OBLIQUE pair's PROJECTED frame
// coordinates femto-split (the f64 `(p−o)·e1` rounds independently per
// vertex), so the exact sweep still builds needle cells → `RoundingCollapse`
// (R0076/R0081). A second layer, where the pair's 2D polygons are built
// (`stage0_preprocess`, ~line 336, just before `coplanar_overlay`), clusters
// the projected u (and, independently, v) coordinates of BOTH faces' loop
// vertices to a first-seen representative.
//
// SETTLED SEAM (the implementer matches this; the call site becomes
// `cluster_frame_coords(&mut [&mut poly_a, &mut poly_b], band)` right before
// the `coplanar_overlay` call):
//
//   fn cluster_frame_coords(polys: &mut [&mut PolygonWithHoles], band: f64)
//
// Deterministic order: polys in slice order, each poly's `outer` loop then its
// `holes`, per vertex; the u axis and v axis cluster INDEPENDENTLY; a
// representative is an original projected value (no averaging). These tests do
// NOT compile until that function exists — that IS the RED state.
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod frame_cluster_tests {
    use super::*;

    fn poly(outer: &[(f64, f64)]) -> PolygonWithHoles {
        PolygonWithHoles {
            outer: outer.iter().map(|&(x, y)| Point2::new(x, y)).collect(),
            holes: Vec::new(),
        }
    }

    /// Every coordinate's bits, in loop order (outer then holes) — for
    /// byte-identity comparison.
    fn bits2(p: &PolygonWithHoles) -> Vec<[u64; 2]> {
        std::iter::once(&p.outer)
            .chain(p.holes.iter())
            .flat_map(|lp| lp.iter().map(|pt| [pt.x().to_bits(), pt.y().to_bits()]))
            .collect()
    }

    /// I7 audit oracle: after clustering, NO two coordinates on one axis differ
    /// by a nonzero amount ≤ `band` (twin-free events), across all loops of all
    /// polygons.
    fn assert_no_twin_events(polys: &[&PolygonWithHoles], band: f64) {
        let mut us: Vec<f64> = Vec::new();
        let mut vs: Vec<f64> = Vec::new();
        for p in polys {
            for lp in std::iter::once(&p.outer).chain(p.holes.iter()) {
                for pt in lp {
                    us.push(pt.x());
                    vs.push(pt.y());
                }
            }
        }
        for (axis_name, mut axis) in [("u", us), ("v", vs)] {
            axis.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for w in axis.windows(2) {
                let d = (w[1] - w[0]).abs();
                assert!(
                    d == 0.0 || d > band,
                    "I7: {axis_name}-axis twin event: {} and {} differ by {d:e} ≤ band {band:e}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    /// C1 / I7 / I8 (RED): two projected coords within band (across A and B)
    /// snap to the first-seen representative on the u axis; v untouched; the
    /// far (3.0) cluster untouched (C2); representative is an original member.
    #[test]
    fn red_frame_coords_cluster_to_representative() {
        // u ≈ 1.0 split by 1 and 2 ULPs (~2.2e-16, 4.4e-16) — the measured
        // R0076 femto-crookedness; band 1e-12.
        let u1 = f64::from_bits(1.0f64.to_bits() + 1); // 1.0 + 1 ULP (A)
        let u2 = f64::from_bits(1.0f64.to_bits() + 2); // 1.0 + 2 ULP (B)
        let band = 1e-12;

        let mut a = poly(&[(1.0, 0.0), (u1, 2.0), (3.0, 2.0), (3.0, 0.0)]);
        let mut b = poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]);

        cluster_frame_coords(&mut [&mut a, &mut b], band);

        // C1 / I8: all three near-1.0 u values are BIT-equal to the first-seen
        // representative 1.0 (a member — no averaging).
        let rep = 1.0f64.to_bits();
        assert_eq!(a.outer[0].x().to_bits(), rep, "A[0].u representative");
        assert_eq!(a.outer[1].x().to_bits(), rep, "A[1].u (1 ULP) snaps to rep");
        assert_eq!(b.outer[0].x().to_bits(), rep, "B[0].u (2 ULP) snaps to rep");

        // v is untouched (no femto split on v).
        assert_eq!(a.outer[1].y(), 2.0, "I8: v coordinate not moved");
        // C2: the 3.0 cluster (already exact) stays 3.0.
        assert_eq!(a.outer[2].x(), 3.0, "C2: far cluster untouched");

        // I7: no twin events remain on either axis.
        assert_no_twin_events(&[&a, &b], band);
    }

    /// C3 guard: generic polygons whose coordinates are all exactly equal or
    /// ≫ band apart are byte-identical through the pass.
    #[test]
    fn guard_generic_distinct_polygons_byte_identical() {
        let band = 1e-12;
        let mut a = poly(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)]);
        let mut b = poly(&[(10.0, 10.0), (15.0, 10.0), (12.0, 13.0)]);
        let (ba, bb) = (bits2(&a), bits2(&b));

        cluster_frame_coords(&mut [&mut a, &mut b], band);

        assert_eq!(
            bits2(&a),
            ba,
            "C3: distinct polygon A must be byte-identical"
        );
        assert_eq!(
            bits2(&b),
            bb,
            "C3: distinct polygon B must be byte-identical"
        );
    }

    /// Axis-independence guard: a pair split only in v clusters ONLY in v; the
    /// distinct u values (1.0 vs 2.0, ≫ band apart) are left untouched.
    #[test]
    fn guard_v_axis_clusters_independent_of_u() {
        let band = 1e-12;
        let v_twin = f64::from_bits(7.0f64.to_bits() + 3); // 7.0 + 3 ULP (~2.7e-15)
        let mut a = poly(&[(1.0, 7.0), (2.0, v_twin), (2.0, 9.0), (1.0, 9.0)]);

        cluster_frame_coords(&mut [&mut a], band);

        // v: the femto twin snaps to the first-seen representative 7.0.
        assert_eq!(
            a.outer[0].y().to_bits(),
            7.0f64.to_bits(),
            "v representative"
        );
        assert_eq!(a.outer[1].y().to_bits(), 7.0f64.to_bits(), "v twin snaps");
        // u: distinct values are NOT touched by v clustering.
        assert_eq!(a.outer[0].x(), 1.0, "u untouched (independent axis)");
        assert_eq!(a.outer[1].x(), 2.0, "u untouched (independent axis)");
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on cluster_frame_coords: band boundary, first-seen no-drift,
    // A-first determinism, axis independence, representative-is-member. In-module
    // (pub(crate)). Purely additive; touches no existing test.

    /// Band boundary: a coordinate 0.9·band from the representative clusters;
    /// 1.1·band away does NOT (it becomes a new representative). Pins the `<=`
    /// band edge at realistic scale (not just the 1-2 ULP splits above).
    #[test]
    fn adversary_band_boundary_below_clusters_above_new_rep() {
        let band = 1e-12;
        let a = 5.0f64;
        let near = a + 0.9 * band; // within band → clusters
        let far = a + 1.1 * band; // beyond band → new rep
        let mut p = poly(&[(a, 0.0), (near, 1.0), (far, 2.0), (9.0, 3.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(p.outer[0].x().to_bits(), a.to_bits(), "rep untouched");
        assert_eq!(
            p.outer[1].x().to_bits(),
            a.to_bits(),
            "0.9·band coord must snap to the representative"
        );
        assert_eq!(
            p.outer[2].x().to_bits(),
            far.to_bits(),
            "1.1·band coord must stay (its own new representative)"
        );
    }

    /// No chain drift (first-seen semantics). Values a, a+0.9·band, a+1.8·band:
    /// the rep list is FIRST-SEEN, so a+1.8·band is measured against rep `a`
    /// (1.8·band > band) → it becomes its OWN rep and does NOT get pulled into
    /// a's cluster even though it is only 0.9·band from the (snapped) middle
    /// value. This is the isolation property that makes greedy clustering safe.
    #[test]
    fn adversary_first_seen_prevents_chain_drift() {
        let band = 1e-12;
        let a = 5.0f64;
        let mid = a + 0.9 * band;
        let outer = a + 1.8 * band;
        let mut p = poly(&[(a, 0.0), (mid, 1.0), (outer, 2.0), (9.0, 3.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(p.outer[1].x().to_bits(), a.to_bits(), "mid snaps to rep a");
        assert_eq!(
            p.outer[2].x().to_bits(),
            outer.to_bits(),
            "no drift: the far value stays its own rep (measured against a, not mid)"
        );
        assert_no_twin_events(&[&p], band);
    }

    /// A-first determinism: the representative is the FIRST-SEEN value in slice
    /// order (A's loop before B's). Two within-band values a1 (A) and a2 (B) both
    /// resolve to a1 — swapping the slice order would pick a2, so this pins the
    /// documented deterministic ordering.
    #[test]
    fn adversary_a_first_representative_determinism() {
        let band = 1e-12;
        let a1 = 5.0f64;
        let a2 = a1 + 0.5 * band;
        let mut a = poly(&[(a1, 0.0), (8.0, 0.0), (8.0, 2.0)]);
        let mut b = poly(&[(a2, 5.0), (8.0, 5.0), (8.0, 4.0)]);
        cluster_frame_coords(&mut [&mut a, &mut b], band);
        assert_eq!(
            a.outer[0].x().to_bits(),
            a1.to_bits(),
            "A's value is the rep"
        );
        assert_eq!(
            b.outer[0].x().to_bits(),
            a1.to_bits(),
            "B's within-band value adopts A's first-seen representative"
        );
    }

    /// MUTATION KILLER (b) — axes must cluster INDEPENDENTLY. A vertex whose v
    /// coordinate (5.0 + 1 ULP) is within band of a DIFFERENT vertex's u
    /// coordinate (5.0) must NOT cross-snap: production keeps the u and v
    /// representative lists separate (fresh per axis), so v = 5.0+ULP stays. A
    /// SHARED rep list (axes coupled) would pull v onto the u-derived rep 5.0.
    ///
    /// Verified: production → v stays 5.0+ULP; shared-rep-list mutant → v snaps
    /// to 5.0. The existing axis-independence guard does NOT catch this (its v
    /// values are far from any u value); this is the dedicated killer.
    #[test]
    fn adversary_axes_independent_v_near_u_no_cross_snap() {
        let band = 1e-12;
        let v_near_u = f64::from_bits(5.0f64.to_bits() + 1); // 5.0 + 1 ULP (~8.9e-16 < band)
                                                             // u values: 8.0, 8.0, 5.0, 5.0 ; v values: 8.0, v_near_u, 9.0, 9.0.
                                                             // v_near_u is a lone v value (no other v within band) → must stay.
        let mut p = poly(&[(8.0, 8.0), (8.0, v_near_u), (5.0, 9.0), (5.0, 8.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(
            p.outer[1].y().to_bits(),
            v_near_u.to_bits(),
            "axis independence: a v value near a u value must NOT snap to the u rep"
        );
        // u values are exact and unchanged.
        assert_eq!(p.outer[0].x().to_bits(), 8.0f64.to_bits());
        assert_eq!(p.outer[2].x().to_bits(), 5.0f64.to_bits());
    }

    /// MUTATION KILLER (a) — the representative is an EXACT MEMBER (I8), never an
    /// average. A femto cluster {a, a+1 ULP, a+2 ULP} collapses so every output
    /// coordinate is bit-equal to ONE of the original inputs (here the first-seen
    /// `a`). An averaging representative would emit a value equal to none of the
    /// three inputs.
    #[test]
    fn adversary_representative_is_exact_member_not_average() {
        let band = 1e-12;
        let a = 5.0f64;
        let a1 = f64::from_bits(a.to_bits() + 1);
        let a2 = f64::from_bits(a.to_bits() + 2);
        let inputs: std::collections::BTreeSet<u64> =
            [a, a1, a2].iter().map(|x| x.to_bits()).collect();
        let mut p = poly(&[(a, 0.0), (a1, 1.0), (a2, 2.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        for (i, q) in p.outer.iter().enumerate() {
            assert!(
                inputs.contains(&q.x().to_bits()),
                "I8: clustered u[{i}]={} is not an original member (averaging?)",
                q.x()
            );
        }
        // And specifically the first-seen member.
        assert_eq!(
            p.outer[0].x().to_bits(),
            a.to_bits(),
            "first-seen member is the rep"
        );
        assert_eq!(p.outer[1].x().to_bits(), a.to_bits());
        assert_eq!(p.outer[2].x().to_bits(), a.to_bits());
    }

    // ════════════════════════════════════════════════════════════════════
    // M8-vertex-canon §2c: RIM-AWARE in-frame clustering
    // (spec `specs/m8_shared_boundary_identity.md` §2c, FIP Phase 2, RED).
    //
    // §2b's clustering is scope-limited to PURE-POLYGON pairs (the call site's
    // `cluster_ok = rim_a.is_empty() && rim_b.is_empty()` gate). §2c lifts that:
    // apply the SAME per-axis band clustering to RIM-CARRYING pairs, but restrict
    // the cluster DOMAIN to POLYGON-CHAIN coordinates and EXCLUDE rim sample
    // coordinates entirely — neither cluster members nor seeds (C4a–C4d, I9).
    // This structurally avoids both P10-reverted failure modes (no rim welding;
    // no snapping polygon corners onto rims).
    //
    // SETTLED SEAM (the implementer provides this; these tests do NOT compile
    // until it exists — that IS the RED state, per the §2b precedent above):
    //
    //   fn cluster_frame_coords_rim_aware(
    //       polys: &mut [&mut PolygonWithHoles],
    //       rim_excluded: &[&[Point2]],   // per-poly rim sample coords (u,v),
    //                                     // excluded from the cluster domain
    //       band: f64,
    //   )
    //
    // Contract: cluster the polygons' non-rim coordinates exactly as
    // `cluster_frame_coords` does (per-axis, first-seen representative, A's loop
    // first); a coordinate bit-equal to a rim sample is NEITHER a member nor a
    // seed, and a polygon coordinate within band of ONLY a rim sample is left
    // untouched (C4c). With every `rim_excluded` slice empty the function is
    // byte-identical to `cluster_frame_coords` (C4d).
    // ════════════════════════════════════════════════════════════════════

    fn pts(coords: &[(f64, f64)]) -> Vec<Point2> {
        coords.iter().map(|&(x, y)| Point2::new(x, y)).collect()
    }

    /// I9 / C4a / C4b / C4c (RED): a rim-carrying pair. Both polygon chains carry
    /// an intended-equal frame coordinate split ~1e-16 (must weld — C4a/I9); a
    /// polygon coordinate sits femto-near a RIM sample only (must NOT weld onto it
    /// — C4c); rim samples stay byte-identical (C4b). RED today because the
    /// rim-aware seam does not exist (production skips clustering entirely for
    /// rim-carrying pairs, so the twins would never weld).
    #[test]
    fn red_rim_carrying_clusters_polygon_excludes_rim() {
        let band = 1e-12;
        // Intended-equal chain twins across A and B (1 ULP / 2 ULP off 1.0) — the
        // measured femto-crookedness; both must collapse to the first-seen rep.
        let u1 = f64::from_bits(1.0f64.to_bits() + 1); // A
        let u2 = f64::from_bits(1.0f64.to_bits() + 2); // B
                                                       // A rim sample at u = 4.0, and a POLYGON coord 1 ULP from it (C4c).
        let rho = 4.0f64;
        let near_rim = f64::from_bits(rho.to_bits() + 1);

        let mut a = poly(&[(1.0, 0.0), (u1, 2.0), (near_rim, 2.0), (3.0, 0.0)]);
        let mut b = poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]);

        // A is the rim-carrying face (a disc-cap ring): its rim samples include
        // rho on the u axis. B carries no rim.
        let rim_a = pts(&[(rho, 2.0), (rho, 0.0)]);
        let rim_b: Vec<Point2> = Vec::new();

        cluster_frame_coords_rim_aware(
            &mut [&mut a, &mut b],
            &[rim_a.as_slice(), rim_b.as_slice()],
            band,
        );

        // C4a / I9: the intended-equal chain twins are BIT-equal to the first-seen
        // representative (1.0, A's loop first).
        let rep = 1.0f64.to_bits();
        assert_eq!(a.outer[0].x().to_bits(), rep, "A[0].u representative");
        assert_eq!(a.outer[1].x().to_bits(), rep, "A[1].u (1 ULP) snaps to rep");
        assert_eq!(b.outer[0].x().to_bits(), rep, "B[0].u (2 ULP) snaps to rep");

        // C4c: the polygon coord femto-near a RIM sample only is UNTOUCHED
        // (rim excluded from the domain — no cross-domain welding).
        assert_eq!(
            a.outer[2].x().to_bits(),
            near_rim.to_bits(),
            "C4c: polygon coord within band of a rim sample only must not weld onto it"
        );

        // C4b: rim samples are byte-identical to their input (never members/seeds;
        // the pass must never mutate them).
        assert_eq!(rim_a[0].x().to_bits(), rho.to_bits(), "C4b: rim sample u");
        assert_eq!(
            rim_a[0].y().to_bits(),
            2.0f64.to_bits(),
            "C4b: rim sample v"
        );

        // I9 audit: no twin events remain among the POLYGON coordinates on either
        // axis (the near_rim coord is isolated from other polygon coords, so it is
        // not itself a twin event).
        assert_no_twin_events(&[&a, &b], band);
    }

    /// C4d guard: a PURE-polygon pair (all `rim_excluded` slices empty) through
    /// the rim-aware path is byte-identical to the §2b `cluster_frame_coords`
    /// behavior — no behavior change for the pure-polygon population that §2b
    /// already serves. GREEN once the seam lands (protects §2b byte-identity);
    /// compile-gated with the RED above until then.
    #[test]
    fn guard_c4d_pure_polygon_pair_matches_2b_behavior() {
        let band = 1e-12;
        let u1 = f64::from_bits(1.0f64.to_bits() + 1);
        let u2 = f64::from_bits(1.0f64.to_bits() + 2);
        let mk = || {
            (
                poly(&[(1.0, 0.0), (u1, 2.0), (3.0, 2.0), (3.0, 0.0)]),
                poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]),
            )
        };

        // §2b reference path.
        let (mut ra, mut rb) = mk();
        cluster_frame_coords(&mut [&mut ra, &mut rb], band);

        // Rim-aware path with empty rim exclusion (C4d).
        let (mut ca, mut cb) = mk();
        cluster_frame_coords_rim_aware(&mut [&mut ca, &mut cb], &[&[], &[]], band);

        assert_eq!(bits2(&ca), bits2(&ra), "C4d: A byte-identical to §2b path");
        assert_eq!(bits2(&cb), bits2(&rb), "C4d: B byte-identical to §2b path");
    }
}

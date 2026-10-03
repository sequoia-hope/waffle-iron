//! Q5 of `specs/agent_mechanical_design.md` §4.2: **wall thickness by
//! sampling**, with the honesty that implies.
//!
//! ## What it does
//!
//! For every face of the solid, lay out sample SITES on it, cast a ray from
//! each one along the INWARD normal, and take the first face it hits. The
//! distance is the wall there. The minimum over the sites, their mean, their
//! spread and the thinnest site itself are the answer.
//!
//! ## Where the sites come from, and why they are facet centroids
//!
//! The sites are derived from the render tessellation — the same triangles the
//! app draws and [`super::distance`] seeds on — subdivided barycentrically
//! until no sub-facet edge exceeds the requested spacing. Each site is a
//! sub-facet's CENTROID, projected onto the face's own analytic surface
//! ([`closest_point_on`], an exact projection for every arena surface) so the
//! ray starts on the real geometry rather than on a chord of it.
//!
//! §4.2's sketch says "every vertex of the render tessellation, and the
//! centroid of every triangle above a size threshold". Vertices are NOT used,
//! and the subdivision replaces the threshold:
//!
//! - A tessellation vertex sits on a face's rim, or on the shared edge of two
//!   facets, where the inward normal is not the face's alone and the cast is
//!   either degenerate or grazing. A sub-facet centroid is strictly inside one
//!   facet of the face's own CDT, so the face's inward normal at it is
//!   unambiguous.
//! - A vertex-only sampler also misses the middle of a large facet entirely,
//!   which is exactly where a plate's thin spot would be. Subdividing to the
//!   spacing covers each face uniformly, which is what makes `spacing` a
//!   number a rule can reason with.
//!
//! ## What is exact and what is not
//!
//! Each individual cast is as exact as the geometry allows. The site is on the
//! analytic surface, and the facet hit is refined by a one-dimensional Newton
//! along the ray on the hit surface's own signed distance
//! ([`signed_offset_at`]) and then CERTIFIED — the refined point must satisfy
//! the surface to the working tolerance and still lie on the hit face's trim.
//! So a plate reports its thickness to rounding, and a tube reports
//! `r_outer − r_inner` to rounding rather than to the chord band (which would
//! be the sagitta-deficient chord-to-chord distance).
//!
//! What is sampled is the SET of sites. A wall thinner than `spacing` between
//! two of them is never looked at, so the reported minimum is an UPPER bound
//! on the body's true minimum wall — which is why
//! `waffle_types::kernel::ThicknessMethod` has one arm and it is `Sampled`. An
//! exact medial axis is not in scope and this result never claims to be one.
//!
//! ## The corner, and why there are two minima
//!
//! Two faces that meet at an edge enclose a wedge of material that goes to
//! zero AT the edge. A cast between them therefore measures how close its site
//! got to that edge, not how thick the body is — and if the dihedral is acute,
//! it measures arbitrarily little. This is not a fixture's problem: a 4 mm
//! radial slot through a 10/7 mm tube meets its outer cylinder at 78°, and
//! [`ThicknessResult::min`] there is 0.043 mm against a 3 mm wall.
//!
//! `min` is still reported, because it is the answer to the question §4.2
//! poses. Beside it, [`ThicknessResult::min_wall`] is the same minimum over
//! only the sites whose two faces do NOT share an edge
//! ([`Site::faces_share_an_edge`]) — the thinnest WALL, which is what a
//! wall-thickness rule is asking for. On the slotted tube that is 3.000 mm; on
//! a plate the two numbers are equal, because a plate has no corner reading to
//! leave out.
//!
//! The exclusion is deliberately coarse: it drops EVERY reading between two
//! faces that meet anywhere, so a tapered rib whose two flanks meet at a tip
//! edge does not contribute its own thickness to `min_wall` either. That is
//! the conservative direction for a rule — a wall it cannot see is not a wall
//! it reports as thick — and `min` plus the thinnest site's own faces are
//! there for a caller that wants to judge such a rib itself.
//!
//! ## The self-hit, and why its band is LOCAL
//!
//! A site on a convex curved face sits OUTSIDE its own chord facets (a chord
//! lies inside the arc it subtends), so the inward ray crosses its own facet
//! within the local sagitta of the snap. That crossing is not a wall. It is
//! rejected by a band around the ray's own start — but only for a hit on the
//! SITE'S OWN face, and only out to a few times the snap distance `|p − c|`,
//! which IS that local sagitta.
//!
//! **Where the factor 4 comes from.** [`closest_point_on`] moves the facet
//! centroid `c` to the surface along the surface normal at the result, and the
//! ray leaves along the NEGATED normal at that same point — so the ray passes
//! back through `c` at `t = |p − c|` exactly, and the facet `c` came from is
//! crossed there and nowhere else (a plane is met once). The band therefore
//! only has to cover `1 ×` the snap; 4 is the margin for the surfaces where
//! the projection direction and the normal are not exactly antiparallel after
//! rounding (a torus or sphere pole fan), plus `8 ε · scale` for a site the
//! projection did not move at all. Nothing beyond the band is skipped, which
//! is why the far side of the same face stays measurable.
//!
//! A global band would have been wrong in both directions: on a 1 mm plate
//! 100 mm across, the body's own chord band is larger than the wall being
//! measured and every site would have been thrown away; and on a face sampled
//! far from the body's extent it would be far too loose. A site whose wall
//! really is under its own local sagitta is COUNTED in
//! [`ThicknessResult::declines`] rather than reported — a cast seeded on a
//! render tessellation cannot resolve it, and saying nothing would be worse.
//! The far side of the SAME face stays measurable: a solid cylinder's diameter
//! is orders of magnitude past the band.

use std::collections::{HashMap, HashSet};

use cad_primitives::{Point3, TAU_WORK};

use super::{add_scaled, cross, dist2, dot, pt_tri, sub, Bvh, Node, Prim, Target};
use crate::arena::{BrepArena, FaceId, SolidId, Surface};
use crate::error::KernelV2Error;
use crate::signature::{closest_point_on, outward_normal_at, signed_offset_at};

/// Sub-facets per facet edge are capped at this, so one enormous facet cannot
/// turn a query into a day's work. The ACHIEVED spacing is measured and
/// reported, so a request the cap could not meet comes back as a looser
/// spacing rather than as a false one.
const MAX_SUBDIVISION: usize = 32;

/// How many sites across the body's bounding diagonal the default spacing asks
/// for.
///
/// 32 — a few thousand sites on a plate, which runs in milliseconds, and dense
/// enough to find a thin spot a human would call a feature. It is a DEFAULT,
/// not a floor: `spacing` asks for more, and the answer reports what it used
/// either way.
const DEFAULT_SITES_ACROSS: f64 = 32.0;

/// Equal-width bins over `[min, max]`.
const HISTOGRAM_BINS: usize = 10;

/// One measured site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Site {
    pub thickness: f64,
    pub point: Point3,
    pub opposite: Point3,
    pub from: FaceId,
    pub to: FaceId,
    /// Whether [`Self::from`] and [`Self::to`] are two DISTINCT faces that
    /// share an edge — so this reading crossed a corner rather than a wall.
    ///
    /// Two faces that meet at an edge enclose a wedge of material that goes to
    /// zero at the edge, so a cast between them measures how close the site is
    /// to that edge and not how thick the body is (see the module docs). A
    /// site that hits its OWN face is NOT such a reading: a solid cylinder's
    /// lateral face measures its diameter across itself, which is a wall.
    pub faces_share_an_edge: bool,
}

/// Sites that produced nothing, by reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Declines {
    pub no_hit: usize,
    pub below_self_band: usize,
    pub no_surface: usize,
}

/// One bar of the histogram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bin {
    pub lo: f64,
    pub hi: f64,
    pub count: usize,
}

/// The answer (Q5).
#[derive(Debug, Clone, PartialEq)]
pub struct ThicknessResult {
    pub min: f64,
    /// The minimum over sites whose two faces do NOT share an edge — the
    /// thinnest WALL, with every corner reading left out. `None` when every
    /// site crossed a corner (a body made only of faces that all meet, a
    /// tetrahedron the sampler found no wall in).
    ///
    /// `min` answers §4.2's question exactly as it is posed and is dominated
    /// by any acute edge; this one answers "how thick is the material here",
    /// which is what a wall-thickness rule asks. Both are reported because
    /// neither is the other's approximation.
    pub min_wall: Option<f64>,
    pub mean: f64,
    pub max: f64,
    pub thinnest: Site,
    /// The site [`Self::min_wall`] was measured at.
    pub thinnest_wall: Option<Site>,
    pub histogram: Vec<Bin>,
    /// Sites that produced a thickness.
    pub samples: usize,
    /// The largest sub-facet edge among the sites — the gap between
    /// neighbouring sites on one face, MEASURED rather than echoed back from
    /// the request.
    pub spacing: f64,
    pub chord_bound: f64,
    /// Sites whose hit was refined onto the analytic surface and certified.
    pub refined: usize,
    pub declines: Declines,
}

/// The solid's triangles, indexed by face, so a containment test costs a scan
/// of one face's facets rather than a re-tessellation of it.
///
/// `super::on_trimmed_face` tessellates the face on every call, which is the
/// right trade for Q1 (a handful of candidate pairs) and the wrong one here
/// (one call per site, thousands of them, plus one per refined hit).
struct Facets<'a> {
    prims: &'a [Prim],
    ranges: HashMap<FaceId, (usize, usize)>,
}

impl<'a> Facets<'a> {
    /// `primitives` emits one face's triangles before moving to the next, so
    /// each face owns a contiguous run.
    fn index(prims: &'a [Prim]) -> Facets<'a> {
        let mut ranges: HashMap<FaceId, (usize, usize)> = HashMap::new();
        for (i, prim) in prims.iter().enumerate() {
            if let Prim::Tri { face, .. } = prim {
                let e = ranges.entry(*face).or_insert((i, 0));
                e.1 += 1;
            }
        }
        Facets { prims, ranges }
    }

    /// Whether `p` lies on `face`'s trimmed surface, as far as the face's own
    /// triangles can say: within `band` of them. The same statement — and the
    /// same tolerance argument — as `super::on_trimmed_face`.
    fn contains(&self, face: FaceId, p: Point3, band: f64) -> bool {
        let Some(&(start, count)) = self.ranges.get(&face) else {
            return false;
        };
        let band2 = band * band;
        self.prims[start..start + count]
            .iter()
            .any(|prim| match prim {
                Prim::Tri { p: tri, .. } => pt_tri(p, tri).0 <= band2,
                _ => false,
            })
    }
}

/// Every pair of DISTINCT faces of `solid` that share an edge, each pair
/// stored once with the smaller id first.
///
/// Built in one pass over the solid's loops — a half-edge and its twin are the
/// two sides of one edge, so their faces are neighbours — and read once per
/// site, which is what keeps the corner test O(1) on a body with a million
/// sites. A face paired with ITSELF is deliberately not stored: a cylinder's
/// lateral face meets itself at its seam, and a cast across its diameter is a
/// wall, not a corner.
fn edge_adjacent_faces(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<HashSet<(FaceId, FaceId)>, KernelV2Error> {
    let mut out = HashSet::new();
    for &sh in &arena.solid(solid)?.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let loops = std::iter::once(face.outer_loop).chain(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    let twin = arena.half_edge(h)?.twin;
                    let other = arena.loop_(arena.half_edge(twin)?.loop_id)?.face;
                    if other != f {
                        out.insert((f.min(other), f.max(other)));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The default sample spacing: the body's bounding diagonal over
/// [`DEFAULT_SITES_ACROSS`].
fn default_spacing(bvh: &Bvh) -> f64 {
    let n = &bvh.nodes[0];
    ((n.hi[0] - n.lo[0]).powi(2) + (n.hi[1] - n.lo[1]).powi(2) + (n.hi[2] - n.lo[2]).powi(2)).sqrt()
        / DEFAULT_SITES_ACROSS
}

/// `max(1, |p|∞)` — the scale a tolerance is relative to at `p`.
fn scale_at(p: Point3) -> f64 {
    p.as_array()
        .iter()
        .fold(1.0f64, |m: f64, v: &f64| m.max(v.abs()))
}

/// Ray-AABB: the entry parameter, or `None` when the ray misses the box or
/// only meets it behind the origin. A slab test; a zero component of `dir`
/// gives infinities that compare correctly.
fn ray_box(node: &Node, origin: Point3, inv_dir: [f64; 3]) -> Option<f64> {
    let o = origin.as_array();
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for k in 0..3 {
        let a = (node.lo[k] - o[k]) * inv_dir[k];
        let b = (node.hi[k] - o[k]) * inv_dir[k];
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t1 >= t0.max(0.0)).then(|| t0.max(0.0))
}

/// Möller–Trumbore, both senses: the ray starts INSIDE the material, so a hit
/// lands on the back of its triangle as often as on the front and the sign of
/// the determinant says nothing about whether it is a wall.
fn ray_tri(origin: Point3, dir: [f64; 3], tri: &[Point3; 3]) -> Option<f64> {
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    // `det = −dir·n_tri`, so this rejects a ray running IN the triangle's own
    // plane (and a degenerate triangle): there is no crossing to report. It is
    // NOT what keeps a planar face's own facets out of the way — a face's
    // inward normal is parallel to `n_tri` there, which is where `det` is
    // largest. What keeps those out is `t > 0.0` together with the self band:
    // the ray starts ON the facet, so its own crossing is at `t = 0` to
    // rounding (see `first_hit`).
    if det == 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(origin, tri[0]);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t.is_finite() && t > 0.0).then_some(t)
}

/// What one cast found.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cast {
    Hit(f64, FaceId),
    /// The only crossing was the site's own face inside the self band — a wall
    /// a render-density cast cannot resolve, not the absence of one.
    BelowSelfBand,
    /// The ray left the body without crossing anything.
    Miss,
}

/// The first face the ray hits, skipping a hit on `own` closer than
/// `self_band` (see the module docs).
fn first_hit(bvh: &Bvh, origin: Point3, dir: [f64; 3], own: FaceId, self_band: f64) -> Cast {
    let inv_dir = [0, 1, 2].map(|k| 1.0 / dir[k]);
    let mut best: Option<(f64, FaceId)> = None;
    let mut skipped_own = false;
    // Depth-first with an explicit stack: the tree is a median split and can
    // be deep, and a query must not recurse on the WASM stack.
    let mut stack = vec![0usize];
    while let Some(i) = stack.pop() {
        let node = &bvh.nodes[i];
        let Some(enter) = ray_box(node, origin, inv_dir) else {
            continue;
        };
        if best.is_some_and(|(t, _)| enter > t) {
            continue;
        }
        match node.children {
            Some((l, r)) => {
                stack.push(l);
                stack.push(r);
            }
            None => {
                for prim in &bvh.prims[node.start..node.start + node.count] {
                    // Only triangles bound material. An edge's chords and a
                    // vertex are measure zero to a ray, and the faces they
                    // bound are already here.
                    let Prim::Tri { p, face, .. } = prim else {
                        continue;
                    };
                    let Some(t) = ray_tri(origin, dir, p) else {
                        continue;
                    };
                    if *face == own && t <= self_band {
                        skipped_own = true;
                        continue;
                    }
                    if best.is_none_or(|(b, _)| t < b) {
                        best = Some((t, *face));
                    }
                }
            }
        }
    }
    match best {
        Some((t, face)) => Cast::Hit(t, face),
        // A skipped own-face crossing with nothing beyond it is not "no hit":
        // the ray left through a band it could not resolve, which is a
        // different fact about the body and is counted separately.
        None if skipped_own => Cast::BelowSelfBand,
        None => Cast::Miss,
    }
}

/// Newton along the ray onto `surface`, then certify: the refined point must
/// satisfy the surface to the working tolerance AND still lie on `face`'s
/// trim. `None` leaves the caller with its facet hit, honestly inexact.
fn refine(
    facets: &Facets,
    face: FaceId,
    surface: &Surface,
    origin: Point3,
    dir: [f64; 3],
    t_seed: f64,
    band: f64,
) -> Option<(f64, Point3)> {
    let mut t = t_seed;
    let mut converged = false;
    for _ in 0..32 {
        let p = add_scaled(origin, dir, t);
        let g = signed_offset_at(surface, p)?;
        let n = outward_normal_at(surface, p)?;
        let slope = dot(dir, n);
        // A grazing ray has no well-conditioned root along it.
        if slope.abs() <= TAU_WORK {
            return None;
        }
        t -= g / slope;
        if !(t.is_finite() && t > 0.0) {
            return None;
        }
        if (g / slope).abs() <= TAU_WORK * scale_at(p) {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    let p = add_scaled(origin, dir, t);
    if signed_offset_at(surface, p)?.abs() > TAU_WORK * scale_at(p) {
        return None;
    }
    if !facets.contains(face, p, band) {
        return None;
    }
    Some((t, p))
}

/// Every site of one facet: the centroids of its `k²` barycentric sub-facets,
/// with `k` chosen so no sub-facet edge exceeds `spacing`.
///
/// Returns the achieved sub-facet edge alongside, so the caller reports the
/// spacing it GOT rather than the one it asked for.
fn facet_sites(tri: &[Point3; 3], spacing: f64, out: &mut Vec<Point3>) -> f64 {
    let (a, b, c) = (tri[0], tri[1], tri[2]);
    let longest = dist2(a, b)
        .sqrt()
        .max(dist2(b, c).sqrt())
        .max(dist2(c, a).sqrt());
    let k = if spacing > 0.0 && longest.is_finite() {
        ((longest / spacing).ceil() as usize).clamp(1, MAX_SUBDIVISION)
    } else {
        1
    };
    let (ab, ac) = (sub(b, a), sub(c, a));
    let kf = k as f64;
    let third = 1.0 / 3.0;
    let mut push = |u: f64, v: f64| {
        out.push(Point3::new(
            a.x() + ab[0] * u + ac[0] * v,
            a.y() + ab[1] * u + ac[1] * v,
            a.z() + ab[2] * u + ac[2] * v,
        ));
    };
    for i in 0..k {
        for j in 0..(k - i) {
            // The "up" sub-facet of lattice cell `(i, j)`, and the "down" one
            // that fills the rhombus beside it. `k = 1` gives exactly the
            // facet's own centroid.
            push((i as f64 + third) / kf, (j as f64 + third) / kf);
            if i + j + 1 < k {
                push((i as f64 + 2.0 * third) / kf, (j as f64 + 2.0 * third) / kf);
            }
        }
    }
    longest / kf
}

/// Sampled wall thickness of `solid` (Q5).
///
/// `spacing` of `None` asks for the default (see [`DEFAULT_SITES_ACROSS`]).
/// `Err` when no site produced a thickness at all: a body whose every cast
/// failed is not a body with no walls, and a `min` of 0 or of infinity for it
/// would be a number nothing measured.
pub fn thickness(
    arena: &BrepArena,
    solid: SolidId,
    spacing: Option<f64>,
) -> Result<ThicknessResult, KernelV2Error> {
    // ONE tessellation pass: the sites and the ray targets are the same
    // triangles. `primitives` hands them back in shell/face order, which is
    // the order the sites are laid out in (the tree sorts its own copy), so a
    // tie between two equally thin sites is broken the same way in every run.
    let by_face = super::primitives(arena, Target::Solid(solid))?;
    let adjacent = edge_adjacent_faces(arena, solid)?;
    let facets = Facets::index(&by_face);
    let bvh = Bvh::build(by_face.clone());
    let chord_bound = super::chord_bound(&bvh, &bvh);
    let requested = match spacing {
        None => default_spacing(&bvh),
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(_) => {
            return Err(KernelV2Error::MeasureInvalidRequest {
                reason: "thickness: spacing must be a positive, finite length in meters",
            })
        }
    };

    let mut declines = Declines::default();
    let mut sites: Vec<Site> = Vec::new();
    let mut refined = 0usize;
    let mut achieved = 0.0f64;
    let mut buf: Vec<Point3> = Vec::new();

    for prim in &by_face {
        let Prim::Tri { p: tri, face, .. } = prim else {
            continue;
        };
        buf.clear();
        achieved = achieved.max(facet_sites(tri, requested, &mut buf));
        // A face with no analytic surface has no inward normal to cast along.
        // Its sites are still COUNTED, so the decline figure is in the same
        // unit as `samples` — sites, not facets.
        let Some(surface) = arena.face(*face).ok().and_then(|f| f.surface) else {
            declines.no_surface += buf.len();
            continue;
        };
        for &centroid in buf.iter() {
            // The site goes onto the analytic surface, so the ray starts on
            // the real geometry rather than on a chord of it — this is what
            // makes a tube's wall exact instead of chord-deficient. The
            // containment check is paid for only when the projection actually
            // MOVED the point: on a plane it is the identity, and the centroid
            // is inside a facet of the face by construction.
            let site = match closest_point_on(&surface, centroid) {
                Some(p) if dist2(p, centroid) <= (TAU_WORK * scale_at(p)).powi(2) => centroid,
                Some(p) if facets.contains(*face, p, chord_bound) => p,
                // The projection failed, or it left the face's own trim.
                // The centroid is still on the face, so measure from there
                // and let the hit refinement carry what accuracy it can.
                _ => centroid,
            };
            let Some(n) = outward_normal_at(&surface, site) else {
                declines.no_surface += 1;
                continue;
            };
            let inward = [-n[0], -n[1], -n[2]];
            // The local sagitta of this site's own snap, which is how far its
            // own facets can be from it — never a global band (module docs).
            let snap = dist2(site, centroid).sqrt();
            let self_band = 4.0 * snap + 8.0 * f64::EPSILON * scale_at(site);
            match first_hit(&bvh, site, inward, *face, self_band) {
                Cast::Miss => declines.no_hit += 1,
                Cast::BelowSelfBand => declines.below_self_band += 1,
                Cast::Hit(t, hit_face) => {
                    let hit_surface = arena.face(hit_face).ok().and_then(|f| f.surface);
                    let (t, opposite) = match hit_surface
                        .and_then(|s| refine(&facets, hit_face, &s, site, inward, t, chord_bound))
                    {
                        Some((t, p)) => {
                            refined += 1;
                            (t, p)
                        }
                        None => (t, add_scaled(site, inward, t)),
                    };
                    let (lo, hi) = ((*face).min(hit_face), (*face).max(hit_face));
                    sites.push(Site {
                        thickness: t,
                        point: site,
                        opposite,
                        from: *face,
                        to: hit_face,
                        faces_share_an_edge: lo != hi && adjacent.contains(&(lo, hi)),
                    });
                }
            }
        }
    }

    let Some(thinnest) = sites
        .iter()
        .copied()
        // `b < a`, so the FIRST site at the minimum wins: the sites are laid
        // out in face order, so a tie is broken the same way in every run.
        .reduce(|a, b| if b.thickness < a.thickness { b } else { a })
    else {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "thickness: no sample produced a hit (see the decline counts)",
        });
    };
    // The thinnest WALL: the same reduction over the sites that did not cross
    // a corner. Same tie-break, so it is the same site in every run.
    let thinnest_wall = sites
        .iter()
        .copied()
        .filter(|s| !s.faces_share_an_edge)
        .reduce(|a, b| if b.thickness < a.thickness { b } else { a });
    let max = sites
        .iter()
        .fold(f64::NEG_INFINITY, |m, s| m.max(s.thickness));
    let min = thinnest.thickness;
    let mean = sites.iter().map(|s| s.thickness).sum::<f64>() / sites.len() as f64;

    // Equal-width bins over the measured range; the last bin is closed, so the
    // maximum is counted rather than falling off the end.
    let span = max - min;
    let bins = if span > 0.0 { HISTOGRAM_BINS } else { 1 };
    let width = if span > 0.0 { span / bins as f64 } else { 0.0 };
    let mut histogram: Vec<Bin> = (0..bins)
        .map(|i| Bin {
            lo: min + width * i as f64,
            hi: min + width * (i + 1) as f64,
            count: 0,
        })
        .collect();
    for s in &sites {
        let i = if width > 0.0 {
            (((s.thickness - min) / width) as usize).min(bins - 1)
        } else {
            0
        };
        histogram[i].count += 1;
    }

    Ok(ThicknessResult {
        min,
        min_wall: thinnest_wall.map(|s| s.thickness),
        thinnest_wall,
        mean,
        max,
        thinnest,
        histogram,
        samples: sites.len(),
        spacing: achieved,
        chord_bound,
        refined,
        declines,
    })
}

//! F2 — the §4.4.1 patch chart on the face's parameter domain.
//!
//! Spec: `specs/yang_f2_patch_chart.md`. Yang 2025 §4.4.1 triangulates each
//! trimmed patch "totally ... in the parametric domain"
//! (`refs/text/yang2025_hybrid_boolean.txt:560-561`). The paper's NURBS faces
//! carry their seam and degenerate edges in the face loop; ours do not, so
//! this module supplies them: it charts a patch in its surface's
//! [`ParamSurface`] domain, cuts a patch that wraps a period open along a
//! seam, and represents a cone apex or a sphere pole inside the patch as a
//! degenerate EDGE of the chart polygon whose copies all map to one mesh
//! vertex.
//!
//! The patch's own triangles answer every topological question: they are
//! developed onto the universal cover of the domain, and the period lattice
//! their loops reach, the degenerate points they cover and the windings of
//! the boundary cycles classify the patch (spec §3).

use crate::param_surface::{AnalyticInverse, Degeneracy, ParamSurface, Uv};
use crate::stage4_project::point_in_polygon;
use crate::stage4_update::Patch;
use crate::Surface;
use cad_primitives::{Point2, Point3};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A degenerate edge is split into pieces no wider than this in u — the
/// angular span the analytic Bézier nets are split to (F5a §3).
const MAX_DEGENERATE_PIECE: f64 = std::f64::consts::FRAC_PI_2;
/// Bisection depth cap for a seam iso-line's subdivision.
const MAX_SEAM_DEPTH: u32 = 10;

/// Which chart polygon the patch's topology called for (spec §3 item 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DomainClass {
    /// Wraps no period and covers no degenerate point.
    Disc,
    /// Two opposite cycles wind the u period (an azimuth band).
    UBand,
    /// Two opposite cycles wind the v period (a torus tube band).
    VBand,
    /// One cycle winds the azimuth around a degenerate point the patch covers
    /// (a cone apex, a sphere pole).
    Cap,
    /// A sphere patch covering both poles; every cycle is a hole.
    Zone,
}

/// Why the domain chart refused a patch. Every variant is loud: the caller
/// keeps the patch's triangles and names the class in its census.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DomainRefusal {
    /// The surface's inverse refused a patch vertex (a cone point behind the
    /// apex, an invalid surface).
    Inverse,
    /// An edge is shared by more than two of the patch's triangles.
    NotManifold,
    /// The patch's triangles are not edge-connected.
    Disconnected,
    /// Two developments of one vertex differ by something other than whole
    /// periods, or the patch's loops reach both periods (a torus patch of
    /// genus ≥ 1).
    Lattice,
    /// A triangle winds around an axis the surface has no degenerate point
    /// on.
    WindingTriangle,
    /// A boundary cycle passes through a degenerate point (an apex or pole ON
    /// the patch boundary).
    DegenerateOnCycle,
    /// Two distinct mesh vertices sit on one degenerate point.
    DegenerateVertices,
    /// The cycle windings and covered degenerate points fit no class.
    Cycles,
    /// No seam clear of every cycle exists.
    NoSeam,
    /// A cycle is shorter than 3 vertices, a vertex is in no triangle, or the
    /// polygon has zero area.
    Polygon,
    /// A replaced triangle's d(T) could not be certified.
    Certify,
}

/// A chart-polygon vertex: an existing mesh vertex, or the `k`-th new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChartVert {
    Mesh(u32),
    New(usize),
}

/// One patch charted on its surface's parameter domain.
#[derive(Debug, Clone)]
pub(crate) struct DomainChart {
    pub class: DomainClass,
    /// The chart polygon in (u, v): `boundary` CCW, `holes` as walked;
    /// `verts` also holds the carried interior points (`interior`).
    pub polygon: Patch,
    /// 1:1 with `polygon.verts`.
    pub back: Vec<ChartVert>,
    /// 1:1 with `polygon.verts`: the vertex is a copy on a degenerate edge.
    pub degenerate: Vec<bool>,
    /// Indices into `polygon.verts` of the carried interior vertices.
    pub interior: Vec<u32>,
    /// Positions of the new vertices (`ChartVert::New(k)`), exactly on the
    /// surface.
    pub new_verts: Vec<Point3>,
    /// Certified max d(T) of the replaced triangles, read on their developed
    /// coordinates.
    pub old_max: f64,
    /// `min(old_max, d_ε)` — the budget the rebuild must meet.
    pub budget: f64,
    /// Arc length per unit of u and of v at the polygon's centre: the scale
    /// the CDT's Delaunay criterion runs in.
    pub metric: [f64; 2],
    /// Interior vertices lying outside the polygon on every copy (relocated
    /// past the curve), removed rather than carried.
    pub removed_outside: usize,
}

/// d(T) corners for a triangle with chart corners `uv`: a corner on a
/// degenerate iso-line has no azimuth, so it takes the u of a corner that
/// has one — the covering rectangle is then the wedge the triangle spans.
pub(crate) fn dt_corners(uv: [Uv; 3], degenerate: [bool; 3]) -> [Uv; 3] {
    let Some(k) = (0..3).find(|&k| !degenerate[k]) else {
        return uv;
    };
    let mut out = uv;
    for m in 0..3 {
        if degenerate[m] {
            out[m] = Point2::new(uv[k].x(), uv[m].y());
        }
    }
    out
}

struct Ctx<'a> {
    surface: &'a Surface,
    verts: &'a [Point3],
    period: [Option<f64>; 2],
    deg_v: Vec<f64>,
}

impl Ctx<'_> {
    fn inv(&self, v: u32, seed: Option<Uv>) -> Result<Uv, DomainRefusal> {
        let p = self
            .verts
            .get(v as usize)
            .ok_or(DomainRefusal::Polygon)?
            .as_array();
        self.surface
            .inverse(p, seed)
            .map_err(|_| DomainRefusal::Inverse)
    }

    fn is_deg(&self, uv: Uv) -> bool {
        self.deg_v.contains(&uv.y())
    }

    /// Whole periods between two developments of one point, per axis;
    /// `None` when a non-periodic coordinate differs.
    fn periods_between(&self, a: Uv, b: Uv) -> Option<(i64, i64)> {
        let mut k = [0i64; 2];
        for (ax, d) in [(0, b.x() - a.x()), (1, b.y() - a.y())] {
            match self.period[ax] {
                Some(p) => k[ax] = (d / p).round() as i64,
                None if d != 0.0 => return None,
                None => {}
            }
        }
        Some((k[0], k[1]))
    }

    fn eval(&self, uv: Uv) -> Result<Point3, DomainRefusal> {
        self.surface
            .eval(uv)
            .map(|q| Point3::new(q[0], q[1], q[2]))
            .map_err(|_| DomainRefusal::Inverse)
    }
}

fn coord(p: Point2, ax: usize) -> f64 {
    if ax == 0 {
        p.x()
    } else {
        p.y()
    }
}

fn shifted(p: Point2, ax: usize, by: f64) -> Point2 {
    if ax == 0 {
        Point2::new(p.x() + by, p.y())
    } else {
        Point2::new(p.x(), p.y() + by)
    }
}

/// The patch's triangles developed onto the universal cover of the domain.
struct Development {
    corner: Vec<[Uv; 3]>,
    deg: Vec<[bool; 3]>,
    lattice: BTreeSet<(i64, i64)>,
    /// The first non-degenerate development of each vertex.
    vert_uv: BTreeMap<u32, Uv>,
    /// Mesh vertices sitting exactly on a degenerate iso-line, by iso value
    /// (as bits, for ordering).
    deg_vertex: BTreeMap<u64, BTreeSet<u32>>,
    /// Degenerate iso values (as bits) some triangle winds around.
    wound: BTreeSet<u64>,
}

fn develop(ctx: &Ctx<'_>, old: &[[u32; 3]]) -> Result<Development, DomainRefusal> {
    let n = old.len();
    if n == 0 {
        return Err(DomainRefusal::Polygon);
    }
    let mut edges: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (ti, t) in old.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let e = edges.entry((a.min(b), a.max(b))).or_default();
            e.push(ti);
            if e.len() > 2 {
                return Err(DomainRefusal::NotManifold);
            }
        }
    }
    let place =
        |t: [u32; 3], known: [Option<Uv>; 3]| -> Result<([Uv; 3], [bool; 3]), DomainRefusal> {
            let mut seed = (0..3).find_map(|m| known[m].filter(|&c| !ctx.is_deg(c)));
            if seed.is_none() {
                for &v in &t {
                    let c = ctx.inv(v, None)?;
                    if !ctx.is_deg(c) {
                        seed = Some(c);
                        break;
                    }
                }
            }
            let seed = seed.ok_or(DomainRefusal::Inverse)?;
            let mut uv = [seed; 3];
            let mut deg = [false; 3];
            for m in 0..3 {
                uv[m] = match known[m] {
                    Some(c) => c,
                    None => ctx.inv(t[m], Some(seed))?,
                };
                deg[m] = ctx.is_deg(uv[m]);
            }
            Ok((uv, deg))
        };

    let mut corner = vec![[Point2::new(0.0, 0.0); 3]; n];
    let mut deg = vec![[false; 3]; n];
    let mut placed = vec![false; n];
    let mut lattice = BTreeSet::new();
    let (c0, d0) = place(old[0], [None; 3])?;
    corner[0] = c0;
    deg[0] = d0;
    placed[0] = true;
    let mut queue = VecDeque::from([0usize]);
    while let Some(ti) = queue.pop_front() {
        let t = old[ti];
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let shared = [
                (a, corner[ti][k], deg[ti][k]),
                (b, corner[ti][(k + 1) % 3], deg[ti][(k + 1) % 3]),
            ];
            for &tj in &edges[&(a.min(b), a.max(b))] {
                if tj == ti {
                    continue;
                }
                if !placed[tj] {
                    let mut known = [None; 3];
                    for m in 0..3 {
                        for &(v, c, _) in &shared {
                            if old[tj][m] == v {
                                known[m] = Some(c);
                            }
                        }
                    }
                    let (c, d) = place(old[tj], known)?;
                    corner[tj] = c;
                    deg[tj] = d;
                    placed[tj] = true;
                    queue.push_back(tj);
                } else {
                    for &(v, c, dv) in &shared {
                        if dv {
                            continue;
                        }
                        for m in 0..3 {
                            if old[tj][m] == v && !deg[tj][m] {
                                let k = ctx
                                    .periods_between(c, corner[tj][m])
                                    .ok_or(DomainRefusal::Lattice)?;
                                if k != (0, 0) {
                                    lattice.insert(k);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if placed.iter().any(|&p| !p) {
        return Err(DomainRefusal::Disconnected);
    }

    let mut vert_uv = BTreeMap::new();
    let mut deg_vertex: BTreeMap<u64, BTreeSet<u32>> = BTreeMap::new();
    let mut wound = BTreeSet::new();
    for ti in 0..n {
        for m in 0..3 {
            if deg[ti][m] {
                deg_vertex
                    .entry(corner[ti][m].y().to_bits())
                    .or_default()
                    .insert(old[ti][m]);
            } else {
                vert_uv.entry(old[ti][m]).or_insert(corner[ti][m]);
            }
        }
        // A triangle around the azimuth axis: its corner azimuths, unwrapped
        // consecutively, sum to ±2π.
        if let (Some(p), false) = (ctx.period[0], deg[ti].iter().any(|&d| d)) {
            let half = 0.5 * p;
            let wrap = |d: f64| d - p * (d / p).round();
            let u = corner[ti].map(|c| c.x());
            let s = wrap(u[1] - u[0]) + wrap(u[2] - u[1]) + wrap(u[0] - u[2]);
            if s.abs() > half {
                let v_mean = (corner[ti][0].y() + corner[ti][1].y() + corner[ti][2].y()) / 3.0;
                let which = match ctx.deg_v.len() {
                    0 => return Err(DomainRefusal::WindingTriangle),
                    1 => ctx.deg_v[0],
                    _ => {
                        // Two degenerate iso-lines (a sphere's poles): the one
                        // on the triangle's side of the middle.
                        let (lo, hi) = (
                            ctx.deg_v.iter().copied().fold(f64::INFINITY, f64::min),
                            ctx.deg_v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        );
                        if v_mean > 0.5 * (lo + hi) {
                            hi
                        } else {
                            lo
                        }
                    }
                };
                wound.insert(which.to_bits());
            }
        }
    }
    Ok(Development {
        corner,
        deg,
        lattice,
        vert_uv,
        deg_vertex,
        wound,
    })
}

/// One boundary cycle unwrapped along itself.
struct Walk {
    verts: Vec<u32>,
    pts: Vec<Uv>,
    /// The first vertex reached again after one walk round.
    close: Uv,
    wind: (i64, i64),
}

impl Walk {
    /// The chart point of position `i` (any `i ≥ 0`), shifted by the walk's
    /// winding for every completed round.
    fn at(&self, i: usize, period: [Option<f64>; 2]) -> (u32, Uv) {
        let n = self.verts.len();
        let (q, r) = (i / n, i % n);
        let p = self.pts[r];
        let du = period[0].map_or(0.0, |t| t * (q as i64 * self.wind.0) as f64);
        let dv = period[1].map_or(0.0, |t| t * (q as i64 * self.wind.1) as f64);
        (self.verts[r], Point2::new(p.x() + du, p.y() + dv))
    }

    /// Edges as chart segments with their mesh endpoints.
    fn edges(&self) -> impl Iterator<Item = ((u32, Uv), (u32, Uv))> + '_ {
        let n = self.verts.len();
        (0..n).map(move |i| {
            let b = if i + 1 < n {
                self.pts[i + 1]
            } else {
                self.close
            };
            ((self.verts[i], self.pts[i]), (self.verts[(i + 1) % n], b))
        })
    }
}

fn walk(ctx: &Ctx<'_>, cyc: &[u32], start: Uv) -> Result<Walk, DomainRefusal> {
    if cyc.len() < 3 {
        return Err(DomainRefusal::Polygon);
    }
    let mut pts = Vec::with_capacity(cyc.len());
    pts.push(start);
    for &v in &cyc[1..] {
        let c = ctx.inv(v, pts.last().copied())?;
        if ctx.is_deg(c) {
            return Err(DomainRefusal::DegenerateOnCycle);
        }
        pts.push(c);
    }
    let close = ctx.inv(cyc[0], pts.last().copied())?;
    let wind = ctx
        .periods_between(start, close)
        .ok_or(DomainRefusal::Lattice)?;
    Ok(Walk {
        verts: cyc.to_vec(),
        pts,
        close,
        wind,
    })
}

/// Proper crossing or touching of two chart segments.
fn segments_meet(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let o = |p: Point2, q: Point2, r: Point2| {
        (q.x() - p.x()) * (r.y() - p.y()) - (q.y() - p.y()) * (r.x() - p.x())
    };
    let (d1, d2) = (o(a0, a1, b0), o(a0, a1, b1));
    let (d3, d4) = (o(b0, b1, a0), o(b0, b1, a1));
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0
}

/// A seam segment is clear when it meets no cycle edge on any copy (two
/// periods either way on every periodic axis), edges incident to its own
/// mesh endpoints excepted.
fn seam_clear(
    walks: &[Walk],
    period: [Option<f64>; 2],
    s0: Point2,
    s1: Point2,
    ends: &[u32],
) -> bool {
    let ku: &[i64] = if period[0].is_some() {
        &[-2, -1, 0, 1, 2]
    } else {
        &[0]
    };
    let kv: &[i64] = if period[1].is_some() {
        &[-2, -1, 0, 1, 2]
    } else {
        &[0]
    };
    for w in walks {
        for ((va, pa), (vb, pb)) in w.edges() {
            if ends.contains(&va) || ends.contains(&vb) {
                continue;
            }
            for &i in ku {
                for &j in kv {
                    let du = period[0].map_or(0.0, |t| t * i as f64);
                    let dv = period[1].map_or(0.0, |t| t * j as f64);
                    let a = Point2::new(pa.x() + du, pa.y() + dv);
                    let b = Point2::new(pb.x() + du, pb.y() + dv);
                    if segments_meet(s0, s1, a, b) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Points strictly between `v0` and `v1` on the iso-line u = `u`, ordered
/// from `v0`, so that each piece's chord stays within `sag` of the surface at
/// its midpoint (bisection; depth-capped — the caller's d(T) gate certifies).
fn iso_points(
    ctx: &Ctx<'_>,
    u: f64,
    v0: f64,
    v1: f64,
    sag: f64,
) -> Result<Vec<f64>, DomainRefusal> {
    fn rec(
        ctx: &Ctx<'_>,
        u: f64,
        a: f64,
        b: f64,
        sag: f64,
        depth: u32,
        out: &mut Vec<f64>,
    ) -> Result<(), DomainRefusal> {
        if depth >= MAX_SEAM_DEPTH {
            return Ok(());
        }
        let m = 0.5 * (a + b);
        let (pa, pb, pm) = (
            ctx.eval(Point2::new(u, a))?.as_array(),
            ctx.eval(Point2::new(u, b))?.as_array(),
            ctx.eval(Point2::new(u, m))?.as_array(),
        );
        let mid = [
            0.5 * (pa[0] + pb[0]),
            0.5 * (pa[1] + pb[1]),
            0.5 * (pa[2] + pb[2]),
        ];
        let d =
            ((pm[0] - mid[0]).powi(2) + (pm[1] - mid[1]).powi(2) + (pm[2] - mid[2]).powi(2)).sqrt();
        if d <= sag {
            return Ok(());
        }
        rec(ctx, u, a, m, sag, depth + 1, out)?;
        out.push(m);
        rec(ctx, u, m, b, sag, depth + 1, out)
    }
    let mut out = Vec::new();
    rec(ctx, u, v0, v1, sag, 0, &mut out)?;
    Ok(out)
}

struct Builder {
    pts: Vec<Uv>,
    back: Vec<ChartVert>,
    deg: Vec<bool>,
}

impl Builder {
    fn push(&mut self, p: Uv, b: ChartVert, d: bool) -> u32 {
        self.pts.push(p);
        self.back.push(b);
        self.deg.push(d);
        (self.pts.len() - 1) as u32
    }

    fn area2(&self, lp: &[u32]) -> f64 {
        (0..lp.len())
            .map(|i| {
                let (a, b) = (
                    self.pts[lp[i] as usize],
                    self.pts[lp[(i + 1) % lp.len()] as usize],
                );
                a.x() * b.y() - b.x() * a.y()
            })
            .sum()
    }

    /// Orient `outer` CCW; refuse a zero or non-finite area.
    fn ccw(&self, mut outer: Vec<u32>) -> Result<Vec<u32>, DomainRefusal> {
        let a = self.area2(&outer);
        if a == 0.0 || !a.is_finite() {
            return Err(DomainRefusal::Polygon);
        }
        if a < 0.0 {
            outer.reverse();
        }
        Ok(outer)
    }

    fn inside(&self, q: Uv, outer: &[u32], holes: &[Vec<u32>]) -> bool {
        point_in_polygon(q, outer, &self.pts)
            && !holes.iter().any(|h| point_in_polygon(q, h, &self.pts))
    }
}

/// Place each walk in `idx` as a hole on the copy (shifts along `ax`) whose
/// first vertex lies inside `outer`.
fn place_holes(
    b: &mut Builder,
    walks: &[Walk],
    idx: &[usize],
    outer: &[u32],
    shift_axis: Option<(usize, f64)>,
) -> Result<Vec<Vec<u32>>, DomainRefusal> {
    let mut holes = Vec::new();
    for &hi in idx {
        let w = &walks[hi];
        let ks: Vec<f64> = match shift_axis {
            Some((_, p)) => [0.0, -1.0, 1.0, -2.0, 2.0].iter().map(|k| k * p).collect(),
            None => vec![0.0],
        };
        let ax = shift_axis.map_or(0, |s| s.0);
        let by = ks
            .into_iter()
            .find(|&s| point_in_polygon(shifted(w.pts[0], ax, s), outer, &b.pts))
            .ok_or(DomainRefusal::Polygon)?;
        let lp = w
            .verts
            .iter()
            .zip(&w.pts)
            .map(|(&v, &p)| b.push(shifted(p, ax, by), ChartVert::Mesh(v), false))
            .collect();
        holes.push(lp);
    }
    Ok(holes)
}

/// Chart the patch with boundary `cycles` and triangles `old` (mesh indices)
/// on `surface`'s parameter domain. `excluded` interiors are not carried.
/// `d_eps` caps the d(T) budget (`min(old max, d_ε)`).
pub(crate) fn domain_chart(
    surface: &Surface,
    verts: &[Point3],
    cycles: &[Vec<u32>],
    old: &[[u32; 3]],
    excluded: &BTreeSet<u32>,
    d_eps: Option<f64>,
) -> Result<DomainChart, DomainRefusal> {
    if surface.validate().is_err() {
        return Err(DomainRefusal::Inverse);
    }
    let dom = surface.domain();
    let mut deg_v = Vec::new();
    for d in &dom.degenerate {
        match *d {
            Degeneracy::VIso(v) => deg_v.push(v),
            // No analytic instance degenerates along a u-iso-line.
            Degeneracy::UIso(_) => return Err(DomainRefusal::Cycles),
        }
    }
    let ctx = Ctx {
        surface,
        verts,
        period: [dom.u.period, dom.v.period],
        deg_v,
    };
    let dev = develop(&ctx, old)?;

    // The replaced triangles' certified bound, on their developed corners.
    let mut old_max = 0.0f64;
    for ti in 0..old.len() {
        let uv = dt_corners(dev.corner[ti], dev.deg[ti]);
        let d = crate::stage4_dt::d_of_t(surface, uv).map_err(|_| DomainRefusal::Certify)?;
        old_max = old_max.max(d);
    }
    let budget = match d_eps {
        Some(e) if e.is_finite() && e > 0.0 => old_max.min(e),
        _ => old_max,
    };

    // Degenerate points the patch covers, and the mesh vertex on each.
    let mut covered: BTreeSet<u64> = dev.deg_vertex.keys().copied().collect();
    covered.extend(dev.wound.iter().copied());
    let mut deg_mesh: BTreeMap<u64, Option<u32>> = BTreeMap::new();
    for &d in &covered {
        let on = dev.deg_vertex.get(&d);
        if on.is_some_and(|s| s.len() > 1) {
            return Err(DomainRefusal::DegenerateVertices);
        }
        deg_mesh.insert(d, on.and_then(|s| s.iter().next().copied()));
    }

    let cycle_set: BTreeSet<u32> = cycles.iter().flatten().copied().collect();
    let mut walks = Vec::with_capacity(cycles.len());
    for c in cycles {
        let Some(&first) = c.first() else {
            return Err(DomainRefusal::Polygon);
        };
        if dev.deg_vertex.values().any(|s| s.contains(&first)) {
            return Err(DomainRefusal::DegenerateOnCycle);
        }
        let start = *dev.vert_uv.get(&first).ok_or(DomainRefusal::Polygon)?;
        walks.push(walk(&ctx, c, start)?);
    }
    let enc: Vec<usize> = (0..walks.len())
        .filter(|&i| walks[i].wind != (0, 0))
        .collect();
    let lat_u = !dev.lattice.is_empty() && dev.lattice.iter().all(|k| k.1 == 0);
    let lat_v = !dev.lattice.is_empty() && dev.lattice.iter().all(|k| k.0 == 0);
    // A loop round a degenerate point inside one triangle (a cap with no
    // vertex on the apex) develops without a mismatch, so a cap or zone may
    // show an empty lattice.
    let u_only = dev.lattice.is_empty() || lat_u;
    let band = |ax_wind: fn(&Walk) -> i64, other: fn(&Walk) -> i64| {
        let (a, b) = (&walks[enc[0]], &walks[enc[1]]);
        other(a) == 0 && other(b) == 0 && ax_wind(a).abs() == 1 && ax_wind(a) == -ax_wind(b)
    };
    if !dev.lattice.is_empty() && !lat_u && !lat_v {
        return Err(DomainRefusal::Lattice);
    }
    let class = match (covered.len(), enc.len()) {
        (0, 0) if dev.lattice.is_empty() => DomainClass::Disc,
        (0, 2) if lat_u && band(|w| w.wind.0, |w| w.wind.1) => DomainClass::UBand,
        (0, 2) if lat_v && band(|w| w.wind.1, |w| w.wind.0) => DomainClass::VBand,
        (1, 1) if u_only && walks[enc[0]].wind.1 == 0 && walks[enc[0]].wind.0.abs() == 1 => {
            DomainClass::Cap
        }
        (2, 0) if u_only => DomainClass::Zone,
        _ => return Err(DomainRefusal::Cycles),
    };
    let wrap_axis: Option<(usize, f64)> = match class {
        DomainClass::Disc => None,
        DomainClass::VBand => Some((1, ctx.period[1].ok_or(DomainRefusal::Cycles)?)),
        _ => Some((0, ctx.period[0].ok_or(DomainRefusal::Cycles)?)),
    };

    // Interior vertices: every old vertex not on a cycle, not excluded, not
    // on a degenerate point, not on a cycle edge (a removed vertex, §4.4.1),
    // with its developed coordinate.
    let on_cycle_edge = |v: u32| -> bool {
        cycles.iter().any(|cyc| {
            let n = cyc.len();
            (0..n).any(|i| {
                crate::stage4_construct::on_segment_interior(verts, cyc[i], cyc[(i + 1) % n], v)
            })
        })
    };
    let deg_mesh_verts: BTreeSet<u32> = dev.deg_vertex.values().flatten().copied().collect();
    let mut inner: Vec<(u32, Uv)> = Vec::new();
    for v in old.iter().flatten().copied().collect::<BTreeSet<u32>>() {
        if cycle_set.contains(&v)
            || excluded.contains(&v)
            || deg_mesh_verts.contains(&v)
            || on_cycle_edge(v)
        {
            continue;
        }
        inner.push((v, *dev.vert_uv.get(&v).ok_or(DomainRefusal::Polygon)?));
    }
    // Interior vertices lying exactly on a seam segment `s0 → s1` (on any
    // copy along the wrap axis) become seam vertices: the CDT would split the
    // seam at them on one copy only. Returned in order from `s0`, each on the
    // `s0 → s1` copy.
    let on_seam = |s0: Uv, s1: Uv| -> Vec<(u32, Uv)> {
        let (dx, dy) = (s1.x() - s0.x(), s1.y() - s0.y());
        let len2 = dx * dx + dy * dy;
        let mut hits: Vec<(f64, u32, Uv)> = Vec::new();
        for &(v, base) in &inner {
            let copies: Vec<Uv> = match wrap_axis {
                None => vec![base],
                Some((ax, p)) => [0.0, -1.0, 1.0, -2.0, 2.0]
                    .iter()
                    .map(|k| shifted(base, ax, k * p))
                    .collect(),
            };
            for q in copies {
                let (qx, qy) = (q.x() - s0.x(), q.y() - s0.y());
                let t = (qx * dx + qy * dy) / len2;
                if dx * qy - dy * qx == 0.0 && t > 0.0 && t < 1.0 {
                    hits.push((t, v, q));
                    break;
                }
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        hits.into_iter().map(|(_, v, q)| (v, q)).collect()
    };
    let mut seam_verts: BTreeSet<u32> = BTreeSet::new();

    // Polygon in (u, v).
    let mut b = Builder {
        pts: Vec::new(),
        back: Vec::new(),
        deg: Vec::new(),
    };
    let mut new_verts: Vec<Point3> = Vec::new();
    // The vertex every copy of a degenerate edge maps to: the mesh vertex on
    // the point, else one new vertex there (minted once).
    let mut deg_new: BTreeMap<u64, ChartVert> = BTreeMap::new();
    let mut deg_vert = |d: u64, u: f64, nv: &mut Vec<Point3>| -> Result<ChartVert, DomainRefusal> {
        if let Some(&c) = deg_new.get(&d) {
            return Ok(c);
        }
        let c = match deg_mesh.get(&d).copied().flatten() {
            Some(v) => ChartVert::Mesh(v),
            None => {
                nv.push(ctx.eval(Point2::new(u, f64::from_bits(d)))?);
                ChartVert::New(nv.len() - 1)
            }
        };
        deg_new.insert(d, c);
        Ok(c)
    };
    // An iso-line seam u = `u` from `v0` to `v1` (exclusive), through the
    // interior vertices on it, each piece subdivided to `sag`.
    let sag = 0.5 * budget;
    let iso_seam = |u: f64,
                    v0: f64,
                    v1: f64,
                    fixed: &[(u32, Uv)],
                    nv: &mut Vec<Point3>|
     -> Result<Vec<(f64, ChartVert)>, DomainRefusal> {
        let mut stops: Vec<(f64, Option<u32>)> = vec![(v0, None)];
        stops.extend(fixed.iter().map(|&(v, q)| (q.y(), Some(v))));
        stops.push((v1, None));
        let mut out = Vec::new();
        for w in stops.windows(2) {
            for v in iso_points(&ctx, u, w[0].0, w[1].0, sag)? {
                nv.push(ctx.eval(Point2::new(u, v))?);
                out.push((v, ChartVert::New(nv.len() - 1)));
            }
            if let Some(m) = w[1].1 {
                out.push((w[1].0, ChartVert::Mesh(m)));
            }
        }
        Ok(out)
    };
    let (outer, holes): (Vec<u32>, Vec<Vec<u32>>) = match class {
        DomainClass::Disc => {
            let mut loops: Vec<Vec<u32>> = walks
                .iter()
                .map(|w| {
                    w.verts
                        .iter()
                        .zip(&w.pts)
                        .map(|(&v, &p)| b.push(p, ChartVert::Mesh(v), false))
                        .collect()
                })
                .collect();
            let mut outer_at = 0;
            for k in 1..loops.len() {
                if b.area2(&loops[k]).abs() > b.area2(&loops[outer_at]).abs() {
                    outer_at = k;
                }
            }
            let outer = b.ccw(loops.swap_remove(outer_at))?;
            (outer, loops)
        }
        DomainClass::UBand | DomainClass::VBand => {
            let (ax, p) = wrap_axis.ok_or(DomainRefusal::Cycles)?;
            let (wa_i, wb_i) = (enc[0], enc[1]);
            let (wa, wb) = (&walks[wa_i], &walks[wb_i]);
            let wind_a = if ax == 0 { wa.wind.0 } else { wa.wind.1 };
            // Shortest clear seam: for each vertex of A, the three vertices of
            // B nearest along the wrap axis.
            let mut best: Option<(f64, usize, usize, Uv)> = None;
            for (ia, &pa) in wa.pts.iter().enumerate() {
                let mut cand: Vec<(f64, usize, Uv)> = wb
                    .pts
                    .iter()
                    .enumerate()
                    .map(|(ib, &pb)| {
                        let k = ((coord(pa, ax) - coord(pb, ax)) / p).round();
                        let q = shifted(pb, ax, k * p);
                        ((coord(q, ax) - coord(pa, ax)).abs(), ib, q)
                    })
                    .collect();
                cand.sort_by(|x, y| x.0.total_cmp(&y.0));
                for &(_, ib, pb) in cand.iter().take(3) {
                    let len = ((pb.x() - pa.x()).powi(2) + (pb.y() - pa.y()).powi(2)).sqrt();
                    if best.is_some_and(|bb| bb.0 <= len) {
                        continue;
                    }
                    if seam_clear(&walks, ctx.period, pa, pb, &[wa.verts[ia], wb.verts[ib]]) {
                        best = Some((len, ia, ib, pb));
                    }
                }
            }
            let (_, ia, ib, pb) = best.ok_or(DomainRefusal::NoSeam)?;
            let pa = wa.pts[ia];
            let fixed = on_seam(pa, pb);
            seam_verts.extend(fixed.iter().map(|f| f.0));
            let mut outer = Vec::new();
            let na = wa.verts.len();
            for k in 0..=na {
                let (v, q) = wa.at(ia + k, ctx.period);
                outer.push(b.push(q, ChartVert::Mesh(v), false));
            }
            // The seam's copy one turn on, from a' to b''.
            let turn = wind_a as f64 * p;
            for &(v, q) in &fixed {
                outer.push(b.push(shifted(q, ax, turn), ChartVert::Mesh(v), false));
            }
            // B from b's copy one turn on, once round, ending at b.
            let start_b = shifted(pb, ax, turn);
            let (_, base_b) = wb.at(ib, ctx.period);
            let nb = wb.verts.len();
            for k in 0..=nb {
                let (v, q) = wb.at(ib + k, ctx.period);
                let q = Point2::new(
                    q.x() - base_b.x() + start_b.x(),
                    q.y() - base_b.y() + start_b.y(),
                );
                outer.push(b.push(q, ChartVert::Mesh(v), false));
            }
            // The seam back from b to a.
            for &(v, q) in fixed.iter().rev() {
                outer.push(b.push(q, ChartVert::Mesh(v), false));
            }
            let outer = b.ccw(outer)?;
            let rest: Vec<usize> = (0..walks.len())
                .filter(|&i| i != wa_i && i != wb_i)
                .collect();
            let holes = place_holes(&mut b, &walks, &rest, &outer, wrap_axis)?;
            (outer, holes)
        }
        DomainClass::Cap => {
            let p = wrap_axis.ok_or(DomainRefusal::Cycles)?.1;
            let d = *covered.iter().next().ok_or(DomainRefusal::Cycles)?;
            let v_d = f64::from_bits(d);
            let we = &walks[enc[0]];
            let mut best: Option<(f64, usize)> = None;
            for (i, &pa) in we.pts.iter().enumerate() {
                let len = (pa.y() - v_d).abs();
                if best.is_some_and(|bb| bb.0 <= len) {
                    continue;
                }
                let foot = Point2::new(pa.x(), v_d);
                if seam_clear(&walks, ctx.period, pa, foot, &[we.verts[i]]) {
                    best = Some((len, i));
                }
            }
            let (_, i) = best.ok_or(DomainRefusal::NoSeam)?;
            let pa = we.pts[i];
            let foot = Point2::new(pa.x(), v_d);
            let u_end = pa.x() + we.wind.0 as f64 * p;
            let fixed = on_seam(pa, foot);
            seam_verts.extend(fixed.iter().map(|f| f.0));
            let seam = iso_seam(pa.x(), pa.y(), v_d, &fixed, &mut new_verts)?;
            let dv = deg_vert(d, pa.x(), &mut new_verts)?;
            let mut outer = Vec::new();
            let n = we.verts.len();
            for k in 0..=n {
                let (v, q) = we.at(i + k, ctx.period);
                outer.push(b.push(q, ChartVert::Mesh(v), false));
            }
            for &(v, c) in &seam {
                outer.push(b.push(Point2::new(u_end, v), c, false));
            }
            let pieces = ((u_end - pa.x()).abs() / MAX_DEGENERATE_PIECE)
                .ceil()
                .max(1.0) as usize;
            for s in 0..=pieces {
                let u = u_end + (pa.x() - u_end) * s as f64 / pieces as f64;
                outer.push(b.push(Point2::new(u, v_d), dv, true));
            }
            for &(v, c) in seam.iter().rev() {
                outer.push(b.push(Point2::new(pa.x(), v), c, false));
            }
            let outer = b.ccw(outer)?;
            let rest: Vec<usize> = (0..walks.len()).filter(|&k| k != enc[0]).collect();
            let holes = place_holes(&mut b, &walks, &rest, &outer, wrap_axis)?;
            (outer, holes)
        }
        DomainClass::Zone => {
            let p = wrap_axis.ok_or(DomainRefusal::Cycles)?.1;
            let mut ds = covered.iter().map(|&d| f64::from_bits(d));
            let (d0, d1) = (
                ds.next().ok_or(DomainRefusal::Cycles)?,
                ds.next().ok_or(DomainRefusal::Cycles)?,
            );
            let (v_lo, v_hi) = (d0.min(d1), d0.max(d1));
            // A meridian through no vertex and clear of every cycle: the
            // middles of the gaps between all the patch's azimuths, widest
            // first.
            let mut us: Vec<f64> = walks
                .iter()
                .flat_map(|w| w.pts.iter().map(|q| q.x().rem_euclid(p)))
                .chain(inner.iter().map(|&(_, q)| q.x().rem_euclid(p)))
                .collect();
            us.sort_by(f64::total_cmp);
            us.dedup();
            let mut gaps: Vec<(f64, f64)> = (0..us.len())
                .map(|i| {
                    let (a, c) = (
                        us[i],
                        if i + 1 < us.len() {
                            us[i + 1]
                        } else {
                            us[0] + p
                        },
                    );
                    (c - a, 0.5 * (a + c))
                })
                .collect();
            gaps.sort_by(|x, y| y.0.total_cmp(&x.0));
            let u_s = gaps
                .iter()
                .map(|g| g.1)
                .find(|&u| {
                    seam_clear(
                        &walks,
                        ctx.period,
                        Point2::new(u, v_lo),
                        Point2::new(u, v_hi),
                        &[],
                    )
                })
                .ok_or(DomainRefusal::NoSeam)?;
            let seam = iso_seam(u_s, v_lo, v_hi, &[], &mut new_verts)?;
            let south = deg_vert(v_lo.to_bits(), u_s, &mut new_verts)?;
            let north = deg_vert(v_hi.to_bits(), u_s, &mut new_verts)?;
            let pieces = (p / MAX_DEGENERATE_PIECE).ceil().max(1.0) as usize;
            let mut outer = Vec::new();
            for s in 0..=pieces {
                let u = u_s + p * s as f64 / pieces as f64;
                outer.push(b.push(Point2::new(u, v_lo), south, true));
            }
            for &(v, c) in &seam {
                outer.push(b.push(Point2::new(u_s + p, v), c, false));
            }
            for s in 0..=pieces {
                let u = u_s + p - p * s as f64 / pieces as f64;
                outer.push(b.push(Point2::new(u, v_hi), north, true));
            }
            for &(v, c) in seam.iter().rev() {
                outer.push(b.push(Point2::new(u_s, v), c, false));
            }
            let outer = b.ccw(outer)?;
            let all: Vec<usize> = (0..walks.len()).collect();
            let holes = place_holes(&mut b, &walks, &all, &outer, wrap_axis)?;
            (outer, holes)
        }
    };

    // The remaining interior vertices, carried on the copy inside the
    // polygon. One inside on no copy has been relocated past the curve: it is
    // removed, the extreme case of §4.4.1's "remove a mesh vertex if it is
    // too close to the intersection curve" (`:563-564`).
    let mut interior = Vec::new();
    let mut removed_outside = 0usize;
    for &(v, base) in &inner {
        if seam_verts.contains(&v) {
            continue;
        }
        let q = match wrap_axis {
            None => b.inside(base, &outer, &holes).then_some(base),
            Some((ax, p)) => [0.0, -1.0, 1.0, -2.0, 2.0]
                .iter()
                .map(|k| shifted(base, ax, k * p))
                .find(|&q| b.inside(q, &outer, &holes)),
        };
        match q {
            Some(q) => interior.push(b.push(q, ChartVert::Mesh(v), false)),
            None => removed_outside += 1,
        }
    }

    // The Delaunay metric: arc length per unit u and v at the polygon centre.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in &b.pts {
        lo = [lo[0].min(q.x()), lo[1].min(q.y())];
        hi = [hi[0].max(q.x()), hi[1].max(q.y())];
    }
    let centre = Point2::new(0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]));
    let norm = |a: [f64; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let metric = match surface.derivs(centre) {
        Ok(d) if norm(d.su) > 0.0 && norm(d.sv) > 0.0 => [norm(d.su), norm(d.sv)],
        _ => [1.0, 1.0],
    };
    Ok(DomainChart {
        class,
        polygon: Patch {
            verts: b.pts,
            boundary: outer,
            holes,
        },
        back: b.back,
        degenerate: b.deg,
        interior,
        new_verts,
        old_max,
        budget,
        metric,
        removed_outside,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage4_construct::{
        directed_boundary, plan_keeps_boundary, rebuild_patch_domain, rebuild_patch_domain_gated,
        rebuild_patch_update, ConstructError,
    };
    use crate::stage4_splice::SplicePatch;
    use cad_primitives::Vector3;
    use cherchi_rs::Mesh;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn z() -> Vector3 {
        Vector3::new(0.0, 0.0, 1.0)
    }
    fn o() -> Point3 {
        Point3::new(0.0, 0.0, 0.0)
    }
    fn cone() -> Surface {
        Surface::Cone {
            apex: o(),
            axis_dir: z(),
            half_angle: PI / 6.0,
        }
    }
    fn sphere() -> Surface {
        Surface::Sphere {
            center: o(),
            radius: 1.0,
        }
    }
    fn torus() -> Surface {
        Surface::Torus {
            center: o(),
            axis_dir: z(),
            major_radius: 3.0,
            minor_radius: 1.0,
        }
    }

    /// A (u, v) grid patch on `s`: `nu` × `nv` cells over the given ranges,
    /// the u (v) columns closing on themselves when `wrap_u` (`wrap_v`), a row
    /// on a degenerate iso-line collapsed to one vertex. Cells for which
    /// `keep(i, j)` is false are left out. Triangles are CCW in (u, v).
    #[allow(clippy::too_many_arguments)]
    fn grid(
        s: &Surface,
        (u0, u1, nu): (f64, f64, usize),
        (v0, v1, nv): (f64, f64, usize),
        wrap_u: bool,
        wrap_v: bool,
        keep: impl Fn(usize, usize) -> bool,
    ) -> Mesh {
        let deg: Vec<f64> = s
            .domain()
            .degenerate
            .iter()
            .filter_map(|d| match *d {
                Degeneracy::VIso(v) => Some(v),
                Degeneracy::UIso(_) => None,
            })
            .collect();
        let cols = if wrap_u { nu } else { nu + 1 };
        let rows = if wrap_v { nv } else { nv + 1 };
        let mut verts = Vec::new();
        let mut index = vec![vec![0u32; rows]; cols];
        let mut pole: BTreeMap<usize, u32> = BTreeMap::new();
        for (j, row) in (0..rows).map(|j| (j, v0 + (v1 - v0) * j as f64 / nv as f64)) {
            for (i, col) in index.iter_mut().enumerate() {
                let u = u0 + (u1 - u0) * i as f64 / nu as f64;
                if deg.contains(&row) {
                    let k = *pole.entry(j).or_insert_with(|| {
                        let q = s.eval(Point2::new(u, row)).unwrap();
                        verts.push(Point3::new(q[0], q[1], q[2]));
                        (verts.len() - 1) as u32
                    });
                    col[j] = k;
                } else {
                    let q = s.eval(Point2::new(u, row)).unwrap();
                    verts.push(Point3::new(q[0], q[1], q[2]));
                    col[j] = (verts.len() - 1) as u32;
                }
            }
        }
        let at = |i: usize, j: usize| index[i % cols][j % rows];
        let mut tris = Vec::new();
        for i in 0..nu {
            for j in 0..nv {
                if !keep(i, j) {
                    continue;
                }
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                for t in [[a, b, c], [a, c, d]] {
                    if t[0] != t[1] && t[1] != t[2] && t[2] != t[0] {
                        tris.push(t);
                    }
                }
            }
        }
        // Drop unreferenced vertices? Not needed: the patch names its tris.
        Mesh { verts, tris }
    }

    /// The boundary loops of a triangle set, each in its walk order.
    fn loops(tris: &[[u32; 3]]) -> Vec<Vec<u32>> {
        let mut next: BTreeMap<u32, u32> = BTreeMap::new();
        for (u, v) in directed_boundary(tris) {
            assert!(next.insert(u, v).is_none(), "pinched boundary at {u}");
        }
        let mut out = Vec::new();
        while let Some((&s, _)) = next.iter().next() {
            let mut cyc = vec![s];
            let mut cur = next.remove(&s).unwrap();
            while cur != s {
                cyc.push(cur);
                cur = next.remove(&cur).unwrap();
            }
            out.push(cyc);
        }
        out
    }

    fn patch_of(s: Surface, mesh: &Mesh) -> SplicePatch {
        SplicePatch {
            cycles: loops(&mesh.tris),
            tris: (0..mesh.tris.len() as u32).collect(),
            surface: s,
        }
    }

    fn chart(s: Surface, mesh: &Mesh) -> Result<DomainChart, DomainRefusal> {
        let p = patch_of(s, mesh);
        domain_chart(
            &s,
            &mesh.verts,
            &p.cycles,
            &mesh.tris,
            &BTreeSet::new(),
            None,
        )
    }

    /// The rebuild keeps the patch's directed boundary, repeats no vertex,
    /// and every triangle faces the way the surface's old triangles did.
    fn rebuild_holds(s: Surface, mesh: &Mesh) -> crate::stage4_construct::PatchRebuild {
        let p = patch_of(s, mesh);
        // With the budget: rebuilt, or refused only by the d(T) budget.
        match rebuild_patch_domain(mesh, 0, &p, None, &BTreeSet::new()) {
            Ok(_) | Err(ConstructError::ChordDegradation { .. }) => {}
            Err(e) => panic!("domain rebuild refused: {e:?}"),
        }
        let plan = rebuild_patch_domain_gated(mesh, 0, &p, None, &BTreeSet::new(), false)
            .unwrap_or_else(|e| panic!("domain rebuild refused: {e:?}"));
        let old: Vec<[u32; 3]> = mesh.tris.clone();
        assert!(
            plan_keeps_boundary(&old, &plan.new_tris),
            "boundary changed: old {:?} new {:?} tris {:?}",
            directed_boundary(&old),
            directed_boundary(&plan.new_tris),
            plan.new_tris
        );
        let pos = |v: u32| -> [f64; 3] {
            if (v as usize) < mesh.verts.len() {
                mesh.verts[v as usize].as_array()
            } else {
                plan.new_verts[v as usize - mesh.verts.len()].as_array()
            }
        };
        for t in &plan.new_tris {
            let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
            let n = [
                (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
                (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
                (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
            ];
            let g = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let uv = s.inverse(g, None).unwrap();
            let nn = s.normal(uv).unwrap();
            assert!(
                n[0] * nn[0] + n[1] * nn[1] + n[2] * nn[2] > 0.0,
                "triangle {t:?} faces inward"
            );
        }
        assert!(plan
            .new_tri_bounds
            .iter()
            .all(|d| d.is_finite() && *d >= 0.0));
        plan
    }

    #[test]
    fn a_cone_cap_through_its_apex_vertex_charts_as_a_cap() {
        let m = grid(
            &cone(),
            (0.0, TAU, 16),
            (0.0, 1.0, 3),
            true,
            false,
            |_, _| true,
        );
        let c = chart(cone(), &m).expect("cap charts");
        assert_eq!(c.class, DomainClass::Cap);
        assert!(c.new_verts.is_empty(), "the apex vertex is reused");
        assert!(c.degenerate.iter().any(|&d| d));
        let plan = rebuild_holds(cone(), &m);
        // The apex vertex (the collapsed row) is still the fan's centre.
        let apex = m
            .verts
            .iter()
            .position(|p| p.as_array() == [0.0, 0.0, 0.0])
            .unwrap() as u32;
        assert!(plan.new_tris.iter().any(|t| t.contains(&apex)));
    }

    #[test]
    fn a_sphere_cap_round_a_pole_charts_as_a_cap() {
        let m = grid(
            &sphere(),
            (0.0, TAU, 16),
            (PI / 6.0, FRAC_PI_2, 4),
            true,
            false,
            |_, _| true,
        );
        let c = chart(sphere(), &m).expect("cap charts");
        assert_eq!(c.class, DomainClass::Cap);
        rebuild_holds(sphere(), &m);
        // The south cap too (the pole is the first row).
        let m = grid(
            &sphere(),
            (0.0, TAU, 16),
            (-FRAC_PI_2, -PI / 6.0, 4),
            true,
            false,
            |_, _| true,
        );
        assert_eq!(chart(sphere(), &m).unwrap().class, DomainClass::Cap);
        rebuild_holds(sphere(), &m);
    }

    #[test]
    fn a_sphere_band_charts_as_a_u_band() {
        let m = grid(
            &sphere(),
            (0.0, TAU, 16),
            (-PI / 6.0, PI / 6.0, 4),
            true,
            false,
            |_, _| true,
        );
        assert_eq!(chart(sphere(), &m).unwrap().class, DomainClass::UBand);
        rebuild_holds(sphere(), &m);
    }

    #[test]
    fn a_sphere_with_a_hole_charts_as_a_zone() {
        // Pole to pole, a 2×2-cell hole on the equator.
        let m = grid(
            &sphere(),
            (0.0, TAU, 16),
            (-FRAC_PI_2, FRAC_PI_2, 8),
            true,
            false,
            |i, j| !((7..9).contains(&i) && (3..5).contains(&j)),
        );
        let c = chart(sphere(), &m).expect("zone charts");
        assert_eq!(c.class, DomainClass::Zone);
        let plan = rebuild_holds(sphere(), &m);
        // Both pole vertices exist in the grid and are reused; only seam
        // points are new.
        assert_eq!(plan.new_verts.len(), c.new_verts.len());
    }

    #[test]
    fn torus_patches_chart_as_disc_and_both_bands() {
        // A disc straddling the tube angle's ±π branch.
        let m = grid(
            &torus(),
            (0.0, 1.0, 4),
            (2.5, 3.8, 4),
            false,
            false,
            |_, _| true,
        );
        assert_eq!(chart(torus(), &m).unwrap().class, DomainClass::Disc);
        rebuild_holds(torus(), &m);
        // A band round the axis.
        let m = grid(
            &torus(),
            (0.0, TAU, 24),
            (-0.5, 0.5, 3),
            true,
            false,
            |_, _| true,
        );
        assert_eq!(chart(torus(), &m).unwrap().class, DomainClass::UBand);
        rebuild_holds(torus(), &m);
        // A band round the tube.
        let m = grid(
            &torus(),
            (0.2, 1.0, 4),
            (-PI, PI, 16),
            false,
            true,
            |_, _| true,
        );
        assert_eq!(chart(torus(), &m).unwrap().class, DomainClass::VBand);
        rebuild_holds(torus(), &m);
    }

    #[test]
    fn an_interior_vertex_past_the_boundary_is_removed() {
        // A torus disc whose interior vertex at cell corner (1, 1) was
        // relocated past the patch's u = 0 edge.
        let mut m = grid(
            &torus(),
            (0.0, 1.0, 4),
            (2.5, 3.8, 4),
            false,
            false,
            |_, _| true,
        );
        let v = 5 + 1; // row 1, column 1 (5 columns per row)
        let q = torus().eval(Point2::new(-0.05, 2.5 + 1.3 / 4.0)).unwrap();
        m.verts[v] = Point3::new(q[0], q[1], q[2]);
        let c = chart(torus(), &m).expect("charts");
        assert_eq!(c.removed_outside, 1);
        let plan = rebuild_holds(torus(), &m);
        assert!(plan.dropped.contains(&(v as u32)));
    }

    #[test]
    fn a_torus_with_a_hole_is_refused_by_its_lattice() {
        let m = grid(
            &torus(),
            (0.0, TAU, 12),
            (-PI, PI, 8),
            true,
            true,
            |i, j| !(i == 3 && j == 2),
        );
        assert_eq!(chart(torus(), &m).unwrap_err(), DomainRefusal::Lattice);
    }

    #[test]
    fn a_cone_cap_without_an_apex_vertex_mints_one() {
        // The apex row left out: the innermost cells' rim at v = 0.25 closes
        // over the axis with one triangle fan replaced by a single triangle.
        let mut m = grid(
            &cone(),
            (0.0, TAU, 3),
            (0.25, 1.0, 1),
            true,
            false,
            |_, _| true,
        );
        // Close the inner 3-ring with one triangle winding round the apex.
        let inner: Vec<u32> = (0..3).map(|i| i as u32).collect();
        m.tris.push([inner[0], inner[2], inner[1]]);
        // Orientation of the closing triangle must match its neighbours:
        // flip if the boundary is no longer a single loop.
        if loops(&m.tris).len() != 1 {
            let last = m.tris.len() - 1;
            m.tris[last].swap(1, 2);
        }
        let c = chart(cone(), &m).expect("cap charts");
        assert_eq!(c.class, DomainClass::Cap);
        assert_eq!(c.new_verts.len(), 1, "the apex is minted");
        let plan = rebuild_holds(cone(), &m);
        assert_eq!(plan.new_verts[0].as_array(), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_update_routes_a_cone_cap_to_the_domain_chart() {
        let m = grid(
            &cone(),
            (0.0, TAU, 16),
            (0.0, 1.0, 3),
            true,
            false,
            |_, _| true,
        );
        let p = patch_of(cone(), &m);
        // The SurfaceChart rebuild refuses the cap ...
        assert!(matches!(
            crate::stage4_construct::rebuild_patch_planar_bounded_ex(
                &m,
                0,
                &p,
                None,
                &BTreeSet::new()
            ),
            Err(ConstructError::ThetaUnwrap { .. } | ConstructError::ApexInPatch { .. })
        ));
        // ... and the update takes it on the domain chart.
        let plan = rebuild_patch_update(&m, 0, &p, None, &BTreeSet::new()).expect("rebuilt");
        assert!(plan_keeps_boundary(&m.tris, &plan.new_tris));
    }

    #[test]
    fn a_degenerate_corner_takes_a_neighbouring_azimuth_for_dt() {
        let uv = [
            Point2::new(5.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(1.2, 1.0),
        ];
        let fixed = dt_corners(uv, [true, false, false]);
        assert_eq!(fixed[0], Point2::new(1.0, 0.0));
        assert_eq!(fixed[1..], uv[1..]);
    }
}

//! Shell contact census (spec `specs/untouched_shell_passthrough.md` §3).
//!
//! Which operand SHELLS does the other operand's exact surface reach? A
//! shell no intersection curve can reach is, per Yang 2025 §4.4.2, bounded
//! by its ORIGINAL curves alone and is restored as it was — kept or dropped
//! WHOLE by the op's in/out rule. A closed sphere/torus face is such a
//! shell on its own, and the Stage-1 mesh gives it no boundary edge to
//! segment along (P0030's `s6-curved-empty-cycles`), so the host
//! (kernel-v2) needs the verdict BEFORE the mesh pipeline, from the
//! operands' own Stage-1 meshes.
//!
//! "Untouched" must hold for the EXACT surfaces, not just the meshes: a
//! sub-sagitta graze leaves the meshes apart while the surfaces meet. The
//! gate is the paper's §4.3.1 conservative intersection check (Fig. 10a,
//! "triangles closer than 2dε are filtered"): a triangle pair is a CONTACT
//! when its distance is within both triangles' surface-deviation bounds
//! plus the YR24 weld band. A shell with no contact is CLEAR, and the
//! generalized winding number of the other operand's mesh at one of its
//! vertices says whether it lies inside or outside.

use cherchi_rs::predicates::{segment_intersects_triangle_3d, SegmentTriangleIntersection};

use crate::brep::BRep;
use crate::errors::YangError;
use crate::geom::Surface;
use cad_primitives::Point3;

/// One shell's verdict against the OTHER operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellContact {
    /// Some triangle of the shell lies within the contact band of the other
    /// operand's mesh: an intersection curve may reach it. The mesh pipeline
    /// owns it.
    Contact,
    /// The shell's exact surface is disjoint from the other operand's exact
    /// surface; `inside_other` is its side of the other operand.
    Clear {
        /// The shell lies inside the other operand's solid.
        inside_other: bool,
    },
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn add_scaled(a: V3, d: V3, t: f64) -> V3 {
    [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]]
}

/// Closest point on triangle `abc` to `p` (Ericson 2005 §5.1.5).
fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add_scaled(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add_scaled(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add_scaled(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add_scaled(add_scaled(a, ab, v), ac, w)
}

/// Least distance between segments `p1q1` and `p2q2` (Ericson §5.1.9).
fn segment_segment_distance(p1: V3, q1: V3, p2: V3, q2: V3) -> f64 {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let (s, t);
    if a <= f64::MIN_POSITIVE && e <= f64::MIN_POSITIVE {
        return norm(r);
    }
    if a <= f64::MIN_POSITIVE {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= f64::MIN_POSITIVE {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    norm(sub(add_scaled(p1, d1, s), add_scaled(p2, d2, t)))
}

/// Least distance between two triangles: 0 when an edge of either crosses
/// the other (EXACT predicate), else the least of the 6 vertex–triangle and
/// 9 edge–edge distances.
fn triangle_distance(t: [Point3; 3], s: [Point3; 3]) -> f64 {
    for (x, y) in [(t, s), (s, t)] {
        for i in 0..3 {
            if segment_intersects_triangle_3d(x[i], x[(i + 1) % 3], y[0], y[1], y[2])
                == SegmentTriangleIntersection::Intersects
            {
                return 0.0;
            }
        }
    }
    let ta = t.map(|p| p.as_array());
    let sa = s.map(|p| p.as_array());
    let mut best = f64::INFINITY;
    for (x, y) in [(ta, sa), (sa, ta)] {
        for &p in &x {
            best = best.min(norm(sub(p, closest_on_triangle(p, y[0], y[1], y[2]))));
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            best = best.min(segment_segment_distance(
                ta[i],
                ta[(i + 1) % 3],
                sa[j],
                sa[(j + 1) % 3],
            ));
        }
    }
    best
}

fn circumradius(a: V3, b: V3, c: V3) -> f64 {
    let (la, lb, lc) = (norm(sub(b, c)), norm(sub(c, a)), norm(sub(a, b)));
    let area2 = norm(cross(sub(b, a), sub(c, a)));
    if area2 <= 0.0 {
        // A degenerate triangle: its longest edge bounds every point of it.
        return la.max(lb).max(lc);
    }
    la * lb * lc / (2.0 * area2)
}

/// Bound on the distance between triangle `t` (vertices on `surface`) and
/// the exact surface patch it stands for (spec §3): the inscribed-cap
/// sagitta of the osculating sphere at the surface's curvature bound.
fn surface_deviation(surface: &Surface, t: [V3; 3]) -> f64 {
    let rho = circumradius(t[0], t[1], t[2]);
    let kappa = match *surface {
        Surface::Plane { .. } => return 0.0,
        Surface::Cylinder { radius, .. } => 1.0 / radius,
        Surface::Sphere { radius, .. } => 1.0 / radius,
        Surface::Torus {
            major_radius,
            minor_radius,
            ..
        } => (1.0 / minor_radius).max(1.0 / (major_radius - minor_radius)),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let o = apex.as_array();
            let ax = axis_dir.as_array();
            let al = norm(ax);
            let ax = [ax[0] / al, ax[1] / al, ax[2] / al];
            let rmin = t
                .iter()
                .map(|&p| {
                    let d = sub(p, o);
                    norm(add_scaled(d, ax, -dot(d, ax)))
                })
                .fold(f64::INFINITY, f64::min);
            half_angle.cos() / rmin
        }
    };
    if !(kappa.is_finite()) || rho * kappa >= 1.0 {
        return rho;
    }
    (1.0 - (1.0 - (rho * kappa).powi(2)).sqrt()) / kappa
}

/// Closest point of `surface` to `p` — the foot a subdivision midpoint is
/// moved to so the sub-triangle is again inscribed. `p` itself where the
/// foot is not unique (a point on a cylinder's / cone's axis, a torus's
/// tube-centre circle, a sphere's centre).
fn project_to_surface(surface: &Surface, p: V3) -> V3 {
    let unit = |v: V3| {
        let l = norm(v);
        (l > 0.0).then(|| [v[0] / l, v[1] / l, v[2] / l])
    };
    match *surface {
        Surface::Plane { .. } => p,
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            match unit(sub(p, c)) {
                Some(d) => add_scaled(c, d, radius),
                None => p,
            }
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let (a, Some(u)) = (axis_point.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, a);
            let foot = add_scaled(a, u, dot(q, u));
            match unit(sub(p, foot)) {
                Some(d) => add_scaled(foot, d, radius),
                None => p,
            }
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let (o, Some(u)) = (apex.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, o);
            let Some(w) = unit(add_scaled(q, u, -dot(q, u))) else {
                return p;
            };
            // The generator in p's half-plane, from the apex; the nappe is
            // the half-line, so a foot behind the apex is the apex.
            let g = [
                half_angle.cos() * u[0] + half_angle.sin() * w[0],
                half_angle.cos() * u[1] + half_angle.sin() * w[1],
                half_angle.cos() * u[2] + half_angle.sin() * w[2],
            ];
            add_scaled(o, g, dot(q, g).max(0.0))
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let (c, Some(u)) = (center.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, c);
            let Some(w) = unit(add_scaled(q, u, -dot(q, u))) else {
                return p;
            };
            let m = add_scaled(c, w, major_radius);
            match unit(sub(p, m)) {
                Some(d) => add_scaled(m, d, minor_radius),
                None => p,
            }
        }
    }
}

/// One side of a refined pair test: a triangle inscribed in `surf`.
#[derive(Clone, Copy)]
struct Piece {
    pts: [V3; 3],
    surf: Surface,
    dev: f64,
}

impl Piece {
    fn new(pts: [V3; 3], surf: Surface) -> Self {
        Piece {
            pts,
            surf,
            dev: surface_deviation(&surf, pts),
        }
    }

    /// The four inscribed children (Yang 2025 §4.1's refinement: a patch
    /// whose deviation exceeds the tolerance is split into four).
    fn split(&self) -> [Piece; 4] {
        let mid = |i: usize, j: usize| {
            let (a, b) = (self.pts[i], self.pts[j]);
            project_to_surface(
                &self.surf,
                [
                    (a[0] + b[0]) * 0.5,
                    (a[1] + b[1]) * 0.5,
                    (a[2] + b[2]) * 0.5,
                ],
            )
        };
        let (m01, m12, m20) = (mid(0, 1), mid(1, 2), mid(2, 0));
        let [p0, p1, p2] = self.pts;
        [
            Piece::new([p0, m01, m20], self.surf),
            Piece::new([m01, p1, m12], self.surf),
            Piece::new([m20, m12, p2], self.surf),
            Piece::new([m01, m12, m20], self.surf),
        ]
    }
}

/// Deepest refinement: each level shrinks a piece's deviation bound about
/// fourfold, so 16 levels take any Stage-1 triangle far below the weld band.
const MAX_REFINE_DEPTH: u32 = 16;

/// Can the exact surface patches over `t` and `s` come within `band` of each
/// other? The chord test is conservative (`dist ≤ dev_t + dev_s + band`); a
/// pair that fails it is refined — the piece with the larger deviation is
/// split into four inscribed children — until it either clears or its
/// deviations no longer exceed the band, where a remaining reach IS a
/// contact.
fn pieces_may_touch(t: Piece, s: Piece, band: f64, depth: u32) -> bool {
    let to_pts = |p: &Piece| p.pts.map(|v| Point3::new(v[0], v[1], v[2]));
    let d = triangle_distance(to_pts(&t), to_pts(&s));
    if d > t.dev + s.dev + band {
        return false;
    }
    if t.dev + s.dev <= band || depth == 0 {
        return true;
    }
    if t.dev >= s.dev {
        t.split()
            .into_iter()
            .any(|c| pieces_may_touch(c, s, band, depth - 1))
    } else {
        s.split()
            .into_iter()
            .any(|c| pieces_may_touch(t, c, band, depth - 1))
    }
}

/// One operand's Stage-1 triangles with their inflated boxes.
struct TriSet {
    pts: Vec<[Point3; 3]>,
    surf: Vec<Surface>,
    dev: Vec<f64>,
    lo: Vec<V3>,
    hi: Vec<V3>,
}

impl TriSet {
    fn new(brep: &BRep) -> Result<Self, YangError> {
        let mesh = brep.as_mesh();
        let tri_face = brep.tri_face();
        if tri_face.len() != mesh.tris.len() {
            // No Stage-1 face lineage (a `from_mesh` operand): no per-face
            // surface, so no deviation bound — refuse rather than guess.
            return Err(YangError::MalformedTopology(
                "shell contact census: operand has no Stage-1 face lineage".to_string(),
            ));
        }
        let mut out = TriSet {
            pts: Vec::with_capacity(mesh.tris.len()),
            surf: Vec::with_capacity(mesh.tris.len()),
            dev: Vec::with_capacity(mesh.tris.len()),
            lo: Vec::with_capacity(mesh.tris.len()),
            hi: Vec::with_capacity(mesh.tris.len()),
        };
        for (ti, tri) in mesh.tris.iter().enumerate() {
            let pts = tri.map(|v| mesh.verts[v as usize]);
            let arr = pts.map(|p| p.as_array());
            let surface = &brep.faces()[tri_face[ti] as usize].surface;
            let dev = surface_deviation(surface, arr);
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for p in arr {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            out.pts.push(pts);
            out.surf.push(*surface);
            out.dev.push(dev);
            out.lo.push(lo);
            out.hi.push(hi);
        }
        Ok(out)
    }
}

/// A median-split bounding-volume tree over a [`TriSet`]'s boxes.
struct Bvh {
    nodes: Vec<BvhNode>,
    order: Vec<u32>,
}

struct BvhNode {
    lo: V3,
    hi: V3,
    /// Leaf: `order[start..start + count]`; interior: `count == 0` and the
    /// children are `left` and `left + 1`.
    start: u32,
    count: u32,
    left: u32,
}

const LEAF: usize = 8;

impl Bvh {
    fn new(set: &TriSet) -> Self {
        let mut bvh = Bvh {
            nodes: Vec::new(),
            order: (0..set.pts.len() as u32).collect(),
        };
        if !set.pts.is_empty() {
            bvh.nodes.push(BvhNode {
                lo: [0.0; 3],
                hi: [0.0; 3],
                start: 0,
                count: 0,
                left: 0,
            });
            bvh.build(set, 0, 0, set.pts.len());
        }
        bvh
    }

    fn build(&mut self, set: &TriSet, node: usize, start: usize, end: usize) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &t in &self.order[start..end] {
            for k in 0..3 {
                lo[k] = lo[k].min(set.lo[t as usize][k]);
                hi[k] = hi[k].max(set.hi[t as usize][k]);
            }
        }
        self.nodes[node].lo = lo;
        self.nodes[node].hi = hi;
        if end - start <= LEAF {
            self.nodes[node].start = start as u32;
            self.nodes[node].count = (end - start) as u32;
            return;
        }
        let axis = (0..3)
            .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
            .unwrap_or(0);
        let mid = (start + end) / 2;
        let key = |t: &u32| set.lo[*t as usize][axis] + set.hi[*t as usize][axis];
        self.order[start..end]
            .select_nth_unstable_by(mid - start, |x, y| key(x).total_cmp(&key(y)));
        let left = self.nodes.len();
        for _ in 0..2 {
            self.nodes.push(BvhNode {
                lo: [0.0; 3],
                hi: [0.0; 3],
                start: 0,
                count: 0,
                left: 0,
            });
        }
        self.nodes[node].left = left as u32;
        self.build(set, left, start, mid);
        self.build(set, left + 1, mid, end);
    }

    /// Calls `visit` with every triangle whose box is within `pad` of
    /// `[lo, hi]`; stops (returning `true`) as soon as `visit` does.
    fn any_near(&self, lo: V3, hi: V3, pad: f64, mut visit: impl FnMut(u32) -> bool) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if (0..3).any(|k| node.lo[k] > hi[k] + pad || lo[k] > node.hi[k] + pad) {
                continue;
            }
            if node.count > 0 {
                let s = node.start as usize;
                for &t in &self.order[s..s + node.count as usize] {
                    if visit(t) {
                        return true;
                    }
                }
            } else {
                stack.push(node.left as usize);
                stack.push(node.left as usize + 1);
            }
        }
        false
    }
}

/// The generalized winding number of the closed mesh `brep` at `p`
/// (Van Oosterom–Strackee solid angles, Jacobson et al. 2013).
fn winding_number(brep: &BRep, p: V3) -> f64 {
    let mesh = brep.as_mesh();
    let mut omega = 0.0;
    for tri in &mesh.tris {
        let a = sub(mesh.verts[tri[0] as usize].as_array(), p);
        let b = sub(mesh.verts[tri[1] as usize].as_array(), p);
        let c = sub(mesh.verts[tri[2] as usize].as_array(), p);
        let (la, lb, lc) = (norm(a), norm(b), norm(c));
        let num = dot(a, cross(b, c));
        let den = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
        omega += 2.0 * num.atan2(den);
    }
    omega / (4.0 * std::f64::consts::PI)
}

/// The census (spec §3): for each listed shell of `a` (resp. `b`) — a
/// shell is a list of the operand's face indices — whether the other
/// operand's exact surface may reach it, and if not which side of the
/// other operand it lies on.
///
/// Errors: an operand without Stage-1 face lineage, a shell that owns no
/// Stage-1 triangle, or a winding number that is not within 0.25 of an
/// integer (an open or self-overlapping mesh) — loud, never a guess.
pub fn shell_contact_census(
    a: &BRep,
    b: &BRep,
    a_shells: &[Vec<u32>],
    b_shells: &[Vec<u32>],
) -> Result<(Vec<ShellContact>, Vec<ShellContact>), YangError> {
    let set_a = TriSet::new(a)?;
    let set_b = TriSet::new(b)?;
    let scale = set_a
        .lo
        .iter()
        .chain(set_a.hi.iter())
        .chain(set_b.lo.iter())
        .chain(set_b.hi.iter())
        .flat_map(|v| v.iter())
        .fold(0.0_f64, |m, &c| m.max(c.abs()));
    // The YR24 weld margin `union_operands_strictly_disjoint` uses: a pair
    // inside it is Stage-0's to weld, so it must read as a contact.
    let band = 2.0 * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    let bvh_a = Bvh::new(&set_a);
    let bvh_b = Bvh::new(&set_b);
    let max_dev_a = set_a.dev.iter().copied().fold(0.0, f64::max);
    let max_dev_b = set_b.dev.iter().copied().fold(0.0, f64::max);

    let verdicts = |own: &BRep,
                    own_set: &TriSet,
                    shells: &[Vec<u32>],
                    other: &BRep,
                    other_set: &TriSet,
                    other_bvh: &Bvh,
                    other_max_dev: f64|
     -> Result<Vec<ShellContact>, YangError> {
        let tri_face = own.tri_face();
        let mut out = Vec::with_capacity(shells.len());
        for shell in shells {
            let mut members: Vec<usize> = (0..tri_face.len())
                .filter(|&t| shell.contains(&tri_face[t]))
                .collect();
            if members.is_empty() {
                return Err(YangError::MalformedTopology(format!(
                    "shell contact census: shell {shell:?} owns no Stage-1 triangle"
                )));
            }
            members.sort_unstable();
            let contact = members.iter().any(|&t| {
                let pad = own_set.dev[t] + other_max_dev + band;
                other_bvh.any_near(own_set.lo[t], own_set.hi[t], pad, |s| {
                    let s = s as usize;
                    let reach = own_set.dev[t] + other_set.dev[s] + band;
                    if (0..3).any(|k| {
                        own_set.lo[t][k] > other_set.hi[s][k] + reach
                            || other_set.lo[s][k] > own_set.hi[t][k] + reach
                    }) {
                        return false;
                    }
                    let piece = |set: &TriSet, i: usize| Piece {
                        pts: set.pts[i].map(|p| p.as_array()),
                        surf: set.surf[i],
                        dev: set.dev[i],
                    };
                    let d = triangle_distance(own_set.pts[t], other_set.pts[s]);
                    let touch = d <= reach
                        && pieces_may_touch(
                            piece(own_set, t),
                            piece(other_set, s),
                            band,
                            MAX_REFINE_DEPTH,
                        );
                    if touch && std::env::var_os("YANG_SHELL_CONTACT_PROBE").is_some() {
                        eprintln!(
                            "[shell-contact] contact: tri {t} × tri {s} dist={d:.6e} reach={reach:.6e} \
                             (dev {:.6e} + {:.6e} + band {band:.3e})",
                            own_set.dev[t], other_set.dev[s]
                        );
                        eprintln!(
                            "[shell-contact]   own {:?}\n[shell-contact]   other {:?}",
                            own_set.pts[t].map(|p| p.as_array()),
                            other_set.pts[s].map(|p| p.as_array())
                        );
                    }
                    touch
                })
            });
            if contact {
                out.push(ShellContact::Contact);
                continue;
            }
            let probe = own_set.pts[members[0]][0].as_array();
            let w = winding_number(other, probe);
            let k = w.round();
            if (w - k).abs() > 0.25 || !(k == 0.0 || k == 1.0) {
                return Err(YangError::MalformedTopology(format!(
                    "shell contact census: winding number {w} of the other operand at a clear \
                     shell's vertex is not 0 or 1"
                )));
            }
            out.push(ShellContact::Clear {
                inside_other: k == 1.0,
            });
        }
        Ok(out)
    };
    let va = verdicts(a, &set_a, a_shells, b, &set_b, &bvh_b, max_dev_b)?;
    let vb = verdicts(b, &set_b, b_shells, a, &set_a, &bvh_a, max_dev_a)?;
    Ok((va, vb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn crossing_triangles_are_at_distance_zero() {
        let t = [p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0), p(0.0, 2.0, 0.0)];
        let s = [p(0.5, 0.5, -1.0), p(0.5, 0.5, 1.0), p(3.0, 3.0, 0.0)];
        assert_eq!(triangle_distance(t, s), 0.0);
    }

    #[test]
    fn parallel_triangles_report_their_gap() {
        let t = [p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0), p(0.0, 2.0, 0.0)];
        let s = [p(0.0, 0.0, 0.25), p(2.0, 0.0, 0.25), p(0.0, 2.0, 0.25)];
        assert!((triangle_distance(t, s) - 0.25).abs() < 1e-15);
    }

    #[test]
    fn skew_edges_report_their_gap() {
        // Two slivers whose closest features are interior edge points.
        let t = [p(-1.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 0.0, -1.0)];
        let s = [p(0.0, -1.0, 0.5), p(0.0, 1.0, 0.5), p(0.0, 0.0, 1.5)];
        assert!((triangle_distance(t, s) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn a_planar_triangle_deviates_by_nothing() {
        let s = Surface::Plane {
            normal: cad_primitives::Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        assert_eq!(
            surface_deviation(&s, [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            0.0
        );
    }

    #[test]
    fn a_sphere_triangle_deviates_by_its_cap_sagitta() {
        // An equilateral triangle inscribed in a unit sphere's circle of
        // latitude at height h: its circumcircle IS that latitude circle, and
        // the cap above it rises 1 − h — the bound must be exactly that.
        let h: f64 = 0.8;
        let r = (1.0 - h * h).sqrt();
        let v = |k: f64| {
            let a = k * 2.0 * std::f64::consts::PI / 3.0;
            [r * a.cos(), r * a.sin(), h]
        };
        let s = Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        let dev = surface_deviation(&s, [v(0.0), v(1.0), v(2.0)]);
        assert!((dev - (1.0 - h)).abs() < 1e-12, "dev {dev}");
    }
}

//! Ray-cast in/out classification (PR-CR-BL2) — Cherchi 2022 §5, step 2.
//!
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! Source: `code/booleans.cpp::computeInsideOut` / `findRayEndpoints` /
//! `pruneIntersectionsAndSortAlongRay` / `analyzeSortedIntersections` /
//! `perturbRayAndFindIntersTri` + helpers.
//!
//! For every BL1 patch, shoot an axis-aligned ray from a vertex of the
//! patch and count which OTHER input solids contain it: the ray is tested
//! against the prepped ORIGINAL input triangles (`soup.in_tris`, the C++
//! `arr_in_tris` — closed shells), the hits are sorted exactly along the
//! ray (each hit is an LPI implicit point compared with `lessThanOnX/Y/Z`),
//! and the NEAREST hit per input label decides in/out by triangle
//! orientation (back-face first → the patch is inside that input).
//!
//! ## Cycle A scope (this slice)
//!
//! - Ray origins from EXPLICIT non-border patch vertices only. A patch
//!   with no such vertex (all-implicit, or every explicit vertex on the
//!   border) returns the loud [`InsideOutError::NoExplicitRayOrigin`] —
//!   the C++ "generated ray" branch is Cycle B.
//! - Candidate triangles are brute-force: ALL `in_tris` are offered to the
//!   exact prune (the C++ octree is a pure acceleration structure feeding
//!   a superset; Cycle C adds it with a pruned ⊆ brute oracle).
//! - Vertex/edge ray-hits are resolved by `nextafter` ray perturbation
//!   over the hit element's incident input triangles, as in the C++.
//!
//! Port deviations (documented in `docs/yang_deviations.md`):
//! - Serial per-patch loop (crate rule #5; C++ is TBB-parallel).
//! - The C++ `btree_set` sort drops hits whose LPI points compare EQUAL
//!   on the ray axis; the port keeps the same set semantics explicitly.
//! - The C++ `std::exit` on a fully implicit patch is a typed error.
//! - Labels are `Vec<InputId>` sets, not `bitset<NBIT>`.

use std::collections::BTreeSet;

use cad_primitives::{Point2, Point3};
use indirect_predicates_sidecar_rs::{
    init_fpu, less_than_on_x, less_than_on_y, less_than_on_z, orient3d as ip_orient3d,
    AsGenericPoint, ExplicitPoint3D, ImplicitPoint3DLpi, Sign as IpSign,
};

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::gp_dispatch::{backing, gp, with_gp, Backing, Gp};
use crate::arrangements::soup::{ArrangementSoup, Label};
use crate::labeled_arrangement::InputId;
use crate::labeling::patches::Patches;
use crate::predicates::{
    max_component_in_triangle_normal, orient2d, orient3d, points_are_collinear_3d, Axis, Sign,
};

/// An axis-aligned in/out ray: `v0` = origin, `v1` = the far endpoint past
/// the global bbox (`max_coords + 0.5` in the C++). `seed_tri` is set by
/// the generated-ray branch (C++ `ray.tv`): the arrangement triangle whose
/// plane anchors the sort's discard test when the origin is synthetic.
#[derive(Debug, Clone)]
pub struct Ray {
    pub v0: Point3,
    pub v1: Point3,
    pub dir: Axis,
    pub seed_tri: Option<[u32; 3]>,
}

/// Loud failure surface — never silent (P9/P10).
#[derive(Debug, PartialEq, Eq)]
pub enum InsideOutError {
    /// The soup carries no prepped input triangles (`in_tris` empty) while
    /// patches exist — the arrangement predates the BL2 soup extension.
    MissingInputTris,
    /// A patch has no triangles (upstream BL1 invariant violation).
    EmptyPatch { patch: u32 },
    /// The patch has no usable ray origin: every vertex is either implicit
    /// (LPI/TPI) or sits on the patch border. The C++ "generated ray"
    /// fallback covers this — deferred to Cycle B.
    NoExplicitRayOrigin { patch: u32 },
    /// `orient3d(tri, ray.v1)` was Zero when classifying the nearest hit —
    /// the C++ asserts non-zero here.
    DegenerateOrientation { patch: u32, tri: u32 },
}

/// Port of `computeInsideOut` (booleans.cpp:621), serial: for each patch,
/// the sorted-ray-hit walk produces the patch's *inner label* — the set of
/// OTHER inputs that strictly contain it. Returns one `Label` per patch
/// (sorted, deduped; never contains the patch's own surface label).
pub fn compute_inside_out(
    soup: &ArrangementSoup,
    patches: &Patches,
) -> Result<Vec<Label>, InsideOutError> {
    if soup.in_tris.is_empty() && !soup.tris.is_empty() {
        return Err(InsideOutError::MissingInputTris);
    }
    init_fpu();

    // C++ max_coords = octree-root bbox max + 0.5 (over the input tris).
    let mut max_c = [f64::NEG_INFINITY; 3];
    for tri in &soup.in_tris {
        for &v in tri {
            let p = explicit_or_unreachable(&soup.verts[v as usize]);
            max_c = [
                max_c[0].max(p.x()),
                max_c[1].max(p.y()),
                max_c[2].max(p.z()),
            ];
        }
    }
    let max_coords = [max_c[0] + 0.5, max_c[1] + 0.5, max_c[2] + 0.5];

    let border: BTreeSet<u32> = patches.border_verts.iter().copied().collect();

    let mut inner_labels: Vec<Label> = Vec::with_capacity(patches.patches.len());
    for (pi, patch) in patches.patches.iter().enumerate() {
        let pi = pi as u32;
        if patch.is_empty() {
            return Err(InsideOutError::EmptyPatch { patch: pi });
        }
        let patch_surface_label = &soup.labels[patch[0] as usize];

        let ray = find_ray_endpoints(soup, patch, &border, max_coords, pi)?;
        let sorted = prune_intersections_and_sort_along_ray(soup, &ray, patch_surface_label)?;
        inner_labels.push(analyze_sorted_intersections(soup, &ray, &sorted, pi)?);
    }
    Ok(inner_labels)
}

/// Input-triangle vertices are always explicit (the welded input corners
/// seed the global vertex array before any implicit point is interned).
fn explicit_or_unreachable(c: &VertexCoords) -> Point3 {
    match c {
        VertexCoords::Explicit(p) => *p,
        other => unreachable!("in_tris vertex is implicit: {other:?}"),
    }
}

/// Port of `findRayEndpoints` (booleans.cpp:504): prefer an EXPLICIT,
/// non-border vertex of the patch (+X ray; all explicit operations are
/// faster). Otherwise the GENERATED-ray branch (cpp:525): for some patch
/// triangle, build a synthetic origin at the approximate centroid offset
/// −0.1 along the triangle's dominant-normal axis, then validate with
/// EXACT predicates that the segment v0→v1 straddles the implicit
/// triangle's plane and passes strictly inside it; the triangle becomes
/// `seed_tri` for the sort's discard test. If no triangle qualifies the
/// C++ exits ("requires rationals"); here a loud typed error.
fn find_ray_endpoints(
    soup: &ArrangementSoup,
    patch: &[u32],
    border: &BTreeSet<u32>,
    max_coords: [f64; 3],
    pi: u32,
) -> Result<Ray, InsideOutError> {
    for &t in patch {
        for &v in &soup.tris[t as usize] {
            if border.contains(&v) {
                continue;
            }
            if let VertexCoords::Explicit(p) = &soup.verts[v as usize] {
                return Ok(Ray {
                    v0: *p,
                    v1: Point3::new(max_coords[0], p.y(), p.z()),
                    dir: Axis::X,
                    seed_tri: None,
                });
            }
        }
    }

    // ----- generated-ray branch (booleans.cpp:525) -----
    for &t in patch {
        let tri = soup.tris[t as usize];
        let (Some(a), Some(b), Some(c)) = (
            approx_point(&soup.verts[tri[0] as usize]),
            approx_point(&soup.verts[tri[1] as usize]),
            approx_point(&soup.verts[tri[2] as usize]),
        ) else {
            continue;
        };
        // C++ `!misaligned(...)` gate on the approximate coordinates.
        if points_are_collinear_3d(a, b, c) {
            continue;
        }
        let dir = max_component_in_triangle_normal(a, b, c);
        let cen = [
            (a.x() + b.x() + c.x()) / 3.0,
            (a.y() + b.y() + c.y()) / 3.0,
            (a.z() + c.z() + b.z()) / 3.0,
        ];
        let k = match dir {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        };
        let mut v0c = cen;
        v0c[k] -= 0.1;
        let mut v1c = v0c;
        v1c[k] = max_coords[k];
        let v0 = Point3::new(v0c[0], v0c[1], v0c[2]);
        let v1 = Point3::new(v1c[0], v1c[1], v1c[2]);

        // EXACT validation against the (possibly implicit) triangle.
        let backs: [Backing; 3] = [
            backing(&soup.verts[tri[0] as usize]),
            backing(&soup.verts[tri[1] as usize]),
            backing(&soup.verts[tri[2] as usize]),
        ];
        let gps = [
            gp(&soup.verts[tri[0] as usize], &backs[0]),
            gp(&soup.verts[tri[1] as usize], &backs[1]),
            gp(&soup.verts[tri[2] as usize], &backs[2]),
        ];
        let v0e = ExplicitPoint3D::new(v0.x(), v0.y(), v0.z());
        let v1e = ExplicitPoint3D::new(v1.x(), v1.y(), v1.z());
        let orf = orient3d_gp3(&gps, &v0e);
        let ors = orient3d_gp3(&gps, &v1e);
        let straddles = (orf == IpSign::Negative && ors == IpSign::Positive)
            || (orf == IpSign::Positive && ors == IpSign::Negative);
        if !straddles {
            continue;
        }
        // checkIntersectionInsideTriangle3DImplPoints: the segment's line
        // passes strictly inside the implicit triangle.
        let same = |x: IpSign, y: IpSign, z: IpSign| {
            (x == IpSign::Positive && y == IpSign::Positive && z == IpSign::Positive)
                || (x == IpSign::Negative && y == IpSign::Negative && z == IpSign::Negative)
        };
        let o01 = orient3d_gp2_e2(&gps[0], &gps[1], &v0e, &v1e);
        let o12 = orient3d_gp2_e2(&gps[1], &gps[2], &v0e, &v1e);
        let o20 = orient3d_gp2_e2(&gps[2], &gps[0], &v0e, &v1e);
        if !same(o01, o12, o20) {
            continue;
        }
        return Ok(Ray {
            v0,
            v1,
            dir,
            seed_tri: Some(tri),
        });
    }
    Err(InsideOutError::NoExplicitRayOrigin { patch: pi })
}

/// Approximate f64 coordinates of any arrangement vertex (the C++
/// `getApproxXYZCoordinates`): exact for explicit points; f64 line-plane /
/// three-plane evaluation for LPI / TPI. `None` when the f64 denominator
/// vanishes (degenerate under approximation — the caller skips, as the
/// C++ `misaligned` gate does).
fn approx_point(c: &VertexCoords) -> Option<Point3> {
    let sub = |u: Point3, v: Point3| [u.x() - v.x(), u.y() - v.y(), u.z() - v.z()];
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let plane_of = |t: &[Point3; 3]| {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let d = dot(n, [t[0].x(), t[0].y(), t[0].z()]);
        (n, d)
    };
    match c {
        VertexCoords::Explicit(p) => Some(*p),
        VertexCoords::Lpi { line, plane } => {
            let (n, d) = plane_of(plane);
            let p = line[0];
            let q = line[1];
            let dir = sub(q, p);
            let den = dot(n, dir);
            if den == 0.0 || !den.is_finite() {
                return None;
            }
            let t = (d - dot(n, [p.x(), p.y(), p.z()])) / den;
            Some(Point3::new(
                p.x() + t * dir[0],
                p.y() + t * dir[1],
                p.z() + t * dir[2],
            ))
        }
        VertexCoords::Tpi { v, w, u } => {
            // Cramer's rule on the three plane equations n_i · x = d_i.
            let (n0, d0) = plane_of(v);
            let (n1, d1) = plane_of(w);
            let (n2, d2) = plane_of(u);
            let col = |i: usize| [n0[i], n1[i], n2[i]];
            let det3 = |c0: [f64; 3], c1: [f64; 3], c2: [f64; 3]| {
                c0[0] * (c1[1] * c2[2] - c1[2] * c2[1]) - c1[0] * (c0[1] * c2[2] - c0[2] * c2[1])
                    + c2[0] * (c0[1] * c1[2] - c0[2] * c1[1])
            };
            let d = [d0, d1, d2];
            let den = det3(col(0), col(1), col(2));
            if den == 0.0 || !den.is_finite() {
                return None;
            }
            let px = det3(d, col(1), col(2)) / den;
            let py = det3(col(0), d, col(2)) / den;
            let pz = det3(col(0), col(1), d) / den;
            Some(Point3::new(px, py, pz))
        }
    }
}

/// `orient3D(a, b, c, q)` with three `Gp` handles and one extra point.
fn orient3d_gp3(gps: &[Gp; 3], q: &impl AsGenericPoint) -> IpSign {
    let [a, b, c] = gps;
    with_gp!(ip_orient3d(a, b, c, q); a, b, c)
}

/// `orient3D(a, b, p, q)` with two `Gp` handles and two explicit points.
fn orient3d_gp2_e2(a: &Gp, b: &Gp, p: &ExplicitPoint3D, q: &ExplicitPoint3D) -> IpSign {
    with_gp!(ip_orient3d(a, b, p, q); a, b)
}

/// 2D classification of one input triangle against the ray (port of
/// `fast2DCheckIntersectionOnRay`): project onto the plane orthogonal to
/// the ray axis and run exact `orient2d` point-in-triangle on the ray line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntersInfo {
    Discard,
    NoInt,
    IntInV(u8),
    IntInEdge(u8), // 0 = edge01, 1 = edge12, 2 = edge20
    IntInTri,
}

fn project(p: Point3, dir: Axis) -> Point2 {
    match dir {
        Axis::X => Point2::new(p.y(), p.z()),
        Axis::Y => Point2::new(p.x(), p.z()),
        Axis::Z => Point2::new(p.x(), p.y()),
    }
}

fn fast_2d_check_intersection_on_ray(ray: &Ray, tv: [Point3; 3]) -> IntersInfo {
    let v = [
        project(tv[0], ray.dir),
        project(tv[1], ray.dir),
        project(tv[2], ray.dir),
    ];
    let q = project(ray.v1, ray.dir);

    let or01 = orient2d(v[0], v[1], q);
    let or12 = orient2d(v[1], v[2], q);
    let or20 = orient2d(v[2], v[0], q);
    let nonneg = |s: Sign| s != Sign::Negative;
    let nonpos = |s: Sign| s != Sign::Positive;

    if (nonneg(or01) && nonneg(or12) && nonneg(or20))
        || (nonpos(or01) && nonpos(or12) && nonpos(or20))
    {
        // Ray through a vertex?
        for (k, vk) in v.iter().enumerate() {
            if vk.x() == q.x() && vk.y() == q.y() {
                return IntersInfo::IntInV(k as u8);
            }
        }
        // Triangle coplanar with the ray?
        let z = |s: Sign| s == Sign::Zero;
        if (z(or01) && z(or12)) || (z(or12) && z(or20)) || (z(or20) && z(or01)) {
            return IntersInfo::Discard;
        }
        // Ray through an edge?
        if z(or01) {
            return IntersInfo::IntInEdge(0);
        }
        if z(or12) {
            return IntersInfo::IntInEdge(1);
        }
        if z(or20) {
            return IntersInfo::IntInEdge(2);
        }
        return IntersInfo::IntInTri;
    }
    IntersInfo::NoInt
}

/// Port of `checkIntersectionInsideTriangle3D`: does the segment v0→v1
/// pass strictly inside the triangle? (Exact `orient3d`, same-sign test.)
fn check_intersection_inside_triangle_3d(ray: &Ray, tv: [Point3; 3]) -> bool {
    let or01 = orient3d(tv[0], tv[1], ray.v0, ray.v1);
    let or12 = orient3d(tv[1], tv[2], ray.v0, ray.v1);
    let or20 = orient3d(tv[2], tv[0], ray.v0, ray.v1);
    (or01 == Sign::Positive && or12 == Sign::Positive && or20 == Sign::Positive)
        || (or01 == Sign::Negative && or12 == Sign::Negative && or20 == Sign::Negative)
}

/// Port of `perturbX/Y/ZRay`: `nextafter` the far endpoint's two
/// off-axis coordinates through the 8 (±,±) combinations.
fn perturb_ray(ray: &Ray, offset: u8) -> Ray {
    // (da, db) per offset for the two off-axis coordinates, C++ order:
    // +a, +a+b, +b, -a+b, -a, -a-b, -b, +a-b.
    const STEPS: [(i8, i8); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let (da, db) = STEPS[offset as usize];
    let bump = |x: f64, d: i8| match d {
        1 => f64::next_up(x),
        -1 => f64::next_down(x),
        _ => x,
    };
    let v1 = ray.v1;
    let v1 = match ray.dir {
        Axis::X => Point3::new(v1.x(), bump(v1.y(), da), bump(v1.z(), db)),
        Axis::Y => Point3::new(bump(v1.x(), da), v1.y(), bump(v1.z(), db)),
        Axis::Z => Point3::new(bump(v1.x(), da), bump(v1.y(), db), v1.z()),
    };
    Ray {
        v0: ray.v0,
        v1,
        dir: ray.dir,
        seed_tri: ray.seed_tri,
    }
}

/// Port of `perturbRayAndFindIntersTri`: try the 8 perturbed rays in
/// order; at the first offset where any candidate triangle is hit
/// strictly inside, sort that offset's hits along the ray and return the
/// nearest. Deviation N19: the C++ early-`break` interleaves hits from
/// DIFFERENT perturbed rays across offsets and sorts them with the last
/// ray; this port evaluates one offset fully — same intent, coherent ray.
fn perturb_ray_and_find_inters_tri(
    soup: &ArrangementSoup,
    ray: &Ray,
    tris_to_test: &[u32],
) -> Option<u32> {
    for offset in 0..8u8 {
        let p_ray = perturb_ray(ray, offset);
        let hits: Vec<u32> = tris_to_test
            .iter()
            .copied()
            .filter(|&t| check_intersection_inside_triangle_3d(&p_ray, in_tri_verts(soup, t)))
            .collect();
        if !hits.is_empty() {
            let sorted = sort_hits_along_ray(soup, &p_ray, &hits);
            return sorted.first().copied();
        }
    }
    None
}

fn in_tri_verts(soup: &ArrangementSoup, t: u32) -> [Point3; 3] {
    let tri = soup.in_tris[t as usize];
    [
        explicit_or_unreachable(&soup.verts[tri[0] as usize]),
        explicit_or_unreachable(&soup.verts[tri[1] as usize]),
        explicit_or_unreachable(&soup.verts[tri[2] as usize]),
    ]
}

/// Exact sort of hit triangles along the ray (port of
/// `sortIntersectedTrisAlong{X,Y,Z}`): each hit becomes the LPI point
/// ray∩plane(tri), ordered by the exact `lessThanOn{X,Y,Z}` comparator;
/// equal-keyed hits collapse to the first inserted (C++ `btree_set`
/// semantics); hits strictly before the ray origin are discarded.
fn sort_hits_along_ray(soup: &ArrangementSoup, ray: &Ray, hits: &[u32]) -> Vec<u32> {
    let e = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
    let ray_v0 = e(ray.v0);
    let ray_v1 = e(ray.v1);

    // Arena of the per-hit plane corners (kept alive for the LPI handles).
    let arena: Vec<[ExplicitPoint3D; 3]> = hits
        .iter()
        .map(|&t| {
            let tv = in_tri_verts(soup, t);
            [e(tv[0]), e(tv[1]), e(tv[2])]
        })
        .collect();
    let lpis: Vec<ImplicitPoint3DLpi<'_>> = arena
        .iter()
        .map(|tv| ImplicitPoint3DLpi::new(&ray_v0, &ray_v1, &tv[0], &tv[1], &tv[2]))
        .collect();

    let less = |a: &ImplicitPoint3DLpi<'_>, b: &ImplicitPoint3DLpi<'_>| match ray.dir {
        Axis::X => less_than_on_x(a, b),
        Axis::Y => less_than_on_y(a, b),
        Axis::Z => less_than_on_z(a, b),
    };

    // Ordered insert with set semantics (drop Equal keys).
    let mut order: Vec<usize> = Vec::with_capacity(hits.len());
    'insert: for i in 0..hits.len() {
        let mut at = order.len();
        for (slot, &j) in order.iter().enumerate() {
            match less(&lpis[i], &lpis[j]) {
                IpSign::Negative => {
                    at = slot;
                    break;
                }
                IpSign::Zero => continue 'insert, // duplicate key — drop
                _ => {}
            }
        }
        order.insert(at, i);
    }

    // Discard hits "behind" the ray start.
    //
    // Generated ray (`seed_tri` set): the C++ discards hits on the
    // OPPOSITE side of the seed triangle's plane from `v1` (the origin is
    // synthetic, slightly behind the patch plane, so the plane itself is
    // the start line).
    //
    // Explicit ray: discard hits before the origin along the axis, AND
    // hits at ray parameter exactly zero (deviation N20): the origin lies
    // ON another input's surface only in tangential configurations (a
    // transversal origin would sit on an intersection curve and be a
    // border vertex, which origin selection excludes), and a tangential
    // t=0 hit crosses nothing. The C++ keeps t=0 hits (`lessThanOnX(hit,
    // v0) < 0` discard only) and silently mislabels point-touch inputs.
    if let Some(seed) = &ray.seed_tri {
        let backs: [Backing; 3] = [
            backing(&soup.verts[seed[0] as usize]),
            backing(&soup.verts[seed[1] as usize]),
            backing(&soup.verts[seed[2] as usize]),
        ];
        let gps = [
            gp(&soup.verts[seed[0] as usize], &backs[0]),
            gp(&soup.verts[seed[1] as usize], &backs[1]),
            gp(&soup.verts[seed[2] as usize], &backs[2]),
        ];
        let s1 = orient3d_gp3(&gps, &ray_v1);
        debug_assert_ne!(
            s1,
            IpSign::Zero,
            "generated ray's v1 was straddle-checked against the seed plane"
        );
        let behind_seed_plane = |i: usize| {
            let sh = orient3d_gp3(&gps, &lpis[i]);
            (s1 == IpSign::Positive && sh == IpSign::Negative)
                || (s1 == IpSign::Negative && sh == IpSign::Positive)
        };
        order
            .into_iter()
            .skip_while(|&i| behind_seed_plane(i))
            .map(|i| hits[i])
            .collect()
    } else {
        let before_origin = |i: usize| {
            let s = match ray.dir {
                Axis::X => less_than_on_x(&lpis[i], &ray_v0),
                Axis::Y => less_than_on_y(&lpis[i], &ray_v0),
                Axis::Z => less_than_on_z(&lpis[i], &ray_v0),
            };
            s == IpSign::Negative || s == IpSign::Zero
        };
        order
            .into_iter()
            .skip_while(|&i| before_origin(i))
            .map(|i| hits[i])
            .collect()
    }
}

/// Port of `pruneIntersectionsAndSortAlongRay` (booleans.cpp:655) with a
/// brute-force candidate set (every prepped input triangle; the octree is
/// Cycle C). Vertex/edge hits resolve via ray perturbation over the hit
/// element's same-label incident triangles.
fn prune_intersections_and_sort_along_ray(
    soup: &ArrangementSoup,
    ray: &Ray,
    patch_surface_label: &Label,
) -> Result<Vec<u32>, InsideOutError> {
    let n = soup.in_tris.len() as u32;
    let mut visited = vec![false; n as usize];
    let mut inters: Vec<u32> = Vec::new();

    // Ray AABB pre-filter (the C++ `intersects_box(octree, rayAABB, ..)`
    // candidate query, brute-force): triangles whose AABB does not touch
    // the segment v0→v1's AABB are not candidates. This is semantically
    // LOAD-BEARING, not just acceleration — it excludes behind-the-origin
    // triangles, whose vertex/edge events would otherwise demand
    // perturbation winners that the sort then (correctly) discards.
    let ray_lo = [
        ray.v0.x().min(ray.v1.x()),
        ray.v0.y().min(ray.v1.y()),
        ray.v0.z().min(ray.v1.z()),
    ];
    let ray_hi = [
        ray.v0.x().max(ray.v1.x()),
        ray.v0.y().max(ray.v1.y()),
        ray.v0.z().max(ray.v1.z()),
    ];
    let in_ray_aabb = |tv: &[Point3; 3]| -> bool {
        (0..3).all(|k| {
            let c = |p: &Point3| match k {
                0 => p.x(),
                1 => p.y(),
                _ => p.z(),
            };
            let lo = c(&tv[0]).min(c(&tv[1])).min(c(&tv[2]));
            let hi = c(&tv[0]).max(c(&tv[1])).max(c(&tv[2]));
            lo <= ray_hi[k] && hi >= ray_lo[k]
        })
    };

    for t in 0..n {
        if visited[t as usize] {
            continue;
        }
        if !in_ray_aabb(&in_tri_verts(soup, t)) {
            continue;
        }
        visited[t as usize] = true;

        let tested_label = &soup.in_labels[t as usize];
        // Same input as the tested patch → skip (the patch's own shell).
        if tested_label
            .iter()
            .any(|id| patch_surface_label.contains(id))
        {
            continue;
        }

        let tv = in_tri_verts(soup, t);
        match fast_2d_check_intersection_on_ray(ray, tv) {
            IntersInfo::Discard | IntersInfo::NoInt => {}
            IntersInfo::IntInTri => inters.push(t),
            IntersInfo::IntInV(k) => {
                let v_id = soup.in_tris[t as usize][k as usize];
                let ring: Vec<u32> = (0..n)
                    .filter(|&t2| {
                        soup.in_labels[t2 as usize] == *tested_label
                            && soup.in_tris[t2 as usize].contains(&v_id)
                    })
                    .collect();
                for &t2 in &ring {
                    visited[t2 as usize] = true;
                }
                // C++ `if(winner_tri != -1)`: no clean perturbed crossing
                // means the event is tangential/grazing — it contributes no
                // parity crossing; skip it (adversary BUG-2/BUG-3 fix).
                if let Some(winner) = perturb_ray_and_find_inters_tri(soup, ray, &ring) {
                    inters.push(winner);
                }
            }
            IntersInfo::IntInEdge(k) => {
                let tri = soup.in_tris[t as usize];
                let (a, b) = match k {
                    0 => (tri[0], tri[1]),
                    1 => (tri[1], tri[2]),
                    _ => (tri[2], tri[0]),
                };
                let edge_tris: Vec<u32> = (0..n)
                    .filter(|&t2| {
                        soup.in_labels[t2 as usize] == *tested_label
                            && soup.in_tris[t2 as usize].contains(&a)
                            && soup.in_tris[t2 as usize].contains(&b)
                    })
                    .collect();
                debug_assert_eq!(
                    edge_tris.len(),
                    2,
                    "edge ({a},{b}) of a closed manifold input must have 2 tris"
                );
                for &t2 in &edge_tris {
                    visited[t2 as usize] = true;
                }
                // Same winner-skip semantics as the vertex case above.
                if let Some(winner) = perturb_ray_and_find_inters_tri(soup, ray, &edge_tris) {
                    inters.push(winner);
                }
            }
        }
    }

    Ok(sort_hits_along_ray(soup, ray, &inters))
}

/// Port of `analyzeSortedIntersections` (booleans.cpp:747): walking the
/// hits nearest-first, the FIRST hit of each input label decides in/out —
/// `orient3d(tri, ray.v1) == Negative` means the ray exits through a
/// back-face, so the origin is INSIDE that input.
fn analyze_sorted_intersections(
    soup: &ArrangementSoup,
    ray: &Ray,
    sorted: &[u32],
    pi: u32,
) -> Result<Label, InsideOutError> {
    let mut visited: BTreeSet<InputId> = BTreeSet::new();
    let mut inner: BTreeSet<InputId> = BTreeSet::new();

    for &t in sorted {
        let label = &soup.in_labels[t as usize];
        if label.iter().all(|id| visited.contains(id)) {
            continue;
        }
        let tv = in_tri_verts(soup, t);
        match orient3d(tv[0], tv[1], tv[2], ray.v1) {
            Sign::Negative => {
                inner.extend(label.iter().copied());
            }
            Sign::Positive => {}
            Sign::Zero => return Err(InsideOutError::DegenerateOrientation { patch: pi, tri: t }),
        }
        visited.extend(label.iter().copied());
    }
    Ok(inner.into_iter().collect())
}

// =========================================================================
// RED oracle tests (PR-CR-BL2 Cycle A)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::soup::mesh_arrangement;
    use crate::labeled_arrangement::InputId;
    use crate::labeling::patches::compute_all_patches;
    use dashu::rational::RBig;
    use std::collections::BTreeSet;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ----- fixtures (the BL1 suite's geometry) ----------------------------

    fn cube(
        ox: f64,
        oy: f64,
        oz: f64,
        s: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let p = |x: f64, y: f64, z: f64| (ox + x * s, oy + y * s, oz + z * s);
        let corners = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    fn concat(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    fn arrange(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> ArrangementSoup {
        crate::arrangements::require_ffi_shim();
        let (coords, tris, labels) = concat(s0, s1);
        mesh_arrangement(&coords, &tris, &labels).expect("arrangement")
    }

    // ----- independent coordinate resolution (oracle-side) ----------------
    // Pure-dashu line-plane intersection for Lpi; trilinear plane-plane-plane
    // is not needed by these fixtures (no TPI on axis-perpendicular cuts).

    fn to_r(x: f64) -> RBig {
        RBig::simplest_from_f64(x).expect("finite")
    }

    fn r3(p: Point3) -> [RBig; 3] {
        [to_r(p.x()), to_r(p.y()), to_r(p.z())]
    }

    fn approx_coords(c: &VertexCoords) -> [f64; 3] {
        match c {
            VertexCoords::Explicit(p) => [p.x(), p.y(), p.z()],
            VertexCoords::Lpi { line, plane } => {
                let [p, q] = [r3(line[0]), r3(line[1])];
                let [a, b, c3] = [r3(plane[0]), r3(plane[1]), r3(plane[2])];
                let sub =
                    |u: &[RBig; 3], v: &[RBig; 3]| [&u[0] - &v[0], &u[1] - &v[1], &u[2] - &v[2]];
                let cross = |u: &[RBig; 3], v: &[RBig; 3]| {
                    [
                        &u[1] * &v[2] - &u[2] * &v[1],
                        &u[2] * &v[0] - &u[0] * &v[2],
                        &u[0] * &v[1] - &u[1] * &v[0],
                    ]
                };
                let dot =
                    |u: &[RBig; 3], v: &[RBig; 3]| &u[0] * &v[0] + &u[1] * &v[1] + &u[2] * &v[2];
                let n = cross(&sub(&b, &a), &sub(&c3, &a));
                let d = dot(&n, &sub(&q, &p));
                assert!(d != RBig::ZERO, "oracle: line parallel to plane");
                let t = dot(&n, &sub(&a, &p)) / d;
                let lerp = |i: usize| &p[i] + &t * (&q[i] - &p[i]);
                [
                    lerp(0).to_f64().value(),
                    lerp(1).to_f64().value(),
                    lerp(2).to_f64().value(),
                ]
            }
            VertexCoords::Tpi { .. } => {
                panic!("oracle: fixtures must not produce TPI vertices")
            }
        }
    }

    /// Scaled open-AABB of one input's prepped triangles (truth source for
    /// "inside" on box fixtures — convex, axis-aligned).
    fn input_aabb(soup: &ArrangementSoup, label: InputId) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (t, tri) in soup.in_tris.iter().enumerate() {
            if !soup.in_labels[t].contains(&label) {
                continue;
            }
            for &v in tri {
                let p = approx_coords(&soup.verts[v as usize]);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }

    /// Truth: does this patch sit strictly inside `label`'s box? Decided by
    /// the patch's triangle centroids vs the input's scaled AABB (strict,
    /// with a relative margin so border-loop vertices never flip it).
    fn patch_inside_box(soup: &ArrangementSoup, patch: &[u32], label: InputId) -> bool {
        let (lo, hi) = input_aabb(soup, label);
        let eps: f64 = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max) * 1e-9;
        patch.iter().all(|&t| {
            let tri = soup.tris[t as usize];
            let mut c = [0.0; 3];
            for &v in &tri {
                let p = approx_coords(&soup.verts[v as usize]);
                for k in 0..3 {
                    c[k] += p[k] / 3.0;
                }
            }
            (0..3).all(|k| c[k] > lo[k] + eps && c[k] < hi[k] - eps)
        })
    }

    fn canonical(l: &Label) -> Label {
        let mut l = l.clone();
        l.sort_unstable();
        l
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #1 — corner-overlapping cubes: each patch's inner label
    // matches the geometric truth (centroids strictly inside the other
    // box ⇔ inner = {other}; else inner = ∅). Exercises ray casting,
    // exact sorting, and (axis-aligned fixture) perturbation paths.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn cut_boxes_inner_labels_match_geometry() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(
            inner.len(),
            patches.patches.len(),
            "one inner label per patch"
        );
        let mut inside_seen = 0;
        for (pi, patch) in patches.patches.iter().enumerate() {
            let own = canonical(&soup.labels[patch[0] as usize]);
            let other = if own == vec![A] { B } else { A };
            let expect: Label = if patch_inside_box(&soup, patch, other) {
                inside_seen += 1;
                vec![other]
            } else {
                vec![]
            };
            assert_eq!(
                canonical(&inner[pi]),
                expect,
                "patch {pi} (own label {own:?}): inner label vs geometric truth"
            );
        }
        assert!(
            inside_seen >= 2,
            "fixture sanity: at least one inside patch per solid (got {inside_seen})"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #2 — enclosed cube: B's whole shell is inside A; A's shell
    // is outside B.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn enclosed_cube_is_inside_outer() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(0.5, 0.5, 0.5, 1.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(patches.patches.len(), 2);
        for (pi, patch) in patches.patches.iter().enumerate() {
            let own = canonical(&soup.labels[patch[0] as usize]);
            let expect: Label = if own == vec![B] { vec![A] } else { vec![] };
            assert_eq!(canonical(&inner[pi]), expect, "patch {pi} of {own:?}");
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #3 — disjoint cubes: nothing is inside anything.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn disjoint_cubes_are_all_outside() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 1.0, A), cube(5.0, 5.0, 5.0, 1.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(inner.len(), 2);
        assert!(
            inner.iter().all(|l| l.is_empty()),
            "disjoint solids: every inner label empty, got {inner:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #4 — structural invariants on every fixture: inner labels
    // never contain the patch's own surface label; inner labels are
    // sorted + deduped; deterministic across runs.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn structural_invariants_and_determinism() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner1 = compute_inside_out(&soup, &patches).expect("inside_out");
        let inner2 = compute_inside_out(&soup, &patches).expect("inside_out");
        assert_eq!(inner1, inner2, "same input → identical inner labels");
        assert_eq!(inner1.len(), patches.patches.len());

        for (pi, patch) in patches.patches.iter().enumerate() {
            let own: BTreeSet<InputId> = soup.labels[patch[0] as usize].iter().copied().collect();
            for id in &inner1[pi] {
                assert!(
                    !own.contains(id),
                    "patch {pi}: inner label contains own surface label {id:?}"
                );
            }
            let mut sorted = inner1[pi].clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                inner1[pi], sorted,
                "patch {pi}: inner label sorted + deduped"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #5 — loud error paths: a soup without prepped input tris
    // must not silently classify everything as outside.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn missing_input_tris_is_loud() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let broken = ArrangementSoup {
            in_tris: Vec::new(),
            in_labels: Vec::new(),
            ..soup
        };
        match compute_inside_out(&broken, &patches) {
            Err(InsideOutError::MissingInputTris) => {}
            other => panic!("expected MissingInputTris, got {other:?}"),
        }
    }

    /// Axis-aligned box with per-axis sizes (through-cut fixtures).
    fn boxx(
        ox: f64,
        oy: f64,
        oz: f64,
        sx: f64,
        sy: f64,
        sz: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let p = |x: f64, y: f64, z: f64| (ox + x * sx, oy + y * sy, oz + z * sz);
        let corners = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #6 (Cycle B) — through-cut: square peg B pierces cube A
    // straight through along z. B's middle band patch is bounded
    // entirely by the two intersection loops (no explicit non-border
    // vertex) → requires the C++ "generated ray" branch. Expected:
    // exactly one B patch inside A (the band, confirmed geometrically),
    // everything else outside.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn through_cut_band_uses_generated_ray() {
        let soup = arrange(
            cube(0.0, 0.0, 0.0, 2.0, A),
            boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
        );
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out (Cycle B)");

        // Symmetric geometric truth: B's middle band is inside A, and the
        // two square disc regions of A's top/bottom faces (where the peg
        // passes through) are inside B. (The RED draft wrongly asserted
        // ALL A patches outside B — the discs are genuinely inside.)
        let mut a_inside = 0;
        let mut b_inside = 0;
        for (pi, patch) in patches.patches.iter().enumerate() {
            let own = canonical(&soup.labels[patch[0] as usize]);
            let other = if own == vec![A] { B } else { A };
            let geometrically_inside = patch_inside_box(&soup, patch, other);
            let expect: Label = if geometrically_inside {
                vec![other]
            } else {
                vec![]
            };
            assert_eq!(
                canonical(&inner[pi]),
                expect,
                "patch {pi} (own {own:?}): inner label vs geometric truth"
            );
            if geometrically_inside {
                if own == vec![A] {
                    a_inside += 1;
                } else {
                    b_inside += 1;
                }
            }
        }
        assert_eq!(b_inside, 1, "exactly ONE B patch (the band) lies inside A");
        assert_eq!(a_inside, 2, "A's two through-hole discs lie inside B");
        let count = |l: InputId| {
            patches
                .patches
                .iter()
                .filter(|p| canonical(&soup.labels[p[0] as usize]) == vec![l])
                .count()
        };
        assert_eq!(count(B), 3, "B splits into below / band / above");
        assert_eq!(count(A), 3, "A splits into shell + two discs");
    }
}

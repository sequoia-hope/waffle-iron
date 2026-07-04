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
//! - Vertex/edge ray-hits are resolved by `nextafter` ray perturbation
//!   over the hit element's incident input triangles, as in the C++.
//!
//! ## Cycle C — octree candidate producer
//!
//! Candidate triangles come from a [`TriOctree`] built once over the
//! `in_tris` AABBs and queried per-ray with the ray's (degenerate) AABB —
//! the C++ `cinolib::Octree` + `intersects_box` walk (booleans.cpp:580).
//! DESIGN INVARIANT: the octree is a pure SUPERSET producer. The exact
//! per-triangle `in_ray_aabb` filter inside the prune is the semantically
//! load-bearing check (it excludes behind-the-origin triangles — see the
//! comment at the filter) and is applied to EVERY candidate
//! unconditionally, so the octree's internal parameters cannot affect
//! labeling correctness. The brute scan survives as the test-only
//! [`compute_inside_out_brute`] structural diff target.
//!
//! Port deviations (documented in `docs/yang_deviations.md`):
//! - Serial per-patch loop (crate rule #5; C++ is TBB-parallel).
//! - The C++ `btree_set` sort drops hits whose LPI points compare EQUAL
//!   on the ray axis; the port keeps the same set semantics explicitly.
//! - The C++ `std::exit` on a fully implicit patch is a typed error.
//! - Labels are `Vec<InputId>` sets, not `bitset<NBIT>`.

use std::collections::BTreeSet;

use cad_primitives::{Point2, Point3};

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::gp_dispatch::to_generic;
use crate::arrangements::soup::{ArrangementSoup, Label};
use crate::labeled_arrangement::InputId;
use crate::labeling::octree::TriOctree;
use crate::labeling::patches::Patches;
use crate::predicates::indirect::{
    less_than_on_x_indirect, less_than_on_y_indirect, less_than_on_z_indirect, orient3d_indirect,
    GenericPoint3D, Sign as IpSign,
};
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
    ///
    /// Since PR-KV4-F1 this is no longer terminal for the pipeline: when
    /// both f64 origin strategies fail, [`rational_ray_inner_label`]
    /// classifies the patch in exact rational arithmetic (the branch the
    /// C++ names "requires exact rationals" and exits on,
    /// booleans.cpp:578). The variant survives for the diagnostic paths.
    NoExplicitRayOrigin { patch: u32 },
    /// The rational-ray fallback could not classify the patch: every
    /// patch triangle has an undefined exact point (degenerate implicit
    /// construction), or all three axis rays graze input geometry exactly
    /// (codimension-2; not reachable from a generic arrangement).
    RationalRayDegenerate { patch: u32 },
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
    // Candidate producer (Cycle C): one octree over the prepped input
    // triangles, queried per-ray with the ray's (degenerate) AABB — the
    // C++ `cinolib::Octree` + `intersects_box` walk. The candidate set
    // only needs to be a SUPERSET of {t : tri_AABB ∩ ray_AABB ≠ ∅} — the
    // prune applies the exact `in_ray_aabb` filter to every candidate
    // regardless, so octree parameters cannot affect correctness (see
    // `octree.rs` module docs; the brute path survives as the test-only
    // `compute_inside_out_brute` diff target).
    let octree = TriOctree::build(soup);
    compute_inside_out_with(soup, patches, |ray| {
        let (lo, hi) = ray_aabb(ray);
        octree.query_aabb(lo, hi)
    })
}

/// `compute_inside_out` parameterized over the per-ray candidate producer
/// (`candidates_for(ray)` must return ids ASCENDING — visit order is part
/// of the prune's duplicate-sort-key semantics). Production uses the
/// octree; the `#[cfg(test)]` brute path diffs against it structurally.
fn compute_inside_out_with<F>(
    soup: &ArrangementSoup,
    patches: &Patches,
    candidates_for: F,
) -> Result<Vec<Label>, InsideOutError>
where
    F: Fn(&Ray) -> Vec<u32>,
{
    if soup.in_tris.is_empty() && !soup.tris.is_empty() {
        return Err(InsideOutError::MissingInputTris);
    }

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

        match find_ray_endpoints(soup, patch, &border, max_coords, pi) {
            Ok(ray) => {
                let candidates = candidates_for(&ray);
                let sorted = prune_intersections_and_sort_along_ray(
                    soup,
                    &ray,
                    patch_surface_label,
                    &candidates,
                )?;
                let label = analyze_sorted_intersections(soup, &ray, &sorted, pi)?;
                // KV9-F1 diagnosis probe (read-only, env-gated): per-patch
                // ray + sorted-hit + verdict census.
                if std::env::var_os("CHERCHI_INOUT_PROBE").is_some() {
                    eprintln!(
                        "[inout-probe] patch {pi} ({} tris, surface {:?}): ray {:?} \
                         from ({},{},{}) hits {} -> inner {:?}",
                        patch.len(),
                        patch_surface_label,
                        ray.dir,
                        ray.v0.x(),
                        ray.v0.y(),
                        ray.v0.z(),
                        sorted.len(),
                        label
                    );
                    for &ht in sorted.iter().take(8) {
                        let t = &soup.in_tris[ht as usize];
                        let l = &soup.in_labels[ht as usize];
                        let p0 = explicit_or_unreachable(&soup.verts[t[0] as usize]);
                        eprintln!(
                            "    hit in_tri {ht} label {l:?} v0 ({},{},{})",
                            p0.x(),
                            p0.y(),
                            p0.z()
                        );
                    }
                }
                inner_labels.push(label);
            }
            // KV4-F1: both f64 origin strategies failed (a fully-implicit
            // or sub-f64-resolution needle patch) — classify in exact
            // rational arithmetic, the branch the C++ exits on.
            Err(InsideOutError::NoExplicitRayOrigin { .. }) => {
                inner_labels.push(rational_ray_inner_label(
                    soup,
                    patch,
                    patch_surface_label,
                    pi,
                )?);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(inner_labels)
}

/// Test-only brute-candidate path (every input triangle offered to the
/// exact prune) — the permanent structural diff target for the octree
/// production path: the two must produce IDENTICAL labels on every input.
#[cfg(test)]
pub(crate) fn compute_inside_out_brute(
    soup: &ArrangementSoup,
    patches: &Patches,
) -> Result<Vec<Label>, InsideOutError> {
    let n = soup.in_tris.len() as u32;
    compute_inside_out_with(soup, patches, |_| (0..n).collect())
}

/// AABB of the ray segment `v0 → v1` (degenerate — zero thickness — in the
/// two off-axis coordinates). Shared by the prune's exact per-triangle
/// filter and the octree candidate query so the two can never drift.
pub(crate) fn ray_aabb(ray: &Ray) -> ([f64; 3], [f64; 3]) {
    (
        [
            ray.v0.x().min(ray.v1.x()),
            ray.v0.y().min(ray.v1.y()),
            ray.v0.z().min(ray.v1.z()),
        ],
        [
            ray.v0.x().max(ray.v1.x()),
            ray.v0.y().max(ray.v1.y()),
            ray.v0.z().max(ray.v1.z()),
        ],
    )
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
            // PR-YR24 fidelity nit: sum in a,b,c operand order like the C++
            // reference (f64 addition is not associative — the previous
            // a,c,b order could differ in the last ulp on needle triangles).
            (a.z() + b.z() + c.z()) / 3.0,
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
        let gps = [
            to_generic(&soup.verts[tri[0] as usize]),
            to_generic(&soup.verts[tri[1] as usize]),
            to_generic(&soup.verts[tri[2] as usize]),
        ];
        let v0e = GenericPoint3D::explicit(v0);
        let v1e = GenericPoint3D::explicit(v1);
        // M7c mirror note: the native orient3d uses the Shewchuk convention,
        // which is the MIRROR of the former FFI's sign. Both uses here are
        // sign-RELATIVE (straddle = orf/ors strictly opposite; below: all
        // three o01/o12/o20 share one non-zero sign), so a global sign flip
        // leaves the verdicts unchanged — no `.flipped()` needed.
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
    if std::env::var_os("KV4F1_PROBE").is_some() {
        eprintln!(
            "[kv4f1-probe] patch {pi}: {} tris, no f64 ray origin (rational fallback engages). \
             Vertex census:",
            patch.len()
        );
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        for &t in patch {
            for &v in &soup.tris[t as usize] {
                if seen.insert(v) {
                    let kind = match &soup.verts[v as usize] {
                        VertexCoords::Explicit(p) => format!("Explicit {p:?}"),
                        other => format!("{other:?}"),
                    };
                    eprintln!("[kv4f1-probe]   v{v} border={} {kind}", border.contains(&v));
                }
            }
        }
        for &t in patch {
            let tri = soup.tris[t as usize];
            let pts = (
                approx_point(&soup.verts[tri[0] as usize]),
                approx_point(&soup.verts[tri[1] as usize]),
                approx_point(&soup.verts[tri[2] as usize]),
            );
            let why = match pts {
                (Some(a), Some(b), Some(c)) => {
                    if points_are_collinear_3d(a, b, c) {
                        "approx-collinear".to_string()
                    } else {
                        "straddle/inside validation failed".to_string()
                    }
                }
                _ => "approx denominator vanished".to_string(),
            };
            eprintln!("[kv4f1-probe]   tri {t} {tri:?}: {why}");
        }
    }
    Err(InsideOutError::NoExplicitRayOrigin { patch: pi })
}

/// KV4-F1: classify one patch's inner label in EXACT rational arithmetic —
/// the branch the C++ acknowledges and exits on (booleans.cpp:578:
/// "a fully implicit patch that requires exact rationals for evaluation.
/// This version of the code does not support rationals"; deviation N21 in
/// `docs/yang_deviations.md`).
///
/// Reached only when BOTH f64 origin strategies of [`find_ray_endpoints`]
/// fail. The canonical trigger is a sub-f64-resolution NEEDLE patch: an
/// input edge pierces a triangle femto-close to its corner (f64-crooked
/// chained inputs make the exact arrangement mint an intersection point
/// ~1e-17 from an existing vertex — the F0016 corpus class), so every
/// explicit vertex is on the patch border and the approximated triangle is
/// too thin for any f64 segment to pass strictly inside it.
///
/// Method — the same nearest-hit walk as the f64 path, in rationals:
/// 1. Origin: the exact centroid of a patch triangle (explicit coords are
///    exact f64→rational; implicit coords come from the exact lambda
///    tier). An arrangement triangle has strictly positive exact area, so
///    its centroid is strictly interior — never a border vertex, which is
///    what made the f64 explicit-origin rule fail.
/// 2. Axis ray +e_k (k = X then Y then Z on graze-retry): for every input
///    triangle of OTHER labels, the plane crossing parameter `t`, the
///    strictly-inside test, and the hit ordering are computed in RBig.
///    Exact grazes (hit on a vertex/edge, origin on a candidate's plane)
///    retry the next axis instead of perturbing — with a rational origin a
///    graze on all three axes is codimension-2 and gets the loud
///    [`InsideOutError::RationalRayDegenerate`].
/// 3. Nearest hit per label decides: with the hit ON the triangle plane
///    and the far end beyond it along +e_k, the f64 rule
///    `orient3d(tv, ray.v1) == Negative → inner` reduces exactly to
///    `n_k > 0` for `n = (b−a)×(c−a)` (pinned by the
///    `rational_orientation_convention_matches_f64_path` oracle).
///
/// Candidate scan: ALL `in_tris` (no octree) — the fallback fires only on
/// pathological patches (corpus: a handful per model), and the exact walk
/// is the semantic authority anyway; a brute scan is provably a superset.
///
/// Set semantics mirror `sort_hits_along_ray`: hits with an exactly equal
/// ray parameter collapse to the first in ascending triangle order, and
/// `t ≤ 0` hits are discarded (deviation N20's t=0 exclusion included).
fn rational_ray_inner_label(
    soup: &ArrangementSoup,
    patch: &[u32],
    patch_surface_label: &Label,
    pi: u32,
) -> Result<Label, InsideOutError> {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // f64 → exact rational (total for finite f64).
    fn rbe(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }
    type R3 = [RBig; 3];
    fn sub(a: &R3, b: &R3) -> R3 {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    }
    fn cross(a: &R3, b: &R3) -> R3 {
        [
            &a[1] * &b[2] - &a[2] * &b[1],
            &a[2] * &b[0] - &a[0] * &b[2],
            &a[0] * &b[1] - &a[1] * &b[0],
        ]
    }
    fn dot(a: &R3, b: &R3) -> RBig {
        &a[0] * &b[0] + &a[1] * &b[1] + &a[2] * &b[2]
    }
    let exact_coords = |v: u32| -> Option<R3> {
        match &soup.verts[v as usize] {
            VertexCoords::Explicit(p) => Some([rbe(p.x()), rbe(p.y()), rbe(p.z())]),
            implicit => {
                let le = to_generic(implicit).lambda_exact();
                if le.is_undefined() {
                    return None;
                }
                Some([&le.l[0] / &le.d, &le.l[1] / &le.d, &le.l[2] / &le.d])
            }
        }
    };

    // (1) Exact centroid of the first patch triangle whose exact points
    // are all defined.
    let three = RBig::from(3);
    let origin: Option<R3> = patch.iter().find_map(|&t| {
        let tri = soup.tris[t as usize];
        let (a, b, c) = (
            exact_coords(tri[0])?,
            exact_coords(tri[1])?,
            exact_coords(tri[2])?,
        );
        Some([
            (&a[0] + &b[0] + &c[0]) / &three,
            (&a[1] + &b[1] + &c[1]) / &three,
            (&a[2] + &b[2] + &c[2]) / &three,
        ])
    });
    let Some(o) = origin else {
        return Err(InsideOutError::RationalRayDegenerate { patch: pi });
    };

    'axis: for k in 0..3usize {
        // (t, ascending in_tris index, inner-verdict if nearest).
        let mut hits: Vec<(RBig, u32, bool)> = Vec::new();
        for (ti, tri) in soup.in_tris.iter().enumerate() {
            let label = &soup.in_labels[ti];
            // Same input as the tested patch → skip (its own shell), as in
            // the f64 prune.
            if label.iter().any(|id| patch_surface_label.contains(id)) {
                continue;
            }
            let a = [
                rbe(explicit_or_unreachable(&soup.verts[tri[0] as usize]).x()),
                rbe(explicit_or_unreachable(&soup.verts[tri[0] as usize]).y()),
                rbe(explicit_or_unreachable(&soup.verts[tri[0] as usize]).z()),
            ];
            let b = [
                rbe(explicit_or_unreachable(&soup.verts[tri[1] as usize]).x()),
                rbe(explicit_or_unreachable(&soup.verts[tri[1] as usize]).y()),
                rbe(explicit_or_unreachable(&soup.verts[tri[1] as usize]).z()),
            ];
            let c = [
                rbe(explicit_or_unreachable(&soup.verts[tri[2] as usize]).x()),
                rbe(explicit_or_unreachable(&soup.verts[tri[2] as usize]).y()),
                rbe(explicit_or_unreachable(&soup.verts[tri[2] as usize]).z()),
            ];
            let n = cross(&sub(&b, &a), &sub(&c, &a));
            let rhs = dot(&n, &sub(&a, &o));
            if n[k] == RBig::ZERO {
                // Ray parallel to the triangle's plane. On-plane overlap
                // contributes no transversal crossing (the f64 path's
                // `Discard` for ray-coplanar triangles); off-plane never
                // hits. Either way: skip.
                continue;
            }
            let t = &rhs / &n[k];
            if t <= RBig::ZERO {
                // Behind the origin, or t == 0 (origin exactly on the
                // candidate's plane — only a tangential touch can produce
                // this, and a t=0 touch crosses nothing; deviation N20).
                continue;
            }
            // Hit point h = o + t·e_k.
            let mut h = o.clone();
            h[k] = &h[k] + &t;
            // Strictly-inside via n-aligned edge orientations.
            let s1 = dot(&cross(&sub(&b, &a), &sub(&h, &a)), &n);
            let s2 = dot(&cross(&sub(&c, &b), &sub(&h, &b)), &n);
            let s3 = dot(&cross(&sub(&a, &c), &sub(&h, &c)), &n);
            let pos = |s: &RBig| *s > RBig::ZERO;
            let neg = |s: &RBig| *s < RBig::ZERO;
            if pos(&s1) && pos(&s2) && pos(&s3) {
                hits.push((t, ti as u32, n[k] > RBig::ZERO));
            } else if !neg(&s1) && !neg(&s2) && !neg(&s3) {
                // On the closed boundary but not strictly inside: an exact
                // vertex/edge graze — retry the next axis (the rational
                // analog of the f64 path's perturbation machinery).
                continue 'axis;
            }
        }

        // Exact sort along the ray; equal-parameter hits collapse to the
        // first in ascending triangle order (`btree_set` semantics).
        hits.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
        hits.dedup_by(|next, kept| next.0 == kept.0);

        // Nearest hit per label decides (the analyze walk).
        let mut visited: BTreeSet<InputId> = BTreeSet::new();
        let mut inner: BTreeSet<InputId> = BTreeSet::new();
        for (_, ti, nk_pos) in &hits {
            let label = &soup.in_labels[*ti as usize];
            if label.iter().all(|id| visited.contains(id)) {
                continue;
            }
            if *nk_pos {
                inner.extend(label.iter().copied());
            }
            visited.extend(label.iter().copied());
        }
        return Ok(inner.into_iter().collect());
    }
    Err(InsideOutError::RationalRayDegenerate { patch: pi })
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

/// `orient3D(a, b, c, q)` over three generic points and one extra point.
///
/// PR-CR-M7c: native `orient3d_indirect` (Shewchuk convention — the MIRROR
/// of the former FFI sign). Every production caller consumes the result
/// RELATIVELY (straddle / all-same-sign / Zero tests), never as an absolute
/// above/below verdict, so no sign flip is applied. Callers are annotated
/// per-site.
fn orient3d_gp3(gps: &[GenericPoint3D; 3], q: &GenericPoint3D) -> IpSign {
    let [a, b, c] = gps;
    orient3d_indirect(a, b, c, q)
}

/// `orient3D(a, b, p, q)` over two generic points and two explicit points.
/// Same mirror-invariance note as [`orient3d_gp3`].
fn orient3d_gp2_e2(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    p: &GenericPoint3D,
    q: &GenericPoint3D,
) -> IpSign {
    orient3d_indirect(a, b, p, q)
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
        if std::env::var_os("CHERCHI_PERTURB_PROBE").is_some() {
            for &t in tris_to_test {
                let tv = in_tri_verts(soup, t);
                let or01 = orient3d(tv[0], tv[1], p_ray.v0, p_ray.v1);
                let or12 = orient3d(tv[1], tv[2], p_ray.v0, p_ray.v1);
                let or20 = orient3d(tv[2], tv[0], p_ray.v0, p_ray.v1);
                eprintln!("[perturb-probe] offset {offset} tri {t}: {or01:?}/{or12:?}/{or20:?}");
            }
        }
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
    let ray_v0 = GenericPoint3D::explicit(ray.v0);
    let ray_v1 = GenericPoint3D::explicit(ray.v1);

    // One owned LPI generic point per hit, constructed ONCE and reused across
    // every comparison below (PR-CR-M7c: replaces the FFI handle arena — the
    // native `GenericPoint3D` caches its f64/interval lambdas internally, so
    // the O(n²) insert-sort never re-derives a hit's lambdas).
    let lpis: Vec<GenericPoint3D> = hits
        .iter()
        .map(|&t| {
            let tv = in_tri_verts(soup, t);
            GenericPoint3D::lpi(ray.v0, ray.v1, tv[0], tv[1], tv[2])
        })
        .collect();

    let less = |a: &GenericPoint3D, b: &GenericPoint3D| match ray.dir {
        Axis::X => less_than_on_x_indirect(a, b),
        Axis::Y => less_than_on_y_indirect(a, b),
        Axis::Z => less_than_on_z_indirect(a, b),
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
        let gps = [
            to_generic(&soup.verts[seed[0] as usize]),
            to_generic(&soup.verts[seed[1] as usize]),
            to_generic(&soup.verts[seed[2] as usize]),
        ];
        // M7c mirror note: native orient3d is the MIRROR of the former FFI
        // sign, but this discard test is sign-RELATIVE — a hit is "behind"
        // iff its sign strictly OPPOSES v1's sign against the seed plane —
        // so a global flip changes nothing. No `.flipped()`.
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
                Axis::X => less_than_on_x_indirect(&lpis[i], &ray_v0),
                Axis::Y => less_than_on_y_indirect(&lpis[i], &ray_v0),
                Axis::Z => less_than_on_z_indirect(&lpis[i], &ray_v0),
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

/// Port of `pruneIntersectionsAndSortAlongRay` (booleans.cpp:655) over a
/// caller-supplied candidate set (ascending ids; the octree query or the
/// test-only brute scan — any SUPERSET of the ray-AABB-touching triangles
/// is correct because the exact `in_ray_aabb` filter below re-checks every
/// candidate). Vertex/edge hits resolve via ray perturbation over the hit
/// element's same-label incident triangles.
///
/// Deviation (documented): the C++ restricts the vertex one-ring /
/// edge-pair searches (`findVertRingTris` / `findEdgeTris`) to the octree
/// candidate set `tmp_inters`; this port scans ALL input triangles for
/// those. Both are complete — a triangle incident to a vertex (or edge) ON
/// the ray has an AABB touching the ray AABB, so it is always in the
/// octree's superset — and the full scan is simpler and provably so.
fn prune_intersections_and_sort_along_ray(
    soup: &ArrangementSoup,
    ray: &Ray,
    patch_surface_label: &Label,
    candidates: &[u32],
) -> Result<Vec<u32>, InsideOutError> {
    let n = soup.in_tris.len() as u32;
    let mut visited = vec![false; n as usize];
    let mut inters: Vec<u32> = Vec::new();

    // Exact ray-AABB filter (the per-item check inside the C++
    // `intersects_box(octree, rayAABB, ..)`): triangles whose AABB does
    // not touch the segment v0→v1's AABB are not candidates. This is
    // semantically LOAD-BEARING, not just acceleration — it excludes
    // behind-the-origin triangles, whose vertex/edge events would
    // otherwise demand perturbation winners that the sort then
    // (correctly) discards. It is applied to EVERY candidate
    // unconditionally, which is what makes the octree's parameters
    // correctness-neutral (any superset producer yields the same result).
    let (ray_lo, ray_hi) = ray_aabb(ray);
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

    for &t in candidates {
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
        let info = fast_2d_check_intersection_on_ray(ray, tv);
        // KV9-F1 diagnosis probe (read-only, env-gated): per-event trace.
        if std::env::var_os("CHERCHI_INOUT_PROBE").is_some() && !matches!(info, IntersInfo::NoInt) {
            eprintln!(
                "[inout-prune] tri {t} label {:?} event {info:?} v0 ({},{},{})",
                tested_label,
                tv[0].x(),
                tv[0].y(),
                tv[0].z()
            );
        }
        match info {
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
                let winner = perturb_ray_and_find_inters_tri(soup, ray, &edge_tris);
                if std::env::var_os("CHERCHI_INOUT_PROBE").is_some() {
                    let pa = explicit_or_unreachable(&soup.verts[a as usize]);
                    let pb = explicit_or_unreachable(&soup.verts[b as usize]);
                    eprintln!(
                        "[inout-prune]   edge ({a},{b}) = ({},{},{})-({},{},{}) edge_tris {edge_tris:?} winner {winner:?}",
                        pa.x(), pa.y(), pa.z(), pb.x(), pb.y(), pb.z()
                    );
                }
                if let Some(winner) = winner {
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

    // ════════════════════════════════════════════════════════════════
    // Oracle #7 (Cycle C) — octree candidate equivalence. On every
    // labeling fixture:
    //   (a) per-patch, the octree's ray-AABB query, passed through the
    //       SAME exact `in_ray_aabb` filter the prune applies, equals
    //       the brute filtered set (superset + exact filter ⇒ equal);
    //   (b) end-to-end, the octree candidate path produces IDENTICAL
    //       `Vec<Label>` to the brute path (`compute_inside_out_brute`,
    //       the permanent structural diff target);
    //   (c) the production `compute_inside_out` equals both.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn octree_candidates_yield_identical_labels() {
        use crate::labeling::octree::TriOctree;

        let fixtures: Vec<(&str, ArrangementSoup)> = vec![
            (
                "corner-overlap cubes",
                arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B)),
            ),
            (
                "enclosed cube",
                arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(0.5, 0.5, 0.5, 1.0, B)),
            ),
            (
                "disjoint cubes",
                arrange(cube(0.0, 0.0, 0.0, 1.0, A), cube(5.0, 5.0, 5.0, 1.0, B)),
            ),
            (
                "through-cut peg",
                arrange(
                    cube(0.0, 0.0, 0.0, 2.0, A),
                    boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
                ),
            ),
        ];

        for (name, soup) in fixtures {
            let patches = compute_all_patches(&soup).expect("patches");
            let octree = TriOctree::build(&soup);
            let n = soup.in_tris.len() as u32;

            // (a) per-patch filtered-candidate equality on each ACTUAL ray.
            let border: BTreeSet<u32> = patches.border_verts.iter().copied().collect();
            let mut max_c = [f64::NEG_INFINITY; 3];
            for tri in &soup.in_tris {
                for &v in tri {
                    let p = approx_coords(&soup.verts[v as usize]);
                    for k in 0..3 {
                        max_c[k] = max_c[k].max(p[k]);
                    }
                }
            }
            let max_coords = [max_c[0] + 0.5, max_c[1] + 0.5, max_c[2] + 0.5];
            for (pi, patch) in patches.patches.iter().enumerate() {
                let ray = find_ray_endpoints(&soup, patch, &border, max_coords, pi as u32)
                    .expect("ray endpoints");
                let (lo, hi) = ray_aabb(&ray);
                let exact_filter = |t: &u32| {
                    let tv = in_tri_verts(&soup, *t);
                    (0..3).all(|k| {
                        let c = |p: &Point3| match k {
                            0 => p.x(),
                            1 => p.y(),
                            _ => p.z(),
                        };
                        let tlo = c(&tv[0]).min(c(&tv[1])).min(c(&tv[2]));
                        let thi = c(&tv[0]).max(c(&tv[1])).max(c(&tv[2]));
                        tlo <= hi[k] && thi >= lo[k]
                    })
                };
                let via_octree: Vec<u32> = octree
                    .query_aabb(lo, hi)
                    .into_iter()
                    .filter(|t| exact_filter(t))
                    .collect();
                let via_brute: Vec<u32> = (0..n).filter(|t| exact_filter(t)).collect();
                assert_eq!(
                    via_octree, via_brute,
                    "{name}: patch {pi}: filtered octree candidates != filtered brute"
                );
            }

            // (b) + (c) end-to-end label identity.
            let brute = compute_inside_out_brute(&soup, &patches).expect("brute path");
            let via_octree = compute_inside_out_with(&soup, &patches, |ray| {
                let (lo, hi) = ray_aabb(ray);
                octree.query_aabb(lo, hi)
            })
            .expect("octree path");
            assert_eq!(
                via_octree, brute,
                "{name}: octree candidate path changed labels vs brute"
            );
            let production = compute_inside_out(&soup, &patches).expect("production");
            assert_eq!(
                production, brute,
                "{name}: production path changed labels vs brute"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #6 — KV4-F1: the rational-ray fallback for fully-degenerate
    // patches (the C++ "requires exact rationals" exit, booleans.cpp:578).
    //
    // A patch whose only triangle is a sub-ulp NEEDLE — an LPI vertex
    // whose exact point lies BELOW one f64 ulp above an explicit vertex —
    // defeats both f64 origin strategies: every explicit vertex is on the
    // patch border, and the generated-ray branch sees the approximated
    // triangle as exactly collinear (`misaligned` fails) or cannot pass
    // an f64 line strictly inside the femto-thin exact triangle. The
    // F0016 corpus family produces exactly this shape (a tessellation
    // edge piercing a triangle femto-close to its corner).
    // ════════════════════════════════════════════════════════════════

    /// Hand-built soup: input B = a closed unit cube at `origin` (the
    /// only `in_tris` shell); the output `tris` carry ONE needle triangle
    /// attributed to input A — explicit `e0 = base + (0.3, 0.3, 0.3)`,
    /// `e1 = base + (0.7, 0.3, 0.3)`, and an LPI vertex whose EXACT point
    /// sits `≈1.7e-17` above `e0` (below one ulp of 0.3, so its f64
    /// approximation rounds to exactly `e0`'s coordinates). All three
    /// needle vertices are border-marked (as in the corpus case).
    fn needle_soup(base: f64) -> (ArrangementSoup, Patches) {
        let (coords, cube_tris, cube_labels) = cube(0.0, 0.0, 0.0, 1.0, B);
        let mut verts: Vec<VertexCoords> = coords
            .chunks_exact(3)
            .map(|c| VertexCoords::Explicit(Point3::new(c[0], c[1], c[2])))
            .collect();
        let e0 = verts.len() as u32;
        verts.push(VertexCoords::Explicit(Point3::new(base + 0.3, 0.125, 0.3)));
        let e1 = verts.len() as u32;
        verts.push(VertexCoords::Explicit(Point3::new(base + 0.7, 0.125, 0.3)));
        let lpi = verts.len() as u32;
        // Plane through z = 0.3 with a one-ulp tilt and a short lever: its
        // intersection with the vertical line x = base+0.3, y = 0.125 is
        // exactly z = 0.3 + ulp(0.3)·(0.8·0.025/0.64) ≈ 0.3 + 1.7e-18 —
        // 30× below the f64 rounding step, so every f64 evaluation of the
        // point collapses onto e0's plane.
        verts.push(VertexCoords::Lpi {
            line: [
                Point3::new(base + 0.3, 0.125, 0.1),
                Point3::new(base + 0.3, 0.125, 0.9),
            ],
            plane: [
                Point3::new(base + 0.1, 0.1, 0.3),
                Point3::new(base + 0.9, 0.1, 0.3),
                Point3::new(base + 0.1, 0.9, f64::next_up(0.3)),
            ],
        });
        let soup = ArrangementSoup {
            verts,
            tris: vec![[e0, e1, lpi]],
            labels: vec![vec![A]],
            source: Vec::new(), // BL2 test fixture; provenance not exercised
            jolly_count: 0,
            in_tris: cube_tris,
            in_labels: cube_labels,
            multiplier: 1.0,
        };
        let patches = Patches {
            patches: vec![vec![0]],
            tri_to_patch: vec![0],
            border_verts: vec![e0, e1, lpi],
        };
        (soup, patches)
    }

    /// The needle's exact LPI point is sub-ulp above the explicit vertex,
    /// so the PRODUCTION f64 approximation (the generated-ray branch's
    /// input) lands within rounding noise of e0 — the needle is below f64
    /// resolution, which is what defeats every f64 origin strategy.
    #[test]
    fn needle_fixture_lpi_collapses_in_f64() {
        let (soup, _) = needle_soup(0.0);
        let a = approx_point(&soup.verts[10]).expect("approx defined");
        assert_eq!((a.x(), a.y()), (0.3, 0.125));
        assert!(
            (a.z() - 0.3).abs() <= 4.0 * (f64::next_up(0.3) - 0.3),
            "LPI f64 approximation must be within rounding noise of e0's plane, got z={}",
            a.z()
        );
    }

    #[test]
    fn needle_patch_inside_cube_classifies_via_rational_ray() {
        let (soup, patches) = needle_soup(0.0);
        let inner = compute_inside_out(&soup, &patches)
            .expect("rational-ray fallback must classify the needle patch");
        assert_eq!(inner, vec![vec![B]], "needle inside the cube → inner {{B}}");
    }

    #[test]
    fn needle_patch_outside_cube_classifies_via_rational_ray() {
        let (soup, patches) = needle_soup(2.0);
        let inner = compute_inside_out(&soup, &patches)
            .expect("rational-ray fallback must classify the needle patch");
        assert_eq!(inner, vec![vec![]], "needle outside the cube → inner {{}}");
    }

    /// Axis-graze retry: a B-shell vertex placed EXACTLY at the needle
    /// centroid's (y, z)… is impossible in f64 (the exact centroid has a
    /// sub-ulp z), so instead pin the convention link: the rational rule
    /// "inner ⇔ n_k > 0 at the nearest hit" must agree with the f64 path's
    /// `orient3d(tv, ray.v1) == Negative → inner` on a concrete triangle.
    #[test]
    fn rational_orientation_convention_matches_f64_path() {
        // +x face of the unit cube, outward normal +x (winding [1,6,5] of
        // the cube fixture): corners (1,0,0), (1,1,1), (1,0,1).
        let tv = [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 1.0),
            Point3::new(1.0, 0.0, 1.0),
        ];
        // Ray along +X from inside the cube; far endpoint beyond the face.
        let v1 = Point3::new(1.5, 0.3, 0.4);
        // f64 path rule: Negative → inner.
        assert_eq!(orient3d(tv[0], tv[1], tv[2], v1), Sign::Negative);
        // n = (b−a)×(c−a); its X component must be POSITIVE — the rational
        // rule's "inner" side.
        let n_x = (tv[1].y() - tv[0].y()) * (tv[2].z() - tv[0].z())
            - (tv[1].z() - tv[0].z()) * (tv[2].y() - tv[0].y());
        assert!(n_x > 0.0, "outward +x face must have n_x > 0");
    }
}

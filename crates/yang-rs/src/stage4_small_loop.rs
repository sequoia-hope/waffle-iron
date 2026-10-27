//! Yang et al. 2025 §4.2.1, §4.2.2, §4.3.3, §4.3.4 & §4.4.1 — non-crossing
//! `< 2d_ε` small-loop detection with closed-form Gauss-map admission (`N92`),
//! Bajaj et al. 1988 closed-loop marching, parameter-midpoint refinement, and
//! local chart CDT mesh updating (`N91`).
//!
//! ## Paper citations (`refs/text/yang2025_hybrid_boolean.txt`)
//! - **§4.2.1 Eq. (1) (`:433-461` L, Fig. 10(a) `:487-489` L):** Non-crossing
//!   triangle pairs with `Dis(△t_A, △t_B) < 2d_ε` form candidate areas where
//!   meshes miss a surface tangency or small intersection loop (Case III).
//! - **§4.2.2 (`:440-446` R, Fig. 10(b) `:489-491` L, §5.6 `:1033-1035` R):**
//!   Within any closed small intersection loop, Sederberg et al. 1989
//!   guarantees `p_A ∈ S_A, p_B ∈ S_B` with collinear normals `n_A = ±n_B`.
//!   [`GaussMap::overlaps`] bounds each candidate area's exact analytic normals
//!   with zero trim-boundary exemption and rules out small loops whenever the
//!   Gauss maps do not overlap (`N92`).
//! - **§4.3.3 (`:523-540` R, `:567-573` L, `:745` R):** Optimizes the area's
//!   seed by parametric Newton; if `∠(n_A, ±n_B) > 10^{-6}` rad, traces the
//!   closed loop from `r_0` by Bajaj et al. 1988 predictor–corrector marching.
//! - **§4.3.4 (`:575-593` L):** Subdivides consecutive loop points via
//!   parameter-domain midpoints until `h < 10² d_p`, `l < 10³ d_p`,
//!   `α < π/18` ([`crate::stage4_correct::paper_chain_sample_redundant`]).
//! - **§4.4.1 & Fig. 11(c) (`:546-570` R):** Maps each loop vertex `r` onto
//!   both meshes with identical coordinates `r_A = r_B = r`, removes mesh
//!   vertices too close to the loop, inserts an interior surface point `i`
//!   when no mesh vertex lies inside the loop, and triangulates the trimmed
//!   patches in the local parametric chart via CDT.

use crate::boolean::failures::{joined_triangle_pairs, RingCapture};
use crate::gauss_map::GaussMap;
use crate::shell_contact::{closest_on_triangle, triangle_distance, Bvh, TriSet};
use crate::stage3_ssi::{
    curve_contains_point, surface_to_quadric, torus_plane_is_perpendicular_section,
};
use crate::stage4_boundary_curve::{boundary_crease_crossed, CreaseIndex, FaceCrease};
use crate::stage4_correct::paper_chain_sample_redundant;
use crate::stage4_relocate::{
    relocate_onto_implicit_pair, s43_tangency_census_log, surface_distance_and_normal,
    surface_kind_tag,
};
use crate::stage4_tangency::{
    add3, coincidence_floor, collinear_angle_rad, cross3, dot3, mag3, norm3, scale3,
    solve_and_classify_seed, solve_parametric_pair_newton, sub3, thin_svd, LocalSurfaceChart,
    SeedOptimizationResult, V3,
};
use crate::{normalize3, ortho_basis, tri_is_degenerate, BRep, Mesh, Point3, Surface, Vector3};
use cad_primitives::Point2 as CadPoint2;
use cherchi_rs::predicates::{segment_intersects_triangle_3d, SegmentTriangleIntersection};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

/// One connected non-crossing `< 2d_ε` candidate area between `(face_a, face_b)`
/// (`yang2025_hybrid_boolean.txt:456-461` L, `:487-495` L).
#[derive(Clone, Debug)]
pub(crate) struct SmallLoopCandidateArea {
    pub(crate) face_a: u32,
    pub(crate) face_b: u32,
    pub(crate) tris_a: Vec<usize>,
    pub(crate) tris_b: Vec<usize>,
    pub(crate) gauss_overlap: bool,
    pub(crate) p_seed: Point3,
    pub(crate) q_seed: Point3,
    #[allow(dead_code)]
    pub(crate) min_dist: f64,
    pub(crate) max_dev_a: f64,
    pub(crate) max_dev_b: f64,
}

#[derive(Clone, Copy)]
struct RawProxPair {
    ta: usize,
    tb: usize,
    dist: f64,
}

fn model_scale_and_band(set_a: &TriSet, set_b: &TriSet) -> (f64, f64) {
    let scale = set_a
        .lo
        .iter()
        .chain(set_a.hi.iter())
        .chain(set_b.lo.iter())
        .chain(set_b.hi.iter())
        .flat_map(|v| v.iter())
        .fold(0.0_f64, |m, &c| m.max(c.abs()))
        .max(1.0);
    let band = 2.0 * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    (scale, band)
}

/// Detect connected non-crossing `< 2d_ε` candidate areas uncaptured by the
/// arrangement `la`, and evaluate the §4.2.2 closed-form [`GaussMap::overlaps`]
/// filter on each area (`yang2025_hybrid_boolean.txt:440-446` R, zero
/// trim-boundary exemption).
pub(crate) fn discover_small_loop_candidate_areas(
    a: &BRep,
    mesh_a: &Mesh,
    tri_face_a: &[u32],
    b: &BRep,
    mesh_b: &Mesh,
    tri_face_b: &[u32],
    la: &cherchi_rs::LabeledArrangement,
) -> Vec<SmallLoopCandidateArea> {
    let (Ok(set_a), Ok(set_b)) = (
        TriSet::from_parts(mesh_a, tri_face_a, a.faces()),
        TriSet::from_parts(mesh_b, tri_face_b, b.faces()),
    ) else {
        return Vec::new();
    };
    if set_a.pts.is_empty() || set_b.pts.is_empty() {
        return Vec::new();
    }

    let (_scale, band) = model_scale_and_band(&set_a, &set_b);
    let max_dev_b = set_b.dev.iter().copied().fold(0.0_f64, f64::max);
    let bvh_b = Bvh::new(&set_b);
    let joined = joined_triangle_pairs(la);
    let capture = RingCapture::new(mesh_a, mesh_b, &joined);

    let mut by_faces: BTreeMap<(u32, u32), Vec<RawProxPair>> = BTreeMap::new();
    for (ta, &fa) in tri_face_a.iter().enumerate().take(set_a.pts.len()) {
        if (fa as usize) >= a.faces().len() {
            continue;
        }
        let surf_a = set_a.surf[ta];
        let pad = set_a.dev[ta] + max_dev_b + band;
        bvh_b.for_each_near(set_a.lo[ta], set_a.hi[ta], pad, |tb_u32| {
            let tb = tb_u32 as usize;
            let Some(&fb) = tri_face_b.get(tb) else {
                return;
            };
            if (fb as usize) >= b.faces().len() {
                return;
            }
            let surf_b = set_b.surf[tb];
            if surf_a == surf_b
                || matches!(
                    (surf_a, surf_b),
                    (Surface::Plane { .. }, Surface::Plane { .. })
                )
            {
                return;
            }
            let reach = set_a.dev[ta] + set_b.dev[tb] + band;
            if (0..3).any(|k| {
                set_b.lo[tb][k] > set_a.hi[ta][k] + reach
                    || set_a.lo[ta][k] > set_b.hi[tb][k] + reach
            }) {
                return;
            }
            let d = triangle_distance(set_a.pts[ta], set_b.pts[tb]);
            // Strictly non-intersecting triangles within `d_ε(A) + d_ε(B) + band`
            // whose 1-rings have no arrangement crossing.
            if d <= 0.0 || d > reach {
                return;
            }
            if capture.captured(ta as u32, tb as u32) {
                return;
            }
            by_faces
                .entry((fa, fb))
                .or_default()
                .push(RawProxPair { ta, tb, dist: d });
        });
    }

    let mut areas = Vec::new();
    for ((fa, fb), mut pairs) in by_faces {
        pairs.sort_by_key(|p| (p.ta, p.tb));
        let n = pairs.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        // Two proximity pairs `(ta_i, tb_i)` and `(ta_j, tb_j)` belong to the
        // same connected non-crossing patch iff `ta_i` shares ≥1 vertex with
        // `ta_j` AND `tb_i` shares ≥1 vertex with `tb_j` — equivalently, iff
        // they share ≥1 of the 9 `(va, vb)` vertex-pair keys (`O(9 n log n)`).
        let mut first_owner: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for (i, pair) in pairs.iter().enumerate() {
            let tri_a = mesh_a.tris[pair.ta];
            let tri_b = mesh_b.tris[pair.tb];
            for &va in &tri_a {
                for &vb in &tri_b {
                    if let Some(&j) = first_owner.get(&(va, vb)) {
                        let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                        if ri != rj {
                            parent[ri.max(rj)] = ri.min(rj);
                        }
                    } else {
                        first_owner.insert((va, vb), i);
                    }
                }
            }
        }

        let mut comps: BTreeMap<usize, Vec<RawProxPair>> = BTreeMap::new();
        for (idx, &pair) in pairs.iter().enumerate() {
            let r = find(&mut parent, idx);
            comps.entry(r).or_default().push(pair);
        }

        let surf_a = a.faces()[fa as usize].surface;
        let surf_b = b.faces()[fb as usize].surface;
        for comp in comps.into_values() {
            let mut tris_a: Vec<usize> = comp.iter().map(|p| p.ta).collect();
            tris_a.sort_unstable();
            tris_a.dedup();
            let mut tris_b: Vec<usize> = comp.iter().map(|p| p.tb).collect();
            tris_b.sort_unstable();
            tris_b.dedup();

            let pts_a: Vec<[[f64; 3]; 3]> = tris_a
                .iter()
                .map(|&ta| set_a.pts[ta].map(|p| p.as_array()))
                .collect();
            let pts_b: Vec<[[f64; 3]; 3]> = tris_b
                .iter()
                .map(|&tb| set_b.pts[tb].map(|p| p.as_array()))
                .collect();

            // §4.2.2 (`:440-446` R): closed-form Gauss-map overlap admission
            // with NO trim-boundary exemption (N92).
            let gm_a = GaussMap::from_triangles(&surf_a, &pts_a);
            let gm_b = GaussMap::from_triangles(&surf_b, &pts_b);
            let gauss_overlap = gm_a.overlaps(&gm_b);

            let best = comp
                .iter()
                .min_by(|x, y| {
                    x.dist
                        .total_cmp(&y.dist)
                        .then_with(|| x.ta.cmp(&y.ta))
                        .then_with(|| x.tb.cmp(&y.tb))
                })
                .expect("non-empty component");

            // Fig. 10(c) (`yang2025_hybrid_boolean.txt:471-473` R, `:491-499` L):
            // locate the triangle `T` on the left surface nearest to the right
            // surface (`best.ta`), compute its centroid `p = barycenter(T)`, and
            // find the exact closest point `q` to `p` on face `fb` in `M_B`.
            let ta_best_pts = set_a.pts[best.ta].map(|p| p.as_array());
            let pa = scale3(
                add3(add3(ta_best_pts[0], ta_best_pts[1]), ta_best_pts[2]),
                1.0 / 3.0,
            );
            let tb_init_pts = set_b.pts[best.tb].map(|p| p.as_array());
            let mut best_pb =
                closest_on_triangle(pa, tb_init_pts[0], tb_init_pts[1], tb_init_pts[2]);
            let mut best_d2 = dot3(sub3(pa, best_pb), sub3(pa, best_pb));
            let mut best_tb = best.tb;
            let query_pad = best_d2.sqrt();
            bvh_b.for_each_near(pa, pa, query_pad, |tb_u32| {
                let tb = tb_u32 as usize;
                if tri_face_b.get(tb).copied() != Some(fb) {
                    return;
                }
                let t = set_b.pts[tb].map(|p| p.as_array());
                let cand = closest_on_triangle(pa, t[0], t[1], t[2]);
                let d2 = dot3(sub3(pa, cand), sub3(pa, cand));
                if d2
                    .total_cmp(&best_d2)
                    .then_with(|| tb.cmp(&best_tb))
                    .is_lt()
                {
                    best_d2 = d2;
                    best_pb = cand;
                    best_tb = tb;
                }
            });

            let max_dev_a = tris_a
                .iter()
                .map(|&ta| set_a.dev[ta])
                .fold(0.0_f64, f64::max);
            let max_dev_b = tris_b
                .iter()
                .map(|&tb| set_b.dev[tb])
                .fold(0.0_f64, f64::max);

            s43_tangency_census_log(
                if gauss_overlap {
                    "small_loop_gauss_admitted"
                } else {
                    "small_loop_gauss_pruned"
                },
                &format!(
                    "faces={}×{} surfs={}×{} tris={}×{} min_d={:.3e}",
                    fa,
                    fb,
                    surface_kind_tag(surf_a),
                    surface_kind_tag(surf_b),
                    tris_a.len(),
                    tris_b.len(),
                    best.dist
                ),
            );

            areas.push(SmallLoopCandidateArea {
                face_a: fa,
                face_b: fb,
                tris_a,
                tris_b,
                gauss_overlap,
                p_seed: Point3::new(pa[0], pa[1], pa[2]),
                q_seed: Point3::new(best_pb[0], best_pb[1], best_pb[2]),
                min_dist: best.dist,
                max_dev_a,
                max_dev_b,
            });
        }
    }

    areas
}

/// Exact unit tangent `τ = (n_A × n_B) / ‖n_A × n_B‖` and space-curve curvature
/// vector `κ⃗ = dτ/ds` at a transversal intersection point `pt ∈ S_A ∩ S_B` via
/// Meusnier's theorem on orthonormal local surface charts (no finite differences).
fn curve_jet_at(pt: Point3, surf_a: Surface, surf_b: Surface) -> Option<(V3, V3, f64)> {
    let ca = LocalSurfaceChart::new(surf_a, pt)?;
    let eb_chart = LocalSurfaceChart::new(surf_b, pt)?;
    let ea = ca.eval(0.0, 0.0)?;
    let eb = eb_chart.eval(0.0, 0.0)?;
    if collinear_angle_rad(ea.normal, eb.normal) <= cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let cr = cross3(ea.normal, eb.normal);
    let sin_th = norm3(cr);
    if sin_th <= cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let tau_vec = scale3(cr, 1.0 / sin_th);
    let cos_th = dot3(ea.normal, eb.normal).clamp(-1.0, 1.0);
    let sin2 = (sin_th * sin_th).max(1e-18);

    // At `(0, 0)`, `(su, sv)` is orthonormal on all 5 `LocalSurfaceChart`s:
    // `r'' · n = -τ · (dn/ds) = -τ · ((τ·su) nu + (τ·sv) nv)`.
    let dna = add3(
        scale3(ea.nu, dot3(tau_vec, ea.su)),
        scale3(ea.nv, dot3(tau_vec, ea.sv)),
    );
    let dnb = add3(
        scale3(eb.nu, dot3(tau_vec, eb.su)),
        scale3(eb.nv, dot3(tau_vec, eb.sv)),
    );
    let k_na = -dot3(tau_vec, dna);
    let k_nb = -dot3(tau_vec, dnb);
    let k_vec = scale3(
        add3(
            scale3(ea.normal, k_na - k_nb * cos_th),
            scale3(eb.normal, k_nb - k_na * cos_th),
        ),
        1.0 / sin2,
    );
    Some((tau_vec, k_vec, norm3(k_vec)))
}

/// Bajaj et al. 1988 orthogonal-plane Newton corrector solving the `4 × 4`
/// square system `[s_1(u,v) - s_2(s,t) = 0; (0.5*(s_1+s_2) - x_pred)·n_plane = 0]`
/// (`yang2025_hybrid_boolean.txt:570-573` L).
fn correct_orthogonal_plane_newton(
    x_pred: V3,
    n_plane: V3,
    surf_a: Surface,
    surf_b: Surface,
    tau: f64,
    scale: f64,
    max_corr: f64,
) -> Option<Point3> {
    const MAX_ITERS: usize = 24;
    let pred_pt = Point3::new(x_pred[0], x_pred[1], x_pred[2]);
    let chart_a = LocalSurfaceChart::new(surf_a, pred_pt)?;
    let chart_b = LocalSurfaceChart::new(surf_b, pred_pt)?;
    let (u0, v0) = chart_a.project(x_pred)?;
    let (s0, t0) = chart_b.project(x_pred)?;
    let machine_tol = 16.0 * f64::EPSILON * scale;

    let try_accept = |ea_pt: V3, eb_pt: V3| -> Option<Point3> {
        let gap = norm3(sub3(ea_pt, eb_pt));
        if gap > tau {
            return None;
        }
        let mid = scale3(add3(ea_pt, eb_pt), 0.5);
        if dot3(sub3(mid, x_pred), n_plane).abs() > tau {
            return None;
        }
        if norm3(sub3(mid, x_pred)) > max_corr {
            return None;
        }
        let (fa, _) = surface_distance_and_normal(surf_a, mid)?;
        let (fb, _) = surface_distance_and_normal(surf_b, mid)?;
        if fa.abs() <= tau && fb.abs() <= tau {
            Some(Point3::new(mid[0], mid[1], mid[2]))
        } else {
            None
        }
    };

    let mut x = [u0, v0, s0, t0];
    for it in 0..=MAX_ITERS {
        let ea = chart_a.eval(x[0], x[1])?;
        let eb = chart_b.eval(x[2], x[3])?;
        let d_pos = sub3(ea.point, eb.point);
        let mid = scale3(add3(ea.point, eb.point), 0.5);
        let plane_res = dot3(sub3(mid, x_pred), n_plane);
        let res_norm = (dot3(d_pos, d_pos) + plane_res * plane_res).sqrt();
        if res_norm <= machine_tol {
            if let Some(pt) = try_accept(ea.point, eb.point) {
                return Some(pt);
            }
        }

        let j_mat: [[f64; 4]; 4] = [
            [ea.su[0], ea.sv[0], -eb.su[0], -eb.sv[0]],
            [ea.su[1], ea.sv[1], -eb.su[1], -eb.sv[1]],
            [ea.su[2], ea.sv[2], -eb.su[2], -eb.sv[2]],
            [
                0.5 * dot3(n_plane, ea.su),
                0.5 * dot3(n_plane, ea.sv),
                0.5 * dot3(n_plane, eb.su),
                0.5 * dot3(n_plane, eb.sv),
            ],
        ];
        let f_vec = [d_pos[0], d_pos[1], d_pos[2], plane_res];
        let (u_svd, sv, v_svd) = thin_svd::<4, 4>(j_mat);
        if sv[0] <= 1e-14 || sv[3] <= 1e-10 * sv[0] {
            return try_accept(ea.point, eb.point);
        }

        let mut delta = [0.0; 4];
        for j in 0..4 {
            let mut uf = 0.0;
            for i in 0..4 {
                uf += u_svd[i][j] * f_vec[i];
            }
            let coeff = -uf / sv[j];
            for i in 0..4 {
                delta[i] += coeff * v_svd[i][j];
            }
        }
        let d_norm =
            (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] + delta[3] * delta[3])
                .sqrt();
        if d_norm <= 1e-15 * scale || it == MAX_ITERS {
            return try_accept(ea.point, eb.point);
        }
        if d_norm > max_corr {
            let sc = max_corr / d_norm;
            for d in &mut delta {
                *d *= sc;
            }
        }

        let mut accepted = false;
        let mut alpha = 1.0;
        for _ls in 0..8 {
            let x_try = [
                x[0] + alpha * delta[0],
                x[1] + alpha * delta[1],
                x[2] + alpha * delta[2],
                x[3] + alpha * delta[3],
            ];
            if let (Some(ta), Some(tb)) = (
                chart_a.eval(x_try[0], x_try[1]),
                chart_b.eval(x_try[2], x_try[3]),
            ) {
                let dp = sub3(ta.point, tb.point);
                let tm = scale3(add3(ta.point, tb.point), 0.5);
                let pr = dot3(sub3(tm, x_pred), n_plane);
                let try_norm = (dot3(dp, dp) + pr * pr).sqrt();
                if try_norm < res_norm || try_norm <= machine_tol {
                    x = x_try;
                    accepted = true;
                    break;
                }
            }
            alpha *= 0.5;
        }
        if !accepted {
            return try_accept(ea.point, eb.point);
        }
    }
    None
}

/// Optimize the parameter-domain midpoint between consecutive curve points
/// `(p, q)` per Yang §4.3.4 (`yang2025_hybrid_boolean.txt:579-583` L).
fn parameter_midpoint_on_curve(
    p: Point3,
    q: Point3,
    surf_a: Surface,
    surf_b: Surface,
) -> Option<Point3> {
    let mid_anchor = Point3::new(
        0.5 * (p.x() + q.x()),
        0.5 * (p.y() + q.y()),
        0.5 * (p.z() + q.z()),
    );
    let ca = LocalSurfaceChart::new(surf_a, mid_anchor)?;
    let cb = LocalSurfaceChart::new(surf_b, mid_anchor)?;
    let (u0, v0) = ca.project(p.as_array())?;
    let (u1, v1) = ca.project(q.as_array())?;
    let (s0, t0) = cb.project(p.as_array())?;
    let (s1, t1) = cb.project(q.as_array())?;
    let ea_mid = ca.eval(0.5 * (u0 + u1), 0.5 * (v0 + v1))?;
    let eb_mid = cb.eval(0.5 * (s0 + s1), 0.5 * (t0 + t1))?;
    let sol = solve_parametric_pair_newton(
        Point3::new(ea_mid.point[0], ea_mid.point[1], ea_mid.point[2]),
        Point3::new(eb_mid.point[0], eb_mid.point[1], eb_mid.point[2]),
        surf_a,
        surf_b,
    )?;
    if sol.normal_angle_rad <= cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    Some(sol.point)
}

#[allow(clippy::too_many_arguments)]
fn subdivide_segment_s434(
    p: Point3,
    q: Point3,
    surf_a: Surface,
    surf_b: Surface,
    creases_a: &[FaceCrease],
    creases_b: &[FaceCrease],
    dp: f64,
    depth: u32,
    out: &mut Vec<Point3>,
) -> bool {
    let pa = p.as_array();
    let qa = q.as_array();
    let chord = norm3(sub3(qa, pa));
    if depth >= 12 || chord <= 2.0 * dp {
        return true;
    }
    let Some(m) = parameter_midpoint_on_curve(p, q, surf_a, surf_b) else {
        return false;
    };
    let ma = m.as_array();
    // Guard against midpoint snapping outside the segment neighborhood.
    if norm3(sub3(ma, pa)) >= 0.95 * chord || norm3(sub3(qa, ma)) >= 0.95 * chord {
        return false;
    }
    if boundary_crease_crossed(p, m, creases_a).is_some()
        || boundary_crease_crossed(p, m, creases_b).is_some()
        || boundary_crease_crossed(m, q, creases_a).is_some()
        || boundary_crease_crossed(m, q, creases_b).is_some()
    {
        return false;
    }
    if paper_chain_sample_redundant(pa, ma, qa) {
        return true;
    }
    if !subdivide_segment_s434(
        p,
        m,
        surf_a,
        surf_b,
        creases_a,
        creases_b,
        dp,
        depth + 1,
        out,
    ) {
        return false;
    }
    out.push(m);
    subdivide_segment_s434(
        m,
        q,
        surf_a,
        surf_b,
        creases_a,
        creases_b,
        dp,
        depth + 1,
        out,
    )
}

/// Trace a closed interior intersection loop from `r0` via Bajaj et al. 1988
/// (`yang2025_hybrid_boolean.txt:569-573` L) and refine every chord by §4.3.4
/// (`:575-593` L). Returns `None` if the curve crosses a face boundary crease,
/// hits a pinch/tangency, or fails to close cleanly.
pub(crate) fn trace_and_refine_small_loop(
    r0: Point3,
    surf_a: Surface,
    surf_b: Surface,
    creases_a: &[FaceCrease],
    creases_b: &[FaceCrease],
    scale_hint: f64,
) -> Option<Vec<Point3>> {
    const MAX_STEPS: usize = 512;
    let (tau, sc0) = coincidence_floor(r0, r0, surf_a, surf_b);
    let scale = sc0.max(scale_hint).max(1.0);
    let dp = cad_primitives::TAU_MODEL * (1.0 + scale);

    let (tau0, kvec0, kappa0) = curve_jet_at(r0, surf_a, surf_b)?;
    let mut raw_pts = vec![r0];
    let mut cur_pt = r0;
    let mut cur_tau = tau0;
    let mut cur_kvec = kvec0;
    let mut cur_kappa = kappa0;
    let mut turned = 0.0_f64;
    let mut closed = false;

    for _step in 0..MAX_STEPS {
        let rho = 1.0 / cur_kappa.max(1e-12 / scale);
        // Curvature-adaptive Bajaj step (`Δθ ≈ π/16 = 11.25°`, ~32–40 steps per
        // closed loop of any radius); §4.3.4 (`subdivide_segment_s434`) then
        // refines every chord to `paper_chain_sample_redundant`.
        let mut h_nom = ((PI / 16.0) * rho).max(0.5 * dp);

        if raw_pts.len() >= 4 && turned >= 1.45 * PI {
            let to_start = sub3(r0.as_array(), cur_pt.as_array());
            let dist_start = norm3(to_start);
            let along = dot3(to_start, cur_tau);
            if dist_start <= 1.35 * h_nom && along > 0.0 && dot3(cur_tau, tau0) >= 0.75 {
                if boundary_crease_crossed(cur_pt, r0, creases_a).is_some()
                    || boundary_crease_crossed(cur_pt, r0, creases_b).is_some()
                {
                    return None;
                }
                closed = true;
                break;
            }
            if dist_start <= 2.2 * h_nom && along > 0.5 * dist_start && dot3(cur_tau, tau0) >= 0.4 {
                h_nom = (0.55 * dist_start).max(0.5 * dp);
            }
        }

        let mut accepted_step: Option<(Point3, V3, V3, f64, f64)> = None;
        for halving in 0..8 {
            let h = h_nom * 0.5_f64.powi(halving);
            if h < 0.1 * dp {
                break;
            }
            let x_pred = add3(
                cur_pt.as_array(),
                add3(scale3(cur_tau, h), scale3(cur_kvec, 0.5 * h * h)),
            );
            let n_plane = normalize3(add3(cur_tau, scale3(cur_kvec, h)));
            if norm3(n_plane) <= 0.5 {
                continue;
            }
            let Some(next_pt) = correct_orthogonal_plane_newton(
                x_pred,
                n_plane,
                surf_a,
                surf_b,
                tau,
                scale,
                0.75 * h,
            ) else {
                continue;
            };
            let actual_step = norm3(sub3(next_pt.as_array(), cur_pt.as_array()));
            if actual_step < 0.1 * h {
                continue;
            }
            let Some((next_tau, next_kvec, next_kappa)) = curve_jet_at(next_pt, surf_a, surf_b)
            else {
                continue;
            };
            // `next_tau` uses the unflipped canonical orientation `+(n_A × n_B)`:
            // requiring `cos Δθ >= 0.85` (`Δθ <= 31.8°`) prevents jumping across
            // narrow ellipse necks onto the opposite-oriented branch.
            if dot3(next_tau, cur_tau) < 0.85 {
                continue;
            }
            accepted_step = Some((next_pt, next_tau, next_kvec, next_kappa, h));
            break;
        }

        let (next_pt, next_tau, next_kvec, next_kappa, h_used) = accepted_step?;
        if boundary_crease_crossed(cur_pt, next_pt, creases_a).is_some()
            || boundary_crease_crossed(cur_pt, next_pt, creases_b).is_some()
        {
            return None;
        }

        let dtheta = dot3(cur_tau, next_tau).clamp(-1.0, 1.0).acos();
        turned += dtheta;

        if raw_pts.len() >= 4
            && turned >= 1.65 * PI
            && norm3(sub3(next_pt.as_array(), r0.as_array())) <= 0.65 * h_used
            && dot3(next_tau, tau0) >= 0.85
        {
            if boundary_crease_crossed(cur_pt, r0, creases_a).is_some()
                || boundary_crease_crossed(cur_pt, r0, creases_b).is_some()
            {
                return None;
            }
            closed = true;
            break;
        }

        if turned > 2.5 * PI {
            return None;
        }

        raw_pts.push(next_pt);
        cur_pt = next_pt;
        cur_tau = next_tau;
        cur_kvec = next_kvec;
        cur_kappa = next_kappa;
    }

    if !closed || raw_pts.len() < 4 {
        return None;
    }

    // §4.3.4 (`:575-593` L): refine every chord `(p, q)` via parameter-midpoint
    // Newton until `paper_chain_sample_redundant` is satisfied.
    let mut refined = Vec::with_capacity(raw_pts.len() * 2);
    for i in 0..raw_pts.len() {
        let p = raw_pts[i];
        let q = raw_pts[(i + 1) % raw_pts.len()];
        refined.push(p);
        if !subdivide_segment_s434(
            p,
            q,
            surf_a,
            surf_b,
            creases_a,
            creases_b,
            dp,
            0,
            &mut refined,
        ) {
            return None;
        }
    }

    // §4.3.3 (`:535-538` R): remove points closer than `d_p` on the same loop.
    let mut deduped: Vec<Point3> = Vec::with_capacity(refined.len());
    for pt in refined {
        if deduped
            .last()
            .is_some_and(|prev| norm3(sub3(pt.as_array(), prev.as_array())) <= dp)
        {
            continue;
        }
        deduped.push(pt);
    }
    while deduped.len() > 1
        && norm3(sub3(
            deduped.last().unwrap().as_array(),
            deduped[0].as_array(),
        )) <= dp
    {
        deduped.pop();
    }
    if deduped.len() < 4 {
        return None;
    }

    let mut max_diam2 = 0.0_f64;
    let first = deduped[0].as_array();
    for pt in &deduped[1..] {
        let d = sub3(pt.as_array(), first);
        max_diam2 = max_diam2.max(dot3(d, d));
    }
    if max_diam2.sqrt() <= 3.0 * dp {
        return None;
    }

    Some(deduped)
}

#[inline]
fn area2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

#[inline]
fn pt_seg_dist2_2d(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if len2 <= 1e-30 {
        let dx = p[0] - a[0];
        let dy = p[1] - a[1];
        return dx * dx + dy * dy;
    }
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
    let dx = p[0] - (a[0] + t * ab[0]);
    let dy = p[1] - (a[1] + t * ab[1]);
    dx * dx + dy * dy
}

fn segs_properly_or_improperly_cross_2d(
    a0: [f64; 2],
    a1: [f64; 2],
    b0: [f64; 2],
    b1: [f64; 2],
) -> bool {
    let lo_x = a0[0].min(a1[0]);
    let hi_x = a0[0].max(a1[0]);
    let lo_y = a0[1].min(a1[1]);
    let hi_y = a0[1].max(a1[1]);
    if b0[0].max(b1[0]) < lo_x
        || b0[0].min(b1[0]) > hi_x
        || b0[1].max(b1[1]) < lo_y
        || b0[1].min(b1[1]) > hi_y
    {
        return false;
    }
    let o1 = area2(a0, a1, b0);
    let o2 = area2(a0, a1, b1);
    let o3 = area2(b0, b1, a0);
    let o4 = area2(b0, b1, a1);
    if (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0)
        && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0)
    {
        return true;
    }
    let eps = 1e-24;
    (o1.abs() <= eps && pt_seg_dist2_2d(b0, a0, a1) <= eps)
        || (o2.abs() <= eps && pt_seg_dist2_2d(b1, a0, a1) <= eps)
        || (o3.abs() <= eps && pt_seg_dist2_2d(a0, b0, b1) <= eps)
        || (o4.abs() <= eps && pt_seg_dist2_2d(a1, b0, b1) <= eps)
}

fn seg_seg_dist_2d(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> f64 {
    if segs_properly_or_improperly_cross_2d(a0, a1, b0, b1) {
        return 0.0;
    }
    pt_seg_dist2_2d(a0, b0, b1)
        .min(pt_seg_dist2_2d(a1, b0, b1))
        .min(pt_seg_dist2_2d(b0, a0, a1))
        .min(pt_seg_dist2_2d(b1, a0, a1))
        .sqrt()
}

fn point_in_poly_2d(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x_int = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < x_int {
                inside = !inside;
            }
        }
    }
    inside
}

fn point_in_tri_2d(p: [f64; 2], t: [[f64; 2]; 3]) -> bool {
    let d1 = area2(p, t[0], t[1]);
    let d2 = area2(p, t[1], t[2]);
    let d3 = area2(p, t[2], t[0]);
    let has_neg = d1 < -1e-24 || d2 < -1e-24 || d3 < -1e-24;
    let has_pos = d1 > 1e-24 || d2 > 1e-24 || d3 > 1e-24;
    !(has_neg && has_pos)
}

fn polygon_is_simple_and_star_around(poly: &[[f64; 2]], center: [f64; 2]) -> bool {
    let m = poly.len();
    if m < 4 {
        return false;
    }
    let a0 = area2(poly[0], poly[1], center);
    if a0.abs() <= 1e-26 {
        return false;
    }
    let sgn = a0.signum();
    for i in 0..m {
        let ai = area2(poly[i], poly[(i + 1) % m], center);
        if ai * sgn <= 1e-26 {
            return false;
        }
    }
    for i in 0..m {
        let a0 = poly[i];
        let a1 = poly[(i + 1) % m];
        for j in (i + 2)..m {
            if i == 0 && j + 1 == m {
                continue;
            }
            if segs_properly_or_improperly_cross_2d(a0, a1, poly[j], poly[(j + 1) % m]) {
                return false;
            }
        }
    }
    true
}

struct InteriorCapPlan {
    m_l: V3,
    c_a: Point3,
    c_b: Point3,
    ring_verts_a: Vec<Point3>,
    ring_verts_b: Vec<Point3>,
    cap_tris: Vec<[u32; 3]>,
}

/// Solve for the Fig. 11(c) interior cap vertices `(c_A, c_B)` along the common
/// loop-normal line `m_L + t · n_L` (`yang2025_hybrid_boolean.txt:563-566` R),
/// plus concentric dyadic interior collar rings when `m > 24`, certifying that
/// `π(m_L)` lies strictly in the 2D star-kernel of `π(L)` and every interior
/// cap vertex maintains strict common-sign separation `|t_A - t_B| > 2 · tau`
/// so the two 3D caps have bounded vertex degree (`<= 24`) and strictly disjoint
/// interiors meeting exclusively along `L` down to sub-`TAU_MODEL` loops.
fn solve_common_axis_interior_caps(
    loop_pts: &[Point3],
    surf_a: Surface,
    surf_b: Surface,
    creases_a: &[FaceCrease],
    creases_b: &[FaceCrease],
    tau: f64,
) -> Option<InteriorCapPlan> {
    let m = loop_pts.len();
    let mut m_l = [0.0; 3];
    for p in loop_pts {
        m_l = add3(m_l, p.as_array());
    }
    m_l = scale3(m_l, 1.0 / (m as f64));

    let mut newell = [0.0; 3];
    for i in 0..m {
        let u = sub3(loop_pts[i].as_array(), m_l);
        let v = sub3(loop_pts[(i + 1) % m].as_array(), m_l);
        newell = add3(newell, cross3(u, v));
    }
    let nl_len = norm3(newell);
    if nl_len <= 1e-24 {
        return None;
    }
    let n_l = scale3(newell, 1.0 / nl_len);
    let (u_v, v_v) = ortho_basis(Vector3::new(n_l[0], n_l[1], n_l[2]));
    let (u_ax, v_ax) = (u_v.as_array(), v_v.as_array());
    let proj_l: Vec<[f64; 2]> = loop_pts
        .iter()
        .map(|p| {
            let d = sub3(p.as_array(), m_l);
            [dot3(d, u_ax), dot3(d, v_ax)]
        })
        .collect();
    if !polygon_is_simple_and_star_around(&proj_l, [0.0, 0.0]) {
        return None;
    }

    let solve_ray_at = |surf: Surface, base: V3| -> Option<(f64, Point3)> {
        let mut t = 0.0_f64;
        for _ in 0..10 {
            let x = add3(base, scale3(n_l, t));
            let (f, grad_n) = surface_distance_and_normal(surf, x)?;
            if f.abs() <= 0.25 * tau {
                return Some((t, Point3::new(x[0], x[1], x[2])));
            }
            let df = dot3(grad_n, n_l);
            if df.abs() <= 1e-4 {
                return None;
            }
            t -= f / df;
        }
        let x = add3(base, scale3(n_l, t));
        let (f, _) = surface_distance_and_normal(surf, x)?;
        (f.abs() <= tau).then_some((t, Point3::new(x[0], x[1], x[2])))
    };

    let (t_a, c_a) = solve_ray_at(surf_a, m_l)?;
    let (t_b, c_b) = solve_ray_at(surf_b, m_l)?;
    if (t_a - t_b).abs() <= 2.0 * tau {
        return None;
    }
    let sep_sgn = (t_a - t_b).signum();
    if boundary_crease_crossed(loop_pts[0], c_a, creases_a).is_some()
        || boundary_crease_crossed(loop_pts[0], c_b, creases_b).is_some()
    {
        return None;
    }

    let proj_sgn = area2(proj_l[0], proj_l[1], [0.0, 0.0]).signum();
    let mut ring_verts_a: Vec<Point3> = Vec::new();
    let mut ring_verts_b: Vec<Point3> = Vec::new();
    let mut cap_tris: Vec<[u32; 3]> = Vec::new();
    let mut cur_rays: Vec<usize> = (0..m).collect();
    let mut cur_ring_ids: Vec<u32> = (0..m as u32).collect();
    let mut cur_rho = 1.0_f64;
    let base_delta = (2.0 * std::f64::consts::PI / (m as f64)).clamp(1e-3, 0.25);
    let mut step_idx = 0u32;

    while cur_rays.len() > 24 {
        let step_in = base_delta * ((1u64 << step_idx.min(20)) as f64);
        step_idx += 1;
        let next_rho = if cur_rho - step_in >= 0.25 {
            cur_rho - step_in
        } else {
            0.5 * cur_rho
        };
        if next_rho <= 0.08 {
            break;
        }

        let n_cur = cur_rays.len();
        let mut sectors: Vec<bool> = Vec::with_capacity(n_cur / 2 + 2);
        let mut next_rays: Vec<usize> = Vec::with_capacity(n_cur / 2 + 2);
        let mut i = 0usize;
        while i < n_cur {
            next_rays.push(cur_rays[i]);
            if i + 2 <= n_cur && (next_rays.len() + (n_cur - i - 2) / 2 >= 8) {
                let r0 = cur_rays[i];
                let r1 = cur_rays[i + 1];
                let r2 = cur_rays[(i + 2) % n_cur];
                let u1 = [proj_l[r1][0] * cur_rho, proj_l[r1][1] * cur_rho];
                let w0 = [proj_l[r0][0] * next_rho, proj_l[r0][1] * next_rho];
                let w2 = [proj_l[r2][0] * next_rho, proj_l[r2][1] * next_rho];
                if area2(u1, w2, w0) * proj_sgn > 1e-26 {
                    sectors.push(true);
                    i += 2;
                    continue;
                }
            }
            sectors.push(false);
            i += 1;
        }

        let mut next_ring_ids: Vec<u32> = Vec::with_capacity(next_rays.len());
        for &ray in &next_rays {
            let q = [proj_l[ray][0] * next_rho, proj_l[ray][1] * next_rho];
            let base = add3(m_l, add3(scale3(u_ax, q[0]), scale3(v_ax, q[1])));
            let (ta_r, pa_r) = solve_ray_at(surf_a, base)?;
            let (tb_r, pb_r) = solve_ray_at(surf_b, base)?;
            if (ta_r - tb_r) * sep_sgn <= 2.0 * tau {
                return None;
            }
            if boundary_crease_crossed(loop_pts[ray], pa_r, creases_a).is_some()
                || boundary_crease_crossed(loop_pts[ray], pb_r, creases_b).is_some()
            {
                return None;
            }
            let vid = (m + ring_verts_a.len()) as u32;
            ring_verts_a.push(pa_r);
            ring_verts_b.push(pb_r);
            next_ring_ids.push(vid);
        }

        let n_sec = sectors.len();
        let mut cur_pos = 0usize;
        for (j, &two_step) in sectors.iter().enumerate() {
            let w0 = next_ring_ids[j];
            let w_next = next_ring_ids[(j + 1) % n_sec];
            if two_step {
                let u0 = cur_ring_ids[cur_pos];
                let u1 = cur_ring_ids[cur_pos + 1];
                let u2 = cur_ring_ids[(cur_pos + 2) % n_cur];
                cap_tris.push([u0, u1, w0]);
                cap_tris.push([u1, u2, w_next]);
                cap_tris.push([u1, w_next, w0]);
                cur_pos += 2;
            } else {
                let u0 = cur_ring_ids[cur_pos];
                let u1 = cur_ring_ids[(cur_pos + 1) % n_cur];
                cap_tris.push([u0, u1, w0]);
                cap_tris.push([u1, w_next, w0]);
                cur_pos += 1;
            }
        }
        debug_assert_eq!(cur_pos, n_cur);
        cur_rays = next_rays;
        cur_ring_ids = next_ring_ids;
        cur_rho = next_rho;
    }

    let c_id = (m + ring_verts_a.len()) as u32;
    let n_inner = cur_ring_ids.len();
    for j in 0..n_inner {
        cap_tris.push([cur_ring_ids[j], cur_ring_ids[(j + 1) % n_inner], c_id]);
    }

    Some(InteriorCapPlan {
        m_l,
        c_a,
        c_b,
        ring_verts_a,
        ring_verts_b,
        cap_tris,
    })
}

struct PlannedFaceSplice {
    removed_tris: BTreeSet<usize>,
    added_verts: Vec<Point3>,
    replacement_tris: Vec<[u32; 3]>,
    exterior_tri_coords: Vec<[Point3; 3]>,
}

/// Plan the §4.4.1 / Fig. 11(c) local chart CDT splice of `loop_pts` + `c_pt`
/// into `face_idx` of `mesh` (`yang2025_hybrid_boolean.txt:546-570` R).
///
/// Certifies:
/// 1. Simple-disk cavity `P` on `face_idx` enclosing `L` with positive boundary
///    clearance (expanding across interior `face_idx` edges if needed);
/// 2. Removal of interior cavity vertices too close to `L` (`:563-564` R) and
///    dyadic exact-surface collar samples around `L` (`:568-570` R);
/// 3. Identical boundary half-edge multiset to `P` and exact `1:1` interior
///    half-edge pairing (preserving 2-manifoldness and orientation).
#[allow(clippy::too_many_arguments)]
fn plan_face_loop_splice(
    mesh: &Mesh,
    tri_face: &[u32],
    face_idx: u32,
    surf: Surface,
    m_l: V3,
    loop_pts: &[Point3],
    ring_verts: &[Point3],
    c_pt: Point3,
    cap_tris: &[[u32; 3]],
) -> Option<PlannedFaceSplice> {
    let chart = LocalSurfaceChart::new(surf, Point3::new(m_l[0], m_l[1], m_l[2]))?;
    let c_uv = chart.project(c_pt.as_array())?;
    let mut loop_uv = Vec::with_capacity(loop_pts.len());
    for p in loop_pts {
        let (u, v) = chart.project(p.as_array())?;
        loop_uv.push([u, v]);
    }
    if !polygon_is_simple_and_star_around(&loop_uv, [c_uv.0, c_uv.1]) {
        return None;
    }

    let m = loop_uv.len();
    let mut min_loop_edge = f64::INFINITY;
    let mut min_loop_rad = f64::INFINITY;
    let mut max_loop_rad = 0.0_f64;
    let mut l_lo = [f64::INFINITY; 2];
    let mut l_hi = [f64::NEG_INFINITY; 2];
    for i in 0..m {
        let a = loop_uv[i];
        let b = loop_uv[(i + 1) % m];
        min_loop_edge = min_loop_edge.min((b[0] - a[0]).hypot(b[1] - a[1]));
        let rad = (a[0] - c_uv.0).hypot(a[1] - c_uv.1);
        min_loop_rad = min_loop_rad.min(rad);
        max_loop_rad = max_loop_rad.max(rad);
        l_lo[0] = l_lo[0].min(a[0]);
        l_lo[1] = l_lo[1].min(a[1]);
        l_hi[0] = l_hi[0].max(a[0]);
        l_hi[1] = l_hi[1].max(a[1]);
    }
    let clearance = (0.35 * min_loop_edge).max(1e-11);
    let (max_du, max_dv) = chart.max_non_wrapping_edge_delta();

    // Project all chart-valid triangles belonging to `face_idx`.
    let mut valid_tri_uv: BTreeMap<usize, [[f64; 2]; 3]> = BTreeMap::new();
    let mut vert_uv_map: BTreeMap<u32, [f64; 2]> = BTreeMap::new();
    let mut edge_to_face_tris: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    let mut vert_to_face_tris: BTreeMap<u32, Vec<usize>> = BTreeMap::new();

    for (ti, &fi) in tri_face.iter().enumerate() {
        if fi != face_idx {
            continue;
        }
        let tri = mesh.tris[ti];
        let (Some(uv0), Some(uv1), Some(uv2)) = (
            chart.project(mesh.verts[tri[0] as usize].as_array()),
            chart.project(mesh.verts[tri[1] as usize].as_array()),
            chart.project(mesh.verts[tri[2] as usize].as_array()),
        ) else {
            continue;
        };
        let p0 = [uv0.0, uv0.1];
        let p1 = [uv1.0, uv1.1];
        let p2 = [uv2.0, uv2.1];
        let edge_wraps = |a: [f64; 2], b: [f64; 2]| -> bool {
            (b[0] - a[0]).abs() > max_du || (b[1] - a[1]).abs() > max_dv
        };
        if edge_wraps(p0, p1) || edge_wraps(p1, p2) || edge_wraps(p2, p0) {
            continue;
        }
        valid_tri_uv.insert(ti, [p0, p1, p2]);
        vert_uv_map.insert(tri[0], p0);
        vert_uv_map.insert(tri[1], p1);
        vert_uv_map.insert(tri[2], p2);
        for k in 0..3 {
            let (u, w) = (tri[k], tri[(k + 1) % 3]);
            edge_to_face_tris
                .entry((u.min(w), u.max(w)))
                .or_default()
                .push(ti);
            vert_to_face_tris.entry(u).or_default().push(ti);
        }
    }

    // Seed `cavity` with every chart-valid `face_idx` triangle touching or
    // within `clearance` of `loop_uv`.
    let tri_touches_loop = |tuv: [[f64; 2]; 3]| -> bool {
        let t_lo_x = tuv[0][0].min(tuv[1][0]).min(tuv[2][0]) - clearance;
        let t_hi_x = tuv[0][0].max(tuv[1][0]).max(tuv[2][0]) + clearance;
        let t_lo_y = tuv[0][1].min(tuv[1][1]).min(tuv[2][1]) - clearance;
        let t_hi_y = tuv[0][1].max(tuv[1][1]).max(tuv[2][1]) + clearance;
        if l_hi[0] < t_lo_x || l_lo[0] > t_hi_x || l_hi[1] < t_lo_y || l_lo[1] > t_hi_y {
            return false;
        }
        if loop_uv.iter().any(|&lp| point_in_tri_2d(lp, tuv))
            || tuv.iter().any(|&tv| point_in_poly_2d(tv, &loop_uv))
        {
            return true;
        }
        for k in 0..3 {
            let a0 = tuv[k];
            let a1 = tuv[(k + 1) % 3];
            for j in 0..m {
                if seg_seg_dist_2d(a0, a1, loop_uv[j], loop_uv[(j + 1) % m]) <= clearance {
                    return true;
                }
            }
        }
        false
    };

    let mut cavity: BTreeSet<usize> = valid_tri_uv
        .iter()
        .filter_map(|(&ti, &tuv)| tri_touches_loop(tuv).then_some(ti))
        .collect();
    if cavity.is_empty() {
        return None;
    }

    // Iteratively close boundary clearance violations, boundary pinches, and
    // multi-loop cavities across interior edges of `face_idx`.
    let extract_boundary_cycle = |cav: &BTreeSet<usize>| -> Result<Vec<u32>, Option<Vec<usize>>> {
        let mut dir_count: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for &ti in cav {
            let t = mesh.tris[ti];
            for k in 0..3 {
                *dir_count.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        let mut boundary_he: Vec<(u32, u32)> = Vec::new();
        for (&(u, w), &cnt) in &dir_count {
            if cnt > 1 {
                return Err(None);
            }
            let rev = dir_count.get(&(w, u)).copied().unwrap_or(0);
            if rev == 0 {
                boundary_he.push((u, w));
            }
        }
        if boundary_he.len() < 3 {
            return Err(None);
        }
        // Check if any boundary edge is within `clearance` of `loop_uv`.
        let mut expand: Vec<usize> = Vec::new();
        for &(u, w) in &boundary_he {
            let pu = vert_uv_map[&u];
            let pw = vert_uv_map[&w];
            let too_close = (0..m)
                .any(|j| seg_seg_dist_2d(pu, pw, loop_uv[j], loop_uv[(j + 1) % m]) <= clearance);
            if too_close {
                let key = (u.min(w), u.max(w));
                if let Some(nbrs) = edge_to_face_tris.get(&key) {
                    for &nt in nbrs {
                        if !cav.contains(&nt) {
                            expand.push(nt);
                        }
                    }
                }
                if expand.is_empty() {
                    // `loop_uv` touches an outer boundary edge of `face_idx`.
                    return Err(None);
                }
            }
        }
        if !expand.is_empty() {
            return Err(Some(expand));
        }

        // Check for pinch vertices on the boundary (`out_deg > 1`).
        let mut next_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &(u, w) in &boundary_he {
            next_of.entry(u).or_default().push(w);
        }
        for (&u, outs) in &next_of {
            if outs.len() > 1 {
                if let Some(inc) = vert_to_face_tris.get(&u) {
                    let add: Vec<usize> =
                        inc.iter().copied().filter(|t| !cav.contains(t)).collect();
                    if !add.is_empty() {
                        return Err(Some(add));
                    }
                }
                return Err(None);
            }
        }

        // Walk single cycle from the lowest vertex id.
        let start = *next_of.keys().next().ok_or(None)?;
        let mut cycle = Vec::with_capacity(boundary_he.len());
        let mut cur = start;
        for _ in 0..boundary_he.len() {
            cycle.push(cur);
            let outs = next_of.get(&cur).ok_or(None)?;
            cur = outs[0];
            if cur == start {
                break;
            }
        }
        if cur != start || cycle.len() != boundary_he.len() {
            // Multiple boundary components: expand one ring around cavity.
            let mut ring_add = Vec::new();
            for &(u, w) in &boundary_he {
                if let Some(nbrs) = edge_to_face_tris.get(&(u.min(w), u.max(w))) {
                    for &nt in nbrs {
                        if !cav.contains(&nt) {
                            ring_add.push(nt);
                        }
                    }
                }
            }
            if !ring_add.is_empty() {
                return Err(Some(ring_add));
            }
            return Err(None);
        }
        Some(cycle).ok_or(None)
    };

    let outer_global = loop {
        match extract_boundary_cycle(&cavity) {
            Ok(cyc) => break cyc,
            Err(Some(more)) if cavity.len() + more.len() <= 64 => {
                for t in more {
                    cavity.insert(t);
                }
            }
            Err(_) => return None,
        }
    };

    // Verify consistent 2D triangle orientation across `cavity` (`:559-562` R)
    // and verify `loop_uv` lies strictly inside `outer_poly`.
    let first_tuv = valid_tri_uv[cavity.iter().next()?];
    let a_first = area2(first_tuv[0], first_tuv[1], first_tuv[2]);
    if a_first.abs() <= 1e-26 {
        return None;
    }
    let orient_sign = a_first.signum();
    for &ti in &cavity {
        let tuv = valid_tri_uv[&ti];
        if area2(tuv[0], tuv[1], tuv[2]) * orient_sign <= 1e-26 {
            return None;
        }
    }

    let outer_poly: Vec<[f64; 2]> = outer_global.iter().map(|v| vert_uv_map[v]).collect();
    // Verify `outer_poly` is simple in 2D and strictly encloses `loop_uv`.
    for i in 0..outer_poly.len() {
        let a0 = outer_poly[i];
        let a1 = outer_poly[(i + 1) % outer_poly.len()];
        for j in (i + 2)..outer_poly.len() {
            if i == 0 && j + 1 == outer_poly.len() {
                continue;
            }
            if segs_properly_or_improperly_cross_2d(
                a0,
                a1,
                outer_poly[j],
                outer_poly[(j + 1) % outer_poly.len()],
            ) {
                return None;
            }
        }
    }
    if !point_in_poly_2d(loop_uv[0], &outer_poly) {
        return None;
    }

    // Build local CDT vertex pool:
    // 1. `outer_global` boundary vertices (preserving exact global vertex IDs);
    // 2. `loop_pts` (`m` shared intersection loop vertices);
    // 3. Kept existing interior vertices of `cavity` not too close to `loop_uv`
    //    (`:563-564` R);
    // 4. Dyadic exact-surface collar vertices around `loop_uv` (`:568-570` R).
    let outer_set: BTreeSet<u32> = outer_global.iter().copied().collect();
    let mut all_cav_verts: BTreeSet<u32> = BTreeSet::new();
    for &ti in &cavity {
        for v in mesh.tris[ti] {
            all_cav_verts.insert(v);
        }
    }

    let base_vert_count = mesh.verts.len() as u32;
    let mut added_verts: Vec<Point3> = Vec::new();
    let mut pool_uv: Vec<CadPoint2> = Vec::new();
    let mut pool_global: Vec<u32> = Vec::new();

    let mut outer_local: Vec<u32> = Vec::with_capacity(outer_global.len());
    for &gv in &outer_global {
        let uv = vert_uv_map[&gv];
        outer_local.push(pool_uv.len() as u32);
        pool_uv.push(CadPoint2::new(uv[0], uv[1]));
        pool_global.push(gv);
    }

    let mut hole_local: Vec<u32> = Vec::with_capacity(m);
    for (p3, &uv) in loop_pts.iter().zip(loop_uv.iter()) {
        let gv = base_vert_count + added_verts.len() as u32;
        added_verts.push(*p3);
        hole_local.push(pool_uv.len() as u32);
        pool_uv.push(CadPoint2::new(uv[0], uv[1]));
        pool_global.push(gv);
    }

    let mut interior_local: Vec<u32> = Vec::new();
    let mut interior_uv_list: Vec<[f64; 2]> = Vec::new();
    let min_keep_dist = (0.6 * min_loop_edge).max(1e-10);
    for gv in all_cav_verts {
        if outer_set.contains(&gv) {
            continue;
        }
        let uv = vert_uv_map[&gv];
        if point_in_poly_2d(uv, &loop_uv) {
            continue;
        }
        let d_loop = (0..m)
            .map(|j| pt_seg_dist2_2d(uv, loop_uv[j], loop_uv[(j + 1) % m]).sqrt())
            .fold(f64::INFINITY, f64::min);
        if d_loop <= min_keep_dist {
            continue;
        }
        interior_local.push(pool_uv.len() as u32);
        pool_uv.push(CadPoint2::new(uv[0], uv[1]));
        pool_global.push(gv);
        interior_uv_list.push(uv);
    }

    // Dyadic exact-surface exterior collar rings from `c_uv` outward around
    // `loop_uv`, keeping exterior vertex degree bounded and exterior triangles
    // well-shaped and within `d_ε` sagitta (`:568-570` R).
    if !matches!(surf, Surface::Plane { .. }) || max_loop_rad > 0.0 {
        let mut outer_diam = 0.0_f64;
        for p in &outer_poly {
            outer_diam = outer_diam.max((p[0] - c_uv.0).hypot(p[1] - c_uv.1));
        }
        let mut step_out = (2.0 * std::f64::consts::PI / (m as f64)).clamp(0.02, 1.0);
        for _ring in 0..16 {
            let factor = 1.0 + step_out;
            if max_loop_rad * factor >= 0.75 * outer_diam {
                break;
            }
            let n_rays = ((2.0 * std::f64::consts::PI * factor / step_out).round() as usize)
                .clamp(8, 256)
                .min(m);
            let ring_sep = (0.35 * step_out * min_loop_rad).max(0.4 * min_loop_edge);
            for r_idx in 0..n_rays {
                let k = (r_idx * m) / n_rays;
                let dir = [loop_uv[k][0] - c_uv.0, loop_uv[k][1] - c_uv.1];
                let cand = [c_uv.0 + factor * dir[0], c_uv.1 + factor * dir[1]];
                if !point_in_poly_2d(cand, &outer_poly) || point_in_poly_2d(cand, &loop_uv) {
                    continue;
                }
                let d_out = (0..outer_poly.len())
                    .map(|j| {
                        pt_seg_dist2_2d(cand, outer_poly[j], outer_poly[(j + 1) % outer_poly.len()])
                            .sqrt()
                    })
                    .fold(f64::INFINITY, f64::min);
                let d_in = (0..m)
                    .map(|j| pt_seg_dist2_2d(cand, loop_uv[j], loop_uv[(j + 1) % m]).sqrt())
                    .fold(f64::INFINITY, f64::min);
                if d_out <= ring_sep || d_in <= 0.4 * ring_sep {
                    continue;
                }
                if interior_uv_list
                    .iter()
                    .any(|q| (cand[0] - q[0]).hypot(cand[1] - q[1]) <= 0.45 * ring_sep)
                {
                    continue;
                }
                let Some(ev) = chart.eval(cand[0], cand[1]) else {
                    continue;
                };
                let gv = base_vert_count + added_verts.len() as u32;
                added_verts.push(Point3::new(ev.point[0], ev.point[1], ev.point[2]));
                interior_local.push(pool_uv.len() as u32);
                pool_uv.push(CadPoint2::new(cand[0], cand[1]));
                pool_global.push(gv);
                interior_uv_list.push(cand);
            }
            step_out *= 2.0;
        }
    }

    let cdt_tris = cherchi_rs::cdt_polygon_with_holes_keep_interior(
        &pool_uv,
        &outer_local,
        std::slice::from_ref(&hole_local),
        &interior_local,
    )
    .ok()?;

    // Append concentric dyadic interior cap vertices + Fig. 11(c) center `c_pt`
    // and build oriented exterior + interior-cap triangles.
    let mut cap_uv: Vec<[f64; 2]> = Vec::with_capacity(m + ring_verts.len() + 1);
    let mut cap_global: Vec<u32> = Vec::with_capacity(m + ring_verts.len() + 1);
    for (k, &uv) in loop_uv.iter().enumerate() {
        cap_uv.push(uv);
        cap_global.push(pool_global[hole_local[k] as usize]);
    }
    for rv in ring_verts {
        let (ru, rv_c) = chart.project(rv.as_array())?;
        let gv = base_vert_count + added_verts.len() as u32;
        added_verts.push(*rv);
        cap_uv.push([ru, rv_c]);
        cap_global.push(gv);
    }
    let c_global = base_vert_count + added_verts.len() as u32;
    added_verts.push(c_pt);
    cap_uv.push([c_uv.0, c_uv.1]);
    cap_global.push(c_global);

    let vert_coords = |gv: u32| -> Point3 {
        if (gv as usize) < mesh.verts.len() {
            mesh.verts[gv as usize]
        } else {
            added_verts[(gv - base_vert_count) as usize]
        }
    };

    let mut replacement_tris: Vec<[u32; 3]> = Vec::with_capacity(cdt_tris.len() + cap_tris.len());
    let mut exterior_tri_coords: Vec<[Point3; 3]> = Vec::with_capacity(cdt_tris.len());
    for lt in cdt_tris {
        let a = [pool_uv[lt[0] as usize].x(), pool_uv[lt[0] as usize].y()];
        let b = [pool_uv[lt[1] as usize].x(), pool_uv[lt[1] as usize].y()];
        let c = [pool_uv[lt[2] as usize].x(), pool_uv[lt[2] as usize].y()];
        let mut gt = [
            pool_global[lt[0] as usize],
            pool_global[lt[1] as usize],
            pool_global[lt[2] as usize],
        ];
        if area2(a, b, c) * orient_sign < 0.0 {
            gt.swap(1, 2);
        }
        let p0 = vert_coords(gt[0]);
        let p1 = vert_coords(gt[1]);
        let p2 = vert_coords(gt[2]);
        if tri_is_degenerate(p0.as_array(), p1.as_array(), p2.as_array()) {
            return None;
        }
        exterior_tri_coords.push([p0, p1, p2]);
        replacement_tris.push(gt);
    }

    let ct_first = *cap_tris.first()?;
    let a_cap0 = area2(
        cap_uv[ct_first[0] as usize],
        cap_uv[ct_first[1] as usize],
        cap_uv[ct_first[2] as usize],
    );
    if a_cap0.abs() <= 1e-26 {
        return None;
    }
    let cap_uv_sgn = a_cap0.signum();
    let flip_cap = cap_uv_sgn * orient_sign < 0.0;
    for &ct in cap_tris {
        let a = cap_uv[ct[0] as usize];
        let b = cap_uv[ct[1] as usize];
        let c = cap_uv[ct[2] as usize];
        if area2(a, b, c) * cap_uv_sgn <= 1e-26 {
            return None;
        }
        let mut gt = [
            cap_global[ct[0] as usize],
            cap_global[ct[1] as usize],
            cap_global[ct[2] as usize],
        ];
        if flip_cap {
            gt.swap(0, 1);
        }
        let p0 = vert_coords(gt[0]);
        let p1 = vert_coords(gt[1]);
        let p2 = vert_coords(gt[2]);
        if tri_is_degenerate(p0.as_array(), p1.as_array(), p2.as_array()) {
            return None;
        }
        replacement_tris.push(gt);
    }

    // Topological conformality gate: `replacement_tris` must have `fwd <= 1` on
    // every directed edge and its unpaired boundary half-edges must match
    // `cavity`'s unpaired boundary half-edges identically.
    let boundary_he_set = |tris: &[[u32; 3]]| -> Option<BTreeSet<(u32, u32)>> {
        let mut counts: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for t in tris {
            if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
                return None;
            }
            for k in 0..3 {
                let e = (t[k], t[(k + 1) % 3]);
                let c = counts.entry(e).or_default();
                *c += 1;
                if *c > 1 {
                    return None;
                }
            }
        }
        let mut bnd = BTreeSet::new();
        for &(u, w) in counts.keys() {
            if !counts.contains_key(&(w, u)) {
                bnd.insert((u, w));
            }
        }
        Some(bnd)
    };

    let orig_cav_tris: Vec<[u32; 3]> = cavity.iter().map(|&ti| mesh.tris[ti]).collect();
    if boundary_he_set(&orig_cav_tris)? != boundary_he_set(&replacement_tris)? {
        return None;
    }

    Some(PlannedFaceSplice {
        removed_tris: cavity,
        added_verts,
        replacement_tris,
        exterior_tri_coords,
    })
}

fn apply_face_splice(
    mesh: &mut Mesh,
    tri_face: &mut Vec<u32>,
    face_idx: u32,
    plan: PlannedFaceSplice,
) {
    mesh.verts.extend(plan.added_verts);
    let mut new_tris =
        Vec::with_capacity(mesh.tris.len() - plan.removed_tris.len() + plan.replacement_tris.len());
    let mut new_tf = Vec::with_capacity(new_tris.capacity());
    for (ti, (&t, &tf)) in mesh.tris.iter().zip(tri_face.iter()).enumerate() {
        if !plan.removed_tris.contains(&ti) {
            new_tris.push(t);
            new_tf.push(tf);
        }
    }
    for rt in plan.replacement_tris {
        new_tris.push(rt);
        new_tf.push(face_idx);
    }
    mesh.tris = new_tris;
    *tri_face = new_tf;
}

/// Verify that no newly created exterior triangle around `L` on `A` crosses any
/// newly created exterior triangle around `L` on `B` away from their shared `L`
/// vertices.
fn exterior_splices_clean(ext_a: &[[Point3; 3]], ext_b: &[[Point3; 3]]) -> bool {
    let same_pt = |p: Point3, q: Point3| -> bool {
        p.x().to_bits() == q.x().to_bits()
            && p.y().to_bits() == q.y().to_bits()
            && p.z().to_bits() == q.z().to_bits()
    };
    let tri_aabb = |t: &[Point3; 3]| -> ([f64; 3], [f64; 3]) {
        let (p0, p1, p2) = (t[0].as_array(), t[1].as_array(), t[2].as_array());
        (
            [
                p0[0].min(p1[0]).min(p2[0]),
                p0[1].min(p1[1]).min(p2[1]),
                p0[2].min(p1[2]).min(p2[2]),
            ],
            [
                p0[0].max(p1[0]).max(p2[0]),
                p0[1].max(p1[1]).max(p2[1]),
                p0[2].max(p1[2]).max(p2[2]),
            ],
        )
    };
    let aabbs_a: Vec<([f64; 3], [f64; 3])> = ext_a.iter().map(tri_aabb).collect();
    let aabbs_b: Vec<([f64; 3], [f64; 3])> = ext_b.iter().map(tri_aabb).collect();

    for (ia, &ta) in ext_a.iter().enumerate() {
        let (lo_a, hi_a) = aabbs_a[ia];
        for (ib, &tb) in ext_b.iter().enumerate() {
            let (lo_b, hi_b) = aabbs_b[ib];
            if hi_a[0] < lo_b[0]
                || lo_a[0] > hi_b[0]
                || hi_a[1] < lo_b[1]
                || lo_a[1] > hi_b[1]
                || hi_a[2] < lo_b[2]
                || lo_a[2] > hi_b[2]
            {
                continue;
            }
            for (seg_src, tri_dst) in [(ta, tb), (tb, ta)] {
                for k in 0..3 {
                    let p0 = seg_src[k];
                    let p1 = seg_src[(k + 1) % 3];
                    if tri_dst.iter().any(|&tv| same_pt(p0, tv) || same_pt(p1, tv)) {
                        continue;
                    }
                    if segment_intersects_triangle_3d(p0, p1, tri_dst[0], tri_dst[1], tri_dst[2])
                        == SegmentTriangleIntersection::Intersects
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn ssi_curve_supports_point(surf_a: Surface, surf_b: Surface, pt: Point3, dp: f64) -> bool {
    if torus_plane_is_perpendicular_section(surf_a, surf_b) {
        return true;
    }
    let (Ok(qa), Ok(qb)) = (surface_to_quadric(surf_a), surface_to_quadric(surf_b)) else {
        return false;
    };
    let Ok(cands) = ssi_rs::intersect(&qa, &qb) else {
        return false;
    };
    let sphere_r = match (surf_a, surf_b) {
        (Surface::Sphere { radius, .. }, Surface::Plane { .. })
        | (Surface::Plane { .. }, Surface::Sphere { radius, .. }) => Some(radius),
        _ => None,
    };
    cands
        .iter()
        .any(|c| curve_contains_point(c, pt, dp, sphere_r))
}

#[inline]
fn point_to_segment_dist2(p: V3, a: V3, b: V3) -> f64 {
    let ab = sub3(b, a);
    let len2 = dot3(ab, ab);
    let foot = if len2 <= 1e-30 {
        a
    } else {
        let t = (dot3(sub3(p, a), ab) / len2).clamp(0.0, 1.0);
        add3(a, scale3(ab, t))
    };
    let d = sub3(p, foot);
    dot3(d, d)
}

fn point_near_any_loop(pt: Point3, loops: &[Vec<Point3>], tol: f64) -> bool {
    let p = pt.as_array();
    let tol2 = tol * tol;
    for lp in loops {
        for i in 0..lp.len() {
            let a = lp[i].as_array();
            let b = lp[(i + 1) % lp.len()].as_array();
            if point_to_segment_dist2(p, a, b) <= tol2 {
                return true;
            }
        }
    }
    false
}

/// Collect and relocate onto `(surf_a, surf_b)` every Stage-2 arrangement
/// intersection edge `(u, v)` whose incident output triangles attribute to
/// `(InputId(0), face_a)` and `(InputId(1), face_b)`.
pub(crate) fn collect_solved_face_pair_segments(
    la: &cherchi_rs::LabeledArrangement,
    tri_face_a: &[u32],
    tri_face_b: &[u32],
    face_a: u32,
    face_b: u32,
    surf_a: Surface,
    surf_b: Surface,
) -> Vec<(Point3, Point3)> {
    use cherchi_rs::InputId as LaInputId;
    if la.intersection_edges.is_empty() {
        return Vec::new();
    }
    let mut edge_flags: BTreeMap<(u32, u32), (bool, bool)> = BTreeMap::new();
    for (ot, tri) in la.mesh.tris.iter().enumerate() {
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            let key = (i.min(j), i.max(j));
            if !la.intersection_edges.contains(&key) {
                continue;
            }
            let flags = edge_flags.entry(key).or_insert((false, false));
            for &(inp, raw) in la.source.get(ot).map(Vec::as_slice).unwrap_or(&[]) {
                if inp == LaInputId(0) && tri_face_a.get(raw as usize).copied() == Some(face_a) {
                    flags.0 = true;
                } else if inp == LaInputId(1)
                    && tri_face_b.get(raw as usize).copied() == Some(face_b)
                {
                    flags.1 = true;
                }
            }
        }
    }

    let mut reloc_cache: BTreeMap<u32, Option<Point3>> = BTreeMap::new();
    let mut relocate_vert = |vid: u32| -> Option<Point3> {
        if let Some(&cached) = reloc_cache.get(&vid) {
            return cached;
        }
        let raw_pt = *la.mesh.verts.get(vid as usize)?;
        let solved = relocate_onto_implicit_pair(raw_pt, surf_a, surf_b).or_else(|| {
            solve_parametric_pair_newton(raw_pt, raw_pt, surf_a, surf_b).map(|s| s.point)
        });
        reloc_cache.insert(vid, solved);
        solved
    };

    let mut segs = Vec::new();
    for ((u, v), (has_a, has_b)) in edge_flags {
        if !(has_a && has_b) {
            continue;
        }
        if let (Some(pu), Some(pv)) = (relocate_vert(u), relocate_vert(v)) {
            segs.push((pu, pv));
        }
    }
    segs
}

/// Yang et al. 2025 §4.2.2 (`yang2025_hybrid_boolean.txt:502-505` L): a
/// candidate intersection point `r0` is redundant if it falls on another
/// accurately solved intersection curve (either an already-spliced small loop
/// or an arrangement-witnessed curve on `(surf_a, surf_b)`).
pub(crate) fn is_redundant_with_solved_curves(
    r0: Point3,
    surf_a: Surface,
    surf_b: Surface,
    solved_segs: &[(Point3, Point3)],
    spliced_loops: &[Vec<Point3>],
    dp: f64,
) -> bool {
    if point_near_any_loop(r0, spliced_loops, 3.0 * dp) {
        return true;
    }
    if solved_segs.is_empty() {
        return false;
    }
    let p = r0.as_array();
    let dp2 = dp * dp;
    if solved_segs
        .iter()
        .any(|&(a, b)| point_to_segment_dist2(p, a.as_array(), b.as_array()) <= dp2)
    {
        return true;
    }

    // Every non-`SurfacePair` `ssi_rs::SsiCurve` (`Circle`, `Line`, `Ellipse`,
    // `Parabola`, single-sheet `Hyperbola`) is a single connected 1D curve
    // branch. If `r0` and a relocated arrangement segment `(a, b)` both lie
    // within `dp` of the same connected conic branch, `r0` falls on that
    // already-solved intersection curve (`:502-505` L).
    if let (Ok(qa), Ok(qb)) = (surface_to_quadric(surf_a), surface_to_quadric(surf_b)) {
        if let Ok(cands) = ssi_rs::intersect(&qa, &qb) {
            for c in &cands {
                if matches!(c, ssi_rs::SsiCurve::SurfacePair { .. }) {
                    continue;
                }
                if curve_contains_point(c, r0, dp, None)
                    && solved_segs.iter().any(|&(a, b)| {
                        curve_contains_point(c, a, dp, None) && curve_contains_point(c, b, dp, None)
                    })
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Updated meshes and spliced loop chords when ≥1 small closed intersection
/// loop was resolved and inserted.
pub(crate) struct SmallLoopMeshUpdate {
    pub(crate) mesh_a: Mesh,
    pub(crate) tri_face_a: Vec<u32>,
    pub(crate) mesh_b: Mesh,
    pub(crate) tri_face_b: Vec<u32>,
    pub(crate) loop_chords: Vec<(Point3, Point3)>,
    pub(crate) scale: f64,
}

/// Detect non-crossing `< 2d_ε` small closed loops admitted by [`GaussMap`],
/// trace and §4.3.4-refine each loop via Bajaj et al. 1988, and splice
/// matching `r_A = r_B = r` polylines + Fig. 11(c) interior points into copies
/// of `(mesh_a, mesh_b)`. Returns `None` when no small loop was spliced.
pub(crate) fn resolve_and_splice_small_loops(
    a: &BRep,
    mesh_a: &Mesh,
    tri_face_a: &[u32],
    b: &BRep,
    mesh_b: &Mesh,
    tri_face_b: &[u32],
    la: &cherchi_rs::LabeledArrangement,
) -> Option<SmallLoopMeshUpdate> {
    if tri_face_a.len() != mesh_a.tris.len() || tri_face_b.len() != mesh_b.tris.len() {
        return None;
    }
    let areas =
        discover_small_loop_candidate_areas(a, mesh_a, tri_face_a, b, mesh_b, tri_face_b, la);
    if !areas.iter().any(|ar| ar.gauss_overlap) {
        return None;
    }

    let (Ok(set_a), Ok(set_b)) = (
        TriSet::from_parts(mesh_a, tri_face_a, a.faces()),
        TriSet::from_parts(mesh_b, tri_face_b, b.faces()),
    ) else {
        return None;
    };
    let (scale, band) = model_scale_and_band(&set_a, &set_b);
    let dp = cad_primitives::TAU_MODEL * (1.0 + scale);

    let creases_a = CreaseIndex::build(a);
    let creases_b = CreaseIndex::build(b);

    let mut mesh_a_work = mesh_a.clone();
    let mut tf_a_work = tri_face_a.to_vec();
    let mut mesh_b_work = mesh_b.clone();
    let mut tf_b_work = tri_face_b.to_vec();
    let mut spliced_loops: Vec<Vec<Point3>> = Vec::new();
    let mut loop_chords: Vec<(Point3, Point3)> = Vec::new();
    let mut solved_segs_by_pair: BTreeMap<(u32, u32), Vec<(Point3, Point3)>> = BTreeMap::new();

    for area in &areas {
        if !area.gauss_overlap {
            continue;
        }
        let fa = area.face_a as usize;
        let fb = area.face_b as usize;
        let surf_a = a.faces()[fa].surface;
        let surf_b = b.faces()[fb].surface;

        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &ta in &area.tris_a {
            for k in 0..3 {
                lo[k] = lo[k].min(set_a.lo[ta][k]);
                hi[k] = hi[k].max(set_a.hi[ta][k]);
            }
        }
        for &tb in &area.tris_b {
            for k in 0..3 {
                lo[k] = lo[k].min(set_b.lo[tb][k]);
                hi[k] = hi[k].max(set_b.hi[tb][k]);
            }
        }
        for seed_pt in [area.p_seed.as_array(), area.q_seed.as_array()] {
            for k in 0..3 {
                lo[k] = lo[k].min(seed_pt[k]);
                hi[k] = hi[k].max(seed_pt[k]);
            }
        }
        let area_diag = norm3(sub3(hi, lo)) + area.max_dev_a + area.max_dev_b + band;

        let seed_res =
            solve_and_classify_seed(area.p_seed, area.q_seed, surf_a, surf_b, Some(area_diag));
        let SeedOptimizationResult::CurvePoint(seed_sol) = seed_res else {
            s43_tangency_census_log("small_loop_decline", &format!("seed_classify={seed_res:?}"));
            continue;
        };
        let r0 = seed_sol.point;

        let solved_segs = solved_segs_by_pair
            .entry((area.face_a, area.face_b))
            .or_insert_with(|| {
                collect_solved_face_pair_segments(
                    la,
                    tri_face_a,
                    tri_face_b,
                    area.face_a,
                    area.face_b,
                    surf_a,
                    surf_b,
                )
            });
        if is_redundant_with_solved_curves(r0, surf_a, surf_b, solved_segs, &spliced_loops, dp) {
            s43_tangency_census_log("small_loop_redundant_on_solved_curve", "pre_trace");
            continue;
        }

        // Eq. (1) (`:456` L) postcondition: `r0` must sit within `d_ε + band`
        // of the candidate area's triangles on BOTH operands.
        let r0_arr = r0.as_array();
        let mut d_area_a = f64::INFINITY;
        let mut foot_a = area.p_seed.as_array();
        for &ta in &area.tris_a {
            let t = set_a.pts[ta].map(|p| p.as_array());
            let c = closest_on_triangle(r0_arr, t[0], t[1], t[2]);
            let d = norm3(sub3(r0_arr, c));
            if d < d_area_a {
                d_area_a = d;
                foot_a = c;
            }
        }
        let mut d_area_b = f64::INFINITY;
        let mut foot_b = area.q_seed.as_array();
        for &tb in &area.tris_b {
            let t = set_b.pts[tb].map(|p| p.as_array());
            let c = closest_on_triangle(r0_arr, t[0], t[1], t[2]);
            let d = norm3(sub3(r0_arr, c));
            if d < d_area_b {
                d_area_b = d;
                foot_b = c;
            }
        }
        if d_area_a > area.max_dev_a + band || d_area_b > area.max_dev_b + band {
            s43_tangency_census_log(
                "small_loop_decline",
                &format!(
                    "d_area=({d_area_a:.3e},{d_area_b:.3e}) vs ({:.3e},{:.3e})",
                    area.max_dev_a + band,
                    area.max_dev_b + band
                ),
            );
            continue;
        }

        let cr_a = creases_a.faces.get(fa).map_or(&[][..], Vec::as_slice);
        let cr_b = creases_b.faces.get(fb).map_or(&[][..], Vec::as_slice);
        let foot_pt_a = Point3::new(foot_a[0], foot_a[1], foot_a[2]);
        let foot_pt_b = Point3::new(foot_b[0], foot_b[1], foot_b[2]);
        if boundary_crease_crossed(foot_pt_a, r0, cr_a).is_some()
            || boundary_crease_crossed(foot_pt_b, r0, cr_b).is_some()
        {
            s43_tangency_census_log("small_loop_decline", "seed_crease_crossed");
            continue;
        }
        if !ssi_curve_supports_point(surf_a, surf_b, r0, dp) {
            s43_tangency_census_log("small_loop_decline", "ssi_curve_unsupported");
            continue;
        }

        let Some(loop_pts) = trace_and_refine_small_loop(r0, surf_a, surf_b, cr_a, cr_b, scale)
        else {
            s43_tangency_census_log("small_loop_decline", "trace_failed");
            continue;
        };
        // For degree-4 `SurfacePair`s (whose zero set can contain multiple
        // disjoint loops), `loop_pts` is refined to §4.3.4 sagitta `< 10² d_p`
        // (`:590` L): if any relocated arrangement vertex on `(fa, fb)` lies
        // within `10² d_p` of `loop_pts`, `loop_pts` is the same connected
        // curve component already captured by the arrangement (`:502-505` L).
        if !solved_segs.is_empty()
            && solved_segs.iter().any(|&(p0, p1)| {
                point_near_any_loop(p0, std::slice::from_ref(&loop_pts), 100.0 * dp)
                    || point_near_any_loop(p1, std::slice::from_ref(&loop_pts), 100.0 * dp)
            })
        {
            s43_tangency_census_log("small_loop_redundant_on_solved_curve", "post_trace_pair");
            continue;
        }
        let (tau, _) = coincidence_floor(r0, r0, surf_a, surf_b);
        let Some(cap_plan) =
            solve_common_axis_interior_caps(&loop_pts, surf_a, surf_b, cr_a, cr_b, tau)
        else {
            s43_tangency_census_log("small_loop_decline", "interior_caps_failed");
            continue;
        };

        let Some(plan_a) = plan_face_loop_splice(
            &mesh_a_work,
            &tf_a_work,
            area.face_a,
            surf_a,
            cap_plan.m_l,
            &loop_pts,
            &cap_plan.ring_verts_a,
            cap_plan.c_a,
            &cap_plan.cap_tris,
        ) else {
            s43_tangency_census_log("small_loop_decline", "plan_a_failed");
            continue;
        };
        let Some(plan_b) = plan_face_loop_splice(
            &mesh_b_work,
            &tf_b_work,
            area.face_b,
            surf_b,
            cap_plan.m_l,
            &loop_pts,
            &cap_plan.ring_verts_b,
            cap_plan.c_b,
            &cap_plan.cap_tris,
        ) else {
            s43_tangency_census_log("small_loop_decline", "plan_b_failed");
            continue;
        };

        if !exterior_splices_clean(&plan_a.exterior_tri_coords, &plan_b.exterior_tri_coords) {
            s43_tangency_census_log("small_loop_decline", "exterior_not_clean");
            continue;
        }

        apply_face_splice(&mut mesh_a_work, &mut tf_a_work, area.face_a, plan_a);
        apply_face_splice(&mut mesh_b_work, &mut tf_b_work, area.face_b, plan_b);

        for k in 0..loop_pts.len() {
            loop_chords.push((loop_pts[k], loop_pts[(k + 1) % loop_pts.len()]));
        }
        s43_tangency_census_log(
            "small_loop_spliced",
            &format!(
                "faces={}×{} surfs={}×{} verts={}",
                area.face_a,
                area.face_b,
                surface_kind_tag(surf_a),
                surface_kind_tag(surf_b),
                loop_pts.len()
            ),
        );
        spliced_loops.push(loop_pts);
    }

    if loop_chords.is_empty() {
        return None;
    }

    Some(SmallLoopMeshUpdate {
        mesh_a: mesh_a_work,
        tri_face_a: tf_a_work,
        mesh_b: mesh_b_work,
        tri_face_b: tf_b_work,
        loop_chords,
        scale,
    })
}

/// Register spliced small-loop chords `(r_k, r_{k+1})` in
/// `la.intersection_edges` after re-running Stage 2 on the updated meshes
/// (Cherchi's `detect_intersecting_pairs` skips shared input edges, while
/// `compute_all_patches` splits patches along multiplicity-4 shared edges).
pub(crate) fn register_spliced_loop_edges_in_arrangement(
    la: &mut cherchi_rs::LabeledArrangement,
    loop_chords: &[(Point3, Point3)],
    scale: f64,
) {
    if loop_chords.is_empty() {
        return;
    }
    let mut existing_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in &la.mesh.tris {
        for k in 0..3 {
            let (u, w) = (tri[k], tri[(k + 1) % 3]);
            existing_edges.insert((u.min(w), u.max(w)));
        }
    }

    let match_tol = cad_primitives::TAU_WORK * (1.0 + scale.max(1.0));
    let match_tol2 = match_tol * match_tol;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &(p, q) in loop_chords {
        for pt in [p.as_array(), q.as_array()] {
            for k in 0..3 {
                lo[k] = lo[k].min(pt[k] - match_tol);
                hi[k] = hi[k].max(pt[k] + match_tol);
            }
        }
    }
    let cand_verts: Vec<(u32, [f64; 3])> = la
        .mesh
        .verts
        .iter()
        .enumerate()
        .filter_map(|(idx, v)| {
            let va = v.as_array();
            (va[0] >= lo[0]
                && va[0] <= hi[0]
                && va[1] >= lo[1]
                && va[1] <= hi[1]
                && va[2] >= lo[2]
                && va[2] <= hi[2])
                .then_some((idx as u32, va))
        })
        .collect();

    let matching_verts = |target: Point3| -> Vec<u32> {
        let ta = target.as_array();
        cand_verts
            .iter()
            .filter_map(|&(idx, va)| {
                let d = sub3(va, ta);
                (dot3(d, d) <= match_tol2 && mag3(d) <= match_tol).then_some(idx)
            })
            .collect()
    };

    for &(p, q) in loop_chords {
        let vp = matching_verts(p);
        let vq = matching_verts(q);
        for &g0 in &vp {
            for &g1 in &vq {
                if g0 == g1 {
                    continue;
                }
                let key = (g0.min(g1), g0.max(g1));
                if existing_edges.contains(&key) {
                    la.intersection_edges.insert(key);
                }
            }
        }
    }
}

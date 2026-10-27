//! Yang 2025 §4.2.2 closed-form Gauss-map estimation and overlap filtering for
//! analytic surfaces (`Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus`).
//!
//! Paper basis:
//! - §4.2.2 (`refs/text/yang2025_hybrid_boolean.txt:440-482` right col;
//!   `:498-503` left col): within a small closed intersection loop (or interior
//!   tangency) of two surface patches `S_A`, `S_B`, Sederberg et al. 1989
//!   guarantees points `p_A ∈ S_A`, `p_B ∈ S_B` whose surface normals are
//!   collinear (`n_A = ±n_B`). Conservatively bounding each patch's Gauss map on
//!   `S²` and testing overlap prunes non-intersecting `2d_ε` regions before
//!   exact intersection evaluation.
//! - §5.6 (`:1033-1035` right col): potential-intersection areas on
//!   non-intersecting meshes are identified from distance evaluation and normal
//!   estimation.
//! - Rational Bézier normal-cone bounds (Theorem 4.1, Appendix B, `:446-466` R,
//!   `:1206-1230`) remain out of scope under ledger **D14**; this module
//!   implements the exact closed-form counterpart for the five analytic
//!   surfaces in scope as the §4.2.2 / §4.3.3 small-loop & tangency candidate
//!   admission filter (ledger **N92** & **N91**, spec
//!   `specs/yang_43_tangency_newton.md`).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::{ortho_basis, Surface, Vector3};

/// Maximum number of elementary [`GaussRegion`]s kept in a [`GaussMap`] before
/// conservative pairwise merging. Four regions suffice to represent any split
/// across both an azimuth span `> π` and a torus inner/outer equator transition
/// (`v = ±π/2`) without merging loss, with zero heap allocation (`Copy`).
pub const MAX_GAUSS_REGIONS: usize = 4;

/// Intrinsic IEEE-754 angular rounding guard (`64 · ε_mach ≈ 1.42e-14` rad)
/// accounting for basis construction, `atan2`/`asin`, and Kahan angle
/// evaluation so boundary-tangent Gauss maps never suffer a false negative.
/// This is a pure floating-point rounding bound, never a geometric tolerance.
const FP_ANGLE_GUARD: f64 = 64.0 * f64::EPSILON;

/// One elementary closed subset of `S²`: the intersection of a spherical cap
/// (circular half-cone) `C(cap_axis, cap_half_angle)` and an axial polar-angle
/// zone `Z(zone_axis, zone_min, zone_max)`:
///
/// `R = { n ∈ S² : ∠(n, cap_axis) ≤ cap_half_angle  ∧  zone_min ≤ ∠(n, zone_axis) ≤ zone_max }`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GaussRegion {
    /// Unit axis of the bounding spherical cap.
    pub cap_axis: [f64; 3],
    /// Half-angle of the bounding spherical cap in `[0, π]`.
    pub cap_half_angle: f64,
    /// Unit reference axis of the bounding spherical zone.
    pub zone_axis: [f64; 3],
    /// Minimum polar angle `∠(n, zone_axis)` in `[0, π]`.
    pub zone_min: f64,
    /// Maximum polar angle `∠(n, zone_axis)` in `[zone_min, π]`.
    pub zone_max: f64,
}

impl GaussRegion {
    #[inline]
    fn dummy() -> Self {
        Self {
            cap_axis: [0.0, 0.0, 1.0],
            cap_half_angle: 0.0,
            zone_axis: [0.0, 0.0, 1.0],
            zone_min: 0.0,
            zone_max: 0.0,
        }
    }

    #[inline]
    fn from_cap_and_zone(
        cap_axis: [f64; 3],
        cap_half_angle: f64,
        zone_axis: [f64; 3],
        zone_min: f64,
        zone_max: f64,
    ) -> Self {
        let cap_half_angle = cap_half_angle.clamp(0.0, PI);
        let z0 = zone_min.clamp(0.0, PI);
        let z1 = zone_max.clamp(0.0, PI);
        Self {
            cap_axis,
            cap_half_angle,
            zone_axis,
            zone_min: z0.min(z1),
            zone_max: z0.max(z1),
        }
    }

    #[inline]
    fn from_cap(cap_axis: [f64; 3], cap_half_angle: f64) -> Self {
        let theta = cap_half_angle.clamp(0.0, PI);
        Self {
            cap_axis,
            cap_half_angle: theta,
            zone_axis: cap_axis,
            zone_min: 0.0,
            zone_max: theta,
        }
    }

    /// Conservative test for `R_A ∩ (sign · R_B) ≠ ∅`, where `sign ∈ {+1.0, -1.0}`.
    #[inline]
    fn overlaps_signed(&self, other: &Self, sign: f64) -> bool {
        let db = [
            sign * other.cap_axis[0],
            sign * other.cap_axis[1],
            sign * other.cap_axis[2],
        ];
        let gamma = unit_angle(self.cap_axis, db);
        if gamma > self.cap_half_angle + other.cap_half_angle + FP_ANGLE_GUARD {
            return false;
        }

        // Zone overlap: for `n ∈ Z(a_A, α0, α1)`, the angle `∠(n, a_B)` spans
        // `[dist(ψ, [α0, α1]), π − dist(π − ψ, [α0, α1])]` where `ψ = ∠(a_A, a_B)`.
        // Under negation (`sign = -1`), `n' = -n` has `∠(n', a_B) = π − ∠(n, a_B)`.
        let (b0, b1) = if sign >= 0.0 {
            (other.zone_min, other.zone_max)
        } else {
            (PI - other.zone_max, PI - other.zone_min)
        };
        let psi = unit_angle(self.zone_axis, other.zone_axis);
        let lo = interval_point_dist(psi, self.zone_min, self.zone_max);
        let hi = PI - interval_point_dist(PI - psi, self.zone_min, self.zone_max);
        lo.max(b0) <= hi.min(b1) + FP_ANGLE_GUARD
    }

    /// Conservatively enclose `self ∪ other` in a single [`GaussRegion`].
    fn merge(self, other: Self) -> Self {
        // 1. Minimal enclosing spherical cap of two caps on S².
        let gamma = unit_angle(self.cap_axis, other.cap_axis);
        let (cap_axis, cap_half_angle) =
            if gamma + other.cap_half_angle <= self.cap_half_angle + FP_ANGLE_GUARD {
                (
                    self.cap_axis,
                    (self.cap_half_angle + FP_ANGLE_GUARD).min(PI),
                )
            } else if gamma + self.cap_half_angle <= other.cap_half_angle + FP_ANGLE_GUARD {
                (
                    other.cap_axis,
                    (other.cap_half_angle + FP_ANGLE_GUARD).min(PI),
                )
            } else {
                let theta_new = 0.5 * (self.cap_half_angle + gamma + other.cap_half_angle);
                if theta_new + FP_ANGLE_GUARD >= PI {
                    (self.cap_axis, PI)
                } else {
                    let beta = 0.5 * (gamma + other.cap_half_angle - self.cap_half_angle);
                    let dot = dot3(self.cap_axis, other.cap_axis);
                    let u = [
                        other.cap_axis[0] - dot * self.cap_axis[0],
                        other.cap_axis[1] - dot * self.cap_axis[1],
                        other.cap_axis[2] - dot * self.cap_axis[2],
                    ];
                    let u_len = norm3(u);
                    let tangent = if u_len > 1e-12 {
                        [u[0] / u_len, u[1] / u_len, u[2] / u_len]
                    } else {
                        let (e1, _) = ortho_basis(Vector3::new(
                            self.cap_axis[0],
                            self.cap_axis[1],
                            self.cap_axis[2],
                        ));
                        e1.as_array()
                    };
                    let (cb, sb) = (beta.cos(), beta.sin());
                    let axis_raw = [
                        cb * self.cap_axis[0] + sb * tangent[0],
                        cb * self.cap_axis[1] + sb * tangent[1],
                        cb * self.cap_axis[2] + sb * tangent[2],
                    ];
                    let axis = unit_or(axis_raw, self.cap_axis);
                    (axis, (theta_new + FP_ANGLE_GUARD).min(PI))
                }
            };

        // 2. Project `other`'s zone onto `self.zone_axis` and take the hull.
        let psi = unit_angle(self.zone_axis, other.zone_axis);
        let other_lo = interval_point_dist(psi, other.zone_min, other.zone_max);
        let other_hi = PI - interval_point_dist(PI - psi, other.zone_min, other.zone_max);
        let zone_min = (self.zone_min.min(other_lo) - FP_ANGLE_GUARD).max(0.0);
        let zone_max = (self.zone_max.max(other_hi) + FP_ANGLE_GUARD).min(PI);

        Self::from_cap_and_zone(cap_axis, cap_half_angle, self.zone_axis, zone_min, zone_max)
    }
}

/// Conservative closed-form bound on the Gauss map `G(U) ⊂ S²` of an analytic
/// surface region `U`: the union of `0..=count` closed [`GaussRegion`]s
/// (`count ≤ MAX_GAUSS_REGIONS`). Whole-sphere coverage (`S²`) is represented
/// uniformly by [`GaussMap::FULL`] (`cap_half_angle = π`, `zone = [0, π]`).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GaussMap {
    count: usize,
    items: [GaussRegion; MAX_GAUSS_REGIONS],
}

impl GaussMap {
    /// Whole unit sphere `S²` (used when the surface region covers antipodal
    /// normal cones or degenerate/singular geometry).
    pub const FULL: Self = Self {
        count: 1,
        items: [
            GaussRegion {
                cap_axis: [0.0, 0.0, 1.0],
                cap_half_angle: PI,
                zone_axis: [0.0, 0.0, 1.0],
                zone_min: 0.0,
                zone_max: PI,
            },
            GaussRegion {
                cap_axis: [0.0, 0.0, 1.0],
                cap_half_angle: 0.0,
                zone_axis: [0.0, 0.0, 1.0],
                zone_min: 0.0,
                zone_max: 0.0,
            },
            GaussRegion {
                cap_axis: [0.0, 0.0, 1.0],
                cap_half_angle: 0.0,
                zone_axis: [0.0, 0.0, 1.0],
                zone_min: 0.0,
                zone_max: 0.0,
            },
            GaussRegion {
                cap_axis: [0.0, 0.0, 1.0],
                cap_half_angle: 0.0,
                zone_axis: [0.0, 0.0, 1.0],
                zone_min: 0.0,
                zone_max: 0.0,
            },
        ],
    };

    #[inline]
    fn empty() -> Self {
        Self {
            count: 0,
            items: [GaussRegion::dummy(); MAX_GAUSS_REGIONS],
        }
    }

    #[inline]
    fn single(region: GaussRegion) -> Self {
        let mut items = [GaussRegion::dummy(); MAX_GAUSS_REGIONS];
        items[0] = region;
        Self { count: 1, items }
    }

    /// Whether this bound covers all of `S²`.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.items[..self.count].iter().any(|r| {
            r.cap_half_angle >= PI - FP_ANGLE_GUARD
                && r.zone_min <= FP_ANGLE_GUARD
                && r.zone_max >= PI - FP_ANGLE_GUARD
        })
    }

    #[inline]
    fn push_region(&mut self, region: GaussRegion) {
        if self.is_full() {
            *self = Self::FULL;
            return;
        }
        if region.cap_half_angle >= PI - FP_ANGLE_GUARD
            && region.zone_min <= FP_ANGLE_GUARD
            && region.zone_max >= PI - FP_ANGLE_GUARD
        {
            *self = Self::FULL;
            return;
        }
        if self.count < MAX_GAUSS_REGIONS {
            self.items[self.count] = region;
            self.count += 1;
        } else {
            // Merge the pair whose merged cap half-angle is smallest.
            let mut all = [GaussRegion::dummy(); MAX_GAUSS_REGIONS + 1];
            all[..MAX_GAUSS_REGIONS].copy_from_slice(&self.items);
            all[MAX_GAUSS_REGIONS] = region;
            let mut best_i = 0;
            let mut best_j = 1;
            let mut best_merged = all[0].merge(all[1]);
            for i in 0..=MAX_GAUSS_REGIONS {
                for j in (i + 1)..=MAX_GAUSS_REGIONS {
                    let cand = all[i].merge(all[j]);
                    if cand.cap_half_angle < best_merged.cap_half_angle {
                        best_i = i;
                        best_j = j;
                        best_merged = cand;
                    }
                }
            }
            let mut out_idx = 0;
            for (k, item) in all.iter().enumerate() {
                if k == best_i {
                    self.items[out_idx] = best_merged;
                    out_idx += 1;
                } else if k != best_j {
                    self.items[out_idx] = *item;
                    out_idx += 1;
                }
            }
            self.count = MAX_GAUSS_REGIONS;
        }
    }

    /// Return the active [`GaussRegion`] slice.
    #[inline]
    pub fn regions(&self) -> &[GaussRegion] {
        &self.items[..self.count]
    }

    /// Exact point [`GaussMap`] for a single normal vector `normal` (normalized
    /// internally; zero or non-finite input yields [`GaussMap::FULL`]).
    #[inline]
    pub fn from_normal(normal: [f64; 3]) -> Self {
        let Some(n) = try_unit3(normal) else {
            return Self::FULL;
        };
        Self::single(GaussRegion::from_cap_and_zone(
            n,
            FP_ANGLE_GUARD,
            n,
            0.0,
            FP_ANGLE_GUARD,
        ))
    }

    /// Conservative union `self ∪ other`.
    pub fn union(&self, other: &Self) -> Self {
        if self.is_full() || other.is_full() {
            return Self::FULL;
        }
        if self.count == 0 {
            return *other;
        }
        let mut acc = *self;
        for r in &other.items[..other.count] {
            acc.push_region(*r);
        }
        acc
    }

    /// Yang 2025 §4.2.2 (`:442-446` R) collinearity overlap test:
    /// returns `false` ONLY when no `n_A ∈ self` and `n_B ∈ other` can be
    /// collinear (`n_A = +n_B` or `n_A = -n_B`).
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        for ra in &self.items[..self.count] {
            for rb in &other.items[..other.count] {
                if ra.overlaps_signed(rb, 1.0) || ra.overlaps_signed(rb, -1.0) {
                    return true;
                }
            }
        }
        false
    }

    /// Oriented Gauss-map overlap test (`∃ n ∈ self ∩ other` with the same
    /// sign `n_A = +n_B`).
    #[inline]
    pub fn overlaps_oriented(&self, other: &Self) -> bool {
        for ra in &self.items[..self.count] {
            for rb in &other.items[..other.count] {
                if ra.overlaps_signed(rb, 1.0) {
                    return true;
                }
            }
        }
        false
    }

    /// Conservative closed-form Gauss map over a parameter-domain rectangle
    /// `[u_range[0], u_range[1]] × [v_range[0], v_range[1]]` in the canonical
    /// surface parameterization of [`crate::stage4_dt::eval_uv`].
    pub fn from_uv_rect(surface: &Surface, u_range: [f64; 2], v_range: [f64; 2]) -> Self {
        if !u_range[0].is_finite()
            || !u_range[1].is_finite()
            || !v_range[0].is_finite()
            || !v_range[1].is_finite()
        {
            return Self::FULL;
        }
        let u0 = u_range[0].min(u_range[1]);
        let u1 = u_range[0].max(u_range[1]);
        let v0 = v_range[0].min(v_range[1]);
        let v1 = v_range[0].max(v_range[1]);

        match surface {
            Surface::Plane { normal, .. } => Self::from_normal(normal.as_array()),
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                if !radius.is_finite() || *radius <= 0.0 || !point_is_finite(axis_point.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let pad = fp_pad_for_origin(axis_point.as_array(), *radius);
                cylinder_from_u_range(e1, e2, axis, [u0, u1], pad)
            }
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                if !half_angle.is_finite()
                    || *half_angle <= 0.0
                    || *half_angle >= FRAC_PI_2
                    || !point_is_finite(apex.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let v_scale = (v0.abs().max(v1.abs()) * half_angle.tan()).max(1e-6);
                let pad = fp_pad_for_origin(apex.as_array(), v_scale);
                let apex_tol = 1e-15;
                if v0 < -apex_tol && v1 > apex_tol {
                    Self::FULL
                } else if v1 >= 0.0 && v0 <= apex_tol {
                    cone_apex_cap(axis, *half_angle, true, pad)
                } else if v0 <= 0.0 && v1 >= -apex_tol {
                    cone_apex_cap(axis, *half_angle, false, pad)
                } else if v0 > apex_tol {
                    cone_from_u_range(e1, e2, axis, *half_angle, true, [u0, u1], pad)
                } else {
                    cone_from_u_range(e1, e2, axis, *half_angle, false, [u0, u1], pad)
                }
            }
            Surface::Sphere { center, radius } => {
                if !radius.is_finite() || *radius <= 0.0 || !point_is_finite(center.as_array()) {
                    return Self::FULL;
                }
                if v0 < -FRAC_PI_2 - 1e-12 || v1 > FRAC_PI_2 + 1e-12 {
                    return Self::FULL;
                }
                let vc0 = v0.clamp(-FRAC_PI_2, FRAC_PI_2);
                let vc1 = v1.clamp(-FRAC_PI_2, FRAC_PI_2);
                let z_axis = [0.0, 0.0, 1.0];
                let (e1, e2) = basis_arrays(Vector3::new(0.0, 0.0, 1.0));
                let pad = fp_pad_for_origin(center.as_array(), *radius);
                latlon_patch_in_frame(e1, e2, z_axis, [u0, u1], [vc0, vc1], pad)
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                if !major_radius.is_finite()
                    || !minor_radius.is_finite()
                    || *minor_radius <= 0.0
                    || *major_radius <= *minor_radius
                    || !point_is_finite(center.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let pad = fp_pad_for_origin(center.as_array(), *minor_radius);
                torus_from_uv_range(e1, e2, axis, [u0, u1], [v0, v1], pad)
            }
        }
    }

    /// Conservative closed-form Gauss map of the exact-surface region covered
    /// by a 3D mesh (or leaf) triangle `tri` on `surface`.
    pub fn from_triangle(surface: &Surface, tri: [[f64; 3]; 3]) -> Self {
        if !point_is_finite(tri[0]) || !point_is_finite(tri[1]) || !point_is_finite(tri[2]) {
            return Self::FULL;
        }
        match surface {
            Surface::Plane { normal, .. } => Self::from_normal(normal.as_array()),
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                if !radius.is_finite() || *radius <= 0.0 || !point_is_finite(axis_point.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let ap = axis_point.as_array();
                let mut pts2 = [[0.0; 2]; 3];
                for k in 0..3 {
                    let w = sub3(tri[k], ap);
                    pts2[k] = [dot3(w, e1), dot3(w, e2)];
                }
                let (rho_min, rho_max, closest) = triangle_2d_origin_metrics(pts2);
                let scale = radius.max(rho_max).max(1e-15);
                if rho_min <= 1e-12 * scale {
                    let pad = fp_pad_for_origin(ap, *radius);
                    return cylinder_from_u_range(e1, e2, axis, [-PI, PI], pad);
                }
                let pad = fp_pad_for_origin(ap, rho_min);
                let u_range = unwrap_triangle_2d_angles(pts2, closest);
                cylinder_from_u_range(e1, e2, axis, u_range, pad)
            }
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                if !half_angle.is_finite()
                    || *half_angle <= 0.0
                    || *half_angle >= FRAC_PI_2
                    || !point_is_finite(apex.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let ap = apex.as_array();
                let mut pts2 = [[0.0; 2]; 3];
                let mut h = [0.0; 3];
                let mut max_dist = 0.0_f64;
                for k in 0..3 {
                    let w = sub3(tri[k], ap);
                    max_dist = max_dist.max(norm3(w));
                    h[k] = dot3(w, axis);
                    pts2[k] = [dot3(w, e1), dot3(w, e2)];
                }
                let scale = max_dist.max(1e-15);
                let tol = 1e-12 * scale;
                let h_min = h[0].min(h[1]).min(h[2]);
                let h_max = h[0].max(h[1]).max(h[2]);
                if h_min < -tol && h_max > tol {
                    return Self::FULL;
                }
                let (rho_min, _rho_max, closest) = triangle_2d_origin_metrics(pts2);
                let (sa, ca) = (half_angle.sin(), half_angle.cos());
                if h_max >= -tol && h_min >= -tol {
                    // +axis nappe: check if the triangle touches the apex,
                    // projects onto the apex (`ρ sin α + h cos α ≤ 0`), or
                    // bridges across the cone axis (`ρ_min ≤ tol`), in which
                    // case the cone region straddles the apex and carries the
                    // full polar normal cone `C(-axis, π/2 − α)`.
                    if rho_min <= tol || rho_min * sa + h_min * ca <= tol {
                        let pad = fp_pad_for_origin(ap, scale);
                        return cone_apex_cap(axis, *half_angle, true, pad);
                    }
                    let pad = fp_pad_for_origin(ap, rho_min);
                    let u_range = unwrap_triangle_2d_angles(pts2, closest);
                    cone_from_u_range(e1, e2, axis, *half_angle, true, u_range, pad)
                } else {
                    // -axis nappe.
                    if rho_min <= tol || rho_min * sa - h_max * ca <= tol {
                        let pad = fp_pad_for_origin(ap, scale);
                        return cone_apex_cap(axis, *half_angle, false, pad);
                    }
                    let pad = fp_pad_for_origin(ap, rho_min);
                    let u_range = unwrap_triangle_2d_angles(pts2, closest);
                    cone_from_u_range(e1, e2, axis, *half_angle, false, u_range, pad)
                }
            }
            Surface::Sphere { center, radius } => {
                if !radius.is_finite() || *radius <= 0.0 || !point_is_finite(center.as_array()) {
                    return Self::FULL;
                }
                let c = center.as_array();
                let rel = [sub3(tri[0], c), sub3(tri[1], c), sub3(tri[2], c)];
                let d_origin = origin_to_triangle_3d_distance(rel);
                let scale = radius
                    .max(norm3(rel[0]))
                    .max(norm3(rel[1]))
                    .max(norm3(rel[2]))
                    .max(1e-15);
                if d_origin <= 1e-12 * scale {
                    return Self::FULL;
                }
                let (Some(n0), Some(n1), Some(n2)) =
                    (try_unit3(rel[0]), try_unit3(rel[1]), try_unit3(rel[2]))
                else {
                    return Self::FULL;
                };
                let pad = fp_pad_for_origin(c, d_origin);
                sphere_geodesic_triangle_cap([n0, n1, n2], pad)
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                if !major_radius.is_finite()
                    || !minor_radius.is_finite()
                    || *minor_radius <= 0.0
                    || *major_radius <= *minor_radius
                    || !point_is_finite(center.as_array())
                {
                    return Self::FULL;
                }
                let Some(axis) = try_unit3(axis_dir.as_array()) else {
                    return Self::FULL;
                };
                let (e1, e2) = basis_arrays(*axis_dir);
                let c = center.as_array();
                let mut pts2 = [[0.0; 2]; 3];
                let mut tau = [0.0; 3];
                for k in 0..3 {
                    let w = sub3(tri[k], c);
                    pts2[k] = [dot3(w, e1), dot3(w, e2)];
                    tau[k] = dot3(w, axis);
                }
                let (rho_min, rho_max, closest) = triangle_2d_origin_metrics(pts2);
                let u_range = if rho_min <= 1e-12 * major_radius.max(rho_max).max(1e-15) {
                    [-PI, PI]
                } else {
                    unwrap_triangle_2d_angles(pts2, closest)
                };

                // Meridian profile rectangle `B_ρτ = [ρ_min − R, ρ_max − R] × [τ_min, τ_max]`.
                // By convexity of `‖·‖`, `ρ_max = max_k ρ_k` and `ρ_min = dist(0, T_2D)`
                // are exact over `T`, so every `x ∈ T` maps into `B_ρτ`.
                let d0 = rho_min - *major_radius;
                let d1 = rho_max - *major_radius;
                let t0 = tau[0].min(tau[1]).min(tau[2]);
                let t1 = tau[0].max(tau[1]).max(tau[2]);
                let spine_dist = rect_origin_dist(d0, d1, t0, t1);
                let pad_scale = spine_dist.max(1e-12 * minor_radius).min(*minor_radius);
                let pad = fp_pad_for_origin(c, pad_scale);
                let v_range = if spine_dist <= 1e-12 * minor_radius.max(1e-15) {
                    [-PI, PI]
                } else {
                    unwrap_rect_angles(d0, d1, t0, t1)
                };
                torus_from_uv_range(e1, e2, axis, u_range, v_range, pad)
            }
        }
    }

    /// Conservative closed-form Gauss map over a non-empty set of 3D mesh/leaf
    /// triangles on `surface`. An empty slice returns a non-overlapping empty
    /// [`GaussMap`].
    pub fn from_triangles(surface: &Surface, tris: &[[[f64; 3]; 3]]) -> Self {
        let mut acc = Self::empty();
        for tri in tris {
            let gm = Self::from_triangle(surface, *tri);
            acc = acc.union(&gm);
            if acc.is_full() {
                break;
            }
        }
        acc
    }
}

/// Convenience free function for §4.2.2 Gauss-map collinearity overlap.
#[inline]
pub fn gauss_maps_overlap(a: &GaussMap, b: &GaussMap) -> bool {
    a.overlaps(b)
}

// =========================================================================
// Analytic surface region builders
// =========================================================================

fn cylinder_from_u_range(
    e1: [f64; 3],
    e2: [f64; 3],
    axis: [f64; 3],
    u_range: [f64; 2],
    pad: f64,
) -> GaussMap {
    let du = u_range[1] - u_range[0];
    let z0 = (FRAC_PI_2 - pad).max(0.0);
    let z1 = (FRAC_PI_2 + pad).min(PI);
    if du >= TAU - 1e-12 {
        return GaussMap::single(GaussRegion::from_cap_and_zone(e1, PI, axis, z0, z1));
    }
    if du <= PI {
        let um = 0.5 * (u_range[0] + u_range[1]);
        let d = radial_dir(e1, e2, um);
        return GaussMap::single(GaussRegion::from_cap_and_zone(
            d,
            0.5 * du + pad,
            axis,
            z0,
            z1,
        ));
    }
    let u_mid = 0.5 * (u_range[0] + u_range[1]);
    let r0 = cylinder_from_u_range(e1, e2, axis, [u_range[0], u_mid], pad);
    let r1 = cylinder_from_u_range(e1, e2, axis, [u_mid, u_range[1]], pad);
    r0.union(&r1)
}

fn cone_apex_cap(axis: [f64; 3], half_angle: f64, pos_nappe: bool, pad: f64) -> GaussMap {
    let cap_theta = (FRAC_PI_2 - half_angle + pad).clamp(0.0, PI);
    if pos_nappe {
        let neg_axis = [-axis[0], -axis[1], -axis[2]];
        let z0 = (FRAC_PI_2 + half_angle - pad).clamp(0.0, PI);
        GaussMap::single(GaussRegion::from_cap_and_zone(
            neg_axis, cap_theta, axis, z0, PI,
        ))
    } else {
        let z1 = (FRAC_PI_2 - half_angle + pad).clamp(0.0, PI);
        GaussMap::single(GaussRegion::from_cap_and_zone(
            axis, cap_theta, axis, 0.0, z1,
        ))
    }
}

fn cone_from_u_range(
    e1: [f64; 3],
    e2: [f64; 3],
    axis: [f64; 3],
    half_angle: f64,
    pos_nappe: bool,
    u_range: [f64; 2],
    pad: f64,
) -> GaussMap {
    let du = u_range[1] - u_range[0];
    let z_center = if pos_nappe {
        FRAC_PI_2 + half_angle
    } else {
        FRAC_PI_2 - half_angle
    };
    let z0 = (z_center - pad).clamp(0.0, PI);
    let z1 = (z_center + pad).clamp(0.0, PI);

    if du >= TAU - 1e-12 {
        let cap_axis = if pos_nappe {
            [-axis[0], -axis[1], -axis[2]]
        } else {
            axis
        };
        let cap_theta = (FRAC_PI_2 - half_angle + pad).clamp(0.0, PI);
        return GaussMap::single(GaussRegion::from_cap_and_zone(
            cap_axis, cap_theta, axis, z0, z1,
        ));
    }
    if du <= PI {
        let um = 0.5 * (u_range[0] + u_range[1]);
        let r = radial_dir(e1, e2, um);
        let (ca, sa) = (half_angle.cos(), half_angle.sin());
        let ax_sign = if pos_nappe { -sa } else { sa };
        let d = unit_or(
            [
                ca * r[0] + ax_sign * axis[0],
                ca * r[1] + ax_sign * axis[1],
                ca * r[2] + ax_sign * axis[2],
            ],
            r,
        );
        let half_chord = (ca * (0.25 * du).sin()).clamp(0.0, 1.0);
        let theta = 2.0 * half_chord.asin() + pad;
        return GaussMap::single(GaussRegion::from_cap_and_zone(d, theta, axis, z0, z1));
    }
    let u_mid = 0.5 * (u_range[0] + u_range[1]);
    let r0 = cone_from_u_range(
        e1,
        e2,
        axis,
        half_angle,
        pos_nappe,
        [u_range[0], u_mid],
        pad,
    );
    let r1 = cone_from_u_range(
        e1,
        e2,
        axis,
        half_angle,
        pos_nappe,
        [u_mid, u_range[1]],
        pad,
    );
    r0.union(&r1)
}

/// Closed-form Gauss-map bound for a latitude-longitude patch
/// `n(u, v) = cos(v) * (cos(u)*e1 + sin(u)*e2) + sin(v) * axis`
/// with `v ∈ [v0, v1] ⊆ [-π/2, π/2]`.
fn latlon_patch_in_frame(
    e1: [f64; 3],
    e2: [f64; 3],
    axis: [f64; 3],
    u_range: [f64; 2],
    v_range: [f64; 2],
    pad: f64,
) -> GaussMap {
    let v0 = v_range[0].clamp(-FRAC_PI_2, FRAC_PI_2);
    let v1 = v_range[1].clamp(-FRAC_PI_2, FRAC_PI_2);
    let z0 = (FRAC_PI_2 - v1 - pad).clamp(0.0, PI);
    let z1 = (FRAC_PI_2 - v0 + pad).clamp(0.0, PI);

    let du = u_range[1] - u_range[0];
    if du >= TAU - 1e-12 {
        // Full revolution around `axis`: the polar zone `Z(axis, π/2−v1, π/2−v0)`
        // is already the exact Gauss map. Attach the tightest axial cap.
        let (cap_axis, cap_theta) = if v0 >= 0.0 {
            (axis, FRAC_PI_2 - v0 + pad)
        } else if v1 <= 0.0 {
            ([-axis[0], -axis[1], -axis[2]], FRAC_PI_2 + v1 + pad)
        } else if FRAC_PI_2 - v0 <= FRAC_PI_2 + v1 {
            (axis, FRAC_PI_2 - v0 + pad)
        } else {
            ([-axis[0], -axis[1], -axis[2]], FRAC_PI_2 + v1 + pad)
        };
        return GaussMap::single(GaussRegion::from_cap_and_zone(
            cap_axis, cap_theta, axis, z0, z1,
        ));
    }

    if du <= PI {
        let um = 0.5 * (u_range[0] + u_range[1]);
        let vm = 0.5 * (v0 + v1);
        let c_half_du = (0.5 * du).cos().max(0.0);
        // Minimax latitude `v_c` on the mid-meridian `um`:
        // `g(v_i; v_c) = cos(v_i) cos(Δu/2) cos(v_c) + sin(v_i) sin(v_c)` peaks
        // at `α_i = atan2(sin v_i, cos v_i cos(Δu/2))`, and `g(v0) = g(v1)` gives
        // `v_eq = atan2(sin(v_m) cos(Δu/2), cos(v_m))`. Clamping `v_eq` to
        // `[min(α0, α1), max(α0, α1)]` gives the exact minimax center on S².
        let alpha0 = v0.sin().atan2(v0.cos() * c_half_du);
        let alpha1 = v1.sin().atan2(v1.cos() * c_half_du);
        let v_eq = (vm.sin() * c_half_du).atan2(vm.cos());
        let vc = v_eq.clamp(alpha0.min(alpha1), alpha0.max(alpha1));

        let r = radial_dir(e1, e2, um);
        let (cvc, svc) = (vc.cos(), vc.sin());
        let d = unit_or(
            [
                cvc * r[0] + svc * axis[0],
                cvc * r[1] + svc * axis[1],
                cvc * r[2] + svc * axis[2],
            ],
            axis,
        );

        // Exact max angle to `[ua, ub] × [v0, v1]` via cancellation-free
        // half-chord `1 − n·d = 2 sin²((v − v_c)/2) + 2 cos(v) cos(v_c) sin²(Δu/4)`
        // evaluated at `v ∈ {v0, v1}` (proven unimodal over `v ∈ [-π/2, π/2]`).
        let s_qdu = (0.25 * du).sin();
        let corner_angle = |v: f64| -> f64 {
            let sv = (0.5 * (v - vc)).sin();
            let one_minus_dot =
                2.0 * sv * sv + 2.0 * v.cos().max(0.0) * cvc.max(0.0) * s_qdu * s_qdu;
            let half_chord = (0.5 * one_minus_dot).clamp(0.0, 1.0).sqrt();
            2.0 * half_chord.asin()
        };
        let theta = corner_angle(v0).max(corner_angle(v1)) + pad;
        return GaussMap::single(GaussRegion::from_cap_and_zone(d, theta, axis, z0, z1));
    }

    let u_mid = 0.5 * (u_range[0] + u_range[1]);
    let r0 = latlon_patch_in_frame(e1, e2, axis, [u_range[0], u_mid], [v0, v1], pad);
    let r1 = latlon_patch_in_frame(e1, e2, axis, [u_mid, u_range[1]], [v0, v1], pad);
    r0.union(&r1)
}

fn torus_from_uv_range(
    e1: [f64; 3],
    e2: [f64; 3],
    axis: [f64; 3],
    u_range: [f64; 2],
    v_range: [f64; 2],
    pad: f64,
) -> GaussMap {
    let dv = v_range[1] - v_range[0];
    // Normalize `[v0, v1]` into `[0, 4π]` with width `≤ 2π`.
    let (v0, v1) = if dv >= TAU - 1e-12 {
        (0.0, TAU)
    } else {
        let shift = (v_range[0] / TAU).floor() * TAU;
        let a = (v_range[0] - shift).clamp(0.0, TAU);
        (a, a + dv)
    };

    // Partition `[0, 4π]` along `cos v = 0` (`v = π/2, 3π/2, 5π/2, 7π/2`) into
    // outer-tube bands (`cos v ≥ 0`) and inner-equator bands (`cos v ≤ 0`).
    // On inner bands, `w = k·π − v ∈ [-π/2, π/2]` satisfies
    // `cos(v)·r̂(u) + sin(v)·â = cos(w)·r̂(u + π) + sin(w)·â`.
    const BANDS: [(f64, f64, bool, f64); 5] = [
        (0.0, FRAC_PI_2, true, 0.0),
        (FRAC_PI_2, 3.0 * FRAC_PI_2, false, PI),
        (3.0 * FRAC_PI_2, 5.0 * FRAC_PI_2, true, -TAU),
        (5.0 * FRAC_PI_2, 7.0 * FRAC_PI_2, false, 3.0 * PI),
        (7.0 * FRAC_PI_2, 2.0 * TAU, true, -2.0 * TAU),
    ];

    let mut acc = GaussMap::empty();
    let mut any = false;
    for &(b_lo, b_hi, is_outer, offset) in &BANDS {
        let a = v0.max(b_lo);
        let b = v1.min(b_hi);
        // Include degenerate point intervals (`v0 == v1`) on the first band
        // containing `v0`, while skipping zero-measure boundary touches of
        // non-degenerate `[v0, v1]` so adjacent bands do not emit spurious caps.
        let take = if v1 - v0 <= 1e-15 {
            !any && v0 >= b_lo && v0 <= b_hi
        } else {
            b > a + 1e-15
        };
        if !take {
            continue;
        }
        any = true;
        let part = if is_outer {
            let lat_lo = (a + offset).clamp(-FRAC_PI_2, FRAC_PI_2);
            let lat_hi = (b + offset).clamp(-FRAC_PI_2, FRAC_PI_2);
            latlon_patch_in_frame(e1, e2, axis, u_range, [lat_lo, lat_hi], pad)
        } else {
            let w_lo = (offset - b).clamp(-FRAC_PI_2, FRAC_PI_2);
            let w_hi = (offset - a).clamp(-FRAC_PI_2, FRAC_PI_2);
            latlon_patch_in_frame(
                e1,
                e2,
                axis,
                [u_range[0] + PI, u_range[1] + PI],
                [w_lo, w_hi],
                pad,
            )
        };
        acc = acc.union(&part);
    }
    if !any {
        return GaussMap::FULL;
    }
    acc
}

fn sphere_geodesic_triangle_cap(n: [[f64; 3]; 3], pad: f64) -> GaussMap {
    // By Welzl/Jung on S², the minimum enclosing cap of `{n0, n1, n2}` in an
    // open hemisphere is centered either at the spherical circumcenter or at an
    // edge midpoint (we also test the centroid for near-degenerate conditioning).
    let mut candidates = [[0.0; 3]; 5];
    let mut n_cand = 0;

    if let Some(c) = try_unit3(add3(add3(n[0], n[1]), n[2])) {
        candidates[n_cand] = c;
        n_cand += 1;
    }
    for &(i, j) in &[(0, 1), (1, 2), (2, 0)] {
        if let Some(m) = try_unit3(add3(n[i], n[j])) {
            candidates[n_cand] = m;
            n_cand += 1;
        }
    }
    let circum_raw = cross3(sub3(n[1], n[0]), sub3(n[2], n[0]));
    if let Some(mut cc) = try_unit3(circum_raw) {
        if dot3(cc, n[0]) < 0.0 {
            cc = [-cc[0], -cc[1], -cc[2]];
        }
        candidates[n_cand] = cc;
        n_cand += 1;
    }

    let mut best_axis = [0.0, 0.0, 1.0];
    let mut best_min_dot = -2.0_f64;
    for cand in &candidates[..n_cand] {
        let min_dot = dot3(*cand, n[0])
            .min(dot3(*cand, n[1]))
            .min(dot3(*cand, n[2]));
        if min_dot > best_min_dot {
            best_min_dot = min_dot;
            best_axis = *cand;
        }
    }

    // Proof of conservativeness requires all three vertices strictly inside an
    // open hemisphere (`min_k (n_k · d̂) > 0`), so `w / ‖w‖ · d̂ ≥ cos θ` holds
    // for every positive linear combination `w = ∑ λ_k ‖v_k − c‖ n_k`.
    if best_min_dot <= 1e-12 {
        return GaussMap::FULL;
    }
    let theta = unit_angle(best_axis, n[0])
        .max(unit_angle(best_axis, n[1]))
        .max(unit_angle(best_axis, n[2]))
        + pad;
    if theta >= FRAC_PI_2 {
        return GaussMap::FULL;
    }
    GaussMap::single(GaussRegion::from_cap(best_axis, theta))
}

// =========================================================================
// Low-level exact 2D / 3D geometric helpers
// =========================================================================

#[inline]
fn point_is_finite(p: [f64; 3]) -> bool {
    p[0].is_finite() && p[1].is_finite() && p[2].is_finite()
}

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

#[inline]
fn try_unit3(v: [f64; 3]) -> Option<[f64; 3]> {
    if !point_is_finite(v) {
        return None;
    }
    let len = norm3(v);
    if len > 0.0 && len.is_finite() {
        Some([v[0] / len, v[1] / len, v[2] / len])
    } else {
        None
    }
}

#[inline]
fn unit_or(v: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
    try_unit3(v).unwrap_or(fallback)
}

#[inline]
fn basis_arrays(axis_dir: Vector3) -> ([f64; 3], [f64; 3]) {
    let (e1, e2) = ortho_basis(axis_dir);
    (e1.as_array(), e2.as_array())
}

#[inline]
fn radial_dir(e1: [f64; 3], e2: [f64; 3], u: f64) -> [f64; 3] {
    let (cu, su) = (u.cos(), u.sin());
    [
        cu * e1[0] + su * e2[0],
        cu * e1[1] + su * e2[1],
        cu * e1[2] + su * e2[2],
    ]
}

/// Kahan's cancellation-free angle `∠(a, b) ∈ [0, π]` between unit vectors.
#[inline]
fn unit_angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    let diff = norm3(sub3(a, b));
    let sum = norm3(add3(a, b));
    2.0 * diff.atan2(sum)
}

#[inline]
fn interval_point_dist(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo - x
    } else if x > hi {
        x - hi
    } else {
        0.0
    }
}

/// Condition-scaled IEEE-754 angular rounding bound for subtracting a surface
/// origin `origin` from points at distance `local_scale`.
#[inline]
fn fp_pad_for_origin(origin: [f64; 3], local_scale: f64) -> f64 {
    let c_norm = norm3(origin);
    let cond = 1.0 + c_norm / local_scale.max(1e-15);
    (FP_ANGLE_GUARD * cond).min(1e-4)
}

#[inline]
fn closest_point_on_seg_2d(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if len2 <= 0.0 {
        return a;
    }
    let t = (-(a[0] * ab[0] + a[1] * ab[1]) / len2).clamp(0.0, 1.0);
    [a[0] + t * ab[0], a[1] + t * ab[1]]
}

/// Returns `(rho_min, rho_max, closest_point_to_origin)` for a 2D triangle.
fn triangle_2d_origin_metrics(pts: [[f64; 2]; 3]) -> (f64, f64, [f64; 2]) {
    let r0 = (pts[0][0] * pts[0][0] + pts[0][1] * pts[0][1]).sqrt();
    let r1 = (pts[1][0] * pts[1][0] + pts[1][1] * pts[1][1]).sqrt();
    let r2 = (pts[2][0] * pts[2][0] + pts[2][1] * pts[2][1]).sqrt();
    let rho_max = r0.max(r1).max(r2);

    let area2 = (pts[1][0] - pts[0][0]) * (pts[2][1] - pts[0][1])
        - (pts[1][1] - pts[0][1]) * (pts[2][0] - pts[0][0]);
    let c01 = pts[0][0] * pts[1][1] - pts[0][1] * pts[1][0];
    let c12 = pts[1][0] * pts[2][1] - pts[1][1] * pts[2][0];
    let c20 = pts[2][0] * pts[0][1] - pts[2][1] * pts[0][0];
    let inside = (area2 > 0.0 && c01 >= 0.0 && c12 >= 0.0 && c20 >= 0.0)
        || (area2 < 0.0 && c01 <= 0.0 && c12 <= 0.0 && c20 <= 0.0);
    if inside {
        return (0.0, rho_max, [0.0, 0.0]);
    }

    let c_edges = [
        closest_point_on_seg_2d(pts[0], pts[1]),
        closest_point_on_seg_2d(pts[1], pts[2]),
        closest_point_on_seg_2d(pts[2], pts[0]),
    ];
    let mut best_q = c_edges[0];
    let mut best_d2 = best_q[0] * best_q[0] + best_q[1] * best_q[1];
    for q in &c_edges[1..] {
        let d2 = q[0] * q[0] + q[1] * q[1];
        if d2 < best_d2 {
            best_d2 = d2;
            best_q = *q;
        }
    }
    (best_d2.sqrt(), rho_max, best_q)
}

#[inline]
fn wrap_signed_pi(angle: f64) -> f64 {
    let mut d = (angle + PI).rem_euclid(TAU) - PI;
    if d <= -PI {
        d += TAU;
    }
    d
}

/// Exact angular wedge `[u_min, u_max]` (width `< π`) of a 2D triangle whose
/// closest point to the origin `closest` is strictly non-zero: by separating
/// hyperplane `p · closest ≥ ‖closest‖² > 0`, all three vertices lie within
/// `(-π/2, π/2)` of `atan2(closest.y, closest.x)`.
fn unwrap_triangle_2d_angles(pts: [[f64; 2]; 3], closest: [f64; 2]) -> [f64; 2] {
    let ref_angle = closest[1].atan2(closest[0]);
    let mut u_min = f64::INFINITY;
    let mut u_max = f64::NEG_INFINITY;
    for p in &pts {
        let a = ref_angle + wrap_signed_pi(p[1].atan2(p[0]) - ref_angle);
        u_min = u_min.min(a);
        u_max = u_max.max(a);
    }
    [u_min, u_max]
}

#[inline]
fn rect_origin_dist(x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    let dx = interval_point_dist(0.0, x0, x1);
    let dy = interval_point_dist(0.0, y0, y1);
    (dx * dx + dy * dy).sqrt()
}

/// Exact angular wedge `[v_min, v_max]` (width `< π`) of an axis-aligned 2D
/// rectangle `[x0, x1] × [y0, y1]` strictly excluding `(0, 0)`.
fn unwrap_rect_angles(x0: f64, x1: f64, y0: f64, y1: f64) -> [f64; 2] {
    let cx = 0.0_f64.clamp(x0, x1);
    let cy = 0.0_f64.clamp(y0, y1);
    let ref_angle = cy.atan2(cx);
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut v_min = f64::INFINITY;
    let mut v_max = f64::NEG_INFINITY;
    for c in &corners {
        let a = ref_angle + wrap_signed_pi(c[1].atan2(c[0]) - ref_angle);
        v_min = v_min.min(a);
        v_max = v_max.max(a);
    }
    [v_min, v_max]
}

/// Exact Euclidean distance in `ℝ³` from `(0, 0, 0)` to triangle `[a, b, c]`.
fn origin_to_triangle_3d_distance(tri: [[f64; 3]; 3]) -> f64 {
    let [a, b, c] = tri;
    let ab = sub3(b, a);
    let ac = sub3(c, a);
    let ap = [-a[0], -a[1], -a[2]];

    let d1 = dot3(ab, ap);
    let d2 = dot3(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm3(a);
    }

    let bp = [-b[0], -b[1], -b[2]];
    let d3 = dot3(ab, bp);
    let d4 = dot3(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return norm3(b);
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denom = d1 - d3;
        let v = if denom.abs() > 0.0 { d1 / denom } else { 0.0 };
        return norm3([a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]]);
    }

    let cp = [-c[0], -c[1], -c[2]];
    let d5 = dot3(ab, cp);
    let d6 = dot3(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return norm3(c);
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denom = d2 - d6;
        let w = if denom.abs() > 0.0 { d2 / denom } else { 0.0 };
        return norm3([a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]]);
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let bc = sub3(c, b);
        let denom = (d4 - d3) + (d5 - d6);
        let w = if denom.abs() > 0.0 {
            (d4 - d3) / denom
        } else {
            0.0
        };
        return norm3([b[0] + w * bc[0], b[1] + w * bc[1], b[2] + w * bc[2]]);
    }

    // Degenerate-triangle guard: if `ab × ac ≈ 0`, fall back to the 3 edges.
    let n = cross3(ab, ac);
    let n_len2 = dot3(n, n);
    let scale2 = dot3(ab, ab).max(dot3(ac, ac));
    if n_len2 <= 1e-28 * scale2 * scale2 {
        let seg_dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
            let pq = sub3(q, p);
            let len2 = dot3(pq, pq);
            if len2 <= 0.0 {
                return norm3(p);
            }
            let t = (-dot3(p, pq) / len2).clamp(0.0, 1.0);
            norm3([p[0] + t * pq[0], p[1] + t * pq[1], p[2] + t * pq[2]])
        };
        return seg_dist(a, b).min(seg_dist(b, c)).min(seg_dist(c, a));
    }
    dot3(a, n).abs() / n_len2.sqrt()
}

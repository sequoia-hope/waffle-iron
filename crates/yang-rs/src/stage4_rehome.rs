//! §I13(f) inverted-junction-pair RE-HOMING — the f1 PLANNER
//! (spec `specs/yang_441_trim_cdt_construction.md` §I13(f)).
//!
//! An inverted junction pair is two TRUE corners on one typed conic whose
//! mesh cycle order contradicts their exact order: `j_cut` {S_i, W, K}
//! (a cone band's cut corner) and `j_rim` {S_i, S_j, W} (the band-rim ×
//! wall corner). Inversion means the cut line W∩K truly crosses the
//! NEIGHBOR band S_j — `j_cut`'s exact solve lies outside S_i's
//! rim-bounded domain (the paper's §4.3.3 rule-out clause at a junction)
//! and the corner must be RE-HOMED: kill `j_cut` (phantom) and `j_rim`
//! (on K's waste side), mint `newJ_wall` = (W∩K) ∩ S_j and `newJ_rim` =
//! (S_i∩S_j rim circle) ∩ K, splice four patch cycles.
//!
//! This module is the pure PLANNER: exact constructions + certificates,
//! no mesh mutation. Every failed certificate is a typed decline (the
//! status quo stays loud — the ring-CDT wall). Wired report-only under
//! `YANG_441_REHOME=census` from the I13d selector's `not_richer` branch;
//! the apply increments (f2/f3) splice the cycles.

use crate::geom::signed_distance_to_surface;
use crate::{Curve, Surface};
use cad_primitives::{Point3, Vector3};

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn axpy(a: f64, x: [f64; 3], y: [f64; 3]) -> [f64; 3] {
    [a * x[0] + y[0], a * x[1] + y[1], a * x[2] + y[2]]
}

/// Typed decline reasons — each names the certificate that failed. A
/// decline leaves the site to its standing loud wall; nothing is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RehomeDecline {
    /// Carriers are not the measured family shape ({cone, plane, plane}
    /// vs {cone, cone, plane} with shared {cone, plane}).
    ShapeMismatch,
    /// The two cone bands are not coaxial — no shared rim circle.
    NotCoaxial,
    /// No positive-radius rim circle between the two cones on the site's
    /// nappes (equal slopes, or the solved station has non-positive
    /// radius).
    RimDegenerate,
    /// The cut plane misses the rim circle (no real root).
    RimCutMiss,
    /// A minted junction fails its own surface residuals at the exactness
    /// band.
    MintResidual,
    /// W and K are parallel — no cut line.
    WallLineDegenerate,
    /// The cut line has no in-window root on the neighbor band's local
    /// nappe.
    WallRootMiss,
    /// Both quadratic roots land in the local window — ambiguous.
    WallRootAmbiguous,
    /// The two exact solves do not sit strictly off the rim on ONE common
    /// side — a tie (tangential contact) or split sides (not one re-homed
    /// corner). NOTE: the pair's ORDER INVERSION itself is certified by
    /// the I13d selector (pre/post pair-order flip along the typed conic)
    /// — the only production entry path to this planner. Which side of
    /// the rim belongs to which band is AUTHORED input data (the profile
    /// assignment), not derivable from the two cone surfaces alone, so
    /// the planner certifies solve CONSISTENCY and the selector holds the
    /// inversion authority; f2's apply plumb re-verifies the selector's
    /// t-params at the apply site.
    SolvesInconsistent,
}

/// The planned surgery for one inverted pair. Positions are exact-solve
/// mints; the apply increments own cycle splicing and identity-sharing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RehomePlan {
    pub j_cut: u32,
    pub j_rim: u32,
    /// (W∩K) ∩ S_j — the wall's re-homed cut corner {S_j, W, K}.
    pub new_wall: [f64; 3],
    /// (S_i∩S_j rim) ∩ K — where the cut truncates the rim {S_i, S_j, K}.
    pub new_rim: [f64; 3],
    /// Worst |signed distance| of either mint to any of its three
    /// surfaces.
    pub residual: f64,
    /// Signed distance of `j_rim` to K (reported for the waste-side
    /// analysis; the apply increment interprets the sign against the
    /// kept material).
    pub rim_side_of_cut: f64,
}

struct Frame {
    s_i: Surface,
    s_j: Surface,
    wall: Surface,
    cut: Surface,
}

/// Split the pair's carrier sets into the family frame, or decline.
fn classify(cut_carriers: &[Surface], rim_carriers: &[Surface]) -> Option<Frame> {
    let cone = |s: &Surface| matches!(s, Surface::Cone { .. });
    let plane = |s: &Surface| matches!(s, Surface::Plane { .. });
    if cut_carriers.len() != 3 || rim_carriers.len() != 3 {
        return None;
    }
    if cut_carriers.iter().filter(|s| cone(s)).count() != 1
        || cut_carriers.iter().filter(|s| plane(s)).count() != 2
        || rim_carriers.iter().filter(|s| cone(s)).count() != 2
        || rim_carriers.iter().filter(|s| plane(s)).count() != 1
    {
        return None;
    }
    let s_i = *cut_carriers
        .iter()
        .find(|s| cone(s) && rim_carriers.contains(s))?;
    let wall = *cut_carriers
        .iter()
        .find(|s| plane(s) && rim_carriers.contains(s))?;
    let cut = *cut_carriers
        .iter()
        .find(|s| plane(s) && !rim_carriers.contains(s))?;
    let s_j = *rim_carriers.iter().find(|s| cone(s) && **s != s_i)?;
    Some(Frame {
        s_i,
        s_j,
        wall,
        cut,
    })
}

/// Plan the corner re-homing for one refused pair. `p_cut` / `p_rim` are
/// the pair's POST-relocation mesh positions (each already on its own
/// exact triple).
pub(crate) fn plan_corner_rehoming(
    j_cut: u32,
    p_cut: [f64; 3],
    cut_carriers: &[Surface],
    j_rim: u32,
    p_rim: [f64; 3],
    rim_carriers: &[Surface],
) -> Result<RehomePlan, RehomeDecline> {
    // The census measured the family's shape as invariant (228/228 on
    // R0003); anything else is a different animal and declines.
    let (frame, j_cut, p_cut, j_rim, p_rim) = match classify(cut_carriers, rim_carriers) {
        Some(f) => (f, j_cut, p_cut, j_rim, p_rim),
        // The caller does not know which end is which — accept the
        // swapped orientation before declining.
        None => match classify(rim_carriers, cut_carriers) {
            Some(f) => (f, j_rim, p_rim, j_cut, p_cut),
            None => return Err(RehomeDecline::ShapeMismatch),
        },
    };
    let (
        Surface::Cone {
            apex: apex_i,
            axis_dir: axis_i,
            half_angle: ha_i,
        },
        Surface::Cone {
            apex: apex_j,
            axis_dir: axis_j,
            half_angle: ha_j,
        },
    ) = (frame.s_i, frame.s_j)
    else {
        unreachable!("classify narrowed the cones");
    };
    let scale = p_cut
        .iter()
        .chain(p_rim.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let band = cad_primitives::TAU_EVAL * (1.0 + scale);

    // --- coaxiality -----------------------------------------------------
    let u_i = {
        let a = axis_i.as_array();
        let l = norm(a);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let u_j = {
        let a = axis_j.as_array();
        let l = norm(a);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    // |u_i × u_j| = sin of the axis angle; the revolve mints one axis, so
    // anything above evaluation noise is a different configuration.
    if norm(cross(u_i, u_j)) > 1e-9 {
        return Err(RehomeDecline::NotCoaxial);
    }
    // Apex offset must lie ON the shared axis.
    let d_apex = sub(apex_j.as_array(), apex_i.as_array());
    let off_axis = sub(d_apex, axpy(dot(d_apex, u_i), u_i, [0.0; 3]));
    if norm(off_axis) > band {
        return Err(RehomeDecline::NotCoaxial);
    }

    // --- the shared rim circle (closed form on the site's nappes) -------
    // Axial coordinate s along u_i measured from apex_i; the local nappe
    // of each cone at the SITE fixes the sign of (s − a_x)·tan(α_x).
    let a_i = 0.0f64;
    let a_j = dot(d_apex, u_i);
    let s_site = dot(sub(p_rim, apex_i.as_array()), u_i);
    let (t_i, t_j) = (ha_i.tan(), ha_j.tan());
    let eps_i = if s_site - a_i >= 0.0 { 1.0 } else { -1.0 };
    let eps_j = if s_site - a_j >= 0.0 { 1.0 } else { -1.0 };
    // ε_i(s−a_i)t_i = ε_j(s−a_j)t_j  ⇒  s(ε_i t_i − ε_j t_j) = ε_i a_i t_i − ε_j a_j t_j
    let denom = eps_i * t_i - eps_j * t_j;
    if denom.abs() * (1.0 + scale) <= band {
        return Err(RehomeDecline::RimDegenerate);
    }
    let s_rim = (eps_i * a_i * t_i - eps_j * a_j * t_j) / denom;
    let r_rim = eps_i * (s_rim - a_i) * t_i;
    if !(r_rim.is_finite() && r_rim > band) {
        return Err(RehomeDecline::RimDegenerate);
    }
    let rim_center = Point3::from(axpy(s_rim, u_i, apex_i.as_array()));
    let rim = Curve::Circle {
        center: rim_center,
        normal: Vector3::from(u_i),
        radius: r_rim,
    };

    // --- newJ_rim = rim ∩ K, nearest the site ---------------------------
    let Surface::Plane {
        normal: n_k,
        d: d_k,
    } = frame.cut
    else {
        unreachable!("classify narrowed the cut plane");
    };
    let mid_site = Point3::new(
        0.5 * (p_cut[0] + p_rim[0]),
        0.5 * (p_cut[1] + p_rim[1]),
        0.5 * (p_cut[2] + p_rim[2]),
    );
    let Some(new_rim) =
        crate::stage4_boundary_curve::circle_plane_nearest_root(&rim, n_k, d_k, mid_site)
    else {
        return Err(RehomeDecline::RimCutMiss);
    };
    let new_rim = new_rim.as_array();

    // --- newJ_wall = (W∩K line) ∩ S_j on the local nappe ----------------
    let Surface::Plane {
        normal: n_w,
        d: d_w,
    } = frame.wall
    else {
        unreachable!("classify narrowed the wall plane");
    };
    let (nw, nk) = (n_w.as_array(), n_k.as_array());
    let dir = cross(nw, nk);
    let dl = norm(dir);
    if dl <= 1e-12 * norm(nw) * norm(nk) {
        return Err(RehomeDecline::WallLineDegenerate);
    }
    let dir = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
    // Point on the line: x0 = α·nw + β·nk with the 2×2 normal system.
    let (g11, g12, g22) = (dot(nw, nw), dot(nw, nk), dot(nk, nk));
    let det = g11 * g22 - g12 * g12;
    let (rhs1, rhs2) = (-d_w, -d_k);
    let alpha = (rhs1 * g22 - rhs2 * g12) / det;
    let beta = (rhs2 * g11 - rhs1 * g12) / det;
    let x0 = axpy(alpha, nw, axpy(beta, nk, [0.0; 3]));
    // Slide the anchor to the closest line point to the site, and search a
    // derived local window: the defect lives at the pair's own scale.
    let x_near = axpy(dot(sub(p_rim, x0), dir), dir, x0);
    let win = 16.0 * norm(sub(p_cut, p_rim)) + 1e3 * band;
    let (p0, p1) = (axpy(-win, dir, x_near), axpy(win, dir, x_near));
    let Some(roots) = crate::stage4_phantom::segment_surface_roots(p0, p1, frame.s_j) else {
        return Err(RehomeDecline::WallRootMiss);
    };
    // Local-nappe filter, then in-window (t ∈ [0,1] spans the window by
    // construction).
    let on_nappe = |t: f64| -> Option<[f64; 3]> {
        let p = axpy(t, sub(p1, p0), p0);
        // Apex_j-relative station (eps_j was derived from the site's
        // apex_j-relative station, so no further shift).
        let s_p = dot(sub(p, apex_j.as_array()), u_i);
        (eps_j * s_p >= 0.0).then_some(p)
    };
    let cands: Vec<[f64; 3]> = roots
        .iter()
        .filter(|t| (0.0..=1.0).contains(*t))
        .filter_map(|&t| on_nappe(t))
        .collect();
    let new_wall = match cands.len() {
        0 => return Err(RehomeDecline::WallRootMiss),
        1 => cands[0],
        _ => return Err(RehomeDecline::WallRootAmbiguous),
    };

    // --- solve-consistency certificate -----------------------------------
    // Both the phantom's own exact solve and the re-homed wall corner must
    // sit strictly off the rim on ONE common side (the S_j side, by the
    // selector-certified inversion this planner is entered under). See
    // `SolvesInconsistent` for the authority split.
    let station = |p: [f64; 3]| dot(sub(p, apex_i.as_array()), u_i);
    let (st_cut, st_wall) = (station(p_cut) - s_rim, station(new_wall) - s_rim);
    if !(st_cut.abs() > band && st_wall.abs() > band && st_cut.signum() == st_wall.signum()) {
        return Err(RehomeDecline::SolvesInconsistent);
    }

    // --- mint residuals (exactness, not acceptance: fail ⇒ decline) -----
    let mut worst = 0.0f64;
    let mut check = |p: [f64; 3], surfs: &[Surface]| -> bool {
        for s in surfs {
            match signed_distance_to_surface(*s, Point3::from(p)) {
                Ok(d) => worst = worst.max(d.abs()),
                Err(_) => return false,
            }
        }
        true
    };
    if !check(new_rim, &[frame.s_i, frame.s_j, frame.cut])
        || !check(new_wall, &[frame.s_j, frame.wall, frame.cut])
        || worst > band
    {
        return Err(RehomeDecline::MintResidual);
    }
    let rim_side_of_cut = dot(nk, p_rim) + d_k;
    Ok(RehomePlan {
        j_cut,
        j_rim,
        new_wall,
        new_rim,
        residual: worst,
        rim_side_of_cut,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::{Point3, Vector3};

    // Synthetic corner: coaxial cones about +z, wall x = 1.5, and a cut
    // plane whose W∩K line crosses the NEIGHBOR band just past the rim —
    // the measured inverted configuration.
    fn cone(apex_z: f64, half_angle: f64) -> Surface {
        Surface::Cone {
            apex: Point3::new(0.0, 0.0, apex_z),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle,
        }
    }
    const HA_I: f64 = 0.4;
    const HA_J: f64 = 0.3;

    /// Rim of cone(0, HA_I) vs cone(-2, HA_J): s·tan(HA_I) = (s+2)·tan(HA_J).
    fn rim() -> (f64, f64) {
        let (ti, tj) = (HA_I.tan(), HA_J.tan());
        let s = 2.0 * tj / (ti - tj);
        (s, s * ti)
    }

    fn wall() -> Surface {
        Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -1.5,
        }
    }

    /// A cut plane through the point of the W∩K line at `y_hit` on the
    /// wall, containing direction `dir` — constructed so the line W∩K is
    /// exactly {x = 1.5, the (y, z) line through (y_hit, z_hit) along
    /// (1, mz)}.
    fn cut_through(y_hit: f64, z_hit: f64, mz: f64) -> Surface {
        // Line direction in the wall: (0, 1, mz). K contains the line and
        // the tilt vector (1, 0, 0.25); normal = dir × tilt.
        let dir = [0.0, 1.0, mz];
        let tilt = [1.0, 0.0, 0.25];
        let n = [
            dir[1] * tilt[2] - dir[2] * tilt[1],
            dir[2] * tilt[0] - dir[0] * tilt[2],
            dir[0] * tilt[1] - dir[1] * tilt[0],
        ];
        let p = [1.5, y_hit, z_hit];
        Surface::Plane {
            normal: Vector3::new(n[0], n[1], n[2]),
            d: -(n[0] * p[0] + n[1] * p[1] + n[2] * p[2]),
        }
    }

    /// The wall-hyperbola point of a cone at wall-y `y` (upper nappe).
    fn z_on_wall(apex_z: f64, ha: f64, y: f64) -> f64 {
        apex_z + (1.5f64 * 1.5 + y * y).sqrt() / ha.tan()
    }

    /// Build the measured pair: j_rim = rim × wall (exact), j_cut = the
    /// (out-of-band) W∩K ∩ S_i solve.
    fn fixture(cut: &Surface) -> ([f64; 3], [f64; 3]) {
        let (s_rim, r_rim) = rim();
        let y_rim = (r_rim * r_rim - 1.5 * 1.5).sqrt();
        let p_rim = [1.5, y_rim, s_rim];
        // j_cut: intersect the W∩K line with S_i by the same quadratic the
        // planner uses (independence is not the point of the fixture — the
        // POSITION is, and the planner never reads j_cut's position except
        // for the order certificate).
        let Surface::Plane { normal, d } = *cut else {
            unreachable!()
        };
        let nw = [1.0, 0.0, 0.0];
        let nk = normal.as_array();
        let dir = [
            nw[1] * nk[2] - nw[2] * nk[1],
            nw[2] * nk[0] - nw[0] * nk[2],
            nw[0] * nk[1] - nw[1] * nk[0],
        ];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let dir = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
        let x0 = {
            // Point on both planes near the rim: x = 1.5; solve K at y_rim.
            let y = y_rim;
            let z = -(d + nk[0] * 1.5 + nk[1] * y) / nk[2];
            [1.5, y, z]
        };
        let seg = |t: f64| [x0[0] + t * dir[0], x0[1] + t * dir[1], x0[2] + t * dir[2]];
        let roots =
            crate::stage4_phantom::segment_surface_roots(seg(-10.0), seg(10.0), cone(0.0, HA_I))
                .expect("cone quadratic");
        let t = roots
            .iter()
            .copied()
            .filter(|t| (0.0..=1.0).contains(t))
            .min_by(|a, b| {
                let pa = seg(-10.0 + a * 20.0);
                let pb = seg(-10.0 + b * 20.0);
                let da = (pa[1] - y_rim).abs();
                let db = (pb[1] - y_rim).abs();
                da.total_cmp(&db)
            })
            .expect("an S_i root near the site");
        let p_cut = seg(-10.0 + t * 20.0);
        (p_cut, p_rim)
    }

    fn carriers(cut: &Surface) -> (Vec<Surface>, Vec<Surface>) {
        (
            vec![cone(0.0, HA_I), wall(), *cut],
            vec![cone(0.0, HA_I), cone(-2.0, HA_J), wall()],
        )
    }

    #[test]
    fn inverted_pair_produces_a_certified_plan() {
        let (s_rim, r_rim) = rim();
        let y_rim = (r_rim * r_rim - 1.5f64 * 1.5).sqrt();
        // Cut line crossing the NEIGHBOR band just past the rim: hit the
        // S_j wall-hyperbola 0.05 beyond y_rim, sloped along S_j's local
        // tangent so the S_i crossing lands beyond the rim too.
        let y_hit = y_rim + 0.05;
        let z_hit = z_on_wall(-2.0, HA_J, y_hit);
        let slope = (z_on_wall(-2.0, HA_J, y_hit + 0.01) - z_on_wall(-2.0, HA_J, y_hit - 0.01))
            / 0.02
            + 0.6;
        let cut = cut_through(y_hit, z_hit, slope);
        let (p_cut, p_rim) = fixture(&cut);
        let (cc, rc) = carriers(&cut);
        let plan = plan_corner_rehoming(7, p_cut, &cc, 9, p_rim, &rc)
            .expect("the inverted configuration must plan");
        // Both mints exact on their three surfaces.
        assert!(plan.residual <= 1e-9, "residual {:.3e}", plan.residual);
        for (p, surfs) in [
            (plan.new_rim, vec![cone(0.0, HA_I), cone(-2.0, HA_J), cut]),
            (plan.new_wall, vec![cone(-2.0, HA_J), wall(), cut]),
        ] {
            for s in surfs {
                let d = signed_distance_to_surface(s, Point3::from(p)).unwrap();
                assert!(d.abs() <= 1e-9, "mint off-surface by {d:.3e}");
            }
        }
        // The re-homed wall corner sits past the rim (on the same side as
        // the phantom's out-of-band solve).
        assert!((plan.new_wall[2] - s_rim).signum() == (p_cut[2] - s_rim).signum());
        assert_eq!((plan.j_cut, plan.j_rim), (7, 9));
    }

    #[test]
    fn swapped_argument_order_still_classifies() {
        let (_, r_rim) = rim();
        let y_rim = (r_rim * r_rim - 1.5f64 * 1.5).sqrt();
        let y_hit = y_rim + 0.05;
        let z_hit = z_on_wall(-2.0, HA_J, y_hit);
        let slope = (z_on_wall(-2.0, HA_J, y_hit + 0.01) - z_on_wall(-2.0, HA_J, y_hit - 0.01))
            / 0.02
            + 0.6;
        let cut = cut_through(y_hit, z_hit, slope);
        let (p_cut, p_rim) = fixture(&cut);
        let (cc, rc) = carriers(&cut);
        let plan = plan_corner_rehoming(9, p_rim, &rc, 7, p_cut, &cc)
            .expect("classification is orientation-free");
        assert_eq!((plan.j_cut, plan.j_rim), (7, 9));
    }

    #[test]
    fn tangential_tie_declines_solves_inconsistent() {
        // A cut line passing exactly THROUGH the rim × wall point: both
        // solves land ON the rim (a tie) — tangential contact, not one
        // re-homed corner.
        let (s_rim, r_rim) = rim();
        let y_rim = (r_rim * r_rim - 1.5f64 * 1.5).sqrt();
        let cut = cut_through(y_rim, s_rim, 0.9);
        let p_rim = [1.5, y_rim, s_rim];
        let (cc, rc) = carriers(&cut);
        match plan_corner_rehoming(7, p_rim, &cc, 9, p_rim, &rc) {
            Err(RehomeDecline::SolvesInconsistent) | Err(RehomeDecline::WallRootAmbiguous) => {}
            other => panic!("a rim-tie must decline, got {other:?}"),
        }
    }

    #[test]
    fn shape_mismatch_declines() {
        let cut = cut_through(2.0, 8.0, 1.0);
        let (p_cut, p_rim) = fixture(&cut);
        let sphere = Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        let cc = vec![sphere, wall(), cut];
        let (_, rc) = carriers(&cut);
        assert_eq!(
            plan_corner_rehoming(7, p_cut, &cc, 9, p_rim, &rc).unwrap_err(),
            RehomeDecline::ShapeMismatch
        );
    }

    #[test]
    fn non_coaxial_declines() {
        let cut = cut_through(2.0, 8.0, 1.0);
        let (p_cut, p_rim) = fixture(&cut);
        let tilted = Surface::Cone {
            apex: Point3::new(0.0, 0.0, -2.0),
            axis_dir: Vector3::new(0.05, 0.0, 1.0),
            half_angle: HA_J,
        };
        let cc = vec![cone(0.0, HA_I), wall(), cut];
        let rc = vec![cone(0.0, HA_I), tilted, wall()];
        assert_eq!(
            plan_corner_rehoming(7, p_cut, &cc, 9, p_rim, &rc).unwrap_err(),
            RehomeDecline::NotCoaxial
        );
    }

    #[test]
    fn parallel_wall_cut_declines() {
        let cut = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -1.6,
        };
        let (s_rim, r_rim) = rim();
        let y_rim = (r_rim * r_rim - 1.5f64 * 1.5).sqrt();
        let p_rim = [1.5, y_rim, s_rim];
        let p_cut = [1.6, y_rim + 0.05, s_rim + 0.1];
        let (mut cc, rc) = carriers(&cut);
        cc[2] = cut;
        assert_eq!(
            plan_corner_rehoming(7, p_cut, &cc, 9, p_rim, &rc).unwrap_err(),
            RehomeDecline::WallLineDegenerate
        );
    }
}

#[cfg(test)]
mod anchor_debug {
    use super::*;
    use cad_primitives::{Point3, Vector3};

    /// R0003 f903's anchor pair, surfaces + positions verbatim from the
    /// `YANG_441_RUN_PROBE_AT` legend (2026-08-28) — the planner must plan
    /// here; a decline reproduces the measured WallRootMiss for debugging.
    #[test]
    fn r0003_anchor_pair_plans() {
        let s0 = Surface::Cone {
            apex: Point3::new(-171.67927599191088, -311.12544513175254, -6.187521433974531),
            axis_dir: Vector3::new(0.3922517812997728, -0.0, -0.9198578912349208),
            half_angle: 1.5350424799485511,
        };
        let s3 = Surface::Cone {
            apex: Point3::new(-176.10284610168935, -311.12544513175254, 4.186060196241023),
            axis_dir: Vector3::new(0.3922517812997728, -0.0, -0.9198578912349208),
            half_angle: 1.4789137519774538,
        };
        let wall = Surface::Plane {
            normal: Vector3::new(
                -0.8960367765914382,
                0.22610263721415896,
                -0.3820938267499592,
            ),
            d: -163.12507508608314,
        };
        let cut = Surface::Plane {
            normal: Vector3::new(
                0.2446671128926911,
                0.9696071389324417,
                7.211501756381181e-17,
            ),
            d: 159.3587874585142,
        };
        let p_cut = [-199.747589833, -113.950400023, -25.931783828]; // v8413 {S3,W,K}
        let p_rim = [-199.737602242, -113.910084342, -25.931348769]; // v8398 {S0,S3,W}
        let cc = vec![s3, wall, cut];
        let rc = vec![s0, s3, wall];
        match plan_corner_rehoming(8413, p_cut, &cc, 8398, p_rim, &rc) {
            Ok(p) => {
                assert!(p.residual <= 1e-6, "residual {:.3e}", p.residual);
                // The re-homed wall corner sits within the pair's own scale
                // of the site.
                let d = ((p.new_wall[0] - p_rim[0]).powi(2)
                    + (p.new_wall[1] - p_rim[1]).powi(2)
                    + (p.new_wall[2] - p_rim[2]).powi(2))
                .sqrt();
                assert!(d < 1.0, "new_wall {d:.3} from the site");
            }
            Err(e) => panic!("the anchor pair must plan, got {e:?}"),
        }
    }
}

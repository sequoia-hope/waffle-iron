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
    /// f2: the selector's plumbed t-params no longer certify the order
    /// inversion at the apply site — the plumb itself failed, not the
    /// geometry.
    InversionUnverified,
    /// f2: no mesh vertex carries exactly the rim×cut triple {S_i, S_j, K}
    /// near the planned `new_rim` — the junction the cone-side chains must
    /// truncate at does not exist yet (an f3 MINT case; f2 declines loudly).
    RimNotRecognized,
    /// f2: two or more mesh vertices carry the rim×cut triple near the
    /// planned `new_rim` — identity is ambiguous, never guessed.
    RimAmbiguous,
    /// f2: re-homing the cut corner would flip (or degenerate) the
    /// orientation of a mesh triangle that rides along with the moved
    /// vertex outside the rebuilt fans.
    TriangleFlip,
    /// f2b: no kept patch on the view's cut surface has the recognized
    /// rim junction on a boundary cycle — the corner-locality requirement
    /// of the material test has no witness patch.
    CutPatchAbsent,
    /// f2b: a corner patch on the cut surface has incoherent or
    /// degenerate result-outward winding (planarity ratio, zero area, or
    /// two corner patches disagreeing in sense) — the material
    /// orientation cannot be read.
    CutPatchWindingDegenerate,
    /// f2b: the victim's material margin against the oriented cut plane
    /// is within the evaluation-noise floor — the side is undecidable.
    MaterialMarginDegenerate,
    /// f2c: a site qualified as both-corners-kept but did not present
    /// exactly TWO material-kept views sharing the phantom — the family
    /// signature (measured 2026-08-28: every corner presents both) is
    /// absent, so the corrected surgery does not know the second corner.
    NotAKeptPair,
    /// f2c: the two mirrored views' independently-planned mints disagree
    /// beyond the evaluation-noise floor — one site, two surgeries; never
    /// guessed between.
    MintMismatch,
    /// f2c: a view's kept-edge (the recognized junction's far chain edge
    /// on the view's cut patch, with the mint interposed) could not be
    /// resolved — the chain-split/insert target is unknown.
    KeptEdgeUnresolved,
    /// f2c: the phantom's S_i fan is not held by exactly one S_i-surface
    /// patch, or its deletion link's ends are not exactly the two true
    /// corners — the fossil sliver is not the measured shape.
    SiFanUnresolved,
    /// f2c: no unique S_j-fragment boundary edge at the view's kept
    /// corner carries the view's kept conic with the mint's parameter
    /// interposed — the seam-insert target is unknown.
    InsertEdgeUnresolved,
    /// f2c: some S_j-surface patch already holds the phantom — the
    /// measured precondition (the join is an INSERT) does not hold here.
    AlreadyJoined,
    /// f2c-2: the S_i-side hole re-fill could not be certified — the kept
    /// corner does not interpose on the dropped corner's rim chord (the
    /// measured overshoot anatomy is absent), the chord's surviving user is
    /// not the dropped view's fragment, the chart CDT of the link polygon
    /// refused, or the bite triangle's orientation is degenerate.
    HoleFillUnresolved,
    /// f2c-2: a view's own-plane corner absorb could not be certified —
    /// the view's wall surface has no unique patch holding its corner, or
    /// the re-homed fan rebuild refused.
    PlaneAbsorbUnresolved,
}

/// §I13(f) f2 gate — `YANG_441_REHOME`. Unset/other = Off (the arm does
/// not run; the f1 report-only planner print in the selector's richer
/// closure keys on `census` separately). `census` = the arm runs its full
/// certificate chain (plan → inversion re-verify → rim recognition → fan
/// planning → flip guard) and REPORTS, applying nothing. `on` = apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RehomeMode {
    Off,
    Census,
    On,
}

pub(crate) fn rehome_mode() -> RehomeMode {
    match std::env::var("YANG_441_REHOME") {
        Ok(v) if v == "census" => RehomeMode::Census,
        Ok(v) if v == "on" || v == "1" => RehomeMode::On,
        _ => RehomeMode::Off,
    }
}

/// §I13(f) f2 — re-verify the SELECTOR's order-inversion certificate from
/// the exact t-params it plumbed through ([`RehomeCandidate`]): the sided
/// pre and post parameter differences of the pair along its typed conic
/// must be nonzero and of OPPOSITE sign (the selector's own `flipped`
/// test, same wrap convention). The selector stays the inversion
/// authority; this guards the plumb, not the math.
pub(crate) fn inversion_still_holds(c: &crate::stage4_fold_risk::RehomeCandidate) -> bool {
    let wrap = |mut d: f64| -> f64 {
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        d
    };
    let periodic = crate::stage4_correct::conic_param_periodic(&c.curve);
    let sided = |t: f64, tref: f64| -> f64 {
        if periodic {
            wrap(t - tref)
        } else {
            t - tref
        }
    };
    let (dq, dp) = (sided(c.t_post_v, c.t_post_j), sided(c.t_pre_v, c.t_pre_j));
    dq != 0.0 && dp != 0.0 && (dq > 0.0) != (dp > 0.0)
}

/// §I13(f) f2 — recognize the EXISTING rim×cut junction by carrier
/// IDENTITY (the BINDING junction contract's identity rule, `docs/
/// yang_junction_research_findings.md`): among the pre-filtered mesh
/// vertices `cands` (id, measured carrier set), exactly ONE must carry
/// exactly the triple {S_i, S_j, K}. The caller's geometric pre-filter is
/// a search window, not an acceptance band — identity is the certificate.
pub(crate) fn recognize_rim_junction(
    cands: &[(u32, Vec<Surface>)],
    target: &[Surface; 3],
) -> Result<u32, RehomeDecline> {
    let hits: Vec<u32> = cands
        .iter()
        .filter(|(_, cs)| cs.len() == 3 && target.iter().all(|t| cs.contains(t)))
        .map(|&(v, _)| v)
        .collect();
    match hits[..] {
        [] => Err(RehomeDecline::RimNotRecognized),
        [v] => Ok(v),
        _ => Err(RehomeDecline::RimAmbiguous),
    }
}

/// §I13(f) f2 — the mint-interposition test: on the kept edge's typed
/// conic, does the mint's parameter `t_j` fall strictly BETWEEN the kept
/// junction's `t_r` and its far neighbor's `t_w`? (Wrapped deltas for
/// periodic conics — the standing chord-subtends-<π convention.) `None`
/// = undecidable (a zero step): the caller declines loudly rather than
/// guessing.
pub(crate) fn mint_interposes(curve: &Curve, t_r: f64, t_j: f64, t_w: f64) -> Option<bool> {
    let wrap = |mut d: f64| -> f64 {
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        d
    };
    let periodic = crate::stage4_correct::conic_param_periodic(curve);
    let delta = |a: f64, b: f64| -> f64 {
        if periodic {
            wrap(b - a)
        } else {
            b - a
        }
    };
    let (d1, d2) = (delta(t_r, t_j), delta(t_j, t_w));
    if d1 == 0.0 || d2 == 0.0 {
        return None;
    }
    Some((d1 > 0.0) == (d2 > 0.0))
}

/// §I13(f) f2b — the material verdict for one view: `margin > 0` means
/// the view's victim sits strictly in VOID at corner scale (absorbing it
/// is sound); `margin < 0` means it sits strictly against kept material
/// (the view is the wrong mirror). `floor` is the degeneracy guard the
/// margin cleared.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MaterialVerdict {
    pub margin: f64,
    pub floor: f64,
    /// Corner witness patches on the cut surface (all sense-consistent).
    pub cut_patches: usize,
}

/// §I13(f) f2b — the kept/waste VIEW DISCRIMINATOR (material evidence).
///
/// A site's two mirrored views are symmetric under every pair-local
/// order/side test (censuses 3–8, 2026-08-28): the defective chains
/// fossilize the arrangement's PRE-relocation order, so mesh-anchored
/// walks are circular and raw plane sides are uninterpreted. The bit
/// that separates the views — which old corner is WASTE — is stage-2
/// LABEL information, and the surviving mesh still carries it: the
/// in/out classification plus the per-op keep flip
/// ([`crate::boolean::flip_for_op`], mirroring Cherchi 2022 §5's
/// `booleans.cpp`) leave every kept triangle RESULT-outward oriented, so
/// a kept patch's winding names the void side of its carrier surface.
/// That orientation is bulk material data a corner-order defect cannot
/// touch.
///
/// The test: the view claims its victim `j_rim` = {S_i, S_j, wall} lies
/// beyond the view's CUT plane. Witnesses are the kept patch(es) ON the
/// cut surface whose boundary cycle contains the view's recognized rim
/// junction — the corner-locality requirement (the census-3 refuted
/// discriminator failed on FAR samples against the planes' infinite
/// extensions; here one corner-scale point is tested against a face
/// whose kept footprint provably reaches this corner). Each witness must
/// read as a coherent planar patch (|Σ signed area·n̂| ≥ ½ Σ |area| —
/// an internal-consistency requirement, not a geometric acceptance
/// band), and all witnesses must agree in sense; `margin` =
/// sense × sdist(victim)/|n|. The floor is the evaluation-noise model
/// (`TAU_EVAL·(1+scale)`, small multiplier) — a degeneracy STOP
/// converting an unreadable side into a loud decline, never a band that
/// admits a case.
pub(crate) fn view_material_verdict(
    mesh: &crate::Mesh,
    patches: &[crate::stage4_splice::SplicePatch],
    cut: &Surface,
    rim_j: u32,
    victim: [f64; 3],
) -> Result<MaterialVerdict, RehomeDecline> {
    let Surface::Plane { normal, d } = *cut else {
        return Err(RehomeDecline::ShapeMismatch);
    };
    let n = normal.as_array();
    let n_len = norm(n);
    if !(n_len.is_finite() && n_len > 0.0) {
        return Err(RehomeDecline::ShapeMismatch);
    }
    let n_hat = [n[0] / n_len, n[1] / n_len, n[2] / n_len];
    let mut sense: Option<f64> = None;
    let mut witnesses = 0usize;
    for pat in patches {
        if pat.surface != *cut || !pat.cycles.iter().any(|c| c.contains(&rim_j)) {
            continue;
        }
        witnesses += 1;
        // Result-outward winding vs the plane normal, with a planarity
        // coherence requirement: a folded or sliver-only patch cannot
        // orient the material side.
        let mut signed = 0.0f64;
        let mut total = 0.0f64;
        for &t in &pat.tris {
            let tri = mesh.tris[t as usize];
            let p = |v: u32| mesh.verts[v as usize].as_array();
            let (p0, p1, p2) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let av = cross(sub(p1, p0), sub(p2, p0));
            signed += dot(av, n_hat);
            total += norm(av);
        }
        if !(total.is_finite() && total > 0.0 && signed.abs() >= 0.5 * total) {
            return Err(RehomeDecline::CutPatchWindingDegenerate);
        }
        let s = signed.signum();
        if *sense.get_or_insert(s) != s {
            return Err(RehomeDecline::CutPatchWindingDegenerate);
        }
    }
    let Some(sense) = sense else {
        return Err(RehomeDecline::CutPatchAbsent);
    };
    let sdist = (dot(n, victim) + d) / n_len;
    let scale = victim.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
    let floor = 64.0 * cad_primitives::TAU_EVAL * (1.0 + scale);
    let margin = sense * sdist;
    if !(margin.is_finite() && margin.abs() > floor) {
        return Err(RehomeDecline::MaterialMarginDegenerate);
    }
    Ok(MaterialVerdict {
        margin,
        floor,
        cut_patches: witnesses,
    })
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
    /// The shared S_i∩S_j rim circle (the planner's own exact solve) — the
    /// f2c-2 hole re-fill constructs the true rim polyline on it.
    pub rim: Curve,
    /// The classified frame, for the apply increments: `s_i` the phantom
    /// band, `s_j` the neighbor band, `wall` the shared plane W, `cut` the
    /// cut plane K. The rim×cut junction's identity triple is
    /// {`s_i`, `s_j`, `cut`}.
    pub s_i: Surface,
    pub s_j: Surface,
    /// (Consumed by the f3 four-cycle surgery; pinned by the planner's unit
    /// tests until then.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub wall: Surface,
    pub cut: Surface,
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
        rim,
        s_i: frame.s_i,
        s_j: frame.s_j,
        wall: frame.wall,
        cut: frame.cut,
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
        // The classified frame rides on the plan for the apply increments.
        assert_eq!(
            (plan.s_i, plan.s_j, plan.wall, plan.cut),
            (cone(0.0, HA_I), cone(-2.0, HA_J), wall(), cut)
        );
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

    // ---- f2 apply-side certificates ------------------------------------

    fn cand(
        curve: crate::Curve,
        t_pre_j: f64,
        t_post_j: f64,
        t_pre_v: f64,
        t_post_v: f64,
    ) -> crate::stage4_fold_risk::RehomeCandidate {
        crate::stage4_fold_risk::RehomeCandidate {
            j: 1,
            v: 2,
            curve,
            t_pre_j,
            t_post_j,
            t_pre_v,
            t_post_v,
        }
    }

    fn unit_circle() -> crate::Curve {
        crate::Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    #[test]
    fn inversion_reverify_certifies_opposite_signs_only() {
        // Open conic, raw diffs: pre v ahead of j, post v behind — inverted.
        let c = cand(crate::Curve::LineSegment, 0.0, 0.0, 0.1, -0.02);
        assert!(inversion_still_holds(&c));
        // Same order on both sides — no inversion.
        let c = cand(crate::Curve::LineSegment, 0.0, 0.0, 0.1, 0.02);
        assert!(!inversion_still_holds(&c));
        // A zero diff cannot certify.
        let c = cand(crate::Curve::LineSegment, 0.0, 0.0, 0.0, -0.02);
        assert!(!inversion_still_holds(&c));
    }

    #[test]
    fn inversion_reverify_wraps_periodic_params() {
        // Post pair straddles the branch cut: raw diff −6.0, wrapped +0.28 —
        // opposite the pre diff only under the periodic convention.
        let c = cand(unit_circle(), 3.0, 3.0, 2.9, -3.0);
        assert!(inversion_still_holds(&c));
        // The same numbers on an OPEN curve read raw: both diffs negative.
        let c = cand(crate::Curve::LineSegment, 3.0, 3.0, 2.9, -3.0);
        assert!(!inversion_still_holds(&c));
    }

    #[test]
    fn mint_interposition_is_a_pure_order_test() {
        let circle = || crate::Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 5.0,
        };
        // Mint strictly inside the kept edge: interposes (the false view).
        assert_eq!(mint_interposes(&circle(), 0.10, 0.15, 0.30), Some(true));
        assert_eq!(mint_interposes(&circle(), 0.30, 0.15, 0.10), Some(true));
        // Mint outside (beyond the kept junction): clean.
        assert_eq!(mint_interposes(&circle(), 0.15, 0.10, 0.30), Some(false));
        // A zero step cannot certify.
        assert_eq!(mint_interposes(&circle(), 0.15, 0.15, 0.30), None);
        // Wrapped across the branch cut: 3.10 → −3.12 → −3.05 is a short
        // monotone walk through the cut — the mint interposes.
        assert_eq!(mint_interposes(&circle(), 3.10, -3.12, -3.05), Some(true));
        // The same numbers raw (open conic): d1 = −6.22 and d2 = +0.07
        // disagree — no interposition.
        assert_eq!(
            mint_interposes(&crate::Curve::LineSegment, 3.10, -3.12, -3.05),
            Some(false)
        );
    }

    #[test]
    fn rim_recognition_is_carrier_identity() {
        let target = [cone(0.0, HA_I), cone(-2.0, HA_J), wall()];
        let triple = || vec![cone(0.0, HA_I), cone(-2.0, HA_J), wall()];
        // Exactly one exact-triple vertex: recognized.
        let cands = vec![
            (4u32, vec![cone(0.0, HA_I), wall()]),
            (7u32, triple()),
            (9u32, vec![cone(0.0, HA_I), cone(-2.0, HA_J)]),
        ];
        assert_eq!(recognize_rim_junction(&cands, &target), Ok(7));
        // A SUPERSET carrier set is not the junction's identity.
        let cands = vec![(
            7u32,
            vec![
                cone(0.0, HA_I),
                cone(-2.0, HA_J),
                wall(),
                Surface::Plane {
                    normal: Vector3::new(0.0, 1.0, 0.0),
                    d: 0.0,
                },
            ],
        )];
        assert_eq!(
            recognize_rim_junction(&cands, &target),
            Err(RehomeDecline::RimNotRecognized)
        );
        // No candidate: not recognized (an f3 mint case, loud at f2).
        assert_eq!(
            recognize_rim_junction(&[], &target),
            Err(RehomeDecline::RimNotRecognized)
        );
        // Two exact triples: identity is ambiguous, never guessed.
        let cands = vec![(7u32, triple()), (8u32, triple())];
        assert_eq!(
            recognize_rim_junction(&cands, &target),
            Err(RehomeDecline::RimAmbiguous)
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

    // ---- f2b view_material_verdict --------------------------------------

    /// Unit square kept patch in z=0 wound CCW from +z (result-outward
    /// +z), with the corner junction v0 on its cycle. The plane's stored
    /// normal is deliberately NON-unit and z-negative to pin the
    /// normalization and the winding-not-surface-normal sense authority.
    fn material_fixture(ccw: bool) -> (crate::Mesh, Vec<crate::stage4_splice::SplicePatch>) {
        let mesh = crate::Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
            ],
            tris: if ccw {
                vec![[0, 1, 2], [0, 2, 3]]
            } else {
                vec![[0, 2, 1], [0, 3, 2]]
            },
        };
        let patch = crate::stage4_splice::SplicePatch {
            cycles: vec![vec![0, 1, 2, 3]],
            tris: vec![0, 1],
            surface: cut_plane(),
        };
        (mesh, vec![patch])
    }

    fn cut_plane() -> Surface {
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -2.0),
            d: 0.0,
        }
    }

    #[test]
    fn material_verdict_signs_follow_the_kept_winding() {
        // CCW-from-+z winding: result-outward is +z, so +z is VOID.
        let (mesh, patches) = material_fixture(true);
        let above = view_material_verdict(&mesh, &patches, &cut_plane(), 0, [0.2, 0.2, 0.5])
            .expect("clean margin");
        assert!(
            above.margin > 0.49 && above.margin < 0.51,
            "void-side victim is WASTE at its physical distance, got {:+.3e}",
            above.margin
        );
        assert_eq!(above.cut_patches, 1);
        let below = view_material_verdict(&mesh, &patches, &cut_plane(), 0, [0.2, 0.2, -0.5])
            .expect("clean margin");
        assert!(
            below.margin < -0.49 && below.margin > -0.51,
            "material-side victim reads KEPT, got {:+.3e}",
            below.margin
        );
        // Flipped winding: the SAME victim flips verdict — the sense
        // authority is the kept patch's winding, not the surface normal.
        let (mesh, patches) = material_fixture(false);
        let above = view_material_verdict(&mesh, &patches, &cut_plane(), 0, [0.2, 0.2, 0.5])
            .expect("clean margin");
        assert!(above.margin < 0.0, "flipped winding flips the void side");
    }

    #[test]
    fn material_verdict_requires_a_corner_witness() {
        let (mesh, patches) = material_fixture(true);
        // Junction v9 is on no cycle of the cut surface's patches.
        assert_eq!(
            view_material_verdict(&mesh, &patches, &cut_plane(), 9, [0.2, 0.2, 0.5]).unwrap_err(),
            RehomeDecline::CutPatchAbsent
        );
        // A non-plane cut declines as a shape mismatch.
        let sphere = Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(
            view_material_verdict(&mesh, &patches, &sphere, 0, [0.2, 0.2, 0.5]).unwrap_err(),
            RehomeDecline::ShapeMismatch
        );
    }

    #[test]
    fn material_verdict_degeneracies_decline() {
        // A folded witness (its two triangles wind oppositely) cannot
        // orient the material side.
        let (mesh, mut patches) = material_fixture(true);
        patches[0].tris = vec![0, 1];
        let folded = crate::Mesh {
            verts: mesh.verts.clone(),
            tris: vec![[0, 1, 2], [0, 3, 2]],
        };
        assert_eq!(
            view_material_verdict(&folded, &patches, &cut_plane(), 0, [0.2, 0.2, 0.5]).unwrap_err(),
            RehomeDecline::CutPatchWindingDegenerate
        );
        // Two corner witnesses with OPPOSITE senses decline.
        let (mesh2, patches2) = material_fixture(true);
        let (_, patches_cw) = material_fixture(false);
        let both = vec![patches2[0].clone(), patches_cw[0].clone()];
        let two_patch_mesh = crate::Mesh {
            verts: mesh2.verts.clone(),
            tris: vec![[0, 1, 2], [0, 2, 3], [0, 2, 1], [0, 3, 2]],
        };
        let mut both = both;
        both[1].tris = vec![2, 3];
        assert_eq!(
            view_material_verdict(&two_patch_mesh, &both, &cut_plane(), 0, [0.2, 0.2, 0.5])
                .unwrap_err(),
            RehomeDecline::CutPatchWindingDegenerate
        );
        // An on-plane victim is inside the evaluation-noise floor.
        assert_eq!(
            view_material_verdict(&mesh, &patches, &cut_plane(), 0, [0.2, 0.2, 0.0]).unwrap_err(),
            RehomeDecline::MaterialMarginDegenerate
        );
    }
}

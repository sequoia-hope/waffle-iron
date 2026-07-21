//! #188 inc-1 — §3.2 envelope-resolution primitives for osculating
//! boundary-curve pairs on a cylinder patch (spec
//! `specs/yang_188_f0082_j3_envelope_selection.md`, §3.2 as revised by the
//! binding inc-0 findings §7.6).
//!
//! UNWIRED (the stage4_update / N-137.1 idiom): nothing in the production
//! pipeline calls this module yet. inc-2 wires it into `emit_topology`
//! behind `YANG_S5_ENVELOPE_ENABLE`; this increment de-risks the pure
//! geometry on the F0082 pinned fixture (`tests_unit/s188_envelope.rs`).
//!
//! Vocabulary (cylinder × plane × plane — the F0082 class):
//! - **§3.2.1 switch points**: the two exact crossings of the intersection
//!   conic (cylinder ∩ partner plane) and the original boundary conic
//!   (cylinder ∩ owner plane) = the hits of the planes' intersection line
//!   on the cylinder, closed form. Fail closed on parallel planes, a line
//!   that misses or grazes the cylinder, or axis-parallel pair planes.
//! - **§3.2.2 band classification (§7.6 revision)**: between consecutive
//!   switch boundaries each support is live or dead by an op-resolved sign
//!   test against ALL crossing support planes — the osculating pair PLUS
//!   any masking wall of the partner solid. A triple point is a switch
//!   junction ONLY when no masking wall covers it (free space); a
//!   wall-masked triple point is NOT a junction — the boundary switch runs
//!   through the wall-crossing complex instead (F0082: BOTH triple points
//!   are masked, §7.3). Per band exactly ONE support is live, or the band
//!   is a wall-complex sliver (the wall surface passes between the two
//!   curves); anything else is a loud typed error, never a guess.
//!
//! All tests are analytic on the exact surface/plane data (A15) — never
//! mesh positions. No tolerance is invented here: sub-`TAU_WORK` sign
//! calls and sub-`TAU_MODEL` wall margins fail closed as degenerate.

use crate::brep::InputId;
use crate::geom::Surface;
use cad_primitives::{BoolOp, TAU_MODEL, TAU_WORK};

/// Angular dedup floor for boundary candidates (radians). Two candidates
/// closer than this are the same geometric event (e.g. an axis-parallel
/// wall crosses both curves at bit-identical azimuth); a triple point this
/// close to a wall crossing is degenerate and fails closed. Far below the
/// narrowest real wall-complex sliver (F0082: 1.9e-5 rad).
const ANG_EPS: f64 = 1e-9;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

/// Wrap an angle into (−π, π].
fn wrap(mut t: f64) -> f64 {
    while t > std::f64::consts::PI {
        t -= 2.0 * std::f64::consts::PI;
    }
    while t <= -std::f64::consts::PI {
        t += 2.0 * std::f64::consts::PI;
    }
    t
}

/// A support plane in normalized Hesse form: `sd(p) = n·p + d`, `n` the
/// unit OUTWARD normal of the solid that owns the face (the inc-4b
/// [`crate::boolean::junction`] convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvPlane {
    /// Unit outward normal.
    pub n: [f64; 3],
    /// Plane offset: `n·p + d = 0` on the plane.
    pub d: f64,
}

impl EnvPlane {
    fn sd(&self, p: [f64; 3]) -> f64 {
        dot(self.n, p) + self.d
    }
}

/// Why an envelope primitive failed CLOSED (spec §3.2: outside the
/// implemented vocabulary ⇒ no envelope, keep the existing STOP).
#[derive(Clone, Debug, PartialEq)]
pub enum EnvelopeError {
    /// The patch surface is not a cylinder (inc-1 vocabulary).
    UnsupportedSurface,
    /// The two pair planes are (near-)parallel — no intersection line.
    PlanesNearParallel,
    /// A pair plane is (near-)parallel to the cylinder axis — its conic
    /// has no single-valued axial profile v(θ).
    AxisParallelPairPlane,
    /// The planes' intersection line misses the cylinder — the two curves
    /// never cross (not the osculating-pair class).
    NoTripleContact,
    /// The planes' intersection line grazes the cylinder — the two switch
    /// points coincide (tangent contact, degenerate).
    TangentTripleContact,
    /// The (op, patch-owner) combination is outside the implemented
    /// same-side (max-envelope) vocabulary: under this op the two curves
    /// bound OPPOSITE ends of the kept band (or none), not competing
    /// bottom envelopes.
    UnsupportedOp,
    /// A boundary configuration too degenerate to classify: a triple
    /// point coinciding with a wall crossing, a triple point ON a wall
    /// plane within `TAU_MODEL`, or a deciding sign below `TAU_WORK`.
    DegenerateBoundary { theta: f64 },
    /// A band midpoint where the liveness tests yield both-live or
    /// neither-live with no wall passing between the curves — broken
    /// input, never a guess.
    AmbiguousBand {
        theta: f64,
        int_live: bool,
        orig_live: bool,
    },
}

/// One exact switch (triple) point: cylinder ∩ pair-plane ∩ pair-plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchPoint3 {
    /// Azimuth in the deterministic chart of [`CylFrame`].
    pub theta: f64,
    /// The exact 3D point (on all three surfaces).
    pub p: [f64; 3],
}

/// §7.6 classification of a triple point against the masking walls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TripleClass {
    /// No wall covers it: the boundary MUST switch curves exactly here —
    /// a junction vertex of the envelope.
    FreeSpace,
    /// Beyond `wall` by `margin` (> `TAU_MODEL`-scale): the ellipse↔rim
    /// switch is subsumed by that wall's crossing complex; this triple
    /// point is NOT an output junction (F0082 §7.3: both, at +1.2921e-3).
    WallMasked { wall: usize, margin: f64 },
}

/// A triple point with its §7.6 masking classification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClassifiedTriple {
    pub point: SwitchPoint3,
    pub class: TripleClass,
}

/// Which support carries the live boundary over one azimuth band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BandLive {
    /// The intersection conic (cylinder ∩ partner-face plane).
    IntCurve,
    /// The original boundary conic (cylinder ∩ owner-face plane).
    OrigCurve,
    /// Neither curve alone: `wall`'s surface passes between the two
    /// curves over this (sliver) band — the boundary weaves through the
    /// wall-crossing complex (owned by the inc-4d pierce machinery, NOT
    /// by this pair's envelope; §3.3 keeps it byte-identical).
    WallComplex { wall: usize },
}

/// What kind of geometric event a retained band boundary is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoundaryKind {
    /// A free-space triple point (both pair curves cross here).
    FreeSpaceTriple,
    /// A crossing of `wall` with one pair curve (`on_int_curve` says
    /// which) — an inc-4d minted junction in the live corpus cases.
    WallCrossing { wall: usize, on_int_curve: bool },
}

/// One retained band boundary (adjacent bands have DIFFERENT liveness —
/// non-boundaries are merged away).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeBoundary {
    pub theta: f64,
    pub p: [f64; 3],
    pub kind: BoundaryKind,
}

/// The §3.2.2 band decomposition of the full circle.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvelopeBands {
    /// Retained boundaries sorted by `theta` in (−π, π].
    pub boundaries: Vec<EnvelopeBoundary>,
    /// `live[i]` = the live support on the band from `boundaries[i]` to
    /// `boundaries[(i+1) % n]` (circular). Same length as `boundaries`.
    pub live: Vec<BandLive>,
    /// The two exact triple points with masking classification (masked
    /// ones do NOT appear in `boundaries`).
    pub triples: [ClassifiedTriple; 2],
}

impl EnvelopeBands {
    /// The live support at azimuth `theta` (test/diagnostic helper).
    pub fn live_at(&self, theta: f64) -> Option<BandLive> {
        let n = self.boundaries.len();
        if n == 0 {
            return None;
        }
        let t = wrap(theta);
        for i in 0..n {
            let a = self.boundaries[i].theta;
            let b = self.boundaries[(i + 1) % n].theta;
            let span = if i + 1 == n {
                // wrap band
                (t >= a) || (t < b)
            } else {
                t >= a && t < b
            };
            if span {
                return Some(self.live[i]);
            }
        }
        None
    }
}

/// Deterministic cylinder chart — the same frame construction as the
/// inc-0 probe (`stage5_osculation_probe`), so azimuths are comparable
/// across the two modules.
struct CylFrame {
    ap: [f64; 3],
    a_hat: [f64; 3],
    x_hat: [f64; 3],
    y_hat: [f64; 3],
    r: f64,
}

impl CylFrame {
    fn build(surface: &Surface) -> Result<CylFrame, EnvelopeError> {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = *surface
        else {
            return Err(EnvelopeError::UnsupportedSurface);
        };
        let a = axis_dir.as_array();
        let an = norm(a);
        if an < TAU_WORK {
            return Err(EnvelopeError::UnsupportedSurface);
        }
        let a_hat = [a[0] / an, a[1] / an, a[2] / an];
        // Seed = the coordinate axis least aligned with the cylinder axis
        // (identical to the probe's construction).
        let ax = [a_hat[0].abs(), a_hat[1].abs(), a_hat[2].abs()];
        let k = if ax[0] <= ax[1] && ax[0] <= ax[2] {
            0
        } else if ax[1] <= ax[2] {
            1
        } else {
            2
        };
        let mut seed = [0.0f64; 3];
        seed[k] = 1.0;
        let pa = dot(seed, a_hat);
        let xr = sub3(seed, [pa * a_hat[0], pa * a_hat[1], pa * a_hat[2]]);
        let xn = norm(xr);
        let x_hat = [xr[0] / xn, xr[1] / xn, xr[2] / xn];
        let y_hat = cross(a_hat, x_hat);
        Ok(CylFrame {
            ap: axis_point.as_array(),
            a_hat,
            x_hat,
            y_hat,
            r: radius,
        })
    }

    /// 3D point at (azimuth θ, axial height v).
    fn embed(&self, theta: f64, v: f64) -> [f64; 3] {
        let (c, s) = (theta.cos(), theta.sin());
        [
            self.ap[0] + self.r * (c * self.x_hat[0] + s * self.y_hat[0]) + v * self.a_hat[0],
            self.ap[1] + self.r * (c * self.x_hat[1] + s * self.y_hat[1]) + v * self.a_hat[1],
            self.ap[2] + self.r * (c * self.x_hat[2] + s * self.y_hat[2]) + v * self.a_hat[2],
        ]
    }

    fn theta_of(&self, p: [f64; 3]) -> f64 {
        let q = sub3(p, self.ap);
        let v = dot(q, self.a_hat);
        let w = sub3(q, [v * self.a_hat[0], v * self.a_hat[1], v * self.a_hat[2]]);
        dot(w, self.y_hat).atan2(dot(w, self.x_hat))
    }

    /// Axial profile of a plane's conic on the cylinder:
    /// `v(θ) = c0 + cc·cosθ + cs·sinθ`. Errors for axis-parallel planes.
    fn profile(&self, plane: &EnvPlane) -> Result<AxialProfile, EnvelopeError> {
        let na = dot(plane.n, self.a_hat);
        if na.abs() < 1e-9 {
            return Err(EnvelopeError::AxisParallelPairPlane);
        }
        Ok(AxialProfile {
            c0: -(dot(plane.n, self.ap) + plane.d) / na,
            cc: -self.r * dot(plane.n, self.x_hat) / na,
            cs: -self.r * dot(plane.n, self.y_hat) / na,
        })
    }
}

/// `v(θ) = c0 + cc·cosθ + cs·sinθ` — a plane conic's axial height.
#[derive(Clone, Copy, Debug)]
struct AxialProfile {
    c0: f64,
    cc: f64,
    cs: f64,
}

impl AxialProfile {
    fn v(&self, theta: f64) -> f64 {
        self.c0 + self.cc * theta.cos() + self.cs * theta.sin()
    }
}

/// The azimuth zeros of a signed distance `sd(θ) = e0 + ec·cosθ + es·sinθ`
/// evaluated along a curve: 0, 1 (tangent, reported as two equal), or 2.
fn sinusoid_zeros(e0: f64, ec: f64, es: f64) -> Vec<f64> {
    let amp = ec.hypot(es);
    if amp == 0.0 {
        return Vec::new();
    }
    let c = -e0 / amp;
    if !(-1.0..=1.0).contains(&c) {
        return Vec::new();
    }
    let psi = es.atan2(ec);
    let dt = c.acos();
    vec![wrap(psi + dt), wrap(psi - dt)]
}

/// Signed-distance sinusoid of plane `q` along the conic of plane
/// `curve_profile` on the cylinder.
fn sd_along(frame: &CylFrame, prof: &AxialProfile, q: &EnvPlane) -> (f64, f64, f64) {
    let na = dot(q.n, frame.a_hat);
    (
        dot(q.n, frame.ap) + q.d + na * prof.c0,
        frame.r * dot(q.n, frame.x_hat) + na * prof.cc,
        frame.r * dot(q.n, frame.y_hat) + na * prof.cs,
    )
}

/// §3.2.1 — the two exact switch points of an (intersection conic ×
/// original conic) pair on a cylinder: the hits of the pair planes'
/// intersection line on the cylinder, closed form. Fails closed on every
/// configuration outside the transversal two-point vocabulary.
pub fn cylinder_two_plane_switch_points(
    surface: &Surface,
    p_int: &EnvPlane,
    p_orig: &EnvPlane,
) -> Result<[SwitchPoint3; 2], EnvelopeError> {
    let frame = CylFrame::build(surface)?;
    // Pair planes must carry single-valued axial profiles (also rules out
    // a line parallel to the axis, which needs BOTH planes axis-parallel).
    let _ = frame.profile(p_int)?;
    let _ = frame.profile(p_orig)?;

    let u = cross(p_int.n, p_orig.n);
    let uu = dot(u, u);
    if uu.sqrt() < 1e-9 {
        return Err(EnvelopeError::PlanesNearParallel);
    }
    // Point on the line: solve n_int·p = −d_int, n_orig·p = −d_orig,
    // u·p = u·axis_point (the line point nearest the axis foot — well
    // conditioned). Cramer determinant = u·(n_int × n_orig) = |u|².
    let rhs = [-p_int.d, -p_orig.d, dot(u, frame.ap)];
    let rows = [p_int.n, p_orig.n, u];
    let det = uu; // = u·(n_int×n_orig)
    let mut p0 = [0.0f64; 3];
    for (col, p0c) in p0.iter_mut().enumerate() {
        let mut m = rows;
        for (ri, row) in m.iter_mut().enumerate() {
            row[col] = rhs[ri];
        }
        let d = dot(m[0], cross(m[1], m[2]));
        *p0c = d / det;
    }
    // Intersect p0 + t·u with the infinite cylinder: strip axial parts.
    let q = sub3(p0, frame.ap);
    let qa = dot(q, frame.a_hat);
    let w0 = sub3(
        q,
        [
            qa * frame.a_hat[0],
            qa * frame.a_hat[1],
            qa * frame.a_hat[2],
        ],
    );
    let ua = dot(u, frame.a_hat);
    let up = sub3(
        u,
        [
            ua * frame.a_hat[0],
            ua * frame.a_hat[1],
            ua * frame.a_hat[2],
        ],
    );
    let a = dot(up, up);
    if a.sqrt() < TAU_WORK {
        // Line parallel to the axis (unreachable behind the profile gate,
        // kept as defense in depth).
        return Err(EnvelopeError::PlanesNearParallel);
    }
    let b = 2.0 * dot(w0, up);
    let c = dot(w0, w0) - frame.r * frame.r;
    let disc = b * b - 4.0 * a * c;
    if disc <= 0.0 {
        return Err(EnvelopeError::NoTripleContact);
    }
    // Separation of the two hits along the line: √disc/a · |u|; grazing
    // contact (separation below TAU_MODEL at model scale) is degenerate.
    let scale = 1.0 + frame.r.abs() + norm(frame.ap);
    if disc.sqrt() / a * norm(u) < TAU_MODEL * scale {
        return Err(EnvelopeError::TangentTripleContact);
    }
    let sq = disc.sqrt();
    let mut out = [SwitchPoint3 {
        theta: 0.0,
        p: [0.0; 3],
    }; 2];
    for (i, t) in [(-b - sq) / (2.0 * a), (-b + sq) / (2.0 * a)]
        .into_iter()
        .enumerate()
    {
        let p = [p0[0] + t * u[0], p0[1] + t * u[1], p0[2] + t * u[2]];
        out[i] = SwitchPoint3 {
            theta: frame.theta_of(p),
            p,
        };
    }
    Ok(out)
}

/// The op-resolved liveness rule for the supported same-side
/// (max-envelope) vocabulary. `owner` = the operand whose surface carries
/// the patch (and whose face plane carries the ORIGINAL conic); the other
/// operand ("partner") contributes the intersection conic's plane and the
/// masking walls.
///
/// Same-side cases (both curves compete for the SAME end of the kept
/// band; kept side of the partner planes = OUTSIDE the partner):
/// - `Union`, either owner — kept surface is outside the partner;
/// - `Subtract` with `owner == A` (base) — kept base surface is outside
///   the tool.
///
/// Fail closed ([`EnvelopeError::UnsupportedOp`]) elsewhere:
/// - `Subtract` with `owner == B` (tool) and `Intersect` — the kept side
///   of the partner planes is INSIDE, so the pair bounds OPPOSITE ends of
///   the kept band (a pinch, not a max-envelope);
/// - `Xor` — both sides of every surface survive, no selection exists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeRule {
    /// Sign `s` such that a point is on the partner-kept side iff
    /// `s · sd_partner_plane(p) ≥ 0`. Always `+1.0` in the supported
    /// vocabulary (kept = outside partner); carried explicitly so the
    /// liveness code reads as the op table, not as a hard-coded Union.
    pub partner_kept_sign: f64,
}

/// Resolve `(op, owner)` into the [`EnvelopeRule`], or fail closed.
pub fn resolve_envelope_rule(op: BoolOp, owner: InputId) -> Result<EnvelopeRule, EnvelopeError> {
    match (op, owner) {
        (BoolOp::Union, _) | (BoolOp::Subtract, InputId::A) => Ok(EnvelopeRule {
            partner_kept_sign: 1.0,
        }),
        (BoolOp::Subtract, InputId::B) | (BoolOp::Intersect, _) | (BoolOp::Xor, _) => {
            Err(EnvelopeError::UnsupportedOp)
        }
    }
}

/// §3.2.2 (§7.6 revision) — decompose the full azimuth circle into bands
/// with exactly one live support each (or a wall-complex sliver), with
/// switch junctions at free-space triple points and wall crossings.
///
/// Liveness at azimuth θ (each support evaluated at ITS OWN curve point):
/// - intersection conic live ⟺ within the owner's surface extent
///   (`sd_owner_plane ≤ 0`, op-independent existence) AND within the
///   partner FACE's extent (inside every wall, op-independent existence);
/// - original conic live ⟺ on the partner-kept side of the partner's
///   pair plane (op-resolved sign) OR beyond any masking wall (outside
///   the partner solid entirely — the §7.6 disjunct).
///
/// Both-live / neither-live bands where some wall separates the two curve
/// points (beyond for one, inside for the other) are the wall-crossing
/// complex slivers ([`BandLive::WallComplex`]); without such a wall they
/// fail closed.
pub fn classify_bands(
    surface: &Surface,
    p_int: &EnvPlane,
    p_orig: &EnvPlane,
    walls: &[EnvPlane],
    op: BoolOp,
    owner: InputId,
) -> Result<EnvelopeBands, EnvelopeError> {
    let rule = resolve_envelope_rule(op, owner)?;
    let frame = CylFrame::build(surface)?;
    let prof_int = frame.profile(p_int)?;
    let prof_orig = frame.profile(p_orig)?;
    let triples = cylinder_two_plane_switch_points(surface, p_int, p_orig)?;

    // --- Classify the triple points against the walls (§7.6 / §7.3) ----
    let mut classified = [ClassifiedTriple {
        point: triples[0],
        class: TripleClass::FreeSpace,
    }; 2];
    for (i, tp) in triples.iter().enumerate() {
        let scale = 1.0 + norm(tp.p);
        let mut class = TripleClass::FreeSpace;
        for (w, wall) in walls.iter().enumerate() {
            let sd = wall.sd(tp.p);
            if sd.abs() < TAU_MODEL * scale {
                // A triple point ON a wall plane: too degenerate to call.
                return Err(EnvelopeError::DegenerateBoundary { theta: tp.theta });
            }
            if sd > 0.0 {
                class = TripleClass::WallMasked {
                    wall: w,
                    margin: sd,
                };
                break;
            }
        }
        classified[i] = ClassifiedTriple { point: *tp, class };
    }

    // --- Boundary candidates: free-space triples + wall crossings ------
    struct Candidate {
        theta: f64,
        p: [f64; 3],
        kind: BoundaryKind,
    }
    let mut cands: Vec<Candidate> = Vec::new();
    for ct in &classified {
        if ct.class == TripleClass::FreeSpace {
            cands.push(Candidate {
                theta: ct.point.theta,
                p: ct.point.p,
                kind: BoundaryKind::FreeSpaceTriple,
            });
        }
    }
    for (w, wall) in walls.iter().enumerate() {
        for (prof, on_int) in [(&prof_int, true), (&prof_orig, false)] {
            let (e0, ec, es) = sd_along(&frame, prof, wall);
            for theta in sinusoid_zeros(e0, ec, es) {
                cands.push(Candidate {
                    theta,
                    p: frame.embed(theta, prof.v(theta)),
                    kind: BoundaryKind::WallCrossing {
                        wall: w,
                        on_int_curve: on_int,
                    },
                });
            }
        }
    }
    cands.sort_by(|a, b| a.theta.total_cmp(&b.theta));
    // Dedup coincident candidates. Same-wall crossings merging across the
    // two curves is legitimate (axis-parallel wall); a triple point
    // coinciding with a wall crossing is degenerate.
    let mut dedup: Vec<Candidate> = Vec::new();
    for c in cands {
        match dedup.last() {
            Some(prev) if wrap(c.theta - prev.theta).abs() < ANG_EPS => {
                let same_wall = matches!(
                    (&prev.kind, &c.kind),
                    (
                        BoundaryKind::WallCrossing { wall: w1, .. },
                        BoundaryKind::WallCrossing { wall: w2, .. },
                    ) if w1 == w2
                );
                if !same_wall {
                    return Err(EnvelopeError::DegenerateBoundary { theta: c.theta });
                }
                // keep the first
            }
            _ => dedup.push(c),
        }
    }
    // Circular wrap coincidence (last vs first across ±π).
    if dedup.len() >= 2 {
        let (first_t, last_t) = (dedup[0].theta, dedup[dedup.len() - 1].theta);
        if wrap(first_t - last_t).abs() < ANG_EPS {
            let same_wall = matches!(
                (&dedup[0].kind, &dedup[dedup.len() - 1].kind),
                (
                    BoundaryKind::WallCrossing { wall: w1, .. },
                    BoundaryKind::WallCrossing { wall: w2, .. },
                ) if w1 == w2
            );
            if !same_wall {
                return Err(EnvelopeError::DegenerateBoundary { theta: first_t });
            }
            dedup.pop();
        }
    }
    if dedup.is_empty() {
        // Both triples masked AND no wall crossing found: walls that mask
        // a triple must cross the curves somewhere — broken input.
        return Err(EnvelopeError::AmbiguousBand {
            theta: 0.0,
            int_live: false,
            orig_live: false,
        });
    }

    // --- Liveness at each band midpoint --------------------------------
    let live_at_midpoint = |theta: f64| -> Result<BandLive, EnvelopeError> {
        let pi3 = frame.embed(theta, prof_int.v(theta));
        let po3 = frame.embed(theta, prof_orig.v(theta));
        let guard = |sd: f64, p: [f64; 3]| -> Result<f64, EnvelopeError> {
            if sd.abs() < TAU_WORK * (1.0 + norm(p)) {
                Err(EnvelopeError::DegenerateBoundary { theta })
            } else {
                Ok(sd)
            }
        };
        // int conic: owner-extent + partner-face extent (both existence).
        let mut int_live = guard(p_orig.sd(pi3), pi3)? <= 0.0;
        for wall in walls {
            int_live = int_live && guard(wall.sd(pi3), pi3)? <= 0.0;
        }
        // orig conic: partner-kept side, or beyond a wall (kept = outside).
        let mut orig_live = rule.partner_kept_sign * guard(p_int.sd(po3), po3)? >= 0.0;
        if rule.partner_kept_sign > 0.0 {
            for wall in walls {
                orig_live = orig_live || guard(wall.sd(po3), po3)? > 0.0;
            }
        }
        match (int_live, orig_live) {
            (true, false) => Ok(BandLive::IntCurve),
            (false, true) => Ok(BandLive::OrigCurve),
            (i, o) => {
                // A wall passing BETWEEN the two curves = the crossing
                // complex owns this sliver.
                for (w, wall) in walls.iter().enumerate() {
                    if (wall.sd(pi3) > 0.0) != (wall.sd(po3) > 0.0) {
                        return Ok(BandLive::WallComplex { wall: w });
                    }
                }
                Err(EnvelopeError::AmbiguousBand {
                    theta,
                    int_live: i,
                    orig_live: o,
                })
            }
        }
    };

    let n = dedup.len();
    let mut band_live: Vec<BandLive> = Vec::with_capacity(n);
    for i in 0..n {
        let a = dedup[i].theta;
        let b = dedup[(i + 1) % n].theta;
        let width = if i + 1 == n {
            b + 2.0 * std::f64::consts::PI - a
        } else {
            b - a
        };
        // Sample away from the (possibly wall-masked) triple azimuths: the
        // pair signed distances legitimately vanish THERE, but a masked
        // triple does not change band liveness (the wall disjunct covers
        // its sign flip), so any interior sample off the triples is
        // equivalent. Fail closed if no fraction clears them.
        let sample = [0.5, 0.25, 0.75]
            .into_iter()
            .map(|f| wrap(a + width * f))
            .find(|&t| triples.iter().all(|tp| wrap(t - tp.theta).abs() > 1e-6))
            .ok_or(EnvelopeError::DegenerateBoundary {
                theta: wrap(a + width / 2.0),
            })?;
        band_live.push(live_at_midpoint(sample)?);
    }

    // --- Merge non-boundaries (adjacent bands with equal liveness) -----
    let keep: Vec<bool> = (0..n)
        .map(|i| band_live[(i + n - 1) % n] != band_live[i])
        .collect();
    if keep.iter().all(|k| !k) {
        // One support live over the whole circle: not an envelope-switch
        // configuration for this pair (nothing to select).
        return Err(EnvelopeError::AmbiguousBand {
            theta: dedup[0].theta,
            int_live: band_live[0] == BandLive::IntCurve,
            orig_live: band_live[0] == BandLive::OrigCurve,
        });
    }
    let mut boundaries = Vec::new();
    let mut live = Vec::new();
    for i in 0..n {
        if keep[i] {
            boundaries.push(EnvelopeBoundary {
                theta: dedup[i].theta,
                p: dedup[i].p,
                kind: dedup[i].kind,
            });
            live.push(band_live[i]);
        }
    }

    Ok(EnvelopeBands {
        boundaries,
        live,
        triples: classified,
    })
}

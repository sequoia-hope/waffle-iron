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
#[derive(Clone, Copy, Debug)]
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

// ===========================================================================
// inc-2 — §3.3 gated loop rebuild (spec §5 inc-2; wiring of the §3.2
// primitives into `emit_topology`'s curved branch behind
// `YANG_S5_ENVELOPE_ENABLE`).
//
// The rebuild is pure boundary SELECTION over EXISTING output vertices
// (inc-0 §7.4: every junction the correct loop needs is already minted):
// per §3.2.2 band, keep the verts lying ON the live support conic in
// azimuth order, keep wall-complex sections byte-identical, and drop
// dead-side verts. A configuration the machinery cannot handle exactly
// (missing junction vert, foreign vert that would be dropped, multiple
// osculating pairs) BAILS to the untouched loop — the existing loud STOP
// downstream stays the failure mode (§3.2 fail-closed). A postcondition
// failure AFTER committing to a rebuild is a loud typed error (P10).
// ===========================================================================

use crate::{non_manifold_at, Curve, Mesh, PatchInfo, YangError};
use std::collections::BTreeMap;

/// Dev gate for the §3.3 rebuild. `=0` / `=off` / unset ⇒ disabled
/// (production byte-identical); anything else enables (the P3a/P3b idiom).
pub(crate) fn envelope_gate_enabled() -> bool {
    matches!(std::env::var("YANG_S5_ENVELOPE_ENABLE"), Ok(v) if !(v.is_empty() || v == "0" || v == "off"))
}

/// Read-only forensics for the gated rebuild (`YANG_S5_ENVELOPE_PROBE`):
/// prints fire/bail/commit decisions so inc-3 corpus triage can tell WHY a
/// case rebuilt or was left alone. Never affects behavior.
fn probe(args: std::fmt::Arguments<'_>) {
    if std::env::var_os("YANG_S5_ENVELOPE_PROBE").is_some() {
        eprintln!("[s5-env] {args}");
    }
}

/// The result of a §3.3 loop rebuild for one curved patch: replacement
/// cycles (same shape as `subdivided_cycles[info_index]`) plus analytic
/// `Curve` attributions for edges the rebuild created (existing edges keep
/// their `intersection_curves` attribution — the #158 gap must not widen,
/// but new envelope edges get true curve vocabulary).
pub(crate) struct LoopRebuild {
    pub cycles: Vec<Vec<(u32, u32)>>,
    /// Keyed by (index into `cycles`, undirected pair) — the same vert
    /// pair may carry DIFFERENT curves on different cycles (the §10.5
    /// lens: the main chain's Seg hop vs the sliver's conic arc).
    pub curve_overrides: BTreeMap<(usize, (u32, u32)), Curve>,
    /// inc-5 (spec §10.8): notch/sliver cycles emitted as STANDALONE
    /// REVERSED-SENSE faces of the owner's surface — the sub-observable
    /// seal patch (e.g. the F0082 crevice-slot end cap), NOT an inner
    /// loop of the owner (a strip below the owner's outer cycle escapes
    /// it — the inc-4a containment refutation) and NOT owner-sense (the
    /// §10.6 layer-4 winding refutation: the seal faces the void pocket,
    /// i.e. cavity sense). Each carries its own curve map (the closing
    /// hop's band conic; run edges resolve via `intersection_curves`).
    pub notches: Vec<NotchFace>,
    /// non-pinch notch anchor → pinch: a neighbor run ending at a vert
    /// that now lives only on a notch cycle enters the main rail THROUGH
    /// the notch's closing edge (spec §10.5 propagation).
    pub sliver_anchors: BTreeMap<u32, u32>,
    /// inc-3 (spec §10): one entry per cycle whose EDGE SET changed —
    /// the old and new vert sequences, for neighbor-chain propagation.
    pub chains: Vec<ChainRewrite>,
    /// Band context for typing propagation-created edges (spec §10.2).
    band_ctx: BandCurveCtx,
}

/// An owner cycle rewrite: the original and rebuilt vert sequences.
pub(crate) struct ChainRewrite {
    pub old_verts: Vec<u32>,
    pub new_verts: Vec<u32>,
}

/// inc-5 (spec §10.8): one notch/sliver cycle destined to become a
/// standalone cavity-sense face of the owner's surface.
pub(crate) struct NotchFace {
    pub cycle: Vec<(u32, u32)>,
    pub curves: BTreeMap<(u32, u32), Curve>,
}

/// Everything needed to assign a `Curve` to an edge created during
/// neighbor propagation (a planar–planar strip edge absent from both the
/// owner chain and `intersection_curves`): the band table plus the two
/// support conics.
struct BandCurveCtx {
    frame: CylFrame,
    bands: EnvelopeBands,
    int_conic: Curve,
    orig_conic: Curve,
}

impl BandCurveCtx {
    /// Band-live conic at the edge's azimuth midpoint; None inside a
    /// wall-complex band (outside the propagation vocabulary → bail).
    fn curve_for(&self, mesh: &Mesh, s: u32, e: u32) -> Option<Curve> {
        let ps = mesh.verts[s as usize].as_array();
        let pe = mesh.verts[e as usize].as_array();
        let (ts, _) = theta_v(&self.frame, ps);
        let (te, _) = theta_v(&self.frame, pe);
        let tm = wrap(ts + wrap(te - ts) / 2.0);
        match self.bands.live_at(tm) {
            Some(BandLive::IntCurve) => Some(self.int_conic),
            Some(BandLive::OrigCurve) => Some(self.orig_conic),
            Some(BandLive::WallComplex { .. }) | None => None,
        }
    }
}

/// Planar carrier of a conic `Curve` (the probe's vocabulary): plane
/// through the conic's center/vertex with its stored normal.
fn conic_carrier(c: &Curve) -> Option<([f64; 3], f64)> {
    let (p, n) = match c {
        Curve::Circle { center, normal, .. } => (center, normal),
        Curve::Ellipse { center, normal, .. } => (center, normal),
        Curve::Parabola { vertex, normal, .. } => (vertex, normal),
        Curve::Hyperbola { center, normal, .. } => (center, normal),
        Curve::LineSegment | Curve::SurfacePair { .. } => return None,
    };
    let n = n.as_array();
    Some((n, -dot(n, p.as_array())))
}

/// Sign-normalized approximate plane equality (either orientation) — the
/// probe's test, kept identical.
fn planes_match(n1: [f64; 3], d1: f64, n2: [f64; 3], d2: f64) -> bool {
    let same = norm(sub3(n1, n2)) < 1e-9 && (d1 - d2).abs() < 1e-9;
    let anti = norm(sub3(n1, [-n2[0], -n2[1], -n2[2]])) < 1e-9 && (d1 + d2).abs() < 1e-9;
    same || anti
}

/// The exact conic of `cylinder ∩ plane` (both from the fired pair): the
/// analytic curve vocabulary for NEW envelope edges on that support.
fn cylinder_plane_conic(frame: &CylFrame, plane: &EnvPlane) -> Curve {
    use cad_primitives::{Point3, Vector3};
    let na = dot(plane.n, frame.a_hat);
    let t = -(dot(plane.n, frame.ap) + plane.d) / na;
    let center = Point3::new(
        frame.ap[0] + t * frame.a_hat[0],
        frame.ap[1] + t * frame.a_hat[1],
        frame.ap[2] + t * frame.a_hat[2],
    );
    let cos_phi = na.abs();
    if (1.0 - cos_phi) < 1e-12 {
        Curve::Circle {
            center,
            normal: Vector3::new(plane.n[0], plane.n[1], plane.n[2]),
            radius: frame.r,
        }
    } else {
        // Major axis = the projection of the cylinder axis onto the plane.
        let proj = sub3(
            frame.a_hat,
            [na * plane.n[0], na * plane.n[1], na * plane.n[2]],
        );
        let pn = norm(proj);
        Curve::Ellipse {
            center,
            normal: Vector3::new(plane.n[0], plane.n[1], plane.n[2]),
            major_axis: Vector3::new(proj[0] / pn, proj[1] / pn, proj[2] / pn),
            major_radius: frame.r / cos_phi,
            minor_radius: frame.r,
        }
    }
}

/// Offset of `theta` past `start` going counter-clockwise, in [0, 2π).
fn ccw_offset(theta: f64, start: f64) -> f64 {
    let mut d = theta - start;
    while d < 0.0 {
        d += 2.0 * std::f64::consts::PI;
    }
    while d >= 2.0 * std::f64::consts::PI {
        d -= 2.0 * std::f64::consts::PI;
    }
    d
}

/// §3.3 entry point: detect the (single) osculating pair on this cylinder
/// patch, classify its bands, and rebuild the weaving cycle(s) by
/// selection. `Ok(None)` = not applicable / fail closed (loop untouched);
/// `Err` = a P10 postcondition violation after committing to a rebuild.
pub(crate) fn rebuild_osculating_loops(
    mesh: &Mesh,
    infos: &[PatchInfo],
    info_index: usize,
    subdivided_cycles: &[Vec<Vec<(u32, u32)>>],
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
    op: cad_primitives::BoolOp,
) -> Result<Option<LoopRebuild>, YangError> {
    let info = &infos[info_index];
    if !matches!(info.inherited, Surface::Cylinder { .. }) {
        return Ok(None);
    }
    let Ok(frame) = CylFrame::build(&info.inherited) else {
        return Ok(None);
    };
    let cycles = &subdivided_cycles[info_index];
    let owner = info.input;
    let vert_set: std::collections::BTreeSet<u32> = cycles
        .iter()
        .flat_map(|c| c.iter().map(|&(s, _)| s))
        .collect();

    // --- Support enumeration (the probe's §3.1 vocabulary) -------------
    // Intersection-conic candidates: distinct planar carriers among this
    // patch's attributed loop edges, matched to a PARTNER planar patch so
    // the outward normal orientation is known.
    let mut int_cands: Vec<(EnvPlane, Curve)> = Vec::new();
    for cycle in cycles {
        for &(s, e) in cycle {
            let key = if s < e { (s, e) } else { (e, s) };
            let Some(curve) = intersection_curves.get(&key) else {
                continue;
            };
            let Some((cn, cd)) = conic_carrier(curve) else {
                continue;
            };
            let matched = infos.iter().find_map(|other| {
                if other.input == owner {
                    return None;
                }
                let Surface::Plane { normal, d } = other.inherited else {
                    return None;
                };
                let n = normal.as_array();
                planes_match(cn, cd, n, d).then_some(EnvPlane { n, d })
            });
            let Some(pl) = matched else { continue };
            if !int_cands
                .iter()
                .any(|(p, _)| planes_match(p.n, p.d, pl.n, pl.d))
            {
                int_cands.push((pl, *curve));
            }
        }
    }
    // Original-conic candidates: OWNER planar patches sharing ≥2 loop verts.
    let mut orig_cands: Vec<EnvPlane> = Vec::new();
    for other in infos {
        if other.input != owner {
            continue;
        }
        let Surface::Plane { normal, d } = other.inherited else {
            continue;
        };
        let pl = EnvPlane {
            n: normal.as_array(),
            d,
        };
        let shared = infos_shared_verts(other, &vert_set);
        if shared >= 2
            && !orig_cands
                .iter()
                .any(|p| planes_match(p.n, p.d, pl.n, pl.d))
        {
            orig_cands.push(pl);
        }
    }

    // --- Osculation test per pair (C0118 floor; §7.7 band_frac gate) ----
    let mut fired: Vec<(EnvPlane, Curve, EnvPlane, f64)> = Vec::new();
    for (p_int, int_conic) in &int_cands {
        for p_orig in &orig_cands {
            if planes_match(p_int.n, p_int.d, p_orig.n, p_orig.d) {
                continue;
            }
            let (Ok(pi), Ok(po)) = (frame.profile(p_int), frame.profile(p_orig)) else {
                continue;
            };
            let (g0, gc, gs) = (pi.c0 - po.c0, pi.cc - po.cc, pi.cs - po.cs);
            let amp = gc.hypot(gs);
            let floor = observability_floor(mesh, cycles, &frame, &pi, &po);
            let band_frac = if amp > 0.0 {
                let lo = ((-floor - g0) / amp).clamp(-1.0, 1.0);
                let hi = ((floor - g0) / amp).clamp(-1.0, 1.0);
                (lo.acos() - hi.acos()) / std::f64::consts::PI
            } else if g0.abs() < floor {
                1.0
            } else {
                0.0
            };
            if band_frac >= 0.7 {
                fired.push((*p_int, *int_conic, *p_orig, floor));
            }
        }
    }
    // Exactly ONE osculating pair is the inc-2 vocabulary (F0085's
    // two-pair loop is an inc-3 concern) — else leave the loop alone.
    if fired.len() != 1 {
        if !fired.is_empty() {
            probe(format_args!(
                "patch info={info_index}: BAIL {} osculating pairs (inc-2 handles exactly 1)",
                fired.len()
            ));
        }
        return Ok(None);
    }
    let (p_int, int_conic, p_orig, floor) = fired.remove(0);
    probe(format_args!(
        "patch info={info_index}: osculating pair FIRED floor={floor:.3e} \
         int n=({:.6},{:.6},{:.6}) d={:.6} orig n=({:.6},{:.6},{:.6}) d={:.6}",
        p_int.n[0],
        p_int.n[1],
        p_int.n[2],
        p_int.d,
        p_orig.n[0],
        p_orig.n[1],
        p_orig.n[2],
        p_orig.d
    ));

    // --- Masking walls: ALL crossing partner planes (§8.2, the face-365
    // finding) — every planar patch of the partner operand sharing ≥1
    // loop vert, except the pair plane itself.
    let mut walls: Vec<EnvPlane> = Vec::new();
    for other in infos {
        if other.input == owner {
            continue;
        }
        let Surface::Plane { normal, d } = other.inherited else {
            continue;
        };
        let pl = EnvPlane {
            n: normal.as_array(),
            d,
        };
        if planes_match(pl.n, pl.d, p_int.n, p_int.d) {
            continue;
        }
        if infos_shared_verts(other, &vert_set) >= 1
            && !walls.iter().any(|w| planes_match(w.n, w.d, pl.n, pl.d))
        {
            walls.push(pl);
        }
    }

    let bands = match classify_bands(&info.inherited, &p_int, &p_orig, &walls, op, owner) {
        Ok(b) => b,
        // Fail closed: outside the implemented vocabulary — keep the
        // existing loop (and whatever loud STOP it produces downstream).
        Err(e) => {
            probe(format_args!(
                "patch info={info_index}: BAIL classify_bands ({e:?}) walls={}",
                walls.len()
            ));
            return Ok(None);
        }
    };
    probe(format_args!(
        "patch info={info_index}: bands={:?} boundaries={:?} triples={:?}",
        bands.live,
        bands
            .boundaries
            .iter()
            .map(|b| (b.theta, b.kind))
            .collect::<Vec<_>>(),
        bands
            .triples
            .iter()
            .map(|t| (t.point.theta, t.class))
            .collect::<Vec<_>>()
    ));

    let prof_int = frame
        .profile(&p_int)
        .expect("profile checked during detection");
    let prof_orig = frame
        .profile(&p_orig)
        .expect("profile checked during detection");
    let orig_conic = cylinder_plane_conic(&frame, &p_orig);

    // inc-3 (spec §10.3): planar-side adjacency over every OTHER patch's
    // conformal cycles. An off-live-conic weave vert with degree ≥ 3 here
    // is a planar junction — it must stay on the owner chain (erasing it
    // cannot close the pairing); degree ≤ 2 is a splice-able pass-through.
    let mut planar_adj: BTreeMap<u32, std::collections::BTreeSet<u32>> = BTreeMap::new();
    for (j, cycles_j) in subdivided_cycles.iter().enumerate() {
        if j == info_index {
            continue;
        }
        for cyc in cycles_j {
            for &(s, e) in cyc {
                planar_adj.entry(s).or_default().insert(e);
                planar_adj.entry(e).or_default().insert(s);
            }
        }
    }
    let is_planar_junction = |v: u32| planar_adj.get(&v).is_some_and(|s| s.len() >= 3);

    // --- Per-cycle rebuild ---------------------------------------------
    let mut out_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(cycles.len());
    let mut overrides: BTreeMap<(usize, (u32, u32)), Curve> = BTreeMap::new();
    // Notch cycles (spec §10.5, re-plumbed by §10.8): standalone
    // cavity-sense seal faces of this patch's surface.
    let mut pending_notches: Vec<NotchFace> = Vec::new();
    let mut sliver_anchors: BTreeMap<u32, u32> = BTreeMap::new();
    let mut chains: Vec<ChainRewrite> = Vec::new();
    let mut any_rebuilt = false;
    for cycle in cycles {
        // A cycle participates iff it carries the sub-observability weave
        // band: ≥3 verts within the floor of BOTH supports.
        let ambiguous = cycle
            .iter()
            .filter(|&&(s, _)| {
                let p = mesh.verts[s as usize].as_array();
                let (t, v) = theta_v(&frame, p);
                (v - prof_int.v(t)).abs() < floor && (v - prof_orig.v(t)).abs() < floor
            })
            .count();
        if ambiguous < 3 {
            out_cycles.push(cycle.clone());
            continue;
        }
        match rebuild_cycle(
            mesh,
            cycle,
            &frame,
            &bands,
            &p_int,
            &p_orig,
            &prof_int,
            &prof_orig,
            &is_planar_junction,
        )? {
            None => {
                probe(format_args!(
                    "patch info={info_index}: BAIL rebuild_cycle (len={})",
                    cycle.len()
                ));
                return Ok(None); // fail closed: loop untouched
            }
            Some(rebuilt) => {
                let new_verts = rebuilt.main;
                probe(format_args!(
                    "patch info={info_index}: cycle REBUILT {} -> {} verts (+{} slivers)\n\
                     [s5-env]   orig: {:?}\n\
                     [s5-env]   new:  {:?}",
                    cycle.len(),
                    new_verts.len(),
                    rebuilt.slivers.len(),
                    cycle.iter().map(|&(s, _)| s).collect::<Vec<_>>(),
                    new_verts
                ));
                let cyc_idx = out_cycles.len();
                let m = new_verts.len();
                let new_cycle: Vec<(u32, u32)> = (0..m)
                    .map(|i| (new_verts[i], new_verts[(i + 1) % m]))
                    .collect();
                // Curve vocabulary for NEW edges — new = absent from the
                // ORIGINAL cycle (an existing edge keeps its attribution,
                // including the un-attributed LineSegment wall-arc chords;
                // the #158 gap must not widen but also must not be
                // misread as "new"). New edges get the live support's
                // conic per band.
                let orig_edges: std::collections::BTreeSet<(u32, u32)> = cycle
                    .iter()
                    .map(|&(s, e)| if s < e { (s, e) } else { (e, s) })
                    .collect();
                for &(s, e) in &new_cycle {
                    let key = if s < e { (s, e) } else { (e, s) };
                    if orig_edges.contains(&key) || intersection_curves.contains_key(&key) {
                        continue;
                    }
                    let ps = mesh.verts[s as usize].as_array();
                    let pe = mesh.verts[e as usize].as_array();
                    let (ts, _) = theta_v(&frame, ps);
                    let (te, _) = theta_v(&frame, pe);
                    let tm = wrap(ts + wrap(te - ts) / 2.0);
                    match bands.live_at(tm) {
                        Some(BandLive::IntCurve) => {
                            overrides.insert((cyc_idx, key), int_conic);
                        }
                        Some(BandLive::OrigCurve) => {
                            overrides.insert((cyc_idx, key), orig_conic);
                        }
                        // A NEW edge inside a wall-complex sliver violates
                        // the §3.3 "wall sections byte-identical"
                        // postcondition — loud (P10).
                        Some(BandLive::WallComplex { .. }) | None => {
                            return Err(non_manifold_at(
                                "s5-envelope-new-wall-edge",
                                format_args!("edge {s}->{e} theta_mid={tm:.6}"),
                            ));
                        }
                    }
                }
                // Residual sliver faces (spec §10.5): the sliver keeps the
                // original run edges byte-identically (their attributions
                // resolve via `intersection_curves`); the closing
                // (next → prev) hop gets the band-live conic. The main
                // chain never touches the sliver's non-pinch anchor, so no
                // lens retyping is needed.
                for sv in &rebuilt.slivers {
                    let vs = &sv.verts;
                    let ns = vs.len();
                    let mut sc: Vec<(u32, u32)> = (0..ns - 1).map(|i| (vs[i], vs[i + 1])).collect();
                    sc.push((vs[ns - 1], vs[0]));
                    let hop = und(vs[0], vs[ns - 1]);
                    let ps = mesh.verts[vs[0] as usize].as_array();
                    let pe = mesh.verts[vs[ns - 1] as usize].as_array();
                    let (ts, _) = theta_v(&frame, ps);
                    let (te, _) = theta_v(&frame, pe);
                    let tm = wrap(ts + wrap(te - ts) / 2.0);
                    let conic = match bands.live_at(tm) {
                        Some(BandLive::IntCurve) => int_conic,
                        Some(BandLive::OrigCurve) => orig_conic,
                        Some(BandLive::WallComplex { .. }) | None => {
                            probe(format_args!(
                                "patch info={info_index}: BAIL sliver hop {hop:?} in WC band"
                            ));
                            return Ok(None);
                        }
                    };
                    let mut sv_curves = BTreeMap::new();
                    sv_curves.insert(hop, conic);
                    pending_notches.push(NotchFace {
                        cycle: sc,
                        curves: sv_curves,
                    });
                    sliver_anchors.insert(sv.non_pinch, sv.pinch);
                }
                // inc-3: record the chain rewrite when the EDGE SET changed
                // (a rotation of the same cycle is a no-op for neighbors).
                let new_edges: std::collections::BTreeSet<(u32, u32)> = new_cycle
                    .iter()
                    .map(|&(s, e)| if s < e { (s, e) } else { (e, s) })
                    .collect();
                if new_edges != orig_edges {
                    chains.push(ChainRewrite {
                        old_verts: cycle.iter().map(|&(s, _)| s).collect(),
                        new_verts: new_verts.clone(),
                    });
                }
                out_cycles.push(new_cycle);
                any_rebuilt = true;
            }
        }
    }
    if !any_rebuilt {
        return Ok(None);
    }
    Ok(Some(LoopRebuild {
        cycles: out_cycles,
        curve_overrides: overrides,
        notches: pending_notches,
        sliver_anchors,
        chains,
        band_ctx: BandCurveCtx {
            frame,
            bands,
            int_conic,
            orig_conic,
        },
    }))
}

/// inc-3 (spec §10.4): the full gate-ON pre-pass result — rewritten
/// cycles per info index (owners AND neighbors) plus ONE global curve
/// override map consulted by both emission branches, so every claimant
/// of a rewritten edge emits the identical `Curve` by construction.
pub(crate) struct EnvelopeRewrites {
    pub cycles: BTreeMap<usize, Vec<Vec<(u32, u32)>>>,
    /// Keyed by (info index, cycle index, undirected pair): one vert
    /// pair can carry different curves on different loops (§10.5), so
    /// overrides are per-loop, never global.
    pub curve_overrides: BTreeMap<(usize, usize, (u32, u32)), Curve>,
    /// inc-5 (spec §10.8): notch seal patches emitted as STANDALONE
    /// cavity-sense faces. The owner supplies the surface/attribution;
    /// the sense is the OPPOSITE of the owner face's.
    pub extra_faces: Vec<ExtraFace>,
}

/// §10.8: (owner info index, notch cycle, per-edge curves).
pub(crate) type ExtraFace = (usize, Vec<(u32, u32)>, BTreeMap<(u32, u32), Curve>);

/// EXACT on-conic membership for the §10.9 chord typing: the point must lie
/// on the conic's carrier plane AND at the conic's radial locus, both within
/// the scale-relative TAU_MODEL band. For ellipses the radial test uses the
/// parametric foot at `t = atan2(y/b, x/a)` — first-order-accurate for
/// near-on points, conservative as a membership test.
fn on_conic(mesh: &Mesh, v: u32, conic: &Curve) -> bool {
    let p = mesh.verts[v as usize].as_array();
    let scale = p[0].abs().max(p[1].abs()).max(p[2].abs());
    let band = TAU_MODEL * (1.0 + scale);
    let Some((n, d)) = conic_carrier(conic) else {
        return false;
    };
    if (dot(p, n) + d).abs() > band {
        return false;
    }
    match conic {
        Curve::Circle { center, radius, .. } => {
            let c = center.as_array();
            let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
            let h = dot(w, n);
            let r_in = [w[0] - h * n[0], w[1] - h * n[1], w[2] - h * n[2]];
            (norm(r_in) - radius).abs() <= band
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let c = center.as_array();
            let e1 = major_axis.as_array();
            let na = normal.as_array();
            let e2 = [
                na[1] * e1[2] - na[2] * e1[1],
                na[2] * e1[0] - na[0] * e1[2],
                na[0] * e1[1] - na[1] * e1[0],
            ];
            let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
            let (x, y) = (dot(w, e1), dot(w, e2));
            let t = (y / minor_radius).atan2(x / major_radius);
            let (fx, fy) = (major_radius * t.cos(), minor_radius * t.sin());
            ((x - fx).powi(2) + (y - fy).powi(2)).sqrt() <= band
        }
        _ => false,
    }
}

fn und(s: u32, e: u32) -> (u32, u32) {
    if s < e {
        (s, e)
    } else {
        (e, s)
    }
}

/// Gate-ON pre-pass: run the §3.3 owner rebuild for every curved patch,
/// then propagate each committed chain rewrite to neighbor patches'
/// copies of the shared chains (spec §10.2: replace each stale run of
/// old-chain edges with the new chain's sub-path between the same
/// endpoints, filtered to verts on the neighbor's surface). Every
/// impossibility bails the WHOLE pre-pass (`Ok(None)`, all loops
/// untouched) so gate-ON can never pair worse than gate-OFF; the final
/// local pairing audit is the contract.
pub(crate) fn envelope_prepass(
    mesh: &Mesh,
    infos: &[PatchInfo],
    subdivided_cycles: &[Vec<Vec<(u32, u32)>>],
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
    op: cad_primitives::BoolOp,
) -> Result<Option<EnvelopeRewrites>, YangError> {
    // --- 1. Owner rebuilds against the pristine cycles ------------------
    let mut rebuilds: Vec<(usize, LoopRebuild)> = Vec::new();
    for i in 0..infos.len() {
        if let Some(r) =
            rebuild_osculating_loops(mesh, infos, i, subdivided_cycles, intersection_curves, op)?
        {
            rebuilds.push((i, r));
        }
    }
    if rebuilds.is_empty() {
        return Ok(None);
    }

    let mut work: BTreeMap<usize, Vec<Vec<(u32, u32)>>> = BTreeMap::new();
    let mut overrides: BTreeMap<(usize, usize, (u32, u32)), Curve> = BTreeMap::new();
    // Owner curve list per pair — the §10.5 lens allocator: each planar
    // claimant of a multiply-typed pair consumes the next owner curve in
    // order (probe-logged; the orientation audit is the correctness gate).
    let mut owner_edge_curves: BTreeMap<(u32, u32), Vec<Curve>> = BTreeMap::new();
    for (i, rb) in &rebuilds {
        work.insert(*i, rb.cycles.clone());
        for (&(cyc, k), c) in &rb.curve_overrides {
            overrides.insert((*i, cyc, k), *c);
        }
        // Owner curve list = EVERY owner-cycle edge (incl. notch faces)
        // with its RESOLVED curve (override → intersection_curves → Seg),
        // so a neighbor mirror of ANY owner edge — including an
        // un-attributed original chord — types identically to the owner.
        for (cyc, cycle) in rb.cycles.iter().enumerate() {
            for &(s, e) in cycle {
                let k = und(s, e);
                let c = rb
                    .curve_overrides
                    .get(&(cyc, k))
                    .or_else(|| intersection_curves.get(&k))
                    .copied()
                    .unwrap_or(Curve::LineSegment);
                owner_edge_curves.entry(k).or_default().push(c);
            }
        }
        for nf in &rb.notches {
            for &(s, e) in &nf.cycle {
                let k = und(s, e);
                let c = nf
                    .curves
                    .get(&k)
                    .or_else(|| intersection_curves.get(&k))
                    .copied()
                    .unwrap_or(Curve::LineSegment);
                owner_edge_curves.entry(k).or_default().push(c);
            }
        }
    }
    let mut lens_claims: BTreeMap<(u32, u32), usize> = BTreeMap::new();

    // --- 2. Neighbor propagation per committed chain --------------------
    let mut watch: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for (owner_idx, rb) in &rebuilds {
        // Edges now living on a notch face (spec §10.5/§10.8) are REAL
        // topology the neighbors keep byte-identically — they must not
        // read as stale runs.
        let sliver_edges: std::collections::BTreeSet<(u32, u32)> = rb
            .notches
            .iter()
            .flat_map(|nf| nf.cycle.iter().map(|&(s, e)| und(s, e)))
            .collect();
        for chain in &rb.chains {
            watch.extend(chain.old_verts.iter().copied());
            watch.extend(chain.new_verts.iter().copied());
            let n_old = chain.old_verts.len();
            let old_edges: std::collections::BTreeSet<(u32, u32)> = (0..n_old)
                .map(|i| und(chain.old_verts[i], chain.old_verts[(i + 1) % n_old]))
                .filter(|k| !sliver_edges.contains(k))
                .collect();
            let mut pos_in_new: BTreeMap<u32, usize> = BTreeMap::new();
            for (p, &v) in chain.new_verts.iter().enumerate() {
                if pos_in_new.insert(v, p).is_some() {
                    probe(format_args!("prepass BAIL: repeated vert {v} in new chain"));
                    return Ok(None);
                }
            }
            #[allow(clippy::needless_range_loop)] // j indexes infos AND subdivided_cycles
            for j in 0..infos.len() {
                if j == *owner_idx {
                    continue;
                }
                let neighbor_cycles: Vec<Vec<(u32, u32)>> = match work.get(&j) {
                    Some(c) => c.clone(),
                    None => subdivided_cycles[j].clone(),
                };
                let mut changed = false;
                let mut rewritten: Vec<Vec<(u32, u32)>> = Vec::with_capacity(neighbor_cycles.len());
                for (cj, cycle) in neighbor_cycles.iter().enumerate() {
                    match rewrite_neighbor_cycle(
                        mesh,
                        infos,
                        (j, cj),
                        cycle,
                        &old_edges,
                        chain,
                        &pos_in_new,
                        &rb.band_ctx,
                        intersection_curves,
                        &mut overrides,
                        &owner_edge_curves,
                        &mut lens_claims,
                        &rb.sliver_anchors,
                    ) {
                        NeighborRewrite::Unchanged => rewritten.push(cycle.clone()),
                        NeighborRewrite::Rewritten(c) => {
                            probe(format_args!(
                                "prepass: info={j} cycle rewritten {} -> {} edges",
                                cycle.len(),
                                c.len()
                            ));
                            rewritten.push(c);
                            changed = true;
                        }
                        NeighborRewrite::Bail => return Ok(None),
                    }
                }
                if changed {
                    work.insert(j, rewritten);
                }
            }
        }
    }

    // --- 2b. inc-6 (spec §10.9): band-conic typing of the rebuilt chains'
    // ORIGINAL un-attributed chords — the fired-patch slice of the #158/F6
    // rim-vocabulary migration. A surviving rim emitted as a bare
    // `LineSegment` chord makes the render boundary SAG by the chord
    // sagitta (F0082: 1.56e-3 at the (925,959) hop — the §10.8 selfx
    // crossing at the junction corner); typing it with the band-live conic
    // restores arc sampling. The override is recorded for EVERY final loop
    // carrying the edge (owner + planar claimants), so both sides sample
    // the identical curve — conformal by construction, and the pairing
    // audit below re-checks the symmetry. Typing is a REPRESENTATION
    // upgrade, never a repair (P9): it applies only when both endpoints
    // lie ON the conic's carrier plane within the TAU_MODEL band (chain
    // verts are on the owner cylinder by construction, so carrier-plane
    // membership ⇔ on-conic); anything else — wall-complex bands, off-conic
    // endpoints — keeps its original vocabulary.
    {
        let mut typed: Vec<((u32, u32), Curve)> = Vec::new();
        for (owner_idx, rb) in &rebuilds {
            // Only chains whose edge set actually REWROTE: the §3.3 rebuild
            // guarantees on-live-conic membership for those verts. A
            // byte-identical rebuild proves nothing about its verts (the
            // sub-observable band admits off-conic originals up to the
            // floor — measured: an Extrude-10 patch's carrier-plane-passing
            // chord failed the kernel's on-ellipse gate), so it keeps its
            // original vocabulary.
            let chain_edges: std::collections::BTreeSet<(u32, u32)> = rb
                .chains
                .iter()
                .flat_map(|c| {
                    let n = c.new_verts.len();
                    (0..n).map(move |i| und(c.new_verts[i], c.new_verts[(i + 1) % n]))
                })
                .collect();
            if chain_edges.is_empty() {
                continue;
            }
            let cycles_owner: &[Vec<(u32, u32)>] = match work.get(owner_idx) {
                Some(c) => c,
                None => &subdivided_cycles[*owner_idx],
            };
            for (cj, cyc) in cycles_owner.iter().enumerate() {
                for &(s, e) in cyc {
                    let k = und(s, e);
                    if !chain_edges.contains(&k)
                        || intersection_curves.contains_key(&k)
                        || overrides.contains_key(&(*owner_idx, cj, k))
                    {
                        continue;
                    }
                    let Some(conic) = rb.band_ctx.curve_for(mesh, s, e) else {
                        continue; // wall-complex band / outside table: keep Seg
                    };
                    // EXACT on-conic membership for BOTH endpoints — typing
                    // is a representation upgrade, never a repair (P9).
                    if !(on_conic(mesh, s, &conic) && on_conic(mesh, e, &conic)) {
                        continue;
                    }
                    probe(format_args!(
                        "typed chord {s}->{e} with band conic (owner info={owner_idx})"
                    ));
                    typed.push((k, conic));
                }
            }
        }
        for (k, conic) in typed {
            for (j, pristine) in subdivided_cycles.iter().enumerate() {
                let cycles_j: &[Vec<(u32, u32)>] = match work.get(&j) {
                    Some(c) => c,
                    None => pristine,
                };
                for (cj, cyc) in cycles_j.iter().enumerate() {
                    if cyc.iter().any(|&(s, e)| und(s, e) == k) {
                        overrides.insert((j, cj, k), conic);
                    }
                }
            }
        }
    }

    // --- 3. Contract audits over every touched vert ---------------------
    // (a) Pairing: every (undirected pair, resolved curve) incident to a
    //     watched vert must be used by exactly TWO directed edges in
    //     OPPOSITE directions across all final loops (incl. slivers) —
    //     the same contract kernel-v2's from_yang re-checks.
    let resolve = |info: usize, cyc: usize, key: (u32, u32)| -> Curve {
        overrides
            .get(&(info, cyc, key))
            .or_else(|| intersection_curves.get(&key))
            .copied()
            .unwrap_or(Curve::LineSegment)
    };
    let mut uses: BTreeMap<(u32, u32), Vec<(Curve, bool)>> = BTreeMap::new();
    for (j, pristine) in subdivided_cycles.iter().enumerate() {
        let cycles_j: &[Vec<(u32, u32)>] = match work.get(&j) {
            Some(c) => c,
            None => pristine,
        };
        for (cj, cyc) in cycles_j.iter().enumerate() {
            for &(s, e) in cyc {
                if watch.contains(&s) || watch.contains(&e) {
                    uses.entry(und(s, e))
                        .or_default()
                        .push((resolve(j, cj, und(s, e)), s < e));
                }
            }
        }
    }
    // Notch seal faces (§10.8) participate in the pairing exactly like
    // any other loop — their edges supply the second use of the strip
    // boundary (e.g. F0082: (925,926)↔A-top, (926,951)↔wall,
    // (925,951)↔cap overhang).
    for (_, rb) in &rebuilds {
        for nf in &rb.notches {
            for &(s, e) in &nf.cycle {
                if watch.contains(&s) || watch.contains(&e) {
                    let k = und(s, e);
                    let c = nf
                        .curves
                        .get(&k)
                        .or_else(|| intersection_curves.get(&k))
                        .copied()
                        .unwrap_or(Curve::LineSegment);
                    uses.entry(k).or_default().push((c, s < e));
                }
            }
        }
    }
    for (k, us) in &uses {
        // Greedy match: each use pairs with an equal-curve opposite-
        // direction partner; any leftover is a violation.
        let mut matched = vec![false; us.len()];
        for a in 0..us.len() {
            if matched[a] {
                continue;
            }
            let partner = (a + 1..us.len())
                .find(|&b| !matched[b] && us[b].0 == us[a].0 && us[b].1 != us[a].1);
            match partner {
                Some(b) => {
                    matched[a] = true;
                    matched[b] = true;
                }
                None => {
                    probe(format_args!(
                        "prepass BAIL: audit — edge {k:?} uses {:?} do not pair",
                        us.iter().map(|(_, f)| *f).collect::<Vec<_>>()
                    ));
                    return Ok(None);
                }
            }
        }
    }
    // (b) Planarity: watched verts in rewritten PLANAR loops must satisfy
    //     the s6 emission band (defensive mirror of the producer gate).
    for (j, cycles_j) in &work {
        let Surface::Plane { normal, d } = infos[*j].inherited else {
            continue;
        };
        let n = normal.as_array();
        for cyc in cycles_j {
            for &(v, _) in cyc {
                if !watch.contains(&v) {
                    continue;
                }
                let p = mesh.verts[v as usize].as_array();
                let dist = dot(p, n) + d;
                let band = TAU_MODEL * (1.0 + p[0].abs().max(p[1].abs()).max(p[2].abs()));
                if dist.abs() > band {
                    probe(format_args!(
                        "prepass BAIL: vert {v} off info={j} plane by {dist:.3e}"
                    ));
                    return Ok(None);
                }
            }
        }
    }

    // Probe-only forensics (inc-5 spec §10.8): the FINAL local topology
    // around the rewritten chains — every cycle touching a watched vert,
    // with the owning info's surface identity and per-edge resolved
    // curves. Never affects behavior.
    if std::env::var_os("YANG_S5_ENVELOPE_PROBE").is_some() {
        for (j, pristine) in subdivided_cycles.iter().enumerate() {
            let cycles_j: &[Vec<(u32, u32)>] = match work.get(&j) {
                Some(c) => c,
                None => pristine,
            };
            for (cj, cyc) in cycles_j.iter().enumerate() {
                if !cyc
                    .iter()
                    .any(|&(s, e)| watch.contains(&s) || watch.contains(&e))
                {
                    continue;
                }
                let surf = match &infos[j].inherited {
                    Surface::Plane { normal, d } => {
                        let n = normal.as_array();
                        format!("Plane n=({:.6},{:.6},{:.6}) d={d:.6}", n[0], n[1], n[2])
                    }
                    other => format!("{other:?}"),
                };
                let edges: Vec<String> = cyc
                    .iter()
                    .map(|&(s, e)| {
                        let tag = match resolve(j, cj, und(s, e)) {
                            Curve::LineSegment => "Seg",
                            Curve::Circle { .. } => "Cir",
                            Curve::Ellipse { .. } => "Ell",
                            Curve::Parabola { .. } => "Par",
                            Curve::Hyperbola { .. } => "Hyp",
                            Curve::SurfacePair { .. } => "SP",
                        };
                        format!("{s}->{e}:{tag}")
                    })
                    .collect();
                probe(format_args!(
                    "final info={j} cyc={cj} input={:?} face_idx={} {surf}\n[s5-env]   edges: {edges:?}",
                    infos[j].input, infos[j].face_idx
                ));
            }
        }
        for (owner, rb) in &rebuilds {
            for nf in &rb.notches {
                let verts: Vec<u32> = nf.cycle.iter().map(|&(s, _)| s).collect();
                probe(format_args!(
                    "final NOTCH-FACE owner={owner} verts={verts:?} (cavity sense)"
                ));
            }
        }
    }

    let extra_faces: Vec<ExtraFace> = rebuilds
        .into_iter()
        .flat_map(|(owner, rb)| {
            rb.notches
                .into_iter()
                .map(move |nf| (owner, nf.cycle, nf.curves))
        })
        .collect();

    Ok(Some(EnvelopeRewrites {
        cycles: work,
        curve_overrides: overrides,
        extra_faces,
    }))
}

enum NeighborRewrite {
    Unchanged,
    Rewritten(Vec<(u32, u32)>),
    Bail,
}

/// Rewrite ONE neighbor cycle against one owner chain: locate maximal
/// cyclic runs of old-chain edges and replace each with the new chain's
/// sub-path between the same endpoints (shorter arc), filtered to verts
/// on the neighbor's plane. Sub-path edges missing curve attribution get
/// the band-live conic (the planar–planar strip-edge vocabulary).
#[allow(clippy::too_many_arguments)]
fn rewrite_neighbor_cycle(
    mesh: &Mesh,
    infos: &[PatchInfo],
    (j, cj): (usize, usize),
    cycle: &[(u32, u32)],
    old_edges: &std::collections::BTreeSet<(u32, u32)>,
    chain: &ChainRewrite,
    pos_in_new: &BTreeMap<u32, usize>,
    ctx: &BandCurveCtx,
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
    overrides: &mut BTreeMap<(usize, usize, (u32, u32)), Curve>,
    owner_edge_curves: &BTreeMap<(u32, u32), Vec<Curve>>,
    lens_claims: &mut BTreeMap<(u32, u32), usize>,
    sliver_anchors: &BTreeMap<u32, u32>,
) -> NeighborRewrite {
    let m = cycle.len();
    let in_chain: Vec<bool> = cycle
        .iter()
        .map(|&(s, e)| old_edges.contains(&und(s, e)))
        .collect();
    if !in_chain.iter().any(|&b| b) {
        return NeighborRewrite::Unchanged;
    }
    if in_chain.iter().all(|&b| b) {
        probe(format_args!(
            "prepass BAIL: info={j} cycle is a whole-loop chain copy"
        ));
        return NeighborRewrite::Bail;
    }
    let Surface::Plane { normal, d } = infos[j].inherited else {
        probe(format_args!(
            "prepass BAIL: chain run in non-planar neighbor info={j}"
        ));
        return NeighborRewrite::Bail;
    };
    let pl = EnvPlane {
        n: normal.as_array(),
        d,
    };
    let on_neighbor_plane = |v: u32| {
        let p = mesh.verts[v as usize].as_array();
        pl.sd(p).abs() < cad_primitives::TAU_EVAL * (1.0 + norm(p))
    };

    let n_new = chain.new_verts.len();
    // Start the walk on a non-run edge so cyclic runs never split.
    let start = match in_chain.iter().position(|&b| !b) {
        Some(s) => s,
        None => unreachable!("all-run case handled above"),
    };
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(m);
    let mut k = 0usize;
    while k < m {
        let idx = (start + k) % m;
        if !in_chain[idx] {
            out.push(cycle[idx]);
            k += 1;
            continue;
        }
        // Maximal run [idx .. run_end].
        let mut len = 1usize;
        while len < m && in_chain[(idx + len) % m] {
            len += 1;
        }
        let a = cycle[idx].0;
        let b = cycle[(idx + len - 1) % m].1;
        // A run endpoint living only on a residual sliver (its non-pinch
        // anchor) enters the main rail THROUGH the sliver's closing edge
        // (spec §10.5): the path gains an [anchor → pinch] hop.
        let rail_entry = |v: u32| -> Option<(u32, Option<u32>)> {
            if pos_in_new.contains_key(&v) {
                Some((v, None))
            } else {
                sliver_anchors.get(&v).map(|&pinch| (pinch, Some(v)))
            }
        };
        let (Some((rail_a, pre_a)), Some((rail_b, pre_b))) = (rail_entry(a), rail_entry(b)) else {
            probe(format_args!(
                "prepass BAIL: info={j} run endpoint {a}/{b} missing from new chain"
            ));
            return NeighborRewrite::Bail;
        };
        let (&pa, &pb) = (&pos_in_new[&rail_a], &pos_in_new[&rail_b]);
        let mut path: Vec<u32> = Vec::new();
        if let Some(v) = pre_a {
            path.push(v);
        }
        if pa == pb {
            path.push(rail_a);
        } else {
            // Shorter cyclic arc pa -> pb in the new chain.
            let fwd = (pb + n_new - pa) % n_new;
            let bwd = (pa + n_new - pb) % n_new;
            if fwd == bwd {
                probe(format_args!(
                    "prepass BAIL: info={j} ambiguous arc {a}->{b}"
                ));
                return NeighborRewrite::Bail;
            }
            let steps = fwd.min(bwd);
            let forward = fwd <= bwd;
            path.push(rail_a);
            for s in 1..steps {
                let p = if forward {
                    (pa + s) % n_new
                } else {
                    (pa + n_new - s) % n_new
                };
                let v = chain.new_verts[p];
                // Plane filter: interior verts off the neighbor's surface
                // are the OTHER side's chain verts — skipped; the
                // endpoints always survive.
                if on_neighbor_plane(v) {
                    path.push(v);
                }
            }
            path.push(rail_b);
        }
        if let Some(v) = pre_b {
            path.push(v);
        }
        if path.len() < 2 {
            probe(format_args!(
                "prepass BAIL: info={j} degenerate replacement path {a}->{b}"
            ));
            return NeighborRewrite::Bail;
        }
        for w in path.windows(2) {
            let key = und(w[0], w[1]);
            out.push((w[0], w[1]));
            if intersection_curves.contains_key(&key) {
                continue;
            }
            let curve = match owner_edge_curves.get(&key) {
                Some(list) if list.len() == 1 => list[0],
                // §10.5 lens: the owner types this pair differently on
                // different loops (main Seg vs sliver conic); planar
                // claimants consume the owner curves in order. The
                // orientation audit adjudicates the assignment.
                Some(list) => {
                    let idx = lens_claims.entry(key).or_insert(0);
                    if *idx >= list.len() {
                        probe(format_args!(
                            "prepass BAIL: info={j} lens pair {key:?} over-claimed"
                        ));
                        return NeighborRewrite::Bail;
                    }
                    let c = list[*idx];
                    probe(format_args!(
                        "prepass: lens {key:?} claim #{idx} -> info={j} cycle={cj}"
                    ));
                    *idx += 1;
                    c
                }
                None => {
                    let Some(c) = ctx.curve_for(mesh, w[0], w[1]) else {
                        probe(format_args!(
                            "prepass BAIL: info={j} no band conic for new edge {key:?}"
                        ));
                        return NeighborRewrite::Bail;
                    };
                    c
                }
            };
            overrides.insert((j, cj, key), curve);
        }
        k += len;
    }
    // Simple-loop check: every vert appears once.
    let starts: std::collections::BTreeSet<u32> = out.iter().map(|&(s, _)| s).collect();
    if starts.len() != out.len() || out.len() < 3 {
        probe(format_args!(
            "prepass BAIL: info={j} rewritten cycle not a simple loop ({} edges, {} distinct verts)",
            out.len(),
            starts.len()
        ));
        return NeighborRewrite::Bail;
    }
    NeighborRewrite::Rewritten(out)
}

/// Shared-vertex count between another patch's loops and this loop's verts.
fn infos_shared_verts(other: &PatchInfo, vert_set: &std::collections::BTreeSet<u32>) -> usize {
    other
        .cycles
        .iter()
        .flat_map(|c| c.iter().map(|&(s, _)| s))
        .filter(|v| vert_set.contains(v))
        .collect::<std::collections::BTreeSet<u32>>()
        .len()
}

/// (azimuth, axial height) of a point in the patch frame.
fn theta_v(frame: &CylFrame, p: [f64; 3]) -> (f64, f64) {
    let q = sub3(p, frame.ap);
    let v = dot(q, frame.a_hat);
    let w = sub3(
        q,
        [v * frame.a_hat[0], v * frame.a_hat[1], v * frame.a_hat[2]],
    );
    (dot(w, frame.y_hat).atan2(dot(w, frame.x_hat)), v)
}

/// The C0118 combined chord-sagitta observability floor, measured from the
/// ACTUAL loop edges (the probe's computation, kept identical): max chord
/// sagitta of the edges nearest each support, summed; `2·max` if one side
/// has no edges.
fn observability_floor(
    mesh: &Mesh,
    cycles: &[Vec<(u32, u32)>],
    frame: &CylFrame,
    prof_int: &AxialProfile,
    prof_orig: &AxialProfile,
) -> f64 {
    let (mut sag_i, mut sag_j) = (0.0f64, 0.0f64);
    for cycle in cycles {
        for &(s, e) in cycle {
            let ps = mesh.verts[s as usize].as_array();
            let pe = mesh.verts[e as usize].as_array();
            let (ts, _) = theta_v(frame, ps);
            let (te, _) = theta_v(frame, pe);
            let mid = [
                (ps[0] + pe[0]) / 2.0,
                (ps[1] + pe[1]) / 2.0,
                (ps[2] + pe[2]) / 2.0,
            ];
            let (tm, vm) = theta_v(frame, mid);
            let sag = frame.r * (1.0 - (wrap(te - ts) / 2.0).cos());
            if (vm - prof_int.v(tm)).abs() <= (vm - prof_orig.v(tm)).abs() {
                sag_i = sag_i.max(sag);
            } else {
                sag_j = sag_j.max(sag);
            }
        }
    }
    if sag_i > 0.0 && sag_j > 0.0 {
        sag_i + sag_j
    } else {
        2.0 * sag_i.max(sag_j)
    }
}

/// Rebuild one weaving cycle by §3.2.2-band selection. `Ok(None)` = a
/// configuration the selection cannot handle exactly (bail, loop
/// untouched); `Err` = P10 violation after committing.
#[allow(clippy::too_many_arguments)]
fn rebuild_cycle(
    mesh: &Mesh,
    cycle: &[(u32, u32)],
    frame: &CylFrame,
    bands: &EnvelopeBands,
    p_int: &EnvPlane,
    p_orig: &EnvPlane,
    prof_int: &AxialProfile,
    prof_orig: &AxialProfile,
    is_planar_junction: &dyn Fn(u32) -> bool,
) -> Result<Option<RebuiltCycle>, YangError> {
    let n_b = bands.boundaries.len();
    let verts: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
    let vert_pos = |v: u32| mesh.verts[v as usize].as_array();
    let on_plane =
        |pl: &EnvPlane, p: [f64; 3]| pl.sd(p).abs() < cad_primitives::TAU_EVAL * (1.0 + norm(p));
    let theta_of = |v: u32| theta_v(frame, vert_pos(v)).0;

    // Nearest cycle vert to a 3D point, with its distance.
    let nearest = |p: [f64; 3]| -> (u32, f64) {
        let mut best = (u32::MAX, f64::INFINITY);
        for &v in &verts {
            let d = norm(sub3(vert_pos(v), p));
            if d < best.1 {
                best = (v, d);
            }
        }
        best
    };
    let junction_tol = |p: [f64; 3]| TAU_MODEL * (1.0 + norm(p));

    // --- Junction vertices per retained boundary ------------------------
    // WC-flanking crossings and free triples map to ONE existing vert; a
    // standalone crossing (Int↔Orig switch at a wall with no sliver) needs
    // the crossing vert on EACH adjacent live curve. Selection-only: a
    // missing junction vert bails (inc-0 measured every needed junction
    // already minted; insertion is future vocabulary).
    let mut junction_of_boundary: Vec<u32> = Vec::with_capacity(n_b);
    for (bi, b) in bands.boundaries.iter().enumerate() {
        let (v, d) = nearest(b.p);
        probe(format_args!(
            "  junction[{bi}] theta={:.7} kind={:?} -> vert {v} d={d:.3e} (tol {:.3e})",
            b.theta,
            b.kind,
            junction_tol(b.p)
        ));
        if d > junction_tol(b.p) {
            return Ok(None);
        }
        junction_of_boundary.push(v);
    }
    // Standalone-crossing partner verts (per boundary, lazily resolved).
    let crossing_vert_on = |prof: &AxialProfile, theta: f64| -> Option<u32> {
        let p = frame.embed(theta, prof.v(theta));
        let (v, d) = nearest(p);
        (d <= junction_tol(p)).then_some(v)
    };

    let mut junction_set: std::collections::BTreeSet<u32> =
        junction_of_boundary.iter().copied().collect();
    // Resolve standalone-crossing connectors up front so their verts are
    // excluded from band interiors too.
    // connector[i] = verts to emit AT boundary i (between band i-1 and i).
    let mut connectors: Vec<Vec<u32>> = vec![Vec::new(); n_b];
    for i in 0..n_b {
        let prev_band = bands.live[(i + n_b - 1) % n_b];
        let next_band = bands.live[i];
        let b = &bands.boundaries[i];
        match b.kind {
            BoundaryKind::FreeSpaceTriple => {
                connectors[i] = vec![junction_of_boundary[i]];
            }
            BoundaryKind::WallCrossing { .. } => {
                let flanks_wc = matches!(prev_band, BandLive::WallComplex { .. })
                    || matches!(next_band, BandLive::WallComplex { .. });
                if flanks_wc {
                    // Absorbed into the WC section emission.
                    continue;
                }
                // Standalone: need the crossing vert on each live curve.
                let prof_for = |bl: BandLive| match bl {
                    BandLive::IntCurve => Some(prof_int),
                    BandLive::OrigCurve => Some(prof_orig),
                    BandLive::WallComplex { .. } => None,
                };
                let (Some(pp), Some(pn)) = (prof_for(prev_band), prof_for(next_band)) else {
                    return Ok(None);
                };
                let (Some(va), Some(vb)) =
                    (crossing_vert_on(pp, b.theta), crossing_vert_on(pn, b.theta))
                else {
                    return Ok(None);
                };
                connectors[i] = if va == vb { vec![va] } else { vec![va, vb] };
                junction_set.insert(va);
                junction_set.insert(vb);
            }
        }
    }

    // --- Band interiors: on-live-curve verts in azimuth order -----------
    let mut member_of_band: Vec<Vec<u32>> = vec![Vec::new(); n_b];
    for (i, members) in member_of_band.iter_mut().enumerate() {
        let live = bands.live[i];
        let pl = match live {
            BandLive::IntCurve => p_int,
            BandLive::OrigCurve => p_orig,
            BandLive::WallComplex { .. } => continue,
        };
        let start = bands.boundaries[i].theta;
        let end = bands.boundaries[(i + 1) % n_b].theta;
        let width = if n_b == 1 {
            2.0 * std::f64::consts::PI
        } else {
            ccw_offset(end, start).max(f64::MIN_POSITIVE)
        };
        let mut sel: Vec<(f64, u32)> = verts
            .iter()
            .copied()
            .filter(|v| !junction_set.contains(v))
            .filter(|&v| on_plane(pl, vert_pos(v)))
            .filter_map(|v| {
                let off = ccw_offset(theta_of(v), start);
                (off > 0.0 && off < width).then_some((off, v))
            })
            .collect();
        sel.sort_by(|a, b| a.0.total_cmp(&b.0));
        *members = sel.into_iter().map(|(_, v)| v).collect();
    }

    // --- WC sections: byte-identical original subsequences --------------
    // For WC band i (boundaries i → i+1): entry junction = the flanking
    // crossing vert on the PREVIOUS band's live curve, exit = on the NEXT
    // band's (curve-adjacency pairing — the θ order of the two crossings
    // is unrelated to which side they serve; §8 WC364).
    let mut wc_section: Vec<Vec<u32>> = vec![Vec::new(); n_b];
    for (i, section) in wc_section.iter_mut().enumerate() {
        let BandLive::WallComplex { .. } = bands.live[i] else {
            continue;
        };
        let prev_live = bands.live[(i + n_b - 1) % n_b];
        let next_live = bands.live[(i + 1) % n_b];
        let (BandLive::IntCurve | BandLive::OrigCurve) = prev_live else {
            return Ok(None); // adjacent WC bands: outside vocabulary
        };
        let (BandLive::IntCurve | BandLive::OrigCurve) = next_live else {
            return Ok(None);
        };
        let b0 = &bands.boundaries[i];
        let b1 = &bands.boundaries[(i + 1) % n_b];
        let on_int_of = |k: &BoundaryKind| match k {
            BoundaryKind::WallCrossing { on_int_curve, .. } => Some(*on_int_curve),
            BoundaryKind::FreeSpaceTriple => None,
        };
        let (Some(oi0), Some(oi1)) = (on_int_of(&b0.kind), on_int_of(&b1.kind)) else {
            return Ok(None); // WC band must be flanked by wall crossings
        };
        let want_entry_int = prev_live == BandLive::IntCurve;
        let (entry_b, exit_b) = if oi0 == want_entry_int && oi1 != want_entry_int {
            (i, (i + 1) % n_b)
        } else if oi1 == want_entry_int && oi0 != want_entry_int {
            ((i + 1) % n_b, i)
        } else {
            return Ok(None); // both crossings on the same curve: not a sliver
        };
        let entry = junction_of_boundary[entry_b];
        let exit = junction_of_boundary[exit_b];
        let Some(path) = extract_wall_section(
            &verts,
            entry,
            exit,
            b0.theta,
            b1.theta,
            &junction_set,
            &theta_of,
        ) else {
            probe(format_args!(
                "  wc[{i}] entry={entry} exit={exit}: extract FAILED"
            ));
            return Ok(None);
        };
        probe(format_args!(
            "  wc[{i}] entry={entry} exit={exit} path={path:?}"
        ));
        *section = path;
    }

    // --- Assembly (ascending θ; reversed at the end if the original
    // cycle winds the other way) ----------------------------------------
    let mut out: Vec<u32> = Vec::with_capacity(verts.len());
    for i in 0..n_b {
        out.extend_from_slice(&connectors[i]);
        match bands.live[i] {
            BandLive::WallComplex { .. } => out.extend_from_slice(&wc_section[i]),
            _ => out.extend_from_slice(&member_of_band[i]),
        }
    }

    // Original winding: the sign of the total azimuth swept.
    let total: f64 = (0..verts.len())
        .map(|i| wrap(theta_of(verts[(i + 1) % verts.len()]) - theta_of(verts[i])))
        .sum();
    if total < 0.0 {
        out.reverse();
    }

    // --- Bail conditions (can't apply exactly) --------------------------
    // Every dropped vert must lie on a pair curve (dead-side removal);
    // dropping a vert on NEITHER curve would break a foreign feature's
    // conformal subdivision.
    let out_set: std::collections::BTreeSet<u32> = out.iter().copied().collect();
    for &v in &verts {
        let dropped = !out_set.contains(&v);
        let on_pair_curve = on_plane(p_int, vert_pos(v)) || on_plane(p_orig, vert_pos(v));
        if dropped && !on_pair_curve {
            return Ok(None);
        }
    }

    // --- Residual sliver cycles (spec §10.5) ----------------------------
    // A maximal ORIGINAL-cycle run of dropped verts containing a planar
    // JUNCTION (non-owner degree ≥ 3, e.g. F0082's v926) cannot be erased
    // — its arcs carry real solid edges whose traversal direction opposes
    // the monotone main chain (the fold's raison d'être). The run splits
    // off as a residual sliver face: [prev, run.., next] in ORIGINAL
    // traversal order (preserving the paired directions byte-identically)
    // closed with the (next → prev) hop. The main chain's matching
    // (prev, next) hop becomes the lens's other side. Requires prev/next
    // adjacent in the main chain; else fail closed. Junction-free runs
    // (pass-throughs, e.g. v938) stay plain drops.
    let n_orig = verts.len();
    let mut slivers: Vec<SliverSpec> = Vec::new();
    let mut oi = 0usize;
    while oi < n_orig {
        if out_set.contains(&verts[oi]) {
            oi += 1;
            continue;
        }
        let mut run = vec![verts[oi]];
        let mut oj = oi + 1;
        while oj < n_orig && !out_set.contains(&verts[oj % n_orig]) {
            run.push(verts[oj]);
            oj += 1;
        }
        // A run wrapping the array seam would have been caught starting at
        // its true head only if verts[0] is kept; a wrapped run (verts[0]
        // dropped AND verts[n-1] dropped) is out of vocabulary — bail.
        if oi == 0 && !out_set.contains(&verts[n_orig - 1]) {
            return Ok(None);
        }
        if run.iter().any(|&v| is_planar_junction(v)) {
            let prev = verts[(oi + n_orig - 1) % n_orig];
            let next = verts[oj % n_orig];
            // The PINCH anchor lies on BOTH pair curves (the osculation
            // point) and stays on the main chain; the other anchor lives
            // on one curve only and moves to the sliver EXCLUSIVELY
            // (keeping it on main pinches its vertex umbrella into two
            // cones — an odd-χ non-manifold assembly, measured on F0082).
            let on_both = |v: u32| {
                let p = vert_pos(v);
                on_plane(p_int, p) && on_plane(p_orig, p)
            };
            let (pinch, other) = match (on_both(prev), on_both(next)) {
                (true, false) => (prev, next),
                (false, true) => (next, prev),
                _ => {
                    probe(format_args!(
                        "  sliver BAIL: run {run:?} anchors {prev}/{next} lack a unique pinch"
                    ));
                    return Ok(None);
                }
            };
            if !out_set.contains(&pinch) {
                probe(format_args!(
                    "  sliver BAIL: pinch {pinch} not on main chain"
                ));
                return Ok(None);
            }
            let mut cyc = Vec::with_capacity(run.len() + 2);
            cyc.push(prev);
            cyc.extend_from_slice(&run);
            cyc.push(next);
            probe(format_args!(
                "  sliver cycle {cyc:?} (closing {next}->{prev}, pinch {pinch}, main drops {other})"
            ));
            slivers.push(SliverSpec {
                verts: cyc,
                pinch,
                non_pinch: other,
            });
        }
        oi = oj;
    }
    // Remove the non-pinch sliver anchors from the main chain.
    let removals: std::collections::BTreeSet<u32> = slivers.iter().map(|s| s.non_pinch).collect();
    if !removals.is_empty() {
        out.retain(|v| !removals.contains(v));
    }
    let out_set: std::collections::BTreeSet<u32> = out.iter().copied().collect();

    // --- Postconditions (P10: loud after commit) ------------------------
    if out.len() < 3 {
        return Err(non_manifold_at(
            "s5-envelope-degenerate-loop",
            format_args!("rebuilt cycle len {}", out.len()),
        ));
    }
    if out_set.len() != out.len() {
        return Err(non_manifold_at(
            "s5-envelope-repeated-vert",
            format_args!("rebuilt cycle repeats a vertex: {out:?}"),
        ));
    }
    for i in 0..out.len() {
        let a = vert_pos(out[i]);
        let b = vert_pos(out[(i + 1) % out.len()]);
        if norm(sub3(a, b)) < TAU_WORK * (1.0 + norm(a)) {
            return Err(non_manifold_at(
                "s5-envelope-coincident-pair",
                format_args!("verts {} and {}", out[i], out[(i + 1) % out.len()]),
            ));
        }
    }
    Ok(Some(RebuiltCycle { main: out, slivers }))
}

/// A rebuilt owner cycle: the monotone main chain plus any residual
/// sliver cycles split off at masked-triple junction runs (spec §10.5).
/// Each sliver is a vert sequence [prev, run.., next] whose consecutive
/// edges are ORIGINAL cycle edges (direction preserved) and whose closing
/// edge is (next → prev). The pinch anchor (on both pair curves) stays on
/// the main chain; the non-pinch anchor lives on the sliver only.
struct RebuiltCycle {
    main: Vec<u32>,
    slivers: Vec<SliverSpec>,
}

struct SliverSpec {
    verts: Vec<u32>,
    pinch: u32,
    non_pinch: u32,
}

/// The byte-identical original traversal of a wall-complex sliver: the
/// path along the ORIGINAL cycle from `entry` to `exit` whose interior
/// verts all lie inside the sliver's azimuth interval and contain no other
/// junction. Returns the vert sequence INCLUDING both endpoints, ordered
/// entry → exit, or None if no unambiguous path exists.
fn extract_wall_section(
    verts: &[u32],
    entry: u32,
    exit: u32,
    band_t0: f64,
    band_t1: f64,
    junction_set: &std::collections::BTreeSet<u32>,
    theta_of: &dyn Fn(u32) -> f64,
) -> Option<Vec<u32>> {
    if entry == exit {
        return None;
    }
    let m = verts.len();
    let pos_of = |v: u32| -> Option<usize> {
        let mut found = None;
        for (i, &x) in verts.iter().enumerate() {
            if x == v {
                if found.is_some() {
                    return None; // duplicated in cycle: ambiguous
                }
                found = Some(i);
            }
        }
        found
    };
    let (ie, ix) = (pos_of(entry)?, pos_of(exit)?);
    let width = ccw_offset(band_t1, band_t0);
    let pad = 10.0 * ANG_EPS;
    let in_band = |v: u32| {
        let off = ccw_offset(theta_of(v), band_t0);
        off <= width + pad || off >= 2.0 * std::f64::consts::PI - pad
    };
    let walk = |from: usize, to: usize, forward: bool| -> Option<Vec<u32>> {
        let mut path = vec![verts[from]];
        let mut i = from;
        loop {
            i = if forward {
                (i + 1) % m
            } else {
                (i + m - 1) % m
            };
            if i == to {
                path.push(verts[i]);
                return Some(path);
            }
            let v = verts[i];
            if junction_set.contains(&v) || !in_band(v) || path.len() > m {
                return None;
            }
            path.push(v);
        }
    };
    match (walk(ie, ix, true), walk(ie, ix, false)) {
        (Some(p), None) | (None, Some(p)) => Some(p),
        // Both valid: only possible when both are the direct 2-vert hop
        // (a 2-cycle) — ambiguous, bail. Neither: bail.
        _ => None,
    }
}

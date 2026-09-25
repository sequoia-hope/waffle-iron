//! General sweep, increment S1 (spec `specs/b6_general_sweep.md`):
//! [`SweepPath`] — the validated value the sweep assembler will be built on.
//! PURE. Nothing here touches the arena; a `SweepPath` value **is** the
//! evidence that every check below has passed, the way a [`Profile`] or a
//! [`super::PipePath`] is.
//!
//! The path is a [`Chain3d`] (`waffle_types::sketch3d`), so it does not
//! matter whether it was drawn in a 3D sketch or in a planar one — a planar
//! sketch constructs the same chain (`specs/sketch3d.md` §6). The section is
//! an ordinary [`Profile`], positioned by the **pierce rule** (spec §8): its
//! plane is perpendicular to the path's start tangent and the path's start
//! point lies in it. The section is used exactly where it is drawn; nothing
//! auto-centres it, because a member offset from its centreline is the
//! normal case in a frame.
//!
//! ## What S1 computes
//!
//! One [`SweepStation`] per joint (`segments + 1` for an open path) carrying
//!
//! - the **cut plane** the two neighbouring segments share — the bisector
//!   mitre at a sharp corner (spec §4), the plane perpendicular to the
//!   common tangent at a G1 joint or an open end;
//! - the **section frame** on each side of the joint, parallel-transported
//!   along the path (spec §6): constant along a line segment, rigidly
//!   rotating about the axis along an arc segment, and turned by the
//!   *minimal* rotation carrying `t̂ᵢₙ` to `t̂ₒᵤₜ` across a joint;
//! - [`SweepStation::rim`], the ONE rim-point computation both neighbouring
//!   segments use, so the shared rim of spec §3 is shared by construction
//!   rather than by comparing two coincident copies.
//!
//! ## Two corrections to the spec, found here
//!
//! **1. The mitre normal is `normalize(t̂₀ + t̂₁)`, not `normalize(t̂₀ − t̂₁)`.**
//! Spec §4 names the interior bisector and then states two behaviours that
//! only the SUM satisfies: a G1 joint must degenerate to the plane
//! perpendicular to the common tangent (the sum gives `t̂₀`; the difference
//! gives `0/0`), and a 180° reversal must leave the bisector undefined (the
//! sum gives `0`; the difference gives a perfectly good `t̂₀`). The
//! picture-frame check settles it: two rails meeting at a right angle are
//! mitred on the diagonal that makes each rail's OUTER edge the long one,
//! and that is the plane perpendicular to the average tangent.
//!
//! **2. A mitre is only possible between two STRAIGHT segments.** Spec §4
//! says both segments are truncated at the mitre plane and "the truncated
//! section becomes the shared rim loop". For two straight members that is
//! exactly true, and for a good reason: the mitre-plane reflection carries
//! `t̂₀` to `−t̂₁` and agrees with the parallel-transport rotation on every
//! vector perpendicular to `t̂₀`, so the two truncated members are mirror
//! images across the plane and meet it in the SAME curve. A bent member has
//! no such symmetry: its points follow circles about the segment's axis, not
//! the tangent line, and the plane cuts them somewhere else. (Measured: a
//! 90° corner from a straight member into a bend of radius 10 with a section
//! point 1 out along the turn gives `(−1, 1)` from the straight side and
//! `(−1.0625, 1.0596)` from the bend — the rims are different curves, and
//! their true meeting curve would be a cylinder×torus intersection, which is
//! not in the analytic vocabulary at all.) So a non-tangent joint at an arc
//! is refused typed ([`KernelV2Error::SweepMitreAtCurvedJoint`]). This costs
//! nothing the motivating cases want: a bend is drawn as a fillet, and a
//! fillet is tangent by construction (`specs/sketch3d.md` §5).
//!
//! `Chain3d::g1` is **advisory** here: the sweep classifies every joint
//! itself, from the tangents it derives, at the same band the sketch uses
//! (`PATH_TANGENT_TOLERANCE`). A hand-built chain therefore cannot mislabel
//! a corner into a smooth join.
//!
//! Consequence worth stating: **every arc segment is cut by planes
//! perpendicular to its own tangent at both ends**, so an arc segment is
//! exactly the partial revolve of spec §1 — no shear, no correction.
//!
//! ## The corner gates are exact
//!
//! Each gate is a LINEAR functional of the section coordinate, so it is
//! decided by the section's support function ([`section_support`]), exact
//! for polygon, circle and arc-polygon regions alike. No sampling, no band
//! that has to be widened when the section vocabulary grows at S4.

use super::*;
use waffle_types::sketch3d::{Chain3d, Edge3dKind};

/// Angular agreement (`1 − t̂ᵢₙ · t̂ₒᵤₜ`) two unit tangents meeting at a joint
/// must satisfy for the joint to be G1 — deliberately the same band as
/// `PIPE_TANGENT_TOLERANCE` and `waffle_types::path::PATH_TANGENT_TOLERANCE`,
/// so a joint the sketch reports tangent is a joint the sweep treats as
/// tangent.
pub const SWEEP_TANGENT_TOLERANCE: f64 = 1e-9;

/// Relative band for an arc endpoint's distance to its own circle
/// (`| |p − c| − ρ | ≤ tol · ρ`) and for the endpoint's distance out of the
/// arc plane.
pub const SWEEP_ARC_ENDPOINT_TOLERANCE: f64 = 1e-9;

/// Band for the pierce rule (spec §8): `|n̂ · t̂₀|` must be within this of 1,
/// and the path start's distance to the section plane within
/// `tol · (1 + magnitude)`.
pub const SWEEP_PIERCE_TOLERANCE: f64 = 1e-9;

/// Relative clearance the section must keep from an arc segment's revolve
/// axis. Touching pinches a non-manifold seam, crossing self-intersects —
/// the revolve axis-clearance rule (`REVOLVE_MIN_AXIS_CLEARANCE_REL`), which
/// is also what `PIPE_MIN_BEND_CLEARANCE_REL` enforces for a round section.
pub const SWEEP_MIN_AXIS_CLEARANCE_REL: f64 = 1e-9;

/// Relative clearance a segment must retain between its two cut planes, for
/// EVERY point of the section: `L + min(v · g) > tol · (1 + L)`.
pub const SWEEP_MIN_CORNER_CLEARANCE_REL: f64 = 1e-9;

// ---------------------------------------------------------------------------
// small vector helpers (f64;3 — the arena's Point3/UnitVector3 are the
// boundary types, these are the working ones)
// ---------------------------------------------------------------------------

type V3 = [f64; 3];

fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add3(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn len3(a: V3) -> f64 {
    dot3(a, a).sqrt()
}

/// `None` for a non-finite or (near) zero vector — every caller turns that
/// into a typed refusal rather than propagating a NaN.
fn unit3(a: V3) -> Option<V3> {
    let l = len3(a);
    if !l.is_finite() || l <= 0.0 {
        return None;
    }
    let u = scale3(a, 1.0 / l);
    u.iter().all(|c| c.is_finite()).then_some(u)
}

fn as_point(a: V3) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn as_unit(a: V3) -> UnitVector3 {
    UnitVector3 {
        x: a[0],
        y: a[1],
        z: a[2],
    }
}

fn of_point(p: Point3) -> V3 {
    [p.x(), p.y(), p.z()]
}

fn of_unit(u: UnitVector3) -> V3 {
    [u.x, u.y, u.z]
}

fn of_vector(v: Vector3) -> V3 {
    [v.x(), v.y(), v.z()]
}

/// The minimal rotation carrying unit `a` to unit `b`, applied to `v`:
/// Rodrigues in the quaternion `(1 + a·b, a × b)` form,
/// `R v = v + k×v + k×(k×v)/(1 + a·b)` with `k = a × b`.
///
/// Exactly the identity when `a == b` (`k = 0`), which is why the transport
/// needs no G1 special case. Undefined at `a·b = −1`; every caller has
/// already refused that as [`KernelV2Error::SweepCornerReversal`].
fn rotate_between(a: V3, b: V3, v: V3) -> V3 {
    let k = cross3(a, b);
    let c = dot3(a, b);
    let kv = cross3(k, v);
    add3(add3(v, kv), scale3(cross3(k, kv), 1.0 / (1.0 + c)))
}

/// Rodrigues rotation of `v` about unit `axis` by `angle`.
fn rotate_about(axis: V3, angle: f64, v: V3) -> V3 {
    let (s, c) = angle.sin_cos();
    add3(
        add3(scale3(v, c), scale3(cross3(axis, v), s)),
        scale3(axis, dot3(axis, v) * (1.0 - c)),
    )
}

// ---------------------------------------------------------------------------
// section support
// ---------------------------------------------------------------------------

/// `max { d · p : p ∈ region }` in the section's own `(u, v)` coordinates.
///
/// Every corner gate in this module is a linear functional of the section
/// coordinate, so a support value decides it EXACTLY — for a polygon (max
/// over the outer vertices), a circle (`d · c + ρ |d|`) and an arc polygon
/// (the outer vertices plus, for each arc edge, the circle's extreme point
/// in direction `d` when that point lies on the arc) alike. Hole loops lie
/// strictly inside the outer loop and cannot extend the support, so they are
/// not consulted.
///
/// Two shapes are outside [`Profile`]'s contract and answer `+∞`, which
/// fails every gate CLOSED rather than open: a region with no boundary, and
/// an arc edge whose endpoints are antipodal (an arc-polygon edge is the
/// unique MINOR arc, `sweep ∈ (0, π)`, so an antipodal pair does not say
/// which way it bulges and its support cannot be decided).
pub fn section_support(region: &ProfileRegion, d: [f64; 2]) -> f64 {
    let dot2 = |p: Point2| d[0] * p.x() + d[1] * p.y();
    let acc = match region {
        ProfileRegion::Polygon { outer, .. } => outer
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, |m, p| m.max(dot2(p))),
        ProfileRegion::Circle { center, radius } => {
            dot2(*center) + radius * (d[0] * d[0] + d[1] * d[1]).sqrt()
        }
        ProfileRegion::ArcPolygon { outer, .. } => {
            let mut m = f64::NEG_INFINITY;
            for e in outer {
                m = m.max(dot2(e.start())).max(dot2(e.end()));
                if let ProfileEdge::Arc {
                    a,
                    b,
                    center,
                    radius,
                    ..
                } = *e
                {
                    // The circle's extreme point in direction d, counted only
                    // when it lies on this edge's (minor) arc. Membership:
                    // its direction is no further from the arc's mid
                    // direction than an endpoint's is.
                    let dl = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    let (ra, rb) = (
                        [a.x() - center.x(), a.y() - center.y()],
                        [b.x() - center.x(), b.y() - center.y()],
                    );
                    let midway = [(ra[0] + rb[0]) * 0.5, (ra[1] + rb[1]) * 0.5];
                    let ml = (midway[0] * midway[0] + midway[1] * midway[1]).sqrt();
                    let ral = (ra[0] * ra[0] + ra[1] * ra[1]).sqrt();
                    if ml <= SWEEP_ARC_ENDPOINT_TOLERANCE * radius {
                        // Antipodal endpoints: which side the arc bulges is
                        // undecidable, so refuse rather than guess low.
                        return f64::INFINITY;
                    }
                    if dl > 0.0 && ral > 0.0 {
                        let dn = [d[0] / dl, d[1] / dl];
                        let mn = [midway[0] / ml, midway[1] / ml];
                        let on_arc =
                            dn[0] * mn[0] + dn[1] * mn[1] >= (ra[0] * mn[0] + ra[1] * mn[1]) / ral;
                        if on_arc {
                            m = m.max(
                                d[0] * (center.x() + radius * dn[0])
                                    + d[1] * (center.y() + radius * dn[1]),
                            );
                        }
                    }
                }
            }
            m
        }
    };
    if acc.is_finite() {
        acc
    } else {
        f64::INFINITY
    }
}

// ---------------------------------------------------------------------------
// the validated value
// ---------------------------------------------------------------------------

/// The section's orientation at one point of the path: the unit tangent and
/// the two in-plane basis directions the section's `(u, v)` coordinates are
/// read in. Orthonormal, and right- or left-handed about the tangent exactly
/// as the section's own basis is (a sweep never silently mirrors a section).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepFrame {
    /// Unit path tangent this frame is perpendicular to.
    pub tangent: UnitVector3,
    /// Direction of the section's `u` coordinate.
    pub x: UnitVector3,
    /// Direction of the section's `v` coordinate.
    pub y: UnitVector3,
}

/// A joint of the path (or one of its two open ends): the cut plane the
/// adjacent segments share, and the section frame on each side of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepStation {
    /// The joint point, on the path centreline.
    pub point: Point3,
    /// Unit normal of the cut plane through [`Self::point`].
    ///
    /// At an open end and at a G1 joint this is the **canonical tangent**
    /// (the outgoing segment's start tangent, or the incoming segment's end
    /// tangent at the final station) — the same convention `PipePath` uses,
    /// and the one that keeps an arc segment's cut plane containing its own
    /// revolve axis exactly. At a mitred corner it is `normalize(t̂ᵢₙ +
    /// t̂ₒᵤₜ)`.
    pub normal: UnitVector3,
    /// Whether the joint is tangent-continuous within
    /// [`SWEEP_TANGENT_TOLERANCE`]. Both open ends are `true`: their cut
    /// plane is perpendicular to the tangent, which is the degenerate mitre.
    pub g1: bool,
    /// The canonical frame — the OUTGOING one where there is one, the
    /// incoming one at the path end. [`Self::rim`] reads it, so a joint's
    /// rim is computed from ONE side and both neighbours get the same bits.
    ///
    /// Outgoing-preferred for the same reason `PipePath` is: it makes this
    /// frame's tangent exactly the tangent [`Self::normal`] was taken from,
    /// so the station's reported plane and its rim are the same plane down
    /// to the last bit.
    pub frame: SweepFrame,
    /// The section frame on the incoming side; `None` at the path start.
    pub frame_in: Option<SweepFrame>,
    /// The section frame on the outgoing side; `None` at the path end.
    pub frame_out: Option<SweepFrame>,
    /// Where the SECTION's own `(u, v)` origin sits in frame coordinates —
    /// constant along the path, and well defined because the pierce rule
    /// puts the section origin in the plane through the path start.
    ///
    /// It is not usually `(0, 0)`, and that is the point: a member offset
    /// from its centreline is the normal case in a frame (spec §8), so the
    /// section travels where it was drawn rather than being re-centred on
    /// the path.
    pub offset: [f64; 2],
}

impl SweepStation {
    /// Where section coordinate `p` lands on this station's cut plane — the
    /// ONE computation the assembler uses for the rim, from BOTH sides, so
    /// the shared rim of spec §3 is shared by construction.
    ///
    /// At a G1 station the cut plane is perpendicular to the tangent and the
    /// section is used exactly as drawn (no shear at all — the fact that
    /// makes a pipe a special case of the sweep, spec §9). At a mitre the
    /// section is sheared along the tangent onto the plane; [`Self::rim_via`]
    /// with the other side's frame gives the same point.
    pub fn rim(&self, p: Point2) -> Point3 {
        self.rim_via(&self.frame, p)
    }

    /// [`Self::rim`] computed from a nominated side. Public because the
    /// agreement of the two sides is an oracle, not an implementation
    /// detail; the assembler must still use [`Self::rim`].
    pub fn rim_via(&self, frame: &SweepFrame, p: Point2) -> Point3 {
        let origin = of_point(self.point);
        let v = add3(
            scale3(of_unit(frame.x), p.x() + self.offset[0]),
            scale3(of_unit(frame.y), p.y() + self.offset[1]),
        );
        if self.g1 {
            return as_point(add3(origin, v));
        }
        let n = of_unit(self.normal);
        let t = of_unit(frame.tangent);
        let lambda = -dot3(v, n) / dot3(t, n);
        as_point(add3(origin, add3(v, scale3(t, lambda))))
    }

    /// The station's shear covector `n̂ / (t̂ · n̂)` on the given side, or
    /// `None` at a G1 station, where there is no shear. `v · g` is the
    /// signed distance the section point at `v` moves ALONG the tangent.
    fn shear_covector(&self, tangent: V3) -> Option<V3> {
        if self.g1 {
            return None;
        }
        let n = of_unit(self.normal);
        Some(scale3(n, 1.0 / dot3(tangent, n)))
    }
}

/// What a path segment sweeps the section through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SweepSegmentKind {
    /// A straight run: the section is extruded along `direction`.
    Line { direction: UnitVector3 },
    /// A bend: the section is revolved about `center`/`axis` through
    /// `sweep ∈ (0, 2π)` radians, counterclockwise about `axis`. The cut
    /// planes at both ends contain this axis (module docs), so this IS the
    /// partial revolve of spec §1.
    Arc {
        center: Point3,
        axis: UnitVector3,
        radius: f64,
        sweep: f64,
    },
}

/// One segment of a validated path, between two [`SweepStation`]s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepSegment {
    pub kind: SweepSegmentKind,
    /// Centreline start (the previous station's point).
    pub start: Point3,
    /// Centreline end (the next station's point).
    pub end: Point3,
    /// Exact centreline length (`|b − a|`, or `sweep · radius`).
    pub length: f64,
}

/// A validated sweep: a section, a chain of segments, and the stations
/// between them. [`SweepPath::new`] is the only way to obtain one.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepPath {
    section: Profile,
    stations: Vec<SweepStation>,
    segments: Vec<SweepSegment>,
    length: f64,
}

/// Per-edge geometry, derived once in step 2 of [`SweepPath::new`].
struct EdgeGeom {
    start: V3,
    end: V3,
    t_start: V3,
    t_end: V3,
    kind: SweepSegmentKind,
    length: f64,
}

impl SweepPath {
    /// Validate a chain + section pair (module docs; spec §4, §6, §8):
    ///
    /// 1. the section's basis is orthonormal (a skewed basis would shear the
    ///    swept solid), and the **pierce rule** holds — the section plane is
    ///    perpendicular to the path's start tangent and the path's start
    ///    point lies in it;
    /// 2. the chain is non-empty, OPEN (a closed path is S5), chained
    ///    head-to-tail exactly, and every segment is well formed;
    /// 3. every joint is either G1 or a mitre between two STRAIGHT segments,
    ///    and no joint doubles back;
    /// 4. every line segment keeps positive length between its two cut
    ///    planes for every point of the section, and every arc segment's
    ///    section clears its revolve axis.
    ///
    /// The frames are parallel-transported (spec §6) as the stations are
    /// built, so there is no second pass and nothing to keep in step.
    pub fn new(chain: &Chain3d, section: &Profile) -> Result<Self, KernelV2Error> {
        // ---- 1a. section basis -------------------------------------------
        // Profile::new guarantees finiteness and a non-degenerate basis; a
        // sweep additionally needs it ORTHONORMAL, since the section's (u, v)
        // coordinates are read in the transported frame.
        let (su, sv) = (of_vector(section.u()), of_vector(section.v()));
        let tol = crate::profile::CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE;
        if (len3(su) - 1.0).abs() > tol || (len3(sv) - 1.0).abs() > tol || dot3(su, sv).abs() > tol
        {
            return Err(KernelV2Error::SweepSectionBasisNotOrthonormal);
        }

        // ---- 2. chain shape ----------------------------------------------
        if chain.edges.is_empty() {
            return Err(KernelV2Error::SweepPathEmpty);
        }
        if chain.closed {
            return Err(KernelV2Error::SweepClosedPathUnsupported);
        }
        let n = chain.edges.len();
        for i in 0..n - 1 {
            if chain.edges[i].b != chain.edges[i + 1].a {
                return Err(KernelV2Error::SweepPathNotChained { segment: i });
            }
        }
        if n > 1 && chain.edges[n - 1].b == chain.edges[0].a {
            return Err(KernelV2Error::SweepClosedPathUnsupported);
        }

        // ---- 2b. per-segment geometry ------------------------------------
        let mut geoms: Vec<EdgeGeom> = Vec::with_capacity(n);
        for (i, e) in chain.edges.iter().enumerate() {
            geoms.push(edge_geometry(e.a, e.b, e.kind, i)?);
        }

        // ---- 1b. the pierce rule (spec §8) -------------------------------
        let start = geoms[0].start;
        let t0 = geoms[0].t_start;
        let sn = cross3(su, sv); // unit: the basis is orthonormal
        if 1.0 - dot3(sn, t0).abs() > SWEEP_PIERCE_TOLERANCE {
            return Err(KernelV2Error::SweepProfileNotPerpendicular);
        }
        let origin = of_point(section.origin());
        let mag = origin
            .iter()
            .chain(start.iter())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        if dot3(sub3(start, origin), sn).abs() > SWEEP_PIERCE_TOLERANCE * (1.0 + mag) {
            return Err(KernelV2Error::SweepPathDoesNotPierceProfile);
        }

        // ---- 3. stations, with the frames transported as we go -----------
        //
        // Seed: the section's own basis at the start station. `handed` keeps
        // a left-handed section left-handed all the way along.
        let handed = if dot3(sn, t0) >= 0.0 { 1.0 } else { -1.0 };
        let seed = orthonormalize(t0, su, handed);
        // Where the section's own origin sits, in frame coordinates. The
        // pierce rule has just established that it lies in the plane through
        // the path start, so this decomposition is exact.
        let to_origin = sub3(origin, start);
        let offset = [
            dot3(to_origin, of_unit(seed.x)),
            dot3(to_origin, of_unit(seed.y)),
        ];
        let mut carried: Option<SweepFrame> = None;
        let mut stations: Vec<SweepStation> = Vec::with_capacity(n + 1);

        for i in 0..=n {
            let t_in = if i > 0 {
                Some(geoms[i - 1].t_end)
            } else {
                None
            };
            let t_out = if i < n { Some(geoms[i].t_start) } else { None };
            let point = if i < n {
                geoms[i].start
            } else {
                geoms[n - 1].end
            };

            // The frame arriving at this station: transported THROUGH the
            // previous segment (constant along a line, rotating rigidly
            // about the axis along an arc).
            let frame_in = carried.map(|f| transport(&geoms[i - 1], &f));

            let (normal, g1) = match (t_in, t_out) {
                (Some(ti), Some(to)) => {
                    let c = dot3(ti, to);
                    if 1.0 + c <= SWEEP_TANGENT_TOLERANCE {
                        return Err(KernelV2Error::SweepCornerReversal { joint: i });
                    }
                    if 1.0 - c <= SWEEP_TANGENT_TOLERANCE {
                        // Degenerate mitre: the plane perpendicular to the
                        // common tangent, taken from the canonical
                        // (outgoing) side — which is what keeps an arc's cut
                        // plane containing its own axis exactly.
                        (to, true)
                    } else {
                        let straight =
                            |g: &EdgeGeom| matches!(g.kind, SweepSegmentKind::Line { .. });
                        if !straight(&geoms[i - 1]) || !straight(&geoms[i]) {
                            return Err(KernelV2Error::SweepMitreAtCurvedJoint { joint: i });
                        }
                        let bisector = unit3(add3(ti, to))
                            .ok_or(KernelV2Error::SweepCornerReversal { joint: i })?;
                        (bisector, false)
                    }
                }
                (None, Some(to)) => (to, true),
                (Some(ti), None) => (ti, true),
                // Unreachable for `n ≥ 1`, which step 2 guarantees; written
                // as a refusal rather than a panic (crate hard rule 4).
                (None, None) => return Err(KernelV2Error::SweepPathEmpty),
            };

            // The frame leaving this station: the minimal rotation carrying
            // the incoming tangent to the outgoing one (spec §6). Exactly
            // the identity at a joint whose tangents are equal, so a G1
            // joint needs no branch of its own.
            let frame_out = t_out.map(|to| match &frame_in {
                Some(f) => {
                    let x = rotate_between(of_unit(f.tangent), to, of_unit(f.x));
                    orthonormalize(to, x, handed)
                }
                None => seed,
            });
            carried = frame_out;

            let frame = match (frame_out, frame_in) {
                (Some(f), _) | (None, Some(f)) => f,
                (None, None) => return Err(KernelV2Error::SweepPathEmpty),
            };
            stations.push(SweepStation {
                point: as_point(point),
                normal: as_unit(normal),
                g1,
                frame,
                frame_in,
                frame_out,
                offset,
            });
        }

        // ---- 4. corner gates ---------------------------------------------
        let region = section.region();
        for (i, g) in geoms.iter().enumerate() {
            let (a, b) = (&stations[i], &stations[i + 1]);
            match g.kind {
                SweepSegmentKind::Line { direction } => {
                    // Material length at section point v is L + v · g, with
                    // g the difference of the two stations' shear covectors.
                    // Linear in v, so the minimum is a support value.
                    let t = of_unit(direction);
                    let ga = a.shear_covector(t).unwrap_or([0.0; 3]);
                    let gb = b.shear_covector(t).unwrap_or([0.0; 3]);
                    let gv = sub3(ga, gb);
                    // `frame_out` is always `Some` at a segment's start
                    // station; the canonical frame is the same value there
                    // whenever it is not.
                    let f = a.frame_out.unwrap_or(a.frame);
                    let d = [dot3(of_unit(f.x), gv), dot3(of_unit(f.y), gv)];
                    // `min over the section of (p + offset) · d`, exactly:
                    // the functional is linear, so the offset is a constant
                    // and the rest is a support value.
                    let margin = g.length + offset[0] * d[0] + offset[1] * d[1]
                        - section_support(region, [-d[0], -d[1]]);
                    let clearance = SWEEP_MIN_CORNER_CLEARANCE_REL * (1.0 + g.length);
                    if !(margin.is_finite() && margin > clearance) {
                        return Err(KernelV2Error::SweepCornerTooTight { segment: i });
                    }
                }
                SweepSegmentKind::Arc {
                    center,
                    axis,
                    radius,
                    ..
                } => {
                    // The axis lies IN the section plane at this station
                    // (spec §1), so "the section clears the axis" is the
                    // 2D test "every section point is strictly on one side
                    // of a line" — linear, hence exact from the support.
                    let f = a.frame_out.unwrap_or(a.frame);
                    let (fx, fy) = (of_unit(f.x), of_unit(f.y));
                    let ax = of_unit(axis);
                    // Both tripwires restate spec §1: at an arc's own cut
                    // plane the axis lies IN the section plane. Guaranteed by
                    // construction (the station's tangent IS the arc's), so
                    // debug-tier.
                    debug_assert!(
                        dot3(ax, of_unit(f.tangent)).abs() < SWEEP_ARC_ENDPOINT_TOLERANCE
                    );
                    let rel = sub3(of_point(center), of_point(a.point));
                    debug_assert!(
                        dot3(rel, of_unit(f.tangent)).abs()
                            < SWEEP_ARC_ENDPOINT_TOLERANCE * (1.0 + radius)
                    );
                    let a2 = [dot3(ax, fx), dot3(ax, fy)];
                    let a2l = (a2[0] * a2[0] + a2[1] * a2[1]).sqrt();
                    let n2 = [-a2[1] / a2l, a2[0] / a2l];
                    // In SECTION coordinates: the section's own origin is
                    // `offset` away from the station point.
                    let c2 = [dot3(rel, fx) - offset[0], dot3(rel, fy) - offset[1]];
                    let off = c2[0] * n2[0] + c2[1] * n2[1];
                    let hi = section_support(region, n2) - off;
                    let lo = -section_support(region, [-n2[0], -n2[1]]) - off;
                    let clearance = SWEEP_MIN_AXIS_CLEARANCE_REL * (1.0 + radius.max(len3(rel)));
                    let clears =
                        hi.is_finite() && lo.is_finite() && (hi < -clearance || lo > clearance);
                    if !clears {
                        return Err(KernelV2Error::SweepSectionCrossesBendAxis { segment: i });
                    }
                }
            }
        }

        let segments: Vec<SweepSegment> = geoms
            .iter()
            .map(|g| SweepSegment {
                kind: g.kind,
                start: as_point(g.start),
                end: as_point(g.end),
                length: g.length,
            })
            .collect();
        let length = segments.iter().map(|s| s.length).sum();

        Ok(Self {
            section: section.clone(),
            stations,
            segments,
            length,
        })
    }

    /// The section, exactly as given.
    pub fn section(&self) -> &Profile {
        &self.section
    }

    /// The stations, in chain order: `segments().len() + 1` of them.
    pub fn stations(&self) -> &[SweepStation] {
        &self.stations
    }

    /// The segments, in chain order.
    pub fn segments(&self) -> &[SweepSegment] {
        &self.segments
    }

    /// Exact centreline length of the path.
    pub fn length(&self) -> f64 {
        self.length
    }
}

/// Derive one segment's geometry, refusing every malformed shape typed.
fn edge_geometry(a: V3, b: V3, kind: Edge3dKind, i: usize) -> Result<EdgeGeom, KernelV2Error> {
    let invalid = || KernelV2Error::SweepPathEdgeInvalid { segment: i };
    if a.iter().chain(b.iter()).any(|c| !c.is_finite()) {
        return Err(invalid());
    }
    match kind {
        Edge3dKind::Line => {
            let d = sub3(b, a);
            let length = len3(d);
            let t = unit3(d).ok_or_else(invalid)?;
            Ok(EdgeGeom {
                start: a,
                end: b,
                t_start: t,
                t_end: t,
                kind: SweepSegmentKind::Line {
                    direction: as_unit(t),
                },
                length,
            })
        }
        Edge3dKind::Arc {
            center,
            normal,
            radius,
        } => {
            if !(radius.is_finite() && radius > 0.0)
                || center.iter().chain(normal.iter()).any(|c| !c.is_finite())
            {
                return Err(invalid());
            }
            let ax = unit3(normal).ok_or_else(invalid)?;
            let (ra, rb) = (sub3(a, center), sub3(b, center));
            let band = SWEEP_ARC_ENDPOINT_TOLERANCE * radius;
            if (len3(ra) - radius).abs() > band
                || (len3(rb) - radius).abs() > band
                || dot3(ra, ax).abs() > band
                || dot3(rb, ax).abs() > band
            {
                return Err(invalid());
            }
            // CCW sweep from a to b about the arc normal, in (0, 2π).
            let mut sweep = dot3(cross3(ra, rb), ax).atan2(dot3(ra, rb));
            if sweep <= 0.0 {
                sweep += std::f64::consts::TAU;
            }
            if !(sweep.is_finite()
                && sweep > SWEEP_ARC_ENDPOINT_TOLERANCE
                && sweep < std::f64::consts::TAU - SWEEP_ARC_ENDPOINT_TOLERANCE)
            {
                return Err(invalid());
            }
            let t_start = unit3(cross3(ax, ra)).ok_or_else(invalid)?;
            let t_end = unit3(cross3(ax, rb)).ok_or_else(invalid)?;
            Ok(EdgeGeom {
                start: a,
                end: b,
                t_start,
                t_end,
                kind: SweepSegmentKind::Arc {
                    center: as_point(center),
                    axis: as_unit(ax),
                    radius,
                    sweep,
                },
                length: sweep * radius,
            })
        }
    }
}

/// The frame at a segment's far end, transported from its near end: constant
/// along a line, rigidly rotated about the axis along an arc (for a planar
/// curve the rotation-minimizing frame IS the rotation about the curve's own
/// plane normal, which is why an arc segment stays a partial revolve).
fn transport(g: &EdgeGeom, frame: &SweepFrame) -> SweepFrame {
    match g.kind {
        SweepSegmentKind::Line { .. } => *frame,
        SweepSegmentKind::Arc { axis, sweep, .. } => {
            let x = rotate_about(of_unit(axis), sweep, of_unit(frame.x));
            // The tangent is taken from the segment's own end geometry
            // rather than from the rotation, so the frame cannot drift off
            // the path it belongs to.
            orthonormalize(g.t_end, x, handedness(frame))
        }
    }
}

/// `+1` when the frame is right-handed about its tangent (`x × y = t̂`),
/// `−1` when it is left-handed — a section drawn in a left-handed basis is
/// carried along as drawn, never silently mirrored.
fn handedness(frame: &SweepFrame) -> f64 {
    if dot3(
        cross3(of_unit(frame.x), of_unit(frame.y)),
        of_unit(frame.tangent),
    ) >= 0.0
    {
        1.0
    } else {
        -1.0
    }
}

/// Build an orthonormal frame perpendicular to unit `t` whose `x` is `x_hint`
/// projected onto that plane. Applied after every rotation, so accumulated
/// rounding cannot tilt a frame out of its section plane.
fn orthonormalize(t: V3, x_hint: V3, handed: f64) -> SweepFrame {
    let projected = sub3(x_hint, scale3(t, dot3(x_hint, t)));
    // The hint is perpendicular to the tangent by construction at every call
    // site (the section basis under the pierce rule, or a rotation of one),
    // so the projection is a rounding-scale correction and cannot vanish.
    let x = unit3(projected).unwrap_or_else(|| any_perpendicular(t));
    let y = scale3(cross3(t, x), handed);
    SweepFrame {
        tangent: as_unit(t),
        x: as_unit(x),
        y: as_unit(y),
    }
}

/// Some unit vector perpendicular to unit `t` — the unreachable fallback of
/// [`orthonormalize`], written out rather than left to a panic.
fn any_perpendicular(t: V3) -> V3 {
    let seed = if t[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    unit3(cross3(t, seed)).unwrap_or([1.0, 0.0, 0.0])
}

//! §4.3.3 Case-IV corner-phantom census and rule-out (spec
//! `specs/yang_433_case_iv_corner_phantom.md`, inc-0 census + inc-2 rule-out).
//!
//! Yang §4.3.3 (`refs/text/yang2025_hybrid_boolean.txt:518-537`): "if there
//! is no solution in one of the two parametric domains, we regard it as a
//! solving failure and rule out the aforementioned Case IV where the meshes
//! detect intersections that do not exist between the surfaces." Our Stage-4
//! relocation solves each junction vertex onto its carried SURFACES but never
//! asks whether the solution lies within the faces' trimmed domains. R0100's
//! face-15 wall is the measured consequence: a prism cap-corner wedge that
//! clears the cone by 1.33 while the Stage-1 mesh sags 2.26–2.29, minting a
//! mesh-level loop whose relocated corners are exact-but-VIRTUAL pierce
//! points (each violating the loop's remaining prism plane by +3.0/+3.1/+9.3
//! and landing outside the face's station band).
//!
//! The exact per-claim certificate, constant-free: a junction vertex whose
//! carried set is {two same-input surfaces + at least one other-input
//! surface} claims "the B-Rep EDGE between those two faces pierces the
//! other-input surface here". The claim is PHANTOM iff the exact
//! line(edge)×surface solve has no root inside the edge's own segment.
//!
//! * [`census_case_iv_phantom`] reports every claim, its roots, and the
//!   verdict — READ-ONLY, print-only, gated on `YANG_433_PHANTOM` (any value).
//! * [`certify_phantom_loops`] (inc-2, ALWAYS-ON; `YANG_433_RULEOUT=0|off`
//!   is the dev A/B off-knob) lifts the per-claim verdict to the LOOP: a
//!   connected component of A×B intersection edges whose EVERY vertex is a
//!   refuted claim, closed within itself (no cross edge leaves the refuted
//!   set) and cyclic (every vertex of degree ≥ 2, size ≥ 3), is a phantom
//!   intersection loop — the meshes detected an intersection the surfaces do
//!   not have. The caller STOPs typed ([`Stage4InvalidReason::PhantomIntersectionLoop`])
//!   with the §4.5.2 under-resolution certificate (the pierced face's chord
//!   band over the claiming edge's clearance to the surface), so the op-level
//!   refinement ladder shrinks the sag below the clearance and re-runs — the
//!   paper's remedy for a detected local error (`:659-670`). A MIXED loop
//!   (any vertex with a valid, endpoint, curved-edge, unsupported or missing
//!   claim) is never certified (P10: no repair claim under uncertainty).
//!
//! Why the certificate lives here and not at Stage 1: the same geometry —
//! an operand corner buried within one chord sag of a curved face — was
//! measured on 26 corpus cases as a Stage-1 density trigger and regressed
//! six CORRECT ones (spec §7); only the CLOSED refuted loop is exact, and it
//! is only observable after relocation.

use crate::brep::{BRep, InputId};
use crate::geom::Surface;
use crate::Curve;
use std::collections::{BTreeMap, BTreeSet};

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

/// Roots `t` of the segment `p0 + t·(p1−p0)` against the infinite analytic
/// surface, unclamped (the caller judges the segment domain). `None` =
/// unsupported surface kind for this increment (census rows it).
pub(crate) fn segment_surface_roots(p0: [f64; 3], p1: [f64; 3], s: Surface) -> Option<Vec<f64>> {
    let d = sub(p1, p0);
    match s {
        Surface::Plane { normal, d: pd } => {
            let n = normal.as_array();
            let denom = dot(n, d);
            // Parallel edge (measured F0064/F0067: edges LYING IN the target
            // plane — a coplanar/tangential contact, not a pierce claim).
            // Signalled to the caller as None-like via a sentinel: the caller
            // treats an empty root list from a PLANE as `parallel`, never as
            // a phantom refutation. Scale-relative parallelism test: the
            // chord's normal component vs its length.
            let chord = dot(d, d).sqrt();
            if denom.abs() <= 1e-12 * chord {
                return Some(Vec::new());
            }
            Some(vec![-(dot(n, p0) + pd) / denom])
        }
        Surface::Sphere { center, radius } => {
            let w = sub(p0, center.as_array());
            let (qa, qb, qc) = (dot(d, d), 2.0 * dot(w, d), dot(w, w) - radius * radius);
            quad_roots(qa, qb, qc)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let u = unit(axis_dir.as_array());
            let w = sub(p0, axis_point.as_array());
            let proj = |v: [f64; 3]| -> [f64; 3] {
                let a = dot(v, u);
                [v[0] - a * u[0], v[1] - a * u[1], v[2] - a * u[2]]
            };
            let (wp, dp) = (proj(w), proj(d));
            let (qa, qb, qc) = (
                dot(dp, dp),
                2.0 * dot(wp, dp),
                dot(wp, wp) - radius * radius,
            );
            quad_roots(qa, qb, qc)
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let u = unit(axis_dir.as_array());
            let w = sub(p0, apex.as_array());
            let c2 = half_angle.cos().powi(2);
            let (du, wu) = (dot(d, u), dot(w, u));
            let (qa, qb, qc) = (
                du * du - c2 * dot(d, d),
                2.0 * (du * wu - c2 * dot(d, w)),
                wu * wu - c2 * dot(w, w),
            );
            quad_roots(qa, qb, qc)
        }
        _ => None,
    }
}

fn quad_roots(qa: f64, qb: f64, qc: f64) -> Option<Vec<f64>> {
    if qa == 0.0 {
        if qb == 0.0 {
            return Some(Vec::new());
        }
        return Some(vec![-qc / qb]);
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return Some(Vec::new());
    }
    let sq = disc.sqrt();
    Some(vec![(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)])
}

/// Axial station of `p` on the surface's own frame (cone: from the apex along
/// the axis; cylinder: from `axis_point`) — reported so the census can be read
/// against a face's rim stations offline. 0 for planes/spheres.
fn station_of(p: [f64; 3], s: Surface) -> f64 {
    match s {
        Surface::Cone { apex, axis_dir, .. } => {
            dot(sub(p, apex.as_array()), unit(axis_dir.as_array()))
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => dot(sub(p, axis_point.as_array()), unit(axis_dir.as_array())),
        _ => 0.0,
    }
}

/// All B-Rep edges shared by a face carrying `s0` and a face carrying `s1`
/// (as surface values — the carried sets speak surfaces, not face indices).
/// Some producers do not share edge INDICES between adjacent faces (each
/// face's loop cites its own edge records — measured on R0100's extrude
/// prism), so index intersection is followed by a GEOMETRIC fallback: two
/// edges are the same edge when their endpoint points are bitwise-equal as
/// an unordered pair.
fn shared_edges(brep: &BRep, s0: Surface, s1: Surface) -> Vec<u32> {
    let face_edges = |target: Surface| -> Vec<std::collections::BTreeSet<u32>> {
        brep.faces()
            .iter()
            .filter(|f| f.surface == target)
            .map(|f| {
                f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .copied()
                    .collect()
            })
            .collect()
    };
    let (f0s, f1s) = (face_edges(s0), face_edges(s1));
    let mut out = Vec::new();
    for e0 in &f0s {
        for e1 in &f1s {
            for &e in e0.intersection(e1) {
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    // Geometric fallback: unordered endpoint-point identity.
    let key = |ei: u32| -> [[f64; 3]; 2] {
        let e = &brep.edges()[ei as usize];
        let p0 = brep.vertices()[e.start as usize].point.as_array();
        let p1 = brep.vertices()[e.end as usize].point.as_array();
        if p0 <= p1 {
            [p0, p1]
        } else {
            [p1, p0]
        }
    };
    for e0 in &f0s {
        for e1 in &f1s {
            for &ea in e0 {
                let ka = key(ea);
                for &eb in e1 {
                    if eb != ea && key(eb) == ka && !out.contains(&ea) {
                        out.push(ea);
                    }
                }
            }
        }
    }
    out
}

/// The per-claim verdict of the exact certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ClaimVerdict {
    /// A root inside the edge's own segment: the pierce is real.
    Valid,
    /// A root indistinguishable from an edge endpoint: a real B-vertex
    /// tangency, not a verdict.
    EndpointGraze,
    /// The edge lies in / parallel to the target plane: a contact, never a
    /// refuted pierce (F0064/F0067 measured).
    ParallelEdge,
    /// A curved shared edge is outside this certificate — no claim either way.
    CurvedEdge,
    /// The target surface kind has no closed-form solve here.
    Unsupported,
    /// The two same-input surfaces share no B-Rep edge.
    NoSharedEdge,
    /// Every shared LineSegment edge's exact roots fall outside the segment:
    /// the claimed pierce does not exist.
    Phantom,
}

/// One junction vertex's pierce claim, classified.
pub(crate) struct Claim {
    pub vertex: u32,
    pub edge_input: InputId,
    pub target: Surface,
    pub verdict: ClaimVerdict,
    /// For a [`ClaimVerdict::Phantom`] claim: a certified LOWER bound on the
    /// claiming edge's distance to the target surface (65-sample min minus
    /// the Lipschitz slack `len/(2·64)`), the wedge clearance the far mesh's
    /// chord sag must drop under. `None` when no shared edge admits the
    /// signed-distance closed form.
    pub clearance: Option<f64>,
    /// The census row text (roots, stations).
    pub rows: String,
}

/// Certified lower bound on the distance from the segment to the pierced
/// FACE (unsigned — a phantom claim's edge may sit on either side): the
/// 65-sample min over the samples whose axial station lies within the
/// face's own rim-station band (extended by each sample's distance, the
/// guard's conservative superset — `edge_graze` in `boolean/rim_junction`),
/// minus the Lipschitz slack. Samples near the surface's INFINITE extension
/// beyond the face's band do not count: no mesh of this face lives there, so
/// they cannot be what clipped the wedge. `None` when the surface has no
/// closed-form signed distance, no face of `pierced` carries it with a
/// derivable band, or no sample approaches the band.
fn segment_clearance(p0: [f64; 3], p1: [f64; 3], s: Surface, pierced: &BRep) -> Option<f64> {
    const S: usize = 65;
    let bands: Vec<([f64; 3], [f64; 3], f64, f64)> = pierced
        .faces()
        .iter()
        .filter(|f| f.surface == s)
        .filter_map(|f| crate::boolean::face_station_band(f, pierced))
        .collect();
    if bands.is_empty() {
        return None;
    }
    let seg = sub(p1, p0);
    let len = dot(seg, seg).sqrt();
    let mut min_d = f64::INFINITY;
    for i in 0..S {
        let t = i as f64 / (S - 1) as f64;
        let p = [p0[0] + t * seg[0], p0[1] + t * seg[1], p0[2] + t * seg[2]];
        let d = crate::boolean::point_surface_signed(p, s)?.abs();
        let in_band = bands.iter().any(|&(origin, u, lo, hi)| {
            let h = dot(sub(p, origin), u);
            h >= lo - d && h <= hi + d
        });
        if in_band {
            min_d = min_d.min(d);
        }
    }
    if !min_d.is_finite() {
        return None;
    }
    Some((min_d - len / (2.0 * (S - 1) as f64)).max(0.0))
}

/// Every junction vertex's pierce claims, classified by the exact certificate.
/// `inc` is the recomputed post-relocation incidence map: mesh edge ->
/// carried `(InputId, Surface)` entries.
pub(crate) fn classify_claims(
    a: &BRep,
    b: &BRep,
    inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> Vec<Claim> {
    // Per-vertex carried (input, surface) sets, from edge incidence.
    let mut carried: BTreeMap<u32, Vec<(InputId, Surface)>> = BTreeMap::new();
    for (&(s, e), entries) in inc {
        for v in [s, e] {
            let list = carried.entry(v).or_default();
            for &(i, sf) in entries {
                if !list.iter().any(|&(i2, s2)| i2 == i && s2 == sf) {
                    list.push((i, sf));
                }
            }
        }
    }
    let mut claims = Vec::new();
    for (&v, list) in &carried {
        let of = |input: InputId| -> Vec<Surface> {
            list.iter()
                .filter(|&&(i, _)| i == input)
                .map(|&(_, s)| s)
                .collect()
        };
        // A claim in each direction: exactly-two same-input surfaces name a
        // B-Rep edge; every other-input surface is a pierce claim on it.
        for (edge_input, brep_e) in [(InputId::A, a), (InputId::B, b)] {
            let pair = of(edge_input);
            let [s0, s1] = pair[..] else { continue };
            let pierced = of(if edge_input == InputId::A {
                InputId::B
            } else {
                InputId::A
            });
            if pierced.is_empty() {
                continue;
            }
            let pierced_brep = if edge_input == InputId::A { b } else { a };
            let edges = shared_edges(brep_e, s0, s1);
            for target in pierced {
                if edges.is_empty() {
                    claims.push(Claim {
                        vertex: v,
                        edge_input,
                        target,
                        verdict: ClaimVerdict::NoSharedEdge,
                        clearance: None,
                        rows: String::new(),
                    });
                    continue;
                }
                let mut any_in = false;
                let mut any_endpoint = false;
                let mut any_unsupported = false;
                let mut any_curved = false;
                let mut any_parallel = false;
                let mut clearance: Option<f64> = None;
                let mut rows: Vec<String> = Vec::new();
                for &ei in &edges {
                    let e = &brep_e.edges()[ei as usize];
                    if e.curve != Curve::LineSegment {
                        any_curved = true;
                        rows.push(format!("e{ei}:curved({:?})", curve_kind(&e.curve)));
                        continue;
                    }
                    let p0 = brep_e.vertices()[e.start as usize].point.as_array();
                    let p1 = brep_e.vertices()[e.end as usize].point.as_array();
                    match segment_surface_roots(p0, p1, target) {
                        None => {
                            any_unsupported = true;
                            rows.push(format!("e{ei}:unsupported-surface"));
                        }
                        Some(roots) => {
                            // f64-noise endpoint window, reporting-only: a
                            // root indistinguishable from an edge endpoint is
                            // a real B-vertex tangency, not a verdict.
                            let eps = 1e-12;
                            let mut row = format!("e{ei}:");
                            for t in &roots {
                                let p = [
                                    p0[0] + t * (p1[0] - p0[0]),
                                    p0[1] + t * (p1[1] - p0[1]),
                                    p0[2] + t * (p1[2] - p0[2]),
                                ];
                                let inside = *t > eps && *t < 1.0 - eps;
                                let endpoint = (*t >= -eps && *t <= eps)
                                    || (*t >= 1.0 - eps && *t <= 1.0 + eps);
                                any_in |= inside;
                                any_endpoint |= endpoint;
                                row.push_str(&format!(
                                    " t={t:.6}{} st={:.4}",
                                    if inside {
                                        "(IN)"
                                    } else if endpoint {
                                        "(END)"
                                    } else {
                                        "(out)"
                                    },
                                    station_of(p, target)
                                ));
                            }
                            if roots.is_empty() {
                                if matches!(target, Surface::Plane { .. }) {
                                    // Parallel/in-plane edge: a contact, not
                                    // a refuted pierce (F0064/F0067 measured).
                                    any_parallel = true;
                                    row.push_str(" parallel-edge");
                                } else {
                                    // A quadric the infinite line strictly
                                    // misses: the pierce cannot exist.
                                    row.push_str(" no-real-roots");
                                }
                            }
                            // The refuted edge's clearance to the surface: the
                            // smallest over the shared edges is the wedge's.
                            if let Some(g) = segment_clearance(p0, p1, target, pierced_brep) {
                                clearance = Some(clearance.map_or(g, |c: f64| c.min(g)));
                            }
                            rows.push(row);
                        }
                    }
                }
                let verdict = if any_in {
                    ClaimVerdict::Valid
                } else if any_endpoint {
                    ClaimVerdict::EndpointGraze
                } else if any_parallel {
                    ClaimVerdict::ParallelEdge
                } else if any_curved {
                    ClaimVerdict::CurvedEdge
                } else if any_unsupported {
                    ClaimVerdict::Unsupported
                } else {
                    ClaimVerdict::Phantom
                };
                claims.push(Claim {
                    vertex: v,
                    edge_input,
                    target,
                    verdict,
                    clearance: if verdict == ClaimVerdict::Phantom {
                        clearance
                    } else {
                        None
                    },
                    rows: rows.join(" | "),
                });
            }
        }
    }
    claims
}

/// The inc-0 census (spec §3), print-only.
pub(crate) fn census_case_iv_phantom(
    mesh: &crate::Mesh,
    a: &BRep,
    b: &BRep,
    inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) {
    let claims = classify_claims(a, b, inc);
    let mut counts: BTreeMap<ClaimVerdict, usize> = BTreeMap::new();
    for c in &claims {
        *counts.entry(c.verdict).or_default() += 1;
        let name = match c.verdict {
            ClaimVerdict::Valid => "VALID",
            ClaimVerdict::EndpointGraze => "ENDPOINT-GRAZE",
            ClaimVerdict::ParallelEdge => "PARALLEL-EDGE",
            ClaimVerdict::CurvedEdge => "CURVED-EDGE",
            ClaimVerdict::Unsupported => "UNSUPPORTED",
            ClaimVerdict::NoSharedEdge => "NO-SHARED-EDGE",
            ClaimVerdict::Phantom => "PHANTOM-CLAIM",
        };
        let p = mesh.verts[c.vertex as usize].as_array();
        eprintln!(
            "[s433-phantom] v={} p=({:.6},{:.6},{:.6}) claim={:?}-edge x {} {} -> {name} \
             clearance={:?}",
            c.vertex,
            p[0],
            p[1],
            p[2],
            c.edge_input,
            crate::stage4_correct::surface_kind_name(c.target),
            c.rows,
            c.clearance,
        );
    }
    let n = |v: ClaimVerdict| counts.get(&v).copied().unwrap_or(0);
    eprintln!(
        "[s433-phantom] SUMMARY claims={} valid={} phantom={} no_shared_edge={} curved_edge={} \
         parallel={} unsupported={} endpoint={}",
        claims.len(),
        n(ClaimVerdict::Valid),
        n(ClaimVerdict::Phantom),
        n(ClaimVerdict::NoSharedEdge),
        n(ClaimVerdict::CurvedEdge),
        n(ClaimVerdict::ParallelEdge),
        n(ClaimVerdict::Unsupported),
        n(ClaimVerdict::EndpointGraze),
    );
    if let Some(cert) = certify_phantom_loops_from(&claims, inc, a, b) {
        eprintln!(
            "[s433-ruleout] PHANTOM LOOP {:?} demand={:?}",
            cert.loop_vertices, cert.under_resolution
        );
    }
}

/// A certified phantom intersection loop (inc-2).
#[derive(Debug, Clone)]
pub(crate) struct PhantomLoopCertificate {
    /// The loop's vertices, ascending (the smallest names the STOP).
    pub loop_vertices: Vec<u32>,
    /// §4.5.2 under-resolution certificate: the largest ratio of the pierced
    /// face's Stage-1 chord band to a claiming edge's certified clearance over
    /// the loop's claims — the factor the far mesh's sag must shrink by before
    /// it stops clipping the wedge. `None` when no claim admits the closed
    /// form (the ladder then runs its default rungs).
    pub under_resolution: Option<f64>,
}

/// §4.3.3 rule-out gate (inc-2). ALWAYS-ON; `YANG_433_RULEOUT=0|off` is the
/// dev A/B off-knob (the pre-inc-2 behaviour: the phantom loop rides into
/// Stage 6 and surfaces as a misnamed CDT ring reject one crate later).
pub(crate) fn ruleout_enabled() -> bool {
    !matches!(
        std::env::var("YANG_433_RULEOUT").as_deref(),
        Ok("0") | Ok("off")
    )
}

/// The loop-level certificate over the post-relocation incidence (see the
/// module doc). `None` = no closed fully-refuted loop.
pub(crate) fn certify_phantom_loops(
    a: &BRep,
    b: &BRep,
    inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> Option<PhantomLoopCertificate> {
    let claims = classify_claims(a, b, inc);
    certify_phantom_loops_from(&claims, inc, a, b)
}

fn certify_phantom_loops_from(
    claims: &[Claim],
    inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    a: &BRep,
    b: &BRep,
) -> Option<PhantomLoopCertificate> {
    // Fully refuted vertices: at least one claim, and EVERY claim phantom.
    let mut by_vertex: BTreeMap<u32, Vec<&Claim>> = BTreeMap::new();
    for c in claims {
        by_vertex.entry(c.vertex).or_default().push(c);
    }
    let refuted: BTreeSet<u32> = by_vertex
        .iter()
        .filter(|(_, cs)| cs.iter().all(|c| c.verdict == ClaimVerdict::Phantom))
        .map(|(&v, _)| v)
        .collect();
    if refuted.is_empty() {
        return None;
    }
    // A×B intersection edges: incidence entries carrying BOTH inputs.
    let cross: Vec<(u32, u32)> = inc
        .iter()
        .filter(|(_, entries)| {
            entries.iter().any(|&(i, _)| i == InputId::A)
                && entries.iter().any(|&(i, _)| i == InputId::B)
        })
        .map(|(&e, _)| e)
        .collect();
    let loops = closed_refuted_components(&refuted, &cross);
    let lp = loops.into_iter().next()?;
    // The §4.5.2 certificate: pierced-face chord band over claiming-edge
    // clearance, max over the loop's claims.
    let mut demand: Option<f64> = None;
    for &v in &lp {
        for c in by_vertex.get(&v).into_iter().flatten() {
            let Some(g) = c.clearance else { continue };
            if g <= 0.0 || g.is_nan() {
                continue;
            }
            let pierced = match c.edge_input {
                InputId::A => InputId::B,
                InputId::B => InputId::A,
            };
            let Ok(d_eps) = crate::stage3_ssi::chord_tol_for_curved_owner(pierced, a, b, 0, (0, 0))
            else {
                continue;
            };
            let r = d_eps / g;
            if r.is_finite() {
                demand = Some(demand.map_or(r, |d: f64| d.max(r)));
            }
        }
    }
    Some(PhantomLoopCertificate {
        loop_vertices: lp,
        under_resolution: demand,
    })
}

/// The pure graph half (unit-testable): connected components of `cross`
/// edges restricted to `refuted` vertices that are CLOSED (no cross edge
/// joins a component vertex to a vertex outside `refuted`) and CYCLIC (every
/// vertex of degree ≥ 2 within the component, size ≥ 3). Each component is
/// returned ascending; components ascend by their smallest vertex.
pub(crate) fn closed_refuted_components(
    refuted: &BTreeSet<u32>,
    cross: &[(u32, u32)],
) -> Vec<Vec<u32>> {
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for &(s, e) in cross {
        if s == e {
            continue;
        }
        adj.entry(s).or_default().insert(e);
        adj.entry(e).or_default().insert(s);
    }
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut out = Vec::new();
    for &start in refuted {
        if seen.contains(&start) {
            continue;
        }
        let mut comp: BTreeSet<u32> = BTreeSet::new();
        let mut stack = vec![start];
        let mut closed = true;
        while let Some(v) = stack.pop() {
            if !comp.insert(v) {
                continue;
            }
            seen.insert(v);
            let Some(nbrs) = adj.get(&v) else {
                closed = false; // no cross edge at all: not on a loop
                continue;
            };
            if nbrs.len() < 2 {
                closed = false;
            }
            for &w in nbrs {
                if refuted.contains(&w) {
                    stack.push(w);
                } else {
                    closed = false; // the loop continues into a valid corner
                }
            }
        }
        if closed && comp.len() >= 3 {
            out.push(comp.into_iter().collect());
        }
    }
    out
}

fn curve_kind(c: &Curve) -> &'static str {
    match c {
        Curve::LineSegment => "LineSegment",
        Curve::Circle { .. } => "Circle",
        Curve::Ellipse { .. } => "Ellipse",
        Curve::Parabola { .. } => "Parabola",
        Curve::Hyperbola { .. } => "Hyperbola",
        _ => "other",
    }
}

#[cfg(test)]
mod ruleout_tests {
    use super::*;

    fn set(v: &[u32]) -> BTreeSet<u32> {
        v.iter().copied().collect()
    }

    /// R0100's shape: three refuted corners joined in a triangle of cross
    /// edges, nothing else touching them — certified.
    #[test]
    fn isolated_refuted_triangle_is_certified() {
        let cross = [(62, 63), (63, 64), (64, 62), (0, 2), (2, 22)];
        let comps = closed_refuted_components(&set(&[62, 63, 64]), &cross);
        assert_eq!(comps, vec![vec![62, 63, 64]]);
    }

    /// A refuted corner whose loop continues into a VALID corner is a MIXED
    /// loop — never certified (P10).
    #[test]
    fn mixed_loop_is_not_certified() {
        let cross = [(62, 63), (63, 64), (64, 65), (65, 62)];
        let comps = closed_refuted_components(&set(&[62, 63, 64]), &cross);
        assert!(comps.is_empty(), "{comps:?}");
    }

    /// Two refuted corners joined by one edge are a chain, not a loop.
    #[test]
    fn refuted_chain_is_not_a_loop() {
        let cross = [(62, 63)];
        assert!(closed_refuted_components(&set(&[62, 63]), &cross).is_empty());
        // A pendant on an otherwise closed triangle breaks closure too.
        let cross = [(62, 63), (63, 64), (64, 62), (64, 9)];
        assert!(closed_refuted_components(&set(&[62, 63, 64]), &cross).is_empty());
    }

    /// A refuted vertex with no cross edge is not a loop member; an unrelated
    /// certified loop elsewhere still reports.
    #[test]
    fn isolated_refuted_vertex_does_not_block_another_loop() {
        let cross = [(1, 2), (2, 3), (3, 1)];
        let comps = closed_refuted_components(&set(&[1, 2, 3, 40]), &cross);
        assert_eq!(comps, vec![vec![1, 2, 3]]);
    }
}

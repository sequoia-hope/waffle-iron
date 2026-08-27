//! §4.3.3 Case-IV corner-phantom census (spec
//! `specs/yang_433_case_iv_corner_phantom.md`, inc-0).
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
//! line(edge)×surface solve has no root inside the edge's own segment. The
//! census reports every claim, its roots, and the verdict — READ-ONLY,
//! print-only, gated on `YANG_433_PHANTOM` (any value). No behavior change.

use crate::brep::{BRep, InputId};
use crate::geom::Surface;
use crate::Curve;
use std::collections::BTreeMap;

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

/// The inc-0 census (spec §3). `inc` is the recomputed post-relocation
/// incidence map: mesh edge -> carried `(InputId, Surface)` entries.
pub(crate) fn census_case_iv_phantom(
    mesh: &crate::Mesh,
    a: &BRep,
    b: &BRep,
    inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) {
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
    let mut n_claims = 0usize;
    let mut n_valid = 0usize;
    let mut n_phantom = 0usize;
    let mut n_no_edge = 0usize;
    let mut n_curved_edge = 0usize;
    let mut n_unsupported = 0usize;
    let mut n_endpoint = 0usize;
    let mut n_parallel = 0usize;
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
            let edges = shared_edges(brep_e, s0, s1);
            for target in pierced {
                n_claims += 1;
                if edges.is_empty() {
                    n_no_edge += 1;
                    eprintln!(
                        "[s433-phantom] v={v} claim={edge_input:?}-edge x {} : NO-SHARED-EDGE",
                        crate::stage4_correct::surface_kind_name(target)
                    );
                    continue;
                }
                let mut any_in = false;
                let mut any_endpoint = false;
                let mut any_unsupported = false;
                let mut any_curved = false;
                let mut any_parallel = false;
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
                            rows.push(row);
                        }
                    }
                }
                let verdict = if any_in {
                    n_valid += 1;
                    "VALID"
                } else if any_endpoint {
                    n_endpoint += 1;
                    "ENDPOINT-GRAZE"
                } else if any_parallel {
                    n_parallel += 1;
                    "PARALLEL-EDGE"
                } else if any_curved {
                    // A curved shared edge is outside this increment's
                    // certificate — no claim either way.
                    n_curved_edge += 1;
                    "CURVED-EDGE"
                } else if any_unsupported {
                    n_unsupported += 1;
                    "UNSUPPORTED"
                } else {
                    n_phantom += 1;
                    "PHANTOM-CLAIM"
                };
                let p = mesh.verts[v as usize].as_array();
                eprintln!(
                    "[s433-phantom] v={v} p=({:.6},{:.6},{:.6}) claim={edge_input:?}-edge x {} \
                     {} -> {verdict}",
                    p[0],
                    p[1],
                    p[2],
                    crate::stage4_correct::surface_kind_name(target),
                    rows.join(" | "),
                );
            }
        }
    }
    eprintln!(
        "[s433-phantom] SUMMARY claims={n_claims} valid={n_valid} phantom={n_phantom} \
         no_shared_edge={n_no_edge} curved_edge={n_curved_edge} parallel={n_parallel} \
         unsupported={n_unsupported} endpoint={n_endpoint}"
    );
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

//! §I13(f) f2c-3 — the BRIDGE CENSUS instrument, and the measurement
//! that RETIRED the slit (spec `specs/yang_441_trim_cdt_construction.md`
//! §I13(f) item 6).
//!
//! Built to scope a "junction-layer SLIT" repair for R0003's measured
//! main-shell genus-2 ([`crate::stage5_topology::chi_audit_report`]).
//! **The census REFUTED the repair instead (2026-08-28): the genus is
//! TRUE topology.** The density ladder (`YANG_NSEG_FLOOR` 41/82/164,
//! chord sag 1.3e-3 → 8e-5) reads per-component χ [−2, 2, 2] at every
//! rung — converged, not a chord-gap artifact — and the census's own
//! closed-form numbers verify the mechanism: at each of the two graze
//! sites the gear flange keeps a paper-thin film of material arching
//! over the pocket-corner VOID (a point 0.5 under the film's witness is
//! inside all six tool half-spaces; both film ends attach to material).
//! Two real micro-handles ⇒ χ_true = 2 for 3 shells, exactly what the
//! boolean produces. The wrong party is the composition oracle's
//! genus-0-per-shell credit (χ = 2·shells): a false-alarm
//! `SUPPORTED_WRONG`. The rescoped increment hardens this census into
//! the HANDLE CERTIFICATE that lets the oracle accept certified genus
//! without a band (accept iff χ == 2·shells − 2·h with every handle
//! certified; typed decline otherwise).
//!
//! The instrument (read-only, gate `YANG_441_SLIT`, unset =
//! byte-identical): the anchor tool is a convex all-planar prism, so
//! the analytic truth is closed-form — a point is strictly inside the
//! tool iff strictly inside every face half-space. The candidate
//! chords sit OUTSIDE the tool (a box tessellates with zero sag; stage
//! 2 kept them correctly per its mesh-exact contract); what dips
//! inside is the analytic carrier OVER the triangle, so the census
//! lifts barycentric samples onto the attributed carrier (closed-form
//! nearest-point projection) and tests the lifts. One lifted sample
//! strictly inside beyond the evaluation-noise floor is a witness; the
//! floor converts an unreadable margin into the frontier bucket, never
//! into a verdict. Output: bucket counts; blob structure (intrinsic χ,
//! solo-sever, partner histograms, neighborhood adjacency, tube
//! probe); the exact rim-window solves (coaxial-cone rim × tool-plane
//! roots — these located every missing true junction and reproduced
//! every f2b census number); and the would-sever probe whose χ
//! arithmetic refuted delete-and-cap surgery on paper (a disk region
//! re-covered by any disk decomposition of the same loop is
//! χ-invariant).

use crate::brep::{InputId, TriangleAttributionMap};
use crate::{BRep, Mesh, Surface};
use cad_primitives::BoolOp;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn scale3(s: f64, a: [f64; 3]) -> [f64; 3] {
    [s * a[0], s * a[1], s * a[2]]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// §I13(f) f2c-3 gate — `YANG_441_SLIT`. Unset/other = Off (byte-
/// identical). `census` = the read-only bridge census prints at stage-4
/// reconstruct entry. `on` is reserved for the rescoped HANDLE
/// CERTIFICATE mode (no slit apply arm will be built — the slit was
/// refuted by the density-ladder adjudication, see module docs); until
/// that lands it behaves as `census`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlitMode {
    Off,
    Census,
    On,
}

pub(crate) fn slit_mode() -> SlitMode {
    match std::env::var("YANG_441_SLIT") {
        Ok(v) if v == "census" => SlitMode::Census,
        Ok(v) if v == "on" || v == "1" => SlitMode::On,
        _ => SlitMode::Off,
    }
}

/// Closed-form nearest point on an analytic surface. `None` = the
/// projection is not defined at this sample (on the axis of a
/// cylinder/cone, at a sphere's center) or the kind is not lifted by the
/// census (Torus) — callers count these, never guess.
pub(crate) fn project_to_surface(s: &Surface, p: [f64; 3]) -> Option<[f64; 3]> {
    let tiny = |scale: f64| 1e-14 * (1.0 + scale);
    match *s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            let n2 = dot(n, n);
            if !(n2.is_finite() && n2 > 0.0) {
                return None;
            }
            let h = (dot(n, p) + d) / n2;
            Some(sub(p, scale3(h, n)))
        }
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let w = sub(p, c);
            let r = norm(w);
            if r <= tiny(radius.abs()) {
                return None;
            }
            Some(add(c, scale3(radius / r, w)))
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let ap = axis_point.as_array();
            let u = axis_dir.as_array();
            let ul = norm(u);
            if !(ul.is_finite() && ul > 0.0) {
                return None;
            }
            let uh = scale3(1.0 / ul, u);
            let w = sub(p, ap);
            let along = dot(w, uh);
            let rad = sub(w, scale3(along, uh));
            let r = norm(rad);
            if r <= tiny(radius.abs()) {
                return None;
            }
            Some(add(add(ap, scale3(along, uh)), scale3(radius / r, rad)))
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let ax = apex.as_array();
            let u = axis_dir.as_array();
            let ul = norm(u);
            let (sg, cg) = half_angle.sin_cos();
            if !(ul.is_finite() && ul > 0.0 && sg > 0.0 && cg > 0.0) {
                return None;
            }
            let uh = scale3(1.0 / ul, u);
            let w = sub(p, ax);
            let along = dot(w, uh);
            let rad = sub(w, scale3(along, uh));
            let r = norm(rad);
            if r <= tiny(norm(w)) {
                // On the axis the nearest surface point is azimuth-free.
                return None;
            }
            let rh = scale3(1.0 / r, rad);
            // Nearest point on each nappe's generating line through the
            // apex (in the (station, radius) half-plane the +nappe line
            // is t·(cosγ, sinγ), the −nappe t·(−cosγ, sinγ); t ≥ 0).
            let mut best: Option<[f64; 3]> = None;
            let mut best_d = f64::INFINITY;
            for nappe in [1.0f64, -1.0] {
                let t = (nappe * along * cg + r * sg).max(0.0);
                let q = add(ax, add(scale3(t * cg * nappe, uh), scale3(t * sg, rh)));
                let d = norm(sub(q, p));
                if d < best_d {
                    best_d = d;
                    best = Some(q);
                }
            }
            best
        }
        Surface::Torus { .. } => None,
    }
}

/// The tool's face half-spaces as unit-normalized `(n̂, d̂)` pairs,
/// outward positive. `None` unless EVERY face is a plane (the convex
/// all-planar tool is what makes the analytic truth closed-form; any
/// other tool declines the census loudly at the call site).
fn tool_planes(b: &BRep) -> Option<Vec<([f64; 3], f64)>> {
    let mut planes: Vec<([f64; 3], f64)> = Vec::new();
    for f in b.faces() {
        let Surface::Plane { normal, d } = f.surface else {
            return None;
        };
        let n = normal.as_array();
        let len = norm(n);
        if !(len.is_finite() && len > 0.0) {
            return None;
        }
        let (mut nh, mut dh) = (scale3(1.0 / len, n), d / len);
        if f.reversed {
            nh = scale3(-1.0, nh);
            dh = -dh;
        }
        let dup = planes.iter().any(|&(en, ed)| {
            norm(sub(en, nh)) <= 1e-12 && (ed - dh).abs() <= 1e-9 * (1.0 + dh.abs())
        });
        if !dup {
            planes.push((nh, dh));
        }
    }
    (!planes.is_empty()).then_some(planes)
}

/// Convex inside-depth: max over faces of the signed distance (outward
/// positive). Strictly negative = strictly inside the tool.
fn inside_depth(planes: &[([f64; 3], f64)], p: [f64; 3]) -> f64 {
    planes
        .iter()
        .map(|&(n, d)| dot(n, p) + d)
        .fold(f64::NEG_INFINITY, f64::max)
}

/// The rim circle of two COAXIAL cones: `(center, unit axis, radius)`.
/// `None` when the cones are not coaxial at the residual guard, the
/// solve is degenerate (equal slopes), or the rim radius is not
/// strictly positive. Mirrors the f1 planner's closed-form rim
/// construction in the two-surface case.
pub(crate) fn coaxial_cone_rim(ci: &Surface, cj: &Surface) -> Option<([f64; 3], [f64; 3], f64)> {
    let (
        Surface::Cone {
            apex: ai,
            axis_dir: ui,
            half_angle: gi,
        },
        Surface::Cone {
            apex: aj,
            axis_dir: uj,
            half_angle: gj,
        },
    ) = (ci, cj)
    else {
        return None;
    };
    let (ai, aj) = (ai.as_array(), aj.as_array());
    let (ui, uj) = (ui.as_array(), uj.as_array());
    let (li, lj) = (norm(ui), norm(uj));
    if !(li > 0.0 && lj > 0.0) {
        return None;
    }
    let u = scale3(1.0 / li, ui);
    let sj = dot(scale3(1.0 / lj, uj), u);
    if (sj.abs() - 1.0).abs() > 1e-9 {
        return None;
    }
    let off = sub(aj, ai);
    let along = dot(off, u);
    let perp = sub(off, scale3(along, u));
    let scale = norm(ai).max(norm(aj)).max(1.0);
    if norm(perp) > 1e-9 * scale {
        return None;
    }
    // Cone k at station s (from apex_i along u): radius_k(s) =
    // tan(γ_k)·σ_k·(s − a_k), on the nappe where that is ≥ 0.
    let (mi, mj) = (gi.tan(), gj.tan());
    let (si, sjn) = (1.0f64, sj.signum());
    // mi·si·s = mj·σj·(s − along)
    let denom = mi * si - mj * sjn;
    if denom.abs() <= 1e-15 * (mi.abs() + mj.abs()) {
        return None;
    }
    let s = -mj * sjn * along / denom;
    let r = mi * si * s;
    if !(r.is_finite() && r > 1e-12 * scale) {
        return None;
    }
    Some((add(ai, scale3(s, u)), u, r))
}

/// Deterministic in-plane frame `(ê1, ê2)` for a circle with unit axis
/// `u`.
pub(crate) fn circle_frame(u: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
    let pick = if u[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut e1 = sub(pick, scale3(dot(pick, u), u));
    let l1 = norm(e1);
    if l1 <= 0.0 {
        return None;
    }
    e1 = scale3(1.0 / l1, e1);
    let e2 = [
        u[1] * e1[2] - u[2] * e1[1],
        u[2] * e1[0] - u[0] * e1[2],
        u[0] * e1[1] - u[1] * e1[0],
    ];
    Some((e1, e2))
}

/// The 0/1/2 crossing ANGLES of a circle with a plane `(n̂, d̂)` in the
/// [`circle_frame`] parameterization — closed-form (tangency counts as
/// one root within the discriminant's own precision; callers print,
/// never gate, on the count).
pub(crate) fn circle_plane_roots(
    center: [f64; 3],
    u: [f64; 3],
    r: f64,
    n: [f64; 3],
    d: f64,
) -> Vec<f64> {
    let Some((e1, e2)) = circle_frame(u) else {
        return Vec::new();
    };
    let (a, b) = (r * dot(n, e1), r * dot(n, e2));
    let c = dot(n, center) + d;
    let big = a.hypot(b);
    if big <= 0.0 || c.abs() > big {
        return Vec::new();
    }
    let phi = b.atan2(a);
    let alpha = (-c / big).clamp(-1.0, 1.0).acos();
    let mut thetas = vec![phi + alpha];
    if alpha != 0.0 {
        thetas.push(phi - alpha);
    }
    thetas
}

/// One convicted bridge triangle: index, its carrier's `a`-face, the
/// deepest lifted witness and its depth (positive = penetration).
struct BridgeTri {
    tri: u32,
    a_face: u32,
    depth: f64,
    witness: [f64; 3],
}

/// §I13(f) f2c-3 — the read-only BRIDGE CENSUS (see module docs). Prints
/// under `[i13f-slit]`; the mesh is never touched.
pub(crate) fn bridge_census(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    op: BoolOp,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ORDINAL: AtomicUsize = AtomicUsize::new(0);
    let ord = ORDINAL.fetch_add(1, Ordering::Relaxed);
    let tag = format!("[i13f-slit] census#{ord}");
    if op != BoolOp::Subtract {
        eprintln!("{tag}: op={op:?} is not Subtract — census skipped");
        return;
    }
    let Some(planes) = tool_planes(b) else {
        eprintln!(
            "{tag}: tool is not an all-planar B-Rep ({} faces) — census skipped",
            b.faces().len()
        );
        return;
    };
    let scale = mesh
        .verts
        .iter()
        .flat_map(|v| v.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let floor = 64.0 * cad_primitives::TAU_EVAL * (1.0 + scale);
    // The prefilter bound is generous against every measured penetration
    // scale of the family (≤ ~1.2 physical at the I13f sites); the census
    // prints the closest skipped margin so the run itself verifies it.
    const PREFILTER: f64 = 4.0;
    const GRID: usize = 8;
    let vert_margin: Vec<f64> = mesh
        .verts
        .iter()
        .map(|v| inside_depth(&planes, v.as_array()))
        .collect();
    let (mut n_on_tool, mut n_unattr, mut n_unattr_inside_chord) = (0usize, 0usize, 0usize);
    let (mut n_prefiltered, mut n_clean, mut n_frontier, mut n_unlifted) =
        (0usize, 0usize, 0usize, 0usize);
    let mut min_skipped = f64::INFINITY;
    let mut frontier_tris: Vec<u32> = Vec::new();
    let mut bridges: Vec<BridgeTri> = Vec::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        let att = attribution.attributions.get(t).copied().flatten();
        let a_face = match att {
            Some(x) if x.input == InputId::B => {
                n_on_tool += 1;
                continue;
            }
            Some(x) => x.face,
            None => {
                n_unattr += 1;
                if tri.iter().all(|&v| vert_margin[v as usize] < -floor) {
                    n_unattr_inside_chord += 1;
                }
                continue;
            }
        };
        let worst = tri
            .iter()
            .map(|&v| vert_margin[v as usize])
            .fold(f64::INFINITY, f64::min);
        if worst > PREFILTER {
            n_prefiltered += 1;
            min_skipped = min_skipped.min(worst);
            continue;
        }
        let Some(face) = a.faces().get(a_face as usize) else {
            n_unattr += 1;
            continue;
        };
        let surf = face.surface;
        let corners: [[f64; 3]; 3] = [
            mesh.verts[tri[0] as usize].as_array(),
            mesh.verts[tri[1] as usize].as_array(),
            mesh.verts[tri[2] as usize].as_array(),
        ];
        let mut best = f64::INFINITY;
        let mut best_q = corners[0];
        let mut lifted = false;
        for i in 0..=GRID {
            for j in 0..=(GRID - i) {
                let k = GRID - i - j;
                let (bi, bj, bk) = (
                    i as f64 / GRID as f64,
                    j as f64 / GRID as f64,
                    k as f64 / GRID as f64,
                );
                let p = add(
                    add(scale3(bi, corners[0]), scale3(bj, corners[1])),
                    scale3(bk, corners[2]),
                );
                let Some(q) = project_to_surface(&surf, p) else {
                    continue;
                };
                lifted = true;
                let m = inside_depth(&planes, q);
                if m < best {
                    best = m;
                    best_q = q;
                }
            }
        }
        if !lifted {
            n_unlifted += 1;
            continue;
        }
        if best < -floor {
            bridges.push(BridgeTri {
                tri: t as u32,
                a_face,
                depth: -best,
                witness: best_q,
            });
        } else if best < floor {
            n_frontier += 1;
            frontier_tris.push(t as u32);
        } else {
            n_clean += 1;
        }
    }
    eprintln!(
        "{tag}: tris={} tool_planes={} scale={scale:.3e} floor={floor:.3e} grid={GRID} \
         prefilter>{PREFILTER} | clean={n_clean} frontier={n_frontier} bridge={} \
         on_tool={n_on_tool} unattributed={n_unattr} (inside-chord={n_unattr_inside_chord}) \
         unlifted={n_unlifted} prefiltered={n_prefiltered} (closest skipped margin {})",
        mesh.tris.len(),
        planes.len(),
        bridges.len(),
        if min_skipped.is_finite() {
            format!("{min_skipped:.3e}")
        } else {
            "-".into()
        },
    );
    if bridges.is_empty() {
        eprintln!("{tag}: NO bridge triangles — nothing to sever here");
        return;
    }
    // ---- bridge BLOBS: edge-connected components -----------------------
    let bridge_ids: std::collections::BTreeSet<u32> = bridges.iter().map(|b2| b2.tri).collect();
    let mut edge_owner: std::collections::BTreeMap<(u32, u32), Vec<usize>> = Default::default();
    for (i, b2) in bridges.iter().enumerate() {
        let tri = mesh.tris[b2.tri as usize];
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            edge_owner.entry((x.min(y), x.max(y))).or_default().push(i);
        }
    }
    let mut parent: Vec<usize> = (0..bridges.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for owners in edge_owner.values() {
        for w in owners.windows(2) {
            let (ra, rb) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut blobs: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..bridges.len() {
        let r = find(&mut parent, i);
        blobs.entry(r).or_default().push(i);
    }
    let (v0, e0, f0, chi_before, comps0, chis0) =
        crate::stage5_topology::tris_complex_stats(&mesh.tris);
    let mut surf_legend: Vec<(u32, Surface)> = Vec::new();
    eprintln!("{tag}: {} bridge blob(s)", blobs.len());
    for (bi, (_, members)) in blobs.iter().enumerate() {
        let deepest = members
            .iter()
            .max_by(|&&x, &&y| {
                bridges[x]
                    .depth
                    .partial_cmp(&bridges[y].depth)
                    .expect("depths are finite")
            })
            .copied()
            .expect("blob is non-empty");
        let mut faces: Vec<u32> = members.iter().map(|&i| bridges[i].a_face).collect();
        faces.sort_unstable();
        faces.dedup();
        for &f in &faces {
            if !surf_legend.iter().any(|&(lf, _)| lf == f) {
                if let Some(af) = a.faces().get(f as usize) {
                    surf_legend.push((f, af.surface));
                }
            }
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for &i in members {
            let tri = mesh.tris[bridges[i].tri as usize];
            for &v in &tri {
                let p = mesh.verts[v as usize].as_array();
                for c in 0..3 {
                    lo[c] = lo[c].min(p[c]);
                    hi[c] = hi[c].max(p[c]);
                }
            }
        }
        // Intrinsic id-complex type of the blob itself: a DISK (χ=1)
        // cannot sever a handle by deletion+cap; an ANNULUS (χ=0) whose
        // two rim loops get separate caps is the +2 severance shape.
        let blob_tris: Vec<[u32; 3]> = members
            .iter()
            .map(|&i| mesh.tris[bridges[i].tri as usize])
            .collect();
        let (bv, be, bf, bchi, bcomps, _) = crate::stage5_topology::tris_complex_stats(&blob_tris);
        // Per-blob would-sever: delete THIS blob alone (all other
        // witnessed triangles stay) — Δχ/Δcomponents plus the deletion
        // hole's loop count. A boundary-displacement sliver reads
        // disk-like (Δχ=-1, one simple loop); anything else is
        // structure.
        let blob_ids: std::collections::BTreeSet<u32> =
            members.iter().map(|&i| bridges[i].tri).collect();
        let solo: Vec<[u32; 3]> = mesh
            .tris
            .iter()
            .enumerate()
            .filter(|(t, _)| !blob_ids.contains(&(*t as u32)))
            .map(|(_, tri)| *tri)
            .collect();
        let (_, _, _, schi, scomps, _) = crate::stage5_topology::tris_complex_stats(&solo);
        // Boundary-edge partner histogram: what the blob is GLUED to —
        // the other user of each blob-boundary edge, by attribution and
        // carrier. (Roofed tunnel: pocket-wall partners under both long
        // sides; boundary sliver: band on one side, notch chain on the
        // other.)
        let mut partner_hist: std::collections::BTreeMap<String, usize> = Default::default();
        // Distinct carrier-cone pairs across the blob's edges (interior:
        // two members with different `a`-faces; boundary: member face ×
        // an A-attributed partner's face) — each pair's shared rim is a
        // candidate severance rim.
        let mut rim_pairs: std::collections::BTreeSet<(u32, u32)> = Default::default();
        for (&(x, y), owners) in &edge_owner {
            let inside: Vec<usize> = owners
                .iter()
                .copied()
                .filter(|i| members.contains(i))
                .collect();
            if inside.is_empty() {
                continue;
            }
            for w in inside.windows(2) {
                let (fa, fb) = (bridges[w[0]].a_face, bridges[w[1]].a_face);
                if fa != fb {
                    rim_pairs.insert((fa.min(fb), fa.max(fb)));
                }
            }
            for (t, tri) in mesh.tris.iter().enumerate() {
                if blob_ids.contains(&(t as u32))
                    || bridge_ids.contains(&(t as u32))
                    || !tri.contains(&x)
                    || !tri.contains(&y)
                {
                    continue;
                }
                let key = match attribution.attributions.get(t).copied().flatten() {
                    Some(att) => {
                        let faces = match att.input {
                            InputId::A => a.faces(),
                            InputId::B => b.faces(),
                        };
                        if att.input == InputId::A {
                            for &i in &inside {
                                let fm = bridges[i].a_face;
                                if fm != att.face {
                                    rim_pairs.insert((fm.min(att.face), fm.max(att.face)));
                                }
                            }
                        }
                        let kind = faces
                            .get(att.face as usize)
                            .map(|f2| match f2.surface {
                                Surface::Plane { .. } => "plane",
                                Surface::Cone { .. } => "cone",
                                Surface::Cylinder { .. } => "cyl",
                                Surface::Sphere { .. } => "sphere",
                                Surface::Torus { .. } => "torus",
                            })
                            .unwrap_or("?");
                        format!("{:?}#{}({kind})", att.input, att.face)
                    }
                    None => "unattr".into(),
                };
                *partner_hist.entry(key).or_default() += 1;
            }
        }
        let partners: Vec<String> = partner_hist
            .iter()
            .map(|(k, c)| format!("{k}x{c}"))
            .collect();
        let w = bridges[deepest].witness;
        // The binding tool planes at the deepest witness — the two the
        // point is nearest to exiting are the slit's walls.
        let mut by_sd: Vec<(f64, usize)> = planes
            .iter()
            .enumerate()
            .map(|(pi, &(n, d))| (dot(n, w) + d, pi))
            .collect();
        by_sd.sort_by(|a2, b2| b2.0.partial_cmp(&a2.0).expect("finite"));
        let walls: Vec<String> = by_sd
            .iter()
            .take(2)
            .map(|&(sd, pi)| {
                let (n, d) = planes[pi];
                format!(
                    "plane#{pi}(n=({:+.6},{:+.6},{:+.6}),d={:+.6}) sd={sd:+.6e}",
                    n[0], n[1], n[2], d
                )
            })
            .collect();
        eprintln!(
            "{tag}:   blob {bi}: tris={} a_faces={faces:?} depth_max={:.6e} \
             witness=({:.9},{:.9},{:.9}) bbox=({:.4},{:.4},{:.4})..({:.4},{:.4},{:.4}) \
             walls=[{}]",
            members.len(),
            bridges[deepest].depth,
            w[0],
            w[1],
            w[2],
            lo[0],
            lo[1],
            lo[2],
            hi[0],
            hi[1],
            hi[2],
            walls.join(", "),
        );
        eprintln!(
            "{tag}:   blob {bi}: intrinsic V({bv})-E({be})+F({bf})={bchi} comps={bcomps} | \
             solo-sever chi={schi} (delta {}) comps={scomps} | partners: {}",
            schi - chi_before,
            partners.join(" "),
        );
        // RECONNECTION probe: delete THIS blob and measure the shortest
        // surviving edge-path between its wall-glued side and its
        // A-glued side (Dijkstra by physical edge length, cutoff 100).
        // An essential handle bridge reconnects only the long way
        // around; a redundant boundary sliver reconnects within its own
        // footprint scale. Deep blobs serve as in-run controls.
        {
            let mut wall_side: Vec<u32> = Vec::new();
            let mut cone_side: Vec<u32> = Vec::new();
            for (&(x, y), owners) in &edge_owner {
                if !owners.iter().any(|i| members.contains(i)) {
                    continue;
                }
                for (t, tri) in mesh.tris.iter().enumerate() {
                    if bridge_ids.contains(&(t as u32)) || !tri.contains(&x) || !tri.contains(&y) {
                        continue;
                    }
                    match attribution.attributions.get(t).copied().flatten() {
                        Some(att) if att.input == InputId::B => {
                            wall_side.extend([x, y]);
                        }
                        Some(_) => cone_side.extend([x, y]),
                        None => {}
                    }
                }
            }
            wall_side.sort_unstable();
            wall_side.dedup();
            cone_side.sort_unstable();
            cone_side.dedup();
            // The strip's END vertices sit on BOTH chains — measure
            // between the PURE interiors of the two sides, or the probe
            // trivially reads 0 through a shared corner.
            let shared: std::collections::BTreeSet<u32> = wall_side
                .iter()
                .copied()
                .filter(|v| cone_side.contains(v))
                .collect();
            let pure_wall: Vec<u32> = wall_side
                .iter()
                .copied()
                .filter(|v| !shared.contains(v))
                .collect();
            let pure_cone: Vec<u32> = cone_side
                .iter()
                .copied()
                .filter(|v| !shared.contains(v))
                .collect();
            let reconnect = if pure_wall.is_empty() || pure_cone.is_empty() {
                format!(
                    "one-sided (wall {} cone {} shared {})",
                    wall_side.len(),
                    cone_side.len(),
                    shared.len()
                )
            } else {
                let src = pure_wall[pure_wall.len() / 2];
                let targets: std::collections::BTreeSet<u32> = pure_cone.iter().copied().collect();
                // Dijkstra on the solo-severed mesh's edge graph.
                let mut adj2: std::collections::BTreeMap<u32, Vec<(u32, f64)>> = Default::default();
                for (t, tri) in mesh.tris.iter().enumerate() {
                    if blob_ids.contains(&(t as u32)) {
                        continue;
                    }
                    for k in 0..3 {
                        let (x, y) = (tri[k], tri[(k + 1) % 3]);
                        let d2 = norm(sub(
                            mesh.verts[x as usize].as_array(),
                            mesh.verts[y as usize].as_array(),
                        ));
                        adj2.entry(x).or_default().push((y, d2));
                        adj2.entry(y).or_default().push((x, d2));
                    }
                }
                const CUTOFF: f64 = 100.0;
                let mut dist: std::collections::BTreeMap<u32, f64> = Default::default();
                let mut heap: std::collections::BinaryHeap<(std::cmp::Reverse<u64>, u32)> =
                    Default::default();
                let key = |d2: f64| std::cmp::Reverse((d2 * 1e6) as u64);
                dist.insert(src, 0.0);
                heap.push((key(0.0), src));
                let mut hit: Option<(u32, f64)> = None;
                while let Some((_, v)) = heap.pop() {
                    let dv = dist[&v];
                    if targets.contains(&v) {
                        hit = Some((v, dv));
                        break;
                    }
                    if dv > CUTOFF {
                        break;
                    }
                    for &(w2, l) in adj2.get(&v).into_iter().flatten() {
                        let nd = dv + l;
                        if nd < *dist.get(&w2).unwrap_or(&f64::INFINITY) {
                            dist.insert(w2, nd);
                            heap.push((key(nd), w2));
                        }
                    }
                }
                match hit {
                    Some((v, d2)) => format!("v{src}->v{v} dist={d2:.3}"),
                    None => format!("v{src}: NO PATH within {CUTOFF}"),
                }
            };
            eprintln!(
                "{tag}:   blob {bi}: reconnect[wall-side {} verts, cone-side {}]: {}",
                wall_side.len(),
                cone_side.len(),
                reconnect
            );
        }
        // Per-triangle witness detail for small blobs: which tool plane
        // binds each triangle's deepest lift — the window's composition.
        if members.len() <= 8 {
            for &i in members {
                let b2 = &bridges[i];
                let wtn = b2.witness;
                let bind = planes
                    .iter()
                    .enumerate()
                    .map(|(pi, &(n, d))| (dot(n, wtn) + d, pi))
                    .max_by(|x2, y2| x2.0.partial_cmp(&y2.0).expect("finite"))
                    .map(|(_, pi)| pi)
                    .unwrap_or(usize::MAX);
                eprintln!(
                    "{tag}:   blob {bi}: tri t{} a_face={} depth={:.3e} binds=plane#{bind} \
                     witness=({:.6},{:.6},{:.6})",
                    b2.tri, b2.a_face, b2.depth, wtn[0], wtn[1], wtn[2]
                );
            }
        }
        // Local FACE-ADJACENCY map for small blobs: every mesh triangle
        // within 2.0 of the blob's inflated bbox, grouped by (input,
        // face), plus the shared-edge adjacency between groups — the
        // reconstructible local topology around the bridge (what the
        // strip, the wall sliver, and the flange's far side are glued
        // to). Larger blobs elide (their character is the notch-outline
        // ribbon, printed above).
        if members.len() <= 8 {
            let pad = 2.0;
            let near_tris: Vec<u32> = (0..mesh.tris.len() as u32)
                .filter(|&t| {
                    mesh.tris[t as usize].iter().any(|&v| {
                        let p = mesh.verts[v as usize].as_array();
                        (0..3).all(|c| p[c] >= lo[c] - pad && p[c] <= hi[c] + pad)
                    })
                })
                .collect();
            let group = |t: u32| -> String {
                match attribution.attributions.get(t as usize).copied().flatten() {
                    Some(att) => format!("{:?}#{}", att.input, att.face),
                    None => "unattr".into(),
                }
            };
            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            for &t in &near_tris {
                *counts.entry(group(t)).or_default() += 1;
            }
            let mut adj_edges: std::collections::BTreeMap<(String, String), usize> =
                Default::default();
            let mut euse: std::collections::BTreeMap<(u32, u32), Vec<u32>> = Default::default();
            for &t in &near_tris {
                let tri = mesh.tris[t as usize];
                for k in 0..3 {
                    let (x, y) = (tri[k], tri[(k + 1) % 3]);
                    euse.entry((x.min(y), x.max(y))).or_default().push(t);
                }
            }
            for owners in euse.values() {
                for w in owners.windows(2) {
                    let (ga, gb) = (group(w[0]), group(w[1]));
                    if ga != gb {
                        let key = if ga < gb { (ga, gb) } else { (gb, ga) };
                        *adj_edges.entry(key).or_default() += 1;
                    }
                }
            }
            let cs: Vec<String> = counts.iter().map(|(g, c)| format!("{g}x{c}")).collect();
            eprintln!(
                "{tag}:   blob {bi}: neighborhood({} tris, pad {pad}): {}",
                near_tris.len(),
                cs.join(" ")
            );
            let es: Vec<String> = adj_edges
                .iter()
                .map(|((ga, gb), c)| format!("{ga}~{gb}x{c}"))
                .collect();
            eprintln!("{tag}:   blob {bi}: adjacency: {}", es.join(" "));
            // TUBE PROBE: the sub-complex [blob ∪ neighborhood triangles
            // whose carrier plane coincides with either of the blob's two
            // wall planes, from either input — the coplanar overlay face
            // counts]. An annular tube (intrinsic χ=0 with exactly two
            // boundary loops) is the handle's cross-section made
            // manifest: slitting it is the measured +2.
            let wall_pis: Vec<usize> = by_sd.iter().take(2).map(|&(_, pi)| pi).collect();
            let on_wall_plane = |t: u32| -> bool {
                let Some(att) = attribution.attributions.get(t as usize).copied().flatten() else {
                    return false;
                };
                let faces = match att.input {
                    InputId::A => a.faces(),
                    InputId::B => b.faces(),
                };
                let Some(Surface::Plane { normal, d }) =
                    faces.get(att.face as usize).map(|f2| f2.surface)
                else {
                    return false;
                };
                let n = normal.as_array();
                let len = norm(n);
                if !(len.is_finite() && len > 0.0) {
                    return false;
                }
                let (nh, dh) = (scale3(1.0 / len, n), d / len);
                wall_pis.iter().any(|&pi| {
                    let (pn, pd) = planes[pi];
                    let c = dot(nh, pn);
                    (c.abs() - 1.0).abs() <= 1e-9
                        && (dh * c.signum() - pd).abs() <= 1e-6 * (1.0 + pd.abs())
                })
            };
            let tube: Vec<[u32; 3]> = near_tris
                .iter()
                .copied()
                .filter(|&t| blob_ids.contains(&t) || on_wall_plane(t))
                .map(|t| mesh.tris[t as usize])
                .collect();
            let (tv, te, tf, tchi, tcomps, tchis) =
                crate::stage5_topology::tris_complex_stats(&tube);
            let mut tuse: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
            for tri in &tube {
                for k in 0..3 {
                    let (x, y) = (tri[k], tri[(k + 1) % 3]);
                    *tuse.entry((x.min(y), x.max(y))).or_default() += 1;
                }
            }
            let tb: Vec<(u32, u32)> = tuse
                .iter()
                .filter(|&(_, &c)| c == 1)
                .map(|(&e, _)| e)
                .collect();
            let mut tadj: std::collections::BTreeMap<u32, usize> = Default::default();
            for &(x, y) in &tb {
                *tadj.entry(x).or_default() += 1;
                *tadj.entry(y).or_default() += 1;
            }
            let tnonsimple = tadj.values().filter(|&&c| c != 2).count();
            let tloops = if tnonsimple == 0 && !tb.is_empty() {
                let mut vis: std::collections::BTreeSet<u32> = Default::default();
                let mut nl = 0usize;
                let mut adj2: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
                for &(x, y) in &tb {
                    adj2.entry(x).or_default().push(y);
                    adj2.entry(y).or_default().push(x);
                }
                for &(s, _) in &tb {
                    if vis.contains(&s) {
                        continue;
                    }
                    nl += 1;
                    let (mut prev, mut cur) = (s, adj2[&s][0]);
                    vis.insert(s);
                    while cur != s {
                        vis.insert(cur);
                        let ns = &adj2[&cur];
                        let nxt = if ns[0] == prev { ns[1] } else { ns[0] };
                        prev = cur;
                        cur = nxt;
                    }
                }
                nl as i64
            } else {
                -1
            };
            eprintln!(
                "{tag}:   blob {bi}: tube-probe[blob+walls{wall_pis:?}]: V({tv})-E({te})+\
                 F({tf})={tchi} comps={tcomps} per-comp={tchis:?} boundary-loops={tloops} \
                 nonsimple={tnonsimple}",
            );
        }
        // The TRUE severance map: for each adjacent carrier-cone pair,
        // the closed-form rim circle and its EXACT crossings with the
        // blob's two wall planes — the [B..C] window ends. A root that
        // is strictly inside every OTHER tool half-space is a genuine
        // severance junction (the rim truly enters the tool there); the
        // in-tool arc between two such roots is the severed rim window.
        if rim_pairs.len() > 6 {
            eprintln!(
                "{tag}:   blob {bi}: {} rim pair(s) — detail elided (blob spans the \
                 notch outline)",
                rim_pairs.len()
            );
        } else {
            for &(fa, fb) in &rim_pairs {
                let (Some(pfa), Some(pfb)) =
                    (a.faces().get(fa as usize), a.faces().get(fb as usize))
                else {
                    continue;
                };
                let Some((rc, ru, rr)) = coaxial_cone_rim(&pfa.surface, &pfb.surface) else {
                    eprintln!("{tag}:   blob {bi}: rim({fa},{fb}): not a coaxial-cone rim");
                    continue;
                };
                let Some((e1, e2)) = circle_frame(ru) else {
                    continue;
                };
                let at = |t: f64| add(rc, add(scale3(rr * t.cos(), e1), scale3(rr * t.sin(), e2)));
                let depth_others = |p: [f64; 3], skip: usize| {
                    planes
                        .iter()
                        .enumerate()
                        .filter(|&(i, _)| i != skip)
                        .map(|(_, &(n, d))| dot(n, p) + d)
                        .fold(f64::NEG_INFINITY, f64::max)
                };
                for &(_, pi) in by_sd.iter().take(2) {
                    let (n, d) = planes[pi];
                    let roots = circle_plane_roots(rc, ru, rr, n, d);
                    let mut root_str: Vec<String> = Vec::new();
                    for &t in &roots {
                        let p = at(t);
                        let (mut bv2, mut bd) = (u32::MAX, f64::INFINITY);
                        for (vi, vp) in mesh.verts.iter().enumerate() {
                            let dd = norm(sub(vp.as_array(), p));
                            if dd < bd {
                                bd = dd;
                                bv2 = vi as u32;
                            }
                        }
                        root_str.push(format!(
                            "({:.6},{:.6},{:.6}) oth={:+.4e} near=v{bv2}@{:.3e}",
                            p[0],
                            p[1],
                            p[2],
                            depth_others(p, pi),
                            bd
                        ));
                    }
                    let window = if roots.len() == 2 {
                        let wrap = |mut x: f64| {
                            while x < 0.0 {
                                x += std::f64::consts::TAU;
                            }
                            while x >= std::f64::consts::TAU {
                                x -= std::f64::consts::TAU;
                            }
                            x
                        };
                        let dl = wrap(roots[1] - roots[0]);
                        let mids = [
                            (dl, at(roots[0] + dl / 2.0)),
                            (
                                std::f64::consts::TAU - dl,
                                at(roots[0] + dl / 2.0 + std::f64::consts::PI),
                            ),
                        ];
                        mids.iter()
                            .map(|&(arc, m)| {
                                format!(
                                    "arc(len={:.4}, mid_depth={:+.4e})",
                                    rr * arc,
                                    inside_depth(&planes, m)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" / ")
                    } else {
                        "-".into()
                    };
                    eprintln!(
                        "{tag}:   blob {bi}: rim({fa},{fb}) r={rr:.4} x plane#{pi}: \
                         {} root(s) [{}] window: {window}",
                        roots.len(),
                        root_str.join(", "),
                    );
                }
            }
        }
    }
    for (f, s) in &surf_legend {
        eprintln!("{tag}:   a_face {f}: {s:?}");
    }
    // ---- WOULD-SEVER probe (scratch, read-only) ------------------------
    let kept: Vec<[u32; 3]> = mesh
        .tris
        .iter()
        .enumerate()
        .filter(|(t, _)| !bridge_ids.contains(&(*t as u32)))
        .map(|(_, tri)| *tri)
        .collect();
    let (v1, e1, f1, chi1, comps1, chis1) = crate::stage5_topology::tris_complex_stats(&kept);
    eprintln!(
        "{tag}: would-sever: BEFORE V({v0})-E({e0})+F({f0})={chi_before} components={comps0} \
         per-component-chi={chis0:?}"
    );
    eprintln!(
        "{tag}: would-sever: AFTER  V({v1})-E({e1})+F({f1})={chi1} components={comps1} \
         per-component-chi={chis1:?}"
    );
    // Boundary loops of the severed complex: undirected edges used once.
    let mut uses: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &kept {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *uses.entry((x.min(y), x.max(y))).or_default() += 1;
        }
    }
    let boundary: Vec<(u32, u32)> = uses
        .iter()
        .filter(|&(_, &c)| c == 1)
        .map(|(&e, _)| e)
        .collect();
    let mut adj: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
    for &(x, y) in &boundary {
        adj.entry(x).or_default().push(y);
        adj.entry(y).or_default().push(x);
    }
    let nonsimple: Vec<u32> = adj
        .iter()
        .filter(|(_, ns)| ns.len() != 2)
        .map(|(&v, _)| v)
        .collect();
    if !nonsimple.is_empty() {
        eprintln!(
            "{tag}: would-sever: boundary is NON-SIMPLE at {} vertex(es) {:?}",
            nonsimple.len(),
            &nonsimple[..nonsimple.len().min(8)]
        );
        for &v in nonsimple.iter().take(8) {
            let p = mesh.verts[v as usize].as_array();
            let cs = crate::stage4_correct::carried_surfaces(
                mesh,
                &attribution.attributions,
                a,
                b,
                v,
                p,
            );
            let kinds: Vec<&str> = cs
                .iter()
                .map(|s| match s {
                    Surface::Plane { .. } => "plane",
                    Surface::Cone { .. } => "cone",
                    Surface::Cylinder { .. } => "cyl",
                    Surface::Sphere { .. } => "sphere",
                    Surface::Torus { .. } => "torus",
                })
                .collect();
            eprintln!(
                "{tag}:   non-simple v{v} ({:.4},{:.4},{:.4}) carriers={kinds:?}",
                p[0], p[1], p[2]
            );
        }
    }
    let mut visited: std::collections::BTreeSet<u32> = Default::default();
    let mut loops: Vec<Vec<u32>> = Vec::new();
    for &(start, _) in &boundary {
        if visited.contains(&start) {
            continue;
        }
        let mut cyc = vec![start];
        visited.insert(start);
        let mut prev = start;
        let mut cur = adj[&start][0];
        while cur != start {
            if visited.contains(&cur) || adj[&cur].len() != 2 {
                cyc.clear();
                break;
            }
            visited.insert(cur);
            cyc.push(cur);
            let ns = &adj[&cur];
            let nxt = if ns[0] == prev { ns[1] } else { ns[0] };
            prev = cur;
            cur = nxt;
        }
        if !cyc.is_empty() {
            loops.push(cyc);
        }
    }
    eprintln!(
        "{tag}: would-sever: {} boundary edge(s) in {} simple loop(s), sizes {:?} — \
         cap arithmetic: chi'={chi1} + loops={} = {} vs 2*components={} ({})",
        boundary.len(),
        loops.len(),
        loops.iter().map(|l| l.len()).collect::<Vec<_>>(),
        loops.len(),
        chi1 + loops.len() as i64,
        2 * comps1 as i64,
        if chi1 + loops.len() as i64 == 2 * comps1 as i64 && nonsimple.is_empty() {
            "SHELL-CONSISTENT"
        } else {
            "MISMATCH"
        },
    );
    let mut loop_legend: Vec<Surface> = Vec::new();
    for (li, cyc) in loops.iter().enumerate().take(8) {
        let show = cyc.len().min(12);
        let mut line = String::new();
        for &v in cyc.iter().take(show) {
            let p = mesh.verts[v as usize].as_array();
            let cs = crate::stage4_correct::carried_surfaces(
                mesh,
                &attribution.attributions,
                a,
                b,
                v,
                p,
            );
            let refs: Vec<usize> = cs
                .iter()
                .map(|s| match loop_legend.iter().position(|x| x == s) {
                    Some(i) => i,
                    None => {
                        loop_legend.push(*s);
                        loop_legend.len() - 1
                    }
                })
                .collect();
            line.push_str(&format!(
                " v{v}{refs:?}({:.4},{:.4},{:.4})",
                p[0], p[1], p[2]
            ));
        }
        eprintln!(
            "{tag}:   loop {li} ({} verts{}):{line}",
            cyc.len(),
            if show < cyc.len() {
                format!(", first {show}")
            } else {
                String::new()
            },
        );
    }
    for (i, s) in loop_legend.iter().enumerate() {
        eprintln!("{tag}:   loop-surf {i}: {s:?}");
    }
    let _ = &frontier_tris;
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::{Point3, Vector3};

    #[test]
    fn cone_projection_lands_on_the_surface_and_picks_the_right_nappe() {
        let cone = Surface::Cone {
            apex: Point3::new(1.0, -2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 2.0),
            half_angle: 0.3,
        };
        // A point near the +nappe at station 5: radius there is 5·tan(0.3).
        let r5 = 5.0 * 0.3f64.tan();
        for (p, want_pos_nappe) in [
            ([1.0 + r5 + 0.7, -2.0, 8.0], true),
            ([1.0 + r5 - 0.2, -2.0, 8.0], true),
            ([1.0, -2.0 - r5 - 0.4, 3.0 - 5.0], false),
        ] {
            let q = project_to_surface(&cone, p).expect("off-axis projects");
            let sd = crate::geom::signed_distance_to_surface(cone, Point3::new(q[0], q[1], q[2]))
                .expect("cone sdist");
            assert!(sd.abs() < 1e-9, "projection residual {sd:.3e}");
            assert_eq!(q[2] > 3.0, want_pos_nappe, "nappe for {p:?}");
            // Nearest: the projected point is closer than the apex.
            let da = norm(sub(p, [1.0, -2.0, 3.0]));
            assert!(norm(sub(p, q)) <= da + 1e-12);
        }
        // On-axis: azimuth-free, declined.
        assert!(project_to_surface(&cone, [1.0, -2.0, 9.0]).is_none());
    }

    #[test]
    fn nearly_flat_cone_projection_stays_exact() {
        // The R0003 graze strips live on nearly-flat cones (half-angle
        // ≈ 1.55 rad, tan γ ≈ 49) — the regime where the census's depth
        // numbers are load-bearing. The projector must stay exact there.
        let cone = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 1.55,
        };
        // A point near radius 200 on the +nappe, displaced 1e-3 along
        // the surface normal-ish (the graze depth scale).
        let s = 200.0 / 1.55f64.tan();
        let p = [200.0 + 1e-3, 0.0, s + 1e-3];
        let q = project_to_surface(&cone, p).expect("off-axis projects");
        let sd = crate::geom::signed_distance_to_surface(cone, Point3::new(q[0], q[1], q[2]))
            .expect("cone sdist");
        assert!(sd.abs() < 1e-9, "flat-cone projection residual {sd:.3e}");
        assert!(norm(sub(q, p)) < 3e-3, "projection stays local");
    }

    #[test]
    fn cylinder_projection_is_radial() {
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        let q = project_to_surface(&cyl, [3.0, 0.0, 7.0]).expect("off-axis");
        assert!((q[0] - 2.0).abs() < 1e-12 && q[1].abs() < 1e-12 && (q[2] - 7.0).abs() < 1e-12);
        assert!(project_to_surface(&cyl, [0.0, 0.0, 1.0]).is_none());
    }

    #[test]
    fn coaxial_rim_and_circle_plane_roots_are_closed_form() {
        // Two coaxial cones on the z-axis meeting where their radii
        // agree: cone A apex at z=0 slope tan(0.4) opening +z; cone B
        // apex at z=10 slope tan(0.7) opening -z. tan(0.4)·s =
        // tan(0.7)·(10−s).
        let ca = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.4,
        };
        let cb = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 10.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.7,
        };
        let (c, u, r) = coaxial_cone_rim(&ca, &cb).expect("rim exists");
        let s = 0.7f64.tan() * 10.0 / (0.4f64.tan() + 0.7f64.tan());
        assert!((c[2] - s).abs() < 1e-12 && (r - 0.4f64.tan() * s).abs() < 1e-12);
        // Both surfaces contain the rim.
        for surf in [ca, cb] {
            let sd =
                crate::geom::signed_distance_to_surface(surf, Point3::new(c[0] + r, c[1], c[2]))
                    .expect("sdist");
            assert!(sd.abs() < 1e-12, "rim on surface: {sd:.3e}");
        }
        // A plane x = r/2 crosses the rim circle at two symmetric roots.
        let roots = circle_plane_roots(c, u, r, [1.0, 0.0, 0.0], -r / 2.0);
        assert_eq!(roots.len(), 2);
        let (e1, e2) = circle_frame(u).expect("frame");
        for t in roots {
            let p = add(add(c, scale3(r * t.cos(), e1)), scale3(r * t.sin(), e2));
            assert!((p[0] - r / 2.0).abs() < 1e-12);
        }
        // A plane past the circle misses.
        assert!(circle_plane_roots(c, u, r, [1.0, 0.0, 0.0], -2.0 * r).is_empty());
    }

    #[test]
    fn inside_depth_is_the_convex_membership_function() {
        // Unit box [0,1]^3 as six outward half-spaces.
        let planes: Vec<([f64; 3], f64)> = vec![
            ([1.0, 0.0, 0.0], -1.0),
            ([-1.0, 0.0, 0.0], 0.0),
            ([0.0, 1.0, 0.0], -1.0),
            ([0.0, -1.0, 0.0], 0.0),
            ([0.0, 0.0, 1.0], -1.0),
            ([0.0, 0.0, -1.0], 0.0),
        ];
        assert!(inside_depth(&planes, [0.5, 0.5, 0.5]) < 0.0);
        assert_eq!(inside_depth(&planes, [0.5, 0.5, 0.5]), -0.5);
        assert!(inside_depth(&planes, [1.2, 0.5, 0.5]) > 0.0);
        // Outside near an edge: still positive.
        assert!(inside_depth(&planes, [1.1, 1.1, 0.5]) > 0.0);
    }
}

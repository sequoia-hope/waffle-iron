//! P3a #146 conformal junction sampling — the pierce-point primitive
//! (increment 1a) and the Stage-1 override builder the increment-2 wiring
//! feeds into `boolean()` behind `YANG_JUNCTION_SAMPLING_ENABLE`
//! (spec `specs/yang_146_conformal_junction_sampling.md` §3.2/§4).
//!
//! Wiring scope (fail-closed — a missed mint is status quo, never
//! worse, spec §6):
//! - edges: `Curve::LineSegment` only, incident to two PLANAR faces (the
//!   F0082 lead-customer class; the chord IS the curve so the
//!   parameter-range test is exact, and the Stage-1 edge-override splice
//!   is planar-incident only);
//! - partner faces: `Surface::Plane` with ALL-LINE loops only — exact
//!   closed-form pierce (line ∩ plane) and exact 2D bounded-face
//!   containment (a chord polygon is the true region only when every loop
//!   edge is straight). Curved edges/partners are a later increment.
//!
//! Every gate is derived, not tuned:
//! - endpoint margin `TAU_MODEL·(1+scale)`: a pierce at/near an edge
//!   endpoint is a CORNER junction — P3b stitch territory, not a mid-edge
//!   sample (spec §6);
//! - boundary margin `TAU_MODEL·(1+scale)` inside the partner face: a
//!   pierce on the partner's own boundary edge is likewise a corner;
//! - transversality floor `1e-9` on `|t̂·n̂|` (the sine of the line/surface
//!   angle; same collinearity band as the backtrack-spike test): below it
//!   the contact is tangential and routes to the #137 path, never minted;
//! - on-surface postcondition `TAU_EVAL·(1+scale)` on both of the edge's
//!   incident surfaces (loud contract: a line edge lies ON both by
//!   construction, so a violation is a producer fault, not a near-miss).

use crate::*;
use std::collections::BTreeMap;

/// One transversal pierce point of a geometric edge through a bounded
/// partner face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PiercePoint {
    /// The exact junction point (on the edge's line and the partner plane;
    /// on both incident surfaces to within the `TAU_EVAL` band).
    pub point: Point3,
    /// Chord parameter along the edge, strictly inside `(0, 1)`.
    pub t: f64,
    /// `|t̂ · n̂_partner|` at the pierce — the transversality margin.
    pub transversality: f64,
    /// Index of the pierced face in the PARTNER operand — the face whose
    /// Stage-1 mesh must carry `point` as an interior Steiner vertex
    /// (increment 2, spec §3.3 second bullet).
    pub partner_face: u32,
    /// The owner edge's two incident planes + their pierce-time GEOMETRIC
    /// material verdicts — the Stage-4 beyond-corner trim's raw provenance
    /// (inc-4b). Resolved against the boolean OP in `boolean()`.
    pub owner_planes: [PierceTrimPlane; 2],
}

/// One owner plane of a pierce, in normalized Hesse form
/// (`signed_dist(p) = n·p + d`, `n` the unit OUTWARD face normal), plus the
/// pierce-time GEOMETRIC verdict `material_beyond`:
/// - `Some(true)`  — reflex incidence: the owner solid's material extends
///   past this plane at the pierce (beyond ⇒ INSIDE the owner);
/// - `Some(false)` — convex incidence: beyond ⇒ OUTSIDE the owner;
/// - `None`        — undetermined (near-coplanar faces, missing directed
///   copy, degenerate data) ⇒ the trim never fires on this plane.
///
/// Whether "beyond" means ZERO KEPT CONTENT depends on the boolean op and
/// the owner side — resolved by [`resolve_trim_beyond`] into the Stage-4
/// [`MintTrimPlane::trim_beyond`] flag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PierceTrimPlane {
    /// Unit outward normal.
    pub n: [f64; 3],
    /// Normalized plane offset: `n·p + d = 0` on the plane.
    pub d: f64,
    /// Geometric material verdict at the pierce (see type docs).
    pub material_beyond: Option<bool>,
}

impl Default for PierceTrimPlane {
    /// Degenerate placeholder: zero normal ⇒ every signed distance is 0 and
    /// the verdict is undetermined ⇒ the trim can never fire (fail closed).
    fn default() -> Self {
        PierceTrimPlane {
            n: [0.0; 3],
            d: 0.0,
            material_beyond: None,
        }
    }
}

/// One owner plane RESOLVED for the executing boolean op: `trim_beyond ==
/// true` ⇔ a section-curve sample beyond this plane — past the minted
/// corner — is interior to / removed from the RESULT, i.e. has zero kept
/// content and may be trimmed onto the mint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MintTrimPlane {
    /// Unit outward normal.
    pub n: [f64; 3],
    /// Normalized plane offset: `n·p + d = 0` on the plane.
    pub d: f64,
    /// Resolved zero-content verdict for the executing op.
    pub trim_beyond: bool,
}

impl Default for MintTrimPlane {
    /// Degenerate placeholder: zero normal ⇒ every signed distance is 0 ⇒
    /// the beyond-corner predicate can never fire on it (fail closed).
    fn default() -> Self {
        MintTrimPlane {
            n: [0.0; 3],
            d: 0.0,
            trim_beyond: false,
        }
    }
}

/// Stage-4 provenance of one Stage-1 minted junction point (keyed by the
/// mint's exact coordinate bits), with per-plane verdicts already RESOLVED
/// for the executing op.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MintProvenance {
    /// The owner edge's two incident planes at the pierce.
    pub owner_planes: [MintTrimPlane; 2],
}

/// Pierce-time provenance of one mint as recorded by
/// [`junction_stage1_overrides`]: the owning operand + the raw geometric
/// plane verdicts (op not yet applied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PierceProvenance {
    /// Which operand owns the pierced edge.
    pub owner: InputId,
    /// The owner edge's two incident planes at the pierce (geometric).
    pub owner_planes: [PierceTrimPlane; 2],
}

/// Resolve a plane's geometric `material_beyond` verdict into the
/// zero-content `trim_beyond` flag for the executing op (spec inc-4b,
/// measured 2026-07-19 on both live customers):
///
/// A trim candidate v lies ON the partner operand's boundary surface and ON
/// the owner's OTHER incident plane, beyond plane i past the minted corner.
/// Zero kept content ⇔ v's beyond-region is interior to / removed from the
/// result:
/// - `Union`: beyond must be INSIDE the owner (reflex, `material_beyond ==
///   true`) — interior of the union. Beyond a CONVEX plane is outside the
///   owner, where the partner's surface is KEPT (F0082's measured union
///   fires are reflex rising-wall corners).
/// - `Subtract` (result = A − B): owner A (base): beyond must be OUTSIDE
///   the base (convex) — outside the base is outside the result. Owner B
///   (tool): beyond must be INSIDE the tool (reflex) — carved away
///   (R0061's measured subtract fires: zigzag-tool reflex corners).
/// - `Intersect`: beyond must be OUTSIDE the owner (convex) — outside
///   either operand is outside the intersection.
/// - `Xor`: NEVER — both sides of every surface survive an XOR boundary.
/// - Undetermined geometry (`None`): never (fail closed).
pub(crate) fn resolve_trim_beyond(
    op: BoolOp,
    owner: InputId,
    material_beyond: Option<bool>,
) -> bool {
    match (op, material_beyond) {
        (_, None) => false,
        (BoolOp::Union, Some(mb)) => mb,
        (BoolOp::Subtract, Some(mb)) => {
            if owner == InputId::A {
                !mb
            } else {
                mb
            }
        }
        (BoolOp::Intersect, Some(mb)) => !mb,
        (BoolOp::Xor, Some(_)) => false,
    }
}

/// The inc-4b trim provenance of one owner edge: both incident planes in
/// normalized Hesse form + the per-plane local-convexity verdict at the
/// edge. Any gate failure (non-plane surface, degenerate normal, missing
/// directed copy, ambiguous/reflex incidence) yields the fail-closed
/// default (`trim_beyond == false` / zero normal) — the mint still happens;
/// only the Stage-4 trim stays inert for that plane.
fn owner_trim_planes(
    x: &BRep,
    s1: Surface,
    s2: Surface,
    copy_faces: &[(u32, Surface)],
) -> [PierceTrimPlane; 2] {
    let surfs = [s1, s2];
    let mut out = [PierceTrimPlane::default(); 2];
    for i in 0..2 {
        let Surface::Plane { normal, d } = surfs[i] else {
            return [PierceTrimPlane::default(); 2];
        };
        let ra = normal.as_array();
        let n_len = (ra[0] * ra[0] + ra[1] * ra[1] + ra[2] * ra[2]).sqrt();
        if n_len < cad_primitives::TAU_WORK {
            return [PierceTrimPlane::default(); 2];
        }
        out[i] = PierceTrimPlane {
            n: [ra[0] / n_len, ra[1] / n_len, ra[2] / n_len],
            d: d / n_len,
            material_beyond: None,
        };
    }
    for i in 0..2 {
        let j = 1 - i;
        // A directed copy of the edge as it appears in the loop of a face ON
        // plane j: under this B-Rep's loop convention ("CCW viewed from
        // outside ALONG the face normal", i.e. looking in the +n direction;
        // planar `Plane.normal` outward, `reversed == false`) face material
        // lies to the RIGHT of travel — direction t̂ⱼ × n̂ⱼ. Pinned
        // empirically by the `box_pierce_provenance_is_convex_and_on_plane`
        // fixture (an rj_box vertical edge: the y=0.3 face's copy runs +z
        // with n = −ŷ, and its material extends toward +x = t̂ × n̂).
        let Some(&(ei, _)) = copy_faces.iter().find(|(_, fs)| *fs == surfs[j]) else {
            continue;
        };
        let e = &x.edges()[ei as usize];
        let a0 = x.vertices()[e.start as usize].point.as_array();
        let a1 = x.vertices()[e.end as usize].point.as_array();
        let t = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
        let tl = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
        if tl == 0.0 {
            continue;
        }
        let th = [t[0] / tl, t[1] / tl, t[2] / tl];
        let nj = out[j].n;
        let u = [
            th[1] * nj[2] - th[2] * nj[1],
            th[2] * nj[0] - th[0] * nj[2],
            th[0] * nj[1] - th[1] * nj[0],
        ];
        // s < 0: convex — face j's material dips strictly INSIDE plane i's
        // negative half-space (beyond plane i = outside the solid).
        // s > 0: reflex — material extends past plane i.
        // |s| within the collinearity floor (near-coplanar incident faces):
        // undetermined — fail closed.
        let s = out[i].n[0] * u[0] + out[i].n[1] * u[1] + out[i].n[2] * u[2];
        out[i].material_beyond = if s > TRANSVERSALITY_MIN {
            Some(true)
        } else if s < -TRANSVERSALITY_MIN {
            Some(false)
        } else {
            None
        };
    }
    out
}

/// Sine-scale transversality floor on `|t̂·n̂|` — below this the edge is
/// tangential to the partner surface (the #137 route), never a mint.
const TRANSVERSALITY_MIN: f64 = 1e-9;

/// All transversal pierce points of each operand's geometric `LineSegment`
/// edges through the OTHER operand's bounded planar faces.
///
/// Keys are `(owning input, edge index)` — and because `LineSegment` edges
/// use the per-loop-copy convention (kernel-v2 `to_yang.rs` m1: one directed
/// yang edge per half-edge), the SAME pierce list is fanned out to EVERY
/// copy of the geometric edge, so both incident faces of the owner see the
/// identical insertion (the spec's conformality-by-identity requirement;
/// keying by a single copy index would silently break it).
pub(crate) fn junction_pierce_points(
    a: &BRep,
    b: &BRep,
) -> BTreeMap<(InputId, u32), Vec<PiercePoint>> {
    // One-off structure probe (read-only, `YANG_P3B_FACE_PROBE=<A|B>,<idx>`):
    // dump the face's surface and loop-edge composition (curve type, endpoint
    // coords) — diagnoses why a face's boundary edges never enter the pierce
    // enumeration.
    if let Ok(spec) = std::env::var("YANG_P3B_FACE_PROBE") {
        if let Some((which, idx)) = spec.split_once(',') {
            if let Ok(fi) = idx.parse::<usize>() {
                let brep = if which == "A" { a } else { b };
                if let Some(f) = brep.faces().get(fi) {
                    eprintln!(
                        "[p3b-face] {which} face {fi} surface={:?} outer={} inner={}",
                        f.surface,
                        f.outer_loop.len(),
                        f.inner_loops.len()
                    );
                    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                        let e = &brep.edges()[ei as usize];
                        eprintln!(
                            "[p3b-face]   edge {ei} {:?} {:?} -> {:?}",
                            e.curve,
                            brep.vertices()[e.start as usize].point,
                            brep.vertices()[e.end as usize].point
                        );
                    }
                }
            }
        }
    }
    let mut out: BTreeMap<(InputId, u32), Vec<PiercePoint>> = BTreeMap::new();
    for (x, y, input) in [(a, b, InputId::A), (b, a, InputId::B)] {
        // Group the per-loop LineSegment edge copies by canonical (unordered,
        // bitwise) endpoint pair; collect every copy index and the DISTINCT
        // incident surfaces.
        let kb = |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
        // Per geometric edge: the per-loop copy indices, the DISTINCT incident
        // surfaces, and each copy's OWN face surface (the copy is directed as
        // it appears in that face's loop — the inc-4b convexity input).
        type Group = (Vec<u32>, Vec<Surface>, Vec<(u32, Surface)>);
        let mut groups: BTreeMap<([u64; 3], [u64; 3]), Group> = BTreeMap::new();
        for f in x.faces() {
            for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let e = &x.edges()[ei as usize];
                if e.curve != Curve::LineSegment {
                    continue; // increment-1a scope: straight edges only
                }
                let k0 = kb(x.vertices()[e.start as usize].point);
                let k1 = kb(x.vertices()[e.end as usize].point);
                let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
                let g = groups.entry(key).or_default();
                if !g.0.contains(&ei) {
                    g.0.push(ei);
                    g.2.push((ei, f.surface));
                }
                if !g.1.contains(&f.surface) {
                    g.1.push(f.surface);
                }
            }
        }
        for (copies, surfs, copy_faces) in groups.values() {
            // Group-structure probe (read-only, `YANG_P3B_GROUP_PROBE=x,y,z,r`):
            // dump every geometric-edge group whose midpoint lies within `r` of
            // the given point — copies, distinct-surface count, endpoints.
            // Diagnoses corners whose owner edges never reach the pierce math
            // (e.g. non-conformal chained-output loops splitting the same
            // geometric edge differently per face → 1-surface groups).
            if let Ok(spec) = std::env::var("YANG_P3B_GROUP_PROBE") {
                let parts: Vec<f64> = spec.split(',').filter_map(|s| s.parse().ok()).collect();
                if let [px, py, pz, pr] = parts.as_slice() {
                    let e = &x.edges()[copies[0] as usize];
                    let a0 = x.vertices()[e.start as usize].point.as_array();
                    let a1 = x.vertices()[e.end as usize].point.as_array();
                    let mid = [
                        0.5 * (a0[0] + a1[0]),
                        0.5 * (a0[1] + a1[1]),
                        0.5 * (a0[2] + a1[2]),
                    ];
                    let d = ((mid[0] - px).powi(2) + (mid[1] - py).powi(2) + (mid[2] - pz).powi(2))
                        .sqrt();
                    if d <= *pr {
                        eprintln!(
                            "[p3b-group] {input:?} copies={copies:?} surfs={} \
                             p0=({:.7},{:.7},{:.7}) p1=({:.7},{:.7},{:.7}) surfaces={surfs:?}",
                            surfs.len(),
                            a0[0],
                            a0[1],
                            a0[2],
                            a1[0],
                            a1[1],
                            a1[2]
                        );
                    }
                }
            }
            let [s1, s2] = surfs.as_slice() else {
                continue; // border/defective incidence — not a 2-surface edge
            };
            let owner_planar =
                matches!(s1, Surface::Plane { .. }) && matches!(s2, Surface::Plane { .. });
            // P3b scope-sizing probe (read-only): enumerate curved-partner
            // pierce CANDIDATES this increment's planar scope skips — the
            // F0082 ellipse×wall corner class. Mints nothing.
            if std::env::var_os("YANG_P3B_PIERCE_PROBE").is_some() {
                let e = &x.edges()[copies[0] as usize];
                p3b_cylinder_pierce_probe(
                    input,
                    copies[0],
                    *s1,
                    *s2,
                    x.vertices()[e.start as usize].point,
                    x.vertices()[e.end as usize].point,
                    y,
                );
            }
            // Increment-2 wiring scope: the OWNER edge must be incident to two
            // PLANAR faces — the Stage-1 edge-override tessellator splices
            // line chains only into planar faces (the 1b fail-closed guard);
            // a curved-incident edge is a later increment (missed mint =
            // status quo, never worse).
            if !owner_planar {
                continue;
            }
            let e = &x.edges()[copies[0] as usize];
            let p0 = x.vertices()[e.start as usize].point;
            let p1 = x.vertices()[e.end as usize].point;
            // inc-4b: the trim provenance is per geometric edge — computed
            // once, stamped on every pierce of this edge.
            let owner_planes = owner_trim_planes(x, *s1, *s2, copy_faces);
            let mut pierces: Vec<PiercePoint> = Vec::new();
            for (f_idx, f) in y.faces().iter().enumerate() {
                if let Some(mut pp) =
                    line_edge_plane_face_pierce(p0, p1, *s1, *s2, f_idx as u32, f, y)
                {
                    pp.owner_planes = owner_planes;
                    pierces.push(pp);
                }
                // P3b inc-3 (spec `yang_169_p3b_curved_partner_pierce.md`
                // §5, gated `YANG_P3B_PIERCE_ENABLE`): canonical-tube
                // CYLINDER partners join the pierce scope — the F0082
                // ellipse×wall corner class. Gate-OFF this arm is dead and
                // the enumeration is byte-identical.
                if std::env::var_os("YANG_P3B_PIERCE_ENABLE").is_some() {
                    pierces.extend(
                        line_edge_cylinder_face_pierce(p0, p1, *s1, *s2, f_idx as u32, f, y)
                            .into_iter()
                            .map(|mut pp| {
                                pp.owner_planes = owner_planes;
                                pp
                            }),
                    );
                }
            }
            if pierces.is_empty() {
                continue;
            }
            pierces.sort_by(|u, v| u.t.total_cmp(&v.t));
            for &ei in copies {
                out.insert((input, ei), pierces.clone());
            }
        }
        // P3b inc-4d-3 (spec §7): FULL-CIRCLE RIM owners join the pierce
        // scope behind the same gate — the F0082 cap-rim×wall corner class
        // (J2). Gate-OFF this arm is dead and the enumeration is
        // byte-identical.
        if std::env::var_os("YANG_P3B_PIERCE_ENABLE").is_some() {
            // Group rim copies by undirected circle geometry + seam bits
            // (the conformality-by-identity fan-out, circle edition).
            type RimGroup = (Vec<u32>, Vec<Surface>);
            let mut rim_groups: BTreeMap<([u64; 3], u64, [u64; 3]), RimGroup> = BTreeMap::new();
            for f in x.faces() {
                for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                    let e = &x.edges()[ei as usize];
                    let Curve::Circle { center, radius, .. } = e.curve else {
                        continue;
                    };
                    if e.start != e.end {
                        continue; // arc rims: a later widening (fail closed)
                    }
                    let key = (
                        kb(center),
                        radius.to_bits(),
                        kb(x.vertices()[e.start as usize].point),
                    );
                    let g = rim_groups.entry(key).or_default();
                    if !g.0.contains(&ei) {
                        g.0.push(ei);
                    }
                    if !g.1.contains(&f.surface) {
                        g.1.push(f.surface);
                    }
                }
            }
            for (copies, surfs) in rim_groups.values() {
                // Canonical rim vocabulary: exactly two distinct incident
                // surfaces (the cap and the lateral) — anything else is a
                // border/defective incidence, fail closed.
                let [s1, s2] = surfs.as_slice() else {
                    continue;
                };
                let e = &x.edges()[copies[0] as usize];
                let Curve::Circle {
                    center,
                    normal,
                    radius,
                } = e.curve
                else {
                    unreachable!("grouped on circles above");
                };
                let seam = x.vertices()[e.start as usize].point;
                let mut pierces: Vec<PiercePoint> = Vec::new();
                for (f_idx, f) in y.faces().iter().enumerate() {
                    pierces.extend(circle_edge_plane_face_pierce(
                        center,
                        normal,
                        radius,
                        seam,
                        *s1,
                        *s2,
                        f_idx as u32,
                        f,
                        y,
                    ));
                }
                if pierces.is_empty() {
                    continue;
                }
                pierces.sort_by(|u, v| u.t.total_cmp(&v.t));
                for &ei in copies {
                    out.insert((input, ei), pierces.clone());
                }
            }
        }
    }
    out
}

/// P3b increment-0 probe (`YANG_P3B_PIERCE_PROBE`, read-only): transversal
/// pierce CANDIDATES of the segment `p0→p1` through the partner operand's
/// CYLINDER lateral faces — the class the planar wiring scope skips (the
/// F0082 ellipse×wall corner: an operand boundary edge piercing the other
/// operand's cylinder is exactly the never-minted section-curve terminus).
/// Line×cylinder is a quadratic in the chord parameter; both roots in (0,1)
/// are candidates. No containment test (scope sizing, not a mint) — the
/// printed height/azimuth let the reader judge against the face loops.
fn p3b_cylinder_pierce_probe(
    input: InputId,
    edge0: u32,
    s1: Surface,
    s2: Surface,
    p0: Point3,
    p1: Point3,
    y: &BRep,
) {
    let owner_planar = matches!(s1, Surface::Plane { .. }) && matches!(s2, Surface::Plane { .. });
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return;
    }
    for (f_idx, f) in y.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ah = normalize3(axis_dir.as_array());
        // Radial component of p(t)−ap: w(t) = w0 + t·wd with the axis
        // projection removed; |w(t)|² − r² = At² + Bt + C.
        let proj = |v: [f64; 3]| -> [f64; 3] {
            let along = v[0] * ah[0] + v[1] * ah[1] + v[2] * ah[2];
            [
                v[0] - along * ah[0],
                v[1] - along * ah[1],
                v[2] - along * ah[2],
            ]
        };
        let w0 = proj([a0[0] - ap[0], a0[1] - ap[1], a0[2] - ap[2]]);
        let wd = proj(dir);
        let qa = wd[0] * wd[0] + wd[1] * wd[1] + wd[2] * wd[2];
        let qb = 2.0 * (w0[0] * wd[0] + w0[1] * wd[1] + w0[2] * wd[2]);
        let qc = w0[0] * w0[0] + w0[1] * w0[1] + w0[2] * w0[2] - radius * radius;
        let disc = qb * qb - 4.0 * qa * qc;
        if qa == 0.0 || disc < 0.0 {
            continue; // line parallel to axis or missing the cylinder
        }
        let sq = disc.sqrt();
        for t in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let j = [a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]];
            // Radial (outward) unit normal at J and the line/surface angle.
            let wj = proj([j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]]);
            let n = normalize3(wj);
            let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
            let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
            let height = (j[0] - ap[0]) * ah[0] + (j[1] - ap[1]) * ah[1] + (j[2] - ap[2]) * ah[2];
            eprintln!(
                "[p3b-pierce] {input:?} edge {edge0} (owner_planar={owner_planar}) × cyl \
                 face {f_idx} (r={radius:.6}): t={t:.6} J=({:.9},{:.9},{:.9}) \
                 transv={transversality:.3} h={height:.6}",
                j[0], j[1], j[2]
            );
        }
        // Increment-1 arm: the production-shaped primitive's verdict on the
        // same face — which candidates survive EVERY mint gate (containment,
        // margins, postconditions). The pre-inc-3 measurement of what the
        // wiring would actually mint.
        for pp in line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx as u32, f, y) {
            eprintln!(
                "[p3b-pierce] MINT {input:?} edge {edge0} × cyl face {f_idx}: t={:.6} \
                 J=({:.9},{:.9},{:.9}) transv={:.3}",
                pp.t,
                pp.point.x(),
                pp.point.y(),
                pp.point.z(),
                pp.transversality
            );
        }
    }
}

/// P3b increment 1 (spec `yang_169_p3b_curved_partner_pierce.md` §3.1–3.2):
/// the transversal pierces of the segment `p0→p1` (whose two incident owner
/// surfaces are `s1`/`s2`) through the bounded CANONICAL-TUBE cylinder face
/// `f` of operand `y` — up to TWO per edge×face (both quadratic roots are
/// genuine crossings, unlike the planar arm's single root). UNWIRED this
/// increment: only the probe and the unit fixtures call it; wiring into
/// `junction_pierce_points` is increment 3, behind `YANG_P3B_PIERCE_ENABLE`.
///
/// Gates mirror `line_edge_plane_face_pierce` one-for-one — every margin is
/// the existing derived vocabulary, fail-closed (a missed mint = status quo):
/// - canonical-tube scope: hole-free face whose outer loop carries exactly
///   two FULL-circle rims (the `tessellate_lateral_face` tube vocabulary) —
///   axial containment is then EXACT via the rim planes. Strips / holed
///   patches are a later widening (spec §5 inc-4);
/// - roots in `[0,1]` with the endpoint margin `TAU_MODEL·(1+scale)` at both
///   owner endpoints (a near-endpoint pierce is a higher-order corner);
/// - transversality `|t̂·n̂(J)|` ≥ `TRANSVERSALITY_MIN` with the radial
///   normal at J (tangential grazes route to #137, never minted);
/// - on-surface postcondition `TAU_EVAL·(1+scale)` for `s1`/`s2` at J
///   (producer-fault guard, identical to the planar arm);
/// - axial containment `v_J ∈ (v_lo+margin, v_hi−margin)` from the rim-circle
///   centers projected on the axis (a rim-margin pierce is a rim corner —
///   P3b-later, fail closed).
pub(crate) fn line_edge_cylinder_face_pierce(
    p0: Point3,
    p1: Point3,
    s1: Surface,
    s2: Surface,
    f_idx: u32,
    f: &BRepFace,
    y: &BRep,
) -> Vec<PiercePoint> {
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    } = f.surface
    else {
        return Vec::new();
    };
    // Canonical-tube vocabulary gate (fail closed).
    if !f.inner_loops.is_empty() {
        return Vec::new();
    }
    let rims: Vec<&BRepEdge> = f
        .outer_loop
        .iter()
        .map(|&ei| &y.edges()[ei as usize])
        .filter(|e| matches!(e.curve, Curve::Circle { .. }) && e.start == e.end)
        .collect();
    let [rim0, rim1] = rims.as_slice() else {
        return Vec::new();
    };
    let ap = axis_point.as_array();
    let ah = normalize3(axis_dir.as_array());
    let axial = |p: Point3| -> f64 {
        let q = p.as_array();
        (q[0] - ap[0]) * ah[0] + (q[1] - ap[1]) * ah[1] + (q[2] - ap[2]) * ah[2]
    };
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for rim in [rim0, rim1] {
        let Curve::Circle { center, .. } = rim.curve else {
            unreachable!("filtered to circles above");
        };
        let v = axial(center);
        v_lo = v_lo.min(v);
        v_hi = v_hi.max(v);
    }
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return Vec::new();
    }
    let proj = |v: [f64; 3]| -> [f64; 3] {
        let along = v[0] * ah[0] + v[1] * ah[1] + v[2] * ah[2];
        [
            v[0] - along * ah[0],
            v[1] - along * ah[1],
            v[2] - along * ah[2],
        ]
    };
    let w0 = proj([a0[0] - ap[0], a0[1] - ap[1], a0[2] - ap[2]]);
    let wd = proj(dir);
    let qa = wd[0] * wd[0] + wd[1] * wd[1] + wd[2] * wd[2];
    let qb = 2.0 * (w0[0] * wd[0] + w0[1] * wd[1] + w0[2] * wd[2]);
    let qc = w0[0] * w0[0] + w0[1] * w0[1] + w0[2] * w0[2] - radius * radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if qa == 0.0 || disc < 0.0 {
        return Vec::new(); // parallel to the axis or missing the cylinder
    }
    let sq = disc.sqrt();
    let mut out = Vec::new();
    for t in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let j = [a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]];
        let scale = j
            .iter()
            .chain(a0.iter())
            .chain(a1.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
        // Endpoint margin: a pierce at/near an owner endpoint is a corner of
        // higher order (vertex-on-surface) — fail closed.
        let dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        if dist(j, a0) <= margin || dist(j, a1) <= margin {
            continue;
        }
        // Transversality via the radial normal at J.
        let wj = proj([j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]]);
        let n = normalize3(wj);
        let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
        let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
        if transversality < TRANSVERSALITY_MIN {
            continue; // tangential graze — the #137 route, never a mint
        }
        // On-surface postcondition for the owner's two incident surfaces.
        let band = cad_primitives::TAU_EVAL * (1.0 + scale);
        let on_owner = [s1, s2]
            .into_iter()
            .all(|s| surface_value_and_normal(s, j).is_some_and(|(fv, _)| fv.abs() <= band));
        if !on_owner {
            continue;
        }
        // Exact axial containment with the rim margin.
        let v_j = (j[0] - ap[0]) * ah[0] + (j[1] - ap[1]) * ah[1] + (j[2] - ap[2]) * ah[2];
        if v_j <= v_lo + margin || v_j >= v_hi - margin {
            continue; // outside the tube span, or a rim corner — fail closed
        }
        out.push(PiercePoint {
            point: Point3::new(j[0], j[1], j[2]),
            t,
            transversality,
            partner_face: f_idx,
            // Stamped by the caller (`junction_pierce_points`) — the trim
            // provenance is per owner EDGE, not per pierced face.
            owner_planes: [PierceTrimPlane::default(); 2],
        });
    }
    out.sort_by(|u, v| u.t.total_cmp(&v.t));
    out
}

/// ALL-LINE partner-loop gate shared by the planar-partner pierce arms: the
/// 2D containment below projects loop-edge START vertices, which is the
/// exact bounded region only when every loop edge is straight (an
/// arc-bounded face's chord polygon misjudges containment by up to the
/// sagitta near the arc). Curved-bounded partners are a later increment
/// (fail closed).
fn plane_face_all_line(f: &BRepFace, y: &BRep) -> bool {
    !f.outer_loop
        .iter()
        .chain(f.inner_loops.iter().flatten())
        .any(|&ei| y.edges()[ei as usize].curve != Curve::LineSegment)
}

/// Exact 2D bounded-face containment with a boundary margin, shared by the
/// planar-partner pierce arms: `ja` must lie strictly inside the outer
/// loop, strictly outside every hole, and ≥ `margin` from every boundary
/// segment (a pierce ON the partner's own boundary is a corner of higher
/// order — never a mid-face mint). `n` is the partner plane's UNIT normal
/// (the projection frame). Caller guarantees the ALL-LINE loop gate.
fn plane_face_contains_with_margin(
    f: &BRepFace,
    y: &BRep,
    n: [f64; 3],
    ja: [f64; 3],
    margin: f64,
) -> bool {
    let nv = Vector3::new(n[0], n[1], n[2]);
    let (u, v) = ortho_basis(nv);
    let (ua, va) = (u.as_array(), v.as_array());
    let project = |p: Point3| -> [f64; 2] {
        let q = p.as_array();
        [
            q[0] * ua[0] + q[1] * ua[1] + q[2] * ua[2],
            q[0] * va[0] + q[1] * va[1] + q[2] * va[2],
        ]
    };
    let j2 = [
        ja[0] * ua[0] + ja[1] * ua[1] + ja[2] * ua[2],
        ja[0] * va[0] + ja[1] * va[1] + ja[2] * va[2],
    ];
    let loop_pts = |lp: &[u32]| -> Vec<[f64; 2]> {
        lp.iter()
            .map(|&ei| {
                let e = &y.edges()[ei as usize];
                project(y.vertices()[e.start as usize].point)
            })
            .collect()
    };
    let outer = loop_pts(&f.outer_loop);
    if outer.len() < 3 || !point_in_polygon(j2, &outer) {
        return false;
    }
    if boundary_distance(j2, &outer) <= margin {
        return false;
    }
    for hole in &f.inner_loops {
        let hp = loop_pts(hole);
        if hp.len() >= 3 && point_in_polygon(j2, &hp) {
            return false;
        }
        if hp.len() >= 2 && boundary_distance(j2, &hp) <= margin {
            return false;
        }
    }
    true
}

/// P3b inc-4d (spec `yang_169_p3b_curved_partner_pierce.md` §7.3): the
/// transversal pierces of a FULL-CIRCLE rim edge (center/normal/radius,
/// seam = the rim's own B-Rep vertex; incident owner surfaces `s1`/`s2`)
/// through the bounded ALL-LINE planar face `f` of operand `y` — up to TWO
/// per rim×face (both circle∩plane roots are genuine crossings). UNWIRED
/// this sub-increment: only unit fixtures call it; wiring into
/// `junction_pierce_points` is inc-4d-3, behind `YANG_P3B_PIERCE_ENABLE`.
///
/// Gates mirror the line arms one-for-one — every margin is the existing
/// derived vocabulary, fail-closed (a missed mint = status quo):
/// - partner: `Surface::Plane` with ALL-LINE loops (exact 2D containment);
/// - roots of `n·p(θ) + d = 0` on `p(θ) = c + r(cosθ·u + sinθ·v)`:
///   `A·cosθ + B·sinθ = C`; `hypot(A,B) < |C|` ⇒ miss;
/// - near-tangency guard (A14.2, the Case-IV `circle_line_roots` rule):
///   the two roots closer than `TAU_MODEL·(1+scale)` in 3D are ONE
///   tangential contact, not two transversal crossings — no mint;
/// - seam-vertex margin `TAU_MODEL·(1+scale)` (a root at/near the rim's own
///   B-Rep vertex is a higher-order corner; also the ring builder's
///   seam-slot authority);
/// - transversality `|t̂(θ)·n̂|` with the circle tangent at the root, same
///   `TRANSVERSALITY_MIN` floor (tangential grazes route to #137);
/// - on-surface postcondition `TAU_EVAL·(1+scale)` for `s1`/`s2` at the
///   root (a rim lies ON both incident surfaces by construction — a
///   violation is a producer fault);
/// - 2D containment with the boundary margin (shared helper).
///
/// `PiercePoint.t` = seam-relative angle normalized to `[0,1)` (per-edge
/// sort key, mirroring the chord parameter of the line arms).
#[allow(clippy::too_many_arguments)]
pub(crate) fn circle_edge_plane_face_pierce(
    center: Point3,
    normal: Vector3,
    radius: f64,
    seam: Point3,
    s1: Surface,
    s2: Surface,
    f_idx: u32,
    f: &BRepFace,
    y: &BRep,
) -> Vec<PiercePoint> {
    let Surface::Plane { normal: pn, d } = f.surface else {
        return Vec::new();
    };
    if !plane_face_all_line(f, y) {
        return Vec::new();
    }
    let pn_raw = pn.as_array();
    let pn_len = (pn_raw[0] * pn_raw[0] + pn_raw[1] * pn_raw[1] + pn_raw[2] * pn_raw[2]).sqrt();
    if pn_len < cad_primitives::TAU_WORK {
        return Vec::new(); // degenerate partner plane — producer fault, skip
    }
    let n = [pn_raw[0] / pn_len, pn_raw[1] / pn_len, pn_raw[2] / pn_len];
    let d = d / pn_len;
    let ca = center.as_array();
    let nu_raw = normal.as_array();
    let nu_len = (nu_raw[0] * nu_raw[0] + nu_raw[1] * nu_raw[1] + nu_raw[2] * nu_raw[2]).sqrt();
    if nu_len < cad_primitives::TAU_WORK || radius <= 0.0 {
        return Vec::new(); // degenerate rim descriptor — producer fault, skip
    }
    let (u, v) = ortho_basis(Vector3::new(
        nu_raw[0] / nu_len,
        nu_raw[1] / nu_len,
        nu_raw[2] / nu_len,
    ));
    let (ua, va) = (u.as_array(), v.as_array());
    let p_at = |theta: f64| -> [f64; 3] {
        let (st, ct) = theta.sin_cos();
        [
            ca[0] + radius * (ct * ua[0] + st * va[0]),
            ca[1] + radius * (ct * ua[1] + st * va[1]),
            ca[2] + radius * (ct * ua[2] + st * va[2]),
        ]
    };
    // n·p(θ) + d = A·cosθ + B·sinθ + (n·c + d) = 0.
    let qa = radius * (n[0] * ua[0] + n[1] * ua[1] + n[2] * ua[2]);
    let qb = radius * (n[0] * va[0] + n[1] * va[1] + n[2] * va[2]);
    let qc = -(n[0] * ca[0] + n[1] * ca[1] + n[2] * ca[2] + d);
    let big_r = qa.hypot(qb);
    if big_r < qc.abs() || big_r == 0.0 {
        return Vec::new(); // circle misses the plane (or lies parallel)
    }
    let phi = qb.atan2(qa);
    let delta = (qc / big_r).clamp(-1.0, 1.0).acos();
    let roots = [phi + delta, phi - delta];
    let sa = seam.as_array();
    // Near-tangency guard: the pair of roots closer than the model band in
    // 3D is ONE tangential contact — never two transversal mints.
    {
        let (r0, r1) = (p_at(roots[0]), p_at(roots[1]));
        let scale = r0
            .iter()
            .chain(r1.iter())
            .chain(ca.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let band = cad_primitives::TAU_MODEL * (1.0 + scale);
        let d2 = (r0[0] - r1[0]).powi(2) + (r0[1] - r1[1]).powi(2) + (r0[2] - r1[2]).powi(2);
        if d2 < band * band {
            return Vec::new();
        }
    }
    // Seam-relative angle of a root (the sort key origin).
    let ws = [sa[0] - ca[0], sa[1] - ca[1], sa[2] - ca[2]];
    let phi0 = (ws[0] * va[0] + ws[1] * va[1] + ws[2] * va[2])
        .atan2(ws[0] * ua[0] + ws[1] * ua[1] + ws[2] * ua[2]);
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut out = Vec::new();
    for theta in roots {
        let j = p_at(theta);
        let scale = j
            .iter()
            .chain(sa.iter())
            .chain(ca.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
        // Seam-vertex margin: a pierce at/near the rim's own B-Rep vertex is
        // a corner of higher order — fail closed.
        let ds2 = (j[0] - sa[0]).powi(2) + (j[1] - sa[1]).powi(2) + (j[2] - sa[2]).powi(2);
        if ds2 <= margin * margin {
            continue;
        }
        // Transversality via the circle tangent at the root (unit by the
        // orthonormal frame).
        let (st, ct) = theta.sin_cos();
        let t_hat = [
            -st * ua[0] + ct * va[0],
            -st * ua[1] + ct * va[1],
            -st * ua[2] + ct * va[2],
        ];
        let transversality = (t_hat[0] * n[0] + t_hat[1] * n[1] + t_hat[2] * n[2]).abs();
        if transversality < TRANSVERSALITY_MIN {
            continue; // tangential graze — the #137 route, never a mint
        }
        // On-surface postcondition for the owner's two incident surfaces.
        let band = cad_primitives::TAU_EVAL * (1.0 + scale);
        let on_owner = [s1, s2]
            .into_iter()
            .all(|s| surface_value_and_normal(s, j).is_some_and(|(fv, _)| fv.abs() <= band));
        if !on_owner {
            continue;
        }
        if !plane_face_contains_with_margin(f, y, n, j, margin) {
            continue;
        }
        out.push(PiercePoint {
            point: Point3::new(j[0], j[1], j[2]),
            t: (theta - phi0).rem_euclid(two_pi) / two_pi,
            transversality,
            partner_face: f_idx,
            // Rim owners have a curved incident surface — the trim
            // provenance stays the fail-closed default (spec §7.3).
            owner_planes: [PierceTrimPlane::default(); 2],
        });
    }
    out.sort_by(|a, b| a.t.total_cmp(&b.t));
    out
}

/// The transversal pierce of the segment `p0→p1` (whose two incident
/// surfaces are `s1`/`s2`) through the bounded planar face `f` of operand
/// `y` — or `None` if any gate rejects (fail-closed).
fn line_edge_plane_face_pierce(
    p0: Point3,
    p1: Point3,
    s1: Surface,
    s2: Surface,
    f_idx: u32,
    f: &BRepFace,
    y: &BRep,
) -> Option<PiercePoint> {
    let Surface::Plane { normal, d } = f.surface else {
        return None; // increment-1a scope: planar partners only
    };
    // Increment-2 scope: ALL-LINE partner loops only (shared gate — see
    // `plane_face_all_line`). Curved-bounded partners fail closed.
    if !plane_face_all_line(f, y) {
        return None;
    }
    let n = normalize3(normal.as_array());
    // The plane's `d` is scaled to the RAW normal; renormalize it too.
    let n_len = {
        let r = normal.as_array();
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    if n_len < cad_primitives::TAU_WORK {
        return None; // degenerate plane descriptor — producer fault, skip
    }
    let d = d / n_len;
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return None;
    }
    let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
    let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
    if transversality < TRANSVERSALITY_MIN {
        return None; // tangential contact — the #137 route, never a mint
    }
    let denom = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
    let t = -(n[0] * a0[0] + n[1] * a0[1] + n[2] * a0[2] + d) / denom;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let j = Point3::new(a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]);
    let ja = j.as_array();
    let scale = ja
        .iter()
        .chain(a0.iter())
        .chain(a1.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
    // Endpoint margin: a pierce at/near an edge endpoint is a corner (P3b).
    let dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    if dist(ja, a0) <= margin || dist(ja, a1) <= margin {
        return None;
    }
    // On-surface postcondition: the line edge lies ON both incident surfaces
    // by construction, so J (on the line) must evaluate on-band for both — a
    // violation is a producer fault, and fail-closed means no mint.
    let band = cad_primitives::TAU_EVAL * (1.0 + scale);
    for s in [s1, s2] {
        let (fv, _) = surface_value_and_normal(s, ja)?;
        if fv.abs() > band {
            return None;
        }
    }
    // Exact 2D bounded-face containment with a boundary margin: J must lie
    // strictly inside the outer loop, strictly outside every hole, and
    // ≥ margin from every boundary segment (a pierce ON the partner's own
    // boundary is a corner — P3b). Shared with the rim arm.
    if !plane_face_contains_with_margin(f, y, n, ja, margin) {
        return None;
    }
    Some(PiercePoint {
        point: j,
        t,
        transversality,
        partner_face: f_idx,
        // Stamped by the caller (`junction_pierce_points`).
        owner_planes: [PierceTrimPlane::default(); 2],
    })
}

/// Increment-2 wiring payload (spec §4): the four per-operand Stage-1
/// override maps derived from [`junction_pierce_points`] — each operand's
/// OWNER-side edge insertions plus the PARTNER-side interior face points the
/// other operand's edges pierce through it. Identical exact bits on both
/// sides of every junction (one mint, shared by identity).
#[derive(Default, Debug)]
pub(crate) struct JunctionStage1Overrides {
    /// A's `LineSegment` edge index → junction points on that edge.
    pub edge_a: BTreeMap<u32, Vec<Point3>>,
    /// A's face index → interior junction points (pierced by B's edges).
    pub face_a: BTreeMap<u32, Vec<Point3>>,
    /// B's `LineSegment` edge index → junction points on that edge.
    pub edge_b: BTreeMap<u32, Vec<Point3>>,
    /// B's face index → interior junction points (pierced by A's edges).
    pub face_b: BTreeMap<u32, Vec<Point3>>,
    /// inc-4d: A's full-circle RIM edge index → junction points on that rim
    /// (the curved-owner half of a rim×planar-face pierce; consumed by the
    /// Stage-1 `rim_overrides` ring channel).
    pub rim_a: BTreeMap<u32, Vec<Point3>>,
    /// inc-4d: B's full-circle RIM edge index → junction points on that rim.
    pub rim_b: BTreeMap<u32, Vec<Point3>>,
    /// inc-4b: mint bit-key → pierce-time trim provenance (owner side +
    /// geometric plane verdicts), for every kept (non-poisoned) pierce
    /// point above. Resolved against the boolean op in `boolean()` and
    /// threaded to the Stage-4 beyond-corner trim alongside the §4.3
    /// moved×minted weld keys.
    pub provenance: BTreeMap<[u64; 3], PierceProvenance>,
}

impl JunctionStage1Overrides {
    pub(crate) fn is_empty(&self) -> bool {
        self.edge_a.is_empty()
            && self.face_a.is_empty()
            && self.edge_b.is_empty()
            && self.face_b.is_empty()
            && self.rim_a.is_empty()
            && self.rim_b.is_empty()
    }
}

/// inc-4d: the exact opposite-rim placement of rim-junction points — the
/// azimuth-merge lateral pairs its two rings 1:1, so every rim insertion
/// must be mirrored onto the opposite rim to keep the sample counts
/// matched (the production `collect_ring_crossings` rule, CYLINDER axial
/// projection: strip the axial component, renormalize the radial offset to
/// the opposite radius). Returns the opposite rim edge + one exact
/// ON-CIRCLE point per input point, or `None` when the rim has no
/// canonical cylinder-lateral pairing (torus profile rims, defective
/// loops) — the caller must then skip the mint entirely on BOTH sides
/// (fail closed: a rim entry without its mirror hits the loud
/// azimuth-merge count wall; a partner-only insert would be the one-sided
/// mint this machinery exists to prevent).
pub(crate) fn opposite_rim_projection(
    owner: &BRep,
    rim_edge: u32,
    pts: &[Point3],
) -> Option<(u32, Vec<Point3>)> {
    let crate::stage0::CapLateral::Cylinder((_, opp_edge, axis_point, axis_dir, _)) =
        crate::stage0::lateral_for_cap(owner, rim_edge).ok()?
    else {
        return None; // torus profile rims: out of inc-4d scope, fail closed
    };
    let Curve::Circle {
        center: opp_center,
        radius: opp_radius,
        ..
    } = owner.edges()[opp_edge as usize].curve
    else {
        return None;
    };
    let oc = opp_center.as_array();
    let mut out = Vec::with_capacity(pts.len());
    for pt in pts {
        let p = pt.as_array();
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let axial = w[0] * axis_dir[0] + w[1] * axis_dir[1] + w[2] * axis_dir[2];
        let radial = [
            w[0] - axial * axis_dir[0],
            w[1] - axial * axis_dir[1],
            w[2] - axial * axis_dir[2],
        ];
        let rlen = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
        if rlen < cad_primitives::TAU_WORK {
            return None; // on-axis rim point: degenerate geometry, fail closed
        }
        let s = opp_radius / rlen;
        out.push(Point3::new(
            oc[0] + radial[0] * s,
            oc[1] + radial[1] * s,
            oc[2] + radial[2] * s,
        ));
    }
    Some((opp_edge, out))
}

/// Build the Stage-1 override maps for both operands from the pierce
/// enumeration. Owner-side lists keep the per-(copy-)edge fan-out from
/// [`junction_pierce_points`]; partner-side face lists deduplicate the
/// per-copy repetition bitwise (each geometric edge contributes its pierce
/// point to the pierced face ONCE — same exact bits as the owner side).
///
/// Sub-weld-band CLUSTER filter (the F0016 lesson): two DISTINCT pierce
/// points closer than the §4.3 weld band `TAU_MODEL·(1+scale)` — e.g. from
/// near-duplicate chained edges piercing the same partner plane — would be
/// minted as separate sliver-spanning vertices that the downstream weld
/// fuses back into COINCIDENT triangles (the I6 non-manifold guard). Per
/// the junction contract, multiplicity below the resolution floor is NOT a
/// P3a mint: every point in such a cluster is DROPPED on both sides (fail
/// closed — a missed mint is status quo, never worse; the cluster itself is
/// P3b/upstream-twin territory). NO merged representative is minted: that
/// would be a tolerance merge (the R0091 hazard, spec §3.4).
pub(crate) fn junction_stage1_overrides(a: &BRep, b: &BRep) -> JunctionStage1Overrides {
    let pierce = junction_pierce_points(a, b);
    // Sub-weld-band cluster scan over ALL pierce points of the pair (both
    // directions — a mutual corner can put an A-side and a B-side mint in
    // one cluster). Bit-identical repeats are the SAME mint (kept); only
    // distinct-bits neighbours poison a cluster.
    let kb = |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
    let all: Vec<Point3> = {
        let mut v: Vec<Point3> = Vec::new();
        let mut seen: Vec<[u64; 3]> = Vec::new();
        for pps in pierce.values() {
            for pp in pps {
                let key = kb(pp.point);
                if !seen.contains(&key) {
                    seen.push(key);
                    v.push(pp.point);
                }
            }
        }
        v
    };
    let mut poisoned: Vec<[u64; 3]> = Vec::new();
    for (i, p) in all.iter().enumerate() {
        let pa = p.as_array();
        for q in all.iter().skip(i + 1) {
            let qa = q.as_array();
            let scale = pa
                .iter()
                .chain(qa.iter())
                .fold(0.0f64, |m, &c| m.max(c.abs()));
            let band = cad_primitives::TAU_MODEL * (1.0 + scale);
            let d2 = (pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2);
            if d2 < band * band {
                poisoned.push(kb(*p));
                poisoned.push(kb(*q));
            }
        }
    }
    if !poisoned.is_empty() && std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
        eprintln!(
            "[p3a-wire] sub-weld-band cluster: dropping {} of {} pierce points",
            poisoned.len(),
            all.len()
        );
    }
    let mut out = JunctionStage1Overrides::default();
    for ((input, ei), pps) in &pierce {
        let kept: Vec<&PiercePoint> = pps
            .iter()
            .filter(|pp| !poisoned.contains(&kb(pp.point)))
            .collect();
        if kept.is_empty() {
            continue;
        }
        // inc-4d: the owner-side channel is picked by the owner edge's
        // curve — full-circle RIM owners route to the Stage-1 rim-ring
        // override channel; `LineSegment` owners to the edge-polyline
        // channel (the pre-4d path, byte-identical).
        let owner = match input {
            InputId::A => a,
            InputId::B => b,
        };
        let rim_owner = matches!(owner.edges()[*ei as usize].curve, Curve::Circle { .. });
        let kept_pts: Vec<Point3> = kept.iter().map(|pp| pp.point).collect();
        // A rim mint is viable only with its opposite-rim mirror (the
        // azimuth-merge 1:1 ring pairing). No pairing ⇒ skip the mint on
        // BOTH sides — fail closed, status quo, never a one-sided insert.
        let rim_mirror = if rim_owner {
            match opposite_rim_projection(owner, *ei, &kept_pts) {
                Some(m) => Some(m),
                None => {
                    if std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
                        eprintln!(
                            "[p3a-wire] rim mint SKIP {input:?} edge {ei}: no canonical \
                             cylinder-lateral pairing (fail closed)"
                        );
                    }
                    continue;
                }
            }
        } else {
            None
        };
        let (edge_map, face_map) = match (input, rim_owner) {
            (InputId::A, false) => (&mut out.edge_a, &mut out.face_b),
            (InputId::B, false) => (&mut out.edge_b, &mut out.face_a),
            (InputId::A, true) => (&mut out.rim_a, &mut out.face_b),
            (InputId::B, true) => (&mut out.rim_b, &mut out.face_a),
        };
        let push_dedup = |entry: &mut Vec<Point3>, pts: Vec<Point3>| {
            for p in pts {
                let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
                if !entry
                    .iter()
                    .any(|q| [q.x().to_bits(), q.y().to_bits(), q.z().to_bits()] == key)
                {
                    entry.push(p);
                }
            }
        };
        if rim_owner {
            // Merge-with-dedup: this rim's entry may already hold ANOTHER
            // rim's mirror points (two rims can cross-mirror) — a plain
            // insert would clobber them.
            push_dedup(edge_map.entry(*ei).or_default(), kept_pts);
        } else {
            edge_map.insert(*ei, kept_pts);
        }
        if let Some((opp_edge, opp_pts)) = rim_mirror {
            // The mirror points are plain exact ring samples (NOT junction
            // mints — no partner-side insert, no trim provenance); bitwise
            // dedup against entries another rim's own mints already placed.
            push_dedup(edge_map.entry(opp_edge).or_default(), opp_pts);
        }
        for pp in &kept {
            let list = face_map.entry(pp.partner_face).or_default();
            let key = kb(pp.point);
            let dup = list.iter().any(|q| kb(*q) == key);
            if !dup {
                list.push(pp.point);
            }
            out.provenance.entry(key).or_insert(PierceProvenance {
                owner: *input,
                owner_planes: pp.owner_planes,
            });
        }
    }
    out
}

/// Even-odd point-in-polygon test (2D).
fn point_in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Minimum distance from `p` to any segment of the closed polyline `poly`.
fn boundary_distance(p: [f64; 2], poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
        };
        let (qx, qy) = (a[0] + t * dx, a[1] + t * dy);
        best = best.min(((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt());
    }
    best
}

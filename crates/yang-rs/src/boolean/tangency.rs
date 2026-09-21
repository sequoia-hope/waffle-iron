//! Yang §4.3.3 + §4.4.1 — exact surface-TANGENCY point insertion
//! (spec `specs/yang_433_tangent_point_mesh_update.md`).
//!
//! §4.3.3 makes tangent points first class: where a single surviving
//! intersection point has COLLINEAR surface normals, "it is a tangent point",
//! and §4.4.1 then trims and updates BOTH meshes with the intersection curves
//! so "the two polylines in the meshes coincide with the intersection curve"
//! (`refs/text/yang2025_hybrid_boolean.txt:518-570`).
//!
//! Our pipeline runs the exact arrangement first and relocates afterwards, so
//! a tangency the two TESSELLATIONS miss is unrecoverable downstream: C0058's
//! equal-radius cylinders touch at (0, ±0.4, 1) and their inscribed prisms do
//! not meet there at all — at Stage-4 entry no mesh vertex lies within 1e-9 of
//! the tangent point, the nearest four standing 1.334403e-1 / 1.868510e-1 away
//! in the KV9-F1 standoff quad. Relocating a vertex onto the tangent point
//! moves geometry but not CONNECTIVITY, so A's kept region stays edge-connected
//! across the (zero-width) band and Stage 6 sees a Newell-cancelling
//! figure-eight.
//!
//! This module supplies the missing half at the place the pipeline can still
//! act: mint the tangent point into BOTH operands' **Stage-1** meshes, through
//! the same face-interior override channel P3a #146 / P3b inc-2 already use, so
//! the two meshes share the point bit-exactly and the exact arrangement sees
//! them touch. Both inscribed surfaces then fall away from that shared apex in
//! the same normal direction with different second-order forms, which is what
//! produces the four alternating A,B,A,B sectors the exact geometry has.
//!
//! Scope (fail-closed — a missed mint is status quo, never worse):
//! - CYLINDER × CYLINDER only. Non-parallel axes touch at isolated POINTS
//!   ([`cyl_cyl_tangent_points`]); parallel axes touch along a whole
//!   GENERATOR ([`cyl_cyl_tangent_generator`]) — a line pinch whose output is
//!   the F0060 class (per-sheet edge duplication, `split_pinch_vertices`), and
//!   whose Stage-1 half is the SAME idea: give both prisms a RULING on the
//!   exact tangent line, with identical bits on all four rims, so the exact
//!   arrangement sees one shared segment instead of a sagitta-scale poke
//!   (C0056: B's 12-gon stood 4.9e-2 outside A's 13-gon at the tangency,
//!   and the crossing chords matched no candidate — `AmbiguousCurve {1, 0}`);
//! - both faces the CANONICAL TUBE vocabulary `line_edge_cylinder_face_pierce`
//!   already uses (hole-free, outer loop = exactly two full-circle rims), so
//!   axial containment is exact via the rim planes;
//! - EXACT tangency only: the admissibility identity below must hold within the
//!   `TAU_WORK·(1+scale)` ROUNDING band — never `TAU_MODEL`, which would fuse
//!   a real sub-resolution gap into a tangency (the R0053 lesson).

use crate::*;

use std::collections::BTreeMap;

/// The exact surface-tangency points of two cylinders.
///
/// A cylinder's unit normal at a surface point is the radial direction from its
/// axis, which is perpendicular to that axis. The two surfaces are tangent
/// where their normals are collinear, so the shared radial direction `m` is
/// perpendicular to BOTH axes:  `m = ±(û × v̂)/|û × v̂|` (hence the non-parallel
/// requirement). Write `f_A`, `f_B` for the feet of the axes' common
/// perpendicular, so `f_B − f_A = δ·m` with `δ = (b − a)·m`, and expand a
/// candidate `p` in the (independent) basis `{û, v̂, m}` anchored at `f_A`:
///
/// ```text
///   p − s_A·R_A·m ∈ L_A   ⟹   β = 0 and γ = s_A·R_A
///   p − s_B·R_B·m ∈ L_B   ⟹   α = 0 and γ − s_B·R_B = δ
/// ```
///
/// The axial components therefore vanish and the pair is tangent **iff**
///
/// ```text
///   s_A·R_A − s_B·R_B = δ        (s_A, s_B ∈ {−1, +1})
/// ```
///
/// in which case `p = f_A + s_A·R_A·m` — one point per admissible sign pair.
/// Flipping `m` maps `(s_A, s_B, δ) → (−s_A, −s_B, −δ)`, so fixing `m` and
/// scanning the four sign pairs enumerates every solution. Equal radii with
/// intersecting axes (`δ = 0`, `s_A = s_B = ±1`) give the TWO Steinmetz
/// tangency points; `δ = R_A − R_B` gives one.
///
/// `None` for parallel axes (`|û × v̂|` below the collinearity floor): those are
/// tangent along a generator, not at isolated points.
pub(crate) fn cyl_cyl_tangent_points(
    (ap1, ad1, r1): (Point3, Vector3, f64),
    (ap2, ad2, r2): (Point3, Vector3, f64),
) -> Option<Vec<Point3>> {
    let u = normalize3(ad1.as_array());
    let v = normalize3(ad2.as_array());
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    // The same collinearity floor the junction transversality gates use: below
    // it the axes are parallel and the contact, if any, is a generator.
    if n_len < 1e-9 {
        return None;
    }
    let m = [n[0] / n_len, n[1] / n_len, n[2] / n_len];
    let (a, b) = (ap1.as_array(), ap2.as_array());
    let w0 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let delta = w0[0] * m[0] + w0[1] * m[1] + w0[2] * m[2];
    // Foot of the common perpendicular on L_A (standard skew-line closest
    // point): t = ((w0 × v̂) · n) / |n|².
    let w0xv = [
        w0[1] * v[2] - w0[2] * v[1],
        w0[2] * v[0] - w0[0] * v[2],
        w0[0] * v[1] - w0[1] * v[0],
    ];
    let t = (w0xv[0] * n[0] + w0xv[1] * n[1] + w0xv[2] * n[2]) / (n_len * n_len);
    let f_a = [a[0] + t * u[0], a[1] + t * u[1], a[2] + t * u[2]];

    let scale = f_a
        .iter()
        .chain([r1, r2, delta].iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    // ROUNDING band (KV10 identity), never TAU_MODEL: this decides whether the
    // pair is EXACTLY tangent, and a model-scale band here would fuse a real
    // sub-resolution gap into a tangency (R0053).
    let band = cad_primitives::TAU_WORK * (1.0 + scale);

    let mut out: Vec<Point3> = Vec::new();
    let mut seen: Vec<[u64; 3]> = Vec::new();
    for (sa, sb) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        if (sa * r1 - sb * r2 - delta).abs() > band {
            continue;
        }
        let g = sa * r1;
        let p = [f_a[0] + g * m[0], f_a[1] + g * m[1], f_a[2] + g * m[2]];
        let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(Point3::new(p[0], p[1], p[2]));
    }
    Some(out)
}

/// The exact surface-tangency GENERATOR of two cylinders with PARALLEL axes.
///
/// With `û ∥ v̂` the shared normal `m` is the unit perpendicular from A's axis
/// to B's: `w⊥ = (b − a) − ((b − a)·û)û`, `m = w⊥/|w⊥|`, `δ = |w⊥|`. The same
/// admissibility identity as the point form then holds along the whole line:
/// `s_A·R_A − s_B·R_B = δ` — external contact `(+,−)` at `δ = R_A + R_B`,
/// internal contact `(+,+)` at `δ = R_A − R_B` (B inside A) or `(−,−)` at
/// `δ = R_B − R_A` (A inside B) — and the generator is `{a + s_A·R_A·m + t·û}`.
///
/// Returns `(p₀, û)` — A's radial foot on the line and A's unit axis — or
/// `None` for non-parallel axes (the point form's domain), a coaxial pair
/// (`δ` below the rounding band: coincident or nested surfaces, no generator
/// contact), or no admissible sign pair within `TAU_WORK·(1+scale)`.
#[cfg(test)]
pub(crate) fn cyl_cyl_tangent_generator(
    (ap1, ad1, r1): (Point3, Vector3, f64),
    (ap2, ad2, r2): (Point3, Vector3, f64),
) -> Option<(Point3, [f64; 3])> {
    cyl_cyl_tangent_generator_contact((ap1, ad1, r1), (ap2, ad2, r2)).map(|(p, u, _)| (p, u))
}

/// [`cyl_cyl_tangent_generator`] plus the contact KIND: `true` for EXTERNAL
/// contact (`(+,−)`, `δ = R_A + R_B` — the two solids touch from outside,
/// a union of two lobes pinched along the line), `false` for INTERNAL
/// contact (one tube inside the other).
pub(crate) fn cyl_cyl_tangent_generator_contact(
    (ap1, ad1, r1): (Point3, Vector3, f64),
    (ap2, ad2, r2): (Point3, Vector3, f64),
) -> Option<(Point3, [f64; 3], bool)> {
    let u = normalize3(ad1.as_array());
    let v = normalize3(ad2.as_array());
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    // The point form's own parallel floor, read the other way round.
    if n_len >= 1e-9 {
        return None;
    }
    let (a, b) = (ap1.as_array(), ap2.as_array());
    let w0 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let axial = w0[0] * u[0] + w0[1] * u[1] + w0[2] * u[2];
    let wp = [
        w0[0] - axial * u[0],
        w0[1] - axial * u[1],
        w0[2] - axial * u[2],
    ];
    let delta = (wp[0] * wp[0] + wp[1] * wp[1] + wp[2] * wp[2]).sqrt();
    let scale = a
        .iter()
        .chain(b.iter())
        .chain([r1, r2, delta].iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    // ROUNDING band (KV10 identity), never TAU_MODEL — see the point form.
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    if delta <= band {
        return None; // coaxial — no generator contact
    }
    let m = [wp[0] / delta, wp[1] / delta, wp[2] / delta];
    for (sa, sb) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        if (sa * r1 - sb * r2 - delta).abs() > band {
            continue;
        }
        let g = sa * r1;
        let p0 = Point3::new(a[0] + g * m[0], a[1] + g * m[1], a[2] + g * m[2]);
        return Some((p0, u, sa != sb));
    }
    None
}

/// The two rulings along which two PARALLEL-axis cylinders CROSS transversally
/// (spec `yang_433_tangent_point_mesh_update.md` §13 — R0038's configuration).
///
/// In the cross-section plane the two circles (radii `R_A`, `R_B`, centres
/// `δ = |w⊥|` apart along `m`) meet where
///
/// ```text
///   x = (R_A² − R_B² + δ²) / (2δ)      along m, from A's axis
///   y = ±√(R_A² − x²)                  along n = û × m
/// ```
///
/// which requires `|R_A − R_B| < δ < R_A + R_B` STRICTLY — outside the
/// rounding band on both ends, since the band itself is the tangent arm's
/// domain ([`cyl_cyl_tangent_generator_contact`]) and a δ inside it must not
/// be minted as two rulings a rounding apart. The crossing angle is the angle
/// between the radial directions `(x, y)/R_A` and `(x − δ, y)/R_B`; the
/// grazing class this serves has it at a few degrees.
///
/// Returns the two feet `a + x·m ± y·n` and A's unit axis, or `None` for
/// non-parallel axes, a coaxial pair, a tangent pair (the band), or disjoint /
/// nested circles.
pub(crate) fn cyl_cyl_crossing_generators(
    (ap1, ad1, r1): (Point3, Vector3, f64),
    (ap2, ad2, r2): (Point3, Vector3, f64),
) -> Option<([Point3; 2], [f64; 3])> {
    let u = normalize3(ad1.as_array());
    let v = normalize3(ad2.as_array());
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if n_len >= 1e-9 {
        return None;
    }
    let (a, b) = (ap1.as_array(), ap2.as_array());
    let w0 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let axial = w0[0] * u[0] + w0[1] * u[1] + w0[2] * u[2];
    let wp = [
        w0[0] - axial * u[0],
        w0[1] - axial * u[1],
        w0[2] - axial * u[2],
    ];
    let delta = (wp[0] * wp[0] + wp[1] * wp[1] + wp[2] * wp[2]).sqrt();
    let scale = a
        .iter()
        .chain(b.iter())
        .chain([r1, r2, delta].iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    // Strictly transversal: beyond the tangency band on BOTH ends.
    if delta <= (r1 - r2).abs() + band || delta >= r1 + r2 - band {
        return None;
    }
    let m = [wp[0] / delta, wp[1] / delta, wp[2] / delta];
    let nn = [
        u[1] * m[2] - u[2] * m[1],
        u[2] * m[0] - u[0] * m[2],
        u[0] * m[1] - u[1] * m[0],
    ];
    let x = (r1 * r1 - r2 * r2 + delta * delta) / (2.0 * delta);
    let y2 = r1 * r1 - x * x;
    if y2 <= 0.0 {
        return None;
    }
    let y = y2.sqrt();
    let foot = |s: f64| {
        Point3::new(
            a[0] + x * m[0] + s * y * nn[0],
            a[1] + x * m[1] + s * y * nn[1],
            a[2] + x * m[2] + s * y * nn[2],
        )
    };
    Some(([foot(1.0), foot(-1.0)], u))
}

/// A canonical TUBE: its axial span plus the two full-circle rim edges (index
/// and centre). `None` when the face is outside that vocabulary (holed, or an
/// outer loop that is not exactly two full-circle rims). Identical gate and
/// reading to [`line_edge_cylinder_face_pierce`], so the two arms agree on what
/// a bounded cylinder face is.
struct Tube {
    lo: f64,
    hi: f64,
    /// `(rim edge index, rim circle centre, rim SEAM vertex)` for each of the
    /// two rims. The seam vertex is the B-Rep vertex the full-circle rim edge
    /// starts and ends at — the ruling the tube grid already carries. For a
    /// SECTOR (§13 checkpoint 2) the "seam" is the arc's start vertex.
    rims: [(u32, Point3, Point3); 2],
    /// §13 checkpoint 2: a partial-revolve SECTOR lateral (`[Arc, Line, Arc,
    /// Line]`, the Stage-1 partial patch strip) carries its two ARC rims'
    /// angular extent, one gate per rim, so a ruling is minted only when it
    /// lies strictly inside BOTH arcs' sweeps. `None` for a canonical tube
    /// (full circles contain every azimuth).
    arcs: Option<[ArcGate; 2]>,
}

/// One arc rim's sweep, in the arc's own frame (`ortho_basis(normal)`):
/// the loop walks `start → end` counter-clockwise about `normal`.
#[derive(Clone, Copy)]
pub(crate) struct ArcGate {
    pub(crate) center: Point3,
    pub(crate) normal: [f64; 3],
    pub(crate) radius: f64,
    pub(crate) start: Point3,
    pub(crate) end: Point3,
}

impl ArcGate {
    /// Does the ruling through `sample` (a point on this rim's circle) lie
    /// STRICTLY inside the arc — its azimuth inside the CCW sweep from
    /// `start` to `end`, and the sample farther than `margin` from both
    /// endpoints? A ruling AT an endpoint is the sector's own boundary
    /// ruling: a corner of higher order (the line-edge pierce vehicle), never
    /// a mid-face mint. Fail-closed on any degenerate reading.
    pub(crate) fn contains(&self, sample: Point3, margin: f64) -> bool {
        let (e1v, e2v) = ortho_basis(Vector3::new(self.normal[0], self.normal[1], self.normal[2]));
        let (e1, e2) = (e1v.as_array(), e2v.as_array());
        let c = self.center.as_array();
        let angle = |p: Point3| -> f64 {
            let q = p.as_array();
            let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
            let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
            let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
            y.atan2(x)
        };
        let two_pi = 2.0 * std::f64::consts::PI;
        let phi0 = angle(self.start);
        let sweep = (angle(self.end) - phi0).rem_euclid(two_pi);
        let off = (angle(sample) - phi0).rem_euclid(two_pi);
        if !(sweep.is_finite() && off.is_finite()) || sweep <= 0.0 || self.radius <= 0.0 {
            return false;
        }
        // Angular margin from the chord margin, plus the endpoint distances
        // themselves (the rim build refuses a sample that coincides with an
        // endpoint but differs in bits; decline before it can).
        let ang_margin = margin / self.radius;
        if off <= ang_margin || off >= sweep - ang_margin {
            return false;
        }
        let far = |p: Point3| -> bool {
            let (a, b) = (sample.as_array(), p.as_array());
            let d2 = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
            d2 > margin * margin
        };
        far(self.start) && far(self.end)
    }
}

fn tube_axial_span(f: &BRepFace, y: &BRep) -> Option<Tube> {
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    } = f.surface
    else {
        return None;
    };
    if !f.inner_loops.is_empty() {
        return None;
    }
    let rims: Vec<(u32, &BRepEdge)> = f
        .outer_loop
        .iter()
        .map(|&ei| (ei, &y.edges()[ei as usize]))
        .filter(|(_, e)| matches!(e.curve, Curve::Circle { .. }) && e.start == e.end)
        .collect();
    // §13 checkpoint 2: the SECTOR vocabulary — exactly two ARC rims
    // (`start != end`) and two ruling LineSegments, four edges in all (the
    // Stage-1 partial patch strip's own dispatch pattern).
    let arcs: Vec<(u32, &BRepEdge)> = f
        .outer_loop
        .iter()
        .map(|&ei| (ei, &y.edges()[ei as usize]))
        .filter(|(_, e)| matches!(e.curve, Curve::Circle { .. }) && e.start != e.end)
        .collect();
    let lines = f
        .outer_loop
        .iter()
        .filter(|&&ei| matches!(y.edges()[ei as usize].curve, Curve::LineSegment))
        .count();
    // A canonical tube: exactly two full-circle rims (its seam rulings ride
    // along in the loop). A sector: exactly the four-edge pattern.
    let (rim_pair, is_sector): ([(u32, &BRepEdge); 2], bool) =
        match (rims.as_slice(), arcs.as_slice()) {
            ([r0, r1], []) => ([*r0, *r1], false),
            ([], [a0, a1]) if lines == 2 && f.outer_loop.len() == 4 => ([*a0, *a1], true),
            _ => return None,
        };
    let ap = axis_point.as_array();
    let ah = normalize3(axis_dir.as_array());
    let axial = |p: Point3| -> f64 {
        let q = p.as_array();
        (q[0] - ap[0]) * ah[0] + (q[1] - ap[1]) * ah[1] + (q[2] - ap[2]) * ah[2]
    };
    let circle = |e: &BRepEdge| -> (Point3, [f64; 3]) {
        let Curve::Circle { center, normal, .. } = e.curve else {
            unreachable!("filtered to circles above");
        };
        (center, normalize3(normal.as_array()))
    };
    let [(ei0, rim0), (ei1, rim1)] = rim_pair;
    let ((c0, n0), (c1, n1)) = (circle(rim0), circle(rim1));
    let (v0, v1) = (axial(c0), axial(c1));
    let vertex = |v: u32| y.vertices()[v as usize].point;
    let gate = |e: &BRepEdge, c: Point3, n: [f64; 3]| ArcGate {
        center: c,
        normal: n,
        radius,
        start: vertex(e.start),
        end: vertex(e.end),
    };
    Some(Tube {
        lo: v0.min(v1),
        hi: v0.max(v1),
        rims: [(ei0, c0, vertex(rim0.start)), (ei1, c1, vertex(rim1.start))],
        arcs: is_sector.then(|| [gate(rim0, c0, n0), gate(rim1, c1, n1)]),
    })
}

/// The four rim samples of a ruling `p₀ + h·û` (one per rim of each tube),
/// or `None` when either tube is a SECTOR whose arc does not strictly contain
/// the ruling — both operands get the ruling or neither (fail closed).
fn ruling_rim_samples(
    p0: Point3,
    u: [f64; 3],
    tubes: [&Tube; 2],
    margin: f64,
    probe: bool,
    label: &str,
) -> Option<[[(u32, Point3, Point3); 2]; 2]> {
    let pa = p0.as_array();
    let h_of = |c: Point3| -> f64 {
        let q = c.as_array();
        (q[0] - pa[0]) * u[0] + (q[1] - pa[1]) * u[1] + (q[2] - pa[2]) * u[2]
    };
    let mut out = [[(0u32, p0, p0); 2]; 2];
    for (t, tube) in tubes.iter().enumerate() {
        for (k, &(ei, centre, seam)) in tube.rims.iter().enumerate() {
            let h = h_of(centre);
            let sample = Point3::new(pa[0] + h * u[0], pa[1] + h * u[1], pa[2] + h * u[2]);
            if let Some(gates) = &tube.arcs {
                if !gates[k].contains(sample, margin) {
                    if probe {
                        eprintln!(
                            "[tangent-insert] {label} {pa:?} + t·{u:?} SKIP: outside the \
                             sector arc rim {ei} of tube {t}"
                        );
                    }
                    return None;
                }
            }
            out[t][k] = (ei, seam, sample);
        }
    }
    Some(out)
}

/// One operand's Stage-1 override payload for the tangency mint.
#[derive(Default)]
pub(crate) struct TangentOverrides {
    /// face index → the exact tangent points to mint as face interiors.
    pub face: BTreeMap<u32, Vec<Point3>>,
    /// rim edge index → the rim-circle samples that carry the tangency's
    /// azimuth, so the tube grid has a RULING through each minted point.
    pub rim: BTreeMap<u32, Vec<Point3>>,
    /// §13: the minimum full-circle rim segment count the CROSSING arm
    /// demands so the two polygons cross ONCE at the minted ruling (chord
    /// steps below twice the crossing angle, `N ≥ π/α`); the max over every
    /// minted crossing. `None` when nothing demanded.
    pub min_rim_n: Option<usize>,
}

/// The largest rim density the crossing arm will demand. Past it the
/// crossing is so grazing that the mint declines (status quo: the loud
/// Stage-4 STOP) rather than tessellate every rim of both solids at
/// thousands of segments — a mesh blow-up is not a fix.
const CROSSING_RIM_N_CEILING: usize = 512;

/// Bit-exact dedup push into an override channel.
fn push_unique(m: &mut BTreeMap<u32, Vec<Point3>>, k: u32, q: Point3) {
    let e = m.entry(k).or_default();
    let qa = q.as_array();
    let key = [qa[0].to_bits(), qa[1].to_bits(), qa[2].to_bits()];
    if !e
        .iter()
        .any(|r| [r.x().to_bits(), r.y().to_bits(), r.z().to_bits()] == key)
    {
        e.push(q);
    }
}

/// Push a rim-ring sample unless its azimuth IS the rim's seam ruling. The
/// tube grid already carries that ruling, and pushing a re-derived copy that
/// differs from the authoritative B-Rep vertex in the last bits is refused
/// loudly by the rim build ("coincides with the seam vertex but differs in
/// bits"). Skip it — fail closed, the ruling is there. Returns whether it
/// pushed.
fn push_rim_unless_seam(
    out: &mut TangentOverrides,
    ei: u32,
    seam: Point3,
    sample: Point3,
    probe: bool,
) -> bool {
    let (sa, sv) = (sample.as_array(), seam.as_array());
    let sc = sa
        .iter()
        .chain(sv.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let band = cad_primitives::TAU_MODEL * (1.0 + sc);
    let d2 = (sa[0] - sv[0]).powi(2) + (sa[1] - sv[1]).powi(2) + (sa[2] - sv[2]).powi(2);
    if d2 < band * band {
        if probe {
            eprintln!(
                "[tangent-insert] rim {ei} SKIP {sa:?}: the tangency azimuth IS the seam ruling"
            );
        }
        return false;
    }
    push_unique(&mut out.rim, ei, sample);
    true
}

/// Is `u` EXACTLY a signed coordinate axis? Then `p₀ + h·û` keeps two
/// coordinates bit-identical for every `h`, so the four rim samples of a
/// generator mint are exactly collinear and the exact arrangement sees ONE
/// shared segment. Any other axis would give four independently rounded
/// points that are collinear only to ~1 ulp — two skew femto-segments to an
/// exact predicate — so the generator arm declines there (status quo).
fn is_exact_coordinate_axis(u: [f64; 3]) -> bool {
    let ones = u.iter().filter(|c| c.abs() == 1.0).count();
    let zeros = u.iter().filter(|c| **c == 0.0).count();
    ones == 1 && zeros == 2
}

/// Yang §4.3.3/§4.4.1: the Stage-1 overrides that mint every exact
/// cylinder×cylinder surface-tangency point into BOTH operands.
///
/// Returns one payload per operand in the [`JunctionStage1Overrides`] shape —
/// the SAME channels P3a/P3b already feed, so the point enters both
/// tessellations with identical bits and the arrangement sees one shared vertex.
///
/// **Both channels are needed.** The face interior carries the point itself; the
/// rim samples carry its AZIMUTH onto the tube's two rim rings, so the Stage-1
/// grid has a full ruling through it and the interior splice lands ON that
/// ruling (a conforming 2+2 edge split) instead of fanning a mid-quad Steiner
/// point into three slivers. Measured: with the face channel alone, an operand
/// whose seam phase puts the tangency mid-quad (the 30°/`cylinder_brep`
/// fixtures, azimuth 3.5 steps off the seam) produced an arrangement edge
/// 1.3e-1 from BOTH exact branches — on neither, attributable to neither — and
/// Stage 3 refused it loudly (`AmbiguousCurve { candidates: 2, matched: 2 }`,
/// both matching only because `cyl_cyl_point_amplification` is unbounded at
/// tangency grade). With the ruling, the mint is conforming on every seam phase.
///
/// Per-pair gates, all fail-closed:
/// 1. both faces canonical tubes ([`tube_axial_span`]);
/// 2. exact tangency within the rounding band ([`cyl_cyl_tangent_points`]);
/// 3. on-surface postcondition `TAU_EVAL·(1+scale)` against BOTH cylinders (a
///    violation is a producer fault in the closed form, not a near-miss);
/// 4. axial containment strictly inside BOTH tubes with the rim margin
///    `TAU_MODEL·(1+scale)` — a tangency AT a rim is a corner of higher order
///    (the rim-junction vehicle), never a mid-face mint.
pub(crate) fn tangent_point_face_overrides(
    a: &BRep,
    b: &BRep,
) -> (TangentOverrides, TangentOverrides) {
    tangent_overrides(a, b, false)
}

/// `(rim samples for A, rim samples for B, the §13 minimum rim segment
/// count both operands must be rebuilt at)`.
pub(crate) type GeneratorRimOverrides = (
    BTreeMap<u32, Vec<Point3>>,
    BTreeMap<u32, Vec<Point3>>,
    Option<usize>,
);

/// The GENERATOR arm alone, as plain rim-sample maps — the Stage-0 path's
/// entry (spec `yang_433_tangent_point_mesh_update.md` §12). Stage 0 builds
/// its meshes through the rim-override channel only (its cap overlays are
/// replaced wholesale, there is no face-interior channel), and a generator
/// mint IS rim samples only: a line needs no interior point. The point arm
/// stays out — its face-interior half has no Stage-0 carrier, and a
/// rim-only point mint is the mid-quad Steiner fan the doc above measured.
///
/// Same per-pair gates as [`mint_generator`]; a pair outside them yields
/// nothing (status quo, never worse).
pub(crate) fn tangent_generator_rim_overrides(a: &BRep, b: &BRep) -> GeneratorRimOverrides {
    let (oa, ob) = tangent_overrides(a, b, true);
    debug_assert!(oa.face.is_empty() && ob.face.is_empty());
    let min_n = match (oa.min_rim_n, ob.min_rim_n) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    (oa.rim, ob.rim, min_n)
}

fn tangent_overrides(
    a: &BRep,
    b: &BRep,
    generator_only: bool,
) -> (TangentOverrides, TangentOverrides) {
    let mut out_a = TangentOverrides::default();
    let mut out_b = TangentOverrides::default();
    let probe = std::env::var_os("YANG_TANGENT_INSERT_PROBE").is_some();

    for (fa_idx, fa) in a.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point: apa,
            axis_dir: ada,
            radius: ra,
        } = fa.surface
        else {
            continue;
        };
        let Some(tube_a) = tube_axial_span(fa, a) else {
            continue;
        };
        for (fb_idx, fb) in b.faces().iter().enumerate() {
            let Surface::Cylinder {
                axis_point: apb,
                axis_dir: adb,
                radius: rb,
            } = fb.surface
            else {
                continue;
            };
            let Some(tube_b) = tube_axial_span(fb, b) else {
                continue;
            };
            let Some(pts) = cyl_cyl_tangent_points((apa, ada, ra), (apb, adb, rb)) else {
                // Parallel axes: a GENERATOR tangency, if any. Same channel,
                // rim samples only — a line needs no face-interior point.
                mint_generator(
                    (fa_idx as u32, &tube_a, (apa, ada, ra), &mut out_a),
                    (fb_idx as u32, &tube_b, (apb, adb, rb), &mut out_b),
                    generator_only,
                    probe,
                );
                // …or the two CROSSING rulings of a transversal pair (§13):
                // the tangent and crossing forms are disjoint in δ, so at
                // most one of the two arms mints.
                mint_crossing_rulings(
                    (fa_idx as u32, &tube_a, (apa, ada, ra), &mut out_a),
                    (fb_idx as u32, &tube_b, (apb, adb, rb), &mut out_b),
                    probe,
                );
                continue;
            };
            if generator_only {
                continue;
            }
            for p in pts {
                let pa = p.as_array();
                let scale = pa.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
                // (3) On-surface postcondition against both cylinders.
                let on = [
                    (apa.as_array(), normalize3(ada.as_array()), ra),
                    (apb.as_array(), normalize3(adb.as_array()), rb),
                ]
                .into_iter()
                .all(|(ap, ah, r)| {
                    let w = [pa[0] - ap[0], pa[1] - ap[1], pa[2] - ap[2]];
                    let h = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
                    let rad = [w[0] - h * ah[0], w[1] - h * ah[1], w[2] - h * ah[2]];
                    let len = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
                    (len - r).abs() <= cad_primitives::TAU_EVAL * (1.0 + scale)
                });
                if !on {
                    if probe {
                        eprintln!(
                            "[tangent-insert] A#{fa_idx} B#{fb_idx} {pa:?} REJECT off-surface"
                        );
                    }
                    continue;
                }
                // (4) Axial containment strictly inside both tubes.
                let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
                let axial = |ap: [f64; 3], ah: [f64; 3]| -> f64 {
                    (pa[0] - ap[0]) * ah[0] + (pa[1] - ap[1]) * ah[1] + (pa[2] - ap[2]) * ah[2]
                };
                let va = axial(apa.as_array(), normalize3(ada.as_array()));
                let vb = axial(apb.as_array(), normalize3(adb.as_array()));
                if va <= tube_a.lo + margin
                    || va >= tube_a.hi - margin
                    || vb <= tube_b.lo + margin
                    || vb >= tube_b.hi - margin
                {
                    if probe {
                        eprintln!(
                            "[tangent-insert] A#{fa_idx} B#{fb_idx} {pa:?} REJECT outside span \
                             (va={va:.6} in [{:.6},{:.6}], vb={vb:.6} in [{:.6},{:.6}])",
                            tube_a.lo, tube_a.hi, tube_b.lo, tube_b.hi
                        );
                    }
                    continue;
                }
                if probe {
                    eprintln!("[tangent-insert] A#{fa_idx} B#{fb_idx} MINT {pa:?}");
                }
                // The point itself, as a face interior…
                push_unique(&mut out_a.face, fa_idx as u32, p);
                push_unique(&mut out_b.face, fb_idx as u32, p);
                // …and its AZIMUTH on each tube's two rims, so the Stage-1 grid
                // carries a ruling through it. The tangency's radial direction
                // from an axis is exactly the shared normal `m` (that is what
                // makes it a tangency), so the rim sample is `centre + R·m̂`
                // with `m̂` read off the point — exact, no re-derivation.
                let rim_sample = |centre: Point3, ap: [f64; 3], ah: [f64; 3], r: f64| -> Point3 {
                    let w = [pa[0] - ap[0], pa[1] - ap[1], pa[2] - ap[2]];
                    let h = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
                    let rad = [w[0] - h * ah[0], w[1] - h * ah[1], w[2] - h * ah[2]];
                    let len = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
                    let c = centre.as_array();
                    Point3::new(
                        c[0] + r * rad[0] / len,
                        c[1] + r * rad[1] / len,
                        c[2] + r * rad[2] / len,
                    )
                };
                for (tube, out, ap, ah, r) in [
                    (
                        &tube_a,
                        &mut out_a,
                        apa.as_array(),
                        normalize3(ada.as_array()),
                        ra,
                    ),
                    (
                        &tube_b,
                        &mut out_b,
                        apb.as_array(),
                        normalize3(adb.as_array()),
                        rb,
                    ),
                ] {
                    for &(ei, centre, seam) in &tube.rims {
                        let sample = rim_sample(centre, ap, ah, r);
                        push_rim_unless_seam(out, ei, seam, sample, probe);
                    }
                }
            }
        }
    }
    (out_a, out_b)
}

/// The generator arm of [`tangent_point_face_overrides`]: two canonical tubes
/// with parallel axes tangent along a line get that line as a RULING of both
/// Stage-1 grids — one exact point `p₀` on the line, and each of the four rim
/// samples `p₀ + h·û` at its rim's own axial height, so the two prisms share
/// the tangent segment bit-exactly and B's facets adjacent to it fall INSIDE
/// A's (or outside, for external contact) instead of poking through.
///
/// Per-pair gates, all fail-closed:
/// 1. both faces canonical tubes (the caller's [`tube_axial_span`]);
/// 2. exact generator tangency within the rounding band
///    ([`cyl_cyl_tangent_generator`]);
/// 3. the axis is EXACTLY a coordinate axis ([`is_exact_coordinate_axis`]) —
///    the only frame in which four rounded samples are exactly collinear;
/// 4. the two tubes' axial spans OVERLAP by more than the rim margin
///    `TAU_MODEL·(1+scale)` — spans that merely touch meet at a rim×rim
///    circle tangency, a corner of higher order (the rim-junction vehicle);
/// 5. on-surface postcondition `TAU_EVAL·(1+scale)` of `p₀` against BOTH
///    cylinders (a violation is a producer fault in the closed form).
///
/// A sample whose azimuth IS a rim's seam is skipped (the ruling exists;
/// [`push_rim_unless_seam`]); one landing on a uniform Steiner slot is
/// MERGED by the rim build with the sample's exact bits (task #143), which is
/// what makes a tube whose own grid already carries the azimuth — C0056's B,
/// whose two rims disagreed in the last bits at that ruling — exact too.
///
/// `internal_only` (the Stage-0 path, §12): decline EXTERNAL contact. Two
/// solids touching from outside along a line union into two lobes pinched
/// along it — an output only the pinch-edge family can emit (C0042,
/// measured: with the ruling minted Stage 5 hands kernel-v2 one shell whose
/// contact line is a 4-valent edge, `InvalidBooleanOutput`; without it the
/// tessellations never meet and the regularized two-lobe union is CORRECT).
/// Stage 0 has an emission for INTERNAL contact only (the touching-disc
/// containment), so that is the boost's scope; the idle-Stage-0 route keeps
/// §11's full arm.
#[allow(clippy::type_complexity)]
fn mint_generator(
    (fa_idx, tube_a, cyl_a, out_a): (u32, &Tube, (Point3, Vector3, f64), &mut TangentOverrides),
    (fb_idx, tube_b, cyl_b, out_b): (u32, &Tube, (Point3, Vector3, f64), &mut TangentOverrides),
    internal_only: bool,
    probe: bool,
) {
    let Some((p0, u, external)) = cyl_cyl_tangent_generator_contact(cyl_a, cyl_b) else {
        return;
    };
    let pa = p0.as_array();
    if internal_only && external {
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} generator {pa:?} + t·{u:?} SKIP: \
                 external contact on the Stage-0 path (pinch-edge family)"
            );
        }
        return;
    }
    let scale = pa.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
    // (3) Exact-collinearity frame.
    if !is_exact_coordinate_axis(u) {
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} generator {pa:?} + t·{u:?} SKIP: \
                 the axis is not a coordinate axis (rim samples would not be exactly collinear)"
            );
        }
        return;
    }
    // (4) Axial overlap of the two tubes along û, measured from p₀.
    let h_of = |c: Point3| -> f64 {
        let q = c.as_array();
        (q[0] - pa[0]) * u[0] + (q[1] - pa[1]) * u[1] + (q[2] - pa[2]) * u[2]
    };
    let span = |t: &Tube| -> (f64, f64) {
        let (h0, h1) = (h_of(t.rims[0].1), h_of(t.rims[1].1));
        (h0.min(h1), h0.max(h1))
    };
    let (la, ha) = span(tube_a);
    let (lb, hb) = span(tube_b);
    let (lo, hi) = (la.max(lb), ha.min(hb));
    let margin = cad_primitives::TAU_MODEL * (1.0 + scale.max(hi.abs()).max(lo.abs()));
    if hi - lo <= margin {
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} generator {pa:?} + t·{u:?} SKIP: \
                 axial spans do not overlap (A [{la:.6},{ha:.6}] B [{lb:.6},{hb:.6}])"
            );
        }
        return;
    }
    // (5) On-surface postcondition against both cylinders.
    let on = [cyl_a, cyl_b].into_iter().all(|(ap, ad, r)| {
        let (ap, ah) = (ap.as_array(), normalize3(ad.as_array()));
        let w = [pa[0] - ap[0], pa[1] - ap[1], pa[2] - ap[2]];
        let h = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
        let rad = [w[0] - h * ah[0], w[1] - h * ah[1], w[2] - h * ah[2]];
        let len = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
        (len - r).abs() <= cad_primitives::TAU_EVAL * (1.0 + scale)
    });
    if !on {
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} generator {pa:?} + t·{u:?} REJECT \
                 off-surface"
            );
        }
        return;
    }
    let Some(samples) = ruling_rim_samples(
        p0,
        u,
        [tube_a, tube_b],
        margin,
        probe,
        &format!("A#{fa_idx} B#{fb_idx} generator"),
    ) else {
        return;
    };
    if probe {
        eprintln!(
            "[tangent-insert] A#{fa_idx} B#{fb_idx} MINT generator {pa:?} + t·{u:?} \
             over h ∈ [{lo:.6},{hi:.6}]"
        );
    }
    for (rims, out) in samples.iter().zip([out_a, out_b]) {
        for &(ei, seam, sample) in rims {
            push_rim_unless_seam(out, ei, seam, sample, probe);
        }
    }
}

/// The CROSSING arm of the generator mint (spec §13): two canonical tubes
/// with parallel axes whose cross-section circles cross transversally get
/// BOTH crossing rulings as rulings of both Stage-1 grids, exactly as
/// [`mint_generator`] gives a tangent pair its one contact line.
///
/// Why a crossing needs the mint at all: with parallel axes every facet-pair
/// intersection of the two prisms is an axis-parallel line, so the exact
/// arrangement's answer for one ruling is however many times the two
/// cross-section POLYGONS cross near it — and at a grazing angle (R0038:
/// 2.8°; the surfaces separate as `sin α · s` while the chords sag as
/// `s(L − s)/2R`) they cross several times. Stage 3 matches every one of
/// those parallel chords to the single exact ruling and Stage 4 relocates
/// them all onto it, collapsing the strips between them into zero-area
/// collinear chains (`degenerate_no_longedge`, `LocalRefinementRequired`)
/// or an inconsistent cap boundary (the Stage-6 walk dead-end of
/// `cyl_cyl_grazing_ruling_kv2`). With the ruling minted the two polygons
/// SHARE the crossing vertex and cross exactly once there — the paper's
/// "the two polylines in the meshes coincide with the intersection curve".
///
/// Same per-pair gates as [`mint_generator`] (canonical tubes; an exact
/// coordinate axis; axial overlap beyond the rim margin; on-surface
/// postcondition of each foot against both cylinders), each fail-closed to
/// the status quo. Both rulings lie on both tubes' full circles, so there is
/// no angular containment to check in this vocabulary.
#[allow(clippy::type_complexity)]
fn mint_crossing_rulings(
    (fa_idx, tube_a, cyl_a, out_a): (u32, &Tube, (Point3, Vector3, f64), &mut TangentOverrides),
    (fb_idx, tube_b, cyl_b, out_b): (u32, &Tube, (Point3, Vector3, f64), &mut TangentOverrides),
    probe: bool,
) {
    let Some((feet, u)) = cyl_cyl_crossing_generators(cyl_a, cyl_b) else {
        return;
    };
    if !is_exact_coordinate_axis(u) {
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} crossing rulings + t·{u:?} SKIP: \
                 the axis is not a coordinate axis (rim samples would not be exactly collinear)"
            );
        }
        return;
    }
    for p0 in feet {
        let pa = p0.as_array();
        let scale = pa.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
        // The crossing angle α between the two radial directions at the
        // foot, and the rim density it demands. With parallel axes the two
        // cross-section POLYGONS must cross exactly once, at the minted
        // vertex: on the side where B's circle is outside A's the surfaces
        // separate as `sin α · s` while B's chord adjacent to the mint sags
        // `s(L − s)/2R_B` inside its circle — and A's polygon may have a
        // vertex ON its circle anywhere along that chord (measured on the
        // R0038 replica: A's uniform slot 0.12° past the mint, B's next
        // vertex 6.9° away, B's chord 1.6e-3 deep against a 1.3e-3
        // separation ⇒ a second crossing, a sliver, the collapsed chain).
        // Sufficient on both sides: every chord step adjacent to the mint
        // has `sin(θ/2) < sin α`, i.e. the shared rim count `N ≥ π/α`.
        let radial = |ap: Point3, ad: Vector3| -> [f64; 3] {
            let (ap, ah) = (ap.as_array(), normalize3(ad.as_array()));
            let w = [pa[0] - ap[0], pa[1] - ap[1], pa[2] - ap[2]];
            let h = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
            normalize3([w[0] - h * ah[0], w[1] - h * ah[1], w[2] - h * ah[2]])
        };
        let (na, nb) = (radial(cyl_a.0, cyl_a.1), radial(cyl_b.0, cyl_b.1));
        let cos_alpha = (na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2]).clamp(-1.0, 1.0);
        let alpha = cos_alpha.acos();
        if alpha.is_nan() || alpha <= 0.0 || !alpha.is_finite() {
            return;
        }
        let demand = (std::f64::consts::PI / alpha).ceil() as usize + 1;
        if demand > CROSSING_RIM_N_CEILING {
            if probe {
                eprintln!(
                    "[tangent-insert] A#{fa_idx} B#{fb_idx} crossing ruling {pa:?} SKIP: \
                     crossing angle {:.4}° demands N = {demand} > {CROSSING_RIM_N_CEILING}",
                    alpha.to_degrees()
                );
            }
            return;
        }
        let h_of = |c: Point3| -> f64 {
            let q = c.as_array();
            (q[0] - pa[0]) * u[0] + (q[1] - pa[1]) * u[1] + (q[2] - pa[2]) * u[2]
        };
        let span = |t: &Tube| -> (f64, f64) {
            let (h0, h1) = (h_of(t.rims[0].1), h_of(t.rims[1].1));
            (h0.min(h1), h0.max(h1))
        };
        let (la, ha) = span(tube_a);
        let (lb, hb) = span(tube_b);
        let (lo, hi) = (la.max(lb), ha.min(hb));
        let margin = cad_primitives::TAU_MODEL * (1.0 + scale.max(hi.abs()).max(lo.abs()));
        if hi - lo <= margin {
            if probe {
                eprintln!(
                    "[tangent-insert] A#{fa_idx} B#{fb_idx} crossing ruling {pa:?} + t·{u:?} \
                     SKIP: axial spans do not overlap (A [{la:.6},{ha:.6}] B [{lb:.6},{hb:.6}])"
                );
            }
            return;
        }
        let on = [cyl_a, cyl_b].into_iter().all(|(ap, ad, r)| {
            let (ap, ah) = (ap.as_array(), normalize3(ad.as_array()));
            let w = [pa[0] - ap[0], pa[1] - ap[1], pa[2] - ap[2]];
            let h = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
            let rad = [w[0] - h * ah[0], w[1] - h * ah[1], w[2] - h * ah[2]];
            let len = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            (len - r).abs() <= cad_primitives::TAU_EVAL * (1.0 + scale)
        });
        if !on {
            if probe {
                eprintln!(
                    "[tangent-insert] A#{fa_idx} B#{fb_idx} crossing ruling {pa:?} + t·{u:?} \
                     REJECT off-surface"
                );
            }
            return;
        }
        // A sector contains at most one of the two rulings (the other lies
        // outside its sweep): a declined ruling is skipped, not the pair.
        let Some(samples) = ruling_rim_samples(
            p0,
            u,
            [tube_a, tube_b],
            margin,
            probe,
            &format!("A#{fa_idx} B#{fb_idx} crossing ruling"),
        ) else {
            continue;
        };
        if probe {
            eprintln!(
                "[tangent-insert] A#{fa_idx} B#{fb_idx} MINT crossing ruling {pa:?} + t·{u:?} \
                 over h ∈ [{lo:.6},{hi:.6}] (crossing {:.4}°, rim N ≥ {demand})",
                alpha.to_degrees()
            );
        }
        for (rims, out) in samples.iter().zip([&mut *out_a, &mut *out_b]) {
            for &(ei, seam, sample) in rims {
                push_rim_unless_seam(out, ei, seam, sample, probe);
            }
            out.min_rim_n = Some(out.min_rim_n.map_or(demand, |n| n.max(demand)));
        }
    }
}

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
//! - CYLINDER × CYLINDER only, non-parallel axes (parallel axes are tangent
//!   along a whole GENERATOR, a line pinch — the F0060 class, a different
//!   vehicle);
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
    /// starts and ends at — the ruling the tube grid already carries.
    rims: [(u32, Point3, Point3); 2],
}

fn tube_axial_span(f: &BRepFace, y: &BRep) -> Option<Tube> {
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        ..
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
    let [(ei0, rim0), (ei1, rim1)] = rims.as_slice() else {
        return None;
    };
    let ap = axis_point.as_array();
    let ah = normalize3(axis_dir.as_array());
    let axial = |p: Point3| -> f64 {
        let q = p.as_array();
        (q[0] - ap[0]) * ah[0] + (q[1] - ap[1]) * ah[1] + (q[2] - ap[2]) * ah[2]
    };
    let centre = |e: &BRepEdge| -> Point3 {
        let Curve::Circle { center, .. } = e.curve else {
            unreachable!("filtered to circles above");
        };
        center
    };
    let (c0, c1) = (centre(rim0), centre(rim1));
    let (v0, v1) = (axial(c0), axial(c1));
    let seam = |e: &BRepEdge| y.vertices()[e.start as usize].point;
    Some(Tube {
        lo: v0.min(v1),
        hi: v0.max(v1),
        rims: [(*ei0, c0, seam(rim0)), (*ei1, c1, seam(rim1))],
    })
}

/// One operand's Stage-1 override payload for the tangency mint.
#[derive(Default)]
pub(crate) struct TangentOverrides {
    /// face index → the exact tangent points to mint as face interiors.
    pub face: BTreeMap<u32, Vec<Point3>>,
    /// rim edge index → the rim-circle samples that carry the tangency's
    /// azimuth, so the tube grid has a RULING through each minted point.
    pub rim: BTreeMap<u32, Vec<Point3>>,
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
                continue; // parallel axes — generator tangency, out of scope
            };
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
                let push = |m: &mut BTreeMap<u32, Vec<Point3>>, k: u32, q: Point3| {
                    let e = m.entry(k).or_default();
                    let qa = q.as_array();
                    let key = [qa[0].to_bits(), qa[1].to_bits(), qa[2].to_bits()];
                    if !e
                        .iter()
                        .any(|r| [r.x().to_bits(), r.y().to_bits(), r.z().to_bits()] == key)
                    {
                        e.push(q);
                    }
                };
                // The point itself, as a face interior…
                push(&mut out_a.face, fa_idx as u32, p);
                push(&mut out_b.face, fb_idx as u32, p);
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
                        // The tangency azimuth can BE the seam's: the tube grid
                        // already carries that ruling, and pushing a re-derived
                        // copy that differs from the authoritative B-Rep vertex
                        // in the last bits is refused loudly by the rim build
                        // ("coincides with the seam vertex but differs in
                        // bits"). Skip it — fail closed, the ruling is there.
                        let (sa, sv) = (sample.as_array(), seam.as_array());
                        let sc = sa
                            .iter()
                            .chain(sv.iter())
                            .fold(0.0f64, |m, &c| m.max(c.abs()));
                        let band = cad_primitives::TAU_MODEL * (1.0 + sc);
                        let d2 = (sa[0] - sv[0]).powi(2)
                            + (sa[1] - sv[1]).powi(2)
                            + (sa[2] - sv[2]).powi(2);
                        if d2 < band * band {
                            if probe {
                                eprintln!(
                                    "[tangent-insert] rim {ei} SKIP {sa:?}: the tangency azimuth \
                                     IS the seam ruling"
                                );
                            }
                            continue;
                        }
                        push(&mut out.rim, ei, sample);
                    }
                }
            }
        }
    }
    (out_a, out_b)
}

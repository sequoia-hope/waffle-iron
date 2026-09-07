//! Stage-0 mesh construction: boundary-split propagation, overlay dump,
//! build_stage0_mesh, vertex interning, exact ring triangulation
//! (extracted verbatim from stage0/mod.rs — spec
//! `specs/stage0_decomposition.md`, increment 3).

#[allow(clippy::wildcard_imports)]
use super::*;

// ════════════════════════════════════════════════════════════════════════
// boundary-split propagation (§4.5.5 shared sampling points)
// ════════════════════════════════════════════════════════════════════════

/// Splits keyed by the UNDIRECTED endpoint vertex-index pair (B-Rep edges
/// are commonly duplicated per face — e.g. the box fixtures carry 24
/// directed edges over 12 undirected segments — so geometric identity is
/// the vertex pair, not the edge index). Each split: exact parameter along
/// the canonical `min(vi) → max(vi)` direction + the shared 3D coordinate.
pub(crate) type SplitMap = BTreeMap<(u32, u32), Vec<(RBig, Point3)>>;

/// PR-M8 disc-rim crossing: extra 3D crossing points to insert into a
/// full-circle rim edge's Stage-1 ring, keyed by the rim's `Curve::Circle`
/// edge index (one map per solid). Threaded into
/// [`stage1_tessellate_with_rim_overrides`] so the cap, the cylinder lateral,
/// and the opposite cap all share the SAME subdivided rim (no T-junction).
pub(crate) type RimSplitMap = BTreeMap<u32, Vec<Point3>>;

// ════════════════════════════════════════════════════════════════════════
// Sub-floor shared-mint grouping: admission predicate (spec
// `m8_stage0_rim_membership_refine` §3b trio-wedge follow-on)
// ════════════════════════════════════════════════════════════════════════

/// Should a minted overlay vertex join the shared-mint collapse group whose
/// head is (`head_3d`, `head_2d`)? Gate-OFF: the historical resolved-3D
/// sub-floor band (`MIN_FEATURE_SIZE`, always-on since the inc-2 corpus
/// flip) — byte-identical. Gate-ON, identity is read where it lives:
///
/// - **2D tier: pre-images closer than the feature floor
///   (`MIN_FEATURE_SIZE`) are ONE arrangement vertex.** The floor applies
///   to the PRE-images because resolution both diverges and converges
///   distances: the crossing-vs-radial branches of one femto-identical
///   column pair diverge O(sag·tan(tilt)) in 3D (F0067: 3.3e-5, spec §3b
///   first follow-on), while two genuinely distinct neighboring-column
///   mints can land sub-floor-close after radial projection (F0067
///   corner_a 761: the corner-column mint's radial image is 8.5e-7 from
///   the flank junction while its 2D pre-image is 8.9e-6 away — a REAL
///   above-floor vertex; enrolling it re-writes chain topology, the
///   V-notch Overlap triangles fold onto the boundary ribbon, and the
///   junction↔corner edge ships FOUR incident triangles,
///   `i6-edge-overuse`). The micro classes need the full floor width, not
///   a femto band: R0072's twins sit ~1e-7 apart in BOTH spaces (model
///   scale 2e-4) and MUST identify (left distinct their wedge folds, the
///   fold gate reverts the mints to chords, and Stage-4 relocation
///   dead-ends: `LocalRefinementRequired`, the R0072 micro class). No
///   3D band can hold both cases (9.5e-7-must-collapse vs
///   8.5e-7-must-not); the 2D pre-image separates them exactly.
/// - **3D tier: resolved images within rounding noise
///   (`TAU_WORK·(1+scale)`)** — the (222,286) wide-anchored class:
///   coincident junction images from far anchors are one point even
///   though their pre-images are far apart.
pub(crate) fn mint_group_admits(
    rim_refine_gate: bool,
    cand_3d: Point3,
    head_3d: Point3,
    cand_2d: Point2,
    head_2d: Point2,
) -> bool {
    let p = cand_3d.as_array();
    let q = head_3d.as_array();
    let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let band3 = if rim_refine_gate {
        let scale = p[0].abs().max(p[1].abs()).max(p[2].abs());
        cad_primitives::TAU_WORK * (1.0 + scale)
    } else {
        cad_primitives::MIN_FEATURE_SIZE
    };
    if d2 < band3 * band3 {
        return true;
    }
    if rim_refine_gate {
        let (du, dv) = (cand_2d.x() - head_2d.x(), cand_2d.y() - head_2d.y());
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        return du * du + dv * dv < floor * floor;
    }
    false
}

// ════════════════════════════════════════════════════════════════════════
// Amendment 18 (spec `m8_stage0_multiclass_cavity_arm` §16b): congruent-rim
// cross-solid table ELECTION. On stacked congruent caps the two solids' rims
// are the SAME geometric circle in different frames; a shared junction
// azimuth then survives as one `rim_a` anchor + one `rim_b` anchor at
// ulp-different exact on-circle values — protected from the rim-aware
// clustering (on-circle points must not move), from the #61 collapse (not
// minted) and from the §15 absorption (rim anchors excluded) BY DESIGN. The
// emission then carries the femto pair plus bridging slivers into both
// meshes (C0048 base-tri-207 needle → cherchi DegenerateTpi). Fuse each
// such pair to ONE member's (uv, point) adopted wholesale.
// ════════════════════════════════════════════════════════════════════════

/// One detected congruent-rim cross-table fusion: adopt `(win_key, v)`
/// wholesale; rewrite the losing member's key/value, polygon corner, and
/// cluster-map image.
pub(crate) struct RimTableFusion {
    pub(crate) win_key: ExactPoint2,
    pub(crate) v: Point3,
    pub(crate) lose_key: ExactPoint2,
    pub(crate) lose_pt: Point3,
    /// True when the LOSING member lives in B's table/polygon.
    pub(crate) losing_is_b: bool,
}

/// Scan `rim_a × rim_b` for cross-solid same-junction pairs: exact uv
/// distance AND f64 3D distance both within the §15 rounding-noise band
/// `TAU_WORK·(1+scale)` (five orders above the measured 4.3e-14 cluster,
/// three below the protected E-C1b distinct-twin population), 3D values
/// bit-DIFFERENT (bit-equal pairs are already the handled M-B
/// identification class). Election is the lexicographically smaller 3D bit
/// pattern — deterministic and frame-independent. Each key participates in
/// at most one fusion (first-seen in BTreeMap order — sub-band pairs are
/// isolated in practice; a chained cluster fuses pairwise deterministically).
pub(crate) fn detect_rim_table_fusions(
    rim_a: &BTreeMap<ExactPoint2, Point3>,
    rim_b: &BTreeMap<ExactPoint2, Point3>,
) -> Vec<RimTableFusion> {
    let bits = |p: &Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let mut used_a: std::collections::BTreeSet<ExactPoint2> = std::collections::BTreeSet::new();
    let mut used_b: std::collections::BTreeSet<ExactPoint2> = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (ka, pa) in rim_a {
        if used_a.contains(ka) {
            continue;
        }
        for (kb, pb) in rim_b {
            if used_b.contains(kb) || bits(pa) == bits(pb) {
                continue;
            }
            let uv_scale = ka.x.to_f64().value().abs().max(ka.y.to_f64().value().abs());
            let band = cad_primitives::TAU_WORK * (1.0 + uv_scale);
            let Ok(band_r) = rat(band) else { continue };
            let band2 = &band_r * &band_r;
            let du = &ka.x - &kb.x;
            let dv = &ka.y - &kb.y;
            if &du * &du + &dv * &dv > band2 {
                continue;
            }
            let d3 = [pa.x() - pb.x(), pa.y() - pb.y(), pa.z() - pb.z()];
            if d3[0] * d3[0] + d3[1] * d3[1] + d3[2] * d3[2] > band * band {
                continue;
            }
            let a_wins = bits(pa) <= bits(pb);
            out.push(if a_wins {
                RimTableFusion {
                    win_key: ka.clone(),
                    v: *pa,
                    lose_key: kb.clone(),
                    lose_pt: *pb,
                    losing_is_b: true,
                }
            } else {
                RimTableFusion {
                    win_key: kb.clone(),
                    v: *pb,
                    lose_key: ka.clone(),
                    lose_pt: *pa,
                    losing_is_b: false,
                }
            });
            used_a.insert(ka.clone());
            used_b.insert(kb.clone());
            break;
        }
    }
    out
}

/// Apply one fusion: the losing table's entry is re-keyed to the elected
/// `(win_key, v)`, every bit-equal losing polygon corner is rewritten to the
/// winning uv, and the cluster pre→post map is chained (pre-images of the
/// losing uv now land on the winning uv — the M-A/E7 contract for every
/// consumer that re-derives 2D coordinates).
pub(crate) fn apply_rim_table_fusion(
    f: &RimTableFusion,
    rim_a: &mut BTreeMap<ExactPoint2, Point3>,
    rim_b: &mut BTreeMap<ExactPoint2, Point3>,
    poly_a: &mut PolygonWithHoles,
    poly_b: &mut PolygonWithHoles,
    key_map: &mut BTreeMap<(u64, u64), (u64, u64)>,
) {
    let (lose_tbl, lose_poly) = if f.losing_is_b {
        (rim_b, poly_b)
    } else {
        (rim_a, poly_a)
    };
    lose_tbl.remove(&f.lose_key);
    lose_tbl.insert(f.win_key.clone(), f.v);
    let (lu, lv) = (f.lose_key.x.to_f64().value(), f.lose_key.y.to_f64().value());
    let (wu, wv) = (f.win_key.x.to_f64().value(), f.win_key.y.to_f64().value());
    for ring in std::iter::once(&mut lose_poly.outer).chain(lose_poly.holes.iter_mut()) {
        for q in ring.iter_mut() {
            if q.x().to_bits() == lu.to_bits() && q.y().to_bits() == lv.to_bits() {
                *q = Point2::new(wu, wv);
            }
        }
    }
    let lb = (lu.to_bits(), lv.to_bits());
    let wb = (wu.to_bits(), wv.to_bits());
    for post in key_map.values_mut() {
        if *post == lb {
            *post = wb;
        }
    }
    key_map.insert(lb, wb);
}

/// M-C diagnosis dump (read-only observer; fires only with
/// `YANG_STAGE0_DUMP_DIR` set — never in production/WASM). One file per
/// processed overlay pair: per-vertex resolution provenance (which map the
/// 2D→3D resolution hit) + resolved 3D coordinate, per-triangle region
/// class and per-side emission verdict including the E8 resolved-degenerate
/// drop, and the split maps as collected after this pair. Joins the operand
/// census's offender vertices back to overlay entities.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dump_pair_overlay(
    pair: (usize, usize, f64, bool),
    overlay: &ClassifiedOverlay,
    corners_a: &BTreeMap<ExactPoint2, u32>,
    corners_b: &BTreeMap<ExactPoint2, u32>,
    rim_a: &BTreeMap<ExactPoint2, Point3>,
    rim_b: &BTreeMap<ExactPoint2, Point3>,
    rim_pts: &[(f64, f64, Point3)],
    snap_eps2: f64,
    minted_mark: &[bool],
    coords: &[Point3],
    frame: &Frame,
    splits_a: &SplitMap,
    splits_b: &SplitMap,
    polys: [&PolygonWithHoles; 2],
) {
    let Some(dir) = std::env::var_os("YANG_STAGE0_DUMP_DIR") else {
        return;
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    static PAIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = PAIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let (face_a, face_b, band, opposite) = pair;
    let mut out = format!(
        "pair: face_a={face_a} face_b={face_b} band={band} opposite={opposite}\n\
         verts: {}\n",
        overlay.verts.len()
    );
    for (i, exact) in overlay.exact_verts.iter().enumerate() {
        let tag = if let Some(ai) = corners_a.get(exact) {
            format!("corner_a({ai})")
        } else if let Some(bi) = corners_b.get(exact) {
            format!("corner_b({bi})")
        } else if rim_a.contains_key(exact) {
            "rim_a".into()
        } else if rim_b.contains_key(exact) {
            "rim_b".into()
        } else {
            let q = overlay.verts[i];
            let (qx, qy) = (q.x(), q.y());
            let near_rim = rim_pts.iter().any(|(u, v, _)| {
                let (du, dv) = (u - qx, v - qy);
                du * du + dv * dv <= snap_eps2
            });
            if near_rim {
                "rimsnap".into()
            } else if minted_mark[i] {
                let q = overlay.verts[i];
                if coords[i] == frame.lift(q.x(), q.y()) {
                    "mint(rev)".into()
                } else {
                    "mint".into()
                }
            } else {
                "lift".into()
            }
        };
        let p3 = coords[i];
        let q = overlay.verts[i];
        out.push_str(&format!(
            "v {i} u={} v={} tag={tag} xyz=({},{},{})\n",
            q.x(),
            q.y(),
            p3.x(),
            p3.y(),
            p3.z()
        ));
    }
    out.push_str(&format!("tris: {}\n", overlay.tris.len()));
    let bits = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    for (ti, (t, c)) in overlay.tris.iter().zip(&overlay.class).enumerate() {
        let b = [
            bits(coords[t[0] as usize]),
            bits(coords[t[1] as usize]),
            bits(coords[t[2] as usize]),
        ];
        let e8 = b[0] == b[1] || b[1] == b[2] || b[0] == b[2];
        let kept_a = matches!(c, RegionClass::AOnly | RegionClass::Overlap) && !e8;
        let kept_b = matches!(c, RegionClass::BOnly | RegionClass::Overlap) && !e8;
        out.push_str(&format!(
            "t {ti} [{},{},{}] class={c:?} e8drop={e8} kept_a={kept_a} kept_b={kept_b}\n",
            t[0], t[1], t[2]
        ));
    }
    for (name, poly) in [("poly_a", polys[0]), ("poly_b", polys[1])] {
        for (li, lp) in std::iter::once(&poly.outer)
            .chain(poly.holes.iter())
            .enumerate()
        {
            out.push_str(&format!("{name} loop {li} n={}:\n", lp.len()));
            for pt in lp {
                out.push_str(&format!("  ({},{})\n", pt.x(), pt.y()));
            }
        }
    }
    for (name, splits) in [("splits_a", splits_a), ("splits_b", splits_b)] {
        out.push_str(&format!("{name}: {}\n", splits.len()));
        for ((lo, hi), pts) in splits {
            let items: Vec<String> = pts
                .iter()
                .map(|(t, p)| {
                    format!(
                        "t={} xyz=({},{},{})",
                        t.to_f64().value(),
                        p.x(),
                        p.y(),
                        p.z()
                    )
                })
                .collect();
            out.push_str(&format!("  edge ({lo},{hi}): [{}]\n", items.join(", ")));
        }
    }
    let path =
        std::path::PathBuf::from(dir).join(format!("overlay_{seq:03}_pair{face_a}_{face_b}.txt"));
    if let Err(e) = std::fs::write(&path, out) {
        eprintln!("[overlay-dump] write {} failed: {e}", path.display());
    }
}

/// Find overlay vertices lying strictly inside one of the face's loop
/// edges (exact 2D on-open-segment test over the overlay's rational
/// coordinates) and record them, with the SAME resolved 3D coordinates the
/// override triangles use, for propagation into adjacent faces.
///
/// `merged_pts` (amendment 13 inc-3.5, spec `m8_stage0_multiclass_cavity_arm`
/// §10d): resolved positions of SURVIVING Fig-11 merge targets. Two distinct
/// overlay vertices identified by a merge still carry distinct exact chord
/// parameters, so without this a merged pair registers the same 3D point
/// TWICE on the shared edge (measured: R0059 splits_b edge (5,6),
/// t=0.62276 and t=0.62517 both at the junction) — the adjacent face's
/// chain then gains a zero-length segment the overlay side dropped at
/// emission (M-B). Deduping by merged position carries the §4.4.1 merge
/// identification through the §4.5.5 propagation — the same argument as the
/// M-B drop itself. Scoped to merge targets so gate-OFF stays byte-identical
/// (an empty set is the historical behavior, bit for bit).
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_edge_splits(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
    cluster_map: &BTreeMap<(u64, u64), (u64, u64)>,
    overlay: &ClassifiedOverlay,
    side_classes: [RegionClass; 2],
    resolved: &[Point3],
    merged_pts: &std::collections::BTreeSet<[u64; 3]>,
    splits: &mut SplitMap,
) {
    // Overlay vertices used by THIS side's triangles (the conforming
    // triangulation guarantees any vertex on the face boundary that the
    // side's triangulation needs is used by a side triangle).
    let mut used = vec![false; overlay.verts.len()];
    // Directed edges of THIS side's triangulation, for the boundary test
    // below: a side vertex is a BOUNDARY vertex of the side's region iff it
    // carries a directed side edge whose reverse is not a side edge.
    let mut side_dir: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for (t, c) in overlay.tris.iter().zip(&overlay.class) {
        if side_classes.contains(c) {
            for &v in t {
                used[v as usize] = true;
            }
            for k in 0..3 {
                side_dir.insert((t[k], t[(k + 1) % 3]));
            }
        }
    }
    let mut on_boundary = vec![false; overlay.verts.len()];
    for &(u, v) in &side_dir {
        if !side_dir.contains(&(v, u)) {
            on_boundary[u as usize] = true;
            on_boundary[v as usize] = true;
        }
    }

    let f = &brep.faces()[fi];
    for &e_idx in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        let edge = &brep.edges()[e_idx as usize];
        // Splits are a STRAIGHT-edge mechanism. A full circle already
        // self-skips below (start == end ⇒ zero length), but a mixed face's
        // ARC edge would otherwise be treated as its secant segment — an
        // exactly-on-secant vertex would register a bogus split
        // (spec `m8_mixed_loop_coplanar_overlay` §6). Curved-chord
        // subdivision walls at the slice-1 gate instead.
        if !matches!(edge.curve, Curve::LineSegment) {
            continue;
        }
        let (lo, hi) = (edge.start.min(edge.end), edge.start.max(edge.end));
        // M-A (spec `m8_stage0_inputcheck_clean_emission` E7): the overlay's
        // vertices live in the CLUSTERED 2D domain; a raw endpoint projection
        // disagrees with it at every clustering-moved vertex, so the exact
        // collinearity test below would silently drop all splits on that
        // edge. Route each projection through the pair's pre→post map (the
        // identity for unmoved vertices — byte-identical path).
        let snap = |u: f64, v: f64| -> (f64, f64) {
            match cluster_map.get(&(u.to_bits(), v.to_bits())) {
                Some(&(nu, nv)) => (f64::from_bits(nu), f64::from_bits(nv)),
                None => (u, v),
            }
        };
        let (su, sv) = {
            let (u, v) = frame.project(coords[lo as usize]);
            snap(u, v)
        };
        let (eu, ev) = {
            let (u, v) = frame.project(coords[hi as usize]);
            snap(u, v)
        };
        let (Some(s2), Some(e2)) = (ExactPoint2::from_f64(su, sv), ExactPoint2::from_f64(eu, ev))
        else {
            continue;
        };
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        for (i, &is_used) in used.iter().enumerate() {
            if !is_used {
                continue;
            }
            let q = &overlay.exact_verts[i];
            let wx = &q.x - &s2.x;
            let wy = &q.y - &s2.y;
            // Exact collinearity + strictly-interior parameter.
            let cross = &dx * &wy - &dy * &wx;
            if cross != RBig::ZERO {
                // 2026-08-19 (R0053 anchor): an overlay vertex that the side's
                // triangulation uses ON ITS REGION BOUNDARY and that is
                // collinear with the edge to the scale-free collinearity
                // IDENTITY (miss ≤ DEGENERACY_IDENTITY_REL · edge length —
                // the `chain_straightness` / `tri_is_degenerate` band) IS a
                // subdivision of this edge: the overlay's own boundary chain
                // passes through it between the edge's corners. The exact
                // test alone dropped such vertices when an identification
                // step had perturbed a minted crossing by f64 rounding
                // (R0053: B face 0 edge (180,181), vertex 1469 at 8.4e-16 —
                // the overlay triangulated f0's boundary THROUGH it while the
                // propagation to the adjacent cone lateral never saw it →
                // a T-junction on the Stage-0 mesh → `i6-input-overuse`).
                // Measured population on R0053: 522 rounding-scale misses
                // (1e-16..1e-13 absolute on ~1.5 m edges) vs 216 genuine
                // misses at 1e-4..1e-1, nothing between — the identity
                // separates them by four orders on each side. Interior
                // (non-boundary) side vertices never qualify, so a vertex
                // merely NEAR the edge cannot be mistaken for a split.
                let len = len2.to_f64().value().sqrt();
                let miss = (cross.to_f64().value() / len).abs();
                let identity_on =
                    on_boundary[i] && miss <= crate::stage4_correct::DEGENERACY_IDENTITY_REL * len;
                // M-C diagnosis probe (read-only, env-gated): report exact
                // NON-collinear vertices whose perpendicular miss distance is
                // tiny — the band-scale near-miss population.
                if std::env::var_os("YANG_SPLIT_PROBE").is_some() && miss < 1.0e-3 * len {
                    let t = ((&dx * &wx + &dy * &wy) / &len2).to_f64().value();
                    eprintln!(
                        "[split-probe] f={fi} edge ({lo},{hi}) vert {i} NEAR-MISS \
                         dist={miss:e} t={t} boundary={} identity_on={identity_on}",
                        on_boundary[i]
                    );
                }
                if !identity_on {
                    continue;
                }
            }
            let t = (&dx * &wx + &dy * &wy) / &len2;
            if t <= RBig::ZERO || t >= RBig::ONE {
                if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                    eprintln!(
                        "[split-probe] f={fi} edge ({lo},{hi}) vert {i} ON-LINE but t={} \
                         out of (0,1)",
                        t.to_f64().value()
                    );
                }
                continue;
            }
            let entry = splits.entry((lo, hi)).or_default();
            let key = {
                let a = resolved[i].as_array();
                [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
            };
            let merged_dup = merged_pts.contains(&key)
                && entry.iter().any(|(_, p0)| {
                    let b = p0.as_array();
                    [b[0].to_bits(), b[1].to_bits(), b[2].to_bits()] == key
                });
            if !merged_dup && !entry.iter().any(|(t0, _)| *t0 == t) {
                if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                    eprintln!(
                        "[split-probe] f={fi} edge ({lo},{hi}) vert {i} SPLIT t={} pos={:?} \
                         merged={} n_on_edge={}",
                        t.to_f64().value(),
                        resolved[i].as_array(),
                        merged_pts.contains(&key),
                        entry.len() + 1
                    );
                }
                entry.push((t, resolved[i]));
            }
        }
    }
    for v in splits.values_mut() {
        v.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

// ════════════════════════════════════════════════════════════════════════
// Stage-1 re-tessellation with overrides + splits
// ════════════════════════════════════════════════════════════════════════

pub(crate) enum BuildErr {
    Yang(YangError),
    /// A face outside the supported shape (curved / holed / non-continuous
    /// loop) needs boundary subdivision — unsupported residue, mapped to
    /// the pair's typed error by the caller.
    Unsupported,
}

impl From<YangError> for BuildErr {
    fn from(e: YangError) -> Self {
        BuildErr::Yang(e)
    }
}

/// Fan-split triangle `tri` along the edge between loop positions `i` and
/// `i+1`, inserting the ordered `interior` vertex indices (in `tri[i]→tri[i+1]`
/// order). Every replacement triangle preserves `tri`'s winding: it fans from
/// the opposite vertex `tri[i+2]` through the subdivided boundary chain
/// `tri[i] → interior… → tri[i+1]`, which is a sub-traversal of the original
/// CCW boundary. Pure index arithmetic — unit-tested.
pub(crate) fn fan_split_tri(tri: [u32; 3], i: usize, interior: &[u32]) -> Vec<[u32; 3]> {
    let opp = tri[(i + 2) % 3];
    let mut chain: Vec<u32> = Vec::with_capacity(interior.len() + 2);
    chain.push(tri[i]);
    chain.extend_from_slice(interior);
    chain.push(tri[(i + 1) % 3]);
    chain.windows(2).map(|w| [opp, w[0], w[1]]).collect()
}

/// Surface-agnostic edge split for a CURVED face whose subdivided boundary
/// edges are ALL straight `Curve::LineSegment` generators (M8 partial-cap /
/// cylinder-lateral case, R0015): a partial-revolve cap shares a generator with
/// the cylinder lateral, and the coplanar overlap boundary crossed that
/// generator. The split points are collinear ON the straight edge — already on
/// the curved surface — so the face's base tessellation absorbs them by
/// splitting the base-tess triangle that carries each subdivided generator
/// (fan from the opposite vertex through the inserted points). NO curved
/// re-tessellation, exact, and conformal with the planar neighbour that splits
/// the same edge at the same shared `splits` points.
///
/// Returns `None` (→ the loud `build-mesh-nonplanar` residue stands) if ANY
/// subdivided boundary edge is CURVED (an arc rim — the deferred resampling
/// case), or if one base-tess triangle carries TWO subdivided edges (a clean
/// fan split is not well-defined) — keeping the conformal contract loud rather
/// than risking a gap.
pub(crate) fn edge_split_curved_face(
    brep: &BRep,
    f_idx: usize,
    tess: &crate::Stage1Tess,
    splits: &SplitMap,
    verts: &mut Vec<Point3>,
    intern: &mut BTreeMap<[u64; 3], u32>,
) -> Option<Vec<[u32; 3]>> {
    let f = &brep.faces()[f_idx];
    let mut subdiv: BTreeMap<(u32, u32), &Vec<(RBig, Point3)>> = BTreeMap::new();
    for &e in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let edge = &brep.edges()[e as usize];
        let key = (edge.start.min(edge.end), edge.start.max(edge.end));
        if let Some(pts) = splits.get(&key) {
            if !matches!(edge.curve, Curve::LineSegment) {
                // Splits are collected on straight edges only; a curved edge
                // matching this vertex-pair key merely SHARES both endpoints
                // with the split straight edge (semicircle arc + diameter
                // chord, M8-mixed) — it is not itself subdivided.
                continue;
            }
            subdiv.insert(key, pts);
        }
    }
    if subdiv.is_empty() {
        return None;
    }
    let range = tess.face_tri_ranges.get(f_idx)?.clone();
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(range.len() + subdiv.len() * 2);
    for tri in &tess.tris[range] {
        let hits: Vec<usize> = (0..3)
            .filter(|&i| {
                let (a, b) = (tri[i], tri[(i + 1) % 3]);
                subdiv.contains_key(&(a.min(b), a.max(b)))
            })
            .collect();
        match hits.len() {
            0 => out.push(*tri),
            1 => {
                let i = hits[0];
                let (a, b) = (tri[i], tri[(i + 1) % 3]);
                let key = (a.min(b), a.max(b));
                let pts = subdiv[&key];
                // Stored points run lo→hi; this triangle traverses a→b.
                let forward = a == key.0;
                let interior: Vec<u32> = {
                    let it: Box<dyn Iterator<Item = &(RBig, Point3)>> = if forward {
                        Box::new(pts.iter())
                    } else {
                        Box::new(pts.iter().rev())
                    };
                    it.map(|(_, p)| intern_vert(verts, intern, *p)).collect()
                };
                out.extend(fan_split_tri(*tri, i, &interior));
            }
            // ≥2 subdivided edges on one triangle — defer loudly (no clean fan).
            _ => return None,
        }
    }
    Some(out)
}

/// Build one solid's Stage-0 mesh: the normal Stage-1 tessellation over the
/// SNAPPED vertex coordinates, with overlay faces' triangles replaced by
/// the overlay triangulation and split-edge neighbor faces re-triangulated
/// with the subdivided boundary ring.
/// Returns the re-tessellated mesh AND a per-output-triangle → owning-face map
/// (`tri_face`, 1:1 with the mesh triangles) — the §4.2.3 provenance for the
/// Stage-0 mesh, so `boolean()`'s Stage-6 can attribute coplanar-overlap
/// triangles by provenance instead of geometric proximity (N4, increment 2a).
pub(crate) fn build_stage0_mesh(
    brep: &BRep,
    final_coords: &[Point3],
    overrides: &BTreeMap<usize, Vec<[Point3; 3]>>,
    splits: &SplitMap,
    rim_overrides: &RimSplitMap,
) -> Result<(Mesh, Vec<u32>), BuildErr> {
    let brep_verts: Vec<BRepVertex> = final_coords
        .iter()
        .map(|&p| BRepVertex { point: p })
        .collect();
    let tess = stage1_tessellate_with_rim_overrides(
        &brep_verts,
        brep.edges(),
        brep.faces(),
        rim_overrides,
        brep.forced_rim_n(),
    )?;

    // Twin-origin drill-down (read-only, shares `YANG_INPUT_VERT_PROBE=x,y,z,r`
    // with the boolean.rs input scan): name the PRODUCER of every Stage-0
    // container entry near the target — base-tessellation vertices with their
    // `TessellationSource`, propagated straight-edge splits, and rim
    // overrides — so an interface ulp-twin pair self-localizes to the
    // machinery that minted each member (F0067 LabelMismatch anchor).
    if let Some(spec) = std::env::var_os("YANG_INPUT_VERT_PROBE") {
        let nums: Vec<f64> = spec
            .to_string_lossy()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if let [x, y, z, r] = nums[..] {
            let near = |p: &Point3| {
                let q = p.as_array();
                let d = [q[0] - x, q[1] - y, q[2] - z];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= r
            };
            for (i, p) in tess.verts.iter().enumerate() {
                if near(p) {
                    let q = p.as_array();
                    eprintln!(
                        "[s0-build-probe] tess vert {i} ({},{},{}) source {:?}",
                        q[0], q[1], q[2], tess.sources[i]
                    );
                }
            }
            for (&(lo, hi), pts) in splits.iter() {
                for (t, p) in pts {
                    if near(p) {
                        let q = p.as_array();
                        eprintln!(
                            "[s0-build-probe] split edge ({lo},{hi}) t={} ({},{},{})",
                            t.to_f64().value(),
                            q[0],
                            q[1],
                            q[2]
                        );
                    }
                }
            }
            for (&e, pts) in rim_overrides.iter() {
                for p in pts {
                    if near(p) {
                        let q = p.as_array();
                        eprintln!(
                            "[s0-build-probe] rim_override edge {e} ({},{},{})",
                            q[0], q[1], q[2]
                        );
                    }
                }
            }
        }
    }

    // Bit-exact coordinate interner seeded with the base tessellation's
    // vertex pool (B-Rep vertices occupy slots 0..n, so override corners
    // resolve back to the B-Rep vertex slots automatically).
    let mut verts: Vec<Point3> = tess.verts.clone();
    let mut intern: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    for (i, p) in verts.iter().enumerate() {
        intern
            .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
            .or_insert(i as u32);
    }

    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(tess.tris.len());
    // Per output triangle, the B-Rep face index that produced it. Each face's
    // triangles are appended contiguously below, so after every append we
    // `resize` to the new `tris` length, filling the just-added slots with the
    // current `f_idx` (resize leaves earlier entries untouched).
    let mut tri_face: Vec<u32> = Vec::with_capacity(tess.tris.len());
    for (f_idx, f) in brep.faces().iter().enumerate() {
        if let Some(ov_tris) = overrides.get(&f_idx) {
            for tri in ov_tris {
                let mut t = [0u32; 3];
                for (k, p) in tri.iter().enumerate() {
                    t[k] = intern_vert(&mut verts, &mut intern, *p);
                }
                tris.push(t);
            }
            tri_face.resize(tris.len(), f_idx as u32);
            continue;
        }

        // Does this face's boundary carry propagated split points? Splits are
        // collected on STRAIGHT edges only, and the map is keyed by vertex
        // pair — a curved edge sharing BOTH endpoints with a split straight
        // edge (a semicircle arc and its diameter chord, M8-mixed) must not
        // be mistaken for subdivided.
        let face_split = std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .flatten()
            .any(|&e| {
                let edge = &brep.edges()[e as usize];
                matches!(edge.curve, Curve::LineSegment)
                    && splits.contains_key(&(edge.start.min(edge.end), edge.start.max(edge.end)))
            });
        if !face_split {
            tris.extend_from_slice(&tess.tris[tess.face_tri_ranges[f_idx].clone()]);
            tri_face.resize(tris.len(), f_idx as u32);
            continue;
        }

        // Neighbor re-triangulation with the subdivided ring. Scope: planar,
        // all-LineSegment, hole-free, continuous outer loop.
        let Surface::Plane { normal, .. } = f.surface else {
            // M8 curved-neighbour (R0015): a CURVED face whose subdivided
            // boundary edges are ALL STRAIGHT line generators — e.g. a
            // partial-revolve cap shares a generator with the cylinder lateral,
            // and the coplanar overlap boundary crossed that generator. The
            // split points are collinear on a straight edge ALREADY ON the
            // curved surface, so the face's base tessellation absorbs them by a
            // surface-agnostic EDGE SPLIT — split each base-tess triangle that
            // carries a subdivided generator at the inserted points. No curved
            // re-tessellation. A subdivided CURVED (arc) boundary edge is NOT
            // handled here (the genuine arc-resampling case, deferred) → the
            // helper returns None and the loud residue stands.
            if let Some(face_tris) =
                edge_split_curved_face(brep, f_idx, &tess, splits, &mut verts, &mut intern)
            {
                tris.extend(face_tris);
                tri_face.resize(tris.len(), f_idx as u32);
                continue;
            }
            probe("build-mesh-nonplanar", &format!("f={f_idx}"));
            return Err(BuildErr::Unsupported);
        };
        if !f.inner_loops.is_empty() || !overlay_face_supported(brep, f_idx) {
            // The planar fan re-triangulation can't handle this face — a mixed
            // arc+line boundary (a partial-revolve washer-sector cap, R0015's
            // f=3 with outer curves [L,C,L,C]) or a holed face. But if its
            // subdivided edges are all STRAIGHT generators that are direct
            // base-tess edges, the surface-agnostic edge split works here too:
            // the base Stage-1 tessellation already conforms to the arc
            // boundary, and we only insert the straight-generator split points
            // (the same shared points the neighbour cap/lateral use).
            if let Some(face_tris) =
                edge_split_curved_face(brep, f_idx, &tess, splits, &mut verts, &mut intern)
            {
                tris.extend(face_tris);
                tri_face.resize(tris.len(), f_idx as u32);
                continue;
            }
            probe(
                "build-mesh-holed-or-unsupported",
                &format!(
                    "f={f_idx} holes={} sup={}",
                    f.inner_loops.len(),
                    overlay_face_supported(brep, f_idx)
                ),
            );
            return Err(BuildErr::Unsupported);
        }
        let mut ring: Vec<u32> = Vec::new();
        if mixed_planar_face(brep, f_idx) {
            // M8-mixed neighbor: splice the loop from the (post-insertion)
            // Stage-1 chains — chain Steiner indices are directly valid in
            // this pool (seeded from `tess.verts`) — and insert straight-edge
            // split points in traversal order. Curved edges take no splits
            // (their subdivision arrives through the chains themselves).
            let Ok(attributed) =
                crate::loop_polyline_attributed(f_idx, &f.outer_loop, brep.edges(), &tess.chains)
            else {
                probe("build-mesh-noncontinuous", &format!("f={f_idx} mixed"));
                return Err(BuildErr::Unsupported);
            };
            for &(g, e_idx) in &attributed {
                let edge = &brep.edges()[e_idx as usize];
                ring.push(g);
                if !matches!(edge.curve, Curve::LineSegment) {
                    continue;
                }
                let (lo, hi) = (edge.start.min(edge.end), edge.start.max(edge.end));
                if let Some(pts) = splits.get(&(lo, hi)) {
                    // A straight edge emits exactly its traversal origin `g`;
                    // stored params run lo→hi.
                    let forward = g == lo;
                    let it: Box<dyn Iterator<Item = &(RBig, Point3)>> = if forward {
                        Box::new(pts.iter())
                    } else {
                        Box::new(pts.iter().rev())
                    };
                    for (_, p) in it {
                        ring.push(intern_vert(&mut verts, &mut intern, *p));
                    }
                }
            }
        } else {
            let n = f.outer_loop.len();
            for i in 0..n {
                let e_idx = f.outer_loop[i];
                let edge = &brep.edges()[e_idx as usize];
                let next = &brep.edges()[f.outer_loop[(i + 1) % n] as usize];
                if edge.end != next.start {
                    probe("build-mesh-noncontinuous", &format!("f={f_idx} i={i}"));
                    return Err(BuildErr::Unsupported);
                }
                ring.push(edge.start);
                let (lo, hi) = (edge.start.min(edge.end), edge.start.max(edge.end));
                if let Some(pts) = splits.get(&(lo, hi)) {
                    // Stored params run lo→hi; traversal runs start→end.
                    let forward = edge.start == lo;
                    let it: Box<dyn Iterator<Item = &(RBig, Point3)>> = if forward {
                        Box::new(pts.iter())
                    } else {
                        Box::new(pts.iter().rev())
                    };
                    for (_, p) in it {
                        ring.push(intern_vert(&mut verts, &mut intern, *p));
                    }
                }
            }
        }
        let ring_tris =
            triangulate_ring(&ring, &mut verts, normal.as_array()).ok_or_else(|| {
                probe(
                    "build-mesh-triangulate",
                    &format!("f={f_idx} ring_len={}", ring.len()),
                );
                if std::env::var_os("YANG_RING_PROBE").is_some() {
                    eprintln!(
                        "[ring-probe] f={f_idx} normal={:?} ring={:?} pts={:?}",
                        normal.as_array(),
                        ring,
                        ring.iter()
                            .map(|&vi| verts[vi as usize].as_array())
                            .collect::<Vec<_>>()
                    );
                }
                BuildErr::Unsupported
            })?;
        tris.extend(ring_tris);
        tri_face.resize(tris.len(), f_idx as u32);
    }

    debug_assert_eq!(tri_face.len(), tris.len(), "tri_face 1:1 with stage0 tris");

    // Compact unreferenced vertices (spec `m8_stage0_inputcheck_clean_emission`
    // E8 tail): an M-B-dropped sliver can orphan a vertex that only its
    // degenerate image referenced, and the reference `mesh_booleans_inputcheck`
    // binary CRASHES on unreferenced vertices (measured, cinolib segfault).
    // Order-preserving remap; a no-op (identity) when every vertex is used.
    let mut used = vec![false; verts.len()];
    for t in &tris {
        for &v in t {
            used[v as usize] = true;
        }
    }
    if used.iter().any(|&u| !u) {
        let mut remap = vec![u32::MAX; verts.len()];
        let mut compact: Vec<Point3> = Vec::with_capacity(verts.len());
        for (i, (&u, p)) in used.iter().zip(&verts).enumerate() {
            if u {
                remap[i] = compact.len() as u32;
                compact.push(*p);
            }
        }
        for t in &mut tris {
            for v in t.iter_mut() {
                *v = remap[*v as usize];
            }
        }
        verts = compact;
    }

    Ok((Mesh::new(verts, tris), tri_face))
}

/// Get-or-append a mesh vertex by bit-exact coordinates.
pub(crate) fn intern_vert(
    verts: &mut Vec<Point3>,
    intern: &mut BTreeMap<[u64; 3], u32>,
    p: Point3,
) -> u32 {
    let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    *intern.entry(key).or_insert_with(|| {
        verts.push(p);
        (verts.len() - 1) as u32
    })
}

/// Triangulate a subdivided face ring as a FAN from one ring vertex,
/// chosen so every fan triangle has STRICTLY positive exact area, oriented
/// so the emitted triangles' normals follow the face's stored outward
/// `normal`. Returns mesh-vertex index triples; `None` when no vertex
/// admits a strictly-positive exact-coverage fan (unsupported residue).
///
/// Why a verified apex-fan and NOT a *generic* ear-clip: the split points
/// on a subdivided edge are only NEARLY collinear with its corners in 3D
/// (the shared-plane lift `o + u·e1 + v·e2` cannot realize exact 2D
/// collinearity through f64 rounding on an oblique plane — the chain is
/// femto-crooked). A generic ear-clip (the kernel-v2 style that DROPS
/// exactly-collinear corners) is free to clip a long ear whose closing
/// diagonal SPANS the crooked chain, leaving a femto-sliver polygon
/// between the diagonal and the chain; those sliver triangles then
/// femto-interpenetrate the overlay face across the hinge and the
/// arrangement faithfully builds unclassifiable sliver patches
/// (`NoExplicitRayOrigin` — the original PR-YR24 failure mode,
/// reintroduced by the re-tessellation). A fan from a corner OFF the chain
/// keeps every crooked sub-segment as a real triangle boundary, so the
/// neighbor and the overlay face stay edge-conforming and no diagonal
/// sliver can exist. The strict-positivity verification is exact
/// (rationals over the dominant-frame projection); a candidate that fails
/// (e.g. a corner whose own edge carries splits — collinear or reflex fan
/// triangles) is skipped deterministically.
///
/// For a REFLEX (non-star) subdivided ring, where neither fan exists, the
/// B3 fallback (spec `m8_nonstar_ring_earclip`) is a CLOSED-containment
/// exact ear-clip: an ear is clippable only when its closed exact triangle
/// contains NO other ring vertex, so a diagonal can never chord over a
/// split point (every sub-segment remains a triangle edge — the same
/// edge-conformality the fans guarantee), collinear split points are never
/// clipped (strict positivity) and never skipped (closed containment
/// blocks any ear that touches them). Coverage certificate as above.
pub(crate) fn triangulate_ring(
    ring: &[u32],
    verts: &mut Vec<Point3>,
    normal: [f64; 3],
) -> Option<Vec<[u32; 3]>> {
    // B6 (spec `m8_nonstar_ring_earclip` amendment): collapse CONSECUTIVE
    // bit-identical duplicate indices (and a duplicated first==last closure)
    // before strategy selection. A real corpus ring can carry a split point
    // interned to the SAME mesh vertex as a ring corner (R0046's
    // [.., 14, 14, ..]) — a zero-length ring edge with no geometry; the
    // vertex survives via its other copy, so nothing is chorded over. Exact
    // index identity only — NEVER a tolerance weld: femto-NEAR-duplicate
    // DISTINCT vertices (the 1-ulp split-point-identity residue) stay in
    // the ring and stall loudly at B4 (see spec "Measured residue").
    let dedup: Vec<u32> = {
        let mut d: Vec<u32> = Vec::with_capacity(ring.len());
        for &v in ring {
            if d.last() != Some(&v) {
                d.push(v);
            }
        }
        while d.len() > 1 && d.first() == d.last() {
            d.pop();
        }
        d
    };
    let ring: &[u32] = &dedup;
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let nu = normalize3(normal);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(nu[0], nu[1], nu[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    // Exact frame projection (spec `m8_ring_exact_projection`): evaluate
    // `u = p·e1`, `v = p·e2` over rationals. A rounded f64 dot product
    // aliases consecutive femto-twin ring vertices (distinct exact 3D
    // points from femto-tied overlay event columns) onto ONE bit-identical
    // 2D point — a zero-length exact edge that stalls every fan/ear
    // strategy below (I-EP1). The basis stays the fixed f64 pair — any
    // nondegenerate fixed frame gives a faithful projection, and every
    // orientation/coverage decision below is made inside this one frame.
    let exact_e1 = [rat(e1[0]).ok()?, rat(e1[1]).ok()?, rat(e1[2]).ok()?];
    let exact_e2 = [rat(e2[0]).ok()?, rat(e2[1]).ok()?, rat(e2[2]).ok()?];
    let pts: Vec<ExactPoint2> = ring
        .iter()
        .map(|&vi| {
            let p = verts[vi as usize].as_array();
            let (px, py, pz) = (rat(p[0]).ok()?, rat(p[1]).ok()?, rat(p[2]).ok()?);
            Some(ExactPoint2 {
                x: &px * &exact_e1[0] + &py * &exact_e1[1] + &pz * &exact_e1[2],
                y: &px * &exact_e2[0] + &py * &exact_e2[1] + &pz * &exact_e2[2],
            })
        })
        .collect::<Option<_>>()?;

    // Ring orientation: exact shoelace sign. CCW in (e1, e2) ⇒ triangle
    // normals along e1 × e2 = n̂ = the face's outward normal.
    let mut area2 = RBig::ZERO;
    for i in 1..n - 1 {
        area2 += cross_r(&pts[0], &pts[i], &pts[i + 1]);
    }
    if area2 == RBig::ZERO {
        return None;
    }
    let order: Vec<usize> = if area2 > RBig::ZERO {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };

    // Apex selection: ANY ring vertex (corner or split point) qualifies as
    // the fan apex iff EVERY fan triangle (apex, r_i, r_{i+1}) over the
    // remaining consecutive boundary pairs has STRICTLY positive exact
    // area. Strictness is load-bearing: a zero-area pair means the apex is
    // collinear with a subdivided boundary chain, and emitting (or
    // skipping) that degenerate triangle would span the chain with a chord
    // that SKIPS its split points — a T-junction the exact arrangement then
    // "repairs" with duplicate geometric vertices and sliver patches. The
    // exact coverage certificate (Σ fan areas == ring area) is the P9 gate
    // that the accepted fan partitions the ring exactly (an overlapping
    // fan over a non-star-shaped ring would over-count). A corner of a
    // doubly-subdivided convex face is never a valid apex, but an interior
    // split point of one of its edges is — so candidates include splits.
    let area_abs = if area2 > RBig::ZERO {
        area2.clone()
    } else {
        -area2.clone()
    };
    'apex: for k in 0..n {
        let apex = order[k];
        let mut tris: Vec<[u32; 3]> = Vec::with_capacity(n - 2);
        let mut covered = RBig::ZERO;
        for j in 0..n {
            let (i0, i1) = (order[(k + 1 + j) % n], order[(k + 2 + j) % n]);
            if i1 == apex || i0 == apex {
                break;
            }
            let c = cross_r(&pts[apex], &pts[i0], &pts[i1]);
            if c <= RBig::ZERO {
                continue 'apex; // collinear/reflex fan triangle — next apex
            }
            covered += c;
            tris.push([ring[apex], ring[i0], ring[i1]]);
        }
        if covered == area_abs {
            return Some(tris);
        }
    }

    // INTERIOR-CENTROID FAN (fallback). A convex face subdivided on ≥2 opposite
    // edges has NO valid boundary-vertex apex (every vertex is collinear with a
    // split on one of its edges). Its exact 2D centroid, however, sees every
    // boundary sub-segment at strictly positive area for a STAR-SHAPED face
    // (every convex face qualifies). Each sub-segment (incl. split points) stays
    // a triangle BASE — no chain-spanning chord, so no T-junction / sliver
    // (the same safety the apex-fan provides). Adds ONE interior vertex
    // (interior to this face, shared with no neighbor). If the face is not
    // star-shaped about its centroid (a genuinely non-convex re-tess face), the
    // exact coverage certificate fails → `None` (unsupported, unchanged).
    'centroid: {
        let nr = RBig::from(n as u64);
        let cx = pts.iter().fold(RBig::ZERO, |a, p| a + &p.x) / &nr;
        let cy = pts.iter().fold(RBig::ZERO, |a, p| a + &p.y) / &nr;
        let centroid = ExactPoint2 { x: cx, y: cy };
        let mut tris: Vec<[u32; 3]> = Vec::with_capacity(n);
        let mut covered = RBig::ZERO;
        // 3D interior point: the on-plane average of the ring's 3D vertices.
        let mut acc = [0.0_f64; 3];
        for &vi in ring {
            let p = verts[vi as usize].as_array();
            acc[0] += p[0];
            acc[1] += p[1];
            acc[2] += p[2];
        }
        let inv = 1.0 / n as f64;
        let cpt = Point3::new(acc[0] * inv, acc[1] * inv, acc[2] * inv);
        let c_idx = verts.len() as u32;
        for j in 0..n {
            let (i0, i1) = (order[j], order[(j + 1) % n]);
            let c = cross_r(&centroid, &pts[i0], &pts[i1]);
            if c <= RBig::ZERO {
                break 'centroid; // not star-shaped about its centroid → B3
            }
            covered += c;
            tris.push([c_idx, ring[i0], ring[i1]]);
        }
        if covered == area_abs {
            verts.push(cpt);
            return Some(tris);
        }
    }

    // ── EXACT EAR-CLIP (B3 fallback, spec `m8_nonstar_ring_earclip`) ────
    // A reflex (non-star) subdivided ring has neither a boundary apex nor a
    // centroid that sees every sub-segment. Clip strictly-convex ears whose
    // CLOSED exact triangle contains no other ring vertex: rejecting an ear
    // that touches ANY vertex (interior or boundary) forbids chording over
    // a split point (I1 — every sub-segment stays a triangle edge, the same
    // edge-conformality the fans guarantee), and strict positivity never
    // clips a collinear split point as an ear (I2). The exact coverage
    // certificate Σ clip areas == ring area is the P9 gate (I3); a stall
    // (no clippable ear — e.g. a candidate diagonal passing exactly through
    // a split point everywhere) stays the loud `None` wall (B4). No new
    // vertex is minted (I4). Deterministic first-clippable-ear scan (I6).
    // Two-ears theorem (Meisters 1975); the closed-containment exact analog
    // of kernel-v2's `ear_clip` [#39 Livesu et al. 2021 family].
    let mut work: Vec<usize> = order;
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(n - 2);
    let mut covered = RBig::ZERO;
    'clip: while work.len() > 3 {
        let m = work.len();
        for i in 0..m {
            let (ip, ic, inx) = (work[(i + m - 1) % m], work[i], work[(i + 1) % m]);
            let c = cross_r(&pts[ip], &pts[ic], &pts[inx]);
            if c <= RBig::ZERO {
                continue; // reflex or collinear (a split point) — not an ear
            }
            let blocked = work.iter().any(|&j| {
                j != ip
                    && j != ic
                    && j != inx
                    && cross_r(&pts[ip], &pts[ic], &pts[j]) >= RBig::ZERO
                    && cross_r(&pts[ic], &pts[inx], &pts[j]) >= RBig::ZERO
                    && cross_r(&pts[inx], &pts[ip], &pts[j]) >= RBig::ZERO
            });
            if blocked {
                continue;
            }
            covered += c;
            tris.push([ring[ip], ring[ic], ring[inx]]);
            work.remove(i);
            continue 'clip;
        }
        return None; // B4: no clippable ear — the loud wall persists
    }
    let fin = cross_r(&pts[work[0]], &pts[work[1]], &pts[work[2]]);
    if fin <= RBig::ZERO {
        return None;
    }
    covered += fin;
    tris.push([ring[work[0]], ring[work[1]], ring[work[2]]]);
    (covered == area_abs).then_some(tris)
}

#[cfg(test)]
mod rim_table_fusion_tests {
    //! Amendment 18 unit oracles (spec §16b): congruent-rim cross-table
    //! election. An ulp pair fuses to the lexicographically-smaller 3D bit
    //! pattern; an E-C1b-scale (1e-9) near pair does NOT; application
    //! rewrites the losing table key, the losing polygon corner, and chains
    //! the cluster pre→post map.

    use super::{apply_rim_table_fusion, detect_rim_table_fusions};
    use crate::coplanar_overlay::{ExactPoint2, PolygonWithHoles};
    use cad_primitives::{Point2, Point3};
    use std::collections::BTreeMap;

    fn ep(u: f64, v: f64) -> ExactPoint2 {
        ExactPoint2::from_f64(u, v).unwrap()
    }

    fn ulp_up(x: f64) -> f64 {
        f64::from_bits(x.to_bits() + 1)
    }

    #[test]
    fn ulp_pair_fuses_to_smaller_bits_far_and_band_scale_pairs_do_not() {
        let mut rim_a: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        let mut rim_b: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        // The fusing ulp pair (a's value has the smaller bit pattern).
        rim_a.insert(ep(1.0, 2.0), Point3::new(1.0, 2.0, 0.5));
        rim_b.insert(ep(ulp_up(1.0), 2.0), Point3::new(ulp_up(1.0), 2.0, 0.5));
        // An E-C1b-scale near pair (1e-9 — genuinely distinct, protected).
        rim_a.insert(ep(3.0, 1.0), Point3::new(3.0, 1.0, 0.5));
        rim_b.insert(ep(3.0 + 1.0e-9, 1.0), Point3::new(3.0 + 1.0e-9, 1.0, 0.5));
        // A far singleton.
        rim_b.insert(ep(5.0, 5.0), Point3::new(5.0, 5.0, 0.5));

        let fusions = detect_rim_table_fusions(&rim_a, &rim_b);
        assert_eq!(fusions.len(), 1, "exactly the ulp pair fuses");
        let f = &fusions[0];
        assert!(f.losing_is_b, "a's smaller bit pattern wins");
        assert_eq!(f.v, Point3::new(1.0, 2.0, 0.5));
        assert_eq!(f.lose_pt, Point3::new(ulp_up(1.0), 2.0, 0.5));
    }

    #[test]
    fn apply_rewrites_table_polygon_and_cluster_map() {
        let mut rim_a: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        let mut rim_b: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        rim_a.insert(ep(1.0, 2.0), Point3::new(1.0, 2.0, 0.5));
        rim_b.insert(ep(ulp_up(1.0), 2.0), Point3::new(ulp_up(1.0), 2.0, 0.5));
        let mut poly_a = PolygonWithHoles {
            outer: vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 2.0),
                Point2::new(0.0, 4.0),
            ],
            holes: vec![],
        };
        let mut poly_b = PolygonWithHoles {
            outer: vec![
                Point2::new(0.5, 0.0),
                Point2::new(ulp_up(1.0), 2.0),
                Point2::new(0.5, 4.0),
            ],
            holes: vec![],
        };
        let mut key_map: BTreeMap<(u64, u64), (u64, u64)> = BTreeMap::new();
        // A pre-cluster image already landing on the losing uv must chain.
        let pre = (9.0f64.to_bits(), 9.0f64.to_bits());
        key_map.insert(pre, (ulp_up(1.0).to_bits(), 2.0f64.to_bits()));

        let fusions = detect_rim_table_fusions(&rim_a, &rim_b);
        assert_eq!(fusions.len(), 1);
        apply_rim_table_fusion(
            &fusions[0],
            &mut rim_a,
            &mut rim_b,
            &mut poly_a,
            &mut poly_b,
            &mut key_map,
        );

        assert_eq!(
            rim_b.get(&ep(1.0, 2.0)),
            Some(&Point3::new(1.0, 2.0, 0.5)),
            "losing table re-keyed to the elected (uv, point)"
        );
        assert!(!rim_b.contains_key(&ep(ulp_up(1.0), 2.0)));
        assert_eq!(
            (poly_b.outer[1].x(), poly_b.outer[1].y()),
            (1.0, 2.0),
            "losing polygon corner rewritten to the winning uv"
        );
        assert_eq!(
            key_map.get(&pre),
            Some(&(1.0f64.to_bits(), 2.0f64.to_bits())),
            "existing cluster image chained onto the winning uv"
        );
        assert_eq!(
            key_map.get(&(ulp_up(1.0).to_bits(), 2.0f64.to_bits())),
            Some(&(1.0f64.to_bits(), 2.0f64.to_bits())),
            "losing uv itself remapped"
        );
    }
}

#[cfg(test)]
mod lift_absorb_band_tests {
    /// Amendment 17 (spec §15b): the lift-absorption band is the
    /// rounding-noise class `TAU_WORK·(1+uv_scale)`. It must ADMIT the
    /// measured F0067 femto cluster (uv spread 4.3e-14 at uv scale ≈ 0.204)
    /// and REJECT the protected E-C1b genuinely-distinct band-close twin
    /// population (~1e-9, R0088/R0070 — both members must enter the ring).
    /// A future band widening that swallows the twin population regresses
    /// those cases; this pin is the tripwire.
    #[test]
    fn band_admits_cluster_rejects_distinct_twins() {
        let uv_scale = 0.2043166720325753_f64; // the measured F0067 site
        let band = cad_primitives::TAU_WORK * (1.0 + uv_scale);
        assert!(
            4.3e-14 < band,
            "measured cluster spread must fall inside the absorption band"
        );
        assert!(
            1.0e-9 > band,
            "the E-C1b distinct-twin population must stay OUTSIDE the band"
        );
    }

    /// Amendment 19 (spec §17): the absorption predicate must reach a
    /// SINGLETON cluster — one mint plus its non-minted lifts — which is the
    /// F0067 crack-field class the §15 multi-mint group filter could never see.
    /// Pins all four exclusion arms at the same time: a minted vertex, an
    /// existing group member, a rim anchor and a corner are each left alone,
    /// while an out-of-band vertex at the E-C1b distance stays out.
    #[test]
    fn singleton_cluster_absorbs_its_lifts_and_respects_every_exclusion() {
        use crate::coplanar_overlay::{ClassifiedOverlay, ExactPoint2};
        use crate::stage0::{absorbable_sub_band_lifts, rim_chords::CollapseGroups};
        use cad_primitives::{Point2, Point3};
        use std::collections::BTreeMap;

        // One geometric crossing at uv scale ≈ 0.2, sampled seven ways.
        let (ux, uy) = (0.2027775970276196_f64, -0.0121599983069508_f64);
        let d = 3.0e-17; // the measured 1-ulp cluster spread
        let pts = [
            (ux, uy),          // 0 the mint (elected)
            (ux + d, uy),      // 1 lift  — absorbable
            (ux, uy + d),      // 2 lift  — absorbable
            (ux - d, uy - d),  // 3 minted elsewhere — excluded
            (ux + d, uy + d),  // 4 already a group member — excluded
            (ux - d, uy),      // 5 rim anchor — excluded by design
            (ux, uy - d),      // 6 corner — excluded by design
            (ux + 1.0e-9, uy), // 7 E-C1b distinct twin — out of band
        ];
        let verts: Vec<Point2> = pts.iter().map(|&(x, y)| Point2::new(x, y)).collect();
        let exact_verts: Vec<ExactPoint2> = pts
            .iter()
            .map(|&(x, y)| ExactPoint2::from_f64(x, y).expect("finite"))
            .collect();
        let overlay = ClassifiedOverlay {
            verts,
            exact_verts,
            tris: Vec::new(),
            class: Vec::new(),
            poly_a: Vec::new(),
            poly_b: Vec::new(),
            fused: BTreeMap::new(),
        };
        let mut minted_mark = vec![false; pts.len()];
        minted_mark[0] = true;
        minted_mark[3] = true;
        let mut groups = CollapseGroups::default();
        groups.members.insert(4, vec![4]);
        let mut corners: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
        corners.insert(overlay.exact_verts[6].clone(), 0);
        let mut rims: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        rims.insert(overlay.exact_verts[5].clone(), Point3::new(0.0, 0.0, 0.0));
        let empty_c: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
        let empty_r: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();

        let got = absorbable_sub_band_lifts(
            &overlay,
            0,
            &[0],
            &minted_mark,
            &groups,
            &corners,
            &empty_c,
            &rims,
            &empty_r,
        );
        assert_eq!(
            got,
            vec![1, 2],
            "only the two non-minted, non-anchored, in-band lifts absorb"
        );
    }
}

#[cfg(test)]
mod fan_split_tests {
    use super::fan_split_tri;

    #[test]
    fn one_point_splits_edge0() {
        // Split edge (tri[0],tri[1]) of [0,1,2] with interior point 3; fan from
        // the opposite vertex 2.
        assert_eq!(
            fan_split_tri([0, 1, 2], 0, &[3]),
            vec![[2, 0, 3], [2, 3, 1]]
        );
    }

    #[test]
    fn two_points_splits_edge0() {
        assert_eq!(
            fan_split_tri([0, 1, 2], 0, &[3, 4]),
            vec![[2, 0, 3], [2, 3, 4], [2, 4, 1]]
        );
    }

    #[test]
    fn splits_edge1_and_edge2() {
        // edge (tri[1],tri[2]) → opposite vertex tri[0].
        assert_eq!(
            fan_split_tri([0, 1, 2], 1, &[3]),
            vec![[0, 1, 3], [0, 3, 2]]
        );
        // edge (tri[2],tri[0]) → opposite vertex tri[1].
        assert_eq!(
            fan_split_tri([0, 1, 2], 2, &[3]),
            vec![[1, 2, 3], [1, 3, 0]]
        );
    }

    #[test]
    fn empty_interior_is_the_original_rotated() {
        // No interior points → one triangle, same winding as the input.
        assert_eq!(fan_split_tri([0, 1, 2], 0, &[]), vec![[2, 0, 1]]);
    }

    #[test]
    fn winding_is_preserved() {
        // CCW triangle (0,0),(2,0),(1,1); split the bottom edge at its midpoint
        // (index 3 = (1,0)). Every output triangle must stay CCW (area > 0).
        let coords = [[0.0, 0.0], [2.0, 0.0], [1.0, 1.0], [1.0, 0.0]];
        let area2 = |t: [u32; 3]| {
            let p = |i: u32| coords[i as usize];
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        assert!(area2([0, 1, 2]) > 0.0, "input is CCW");
        for t in fan_split_tri([0, 1, 2], 0, &[3]) {
            assert!(area2(t) > 0.0, "split triangle {t:?} must stay CCW");
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// M8-earclip: exact ear-clip fallback for non-star subdivided rings
// (spec `specs/m8_nonstar_ring_earclip.md`, FIP Phase 2, RED).
//
// `triangulate_ring` is module-private, so these unit tests call it directly
// through this in-module test seam. Fixtures are pure geometry: a ring of
// `Vec<u32>` indices into a `Vec<Point3>` plus a plane normal.
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod earclip_ring_tests {
    use super::*;

    /// Project the (already planar) `verts` onto the SAME dominant 2D frame
    /// `triangulate_ring` uses, so the oracle's exact areas live in the same
    /// coordinate system the function's coverage certificate does.
    fn project(verts: &[Point3], normal: [f64; 3]) -> Vec<ExactPoint2> {
        let nu = normalize3(normal);
        let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(nu[0], nu[1], nu[2]));
        let (e1, e2) = (e1.as_array(), e2.as_array());
        verts
            .iter()
            .map(|p| {
                let a = p.as_array();
                let u = a[0] * e1[0] + a[1] * e1[1] + a[2] * e1[2];
                let v = a[0] * e2[0] + a[1] * e2[1] + a[2] * e2[2];
                ExactPoint2::from_f64(u, v).expect("finite projection")
            })
            .collect()
    }

    /// Exact 2× signed area of the ring (shoelace over its boundary order).
    fn ring_area2(pts: &[ExactPoint2], ring: &[u32]) -> RBig {
        let n = ring.len();
        let mut a = RBig::ZERO;
        for i in 1..n - 1 {
            a += cross_r(
                &pts[ring[0] as usize],
                &pts[ring[i] as usize],
                &pts[ring[i + 1] as usize],
            );
        }
        a
    }

    fn p3(x: f64, y: f64) -> Point3 {
        Point3::new(x, y, 0.0)
    }

    /// The full B3 oracle bundle (I1–I4) for a successful ear-clip.
    ///
    /// `call_ring` is passed to `triangulate_ring` verbatim (it may carry B6
    /// consecutive-duplicate / closure-duplicate indices); `oracle_ring` is the
    /// deduplicated ring the invariants are evaluated against (I1 boundary
    /// tiling, I3 exact area, triangle count). For the plain B3 case the two are
    /// identical.
    fn assert_earclip_invariants(
        verts_before: &[Point3],
        call_ring: &[u32],
        oracle_ring: &[u32],
        normal: [f64; 3],
    ) {
        let mut verts = verts_before.to_vec();
        let n_before = verts.len();
        let tris = triangulate_ring(call_ring, &mut verts, normal)
            .expect("B3: reflex subdivided ring must triangulate via exact ear-clip");

        // I4 (no new vertices): the ear-clip adds no centroid (unlike B2).
        assert_eq!(
            verts.len(),
            n_before,
            "I4: ear-clip must not push any new vertex"
        );
        let ring_set: std::collections::BTreeSet<u32> = oracle_ring.iter().copied().collect();
        for t in &tris {
            for &vi in t {
                assert!(
                    ring_set.contains(&vi),
                    "I4: triangle references index {vi} outside the ring"
                );
            }
        }

        // n−2 triangles for a simple polygon with no interior vertex.
        assert_eq!(
            tris.len(),
            oracle_ring.len() - 2,
            "a hole-free ring triangulates into ring.len()−2 triangles"
        );

        let pts = project(&verts, normal);
        let area = ring_area2(&pts, oracle_ring);
        assert!(area != RBig::ZERO, "fixture defect: zero-area ring");
        let ring_positive = area > RBig::ZERO;

        // I2 (strict positivity, ring orientation) + I3 (exact coverage).
        let mut covered = RBig::ZERO;
        for t in &tris {
            let c = cross_r(
                &pts[t[0] as usize],
                &pts[t[1] as usize],
                &pts[t[2] as usize],
            );
            assert!(c != RBig::ZERO, "I2: triangle {t:?} has zero exact area");
            assert_eq!(
                c > RBig::ZERO,
                ring_positive,
                "I2: triangle {t:?} is not strictly positive in the ring's orientation frame"
            );
            covered += c;
        }
        assert_eq!(
            covered, area,
            "I3: Σ clip areas must equal the exact ring area (coverage certificate)"
        );

        // I1 (no chord over a split point): every consecutive ring boundary
        // pair is an edge of EXACTLY one output triangle. Interior diagonals
        // appear in two triangles; boundary segments in one.
        let mut edge_count: std::collections::BTreeMap<(u32, u32), usize> =
            std::collections::BTreeMap::new();
        let undirected = |a: u32, b: u32| if a <= b { (a, b) } else { (b, a) };
        for t in &tris {
            for k in 0..3 {
                *edge_count
                    .entry(undirected(t[k], t[(k + 1) % 3]))
                    .or_default() += 1;
            }
        }
        let n = oracle_ring.len();
        for i in 0..n {
            let e = undirected(oracle_ring[i], oracle_ring[(i + 1) % n]);
            assert_eq!(
                edge_count.get(&e).copied().unwrap_or(0),
                1,
                "I1: boundary segment {e:?} must be an edge of exactly one triangle \
                 (no chord skipping a split point)"
            );
        }
    }

    /// B3 (RED): a deep L-shaped (reflex) ring, subdivided by split points on
    /// three edges (collinear runs of three), is NOT star-shaped — neither the
    /// boundary-vertex apex fan (B1) nor the interior-centroid fan (B2) can
    /// triangulate it. 9 vertices (the R0046 ring-9 signature).
    ///
    /// RED today: `triangulate_ring` returns `None` for this ring (both fans
    /// fail), so `assert_earclip_invariants`'s `.expect(..)` on `Some` fails.
    #[test]
    fn reflex_l_ring_with_collinear_splits_earclips() {
        // Deep L: bottom rect [0,6]×[0,1] ∪ left rect [0,1]×[0,6], reflex at
        // (1,1). Vertex centroid ≈ (2.0, 2.28) lies OUTSIDE the L, so the
        // centroid fan cannot see the boundary — genuinely non-star.
        let verts = vec![
            p3(0.0, 0.0),
            p3(3.0, 0.0), // split — bottom edge collinear run (0,0)-(3,0)-(6,0)
            p3(6.0, 0.0),
            p3(6.0, 1.0),
            p3(1.0, 1.0), // reflex corner
            p3(1.0, 3.5), // split — inner vertical run (1,1)-(1,3.5)-(1,6)
            p3(1.0, 6.0),
            p3(0.0, 6.0),
            p3(0.0, 3.0), // split — left edge collinear run (0,6)-(0,3)-(0,0)
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        assert_earclip_invariants(&verts, &ring, &ring, [0.0, 0.0, 1.0]);
    }

    /// B6 (spec amendment, RED on parent): the real corpus rings carry
    /// CONSECUTIVE bit-identical duplicate indices (a split point interned to
    /// the same mesh vertex as a neighbor → a zero-length ring edge) and a
    /// first==last closure duplicate — e.g. R0046's ring
    /// `[2,1,5,27,23,19,14,14,4]` (vertex 14 twice). These must be collapsed by
    /// EXACT index equality BEFORE strategy selection; the deduplicated ring
    /// then ear-clips exactly like the plain B3 case.
    ///
    /// Fixture: the reflex-L ring with split-point index 1 duplicated in place
    /// AND index 0 appended as a closure duplicate. The oracle bundle runs
    /// against the DEDUPED ring (`0..9`); `verts.len()` is unchanged (the
    /// duplicated vertex survives via its surviving copy — no point is chorded
    /// over, no vertex is added).
    ///
    /// RED on parent `69f3c8a8`: `triangulate_ring` there has neither dedup nor
    /// ear-clip, so a reflex ring (deduped or not) returns `None` and the
    /// `.expect(Some)` fails — the identical failure the plain B3 test showed on
    /// that parent.
    #[test]
    fn b6_consecutive_duplicate_indices_collapse() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(3.0, 0.0), // split — bottom edge collinear run (0,0)-(3,0)-(6,0)
            p3(6.0, 0.0),
            p3(6.0, 1.0),
            p3(1.0, 1.0), // reflex corner
            p3(1.0, 3.5), // split — inner vertical run (1,1)-(1,3.5)-(1,6)
            p3(1.0, 6.0),
            p3(0.0, 6.0),
            p3(0.0, 3.0), // split — left edge collinear run (0,6)-(0,3)-(0,0)
        ];
        // Split-point index 1 duplicated consecutively (zero-length edge) and a
        // closure duplicate (first index 0 appended at the end).
        let call_ring: Vec<u32> = vec![0, 1, 1, 2, 3, 4, 5, 6, 7, 8, 0];
        // Exact collapse of consecutive duplicates + first==last closure.
        let oracle_ring: Vec<u32> = (0..verts.len() as u32).collect();
        assert_earclip_invariants(&verts, &call_ring, &oracle_ring, [0.0, 0.0, 1.0]);
    }

    /// I5 guard (B1): a convex ring with one edge split still succeeds via the
    /// boundary-vertex apex fan and adds NO vertex. Pins the fast-path count so
    /// a regression that reroutes convex rings through the ear-clip (or the
    /// centroid fan) is caught. CURRENT behavior verified: B1, `verts.len()`
    /// unchanged (5).
    #[test]
    fn convex_split_ring_uses_boundary_fan_guard() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(2.0, 0.0), // split on the bottom edge
            p3(4.0, 0.0),
            p3(4.0, 4.0),
            p3(0.0, 4.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        let n_before = v.len();
        let tris = triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0])
            .expect("convex subdivided ring must triangulate (B1/B2)");
        assert_eq!(
            v.len(),
            n_before,
            "I5: convex ring uses the boundary apex fan (B1) — no interior vertex added"
        );
        assert_eq!(tris.len(), ring.len() - 2, "3 triangles for a 5-gon via B1");
    }

    /// B5 guard: a self-crossing (bowtie) ring has zero exact signed area and
    /// must return `None` — today AND after the ear-clip lands (the zero-area
    /// short-circuit precedes B3, so the fix never triangulates a non-simple
    /// ring).
    #[test]
    fn bowtie_ring_returns_none_guard() {
        // Ordered so edges (4,0)-(0,4) and (4,4)-(0,0) cross; net area = 0.
        let verts = vec![p3(0.0, 0.0), p3(4.0, 0.0), p3(0.0, 4.0), p3(4.0, 4.0)];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
            "B5: a self-crossing / zero-area ring must not triangulate"
        );
        assert_eq!(v.len(), verts.len(), "no vertex pushed on the None path");
    }

    /// B5 guard: a degenerate ring (n < 3) returns `None`.
    #[test]
    fn too_few_vertices_returns_none_guard() {
        let verts = vec![p3(0.0, 0.0), p3(1.0, 0.0)];
        let ring: Vec<u32> = vec![0, 1];
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
            "B5: a ring with fewer than 3 vertices must return None"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the B3 closed-containment ear-clip + B6 dedup in
    // `triangulate_ring`, appended to this in-module test seam (the function is
    // private). Purely additive; touches no existing test. Fixture geometry was
    // localized with a throwaway probe: each attack notes whether it reaches B3
    // and, for the mutation killers, the exact production-vs-mutant divergence.

    /// Assert the function returns `Some` and the triangulation is oriented to
    /// the given `normal` (all triangles strictly positive in the (e1,e2) frame,
    /// which the function reorients to regardless of input winding), covers the
    /// ring's exact area, and tiles every deduped boundary segment exactly once
    /// (I1). Unlike `assert_earclip_invariants`, this does NOT assume the input
    /// ring's winding matches `normal` — so it can attack CW input rings.
    fn assert_oriented_triangulation(verts: &[Point3], ring: &[u32], normal: [f64; 3]) {
        let mut v = verts.to_vec();
        let n_before = v.len();
        let tris = triangulate_ring(ring, &mut v, normal).expect("must triangulate");
        assert_eq!(v.len(), n_before, "no vertex may be pushed (B3, I4)");

        // Deduped ring (mirror of the function's B6 collapse) for the oracle.
        let mut ded: Vec<u32> = Vec::new();
        for &x in ring {
            if ded.last() != Some(&x) {
                ded.push(x);
            }
        }
        while ded.len() > 1 && ded.first() == ded.last() {
            ded.pop();
        }
        assert_eq!(tris.len(), ded.len() - 2, "n−2 triangles");

        let pts = project(&v, normal);
        let area = ring_area2(&pts, &ded);
        let area_abs = if area > RBig::ZERO {
            area.clone()
        } else {
            -area.clone()
        };
        let mut covered = RBig::ZERO;
        for t in &tris {
            let c = cross_r(
                &pts[t[0] as usize],
                &pts[t[1] as usize],
                &pts[t[2] as usize],
            );
            assert!(
                c > RBig::ZERO,
                "I2: every triangle must be strictly positive in the normal's frame, got {t:?}"
            );
            covered += c;
        }
        assert_eq!(covered, area_abs, "I3: exact coverage certificate");

        let undirected = |a: u32, b: u32| if a <= b { (a, b) } else { (b, a) };
        let mut edge_count: std::collections::BTreeMap<(u32, u32), usize> =
            std::collections::BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                *edge_count
                    .entry(undirected(t[k], t[(k + 1) % 3]))
                    .or_default() += 1;
            }
        }
        let n = ded.len();
        for i in 0..n {
            let e = undirected(ded[i], ded[(i + 1) % n]);
            assert_eq!(
                edge_count.get(&e).copied().unwrap_or(0),
                1,
                "I1: boundary segment {e:?} must bound exactly one triangle"
            );
        }
    }

    /// MUTATION KILLER (a) — vertex EXACTLY on an ear diagonal. A deep U with a
    /// rectangular top notch (two reflex corners → non-star, reaches B3), plus a
    /// split at (3,1) on the notch floor and (3,0) on the base. During the clip,
    /// a convex ear's closing diagonal passes EXACTLY through split (3,1): closed
    /// containment (`>=`) rejects that ear (the vertex is on the triangle
    /// boundary) and the clip routes around it, keeping (3,1) a boundary edge (I1
    /// holds). An OPEN-containment mutant (`>` instead of `>=`) clips that ear,
    /// chording over (3,1), which strands a degenerate sub-polygon → the clip
    /// STALLS and `triangulate_ring` returns `None`.
    ///
    /// Verified: production → `Some(8)`, passes all invariants; the `>` mutant →
    /// `None` (the `.expect` fires). The existing reflex-L test does NOT exercise
    /// an on-diagonal vertex, so the mutant survives it — this fixture is the
    /// dedicated killer.
    #[test]
    fn adversary_vertex_on_ear_diagonal_forces_closed_containment() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(3.0, 0.0), // split on base (collinear (0,0)-(3,0)-(6,0))
            p3(6.0, 0.0),
            p3(6.0, 3.0),
            p3(4.0, 3.0),
            p3(4.0, 1.0), // reflex
            p3(3.0, 1.0), // split on notch floor — lands on ear diagonals
            p3(2.0, 1.0), // reflex
            p3(2.0, 3.0),
            p3(0.0, 3.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        assert_earclip_invariants(&verts, &ring, &ring, [0.0, 0.0, 1.0]);
    }

    /// B4 stall — two squares pinched at a shared corner (2,2) appearing at two
    /// DISTINCT ring indices (self-touching, net area 8, non-star). No
    /// strictly-convex empty ear survives closed containment at the pinch, so the
    /// clip STALLS: loud `None`, never a partial/overlapping triangulation, no
    /// vertex pushed. (Confirmed a genuine stall: `None` persists even with the
    /// coverage certificate removed.)
    #[test]
    fn adversary_pinched_squares_self_touch_stalls() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(2.0, 0.0),
            p3(2.0, 2.0),
            p3(4.0, 2.0),
            p3(4.0, 4.0),
            p3(2.0, 4.0),
            p3(2.0, 2.0), // same coord as index 2, distinct index
            p3(0.0, 2.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
            "B4: a self-touching (pinched) ring must stall loudly"
        );
        assert_eq!(v.len(), verts.len(), "no vertex pushed on the stall path");
    }

    /// B4 stall — a rectangle with an inward spike whose tip (2,0) lies EXACTLY
    /// on the opposite base edge (0,0)-(4,0) (weakly simple, net area 24). The
    /// spike tip touches a non-adjacent edge, so no valid ear survives closed
    /// containment → loud `None`.
    #[test]
    fn adversary_spike_tip_on_opposite_edge_stalls() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(4.0, 0.0),
            p3(4.0, 4.0),
            p3(2.0, 4.0),
            p3(2.0, 0.0), // spike tip on the base edge (0,0)-(4,0)
            p3(0.0, 4.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
            "B4: a spike tip resting on an opposite edge must stall loudly"
        );
    }

    /// I3 coverage on a completing B3 clip — a self-overlapping (winding-2) ring:
    /// an outer CCW triangle and a smaller CCW triangle traced inside it, joined
    /// at v0. It reaches B3 and CLIPS TO COMPLETION with exact coverage
    /// (`Σ = shoelace`, which itself double-counts the winding-2 overlap, so they
    /// agree). Documents that when the closed-containment clip completes,
    /// coverage holds — see the mutation-(b) finding.
    #[test]
    fn adversary_self_overlap_winding2_has_exact_coverage() {
        let verts = vec![
            p3(0.0, 0.0),
            p3(6.0, 0.0),
            p3(3.0, 6.0),
            p3(1.0, 1.0),
            p3(5.0, 1.0),
            p3(3.0, 4.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        let tris = triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0])
            .expect("winding-2 ring reaches B3 and clips to completion");
        let pts = project(&v, [0.0, 0.0, 1.0]);
        let area = ring_area2(&pts, &ring);
        let area_abs = if area > RBig::ZERO {
            area.clone()
        } else {
            -area
        };
        let mut covered = RBig::ZERO;
        for t in &tris {
            let c = cross_r(
                &pts[t[0] as usize],
                &pts[t[1] as usize],
                &pts[t[2] as usize],
            );
            assert!(c > RBig::ZERO, "I2: non-positive triangle {t:?}");
            covered += c;
        }
        assert_eq!(
            covered, area_abs,
            "I3: coverage must hold on a completing clip"
        );
    }

    /// Orientation / I6 — a CW (clockwise) reflex U with `normal = +z`. The
    /// function detects the negative shoelace and reverses `order`, emitting
    /// triangles that follow `normal` (all strictly positive in the frame). This
    /// exercises the `order = (0..n).rev()` branch the CCW fixtures never hit.
    #[test]
    fn adversary_cw_reflex_ring_reorients_to_normal() {
        // CCW U reversed → CW winding.
        let mut verts = vec![
            p3(0.0, 0.0),
            p3(6.0, 0.0),
            p3(6.0, 3.0),
            p3(4.0, 3.0),
            p3(4.0, 1.0),
            p3(2.0, 1.0),
            p3(2.0, 3.0),
            p3(0.0, 3.0),
        ];
        verts.reverse();
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        assert_oriented_triangulation(&verts, &ring, [0.0, 0.0, 1.0]);
    }

    /// B6 / B5 — a ring that collapses to fewer than 3 DISTINCT consecutive
    /// indices returns `None` and pushes no vertex. `[3,3,3]` → dedup `[3]`;
    /// `[3,3,7,7,3]` → dedup `[3,7]` (closure `3` popped) — both < 3.
    #[test]
    fn adversary_dedup_to_fewer_than_three_returns_none() {
        let verts = vec![p3(0.0, 0.0), p3(1.0, 0.0), p3(0.5, 1.0), p3(2.0, 2.0)];
        for ring in [
            vec![3u32, 3, 3],
            vec![3u32, 3, 3, 3],
            vec![3u32, 3, 3, 3, 3],
        ] {
            let mut v = verts.clone();
            assert!(
                triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
                "all-duplicate ring {ring:?} must dedup below 3 and return None"
            );
            assert_eq!(v.len(), verts.len());
        }
        // Two distinct indices after dedup (with a closure duplicate).
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&[3u32, 3, 0, 0, 3], &mut v, [0.0, 0.0, 1.0]).is_none(),
            "ring deduping to two indices must return None"
        );
    }

    /// B5 — an all-collinear ring (every vertex on one line) has zero exact area
    /// and returns `None` before any strategy runs, never emitting a zero-area
    /// triangle.
    #[test]
    fn adversary_all_collinear_ring_returns_none() {
        let verts = vec![p3(0.0, 0.0), p3(1.0, 0.0), p3(2.0, 0.0), p3(3.0, 0.0)];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        assert!(
            triangulate_ring(&ring, &mut v, [0.0, 0.0, 1.0]).is_none(),
            "B5: a zero-area collinear ring must return None"
        );
    }

    /// Femto-thin ear (measured-residue family). A reflex L whose inner-vertical
    /// split is minted TWICE ~1 ULP apart (a femto-twin — the known §4.5.5
    /// conformality-break class). The ring zigzags at femto scale, so no
    /// strictly-positive ear adjacent to the twins survives closed containment.
    /// The contract (spec "Measured residue"): the result is EITHER a loud stall
    /// (`None`) OR a fully valid triangulation (all strictly positive + exact
    /// coverage) — NEVER a non-positive/degenerate triangle, and never a panic.
    #[test]
    fn adversary_femto_twin_ring_never_emits_degenerate() {
        let bump = |x: f64, n: u64| f64::from_bits(x.to_bits().wrapping_add(n));
        let verts = vec![
            p3(0.0, 0.0),
            p3(3.0, 0.0),
            p3(6.0, 0.0),
            p3(6.0, 1.0),
            p3(1.0, 1.0),                   // reflex
            p3(1.0, 3.5),                   // inner-vertical split
            p3(bump(1.0, 3), bump(3.5, 2)), // femto-twin ~1 ULP away
            p3(1.0, 6.0),
            p3(0.0, 6.0),
            p3(0.0, 3.0),
        ];
        let ring: Vec<u32> = (0..verts.len() as u32).collect();
        let mut v = verts.clone();
        let normal = [0.0, 0.0, 1.0];
        let result = triangulate_ring(&ring, &mut v, normal);
        if let Some(tris) = result {
            // If it DID triangulate, every triangle must still be exact-valid.
            let pts = project(&v, normal);
            let area = ring_area2(&pts, &ring);
            let area_abs = if area > RBig::ZERO {
                area.clone()
            } else {
                -area
            };
            let mut covered = RBig::ZERO;
            for t in &tris {
                let c = cross_r(
                    &pts[t[0] as usize],
                    &pts[t[1] as usize],
                    &pts[t[2] as usize],
                );
                assert!(
                    c > RBig::ZERO,
                    "I2: femto ring must never emit a non-positive triangle {t:?}"
                );
                covered += c;
            }
            assert_eq!(
                covered, area_abs,
                "I3: coverage must hold if it triangulated"
            );
        }
        // else: loud None stall — the honest measured-residue outcome.
    }
}

#[cfg(test)]
mod ring_exact_projection_tests {
    use super::*;

    /// Spec `m8_ring_exact_projection` B2 (RED 2026-07-10): the F0068
    /// corpus ring (lateral f=207) carries a consecutive 1-ULP twin pair
    /// (distinct exact 3D points from two femto-tied overlay sweep event
    /// columns). The f64 frame projection aliases the twins onto ONE
    /// bit-identical 2D point, so the exact 2D ring gets a zero-length
    /// edge and every fan/ear candidate is rejected — `None`, walling
    /// the whole coplanar pair (`build-mesh-triangulate`). With the
    /// exact rational projection the twins stay distinct and the ring
    /// triangulates like any subdivided chain.
    #[test]
    fn f0068_lateral_ring_femto_twins() {
        let mut verts = vec![
            Point3::new(
                -0.09834728650612151,
                0.22487103457140994,
                1.2552358181232062,
            ), // 207
            Point3::new(
                -0.09372779082604177,
                0.22071665685042138,
                1.2552358181232062,
            ), // 206
            Point3::new(
                -0.09372779082604177,
                0.22071665685042138,
                1.0167700011240253,
            ), // 468
            Point3::new(
                -0.09689736564471349,
                0.22356710025888266,
                1.0167700011240253,
            ), // 881
            Point3::new(-0.0968973656447135, 0.22356710025888268, 1.0167700011240253), // 878
            Point3::new(
                -0.09834728650612151,
                0.22487103457140994,
                1.0167700011240253,
            ), // 467
        ];
        let ring: Vec<u32> = vec![0, 1, 2, 3, 4, 5];
        let normal = [0.6686829267246631, 0.7435476739973965, 0.0];
        let n_before = verts.len();
        let tris = triangulate_ring(&ring, &mut verts, normal)
            .expect("femto-twin ring must triangulate under the exact projection");

        // I1 boundary tiling: every ring sub-segment is an edge of exactly
        // one emitted triangle (directed occurrence count == 1).
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let count = tris
                .iter()
                .filter(|t| {
                    (0..3).any(|k| {
                        (t[k] == a && t[(k + 1) % 3] == b) || (t[k] == b && t[(k + 1) % 3] == a)
                    })
                })
                .count();
            assert_eq!(
                count, 1,
                "ring sub-segment ({a},{b}) tiled by {count} triangles"
            );
        }

        // I2 strict exact positivity in the exact frame (independent
        // re-check of the function's own certificate).
        let nu = normalize3(normal);
        let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(nu[0], nu[1], nu[2]));
        let (e1, e2) = (e1.as_array(), e2.as_array());
        let proj = |vi: u32| -> ExactPoint2 {
            let p = verts[vi as usize].as_array();
            let dot = |e: &[f64; 3]| {
                crate::coplanar_overlay::rat(p[0]).unwrap()
                    * crate::coplanar_overlay::rat(e[0]).unwrap()
                    + crate::coplanar_overlay::rat(p[1]).unwrap()
                        * crate::coplanar_overlay::rat(e[1]).unwrap()
                    + crate::coplanar_overlay::rat(p[2]).unwrap()
                        * crate::coplanar_overlay::rat(e[2]).unwrap()
            };
            ExactPoint2 {
                x: dot(&e1),
                y: dot(&e2),
            }
        };
        let mut covered = RBig::ZERO;
        for t in &tris {
            let (a, b, c) = (proj(t[0]), proj(t[1]), proj(t[2]));
            let area2 = cross_r(&a, &b, &c);
            assert!(area2 > RBig::ZERO, "triangle {t:?} not strictly positive");
            covered += area2;
        }
        // I3 exact coverage: fan areas sum to the ring area.
        let pts: Vec<ExactPoint2> = ring.iter().map(|&vi| proj(vi)).collect();
        let mut ring_area2 = RBig::ZERO;
        for i in 1..n - 1 {
            ring_area2 += cross_r(&pts[0], &pts[i], &pts[i + 1]);
        }
        let ring_abs = if ring_area2 > RBig::ZERO {
            ring_area2
        } else {
            -ring_area2
        };
        assert_eq!(covered, ring_abs, "exact coverage certificate");
        // I4: no vertex minted beyond the optional centroid.
        assert!(verts.len() <= n_before + 1, "at most one centroid vertex");
    }
}

#[cfg(test)]
mod edge_split_merge_dedup_tests {
    //! Amendment 13 inc-3.5 (spec `m8_stage0_multiclass_cavity_arm` §10d):
    //! a SURVIVING Fig-11 merge identifies two overlay vertices with
    //! distinct exact chord parameters to one 3D point; the split
    //! collector must propagate the identification (one entry), while the
    //! historical no-merge path keeps both entries bit-for-bit (the
    //! gate-OFF byte-identity pin).

    use super::collect_edge_splits;
    use crate::coplanar_overlay::{ClassifiedOverlay, ExactPoint2, RegionClass};
    use crate::stage0::Frame;
    use crate::tests_unit::n2_junction::rj_box;
    use cad_primitives::{Point2, Point3};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn merged_twin_collapses_to_one_split_entry() {
        let b = rj_box([0.0; 3], [1.0; 3]);
        let va: Vec<Point3> = b.vertices().iter().map(|v| v.point).collect();
        // The face whose outer loop carries edge 0 (0→1 along y=0, z=0).
        let fi = b
            .faces()
            .iter()
            .position(|f| f.outer_loop.contains(&0))
            .expect("rj_box bottom face");
        let frame = Frame {
            n: [0.0, 0.0, 1.0],
            d: 0.0,
            o: [0.0, 0.0, 0.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        };
        let uv = [(0.3, 0.0), (0.5, 0.0), (0.5, 0.5)];
        let overlay = ClassifiedOverlay {
            verts: uv.iter().map(|&(u, v)| Point2::new(u, v)).collect(),
            exact_verts: uv
                .iter()
                .map(|&(u, v)| ExactPoint2::from_f64(u, v).unwrap())
                .collect(),
            tris: vec![[0, 1, 2]],
            class: vec![RegionClass::AOnly],
            poly_a: vec![0],
            poly_b: vec![u32::MAX],
            fused: BTreeMap::new(),
        };
        // Both edge vertices resolve to ONE junction point (a survived
        // position merge); the apex resolves elsewhere.
        let junction = Point3::new(0.4, 0.0, 0.0);
        let resolved = vec![junction, junction, Point3::new(0.5, 0.5, 0.0)];
        let classes = [RegionClass::AOnly, RegionClass::Overlap];

        // Historical path (no merge): BOTH entries, distinct parameters.
        let mut splits = BTreeMap::new();
        collect_edge_splits(
            &b,
            fi,
            &va,
            &frame,
            &BTreeMap::new(),
            &overlay,
            classes,
            &resolved,
            &BTreeSet::new(),
            &mut splits,
        );
        let entry = splits.get(&(0, 1)).expect("edge (0,1) split");
        assert_eq!(entry.len(), 2, "no-merge path keeps both entries");
        assert!(entry.iter().all(|(_, p)| *p == junction));

        // Merge-aware path: the identification propagates — ONE entry.
        let merged: BTreeSet<[u64; 3]> = [[
            junction.x().to_bits(),
            junction.y().to_bits(),
            junction.z().to_bits(),
        ]]
        .into_iter()
        .collect();
        let mut splits2 = BTreeMap::new();
        collect_edge_splits(
            &b,
            fi,
            &va,
            &frame,
            &BTreeMap::new(),
            &overlay,
            classes,
            &resolved,
            &merged,
            &mut splits2,
        );
        let entry2 = splits2.get(&(0, 1)).expect("edge (0,1) split");
        assert_eq!(entry2.len(), 1, "merged twin collapses to one entry");
        assert_eq!(entry2[0].1, junction);
    }
}

#[cfg(test)]
mod edge_split_identity_tests {
    //! 2026-08-19 (R0053 anchor): the split collector honors a side-region
    //! BOUNDARY vertex that is collinear with the face edge to the scale-free
    //! identity but not bit-exactly (a rounding-perturbed minted crossing),
    //! and still ignores an INTERIOR vertex merely near the edge. RED under
    //! the exact-only test (the boundary split was dropped → T-junction),
    //! GREEN under the boundary + identity rule.
    use super::*;
    use crate::coplanar_overlay::{ClassifiedOverlay, ExactPoint2, RegionClass};
    use crate::stage0::frame::canonical_frame;
    use crate::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, Vector3};
    use cad_primitives::{Point2, Point3};

    fn unit_square_face() -> BRep {
        let p = |x: f64, y: f64| BRepVertex {
            point: Point3::new(x, y, 0.0),
        };
        let verts = vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)];
        let e = |s: u32, t: u32| BRepEdge {
            start: s,
            end: t,
            curve: Curve::LineSegment,
        };
        let edges = vec![e(0, 1), e(1, 2), e(2, 3), e(3, 0)];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("unit square face")
    }

    #[test]
    fn boundary_vertex_at_rounding_miss_splits_edge_interior_vertex_does_not() {
        let brep = unit_square_face();
        let frame = canonical_frame(&brep, 0).expect("frame");
        let coords: Vec<Point3> = brep.vertices().iter().map(|v| v.point).collect();
        // Overlay over the square: the four corners (0..3), a BOUNDARY vertex 4
        // on edge (0,1) at t=0.6 perturbed 1e-15 off the line (the rounding-
        // scale identification residue), and an INTERIOR vertex 5 at the same
        // u but 1e-12 inside the region (near the edge, NOT on the boundary).
        let pts2 = [
            frame.project(coords[0]),
            frame.project(coords[1]),
            frame.project(coords[2]),
            frame.project(coords[3]),
        ];
        let along = |t: f64| {
            (
                pts2[0].0 + t * (pts2[1].0 - pts2[0].0),
                pts2[0].1 + t * (pts2[1].1 - pts2[0].1),
            )
        };
        let (bu, bv) = along(0.6);
        // Perpendicular-ish nudge: add 1e-15 to both coordinates (off the
        // exact line in the generic frame).
        let boundary_pt = (bu + 1.0e-15, bv + 1.0e-15);
        let (iu, iv) = along(0.3);
        // Inward normal of edge (0,1) in the frame: toward corner 3.
        let (nu, nv) = (pts2[3].0 - pts2[0].0, pts2[3].1 - pts2[0].1);
        let interior_pt = (iu + 1.0e-12 * nu, iv + 1.0e-12 * nv);
        let all = [pts2[0], pts2[1], pts2[2], pts2[3], boundary_pt, interior_pt];
        let verts: Vec<Point2> = all.iter().map(|&(x, y)| Point2::new(x, y)).collect();
        let exact_verts: Vec<ExactPoint2> = all
            .iter()
            .map(|&(x, y)| ExactPoint2::from_f64(x, y).expect("finite"))
            .collect();
        // Side triangulation using 4 ON the boundary chain 0→4→1 and 5 as an
        // interior vertex: [0,4,5], [4,1,5], [1,2,5], [2,3,5], [3,0,5].
        let tris = vec![[0, 4, 5], [4, 1, 5], [1, 2, 5], [2, 3, 5], [3, 0, 5]];
        let class = vec![RegionClass::AOnly; tris.len()];
        let overlay = ClassifiedOverlay {
            verts,
            exact_verts,
            tris,
            class,
            poly_a: vec![0; 5],
            poly_b: vec![u32::MAX; 5],
            fused: BTreeMap::new(),
        };
        let resolved: Vec<Point3> = all.iter().map(|&(u, v)| frame.lift(u, v)).collect();
        let mut splits: SplitMap = BTreeMap::new();
        collect_edge_splits(
            &brep,
            0,
            &coords,
            &frame,
            &BTreeMap::new(),
            &overlay,
            [RegionClass::AOnly, RegionClass::Overlap],
            &resolved,
            &std::collections::BTreeSet::new(),
            &mut splits,
        );
        let on_01 = splits.get(&(0, 1)).cloned().unwrap_or_default();
        assert_eq!(
            on_01.len(),
            1,
            "the boundary vertex at a rounding-scale miss must register as ONE split of edge (0,1); got {on_01:?}"
        );
        let t = on_01[0].0.to_f64().value();
        assert!((t - 0.6).abs() < 1e-9, "split parameter {t} must be the boundary vertex's t≈0.6 (not the interior vertex at 0.3)");
        assert_eq!(splits.len(), 1, "no other edge gains a split: {splits:?}");
    }
}

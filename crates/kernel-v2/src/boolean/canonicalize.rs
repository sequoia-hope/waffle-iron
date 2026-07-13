//! M8 producer-boundary canonicalization of the yang `BRep` arrays, applied
//! by [`super::to_yang_brep`] between assembly and the `yang_rs::boolean` call.
//! Move-only split from the boolean god-module (design review 2026-07-12 F9);
//! byte-identical.
//!
//! - [`canonicalize_sibling_planes`] — sign-aware clustering of near-identical
//!   face planes (spec `specs/m8_intra_opposite_plane_canonicalization.md`).
//! - [`canonicalize_vertices_to_planes`] — re-derives all-planar vertices from
//!   their incident canonical planes (spec `specs/m8_shared_boundary_identity.md`).

use super::*;

/// PR-KV10 (M8 slice d): collapse rounding-noise plane bits across
/// same-plane sibling faces.
///
/// A boolean output legitimately carries several faces that are disjoint
/// fragments of ONE plane (a side plane split in two by a crossing union).
/// The arena stores each fragment's plane in point-normal form with a
/// per-fragment Newell normal, and `d` above is derived from each face's
/// own first loop vertex — so on oblique geometry the fragments' emitted
/// `(normal, d)` differ at the ~1e-16 rounding level. yang's intra-solid
/// near-coplanar gate treats BIT-identical planes as benign (one plane
/// split into several faces) and walls anything else, so without this pass
/// a fragment-carrying output cannot enter ANY further boolean (the
/// F0016-class corpus residue).
///
/// Rule: planar faces whose unit normals agree component-wise within
/// `TAU_WORK` and whose offsets agree within the scale-relative
/// `TAU_WORK·(1+|d|)` band — under EITHER sign `s ∈ {+1, −1}` applied to the
/// representative (spec `m8_intra_opposite_plane_canonicalization` B1/B2) —
/// adopt the FIRST such face's exact bits times `s` (deterministic; greedy
/// in face order — the band is ~4 orders below the near-coplanar DETECTION
/// band and ~6 below `MIN_FEATURE_SIZE`, so only rounding noise collapses
/// and cluster drift is impossible). The sign keeps each face's outward
/// sense (I1: `dot(n_before, n_after) > 0`) while an opposite-orientation
/// step pair (a chained output whose lower-step top and overhang bottom
/// share one geometric plane) ends up with EXACTLY negated plane bits (I2)
/// — the form yang-rs's intra-solid gate treats as benign. Vertex
/// coordinates are untouched: the residual between a loop vertex and the
/// adopted plane stays in the same scale-relative rounding class the
/// stored plane already had.
pub(super) fn canonicalize_sibling_planes(yfaces: &mut [yang_rs::BRepFace]) {
    // Representatives: (normal, d) of the first face seen in each cluster.
    let mut reps: Vec<([f64; 3], f64)> = Vec::new();
    for f in yfaces.iter_mut() {
        let yang_rs::Surface::Plane { normal, d } = &mut f.surface else {
            continue;
        };
        let n = normal.as_array();
        if !(n.iter().all(|c| c.is_finite()) && d.is_finite()) {
            continue;
        }
        let eps_n = cad_primitives::TAU_WORK;
        let dv = *d;
        let matched = reps.iter().find_map(|&(rn, rd)| {
            [1.0f64, -1.0f64]
                .into_iter()
                .find(|s| {
                    (0..3).all(|k| (n[k] - s * rn[k]).abs() <= eps_n)
                        && (dv - s * rd).abs() <= cad_primitives::TAU_WORK * (1.0 + rd.abs())
                })
                .map(|s| (rn, rd, s))
        });
        match matched {
            Some((rn, rd, s)) => {
                *normal = Vector3::new(s * rn[0], s * rn[1], s * rn[2]);
                *d = s * rd;
            }
            None => reps.push((n, dv)),
        }
    }
}

/// Spec `m8_shared_boundary_identity` — chained-output VERTEX canonicalization
/// (the KV10 completion: planes above, vertices here).
///
/// Each boolean-output vertex carries independent ~1e-16 rounding, so a
/// re-imported face loop is femto-crooked relative to its (canonicalized)
/// planes: intended-straight edges are not exactly straight, intended-
/// plane-constant coordinates are not bit-constant. The Stage-0 exact
/// overlay faithfully arranges that crookedness into femto-wide sweep
/// slabs, needle cells (`RoundingCollapse`), femto-twin split vertices
/// (ear-clip stalls), and near-coincident cross-input vertices inside
/// cherchi (`LabelMismatch`). Re-deriving each vertex from its incident
/// canonical planes eliminates the disease at the producer boundary.
///
/// Rules (spec §3): a vertex whose incident faces are ALL planar is
/// re-derived from its distinct incident planes ((n,d) and exactly
/// (−n,−d) are ONE plane): ≥3 independent → exact rational 3-plane solve
/// (B1); exactly 2 (or no independent triple, B6) → exact projection onto
/// the 2-plane intersection line (B2); <2 → unchanged (B3). The result is
/// rounded to f64 ONCE and adopted only when it moves the vertex by at
/// most the KV10-scale band `TAU_WORK·(1+|coord|)` per component (B4 —
/// a vertex genuinely off its planes' intersection is never forced).
/// Any curved incident face vetoes the vertex (B5 — rim/arc endpoints
/// must stay exactly on their curves). Deterministic: faces and planes in
/// push order; first independent triple / first non-degenerate pair wins.
pub(super) fn canonicalize_vertices_to_planes(
    yverts: &mut [yang_rs::BRepVertex],
    yedges: &[yang_rs::BRepEdge],
    yfaces: &[yang_rs::BRepFace],
) {
    use dashu::rational::RBig;

    // Exact f64 → RBig (f64 is exactly representable; non-finite handled
    // by the incidence filter below).
    fn rat(x: f64) -> RBig {
        let fb: dashu::float::FBig = dashu::float::FBig::try_from(x).expect("finite");
        RBig::try_from(fb).expect("finite")
    }

    // ── incidence: vertex → distinct incident canonical planes ──────────
    // Plane identity key: the raw (n, d) 4-tuple sign-normalized so (n, d)
    // and exactly (−n, −d) collapse to one geometric plane. Sign flip of an
    // f64 is exact, so the key is exact.
    let mut planes: Vec<Vec<([f64; 3], f64)>> = vec![Vec::new(); yverts.len()];
    let mut plane_keys: Vec<Vec<[u64; 4]>> = vec![Vec::new(); yverts.len()];
    let mut curved: Vec<bool> = vec![false; yverts.len()];
    for f in yfaces {
        let planar = match f.surface {
            yang_rs::Surface::Plane { normal, d } => {
                let n = normal.as_array();
                (n.iter().all(|c| c.is_finite()) && d.is_finite()).then_some((n, d))
            }
            _ => None,
        };
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &e in lp {
                let Some(edge) = yedges.get(e as usize) else {
                    continue;
                };
                for vi in [edge.start as usize, edge.end as usize] {
                    if vi >= yverts.len() {
                        continue;
                    }
                    match planar {
                        None => curved[vi] = true,
                        Some((n, d)) => {
                            let raw = [n[0], n[1], n[2], d];
                            // Sign-normalize: flip so the first nonzero
                            // component is positive (0.0/−0.0 both count
                            // as zero — skip them for the sign choice).
                            let s = raw.iter().find(|c| **c != 0.0).map_or(1.0, |c| {
                                if *c < 0.0 {
                                    -1.0
                                } else {
                                    1.0
                                }
                            });
                            let key = [
                                (s * raw[0]).to_bits(),
                                (s * raw[1]).to_bits(),
                                (s * raw[2]).to_bits(),
                                (s * raw[3]).to_bits(),
                            ];
                            if !plane_keys[vi].contains(&key) {
                                plane_keys[vi].push(key);
                                planes[vi].push((n, d));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── per-vertex re-derivation ─────────────────────────────────────────
    // B6 conditioning floor: skip a plane triple whose exact determinant
    // satisfies det² ≤ FLOOR²·(|n1|²·|n2|²·|n3|²) — sub-floor dihedrals
    // amplify femto residuals into large motion the band guard would
    // reject anyway; the floor keeps the search deterministic and cheap.
    const DET_FLOOR: f64 = 1.0e-9;

    for (vi, v) in yverts.iter_mut().enumerate() {
        if curved[vi] || planes[vi].len() < 2 {
            continue; // B5 / B3
        }
        let pls: Vec<([RBig; 3], RBig)> = planes[vi]
            .iter()
            .map(|&(n, d)| ([rat(n[0]), rat(n[1]), rat(n[2])], rat(d)))
            .collect();
        let norm2 = |n: &[RBig; 3]| &n[0] * &n[0] + &n[1] * &n[1] + &n[2] * &n[2];
        let det3 = |a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]| -> RBig {
            &a[0] * (&b[1] * &c[2] - &b[2] * &c[1]) - &a[1] * (&b[0] * &c[2] - &b[2] * &c[0])
                + &a[2] * (&b[0] * &c[1] - &b[1] * &c[0])
        };
        let floor2 = rat(DET_FLOOR) * rat(DET_FLOOR);

        // B1: first independent triple in plane order.
        let mut exact: Option<[RBig; 3]> = None;
        'triple: for i in 0..pls.len() {
            for j in (i + 1)..pls.len() {
                for k in (j + 1)..pls.len() {
                    let (na, nb, nc) = (&pls[i].0, &pls[j].0, &pls[k].0);
                    let det = det3(na, nb, nc);
                    if &det * &det <= &floor2 * &(norm2(na) * norm2(nb) * norm2(nc)) {
                        continue;
                    }
                    // Cramer: solve n·X = −d for the three planes (replace
                    // column m of [na; nb; nc] with the rhs).
                    let rhs = [-pls[i].1.clone(), -pls[j].1.clone(), -pls[k].1.clone()];
                    let col = |m: usize| -> RBig {
                        let rep = |r: &[RBig; 3], rv: &RBig| -> [RBig; 3] {
                            let mut o = r.clone();
                            o[m] = rv.clone();
                            o
                        };
                        det3(&rep(na, &rhs[0]), &rep(nb, &rhs[1]), &rep(nc, &rhs[2])) / &det
                    };
                    exact = Some([col(0), col(1), col(2)]);
                    break 'triple;
                }
            }
        }

        // B2 (or B6 degrade): exact projection onto the first
        // non-degenerate pair's intersection line. Solve
        // [n1; n2; dir] · X = [−d1; −d2; dir·P] with dir = n1×n2.
        if exact.is_none() {
            let p = v.point.as_array();
            let pr = [rat(p[0]), rat(p[1]), rat(p[2])];
            'pair: for i in 0..pls.len() {
                for j in (i + 1)..pls.len() {
                    let (na, nb) = (&pls[i].0, &pls[j].0);
                    let dir = [
                        &na[1] * &nb[2] - &na[2] * &nb[1],
                        &na[2] * &nb[0] - &na[0] * &nb[2],
                        &na[0] * &nb[1] - &na[1] * &nb[0],
                    ];
                    // |dir|² ≤ floor²·|na|²·|nb|² guards near-parallel
                    // distinct planes (sub-floor sin of the dihedral).
                    let d2 = norm2(&dir);
                    if d2 <= &floor2 * &(norm2(na) * norm2(nb)) {
                        continue;
                    }
                    let det = det3(na, nb, &dir);
                    if det == RBig::ZERO {
                        continue;
                    }
                    let rhs = [
                        -pls[i].1.clone(),
                        -pls[j].1.clone(),
                        &dir[0] * &pr[0] + &dir[1] * &pr[1] + &dir[2] * &pr[2],
                    ];
                    let rep = |r: &[RBig; 3], m: usize, rv: &RBig| -> [RBig; 3] {
                        let mut o = r.clone();
                        o[m] = rv.clone();
                        o
                    };
                    let col = |m: usize| -> RBig {
                        det3(
                            &rep(na, m, &rhs[0]),
                            &rep(nb, m, &rhs[1]),
                            &rep(&dir, m, &rhs[2]),
                        ) / &det
                    };
                    exact = Some([col(0), col(1), col(2)]);
                    break 'pair;
                }
            }
        }

        let Some(exact) = exact else { continue };
        let p = v.point.as_array();
        let mut newp = [0.0f64; 3];
        let mut ok = true;
        for k in 0..3 {
            let nf = exact[k].to_f64().value();
            if !nf.is_finite() {
                ok = false;
                break;
            }
            // B4 band guard, per component (KV10-scale, A14.3 reuse).
            if (nf - p[k]).abs() > cad_primitives::TAU_WORK * (1.0 + p[k].abs()) {
                ok = false;
                break;
            }
            newp[k] = nf;
        }
        if ok {
            v.point = Point3::new(newp[0], newp[1], newp[2]);
        } else if std::env::var_os("KV2_VERTEX_CANON_PROBE").is_some() {
            eprintln!(
                "[vertex-canon-over-band] v{vi} p={p:?} planes={}",
                planes[vi].len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M8-intra: sign-aware `canonicalize_sibling_planes` (spec
// `specs/m8_intra_opposite_plane_canonicalization.md`).
//
// RED (FIP Phase 2). `canonicalize_sibling_planes` is private to this module,
// so these unit tests exercise it directly (the seam KV10 pins E2E through the
// public `to_yang_brep`; the femto-EXACT-negation assertions below need
// hand-crafted plane bits that a real solid cannot be coaxed into producing).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod m8_intra_canonicalization_tests {
    use super::*;

    fn normalize(v: [f64; 3]) -> [f64; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    }

    /// A planar `yang_rs::BRepFace` carrying exactly `(normal, d)` — the loops
    /// are irrelevant to `canonicalize_sibling_planes` (it only rewrites
    /// `surface`), so they stay empty.
    fn plane_face(n: [f64; 3], d: f64) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: Vec::new(),
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    fn plane_of(f: &yang_rs::BRepFace) -> ([f64; 3], f64) {
        match f.surface {
            yang_rs::Surface::Plane { normal, d } => (normal.as_array(), d),
            _ => panic!("expected a planar face"),
        }
    }

    /// Nudge an f64 by `n` ULPs (femto-scale drift ~1e-16 at unit magnitude) —
    /// the same rounding-noise class PR-KV10 collapses, here on the NEGATED
    /// sibling.
    fn bump(x: f64, n: u64) -> f64 {
        f64::from_bits(x.to_bits().wrapping_add(n))
    }

    /// Spec B2 + I1 + I2 (RED): a femto-near-negated sibling pair must
    /// canonicalize so the second face's plane bits are the EXACT negation of
    /// the first (representative) face's bits, with sense preserved.
    ///
    /// RED today: `canonicalize_sibling_planes` matches only same-sign
    /// component-wise, so the near-negated face never joins the cluster and
    /// keeps its perturbed bits — the exact-negation assertions fail.
    #[test]
    fn femto_negated_sibling_canonicalizes_to_exact_negation() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // Face B: the near-EXACT negation of A, drifted a few ULPs per
        // component (a chained-output rounding artifact).
        let nb_before = [bump(-n[0], 3), bump(-n[1], 2), bump(-n[2], 5)];
        let db_before = bump(-d0, 4);

        let mut faces = vec![plane_face(n, d0), plane_face(nb_before, db_before)];
        canonicalize_sibling_planes(&mut faces);

        let (na_after, da_after) = plane_of(&faces[0]);
        let (nb_after, db_after) = plane_of(&faces[1]);

        // The representative (first face) is untouched.
        assert_eq!(na_after, n, "representative normal moved");
        assert_eq!(da_after, d0, "representative offset moved");

        // I2 (bit-exact negation): B adopts exactly (-n_rep, -d_rep).
        for k in 0..3 {
            assert_eq!(
                nb_after[k], -na_after[k],
                "component {k}: sibling plane not the exact negation of the representative"
            );
        }
        assert_eq!(db_after, -da_after, "sibling offset not the exact negation");

        // I1 (sense preservation): the adopted normal keeps B's outward sense.
        let dot: f64 = (0..3).map(|k| nb_before[k] * nb_after[k]).sum();
        assert!(
            dot > 0.0,
            "canonicalization flipped face B's sense (dot = {dot})"
        );
    }

    /// Spec B3 (guard): two GENUINELY distinct parallel planes (1e-3 apart —
    /// six orders above the `TAU_WORK·(1+|d|)` band) must never cluster. Passes
    /// today and must keep passing (no over-merge).
    #[test]
    fn distinct_parallel_planes_stay_unclustered_guard() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;
        let d1 = d0 + 1.0e-3;

        let mut faces = vec![plane_face(n, d0), plane_face(n, d1)];
        canonicalize_sibling_planes(&mut faces);

        let (_, da) = plane_of(&faces[0]);
        let (_, db) = plane_of(&faces[1]);
        assert_eq!(da, d0, "first distinct plane offset must be untouched");
        assert_eq!(
            db, d1,
            "distinct parallel plane wrongly collapsed onto sibling"
        );
    }

    // Spec I3 (same-orientation path byte-identical): the same-orientation
    // sibling-collapse behavior is pinned END-TO-END through the public
    // `to_yang_brep` path by `tests/kv10_plane_canonicalization.rs`
    // (`sibling_fragments_emit_bit_identical_planes`,
    // `chained_boolean_over_split_fragments_succeeds`). Those tests must remain
    // green after the sign-aware extension; this guard pins the same rule at
    // the unit boundary so a regression is localized here too.
    #[test]
    fn same_orientation_femto_siblings_still_collapse_guard() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // A femto-drifted SAME-sign sibling (the KV10 class).
        let nb = [bump(n[0], 3), bump(n[1], 2), bump(n[2], 5)];
        let db = bump(d0, 4);

        let mut faces = vec![plane_face(n, d0), plane_face(nb, db)];
        canonicalize_sibling_planes(&mut faces);

        let (na, da) = plane_of(&faces[0]);
        let (nb2, db2) = plane_of(&faces[1]);
        assert_eq!(na, n, "representative normal moved");
        assert_eq!(
            nb2, n,
            "same-orientation sibling did not adopt representative bits"
        );
        assert_eq!(da, d0, "representative offset moved");
        assert_eq!(
            db2, d0,
            "same-orientation sibling offset did not adopt representative"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the sign-aware `canonicalize_sibling_planes` at the SAME unit
    // boundary the RED tests above use. These live in-module (not in a new
    // `tests/` integration file) because `canonicalize_sibling_planes` is
    // module-private: the ULP-level band/greedy/zero-component attacks require
    // hand-crafted plane bits that only a direct call can inject, and the RED
    // note above already established that "a real solid cannot be coaxed into
    // producing" them through the public `to_yang_brep` seam. The E2E
    // over-merge guards that ARE reachable through the public API live in
    // `tests/m8_intra_adversary.rs`. Purely additive; touches no existing test.

    /// Attack 1 (offset-band boundary, negated arm): a negated sibling whose
    /// offset drift is 0.5× the `TAU_WORK·(1+|d|)` band clusters; 2× the band
    /// does not. Pins that the sign-aware match reuses the KV10 offset band
    /// unchanged (spec §2 — no new tolerance).
    #[test]
    fn adversary_negated_offset_band_just_below_and_just_above() {
        let n = [1.0, 0.0, 0.0];
        let d0 = 5.0_f64;
        let band = cad_primitives::TAU_WORK * (1.0 + d0.abs()); // = 6e-12

        // EXACT negation of the normal; offset drifted from -d0.
        let below = -d0 + 0.5 * band; // |below + d0| = 0.5·band ≤ band → cluster
        let above = -d0 + 2.0 * band; // 2·band > band → no cluster

        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.0, 0.0], below)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, db) = plane_of(&faces[1]);
            assert_eq!(
                nb,
                [-1.0, 0.0, 0.0],
                "just-below sibling normal not negated"
            );
            assert_eq!(db, -d0, "just-below sibling offset did not adopt −d_rep");
        }
        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.0, 0.0], above)];
            canonicalize_sibling_planes(&mut faces);
            let (_, db) = plane_of(&faces[1]);
            assert_eq!(
                db, above,
                "just-above sibling wrongly clustered (over-merge)"
            );
        }
    }

    /// Attack 1 (normal-component band, negated arm): a per-component normal
    /// drift of 0.5·`TAU_WORK` off exact negation clusters; 2·`TAU_WORK` does
    /// not. Uses a zero representative component so the drift is injected
    /// exactly (no cancellation).
    #[test]
    fn adversary_negated_normal_component_band_boundary() {
        let n = [1.0, 0.0, 0.0];
        let d0 = 5.0_f64;
        let eps = cad_primitives::TAU_WORK;

        // y-component drift off exact negation (rep y = 0, so |n_y − s·0| = n_y).
        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.5 * eps, 0.0], -d0)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, db) = plane_of(&faces[1]);
            assert_eq!(
                nb,
                [-1.0, 0.0, 0.0],
                "0.5·eps drift should cluster to −n_rep"
            );
            assert_eq!(db, -d0);
        }
        {
            let drift = [-1.0, 2.0 * eps, 0.0];
            let mut faces = vec![plane_face(n, d0), plane_face(drift, -d0)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, _) = plane_of(&faces[1]);
            assert_eq!(
                nb, drift,
                "2·eps normal drift wrongly clustered (over-merge)"
            );
        }
    }

    /// Attack 3 (greedy / order determinism): three faces — a representative, a
    /// same-sign femto sibling, and a negated femto sibling — collapse to ONE
    /// cluster under ALL 6 orderings, every face keeps its outward sense
    /// (I1: dot(before, after) > 0), and the result is sense-preserving and
    /// deterministic (each face's plane is exactly ± the first-seen rep's).
    #[test]
    fn adversary_three_face_cluster_is_order_invariant_and_sense_preserving() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // Same-sign femto sibling and negated femto sibling.
        let same = ([bump(n[0], 3), bump(n[1], 1), bump(n[2], 2)], bump(d0, 4));
        let neg = (
            [bump(-n[0], 2), bump(-n[1], 5), bump(-n[2], 1)],
            bump(-d0, 3),
        );
        let specs = [(n, d0), same, neg];

        for perm in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let before: Vec<([f64; 3], f64)> = perm.iter().map(|&i| specs[i]).collect();
            let mut faces: Vec<_> = before.iter().map(|&(nn, dd)| plane_face(nn, dd)).collect();
            canonicalize_sibling_planes(&mut faces);

            let after: Vec<([f64; 3], f64)> = faces.iter().map(plane_of).collect();
            let (rn, rd) = after[0];

            for (i, (&(nb, _), &(na, da))) in before.iter().zip(after.iter()).enumerate() {
                // I1: outward sense preserved.
                let dot: f64 = (0..3).map(|k| nb[k] * na[k]).sum();
                assert!(
                    dot > 0.0,
                    "perm {perm:?} face {i}: sense flipped (dot={dot})"
                );
                // Collapsed to exactly ± the first-seen representative.
                let pos = na == rn && da == rd;
                let negd = (0..3).all(|k| na[k] == -rn[k]) && da == -rd;
                assert!(
                    pos || negd,
                    "perm {perm:?} face {i}: plane {na:?},{da} is neither +rep nor −rep {rn:?},{rd}"
                );
            }
        }
    }

    /// Attack 4 (zero normal components): a rep with 0.0 components and a
    /// femto-near-negated sibling canonicalizes to the value-exact negation,
    /// with the zero components carried as −0.0 (= s·0.0) — the form the
    /// yang-rs exclusion's `0.0 == -0.0` value compare treats as benign.
    #[test]
    fn adversary_zero_component_negation_is_value_exact() {
        let n = [0.0, 0.0, 1.0];
        let d0 = 3.0_f64;
        let nb_before = [bump(-0.0, 0), bump(-0.0, 0), bump(-1.0, 4)]; // −0.0,−0.0,~−1
        let mut faces = vec![plane_face(n, d0), plane_face(nb_before, bump(-d0, 2))];
        canonicalize_sibling_planes(&mut faces);

        let (nb, db) = plane_of(&faces[1]);
        // Value-exact negation (0.0 == −0.0 holds by value).
        for k in 0..3 {
            assert_eq!(
                nb[k], -n[k],
                "component {k} not the value-negation of the rep"
            );
        }
        assert_eq!(db, -d0);
        // s·0.0 = −0.0: the adopted zero components carry the negative sign bit.
        assert_eq!(
            nb[0].to_bits(),
            (-0.0f64).to_bits(),
            "zero comp lost its −0.0 bit"
        );
        assert_eq!(nb[1].to_bits(), (-0.0f64).to_bits());
    }

    /// Attack 5 (non-unit normals): two faces with normals of different
    /// magnitude on ONE geometric plane (n vs −2n) must NOT cluster — the
    /// component band assumes unit normals, so the |−2 − (−1)·1| = 1 gap keeps
    /// them apart. Documented conservative residue; nothing crashes.
    #[test]
    fn adversary_nonunit_opposite_normals_do_not_cluster() {
        let mut faces = vec![
            plane_face([0.0, 0.0, 1.0], 3.0),
            plane_face([0.0, 0.0, -2.0], -6.0),
        ];
        canonicalize_sibling_planes(&mut faces);
        let (nb, db) = plane_of(&faces[1]);
        assert_eq!(nb, [0.0, 0.0, -2.0], "non-unit sibling wrongly rewritten");
        assert_eq!(db, -6.0);
    }

    /// Attack 6 (offset near 0, F0084-class): exactly-negated normals with tiny
    /// femto-scale offsets (the real probed signature d ≈ −6.9e-18 vs
    /// ≈ 1.2e-17) cluster — the offset band is ≈ `TAU_WORK`, orders above the
    /// drift — and the sibling adopts the exact negation −d_rep.
    #[test]
    fn adversary_offset_near_zero_negation_clusters() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let rd = -6.9e-18_f64;
        let neg_n = [-n[0], -n[1], -n[2]]; // exact negation of the normal
        let db_before = 1.2e-17_f64; // ≈ −rd, drifted at the femto scale

        let mut faces = vec![plane_face(n, rd), plane_face(neg_n, db_before)];
        canonicalize_sibling_planes(&mut faces);

        let (nb, db) = plane_of(&faces[1]);
        for k in 0..3 {
            assert_eq!(
                nb[k], -n[k],
                "near-zero-offset sibling normal not exactly negated"
            );
        }
        assert_eq!(
            db, -rd,
            "near-zero-offset sibling did not adopt −d_rep exactly"
        );
        let dot: f64 = (0..3).map(|k| neg_n[k] * nb[k]).sum();
        assert!(dot > 0.0, "sense flipped on the near-zero-offset sibling");
    }
}

// ---------------------------------------------------------------------------
// M8-vertex-canon: chained-output VERTEX canonicalization
// (spec `specs/m8_shared_boundary_identity.md`, FIP Phase 2, RED).
//
// Seam: a direct unit on the new pass `canonicalize_vertices_to_planes`, which
// `to_yang_brep` will call immediately after `canonicalize_sibling_planes` on
// the assembled yang arrays. A hand-CROOKED arena cannot be built through the
// public arena/Euler constructors — `to_yang_brep` anchors each yang plane's
// `d` at a loop vertex, so a vertex is never inconsistent with its own derived
// plane; the femto-crooked-vs-canonical divergence only appears mid-`to_yang`,
// after plane canonicalization. So the invariants are exercised on the yang
// (verts, edges, faces) shape directly.
//
// SETTLED SIGNATURE (the implementer matches this — it is exactly the data
// `to_yang_brep` holds at that point):
//
//   fn canonicalize_vertices_to_planes(
//       yverts: &mut [yang_rs::BRepVertex],
//       yedges: &[yang_rs::BRepEdge],
//       yfaces: &[yang_rs::BRepFace],
//   )
//
// Vertex→incident-plane incidence is recovered from `yedges` (loop edge →
// vertex pair) over each face's loops. These tests do NOT compile until that
// function exists — that IS the RED state for the unit oracles.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod m8_vertex_canon_tests {
    use super::*;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    fn vtx(x: f64, y: f64, z: f64) -> yang_rs::BRepVertex {
        yang_rs::BRepVertex {
            point: Point3::new(x, y, z),
        }
    }

    fn seg(s: u32, e: u32) -> yang_rs::BRepEdge {
        yang_rs::BRepEdge {
            start: s,
            end: e,
            curve: yang_rs::Curve::LineSegment,
        }
    }

    fn plane_face(n: [f64; 3], d: f64, loop_edges: Vec<u32>) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: loop_edges,
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    fn cyl_face(loop_edges: Vec<u32>) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
            outer_loop: loop_edges,
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    /// Nudge `x` by `k` ULPs (femto drift ~1e-16 at unit magnitude).
    fn bump(x: f64, k: i64) -> f64 {
        if k >= 0 {
            f64::from_bits(x.to_bits().wrapping_add(k as u64))
        } else {
            f64::from_bits(x.to_bits().wrapping_sub((-k) as u64))
        }
    }

    fn vbits(v: &yang_rs::BRepVertex) -> [u64; 3] {
        let a = v.point.as_array();
        [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
    }

    fn rat(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite");
        RBig::try_from(fb).expect("finite")
    }

    fn round_f64(r: &RBig) -> f64 {
        r.to_f64().value()
    }

    /// Box [1,3]³ topology: 6 quad faces, each carrying its own 4 directed
    /// edges over its corner loop; every corner is incident to exactly its 3
    /// axis planes. Returns (edges, faces); vertices supplied separately.
    fn box_topology() -> (Vec<yang_rs::BRepEdge>, Vec<yang_rs::BRepFace>) {
        // (corner loop, plane normal, plane d) — n·x + d = 0.
        let faces: [([u32; 4], [f64; 3], f64); 6] = [
            ([0, 1, 2, 3], [0.0, 0.0, -1.0], 1.0), // z = 1
            ([4, 5, 6, 7], [0.0, 0.0, 1.0], -3.0), // z = 3
            ([0, 1, 5, 4], [0.0, -1.0, 0.0], 1.0), // y = 1
            ([1, 2, 6, 5], [1.0, 0.0, 0.0], -3.0), // x = 3
            ([2, 3, 7, 6], [0.0, 1.0, 0.0], -3.0), // y = 3
            ([3, 0, 4, 7], [-1.0, 0.0, 0.0], 1.0), // x = 1
        ];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (corners, n, d) in faces {
            let base = yedges.len() as u32;
            for k in 0..4 {
                yedges.push(seg(corners[k], corners[(k + 1) % 4]));
            }
            yfaces.push(plane_face(n, d, (base..base + 4).collect()));
        }
        (yedges, yfaces)
    }

    const BOX_CORNERS: [[f64; 3]; 8] = [
        [1.0, 1.0, 1.0],
        [3.0, 1.0, 1.0],
        [3.0, 3.0, 1.0],
        [1.0, 3.0, 1.0],
        [1.0, 1.0, 3.0],
        [3.0, 1.0, 3.0],
        [3.0, 3.0, 3.0],
        [1.0, 3.0, 3.0],
    ];

    /// B1 / I1 (RED): a femto-crooked axis-aligned box — planes exact, each
    /// corner perturbed a few ULPs off its exact tri-plane intersection —
    /// snaps every corner BIT-equal to the exact integer intersection. I3: a
    /// second pass is a byte-identical no-op.
    #[test]
    fn femto_crooked_box_snaps_corners_to_exact_intersections() {
        // Distinct per-vertex/per-axis ULP perturbations (all ≪ the band
        // TAU_WORK·(1+3) = 4e-12, so all adopted).
        let dk: [[i64; 3]; 8] = [
            [1, -2, 3],
            [-1, 2, -3],
            [2, -1, 1],
            [-3, 1, -2],
            [1, 3, -1],
            [-2, -1, 2],
            [3, -3, 1],
            [-1, 2, -2],
        ];
        let mut yverts: Vec<_> = (0..8)
            .map(|i| {
                vtx(
                    bump(BOX_CORNERS[i][0], dk[i][0]),
                    bump(BOX_CORNERS[i][1], dk[i][1]),
                    bump(BOX_CORNERS[i][2], dk[i][2]),
                )
            })
            .collect();
        let (yedges, yfaces) = box_topology();

        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        for i in 0..8 {
            let got = yverts[i].point.as_array();
            for k in 0..3 {
                assert_eq!(
                    got[k].to_bits(),
                    BOX_CORNERS[i][k].to_bits(),
                    "B1/I1: corner {i} coord {k} not bit-equal to the exact intersection"
                );
            }
        }

        // I3 idempotence: rerun is byte-identical.
        let snap: Vec<_> = yverts.iter().map(vbits).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        for i in 0..8 {
            assert_eq!(
                vbits(&yverts[i]),
                snap[i],
                "I3: second pass is not byte-identical (vertex {i})"
            );
        }
    }

    /// B2 (RED): a subdivided-edge vertex on exactly 2 (orthogonal) planes,
    /// perturbed femto off their intersection line, lands BIT-equal to the
    /// exact rational line projection and both plane residuals collapse to a
    /// rounding ULP.
    #[test]
    fn subdivided_edge_vertex_projects_onto_intersection_line() {
        // Planes y=1 (n=(0,1,0), d=−1) and z=1 (n=(0,0,1), d=−1); line {(t,1,1)}.
        let n_a = [0.0, 1.0, 0.0];
        let d_a = -1.0;
        let n_b = [0.0, 0.0, 1.0];
        let d_b = -1.0;
        // V intended at x=2 on the line, perturbed femto in y and z (x exact).
        let vx = 2.0;
        let v = [vx, bump(1.0, 3), bump(1.0, 2)];

        let mut yverts = vec![
            vtx(v[0], v[1], v[2]), // 0: V — incident to both planes
            vtx(0.0, 1.0, 0.0),    // 1: on y=1
            vtx(0.0, 1.0, 3.0),    // 2: on y=1
            vtx(0.0, 0.0, 1.0),    // 3: on z=1
            vtx(0.0, 3.0, 1.0),    // 4: on z=1
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0), // face A (y=1): edges 0,1,2
            seg(0, 3),
            seg(3, 4),
            seg(4, 0), // face B (z=1): edges 3,4,5
        ];
        let yfaces = vec![
            plane_face(n_a, d_a, vec![0, 1, 2]),
            plane_face(n_b, d_b, vec![3, 4, 5]),
        ];

        // Exact expected: P' = V − (n_a·V+d_a)·n_a − (n_b·V+d_b)·n_b, valid as
        // the line projection because n_a·n_b = 0 (orthogonal). Computed in
        // RBig, rounded once — exactly what the pass must produce.
        let dot = n_a[0] * n_b[0] + n_a[1] * n_b[1] + n_a[2] * n_b[2];
        assert_eq!(
            dot, 0.0,
            "fixture: planes must be orthogonal for this closed form"
        );
        let vr = [rat(v[0]), rat(v[1]), rat(v[2])];
        let nar = [rat(n_a[0]), rat(n_a[1]), rat(n_a[2])];
        let nbr = [rat(n_b[0]), rat(n_b[1]), rat(n_b[2])];
        let dot3 = |a: &[RBig; 3], b: &[RBig; 3]| {
            a[0].clone() * b[0].clone() + a[1].clone() * b[1].clone() + a[2].clone() * b[2].clone()
        };
        let ra = dot3(&nar, &vr) + rat(d_a);
        let rb = dot3(&nbr, &vr) + rat(d_b);
        let mut expected = [0.0; 3];
        for k in 0..3 {
            let pk = vr[k].clone() - ra.clone() * nar[k].clone() - rb.clone() * nbr[k].clone();
            expected[k] = round_f64(&pk);
        }

        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        let got = yverts[0].point.as_array();
        for k in 0..3 {
            assert_eq!(
                got[k].to_bits(),
                expected[k].to_bits(),
                "B2: vertex coord {k} not bit-equal to the exact line projection"
            );
        }
        let res_a = n_a[0] * got[0] + n_a[1] * got[1] + n_a[2] * got[2] + d_a;
        let res_b = n_b[0] * got[0] + n_b[1] * got[1] + n_b[2] * got[2] + d_b;
        let ulp = 4.0 * f64::EPSILON * (1.0 + vx.abs());
        assert!(
            res_a.abs() <= ulp && res_b.abs() <= ulp,
            "B2: plane residuals must collapse to a rounding ULP (a={res_a}, b={res_b})"
        );
    }

    /// B4 guard: a vertex 1e-6 off its 3 planes (≫ band) is left UNCHANGED
    /// (never forced onto an intersection it doesn't belong to).
    #[test]
    fn vertex_beyond_band_is_unchanged() {
        // Planes x=2, y=2, z=2; V is 1e-6 off each (band = 3e-12).
        let v = [2.0 + 1e-6, 2.0 + 1e-6, 2.0 + 1e-6];
        let mut yverts = vec![
            vtx(v[0], v[1], v[2]),
            vtx(2.0, 0.0, 0.0),
            vtx(2.0, 0.0, 5.0), // x=2
            vtx(0.0, 2.0, 0.0),
            vtx(0.0, 2.0, 5.0), // y=2
            vtx(0.0, 0.0, 2.0),
            vtx(5.0, 0.0, 2.0), // z=2
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0),
            seg(0, 3),
            seg(3, 4),
            seg(4, 0),
            seg(0, 5),
            seg(5, 6),
            seg(6, 0),
        ];
        let yfaces = vec![
            plane_face([1.0, 0.0, 0.0], -2.0, vec![0, 1, 2]),
            plane_face([0.0, 1.0, 0.0], -2.0, vec![3, 4, 5]),
            plane_face([0.0, 0.0, 1.0], -2.0, vec![6, 7, 8]),
        ];
        let before = vbits(&yverts[0]);
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        assert_eq!(
            vbits(&yverts[0]),
            before,
            "B4: a vertex 1e-6 off its planes (≫ band) must be left UNCHANGED"
        );
    }

    /// B7 / I4 guard: an already-exact box is byte-identical through the pass.
    #[test]
    fn exact_box_is_byte_identical() {
        let mut yverts: Vec<_> = (0..8)
            .map(|i| vtx(BOX_CORNERS[i][0], BOX_CORNERS[i][1], BOX_CORNERS[i][2]))
            .collect();
        let (yedges, yfaces) = box_topology();
        let before: Vec<_> = yverts.iter().map(vbits).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        for i in 0..8 {
            assert_eq!(
                vbits(&yverts[i]),
                before[i],
                "B7/I4: exact box vertex {i} must be byte-identical"
            );
        }
    }

    /// B5 guard: a vertex that WOULD snap under B2 (femto off two planes) but
    /// also touches a curved (cylinder) face is left UNCHANGED — curve
    /// exactness owns the vertex.
    #[test]
    fn vertex_with_curved_incident_face_is_unchanged() {
        let v = [2.0, bump(1.0, 3), bump(1.0, 2)];
        let mut yverts = vec![
            vtx(v[0], v[1], v[2]), // 0: V
            vtx(0.0, 1.0, 0.0),
            vtx(0.0, 1.0, 3.0), // y=1
            vtx(0.0, 0.0, 1.0),
            vtx(0.0, 3.0, 1.0), // z=1
            vtx(5.0, 0.0, 0.0),
            vtx(5.0, 5.0, 0.0), // cylinder-face loop mates
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0), // plane A (y=1)
            seg(0, 3),
            seg(3, 4),
            seg(4, 0), // plane B (z=1)
            seg(0, 5),
            seg(5, 6),
            seg(6, 0), // cylinder face touches V
        ];
        let yfaces = vec![
            plane_face([0.0, 1.0, 0.0], -1.0, vec![0, 1, 2]),
            plane_face([0.0, 0.0, 1.0], -1.0, vec![3, 4, 5]),
            cyl_face(vec![6, 7, 8]),
        ];
        let before = vbits(&yverts[0]);
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        assert_eq!(
            vbits(&yverts[0]),
            before,
            "B5: a vertex with ANY curved incident face must be left UNCHANGED"
        );
    }

    /// I2 (oblique bounded motion): a rotated-frame crooked box — planes exact
    /// in an oblique orthonormal frame, corners carrying the sub-band residuals
    /// an oblique fresh extrude has by construction (§4 I4 amendment) — is
    /// canonicalized with EVERY adopted per-component displacement ≤ the KV10
    /// band `TAU_WORK·(1+|coord|)`, and at least one vertex actually moves
    /// (non-vacuous). This pins the oblique blast radius the amended I4 carved
    /// out of byte-identity.
    #[test]
    fn oblique_crooked_box_moves_within_band() {
        // Oblique orthonormal frame (u, v, t) — irrational direction cosines.
        fn norm(a: [f64; 3]) -> [f64; 3] {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            [a[0] / l, a[1] / l, a[2] / l]
        }
        let u = norm([1.0, 2.0, 3.0]);
        let wref = [0.3, -0.4, 0.5];
        let du = wref[0] * u[0] + wref[1] * u[1] + wref[2] * u[2];
        let v = norm([
            wref[0] - du * u[0],
            wref[1] - du * u[1],
            wref[2] - du * u[2],
        ]);
        let t = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let o = [0.1, 0.2, 0.3];
        let l = 2.0;
        // Corner (i,j,k) = O + i·L·u + j·L·v + k·L·t (i,j,k ∈ {0,1}); its f64
        // evaluation carries the ~1e-16 frame residual off the exact 3-plane
        // intersection. A couple ULPs of extra perturbation guarantees motion.
        let corner = |i: f64, j: f64, k: f64| {
            [
                o[0] + i * l * u[0] + j * l * v[0] + k * l * t[0],
                o[1] + i * l * u[1] + j * l * v[1] + k * l * t[1],
                o[2] + i * l * u[2] + j * l * v[2] + k * l * t[2],
            ]
        };
        let ijk = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let mut yverts: Vec<_> = ijk
            .iter()
            .map(|c| {
                let p = corner(c[0], c[1], c[2]);
                // +2 ULPs on x to force a detectable move on adoption.
                vtx(bump(p[0], 2), p[1], p[2])
            })
            .collect();

        // 6 faces: normals u,v,t; offset d = −(axis·O + s·L), s the face's slab.
        let faces: [([u32; 4], [f64; 3], f64); 6] = [
            ([0, 1, 2, 3], t, -dot(t, o)),       // t = 0
            ([4, 5, 6, 7], t, -(dot(t, o) + l)), // t = L
            ([0, 1, 5, 4], v, -dot(v, o)),       // v = 0
            ([1, 2, 6, 5], u, -(dot(u, o) + l)), // u = L
            ([2, 3, 7, 6], v, -(dot(v, o) + l)), // v = L
            ([3, 0, 4, 7], u, -dot(u, o)),       // u = 0
        ];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (corners, n, d) in faces {
            let base = yedges.len() as u32;
            for k in 0..4 {
                yedges.push(seg(corners[k], corners[(k + 1) % 4]));
            }
            yfaces.push(plane_face(n, d, (base..base + 4).collect()));
        }

        let before: Vec<[f64; 3]> = yverts.iter().map(|v| v.point.as_array()).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        let mut moved = 0usize;
        for i in 0..8 {
            let a = before[i];
            let b = yverts[i].point.as_array();
            for k in 0..3 {
                let disp = (b[k] - a[k]).abs();
                let band = cad_primitives::TAU_WORK * (1.0 + a[k].abs());
                assert!(
                    disp <= band,
                    "I2: oblique vertex {i} coord {k} moved {disp:e} > band {band:e}"
                );
            }
            if b != a {
                moved += 1;
            }
        }
        assert!(
            moved > 0,
            "non-vacuous: the oblique pass must actually move at least one vertex"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on canonicalize_vertices_to_planes (band edge, DET_FLOOR wedge,
    // negated/duplicate plane dedup, >3-plane determinism). In-module (the
    // function is private). Purely additive; touches no existing test.

    /// Build a vertex-0 incident to a list of planes, each face a triangle loop
    /// (V, a, b) so V is topologically on every plane. `extra_verts` supplies the
    /// two loop-mate coordinates per face (their positions are irrelevant to the
    /// pass — only V's planes matter).
    fn single_vertex_on_planes(
        v0: [f64; 3],
        planes: &[([f64; 3], f64)],
    ) -> (
        Vec<yang_rs::BRepVertex>,
        Vec<yang_rs::BRepEdge>,
        Vec<yang_rs::BRepFace>,
    ) {
        let mut yverts = vec![vtx(v0[0], v0[1], v0[2])];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (i, &(n, d)) in planes.iter().enumerate() {
            // Two throwaway loop mates per face (distinct indices).
            let a = yverts.len() as u32;
            yverts.push(vtx(10.0 + i as f64, 0.0, 0.0));
            let b = yverts.len() as u32;
            yverts.push(vtx(0.0, 10.0 + i as f64, 0.0));
            let base = yedges.len() as u32;
            yedges.push(seg(0, a));
            yedges.push(seg(a, b));
            yedges.push(seg(b, 0));
            yfaces.push(plane_face(n, d, vec![base, base + 1, base + 2]));
        }
        (yverts, yedges, yfaces)
    }

    /// B4 band edge (per component): a vertex off its 3 axis planes by 0.5·band
    /// on one axis ADOPTS the exact intersection; by 2·band it is left
    /// UNCHANGED. Tighter than the 1e-6 guard; pins the `<=` boundary and is the
    /// dedicated killer for dropping the band guard.
    #[test]
    fn adversary_band_edge_adopt_below_reject_above() {
        let planes = [
            ([1.0, 0.0, 0.0], -2.0),
            ([0.0, 1.0, 0.0], -2.0),
            ([0.0, 0.0, 1.0], -2.0),
        ];
        let band = cad_primitives::TAU_WORK * (1.0 + 2.0); // 3e-12

        // 0.5·band off on x only → adopts (x snaps to 2.0 exactly).
        let (mut yv, ye, yf) = single_vertex_on_planes([2.0 + 0.5 * band, 2.0, 2.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        assert_eq!(
            yv[0].point.x().to_bits(),
            2.0f64.to_bits(),
            "0.5·band off must adopt the exact plane intersection"
        );

        // 2·band off on x → whole vertex unchanged (band guard rejects).
        let off = 2.0 + 2.0 * band;
        let (mut yv, ye, yf) = single_vertex_on_planes([off, 2.0, 2.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        assert_eq!(
            yv[0].point.x().to_bits(),
            off.to_bits(),
            "2·band off must leave the vertex UNCHANGED (band guard)"
        );
    }

    /// MUTATION KILLER (c) — DET_FLOOR wedge. A vertex on two well-conditioned
    /// planes (x=2, y=2) plus a THIN-WEDGE plane whose normal (1,0,ε), ε=1e-11,
    /// is near-parallel to x=2 AND whose offset places the exact 3-plane
    /// intersection FAR away (z≈1e6). With the DET_FLOOR (production) the
    /// near-dependent triple is skipped and the vertex degrades to B2 — projected
    /// exactly onto the x=2,y=2 line, so its femto-off x/y SNAP to 2.0. WITHOUT
    /// the floor (DET_FLOOR=0) the ill-conditioned 3-plane solve returns the far
    /// (2,2,≈1e6) point, which the band guard REJECTS → the vertex is left
    /// crooked (x stays 2+δ). So the floor is load-bearing: it turns a rejected
    /// wild solve into an adopted B2 straighten.
    ///
    /// Verified: production → x bit-equal 2.0; DET_FLOOR=0 mutant → x unchanged.
    #[test]
    fn adversary_thin_wedge_floor_degrades_to_b2_straighten() {
        let eps = 1.0e-11_f64; // < DET_FLOOR=1e-9 → triple skipped in production
        let d3 = -2.0 - 1.0e-5_f64; // 3-plane intersection z = 1e-5/eps = 1e6
        let planes = [
            ([1.0, 0.0, 0.0], -2.0), // x = 2
            ([0.0, 1.0, 0.0], -2.0), // y = 2
            ([1.0, 0.0, eps], d3),   // thin wedge, near-parallel to x=2
        ];
        let delta = 1.0e-13_f64; // femto off the x=2,y=2 line, ≪ band
        let (mut yv, ye, yf) = single_vertex_on_planes([2.0 + delta, 2.0 + delta, 5.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        // B2 degrade projects onto the exact x=2,y=2 line.
        assert_eq!(
            yv[0].point.x().to_bits(),
            2.0f64.to_bits(),
            "B6→B2: x must snap to the exact 2-plane line (DET_FLOOR skipped the wild triple)"
        );
        assert_eq!(
            yv[0].point.y().to_bits(),
            2.0f64.to_bits(),
            "B6→B2: y snaps to 2.0"
        );
        assert_eq!(yv[0].point.z(), 5.0, "z stays on the free line coordinate");
    }

    /// Negated + exact-duplicate plane dedup. A vertex incident to x=2 via BOTH
    /// orientations (n,d)=((1,0,0),−2) AND ((−1,0,0),2) — plus y=2 and z=2 — must
    /// solve to the exact apex (2,2,2): the negated pair is ONE plane, so three
    /// DISTINCT planes remain. Pins the dedup's intended semantics.
    ///
    /// FINDING (documented, not a killer): dropping the dedup does NOT change the
    /// result — the exact det floor skips every triple containing a
    /// negated/duplicate pair (det ≡ 0), so the first INDEPENDENT triple found is
    /// identical with or without the dedup. The dedup is a
    /// performance/legibility optimization, structurally redundant with the det
    /// floor for correctness (analogous to the ear-clip coverage-cert finding).
    #[test]
    fn adversary_negated_duplicate_planes_solve_to_apex() {
        let planes = [
            ([1.0, 0.0, 0.0], -2.0), // x = 2
            ([-1.0, 0.0, 0.0], 2.0), // x = 2, opposite orientation (dedup target)
            ([0.0, 1.0, 0.0], -2.0), // y = 2
            ([0.0, 0.0, 1.0], -2.0), // z = 2
        ];
        let bump = |x: f64, k: i64| bump(x, k);
        let (mut yv, ye, yf) =
            single_vertex_on_planes([bump(2.0, 2), bump(2.0, -1), bump(2.0, 3)], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        for (k, want) in [(0usize, 2.0f64), (1, 2.0), (2, 2.0)] {
            assert_eq!(
                yv[0].point.as_array()[k].to_bits(),
                want.to_bits(),
                "negated/duplicate planes must solve to the exact apex (coord {k})"
            );
        }
    }

    /// I5 determinism — a vertex where FOUR planes concur at the apex (2,2,2):
    /// the three axis planes plus a diagonal x+y+z=6. Every face-order
    /// permutation selects a valid independent triple through the SAME apex, so
    /// the adopted point is permutation-invariant and bit-exact.
    #[test]
    fn adversary_four_concurrent_planes_permutation_invariant() {
        let base = [
            ([1.0, 0.0, 0.0], -2.0),
            ([0.0, 1.0, 0.0], -2.0),
            ([0.0, 0.0, 1.0], -2.0),
            ([1.0, 1.0, 1.0], -6.0), // x+y+z=6, through (2,2,2)
        ];
        let start = [bump(2.0, 1), bump(2.0, -2), bump(2.0, 2)];
        // A few representative orderings of the four planes.
        for perm in [[0, 1, 2, 3], [3, 2, 1, 0], [2, 0, 3, 1], [1, 3, 0, 2]] {
            let planes: Vec<_> = perm.iter().map(|&i| base[i]).collect();
            let (mut yv, ye, yf) = single_vertex_on_planes(start, &planes);
            canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
            for k in 0..3 {
                assert_eq!(
                    yv[0].point.as_array()[k].to_bits(),
                    2.0f64.to_bits(),
                    "I5: permutation {perm:?} coord {k} must be the exact apex"
                );
            }
        }
    }
}

//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing
//! - **Stage 1** (§4.1): Bijective tessellation — PR-YR2: planar B-Reps;
//!   PR-YR7: cylinder; PR-YR12: sphere (Cone still rejects loudly)
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate to `ssi-rs`
//! - **Stage 4** (§4.4.1): Mesh updating — RELOCATION of intersection crossings
//!   onto the exact curve (+ §4.5.3 reversed-point sweep), watertightness
//!   inherited from the mesh boolean. The paper's CDT remesh / split-merge-insert
//!   is **NOT implemented** (deviation N2 in `docs/yang_deviations.md`); the
//!   sidecar's trimmed mesh is trusted and `check_watertight_2manifold` gates the
//!   output. Likewise §4.5.4 illegal-self-intersection removal is **NOT
//!   implemented** (deviation N6, roadmap-tracked).
//! - **Stage 5** (§4.4.2): Patch segmentation (flood-fill)
//! - **Stage 6** (§4.4.2): B-Rep reassembly
//!
//! ## Current implementation status (PR-YR5)
//!
//! - **Stage 1 PLANAR** (PR-YR2): `BRep::new(verts, edges, faces)`
//!   fan-triangulates each planar face from its first vertex; produces
//!   a 1:1 bijection (no Steiner points). Convex faces only; no inner
//!   loops; `Surface::Plane` only.
//! - **`boolean()` vertex provenance** (PR-YR3): every output mesh
//!   vertex is spatially matched against input A then B (within
//!   [`MATCH_TOLERANCE`]). On match, the corresponding input's
//!   `TessellationSource` is copied; unmatched verts get
//!   `TessellationSource::Intersection`.
//! - **`boolean()` triangle attribution** (PR-YR4): every output
//!   triangle is attributed to an input `(InputId, face_idx)` via
//!   majority-vote (≥2 of 3) over the vertices' provenance.
//!   Accessible via [`BRep::triangle_attribution`].
//! - **`boolean()` topology reconstruction** (PR-YR5): output `BRep`
//!   gets non-empty `vertices` (1:1 with mesh), `edges`, and `faces`
//!   via patch flood-fill on triangle attribution + boundary cycle
//!   recovery + surface inheritance from input faces.
//!   None-attributed (cut surface) triangles are intentionally
//!   skipped — output is a "kept-portions skeleton."
//! - **`BRep::from_mesh()` degenerate path** (PR-YR1 compat): empty
//!   topology; all-`Unknown` TessellationMap; empty
//!   TriangleAttributionMap.
//!
//! **Honest framing**: PR-YR3 + PR-YR4 + PR-YR5 are NOT real Yang
//! Stage 5/6. Real Stage 5/6 needs per-triangle labels from Stage 2's
//! arrangement which the C++ sidecar doesn't expose. The current
//! pipeline is a sidecar-feasible substitute.
//!
//! **PR-YR5 output is intentionally NOT 2-manifold** (rule-4
//! deviation): faces cover input-derived ("kept") portions only.
//! Cut-surface faces (`None`-attributed triangles → new BRepFaces with
//! reconstructed surfaces) are PR-YR6, which also re-enables the
//! 2-manifold contract.
//!
//! Banked for future PRs:
//! - PR-YR2b: ear-cutting for non-convex faces
//! - PR-YR2c: inner loops (holes) — currently → `NonManifoldOutput`
//! - PR-YR2d: curved surfaces (`Surface::Cylinder`, `Sphere`, NURBS)
//! - PR-YR2e: Steiner points + dε tolerance
//! - PR-YR2f: CDT at shared edges
//! - PR-YR4b: precomputed vertex→edge / edge→face incidence indices
//! - PR-YR5b: edge deduplication across faces (each face owns its edges in v1)
//! - PR-YR5c: inner-loop / hole support in patch boundary recovery
//! - PR-YR6: cut-surface face generation + 2-manifold validation
//! - PR-YR7+: edge curve recovery beyond `Curve::LineSegment`
//! - Real Stage 5/6: gated on labeled arrangement output
//!
//! ## Input / output
//!
//! - Input: two B-Rep solids (`BRep`)
//! - Output: one B-Rep solid
//! - Non-manifold detection is **not yet implemented** in PR-YR2.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`

// Stage 0 (Yang §4.5.5) coplanar-overlay geometric engine — M8 slice a
// (PR-YR25). NOT yet wired into `boolean()`; that's M8 slice b.
pub mod coplanar_overlay;
mod stage0;
// N2 increment 2: the §4.1.2 / Fig 6 per-triangle `d(T)` bound + its pinned
// parametric embedding. NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_dt_recompute.md`.
pub mod stage4_dt;
// N2 increment 1: the §4.4.1 mesh-updating primitive (Fig 11 split/merge/insert
// + interior-constraint CDT). NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_mesh_updating.md`.
mod brep;
mod errors;
mod geom;
mod stage1_tessellate;
mod stage3_ssi;
pub(crate) use stage3_ssi::*;
mod stage4_correct;
mod stage5_topology;
pub(crate) use stage5_topology::*;
mod stage4_relocate;
pub use brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationMap, TessellationSource,
    TriangleAttribution, TriangleAttributionMap, MATCH_TOLERANCE,
};
pub use stage1_tessellate::tessellate_torus_patch;
pub(crate) use stage1_tessellate::*;
pub(crate) use stage4_correct::*;
pub(crate) use stage4_relocate::*;
pub mod stage4_update;
pub use errors::{SsiRefinementError, Stage4InvalidReason, YangError};
pub(crate) use geom::{ellipse_param, ellipse_point, ellipse_tangent, surface_normal_at};
pub use geom::{hyperbola_point, parabola_point, signed_distance_to_surface, Curve, Surface};

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};
pub use cherchi_rs::{NativeBoolean, NativeBooleanError};
// The constrained-Delaunay primitive, re-exported for the kernel-v2 render
// tessellation cores (its `tessellate.rs` patch/planar triangulation). kernel-v2
// may depend on yang-rs but NOT on cherchi-rs directly, so it consumes the CDT
// through this seam — the same pattern as `NativeBoolean` above and the torus
// UV-patch consumer's existing use of this primitive.
pub use cherchi_rs::triangulation::{
    cdt_polygon_with_holes, cdt_polygon_with_holes_floodfill, CdtError,
};
// `ArrangementError` is re-exported so that kernel-v2 (whose dep rules allow
// `yang-rs` but NOT `cherchi-rs`) can pattern-match the M8 boundary inside
// `NativeBooleanError::Arrangement` — specifically
// `ArrangementError::CoplanarPairDeferred`, which kernel-v2 maps to its
// typed `UnsupportedCoplanar` error. Public-surface addition only.
pub use cherchi_rs::ArrangementError;

/// Construct the PRODUCTION boolean backend: the native, in-process
/// cherchi-rs pipeline ([`NativeBoolean`]) — `mesh_arrangement` → labeling →
/// `keep_set(op)`. Reference parity vs the upstream C++ `mesh_booleans`
/// binary is the M6 gate (cherchi-rs `tests/parity_native_vs_sidecar.rs`);
/// the C++ subprocess sidecar (`cherchi-sidecar-rs`) is demoted to a
/// test-only parity oracle (PR-CR-BL3c).
///
/// Always `Some` since PR-CR-M7c: the predicates are clean-room pure Rust
/// (`cherchi-rs::predicates::indirect`) — there is no FFI stub build left to
/// guard against, and the backend is WASM-clean. The `Option` signature is
/// retained for the many existing
/// `let Some(nb) = yang_rs::native_backend() else { /* skip */ }` call
/// sites (their skip arms are now dead but harmless).
pub fn native_backend() -> Option<NativeBoolean> {
    Some(NativeBoolean)
}

// =========================================================================
// boolean() — PR-YR3 vertex provenance + PR-YR4 triangle attribution
// =========================================================================

/// Per-op orientation fix for a kept arrangement triangle, mirroring
/// Cherchi's `booleans.cpp` post-keep flip loops:
/// - Union (`boolUnion`) / Intersection (`boolIntersection`): no flip.
/// - Subtraction (`boolSubtraction`:1480-1483): flip kept tris NOT on
///   solid A's surface (`surface[t][0] != 1`) — the B-surface tris that
///   bound the carved cavity, whose outward normal must point into A.
/// - Xor (`boolXOR`:1506-1509): flip kept tris with any inside bit set
///   (`inside.count() > 0`).
fn flip_for_op(op: BoolOp, la: &LabeledArrangement, t: usize) -> bool {
    match op {
        BoolOp::Union | BoolOp::Intersect => false,
        BoolOp::Subtract => {
            // surface[t][0] set ⟺ solid 0 (A) is in the surface label list.
            let on_a = la.surface[t].iter().any(|&LaInputId(id)| id == 0);
            !on_a
        }
        BoolOp::Xor => la.inside[t].iter().any(|&b| b),
    }
}

/// PR-YR24: Stage-1 NEAR-coplanar input scan (PR-YR26: now the Stage-0
/// DETECTOR, no longer a hard gate).
///
/// Scans A-face × B-face pairs of the two input B-Reps (planar faces only,
/// while their surfaces are still symbolic `Surface::Plane`s — i.e. BEFORE
/// any mesh-level processing) and returns ALL cross pairs that are coplanar
/// within the sub-model-resolution band AND could actually interact
/// (overlapping AABBs), plus the first INTRA-solid pair (which remains the
/// loud unsupported residue).
///
/// **Why this scan exists.** Yang 2025 §4.5.5 requires coplanar face pairs
/// to be detected and resolved by a 2D Boolean at the B-Rep level BEFORE
/// mesh discretization ("it is necessary to check coplanar planes and
/// perform 2D Boolean operations before mesh discretizations",
/// `refs/text/yang2025_hybrid_boolean.txt:717-731`) — Stage 0, roadmap
/// milestone M8. Bit-EXACT coplanar overlaps that reach the arrangement
/// unhandled hit cherchi-rs's loud deferral (`CoplanarPairDeferred`,
/// deviation N17, `arrangements/soup.rs`). But f64 vertex construction
/// leaves femto-scale residuals on faces built on the SAME oblique sketch
/// plane (the KV4-F1 corpus family: R0029, F0016/18/19/21/25), so the EXACT
/// deferral does not catch them; the exact arrangement then faithfully
/// builds sub-f64-ulp sliver patches (all-LPI, all-border, width < 1 ulp)
/// whose in/out classification has no seedable ray origin
/// (`NoExplicitRayOrigin` — the C++ reference `booleans.cpp:504-575` would
/// exit there too). PR-YR24 converted both classes into the loud typed
/// `CoplanarFacesUnsupported` wall; PR-YR26 (M8 slice b) HANDLES the
/// cross-pair planar class via the §4.5.5 overlay (`stage0_preprocess`) and
/// keeps the wall only for the residue (intra pairs, unsupported face
/// shapes, multi-pair faces).
///
/// **The band.** For a candidate pair, with unit normals `n̂a`, `n̂b`
/// (orientation-aligned: `s = sign(n̂a·n̂b)`) and unit-normal plane offsets
/// `d̂ = d/‖n‖` (`n̂·x + d̂ = 0`):
///
/// ```text
/// scale = max |coordinate| over both faces' AABB corners
/// band  = max(TAU_MODEL, scale · TAU_WORK)
/// ```
///
/// and the pair is flagged iff ALL of:
/// 1. offset agreement:  |d̂a − s·d̂b| ≤ band
/// 2. parallel normals:  ‖n̂a × n̂b‖ · extent ≤ band, where `extent` is the
///    diagonal of the union of the two faces' AABBs (an angular tilt θ
///    displaces the planes by at most sin θ · extent over the region where
///    the faces could meet, so this bounds the true plane-to-plane gap by
///    2·band over that region)
/// 3. AABB overlap (each axis, inflated by band) — far-apart faces on the
///    same plane do not interact in the boolean and are NOT flagged
///    (over-deferral avoided).
///
/// Justification: `TAU_MODEL` (1e-7, absolute, governance A14) is the model
/// resolution — two parallel planes closer than `TAU_MODEL` are
/// sub-model-resolution and semantically the same plane (the R0029 family's
/// residuals are ~1e-13..1e-15 absolute at |coord| ~ 6e2, far inside the
/// band, while `MIN_FEATURE_SIZE` = 1e-6 guarantees genuinely distinct
/// model features sit OUTSIDE it). The `scale·TAU_WORK` term (relative
/// 1e-12 ≫ machine ε ≈ 2.2e-16) keeps the band above the f64
/// construction-noise floor for very large models where 1e-7 absolute
/// approaches the coordinate ulp; for |coord| < 1e5 it is inactive.
///
/// Conservative choices: face AABBs are taken over the loop edges'
/// START/END vertices (a curved rim's bulge is not included), which can
/// only UNDER-approximate the AABB — i.e. err toward NOT flagging; a missed
/// pair falls through to the existing loud downstream errors, never to a
/// silent wrong result. Non-planar surfaces are skipped (curved-curved
/// coplanarity is out of this gate's scope; the curved pipeline has its own
/// guards).
///
/// **Intra-solid pairs (the CHAINED KV4-F1 mechanism).** A solid that is
/// itself the output of an exact boolean re-creates near-incidences via
/// exact→f64 output rounding: the surviving A-side and B-side fragments of
/// one near-coplanar plane come back as faces of the SAME solid on planes a
/// few ulps apart (e.g. F0016's second union: operand A carries face pairs
/// with offset residual ~1.6e-16). The next boolean then builds the same
/// sub-ulp sliver patches. So the gate also scans A×A and B×B pairs — with
/// one crucial distinction: BIT-IDENTICAL intra-solid planes are benign (one
/// plane legitimately split into several faces, e.g. an annulus; cherchi's
/// N17 passes exact same-plane adjacency through) and are skipped; only
/// near-but-NOT-bit-identical intra pairs carry the femto signature. Cross
/// (A×B) pairs flag in BOTH cases — bit-exact A×B coplanarity is the
/// original M8 case.
///
/// Intra pairs use a DIFFERENT condition 3: the two fragments of a rounded
/// plane are usually disjoint in-plane regions that never overlap each
/// other, so the cross rule's mutual-overlap test can never fire. The
/// danger is contact by the OTHER solid: crossing both fragments creates
/// two cut lines a few ulps apart (verified on F0018), and even crossing
/// ONE fragment can cut through the rounded seam geometry the split left
/// behind (observed on F0025, where the other solid overlaps only one
/// fragment yet in/out still fails). AABB granularity cannot localize the
/// seam, so the conservative rule is: flag the intra pair iff the other
/// solid's whole-solid AABB overlaps EITHER fragment's AABB
/// (band-inflated). This over-defers a boolean that touches a femto-split
/// plane's region without actually reaching its seam — weighed and
/// accepted: a loud typed M8 deferral is strictly better than
/// `NoExplicitRayOrigin` (P9), and a boolean that stays clear of the
/// region entirely is still NOT flagged.
/// One near-coplanar CROSS (A-face × B-face) pair found by
/// [`scan_near_coplanar`], with the pair's detection `band`.
pub(crate) struct CrossCoplanarPair {
    pub(crate) face_a: usize,
    pub(crate) face_b: usize,
    pub(crate) band: f64,
}

/// Output of [`scan_near_coplanar`]: ALL cross pairs (PR-YR26 Stage-0
/// handles each via the §4.5.5 overlay) plus the FIRST intra-solid pair
/// (still the loud unsupported-residue error — the chained-output class).
pub(crate) struct CoplanarScan {
    pub(crate) cross: Vec<CrossCoplanarPair>,
    pub(crate) intra: Option<(InputId, usize, usize)>,
}

pub(crate) fn scan_near_coplanar(a: &BRep, b: &BRep) -> CoplanarScan {
    /// Per-face plane data: unit normal, unit-normal offset, loop-vertex
    /// AABB, plus the RAW (un-normalized) plane bits for the intra-solid
    /// bit-identical exclusion.
    struct FacePlane {
        n: [f64; 3],
        d: f64,
        lo: [f64; 3],
        hi: [f64; 3],
        raw_bits: [u64; 4],
    }

    fn collect(brep: &BRep) -> Vec<Option<FacePlane>> {
        brep.faces()
            .iter()
            .map(|f| {
                let Surface::Plane { normal, d } = f.surface else {
                    return None;
                };
                let na = normal.as_array();
                let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
                if len < cad_primitives::MIN_FEATURE_SIZE {
                    // Degenerate normal — rejected loudly elsewhere
                    // (`DegenerateFace`); not this gate's job.
                    return None;
                }
                let n = [na[0] / len, na[1] / len, na[2] / len];
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                    for &e in lp {
                        let Some(edge) = brep.edges().get(e as usize) else {
                            continue;
                        };
                        for vi in [edge.start, edge.end] {
                            let Some(v) = brep.vertices().get(vi as usize) else {
                                continue;
                            };
                            let p = v.point.as_array();
                            for k in 0..3 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                        // A `Circle`/`Ellipse` loop edge's endpoints are only
                        // its seam — the swept curve reaches much further. A
                        // disc cap bounded by a single closed circle would
                        // otherwise get a single-POINT AABB (the seam), so a
                        // coplanar disc∩polygon pair is detected only when the
                        // seam happens to overlap the other face. Expand by the
                        // analytic circle box: `center ± r·√(1−n_k²)` per axis.
                        if let Curve::Circle {
                            center,
                            normal,
                            radius,
                        } = edge.curve
                        {
                            let c = center.as_array();
                            let nu = normalize3(normal.as_array());
                            for k in 0..3 {
                                let ext = radius * (1.0 - nu[k] * nu[k]).max(0.0).sqrt();
                                lo[k] = lo[k].min(c[k] - ext);
                                hi[k] = hi[k].max(c[k] + ext);
                            }
                        }
                    }
                }
                if !lo[0].is_finite() {
                    return None;
                }
                Some(FacePlane {
                    n,
                    d: d / len,
                    lo,
                    hi,
                    raw_bits: [
                        na[0].to_bits(),
                        na[1].to_bits(),
                        na[2].to_bits(),
                        d.to_bits(),
                    ],
                })
            })
            .collect()
    }

    /// Conditions 1 (offset agreement) + 2 (parallel normals) for one face
    /// pair; returns the pair's `band` when both hold. Condition 3 (which
    /// AABBs must overlap) differs between cross and intra pairs — see the
    /// scan loops below.
    fn near_coplanar_band(pa: &FacePlane, pb: &FacePlane) -> Option<f64> {
        // scale = max |coordinate| over both faces' AABB corners.
        let mut scale: f64 = 0.0;
        for p in [&pa.lo, &pa.hi, &pb.lo, &pb.hi] {
            for &c in p.iter() {
                scale = scale.max(c.abs());
            }
        }
        let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);

        // 1. Orientation-aligned offset agreement.
        let dot = pa.n[0] * pb.n[0] + pa.n[1] * pb.n[1] + pa.n[2] * pb.n[2];
        let s = if dot >= 0.0 { 1.0 } else { -1.0 };
        if (pa.d - s * pb.d).abs() > band {
            return None;
        }

        // 2. Parallel normals over the pair's geometric extent.
        let cross = [
            pa.n[1] * pb.n[2] - pa.n[2] * pb.n[1],
            pa.n[2] * pb.n[0] - pa.n[0] * pb.n[2],
            pa.n[0] * pb.n[1] - pa.n[1] * pb.n[0],
        ];
        let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let mut ext2 = 0.0;
        for k in 0..3 {
            let e = pa.hi[k].max(pb.hi[k]) - pa.lo[k].min(pb.lo[k]);
            ext2 += e * e;
        }
        if sin * ext2.sqrt() > band {
            return None;
        }
        Some(band)
    }

    /// Band-inflated AABB overlap on every axis.
    fn aabbs_overlap(
        lo_a: &[f64; 3],
        hi_a: &[f64; 3],
        lo_b: &[f64; 3],
        hi_b: &[f64; 3],
        band: f64,
    ) -> bool {
        (0..3).all(|k| lo_a[k] <= hi_b[k] + band && lo_b[k] <= hi_a[k] + band)
    }

    /// Whole-solid AABB over all B-Rep vertices (None for an empty solid).
    fn solid_aabb(brep: &BRep) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in brep.vertices() {
            let p = v.point.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }

    let fa = collect(a);
    let fb = collect(b);

    // Cross pairs (A×B): bit-exact AND near-coplanar both flag; condition 3
    // is mutual AABB overlap (the two faces must be able to interact).
    // PR-YR26: collect ALL such pairs (Stage 0 overlays each), not just the
    // first.
    let mut cross: Vec<CrossCoplanarPair> = Vec::new();
    for (ia, pa) in fa.iter().enumerate() {
        let Some(pa) = pa else { continue };
        for (ib, pb) in fb.iter().enumerate() {
            let Some(pb) = pb else { continue };
            if let Some(band) = near_coplanar_band(pa, pb) {
                if aabbs_overlap(&pa.lo, &pa.hi, &pb.lo, &pb.hi, band) {
                    cross.push(CrossCoplanarPair {
                        face_a: ia,
                        face_b: ib,
                        band,
                    });
                }
            }
        }
    }

    // Intra-solid pairs (A×A, B×B): only near-but-NOT-bit-identical planes
    // flag (bit-identical = one plane split into several faces, benign).
    // Condition 3 is different: the fragments are typically DISJOINT
    // in-plane regions, so the danger is contact by the OTHER solid —
    // flagged iff the other solid's whole-solid AABB overlaps EITHER
    // fragment (see the function docs for the F0018/F0025 evidence and the
    // weighed over-deferral).
    let mut intra: Option<(InputId, usize, usize)> = None;
    'intra: for (input, fp, other) in [
        (InputId::A, &fa, solid_aabb(b)),
        (InputId::B, &fb, solid_aabb(a)),
    ] {
        let Some((olo, ohi)) = other else { continue };
        for (i, pi) in fp.iter().enumerate() {
            let Some(pi) = pi else { continue };
            for (j, pj) in fp.iter().enumerate().skip(i + 1) {
                let Some(pj) = pj else { continue };
                if pi.raw_bits == pj.raw_bits {
                    continue;
                }
                // Spec `m8_intra_opposite_plane_canonicalization` B6: raw
                // plane values that are EXACTLY negated (f64 VALUE compare,
                // so `0.0 == -0.0` matches — bit compare would not) are two
                // orientations of ONE geometric plane. A valid 2-manifold
                // solid's faces on one plane are disjoint in-plane (a
                // stepped solid: lower-step top + overhang bottom), so the
                // arrangement needs no Stage-0 resolution — benign, like
                // the bit-identical case above. `to_yang_brep`'s sign-aware
                // sibling canonicalization produces exactly this form for
                // chained outputs; near-but-NOT-exact negation still walls
                // loud below (B7).
                if (0..4).all(|k| f64::from_bits(pi.raw_bits[k]) == -f64::from_bits(pj.raw_bits[k]))
                {
                    continue;
                }
                if let Some(band) = near_coplanar_band(pi, pj) {
                    // (A PR-KV6b attempt narrowed this to ADJACENT fragments;
                    // it regressed F0017–F0025 from the typed M8 deferral
                    // into NoExplicitRayOrigin failures — the conservative
                    // rule stands. The benign exactly-coplanar class — a
                    // 180° revolve's two caps — is excluded by the
                    // bit-identical rule above instead: producers SNAP their
                    // trig so exact-π caps carry bitwise-equal planes.)
                    if aabbs_overlap(&pi.lo, &pi.hi, &olo, &ohi, band)
                        || aabbs_overlap(&pj.lo, &pj.hi, &olo, &ohi, band)
                    {
                        intra = Some((input, i, j));
                        break 'intra;
                    }
                }
            }
        }
    }
    CoplanarScan { cross, intra }
}

/// PR-YR27 (Finding 3): finite-extent STRICT containment — is `p` strictly
/// inside planar face `fi`'s trimmed region (outer loop minus holes) of
/// `brep`, tested EXACTLY in the face's 2D plane frame?
///
/// Verdicts:
/// - `Some(true)`  — strictly interior: inside the loop arrangement
///   (even-odd over outer + holes) and ON no loop edge,
/// - `Some(false)` — ON a loop edge, or outside,
/// - `None`        — undecidable by this test (curved surface, a curved
///   loop edge — whose chord segment would misrepresent the boundary —
///   or non-finite coordinates). The caller must NOT exclude the face.
///
/// Exactness: the 2D projection `(u, v) = (q·e1, q·e2)` is one LINEAR map
/// applied in f64 and lifted exactly to rationals, so points that are
/// 3D-collinear along a straight loop edge project to EXACTLY 2D-collinear
/// points — the on-boundary rejection cannot be defeated by femto rounding.
/// Loop-vertex off-plane residuals (e.g. a Stage-0 snapped pair face) lie
/// along the face normal, which both frame axes annihilate, so they do not
/// perturb the in-plane region shape.
/// PR-KV7: finite-extent strict containment for a CYLINDER face, along the
/// AXIS only. A chainable boolean output can carry several faces of the SAME
/// infinite cylinder (the two stubs of a drill-through), so the YR27
/// infinite-surface membership ties between them; the axial span breaks the
/// tie exactly like the planar 2D test: the TRUE owning face's loop vertices
/// (rims / arc endpoints / ruling ends — all exactly on the surface) bound an
/// axial interval that strictly contains the centroid of every positive-area
/// triangle attributed to it, while a different same-cylinder face at best
/// touches the boundary. Azimuthal extent is NOT tested: a false candidate
/// that ties axially merely keeps the tie loud (P9-safe), never mis-excludes
/// the owner. `None` for non-cylinder faces / degenerate axes.
fn point_strictly_in_cylinder_face_axially(brep: &BRep, fi: usize, p: [f64; 3]) -> Option<bool> {
    let f = brep.faces().get(fi)?;
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        ..
    } = f.surface
    else {
        return None;
    };
    let a = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let t_of = |q: [f64; 3]| (q[0] - ap[0]) * a[0] + (q[1] - ap[1]) * a[1] + (q[2] - ap[2]) * a[2];
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = brep.edges().get(*e_idx as usize)?;
        for v in [e.start, e.end] {
            let t = t_of(brep.vertices().get(v as usize)?.point.as_array());
            t_min = t_min.min(t);
            t_max = t_max.max(t);
        }
    }
    if !(t_min.is_finite() && t_max.is_finite() && t_min < t_max) {
        return None;
    }
    let t = t_of(p);
    Some(t_min < t && t < t_max)
}

fn point_strictly_in_planar_face(brep: &BRep, fi: usize, p: [f64; 3]) -> Option<bool> {
    use crate::coplanar_overlay::{cross_r, point_in_even_odd, ExactPoint2};
    use dashu::rational::RBig;

    let f = brep.faces().get(fi)?;
    let Surface::Plane { normal, .. } = f.surface else {
        return None;
    };
    let n = normal.as_array();
    if (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let (e1, e2) = ortho_basis(normal);
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let proj = |q: [f64; 3]| -> Option<ExactPoint2> {
        ExactPoint2::from_f64(
            q[0] * e1[0] + q[1] * e1[1] + q[2] * e1[2],
            q[0] * e2[0] + q[1] * e2[1] + q[2] * e2[2],
        )
    };
    let q = proj(p)?;

    let mut edges2: Vec<(ExactPoint2, ExactPoint2)> = Vec::new();
    for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
        for &ei in lp {
            let edge = brep.edges().get(ei as usize)?;
            // A curved loop edge's chord would misrepresent the trimmed
            // boundary — undecidable, never a silent approximation.
            if !matches!(edge.curve, Curve::LineSegment) {
                return None;
            }
            let s = brep.vertices().get(edge.start as usize)?.point.as_array();
            let e = brep.vertices().get(edge.end as usize)?.point.as_array();
            edges2.push((proj(s)?, proj(e)?));
        }
    }

    // Exact ON-closed-segment rejection against every loop edge (strictness:
    // a boundary point is NOT contained).
    for (a, b) in &edges2 {
        if cross_r(a, b, &q) != RBig::ZERO {
            continue;
        }
        let dx = &b.x - &a.x;
        let dy = &b.y - &a.y;
        let t_num = (&q.x - &a.x) * &dx + (&q.y - &a.y) * &dy;
        let len2 = &dx * &dx + &dy * &dy;
        if t_num >= RBig::ZERO && t_num <= len2 {
            return Some(false);
        }
    }

    // Strictly off the boundary: exact even-odd over outer + hole loops
    // (the no-boundary precondition of `point_in_even_odd` now holds).
    Some(point_in_even_odd(&q, &edges2))
}

/// Surface distance of a point `c` to a coincident-cylinder pair, namely the
/// value `abs(dist_to_axis_line minus radius)`, which is zero on the shared
/// cylindrical surface. Used by the membrane resolution to match an
/// overlap-sheet triangle to a [`stage0::PairCylinder`] (the cylinder analog of
/// the planar plane-distance match).
fn centroid_on_cylinder(c: [f64; 3], p: &stage0::PairCylinder) -> f64 {
    let w = [
        c[0] - p.axis_point[0],
        c[1] - p.axis_point[1],
        c[2] - p.axis_point[2],
    ];
    let t = w[0] * p.axis_dir[0] + w[1] * p.axis_dir[1] + w[2] * p.axis_dir[2];
    let perp = [
        w[0] - t * p.axis_dir[0],
        w[1] - t * p.axis_dir[1],
        w[2] - t * p.axis_dir[2],
    ];
    let dist = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    (dist - p.radius).abs()
}

/// PR-5: are `surf0` and `surf1` COINCIDENT cylinders — same axis line
/// (parallel axes, collinear) and equal radius, all within `tol`? Two such
/// cylinders share their entire lateral surface and `ssi_rs::intersect` refuses
/// them (`DegenerateInput`), so the caller must NOT route their edges to SSI.
fn cylinders_are_coincident(surf0: Surface, surf1: Surface, tol: f64) -> bool {
    let (
        Surface::Cylinder {
            axis_point: ap0,
            axis_dir: ad0,
            radius: r0,
        },
        Surface::Cylinder {
            axis_point: ap1,
            axis_dir: ad1,
            radius: r1,
        },
    ) = (surf0, surf1)
    else {
        return false;
    };
    let ad0 = normalize3(ad0.as_array());
    let ad1 = normalize3(ad1.as_array());
    // Parallel axes (|cross| ≈ 0).
    let cross = [
        ad0[1] * ad1[2] - ad0[2] * ad1[1],
        ad0[2] * ad1[0] - ad0[0] * ad1[2],
        ad0[0] * ad1[1] - ad0[1] * ad1[0],
    ];
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    if sin > tol.max(cad_primitives::TAU_MODEL) {
        return false;
    }
    // Equal radius.
    if (r0 - r1).abs() > tol {
        return false;
    }
    // Collinear axes: ap1 lies on ap0's axis line (perpendicular distance ≈ 0).
    let ap0a = ap0.as_array();
    let ap1a = ap1.as_array();
    let w = [ap1a[0] - ap0a[0], ap1a[1] - ap0a[1], ap1a[2] - ap0a[2]];
    let tw = w[0] * ad0[0] + w[1] * ad0[1] + w[2] * ad0[2];
    let perp = [w[0] - tw * ad0[0], w[1] - tw * ad0[1], w[2] - tw * ad0[2]];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= tol
}

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// **M3 functional pipeline** (replaces the PR-YR3/YR4 spatial-match +
/// majority-vote substitute, now a `#[cfg(test)]` differential oracle):
///
/// 0. **XOR is deferred (spec §Scope)** — its symmetric-difference result
///    is multi-shell / has a void that `reconstruct_topology` cannot
///    reassemble yet. `boolean()` errors loudly with `UnsupportedOp` once it
///    sees a non-empty XOR kept-set (a degenerate XOR with nothing to
///    reassemble still trivially yields an empty result).
/// 1. Obtain the real Stage-2 [`LabeledArrangement`] from
///    `backend.labeled_arrangement(..)` (full arrangement mesh +
///    per-triangle `surface`/`inside`/`patch` labels).
/// 2. **I6 weld** — the C++ producer does NOT always weld coincident
///    vertices (e.g. A@[0,0,0]/B@[0.7,0.3,0.4] emits a bit-exact duplicate
///    vertex used by shared triangles), so yang welds: map each vertex to
///    the *original index* of its first bit-identical occurrence. yang's
///    index-based adjacency then sees coincident points as one index. A
///    kept triangle that welds to a repeated index is a zero-area sliver at
///    that coincident point — dropped (no surface/volume; its edges pair up
///    so the output stays watertight). Two *distinct* surviving triangles
///    that weld to the same 3 indices are genuinely coincident faces →
///    `NonManifoldInput` (the a4 bit-exact-coincident-vertex case).
/// 3. `keep = la.keep_set(op)` — Stage 4 face survival.
/// 4. Compact the welded kept tris into a fresh sub-mesh (the output mesh).
/// 5. **Geometric face resolution** (Stage 6) per kept tri → a FULL
///    `TriangleAttributionMap` (every entry `Some`). A SURVIVING
///    multi-solid `surface[t]` (a §4.5.5 overlap-sheet triangle the (3b)
///    side rule kept) attributes to input A — the dedup survivor's side,
///    whose winding it carries (PR-YR26; B's coincident face has the same
///    plane, so the inherited output surface is identical). For a
///    *non-degenerate* (positive-area) triangle: pick the unique labeled-solid
///    face plane within `TAU_WORK` of the centroid; no match / a genuine tie →
///    `FaceResolutionFailed` (F3). For a *degenerate* (zero-area sliver, kept
///    because its edges pair into the watertight result) triangle: attribute
///    to the LOWEST labeled-solid face index within `TAU_WORK` (its centroid
///    sits on a solid edge, so the two adjacent planes tie — harmless for a
///    zero-area tri; never F3). Never a silent `None` (P9).
/// 6. `reconstruct_topology(..)` — flood-fill patches, walk boundary
///    cycles, inherit input-face `Surface`; full attribution ⇒ closed
///    boundary cycles ⇒ watertight 2-manifold output.
///
/// **N4 (provenance):** before the geometric resolution in step 5, a kept
/// triangle is attributed DIRECTLY from cherchi's per-triangle provenance
/// (`LabeledArrangement.source` → the parent input triangle → its B-Rep face via
/// the Stage-1 `tri_face` map) whenever that is unambiguous. The geometric path
/// remains the fallback. See [`provenance_face_reason`].
///
/// N4 helper: resolve a kept arrangement triangle's B-Rep face from cherchi's
/// per-triangle provenance (`§4.2.3`), not geometric centroid-proximity.
///
/// The triangle is attributed to `surface_input` (A or B — the side the keep-rule
/// kept it on; for a coplanar overlap sheet the §4.5.5 survivor convention picks
/// A). We select that side's parent from `source` and resolve it through that
/// input mesh's per-triangle face map (`tri_face_a` for A, `tri_face_b` for B).
/// This handles BOTH a non-coplanar triangle (its only parent) AND a coplanar
/// overlap sheet (the parent on the kept side). Returns `None` (→ geometric
/// fallback) when that side has no parent in `source`, the parent is beyond
/// the face map (a Stage-0 path that did not emit provenance, or a lineage-less
/// `from_mesh` / boolean-output input), or the parent maps to the `u32::MAX`
/// sentinel (a producer that emitted a map but could not attribute THAT
/// triangle — e.g. a coincident-cylinder band-strip column with no covering
/// arc-patch face). Never a wrong face.
/// Why N4 provenance attribution could not name a face for a kept triangle —
/// the exact reason the Stage-6 geometric fallback is still reached. Used by the
/// `YANG_N4_FALLBACK_PROBE` measurement (N4 retirement: prove the geometric path
/// is dead in production, or name the producers that still leave a triangle
/// un-provenanced).
#[derive(Debug, Clone, Copy)]
enum ProvMiss {
    /// The kept triangle's `source` has no parent triangle from this input
    /// (e.g. a cut/arrangement triangle with only the OTHER input's lineage).
    /// On a lineage-carrying input this is a producer FAULT (loud).
    NoSourceEntry,
    /// This input emitted NO provenance map at all (empty `tri_face`) — a
    /// LINEAGE-LESS input: a yang boolean OUTPUT chained directly back in,
    /// or a `from_mesh` B-Rep. This is the documented geometric-resolution
    /// path (task #53), NOT a fault.
    NoLineage,
    /// The map is present but the parent-triangle index lies beyond it —
    /// the producer emitted a TOO-SHORT provenance map (fault, loud).
    NoMap,
    /// The producer minted this triangle but could not attribute it to a face
    /// (`u32::MAX` sentinel — e.g. the coincident-cylinder band strip with no
    /// covering arc column). Fault, loud.
    Sentinel,
}

/// N4 (§4.2.3): map a kept triangle to its owning B-Rep face via the
/// arrangement's per-triangle provenance. `Ok(face)` on a hit; `Err(reason)`
/// records WHY it missed — `NoLineage` is the one non-fault reason (the
/// input never had a provenance map), everything else is loud at the caller
/// (task #53, spec `specs/n4_retire_stage6_fallback.md`).
fn provenance_face_reason(
    source: &[(LaInputId, u32)],
    surface_input: InputId,
    tri_face_a: &[u32],
    tri_face_b: &[u32],
) -> Result<u32, ProvMiss> {
    let (want_k, tf): (u32, &[u32]) = match surface_input {
        InputId::A => (0, tri_face_a),
        InputId::B => (1, tri_face_b),
    };
    if tf.is_empty() {
        return Err(ProvMiss::NoLineage);
    }
    let &(_, local) = source
        .iter()
        .find(|&&(LaInputId(k), _)| k == want_k)
        .ok_or(ProvMiss::NoSourceEntry)?;
    match tf.get(local as usize).copied() {
        None => Err(ProvMiss::NoMap),
        Some(f) if f == u32::MAX => Err(ProvMiss::Sentinel),
        Some(f) => Ok(f),
    }
}

/// KV15 (spec `kv15_mixed_operand_planar_near_weld` §3): per-vertex weld
/// eligibility for MIXED operands. A vertex is CURVED-ADJACENT (ineligible
/// for the near-weld, `true` in the returned vec) when ANY incident
/// arrangement triangle fails to prove planar descent: empty provenance
/// (`source[t]` empty — e.g. the sidecar parity producer, spec W4),
/// out-of-range / `u32::MAX`-sentinel `tri_face` entries, an out-of-range
/// face index, or a face whose surface is not `Surface::Plane`
/// (`face_planar` returns `Some(false)` — or `None` for a bad index).
/// Conservative by construction: only positively-proven all-planar descent
/// yields eligibility.
fn kv15_curved_touch(
    n_verts: usize,
    tris: &[[u32; 3]],
    source: &[Vec<(LaInputId, u32)>],
    tri_face_a: &[u32],
    tri_face_b: &[u32],
    face_planar: impl Fn(u32, u32) -> Option<bool>,
) -> Vec<bool> {
    let mut curved = vec![false; n_verts];
    for (t, tri) in tris.iter().enumerate() {
        let src = source.get(t).map(Vec::as_slice).unwrap_or(&[]);
        let tri_curved = src.is_empty()
            || src.iter().any(|&(LaInputId(k), local)| {
                let tf: &[u32] = if k == 0 { tri_face_a } else { tri_face_b };
                match tf.get(local as usize).copied() {
                    Some(fi) if fi != u32::MAX => !matches!(face_planar(k, fi), Some(true)),
                    _ => true,
                }
            });
        if tri_curved {
            for &v in tri {
                if let Some(slot) = curved.get_mut(v as usize) {
                    *slot = true;
                }
            }
        }
    }
    curved
}

/// KV15 (spec §3): near-union among planar-only weld roots — the identical
/// grid, per-pair band `TAU_WORK·(1+max|coord|)`, and min-index-survivor
/// rule as the all-planar KV10 weld (spec I2/I4). `weld` enters as the
/// bit-exact weld map (each entry pointing at its cluster's original
/// representative) and leaves fully resolved. Roots flagged in
/// `root_curved` never participate (kv9 junction-duplicate protection).
fn kv15_near_weld_pass(verts: &[Point3], weld: &mut [u32], root_curved: &[bool]) {
    use std::collections::HashMap;
    let mut parent: Vec<u32> = weld.to_vec();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let scale = verts
        .iter()
        .flat_map(|v| v.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for i in 0..verts.len() as u32 {
        if weld[i as usize] != i || root_curved[i as usize] {
            continue;
        }
        let p = verts[i as usize].as_array();
        let key = [cell(p[0]), cell(p[1]), cell(p[2])];
        for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                for dz in -1..=1i64 {
                    let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                        continue;
                    };
                    for &j in occ {
                        let q = verts[j as usize].as_array();
                        let pair_band = cad_primitives::TAU_WORK
                            * (1.0 + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                        if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                            if ri != rj {
                                parent[ri.max(rj) as usize] = ri.min(rj);
                            }
                        }
                    }
                }
            }
        }
        grid.entry(key).or_default().push(i);
    }
    for w in weld.iter_mut() {
        *w = find(&mut parent, *w);
    }
}

/// M8 Stage-0 operand dump — diagnostic-only observer (spec
/// `specs/m8_stage0_inputcheck_clean_emission.md` §6). Env-gated on
/// `YANG_STAGE0_DUMP_DIR`; zero-cost when unset (never set in production or
/// WASM). Writes, per boolean call, the EXACT operand meshes handed to the
/// backend — plus, when Stage 0 rewrote them, each solid's pre-Stage-0
/// Stage-1 mesh (`_pre`) and the `tri_face` provenance maps — so the
/// five-axiom census can split defects introduced-vs-inherited and join
/// offenders back to B-Rep faces. Vertex coordinates use f64 `Display`
/// (shortest round-trip), so the dump is bit-faithful. Write failures are
/// reported on stderr and never affect the boolean (read-only, spec I6).
fn stage0_dump(
    op: BoolOp,
    stage0: Option<&stage0::Stage0>,
    cyl_pair_count: usize,
    mesh_a: &Mesh,
    mesh_b: &Mesh,
    pre_a: &Mesh,
    pre_b: &Mesh,
) {
    let Some(dir) = std::env::var_os("YANG_STAGE0_DUMP_DIR") else {
        return;
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    // Process-global op counter: yang-rs has no case identity; harnesses
    // namespace by pointing the env var at a per-case directory.
    static OP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = OP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::path::PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "[stage0-dump] create_dir_all({}) failed: {e}",
            dir.display()
        );
        return;
    }
    let op_name = match op {
        BoolOp::Union => "union",
        BoolOp::Intersect => "intersect",
        BoolOp::Subtract => "subtract",
        BoolOp::Xor => "xor",
    };
    let stem = format!("{n:03}_{op_name}");
    let write_obj = |suffix: &str, m: &Mesh| {
        let path = dir.join(format!("{stem}_{suffix}.obj"));
        let mut out = String::new();
        for v in &m.verts {
            out.push_str(&format!("v {} {} {}\n", v.x(), v.y(), v.z()));
        }
        for t in &m.tris {
            out.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
        }
        if let Err(e) = std::fs::write(&path, out) {
            eprintln!("[stage0-dump] write {} failed: {e}", path.display());
        }
    };
    write_obj("a", mesh_a);
    write_obj("b", mesh_b);
    let mut meta = format!(
        "op: {op_name}\nstage0: {}\ncyl_pairs: {cyl_pair_count}\n\
         mesh_a: {} verts / {} tris\nmesh_b: {} verts / {} tris\n",
        stage0.is_some(),
        mesh_a.verts.len(),
        mesh_a.tris.len(),
        mesh_b.verts.len(),
        mesh_b.tris.len(),
    );
    if let Some(s0) = stage0 {
        write_obj("a_pre", pre_a);
        write_obj("b_pre", pre_b);
        let write_csv = |suffix: &str, tf: &[u32]| {
            let path = dir.join(format!("{stem}_{suffix}.tri_face.csv"));
            let mut out = String::new();
            for f in tf {
                out.push_str(&format!("{f}\n"));
            }
            if let Err(e) = std::fs::write(&path, out) {
                eprintln!("[stage0-dump] write {} failed: {e}", path.display());
            }
        };
        write_csv("a", &s0.tri_face_a);
        write_csv("b", &s0.tri_face_b);
        for p in &s0.pairs {
            meta.push_str(&format!(
                "pair_plane: face_a={} face_b={} opposite={} n=({},{},{}) d={} band={}\n",
                p.face_a, p.face_b, p.opposite, p.n[0], p.n[1], p.n[2], p.d, p.band,
            ));
        }
    }
    let meta_path = dir.join(format!("{stem}_meta.txt"));
    if let Err(e) = std::fs::write(&meta_path, meta.as_bytes()) {
        eprintln!("[stage0-dump] write {} failed: {e}", meta_path.display());
    }
}

/// Case-IV phantom guard analysis (spec `yang_case_iv_phantom_guard`,
/// M8 increment 15): the forced minimum rim segment count over all
/// ANALYTICALLY DISJOINT cylinder-face pairs (A×B) whose Stage-1 chord
/// bands could otherwise overlap the gap between the surfaces (Yang Fig. 8
/// Case IV — the meshes would intersect where the surfaces do not,
/// manufacturing a phantom intersection curve; measured F0088 op 4).
///
/// For each pair: the axis-line distance gives the analytic gap (external
/// `d − r_a − r_b` for any axis pose; nested `r_large − d − r_small` for
/// parallel axes). A positive gap demands the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))` —
/// the Stage-1 sagitta, A14.3 single source; the factor-2 margin keeps the
/// combined band strictly clear, and a finer N is always chord-valid). Far
/// pairs derive a tiny N that the natural Stage-1 `max()` absorbs — the
/// guard is self-limiting, no mode branch. True near-tangency (N would
/// exceed 4096) yields no requirement: the loud Stage-3 `AmbiguousCurve`
/// remains the tripwire (P9 — never silently proceed with phantom
/// topology).
/// The Case-IV pairwise requirement of two cylinder surfaces (spec
/// `yang_case_iv_phantom_guard`): `None` unless the pair is analytically
/// disjoint with a practical derived N — the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))`, the
/// Stage-1 sagitta; the factor-2 margin keeps the combined chord band
/// strictly clear of the gap, and a finer N is always chord-valid). Shared
/// by the `boolean()` cross-pair guard AND Stage 1's intra-solid fold.
fn cyl_pair_phantom_n(
    (pa, da, ra): (Point3, Vector3, f64),
    (pb, db, rb): (Point3, Vector3, f64),
) -> Option<usize> {
    let ua = normalize3(da.as_array());
    let ub = normalize3(db.as_array());
    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
    let cx = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    // Axis-line distance: skew/crossing axes project the offset onto the
    // common normal; parallel axes take the perpendicular point-line
    // distance.
    let (parallel, d_axes) = if cross_norm > 1e-12 {
        let d = (w[0] * cx[0] + w[1] * cx[1] + w[2] * cx[2]).abs() / cross_norm;
        (false, d)
    } else {
        let t = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
        let perp = [w[0] - t * ua[0], w[1] - t * ua[1], w[2] - t * ua[2]];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        (true, d)
    };
    let external = d_axes - (ra + rb);
    let nested = if parallel {
        ra.max(rb) - d_axes - ra.min(rb)
    } else {
        f64::NEG_INFINITY
    };
    let gap = external.max(nested);
    if gap.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // surfaces intersect / NaN (degenerate input): real curve or no-op
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(ra, n) + sag(rb, n) > gap / 2.0 {
        n += 1;
        if n > 4096 {
            // True near-tangency: no finite practical N — leave the loud
            // Stage-3 stop as the tripwire.
            return None;
        }
    }
    Some(n)
}

fn phantom_min_rim_segments(a: &BRep, b: &BRep) -> Option<usize> {
    let cyls = |brep: &BRep| -> Vec<(Point3, Vector3, f64)> {
        brep.faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => Some((axis_point, axis_dir, radius)),
                _ => None,
            })
            .collect()
    };
    let (ca, cb) = (cyls(a), cyls(b));
    let mut req: Option<usize> = None;
    // CROSS pairs only (A×B): the two operands' meshes must not intersect
    // where their surfaces do not (the measured F0088 cut-4 class).
    // INTRA-solid pairs are folded into Stage 1's own N selection
    // (`stage1_tessellate_inner` — M8 increment 16), where EVERY
    // tessellation of the solid picks them up (conversion, Stage-0 rebuilds,
    // this guard's rebuilds), so they need no handling here.
    for &sa in &ca {
        for &sb in &cb {
            if let Some(n) = cyl_pair_phantom_n(sa, sb) {
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    // Self-limiting gate: a requirement BOTH solids' natural Stage-1 N
    // already satisfies is dropped, keeping the common path byte-identical
    // (and rebuild-free). `natural_rim_n` mirrors the Stage-1 N derivation
    // (chord bound over all rim circles, N from the max radius).
    let natural_rim_n = |brep: &BRep| -> usize {
        let Some(d_eps) = curved_chord_bound(brep.edges()) else {
            return usize::MAX; // no circles: nothing to boost
        };
        let max_r = brep
            .edges()
            .iter()
            .filter_map(|e| match e.curve {
                Curve::Circle { radius, .. } => Some(radius),
                _ => None,
            })
            .fold(0.0f64, f64::max);
        let mut n = 3usize;
        if d_eps > 0.0 {
            while max_r * (1.0 - (std::f64::consts::PI / n as f64).cos()) > d_eps {
                n += 1;
            }
        }
        n
    };
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[phantom-guard] req={req:?} natural=({},{}) gated={gated:?} \
             cyl_faces=({},{})",
            natural_rim_n(a),
            natural_rim_n(b),
            a.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
            b.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
        );
    }
    gated
}

/// N2/F0059 epic increment 2, BANKED-UNWIRED (spec
/// `yang_rim_junction_insertion`): per full-circle rim edge of `x`, the
/// exact points where that rim circle transversally CROSSES one of `y`'s
/// cylinder laterals — the §4.3.3 Case-IV junction points that Stage-1
/// must carry as rim samples so the mesh-level seam chains can terminate
/// exactly at the junctions (the truncated-Steinmetz cap-lobe corners).
///
/// v1 closed-form scope (A13.3/P8 — no ad-hoc root finding): only
/// laterals whose axis is PARALLEL to the rim plane contribute (their
/// section in the rim plane is two lines ⇒ circle∩line quadratics; the
/// F0059 class). Transversal-axis laterals (ellipse section, quartic) and
/// non-cylinder surfaces are out of scope and keep today's loud walls.
/// Tangent grazes are excluded by a DERIVED resolution gate: a root pair
/// closer than `TAU_MODEL` along the section line is one model point
/// (A14.2), i.e. the §4.3.3 tangency class — not a transversal crossing.
///
/// Returned points satisfy `|‖p−c‖−r| ≤ TAU_WORK` and lie on the
/// contributing lateral to fp accuracy (unit-asserted). Deterministic:
/// faces in index order, both section lines, roots in ascending-t order.
fn rim_junctions_against(x: &BRep, y: &BRep) -> std::collections::BTreeMap<u32, Vec<Point3>> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let add = |a: [f64; 3], b: [f64; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let scl = |a: [f64; 3], s: f64| [a[0] * s, a[1] * s, a[2] * s];
    let crs = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    // The lateral's axial extent, from the Circle edges its loops carry
    // (both rims project onto the axis; a lateral without circle loop
    // edges yields None → skipped, loud walls preserved).
    let lateral_extent = |brep: &BRep, f: &BRepFace, ap: [f64; 3], d: [f64; 3]| {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            if let Curve::Circle { center, .. } = brep.edges()[ei as usize].curve {
                let z = dot(sub(center.as_array(), ap), d);
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        (lo < hi).then_some((lo, hi))
    };

    let probe = std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some();
    if probe {
        let full_rims = x
            .edges()
            .iter()
            .filter(|e| e.start == e.end && matches!(e.curve, Curve::Circle { .. }))
            .count();
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for f in y.faces() {
            let k = match f.surface {
                Surface::Plane { .. } => "plane",
                Surface::Cylinder { .. } => "cyl",
                Surface::Cone { .. } => "cone",
                Surface::Sphere { .. } => "sphere",
                Surface::Torus { .. } => "torus",
            };
            *kinds.entry(k).or_default() += 1;
        }
        eprintln!(
            "[rim-junction] x: edges={} full_circle_rims={full_rims}; y faces: {kinds:?}",
            x.edges().len()
        );
        let mut ekinds: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for e in x.edges() {
            let k = match e.curve {
                Curve::Circle { .. } => {
                    if e.start == e.end {
                        "circle-closed".to_string()
                    } else {
                        "circle-arc".to_string()
                    }
                }
                Curve::LineSegment => "line".to_string(),
                ref other => format!("{other:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap_or("?")
                    .to_string(),
            };
            *ekinds.entry(k).or_default() += 1;
        }
        eprintln!("[rim-junction] x edge kinds: {ekinds:?}");
    }
    let mut out: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    // Rim geometry retained for the §4b coaxial propagation post-pass.
    let mut rims: Vec<RimDesc> = Vec::new();
    for (ei, e) in x.edges().iter().enumerate() {
        let Curve::Circle {
            center,
            normal,
            radius: r,
        } = e.curve
        else {
            continue;
        };
        let n = normalize3(normal.as_array());
        let c = center.as_array();
        // Increment 4 (measured scope correction): partial-revolve rims are
        // ARC edges — candidates are filtered to the CCW sweep window
        // (stage-1 arc-chain convention) by `point_in_rim_sweep`, which
        // also rejects candidates coinciding with the rim's own B-Rep
        // vertices (arc endpoints / the closed rim's seam): such a
        // junction already IS a mesh vertex, and inserting its twin would
        // trip the uniform-coincidence stop (the seam sits at ring slot 0).
        let arc = if e.start != e.end {
            Some((
                x.vertices()[e.start as usize].point.as_array(),
                x.vertices()[e.end as usize].point.as_array(),
            ))
        } else {
            None
        };
        let rim = RimDesc {
            edge: ei as u32,
            c,
            n,
            r,
            seam: x.vertices()[e.start as usize].point.as_array(),
            arc,
        };
        // Increment 4 v1 scope (demonstrated need — the whole measured
        // class is CONE-band lathes): the PLANE arm fires only on rims
        // flanked by ≥1 cone face. Cylinder-rim × plane-face junctions
        // have no demanding case, and the corpus proves that population
        // healthy without insertion (F0047/R0006/R0075/F0081 were CORRECT
        // pre-arm and regressed under it; R0091's cut-tool rim insertions
        // unmask the banked-§3b unverifiable-χ path). The LATERAL arm
        // (the F0059 cylinder class) is independent and unchanged.
        let cone_flanked = x.faces().iter().any(|f| {
            matches!(f.surface, Surface::Cone { .. })
                && f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|&le| le == ei as u32)
        });
        let mut pts: Vec<Point3> = Vec::new();
        // Shared circle∩line quadratic for a line (q0 + t·u) in the rim
        // plane: t² + 2t·(q0−c)·u + |q0−c|² − r² = 0. `None` = miss or
        // graze (derived tangency gate, A14.2: roots closer than model
        // resolution are ONE point, not two transversal crossings).
        let circle_line_roots = |q0: [f64; 3], u: [f64; 3]| -> Option<[f64; 2]> {
            let m = sub(q0, c);
            let bq = dot(m, u);
            let cq = dot(m, m) - r * r;
            let disc = bq * bq - cq;
            if disc <= 0.0 {
                return None; // no crossing / exact tangent
            }
            let sq = disc.sqrt();
            if 2.0 * sq < cad_primitives::TAU_MODEL {
                return None;
            }
            Some([-bq - sq, -bq + sq])
        };
        for f in y.faces() {
            match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius: rb,
                } => {
                    let d = normalize3(axis_dir.as_array());
                    // v1: axis parallel to the rim plane (same floor as the
                    // phantom guard's axis-parallel test).
                    if dot(n, d).abs() > 1e-12 {
                        continue;
                    }
                    let ap = axis_point.as_array();
                    let Some((z_lo, z_hi)) = lateral_extent(y, f, ap, d) else {
                        continue;
                    };
                    // Signed axis-to-rim-plane distance; |δ| ≥ r_b ⇒ empty
                    // or a plane-tangent lateral (the tangency class —
                    // skipped).
                    let delta = dot(n, sub(ap, c));
                    if delta.abs() >= rb {
                        continue;
                    }
                    // Section of the lateral in the rim plane: two lines
                    // parallel to the axis at in-plane offset ±√(r_b²−δ²)
                    // from the axis foot.
                    let w_half = (rb * rb - delta * delta).sqrt();
                    let foot = sub(ap, scl(n, delta));
                    let eo = normalize3(crs(d, n));
                    for sgn in [-1.0f64, 1.0] {
                        let q0 = add(foot, scl(eo, sgn * w_half));
                        let Some(roots) = circle_line_roots(q0, d) else {
                            continue;
                        };
                        for t in roots {
                            let pj = add(q0, scl(d, t));
                            // Inside the lateral's axial extent; the
                            // ±TAU_WORK slack keeps boundary-of-extent
                            // triple junctions (rim ∩ lateral ∩ far cap —
                            // the F0059 corners).
                            let z = dot(sub(pj, ap), d);
                            if z < z_lo - cad_primitives::TAU_WORK
                                || z > z_hi + cad_primitives::TAU_WORK
                            {
                                continue;
                            }
                            if !point_in_rim_sweep(&rim, pj) {
                                continue;
                            }
                            let pjp = Point3::new(pj[0], pj[1], pj[2]);
                            // Cross-arm dedup at model resolution (two
                            // laterals / both lines can meet the rim at one
                            // triple point).
                            let dup = pts.iter().any(|q| {
                                let qa = q.as_array();
                                let dd = sub(qa, pj);
                                dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                            });
                            if !dup {
                                pts.push(pjp);
                            }
                        }
                    }
                }
                // Increment 4 §4a (spec v1 table row 2, promoted): a PLANE
                // face sections the rim plane in a single line — the
                // coaxial cone-band junction class (R0017 et al.).
                Surface::Plane { normal: m, d } => {
                    if !cone_flanked {
                        continue; // v1 scope: cone-band rims only (see above)
                    }
                    let ma = m.as_array();
                    let mlen = dot(ma, ma).sqrt();
                    if mlen <= 0.0 {
                        continue;
                    }
                    let mh = scl(ma, 1.0 / mlen);
                    let dh = d / mlen;
                    let ndm = dot(n, mh);
                    let denom = 1.0 - ndm * ndm;
                    // Parallel/coincident planes have no transversal
                    // section line (same 1e-12 floor class as the lateral
                    // arm's axis test).
                    if denom <= 1e-12 {
                        continue;
                    }
                    // v1: polygonal faces only — every loop edge a
                    // LineSegment (arc-bounded caps keep today's walls).
                    let Some(face2d) = planar_face_segments(y, f, mh) else {
                        continue;
                    };
                    // Line P∩F: q0 lies in BOTH planes, direction u = n×m̂.
                    let alpha = -(dot(mh, c) + dh) / denom;
                    let mperp = sub(mh, scl(n, ndm));
                    let q0 = add(c, scl(mperp, alpha));
                    let u = normalize3(crs(n, mh));
                    let Some(roots) = circle_line_roots(q0, u) else {
                        continue;
                    };
                    for t in roots {
                        let pj = add(q0, scl(u, t));
                        // Within the face extents: boundary-inclusive
                        // (±TAU_WORK) 2D containment — the plane analog of
                        // the lateral arm's z-extent slack.
                        if !point_in_planar_face(&face2d, pj) {
                            continue;
                        }
                        if !point_in_rim_sweep(&rim, pj) {
                            continue;
                        }
                        let pjp = Point3::new(pj[0], pj[1], pj[2]);
                        let dup = pts.iter().any(|q| {
                            let qa = q.as_array();
                            let dd = sub(qa, pj);
                            dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                        });
                        if !dup {
                            pts.push(pjp);
                        }
                    }
                }
                _ => continue,
            }
        }
        if !pts.is_empty() {
            out.insert(ei as u32, pts);
        }
        rims.push(rim);
    }

    // §4b coaxial azimuth propagation: Stage-1 band strips
    // (`tessellate_cone_frustum_band`, the cylinder tube, the partial-arc
    // strips) pair rims ring-for-ring, so a junction azimuth inserted on
    // ONE rim of a coaxial stack must exist on ALL of them (where their
    // sweep covers it) — otherwise the stack's sample counts diverge and
    // the strip stops loudly.
    if !out.is_empty() {
        // Group rims by axis line: parallel normals (1e-12 floor) with
        // centers on one line (TAU_MODEL off-axis budget).
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..rims.len() {
            let (ci, ni) = (rims[i].c, rims[i].n);
            let mut placed = false;
            for g in groups.iter_mut() {
                let (cj, nj) = (rims[g[0]].c, rims[g[0]].n);
                let cx = crs(ni, nj);
                if dot(cx, cx).sqrt() > 1e-12 {
                    continue;
                }
                let w = sub(ci, cj);
                let along = dot(w, nj);
                let off = sub(w, scl(nj, along));
                if dot(off, off).sqrt() > cad_primitives::TAU_MODEL {
                    continue;
                }
                g.push(i);
                placed = true;
                break;
            }
            if !placed {
                groups.push(vec![i]);
            }
        }
        for g in &groups {
            if !g.iter().any(|&i| out.contains_key(&rims[i].edge)) {
                continue;
            }
            // Vocabulary gate: every operand face touching a group rim
            // must be a Cone/Cylinder/Plane — the surfaces whose Stage-1
            // tessellation consumes shared rim rings. A torus/sphere band
            // stack keeps today's loud walls (never a half-inserted
            // stack).
            let rim_set: std::collections::BTreeSet<u32> =
                g.iter().map(|&i| rims[i].edge).collect();
            let vocab_ok = x.faces().iter().all(|f| {
                let touches = f
                    .outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|e| rim_set.contains(e));
                !touches
                    || matches!(
                        f.surface,
                        Surface::Cone { .. } | Surface::Cylinder { .. } | Surface::Plane { .. }
                    )
            });
            if !vocab_ok {
                for &i in g {
                    out.remove(&rims[i].edge);
                }
                continue;
            }
            // One shared frame about the group axis (g[0] is the
            // lowest-index rim — deterministic, I4). ALL window / dedup
            // decisions below are made in ANGLE space with ONE shared
            // tolerance `th_eps = TAU_MODEL / r_min` — per-radius chord
            // tolerances would let band-partner arcs (which share their
            // sweep window) disagree by a point and stop the Stage-1
            // strip loudly on a count mismatch (the R0019 161-vs-162
            // wall). Angle-space decisions are conformal by construction.
            let (c0, axis) = (rims[g[0]].c, rims[g[0]].n);
            let (b1v, b2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
            let (b1, b2) = (b1v.as_array(), b2v.as_array());
            let two_pi = 2.0 * std::f64::consts::PI;
            let group_az = |p: [f64; 3]| -> f64 {
                let w = sub(p, c0);
                dot(w, b2).atan2(dot(w, b1)).rem_euclid(two_pi)
            };
            let r_min = g
                .iter()
                .map(|&i| rims[i].r)
                .fold(f64::INFINITY, f64::min)
                .max(cad_primitives::MIN_FEATURE_SIZE);
            let th_eps = cad_primitives::TAU_MODEL / r_min;
            // A rim's admissible azimuth window, with the ±th_eps margin
            // excluding its own B-Rep vertices (arc endpoints / seam).
            let in_window = |rim: &RimDesc, th: f64| -> bool {
                match rim.arc {
                    Some((sp, ep)) => {
                        // Own-orientation sweep mapped through the GROUP
                        // frame: the CCW window about rim.n runs start->end;
                        // in the group frame it runs the same way when
                        // rim.n aligns with the group axis, reversed when
                        // anti-aligned.
                        let a0 = group_az(sp);
                        let a1 = group_az(ep);
                        let aligned = dot(rim.n, axis) >= 0.0;
                        let (w0, w1) = if aligned { (a0, a1) } else { (a1, a0) };
                        let sweep = (w1 - w0).rem_euclid(two_pi);
                        let off = (th - w0).rem_euclid(two_pi);
                        off > th_eps && off < sweep - th_eps
                    }
                    None => {
                        let off = (th - group_az(rim.seam)).rem_euclid(two_pi);
                        off > th_eps && off < two_pi - th_eps
                    }
                }
            };
            // Cluster ALL direct-junction azimuths at th_eps. Each cluster
            // is one physical junction column; its representative azimuth
            // is the smallest member (deterministic).
            let mut annotated: Vec<(f64, usize, Point3)> = Vec::new();
            for &i in g {
                if let Some(pts) = out.get(&rims[i].edge) {
                    for pt in pts {
                        annotated.push((group_az(pt.as_array()), i, *pt));
                    }
                }
            }
            annotated.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut clusters: Vec<Vec<(f64, usize, Point3)>> = Vec::new();
            for a in annotated {
                match clusters.last_mut() {
                    Some(cl) if (a.0 - cl.last().unwrap().0).abs() <= th_eps => cl.push(a),
                    _ => clusters.push(vec![a]),
                }
            }
            // Wrap-around: the first and last clusters may be one junction
            // column split at the 0/2pi cut.
            if clusters.len() > 1 {
                let lo = clusters.first().unwrap().first().unwrap().0;
                let hi = clusters.last().unwrap().last().unwrap().0;
                if (lo + two_pi - hi).abs() <= th_eps {
                    let merged = clusters.pop().unwrap();
                    clusters[0].extend(merged);
                }
            }
            // Rebuild every rim's list from the clusters: the rim's own
            // direct point where it has one (the exact junction position),
            // else the on-circle point at the cluster representative.
            for &i in g {
                let rim = &rims[i];
                let mut pts: Vec<Point3> = Vec::new();
                for cl in &clusters {
                    let th = cl.first().unwrap().0;
                    if !in_window(rim, th) {
                        continue;
                    }
                    if let Some(own) = cl.iter().find(|(_, ri, _)| *ri == i) {
                        pts.push(own.2);
                    } else {
                        let (st, ct) = th.sin_cos();
                        let pj = add(rim.c, add(scl(b1, rim.r * ct), scl(b2, rim.r * st)));
                        pts.push(Point3::new(pj[0], pj[1], pj[2]));
                    }
                }
                if pts.is_empty() {
                    out.remove(&rim.edge);
                } else {
                    out.insert(rim.edge, pts);
                }
            }
        }
    }
    out
}

/// Increment 4: rim descriptor for `rim_junctions_against` — a full-circle
/// rim or a partial ARC (the corpus partial-revolve shape). For an arc,
/// the sweep runs CCW about `n` from `arc.0` to `arc.1` (the stage-1
/// arc-chain convention).
struct RimDesc {
    edge: u32,
    c: [f64; 3],
    n: [f64; 3],
    r: f64,
    /// The edge's start vertex — the seam of a closed rim (ring slot 0).
    seam: [f64; 3],
    arc: Option<([f64; 3], [f64; 3])>,
}

/// Increment 4: candidate filter — never within TAU_MODEL of the rim's
/// own B-Rep vertices (arc endpoints / the closed rim's seam: a boundary
/// junction IS the existing vertex; inserting its ULP twin would trip the
/// uniform-coincidence stop or desynchronize the chain), and for an ARC,
/// inside the CCW sweep window. Full-circle rims accept everything else.
fn point_in_rim_sweep(rim: &RimDesc, pj: [f64; 3]) -> bool {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    {
        let dd = sub(pj, rim.seam);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let Some((sp, ep)) = rim.arc else {
        return true;
    };
    for q in [sp, ep] {
        let dd = sub(pj, q);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let (e1v, e2v) = ortho_basis(Vector3::new(rim.n[0], rim.n[1], rim.n[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let ang = |q: [f64; 3]| -> f64 {
        let w = sub(q, rim.c);
        dot(w, e2).atan2(dot(w, e1))
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let phi0 = ang(sp);
    let sweep = (ang(ep) - phi0).rem_euclid(two_pi);
    let off = (ang(pj) - phi0).rem_euclid(two_pi);
    off < sweep
}

/// Increment 4 §4a: a planar face's loops as 2D segments + full circles in
/// the plane's own frame (frame returned alongside so containment projects
/// identically) — `None` when any loop edge is neither a `LineSegment` nor
/// a closed `Circle` (arc-bounded faces keep today's loud walls). Inner
/// loops (holes) are included: even-odd containment handles both segment
/// and circle boundaries by parity, so discs, annuli, polygons, and mixed
/// forms all work.
type PlanarFace2d = (
    [[f64; 3]; 2],
    Vec<([f64; 2], [f64; 2])>,
    Vec<([f64; 2], f64)>,
);

fn planar_face_segments(
    brep: &BRep,
    f: &BRepFace,
    plane_unit_normal: [f64; 3],
) -> Option<PlanarFace2d> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let nh = plane_unit_normal;
    let (e1v, e2v) = ortho_basis(Vector3::new(nh[0], nh[1], nh[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut circles: Vec<([f64; 2], f64)> = Vec::new();
    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = &brep.edges()[ei as usize];
        match e.curve {
            Curve::LineSegment => {
                let a3 = brep.vertices()[e.start as usize].point.as_array();
                let b3 = brep.vertices()[e.end as usize].point.as_array();
                segs.push(([dot(a3, e1), dot(a3, e2)], [dot(b3, e1), dot(b3, e2)]));
            }
            Curve::Circle { center, radius, .. } if e.start == e.end => {
                let c3 = center.as_array();
                circles.push(([dot(c3, e1), dot(c3, e2)], radius));
            }
            _ => return None,
        }
    }
    Some(([e1, e2], segs, circles))
}

/// Increment 4 §4a: boundary-inclusive (±TAU_WORK) even-odd containment of
/// a 3D point (assumed ON the face plane) in the planar face's boundary
/// set. The TAU_WORK boundary band keeps triple junctions at face edges —
/// the plane analog of the lateral arm's z-extent slack. Holes are
/// excluded by parity (segment ray crossings + circle inside-count).
fn point_in_planar_face(face2d: &PlanarFace2d, p3: [f64; 3]) -> bool {
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let ([e1, e2], segs, circles) = face2d;
    let p = [dot3(p3, *e1), dot3(p3, *e2)];
    // Boundary band first (a point within TAU_WORK of any loop boundary is
    // IN — never lose a face-edge triple junction to parity jitter).
    for &(a, b) in segs {
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ap = [p[0] - a[0], p[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 > 0.0 {
            ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dx = ap[0] - t * ab[0];
        let dy = ap[1] - t * ab[1];
        if (dx * dx + dy * dy).sqrt() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if (d - r).abs() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    // Even-odd parity: +x-ray crossings over segments (half-open on each
    // segment's y-range so shared loop vertices count once) + one toggle
    // per enclosing circle.
    let mut inside = false;
    for &(a, b) in segs {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let xi = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if xi > p[0] {
                inside = !inside;
            }
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if d < r {
            inside = !inside;
        }
    }
    inside
}

/// Increment-2 entry point: both operands' rim junction maps against each
/// other (wired in `boolean()` behind the no-Stage-0-interaction scope
/// gate; spec branch table row 3 records the pass-through trap that gate
/// avoids).
fn rim_junction_overrides(
    a: &BRep,
    b: &BRep,
) -> (
    std::collections::BTreeMap<u32, Vec<Point3>>,
    std::collections::BTreeMap<u32, Vec<Point3>>,
) {
    (rim_junctions_against(a, b), rim_junctions_against(b, a))
}

pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    // both operands at the pair-derived rim density BEFORE any Stage-0/1
    // machinery samples their meshes, so analytically-disjoint cylinder
    // pairs cannot mesh-intersect. `None` (no cylinder faces, e.g. the
    // `from_mesh` chained-output operand, or no disjoint pair demanding
    // more than each solid's own N) leaves both operands byte-identical.
    let boosted: Option<(BRep, BRep)> = match phantom_min_rim_segments(a, b) {
        Some(n) => Some((
            a.rebuilt_with_min_rim_segments(n)?,
            b.rebuilt_with_min_rim_segments(n)?,
        )),
        None => None,
    };
    let (a, b): (&BRep, &BRep) = match &boosted {
        Some((ba, bb)) => (ba, bb),
        None => (a, b),
    };

    // (0) Stage 0 — §4.5.5 coplanar preprocessing (PR-YR26, M8 slice b).
    // Near-coplanar planar A×B face pairs are HANDLED: both faces snapped
    // onto one canonical shared plane, segmented by the exact 2D overlay,
    // and re-tessellated so the overlap region carries IDENTICAL meshes on
    // both solids (see `stage0::stage0_preprocess`). Unsupported residue
    // (intra-solid near pairs — the chained-output class — plus curved /
    // multi-pair faces and overlay failures) keeps the loud typed PR-YR24
    // wall (`CoplanarFacesUnsupported`).
    let stage0 = stage0::stage0_preprocess(a, b)?;
    // M8-cyl Increment 1 (§4.5.5 curved analog): when the planar scan found NO
    // cross pairs, a COINCIDENT-CYLINDER pair (the gear's bore wall ∩ a coaxial
    // flange/plug wall, opposite normal, full θ, one z-extent contained in the
    // other) gets a conformal re-tessellation so its overlap band is
    // bit-identical on BOTH solids' meshes. cherchi then pocket-dedups the band
    // into one multi-label sheet and the membrane resolution below drops it.
    // `task28_plug_in_bore` proved both native cherchi AND the C++ sidecar leave
    // this non-watertight WITHOUT this upstream conformal step. Only consulted
    // when the planar Stage-0 produced nothing (the two paths never overlap on a
    // single pair in Increment 1's scope).
    let stage0 = match stage0 {
        Some(s0) => Some(s0),
        None => stage0::coincident_cylinder_stage0(a, b)?,
    };
    // PR-5: coincident-CYLINDER A×B pairs (the membrane analog of the planar
    // `PairPlane`s in `stage0`). cherchi (coplanar PRs 1-4) constructs the
    // coincident-cylinder overlap with a MULTI-SOLID label exactly as it does a
    // coplanar planar overlap, but the Stage-0 planar scan records only
    // `Surface::Plane` pairs — so a coaxial-cylinder sheet (a flange outer wall
    // coincident with a gear bore, `err.waffle`) had no matching pair and was
    // dropped with `FaceResolutionFailed`. This parallel detector supplies the
    // keep/drop decision for those sheets. It does NOT touch the planar overlay
    // / mesh re-tessellation path (the coincident-cylinder meshes are already
    // bit-identical: both faces are the identical analytic cylinder).
    let cyl_pairs = stage0::detect_coincident_cylinder_pairs(a, b);

    // Increment 2 (spec `yang_rim_junction_insertion`): insert the exact
    // §4.3.3 Case-IV rim junction points as Stage-1 rim samples, so the
    // mesh-level seam chains can terminate exactly at the junctions (the
    // truncated-Steinmetz cap-lobe corners). SCOPE GATE (spec branch row
    // 3): only for a pair with NO Stage-0 interaction — the Stage-0
    // re-tessellation paths do not thread rim overrides yet (the M8
    // incr-15 pass-through trap), and skipping keeps them byte-identical.
    // Rim re-tessellation changes neither surfaces nor topology, so the
    // Stage-0 detectors' verdicts (computed above) remain valid for the
    // rebuilt operands.
    if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
        eprintln!(
            "[rim-junction] gate: stage0_none={} cyl_pairs_empty={}",
            stage0.is_none(),
            cyl_pairs.is_empty()
        );
    }
    let junction_boosted: Option<(BRep, BRep)> = if stage0.is_none()
        && cyl_pairs.is_empty()
        // Diagnostic kill-switch (read-only, env-gated): bisect whether a
        // downstream behavior change is enabled by the insertion.
        && std::env::var_os("YANG_RIM_JUNCTION_DISABLE").is_none()
    {
        let (map_a, map_b) = rim_junction_overrides(a, b);
        if map_a.is_empty() && map_b.is_empty() {
            None
        } else {
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!("[rim-junction] overrides a={map_a:?} b={map_b:?}");
            }
            Some((
                a.rebuilt_with_rim_overrides(&map_a)?,
                b.rebuilt_with_rim_overrides(&map_b)?,
            ))
        }
    } else {
        None
    };
    let (a, b): (&BRep, &BRep) = match &junction_boosted {
        Some((ba, bb)) => (ba, bb),
        None => (a, b),
    };

    // Twin-origin probe (read-only, env-gated): `YANG_INPUT_VERT_PROBE=x,y,z,r`
    // dumps every INPUT B-Rep vertex and every Stage-0/1 mesh vertex within
    // radius r of the target point, per operand — to establish whether a
    // downstream femto-twin pair arrives as two distinct input points
    // (chained-output drift) or is minted inside this boolean.
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
            for (tag, brep) in [("A", a), ("B", b)] {
                for (i, v) in brep.vertices().iter().enumerate() {
                    if near(&v.point) {
                        let q = v.point.as_array();
                        eprintln!(
                            "[input-vert-probe] input {tag} brep vert {i}: ({},{},{})",
                            q[0], q[1], q[2]
                        );
                    }
                }
            }
            if let Some(s0) = &stage0 {
                for (tag, m) in [("A", &s0.mesh_a), ("B", &s0.mesh_b)] {
                    for (i, v) in m.verts.iter().enumerate() {
                        if near(v) {
                            let q = v.as_array();
                            eprintln!(
                                "[input-vert-probe] stage0 mesh {tag} vert {i}: ({},{},{})",
                                q[0], q[1], q[2]
                            );
                        }
                    }
                }
            }
        }
    }
    let (mesh_a, mesh_b): (&Mesh, &Mesh) = match &stage0 {
        Some(s0) => (&s0.mesh_a, &s0.mesh_b),
        // No coplanar pairs: the B-Reps' own Stage-1 meshes — byte-for-byte
        // the pre-YR26 path.
        None => (a.as_mesh(), b.as_mesh()),
    };
    // M8 diagnostic operand dump (env-gated, read-only; spec
    // `m8_stage0_inputcheck_clean_emission` §6).
    stage0_dump(
        op,
        stage0.as_ref(),
        cyl_pairs.len(),
        mesh_a,
        mesh_b,
        a.as_mesh(),
        b.as_mesh(),
    );

    // (1) Stage 2: full labeled arrangement.
    let la = backend
        .labeled_arrangement(mesh_a, mesh_b)
        .map_err(YangError::MeshBooleanFailed)?;

    // (2) I6 weld: the C++ producer does NOT always weld coincident vertices
    // (it can emit two distinct indices at bit-identical coordinates — a
    // non-manifold touching point — used by shared triangles). yang's
    // index-based adjacency requires coincident points to share one index, so
    // weld each vertex to the ORIGINAL index of its first coincident
    // occurrence. (Mapping to the original index — not a renumbered counter —
    // keeps `la.mesh.verts[welded]` valid: coordinates are unchanged.)
    //
    // PR-KV10 (M8 residue): for ALL-PLANAR input pairs the weld is
    // NEAR-aware, not just bit-exact. The old "the producer never emits
    // TAU_WORK-near-but-bit-distinct coincident verts" assumption is FALSE
    // for chained planar inputs: an oblique solid's f64 vertices make
    // adjacent same-face tessellation triangles span femto-different EXACT
    // planes, so the exact arrangement legitimately mints distinct
    // intersection points ~1e-16·scale apart where several intersection
    // segments junction (one geometric point, several generating tri
    // pairs). Left distinct, the copies chain into sliver fans in the
    // output B-Rep and poison the NEXT boolean's attribution (the
    // F0016-class corpus residue's second layer — found behind the
    // intra-coplanar wall). Welding them within the scale-relative rounding
    // band `TAU_WORK·(1+|coord|)` is the same reconciliation principle as
    // the §4.5.5 Stage-0 snap; genuinely distinct model features are
    // ≥ MIN_FEATURE_SIZE apart — six orders beyond the band. Clusters weld
    // to their LOWEST member index (deterministic; survivor keeps its own
    // coordinates). Bucketed by a quantized grid with 27-neighborhood
    // probing + an EXACT per-pair band check — quantization alone aliases
    // (the KV8c lesson), so it only ever NOMINATES candidates, never
    // decides.
    //
    // CURVED inputs keep the bit-exact weld: the cyl×cyl pipeline expects
    // near-coincident-but-structurally-distinct vertices at ruling-line /
    // tangency junctions (one copy per incident surface's chord ring) and
    // reconciles them ITSELF in Stage-4 relocation with curve knowledge
    // (the KV9 junction duplicate collapse); welding them at step (2)
    // collapses lens-tip seam edges into degenerate (<3-edge) output loops
    // — found by kv9_cyl_cyl_special RED on the first attempt.
    // Per-triangle B-Rep face maps for the operand meshes — the inputs' OWN
    // Stage-1 `tri_face` when Stage 0 did not re-tessellate, else the Stage-0
    // re-tessellated meshes' maps. Consumed by the KV15 weld eligibility
    // below and by the Stage-6 N4 provenance attribution.
    let (tri_face_a, tri_face_b): (&[u32], &[u32]) = match &stage0 {
        Some(s0) => (&s0.tri_face_a, &s0.tri_face_b),
        None => (a.tri_face(), b.tri_face()),
    };
    let all_planar = a
        .faces()
        .iter()
        .chain(b.faces().iter())
        .all(|f| matches!(f.surface, Surface::Plane { .. }));
    let weld: Vec<u32> = if all_planar {
        use std::collections::HashMap;
        let verts = &la.mesh.verts;
        // Union-find over vertex indices (path-halving; union by min index
        // happens at the final resolution pass).
        let mut parent: Vec<u32> = (0..verts.len() as u32).collect();
        fn find(parent: &mut [u32], mut x: u32) -> u32 {
            while parent[x as usize] != x {
                parent[x as usize] = parent[parent[x as usize] as usize];
                x = parent[x as usize];
            }
            x
        }
        // Grid cell size: one band at the mesh's coordinate scale.
        let scale = verts
            .iter()
            .flat_map(|v| v.as_array())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        let band = cad_primitives::TAU_WORK * (1.0 + scale);
        let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
        let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(verts.len());
        for (i, v) in verts.iter().enumerate() {
            let p = v.as_array();
            let key = [cell(p[0]), cell(p[1]), cell(p[2])];
            // Probe the 27-neighborhood for near-coincident occupants; the
            // EXACT pairwise band test decides. Union with EVERY in-band
            // occupant (a vertex can bridge two so-far-separate clusters).
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    for dz in -1..=1i64 {
                        let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                            continue;
                        };
                        for &j in occ {
                            let q = verts[j as usize].as_array();
                            let pair_band = cad_primitives::TAU_WORK
                                * (1.0
                                    + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                            if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                let (ri, rj) = (find(&mut parent, i as u32), find(&mut parent, j));
                                if ri != rj {
                                    // Root at the smaller index so the final
                                    // representative is the cluster minimum.
                                    parent[ri.max(rj) as usize] = ri.min(rj);
                                }
                            }
                        }
                    }
                }
            }
            grid.entry(key).or_default().push(i as u32);
        }
        (0..verts.len() as u32)
            .map(|i| find(&mut parent, i))
            .collect()
    } else {
        // Bit-exact weld (the pre-KV10 path, byte-identical for curved
        // pipelines): weld each vertex to the ORIGINAL index of its first
        // bit-identical occurrence.
        use std::collections::HashMap;
        let mut first: HashMap<[u64; 3], u32> = HashMap::with_capacity(la.mesh.verts.len());
        let mut weld: Vec<u32> = la
            .mesh
            .verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
                *first.entry(key).or_insert(i as u32)
            })
            .collect();

        // KV15 (spec `kv15_mixed_operand_planar_near_weld` §3): per-vertex
        // planar near-weld for MIXED operands. The chained-extrude corpus
        // mints planar femto twins whose reconciliation is exactly the KV10
        // near-weld above — but one curved face ANYWHERE in either operand
        // used to drop the whole model to bit-exact, leaving the twins'
        // femto membrane to poison Stage-6 patch boundaries (the
        // edge-not-2-directed InvalidBooleanOutput class). Eligibility is
        // PER VERTEX: a vertex near-welds only when EVERY incident
        // arrangement triangle descends, via `la.source` + the operand
        // `tri_face` map, from a `Surface::Plane` face. Curved-adjacent
        // vertices keep bit-exact (kv9: cyl×cyl junction duplicates are
        // structurally distinct — one copy per incident surface's chord
        // ring — and Stage-4 owns their collapse). Empty / out-of-range /
        // sentinel provenance marks its vertices ineligible (conservative:
        // the sidecar parity producer keeps today's behavior, spec W4).
        {
            let face_planar = |k: u32, fi: u32| -> Option<bool> {
                let brep: &BRep = if k == 0 { a } else { b };
                brep.faces()
                    .get(fi as usize)
                    .map(|f| matches!(f.surface, Surface::Plane { .. }))
            };
            let curved = kv15_curved_touch(
                la.mesh.verts.len(),
                &la.mesh.tris,
                &la.source,
                tri_face_a,
                tri_face_b,
                face_planar,
            );
            // Propagate ineligibility through bit-exact clusters: a root is
            // curved if ANY member is (a bit-duplicate of a protected
            // junction vertex must not drag it into a near-weld).
            let mut root_curved = vec![false; la.mesh.verts.len()];
            for (i, &c) in curved.iter().enumerate() {
                if c {
                    root_curved[weld[i] as usize] = true;
                }
            }
            kv15_near_weld_pass(&la.mesh.verts, &mut weld, &root_curved);
        }

        // PR-6 (coincident-cylinder rim conformal weld). The §4.5.5 planar
        // Stage-0 overlay makes two coincident PLANAR faces' shared loop
        // vertices bit-identical (the cross-weld at `stage0.rs:261`). Its
        // curved analog: where a coincident-CYLINDER pair's lateral meets a
        // CAP PLANE, cherchi's exact arrangement mints the SAME rim-circle
        // point redundantly (once per generating tri-pair / incident surface),
        // landing a cluster of copies a FEW ULPs apart (verified on
        // `err.waffle`: 31 such near-twins, all at machine-zero distance from
        // a `cyl_pairs` lateral AND on the cap plane, max separation ~9e-19 at
        // a coordinate scale of 5e-3 — i.e. ~1 ULP). The bit-exact weld leaves
        // them distinct, so a kept triangle can carry two copies of one
        // geometric rim point: a zero-area sliver that fails Stage-4
        // (`DegenerateTriangle` at v4497/v4495) and pinches the post-membrane
        // seam.
        //
        // The conformal reconciliation: union ONLY vertices that lie EXACTLY
        // (within the pair's analytic band) on a coincident-cylinder pair's
        // shared lateral AND are within the scale-relative `TAU_WORK·(1+scale)`
        // band of each other. This is an EXACT-IDENTITY weld of redundant
        // reconstructions of one analytic point — NOT a tolerance bucket:
        //   • Membership is gated on the analytic coincident-cylinder surface
        //     (machine-zero radial distance), not a proximity guess.
        //   • The union band (~1e-12) is six orders below MIN_FEATURE_SIZE
        //     (1e-6); genuinely distinct rim points (≥ chord-spacing ~1e-4)
        //     never fuse — only sub-ULP duplicates do.
        //   • It touches NO planar case (gated on `cyl_pairs`), so it cannot
        //     reintroduce the reverted F0057 planar-weld masking (that weld
        //     fused planar vertices and hid 74 unpaired edges).
        // Survivor = the cluster's minimum welded index (deterministic).
        if !cyl_pairs.is_empty() {
            let verts = &la.mesh.verts;
            // On-cylinder predicate: radial distance within the pair band. The
            // observed rim duplicates sit at ~1e-19 (machine zero); the band
            // (1e-7) is a safe analytic membership gate that admits no
            // off-surface vertex of this model (off-rim arrangement points are
            // ≥ chord-scale ~1e-4 off any OTHER cylinder, and on-lateral
            // tessellation chords sit up to the sagitta INSIDE the radius —
            // far beyond 1e-7 — so only true on-surface rim points qualify).
            let on_rim = |i: u32| -> bool {
                let c = verts[i as usize].as_array();
                cyl_pairs
                    .iter()
                    .any(|p| centroid_on_cylinder(c, p) <= p.band)
            };
            let scale = verts
                .iter()
                .flat_map(|v| v.as_array())
                .fold(0.0f64, |m, c| m.max(c.abs()));
            let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
            // Candidate rim vertices (post bit-exact weld representatives only).
            let rim: Vec<u32> = (0..verts.len() as u32)
                .filter(|&i| weld[i as usize] == i && on_rim(i))
                .collect();
            // Bucketed union-find (27-neighborhood probe + exact pairwise band).
            let mut parent: HashMap<u32, u32> = rim.iter().map(|&i| (i, i)).collect();
            fn find(parent: &mut HashMap<u32, u32>, mut x: u32) -> u32 {
                while parent[&x] != x {
                    let g = parent[&parent[&x]];
                    parent.insert(x, g);
                    x = g;
                }
                x
            }
            let cell = |c: f64| -> i64 { (c / cluster_band).floor() as i64 };
            let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
            for &i in &rim {
                let p = verts[i as usize].as_array();
                let key = [cell(p[0]), cell(p[1]), cell(p[2])];
                for dx in -1..=1i64 {
                    for dy in -1..=1i64 {
                        for dz in -1..=1i64 {
                            let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz])
                            else {
                                continue;
                            };
                            for &j in occ {
                                let q = verts[j as usize].as_array();
                                let pair_band = cad_primitives::TAU_WORK
                                    * (1.0
                                        + p.iter()
                                            .chain(q.iter())
                                            .fold(0.0f64, |m, c| m.max(c.abs())));
                                if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                                    if ri != rj {
                                        parent.insert(ri.max(rj), ri.min(rj));
                                    }
                                }
                            }
                        }
                    }
                }
                grid.entry(key).or_default().push(i);
            }
            // Re-point every vertex whose bit-exact representative is a rim
            // candidate to its cluster minimum.
            for w in weld.iter_mut() {
                if parent.contains_key(w) {
                    *w = find(&mut parent, *w);
                }
            }
        }

        weld
    };

    // (3) Stage 4: which arrangement tris survive `op`.
    let kept = la.keep_set(op);

    // KV9-F1 diagnosis probe (read-only, env-gated): per-input label + keep
    // census over the labeled arrangement.
    if std::env::var_os("YANG_KEEP_PROBE").is_some() {
        let kept_set: std::collections::BTreeSet<usize> = kept.iter().copied().collect();
        let mut rows: std::collections::BTreeMap<(String, Vec<bool>, bool), usize> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let surf = format!("{:?}", la.surface[t]);
            *rows
                .entry((surf, la.inside[t].clone(), kept_set.contains(&t)))
                .or_insert(0) += 1;
        }
        eprintln!(
            "[keep-probe] la tris {} kept {} (op {op:?})",
            la.mesh.tris.len(),
            kept.len()
        );
        for ((surf, inside, k), n) in rows {
            eprintln!("[keep-probe]   surface {surf} inside {inside:?} kept={k}: {n}");
        }
        let mut patches: std::collections::BTreeMap<u32, (String, usize)> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let e = patches
                .entry(la.patch[t])
                .or_insert_with(|| (format!("{:?}", la.surface[t]), 0));
            e.1 += 1;
        }
        for (pid, (surf, n)) in patches {
            eprintln!("[keep-probe]   patch {pid}: surface {surf} tris {n}");
        }
    }

    // (3a) XOR deferred (spec §Scope): its symmetric-difference result is
    // multi-shell / has a void that `reconstruct_topology` cannot reassemble
    // yet. Error LOUDLY (`UnsupportedOp`) rather than emitting a generic
    // `NonManifoldOutput` or a silently-wrong result (P9). Gated on a
    // non-empty XOR kept-set: a degenerate XOR with nothing to reassemble
    // (empty arrangement) still trivially succeeds with an empty result, so
    // op-dispatch over an empty arrangement is well-defined for all four ops.
    if op == BoolOp::Xor && !kept.is_empty() {
        return Err(YangError::UnsupportedOp(op));
    }

    // (4) Compact kept sub-mesh: weld + per-op winding fix, then remap the
    // referenced (welded) verts to dense indices.
    let mut remap: Vec<Option<u32>> = vec![None; la.mesh.verts.len()];
    let mut compact_verts: Vec<Point3> = Vec::new();
    let mut compact_tris: Vec<[u32; 3]> = Vec::with_capacity(kept.len());
    // compact-tri index -> original `la` tri index (for surface lookup).
    let mut orig_tri: Vec<usize> = Vec::with_capacity(kept.len());
    for &orig_t in &kept {
        let raw = la.mesh.tris[orig_t];

        // (3b) §4.5.5 overlap-sheet ("membrane") resolution. A triangle with
        // a multi-solid surface label lies on the trimmed common planar
        // surface of a Stage-0 pair. Cherchi's keep-rules alone keep it for
        // EVERY op (surface = {A,B}, inside = ∅ satisfies the union /
        // intersection / subtraction-branch-1 rules, booleans.cpp:1397/
        // 1422/1467 — the C++ emits the zero-volume sheet); solid semantics
        // instead keep it iff exactly ONE side of its plane is inside the
        // result. With the pair's normal-agreement flag (`opposite`: solids
        // on opposite sides, stacked; else both interiors on the same
        // side, flush/pocket) that side rule reduces to:
        //
        //   Union:     keep iff !opposite (boundary of both ⇒ of the union)
        //   Intersect: keep iff !opposite (boundary of A∩B; opposite ⇒ the
        //              intersection is the zero-volume sheet itself: drop)
        //   Subtract:  keep iff opposite (B is beyond the plane: the sheet
        //              stays A's boundary; equal ⇒ B consumes it: the
        //              pocket OPENING is removed)
        //
        // The kept copy is the dedup survivor — input A's, with A's winding
        // — which is the correct result orientation in every kept case
        // (subtract-opposite / union-equal / intersect-equal all bound the
        // result with A's outward direction).
        if la.surface[orig_t].len() > 1 {
            let p0 = la.mesh.verts[raw[0] as usize].as_array();
            let p1 = la.mesh.verts[raw[1] as usize].as_array();
            let p2 = la.mesh.verts[raw[2] as usize].as_array();
            let c = [
                (p0[0] + p1[0] + p2[0]) / 3.0,
                (p0[1] + p1[1] + p2[1]) / 3.0,
                (p0[2] + p1[2] + p2[2]) / 3.0,
            ];
            // The sheet's `opposite` flag — found by matching its centroid to a
            // Stage-0 PLANAR pair plane (the §4.5.5 membrane) OR, failing that,
            // to a coincident-CYLINDER pair (PR-5: a sheet triangle lies on a
            // cylinder pair iff `|dist(c, axis_line) − radius| <= band`). Only
            // if NEITHER matches is it an unhandled config — still loud (P9).
            let planar = stage0.as_ref().and_then(|s0| {
                s0.pairs
                    .iter()
                    .find(|p| (p.n[0] * c[0] + p.n[1] * c[1] + p.n[2] * c[2] + p.d).abs() <= p.band)
                    .map(|p| p.opposite)
            });
            let opposite = match planar {
                Some(o) => o,
                // A sheet triangle on the TESSELLATED cylinder sits up to the
                // Stage-1 chord sagitta inside the analytic radius — far beyond
                // the detection `band`. Match against the curved chord bound
                // `d_ε` (the SAME bound Stage 1 sizes the tessellation to and
                // Stage-6 attribution uses for cylinder faces — A14.3, not a
                // widening). Both solids' overlap meshes are bit-identical, so
                // either chord bound applies; use the larger to be safe.
                None => match cyl_pairs.iter().find(|p| {
                    let de = curved_chord_bound(a.edges())
                        .unwrap_or(0.0)
                        .max(curved_chord_bound(b.edges()).unwrap_or(0.0))
                        .max(p.band);
                    centroid_on_cylinder(c, p) <= de
                }) {
                    Some(p) => p.opposite,
                    // On no known pair (planar or cylinder) — loud, never a
                    // guessed config.
                    None => return Err(YangError::FaceResolutionFailed { tri: orig_t }),
                },
            };
            let keep_sheet = match op {
                BoolOp::Union | BoolOp::Intersect => !opposite,
                BoolOp::Subtract => opposite,
                // XOR never reaches here (rejected at (3a) on a non-empty
                // kept set), but the side rule drops the sheet in both
                // configs anyway.
                BoolOp::Xor => false,
            };
            if !keep_sheet {
                continue;
            }
        }

        // Apply the weld (coincident points → shared original index).
        let mut tri = [
            weld[raw[0] as usize],
            weld[raw[1] as usize],
            weld[raw[2] as usize],
        ];
        // A welded triangle with a repeated index is a zero-area sliver at a
        // coincident (welded) point — it carries no surface and no volume, and
        // its two non-degenerate directed edges are mutual opposites that
        // cancel, so dropping it preserves the watertight half-edge pairing.
        // (Real, in-scope arrangement artifact — NOT non-manifold input.)
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
            continue;
        }
        // Per-op winding fix (Cherchi booleans.cpp boolSubtraction:1480-1483):
        // the keep-rule selects triangles but some kept triangles bound the
        // result with reversed orientation and must be flipped so the output
        // is consistently outward-oriented (I9 signed volume). Union /
        // Intersection keep winding as-is.
        if flip_for_op(op, &la, orig_t) {
            tri.swap(1, 2);
        }
        let mut new_tri = [0u32; 3];
        for (k, &wi) in tri.iter().enumerate() {
            let slot = &mut remap[wi as usize];
            let new_vi = match slot {
                Some(idx) => *idx,
                None => {
                    let idx = compact_verts.len() as u32;
                    compact_verts.push(la.mesh.verts[wi as usize]);
                    *slot = Some(idx);
                    idx
                }
            };
            new_tri[k] = new_vi;
        }
        compact_tris.push(new_tri);
        orig_tri.push(orig_t);
    }
    // (I6 guard) Two distinct surviving triangles that welded to the same 3
    // vertices are genuinely coincident faces (non-manifold input) — e.g. the
    // a4 fixture's two tris over bit-exact-coincident vertices. A valid
    // arrangement has no such pair; reject it. (Compact indices are 1:1 with
    // welded indices, so a sorted-index key suffices.)
    {
        use std::collections::HashSet;
        let mut seen: HashSet<[u32; 3]> = HashSet::with_capacity(compact_tris.len());
        for t in &compact_tris {
            let mut sorted = *t;
            sorted.sort_unstable();
            if !seen.insert(sorted) {
                return Err(YangError::NonManifoldInput);
            }
        }
    }
    let kept_submesh = Mesh::new(compact_verts, compact_tris);

    // (5) Stage 6: face resolution → FULL attribution. PRIMARY path is N4
    // provenance (cherchi `source` → B-Rep face via the per-triangle face map,
    // `tri_face_a`/`tri_face_b` bound above the weld); the geometric
    // resolution below is the fallback. Either map may be empty (a Stage-0
    // path that does not emit provenance yet, or a lineage-less input) → that
    // triangle falls back to geometric.
    let mut attributions: Vec<Option<TriangleAttribution>> = Vec::with_capacity(orig_tri.len());
    for (compact_t, &orig_t) in orig_tri.iter().enumerate() {
        let surf = &la.surface[orig_t];
        let (input_brep, input) = if surf.len() > 1 {
            // §4.5.5 trimmed common surface (PR-YR26): a SURVIVING
            // multi-label triangle is a kept overlap-sheet triangle (the
            // (3b) side rule already decided it bounds the result). It
            // descends from coincident faces of BOTH inputs; the kept copy
            // is the dedup survivor — input A's, with A's winding — so it
            // attributes to input A (its plane equals B's, so the
            // inherited output surface is identical either way; A is the
            // deterministic choice consistent with the kept orientation).
            (a, InputId::A)
        } else {
            let LaInputId(k) = surf[0];
            // cherchi InputId(u32): 0 → A, 1 → B.
            match k {
                0 => (a, InputId::A),
                _ => (b, InputId::B),
            }
        };

        // N4 (provenance, §4.2.3): attribute this kept triangle to its B-Rep face
        // DIRECTLY from its parent input triangle (cherchi `source` → `tri_face`)
        // — exact, no geometry, no tolerance. Works for non-coplanar AND coplanar
        // overlaps (the latter via the Stage-0 re-tessellated meshes' face maps).
        //
        // N4 RETIREMENT (task #53, spec `specs/n4_retire_stage6_fallback.md`):
        // on a lineage-CARRYING input, a provenance MISS is a producer fault
        // and fails LOUDLY — the `YANG_N4_FALLBACK_PROBE` measurement proved
        // zero misses across the full corpus, and a silent geometric guess can
        // misattribute (the failure class N4 eliminated) while masking
        // provenance regressions. The geometric resolution below remains ONLY
        // for LINEAGE-LESS attribution: an arrangement without `source` (the
        // dev-only C++ sidecar oracle and the in-crate mock-label fixtures;
        // reference parity depends on it) or an input without a provenance
        // map (`ProvMiss::NoLineage` — a yang boolean OUTPUT chained directly
        // back in, or a `from_mesh` B-Rep).
        if !la.source.is_empty() {
            match provenance_face_reason(&la.source[orig_t], input, tri_face_a, tri_face_b) {
                Ok(face) => {
                    attributions.push(Some(TriangleAttribution { input, face }));
                    continue;
                }
                // Lineage-less input: the documented geometric path below.
                Err(ProvMiss::NoLineage) => {}
                Err(reason) => {
                    // Env-gated diagnostic naming the miss reason; the error
                    // itself is unconditional.
                    if std::env::var_os("YANG_N4_FALLBACK_PROBE").is_some() {
                        eprintln!(
                            "[n4-fallback] input={input:?} orig_t={orig_t} reason={reason:?} \
                             stage0={} tf_a_len={} tf_b_len={}",
                            stage0.is_some(),
                            tri_face_a.len(),
                            tri_face_b.len(),
                        );
                    }
                    return Err(YangError::FaceResolutionFailed { tri: compact_t });
                }
            }
        }

        // Centroid of the (compact) triangle — same coords as `la.mesh`.
        let tri = kept_submesh.tris[compact_t];
        let p0 = kept_submesh.verts[tri[0] as usize].as_array();
        let p1 = kept_submesh.verts[tri[1] as usize].as_array();
        let p2 = kept_submesh.verts[tri[2] as usize].as_array();
        let c = [
            (p0[0] + p1[0] + p2[0]) / 3.0,
            (p0[1] + p1[1] + p2[1]) / 3.0,
            (p0[2] + p1[2] + p2[2]) / 3.0,
        ];

        // Is this kept triangle DEGENERATE (zero-area / collinear)? The exact
        // arrangement emits sliver triangles along shared solid edges (3
        // distinct welded verts, all collinear). They carry no surface and no
        // volume but pair their edges into the watertight result, so they are
        // kept (not dropped — dropping breaks edge-pairing). Their centroid
        // lands on a solid edge, equidistant from the two adjacent face planes,
        // so the unique-face rule would (wrongly) F3-tie them. Threshold is the
        // M1 area threshold (2·area = ‖cross(e1,e2)‖; compare to MIN_FEATURE_SIZE²;
        // governance A14.3 — shared constant, no ad-hoc epsilon).
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let degenerate =
            twice_area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;

        // Distance of the centroid to each labeled-solid face plane. Curved
        // faces are already rejected at `BRep::new`, so this is defensive — but
        // it must compile and be LOUD (P9): a curved arm returns the carrying
        // `Err`, never `unreachable!`/panic. `fi` is the input B-Rep face index.
        // PR-YR27 (Finding 2): a face that went through a Stage-0 pair had
        // its loop vertices SNAPPED onto the pair's CANONICAL plane, so its
        // kept triangles lie on the canonical plane — up to the pair's
        // detection `band` (≫ TAU_WORK) away from the face's STORED plane.
        // Membership for exactly those faces is therefore measured against
        // the canonical pair plane (KEYED to the pair: every non-pair face
        // keeps its stored surface + TAU_WORK byte-for-byte — this is the
        // Stage-1 geometry the snap actually produced, NOT a tolerance
        // widening).
        let stage0_pair_plane = |fi: usize| -> Option<&stage0::PairPlane> {
            stage0.as_ref().and_then(|s0| {
                s0.pairs.iter().find(|p| match input {
                    InputId::A => p.face_a == fi,
                    InputId::B => p.face_b == fi,
                })
            })
        };
        let plane_dist = |fi: usize, face: &BRepFace| -> Result<f64, YangError> {
            if let Some(pp) = stage0_pair_plane(fi) {
                return Ok((pp.n[0] * c[0] + pp.n[1] * c[1] + pp.n[2] * c[2] + pp.d).abs());
            }
            // PR-YR7: delegate to the shared `signed_distance_to_surface`
            // (Plane + Cylinder + Sphere); take `.abs()` (distance to the
            // surface). Cone still rejects loudly — the free function returns a
            // sentinel face index, which we replace with the real input `fi`.
            match signed_distance_to_surface(face.surface, Point3::new(c[0], c[1], c[2])) {
                Ok(d) => Ok(d.abs()),
                Err(YangError::CurvedSurfaceNotYetSupported { .. }) => {
                    Err(YangError::CurvedSurfaceNotYetSupported { face: fi })
                }
                Err(other) => Err(other),
            }
        };

        // PER-FACE membership tolerance (PR-YR8 Blocker 1, spec §4). The
        // membership tolerance is the surface's OWN Stage-1 tessellation chord
        // bound (governance A15 / A14.3 — not tolerance widening): a `Plane`
        // face has zero chord error → `TAU_WORK`; a `Cylinder` face is a
        // `d_ε`-chord approximation BY CONSTRUCTION → its labeled solid's curved
        // chord band `d_ε`, the SAME bound Stage 1 guarantees. Computed once per
        // labeled solid from the SINGLE shared source.
        //
        // A `Cylinder` face implies the solid HAS circle rims, so `band` is
        // `Some`; if it is somehow `None` for a cylinder face that is a genuine
        // producer fault → `FaceResolutionFailed` (do NOT silently default a
        // cylinder face to `TAU_WORK`).
        //
        // For ALL-PLANAR inputs every face uses `TAU_WORK` (planar faces always
        // do; an all-planar solid has `band == None` so no face consults it),
        // making BOTH branches below byte-for-byte the OLD rules — the 900-case
        // box fuzz and the m3/yr5c planar-sliver tests are unaffected.
        let band = curved_chord_bound(input_brep.edges());
        let tol_for = |fi: usize, surface: Surface| -> Result<f64, YangError> {
            match surface {
                // PR-YR27 Finding 2 (completion): a planar face welded onto a
                // Stage-0 canonical pair plane legitimately lies up to the
                // pair's detection `band` from it — the SAME band `plane_dist`
                // above already measures the centroid against. The membership
                // THRESHOLD must match that distance basis, so a pair-plane face
                // uses its pair band; every NON-pair planar face keeps TAU_WORK
                // byte-for-byte (the exact/band tier split below still keys on
                // TAU_WORK, so on-plane triangles stay EXACT hits and the
                // all-planar fuzz corpus is unaffected — this only admits the
                // band-level offset the Stage-0 weld itself introduced, NOT a
                // widening). Without it a coplanar boolean at non-unit model
                // scale (e.g. a 10 mm bearing recess, coords ~1e-2, weld
                // residual ~1e-10 ≫ TAU_WORK) loses its annulus-cap triangles to
                // a spurious FaceResolutionFailed.
                Surface::Plane { .. } => Ok(match stage0_pair_plane(fi) {
                    Some(pp) => pp.band.max(cad_primitives::TAU_WORK),
                    None => cad_primitives::TAU_WORK,
                }),
                Surface::Cylinder { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
                // PR-YR15: a Sphere face uses its OWN Stage-1 chord bound
                // `sphere_chord_bound(radius) = 1e-2·2r√3` — the SAME bound
                // Stage 1 guarantees (A15/A14.3, NOT tolerance widening). It is
                // deliberately NOT the Circle-rim `band` (2r√2), which would
                // underestimate the sphere's chord error.
                Surface::Sphere { radius, .. } => Ok(sphere_chord_bound(radius)),
                // PR-YR17: a Cone face uses its OWN Stage-1 chord bound
                // `cone_chord_bound(height, half_angle)` — the SAME bound Stage 1
                // guarantees (A15/A14.3, NOT tolerance widening). The cone height
                // is not in `Surface::Cone` (only apex/axis_dir/half_angle), so it
                // is derived from the cone face's rim `Curve::Circle` edge in its
                // outer loop exactly as the Stage-1 pre-pass does (src/lib.rs
                // ~503-525): `height = |(rim_center − apex)·â|`. This is the live
                // reject site for a Cone (PR-YR16 made
                // `signed_distance_to_surface(Cone)` return `Ok`, so `plane_dist`
                // no longer rejects the cone upstream). If the cone face's outer
                // loop has NO rim Circle, no sound height can be derived → loud
                // `FaceResolutionFailed` (a genuine producer fault; P9 — NEVER a
                // defaulted or widened tolerance).
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    let au = normalize3(axis_dir.as_array());
                    let ap = apex.as_array();
                    let mut height: Option<f64> = None;
                    for &e_idx in &input_brep.faces()[fi].outer_loop {
                        if let Curve::Circle { center, .. } =
                            input_brep.edges()[e_idx as usize].curve
                        {
                            let c = center.as_array();
                            height = Some(
                                ((c[0] - ap[0]) * au[0]
                                    + (c[1] - ap[1]) * au[1]
                                    + (c[2] - ap[2]) * au[2])
                                    .abs(),
                            );
                            break;
                        }
                    }
                    match height {
                        Some(h) => Ok(cone_chord_bound(h, half_angle)),
                        None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                    }
                }
                // KV6d: a torus face uses the rim chord `band` (the rim AABB
                // bound covers the outermost latitude radius major+minor).
                Surface::Torus { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };

        let face = if degenerate {
            // Degenerate sliver: attribute to the LOWEST face index within ITS
            // per-face tolerance (a zero-area triangle has no area, so which
            // adjacent face it joins is geometrically harmless). Never an F3
            // tie — the tie contract is for *real* (positive-area) triangles.
            //
            // PR-YR8: this branch uses the PER-FACE tolerance, not absolute
            // TAU_WORK. The spec §4 "degenerate branch keeps TAU_WORK" line was
            // written for the planar-only world (slivers only on shared
            // planar-planar solid edges, centroid on both planes within
            // TAU_WORK). It did not foresee a sliver lying ON a tessellated
            // CYLINDER face: the sidecar arrangement emits a near-zero-area
            // sliver on the cylinder lateral surface whose centroid is ~d_ε
            // inside the analytic cylinder (within the Stage-1 bound, but ≫
            // TAU_WORK). The governing PRINCIPLE (§4 Blocker 1: test membership
            // at the surface's own Stage-1 chord bound) applies to ANY triangle
            // on the cylinder face, degenerate or not. For all-planar inputs
            // this stays byte-identical (every tol = TAU_WORK). If no face is
            // within tolerance, that is a genuine producer fault → loud (P9).
            let mut hit: Option<u32> = None;
            for (fi, f) in input_brep.faces().iter().enumerate() {
                if plane_dist(fi, f)? < tol_for(fi, f.surface)? {
                    hit = Some(fi as u32);
                    break;
                }
            }
            match hit {
                Some(fi) => fi,
                None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
            }
        } else {
            // PR-YR20 tiered tie-break: an EXACT membership (centroid within
            // TAU_WORK of the surface — it lies ON it) dominates a
            // within-chord-band membership. Each face still uses its own A14.3
            // band via tol_for; we only rank the tie by tier. For all-planar
            // inputs every hit is EXACT (planar tol == TAU_WORK), so a unique
            // hit is byte-for-byte the old "exactly one face within TAU_WORK"
            // rule.
            let mut exact_hits: Vec<u32> = Vec::new();
            let mut band_hits: Vec<u32> = Vec::new();
            for (fi, f) in input_brep.faces().iter().enumerate() {
                let d = plane_dist(fi, f)?;
                if d < tol_for(fi, f.surface)? {
                    if d < cad_primitives::TAU_WORK {
                        exact_hits.push(fi as u32);
                    } else {
                        band_hits.push(fi as u32);
                    }
                }
            }
            // PR-YR27 (Finding 3): a multi-hit tier is narrowed by FINITE-
            // EXTENT strict containment before it is declared a tie. The
            // infinite-plane rule alone false-positives whenever a kept
            // triangle's centroid happens to lie bit-exactly ON another
            // face's plane (the L-profile CDT class: cap triangle
            // (0,0),(2,0),(1,1) → centroid x = 1 = the x=1 side plane;
            // likewise a chained input carrying two same-plane faces). The
            // TRUE owning face strictly contains the centroid of every
            // positive-area kept triangle attributed to it; the false
            // positive at best touches its trimmed region's boundary —
            // strictness is therefore sound and load-bearing. Faces the
            // exact 2D test cannot decide (curved surfaces / curved loop
            // edges → `None`) are NEVER excluded, so an undecidable tie
            // stays the loud error (P9 — containment breaks ties, it never
            // widens membership; a unique hit is accepted without it,
            // byte-identical to the old rule).
            let narrow = |hits: Vec<u32>| -> Result<Option<u32>, YangError> {
                match hits.len() {
                    0 => Ok(None),
                    1 => Ok(Some(hits[0])),
                    _ => {
                        let kept: Vec<u32> = hits
                            .into_iter()
                            .filter(|&fi| {
                                point_strictly_in_planar_face(input_brep, fi as usize, c)
                                    != Some(false)
                                    && point_strictly_in_cylinder_face_axially(
                                        input_brep,
                                        fi as usize,
                                        c,
                                    ) != Some(false)
                            })
                            .collect();
                        match kept.len() {
                            1 => Ok(Some(kept[0])),
                            // 0 (centroid on every tied face's boundary) — loud.
                            0 => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                            // ≥2 survivors. SAME-SURFACE TIE: faces sharing
                            // IDENTICAL surface geometry are INTERCHANGEABLE for
                            // attribution — a triangle on that surface belongs to
                            // it no matter which fragment owns it, and topology
                            // reconstruction regroups them by adjacency into one
                            // output face. This arises when one analytic surface
                            // is SPLIT into several faces — e.g. a cylindrical
                            // bore fragmented into arc-faces by the
                            // tessellated-polygon profile fallback (gear bores).
                            // Pick the lowest index: NOT silent-wrong (same
                            // surface), unlike a tolerance widening. A tie among
                            // GEOMETRICALLY DISTINCT surfaces stays the loud error
                            // (P9 — genuinely ambiguous).
                            _ => {
                                let s0 = input_brep.faces()[kept[0] as usize].surface;
                                if kept
                                    .iter()
                                    .all(|&fi| input_brep.faces()[fi as usize].surface == s0)
                                {
                                    Ok(kept.iter().copied().min())
                                } else {
                                    Err(YangError::FaceResolutionFailed { tri: compact_t })
                                }
                            }
                        }
                    }
                }
            };
            match narrow(exact_hits)? {
                Some(fi) => fi, // exact tier dominates
                None => match narrow(band_hits)? {
                    Some(fi) => fi,
                    None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };
        attributions.push(Some(TriangleAttribution { input, face }));
    }
    let mut triangle_attribution = TriangleAttributionMap { attributions };

    // (6) Topology reconstruction + Stage-4 relocation (PR-YR10). Stage 4 may
    // relocate intersection vertices in-place (onto the exact curves) and, on a
    // §4.5.3 reversal, edge-collapse a mesh vertex — mutating BOTH the mesh and
    // the attribution in lockstep — so both are passed by `&mut` and the
    // tessellation sources come back from `reconstruct_topology`.
    let mut kept_submesh = kept_submesh;
    let (vertices, edges, faces, sources, face_attribution) =
        reconstruct_topology_stage4(&mut kept_submesh, &mut triangle_attribution, a, b, op)?;

    let tessellation = TessellationMap { sources };

    Ok(BRep {
        vertices,
        edges,
        faces,
        mesh: kept_submesh,
        tessellation,
        triangle_attribution,
        face_attribution,
        // A boolean-output BRep has no Stage-1 face_tri_ranges lineage; leave the
        // provenance map empty so a CHAINED boolean falls back to geometric
        // attribution (until the output reconstruction also emits a tri→face map).
        tri_face: Vec::new(),
        forced_rim_n: None,
    })
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    // ── collapse_vertex membrane cancellation ────────────────────────────
    // Spec `specs/yang_collapse_membrane_cancellation.md` (task #121, the
    // N2/F0059 Stage-6 double-cover origin). A twin collapse can turn the
    // two-triangle pleat spanning the twin gap into an EXACT duplicate pair
    // with OPPOSITE windings — a zero-volume doubled flap that must cancel
    // (drop BOTH), restoring manifold edge counts.

    /// The minimal closed pleat: a sliver tetra {a,b,u,v} whose two large
    /// walls (a,b,u)/(a,v,b) become the opposite-winding duplicate after the
    /// twin collapse v→u. Indices 0..=3; positions are irrelevant to the
    /// combinatorial collapse but kept realistic (near-twin apexes).
    fn pleat_tetra_tris() -> Vec<[u32; 3]> {
        vec![[0, 1, 2], [1, 3, 2], [0, 2, 3], [0, 3, 1]]
    }

    fn membrane_fixture_verts() -> Vec<Point3> {
        vec![
            Point3::new(0.0, 0.0, 0.0),       // 0 = a
            Point3::new(1.0, 0.0, 0.0),       // 1 = b
            Point3::new(0.5, 0.4, 0.1),       // 2 = u (survivor twin)
            Point3::new(0.5, 0.4, 0.1000001), // 3 = v (victim twin)
            // Bystander tetra (a separate closed component that must be
            // preserved byte-for-byte through the cancellation).
            Point3::new(3.0, 0.0, 0.0), // 4
            Point3::new(4.0, 0.0, 0.0), // 5
            Point3::new(3.5, 1.0, 0.0), // 6
            Point3::new(3.5, 0.5, 1.0), // 7
        ]
    }

    fn bystander_tetra_tris() -> Vec<[u32; 3]> {
        vec![[4, 5, 6], [4, 6, 7], [4, 7, 5], [5, 7, 6]]
    }

    fn undirected_edge_counts(tris: &[[u32; 3]]) -> std::collections::BTreeMap<(u32, u32), u32> {
        let mut counts = std::collections::BTreeMap::new();
        for tri in tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (a, b) = (tri[i], tri[j]);
                let key = if a < b { (a, b) } else { (b, a) };
                *counts.entry(key).or_insert(0u32) += 1;
            }
        }
        counts
    }

    /// Cancellation branch: the pleat annihilates (both duplicate copies
    /// dropped), the bystander survives byte-identically, every remaining
    /// undirected edge is manifold count-2, and attribution stays lockstep.
    #[test]
    fn collapse_membrane_pleat_cancels_both_copies() {
        let mut tris = pleat_tetra_tris();
        tris.extend(bystander_tetra_tris());
        let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = (0..mesh.tris.len())
            .map(|i| {
                Some(TriangleAttribution {
                    input: InputId::A,
                    face: i as u32,
                })
            })
            .collect();
        collapse_vertex(&mut mesh, &mut attribution, 3, 2);
        // The pleat's two gap slivers drop as degenerate; its two walls map
        // to the SAME sorted triple {0,1,2} with opposite windings — the
        // zero-volume flap — and must BOTH cancel. Only the bystander stays.
        assert_eq!(
            mesh.tris,
            bystander_tetra_tris(),
            "pleat must annihilate; bystander byte-identical"
        );
        assert_eq!(
            attribution
                .iter()
                .map(|a| a.expect("bystander attribution").face)
                .collect::<Vec<_>>(),
            vec![4, 5, 6, 7],
            "attribution must drop the cancelled pair in lockstep"
        );
        for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
            assert_eq!(n, 2, "edge ({a},{b}) not manifold after cancellation");
        }
    }

    /// Same-winding branch: a genuine same-winding double cover is NOT a
    /// cancellable flap — both copies stay for the downstream loud STOPs.
    #[test]
    fn collapse_same_winding_duplicate_is_kept() {
        let mut tris = pleat_tetra_tris();
        // Flip the second wall so the post-collapse duplicates share one
        // winding: (0,3,1) → (0,1,3) maps to (0,1,2) — same cycle as wall 1.
        tris[3] = [0, 1, 3];
        tris.extend(bystander_tetra_tris());
        let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
        collapse_vertex(&mut mesh, &mut attribution, 3, 2);
        let dup_count = mesh
            .tris
            .iter()
            .filter(|t| {
                let mut s = **t;
                s.sort_unstable();
                s == [0, 1, 2]
            })
            .count();
        assert_eq!(
            dup_count, 2,
            "same-winding duplicates must be left for downstream loudness"
        );
        assert_eq!(mesh.tris.len(), 6, "2 kept duplicates + 4 bystander tris");
    }

    /// No-duplicate branch: a clean twin collapse (split-pole octahedron —
    /// the twins own DISJOINT fan sectors) is byte-identical to the plain
    /// index-mapping semantics: seam tents drop as degenerate, fans merge,
    /// nothing cancels.
    #[test]
    fn collapse_without_duplicate_is_byte_identical() {
        // Equator 0..=3, south pole 4, north twins u=5 / v=6.
        let verts: Vec<Point3> = vec![
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(-1.0, 0.0, 0.0),
            Point3::new(0.0, -1.0, 0.0),
            Point3::new(0.0, 0.0, -1.0),
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(0.0, 0.0, 1.0000001),
        ];
        let tris: Vec<[u32; 3]> = vec![
            // south fans
            [1, 0, 4],
            [2, 1, 4],
            [3, 2, 4],
            [0, 3, 4],
            // north: u covers sectors 01/12, v covers 23/30
            [0, 1, 5],
            [1, 2, 5],
            [2, 3, 6],
            [3, 0, 6],
            // seam tents at equator verts 2 and 0
            [5, 2, 6],
            [6, 0, 5],
        ];
        let mut mesh = Mesh::new(verts.clone(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
        let dropped = collapse_vertex(&mut mesh, &mut attribution, 6, 5);
        assert_eq!(dropped, 2, "exactly the two seam tents drop as degenerate");
        let expected: Vec<[u32; 3]> = vec![
            [1, 0, 4],
            [2, 1, 4],
            [3, 2, 4],
            [0, 3, 4],
            [0, 1, 5],
            [1, 2, 5],
            [2, 3, 5],
            [3, 0, 5],
        ];
        assert_eq!(
            mesh.tris, expected,
            "clean collapse must not cancel anything"
        );
        assert_eq!(mesh.verts, verts, "collapse never touches vertex storage");
        for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
            assert_eq!(n, 2, "edge ({a},{b}) not manifold after clean collapse");
        }
    }

    // ── rim junction derivation (N2/F0059 increment 2, banked) ──────────
    // Spec `specs/yang_rim_junction_insertion.md`. Fixture mirrors the
    // integration cylinder fixture (seam-edge encoding).

    fn rj_cylinder(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let crs = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let d = normalize3(axis_dir);
        let bot = axis_point;
        let top = [
            bot[0] + d[0] * height,
            bot[1] + d[1] * height,
            bot[2] + d[2] * height,
        ];
        let abs = [d[0].abs(), d[1].abs(), d[2].abs()];
        let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
            [1.0, 0.0, 0.0]
        } else if abs[1] <= abs[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = normalize3(crs(d, world));
        let verts = vec![
            BRepVertex {
                point: Point3::new(
                    bot[0] + e1[0] * radius,
                    bot[1] + e1[1] * radius,
                    bot[2] + e1[2] * radius,
                ),
            },
            BRepVertex {
                point: Point3::new(
                    top[0] + e1[0] * radius,
                    top[1] + e1[1] * radius,
                    top[2] + e1[2] * radius,
                ),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(bot[0], bot[1], bot[2]),
                    normal: Vector3::new(-d[0], -d[1], -d[2]),
                    radius,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(top[0], top[1], top[2]),
                    normal: Vector3::new(d[0], d[1], d[2]),
                    radius,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(axis_point[0], axis_point[1], axis_point[2]),
                    axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                    radius,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-d[0], -d[1], -d[2]),
                    d: dot(d, bot),
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(d[0], d[1], d[2]),
                    d: -dot(d, top),
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("rj cylinder fixture builds")
    }

    /// The truncated-Steinmetz pair (h/2 < r): axes x and y crossing at
    /// each other's midpoints — the F0059 shape.
    fn rj_truncated_pair(r: f64, h: f64) -> (BRep, BRep) {
        (
            rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
            rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
        )
    }

    /// F0059 class: each cap rim of each operand carries exactly the four
    /// lobe corners `(±h/2, ±√(r²−h²/4))`, exact on the rim circle AND on
    /// the other operand's lateral (spec oracle 1 + I2).
    #[test]
    fn rim_junctions_truncated_steinmetz_four_corners_per_cap() {
        let (r, h) = (0.35f64, 0.5f64);
        let (a, b) = rj_truncated_pair(r, h);
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        let w = (r * r - h * h / 4.0).sqrt();
        for (brep, map, other_axis_is_x) in [(&a, &map_a, true), (&b, &map_b, false)] {
            assert_eq!(
                map.keys().copied().collect::<Vec<_>>(),
                vec![0, 1],
                "both cap rims carry junctions"
            );
            for (&ei, pts) in map.iter() {
                assert_eq!(pts.len(), 4, "four lobe corners per cap rim");
                let Curve::Circle { center, radius, .. } = brep.edges()[ei as usize].curve else {
                    panic!("rim edge is a circle");
                };
                for p in pts {
                    let pa = p.as_array();
                    let ca = center.as_array();
                    let dd = [pa[0] - ca[0], pa[1] - ca[1], pa[2] - ca[2]];
                    let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                    assert!(
                        (dist - radius).abs() <= 1e-12,
                        "I2: junction exactly on the rim circle"
                    );
                    // Exactly on the OTHER operand's lateral: distance to
                    // its axis (x or y axis through the origin) equals r.
                    let lat = if other_axis_is_x {
                        (pa[1] * pa[1] + pa[2] * pa[2]).sqrt()
                    } else {
                        (pa[0] * pa[0] + pa[2] * pa[2]).sqrt()
                    };
                    assert!(
                        (lat - r).abs() <= 1e-12,
                        "I2: junction exactly on the crossing lateral"
                    );
                    // The corner coordinates are the analytic lobe corners.
                    let along = if other_axis_is_x { pa[0] } else { pa[1] };
                    assert!(
                        (along.abs() - h / 2.0).abs() <= 1e-12,
                        "corner sits at ±h/2 along the crossing axis"
                    );
                    assert!(
                        (pa[2].abs() - w).abs() <= 1e-12,
                        "corner sits at ±√(r²−h²/4) in z"
                    );
                }
            }
        }
    }

    /// Rebuild plumbing (spec I1/I3): an empty override map rebuild is
    /// byte-identical; a real map plants every junction as a bit-exact
    /// Stage-1 mesh vertex.
    #[test]
    fn rebuilt_with_rim_overrides_identity_and_insertion() {
        let (a, b) = rj_truncated_pair(0.35, 0.5);
        let same = a
            .rebuilt_with_rim_overrides(&std::collections::BTreeMap::new())
            .expect("empty rebuild");
        assert_eq!(
            same.as_mesh(),
            a.as_mesh(),
            "I1: empty override map is byte-identical"
        );
        let (map_a, _) = rim_junction_overrides(&a, &b);
        let boosted = a
            .rebuilt_with_rim_overrides(&map_a)
            .expect("boosted rebuild");
        for pts in map_a.values() {
            for p in pts {
                assert!(
                    boosted.as_mesh().verts.iter().any(|q| q == p),
                    "junction {p:?} must be a bit-exact Stage-1 mesh vertex"
                );
            }
        }
    }

    /// kv9f1 class (h/2 > r): the seam never reaches the caps — no rim
    /// junctions, both maps empty (spec oracle 2 / branch row 1).
    #[test]
    fn rim_junctions_empty_when_seam_clears_caps() {
        let (a, b) = (
            rj_cylinder([0.0, -0.45, 0.0], [0.0, 1.0, 0.0], 0.2, 0.9),
            rj_cylinder([-0.45, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2, 0.9),
        );
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(map_a.is_empty() && map_b.is_empty());
    }

    /// h/2 == r: each cap plane is exactly TANGENT to the other lateral —
    /// the tangency class is skipped (|δ| ≥ r_b), never inserted.
    #[test]
    fn rim_junctions_tangent_cap_plane_skipped() {
        let (a, b) = rj_truncated_pair_tangent();
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(map_a.is_empty() && map_b.is_empty());
    }

    fn rj_truncated_pair_tangent() -> (BRep, BRep) {
        let (r, h) = (0.35f64, 0.7f64);
        (
            rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
            rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
        )
    }

    /// Candidates beyond the crossing lateral's axial extent are excluded
    /// (spec candidate filter 2): shifting B along its axis puts every
    /// infinite-LATERAL junction outside both operands' extents
    /// (a-rim × b-lateral would sit at x = ±0.245, outside b's
    /// [0.3, 0.65]; b-rim × a-lateral at y = ±0.302, outside a's
    /// [−0.25, 0.25]). The PLANE arm never fires here: cylinder rims are
    /// outside its cone-flanked v1 scope (the demonstrated-need gate —
    /// this population is proven healthy without insertion).
    #[test]
    fn rim_junctions_respect_lateral_extent() {
        let a = rj_cylinder([0.0, -0.25, 0.0], [0.0, 1.0, 0.0], 0.35, 0.5);
        let b = rj_cylinder([0.3, 0.0, 0.0], [1.0, 0.0, 0.0], 0.35, 0.5);
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(
            map_a.is_empty() && map_b.is_empty(),
            "lateral out-of-extent candidates excluded; cylinder rims outside \
             the plane arm's cone-flanked scope"
        );
    }

    // ── Increment 4: plane-face arm + coaxial azimuth propagation ────────
    // Spec `specs/yang_rim_junction_insertion.md` §4a/§4b — the
    // cone-hyperbola junction class (R0004/R0017/R0019/R0044/R0047/R0049):
    // coaxial cone-band rim circles crossing a PLANE face of the other
    // operand.

    /// Coaxial double-frustum lathe on the z-axis: rims (z=0, r0),
    /// (z=1, r1), (z=2, r2), two cone bands sharing the middle rim, planar
    /// caps at both ends. Adjacent radii must differ (genuine cones).
    fn rj_lathe(r0: f64, r1: f64, r2: f64) -> BRep {
        assert!(r0 != r1 && r1 != r2, "bands must be genuine cones");
        let verts = vec![
            BRepVertex {
                point: Point3::new(r0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(r1, 0.0, 1.0),
            },
            BRepVertex {
                point: Point3::new(r2, 0.0, 2.0),
            },
        ];
        let circle = |cz: f64, nz: f64, radius: f64| Curve::Circle {
            center: Point3::new(0.0, 0.0, cz),
            normal: Vector3::new(0.0, 0.0, nz),
            radius,
        };
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: circle(0.0, -1.0, r0),
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: circle(1.0, 1.0, r1),
            },
            BRepEdge {
                start: 2,
                end: 2,
                curve: circle(2.0, 1.0, r2),
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
        ];
        // Cone through profile points (ra, za)-(rb, zb): apex on the axis
        // where the linear radius profile reaches 0; axis_dir points from
        // the apex toward the band.
        let cone = |ra: f64, za: f64, rb: f64, zb: f64| -> Surface {
            let slope = (rb - ra) / (zb - za);
            let z_apex = za - ra / slope;
            let dir = if slope > 0.0 { 1.0 } else { -1.0 };
            Surface::Cone {
                apex: Point3::new(0.0, 0.0, z_apex),
                axis_dir: Vector3::new(0.0, 0.0, dir),
                half_angle: slope.abs().atan(),
            }
        };
        let faces = vec![
            BRepFace {
                surface: cone(r0, 0.0, r1, 1.0),
                outer_loop: vec![0, 3, 1, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: cone(r1, 1.0, r2, 2.0),
                outer_loop: vec![1, 4, 2, 4],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -2.0,
                },
                outer_loop: vec![2],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("rj lathe fixture builds")
    }

    /// Axis-aligned box (the slab operand): 6 polygonal plane faces.
    fn rj_box(lo: [f64; 3], hi: [f64; 3]) -> BRep {
        let v = |x: f64, y: f64, z: f64| BRepVertex {
            point: Point3::new(x, y, z),
        };
        let vertices = vec![
            v(lo[0], lo[1], lo[2]),
            v(hi[0], lo[1], lo[2]),
            v(hi[0], hi[1], lo[2]),
            v(lo[0], hi[1], lo[2]),
            v(hi[0], hi[1], hi[2]),
            v(hi[0], lo[1], hi[2]),
            v(lo[0], lo[1], hi[2]),
            v(lo[0], hi[1], hi[2]),
        ];
        const EDGE_PAIRS: [(u32, u32); 24] = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (2, 1),
            (1, 5),
            (5, 4),
            (4, 2),
            (3, 2),
            (2, 4),
            (4, 7),
            (7, 3),
            (0, 3),
            (3, 7),
            (7, 6),
            (6, 0),
            (1, 0),
            (0, 6),
            (6, 5),
            (5, 1),
        ];
        let edges: Vec<BRepEdge> = EDGE_PAIRS
            .iter()
            .map(|&(start, end)| BRepEdge {
                start,
                end,
                curve: Curve::LineSegment,
            })
            .collect();
        let planes: [([f64; 3], f64); 6] = [
            ([0.0, 0.0, -1.0], lo[2]),
            ([0.0, 0.0, 1.0], -hi[2]),
            ([1.0, 0.0, 0.0], -hi[0]),
            ([0.0, 1.0, 0.0], -hi[1]),
            ([-1.0, 0.0, 0.0], lo[0]),
            ([0.0, -1.0, 0.0], lo[1]),
        ];
        let faces: Vec<BRepFace> = planes
            .iter()
            .enumerate()
            .map(|(i, &(n, d))| BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(n[0], n[1], n[2]),
                    d,
                },
                outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
                inner_loops: Vec::new(),
                reversed: false,
            })
            .collect();
        BRep::new(vertices, edges, faces).expect("rj box fixture builds")
    }

    /// §4a+§4b class oracle: every lathe rim crosses the slab's x = c face
    /// plane transversally → per rim, TWO direct junctions
    /// `(c, ±√(r²−c²), z)` PLUS the other rims' azimuths propagated
    /// exactly onto its own circle. All three rims present the SAME
    /// azimuth multiset (the Stage-1 band-strip alignment invariant I5).
    #[test]
    fn rim_junctions_plane_arm_lathe_slab_all_rims() {
        let (r0, r1, r2) = (1.0f64, 2.0, 0.8);
        let c = 0.75f64;
        let lathe = rj_lathe(r0, r1, r2);
        let slab = rj_box([c, -4.0, -0.5], [4.0, 4.0, 2.5]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(map_s.is_empty(), "the slab has no circle rims");
        assert_eq!(
            map_l.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 2],
            "all three rims carry insertions"
        );
        let mut az_sets: Vec<Vec<f64>> = Vec::new();
        for (&ei, pts) in map_l.iter() {
            let Curve::Circle { center, radius, .. } = lathe.edges()[ei as usize].curve else {
                panic!("rim edge is a circle");
            };
            let cz = center.as_array()[2];
            // 2 direct junctions per rim + 2 propagated from each other rim.
            assert_eq!(pts.len(), 6, "rim {ei}: 2 direct + 4 propagated");
            let mut on_plane = 0usize;
            let mut azimuths: Vec<f64> = Vec::new();
            for pt in pts {
                let pa = pt.as_array();
                let rad = (pa[0] * pa[0] + pa[1] * pa[1]).sqrt();
                assert!(
                    (rad - radius).abs() <= 1e-12,
                    "I2/I5: point exactly on rim {ei}'s circle"
                );
                assert!((pa[2] - cz).abs() <= 1e-12, "point in rim {ei}'s plane");
                if (pa[0] - c).abs() <= 1e-12 {
                    on_plane += 1;
                    let w = (radius * radius - c * c).sqrt();
                    assert!(
                        (pa[1].abs() - w).abs() <= 1e-12,
                        "direct junction at (c, ±√(r²−c²), z)"
                    );
                }
                azimuths.push(pa[1].atan2(pa[0]).rem_euclid(2.0 * std::f64::consts::PI));
            }
            assert_eq!(on_plane, 2, "rim {ei}: exactly two direct junctions");
            azimuths.sort_by(f64::total_cmp);
            az_sets.push(azimuths);
        }
        for k in 1..az_sets.len() {
            assert_eq!(az_sets[k].len(), az_sets[0].len());
            for (a, b) in az_sets[k].iter().zip(az_sets[0].iter()) {
                assert!(
                    (a - b).abs() <= 1e-12,
                    "azimuth multisets align across coaxial rims"
                );
            }
        }
    }

    /// §4a containment: the slab shifted so its x-face plane still crosses
    /// the rim circles but OUTSIDE the face polygon → no insertion.
    #[test]
    fn rim_junctions_plane_arm_containment_outside_face() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let slab = rj_box([0.75, 2.5, -0.5], [4.0, 5.0, 2.5]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(
            map_l.is_empty() && map_s.is_empty(),
            "crossings outside the face polygon must not insert"
        );
    }

    /// §4a parallel skip: a box whose only near face is PARALLEL to the rim
    /// planes (top face containing the middle rim's plane) → no section
    /// line, no insertion; its transversal side faces miss the circles.
    #[test]
    fn rim_junctions_plane_arm_parallel_plane_skipped() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let slab = rj_box([-4.0, -4.0, -1.0], [4.0, 4.0, 1.0]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(
            map_l.is_empty() && map_s.is_empty(),
            "parallel planes have no transversal section line"
        );
    }

    /// §4b vocabulary gate: a full-circle rim owned by a TORUS face (the
    /// kv6d bent-tube profile rim) must never receive insertions — the
    /// band-strip propagation vocabulary covers Cone/Cylinder/Plane only.
    #[test]
    fn rim_junctions_group_gate_drops_torus_rims() {
        // 90° bent tube: torus center origin, axis +z, R=3, r=1 (the kv6d
        // fixture), profile rim e0 at center (3,0,0), normal +y, radius 1.
        let verts = vec![
            BRepVertex {
                point: Point3::new(4.0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 4.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(3.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 1.0, 0.0),
                    radius: 1.0,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 3.0, 0.0),
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    radius: 1.0,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: 4.0,
                },
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Torus {
                    center: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    major_radius: 3.0,
                    minor_radius: 1.0,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, -1.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-1.0, 0.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        let tube = BRep::new(verts, edges, faces).expect("kv6d bent tube builds");
        // The slab's x = 3 face plane crosses profile rim e0 (center
        // (3,0,0), r=1, plane y=0) at (3, 0, ±1) — transversal, contained.
        let slab = rj_box([3.0, -0.5, -2.0], [5.0, 0.5, 2.0]);
        let (map_t, map_s) = rim_junction_overrides(&tube, &slab);
        assert!(
            map_t.is_empty() && map_s.is_empty(),
            "torus-owned rim groups must be dropped by the vocabulary gate"
        );
    }

    /// §4a arc extension (the measured corpus shape — partial revolves):
    /// a half-turn washer sector's OUTER arcs cross the slab plane at ONE
    /// in-sweep azimuth (the mirror root lies in the missing half); the
    /// junction is inserted there and NEVER at the out-of-sweep root, and
    /// §4b propagates the azimuth onto the INNER arcs exactly on-circle.
    #[test]
    fn rim_junctions_plane_arm_partial_arc_rims() {
        // Half-turn CONE-walled washer sector about +x (the plane arm's
        // v1 scope demands cone-flanked rims): trapezoid profile
        // (0,1.0)-(1,1.3)-(1,2.3)-(0,2.0), swept z ≥ 0 (angle π). Arcs:
        // e8 (r=1.0 @ x=0), e9 (r=1.3 @ x=1), e10 (r=2.3 @ x=1),
        // e11 (r=2.0 @ x=0), all centered on the x-axis with normal +x̂.
        let angle = std::f64::consts::PI;
        let prof = [(0.0, 1.0), (1.0, 1.3), (1.0, 2.3), (0.0, 2.0)];
        let mut verts: Vec<BRepVertex> = prof
            .iter()
            .map(|&(x, y)| BRepVertex {
                point: Point3::new(x, y, 0.0),
            })
            .collect();
        for &(x, y) in &prof {
            // Rotation by π about +x̂: (y, z) → (−y, z sign-flipped ≈ 0).
            let (c, s) = (angle.cos(), angle.sin());
            verts.push(BRepVertex {
                point: Point3::new(x, y * c, y * s),
            });
        }
        let seg = |a: u32, b: u32| BRepEdge {
            start: a,
            end: b,
            curve: Curve::LineSegment,
        };
        let mut edges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 3),
            seg(3, 0),
            seg(4, 5),
            seg(5, 6),
            seg(6, 7),
            seg(7, 4),
        ];
        for i in 0..4u32 {
            let (x, y) = prof[i as usize];
            edges.push(BRepEdge {
                start: i,
                end: i + 4,
                curve: Curve::Circle {
                    center: Point3::new(x, 0.0, 0.0),
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    radius: y,
                },
            });
        }
        let (a0, a1, a2, a3) = (8u32, 9u32, 10u32, 11u32);
        let faces = vec![
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: vec![],
                reversed: false,
            },
            // End cap after a π sweep: the z = 0 plane again, outward −ẑ
            // rotated → +ẑ... outward normal is R_x(π)·ẑ = −ẑ → (0,0,-1)?
            // The kv6b fixture computes (0, −sin α, cos α) = (0, 0, −1).
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                // Inner CONE wall (cavity sense): r = 1.0 @ x=0 → 1.3 @
                // x=1, slope 0.3, apex on the axis at x = −1.0/0.3.
                surface: Surface::Cone {
                    apex: Point3::new(-1.0 / 0.3, 0.0, 0.0),
                    axis_dir: Vector3::new(1.0, 0.0, 0.0),
                    half_angle: 0.3f64.atan(),
                },
                outer_loop: vec![0, a1, 4, a0],
                inner_loops: vec![],
                reversed: true,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    d: -1.0,
                },
                outer_loop: vec![1, a2, 5, a1],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                // Outer CONE wall: r = 2.0 @ x=0 → 2.3 @ x=1, slope 0.3,
                // apex at x = −2.0/0.3.
                surface: Surface::Cone {
                    apex: Point3::new(-2.0 / 0.3, 0.0, 0.0),
                    axis_dir: Vector3::new(1.0, 0.0, 0.0),
                    half_angle: 0.3f64.atan(),
                },
                outer_loop: vec![2, a3, 6, a2],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-1.0, 0.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![3, a0, 7, a3],
                inner_loops: vec![],
                reversed: false,
            },
        ];
        let sector = BRep::new(verts, edges, faces).expect("washer sector builds");
        // Slab beyond y = −1.5: its y = −1.5 face plane crosses the OUTER
        // arcs (r = 2.3, 2.0) at z = +√(r² − 2.25) — only z > 0 is in the
        // sweep (the mirror root lies in the missing half). The inner arcs
        // (r = 1.0, 1.3) never reach y = −1.5 and receive only the
        // propagated cluster azimuths.
        let slab = rj_box([-1.0, -4.0, -4.0], [2.0, -1.5, 4.0]);
        let (map_x, map_s) = rim_junction_overrides(&sector, &slab);
        assert!(map_s.is_empty(), "the slab has no circle rims");
        assert_eq!(
            map_x.keys().copied().collect::<Vec<_>>(),
            vec![8, 9, 10, 11],
            "outer arcs carry direct junctions; inner arcs the propagated azimuths"
        );
        for (&ei, pts) in map_x.iter() {
            let Curve::Circle { center, radius, .. } = sector.edges()[ei as usize].curve else {
                panic!("arc edge is a circle");
            };
            // TWO clusters (one per outer arc's distinct junction azimuth),
            // both inside every arc's sweep window.
            assert_eq!(pts.len(), 2, "arc {ei}: both cluster azimuths inserted");
            let ca = center.as_array();
            for pt in pts {
                let pa = pt.as_array();
                assert!(pa[2] > 0.0, "arc {ei}: insertion inside the sweep window");
                let rad = ((pa[1] - ca[1]).powi(2) + (pa[2] - ca[2]).powi(2)).sqrt();
                assert!(
                    (rad - radius).abs() <= 1e-12,
                    "I2/I5: insertion exactly on arc {ei}'s circle"
                );
                assert!(
                    (pa[0] - ca[0]).abs() <= 1e-12,
                    "insertion in arc {ei}'s plane"
                );
            }
            if ei >= 10 {
                // Outer arcs contain their own DIRECT junction at
                // (x, −1.5, √(r²−2.25)) bit-near exactly.
                let w = (radius * radius - 2.25).sqrt();
                assert!(
                    pts.iter().any(|pt| {
                        let pa = pt.as_array();
                        (pa[1] + 1.5).abs() <= 1e-12 && (pa[2] - w).abs() <= 1e-12
                    }),
                    "outer arc {ei}: direct junction at (x, −1.5, √(r²−2.25)) missing"
                );
            }
        }
    }

    /// §4a disc containment: a cylinder's cap DISC (circle-bounded loop)
    /// admits only junctions within its radius — the R0019/R0044 shape.
    #[test]
    fn rim_junctions_plane_arm_disc_cap_containment() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        // Cylinder along +x from x = 0.75, radius 1.3, centered at z = 1:
        // its x = 0.75 cap disc admits rim0's junction (distance 1.20 from
        // the cap center) and rim2's (1.04) but NOT rim1's (1.854 > 1.3).
        let cyl = rj_cylinder([0.75, 0.0, 1.0], [1.0, 0.0, 0.0], 1.3, 3.25);
        let (map_l, _map_c) = rim_junction_overrides(&lathe, &cyl);
        let c = 0.75f64;
        let cap_center = [0.75f64, 0.0, 1.0];
        // Every on-cap-plane insertion respects the disc radius.
        for pts in map_l.values() {
            for pt in pts {
                let pa = pt.as_array();
                if (pa[0] - c).abs() <= 1e-9 {
                    let dd = [
                        pa[0] - cap_center[0],
                        pa[1] - cap_center[1],
                        pa[2] - cap_center[2],
                    ];
                    let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                    assert!(
                        dist <= 1.3 + 1e-9,
                        "on-cap junction outside the disc: {pa:?} (dist {dist})"
                    );
                }
            }
        }
        // The in-disc junctions on rim0 ARE inserted (red oracle).
        let w0 = (1.0f64 - c * c).sqrt();
        let rim0 = map_l.get(&0).expect("rim0 carries junctions");
        for sy in [-1.0f64, 1.0] {
            assert!(
                rim0.iter().any(|p| {
                    let pa = p.as_array();
                    (pa[0] - c).abs() <= 1e-9
                        && (pa[1] - sy * w0).abs() <= 1e-9
                        && pa[2].abs() <= 1e-9
                }),
                "rim0 in-disc junction (c, {sy}·√(1−c²), 0) missing"
            );
        }
        // And rim1's on-cap-plane candidates (outside the disc) are NOT.
        if let Some(rim1) = map_l.get(&1) {
            assert!(
                rim1.iter().all(|p| (p.as_array()[0] - c).abs() > 1e-9),
                "rim1 candidates on the cap plane must be rejected by the disc"
            );
        }
    }

    /// §4d: the certificate band is the TAU_WORK floor at unit scale,
    /// covers the measured ~1.2·ε·L ULP noise at the R0017 magnitude, and
    /// stays orders below every measured junction sagitta at its own
    /// scale (band monotonicity, spec I7).
    #[test]
    fn junction_certificate_band_is_scale_aware() {
        // Unit scale: the floor.
        let plane_unit = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -0.5,
        };
        assert_eq!(
            junction_certificate_band([0.1, 0.2, 0.5], plane_unit),
            cad_primitives::TAU_WORK
        );
        // R0017 magnitude (~4e3 coords, cone apex ~3e3): the measured
        // already-exact junction residual 1.36e-12 must certify, while
        // the measured chord sagitta 10.7 must stay ≥ 1e6× above.
        let cone_large = Surface::Cone {
            apex: Point3::new(-3216.2, -1481.6, 1664.5),
            axis_dir: Vector3::new(0.7596, 0.0, -0.6504),
            half_angle: 1.0477,
        };
        let band = junction_certificate_band([-3901.5, -2954.8, -2747.5], cone_large);
        assert!(
            band >= 1.36e-12,
            "covers evaluation-precision noise: {band}"
        );
        assert!(band <= 1e-10, "stays sub-sagitta by ≥6 orders: {band}");
        // R0047 micro magnitude (~3e-4): the floor rules, and the measured
        // 1.35e-7 sagitta can never certify.
        let cone_micro = Surface::Cone {
            apex: Point3::new(2.68e-4, -2.09e-4, 2.76e-4),
            axis_dir: Vector3::new(-0.4092, 0.0, -0.9124),
            half_angle: 0.5959,
        };
        let band_micro = junction_certificate_band([1.02e-4, -1.53e-4, 1.59e-4], cone_micro);
        assert_eq!(band_micro, cad_primitives::TAU_WORK);
        assert!(band_micro < 1.35e-7 / 1e4, "micro sagitta stays loud");
    }

    /// §4c: a group-consistent insertion (one azimuth on all three coaxial
    /// rims) tessellates the double-frustum watertight, with every inserted
    /// point a bit-exact Stage-1 mesh vertex.
    #[test]
    fn cone_bands_with_inserted_shared_rim_tessellate_watertight() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let th = 0.6f64;
        let mut map: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        for (ei, r, z) in [(0u32, 1.0f64, 0.0f64), (1, 2.0, 1.0), (2, 0.8, 2.0)] {
            map.insert(ei, vec![Point3::new(r * th.cos(), r * th.sin(), z)]);
        }
        let boosted = lathe
            .rebuilt_with_rim_overrides(&map)
            .expect("group-consistent insertion tessellates");
        let mesh = boosted.as_mesh();
        for pts in map.values() {
            for pt in pts {
                assert!(
                    mesh.verts.iter().any(|q| q == pt),
                    "inserted point {pt:?} must be a bit-exact mesh vertex"
                );
            }
        }
        // Watertight: every directed edge pairs with its reverse.
        let mut counts: std::collections::HashMap<(u32, u32), i64> =
            std::collections::HashMap::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
            }
        }
        for (&(s, e), &fwd) in &counts {
            let rev = counts.get(&(e, s)).copied().unwrap_or(0);
            assert_eq!(
                fwd, rev,
                "unpaired half-edge ({s},{e}) after shared-rim insertion"
            );
        }
    }

    // ── M5 surface-pair plumbing (Y1–Y3) ─────────────────────────────────

    fn qcyl(ap: [f64; 3], ad: [f64; 3], r: f64) -> ssi_rs::QuadricSurface {
        ssi_rs::QuadricSurface::Cylinder {
            axis_point: Point3::new(ap[0], ap[1], ap[2]),
            axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
            radius: r,
        }
    }

    /// Y1: `SsiCurve::SurfacePair` maps to `Curve::SurfacePair` carrying both
    /// operands field-for-field as yang `Surface::Cylinder`s.
    #[test]
    fn m5_ssi_surface_pair_maps_to_curve_surface_pair() {
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.5);
        let curve = ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
            .expect("cyl×cyl surface pair maps");
        match curve {
            Curve::SurfacePair {
                a: Surface::Cylinder { radius: ra, .. },
                b: Surface::Cylinder { radius: rb, .. },
            } => {
                assert_eq!(ra, 1.0);
                assert_eq!(rb, 0.5);
            }
            other => panic!("expected Curve::SurfacePair of two cylinders, got {other:?}"),
        }
    }

    /// Y1: a non-cylinder operand (no producer yet) rejects loudly.
    #[test]
    fn m5_surface_pair_non_cylinder_operand_rejected() {
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let plane = ssi_rs::QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        assert!(ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: plane }).is_err());
    }

    /// Y2: on-both-surfaces membership — a point exactly on the perpendicular
    /// unequal-R curve passes; a point off either cylinder by ≫ tol fails.
    #[test]
    fn m5_surface_pair_membership() {
        // x²+y²=1 ∧ x²+z²=¼ : point (0, 1, ½) lies on both.
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        assert!(curve_contains_point(
            &sp,
            Point3::new(0.0, 1.0, 0.5),
            1e-9,
            None
        ));
        // Off cylinder b radially by 0.1 ≫ tol.
        assert!(!curve_contains_point(
            &sp,
            Point3::new(0.0, 1.0, 0.6),
            1e-9,
            None
        ));
    }

    /// Y3: the surface-pair tangent at a point is `n̂_a × n̂_b`. At (0, 1, ½)
    /// the cylinder-a radial normal is +ŷ and cylinder-b radial normal is +ẑ,
    /// so the tangent is ±x̂.
    #[test]
    fn m5_surface_pair_tangent_is_normal_cross() {
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        let t = curve_tangent_at(&sp, Point3::new(0.0, 1.0, 0.5)).expect("transversal ⇒ tangent");
        assert!(t[0].abs() > 0.999, "tangent should be ±x̂, got {t:?}");
        assert!(t[1].abs() < 1e-9 && t[2].abs() < 1e-9);
    }

    /// Y3/Y4 failure mode: tangent (parallel normals) → no tangent (None), so
    /// the candidate stays non-tie-breakable and the loud stop stands.
    #[test]
    fn m5_surface_pair_tangent_none_at_tangency() {
        // Externally tangent unit cylinders touch along x=1,y=0: both normals
        // are ±x̂ on the contact line ⇒ parallel ⇒ no finite tangent.
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        assert!(curve_tangent_at(&sp, Point3::new(1.0, 0.0, 0.0)).is_none());
    }

    // ── M5 cone-pair producer (Y1–Y3 with Cone operands) ─────────────────

    fn qcone(apex: [f64; 3], ad: [f64; 3], alpha: f64) -> ssi_rs::QuadricSurface {
        ssi_rs::QuadricSurface::Cone {
            apex: Point3::new(apex[0], apex[1], apex[2]),
            axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
            half_angle: alpha,
        }
    }

    /// Y1: a cone-pair `SsiCurve::SurfacePair` maps to `Curve::SurfacePair`
    /// carrying both `Surface::Cone` operands field-for-field (cone-pair
    /// producer). A cyl×cone mixed pair maps too.
    #[test]
    fn m5_cone_pair_maps_to_curve_surface_pair() {
        let a = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let b = qcone([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0_f64.atan());
        match ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
            .expect("cone×cone surface pair maps")
        {
            Curve::SurfacePair {
                a: Surface::Cone { half_angle: ha, .. },
                b: Surface::Cone { half_angle: hb, .. },
            } => {
                assert_eq!(ha, std::f64::consts::FRAC_PI_4);
                assert_eq!(hb, 3.0_f64.atan());
            }
            other => panic!("expected Curve::SurfacePair of two cones, got {other:?}"),
        }
        // Mixed cyl×cone also maps (both operand kinds supported).
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let cone = qcone(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        assert!(matches!(
            ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: cone }),
            Ok(Curve::SurfacePair {
                a: Surface::Cylinder { .. },
                b: Surface::Cone { .. }
            })
        ));
    }

    /// Y2: on-both-surfaces membership for a cone∩cylinder curve. The z-axis
    /// cone `radial = |h|·tan(π/4) = |h|` meets the z-axis cylinder `radial = 1`
    /// on the circle `radial = 1, h = ±1`; the point (1, 0, 1) lies on both.
    #[test]
    fn m5_cone_pair_membership() {
        let cone = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
        assert!(curve_contains_point(
            &sp,
            Point3::new(1.0, 0.0, 1.0),
            1e-9,
            None
        ));
        // Off the cone (h=1 needs radial=1, but radial here is 1.2) by ≫ tol.
        assert!(!curve_contains_point(
            &sp,
            Point3::new(1.2, 0.0, 1.0),
            1e-9,
            None
        ));
    }

    /// Y3: the cone-pair tangent at a transversal point is `n̂_a × n̂_b`. At
    /// (1, 0, 1) the π/4 cone normal is `(x̂ − ẑ)/√2` and the cylinder radial
    /// normal is `x̂`; their cross is ∓ŷ.
    #[test]
    fn m5_cone_pair_tangent_is_normal_cross() {
        let cone = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
        let t = curve_tangent_at(&sp, Point3::new(1.0, 0.0, 1.0)).expect("transversal ⇒ tangent");
        assert!(t[1].abs() > 0.999, "tangent should be ±ŷ, got {t:?}");
        assert!(t[0].abs() < 1e-9 && t[2].abs() < 1e-9);
    }

    /// Y4: a perturbed near-curve point relocates onto both surfaces of a
    /// cone∩cylinder pair (the generic Newton engine handles Cone operands).
    #[test]
    fn m5_cone_pair_relocation_onto_both() {
        let cone = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        // Perturb the true curve point (1,0,1) off both surfaces.
        let p = relocate_onto_implicit_pair(Point3::new(1.02, 0.03, 0.98), cone, cyl)
            .expect("near-curve point relocates");
        assert!(signed_distance_to_surface(cone, p).unwrap().abs() < 1e-9);
        assert!(signed_distance_to_surface(cyl, p).unwrap().abs() < 1e-9);
    }

    // ── Case-IV phantom guard (spec `yang_case_iv_phantom_guard`) ────────

    /// Minimal solid cylinder B-Rep (two rims + seam) for the guard tests.
    fn guard_cyl(cx: f64, cy: f64, r: f64, h: f64) -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(cx + r, cy, 0.0),
            },
            BRepVertex {
                point: Point3::new(cx + r, cy, h),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(cx, cy, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -h,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("guard cylinder")
    }

    /// The measured F0088 pair: a nested-disjoint tool inside the plate
    /// cylinder with gap 0.0115 < the natural N=14 sagitta — the guard must
    /// demand a finer N (34 at these radii: the smallest N with
    /// sag(R,N)+sag(r,N) ≤ gap/2).
    #[test]
    fn phantom_guard_nested_disjoint_demands_finer_n() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.2243, 0.0, 0.042871795720997065, 0.23);
        let n = phantom_min_rim_segments(&plate, &tool).expect("guard must fire");
        let gap = 1.2787008340600021 - 1.2243 - 0.042871795720997065;
        let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
        assert!(
            sag(1.2787008340600021, n) + sag(0.042871795720997065, n) <= gap / 2.0,
            "derived N={n} must clear the analytic gap with the factor-2 margin"
        );
        assert!(
            sag(1.2787008340600021, n - 1) + sag(0.042871795720997065, n - 1) > gap / 2.0,
            "derived N={n} must be MINIMAL (no over-refinement)"
        );
    }

    /// A crossing pair (the tool overlaps the plate wall) has no analytic
    /// gap — a real intersection curve exists and SSI refines it. No boost.
    #[test]
    fn phantom_guard_crossing_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.26, 0.0, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// A far-disjoint pair derives a tiny N that both solids' natural
    /// Stage-1 N already satisfies — the self-limiting gate drops it.
    #[test]
    fn phantom_guard_far_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(0.3, 0.1, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// Build one B-Rep carrying TWO cylinders (a plate wall + a hole at
    /// `(hx, hy)` with radius `hr`).
    fn two_cyl_brep(hx: f64, hy: f64, hr: f64) -> BRep {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(hx, hy, hr, 0.23);
        let mut verts = plate.vertices.clone();
        let mut edges = plate.edges.clone();
        let mut faces = plate.faces.clone();
        let (vo, eo) = (verts.len() as u32, edges.len() as u32);
        verts.extend(tool.vertices.iter().cloned());
        for e in &tool.edges {
            edges.push(BRepEdge {
                start: e.start + vo,
                end: e.end + vo,
                curve: e.curve,
            });
        }
        for f in &tool.faces {
            faces.push(BRepFace {
                surface: f.surface,
                outer_loop: f.outer_loop.iter().map(|&e| e + eo).collect(),
                inner_loops: Vec::new(),
                reversed: f.reversed,
            });
        }
        BRep::new(verts, edges, faces).expect("combined solid")
    }

    /// INTRA-solid pair (the chained F0088 output: hole 4's lateral 0.0115
    /// from the plate wall inside ONE solid): STAGE 1's own N selection must
    /// fold the pair's derived N in — otherwise ANY tessellation of the
    /// solid (input conversion included) puts the cap's outer-rim chords
    /// across the hole rim and the planar CDT gets crossing constraints
    /// (measured corpus F0088 ops 7/15, `CDT triangulation failed`). The
    /// near-rim solid must tessellate strictly denser than the same solid
    /// with its hole far from the wall.
    #[test]
    fn stage1_intra_solid_phantom_fold_densifies_rims() {
        let near = two_cyl_brep(1.2243, 0.0, 0.042871795720997065);
        let far = two_cyl_brep(0.3, 0.1, 0.042871795720997065);
        assert!(
            near.as_mesh().num_verts() > far.as_mesh().num_verts(),
            "near-rim solid must tessellate denser (near {} verts vs far {})",
            near.as_mesh().num_verts(),
            far.as_mesh().num_verts()
        );
        // And the cross-pair guard is silent for it — the intra fold lives
        // in Stage 1, not in the pair analysis.
        let partner = guard_cyl(10.0, 10.0, 0.1, 0.23);
        assert_eq!(phantom_min_rim_segments(&near, &partner), None);
    }

    /// An operand without B-Rep faces (the `from_mesh` chained-output
    /// degenerate) has no cylinder faces to scan — byte-identical path.
    #[test]
    fn phantom_guard_faceless_operand_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let raw = BRep::from_mesh(plate.as_mesh().clone());
        assert_eq!(phantom_min_rim_segments(&plate, &raw), None);
        assert_eq!(phantom_min_rim_segments(&raw, &plate), None);
    }

    // R0072: position tie-break for near-coincident PARALLEL line candidates
    // (`select_disjoint_parallel_line`). Mirrors the instrumented R0072 edge
    // (2,143): two parallel generators whose endpoint-distance intervals are
    // disjoint → the lower (nearer) one is selected. The numbers are the live
    // probe values (cand0 ≈ 2.0e-5, cand1 ≈ 3.3e-5).
    #[test]
    fn r0072_parallel_line_position_tiebreak() {
        let dir = Vector3::new(
            0.539_214_627_766_961_7,
            -0.348_918_218_865_836_5,
            -0.766_487_874_493_543,
        );
        // Two parallel lines offset along a perpendicular `n̂` (⟂ dir), 2e-5 and
        // 3.3e-5 from the edge endpoints which sit on the origin segment.
        let n = {
            // any unit vector ⟂ dir
            let d = normalize3(dir.as_array());
            let t = [1.0, 0.0, 0.0];
            let dot = t[0] * d[0] + t[1] * d[1] + t[2] * d[2];
            let p = [t[0] - dot * d[0], t[1] - dot * d[1], t[2] - dot * d[2]];
            normalize3(p)
        };
        let line_at = |off: f64| (Point3::new(off * n[0], off * n[1], off * n[2]), dir);
        let cand0 = line_at(2.0e-5);
        let cand1 = line_at(3.3e-5);
        let p_s = Point3::new(0.0, 0.0, 0.0);
        let p_e = Point3::new(
            d_scale(dir, 1e-4)[0],
            d_scale(dir, 1e-4)[1],
            d_scale(dir, 1e-4)[2],
        );

        // Disjoint intervals → the nearer line (index 0) wins regardless of order.
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, cand1], p_s, p_e),
            Some(0)
        );
        assert_eq!(
            select_disjoint_parallel_line(&[cand1, cand0], p_s, p_e),
            Some(1)
        );

        // OVERLAPPING intervals (generators merged below resolution) → no clear
        // winner → None (the caller keeps its loud `AmbiguousCurve`). Put the two
        // lines symmetrically about the segment so each endpoint is equidistant.
        let near_a = line_at(2.0e-5);
        let near_b = line_at(-2.0e-5);
        assert_eq!(
            select_disjoint_parallel_line(&[near_a, near_b], p_s, p_e),
            None
        );

        // NON-parallel candidates → None (the tangent discriminator's job).
        let crossing = (Point3::new(0.0, 0.0, 0.0), Vector3::new(n[0], n[1], n[2]));
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, crossing], p_s, p_e),
            None
        );

        // Fewer than two candidates → None.
        assert_eq!(select_disjoint_parallel_line(&[cand0], p_s, p_e), None);
    }

    fn d_scale(v: Vector3, s: f64) -> [f64; 3] {
        let d = normalize3(v.as_array());
        [d[0] * s, d[1] * s, d[2] * s]
    }

    // PR-YR10 N3 regression (Yang §4.5.3): a U-turn at p_r — consecutive points
    // double back so v1 ≈ −v2 ⇒ |t̃| ≈ 0 — IS a reversal. The paper places the
    // collinear/degenerate-t̃ case WITHIN the reversal subset ("directly detect
    // the reversal, avoiding the angle comparisons"). p_b=(0,0,0) → p_r=(1,0,0)
    // → p_n=(0.5,0,0) reverses direction (v1=+x, v2=−x, t̃=0). The degenerate
    // branch must report a reversal. (Was the N3 logic inversion: returned
    // `false` = "healthy", silently failing to correct the very reversal §4.5.3
    // exists for; reachable whenever relocation produces an out-of-order point.)

    // PR-6 (coincident-cylinder rim conformal weld). Locks the two invariants
    // that make the curved-input rim weld a conformal exact-identity merge of
    // redundant reconstructions — NOT a tolerance bucket that could mask
    // unpaired edges (the reverted F0057 hazard):
    //   (1) two sub-ULP rim duplicates of one analytic point are BOTH on the
    //       cylinder (within the analytic band) AND within the cluster band,
    //       so they fuse;
    //   (2) two GENUINELY distinct rim points (≥ MIN_FEATURE_SIZE apart, here
    //       the ~1e-4 chord spacing) are on the cylinder but FAR outside the
    //       cluster band, so they never fuse.
    #[test]
    fn pr6_rim_weld_fuses_only_sub_ulp_duplicates() {
        let cyl = stage0::PairCylinder {
            axis_point: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 1.0,
            band: 1e-7,
            opposite: true,
        };
        let base = [1.0, 0.0, 0.3];
        // (1) A sub-ULP duplicate: perturb the in-plane coord by ~2 ULPs.
        let twin = [1.0 + 2.0 * f64::EPSILON, 0.0, 0.3];
        let scale = base
            .iter()
            .chain(twin.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
        assert!(
            centroid_on_cylinder(base, &cyl) <= cyl.band,
            "base rim point must be on the cylinder"
        );
        assert!(
            centroid_on_cylinder(twin, &cyl) <= cyl.band,
            "sub-ULP twin must still be on the cylinder"
        );
        assert!(
            (0..3).all(|k| (base[k] - twin[k]).abs() <= cluster_band),
            "sub-ULP twin must be within the cluster band ⇒ fuses"
        );
        // (2) A genuinely distinct rim point ~1e-4 away along the rim: on the
        // cylinder, but FAR outside the cluster band ⇒ never fused.
        let theta = 1e-4_f64;
        let distinct = [theta.cos(), theta.sin(), 0.3];
        assert!(
            centroid_on_cylinder(distinct, &cyl) <= cyl.band,
            "the distinct rim point is also exactly on the cylinder"
        );
        assert!(
            (0..3).any(|k| (base[k] - distinct[k]).abs() > cluster_band),
            "a genuinely distinct rim point (≥ chord spacing) must lie OUTSIDE \
             the cluster band so the conformal weld never fuses it (no \
             tolerance-bucket masking)"
        );
    }

    // KV15 (spec `kv15_mixed_operand_planar_near_weld` §4): the mixed-operand
    // per-vertex near-weld. W2 — a planar-only femto pair (2 ULPs) fuses to
    // the min index; W3 — a curved-adjacent root never near-welds (kv9
    // junction-duplicate protection); W5 — genuinely distinct features
    // (≥ MIN_FEATURE_SIZE) sit far outside the band and never fuse.
    #[test]
    fn kv15_planar_femto_pair_welds_to_min_index() {
        let base = p(1.0, 0.0, 0.3);
        let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
        let verts = vec![base, twin];
        let mut weld = vec![0u32, 1u32];
        kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
        assert_eq!(
            weld,
            vec![0, 0],
            "W2: a planar femto pair fuses, min-index survivor"
        );
    }

    #[test]
    fn kv15_curved_adjacent_root_never_near_welds() {
        let base = p(1.0, 0.0, 0.3);
        let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
        let verts = vec![base, twin];
        for flags in [[true, false], [false, true], [true, true]] {
            let mut weld = vec![0u32, 1u32];
            kv15_near_weld_pass(&verts, &mut weld, &flags);
            assert_eq!(
                weld,
                vec![0, 1],
                "W3: a curved-adjacent root (flags {flags:?}) must keep bit-exact \
                 identity — Stage-4 owns junction-duplicate collapse"
            );
        }
    }

    #[test]
    fn kv15_distinct_features_never_fuse() {
        // 1e-4 apart at coordinate scale ~1 — eight orders beyond the
        // TAU_WORK·(1+scale) band; the pair must never fuse (no
        // tolerance-bucket masking, the reverted-F0057 hazard).
        let verts = vec![p(1.0, 0.0, 0.3), p(1.0 + 1.0e-4, 0.0, 0.3)];
        let mut weld = vec![0u32, 1u32];
        kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
        assert_eq!(
            weld,
            vec![0, 1],
            "W5: sub-floor is the mint-site's job; ≥-floor never fuses"
        );
    }

    /// KV15 spec W4 + §3 eligibility: only positively-proven all-planar
    /// descent yields an eligible (non-curved) vertex. Empty provenance,
    /// sentinel / out-of-range `tri_face` entries, an unknown face, and a
    /// non-planar face all mark every vertex of the triangle curved.
    #[test]
    fn kv15_eligibility_is_conservative() {
        let tris = vec![[0u32, 1, 2]];
        let planar_a = |k: u32, fi: u32| (k == 0 && fi == 7).then_some(true);
        // Positively proven planar descent → eligible.
        let src = vec![vec![(LaInputId(0), 0u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src, &[7], &[], planar_a),
            vec![false; 3],
            "proven planar descent is eligible"
        );
        // Empty provenance (sidecar producer) → curved.
        assert_eq!(
            kv15_curved_touch(3, &tris, &[Vec::new()], &[7], &[], planar_a),
            vec![true; 3],
            "W4: empty provenance stays bit-exact"
        );
        // Sentinel tri_face entry → curved.
        assert_eq!(
            kv15_curved_touch(3, &tris, &src, &[u32::MAX], &[], planar_a),
            vec![true; 3],
            "sentinel face map entry stays bit-exact"
        );
        // Out-of-range local tri index → curved.
        let src_oob = vec![vec![(LaInputId(0), 9u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src_oob, &[7], &[], planar_a),
            vec![true; 3],
            "out-of-range provenance stays bit-exact"
        );
        // Non-planar face → curved; input B routes through tri_face_b.
        let cyl_b = |k: u32, fi: u32| (k == 1 && fi == 3).then_some(false);
        let src_b = vec![vec![(LaInputId(1), 0u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src_b, &[], &[3], cyl_b),
            vec![true; 3],
            "a curved-face descendant marks its vertices ineligible"
        );
        // Multi-parent (coplanar overlap): ONE curved parent poisons the tri.
        let mixed = vec![vec![(LaInputId(0), 0u32), (LaInputId(1), 0u32)]];
        let planar_a_cyl_b = |k: u32, fi: u32| ((k, fi) == (0, 7)).then_some(true).or(Some(false));
        assert_eq!(
            kv15_curved_touch(3, &tris, &mixed, &[7], &[3], planar_a_cyl_b),
            vec![true; 3],
            "any curved parent of a multi-parent tri stays bit-exact"
        );
    }

    // KV15b (spec `kv15b_mint_site_subresolution_collapse` §7): the
    // emission collapse of sub-`TAU_MODEL` intersection segments.
    fn kv15b_map(segs: &[(u32, u32)]) -> std::collections::BTreeMap<(u32, u32), Curve> {
        segs.iter()
            .map(|&(a, b)| ((a.min(b), a.max(b)), Curve::LineSegment))
            .collect()
    }

    #[test]
    fn kv15b_subresolution_intersection_segment_collapses() {
        // B1/I1: a 5e-8 intersection segment (0,1) collapses; min index
        // survives with its original bits; the degenerate tri drops.
        let twin = p(5.0e-8, 0.0, 0.0);
        let mut mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), twin, p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(0, 1)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 2, 3]],
            "degenerate tri dropped, twin remapped"
        );
        assert_eq!(
            mesh.verts[0],
            p(0.0, 0.0, 0.0),
            "I1: the survivor keeps its own exact coordinates"
        );
        assert_eq!(attr.len(), 1, "attribution stays in lockstep with tris");
    }

    #[test]
    fn kv15b_supraresolution_segment_untouched() {
        // B2/I2: 2e-7 ≥ TAU_MODEL — never collapses (a mutation widening the
        // band to MIN_FEATURE_SIZE must fail here: 2e-7 < 1e-6).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(2.0e-7, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(0, 1)]);
        assert!(!collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 1, 3], [1, 2, 3]],
            "B2: ≥ TAU_MODEL stays"
        );
    }

    #[test]
    fn kv15b_non_intersection_edge_untouched() {
        // B4/I3: the sub-TAU pair (0,1) is NOT an intersection segment —
        // inherited operand geometry (micro-profile corners) never collapses
        // (a mutation dropping the intersection-membership gate fails here).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(1, 2)]); // only the LONG edge is intersection
        assert!(!collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 1, 3], [1, 2, 3]],
            "B4: a sub-TAU NON-intersection edge is inherited geometry — untouched"
        );
    }

    #[test]
    fn kv15b_twin_chain_resolves_to_single_survivor() {
        // B5: chain 0–1–2 with both links sub-TAU (5e-8 + 4e-8): both
        // collapse onto vertex 0 through the redirect (no chain drift beyond
        // the original twin cluster; exact-zero pairs B3 are never touched).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(9.0e-8, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
        );
        let mut attr = vec![None; 3];
        let map = kv15b_map(&[(0, 1), (1, 2)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 3, 4]],
            "B5: both twins collapse onto the min index; degenerate tris drop"
        );
    }

    // Spec `yang_stage6_sliver_topology` amendment 1 (S7): the
    // certainly-fatal chord split + null-excursion cancellation.
    fn s7_info(cycles: Vec<Vec<(u32, u32)>>) -> PatchInfo {
        PatchInfo {
            cycles,
            input: InputId::A,
            inherited: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            face_idx: 0,
            input_reversed: false,
            had_fold_sliver: false,
        }
    }

    fn s7_mesh() -> Mesh {
        Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),   // 0: chord start
                p(0.374, 0.0, 0.0), // 1: on the chord (exact)
                p(1.0, 0.0, 0.0),   // 2: chord end
                p(0.5, 1.0, 0.0),   // 3: apex of loop A
                p(0.5, -1.0, 0.0),  // 4: apex of loop B
                p(0.2, -1.0, 0.0),  // 5: apex of the second chord user (benign T)
            ],
            vec![[0, 2, 3], [1, 2, 4]],
        )
    }

    #[test]
    fn s7_fatal_chord_splits_and_spur_cancels() {
        // Loop A walks a spur (1→0) + the chord (0,2) over vertex 1; loop B
        // walks (2→1). Chord use-count 1, complementary {0,1}/{1,2} both
        // present → split at 1; the spur then cancels (amendment 1a) and A
        // emerges as the clean triangle 1→2→3→1.
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(
            out[0][0],
            vec![(1, 2), (2, 3), (3, 1)],
            "S7: chord split at the on-segment vertex, spur cancelled"
        );
        assert_eq!(out[1][0], infos[1].cycles[0], "loop B untouched");
    }

    #[test]
    fn s7_benign_t_junction_untouched() {
        // The chord (0,2) is walked by TWO loops (use 2) while the
        // complementary chain {0,1}/{1,2} ALSO exists (loops A + C) — this
        // isolates the use==1 gate: a mutation dropping it splits here and
        // fails (the reference-parity guard for benign T-junctions).
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 0), (0, 5), (5, 2)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(out[0][0], infos[0].cycles[0], "use-2 chord never splits");
        assert_eq!(out[1][0], infos[1].cycles[0]);
    }

    #[test]
    fn s7_missing_complementary_chain_untouched() {
        // No loop walks {1,2}: the complementary chain is absent, so the
        // split cannot certify a repair — S6 residue, unchanged.
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(0, 1), (1, 4), (4, 0)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(out[0][0], infos[0].cycles[0]);
    }

    #[test]
    fn s7_off_band_vertex_untouched() {
        // Vertex 1 lifted 1e-9 off the segment (> TAU_WORK): outside the
        // last-ulp band — no split (a mutation widening the band fails here).
        let mut mesh = s7_mesh();
        mesh.verts[1] = p(0.374, 1.0e-9, 0.0);
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &mesh);
        assert_eq!(out[0][0], infos[0].cycles[0]);
    }

    // Spec `yang_s3_ellipse_rim_chord_bound` §7: the Stage-3 fallback bound
    // for ellipse-rim-only curved owners.
    #[test]
    fn s3_ellipse_rim_bound_is_max_major_radius_scaled() {
        // T2: mixed seg/ellipse edge list → 1e-2 · MAX major_radius (the
        // largest Stage-1 chain bound; a mutation picking min or the
        // minor_radius must fail).
        let ell = |a: f64, b: f64| BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Ellipse {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                major_axis: Vector3::new(1.0, 0.0, 0.0),
                major_radius: a,
                minor_radius: b,
            },
        };
        let seg = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg.clone(), ell(0.25, 0.2), ell(0.5, 0.1), seg];
        assert_eq!(
            ellipse_rim_chord_bound(&edges),
            Some(1e-2 * 0.5),
            "T2: the fallback is the LARGEST ellipse-chain bound"
        );
    }

    #[test]
    fn s3_ellipse_rim_bound_none_without_ellipses() {
        // T3: a seg-only owner has no fallback — the loud producer fault
        // stands (a mutation returning Some(TAU_WORK) here must fail).
        let seg = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        assert_eq!(
            ellipse_rim_chord_bound(&[seg]),
            None,
            "T3: no Circle and no Ellipse → producer fault preserved"
        );
    }

    #[test]
    fn kv15b_resolved_length_regrows_past_band_stays() {
        // B5 second half: after 1→0, segment (1,2) resolves to (0,2) at
        // 1.2e-7 ≥ TAU_MODEL — it must NOT collapse (single-sweep, no drift).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(1.2e-7, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
        );
        let mut attr = vec![None; 3];
        let map = kv15b_map(&[(0, 1), (1, 2)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 2, 4], [2, 3, 4]],
            "a segment whose RESOLVED length is ≥ TAU_MODEL stays (I2)"
        );
    }

    // Spec `yang_453_junction_protected_collapse` §3: the §4.5.3 collapse
    // victim is `p_n` on a same-curve run, but `p_r` when `p_n` is a curve
    // junction (the loop's curve changes at `p_n`).
    #[test]
    fn s453_collapse_removes_p_n_on_same_curve_run() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), circle);
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert_eq!(
            reversal_collapse_direction(&curves, &inc, 1, 2, 3),
            (2, 1),
            "same curve beyond p_n ⇒ paper default: p_n is the victim"
        );
    }

    #[test]
    fn s453_collapse_protects_junction_p_n() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let other = Curve::Circle {
            center: p(5.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), other);
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert_eq!(
            reversal_collapse_direction(&curves, &inc, 1, 2, 3),
            (1, 2),
            "curve changes at p_n ⇒ p_n is an exact curve-junction endpoint \
             and must survive; the overshooting p_r is the victim"
        );
        // Canonical-key robustness: descending vertex ids on both edges.
        let mut curves_rev: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves_rev.insert((7, 9), circle);
        curves_rev.insert((3, 7), other);
        assert_eq!(
            reversal_collapse_direction(&curves_rev, &inc, 9, 7, 3),
            (9, 7),
            "junction protection must hold under canonical (min,max) edge keys"
        );
    }

    // Spec §3c: straight-run reversal — branch table 4–7 on synthetic
    // curve + incidence maps. The seam runs along +x; vertex 1 (p_r) doubles
    // back to vertex 2 (p_n) at 0.5 (a U-turn on the run).
    #[test]
    fn s453c_line_run_reversal_branches() {
        use std::collections::BTreeMap;
        let mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.5, 0.0, 0.0),
                p(2.0, 0.0, 0.0),
            ],
            vec![],
        );
        let lo = std::f64::consts::FRAC_PI_4;
        let hi = 3.0 * std::f64::consts::FRAC_PI_4;
        let plane_a = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let plane_b = Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 0.0),
            d: 0.0,
        };
        let plane_c = Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 1.0),
            d: 0.0,
        };
        let mut curves: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
        curves.insert((0, 1), Curve::LineSegment);
        curves.insert((1, 2), Curve::LineSegment);
        curves.insert((2, 3), Curve::LineSegment);
        let pair = vec![(InputId::A, plane_a), (InputId::B, plane_b)];
        let pair_swapped = vec![(InputId::B, plane_b), (InputId::A, plane_a)];
        let pair_other = vec![(InputId::A, plane_a), (InputId::B, plane_c)];

        // Branch 7/6 precondition: same run through p_r (pair equality is
        // unordered), U-turn detected.
        let mut inc: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc.insert((0, 1), pair.clone());
        inc.insert((1, 2), pair_swapped.clone());
        inc.insert((2, 3), pair.clone());
        assert!(
            is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
            "a U-turn on ONE straight seam run (unordered-equal pairs) is a \
             §4.5.3 reversal"
        );
        // Branch 7: same pair continues past p_n → paper default victim p_n.
        assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (2, 1));
        // Branch 6: pair changes at p_n → p_n is the run junction; p_r is
        // the victim.
        inc.insert((2, 3), pair_other.clone());
        assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (1, 2));

        // Branch 4: pair changes AT p_r → corner, never tested as a reversal
        // (even though the polyline doubles back).
        let mut inc4: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc4.insert((0, 1), pair.clone());
        inc4.insert((1, 2), pair_other.clone());
        assert!(
            !is_reversed(&mesh, &curves, &inc4, 0, 1, 2, lo, hi),
            "a surface-pair change at p_r is a genuine corner, not a reversal"
        );

        // Branch 5: tangent/parallel pair (n_A × n_B ≈ 0) — cannot diagnose.
        // Use NON-doubling geometry so the U-turn arm doesn't fire first.
        let mesh5 = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0)],
            vec![],
        );
        let coincident = vec![(InputId::A, plane_a), (InputId::B, plane_a)];
        let mut inc5: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc5.insert((0, 1), coincident.clone());
        inc5.insert((1, 2), coincident.clone());
        assert!(
            !is_reversed(&mesh5, &curves, &inc5, 0, 1, 2, lo, hi),
            "a coincident-plane seam (§4.5.5) has no cross-product tangent — \
             healthy skip"
        );

        // Per-site eligibility: a run boundary (missing curve entry on one
        // side) is never a reversal site.
        let mut curves_gap: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
        curves_gap.insert((1, 2), Curve::LineSegment);
        assert!(
            !is_reversed(&mesh, &curves_gap, &inc, 0, 1, 2, lo, hi),
            "p_r with a curve-less incident edge is a run boundary, not a site"
        );
        // Run END at p_n: curve(p_r,p_n) exists, curve(p_n,p_after) doesn't —
        // p_n survives, p_r is the victim.
        assert_eq!(
            reversal_collapse_direction(&curves_gap, &inc, 1, 2, 3),
            (1, 2),
            "the run's exact endpoint (no intersection edge beyond) survives"
        );
    }

    #[test]
    fn s453c_surface_normal_at_canonical() {
        let n = surface_normal_at(
            Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 2.0),
                d: 1.0,
            },
            p(5.0, 5.0, 5.0),
        )
        .expect("plane normal");
        assert!((n[2] - 1.0).abs() < 1e-15, "plane normal unit-normalized");

        let n = surface_normal_at(
            Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
            p(2.0, 0.0, 7.0),
        )
        .expect("cylinder normal");
        assert!((n[0] - 1.0).abs() < 1e-15 && n[2].abs() < 1e-15);
        assert!(
            surface_normal_at(
                Surface::Cylinder {
                    axis_point: p(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: 2.0,
                },
                p(0.0, 0.0, 3.0),
            )
            .is_none(),
            "on-axis point has no radial direction"
        );

        let n = surface_normal_at(
            Surface::Sphere {
                center: p(1.0, 0.0, 0.0),
                radius: 5.0,
            },
            p(1.0, 3.0, 0.0),
        )
        .expect("sphere normal");
        assert!((n[1] - 1.0).abs() < 1e-15);

        // 45° cone: at a lateral point the normal is perpendicular to the
        // ruling direction and tilted 45° from the axis.
        let n = surface_normal_at(
            Surface::Cone {
                apex: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: std::f64::consts::FRAC_PI_4,
            },
            p(1.0, 0.0, 1.0),
        )
        .expect("cone normal");
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((n[0] - s).abs() < 1e-12 && (n[2] + s).abs() < 1e-12);
    }

    // Spec §3b: §4.4.1(b) merge survivor ranking — junction > conic endpoint
    // > plain vertex; equal ranks keep the lower-index rule.
    #[test]
    fn s453_merge_survivor_prefers_exact_vertex() {
        use std::collections::BTreeSet;
        let junction: BTreeSet<u32> = [15u32].into_iter().collect();
        let conic: BTreeSet<u32> = [15u32, 20u32].into_iter().collect();

        // Conic endpoint (higher index) survives over a plain vertex — the
        // R0091 configuration, in BOTH argument orders.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 8, 20),
            (8, 20)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 8),
            (8, 20)
        );

        // Junction survives over a plain single-curve conic endpoint.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 15),
            (20, 15)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 15, 20),
            (20, 15)
        );

        // Equal rank (both plain): lower index survives — byte-identical to
        // the pre-fix behavior.
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 4, 9), (9, 4));
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 9, 4), (9, 4));
    }

    #[test]
    fn n3_degenerate_tangent_is_reversal() {
        let mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.5, 0.0, 0.0)],
            vec![],
        );
        // Spec §3c per-site eligibility: p_r is a §4.5.3 site only when both
        // incident edges are intersection edges — give both a Circle entry on
        // the SAME curve (the original N3 fixture predates the site guard).
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((0, 1), circle);
        curves.insert((1, 2), circle);
        let lo = std::f64::consts::FRAC_PI_4;
        let hi = 3.0 * std::f64::consts::FRAC_PI_4;
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert!(
            is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
            "a 180° U-turn (degenerate t̃, Yang §4.5.3 collinear case) must be \
             detected as a reversal, not treated as healthy"
        );
    }

    // =====================================================================
    // M4 — demoted substitutes (test-only differential oracle).
    //
    // These were the production PR-YR3/YR4 spatial-match + majority-vote
    // attribution path. M3 replaced production attribution with real
    // LabeledArrangement labels; per roadmap rule #9 the substitutes are
    // RETAINED here as a second independent attribution method that
    // cross-checks the true-label path (the `m4_*` differential test).
    // Disagreement on a fixture localizes a label-path bug. Do NOT delete.
    // =====================================================================

    /// M4 oracle: try to match `target` against a vertex in `brep`'s mesh
    /// within `MATCH_TOLERANCE`. Returns the matched vertex's
    /// `TessellationSource` or `None`.
    fn match_against(brep: &BRep, target: Point3) -> Option<TessellationSource> {
        let tol2 = MATCH_TOLERANCE * MATCH_TOLERANCE;
        for (i, v) in brep.as_mesh().verts.iter().enumerate() {
            let dx = v.x() - target.x();
            let dy = v.y() - target.y();
            let dz = v.z() - target.z();
            if dx * dx + dy * dy + dz * dz <= tol2 {
                return Some(brep.tessellation_map().lookup(i as u32));
            }
        }
        None
    }

    /// M4 oracle: match `target` against A first, then B; track which
    /// input matched.
    fn match_with_input(
        a: &BRep,
        b: &BRep,
        target: Point3,
    ) -> (Option<InputId>, TessellationSource) {
        if let Some(src) = match_against(a, target) {
            return (Some(InputId::A), src);
        }
        if let Some(src) = match_against(b, target) {
            return (Some(InputId::B), src);
        }
        (None, TessellationSource::Intersection)
    }

    /// M4 oracle: the set of `(InputId, face_idx)` pairs that a single
    /// output vertex's provenance is compatible with.
    fn face_candidates(
        input: Option<InputId>,
        source: TessellationSource,
        a: &BRep,
        b: &BRep,
    ) -> Vec<(InputId, u32)> {
        let Some(input) = input else {
            return Vec::new();
        };
        let brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        match source {
            TessellationSource::BRepFace { face, .. } => vec![(input, face)],
            TessellationSource::BRepEdge { edge, .. } => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.outer_loop.contains(&edge))
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::BRepVertex(v) => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    f.outer_loop.iter().any(|&e| {
                        let edge = &brep.edges()[e as usize];
                        edge.start == v || edge.end == v
                    })
                })
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::Intersection | TessellationSource::Unknown => Vec::new(),
        }
    }

    /// M4 oracle: count votes per `(InputId, face)` across 3 candidate
    /// sets; return the highest-count pair reaching ≥2 votes (ties → lowest
    /// `(InputId, face)` lexicographic).
    fn majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new();
        for set in sets {
            let mut uniq: Vec<(InputId, u32)> = set.clone();
            uniq.sort();
            uniq.dedup();
            for c in uniq {
                *counts.entry(c).or_insert(0) += 1;
            }
        }
        let mut best: Option<((InputId, u32), u8)> = None;
        for (key, &count) in &counts {
            if count < 2 {
                continue;
            }
            match best {
                None => best = Some((*key, count)),
                Some((_, bc)) if count > bc => best = Some((*key, count)),
                _ => {}
            }
        }
        best.map(|((input, face), _)| TriangleAttribution { input, face })
    }

    /// M4 oracle composite: run the full demoted substitute attribution
    /// (vertex provenance → per-vertex face candidates → majority vote)
    /// over `mesh`, producing a `TriangleAttributionMap`. This is exactly
    /// what the pre-M3 production `boolean()` computed internally; the
    /// reworked PR-YR4 substitute tests and the yr5_* reconstruction tests
    /// call it directly instead of routing through production `boolean()`
    /// (whose attribution is now the real-label path).
    fn substitute_attribution(mesh: &Mesh, a: &BRep, b: &BRep) -> TriangleAttributionMap {
        let mut inputs: Vec<Option<InputId>> = Vec::with_capacity(mesh.num_verts());
        let mut sources: Vec<TessellationSource> = Vec::with_capacity(mesh.num_verts());
        for &target in &mesh.verts {
            let (inp, src) = match_with_input(a, b, target);
            inputs.push(inp);
            sources.push(src);
        }
        let mut attributions = Vec::with_capacity(mesh.num_tris());
        for tri in &mesh.tris {
            let sets = [
                face_candidates(inputs[tri[0] as usize], sources[tri[0] as usize], a, b),
                face_candidates(inputs[tri[1] as usize], sources[tri[1] as usize], a, b),
                face_candidates(inputs[tri[2] as usize], sources[tri[2] as usize], a, b),
            ];
            attributions.push(majority_vote(&sets));
        }
        TriangleAttributionMap { attributions }
    }

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// An empty (0-triangle) `LabeledArrangement` for backend-dispatch
    /// tests that only care about the Ok/err control flow, not labels.
    fn empty_arrangement() -> LabeledArrangement {
        LabeledArrangement {
            mesh: Mesh::empty(),
            surface: Vec::new(),
            inside: Vec::new(),
            patch: Vec::new(),
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    fn sample_mesh() -> Mesh {
        Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// ADVERSARY (spec §2/I1, task #86): a vertex shared by ONE closed
    /// 3-triangle fan and ONE OPEN 2-triangle fan must NOT be split. The
    /// open fan's boundary edges (each incident to a single triangle) mean
    /// the star is not a union of closed disks, so the honest-split guard
    /// (`I1`) must leave the vertex — and the whole mesh — untouched, keeping
    /// the loud downstream gates in charge. This pins the closed-fan guard:
    /// the existing corpus/canonical union oracles cannot catch a weakened
    /// guard because their real pinch meshes have only closed fans.
    #[test]
    fn split_pinch_vertices_leaves_open_fan_untouched() {
        // Vertex 0 is the shared apex. Closed fan: (0,1,2),(0,2,3),(0,3,1)
        // — every 0-incident edge is 2-valent. Open fan: (0,4,5),(0,5,6) —
        // edges (0,4) and (0,6) are 1-valent (boundary). The two fans share
        // no vertex besides 0, so they are separate star components; a
        // guardless split would wrongly cut them into per-fan copies.
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),  // 0 apex
                p(1.0, 0.0, 0.0),  // 1
                p(0.0, 1.0, 0.0),  // 2
                p(-1.0, 0.0, 0.0), // 3
                p(0.0, 0.0, 1.0),  // 4
                p(0.0, 0.0, 2.0),  // 5
                p(0.0, 0.0, 3.0),  // 6
            ],
            vec![[0, 1, 2], [0, 2, 3], [0, 3, 1], [0, 4, 5], [0, 5, 6]],
        );
        let before_verts = mesh.verts.len();
        let before_tris = mesh.tris.clone();
        let mut relocations: Vec<(u32, f64)> = Vec::new();
        let splits = split_pinch_vertices(&mut mesh, &mut relocations);
        assert_eq!(splits, 0, "open-fan vertex must not be split (I1 guard)");
        assert_eq!(
            mesh.verts.len(),
            before_verts,
            "open-fan split must not append vertices"
        );
        assert_eq!(
            mesh.tris, before_tris,
            "open-fan split must not rewrite triangle indices"
        );
    }

    /// ADVERSARY (spec §8/I4, task #86): a bowtie patch — two triangle lobes
    /// meeting at ONE mesh-manifold pinch vertex — must walk into TWO
    /// separate boundary cycles, one per lobe, NOT one chained self-crossing
    /// cycle. The pinch (vertex 3) is entered MID-walk with out-degree 2, and
    /// the wedge-correct continuation (stay in the incoming lobe) is
    /// deliberately the HIGHER-indexed outgoing edge, so lowest-first would
    /// cross into the other lobe and chain both loops into one cycle. This
    /// pins the wedge walk; the union oracles cannot catch a lowest-first
    /// regression because their post-split walks never hit a mid-walk pinch.
    #[test]
    fn patch_boundary_cycle_splits_bowtie_into_two_cycles() {
        // Lobe A = tri[3,6,0], Lobe B = tri[3,1,2], sharing pinch vertex 3.
        // Verts 4,5 are unused filler so index 6 is addressable.
        let mesh = Mesh::new(
            vec![
                p(1.0, 1.0, 0.0),  // 0
                p(-1.0, 0.0, 0.0), // 1
                p(-1.0, 1.0, 0.0), // 2
                p(0.0, 0.0, 0.0),  // 3 = pinch
                p(5.0, 5.0, 5.0),  // 4 filler
                p(6.0, 6.0, 6.0),  // 5 filler
                p(1.0, 0.0, 0.0),  // 6
            ],
            vec![[3, 6, 0], [3, 1, 2]],
        );
        let patch = Patch {
            attribution: TriangleAttribution {
                input: InputId::A,
                face: 0,
            },
            tri_indices: vec![0, 1],
        };
        let cycles =
            patch_boundary_cycle(&patch, &mesh).expect("bowtie patch boundary walk must succeed");
        assert_eq!(
            cycles.len(),
            2,
            "bowtie patch must split into 2 per-lobe cycles, not chain into \
             one; got {cycles:?}"
        );
        for c in &cycles {
            assert_eq!(c.len(), 3, "each lobe is a 3-edge triangle boundary");
        }
    }

    /// Backend whose `boolean()` always errors and which does NOT override
    /// the M3 `labeled_arrangement` trait method, so it surfaces through
    /// the default ("not supported") error. Used by
    /// `boolean_with_err_backend` to confirm `boolean()` maps a backend
    /// failure to `YangError::MeshBooleanFailed`.
    struct MockBackend;
    impl MeshBoolean for MockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            Err(Box::from("mock failure"))
        }
    }

    // ----- Group 2: yang-rs type construction -----

    #[test]
    fn surface_plane_construction() {
        let s = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -1.0,
        };
        match s {
            Surface::Plane { normal, d } => {
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(d, -1.0);
            }
            // `s` is constructed as `Plane`, so this arm is never hit; it
            // only satisfies exhaustiveness once curved variants are added.
            _ => panic!("expected Plane"),
        }
    }

    // ----- PR-YR6: curved Surface / Curve construction round-trips -----

    #[test]
    fn surface_sphere_construction() {
        let s = Surface::Sphere {
            center: p(1.0, 2.0, 3.0),
            radius: 5.0,
        };
        match s {
            Surface::Sphere { center, radius } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(radius, 5.0);
            }
            _ => panic!("expected Sphere"),
        }
    }

    #[test]
    fn surface_cylinder_construction() {
        let s = Surface::Cylinder {
            axis_point: p(1.0, 2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 4.0,
        };
        match s {
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                assert_eq!(axis_point, p(1.0, 2.0, 3.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 4.0);
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn surface_cone_construction() {
        let s = Surface::Cone {
            apex: p(0.0, 0.0, 10.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        };
        match s {
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                assert_eq!(apex, p(0.0, 0.0, 10.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, -1.0));
                assert_eq!(half_angle, 0.5);
            }
            _ => panic!("expected Cone"),
        }
    }

    #[test]
    fn curve_circle_construction() {
        let c = Curve::Circle {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.5,
        };
        match c {
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 2.5);
            }
            _ => panic!("expected Circle"),
        }
    }

    #[test]
    fn curve_ellipse_construction() {
        let c = Curve::Ellipse {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            major_radius: 6.0,
            minor_radius: 3.0,
        };
        match c {
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(major_axis, Vector3::new(1.0, 0.0, 0.0));
                assert_eq!(major_radius, 6.0);
                assert_eq!(minor_radius, 3.0);
            }
            _ => panic!("expected Ellipse"),
        }
    }

    // ----- PR-YR6: BRep::new loud-rejects curved surfaces -----

    /// Minimal well-formed single-triangle topology (3 verts, 3 edges, one
    /// face with a 3-edge outer loop). Mirrors the `brep_new_single_triangle`
    /// fixture exactly except the single face's surface is caller-supplied,
    /// so the ONLY variable across the loud-rejection tests is the surface.
    fn single_triangle_topology(
        surface: Surface,
    ) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface,
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        (verts, edges, faces)
    }

    #[test]
    fn brep_new_rejects_sphere_face() {
        // PR-YR12 migration: the sphere path is now implemented, but a sphere
        // face on a single *triangle* (no Circle meridian seam edge) lacks the
        // seam the sphere tessellation requires, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed (mirrors the cylinder
        // migration above).
        let (verts, edges, faces) = single_triangle_topology(Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (sphere on a triangle lacks its meridian \
             seam Circle edge), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cylinder_face() {
        // PR-YR7 migration: the cylinder lateral path is now implemented, but a
        // cylinder face on a single *triangle* (no Circle rim edges) lacks the
        // lateral's 2 required Circle rims, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cylinder {
            axis_point: p(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cylinder lateral on a triangle lacks its \
             2 Circle rim edges), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cone_face() {
        // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle the
        // cone tessellation path requires) is now MalformedTopology, mirroring the
        // cylinder/sphere-on-a-triangle rejection. It must STILL error loudly
        // (never silently succeed); only the error *kind* changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cone {
            apex: p(0.0, 0.0, 1.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cone lateral on a triangle lacks its \
             base-rim Circle edge), got {result:?}"
        );
    }

    #[test]
    fn curve_line_segment_construction() {
        let c = Curve::LineSegment;
        assert_eq!(c, Curve::LineSegment);
    }

    #[test]
    fn brep_topology_construction() {
        let v = BRepVertex {
            point: p(0.0, 0.0, 0.0),
        };
        let e = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        let f = BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        };
        assert_eq!(v.point, p(0.0, 0.0, 0.0));
        assert_eq!(e.start, 0);
        assert_eq!(f.outer_loop.len(), 3);
    }

    #[test]
    fn tessellation_source_round_trip() {
        let src = TessellationSource::BRepVertex(7);
        match src {
            TessellationSource::BRepVertex(i) => assert_eq!(i, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tessellation_map_empty() {
        let m = TessellationMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- Group 3: from_mesh degenerate path -----

    #[test]
    fn from_mesh_preserves_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn from_mesh_map_length_matches_verts() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.tessellation_map().len(), m.num_verts());
    }

    #[test]
    fn from_mesh_map_entries_all_unknown() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        for i in 0..b.tessellation_map().len() as u32 {
            assert_eq!(b.tessellation_map().lookup(i), TessellationSource::Unknown);
        }
    }

    // ----- Group 4: BRep::new Stage 1 happy paths -----

    fn plane_z_up() -> Surface {
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        }
    }

    #[test]
    fn brep_new_single_triangle() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 3);
        assert_eq!(b.num_tris(), 1);
        for i in 0..3u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i)
            );
        }
    }

    #[test]
    fn brep_new_quad_face() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 2); // 4-vert fan: 2 tris
    }

    #[test]
    fn brep_new_tetrahedron() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        // Edges of a tetrahedron: 6 edges between 4 vertices.
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            }, // 0
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // 1
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            }, // 2
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            }, // 3
            BRepEdge {
                start: 1,
                end: 3,
                curve: Curve::LineSegment,
            }, // 4
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            }, // 5
            // Reverse-direction edges for the loops (each tet face has 3 edges)
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // 6
            BRepEdge {
                start: 3,
                end: 1,
                curve: Curve::LineSegment,
            }, // 7
            BRepEdge {
                start: 3,
                end: 2,
                curve: Curve::LineSegment,
            }, // 8
            BRepEdge {
                start: 1,
                end: 0,
                curve: Curve::LineSegment,
            }, // 9
            BRepEdge {
                start: 2,
                end: 1,
                curve: Curve::LineSegment,
            }, // 10
            BRepEdge {
                start: 0,
                end: 2,
                curve: Curve::LineSegment,
            }, // 11
        ];
        // 4 triangular faces. Each loop is 3 edges. Note: outer_loop's
        // start vertices must form a coherent cycle for fan-triangulation
        // to produce correct tris; we use edges 0,1,2 for the "bottom"
        // (verts 0→1→2), etc.
        let faces = vec![
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // bottom (verts 0,1,2)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![9, 3, 7],
                inner_loops: Vec::new(),
                reversed: false,
            }, // back (verts 1,0,3) - using 1→0,0→3,3→1
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![10, 4, 8],
                inner_loops: Vec::new(),
                reversed: false,
            }, // right (verts 2,1,3)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![11, 5, 6],
                inner_loops: Vec::new(),
                reversed: false,
            }, // left (verts 0,2,3)
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 4);
    }

    #[test]
    fn brep_new_unit_cube() {
        // 8 verts of a unit cube at origin.
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 1.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            },
        ];
        // For PR-YR2 we don't need real edge dedup; just enumerate the
        // 24 directed edges we'll need (one per face boundary).
        // bottom face vertices: 0→3→2→1, edges 0:0→3, 1:3→2, 2:2→1, 3:1→0
        // (we just need fan_verts[0] to be the starting vertex of each
        // outer_loop)
        let edges: Vec<BRepEdge> = vec![
            // bottom face: 0, 3, 2, 1
            (0, 3),
            (3, 2),
            (2, 1),
            (1, 0),
            // top face: 4, 5, 6, 7
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            // south face: 0, 1, 5, 4
            (0, 1),
            (1, 5),
            (5, 4),
            (4, 0),
            // north face: 3, 7, 6, 2
            (3, 7),
            (7, 6),
            (6, 2),
            (2, 3),
            // east face: 1, 2, 6, 5
            (1, 2),
            (2, 6),
            (6, 5),
            (5, 1),
            // west face: 0, 4, 7, 3
            (0, 4),
            (4, 7),
            (7, 3),
            (3, 0),
        ]
        .into_iter()
        .map(|(s, e)| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        })
        .collect();
        let plane = plane_z_up();
        let faces = vec![
            BRepFace {
                surface: plane,
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![8, 9, 10, 11],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![12, 13, 14, 15],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![16, 17, 18, 19],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![20, 21, 22, 23],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 8);
        assert_eq!(b.num_tris(), 12); // 6 quads × 2 tris each
    }

    #[test]
    fn brep_new_bijection_is_one_to_one() {
        // Build a tetrahedron and confirm every mesh vertex i maps to
        // TessellationSource::BRepVertex(i).
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        for i in 0..b.num_verts() as u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i),
                "vertex {i} should map to BRepVertex({i})"
            );
        }
    }

    // ----- Group 5: Error paths -----

    #[test]
    fn brep_new_face_with_too_few_edges_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
        ];
        let edges = vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        }];
        // 1-edge face — degenerate
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    #[test]
    fn brep_new_out_of_range_edge_index_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // Face references edge 99 — out of range
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 99],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    // ----- PR-YR1 backward-compat: existing boolean dispatch tests -----

    #[test]
    fn brep_from_mesh_as_mesh_round_trip() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn brep_into_mesh_returns_wrapped() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.into_mesh(), m);
    }

    #[test]
    fn brep_counts_delegate_to_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.num_verts(), m.num_verts());
        assert_eq!(b.num_tris(), m.num_tris());
    }

    #[test]
    fn yang_error_display_non_empty() {
        for e in [
            YangError::NonManifoldInput,
            YangError::NonManifoldOutput,
            YangError::MeshBooleanFailed(Box::from("test")),
            YangError::MalformedTopology("test".to_string()),
        ] {
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "empty Display for {e:?}");
        }
    }

    #[test]
    fn yang_error_source_propagates() {
        let inner: Box<dyn Error + Send + Sync> = Box::from("inner");
        let e = YangError::MeshBooleanFailed(inner);
        let src = e.source().expect("source should be Some");
        assert_eq!(src.to_string(), "inner");
    }

    #[test]
    fn boolean_with_ok_backend() {
        // M3: boolean() consumes a LabeledArrangement. An empty arrangement
        // (0 tris) keeps nothing → empty output BRep, Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let backend = LabelMockBackend::new(empty_arrangement());
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(r.num_verts(), 0);
    }

    #[test]
    fn boolean_with_err_backend() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend;
        match boolean(&a, &b, BoolOp::Union, &mock) {
            Err(YangError::MeshBooleanFailed(_)) => {}
            other => panic!("expected MeshBooleanFailed, got {:?}", other),
        }
    }

    #[test]
    fn boolean_dispatches_all_four_ops() {
        // M3: an empty arrangement is keep-set-empty for every op → Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        for op in [
            BoolOp::Union,
            BoolOp::Intersect,
            BoolOp::Subtract,
            BoolOp::Xor,
        ] {
            let backend = LabelMockBackend::new(empty_arrangement());
            assert!(boolean(&a, &b, op, &backend).is_ok(), "op {op:?}");
        }
    }

    // ----- PR-YR3: Group 1 — TessellationSource::Intersection variant -----

    #[test]
    fn intersection_variant_constructs_and_matches() {
        let s = TessellationSource::Intersection;
        match s {
            TessellationSource::Intersection => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn intersection_distinct_from_unknown() {
        assert_ne!(
            TessellationSource::Intersection,
            TessellationSource::Unknown
        );
    }

    // ----- PR-YR3: Group 2 — MATCH_TOLERANCE constant -----

    #[test]
    fn match_tolerance_is_1e_minus_9() {
        assert_eq!(MATCH_TOLERANCE, 1e-9);
    }

    // ----- PR-YR3: Group 3 — Spatial matching via mock backend -----

    /// Build a BRep with explicit topology (triangle) so its mesh has
    /// non-trivial TessellationMap entries (`BRepVertex(i)` for each i).
    fn triangle_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR3 spatial-vertex-provenance was REMOVED from production by M3
    // (production tessellation_map is now BRepVertex(i) 1:1 with the kept
    // sub-mesh). Per Manager policy (a), these tests are reworked to call
    // the now-#[cfg(test)] substitute helper `match_with_input` DIRECTLY,
    // preserving the substitute's coverage as the M4 oracle rather than
    // routing through production `boolean()`.

    #[test]
    fn boolean_input_a_verbatim_copies_a_map() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Each of A's mesh verts matches input A's BRepVertex(i).
        for (i, &target) in a.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::A), "vert {i} should match A");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i}"
            );
        }
    }

    #[test]
    fn boolean_input_b_verbatim_copies_b_map() {
        let a = triangle_brep();
        // B has different vertices so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 10.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        for (i, &target) in b.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::B), "vert {i} should match B");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i} — should match input B's BRepVertex({i})"
            );
        }
    }

    #[test]
    fn boolean_all_new_coords_are_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Coords far from both inputs → no match → Intersection.
        for target in [
            p(100.0, 100.0, 100.0),
            p(101.0, 100.0, 100.0),
            p(100.0, 101.0, 100.0),
        ] {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, None);
            assert_eq!(
                src,
                TessellationSource::Intersection,
                "novel coord should be Intersection"
            );
        }
    }

    #[test]
    fn boolean_mixed_match_and_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // 2 verts from A + 2 new coords.
        let expectations = [
            (p(0.0, 0.0, 0.0), TessellationSource::BRepVertex(0)),
            (p(1.0, 0.0, 0.0), TessellationSource::BRepVertex(1)),
            (p(99.0, 99.0, 0.0), TessellationSource::Intersection),
            (p(98.0, 98.0, 0.0), TessellationSource::Intersection),
        ];
        for (i, (target, expect)) in expectations.into_iter().enumerate() {
            let (_input, src) = match_with_input(&a, &b, target);
            assert_eq!(src, expect, "vertex {i}");
        }
    }

    // ----- PR-YR4: Group 1 — types -----

    #[test]
    fn input_id_ordering_and_derives() {
        assert!(InputId::A < InputId::B);
        assert_eq!(InputId::A, InputId::A);
        assert_ne!(InputId::A, InputId::B);
        assert_eq!(format!("{:?}", InputId::A), "A");
        assert_eq!(format!("{:?}", InputId::B), "B");
        // Copy
        let x = InputId::A;
        let y = x;
        assert_eq!(x, y);
    }

    #[test]
    fn triangle_attribution_construct_and_equality() {
        let t1 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t2 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t3 = TriangleAttribution {
            input: InputId::B,
            face: 7,
        };
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        // Copy + accessors
        let t4 = t1;
        assert_eq!(t4.input, InputId::A);
        assert_eq!(t4.face, 7);
    }

    #[test]
    fn triangle_attribution_map_empty_and_len() {
        let m = TriangleAttributionMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- PR-YR4: Group 2 — algorithm via mock backend -----

    /// Two-face B-Rep where V0 is shared by F0 and F1; V1, V2 only in F0;
    /// V3, V4 only in F1. Used by tie-break + pure-input tests.
    fn two_face_shared_vertex_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            }, // 0 — shared (F0 & F1)
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            }, // 1 — F0 only
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            }, // 2 — F0 only (moved off x-axis: was (2,0,0)) so F0 is a real triangle in z=0
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            }, // 3 — F1 only
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            }, // 4 — F1 only (moved off y-axis: was (0,2,0)) so F1 is a real triangle in x=0
        ];
        // F0 edges (triangle V0-V1-V2):
        // E0 V0→V1, E1 V1→V2, E2 V2→V0
        // F1 edges (triangle V0-V3-V4):
        // E3 V0→V3, E4 V3→V4, E5 V4→V0
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 4,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 4,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // F0 lies in z=0 (normal +z); F1 now lies in x=0 (normal +x).
        let f0_plane = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let f1_plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let faces = vec![
            BRepFace {
                surface: f0_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F0
            BRepFace {
                surface: f1_plane,
                outer_loop: vec![3, 4, 5],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F1
        ];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR4 majority-vote ATTRIBUTION was REMOVED from production by M3
    // (production attributes via real LabeledArrangement labels + geometric
    // face resolution). Per Manager policy (a), these tests are reworked to
    // exercise the now-#[cfg(test)] substitute via `substitute_attribution`
    // DIRECTLY (not via production `boolean()`), preserving the substitute's
    // coverage as the M4 differential oracle.

    #[test]
    fn boolean_pure_a_attributes_to_a_faces() {
        // Pure-A: substitute over A's mesh. Each tri's verts are
        // BRepVertex(i) of A → per-vertex face incidence → majority vote
        // attributes each tri to its source face.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(attr.len(), 2);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "output tri 0 (F0 fan tri) should attribute to A's F0"
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            }),
            "output tri 1 (F1 fan tri) should attribute to A's F1"
        );
    }

    #[test]
    fn boolean_pure_b_attributes_to_b_faces() {
        let a = two_face_shared_vertex_brep();
        // B is the same B-Rep, shifted so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let attr = substitute_attribution(b.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 1
            })
        );
    }

    #[test]
    fn boolean_all_new_coords_attribute_to_none() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A mesh with coords far from both inputs.
        let novel = Mesh::new(
            vec![
                p(1000.0, 1000.0, 1000.0),
                p(1001.0, 1000.0, 1000.0),
                p(1000.0, 1001.0, 1000.0),
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&novel, &a, &b);
        assert_eq!(attr.len(), 1);
        assert_eq!(
            attr.lookup(0),
            None,
            "all-new triangle should have None attribution"
        );
    }

    #[test]
    fn boolean_mixed_majority_wins() {
        // 2 verts match A's F0 + 1 novel → F0 attribution.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),       // matches a.verts[1] (F0 only)
                p(1.0, 1.0, 0.0),       // matches a.verts[2] (F0 only)
                p(1000.0, 0.0, 1000.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "2 A-F0-verts + 1 novel → majority F0"
        );
    }

    #[test]
    fn boolean_no_majority_returns_none() {
        // 1 A-vert + 1 B-vert + 1 novel → no majority, None.
        let a = two_face_shared_vertex_brep();
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),     // matches a.verts[1] (A, F0)
                p(101.0, 0.0, 0.0),   // matches b.verts[1] (B, F0)
                p(500.0, 500.0, 0.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            None,
            "1 A + 1 B + 1 novel → no 2-of-3 majority"
        );
    }

    #[test]
    fn boolean_tie_break_picks_lowest_face() {
        // Triangle (V0 shared, V1 F0-only, V3 F1-only) → candidates
        // {F0,F1}, {F0}, {F1}. Counts: F0=2, F1=2. Tie. Lowest face → F0.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let tie_mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // V0 — shared
                p(1.0, 0.0, 0.0), // V1 — F0 only
                p(0.0, 1.0, 0.0), // V3 — F1 only
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&tie_mesh, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "tie at count 2 between F0 and F1 → lowest face (F0)"
        );
    }

    // ----- PR-YR4: Group 3 — empty-topology degradation (substitute) -----

    #[test]
    fn boolean_both_inputs_from_mesh_all_none() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(&sample_mesh(), &a, &b);
        assert_eq!(attr.len(), sample_mesh().num_tris());
        assert_eq!(
            attr.lookup(0),
            None,
            "from_mesh inputs have all-Unknown sources → all-None attribution"
        );
    }

    #[test]
    fn boolean_mixed_from_mesh_and_topologized() {
        // a has topology, b is from_mesh. Substitute over a's mesh.
        // Attribution should reflect a's per-tri face ownership.
        let a = two_face_shared_vertex_brep();
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            })
        );
    }

    // ----- PR-YR5: topology reconstruction -----
    //
    // `reconstruct_topology` is UNCHANGED production. Per Manager policy
    // (b), these tests previously routed through `boolean()` via the
    // boolean-only MockBackend (which M3 no longer drives); they are
    // reworked to build a `TriangleAttributionMap` via the #[cfg(test)]
    // substitute and call `reconstruct_topology` DIRECTLY — exercising the
    // same durable reconstruction logic without the removed substitute
    // production path.

    #[test]
    fn yr5_single_triangle_round_trip_produces_one_face() {
        // Pure-A on triangle_brep (1 face, 1 fan tri) → 1 face with 3
        // boundary edges + 3 vertices forming a closed cycle.
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1, "expected 1 BRepFace");
        assert_eq!(faces[0].outer_loop.len(), 3, "expected 3-edge loop");
        assert_eq!(edges.len(), 3, "expected 3 BRepEdges");
        assert_eq!(verts.len(), 3, "expected 3 BRepVertices");
        // Cycle closure
        let f = &faces[0];
        for i in 0..3 {
            let e_curr = &edges[f.outer_loop[i] as usize];
            let e_next = &edges[f.outer_loop[(i + 1) % 3] as usize];
            assert_eq!(
                e_curr.end, e_next.start,
                "cycle break at edge {i}: {} != {}",
                e_curr.end, e_next.start
            );
        }
    }

    #[test]
    fn yr5_two_face_round_trip_produces_two_faces() {
        // two_face_shared_vertex_brep has 2 triangular faces sharing only
        // V0; 2 output tris with different attributions (F0 vs F1) → 2
        // BRepFaces.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 2, "expected 2 BRepFaces");
        for f in &faces {
            assert_eq!(f.outer_loop.len(), 3);
        }
    }

    #[test]
    fn yr5_disconnected_components_become_separate_faces() {
        // Two tris with the SAME attribution but NO shared vertex →
        // flood-fill leaves them as 2 patches → 2 faces. Regression guard
        // vs. naive attribution-bucketing.
        let a = triangle_brep();
        let b = triangle_brep();
        // 6 vertices = TWO copies of A's 3 verts at distinct indices.
        let dup = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(0.0, 0.0, 0.0), // duplicate matching A.V0 (different idx)
                p(1.0, 0.0, 0.0), // duplicate matching A.V1
                p(0.0, 1.0, 0.0), // duplicate matching A.V2
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&dup, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&dup, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            2,
            "disconnected same-attribution tris should be separate faces"
        );
    }

    #[test]
    fn yr5_none_attributed_tris_omitted_from_faces() {
        // tri 0 matches A's verts (Some(A, F0)); tri 1 is all novel coords
        // (None). reconstruct_topology should yield 1 face.
        let a = triangle_brep();
        let b = triangle_brep();
        let mixed = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(1000.0, 0.0, 0.0),
                p(1001.0, 0.0, 0.0),
                p(1000.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mixed, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            1,
            "None-attributed tris should not contribute faces"
        );
    }

    #[test]
    fn yr5_vertex_count_matches_mesh() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, _e, _f) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(verts.len(), mesh.num_verts());
        for (i, v) in verts.iter().enumerate() {
            assert_eq!(v.point, mesh.verts[i]);
        }
    }

    #[test]
    fn yr5_surface_inherited_from_input() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(
            faces[0].surface,
            a.faces()[0].surface,
            "output face should inherit input A's surface"
        );
    }

    #[test]
    fn yr5_empty_input_produces_empty_face_set() {
        // Both inputs from_mesh → all-None attribution → no faces/edges.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mesh = sample_mesh();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert!(
            faces.is_empty(),
            "all-None attribution should yield empty faces"
        );
        assert!(
            edges.is_empty(),
            "all-None attribution should yield empty edges"
        );
        // Vertices still populated 1:1 with mesh.
        assert_eq!(verts.len(), mesh.num_verts());
    }

    // ----- Stage-6 degenerate-sliver topology (spec yang_stage6_sliver_topology) -----
    //
    // Reproduces §2's measured structure at the unit level: a shared collinear
    // solid-edge chain a–c–d–b where two abutting faces subdivide it
    // DIFFERENTLY, and the arrangement keeps ZERO-AREA shim slivers along the
    // chord to stay watertight. One sliver is wound so its directed chord edge
    // DUPLICATES the real triangle's chord edge (sign-of-zero winding is
    // arbitrary) — the measured fold. Today `reconstruct_topology` dead-ends in
    // `patch_boundary_cycle` at `NonManifoldOutput`; the Stage-6 design (spec §4:
    // exclude degenerate tris from boundary derivation + loop T-subdivision) must
    // reassemble a 2-manifold output whose shared segments are each 2-covered.

    /// The shared solid edge is the y-axis (x=0, z=0): the intersection of the
    /// two abutting faces' planes z=0 (face 0, apex off +y in z=0) and x=0
    /// (face 1, apex off +y in x=0). Chain vertices a<c<d<b sit on the y-axis,
    /// exactly collinear, so every sliver along it is exactly zero-area.
    ///
    /// Vertex indices: 0=a 1=b 2=c 3=d 4=x1(face-0 apex) 5=x2(face-1 apex).
    fn sliver_fixture_mesh() -> Mesh {
        Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a  (chain end)
                p(0.0, 3.0, 0.0), // 1 = b  (chain end)
                p(0.0, 1.0, 0.0), // 2 = c  (between a,b)
                p(0.0, 2.0, 0.0), // 3 = d  (between a,b)
                p(1.0, 1.5, 0.0), // 4 = x1 (face 0 apex, z=0 plane)
                p(0.0, 1.5, 1.0), // 5 = x2 (face 1 apex, x=0 plane)
            ],
            vec![
                // face 0 (z=0 plane, normal +z): ONE real triangle carrying the
                // whole chord b→a, plus two zero-area shim slivers wound so each
                // DUPLICATES the real directed chord edge b→a (1→0).
                [0, 4, 1], // T1 real: edges a→x1, x1→b, b→a
                [1, 0, 2], // S1 sliver: edges b→a (dup!), a→c, c→b
                [1, 0, 3], // S2 sliver: edges b→a (dup!), a→d, d→b
                // face 1 (x=0 plane, normal +x): the OTHER side subdivides the
                // chain a→c→d→b (opposite direction) via a fan from x2.
                [0, 2, 5], // edges a→c, c→x2, x2→a
                [2, 3, 5], // edges c→d, d→x2, x2→c
                [3, 1, 5], // edges d→b, b→x2, x2→d
            ],
        )
    }

    /// Attribution for `sliver_fixture_mesh`: face-0 patch = {T1,S1,S2},
    /// face-1 patch = {the three fan tris}. Built directly (in-module access to
    /// the private field) so the slivers land in face 0's patch deterministically
    /// — this is the measured N4-provenance placement (§2.3), not a geometric
    /// guess.
    fn sliver_fixture_attr() -> TriangleAttributionMap {
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let f1 = Some(TriangleAttribution {
            input: InputId::A,
            face: 1,
        });
        TriangleAttributionMap {
            attributions: vec![f0, f0, f0, f1, f1, f1],
        }
    }

    /// Canonical undirected key.
    fn und(x: u32, y: u32) -> (u32, u32) {
        if x < y {
            (x, y)
        } else {
            (y, x)
        }
    }

    /// Multiset of undirected loop edges across ALL output faces, derived from
    /// each face's `outer_loop` (edge indices) via the returned edge table.
    fn loop_edge_counts(
        edges: &[BRepEdge],
        faces: &[BRepFace],
    ) -> std::collections::BTreeMap<(u32, u32), u32> {
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for f in faces {
            for &ei in &f.outer_loop {
                let e = &edges[ei as usize];
                *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
            }
            for hole in &f.inner_loops {
                for &ei in hole {
                    let e = &edges[ei as usize];
                    *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    /// TARGET (spec §5 S2/S4). RED today: `reconstruct_topology` dead-ends at
    /// `NonManifoldOutput` because sliver S1's directed edge b→a duplicates
    /// real T1's b→a, unbalancing face 0's boundary walk. GREEN: slivers are
    /// excluded from boundary derivation (A) and face 0's chord is T-subdivided
    /// at c,d (B) so every shared segment is 2-covered.
    #[test]
    fn stage6_sliver_fold_reassembles_with_subdivided_chord() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = sliver_fixture_mesh();
        let attr = sliver_fixture_attr();

        let (_verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).expect(
            "Stage-6 sliver RED: reconstruction must succeed once zero-area slivers are \
             excluded from boundary derivation (spec §4A) — today it dead-ends at \
             NonManifoldOutput on the duplicated chord edge b→a",
        );

        // S2: both real faces survive (slivers carry no boundary of their own).
        assert_eq!(
            faces.len(),
            2,
            "expected 2 output faces (chord side + chain side)"
        );

        let counts = loop_edge_counts(&edges, &faces);

        // S4: the full chord (a,b) must NOT remain a raw loop edge — it is
        // T-subdivided at c,d.
        assert_eq!(
            counts.get(&und(0, 1)).copied().unwrap_or(0),
            0,
            "chord (a,b) must be subdivided at c,d, not carried as a single loop edge; \
             loop edges: {counts:?}"
        );
        // S4: every shared segment of the solid edge is used by exactly two
        // directed loop edges (2-manifold seam).
        for (name, key) in [("a–c", und(0, 2)), ("c–d", und(2, 3)), ("d–b", und(3, 1))] {
            assert_eq!(
                counts.get(&key).copied().unwrap_or(0),
                2,
                "shared segment {name} must be 2-covered across output loops; \
                 loop edges: {counts:?}"
            );
        }
    }

    /// S5 (spec §5): a patch made ENTIRELY of zero-area slivers cannot bound a
    /// face — it must stay loudly `NonManifoldOutput`, never silently emit a
    /// degenerate face. Passes today (the fold errors) and must remain Err
    /// through the fix (excluding all its triangles leaves no boundary).
    #[test]
    fn stage6_all_degenerate_patch_stays_loud() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A single patch of ONLY collinear slivers on the y-axis (no real tri).
        let mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a
                p(0.0, 3.0, 0.0), // 1 = b
                p(0.0, 1.0, 0.0), // 2 = c
                p(0.0, 2.0, 0.0), // 3 = d
            ],
            vec![[1, 0, 2], [1, 0, 3]], // two zero-area slivers sharing (a,b)
        );
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let attr = TriangleAttributionMap {
            attributions: vec![f0, f0],
        };
        assert!(
            reconstruct_topology(&mesh, &attr, &a, &b).is_err(),
            "an all-degenerate patch must stay loud (NonManifoldOutput) — it cannot bound a face"
        );
    }

    // ====================================================================
    // M3 — functional boolean via LabeledArrangement (Group A unit tests)
    //
    // These tests target the M3 rewire: boolean() must consume a real
    // `LabeledArrangement` from `backend.labeled_arrangement(..)`, select
    // result triangles via `keep_set(op)`, geometrically resolve each kept
    // triangle's source face (centroid-in-plane), and produce a FULL
    // attribution (every output triangle → Some). Spec:
    // specs/yang_m3_functional_boolean.md (I7 unique-face, F1/F2/F3).
    //
    // RED expectations until the Implementer lands M3:
    //   - `MeshBoolean::labeled_arrangement` trait method does not exist.
    //   - `YangError::FaceResolutionFailed { tri }` variant does not exist.
    //   - `LabeledArrangement` is not imported here yet.
    //   - current boolean() ignores labels → no full coverage.
    // ====================================================================

    use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};

    /// Mock backend that returns a hand-built `LabeledArrangement` from
    /// the (M3) `labeled_arrangement` trait method. `boolean()` is still
    /// required (object-safe trait) but is unused on the M3 path.
    struct LabelMockBackend {
        arrangement: LabeledArrangement,
    }
    impl LabelMockBackend {
        fn new(arrangement: LabeledArrangement) -> Self {
            Self { arrangement }
        }
    }
    impl MeshBoolean for LabelMockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            // Not exercised on the M3 path; return the arrangement mesh so
            // a stray call is at least well-formed.
            Ok(self.arrangement.mesh.clone())
        }
        // M3: the trait gains this method (default impl errors NotSupported);
        // this mock overrides it with a hand-built arrangement.
        fn labeled_arrangement(
            &self,
            _a: &Mesh,
            _b: &Mesh,
        ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
            Ok(self.arrangement.clone())
        }
    }

    /// Axis-aligned unit cube BRep at `origin` with correct OUTWARD face
    /// normals — minimal topology sufficient for geometric face
    /// resolution (centroid-in-plane). 8 verts, 24 edges, 6 quad faces.
    fn cube_brep(origin: [f64; 3]) -> BRep {
        let [x, y, z] = origin;
        let verts = vec![
            BRepVertex { point: p(x, y, z) },
            BRepVertex {
                point: p(x + 1.0, y, z),
            },
            BRepVertex {
                point: p(x + 1.0, y + 1.0, z),
            },
            BRepVertex {
                point: p(x, y + 1.0, z),
            },
            BRepVertex {
                point: p(x, y, z + 1.0),
            },
            BRepVertex {
                point: p(x + 1.0, y, z + 1.0),
            },
            BRepVertex {
                point: p(x + 1.0, y + 1.0, z + 1.0),
            },
            BRepVertex {
                point: p(x, y + 1.0, z + 1.0),
            },
        ];
        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3], // bottom (z)
            [4, 7, 6, 5], // top (z+1)
            [0, 4, 5, 1], // front (y)
            [1, 5, 6, 2], // right (x+1)
            [2, 6, 7, 3], // back (y+1)
            [3, 7, 4, 0], // left (x)
        ];
        let mut edges = Vec::new();
        let mut loops = Vec::new();
        for vs in &face_verts {
            let base = edges.len() as u32;
            for i in 0..4 {
                edges.push(BRepEdge {
                    start: vs[i],
                    end: vs[(i + 1) % 4],
                    curve: Curve::LineSegment,
                });
            }
            loops.push(vec![base, base + 1, base + 2, base + 3]);
        }
        let normals = [
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ];
        // Plane convention n·x + d = 0. For a face on plane n·x = c the
        // offset is d = -c — WITH n the face's OUTWARD normal, so the three
        // negative-axis faces have c = -coord (e.g. bottom: n=(0,0,-1),
        // n·p = -z ⇒ d = z). The pre-2026-07-03 array had the sign flipped
        // on every face with a non-zero plane coordinate; it went unnoticed
        // because the historical bottom-quad arrangement only ever resolved
        // attribution against the origin cube's BOTTOM face (d = 0 either
        // way). The closed-shell fixture (rule-4 gate cycle) exercises all
        // six planes and unmasked it.
        let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
        let faces: Vec<BRepFace> = (0..6)
            .map(|i| BRepFace {
                surface: Surface::Plane {
                    normal: normals[i],
                    d: offs[i],
                },
                outer_loop: loops[i].clone(),
                inner_loops: Vec::new(),
                reversed: false,
            })
            .collect();
        BRep::new(verts, edges, faces).unwrap()
    }

    // N4 (1b): `BRep::new` must populate the per-triangle → owning-face map
    // (`tri_face`) 1:1 with the Stage-1 mesh triangles, with valid face indices
    // and every face owning ≥1 triangle. This is the provenance substrate that
    // lets `boolean()` attribute kept triangles to faces directly from cherchi's
    // `source` instead of geometric proximity. (The end-to-end correctness of
    // provenance attribution is covered by the full boolean suite / box fuzz,
    // which now runs provenance as the PRIMARY path.)
    #[test]
    fn brep_new_populates_tri_face_provenance() {
        let cube = cube_brep([0.0, 0.0, 0.0]);
        let tf = cube.tri_face();
        assert_eq!(
            tf.len(),
            cube.as_mesh().tris.len(),
            "tri_face must be 1:1 with the Stage-1 mesh triangles"
        );
        let nf = cube.faces().len() as u32;
        assert_eq!(nf, 6, "cube has 6 faces");
        let mut owned = vec![false; nf as usize];
        for (t, &f) in tf.iter().enumerate() {
            assert!(f < nf, "tri {t} → face {f} out of range (faces = {nf})");
            owned[f as usize] = true;
        }
        assert!(
            owned.iter().all(|&o| o),
            "every cube face must own ≥1 Stage-1 triangle"
        );

        // `from_mesh` has no Stage-1 face lineage → empty tri_face (→ geometric
        // fallback in attribution).
        let degenerate = BRep::from_mesh(cube.as_mesh().clone());
        assert!(
            degenerate.tri_face().is_empty(),
            "from_mesh BRep carries no provenance map"
        );
    }

    /// Centroid of a triangle.
    fn centroid(mesh: &Mesh, tri: [u32; 3]) -> Point3 {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        Point3::new(
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        )
    }

    /// Find the single face of `brep` whose plane contains `c` within
    /// TAU_WORK; panics if zero or >1 (the expected-attribution helper
    /// must be unambiguous for a well-posed fixture).
    fn resolve_face(brep: &BRep, c: Point3) -> u32 {
        let mut hit: Option<u32> = None;
        for (i, f) in brep.faces().iter().enumerate() {
            let Surface::Plane { normal, d } = f.surface else {
                continue;
            };
            let n = normal.as_array();
            let cc = c.as_array();
            let dist = (n[0] * cc[0] + n[1] * cc[1] + n[2] * cc[2] + d).abs();
            if dist < cad_primitives::TAU_WORK {
                assert!(hit.is_none(), "ambiguous: centroid on >1 face plane");
                hit = Some(i as u32);
            }
        }
        hit.expect("centroid lies on no face plane")
    }

    // ----- Group A.1: full attribution coverage + correctness -----

    /// Hand-built arrangement: cube A's full closed surface shell. The verts
    /// are A's exact 8 `BRepVertex` corners, so:
    /// - real-label path: each tri's centroid lies strictly inside exactly
    ///   one A face plane → I7 unique-face → full Some(A, face) attribution;
    /// - every patch boundary closes (per-face manifold cycles) and the
    ///   whole shell is watertight, matching the closed kept mesh a real
    ///   boolean produces;
    /// - the verts coincide with A's `BRepVertex`es, so the M4 substitute's
    ///   spatial matching also resolves each tri to its cube face
    ///   (vertex-face incidence majority), letting the differential oracle
    ///   agree.
    ///
    /// All `inside` all-false ⇒ all 12 tris kept by Union.
    fn arrangement_a_cube_shell() -> LabeledArrangement {
        // The full unit-cube SURFACE of `cube_brep([0,0,0])`: 12 outward-wound
        // tris, 2 per face. Historically this fixture was A's bottom quad only
        // (an open 2-tri sheet) — a mock shape no real boolean produces. The
        // 2026-07-03 gate cycle (spec `yang_kept_mesh_manifold_gate`, aborted
        // per P10 — see its §2b) closed it to model a real kept mesh; the
        // closed form is kept: it is strictly more faithful and it unmasked
        // the `cube_brep` plane-offset sign bug below. All consuming
        // assertions are computed FROM the fixture (keep-set count, geometric
        // face resolve, majority vote), so their intent is unchanged.
        let verts = vec![
            p(0.0, 0.0, 0.0), // 0
            p(1.0, 0.0, 0.0), // 1
            p(1.0, 1.0, 0.0), // 2
            p(0.0, 1.0, 0.0), // 3
            p(0.0, 0.0, 1.0), // 4
            p(1.0, 0.0, 1.0), // 5
            p(1.0, 1.0, 1.0), // 6
            p(0.0, 1.0, 1.0), // 7
        ];
        // Outward winding per face (−z, +z, −y, +y, −x, +x); every directed
        // edge pairs with its reverse ⇒ watertight 2-manifold (χ = 2).
        let tris = vec![
            [0u32, 3, 2],
            [0, 2, 1], // bottom z=0
            [4, 5, 6],
            [4, 6, 7], // top z=1
            [0, 1, 5],
            [0, 5, 4], // front y=0
            [2, 3, 7],
            [2, 7, 6], // back y=1
            [0, 4, 7],
            [0, 7, 3], // left x=0
            [1, 2, 6],
            [1, 6, 5], // right x=1
        ];
        let mesh = Mesh::new(verts, tris);
        // All on A's surface (solid 0), none on B; inside all-false ⇒ Union keeps.
        let surface = vec![vec![LaInputId(0)]; 12];
        let inside = vec![vec![false, false]; 12];
        let patch = vec![0u32, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5];
        LabeledArrangement {
            mesh,
            surface,
            inside,
            patch,
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    #[test]
    fn m3_union_full_attribution_coverage() {
        // I7 + full-coverage: every kept output triangle resolves to Some.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();

        let attr = r.triangle_attribution();
        assert_eq!(
            attr.len(),
            r.num_tris(),
            "attribution length must equal output triangle count"
        );
        assert!(r.num_tris() > 0, "expected non-empty kept sub-mesh");
        for t in 0..attr.len() as u32 {
            assert!(
                attr.lookup(t).is_some(),
                "M3 requires FULL attribution: tri {t} is None (skeleton, not closed)"
            );
        }
    }

    #[test]
    fn m3_union_attribution_matches_geometric_face() {
        // F1: each kept tri attributes to the unique A-face plane its
        // centroid lies on (one of the cube shell's six faces).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // The kept sub-mesh re-indexes verts but preserves triangle geometry.
        // For each output triangle, its centroid must lie on A's face that
        // the attribution names.
        for t in 0..r.num_tris() as u32 {
            let got = attr.lookup(t).expect("full coverage");
            assert_eq!(got.input, InputId::A, "tris are all on solid A's surface");
            let c = centroid(r.as_mesh(), r.as_mesh().tris[t as usize]);
            let expected_face = resolve_face(&a, c);
            assert_eq!(
                got.face, expected_face,
                "tri {t}: attributed face {} != geometric face {}",
                got.face, expected_face
            );
        }
        let _ = mesh; // keep capture explicit
    }

    #[test]
    fn m3_kept_submesh_is_keep_set_count() {
        // Stage 4: the kept sub-mesh must contain exactly keep_set(op) tris.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let expected_kept = la.keep_set(BoolOp::Union).len();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(
            r.num_tris(),
            expected_kept,
            "output mesh tri count must equal keep_set(Union) count"
        );
    }

    // ----- Group A.2: F2 / F3 error cases (P9: loud, never None) -----

    #[test]
    fn m3_coplanar_surface_len_two_errors_f2() {
        // F2: a kept tri whose surface label names BOTH solids (coplanar
        // overlap, len==2) → FaceResolutionFailed (out of scope, M8).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B must NOT be input-coplanar with A (the gate fires
        // first, before the backend); the F2 condition under test is the
        // ARRANGEMENT-level multi-solid surface label, which the mock
        // fabricates below regardless of the input geometry.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.0, 0.0, 0.0), p(0.5, 0.0, 0.0), p(0.0, 0.5, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            // surface names BOTH A and B (coplanar multi-solid) — F2.
            surface: vec![vec![LaInputId(0), LaInputId(1)]],
            inside: vec![vec![false, false]], // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F2 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F2), got {other:?}"),
        }
    }

    #[test]
    fn m3_centroid_off_all_planes_errors_f3() {
        // F3: a kept tri on solid A's surface whose centroid lies on NO
        // A-face plane → FaceResolutionFailed (loud, never None).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        // Triangle floating at z=0.5 (interior; off every cube face plane).
        let verts = vec![p(0.25, 0.25, 0.5), p(0.5, 0.25, 0.5), p(0.25, 0.5, 0.5)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F3 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F3), got {other:?}"),
        }
    }

    /// N4 retirement (task #53, spec `specs/n4_retire_stage6_fallback.md`):
    /// on a provenance-CARRYING arrangement, a triangle whose provenance
    /// MISSES must fail loudly — never a silent geometric guess. The
    /// triangle lies ON A's bottom face plane, so the old geometric
    /// fallback would happily (mis)attribute it; the miss is a
    /// `NoSourceEntry` (its source names only input B while the surface
    /// label says A).
    #[test]
    fn n4_provenance_miss_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface…
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            // …but provenance names only input B: a NoSourceEntry miss.
            source: vec![vec![(LaInputId(1), 0)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    /// N4 retirement: the `NoMap` miss reason (parent index beyond the
    /// input's `tri_face` map) is equally loud.
    #[test]
    fn n4_provenance_out_of_range_parent_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]],
            inside: vec![vec![false, false]],
            patch: vec![0],
            // Parent index far beyond A's 12-triangle Stage-1 map: NoMap.
            source: vec![vec![(LaInputId(0), 9999)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    // ----- Group C: M4 differential oracle (real label vs substitute) -----

    #[test]
    fn m4_real_label_and_substitute_agree_on_pure_a() {
        // The (now test-only) substitute attribution and the real-label
        // path must agree on a pure-A fixture. Disagreement localizes a
        // label-path bug. The substitute is exercised here via the M4
        // test-only helpers (`match_with_input`/`face_candidates`/
        // `majority_vote`), which the Implementer relocates into the test
        // module. If those are not yet callable, this is a compile RED.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);

        // Real-label path:
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // Substitute path (vertex provenance + majority vote) over the
        // SAME kept sub-mesh:
        for t in 0..r.num_tris() {
            let tri = r.as_mesh().tris[t];
            let mut inputs = [None; 3];
            let mut sources = [TessellationSource::Unknown; 3];
            for (k, &vi) in tri.iter().enumerate() {
                let target = r.as_mesh().verts[vi as usize];
                let (inp, src) = match_with_input(&a, &b, target);
                inputs[k] = inp;
                sources[k] = src;
            }
            let sets = [
                face_candidates(inputs[0], sources[0], &a, &b),
                face_candidates(inputs[1], sources[1], &a, &b),
                face_candidates(inputs[2], sources[2], &a, &b),
            ];
            let substitute = majority_vote(&sets);
            let real = attr.lookup(t as u32);
            assert_eq!(
                real, substitute,
                "M4 differential: real-label tri {t} attribution {real:?} \
                 disagrees with substitute {substitute:?}"
            );
        }
        let _ = mesh;
    }

    // ───────────────────────────────────────────────────────────────────
    // PR-M8 disc-rim crossing — rim-override Stage-1 unit tests
    // ───────────────────────────────────────────────────────────────────

    /// A z-axis cylinder B-Rep: bottom cap (−z) at `z=base`, top cap (+z) at
    /// `z=base+h`, seam at +x, radius `r`. Two full-circle rims + one seam
    /// segment (mirrors the m8 test fixture).
    fn rt_cylinder(base: f64, h: f64, r: f64) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let v0 = Point3::new(r, 0.0, base);
        let v1 = Point3::new(r, 0.0, base + h);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base + h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, base),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: base,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -(base + h),
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        (verts, edges, faces)
    }

    /// An EMPTY rim-override map yields byte-identical verts AND tris to the
    /// plain `stage1_tessellate` for a plain cylinder — the uniform-rim path is
    /// 100% untouched.
    #[test]
    fn rim_override_empty_is_byte_identical() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let empty: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
        let overridden = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &empty, None)
            .expect("empty");
        assert_eq!(
            plain.verts.len(),
            overridden.verts.len(),
            "empty override must not add verts"
        );
        for (a, b) in plain.verts.iter().zip(&overridden.verts) {
            assert_eq!(a.as_array(), b.as_array(), "verts must be byte-identical");
        }
        assert_eq!(plain.tris, overridden.tris, "tris must be byte-identical");
    }

    /// Inserting a crossing point on BOTH rims (at the same geometric azimuth):
    /// both points appear bit-exactly on the top AND bottom rim rings, and the
    /// resulting cylinder mesh (caps + lateral) stays a closed 2-manifold.
    #[test]
    fn rim_override_inserts_into_both_rims_no_t_junction() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        // A point on each rim at azimuth 0.3 rad (NOT a uniform sample): radius
        // 0.5 in the rim's plane.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let bottom_pt = Point3::new(0.5 * c, 0.5 * s, 0.0);
        let top_pt = Point3::new(0.5 * c, 0.5 * s, 1.0);
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![bottom_pt]); // bottom rim = circle edge 0
        ov.insert(1, vec![top_pt]); // top rim = circle edge 1
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("dual-rim override");

        // Both inserted points present bit-exactly in the vertex pool.
        let has = |p: Point3| t.verts.iter().any(|q| q.as_array() == p.as_array());
        assert!(has(bottom_pt), "bottom crossing point missing from mesh");
        assert!(has(top_pt), "top crossing point missing from mesh");

        // The mesh stays a closed 2-manifold (every undirected edge shared by
        // exactly two triangles).
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(!counts.is_empty());
        assert!(
            counts.values().all(|&c| c == 2),
            "dual-rim override must keep the cylinder a closed 2-manifold"
        );
    }

    /// KV14 Slice A (spec `yang_stage1_curved_holed_patch`): a cylinder lateral
    /// PARTIAL patch (2 sweep arcs + 2 rulings) carrying an interior hole (an
    /// on-surface inner loop) must tessellate via the unroll+CDT path so the
    /// hole is EXCLUDED from the mesh. The pre-Slice-A partial-patch strip
    /// ignored `inner_loops` and paved over the hole (RED before the fix).
    #[test]
    fn lateral_holed_patch_excludes_hole() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        // Sector theta in [0, PI], z in [0, 2] (a bounded patch with a clean
        // angular gap for the branch cut).
        let a = on(0.0, 0.0); // V0
        let b = on(PI, 0.0); // V1
        let c = on(PI, 2.0); // V2
        let d = on(0.0, 2.0); // V3
                              // Interior triangular hole around theta=PI/2, z=1 (all verts on-surface).
        let h0 = on(PI / 2.0 - 0.4, 0.7); // V4
        let h1 = on(PI / 2.0 + 0.4, 0.7); // V5
        let h2 = on(PI / 2.0, 1.3); // V6
        let verts = [a, b, c, d, h0, h1, h2]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            }, // bottom arc A->B (CCW around +z, sweep PI)
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // ruling B->C
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 2.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            }, // top arc C->D (CCW around -z, sweep PI back over [0,PI])
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // ruling D->A
            BRepEdge {
                start: 4,
                end: 5,
                curve: Curve::LineSegment,
            }, // hole H0->H1
            BRepEdge {
                start: 5,
                end: 6,
                curve: Curve::LineSegment,
            }, // hole H1->H2
            BRepEdge {
                start: 6,
                end: 4,
                curve: Curve::LineSegment,
            }, // hole H2->H0
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("holed lateral tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Param unroll (u = r*theta, v = axial); the axis is +z through origin,
        // so theta = atan2(y, x) is continuous over the [0, PI] sector.
        let param = |p: [f64; 3]| -> (f64, f64) { (r * p[1].atan2(p[0]), p[2]) };
        let huv = [
            param(h0.as_array()),
            param(h1.as_array()),
            param(h2.as_array()),
        ];
        let inside_hole = |u: f64, v: f64| -> bool {
            let (x0, y0) = huv[0];
            let (x1, y1) = huv[1];
            let (x2, y2) = huv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            !(has_neg && has_pos)
        };

        // Oracle 1: no triangle centroid lies inside the hole.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside_hole(u, v),
                "triangle centroid (u={u}, v={v}) lies inside the hole — hole was paved over"
            );
        }

        // Oracle 2: watertight patch — each hole boundary edge borders exactly
        // one triangle (a mesh boundary), never two.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let find = |p: [f64; 3]| -> u32 {
            t.verts
                .iter()
                .position(|q| {
                    let a = q.as_array();
                    (a[0] - p[0]).abs() < 1e-9
                        && (a[1] - p[1]).abs() < 1e-9
                        && (a[2] - p[2]).abs() < 1e-9
                })
                .map(|i| i as u32)
                .expect("hole vertex present in mesh")
        };
        let (gh0, gh1, gh2) = (
            find(h0.as_array()),
            find(h1.as_array()),
            find(h2.as_array()),
        );
        for (x, y) in [(gh0, gh1), (gh1, gh2), (gh2, gh0)] {
            let cnt = undirected.get(&(x.min(y), x.max(y))).copied().unwrap_or(0);
            assert_eq!(
                cnt, 1,
                "hole boundary edge ({x},{y}) must be a mesh boundary (appear once), got {cnt}"
            );
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            // radial = centroid projected off the +z axis through origin.
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 Slice E (spec `yang_stage1_curved_holed_patch`): a CONE lateral
    /// PARTIAL patch (a frustum sector) carrying an interior hole re-enters via
    /// the shared unroll+CDT path (cone isometric development), and the hole is
    /// KV14 Slice F: a POLOIDAL PERIODIC TORUS BAND (the corpus torus-boolean
    /// shape — probe KV14_TORUS_PROBE) re-enters Stage 1 via `tessellate_torus_band`
    /// → `tessellate_torus_patch`. Two full profile circles (at θ0, θ1) bound the
    /// band, one labeled outer, the opposite inner. A torus is not ruled in the
    /// toroidal direction, so the UV-CDT must sample interior toroidal rings onto
    /// the surface. Exact-area oracle: a full-φ band over Δθ has developable area
    /// 2π·R·rm·Δθ; watertightness oracle catches a cracked seam.
    #[test]
    fn torus_poloidal_band_two_encircling_profiles() {
        use std::f64::consts::PI;
        let major = 3.0_f64;
        let minor = 1.0_f64;
        let on = |theta: f64, phi: f64| {
            let rad = major + minor * phi.cos();
            Point3::new(rad * theta.cos(), rad * theta.sin(), minor * phi.sin())
        };
        let n = 24usize;
        let (th0, th1) = (0.2_f64, 1.4_f64);
        let mut verts: Vec<BRepVertex> = Vec::new();
        let circle_at = |theta: f64, verts: &mut Vec<BRepVertex>| -> Vec<u32> {
            let base = verts.len() as u32;
            for k in 0..n {
                let phi = 2.0 * PI * (k as f64) / (n as f64);
                verts.push(BRepVertex {
                    point: on(theta, phi),
                });
            }
            (0..n as u32).map(|k| base + k).collect()
        };
        let ring0 = circle_at(th0, &mut verts);
        let ring1 = circle_at(th1, &mut verts);
        let mut edges: Vec<BRepEdge> = Vec::new();
        let loop_of = |ring: &[u32], edges: &mut Vec<BRepEdge>| -> Vec<u32> {
            let base = edges.len() as u32;
            for k in 0..ring.len() {
                edges.push(BRepEdge {
                    start: ring[k],
                    end: ring[(k + 1) % ring.len()],
                    curve: Curve::LineSegment,
                });
            }
            (0..ring.len() as u32).map(|k| base + k).collect()
        };
        // Outer winds +φ; the inner (a hole boundary) winds −φ — opposite
        // poloidal wrap, as a real face's outer/inner loops are oriented (the
        // band seam bridge requires the two profiles wrap oppositely).
        let ring1_rev: Vec<u32> = ring1.iter().rev().copied().collect();
        let outer = loop_of(&ring0, &mut edges);
        let inner = loop_of(&ring1_rev, &mut edges);
        let faces = vec![BRepFace {
            surface: Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: major,
                minor_radius: minor,
            },
            outer_loop: outer,
            inner_loops: vec![inner],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("torus band tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let nx = e1[1] * e2[2] - e1[2] * e2[1];
            let ny = e1[2] * e2[0] - e1[0] * e2[2];
            let nz = e1[0] * e2[1] - e1[1] * e2[0];
            0.5 * (nx * nx + ny * ny + nz * nz).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        let band = 2.0 * PI * major * minor * (th1 - th0);
        assert!(
            area > 0.97 * band && area <= band + 1e-9,
            "torus band area {area} must fill 2π·R·rm·Δθ (≈{band}, inscribed)"
        );

        // Watertight: every undirected edge is shared by exactly 2 triangles OR
        // lies on the two profile-circle boundaries (a shared-with-cap rim). A
        // cracked seam would leave interior edges with count 1.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |g: u32| {
            let p = t.verts[g as usize].as_array();
            p[1].atan2(p[0])
        };
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times (fold)");
            if c == 1 {
                // Only profile-rim edges (both ends at θ0 or both at θ1) may be
                // single-count (they border the adjacent cap, absent here).
                let (tx, ty) = (theta_of(x), theta_of(y));
                let on_rim = ((tx - th0).abs() < 1e-6 && (ty - th0).abs() < 1e-6)
                    || ((tx - th1).abs() < 1e-6 && (ty - th1).abs() < 1e-6);
                assert!(
                    on_rim,
                    "interior edge ({x},{y}) is a boundary — cracked seam in the band"
                );
            }
        }
    }

    /// EXCLUDED. Covers the cone `inner_loops` → CDT route (P4).
    #[test]
    fn cone_holed_patch_excludes_hole() {
        use std::f64::consts::PI;
        let tan_a = 0.5_f64;
        let half_angle = tan_a.atan();
        let (sa, ca) = (half_angle.sin(), half_angle.cos());
        let on = |theta: f64, z: f64| {
            let rr = z * tan_a;
            Point3::new(rr * theta.cos(), rr * theta.sin(), z)
        };
        // Sector theta in [0, PI], z in [1, 3] (a bounded frustum patch).
        let z0 = 1.0_f64;
        let z1 = 3.0_f64;
        let a = on(0.0, z0); // V0
        let b = on(PI, z0); // V1
        let c = on(PI, z1); // V2
        let d = on(0.0, z1); // V3
                             // Interior triangular hole around theta=PI/2, z=2 (on-surface).
        let h0 = on(PI / 2.0 - 0.4, 1.6); // V4
        let h1 = on(PI / 2.0 + 0.4, 1.6); // V5
        let h2 = on(PI / 2.0, 2.4); // V6
        let verts = [a, b, c, d, h0, h1, h2]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, z0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: z0 * tan_a,
                },
            }, // bottom arc A->B
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // ruling B->C
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, z1),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: z1 * tan_a,
                },
            }, // top arc C->D
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // ruling D->A
            BRepEdge {
                start: 4,
                end: 5,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 5,
                end: 6,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 6,
                end: 4,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("holed cone tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Cone isometric development (ℓ = v/cosα, ψ = θ·sinα) — the same 2D
        // layout the tessellator uses (up to the branch-cut rotation, which does
        // not affect a point-in-triangle test).
        let param = |p: [f64; 3]| -> (f64, f64) {
            let ell = p[2].abs() / ca;
            let psi = p[1].atan2(p[0]) * sa;
            (ell * psi.cos(), ell * psi.sin())
        };
        let huv = [
            param(h0.as_array()),
            param(h1.as_array()),
            param(h2.as_array()),
        ];
        let inside_hole = |u: f64, v: f64| -> bool {
            let (x0, y0) = huv[0];
            let (x1, y1) = huv[1];
            let (x2, y2) = huv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            !(has_neg && has_pos)
        };

        // Oracle 1: no triangle centroid lies inside the hole.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside_hole(u, v),
                "cone triangle centroid (u={u}, v={v}) lies inside the hole — hole paved over"
            );
        }

        // Oracle 2: watertight — each hole boundary edge borders exactly one tri.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let find = |p: [f64; 3]| -> u32 {
            t.verts
                .iter()
                .position(|q| {
                    let a = q.as_array();
                    (a[0] - p[0]).abs() < 1e-9
                        && (a[1] - p[1]).abs() < 1e-9
                        && (a[2] - p[2]).abs() < 1e-9
                })
                .map(|i| i as u32)
                .expect("hole vertex present in mesh")
        };
        let (gh0, gh1, gh2) = (
            find(h0.as_array()),
            find(h1.as_array()),
            find(h2.as_array()),
        );
        for (x, y) in [(gh0, gh1), (gh1, gh2), (gh2, gh0)] {
            let cnt = undirected.get(&(x.min(y), x.max(y))).copied().unwrap_or(0);
            assert_eq!(
                cnt, 1,
                "hole boundary edge ({x},{y}) must be a mesh boundary (once), got {cnt}"
            );
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot > 0.0,
                "cone triangle must face radially outward, dot={dot}"
            );
        }
    }

    /// KV14 Slice B (spec `yang_stage1_curved_holed_patch`): a PERIODIC
    /// cylinder-wall strip whose boundary loops each ENCIRCLE the axis (a full
    /// 2π rim / intersection ring, |Σ Δθ| ≈ 2π). Real boolean outputs represent
    /// a windowed cylinder wall this way — one encircling loop labeled `outer`,
    /// the opposite rim labeled `inner`. Slice A's polygon-with-holes model
    /// unrolls a full rim to a zero-area horizontal line, so the CDT fails
    /// outright (RED before Slice B). Slice B classifies the two encircling
    /// loops as the strip's v-boundaries and lays them into ONE simple ribbon.
    #[test]
    fn periodic_strip_two_encircling_rims() {
        let r = 1.0_f64;
        let h = 2.0_f64;
        // Square cross-section sampling: 4 azimuths per rim (θ = 0, π/2, π,
        // 3π/2) → the exact lateral area is a 4-gon prism wall = 4·(r√2)·h.
        let bottom = [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(-1.0, 0.0, 0.0),
            Point3::new(0.0, -1.0, 0.0),
        ];
        let top = [
            Point3::new(1.0, 0.0, h),
            Point3::new(0.0, 1.0, h),
            Point3::new(-1.0, 0.0, h),
            Point3::new(0.0, -1.0, h),
        ];
        let verts = bottom
            .iter()
            .chain(top.iter())
            .map(|&point| BRepVertex { point })
            .collect::<Vec<_>>();
        let arc = |start: u32, end: u32, z: f64| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        };
        // Bottom rim (outer): 4 CCW arcs winding +2π. Top rim (inner): likewise.
        let edges = vec![
            arc(0, 1, 0.0),
            arc(1, 2, 0.0),
            arc(2, 3, 0.0),
            arc(3, 0, 0.0),
            arc(4, 5, h),
            arc(5, 6, h),
            arc(6, 7, h),
            arc(7, 4, h),
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6, 7]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("periodic strip tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: total lateral area equals the exact 4-gon prism wall
        // (proves the strip covers the FULL 2π, no seam gap, no double cover).
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        // The strip is inscribed in the true cylinder wall (2π·r·h), so its area
        // approaches that from BELOW as sampling refines. A missing seam wedge
        // drops the area by a whole facet column (≈10% at this sampling), so a
        // 97% floor cleanly separates a full wrap from a gap — independent of
        // the exact arc-sample count.
        let full_wall = 2.0 * std::f64::consts::PI * r * h;
        assert!(
            area > 0.97 * full_wall && area <= full_wall + 1e-9,
            "strip area {area} must fill the full 2π wall (≈{full_wall}, inscribed)"
        );

        // Oracle 2: watertight ribbon — every mesh-boundary (count-1) edge lies
        // ENTIRELY on a rim (both endpoints at z=0 or both at z=h), and no edge
        // is covered more than twice. A seam gap leaves a vertical boundary edge
        // spanning z=0→z=h; a fold double-covers. Sampling-independent.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let on_rim = |z: f64| z.abs() < 1e-9 || (z - h).abs() < 1e-9;
        let mut boundary_edges = 0usize;
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                boundary_edges += 1;
                let zx = t.verts[x as usize].as_array()[2];
                let zy = t.verts[y as usize].as_array()[2];
                assert!(
                    on_rim(zx) && on_rim(zy) && (zx - zy).abs() < 1e-9,
                    "boundary edge ({x},{y}) at z=({zx},{zy}) is not a rim edge — seam gap"
                );
            }
        }
        assert!(boundary_edges > 0, "the tube strip has open rims");

        // Oracle 3: every triangle faces radially outward.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 ellipse-arc re-entry (spec `kv14_ellipse_arc_reentry`): a PLANAR
    /// face whose loop mixes LineSegment + one `Curve::Ellipse` ARC (the
    /// oblique plane∩cylinder section a prior boolean leaves on a cap —
    /// R0006/F0076's planar-loop sub-kind) re-enters Stage 1 through the
    /// generalized curved CDT. The ellipse chain pre-pass samples the arc at
    /// the circle chord rule on `major_radius`; the sector tessellates
    /// watertight with the chorded area approaching the analytic sector area
    /// `½·a·b·Δt` from below.
    #[test]
    fn planar_ellipse_sector_reenters_stage1() {
        use std::f64::consts::FRAC_PI_2;
        let a = 2.0_f64; // major radius (along +x)
        let b = 1.0_f64; // minor radius (along +y)
                         // Quarter sector: ellipse arc from t=0 (2,0,0) to t=π/2 (0,1,0)
                         // (sweep π/2 < π — the guaranteed-minor-arc input convention), then
                         // two straight legs through the center.
        let verts = vec![
            BRepVertex {
                point: Point3::new(a, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, b, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 0.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Ellipse {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    major_axis: Vector3::new(1.0, 0.0, 0.0),
                    major_radius: a,
                    minor_radius: b,
                },
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("ellipse sector tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1 (on-surface): every vertex lies in the z=0 plane, and every
        // NON-endpoint vertex sourced from the ellipse edge satisfies the
        // ellipse implicit (x/a)² + (y/b)² = 1.
        let mut ellipse_steiner = 0usize;
        for (i, v) in t.verts.iter().enumerate() {
            let p = v.as_array();
            assert!(p[2].abs() < 1e-12, "vertex {i} off the sector plane");
            if let TessellationSource::BRepEdge { edge: 0, .. } = t.sources[i] {
                let r = (p[0] / a).powi(2) + (p[1] / b).powi(2);
                assert!(
                    (r - 1.0).abs() < 1e-9,
                    "ellipse sample {i} off the ellipse: implicit residual {r}"
                );
                ellipse_steiner += 1;
            }
        }
        assert!(
            ellipse_steiner >= 1,
            "the arc must be subdivided (chord rule), got {ellipse_steiner} interior samples"
        );

        // Oracle 2 (area): the chorded sector area approaches the analytic
        // `½·a·b·Δt` from BELOW (inscribed).
        let analytic = 0.5 * a * b * FRAC_PI_2;
        let area: f64 = t
            .tris
            .iter()
            .map(|tri| {
                let p0 = t.verts[tri[0] as usize].as_array();
                let p1 = t.verts[tri[1] as usize].as_array();
                let p2 = t.verts[tri[2] as usize].as_array();
                let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
                0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs()
            })
            .sum();
        assert!(
            area <= analytic + 1e-9 && area > 0.985 * analytic,
            "sector area {area} vs analytic {analytic}"
        );

        // Oracle 3 (watertight cover): every undirected mesh edge is covered
        // once (boundary) or twice (interior) — no T-junction, no fold.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times");
        }
    }

    /// KV14 ellipse-arc re-entry: a planar cap bounded by a single FULL
    /// `Curve::Ellipse` loop (`start == end` — the complete oblique section)
    /// tessellates through the same chain + CDT path, area → π·a·b from below.
    #[test]
    fn planar_full_ellipse_cap_reenters_stage1() {
        let a = 2.0_f64;
        let b = 1.0_f64;
        let verts = vec![BRepVertex {
            point: Point3::new(a, 0.0, 0.0),
        }];
        let edges = vec![BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Ellipse {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                major_axis: Vector3::new(1.0, 0.0, 0.0),
                major_radius: a,
                minor_radius: b,
            },
        }];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("full ellipse cap tessellation");
        let analytic = std::f64::consts::PI * a * b;
        let area: f64 = t
            .tris
            .iter()
            .map(|tri| {
                let p0 = t.verts[tri[0] as usize].as_array();
                let p1 = t.verts[tri[1] as usize].as_array();
                let p2 = t.verts[tri[2] as usize].as_array();
                let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
                0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs()
            })
            .sum();
        assert!(
            area <= analytic + 1e-9 && area > 0.985 * analytic,
            "cap area {area} vs analytic {analytic}"
        );
    }

    /// KV14 ellipse-arc re-entry (curved-lateral sub-kind): a cylinder wall
    /// bounded below by a full circle rim and above by the full OBLIQUE
    /// ellipse (`plane ∩ cylinder`, R0095's vocabulary) routes through the
    /// holed-CDT periodic strip: both loops encircle the axis, the ellipse
    /// chain samples lie exactly ON the cylinder, and the wall area
    /// approaches `r·∫(h + k·cosθ)dθ = 2π·r·h` from below.
    #[test]
    fn lateral_oblique_ellipse_tube_reenters_stage1() {
        let r = 1.0_f64;
        let h = 2.0_f64; // ellipse-plane height at the axis
        let k = 0.5_f64; // slope: top plane z = h + k·x
                         // Oblique plane through (0,0,h) with unit normal (−sinφ, 0, cosφ),
                         // tanφ = k: section ellipse center (0,0,h), major axis (cosφ,0,sinφ),
                         // a = r/cosφ, b = r. P(t) = (r·cos t, r·sin t, h + k·r·cos t) — every
                         // sample is exactly on the cylinder.
        let cphi = 1.0 / (1.0 + k * k).sqrt();
        let sphi = k * cphi;
        let verts = vec![
            BRepVertex {
                point: Point3::new(r, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(r, 0.0, h + k * r),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Ellipse {
                    center: Point3::new(0.0, 0.0, h),
                    normal: Vector3::new(-sphi, 0.0, cphi),
                    major_axis: Vector3::new(cphi, 0.0, sphi),
                    major_radius: r / cphi,
                    minor_radius: r,
                },
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0],
            inner_loops: vec![vec![1]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("oblique ellipse tube");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: every vertex lies exactly on the cylinder (the ellipse
        // parameterization is on-surface by construction; the unroll must
        // not displace it).
        for (i, v) in t.verts.iter().enumerate() {
            let p = v.as_array();
            let rad = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!(
                (rad - r).abs() < 1e-9,
                "vertex {i} off the cylinder: radial {rad}"
            );
        }

        // Oracle 2: wall area → 2π·r·h from below (the k·cosθ term integrates
        // to zero over the full turn).
        let analytic = 2.0 * std::f64::consts::PI * r * h;
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let p0 = t.verts[tri[0] as usize].as_array();
            let p1 = t.verts[tri[1] as usize].as_array();
            let p2 = t.verts[tri[2] as usize].as_array();
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        assert!(
            area > 0.97 * analytic && area <= analytic + 1e-9,
            "wall area {area} vs analytic {analytic} (inscribed)"
        );

        // Oracle 3: watertight ribbon — every boundary (count-1) edge lies
        // entirely on the bottom rim (z≈0) or on the ellipse plane
        // (z ≈ h + k·x); no edge covered more than twice.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k3 in 0..3 {
                let (x, y) = (tri[k3], tri[(k3 + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let on_boundary = |g: u32| -> bool {
            let p = t.verts[g as usize].as_array();
            p[2].abs() < 1e-9 || (p[2] - (h + k * p[0])).abs() < 1e-9
        };
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times (fold)");
            if c == 1 {
                assert!(
                    on_boundary(x) && on_boundary(y),
                    "boundary edge ({x},{y}) is not on a rim/ellipse — seam gap"
                );
            }
        }
    }

    /// KV14 Slice D (spec `yang_stage1_curved_holed_patch`): a cylinder lateral
    /// whose outer loop is NON-canonical — no full-circle rims and NOT the
    /// structured 2-arc partial-patch pattern — with NO holes. Real boolean
    /// outputs produce these when a prior op bites an irregular boundary into a
    /// partial patch (R0053 = [L,A,A,A,L,A,A,A]: each rim split into 3 arcs +
    /// 2 rulings). The pre-Slice-D dispatch walled these `MalformedTopology`
    /// ("found 0 full rims and 6 arcs"); Slice D routes them to the same
    /// unroll+CDT path (empty hole set), classifying the single winding-0 outer
    /// loop as a bounded partial patch.
    #[test]
    fn lateral_partial_patch_multi_arc_no_holes() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let h = 2.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        // Sector theta in [0, PI] (a clean angular gap over (PI, 2PI) for the
        // branch cut), z in [0, h]. Each rim split into 3 arcs at PI/3, 2PI/3.
        // Outer loop: [A,A,A, L, A,A,A, L] = R0053's vocabulary (rotated).
        let b0 = on(0.0, 0.0); // V0
        let b1 = on(PI / 3.0, 0.0); // V1
        let b2 = on(2.0 * PI / 3.0, 0.0); // V2
        let b3 = on(PI, 0.0); // V3
        let t3 = on(PI, h); // V4
        let t2 = on(2.0 * PI / 3.0, h); // V5
        let t1 = on(PI / 3.0, h); // V6
        let t0 = on(0.0, h); // V7
        let verts = [b0, b1, b2, b3, t3, t2, t1, t0]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        // Bottom arcs sweep CCW about +z; top arcs sweep CCW about −z (returning
        // over [PI, 0]) so the loop nets zero axial winding (a bounded patch).
        let bot_arc = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        };
        let top_arc = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, h),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        };
        let ruling = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            bot_arc(0, 1), // e0
            bot_arc(1, 2), // e1
            bot_arc(2, 3), // e2
            ruling(3, 4),  // e3 (V3->V4, up)
            top_arc(4, 5), // e4
            top_arc(5, 6), // e5
            top_arc(6, 7), // e6
            ruling(7, 0),  // e7 (V7->V0, down)
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3, 4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces)
            .expect("Slice D multi-arc partial patch tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: total area equals the inscribed sector wall (r·PI)·h = PI·h.
        // A CDT that dropped the seam wedge or double-covered would miss/exceed
        // this; approached from BELOW since the arcs are chord-sampled.
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        let sector_wall = r * PI * h;
        assert!(
            area > 0.97 * sector_wall && area <= sector_wall + 1e-9,
            "patch area {area} must fill the PI sector wall (≈{sector_wall}, inscribed)"
        );

        // Oracle 2: watertight bounded patch — no interior holes, no fold. Every
        // count-1 boundary edge lies on the OUTER boundary: a rim (both ends at
        // z=0 or both at z=h) or a ruling (both ends at theta=0 or theta=PI).
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |p: [f64; 3]| p[1].atan2(p[0]);
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                let px = t.verts[x as usize].as_array();
                let py = t.verts[y as usize].as_array();
                let on_rim = (px[2].abs() < 1e-9 && py[2].abs() < 1e-9)
                    || ((px[2] - h).abs() < 1e-9 && (py[2] - h).abs() < 1e-9);
                let (tx, ty) = (theta_of(px), theta_of(py));
                let on_ruling = (tx.abs() < 1e-6 && ty.abs() < 1e-6)
                    || ((tx - PI).abs() < 1e-6 && (ty - PI).abs() < 1e-6);
                assert!(
                    on_rim || on_ruling,
                    "boundary edge ({x},{y}) is interior — hole or seam gap in a hole-free patch"
                );
            }
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 Slice E: a non-canonical CONE partial patch (multi-arc, no holes)
    /// re-enters the unroll+CDT path. A cone frustum sector [A,A,A,L,A,A,A,L]
    /// (R0020's vocabulary) with the u-scale varying by axial radius. Oracles:
    /// the patch fills the exact developable sector-frustum area (from below —
    /// chord-sampled), it is watertight and bounded (no interior hole), and it
    /// faces radially outward.
    #[test]
    fn cone_partial_patch_multi_arc_no_holes() {
        use std::f64::consts::PI;
        // Cone: apex at origin, axis +z, half-angle atan(0.5) (tan α = 0.5).
        let tan_a = 0.5_f64;
        let half_angle = tan_a.atan();
        let on = |theta: f64, z: f64| {
            let r = z * tan_a;
            Point3::new(r * theta.cos(), r * theta.sin(), z)
        };
        // Sector theta in [0, PI] (a clean gap over (PI, 2PI) for the branch
        // cut), between z=1 (r=0.5) and z=3 (r=1.5). Each rim split into 3 arcs.
        let z0 = 1.0_f64;
        let z1 = 3.0_f64;
        let b0 = on(0.0, z0);
        let b1 = on(PI / 3.0, z0);
        let b2 = on(2.0 * PI / 3.0, z0);
        let b3 = on(PI, z0);
        let t3 = on(PI, z1);
        let t2 = on(2.0 * PI / 3.0, z1);
        let t1 = on(PI / 3.0, z1);
        let t0 = on(0.0, z1);
        let verts = [b0, b1, b2, b3, t3, t2, t1, t0]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        // Bottom arcs sweep CCW about +z at radius r0; top arcs return over
        // [PI, 0] about −z at radius r1 (nets zero axial winding = bounded).
        let arc = |start: u32, end: u32, z: f64, up: bool| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal: Vector3::new(0.0, 0.0, if up { 1.0 } else { -1.0 }),
                radius: z * tan_a,
            },
        };
        let ruling = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            arc(0, 1, z0, true),  // e0
            arc(1, 2, z0, true),  // e1
            arc(2, 3, z0, true),  // e2
            ruling(3, 4),         // e3 (up generator)
            arc(4, 5, z1, false), // e4
            arc(5, 6, z1, false), // e5
            arc(6, 7, z1, false), // e6
            ruling(7, 0),         // e7 (down generator)
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle,
            },
            outer_loop: vec![0, 1, 2, 3, 4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces)
            .expect("Slice E cone multi-arc partial patch tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        // Developable frustum-sector area over Δθ = PI:
        // (Δθ/2)·(r0+r1)·L, L = (z1−z0)/cosα.
        let r0 = z0 * tan_a;
        let r1 = z1 * tan_a;
        let cos_a = half_angle.cos();
        let slant = (z1 - z0) / cos_a;
        let sector_wall = (PI / 2.0) * (r0 + r1) * slant;
        assert!(
            area > 0.97 * sector_wall && area <= sector_wall + 1e-9,
            "cone patch area {area} must fill the frustum sector wall (≈{sector_wall}, inscribed)"
        );

        // Watertight bounded patch: every count-1 edge lies on the OUTER
        // boundary — a rim (both ends at z0 or both at z1) or a generator (both
        // ends at theta=0 or theta=PI).
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |p: [f64; 3]| p[1].atan2(p[0]);
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                let px = t.verts[x as usize].as_array();
                let py = t.verts[y as usize].as_array();
                let on_rim = ((px[2] - z0).abs() < 1e-9 && (py[2] - z0).abs() < 1e-9)
                    || ((px[2] - z1).abs() < 1e-9 && (py[2] - z1).abs() < 1e-9);
                let (tx, ty) = (theta_of(px), theta_of(py));
                let on_gen = (tx.abs() < 1e-6 && ty.abs() < 1e-6)
                    || ((tx - PI).abs() < 1e-6 && (ty - PI).abs() < 1e-6);
                assert!(
                    on_rim || on_gen,
                    "boundary edge ({x},{y}) is interior — hole or seam gap in a hole-free patch"
                );
            }
        }

        // Every triangle faces radially outward (reversed = false): positive
        // radial component (a cone normal is tilted but stays outward in r).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot > 0.0,
                "cone triangle must face radially outward, dot={dot}"
            );
        }
    }

    /// KV14 Slice A edge case: a `reversed` holed lateral (a cavity/bore wall)
    /// excludes the hole AND faces radially INWARD, and a patch with TWO holes
    /// excludes both. Covers the `f.reversed` branch (P4) + multi-hole input.
    #[test]
    fn lateral_holed_patch_reversed_and_multi_hole() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        let a = on(0.0, 0.0);
        let b = on(PI, 0.0);
        let c = on(PI, 2.0);
        let d = on(0.0, 2.0);
        // Two disjoint triangular holes in the sector.
        let h = |cz: f64| {
            [
                on(PI / 2.0 - 0.3, cz - 0.2),
                on(PI / 2.0 + 0.3, cz - 0.2),
                on(PI / 2.0, cz + 0.25),
            ]
        };
        let hole_a = h(0.6);
        let hole_b = h(1.4);
        let verts = [a, b, c, d]
            .into_iter()
            .chain(hole_a)
            .chain(hole_b)
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let mut edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 2.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // Hole A verts = 4,5,6 ; hole B verts = 7,8,9.
        for (base, _) in [(4u32, ()), (7u32, ())] {
            edges.push(BRepEdge {
                start: base,
                end: base + 1,
                curve: Curve::LineSegment,
            });
            edges.push(BRepEdge {
                start: base + 1,
                end: base + 2,
                curve: Curve::LineSegment,
            });
            edges.push(BRepEdge {
                start: base + 2,
                end: base,
                curve: Curve::LineSegment,
            });
        }
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6], vec![7, 8, 9]],
            reversed: true,
        }];
        let t =
            stage1_tessellate(&verts, &edges, &faces).expect("reversed multi-hole tessellation");
        assert!(!t.tris.is_empty());

        let param = |p: [f64; 3]| -> (f64, f64) { (r * p[1].atan2(p[0]), p[2]) };
        let tri_of = |hole: &[Point3; 3]| {
            [
                param(hole[0].as_array()),
                param(hole[1].as_array()),
                param(hole[2].as_array()),
            ]
        };
        let inside = |uv: &[(f64, f64); 3], u: f64, v: f64| -> bool {
            let (x0, y0) = uv[0];
            let (x1, y1) = uv[1];
            let (x2, y2) = uv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        let uva = tri_of(&hole_a);
        let uvb = tri_of(&hole_b);
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside(&uva, u, v) && !inside(&uvb, u, v),
                "a hole was paved over"
            );
            // reversed ⇒ inward-facing: geometric normal · radial < 0.
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot < 0.0,
                "reversed cavity wall must face inward, dot={dot}"
            );
        }
    }

    /// M-C RED (spec `m8_stage0_band_scale_crossing_verts` §4 E-C1): two
    /// DISTINCT override points whose angular separation is far below the
    /// legacy merge_tol (band-close genuine crossings — the R0088/R0070
    /// twin population) must BOTH be inserted into the rim ring. Silently
    /// keeping only one desynchronizes the ring from the cap override that
    /// carries both points (T-junction holes, the measured M-C class). A
    /// bit-identical duplicate must still be deduplicated (E-C1b).
    #[test]
    fn rim_override_band_close_distinct_points_both_inserted() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let mk = |az: f64, z: f64| {
            let (s, c) = az.sin_cos();
            Point3::new(r * c, r * s, z)
        };
        // Two on-circle points ~2e-13 rad apart (distinct f64 coordinates,
        // far below uni_step·1e-6), on both rims for lateral balance.
        let (az1, az2) = (0.3_f64, 0.3_f64 + 2.0e-13);
        let (b1, b2) = (mk(az1, 0.0), mk(az2, 0.0));
        let (t1, t2) = (mk(az1, 1.0), mk(az2, 1.0));
        assert_ne!(b1.as_array(), b2.as_array(), "twin construction degenerate");
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![b1, b2]);
        ov.insert(1, vec![t1, t2]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("band-close distinct overrides must be accepted");
        for (name, p) in [("b1", b1), ("b2", b2), ("t1", t1), ("t2", t2)] {
            assert!(
                t.verts.iter().any(|q| q.as_array() == p.as_array()),
                "M-C RED — distinct band-close override {name} missing from the \
                 rim ring (silent merge_tol drop, spec §2)"
            );
        }
        // Ring stays a closed 2-manifold with the band-thin segments present.
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            counts.values().all(|&c| c == 2),
            "band-close override insertion must keep the cylinder closed"
        );

        // E-C1b: a bit-identical duplicate is still dropped (no double vertex).
        // Balanced across both rims (the lateral azimuth-merge expectation).
        let mut dup: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        dup.insert(0, vec![b1, b1]);
        dup.insert(1, vec![t1, t1]);
        let td = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &dup, None)
            .expect("bit-identical duplicate override must be accepted");
        assert_eq!(
            td.verts
                .iter()
                .filter(|q| q.as_array() == t1.as_array())
                .count(),
            1,
            "bit-identical duplicate override must be deduplicated exactly once"
        );
    }

    /// Chained swiss-cheese wall 1 RED (task #62, spec
    /// `m8_holed_disc_coplanar_overlay` §8 increment 5): the azimuth-merge
    /// lateral pairing must be WRAP-AWARE. A RECOVERED B-Rep (boolean output
    /// re-entering a boolean) can carry one rim's seam vertex at azimuth
    /// exactly 0 while the other rim's sits a femto BELOW the +x axis
    /// (y = −ε): `atan2(…).rem_euclid(2π)` maps the latter to 2π−ε, sorting
    /// it LAST instead of FIRST, and the positional `bot[k] ↔ top[k]` pairing
    /// shifts by one slot — the F0086 step-2 wall
    /// (`azimuth-merge rims disagree at index 0 (bottom 0 vs top 0.4488)`).
    /// The two sorted rings are CIRCULAR sequences: pairing must align them
    /// by cyclic shift, not by absolute sort position.
    ///
    /// Fixture: rt-style cylinder whose TOP seam vertex is rotated a femto
    /// below the +x axis (y = −r·5e−16, on-circle within band), with one
    /// same-azimuth override pair on both rims to force the azimuth-merge
    /// path. Oracle: tessellation SUCCEEDS and stays a closed 2-manifold.
    /// RED today: MalformedTopology "rims disagree at index 0".
    #[test]
    fn rim_override_wrap_seam_cyclic_alignment() {
        let r = 0.5_f64;
        let eps_y = -r * 5.0e-16; // top seam vertex a femto BELOW the +x axis
        let v0 = Point3::new(r, 0.0, 0.0);
        let v1 = Point3::new(r, eps_y, 1.0);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 1.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -1.0,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        // One override pair at the same geometric azimuth on both rims (not
        // near a uniform sample) — forces the azimuth-merge lateral path.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![Point3::new(r * c, r * s, 0.0)]);
        ov.insert(1, vec![Point3::new(r * c, r * s, 1.0)]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None).expect(
            "wrap-seam cylinder must tessellate — the azimuth-merge pairing \
             must align the rings cyclically, not by absolute sort position",
        );
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            !counts.is_empty() && counts.values().all(|&c| c == 2),
            "wrap-seam cylinder must stay a closed 2-manifold"
        );
    }

    /// M8 holed-disc increment 3 RED (spec `m8_holed_disc_coplanar_overlay`
    /// §8): ULP-TWIN override points — two distinct points 1 ULP apart in x
    /// whose f64 seam-relative rim angles COLLIDE — must be ring-ordered by
    /// their EXACT angular order on BOTH rims, regardless of the caller's
    /// insertion order, and the lateral strip must pair each bottom twin with
    /// its same-azimuth top partner (no twisted quad). Today the slot sort
    /// falls back to insertion order on the f64 tie, and the two rims' frames
    /// have OPPOSITE orientations, so one rim always comes out mis-ordered →
    /// the cap fan walks U_lo–twinB–twinA–U_hi on one cap (wrong adjacency)
    /// and the wall strip twists (a self-intersecting Stage-0 mesh — the
    /// `annular_cap_under_disc` cherchi `SegmentNotLocatable` wall).
    ///
    /// Oracles (frame-independent, structural):
    /// - on each cap, the uniform sample at the LOWER global azimuth is
    ///   ring-adjacent to the LOWER-azimuth twin (and not to the other);
    /// - the lateral contains BOTH vertical edges (A_bot,A_top), (B_bot,B_top);
    /// - the full mesh stays a closed 2-manifold;
    /// - both insertion orders ([A,B] and [B,A]) yield the same triangle SET.
    #[test]
    fn rim_override_ulp_twins_exact_order_both_rims() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);

        // Pick the bottom-rim chord whose midpoint has the smallest |x| (near
        // the ±y axis, far from the seam at +x): there the azimuth derivative
        // dθ/dx = |y|/r² is maximal while ULP(θ-offset) is fixed, so a 1-ULP
        // x perturbation moves the angle by far LESS than one ULP of the
        // seam-relative offset → the f64 angles of the twins collide.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim0: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 0, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim0.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim0.len() >= 4, "bottom rim must have >=4 Steiner samples");
        let mut best: Option<([f64; 2], [f64; 2])> = None;
        for w in rim0.windows(2) {
            let (p0, p1) = (w[0].1.as_array(), w[1].1.as_array());
            let mid_x = 0.5 * (p0[0] + p1[0]);
            if best.is_none_or(|(a, b)| mid_x.abs() < 0.5 * (a[0] + b[0]).abs()) {
                best = Some(([p0[0], p0[1]], [p1[0], p1[1]]));
            }
        }
        let (e0, e1) = best.unwrap();
        let mx = 0.5 * (e0[0] + e1[0]);
        let my = 0.5 * (e0[1] + e1[1]);
        // The ULP twins: same y, x one ULP apart (the real Stage-0 twin shape:
        // two sweep-event columns from 1-ULP-different rim-sample x's).
        let xa = mx;
        let xb = f64::from_bits(mx.to_bits() + 1);
        assert_ne!(xa, xb, "twin construction degenerate");
        // Exact global-azimuth order: cross(A,B) = xa·my − my·xb = my·(xa−xb),
        // exact in f64 (adjacent-float subtraction is exact). Positive cross
        // means B is CCW of A, i.e. A has the LOWER azimuth.
        let a_first = my * (xa - xb) > 0.0;
        let (x_lo, x_hi) = if a_first { (xa, xb) } else { (xb, xa) };
        let tw_lo_b = Point3::new(x_lo, my, 0.0); // lower-azimuth twin, bottom
        let tw_hi_b = Point3::new(x_hi, my, 0.0);
        let tw_lo_t = Point3::new(x_lo, my, 1.0); // same azimuths on top rim
        let tw_hi_t = Point3::new(x_hi, my, 1.0);
        // Twin global azimuth (for locating each cap's bracketing uniform
        // samples — the top rim's samples are NOT bit-identical in (x,y) to
        // the bottom's, its frame flips, so each cap is searched on its own).
        let az_of = |x: f64, y: f64| y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI);
        let az_tw = az_of(mx, my);

        let run = |first: Point3, second: Point3, tfirst: Point3, tsecond: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(0, vec![first, second]);
            ov.insert(1, vec![tfirst, tsecond]);
            stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
                .expect("ULP-twin overrides must be accepted")
        };

        let check = |t: &Stage1Tess, tag: &str| {
            let vid = |p: Point3| -> u32 {
                t.verts
                    .iter()
                    .position(|q| q.as_array() == p.as_array())
                    .unwrap_or_else(|| panic!("{tag}: point {p:?} missing from mesh"))
                    as u32
            };
            // The rim-E uniform samples bracketing the twin azimuth (the
            // twins' ring neighbours on that rim).
            let brackets = |edge: u32| -> (u32, u32) {
                let mut lo: Option<(f64, u32)> = None;
                let mut hi: Option<(f64, u32)> = None;
                for (i, src) in t.sources.iter().enumerate() {
                    if !matches!(src, TessellationSource::BRepEdge { edge: e, .. } if *e == edge) {
                        continue;
                    }
                    let a = t.verts[i].as_array();
                    // Skip the inserted twins themselves (also BRepEdge-tagged).
                    if a[1] == my && (a[0] == xa || a[0] == xb) {
                        continue;
                    }
                    let az = az_of(a[0], a[1]);
                    if az < az_tw {
                        if lo.is_none_or(|(b, _)| az > b) {
                            lo = Some((az, i as u32));
                        }
                    } else if hi.is_none_or(|(b, _)| az < b) {
                        hi = Some((az, i as u32));
                    }
                }
                (
                    lo.unwrap_or_else(|| panic!("{tag}: no uniform below twin on rim {edge}"))
                        .1,
                    hi.unwrap_or_else(|| panic!("{tag}: no uniform above twin on rim {edge}"))
                        .1,
                )
            };
            // Undirected edge sets: bottom cap (all z==0), top cap (all z==1),
            // lateral (z-spanning).
            let mut cap_b = std::collections::BTreeSet::new();
            let mut cap_t = std::collections::BTreeSet::new();
            let mut lat = std::collections::BTreeSet::new();
            let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
                std::collections::BTreeMap::new();
            for tri in &t.tris {
                let zs: Vec<f64> = tri
                    .iter()
                    .map(|&v| t.verts[v as usize].as_array()[2])
                    .collect();
                let bucket: &mut std::collections::BTreeSet<(u32, u32)> =
                    if zs.iter().all(|&z| z == 0.0) {
                        &mut cap_b
                    } else if zs.iter().all(|&z| z == 1.0) {
                        &mut cap_t
                    } else {
                        &mut lat
                    };
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    let e = (a.min(b), a.max(b));
                    bucket.insert(e);
                    *counts.entry(e).or_insert(0) += 1;
                }
            }
            let e = |a: u32, b: u32| (a.min(b), a.max(b));
            for (cap, lo, hi, edge, z) in [
                (&cap_b, tw_lo_b, tw_hi_b, 0u32, 0.0),
                (&cap_t, tw_lo_t, tw_hi_t, 1u32, 1.0),
            ] {
                let (vlo, vhi) = (vid(lo), vid(hi));
                let (ulo, uhi) = brackets(edge);
                assert!(
                    cap.contains(&e(ulo, vlo)),
                    "{tag}: cap z={z} — lower uniform must be ring-adjacent to \
                     the LOWER-azimuth twin (exact order), edge missing"
                );
                assert!(
                    !cap.contains(&e(ulo, vhi)),
                    "{tag}: cap z={z} — lower uniform adjacent to the HIGHER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
                assert!(
                    cap.contains(&e(uhi, vhi)),
                    "{tag}: cap z={z} — upper uniform must be ring-adjacent to \
                     the HIGHER-azimuth twin, edge missing"
                );
                assert!(
                    !cap.contains(&e(uhi, vlo)),
                    "{tag}: cap z={z} — upper uniform adjacent to the LOWER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
            }
            // Untwisted wall: both same-azimuth vertical edges exist.
            let (blo, bhi) = (vid(tw_lo_b), vid(tw_hi_b));
            let (tlo, thi) = (vid(tw_lo_t), vid(tw_hi_t));
            assert!(
                lat.contains(&e(blo, tlo)),
                "{tag}: lateral misses vertical edge at the lower twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                lat.contains(&e(bhi, thi)),
                "{tag}: lateral misses vertical edge at the higher twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                counts.values().all(|&c| c == 2),
                "{tag}: mesh must stay a closed 2-manifold"
            );
            let mut tris: Vec<[[u64; 3]; 3]> = t
                .tris
                .iter()
                .map(|tri| {
                    let mut ps: [[u64; 3]; 3] = [[0; 3]; 3];
                    for (k, &v) in tri.iter().enumerate() {
                        let a = t.verts[v as usize].as_array();
                        ps[k] = [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()];
                    }
                    ps.sort();
                    ps
                })
                .collect();
            tris.sort();
            tris
        };

        // Insertion order 1: exact order (lo, hi). Insertion order 2: reversed.
        // BOTH must produce the exact ring order (the sort may not fall back
        // to insertion order on the f64 angle tie) and the same geometry.
        let t1 = run(tw_lo_b, tw_hi_b, tw_lo_t, tw_hi_t);
        let g1 = check(&t1, "insertion (lo,hi)");
        let t2 = run(tw_hi_b, tw_lo_b, tw_hi_t, tw_lo_t);
        let g2 = check(&t2, "insertion (hi,lo)");
        assert_eq!(
            g1, g2,
            "ring order must be insertion-order independent (exact, not stable-tie)"
        );
    }

    /// A rim-crossing override lies on the tessellated rim POLYGON (a CHORD
    /// between two on-circle samples), so it sits radially INSIDE the analytic
    /// circle by up to the Stage-1 chord sagitta. The override validation must
    /// ACCEPT such a point (it is the same point the cap overlay uses — snapping
    /// it to the circle would mint a T-junction), while still rejecting a point
    /// that is OUTSIDE the circle or inside by MORE than the sagitta (a genuine
    /// off-rim fault). Regression for task #21 (the `is not on the circle`
    /// rejection that masked the same-normal crossing path).
    #[test]
    fn rim_override_accepts_chord_point_rejects_off_rim() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let az = 0.3_f64; // not a uniform sample
        let (s, c) = az.sin_cos();
        // Derive a point GUARANTEED on a chord of the actual tessellated top
        // rim (circle edge 1): the midpoint of two consecutive rim samples — its
        // radial deficit equals the exact Stage-1 chord sagitta for this N.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim1: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 1, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim1.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim1.len() >= 2, "top rim must have >=2 samples");
        let (p0, p1) = (rim1[0].1.as_array(), rim1[1].1.as_array());
        let mx = 0.5 * (p0[0] + p1[0]);
        let my = 0.5 * (p0[1] + p1[1]);
        let top_chord = Point3::new(mx, my, 1.0);
        // Same (x,y) on the BOTTOM rim plane (z=0): same global azimuth + same
        // radial deficit (the cylinder is axis-aligned), so inserting on BOTH
        // rims keeps the lateral azimuth-merge balanced.
        let bot_chord = Point3::new(mx, my, 0.0);
        let single = |e: u32, p: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(e, vec![p]);
            ov
        };

        // (1) chord point (radial deficit = chord sagitta) → ACCEPTED + present.
        let mut both: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        both.insert(0, vec![bot_chord]);
        both.insert(1, vec![top_chord]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &both, None)
            .expect("a rim point on the tessellated chord must be accepted");
        assert!(
            t.verts.iter().any(|q| q.as_array() == top_chord.as_array()),
            "accepted chord point must appear in the mesh"
        );

        // (2) far INSIDE the circle (deficit 0.1 ≫ sagitta) → loud reject
        // (the off-rim validation fires before the lateral merge).
        let too_deep = Point3::new((r - 0.1) * c, (r - 0.1) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, too_deep),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point far inside the rim circle must be rejected (off-rim fault)"
        );

        // (3) OUTSIDE the circle → loud reject.
        let outside = Point3::new((r + 0.01) * c, (r + 0.01) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, outside),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point outside the rim circle must be rejected"
        );
    }

    // ── M8-intra: exactly-negated intra-solid coplanar exclusion ────────────
    // Spec `specs/m8_intra_opposite_plane_canonicalization.md` (FIP Phase 2,
    // RED). `scan_near_coplanar` is `pub(crate)`, so these unit tests reach it
    // directly.

    /// A minimal planar `BRepFace` with a valid CCW square loop in one plane,
    /// so `BRep::new`'s Stage-1 tessellation accepts it while `scan` reads the
    /// DECLARED `(normal, d)`.
    fn m8_intra_square_a() -> BRep {
        // Two coplanar squares (z = 3) with EXACTLY-negated plane values — a
        // stepped solid's shared plane carrying opposite outward normals. The
        // negation is value-exact AND exercises 0.0 == -0.0 in the normal's x/y
        // components (spec B6 / §6): F0 = ((0.0, 0.0, 1.0), -3.0),
        // F1 = ((-0.0, -0.0, -1.0), 3.0).
        let verts = vec![
            // F0 corners (CCW viewed from +z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
            // F1 corners (same coords; wound CCW viewed from −z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 3),
            seg(3, 0), // F0 (+z winding)
            seg(4, 7),
            seg(7, 6),
            seg(6, 5),
            seg(5, 4), // F1 (−z winding)
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -3.0,
                },
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-0.0, -0.0, -1.0),
                    d: 3.0,
                },
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("intra-A BRep::new")
    }

    /// Solid B: a single tilted triangle whose AABB overlaps solid A's face
    /// region (x,y ∈ [0.5,1.5], z ∈ [2.5,3.5]) but shares NO plane with A — the
    /// "other operand reaches the shared-plane region" contact condition the
    /// intra gate keys on.
    fn m8_intra_overlapping_b() -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(0.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.0, 1.5, 3.5),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
        // Tilted plane normal = (v1−v0)×(v2−v0), un-normalized is fine (scan
        // normalizes); it is not parallel to z, so no coplanar cross pair.
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 1.0),
                d: -2.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("intra-B BRep::new")
    }

    /// Spec B6 (RED): an intra-solid pair on EXACTLY-negated planes (two
    /// orientations of ONE plane) is benign and must NOT flag the intra gate,
    /// even though the other solid overlaps the region.
    ///
    /// RED today: the two faces' raw bits differ (n vs −n, d vs −d, and
    /// 0.0 vs −0.0), so the bit-identity exclusion does not fire and the
    /// near-coplanar band flags them → `scan.intra == Some(..)`.
    #[test]
    fn intra_exactly_negated_pair_is_excluded() {
        let a = m8_intra_square_a();
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "exactly-negated intra pair must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Spec B7 (guard): a near-but-NOT-exactly-negated intra pair (one normal
    /// component drifted 1 ULP from exact negation) is the loud residue and
    /// MUST still flag. Passes today; pins that the B6 exclusion is exact-only.
    #[test]
    fn intra_one_ulp_off_negation_still_walls_guard() {
        let mut a = m8_intra_square_a();
        // Drift F1's z-normal component 1 ULP off exact negation.
        {
            let faces = a.faces();
            let Surface::Plane { normal, d } = faces[1].surface else {
                panic!("F1 not planar");
            };
            let n = normal.as_array();
            let drifted = f64::from_bits(n[2].to_bits().wrapping_add(1));
            // Rebuild A with the drifted F1 normal (BRep faces are not mutable
            // in place through the accessor).
            let verts = a.vertices().to_vec();
            let edges = a.edges().to_vec();
            let mut new_faces = a.faces().to_vec();
            new_faces[1].surface = Surface::Plane {
                normal: Vector3::new(n[0], n[1], drifted),
                d,
            };
            a = BRep::new(verts, edges, new_faces).expect("drifted intra-A BRep::new");
        }
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "a 1-ULP-off (not exactly negated) intra pair must still wall loud (B7)"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the exactly-negated intra exclusion in `scan_near_coplanar`.
    // Appended here (not in a new `tests/` file) because `scan_near_coplanar`
    // is `pub(crate)`. Purely additive; touches no existing test. Reuses the
    // `m8_intra_square_a` / `m8_intra_overlapping_b` helpers above.

    /// Rebuild solid A with a chosen F1 (upper-plane) normal/offset so an attack
    /// can inject exact bit patterns the accessor cannot mutate in place.
    fn m8_intra_a_with_f1(normal: Vector3, d: f64) -> BRep {
        let a = m8_intra_square_a();
        let verts = a.vertices().to_vec();
        let edges = a.edges().to_vec();
        let mut faces = a.faces().to_vec();
        faces[1].surface = Surface::Plane { normal, d };
        BRep::new(verts, edges, faces).expect("rebuilt intra-A")
    }

    /// FINDING (test strength). Spec §6 / B6 claim the exclusion uses f64 VALUE
    /// equality "so `0.0 == -0.0` matches — bit compare would not". The existing
    /// `intra_exactly_negated_pair_is_excluded` fixture puts −0.0 on F1's x/y,
    /// but for a −0.0 vs 0.0 pairing a *sign-flip-bit* compare
    /// (`a.to_bits() == b.to_bits() ^ SIGN`) gives the SAME answer as the value
    /// compare — so that test does NOT actually distinguish value from bit and
    /// SURVIVES the sign-flip-bit mutation. This fixture uses +0.0 on BOTH
    /// faces' x/y (0.0 vs 0.0), where value-negation still holds (0.0 == −0.0)
    /// but sign-flip-bit does NOT — a producer that emits +0.0 on both
    /// orientations (a hand-built / file-loaded solid that never ran
    /// `canonicalize_sibling_planes`) is a real input. This is the case that
    /// genuinely KILLS a bit-compare mutation.
    #[test]
    fn adversary_both_positive_zero_negation_excluded() {
        // F0 = ((0,0,1), −3); F1 = ((+0,+0,−1), +3): value-exact negation with
        // +0.0 (NOT −0.0) in x/y on BOTH faces.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -1.0), 3.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "value-exact negation with +0.0/+0.0 must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack 5 (non-unit normals). Two faces on ONE geometric plane whose raw
    /// stored normals differ in magnitude (n vs −2n) are NOT exact value
    /// negations, so the B6 exclusion must NOT fire; the pair then normalizes to
    /// parallel-opposite-coplanar and — since B reaches the region — walls LOUD.
    /// The documented conservative residue; nothing crashes.
    #[test]
    fn adversary_nonunit_opposite_normals_still_wall() {
        // F1 = ((0,0,−2), 6): plane −2z + 6 = 0 ⇒ z = 3, opposite orientation of
        // F0's z = 3 plane, but stored non-unit.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -2.0), 6.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "non-unit opposite normals must not be excluded (conservative residue)"
        );
    }

    /// Attack 4 (plane through the origin). Both faces carry d = 0.0 and a zero
    /// x/y normal component; F1's normal is the value-negation of F0's. The
    /// value compare (0.0 == −0.0, and 0.0 == −0.0 on d) excludes it.
    #[test]
    fn adversary_plane_through_origin_negation_excluded() {
        // Move both squares to z = 0 so d = 0 on both faces, then negate F1.
        let mut a = m8_intra_square_a();
        {
            let mut verts = a.vertices().to_vec();
            for v in verts.iter_mut() {
                v.point = Point3::new(v.point.x(), v.point.y(), 0.0);
            }
            let edges = a.edges().to_vec();
            let mut faces = a.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: -0.0,
            };
            a = BRep::new(verts, edges, faces).expect("origin-plane intra-A");
        }
        // B straddles z = 0 so its AABB overlaps the shared plane region.
        let b = {
            let verts = vec![
                BRepVertex {
                    point: Point3::new(0.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.0, 1.5, 0.5),
                },
            ];
            let seg = |s: u32, e: u32| BRepEdge {
                start: s,
                end: e,
                curve: Curve::LineSegment,
            };
            let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
            let faces = vec![BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, -1.0, 1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }];
            BRep::new(verts, edges, faces).expect("origin-plane B")
        };
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "through-origin value-negation (d = 0.0/−0.0) must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack (asymmetry). The B6 exclusion is orientation-blind to which face
    /// is listed first: swapping F0/F1 (rep negated first) is still excluded.
    #[test]
    fn adversary_negation_exclusion_is_symmetric() {
        // A with F0 negated instead of F1: F0 = ((−0,−0,−1), 3), F1 = ((0,0,1), −3).
        let a = {
            let base = m8_intra_square_a();
            let verts = base.vertices().to_vec();
            let edges = base.edges().to_vec();
            let mut faces = base.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: 3.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -3.0,
            };
            BRep::new(verts, edges, faces).expect("swapped intra-A")
        };
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "negation exclusion must be symmetric in face order, got {:?}",
            scan.intra
        );
    }
}

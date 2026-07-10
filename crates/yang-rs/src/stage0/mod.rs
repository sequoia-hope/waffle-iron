//! Stage 0 — §4.5.5 coplanar preprocessing (PR-YR26, M8 slice b).
//!
//! Wires the PR-YR25 exact 2D overlay engine ([`crate::coplanar_overlay`])
//! into the pipeline: where the PR-YR24 gate returned
//! `CoplanarFacesUnsupported` for a near-coplanar planar A×B face pair,
//! [`stage0_preprocess`] now HANDLES the pair per Yang 2025 §4.5.5
//! (`refs/text/yang2025_hybrid_boolean.txt:717-731`, Fig. 16 at `:752-760`):
//!
//! 1. **Canonical shared plane + snap.** One canonical plane per pair —
//!    face A's `Surface::Plane`, unit-normalized. Both faces' loop vertices
//!    are projected onto it (`p − (n̂·p + d̂)·n̂`). This is the §4.5.5
//!    "trimmed common planar surface": THE place femto-scale residuals
//!    (the YR24 R0029 class, ~1e-13 plane offsets) are reconciled
//!    symbolically. For bit-exact coplanar input the projection is the
//!    identity (the residual `t` is exactly 0). Vertices of the two solids
//!    that land on the SAME in-plane coordinates are cross-welded (B's
//!    vertex takes A's coordinates) so the overlap meshes can be
//!    bit-identical.
//! 2. **Exact 2D overlay.** Both faces are projected into ONE shared 2D
//!    frame (`ortho_basis` of the canonical normal) and segmented by
//!    [`coplanar_overlay`] into A-only / B-only / Overlap regions ("Two
//!    coplanar planes will be segmented into three parts after a Boolean
//!    operation in 2D").
//! 3. **Identical overlap meshes.** Face A is re-tessellated with the
//!    AOnly + Overlap triangles, face B with BOnly + Overlap; the Overlap
//!    triangles use the SAME f64 vertices and connectivity in both solids
//!    ("identical meshes are generated for both models in this part"),
//!    wound per each solid's outward normal (opposite-normal pairs swap).
//!    Downstream, cherchi prep welds the exact duplicates into multi-label
//!    `{A,B}` arrangement triangles, and the overlap boundaries surface as
//!    intersection curves ("The boundaries of the common surface are
//!    regarded as intersection curves between the two models").
//! 4. **Shared boundary sampling.** Overlay vertices that subdivide a
//!    face's boundary edges are propagated into every OTHER face of the
//!    same solid using that edge, which is re-triangulated with the
//!    subdivided ring ("The common part and the other two parts share
//!    identical sampling points on their boundaries" — Fig. 16 caption).
//!    Without this the Stage-1 mesh would carry T-junctions and the boolean
//!    output could not be watertight.
//!
//! ## Scope (unsupported residue keeps the loud YR24 error)
//!
//! Handled: A×B pairs of PLANAR faces with all-`LineSegment` loops, each
//! face in at most ONE pair. Everything else stays
//! `YangError::CoplanarFacesUnsupported`: intra-solid near pairs (the
//! chained-output class — with only A×B pairs overlaid, intra-solid
//! near-coplanarity has no Stage-0 resolution and would still build femto
//! sliver patches), curved faces, faces in multiple pairs, and overlay
//! engine failures (e.g. `RoundingCollapse` on sub-ulp in-plane slivers).
//!
//! ## What the caller gets
//!
//! [`Stage0`]: the re-tessellated Stage-1 meshes for both solids (internal
//! to `boolean()` — the input B-Reps and their `TessellationMap`s are NOT
//! mutated) plus one [`PairPlane`] per detected pair carrying the canonical
//! plane and the normal-agreement flag, which `boolean()` uses to resolve
//! multi-label overlap triangles after `keep_set` (see the result-boundary
//! rule at the `boolean()` call site).

use std::collections::BTreeMap;
mod disc_pair;
#[allow(unused_imports)]
pub(crate) use disc_pair::*;
mod mesh_build;
#[allow(unused_imports)]
pub(crate) use mesh_build::*;
mod cylinder;
#[allow(unused_imports)]
pub(crate) use cylinder::*;

use cad_primitives::Point3;
use dashu::rational::RBig;

use crate::coplanar_overlay::{
    coplanar_overlay, cross_r, rat, ClassifiedOverlay, ExactPoint2, PolygonWithHoles, RegionClass,
};
use crate::{
    normalize3, ortho_basis, scan_near_coplanar, stage1_tessellate,
    stage1_tessellate_with_rim_overrides, BRep, BRepEdge, BRepVertex, Curve, InputId, Mesh,
    Surface, YangError,
};
use cad_primitives::Point2;

// ════════════════════════════════════════════════════════════════════════
// public (crate) surface
// ════════════════════════════════════════════════════════════════════════

/// One handled near-coplanar pair's canonical plane, for the post-`keep_set`
/// multi-label (overlap "membrane") resolution in `boolean()`.
pub(crate) struct PairPlane {
    /// Unit canonical normal (face A's outward direction).
    pub(crate) n: [f64; 3],
    /// Unit-normal plane offset (`n·x + d = 0`).
    pub(crate) d: f64,
    /// The pair's YR24 detection band (sub-model-resolution).
    pub(crate) band: f64,
    /// Input A's face index of the pair (PR-YR27 Finding 2: Stage-6
    /// membership for THIS face is measured against the canonical pair
    /// plane — the face's mesh was snapped onto it, so its STORED plane
    /// can be up to `band` away, far beyond `TAU_WORK`).
    pub(crate) face_a: usize,
    /// Input B's face index of the pair (same keyed-membership role).
    pub(crate) face_b: usize,
    /// `true` iff face B's outward normal OPPOSES face A's (stacked
    /// configuration: the solids lie on opposite sides of the shared
    /// plane). `false` = equal normals (flush/pocket: both interiors on
    /// the same side).
    pub(crate) opposite: bool,
}

/// One coincident-CYLINDER A×B face pair, for the post-`keep_set` multi-label
/// overlap ("membrane") resolution in `boolean()`. The analog of [`PairPlane`]
/// for two faces that share the SAME cylindrical surface (a coaxial flange
/// outer wall coincident with a gear's central bore — the `err.waffle` case).
///
/// cherchi (coplanar PRs 1-4) constructs the coincident-cylinder overlap region
/// with a MULTI-SOLID label, exactly as for a coplanar planar overlap; but
/// Stage-0's planar scan ([`scan_near_coplanar`]) only records `Surface::Plane`
/// pairs, so a coincident-cylinder sheet has no matching [`PairPlane`] and was
/// dropped with `FaceResolutionFailed`. This parallel detector supplies the
/// keep/drop decision for those sheets the SAME way the planar path does.
pub(crate) struct PairCylinder {
    /// A point on the shared axis (input A's `axis_point`).
    pub(crate) axis_point: [f64; 3],
    /// Unit axis direction (input A's `axis_dir`, normalized).
    pub(crate) axis_dir: [f64; 3],
    /// The shared cylinder radius.
    pub(crate) radius: f64,
    /// The pair's scale-relative detection band (mirrors the planar
    /// `near_coplanar_band`: sub-model-resolution, NOT absolute `TAU_WORK`, so
    /// it works at mm model scale — the banked bearing-recess lesson).
    pub(crate) band: f64,
    /// `true` iff the two cylinder faces' EFFECTIVE outward normals OPPOSE on
    /// the shared surface (one an inner/bore wall pointing toward the axis, the
    /// other an outer wall pointing away). Derived from the faces' `reversed`
    /// flags exactly as the planar pair derives `opposite` from its normals:
    /// both faces share the same analytic outward direction (radially away from
    /// the axis), so they oppose iff exactly one is `reversed`.
    pub(crate) opposite: bool,
}

/// Output of Stage-0 coplanar preprocessing.
pub(crate) struct Stage0 {
    pub(crate) mesh_a: Mesh,
    pub(crate) mesh_b: Mesh,
    pub(crate) pairs: Vec<PairPlane>,
    /// N4 (2a): per-triangle → owning-face map for the RE-TESSELLATED `mesh_a` /
    /// `mesh_b` (1:1 with their `tris`), so `boolean()` Stage-6 can attribute
    /// coplanar-overlap triangles by provenance (cherchi `source` → face) rather
    /// than geometric proximity. Emitted by BOTH the planar overlay (2a) and the
    /// coincident-cylinder membrane path. May carry the `u32::MAX` sentinel for a
    /// triangle the producer could not attribute (→ geometric fallback). EMPTY
    /// only for a producer that emits none (lineage-less / sidecar).
    pub(crate) tri_face_a: Vec<u32>,
    pub(crate) tri_face_b: Vec<u32>,
}

/// Diagnostic probe for M8 residue-distribution surveys: tags which
/// unsupported-residue sub-class fired (`intra-solid` / `multi-pair` /
/// `face-unsupported` / `polygon2d-*` / `overlay-failed` / `build-mesh-*`).
/// Env-gated (`YANG_COPLANAR_PROBE=1`), zero-cost when unset; used by the
/// corpus-survey workflow to size the remaining M8 sub-classes.
fn probe(tag: &str, detail: &str) {
    if std::env::var_os("YANG_COPLANAR_PROBE").is_some() {
        eprintln!("[stage0-probe] {tag} | {detail}");
    }
}

/// Run §4.5.5 Stage-0 coplanar preprocessing. `Ok(None)` = no near-coplanar
/// cross pairs (the caller uses the B-Reps' own Stage-1 meshes, byte-for-
/// byte the pre-YR26 path). `Ok(Some(_))` = handled. `Err` = unsupported
/// residue (loud typed YR24 wall).
pub(crate) fn stage0_preprocess(a: &BRep, b: &BRep) -> Result<Option<Stage0>, YangError> {
    let scan = scan_near_coplanar(a, b);

    // Intra-solid near pairs stay the loud unsupported residue (see module
    // docs — the chained-output class has no A×B overlay resolution).
    if let Some((input, i, j)) = scan.intra {
        let brep = if input == InputId::A { a } else { b };
        probe(
            "intra-solid",
            &format!(
                "input={input:?} faces=({i},{j}) si={:?} sj={:?}",
                brep.faces()[i].surface,
                brep.faces()[j].surface
            ),
        );
        return Err(YangError::CoplanarFacesUnsupported {
            input_a: input,
            face_a: i,
            input_b: input,
            face_b: j,
        });
    }
    if scan.cross.is_empty() {
        return Ok(None);
    }

    // ── Scope validation ────────────────────────────────────────────────
    let pair_err = |face_a: usize, face_b: usize| YangError::CoplanarFacesUnsupported {
        input_a: InputId::A,
        face_a,
        input_b: InputId::B,
        face_b,
    };
    let mut count_a = vec![0usize; a.faces().len()];
    let mut count_b = vec![0usize; b.faces().len()];
    for p in &scan.cross {
        count_a[p.face_a] += 1;
        count_b[p.face_b] += 1;
    }
    for p in &scan.cross {
        // A face in MORE than one pair would need an n-ary overlay —
        // unsupported residue.
        if count_a[p.face_a] > 1 || count_b[p.face_b] > 1 {
            probe(
                "multi-pair",
                &format!(
                    "pair=({},{}) count_a={} count_b={} total_pairs={}",
                    p.face_a,
                    p.face_b,
                    count_a[p.face_a],
                    count_b[p.face_b],
                    scan.cross.len()
                ),
            );
            return Err(pair_err(p.face_a, p.face_b));
        }
        for (brep, fi) in [(a, p.face_a), (b, p.face_b)] {
            if !overlay_face_supported(brep, fi) {
                let f = &brep.faces()[fi];
                let planar = matches!(f.surface, Surface::Plane { .. });
                probe(
                    "face-unsupported",
                    &format!(
                        "pair=({},{}) face={fi} planar={planar} bad[{}] A[{}] B[{}]",
                        p.face_a,
                        p.face_b,
                        face_curve_histogram(brep, fi),
                        face_curve_histogram(a, p.face_a),
                        face_curve_histogram(b, p.face_b),
                    ),
                );
                return Err(pair_err(p.face_a, p.face_b));
            }
        }
    }

    // ── Snap phase (canonical plane per pair, deterministic pair order) ─
    let mut va: Vec<Point3> = a.vertices().iter().map(|v| v.point).collect();
    let mut vb: Vec<Point3> = b.vertices().iter().map(|v| v.point).collect();
    let mut frames: Vec<Frame> = Vec::with_capacity(scan.cross.len());
    for p in &scan.cross {
        let frame = canonical_frame(a, p.face_a).ok_or_else(|| {
            probe(
                "frame-degenerate",
                &format!("pair=({},{})", p.face_a, p.face_b),
            );
            pair_err(p.face_a, p.face_b)
        })?;
        for vi in face_loop_verts(a, p.face_a) {
            va[vi as usize] = frame.snap(va[vi as usize]);
        }
        for vi in face_loop_verts(b, p.face_b) {
            vb[vi as usize] = frame.snap(vb[vi as usize]);
        }
        // Cross-weld: a B loop vertex landing on the SAME in-plane (u,v)
        // as an A loop vertex takes A's coordinates — the §4.5.5 symbolic
        // reconciliation that makes shared corners bit-identical across the
        // two solids (e.g. the stacked-box corners 1e-13 apart pre-snap).
        let key = |p: Point3, f: &Frame| {
            let (u, v) = f.project(p);
            (u.to_bits(), v.to_bits())
        };
        let a_keys: BTreeMap<(u64, u64), u32> = face_loop_verts(a, p.face_a)
            .into_iter()
            .map(|vi| (key(va[vi as usize], &frame), vi))
            .collect();
        for vi in face_loop_verts(b, p.face_b) {
            if let Some(&ai) = a_keys.get(&key(vb[vi as usize], &frame)) {
                vb[vi as usize] = va[ai as usize];
            }
        }
        frames.push(frame);
    }

    // ── Overlay phase ───────────────────────────────────────────────────
    let mut overrides_a: BTreeMap<usize, Vec<[Point3; 3]>> = BTreeMap::new();
    let mut overrides_b: BTreeMap<usize, Vec<[Point3; 3]>> = BTreeMap::new();
    let mut splits_a: SplitMap = BTreeMap::new();
    let mut splits_b: SplitMap = BTreeMap::new();
    let mut rim_overrides_a: RimSplitMap = BTreeMap::new();
    let mut rim_overrides_b: RimSplitMap = BTreeMap::new();
    let mut pairs: Vec<PairPlane> = Vec::with_capacity(scan.cross.len());

    for (p, frame) in scan.cross.iter().zip(&frames) {
        // Normal agreement: face B's outward vs the canonical normal.
        let nb = match b.faces()[p.face_b].surface {
            Surface::Plane { normal, .. } => normalize3(normal.as_array()),
            _ => unreachable!("validated planar above"),
        };
        let opposite = frame.n[0] * nb[0] + frame.n[1] * nb[1] + frame.n[2] * nb[2] < 0.0;
        pairs.push(PairPlane {
            n: frame.n,
            d: frame.d,
            band: p.band,
            face_a: p.face_a,
            face_b: p.face_b,
            opposite,
        });

        // PR-M8-disc (increment 1): a pair where ONE face is a flat circular
        // disc and the other a convex polygon, in pure CONTAINMENT, is built
        // DIRECTLY (not through the sweep overlay, which re-subdivides the
        // disc rim and would break conformality with the cylinder lateral
        // that shares it). The disc keeps its exact Stage-1 rim ring; the
        // overlap is a shared rim/boundary triangulation and the remainder an
        // angular-merge annulus. Crossing / non-convex / disc∩disc stay the
        // loud residue.
        // An ANNULAR face in the pair is NOT eligible for the direct disc-pair
        // builder (it segments a hole-free disc); it must go through the general
        // `PolygonWithHoles` overlay below. So the disc fast-path applies only
        // when NEITHER face is annular (M8 holed-disc, spec
        // `m8_holed_disc_coplanar_overlay`).
        // M8 holed-disc increment boundary (spec
        // `m8_holed_disc_coplanar_overlay`; pinned by
        // `annular_cap_hole_crossing_stays_loud`): a partner DISC rim that
        // CROSSES a hole rim of an annular face needs arc∩arc crossing +
        // hole-rim split propagation into the bore lateral — out of scope.
        // Without this wall the general overlay emits non-conformal doubled
        // sheets whose NonManifold symptoms surface only downstream (and any
        // downstream repair machinery risks converting them into silent
        // geometry). Two coplanar circles (both in the pair plane) cross iff
        // |r1 − r2| < d(centers) < r1 + r2 strictly.
        {
            let rim_circle = |brep: &BRep, fi: usize, e: u32| -> Option<([f64; 3], f64)> {
                match brep.edges()[e as usize].curve {
                    Curve::Circle { center, radius, .. } => {
                        let _ = fi;
                        Some((center.as_array(), radius))
                    }
                    _ => None,
                }
            };
            let wall_on_hole_crossing =
                |ann: &BRep, ann_fi: usize, disc: &BRep, disc_fi: usize| -> bool {
                    let Some((_, holes)) = annular_disc_face(ann, ann_fi) else {
                        return false;
                    };
                    let Some(rim_e) = disc_circle_edge(disc, disc_fi) else {
                        return false;
                    };
                    let Some((rc, rr)) = rim_circle(disc, disc_fi, rim_e) else {
                        return false;
                    };
                    for &he in &holes {
                        let Some((hc, hr)) = rim_circle(ann, ann_fi, he) else {
                            continue;
                        };
                        let d = ((rc[0] - hc[0]).powi(2)
                            + (rc[1] - hc[1]).powi(2)
                            + (rc[2] - hc[2]).powi(2))
                        .sqrt();
                        if (rr - hr).abs() < d && d < rr + hr {
                            return true;
                        }
                    }
                    false
                };
            if wall_on_hole_crossing(a, p.face_a, b, p.face_b)
                || wall_on_hole_crossing(b, p.face_b, a, p.face_a)
            {
                probe(
                    "annular-hole-rim-crossing",
                    &format!("pair=({},{})", p.face_a, p.face_b),
                );
                return Err(pair_err(p.face_a, p.face_b));
            }
        }
        // A MIXED Line+Arc face is NOT eligible for the direct disc-pair
        // builder: `build_disc_pair` rings the partner via `loop_vertex_ring`,
        // which silently replaces arc edges by their chords (sagitta-wrong
        // geometry in a convex-chord containment). Mixed partners route
        // through the general overlay (spec `m8_mixed_loop_coplanar_overlay`
        // §6, I4).
        let disc_pair = (disc_circle_edge(a, p.face_a).is_some()
            || disc_circle_edge(b, p.face_b).is_some())
            && annular_disc_face(a, p.face_a).is_none()
            && annular_disc_face(b, p.face_b).is_none()
            && !mixed_planar_face(a, p.face_a)
            && !mixed_planar_face(b, p.face_b);
        if disc_pair {
            match build_disc_pair(a, b, p.face_a, p.face_b, &va, &vb, frame, opposite) {
                DiscPair::Handled { tris_a, tris_b } => {
                    overrides_a.insert(p.face_a, tris_a);
                    overrides_b.insert(p.face_b, tris_b);
                    continue;
                }
                DiscPair::Empty => continue,
                // A non-convex polygon partner OR a disc∩polygon CROSSING falls
                // through to the GENERAL overlay path (disc-aware via the
                // tessellated builder), which segments the overlap exactly and
                // — for a crossing — propagates the rim split into the cylinder
                // lateral + opposite cap (`collect_rim_crossings`, PR-M8
                // disc-rim crossing). disc∩disc crossing stays walled here.
                // Any other wall stays the loud residue.
                DiscPair::Wall("disc-poly-nonconvex") | DiscPair::Wall("disc-crossing") => {}
                DiscPair::Wall(tag) => {
                    probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                    return Err(pair_err(p.face_a, p.face_b));
                }
            }
        }

        // Shared-frame 2D polygons (corner→vertex keys, plus rim→3D for a
        // tessellated disc face; curved sub-chord masks for a MIXED face).
        let (mut poly_a, corners_a, rim_a, curved_masks_a) =
            face_polygon_2d_tessellated(a, p.face_a, &va, frame).ok_or_else(|| {
                probe("polygon2d-a", &format!("pair=({},{})", p.face_a, p.face_b));
                pair_err(p.face_a, p.face_b)
            })?;
        let (mut poly_b, corners_b, rim_b, curved_masks_b) =
            face_polygon_2d_tessellated(b, p.face_b, &vb, frame).ok_or_else(|| {
                probe("polygon2d-b", &format!("pair=({},{})", p.face_a, p.face_b));
                pair_err(p.face_a, p.face_b)
            })?;

        // §2b in-frame coordinate clustering (spec `m8_shared_boundary_identity`
        // C1-C3/I7-I8): the f64 frame projection mints femto-split coordinates
        // for OBLIQUE solids (intended-frame-vertical edges land ~1e-16 off
        // vertical even when the world coordinates are consistent), and the
        // exact overlay faithfully builds femto sweep slabs → needle cells →
        // `RoundingCollapse` / femto-twin split points from them. Cluster the
        // projected u and v values of BOTH polygons within the pair band so
        // intended-equal frame coordinates are BIT-equal across the pair. The
        // corner/rim key maps were built from the pre-cluster coordinates —
        // remap their keys through the same snap so overlay-vertex → 3D corner
        // resolution stays exact (T-junction-free with the neighbor faces).
        // §2c rim-aware clustering (spec `m8_shared_boundary_identity` §2c,
        // C4a-C4d/I9): rim-carrying pairs ALSO cluster now, but the cluster
        // DOMAIN is the polygon-chain coordinates only — rim sample coordinates
        // are EXCLUDED entirely (neither members nor seeds). This lifts the §2b
        // pure-polygon scope limit while structurally avoiding both §2b-reverted
        // failure modes: a disc rim's 2D samples are projections of exact 3D
        // rim-ring points bit-shared with the cylinder lateral (welding them
        // broke the rim-chord ↔ lateral correspondence — m8_disc_coplanar
        // cylinder_cap_crossing LabelMismatch), and snapping polygon corners ONTO
        // rims broke the disc-pair machinery's exact expectations (3 disc
        // fixtures). Excluding the rim domain does neither: rim samples stay
        // byte-identical (C4b) and a polygon coord femto-near a rim only is left
        // untouched (C4c). The rim sample 2D coords are the disc face's exact
        // Stage-1 rim-ring projections (= `rim_a`/`rim_b` map keys, which for a
        // disc face are exactly its `poly.outer`); for a pure-polygon pair both
        // are empty and the pass is byte-identical to §2b (C4d).
        let rim_pts_a: Vec<Point2> = rim_a
            .keys()
            .map(|ex| Point2::new(ex.x.to_f64().value(), ex.y.to_f64().value()))
            .collect();
        let rim_pts_b: Vec<Point2> = rim_b
            .keys()
            .map(|ex| Point2::new(ex.x.to_f64().value(), ex.y.to_f64().value()))
            .collect();
        let (corners_a, corners_b, rim_a, rim_b, cluster_map) = {
            let pre_a = poly_a.clone();
            let pre_b = poly_b.clone();
            cluster_frame_coords_rim_aware(
                &mut [&mut poly_a, &mut poly_b],
                &[rim_pts_a.as_slice(), rim_pts_b.as_slice()],
                p.band,
            );
            if std::env::var_os("YANG_CLUSTER_PROBE").is_some() {
                for (tag, pre, post) in [("A", &pre_a, &poly_a), ("B", &pre_b, &poly_b)] {
                    for (lp_pre, lp_post) in std::iter::once(&pre.outer)
                        .chain(pre.holes.iter())
                        .zip(std::iter::once(&post.outer).chain(post.holes.iter()))
                    {
                        for (i, (q0, q1)) in lp_pre.iter().zip(lp_post.iter()).enumerate() {
                            if q0 != q1 {
                                eprintln!(
                                    "[cluster-probe] moved {tag}[{i}]: ({:?},{:?}) → ({:?},{:?})",
                                    q0.x(),
                                    q0.y(),
                                    q1.x(),
                                    q1.y()
                                );
                            }
                        }
                    }
                }
            }
            let mut key_map: BTreeMap<(u64, u64), (u64, u64)> = BTreeMap::new();
            for (pre, post) in [(&pre_a, &poly_a), (&pre_b, &poly_b)] {
                for (lp_pre, lp_post) in std::iter::once(&pre.outer)
                    .chain(pre.holes.iter())
                    .zip(std::iter::once(&post.outer).chain(post.holes.iter()))
                {
                    for (q_pre, q_post) in lp_pre.iter().zip(lp_post.iter()) {
                        key_map.insert(
                            (q_pre.x().to_bits(), q_pre.y().to_bits()),
                            (q_post.x().to_bits(), q_post.y().to_bits()),
                        );
                    }
                }
            }
            let remap_exact = |ex: ExactPoint2| -> ExactPoint2 {
                let ux = ex.x.to_f64().value();
                let vy = ex.y.to_f64().value();
                match key_map.get(&(ux.to_bits(), vy.to_bits())) {
                    Some(&(nx, ny)) => {
                        ExactPoint2::from_f64(f64::from_bits(nx), f64::from_bits(ny)).unwrap_or(ex)
                    }
                    None => ex,
                }
            };
            let n_ca = corners_a.len();
            let n_cb = corners_b.len();
            let n_ra = rim_a.len();
            let n_rb = rim_b.len();
            let ca: BTreeMap<ExactPoint2, u32> = corners_a
                .into_iter()
                .map(|(k, v)| (remap_exact(k), v))
                .collect();
            let cb: BTreeMap<ExactPoint2, u32> = corners_b
                .into_iter()
                .map(|(k, v)| (remap_exact(k), v))
                .collect();
            let ra: BTreeMap<ExactPoint2, Point3> = rim_a
                .into_iter()
                .map(|(k, pt)| (remap_exact(k), pt))
                .collect();
            let rb: BTreeMap<ExactPoint2, Point3> = rim_b
                .into_iter()
                .map(|(k, pt)| (remap_exact(k), pt))
                .collect();
            if std::env::var_os("YANG_CLUSTER_PROBE").is_some()
                && (ca.len() != n_ca || cb.len() != n_cb || ra.len() != n_ra || rb.len() != n_rb)
            {
                eprintln!(
                    "[cluster-probe] KEY COLLISION pair=({},{}): corners_a {}→{} corners_b {}→{} \
                     rim_a {}→{} rim_b {}→{}",
                    p.face_a,
                    p.face_b,
                    n_ca,
                    ca.len(),
                    n_cb,
                    cb.len(),
                    n_ra,
                    ra.len(),
                    n_rb,
                    rb.len()
                );
            }
            // M-A (spec `m8_stage0_inputcheck_clean_emission` §2/E7): the
            // clustering rewrote the pair's 2D domain; every consumer that
            // re-derives 2D coordinates from `va`/`vb` must pass its raw
            // projections through this same pre→post map, or its exact
            // comparisons against overlay coordinates silently miss at every
            // moved vertex (`collect_edge_splits` dropped all boundary splits
            // on moved edges — the R0046/R0088/F0063 hole class).
            (ca, cb, ra, rb, key_map)
        };

        let mut overlay = coplanar_overlay(&poly_a, &poly_b).map_err(|e| {
            probe(
                "overlay-failed",
                &format!("pair=({},{}) err={e:?}", p.face_a, p.face_b),
            );
            if std::env::var_os("YANG_POLY_PROBE").is_some() {
                eprintln!(
                    "[poly-probe] pair=({},{}) A outer={:?} holes={} | B outer={:?} holes={}",
                    p.face_a,
                    p.face_b,
                    poly_a.outer,
                    poly_a.holes.len(),
                    poly_b.outer,
                    poly_b.holes.len()
                );
                let world: Vec<[f64; 3]> = face_loop_verts(a, p.face_a)
                    .into_iter()
                    .map(|vi| va[vi as usize].as_array())
                    .collect();
                eprintln!(
                    "[poly-probe] A face {} 3D loop verts (snapped): {world:?}",
                    p.face_a
                );
            }
            pair_err(p.face_a, p.face_b)
        })?;

        if overlay.area_exact(RegionClass::Overlap) == RBig::ZERO {
            // No positive-area overlap (an in-plane touch): the snap has
            // already reconciled the planes; both faces tessellate normally
            // and the exact arrangement passes the coplanar touch through
            // as benign (cherchi deviation N17).
            continue;
        }

        // Does the overlap boundary CROSS a disc rim (subdivide a rim
        // sub-chord)? PR-M8 disc-rim crossing handles the OPPOSITE-normal case
        // (a boss/recess whose rim crosses a coplanar polygon edge) by
        // propagating the crossing points into the cylinder lateral + opposite
        // cap (`collect_rim_crossings` below). SAME-normal crossings stay the
        // loud residue (see the SCOPE GATE below). A MIXED face's rim map
        // holds arc-chain samples, not a disc rim — its curved sub-chord
        // subdivision (partner crossings AND the overlay's own sweep-event
        // columns) propagates through `collect_mixed_crossings` instead
        // (spec `m8_mixed_loop_coplanar_overlay` amendment 1).
        let rim_cross_a =
            curved_masks_a.is_empty() && !rim_a.is_empty() && rim_subdivided(&poly_a, &overlay);
        let rim_cross_b =
            curved_masks_b.is_empty() && !rim_b.is_empty() && rim_subdivided(&poly_b, &overlay);
        let mixed_cross_a = !curved_masks_a.is_empty()
            && curved_chords_subdivided(&poly_a, &curved_masks_a, &overlay);
        let mixed_cross_b = !curved_masks_b.is_empty()
            && curved_chords_subdivided(&poly_b, &curved_masks_b, &overlay);

        // SAME-NORMAL disc∩polygon crossings now route through the SAME path as
        // opposite-normal (the M8 same-normal wall is LIFTED, 2b). Two things made
        // this safe: (1) N4 provenance attribution dissolved the Stage-6
        // `FaceResolutionFailed` mode that the wall guarded against (R0013/R0024
        // now build correctly end-to-end); (2) the remaining same-normal modes
        // (Stage-3 SSI ambiguity, Stage-4 relocation, kernel-v2 azimuth-merge,
        // residual second-pair) all fail LOUD at their own stage — P9-safe, never
        // a wrong result — so the downstream validations are the safety net, not
        // a blanket Stage-0 wall. The `YANG_M8_SAMENORMAL_DEV` env (previously the
        // dev-only lift) is now a no-op; the campaign tests still document each
        // remaining mode. The `probe` is kept for the M8 residue survey.
        if (rim_cross_a || rim_cross_b) && !opposite {
            probe(
                "disc-crossing-same-normal",
                &format!("pair=({},{})", p.face_a, p.face_b),
            );
        }

        // Tessellated rim points (f64 in-frame u,v → 3D), for the near-snap
        // below. A curved rim fed through the exact overlay can spawn a sweep
        // vertex a few ULPs off a rim point; lifting it independently would mint
        // a near-coincident-but-distinct 3D point (a degenerate sliver against
        // the cylinder lateral's exact rim → a spurious coplanar deferral). Snap
        // such a vertex to the exact rim point it is essentially on.
        let rim_pts: Vec<(f64, f64, Point3)> = rim_a
            .iter()
            .chain(rim_b.iter())
            .map(|(ex, &pt)| (ex.x.to_f64().value(), ex.y.to_f64().value(), pt))
            .collect();
        // ε ≪ any real rim spacing (chord tolerance ~1e-3·scale), ≫ the ULP gap.
        let snap_eps2 = {
            let scale = rim_pts
                .iter()
                .map(|(u, v, _)| u.abs().max(v.abs()))
                .fold(1.0_f64, f64::max);
            let e = 1.0e-9 * scale;
            e * e
        };

        // N2-3a (spec `n2_stage4_junction_cluster_merge` §3, [#24 §4.5.5]):
        // exact rim-chord resolution contexts per disc/annular face of the
        // pair — ONE per rim circle (increment 6: an annular face carries
        // outer + one per hole). The overlay's trapezoidal decomposition
        // splits every rim chord at every event x-coordinate; those split
        // vertices must be minted ON the exact rim `Curve::Circle` (I1:
        // every output loop vertex on its face's surface), not at their
        // chord positions — §4.5.5's "overlap boundaries become intersection
        // curves" carries exact curve geometry. Empty for a non-disc/
        // non-annular face (zero behavior change).
        let rim_ctxs_a = if !curved_masks_a.is_empty() {
            // M8-mixed: one ctx per curved EDGE of the mixed face — the same
            // on-circle minting, over that edge's own chord subset.
            mixed_chord_ctxs(a, &poly_a, &curved_masks_a, &poly_b, frame)
        } else if rim_a.is_empty() {
            Vec::new()
        } else {
            rim_chord_ctxs(a, p.face_a, &poly_a, &poly_b, frame)
        };
        let rim_ctxs_b = if !curved_masks_b.is_empty() {
            mixed_chord_ctxs(b, &poly_b, &curved_masks_b, &poly_a, frame)
        } else if rim_b.is_empty() {
            Vec::new()
        } else {
            rim_chord_ctxs(b, p.face_b, &poly_b, &poly_a, frame)
        };
        // Mint-collapse slot space: one slot per rim circle across the pair
        // (a shared collapse target cannot lie on two circles).
        let n_mint_slots = rim_ctxs_a.len() + rim_ctxs_b.len();

        // Resolve every overlay vertex to ONE solid-independent 3D point:
        // corner of face A → A's (snapped/welded) vertex; corner of face B
        // → B's; rim point → the exact 3D rim point; a rim-CHORD point →
        // minted on the exact rim circle (N2-3a, see below); otherwise the
        // frame lift L(u,v) (snapped to a rim point if it lands within ε of
        // one). Shared between BOTH solids' meshes so the Overlap triangles
        // are bit-identical — and consumed by `collect_rim_crossings` /
        // `collect_edge_splits` below, so cap, lateral, and opposite rim all
        // see the SAME minted point (§4.5.5 identical-mesh requirement).
        let mut coords: Vec<Point3> = Vec::with_capacity(overlay.verts.len());
        // Explicit N2-3a minted-index tracking for the fold-validity gate
        // below (spec §3 amendment 2: coordinate-comparison inference is
        // FORBIDDEN — it falsely captures ULP-snapped rim vertices).
        let mut minted_mark = vec![false; overlay.verts.len()];
        // Increment 4 (spec `m8_holed_disc_coplanar_overlay` §8): per-mint
        // record (overlay vertex, rim-ctx slot, crossing-branch flag) for the
        // sub-floor shared-mint collapse below. Slot-scoped so a collapse
        // never merges mints of two DIFFERENT rim circles (a shared target
        // cannot lie on both).
        let mut minted_info: Vec<(usize, usize, bool)> = Vec::new();
        for (i, mark) in minted_mark.iter_mut().enumerate() {
            let exact = &overlay.exact_verts[i];
            let pt = if let Some(&ai) = corners_a.get(exact) {
                va[ai as usize]
            } else if let Some(&bi) = corners_b.get(exact) {
                vb[bi as usize]
            } else if let Some(&pt) = rim_a.get(exact) {
                pt
            } else if let Some(&pt) = rim_b.get(exact) {
                pt
            } else {
                let q = overlay.verts[i];
                let (qx, qy) = (q.x(), q.y());
                if let Some(&(_, _, pt)) = rim_pts.iter().find(|(u, v, _)| {
                    let (du, dv) = (u - qx, v - qy);
                    du * du + dv * dv <= snap_eps2
                }) {
                    pt
                } else {
                    // N2-3a: a vertex the overlay minted STRICTLY INTERIOR to
                    // a disc-rim sub-chord (exact rational collinearity + the
                    // same interior-parameter predicate as
                    // `collect_rim_crossings`) lies sagitta-deep off the exact
                    // rim circle if lifted raw. Mint it on the circle instead:
                    // - on a rim chord AND transversally on another input's
                    //   edge sub-segment → the exact 2D circle∩line
                    //   intersection (I2: the junction stays on BOTH the
                    //   circle and the other input's edge — radial projection
                    //   would slide it off that edge);
                    // - on a rim chord only (an x-event subdivision) → radial
                    //   projection onto the exact circle in the cap plane
                    //   (the own-cap analog of the opposite-rim exact-radius
                    //   projection below).
                    // All other vertices fall through to the raw lift,
                    // byte-identical to the pre-N2-3a path (I4).
                    let mut minted: Option<Point3> = None;
                    for (slot, ctx) in rim_ctxs_a.iter().chain(rim_ctxs_b.iter()).enumerate() {
                        match resolve_rim_chord_vertex(ctx, exact, qx, qy, frame) {
                            RimResolve::NotOnChord => {}
                            RimResolve::OnCircle { point, crossing } => {
                                minted = Some(point);
                                minted_info.push((i, slot, crossing));
                                break;
                            }
                            RimResolve::NoIntersection => {
                                // Spec §6: the exact discriminant says the
                                // other input's edge line misses the circle —
                                // impossible for a genuine crossing (a chord-
                                // interior point is strictly inside the
                                // circle, so any line through it crosses it).
                                // A loud Stage-0 stop, never a silent fall
                                // back to the chord position.
                                probe(
                                    "rim-circle-line-no-intersection",
                                    &format!(
                                        "pair=({},{}) vert={i} uv=({qx},{qy})",
                                        p.face_a, p.face_b
                                    ),
                                );
                                return Err(pair_err(p.face_a, p.face_b));
                            }
                        }
                    }
                    *mark = minted.is_some();
                    minted.unwrap_or_else(|| frame.lift(qx, qy))
                }
            };
            coords.push(pt);
        }

        // Twin-origin probe (read-only, env-gated): `YANG_INPUT_VERT_PROBE=
        // x,y,z,r` — report every RESOLVED overlay vertex within r of the
        // target, with its resolution branch and this pair's frame, to
        // localize femto-twin mints to a pair/branch.
        if let Some(spec) = std::env::var_os("YANG_INPUT_VERT_PROBE") {
            let nums: Vec<f64> = spec
                .to_string_lossy()
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if let [x, y, z, r] = nums[..] {
                for (i, pt) in coords.iter().enumerate() {
                    let q = pt.as_array();
                    let d = [q[0] - x, q[1] - y, q[2] - z];
                    if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > r {
                        continue;
                    }
                    let exact = &overlay.exact_verts[i];
                    let branch = if corners_a.contains_key(exact) {
                        "corner_a"
                    } else if corners_b.contains_key(exact) {
                        "corner_b"
                    } else if rim_a.contains_key(exact) {
                        "rim_a"
                    } else if rim_b.contains_key(exact) {
                        "rim_b"
                    } else if minted_mark[i] {
                        "rim_mint"
                    } else {
                        "lift_or_snap"
                    };
                    eprintln!(
                        "[stage0-twin-probe] pair=({},{}) overlay_vert {i} branch={branch} \
                         pt=({},{},{}) frame.n=({},{},{}) frame.d={}",
                        p.face_a,
                        p.face_b,
                        q[0],
                        q[1],
                        q[2],
                        frame.n[0],
                        frame.n[1],
                        frame.n[2],
                        frame.d
                    );
                }
            }
        }

        // Twin-scan probe (read-only, env-gated `YANG_STAGE0_TWIN_SCAN`):
        // report every pair of RESOLVED overlay vertices with distinct exact
        // 2D coordinates whose 3D images are closer than MIN_FEATURE_SIZE —
        // the sub-floor femto-twin census for this pair's overlay.
        if std::env::var_os("YANG_STAGE0_TWIN_SCAN").is_some() {
            for i in 0..coords.len() {
                for j in (i + 1)..coords.len() {
                    let (q, w) = (coords[i].as_array(), coords[j].as_array());
                    let d = [q[0] - w[0], q[1] - w[1], q[2] - w[2]];
                    let dist2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                    if dist2 == 0.0
                        || dist2
                            >= cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
                    {
                        continue;
                    }
                    let branch = |k: usize| {
                        let exact = &overlay.exact_verts[k];
                        if corners_a.contains_key(exact) {
                            "corner_a"
                        } else if corners_b.contains_key(exact) {
                            "corner_b"
                        } else if rim_a.contains_key(exact) {
                            "rim_a"
                        } else if rim_b.contains_key(exact) {
                            "rim_b"
                        } else if minted_mark[k] {
                            "rim_mint"
                        } else {
                            "lift_or_snap"
                        }
                    };
                    eprintln!(
                        "[stage0-twin-scan] pair=({},{}) verts {i}/{j} dist={:e} \
                         branch=({},{}) pt_i=({},{},{})",
                        p.face_a,
                        p.face_b,
                        dist2.sqrt(),
                        branch(i),
                        branch(j),
                        q[0],
                        q[1],
                        q[2]
                    );
                }
            }
        }

        // ── Increment 4: sub-floor shared-mint collapse (spec
        // `m8_holed_disc_coplanar_overlay` §8, task #61; A14.2) ──────────
        // The trapezoidal overlay legitimately mints femto-twin split pairs
        // (two sweep-event columns ULPs apart in u crossing the same rim
        // chord). Resolved independently, the twins become two distinct
        // on-circle points closer than MIN_FEATURE_SIZE — BELOW the kernel's
        // supported feature floor, so they cannot be two real features. Left
        // distinct, the wedge between them folds under the gate below,
        // reverting BOTH mints to chord positions where Stage 4 cannot
        // relocate them (no conic assignment — the R0072 micro class).
        // Collapse each sub-floor group to ONE shared on-circle target: a
        // crossing-branch member if the group has one (I2 — the junction
        // stays on the other input's edge), else the first member; never an
        // average. The resulting 2D-distinct/3D-identical boundary pair is
        // the M-B emission-identification class: the degenerate wedge drops
        // at emission and neighbors' resolved edges pair directly. Groups
        // are per rim-ctx slot — one slot per rim CIRCLE across the pair
        // (a shared target cannot lie on two circles) — and isolated (real
        // crossings are ≥ MIN_FEATURE_SIZE apart), so greedy first-seen
        // grouping cannot chain-drift.
        for slot in 0..n_mint_slots {
            let members: Vec<(usize, bool)> = minted_info
                .iter()
                .filter(|&&(_, s, _)| s == slot)
                .map(|&(vi, _, crossing)| (vi, crossing))
                .collect();
            let mut groups: Vec<Vec<(usize, bool)>> = Vec::new();
            for &(vi, crossing) in &members {
                let p = coords[vi].as_array();
                let group = groups.iter_mut().find(|g| {
                    let q = coords[g[0].0].as_array();
                    let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
                        < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
                });
                match group {
                    Some(g) => g.push((vi, crossing)),
                    None => groups.push(vec![(vi, crossing)]),
                }
            }
            for g in groups.iter().filter(|g| g.len() > 1) {
                let target_vi = g.iter().find(|&&(_, c)| c).map_or(g[0].0, |&(vi, _)| vi);
                let target = coords[target_vi];
                if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                    eprintln!(
                        "[mint-collapse] slot={slot} group={:?} -> vert {target_vi} {target:?}",
                        g.iter().map(|&(vi, c)| (vi, c)).collect::<Vec<_>>(),
                    );
                }
                for &(vi, _) in g {
                    coords[vi] = target;
                }
            }
        }

        // N2-3a fold-validity gate (spec §3 amendment 2, grounded §0 item 6):
        // exact minting is only sound where it keeps the PRE-EXISTING overlay
        // triangulation valid. Where the rim tessellation is coarse, moving a
        // chord vertex outward by the local sagitta can cross other-input
        // mesh edges inside the chord↔arc band (measured: R0013's 9-gon rim,
        // sagitta 0.53 at r=8.73 → an inverted gear-side triangle → cherchi
        // self-intersection → Stage-6 patch dead-end). Revert any minted
        // vertex whose incident overlay triangle's 2D signed area goes ≤ 0
        // back to today's chord lift (byte-identical to the pre-N2-3a path),
        // iterated to a deterministic fixpoint. This is a validity check,
        // not a tolerance: reverted vertices stay observable via kernel-v2's
        // untouched vertex-on-surface tripwire (spec §6) and are the
        // recorded demand for overlay-level mesh updating (Yang Fig 11 —
        // repositioned boundary vertices need local re-triangulation).
        // Increment 4: a triangle whose RESOLVED 3D image is degenerate
        // (bit-duplicate vertices) is dropped at emission by the M-B filter
        // below — it is never part of the emitted mesh, so its 2D fold
        // state must not revert mints. Without this skip the collapsed twin
        // wedge (projected area exactly 0) would un-collapse its own shared
        // mint.
        let tri_degenerate = gate_tri_degenerate;
        let tri_area = |t: &[u32; 3], coords: &[Point3]| gate_tri_area(t, coords, frame);
        // A replacement triangle is valid under the current resolved
        // coordinates if it winds material-CCW (positive area) or its 3D
        // image is bit-degenerate (the M-B emission-drop class).
        let tri_valid = |t: &[u32; 3], coords: &[Point3]| gate_tri_valid(t, coords, frame);

        // Amendment 4 (spec `n2_stage4_junction_cluster_merge` §3, M8
        // increment 7): edge → incident-triangle map for the constrained
        // flip repair, maintained incrementally across flips. An edge is
        // flippable only if it is shared by exactly two SAME-class
        // triangles: a class-boundary edge IS the intersection curve and a
        // single-incidence edge is the domain boundary — both immovable.
        let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut edge_map: BTreeMap<[u32; 2], Vec<usize>> = BTreeMap::new();
        for (ti, t) in overlay.tris.iter().enumerate() {
            for k in 0..3 {
                edge_map
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default()
                    .push(ti);
            }
        }

        loop {
            let mut changed = false;
            for ti in 0..overlay.tris.len() {
                let t = overlay.tris[ti];
                if tri_degenerate(&t, &coords) || tri_area(&t, &coords) > 0.0 {
                    continue;
                }
                if !t.iter().any(|&v| minted_mark[v as usize]) {
                    continue;
                }

                // ── Amendment 4: constrained flip repair (Lawson; [#24 Yang
                // §4.4.1 Fig 11] — a repositioned boundary vertex demands
                // local re-triangulation). An on-circle mint whose
                // displacement dwarfs a femto-strip's width inverts the
                // strip-diagonal sliver; reverting the mint (amendment 2)
                // would leak a chord-position vertex into the output rims.
                // Repair the triangulation instead where a legal flip
                // exists; the revert below stays the fallback (R0013-class
                // folds crossing another input's edges are NOT flippable —
                // their edges are class boundaries).
                let mut flipped = false;
                let probe_flip = std::env::var_os("YANG_SPLIT_PROBE").is_some();
                for k in 0..3 {
                    let (ea, eb) = (t[k], t[(k + 1) % 3]);
                    let c = t[(k + 2) % 3];
                    let reject = |why: &str| {
                        if probe_flip {
                            eprintln!("  [flip-reject] tri {ti} edge ({ea},{eb}) {why}");
                        }
                    };
                    let Some(inc) = edge_map.get(&edge_key(ea, eb)) else {
                        continue;
                    };
                    if inc.len() != 2 {
                        reject("domain-boundary (1 incident tri)");
                        continue;
                    }
                    let tj = if inc[0] == ti { inc[1] } else { inc[0] };
                    if overlay.class[tj] != overlay.class[ti] {
                        reject("class-boundary (constraint)");
                        continue;
                    }
                    let tn = overlay.tris[tj];
                    if tri_degenerate(&tn, &coords) {
                        reject("neighbor 3D-degenerate");
                        continue;
                    }
                    let Some(d) = tn.iter().copied().find(|&v| v != ea && v != eb) else {
                        continue;
                    };
                    // The replacement diagonal must be NEW — an existing
                    // (c,d) edge would go non-manifold in 2D.
                    if edge_map.contains_key(&edge_key(c, d)) {
                        reject("diagonal exists");
                        continue;
                    }
                    // Consistent-CCW mesh: the neighbor traverses (eb, ea).
                    // Flip (ea,eb,c)+(eb,ea,d) → (ea,d,c)+(d,eb,c); both
                    // replacements must be valid for acceptance (each
                    // accepted flip strictly reduces the folded count —
                    // termination).
                    let n1 = [ea, d, c];
                    let n2 = [d, eb, c];
                    if !tri_valid(&n1, &coords) || !tri_valid(&n2, &coords) {
                        reject("replacements invalid");
                        continue;
                    }
                    for (idx, old) in [(ti, t), (tj, tn)] {
                        for k2 in 0..3 {
                            let kk = edge_key(old[k2], old[(k2 + 1) % 3]);
                            if let Some(v) = edge_map.get_mut(&kk) {
                                v.retain(|&x| x != idx);
                                if v.is_empty() {
                                    edge_map.remove(&kk);
                                }
                            }
                        }
                    }
                    overlay.tris[ti] = n1;
                    overlay.tris[tj] = n2;
                    for (idx, newt) in [(ti, n1), (tj, n2)] {
                        for k2 in 0..3 {
                            edge_map
                                .entry(edge_key(newt[k2], newt[(k2 + 1) % 3]))
                                .or_default()
                                .push(idx);
                        }
                    }
                    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                        eprintln!(
                            "[fold-flip] pair=({},{}) tri {ti}+{tj} edge ({ea},{eb}) -> \
                             diagonal ({c},{d})",
                            p.face_a, p.face_b
                        );
                    }
                    flipped = true;
                    changed = true;
                    break;
                }
                if flipped {
                    continue;
                }

                // ── Amendment 5: cavity relocation (M8 increment 8) — the
                // full [#24 Yang §4.4.1 Fig 11] delete-and-reinsert form,
                // for the rim-mint COLUMN HOP class no single flip can
                // repair (the mint's in-plane displacement crosses a
                // populated sweep-event column and the whole inter-column
                // strip folds together; the folded set's boundary is
                // non-simple under the moved vertex). The minted vertex's
                // star is carved out and re-fanned from its resolved
                // position, growing by visibility within the region class;
                // constraint edges (class/domain boundaries) are never
                // crossed — an uncarvable cavity rejects and falls through
                // to the revert below (loud, never silently blessed).
                let mut relocated = false;
                // Amendment 6: per-vertex rejections with a NON-SIMPLE
                // cavity polygon accumulate joint-relocation seeds — the
                // folded triangle's minted vertices plus the OTHER minted
                // vertices found on each non-simple ring (the interacting
                // multi-column strip class; F0087 cut 9).
                let mut joint_seeds: std::collections::BTreeSet<u32> =
                    std::collections::BTreeSet::new();
                let mut saw_nonsimple = false;
                for &vv in &t {
                    if !minted_mark[vv as usize] {
                        continue;
                    }
                    match relocate_minted_vertex(
                        &mut overlay.tris,
                        &mut overlay.class,
                        &mut edge_map,
                        vv,
                        &coords,
                        frame,
                        &minted_mark,
                        probe_flip,
                    ) {
                        RelocOutcome::Committed => {
                            if probe_flip {
                                eprintln!(
                                    "[fold-reloc] pair=({},{}) tri {ti} vert {vv} \
                                     star re-fanned at minted position",
                                    p.face_a, p.face_b
                                );
                            }
                            relocated = true;
                            changed = true;
                            break;
                        }
                        RelocOutcome::NonSimple { ring_mints } => {
                            saw_nonsimple = true;
                            joint_seeds.insert(vv);
                            joint_seeds.extend(ring_mints);
                        }
                        RelocOutcome::Rejected => {
                            joint_seeds.insert(vv);
                        }
                    }
                }
                if !relocated && saw_nonsimple && joint_seeds.len() >= 2 {
                    let seeds: Vec<u32> = joint_seeds.into_iter().collect();
                    if relocate_minted_region(
                        &mut overlay.tris,
                        &mut overlay.class,
                        &mut edge_map,
                        &seeds,
                        &coords,
                        frame,
                        probe_flip,
                    ) {
                        if probe_flip {
                            eprintln!(
                                "[fold-reloc-region] pair=({},{}) tri {ti} seeds {seeds:?} \
                                 region re-triangulated at minted positions",
                                p.face_a, p.face_b
                            );
                        }
                        relocated = true;
                        changed = true;
                    }
                }
                if relocated {
                    continue;
                }

                // ── Amendment 2 fallback: revert the fold's minted
                // vertices to today's chord lift (still observable via
                // kernel-v2's vertex-on-surface tripwire — never silently
                // blessed).
                let area = tri_area(&t, &coords);
                for &v in &t {
                    let vi = v as usize;
                    if minted_mark[vi] {
                        let q = overlay.verts[vi];
                        let lifted = frame.lift(q.x(), q.y());
                        if coords[vi] != lifted {
                            if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                                eprintln!(
                                    "[fold-revert] pair=({},{}) vert={vi} area={area:e} \
                                     minted={:?} -> chord {lifted:?}",
                                    p.face_a, p.face_b, coords[vi]
                                );
                            }
                            coords[vi] = lifted;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Per-solid override triangles. Overlay triangles are CCW in the
        // (e1, e2) frame ⇒ normal +n̂ (e1×e2 = n̂): face A keeps the order
        // (n̂ IS its outward normal); face B swaps iff its outward opposes.
        let tris_for = |keep: [RegionClass; 2], swap: bool| -> Vec<[Point3; 3]> {
            let bits = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
            overlay
                .tris
                .iter()
                .zip(&overlay.class)
                .filter(|(_, c)| keep.contains(c))
                .filter_map(|(t, _)| {
                    let mut tri = [
                        coords[t[0] as usize],
                        coords[t[1] as usize],
                        coords[t[2] as usize],
                    ];
                    // M-B (spec `m8_stage0_inputcheck_clean_emission` §2/E8):
                    // the 2D→3D resolution deliberately identifies femto-split
                    // overlay vertices to ONE exact point, so a positive-area
                    // 2D sliver can have a degenerate 3D image. Emitting it
                    // would intern to a `[u,u,v]` triangle (zero cover → holes
                    // + pinches). Drop it: the identification makes the
                    // neighbors' resolved edges pair directly. Bit-identity
                    // matches the downstream interner's key exactly.
                    let b = [bits(tri[0]), bits(tri[1]), bits(tri[2])];
                    if b[0] == b[1] || b[1] == b[2] || b[0] == b[2] {
                        return None;
                    }
                    if swap {
                        tri.swap(1, 2);
                    }
                    Some(tri)
                })
                .collect()
        };
        overrides_a.insert(
            p.face_a,
            tris_for([RegionClass::AOnly, RegionClass::Overlap], false),
        );
        overrides_b.insert(
            p.face_b,
            tris_for([RegionClass::BOnly, RegionClass::Overlap], opposite),
        );

        // §4.5.5 shared boundary sampling: overlay vertices subdividing a
        // face's boundary edges propagate to the adjacent faces. (Disc pairs
        // never reach here — they `continue` from the direct builder above.)
        collect_edge_splits(
            a,
            p.face_a,
            &va,
            frame,
            &cluster_map,
            &overlay,
            [RegionClass::AOnly, RegionClass::Overlap],
            &coords,
            &mut splits_a,
        );
        collect_edge_splits(
            b,
            p.face_b,
            &vb,
            frame,
            &cluster_map,
            &overlay,
            [RegionClass::BOnly, RegionClass::Overlap],
            &coords,
            &mut splits_b,
        );

        // PR-M8 disc-rim crossing: a disc whose rim the overlap boundary
        // crosses propagates each crossing point into its OWN cap rim AND the
        // opposite cap rim of the same cylinder (and thus the lateral, which
        // shares both rims). OPPOSITE-normal only (the SCOPE GATE above).
        if rim_cross_a {
            if let Err(tag) = collect_rim_crossings(
                a,
                p.face_a,
                &poly_a,
                &overlay,
                &coords,
                &mut rim_overrides_a,
            ) {
                probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                return Err(pair_err(p.face_a, p.face_b));
            }
        }
        if rim_cross_b {
            if let Err(tag) = collect_rim_crossings(
                b,
                p.face_b,
                &poly_b,
                &overlay,
                &coords,
                &mut rim_overrides_b,
            ) {
                probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                return Err(pair_err(p.face_a, p.face_b));
            }
        }

        // M8-mixed (spec `m8_mixed_loop_coplanar_overlay` amendment 1): a
        // mixed face whose curved sub-chords the overlay subdivided (sweep
        // events or genuine crossings) propagates each on-circle split point
        // into the curved edge's own chain AND its lateral's opposite arc /
        // rim, exactly like the disc path — the adjacent partial strip pairs
        // its two chains index-for-index, so both sides must gain the point.
        if mixed_cross_a {
            if let Err(tag) = collect_mixed_crossings(
                a,
                p.face_a,
                &poly_a,
                &curved_masks_a,
                &overlay,
                &coords,
                &mut rim_overrides_a,
            ) {
                probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                return Err(pair_err(p.face_a, p.face_b));
            }
        }
        if mixed_cross_b {
            if let Err(tag) = collect_mixed_crossings(
                b,
                p.face_b,
                &poly_b,
                &curved_masks_b,
                &overlay,
                &coords,
                &mut rim_overrides_b,
            ) {
                probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                return Err(pair_err(p.face_a, p.face_b));
            }
        }

        // M-C diagnosis (read-only observer, spec I6 pattern): with
        // `YANG_STAGE0_DUMP_DIR` set, dump this pair's classified overlay —
        // per-vertex resolution provenance + resolved 3D, per-triangle class
        // and per-side emission verdict (incl. the E8 resolved-degenerate
        // drop), and the split maps as collected so far — so census
        // offenders on the emitted operands join back to overlay entities.
        dump_pair_overlay(
            (p.face_a, p.face_b, p.band, opposite),
            &overlay,
            &corners_a,
            &corners_b,
            &rim_a,
            &rim_b,
            &rim_pts,
            snap_eps2,
            &minted_mark,
            &coords,
            frame,
            &splits_a,
            &splits_b,
            [&poly_a, &poly_b],
        );
    }

    // ── Stage-1 re-tessellation with overrides + propagated splits ──────
    let report_pair = (scan.cross[0].face_a, scan.cross[0].face_b);
    let (mesh_a, tri_face_a) = build_stage0_mesh(a, &va, &overrides_a, &splits_a, &rim_overrides_a)
        .map_err(|e| match e {
            BuildErr::Yang(y) => y,
            BuildErr::Unsupported => {
                probe("build-mesh-a", &format!("pair={report_pair:?}"));
                pair_err(report_pair.0, report_pair.1)
            }
        })?;
    let (mesh_b, tri_face_b) = build_stage0_mesh(b, &vb, &overrides_b, &splits_b, &rim_overrides_b)
        .map_err(|e| match e {
            BuildErr::Yang(y) => y,
            BuildErr::Unsupported => {
                probe("build-mesh-b", &format!("pair={report_pair:?}"));
                pair_err(report_pair.0, report_pair.1)
            }
        })?;

    Ok(Some(Stage0 {
        mesh_a,
        mesh_b,
        pairs,
        tri_face_a,
        tri_face_b,
    }))
}

// ════════════════════════════════════════════════════════════════════════
// canonical frame
// ════════════════════════════════════════════════════════════════════════

/// The pair's canonical shared plane + deterministic 2D frame: face A's
/// unit normal `n` with unit offset `d` (`n·x + d = 0`), an on-plane origin
/// `o = −d·n`, and the in-plane axes `(e1, e2) = ortho_basis(n)`
/// (right-handed: `e1 × e2 = n`).
struct Frame {
    n: [f64; 3],
    d: f64,
    o: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
}

impl Frame {
    /// Project `p` onto the canonical plane along `n` — the §4.5.5 snap.
    /// Exactly the identity for points already on the plane (`t == 0.0`).
    fn snap(&self, p: Point3) -> Point3 {
        let pa = p.as_array();
        let t = self.n[0] * pa[0] + self.n[1] * pa[1] + self.n[2] * pa[2] + self.d;
        Point3::new(
            pa[0] - t * self.n[0],
            pa[1] - t * self.n[1],
            pa[2] - t * self.n[2],
        )
    }

    /// In-plane coordinates of (the plane projection of) `p`.
    fn project(&self, p: Point3) -> (f64, f64) {
        let pa = p.as_array();
        let w = [pa[0] - self.o[0], pa[1] - self.o[1], pa[2] - self.o[2]];
        (
            w[0] * self.e1[0] + w[1] * self.e1[1] + w[2] * self.e1[2],
            w[0] * self.e2[0] + w[1] * self.e2[1] + w[2] * self.e2[2],
        )
    }

    /// The 3D lift `o + u·e1 + v·e2` — the shared coordinate of every NEW
    /// overlay vertex (computed once per overlay vertex, used by BOTH
    /// solids' meshes).
    fn lift(&self, u: f64, v: f64) -> Point3 {
        Point3::new(
            self.o[0] + u * self.e1[0] + v * self.e2[0],
            self.o[1] + u * self.e1[1] + v * self.e2[1],
            self.o[2] + u * self.e1[2] + v * self.e2[2],
        )
    }
}

/// Build the canonical frame from face A's stored plane. `None` for a
/// degenerate normal (rejected loudly by the caller as unsupported).
fn canonical_frame(a: &BRep, face_a: usize) -> Option<Frame> {
    let Surface::Plane { normal, d } = a.faces()[face_a].surface else {
        return None;
    };
    let na = normal.as_array();
    let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
    if len < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let n = [na[0] / len, na[1] / len, na[2] / len];
    let du = d / len;
    let o = [-du * n[0], -du * n[1], -du * n[2]];
    let (e1, e2) = ortho_basis(normal);
    Some(Frame {
        n,
        d: du,
        o,
        e1: e1.as_array(),
        e2: e2.as_array(),
    })
}

// ════════════════════════════════════════════════════════════════════════
// face → polygon helpers
// ════════════════════════════════════════════════════════════════════════

/// Is this face overlay-supported: a planar surface that is EITHER an
/// all-`LineSegment` polygon OR a full-circle disc (PR-M8-disc — the single
/// dominant M8 coplanar sub-class: a cylinder end-cap flush against another
/// planar face). The disc is handled by sampling its rim into the SAME ring
/// Stage 1 uses, then routing it through the existing polygon overlay.
fn overlay_face_supported(brep: &BRep, fi: usize) -> bool {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) {
        return false;
    }
    if disc_circle_edge(brep, fi).is_some() {
        return true;
    }
    // M8 holed-disc (spec `m8_holed_disc_coplanar_overlay`): a planar ANNULAR
    // face — single-circle outer loop + each inner loop a single closed circle
    // — is overlay-eligible (its outer + hole rims sample into the exact
    // `PolygonWithHoles` the overlay already consumes).
    if annular_disc_face(brep, fi).is_some() {
        return true;
    }
    // M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): a planar face whose
    // loops mix `LineSegment` and `Circle`/`Ellipse` edges samples its loops
    // from the face's own Stage-1 chains. (Curved-chord subdivision by the
    // overlap boundary walls later, at the slice-1 gate.)
    if mixed_planar_face(brep, fi) {
        return true;
    }
    std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
        .all(|&e| matches!(brep.edges()[e as usize].curve, Curve::LineSegment))
}

/// If `fi` is a flat circular disc — planar surface, no holes, a single
/// outer-loop edge that is a closed `Curve::Circle` (`start == end`) — return
/// that circle edge's index. Else `None`.
fn disc_circle_edge(brep: &BRep, fi: usize) -> Option<u32> {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) || !f.inner_loops.is_empty() {
        return None;
    }
    if f.outer_loop.len() != 1 {
        return None;
    }
    let e = f.outer_loop[0];
    let edge = &brep.edges()[e as usize];
    matches!(edge.curve, Curve::Circle { .. } if edge.start == edge.end).then_some(e)
}

/// If `fi` is a flat ANNULAR disc — planar surface, a single closed-`Curve::Circle`
/// outer loop, and ≥1 inner loop each a single closed `Curve::Circle` (a bore /
/// swiss-cheese hole) — return `(outer_circle_edge, [hole_circle_edges])`. Else
/// `None`. The holes need not be concentric (each is classified by its own
/// circle geometry downstream). Spec `m8_holed_disc_coplanar_overlay` §1.
fn annular_disc_face(brep: &BRep, fi: usize) -> Option<(u32, Vec<u32>)> {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) || f.inner_loops.is_empty() {
        return None;
    }
    let is_full_circle = |loop_edges: &[u32]| -> Option<u32> {
        if loop_edges.len() != 1 {
            return None;
        }
        let e = loop_edges[0];
        let edge = &brep.edges()[e as usize];
        matches!(edge.curve, Curve::Circle { .. } if edge.start == edge.end).then_some(e)
    };
    let outer = is_full_circle(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(is_full_circle(lp)?);
    }
    Some((outer, holes))
}

/// Extract a disc face's rim ring (ordered CCW in the pair `frame`) by
/// re-running Stage 1 on this solid with the current (snapped) `coords` and
/// reading the cap fan's vertices. Returns the ring as ordered 3D points.
///
/// Pulling the ring from Stage 1's OWN output (rather than re-deriving it)
/// makes the disc mesh bit-identical to the cap/lateral tessellation
/// `build_stage0_mesh` produces for every non-overridden face — the
/// conformality the §4.5.5 shared-mesh guarantee rests on.
fn disc_rim_ring(brep: &BRep, fi: usize, coords: &[Point3], frame: &Frame) -> Option<Vec<Point3>> {
    let circle_e = disc_circle_edge(brep, fi)?;
    let Curve::Circle { center, .. } = brep.edges()[circle_e as usize].curve else {
        return None;
    };
    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let range = tess.face_tri_ranges.get(fi)?.clone();

    // Unique vertices of the cap fan = the rim ring + the one center Steiner
    // vertex. Drop the vertex nearest the circle centre; the rest are the rim.
    let c = center.as_array();
    let mut seen = std::collections::BTreeSet::new();
    let mut rim: Vec<Point3> = Vec::new();
    for tri in &tess.tris[range] {
        for &v in tri {
            if seen.insert(v) {
                let p = tess.verts[v as usize];
                let pa = p.as_array();
                let dr = ((pa[0] - c[0]).powi(2) + (pa[1] - c[1]).powi(2) + (pa[2] - c[2]).powi(2))
                    .sqrt();
                rim.push(p);
                let _ = dr;
            }
        }
    }
    if rim.len() < 4 {
        return None;
    }
    // Identify and drop the centre vertex (strictly closest to `center`).
    let center_idx = (0..rim.len()).min_by(|&i, &j| {
        let di = dist2(rim[i].as_array(), c);
        let dj = dist2(rim[j].as_array(), c);
        di.partial_cmp(&dj).unwrap()
    })?;
    rim.remove(center_idx);
    if rim.len() < 3 {
        return None;
    }
    // Order CCW by the in-frame angle about the circle centre.
    rim.sort_by(|p, q| {
        let ang = |x: &Point3| {
            let (u, v) = frame.project(*x);
            let (cu, cv) = frame.project(Point3::new(c[0], c[1], c[2]));
            (v - cv).atan2(u - cu)
        };
        ang(p).partial_cmp(&ang(q)).unwrap()
    });
    Some(rim)
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Extract the outer rim ring AND each hole rim ring of an ANNULAR disc face
/// (spec `m8_holed_disc_coplanar_overlay`). Like [`disc_rim_ring`], pulls the
/// rings from Stage 1's OWN output so the overlay mesh is bit-identical to the
/// cap/lateral tessellation (§4.5.5 conformality). The planar-curved CDT emits
/// NO interior Steiner points, so every unique face-triangle vertex lies on the
/// outer circle or one hole circle; each vertex is classified to the ring whose
/// circle it lies on (`||p − centerᵢ| − rᵢ|` minimal — robust for off-centre
/// holes). Outer ring ordered CCW, holes ordered CW (opposite sense) in the
/// pair `frame`. Returns `(outer_ring, [hole_rings])`.
fn annular_rim_rings(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(Vec<Point3>, Vec<Vec<Point3>>)> {
    let (outer_e, hole_es) = annular_disc_face(brep, fi)?;
    let circle_geo = |e: u32| -> Option<([f64; 3], f64)> {
        match brep.edges()[e as usize].curve {
            Curve::Circle { center, radius, .. } => Some((center.as_array(), radius)),
            _ => None,
        }
    };
    let (oc, or) = circle_geo(outer_e)?;
    let mut holes_geo: Vec<([f64; 3], f64)> = Vec::with_capacity(hole_es.len());
    for &e in &hole_es {
        holes_geo.push(circle_geo(e)?);
    }

    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let range = tess.face_tri_ranges.get(fi)?.clone();

    // Unique face-triangle vertices (all on a rim — no Steiner center here).
    let mut seen = std::collections::BTreeSet::new();
    let mut pts: Vec<Point3> = Vec::new();
    for tri in &tess.tris[range] {
        for &v in tri {
            if seen.insert(v) {
                pts.push(tess.verts[v as usize]);
            }
        }
    }

    // In-frame radial residual of a point against a circle (center, r).
    let residual = |p: &Point3, center: &[f64; 3], r: f64| -> f64 {
        let (pu, pv) = frame.project(*p);
        let (cu, cv) = frame.project(Point3::new(center[0], center[1], center[2]));
        (((pu - cu).powi(2) + (pv - cv).powi(2)).sqrt() - r).abs()
    };

    // Classify each vertex to the ring (outer=0, hole k → k+1) it lies on.
    let mut outer: Vec<Point3> = Vec::new();
    let mut holes: Vec<Vec<Point3>> = vec![Vec::new(); holes_geo.len()];
    for p in &pts {
        let mut best = (residual(p, &oc, or), 0usize);
        for (k, (hc, hr)) in holes_geo.iter().enumerate() {
            let d = residual(p, hc, *hr);
            if d < best.0 {
                best = (d, k + 1);
            }
        }
        if best.1 == 0 {
            outer.push(*p);
        } else {
            holes[best.1 - 1].push(*p);
        }
    }
    if outer.len() < 3 || holes.iter().any(|h| h.len() < 3) {
        return None;
    }

    // Order a ring by in-frame angle about `center`; `ccw` selects the sense.
    let order = |ring: &mut Vec<Point3>, center: &[f64; 3], ccw: bool| {
        let (cu, cv) = frame.project(Point3::new(center[0], center[1], center[2]));
        ring.sort_by(|p, q| {
            let ang = |x: &Point3| {
                let (u, v) = frame.project(*x);
                (v - cv).atan2(u - cu)
            };
            let (ap, aq) = (ang(p), ang(q));
            if ccw {
                ap.partial_cmp(&aq).unwrap()
            } else {
                aq.partial_cmp(&ap).unwrap()
            }
        });
    };
    order(&mut outer, &oc, true);
    for (k, h) in holes.iter_mut().enumerate() {
        order(h, &holes_geo[k].0, false);
    }
    Some((outer, holes))
}

/// Diagnostic-only: histogram of a face's loop-edge curve types + structure,
/// for the M8 residue survey (`YANG_COPLANAR_PROBE`). Not on any hot path.
fn face_curve_histogram(brep: &BRep, fi: usize) -> String {
    let f = &brep.faces()[fi];
    let mut seg = 0;
    let mut circle = 0;
    let mut ellipse = 0;
    let mut other = 0;
    for &e in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        match brep.edges()[e as usize].curve {
            Curve::LineSegment => seg += 1,
            Curve::Circle { .. } => circle += 1,
            Curve::Ellipse { .. } => ellipse += 1,
            _ => other += 1,
        }
    }
    let surf = match f.surface {
        Surface::Plane { .. } => "plane",
        _ => "nonplane",
    };
    format!(
        "surf={surf} outer={} holes={} seg={seg} circle={circle} ellipse={ellipse} other={other}",
        f.outer_loop.len(),
        f.inner_loops.len(),
    )
}

/// All loop vertex indices of a face (outer + holes), deduped.
fn face_loop_verts(brep: &BRep, fi: usize) -> Vec<u32> {
    let f = &brep.faces()[fi];
    let mut out: Vec<u32> = std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
        .flat_map(|&e| {
            let edge = &brep.edges()[e as usize];
            [edge.start, edge.end]
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Ordered vertex ring of one loop, taking each edge's `start` (the loop
/// continuity convention the Stage-1 fan path uses). `None` if the loop is
/// not continuous (`edges[loop[i]].end != edges[loop[i+1]].start`).
fn loop_vertex_ring(edges: &[BRepEdge], lp: &[u32]) -> Option<Vec<u32>> {
    let n = lp.len();
    if n < 3 {
        return None;
    }
    for i in 0..n {
        let e = &edges[lp[i] as usize];
        let next = &edges[lp[(i + 1) % n] as usize];
        if e.end != next.start {
            return None;
        }
    }
    Some(lp.iter().map(|&e| edges[e as usize].start).collect())
}

/// The face as a [`PolygonWithHoles`] in the pair frame, plus the exact
/// (u,v) → vertex-index map of its loop corners (for overlay-vertex
/// resolution). `None` on a non-continuous loop or non-finite coordinates.
fn face_polygon_2d(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(PolygonWithHoles, BTreeMap<ExactPoint2, u32>)> {
    let f = &brep.faces()[fi];
    let mut corners: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut project_ring = |lp: &[u32]| -> Option<Vec<Point2>> {
        let ring = loop_vertex_ring(brep.edges(), lp)?;
        let mut out = Vec::with_capacity(ring.len());
        for vi in ring {
            let (u, v) = frame.project(coords[vi as usize]);
            corners.insert(ExactPoint2::from_f64(u, v)?, vi);
            out.push(Point2::new(u, v));
        }
        Some(out)
    };
    let outer = project_ring(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(project_ring(lp)?);
    }
    Some((PolygonWithHoles { outer, holes }, corners))
}

/// Does any overlay vertex lie STRICTLY interior to one of `poly`'s outer
/// sub-chords (a rim edge)? True ⇒ the overlap boundary crosses the rim, so the
/// rim is subdivided and the cylinder lateral must absorb the split (the
/// crossing increment). Exact (rational), endpoints excluded.
fn rim_subdivided(poly: &PolygonWithHoles, overlay: &ClassifiedOverlay) -> bool {
    let ring = &poly.outer;
    let n = ring.len();
    if n < 2 {
        return false;
    }
    // Exact rim-edge keys (one per sub-chord), to skip overlay verts that ARE
    // rim vertices (endpoints) cheaply.
    for i in 0..n {
        let s = &ring[i];
        let e = &ring[(i + 1) % n];
        let (sx, sy) = (s.x(), s.y());
        let (ex, ey) = (e.x(), e.y());
        let (Some(s2), Some(e2)) = (ExactPoint2::from_f64(sx, sy), ExactPoint2::from_f64(ex, ey))
        else {
            continue;
        };
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        for q in &overlay.exact_verts {
            let wx = &q.x - &s2.x;
            let wy = &q.y - &s2.y;
            // On the sub-chord's supporting line?
            if &dx * &wy - &dy * &wx != RBig::ZERO {
                continue;
            }
            // Strictly interior, away from BOTH endpoints by a margin? A vertex
            // a few ULPs off a rim sample (t≈0 or t≈1) is that sample
            // reconstructed by the overlay — the rim-snap reconciles it, so it
            // is NOT a crossing. A genuine crossing sits macroscopically
            // mid-chord. The 1e-6 margin cleanly separates the two.
            let t = (&dx * &wx + &dy * &wy) / &len2;
            let tf = t.to_f64().value();
            if tf > 1.0e-6 && tf < 1.0 - 1.0e-6 {
                if std::env::var_os("RIM_SUBDIV_PROBE").is_some() {
                    eprintln!(
                        "[rim-subdiv] sub-chord {i} ({sx},{sy})->({ex},{ey}) interior vert ({},{}) t={tf}",
                        q.x.to_f64().value(),
                        q.y.to_f64().value(),
                    );
                }
                return true;
            }
        }
    }
    false
}

/// M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): does any overlay vertex
/// lie strictly interior to a CURVED sub-chord of this mixed face — outer
/// ring or hole rings, segments selected by `masks` (the
/// [`face_polygon_2d_tessellated`] mixed-arm attribution)? Same exact
/// predicate as [`rim_subdivided`] (rational collinearity + interior
/// parameter with the ULP-reconstruction margin), restricted to curved
/// segments: straight-edge subdivision is legitimate `collect_edge_splits`
/// traffic. True triggers [`collect_mixed_crossings`] propagation.
fn curved_chords_subdivided(
    poly: &PolygonWithHoles,
    masks: &[Vec<Option<u32>>],
    overlay: &ClassifiedOverlay,
) -> bool {
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(masks)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        for i in 0..n {
            if mask[i].is_none() {
                continue;
            }
            let s = &ring[i];
            let e = &ring[(i + 1) % n];
            let (Some(s2), Some(e2)) = (
                ExactPoint2::from_f64(s.x(), s.y()),
                ExactPoint2::from_f64(e.x(), e.y()),
            ) else {
                continue;
            };
            let dx = &e2.x - &s2.x;
            let dy = &e2.y - &s2.y;
            let len2 = &dx * &dx + &dy * &dy;
            if len2 == RBig::ZERO {
                continue;
            }
            for q in &overlay.exact_verts {
                let wx = &q.x - &s2.x;
                let wy = &q.y - &s2.y;
                if &dx * &wy - &dy * &wx != RBig::ZERO {
                    continue;
                }
                // Strictly interior with the same margin as `rim_subdivided`:
                // a vertex a few ULPs off a chain sample is that sample
                // reconstructed by the overlay, not a crossing.
                let t = ((&dx * &wx + &dy * &wy) / &len2).to_f64().value();
                if t > 1.0e-6 && t < 1.0 - 1.0e-6 {
                    return true;
                }
            }
        }
    }
    false
}

/// The fold gate's 3D-bit-degeneracy test (the M-B emission-drop class): a
/// triangle whose RESOLVED image carries a bit-duplicate vertex is never
/// emitted, so its 2D state must not drive gate decisions.
fn gate_tri_degenerate(t: &[u32; 3], coords: &[Point3]) -> bool {
    let bits = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let b = [
        bits(coords[t[0] as usize]),
        bits(coords[t[1] as usize]),
        bits(coords[t[2] as usize]),
    ];
    b[0] == b[1] || b[1] == b[2] || b[0] == b[2]
}

/// The fold gate's 2D signed area under the CURRENT resolved coordinates,
/// projected into the pair frame.
fn gate_tri_area(t: &[u32; 3], coords: &[Point3], frame: &Frame) -> f64 {
    let p0 = frame.project(coords[t[0] as usize]);
    let p1 = frame.project(coords[t[1] as usize]);
    let p2 = frame.project(coords[t[2] as usize]);
    (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)
}

/// A triangle is valid under the current resolved coordinates if it winds
/// material-CCW (positive area) or its 3D image is bit-degenerate (the M-B
/// emission-drop class). The single validity contract shared by the
/// amendment-4 flips and the amendment-5 cavity relocation.
fn gate_tri_valid(t: &[u32; 3], coords: &[Point3], frame: &Frame) -> bool {
    gate_tri_degenerate(t, coords) || gate_tri_area(t, coords, frame) > 0.0
}

/// Exact orientation sign of the 2D triple (a, b, c) — rational arithmetic
/// over the raw f64 frame projections (P9: no tolerance). `None` on
/// non-finite input.
fn orient_sign_exact(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Option<i8> {
    use crate::coplanar_overlay::rat;
    let (ax, ay) = (rat(a.0).ok()?, rat(a.1).ok()?);
    let (bx, by) = (rat(b.0).ok()?, rat(b.1).ok()?);
    let (cx, cy) = (rat(c.0).ok()?, rat(c.1).ok()?);
    let det = (&bx - &ax) * (&cy - &ay) - (&by - &ay) * (&cx - &ax);
    Some(match det.cmp(&RBig::ZERO) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
    })
}

/// Outcome of a per-vertex cavity relocation attempt (amendments 5/6).
enum RelocOutcome {
    /// The cavity was re-triangulated and committed.
    Committed,
    /// Rejected for a reason joint relocation cannot help with; no mutation.
    Rejected,
    /// Amendment 6: the cavity polygon was exactly NON-SIMPLE — the classic
    /// interacting-mints signature (another minted vertex's collapsed spokes
    /// cross the ring). `ring_mints` are the OTHER minted vertices on the
    /// CROSSING edges (amendment 10: the interacting set per Fig-11
    /// locality — NOT every mint on the ring; a hole-encircling ring lists
    /// dozens of mints and seeding them all inflates the joint region into
    /// an annulus). No mutation.
    NonSimple { ring_mints: Vec<u32> },
}

/// Reject reason of [`earclip_cavity_polygon`]: exact non-simplicity is
/// distinguished because it is the amendment-6 joint-relocation trigger.
/// `crossing` carries the first crossing pair's endpoint POSITIONS (in the
/// caller's frame projection — bit-identical to `frame.project` of the
/// poly vertices), so the caller can identify the interacting mints.
enum EarclipErr {
    NotSimple { crossing: [(f64, f64); 4] },
    Other(&'static str),
}

/// Shared amendment-5/6 re-triangulation core: exact simplicity + CCW
/// verification on the DEDUPLICATED position ring of `poly`, then
/// constrained exact ear-clipping of the polygon (ears exact-CCW,
/// gate-valid, empty, and a NEW diagonal — one not already carried by a
/// triangle outside `cavity`; ears whose 3D image is bit-degenerate clip
/// freely, the M-B emission-drop class). Pure — mutates nothing; the
/// caller commits. `poly` is the cavity boundary cycle (per-vertex form:
/// `[v, w₀, …, w_k]`; joint form: the region boundary), all positions at
/// their CURRENT resolved coordinates.
#[allow(clippy::too_many_arguments)]
fn earclip_cavity_polygon(
    poly: &[u32],
    cavity: &std::collections::BTreeSet<usize>,
    cls0: RegionClass,
    coords: &[Point3],
    frame: &Frame,
    edge_map: &BTreeMap<[u32; 2], Vec<usize>>,
    probe: bool,
    probe_who: &str,
) -> Result<Vec<([u32; 3], RegionClass)>, EarclipErr> {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let pos = |i: u32| frame.project(coords[i as usize]);

    // Exact simplicity + CCW on the DEDUPLICATED position ring (collapsed
    // sub-floor twins share one resolved position; their zero-length edges
    // cannot cross anything).
    let ring: Vec<(f64, f64)> = {
        let mut r: Vec<(f64, f64)> = Vec::with_capacity(poly.len());
        for &pi in poly {
            let q = pos(pi);
            if r.last() != Some(&q) {
                r.push(q);
            }
        }
        while r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
        r
    };
    if ring.len() < 3 {
        return Err(EarclipErr::Other("degenerate cavity polygon"));
    }
    // Simplicity BEFORE orientation (amendment 11, M8 increment 14): a
    // bow-tie's signed area is lobe-balance noise — a net-CW non-simple
    // ring (measured F0088 vert 674: hair-thin full-height strip whose
    // return edge crosses the up-chain, net 2A = −4.2e-3) must surface as
    // `NotSimple` (the joint-relocation trigger), not die at the
    // orientation guard. Only a SIMPLE ring has a meaningful winding.
    let n = ring.len();
    for a in 0..n {
        for b in (a + 1)..n {
            // Adjacent ring edges share exactly one endpoint — allowed.
            if b == a + 1 || (a == 0 && b == n - 1) {
                continue;
            }
            let (p1, p2) = (ring[a], ring[(a + 1) % n]);
            let (q1, q2) = (ring[b], ring[(b + 1) % n]);
            // Non-adjacent shared position = a pinch.
            if p1 == q1 || p1 == q2 || p2 == q1 || p2 == q2 {
                return Err(EarclipErr::Other(
                    "cavity polygon pinched (repeated position)",
                ));
            }
            let (Some(o1), Some(o2), Some(o3), Some(o4)) = (
                orient_sign_exact(p1, p2, q1),
                orient_sign_exact(p1, p2, q2),
                orient_sign_exact(q1, q2, p1),
                orient_sign_exact(q1, q2, p2),
            ) else {
                return Err(EarclipErr::Other("non-finite cavity polygon"));
            };
            // Proper crossing, or an endpoint on the other segment's
            // INTERIOR. Bare collinearity (o == 0 with the point outside
            // the segment) is NOT an intersection — sweep-event columns
            // legitimately put many ring vertices on one exact vertical
            // line, so rejecting all collinear pairs falsely rejects
            // repairable cavities (measured: F0087 cut 10, vert 186).
            // Endpoint coincidence was excluded above, so on-segment
            // here means strictly interior; collinear-overlapping
            // segments have an endpoint interior to the other and are
            // caught by the same test.
            let within = |o: i8, e1: (f64, f64), e2: (f64, f64), q: (f64, f64)| {
                o == 0
                    && q.0 >= e1.0.min(e2.0)
                    && q.0 <= e1.0.max(e2.0)
                    && q.1 >= e1.1.min(e2.1)
                    && q.1 <= e1.1.max(e2.1)
            };
            if (o1 * o2 < 0 && o3 * o4 < 0)
                || within(o1, p1, p2, q1)
                || within(o2, p1, p2, q2)
                || within(o3, q1, q2, p1)
                || within(o4, q1, q2, p2)
            {
                if probe {
                    eprintln!(
                        "  [reloc-ring] edges {a}:({p1:?}->{p2:?}) x {b}:({q1:?}->{q2:?}) \
                         o=({o1},{o2},{o3},{o4}) ring={ring:?}"
                    );
                }
                return Err(EarclipErr::NotSimple {
                    crossing: [p1, p2, q1, q2],
                });
            }
        }
    }
    {
        use crate::coplanar_overlay::rat;
        let mut two_area = RBig::ZERO;
        for k in 0..ring.len() {
            let (ax, ay) = ring[k];
            let (bx, by) = ring[(k + 1) % ring.len()];
            let Ok(t) = rat(ax).and_then(|axr| Ok(axr * rat(by)? - rat(bx)? * rat(ay)?)) else {
                return Err(EarclipErr::Other("non-finite cavity polygon"));
            };
            two_area += t;
        }
        if two_area <= RBig::ZERO {
            // The ring is SIMPLE (checked above) yet winds CW or is
            // degenerate: a genuinely inside-out cavity — terminal.
            if probe {
                eprintln!(
                    "  [reloc-ccw] {probe_who} two_area {} ring={ring:?}",
                    if two_area == RBig::ZERO {
                        "ZERO"
                    } else {
                        "NEG"
                    }
                );
            }
            return Err(EarclipErr::Other("cavity polygon not CCW"));
        }
    }

    // Constrained ear-clip: deterministic first-clippable-ear order.
    let mut work: Vec<u32> = poly.to_vec();
    let mut ears: Vec<([u32; 3], RegionClass)> = Vec::with_capacity(poly.len());
    while work.len() > 3 {
        let m = work.len();
        let mut clipped = false;
        'ear: for k in 0..m {
            let (ia, ib, ic) = (work[(k + m - 1) % m], work[k], work[(k + 1) % m]);
            let ear = [ia, ib, ic];
            if !gate_tri_degenerate(&ear, coords) {
                // Convex, gate-valid, empty, and a NEW diagonal.
                if !gate_tri_valid(&ear, coords, frame) {
                    continue;
                }
                let (pa, pb, pc) = (pos(ia), pos(ib), pos(ic));
                if orient_sign_exact(pa, pb, pc) != Some(1) {
                    continue;
                }
                for &other in work.iter() {
                    if other == ia || other == ib || other == ic {
                        continue;
                    }
                    let q = pos(other);
                    // Coincident with a corner (a collapsed twin) never
                    // blocks; its own zero-area ear clips it.
                    if q == pa || q == pb || q == pc {
                        continue;
                    }
                    let (Some(s1), Some(s2), Some(s3)) = (
                        orient_sign_exact(pa, pb, q),
                        orient_sign_exact(pb, pc, q),
                        orient_sign_exact(pc, pa, q),
                    ) else {
                        return Err(EarclipErr::Other("non-finite cavity polygon"));
                    };
                    if s1 >= 0 && s2 >= 0 && s3 >= 0 {
                        continue 'ear; // inside or on the ear
                    }
                }
                if let Some(inc) = edge_map.get(&edge_key(ia, ic)) {
                    if inc.iter().any(|t| !cavity.contains(t)) {
                        continue; // diagonal exists outside the cavity
                    }
                }
            }
            ears.push((ear, cls0));
            work.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            return Err(EarclipErr::Other("no clippable ear"));
        }
    }
    let last = [work[0], work[1], work[2]];
    if !gate_tri_valid(&last, coords, frame) {
        return Err(EarclipErr::Other("final ear invalid"));
    }
    ears.push((last, cls0));
    if probe {
        eprintln!("  [reloc-earclip] {probe_who} cavity={} tris", cavity.len());
    }
    Ok(ears)
}

/// Amendment 5 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment 8):
/// delete-and-reinsert cavity relocation of one minted vertex — the full
/// [#24 Yang §4.4.1 Fig 11] mesh-updating form, for folds a single Lawson
/// flip cannot repair (the rim-mint COLUMN HOP: a mint's in-plane
/// displacement crosses a populated sweep-event column, folding the whole
/// inter-column strip of triangles together; the folded set's boundary is
/// non-simple under the moved vertex, so neither flips nor a fan of the
/// folded set alone can fix it).
///
/// The star of `v` is carved out and re-triangulated around `v`'s CURRENT
/// resolved (minted) position in two stages:
///
/// 1. **Visibility growth (Bowyer–Watson):** a link edge whose fan triangle
///    `(v, wᵢ, wᵢ₊₁)` is invalid is crossed into its single external
///    same-class neighbor. Constraint edges are never crossed — a
///    class-boundary edge IS the intersection curve and a single-incidence
///    edge is the domain boundary — nor is a neighbor absorbed whose apex
///    already lies on the link (a pinch); such edges are DEFERRED, not
///    fatal. Growth is monotone (the cavity only gains triangles), so it
///    terminates; blocked edges can never become growable (fan validity is
///    coordinate-determined and externals only shrink), so one forward scan
///    with in-place re-checks suffices.
/// 2. If every fan triangle is valid, the fan IS the re-triangulation.
///    Otherwise (some deferred edge remains — the mint crossed the LINE of
///    a constraint chord whose segment lies elsewhere, so the cavity is not
///    star-shaped from `v`) the cavity polygon `[v, w₀, …, w_k]` is
///    re-triangulated by **constrained exact ear-clipping**: the constraint
///    edge stays a cavity boundary and is connected to other link vertices
///    instead of `v`. Guards (each rejects, loud): single-class cavity and
///    an open chain only (no constraint spokes to preserve, `v` on the
///    domain boundary); the polygon must be exactly simple and CCW on the
///    deduplicated position ring; an ear needs exact-CCW orientation, gate
///    validity, no other polygon vertex strictly inside or on it, and a
///    diagonal that does not already exist outside the cavity. Ears whose
///    3D image is bit-degenerate (collapsed sub-floor twins) clip freely —
///    they are dropped at emission (M-B).
///
/// Any reject leaves NO mutation (build-then-commit); the amendment-2
/// revert stays the caller's fallback, observable via kernel-v2's tripwire.
/// Purely combinatorial (`coords` fixed, same contract as the amendment-4
/// flips): every committed relocation replaces its cavity with all-valid
/// triangles and no other triangle changes shape, so the gate's folded
/// count strictly decreases — termination. Deterministic: BTree orders,
/// first-invalid-link-edge growth, first-clippable-ear order (I6). Cavity
/// size equals link-edge count throughout (a boundary star of k triangles
/// has a k-edge open chain, an interior star a k-edge cycle; each growth
/// step adds one of each; a (k+2)-gon ear-clips to k triangles), so the
/// replacement overwrites the cavity slots in place and `edge_map` is
/// maintained incrementally.
fn relocate_minted_vertex(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    v: u32,
    coords: &[Point3],
    frame: &Frame,
    minted_mark: &[bool],
    probe: bool,
) -> RelocOutcome {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-reject] vert {v} {why}");
        }
        RelocOutcome::Rejected
    };

    // ── 1. Star + oriented link chain ────────────────────────────────────
    let star: Vec<usize> = tris
        .iter()
        .enumerate()
        .filter(|(_, t)| t.contains(&v))
        .map(|(i, _)| i)
        .collect();
    if star.is_empty() {
        return reject("empty star");
    }
    // Oriented opposite edge of each star triangle (consistent-CCW mesh ⇒
    // the link edges chain head-to-tail around v).
    let mut out: BTreeMap<u32, (u32, RegionClass)> = BTreeMap::new();
    let mut heads: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for &ti in &star {
        let t = tris[ti];
        let k = t.iter().position(|&x| x == v).unwrap();
        let (a, b) = (t[(k + 1) % 3], t[(k + 2) % 3]);
        if out.insert(a, (b, class[ti])).is_some() {
            return reject("non-manifold star (duplicate link tail)");
        }
        heads.insert(b);
    }
    // Open chain (boundary vertex): exactly one tail that is never a head.
    // Closed chain (interior vertex): none — start at the smallest tail.
    let starts: Vec<u32> = out.keys().copied().filter(|a| !heads.contains(a)).collect();
    let start = match starts.len() {
        0 => *out.keys().next().unwrap(),
        1 => starts[0],
        _ => return reject("disconnected star (multiple open chains)"),
    };
    let mut link: Vec<(u32, u32, RegionClass)> = Vec::with_capacity(star.len());
    let mut visited: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut cur = start;
    for _ in 0..star.len() {
        if !visited.insert(cur) {
            // A revisit before covering every star triangle = a shorter
            // subloop (non-manifold star).
            return reject("link chain revisits a vertex (subloop)");
        }
        let Some(&(next, cls)) = out.get(&cur) else {
            return reject("broken link chain");
        };
        link.push((cur, next, cls));
        cur = next;
    }
    if link.len() != star.len() || (starts.is_empty() && cur != start) {
        return reject("link chain does not cover the star");
    }

    // ── 2. Cavity carve by visibility growth (deferring at constraints) ──
    let mut cavity: std::collections::BTreeSet<usize> = star.iter().copied().collect();
    let mut deferred = false;
    let mut i = 0;
    while i < link.len() {
        let (a, b, cls) = link[i];
        if gate_tri_valid(&[v, a, b], coords, frame) {
            i += 1;
            continue;
        }
        let Some(inc) = edge_map.get(&edge_key(a, b)) else {
            return reject("link edge missing from edge map");
        };
        let ext: Vec<usize> = inc
            .iter()
            .copied()
            .filter(|t| !cavity.contains(t))
            .collect();
        if ext.len() != 1 {
            // Domain boundary (or a pinched edge both of whose sides were
            // absorbed): uncrossable — defer to the ear-clip.
            deferred = true;
            i += 1;
            continue;
        }
        let tj = ext[0];
        if class[tj] != cls {
            // Class boundary IS the intersection curve: uncrossable — defer.
            deferred = true;
            i += 1;
            continue;
        }
        // The external neighbor traverses (b, a); its apex joins the link.
        let tn = tris[tj];
        let Some(k) = (0..3).find(|&k| tn[k] == b && tn[(k + 1) % 3] == a) else {
            return reject("inconsistent neighbor orientation");
        };
        let x = tn[(k + 2) % 3];
        if x == v
            || link.iter().any(|&(la, lb, _)| la == x || lb == x)
            || edge_map.contains_key(&edge_key(v, x))
        {
            // Absorbing would pinch the cavity (apex already on the link /
            // spoke already exists): defer to the ear-clip.
            deferred = true;
            i += 1;
            continue;
        }
        let ncls = class[tj];
        cavity.insert(tj);
        link.splice(i..=i, [(a, x, ncls), (x, b, ncls)]);
        // Re-check from the first replacement edge. Edges before i cannot
        // regress (fan validity is coordinate-determined) and blocked edges
        // cannot become growable (externals only shrink) — one scan is a
        // fixpoint.
    }
    if cavity.len() != link.len() {
        return reject("cavity/link size mismatch");
    }

    // ── 3. Re-triangulation: fan, or constrained exact ear-clip ──────────
    let new_tris: Vec<([u32; 3], RegionClass)> = if !deferred {
        if probe {
            eprintln!("  [reloc-fan] vert {v} cavity={} tris", cavity.len());
        }
        link.iter().map(|&(a, b, cls)| ([v, a, b], cls)).collect()
    } else {
        // The cavity is not star-shaped from v's minted position (the mint
        // crossed the LINE of a constraint chord). Ear-clip the cavity
        // polygon [v, w0..wk] instead — the constraint edge stays a cavity
        // BOUNDARY, connected to other link vertices.
        if starts.is_empty() {
            return reject("interior vertex with constraint-blocked fan");
        }
        let cls0 = link[0].2;
        if link.iter().any(|&(_, _, c)| c != cls0) {
            return reject("multi-class cavity with constraint-blocked fan");
        }
        let mut poly: Vec<u32> = Vec::with_capacity(link.len() + 2);
        poly.push(v);
        poly.push(link[0].0);
        for &(_, b, _) in &link {
            poly.push(b);
        }
        if probe {
            let w0 = link[0].0;
            let wk = link[link.len() - 1].1;
            eprintln!(
                "  [reloc-spokes] v={v} w0={w0} inc={:?} wk={wk} inc={:?}",
                edge_map.get(&edge_key(v, w0)).map(|x| x.len()),
                edge_map.get(&edge_key(v, wk)).map(|x| x.len()),
            );
        }
        match earclip_cavity_polygon(
            &poly,
            &cavity,
            cls0,
            coords,
            frame,
            edge_map,
            probe,
            &format!("vert {v}"),
        ) {
            Ok(ears) => ears,
            Err(EarclipErr::NotSimple { crossing }) => {
                if probe {
                    eprintln!("  [reloc-reject] vert {v} cavity polygon not simple");
                }
                // Amendment 6 trigger, amendment-10 narrowed: the joint
                // seeds are the minted vertices ON the crossing edges (the
                // interacting set — Fig-11 locality), identified by exact
                // position match against the same frame projection the
                // ear-clip used.
                return RelocOutcome::NonSimple {
                    ring_mints: poly
                        .iter()
                        .copied()
                        .filter(|&pi| {
                            pi != v
                                && minted_mark[pi as usize]
                                && crossing.contains(&frame.project(coords[pi as usize]))
                        })
                        .collect(),
                };
            }
            Err(EarclipErr::Other(why)) => return reject(why),
        }
    };
    if new_tris.len() != cavity.len() {
        return reject("replacement/cavity size mismatch");
    }

    // ── 4. Commit: overwrite the cavity slots in place ────────────────────
    let cavity: Vec<usize> = cavity.into_iter().collect();
    for &ti in &cavity {
        let t = tris[ti];
        for k in 0..3 {
            let kk = edge_key(t[k], t[(k + 1) % 3]);
            if let Some(e) = edge_map.get_mut(&kk) {
                e.retain(|&x| x != ti);
                if e.is_empty() {
                    edge_map.remove(&kk);
                }
            }
        }
    }
    for (&ti, &(t, cls)) in cavity.iter().zip(&new_tris) {
        tris[ti] = t;
        class[ti] = cls;
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    RelocOutcome::Committed
}

/// Amendment 6 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment 9,
/// task #64): JOINT delete-and-reinsert relocation of an interacting set of
/// minted vertices — the [#24 Yang §4.4.1 Fig 11] mesh-updating form
/// generalized from one vertex's star to the UNION of the seeds' stars, for
/// the multi-column strip class where each per-vertex cavity polygon is
/// exactly NON-SIMPLE because it contains the OTHER minted vertex's
/// collapsed spokes (measured F0087 cut 9: the plate-rim mint and a
/// hole-rim mint at the two ends of one strip of long CDT triangles).
///
/// The region = the union of the seeds' vertex stars. Its oriented
/// boundary (edges whose reverse no region triangle carries — domain
/// boundaries qualify by construction) must chain into exactly ONE closed
/// cycle passing through every region-triangle vertex; the cycle is then
/// re-triangulated by the shared constrained exact ear-clip
/// ([`earclip_cavity_polygon`]) with all seeds at their minted positions.
/// Guards, each a reject (the caller's amendment-2 revert stays the loud
/// fallback): single class across the region (class-boundary edges — the
/// intersection curve — are then automatically ON the cycle, never
/// re-triangulated across); no interior vertex (seed or not — a polygon
/// triangulation would orphan it); one cycle; exact simplicity + CCW.
///
/// Build-then-commit: a reject leaves NO mutation. Purely combinatorial
/// (`coords` fixed, the amendment-4/5 termination contract): a committed
/// joint relocation replaces ≥1 folded triangle with all-valid triangles
/// and no fold can be created, so the gate's folded count strictly
/// decreases. Deterministic: ascending seeds, smallest-tail cycle start,
/// first-clippable-ear order (I6). A triangulated simple polygon with no
/// interior vertices has exactly (cycle length − 2) triangles, so the
/// replacement count equals the region size and the region's slots are
/// overwritten in place.
fn relocate_minted_region(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    seeds: &[u32],
    coords: &[Point3],
    frame: &Frame,
    probe: bool,
) -> bool {
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-region-reject] seeds {seeds:?} {why}");
        }
        false
    };

    // ── 1. Region = union of the seeds' stars, PARTITIONED by class ──────
    // Amendment 7 (M8 increment 10): rim mints are minted exactly ON the
    // intersection curve — that is what a rim crossing is — so the star
    // union routinely straddles the class boundary. Each class sub-region
    // is relocated independently: a class-boundary edge's reverse lives in
    // the OTHER class's triangle (outside the sub-region), so the
    // intersection curve becomes sub-region boundary by construction and
    // is never re-triangulated across. A single-class region makes the
    // partition the identity (amendment-6 behavior, unchanged).
    let mut by_class: BTreeMap<RegionClass, std::collections::BTreeSet<usize>> = BTreeMap::new();
    for (ti, t) in tris.iter().enumerate() {
        if t.iter().any(|v| seeds.contains(v)) {
            by_class.entry(class[ti]).or_default().insert(ti);
        }
    }
    let mut committed_any = false;
    for (cls0, region) in by_class {
        // Amendment 9 (M8 increment 12): a class sub-region may be
        // DISCONNECTED — the joint trigger accumulates seeds from several
        // separate strips, and one boundary walk cannot cover two
        // components. Split into edge-connected components (deterministic
        // ascending-index BFS through shared edges); each is its own
        // Fig-11 instance, attempted independently.
        let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut unvisited = region.clone();
        while let Some(&comp_seed) = unvisited.iter().next() {
            let mut component: std::collections::BTreeSet<usize> =
                std::collections::BTreeSet::new();
            let mut queue = vec![comp_seed];
            unvisited.remove(&comp_seed);
            while let Some(ti) = queue.pop() {
                component.insert(ti);
                let t = tris[ti];
                for k in 0..3 {
                    if let Some(inc) = edge_map.get(&edge_key(t[k], t[(k + 1) % 3])) {
                        for &tj in inc {
                            if unvisited.remove(&tj) {
                                queue.push(tj);
                            }
                        }
                    }
                }
            }
            // Termination contract: only a component carrying at least one
            // FOLDED triangle is attempted — its commit strictly decreases
            // the gate's folded count (replacement ears are gate-valid by
            // construction). A valid-only component is SKIPPED:
            // re-triangulating it could churn the mesh without progress.
            let folded = component.iter().any(|&ti| {
                !gate_tri_degenerate(&tris[ti], coords)
                    && gate_tri_area(&tris[ti], coords, frame) <= 0.0
            });
            if !folded {
                continue;
            }
            if relocate_region_single_class(
                tris, class, edge_map, seeds, &component, cls0, coords, frame, probe,
            ) {
                committed_any = true;
            }
        }
    }
    if !committed_any {
        return reject("every folded class sub-region rejected");
    }
    true
}

/// One class sub-region of the amendment-6/7 joint relocation: oriented
/// boundary cycle, no interior vertex, exact simplicity + CCW, shared
/// constrained exact ear-clip, in-place commit. Build-then-commit: a
/// reject leaves NO mutation of this sub-region (other sub-regions'
/// commits are independent — each is separately valid and fold-reducing).
#[allow(clippy::too_many_arguments)]
fn relocate_region_single_class(
    tris: &mut [[u32; 3]],
    class: &mut [RegionClass],
    edge_map: &mut BTreeMap<[u32; 2], Vec<usize>>,
    seeds: &[u32],
    region: &std::collections::BTreeSet<usize>,
    cls0: RegionClass,
    coords: &[Point3],
    frame: &Frame,
    probe: bool,
) -> bool {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-region-reject] seeds {seeds:?} class {cls0:?} {why}");
        }
        false
    };
    if region.len() < 2 {
        return reject("region too small");
    }

    // Amendment 8 (M8 increment 11): the sub-region may GROW across a
    // crossing boundary edge (below), so the boundary cycle and its guards
    // are recomputed per growth step.
    let mut region: std::collections::BTreeSet<usize> = region.clone();
    let poly = loop {
        // ── 2. Oriented boundary cycle ────────────────────────────────────
        // A consistent-CCW mesh: an oriented edge (a,b) of a region triangle
        // is boundary iff no region triangle carries (b,a).
        let mut oriented: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        for &ti in &region {
            let t = tris[ti];
            for k in 0..3 {
                if !oriented.insert((t[k], t[(k + 1) % 3])) {
                    return reject("duplicate oriented edge (non-manifold region)");
                }
            }
        }
        let mut nxt: BTreeMap<u32, u32> = BTreeMap::new();
        let mut boundary_edges = 0usize;
        for &(a, b) in &oriented {
            if !oriented.contains(&(b, a)) {
                if nxt.insert(a, b).is_some() {
                    return reject("non-manifold region boundary (duplicate tail)");
                }
                boundary_edges += 1;
            }
        }
        if boundary_edges < 3 {
            return reject("degenerate region boundary");
        }
        let start = *nxt.keys().next().unwrap();
        let mut poly: Vec<u32> = Vec::with_capacity(boundary_edges);
        let mut cur = start;
        for _ in 0..boundary_edges {
            poly.push(cur);
            let Some(&next) = nxt.get(&cur) else {
                return reject("broken region boundary chain");
            };
            cur = next;
        }
        if cur != start || poly.len() != boundary_edges {
            // Increment-13 measurement probe: enumerate ALL boundary
            // cycles (count + lengths) so the annular class's structure is
            // observable at the reject site (one component with several
            // cycles = a region encircling a hole).
            if probe {
                let mut rest = nxt.clone();
                let mut cycles: Vec<usize> = Vec::new();
                while let Some((&s0, _)) = rest.iter().next() {
                    let mut c = s0;
                    let mut len = 0usize;
                    while let Some(n) = rest.remove(&c) {
                        len += 1;
                        c = n;
                        if c == s0 {
                            break;
                        }
                    }
                    cycles.push(len);
                }
                eprintln!(
                    "  [reloc-region-cycles] seeds {seeds:?} class {cls0:?} \
                     {} boundary cycles, lengths {cycles:?}",
                    cycles.len()
                );
            }
            return reject("region boundary is not a single closed cycle");
        }

        // ── 3. No interior vertex (a triangulation would orphan it) ───────
        let on_cycle: std::collections::BTreeSet<u32> = poly.iter().copied().collect();
        for &ti in &region {
            for &vv in &tris[ti] {
                if !on_cycle.contains(&vv) {
                    return reject("region has an interior vertex");
                }
            }
        }

        // ── 3b. Amendment 8: growth to simplicity ─────────────────────────
        // A femto-strip sub-region's boundary can be a BOW-TIE under the
        // minted positions (the strip's two long sides cross exactly — the
        // F0090 class). The region form of amendment 5's constrained
        // visibility growth: absorb the single external same-class neighbor
        // of a crossing edge and rebuild the boundary, until the ring is
        // exactly simple. Constraint edges (domain boundary, intersection
        // curve) are never crossed; an apex already on the cycle would
        // pinch the ring (both defer to the partner edge, else reject).
        let Some((ei, ej)) = first_ring_crossing(&poly, coords, frame) else {
            break poly;
        };
        let mut grew = false;
        for e in [ei, ej] {
            let (a, b) = (poly[e], poly[(e + 1) % poly.len()]);
            let Some(inc) = edge_map.get(&edge_key(a, b)) else {
                continue;
            };
            let ext: Vec<usize> = inc
                .iter()
                .copied()
                .filter(|t| !region.contains(t))
                .collect();
            if ext.len() != 1 {
                continue; // domain boundary (or pinched): uncrossable
            }
            let tj = ext[0];
            if class[tj] != cls0 {
                continue; // class boundary IS the intersection curve
            }
            let Some(x) = tris[tj].iter().copied().find(|&v| v != a && v != b) else {
                continue;
            };
            if on_cycle.contains(&x) {
                continue; // absorbing would pinch the ring
            }
            if probe {
                eprintln!(
                    "  [reloc-region-grow] seeds {seeds:?} class {cls0:?} \
                     edge ({a},{b}) absorbs tri {tj} (apex {x})"
                );
            }
            region.insert(tj);
            grew = true;
            break;
        }
        if !grew {
            return reject("crossing edges ungrowable (region polygon not simple)");
        }
    };

    // ── 4. Shared constrained exact ear-clip ──────────────────────────────
    let ears = match earclip_cavity_polygon(
        &poly,
        &region,
        cls0,
        coords,
        frame,
        edge_map,
        probe,
        &format!("region {seeds:?}"),
    ) {
        Ok(ears) => ears,
        Err(EarclipErr::NotSimple { .. }) => return reject("region polygon not simple"),
        Err(EarclipErr::Other(why)) => return reject(why),
    };
    if ears.len() != region.len() {
        return reject("replacement/region size mismatch");
    }

    // ── 5. Commit: overwrite the region slots in place ────────────────────
    let region: Vec<usize> = region.iter().copied().collect();
    for &ti in &region {
        let t = tris[ti];
        for k in 0..3 {
            let kk = edge_key(t[k], t[(k + 1) % 3]);
            if let Some(e) = edge_map.get_mut(&kk) {
                e.retain(|&x| x != ti);
                if e.is_empty() {
                    edge_map.remove(&kk);
                }
            }
        }
    }
    for (&ti, &(t, cls)) in region.iter().zip(&ears) {
        tris[ti] = t;
        class[ti] = cls;
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    true
}

/// Amendment 8 (spec `n2_stage4_junction_cluster_merge` §3, M8 increment
/// 11): first exact crossing of a boundary polygon's edges under the
/// CURRENT resolved coordinates — a proper crossing or an endpoint strictly
/// interior to the other segment (the same predicate as
/// [`earclip_cavity_polygon`]'s simplicity guard, `EarclipErr::NotSimple`
/// class). Returns the two poly edge indices, first pair in boundary-order
/// scan (deterministic — I6); `None` = simple. Zero-length edges (collapsed
/// sub-floor twins) and edge pairs sharing a POSITION are skipped — shared
/// positions are the pinch class, terminal in the ear-clip, not grown.
fn first_ring_crossing(poly: &[u32], coords: &[Point3], frame: &Frame) -> Option<(usize, usize)> {
    let n = poly.len();
    let pos = |i: usize| frame.project(coords[poly[i] as usize]);
    for a in 0..n {
        let (p1, p2) = (pos(a), pos((a + 1) % n));
        if p1 == p2 {
            continue; // zero-length (collapsed twins) cannot cross
        }
        for b in (a + 1)..n {
            let (q1, q2) = (pos(b), pos((b + 1) % n));
            if q1 == q2 {
                continue;
            }
            // Edges sharing a position are adjacent (or a pinch — the
            // ear-clip's terminal class): never a growth trigger.
            if p1 == q1 || p1 == q2 || p2 == q1 || p2 == q2 {
                continue;
            }
            let (Some(o1), Some(o2), Some(o3), Some(o4)) = (
                orient_sign_exact(p1, p2, q1),
                orient_sign_exact(p1, p2, q2),
                orient_sign_exact(q1, q2, p1),
                orient_sign_exact(q1, q2, p2),
            ) else {
                return None; // non-finite: leave for the ear-clip to reject
            };
            let within = |o: i8, e1: (f64, f64), e2: (f64, f64), q: (f64, f64)| {
                o == 0
                    && q.0 >= e1.0.min(e2.0)
                    && q.0 <= e1.0.max(e2.0)
                    && q.1 >= e1.1.min(e2.1)
                    && q.1 <= e1.1.max(e2.1)
            };
            if (o1 * o2 < 0 && o3 * o4 < 0)
                || within(o1, p1, p2, q1)
                || within(o2, p1, p2, q2)
                || within(o3, q1, q2, p1)
                || within(o4, q1, q2, p2)
            {
                return Some((a, b));
            }
        }
    }
    None
}

/// N2-3a (spec `n2_stage4_junction_cluster_merge` §3): exact resolution
/// context for one disc face's rim chords, built once per handled pair.
/// Carries the disc polygon's rim sub-chords and the OTHER input polygon's
/// boundary sub-segments as exact rationals (classification is exact — no
/// tolerance), plus the disc's exact rim `Curve::Circle` geometry
/// (`disc_circle_edge`) snapped into the pair's canonical cap plane.
struct RimChordCtx {
    /// The disc's rim sub-chords (consecutive rim-ring samples), exact 2D.
    chords: Vec<(ExactPoint2, ExactPoint2)>,
    /// The OTHER input's boundary sub-segments (outer ring + holes), exact 2D.
    other_segs: Vec<(ExactPoint2, ExactPoint2)>,
    /// The exact rim circle's center, snapped onto the pair plane (identity
    /// for bit-exact coplanar input) so both minting branches stay in the
    /// cap plane.
    center: Point3,
    /// The exact rim circle's radius.
    radius: f64,
}

/// Build the N2-3a mint contexts for face `fi` of `brep` — ONE
/// [`RimChordCtx`] per rim circle (`poly` is the face's in-frame polygon,
/// `other` the partner face's). A plain disc yields exactly one (the outer
/// rim — byte-identical to the historical single-ctx path); an annular face
/// (M8 holed-disc increment 6, task #62) yields outer + one PER HOLE rim,
/// each with its own chord ring and exact circle, sharing the partner
/// polygon's boundary sub-segments. Empty for a non-disc/non-annular face
/// or a non-finite coordinate (→ the caller falls through to the raw lift,
/// byte-identical to the pre-N2-3a path). Without the annular arm, crossing
/// vertices on an annular face's rim chords resolved to raw CHORD lifts —
/// off-circle by the Stage-1 sagitta — populating its rim overrides with
/// on-chord points that reach chained outputs as mixed on-circle/on-chord
/// rims (the cut-3 re-entry wall + F0087/88/90 VertexOffSurface class).
fn rim_chord_ctxs(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    other: &PolygonWithHoles,
    frame: &Frame,
) -> Vec<RimChordCtx> {
    let ring_exact = |ring: &[Point2]| -> Option<Vec<(ExactPoint2, ExactPoint2)>> {
        let n = ring.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let s = &ring[i];
            let e = &ring[(i + 1) % n];
            out.push((
                ExactPoint2::from_f64(s.x(), s.y())?,
                ExactPoint2::from_f64(e.x(), e.y())?,
            ));
        }
        Some(out)
    };
    // (ring, circle edge) per rim of the face, in loop order (outer first).
    let mut rims: Vec<(&[Point2], u32)> = Vec::new();
    if let Some(e) = disc_circle_edge(brep, fi) {
        rims.push((poly.outer.as_slice(), e));
    } else if let Some((outer_e, hole_es)) = annular_disc_face(brep, fi) {
        rims.push((poly.outer.as_slice(), outer_e));
        for (k, &he) in hole_es.iter().enumerate() {
            let Some(h) = poly.holes.get(k) else {
                return Vec::new();
            };
            rims.push((h.as_slice(), he));
        }
    } else {
        return Vec::new();
    }
    let other_segs = {
        let Some(mut segs) = ring_exact(&other.outer) else {
            return Vec::new();
        };
        for h in &other.holes {
            let Some(hs) = ring_exact(h) else {
                return Vec::new();
            };
            segs.extend(hs);
        }
        segs
    };
    let mut out = Vec::with_capacity(rims.len());
    for (ring, e) in rims {
        let Curve::Circle { center, radius, .. } = brep.edges()[e as usize].curve else {
            return Vec::new();
        };
        let Some(chords) = ring_exact(ring) else {
            return Vec::new();
        };
        out.push(RimChordCtx {
            chords,
            other_segs: other_segs.clone(),
            center: frame.snap(center),
            radius,
        });
    }
    out
}

/// M8-mixed analog of [`rim_chord_ctxs`] (spec
/// `m8_mixed_loop_coplanar_overlay` amendment 1): ONE [`RimChordCtx`] per
/// curved EDGE of a mixed face, its chord set = the ring sub-chords `masks`
/// attributes to that edge (an arc contributes its chain's chords; a
/// full-circle loop its whole ring). The minting/resolution machinery is
/// shared unchanged — each ctx carries its own exact circle.
fn mixed_chord_ctxs(
    brep: &BRep,
    poly: &PolygonWithHoles,
    masks: &[Vec<Option<u32>>],
    other: &PolygonWithHoles,
    frame: &Frame,
) -> Vec<RimChordCtx> {
    let exact_seg = |s: &Point2, e: &Point2| -> Option<(ExactPoint2, ExactPoint2)> {
        Some((
            ExactPoint2::from_f64(s.x(), s.y())?,
            ExactPoint2::from_f64(e.x(), e.y())?,
        ))
    };
    let other_segs = {
        let mut segs = Vec::new();
        for ring in std::iter::once(&other.outer).chain(other.holes.iter()) {
            let n = ring.len();
            for i in 0..n {
                let Some(seg) = exact_seg(&ring[i], &ring[(i + 1) % n]) else {
                    return Vec::new();
                };
                segs.push(seg);
            }
        }
        segs
    };
    // Chords grouped per curved edge, in first-appearance order.
    let mut edge_order: Vec<u32> = Vec::new();
    let mut chords_of: BTreeMap<u32, Vec<(ExactPoint2, ExactPoint2)>> = BTreeMap::new();
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(masks)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        for i in 0..n {
            let Some(e) = mask[i] else { continue };
            let Some(seg) = exact_seg(&ring[i], &ring[(i + 1) % n]) else {
                return Vec::new();
            };
            if !edge_order.contains(&e) {
                edge_order.push(e);
            }
            chords_of.entry(e).or_default().push(seg);
        }
    }
    let mut out = Vec::with_capacity(edge_order.len());
    for e in edge_order {
        let Curve::Circle { center, radius, .. } = brep.edges()[e as usize].curve else {
            return Vec::new();
        };
        out.push(RimChordCtx {
            chords: chords_of.remove(&e).unwrap_or_default(),
            other_segs: other_segs.clone(),
            center: frame.snap(center),
            radius,
        });
    }
    out
}

/// Outcome of [`resolve_rim_chord_vertex`].
enum RimResolve {
    /// Not strictly interior to any rim sub-chord — resolve as before.
    NotOnChord,
    /// Minted on the exact rim circle (I1), in the cap plane. `crossing` is
    /// true for the circle∩line branch (the point is a transversal junction
    /// with another input's edge — I2 pins it to that edge, so a sub-floor
    /// shared-mint group prefers it as the collapse target) and false for a
    /// pure x-event radial projection.
    OnCircle { point: Point3, crossing: bool },
    /// The exact discriminant of the circle∩line quadratic is negative for a
    /// claimed rim×other-edge crossing — a loud Stage-0 stop (spec §6).
    NoIntersection,
}

/// N2-3a: resolve one overlay vertex that may lie on a disc-rim chord (spec
/// §3 branch table, [#24 §4.5.5] — overlap boundaries carry exact curve
/// geometry). Uses the SAME exact on-chord predicate as
/// `collect_rim_crossings` (exact rational collinearity + strictly-interior
/// parameter with the 1e-6 endpoint margin — a vertex inside the margin is a
/// reconstructed rim sample, reconciled by the rim ULP-snap upstream).
fn resolve_rim_chord_vertex(
    ctx: &RimChordCtx,
    q: &ExactPoint2,
    qx: f64,
    qy: f64,
    frame: &Frame,
) -> RimResolve {
    // ── On a rim sub-chord? (the `collect_rim_crossings` predicate) ─────
    let mut on_chord: Option<usize> = None;
    for (ci, (s2, e2)) in ctx.chords.iter().enumerate() {
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let wx = &q.x - &s2.x;
        let wy = &q.y - &s2.y;
        if &dx * &wy - &dy * &wx != RBig::ZERO {
            continue;
        }
        let t = (&dx * &wx + &dy * &wy) / &len2;
        let tf = t.to_f64().value();
        if tf > 1.0e-6 && tf < 1.0 - 1.0e-6 {
            on_chord = Some(ci);
            break;
        }
    }
    let Some(ci) = on_chord else {
        return RimResolve::NotOnChord;
    };
    let (cs, ce) = &ctx.chords[ci];
    let cdx = &ce.x - &cs.x;
    let cdy = &ce.y - &cs.y;

    // ── Also on another input's edge sub-segment (exact, transversal)? ──
    // A crossing must be minted at the exact circle∩line intersection (I2):
    // radial projection would slide it off the other input's edge, breaking
    // that solid's edge-split propagation. An other-edge COLLINEAR with the
    // chord defines no transversal junction and is skipped (the vertex then
    // radially projects like a pure subdivision point).
    let mut crossing: Option<(&ExactPoint2, RBig, RBig)> = None;
    for (s2, e2) in &ctx.other_segs {
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let wx = &q.x - &s2.x;
        let wy = &q.y - &s2.y;
        if &dx * &wy - &dy * &wx != RBig::ZERO {
            continue;
        }
        let t = (&dx * &wx + &dy * &wy) / &len2;
        if t < RBig::ZERO || t > RBig::ONE {
            continue;
        }
        if &cdx * &dy - &cdy * &dx == RBig::ZERO {
            continue;
        }
        crossing = Some((s2, dx, dy));
        break;
    }

    if let Some((s2, dx, dy)) = crossing {
        // ── Exact 2D circle∩line intersection (spec §3 row 4, I2) ───────
        // Line p(t) = s + t·d against circle |p − c|² = r²: the quadratic
        // a·t² + b·t + c₀ = 0 with exact rational coefficients; the
        // discriminant sign is decided EXACTLY (spec §6), the root itself
        // via one f64 square root (closed-form, ~ULP accuracy — the same
        // class as the opposite-rim exact-radius projection).
        let (cu, cv) = frame.project(ctx.center);
        let (Some(cc), Ok(rr)) = (ExactPoint2::from_f64(cu, cv), rat(ctx.radius)) else {
            return RimResolve::NotOnChord;
        };
        let fx = &s2.x - &cc.x;
        let fy = &s2.y - &cc.y;
        let a_q = &dx * &dx + &dy * &dy;
        let b_q = (&dx * &fx + &dy * &fy) * RBig::from(2);
        let c_q = &fx * &fx + &fy * &fy - &rr * &rr;
        let disc = &b_q * &b_q - RBig::from(4) * (&a_q * &c_q);
        if disc < RBig::ZERO {
            return RimResolve::NoIntersection;
        }
        let a_f = a_q.to_f64().value();
        let b_f = b_q.to_f64().value();
        let c_f = c_q.to_f64().value();
        let sq = disc.to_f64().value().sqrt();
        // Numerically stable root pair (no catastrophic −b ± √D cancellation).
        let qq = if b_f >= 0.0 {
            -(b_f + sq) / 2.0
        } else {
            -(b_f - sq) / 2.0
        };
        let (t1, t2) = if qq != 0.0 {
            (qq / a_f, c_f / qq)
        } else {
            (0.0, 0.0)
        };
        let (sxf, syf) = (s2.x.to_f64().value(), s2.y.to_f64().value());
        let (dxf, dyf) = (dx.to_f64().value(), dy.to_f64().value());
        let p_at = |t: f64| [sxf + t * dxf, syf + t * dyf];
        // Choose the root on THIS chord's parameter interval (spec §3); if
        // that is ambiguous (a near-tangent line can put both roots over one
        // chord), the root nearest the overlay's exact chord crossing — the
        // two are within a sagitta of each other — disambiguates.
        let (csx, csy) = (cs.x.to_f64().value(), cs.y.to_f64().value());
        let (cdxf, cdyf) = (cdx.to_f64().value(), cdy.to_f64().value());
        let clen2 = cdxf * cdxf + cdyf * cdyf;
        let t_chord = |pp: [f64; 2]| ((pp[0] - csx) * cdxf + (pp[1] - csy) * cdyf) / clen2;
        let d2q = |pp: [f64; 2]| (pp[0] - qx) * (pp[0] - qx) + (pp[1] - qy) * (pp[1] - qy);
        let (p1, p2) = (p_at(t1), p_at(t2));
        let in1 = (0.0..=1.0).contains(&t_chord(p1));
        let in2 = (0.0..=1.0).contains(&t_chord(p2));
        let chosen = match (in1, in2) {
            (true, false) => p1,
            (false, true) => p2,
            _ => {
                if d2q(p1) <= d2q(p2) {
                    p1
                } else {
                    p2
                }
            }
        };
        return RimResolve::OnCircle {
            point: frame.lift(chosen[0], chosen[1]),
            crossing: true,
        };
    }

    // ── Pure x-event subdivision (spec §3 row 5, I1): radial projection ──
    // onto the exact circle in the cap plane — the own-cap analog of the
    // opposite-rim exact-radius projection (`opp_radius` below):
    // center + radius·normalize(lift(q) − center).
    let c3 = ctx.center.as_array();
    let l3 = frame.lift(qx, qy).as_array();
    let w = [l3[0] - c3[0], l3[1] - c3[1], l3[2] - c3[2]];
    let n = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    if n == 0.0 {
        // Degenerate (chord through the exact center — impossible for a
        // sampled rim): fall through unchanged rather than divide by zero.
        return RimResolve::NotOnChord;
    }
    let s = ctx.radius / n;
    RimResolve::OnCircle {
        point: Point3::new(c3[0] + w[0] * s, c3[1] + w[1] * s, c3[2] + w[2] * s),
        crossing: false,
    }
}

/// The cylinder lateral incident to a cap's circle edge, the OPPOSITE rim
/// edge, and the cylinder's axis params. The cap's circle edge appears in
/// exactly one `Surface::Cylinder` face's loops; that lateral's OTHER full-
/// circle rim is the opposite cap's edge.
///
/// Returns `Err(tag)` (→ the caller raises the loud residue) if the cap is not
/// a clean 2-rim cylinder cap (no incident cylinder lateral, or the lateral
/// does not have exactly two full-circle rims).
type LateralForCap = (usize, u32, [f64; 3], [f64; 3], f64);

fn lateral_for_cap(brep: &BRep, cap_edge: u32) -> Result<LateralForCap, &'static str> {
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        if !f.outer_loop.contains(&cap_edge) {
            continue;
        }
        // Full-circle rims of this lateral.
        let rims: Vec<u32> = f
            .outer_loop
            .iter()
            .copied()
            .filter(|&e| {
                let ed = &brep.edges()[e as usize];
                matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
            })
            .collect();
        // Dedup (the lateral loop lists the seam twice but each rim once).
        let mut uniq = rims.clone();
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() != 2 {
            return Err("rim-lateral-not-2rim");
        }
        let Some(&opposite) = uniq.iter().find(|&&e| e != cap_edge) else {
            return Err("rim-lateral-no-opposite");
        };
        return Ok((
            fi,
            opposite,
            axis_point.as_array(),
            normalize3(axis_dir.as_array()),
            radius,
        ));
    }
    if std::env::var_os("YANG_RIMLAT_PROBE").is_some() {
        for (fi, f) in brep.faces().iter().enumerate() {
            let in_outer = f.outer_loop.contains(&cap_edge);
            let in_inner = f.inner_loops.iter().any(|l| l.contains(&cap_edge));
            if in_outer || in_inner {
                eprintln!(
                    "[rimlat-probe] cap_edge={cap_edge} face={fi} outer={in_outer} \
                     inner={in_inner} surface={:?}",
                    f.surface
                );
            }
        }
    }
    Err("rim-lateral-none")
}

/// PR-M8 disc-rim crossing (§4.5.5 shared sampling for a CROSSING disc rim):
/// for each overlay vertex strictly interior to one of the disc rim polygon's
/// sub-chords, resolve it to its BIT-EXACT shared 3D point (`coords[vi]` — the
/// SAME point the cap override uses, so no T-junction) and record it on the
/// cap rim edge; also project that crossing's azimuth (in the cylinder axis
/// frame) onto the OPPOSITE rim circle and record the exact-radius point there
/// (so the opposite cap + the lateral stay conformal).
fn collect_rim_crossings(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    // Disc: one rim (the outer circle). Annular (M8 holed-disc): the outer rim
    // PLUS each hole rim — each propagated into ITS OWN cylinder lateral +
    // opposite rim via `lateral_for_cap(rim_edge)`. `poly.holes[k]` corresponds
    // to `annular_disc_face`'s hole-edge `k` (both follow `f.inner_loops` order;
    // `face_polygon_2d_tessellated` builds the hole rings in that order).
    if let Some(cap_edge) = disc_circle_edge(brep, fi) {
        return collect_ring_crossings(brep, cap_edge, &poly.outer, overlay, coords, rim_overrides);
    }
    if let Some((outer_edge, hole_edges)) = annular_disc_face(brep, fi) {
        collect_ring_crossings(
            brep,
            outer_edge,
            &poly.outer,
            overlay,
            coords,
            rim_overrides,
        )?;
        for (k, &he) in hole_edges.iter().enumerate() {
            let ring = poly.holes.get(k).ok_or("rim-hole-count-mismatch")?;
            collect_ring_crossings(brep, he, ring, overlay, coords, rim_overrides)?;
        }
        return Ok(());
    }
    Err("rim-not-disc")
}

/// Propagate the overlay's rim-chord split points for ONE circular rim
/// (`cap_edge`) into that rim's override AND its cylinder's opposite rim (so the
/// shared lateral stays conformal). Called once per rim by
/// [`collect_rim_crossings`] (outer + each hole for an annular cap).
fn collect_ring_crossings(
    brep: &BRep,
    cap_edge: u32,
    ring: &[Point2],
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    // The cap circle's own geometry is not needed (crossing points come from
    // the resolved `coords`); only the OPPOSITE rim + the cylinder axis are.
    let (_lat_fi, opp_edge, axis_point, axis_dir, _r) = lateral_for_cap(brep, cap_edge)?;
    let Curve::Circle {
        center: opp_center,
        normal: opp_normal,
        radius: opp_radius,
    } = brep.edges()[opp_edge as usize].curve
    else {
        return Err("rim-opp-not-circle");
    };

    let n = ring.len();
    if n < 2 {
        return Err("rim-poly-degenerate");
    }
    let cap_entry = rim_overrides.entry(cap_edge).or_default();
    // Collected as (chord index, exact chord parameter, point) and sorted
    // before pushing (spec `m8_holed_disc_coplanar_overlay` §8 F1): the
    // override insertion order is then the EXACT boundary order along the rim
    // polygon, not the overlay-vertex enumeration order. Ring correctness no
    // longer depends on it (the ring sort has an exact tie-break), but the
    // deterministic order keeps probes readable and future consumers safe.
    let mut found: Vec<(usize, RBig, Point3)> = Vec::new();
    for i in 0..n {
        let s = &ring[i];
        let e = &ring[(i + 1) % n];
        let (Some(s2), Some(e2)) = (
            ExactPoint2::from_f64(s.x(), s.y()),
            ExactPoint2::from_f64(e.x(), e.y()),
        ) else {
            continue;
        };
        let dx = &e2.x - &s2.x;
        let dy = &e2.y - &s2.y;
        let len2 = &dx * &dx + &dy * &dy;
        if len2 == RBig::ZERO {
            continue;
        }
        let rim_probe = std::env::var_os("YANG_SPLIT_PROBE").is_some();
        for (vi, q) in overlay.exact_verts.iter().enumerate() {
            let wx = &q.x - &s2.x;
            let wy = &q.y - &s2.y;
            // Exact collinearity with the sub-chord's supporting line.
            if &dx * &wy - &dy * &wx != RBig::ZERO {
                continue;
            }
            // Strictly interior parameter, away from BOTH endpoints.
            let t = (&dx * &wx + &dy * &wy) / &len2;
            let tf = t.to_f64().value();
            if !(tf > 1.0e-6 && tf < 1.0 - 1.0e-6) {
                // M-C diagnosis probe (read-only, env-gated): report the
                // exactly-collinear chord vertices the endpoint window skips.
                if rim_probe && tf > 0.0 && tf < 1.0 {
                    eprintln!(
                        "[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} \
                         SKIPPED (endpoint window)"
                    );
                }
                continue;
            }
            // The BIT-EXACT shared point (the cap override uses the same one).
            let pt = coords[vi];
            if found.iter().any(|(_, _, p)| *p == pt) {
                if rim_probe {
                    eprintln!(
                        "[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} \
                         SKIPPED (duplicate pt)"
                    );
                }
                continue;
            }
            if rim_probe {
                eprintln!("[rim-cross-probe] edge={cap_edge} chord {i} vert {vi} t={tf} KEPT");
            }
            found.push((i, t, pt));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let cap_pts: Vec<Point3> = found.into_iter().map(|(_, _, p)| p).collect();
    for &pt in &cap_pts {
        if !cap_entry.contains(&pt) {
            cap_entry.push(pt);
        }
    }

    // Place each cap crossing onto the OPPOSITE rim by EXACT AXIAL PROJECTION:
    // strip the point's axial component and re-attach the radial offset at the
    // opposite rim's plane/radius. This is a direct 1:1 map (NO azimuth grid
    // search) — so it preserves the cap set's cardinality EXACTLY, including
    // femto-close split pairs, giving the two rims of the shared lateral matched
    // sample counts (the azimuth-merge conformality requirement). The old
    // 720-step f64 grid search collapsed femto-close azimuths to a single theta,
    // desynchronising the rims (18 cap → 12 opp — the M8 holed-disc `24 vs 30`
    // azimuth-merge wall). Radial magnitude is renormalised to `opp_radius`, so
    // this is exact for equal AND unequal cap/opposite radii.
    let oc = opp_center.as_array();
    let _ = opp_normal; // opposite plane is fixed by `oc`; normal no longer used
    let opp_entry = rim_overrides.entry(opp_edge).or_default();
    for &pt in &cap_pts {
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
            // A rim point should never sit on the axis; if it does the geometry
            // is degenerate — skip rather than mint a NaN (P9: no silent bad pt).
            continue;
        }
        let scale = opp_radius / rlen;
        let opp_pt = Point3::new(
            oc[0] + radial[0] * scale,
            oc[1] + radial[1] * scale,
            oc[2] + radial[2] * scale,
        );
        if !opp_entry.contains(&opp_pt) {
            opp_entry.push(opp_pt);
        }
    }
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[rim-count] cap_edge={cap_edge} cap_pts={} cap_entry={} opp_edge={opp_edge} opp_entry={}",
            cap_pts.len(),
            rim_overrides.get(&cap_edge).map(|v| v.len()).unwrap_or(0),
            rim_overrides.get(&opp_edge).map(|v| v.len()).unwrap_or(0),
        );
    }
    Ok(())
}

/// M8-mixed (spec `m8_mixed_loop_coplanar_overlay` amendment 1): propagate
/// the overlay's curved-chord split points of a MIXED face into the curved
/// edges' chains. Per curved edge:
/// - a FULL-CIRCLE loop delegates to [`collect_ring_crossings`] (the disc
///   machinery: own rim + opposite rim of the shared cylinder);
/// - an ARC inserts each split point into its own chain AND, by the same
///   exact axial projection the ring path uses, into the OPPOSITE arc of the
///   shared partial-strip lateral — the strip pairs its two chains
///   index-for-index, so both must gain the point.
fn collect_mixed_crossings(
    brep: &BRep,
    fi: usize,
    poly: &PolygonWithHoles,
    seg_edges: &[Vec<Option<u32>>],
    overlay: &ClassifiedOverlay,
    coords: &[Point3],
    rim_overrides: &mut RimSplitMap,
) -> Result<(), &'static str> {
    for (ring, mask) in std::iter::once(&poly.outer)
        .chain(poly.holes.iter())
        .zip(seg_edges)
    {
        let n = ring.len();
        if n < 2 || mask.len() != n {
            continue;
        }
        // Curved edges of this ring, first-appearance order (deterministic).
        let mut curved: Vec<u32> = Vec::new();
        for e in mask.iter().flatten() {
            if !curved.contains(e) {
                curved.push(*e);
            }
        }
        for &e in &curved {
            let be = &brep.edges()[e as usize];
            if be.start == be.end {
                // Full-circle loop: the ring IS this edge's polyline — the
                // disc-rim propagation applies wholesale (own + opposite rim
                // of the cylinder found via `lateral_for_cap`).
                collect_ring_crossings(brep, e, ring, overlay, coords, rim_overrides)?;
                continue;
            }
            // ARC: gather split points strictly interior to THIS edge's
            // chords ((chord index, exact parameter) sorted — boundary order).
            let mut found: Vec<(usize, RBig, Point3)> = Vec::new();
            for i in 0..n {
                if mask[i] != Some(e) {
                    continue;
                }
                let s = &ring[i];
                let ee = &ring[(i + 1) % n];
                let (Some(s2), Some(e2)) = (
                    ExactPoint2::from_f64(s.x(), s.y()),
                    ExactPoint2::from_f64(ee.x(), ee.y()),
                ) else {
                    continue;
                };
                let dx = &e2.x - &s2.x;
                let dy = &e2.y - &s2.y;
                let len2 = &dx * &dx + &dy * &dy;
                if len2 == RBig::ZERO {
                    continue;
                }
                for (vi, q) in overlay.exact_verts.iter().enumerate() {
                    let wx = &q.x - &s2.x;
                    let wy = &q.y - &s2.y;
                    if &dx * &wy - &dy * &wx != RBig::ZERO {
                        continue;
                    }
                    let t = (&dx * &wx + &dy * &wy) / &len2;
                    let tf = t.to_f64().value();
                    if !(tf > 1.0e-6 && tf < 1.0 - 1.0e-6) {
                        continue;
                    }
                    let pt = coords[vi];
                    if found.iter().any(|(_, _, p)| *p == pt) {
                        continue;
                    }
                    found.push((i, t, pt));
                }
            }
            if found.is_empty() {
                continue;
            }
            found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let pts: Vec<Point3> = found.into_iter().map(|(_, _, p)| p).collect();

            // Classify the shared lateral (spec `m8_mixed_arc_lateral_holed`
            // branch table): a structured 2-arc strip needs PAIRED insertion;
            // a chain-consuming (holed CDT) lateral takes the point from the
            // arc's own chain — one-sided.
            let lateral = arc_lateral_opposite(brep, fi, e)?;

            let cap_entry = rim_overrides.entry(e).or_default();
            for &pt in &pts {
                if !cap_entry.contains(&pt) {
                    cap_entry.push(pt);
                }
            }
            let ArcLateral::Strip {
                opp_edge,
                axis_point,
                axis_dir,
                opp_center,
                opp_radius,
            } = lateral
            else {
                // Chain-consuming lateral (`tessellate_lateral_holed_cdt`
                // splices every boundary loop from the shared per-edge
                // chains via `loop_polyline`): the inserted point is
                // consumed automatically and conformally — no strip
                // index-pairing constraint, so no opposite-arc projection.
                continue;
            };
            // Exact axial projection onto the opposite arc (the
            // `collect_ring_crossings` map: strip the axial component,
            // renormalise the radial offset to the opposite radius).
            let oc = opp_center;
            let opp_entry = rim_overrides.entry(opp_edge).or_default();
            for &pt in &pts {
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
                let rlen =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                if rlen < cad_primitives::TAU_WORK {
                    continue;
                }
                let scale = opp_radius / rlen;
                let opp_pt = Point3::new(
                    oc[0] + radial[0] * scale,
                    oc[1] + radial[1] * scale,
                    oc[2] + radial[2] * scale,
                );
                if !opp_entry.contains(&opp_pt) {
                    opp_entry.push(opp_pt);
                }
            }
        }
    }
    Ok(())
}

/// Classification of the lateral adjacent to a mixed face's arc edge — how a
/// crossing split point must be propagated into its tessellation (spec
/// `m8_mixed_arc_lateral_holed` §2).
enum ArcLateral {
    /// Structured 2-arc partial strip: its tessellation pairs the two arc
    /// chains index-for-index, so insertion must be PAIRED (own chain + exact
    /// axial projection onto the opposite arc).
    Strip {
        opp_edge: u32,
        axis_point: [f64; 3],
        axis_dir: [f64; 3],
        opp_center: [f64; 3],
        opp_radius: f64,
    },
    /// Holed cylinder lateral routed through the KV14 unroll+CDT path
    /// (`tessellate_lateral_holed_cdt`), which splices every boundary loop
    /// from the shared per-edge chains via `loop_polyline`: an inserted chain
    /// point is consumed automatically — one-sided insertion suffices.
    ChainConsuming,
}

/// Find and classify the CYLINDER lateral adjacent to arc edge `e` of mixed
/// face `fi` (see [`ArcLateral`]). Loud typed tags for the unsupported
/// shapes: non-cylinder lateral; a holed lateral with a loop the CDT path
/// cannot splice (multi-edge loop containing a full-circle rim, or a
/// degree-4 `SurfacePair` edge); a hole-free lateral that is not the
/// structured 2-arc strip.
fn arc_lateral_opposite(brep: &BRep, fi: usize, e: u32) -> Result<ArcLateral, &'static str> {
    for (gi, g) in brep.faces().iter().enumerate() {
        if gi == fi {
            continue;
        }
        let in_loops = std::iter::once(&g.outer_loop)
            .chain(g.inner_loops.iter())
            .flatten()
            .any(|&ge| ge == e);
        if !in_loops {
            continue;
        }
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } = g.surface
        else {
            return Err("mixed-arc-lateral-not-cylinder");
        };
        if !g.inner_loops.is_empty() {
            // Holed lateral → Stage 1 routes it to the unroll+CDT path,
            // which consumes the arc's own chain — IF every loop is
            // `loop_polyline`-spliceable (spec branch 2 vs 3). A loop it
            // cannot splice would turn the typed capability wall into a
            // Stage-1 `MalformedTopology` ERROR, so verify here.
            let spliceable = std::iter::once(&g.outer_loop)
                .chain(g.inner_loops.iter())
                .all(|lp| {
                    let closed_single = lp.len() == 1 && {
                        let ed = &brep.edges()[lp[0] as usize];
                        matches!(ed.curve, Curve::Circle { .. } | Curve::Ellipse { .. })
                            && ed.start == ed.end
                    };
                    closed_single
                        || lp.iter().all(|&ge| {
                            let ed = &brep.edges()[ge as usize];
                            match ed.curve {
                                Curve::LineSegment => true,
                                Curve::Circle { .. } | Curve::Ellipse { .. } => ed.start != ed.end,
                                _ => false,
                            }
                        })
                });
            if !spliceable {
                return Err("mixed-arc-lateral-holed");
            }
            return Ok(ArcLateral::ChainConsuming);
        }
        let arcs: Vec<u32> = g
            .outer_loop
            .iter()
            .copied()
            .filter(|&ge| {
                let edge = &brep.edges()[ge as usize];
                matches!(edge.curve, Curve::Circle { .. }) && edge.start != edge.end
            })
            .collect();
        if arcs.len() != 2 || !arcs.contains(&e) {
            return Err("mixed-arc-lateral-unpaired");
        }
        let opp = if arcs[0] == e { arcs[1] } else { arcs[0] };
        let Curve::Circle { center, radius, .. } = brep.edges()[opp as usize].curve else {
            return Err("mixed-arc-lateral-unpaired");
        };
        let ap = axis_point.as_array();
        let ad = normalize3(axis_dir.as_array());
        return Ok(ArcLateral::Strip {
            opp_edge: opp,
            axis_point: ap,
            axis_dir: ad,
            opp_center: center.as_array(),
            opp_radius: radius,
        });
    }
    Err("mixed-arc-no-lateral")
}

/// Like [`face_polygon_2d`], but a flat circular DISC face is tessellated to its
/// Result of [`face_polygon_2d_tessellated`]: the in-frame 2D polygon, a
/// corner→vertex-index key map, a rim-key→3D-point map (empty for line
/// loops), and — for a MIXED Line+Arc face only (spec
/// `m8_mixed_loop_coplanar_overlay`) — per-ring sub-chord edge attribution
/// (`segs[0]` = outer, `segs[1..]` = holes; `segs[r][i] = Some(e)` ⇔ the
/// segment ring[i]→ring[i+1] lies on curved B-Rep edge `e`, `None` ⇔ a
/// straight edge). Empty ⇔ not a mixed face — disc / annular / all-segment
/// faces keep their existing paths.
type TessellatedFacePolygon = (
    PolygonWithHoles,
    BTreeMap<ExactPoint2, u32>,
    BTreeMap<ExactPoint2, Point3>,
    Vec<Vec<Option<u32>>>,
);

/// §2b in-frame coordinate clustering (spec `m8_shared_boundary_identity`
/// C1-C3, I7/I8): snap projected u values (and, independently, v values)
/// that agree within `band` — across ALL the pair's polygons — to the
/// cluster's FIRST-SEEN representative (an original projected value, never
/// an average). The f64 frame projection rounds each vertex independently,
/// so an OBLIQUE solid's intended-frame-vertical edge lands ~1e-16 off
/// vertical even when its world coordinates are consistent; the exact
/// overlay then faithfully builds femto sweep slabs → needle cells →
/// `RoundingCollapse` / femto-twin split points. Clustering makes
/// intended-equal frame coordinates BIT-equal across the pair (§4.5.5
/// identical boundary sampling in the overlay's own domain).
///
/// Deterministic order: `polys` in slice order, each polygon's `outer` then
/// `holes`, vertices in loop order. Clusters are isolated (real features
/// are ≥ MIN_FEATURE_SIZE apart, six orders above the band — the KV10
/// margin), so greedy first-seen matching cannot chain-drift.
///
/// Test-only since §2c: production wires `cluster_frame_coords_rim_aware`
/// directly (pure-polygon pairs pass empty `rim_excluded`). This wrapper is
/// retained as the §2b reference path the C4d guard compares against.
#[cfg(test)]
fn cluster_frame_coords(polys: &mut [&mut PolygonWithHoles], band: f64) {
    // §2b behavior = §2c rim-aware clustering with NO excluded rim coordinates;
    // delegating keeps the two paths byte-identical for pure-polygon pairs (the
    // C4d guard is the arbiter).
    cluster_frame_coords_rim_aware(polys, &[], band);
}

/// §2c rim-aware variant of `cluster_frame_coords`
/// (spec `m8_shared_boundary_identity` C4a–C4d, invariant I9). The cluster
/// DOMAIN is the polygon-chain coordinates only: rim sample coordinates
/// (`rim_excluded`, per polygon) are neither cluster members nor seeds, and a
/// polygon coordinate within `band` of a rim sample only is left UNTOUCHED (no
/// cross-domain welding). This structurally avoids both §2b-reverted failure
/// modes (welding rim samples; snapping polygon corners onto rims). With every
/// `rim_excluded` slice empty it is byte-identical to `cluster_frame_coords`.
fn cluster_frame_coords_rim_aware(
    polys: &mut [&mut PolygonWithHoles],
    rim_excluded: &[&[Point2]],
    band: f64,
) {
    for axis in 0..2 {
        // Rim sample coordinate values on this axis — excluded from the cluster
        // domain (C4b): never members, never seeds. A polygon coord within band
        // of any of these is left untouched (C4c).
        let rim_coords: Vec<f64> = rim_excluded
            .iter()
            .flat_map(|rim| rim.iter())
            .map(|pt| if axis == 0 { pt.x() } else { pt.y() })
            .collect();
        let near_rim = |c: f64| rim_coords.iter().any(|r| (*r - c).abs() <= band);

        let mut reps: Vec<f64> = Vec::new();
        for poly in polys.iter_mut() {
            for lp in std::iter::once(&mut poly.outer).chain(poly.holes.iter_mut()) {
                for q in lp.iter_mut() {
                    let c = if axis == 0 { q.x() } else { q.y() };
                    // C4b/C4c: a coordinate within band of a rim sample is
                    // neither snapped nor a seed — left exactly as-is.
                    if near_rim(c) {
                        continue;
                    }
                    match reps.iter().find(|r| (**r - c).abs() <= band) {
                        Some(&r) => {
                            *q = if axis == 0 {
                                Point2::new(r, q.y())
                            } else {
                                Point2::new(q.x(), r)
                            };
                        }
                        None => reps.push(c),
                    }
                }
            }
        }
    }
}

/// exact Stage-1 rim ring. The third return value maps each rim vertex's exact
/// 2D key to its bit-identical 3D rim point (for overlay-vertex → 3D
/// resolution; the cylinder lateral shares that exact ring, keeping the overlap
/// mesh conformal). Line-loop faces return an empty rim map.
fn face_polygon_2d_tessellated(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<TessellatedFacePolygon> {
    if disc_circle_edge(brep, fi).is_some() {
        let rim = disc_rim_ring(brep, fi, coords, frame)?;
        let mut outer = Vec::with_capacity(rim.len());
        let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        for &pt in &rim {
            let (u, v) = frame.project(pt);
            let ex = ExactPoint2::from_f64(u, v)?;
            rim_map.insert(ex, pt);
            outer.push(Point2::new(u, v));
        }
        return Some((
            PolygonWithHoles {
                outer,
                holes: Vec::new(),
            },
            BTreeMap::new(),
            rim_map,
            Vec::new(),
        ));
    }
    // M8 holed-disc (spec `m8_holed_disc_coplanar_overlay`): an ANNULAR cap —
    // outer + hole rims sampled from Stage 1's own tessellation into a
    // `PolygonWithHoles`, with every rim point registered in `rim_map` so the
    // overlay-vertex → exact 3D rim point resolution is T-junction-free.
    if annular_disc_face(brep, fi).is_some() {
        let (outer_ring, hole_rings) = annular_rim_rings(brep, fi, coords, frame)?;
        let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
        let mut project_ring = |ring: &[Point3]| -> Option<Vec<Point2>> {
            let mut out = Vec::with_capacity(ring.len());
            for &pt in ring {
                let (u, v) = frame.project(pt);
                let ex = ExactPoint2::from_f64(u, v)?;
                rim_map.insert(ex, pt);
                out.push(Point2::new(u, v));
            }
            Some(out)
        };
        let outer = project_ring(&outer_ring)?;
        let mut holes = Vec::with_capacity(hole_rings.len());
        for hr in &hole_rings {
            holes.push(project_ring(hr)?);
        }
        return Some((
            PolygonWithHoles { outer, holes },
            BTreeMap::new(),
            rim_map,
            Vec::new(),
        ));
    }
    // M8-mixed (spec `m8_mixed_loop_coplanar_overlay`): a planar face whose
    // loops mix `LineSegment` and `Circle`/`Ellipse` edges (and full-circle
    // loops in non-annular configurations). Splice each loop from Stage 1's
    // OWN per-edge sample chains (§4.5.5 conformality with the adjacent
    // curved laterals): polyline vertices that are B-Rep vertices → `corners`
    // (resolved to the pair's snapped/welded coordinates); chain Steiner
    // samples → `rim_map` (exact 3D points, bit-shared with the laterals).
    // Per-ring masks mark which sub-chords lie on curved edges — the caller's
    // slice-1 gate walls the pair if the overlap boundary subdivides one.
    if mixed_planar_face(brep, fi) {
        return mixed_face_polygon_2d(brep, fi, coords, frame);
    }
    let (poly, corners) = face_polygon_2d(brep, fi, coords, frame)?;
    Some((poly, corners, BTreeMap::new(), Vec::new()))
}

/// Is `fi` a MIXED planar face (spec `m8_mixed_loop_coplanar_overlay` §2):
/// `Surface::Plane`, not a disc, not annular, every loop edge's curve ∈
/// {`LineSegment`, `Circle`}, at least one `Circle`? Ellipse edges stay the
/// `face-unsupported` wall — chord-interior overlay vertices are minted onto
/// the exact CIRCLE ([`RimChordCtx`]); there is no ellipse mint.
fn mixed_planar_face(brep: &BRep, fi: usize) -> bool {
    let f = &brep.faces()[fi];
    if !matches!(f.surface, Surface::Plane { .. }) {
        return false;
    }
    if disc_circle_edge(brep, fi).is_some() || annular_disc_face(brep, fi).is_some() {
        return false;
    }
    let mut any_curved = false;
    for &e in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        match brep.edges()[e as usize].curve {
            Curve::LineSegment => {}
            Curve::Circle { .. } => any_curved = true,
            _ => return false,
        }
    }
    any_curved
}

/// The MIXED-face arm of [`face_polygon_2d_tessellated`]: loop polylines
/// spliced from the face's own Stage-1 tessellation chains.
fn mixed_face_polygon_2d(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<TessellatedFacePolygon> {
    let verts: Vec<BRepVertex> = coords.iter().map(|&p| BRepVertex { point: p }).collect();
    let tess = crate::stage1_tessellate_min_segments(
        &verts,
        brep.edges(),
        brep.faces(),
        brep.forced_rim_n(),
    )
    .ok()?;
    let n_brep_verts = brep.vertices().len() as u32;
    let f = &brep.faces()[fi];
    let is_curved = |e_idx: u32| matches!(brep.edges()[e_idx as usize].curve, Curve::Circle { .. });

    let mut corners: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut rim_map: BTreeMap<ExactPoint2, Point3> = BTreeMap::new();
    let mut masks: Vec<Vec<Option<u32>>> = Vec::with_capacity(1 + f.inner_loops.len());
    let mut project_loop = |lp: &[u32]| -> Option<Vec<Point2>> {
        let attributed =
            crate::loop_polyline_attributed(fi, lp, brep.edges(), &tess.chains).ok()?;
        let mut ring = Vec::with_capacity(attributed.len());
        let mut mask = Vec::with_capacity(attributed.len());
        for &(g, e_idx) in &attributed {
            // Chain Steiner samples live in the tessellation pool; B-Rep
            // vertices resolve through the pair's snapped `coords` (identical
            // values — the tessellation ran on those same coordinates).
            let pt = tess.verts.get(g as usize).copied()?;
            let (u, v) = frame.project(pt);
            let ex = ExactPoint2::from_f64(u, v)?;
            if g < n_brep_verts {
                corners.insert(ex, g);
            } else {
                rim_map.insert(ex, pt);
            }
            ring.push(Point2::new(u, v));
            // The segment STARTING at this vertex lies on its emitting edge.
            mask.push(is_curved(e_idx).then_some(e_idx));
        }
        masks.push(mask);
        Some(ring)
    };

    let outer = project_loop(&f.outer_loop)?;
    let mut holes = Vec::with_capacity(f.inner_loops.len());
    for lp in &f.inner_loops {
        holes.push(project_loop(lp)?);
    }
    Some((PolygonWithHoles { outer, holes }, corners, rim_map, masks))
}

// ════════════════════════════════════════════════════════════════════════
// M8-vertex-canon §2b: in-frame coordinate clustering
// (spec `specs/m8_shared_boundary_identity.md` §2b, FIP Phase 2, RED).
//
// The world-space vertex pass leaves an OBLIQUE pair's PROJECTED frame
// coordinates femto-split (the f64 `(p−o)·e1` rounds independently per
// vertex), so the exact sweep still builds needle cells → `RoundingCollapse`
// (R0076/R0081). A second layer, where the pair's 2D polygons are built
// (`stage0_preprocess`, ~line 336, just before `coplanar_overlay`), clusters
// the projected u (and, independently, v) coordinates of BOTH faces' loop
// vertices to a first-seen representative.
//
// SETTLED SEAM (the implementer matches this; the call site becomes
// `cluster_frame_coords(&mut [&mut poly_a, &mut poly_b], band)` right before
// the `coplanar_overlay` call):
//
//   fn cluster_frame_coords(polys: &mut [&mut PolygonWithHoles], band: f64)
//
// Deterministic order: polys in slice order, each poly's `outer` loop then its
// `holes`, per vertex; the u axis and v axis cluster INDEPENDENTLY; a
// representative is an original projected value (no averaging). These tests do
// NOT compile until that function exists — that IS the RED state.
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod frame_cluster_tests {
    use super::*;

    fn poly(outer: &[(f64, f64)]) -> PolygonWithHoles {
        PolygonWithHoles {
            outer: outer.iter().map(|&(x, y)| Point2::new(x, y)).collect(),
            holes: Vec::new(),
        }
    }

    /// Every coordinate's bits, in loop order (outer then holes) — for
    /// byte-identity comparison.
    fn bits2(p: &PolygonWithHoles) -> Vec<[u64; 2]> {
        std::iter::once(&p.outer)
            .chain(p.holes.iter())
            .flat_map(|lp| lp.iter().map(|pt| [pt.x().to_bits(), pt.y().to_bits()]))
            .collect()
    }

    /// I7 audit oracle: after clustering, NO two coordinates on one axis differ
    /// by a nonzero amount ≤ `band` (twin-free events), across all loops of all
    /// polygons.
    fn assert_no_twin_events(polys: &[&PolygonWithHoles], band: f64) {
        let mut us: Vec<f64> = Vec::new();
        let mut vs: Vec<f64> = Vec::new();
        for p in polys {
            for lp in std::iter::once(&p.outer).chain(p.holes.iter()) {
                for pt in lp {
                    us.push(pt.x());
                    vs.push(pt.y());
                }
            }
        }
        for (axis_name, mut axis) in [("u", us), ("v", vs)] {
            axis.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for w in axis.windows(2) {
                let d = (w[1] - w[0]).abs();
                assert!(
                    d == 0.0 || d > band,
                    "I7: {axis_name}-axis twin event: {} and {} differ by {d:e} ≤ band {band:e}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    /// C1 / I7 / I8 (RED): two projected coords within band (across A and B)
    /// snap to the first-seen representative on the u axis; v untouched; the
    /// far (3.0) cluster untouched (C2); representative is an original member.
    #[test]
    fn red_frame_coords_cluster_to_representative() {
        // u ≈ 1.0 split by 1 and 2 ULPs (~2.2e-16, 4.4e-16) — the measured
        // R0076 femto-crookedness; band 1e-12.
        let u1 = f64::from_bits(1.0f64.to_bits() + 1); // 1.0 + 1 ULP (A)
        let u2 = f64::from_bits(1.0f64.to_bits() + 2); // 1.0 + 2 ULP (B)
        let band = 1e-12;

        let mut a = poly(&[(1.0, 0.0), (u1, 2.0), (3.0, 2.0), (3.0, 0.0)]);
        let mut b = poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]);

        cluster_frame_coords(&mut [&mut a, &mut b], band);

        // C1 / I8: all three near-1.0 u values are BIT-equal to the first-seen
        // representative 1.0 (a member — no averaging).
        let rep = 1.0f64.to_bits();
        assert_eq!(a.outer[0].x().to_bits(), rep, "A[0].u representative");
        assert_eq!(a.outer[1].x().to_bits(), rep, "A[1].u (1 ULP) snaps to rep");
        assert_eq!(b.outer[0].x().to_bits(), rep, "B[0].u (2 ULP) snaps to rep");

        // v is untouched (no femto split on v).
        assert_eq!(a.outer[1].y(), 2.0, "I8: v coordinate not moved");
        // C2: the 3.0 cluster (already exact) stays 3.0.
        assert_eq!(a.outer[2].x(), 3.0, "C2: far cluster untouched");

        // I7: no twin events remain on either axis.
        assert_no_twin_events(&[&a, &b], band);
    }

    /// C3 guard: generic polygons whose coordinates are all exactly equal or
    /// ≫ band apart are byte-identical through the pass.
    #[test]
    fn guard_generic_distinct_polygons_byte_identical() {
        let band = 1e-12;
        let mut a = poly(&[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)]);
        let mut b = poly(&[(10.0, 10.0), (15.0, 10.0), (12.0, 13.0)]);
        let (ba, bb) = (bits2(&a), bits2(&b));

        cluster_frame_coords(&mut [&mut a, &mut b], band);

        assert_eq!(
            bits2(&a),
            ba,
            "C3: distinct polygon A must be byte-identical"
        );
        assert_eq!(
            bits2(&b),
            bb,
            "C3: distinct polygon B must be byte-identical"
        );
    }

    /// Axis-independence guard: a pair split only in v clusters ONLY in v; the
    /// distinct u values (1.0 vs 2.0, ≫ band apart) are left untouched.
    #[test]
    fn guard_v_axis_clusters_independent_of_u() {
        let band = 1e-12;
        let v_twin = f64::from_bits(7.0f64.to_bits() + 3); // 7.0 + 3 ULP (~2.7e-15)
        let mut a = poly(&[(1.0, 7.0), (2.0, v_twin), (2.0, 9.0), (1.0, 9.0)]);

        cluster_frame_coords(&mut [&mut a], band);

        // v: the femto twin snaps to the first-seen representative 7.0.
        assert_eq!(
            a.outer[0].y().to_bits(),
            7.0f64.to_bits(),
            "v representative"
        );
        assert_eq!(a.outer[1].y().to_bits(), 7.0f64.to_bits(), "v twin snaps");
        // u: distinct values are NOT touched by v clustering.
        assert_eq!(a.outer[0].x(), 1.0, "u untouched (independent axis)");
        assert_eq!(a.outer[1].x(), 2.0, "u untouched (independent axis)");
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on cluster_frame_coords: band boundary, first-seen no-drift,
    // A-first determinism, axis independence, representative-is-member. In-module
    // (pub(crate)). Purely additive; touches no existing test.

    /// Band boundary: a coordinate 0.9·band from the representative clusters;
    /// 1.1·band away does NOT (it becomes a new representative). Pins the `<=`
    /// band edge at realistic scale (not just the 1-2 ULP splits above).
    #[test]
    fn adversary_band_boundary_below_clusters_above_new_rep() {
        let band = 1e-12;
        let a = 5.0f64;
        let near = a + 0.9 * band; // within band → clusters
        let far = a + 1.1 * band; // beyond band → new rep
        let mut p = poly(&[(a, 0.0), (near, 1.0), (far, 2.0), (9.0, 3.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(p.outer[0].x().to_bits(), a.to_bits(), "rep untouched");
        assert_eq!(
            p.outer[1].x().to_bits(),
            a.to_bits(),
            "0.9·band coord must snap to the representative"
        );
        assert_eq!(
            p.outer[2].x().to_bits(),
            far.to_bits(),
            "1.1·band coord must stay (its own new representative)"
        );
    }

    /// No chain drift (first-seen semantics). Values a, a+0.9·band, a+1.8·band:
    /// the rep list is FIRST-SEEN, so a+1.8·band is measured against rep `a`
    /// (1.8·band > band) → it becomes its OWN rep and does NOT get pulled into
    /// a's cluster even though it is only 0.9·band from the (snapped) middle
    /// value. This is the isolation property that makes greedy clustering safe.
    #[test]
    fn adversary_first_seen_prevents_chain_drift() {
        let band = 1e-12;
        let a = 5.0f64;
        let mid = a + 0.9 * band;
        let outer = a + 1.8 * band;
        let mut p = poly(&[(a, 0.0), (mid, 1.0), (outer, 2.0), (9.0, 3.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(p.outer[1].x().to_bits(), a.to_bits(), "mid snaps to rep a");
        assert_eq!(
            p.outer[2].x().to_bits(),
            outer.to_bits(),
            "no drift: the far value stays its own rep (measured against a, not mid)"
        );
        assert_no_twin_events(&[&p], band);
    }

    /// A-first determinism: the representative is the FIRST-SEEN value in slice
    /// order (A's loop before B's). Two within-band values a1 (A) and a2 (B) both
    /// resolve to a1 — swapping the slice order would pick a2, so this pins the
    /// documented deterministic ordering.
    #[test]
    fn adversary_a_first_representative_determinism() {
        let band = 1e-12;
        let a1 = 5.0f64;
        let a2 = a1 + 0.5 * band;
        let mut a = poly(&[(a1, 0.0), (8.0, 0.0), (8.0, 2.0)]);
        let mut b = poly(&[(a2, 5.0), (8.0, 5.0), (8.0, 4.0)]);
        cluster_frame_coords(&mut [&mut a, &mut b], band);
        assert_eq!(
            a.outer[0].x().to_bits(),
            a1.to_bits(),
            "A's value is the rep"
        );
        assert_eq!(
            b.outer[0].x().to_bits(),
            a1.to_bits(),
            "B's within-band value adopts A's first-seen representative"
        );
    }

    /// MUTATION KILLER (b) — axes must cluster INDEPENDENTLY. A vertex whose v
    /// coordinate (5.0 + 1 ULP) is within band of a DIFFERENT vertex's u
    /// coordinate (5.0) must NOT cross-snap: production keeps the u and v
    /// representative lists separate (fresh per axis), so v = 5.0+ULP stays. A
    /// SHARED rep list (axes coupled) would pull v onto the u-derived rep 5.0.
    ///
    /// Verified: production → v stays 5.0+ULP; shared-rep-list mutant → v snaps
    /// to 5.0. The existing axis-independence guard does NOT catch this (its v
    /// values are far from any u value); this is the dedicated killer.
    #[test]
    fn adversary_axes_independent_v_near_u_no_cross_snap() {
        let band = 1e-12;
        let v_near_u = f64::from_bits(5.0f64.to_bits() + 1); // 5.0 + 1 ULP (~8.9e-16 < band)
                                                             // u values: 8.0, 8.0, 5.0, 5.0 ; v values: 8.0, v_near_u, 9.0, 9.0.
                                                             // v_near_u is a lone v value (no other v within band) → must stay.
        let mut p = poly(&[(8.0, 8.0), (8.0, v_near_u), (5.0, 9.0), (5.0, 8.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        assert_eq!(
            p.outer[1].y().to_bits(),
            v_near_u.to_bits(),
            "axis independence: a v value near a u value must NOT snap to the u rep"
        );
        // u values are exact and unchanged.
        assert_eq!(p.outer[0].x().to_bits(), 8.0f64.to_bits());
        assert_eq!(p.outer[2].x().to_bits(), 5.0f64.to_bits());
    }

    /// MUTATION KILLER (a) — the representative is an EXACT MEMBER (I8), never an
    /// average. A femto cluster {a, a+1 ULP, a+2 ULP} collapses so every output
    /// coordinate is bit-equal to ONE of the original inputs (here the first-seen
    /// `a`). An averaging representative would emit a value equal to none of the
    /// three inputs.
    #[test]
    fn adversary_representative_is_exact_member_not_average() {
        let band = 1e-12;
        let a = 5.0f64;
        let a1 = f64::from_bits(a.to_bits() + 1);
        let a2 = f64::from_bits(a.to_bits() + 2);
        let inputs: std::collections::BTreeSet<u64> =
            [a, a1, a2].iter().map(|x| x.to_bits()).collect();
        let mut p = poly(&[(a, 0.0), (a1, 1.0), (a2, 2.0)]);
        cluster_frame_coords(&mut [&mut p], band);
        for (i, q) in p.outer.iter().enumerate() {
            assert!(
                inputs.contains(&q.x().to_bits()),
                "I8: clustered u[{i}]={} is not an original member (averaging?)",
                q.x()
            );
        }
        // And specifically the first-seen member.
        assert_eq!(
            p.outer[0].x().to_bits(),
            a.to_bits(),
            "first-seen member is the rep"
        );
        assert_eq!(p.outer[1].x().to_bits(), a.to_bits());
        assert_eq!(p.outer[2].x().to_bits(), a.to_bits());
    }

    // ════════════════════════════════════════════════════════════════════
    // M8-vertex-canon §2c: RIM-AWARE in-frame clustering
    // (spec `specs/m8_shared_boundary_identity.md` §2c, FIP Phase 2, RED).
    //
    // §2b's clustering is scope-limited to PURE-POLYGON pairs (the call site's
    // `cluster_ok = rim_a.is_empty() && rim_b.is_empty()` gate). §2c lifts that:
    // apply the SAME per-axis band clustering to RIM-CARRYING pairs, but restrict
    // the cluster DOMAIN to POLYGON-CHAIN coordinates and EXCLUDE rim sample
    // coordinates entirely — neither cluster members nor seeds (C4a–C4d, I9).
    // This structurally avoids both P10-reverted failure modes (no rim welding;
    // no snapping polygon corners onto rims).
    //
    // SETTLED SEAM (the implementer provides this; these tests do NOT compile
    // until it exists — that IS the RED state, per the §2b precedent above):
    //
    //   fn cluster_frame_coords_rim_aware(
    //       polys: &mut [&mut PolygonWithHoles],
    //       rim_excluded: &[&[Point2]],   // per-poly rim sample coords (u,v),
    //                                     // excluded from the cluster domain
    //       band: f64,
    //   )
    //
    // Contract: cluster the polygons' non-rim coordinates exactly as
    // `cluster_frame_coords` does (per-axis, first-seen representative, A's loop
    // first); a coordinate bit-equal to a rim sample is NEITHER a member nor a
    // seed, and a polygon coordinate within band of ONLY a rim sample is left
    // untouched (C4c). With every `rim_excluded` slice empty the function is
    // byte-identical to `cluster_frame_coords` (C4d).
    // ════════════════════════════════════════════════════════════════════

    fn pts(coords: &[(f64, f64)]) -> Vec<Point2> {
        coords.iter().map(|&(x, y)| Point2::new(x, y)).collect()
    }

    /// I9 / C4a / C4b / C4c (RED): a rim-carrying pair. Both polygon chains carry
    /// an intended-equal frame coordinate split ~1e-16 (must weld — C4a/I9); a
    /// polygon coordinate sits femto-near a RIM sample only (must NOT weld onto it
    /// — C4c); rim samples stay byte-identical (C4b). RED today because the
    /// rim-aware seam does not exist (production skips clustering entirely for
    /// rim-carrying pairs, so the twins would never weld).
    #[test]
    fn red_rim_carrying_clusters_polygon_excludes_rim() {
        let band = 1e-12;
        // Intended-equal chain twins across A and B (1 ULP / 2 ULP off 1.0) — the
        // measured femto-crookedness; both must collapse to the first-seen rep.
        let u1 = f64::from_bits(1.0f64.to_bits() + 1); // A
        let u2 = f64::from_bits(1.0f64.to_bits() + 2); // B
                                                       // A rim sample at u = 4.0, and a POLYGON coord 1 ULP from it (C4c).
        let rho = 4.0f64;
        let near_rim = f64::from_bits(rho.to_bits() + 1);

        let mut a = poly(&[(1.0, 0.0), (u1, 2.0), (near_rim, 2.0), (3.0, 0.0)]);
        let mut b = poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]);

        // A is the rim-carrying face (a disc-cap ring): its rim samples include
        // rho on the u axis. B carries no rim.
        let rim_a = pts(&[(rho, 2.0), (rho, 0.0)]);
        let rim_b: Vec<Point2> = Vec::new();

        cluster_frame_coords_rim_aware(
            &mut [&mut a, &mut b],
            &[rim_a.as_slice(), rim_b.as_slice()],
            band,
        );

        // C4a / I9: the intended-equal chain twins are BIT-equal to the first-seen
        // representative (1.0, A's loop first).
        let rep = 1.0f64.to_bits();
        assert_eq!(a.outer[0].x().to_bits(), rep, "A[0].u representative");
        assert_eq!(a.outer[1].x().to_bits(), rep, "A[1].u (1 ULP) snaps to rep");
        assert_eq!(b.outer[0].x().to_bits(), rep, "B[0].u (2 ULP) snaps to rep");

        // C4c: the polygon coord femto-near a RIM sample only is UNTOUCHED
        // (rim excluded from the domain — no cross-domain welding).
        assert_eq!(
            a.outer[2].x().to_bits(),
            near_rim.to_bits(),
            "C4c: polygon coord within band of a rim sample only must not weld onto it"
        );

        // C4b: rim samples are byte-identical to their input (never members/seeds;
        // the pass must never mutate them).
        assert_eq!(rim_a[0].x().to_bits(), rho.to_bits(), "C4b: rim sample u");
        assert_eq!(
            rim_a[0].y().to_bits(),
            2.0f64.to_bits(),
            "C4b: rim sample v"
        );

        // I9 audit: no twin events remain among the POLYGON coordinates on either
        // axis (the near_rim coord is isolated from other polygon coords, so it is
        // not itself a twin event).
        assert_no_twin_events(&[&a, &b], band);
    }

    /// C4d guard: a PURE-polygon pair (all `rim_excluded` slices empty) through
    /// the rim-aware path is byte-identical to the §2b `cluster_frame_coords`
    /// behavior — no behavior change for the pure-polygon population that §2b
    /// already serves. GREEN once the seam lands (protects §2b byte-identity);
    /// compile-gated with the RED above until then.
    #[test]
    fn guard_c4d_pure_polygon_pair_matches_2b_behavior() {
        let band = 1e-12;
        let u1 = f64::from_bits(1.0f64.to_bits() + 1);
        let u2 = f64::from_bits(1.0f64.to_bits() + 2);
        let mk = || {
            (
                poly(&[(1.0, 0.0), (u1, 2.0), (3.0, 2.0), (3.0, 0.0)]),
                poly(&[(u2, 5.0), (3.0, 5.0), (3.0, 4.0)]),
            )
        };

        // §2b reference path.
        let (mut ra, mut rb) = mk();
        cluster_frame_coords(&mut [&mut ra, &mut rb], band);

        // Rim-aware path with empty rim exclusion (C4d).
        let (mut ca, mut cb) = mk();
        cluster_frame_coords_rim_aware(&mut [&mut ca, &mut cb], &[&[], &[]], band);

        assert_eq!(bits2(&ca), bits2(&ra), "C4d: A byte-identical to §2b path");
        assert_eq!(bits2(&cb), bits2(&rb), "C4d: B byte-identical to §2b path");
    }
}

#[cfg(test)]
mod reloc_tests {
    //! Amendment-5 cavity relocation unit oracles (spec
    //! `n2_stage4_junction_cluster_merge` §3, M8 increment 8). The F0087
    //! engine-frame chain exercises the ear-clip branch end-to-end
    //! (`kernel-v2/tests/m8_swiss_cheese_chain.rs`); these cover the
    //! remaining branch rows in isolation on synthetic triangulations
    //! (P4): fan-with-growth, pinch-defer → ear-clip, and reject with NO
    //! mutation. All fixtures live on the z=0 plane with the identity
    //! frame, so the resolved 3D coords ARE the 2D positions.

    use super::RelocOutcome;
    use super::{gate_tri_valid, relocate_minted_region, relocate_minted_vertex, Frame};
    use crate::coplanar_overlay::RegionClass;
    use cad_primitives::Point3;
    use std::collections::BTreeMap;

    fn frame_z0() -> Frame {
        Frame {
            n: [0.0, 0.0, 1.0],
            d: 0.0,
            o: [0.0, 0.0, 0.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
        }
    }

    fn p(u: f64, v: f64) -> Point3 {
        Point3::new(u, v, 0.0)
    }

    fn edge_map_of(tris: &[[u32; 3]]) -> BTreeMap<[u32; 2], Vec<usize>> {
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        let mut m: BTreeMap<[u32; 2], Vec<usize>> = BTreeMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                m.entry(key(t[k], t[(k + 1) % 3])).or_default().push(ti);
            }
        }
        m
    }

    /// Incident-list ORDER is insertion-dependent and no consumer reads it;
    /// compare edge maps as sets.
    fn canon(m: &BTreeMap<[u32; 2], Vec<usize>>) -> BTreeMap<[u32; 2], Vec<usize>> {
        m.iter()
            .map(|(k, v)| {
                let mut v = v.clone();
                v.sort_unstable();
                (*k, v)
            })
            .collect()
    }

    /// Shared base fixture: v (=0) is a bottom-boundary vertex of the
    /// triangle-ish domain {w0(0,0), w2(4,0), far(2,2)} with one interior
    /// vertex w1(2,0.6). Star of v: (v,w2,w1), (v,w1,w0); non-star:
    /// (w0,w1,far), (w1,w2,far). v's resolved coordinate has been minted
    /// to (0.8, 0.4) — ACROSS the line through link edge (w1,w0), so the
    /// fan triangle (v,w1,w0) folds and the gate's flip repair cannot fix
    /// it (the fold is the only same-class neighbor configuration the
    /// fixture cares about; relocation is called directly here).
    fn base() -> (Vec<[u32; 3]>, Vec<RegionClass>, Vec<Point3>) {
        let tris = vec![[0, 2, 3], [0, 3, 1], [1, 3, 4], [3, 2, 4]];
        let class = vec![RegionClass::AOnly; 4];
        let coords = vec![
            p(0.8, 0.4),
            p(0.0, 0.0),
            p(4.0, 0.0),
            p(2.0, 0.6),
            p(2.0, 2.0),
        ];
        (tris, class, coords)
    }

    /// Branch row 1: all fan triangles valid after ONE visibility-growth
    /// step (the folded link edge (w1,w0) is crossed into its same-class
    /// neighbor, whose apex `far` joins the link) — the fan IS the
    /// re-triangulation and v keeps every spoke.
    #[test]
    fn fan_after_growth_retriangulates_star() {
        let (mut tris, mut class, coords) = base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 3, 1], &coords, &frame),
            "fixture must start folded"
        );
        let minted = vec![true, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Cavity slots (0,1,2 — star + grown neighbor) fan from v over the
        // final link w2→w1→far→w0; slot 3 untouched.
        assert_eq!(tris, vec![[0, 2, 3], [0, 3, 4], [0, 4, 1], [3, 2, 4]]);
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after fan"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
    }

    /// Branch row 3 (reject, no mutation): the same fold, but the growable
    /// neighbor is across a CLASS boundary (the intersection curve). Growth
    /// defers, and the cavity polygon [v,w2,w1,w0] is non-simple under the
    /// minted position (edge v→w2 crosses edge w1→w0), so the ear-clip
    /// rejects — the caller falls back to the amendment-2 revert.
    #[test]
    fn class_blocked_nonsimple_polygon_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        class[2] = RegionClass::Overlap; // (w0,w1,far) across the curve
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        let minted = vec![true, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::NonSimple { .. }
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    /// Branch row 2 (constrained ear-clip): a column-hop strip where growth
    /// pinches (the absorbable neighbor's apex is already on the link), so
    /// the fan is impossible, and the cavity polygon is ear-clipped instead
    /// — v loses its spokes to the hopped column but keeps its two domain-
    /// boundary edges, and the triangulation covers the cavity exactly.
    #[test]
    fn pinch_deferred_cavity_ear_clips() {
        // v(=0) minted to (2.2,-0.3), past the column {b(2,0.5), a(2,1.5)}.
        // Star: (w0,v,tl),(v,a,tl),(v,b,a),(v,w3,b); non-star: (b,w3,tr),
        // (a,b,tr),(tl,a,tr). Growth absorbs (a,b,tr), then the next fold's
        // neighbor (b,w3,tr) has apex w3 already on the link → defer.
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl)
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a)
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr)
            [3, 4, 6], // (a, b, tr)
            [2, 3, 6], // (tl, a, tr)
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded"
        );
        let minted = vec![true, false, false, false, false, false, false];
        assert!(matches!(
            relocate_minted_vertex(
                &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false
            ),
            RelocOutcome::Committed
        ));
        // Cavity = star (4) + absorbed (a,b,tr) = 5 tris, ear-clipped over
        // the polygon [v, w3, b, tr, a, tl, w0]. Slot 6 untouched.
        assert_eq!(tris[6], [2, 3, 6]);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after ear-clip"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // v keeps its domain-boundary edges but no longer spokes to the
        // hopped column.
        let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
        assert!(em.contains_key(&key(0, 1)) && em.contains_key(&key(0, 5)));
        assert!(
            !em.contains_key(&key(0, 4)),
            "spoke to hopped column must be gone"
        );
        // Exact cover: total unsigned area of the 7 triangles equals the
        // domain area (rect 4×2 minus the two boundary notches at v).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        // Rect 8.0 plus the dip of the boundary V at v below y=0:
        // triangle (w0, v, w3) area = 0.5·base(4)·depth(0.3) = 0.6.
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    // ── Amendment 6: joint region relocation (M8 increment 9) ────────────

    /// Region success: the pinch fixture's star-union region (single seed —
    /// the region form must subsume the per-vertex scope) has the closed
    /// boundary cycle [v, w3, b, a, tl, w0], simple and CCW at the minted
    /// position, and ear-clips into exactly region-size triangles with the
    /// edge map maintained. Non-region slots untouched.
    #[test]
    fn region_relocation_retriangulates_star_union() {
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl)
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a)
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr)
            [3, 4, 6], // (a, b, tr)
            [2, 3, 6], // (tl, a, tr)
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded"
        );
        assert!(relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        // Non-region slots untouched.
        assert_eq!(tris[4], [4, 5, 6]);
        assert_eq!(tris[5], [3, 4, 6]);
        assert_eq!(tris[6], [2, 3, 6]);
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after region relocation"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover unchanged (same domain as the per-vertex fixture).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    /// Amendment 8 (was: `region_nonsimple_cycle_rejects_without_mutation`,
    /// which pinned the amendment-6 limitation this amendment removes): the
    /// base fixture's single-seed region boundary [v, w2, w1, w0] is
    /// exactly NON-SIMPLE at the minted position (edge v→w2 crosses edge
    /// w1→w0) — the region now GROWS across the crossing edge into its
    /// same-class neighbor and commits, all replacement triangles
    /// gate-valid with the exact cover preserved.
    #[test]
    fn region_nonsimple_cycle_grows_to_simplicity() {
        let (mut tris, mut class, coords) = base();
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after grown region relocation"
            );
        }
        assert!(class.iter().all(|&c| c == RegionClass::AOnly));
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover: the domain polygon w0(0,0) → v(0.8,0.4) → w2(4,0) →
        // far(2,2) has area 3.2, and every replacement triangle is
        // positive, so the sum doubles as a no-overlap certificate.
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 3.2).abs() < 1e-12, "cover area {total} != 3.2");
    }

    /// Amendment 8 reject, no mutation: the same non-simple boundary, but
    /// every neighbor beyond the crossing edges is across the intersection
    /// curve (class boundary) — growth is blocked on both sides, the
    /// sub-region rejects, and nothing is mutated (the caller's
    /// amendment-2 revert stays the loud fallback).
    #[test]
    fn region_nonsimple_ungrowable_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        // Both non-star triangles across the curve: the folded AOnly
        // sub-region is the star {0,1}; its crossing edges' external
        // neighbors (tris 2 and 3) are Overlap — ungrowable.
        class[2] = RegionClass::Overlap;
        class[3] = RegionClass::Overlap;
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        assert!(!relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    /// Amendment 7 boundary: a multi-class region whose folded class
    /// sub-region is a SINGLE triangle still rejects without mutation —
    /// the partition never re-triangulates across the class boundary, and
    /// a one-triangle sub-region has no alternative triangulation
    /// (`region too small`). The caller's amendment-2 revert stays the
    /// loud fallback.
    #[test]
    fn region_multiclass_rejects_without_mutation() {
        let (mut tris, mut class, coords) = base();
        class[1] = RegionClass::Overlap; // second star triangle across the curve
        let tris0 = tris.clone();
        let class0 = class.clone();
        let mut em = edge_map_of(&tris);
        let em0 = em.clone();
        let frame = frame_z0();
        assert!(!relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        assert_eq!(tris, tris0, "reject must not mutate triangles");
        assert_eq!(class, class0, "reject must not mutate classes");
        assert_eq!(em, em0, "reject must not mutate the edge map");
    }

    // ── Amendment 7: class-partitioned joint region (M8 increment 10) ────

    /// A multi-class star union (the F0089/F0090 signature: the mint sits
    /// ON the intersection curve, so its star straddles the class
    /// boundary): the FOLDED class sub-region is re-triangulated
    /// independently while the valid sub-region across the curve is left
    /// untouched, and the class-boundary edge survives as sub-region
    /// boundary.
    #[test]
    fn region_multiclass_folded_subregion_relocates_partitioned() {
        // The star-union fixture with the (w0, v, tl) star triangle moved
        // across the intersection curve (Overlap). At v's minted position
        // that triangle is VALID; the fold lives in the AOnly sub-region
        // {(v,a,tl), (v,b,a), (v,w3,b)}.
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl) — Overlap, valid, must stay untouched
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a) — folded at the minted position
            [0, 5, 4], // (v, w3, b)
            [4, 5, 6], // (b, w3, tr) — non-region
            [3, 4, 6], // (a, b, tr) — non-region
            [2, 3, 6], // (tl, a, tr) — non-region
        ];
        let mut class = vec![RegionClass::AOnly; 7];
        class[0] = RegionClass::Overlap;
        let coords = vec![
            p(2.2, -0.3), // v (minted)
            p(0.0, 0.0),  // w0
            p(0.0, 2.0),  // tl
            p(2.0, 1.5),  // a
            p(2.0, 0.5),  // b
            p(4.0, 0.0),  // w3
            p(4.0, 2.0),  // tr
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 3], &coords, &frame),
            "fixture must start folded in the AOnly sub-region"
        );
        assert!(
            gate_tri_valid(&[1, 0, 2], &coords, &frame),
            "the Overlap sub-region must start valid (it is skipped)"
        );
        assert!(relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        // The Overlap sub-region and the non-region slots are untouched.
        assert_eq!(tris[0], [1, 0, 2]);
        assert_eq!(class[0], RegionClass::Overlap);
        assert_eq!(tris[4], [4, 5, 6]);
        assert_eq!(tris[5], [3, 4, 6]);
        assert_eq!(tris[6], [2, 3, 6]);
        // The AOnly sub-region slots (1..=3) are re-triangulated valid.
        for ti in 1..=3 {
            assert!(
                gate_tri_valid(&tris[ti], &coords, &frame),
                "{:?} invalid after partitioned relocation",
                tris[ti]
            );
            assert_eq!(class[ti], RegionClass::AOnly);
        }
        // The class-boundary edge (v, tl) — the intersection curve — is
        // preserved with both sides intact.
        assert_eq!(
            em.get(&[0, 2]).map(|e| e.len()),
            Some(2),
            "class-boundary edge (v,tl) must survive the partition"
        );
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover unchanged (same total domain as the star-union
        // fixture: rect 8.0 + the boundary-V dip 0.6).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (p0, p1, p2) = (q(t[0]), q(t[1]), q(t[2]));
            ((p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 8.6).abs() < 1e-12, "cover area {total} != 8.6");
    }

    /// Amendment 7 termination gate: a VALID-ONLY class sub-region is
    /// skipped even when another sub-region commits — re-triangulating a
    /// fold-free sub-region could churn the mesh without reducing the
    /// gate's folded count.
    #[test]
    fn region_validonly_subregion_is_skipped() {
        let mut tris = vec![
            [1, 0, 2], // (w0, v, tl) — Overlap, valid
            [0, 3, 2], // (v, a, tl)
            [0, 4, 3], // (v, b, a) — folded
            [0, 5, 4], // (v, w3, b)
        ];
        let mut class = vec![
            RegionClass::Overlap,
            RegionClass::AOnly,
            RegionClass::AOnly,
            RegionClass::AOnly,
        ];
        let coords = vec![
            p(2.2, -0.3),
            p(0.0, 0.0),
            p(0.0, 2.0),
            p(2.0, 1.5),
            p(2.0, 0.5),
            p(4.0, 0.0),
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        let overlap_before = (tris[0], class[0]);
        assert!(relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0],
            &coords,
            &frame,
            false
        ));
        assert_eq!(
            (tris[0], class[0]),
            overlap_before,
            "valid-only Overlap sub-region must be skipped, not re-triangulated"
        );
    }

    /// Amendment 10 (M8 increment 13): the joint seeds surfaced by a
    /// NON-SIMPLE cavity polygon are the mints on the CROSSING edges only —
    /// the interacting set per Fig-11 locality — not every mint on the
    /// ring. A 40+-edge ring around a hole lists ~30 mints; seeding them
    /// all inflates the star union into an ANNULUS (measured F0090 ~cut
    /// 22: 2 boundary cycles [32, 20]) that no single boundary walk can
    /// triangulate. Here: the ring [v, w2, w1, w1b, w0] has its (first)
    /// exact crossing at v→w2 × w1b→w0; the minted vertex w1 sits on the
    /// ring but NOT on the crossing — it must not become a joint seed.
    #[test]
    fn nonsimple_ring_mints_narrow_to_crossing_endpoints() {
        // v(=0) minted; star of three triangles, no external neighbors
        // (every link edge is domain boundary ⇒ growth defers), single
        // class, open chain. Fan tri (v,w1b,w0) is invalid at the minted
        // position and the polygon crosses exactly at v→w2 × w1b→w0.
        let mut tris = vec![[0, 2, 3], [0, 3, 4], [0, 4, 1]];
        let mut class = vec![RegionClass::AOnly; 3];
        let coords = vec![
            p(0.8, 0.4), // v (minted)
            p(0.0, 0.0), // w0
            p(4.0, 0.0), // w2
            p(2.0, 0.6), // w1 (minted, NOT on the crossing)
            p(1.2, 0.5), // w1b (minted, crossing endpoint)
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 4, 1], &coords, &frame),
            "fixture must start folded at (v,w1b,w0)"
        );
        let minted = vec![true, false, false, true, true];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple { ring_mints } = out else {
            panic!("fixture must reach the non-simple cavity polygon");
        };
        assert_eq!(
            ring_mints,
            vec![4],
            "joint seeds must be the crossing-edge mints only (w1b), not \
             every ring mint (w1 excluded)"
        );
    }

    /// Amendment 11 (M8 increment 14): a NET-CW BOW-TIE cavity polygon —
    /// non-simple with the inverted lobe dominating the signed area
    /// (measured F0088 vert 674: a hair-thin full-height strip whose long
    /// return edge crosses the up-chain; net 2A = −4.2e-3) — must surface
    /// as `NonSimple` (the joint trigger), not die at the orientation
    /// guard. Simplicity is checked BEFORE orientation: a crossing makes
    /// the signed area lobe-balance noise.
    #[test]
    fn net_cw_bowtie_cavity_triggers_joint_path() {
        // Star of v: link chain [a, b, c]; polygon [v, a, b, c] has edge
        // v→a crossing edge b→c at (0.75, 0.375) and net 2A = −1.5 (CW).
        let mut tris = vec![[0, 1, 2], [0, 2, 3]];
        let mut class = vec![RegionClass::AOnly; 2];
        let coords = vec![
            p(0.0, 0.0), // v (minted)
            p(2.0, 1.0), // a (minted, crossing endpoint)
            p(3.0, 0.0), // b
            p(0.0, 0.5), // c
        ];
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&[0, 1, 2], &coords, &frame),
            "fixture must start folded at (v,a,b)"
        );
        let minted = vec![true, true, false, false];
        let out = relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, &minted, false,
        );
        let RelocOutcome::NonSimple { ring_mints } = out else {
            panic!(
                "net-CW bow-tie must surface NonSimple (joint trigger), \
                 not a terminal orientation reject"
            );
        };
        assert_eq!(
            ring_mints,
            vec![1],
            "the crossing-endpoint mint must be surfaced as a joint seed"
        );
    }

    // ── Amendment 9: connected-component split (M8 increment 12) ─────────

    /// A DISCONNECTED class sub-region (the F0090 33-seed signature: the
    /// joint trigger accumulates seeds from several separate strips): each
    /// edge-connected component is relocated independently — one boundary
    /// walk per component, not one for the union.
    #[test]
    fn region_disconnected_components_relocate_independently() {
        // Two disjoint copies of the base fixture (the second offset by
        // +10 in u), both folded, seeds one vertex from each.
        let (t1, c1, p1) = base();
        let mut tris = t1.clone();
        let mut class = c1.clone();
        let mut coords = p1.clone();
        let off = p1.len() as u32;
        for t in &t1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        class.extend(c1.iter().copied());
        for q in &p1 {
            coords.push(p(frame_z0().project(*q).0 + 10.0, frame_z0().project(*q).1));
        }
        let mut em = edge_map_of(&tris);
        let frame = frame_z0();
        assert!(
            !gate_tri_valid(&tris[1], &coords, &frame)
                && !gate_tri_valid(&tris[4 + 1], &coords, &frame),
            "both copies must start folded"
        );
        assert!(relocate_minted_region(
            &mut tris,
            &mut class,
            &mut em,
            &[0, off],
            &coords,
            &frame,
            false
        ));
        for t in &tris {
            assert!(
                gate_tri_valid(t, &coords, &frame),
                "{t:?} invalid after component-split relocation"
            );
        }
        assert_eq!(
            canon(&em),
            canon(&edge_map_of(&tris)),
            "edge map must be maintained incrementally"
        );
        // Exact cover per copy (boundary-determined 3.2 each, all ears
        // positive ⇒ no overlap).
        let area = |t: &[u32; 3]| {
            let q = |i: u32| frame.project(coords[i as usize]);
            let (q0, q1, q2) = (q(t[0]), q(t[1]), q(t[2]));
            ((q1.0 - q0.0) * (q2.1 - q0.1) - (q1.1 - q0.1) * (q2.0 - q0.0)) / 2.0
        };
        let total: f64 = tris.iter().map(|t| area(t)).sum();
        assert!((total - 6.4).abs() < 1e-12, "cover area {total} != 6.4");
    }
}

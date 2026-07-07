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
        let disc_pair = (disc_circle_edge(a, p.face_a).is_some()
            || disc_circle_edge(b, p.face_b).is_some())
            && annular_disc_face(a, p.face_a).is_none()
            && annular_disc_face(b, p.face_b).is_none();
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
        // tessellated disc face).
        let (mut poly_a, corners_a, rim_a) = face_polygon_2d_tessellated(a, p.face_a, &va, frame)
            .ok_or_else(|| {
            probe("polygon2d-a", &format!("pair=({},{})", p.face_a, p.face_b));
            pair_err(p.face_a, p.face_b)
        })?;
        let (mut poly_b, corners_b, rim_b) = face_polygon_2d_tessellated(b, p.face_b, &vb, frame)
            .ok_or_else(|| {
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
        // loud residue (see the SCOPE GATE below).
        let rim_cross_a = !rim_a.is_empty() && rim_subdivided(&poly_a, &overlay);
        let rim_cross_b = !rim_b.is_empty() && rim_subdivided(&poly_b, &overlay);

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
        let rim_ctxs_a = if rim_a.is_empty() {
            Vec::new()
        } else {
            rim_chord_ctxs(a, p.face_a, &poly_a, &poly_b, frame)
        };
        let rim_ctxs_b = if rim_b.is_empty() {
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
                for &vv in &t {
                    if !minted_mark[vv as usize] {
                        continue;
                    }
                    if relocate_minted_vertex(
                        &mut overlay.tris,
                        &mut overlay.class,
                        &mut edge_map,
                        vv,
                        &coords,
                        frame,
                        probe_flip,
                    ) {
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
    let tess = stage1_tessellate(&verts, brep.edges(), brep.faces()).ok()?;
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
    let tess = stage1_tessellate(&verts, brep.edges(), brep.faces()).ok()?;
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
    probe: bool,
) -> bool {
    let edge_key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    let reject = |why: &str| {
        if probe {
            eprintln!("  [reloc-reject] vert {v} {why}");
        }
        false
    };
    let pos = |i: u32| frame.project(coords[i as usize]);

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

        // Exact simplicity + CCW on the DEDUPLICATED position ring
        // (collapsed sub-floor twins share one resolved position; their
        // zero-length edges cannot cross anything).
        let ring: Vec<(f64, f64)> = {
            let mut r: Vec<(f64, f64)> = Vec::with_capacity(poly.len());
            for &pi in &poly {
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
            return reject("degenerate cavity polygon");
        }
        {
            use crate::coplanar_overlay::rat;
            let mut two_area = RBig::ZERO;
            for k in 0..ring.len() {
                let (ax, ay) = ring[k];
                let (bx, by) = ring[(k + 1) % ring.len()];
                let Ok(t) = rat(ax).and_then(|axr| Ok(axr * rat(by)? - rat(bx)? * rat(ay)?)) else {
                    return reject("non-finite cavity polygon");
                };
                two_area += t;
            }
            if two_area <= RBig::ZERO {
                return reject("cavity polygon not CCW");
            }
        }
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
                    return reject("cavity polygon pinched (repeated position)");
                }
                let (Some(o1), Some(o2), Some(o3), Some(o4)) = (
                    orient_sign_exact(p1, p2, q1),
                    orient_sign_exact(p1, p2, q2),
                    orient_sign_exact(q1, q2, p1),
                    orient_sign_exact(q1, q2, p2),
                ) else {
                    return reject("non-finite cavity polygon");
                };
                // Proper crossing, or an endpoint on the other segment's
                // interior (any collinear touch is conservatively rejected).
                if (o1 * o2 < 0 && o3 * o4 < 0) || o1 == 0 || o2 == 0 || o3 == 0 || o4 == 0 {
                    return reject("cavity polygon not simple");
                }
            }
        }

        // Constrained ear-clip: deterministic first-clippable-ear order.
        let mut work = poly.clone();
        let mut ears: Vec<([u32; 3], RegionClass)> = Vec::with_capacity(link.len());
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
                            return reject("non-finite cavity polygon");
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
                return reject("no clippable ear");
            }
        }
        let last = [work[0], work[1], work[2]];
        if !gate_tri_valid(&last, coords, frame) {
            return reject("final ear invalid");
        }
        ears.push((last, cls0));
        if probe {
            eprintln!("  [reloc-earclip] vert {v} cavity={} tris", cavity.len());
        }
        ears
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
    true
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

/// Like [`face_polygon_2d`], but a flat circular DISC face is tessellated to its
/// Result of [`face_polygon_2d_tessellated`]: the in-frame 2D polygon, a
/// corner→vertex-index key map, and a rim-key→3D-point map (empty for line
/// loops).
type TessellatedFacePolygon = (
    PolygonWithHoles,
    BTreeMap<ExactPoint2, u32>,
    BTreeMap<ExactPoint2, Point3>,
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
        return Some((PolygonWithHoles { outer, holes }, BTreeMap::new(), rim_map));
    }
    let (poly, corners) = face_polygon_2d(brep, fi, coords, frame)?;
    Some((poly, corners, BTreeMap::new()))
}

// ════════════════════════════════════════════════════════════════════════
// PR-M8-disc — direct disc∩convex-polygon containment builder
// ════════════════════════════════════════════════════════════════════════

/// Outcome of the direct disc-pair construction.
enum DiscPair {
    /// Handled: final per-face override triangles (face B already winding-
    /// swapped iff `opposite`).
    Handled {
        tris_a: Vec<[Point3; 3]>,
        tris_b: Vec<[Point3; 3]>,
    },
    /// Coplanar disc pair that is disjoint in-plane — benign, no override.
    Empty,
    /// Outside increment 1's scope — the caller raises the loud residue. The
    /// `&str` is the probe sub-tag.
    Wall(&'static str),
}

/// A disc-loop vertex carrying its in-frame 2D position (exact, for
/// orientation/containment; f64, for angular sorting) and its resolved 3D
/// point (shared between both solids).
struct V2 {
    e: ExactPoint2,
    u: f64,
    v: f64,
    p: Point3,
}

/// Build the override triangles for a near-coplanar pair in which exactly one
/// face is a flat circular disc and the other a convex polygon, when one
/// strictly contains the other (§4.5.5, the dominant M8 sub-class).
///
/// The disc keeps its exact Stage-1 rim ring (so the override is conformal
/// with the cylinder lateral that shares it); the contained region is a
/// shared rim/boundary triangulation emitted IDENTICALLY to both solids, and
/// the surrounding region is an angular-merge annulus on the larger face.
#[allow(clippy::too_many_arguments)]
fn build_disc_pair(
    a: &BRep,
    b: &BRep,
    face_a: usize,
    face_b: usize,
    va: &[Point3],
    vb: &[Point3],
    frame: &Frame,
    opposite: bool,
) -> DiscPair {
    let da = disc_circle_edge(a, face_a);
    let db = disc_circle_edge(b, face_b);
    // disc∩disc (e.g. a bearing recess: a small cylinder cap coplanar with a
    // larger cylinder cap) — both faces keep their exact Stage-1 rim rings, so
    // the containment build stays conformal with BOTH cylinder laterals.
    if da.is_some() && db.is_some() {
        return build_disc_disc_containment(a, b, face_a, face_b, va, vb, frame, opposite);
    }
    let disc_is_a = da.is_some();
    let (disc_brep, disc_fi, disc_coords) = if disc_is_a {
        (a, face_a, va)
    } else {
        (b, face_b, vb)
    };
    let (poly_brep, poly_fi, poly_coords) = if disc_is_a {
        (b, face_b, vb)
    } else {
        (a, face_a, va)
    };

    // Disc rim (exact Stage-1 ring, CCW in frame) + centre as 2D/3D verts.
    let Some(rim3) = disc_rim_ring(disc_brep, disc_fi, disc_coords, frame) else {
        return DiscPair::Wall("disc-rim");
    };
    let circle_e = da.or(db).expect("one disc");
    let Curve::Circle { center, .. } = disc_brep.edges()[circle_e as usize].curve else {
        return DiscPair::Wall("disc-rim");
    };
    let disc: Vec<V2> = match rim3.iter().map(|&p| mk_v2(p, frame)).collect() {
        Some(v) => v,
        None => return DiscPair::Wall("disc-rim"),
    };
    let center_v = match mk_v2(center, frame) {
        Some(v) => v,
        None => return DiscPair::Wall("disc-rim"),
    };

    // Convex polygon corners (must be hole-free; CCW in frame).
    if !poly_brep.faces()[poly_fi].inner_loops.is_empty() {
        return DiscPair::Wall("disc-poly-holed");
    }
    let Some(poly_ring) =
        loop_vertex_ring(poly_brep.edges(), &poly_brep.faces()[poly_fi].outer_loop)
    else {
        return DiscPair::Wall("disc-poly-loop");
    };
    let poly: Vec<V2> = match poly_ring
        .iter()
        .map(|&vi| mk_v2(poly_coords[vi as usize], frame))
        .collect()
    {
        Some(v) => v,
        None => return DiscPair::Wall("disc-poly-loop"),
    };
    let Some(poly) = orient_ccw(poly) else {
        return DiscPair::Wall("disc-poly-degenerate");
    };
    if !is_strictly_convex(&poly) {
        return DiscPair::Wall("disc-poly-nonconvex");
    }
    // `disc` is convex by construction but re-orient defensively (the rim is
    // already CCW in frame).
    let Some(disc) = orient_ccw(disc) else {
        return DiscPair::Wall("disc-degenerate");
    };

    // Containment: which shape is strictly inside the other? (Strict — a
    // tangency or crossing falls through to the loud residue.)
    let disc_in_poly = disc.iter().all(|v| strictly_inside_convex(&poly, &v.e));
    let poly_in_disc = poly.iter().all(|v| strictly_inside_convex(&disc, &v.e));

    let (inner, outer, center_opt): (&[V2], &[V2], Option<&V2>) = if disc_in_poly {
        (&disc, &poly, Some(&center_v))
    } else if poly_in_disc {
        (&poly, &disc, None)
    } else if convex_rings_overlap(&disc, &poly) {
        // Partial overlap: a circle×segment crossing (irrational on the
        // sampled ring) plus boundary-split propagation — a deferred slice.
        return DiscPair::Wall("disc-crossing");
    } else {
        // Coplanar but disjoint in-plane (the scan's AABBs overlap, the
        // shapes do not): benign — the exact arrangement passes the coplanar
        // non-overlap through (deviation N17). Nothing to override.
        return DiscPair::Empty;
    };

    // OVERLAP = the inner region; emitted to BOTH faces. A disc inner uses a
    // rim fan about its centre; a polygon inner uses an ear-clip.
    let Some(overlap) = (match center_opt {
        Some(c) => fan_tris(c, inner),
        None => earclip_tris(inner),
    }) else {
        return DiscPair::Wall("disc-overlap-tri");
    };
    // OUTER-only = `outer` with `inner` as a hole; emitted to the larger face.
    let Some(annulus) = annulus_tris(outer, inner) else {
        return DiscPair::Wall("disc-annulus-tri");
    };

    // The larger face owns the annulus; both faces own the overlap. Triangles
    // are frame-CCW (normal = +n̂ = face A's outward normal): face A keeps
    // them, face B swaps iff opposite.
    let outer_is_disc = poly_in_disc; // when poly⊆disc, the disc is larger
    let mut disc_face_tris = overlap.clone();
    let mut poly_face_tris = overlap;
    if outer_is_disc {
        disc_face_tris.extend(annulus);
    } else {
        poly_face_tris.extend(annulus);
    }
    let (tris_a, mut tris_b) = if disc_is_a {
        (disc_face_tris, poly_face_tris)
    } else {
        (poly_face_tris, disc_face_tris)
    };
    if opposite {
        for t in &mut tris_b {
            t.swap(1, 2);
        }
    }
    DiscPair::Handled { tris_a, tris_b }
}

/// A disc face's exact Stage-1 rim ring (frame coords) plus its centre, both as
/// `V2`. The rim is bit-identical to what the cylinder lateral sharing it gets,
/// so any override built from it stays conformal.
fn disc_ring_and_center(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
) -> Option<(Vec<V2>, V2)> {
    let circle_e = disc_circle_edge(brep, fi)?;
    let Curve::Circle { center, .. } = brep.edges()[circle_e as usize].curve else {
        return None;
    };
    let rim3 = disc_rim_ring(brep, fi, coords, frame)?;
    let ring: Vec<V2> = rim3
        .iter()
        .map(|&p| mk_v2(p, frame))
        .collect::<Option<_>>()?;
    let center_v = mk_v2(center, frame)?;
    Some((ring, center_v))
}

/// Build override triangles for a near-coplanar pair where BOTH faces are flat
/// circular discs and one rim strictly contains the other (the §4.5.5
/// disc∩disc containment sub-class — a bearing recess / coaxial cap-on-cap).
///
/// Mirrors [`build_disc_pair`]'s containment build: the OVERLAP is the inner
/// disc fanned about its own centre (emitted identically to both solids), and
/// the larger disc additionally owns the angular-merge ANNULUS between the two
/// rims. Both rims are kept exactly (each shared with its cylinder lateral).
/// Crossing rims defer to Increment 2 (`Wall("disc-disc-crossing")`); a benign
/// disjoint coplanar pair returns `Empty`.
#[allow(clippy::too_many_arguments)]
fn build_disc_disc_containment(
    a: &BRep,
    b: &BRep,
    face_a: usize,
    face_b: usize,
    va: &[Point3],
    vb: &[Point3],
    frame: &Frame,
    opposite: bool,
) -> DiscPair {
    let (Some((ring_a, center_a)), Some((ring_b, center_b))) = (
        disc_ring_and_center(a, face_a, va, frame),
        disc_ring_and_center(b, face_b, vb, frame),
    ) else {
        return DiscPair::Wall("disc-rim");
    };
    let (Some(ring_a), Some(ring_b)) = (orient_ccw(ring_a), orient_ccw(ring_b)) else {
        return DiscPair::Wall("disc-degenerate");
    };

    // Strict containment (a tangency or crossing falls through, as in the
    // disc∩polygon path).
    let a_in_b = ring_a.iter().all(|v| strictly_inside_convex(&ring_b, &v.e));
    let b_in_a = ring_b.iter().all(|v| strictly_inside_convex(&ring_a, &v.e));
    // (inner, outer, inner-centre, inner_is_a)
    let (inner, outer, inner_center, inner_is_a) = if a_in_b {
        (&ring_a, &ring_b, &center_a, true)
    } else if b_in_a {
        (&ring_b, &ring_a, &center_b, false)
    } else if convex_rings_overlap(&ring_a, &ring_b) {
        // CROSSING rims (a lens overlap, neither contained). No Stage-0
        // override: the two caps keep their default conformal Stage-1 fans and
        // cherchi's coplanar arrangement (single-coplanar-edge N13 + the
        // fully-coplanar PRs 1-4 pocket dedup) resolves the coplanar lens
        // directly — the explicit two-disc lens construction the overlay would
        // need is unnecessary now that cherchi handles coplanar overlap. (A
        // genuine disjoint pair returns `Empty` below; a crossing produces a
        // real coplanar overlap cherchi must arrange, but the keep/drop is the
        // same `Empty` no-override path.)
        return DiscPair::Empty;
    } else {
        return DiscPair::Empty;
    };

    let Some(overlap) = fan_tris(inner_center, inner) else {
        return DiscPair::Wall("disc-overlap-tri");
    };
    let Some(annulus) = annulus_tris(outer, inner) else {
        return DiscPair::Wall("disc-annulus-tri");
    };

    // Triangles are frame-CCW (= face A's outward normal): the inner face owns
    // the overlap, the outer face owns overlap + annulus. Face A keeps frame-CCW
    // winding; face B swaps iff its outward normal opposes the canonical one.
    let (tris_a, mut tris_b) = if inner_is_a {
        let mut outer_t = overlap.clone();
        outer_t.extend(annulus);
        (overlap, outer_t)
    } else {
        let mut outer_t = overlap.clone();
        outer_t.extend(annulus);
        (outer_t, overlap)
    };
    if opposite {
        for t in &mut tris_b {
            t.swap(1, 2);
        }
    }
    DiscPair::Handled { tris_a, tris_b }
}

/// Lift a 3D point to a `V2` (in-frame 2D + the original 3D point).
fn mk_v2(p: Point3, frame: &Frame) -> Option<V2> {
    let (u, v) = frame.project(p);
    Some(V2 {
        e: ExactPoint2::from_f64(u, v)?,
        u,
        v,
        p,
    })
}

/// Re-orient a ring CCW in the frame (exact shoelace); `None` if degenerate.
fn orient_ccw(ring: Vec<V2>) -> Option<Vec<V2>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut area2 = RBig::ZERO;
    for i in 1..n - 1 {
        area2 += cross_r(&ring[0].e, &ring[i].e, &ring[i + 1].e);
    }
    if area2 == RBig::ZERO {
        return None;
    }
    if area2 > RBig::ZERO {
        Some(ring)
    } else {
        Some(ring.into_iter().rev().collect())
    }
}

/// Strictly convex CCW polygon: every corner turns strictly left.
fn is_strictly_convex(ring: &[V2]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    (0..n).all(|i| cross_r(&ring[(i + n - 1) % n].e, &ring[i].e, &ring[(i + 1) % n].e) > RBig::ZERO)
}

/// Is `q` strictly inside the convex CCW polygon `ring`?
fn strictly_inside_convex(ring: &[V2], q: &ExactPoint2) -> bool {
    let n = ring.len();
    (0..n).all(|i| cross_r(&ring[i].e, &ring[(i + 1) % n].e, q) > RBig::ZERO)
}

/// Do two convex CCW rings overlap with positive area? A vertex of one
/// strictly inside the other, or a proper edge crossing (the rotated-rectangle
/// case with no vertex inside). Exact. Used only to tell a benign disjoint
/// coplanar pair from a partial-overlap (crossing) one.
fn convex_rings_overlap(a: &[V2], b: &[V2]) -> bool {
    if a.iter().any(|v| strictly_inside_convex(b, &v.e))
        || b.iter().any(|v| strictly_inside_convex(a, &v.e))
    {
        return true;
    }
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        let (a0, a1) = (&a[i].e, &a[(i + 1) % na].e);
        for j in 0..nb {
            let (b0, b1) = (&b[j].e, &b[(j + 1) % nb].e);
            if segs_properly_cross(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    false
}

/// Do open segments `p0p1` and `q0q1` cross at a single interior point? (Both
/// endpoints of each strictly straddle the other's supporting line.)
fn segs_properly_cross(
    p0: &ExactPoint2,
    p1: &ExactPoint2,
    q0: &ExactPoint2,
    q1: &ExactPoint2,
) -> bool {
    let d1 = cross_r(p0, p1, q0);
    let d2 = cross_r(p0, p1, q1);
    let d3 = cross_r(q0, q1, p0);
    let d4 = cross_r(q0, q1, p1);
    ((d1 > RBig::ZERO) != (d2 > RBig::ZERO))
        && (d1 != RBig::ZERO && d2 != RBig::ZERO)
        && ((d3 > RBig::ZERO) != (d4 > RBig::ZERO))
        && (d3 != RBig::ZERO && d4 != RBig::ZERO)
}

/// Fan a convex CCW ring about an interior apex (the disc centre).
fn fan_tris(apex: &V2, ring: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push([apex.p, ring[i].p, ring[(i + 1) % n].p]);
    }
    Some(out)
}

/// Ear-clip a simple CCW ring into frame-CCW triangles.
fn earclip_tris(ring: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let pts: Vec<ExactPoint2> = ring.iter().map(|v| v.e.clone()).collect();
    let idx = crate::coplanar_overlay::ear_clip(&pts).ok()?;
    Some(
        idx.into_iter()
            .map(|[i, j, k]| [ring[i].p, ring[j].p, ring[k].p])
            .collect(),
    )
}

/// Triangulate `outer` (convex CCW) minus `inner` (convex CCW, strictly
/// inside) — the annular region between two nested convex rings.
///
/// Both rings are star-shaped about the inner ring's centroid `O` (interior to
/// `inner` by convexity, hence to `outer` since `inner ⊆ outer`), so their
/// vertices are angularly monotone about `O`. The annulus is the strip between
/// the two monotone chains; it triangulates by an angular merge (advance the
/// chain whose next vertex comes first in angle), each triangle oriented
/// frame-CCW exactly. No keyhole, no Steiner points — every boundary vertex of
/// both rings is preserved, so the inner ring stays bit-shared with the
/// overlap fan and the cylinder lateral.
fn annulus_tris(outer: &[V2], inner: &[V2]) -> Option<Vec<[Point3; 3]>> {
    let (ni, no) = (inner.len(), outer.len());
    if ni < 3 || no < 3 {
        return None;
    }
    let ox: f64 = inner.iter().map(|v| v.u).sum::<f64>() / ni as f64;
    let oy: f64 = inner.iter().map(|v| v.v).sum::<f64>() / ni as f64;
    let ang = |v: &V2| (v.v - oy).atan2(v.u - ox);

    // A ring → an ascending-unwrapped angle chain starting at its min-angle
    // vertex, with the start vertex appended again (closing the loop at
    // angle a0 + 2π).
    let chain = |ring: &[V2]| -> (Vec<usize>, Vec<f64>) {
        let n = ring.len();
        let start = (0..n)
            .min_by(|&a, &b| ang(&ring[a]).partial_cmp(&ang(&ring[b])).unwrap())
            .unwrap();
        let mut order = Vec::with_capacity(n + 1);
        let mut angs = Vec::with_capacity(n + 1);
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=n {
            let idx = (start + k) % n;
            let mut a = ang(&ring[idx]);
            while a <= prev {
                a += std::f64::consts::TAU;
            }
            prev = a;
            order.push(idx);
            angs.push(a);
        }
        (order, angs)
    };
    let (io, ia) = chain(inner);
    let (oo, oa) = chain(outer);

    // Exact centroid for the half-plane visibility guards (spec
    // `m8_stage0_fold_pair_emission` E-F1..E-F3): strictly interior to the
    // convex inner ring, so it decides which side of a chord's supporting
    // line is "inner". Exact sign tests only — no tolerances (A14.3).
    let o_exact = ExactPoint2::from_f64(ox, oy)?;

    // Merge the two monotone chains into a strip triangulation.
    let tri = |a: &V2, b: &V2, c: &V2| -> [Point3; 3] {
        if cross_r(&a.e, &b.e, &c.e) >= RBig::ZERO {
            [a.p, b.p, c.p]
        } else {
            [a.p, c.p, b.p]
        }
    };
    // E-F1/E-F2: an inner-advance triangle (chord c1→c2 fanned to outer P)
    // is valid iff P lies STRICTLY on the opposite side of the chord's
    // supporting line from O. Angular monotonicity alone does not imply
    // this (measured, F0027: a far square corner falls on the CENTER side
    // of a distant chord's line → the fan double-covers the disc pocket —
    // the fold-pair census class). Returns the triangle's exact area (×2)
    // for the E-F4 certificate.
    let inner_valid = |i: usize, j: usize| -> Option<RBig> {
        let (c1, c2) = (&inner[io[i]], &inner[io[i + 1]]);
        let s_p = cross_r(&c1.e, &c2.e, &outer[oo[j]].e);
        let s_o = cross_r(&c1.e, &c2.e, &o_exact);
        if s_p == RBig::ZERO || s_o == RBig::ZERO {
            return None;
        }
        if (s_p > RBig::ZERO) == (s_o > RBig::ZERO) {
            return None;
        }
        Some(if s_p > RBig::ZERO { s_p } else { -s_p })
    };
    // E-F3: an outer-advance triangle (outer edge o1→o2 with inner apex Q)
    // is valid iff Q lies STRICTLY on O's side of the outer edge's line
    // (guaranteed by convex nesting; a violation is a loud E-F5).
    let outer_valid = |i: usize, j: usize| -> Option<RBig> {
        let (o1, o2) = (&outer[oo[j]], &outer[oo[j + 1]]);
        let s_q = cross_r(&o1.e, &o2.e, &inner[io[i]].e);
        let s_o = cross_r(&o1.e, &o2.e, &o_exact);
        if s_q == RBig::ZERO || s_o == RBig::ZERO {
            return None;
        }
        if (s_q > RBig::ZERO) != (s_o > RBig::ZERO) {
            return None;
        }
        Some(if s_q > RBig::ZERO { s_q } else { -s_q })
    };
    let mut out: Vec<[Point3; 3]> = Vec::with_capacity(ni + no);
    let mut covered2 = RBig::ZERO;
    let (mut i, mut j) = (0usize, 0usize);
    let mut guard = 0usize;
    while i < ni || j < no {
        guard += 1;
        if guard > ni + no + 8 {
            return None;
        }
        // Angle preference as before; validity redirects an invalid
        // preferred advance to the other chain (E-F2), and a step where
        // NEITHER advance is valid is a loud `None` (E-F5) — never a
        // silently-flipped or invisible fan.
        let prefer_inner = if i >= ni {
            false
        } else if j >= no {
            true
        } else {
            ia[i + 1] <= oa[j + 1]
        };
        let inner_ok = if i < ni && j < no {
            inner_valid(i, j)
        } else if i < ni && j >= no {
            // Outer chain exhausted: the closing outer vertex is oo[no]
            // (== oo[0]); the guard still applies against it.
            inner_valid(i, no)
        } else {
            None
        };
        let outer_ok = if j < no && i < ni {
            outer_valid(i, j)
        } else if j < no && i >= ni {
            outer_valid(ni, j)
        } else {
            None
        };
        let advance_inner = match (prefer_inner, &inner_ok, &outer_ok) {
            (true, Some(_), _) => true,
            (true, None, Some(_)) => false,
            (false, _, Some(_)) => false,
            (false, Some(_), None) => true,
            (_, None, None) => return None,
        };
        if advance_inner {
            let jj = if j < no { j } else { no };
            let t = tri(&inner[io[i]], &inner[io[i + 1]], &outer[oo[jj]]);
            covered2 += inner_ok.expect("validated");
            if !degenerate(&t) {
                out.push(t);
            }
            i += 1;
        } else {
            let ii = if i < ni { i } else { ni };
            let t = tri(&outer[oo[j]], &outer[oo[j + 1]], &inner[io[ii]]);
            covered2 += outer_ok.expect("validated");
            if !degenerate(&t) {
                out.push(t);
            }
            j += 1;
        }
    }

    // E-F4 coverage certificate (I2, the `triangulate_ring` P9-gate
    // pattern): the emitted strip covers EXACTLY the region between the
    // rings — no pleat, no gap. Exact shoelace over the same coordinates
    // the triangles use.
    let shoelace2 = |ring: &[V2]| -> RBig {
        let n = ring.len();
        let mut a = RBig::ZERO;
        for k in 0..n {
            let p = &ring[k].e;
            let q = &ring[(k + 1) % n].e;
            a += &p.x * &q.y - &q.x * &p.y;
        }
        if a > RBig::ZERO {
            a
        } else {
            -a
        }
    };
    if covered2 != shoelace2(outer) - shoelace2(inner) {
        return None;
    }
    Some(out)
}

/// A triangle with two coincident vertices (zero geometric extent).
fn degenerate(t: &[Point3; 3]) -> bool {
    t[0] == t[1] || t[1] == t[2] || t[2] == t[0]
}

// ════════════════════════════════════════════════════════════════════════
// boundary-split propagation (§4.5.5 shared sampling points)
// ════════════════════════════════════════════════════════════════════════

/// Splits keyed by the UNDIRECTED endpoint vertex-index pair (B-Rep edges
/// are commonly duplicated per face — e.g. the box fixtures carry 24
/// directed edges over 12 undirected segments — so geometric identity is
/// the vertex pair, not the edge index). Each split: exact parameter along
/// the canonical `min(vi) → max(vi)` direction + the shared 3D coordinate.
type SplitMap = BTreeMap<(u32, u32), Vec<(RBig, Point3)>>;

/// PR-M8 disc-rim crossing: extra 3D crossing points to insert into a
/// full-circle rim edge's Stage-1 ring, keyed by the rim's `Curve::Circle`
/// edge index (one map per solid). Threaded into
/// [`stage1_tessellate_with_rim_overrides`] so the cap, the cylinder lateral,
/// and the opposite cap all share the SAME subdivided rim (no T-junction).
type RimSplitMap = BTreeMap<u32, Vec<Point3>>;

/// M-C diagnosis dump (read-only observer; fires only with
/// `YANG_STAGE0_DUMP_DIR` set — never in production/WASM). One file per
/// processed overlay pair: per-vertex resolution provenance (which map the
/// 2D→3D resolution hit) + resolved 3D coordinate, per-triangle region
/// class and per-side emission verdict including the E8 resolved-degenerate
/// drop, and the split maps as collected after this pair. Joins the operand
/// census's offender vertices back to overlay entities.
#[allow(clippy::too_many_arguments)]
fn dump_pair_overlay(
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
#[allow(clippy::too_many_arguments)]
fn collect_edge_splits(
    brep: &BRep,
    fi: usize,
    coords: &[Point3],
    frame: &Frame,
    cluster_map: &BTreeMap<(u64, u64), (u64, u64)>,
    overlay: &ClassifiedOverlay,
    side_classes: [RegionClass; 2],
    resolved: &[Point3],
    splits: &mut SplitMap,
) {
    // Overlay vertices used by THIS side's triangles (the conforming
    // triangulation guarantees any vertex on the face boundary that the
    // side's triangulation needs is used by a side triangle).
    let mut used = vec![false; overlay.verts.len()];
    for (t, c) in overlay.tris.iter().zip(&overlay.class) {
        if side_classes.contains(c) {
            for &v in t {
                used[v as usize] = true;
            }
        }
    }

    let f = &brep.faces()[fi];
    for &e_idx in std::iter::once(&f.outer_loop)
        .chain(f.inner_loops.iter())
        .flatten()
    {
        let edge = &brep.edges()[e_idx as usize];
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
                // M-C diagnosis probe (read-only, env-gated): report exact
                // NON-collinear vertices whose perpendicular miss distance is
                // tiny — the band-scale near-miss population the split
                // collector silently skips.
                if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                    let len = len2.to_f64().value().sqrt();
                    let miss = (cross.to_f64().value() / len).abs();
                    if miss < 1.0e-3 * len {
                        let t = ((&dx * &wx + &dy * &wy) / &len2).to_f64().value();
                        eprintln!(
                            "[split-probe] f={fi} edge ({lo},{hi}) vert {i} NEAR-MISS \
                             dist={miss:e} t={t}"
                        );
                    }
                }
                continue;
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
            if !entry.iter().any(|(t0, _)| *t0 == t) {
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

enum BuildErr {
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
fn fan_split_tri(tri: [u32; 3], i: usize, interior: &[u32]) -> Vec<[u32; 3]> {
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
fn edge_split_curved_face(
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
                return None; // a curved (arc) subdivided edge — deferred
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
fn build_stage0_mesh(
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
    )?;

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

        // Does this face's boundary carry propagated split points?
        let face_split = std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .flatten()
            .any(|&e| {
                let edge = &brep.edges()[e as usize];
                splits.contains_key(&(edge.start.min(edge.end), edge.start.max(edge.end)))
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
        let n = f.outer_loop.len();
        let mut ring: Vec<u32> = Vec::new();
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
fn intern_vert(verts: &mut Vec<Point3>, intern: &mut BTreeMap<[u64; 3], u32>, p: Point3) -> u32 {
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
fn triangulate_ring(
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
    let pts: Vec<ExactPoint2> = ring
        .iter()
        .map(|&vi| {
            let p = verts[vi as usize].as_array();
            let u = p[0] * e1[0] + p[1] * e1[1] + p[2] * e1[2];
            let v = p[0] * e2[0] + p[1] * e2[1] + p[2] * e2[2];
            ExactPoint2::from_f64(u, v)
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

// ════════════════════════════════════════════════════════════════════════
// PR-5 — coincident-cylinder pair detector (the membrane analog of PairPlane)
// ════════════════════════════════════════════════════════════════════════

/// Detect coincident-cylinder A×B face pairs: one `Surface::Cylinder` face from
/// A and one from B that share the SAME cylindrical surface (collinear axes,
/// equal radius) with overlapping axial extent. Each becomes a [`PairCylinder`]
/// supplying the post-`keep_set` membrane keep/drop decision in `boolean()`.
///
/// This is a PARALLEL detector — it does NOT touch the planar overlay / mesh
/// re-tessellation path. cherchi already constructs the coincident-cylinder
/// overlap (the shared lateral sheet is bit-identical in both solids' Stage-1
/// meshes because the gear's bore wall and the flange's outer wall are the
/// identical analytic cylinder); we only need to tell `boolean()` whether that
/// internal sheet survives the op.
pub(crate) fn detect_coincident_cylinder_pairs(a: &BRep, b: &BRep) -> Vec<PairCylinder> {
    let cyls_a = cylinder_faces(a);
    let cyls_b = cylinder_faces(b);
    let mut out = Vec::new();
    for ca in &cyls_a {
        for cb in &cyls_b {
            // Scale-relative band over both cylinders' geometry (axis points,
            // radii, and the axial-extent endpoints). Mirrors the planar
            // `near_coplanar_band`: `TAU_MODEL.max(scale·TAU_WORK)`.
            let mut scale = 0.0_f64;
            for v in ca
                .axis_point
                .iter()
                .chain(cb.axis_point.iter())
                .chain(std::iter::once(&ca.radius))
                .chain(std::iter::once(&cb.radius))
                .chain(ca.extent.iter())
                .chain(cb.extent.iter())
            {
                scale = scale.max(v.abs());
            }
            let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);

            if !cylinders_coincident(ca, cb, band) {
                continue;
            }
            // Axial extents must overlap (band-inflated) — two coaxial,
            // equal-radius cylinders that do not overlap along the axis share
            // no surface region.
            let (lo_a, hi_a) = (ca.extent[0], ca.extent[1]);
            let (lo_b, hi_b) = (cb.extent[0], cb.extent[1]);
            if lo_a > hi_b + band || lo_b > hi_a + band {
                continue;
            }
            // Opposite iff exactly one face is a cavity wall (`reversed`): both
            // share the analytic outward direction (radially away from axis), so
            // their EFFECTIVE outward normals oppose iff their `reversed` flags
            // differ — the same opposite/equal split the planar pair makes.
            let opposite = ca.reversed != cb.reversed;
            out.push(PairCylinder {
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                radius: ca.radius,
                band,
                opposite,
            });
        }
    }
    out
}

/// A cylinder face's analytic parameters plus the axial extent of its loop
/// vertices (projected onto the axis) and its `reversed` flag.
struct CylFace {
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    /// `[lo, hi]` axial parameter `(p − axis_point)·axis_dir` over the face's
    /// loop vertices.
    extent: [f64; 2],
    reversed: bool,
}

/// All `Surface::Cylinder` faces of `brep` with normalized axes and the axial
/// extent of their loop vertices. Faces whose axis is degenerate are skipped.
fn cylinder_faces(brep: &BRep) -> Vec<CylFace> {
    let mut out = Vec::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ad = axis_dir.as_array();
        let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
        if len < cad_primitives::MIN_FEATURE_SIZE {
            continue;
        }
        let au = [ad[0] / len, ad[1] / len, ad[2] / len];
        // Axial extent over the face's loop vertices.
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for vi in face_loop_verts(brep, fi) {
            let Some(v) = brep.vertices().get(vi as usize) else {
                continue;
            };
            let p = v.point.as_array();
            let t = (p[0] - ap[0]) * au[0] + (p[1] - ap[1]) * au[1] + (p[2] - ap[2]) * au[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
        if !lo.is_finite() {
            // No loop vertices (e.g. a seam-only loop): treat the extent as a
            // point at the axis origin so coaxial/equal-radius matching still
            // fires but the axial-overlap test stays meaningful.
            lo = 0.0;
            hi = 0.0;
        }
        out.push(CylFace {
            axis_point: ap,
            axis_dir: au,
            radius,
            extent: [lo, hi],
            reversed: f.reversed,
        });
    }
    out
}

/// Are two cylinder faces COINCIDENT: collinear axes (parallel directions AND
/// one axis point lies on the other's axis line) and equal radius, all within
/// the scale-relative `band`?
fn cylinders_coincident(ca: &CylFace, cb: &CylFace, band: f64) -> bool {
    // Equal radius.
    if (ca.radius - cb.radius).abs() > band {
        return false;
    }
    // Parallel axis directions (|cross| ≈ 0).
    let cross = [
        ca.axis_dir[1] * cb.axis_dir[2] - ca.axis_dir[2] * cb.axis_dir[1],
        ca.axis_dir[2] * cb.axis_dir[0] - ca.axis_dir[0] * cb.axis_dir[2],
        ca.axis_dir[0] * cb.axis_dir[1] - ca.axis_dir[1] * cb.axis_dir[0],
    ];
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    // Scale the angular tolerance by the radius so the band is a true distance
    // bound on the surface (a tiny angular error over a large radius is still a
    // surface displacement of band·… — keep it conservative: compare directly).
    if sin > band.max(cad_primitives::TAU_MODEL) {
        return false;
    }
    // b's axis point lies on a's axis line: the perpendicular distance from
    // cb.axis_point to a's line (point ca.axis_point, dir ca.axis_dir).
    let w = [
        cb.axis_point[0] - ca.axis_point[0],
        cb.axis_point[1] - ca.axis_point[1],
        cb.axis_point[2] - ca.axis_point[2],
    ];
    let t = w[0] * ca.axis_dir[0] + w[1] * ca.axis_dir[1] + w[2] * ca.axis_dir[2];
    let perp = [
        w[0] - t * ca.axis_dir[0],
        w[1] - t * ca.axis_dir[1],
        w[2] - t * ca.axis_dir[2],
    ];
    let perp_dist = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    perp_dist <= band
}

// ════════════════════════════════════════════════════════════════════════
// M8-cyl Increment 1 — coincident-cylinder Stage-0 conformal re-tessellation
// (Yang 2025 §4.5.5, the CURVED analog of the planar coplanar overlay).
// ════════════════════════════════════════════════════════════════════════
//
// §4.5.5 requires coincident surfaces between two solids to carry IDENTICAL
// meshes on their overlap region BEFORE the mesh boolean. For two coincident
// cylinders that overlap on a z-band (full θ — the gear's bore-wall ∩
// flange-wall case), the (θ, z) 2D Boolean reduces to a 1D z-interval: the
// overlap is `[max(za0, zb0), min(za1, zb1)]`. We make the overlap band
// bit-identical by inserting, into the LARGER cylinder's lateral, conformal
// rings that are LITERAL COPIES of the smaller (contained) cylinder's rim-ring
// vertices at the overlap boundary z-levels (`task28` proved both impls produce
// a non-watertight raw boolean here, so this upstream step is the un-portable-
// from-Cherchi capability). The two laterals then share bit-identical triangles
// on the overlap, so cherchi's pocket-dedup (PR-4) collapses them to ONE
// multi-label sheet and the §4.5.5 membrane resolution in `boolean()` drops it
// for the union — leaving a watertight result.
//
// Bit-identity is BY CONSTRUCTION (the inserted ring vertices are the SAME f64
// `Point3`s the contained solid's Stage-1 tessellation produced), NOT by
// tolerance fusing (P9 — the F0057 rounding-weld and broad SSI fallback were
// both reverted; this never welds within a tolerance).

/// A coincident-cylinder A×B pair with the lateral FACE indices and the
/// solids' axial extents — the richer form of [`PairCylinder`] used by the
/// conformal re-tessellation (which needs to know WHICH face to rebuild).
/// A coincident-cylinder GROUP: ALL faces of A and of B that lie on ONE shared
/// cylinder (the gear's bore wall is split into 4 arc-patch faces, the flange
/// wall into 4 more — collectively two coincident full-θ cylinders). The
/// conformal re-tessellation treats the group as a unit: aggregate each solid's
/// rings over ALL its faces in the group, then rebuild the outer solid's group
/// faces as one re-banded full-θ strip.
struct CoincidentCylinderGroup {
    faces_a: Vec<usize>,
    faces_b: Vec<usize>,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    band: f64,
    /// `[lo, hi]` aggregate axial extent of A's faces, B's faces.
    extent_a: [f64; 2],
    extent_b: [f64; 2],
    /// `true` iff A's and B's faces have OPPOSING effective outward normals
    /// (bore cavity wall vs solid wall) — derived from the `reversed` flags,
    /// which must agree within each solid's faces of the group.
    opposite: bool,
}

/// Detect coincident-cylinder GROUPS between A and B: cluster each solid's
/// cylinder faces by shared analytic cylinder (collinear axis + equal radius),
/// then pair an A-cluster with a B-cluster on the SAME cylinder with
/// overlapping axial extent. Increment 1 returns groups where every face in a
/// solid's cluster shares the SAME `reversed` flag (a single coherent wall);
/// mixed flags → that cluster is skipped (a later increment).
fn detect_coincident_cylinder_groups(a: &BRep, b: &BRep) -> Vec<CoincidentCylinderGroup> {
    let clusters_a = cluster_cylinder_faces(a);
    let clusters_b = cluster_cylinder_faces(b);
    let mut out = Vec::new();
    for ca in &clusters_a {
        for cb in &clusters_b {
            let mut scale = 0.0_f64;
            for v in ca
                .axis_point
                .iter()
                .chain(cb.axis_point.iter())
                .chain(std::iter::once(&ca.radius))
                .chain(std::iter::once(&cb.radius))
                .chain(ca.extent.iter())
                .chain(cb.extent.iter())
            {
                scale = scale.max(v.abs());
            }
            let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
            let rep_a = CylFace {
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                radius: ca.radius,
                extent: ca.extent,
                reversed: ca.reversed,
            };
            let rep_b = CylFace {
                axis_point: cb.axis_point,
                axis_dir: cb.axis_dir,
                radius: cb.radius,
                extent: cb.extent,
                reversed: cb.reversed,
            };
            if !cylinders_coincident(&rep_a, &rep_b, band) {
                continue;
            }
            let (lo_a, hi_a) = (ca.extent[0], ca.extent[1]);
            let (lo_b, hi_b) = (cb.extent[0], cb.extent[1]);
            if lo_a > hi_b + band || lo_b > hi_a + band {
                continue;
            }
            out.push(CoincidentCylinderGroup {
                faces_a: ca.faces.clone(),
                faces_b: cb.faces.clone(),
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                band,
                extent_a: ca.extent,
                extent_b: cb.extent,
                opposite: ca.reversed != cb.reversed,
            });
        }
    }
    out
}

/// One solid's cluster of cylinder faces sharing an analytic cylinder.
struct CylCluster {
    faces: Vec<usize>,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    extent: [f64; 2],
    /// Shared `reversed` flag across the cluster (clusters with mixed flags are
    /// split so each cluster is a single coherent wall).
    reversed: bool,
}

/// Cluster a solid's `Surface::Cylinder` faces by shared analytic cylinder
/// (collinear axis + equal radius + same `reversed`), aggregating each
/// cluster's axial extent over all its faces.
fn cluster_cylinder_faces(brep: &BRep) -> Vec<CylCluster> {
    let faces = cylinder_faces_indexed(brep);
    let mut clusters: Vec<CylCluster> = Vec::new();
    for (fi, cf) in &faces {
        let mut scale = 0.0_f64;
        for v in cf
            .axis_point
            .iter()
            .chain(std::iter::once(&cf.radius))
            .chain(cf.extent.iter())
        {
            scale = scale.max(v.abs());
        }
        let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
        let mut matched = false;
        for cl in clusters.iter_mut() {
            let rep = CylFace {
                axis_point: cl.axis_point,
                axis_dir: cl.axis_dir,
                radius: cl.radius,
                extent: cl.extent,
                reversed: cl.reversed,
            };
            if cl.reversed == cf.reversed && cylinders_coincident(&rep, cf, band) {
                cl.faces.push(*fi);
                cl.extent[0] = cl.extent[0].min(cf.extent[0]);
                cl.extent[1] = cl.extent[1].max(cf.extent[1]);
                matched = true;
                break;
            }
        }
        if !matched {
            clusters.push(CylCluster {
                faces: vec![*fi],
                axis_point: cf.axis_point,
                axis_dir: cf.axis_dir,
                radius: cf.radius,
                extent: cf.extent,
                reversed: cf.reversed,
            });
        }
    }
    clusters
}

/// All `Surface::Cylinder` faces of `brep` with their FACE INDEX and parameters.
fn cylinder_faces_indexed(brep: &BRep) -> Vec<(usize, CylFace)> {
    let mut out = Vec::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ad = axis_dir.as_array();
        let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
        if len < cad_primitives::MIN_FEATURE_SIZE {
            continue;
        }
        let au = [ad[0] / len, ad[1] / len, ad[2] / len];
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for vi in face_loop_verts(brep, fi) {
            let Some(v) = brep.vertices().get(vi as usize) else {
                continue;
            };
            let p = v.point.as_array();
            let t = (p[0] - ap[0]) * au[0] + (p[1] - ap[1]) * au[1] + (p[2] - ap[2]) * au[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
        if !lo.is_finite() {
            lo = 0.0;
            hi = 0.0;
        }
        out.push((
            fi,
            CylFace {
                axis_point: ap,
                axis_dir: au,
                radius,
                extent: [lo, hi],
                reversed: f.reversed,
            },
        ));
    }
    out
}

/// One conformal ring on the shared cylinder: its axial parameter `z` (along
/// the axis from `axis_point`) and the ORDERED mesh-vertex indices around it
/// (CCW in the shared axis frame).
struct ConformalRing {
    z: f64,
    /// Vertex indices into the host solid's growing mesh vertex pool.
    ids: Vec<u32>,
}

/// Run §4.5.5 coincident-cylinder Stage-0 conformal re-tessellation.
///
/// `Ok(None)` — not Increment 1's case (no coincident pair, a face in >1 pair,
/// non-opposite, full-θ extents that don't yield a clean 1-contained-in-other
/// band, or a lateral whose rim rings cannot be extracted). The caller falls
/// back to the existing path (raw Stage-1 meshes / the planar Stage-0). This is
/// a LOUD-free fall-through: the downstream membrane resolution or the
/// `NonManifoldOutput` wall still fires if the config truly is unhandled.
///
/// `Ok(Some(_))` — both solids re-tessellated so the coincident overlap band is
/// bit-identical; feed the meshes to cherchi exactly as the planar overlay
/// output is fed.
pub(crate) fn coincident_cylinder_stage0(a: &BRep, b: &BRep) -> Result<Option<Stage0>, YangError> {
    let probe = std::env::var_os("CYLST0_PROBE").is_some();
    let groups = detect_coincident_cylinder_groups(a, b);
    if probe {
        eprintln!(
            "[cylst0] detected {} coincident cylinder groups",
            groups.len()
        );
        for (i, g) in groups.iter().enumerate() {
            eprintln!(
                "  group[{i}] fa={:?} fb={:?} opp={} ea=[{:.5},{:.5}] eb=[{:.5},{:.5}]",
                g.faces_a,
                g.faces_b,
                g.opposite,
                g.extent_a[0],
                g.extent_a[1],
                g.extent_b[0],
                g.extent_b[1]
            );
        }
    }
    if groups.len() != 1 {
        // Increment 1: exactly one coincident-cylinder GROUP. Zero → not our
        // case; >1 → a later increment (n-ary coincidence).
        return Ok(None);
    }
    let g = &groups[0];

    // Increment 1 scope gate: OPPOSITE-normal, full-θ, with one cluster's axial
    // extent CONTAINED in (or equal to) the other within the band.
    if !g.opposite {
        if probe {
            eprintln!("[cylst0] group not opposite");
        }
        return Ok(None);
    }
    let (lo_a, hi_a) = (g.extent_a[0], g.extent_a[1]);
    let (lo_b, hi_b) = (g.extent_b[0], g.extent_b[1]);
    let (outer_is_a, ov_lo, ov_hi) = {
        let a_contains_b = lo_a <= lo_b + g.band && hi_b <= hi_a + g.band;
        let b_contains_a = lo_b <= lo_a + g.band && hi_a <= hi_b + g.band;
        if a_contains_b {
            (true, lo_b, hi_b)
        } else if b_contains_a {
            (false, lo_a, hi_a)
        } else {
            if probe {
                eprintln!("[cylst0] partial overlap a=[{lo_a},{hi_a}] b=[{lo_b},{hi_b}]");
            }
            return Ok(None);
        }
    };

    // Tessellate both solids forcing the SAME circle-rim N (§4.5.5 identical
    // overlap meshes): two coincident cylinders sampled at different N produce
    // non-identical overlap rings cherchi cannot pocket-dedup. Probe each
    // solid's own N (its cluster's aggregate rings), then re-tessellate BOTH at
    // the max (a finer N only shrinks the sagitta — chord-valid for both, NOT a
    // tolerance relaxation).
    let verts_a: Vec<BRepVertex> = a.vertices().to_vec();
    let verts_b: Vec<BRepVertex> = b.vertices().to_vec();
    let probe0_a = stage1_tessellate(&verts_a, a.edges(), a.faces())?;
    let probe0_b = stage1_tessellate(&verts_b, b.edges(), b.faces())?;
    let n_a = cluster_rim_rings(&probe0_a, &g.faces_a, g.axis_point, g.axis_dir)
        .and_then(|r| r.first().map(|ring| ring.ids.len()));
    let n_b = cluster_rim_rings(&probe0_b, &g.faces_b, g.axis_point, g.axis_dir)
        .and_then(|r| r.first().map(|ring| ring.ids.len()));
    let shared_n = match (n_a, n_b) {
        (Some(na), Some(nb)) => na.max(nb),
        _ => {
            if probe {
                eprintln!("[cylst0] could not extract cluster ring N (na={n_a:?} nb={n_b:?})");
            }
            return Ok(None);
        }
    };
    let tess_a =
        crate::stage1_tessellate_min_segments(&verts_a, a.edges(), a.faces(), Some(shared_n))?;
    let tess_b =
        crate::stage1_tessellate_min_segments(&verts_b, b.edges(), b.faces(), Some(shared_n))?;

    let outer_tess = if outer_is_a { &tess_a } else { &tess_b };
    let outer_faces = if outer_is_a { &g.faces_a } else { &g.faces_b };
    let outer_reversed = if outer_is_a {
        a.faces()[g.faces_a[0]].reversed
    } else {
        b.faces()[g.faces_b[0]].reversed
    };
    let cont_tess = if outer_is_a { &tess_b } else { &tess_a };
    let cont_faces = if outer_is_a { &g.faces_b } else { &g.faces_a };

    let Some(outer_rings) = cluster_rim_rings(outer_tess, outer_faces, g.axis_point, g.axis_dir)
    else {
        if probe {
            eprintln!("[cylst0] outer cluster rings None");
        }
        return Ok(None);
    };
    let Some(cont_rings) = cluster_rim_rings(cont_tess, cont_faces, g.axis_point, g.axis_dir)
    else {
        if probe {
            eprintln!("[cylst0] cont cluster rings None");
        }
        return Ok(None);
    };
    // Increment 1: each clustered wall presents exactly 2 aggregate rim rings.
    if outer_rings.len() != 2 || cont_rings.len() != 2 {
        if probe {
            eprintln!(
                "[cylst0] ring count: outer={} cont={}",
                outer_rings.len(),
                cont_rings.len()
            );
        }
        return Ok(None);
    }

    let Some((outer_mesh, outer_tri_face)) = build_conformal_outer_mesh(
        outer_tess,
        outer_faces,
        &outer_rings,
        &cont_rings,
        cont_tess,
        g.axis_point,
        g.axis_dir,
        outer_reversed,
        ov_lo,
        ov_hi,
        g.band,
    ) else {
        if probe {
            eprintln!("[cylst0] build_conformal_outer_mesh None");
        }
        return Ok(None);
    };
    let cont_mesh = Mesh::new(cont_tess.verts.clone(), cont_tess.tris.clone());
    // N4: the contained mesh IS `cont_tess` unchanged → its face map is the
    // direct inversion of the face ranges (every triangle a real Stage-1 face).
    let cont_tri_face = invert_face_tri_ranges(cont_tess);

    let (mesh_a, mesh_b, tri_face_a, tri_face_b) = if outer_is_a {
        (outer_mesh, cont_mesh, outer_tri_face, cont_tri_face)
    } else {
        (cont_mesh, outer_mesh, cont_tri_face, outer_tri_face)
    };
    if probe {
        eprintln!(
            "[cylst0] HANDLED: outer_is_a={outer_is_a} outer_faces={outer_faces:?} \
             outer_rings_z={:?} cont_rings_z={:?} ov=[{ov_lo},{ov_hi}] N={shared_n} \
             mesh_a(v={},t={}) mesh_b(v={},t={})",
            outer_rings.iter().map(|r| r.z).collect::<Vec<_>>(),
            cont_rings.iter().map(|r| r.z).collect::<Vec<_>>(),
            mesh_a.verts.len(),
            mesh_a.tris.len(),
            mesh_b.verts.len(),
            mesh_b.tris.len(),
        );
    }

    debug_assert_eq!(tri_face_a.len(), mesh_a.tris.len(), "tri_face_a 1:1");
    debug_assert_eq!(tri_face_b.len(), mesh_b.tris.len(), "tri_face_b 1:1");
    Ok(Some(Stage0 {
        mesh_a,
        mesh_b,
        pairs: Vec::new(),
        // N4: per-triangle → face provenance for BOTH re-tessellated meshes, so
        // Stage-6 attributes coincident-cylinder overlaps by provenance rather
        // than geometric proximity (the last Stage-0 producer to gain this).
        tri_face_a,
        tri_face_b,
    }))
}

/// Extract a CLUSTER of cylinder faces' aggregate full-circle rim rings from
/// Stage-1 triangles: collect the unique vertices of ALL the cluster's faces,
/// group by axial parameter `z` along the shared axis, and order each ring CCW
/// in the shared axis frame. Aggregating over the (arc-patch) faces re-forms
/// the full-θ rings the gear's 4-arc-per-wall decomposition splits up.
/// `None` if the cluster does not present clean equal-size rings (≥ 3 each).
fn cluster_rim_rings(
    tess: &crate::Stage1Tess,
    faces: &[usize],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
) -> Option<Vec<ConformalRing>> {
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let zof = |p: [f64; 3]| -> f64 {
        (p[0] - axis_point[0]) * au[0]
            + (p[1] - axis_point[1]) * au[1]
            + (p[2] - axis_point[2]) * au[2]
    };
    let azof = |p: [f64; 3]| -> f64 {
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };

    // Collect unique cluster vertices (deduped across the arc faces — adjacent
    // arcs share their boundary ruling vertices), bucketed by axial level.
    let mut seen = std::collections::BTreeSet::new();
    let mut by_z: Vec<(f64, Vec<u32>)> = Vec::new();
    for &fi in faces {
        let range = tess.face_tri_ranges.get(fi)?.clone();
        for tri in &tess.tris[range] {
            for &v in tri {
                if !seen.insert(v) {
                    continue;
                }
                let z = zof(tess.verts[v as usize].as_array());
                let scale = z.abs().max(1.0);
                let zband = 1.0e-9 * scale;
                if let Some(slot) = by_z.iter_mut().find(|(zz, _)| (*zz - z).abs() <= zband) {
                    slot.1.push(v);
                } else {
                    by_z.push((z, vec![v]));
                }
            }
        }
    }
    by_z.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if by_z.len() < 2 {
        return None;
    }
    let nring = by_z[0].1.len();
    if nring < 3 || by_z.iter().any(|(_, ids)| ids.len() != nring) {
        return None;
    }
    // De-duplicate vertices at the SAME azimuth within a ring (an arc-patch
    // decomposition can list a shared ruling vertex once per incident arc — the
    // `seen` set already dedups by index, but two DISTINCT indices at the same
    // bit coordinates would double-count; guard by azimuth uniqueness).
    let mut rings = Vec::with_capacity(by_z.len());
    for (z, mut ids) in by_z {
        ids.sort_by(|&i, &j| {
            azof(tess.verts[i as usize].as_array())
                .total_cmp(&azof(tess.verts[j as usize].as_array()))
        });
        rings.push(ConformalRing { z, ids });
    }
    Some(rings)
}

/// N4: invert a Stage-1 tessellation's `face_tri_ranges` into a per-triangle →
/// owning-face map (1:1 with `tess.tris`), mirroring the `BRep::new` inversion.
fn invert_face_tri_ranges(tess: &crate::Stage1Tess) -> Vec<u32> {
    let mut tf = vec![0u32; tess.tris.len()];
    for (fi, range) in tess.face_tri_ranges.iter().enumerate() {
        for ti in range.clone() {
            tf[ti] = fi as u32;
        }
    }
    tf
}

/// The smallest CCW arc `(start, end)` covering all `azimuths` (each in
/// `[0, 2π)`): the circle minus the LARGEST cyclic gap between consecutive
/// (sorted) azimuths. `end < start` denotes an arc that wraps past 2π. `None`
/// when empty. Recovers an arc-patch cluster face's angular span from its rim
/// vertices (§4.5.5 coincident-cylinder provenance). Only used for a MULTI-face
/// cluster, where each face is a proper sub-arc (a single full-θ face is handled
/// without this — it owns every azimuth).
fn smallest_covering_arc(azimuths: &[f64]) -> Option<(f64, f64)> {
    if azimuths.is_empty() {
        return None;
    }
    let mut a: Vec<f64> = azimuths.to_vec();
    a.sort_by(|x, y| x.total_cmp(y));
    let m = a.len();
    if m == 1 {
        return Some((a[0], a[0]));
    }
    let tau = 2.0 * std::f64::consts::PI;
    // Start assuming the WRAP gap (last → first+2π) is the largest, so the
    // covering arc is the contiguous span [first, last] with no wrap.
    let mut best_gap = a[0] + tau - a[m - 1];
    let mut start = a[0];
    let mut end = a[m - 1];
    for i in 0..m - 1 {
        let gap = a[i + 1] - a[i];
        if gap > best_gap {
            // A larger interior gap → the arc wraps: it runs from the vertex
            // after the gap, past 2π, to the vertex before it.
            best_gap = gap;
            start = a[i + 1];
            end = a[i];
        }
    }
    Some((start, end))
}

/// Is `theta` within the CCW arc `[start, end]`? Wraps past 2π when `end < start`.
fn arc_contains(theta: f64, start: f64, end: f64) -> bool {
    if start <= end {
        theta >= start && theta <= end
    } else {
        theta >= start || theta <= end
    }
}

/// Per outer cluster face, its rim-vertex azimuth arc `(face_idx, start, end)`
/// in the shared axis frame — used to attribute a band-strip triangle to the
/// arc-patch face covering its column's azimuth.
fn cluster_face_arcs(
    outer_tess: &crate::Stage1Tess,
    outer_faces: &[usize],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
) -> Vec<(u32, f64, f64)> {
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let azof = |p: [f64; 3]| -> f64 {
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };
    let mut arcs = Vec::new();
    for &fi in outer_faces {
        let Some(range) = outer_tess.face_tri_ranges.get(fi) else {
            continue;
        };
        let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut azs: Vec<f64> = Vec::new();
        for t in &outer_tess.tris[range.clone()] {
            for &v in t {
                if seen.insert(v) {
                    azs.push(azof(outer_tess.verts[v as usize].as_array()));
                }
            }
        }
        if let Some((s, e)) = smallest_covering_arc(&azs) {
            arcs.push((fi as u32, s, e));
        }
    }
    arcs
}

/// Build the OUTER solid's conformal mesh: every face is its Stage-1
/// triangles, EXCEPT the coincident lateral, which is rebuilt as a banded strip
/// from its own two rim rings plus the contained solid's overlap-boundary rings
/// inserted as LITERAL COPIES (bit-identical vertices) at their z-levels. The
/// band strips between consecutive z-rings are paired by GLOBAL azimuth (the
/// merge convention — robust to the two solids' differing seam frames).
///
/// N4 (provenance): also returns a per-output-triangle → owning-face map
/// (1:1 with the mesh `tris`). Non-cluster triangles keep their Stage-1 face;
/// band-strip triangles are attributed to the arc-patch cluster face whose
/// azimuth arc contains the strip column's midpoint (trivial when the cluster
/// is a single face). A column that finds no covering arc (a floating-point
/// anomaly at a seam) gets the `u32::MAX` sentinel → geometric fallback.
#[allow(clippy::too_many_arguments)]
fn build_conformal_outer_mesh(
    outer_tess: &crate::Stage1Tess,
    outer_faces: &[usize],
    outer_rings: &[ConformalRing],
    cont_rings: &[ConformalRing],
    cont_tess: &crate::Stage1Tess,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    reversed: bool,
    ov_lo: f64,
    ov_hi: f64,
    band: f64,
) -> Option<(Mesh, Vec<u32>)> {
    let mut verts: Vec<Point3> = outer_tess.verts.clone();

    // Assemble the full set of conformal rings for the outer lateral, ordered by
    // z: the outer lateral's own two rims + the contained rings whose z lies
    // STRICTLY inside the outer extent (the overlap boundary). Contained-ring
    // vertices are appended as new mesh vertices (literal copies → bit-identical
    // to the contained solid's mesh).
    let mut all: Vec<ConformalRing> = Vec::new();
    for r in outer_rings {
        all.push(ConformalRing {
            z: r.z,
            ids: r.ids.clone(),
        });
    }
    let (z_lo, z_hi) = (outer_rings[0].z, outer_rings[outer_rings.len() - 1].z);
    let _ = (ov_lo, ov_hi);
    for r in cont_rings {
        // Insert the contained solid's rim rings that sit STRICTLY between the
        // outer rims (the overlap-band boundary; a ring AT an outer rim would be
        // a duplicate). The outer ring span IS the overlap geometry — using the
        // extracted ring z-levels (not the loop-vertex extent) is the reliable
        // truth, since a wall's tessellated rims can sit at different axial
        // params than its loop vertices' aggregate extent.
        if r.z <= z_lo + band || r.z >= z_hi - band {
            continue;
        }
        // Equal ring size required for the banded strip (Increment 1: same N).
        if r.ids.len() != outer_rings[0].ids.len() {
            if std::env::var_os("CYLST0_PROBE").is_some() {
                eprintln!(
                    "[cylst0] ring size mismatch: contained ring N={} vs outer N={}",
                    r.ids.len(),
                    outer_rings[0].ids.len()
                );
            }
            return None;
        }
        let mut ids = Vec::with_capacity(r.ids.len());
        for &v in &r.ids {
            let idx = verts.len() as u32;
            verts.push(cont_tess.verts[v as usize]);
            ids.push(idx);
        }
        all.push(ConformalRing { z: r.z, ids });
    }
    all.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap());

    // If nothing was inserted (extents equal, no interior boundary) the outer
    // lateral is already conformal with the contained one — no rebuild needed.
    // The mesh IS `outer_tess.tris` unchanged, so its face map is the direct
    // inversion of the face ranges.
    if all.len() == outer_rings.len() {
        let tri_face = invert_face_tri_ranges(outer_tess);
        return Some((Mesh::new(verts, outer_tess.tris.clone()), tri_face));
    }

    // Rebuild: keep all faces' triangles EXCEPT the coincident-cylinder cluster
    // faces (the arc patches), whose triangles are replaced by the re-banded
    // full-θ strip below.
    let mut in_cluster = vec![false; outer_tess.tris.len()];
    for &fi in outer_faces {
        let range = outer_tess.face_tri_ranges.get(fi)?.clone();
        for slot in in_cluster.iter_mut().take(range.end).skip(range.start) {
            *slot = true;
        }
    }
    // N4: face map built in lockstep with `tris`. Non-cluster triangles keep
    // their Stage-1 owning face; band-strip triangles are attributed by azimuth.
    let face_of = invert_face_tri_ranges(outer_tess);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut tri_face: Vec<u32> = Vec::new();
    for (i, tri) in outer_tess.tris.iter().enumerate() {
        if !in_cluster[i] {
            tris.push(*tri);
            tri_face.push(face_of[i]);
        }
    }
    // Per band-strip column midpoint azimuth → the arc-patch cluster face that
    // covers it. Single-face cluster: trivially that face (the full-θ wall).
    let single_face = (outer_faces.len() == 1).then(|| outer_faces[0] as u32);
    let arcs = if single_face.is_some() {
        Vec::new()
    } else {
        cluster_face_arcs(outer_tess, outer_faces, axis_point, axis_dir)
    };
    let face_at = |mid: f64| -> u32 {
        if let Some(f) = single_face {
            return f;
        }
        for &(fi, s, e) in &arcs {
            if arc_contains(mid, s, e) {
                return fi;
            }
        }
        u32::MAX // no covering arc → geometric fallback (P9-safe)
    };
    // Banded strip over consecutive z-rings.
    let probe = std::env::var_os("CYLST0_PROBE").is_some();
    if probe {
        eprintln!(
            "[cylst0] all rings z = {:?}",
            all.iter().map(|r| r.z).collect::<Vec<_>>()
        );
    }
    for w in all.windows(2) {
        if band_strip(
            &w[0],
            &w[1],
            &verts,
            axis_point,
            axis_dir,
            reversed,
            &mut tris,
            &face_at,
            &mut tri_face,
        )
        .is_none()
        {
            if probe {
                eprintln!("[cylst0] band_strip None at z=[{},{}]", w[0].z, w[1].z);
            }
            return None;
        }
    }
    debug_assert_eq!(tri_face.len(), tris.len(), "outer tri_face 1:1 with tris");
    Some((Mesh::new(verts, tris), tri_face))
}

/// Connect two cylinder rings (`lo`, `hi`) into a watertight quad strip, pairing
/// their vertices by GLOBAL azimuth (in the shared axis frame). Each ring must
/// present the SAME azimuth multiset (within a quarter-step tol — a missing
/// match is malformed, not fudged). Triangles are oriented radially outward
/// (inward for a `reversed` cavity wall), matching `tessellate_lateral_face`.
///
/// N4 (provenance): each column's two triangles are tagged with the owning
/// face via `face_at(column_midpoint_azimuth)`, pushed to `out_tri_face` in
/// lockstep with `out_tris`.
#[allow(clippy::too_many_arguments)]
fn band_strip(
    lo: &ConformalRing,
    hi: &ConformalRing,
    verts: &[Point3],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    reversed: bool,
    out_tris: &mut Vec<[u32; 3]>,
    face_at: &dyn Fn(f64) -> u32,
    out_tri_face: &mut Vec<u32>,
) -> Option<()> {
    let n = lo.ids.len();
    if n < 3 || hi.ids.len() != n {
        return None;
    }
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let azof = |vi: u32| -> f64 {
        let p = verts[vi as usize].as_array();
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };
    let mut lo_s: Vec<(f64, u32)> = lo.ids.iter().map(|&v| (azof(v), v)).collect();
    let mut hi_s: Vec<(f64, u32)> = hi.ids.iter().map(|&v| (azof(v), v)).collect();
    lo_s.sort_by(|a, b| a.0.total_cmp(&b.0));
    hi_s.sort_by(|a, b| a.0.total_cmp(&b.0));
    let tol = (2.0 * std::f64::consts::PI / n as f64) * 0.25;
    for k in 0..n {
        let mut d = (lo_s[k].0 - hi_s[k].0).abs();
        d = d.min(2.0 * std::f64::consts::PI - d);
        if d > tol {
            return None;
        }
    }
    let orient = |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let nrm = ring_radial_normal(verts, tri, axis_point, au);
        if reversed {
            [-nrm[0], -nrm[1], -nrm[2]]
        } else {
            nrm
        }
    };
    let tau = 2.0 * std::f64::consts::PI;
    for k in 0..n {
        let kn = (k + 1) % n;
        let b0 = lo_s[k].1;
        let b1 = lo_s[kn].1;
        let t0 = hi_s[k].1;
        let t1 = hi_s[kn].1;
        // Column midpoint azimuth (the wrap column advances the upper azimuth
        // by 2π so the mean lands inside the column, not on the far side).
        let a0 = lo_s[k].0;
        let mut a1 = lo_s[kn].0;
        if a1 < a0 {
            a1 += tau;
        }
        let mid = ((a0 + a1) * 0.5).rem_euclid(tau);
        let face = face_at(mid);
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let nrm = orient(verts, &tri);
            orient_band_tri(verts, &mut tri, nrm);
            out_tris.push(tri);
            out_tri_face.push(face);
        }
    }
    Some(())
}

/// Outward radial normal at a band triangle's centroid (local copy of
/// `radial_outward_normal`, kept inside Stage-0 to avoid widening its
/// visibility).
fn ring_radial_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    axis_point: [f64; 3],
    axis_unit: [f64; 3],
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let w = [
        cen[0] - axis_point[0],
        cen[1] - axis_point[1],
        cen[2] - axis_point[2],
    ];
    let along = w[0] * axis_unit[0] + w[1] * axis_unit[1] + w[2] * axis_unit[2];
    let radial = [
        w[0] - along * axis_unit[0],
        w[1] - along * axis_unit[1],
        w[2] - along * axis_unit[2],
    ];
    normalize3(radial)
}

/// Flip `tri` to align its geometric normal with `target` (local copy of
/// `orient_tri`).
fn orient_band_tri(verts: &[Point3], tri: &mut [u32; 3], target: [f64; 3]) {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let dot = cross[0] * target[0] + cross[1] * target[1] + cross[2] * target[2];
    if dot < 0.0 {
        tri.swap(1, 2);
    }
}

#[cfg(test)]
mod annulus_tests {
    //! Fold-pair emission RED oracle (spec `m8_stage0_fold_pair_emission`
    //! §6): F0027's measured configuration — a square outer ring whose
    //! corners fall on the CENTER side of distant inner chords' supporting
    //! lines. The angle-only merge fans those chords to invisible corners,
    //! double-covering part of the disc (the misoriented+improper census
    //! class). The exact coverage certificate (I2/E-F4) is the assertion:
    //! Σ triangle areas == area(outer) − area(inner), rational shoelace.

    use super::{annulus_tris, V2};
    use crate::coplanar_overlay::ExactPoint2;
    use cad_primitives::Point3;
    use dashu::rational::RBig;

    const Z: f64 = 0.236530362945883;

    fn v2(u: f64, v: f64) -> V2 {
        V2 {
            e: ExactPoint2::from_f64(u, v).expect("finite"),
            u,
            v,
            p: Point3::new(u, v, Z),
        }
    }

    /// The F0027 rings, verbatim from the dumped defective operand
    /// (square corners CCW; 11-gon rim CCW by ascending azimuth).
    fn f0027_rings() -> (Vec<V2>, Vec<V2>) {
        let outer = [
            (0.24933140012920343, -0.18511094772209571),
            (0.24933140012920343, 0.18511094772209571),
            (-0.24933140012920343, 0.18511094772209571),
            (-0.24933140012920343, -0.18511094772209571),
        ];
        let inner = [
            (-0.10624127713105047, -0.048518765551481664),
            (-0.06314462464930325, -0.09825495384444957),
            (0.0, -0.11679588852813404),
            (0.06314462464930323, -0.09825495384444959),
            (0.10624127713105048, -0.04851876555148163),
            (0.11560707478868232, 0.016621787986866053),
            (0.08826844304146464, 0.07648504128332562),
            (0.032905204303597765, 0.1120648343898067),
            (-0.032905204303597814, 0.11206483438980669),
            (-0.08826844304146471, 0.07648504128332555),
            (-0.11560707478868233, 0.016621787986866008),
        ];
        (
            outer.iter().map(|&(u, v)| v2(u, v)).collect(),
            inner.iter().map(|&(u, v)| v2(u, v)).collect(),
        )
    }

    /// Exact CCW shoelace area (×2) of a ring.
    fn ring_area2(ring: &[V2]) -> RBig {
        let n = ring.len();
        let mut a = RBig::ZERO;
        for i in 0..n {
            let p = &ring[i].e;
            let q = &ring[(i + 1) % n].e;
            a += &p.x * &q.y - &q.x * &p.y;
        }
        a
    }

    /// Exact area (×2) of an emitted triangle, from its (u,v) = (x,y)
    /// in-plane coordinates (the test plane is z=const with normal +z).
    fn tri_area2(t: &[Point3; 3]) -> RBig {
        let e: Vec<ExactPoint2> = t
            .iter()
            .map(|p| ExactPoint2::from_f64(p.x(), p.y()).expect("finite"))
            .collect();
        let dx1 = &e[1].x - &e[0].x;
        let dy1 = &e[1].y - &e[0].y;
        let dx2 = &e[2].x - &e[0].x;
        let dy2 = &e[2].y - &e[0].y;
        &dx1 * &dy2 - &dy1 * &dx2
    }

    /// RED (spec §6): the F0027 annulus must cover EXACTLY the region
    /// between the rings. Today the angle-only merge double-covers two
    /// pockets (fold pairs at corners 1 and 3), so Σ areas exceeds the
    /// annulus area and this certificate fails.
    #[test]
    fn f0027_annulus_coverage_certificate() {
        let (outer, inner) = f0027_rings();
        let tris = annulus_tris(&outer, &inner).expect("annulus must build");
        let annulus2 = ring_area2(&outer) - ring_area2(&inner);
        let mut covered2 = RBig::ZERO;
        let mut folded = 0usize;
        for t in &tris {
            let a2 = tri_area2(t);
            if a2 <= RBig::ZERO {
                folded += 1;
            }
            covered2 += a2;
        }
        assert_eq!(folded, 0, "annulus emitted non-positive-area triangles");
        assert_eq!(
            covered2,
            annulus2,
            "fold-pair RED — annulus triangulation does not cover the region \
             between the rings exactly (spec m8_stage0_fold_pair_emission I2): \
             covered {} vs annulus {} (×2, exact); the surplus is the measured \
             double-cover pleat at the invisible corners",
            covered2.to_f64().value(),
            annulus2.to_f64().value()
        );
    }
}

#[cfg(test)]
mod cylinder_pair_tests {
    use super::*;
    use crate::{BRepEdge, BRepFace, BRepVertex, Curve};
    use cad_primitives::{Point3, Vector3};

    /// Build a minimal closed-cylinder B-Rep: two full-circle rim edges at
    /// z=`z0` and z=`z1`, one lateral `Surface::Cylinder` face referencing both
    /// rims, with the given `reversed` flag. Axis = +Z through the origin.
    fn cylinder_brep(radius: f64, z0: f64, z1: f64, reversed: bool) -> BRep {
        // Two rim vertices (seam points) + the lateral face.
        let v0 = BRepVertex {
            point: Point3::new(radius, 0.0, z0),
        };
        let v1 = BRepVertex {
            point: Point3::new(radius, 0.0, z1),
        };
        let rim0 = BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        };
        let rim1 = BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z1),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        };
        let face = BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 1],
            inner_loops: vec![],
            reversed,
        };
        BRep::new(vec![v0, v1], vec![rim0, rim1], vec![face]).expect("build cylinder brep")
    }

    #[test]
    fn coaxial_bore_vs_wall_one_opposite_pair() {
        // A: a bore (cavity wall, reversed) of radius 2, z∈[0,5].
        // B: an outer wall (solid, not reversed) of the SAME cylinder, z∈[0,5].
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 0.0, 5.0, false);
        let pairs = detect_coincident_cylinder_pairs(&a, &b);
        assert_eq!(pairs.len(), 1, "exactly one coincident-cylinder pair");
        assert!(
            pairs[0].opposite,
            "bore (reversed) vs wall (not reversed) must be opposite"
        );
        assert!((pairs[0].radius - 2.0).abs() < 1e-12);
        assert!((pairs[0].axis_dir[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn coaxial_same_sense_not_opposite() {
        // Two solid walls of the same cylinder (both not reversed) → equal.
        let a = cylinder_brep(2.0, 0.0, 5.0, false);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let pairs = detect_coincident_cylinder_pairs(&a, &b);
        assert_eq!(pairs.len(), 1);
        assert!(!pairs[0].opposite, "same-sense walls are not opposite");
    }

    #[test]
    fn different_radius_no_pair() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(3.0, 0.0, 5.0, false);
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    #[test]
    fn offset_axis_no_pair() {
        // Same radius/direction but axis shifted off in x by 1 (parallel, not
        // collinear) → not coincident.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let mut b = cylinder_brep(2.0, 0.0, 5.0, false);
        // Shift B's axis off the line: rebuild with a translated axis_point.
        if let Surface::Cylinder {
            axis_dir, radius, ..
        } = b.faces()[0].surface
        {
            let new_face = BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(1.0, 0.0, 0.0),
                    axis_dir,
                    radius,
                },
                outer_loop: b.faces()[0].outer_loop.clone(),
                inner_loops: vec![],
                reversed: b.faces()[0].reversed,
            };
            b = BRep::new(b.vertices().to_vec(), b.edges().to_vec(), vec![new_face])
                .expect("rebuild offset cylinder");
        }
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    #[test]
    fn disjoint_axial_extent_no_pair() {
        // Coaxial, equal radius, but z-ranges do not overlap.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 10.0, 15.0, false);
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    // ── M8-cyl Increment 1: group detection (cluster + cross-pairing) ──────

    #[test]
    fn cluster_single_cylinder_is_one_cluster() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let clusters = cluster_cylinder_faces(&a);
        assert_eq!(clusters.len(), 1, "one cylinder face → one cluster");
        assert_eq!(clusters[0].faces.len(), 1);
        assert!(clusters[0].reversed);
        assert!((clusters[0].radius - 2.0).abs() < 1e-12);
    }

    #[test]
    fn coincident_group_opposite_pair() {
        // Bore (reversed) z∈[0,5] vs an outward wall z∈[1,4]: one group, the
        // wall contained, opposite normals.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let groups = detect_coincident_cylinder_groups(&a, &b);
        assert_eq!(groups.len(), 1, "exactly one coincident cylinder group");
        assert!(groups[0].opposite, "bore vs wall must be opposite");
        assert_eq!(groups[0].faces_a, vec![0]);
        assert_eq!(groups[0].faces_b, vec![0]);
        // A's extent contains B's.
        assert!(groups[0].extent_a[0] <= groups[0].extent_b[0] + groups[0].band);
        assert!(groups[0].extent_b[1] <= groups[0].extent_a[1] + groups[0].band);
    }

    #[test]
    fn different_radius_no_group() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(3.0, 0.0, 5.0, false);
        assert!(detect_coincident_cylinder_groups(&a, &b).is_empty());
    }

    #[test]
    fn coincident_stage0_emits_valid_tri_face() {
        // N4 (coincident-cylinder provenance): the handled path must emit a
        // per-triangle → face map 1:1 with each produced mesh, so Stage-6 can
        // attribute by cherchi provenance instead of geometric proximity.
        // Single-cluster-face case: bore (reversed) z∈[0,5] vs outward wall
        // z∈[1,4]; A is the containing (outer) extent.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let s0 = coincident_cylinder_stage0(&a, &b)
            .expect("must not error")
            .expect("must reach the handled path");

        // I1: 1:1 with the meshes, non-empty (the whole point).
        assert_eq!(s0.tri_face_a.len(), s0.mesh_a.tris.len(), "A map 1:1");
        assert_eq!(s0.tri_face_b.len(), s0.mesh_b.tris.len(), "B map 1:1");
        assert!(
            !s0.tri_face_a.is_empty() && !s0.tri_face_b.is_empty(),
            "coincident-cylinder Stage-0 must emit provenance"
        );

        // I2: every entry is a valid face index or the u32::MAX fallback.
        let na = a.faces().len() as u32;
        let nb = b.faces().len() as u32;
        assert!(
            s0.tri_face_a.iter().all(|&f| f < na || f == u32::MAX),
            "A face indices valid"
        );
        assert!(
            s0.tri_face_b.iter().all(|&f| f < nb || f == u32::MAX),
            "B face indices valid"
        );

        // outer_is_a here (A contains B): A is the outer, one cluster face (0);
        // its band-strip tris all attribute to real faces, never the sentinel.
        assert!(
            s0.tri_face_a.iter().all(|&f| f != u32::MAX),
            "single-cluster outer fully attributed (no sentinel)"
        );
        // I3: the contained mesh (B) is the full Stage-1 re-tessellation — every
        // tri is a real face; the lateral-only helper has one face (0).
        assert!(
            s0.tri_face_b.iter().all(|&f| f == 0),
            "contained lateral-only cylinder: all tris on face 0"
        );
    }

    #[test]
    fn arc_helpers_partition_the_circle() {
        use std::f64::consts::PI;
        // I4: a quarter-arc face's vertices span [0, π/2]; the largest gap is
        // the rest of the circle → covering arc is exactly [0, π/2] (no wrap).
        let q = [0.0, PI / 6.0, PI / 3.0, PI / 2.0];
        let (s, e) = smallest_covering_arc(&q).unwrap();
        assert!((s - 0.0).abs() < 1e-12 && (e - PI / 2.0).abs() < 1e-12);
        assert!(arc_contains(PI / 4.0, s, e), "midpoint inside");
        assert!(!arc_contains(PI, s, e), "opposite side outside");

        // A face straddling the 0/2π seam: vertices near 2π and near 0 → the
        // covering arc WRAPS (end < start).
        let w = [0.1, 0.2, 2.0 * PI - 0.2, 2.0 * PI - 0.1];
        let (s, e) = smallest_covering_arc(&w).unwrap();
        assert!(s > PI && e < PI, "wrap arc: s={s} e={e}");
        assert!(
            arc_contains(0.0, s, e),
            "seam azimuth 0 is inside the wrap arc"
        );
        assert!(arc_contains(2.0 * PI - 0.15, s, e));
        assert!(!arc_contains(PI, s, e), "far side outside");

        // Two adjacent quarter arcs partition [0, π]: a midpoint lands in
        // exactly one (they meet at the shared seam π/2, never a column mid).
        let a0 = smallest_covering_arc(&[0.0, PI / 4.0, PI / 2.0]).unwrap();
        let a1 = smallest_covering_arc(&[PI / 2.0, 3.0 * PI / 4.0, PI]).unwrap();
        assert!(arc_contains(PI / 8.0, a0.0, a0.1) && !arc_contains(PI / 8.0, a1.0, a1.1));
        assert!(
            arc_contains(3.0 * PI / 4.0, a1.0, a1.1) && !arc_contains(3.0 * PI / 4.0, a0.0, a0.1)
        );
    }

    #[test]
    fn coincident_stage0_returns_none_on_lateral_only_breps() {
        // The lateral-only test helper is not a closed solid (no caps); its
        // clusters present but the rebuild has no incident caps, so the path
        // either falls back (Ok(None)) or handles it — it must NOT error and
        // must not panic.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let r = coincident_cylinder_stage0(&a, &b);
        assert!(
            r.is_ok(),
            "coincident_cylinder_stage0 must never error here"
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

    use super::{gate_tri_valid, relocate_minted_vertex, Frame};
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
        assert!(relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, false
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
        assert!(!relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, false
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
        assert!(relocate_minted_vertex(
            &mut tris, &mut class, &mut em, 0, &coords, &frame, false
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
}

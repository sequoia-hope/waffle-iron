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
mod reloc;
#[allow(unused_imports)]
pub(crate) use reloc::*;
mod rim_chords;
#[allow(unused_imports)]
pub(crate) use rim_chords::*;
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

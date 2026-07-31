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
//! Handled: A×B pairs of PLANAR faces with all-`LineSegment` loops (plus
//! the disc/annular/mixed extensions of the 1×1 path). Pairs are processed
//! in PLANE GROUPS (spec `m8_plane_group_nary_overlay`, `stage0::nary`): a
//! face in MULTIPLE pairs joins its partners in one n-ary overlay when
//! every group face is a pure line-loop polygon with per-side uniform
//! orientation. Everything else stays
//! `YangError::CoplanarFacesUnsupported`: intra-solid near pairs (the
//! chained-output class — with only A×B pairs overlaid, intra-solid
//! near-coplanarity has no Stage-0 resolution and would still build femto
//! sliver patches), curved faces, multi-pair groups carrying a
//! disc/annular/mixed face, and overlay engine failures (e.g.
//! `RoundingCollapse` on sub-ulp in-plane slivers).
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
mod frame;
#[allow(unused_imports)]
pub(crate) use frame::*;
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
mod nary;
#[allow(unused_imports)]
pub(crate) use nary::*;

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
/// unsupported-residue sub-class fired (`intra-solid` / `face-unsupported`
/// / `polygon2d-*` / `overlay-failed` / `build-mesh-*` / `nary-*`).
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
    // TEMP characterization probe (#130 F0082 Extrude-12 layer): dump B's
    // full face list + A faces whose loop AABB covers the given (x,z)
    // column, format "x,z". Read-only, env-gated.
    if let Ok(spec) = std::env::var("YANG_OPFACE_DUMP") {
        let parts: Vec<f64> = spec.split(',').filter_map(|s| s.parse().ok()).collect();
        if parts.len() == 2 {
            let (cx, cz) = (parts[0], parts[1]);
            let tol = 1e-3;
            for (tag, brep, filter) in [("B", b, false), ("A", a, true)] {
                for (fi, f) in brep.faces().iter().enumerate() {
                    if filter {
                        let mut lo = [f64::INFINITY; 3];
                        let mut hi = [f64::NEG_INFINITY; 3];
                        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                            for &e in lp {
                                if let Some(edge) = brep.edges().get(e as usize) {
                                    for vi in [edge.start, edge.end] {
                                        if let Some(v) = brep.vertices().get(vi as usize) {
                                            let p = v.point.as_array();
                                            for k in 0..3 {
                                                lo[k] = lo[k].min(p[k]);
                                                hi[k] = hi[k].max(p[k]);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !(lo[0] - tol <= cx
                            && cx <= hi[0] + tol
                            && lo[2] - tol <= cz
                            && cz <= hi[2] + tol)
                        {
                            continue;
                        }
                    }
                    eprintln!("[opface] {tag}#{fi} surface={:?}", f.surface);
                }
            }
        }
    }
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
    if std::env::var_os("YANG_COPLANAR_PROBE").is_some() {
        for p in &scan.cross {
            probe(
                "cross-pair",
                &format!(
                    "pair=({},{}) band={:.3e} gap={:.3e} subres={} sa={:?} sb={:?}",
                    p.face_a,
                    p.face_b,
                    p.band,
                    p.gap,
                    p.sub_resolution,
                    a.faces()[p.face_a].surface,
                    b.faces()[p.face_b].surface
                ),
            );
        }
    }

    // #178 (spec `yang_178_subres_coplanar_gap_stop.md`): a matched cross
    // pair whose planes are DISTINCT (offset gap above the rounding-noise
    // class) means the model carries a sub-resolution feature between two
    // real parallel planes — the overlay would dissolve it silently (the
    // measured C0111/C0113 χ 0→2 wall dissolve). Out-of-contract input:
    // STOP loudly before any overlay work (first pair in scan order,
    // deterministic).
    if let Some(p) = scan.cross.iter().find(|p| p.sub_resolution) {
        return Err(YangError::SubResolutionCoplanarGap {
            face_a: p.face_a,
            face_b: p.face_b,
            gap: p.gap,
            band: p.band,
        });
    }

    // ── Scope validation ────────────────────────────────────────────────
    let pair_err = |face_a: usize, face_b: usize| YangError::CoplanarFacesUnsupported {
        input_a: InputId::A,
        face_a,
        input_b: InputId::B,
        face_b,
    };
    for p in &scan.cross {
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

    // ── Plane groups (spec `m8_plane_group_nary_overlay`): connected
    // components of the pair graph over shared faces. Singleton groups run
    // the historical 1×1 path byte-identically; multi-pair groups run the
    // n-ary overlay (`overlay_nary_group`). ─────────────────────────────
    let groups = build_plane_groups(&scan.cross);

    // ── Snap phase (ONE canonical plane per group, deterministic order) ─
    let mut va: Vec<Point3> = a.vertices().iter().map(|v| v.point).collect();
    let mut vb: Vec<Point3> = b.vertices().iter().map(|v| v.point).collect();
    let mut frames: Vec<Frame> = Vec::with_capacity(groups.len());
    for g in &groups {
        let first = &scan.cross[g.pair_idxs[0]];
        // Group canonical plane: the LOWEST participating A face (for a
        // singleton group this IS the pair's face_a — the historical frame).
        let frame = canonical_frame(a, g.faces_a[0]).ok_or_else(|| {
            probe(
                "frame-degenerate",
                &format!("pair=({},{})", first.face_a, first.face_b),
            );
            pair_err(first.face_a, first.face_b)
        })?;
        for &fa in &g.faces_a {
            for vi in face_loop_verts(a, fa) {
                va[vi as usize] = frame.snap(va[vi as usize]);
            }
        }
        for &fb in &g.faces_b {
            for vi in face_loop_verts(b, fb) {
                vb[vi as usize] = frame.snap(vb[vi as usize]);
            }
        }
        // Cross-weld: a B loop vertex landing on the SAME in-plane (u,v)
        // as an A loop vertex takes A's coordinates — the §4.5.5 symbolic
        // reconciliation that makes shared corners bit-identical across the
        // two solids (e.g. the stacked-box corners 1e-13 apart pre-snap).
        let key = |p: Point3, f: &Frame| {
            let (u, v) = f.project(p);
            (u.to_bits(), v.to_bits())
        };
        let a_keys: BTreeMap<(u64, u64), u32> = g
            .faces_a
            .iter()
            .flat_map(|&fa| face_loop_verts(a, fa))
            .map(|vi| (key(va[vi as usize], &frame), vi))
            .collect();
        for &fb in &g.faces_b {
            for vi in face_loop_verts(b, fb) {
                if let Some(&ai) = a_keys.get(&key(vb[vi as usize], &frame)) {
                    vb[vi as usize] = va[ai as usize];
                }
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

    for (g, frame) in groups.iter().zip(&frames) {
        // Multi-pair plane group → the n-ary overlay path (spec
        // `m8_plane_group_nary_overlay` B2–B5); its scope walls keep the
        // typed residue for disc/annular/mixed faces and mixed orientation.
        if g.pair_idxs.len() > 1 {
            overlay_nary_group(
                a,
                b,
                g,
                &scan.cross,
                frame,
                &va,
                &vb,
                &mut pairs,
                &mut overrides_a,
                &mut overrides_b,
                &mut splits_a,
                &mut splits_b,
                &mut rim_overrides_a,
                &mut rim_overrides_b,
                &probe,
            )?;
            continue;
        }
        let p = &scan.cross[g.pair_idxs[0]];
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
        if annular_hole_rim_crossing(a, p.face_a, b, p.face_b)
            || annular_hole_rim_crossing(b, p.face_b, a, p.face_a)
        {
            probe(
                "annular-hole-rim-crossing",
                &format!("pair=({},{})", p.face_a, p.face_b),
            );
            return Err(pair_err(p.face_a, p.face_b));
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

        // §16 inc-0 census probe (read-only, spec
        // `m8_stage0_multiclass_cavity_arm` §16): cross-table sub-band
        // rim-anchor pairs — the congruent-rim cross-solid divergence class
        // (C0048 base-tri-207 femto needle → cherchi DegenerateTpi). The
        // rim-aware clustering PROTECTS on-circle points (moving one drags
        // it off its circle), so a junction azimuth on a SHARED congruent
        // circle survives as one rim_a anchor + one rim_b anchor at
        // ulp-different exact on-circle values. Census before designing the
        // amendment-18 election.
        if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
            for (ka, pa3) in &rim_a {
                for (kb, pb3) in &rim_b {
                    let du = (&ka.x - &kb.x).to_f64().value();
                    let dv = (&ka.y - &kb.y).to_f64().value();
                    let d2 = du * du + dv * dv;
                    let scale = ka.x.to_f64().value().abs().max(ka.y.to_f64().value().abs());
                    let band = cad_primitives::TAU_WORK * (1.0 + scale);
                    if d2 < band * band && pa3 != pb3 {
                        eprintln!(
                            "[rim-table-twin] pair=({},{}) uv_dist={:e} a={:?} b={:?}",
                            p.face_a,
                            p.face_b,
                            d2.sqrt(),
                            pa3.as_array(),
                            pb3.as_array()
                        );
                    }
                }
            }
        }

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
            mixed_chord_ctxs(
                a,
                &poly_a,
                &curved_masks_a,
                std::slice::from_ref(&poly_b),
                frame,
            )
        } else if rim_a.is_empty() {
            Vec::new()
        } else {
            rim_chord_ctxs(a, p.face_a, &poly_a, std::slice::from_ref(&poly_b), frame)
        };
        let rim_ctxs_b = if !curved_masks_b.is_empty() {
            mixed_chord_ctxs(
                b,
                &poly_b,
                &curved_masks_b,
                std::slice::from_ref(&poly_a),
                frame,
            )
        } else if rim_b.is_empty() {
            Vec::new()
        } else {
            rim_chord_ctxs(b, p.face_b, &poly_b, std::slice::from_ref(&poly_a), frame)
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
        // Amendment 13 (spec `m8_stage0_multiclass_cavity_arm` §10): a
        // vertex is MERGEABLE iff it is a pure sweep-event discretization
        // vertex — not a corner of either input, not an input rim sample,
        // not itself minted (the `lift_or_snap` resolution branch). Only
        // such a vertex may be position-merged into a mint by the Fig-11
        // merge arm below; every other provenance stays immovable (P10).
        let mut mergeable_mark = vec![false; overlay.verts.len()];
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
                    mergeable_mark[i] = true;
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
                    mergeable_mark[i] = minted.is_none();
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
        // Amendment 16 (spec §14): the groups are RECORDED for the revert
        // authorities (the amendment-2 fallback below and the §10d settle
        // check) — a qualified group reverts WHOLE to ONE shared chord
        // target, or the tear ships a real-scale phantom pair (the C0048
        // 68v67 azimuth-merge wall) and DESYNCS the pair's interface
        // meshes (F0067's manufactured N17 deferral). Qualification is
        // sub-floor ANCHORING: every member's own chord lift within
        // MIN_FEATURE_SIZE of the elected member's — the class the
        // collapse was designed for. A wide-anchored group (coincident
        // junction images from far anchors, the measured (222,286) class)
        // is NOT qualified: per-member semantics, byte-identical,
        // census-probed. ALWAYS-ON since the inc-2 corpus flip.
        let mut collapse_groups = CollapseGroups::default();
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
                let elected_2d = overlay.verts[target_vi];
                let shared = frame.lift(elected_2d.x(), elected_2d.y());
                let sf = cad_primitives::MIN_FEATURE_SIZE;
                let sub_floor_anchored = g.iter().all(|&(vi, _)| {
                    let q = overlay.verts[vi];
                    let l = frame.lift(q.x(), q.y()).as_array();
                    let s = shared.as_array();
                    let d = [l[0] - s[0], l[1] - s[1], l[2] - s[2]];
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < sf * sf
                });
                if sub_floor_anchored {
                    let mut member_ids: Vec<usize> = g.iter().map(|&(vi, _)| vi).collect();
                    // Amendment 17 (spec §15): sub-band LIFT absorption. A
                    // femto 2D cluster can carry NON-minted members (a
                    // chord-world lift from another sweep column — F0067's
                    // vert 189) that the slot machinery never groups; left
                    // out, the lift resolves 1 ulp from the group target and
                    // the emission carries BOTH values (divergent interface
                    // chains → cherchi LabelMismatch). Absorb every
                    // non-minted, non-corner, non-rim-anchored vertex whose
                    // EXACT uv distance to the elected member is within the
                    // rounding-noise band TAU_WORK·(1+uv_scale) — five
                    // orders above the measured cluster (4.3e-14), three
                    // below the protected E-C1b genuinely-distinct twin
                    // population (~1e-9) — and enroll it as a full group
                    // member (minted_mark + the §14 carrier), so the
                    // amendment-16 atomic revert covers it in both
                    // directions. ALWAYS-ON since the inc-2 corpus flip
                    // (2026-07-31: zero category deltas; F0067 advances two
                    // stages past cherchi to the §4.5.2 wall).
                    {
                        let elected_exact = overlay.exact_verts[target_vi].clone();
                        let tq = overlay.verts[target_vi];
                        let uv_scale = tq.x().abs().max(tq.y().abs());
                        let band = cad_primitives::TAU_WORK * (1.0 + uv_scale);
                        if let Ok(band_r) = rat(band) {
                            let band2 = &band_r * &band_r;
                            for vi in 0..overlay.exact_verts.len() {
                                if minted_mark[vi]
                                    || member_ids.contains(&vi)
                                    || collapse_groups.members.contains_key(&vi)
                                {
                                    continue;
                                }
                                let exact = &overlay.exact_verts[vi];
                                if corners_a.contains_key(exact)
                                    || corners_b.contains_key(exact)
                                    || rim_a.contains_key(exact)
                                    || rim_b.contains_key(exact)
                                {
                                    continue;
                                }
                                let du = &exact.x - &elected_exact.x;
                                let dv = &exact.y - &elected_exact.y;
                                if &du * &du + &dv * &dv > band2 {
                                    continue;
                                }
                                if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                                    eprintln!(
                                        "[mint-collapse] lift-absorb vert {vi} -> vert \
                                         {target_vi} {target:?} (spec §15)"
                                    );
                                }
                                coords[vi] = target;
                                minted_mark[vi] = true;
                                member_ids.push(vi);
                            }
                        }
                    }
                    for &vi in &member_ids {
                        collapse_groups.members.insert(vi, member_ids.clone());
                        collapse_groups.shared_lift.insert(vi, shared);
                    }
                } else if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                    eprintln!(
                        "[mint-collapse] slot={slot} group -> vert {target_vi} NOT \
                         sub-floor-anchored (revert stays per-member; spec §14d WATCH)"
                    );
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

        // Amendment 13 (spec `m8_stage0_multiclass_cavity_arm` §10d): the
        // Fig-11(b→c) MERGE arm (inc-3.1) + the rim-chain boundary-order
        // settle check (inc-3.5), ALWAYS-ON since the inc-3.6 corpus flip
        // (2026-07-30: zero CORRECT→ERROR; F0067/F0072 recategorized onto
        // the loud typed coplanar wall). The Fig-11(a) SPLIT arm is
        // measurement-only pending the vertex-inserting design (§10d
        // inc-3.2).
        // Amendment 13 inc-3.5 bookkeeping (spec §10d): every committed
        // merge is recorded as (p, q, p's original position) so a later
        // revert of the TARGET — by the amendment-2 fallback or the
        // boundary-order settle check below — restores its partner:
        // merges propagate through the revert path exactly like mints do.
        // `merge_settled` marks targets that are no longer mergeable (their
        // keep was reverted); both are inert gate-OFF (no merges recorded).
        let mut merges: Vec<(u32, u32, Point3)> = Vec::new();
        let mut merge_settled: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let probe_flip = std::env::var_os("YANG_SPLIT_PROBE").is_some();
        // Amendment 14 (spec §11): the vertex-inserting split, ALWAYS-ON
        // since the inc-3.2d corpus flip (2026-07-30: the only category
        // change was R0099 ERROR → SUPPORTED_CORRECT — the conversion
        // itself). `split_extras` carries the split's q-points into the
        // rim-override chains (§11c step 3 — the A-leg); any extra left
        // unconsumed by the collectors below is a T-junction in waiting
        // and fails the pair loudly.
        let mut split_extras: Vec<ExtraRimPoint> = Vec::new();
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
                // Amendment 13: the first Fig-11 backtrack pair surfaced by
                // a per-vertex NonSimple reject (the SINGLETON class never
                // reaches the joint form); a region-form candidate below
                // takes precedence when the joint path runs. `split_pair`
                // is the Fig-11(a) form: (q, a, b) — reroute chord (a,b)
                // through the mint q.
                let mut merge_pair: Option<(u32, u32, f64, f64)> = None;
                let mut split_pair: Option<(u32, u32, u32)> = None;
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
                        RelocOutcome::NonSimple {
                            ring_mints,
                            merge_candidate,
                            split_chord,
                        } => {
                            saw_nonsimple = true;
                            joint_seeds.insert(vv);
                            joint_seeds.extend(ring_mints);
                            if merge_pair.is_none() {
                                merge_pair = merge_candidate;
                            }
                            if split_pair.is_none() {
                                split_pair = split_chord.map(|(a, b)| (vv, a, b));
                            }
                        }
                        RelocOutcome::Rejected => {
                            joint_seeds.insert(vv);
                        }
                    }
                }
                if !relocated && saw_nonsimple && joint_seeds.len() >= 2 {
                    let seeds: Vec<u32> = joint_seeds.into_iter().collect();
                    match relocate_minted_region(
                        &mut overlay.tris,
                        &mut overlay.class,
                        &mut edge_map,
                        &seeds,
                        &coords,
                        frame,
                        &minted_mark,
                        probe_flip,
                    ) {
                        RegionOutcome::Committed => {
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
                        RegionOutcome::MergeCandidate {
                            p: mp,
                            q: mq,
                            overshoot,
                            chord_len,
                        } => {
                            // The region form's candidate outranks a
                            // wedge-level one (it is derived from the
                            // deeper, grown form).
                            merge_pair = Some((mp, mq, overshoot, chord_len));
                        }
                        RegionOutcome::Rejected => {}
                    }
                }
                // ── Amendment 13 (spec `m8_stage0_multiclass_cavity_arm`
                // §10, gated): the Fig-11(b→c) MERGE. A rejecting ring
                // walks out past the mint mq and BACKTRACKS — the
                // overshooting constrained chord crosses mq's exit edge,
                // and no growth can cross a constraint. The paper's
                // operation: the too-close endpoint mp merges with mq. mp
                // must be a pure discretization vertex (`mergeable_mark`)
                // and the merge is position-only: mp becomes a bit-twin of
                // mq, the backtrack edge collapses, its slivers go
                // bit-degenerate (dropped at emission, M-B), and the next
                // gate pass re-attempts the whole ladder on the merged
                // geometry. Idempotent by the bit-inequality guard; merges
                // strictly reduce distinct positions (bounded), so the
                // lexicographic (merges, folds) termination argument holds.
                let mut merged = false;
                if !relocated {
                    if let Some((mp, mq, overshoot, chord_len)) = merge_pair {
                        // Displacement guard (§10c): the merge may only
                        // absorb a vertex inside the zone the mint's own
                        // displacement SWEPT OVER — ‖p−q‖ ≤ ‖q − chord(q)‖.
                        // Per-mint and scale-free, not a tuned band: p
                        // beyond the swept zone is real geometry the moved
                        // boundary never touched (the base()-shaped false
                        // positive fails this by an order of magnitude).
                        let qv = overlay.verts[mq as usize];
                        let chord = frame.lift(qv.x(), qv.y());
                        let d3 = |a: Point3, b: Point3| {
                            let (da, db) = (a.as_array(), b.as_array());
                            let d = [da[0] - db[0], da[1] - db[1], da[2] - db[2]];
                            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                        };
                        let disp = d3(coords[mq as usize], chord);
                        let gap = d3(coords[mp as usize], coords[mq as usize]);
                        // Containment guard (§10d inc-3.4): Fig 11(b)'s
                        // premise is that q lies ON the split edge — the
                        // overshoot must be within the chord's own
                        // circle-approximation error (sagitta from the
                        // mint's rim-slot radius). The R0059 counterexample
                        // (overshoot/chord ≈ 0.5, a unit-scale
                        // interpenetration admitted by the displacement
                        // guard alone at model scale ~300) fails this by
                        // orders; the R0099 true grazes (≈ 1e-4) pass.
                        let sagitta = minted_info
                            .iter()
                            .find(|&&(vi, _, _)| vi == mq as usize)
                            .and_then(|&(_, slot, _)| {
                                rim_ctxs_a.iter().chain(rim_ctxs_b.iter()).nth(slot)
                            })
                            .map(|ctx| chord_len * chord_len / (8.0 * ctx.radius));
                        let contained = sagitta.is_some_and(|s| overshoot <= s);
                        // inc-3.5 measurement: is p on the union boundary
                        // (any 1-incident edge at p)? A boundary-vertex
                        // merge moves geometry other faces' meshes were
                        // built against — the R0059 seam suspect.
                        if probe_flip {
                            let p_boundary = edge_map
                                .iter()
                                .any(|(k, e)| (k[0] == mp || k[1] == mp) && e.len() == 1);
                            let q_boundary = edge_map
                                .iter()
                                .any(|(k, e)| (k[0] == mq || k[1] == mq) && e.len() == 1);
                            eprintln!(
                                "  [fold-merge-boundary] p={mp} on_union_boundary={p_boundary} \
                                 q={mq} on_union_boundary={q_boundary}"
                            );
                        }
                        if mergeable_mark[mp as usize]
                            && coords[mp as usize] != coords[mq as usize]
                            && !merge_settled.contains(&mq)
                            // One live merge per partner: a second absorb of
                            // the same p would record a corrupted origin (the
                            // first target's position) and make restoration
                            // ambiguous — refuse it, the amendment-2 revert
                            // stays the fallback (no measured customer).
                            && !merges.iter().any(|&(pp, _, _)| pp == mp)
                            && gap <= disp
                            && contained
                        {
                            if probe_flip {
                                eprintln!(
                                    "[fold-merge] pair=({},{}) tri {ti} p={mp} -> q={mq} \
                                     gap={gap:e} disp={disp:e} overshoot={overshoot:e} \
                                     sagitta={sagitta:?} ({:?} -> {:?})",
                                    p.face_a, p.face_b, coords[mp as usize], coords[mq as usize]
                                );
                            }
                            merges.push((mp, mq, coords[mp as usize]));
                            coords[mp as usize] = coords[mq as usize];
                            merged = true;
                            changed = true;
                        } else if probe_flip {
                            eprintln!(
                                "[fold-merge-reject] pair=({},{}) tri {ti} p={mp} q={mq} \
                                 mergeable={} settled={} gap={gap:e} disp={disp:e} \
                                 overshoot={overshoot:e} sagitta={sagitta:?}",
                                p.face_a,
                                p.face_b,
                                mergeable_mark[mp as usize],
                                merge_settled.contains(&mq),
                            );
                        }
                    }
                }
                // ── Amendment 14 Fig-11(a) SPLIT (spec
                // `m8_stage0_multiclass_cavity_arm` §11, ALWAYS-ON since
                // the inc-3.2d flip): the vertex-inserting split. The
                // 1-incident chord is the OTHER input's real model edge;
                // the mint's on-circle position bulges a hair past it
                // (near-tangency the chord-geometry arrangement never
                // saw). q_a/q_b are minted with exact rational UVs ON the
                // chord — the other-input propagation leg rides
                // `collect_edge_splits` for free — and the cavity re-cuts
                // per §11c; the A-leg propagates via `split_extras` into
                // the rim-override chains below.
                let mut split_done = false;
                if !relocated && !merged {
                    if let Some((sq, sa, sb)) = split_pair {
                        let slot = minted_info
                            .iter()
                            .find(|&&(vi, _, _)| vi == sq as usize)
                            .map(|&(_, slot, _)| slot);
                        let ctx =
                            slot.and_then(|s| rim_ctxs_a.iter().chain(rim_ctxs_b.iter()).nth(s));
                        if let (Some(slot), Some(ctx)) = (slot, ctx) {
                            let (au, av) = frame.project(coords[sa as usize]);
                            let (bu, bv) = frame.project(coords[sb as usize]);
                            let clen = ((au - bu).powi(2) + (av - bv).powi(2)).sqrt();
                            let sag = clen * clen / (8.0 * ctx.radius);
                            if fig11_split_cavity(
                                &mut overlay,
                                &mut edge_map,
                                sq,
                                (sa, sb),
                                &mut coords,
                                &mut minted_mark,
                                &mut mergeable_mark,
                                frame,
                                Some(sag),
                                &ctx.chords,
                                &ctx.other_segs,
                                slot < rim_ctxs_a.len(),
                                ctx.center,
                                &mut split_extras,
                                probe_flip,
                            ) {
                                split_done = true;
                                changed = true;
                            }
                        } else if probe_flip {
                            eprintln!("  [split-reject] vert {sq} no rim-slot context");
                        }
                    }
                }
                if probe_flip && !relocated && !merged && !split_done {
                    if let Some((sq, sa, sb)) = split_pair {
                        let inc_n = edge_map
                            .get(&edge_key(sa, sb))
                            .map(|e| e.len())
                            .unwrap_or(0);
                        eprintln!(
                            "  [fold-split-reject] q={sq} chord=({sa},{sb}) {inc_n}-incident                              (no split arm accepted)"
                        );
                        // inc-3.2 anatomy probe: exact in-frame UVs of the
                        // candidate mint (current = minted position), its
                        // pre-mint UV, the chord endpoints, and every
                        // class-boundary / boundary neighbor of sq — the
                        // measured inputs of the §11 split design (offline
                        // frame reconstruction is too lossy at 1e-4 scale).
                        let pr = |v: u32| frame.project(coords[v as usize]);
                        let (mu, mv) = pr(sq);
                        let quv = overlay.verts[sq as usize];
                        eprintln!(
                            "  [fold-split-anatomy] q={sq} minted_uv=({mu},{mv}) \
                             chord_uv=({},{})",
                            quv.x(),
                            quv.y()
                        );
                        for (&[ka, kb], inc) in edge_map.iter() {
                            if ka != sq && kb != sq {
                                continue;
                            }
                            let w = if ka == sq { kb } else { ka };
                            let classes: Vec<_> =
                                inc.iter().map(|&ti2| overlay.class[ti2]).collect();
                            let boundaryish =
                                inc.len() == 1 || classes.windows(2).any(|c| c[0] != c[1]);
                            if boundaryish {
                                let (wu, wv) = pr(w);
                                eprintln!(
                                    "  [fold-split-anatomy]   nbr {w} uv=({wu},{wv}) \
                                     inc={} classes={classes:?}",
                                    inc.len()
                                );
                            }
                        }
                        let (au, av) = pr(sa);
                        let (bu, bv) = pr(sb);
                        eprintln!(
                            "  [fold-split-anatomy]   chord {sa}=({au},{av}) {sb}=({bu},{bv})"
                        );
                    }
                }
                if relocated || merged || split_done {
                    continue;
                }

                // ── Amendment 2 fallback: revert the fold's minted
                // vertices to today's chord lift (still observable via
                // kernel-v2's vertex-on-surface tripwire — never silently
                // blessed). Amendment 16 (spec §14): a qualified sub-floor
                // collapse group reverts WHOLE — every member to the ONE
                // shared chord target — or not at all; a per-member revert
                // tears the A14.2 identification into a real-scale phantom
                // pair (the C0048 68v67 azimuth-merge wall).
                let area = tri_area(&t, &coords);
                for &v in &t {
                    let vi = v as usize;
                    if !minted_mark[vi] {
                        continue;
                    }
                    for m in collapse_groups.revert_unit(vi) {
                        let q = overlay.verts[m];
                        let lifted = collapse_groups.effective_lift(m, frame.lift(q.x(), q.y()));
                        if coords[m] == lifted {
                            continue;
                        }
                        if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                            let tag = if m == vi { "" } else { " (group-atomic)" };
                            eprintln!(
                                "[fold-revert] pair=({},{}) vert={m} area={area:e} \
                                 minted={:?} -> chord {lifted:?}{tag}",
                                p.face_a, p.face_b, coords[m]
                            );
                        }
                        coords[m] = lifted;
                        changed = true;
                        // inc-3.5: a reverted mint is no longer a valid
                        // merge target — restore any partner merged into
                        // it (else the partner is a stale bit-twin of a
                        // position no vertex holds) and block re-merges.
                        merge_settled.insert(m as u32);
                        for &(mp, mq, orig) in &merges {
                            if mq as usize == m && coords[mp as usize] != orig {
                                if probe_flip {
                                    eprintln!(
                                        "[fold-revert]   merge partner {mp} of \
                                         reverted target {mq} restored"
                                    );
                                }
                                coords[mp as usize] = orig;
                            }
                        }
                    }
                }
            }
            if !changed {
                // ── Amendment 13 inc-3.5 (spec §10d): rim-chain boundary-
                // order settle check, run at quiescence. The cap overlay
                // emits each rim chord's crossing chain in chord-parameter
                // order; the ring builder re-orders the same points by
                // azimuth. A kept junction mint beside a fold-reverted
                // neighbor can azimuthally leap past it, desynchronizing
                // the two consumers (the R0059 seam). The check reverts the
                // displaced member of an inverted pair (amendment-2
                // semantics at chord granularity), restores merge partners,
                // and re-runs the ladder. The inversion class also exists
                // merge-free (R0059 op 002, canonical-latent), so the
                // check polices every pair, not just merged ones.
                let mut n = settle_rim_chain_order(
                    &rim_ctxs_a,
                    &overlay,
                    &mut coords,
                    &minted_mark,
                    frame,
                    &merges,
                    &mut merge_settled,
                    &collapse_groups,
                    probe_flip,
                );
                if n == 0 {
                    n = settle_rim_chain_order(
                        &rim_ctxs_b,
                        &overlay,
                        &mut coords,
                        &minted_mark,
                        frame,
                        &merges,
                        &mut merge_settled,
                        &collapse_groups,
                        probe_flip,
                    );
                }
                if n > 0 {
                    continue;
                }
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
        // inc-3.5: positions of SURVIVING merge targets — the split
        // collector collapses a merged pair's two same-position entries to
        // one (empty gate-OFF, so the historical path is byte-identical).
        let merged_pts: std::collections::BTreeSet<[u64; 3]> = merges
            .iter()
            .filter(|&&(mp, mq, _)| coords[mp as usize] == coords[mq as usize])
            .map(|&(_, mq, _)| {
                let a = coords[mq as usize].as_array();
                [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
            })
            .collect();
        collect_edge_splits(
            a,
            p.face_a,
            &va,
            frame,
            &cluster_map,
            &overlay,
            [RegionClass::AOnly, RegionClass::Overlap],
            &coords,
            &merged_pts,
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
            &merged_pts,
            &mut splits_b,
        );

        // PR-M8 disc-rim crossing: a disc whose rim the overlap boundary
        // crosses propagates each crossing point into its OWN cap rim AND the
        // opposite cap rim of the same cylinder (and thus the lateral, which
        // shares both rims). OPPOSITE-normal only (the SCOPE GATE above).
        let extras_a: Vec<ExtraRimPoint> =
            split_extras.iter().filter(|x| x.side_a).cloned().collect();
        let extras_b: Vec<ExtraRimPoint> =
            split_extras.iter().filter(|x| !x.side_a).cloned().collect();
        let mut extras_consumed = 0usize;
        if rim_cross_a {
            match collect_rim_crossings(
                a,
                p.face_a,
                &poly_a,
                &overlay,
                &coords,
                &extras_a,
                &mut rim_overrides_a,
            ) {
                Ok(n) => extras_consumed += n,
                Err(tag) => {
                    probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                    return Err(pair_err(p.face_a, p.face_b));
                }
            }
        }
        if rim_cross_b {
            match collect_rim_crossings(
                b,
                p.face_b,
                &poly_b,
                &overlay,
                &coords,
                &extras_b,
                &mut rim_overrides_b,
            ) {
                Ok(n) => extras_consumed += n,
                Err(tag) => {
                    probe(tag, &format!("pair=({},{})", p.face_a, p.face_b));
                    return Err(pair_err(p.face_a, p.face_b));
                }
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
        // §11c step 3 accounting: every split-minted rim point must have
        // reached a rim-override chain — an unconsumed extra is a
        // T-junction between this pair's emission and the rim's other
        // consumers (the lateral, the opposite cap). Fail the pair loudly
        // rather than emit the seam (P10).
        if extras_consumed < split_extras.len() {
            probe(
                "split-extras-unconsumed",
                &format!(
                    "pair=({},{}) consumed {extras_consumed} of {}",
                    p.face_a,
                    p.face_b,
                    split_extras.len()
                ),
            );
            return Err(pair_err(p.face_a, p.face_b));
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

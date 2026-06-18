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
    coplanar_overlay, cross_r, ClassifiedOverlay, ExactPoint2, PolygonWithHoles, RegionClass,
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

/// Output of Stage-0 coplanar preprocessing.
pub(crate) struct Stage0 {
    pub(crate) mesh_a: Mesh,
    pub(crate) mesh_b: Mesh,
    pub(crate) pairs: Vec<PairPlane>,
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
        let disc_pair =
            disc_circle_edge(a, p.face_a).is_some() || disc_circle_edge(b, p.face_b).is_some();
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
        let (poly_a, corners_a, rim_a) = face_polygon_2d_tessellated(a, p.face_a, &va, frame)
            .ok_or_else(|| {
                probe("polygon2d-a", &format!("pair=({},{})", p.face_a, p.face_b));
                pair_err(p.face_a, p.face_b)
            })?;
        let (poly_b, corners_b, rim_b) = face_polygon_2d_tessellated(b, p.face_b, &vb, frame)
            .ok_or_else(|| {
                probe("polygon2d-b", &format!("pair=({},{})", p.face_a, p.face_b));
                pair_err(p.face_a, p.face_b)
            })?;

        let overlay = coplanar_overlay(&poly_a, &poly_b).map_err(|e| {
            probe(
                "overlay-failed",
                &format!("pair=({},{}) err={e:?}", p.face_a, p.face_b),
            );
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

        // SCOPE GATE — only OPPOSITE-normal disc∩polygon crossings route
        // through. Same-normal makes equal-winding overlap copies meet edge-on
        // → cherchi N13 `SingleCoplanarEdge` → loud downstream failure; keep it
        // the fast loud coplanar residue.
        if (rim_cross_a || rim_cross_b) && !opposite {
            probe(
                "disc-crossing-same-normal",
                &format!("pair=({},{})", p.face_a, p.face_b),
            );
            return Err(pair_err(p.face_a, p.face_b));
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

        // Resolve every overlay vertex to ONE solid-independent 3D point:
        // corner of face A → A's (snapped/welded) vertex; corner of face B
        // → B's; rim point → the exact 3D rim point; otherwise the frame lift
        // L(u,v) (snapped to a rim point if it lands within ε of one). Shared
        // between BOTH solids' meshes so the Overlap triangles are bit-identical.
        let coords: Vec<Point3> = (0..overlay.verts.len())
            .map(|i| {
                let exact = &overlay.exact_verts[i];
                if let Some(&ai) = corners_a.get(exact) {
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
                        frame.lift(qx, qy)
                    }
                }
            })
            .collect();

        // Per-solid override triangles. Overlay triangles are CCW in the
        // (e1, e2) frame ⇒ normal +n̂ (e1×e2 = n̂): face A keeps the order
        // (n̂ IS its outward normal); face B swaps iff its outward opposes.
        let tris_for = |keep: [RegionClass; 2], swap: bool| -> Vec<[Point3; 3]> {
            overlay
                .tris
                .iter()
                .zip(&overlay.class)
                .filter(|(_, c)| keep.contains(c))
                .map(|(t, _)| {
                    let mut tri = [
                        coords[t[0] as usize],
                        coords[t[1] as usize],
                        coords[t[2] as usize],
                    ];
                    if swap {
                        tri.swap(1, 2);
                    }
                    tri
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
    }

    // ── Stage-1 re-tessellation with overrides + propagated splits ──────
    let report_pair = (scan.cross[0].face_a, scan.cross[0].face_b);
    let mesh_a = build_stage0_mesh(a, &va, &overrides_a, &splits_a, &rim_overrides_a).map_err(
        |e| match e {
            BuildErr::Yang(y) => y,
            BuildErr::Unsupported => {
                probe("build-mesh-a", &format!("pair={report_pair:?}"));
                pair_err(report_pair.0, report_pair.1)
            }
        },
    )?;
    let mesh_b = build_stage0_mesh(b, &vb, &overrides_b, &splits_b, &rim_overrides_b).map_err(
        |e| match e {
            BuildErr::Yang(y) => y,
            BuildErr::Unsupported => {
                probe("build-mesh-b", &format!("pair={report_pair:?}"));
                pair_err(report_pair.0, report_pair.1)
            }
        },
    )?;

    Ok(Some(Stage0 {
        mesh_a,
        mesh_b,
        pairs,
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
    let cap_edge = disc_circle_edge(brep, fi).ok_or("rim-not-disc")?;
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

    // Shared azimuth basis (same `ortho_basis(axis)` for both rims → a global
    // azimuth, exactly what the lateral azimuth-merge uses).
    let (a1, a2) = ortho_basis(cad_primitives::Vector3::new(
        axis_dir[0],
        axis_dir[1],
        axis_dir[2],
    ));
    let (a1, a2) = (a1.as_array(), a2.as_array());
    let azimuth = |p: [f64; 3]| -> f64 {
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * a1[0] + w[1] * a1[1] + w[2] * a1[2];
        let y = w[0] * a2[0] + w[1] * a2[1] + w[2] * a2[2];
        y.atan2(x)
    };

    let ring = &poly.outer;
    let n = ring.len();
    if n < 2 {
        return Err("rim-poly-degenerate");
    }
    let cap_entry = rim_overrides.entry(cap_edge).or_default();
    let mut cap_pts: Vec<Point3> = Vec::new();
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
                continue;
            }
            // The BIT-EXACT shared point (the cap override uses the same one).
            let pt = coords[vi];
            if cap_pts.contains(&pt) {
                continue;
            }
            cap_pts.push(pt);
        }
    }
    for &pt in &cap_pts {
        if !cap_entry.contains(&pt) {
            cap_entry.push(pt);
        }
    }

    // Project each crossing's azimuth onto the OPPOSITE rim circle (exact
    // radius, on the opposite cap's plane). The opposite rim's `ortho_basis`
    // frame differs from the axis frame; place the point by world geometry so
    // it lands at the SAME global azimuth as the cap crossing.
    let (o1, o2) = ortho_basis(opp_normal);
    let (o1, o2) = (o1.as_array(), o2.as_array());
    let oc = opp_center.as_array();
    let opp_entry = rim_overrides.entry(opp_edge).or_default();
    for &pt in &cap_pts {
        let az = azimuth(pt.as_array());
        // Build the opposite-rim point at global azimuth `az`: choose the angle
        // in the opposite circle's own frame whose world azimuth equals `az`.
        // Try both senses of the opposite frame's e2 relative to the axis.
        let cand = |theta: f64| -> [f64; 3] {
            let (ct, st) = theta.sin_cos();
            [
                oc[0] + opp_radius * (st * o1[0] + ct * o2[0]),
                oc[1] + opp_radius * (st * o1[1] + ct * o2[1]),
                oc[2] + opp_radius * (st * o1[2] + ct * o2[2]),
            ]
        };
        // Solve for theta so that azimuth(cand(theta)) == az. The opposite
        // circle's frame maps theta→world; azimuth is a fixed rotation/flip of
        // theta, so a 1D search over a fine grid + refine is robust and avoids
        // sign-convention pitfalls (deterministic, no trig inversion guesswork).
        let mut best_theta = 0.0;
        let mut best_err = f64::INFINITY;
        let steps = 720usize;
        for k in 0..steps {
            let theta = std::f64::consts::TAU * (k as f64) / (steps as f64);
            let mut d = (azimuth(cand(theta)) - az).abs();
            d = d.min(std::f64::consts::TAU - d);
            if d < best_err {
                best_err = d;
                best_theta = theta;
            }
        }
        // Refine by bisection-free local sampling around best_theta.
        let mut span = std::f64::consts::TAU / steps as f64;
        for _ in 0..40 {
            let mut local_best = best_theta;
            let mut local_err = best_err;
            for s in [-1.0_f64, 1.0] {
                let theta = best_theta + s * span;
                let mut d = (azimuth(cand(theta)) - az).abs();
                d = d.min(std::f64::consts::TAU - d);
                if d < local_err {
                    local_err = d;
                    local_best = theta;
                }
            }
            best_theta = local_best;
            best_err = local_err;
            span *= 0.5;
        }
        let opp_pt3 = cand(best_theta);
        let opp_pt = Point3::new(opp_pt3[0], opp_pt3[1], opp_pt3[2]);
        if !opp_entry.contains(&opp_pt) {
            opp_entry.push(opp_pt);
        }
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
        return DiscPair::Wall("disc-disc-crossing");
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

    // Merge the two monotone chains into a strip triangulation.
    let tri = |a: &V2, b: &V2, c: &V2| -> [Point3; 3] {
        if cross_r(&a.e, &b.e, &c.e) >= RBig::ZERO {
            [a.p, b.p, c.p]
        } else {
            [a.p, c.p, b.p]
        }
    };
    let mut out: Vec<[Point3; 3]> = Vec::with_capacity(ni + no);
    let (mut i, mut j) = (0usize, 0usize);
    let mut guard = 0usize;
    while i < ni || j < no {
        guard += 1;
        if guard > ni + no + 8 {
            return None;
        }
        let advance_inner = if i >= ni {
            false
        } else if j >= no {
            true
        } else {
            ia[i + 1] <= oa[j + 1]
        };
        if advance_inner {
            let t = tri(&inner[io[i]], &inner[io[i + 1]], &outer[oo[j]]);
            if !degenerate(&t) {
                out.push(t);
            }
            i += 1;
        } else {
            let t = tri(&outer[oo[j]], &outer[oo[j + 1]], &inner[io[i]]);
            if !degenerate(&t) {
                out.push(t);
            }
            j += 1;
        }
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
        let (su, sv) = frame.project(coords[lo as usize]);
        let (eu, ev) = frame.project(coords[hi as usize]);
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
            if &dx * &wy - &dy * &wx != RBig::ZERO {
                continue;
            }
            let t = (&dx * &wx + &dy * &wy) / &len2;
            if t <= RBig::ZERO || t >= RBig::ONE {
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

/// Build one solid's Stage-0 mesh: the normal Stage-1 tessellation over the
/// SNAPPED vertex coordinates, with overlay faces' triangles replaced by
/// the overlay triangulation and split-edge neighbor faces re-triangulated
/// with the subdivided boundary ring.
fn build_stage0_mesh(
    brep: &BRep,
    final_coords: &[Point3],
    overrides: &BTreeMap<usize, Vec<[Point3; 3]>>,
    splits: &SplitMap,
    rim_overrides: &RimSplitMap,
) -> Result<Mesh, BuildErr> {
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
    for (f_idx, f) in brep.faces().iter().enumerate() {
        if let Some(ov_tris) = overrides.get(&f_idx) {
            for tri in ov_tris {
                let mut t = [0u32; 3];
                for (k, p) in tri.iter().enumerate() {
                    t[k] = intern_vert(&mut verts, &mut intern, *p);
                }
                tris.push(t);
            }
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
            continue;
        }

        // Neighbor re-triangulation with the subdivided ring. Scope: planar,
        // all-LineSegment, hole-free, continuous outer loop.
        let Surface::Plane { normal, .. } = f.surface else {
            return Err(BuildErr::Unsupported);
        };
        if !f.inner_loops.is_empty() || !overlay_face_supported(brep, f_idx) {
            return Err(BuildErr::Unsupported);
        }
        let n = f.outer_loop.len();
        let mut ring: Vec<u32> = Vec::new();
        for i in 0..n {
            let e_idx = f.outer_loop[i];
            let edge = &brep.edges()[e_idx as usize];
            let next = &brep.edges()[f.outer_loop[(i + 1) % n] as usize];
            if edge.end != next.start {
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
            triangulate_ring(&ring, &verts, normal.as_array()).ok_or(BuildErr::Unsupported)?;
        tris.extend(ring_tris);
    }

    Ok(Mesh::new(verts, tris))
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
/// Why a verified apex-fan and NOT a generic ear-clip: the split points on
/// a subdivided edge are only NEARLY collinear with its corners in 3D (the
/// shared-plane lift `o + u·e1 + v·e2` cannot realize exact 2D collinearity
/// through f64 rounding on an oblique plane — the chain is femto-crooked).
/// An ear-clip is free to clip a long ear whose closing diagonal SPANS the
/// crooked chain, leaving a femto-sliver polygon between the diagonal and
/// the chain; those sliver triangles then femto-interpenetrate the overlay
/// face across the hinge and the arrangement faithfully builds
/// unclassifiable sliver patches (`NoExplicitRayOrigin` — the original
/// PR-YR24 failure mode, reintroduced by the re-tessellation). A fan from a
/// corner OFF the chain keeps every crooked sub-segment as a real triangle
/// boundary, so the neighbor and the overlay face stay edge-conforming and
/// no diagonal sliver can exist. The strict-positivity verification is
/// exact (rationals over the dominant-frame projection); a candidate that
/// fails (e.g. a corner whose own edge carries splits — collinear or
/// reflex fan triangles) is skipped deterministically.
fn triangulate_ring(ring: &[u32], verts: &[Point3], normal: [f64; 3]) -> Option<Vec<[u32; 3]>> {
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
    None
}

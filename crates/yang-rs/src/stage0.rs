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
    normalize3, ortho_basis, scan_near_coplanar, stage1_tessellate, BRep, BRepEdge, BRepVertex,
    Curve, InputId, Mesh, Surface, YangError,
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
                    &format!("pair=({},{}) face={fi} planar={planar}", p.face_a, p.face_b),
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

        // Shared-frame 2D polygons (and per-corner (u,v) keys).
        let (poly_a, corners_a) = face_polygon_2d(a, p.face_a, &va, frame).ok_or_else(|| {
            probe("polygon2d-a", &format!("pair=({},{})", p.face_a, p.face_b));
            pair_err(p.face_a, p.face_b)
        })?;
        let (poly_b, corners_b) = face_polygon_2d(b, p.face_b, &vb, frame).ok_or_else(|| {
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

        // Resolve every overlay vertex to ONE solid-independent 3D point:
        // corner of face A → A's (snapped/welded) vertex; corner of face B
        // → B's; otherwise the frame lift L(u,v). Shared between BOTH
        // solids' meshes so the Overlap triangles are bit-identical.
        let coords: Vec<Point3> = (0..overlay.verts.len())
            .map(|i| {
                let exact = &overlay.exact_verts[i];
                if let Some(&ai) = corners_a.get(exact) {
                    va[ai as usize]
                } else if let Some(&bi) = corners_b.get(exact) {
                    vb[bi as usize]
                } else {
                    let q = overlay.verts[i];
                    frame.lift(q.x(), q.y())
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
        // face's boundary edges propagate to the adjacent faces.
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
    }

    // ── Stage-1 re-tessellation with overrides + propagated splits ──────
    let report_pair = (scan.cross[0].face_a, scan.cross[0].face_b);
    let mesh_a = build_stage0_mesh(a, &va, &overrides_a, &splits_a).map_err(|e| match e {
        BuildErr::Yang(y) => y,
        BuildErr::Unsupported => {
            probe("build-mesh-a", &format!("pair={report_pair:?}"));
            pair_err(report_pair.0, report_pair.1)
        }
    })?;
    let mesh_b = build_stage0_mesh(b, &vb, &overrides_b, &splits_b).map_err(|e| match e {
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

/// Is this face overlay-supported: planar surface, every loop edge a
/// `Curve::LineSegment`?
fn overlay_face_supported(brep: &BRep, fi: usize) -> bool {
    let f = &brep.faces()[fi];
    matches!(f.surface, Surface::Plane { .. })
        && std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .flatten()
            .all(|&e| matches!(brep.edges()[e as usize].curve, Curve::LineSegment))
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

// ════════════════════════════════════════════════════════════════════════
// boundary-split propagation (§4.5.5 shared sampling points)
// ════════════════════════════════════════════════════════════════════════

/// Splits keyed by the UNDIRECTED endpoint vertex-index pair (B-Rep edges
/// are commonly duplicated per face — e.g. the box fixtures carry 24
/// directed edges over 12 undirected segments — so geometric identity is
/// the vertex pair, not the edge index). Each split: exact parameter along
/// the canonical `min(vi) → max(vi)` direction + the shared 3D coordinate.
type SplitMap = BTreeMap<(u32, u32), Vec<(RBig, Point3)>>;

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
) -> Result<Mesh, BuildErr> {
    let brep_verts: Vec<BRepVertex> = final_coords
        .iter()
        .map(|&p| BRepVertex { point: p })
        .collect();
    let tess = stage1_tessellate(&brep_verts, brep.edges(), brep.faces())?;

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

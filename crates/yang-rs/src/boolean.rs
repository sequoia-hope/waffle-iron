//! The `boolean()` driver — PR-YR3 vertex provenance, PR-YR4 triangle
//! attribution, Stage-0 coplanar scan glue, KV15 near-weld, phantom
//! rim N, rim-junction overrides (extracted verbatim from lib.rs —
//! spec `specs/yang_rs_lib_decomposition.md`, increment 9).

#[allow(clippy::wildcard_imports)]
use crate::*;

mod coplanar_scan;
mod junction;
mod predicates;
mod provenance;
mod rim_junction;
pub(crate) use coplanar_scan::*;
pub(crate) use junction::*;
pub(crate) use predicates::*;
pub(crate) use provenance::*;
pub(crate) use rim_junction::*;

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
pub(crate) fn flip_for_op(op: BoolOp, la: &LabeledArrangement, t: usize) -> bool {
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

/// #146 increment 3a (spec `specs/yang_146_collapsed_wedge_dedup.md` §2):
/// classify a coincident post-weld surviving-triangle pair at the I6 site.
/// Returns `None` iff the candidate is a COLLAPSED WEDGE — one surface
/// strip folded shut by the I6 weld — safe to drop in favour of the kept
/// representative; otherwise the named reject reason (the pair falls
/// through to the post-loop I6 `NonManifoldInput` backstop unchanged).
///
/// The signature is exact and structural (no tolerance beyond the weld
/// that already fused the pair): same final winding, same surface label,
/// a shared raw arrangement edge with weld-fused tips, and both triangles
/// descending from DISTINCT parent triangles of the SAME B-Rep FACE of the
/// SAME input (via the `tri_face` provenance maps — one surface strip
/// folding shut lives inside one face; measured on F0016, whose parents
/// B46/B47 share a face but NOT a mesh edge, which refuted the stricter
/// parent-adjacency arm). Genuine coincident-sheet inputs are DIFFERENT
/// B-Rep faces — or carry no lineage at all (the a4 adversary class) —
/// and still STOP.
#[allow(clippy::too_many_arguments)] // a pure classifier over the I6 site's locals
pub(crate) fn wedge_reject_reason(
    raw_first: [u32; 3],
    raw_cur: [u32; 3],
    welded_first: [u32; 3],
    welded_cur: [u32; 3],
    weld: &[u32],
    src_first: &[(LaInputId, u32)],
    src_cur: &[(LaInputId, u32)],
    surface_first: &[LaInputId],
    surface_cur: &[LaInputId],
    tri_face_a: &[u32],
    tri_face_b: &[u32],
) -> Option<&'static str> {
    // §2.1 same final winding: cyclic equality of the post-flip triples. An
    // opposite-winding pair is a collapsed two-sided pocket — out of scope
    // (spec §4), backstop STOPs.
    if !(0..3).any(|r| (0..3).all(|k| welded_cur[(k + r) % 3] == welded_first[k])) {
        return Some("winding");
    }
    // §2.2 same surface label (also forces an equal per-op flip decision).
    if surface_first != surface_cur {
        return Some("surface");
    }
    // §2.3 shared raw edge + weld-fused tips: the pair became coincident
    // THROUGH the weld, tiling one strip side-by-side.
    let shared: Vec<u32> = raw_first
        .iter()
        .copied()
        .filter(|v| raw_cur.contains(v))
        .collect();
    if shared.len() != 2 {
        return Some("raw-shared");
    }
    let tip = |raw: [u32; 3]| raw.iter().copied().find(|v| !shared.contains(v));
    let (Some(tip_first), Some(tip_cur)) = (tip(raw_first), tip(raw_cur)) else {
        return Some("raw-shared");
    };
    if weld[tip_first as usize] != weld[tip_cur as usize] {
        return Some("tips-not-welded");
    }
    // §2.4 locally-connected provenance: single-valued lineage, same input,
    // distinct parent triangles of the SAME B-Rep face (the `tri_face`
    // provenance maps). A wedge is ONE surface strip folding shut inside one
    // face; independent coincident sheets are different B-Rep faces.
    // (Measured on F0016: parents B46/B47 share the face but NOT a mesh
    // edge — the intersection-minted strip edge is not an inherited parent
    // edge, so parent-tri adjacency is the WRONG locality notion.)
    let (&[(input_first, parent_first)], &[(input_cur, parent_cur)]) = (src_first, src_cur) else {
        return Some("lineage");
    };
    if input_first != input_cur {
        return Some("cross-input");
    }
    if parent_first == parent_cur {
        return Some("same-parent");
    }
    let faces = if input_first == LaInputId(0) {
        tri_face_a
    } else {
        tri_face_b
    };
    if faces.is_empty() {
        return Some("no-face-map");
    }
    let (Some(&face_first), Some(&face_cur)) = (
        faces.get(parent_first as usize),
        faces.get(parent_cur as usize),
    ) else {
        return Some("parent-range");
    };
    if face_first != face_cur {
        return Some("parents-not-same-face");
    }
    None
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
pub(crate) fn stage0_dump(
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

/// Task #134: conservative world AABB of a B-Rep — the vertex hull expanded
/// by every periodic curve's full-circle bounds (center ± radius on every
/// axis; ellipse by its major radius) and by the bulging closed surfaces
/// (sphere: center ± r; torus: center ± (R + r)). Plane / cylinder / cone
/// faces are inside the hull of their boundary bounds (planar faces by hull
/// convexity; laterals by the hull of their rim circles + apex vertices).
/// `None` when an edge carries an open unbounded-bulge curve (hyperbola /
/// parabola / surface-pair) — no fast path.
fn conservative_aabb(brep: &BRep) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut grow = |p: [f64; 3], r: f64| {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k] - r);
            hi[k] = hi[k].max(p[k] + r);
        }
    };
    for v in brep.vertices() {
        grow(v.point.as_array(), 0.0);
    }
    for e in brep.edges() {
        match e.curve {
            Curve::LineSegment => {}
            Curve::Circle { center, radius, .. } => grow(center.as_array(), radius),
            Curve::Ellipse {
                center,
                major_radius,
                ..
            } => grow(center.as_array(), major_radius),
            Curve::Parabola { .. } | Curve::Hyperbola { .. } | Curve::SurfacePair { .. } => {
                return None
            }
        }
    }
    for f in brep.faces() {
        match f.surface {
            Surface::Sphere { center, radius } => grow(center.as_array(), radius),
            Surface::Torus {
                center,
                major_radius,
                minor_radius,
                ..
            } => grow(center.as_array(), major_radius + minor_radius),
            _ => {}
        }
    }
    lo[0].is_finite().then_some((lo, hi))
}

/// Task #134: are the operands' conservative AABBs strictly disjoint on
/// some axis, beyond the YR24 near-coplanar weld band? Public so kernel-v2
/// can apply the SAME predicate for its arena-level disjoint-union merge
/// (the yang passthrough output is INPUT-convention topology, which
/// `from_yang_brep` does not ingest — kernel-v2 merges shells natively
/// instead).
pub fn union_operands_strictly_disjoint(a: &BRep, b: &BRep) -> bool {
    let (Some((lo_a, hi_a)), Some((lo_b, hi_b))) = (conservative_aabb(a), conservative_aabb(b))
    else {
        return false;
    };
    let scale = hi_a
        .iter()
        .chain(lo_a.iter())
        .chain(hi_b.iter())
        .chain(lo_b.iter())
        .fold(0.0_f64, |m, &v| m.max(v.abs()));
    // The margin must EXCEED the YR24 near-coplanar weld band
    // (`near_coplanar_band` = max(TAU_MODEL, scale·TAU_WORK)): a pair whose
    // gap is inside that band is welded by Stage-0 (the yr27 near-partial
    // r=1e-8 class) and must NOT take the disjoint fast path. Factor 2 for
    // comfort.
    let band = 2.0 * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    (0..3).any(|k| lo_a[k] > hi_b[k] + band || lo_b[k] > hi_a[k] + band)
}

/// Task #134: the disjoint sum — both inputs concatenated verbatim (B's
/// vertex / edge indices offset), every curve and surface tag bit-identical.
fn concat_breps(a: &BRep, b: &BRep) -> Result<BRep, YangError> {
    let mut verts: Vec<BRepVertex> = a.vertices().to_vec();
    verts.extend_from_slice(b.vertices());
    let vo = a.vertices().len() as u32;
    let eo = a.edges().len() as u32;
    let mut edges: Vec<BRepEdge> = a.edges().to_vec();
    edges.extend(b.edges().iter().map(|e| BRepEdge {
        start: e.start + vo,
        end: e.end + vo,
        curve: e.curve,
    }));
    let mut faces: Vec<BRepFace> = a.faces().to_vec();
    faces.extend(b.faces().iter().map(|f| {
        BRepFace {
            surface: f.surface,
            outer_loop: f.outer_loop.iter().map(|&e| e + eo).collect(),
            inner_loops: f
                .inner_loops
                .iter()
                .map(|lp| lp.iter().map(|&e| e + eo).collect())
                .collect(),
            reversed: f.reversed,
        }
    }));
    BRep::new(verts, edges, faces)
}

/// Exact triangle-triangle self-intersection COUNT of a boolean output —
/// the §4.5.4 illegal-self-intersection measure the #173 detector reports
/// (`cherchi_rs::detect_improper_contacts`). The DETECT half of #195 inc-4
/// detect-then-refine. We count `improper_pairs` (genuine tri-tri
/// intersections) only, NOT the `unresolved` (degenerate) tier, so a benign
/// degeneracy never registers.
///
/// NOTE this count is NOISY as an absolute quality signal — a CORRECT output
/// can carry benign improper contacts (coplanar touches; the ~33 #173
/// false-positive cases). It is therefore used only as a RELATIVE monotone
/// measure in the acceptance gate ("did refinement reduce the count?"), never
/// as an absolute "is this output valid?" test.
fn output_improper_count(brep: &BRep) -> usize {
    cherchi_rs::detect_improper_contacts(&brep.mesh.verts, &brep.mesh.tris)
        .improper_pairs
        .len()
}

/// Boolean entry point.
///
/// Runs the paper's §4.5.4 detect-then-refine (spec
/// `yang_195_seal_neighborhood_self_overlap.md` §5h, inc-4/inc-5): pass 1 at
/// natural rim resolution; ONLY if that output self-intersects (or errors)
/// AND a rim×plane graze is geometrically present do we re-run with the
/// crossing-sampling rim boost, ACCEPTING the refined output only when it is
/// self-intersection-free. A CORRECT natural-N result is never refined, so the
/// refinement provably cannot regress a passing case — the eager-boost
/// false-positive class (R0021 / F0085 over-fire, spec §5h) is structurally
/// excluded. This replaces the eager pre-tessellation boost, which fired on
/// every geometric graze whether or not it produced a defect.
///
/// ALWAYS-ON since inc-5 (was `YANG_RIM_PLANE_GRAZE_ENABLE`). Flip measured on
/// the full 312-case corpus against the honest 252C/0W/58E/0T baseline:
/// **254C/0W/56E/0T, exactly two deltas — R0072 and R0095 ERROR→CORRECT — and
/// zero CORRECT→ERROR.** The flip is paired with the §4.4.1 rim-snap pass
/// (`stage4_boundary_curve`), which the refinement depends on: boosting the rim
/// exposes a latent Stage-4 relocation gap that rim-snap closes (the
/// `n2_junction_cluster::i1` oracle is RED with this arm alone and GREEN with
/// both). The two must stay on together.
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // Detect-then-refine. Pass 1 at natural resolution.
    let natural = boolean_once(a, b, op, backend, false);
    let probe = std::env::var_os("YANG_REFINE_PROBE").is_some();
    // CHEAP GATE FIRST: a rim×plane graze the natural resolution
    // under-samples must be present, else no refinement is possible — return
    // immediately WITHOUT the output self-intersection scan. This keeps the
    // per-op `detect_improper_contacts` cost off the common no-graze path (an
    // always-on scan pushed CORRECT large cases F0090/R0019/R0081 over the
    // assay budget — the scan must ride the graze gate). `rim_plane_graze_min_segments`
    // is self-limiting: `None` when both operands' natural N already suffice.
    let graze = rim_plane_graze_min_segments(a, b);
    if graze.is_none() {
        return natural;
    }
    // An INPUT-side non-manifold error is not a §4.5.4 self-intersection the
    // rim boost can address — non-manifoldness is topological, not a
    // resolution deficit — so refining it is a provably futile second full
    // pass (measured R0019: 90s → 188s, still ERROR). Skip it. Every OTHER
    // error is an OUTPUT-side failure (LocalRefinementRequired,
    // NonManifoldOutput, χ mismatch, …) that the refinement legitimately
    // attempts (measured R0072: LRR → Ok).
    if matches!(&natural, Err(YangError::NonManifoldInput)) {
        return natural;
    }
    // `Some(n)` = natural emitted a body with n self-intersections (n>0 ⇒
    // broken); `None` = natural was a hard error (the strongest "broken").
    let natural_selfx: Option<usize> = match &natural {
        Ok(brep) => Some(output_improper_count(brep)),
        Err(_) => None,
    };
    let natural_broken = !matches!(natural_selfx, Some(0));
    if probe {
        let nat = match (&natural, natural_selfx) {
            (Ok(_), Some(n)) => format!("Ok improper={n}"),
            (Err(e), _) => format!("Err({e:?})"),
            _ => unreachable!(),
        };
        eprintln!("[refine] op={op:?} natural={nat} broken={natural_broken} graze={graze:?}");
    }
    // Refine when the natural output is broken (the graze is already confirmed
    // present above).
    if natural_broken {
        if let Ok(refined) = boolean_once(a, b, op, backend, true) {
            let refined_improper = output_improper_count(&refined);
            // Adopt the refinement unless it is WORSE than natural:
            //  - natural was a hard error  ⇒ any emitted body is better;
            //  - natural was Ok-but-selfx  ⇒ accept when the refined body has
            //    NO MORE illegal intersections than natural (`<=`). The count
            //    is a noisy ABSOLUTE (benign coplanar contacts survive
            //    refinement, and repositioning a malignant crossing can trade
            //    it for a benign one at equal count — measured R0095: 2→2 with
            //    the downstream op fixed), so we never demand a reduction, only
            //    that refinement did not ADD illegal geometry. Refinement is
            //    the paper's §4.5.4 remedy; the tie goes to the more-sampled
            //    body. Non-regressing: refined only for already-broken ops, and
            //    only adopted when it does not increase the self-intersection
            //    count. The full-corpus assay is the P10 verdict on `<=`.
            let accept = match natural_selfx {
                None => true,
                Some(n) => refined_improper <= n,
            };
            if probe {
                eprintln!("[refine]   refined=Ok improper={refined_improper} accept={accept}");
            }
            if accept {
                return Ok(refined);
            }
        } else if probe {
            eprintln!("[refine]   refined=Err");
        }
        // Refinement did not improve on natural (or errored): keep the natural
        // result (Ok-but-selfx or its original error) — never worse.
    }
    natural
}

fn boolean_once(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
    refine_rim_plane: bool,
) -> Result<BRep, YangError> {
    // Run separator for env-gated probe streams: which boolean call a probe
    // line belongs to (multi-op corpus cases interleave several runs).
    if std::env::var_os("YANG_RUN_PROBE").is_some() {
        eprintln!(
            "[yang-run] op={op:?} a: {}v/{}f b: {}v/{}f",
            a.vertices().len(),
            a.faces().len(),
            b.vertices().len(),
            b.faces().len()
        );
    }
    // Task #134 (spec `yang_disjoint_union_passthrough` B1): a UNION whose
    // operands' conservative AABBs are strictly disjoint is the DISJOINT
    // SUM — emit the concatenated B-Rep verbatim (every curve/surface tag
    // preserved bit-for-bit). The mesh pipeline would re-emit all the
    // untouched geometry from mesh patches, degrading every full rim to a
    // LineSegment chord polyline: the output then carries NO Circle
    // vocabulary and a LATER boolean dies at the Stage-3 producer fault
    // (`chord_tol_for_curved_owner` → AmbiguousCurve{0,0}). Subtract /
    // Intersect keep the pipeline (B3 — their disjoint outputs are
    // byte-load-bearing for existing corpus verdicts).
    if op == BoolOp::Union && union_operands_strictly_disjoint(a, b) {
        return concat_breps(a, b);
    }

    // Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    // both operands at the pair-derived rim density BEFORE any Stage-0/1
    // machinery samples their meshes, so analytically-disjoint cylinder
    // pairs cannot mesh-intersect. `None` (no cylinder faces, e.g. the
    // `from_mesh` chained-output operand, or no disjoint pair demanding
    // more than each solid's own N) leaves both operands byte-identical.
    // Case-III graze guard (spec `yang_172_case_iii_graze_guard`, M5 #172
    // half b): the mirror scan — cross cylinder pairs whose surfaces
    // INTERSECT at a penetration shallower than the combined chord sagitta
    // would be MISSED by the meshes (measured C0116: unfused emission
    // whose true trims interpenetrate). Boost so the meshes must sample
    // the wedge; a genuine sub-resolution graze STOPs loudly
    // (`SubSagittaGrazeIntersection`).
    // Rim×plane graze arm (spec `yang_195_seal_neighborhood_self_overlap`
    // §5): a rim circle shallowly crossing a partner PLANE face (extent
    // below the rim's chord sagitta) is missed by the meshes; the labeling
    // then keeps the submerged region and Stage-4 relocation mints the true
    // junction beyond the plane — the producing op emits a self-intersecting
    // B-Rep (measured F0082 Extrude-11; Yang §4.5.4). Refinement eliminates
    // it, per the paper.
    //
    // #195 inc-4 (spec §5h): applied here ONLY on the refinement pass
    // (`refine_rim_plane`) driven by `boolean`'s detect-then-refine wrapper,
    // never eagerly. Pass 1 (natural resolution) always sees `None`, so a
    // shallow crossing that produces no self-intersection (the R0021/F0085
    // eager-boost false-positive class, spec §5h) is never boosted. The
    // detect-then-refine path in `boolean` is always-on since inc-5; pass 1
    // still takes the `refine_rim_plane=false` route, so a case that never
    // trips detection is byte-identical to the pre-#195 behavior.
    let rim_plane_req = if refine_rim_plane {
        rim_plane_graze_min_segments(a, b)
    } else {
        None
    };
    let req = [
        phantom_min_rim_segments(a, b),
        graze_min_rim_segments(a, b)?,
        rim_plane_req,
        // §4.3.3 Case-IV corner-phantom guard (spec
        // `yang_433_case_iv_corner_phantom.md` inc-1, GATED
        // `YANG_433_GUARD=1|on`): a B-Rep edge passing within an operand's
        // chord band of a curved face WITHOUT piercing it (both exact roots
        // outside the segment) demands the rim N that keeps the inscribed
        // mesh strictly clear of the wedge the surfaces clear.
        edge_graze_min_rim_segments(a, b),
    ]
    .into_iter()
    .flatten()
    .max();
    let boosted: Option<(BRep, BRep)> = match req {
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

    // Backtrack-spike normalization (task #146): a chained-boolean-drift operand
    // can carry an invalid, self-overlapping boundary loop — a straight edge
    // overshoots a near-tangent arc/line junction by a tiny real-scale amount,
    // then a second straight edge backtracks to the junction. Re-tessellating
    // that loop emits a zero-area triangle that survives the Cherchi
    // arrangement and trips the Stage-4 watertight gate. Merge such
    // `LineSegment` spike pairs (arc-safe, per-loop conformal) before Stage 0.
    // The fast path (no spike, the overwhelming majority) leaves both operands
    // byte-identical. See `BRep::normalized_without_backtrack_spikes`.
    let na = a.normalized_without_backtrack_spikes()?;
    let nb = b.normalized_without_backtrack_spikes()?;
    let despiked: Option<(BRep, BRep)> = if na.is_some() || nb.is_some() {
        Some((
            na.unwrap_or_else(|| a.clone()),
            nb.unwrap_or_else(|| b.clone()),
        ))
    } else {
        None
    };
    let (a, b): (&BRep, &BRep) = match &despiked {
        Some((na, nb)) => (na, nb),
        None => (a, b),
    };

    // P3a #146 increment 0 (spec `yang_146_conformal_junction_sampling.md`):
    // dev-only junction-mint measurement probe. Enumerates cross edge×face
    // pierce candidates (X-edge's two incident surfaces + Y-face's surface,
    // solved by the N-137.1 implicit-triple Newton) and reports each
    // converged pierce point with its distance to the edge's existing
    // endpoint samples — the mint-gap measurement the spec's increment 0
    // demands. Print-only; production byte-identical.
    if std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
        junction_mint_probe(a, b);
    }

    // (0) Stage 0 — §4.5.5 coplanar preprocessing (PR-YR26, M8 slice b).
    // Near-coplanar planar A×B face pairs are HANDLED: both faces snapped
    // onto one canonical shared plane, segmented by the exact 2D overlay,
    // and re-tessellated so the overlap region carries IDENTICAL meshes on
    // both solids (see `stage0::stage0_preprocess`). Unsupported residue
    // (intra-solid near pairs — the chained-output class — plus curved
    // faces in a multi-pair group and overlay failures) keeps the loud
    // typed PR-YR24 wall (`CoplanarFacesUnsupported`); multi-pair PLANAR
    // groups route through the n-ary overlay (`stage0::nary`, spec
    // `m8_plane_group_nary_overlay`).
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
        // Diagnostic kill-switch, dev-only — gated out of release (F12): in
        // release the junction is always enabled (the correct default); the
        // env var is honored only under debug_assertions.
        && (!cfg!(debug_assertions) || std::env::var_os("YANG_RIM_JUNCTION_DISABLE").is_none())
    {
        let (map_a, map_b) = rim_junction_overrides(a, b);
        if map_a.is_empty() && map_b.is_empty() {
            None
        } else {
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!("[rim-junction] overrides a={map_a:?} b={map_b:?}");
            }
            Some((
                {
                    // Rim-override volume probe. The §4b coaxial propagation
                    // makes every coaxial rim carry the UNION of all junction
                    // angles, so this count is `rims x distinct_angles` and is
                    // the dominant Stage-1 density term on revolve-heavy models
                    // (measured R0019: 644 rims x 133 angles = 85,652 points,
                    // turning a 36,060-triangle natural mesh into 207,364).
                    if std::env::var_os("YANG_RIMOV_PROBE").is_some() {
                        let pa: usize = map_a.values().map(Vec::len).sum();
                        let pb: usize = map_b.values().map(Vec::len).sum();
                        eprintln!(
                            "[rim-ov] a_rims={} a_pts={pa} b_rims={} b_pts={pb}",
                            map_a.len(),
                            map_b.len()
                        );
                    }
                    a.rebuilt_with_rim_overrides(&map_a)?
                },
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

    // P3a #146 conformal junction sampling (spec
    // `yang_146_conformal_junction_sampling.md` §4, ALWAYS-ON since
    // increment 3): mint each cross edge×face transversal pierce point ONCE
    // and insert it by identity into the owner's edge polylines AND the
    // pierced partner face's CDT, so the two operands' Stage-1 meshes share
    // the junction vertex bit-exactly (no near-dup mint downstream).
    // `YANG_JUNCTION_SAMPLING_ENABLE=off|0` disables it purely as a dev A/B
    // knob (compliance-ledger measurement, the `weld_enabled` pattern);
    // `=edge|face` select one insertion half (dev diagnostics). Unset =
    // production default = fully on.
    // SCOPE GATE mirrors the rim-junction insertion above: no Stage-0
    // interaction (the re-tessellation paths do not thread these overrides
    // — the M8 incr-15 pass-through trap) and no rim-junction rebuild (a
    // second from-topology rebuild would DROP the first rebuild's inserted
    // rim samples; overrides do not compose across rebuilds yet). A skipped
    // pair is a missed mint = status quo, never worse.
    let p3a_disabled = matches!(
        std::env::var("YANG_JUNCTION_SAMPLING_ENABLE").as_deref(),
        Ok("off") | Ok("0")
    );
    // P3b inc-4a: bit-keys of every Stage-1 minted junction point actually
    // inserted below, mapped (inc-4b) to the mint's owner-edge trim
    // provenance. Threaded into Stage 4 so the §4.3 coincident weld can
    // recognize a relocated vertex converging onto a minted junction (the
    // moved×minted arm; survivor = the mint) and the beyond-corner trim can
    // test the owner planes. Empty when no mint happened.
    let mut minted_junction_keys: std::collections::BTreeMap<[u64; 3], MintProvenance> =
        std::collections::BTreeMap::new();
    let p3a_sampled: Option<(BRep, BRep)> = if !p3a_disabled
        && stage0.is_none()
        && cyl_pairs.is_empty()
        && junction_boosted.is_none()
    {
        let mut jo = junction_stage1_overrides(a, b);
        // Diagnostic sub-modes for the gate value (dev measurement, spec §4
        // increment-2 iteration): `edge` = owner-side polyline insertion
        // only, `face` = partner-side interior insertion only. Any other
        // value = both halves (the full junction contract).
        match std::env::var("YANG_JUNCTION_SAMPLING_ENABLE").as_deref() {
            Ok("edge") => {
                jo.face_a.clear();
                jo.face_b.clear();
            }
            Ok("face") => {
                jo.edge_a.clear();
                jo.edge_b.clear();
            }
            _ => {}
        }
        if jo.is_empty() {
            None
        } else {
            if std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
                eprintln!(
                    "[p3a-wire] edge_a={} face_a={} edge_b={} face_b={} rim_a={} rim_b={}",
                    jo.edge_a.len(),
                    jo.face_a.len(),
                    jo.edge_b.len(),
                    jo.face_b.len(),
                    jo.rim_a.len(),
                    jo.rim_b.len()
                );
                // Verbose arm (`=v`): the full override payload per operand —
                // targeted edge topology (endpoint indices + coords) and the
                // exact junction points — to join a defective rebuilt mesh
                // back to the insertion that produced it.
                if std::env::var("YANG_JUNCTION_MINT_PROBE").as_deref() == Ok("v") {
                    for (tag, brep, eo, fo) in [
                        ("A", a, &jo.edge_a, &jo.face_a),
                        ("B", b, &jo.edge_b, &jo.face_b),
                    ] {
                        for (ei, pts) in eo.iter() {
                            let e = &brep.edges()[*ei as usize];
                            eprintln!(
                                "[p3a-wire-v] {tag} edge {ei} v{}→v{} {:?}→{:?} pts {pts:?}",
                                e.start,
                                e.end,
                                brep.vertices()[e.start as usize].point,
                                brep.vertices()[e.end as usize].point
                            );
                        }
                        for (fi, pts) in fo.iter() {
                            eprintln!("[p3a-wire-v] {tag} face {fi} pts {pts:?}");
                        }
                    }
                }
            }
            for pts in jo
                .edge_a
                .values()
                .chain(jo.face_a.values())
                .chain(jo.edge_b.values())
                .chain(jo.face_b.values())
                .chain(jo.rim_a.values())
                .chain(jo.rim_b.values())
            {
                for p in pts {
                    let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
                    // inc-4b: resolve the pierce-time geometric verdicts into
                    // zero-content flags for THIS op (see resolve_trim_beyond).
                    let prov = match jo.provenance.get(&key) {
                        Some(pp) => MintProvenance {
                            owner_planes: pp.owner_planes.map(|pl| MintTrimPlane {
                                n: pl.n,
                                d: pl.d,
                                trim_beyond: resolve_trim_beyond(op, pp.owner, pl.material_beyond),
                            }),
                        },
                        None => MintProvenance {
                            owner_planes: [MintTrimPlane::default(); 2],
                        },
                    };
                    minted_junction_keys.insert(key, prov);
                }
            }
            Some((
                a.rebuilt_with_all_overrides(&jo.rim_a, &jo.edge_a, &jo.face_a)?,
                b.rebuilt_with_all_overrides(&jo.rim_b, &jo.edge_b, &jo.face_b)?,
            ))
        }
    } else {
        if !p3a_disabled && std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
            eprintln!(
                "[p3a-wire] SKIP stage0={} cyl_pairs={} rim_junction={}",
                stage0.is_some(),
                !cyl_pairs.is_empty(),
                junction_boosted.is_some()
            );
        }
        None
    };
    let (a, b): (&BRep, &BRep) = match &p3a_sampled {
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
                for (tag, m, tf) in [
                    ("A", &s0.mesh_a, &s0.tri_face_a),
                    ("B", &s0.mesh_b, &s0.tri_face_b),
                ] {
                    for (i, v) in m.verts.iter().enumerate() {
                        if near(v) {
                            let q = v.as_array();
                            eprintln!(
                                "[input-vert-probe] stage0 mesh {tag} vert {i}: ({},{},{})",
                                q[0], q[1], q[2]
                            );
                            // Face attribution (F0067 LabelMismatch anchor):
                            // which owning faces' triangles reference this
                            // vertex — the two members of an input ulp pair
                            // living on DIFFERENT faces names the chain
                            // divergence site.
                            for (ti, t) in m.tris.iter().enumerate() {
                                if t.contains(&(i as u32)) {
                                    let f = tf.get(ti).copied().unwrap_or(u32::MAX);
                                    eprintln!("[input-vert-probe]   tri {ti} face {f} verts {t:?}");
                                }
                            }
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
    crate::stage5_topology::chi_audit_report("stage2-input-a", &mesh_a.tris);
    crate::stage5_topology::chi_audit_report("stage2-input-b", &mesh_b.tris);
    if crate::stage5_topology::chi_audit_enabled() && stage0.is_some() {
        crate::stage5_topology::chi_audit_report("stage1-raw-a", &a.as_mesh().tris);
        crate::stage5_topology::chi_audit_report("stage1-raw-b", &b.as_mesh().tris);
    }
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

    // (#146 inc-3b probe, print-only) Operand-mesh over-use scan: does the
    // Stage-1/Stage-0 mesh handed to the arrangement ALREADY carry an
    // asymmetric directed edge (fwd≠rev)? Distinguishes an over-use minted
    // at the downstream weld from one inherited from the operand build
    // (e.g. a gate-ON junction-insertion defect). Env-gated, no behavior
    // change.
    if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
        use std::collections::BTreeMap;
        for (tag, m) in [("A", mesh_a), ("B", mesh_b)] {
            let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
            for t in &m.tris {
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    *dir.entry((t[i], t[j])).or_insert(0) += 1;
                }
            }
            let mut any = false;
            // inc-3.5 probe fix: aggregate per CANONICAL pair before
            // comparing. The old skip-s>e iteration silently missed any
            // one-sided edge whose s<e direction never occurs (measured on
            // R0059: 3 of 5 seam edges unreported).
            let mut canon: BTreeMap<(u32, u32), (i32, i32)> = BTreeMap::new();
            for (&(s, e), &n) in &dir {
                let ent = canon.entry((s.min(e), s.max(e))).or_insert((0, 0));
                if s < e {
                    ent.0 += n;
                } else {
                    ent.1 += n;
                }
            }
            for (&(s, e), &(fwd, rev)) in &canon {
                if fwd != rev {
                    any = true;
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-input-overuse: input {tag} edge ({s},{e}) \
                         fwd={fwd} rev={rev} coords {:?} {:?}",
                        m.verts[s as usize], m.verts[e as usize]
                    );
                    // Amendment-13 inc-3.2 enrichment: name the incident
                    // triangles so the unpaired side self-localizes.
                    let brep_of = if tag == "A" { a } else { b };
                    // The tri→face lineage of the mesh actually scanned:
                    // Stage-0's rebuilt mesh carries its own map (its
                    // triangle order differs from `as_mesh()`).
                    let tri_face_of: &[u32] = match (&stage0, tag) {
                        (Some(s0), "A") => &s0.tri_face_a,
                        (Some(s0), _) => &s0.tri_face_b,
                        (None, _) => brep_of.tri_face(),
                    };
                    for (ti, t) in m.tris.iter().enumerate() {
                        if t.contains(&s) && t.contains(&e) {
                            let third = t.iter().copied().find(|&x| x != s && x != e);
                            // 2026-08-19: name the OWNING input face (Stage-1
                            // `tri_face` lineage) so a non-conformal seam
                            // between two faces' tessellations self-localizes.
                            let owner = tri_face_of.get(ti).map(|&f| {
                                (
                                    f,
                                    brep_of.faces().get(f as usize).map(|face| {
                                        crate::stage4_correct::surface_kind_name(face.surface)
                                    }),
                                )
                            });
                            eprintln!(
                                "NONMANIFOLD_SITE_PROBE i6-input-overuse:   tri {ti} {t:?} \
                                 third {:?} owner_face {owner:?}",
                                third.map(|x| m.verts[x as usize])
                            );
                        }
                    }
                    // The B-Rep edges joining the two endpoints (mesh ids of
                    // seeded B-Rep vertices ARE their B-Rep ids) and the
                    // faces whose loops carry each: a shared edge whose two
                    // sides disagree on CURVE or sampling self-localizes.
                    for (ei, be) in brep_of.edges().iter().enumerate() {
                        let joins =
                            (be.start == s && be.end == e) || (be.start == e && be.end == s);
                        if !joins {
                            continue;
                        }
                        let owners: Vec<String> = brep_of
                            .faces()
                            .iter()
                            .enumerate()
                            .filter(|(_, face)| {
                                face.outer_loop
                                    .iter()
                                    .chain(face.inner_loops.iter().flatten())
                                    .any(|&x| x as usize == ei)
                            })
                            .map(|(fi, face)| {
                                format!(
                                    "f{fi}:{}(loop {} edges)",
                                    crate::stage4_correct::surface_kind_name(face.surface),
                                    face.outer_loop.len()
                                )
                            })
                            .collect();
                        eprintln!(
                            "NONMANIFOLD_SITE_PROBE i6-input-overuse:   brep edge {ei} v{}→v{} {:?} owners [{}]",
                            be.start,
                            be.end,
                            be.curve,
                            owners.join(", ")
                        );
                    }
                    // Any OTHER triangle touching a vertex of the edge that
                    // is not an endpoint of it: a T-junction witness (a
                    // sample inserted on this edge by one face only).
                    for (ti, t) in m.tris.iter().enumerate() {
                        if (t.contains(&s) || t.contains(&e)) && !(t.contains(&s) && t.contains(&e))
                        {
                            let owner = tri_face_of.get(ti).map(|&f| {
                                (
                                    f,
                                    brep_of.faces().get(f as usize).map(|face| {
                                        crate::stage4_correct::surface_kind_name(face.surface)
                                    }),
                                )
                            });
                            eprintln!(
                                "NONMANIFOLD_SITE_PROBE i6-input-overuse:   near-tri {ti} {t:?} owner_face {owner:?}"
                            );
                        }
                    }
                }
            }
            // Small-operand topology dump: join the defective mesh back to
            // the B-Rep loops/edges that tessellated it.
            let brep = if tag == "A" { a } else { b };
            if any && brep.faces().len() <= 20 {
                for (i, v) in brep.vertices().iter().enumerate() {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-input-topo: {tag} vert {i} {:?}",
                        v.point
                    );
                }
                for (i, e) in brep.edges().iter().enumerate() {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-input-topo: {tag} edge {i} v{}→v{} {:?}",
                        e.start, e.end, e.curve
                    );
                }
                for (i, f) in brep.faces().iter().enumerate() {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-input-topo: {tag} face {i} surface {:?} \
                         outer {:?} inner {:?}",
                        f.surface, f.outer_loop, f.inner_loops
                    );
                }
            }
        }
    }

    // (#195 probe, print-only) Operand-mesh exact self-intersection scan: does
    // the Stage-1/Stage-0 mesh handed to the arrangement ALREADY carry
    // improper triangle-triangle contacts (operand self-overlap)?
    // Distinguishes a self-overlap inherited from the producing op's emission
    // from one minted in-boolean. Env-gated, no behavior change.
    if std::env::var_os("YANG_INPUT_SELFX_PROBE").is_some() {
        for (tag, m, brep) in [("A", mesh_a, a), ("B", mesh_b, b)] {
            // Double-cover edge scan (fwd = rev ≥ 2): the balanced 4-page book
            // seam the improper-contact sweep cannot see (its pairs share
            // vertices) — the exact signature of the output-side χ=3 STOP.
            {
                use std::collections::BTreeMap;
                let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
                for t in &m.tris {
                    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                        *dir.entry((t[i], t[j])).or_insert(0) += 1;
                    }
                }
                for (&(s, e), &fwd) in &dir {
                    if s < e && fwd >= 2 {
                        eprintln!(
                            "YANG_INPUT_SELFX {tag} double-cover edge ({s},{e}) fwd={fwd} \
                             rev={} v{s}={:?} v{e}={:?}",
                            dir.get(&(e, s)).copied().unwrap_or(0),
                            m.verts[s as usize],
                            m.verts[e as usize]
                        );
                    }
                }
            }
            let contacts = cherchi_rs::detect_improper_contacts(&m.verts, &m.tris);
            eprintln!(
                "YANG_INPUT_SELFX {tag}: tris={} improper={} unresolved={}",
                m.tris.len(),
                contacts.improper_pairs.len(),
                contacts.unresolved_pairs.len()
            );
            let face_of = |t: u32| -> Option<u32> {
                (brep.tri_face().len() == m.tris.len())
                    .then(|| brep.tri_face().get(t as usize).copied())
                    .flatten()
            };
            for &(ta, tb) in contacts
                .improper_pairs
                .iter()
                .chain(contacts.unresolved_pairs.iter())
                .take(16)
            {
                let surf = |t: u32| {
                    face_of(t).map(|f| (f, brep.faces().get(f as usize).map(|fa| fa.surface)))
                };
                eprintln!(
                    "YANG_INPUT_SELFX {tag} pair ({ta},{tb}) faces=({:?},{:?}) \
                     ta={:?} tb={:?}",
                    surf(ta),
                    surf(tb),
                    m.tris
                        .get(ta as usize)
                        .map(|t| t.map(|v| m.verts[v as usize])),
                    m.tris
                        .get(tb as usize)
                        .map(|t| t.map(|v| m.verts[v as usize])),
                );
            }
            // For each face involved in an improper pair, dump its B-Rep
            // boundary loops (vertex ids + coords): distinguishes a producer
            // defect (the beyond-wall point IS a B-Rep boundary vertex of the
            // previous op's output) from a Stage-1-minted interior sample.
            let mut involved: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for &(ta, tb) in contacts
                .improper_pairs
                .iter()
                .chain(contacts.unresolved_pairs.iter())
            {
                for t in [ta, tb] {
                    if let Some(f) = face_of(t) {
                        involved.insert(f);
                    }
                }
            }
            for &fi in &involved {
                let Some(f) = brep.faces().get(fi as usize) else {
                    continue;
                };
                for (li, lp) in std::iter::once(&f.outer_loop)
                    .chain(f.inner_loops.iter())
                    .enumerate()
                {
                    let pts: Vec<String> = lp
                        .iter()
                        .filter_map(|&e| brep.edges().get(e as usize))
                        .map(|edge| {
                            format!(
                                "v{}={:?}",
                                edge.start,
                                brep.vertices().get(edge.start as usize).map(|v| v.point)
                            )
                        })
                        .collect();
                    eprintln!(
                        "YANG_INPUT_SELFX {tag} face {fi} loop {li}: {}",
                        pts.join(" ")
                    );
                }
            }
        }
    }

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

    // Producer per-EDGE intersection provenance for this boolean
    // (spec `yang_s3_intersection_edge_provenance.md` inc-2, ALWAYS-ON),
    // POSITION-keyed through the weld so it survives the compaction below
    // (the `minted_junction_keys` position-key precedent). A pair whose
    // endpoints weld together carries no edge and is dropped. Empty from a
    // provenance-less producer (sidecar parity, hand-built fixtures) — the
    // historical geometric-gate behavior applies byte-identically then.
    let edge_provenance: crate::stage3_ssi::PosKeyedEdgeSet = {
        let mut set = crate::stage3_ssi::PosKeyedEdgeSet::new();
        for &(g0, g1) in &la.intersection_edges {
            let (w0, w1) = (weld[g0 as usize], weld[g1 as usize]);
            if w0 == w1 {
                continue;
            }
            let ka = crate::stage3_ssi::pos_key(la.mesh.verts[w0 as usize]);
            let kb = crate::stage3_ssi::pos_key(la.mesh.verts[w1 as usize]);
            set.insert((ka.min(kb), ka.max(kb)));
        }
        if std::env::var_os("YANG_S3_PROVENANCE_PROBE").is_some() {
            eprintln!(
                "YANG_S3_PROV install n_pairs={} (la had {})",
                set.len(),
                la.intersection_edges.len()
            );
        }
        set
    };

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
    // (I6.5, #146 inc-3a) Collapsed-wedge dedup bookkeeping: sorted welded
    // post-flip triple → the kept representative's (raw triple, welded
    // triple, `la` tri index). See the dedup arm inside the loop.
    let mut wedge_seen: std::collections::HashMap<[u32; 3], ([u32; 3], [u32; 3], usize)> =
        std::collections::HashMap::new();
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
        // (I6.5, #146 inc-3a) Collapsed-wedge dedup — spec
        // `specs/yang_146_collapsed_wedge_dedup.md` §2. When the I6 weld
        // fuses sub-weld twin junction verts (flush-operand contact residue,
        // measured 1e-18…5e-15), the hair-thin strip between them collapses
        // and two surviving sub-triangles of ONE surface strip land on the
        // same welded triple with the same winding. Drop the later copy iff
        // the pair matches the exact structural wedge signature; every other
        // coincidence falls through to the post-loop I6 `NonManifoldInput`
        // backstop unchanged (the a4 adversary contract).
        {
            let mut key = tri;
            key.sort_unstable();
            if let Some(&(raw_first, tri_first, o_first)) = wedge_seen.get(&key) {
                let src = |o: usize| -> &[(LaInputId, u32)] {
                    if la.source.is_empty() {
                        &[]
                    } else {
                        &la.source[o]
                    }
                };
                let verdict = wedge_reject_reason(
                    raw_first,
                    raw,
                    tri_first,
                    tri,
                    &weld,
                    src(o_first),
                    src(orig_t),
                    &la.surface[o_first],
                    &la.surface[orig_t],
                    tri_face_a,
                    tri_face_b,
                );
                if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                    match verdict {
                        None => eprintln!(
                            "NONMANIFOLD_SITE_PROBE i6-wedge-dedup: DROP orig_t {orig_t} raw \
                             {raw:?} (kept orig_t {o_first} raw {raw_first:?}) sources {:?}/{:?}",
                            src(o_first),
                            src(orig_t)
                        ),
                        Some(reason) => eprintln!(
                            "NONMANIFOLD_SITE_PROBE i6-wedge-dedup: REJECT({reason}) orig_t \
                             {orig_t} raw {raw:?} vs kept orig_t {o_first} raw {raw_first:?}"
                        ),
                    }
                }
                if verdict.is_none() {
                    continue;
                }
            } else {
                wedge_seen.insert(key, (raw, tri, orig_t));
            }
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
        use std::collections::HashMap;
        let mut seen: HashMap<[u32; 3], usize> = HashMap::with_capacity(compact_tris.len());
        for (ci, t) in compact_tris.iter().enumerate() {
            let mut sorted = *t;
            sorted.sort_unstable();
            if let Some(&prev_ci) = seen.get(&sorted) {
                if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-coincident-tris: verts {:?} coords {:?} {:?} {:?}",
                        sorted,
                        compact_verts[sorted[0] as usize],
                        compact_verts[sorted[1] as usize],
                        compact_verts[sorted[2] as usize]
                    );
                    for label in [prev_ci, ci] {
                        let ot = orig_tri[label];
                        eprintln!(
                            "NONMANIFOLD_SITE_PROBE i6-coincident-tris: compact {label} orig_t {ot} \
                             raw_tri {:?} source {:?} surface {:?}",
                            la.mesh.tris[ot], la.source[ot], la.surface[ot]
                        );
                    }
                    // #146 inc-3 provenance join: for each welded vertex of
                    // the coincident pair, the ORIGINAL arrangement vertex
                    // cluster that fused into it — these original indices
                    // are cherchi OUTPUT indices, joining directly against
                    // the CHERCHI_VERT_PROVENANCE pair log.
                    for &cv in &sorted {
                        let root = remap
                            .iter()
                            .position(|s| *s == Some(cv))
                            .expect("compact vert has a welded root");
                        let members: Vec<u32> = weld
                            .iter()
                            .enumerate()
                            .filter(|&(_, &r)| r == root as u32)
                            .map(|(i, _)| i as u32)
                            .collect();
                        eprintln!(
                            "NONMANIFOLD_SITE_PROBE i6-cluster: compact {cv} root {root} \
                             members(la-vert) {members:?}"
                        );
                    }
                }
                return Err(YangError::NonManifoldInput);
            }
            seen.insert(sorted, ci);
        }
    }
    // (#146 inc-3b probe, print-only) Edge-over-use provenance: scan the
    // compacted kept set for ASYMMETRIC directed edges (fwd count ≠ rev
    // count — the F0084 `s4-halfedge-pairing fwd=1 rev=2` class) and print
    // every incident triangle's raw triple / source / surface plus the two
    // endpoints' weld clusters. Localizes whether the over-use is minted
    // HERE (in-boolean, the I6.5 wedge class one simplex down) or later in
    // Stage 4. Env-gated, no behavior change.
    if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
        use std::collections::BTreeMap;
        let mut dir: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for (ci, t) in compact_tris.iter().enumerate() {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                dir.entry((t[i], t[j])).or_default().push(ci);
            }
        }
        for (&(s, e), fwd_tris) in &dir {
            if s > e {
                continue; // report each undirected edge once
            }
            let rev_tris = dir.get(&(e, s)).map(Vec::as_slice).unwrap_or(&[]);
            if fwd_tris.len() == rev_tris.len() {
                continue;
            }
            eprintln!(
                "NONMANIFOLD_SITE_PROBE i6-edge-overuse: edge ({s},{e}) fwd={} rev={} \
                 coords {:?} {:?}",
                fwd_tris.len(),
                rev_tris.len(),
                compact_verts[s as usize],
                compact_verts[e as usize]
            );
            for (dir_name, list) in [("fwd", fwd_tris.as_slice()), ("rev", rev_tris)] {
                for &ci in list {
                    let ot = orig_tri[ci];
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE i6-edge-overuse: {dir_name} compact {ci} \
                         tri {:?} orig_t {ot} raw {:?} source {:?} surface {:?}",
                        compact_tris[ci],
                        la.mesh.tris[ot],
                        if la.source.is_empty() {
                            &[][..]
                        } else {
                            &la.source[ot][..]
                        },
                        la.surface[ot]
                    );
                }
            }
            for &cv in &[s, e] {
                let root = remap
                    .iter()
                    .position(|sl| *sl == Some(cv))
                    .expect("compact vert has a welded root");
                let members: Vec<u32> = weld
                    .iter()
                    .enumerate()
                    .filter(|&(_, &r)| r == root as u32)
                    .map(|(i, _)| i as u32)
                    .collect();
                eprintln!(
                    "NONMANIFOLD_SITE_PROBE i6-edge-overuse-cluster: compact {cv} root {root} \
                     members(la-vert) {members:?}"
                );
            }
        }
    }
    let kept_submesh = Mesh::new(compact_verts, compact_tris);
    crate::stage5_topology::chi_audit_report("stage2-kept-submesh", &kept_submesh.tris);
    crate::stage5_topology::chi_audit_pinch_scan(
        "stage2-kept-submesh",
        &kept_submesh.tris,
        &kept_submesh.verts,
    );

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
        // so the unique-face rule would (wrongly) F3-tie them. The test is the
        // scale-free `tri_is_degenerate` identity (min-height/max-edge ≤
        // `DEGENERACY_IDENTITY_REL`) shared with every Stage-4/Stage-6
        // degeneracy gate — formerly the absolute `MIN_FEATURE_SIZE²` twice-
        // area floor, which at micro model scale routed HEALTHY small kept
        // triangles through this sliver branch (lowest-face-within-tolerance
        // instead of the unique-face rule).
        let degenerate = crate::tri_is_degenerate(p0, p1, p2);

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
            // Task #162 (#146/#133 off-plane emission class): a triangle lies
            // IN a `Plane` face iff ALL THREE of its vertices do. The centroid
            // ALONE is fooled by a triangle that straddles the plane
            // symmetrically — a tall cylinder-wall triangle spanning
            // z ∈ {0, 1, 2} has its centroid EXACTLY on the z=1 pocket-floor
            // plane (an "exact" hit, d=0) while no vertex is on it, so the
            // centroid rule mis-attributes the wall sliver to the floor face
            // (whose arc-edged loop defeats the `point_strictly_in_planar_face`
            // containment tie-break → `None`, undecidable). Measure the WORST
            // vertex, not the centroid, so a straddling triangle is rejected at
            // its true membership distance. This is EXACT and byte-identical for
            // a genuine on-plane triangle (its vertices are exactly on the face
            // plane → max == centroid == 0), so the all-planar fuzz corpus and
            // every clean planar hit are unaffected; it only removes the false
            // straddle hit. Stage-0 pair-plane faces keep the centroid basis
            // above (their loop vertices carry a DESIGNED band-level off-plane
            // residual from the coplanar weld — the centroid is the matching
            // distance basis there, NOT a straddle). Curved faces keep the
            // centroid (a wall/cap triangle sits `d_ε` inside the surface BY
            // CONSTRUCTION; per-vertex distance would mis-tier it).
            if let Surface::Plane { normal, d } = face.surface {
                let n = normal.as_array();
                let dist = |q: [f64; 3]| (q[0] * n[0] + q[1] * n[1] + q[2] * n[2] + d).abs();
                return Ok(dist(p0).max(dist(p1)).max(dist(p2)));
            }
            // PR-YR7: delegate to the shared `signed_distance_to_surface`
            // (Cylinder + Sphere); take `.abs()` (distance to the surface). Cone
            // still rejects loudly — the free function returns a sentinel face
            // index, which we replace with the real input `fi`.
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
                // KV6d: a STRUCTURED torus face (profile-circle rims) uses the
                // rim chord `band` (the rim AABB bound covers the outermost
                // latitude radius major+minor). KV14 Slice F-3 (code review
                // 2026-09-04): a PATCH-path torus face — a band with inner
                // loops, or a lone DISK of chords with no `Curve::Circle`
                // anywhere on the operand — carries its OWN Stage-1 bound
                // `torus_chord_bound(R, r)`, the budget the UV-CDT was seeded
                // with, exactly as Stage 4's `input_curved_chord_bound` folds
                // it in. Without this arm a Circle-free torus operand had
                // `band == None` and every one of its triangles was a
                // `FaceResolutionFailed` on the lineage-less path (the C++
                // sidecar parity oracle, mock-label fixtures, `from_mesh`);
                // production inputs resolve by provenance first and never saw
                // it. `max` with the rim band where both exist: a band must
                // cover every chain the face carries (the Slice F-3 lesson).
                Surface::Torus {
                    major_radius,
                    minor_radius,
                    ..
                } => {
                    let own = input_brep
                        .faces()
                        .get(fi)
                        .filter(|f| {
                            torus_face_takes_patch_path(
                                f,
                                input_brep.edges(),
                                major_radius,
                                minor_radius,
                            )
                        })
                        .map(|_| torus_chord_bound(major_radius, minor_radius));
                    match (band, own) {
                        (Some(de), Some(o)) => Ok(de.max(o)),
                        (Some(de), None) => Ok(de),
                        (None, Some(o)) => Ok(o),
                        (None, None) => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                    }
                }
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
    let (vertices, edges, faces, sources, face_attribution) = reconstruct_topology_stage4(
        &mut kept_submesh,
        &mut triangle_attribution,
        a,
        b,
        op,
        &minted_junction_keys,
        &edge_provenance,
    )?;

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

/// P3a #146 increment 0 (spec `yang_146_conformal_junction_sampling.md` §4):
/// dev-only measurement probe behind `YANG_JUNCTION_MINT_PROBE`.
///
/// For each cross pair (edge `e` of operand X with exactly two distinct
/// incident surfaces, face `f` of operand Y) whose padded AABBs overlap,
/// seed the exact 3-surface Newton ([`relocate_onto_implicit_triple`]) at
/// the edge chord midpoint. A converged, in-band solution `J` is a candidate
/// shared-junction pierce point; the printed `d_start`/`d_end` distances to
/// the edge's endpoint samples measure the MINT GAP (how far the nearest
/// existing Stage-1 sample sits from the true junction — for `LineSegment`
/// edges the endpoints ARE the samples). Print-only diagnostics; never
/// mutates the operands.
fn junction_mint_probe(a: &BRep, b: &BRep) {
    let aabb_of = |pts: &mut dyn Iterator<Item = Point3>| -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut any = false;
        for p in pts {
            let q = p.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
            any = true;
        }
        any.then_some((lo, hi))
    };
    let overlap = |x: ([f64; 3], [f64; 3]), y: ([f64; 3], [f64; 3]), pad: f64| -> bool {
        (0..3).all(|k| x.0[k] - pad <= y.1[k] && y.0[k] - pad <= x.1[k])
    };
    let dist = |p: Point3, q: Point3| -> f64 {
        let (p, q) = (p.as_array(), q.as_array());
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    let sides: [(&BRep, &BRep, &str); 2] = [(a, b, "A"), (b, a, "B")];
    for (x, y, tag) in sides {
        // Geometric edge → the DISTINCT surfaces of the faces whose loops
        // carry it. `LineSegment` edges use the per-loop-copy convention (one
        // directed yang edge per half-edge, kernel-v2 `to_yang.rs` m1), so
        // incidence CANNOT be keyed by edge index — group copies by their
        // canonical (unordered, bitwise) endpoint pair instead. Distinct
        // curves sharing both endpoints (an arc + its chord) collapse into
        // one group and are skipped by the two-surface filter — a
        // probe-grade loss; increment 1 keys by curve identity.
        let kb = |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
        type EdgeKey = ([u64; 3], [u64; 3]);
        let mut edge_surfs: std::collections::BTreeMap<EdgeKey, (u32, Vec<Surface>)> =
            std::collections::BTreeMap::new();
        for f in x.faces() {
            for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let e = &x.edges()[ei as usize];
                let k0 = kb(x.vertices()[e.start as usize].point);
                let k1 = kb(x.vertices()[e.end as usize].point);
                let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
                let entry = edge_surfs.entry(key).or_insert((ei, Vec::new()));
                if !entry.1.contains(&f.surface) {
                    entry.1.push(f.surface);
                }
            }
        }
        let mut n_candidates = 0usize;
        let mut n_pierce = 0usize;
        for (ei, surfs) in edge_surfs.values() {
            let ei = *ei;
            let [s1, s2] = surfs.as_slice() else {
                continue; // border/defective incidence — not a 2-surface edge
            };
            let e = &x.edges()[ei as usize];
            let p0 = x.vertices()[e.start as usize].point;
            let p1 = x.vertices()[e.end as usize].point;
            let chord = dist(p0, p1);
            if chord == 0.0 {
                continue; // closed-curve edge (full circle): midpoint seed is
                          // meaningless — increment 1 will sample the curve.
            }
            let Some(e_box) = aabb_of(&mut [p0, p1].into_iter()) else {
                continue;
            };
            let seed = Point3::new(
                (p0.x() + p1.x()) / 2.0,
                (p0.y() + p1.y()) / 2.0,
                (p0.z() + p1.z()) / 2.0,
            );
            for (fi, f) in y.faces().iter().enumerate() {
                let mut f_pts = f
                    .outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .flat_map(|&fei| {
                        let fe = &y.edges()[fei as usize];
                        [
                            y.vertices()[fe.start as usize].point,
                            y.vertices()[fe.end as usize].point,
                        ]
                    });
                let Some(f_box) = aabb_of(&mut f_pts) else {
                    continue;
                };
                // Pad by the edge chord: covers arc bulge + chord sagitta at
                // probe precision (increment 1 replaces this with the exact
                // curve bound).
                if !overlap(e_box, f_box, chord) {
                    continue;
                }
                n_candidates += 1;
                let Some(j) = relocate_onto_implicit_triple(seed, *s1, *s2, f.surface) else {
                    continue;
                };
                // Reject far-off convergence: J must stay within the padded
                // edge box (a Newton walk to a distant root is not this
                // junction).
                let ja = j.as_array();
                if !(0..3).all(|k| e_box.0[k] - chord <= ja[k] && ja[k] <= e_box.1[k] + chord) {
                    continue;
                }
                n_pierce += 1;
                let fv = |s: Surface| {
                    surface_value_and_normal(s, ja)
                        .map(|(v, _)| v)
                        .unwrap_or(f64::NAN)
                };
                eprintln!(
                    "YANG_JUNCTION_MINT {tag}-edge={ei} vs-face={fi} \
                     J=({:.9},{:.9},{:.9}) d_start={:.3e} d_end={:.3e} chord={:.3e} \
                     F=({:.2e},{:.2e},{:.2e})",
                    ja[0],
                    ja[1],
                    ja[2],
                    dist(j, p0),
                    dist(j, p1),
                    chord,
                    fv(*s1),
                    fv(*s2),
                    fv(f.surface),
                );
            }
        }
        eprintln!(
            "YANG_JUNCTION_MINT summary side={tag} candidates={n_candidates} pierce={n_pierce}"
        );
    }
}

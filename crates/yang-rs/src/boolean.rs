//! The `boolean()` driver — PR-YR3 vertex provenance, PR-YR4 triangle
//! attribution, Stage-0 coplanar scan glue, KV15 near-weld, phantom
//! rim N, rim-junction overrides (extracted verbatim from lib.rs —
//! spec `specs/yang_rs_lib_decomposition.md`, increment 9).

#[allow(clippy::wildcard_imports)]
use crate::*;

mod coplanar_scan;
mod predicates;
mod provenance;
mod rim_junction;
pub(crate) use coplanar_scan::*;
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

pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
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

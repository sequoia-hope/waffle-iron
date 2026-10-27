#[allow(clippy::wildcard_imports)]
use super::*;

/// PR-YR5 internal: grouped patch of same-attribution triangles.
pub(crate) struct Patch {
    pub(crate) attribution: TriangleAttribution,
    pub(crate) tri_indices: Vec<u32>,
}

/// PR-YR5 helper: per-triangle neighbor list via canonical-edge
/// BTreeMap (deterministic insertion + iteration order).
pub(crate) fn triangle_adjacency(mesh: &Mesh) -> Vec<Vec<u32>> {
    use std::collections::BTreeMap;
    let mut edge_to_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            edge_to_tris.entry(key).or_default().push(t as u32);
        }
    }
    let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); mesh.tris.len()];
    for sharing in edge_to_tris.values() {
        for &t1 in sharing {
            for &t2 in sharing {
                if t1 != t2 && !neighbors[t1 as usize].contains(&t2) {
                    neighbors[t1 as usize].push(t2);
                }
            }
        }
    }
    neighbors
}

/// PR-YR5 helper: BFS flood-fill same-attribution triangles into
/// patches. Skip None-attributed triangles. Deterministic seed order:
/// lowest unvisited tri index first.
pub(crate) fn flood_fill_patches(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    adjacency: &[Vec<u32>],
) -> Vec<Patch> {
    flood_fill_patches_with_side_labels(mesh, attribution, None, adjacency)
}

/// F1-I3 (`specs/yang_442_classify_after_update.md` §4–§5; Yang §4.4.2,
/// `refs/text/yang2025_hybrid_boolean.txt:572-578`): BFS flood-fill that keys
/// patches by `(attribution, side_label)` when `side_labels` is `Some` (Stage 4
/// on the full arrangement, keeping `A-in` and `A-out` separate across every
/// intersection curve via `cherchi_patch` / `inside` / `surface`) and by
/// `attribution` alone when `None` (post-keep Phase A / B-Rep emission).
pub(crate) fn flood_fill_patches_with_side_labels(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    side_labels: Option<&TriangleSideLabelMap>,
    adjacency: &[Vec<u32>],
) -> Vec<Patch> {
    if let Some(sl) = side_labels {
        debug_assert_eq!(
            sl.len(),
            mesh.tris.len(),
            "flood_fill_patches_with_side_labels: side_labels ({}) != mesh.tris ({})",
            sl.len(),
            mesh.tris.len(),
        );
    }
    use std::collections::VecDeque;
    let mut visited = vec![false; mesh.tris.len()];
    let mut patches: Vec<Patch> = Vec::new();
    for seed in 0..mesh.tris.len() as u32 {
        if visited[seed as usize] {
            continue;
        }
        let Some(seed_attr) = attribution.lookup(seed) else {
            visited[seed as usize] = true;
            continue;
        };
        let seed_sl = side_labels.map(|sl| sl.lookup(seed));
        let mut queue: VecDeque<u32> = VecDeque::from([seed]);
        let mut tri_indices: Vec<u32> = Vec::new();
        while let Some(t) = queue.pop_front() {
            if visited[t as usize] {
                continue;
            }
            let Some(t_attr) = attribution.lookup(t) else {
                continue;
            };
            if t_attr != seed_attr {
                continue;
            }
            if let Some(sl) = side_labels {
                if Some(sl.lookup(t)) != seed_sl {
                    continue;
                }
            }
            visited[t as usize] = true;
            tri_indices.push(t);
            for &n in &adjacency[t as usize] {
                if !visited[n as usize] {
                    queue.push_back(n);
                }
            }
        }
        patches.push(Patch {
            attribution: seed_attr,
            tri_indices,
        });
    }
    patches
}

/// PR-YR5c helper: recover ALL directed boundary cycles of a patch.
/// Boundary edges = edges in exactly one patch triangle (canonical
/// (min, max) test). Walk each cycle from the lowest remaining
/// start-vertex; follow start→end chain via `BTreeMap` (deterministic).
///
/// A simple face yields 1 cycle; an annulus (holed face) yields 2 (the
/// outer boundary + one hole); etc. Classification of which cycle is
/// outer vs inner happens in `reconstruct_topology`.
///
/// Returns `Err(NonManifoldOutput)` on dead-end or T-junction (a genuine
/// non-manifold patch).
pub(crate) fn patch_boundary_cycle(
    patch: &Patch,
    mesh: &Mesh,
) -> Result<Vec<Vec<(u32, u32)>>, YangError> {
    use std::collections::{BTreeMap, HashSet};

    let patch_set: HashSet<u32> = patch.tri_indices.iter().copied().collect();

    // Precompute edge → tris-in-patch count for O(T) total cost.
    //
    // Spec yang_stage6_sliver_topology §4A (walk robustness): EXCLUDE the
    // patch's FOLD slivers (`patch_fold_slivers`) from BOTH the edge-count
    // preamble and the directed-boundary collection. A fold sliver is a
    // zero-area triangle whose sign-of-zero winding duplicates a real
    // triangle's directed edge (the measured F0016 chord fold); its spurious
    // directed edges would unbalance the walk into a false `NonManifoldOutput`.
    // The fold slivers keep their attribution and stay in the output mesh; they
    // simply carry no boundary of their own. NON-fold degenerate slivers (e.g.
    // a femto-twin membrane welding two coincident vertices, whose edges all
    // pair anti-parallel with real neighbours) are KEPT — excluding them would
    // promote a legitimately-interior real edge to a false boundary and diverge
    // from the reference arrangement. A patch that is ALL fold slivers derives
    // an empty boundary here → the caller's empty-cycles guard raises the loud
    // `NonManifoldOutput` (S5), never a silent degenerate face.
    // §4.5 T5: a walk STOP names this patch's input face.
    let _scope = crate::boolean::failures::nonmanifold_scope(
        patch.attribution.input,
        patch.attribution.face,
    );
    let excluded_slivers = patch_fold_slivers(patch, mesh);
    let mut patch_edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            *patch_edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Collect directed boundary edges in triangle CCW order (fold slivers
    // excluded — see §4A note above). Also record each directed boundary
    // edge's OWNING triangle and the patch's undirected edge→triangle
    // adjacency — the figure-eight wedge walk below needs both.
    let mut directed_boundary: Vec<(u32, u32)> = Vec::new();
    let mut dir_tri: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut edge_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            edge_tris.entry(key).or_default().push(t);
            if patch_edge_count.get(&key).copied().unwrap_or(0) == 1 {
                directed_boundary.push((va, vb));
                dir_tri.insert((va, vb), t);
            }
        }
    }
    let _ = patch_set; // patch_set was kept for readability; not needed after precompute

    if directed_boundary.is_empty() {
        return Ok(Vec::new());
    }

    // Build start → ends adjacency (sorted ascending for determinism)
    let mut by_start: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &(s, e) in &directed_boundary {
        by_start.entry(s).or_default().push(e);
    }
    for ends in by_start.values_mut() {
        ends.sort_unstable();
    }

    let mut cycles: Vec<Vec<(u32, u32)>> = Vec::new();

    // Figure-eight wedge walk (spec `yang_tangency_pinch_split.md` I4, the
    // KV9-F1 union follow-up): at a boundary vertex with SEVERAL outgoing
    // boundary edges (a patch pinched at a mesh-manifold vertex — e.g. the
    // tangency point of C0058's union, where the patch touches the vertex
    // in two opposite wedges), naive lowest-first chaining can join the
    // two lobes into one self-crossing cycle whose Newell cancels. The
    // wedge-correct continuation of an incoming boundary edge is found by
    // rotating through the patch's triangle fan at `current`, starting at
    // the incoming edge's triangle and crossing interior edges, until the
    // wedge's far boundary edge appears. Engaged ONLY at ambiguous vertices
    // (out-degree > 1) — everywhere else the walk is byte-identical.
    let wedge_continuation = |current: u32, prev: u32| -> Result<u32, YangError> {
        let mut t = *dir_tri.get(&(prev, current)).ok_or_else(|| {
            non_manifold_at(
                "s6-wedge-walk-no-owner",
                format_args!("incoming boundary edge ({prev}, {current}) has no owning triangle"),
            )
        })?;
        let mut via = prev;
        let mut hops = 0usize;
        loop {
            hops += 1;
            if hops > patch.tri_indices.len() + 1 {
                return Err(non_manifold_at(
                    "s6-wedge-walk-diverged",
                    format_args!("vertex {current} wedge walk did not terminate"),
                ));
            }
            // The triangle's OTHER current-incident edge (current, x).
            let tri = &mesh.tris[t as usize];
            let Some(&x) = tri.iter().find(|&&u| u != current && u != via) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-degenerate",
                    format_args!("triangle {t} degenerate at vertex {current}"),
                ));
            };
            let key = if current < x {
                (current, x)
            } else {
                (x, current)
            };
            if patch_edge_count.get(&key).copied().unwrap_or(0) == 1 {
                // The wedge's far boundary edge — the continuation.
                return Ok(x);
            }
            // Interior edge: cross to the wedge's next triangle.
            let Some(pair) = edge_tris.get(&key) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-missing-edge",
                    format_args!("edge ({current}, {x}) has no adjacency"),
                ));
            };
            let Some(&other) = pair.iter().find(|&&tj| tj != t) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-open-fan",
                    format_args!("interior edge ({current}, {x}) has one triangle"),
                ));
            };
            t = other;
            via = x;
        }
    };

    // PRIMARY (#169 P3b inc-4a, the R0061 4-strand crossing): extract every
    // cycle as an ORBIT of the wedge-consistent successor map — pair EVERY
    // directed boundary edge (u,v) with its unique continuation (v,w) (the
    // sole outgoing edge at v when v is unambiguous, else the wedge-consistent
    // one), require the map to be a bijection, and take its orbits. No start
    // heuristic; closure is at the EDGE level, so a crossing vertex is
    // traversed once per strand, each along its own wedge. This fixes the
    // legacy chain walk's two crossing defects: a cycle STARTING at a crossing
    // picked its first edge without wedge pairing, and every cycle CLOSED on
    // first return to its start VERTEX even when the arrival strand's wedge
    // continuation was a different lobe — both stitch lobes wrongly at a
    // 4-strand crossing (two section curves crossing at a minted pierce
    // corner), leaving a later cycle's wedge continuation already consumed.
    //
    // FALLBACK: the fan rotation inside `wedge_continuation` is only reliable
    // where every interior edge of the fan has exactly TWO patch triangles. At
    // a tangency generator (the KV9-F1 Steinmetz pair) FOUR sheets share the
    // tangency edge and all four are mutually tangent (first-order dihedral
    // sorting degenerates), so the rotation can emerge in the wrong sheet and
    // the successor map fails to resolve. Until a curvature-aware radial sort
    // exists, an unresolvable patch falls back to the legacy consumption walk
    // (which the KV9-F1 volume oracles validate on those fixtures); a patch
    // that fails BOTH paths keeps the legacy loud error taxonomy.
    let orbit_attempt = || -> Result<Vec<Vec<(u32, u32)>>, YangError> {
        use std::collections::BTreeSet;
        let mut succ: BTreeMap<(u32, u32), (u32, u32)> = BTreeMap::new();
        let mut claimed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for &(u, v) in &directed_boundary {
            let outs = by_start.get(&v).map(|o| o.as_slice()).unwrap_or(&[]);
            let w = match outs {
                [] => {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                        eprintln!("[wedge-dump] deadend incoming=({u},{v}) no outgoing");
                        for &(s, e) in &directed_boundary {
                            if s == v || e == v {
                                eprintln!(
                                    "[wedge-dump] dir boundary ({s},{e}) tri {:?}",
                                    dir_tri.get(&(s, e))
                                );
                            }
                        }
                        for &t in &patch.tri_indices {
                            let tri = &mesh.tris[t as usize];
                            if tri.contains(&v) {
                                eprintln!(
                                    "[wedge-dump] patch tri {t} = {tri:?} sliver={} coords {:?}",
                                    excluded_slivers.contains(&t),
                                    tri.iter()
                                        .map(|&vv| mesh.verts[vv as usize])
                                        .collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                    return Err(non_manifold_at(
                        "s6-boundary-walk-deadend",
                        format_args!("vertex {v} has incoming boundary but no outgoing"),
                    ));
                }
                [only] => *only,
                _ => {
                    // Ambiguous crossing: take the wedge-consistent edge.
                    let cont = wedge_continuation(v, u)?;
                    if !outs.contains(&cont) {
                        // EXPERIMENTAL dump (task #121): full local state at
                        // the failed crossing, env-gated with the site probe.
                        if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                            eprintln!("[wedge-dump] incoming=({u},{v}) cont={cont} outs={outs:?}");
                            for &(s, e) in &directed_boundary {
                                if s == v || e == v || s == cont || e == cont {
                                    eprintln!(
                                        "[wedge-dump] dir boundary ({s},{e}) tri {:?}",
                                        dir_tri.get(&(s, e))
                                    );
                                }
                            }
                            for &t in &patch.tri_indices {
                                let tri = &mesh.tris[t as usize];
                                if tri.contains(&v) {
                                    eprintln!(
                                        "[wedge-dump] patch tri {t} = {tri:?} sliver={} coords {:?}",
                                        excluded_slivers.contains(&t),
                                        tri.iter()
                                            .map(|&vv| mesh.verts[vv as usize])
                                            .collect::<Vec<_>>()
                                    );
                                }
                            }
                        }
                        return Err(non_manifold_at(
                            "s6-wedge-walk-not-outgoing",
                            format_args!(
                                "vertex {v}: wedge continuation {cont} is not an \
                                 available outgoing boundary edge"
                            ),
                        ));
                    }
                    cont
                }
            };
            succ.insert((u, v), (v, w));
            *claimed.entry((v, w)).or_insert(0) += 1;
        }
        // The successor map must be a bijection: every directed boundary edge
        // is claimed as a continuation exactly once (counts sum to the edge
        // count, so all-once ⇔ every edge claimed). Two strands claiming one
        // outgoing edge is an unresolvable pairing.
        for (&(s, e), &n) in &claimed {
            if n != 1 {
                return Err(non_manifold_at(
                    "s6-wedge-succ-collision",
                    format_args!("edge ({s}, {e}) claimed by {n} strands"),
                ));
            }
        }
        // Orbits partition the boundary; extract each starting at its lowest
        // unvisited edge (deterministic; for a simple boundary this matches
        // the old lowest-start-vertex rotation).
        let mut ordered: Vec<(u32, u32)> = directed_boundary.clone();
        ordered.sort_unstable();
        let mut visited: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut orbit_cycles: Vec<Vec<(u32, u32)>> = Vec::new();
        for &e0 in &ordered {
            if visited.contains(&e0) {
                continue;
            }
            let mut cycle: Vec<(u32, u32)> = Vec::new();
            let mut e = e0;
            loop {
                visited.insert(e);
                cycle.push(e);
                e = succ[&e];
                if e == e0 {
                    break;
                }
            }
            orbit_cycles.push(cycle);
        }
        Ok(orbit_cycles)
    };
    match orbit_attempt() {
        Ok(orbit_cycles) => return Ok(orbit_cycles),
        Err(orbit_err) => {
            if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                eprintln!("[wedge-orbit] unresolvable, legacy fallback: {orbit_err:?}");
            }
        }
    }

    // LEGACY consumption walk (the pre-inc-4a algorithm, byte-identical):
    // while any start vertex still has an outgoing edge, begin a new cycle at
    // the LOWEST such start vertex and chain-walk it, consuming edges as we
    // go, closing on first return to the start vertex.
    let mut remaining = directed_boundary.len();
    while let Some((&start, _)) = by_start.iter().find(|(_, ends)| !ends.is_empty()) {
        let budget = remaining;
        let mut current = start;
        let mut prev: Option<u32> = None;
        let mut cycle: Vec<(u32, u32)> = Vec::new();
        loop {
            let next = {
                let next_vec = by_start.get_mut(&current).ok_or_else(|| {
                    non_manifold_at(
                        "s6-boundary-walk-no-start",
                        format_args!("vertex {current}"),
                    )
                })?;
                if next_vec.is_empty() {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    return Err(non_manifold_at(
                        "s6-boundary-walk-deadend",
                        format_args!("vertex {current} cycle so far {}", cycle.len()),
                    ));
                }
                if next_vec.len() == 1 {
                    next_vec.remove(0)
                } else if let Some(p) = prev {
                    // Ambiguous crossing: take the wedge-consistent edge.
                    let cont = wedge_continuation(current, p)?;
                    let Some(pos) = next_vec.iter().position(|&e| e == cont) else {
                        return Err(non_manifold_at(
                            "s6-wedge-walk-not-outgoing",
                            format_args!(
                                "vertex {current}: wedge continuation {cont} is not an \
                                 available outgoing boundary edge"
                            ),
                        ));
                    };
                    next_vec.remove(pos)
                } else {
                    // Cycle START at a crossing: no incoming edge yet — take
                    // the lowest (deterministic).
                    next_vec.remove(0)
                }
            };
            cycle.push((current, next));
            remaining -= 1;
            prev = Some(current);
            current = next;
            if current == start {
                break;
            }
            // Per-cycle safety: a single cycle cannot be longer than the
            // edges that remained when it started (else the walk escaped).
            if cycle.len() > budget {
                return Err(non_manifold_at(
                    "s6-boundary-walk-escaped",
                    format_args!("start {start} budget {budget}"),
                ));
            }
        }
        cycles.push(cycle);
    }

    Ok(cycles)
}

/// A mesh triangle is a degenerate (zero-area) sliver when it is numerically
/// collinear RELATIVE TO ITS OWN EXTENT — `tri_is_degenerate`, the scale-free
/// `DEGENERACY_IDENTITY_REL` identity shared with the Stage-4 degeneracy
/// gates and the attribution's degenerate branch (one metric, one
/// definition). Formerly the absolute `MIN_FEATURE_SIZE²` twice-area floor,
/// which at micro model scale classified healthy small triangles as slivers.
/// The exact arrangement keeps genuine slivers along shared collinear solid
/// edges for watertightness; spec `yang_stage6_sliver_topology` §4A excludes
/// them from boundary derivation.
pub(crate) fn triangle_is_degenerate(mesh: &Mesh, t: u32) -> bool {
    let tri = &mesh.tris[t as usize];
    crate::tri_is_degenerate(
        mesh.verts[tri[0] as usize].as_array(),
        mesh.verts[tri[1] as usize].as_array(),
        mesh.verts[tri[2] as usize].as_array(),
    )
}

/// The patch's FOLD slivers — the degenerate zero-area triangles that spec
/// `yang_stage6_sliver_topology` §4A excludes from boundary derivation.
///
/// A fold sliver has at least one DIRECTED edge `(a→b)` that coincides,
/// SAME-direction, with a directed edge of another triangle in the patch
/// (directed multiplicity ≥ 2). That is the measured F0016 signature: a
/// zero-area shim whose sign-of-zero winding duplicates the real triangle's
/// chord edge, unbalancing the boundary walk into a false `NonManifoldOutput`.
///
/// A degenerate sliver whose edges instead pair ANTI-parallel with their
/// neighbours — e.g. a femto-twin membrane welding two coincident vertices,
/// where every directed edge is unique — is NOT a fold. Such a sliver carries a
/// legitimate (if zero-length) boundary; excluding it would drop a real
/// neighbour edge from interior to boundary and diverge from the reference
/// arrangement (curved / twin parity). So it is KEPT.
pub(crate) fn patch_fold_slivers(patch: &Patch, mesh: &Mesh) -> std::collections::HashSet<u32> {
    use std::collections::{HashMap, HashSet};
    // Directed edge multiplicity over ALL patch triangles (real + degenerate).
    let mut dir_count: HashMap<(u32, u32), u32> = HashMap::new();
    for &t in &patch.tri_indices {
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir_count.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut folds: HashSet<u32> = HashSet::new();
    for &t in &patch.tri_indices {
        if !triangle_is_degenerate(mesh, t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        let is_fold = [(0usize, 1usize), (1, 2), (2, 0)]
            .iter()
            .any(|&(i, j)| dir_count.get(&(tri[i], tri[j])).copied().unwrap_or(0) >= 2);
        if is_fold {
            folds.insert(t);
        }
    }
    folds
}

/// Exact 3D on-open-segment test for the Stage-6 loop T-subdivision (spec
/// `yang_stage6_sliver_topology` §4B). Returns `Some(t_num)` — the exact
/// numerator of the segment parameter `t = ((v−a)·(b−a)) / |b−a|²` — when `v`
/// lies STRICTLY between `a` and `b` (exactly collinear AND `0 < t < 1`);
/// `None` otherwise. All candidate vertices on one edge share the `|b−a|²`
/// denominator, so `t_num` alone orders them. Pure rational (`f64 → RBig` is
/// lossless); no tolerance — a vertex only ULP-near the segment does NOT split
/// it (that residue class stays loud downstream, spec §5 S6).
pub(crate) fn on_open_segment_param(
    a: [f64; 3],
    b: [f64; 3],
    v: [f64; 3],
) -> Option<dashu::rational::RBig> {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    let r = |x: f64| rat(x).ok();
    let (ax, ay, az) = (r(a[0])?, r(a[1])?, r(a[2])?);
    let (bx, by, bz) = (r(b[0])?, r(b[1])?, r(b[2])?);
    let (vx, vy, vz) = (r(v[0])?, r(v[1])?, r(v[2])?);
    let (abx, aby, abz) = (&bx - &ax, &by - &ay, &bz - &az);
    let (dax, day, daz) = (&vx - &ax, &vy - &ay, &vz - &az);
    // Exactly collinear: cross(ab, da) == 0 in all three components.
    let cx = &aby * &daz - &abz * &day;
    let cy = &abz * &dax - &abx * &daz;
    let cz = &abx * &day - &aby * &dax;
    if cx != RBig::ZERO || cy != RBig::ZERO || cz != RBig::ZERO {
        return None;
    }
    let t_num = &dax * &abx + &day * &aby + &daz * &abz;
    let len2 = &abx * &abx + &aby * &aby + &abz * &abz;
    // Strict betweenness: 0 < t_num < len2 (len2 > 0 for a real edge; a
    // degenerate zero-length edge yields len2 == 0 and no split).
    if t_num > RBig::ZERO && t_num < len2 {
        Some(t_num)
    } else {
        None
    }
}

/// Stage-6 loop T-subdivision (spec `yang_stage6_sliver_topology` §4B). After
/// §4A excludes degenerate slivers from boundary derivation, a face whose patch
/// carried a whole shared solid edge as ONE chord `(a,b)` must split that chord
/// at every output vertex that (i) lies STRICTLY on segment `a–b` (exact
/// rational collinearity + betweenness) AND (ii) is used by SOME OTHER output
/// loop. The other side of the shared edge subdivides it at those same
/// vertices, so after the split every segment of the solid edge is used by
/// exactly two directed loop edges — the 2-manifold seam kernel-v2's
/// edge-pairing check demands. Self-pairs within one weakly-simple loop are
/// legitimate (matching the existing kernel-v2 self-pair handling).
///
/// Returns per-`info` subdivided cycles (same outer shape as `info.cycles`).
/// Determinism: split vertices ordered by exact segment parameter, ties by
/// vertex index. A no-op (byte-identical cycles) when no loop edge has an
/// on-segment foreign vertex (spec §5 S3), so non-sliver output is unaffected.
pub(crate) fn subdivide_loops_at_shared_vertices(
    infos: &[PatchInfo],
    mesh: &Mesh,
) -> Vec<Vec<Vec<(u32, u32)>>> {
    use dashu::rational::RBig;
    use std::collections::BTreeMap;

    // (1) Assign a global loop id to every (info, cycle); record which loop ids
    // use each vertex. A vertex used repeatedly within ONE loop counts that
    // loop once (dedup) so the "used by some OTHER loop" test is exact.
    let mut next_loop = 0usize;
    let mut cycle_loop_ids: Vec<Vec<usize>> = Vec::with_capacity(infos.len());
    let mut vertex_loops: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for info in infos {
        let mut ids = Vec::with_capacity(info.cycles.len());
        for cycle in &info.cycles {
            for &(s, _e) in cycle {
                vertex_loops.entry(s).or_default().push(next_loop);
            }
            ids.push(next_loop);
            next_loop += 1;
        }
        cycle_loop_ids.push(ids);
    }
    for ids in vertex_loops.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }

    // (2) Split each loop edge at its foreign on-segment vertices — but ONLY
    // for patches that had a degenerate sliver excluded (spec §2/§4B). A patch
    // with no excluded sliver keeps byte-identical loops: the measured
    // un-subdivided chord lives on the sliver-bearing side, and subdividing a
    // benign T-junction on a clean curved/planar patch would diverge from the
    // C++ reference arrangement (which does not carry that vertex on the edge),
    // breaking reference parity. The vertex-loop map above still spans ALL
    // loops, so the sliver side splits at the foreign (fine-side) vertices.
    // Diagnosis probe (read-only, env-gated): report every loop edge with a
    // foreign vertex that is ULP-NEAR the open segment (f64 perpendicular
    // distance < 1e-9) but NOT exactly collinear — the spec §5 S6 residue —
    // plus whether the owning patch passes the fold-sliver gate at all.
    if std::env::var_os("YANG_S6_SPLIT_PROBE").is_some() {
        for (ii, info) in infos.iter().enumerate() {
            for (ci, cycle) in info.cycles.iter().enumerate() {
                let lid = cycle_loop_ids[ii][ci];
                for &(s, e) in cycle {
                    let pa = mesh.verts[s as usize].as_array();
                    let pb = mesh.verts[e as usize].as_array();
                    let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                    if len2 == 0.0 {
                        continue;
                    }
                    for (&v, luse) in &vertex_loops {
                        if v == s || v == e || !luse.iter().any(|&l| l != lid) {
                            continue;
                        }
                        let pv = mesh.verts[v as usize].as_array();
                        let av = [pv[0] - pa[0], pv[1] - pa[1], pv[2] - pa[2]];
                        let t = (av[0] * ab[0] + av[1] * ab[1] + av[2] * ab[2]) / len2;
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
                        let d = [pv[0] - proj[0], pv[1] - proj[1], pv[2] - proj[2]];
                        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                        if dist < 1e-9 && on_open_segment_param(pa, pb, pv).is_none() {
                            eprintln!(
                                "[s6-split-probe] info {ii} (face_idx {}) cycle {ci} edge ({s},{e}) \
                                 near-vertex {v} dist={dist:e} t={t:.6} gate(had_fold_sliver)={}",
                                info.face_idx, info.had_fold_sliver
                            );
                        }
                    }
                }
            }
        }
    }
    // Companion probe: dump every mesh triangle touching a comma-separated
    // vertex list (env `YANG_S6_VERT_PROBE=842,843,845`) with its area and
    // which info/cycle (if any) carries each directed edge — to see whether
    // the mesh is conformal at a suspect femto-twin site.
    if let Some(list) = std::env::var_os("YANG_S6_VERT_PROBE") {
        let want: std::collections::BTreeSet<u32> = list
            .to_string_lossy()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        for (t, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|v| want.contains(v)) {
                continue;
            }
            let deg = triangle_is_degenerate(mesh, t as u32);
            eprintln!(
                "[s6-vert-probe] tri {t} ({},{},{}) degenerate={deg}",
                tri[0], tri[1], tri[2]
            );
        }
        for (ii, info) in infos.iter().enumerate() {
            for (ci, cycle) in info.cycles.iter().enumerate() {
                for &(s, e) in cycle {
                    if want.contains(&s) || want.contains(&e) {
                        eprintln!(
                            "[s6-vert-probe] info {ii} face_idx {} cycle {ci} edge {s}->{e}",
                            info.face_idx
                        );
                    }
                }
            }
        }
    }
    // Spec amendment 1 (S7): undirected segment-use census over ALL loops.
    // A loop edge with use-count 1 is CERTAIN to fail kernel-v2's manifold
    // edge pairing — the S7 split can therefore never alter a passing
    // output (every valid output uses each undirected segment exactly
    // twice), preserving reference parity structurally rather than via the
    // fold-sliver scope.
    let mut seg_use: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for info in infos {
        for cycle in &info.cycles {
            for &(s, e) in cycle {
                *seg_use.entry((s.min(e), s.max(e))).or_default() += 1;
            }
        }
    }

    let mut out: Vec<Vec<Vec<(u32, u32)>>> = Vec::with_capacity(infos.len());
    for (ii, info) in infos.iter().enumerate() {
        if !info.had_fold_sliver {
            // S7 (spec `yang_stage6_sliver_topology` amendment 1): the
            // certainly-fatal chord repair. Split a use-count-1 loop edge
            // (a,b) at a foreign vertex v strictly inside it (0<t<1, within
            // TAU_WORK of the open segment — the spec §4 "band for the
            // last-ulp case"; F0079's site is f64-dist 0.0 but sub-ULP off
            // the exact segment) when BOTH complementary sub-segments (a,v)
            // and (v,b) are walked by some loop. Any currently-valid output
            // has use == 2 everywhere → byte-identical (S1/S3).
            let mut info_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(info.cycles.len());
            for (ci, cycle) in info.cycles.iter().enumerate() {
                let lid = cycle_loop_ids[ii][ci];
                let mut new_cycle: Vec<(u32, u32)> = Vec::with_capacity(cycle.len());
                let mut inserted: std::collections::BTreeSet<(u32, u32)> =
                    std::collections::BTreeSet::new();
                for &(s, e) in cycle {
                    if seg_use.get(&(s.min(e), s.max(e))).copied() != Some(1) {
                        new_cycle.push((s, e));
                        continue;
                    }
                    let pa = mesh.verts[s as usize].as_array();
                    let pb = mesh.verts[e as usize].as_array();
                    let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                    if len2 == 0.0 {
                        new_cycle.push((s, e));
                        continue;
                    }
                    let mut splits: Vec<(f64, u32)> = Vec::new();
                    for (&v, luse) in &vertex_loops {
                        if v == s || v == e || !luse.iter().any(|&l| l != lid) {
                            continue;
                        }
                        if seg_use.get(&(s.min(v), s.max(v))).copied().unwrap_or(0) == 0
                            || seg_use.get(&(e.min(v), e.max(v))).copied().unwrap_or(0) == 0
                        {
                            continue;
                        }
                        let pv = mesh.verts[v as usize].as_array();
                        let av = [pv[0] - pa[0], pv[1] - pa[1], pv[2] - pa[2]];
                        let t = (av[0] * ab[0] + av[1] * ab[1] + av[2] * ab[2]) / len2;
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
                        let d = [pv[0] - proj[0], pv[1] - proj[1], pv[2] - proj[2]];
                        let dist2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                        if dist2 <= cad_primitives::TAU_WORK * cad_primitives::TAU_WORK {
                            splits.push((t, v));
                        }
                    }
                    splits.sort_by(|x, y| {
                        x.0.partial_cmp(&y.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(x.1.cmp(&y.1))
                    });
                    let mut prev = s;
                    for (_, v) in splits {
                        inserted.insert((prev.min(v), prev.max(v)));
                        new_cycle.push((prev, v));
                        prev = v;
                    }
                    if prev != s {
                        inserted.insert((prev.min(e), prev.max(e)));
                    }
                    new_cycle.push((prev, e));
                }
                // Amendment 1a: cancel null excursions (adjacent inverse
                // directed pairs, wrap-around included) in which at least one
                // member is a split-inserted segment — a spur made
                // self-pairing by the split is a zero-width slit that leaves
                // χ odd (E+1, no face). Restricting to split-inserted members
                // keeps non-S7 loops byte-identical and legitimate bigons
                // untouched. Iterate to a fixed point (cancellation can make
                // a new pair adjacent).
                if !inserted.is_empty() {
                    loop {
                        let m = new_cycle.len();
                        let mut cancelled = false;
                        'scan: for i in 0..m {
                            let j = (i + 1) % m;
                            if m < 2 {
                                break;
                            }
                            let (a1, b1) = new_cycle[i];
                            let (a2, b2) = new_cycle[j];
                            if a1 == b2
                                && b1 == a2
                                && (inserted.contains(&(a1.min(b1), a1.max(b1))))
                            {
                                let (hi, lo) = if i < j { (j, i) } else { (i, j) };
                                new_cycle.remove(hi);
                                new_cycle.remove(lo);
                                cancelled = true;
                                break 'scan;
                            }
                        }
                        if !cancelled {
                            break;
                        }
                    }
                }
                info_cycles.push(new_cycle);
            }
            out.push(info_cycles);
            continue;
        }
        let mut info_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(info.cycles.len());
        for (ci, cycle) in info.cycles.iter().enumerate() {
            let lid = cycle_loop_ids[ii][ci];
            let mut new_cycle: Vec<(u32, u32)> = Vec::with_capacity(cycle.len());
            for &(s, e) in cycle {
                let pa = mesh.verts[s as usize].as_array();
                let pb = mesh.verts[e as usize].as_array();
                let mut splits: Vec<(RBig, u32)> = Vec::new();
                for (&v, luse) in &vertex_loops {
                    if v == s || v == e {
                        continue;
                    }
                    // Must be used by a loop OTHER than this one.
                    if !luse.iter().any(|&l| l != lid) {
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    if let Some(t_num) = on_open_segment_param(pa, pb, pv) {
                        splits.push((t_num, v));
                    }
                }
                splits.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
                let mut prev = s;
                for (_, v) in splits {
                    new_cycle.push((prev, v));
                    prev = v;
                }
                new_cycle.push((prev, e));
            }
            info_cycles.push(new_cycle);
        }
        out.push(info_cycles);
    }
    out
}

/// The outer boundary of a face among its boundary cycles.
///
/// For a BOUNDED patch on a cylinder or cone — no cycle encircles the axis —
/// the outer loop is the cycle of LARGEST |Newell area vector| (the loop's
/// extent: its projection onto its own mean plane, the reading the planar
/// branch uses). Edge COUNT is sampling density, not extent: R0070's 226-edge
/// ellipse-chain hole outnumbered its 14-edge outer rim loop (chart areas
/// 3.7e-5 vs 7.3e-4) and the face was labelled inside-out — the next op's
/// holed chart CDT then emptied it.
///
/// For a periodic STRIP (some cycle winds the axis — two rims and their
/// windows) and for every other surface kind, the labels carry no geometric
/// meaning (KV14 Slice B classifies strip loops by winding) and the
/// historical deterministic choice is kept byte-identically: MOST edges, then
/// the lowest start-vertex index. (R0099, 2026-09-11: relabelling a tube's
/// two rims — equal edge counts, |N| within 0.13 % — tripped kernel-v2's
/// `cylpatch-vertex` import check one op later; a latent label sensitivity
/// there, not a reason to relabel.)
pub(crate) fn select_outer_cycle(
    cycles: &[Vec<(u32, u32)>],
    verts: &[Point3],
    surface: Surface,
) -> usize {
    let cycle_min_vert = |c: &[(u32, u32)]| c.iter().map(|&(s, _)| s).min().unwrap_or(0);
    let by_edges = {
        let mut best = 0usize;
        for i in 1..cycles.len() {
            let (cur, b) = (cycles[i].len(), cycles[best].len());
            if cur > b || (cur == b && cycle_min_vert(&cycles[i]) < cycle_min_vert(&cycles[best])) {
                best = i;
            }
        }
        best
    };
    let (ap, ad) = match surface {
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => (axis_point, axis_dir),
        Surface::Cone { apex, axis_dir, .. } => (apex, axis_dir),
        _ => return by_edges,
    };
    let au = normalize3(ad.as_array());
    let (e1, e2) = ortho_basis(ad);
    let (e1a, e2a, apa) = (e1.as_array(), e2.as_array(), ap.as_array());
    let theta = |g: u32| -> f64 {
        let p = verts[g as usize].as_array();
        let w = [p[0] - apa[0], p[1] - apa[1], p[2] - apa[2]];
        let h = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let r = [w[0] - h * au[0], w[1] - h * au[1], w[2] - h * au[2]];
        (r[0] * e2a[0] + r[1] * e2a[1] + r[2] * e2a[2])
            .atan2(r[0] * e1a[0] + r[1] * e1a[1] + r[2] * e1a[2])
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let encircles = |c: &[(u32, u32)]| -> bool {
        let m = c.len();
        let mut sum = 0.0;
        for i in 0..m {
            let mut d = theta(c[(i + 1) % m].0) - theta(c[i].0);
            while d > std::f64::consts::PI {
                d -= two_pi;
            }
            while d < -std::f64::consts::PI {
                d += two_pi;
            }
            sum += d;
        }
        sum.abs() > 1.5 * std::f64::consts::PI
    };
    if cycles.iter().any(|c| encircles(c)) {
        return by_edges;
    }
    let newell_mag = |c: &[(u32, u32)]| -> f64 {
        let m = c.len();
        let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..m {
            let a = verts[c[i].0 as usize].as_array();
            let b = verts[c[(i + 1) % m].0 as usize].as_array();
            nx += a[1] * b[2] - a[2] * b[1];
            ny += a[2] * b[0] - a[0] * b[2];
            nz += a[0] * b[1] - a[1] * b[0];
        }
        (nx * nx + ny * ny + nz * nz).sqrt()
    };
    let mut outer_idx = 0usize;
    let mut best = cycles.first().map_or(0.0, |c| newell_mag(c));
    for (i, c) in cycles.iter().enumerate().skip(1) {
        let mag = newell_mag(c);
        let better = mag > best
            || (mag == best
                && (c.len() > cycles[outer_idx].len()
                    || (c.len() == cycles[outer_idx].len()
                        && cycle_min_vert(c) < cycle_min_vert(&cycles[outer_idx]))));
        if better {
            outer_idx = i;
            best = mag;
        }
    }
    outer_idx
}

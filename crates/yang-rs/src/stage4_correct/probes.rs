#[allow(clippy::wildcard_imports)]
use super::*;

/// Diagnostic helper for `YANG_S4_RIM_SNAP_TARGET`: the same bound the rim-snap
/// pass uses (the larger of the two operands' Stage-1 chord budgets).
pub(crate) fn bound_probe(a: &BRep, b: &BRep) -> f64 {
    [InputId::A, InputId::B]
        .into_iter()
        .filter_map(|i| crate::stage3_ssi::chord_tol_for_curved_owner(i, a, b, 0, (0, 0)).ok())
        .fold(0.0f64, f64::max)
}

/// Diagnostic helper: short surface-kind name for probe output.
pub(crate) fn surface_kind_name(s: Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Plane",
        Surface::Cylinder { .. } => "Cylinder",
        Surface::Cone { .. } => "Cone",
        Surface::Sphere { .. } => "Sphere",
        Surface::Torus { .. } => "Torus",
    }
}

/// Read-only NON-MANIFOLD EDGE census (`YANG_NM_EDGE_PROBE=1`), printed at the
/// same six Stage-4 checkpoints as [`star_probe`]. Reports how many undirected
/// edges carry fewer than two triangles (an open boundary) and how many carry
/// more than two (a pinch), then lists every over-2 edge with its endpoints,
/// their positions, the incident triangles and each triangle's `(input, face)`.
///
/// This is the instrument that located F0060's real owner (ledger 2026-09-13
/// night-latest). The case's cap plane is tangent to the other operand's lateral
/// along a whole diameter, and the census reads **14 over-2 edges already at
/// `s4-entry`** — so the pinch comes out of the ARRANGEMENT, not out of any
/// Stage-4 pass; §4.4.1(b) then reduces them to 3 rather than causing them,
/// which is the opposite of what the earlier reading assumed. Every one of the
/// 14 carries exactly four triangles with the signature `2 × (A, cap face) +
/// 2 × (B, lateral)`, which is what makes the per-sheet pairing FORCED rather
/// than a radial sort: each operand contributes one forward and one reverse
/// triangle, so pairing each A triangle with the B triangle of opposite
/// orientation is the only consistent split.
///
/// Byte-identical when the env is unset (the function returns before touching
/// anything).
/// Census (`YANG_COINCIDENT_PROBE`, read-only): DISTINCT mesh vertices at
/// bit-identical positions, per checkpoint.
///
/// Output vertices are 1:1 with `mesh.verts` (Stage 6 §(1)), so such a pair
/// leaves the pipeline as two B-Rep vertices at one point — a pinch the next
/// boolean's Stage-1 chart CDT refuses (`duplicate (coincident) loop vertex`,
/// P0020). Every sub-resolution collapse deliberately EXCLUDES the exact-zero
/// pair (KV15b B3, #194 B3), so no gate names it; this census is how a pair is
/// attributed to the checkpoint that minted it.
pub(crate) fn coincident_vertex_census(tag: &str, mesh: &crate::Mesh) {
    if std::env::var_os("YANG_COINCIDENT_PROBE").is_none() {
        return;
    }
    let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
        std::collections::BTreeMap::new();
    for (v, p) in mesh.verts.iter().enumerate() {
        by_pos
            .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
            .or_default()
            .push(v as u32);
    }
    let groups: Vec<&Vec<u32>> = by_pos.values().filter(|g| g.len() > 1).collect();
    eprintln!(
        "[coincident {tag}] {} verts, {} coincident group(s)",
        mesh.verts.len(),
        groups.len()
    );
    for g in groups {
        let joined = mesh
            .tris
            .iter()
            .filter(|t| t.iter().filter(|v| g.contains(v)).count() > 1)
            .count();
        let p = mesh.verts[g[0] as usize].as_array();
        eprintln!(
            "[coincident {tag}]   {g:?} at ({:.17e},{:.17e},{:.17e}) joined_tris={joined}",
            p[0], p[1], p[2]
        );
    }
    // Sub-`TAU_MODEL` mesh EDGES at the same checkpoint: the pair that is not
    // yet coincident but is already one model point. Same instrument, because
    // the two questions are always asked together.
    let mut short: Vec<((u32, u32), f64)> = Vec::new();
    let mut seen_edge: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for t in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (x, y) = (t[i].min(t[j]), t[i].max(t[j]));
            if x == y || !seen_edge.insert((x, y)) {
                continue;
            }
            let (p, q) = (
                mesh.verts[x as usize].as_array(),
                mesh.verts[y as usize].as_array(),
            );
            let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
            let scale = p
                .iter()
                .chain(q.iter())
                .fold(0.0f64, |m, &c| m.max(c.abs()));
            if d < cad_primitives::TAU_MODEL * (1.0 + scale) {
                short.push(((x, y), d));
            }
        }
    }
    short.sort_by(|a, b| a.1.total_cmp(&b.1));
    eprintln!("[coincident {tag}] {} sub-d_p mesh edge(s)", short.len());
    for ((x, y), d) in short.iter().take(20) {
        eprintln!("[coincident {tag}]   edge ({x},{y}) len={d:.3e}");
    }
}

thread_local! {
    static S4_LAP: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) }; // wasm-ok: env-gated probe (YANG_STAGE_TIME_PROBE), never set in wasm
}

/// Print-only Stage-4 lap timer behind `YANG_STAGE_TIME_PROBE` (matrix M-1:
/// which Stage-4 pass scales with the mesh rather than the curve). Each call
/// prints the time since the previous lap; `tag == "s4-entry"` restarts.
pub(crate) fn s4_lap(tag: &str, mesh: &crate::Mesh) {
    if std::env::var_os("YANG_STAGE_TIME_PROBE").is_none() {
        return;
    }
    let now = std::time::Instant::now(); // wasm-ok: env-gated probe (YANG_STAGE_TIME_PROBE), never set in wasm
    let prev = S4_LAP.with(|c| c.replace(Some(now)));
    if tag != "s4-entry" {
        if let Some(prev) = prev {
            eprintln!(
                "[s4-lap] {tag} {:.2}s (tris={})",
                (now - prev).as_secs_f64(),
                mesh.tris.len()
            );
        }
    }
}

pub(crate) fn nonmanifold_edge_census(
    tag: &str,
    mesh: &crate::Mesh,
    attribution: &TriangleAttributionMap,
) {
    s4_lap(tag, mesh);
    if std::env::var_os("YANG_NM_EDGE_PROBE").is_none() {
        return;
    }
    nonmanifold_edge_census_with(tag, mesh, &|t| {
        attribution.lookup(t).map(|a| (a.input, a.face))
    });
}

/// Same census over a raw per-triangle attribution slice — the shape the
/// Stage-4 interior passes carry (`attr_vec`), so the four passes between
/// `after-reloc` and `before-3c-merge` can each be checkpointed (P0020's
/// bisect, 2026-10-03). Byte-identical when `YANG_NM_EDGE_PROBE` is unset.
pub(crate) fn nonmanifold_edge_census_attrs(
    tag: &str,
    mesh: &crate::Mesh,
    attrs: &[Option<TriangleAttribution>],
) {
    if std::env::var_os("YANG_NM_EDGE_PROBE").is_none() {
        return;
    }
    nonmanifold_edge_census_with(tag, mesh, &|t| {
        attrs
            .get(t as usize)
            .copied()
            .flatten()
            .map(|a| (a.input, a.face))
    });
}

pub(crate) fn nonmanifold_edge_census_with(
    tag: &str,
    mesh: &crate::Mesh,
    attr_of: &dyn Fn(u32) -> Option<(crate::InputId, u32)>,
) {
    let mut inc: std::collections::BTreeMap<(u32, u32), Vec<u32>> =
        std::collections::BTreeMap::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            let key = if a < b { (a, b) } else { (b, a) };
            inc.entry(key).or_default().push(ti as u32);
        }
    }
    let open = inc.values().filter(|v| v.len() < 2).count();
    let over: Vec<(&(u32, u32), &Vec<u32>)> = inc.iter().filter(|(_, v)| v.len() > 2).collect();
    eprintln!(
        "[nm-edge {tag}] {} tris, {open} open edge(s), {} over-2 edge(s)",
        mesh.tris.len(),
        over.len()
    );
    for (&(u, v), tris) in over {
        let (pu, pv) = (mesh.verts[u as usize], mesh.verts[v as usize]);
        eprintln!(
            "[nm-edge {tag}]   ({u},{v}) n={} u=({:.9},{:.9},{:.9}) v=({:.9},{:.9},{:.9}) \
             tris={tris:?} attrs={:?}",
            tris.len(),
            pu.x(),
            pu.y(),
            pu.z(),
            pv.x(),
            pv.y(),
            pv.z(),
            tris.iter().map(|&t| attr_of(t)).collect::<Vec<_>>()
        );
    }
}

/// Read-only positional STAR probe (`YANG_STAR_PROBE="x,y,z"`), printed at the
/// named Stage-4 checkpoints (`s4-entry`, `after-reloc`, `before-3c-merge`,
/// `after-3c-merge`, `before-3d`, `before-validate`).
///
/// For the given model position it reports every mesh vertex within 1e-9 of it,
/// the six NEAREST vertices with their distances, and — for each hit — the whole
/// incident triangle star with each triangle's `(input, face)` attribution and
/// its corner positions.
///
/// This is the instrument that read C0058's tangency anatomy (spec
/// `yang_433_tangent_point_mesh_update.md`): a position-keyed probe survives the
/// vertex RENUMBERING that the collapse/compact/merge passes do between
/// checkpoints, which an id-keyed one cannot. Byte-identical when the env is
/// unset (the function returns before touching anything).
/// Read-only WHOLE-MESH dump (`YANG_MESH_DUMP=1`), printed at every checkpoint
/// that calls `star_probe`: every vertex with its position, every triangle
/// with its `(input, face)` attribution. The instrument for a face whose
/// emitted boundary cycle carries an edge the arrangement should have split
/// (P0001: B's tip edge lying IN A's cap plane) — the star probe answers
/// "what is at this point?", this one answers "which triangles exist at all?".
/// Byte-identical when the env is unset.
pub(crate) fn mesh_dump_probe(tag: &str, mesh: &crate::Mesh, attribution: &TriangleAttributionMap) {
    if std::env::var_os("YANG_MESH_DUMP").is_none() {
        return;
    }
    eprintln!(
        "[mesh-dump {tag}] {} verts {} tris",
        mesh.verts.len(),
        mesh.tris.len()
    );
    for (i, p) in mesh.verts.iter().enumerate() {
        eprintln!("[mesh-dump {tag}] v{i} = {:?}", p.as_array());
    }
    for (t, tri) in mesh.tris.iter().enumerate() {
        let attr = attribution
            .lookup(t as u32)
            .map(|a| format!("{:?}:{}", a.input, a.face))
            .unwrap_or_else(|| "none".to_string());
        eprintln!("[mesh-dump {tag}] t{t} = {tri:?} attr={attr}");
    }
}

pub(crate) fn star_probe(tag: &str, mesh: &crate::Mesh, attribution: &TriangleAttributionMap) {
    mesh_dump_probe(tag, mesh, attribution);
    let Ok(spec) = std::env::var("YANG_STAR_PROBE") else {
        return;
    };
    let want: Vec<f64> = spec
        .split(',')
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .collect();
    if want.len() != 3 {
        return;
    }
    let hits: Vec<u32> = mesh
        .verts
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            (p.x() - want[0]).abs() < 1e-9
                && (p.y() - want[1]).abs() < 1e-9
                && (p.z() - want[2]).abs() < 1e-9
        })
        .map(|(i, _)| i as u32)
        .collect();
    eprintln!(
        "[star-probe {tag}] verts at {want:?} = {hits:?} (mesh {} tris)",
        mesh.tris.len()
    );
    let mut near: Vec<(f64, u32)> = mesh
        .verts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d =
                ((p.x() - want[0]).powi(2) + (p.y() - want[1]).powi(2) + (p.z() - want[2]).powi(2))
                    .sqrt();
            (d, i as u32)
        })
        .collect();
    near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for &(d, i) in near.iter().take(6) {
        let q = mesh.verts[i as usize];
        eprintln!(
            "[star-probe {tag}]   near v{i} d={d:.6e} ({}, {}, {})",
            q.x(),
            q.y(),
            q.z()
        );
    }
    for &v in &hits {
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if tri.contains(&v) {
                eprintln!(
                    "[star-probe {tag}]   v{v} tri {ti} = {tri:?} attr={:?} pts={:?}",
                    attribution.lookup(ti as u32),
                    tri.iter()
                        .map(|&u| {
                            let q = mesh.verts[u as usize];
                            (u, q.x(), q.y(), q.z())
                        })
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

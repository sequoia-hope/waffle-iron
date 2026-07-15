//! Stage 4 — mesh correction: Phase-A patch census, vertex collapse,
//! sub-resolution segment collapse, relocation application + reversal
//! sweeps, relocated-triangle validation (extracted verbatim from
//! lib.rs — spec `specs/yang_rs_lib_decomposition.md`, increment 7).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// §4.4.2 tangent-plane corridor half-width. The two surfaces at a
/// relocation site, linearized as tangent planes P_A, P_B meeting at angle
/// θ, admit a corridor of `2·budget/sinθ` around L = P_A ∩ P_B — the
/// Stage-1 budget mapped through the wedge (paper
/// refs/text/yang2025_hybrid_boolean.txt:494-537). `divergence` is the
/// unit-vector cross-product magnitude (= sin θ), or the gradient magnitude
/// for implicit forms. At exact tangency the corridor is unbounded →
/// `INFINITY` (the circle-junction gate precedent: the projection is still
/// the local nearest point; callers that need a finite band gate on a
/// tangency cutoff FIRST). Extracted from five duplicated sites (design
/// review 2026-07-12 F8) — the formula must never be re-inlined: a future
/// correction has to land HERE once.
pub(crate) fn tangent_plane_corridor(budget: f64, divergence: f64) -> f64 {
    if divergence > 0.0 {
        2.0 * budget / divergence
    } else {
        f64::INFINITY
    }
}

// =========================================================================
// PR-YR5 — topology reconstruction
// =========================================================================

/// PR-YR5 internal: the triple `(vertices, edges, faces)` produced
/// by `reconstruct_topology` to populate the output `BRep`.
///
/// PR-YR10: extended with a fourth component — the per-output-mesh-vertex
/// `Vec<TessellationSource>` (default `BRepVertex(i)`, overridden to
/// `BRepEdge { edge, t }` for Stage-4-relocated intersection vertices).
pub(crate) type ReconstructedTopology = (
    Vec<BRepVertex>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    Vec<TessellationSource>,
    // PR-KV13 F2: per-output-face attribution, parallel to `faces` — the
    // `(input, face)` the patch descends from (the kernel maps it to the
    // operand's persistent face id for boolean provenance).
    Vec<TriangleAttribution>,
);

/// PR-YR5/9 `(vertices, edges, faces)` triple — the pre-PR-YR10 reconstruction
/// shape retained for the `#[cfg(test)]` unit-test callers.
#[cfg(test)]
pub(crate) type LegacyTopology = (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>);

/// PR-YR9 (lifted to module scope in PR-YR10 so `stage4_relocate_and_correct`
/// can consume the same ordered, oriented patch loops + inherited surface that
/// the Phase-B emission uses — no re-derivation, no classification drift).
pub(crate) struct PatchInfo {
    pub(crate) cycles: Vec<Vec<(u32, u32)>>,
    pub(crate) input: InputId,
    pub(crate) inherited: Surface,
    pub(crate) face_idx: usize,
    /// The INPUT face's cavity sense (PR-KV6b-1): a kept patch of an
    /// already-reversed input wall (e.g. a washer's inner tube) must keep
    /// its sense in the output — composed by XOR with the Subtract-B flip.
    pub(crate) input_reversed: bool,
    /// Spec yang_stage6_sliver_topology §2/§4B: this patch contained ≥1 FOLD
    /// sliver that §4A excluded from boundary derivation (`patch_fold_slivers`).
    /// Such a patch may carry a whole shared solid edge as ONE un-subdivided
    /// chord (the collapsed subdivision the slivers used to represent), so it
    /// — and ONLY it — is eligible for the §4B loop T-subdivision. Patches
    /// with no excluded fold sliver keep byte-identical loops (the measured
    /// chord lives on the fold-bearing side; the other side already
    /// subdivides), which keeps curved / benign-T-junction output at exact
    /// reference parity.
    pub(crate) had_fold_sliver: bool,
}

/// PR-YR10: the Phase-A structures `reconstruct_topology` derives before the
/// Phase-B emission: per-patch ordered loops + inherited surface (`infos`), the
/// edge→incident-(input,surface) map (`incidence`), and the exact per-edge
/// analytical `Curve` map (`curves`). Recomputed after a §4.5.3 collapse.
pub(crate) type PhaseA = (
    Vec<PatchInfo>,
    std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    std::collections::BTreeMap<(u32, u32), Curve>,
);

/// PR-YR10: compute the Phase-A structures (adjacency → patches → cycles →
/// incidence → exact intersection curves) from the current mesh + attribution.
/// Factored out of `reconstruct_topology` so it can be re-run after a §4.5.3
/// collapse mutates the mesh.
pub(crate) fn compute_phase_a(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<PhaseA, YangError> {
    let adjacency = triangle_adjacency(mesh);
    let patches = flood_fill_patches(mesh, attribution, &adjacency);
    // PR-YR27 (Finding 1a): merge edge-adjacent patches lying on the SAME
    // plane with the SAME orientation into one output face — a coplanar
    // boolean otherwise emits e.g. A's and B's side fragments as two faces
    // on one bit-identical plane, and the NEXT boolean in a chain
    // exact-ties between them. Non-adjacent same-plane patches stay
    // separate faces (their union is not a single connected face).
    let patches = merge_same_plane_patches(patches, &adjacency, a, b);

    let mut infos: Vec<PatchInfo> = Vec::with_capacity(patches.len());
    for patch in &patches {
        let cycles = patch_boundary_cycle(patch, mesh)?;
        let input = patch.attribution.input;
        let input_brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        let face_idx = patch.attribution.face as usize;
        if face_idx >= input_brep.faces().len() {
            return Err(YangError::MalformedTopology(format!(
                "attribution.face = {face_idx} out of range (input has {} faces)",
                input_brep.faces().len()
            )));
        }
        let inherited = input_brep.faces()[face_idx].surface;
        let input_reversed = input_brep.faces()[face_idx].reversed;
        let had_fold_sliver = !patch_fold_slivers(patch, mesh).is_empty();
        infos.push(PatchInfo {
            cycles,
            input,
            inherited,
            face_idx,
            input_reversed,
            had_fold_sliver,
        });
    }

    let mut incidence: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    for info in &infos {
        for cycle in &info.cycles {
            for &(s, e) in cycle {
                let key = if s < e { (s, e) } else { (e, s) };
                incidence
                    .entry(key)
                    .or_default()
                    .push((info.input, info.inherited));
            }
        }
    }
    let curves = build_intersection_curves(&incidence, mesh, a, b)?;
    Ok((infos, incidence, curves))
}

/// PR-YR27 (Finding 1a): merge edge-adjacent output patches whose inherited
/// planes are the same plane with the same orientation (bit-identical or
/// within `TAU_WORK` on the UNIT-normalized `(n̂, d̂)`) into ONE patch, so
/// Stage 6 emits one face per connected same-plane region of the output
/// solid.
///
/// Why: a coplanar boolean's output legitimately carries triangles from
/// BOTH inputs' faces on one geometric plane (e.g. exactly stacked boxes:
/// each side plane has an A fragment and a B fragment, edge-adjacent along
/// the seam). `flood_fill_patches` groups by attribution, so those
/// fragments emit as TWO faces on a bit-identical plane — a fragmented
/// B-Rep whose NEXT boolean exact-ties Stage-6 membership between them
/// (assay F0066). Merging is keyed to edge adjacency: non-adjacent
/// same-plane patches (genuinely separate faces) are NOT merged.
///
/// Safety / blast radius:
/// - Only `Surface::Plane` patches participate; the orientation test
///   (component-wise `|n̂ᵢ−n̂ⱼ| ≤ TAU_WORK`) means an opposite-normal pair
///   (e.g. a subtract cavity wall against an outer wall) NEVER merges.
/// - Distinct input faces on one plane only exist when an input itself
///   carries same-plane faces or the two inputs share a plane — exactly
///   the coplanar classes; every other fixture has zero mergeable pairs
///   and is byte-identical.
/// - The merged patch's attribution is the lexicographically smallest
///   member `(input, face)` (deterministic); the members' inherited
///   surfaces agree within `TAU_WORK`, so the choice is geometric noise.
/// - The seam edges become patch-INTERIOR (they vanish from the boundary
///   cycles and therefore from the output edge set) — the merged region's
///   single outer cycle is exactly the §4.5.5 result-face boundary.
pub(crate) fn merge_same_plane_patches(
    mut patches: Vec<Patch>,
    adjacency: &[Vec<u32>],
    a: &BRep,
    b: &BRep,
) -> Vec<Patch> {
    if patches.len() < 2 {
        return patches;
    }

    // Inherited surface key per patch (`None` = unmergeable surface kind or
    // degenerate — never merged). A `Plane` keys on its unit `(n̂, d̂)`; a
    // `Cylinder` keys on its unit axis, an axis-line anchor (the axis point
    // projected to remove the free axial slide), the radius, AND the effective
    // outward sense (`reversed`) — two coincident cylinders of OPPOSITE sense
    // (a bore wall vs an outer wall) must NEVER merge (PR-5; mirrors the planar
    // opposite-normal guard). Spheres/cones keep `None` (not yet needed).
    enum SurfKey {
        Plane {
            n: [f64; 3],
            d: f64,
        },
        Cyl {
            axis: [f64; 3],
            anchor: [f64; 3],
            radius: f64,
            reversed: bool,
        },
    }
    let keys: Vec<Option<SurfKey>> = patches
        .iter()
        .map(|p| {
            let brep = match p.attribution.input {
                InputId::A => a,
                InputId::B => b,
            };
            let f = brep.faces().get(p.attribution.face as usize)?;
            match f.surface {
                Surface::Plane { normal, d } => {
                    let n = normal.as_array();
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    Some(SurfKey::Plane {
                        n: [n[0] / len, n[1] / len, n[2] / len],
                        d: d / len,
                    })
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    let ad = axis_dir.as_array();
                    let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    let axis = [ad[0] / len, ad[1] / len, ad[2] / len];
                    // Anchor = axis_point with its axial component removed, so
                    // two cylinders sharing one axis LINE but with axis points at
                    // different axial offsets get an identical anchor.
                    let ap = axis_point.as_array();
                    let t = ap[0] * axis[0] + ap[1] * axis[1] + ap[2] * axis[2];
                    let anchor = [
                        ap[0] - t * axis[0],
                        ap[1] - t * axis[1],
                        ap[2] - t * axis[2],
                    ];
                    Some(SurfKey::Cyl {
                        axis,
                        anchor,
                        radius,
                        reversed: f.reversed,
                    })
                }
                _ => None,
            }
        })
        .collect();
    let mergeable = |i: usize, j: usize| -> bool {
        match (&keys[i], &keys[j]) {
            (Some(SurfKey::Plane { n: ni, d: di }), Some(SurfKey::Plane { n: nj, d: dj })) => {
                (di - dj).abs() <= cad_primitives::TAU_WORK
                    && (0..3).all(|k| (ni[k] - nj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            (
                Some(SurfKey::Cyl {
                    axis: ai,
                    anchor: anchi,
                    radius: ri,
                    reversed: revi,
                }),
                Some(SurfKey::Cyl {
                    axis: aj,
                    anchor: anchj,
                    radius: rj,
                    reversed: revj,
                }),
            ) => {
                // Same effective sense, equal radius, parallel axes, same axis
                // line (anchors agree up to TAU_WORK; axes may be antiparallel —
                // a cylinder's axis_dir sign is free — so compare |aᵢ·aⱼ|≈1).
                revi == revj
                    && (ri - rj).abs() <= cad_primitives::TAU_WORK
                    && (ai[0] * aj[0] + ai[1] * aj[1] + ai[2] * aj[2]).abs()
                        >= 1.0 - cad_primitives::TAU_WORK
                    && (0..3).all(|k| (anchi[k] - anchj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            _ => false,
        }
    };

    // patch index per mesh triangle.
    let mut patch_of: Vec<usize> = vec![usize::MAX; adjacency.len()];
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            patch_of[t as usize] = pi;
        }
    }

    // Union-find over patches, united on (edge-adjacent AND same-plane).
    let mut parent: Vec<usize> = (0..patches.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            for &u in &adjacency[t as usize] {
                let pj = patch_of[u as usize];
                if pj == usize::MAX || pj == pi {
                    continue;
                }
                if mergeable(pi, pj) {
                    let (ri, rj) = (find(&mut parent, pi), find(&mut parent, pj));
                    if ri != rj {
                        parent[ri.max(rj)] = ri.min(rj);
                    }
                }
            }
        }
    }

    // Rebuild merged patches in first-member order (deterministic; a strict
    // no-op — same patches, same order — when nothing merged).
    let roots: Vec<usize> = (0..patches.len()).map(|i| find(&mut parent, i)).collect();
    let mut out: Vec<Patch> = Vec::with_capacity(patches.len());
    let mut taken = vec![false; patches.len()];
    for i in 0..patches.len() {
        if taken[i] {
            continue;
        }
        let members: Vec<usize> = (i..patches.len())
            .filter(|&j| roots[j] == roots[i])
            .collect();
        for &m in &members {
            taken[m] = true;
        }
        let attribution = members
            .iter()
            .map(|&m| patches[m].attribution)
            .min()
            .expect("members is non-empty");
        let mut tri_indices: Vec<u32> = Vec::new();
        for &m in &members {
            tri_indices.append(&mut patches[m].tri_indices);
        }
        out.push(Patch {
            attribution,
            tri_indices,
        });
    }
    out
}

/// PR-YR15 helper: the Stage-1 curved chord bound of ONE input, choosing the
/// surface's OWN bound (A14.3 / I-sphere-band). A `Surface::Sphere` face's
/// tessellation vertices sit off the exact great circle by up to the sphere's
/// own `sphere_chord_bound(radius) = 1e-2·2r√3`, which is LARGER than the
/// rim-AABB `curved_chord_bound` (2r√2) — so a sphere-bearing input must report
/// its sphere bound, NOT the rim band (which would underestimate and reject
/// valid sphere-rim vertices). Cylinder/all-planar inputs keep the rim-AABB
/// `curved_chord_bound` byte-for-byte. When both are present we take the MAX
/// (the budget must admit every curved-surface vertex). `None` only for an
/// all-planar input (zero chord error). This is the SINGLE source consulted by
/// both `build_intersection_curves` (selection tol) and `stage4_chord_band`
/// (relocation budget); it is NOT tolerance widening.
pub(crate) fn input_curved_chord_bound(brep: &BRep) -> Option<f64> {
    // Spec `yang_s3_ellipse_rim_chord_bound` amendment 1: an ellipse-rim-only
    // input (obliquely-trimmed cylinder re-entering from a prior boolean)
    // carries the Stage-1 ellipse chain bound — fallback-only composition,
    // byte-identical whenever a Circle rim exists.
    let rim = curved_chord_bound(brep.edges()).or_else(|| ellipse_rim_chord_bound(brep.edges()));
    let sphere = brep
        .faces()
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Sphere { radius, .. } => Some(sphere_chord_bound(radius)),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        });
    match (rim, sphere) {
        (Some(r), Some(s)) => Some(r.max(s)),
        (Some(r), None) => Some(r),
        (None, s) => s,
    }
}

/// PR-YR10 helper: the Stage-4 chord-band relocation budget `d_ε` — the
/// Stage-1 chord bound of whichever input bears a curved surface (the curved
/// solid). Uses [`input_curved_chord_bound`] so a sphere input reports its OWN
/// (larger) 2r√3 bound, not the rim-AABB 2r√2 (I-sphere-band). `None` only if
/// NEITHER input has a curved surface, which cannot happen when a conic
/// intersection edge exists (a conic edge implies a curved input).
pub(crate) fn stage4_chord_band(a: &BRep, b: &BRep) -> Option<f64> {
    // PR-KV7: the MAX of the two inputs' Stage-1 bounds, not A-with-B-
    // fallback. An arrangement vertex on an A×B intersection curve sits on
    // the curved OWNER's facet chord, off the exact curve by up to that
    // owner's OWN sagitta — and with chainable boolean outputs the owner
    // can be EITHER input (a recovered body re-entering as A can have a
    // much tighter rim AABB than the fresh operand B whose curves are
    // being relocated). `max` admits exactly up to the looser owner's
    // honest Stage-1 bound for this model pair — a derived bound, not
    // tolerance widening. (Per-curve owner resolution, as Stage-3's
    // `chord_tol_for_curved_owner` does for selection, is the M5-era
    // refinement; `max` is its conservative envelope.)
    match (input_curved_chord_bound(a), input_curved_chord_bound(b)) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// PR-YR10 helper: edge-collapse `victim` onto `survivor` in `mesh` + the
/// parallel `attribution`. Replaces every `victim` index with `survivor`, then
/// drops the now-degenerate triangles (two equal indices) from BOTH the mesh
/// and the attribution in lockstep. A proper edge-collapse preserves the
/// watertight half-edge pairing (the two collapsed slivers' surviving directed
/// edges are mutual opposites that cancel — spec §4.5.3 / boolean() sliver rule
/// at the compaction step). The cancellation also covers the COINCIDENT-PAIR
/// form (spec `yang_collapse_membrane_cancellation`): an exact duplicate
/// triangle pair with opposite windings — the pleat spanning the twin gap —
/// is a zero-volume flap whose directed edges pair with each other; both
/// copies are dropped. Returns the number of triangles dropped.
/// N2 §4.4.1 mesh-updating: re-triangulate a degenerate CYLINDER patch in its
/// `(θ, z)` parametric domain, KEEPING every vertex (no geometry moves — the
/// re-CDT only re-connects existing vertices), so a collinear-generator sliver
/// band becomes valid triangles. Returns `Ok(true)` if any patch was re-meshed
/// (the caller re-scans), `Ok(false)` if no eligible patch exists (caller keeps
/// its loud STOP). SCOPED: `Surface::Cylinder` only, and only patches whose θ-span
/// is `< π` (no seam wrap — the full-ring / seam-straddling case is deferred to
/// the periodic-θ closer, spec §5c.5). Any malformed boundary / CDT failure is a
/// loud STOP (`LocalRefinementRequired`), never a silent accept (P9/P10).
///
/// Faithful-ness: this is §4.4.1 CDT re-triangulation. It moves NO vertex, drops
/// none, adds no Steiner point — so it cannot distort neighbour geometry (the
/// R0091 silent-wrong the tolerance-collapse would risk). The watertight/validity
/// re-gate the caller runs after this is the proof gate.
#[allow(clippy::too_many_lines)]
pub(crate) fn replan_degenerate_cylinder_patches(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
    brep_a: &BRep,
    brep_b: &BRep,
) -> Result<bool, YangError> {
    use std::collections::BTreeSet;
    let pi = std::f64::consts::PI;
    let degen_area = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
    let is_degen = |t: [u32; 3], mesh: &Mesh| -> bool {
        if !t.iter().any(|v| moved.contains(v)) {
            return false;
        }
        let av = tri_area_vector(
            mesh.verts[t[0] as usize].as_array(),
            mesh.verts[t[1] as usize].as_array(),
            mesh.verts[t[2] as usize].as_array(),
        );
        0.5 * (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() < degen_area
    };
    let attr_of =
        |ti: usize| -> Option<TriangleAttribution> { attr_vec.get(ti).copied().flatten() };
    let key_of = |at: TriangleAttribution| (matches!(at.input, InputId::A), at.face);
    let surf_of = |at: TriangleAttribution| -> Surface {
        let br = match at.input {
            InputId::A => brep_a,
            InputId::B => brep_b,
        };
        br.faces()[at.face as usize].surface
    };

    // Attributions carrying a degenerate triangle on a Cylinder face.
    let mut targets: BTreeSet<(bool, u32)> = BTreeSet::new();
    for ti in 0..mesh.tris.len() {
        if is_degen(mesh.tris[ti], mesh) {
            if let Some(at) = attr_of(ti) {
                if matches!(surf_of(at), Surface::Cylinder { .. }) {
                    targets.insert(key_of(at));
                }
            }
        }
    }
    if targets.is_empty() {
        return Ok(false);
    }

    // Copy through every triangle NOT in a target patch; remesh each target.
    let mut new_tris: Vec<[u32; 3]> = Vec::new();
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::new();
    for ti in 0..mesh.tris.len() {
        let keep = attr_of(ti).is_none_or(|at| !targets.contains(&key_of(at)));
        if keep {
            new_tris.push(mesh.tris[ti]);
            new_attr.push(attr_of(ti));
        }
    }

    // Global undirected edge → incident-triangle attribution keys (whole mesh).
    // Used to define a patch's TRUE seam: an edge is a patch boundary iff it is
    // shared with a triangle of a DIFFERENT attribution (or is a mesh boundary) —
    // so the re-mesh reproduces the neighbour's chain verbatim (spec §5c.7). This
    // is robust to the zero-area caps: a cap edge shared with another SAME-patch
    // triangle is interior (dropped), only cap edges facing a neighbour are seam.
    type AttrKey = Option<(bool, u32)>;
    let mut global_edge_attrs: std::collections::HashMap<(u32, u32), Vec<AttrKey>> =
        std::collections::HashMap::new();
    for ti in 0..mesh.tris.len() {
        let k = attr_of(ti).map(key_of);
        let tri = mesh.tris[ti];
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (u, v) = (tri[i], tri[j]);
            let e = if u < v { (u, v) } else { (v, u) };
            global_edge_attrs.entry(e).or_default().push(k);
        }
    }

    let mut remeshed = false;
    for &(is_a, face) in &targets {
        let at = TriangleAttribution {
            input: if is_a { InputId::A } else { InputId::B },
            face,
        };
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } = surf_of(at)
        else {
            continue;
        };
        let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
            .filter(|&t| attr_of(t as usize).is_some_and(|a| key_of(a) == (is_a, face)))
            .collect();

        // (θ, z) frame.
        let (e1, e2) = ortho_basis(axis_dir);
        let au = normalize3(axis_dir.as_array());
        let o = axis_point.as_array();
        let proj = |v: u32| -> (f64, f64) {
            let p = mesh.verts[v as usize].as_array();
            let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
            let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
            let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
            let z = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
            (y.atan2(x), z)
        };

        // Shared vertices: incident to ≥1 neighbour (different-attribution)
        // triangle — i.e. genuinely ON the intersection curve, present on both
        // sides. A generator-θ vertex that is NOT shared is a cylinder-only
        // tessellation vertex lying on the (straight) intersection line; the
        // neighbour's coarser chain skips it, so keeping it on our seam tears the
        // seam. Such vertices are collinear-redundant on the generator → DROP.
        let mut shared: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for ti in 0..mesh.tris.len() {
            if attr_of(ti).map(key_of) != Some((is_a, face)) {
                for &v in &mesh.tris[ti] {
                    shared.insert(v);
                }
            }
        }
        // Generator θ values (where the degenerate caps sit).
        let mut gen_theta: Vec<f64> = Vec::new();
        for &t in &patch_tris {
            if is_degen(mesh.tris[t as usize], mesh) {
                for &v in &mesh.tris[t as usize] {
                    let th = proj(v).0;
                    if !gen_theta.iter().any(|g| (g - th).abs() < 1e-9) {
                        gen_theta.push(th);
                    }
                }
            }
        }
        let on_generator = |v: u32| {
            let th = proj(v).0;
            gen_theta.iter().any(|g| (g - th).abs() < 1e-9)
        };

        // Unique patch vertices → local 2D pool (θ unwrapped near a reference).
        // Drop cylinder-only generator vertices (collinear-redundant on the seam).
        let mut vset: BTreeSet<u32> = BTreeSet::new();
        for &t in &patch_tris {
            for &v in &mesh.tris[t as usize] {
                if on_generator(v) && !shared.contains(&v) {
                    continue;
                }
                vset.insert(v);
            }
        }
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            for &t in &patch_tris {
                for &v in &mesh.tris[t as usize] {
                    if on_generator(v) {
                        eprintln!(
                            "YANG_RECDT_GENV v={v} shared={} z={:.4}",
                            shared.contains(&v),
                            proj(v).1
                        );
                    }
                }
            }
        }
        let th_ref = proj(*vset.iter().next().unwrap()).0;
        let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
        let mut global_of_local: Vec<u32> = Vec::new();
        let mut local_of_global: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        let (mut th_lo, mut th_hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &vset {
            let (mut th, z) = proj(v);
            th -= th_ref;
            while th > pi {
                th -= 2.0 * pi;
            }
            while th < -pi {
                th += 2.0 * pi;
            }
            th_lo = th_lo.min(th);
            th_hi = th_hi.max(th);
            let l = verts2d.len() as u32;
            local_of_global.insert(v, l);
            global_of_local.push(v);
            verts2d.push(cad_primitives::Point2::new(th, z));
        }
        // Seam-wrap guard: only LOCAL (θ-span < π) patches are in scope.
        if th_hi - th_lo >= pi {
            return Ok(remeshed);
        }

        // TRUE seam boundary via GLOBAL cross-attribution edge sharing (spec
        // §5c.7): an edge of this patch is a boundary edge iff it is NOT shared by
        // exactly two triangles that both carry THIS patch's attribution — i.e. it
        // faces a different-attribution neighbour (the seam) or is a mesh boundary.
        // This takes the neighbour's chain verbatim, so the re-mesh stays exactly
        // conformal, and the zero-area caps' internal edges (both sides this patch)
        // are correctly interior. Collect the seam edges of THIS patch's tris.
        let mykey = (is_a, face);
        let mut seam_edges: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        for &t in &patch_tris {
            let tri = mesh.tris[t as usize];
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (u, v) = (tri[i], tri[j]);
                let e = if u < v { (u, v) } else { (v, u) };
                let inc = &global_edge_attrs[&e];
                let all_mine = inc.len() == 2 && inc.iter().all(|k| *k == Some(mykey));
                if !all_mine {
                    seam_edges.insert(e);
                }
            }
        }
        // NO generator-chain reconstruction. An earlier increment z-reconstructed
        // each generator's seam into the fine z-consecutive chain, on the theory
        // that the neighbour shares those vertices and so uses that chain. That is
        // FALSE for the tangency/pinch configuration (R0038, refuted in §5c.10):
        // when a plane is tangent to the cylinder along a generator, the plane's
        // seam edges are NOT the fine z-consecutive chain — the plane connects
        // 14→18 (skipping 21,23), and verts 18,19 are DEGREE-3 on the seam (a
        // pinch where two boundary strands meet). The conformal cylinder seam there
        // is carried by ZERO-AREA caps, which the re-CDT necessarily drops. So the
        // z-reconstruction produced a seam the neighbour does not have (edge 14→21,
        // fwd=1 rev=0) → a non-manifold output caught only downstream.
        //
        // The verbatim cross-attribution `seam_edges` above IS the neighbour's
        // seam (every seam edge is incident to a patch triangle — cap or real — so
        // the patch-edge scan captures all of them). Use it directly. A genuine
        // simple degenerate-cylinder strip yields a clean degree-2 boundary and
        // re-CDTs. A pinched tangency (R0038) yields degree-3 seam vertices and is
        // rejected by the degree-2 boundary gate below — a clean, self-validating
        // LOUD STOP at the right place, never a downstream non-manifold surprise.
        // Local-index boundary adjacency; each boundary vertex must have exactly
        // two boundary neighbours (manifold boundary) or we bail (loud STOP).
        let mut bnd_adj: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();
        for &(u, v) in &seam_edges {
            let (lu, lv) = (local_of_global[&u], local_of_global[&v]);
            bnd_adj.entry(lu).or_default().push(lv);
            bnd_adj.entry(lv).or_default().push(lu);
        }
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            let bad: Vec<(u32, usize)> = bnd_adj
                .iter()
                .filter(|(_, n)| n.len() != 2)
                .map(|(&v, n)| (global_of_local[v as usize], n.len()))
                .collect();
            eprintln!(
                "YANG_RECDT_SEAM face={face} nverts={} nseam_edges={} nbnd={} bad_degree={:?}",
                verts2d.len(),
                seam_edges.len(),
                bnd_adj.len(),
                bad
            );
            for (&lv, n) in &bnd_adj {
                if n.len() != 2 {
                    let gv = global_of_local[lv as usize];
                    let nbrs: Vec<(u32, f64, f64)> = n
                        .iter()
                        .map(|&ln| {
                            let g = global_of_local[ln as usize];
                            (g, verts2d[ln as usize].x(), verts2d[ln as usize].y())
                        })
                        .collect();
                    eprintln!(
                        "  bad v{gv} (θ,z)={:?} seam_nbrs={nbrs:?}",
                        verts2d[lv as usize]
                    );
                }
            }
        }
        if bnd_adj.is_empty() || bnd_adj.values().any(|n| n.len() != 2) {
            return Err(YangError::Stage4RegionInvalid {
                vertex: u32::MAX,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        // Walk the boundary edges into closed loops.
        let mut loops_local: Vec<Vec<u32>> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for &start in bnd_adj.keys() {
            if seen.contains(&start) {
                continue;
            }
            let mut lp = vec![start];
            seen.insert(start);
            let mut prev = start;
            let mut cur = bnd_adj[&start][0];
            while cur != start {
                if !seen.insert(cur) {
                    // revisited a non-start vertex → tangled boundary, bail.
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: u32::MAX,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                }
                lp.push(cur);
                let nb = &bnd_adj[&cur];
                let next = if nb[0] == prev { nb[1] } else { nb[0] };
                prev = cur;
                cur = next;
            }
            loops_local.push(lp);
        }
        // Fig-11(a): a shared generator/intersection vertex whose ONLY incident
        // triangles were degenerate caps is missing from the non-degenerate
        // boundary above, yet it lies ON a boundary edge and is shared with the
        // neighbouring patch across the intersection curve — it MUST stay on the
        // boundary or the seam tears (non-manifold). Insert every such
        // interior-but-on-a-boundary-edge vertex into the boundary chain (split
        // the constraint edge at it), iterating so multiple collinear inserts on
        // one edge each find their sub-edge.
        loop {
            let on_bnd: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
            let mut inserted = false;
            for vi in 0..verts2d.len() as u32 {
                if on_bnd.contains(&vi) {
                    continue;
                }
                let p = verts2d[vi as usize];
                'find: for lp in &mut loops_local {
                    for i in 0..lp.len() {
                        let a = verts2d[lp[i] as usize];
                        let b = verts2d[lp[(i + 1) % lp.len()] as usize];
                        let ab = (b.x() - a.x(), b.y() - a.y());
                        let ap = (p.x() - a.x(), p.y() - a.y());
                        let cross = ab.0 * ap.1 - ab.1 * ap.0;
                        let len2 = ab.0 * ab.0 + ab.1 * ab.1;
                        let dot = ab.0 * ap.0 + ab.1 * ap.1;
                        // Collinear (area of a-b-p ≈ 0 vs the edge length) AND
                        // strictly between a and b.
                        if len2 > 0.0 && cross.abs() <= 1e-9 * len2 && dot > 0.0 && dot < len2 {
                            lp.insert(i + 1, vi);
                            inserted = true;
                            break 'find;
                        }
                    }
                }
            }
            if !inserted {
                break;
            }
        }
        let signed_area = |lp: &[u32]| -> f64 {
            let mut a = 0.0;
            for i in 0..lp.len() {
                let p = verts2d[lp[i] as usize];
                let q = verts2d[lp[(i + 1) % lp.len()] as usize];
                a += p.x() * q.y() - q.x() * p.y();
            }
            a * 0.5
        };
        let outer_i = (0..loops_local.len())
            .max_by(|&x, &y| {
                signed_area(&loops_local[x])
                    .abs()
                    .partial_cmp(&signed_area(&loops_local[y]).abs())
                    .unwrap()
            })
            .unwrap();
        let outer = loops_local[outer_i].clone();
        let holes: Vec<Vec<u32>> = loops_local
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != outer_i)
            .map(|(_, l)| l.clone())
            .collect();
        let bnd: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
        let interior: Vec<u32> = (0..verts2d.len() as u32)
            .filter(|l| !bnd.contains(l))
            .collect();
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            let interior_g: Vec<u32> = interior
                .iter()
                .map(|&l| global_of_local[l as usize])
                .collect();
            eprintln!(
                "YANG_RECDT face={face} nverts={} nloops={} outer_len={} n_interior={} interior_g={:?}",
                verts2d.len(),
                loops_local.len(),
                loops_local.iter().map(|l| l.len()).max().unwrap_or(0),
                interior.len(),
                interior_g,
            );
        }

        let tris_local =
            cherchi_rs::cdt_polygon_with_holes_keep_interior(&verts2d, &outer, &holes, &interior)
                .map_err(|_| YangError::Stage4RegionInvalid {
                vertex: u32::MAX,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            })?;

        // Reference winding sign: align to the patch's existing non-degenerate
        // triangles (robust to inward/outward cylinder faces).
        let radial_at = |g: [u32; 3]| -> [f64; 3] {
            let c = [
                (mesh.verts[g[0] as usize].x()
                    + mesh.verts[g[1] as usize].x()
                    + mesh.verts[g[2] as usize].x())
                    / 3.0,
                (mesh.verts[g[0] as usize].y()
                    + mesh.verts[g[1] as usize].y()
                    + mesh.verts[g[2] as usize].y())
                    / 3.0,
                (mesh.verts[g[0] as usize].z()
                    + mesh.verts[g[1] as usize].z()
                    + mesh.verts[g[2] as usize].z())
                    / 3.0,
            ];
            let r = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
            let axl = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
            [r[0] - axl * au[0], r[1] - axl * au[1], r[2] - axl * au[2]]
        };
        let mut ref_sign = 0.0_f64;
        for &t in &patch_tris {
            let g = mesh.tris[t as usize];
            if is_degen(g, mesh) {
                continue;
            }
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            let rad = radial_at(g);
            ref_sign += av[0] * rad[0] + av[1] * rad[1] + av[2] * rad[2];
        }
        let ref_sign = if ref_sign >= 0.0 { 1.0 } else { -1.0 };

        for tl in tris_local {
            let mut g = [
                global_of_local[tl[0] as usize],
                global_of_local[tl[1] as usize],
                global_of_local[tl[2] as usize],
            ];
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            let rad = radial_at(g);
            let dot = av[0] * rad[0] + av[1] * rad[1] + av[2] * rad[2];
            if dot * ref_sign < 0.0 {
                g.swap(1, 2);
            }
            new_tris.push(g);
            new_attr.push(Some(at));
        }
        remeshed = true;
    }

    if remeshed {
        mesh.tris = new_tris;
        *attr_vec = new_attr;
    }
    Ok(remeshed)
}

pub(crate) fn collapse_vertex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    victim: u32,
    survivor: u32,
) -> usize {
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attribution.len());
    let mut dropped = 0usize;
    for (t, tri) in mesh.tris.iter().enumerate() {
        let mapped = [
            if tri[0] == victim { survivor } else { tri[0] },
            if tri[1] == victim { survivor } else { tri[1] },
            if tri[2] == victim { survivor } else { tri[2] },
        ];
        if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[2] == mapped[0] {
            dropped += 1;
            continue;
        }
        new_tris.push(mapped);
        new_attr.push(attribution.get(t).copied().flatten());
    }
    // Membrane cancellation (spec `yang_collapse_membrane_cancellation`):
    // identifying `victim` with `survivor` can turn the two-triangle pleat
    // that spanned the twin gap into an EXACT duplicate pair with OPPOSITE
    // windings — a zero-volume doubled flap whose 6 directed edges are 3
    // mutual-reverse pairs (they pair with EACH OTHER). Dropping BOTH
    // preserves the watertight half-edge pairing and restores manifold
    // count-2 on the shared fan edges (the measured F0059 mint: the PR-KV9
    // junction-twin collapse at the Steinmetz seam apex derailed the Stage-6
    // wedge walk). Same-winding duplicates and ≥3-copy groups are genuine
    // non-manifold configurations — left untouched for the downstream loud
    // STOPs (P9: never silently pick).
    {
        let mut by_triple: std::collections::HashMap<[u32; 3], Vec<usize>> =
            std::collections::HashMap::new();
        for (t, tri) in new_tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
        }
        // Cyclic-winding key: rotate the smallest index to the front; equal
        // keys ⇔ same winding.
        let winding_key = |tri: [u32; 3]| -> [u32; 3] {
            let k = (0..3).min_by_key(|&i| tri[i]).expect("3 verts");
            [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]]
        };
        let mut cancel: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for ts in by_triple.values() {
            if ts.len() != 2 {
                continue;
            }
            let (x, y) = (ts[0], ts[1]);
            if winding_key(new_tris[x]) != winding_key(new_tris[y]) {
                cancel.insert(x);
                cancel.insert(y);
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[membrane-cancel] dropping opposite-winding dup pair tris {x},{y} = \
                         {:?}/{:?} (victim={victim} survivor={survivor})",
                        new_tris[x], new_tris[y]
                    );
                }
            }
        }
        if !cancel.is_empty() {
            let keep: Vec<usize> = (0..new_tris.len())
                .filter(|t| !cancel.contains(t))
                .collect();
            new_tris = keep.iter().map(|&t| new_tris[t]).collect();
            new_attr = keep.iter().map(|&t| new_attr[t]).collect();
            dropped += cancel.len();
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attribution = new_attr;
    // EXPERIMENTAL probe (task #121, read-only, env-gated): did THIS collapse
    // mint a duplicate (double-cover) triangle pair?
    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
        use std::collections::HashMap;
        let mut by_triple: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
        for (t, tri) in mesh.tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
        }
        for (key, ts) in &by_triple {
            if ts.len() > 1 {
                eprintln!(
                    "[doublecover-collapse] victim={victim} survivor={survivor} \
                     dup triple {key:?} tris {ts:?} windings {:?}",
                    ts.iter().map(|&t| mesh.tris[t]).collect::<Vec<_>>()
                );
            }
        }
    }
    dropped
}

/// N47 (spec `yang_n47_coincident_moved_weld`): weld coincident RELOCATED
/// vertices before topology emission.
///
/// Two vertices this pipeline pushed onto an analytic curve (`moved`) can
/// Newton-converge to within the MODEL coincidence tolerance
/// `TAU_MODEL·(1+scale)` — they are the SAME geometric point emitted twice (a
/// near-tangent seam crossing whose two arrangement vertices both project onto
/// one intersection point). Emitted distinct, they become a sub-render-precision
/// output edge that trips kernel-v2's G1 render-collapse gate far downstream.
///
/// Band: the scale-relative model coincidence tolerance (`scale` = max |coord| of
/// the pair) — the SAME band every other coincidence test uses, 10× tighter than
/// the `MIN_FEATURE_SIZE·(1+scale)` feature floor, so it admits ONLY
/// sub-(feature/10) coincidences. Restricted to `moved`×`moved` pairs: it never
/// touches un-relocated arrangement geometry `boolean()` kept for watertightness
/// (cf. the §4.4.1(b) micro-scale R0091 revert — P9/P10). `collapse_vertex` is the
/// proven watertight-preserving edge-collapse; iterate to a fixed point over live
/// (still-referenced) vertices. Returns whether any pair welded.
pub(crate) fn weld_coincident_relocated(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
) -> bool {
    let mut welded = false;
    loop {
        // Live moved verts (still referenced by some triangle), ascending.
        let mut live: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for &v in tri {
                if moved.contains(&v) {
                    live.insert(v);
                }
            }
        }
        let live: Vec<u32> = live.into_iter().collect();
        let mut pair: Option<(u32, u32)> = None;
        'scan: for (i, &u) in live.iter().enumerate() {
            let pu = mesh.verts[u as usize].as_array();
            for &w in &live[i + 1..] {
                let pw = mesh.verts[w as usize].as_array();
                let d =
                    ((pu[0] - pw[0]).powi(2) + (pu[1] - pw[1]).powi(2) + (pu[2] - pw[2]).powi(2))
                        .sqrt();
                let scale = pu[0]
                    .abs()
                    .max(pu[1].abs())
                    .max(pu[2].abs())
                    .max(pw[0].abs())
                    .max(pw[1].abs())
                    .max(pw[2].abs());
                let band = cad_primitives::TAU_MODEL * (1.0 + scale);
                if d < band {
                    // survivor = lower index (matches every other collapse's
                    // survivor rule — both are already exact on their curve, so
                    // no exactness ranking is needed).
                    pair = Some((w, u)); // (victim = higher, survivor = lower)
                    break 'scan;
                }
            }
        }
        match pair {
            Some((victim, survivor)) => {
                if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
                    eprintln!(
                        "[moved-weld] victim={victim} survivor={survivor} p={:?}",
                        mesh.verts[survivor as usize]
                    );
                }
                collapse_vertex(mesh, attribution, victim, survivor);
                welded = true;
            }
            None => break,
        }
    }
    welded
}

/// N50 (spec `yang_n50_f32_render_twin_weld`, deviation N50): collapse two
/// DISTINCT output vertices that are **bitwise-identical after rounding to
/// f32** — the exact G1 render-collapse criterion (kernel-v2
/// `f32_render_degenerate`, B2 clause). This is the 3D, output-magnitude
/// completion of N47's `weld_coincident_relocated`:
///
/// - N47 reaches only `moved`×`moved` relocated pairs; the R0012/R0098 twins are
///   NON-relocated Cherchi arrangement vertices minted by near-coincident
///   Stage-0 overlay sweep-event columns (N48/N49). After the FINAL Stage-4
///   relocation onto the exact curves the pair converges to within f32 render
///   precision at the OUTPUT (world) magnitude, surviving every earlier merge and
///   tripping G1 downstream (`planar triangle collapsed at render precision`).
/// - The criterion is the f32 **bit-key** `[(x as f32).to_bits(), …]`, not a
///   model band. Two vertices that round to the same f32 bits are the same
///   rendered point — collapsing them is render-invariant. The key is LOCAL by
///   construction (f32 ulp ≈ `|coord|·2⁻²³`), so it never over-merges a
///   near-origin pair in a far-flung model the way a global-`scale` `TAU_MODEL`
///   band does (the refuted N49 approach). Grouping by exact f32 cell is an
///   equivalence relation, so the weld never single-linkages across distinct
///   render cells (the N49 fault-1 / F0090 rim-drop hazard).
///
/// Runs on the FINAL mesh (after Stage-4 relocation and the KV15b collapse,
/// immediately before `emit_topology`, whose output vertices are 1:1 with
/// `mesh.verts`). `collapse_vertex` is the proven watertight-preserving
/// edge-collapse; iterate to a fixed point (one pair per BTreeMap-ordered sweep,
/// min-index survivor). Byte-identical no-op when no two live verts share an f32
/// cell (the overwhelming-majority fast path). Returns whether any pair welded.
pub(crate) fn weld_f32_render_twins(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
) -> bool {
    let f32_key = |p: &Point3| -> [u32; 3] {
        let a = p.as_array();
        [
            (a[0] as f32).to_bits(),
            (a[1] as f32).to_bits(),
            (a[2] as f32).to_bits(),
        ]
    };
    let mut welded = false;
    loop {
        // Group live (still triangle-referenced) verts by f32 render cell.
        let mut buckets: std::collections::BTreeMap<[u32; 3], std::collections::BTreeSet<u32>> =
            std::collections::BTreeMap::new();
        for tri in &mesh.tris {
            for &v in tri {
                buckets
                    .entry(f32_key(&mesh.verts[v as usize]))
                    .or_default()
                    .insert(v);
            }
        }
        // First cell (deterministic key order) holding two distinct verts.
        let pair = buckets.values().find(|g| g.len() > 1).map(|g| {
            let mut it = g.iter();
            let survivor = *it.next().expect("len > 1"); // min index (BTreeSet)
            let victim = *it.next().expect("len > 1");
            (victim, survivor)
        });
        match pair {
            Some((victim, survivor)) => {
                if std::env::var_os("YANG_F32_WELD_PROBE").is_some() {
                    eprintln!(
                        "[f32-weld] victim={victim} survivor={survivor} p={:?}",
                        mesh.verts[survivor as usize]
                    );
                }
                collapse_vertex(mesh, attribution, victim, survivor);
                welded = true;
            }
            None => break,
        }
    }
    welded
}

/// KV15b (spec `kv15b_mint_site_subresolution_collapse`): collapse
/// sub-resolution intersection segments before Phase-B emission.
///
/// The exact arrangement legitimately mints two crossings of near-parallel
/// geometry closer than the model tolerance (R0076: gear flank grazing a box
/// edge, 3.999e-8 / 6.472e-8 pairs). Emitted as two distinct output vertices,
/// the pair is POISON downstream: the Stage-0 coplanar clustering band floor
/// is exactly `TAU_MODEL`, and Stage-6 patch walks of the next boolean
/// disagree over the twin (the measured F0070/KV15 mechanism at sub-floor
/// scale). Per A8.1/A14 `TAU_MODEL` is the single central vertex-merge
/// resolution — two points closer than it ARE one model point — so emission
/// hygiene collapses the segment at the mint site.
///
/// Eligibility is FULL-PROVENANCE (I3): only consecutive intersection-curve
/// vertices — keys of `intersection_curves` — are candidates; inherited
/// operand geometry (e.g. legitimately sub-floor micro-profile corners) is
/// never touched. This is one order TIGHTER than the reverted-R0091
/// `MIN_FEATURE_SIZE` global widening and scoped to the increment-4
/// provenance pattern. One sweep over the ORIGINAL segment set in
/// deterministic `BTreeMap` order; endpoints resolve through prior collapses
/// (min-index survivor, I1 — the survivor keeps its own exact coordinates,
/// never an average), and a segment whose RESOLVED length is ≥ `TAU_MODEL`
/// stays (I2/B5 — no chain drift). Exact-zero pairs are the M-B
/// emission-identification class and stay untouched here (B3).
pub(crate) fn collapse_subresolution_intersection_segments(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
) -> bool {
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    // C0036 amendment (spec `kv15b_mint_site_subresolution_collapse` I1b):
    // the surviving POSITION is the pair's plane-incidence-richer endpoint.
    // A sub-floor pair often joins the TRUE junction of k carried planes
    // with a near-degenerate crossing OFF one of them by the sub-floor gap
    // (the C0036 near-coplanar seam corner: the exact 3-plane corner vs a
    // crossing 1.75e-8 off the tilted wall). Keeping the min-index position
    // blindly evicts a face-loop vertex off its carried analytic plane,
    // twisting the loop (the fitted Newell then misses the exact input
    // corners — the debug-tier NonPlanarFace red). The topological survivor
    // stays min-index (I1 determinism); only its COORDINATES may adopt the
    // strictly richer endpoint. Ties keep the survivor's own coordinates
    // (byte-identical to the shipped behavior).
    let plane_count = |mesh: &Mesh,
                       attribution: &[Option<TriangleAttribution>],
                       vi: u32,
                       pos: [f64; 3]|
     -> usize {
        let mut seen: Vec<[u64; 4]> = Vec::new();
        let band =
            cad_primitives::TAU_WORK * (1.0 + pos[0].abs().max(pos[1].abs()).max(pos[2].abs()));
        for (t, tri) in mesh.tris.iter().enumerate() {
            if !tri.contains(&vi) {
                continue;
            }
            let Some(att) = attribution.get(t).copied().flatten() else {
                continue;
            };
            let faces = match att.input {
                InputId::A => a.faces(),
                InputId::B => b.faces(),
            };
            let Some(face) = faces.get(att.face as usize) else {
                continue;
            };
            let Surface::Plane { normal, d } = face.surface else {
                continue;
            };
            let n = normal.as_array();
            if (n[0] * pos[0] + n[1] * pos[1] + n[2] * pos[2] + d).abs() > band {
                continue;
            }
            let key = [n[0].to_bits(), n[1].to_bits(), n[2].to_bits(), d.to_bits()];
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
        seen.len()
    };
    let tau2 = cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL;
    let mut any = false;
    for &(u, v) in intersection_curves.keys() {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        if d2 == 0.0 || d2 >= tau2 {
            continue;
        }
        let survivor = ru.min(rv);
        let victim = ru.max(rv);
        // I1b: adopt the plane-incidence-richer position onto the surviving
        // index (strictly richer only).
        {
            let sp = mesh.verts[survivor as usize].as_array();
            let vp = mesh.verts[victim as usize].as_array();
            let cs = plane_count(mesh, attribution, survivor, sp);
            let cv = plane_count(mesh, attribution, victim, vp);
            if cv > cs {
                mesh.verts[survivor as usize] = mesh.verts[victim as usize];
            }
        }
        if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
            eprintln!(
                "[collapse-site] kv15b victim={victim} p=({:.17e},{:.17e},{:.17e}) \
                 survivor={survivor} q=({:.17e},{:.17e},{:.17e}) dist={:.3e}",
                mesh.verts[victim as usize].as_array()[0],
                mesh.verts[victim as usize].as_array()[1],
                mesh.verts[victim as usize].as_array()[2],
                mesh.verts[survivor as usize].as_array()[0],
                mesh.verts[survivor as usize].as_array()[1],
                mesh.verts[survivor as usize].as_array()[2],
                d2.sqrt(),
            );
        }
        collapse_vertex(mesh, attribution, victim, survivor);
        redirect.insert(victim, survivor);
        any = true;
    }
    any
}

/// PR-YR11 helper: drop mesh vertices no surviving triangle references and remap
/// triangle indices + the Stage-4 `relocations` keys to the dense vertex set.
///
/// A §4.5.3 [`collapse_vertex`] keeps the full vertex array (it only drops the
/// now-degenerate triangles), leaving the collapsed-away vertices DANGLING. The
/// internal per-shell `check_watertight_2manifold` gate ignores them (it sums V
/// over triangle-referenced verts only), but they inflate a caller's GLOBAL
/// `V − E + F`. An output mesh must carry no unreferenced vertices, so this
/// compaction runs after Stage 4. It is a strict NO-OP (returns early, mesh and
/// `relocations` untouched) when every vertex is already referenced — so the
/// no-collapse paths (planar / perpendicular-circle / on-curve mock) stay
/// byte-identical.
pub(crate) fn compact_unreferenced_verts(mesh: &mut Mesh, relocations: &mut Vec<(u32, f64)>) {
    let n = mesh.verts.len();
    let mut referenced = vec![false; n];
    for tri in &mesh.tris {
        for &v in tri {
            referenced[v as usize] = true;
        }
    }
    if referenced.iter().all(|&r| r) {
        return; // no danglers — byte-identical no-op.
    }
    // Dense remap preserving the relative order of surviving vertices.
    let mut remap: Vec<Option<u32>> = vec![None; n];
    let mut new_verts: Vec<Point3> = Vec::with_capacity(n);
    for (i, &r) in referenced.iter().enumerate() {
        if r {
            remap[i] = Some(new_verts.len() as u32);
            new_verts.push(mesh.verts[i]);
        }
    }
    let new_tris: Vec<[u32; 3]> = mesh
        .tris
        .iter()
        .map(|tri| {
            // Invariant: `referenced` was built from this same triangle list
            // above, so every triangle vertex has a `Some` remap entry.
            tri.map(|v| {
                remap[v as usize]
                    .expect("compact_unreferenced_verts: triangle vertex not marked referenced")
            })
        })
        .collect();
    *mesh = Mesh::new(new_verts, new_tris);
    // Remap (and drop) relocation keys: a relocation referencing a collapsed-away
    // (now-unreferenced) vertex is no longer in the mesh, so it is dropped.
    let remapped: Vec<(u32, f64)> = relocations
        .iter()
        .filter_map(|&(v, t)| remap[v as usize].map(|nv| (nv, t)))
        .collect();
    *relocations = remapped;
}

/// PR-YR10 (Yang §4.4.1 + §4.5.3): Stage 4 — relocate the mesh intersection
/// points onto the exact analytical `Circle` curves, then correct any reversed
/// intersection points by the §4.5.3 polyline-tangent sweep.
///
/// Returns `(relocations, collapsed)` where `relocations` is the list of
/// `(vertex, t)` pairs (the circle-frame angle `t` for every relocated OR
/// already-on-curve intersection vertex — the caller maps these to
/// `BRepEdge { edge, t }` tessellation sources once the output edges exist), and
/// `collapsed` is `true` iff the §4.5.3 sweep edge-collapsed at least one
/// vertex (so the caller must recompute Phase A).
///
/// LOUD STOPs (P9/P10), never a silent snap / tolerance widening / no-op:
/// - `Stage4RegionInvalid { OnAxis }` — a point projects onto the circle/cylinder
///   axis.
/// - `Stage4RegionInvalid { OffCurveBeyondChordBand }` — residual `ρ > d_ε`.
/// - `Stage4RegionInvalid { LoopTooSmall }` — a loop shrank below 3 verts.
/// - `Stage4RegionInvalid { InvertedTriangle / DegenerateTriangle }` — a
///   relocated triangle is inverted / degenerate after correction.
/// - `Stage4ReversalUnresolved` — the §4.5.3 sweep could not resolve a reversal.
/// - `Stage4RegionInvalid { LocalRefinementRequired }` — relocate + §4.5.3 left
///   a region invalid (genuine §4.5.2 territory, out of scope).
///
/// No-skip audit (anti-disproven-attempt): a `processed` set tracks EVERY conic
/// edge endpoint; it must equal the relocation-key set at the end. The function
/// NEVER `continue`s past a `Circle` edge endpoint.
pub(crate) fn stage4_relocate_and_correct(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    use std::collections::{BTreeMap, HashSet};

    // Non-shadowed aliases for the input BReps (the loops below rebind `a`/`b`
    // to per-triangle vertex indices, so diagnostics/lookups that need the BReps
    // use these).
    let (brep_a, brep_b) = (a, b);

    // d_ε relocation budget (a conic edge implies a curved input ⇒ Some).
    let d_eps = match stage4_chord_band(a, b) {
        Some(de) => de,
        None => {
            // A conic edge with no circle-bearing input is a producer fault;
            // never default to TAU_WORK for a curved relocation (P10).
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                eprintln!("YANG_LRR_STOP site=chord_band_none");
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: u32::MAX,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    };

    // (1) Collect + classify every conic-edge endpoint from the CURRENT Phase A.
    // PR-YR11: the incidence map (no longer discarded) supplies the TRUE cylinder
    // + cutting plane per Ellipse edge for the closed-form cylinder relocation.
    let (_infos0, inc0, curves0) = compute_phase_a(mesh, attribution, a, b)?;

    // Per-vertex Circle assignment (deterministic via BTreeMap). PR-YR19: the
    // 4th tuple element carries the originating sphere radius `Some(R)` for a
    // sphere section circle (else `None`) so the relocation guard can scale the
    // in-plane radial band by `(R/r_c)` (spec §2/§4 Site 2).
    let mut vert_circle: BTreeMap<u32, (Point3, Vector3, f64, Option<f64>)> = BTreeMap::new();
    // PR-YR11: per-vertex Ellipse relocation data (the true cylinder + plane +
    // stored ellipse), analogous to `vert_circle`.
    let mut vert_ellipse: BTreeMap<u32, EllipseReloc> = BTreeMap::new();
    // PR-YR21: per-vertex cone-ellipse relocation data (the true cone + plane +
    // stored ellipse + the cone's OWN chord budget), for a `cone ∩ plane`
    // oblique section. Kept separate from `vert_ellipse` (cylinder) so the
    // cylinder path stays byte-identical.
    let mut vert_cone_ellipse: BTreeMap<u32, ConeEllipseReloc> = BTreeMap::new();
    // PR-YR22: per-vertex cone-parabola relocation data for a `cone ∩ plane` θ=α
    // (generator-parallel) section. Kept separate from the ellipse maps so the
    // ellipse/cylinder paths stay byte-identical.
    let mut vert_parabola: BTreeMap<u32, ConeParabolaReloc> = BTreeMap::new();
    // PR-YR23: per-vertex cone-hyperbola relocation data for a `cone ∩ plane`
    // axis-parallel (HYPE) section. Kept separate from the other conic maps so
    // the ellipse/cylinder/parabola paths stay byte-identical.
    let mut vert_cone_hyperbola: BTreeMap<u32, ConeHyperbolaReloc> = BTreeMap::new();
    // KV16 (spec `kv16_hyperbola_arc_vocabulary`): a vertex receiving TWO
    // DIFFERENT cone-hyperbola descriptors (the prism-edge × cone-lateral
    // pierce — same cone, two steep planes, BOTH sections hyperbolas; R0017
    // v47) collapses into the ONE map above, so the increment-5 "≥2 maps"
    // trigger cannot see the junction and the vertex would be relocated
    // onto only one curve (an off-branch endpoint on the other's output
    // edge). Detected at insert time — the vert_ell_junction precedent —
    // and force-fed to the triple-junction relocation below.
    let mut same_type_junction: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // PR-F3: per-vertex ruling-LINE relocation data for a plane∥axis ×
    // cylinder intersection edge (ssi C3a/C3b). A `Curve::LineSegment`
    // intersection edge whose incidence carries a CYLINDER is such a line; its
    // arrangement points sit on Stage-1 facet chords, off the exact line (and
    // off the cylinder) by up to the sagitta — they need relocation exactly
    // like the conic arms. Plane∩plane segments are exact and stay skipped.
    let mut vert_line: BTreeMap<u32, LineReloc> = BTreeMap::new();
    // M5 (Y4): per-vertex procedural surface-pair relocation data — the TWO
    // defining surfaces of a `Curve::SurfacePair` edge, carried on the curve
    // itself (no incidence scan needed). Each endpoint is Newton-projected
    // onto BOTH surfaces (`relocate_onto_implicit_pair`), the analog of the
    // torus implicit-pair block but with the pair supplied directly.
    let mut vert_surface_pair: BTreeMap<u32, (Surface, Surface)> = BTreeMap::new();
    // PR-KV9: a vertex shared by TWO DIFFERENT ellipse edges (the crossing
    // points of the Steinmetz cyl×cyl pair) must land on BOTH curves — the
    // exact junction is `(plane₁ ∩ plane₂) line ∩ cylinder`. Detected at
    // insert time (a silent overwrite would relocate one ellipse's endpoint
    // onto the other, collapsing the seam).
    let mut vert_ell_junction: BTreeMap<u32, (EllipseReloc, EllipseReloc)> = BTreeMap::new();
    // M8 disc∩disc CROSSING: a vertex shared by TWO DIFFERENT coplanar CIRCLE
    // edges (the lens corners of two overlapping coplanar cap rims) must land on
    // BOTH circles — the exact junction is the closed-form circle∩circle
    // intersection in their shared plane. Detected at insert time (a silent
    // overwrite would relocate it onto only the last-scanned circle, leaving the
    // other arc's endpoint off-circle by the lens displacement — the kernel-v2
    // "output arc endpoint does not lie on its circle" reject). The circle analog
    // of `vert_ell_junction`.
    let mut vert_circle_junction: BTreeMap<u32, (CircleAssign, CircleAssign)> = BTreeMap::new();
    // PR-KV11: per-vertex plane∩plane intersection-LINE incidences. The pp
    // segments themselves are exact (skipped), but their ENDPOINT on a
    // chordized curved lateral is a TRIPLE point (e.g. capA∩faceB line ×
    // lateral ellipse): the arrangement vertex lies exactly ON the line but
    // only chord-close to the cylinder, so relocating it onto the conic
    // alone slides it OFF the line (off the cap plane — the F0046 Newell
    // disagreement). Collected here; resolved into `vert_ell_junction`
    // after the scan (the junction is `(plane ∩ plane) ∩ cylinder`, the
    // same closed form as the ellipse×ellipse box-edge junction).
    let mut vert_pp_planes: BTreeMap<u32, Vec<(Vector3, f64, Vector3, f64)>> = BTreeMap::new();
    // PR-KV11: junction-aware insertion, shared by BOTH ellipse arms
    // (cylinder+plane AND cylinder×cylinder). A vertex already assigned a
    // DIFFERENT ellipse (the box-edge crossing of two cylinder∩plane
    // sections, or the Steinmetz cyl×cyl crossing) is demoted to the
    // junction map; a silent overwrite would relocate it onto only the
    // last-scanned ellipse, leaving it off the first by the Stage-1 chord
    // error (the F0046-class "endpoint does not lie on its ellipse").
    fn insert_ellipse_or_junction(
        v: u32,
        er: EllipseReloc,
        vert_ellipse: &mut BTreeMap<u32, EllipseReloc>,
        vert_ell_junction: &mut BTreeMap<u32, (EllipseReloc, EllipseReloc)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Ok(list) = std::env::var("YANG_V_PROBE") {
            if list.split(',').any(|t| t.trim().parse::<u32>() == Ok(v)) {
                eprintln!(
                    "YANG_V_PROBE insert_ellipse v={v} plane_n={:?} plane_d={:.17e} center={:?}",
                    er.plane_n, er.plane_d, er.center,
                );
            }
        }
        if let Some(prev) = vert_ellipse.get(&v).copied() {
            let same = prev.plane_d == er.plane_d
                && prev.plane_n.as_array() == er.plane_n.as_array()
                && prev.center.as_array() == er.center.as_array();
            if !same {
                vert_ellipse.remove(&v);
                vert_ell_junction.insert(v, (prev, er));
                endpoints.push(v);
                return;
            }
        } else if vert_ell_junction.contains_key(&v) {
            // Already a junction of two ellipses; a third co-incident
            // section adds no relocation freedom (the junction point is
            // fully determined by line ∩ cylinder).
            endpoints.push(v);
            return;
        }
        vert_ellipse.insert(v, er);
        endpoints.push(v);
    }
    // M8 disc∩disc: insert a CIRCLE assignment, demoting to `vert_circle_junction`
    // when the vertex already carries a DIFFERENT circle (the lens corner of two
    // coplanar cap rims). Mirrors `insert_ellipse_or_junction`.
    fn insert_circle_or_junction(
        v: u32,
        ca: CircleAssign,
        vert_circle: &mut BTreeMap<u32, CircleAssign>,
        vert_circle_junction: &mut BTreeMap<u32, (CircleAssign, CircleAssign)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Some(prev) = vert_circle.get(&v).copied() {
            // Same circle (two arcs of ONE split circle meet here) → keep single.
            let same = prev.0.as_array() == ca.0.as_array()
                && prev.1.as_array() == ca.1.as_array()
                && prev.2 == ca.2;
            if !same {
                vert_circle.remove(&v);
                vert_circle_junction.insert(v, (prev, ca));
                endpoints.push(v);
                return;
            }
        } else if vert_circle_junction.contains_key(&v) {
            // Already a circle∩circle junction; a third co-incident circle adds
            // no relocation freedom (the junction is fully determined by the
            // first two), so don't overwrite — just keep it an endpoint.
            endpoints.push(v);
            return;
        }
        vert_circle.insert(v, ca);
        endpoints.push(v);
    }
    let mut endpoints: Vec<u32> = Vec::new();
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        let probed: Vec<u32> = list
            .split(',')
            .filter_map(|t| t.trim().parse::<u32>().ok())
            .collect();
        for (&(s, e), curve) in &curves0 {
            if probed.contains(&s) || probed.contains(&e) {
                eprintln!("YANG_V_PROBE curves0 edge ({s},{e}) curve={curve:?}");
            }
        }
    }
    // Increment 3 (spec `yang_rim_junction_insertion` §Failure modes):
    // PRE-SCAN EXACTNESS CERTIFICATE for over-determined junction vertices.
    // A vertex whose incidence (inc0) carries ≥3 DISTINCT surfaces and whose
    // position is ALREADY within TAU_WORK of EVERY one of them is a fully
    // determined junction point that needs no relocation — the Stage-1 rim
    // junction insertion (increment 2) mints exactly this population (the
    // truncated-Steinmetz lobe corners, on 4 surfaces bit-exactly). Today
    // such a vertex trips one of the scan's insert-time junction detectors
    // (e.g. the line∩line "out of scope" STOP) or the post-scan
    // over-determined audits. Certified vertices are skipped by EVERY map
    // insertion below: they enter no conic map, no junction map, and no
    // `endpoints`, so every detector, audit, and relocation loop is
    // unchanged for all other vertices (the no-skip audit stays balanced).
    // `surface_value_and_normal`'s F is a signed DISTANCE (shared with
    // `signed_distance_to_surface`), so this is a genuine exactness
    // certificate — never a silent pick (P9): anything inexact keeps
    // today's loud walls. Ordinary 2-surface curve vertices are NOT
    // certified (they keep their retag/`t` bookkeeping).
    // Per-vertex DISTINCT incident surfaces (inc0 dedup) — shared by the
    // increment-3 exactness certificate below and the increment-5 conic
    // triple-junction relocation (spec `yang_stage4_conic_triple_junction`).
    let vert_surfs: BTreeMap<u32, Vec<Surface>> = {
        let mut vert_surfs: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            for v in [s, e] {
                let list = vert_surfs.entry(v).or_default();
                for &(_input, surf) in entries {
                    if !list.contains(&surf) {
                        list.push(surf);
                    }
                }
            }
        }
        vert_surfs
    };
    let exact_junctions: HashSet<u32> = {
        let mut set = HashSet::new();
        for (&v, surfs) in &vert_surfs {
            if surfs.len() < 3 {
                continue;
            }
            let p = mesh.verts[v as usize].as_array();
            // Increment 4 §4d: scale-aware band (was the absolute
            // TAU_WORK, ~2 ULP at coordinate magnitude 4000 — see
            // `junction_certificate_band`).
            let exact_on_all = surfs.iter().all(|&s| {
                surface_value_and_normal(s, p)
                    .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(p, s))
            });
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-exact-junction] v={v} surfs={} exact={exact_on_all} p={:?}",
                    surfs.len(),
                    p,
                );
                for &s in surfs {
                    let f = surface_value_and_normal(s, p).map(|(f, _)| f);
                    eprintln!("[s4-exact-junction]   v={v} f={f:?} surf={s:?}");
                }
            }
            if exact_on_all {
                set.insert(v);
            }
        }
        set
    };

    for (&(s, e), curve) in &curves0 {
        match *curve {
            Curve::Parabola {
                vertex,
                normal,
                axis_dir,
                focal_length: _, // recovered from the output edge in eval_source.
            } => {
                // PR-YR22: identify the TRUE cone + cutting plane from this edge's
                // incidence (the θ=α generator-parallel section), mirroring the
                // cone-ellipse arm. Carry the cone's owning `InputId` so its chord
                // budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A parabola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-ellipse `_ =>` arm.
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let cpr = ConeParabolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    vertex,
                    normal,
                    para_axis_dir: axis_dir,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    vert_parabola.insert(v, cpr);
                    endpoints.push(v);
                }
            }
            Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse: _, // recovered from the output edge in eval_source.
                semi_conjugate,
            } => {
                // PR-YR23: identify the TRUE cone + cutting plane from this edge's
                // incidence (the axis-parallel HYPE section), mirroring the
                // cone-parabola arm. Carry the cone's owning `InputId` so its
                // chord budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A hyperbola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-parabola arm.
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let chr = ConeHyperbolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    center,
                    normal,
                    major_axis,
                    semi_conjugate,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // KV16: a SECOND, DIFFERENT descriptor for the same
                    // vertex is a same-type conic junction (two hyperbolas
                    // meeting) — never silently overwrite-and-relocate onto
                    // one curve; route to the triple-junction pass.
                    if let Some(prev) = vert_cone_hyperbola.get(&v) {
                        let differs = prev.apex != chr.apex
                            || prev.cone_axis_dir != chr.cone_axis_dir
                            || prev.half_angle != chr.half_angle
                            || prev.plane_n != chr.plane_n
                            || prev.plane_d != chr.plane_d;
                        if differs {
                            same_type_junction.insert(v);
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                let pv = mesh.verts[v as usize].as_array();
                                eprintln!(
                                    "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) hyperbola \
                                     junction: prev apex={:?} ha={:.6} plane_n={:?} d={:.6} \
                                     -> new apex={:?} ha={:.6} plane_n={:?} d={:.6}",
                                    pv[0],
                                    pv[1],
                                    pv[2],
                                    prev.apex,
                                    prev.half_angle,
                                    prev.plane_n,
                                    prev.plane_d,
                                    chr.apex,
                                    chr.half_angle,
                                    chr.plane_n,
                                    chr.plane_d,
                                );
                            }
                        }
                    }
                    vert_cone_hyperbola.insert(v, chr);
                    endpoints.push(v);
                }
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                // PR-YR19: scan this edge's incidence for a `Surface::Sphere`
                // owner → `Some(R)`; else `None`. Uses the SAME canonical key as
                // the Ellipse arm below.
                let key = if s < e { (s, e) } else { (e, s) };
                let mut source_radius: Option<f64> = None;
                if let Some(entries) = inc0.get(&key) {
                    for &(_input, surf) in entries {
                        if let Surface::Sphere { radius: sr, .. } = surf {
                            source_radius = Some(sr);
                        }
                    }
                }
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    insert_circle_or_junction(
                        v,
                        (center, normal, radius, source_radius),
                        &mut vert_circle,
                        &mut vert_circle_junction,
                        &mut endpoints,
                    );
                }
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                // PR-YR11: identify the TRUE cylinder + cutting plane from this
                // edge's incidence (the two incident surfaces of DIFFERENT
                // inputs). A conic Ellipse edge is, by construction, one cylinder
                // lateral + one cutting plane.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cyl: Option<(Point3, Vector3, f64)> = None;
                // PR-KV9: ALL cylinder entries with their owning inputs —
                // a cylinder×cylinder ellipse needs both for the per-point
                // gradient band + the combined chord budget.
                let mut cyls: Vec<(InputId, Point3, Vector3, f64)> = Vec::new();
                let mut plane: Option<(Vector3, f64)> = None;
                // PR-YR21: additionally scan for a `Surface::Cone` owner (the
                // cone+plane oblique section). Carry the owning `InputId` so the
                // cone's chord budget can be derived from its rim Circle.
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                            } => {
                                cyl = Some((axis_point, axis_dir, radius));
                                cyls.push((input, axis_point, axis_dir, radius));
                            }
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                            } => cone = Some((input, apex, axis_dir, half_angle)),
                            _ => {}
                        }
                    }
                }
                match (cyl, cone, plane) {
                    // YR11 cylinder + plane: the EXISTING path, byte-for-byte.
                    (Some((axis_point, axis_dir, radius)), _, Some((plane_n, plane_d))) => {
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: None,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // PR-YR21 cone + plane (no cylinder): the new cone-ellipse
                    // path. Derive the cone's OWN chord budget from the cone
                    // owner's rim Circle (spec §3.3); a cone owner with no rim
                    // Circle is a producer fault → loud STOP (never TAU_WORK).
                    (
                        None,
                        Some((cone_input, apex, axis_dir, half_angle)),
                        Some((plane_n, plane_d)),
                    ) => {
                        let owner = match cone_input {
                            InputId::A => a,
                            InputId::B => b,
                        };
                        let Some(cone_d_eps) =
                            cone_chord_budget_from_owner(apex, axis_dir, half_angle, owner)
                        else {
                            return Err(YangError::Stage4RegionInvalid {
                                vertex: s,
                                reason: Stage4InvalidReason::LocalRefinementRequired,
                            });
                        };
                        let cer = ConeEllipseReloc {
                            apex,
                            axis_dir,
                            half_angle,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            cone_d_eps,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            // KV16b (spec `kv16b_cone_ellipse_same_type_junction`):
                            // a SECOND, DIFFERENT descriptor for the same vertex
                            // is a same-type conic junction (two cone-ellipses
                            // meeting) — never silently overwrite-and-relocate
                            // onto one curve; route to the triple-junction pass
                            // (the KV16 hyperbola recipe, sibling map).
                            if let Some(prev) = vert_cone_ellipse.get(&v) {
                                let differs = prev.apex != cer.apex
                                    || prev.axis_dir != cer.axis_dir
                                    || prev.half_angle != cer.half_angle
                                    || prev.plane_n != cer.plane_n
                                    || prev.plane_d != cer.plane_d;
                                if differs {
                                    same_type_junction.insert(v);
                                    if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                        let pv = mesh.verts[v as usize].as_array();
                                        eprintln!(
                                            "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) \
                                             cone-ellipse junction: prev apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6} -> new apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6}",
                                            pv[0],
                                            pv[1],
                                            pv[2],
                                            prev.apex,
                                            prev.half_angle,
                                            prev.plane_n,
                                            prev.plane_d,
                                            cer.apex,
                                            cer.half_angle,
                                            cer.plane_n,
                                            cer.plane_d,
                                        );
                                    }
                                }
                            }
                            vert_cone_ellipse.insert(v, cer);
                            endpoints.push(v);
                        }
                    }
                    // PR-KV9: cylinder × CYLINDER ellipse (the equal-radius
                    // intersecting-axes Steinmetz section, ssi cyl∩cyl). The
                    // ellipse lies in a KNOWN plane — its own stored frame —
                    // and it equals `cylinder ∩ that-plane` for EITHER owner
                    // (the curve is on both), so the existing cylinder+plane
                    // relocation closed form applies verbatim with the plane
                    // derived from the stored curve: n̂ from the ellipse
                    // normal, d = −n̂·center. `cyl` here holds the LAST
                    // cylinder scanned; with two cylinder entries either is
                    // exact, and the incidence order is deterministic.
                    (Some(_), None, None) if cyls.len() == 2 => {
                        // Deterministic owner order: sort by InputId (A first).
                        let mut cs = cyls.clone();
                        cs.sort_by_key(|&(i, ..)| matches!(i, InputId::B));
                        let (i1, axis_point, axis_dir, radius) = cs[0];
                        let (i2, ap2, ad2, _) = cs[1];
                        let budget = chord_tol_for_curved_owner(i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(i2, a, b, 0, (s, e))?;
                        let nn = normalize3(normal.as_array());
                        let plane_n = Vector3::new(nn[0], nn[1], nn[2]);
                        let c = center.as_array();
                        let plane_d = -(nn[0] * c[0] + nn[1] * c[1] + nn[2] * c[2]);
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: Some((ap2, ad2, budget)),
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // Anything else (sphere, coplanar multi-solid): out of
                    // scope. Loud STOP (P9/P10).
                    _ => {
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: s,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                }
            }
            // M5 (Y4): a procedural surface-pair edge carries its two defining
            // surfaces directly. Like the TORUS block, its endpoints are an
            // implicit-pair (degree-4) relocation handled AFTER the conic
            // audit below — NOT part of the conic `endpoints`/`relocations`
            // bookkeeping (a procedural curve has no `t`). Only record the
            // pair here.
            Curve::SurfacePair { a, b } => {
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    vert_surface_pair.insert(v, (a, b));
                }
            }
            Curve::LineSegment => {
                // PR-F3: a LineSegment intersection edge between a PLANE and a
                // CYLINDER is a ruling LINE of the cylinder (ssi plane_cylinder
                // C3a/C3b). Recompute the exact line from the incidence and
                // re-select the unique candidate through both endpoints (the
                // SAME rule Stage 3's `build_intersection_curves` used).
                // Plane∩plane segments are exact → skip. Any OTHER curved
                // surface on a LineSegment edge is out of scope → loud STOP
                // (P9; cone generator lines arrive with their own closed form
                // when a fixture demands them).
                let key = if s < e { (s, e) } else { (e, s) };
                let Some(entries) = inc0.get(&key) else {
                    continue;
                };
                // KV6d Tier B: a TORUS-bearing LineSegment edge is a degree-4
                // intersection handled by the implicit-pair Newton relocation
                // block after this scan — defer it here (the conic LineSegment
                // arm has no closed form for it). Skip rather than STOP.
                if entries
                    .iter()
                    .any(|&(_, s)| matches!(s, Surface::Torus { .. }))
                {
                    continue;
                }
                let mut cyls: Vec<(InputId, Surface)> = Vec::new();
                let mut plane_surf: Option<Surface> = None;
                let mut pp: Vec<(Vector3, f64)> = Vec::new();
                let mut other_curved = false;
                for &(input, surf) in entries {
                    match surf {
                        Surface::Cylinder { .. } => cyls.push((input, surf)),
                        Surface::Plane { normal, d } => {
                            plane_surf = Some(surf);
                            pp.push((normal, d));
                        }
                        _ => other_curved = true,
                    }
                }
                // Two convertible pairs: cylinder × ⊥plane (F3) and PARALLEL
                // cylinder × cylinder (PR-KV9, ssi cyl∥cyl ruling lines).
                // Other curved-bearing line edges stay a loud STOP.
                let (surf_a, surf_b, tol) = match (cyls.as_slice(), plane_surf) {
                    ([(ci, cs)], Some(pl)) if !other_curved => {
                        (*cs, pl, chord_tol_for_curved_owner(*ci, a, b, 0, (s, e))?)
                    }
                    ([(i1, c1), (i2, c2)], None) if !other_curved => {
                        // Both meshes' facet chords contribute to the crossing
                        // vertex — the combined band is the SUM of the two
                        // owners' Stage-1 bounds (derived, not widening).
                        let t = chord_tol_for_curved_owner(*i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(*i2, a, b, 0, (s, e))?;
                        (*c1, *c2, t)
                    }
                    ([], _) if !other_curved => {
                        // plane∩plane — the segment is exact, but record the
                        // line's planes per endpoint for the PR-KV11 triple-
                        // point pass below.
                        if pp.len() == 2 {
                            let entry = (pp[0].0, pp[0].1, pp[1].0, pp[1].1);
                            for v in [s, e] {
                                // Increment 3: certified exact junction — enters no map (see above).
                                if exact_junctions.contains(&v) {
                                    continue;
                                }
                                vert_pp_planes.entry(v).or_default().push(entry);
                            }
                        }
                        continue;
                    }
                    _ => {
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: s,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                };
                let to_ssi_err = |reason| YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason,
                };
                let q0 = surface_to_quadric(surf_a).map_err(to_ssi_err)?;
                let q1 = surface_to_quadric(surf_b).map_err(to_ssi_err)?;
                let returned =
                    ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::IntersectFailed(err),
                    })?;
                let p_s = mesh.verts[s as usize];
                let p_e = mesh.verts[e as usize];
                // PR-F3b: the SAME propagated band as Stage-3 matching (the
                // metric is shared, so every gate carries the factor).
                // N46 (task #164): a `cylinder ∩ plane` generator uses the EXACT
                // worst-case band `√(B_in² + tol²)` (superseding the first-order
                // `line_band_amplification`, which under-admits near tangency —
                // R0026's `AmbiguousCurve{2,0}` reaches THIS Stage-4 relocation
                // once Stage-3 selection passes). Non-cyl/plane pairs keep the
                // linear factor (cyl∩cyl Steinmetz, cone-apex lines).
                let line_tol = cyl_plane_generator_band(surf_a, surf_b, tol).unwrap_or_else(|| {
                    line_band_amplification(surf_a, surf_b).unwrap_or(1.0) * tol
                });
                let mut matched: Option<LineReloc> = None;
                let mut matched_n = 0usize;
                let mut matched_lines: Vec<(Point3, Vector3)> = Vec::new();
                for c in &returned {
                    if let ssi_rs::SsiCurve::Line { point, dir } = *c {
                        if line_perp_distance(p_s, point, dir) <= line_tol
                            && line_perp_distance(p_e, point, dir) <= line_tol
                        {
                            matched_n += 1;
                            matched_lines.push((point, dir));
                            matched = Some(LineReloc {
                                point,
                                dir,
                                band_budget: line_tol,
                            });
                        }
                    }
                }
                // R0072: near-tangent plane∩cylinder yields two near-coincident
                // parallel generators that both pass the band; the edge lies on
                // exactly one. Break the tie by position (the disjoint-lowest
                // endpoint-distance interval) — the SAME rule Stage 3 uses. If no
                // unambiguous winner (overlapping intervals / non-parallel), the
                // loud `AmbiguousCurve` below stands.
                if matched_n > 1 {
                    if let Some(wk) = select_disjoint_parallel_line(&matched_lines, p_s, p_e) {
                        let (point, dir) = matched_lines[wk];
                        matched_n = 1;
                        matched = Some(LineReloc {
                            point,
                            dir,
                            band_budget: line_tol,
                        });
                    }
                }
                let Some(lr) = (if matched_n == 1 { matched } else { None }) else {
                    return Err(YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::AmbiguousCurve {
                            candidates: returned.len(),
                            matched: matched_n,
                        },
                    });
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // A vertex on TWO DIFFERENT lines (e.g. a box corner ruling
                    // piercing the cylinder) would need a line∩line junction —
                    // out of scope, loud STOP rather than silently overwriting
                    // (the same defect class F3 fixes for line+circle).
                    if let Some(prev) = vert_line.get(&v) {
                        let same = line_perp_distance(prev.point, lr.point, lr.dir)
                            <= cad_primitives::TAU_MODEL
                            && {
                                let d1 = normalize3(prev.dir.as_array());
                                let d2 = normalize3(lr.dir.as_array());
                                let cx = [
                                    d1[1] * d2[2] - d1[2] * d2[1],
                                    d1[2] * d2[0] - d1[0] * d2[2],
                                    d1[0] * d2[1] - d1[1] * d2[0],
                                ];
                                (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt()
                                    <= cad_primitives::TAU_MODEL
                            };
                        if !same {
                            return Err(YangError::Stage4RegionInvalid {
                                vertex: v,
                                reason: Stage4InvalidReason::LocalRefinementRequired,
                            });
                        }
                    }
                    vert_line.insert(v, lr);
                    endpoints.push(v);
                }
            }
        }
    }

    // PR-KV11: resolve ellipse × (plane∩plane line) TRIPLE points. An ellipse
    // endpoint that also terminates an exact pp-segment (the cap∩face trace
    // crossing the lateral) must land on `(plane ∩ plane) ∩ cylinder`, not on
    // the ellipse alone — reuse the ellipse-junction closed form with a
    // synthetic second member carrying the line's OTHER plane (the one that
    // is not the ellipse's own cutting plane; bit identity — both come from
    // the same incidence `Surface::Plane` values).
    {
        let shared: Vec<u32> = vert_ellipse
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let e_a = vert_ellipse[&v];
            let mut others: Vec<(Vector3, f64)> = Vec::new();
            for &(n1, d1, n2, d2) in &vert_pp_planes[&v] {
                let m1 = n1.as_array() == e_a.plane_n.as_array() && d1 == e_a.plane_d;
                let m2 = n2.as_array() == e_a.plane_n.as_array() && d2 == e_a.plane_d;
                let other = if m1 {
                    Some((n2, d2))
                } else if m2 {
                    Some((n1, d1))
                } else {
                    None
                };
                if let Some(o) = other {
                    if !others
                        .iter()
                        .any(|&(n, d)| n.as_array() == o.0.as_array() && d == o.1)
                    {
                        others.push(o);
                    }
                }
            }
            match others.len() {
                // A pp-line through an ellipse endpoint whose pair does not
                // include the ellipse's own plane, or more than one distinct
                // crossing line: relocating onto any single curve leaves the
                // vertex off the others — loud STOP, never a silent pick
                // (P9/P10).
                0 | 2.. => {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: v,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                }
                1 => {
                    let (on, od) = others[0];
                    let e_b = EllipseReloc {
                        plane_n: on,
                        plane_d: od,
                        ..e_a
                    };
                    vert_ellipse.remove(&v);
                    vert_ell_junction.insert(v, (e_a, e_b));
                }
            }
        }
    }

    // PR-F3: a vertex shared by a LINE edge and a CIRCLE edge is a TRIPLE
    // point — it must end up on BOTH curves. Relocating onto either alone
    // leaves it off the other (the KV6b-F3 probe defect: radius exactly r,
    // axial coordinate off by the sagitta → output-face plane vs Newell
    // disagreement). The exact junction is `line ∩ plane-of-circle`: the line
    // lies ON the cylinder and the circle IS `cylinder ∩ circle-plane`, so the
    // line's piercing of the circle plane lies exactly on the circle. Pull
    // such vertices OUT of both single-curve maps into a junction map.
    let mut vert_junction: BTreeMap<u32, (LineReloc, CircleAssign)> = BTreeMap::new();
    {
        let shared: Vec<u32> = vert_line
            .keys()
            .filter(|v| vert_circle.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let lr = vert_line.remove(&v).expect("key from vert_line");
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            vert_junction.insert(v, (lr, circ));
        }
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 1–3):
    // resolve CIRCLE × (plane∩plane line) TRIPLE points — the circle analog
    // of the PR-KV11 ellipse×pp pass above. A vertex on both a section
    // circle and an exact pp-line is their junction; relocating onto the
    // circle alone slides it off the line's planes at real scale (the F0064
    // Newell-normal class). Exactly ONE distinct pp-line reroutes; zero or
    // several distinct lines (or an ellipse junction already claimed) is
    // over-determined — loud STOP, never a silent pick (P9/P10).
    let mut vert_pp_circle_junction: BTreeMap<u32, ((Point3, Vector3), CircleAssign)> =
        BTreeMap::new();
    {
        let shared: Vec<u32> = vert_circle
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            if vert_ell_junction.contains_key(&v) {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            let Some((n1, d1, n2, d2)) = dedup_single_pp_line(&vert_pp_planes[&v]) else {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            };
            let Some((lp, ld)) = pp_line(n1, d1, n2, d2) else {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            };
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            vert_pp_circle_junction.insert(v, ((lp, ld), circ));
        }
    }

    // Increment 5 (spec `yang_stage4_conic_triple_junction`, WIRED): a
    // vertex on ≥2 of the six single-curve conic maps whose inc0 incidence
    // dedups to EXACTLY 3 distinct surfaces is NOT ambiguous — it is the
    // unique transversal common point of those surfaces (the R0017-class
    // prism-edge × cone-lateral junction: exact on both planes,
    // chord-inexact on the cone). Relocate it onto all three via the
    // torus-block triple primitive instead of letting the over-determined
    // audits below STOP. Newton failure leaves the vertex in its maps —
    // the audits then STOP exactly as today (spec branch table). 2- or
    // ≥4-surface configurations are untouched (spec I2).
    let mut triple_moved: Vec<u32> = Vec::new();
    {
        let mut cand: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for v in vert_circle
            .keys()
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_line.keys())
        {
            cand.insert(*v);
        }
        for v in cand {
            let n_maps = [
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
            ]
            .iter()
            .filter(|b| **b)
            .count();
            // KV16: a same-type conic junction (two hyperbolas in the ONE
            // `vert_cone_hyperbola` slot) counts as multi-curve even though
            // only one map sees the vertex.
            if n_maps < 2 && !same_type_junction.contains(&v) {
                continue;
            }
            let probe_v = std::env::var_os("YANG_SAMETYPE_PROBE").is_some();
            let Some(surfs) = vert_surfs.get(&v) else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} no vert_surfs entry");
                }
                continue;
            };
            if surfs.len() != 3 {
                if probe_v {
                    eprintln!("[triple-bail] v={v} surfs={}", surfs.len());
                }
                continue; // 2 / ≥4 surfaces keep the loud audits (I2)
            }
            let p = mesh.verts[v as usize];
            let Some(proj) = relocate_onto_implicit_triple(p, surfs[0], surfs[1], surfs[2]) else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} newton diverged");
                }
                continue; // Newton diverged → the audits STOP loudly
            };
            let qa = proj.as_array();
            let (Some((_, n0)), Some((_, n1))) = (
                surface_value_and_normal(surfs[0], qa),
                surface_value_and_normal(surfs[1], qa),
            ) else {
                continue; // evaluation failed → the audits STOP loudly
            };
            // Derived displacement gate: a chord vertex moves to the exact
            // junction by ≤ 2·d_ε / sin θ (the torus-block metric — NOT a
            // tolerance widening). Beyond it is a real off-curve error.
            let pa = p.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let sin_theta = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if rho > gate {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::OffCurveBeyondChordBand,
                });
            }
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-triple-junction] v={v} rho={rho:.4e} gate={gate:.4e} surfs=3 relocated"
                );
            }
            // Bookkeeping (spec I3/I4): out of every single-curve map and
            // out of `endpoints` (all occurrences — one push per incident
            // curve), so the audits and the no-skip balance never see it;
            // NOT added to `processed`/`relocations` (source stays
            // `BRepVertex`, position now exact).
            vert_circle.remove(&v);
            vert_ellipse.remove(&v);
            vert_cone_ellipse.remove(&v);
            vert_parabola.remove(&v);
            vert_cone_hyperbola.remove(&v);
            vert_line.remove(&v);
            endpoints.retain(|&u| u != v);
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                triple_moved.push(v);
            }
        }
    }

    // M8 disc∩disc no-skip audit (P10): a circle∩circle lens corner that is ALSO
    // on any OTHER curve type (a line, ellipse, cone conic, or line+circle
    // junction) is an over-determined junction this arm does not resolve — loud
    // STOP rather than relocate it onto only the two circles. (Cannot arise for a
    // pure disc∩disc lens, but never silently pick.)
    for v in vert_circle_junction.keys() {
        if vert_line.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }

    // A vertex shared by BOTH a circle and an ellipse edge (two distinct curves
    // through one vertex) is a genuine ambiguity — relocating it twice would be
    // wrong, so loud STOP rather than silently picking one (spec §4 no-skip
    // audit / P10).
    // PR-F3: the line+circle junction is HANDLED (vert_junction above); a line
    // meeting any OTHER conic is still a loud STOP, folded into each audit.
    for v in vert_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR21: a vertex shared by a cone-ellipse edge AND any other conic edge
    // (cylinder-ellipse or circle) is a genuine ambiguity — loud STOP (spec
    // §3.2 / P10), the same no-skip audit extended to the cone map.
    for v in vert_cone_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR22: a vertex shared by a cone-parabola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse) is a genuine ambiguity — loud
    // STOP (P10), the same no-skip audit extended to the parabola map.
    for v in vert_parabola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR23: a vertex shared by a cone-hyperbola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse / cone-parabola) is a genuine
    // ambiguity — loud STOP (P10), the same no-skip audit extended to the
    // hyperbola map.
    for v in vert_cone_hyperbola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }

    // (2) Relocate / retag every endpoint. `processed` is the no-skip audit set;
    // `moved` is the subset whose position actually changed (ρ > TAU_WORK) — the
    // triangles touching THOSE verts are the ones Stage-4 validation gates
    // (spec §4.5 step 4: validate per RELOCATED triangle, not pre-existing
    // arrangement slivers that `boolean()` legitimately kept for watertightness).
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        for tok in list.split(',') {
            let Ok(v) = tok.trim().parse::<u32>() else {
                continue;
            };
            if let Some(er) = vert_ellipse.get(&v) {
                eprintln!(
                    "YANG_V_PROBE v={v} er plane_n={:?} plane_d={:.17e} center={:?} \
                     normal={:?} major_axis={:?} a={:.17e} b={:.17e} second_cyl={:?}",
                    er.plane_n,
                    er.plane_d,
                    er.center,
                    er.normal,
                    er.major_axis,
                    er.major_radius,
                    er.minor_radius,
                    er.second_cyl,
                );
            }
            eprintln!(
                "YANG_V_PROBE v={v} p={:?} circle={} ellipse={} cone_ell={} parab={} hyp={} \
                 line={} ell_junction={} circle_junction={} line_circle_junction={} \
                 pp_planes={} pp_circle_junction={} endpoint={}",
                mesh.verts.get(v as usize),
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
                vert_ell_junction.contains_key(&v),
                vert_circle_junction.contains_key(&v),
                vert_junction.contains_key(&v),
                vert_pp_planes.contains_key(&v),
                vert_pp_circle_junction.contains_key(&v),
                endpoints.contains(&v),
            );
        }
    }
    let mut processed: HashSet<u32> = HashSet::new();
    let mut moved: HashSet<u32> = HashSet::new();
    // Increment 5: triple-junction relocations count as moved (their
    // incident triangles get the Stage-4 fold validation) but are NOT in
    // `processed`/`relocations` — the no-skip audit balance is untouched
    // because they left `endpoints` too (spec I3).
    moved.extend(triple_moved.iter().copied());
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    // Deterministic order: BTreeMap iteration.
    for (&v, &(center, normal, radius, src_r)) in &vert_circle {
        let p = mesh.verts[v as usize];
        // PR-YR19 (spec §4 Site 2): split the residual so the in-plane RADIAL
        // band is the propagated `(R/r_c)·d_ε` for a sphere section circle while
        // the AXIAL band stays `d_ε`. For `None`/non-sphere this is identical to
        // `max(axial, radial_dev) > d_eps`, i.e. byte-identical to the prior
        // `circle_residual > d_eps`. Near-tangent (`radius ≤ MIN_FEATURE_SIZE`)
        // fails closed (keeps the unscaled band).
        let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
        let radial_band = match src_r {
            Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => (big_r / radius) * d_eps,
            _ => d_eps,
        };
        if axial > d_eps || radial_dev > radial_band {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        // Preserve the original combined-max `rho` for the `> TAU_WORK`
        // move-gate so its semantics are unchanged.
        let rho = axial.max(radial_dev);
        // Always project to obtain the circle-frame angle `t` (and the exact
        // on-curve position). For ρ ≤ TAU_WORK the projection is a no-op move
        // but still yields the retag `t`; for the relocate band it moves the
        // vertex onto the curve.
        let (proj, t) = project_onto_circle(p, center, normal, radius)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // M8 disc∩disc CROSSING: relocate each lens-corner vertex onto the EXACT
    // circle∩circle intersection (on BOTH coplanar circles). The vertex sits on
    // a Stage-1 chord, off each circle radially by ≤ d_eps; the displacement to
    // the exact corner is amplified by `1/sin θ`, θ = angle between the two
    // circles' radial directions at the corner (the same derived gradient metric
    // as the cyl×cyl ellipse junction — NOT tolerance widening). A grazing/
    // tangent crossing (θ → 0) has no well-defined corner and `coplanar_circle_
    // circle_intersection` returns `None` → loud STOP.
    for (&v, &(ca, cb)) in &vert_circle_junction {
        let p = mesh.verts[v as usize];
        let (c_a, n_a, r_a, _) = ca;
        let (c_b, n_b, r_b, _) = cb;
        let Some(j) = coplanar_circle_circle_intersection(c_a, n_a, r_a, c_b, n_b, r_b, p) else {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // sin θ = |r̂_a × r̂_b| at the corner (both radial vectors are in-plane).
        let ra_v = [ja[0] - c_a.x(), ja[1] - c_a.y(), ja[2] - c_a.z()];
        let rb_v = [ja[0] - c_b.x(), ja[1] - c_b.y(), ja[2] - c_b.z()];
        let ra_h = normalize3(ra_v);
        let rb_h = normalize3(rb_v);
        let cr = [
            ra_h[1] * rb_h[2] - ra_h[2] * rb_h[1],
            ra_h[2] * rb_h[0] - ra_h[0] * rb_h[2],
            ra_h[0] * rb_h[1] - ra_h[1] * rb_h[0],
        ];
        let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        // `j` is on circle_a by construction; project to get its frame angle `t`
        // for the source retag (positionally exact on both circles either way).
        let (proj, t) = project_onto_circle(j, c_a, n_a, r_a)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR11: ellipse relocation loop, mirroring the circle loop above. Closed
    // form via the cylinder parameterization (spec §2). Same `d_eps` chord band.
    for (&v, er) in &vert_ellipse {
        let p = mesh.verts[v as usize];
        let rho = ellipse_residual(p, er);
        // PR-KV9: cylinder×cylinder sections gate against the per-point
        // gradient band (combined budget × 1/sin α); at tangency grade the
        // metric is unbounded and the Stage-3 surface-membership gate is
        // the backstop. The cylinder×plane path keeps the global d_ε
        // byte-for-byte.
        let gate = match er.second_cyl {
            Some((ap2, ad2, budget)) => {
                cyl_cyl_point_amplification(p, (er.axis_point, er.axis_dir), (ap2, ad2))
                    .map_or(f64::INFINITY, |amp| amp * budget)
            }
            None => d_eps,
        };
        if rho > gate {
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE ellipse band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?}"
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let (proj, t) = project_onto_ellipse_via_cylinder(p, er)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Task #145 mechanism 2 (spec `yang_453_mixed_cycle_conic_backtrack`
        // §3b, I6): the azimuth projection amplifies by 1/(n·â) ALONG a
        // near-tangent section — a §4.4.1 relocation is bounded by the same
        // band the ρ gate uses. Move within band → keep the closed form
        // byte-identically (R1); beyond → in-plane nearest point (R2);
        // still beyond → loud STOP (R3), never a silent macro slide.
        let move_len = |q: Point3| -> f64 {
            let qa = q.as_array();
            let pa = p.as_array();
            ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt()
        };
        let (proj, t) = if er.second_cyl.is_some() || move_len(proj) <= gate {
            // R1 (and the cyl×cyl arm, whose per-point-amplified `gate`
            // already carries the KV9 gradient machinery): byte-identical
            // closed-form azimuth projection.
            (proj, t)
        } else {
            let (near_proj, near_t) = project_onto_ellipse_nearest(p, er)
                .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
            // R2/R3 budget: the vertex's surface residuals are ≤ `gate` each,
            // and distance-to-curve amplifies by 1/sin θ (θ = angle between
            // the two surface normals AT the relocated point) — the same
            // derived gradient-band the circle-junction and pp-plane gates
            // use (never widening). Evaluated at `near_proj`, where the
            // transversality of the accepted position is what matters.
            let budget = {
                let np = near_proj.as_array();
                let q = er.axis_point.as_array();
                let a_hat = normalize3(er.axis_dir.as_array());
                let w = [np[0] - q[0], np[1] - q[1], np[2] - q[2]];
                let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
                let radial = normalize3([
                    w[0] - along * a_hat[0],
                    w[1] - along * a_hat[1],
                    w[2] - along * a_hat[2],
                ]);
                let n_pl = normalize3(er.plane_n.as_array());
                let cr = [
                    radial[1] * n_pl[2] - radial[2] * n_pl[1],
                    radial[2] * n_pl[0] - radial[0] * n_pl[2],
                    radial[0] * n_pl[1] - radial[1] * n_pl[0],
                ];
                let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
                // Exact tangency → unbounded corridor (see
                // `tangent_plane_corridor`); the projection is still the
                // local nearest point.
                tangent_plane_corridor(gate, sin_theta)
            };
            if std::env::var_os("YANG_T145_RELOC_PROBE").is_some() {
                eprintln!(
                    "[t145-reloc] v={v} rho={rho:.3e} gate={gate:.3e} budget={budget:.3e} \
                     az_move={:.3e} near_move={:.3e} p={p:?} az={proj:?} near={near_proj:?}",
                    move_len(proj),
                    move_len(near_proj),
                );
            }
            if move_len(near_proj) > budget {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::OffCurveBeyondChordBand,
                });
            }
            (near_proj, near_t)
        };
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-KV9: ellipse×ellipse JUNCTION relocation. The exact junction lies
    // on `(plane₁ ∩ plane₂) ∩ cylinder` (the crossing point of the two
    // Steinmetz sections — on the cylinder and in BOTH cutting planes,
    // hence on both ellipses). The plane–plane line is exact; intersecting
    // it with the relocation cylinder is a quadratic with ≤ 2 roots; the
    // root nearest the current vertex is the junction (the two crossing
    // points are 2r apart — far outside any chord band, so nearest-pick is
    // deterministic and unambiguous). Gate at 2·d_ε (each constituent
    // membership is within its own propagated band; the junction inherits
    // both, mirroring the line+circle junction's derivation).
    for (&v, &(e_a, e_b)) in &vert_ell_junction {
        let p = mesh.verts[v as usize];
        let n1 = normalize3(e_a.plane_n.as_array());
        let n2 = normalize3(e_b.plane_n.as_array());
        let dir = [
            n1[1] * n2[2] - n1[2] * n2[1],
            n1[2] * n2[0] - n1[0] * n2[2],
            n1[0] * n2[1] - n1[1] * n2[0],
        ];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if dl < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let d = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
        // A point on both planes: solve n1·x = −d1, n2·x = −d2 in the span
        // of {n1, n2} (x = α·n1 + β·n2; Gram system with g = n1·n2).
        let g = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
        let det = 1.0 - g * g;
        if det.abs() < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let (r1, r2) = (-e_a.plane_d, -e_b.plane_d);
        let alpha = (r1 - g * r2) / det;
        let beta = (r2 - g * r1) / det;
        let p0 = [
            alpha * n1[0] + beta * n2[0],
            alpha * n1[1] + beta * n2[1],
            alpha * n1[2] + beta * n2[2],
        ];
        // Intersect the line p0 + t·d with the relocation cylinder of e_a.
        let ax = normalize3(e_a.axis_dir.as_array());
        let ap = e_a.axis_point.as_array();
        let rel = [p0[0] - ap[0], p0[1] - ap[1], p0[2] - ap[2]];
        let perp = |w: [f64; 3]| -> [f64; 3] {
            let h = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            [w[0] - h * ax[0], w[1] - h * ax[1], w[2] - h * ax[2]]
        };
        let rp = perp(rel);
        let dp = perp(d);
        let aa = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
        let bb = 2.0 * (rp[0] * dp[0] + rp[1] * dp[1] + rp[2] * dp[2]);
        let cc = rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2] - e_a.radius * e_a.radius;
        let disc = bb * bb - 4.0 * aa * cc;
        if !(aa > cad_primitives::MIN_FEATURE_SIZE && disc >= 0.0) {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let sq = disc.sqrt();
        let pa = p.as_array();
        let mut best: Option<([f64; 3], f64)> = None;
        for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
            let x = [p0[0] + t * d[0], p0[1] + t * d[1], p0[2] + t * d[2]];
            let dd =
                ((x[0] - pa[0]).powi(2) + (x[1] - pa[1]).powi(2) + (x[2] - pa[2]).powi(2)).sqrt();
            if best.map(|(_, b)| dd < b).unwrap_or(true) {
                best = Some((x, dd));
            }
        }
        let (j, rho) = best.expect("two real roots checked");
        // PR-KV11: the vertex moves ALONG the junction line to reach the
        // cylinder, so its radial chord residual (≤ the combined band) is
        // amplified by `1/|d̂·r̂|` — the directional derivative of the
        // radial distance along the line at the junction (the same derived
        // metric propagation as the KV9 cyl×cyl `1/sin α` gradient band; a
        // grazing line ⇒ unbounded metric, backstopped by the Stage-3
        // surface-membership gates, mirroring the cyl×cyl arm).
        let rel_j = [j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]];
        let rp_j = perp(rel_j);
        let rp_j_len = (rp_j[0] * rp_j[0] + rp_j[1] * rp_j[1] + rp_j[2] * rp_j[2]).sqrt();
        let grad = if rp_j_len > 0.0 {
            ((d[0] * rp_j[0] + d[1] * rp_j[1] + d[2] * rp_j[2]) / rp_j_len).abs()
        } else {
            0.0
        };
        // KV9-F1 E-L2 (spec §2c, branch row J1): a junction of two sections of
        // the SAME unordered cylinder pair is ALWAYS the pair's surface-tangency
        // point (the two decomposition planes intersect in the line through both
        // tangency points; that line meets the cylinder exactly where the two
        // radial gradients align). There the vertex is the PINCH of the two
        // faceted-surface intersection polylines, whose standoff from the exact
        // crossing is SECOND-order-controlled: in tangent-plane coordinates the
        // cylinders are the graphs y = r − x²/2r and y = r − z²/2r; facet
        // displacements a ∈ [0, ε_A], b ∈ [0, ε_B] perturb the intersection to
        // the hyperbola x² − z² = 2r(b−a), standoff √(2r·|b−a|) ≤ √(2r·B) with
        // B the combined chord budget carried by `second_cyl`, plus ≤ B
        // normal-direction offset. A derived metric conversion (the
        // single-ellipse arm's 1/sin α analog at tangency grade), NOT tolerance
        // widening — the relocation target stays the EXACT junction. Every
        // other junction (row J2 — the KV11 box-edge class) keeps the
        // first-order 2·d_ε/|d̂·r̂| line metric byte-identical.
        let same_pair_budget = match (e_a.second_cyl, e_b.second_cyl) {
            (Some((sa_p, sa_d, ba)), Some((sb_p, sb_d, bb))) => {
                let same = e_a.axis_point.as_array() == e_b.axis_point.as_array()
                    && e_a.axis_dir.as_array() == e_b.axis_dir.as_array()
                    && sa_p.as_array() == sb_p.as_array()
                    && sa_d.as_array() == sb_d.as_array();
                if same {
                    Some(ba.max(bb))
                } else {
                    None
                }
            }
            _ => None,
        };
        let gate = if let Some(budget) = same_pair_budget {
            (2.0 * e_a.radius * budget).sqrt() + budget
        } else {
            tangent_plane_corridor(d_eps, grad)
        };
        // KV9-F1 Increment 0c census: per-junction second_cyl provenance +
        // first-order gate state (kept env-gated, like the other Stage-4 probes).
        if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
            eprintln!(
                "KV9_JUNCTION_PROBE v={v} p={p:?} j={j:?} rho={rho:.4e} grad={grad:.4e} \
                 gate={gate:.4e} d_eps={d_eps:.4e} \
                 a_axis=({:?},{:?}) a_second={:?} b_axis=({:?},{:?}) b_second={:?}",
                e_a.axis_point.as_array(),
                e_a.axis_dir.as_array(),
                e_a.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
                e_b.axis_point.as_array(),
                e_b.axis_dir.as_array(),
                e_b.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
            );
        }
        if rho > gate {
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE junction band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?} j={j:?}"
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = Point3::new(j[0], j[1], j[2]);
        // Param on e_a's ellipse for the source retag (output edges of BOTH
        // ellipses touch this vertex; the position is exact on both, so the
        // retag curve choice is positional-exact either way).
        let t = ellipse_param(
            proj,
            e_a.center,
            e_a.normal,
            e_a.major_axis,
            e_a.major_radius,
            e_a.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // LRR diagnostic (read-only): unified over-band run-structure across ALL
    // conic relocation maps. Per curve identity, sort by parameter `t`, flag
    // over-band vertices, and classify each over-band vertex as INTERIOR-bounded
    // (a within-band vertex exists at both a smaller AND larger t on the same
    // curve — the paper §4.5.1 condition) vs END/UNBOUNDED. Emits a per-case
    // verdict on whether EVERY over-band conic vertex is interior-bounded.
    if std::env::var_os("YANG_LRR_PROBE").is_some() {
        // Per (curve-kind, curve-identity) → sorted samples `(t, v, rho, band)`.
        type LrrGroups = BTreeMap<(&'static str, [u64; 3]), Vec<(f64, u32, f64, f64)>>;
        let mut groups: LrrGroups = BTreeMap::new();
        let kb = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        let mut push = |kind: &'static str, key: [u64; 3], t: f64, v: u32, rho: f64, band: f64| {
            groups
                .entry((kind, key))
                .or_default()
                .push((t, v, rho, band));
        };
        for (&v, &(center, normal, radius, src_r)) in &vert_circle {
            let p = mesh.verts[v as usize];
            let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
            let band = match src_r {
                Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => {
                    (big_r / radius) * d_eps
                }
                _ => d_eps,
            };
            let t = project_onto_circle(p, center, normal, radius)
                .map(|(_, t)| t)
                .unwrap_or(0.0);
            push(
                "circle",
                kb(center),
                t,
                v,
                axial.max(radial_dev),
                band.max(d_eps),
            );
        }
        for (&v, er) in &vert_ellipse {
            let p = mesh.verts[v as usize];
            push(
                "ellipse",
                kb(er.center),
                0.0,
                v,
                ellipse_residual(p, er),
                d_eps,
            );
        }
        for (&v, cer) in &vert_cone_ellipse {
            let p = mesh.verts[v as usize];
            let t = ellipse_param(
                p,
                cer.center,
                cer.normal,
                cer.major_axis,
                cer.major_radius,
                cer.minor_radius,
            );
            push(
                "cone_ell",
                kb(cer.center),
                t,
                v,
                cone_ellipse_residual(p, cer),
                cer.cone_d_eps,
            );
        }
        for (&v, cpr) in &vert_parabola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                cpr.apex,
                cpr.cone_axis_dir,
                cpr.half_angle,
                cpr.plane_n,
                cpr.plane_d,
            );
            push("parabola", kb(cpr.vertex), 0.0, v, rho, cpr.cone_d_eps);
        }
        for (&v, chr) in &vert_cone_hyperbola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                chr.apex,
                chr.cone_axis_dir,
                chr.half_angle,
                chr.plane_n,
                chr.plane_d,
            );
            push("hyperbola", kb(chr.apex), 0.0, v, rho, chr.cone_d_eps);
        }
        let mut all_interior = true;
        let mut n_over = 0usize;
        for ((kind, _key), list) in &mut groups {
            list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let within: Vec<f64> = list.iter().filter(|r| r.2 <= r.3).map(|r| r.0).collect();
            let (tmin, tmax) = (
                within.iter().cloned().fold(f64::INFINITY, f64::min),
                within.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            let mut seq = String::new();
            for (t, _v, rho, band) in list.iter() {
                if rho > band {
                    n_over += 1;
                    let interior = *t > tmin && *t < tmax;
                    if !interior {
                        all_interior = false;
                    }
                    seq.push(if interior { 'I' } else { 'E' });
                } else {
                    seq.push('.');
                }
            }
            if seq.contains('I') || seq.contains('E') {
                eprintln!(
                    "YANG_LRR_PROBE {kind} n={} within={} seq={seq}",
                    list.len(),
                    within.len()
                );
            }
        }
        eprintln!(
            "YANG_LRR_VERDICT n_over={n_over} all_over_band_interior_bounded={}",
            n_over > 0 && all_interior
        );
    }

    // PR-YR21: cone-ellipse relocation loop, mirroring the cylinder-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (spec §3.1). Gated
    // against the cone's OWN chord budget `cone_d_eps` (NOT the rim-AABB `d_eps`)
    // so a tall-thin cone's residual is checked against the honest cone bound.
    for (&v, cer) in &vert_cone_ellipse {
        let p = mesh.verts[v as usize];
        let rho = cone_ellipse_residual(p, cer);
        if rho > cer.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            cer.apex,
            cer.axis_dir,
            cer.half_angle,
            cer.plane_n,
            cer.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t` in the stored ellipse frame so the unchanged
        // `eval_source` Ellipse arm reproduces the relocated position.
        let t = ellipse_param(
            proj,
            cer.center,
            cer.normal,
            cer.major_axis,
            cer.major_radius,
            cer.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR22: cone-parabola relocation loop, mirroring the cone-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (the section TYPE does
    // not change the relocation — `project_onto_cone_section` is type-agnostic;
    // its `s ≤ 0` / generator-parallel guards correctly reject the out-of-scope
    // parabola tail, which the fixture's finite arc avoids). Gated against the
    // cone's OWN chord budget `cone_d_eps`.
    for (&v, cpr) in &vert_parabola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        );
        if rho > cpr.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t` = the conjugate-axis coordinate of the parabola
        // parameterization `(proj − vertex)·(normal × axis_dir)`, so the unchanged
        // `eval_source` Parabola arm reproduces the relocated position (oracle3).
        let n = normalize3(cpr.normal.as_array());
        let ax = normalize3(cpr.para_axis_dir.as_array());
        let conj = [
            n[1] * ax[2] - n[2] * ax[1],
            n[2] * ax[0] - n[0] * ax[2],
            n[0] * ax[1] - n[1] * ax[0],
        ];
        let vtx = cpr.vertex.as_array();
        let pr = proj.as_array();
        let t =
            (pr[0] - vtx[0]) * conj[0] + (pr[1] - vtx[1]) * conj[1] + (pr[2] - vtx[2]) * conj[2];
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR23: cone-hyperbola relocation loop, mirroring the cone-parabola loop.
    // Closed form via the same type-agnostic cone GENERATOR parameterization
    // (`project_onto_cone_section`); its `s ≤ 0` / generator-parallel guards
    // correctly reject the out-of-scope asymptote, which the fixture's finite arc
    // avoids. Gated against the cone's OWN chord budget `cone_d_eps`.
    for (&v, chr) in &vert_cone_hyperbola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        );
        if rho > chr.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t = asinh(v_coord / b)` where `v_coord` is the
        // conjugate-axis coordinate `(proj − center)·(normal × major_axis)` and
        // `b = semi_conjugate`. The eval is
        // `center + a·cosh(t)·major + b·sinh(t)·(normal×major)`, so
        // `v_coord = b·sinh(t) ⇒ t = asinh(v_coord/b)` (sinh is the bijective
        // coordinate; well-defined ∀ v_coord). The unchanged `eval_source`
        // Hyperbola arm reproduces the relocated position (oracle3).
        let n = normalize3(chr.normal.as_array());
        let maj = normalize3(chr.major_axis.as_array());
        let conj = [
            n[1] * maj[2] - n[2] * maj[1],
            n[2] * maj[0] - n[0] * maj[2],
            n[0] * maj[1] - n[1] * maj[0],
        ];
        let ctr = chr.center.as_array();
        let pr = proj.as_array();
        let v_coord =
            (pr[0] - ctr[0]) * conj[0] + (pr[1] - ctr[1]) * conj[1] + (pr[2] - ctr[2]) * conj[2];
        let t = (v_coord / chr.semi_conjugate).asinh();
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-F3: ruling-line relocation loop. The residual is the perpendicular
    // distance to the exact line (the sagitta of the Stage-1 facet chord the
    // arrangement point sits on), gated at the same global `d_eps` band as the
    // circle loop. The relocated position is the foot of the perpendicular —
    // exactly on the line, hence exactly on BOTH the cutting plane and the
    // cylinder. `t` is the along-line parameter; no conic OUTPUT edge claims a
    // line vertex in `emit_topology`, so its source stays `BRepVertex` and
    // `eval_source` returns the relocated mesh position directly.
    for (&v, lr) in &vert_line {
        let p = mesh.verts[v as usize];
        let rho = line_perp_distance(p, lr.point, lr.dir);
        // PR-F3b/PR-KV9: the residual is the line-distance metric, so the
        // gate is the ABSOLUTE propagated budget computed at collection (the
        // owner chord band(s) converted into this metric) — not the raw
        // radial band, and not the global d_ε (whose owner mix is wrong for
        // cylinder×cylinder lines).
        if rho > lr.band_budget {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let d = normalize3(lr.dir.as_array());
        let pt = lr.point.as_array();
        let x = p.as_array();
        let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
        let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let proj = Point3::new(
            pt[0] + along * d[0],
            pt[1] + along * d[1],
            pt[2] + along * d[2],
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, along));
        processed.insert(v);
    }

    // PR-F3: line+circle JUNCTION relocation loop. The exact junction is
    // `line ∩ plane-of-circle` (which lies exactly on the circle, since the
    // line is on the cylinder and the circle is cylinder ∩ circle-plane). The
    // residual gate is `2·d_eps`: the vertex is off the line radially by ≤ one
    // sagitta AND off the circle plane along the line by ≤ another
    // sagitta-order term (it sits on the crossing of the cutting plane with a
    // rim-chord facet edge), so the combined displacement to the junction is
    // bounded by 2·d_eps — a derived bound, not tolerance widening. The final
    // position is `project_onto_circle(j)` so the vertex's `BRepEdge { edge, t }`
    // source round-trips bitwise through the unchanged `eval_source` Circle arm.
    for (&v, &(lr, (center, normal, radius, _src_r))) in &vert_junction {
        let p = mesh.verts[v as usize];
        let n = normalize3(normal.as_array());
        let d = normalize3(lr.dir.as_array());
        let denom = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
        if denom.abs() < cad_primitives::TAU_MODEL {
            // Line parallel to the circle plane: no transversal junction.
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let pt = lr.point.as_array();
        let c = center.as_array();
        let s_par = (n[0] * (c[0] - pt[0]) + n[1] * (c[1] - pt[1]) + n[2] * (c[2] - pt[2])) / denom;
        let j = Point3::new(
            pt[0] + s_par * d[0],
            pt[1] + s_par * d[1],
            pt[2] + s_par * d[2],
        );
        let pj = [
            p.as_array()[0] - j.as_array()[0],
            p.as_array()[1] - j.as_array()[1],
            p.as_array()[2] - j.as_array()[2],
        ];
        let rho = (pj[0] * pj[0] + pj[1] * pj[1] + pj[2] * pj[2]).sqrt();
        // PR-F3b: line-band component carries the propagated budget; the
        // along-line crossing component stays at the raw d_ε.
        if rho > lr.band_budget + d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 4–6):
    // relocate each circle×pp-line junction vertex onto the exact
    // line∩circle point (line∩sphere quadratic + circle-plane residual
    // certificate — valid for the in-plane AND transversal configurations).
    for (&v, &((lp, ld), (center, normal, radius, _src_r))) in &vert_pp_circle_junction {
        let p = mesh.verts[v as usize];
        let Some(j) = pp_line_circle_junction(lp, ld, center, normal, radius, p, d_eps) else {
            // Branch 5: the line misses the circle (or no root is on the
            // circle's plane) — not a resolvable junction here.
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // Branch 6: crossing amplification — the vertex sits within its
        // chord bands of BOTH curves; the displacement to their junction is
        // amplified by 1/sin θ, θ = angle between the line direction and the
        // circle tangent at the junction (the vert_circle_junction pattern;
        // derived, not widening).
        let n = normalize3(normal.as_array());
        let dh = normalize3(ld.as_array());
        let c = center.as_array();
        let rvec = normalize3([ja[0] - c[0], ja[1] - c[1], ja[2] - c[2]]);
        let tangent = [
            n[1] * rvec[2] - n[2] * rvec[1],
            n[2] * rvec[0] - n[0] * rvec[2],
            n[0] * rvec[1] - n[1] * rvec[0],
        ];
        let cross = [
            dh[1] * tangent[2] - dh[2] * tangent[1],
            dh[2] * tangent[0] - dh[0] * tangent[2],
            dh[0] * tangent[1] - dh[1] * tangent[0],
        ];
        let sin_theta = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        // Branch 4: `j` is exactly on the line and on the circle's sphere;
        // the circle projection yields the frame angle `t` for the retag
        // (positionally a no-op up to f64 — `j` is on the circle).
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // No-skip audit (anti-disproven-attempt): every conic endpoint was handled.
    let relocation_keys: HashSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
    let endpoint_set: HashSet<u32> = endpoints.iter().copied().collect();
    if std::env::var_os("YANG_LRR_PROBE").is_some() && processed != endpoint_set {
        for &v in endpoint_set.difference(&processed) {
            let mut curs: Vec<String> = Vec::new();
            for (&(s, e), curve) in &curves0 {
                if s == v || e == v {
                    curs.push(format!("({s},{e})={curve:?}"));
                }
            }
            eprintln!(
                "YANG_LRR_UNCLAIMED endpoint v={v} on curves: {}",
                curs.join(" | ")
            );
        }
        for &v in processed.difference(&endpoint_set) {
            eprintln!("YANG_LRR_EXTRA processed-but-not-endpoint v={v}");
        }
    }
    if processed != endpoint_set || processed != relocation_keys {
        if std::env::var_os("YANG_LRR_PROBE").is_some() {
            eprintln!(
                "YANG_LRR_STOP site=no_skip_audit ep_ne_proc={} proc_ne_reloc={}",
                processed != endpoint_set,
                processed != relocation_keys
            );
        }
        return Err(YangError::Stage4RegionInvalid {
            vertex: u32::MAX,
            reason: Stage4InvalidReason::LocalRefinementRequired,
        });
    }

    // M5 (Y4): degree-4 surface-pair relocation via Newton on the two defining
    // surfaces — a sibling of the TORUS block below (both are implicit-pair,
    // not conic, so they are relocated AFTER the conic audit and are NOT part
    // of the conic `endpoints`/`relocations` bookkeeping). Each endpoint keeps
    // its `BRepVertex` source (a procedural curve has no `t`). A surface-pair
    // endpoint that is ALSO a conic endpoint mixes closed-form and
    // implicit-pair relocations — out of v1 scope, loud STOP (mirrors the
    // torus block's `endpoint_set` guard). `None` is a loud STOP (tangency /
    // parallel normals or non-convergence — never a partial move, P9).
    for (&v, &(sa, sb)) in &vert_surface_pair {
        if endpoint_set.contains(&v) {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let p = mesh.verts[v as usize];
        let proj =
            relocate_onto_implicit_pair(p, sa, sb).ok_or(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            })?;
        mesh.verts[v as usize] = proj;
        moved.insert(v);
    }

    // (2t) KV6d Tier B — degree-4 (TORUS) relocation via Newton on the implicit
    // surface pair. A torus's intersections are not conics, so these edges never
    // reach the `curves0` conic scan above; they arrive as untyped chord
    // segments and would otherwise stay off the analytic torus (the proven KV6d
    // blocker). For each intersection edge bearing exactly one torus and one
    // transversal partner, relocate both endpoints onto {F_torus=0, F_other=0}.
    // Kept SEPARATE from the conic bookkeeping (processed / endpoints /
    // relocations) — the output torus-intersection edges stay LineSegment
    // polylines (no analytic curve, no `t` retag), which validation and
    // `tessellate_torus_patch` already accept — so the conic no-skip audit above
    // is unaffected. Moved vertices join `moved` for the relocated-triangle
    // validation. v1 scope: one torus + one partner per edge; torus∩torus,
    // multi-surface junctions, and torus×conic junctions are loud STOPs (P9).
    {
        // Aggregate, per torus-edge endpoint, the single incident torus and the
        // DISTINCT partner surfaces across all its torus edges. One partner is a
        // plain torus∩surface edge (2-equation Newton); two partners is a
        // 3-surface JUNCTION — a box edge (two planes) piercing the torus, or a
        // torus∩plane meeting a torus∩plane′ — relocated onto all three. More
        // than two partners, or a torus∩torus edge, is out of v1 scope (STOP).
        let mut vert_torus: BTreeMap<u32, Surface> = BTreeMap::new();
        let mut vert_partners: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            let mut tori: Vec<Surface> = Vec::new();
            let mut others: Vec<Surface> = Vec::new();
            for &(_input, surf) in entries {
                if matches!(surf, Surface::Torus { .. }) {
                    tori.push(surf);
                } else {
                    others.push(surf);
                }
            }
            if tori.is_empty() {
                continue; // not a torus edge — conic scan / exact handles it
            }
            if tori.len() != 1 {
                // torus∩torus (degree-4 with no single base surface) — out of
                // v1 scope. Loud STOP.
                return Err(YangError::Stage4RegionInvalid {
                    vertex: s,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            for v in [s, e] {
                vert_torus.insert(v, tori[0]);
                let entry = vert_partners.entry(v).or_default();
                for o in &others {
                    if !entry.contains(o) {
                        entry.push(*o);
                    }
                }
            }
        }
        for (&v, &t_surf) in &vert_torus {
            // A torus-edge endpoint that is also a CONIC endpoint mixes the
            // implicit-pair and closed-form relocations — out of v1 scope, STOP.
            if endpoint_set.contains(&v) {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            let partners = &vert_partners[&v];
            let p = mesh.verts[v as usize];
            let (proj, n0, n1) = match partners.as_slice() {
                [s1] => {
                    let proj = relocate_onto_implicit_pair(p, t_surf, *s1).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    (proj, n0, n1)
                }
                [s1, s2] => {
                    // 3-surface junction: relocate onto {torus, s1, s2}. The
                    // displacement gate uses the torus∩s1 angle (the junction is
                    // a point; any incident curve's metric bounds the move).
                    let proj = relocate_onto_implicit_triple(p, t_surf, *s1, *s2).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    (proj, n0, n1)
                }
                _ => {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: v,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                }
            };
            // Derived displacement gate: a chord point moves to the exact curve
            // by ≤ 2·d_ε / sin θ, θ the angle between two incident surface
            // normals at the relocated point (the same metric as the disc∩disc /
            // cyl×cyl junction bands — NOT tolerance widening). Beyond it is a
            // real off-curve error, not a Stage-1 chord artifact → STOP.
            let pa = p.as_array();
            let qa = proj.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let sin_theta = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if std::env::var_os("YANG_TORUS_PROBE").is_some() {
                eprintln!(
                    "YANG_TORUS_PROBE v={v} rho={rho:.4e} gate={gate:.4e} d_eps={d_eps:.4e} \
                     sin_theta={sin_theta:.4e} p={p:?} proj={proj:?}"
                );
            }
            if rho > gate {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::OffCurveBeyondChordBand,
                });
            }
            // Bounded-face containment (KV6d closed torus, spec
            // `kv6d_closed_torus_revolve.md` failure modes): the wedge gate
            // bounds the TRANSVERSAL chord error but balloons (2d_ε/sinθ)
            // when the pair is near-tangential — exactly where an inscribed
            // mesh can close an intersection loop EARLY (entirely inside the
            // partner's bounded face) and the implicit-pair Newton then drags
            // the loop onto the infinite-surface curve, OUTSIDE the partner
            // FACE (C0065: wall x=1.45 vs outer equator 1.5, gap ≈ sagitta —
            // relocated points at |y| = 0.384 vs the wall's |y| ≤ 0.25). A
            // correctly resolved intersection vertex lies on both BOUNDED
            // faces, so a relocation escaping every matching partner face's
            // vertex hull (+d_ε) is a mesh-resolution artifact, not a chord
            // correction → loud STOP (the §4.3.3 near-tangency increment owns
            // the honest fix). Planes only: a planar face's loop hull bounds
            // the face (curved hulls under-bound — closed seam loops).
            for partner in partners {
                let Surface::Plane { .. } = partner else {
                    continue;
                };
                // Per matching face: an AABB that BOUNDS the face — loop
                // vertices plus each boundary CURVE's own extent (a disk's
                // loop is one closed circle through a single anchor vertex,
                // so vertex hulls under-bound curved loops — the t134 trap).
                // A loop curve without a cheap conservative bound makes the
                // face unbounded → the whole partner check is skipped
                // (defensive: no verdict, never a false wall).
                let mut hull: Option<[f64; 6]> = None;
                let mut unbounded = false;
                'faces: for brep in [a, b] {
                    for face in brep.faces() {
                        if face.surface != *partner {
                            continue;
                        }
                        let mut lo = [f64::MAX; 3];
                        let mut hi = [f64::MIN; 3];
                        for &e in face
                            .outer_loop
                            .iter()
                            .chain(face.inner_loops.iter().flatten())
                        {
                            let ed = &brep.edges()[e as usize];
                            for vid in [ed.start, ed.end] {
                                let q = brep.vertices()[vid as usize].point.as_array();
                                for k in 0..3 {
                                    lo[k] = lo[k].min(q[k]);
                                    hi[k] = hi[k].max(q[k]);
                                }
                            }
                            match ed.curve {
                                Curve::LineSegment => {}
                                Curve::Circle {
                                    center,
                                    normal,
                                    radius,
                                } => {
                                    let c = center.as_array();
                                    let n = normalize3(normal.as_array());
                                    for k in 0..3 {
                                        let ext = radius * (1.0 - n[k] * n[k]).max(0.0).sqrt();
                                        lo[k] = lo[k].min(c[k] - ext);
                                        hi[k] = hi[k].max(c[k] + ext);
                                    }
                                }
                                Curve::Ellipse {
                                    center,
                                    major_radius,
                                    ..
                                } => {
                                    let c = center.as_array();
                                    for k in 0..3 {
                                        lo[k] = lo[k].min(c[k] - major_radius);
                                        hi[k] = hi[k].max(c[k] + major_radius);
                                    }
                                }
                                _ => {
                                    unbounded = true;
                                    break 'faces;
                                }
                            }
                        }
                        let h = hull.get_or_insert([
                            f64::MAX,
                            f64::MAX,
                            f64::MAX,
                            f64::MIN,
                            f64::MIN,
                            f64::MIN,
                        ]);
                        for k in 0..3 {
                            h[k] = h[k].min(lo[k]);
                            h[3 + k] = h[3 + k].max(hi[k]);
                        }
                    }
                }
                if unbounded {
                    continue;
                }
                let Some(h) = hull else {
                    continue; // no matching input face (defensive): no verdict
                };
                let inside = (0..3).all(|k| {
                    proj.as_array()[k] >= h[k] - d_eps && proj.as_array()[k] <= h[3 + k] + d_eps
                });
                if !inside {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: v,
                        reason: Stage4InvalidReason::OffCurveBeyondChordBand,
                    });
                }
            }
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                moved.insert(v);
            }
        }
    }

    // (3) §4.5.3 reversed-intersection correction sweep.
    let mut collapsed_any = false;
    let mut attr_vec = std::mem::take(&mut attribution.attributions);
    // PR-KV9: junction vertices that landed on the SAME exact point are
    // duplicates of one geometric junction (near a tangency-grade curve
    // crossing the two chord polylines can intersect several times, giving
    // several arrangement vertices for ONE junction). Collapse the extras
    // onto the lowest index — the standard edge-collapse, which drops the
    // degenerate slivers between them and keeps the half-edge pairing
    // watertight.
    {
        let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
            std::collections::BTreeMap::new();
        for &v in vert_ell_junction.keys() {
            let p = mesh.verts[v as usize];
            by_pos
                .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                .or_default()
                .push(v);
        }
        for (_, group) in by_pos {
            if group.len() < 2 {
                continue;
            }
            let survivor = *group.iter().min().expect("non-empty");
            for &victim in group.iter().filter(|&&v| v != survivor) {
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[collapse-site] PR-KV9 junction-twin victim={victim} survivor={survivor}"
                    );
                }
                collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                collapsed_any = true;
            }
        }
    }
    let sweep_result = sweep_reversed_intersections(mesh, &mut attr_vec, a, b, d_eps);
    attribution.attributions = attr_vec;
    let any_collapse = sweep_result?;
    collapsed_any |= any_collapse;

    // (3c) §4.4.1(b) sub-feature-size vertex merge (Yang Fig. 11(b): "if an
    // endpoint p of the split edge is too close to q, we merge p with q"). After
    // relocation a degenerate triangle can have two vertices nearer than
    // MIN_FEATURE_SIZE — the governance feature floor (A14.2): two points closer
    // than the smallest representable feature ARE the same point. This is the
    // curved-input analog of the I6 near-weld (which is bit-exact-only for curved
    // inputs — "Stage-4 owns junction-duplicate collapse"). Merge such a pair via
    // the watertight-preserving `collapse_vertex` (higher index → lower, dropping
    // the now-degenerate slivers), iterating to a fixed point. P9/P10: the gate is
    // the GOVERNANCE feature floor, not a tuned tolerance, and a genuinely-spread
    // degenerate (vertices ≥ the floor apart — e.g. a monotonic-collinear sliver
    // on a curved patch) is left UNTOUCHED for `validate_relocated_triangles` to
    // STOP loudly / the curved-patch re-CDT (N2-2) to handle. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md` §5 increment N2-1.
    //
    // SCOPE NOTE (M8 holed-disc increment 3, 2026-07-06): a GLOBAL widening of
    // this scan (all triangles + a Stage-4 ENTRY pass) was tried and REVERTED —
    // at micro model scale (R0091, 1.6e-4) the ABSOLUTE floor collapses
    // legitimately-distinct arrangement geometry (Euler flipped to −4,
    // SUPPORTED_WRONG). The relocation/conic-adjacent eligibility below is
    // LOAD-BEARING: it keeps the merge away from pre-existing arrangement
    // slivers that `boolean()` legitimately kept for watertightness.
    {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        // KV9-F3 (spec `kv9_f3_output_vertex_identity` E-V2): junction
        // duplicates that are ALREADY on their exact curve (rho ≤ TAU_WORK)
        // are never `moved`, yet they are precisely the population the I6
        // weld delegates to Stage-4 ("Stage-4 owns junction-duplicate
        // collapse" — curved inputs weld bit-exact only). Scan eligibility
        // therefore includes triangles touching any CONIC-ENDPOINT vertex;
        // the merge criterion below is unchanged (the governance
        // MIN_FEATURE_SIZE floor, A14.2 — never a tuned tolerance).
        let conic_endpoint: std::collections::BTreeSet<u32> = vert_circle
            .keys()
            .chain(vert_line.keys())
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_ell_junction.keys())
            .chain(vert_circle_junction.keys())
            .copied()
            .collect();
        // Spec `yang_453_junction_protected_collapse` §3b: closed-form junction
        // vertices (exact on TWO curves) outrank single-curve conic endpoints,
        // which outrank plain mesh vertices, in merge-survivor selection.
        let junction_verts: std::collections::BTreeSet<u32> = vert_ell_junction
            .keys()
            .chain(vert_circle_junction.keys())
            .chain(vert_junction.keys())
            .copied()
            .collect();
        // Each pass collapses ≤1 sub-feature edge; bounded by the triangle count.
        let max_merge_passes = mesh.tris.len() + 1;
        let mut merge_passes = 0usize;
        let mut last_merge: Option<(u32, u32, f64, usize)> = None;
        loop {
            merge_passes += 1;
            if merge_passes > max_merge_passes {
                // §4.4.1(b) diagnosis probe (read-only, env-gated): the budget
                // guard should be unreachable if every pass drops ≥1 triangle
                // — print the terminal state to localize a livelock.
                if std::env::var_os("YANG_S4_MERGE_PROBE").is_some() {
                    eprintln!(
                        "[s4-merge-probe] BUDGET EXHAUSTED: passes={merge_passes} \
                         max={max_merge_passes} tris_now={} last_merge={last_merge:?}",
                        mesh.tris.len()
                    );
                }
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=merge_budget");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::Stage4RegionInvalid {
                    vertex: u32::MAX,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            let mut to_merge: Option<(u32, u32)> = None;
            for tri in &mesh.tris {
                if !tri
                    .iter()
                    .any(|v| moved.contains(v) || conic_endpoint.contains(v))
                {
                    continue;
                }
                let p0 = mesh.verts[tri[0] as usize].as_array();
                let p1 = mesh.verts[tri[1] as usize].as_array();
                let p2 = mesh.verts[tri[2] as usize].as_array();
                let nrm = tri_area_vector(p0, p1, p2);
                let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
                if twice_area * 0.5 >= floor * floor {
                    continue; // not degenerate — leave it
                }
                // Degenerate relocated triangle: if its SHORTEST edge is below the
                // feature floor, those two endpoints are the same point → merge.
                let dist = |a: [f64; 3], b: [f64; 3]| {
                    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                let edges = [
                    (tri[0], tri[1], dist(p0, p1)),
                    (tri[1], tri[2], dist(p1, p2)),
                    (tri[2], tri[0], dist(p2, p0)),
                ];
                let (u, v, len) = edges
                    .iter()
                    .copied()
                    .min_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("3 edges");
                if len < floor {
                    // Spec `yang_453_junction_protected_collapse` §3b: the
                    // exactness-ranked survivor (`sub_feature_merge_direction`,
                    // Yang Fig. 11(b) — the exact vertex survives) is BANKED,
                    // DELIBERATELY UNWIRED: wiring it flips R0091 from its
                    // loud ellipse-endpoint ERROR to SUPPORTED_WRONG
                    // (χ = −4 vs meta 2; unverifiable in-session — see spec
                    // §3b status). The index rule stays until the R0091
                    // output's true χ is verified (sidecar reference parity)
                    // or the meta χ is refuted from the authored numbers.
                    let _ = &junction_verts;
                    let survivor = u.min(v);
                    let victim = u.max(v);
                    to_merge = Some((victim, survivor));
                    break;
                }
            }
            match to_merge {
                Some((victim, survivor)) => {
                    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                        eprintln!(
                            "[collapse-site] s4.4.1b-merge victim={victim} survivor={survivor}"
                        );
                    }
                    let dropped = collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                    last_merge = Some((victim, survivor, dropped as f64, mesh.tris.len()));
                    collapsed_any = true;
                }
                None => break,
            }
        }
        attribution.attributions = attr_vec;
    }

    // (3b′) Coincident RELOCATED-vertex weld (spec `yang_n47_coincident_moved_weld`,
    // deviation N47). Two vertices this pipeline RELOCATED (`moved`: pushed onto an
    // analytic circle/ellipse/line/torus/surface-pair) can converge to within the
    // MODEL coincidence tolerance `TAU_MODEL·(1+scale)` — they are the SAME
    // geometric point emitted twice (a near-tangent seam crossing whose two
    // arrangement points both Newton-project onto one intersection point). The
    // §4.4.1(b) merge above misses them: it scans TRIANGLE edges and gates on the
    // triangle AREA (`floor²`), so a NEEDLE (two coincident verts + one far vert:
    // large area, sub-floor edge) is skipped, and a coincident pair that is only
    // LOOP-adjacent (not a shared triangle edge) is never examined. Left in place,
    // the twins survive into `emit_topology` (vertices 1:1 with `mesh.verts`) as a
    // sub-render-precision output edge, tripping kernel-v2's G1 render-collapse
    // gate FAR downstream (R0012 face 1023 @ 7e-7 / scale 100; R0098 face 599 @
    // 4e-6 / scale 1900). Welding here is a self-localizing PRODUCER fix.
    //
    // Band: the scale-relative MODEL coincidence tolerance `TAU_MODEL·(1+scale)`
    // (`scale` = max |coord| of the pair) — the SAME band the stage-5 planarity
    // wall and every other coincidence test uses; it is 10× TIGHTER than the
    // MIN_FEATURE_SIZE feature floor, so it admits ONLY sub-(feature/10)
    // coincidences (a genuine feature is ≥ `MIN_FEATURE_SIZE·(1+scale)` apart).
    // NOT tolerance widening (P9): it is the model's own definition of "same
    // point," and it only ever COLLAPSES an already-degenerate output edge.
    // Restricted to `moved`×`moved` pairs (the relocation-convergence mechanism) —
    // it never touches un-relocated arrangement geometry `boolean()` kept for
    // watertightness (cf. the §4.4.1(b) micro-scale R0091 revert). `collapse_vertex`
    // is the proven watertight-preserving edge-collapse (with membrane
    // cancellation); iterate to a fixed point over live (still-referenced) verts.
    {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        if weld_coincident_relocated(mesh, &mut attr_vec, &moved) {
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
    }

    // Twin-scan probe (read-only, env-gated `YANG_TWIN_SCAN`): dump every
    // sub-feature-floor mesh edge surviving the §4.4.1(b) merge, with
    // eligibility flags — self-localizes a surviving ULP-twin pair (the
    // F0047 render-collapse diagnosis tool).
    if std::env::var_os("YANG_TWIN_SCAN").is_some() {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, v) = (tri[i].min(tri[j]), tri[i].max(tri[j]));
                if u == v || !seen.insert((u, v)) {
                    continue;
                }
                let pu = mesh.verts[u as usize].as_array();
                let pv = mesh.verts[v as usize].as_array();
                let d = [pu[0] - pv[0], pu[1] - pv[1], pu[2] - pv[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if len < floor {
                    eprintln!(
                        "[twin-scan] edge ({u},{v}) len={len:.3e} \
                         exact_j=({},{}) moved=({},{}) pu={pu:?}",
                        exact_junctions.contains(&u),
                        exact_junctions.contains(&v),
                        moved.contains(&u),
                        moved.contains(&v),
                    );
                }
            }
        }
    }

    // KV9-F1 Increment 0c census: post-merge junction-twin state — coincident
    // junction vertices that SURVIVED the §4.4.1(b) merge, and whether the
    // survivors are edge-adjacent in the current mesh (kept env-gated).
    if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
        let keys: Vec<u32> = vert_ell_junction.keys().copied().collect();
        for (i, &u) in keys.iter().enumerate() {
            for &w in &keys[i + 1..] {
                let (pu, pw) = (mesh.verts[u as usize], mesh.verts[w as usize]);
                if pu.as_array() != pw.as_array() {
                    continue;
                }
                let adjacent = mesh.tris.iter().any(|t| t.contains(&u) && t.contains(&w));
                let (du, dw) = (
                    mesh.tris.iter().filter(|t| t.contains(&u)).count(),
                    mesh.tris.iter().filter(|t| t.contains(&w)).count(),
                );
                eprintln!(
                    "KV9_JUNCTION_PROBE post-merge coincident twins: v{u} v{w} at {:?} \
                     edge_adjacent={adjacent} deg({u})={du} deg({w})={dw}",
                    pu.as_array()
                );
            }
        }
    }

    // (3d) §4.4.1(a) edge-split (Yang Fig. 11(a): "locate the constrained edge
    // containing q, split it at q"). A degenerate relocated triangle D=[a,b,c] is
    // collinear: the vertex OFF its longest edge (`b`) lies on that long edge
    // `a-c` (a redundant intersection point on the constraint curve). The faithful
    // fix inserts `b` into the triangle ON THE OTHER SIDE of `a-c` — split that
    // neighbour N=[a,c,d] into [a,b,d]+[b,c,d] — and drops D. This is a LOCAL,
    // watertight-preserving operation (D's edges a-b/b-c re-pair with the split
    // halves; the long edge a-c, shared only by D and N, vanishes): no re-CDT, no
    // parametric domain, no cylinder θ-seam. Iterate, each step acting on a
    // degenerate triangle whose long-edge neighbour is NON-degenerate (so the
    // strip unzips from its non-degenerate margin inward); a remaining degenerate
    // triangle with no non-degenerate neighbour is a genuine §4.5.2 STOP. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md`.
    {
        let degen_area = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
        let is_degen = |ti: usize, mesh: &Mesh| -> bool {
            let t = mesh.tris[ti];
            if !t.iter().any(|v| moved.contains(v)) {
                return false;
            }
            let av = tri_area_vector(
                mesh.verts[t[0] as usize].as_array(),
                mesh.verts[t[1] as usize].as_array(),
                mesh.verts[t[2] as usize].as_array(),
            );
            (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() * 0.5 < degen_area
        };
        // The off-longest-edge vertex `b` (the collinear middle) + extremes a,c.
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                let e = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2]) // long a-c = v0-v1, off b = v2
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let max_passes = mesh.tris.len() + 1;
        let mut passes = 0usize;
        loop {
            passes += 1;
            if passes > max_passes {
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=split_max_passes");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::Stage4RegionInvalid {
                    vertex: u32::MAX,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            // Edge → incident triangle indices (for the across-edge neighbour).
            let mut edge_tris: std::collections::HashMap<(u32, u32), Vec<u32>> =
                std::collections::HashMap::new();
            for (ti, tri) in mesh.tris.iter().enumerate() {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (u, v) = (tri[i], tri[j]);
                    let key = if u < v { (u, v) } else { (v, u) };
                    edge_tris.entry(key).or_default().push(ti as u32);
                }
            }
            // Pick a degenerate triangle whose long-edge neighbour is non-degenerate.
            let mut action: Option<(usize, usize, u32, u32, u32)> = None;
            let mut any_degen = false;
            for ti in 0..mesh.tris.len() {
                if !is_degen(ti, mesh) {
                    continue;
                }
                any_degen = true;
                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                let key = if a < c { (a, c) } else { (c, a) };
                let inc = match edge_tris.get(&key) {
                    Some(v) if v.len() == 2 => v,
                    _ => continue, // boundary / non-manifold long edge — skip
                };
                let n = if inc[0] as usize == ti {
                    inc[1]
                } else {
                    inc[0]
                } as usize;
                if is_degen(n, mesh) {
                    continue; // defer until the neighbour is resolved
                }
                action = Some((ti, n, a, c, b));
                break;
            }
            let (d_idx, n_idx, a, c, b) = match action {
                Some(x) => x,
                None => {
                    if any_degen {
                        // Degenerate triangles remain but none has a non-degenerate
                        // long-edge neighbour — genuine local-refinement territory.
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            let mut ndeg = 0usize;
                            for ti in 0..mesh.tris.len() {
                                if !is_degen(ti, mesh) {
                                    continue;
                                }
                                ndeg += 1;
                                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                                let surf = attr_vec.get(ti).and_then(|o| o.as_ref()).map(|at| {
                                    let br = match at.input {
                                        InputId::A => brep_a,
                                        InputId::B => brep_b,
                                    };
                                    br.faces()[at.face as usize].surface
                                });
                                eprintln!("YANG_LRR_DEGEN_SURF tri={ti} surface={surf:?}");
                                let key = if a < c { (a, c) } else { (c, a) };
                                let inc = edge_tris.get(&key).map(|v| v.len()).unwrap_or(0);
                                let nbr_degen = edge_tris.get(&key).is_some_and(|v| {
                                    v.iter()
                                        .any(|&n| n as usize != ti && is_degen(n as usize, mesh))
                                });
                                eprintln!(
                                    "YANG_LRR_DEGEN tri={ti} verts={:?} long_edge=({a},{c}) off={b} \
                                     inc_count={inc} nbr_degen={nbr_degen} moved_a={} moved_c={} moved_b={} \
                                     pa={:?} pc={:?} pb={:?}",
                                    mesh.tris[ti],
                                    moved.contains(&a),
                                    moved.contains(&c),
                                    moved.contains(&b),
                                    mesh.verts[a as usize].as_array(),
                                    mesh.verts[c as usize].as_array(),
                                    mesh.verts[b as usize].as_array(),
                                );
                            }
                            eprintln!("YANG_LRR_STOP site=degenerate_no_longedge ndeg={ndeg}");
                            // Grounding: for each attribution carrying a degenerate
                            // triangle, size the same-attribution tri set and count
                            // its boundary edges (undirected edges used exactly once).
                            let mut deg_attrs: std::collections::BTreeSet<(u8, u32)> =
                                std::collections::BTreeSet::new();
                            for ti in 0..mesh.tris.len() {
                                if is_degen(ti, mesh) {
                                    if let Some(at) = attr_vec.get(ti).and_then(|o| o.as_ref()) {
                                        let ik = matches!(at.input, InputId::A) as u8;
                                        deg_attrs.insert((ik, at.face));
                                    }
                                }
                            }
                            for (ik, face) in &deg_attrs {
                                let want = |ti: usize| {
                                    attr_vec.get(ti).and_then(|o| o.as_ref()).is_some_and(|at| {
                                        (matches!(at.input, InputId::A) as u8) == *ik
                                            && at.face == *face
                                    })
                                };
                                let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
                                    .filter(|&t| want(t as usize))
                                    .collect();
                                let mut edge_ct: std::collections::HashMap<(u32, u32), u32> =
                                    std::collections::HashMap::new();
                                let mut ndeg_in = 0usize;
                                for &t in &patch_tris {
                                    if is_degen(t as usize, mesh) {
                                        ndeg_in += 1;
                                    }
                                    let tri = mesh.tris[t as usize];
                                    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                                        let (u, v) = (tri[i], tri[j]);
                                        let k = if u < v { (u, v) } else { (v, u) };
                                        *edge_ct.entry(k).or_insert(0) += 1;
                                    }
                                }
                                let bnd = edge_ct.values().filter(|&&c| c == 1).count();
                                let nonmanifold = edge_ct.values().filter(|&&c| c > 2).count();
                                // θ/z span of the patch in the cylinder frame (if
                                // this face is a Cylinder), to decide seam-wrap.
                                let br = if *ik == 1 { brep_a } else { brep_b };
                                let mut span_str = String::from("(not cylinder)");
                                if let Surface::Cylinder {
                                    axis_point,
                                    axis_dir,
                                    ..
                                } = br.faces()[*face as usize].surface
                                {
                                    let (e1, e2) = ortho_basis(axis_dir);
                                    let au = normalize3(axis_dir.as_array());
                                    let o = axis_point.as_array();
                                    let mut verts_set: std::collections::BTreeSet<u32> =
                                        std::collections::BTreeSet::new();
                                    for &t in &patch_tris {
                                        for &v in &mesh.tris[t as usize] {
                                            verts_set.insert(v);
                                        }
                                    }
                                    let th_ref = {
                                        let p = mesh.verts
                                            [*verts_set.iter().next().unwrap() as usize]
                                            .as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        y.atan2(x)
                                    };
                                    let (mut th_lo, mut th_hi, mut z_lo, mut z_hi) = (
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                    );
                                    for &v in &verts_set {
                                        let p = mesh.verts[v as usize].as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        let z = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
                                        // Unwrap θ near th_ref.
                                        let mut th = y.atan2(x) - th_ref;
                                        while th > std::f64::consts::PI {
                                            th -= 2.0 * std::f64::consts::PI;
                                        }
                                        while th < -std::f64::consts::PI {
                                            th += 2.0 * std::f64::consts::PI;
                                        }
                                        th_lo = th_lo.min(th);
                                        th_hi = th_hi.max(th);
                                        z_lo = z_lo.min(z);
                                        z_hi = z_hi.max(z);
                                    }
                                    span_str = format!(
                                        "theta_span={:.4} (pi={:.4}) z_span={:.4} nverts={}",
                                        th_hi - th_lo,
                                        std::f64::consts::PI,
                                        z_hi - z_lo,
                                        verts_set.len()
                                    );
                                }
                                eprintln!(
                                    "YANG_LRR_PATCH input={} face={face} n_tris={} n_degen={ndeg_in} \
                                     boundary_edges={bnd} nonmanifold_edges={nonmanifold} {span_str}",
                                    if *ik == 1 { "A" } else { "B" },
                                    patch_tris.len()
                                );
                            }
                        }
                        // N2 §4.4.1: try re-meshing degenerate CYLINDER patches
                        // in their (θ,z) parametric domain (keep-interior CDT — no
                        // geometry moves). If it re-meshed, re-scan the loop; the
                        // `max_passes` guard bounds any pathological repeat.
                        //
                        // WIP (task #168): gated OFF by default — the re-CDT fires
                        // and resolves the degenerate cluster, but the seam with the
                        // neighbour patch across the generator is not yet exactly
                        // conformal (R0038 reaches a single unpaired generator edge,
                        // spec §5c.6). Byte-identical to baseline when the env is
                        // unset (production keeps the loud STOP). Enable with
                        // `YANG_N2_RECDT_ENABLE` for development.
                        if std::env::var_os("YANG_N2_RECDT_ENABLE").is_some() {
                            match replan_degenerate_cylinder_patches(
                                mesh,
                                &mut attr_vec,
                                &moved,
                                brep_a,
                                brep_b,
                            ) {
                                Ok(true) => continue,
                                Ok(false) => {}
                                Err(e) => {
                                    attribution.attributions = attr_vec;
                                    return Err(e);
                                }
                            }
                        }
                        attribution.attributions = attr_vec;
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: u32::MAX,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                    break; // no degenerate relocated triangles remain
                }
            };
            // Split N=[a,c,d] at b → [a,b,d] + [b,c,d], wound like N; drop D.
            let nt = mesh.tris[n_idx];
            let dd = nt
                .iter()
                .copied()
                .find(|&v| v != a && v != c)
                .expect("neighbour shares edge a-c, has a third vertex");
            let n_norm = tri_area_vector(
                mesh.verts[nt[0] as usize].as_array(),
                mesh.verts[nt[1] as usize].as_array(),
                mesh.verts[nt[2] as usize].as_array(),
            );
            let mut t1 = [a, b, dd];
            let mut t2 = [b, c, dd];
            orient_tri(&mesh.verts, &mut t1, n_norm);
            orient_tri(&mesh.verts, &mut t2, n_norm);
            let n_attr = attr_vec.get(n_idx).copied().flatten();
            // Rebuild tris + attribution, dropping D and N, appending the split.
            let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len() + 1);
            let mut new_attr: Vec<Option<TriangleAttribution>> =
                Vec::with_capacity(attr_vec.len() + 1);
            for (i, t) in mesh.tris.iter().enumerate() {
                if i == d_idx || i == n_idx {
                    continue;
                }
                new_tris.push(*t);
                new_attr.push(attr_vec.get(i).copied().flatten());
            }
            new_tris.push(t1);
            new_attr.push(n_attr);
            new_tris.push(t2);
            new_attr.push(n_attr);
            *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
            attr_vec = new_attr;
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
    }

    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin mesh
    // vertex pairs at Stage-4 exit with their merge-eligibility context —
    // `moved` membership, shared-triangle adjacency, curve assignments.
    if std::env::var_os("YANG_S4_TWIN_PROBE").is_some() {
        let n = mesh.verts.len();
        let scale = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..n {
            for j in (i + 1)..n {
                let (p, q) = (mesh.verts[i].as_array(), mesh.verts[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 > band * band || d2 == 0.0 {
                    continue;
                }
                let (iu, ju) = (i as u32, j as u32);
                let shared_tri = mesh
                    .tris
                    .iter()
                    .position(|t| t.contains(&iu) && t.contains(&ju));
                eprintln!(
                    "[s4-twin-probe] verts {i}/{j} dist={:e} moved=({},{}) shared_tri={:?}\n  \
                     circle=({},{}) line=({},{}) ell=({},{}) junction=({},{})\n  \
                     {i}: ({},{},{})\n  {j}: ({},{},{})",
                    d2.sqrt(),
                    moved.contains(&iu),
                    moved.contains(&ju),
                    shared_tri,
                    vert_circle.contains_key(&iu),
                    vert_circle.contains_key(&ju),
                    vert_line.contains_key(&iu),
                    vert_line.contains_key(&ju),
                    vert_ellipse.contains_key(&iu),
                    vert_ellipse.contains_key(&ju),
                    vert_ell_junction.contains_key(&iu),
                    vert_ell_junction.contains_key(&ju),
                    p[0],
                    p[1],
                    p[2],
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    // (4) Validate every RELOCATED triangle (one touching a moved vertex) for
    // non-degeneracy (Yang §4.5 step 4). Reversed intersections are handled by
    // the §4.5.3 sweep above; watertightness by the global gate below (§4.4.3).
    validate_relocated_triangles(mesh, attribution, &moved)?;
    // (4a1) Doubled-membrane removal (spec `yang_doubled_membrane_removal.md`,
    // task #146 χ=3 sub-layer): drop opposite-winding coincident-triangle fins
    // (a zero-volume artifact of a backtrack-spike / near-tangent junction)
    // BEFORE the shell gate reads χ. Volume- and edge-balance-preserving; it
    // leaves the spur apex dangling for `compact_unreferenced_verts`, so it
    // rides the same Phase-A recompute path as a §4.5.3 collapse.
    let membranes_removed = remove_doubled_membranes(mesh);
    if membranes_removed > 0 {
        collapsed_any = true;
    }
    // (4a2) Tangency pinch-vertex split (spec `yang_tangency_pinch_split.md`):
    // uniform per-sheet representation of self-touching union boundaries
    // BEFORE the shell gate reads χ. Splitting appends vertices (a topology
    // change), so it rides the same Phase-A recompute path as a §4.5.3
    // collapse via the returned flag.
    let pinch_splits = split_pinch_vertices(mesh, &mut relocations);
    if pinch_splits > 0 {
        collapsed_any = true;
    }
    // (4b) Explicit Stage-4 watertightness gate (§4.4.3).
    check_watertight_2manifold(mesh)?;

    // After a collapse the vertex set may have lost some relocated verts; keep
    // only relocations whose vertex still carries a conic output edge. The
    // caller resolves the output-edge index; relocations referencing a
    // now-absent vertex are simply not emitted (the caller guards the index).
    Ok((relocations, collapsed_any))
}

mod reversal;
pub(crate) use reversal::*;

mod validate;
pub(crate) use validate::*;

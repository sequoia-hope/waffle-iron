//! Conformal-mesh measurement oracle (Yang 2025 §4.4.3 / Cherchi 2020 §5).
//!
//! ## Purpose (PR-Y14a)
//!
//! Pure measurement: given any `(verts, tris)` triangle mesh produced
//! anywhere in the Yang pipeline, answer the single question
//! **"is this a well-formed simplicial complex?"** Reports every
//! directed edge that lacks its reverse counterpart and every directed
//! edge appearing with unexpected multiplicity, plus the Euler
//! characteristic.
//!
//! This oracle is consumed by three call-site probes (Stages 2, 4, 6)
//! installed by PR-Y14a and gated on the `YANG_CONFORMAL_PROBE=1`
//! env var. The probes are observation-only — this oracle never
//! mutates state, never panics, and never logs.
//!
//! ## Spec
//!
//! `specs/yang_conformal_mesh_oracle.md` is the API contract. The
//! types and function signature in this file MUST match that spec
//! exactly.
//!
//! ## Status
//!
//! STUB. Tests in this file are RED-PHASE: they assert the contract
//! and currently fail because `check_conformal` is `unimplemented!()`.
//! The implementer (PR-Y14a Phase 2) replaces the stub with a real
//! body and the tests turn green.

// ── Public types (per spec §"Public types") ────────────────────────────

/// Result of a conformality check. Always inspectable; `is_well_formed`
/// is the single boolean predicate downstream probes log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformalReport {
    /// Directed edges `(v0, v1)` for which no triangle contains the
    /// reverse `(v1, v0)`. v0/v1 are CANONICAL vertex indices (post
    /// nanometer-quantize), not raw indices into `verts`.
    pub unpaired_directed_edges: Vec<UnpairedEdge>,

    /// Directed edges that appear in more than one triangle in the same
    /// direction (or whose fwd/rev counts disagree by >1).
    pub multi_paired_edges: Vec<MultiPairedEdge>,

    /// V − E + F over canonical vertices, unique undirected edges, and
    /// triangles. For a closed orientable manifold mesh consisting of
    /// `k` disjoint shells, equals `2 * k`.
    pub euler_characteristic: i64,

    /// Number of canonical (post-quantize) vertices actually referenced
    /// by `tris`. Vertices in `verts` not referenced by any triangle do
    /// not count.
    pub vertex_count: usize,

    /// `tris.len()`.
    pub triangle_count: usize,

    /// Count of unique undirected edges (i.e. unordered `{v0, v1}`
    /// pairs) over all triangles.
    pub unique_undirected_edge_count: usize,

    /// `unpaired_directed_edges.is_empty() && multi_paired_edges.is_empty()`.
    pub is_well_formed: bool,
}

/// Directed edge `(v0, v1)` lacking a reverse partner in any triangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedEdge {
    pub v0: usize,
    pub v1: usize,
    /// Indices into the input `tris` slice that contain `(v0, v1)` as a
    /// directed edge.
    pub source_tris: Vec<usize>,
}

/// Directed edge whose forward / reverse multiplicities exceed the
/// 1-fwd + (0 or 1)-rev pattern that conformality permits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiPairedEdge {
    pub v0: usize,
    pub v1: usize,
    /// Triangles containing `(v0, v1)` as a directed edge.
    pub fwd_tris: Vec<usize>,
    /// Triangles containing `(v1, v0)` as a directed edge.
    pub rev_tris: Vec<usize>,
}

// ── Public function (per spec §"Public function") ──────────────────────

/// Measure conformality of a triangle mesh.
///
/// See `specs/yang_conformal_mesh_oracle.md` for the full contract.
/// Pure: no logging, no panics on degenerate input, no global state.
/// Empty input returns a trivially well-formed report. Vertices are
/// canonical-quantized internally at nanometer precision
/// (`crate::units::QUANT_NANOMETER_SCALE`).
pub fn check_conformal(verts: &[[f64; 3]], tris: &[[usize; 3]]) -> ConformalReport {
    use std::collections::BTreeMap;

    if tris.is_empty() {
        return ConformalReport {
            unpaired_directed_edges: Vec::new(),
            multi_paired_edges: Vec::new(),
            euler_characteristic: 0,
            vertex_count: 0,
            triangle_count: 0,
            unique_undirected_edge_count: 0,
            is_well_formed: true,
        };
    }

    // Canonical-quantize verts at nanometer precision. Inlined verbatim
    // from `topology_extract.rs:375-393` per spec §"Canonical-quantize
    // policy" — the oracle MUST produce byte-identical canonicalization
    // to the downstream code that consumes the same mesh.
    //
    // Invariant: must match topology_extract.rs:375-393.
    let quant = |p: [f64; 3]| -> [i64; 3] {
        let scale = crate::units::QUANT_NANOMETER_SCALE;
        [
            (p[0] * scale).round() as i64,
            (p[1] * scale).round() as i64,
            (p[2] * scale).round() as i64,
        ]
    };
    let mut mesh_to_canon: BTreeMap<usize, usize> = BTreeMap::new();
    let mut pos_to_canon: BTreeMap<[i64; 3], usize> = BTreeMap::new();
    let mut next_canon: usize = 0;

    for (vi, pos) in verts.iter().enumerate() {
        let qp = quant(*pos);
        let canon = *pos_to_canon.entry(qp).or_insert_with(|| {
            let c = next_canon;
            next_canon += 1;
            c
        });
        mesh_to_canon.insert(vi, canon);
    }

    // Out-of-range vertex indices are not in `verts`; we synthesize a
    // distinct canonical key for each so they appear in edge accounting
    // (without panicking on indexing). Per spec §"Failure modes":
    // "treats the index as its own canonical key (i.e., does not crash)".
    let mut oob_to_canon: BTreeMap<usize, usize> = BTreeMap::new();
    let mut canon_v = |raw: usize| -> usize {
        if let Some(&c) = mesh_to_canon.get(&raw) {
            c
        } else {
            *oob_to_canon.entry(raw).or_insert_with(|| {
                let c = next_canon;
                next_canon += 1;
                c
            })
        }
    };

    // Build directed-edge multimap (canonical (v0,v1) -> source tri indices).
    let mut directed: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    let mut undirected: std::collections::BTreeSet<(usize, usize)> =
        std::collections::BTreeSet::new();
    let mut referenced: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    for (ti, tri) in tris.iter().enumerate() {
        let cv0 = canon_v(tri[0]);
        let cv1 = canon_v(tri[1]);
        let cv2 = canon_v(tri[2]);
        referenced.insert(cv0);
        referenced.insert(cv1);
        referenced.insert(cv2);
        for &(a, b) in &[(cv0, cv1), (cv1, cv2), (cv2, cv0)] {
            directed.entry((a, b)).or_default().push(ti);
            let ue = if a <= b { (a, b) } else { (b, a) };
            undirected.insert(ue);
        }
    }

    // Walk directed edges. For each (a,b), inspect (b,a) once when a<=b
    // so we don't double-process. For each pair, classify:
    //   - fwd present, rev absent → unpaired (record fwd side; if a==b
    //     the self-loop is its own reverse, handled below).
    //   - fwd absent, rev present → unpaired (record rev side as
    //     directed (b,a) with no reverse).
    //   - fwd.len() != 1 || rev.len() != 1 → multi-paired.
    let mut unpaired_directed_edges: Vec<UnpairedEdge> = Vec::new();
    let mut multi_paired_edges: Vec<MultiPairedEdge> = Vec::new();
    let mut visited: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();

    for (&(a, b), fwd_tris) in &directed {
        if visited.contains(&(a, b)) {
            continue;
        }
        let rev_tris = directed.get(&(b, a)).cloned().unwrap_or_default();
        let fwd_count = fwd_tris.len();
        let rev_count = rev_tris.len();

        // Self-loop (a == b): degenerate edge; report under multi-paired
        // since the directed (a,a) is its own reverse (unusual multiplicity
        // pattern). Per spec §"Failure modes": "(0,0) self-loop... the
        // oracle reports it (likely under multi_paired_edges)".
        if a == b {
            multi_paired_edges.push(MultiPairedEdge {
                v0: a,
                v1: b,
                fwd_tris: fwd_tris.clone(),
                rev_tris: fwd_tris.clone(),
            });
            visited.insert((a, b));
            continue;
        }

        match (fwd_count, rev_count) {
            (1, 1) => { /* manifold interior — well-formed */ }
            (1, 0) => {
                // Boundary edge — fwd present, rev missing.
                unpaired_directed_edges.push(UnpairedEdge {
                    v0: a,
                    v1: b,
                    source_tris: fwd_tris.clone(),
                });
            }
            (0, 1) => {
                // Should not occur via this iteration order (we'd have
                // processed (b,a) entry from the map). Defensive — emit
                // the rev side as the unpaired directed edge.
                unpaired_directed_edges.push(UnpairedEdge {
                    v0: b,
                    v1: a,
                    source_tris: rev_tris.clone(),
                });
            }
            _ => {
                // Multi-paired: fwd_count > 1 OR rev_count > 1 OR
                // fwd_count == 0 (rev > 1) etc. Report once.
                multi_paired_edges.push(MultiPairedEdge {
                    v0: a,
                    v1: b,
                    fwd_tris: fwd_tris.clone(),
                    rev_tris: rev_tris.clone(),
                });
                // Also record any unpaired side from the multi case
                // (e.g. fwd=2, rev=0 → both fwd entries are unpaired).
                if rev_count == 0 {
                    unpaired_directed_edges.push(UnpairedEdge {
                        v0: a,
                        v1: b,
                        source_tris: fwd_tris.clone(),
                    });
                }
                if fwd_count == 0 {
                    unpaired_directed_edges.push(UnpairedEdge {
                        v0: b,
                        v1: a,
                        source_tris: rev_tris.clone(),
                    });
                }
            }
        }
        visited.insert((a, b));
        visited.insert((b, a));
    }

    // Deterministic ordering per spec §"Purity, panic, and allocation
    // contract": sort by (v0, v1) ascending.
    unpaired_directed_edges.sort_by_key(|e| (e.v0, e.v1));
    multi_paired_edges.sort_by_key(|e| (e.v0, e.v1));

    let vertex_count = referenced.len();
    let triangle_count = tris.len();
    let unique_undirected_edge_count = undirected.len();
    let euler_characteristic =
        vertex_count as i64 - unique_undirected_edge_count as i64 + triangle_count as i64;
    let is_well_formed = unpaired_directed_edges.is_empty() && multi_paired_edges.is_empty();

    ConformalReport {
        unpaired_directed_edges,
        multi_paired_edges,
        euler_characteristic,
        vertex_count,
        triangle_count,
        unique_undirected_edge_count,
        is_well_formed,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical unit cube — 8 verts (corners of [0,1]³), 12 triangles
    /// with consistent winding. Every undirected edge is shared by
    /// exactly 2 triangles. χ = V − E + F = 8 − 18 + 12 = 2.
    fn unit_cube() -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        // Vertices indexed bit-packed (x, y, z) ∈ {0,1}³.
        let verts = vec![
            [0.0, 0.0, 0.0], // 0 (0,0,0)
            [1.0, 0.0, 0.0], // 1 (1,0,0)
            [0.0, 1.0, 0.0], // 2 (0,1,0)
            [1.0, 1.0, 0.0], // 3 (1,1,0)
            [0.0, 0.0, 1.0], // 4 (0,0,1)
            [1.0, 0.0, 1.0], // 5 (1,0,1)
            [0.0, 1.0, 1.0], // 6 (0,1,1)
            [1.0, 1.0, 1.0], // 7 (1,1,1)
        ];
        // 12 triangles — same indexing scheme as
        // arrangement_wellformed.rs's two_disjoint_cubes() fixture
        // (proven manifold by that test).
        let tris = vec![
            // -Z face (z=0 verts: 0,1,2,3)
            [0, 2, 1],
            [1, 2, 3],
            // +Z face (z=1 verts: 4,5,6,7)
            [4, 5, 6],
            [5, 7, 6],
            // -Y face (y=0 verts: 0,1,4,5)
            [0, 1, 4],
            [1, 5, 4],
            // +Y face (y=1 verts: 2,3,6,7)
            [2, 6, 3],
            [3, 6, 7],
            // -X face (x=0 verts: 0,2,4,6)
            [0, 4, 2],
            [2, 4, 6],
            // +X face (x=1 verts: 1,3,5,7)
            [1, 3, 5],
            [3, 7, 5],
        ];
        (verts, tris)
    }

    #[test]
    fn cube_well_formed() {
        let (verts, tris) = unit_cube();
        let report = check_conformal(&verts, &tris);

        assert!(
            report.is_well_formed,
            "unit cube should be well-formed; report = {report:#?}"
        );
        assert!(
            report.unpaired_directed_edges.is_empty(),
            "unit cube: no unpaired directed edges expected, got {:?}",
            report.unpaired_directed_edges
        );
        assert!(
            report.multi_paired_edges.is_empty(),
            "unit cube: no multi-paired edges expected, got {:?}",
            report.multi_paired_edges
        );
        assert_eq!(report.vertex_count, 8, "cube has 8 unique canonical verts");
        assert_eq!(report.triangle_count, 12, "cube has 12 triangles");
        assert_eq!(
            report.unique_undirected_edge_count, 18,
            "cube has 18 unique undirected edges (12 face diagonals... actually 12 cube edges + 6 face-diag splits = 18)"
        );
        assert_eq!(
            report.euler_characteristic, 2,
            "closed orientable manifold sphere has χ = 2"
        );
        // I1: is_well_formed memoizes the conjunction.
        assert_eq!(
            report.is_well_formed,
            report.unpaired_directed_edges.is_empty() && report.multi_paired_edges.is_empty(),
            "I1 invariant: is_well_formed must equal the conjunction"
        );
    }

    #[test]
    fn cube_one_tri_flipped() {
        // Build a cube, then flip the winding of triangle index 0
        // (was [0, 2, 1] on the -Z face). The new triangle is
        // [0, 1, 2]: its directed edges are (0,1), (1,2), (2,0) —
        // each was the REVERSE direction in the original mesh, so
        // they now "double up" with the original (0,1)→(1,2)→(2,0)
        // edges from sibling triangle [1, 2, 3] and the +X-face
        // chain. The flipped tri's edges (1,0), (2,1), (0,2) all
        // disappear from the directed-edge multiset — the reverses
        // of those edges (0,1), (1,2), (2,0) are now unpaired.
        let (verts, mut tris) = unit_cube();
        tris[0] = [0, 1, 2]; // was [0, 2, 1]

        let report = check_conformal(&verts, &tris);

        assert!(
            !report.is_well_formed,
            "cube with one flipped triangle must NOT be well-formed; report = {report:#?}"
        );
        // Per spec test case #2: the test author asserts a
        // DISJUNCTION on counts, not exact counts (the doubled-fwd
        // and unpaired-rev partition is adjacency-dependent).
        assert!(
            !report.unpaired_directed_edges.is_empty() || !report.multi_paired_edges.is_empty(),
            "flipped triangle must produce at least one unpaired or multi-paired edge"
        );
        // The flipped triangle (index 0) MUST appear as a source in
        // at least one violation (either as fwd or as the source of
        // an unpaired edge). This pins the provenance contract.
        let tri0_in_unpaired = report
            .unpaired_directed_edges
            .iter()
            .any(|e| e.source_tris.contains(&0));
        let tri0_in_multi = report
            .multi_paired_edges
            .iter()
            .any(|e| e.fwd_tris.contains(&0) || e.rev_tris.contains(&0));
        assert!(
            tri0_in_unpaired || tri0_in_multi,
            "flipped triangle (index 0) must appear in at least one violation's source_tris/fwd_tris/rev_tris"
        );
        // Triangle count and vertex count are unchanged.
        assert_eq!(report.triangle_count, 12);
        assert_eq!(report.vertex_count, 8);
        // I1: is_well_formed is the conjunction.
        assert_eq!(
            report.is_well_formed,
            report.unpaired_directed_edges.is_empty() && report.multi_paired_edges.is_empty()
        );
    }

    #[test]
    fn two_disconnected_cubes() {
        // Cube A at origin, Cube B translated to (10, 0, 0). No
        // shared vertices, no shared edges. Each cube is closed
        // orientable manifold (χ = 2), so χ_total = 4. (Spec I2:
        // 2 * k where k = 2 connected shells.)
        let (verts_a, tris_a) = unit_cube();
        let mut verts = verts_a.clone();
        for v in &verts_a {
            verts.push([v[0] + 10.0, v[1], v[2]]);
        }
        let mut tris = tris_a.clone();
        for t in &tris_a {
            tris.push([t[0] + 8, t[1] + 8, t[2] + 8]);
        }

        let report = check_conformal(&verts, &tris);

        assert!(
            report.is_well_formed,
            "two disjoint closed cubes must be well-formed; report = {report:#?}"
        );
        assert_eq!(report.vertex_count, 16, "16 canonical verts (8 + 8)");
        assert_eq!(report.triangle_count, 24, "24 tris (12 + 12)");
        assert_eq!(
            report.unique_undirected_edge_count, 36,
            "36 unique undirected edges (18 + 18)"
        );
        assert_eq!(
            report.euler_characteristic, 4,
            "I2: two disjoint shells → χ = 2 * 2 = 4"
        );
    }

    #[test]
    fn empty_mesh() {
        let report = check_conformal(&[], &[]);

        assert!(report.is_well_formed, "empty mesh is trivially well-formed");
        assert!(report.unpaired_directed_edges.is_empty());
        assert!(report.multi_paired_edges.is_empty());
        assert_eq!(report.vertex_count, 0);
        assert_eq!(report.triangle_count, 0);
        assert_eq!(report.unique_undirected_edge_count, 0);
        assert_eq!(report.euler_characteristic, 0);
    }

    #[test]
    fn degenerate_triangle() {
        // Triangle with two equal vertex indices: directed edges
        // are (0,0), (0,2), (2,0). The (0,0) self-loop is an
        // unusual multiplicity pattern — per spec §"Failure modes",
        // the oracle reports it (likely under multi_paired_edges)
        // and does NOT panic.
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let tris = vec![[0, 0, 2]];

        let report = check_conformal(&verts, &tris);

        assert!(
            !report.is_well_formed,
            "degenerate triangle is not well-formed"
        );
        // Spec contract: the oracle reports the triangle, does NOT
        // panic. Concrete count assertions are deferred (spec §"Failure
        // modes" leaves the partition between unpaired vs multi_paired
        // to the implementer's choice for self-loops).
        assert!(
            !report.unpaired_directed_edges.is_empty() || !report.multi_paired_edges.is_empty(),
            "degenerate triangle must produce at least one violation"
        );
        assert_eq!(report.triangle_count, 1);
    }

    #[test]
    fn duplicate_vertex_canonical_collapse() {
        // Two vertices at byte-identical positions, used by separate
        // triangles. After canonical-quantize they collapse to ONE
        // canonical vertex, and the resulting two triangles share
        // that vertex. Build a tiny closed shell where this matters:
        // a tetrahedron whose vertex 3 is duplicated as vertex 4 at
        // the SAME position. After canonicalization, the mesh
        // behaves as a 4-vertex tetrahedron (V=4, E=6, F=4, χ=2).
        let verts = vec![
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [0.0, 1.0, 0.0], // 2
            [0.0, 0.0, 1.0], // 3
            [0.0, 0.0, 1.0], // 4 — byte-identical to vertex 3
        ];
        // Replace some references to vertex 3 with vertex 4. After
        // canonicalization, both should fold to the same canonical
        // index, restoring the canonical tetrahedron's manifold
        // property.
        let tris = vec![
            [0, 2, 1], // -Z base
            [0, 1, 4], // +X face — uses dup vertex 4 instead of 3
            [1, 2, 3], // +Y face — uses original vertex 3
            [0, 3, 2], // +X-ish face — uses original vertex 3
        ];

        let report = check_conformal(&verts, &tris);

        // After canonical-quantize, vertex 4 collapses onto vertex 3,
        // leaving 4 unique canonical vertices.
        assert_eq!(
            report.vertex_count, 4,
            "byte-identical vertices must collapse under canonical-quantize; got {} canonical verts",
            report.vertex_count
        );
        assert_eq!(report.triangle_count, 4);
        // The collapsed mesh is a closed tetrahedron — should be
        // well-formed (χ = 4 - 6 + 4 = 2).
        assert!(
            report.is_well_formed,
            "after canonical-collapse the tetrahedron should be well-formed; report = {report:#?}"
        );
        assert_eq!(report.euler_characteristic, 2);
        assert_eq!(report.unique_undirected_edge_count, 6);
    }

    #[test]
    fn out_of_range_index() {
        // Triangle references a vertex index past `verts.len()`. Per
        // spec §"Failure modes": oracle returns a report (does NOT
        // panic). The recovery is implementer's choice (skip the tri
        // OR report it via the multi-paired path); the test asserts
        // only "no panic + report exists".
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        // Index 99 is out of range (verts.len() == 2).
        let tris = vec![[0, 1, 99]];

        // The KEY assertion is "does not panic". If the call panics,
        // this test fails before the assertion — that's the
        // expected RED-phase mode (the panic-free guarantee is part
        // of the contract).
        let report = check_conformal(&verts, &tris);

        // Beyond no-panic: the report must be coherent. tris.len()
        // is the literal value passed in.
        assert_eq!(report.triangle_count, 1);
        // Spec says "no panic" is the only hard requirement. The tri
        // SHOULD surface as a violation (either unpaired edges from
        // the malformed tri, or recorded under multi_paired_edges).
        assert!(
            !report.is_well_formed,
            "out-of-range vertex index must not produce a 'well-formed' verdict"
        );
    }

    #[test]
    fn mutation_well_formed_field() {
        // Mutation sanity (spec §"Oracles for the Oracle's Own Tests"
        // case 5): build a known well-formed cube, run the oracle,
        // confirm `is_well_formed = true`. Then construct a mutated
        // mesh that differs in one triangle's winding and confirm
        // `is_well_formed` flips to `false`. This catches "what if
        // is_well_formed is hardcoded to true?" — the field MUST
        // depend on actual input.
        let (verts, tris) = unit_cube();
        let baseline = check_conformal(&verts, &tris);
        assert!(
            baseline.is_well_formed,
            "baseline cube must be well-formed for mutation test to make sense"
        );

        // Mutation: flip the winding of triangle index 7 (was
        // [3, 6, 7] on the +Y face). The new winding [3, 7, 6]
        // produces edges (3,7), (7,6), (6,3) — directly reversing
        // the triangle's contribution.
        let mut tris_mut = tris.clone();
        tris_mut[7] = [3, 7, 6]; // was [3, 6, 7]
        let mutated = check_conformal(&verts, &tris_mut);

        assert!(
            !mutated.is_well_formed,
            "mutated cube must NOT be well-formed; if this passes, is_well_formed is hardcoded — report = {mutated:#?}"
        );
        // Sanity: the two reports must differ.
        assert_ne!(
            baseline, mutated,
            "baseline and mutated reports must not compare equal"
        );
    }
}

# Spec: Cherchi 2020 Indirect Predicates for Mesh Arrangement

## Paper Lineage and Codebase Map

This spec covers **only the Cherchi 2020 indirect predicates** (#9 in
`REFERENCES.md`). Two additional papers from the same author group constitute
the rest of the mesh-Boolean stack we depend on, and citations across the
codebase have historically conflated them. This section establishes the
canonical disambiguation.

### Three Distinct Papers

| Short label             | Full reference                                                                                                                      | Local doc                                                       | Where it sits in the pipeline                                                                                                                                                                  |
|-------------------------|-------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Cherchi 2020** (#9)   | Cherchi, Livesu, Scateni & Attene. *Fast and Robust Mesh Arrangements using Floating-point Arithmetic.* SIGGRAPH Asia 2020.         | `docs/references/cherchi-indirect-predicates-2020.md`           | **Predicates + arrangement.** Sections 4.1–4.3 define the indirect predicates (E/L/T points, orient2d/3d, pointCompare). Sections 5.1–5.4 define the *single-mesh* arrangement algorithm.       |
| **Livesu et al. 2021**  | Livesu, Cherchi, Scateni & Attene. *Deterministic Linear Time Constrained Triangulation Using Simplified Earcut.* IEEE TVCG.        | `docs/references/livesu-cherchi-cdt-2022.md`                    | **Linear-time CDT.** Replaces the O(n²) earcut used in the original Cherchi 2020 segment-insertion step. (Sometimes referred to in our older plans as "Livesu & Cherchi 2022" — same paper.)   |
| **Cherchi 2022**        | Cherchi, Pellacini, Attene & Livesu. *Interactive and Robust Mesh Booleans.* SIGGRAPH Asia 2022. arXiv:2205.14151.                  | `docs/references/cherchi-interactive-booleans-2022.md`          | **Full Boolean pipeline.** Reuses Cherchi 2020 arrangement (with speed-ups + Livesu 2021 CDT) and adds the new exact ray-cast inside/outside classification (Algorithm 1, §5). **Yang 2025 §4.2 cites this paper for mesh intersection and §4.4.2 for in/out classification.** |

### Citation Hygiene Rules

When writing or updating a citation in **code, specs, plans, or governance docs**:

1. **Indirect predicates / E-L-T points / filter constants / arrangement step
   (intersection-detection → point-insertion → segment-insertion → coplanar
   pockets)** → cite **Cherchi 2020** (#9). This spec is the local authority.
2. **Linear-time CDT used in segment insertion (`earcut_linear`)** → cite
   **Livesu et al. 2021**. (The Cherchi 2022 paper substituted this in for the
   original earcut; the algorithm itself is from the 2021 paper.)
3. **Whole Boolean pipeline (arrangement + filtering by Boolean op)** or the
   **per-patch ray-cast inside/outside classification (Algorithm 1)** →
   cite **Cherchi 2022** (the new doc-id). For Yang stage 2 / stage 4a, this is
   the correct primary citation.
4. **`Cherchi §5.4` references in code** → these almost always mean
   **Cherchi 2020 §5.4** (coplanar / cosurface-orientation pocket map).
   Cherchi 2022 §5.4 is the *Section 6 lead-in*, not coplanar handling. When
   in doubt, use the explicit form `Cherchi 2020 §5.4` for clarity.
5. **`Cherchi 2022; Livesu 2019` (Yang's citation in §4.2)** — the "Livesu
   2019" half is **CinoLib** (the C++ data-structure library), not an
   algorithmic paper. When porting Yang stage 2, cite **Cherchi 2022 + Cherchi
   2020 (predicates) + Livesu et al. 2021 (CDT)**, not the CinoLib library.

### Codebase Map

```
crates/kernel/src/boolean/
├── indirect_predicates.rs      ← Cherchi 2020 §4.1–4.3 (this spec)
├── cherchi/
│   ├── mod.rs                  ← Cherchi 2020 arrangement entry point
│   │                             (with Cherchi 2022 §4 speedups where applicable)
│   ├── triangulation.rs
│   │   ├── earcut_linear()     ← Livesu et al. 2021 simplified earcut
│   │   └── earcut()            ← classical earcut (Eberly 2008) fallback
│   ├── intersection_class.rs   ← Cherchi 2020 §5.1 intersection classification
│   ├── processing.rs           ← Cherchi 2020 §5.4 coplanar handling
│   ├── tree.rs                 ← Cherchi 2020 §5.1 KdTree (with Cherchi 2022
│   │                             §4 octree improvements where ported)
│   ├── triangle_soup.rs        ← Cherchi 2020 §5.0 triangle soup input
│   ├── fast_trimesh.rs         ← Cherchi 2020 §5.2-5.3 mesh data structure
│   ├── aux_structure.rs        ← Cherchi 2020 §5.4 auxiliary tetrahedron
│   └── common.rs               ← shared types
└── exact_mesh.rs
    ├── label_sub_tri_raycast() ← Cherchi 2022 §5 / Algorithm 1 ray-cast
    │                             (with Hoffmann 1989 §5.3 perturbation
    │                             fallback for boundary-coincident faces)
    └── label_cells()           ← Yang 2025 §4.4 / Cherchi 2022 §5
                                  per-patch in/out classification
```

The `Copyright (c) 2022` headers on `triangulation.rs`,
`intersection_class.rs`, and `processing.rs` are the **MIT license** of the
[InteractiveAndRobustMeshBooleans](https://github.com/gcherchi/InteractiveAndRobustMeshBooleans)
codebase from which they were ported. The license year reflects the source
repository's copyright, not which algorithmic paper introduced the contents.
The algorithmic citations on each function should follow the rules in the
Citation Hygiene section above.

### Cross-References

- For the Cherchi 2022 Boolean pipeline as a whole, see
  `specs/cherchi_2022_boolean_pipeline.md`.
- For the Yang hybrid pipeline that integrates Cherchi 2022 as stage 2, see
  Architectural Invariant **A15.6** in
  `governance/ARCHITECTURAL_INVARIANTS.md`.
- For the indirect-predicate spec details (the rest of this document), see
  the Goal section below.

---

## Goal

Implement indirect geometric predicates per Cherchi et al. 2020 [#9] Sections 4.1-4.3.
These are the mathematical foundation for exact mesh arrangement — all geometric
decisions (orientation, point comparison, point-in-triangle) operate on implicit
intersection point representations without materializing coordinates.

## Research Basis

- Cherchi et al. 2020 [#9]: "Fast and Robust Mesh Arrangements using Floating-point Arithmetic"
- Local reference: `docs/references/cherchi-indirect-predicates-2020.md`
- Filter constants from Table 1 (Section 4.2.2)
- Point comparison from Section 4.3

## Implicit Point Types (Section 4.1)

### E — Explicit Point
Input vertex with known f64 coordinates. Trivial.

### L — Line-Plane Intersection (LPI)
Edge (q1, q2) intersects plane defined by triangle (r, s, t).
5 defining points. Coordinates are:
```
p_L = (λ_Lx/d_L, λ_Ly/d_L, λ_Lz/d_L)
d_L = det|(q1-q2), (s-r), (t-r)|
n = det|(q1-r), (s-r), (t-r)|
λ_Lx = d_L·q1x + n·(q2x - q1x)
```
Undefined if d_L = 0 (edge parallel to plane).

### T — Three-Plane Intersection (TPI)
Three non-coplanar triangles define a unique intersection point.
9 defining points (3 triangles × 3 vertices). Coordinates via 3×3 linear system.
Undefined if d_T = 0 (planes not linearly independent).

## Orient2d Predicates (Section 4.2)

10 variants for all E/L/T combinations:

| Variant | Points | Filter epsilon power | Implementation status |
|---------|--------|---------------------|-----------------------|
| EEE | E, E, E | 8.88e-16 · δ² | filtered + exact (delegates to `geometry_predicates::orient2d`) |
| LEE | L, E, E | 4.75e-14 · δ⁵ | filtered + exact |
| LLE | L, L, E | 1.70e-11 · δ¹¹ | exact-only (expansion) |
| LLL | L, L, L | 1.76e-10 · δ¹⁴ | exact-only (expansion) |
| TEE | T, E, E | 9.06e-13 · δ⁸ | exact-only (expansion) — Phase B (this PR) |
| LTE | L, T, E | 2.18e-10 · δ¹⁴ | exact-only (expansion) — Phase B (this PR) |
| LLT | L, L, T | 2.14e-9 · δ¹⁷ | exact-only (expansion) — Phase B (this PR) |
| LTT | L, T, T | 2.54e-8 · δ²⁰ | exact-only (expansion) — Phase B (this PR) |
| TTE | T, T, E | 3.31e-8 · δ²⁰ | exact-only (expansion) — Phase B (this PR) |
| TTT | T, T, T | 3.10e-6 · δ²⁶ | exact-only (expansion) — Phase B (this PR) |

Each operates on a 2D projection (drop one coordinate axis).

Dispatch over (E,L,T)³ is exhaustive; PR2 telemetry verified zero
materialize-fallback hits across the yang_fast assay corpus, and the prior
fallback was deleted. Reaching the dispatcher's `_` arm now panics via
`unreachable!()`.

## Orient3d Predicates (Section 4.2)

15 base variants for all E/L/T combinations on 4 points (full ordered table is
27 × 3 = 81 ordered cases; permutations are folded by sign-tracked dispatch
onto the 15 multisets).

| Variant | Points | Implementation status |
|---------|--------|-----------------------|
| EEEE | E, E, E, E | filtered + exact (delegates to `geometry_predicates::orient3d`) |
| LEEE | L, E, E, E | filtered + exact |
| LLEE | L, L, E, E | exact-only (expansion) |
| LLLE | L, L, L, E | exact-only (expansion) — Phase C (this PR) |
| LLLL | L, L, L, L | exact-only (expansion) — Phase C (this PR) |
| TEEE | T, E, E, E | exact-only (expansion) — Phase C (this PR) |
| LTEE | L, T, E, E | exact-only (expansion) — Phase C (this PR) |
| LLTE | L, L, T, E | exact-only (expansion) — Phase C (this PR) |
| TTEE | T, T, E, E | exact-only (expansion) — Phase C (this PR) |
| LTTE | L, T, T, E | exact-only (expansion) — Phase C (this PR) |
| LLTT | L, L, T, T | exact-only (expansion) — Phase C (this PR) |
| LTTT | L, T, T, T | exact-only (expansion) — Phase C (this PR) |
| TTTE | T, T, T, E | exact-only (expansion) — Phase C (this PR) |
| TTTT | T, T, T, T | exact-only (expansion) — Phase C (this PR) |
| LLLT | L, L, L, T | exact-only (expansion) — Phase C (this PR) |

Filter constants for orient3d TPI variants are not provided in the Cherchi 2020
paper text — they live in the C++ reference at
`github.com/gcherchi/FastAndRobustMeshArrangements`. Fetching and porting them
is out of scope for this PR (see PR2).

## Point Comparison (Section 4.3)

6 variants per axis for lexicographic sorting:

| Variant | Points | Filter epsilon power | Implementation status |
|---------|--------|---------------------|-----------------------|
| EE | E, E | exact (no filter) | filtered + exact |
| LE | L, E | 1.93e-14 · δ⁴ | filtered + exact |
| LL | L, L | 2.92e-13 · δ⁷ | exact-only (expansion) |
| TE | T, E | 3.98e-13 · δ⁷ | exact-only (expansion) — Phase D (this PR) |
| LT | L, T | 4.32e-12 · δ¹⁰ | exact-only (expansion) — Phase D (this PR) |
| TT | T, T | 5.50e-11 · δ¹³ | exact-only (expansion) — Phase D (this PR) |

## Two-Stage Filtering

Each predicate uses:
1. **Float with semi-static error bound** — compute expression and bound, if |result| > epsilon, return sign
2. **Expansion arithmetic fallback** — use Shewchuk adaptive expansions for guaranteed correct sign

(Interval arithmetic Stage 2 from the paper is skipped — no Rust interval crate. Float filter handles >99.99% of cases, expansion handles the rest.)

## Invariants

1. orient2d_indirect with 3 explicit points matches geometry_predicates::orient2d exactly
2. orient2d_indirect never returns wrong sign (correctness by expansion fallback)
3. point_compare_on_axis is a total order (antisymmetric, transitive)
4. LPI point is undefined (returns None) when edge is parallel to plane
5. TPI point is undefined (returns None) when planes are coplanar
6. **T-point predicates use expansion-only (no float filter) as of this PR**:
   the variants newly added in Phase B / C / D (TEE, LTE, LLT, LTT, TTE, TTT,
   and the orient3d/pointCompare T-bearing siblings) skip Stage 1 of the
   filtering architecture and go straight to Shewchuk expansion arithmetic.
   The filter constants tabulated above (and from Cherchi 2020 Table 1) are
   reserved for PR2; correctness is unaffected — only constant-factor speed.

## Branch Table

| Point combo | orient2d variant | Filter power | Expansion depth |
|---|---|---|---|
| All explicit | EEE | δ² | 2-expansion |
| 1 LPI + 2 explicit | LEE | δ⁵ | 5-expansion |
| 2 LPI + 1 explicit | LLE | δ¹¹ | 11-expansion |
| All LPI | LLL | δ¹⁴ | 14-expansion |
| 1 TPI + 2 explicit | TEE | δ⁸ | 8-expansion |
| Higher combos | see table | up to δ²⁶ | up to 26-expansion |

## Failure Modes

- LPI with d_L = 0: return None (caller must handle — edge parallel to plane)
- TPI with d_T = 0: return None (caller must handle — planes coplanar)
- Expansion arithmetic overflow: theoretically impossible with Shewchuk's algorithm (exact)

## Implementation phases

This spec was partially implemented across multiple PRs:

- **PR0** (initial): EEE, LEE, LLE, LLL orient2d; EEEE, LEEE, LLEE orient3d;
  EE, LE, LL pointCompare. Filter + exact stages where listed; exact-only
  otherwise.
- **PR1 (this PR — `cherchi-tpi-port` branch)**: All T-point variants of
  orient2d, orient3d, and pointCompare (Phases A–D). Expansion-only — no
  float filter. Oracle vectors extended with TPI cases (Phase E). See plan
  `/home/claude/.claude/plans/fluttering-rolling-crystal.md`.
- **PR2 (`cherchi-fallback-deletion` branch)**: Added telemetry counters
  (`ORIENT2D_FALLBACK_HITS`, `ORIENT3D_FALLBACK_HITS`,
  `POINT_COMPARE_FALLBACK_HITS`, `JOLLY_POINT_CREATIONS`) and ran the
  yang_fast assay (157 cases). Result: zero fallback hits across the corpus;
  332/333 cherchi calls had zero jolly creations, 1 call needed exactly one.
  With the gate cleared, deleted `orient2d_materialize_fallback`,
  `orient3d_materialize_fallback`, and the `point_compare_on_axis` materialize
  catch-all body. The dispatchers' `_` arms now contain `unreachable!()` with
  a Cherchi-citing message. Counters and the conditional `[cherchi-tele]
  FALLBACK HIT` emit were also removed; the `JOLLY_POINT_CREATIONS` counter
  and `[cherchi-tele] jolly_creations:` per-call emit remain (informational
  signal for future coplanar-pipeline work).
- **PR3 (planned)**: Add Stage 1 float filters to T-point variants using the
  Cherchi 2020 Table 1 constants for orient2d/pointCompare and the C++
  reference constants for orient3d.

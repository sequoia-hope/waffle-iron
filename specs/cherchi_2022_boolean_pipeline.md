# Spec: Cherchi 2022 Boolean Pipeline

## Goal

Document the **Cherchi 2022** [Cherchi/Pellacini/Attene/Livesu, *Interactive and
Robust Mesh Booleans*, SIGGRAPH Asia 2022] full mesh-Boolean pipeline as
implemented in the Waffle Iron kernel, mapping each stage to its source paper
(2020 vs 2021 vs 2022) and the corresponding Rust file/function.

This spec is the **canonical pipeline-level reference** that downstream
implementations (Yang 2025 stage 2 in particular) cite. For the underlying
predicate algebra, see `specs/cherchi_indirect_predicates.md` (the "Paper
Lineage and Codebase Map" section there is the citation-hygiene authority).

## Research Basis

- **Primary**: Cherchi et al. 2022 — *Interactive and Robust Mesh Booleans*.
  Local reference: `docs/references/cherchi-interactive-booleans-2022.md`.
- **Foundational**: Cherchi et al. 2020 — *Fast and Robust Mesh Arrangements*.
  Local reference: `docs/references/cherchi-indirect-predicates-2020.md`.
  REFERENCES.md ID: #9.
- **CDT subroutine**: Livesu et al. 2021 — *Deterministic Linear Time
  Constrained Triangulation Using Simplified Earcut.* Local reference:
  `docs/references/livesu-cherchi-cdt-2022.md`.
- **Per-stage perturbation fallback**: Hoffmann 1989 §5.3 — boundary-coincident
  perturb-and-classify pattern.
- **Downstream consumer**: Yang et al. 2025 [#24] — uses Cherchi 2022 as the
  stage-2 mesh-Boolean kernel.

## Pipeline Overview

The Cherchi 2022 paper organizes the mesh-Boolean computation into **two
phases**: arrangement (Section 4) and inside/outside classification
(Section 5). The phases are sequential: phase 2 reads the well-formed
simplicial complex produced by phase 1.

| Phase | Source paper           | Source section | Rust location                                                                  |
|-------|------------------------|----------------|---------------------------------------------------------------------------------|
| 1     | Cherchi 2020 + 2022    | C2020 §5; C2022 §4 | `crates/kernel/src/boolean/cherchi/`                                       |
| 2     | Cherchi 2022           | §5             | `crates/kernel/src/boolean/exact_mesh.rs::label_cells` /                       |
|       |                        |                | `label_sub_tri_raycast`                                                        |

## Phase 1 — Arrangement

The arrangement phase takes a triangle soup (the union of input meshes) and
splits triangles along their pairwise intersections to produce a well-formed
simplicial complex of patches. Cherchi 2022 §4 says explicitly: *"We based our
implementation of the arrangement step on the method described in
[Cherchi et al. 2020]."* The 2022 paper layers performance improvements on top
of the 2020 algorithm without changing its algorithmic structure.

### Stage 1.1 — Intersection Detection (Triangle Soup → Intersection Map)

| Source paper      | Section          | Rust file                                                              |
|-------------------|------------------|-------------------------------------------------------------------------|
| **Cherchi 2020**  | §5.1             | `cherchi/intersection_class.rs`                                         |
| Cherchi 2022      | §4 (octree improvements)| `cherchi/tree.rs`                                              |

- KdTree / octree spatial acceleration; pairwise triangle intersection tests.
- **Cherchi 2022** caches `orient3d` minors per input triangle plane to reduce
  each `orient3d` call to a 4-D dot product (§4 "Cached Predicates"). When the
  Rust port adopts this optimization, the citation should mention both papers:
  algorithm from 2020, optimization from 2022.
- Intersection classification (Cherchi 2020 Figure 5: disjoint, simplicial,
  point, segment, coplanar pocket) lives in `intersection_class.rs`.

### Stage 1.2 — Implicit-Point Insertion (Intersection Points → Sub-Triangles)

| Source paper      | Section                              | Rust file                                          |
|-------------------|--------------------------------------|-----------------------------------------------------|
| **Cherchi 2020**  | §4.1 (E/L/T points), §5.2 (insertion)| `boolean/indirect_predicates.rs`,                  |
|                   |                                      | `cherchi/fast_trimesh.rs`,                          |
|                   |                                      | `cherchi/triangle_soup.rs`                          |

- E (explicit), L (line-plane), T (three-plane) implicit point types — see
  `specs/cherchi_indirect_predicates.md`.
- Tree-of-sub-triangles insertion: `cherchi/fast_trimesh.rs` mirrors the
  `Tree<>` structure described in Cherchi 2020 §5.2.
- Edge-splitting with sorted insertion uses `pointCompare_on_axis` from
  `indirect_predicates.rs`.

### Stage 1.3 — Segment Insertion (Polygonal Pocket → CDT)

| Source paper            | Section          | Rust file                                                       |
|-------------------------|------------------|------------------------------------------------------------------|
| **Cherchi 2020**        | §5.3 (Algorithm 1, topological walk) | `cherchi/triangulation.rs`                  |
| **Livesu et al. 2021**  | (Simplified earcut)                  | `cherchi/triangulation.rs::earcut_linear`   |
| Cherchi 2022            | §4 (substitution rationale)          | (citation only — no new algorithm)         |

- The **topological walk** that builds left/right polygonal pockets around the
  segment is from Cherchi 2020 §5.3 (Algorithm 1 of that paper).
- The **CDT that triangulates each pocket** is the Livesu et al. 2021
  simplified earcut. Cherchi 2022 §4 says: *"We substituted earcut with a
  method recently introduced in [Livesu et al. 2021], which ensures optimal
  deterministic O(n) complexity in all cases."*
- The classical earcut (`triangulation.rs::earcut`) is retained as a fallback
  for non-simple polygons; cite as **Eberly 2008**.
- Conflict handling (segment crossing a previously-inserted constrained edge)
  introduces a new T-type implicit point — see Cherchi 2020 §5.3, Figure 6.

### Stage 1.4 — Coplanar Handling

| Source paper      | Section            | Rust file                                                               |
|-------------------|--------------------|-------------------------------------------------------------------------|
| **Cherchi 2020**  | §5.4               | `cherchi/processing.rs`, `cherchi/aux_structure.rs`                     |

- Auxiliary-tetrahedron trick to give coplanar triangle pairs a non-degenerate
  T-point representation (Cherchi 2020 §5.4).
- Global pocket map for coplanar tessellation deduplication.
- Cosurface orientation classification (`CosurfaceOrientation::Parallel /
  AntiParallel`) feeds into Phase 2's `label_sub_tri_raycast` — see the
  short-circuit at `exact_mesh.rs::1485`.

> **Citation note**: many in-code comments say `// Cherchi §5.4 / Hoffmann 1989
> §5.3`. The §5.4 reference is to **Cherchi 2020 §5.4** (coplanar pocket map),
> not Cherchi 2022 (whose §5 is *inside/outside classification*, not coplanar
> handling). Auditors should clarify ambiguous instances to `Cherchi 2020 §5.4`.

### Stage 1.5 — Low-Level Optimizations (Cherchi 2022 only)

| Source paper      | Section          | Rust file                          |
|-------------------|------------------|-------------------------------------|
| **Cherchi 2022**  | §4 last paragraph | distributed across `cherchi/*`     |

- Swiss-table hash maps, arena allocators, small-array-optimized adjacency
  lists, work-stealing parallelism. These do not change the algorithm's
  pipeline, only its constant factors. Cite Cherchi 2022 §4 when these
  techniques are explicitly used.
- Where the Rust port has not yet adopted a given 2022 optimization, no
  citation is needed; the algorithm stands on Cherchi 2020 alone.

## Phase 2 — Inside/Outside Classification (Cherchi 2022 §5, Algorithm 1)

This is **the new contribution of Cherchi 2022** — independent of the
arrangement and worth a careful read of §5 in the local extract.

| Source paper      | Section / Algorithm  | Rust file                                                                  |
|-------------------|----------------------|----------------------------------------------------------------------------|
| **Cherchi 2022**  | §5, Algorithm 1      | `boolean/exact_mesh.rs::label_sub_tri_raycast`,                            |
|                   |                      | `boolean/exact_mesh.rs::label_cells`,                                      |
|                   |                      | `boolean/exact_mesh.rs::ray_cast_inside`                                   |

### Stage 2.1 — Per-Patch Ray Cast (Algorithm 1)

For each arrangement patch P (in our pipeline, each labeled sub-triangle of
the conformal subdivision):

1. Pick a guaranteed-interior point `p ∈ P` (Cherchi 2022 §5.1).
2. Define the ray `r` from `p` toward an axis-aligned infinity point `p_∞`
   guaranteed to lie outside all input meshes.
3. For each input mesh `M`: compute and sort intersections of `r` with `M`
   using LPI implicit points (§5.2).
4. If `r` and `M` intersect, find the first intersecting triangle `t`.
5. Compute the signed volume of the tetrahedron `(t, p_∞)`. Negative → P is
   inside M; non-negative → P is outside.

Algorithm 1 pseudocode is reproduced verbatim in
`docs/references/cherchi-interactive-booleans-2022.md` §4.1.

In Waffle Iron, sub-triangle centroids serve as the emanating point `p`. The
ray-cast result enters `label_sub_tri_raycast` as the variable `primary` (see
`exact_mesh.rs:1513`). The `bvh: &BvhNode` parameter implements §5.2's
acceleration structure.

### Stage 2.2 — Cascaded Ray Definition (§5.1)

When the approximate floating-point barycenter fails the snap-rounding round-trip:

1. Try other triangles of P (Cherchi 2022 §5.1).
2. As last resort, fall back to **rational-number** ray-cast.

In Waffle Iron we use sub-triangle centroids and rely on Hoffmann 1989 §5.3
perturb-and-classify (sample both sides along the surface normal — see
`exact_mesh.rs:1545–1620`) instead of the rational fallback. This is a
deliberate divergence justified by the smaller sub-triangle scale and the
existing perturbation infrastructure.

### Stage 2.3 — Ambiguity Handling at Vertices/Edges (§5.3)

When the ray hits a vertex or edge (Figure 6 cases): perturb the coordinates of
`p_∞` by `std::nextafter` until the intersection lands strictly inside a
triangle, then re-test in 3D using three `orient3d` predicates.

In Waffle Iron, the cosurface-orientation short-circuit
(`CosurfaceOrientation::AntiParallel` / `Parallel`) is the
boundary-coincident analog: it pre-classifies sub-triangles whose parent
triangle in Phase 1 was identified as coplanar with the other operand
(Cherchi 2020 §5.4), bypassing Phase 2 entirely for those faces. See
`exact_mesh.rs:1485-1510`.

### Stage 2.4 — Final Filtering (per Boolean op)

After every patch has an in/out label per input mesh, the requested Boolean
operator (union / intersection / subtraction) selects which patches to keep.
Cherchi 2022 §3 states the standard rule:
- **Union**: patches of A outside B + patches of B outside A.
- **Intersection**: patches of A inside B + patches of B inside A.
- **Subtraction A−B**: patches of A outside B + patches of B inside A
  (with reversed orientation).

In Waffle Iron, this filtering is implemented in `label_cells` and downstream
in Yang's stage-5 flood-fill patch segmentation
(`crates/kernel/src/boolean/topology_extract.rs` per A15.6).

## Robustness Guarantees (Cherchi 2022 §5)

The pipeline guarantees:

1. **Manifold-watertight output** when inputs are manifold-watertight.
2. **Exact topology** — no false intersections, no missed intersections, no
   "tiny topological channels" between patches that would corrupt in/out
   propagation.
3. **Ambiguity-safe ray-casting** — every patch receives a correct in/out
   label even when the ray is tangent or hits non-generic features.
4. **No floating-point drift** — all geometric decisions go through indirect
   predicates with multi-stage filter → expansion fallback.

These guarantees are *contingent* on:

- Inputs being manifold-watertight and self-intersection-free.
- The complete predicate set (E/L/T variants of orient2d / orient3d /
  pointCompare) being available — see `specs/cherchi_indirect_predicates.md`.

## Integration with Yang 2025 Hybrid Pipeline

Yang 2025 §4.2 cites Cherchi 2022 as the mesh-intersection backbone:

> "We refer to [Cherchi et al. 2022; Livesu 2019] as the mesh intersection
> computation method used in our paper." (Yang §4.2)

And §4.4.2 cites it again for in/out classification:

> "Mesh Booleans. After trimming the meshes using the intersection curves, we
> directly apply a standard inside/outside classification step
> [Cherchi et al. 2022] to identify the triangles that need to be retained."
> (Yang §4.4.2)

Yang's "Livesu 2019" companion citation refers to **CinoLib** (the C++ data
structure library), not to a different paper introducing algorithmic content.

In the Yang hybrid pipeline as described in **A15.6** of
`governance/ARCHITECTURAL_INVARIANTS.md`:

| Yang stage | Yang section   | Cherchi-side responsibility                                  |
|------------|----------------|---------------------------------------------------------------|
| 0          | §4.5.5         | (Yang's coplanar preprocessing — runs before Cherchi)         |
| 1          | §4.1           | (Yang's bijective tessellation — runs before Cherchi)         |
| 2          | §4.2           | **Cherchi 2022 Phase 1 (arrangement)** — Stages 1.1–1.5 above |
| 3          | §4.3           | (Yang's SSI refinement — runs after Cherchi Phase 1)          |
| 4a         | §4.4.1, §4.4.2 | **Cherchi 2022 Phase 2 (Algorithm 1 in/out)** — Stage 2.1–2.4 |
| 5          | §4.4.2         | (Yang's flood-fill patch segmentation — runs after Cherchi Phase 2) |
| 6          | §4.4.3         | (Yang's B-Rep reassembly — runs after Cherchi Phase 2)        |

In particular: the function `exact_mesh.rs::label_sub_tri_raycast` is **Yang
stage 4a / Cherchi 2022 §5 (Algorithm 1)**. Its current doc comment cites
`#24 Yang 2025 §4.4` and `Hoffmann 1989 §5.3`; auditors should add
`Cherchi 2022 §5 / Algorithm 1` as the canonical primary algorithmic
reference.

## Invariants

1. **Phase 1 produces a well-formed simplicial complex** — every output
   triangle is either disjoint from every other output triangle or shares
   sub-simplices (edges/vertices). Verified by self-intersection oracle
   (`specs/inter_face_self_intersection_oracle.md`).
2. **Phase 2 reads only the simplicial complex topology** — it does not
   re-examine input-mesh self-intersections. If Phase 1's output is malformed,
   Phase 2 produces undefined results.
3. **Implicit points carry through Phase 1** — Phase 2 must accept LPI / TPI
   inputs without materializing coordinates. Materialization happens only when
   the result is exported (snap-rounding step, see Cherchi 2020 §5.6 — open
   problem).
4. **Boolean-source tag preservation** — Phase 1 must propagate the
   "originating input mesh" tag through every triangle subdivision so that
   Phase 2's per-mesh in/out labeling is unambiguous.

## Known Divergences from Cherchi 2022 (Waffle Iron-specific)

1. **Per-sub-triangle ray-cast instead of per-patch** — Waffle Iron currently
   labels each *sub-triangle* of the Cherchi arrangement, not a flood-filled
   *patch*. This is conservative (it makes the same in/out decision more
   times) and slower than the paper's per-patch approach. Yang's flood-fill
   patch segmentation (stage 5) recovers the patch granularity afterward.
2. **Hoffmann perturbation instead of `nextafter` perturbation** — for
   boundary-coincident sub-triangles, we sample both sides along the surface
   normal rather than perturbing `p_∞` per Cherchi 2022 §5.3. This is
   mathematically equivalent in the limit but reuses our existing perturbation
   path. See `exact_mesh.rs:1545–1620` doc comment.
3. **No rational-number fallback in `label_sub_tri_raycast`** — we rely on the
   Cherchi 2020 indirect-predicate guarantee that no decision exceeds the
   filter+expansion stack. The `JOLLY_POINT_CREATIONS` telemetry counter (see
   `cherchi_indirect_predicates.md` PR2 notes) tracks when the auxiliary
   tetrahedron mechanism activates, which is the indirect-predicate analog of
   the rational fallback.
4. **No interactive-frame-rate optimizations** — we have not (yet) ported the
   Cherchi 2022 §4 cached-predicate or parallelization optimizations. These
   are valuable but orthogonal to algorithmic correctness.

## Failure Modes

| Symptom                             | Likely stage                            | Diagnostic                                   |
|-------------------------------------|------------------------------------------|----------------------------------------------|
| Missing intersection segment in output | Stage 1.1 octree miss                  | Compare AABB-overlap pairs vs detected pairs |
| Sub-triangle classified inside/outside swapped | Stage 2.1 ray-cast direction       | Set `RAYCAST_DEBUG=1`, re-run                |
| Hang in segment insertion           | Stage 1.3 — non-simple pocket          | Log polygon vertex count; fall back to classical earcut |
| Coplanar overlap producing extra triangles | Stage 1.4 — pocket map missing or duplicating | `JOLLY_POINT_CREATIONS` counter, `[cherchi-tele]` traces |
| Boundary-coincident sub-triangle wrong label | Stage 2.3 — cosurface short-circuit absent | `label-cells-trace` stderr lines              |

## Cross-References

- `specs/cherchi_indirect_predicates.md` — predicates spec (Cherchi 2020
  §4.1–4.3); contains the canonical Paper Lineage and Codebase Map section.
- `docs/references/cherchi-indirect-predicates-2020.md` — Cherchi 2020 reference.
- `docs/references/cherchi-interactive-booleans-2022.md` — Cherchi 2022 reference.
- `docs/references/livesu-cherchi-cdt-2022.md` — Livesu et al. 2021 CDT reference.
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6 — Yang hybrid pipeline that
  consumes this Cherchi 2022 pipeline as stages 2 and 4a.
- `specs/inter_face_self_intersection_oracle.md` — invariant verification for
  Phase 1 output.
- `refs/yang2025_hybrid_boolean.pdf` — the Yang 2025 paper that drives the
  consumer architecture.

## Implementation Status

This spec documents the *intended* pipeline. As of 2026-04-27 the Rust port has:

- ✅ Stage 1.1 (intersection detection) — `cherchi/intersection_class.rs`,
  `cherchi/tree.rs`. Cherchi 2022 §4 cached-predicate optimization not yet ported.
- ✅ Stage 1.2 (implicit-point insertion) — `cherchi/fast_trimesh.rs`,
  `boolean/indirect_predicates.rs` (E/L/T variants complete; T-point variants
  use expansion-only fallback per `cherchi_indirect_predicates.md` PR1–PR2).
- ✅ Stage 1.3 (segment insertion) — `cherchi/triangulation.rs::earcut_linear`
  is the Livesu et al. 2021 CDT.
- ✅ Stage 1.4 (coplanar handling) — `cherchi/processing.rs`,
  `cherchi/aux_structure.rs`.
- ⏸ Stage 1.5 (Cherchi 2022 §4 low-level optimizations) — partial / TBD.
- ✅ Stage 2.1 (per-sub-triangle ray-cast) — `exact_mesh.rs::label_sub_tri_raycast`.
- ⚠ Stage 2.2 (cascaded ray definition) — uses Hoffmann perturbation rather
  than `nextafter` cascade. Documented divergence above.
- ✅ Stage 2.3 (ambiguity handling) — cosurface-orientation short-circuit
  + Hoffmann sample-both-sides. Documented divergence above.
- ✅ Stage 2.4 (Boolean filtering) — `label_cells` + Yang stage-5 flood-fill in
  `topology_extract.rs`.

# Spec: Yang Conformal-Mesh Oracle (PR-Y14a)

## Goal

Define the API contract for a pure measurement oracle that answers
**"is this triangle mesh a well-formed simplicial complex?"** for any
`(verts, tris)` pair produced anywhere in the Yang pipeline. The oracle
returns a structured `ConformalReport` enumerating every directed edge
that lacks its reverse counterpart and every directed edge that appears
more than expected, together with the mesh's Euler characteristic. PR-Y14a
will install three call-site probes (Stages 2, 4, 6) that invoke this
oracle behind an `YANG_CONFORMAL_PROBE=1` env-var gate — those probe
sites tell us *which* pipeline stage first violates conformality, which
in turn picks the empirical anchor for the PR-Y14b fix.

This spec is the API contract for the oracle ONLY. It is NOT the PR-Y14b
fix spec. The fix anchor is decided by where the probes report
`well_formed=false` on F0002 / F0004, not by anything in this document.

## Research Basis

The conformal-mesh property the oracle measures is the
**well-formed simplicial complex** guarantee that Cherchi 2020's
arrangement is supposed to produce, and that Yang 2025's hybrid pipeline
explicitly inherits.

- **[#9] Cherchi et al. 2020 (arrangement)**, §1 / §5: the algorithm
  "transform[s] any generic set of triangles in 3D space into a
  well-formed simplicial complex (mesh arrangement)" where triangles are
  "either disjoint or connected through shared sub-simplices (shared
  edges or vertices)". A directed edge `(v0→v1)` of one triangle has
  exactly one paired directed edge `(v1→v0)` in another triangle whenever
  the underlying undirected edge is interior (manifold); boundary edges
  appear once with no reverse; the only legal multiplicity is the
  manifold case (multiplicity 2 — one fwd, one rev).
- **[#38] Cherchi et al. 2022 §5 / Algorithm 1**: the inside/outside
  classifier propagates one label per *patch* over the manifold-edge
  graph induced by this conformal property. If the input mesh is not a
  well-formed simplicial complex, the patch graph is wrong and labels
  leak across patch boundaries.
- **[#24] Yang et al. 2025 §4.4.3 ("Watertightness and correctness",
  paper line ~1010)**: "The watertightness of our result is **inherited
  from the mesh Boolean output**." Yang's B-Rep assembly trusts the
  conformal property as a precondition; it has no fallback recovery for
  mesh non-conformality. §4.4.2's flood-fill patch segmentation
  presupposes that twin pairing is determined entirely by the conformal
  mesh — exactly what `ARCHITECTURAL_INVARIANTS.md` A15.6 codifies.
- **[#39] Livesu et al. 2021** is the CDT used for segment insertion
  inside Cherchi's arrangement; if its simplified-earcut step is invoked
  asymmetrically across coplanar pairs, conformality breaks at Stage 2.

The oracle measures the property that all three of these references
guarantee or assume. A `well_formed=false` reading at any pipeline stage
is a contract violation that downstream code is licensed to assume away.

## API Contract

### Module path

```
crates/kernel/src/boolean/oracles/conformal_mesh.rs
```

Add a single line to `crates/kernel/src/boolean/oracles/mod.rs`:

```rust
pub(crate) mod conformal_mesh;
```

The module is `pub(crate)` to match the existing oracle pattern
(`coplanar_identical`, `arrangement_wellformed`, `label_consistency`).
The oracle is consumed only by call-site probes within the `kernel`
crate; it does not cross the WASM bridge.

### Public function

```rust
/// Measure conformality of a triangle mesh.
///
/// A mesh is "well-formed" (Cherchi 2020 §5; Yang 2025 §4.4.3) when
/// every directed edge `(v0→v1)` has exactly one reverse counterpart
/// `(v1→v0)` belonging to a different triangle. Boundary edges (no
/// reverse) and multi-paired edges (more than one fwd or rev) are both
/// reported.
///
/// The function is pure: no logging, no panics on degenerate input,
/// no global state. Empty input returns a trivially well-formed report.
///
/// Vertices are canonical-quantized internally at nanometer precision
/// (`crate::units::QUANT_NANOMETER_SCALE`) so callers may pass the raw
/// `[f64; 3]` positions emitted by upstream stages without pre-merging
/// coincident vertices. This mirrors the canonical-quantize closure at
/// `topology_extract.rs:375-393` (which the oracle MUST reuse, not
/// duplicate).
pub fn check_conformal(verts: &[[f64; 3]], tris: &[[usize; 3]]) -> ConformalReport;
```

### Public types

```rust
/// Result of a conformality check. Always inspectable; `is_well_formed`
/// is the single boolean predicate downstream probes log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformalReport {
    /// Directed edges `(v0, v1)` for which no triangle contains the
    /// reverse `(v1, v0)`. v0/v1 are CANONICAL vertex indices (post
    /// nanometer-quantize), not raw indices into `verts`.
    pub unpaired_directed_edges: Vec<UnpairedEdge>,

    /// Directed edges that appear in more than one triangle in the same
    /// direction (or whose fwd/rev counts disagree by >1). Conformal
    /// meshes have exactly one fwd and one rev per interior undirected
    /// edge, exactly one fwd OR rev per boundary edge. Anything else is
    /// reported here.
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
    /// Pre-computed so callers don't have to re-derive it.
    pub is_well_formed: bool,
}

/// Directed edge `(v0, v1)` lacking a reverse partner in any triangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedEdge {
    pub v0: usize,
    pub v1: usize,
    /// Indices into the input `tris` slice that contain `(v0, v1)` as a
    /// directed edge. For a well-formed open boundary, this is length 1.
    /// Length > 1 means the same boundary edge is asserted by multiple
    /// triangles — also a violation, reported additionally under
    /// `multi_paired_edges`.
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
```

### Purity, panic, and allocation contract

- **Pure**: no I/O, no logging, no `eprintln!`, no global state read or
  write. Same `(verts, tris)` always returns a `ConformalReport` that
  compares `assert_eq!`-equal to any earlier call.
- **No panics on degenerate input**: empty mesh, zero-area triangles,
  triangles with two equal vertex indices, vertex indices `>= verts.len()`
  must all return a `ConformalReport` — not panic. Out-of-range vertex
  indices are clamped/skipped (the triangle is treated as if it
  contributed no directed edges) and the triangle is recorded under
  `multi_paired_edges` if degenerate.
- **Deterministic ordering**: `unpaired_directed_edges` and
  `multi_paired_edges` are sorted by `(v0, v1)` ascending so two equal
  meshes produce byte-identical reports. Use `BTreeMap`, not `HashMap`,
  for internal accumulators (per `feedback_no_regression_chasing.md`).
- **Allocation**: the function allocates `O(tris.len())` for the edge
  multimap and the report vectors; no quadratic blow-up.

### Canonical-quantize policy

The function applies canonical-quantize **inside** `check_conformal`
before edge accounting. Rationale:

1. Probe call sites pass `(verts, tris)` directly from the upstream
   stage. The upstream stages do not all merge near-duplicate vertices
   (Stage 2 emits `SubdividedMesh::verts` with LPI-rounded floats; Stage
   6 emits `patch_boundaries` with vertices that may straddle two patches
   at slightly different float bits).
2. The oracle's contract is a *combinatorial* one — directed edge
   pairing — and must agree with how downstream code pairs edges.
   `topology_extract.rs:375-393` already uses
   `crate::units::QUANT_NANOMETER_SCALE` for that purpose; the oracle
   reusing it guarantees the oracle and downstream see the same
   incidence graph.
3. The reuse is a behavioral contract: the oracle MUST reuse the same
   `QUANT_NANOMETER_SCALE` constant from `crate::units` and produce
   byte-identical canonicalization to the closure at
   `topology_extract.rs:375-393`. Drift between the oracle's quantize
   and downstream's quantize would make oracle verdicts misleading.

The implementer SHOULD inline the closure form (verbatim, with a
`// Invariant: must match topology_extract.rs:375-393` comment) rather
than extract a `pub(crate) fn quantize_pos` helper. Rationale: PR-Y14a
is a measurement-only PR with zero behavior change; extracting a helper
would touch `topology_extract.rs` for non-measurement reasons and
expand the diff. A future cleanup PR may extract the helper once the
canonical-quantize is consumed by three or more call sites.

## Probe Call Sites

Three diagnostic call sites, all gated on the env var
`YANG_CONFORMAL_PROBE`. When unset (default), zero behavior change —
mirroring the existing `TWIN_DEBUG` and `CHERCHI_DEBUG` patterns at
`yang_integration.rs:1106` and `exact_mesh.rs`.

| Probe | Stage | File / anchor | What it measures |
|-------|-------|---------------|------------------|
| **A** | Stage 2 (after Cherchi arrangement) | `crates/kernel/src/boolean/yang_integration.rs`, near the existing `[yang-diag] after subdivide:` log (~L770). Pass the post-`subdivide_mesh_pair_full_cherchi` `SubdividedMesh::verts` plus the union of `tris_a + tris_b` (extracted via `sub.verts` triples). | The conformality of the raw arrangement mesh. If `well_formed=false` here, H1 (Cherchi-stage break — the per-triangle local-CDT divergence audit hypothesis) or H2 (Stage 0 coplanar asymmetry feeding Stage 2) is on the table. |
| **B** | Stage 4 (after `survival.groups` populated) | Same file, near `[yang-diag] after survival:`. Pass the surviving sub-tri vertex/index slices. | The conformality of the *kept* mesh after inside/outside classification. If A passed but B fails, H3 (survival filter drops one side of an edge while keeping the other) is on the table. |
| **C** | Stage 6 (after `flood_fill_patches` produces `patch_boundaries`, BEFORE Step 7's `// ── Step 7: Build B-Rep from patches ──`) | `crates/kernel/src/boolean/topology_extract.rs:~720`. Pass the patch-boundary vertex/index slices. | The conformality of the patch-boundary topology that twin pairing is about to consume. If A and B passed but C fails, H4 (our own Step 5a/Step 6 patch-boundary extraction emits a directed edge no other patch emits as its reverse) is on the table. |

### Probe log format

Each probe emits exactly one summary line per invocation, plus up to
five detail lines when `well_formed=false`:

```
[conformal-probe] stage={A|B|C} unpaired={N} multi_paired={M} euler_chi={X} well_formed={true|false} verts={V} tris={T} unique_edges={E}
```

When `well_formed=false`, follow with up to 5 lines (no more — log
volume budget):

```
[conformal-probe]   unpaired #{i}: v0={n0} v1={n1} source_tris={[t0,t1,...]}
[conformal-probe]   multi_paired #{i}: v0={n0} v1={n1} fwd={[...]} rev={[...]}
```

Both detail kinds are interleaved by the natural sort order
(`unpaired_directed_edges` first, then `multi_paired_edges`). The
implementer chooses how to slice "first 5" — recommended: first 5
total across both vectors, prefer unpaired before multi.

### Zero-behavior-change invariant

**When `YANG_CONFORMAL_PROBE` is unset, no `[conformal-probe]` line is
emitted, no oracle is invoked, and pipeline output is byte-identical to
the pre-PR baseline.** Verification: `cargo test -p test-harness --test
assay_randomized -- yang_fast --ignored` produces zero
`[conformal-probe]` lines and identical pass/fail counts to current
`results.json` (9 passed, 180 failed, 1 errored, 33 timeouts).

The probe sites must be wrapped in a single env-var read at the top of
the function (mirror `topology_extract.rs:1106`'s `let twin_debug =
std::env::var("TWIN_DEBUG").as_deref() == Ok("1");`). Reading the env
var inside a tight loop would be a behavior change (allocator pressure
+ syscall noise) even though it returns the same value.

## Branch Table

The oracle itself has no branching parameters — it is a pure function of
`(verts, tris)`. Branching lives at the call sites (env-var on/off).

| Probe stage | `YANG_CONFORMAL_PROBE` | Behavior | Output |
|---|---|---|---|
| A / B / C | unset | Skip env-var read once; do not call `check_conformal`; do not log. | None — bit-exact baseline. |
| A | set | Build `(verts, tris)` from `subdivided`. Call oracle. Emit summary line. | One `[conformal-probe] stage=A …` line per boolean op. |
| B | set | Build `(verts, tris)` from surviving sub-tris. Call oracle. Emit summary line. | One `[conformal-probe] stage=B …` line per boolean op. |
| C | set | Build `(verts, tris)` from `patch_boundaries`. Call oracle. Emit summary line. | One `[conformal-probe] stage=C …` line per boolean op. |
| any | set, but stage's snapshot empty (e.g. AABB-disjoint short-circuit skipped Cherchi) | Skip the probe; no panic. | Nothing emitted; that stage is just absent from the trace. |

The "snapshot empty" row matters because PR13's AABB-disjoint
short-circuit at `topology_extract.rs:1361` skips Cherchi entirely. In
that path, Probe A has nothing to measure; the implementer must guard
against passing an empty mesh slice (the oracle itself accepts it, but
the probe's log line would be misleading — emit nothing instead).

## Invariants

- **I1 (well_formed predicate)**:
  `report.is_well_formed == (report.unpaired_directed_edges.is_empty() && report.multi_paired_edges.is_empty())`
  for every report ever returned. The field is a memoization of this
  conjunction; never re-define it independently.
- **I2 (Euler characteristic on closed orientable manifolds)**: when
  `is_well_formed == true` and the mesh is closed (every undirected
  edge has multiplicity exactly 2), `euler_characteristic == 2 * k`
  where `k` is the number of connected components in the dual graph
  (triangles linked by shared edges). The oracle does NOT need to
  decompose into components — `2 * k` is a property the test author
  asserts in the "two disconnected cubes" oracle test.
- **I3 (boundary-edge accounting on well-formed meshes)**: when
  `is_well_formed == true` AND there are no boundary edges (closed
  mesh), `unique_undirected_edge_count * 2 == 3 * triangle_count`.
  Equivalently, every directed edge has exactly one reverse, and the
  total directed-edge count is `3 * F == 2 * E`. For meshes with
  boundary edges (well-formed open meshes), the relation is
  `3 * F == 2 * (E - B) + B`, where `B` is the number of boundary
  undirected edges; the oracle does not separately report `B`, but a
  test author can derive it as
  `3 * triangle_count - 2 * unique_undirected_edge_count`.
- **I4 (totality and purity)**: `check_conformal` is total — defined
  for all `&[[f64; 3]]` and `&[[usize; 3]]` slices regardless of
  contents. Same input always returns `assert_eq!`-equal output. No
  allocation panic on inputs up to 100k triangles (the largest assay
  meshes are ~13k tris per Yang `subdivided.verts`).
- **I5 (probe zero-impact when off)**: with `YANG_CONFORMAL_PROBE`
  unset, no `[conformal-probe]` line is emitted, and no production code
  path is altered. Verifiable by diffing
  `cargo test -p test-harness --test assay_randomized -- yang_fast
  --ignored` stderr/stdout against pre-PR baseline.
- **I6 (oracle name-spacing in logs)**: every log line emitted by the
  probe sites starts with the literal prefix `[conformal-probe]`. No
  other module in the codebase may use this prefix. This makes it
  trivial to grep all probe output (`grep '\[conformal-probe\]'`) and
  to confirm I5 (no lines when env var unset).

## Oracles for the Oracle's Own Tests

The Test Author writes the unit tests. The cases this spec REQUIRES the
test suite to cover (failure of any one of these cases is a Test Author
bug, not an Implementer bug):

| # | Case | Setup | Asserted properties |
|---|------|-------|---------------------|
| 1 | **Well-formed cube** | 8 vertices, 12 triangles (2 per cube face, consistent CCW outward winding) | `is_well_formed == true`; `euler_characteristic == 2`; `vertex_count == 8`; `triangle_count == 12`; `unique_undirected_edge_count == 18`; both vectors empty. |
| 2 | **Cube with one triangle reversed** | Same as case 1, but one triangle's winding flipped (e.g. `[0, 1, 2]` → `[0, 2, 1]`). | `is_well_formed == false`; at least 2 entries in `unpaired_directed_edges` (the 3 edges of the reversed tri lose their pair, but two of them gain a doubled-fwd partner, so the `multi_paired_edges` and `unpaired_directed_edges` counts depend on adjacency — test author asserts the DISJUNCTION, not exact counts). |
| 3 | **Two disconnected cubes** | 16 vertices, 24 triangles — two cubes from case 1 translated apart, no shared vertices. | `is_well_formed == true`; `euler_characteristic == 4`; `vertex_count == 16`; `triangle_count == 24`. (This case anchors I2's `2 * k` claim with `k = 2`.) |
| 4 | **Empty mesh** | `verts = &[]`, `tris = &[]`. | `is_well_formed == true`; all counts == 0; both vectors empty. No panic. |
| 5 | **Mutation sanity** | The Adversary or Test Author should temporarily invert one branch of `is_well_formed`'s computation (e.g. flip the `&&` to `||`). At least one of the cases above MUST then fail. If all five still pass, the tests are insufficient and must be strengthened. | Mutation kills at least one test. |

The test cases above are the *floor*, not the ceiling. The Test Author
may add a "vertex-coincidence canonicalize check" case (two
near-duplicate vertices that quantize to the same canonical key
collapse and produce a well-formed result) and a "degenerate triangle"
case (a triangle with two equal vertex indices is reported under
`multi_paired_edges`, not panic).

## Failure Modes

The oracle is total — it does not return errors, only `ConformalReport`s
that may indicate violations. The behavior on edge inputs:

- **Empty mesh** (`verts = []`, `tris = []`): returns a trivially
  well-formed report (all counts zero, both vectors empty,
  `is_well_formed == true`). This case fires every time PR13's
  AABB-disjoint short-circuit skips Cherchi — the probe site MUST
  guard against logging in that case (per Branch Table row "snapshot
  empty"), but the oracle itself accepts the input.
- **Duplicate vertices in input** (two indices in `verts` quantize to
  the same canonical key): the duplicate vertices are merged by the
  canonical-quantize step. By design — the oracle measures
  combinatorial conformality after deduplication, which is exactly
  what downstream Yang code sees.
- **Degenerate triangle with two equal vertex indices** (e.g.
  `tris = [[0, 0, 1]]`): the directed edges `(0, 0)`, `(0, 1)`, `(1, 0)`
  are all generated. `(0, 0)` has no reverse (and is also a self-loop,
  which the oracle reports under `multi_paired_edges` as an unusual
  multiplicity pattern with `fwd_tris = [0], rev_tris = [0]`). The
  triangle is reported, not panicked on.
- **Out-of-range vertex index** (`tris[i][j] >= verts.len()`): the
  vertex index cannot be quantized. The oracle treats the index as its
  own canonical key (i.e., does not crash on indexing), and the
  resulting unpaired edge pattern surfaces in `unpaired_directed_edges`.
  No panic. The implementer may choose to guard with a `get(...)?`
  pattern; the spec only requires "no panic", not a specific recovery.
- **Very large input** (`tris.len()` >> 100k): the oracle still runs in
  O(F log F) time and O(F) memory, but is not load-bearing for
  large-mesh performance. The probe call sites are diagnostic; if a
  probe ever runs on >100k triangles in production, that's a separate
  concern.

## Why this oracle does NOT define the PR-Y14b fix

This oracle is a **measurement**, not a remedy. Its output is a
diagnostic `ConformalReport`; it does not propose, choose, or apply any
fix. The PR-Y14b fix anchor is decided by:

1. Running PR-Y14a's three probes on F0002 and F0004.
2. Reading the four `(stage, well_formed)` tuples emitted (Stage 0 is
   not probed in PR-Y14a — Stage 0 has its own oracle at
   `coplanar_identical.rs`; the chain Probe-A → Probe-B → Probe-C
   covers the post-tess Yang stages).
3. Identifying the **first** stage at which `well_formed=false` is
   reported (per `feedback_anchor_before_fix.md` —
   verify-anchor-before-coding).
4. Writing the PR-Y14b spec at
   `specs/yang_pr_y14b_<anchor>.md`, AFTER the findings memo lands and
   pinpoints which of H1/H2/H3/H4 the data supports.

If all three probes report `well_formed=true` but twin pairing still
fails, that's evidence for H4 (the bug is in our patch-boundary
extraction itself, downstream of the probes), and PR-Y14b targets that
narrowly. If Probe A reports false, the bug is at Stage 2 and PR-Y14b
investigates the Cherchi local-CDT step or Stage 0's coplanar
preprocessing. Either outcome — finding the anchor or proving it
elsewhere — is acceptable per the plan's "Outcome we want" section.
**This oracle's job is to enable that decision with empirical data, not
to make it.**

## References

- [#9] Cherchi et al. 2020 — "Fast and Robust Mesh Arrangements using
  Floating-point Arithmetic" (well-formed simplicial complex,
  §5; canonicalization via indirect predicates).
- [#24] Yang et al. 2025 — "Boolean Operation for CAD Models Using a
  Hybrid Representation" (§4.4.3 watertightness inheritance from mesh
  Boolean output; §4.4.2 flood-fill patch segmentation; §4.5.5 coplanar
  preprocessing).
- [#38] Cherchi et al. 2022 — "Interactive and Robust Mesh Booleans"
  (§5 Algorithm 1 patch-based ray-cast in/out classification; assumes
  the conformality property §5/§5.5 of Cherchi 2020).
- [#39] Livesu et al. 2021 — "Deterministic Linear Time Constrained
  Triangulation Using Simplified Earcut" (the CDT used inside Cherchi
  2022's segment insertion; if invoked asymmetrically, breaks
  conformality).
- `governance/ARCHITECTURAL_INVARIANTS.md` A15.6 — codifies that
  twin pairing must be inherited from the conformal mesh, not
  re-derived by boundary-edge-chaining.
- `governance/ENGINEERING_CONSTITUTION.md` P8 (research basis), P9
  (no hack-to-green), P10 (plan-first).
- `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3 (spec phase),
  §8 (bug-fix variant — applies to PR-Y14b, not this spec).
- `crates/kernel/src/boolean/topology_extract.rs:375-393` — canonical
  quantize closure that the oracle MUST reuse (or factor into a
  shared helper).
- `specs/yang_topology_extract_twin_pairing.md` — PR11/PR12 prior
  art; documents the dominant assay failure that motivated this
  oracle.
- `specs/yang_twin_pairing_partial_topology.md` — PR-era exploration
  of partial-topology acceptance; superseded by the
  measure-then-fix approach this oracle enables.

# Spec — PR11: Yang Stage 4b per-patch labeling (Cherchi 2022 §5 Algorithm 1) + F1 / F2 oracle fixes

Per FIP §3.2. Gates T2 (red-phase tests), T3 (Stage 4b implementation), T4 (oracle fixes).

Author: agent-spec, team `yang-per-patch-labeling-pr11`.

References:
- Yang 2025 §4.4 — labeling stage of the hybrid B-Rep / mesh boolean.
- Cherchi 2022 §5 + Algorithm 1 — per-patch ray-cast inside/outside (the algorithm being adopted).
- Cherchi 2022 §5.1 — ray definition: "we pick a random triangle t ∈ P".
- `specs/oracle_validity_audit.md` — PR10 audit; defines F1 / F2.
- `specs/pipeline_oracles.md` — PR9 oracle harness.
- `crates/kernel/src/boolean/exact_mesh.rs::label_cells` (lines 1944-2035) — current per-sub-tri implementation.
- `crates/kernel/src/boolean/exact_mesh.rs::build_manifold_patch_graph` (lines 1829-1922) — PR8 patch graph.
- `crates/kernel/src/boolean/exact_mesh.rs::label_sub_tri_raycast` (lines 1528-1693) — ray-cast + Hoffmann + GWN.
- `crates/kernel/src/boolean/oracles/label_consistency.rs` — Stage 4b oracle.

---

## 1. Goal

**User-visible**: more Yang corpus cases pass `YANG_BOOLEAN=1` (currently 9/157 non-timeout).
PR10's oracle-validity audit measured the largest single empirical lever:
**120 / 157 corpus cases first-fail at `LabelConsistencyWithinPatchOracle` (Stage 4b)**, with diagnostics
of the form `patch K contains 2 distinct labels [Inside, Outside] across N sub-tris`. 120 / 120 of those
also fire the Stage 6 `TwinSymmetryOracle`, so a correct Stage 4b fix plausibly subsumes the cascade.

**Architectural**: Yang Stage 4b labeling becomes per-patch per Cherchi 2022 §5 Algorithm 1 instead of
per-sub-triangle. Cherchi's headline insight is that "the algorithm scales with the number of patches in
the arrangement and not with the number of triangles in the mesh" (§5, p. 6); one ray-cast per
manifold-edge-bounded patch suffices, and the resulting label propagates to every member sub-triangle by
construction. The PR8 `ManifoldPatchGraph` already supplies the patch decomposition; PR11 wires it into
`label_cells` so that the per-patch invariant — already encoded in the
`LabelConsistencyWithinPatchOracle` — holds **by construction**, not by post-hoc validation.

**What this PR does NOT claim** (per `feedback_no_last_bug.md` and the plan's honest framing):
- Does not fix the 28 Stage 2 first-fail cases (degenerate sub-triangles in Cherchi output — deferred to PR12).
- Does not guarantee all 120 Stage 4b cases pass: cascade may unmask additional Stage 6 defects, and
  representative-picking quality may misbehave on adversarial inputs.
- Success criterion is **histogram shift relative to PR10 baseline**, not absolute pass count.

---

## 2. Parameters

The refactored `label_cells` signature (additive — adds one parameter, signature otherwise unchanged):

```rust
pub(crate) fn label_cells(
    subdivided: &SubdividedMesh,
    graph: &ManifoldPatchGraph,            // NEW (PR11): patch decomposition.
    original_verts_a: &[[f64; 3]],
    original_tris_a: &[[usize; 3]],
    original_verts_b: &[[f64; 3]],
    original_tris_b: &[[usize; 3]],
    deadline: Option<std::time::Instant>,
    d_epsilon: f64,
) -> Result<CellLabeling, KernelError>
```

| Parameter | Meaning | Default | Units | Valid range | Error condition |
|---|---|---|---|---|---|
| `subdivided` | Output of `subdivide_mesh_pair` (Cherchi arrangement). | n/a | unitless indices + meters | non-empty `tris_a` ∪ `tris_b` | Empty case is *not* an error; both-empty short-circuits with empty labels. |
| `graph` | `ManifoldPatchGraph` produced by `build_manifold_patch_graph(subdivided)`. **Caller's contract** to build this fresh from the `subdivided` argument. | n/a | unitless | `graph.tris_a_count == subdivided.tris_a.len()` and `graph.patch_of.len() == subdivided.tris_a.len() + subdivided.tris_b.len()` | Mismatch → `KernelError::NotSupported { operation: "label_cells: graph/subdivided count mismatch" }` (defensive — caller bug). |
| `original_verts_a`, `original_tris_a` | Pre-arrangement mesh A geometry — ray-cast target for B sub-tris. | n/a | meters / indices | `verts_a` may be empty → all B labels = `Outside`. | None (empty handled). |
| `original_verts_b`, `original_tris_b` | Pre-arrangement mesh B geometry — ray-cast target for A sub-tris. | n/a | meters / indices | `verts_b` may be empty → all A labels = `Outside`. | None (empty handled). |
| `deadline` | Pipeline timeout cutoff. | `None` | wall-clock instant | any | Past deadline → `KernelError::NotSupported { operation: "yang_boolean: label_cells timeout (...)" }`. |
| `d_epsilon` | Hoffmann perturbation magnitude inside `label_sub_tri_raycast`. | `1e-6` (hard floor when `d_epsilon <= 0.0`) | meters | `> 0` (clamped) | `<= 0.0` is *not* an error; clamped to `1e-6`. |

Returns: `CellLabeling { labels_a, labels_b }` where
`labels_a.len() == subdivided.tris_a.len()` and `labels_b.len() == subdivided.tris_b.len()` —
**unchanged from the pre-PR11 contract**.

---

## 3. Branch Table

| # | Patch composition | Representative pick | Ray-cast outcome | Action | Notes |
|---|---|---|---|---|---|
| B1 | All sub-tris from mesh A (`flat_idx < tris_a_count`) | First non-degenerate member | `label_sub_tri_raycast` returns `Inside`/`Outside` against mesh B | Propagate that label to every patch member's `labels_a[...]` slot. | Common path. Cherchi 2022 §5 Algorithm 1 main case. |
| B2 | All sub-tris from mesh B (`flat_idx >= tris_a_count`) | First non-degenerate member | `label_sub_tri_raycast` returns `Inside`/`Outside` against mesh A | Propagate to every member's `labels_b[...]` slot. | Symmetric to B1. |
| B3 | **Mixed-mesh patch**: members from both A and B | First non-degenerate member, regardless of side | Ray-cast against the *opposing* mesh per member when propagating: A-members against mesh B, B-members against mesh A | Propagate the **single representative's classification** to the appropriate per-member slot, using a per-member opposing-mesh lookup. | Occurs post-Cherchi at coplanar STAGE2 unifications (label=3 emits both an A and a B sub-tri sharing edges). Confirmed possible by reading `subdivide_mesh_pair_full_cherchi` (`exact_mesh.rs:2375-2434`) and `build_manifold_patch_graph` (no cross-mesh barrier). **See §6.B3 for the precise rule** — naive single-target propagation would be wrong because A vs. B members have different opposing meshes. |
| B4 | Representative is degenerate or sliver | Skip representative; try next member; if none non-degenerate, use the first member regardless | `label_sub_tri_raycast` exercises Hoffmann + GWN fallback as today | Propagate. | Cherchi 2022 §5.1: "If the test fails we attempt to produce the same construction with another triangle of P". |
| B5 | Empty patch | n/a | n/a | No-op (no slot to write). | Should not occur — `build_manifold_patch_graph` only emits non-empty patches by construction (each unvisited seed yields at least itself). Defensive: skip silently. |
| B6 | Deadline expired (every 100 patches) | n/a | n/a | Return `KernelError::NotSupported { operation: "yang_boolean: label_cells timeout (per-patch loop)" }` | Mirrors current per-sub-tri timeout policy; cadence becomes per-patch instead of per-sub-tri (same wall-clock budget). |
| B7 | Caller passed mismatched `graph` | n/a | n/a | Return `KernelError::NotSupported { operation: "label_cells: graph/subdivided count mismatch" }` | Defensive parameter validation — no real production caller should hit this. |

**Implicit branches retained** inside `label_sub_tri_raycast` (unchanged):
- Cosurface short-circuit (Parallel / AntiParallel from `SubTriangle::cosurface_orientation`).
- Primary axis-aligned ray-cast.
- Hoffmann perturb-and-classify fallback (above / below).
- GWN fallback for double-degenerate cases.

These are NOT re-enumerated; per the plan, `label_sub_tri_raycast` is reused **verbatim** — PR11 only
changes how often (and on which sub-tri) it is called.

---

## 4. Invariants

All measurable. Tests (T2) MUST encode at least invariants I1, I2, I4 as red-phase assertions.

### I1 — Per-patch label uniformity (the Cherchi 2022 §5 contract)
For every patch `k ∈ 0..graph.patches.len()`, every member of that patch has the same `CellLabel`:
```
∀ k. ∀ flat_i, flat_j ∈ graph.patches[k].
    label_of(flat_i, labeling, graph) == label_of(flat_j, labeling, graph)
```
where `label_of(flat, labeling, graph) = labeling.labels_a[flat]` if `flat < graph.tris_a_count` else
`labeling.labels_b[flat - graph.tris_a_count]`.

**Measurable by**: `LabelConsistencyWithinPatchOracle::check` returns `Ok(())` (PR9). Holds **by
construction** post-PR11; the oracle becomes a proof-of-construction sentinel rather than a defect
detector for this contract.

### I2 — Representative-pick equivalence
The label assigned to every member of patch `k` equals the label that `label_sub_tri_raycast` would
return when invoked on the patch's representative sub-triangle:
```
∀ k. ∀ flat ∈ graph.patches[k].
    label_of(flat, labeling, graph) == label_sub_tri_raycast(rep_of(k), opposing_mesh_of(rep_of(k)), …)
```
For mixed-mesh patches (B3) the right-hand side is evaluated against the *representative's* opposing
mesh; for monochromatic patches (B1, B2) the opposing mesh is the same for every member, so the
distinction is moot.

**Measurable by**: T2 unit test that builds a synthetic `SubdividedMesh` + `ManifoldPatchGraph`, pre-
computes the per-representative label by calling `label_sub_tri_raycast` directly, then asserts
equality with `label_cells` output for every patch member.

### I3 — Patch identification preservation
The `ManifoldPatchGraph` consumed by `label_cells` is identical to the one
`build_manifold_patch_graph(subdivided)` would produce if called inside `label_cells`. PR11 does NOT
modify `build_manifold_patch_graph`; the patch decomposition is unchanged from PR8.

**Measurable by**: comparing the `graph` parameter against `build_manifold_patch_graph(subdivided)` in
T2 — they must agree on `patch_of`, `patches`, `tris_a_count`. (This is a contract on callers, not on
`label_cells`'s internals.)

### I4 — Total-count conservation
Every sub-triangle gets exactly one label:
```
labeling.labels_a.len() == subdivided.tris_a.len()
labeling.labels_b.len() == subdivided.tris_b.len()
sum(|graph.patches[k]| for k) == subdivided.tris_a.len() + subdivided.tris_b.len()
```

**Measurable by**: length assertions in T2; also enforced by the existing length-mismatch check in
`LabelConsistencyWithinPatchOracle` (lines 95-111).

### I5 — Determinism
For fixed (subdivided, graph, originals, d_epsilon) inputs, `label_cells` returns identical output
across runs and across representative-pick policies that select equivalent (non-degenerate) members.

**Measurable by**: T2 secondary test — re-run `label_cells` with a manually-permuted patch member order
(simulating an alternative deterministic representative-pick policy) and assert identical output. See §5.

---

## 5. Oracles

### Existing — `LabelConsistencyWithinPatchOracle` (PR9)
Already validates I1. Post-PR11 it is expected to pass on every case where Stage 2 + Stage 4b populate
their snapshots. The oracle's docstring (`label_consistency.rs:26-34`) currently says "expected to fire
heavily — that is the point" until per-patch labeling lands; PR11 updates that docstring (and ONLY the
docstring — no behavioral change) to reflect the new "by-construction" status.

**Stays in the oracle suite** because it now serves as a builder-sentinel: any future regression that
re-introduces per-sub-tri labeling, or a `ManifoldPatchGraph` builder defect that fragments a patch,
will fire it.

### New unit-test oracle — Representative-pick consistency (T2 only, not corpus runner)
A test-only oracle invoked from the new `pr11_per_patch_labeling.rs` red-phase test file. Re-runs
`label_cells` with a deterministically permuted patch member order — emulating an alternative
representative-pick policy — and asserts:
```
label_cells(subdivided, graph_a, …) == label_cells(subdivided, graph_b, …)
```
where `graph_b` differs from `graph_a` only in the ordering of members within each `patches[k]` Vec.

**Why a test-only oracle**: corpus oracles run on production traffic; this one needs a deliberate
permutation that no production caller produces. It guards against a non-determinism risk: if PR11's
representative-pick logic depends on member ordering in subtle ways (e.g., picks `patches[k][0]`), and
the ordering ever changes (e.g., a future BFS variant in `build_manifold_patch_graph`), the labels
should be invariant up to ties between equally-valid representatives.

### Existing — `MeshArrangementWellFormedOracle` (Stage 2)
PR11 extends this oracle (per F1 sub-spec below) — does not replace it.

### Existing — `CoplanarMeshIdenticalOracle` (Stage 0)
PR11 fixes the *snapshot site* feeding this oracle (per F2 sub-spec below) — does not change the oracle
itself.

---

## 6. Failure Modes

### B1 / B2 — Monochromatic patch, all members non-degenerate
Common path. `label_sub_tri_raycast` returns a definite label (after cosurface, primary, Hoffmann, or
GWN); propagate. **No new failure modes.**

### B3 — Mixed-mesh patch
Cherchi 2022 §5 does not explicitly forbid mixed-mesh patches; in practice they arise post-Cherchi at
coplanar STAGE2 unifications where `subdivide_mesh_pair_full_cherchi` (lines 2375-2434) emits both an A
and a B sub-tri from a single Cherchi tri with `label == 3`, and these share edges.

Per `feedback_no_last_bug.md` (don't claim the last bug), I am being explicit about uncertainty here: I
read the code paths, but I have not corpus-instrumented to count how often B3 actually fires in
practice. The implementer (T3) should add a one-line `eprintln!` count at first call to confirm the
branch is exercised.

**Rule for B3**: pick a single representative sub-tri (e.g., the first non-degenerate member). Compute
its label by ray-casting against *its own* opposing mesh (A → mesh B, B → mesh A). Propagate that
single label to **every** member, writing into `labels_a` slot for A-members and `labels_b` slot for
B-members.

**Justification**: at a Cherchi STAGE2 cosurface unification, A and B sub-tris are geometrically
co-located; "is this point inside the other operand?" returns the same answer regardless of which side
emitted the sub-tri (the cosurface_orientation short-circuit in `label_sub_tri_raycast` already
collapses these cases to the same label). The per-side propagation slot is a bookkeeping detail, not a
geometry difference.

**Open question (flag for adversary T5)**: if B3 sub-tris carry different
`SubTriangle::cosurface_orientation` values (e.g., one Parallel, one None), the cosurface short-circuit
diverges. The representative pick MUST therefore propagate the representative's label uniformly —
this is correct per Cherchi §5 ("one ray, one label per patch") but means a member's
own `cosurface_orientation` is **ignored** in favor of the representative's classification. The
`feedback_yang_only.md` posture: trust the paper. Cherchi treats the patch as a single object; the
representative's label IS the patch's label.

### B4 — Representative degenerate
Walk patch members until one with non-zero cross-product is found. If the entire patch is degenerate
(unlikely — `MeshArrangementWellFormedOracle` should have rejected the snapshot upstream at Stage 2,
per F1 anchored conservation), call `label_sub_tri_raycast` on `patch[0]` anyway: its Hoffmann + GWN
fallback path will produce *some* label rather than panicking. **Do NOT silently skip the patch** — that
would violate I4 (count conservation).

### B5 — Empty patch
Cannot occur from `build_manifold_patch_graph` (every BFS seed contributes itself). Defensive: skip
silently with a debug-mode assertion.

### B6 — Deadline expiration
Same `KernelError::NotSupported` as today, with operation tag updated to identify the per-patch loop.
Check cadence: every 100 patches (was: every 100 sub-triangles). For typical corpus inputs (hundreds of
patches) this is roughly the same wall-clock checking frequency.

### B7 — Graph / subdivided mismatch
Caller bug. Return `KernelError::NotSupported`. Production callers in `topology_extract.rs` build the
graph immediately before calling `label_cells`; a mismatch would mean the caller mutated `subdivided`
between `build_manifold_patch_graph` and `label_cells`, which no current code path does.

### Upstream errors propagated
- `ManifoldPatchGraph` build is infallible (no error type today). If a future refactor adds one, it
  bubbles up to `topology_extract.rs` via `?` and Stage 5 reports `ContractViolated` per existing
  conventions.
- `label_sub_tri_raycast` returns `CellLabel` (no error). Internal degeneracy is handled by the
  Hoffmann + GWN cascade; the function never panics.

### Out of scope
- Parallel labeling across patches (Cherchi 2022 §6 mentions thread parallelism but PR11 stays
  sequential to avoid scope creep).
- Patches that span >2 input meshes (Cherchi 2022 §5 supports n-ary booleans; Yang/PR11 is binary, so
  this is a non-issue).

---

## 7. Research Basis

Primary references:
- **Cherchi 2022** — *Interactive and Robust Mesh Booleans*. §5 lines 386-417 + Algorithm 1
  (lines 383-450): for each input patch P, construct ray r emanating from a random t ∈ P toward p∞;
  test r against each input mesh M; first intersection's signed-tetrahedron volume gives in/out.
  Headline complexity claim (§5, p. 6): "the algorithm scales with the number of patches in the
  arrangement and not with the number of triangles in the mesh".
- **Yang 2025** [#24] — *Hybrid B-Rep / Mesh Booleans*. §4.4 invokes Cherchi 2022 for the in/out
  classification step of the hybrid pipeline.
- **Hoffmann 1989** §5.3 — perturb-and-classify for boundary-coincident centroids. Used inside
  `label_sub_tri_raycast` and **inherited unchanged**.
- **Jacobson 2013** [#7] — generalized winding numbers. Used as the GWN fallback inside
  `label_sub_tri_raycast` and **inherited unchanged**.
- **Shewchuk 1997** [#4] — adaptive `orient2d`/`orient3d` predicates. Used inside the BVH ray-cast
  and inherited unchanged.

### Documented deviations from Cherchi 2022 §5

1. **Representative-pick policy**: Cherchi §5.1 says "we pick a random triangle t ∈ P". PR11 picks
   *deterministically* (first non-degenerate member by patch-index order) for reproducibility per
   `feedback_no_regression_chasing.md` (BTreeMap-style determinism). This is a strict refinement: any
   deterministic in-patch member is an admissible "random" choice in Cherchi's framing.

2. **Mesh-pair binary boolean**: Cherchi 2022 §5 handles n-ary booleans; PR11 specializes to binary
   (mesh A vs. mesh B), folded into the existing `target_label` parameter of
   `label_sub_tri_raycast`. No expressive loss.

3. **Cosurface short-circuit**: PR10 added Path A-refined cosurface_orientation handling inside
   `label_sub_tri_raycast`. This is a Cherchi 2020 §5.4 / Hoffmann 1989 §5.3 compliant addition that
   was **not present in Cherchi 2022 §5** (which assumes upstream coplanar preprocessing has unified
   such cases). PR11 inherits this short-circuit; it fires per-representative now, propagated to
   patch members. Justified because Yang §4.5.5 places coplanar handling upstream of Stage 4b — a
   patch whose representative carries Parallel/AntiParallel orientation is a unified-coplanar patch,
   and the short-circuit's label assignment is geometrically correct for the whole patch.

### What is NOT a deviation
- Hoffmann fallback, GWN fallback, BVH acceleration, axis-aligned ray casting — all inherited
  verbatim from the existing `label_sub_tri_raycast` and unchanged by PR11.

---

## F1 sub-spec — Stage 2 conservation anchor

### Goal
Detect "lost-during-emit" defects (e.g., a stray `sub_tris_a.pop()` immediately before
`SubdividedMesh` is built) that the current Stage 2 oracle cannot see, because its conservation check
is tautological:
`total_directed == 3 × (|tris_a| + |tris_b|)` shrinks proportionally with snapshot size.

### Mechanism
Anchor conservation to an upstream invariant. Snapshot the upstream Cherchi `solve_intersections`
output's tri count (the source of truth for "how many sub-triangles should have been emitted") and
assert post-snapshot equals upstream. This is decoupled from the snapshot's own internal arithmetic.

### Branch table

| # | Condition | Action |
|---|---|---|
| F1-1 | Snapshot has `upstream_tri_count == effective_emitted_count` | Pass. |
| F1-2 | Snapshot has `upstream_tri_count != effective_emitted_count` | Fire `ContractViolated` with diagnostic naming both counts. |

### Invariant
```
subdivided.tris_a.len() + subdivided.tris_b.len()
    == subdivided.upstream_tri_count
       + (count of label==3 Cherchi tris that emit BOTH an A and a B sub-tri)
```

**Important**: the literal form `tris_a.len() + tris_b.len() == upstream_tri_count` is **wrong in
general** because Cherchi `label==3` (coplanar duplicate) tris emit **both** an A and a B sub-tri from
a single upstream tri (see `exact_mesh.rs::subdivide_mesh_pair_full_cherchi` lines 2391-2433). The
implementer (T4) MUST account for this duplicate-emission. Two acceptable encodings:

**(a)** Snapshot `upstream_tri_count` as the count of **distinct emitted sub-tris** (so a label==3 tri
contributes 2). The invariant becomes literally `tris_a.len() + tris_b.len() == upstream_tri_count`.

**(b)** Snapshot `upstream_tri_count` as `result.tris.len()` (the raw Cherchi count) AND snapshot
`upstream_label3_count`. Invariant:
`tris_a.len() + tris_b.len() == upstream_tri_count + upstream_label3_count`.

**Recommendation**: encoding (a) — single counter, simpler invariant, matches the natural definition of
"number of sub-triangles the arrangement emitted". T4 implementer chooses; document the choice in the
commit message.

### Field addition to `SubdividedMesh` (`exact_mesh.rs:1134`)
```rust
pub upstream_tri_count: usize,
```
Default in synthetic constructions (other call sites in `topology_extract.rs:1382`,
`label_consistency.rs` test fixtures, etc.) MUST be `tris_a.len() + tris_b.len()` so the invariant
holds tautologically for synthetic snapshots — the F1 anchor only catches real Cherchi-derived
mismatches, which is the intended detection surface.

### Oracle extension
`MeshArrangementWellFormedOracle::check` (`oracles/arrangement_wellformed.rs:106-139`) — add a check
**after** the existing directed-edge conservation check (so the existing tautology check stays in
place as a structural sanity guard, but the new check actually anchors to upstream truth).

### Failure mode
Count mismatch → `OracleViolation { kind: ContractViolated, message: "Stage 2 emit conservation
violated: subdivided.tris_a.len() + subdivided.tris_b.len() = N1, but upstream_tri_count = N2 (expected
equality per F1)" }`.

### Research basis
PR10 oracle-validity audit (`specs/oracle_validity_audit.md` §F1, lines 69-88). No published technique
— this is project-specific oracle design.

---

## F2 sub-spec — Stage 0 post-injection snapshot

### Goal
Make the Stage 0 oracle (`CoplanarMeshIdenticalOracle`) reachable on the production identical-footprint
code path. Per PR10 audit (F2), the current snapshot at `yang_integration.rs:702` records `mesh_a` /
`mesh_b` from BEFORE `inject_identical_footprint_mesh` runs at line 662, so an injected byte
divergence on operand B is invisible to the oracle in production.

### Decision required (per teammate brief)

**Choice**: **(a) re-derive `RenderMesh` from post-injection flat arrays via a small helper.**

**Justification (one sentence)**: option (a) is strictly local to the Stage 0 snapshot block (~30
lines added in `yang_integration.rs:694-717`), preserves the surrounding pipeline ordering that other
code paths and snapshot consumers already depend on, and avoids the broader call-graph reshuffle that
option (b) would impose on
`render_mesh_to_arrays` callers, `BijectiveMap::from_render_mesh`, and `compute_vertex_params` —
each of which currently consumes the pre-injection RenderMesh and would need re-validation.

**Helper signature** (new private function in `yang_integration.rs`):
```rust
fn flat_arrays_to_render_mesh(
    verts: &[[f64; 3]],
    tris: &[[usize; 3]],
    template: &RenderMesh,           // for face_ranges + normals reuse
) -> RenderMesh;
```

The helper repacks the post-injection `verts_*` / `tris_*` flat arrays into a `RenderMesh` shape
suitable for the snapshot. Normals and `face_ranges` are best-effort copies from the template
RenderMesh (the pre-injection one already in scope) — the F2 oracle compares vertex/index byte content,
not normals. T4 implementer is free to leave normals empty or recompute via `compute_newell_normal`;
the oracle does not currently check normals (verify against `coplanar_identical.rs` before T4 commit).

### Branch table

| # | Condition | Action |
|---|---|---|
| F2-1 | Identical-footprint pair present in `coplanar_pairs` | Run injection; re-derive post-injection RenderMesh; snapshot. |
| F2-2 | Partial-overlap pair present (no identical footprint) | Run injection; re-derive post-injection RenderMesh; snapshot. (Oracle still reports `OracleStub` for the partial-overlap path per existing semantics — F2 only fixes the snapshot site.) |
| F2-3 | No coplanar pairs | Snapshot the original `mesh_a` / `mesh_b` directly (pre-injection state == post-injection state when injection is a no-op). |

### Invariant
```
let post_inject_mesh_a = flat_arrays_to_render_mesh(&verts_a, &tris_a, &mesh_a);
let post_inject_mesh_b = flat_arrays_to_render_mesh(&verts_b, &tris_b, &mesh_b);
state.stage_0_coplanar.mesh_a == Some(post_inject_mesh_a)   // byte-equal vertices + indices
state.stage_0_coplanar.mesh_b == Some(post_inject_mesh_b)
```

### Oracle (unchanged)
`CoplanarMeshIdenticalOracle` (`oracles/coplanar_identical.rs`). The F2 fix is to the *snapshot
producer*, not the oracle itself.

### Failure mode
Byte divergence detected by the unchanged oracle → `OracleViolation { kind: ContractViolated, … }`
naming the divergent vertex / triangle. Same as today, but now actually reachable on the production
identical-footprint code path.

### Research basis
PR10 oracle-validity audit (`specs/oracle_validity_audit.md` §F2, lines 90-118). No published
technique — project-specific oracle plumbing.

---

## Uncertainties flagged for lead's attention before unblocking T2 / T3 / T4

1. **B3 frequency**: I have not corpus-instrumented to count how often mixed-mesh patches occur. The
   spec rule (single-representative propagation across both `labels_a` and `labels_b` slots) is
   geometrically correct per Cherchi §5 + the cosurface short-circuit, but if B3 fires very rarely the
   T2 test for it may need a hand-built fixture rather than a corpus capture. T2 implementer should
   pick a synthetic fixture (two coplanar A/B triangles with a shared edge → one mixed patch).

2. **F1 invariant encoding**: I recommend encoding (a) in §F1 above, but T4 implementer must pick one
   and match T2 author's expectations. **Suggest lead instructs T2 and T4 to coordinate** on encoding
   choice before commit (one Slack-equivalent ping is sufficient).

3. **Representative-pick "non-degenerate" definition**: "non-zero cross-product magnitude" is the
   natural choice, but the existing `MeshArrangementWellFormedOracle` already rejects fully-degenerate
   sub-tris with `cx == 0.0 && cy == 0.0 && cz == 0.0`. PR11's representative-pick should use the same
   exact-zero test for consistency (don't introduce a new tolerance threshold per `feedback_yang_only.md`
   — degenerate is defined by exact zero, not "small").

4. **F2 helper normals**: As above, the oracle does not check normals. T4 implementer may leave normals
   empty (`vec![]`) on the re-derived RenderMesh, but should add a comment noting this. If a future
   oracle checks normals, F2 will need extending — flag in commit message.

5. **WASM bridge surface**: `SubdividedMesh` is `pub(crate)` per `exact_mesh.rs:1134` — kernel-internal,
   per A15.6. Adding `upstream_tri_count` does NOT change the WASM surface. **No WASM rebuild required
   for F1**. F2 changes are entirely inside `yang_integration.rs` (also `pub(crate)`-scoped), so no
   WASM rebuild for F2 either. Lead can defer the WASM rebuild step in the T6 sequence to "only if the
   T3 refactor exposes a new symbol" — it should not.

# Spec: Yang Topology Extract — Twin-Pairing Investigation Thread

## Goal

Document the half-edge twin-pairing failure mode that PR10's Phase D
validator identified as the dominant Yang assay failure (133 of 221
Yang failures match `half_edge[X].twin = 0 but twin.twin = expected_X`),
and capture PR11's investigation finding that this is a **downstream
symptom of an upstream `label_cells` misclassification**, not a
twin-pairing bug.

This is a fresh investigation thread distinct from
`yang_555_identical_footprint.md` (the §4.5.5 cosurface annihilation
thread closed by PR3-PR10). The two threads may interact (PR10's
correct cosurface classification ought to make this surface more
clearly), but the root causes are architecturally distinct.

## Background — what the failure looks like

The Yang `validate_yang_result_topology` enforces half-edge twin
symmetry: for every HE `i`, `arena.half_edges[i].twin = X` must imply
`arena.half_edges[X].twin = i`. The error format is

```
half_edge[{i}].twin = {twin_idx} but twin.twin = {actual} (expected {i})
```

Index 0 is a real half-edge, not a sentinel. Default
`twin: HalfEdgeIdx(0)` aliases real HE 0. So `half_edge[1].twin = 0
but twin.twin = 0` typically means HE 1 is unset (defaulted to 0) AND
HE 0 is also unset (its twin also defaults to 0). Both unpaired.

Twin assignment happens in
`crates/kernel/src/boolean/topology_extract.rs:flood_fill_patches`
(lines 776-778). The pairing loop at lines 754-805 iterates undirected
edges and matches forward/reverse half-edge candidates from a
`directed_he` map keyed on directed vertex pairs.

## PR11 Phase A finding — R0002 root cause

R0002 is a 2-feature assay part: Revolve cut=false (Union), then
Revolve cut=true (Subtract). The Subtract step fails. Trace
(`TWIN_DEBUG=1` instrumentation):

```
[yang-diag] after subdivide: tris_a=650, tris_b=12872, verts=4403
[yang-diag] after label_cells: A outside=650 inside=0 cosurface=0,
                                B outside=12872 inside=0 cosurface=0
[yang-diag] after survival: 6 groups, 650 tris (ALL mesh_id=A)
[topo-extract] summary: paired=65, unpaired=12, ambiguous=0
```

For Subtract, the survival keep-table is `keep_a = Outside, keep_b =
Inside`. But `label_cells` reports zero B sub-tris classified Inside —
so zero B sub-tris survive. All 6 patches are A-only.

The 12 unpaired half-edges sit on **A-face boundary edges** — places
where two adjacent B-Rep faces of mesh A meet (e.g., FaceIdx(0)'s
outer ring meeting FaceIdx(2)/3/4/5's side faces). Each unpaired edge
has the signature `fwd_count=2, rev_count=0` — two A-A boundary HEs
going the same direction, no B-side reverse partner. The missing
reverse partners *should* be B's interior wall-tris (B revolve cuts
into A; B's interior surface inside A should classify Inside under
the Subtract keep-table). With zero B sub-tris in survival, the
A-side boundary edges have nothing to pair against.

**This is a label_cells misclassification, not a twin-pairing bug.**

### PR10 is innocent

PR11 engineer-a re-ran R0002 against PR9 head (commit `0e526ef`).
Result: identical `A outside=650 inside=0, B outside=12872 inside=0`,
identical 12 unpaired HEs, identical failing HE signature. PR10's
cosurface fix did not cause R0002. The assay regression count
(~9 → 2) is partially attributable to PR10 for cosurface-affected
cases, but R0002-class failures pre-existed.

### Hypothesis verdicts (PR11's 4-way + 5th unanticipated)

a. **Cosurface annihilation asymmetry — NO.** Zero cosurface tris in
   R0002 (`cosurface=0` for both A and B). All 22785 provenance emits
   in the batch trace show `cosurface_orientation = None`.

b. **Parallel per-side over-drop — NO.** No `Some(Parallel)` anywhere
   in this case.

c. **Pre-existing bug exposed by PR10 — YES.** Confirmed by PR9 head
   re-run.

d. **Winding-direction mismatch — PARTIAL.** The fwd=2/rev=0 signature
   IS present, but it's a *consequence* of the upstream
   misclassification (zero B sub-tris in survival), not a Cherchi
   STAGE2-style winding bug.

**Conclusion: 5th unanticipated cause — upstream `label_cells`
misclassifies all 12872 B sub-tris as Outside.** Twin-pairing failure
is a downstream symptom. Fixing twin pairing would not resolve R0002.

## PR12 scope — investigate label_cells misclassification

The fix lives in `crates/kernel/src/boolean/exact_mesh.rs` —
specifically `label_sub_tri_raycast` and its supporting BVH /
generalized winding number code. PR12 will investigate why every
B-sub-tri in R0002's Subtract returns `CellLabel::Outside` from
`ray_cast_inside` against mesh A.

Hypotheses for PR12 to test:

1. **Axis-pick degeneracy on revolve geometry**: Revolve produces
   surfaces with rotational symmetry. If the BVH ray-cast picks an
   axis (`+x`/`+y`/`+z`) that's parallel to the revolve axis, hits
   may skim the surface in a way that returns degenerate parity. The
   PR9 instrumentation showed primary returns from any non-degenerate
   axis; verify here whether the chosen axis is degenerate for the
   revolve case.

2. **Tolerance / epsilon mismatch**: PR9-PR10 work didn't touch the
   `effective_eps` value. If revolve generates near-coplanar surface
   tris that hit the epsilon threshold differently than box tris, the
   misclassification could be tolerance-driven.

3. **BVH construction over revolve mesh has a bug**: e.g., empty
   sub-trees, incorrect AABB bounds, missed triangles.

4. **GWN fallback mis-fires**: when ray-cast returns `None` (all axes
   degenerate), the Hoffmann perturb-and-classify or GWN fallback
   from PR6 may return the wrong sign on revolve geometry.

5. **`subdivide_mesh_pair_full_cherchi` returns wrong B-sub-tri
   positions**: less likely (cherchi is well-tested), but possible if
   the bijective mapping `bijective_b.tri_face_ids[parent_tri]` has
   an off-by-one for revolve features.

PR12's investigation should:
- Pick a single B-sub-tri from R0002 that should classify Inside
  (e.g., one whose centroid is geometrically inside A's solid body).
- Trace `label_sub_tri_raycast` for that single sub-tri with
  `CHERCHI_DEBUG=1` (PR9 instrumentation already exists).
- Identify whether primary returns wrong, or fallback fires wrong, or
  position is wrong upstream.
- Memo lead, lock fix scope, ship Phase B-D.

## What PR11 ships — instrumentation only

### Files modified

- `crates/kernel/src/boolean/topology_extract.rs`:
  - `let twin_debug = std::env::var("TWIN_DEBUG").as_deref() == Ok("1");` at top of `flood_fill_patches`.
  - `SurvivingSubTri` augmented with diagnostic-only fields
    `cosurface_orientation: Option<CosurfaceOrientation>` and
    `parent_tri: usize`. Derived `Default`. Production constructors
    populate from `SubTriangle`. 16 in-file test constructors updated
    to `..Default::default()`.
  - `FlatSubTri` (struct internal to `flood_fill_patches`) gets the
    same two fields, populated from `SurvivingSubTri`.
  - New per-HE insert trace at insertion site (`[twin-debug] insert
    HE[i] (vN→vM) source=… parent_tri=… cosurface=…`).
  - New per-edge candidate-count trace in pairing loop
    (`[twin-debug] edge (lo,hi) fwd_count=N rev_count=M fwd_hes=[…]
    rev_hes=[…]`).
  - New `[twin-debug] paired HE[a] ↔ HE[b]` on success.
  - New `[twin-debug] UNPAIRED HE[i] (...): provenance=…` and
    `[twin-debug] AMBIGUOUS HE[i] (...): fwd_prov=… rev_prov=…`.
  - All existing `[topo-extract] unpaired forward HE`, `[topo-extract]
    ambiguous twin`, `[topo-extract] summary`, `[flood_fill DIAG
    Step5a]`, and `[yang-diag] flood_fill_patches: N unpaired` emits
    gated behind `twin_debug` (previously always-on).

- `crates/kernel/src/boolean/yang_integration.rs`:
  - At validation FAIL site (~L1028), new `[twin-debug] FAIL HE[i]`
    emit gated on `TWIN_DEBUG=1`. Production `Err(...)` message
    format unchanged.

### Verification

- `TWIN_DEBUG` unset = no behavior change (smoke check confirmed: 0
  twin-debug emits, 0 [topo-extract] emits across `batch_2op_extrude`
  with TWIN_DEBUG unset).
- `cargo test -p kernel --lib boolean::topology_extract`: matches
  baseline (PR10).
- `cargo test -p kernel --lib boolean::yang_integration`: matches
  baseline.
- `cargo clippy -p kernel --no-deps -- -D warnings`: no new warnings
  vs origin/main (93 baseline).
- `cargo fmt --check -p kernel`: clean.

### Out of scope for PR11

- Pre-existing red tests in `topology_extract.rs` (e.g.,
  `test_flood_fill_two_overlapping_boxes`) with nondeterministic
  unpaired counts varying 16-19 between runs. Pre-existing red,
  unrelated to PR11.
- The `label_cells` misclassification fix — that's PR12.

## References

- Yang et al. 2025 [#24] §4.4 (binary classification), §4.5 (B-rep
  extraction from labeled mesh), §4.5.5 (coplanar handling).
- Cherchi et al. 2020 §5.1 (Algorithm 1), §5.4 (coplanar pocket map),
  §5.5 (region growing — relevant to flood_fill_patches).
- ARCHITECTURAL_INVARIANTS A15.6 (Yang hybrid pipeline; flood-fill
  patch segmentation; do NOT use boundary-edge-chaining or greedy
  twin-pairing — already followed correctly in `flood_fill_patches`).
- Hoffmann 1989 §5.3 (perturb-and-classify; not directly relevant to
  this thread but cited in PR6 and continues to live in
  `label_sub_tri_raycast`).
- PR1-PR10 commit refs: `9fccebc`, `6695be1`, `12cf789`, `35ee814`,
  `17216a7`, `2b625f2`, `3d88fab`, `14f402f`, `0e526ef`, `5801571`.
- PR11 commit ref: TBD (this commit).

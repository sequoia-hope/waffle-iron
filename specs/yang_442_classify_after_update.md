# §4.4.2 order — classify AFTER the update (ledger N80, matrix O-1)

**Status: DESIGN (2026-10-17).** No code yet. A prerequisite named by
`specs/yang_faithful_cleanup.md` Phase 2 before §4.4.1 (Phase 2b) widens
further. Survey of the live code: a read-only pass over `boolean.rs`,
`stage4_correct.rs`, `stage5_topology.rs`, `stage3_ssi.rs` and cherchi-rs's
labeling, 2026-10-17; cites below were opened when written (line numbers
drift, names are stable).

## 1. The paper's order

Yang 2025 (`refs/text/yang2025_hybrid_boolean.txt`):

1. §4.2 intersect the two meshes;
2. §4.3 optimize the intersection points onto the exact curves; a mesh
   intersection with no solution in one of the two parametric domains is a
   solving failure and Case IV — the meshes meeting where the surfaces do
   not — is RULED OUT (`:527-536`);
3. §4.4.1 trim and update BOTH meshes so they share the optimized curves,
   r_A = r_B = r (`:546-571`);
4. §4.4.2 "After trimming the meshes using the intersection curves, we
   directly apply a standard inside/outside classification step [Cherchi et
   al. 2022] to identify the triangles that need to be retained"
   (`:574-578`, right column); patches are segmented along the optimized
   boundary curves (§3 Fig. 2e, `:318-321`; `:581-594`).

## 2. What the code does

`boolean_once` (`boolean.rs`): (1) Stage 2 builds the exact labeled
arrangement; (2) welds; **(3) `la.keep_set(op)` keeps the op's triangles**;
(4) compacts the kept submesh and applies the per-op winding flip and the
§4.5.5 membrane side rule; (5) Stage 6 face resolution; (6)
`reconstruct_topology_stage4` → `stage4_relocate_and_correct_inner` runs §4.3
relocation, §4.5.3, the §4.4.1 update (`update_patches`) and the repairs on
the KEPT submesh.

**The labels** (`cherchi-rs` `labeling/native.rs`): `compute_all_patches`
floods across manifold edges, so a Cherchi patch stops at every
intersection edge (four triangles); one exact ray per patch
(`compute_inside_out`) labels it; every triangle copies its patch's bits.
Labels are therefore **per Cherchi patch, cut by the RAW mesh-intersection
curves** before any optimization, and computed in exact arithmetic on a soup
that is dropped when the `LabeledArrangement` is built — no ray-cast
structure survives into yang-rs. `keep_tri` reads `surface` / `inside` /
`num` only, no geometry.

(The N80 entry said "per arrangement CELL"; corrected with this spec.)

## 3. When does the order change the answer?

`keep_tri` reads no geometry, so **pure vertex motion cannot change the kept
set**, and neither can an edit whose new triangles inherit their parent's
label — an edge split, a Fig-11(a) split inside one patch, the re-CDT of one
patch. `collapse_vertex` drops slivers from every side alike. So today's
order gives the paper's kept set EXCEPT where a pass changes the patch
structure:

- **(a) Case-IV rule-out (N68).** The paper removes the phantom curve; the
  patches on its two sides become one patch, which needs ONE label — a new
  ray, because the old rays started from interior vertices relocation does
  not move and would repeat the old answer. Today: a closed refuted loop
  STOPs (`certify_phantom_loops`) and the §4.5.2 loop refines it away; an
  open phantom (a bump on a real curve) is not ruled out at all.
  *Watertightness catch:* after a rule-out, A's corner tip and B's sag sheet
  are both kept and still meet within d_ε; the paper rebuilds its B-Rep from
  patch segmentation and tolerates this, while our `emit_topology` and the
  Stage-4 gate need a 2-manifold mesh. A faithful rule-out therefore also
  needs the sag re-meshed locally — §4.5.2's "compute the intersections …
  only in the refined regions" (N62, M-1).
- **(b) §4.5.3 collapses** that shrink a loop below three vertices: if both
  bubble patches vanish, no relabel; if the sides merge, it is (a).
- **(c) A relocation across a third surface** (the §4-I9 corner-crossers):
  the cell structure itself is wrong; no relabel fixes it; it stays a STOP
  that names faces (T1) — §4.5.2.
- **(d) Pre-CDT 2D collapse / relocate-in-place (`N2`, `N81`) on dropped
  patches, plus Stage-5 vertex-index-dependent chain decimation (`N81`).**
  The argument above assumed Stage 4 updates every patch cleanly by
  parametric CDT (`update_patches`). In the paper, §4.3 (`:535-536`) and
  §4.5.3 (`:687-690`, `:718-721`, `:746-751`) edit the **1D intersection
  curve polyline** (`"removes the reversed point, and locally corrects the
  intersection curve"`), and §4.4.1 (`:550-562`, Fig. 11) then maps
  `r_A = r_B = r` onto $M_A, M_B$ and CDT-retriangulates every trimmed patch
  in parameter space before §4.4.2 (`:574-579`) classifies retained
  triangles. Today's Stage 4 instead runs 2D vertex-index relabelling
  (`collapse_vertex` in PR-KV9 junction-twin dedup, §4.5.3
  `sweep_reversed_intersections`, and §4.4.1(b) sub-feature merge) BEFORE
  `update_patches`, leaves refused/unhandled patches on relocate-in-place +
  post-hoc `(3d)` triangle unzipping (`4.4-03`, `4.4-12`, `N2`), and finishes
  with Stage-5 polyline chain decimation (`stage5_topology.rs:6796`, one of
  `N81`'s three §4.4.1 sites). Pre-Stage-4 `keep_set` masked all three on
  dropped patches; empirical `I3b` evaluation (2026-10-27) exposed them at
  three sites:
  1. `yang-rs` permanent pin `tangency_pinch_split::c0058_authored_geometry_union_mints_its_tangent_points`
     (matrix `4.4-06` pin; `la tris 360`: `A-out=105`, `A-in=77`, `B-out=109`,
     `B-in=69`): in dropped patch `A-in` (`inside = [false, true]`, a cap-free
     elliptical dome triangulated by cross-ellipse chords), `PR-KV9`
     (`41 -> 34`) + §4.5.3 (`43 -> 34` on `loop=[34, 43, 79, 78]`) relabel
     five vertices on one planar Steinmetz ellipse so triangles `[77, 34, 78]`,
     `[34, 77, 79]`, and `[77, 34, 46]` all share undirected edge `(34, 77)`
     (`3` incident triangles in `A-in`, `2` fwd / `1` rev), breaking
     `patch_boundary_cycle` (`s6-boundary-walk-deadend: vertex 77`) before
     `update_patches` runs and exhausting §4.5.2 at `16×`
     (`NonManifoldOutput`) — the exact `N2` mechanism documented on its
     quarantined twin (`tangency_pinch_split.rs:333-344`: *"a braid collapsed
     onto a minted node must be RE-TRIANGULATED, not relabelled"*).
     (`ASSAY_CASE=C0058` passes in `47.64s` as `SUPPORTED_CORRECT`.)
  2. Corpus case `R0063` (`Subtract`, `SUPPORTED_CORRECT → ERROR` in `8.88s`):
     on dropped cutter patches `(B, 1)` and `(B, 5)` (`B-out`), junction
     relocation seats `v31` at `t = 0.8026` on their shared planar crease edge
     `(177, 175)`, leaving a collinear opposite-winding degenerate pair
     `tri 426 = [177, 175, 31]` (`(B, 1)`) and `tri 503 = [31, 175, 177]`
     (`(B, 5)`). Because `(3d)` unzip (`4.4-12 DEVIATES`, `N2`) replaces
     Fig. 11(a)'s boundary-edge split inside per-patch CDT (`:555-562`) and
     cannot resolve two degenerate triangles sharing the same off-vertex
     `b = nb = 31`, Stage 4 STOPs with `LocalRefinementRequired` at sentinel
     `v = 4294967295` (`u32::MAX`, `faces = []`, `stage4_correct.rs:6214`).
  3. `kernel-v2` `b2_pipe` (`3` `s_bend()` tests:
     `"torus lateral missing its +axis seam arc"`): numbering vertices across
     the full arrangement shifts compact vertex-index parity between Bend 1's
     two closed 8-gon rims `chain 1` (`(0,0,0)`) and `chain 2` (`(0.3,0.3,0)`),
     so Stage-5 chain decimation (`N81`) keeps phases `{0, ±π/2, π}` on
     `chain 1` and `{±π/4, ±3π/4}` on `chain 2` (`anchor = Some(25)` at
     `z = +0.0354`), deleting the constructor's original `+π/2` seam vertex
     `(0.3, 0.3, +0.05)` on `chain 2`. *(Resolved 2026-10-28/30 in Roadmap F2 Step 1:
     `stage5_chain_decimate` deleted and re-sited to §4.4.1
     `stage4_chain_decimate::decimate_patch_chains_441` with forward §4.3.4
     endpoint subdivision and operand-anchored closed loops, and
     `merge_conic_seam_runs_with_operand_verts` preserves operand `BRepVertex`
     anchors; under an uncommitted I3b trial — routing `la.mesh`, full-mesh
     `attribution`, and `TriangleSideLabel::from_arrangement` into
     `reconstruct_topology_stage4_full_arrangement` in `boolean.rs` —
     `~/.claude/bin/reserve -c 4 -l "b2_pipe" -- cargo test --release -p kernel-v2 --test b2_pipe`
     passes `17 passed; 0 failed` in `0.41s`, even without `developable.rs`.)*

So O-1 is observable through (a)–(d): (a) is the paper mechanism the order
disables (C0117, cleanup §2a, and N68 trace to it), while (d) establishes
that Roadmap **F2** (`N2` / `N81`: decoupling 1D §4.3/§4.5.3 curve point
removal from 2D pre-CDT triangle relabelling, replacing post-hoc `(3d)` unzip
with Fig. 11(a) boundary-edge splitting in CDT, and retiring Stage-5 chain
decimation) is a prerequisite before `I3b` flips `boolean_once`.

## 4. What assumes "kept triangles only"

On the kept submesh an intersection edge has two triangles, one per
operand; on the full arrangement it has four (A-in, A-out on one
attribution; B-in, B-out on the other). Every consumer below reads the
two-triangle form:

| consumer | site | on the full arrangement |
|---|---|---|
| per-op winding flip, §4.5.5 membrane side rule | `boolean.rs` step (4) | must move after the keep |
| wedge dedup, coincident-triple guard | step (4) | could fuse a kept with a dropped triangle |
| `flood_fill_patches` (key = attribution) | `stage5_topology.rs` | **merges A-in with A-out across the curve** — the first break |
| `patch_boundary_cycle` | same | curve edges become patch-interior; cycles lose the curve |
| `build_intersection_curves` (`len != 2` skip) | `stage3_ssi.rs` | every curve edge has four entries — every curve dropped |
| `split_pinch_vertices`, `carrier_counts`, `vertex_on_curve` | Stage 4 | counts double |
| §4.4.1(a) unzip ("the triangle on the other side") | `stage4_correct.rs` | ambiguous |
| `collapse_vertex` membrane cancellation (exactly-2 pairs) | same | wrong pairs |
| watertight gate (directed half-edge balance) | Stage 4 | passes VACUOUSLY (2 forward / 2 reverse) |
| `update_patches` (one attribution per patch) | `stage4_construct.rs` | patches merged by the flood |
| `certify_phantom_loops` (cross-input entries) | `stage4_phantom.rs` | works as is |
| `emit_topology` | `stage5_topology.rs` | must stay kept-only |

## 5. Increments

Each lands as the cleanup's method requires (primitive with pins → one
wired increment → corpus run → the substitute retires in the same series).

**I1 — carry the labels through Stage 4 (byte-identical).** A per-triangle
side vector `(cherchi patch, inside bits, surface)` threaded in lockstep with
`TriangleAttributionMap` through every mutator of `mesh.tris`
(`collapse_vertex`, the split helpers, `apply_rebuild_batch`,
`collapse_subresolution_intersection_segments`,
`collapse_subtauwork_mesh_edges`, `split_pinch_vertices`) — NOT a field of
`TriangleAttribution`, whose equality is the flood key. Acceptance: a debug
assertion that the carried labels re-derive today's kept set exactly;
corpus byte-identical. Restores no order by itself; it is the plumbing I2
and I3 both need.

*Landed 2026-10-27:*
- **Types (`crates/yang-rs/src/brep.rs`):** `TriangleSideLabel { cherchi_patch: u32, inside: [bool; 2], surface: [bool; 2] }` (`from_arrangement`, `keep_for_op`) and `TriangleSideLabelMap { labels: Vec<TriangleSideLabel> }` (`keep_set`), kept separate from `TriangleAttribution` so `flood_fill_patches` equality is untouched.
- **Origin (`crates/yang-rs/src/boolean.rs`):** After pre-Stage-4 submesh compaction (`kept_submesh` + `cancel_subresolution_pleats`, both maintaining `orig_tri` 1:1 with `submesh.tris`), `side_labels` is built 1:1 via `TriangleSideLabel::from_arrangement(&la, orig_t)` and passed `&mut side_labels` into `reconstruct_topology_stage4`.
- **Complete Stage-4/5 mutator inventory audited and threaded:**
  1. `apply_rebuild_batch` (`stage4_construct.rs`) — called from `update_patches` (`stage4_construct.rs`), `corner_transit_apply` (`stage4_correct/corner_transit.rs`), `run_construct_passes` (`stage5_topology/construct.rs`), and `run_fold_merge_passes` (`stage5_topology/fold_merge.rs`). Rebuilt patches assert uniform `TriangleSideLabel` in debug and stamp it onto replacement CDT triangles; non-replaced slots keep their labels.
  2. `transit_emission_write` (`stage4_boundary_curve.rs`) — called from `stage4_relocate_and_correct_inner` (`stage4_correct.rs`). Snapshots each fill polygon's `(InputId, face)` label from `fill.removed` before slot-stable overwrite + append.
  3. `collapse_vertex` (`stage4_correct/collapse.rs`) — called from §4.5.1 repair (`stage4_correct.rs`), PR-KV9 junction-twin collapse (`stage4_correct.rs`), `weld_coincident_relocated` (`collapse.rs`), `trim_beyond_corner_phantoms` (`collapse.rs`), `sweep_reversed_intersections` (`stage4_correct/reversal.rs`), §4.4.1(b) sub-feature merge (`stage4_correct.rs`), `collapse_subtauwork_mesh_edges`, and `collapse_subresolution_intersection_segments`. Drops degenerate and opposite-winding pleat slots in lockstep.
  4. `retriangulate_collapsed_fan_regions` / `repair_fan_cluster` (`stage4_correct/collapse.rs`) — called from `stage4_correct.rs`. Rebuilt region triangles inherit the region's `TriangleSideLabel`; removed region triangles drop in lockstep.
  5. `resolve_simple_degenerate_split` (`stage4_correct/collapse.rs`, extracted from inline §4.4.1(a) split in `stage4_correct.rs`) — drops `(d_idx, n_idx)` and appends the two split children carrying `n_idx`'s `TriangleSideLabel`.
  6. `resolve_mutual_degenerate_pair` (`stage4_correct/collapse.rs`) — drops `(t1, t2, nl, nh)` and appends `nl`'s two children with `nl`'s label and `nh`'s two children with `nh`'s label.
  7. `remove_doubled_membranes` (`stage4_relocate.rs`) — filters `side_labels` alongside `mesh.tris` and `attribution`.
  8. `split_pinch_vertices` (`stage4_relocate.rs`) and `compact_unreferenced_verts` (`stage4_correct/collapse.rs`) — rewrite vertex indices inside `mesh.tris` in place without adding, removing, or reordering triangle slots; take `&[TriangleSideLabel]` and `debug_assert_eq!(side_labels.len(), mesh.tris.len())`. (Vertex-only `&mut Mesh` helpers `apply_boundary_relocations` and `corner_transit_apply`'s rollback closure leave `mesh.tris` untouched.)
- **Hand-off assertion & pins:** In `reconstruct_topology_stage4` (`stage5_topology.rs`), immediately before `emit_topology`, `debug_assert_eq!` verifies `side_labels.len() == mesh.tris.len() == attribution.attributions.len()` and `side_labels.keep_set(op) == (0..mesh.tris.len()).collect::<Vec<_>>()`. Unit pins in `crates/yang-rs/src/tests_unit/s442_side_labels.rs` (plus `s451_crease_domain.rs` and `stage4_construct.rs` inline tests); debug `yang-rs` (1521 passed, 0 failed) and `kernel-v2` (869 passed, 0 failed) green; canonical release corpus `360C / 0W / 9E / 5EE / 0T` over 374 (0 moved cases, `928.7s` wall vs 2026-10-25 `~930s`).

### Pre-I3 cost measurement (2026-10-27)

Measured at `cae429f2` (post-I1, pre-I3) under booked isolation (`ASSAY_JOBS=1`,
`ASSAY_CASE_TIMEOUT_SECS=900`, `YANG_STAGE_TIME_PROBE=1`, `YANG_KEEP_PROBE=1`,
`cargo test --release -p test-harness --test assay_kv2 single_case -- --ignored --exact --nocapture`)
across six corpus cases spanning arrangement sizes (`344` to `804,340` triangles):

| Case (wall) | Op / rung | `la tris` ($T_{\text{full}}$) | `kept` | Stage-4 entry `tris` ($T_{\text{s4}}$) | $r = T_{\text{full}} / T_{\text{s4}}$ ($r^2$) | Verbatim `[stage-time]` & `[s4-lap]` | Projected $O(T^2)$ $\Delta t$ |
|---|---|---:|---:|---:|---:|---|---:|
| `C0058` (`56.84s`, 2 replays + check) | `Union` natural | `344` | `204` | `204` | `1.69×` (`2.84×`) | `total=0.05s pre_s2=0.00 s2=0.03 keep_resolve=0.00 s4_emit=0.02` (`441-update 0.00s`) | `+<0.1s` |
| `P0003` (`0.45s`) | `Subtract` natural | `584` | `96` | `96` | `6.08×` (`37.0×`) | `total=0.02s pre_s2=0.00 s2=0.02 keep_resolve=0.00 s4_emit=0.00` | `+<0.1s` |
| | `Subtract` `iter 0 k=2` | `1788` | `142` | `142` | `12.59×` (`158.5×`) | `total=0.04s pre_s2=0.00 s2=0.04 keep_resolve=0.00 s4_emit=0.00` | `+<0.1s` |
| `P0053` (`8.33s`) | `Subtract` natural (STOP) | `4740` | `3058` | `3058` | `1.55×` (`2.40×`) | `STOPped total=0.55s pre_s2=0.07 s2=0.38 keep_resolve=0.04` | `+0.1s` |
| | `Subtract` `iter 0 k=2` (STOP) | `4792` | `3062` | `3062` | `1.56×` (`2.45×`) | `STOPped total=0.56s pre_s2=0.07 s2=0.37 keep_resolve=0.04` (`before-3c-merge 0.02s 441-update 0.01s before-transit 0.01s`) | `+0.1s` |
| | `Subtract` `iter 1 k=2` | `5136` | `3196` | `3196` | `1.61×` (`2.58×`) | `total=3.46s pre_s2=0.07 s2=0.40 keep_resolve=0.04 s4_emit=2.96` (`phase-a 0.01s before-3c-merge 0.01s 441-update 0.01s before-transit 0.01s`; rest is `emit_topology`, which stays kept-only) | `+<0.2s` |
| `R0085` (`250.44s`, 2 replays of 2-op tree) | Op 1 `Union` natural (STOP) | `33395` | `27467` | `26838` | `1.24×` (`1.55×`) | `STOPped total=32.71s/37.42s pre_s2=4.61/4.95 s2=2.27/2.41 keep_resolve=18.50/20.63` (`before-3c-merge 7.11s/9.15s 441-update 0.06s/0.07s`) | `+4.5s`/replay |
| | Op 1 `Union` `iter 0 k=8` | `143800` | `137214` | `136484` | `1.05×` (`1.11×`) | `total=16.60s/19.37s pre_s2=6.22/6.98 s2=4.07/4.30 keep_resolve=0.84/0.92 s4_emit=5.47/7.15` (`phase-a 0.19s/0.20s before-3c-merge 3.85s/4.94s 441-update 0.26s/0.33s before-transit 0.31s/0.42s`) | `+0.8s`/replay |
| | Op 2 `Union` natural | `25968` | `19770` | `19230` | `1.35×` (`1.82×`) | `total=46.12s/47.55s pre_s2=28.43/29.69 s2=13.21/13.40 keep_resolve=1.99/1.97 s4_emit=2.50/2.49` (`phase-a 0.13s before-3c-merge 0.88s 441-update 0.18s before-transit 0.16s/0.15s`) | `+2.0s`/replay |
| `R0044` (`430.91s`) | `Union` natural (STOP) | `102588` | `100212` | `100212` | `1.024×` (`1.048×`) | `STOPped total=26.62s pre_s2=0.17 s2=1.29 keep_resolve=0.08` (`phase-a 0.15s before-3c-merge 23.91s 441-update 0.28s before-transit 0.26s transit 0.07s`) | `+1.2s` |
| | `Union` `iter 0 k=4` | `403684` | `396142` | `396142` | `1.019×` (`1.038×`) | `total=77.98s pre_s2=0.58 s2=5.10 keep_resolve=0.38 s4_emit=71.87` (`phase-a 0.66s before-3c-merge 61.46s 441-update 2.07s before-transit 1.42s transit 0.54s`) | `+2.7s` |
| | `Union` `iter 1 k=2` | `804340` | `789910` | `789910` | `1.018×` (`1.037×`) | `total=151.48s pre_s2=1.18 s2=35.10 keep_resolve=0.86 s4_emit=114.18` (`phase-a 1.55s before-3c-merge 81.70s 441-update 8.56s before-transit 4.07s transit 0.25s`) | `+4.2s` |
| `F0067` (`547.56s`, 2 replays of 9-op tree) | Ops 1–8 `Union` | `72..11108` | `72..11108` | `72..10528` | `1.00–1.06×` (`≤1.11×`) | Ops 1–7 `s4_emit=0.00..0.05s`; Op 8 `total=10.21s/10.33s s2=4.28/4.31 s4_emit=0.16/0.17` | `+<0.1s`/replay |
| | Op 9 `Union` natural + `iter 0..3 k=2` | `15450..17158` | `15450..17158` | `13544..15052` | `1.14×` (`1.30×`) | `total=42.92s..58.64s pre_s2=9.95..14.67 s2=29.66..40.89 keep_resolve=2.19..2.57 s4_emit=0.54..1.23` (`before-3c-merge 0.02..0.17s 441-update 0.09..0.16s`; `1906..2106` `[InputId(0), InputId(1)]` overlap-sheet tris retained through Stage 4 under I3) | `+1.7s`/replay |

**Feasibility verdict:** On the two slowest corpus cases (`F0067` at `547.56s`, dominated by Stage 2 `s2=29.66..40.89s`/rung with `s4_emit ≤ 1.23s`; `R0044` at `430.91s`, where `Union` already retains `98.2%` of the `804,340`-triangle arrangement so $r = 1.018\times$, $r^2 = 1.037\times$), even worst-case $O(T^2)$ scaling on every Stage-4 pass adds `< 9s` per case (`~551s` and `~439s`), leaving `> 340s` of headroom below the `900s` corpus timeout. Higher full/kept ratios occur on `Subtract` cases where a large cutter lies mostly outside the target (`P0003`: `12.59×` at `1788 / 142`; `P0053`: `1.61×` at `5136 / 3196`), where Stage 4 itself runs in `< 0.05s`. I3 is feasible within the `900s` budget without early-outs.

**I3 — Stage 4 on the full arrangement, keep afterwards.** Stage 4 receives
the welded full `la.mesh` with N4 attribution on every triangle and I1's
labels; the keep, the winding flip and the membrane rule run after it;
`reconstruct_topology_stage4` splits into Stage 4 (full mesh) and Phase A +
emission (kept mesh). Required: flood key `(attribution, cherchi patch)`;
curve incidence deduped by `(input, surface)`; the unzip's other side read
per operand; membrane cancellation and pinch splitting per operand; a
per-operand closed-manifold gate in place of the vacuous half-edge balance.
This restores §4.3 → §4.4.1 → classify literally: r_A = r_B = r through the
shared vertex index on two trimmed meshes. Cost: the full arrangement is
about 1.6–1.7× the kept submesh (P0053 5,722 vs 3,612; 6,388 vs 3,882;
two canaries 420/246, 840/480 — `YANG_KEEP_PROBE`), and Stage 4's
whole-list passes (§4.4.1(b) merge, the §4.5.3 sweep's `compute_phase_a`
per pass, the unzip) are up to O(T²) — about 2.7× worst case. Measure before
flipping. A "band" variant (keep a one-ring of dropped triangles) is
rejected: it leaves open boundaries that break the cycles and the gate.
*(I3a full-arrangement Stage-4 primitives and unit pins landed 2026-10-27,
unwired in `boolean_once`, including exact `TriangleSideLabel` scoping in
`collapse_vertex_scoped` / `remove_doubled_membranes_scoped`, CW-complex
per-patch interior-chord counting in `check_watertight_2manifold_per_operand`,
and `BoolOp::Xor` winding parity in `flip_winding_for_op` — 9 new pins in
`s442_side_labels.rs` + `stage4_construct.rs`, `yang-rs` `1530 passed; 0 failed;
8 ignored`. Empirical I3b evaluation wiring `boolean_once` across all 76
`yang-rs` targets (`1526 passed; 3 failed; 8 ignored` initially, with
`yr10_adversary` `4 passed; 0 failed` and `n2_junction_cluster` `5 passed; 0
failed` after the exact-`TriangleSideLabel` and CW-chord hardenings above, plus
`ASSAY_CASE=C0058` `SUPPORTED_CORRECT` in `47.64s`) and all 82 `kernel-v2`
targets (`80/82` targets pass, `865 passed; 4 failed; 10 ignored`) hit §3(d)'s
mandatory STOPs on `tangency_pinch_split::c0058_authored_geometry_union_mints_its_tangent_points`,
corpus case `R0063`, and `b2_pipe`'s 3 `s_bend()` tests. `I3b` `boolean_once`
flip is therefore sequenced immediately after Roadmap F2 retires pre-CDT 2D
relabelling, `(3d)` post-hoc unzip, and Stage-5 index-parity decimation.)*

**I2 — the Case-IV rule-out, re-keyed.** With I3 the phantom certificate
returns to `boolean_once` with `la` alive: merge the patches across the
phantom edges, relabel the merged patch by one new ray (a new f64 ray
caster against the updated other operand, or a cherchi-rs Stage-2
re-entry), and re-keep. Because of §3(a)'s watertightness catch it lands
WITH §4.5.2 locality (M-1): the sag is re-meshed in the refined region.
C0117 and N68 are its customers; the Case-IV phantom boost retires with it.

Order: I1, then I3, then I2 inside the M-1 work. I1 is safe to start now;
I3 waits for the measurement of §5's cost on the corpus at I1.

## 6. Ledger

N80 moves OPEN → RESOLVED when I3 lands (the order) and I2 lands (the
rule-out it enables); N68 resolves with I2; N81's three §4.4.1 sites become
one under I3. Each increment amends the entries it touches in its own
commit (P2).

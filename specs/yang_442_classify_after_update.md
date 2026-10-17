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

So O-1 is observable only through (a)–(c), and (a) is the paper mechanism
the order disables: C0117 (cleanup §2a, the phantom boost) and N68 trace to
it.

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

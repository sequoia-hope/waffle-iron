## PR-Y15c-fix Phase 0 (v3) — Per-face dispatch ABORT validation

**Author:** adversary-6 (NEW agent; full role rotation per `feedback_oracle_credibility_via_role_separation.md` — NOT adversary-5).
**Date:** 2026-05-05.
**Diagnostic under review:** `docs/audits/pr_y15c_fix_phase0_v3_diagnostic.md`.
**Spec:** `specs/yang_pr_y15c_fix_phase0_v3_per_face_dispatch.md`.
**Reproducer:** `batch_enclosed_subtract_fix` at `crates/test-harness/tests/assay_randomized.rs:445`.
**Probe family used (independent, never overlapping with implementer-i's tags):** `[adv6-bounded-entry]`, `[adv6-face-dispatch]`, `[adv6-cyl-fn-entry]`, `[adv6-unequal-ring]`, `[adv6-surface-map-A/B]`, `[adv6-result-assembly-entry]`, `[adv6-planar-fan]`, `[adv6-planar-centroid-fan]`, `[adv6-planar-earcut]`, `[adv6-planar-earcut-result]`, `[adv6-planar-earcut-holes]`. All inserted, exercised, and reverted to a byte-clean working tree (§5).

## Verdict

**ACCEPT_ABORT.** implementer-i's canary discipline is correct, the L4053 unequal-ring earcut hypothesis is genuinely refuted, and the spec §4.2 P10 trigger fires. Wrong-anchor count for the PR-Y15c-fix arc moves to **2 of 3** (v1 weld site refuted; v3 L4053 refuted). My independent canary trace reproduces implementer-i's tally byte-for-byte; the all-Planar finding is REAL and is an **A15.5 violation** (load-bearing for v3-redirect direction); diff is byte-clean except for the documented results.json winding-count non-determinism on F0037/F0040.

## §1. Independent canary trace verification

I inserted my own canaries (`[adv6-*]` tags, distinct from implementer-i's `[unequal-ring-*]` family) at the same critical sites: `tessellate_solid_bounded` entry (mod.rs:4181), per-face match dispatch (4184), `tessellate_cylindrical_face_bounded` entry (3489), and the `} else {` unequal-ring branch (4027). Ran `YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 cargo test … batch_enclosed_subtract_fix --ignored --nocapture --test-threads=1`.

| Canary | Tally | Implementer-i tally | Match |
|---|---:|---:|:---:|
| `[adv6-bounded-entry]` | 20 (10× len=6 + 10× len=10) | 20 | ✓ |
| `[adv6-face-dispatch]` | 160 (16/case × 10) | 160 | ✓ |
| `[adv6-face-dispatch] geom=Planar` | **160 / 160 (100%)** | 160 / 160 | ✓ |
| `[adv6-cyl-fn-entry]` | **0** | 0 | ✓ |
| `[adv6-unequal-ring]` | **0** | 0 | ✓ |

`tessellate_cylindrical_face_bounded` is never invoked on this cohort. The unequal-ring branch is unreachable. **L4053 silent-failure earcut hypothesis REFUTED.** Decision-tree row 3 (canary doesn't fire) is the correct row determination.

## §2. All-Planar finding — A15.5 violation, NOT legitimate degradation

This is the load-bearing finding. The all-Planar tagging is **not** legitimate "polygonal approximation during boolean": **the Yang result-mesh assembly receives correctly-tagged Cylindrical surface_map entries from the operands and discards them.**

Evidence: I added `[adv6-surface-map-A/B]` probes inside `build_surface_map` (`yang_integration.rs:115-127`) and `[adv6-result-assembly-entry]` inside `result_topology_to_waffle_solid` (yang_integration.rs:204).

| Probe | Result |
|---|---|
| `[adv6-surface-map-A]` per call | **5 Cylindrical + 40 Planar** |
| `[adv6-surface-map-B]` per call | **5 Cylindrical + 40 Planar** |
| `[adv6-result-assembly-entry]` per result | `surface_map.size=9 face_provenance.size=10 surface_map_breakdown={"Cylindrical":1,"Planar":8}` (10 fires, identical) |

The `surface_map` reaching `result_topology_to_waffle_solid` carries the **Cylindrical tag** for the 1 cylinder side face (correctly propagated from the operand's `cylinder_to_face_polys` at `boolean/mod.rs:583-588`). But `result_topology_to_waffle_solid` takes `_surface_map` (underscore-prefixed = unused) and at L235-264 unconditionally writes `SurfaceGeom::Planar` for every face from the Newell normal:

```rust
// yang_integration.rs:241-263
let mut face_geometry = BTreeMap::new();
for &face_idx in result.face_provenance.keys() {
    …
    face_geometry.insert(face_idx, SurfaceGeom::Planar(Plane { origin, normal }));  // ← always Planar
}
```

Per A15.5 (`governance/ARCHITECTURAL_INVARIANTS.md:453-472`): "Boolean operations must preserve surface tier for unmodified faces. When a face passes through a boolean operation without being split by an intersection curve, it retains its original `SurfaceGeom` variant — an analytic face remains analytic." The cylinder side face in box-minus-enclosed-cyl is exactly such an unmodified face (or a trimmed unmodified face — the trim does not change the underlying surface). It must stay Cylindrical. It does not. **A15.5 violated.**

The spec `specs/yang_face_geometry_propagation.md` documents the Newell-fallback as a chained-boolean correctness fix — but it overshoots: it should only apply when `surface_map` lookup fails, not unconditionally. The current code never even tries the lookup (L207 prefix `_surface_map` confirms intent-to-ignore).

This finding is load-bearing because it reframes the −8 tris/case loss: it is plausibly a **downstream symptom of cylindrical→planar tag-loss**. A face that should have been tessellated by `tessellate_cylindrical_face_bounded` (which would emit a quad strip with intermediate axial rows, often >12 tris) is instead going through `tessellate_planar_face_bounded` (which fan-tessellates, emitting n-2 tris for an n-gon boundary). For a boundary of size 8 (the cylinder side's hexagonal-to-octagonal trim), the planar fan produces 6 tris — far fewer than the cylindrical bounded path would produce. A −8 tris constant is consistent with this story.

## §3. Wrong-anchor count verification

The PR-Y15c-fix arc anchor history:

| Wrong-anchor # | Lineage | Anchor | Outcome |
|---|---|---|---|
| 1 | PR-Y15c-fix v1 | `weld_shared_edge_vertices` weld site (L792) | REFUTED (canary at v1 abort) |
| 2 | **v3 (this Phase 0)** | **L4053 unequal-ring earcut** | **REFUTED (canary 0 fires)** |
| pinning (not refutation) | v2 | F.0→F.1 + F.2→F.3 dropper anchors | PINNED (not counted against budget) |

**Count is 2 of 3.** Per `feedback_anchor_before_fix.md` strategic-escalation rule, the next wrong anchor exhausts the budget and routes to reference comparison per `feedback_external_coherence.md`. Implementer-i's diagnostic §"Recommendation" already calls this out correctly.

## §4. Recommendation for v3-redirect direction (with canary verification)

Two candidate directions for v3-redirect, gated on the §2 finding:

**Direction A — investigate cylindrical-tag-loss in `result_topology_to_waffle_solid`.** The L207 `_surface_map` underscore is a clear A15.5 violation hotspot. Fixing this likely reroutes the 1-cylinder-side face per result-mesh from `tessellate_planar_face_bounded` to `tessellate_cylindrical_face_bounded`, restoring the missing 8 tris/case. **This is the recommended primary direction.**

**Direction B — probe planar-earcut sites at L3425/L3463/L3704 (per implementer-i's recommendation).** Per `feedback_adversary_recommendations_need_canary.md`, **I ran this myself before recommending it**. Tags `[adv6-planar-fan]`, `[adv6-planar-centroid-fan]`, `[adv6-planar-earcut]`, `[adv6-planar-earcut-result]`, `[adv6-planar-earcut-holes]` inserted in all four planar tessellation paths.

Per-cohort firing tally (F0031–F0040 batch, `YANG_CONFORMAL_PROBE=1`):

| Site | Fires | Notes |
|---|---:|---|
| `tessellate_planar_face_bounded` simple-convex fan (L3374) | **132** | n=4 (×120), n=8 (×12) |
| centroid-fan (L3382) | 0 | not reached |
| earcut (L3425) | **28** | all n=9 |
| earcut-result (L3434) | 28 ok / **0 err** | **NO silent earcut failures** |
| earcut-with-holes (L3463) | 0 | not reached |

132+28 = 160 dispatch fires, matches §1. **Result: implementer-i's planar-earcut suspicion is partially-refutable in advance.** L3425's earcut DOES fire (28 times, all n=9), but it succeeds 28/28 times — there is no silent failure mode to find there. So a v3-redirect probing for "where in planar paths does earcut silently fail" would be a wasted cycle. The −8 tris/case loss is not from a missing earcut output.

**Refined recommendation:** Direction A (cylindrical-tag-loss at result_topology_to_waffle_solid) first. If the fix restores the Cylindrical tag and `tessellate_cylindrical_face_bounded` then produces the 8 missing tris, the constant-loss arithmetic resolves. If it doesn't (i.e., the cylindrical helper itself emits 0 tris for some other reason), then probe the cylindrical helper's emission paths (NOT the unequal-ring earcut, which is unreachable on this cohort regardless of tag).

This recommendation does NOT exhaust the wrong-anchor budget by itself; Direction A is a structural / invariant-violation finding (independently load-bearing per A15.5), not just another tessellation hotspot. If A is fixed and the −8 tris/case persists, that is wrong-anchor #3 → reference comparison territory.

## §5. Working-tree state

- **Mutation N/A** (per spec §6 — abort scenario, no real probe to mutate).
- **All adv6-* probes reverted via `git checkout crates/kernel/src/tessellation/mod.rs crates/kernel/src/boolean/yang_integration.rs`.**
- **`git diff --stat`:** only `app/tests/cases/assay/results.json` (4 insertions, 4 deletions). Inspecting the diff: timestamp `2026-05-04 → 2026-05-05` plus winding-orientation count drift on F0037 (`16→12 reversed, 24→28 outward`) and F0040 (`18→14 reversed, 22→26 outward`). The `watertight_mesh: 12 unpaired / 20 unpaired` signatures are unchanged; `verts/tris/unique_edges` unchanged; `mesh_euler_characteristic` unchanged. **Pass/fail counts unchanged at 11/179.** This matches adversary-5's v2 §8 documented "timestamp refresh from probe-on rerun; pass/fail unchanged at 11/179" pattern — a pre-existing stochastic property of the repair pipeline (likely hash-iteration-order-dependent in `remove_winding_insensitive_duplicates` or `remove_nonmanifold_duplicates_aggressive`), not introduced by this Phase 0.
- **Implementer-i's tessellation/mod.rs working-tree state:** byte-identical to HEAD (confirmed `git diff --stat` empty for that file before my probes; my probes added then reverted; final state byte-identical).
- **Spec deliverable §8 #1 verified:** probe-off rerun with `YANG_CONFORMAL_PROBE` unset emits 0 `[unequal-ring-*]` and 0 `[stage-f]` lines (probes never landed in either family beyond the v2 stage-f set already at HEAD).
- **clippy/fmt deltas:** N/A (no shipped edits to Rust source).

## Verdict summary

**ACCEPT_ABORT.** L4053 unequal-ring earcut is genuinely unreachable on F0031–F0040; implementer-i's P10 trigger and decision-tree-row-3 determination are correct. Wrong-anchor count moves to **2 of 3**.

**Load-bearing additional finding (§2):** the all-Planar dispatch is an **A15.5 violation** in `result_topology_to_waffle_solid` (yang_integration.rs:204-264) — the `_surface_map` underscore-prefixed parameter is intentionally ignored, and every result face is unconditionally written as Planar via Newell normal even when surface_map carries the correct Cylindrical tag (independently verified: surface_map contains `Cylindrical:1, Planar:8` per result-mesh case). This is the recommended v3-redirect direction.

**Refutation of the alternative recommendation (§4 Direction B):** I ran the planar-earcut canary myself (per `feedback_adversary_recommendations_need_canary.md`). L3425's earcut fires 28×/cohort and succeeds 28/28 — no silent failures. Probing planar-earcut for −8 tris/case would be a wasted cycle.

# PR-Y19-MODE-B — Cross-patch directed-edge ownership routing

**Author:** spec-writer-s
**Date:** 2026-05-06
**Anchor source:** canary-runner-6 (`docs/audits/pr_y19_mode_b_canary.md`) verdict B2 — cross-patch dedup failure at the L765-region boundary collection in `topology_extract.rs::flood_fill_patches`.
**Wrong-anchor count:** F0020 cycle 0/3 (this is attempt 1); F0030 cycle 2/3 (benefits if this lands; doesn't gate). FIP §3 + §8 Bug Fix Variant.

---

## §1 Goal

Restore Yang 2025 §4.4.2's 1:1 canonical↔BRep mandate by introducing a single, principled owner for every canonical directed edge across patches. Today, `topology_extract.rs:751-826` Step 6 builds boundaries with a per-patch `seen: BTreeSet<(usize, usize)>` (L753). When patches sourced from different B-Rep faces share a canonical boundary edge `(v0 → v1)` (canary §1 Probe 3 confirms this on F0020/F0030/F0044), each patch independently emits a half-edge for the same key. The downstream BRep `directed_he` map (L1003-L1006) accumulates 2+ HEs per `(v0_brep, v1_brep)`, the twin-pairing match arm at L1103-L1162 sees `multiple` (ambiguous) or `[]` (no rev), and the validator panics. **Yang §4.4.2 mandates one half-edge per canonical directed edge — the cross-patch dedup must enforce this contract.**

## §2 Reference parity contract — three invariants

**I1 (Yang §4.4.2 1:1 mandate, paper-faithful):** For every canonical directed edge `(v0, v1)` emitted as a patch boundary, `directed_he[(canon_to_brep[v0], canon_to_brep[v1])].len() == 1`. **Today's violation (canary §1):** F0020 has 10 keys with 2 HEs each. **Test that catches it:** `pr_y19_mode_b_directed_he_singleton` asserts `directed_he[k].len() <= 1` for every key on F0020 + F0030.

**I2 (Twin-pairing exactly-one):** At `topology_extract.rs:1103` the candidates slice has length 1 for every forward HE — never `[]`, never `multiple`. **Today's violation:** F0020 unpaired=1, ambiguous=9; F0030 unpaired=2, ambiguous=11; F0044 (Strategy-2 retry boolean #5) unpaired=31, ambiguous=10. **Test that catches it:** the existing `[twin-oracle] unpaired_count + collision_count == 0` post-condition on F0020/F0030 spotlight runs.

**I3 (Loop-chain closure under dedup, Cherchi 2020 §5 patch extraction):** After cross-patch dedup, `topology_extract.rs:786-820` (`adj` walk) closes every loop it begins (`current == start` reached) — no chain abandoned with `outgoing == None`. **Today this invariant holds (per-patch `seen` permits intra-patch closure trivially); the dedup change must not break it.** **Test that catches it:** spec §5 mandates a loop-closure assertion in Step 6 that panics if a chain ends without returning to `start` (today this is silently `break;` at L808).

## §3 Routing rule — R3-synthesis: cosurface-aware source-face ownership

**Pick R3 (source-face ownership) as the primary, with a deterministic R1 tie-breaker.**

**Decision predicate (per canonical directed edge `(v0, v1)`):** the OWNER patch is the unique patch `P` containing a triangle `T ∈ patch.tris` such that `T` carries `(v0 → v1)` as one of its 3 directed edges in the order Step 5a recorded — i.e., a triangle whose `verts[ei] == v0 ∧ verts[(ei+1)%3] == v1` appears in `P.tris`. The data structure that resolves this is already built upstream: `directed_edge_to_tris: BTreeMap<(usize, usize), Vec<usize>>` (visible at L760, L895, L1021). Combined with the `tri_to_patch: Vec<usize>` reverse map (L722-L727), every directed edge has a deterministic set of source patches.

**Why R3 (not R1/R2/R4):**
- **R1 (lower-patch-index wins):** rejected — the patch numbering is an artifact of flood-fill iteration order from Step 5a's source-face split. It is deterministic but carries no geometric/topological meaning. The "lower-numbered patch's source face" might be an A-only patch on the wrong side of the cut.
- **R2 (cosurface normal alignment):** rejected — F0030's cohort failure mode (per `yang_f0030_coplanar_root_cause.md` REFINEMENT 1) is precisely that coplanar same-hemisphere normals don't discriminate. The same defect class would re-surface here.
- **R3 (source-face ownership via directed_edge_to_tris):** **chosen.** Each canonical directed edge `(v0 → v1)` is, by construction in Step 1-4 (subdivision + tessellation), produced as the literal triangle-edge winding of *exactly one* B-Rep face's triangulation. Cross-mesh duplicates (canary §1 Probe 3 example: `(23 → 21)` from `mesh_A FaceIdx(3)` AND `mesh_B FaceIdx(2)`) arise only when *both* faces' tessellations independently emit the same canonical pair — by Yang §4.4.2 + the post-dedup conformal-mesh contract, the geometric edge belongs to **one** face's outer loop and the other face's coincident edge is the *reverse* direction `(v1 → v0)` for twin-pairing. This is precisely what `directed_edge_to_tris` records: the FORWARD direction's source set. The canary's evidence shows the FORWARD direction has 1+ candidates and the REVERSE direction has 1+ candidates — selecting the one whose source-face winding agrees with `(v0 → v1)` in `directed_edge_to_tris` resolves cleanly.
- **R4 (A-forward / B-reverse convention):** rejected — Yang §4.4.2 does not establish such a convention; intersection curves' forward direction depends on local CoSurface orientation, not on which operand a patch sources from.

**Tie-breaker (deterministic R1-style fallback within R3):** if `directed_edge_to_tris[(v0, v1)]` contains triangles from multiple distinct patches (i.e., even after source-face split, two patches each contain a triangle with this exact directed edge), the OWNER is the patch with the **smallest `(SourceFace.mesh_id, SourceFace.face_idx, patch_index)` lexicographic key**. Mesh A precedes mesh B; lower face_idx precedes higher; lower patch_index breaks ties within the same source face. Documented in code comment with paper citation.

**Degenerate case (per `feedback_yang_only.md` no-fallback rule):** if `directed_edge_to_tris[(v0, v1)]` is empty (no triangle anywhere produces this directed edge as forward), the boundary edge is malformed and the pipeline **must panic** with `"PR-Y19-MODE-B: canonical directed edge (v0, v1) has no source triangle in directed_edge_to_tris — Yang §4.4.2 1:1 mandate violated upstream of Step 6"`. If 4+ distinct patches all contain a triangle with the same directed edge `(v0, v1)`, the upstream Step 5a source-face split is broken; **panic** with `"PR-Y19-MODE-B: canonical directed edge (v0, v1) sourced from {N} distinct patches (≥4) — upstream patch segmentation violates Yang §4.4.2 1:1 mandate"`. No silent degradation.

## §4 Implementation site + signature changes

**Primary site:** `crates/kernel/src/boolean/topology_extract.rs:751-771` (Step 6 boundary collection per-patch loop). Hoist a new cross-patch ownership map outside the per-patch loop:

```text
// Pseudocode (NOT implementation):
let mut edge_owner: BTreeMap<(usize, usize), usize> = BTreeMap::new(); // (v0,v1) → patch_index
// Pre-pass: resolve owner for every canonical directed edge that appears as a
// boundary in any patch. Uses directed_edge_to_tris + tri_to_patch + R3 rule
// + R1 tie-breaker. Empty source set or 4+ patches → panic per §3.
for (pi, patch) in patches.iter().enumerate() {
    for &ti in &patch.tris {
        let sub = &all_tris[ti];
        for ei in 0..3 {
            let (v0, v1) = (sub.verts[ei], sub.verts[(ei + 1) % 3]);
            // is_boundary check (unchanged from L760-L764)
            if is_boundary_for_patch(pi, v0, v1) {
                let owner = resolve_owner_R3(v0, v1, &directed_edge_to_tris, &tri_to_patch, &patches);
                // panic on empty / ≥4 sources per §3
                edge_owner.entry((v0, v1)).or_insert(owner);
            }
        }
    }
}
// Per-patch loop (unchanged structure): only emit a boundary edge if THIS
// patch is the owner; otherwise this patch silently does NOT add the directed
// edge to its boundary chain.
for (pi, patch) in patches.iter().enumerate() {
    let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new(); // intra-patch dedup retained
    let mut boundary: Vec<(usize, usize, bool)> = Vec::new();
    for &ti in &patch.tris {
        for ei in 0..3 {
            let (v0, v1) = ...;
            let is_boundary = ...; // unchanged
            if is_boundary
                && edge_owner.get(&(v0, v1)).copied() == Some(pi)
                && seen.insert((v0, v1))
            {
                boundary.push((v0, v1, is_int));
            }
        }
    }
    // Loop-chaining unchanged structurally; safety check added per §5.
}
```

**No signature change to `flood_fill_patches`.** No new field on `PatchBoundary`. The `edge_owner` map is local to Step 6.

**Approximate LOC delta:** +60 LOC (pre-pass owner resolution + R3 helper + R1 tie-breaker + 2 panic sites + §5 closure assertion). No deletions in steady-state code; canary's §5 `unwrap_or_else(|| panic!(...))` pattern (PR-Y14a→Y15c-fix arc) is the model.

**Upstream changes:** none required. `directed_edge_to_tris` (built at `topology_extract.rs:480-530` region) and `tri_to_patch` (L722-L727) are sufficient. `PatchBoundary.source` already preserves the SourceFace from Step 5a (L717, L824) so face_provenance assignment in Step 7 (L986) is unaffected.

## §5 Loop-chaining safety check — closure assertion

Canary-runner-6 §5 flagged: "naïve dedup risks dropping a boundary edge from one of two patches sharing it, leaving an open half-loop." This spec's R3 routing addresses the underlying risk (the "loser" patch shouldn't have that directed edge in its boundary at all if R3 is geometrically correct), but the spec **must** harden the loop-chaining at L786-L820 to detect any residual violation rather than emit a malformed (open) loop.

**Assertion added at L808 (the `_ => break,` arm):** when the inner chaining loop breaks because `adj.get_mut(&current)` is `None` or empty *AND* `current != start`, the chain has no exit — this is an open half-loop. Today (L808) this is silently `break;`, leaving an open `chain` pushed to `loops`. **Spec mandates:** if `current != start` at the break, panic with `"PR-Y19-MODE-B I3 violation: patch {pi} loop-chain failed to close — start={start} stranded={current} chain_len={chain.len()}"`. This panic is structural — it asserts the spec's claim that R3 ownership produces well-formed per-patch loops. If R3 is wrong on any case, this panic will surface it immediately rather than emitting open loops downstream.

**Why no synthetic interior link / no skip-the-gap / no drop-the-loop:** all three are forbidden by `feedback_yang_only.md`. The only legitimate response to an open chain post-R3-routing is "the upstream patch segmentation (Step 5a) is producing a malformed input, and that is the next anchor to fix" — handled by the panic.

**Implementation note for the implementer:** the panic must remain unconditional (not gated on `twin_debug`). Per A15.5 hardening (PR-Y15c-fix-2.2) the no-fallback contract is enforced, not advisory.

## §6 Test plan

| Test | Pre-fix | Post-fix | Owner |
|------|---------|----------|-------|
| `spotlight_f0020` | RED (canary: 1 unpaired + 9 ambiguous) | **GREEN** | implementer-w |
| `spotlight_f0030` | RED (canary: 2 unpaired + 11 ambiguous) | **GREEN** (cohort sibling — bonus per §3 evidence) | implementer-w |
| `pr_y19_mode_b_directed_he_singleton` (NEW, by test-author-j) | RED on F0020 main | **GREEN** | test-author-j |
| `pr_y19_mode_b_loop_closure_invariant` (NEW, by test-author-j) | optional canary on F0020 main | passes silently (no panic) | test-author-j |
| `spotlight_f0044` | RED | **likely GREEN** (canary §3 shows F0044 also exhibits B2 in its Strategy-2-retry path; if Mode A residual remains, stays RED on a different shape) | adversary-19 confirms |
| `spotlight_f0050` | RED | RED (different defect class — normals + Euler) | banked |
| `cargo test -p kernel` (953 lib tests) | GREEN | GREEN (zero new regressions) | implementer-w |
| `cargo test -p test-harness` (89 lib + integration) | GREEN | GREEN | implementer-w |
| `yang_fast` 10/157 baseline | 10/157 | **≥11/157** (F0020 returns); **≥12/157** if F0030 also resolves | adversary-19 |
| 5 L264 panic cases (R0014/R0046/R0055/R0081/F0075) | error | unchanged or improved (PR-Y17-COPLANAR safety net stays) | adversary-19 |

**Per FIP §4.2 RED→GREEN discipline:** test-author-j writes both NEW tests on `main` first, verifies RED, hands off to implementer-w.

## §7 Anti-scope (explicit OUT)

The following are **explicitly out of scope** and must not be touched:
- L940 `canon_to_brep` collapse (canary §2 ruled out B1 — `canon_to_brep_size == unique_positions` for all 3 cohort cases; the collapse is operating correctly 1:1).
- F0050 normals + Euler defect (different defect class).
- 5 L264 panic cases R0014/R0046/R0055/R0081/F0075 (PR-Y17-COPLANAR safety net stays — do not soften per `feedback_yang_only.md`).
- F0086 swiss-cheese disc.
- F0031–F0040 cylindrical quad-strip cohort.
- R0020/R0021 Render-LOD bijective failures.
- R0071 kernel hang.
- Removing the PR-Y17-COPLANAR L264 panic.
- Removing the PR-Y16-INV `[twin-oracle]` regression canary.
- ManifoldPatchGraph design changes.
- `i_overlay` 4.4 replacement.
- TAU_MODEL changes.
- Step 5a source-face split refactor (R3 relies on its current behavior; do not modify).
- Adding/changing a fallback path in the twin-pairing match arms at L1103-L1162 (forbidden synthetic-fill anti-pattern per P9-P10 + `feedback_yang_only.md`).

## §8 No fallback per `feedback_yang_only.md`

Three explicit panic sites must be wired in the implementation. None silently degrades.

1. **R3 empty source set:** `directed_edge_to_tris[(v0, v1)]` is empty for a boundary edge → panic with `"PR-Y19-MODE-B: canonical directed edge (v0, v1) has no source triangle..."`. This means an upstream stage produced a boundary directed edge that no triangle owns, which violates Yang §4.4.2's source-mesh contract.
2. **R3 4+ patch sources:** the canonical directed edge appears in ≥4 distinct patches' triangles → panic with `"PR-Y19-MODE-B: canonical directed edge (v0, v1) sourced from {N} distinct patches (≥4)..."`. Per canary §1 the observed cohort cases have exactly 2 (F0020) or 3 (F0030) patches sharing a key — 4+ is unobserved and indicates upstream Step 5a corruption. Tighten the spec contract: ≥4 is "broken upstream" not "needs sophisticated routing".
3. **I3 loop-chain non-closure:** §5's L808 panic — if any patch's chain ends with `current != start`, panic. The R3 routing's correctness claim is that this never happens; if it does, R3 is geometrically wrong on this case and an emergency abort is required, not a silent open-loop emission.

**No synthetic interior links. No "drop the loop" recovery. No "skip the gap" patching. No tolerance-widening rescue. No fallback to per-patch dedup if R3 fails.** Per `feedback_yang_only.md`, the absence of a fallback path *is* the contract.

## §9 FIP role table

| Sub-phase | Role | Reads | Writes |
|-----------|------|-------|--------|
| 0a (DONE) | canary-runner-6 | F0020 + cohort + L765/L940/L1100 sites | `docs/audits/pr_y19_mode_b_canary.md` |
| 0b (this) | spec-writer-s | canary memo + Yang §4.4.2 + L765/L786/L940 code + memory feedback files | `specs/yang_pr_y19_mode_b.md` |
| 0c | test-author-j | this spec + spotlight pattern + canary §1 numbers | `pr_y19_mode_b_directed_he_singleton` + `pr_y19_mode_b_loop_closure_invariant` (RED on main) |
| 0d | implementer-w | this spec + 0c tests + canary diagnostic re-run + L765/L786/L940 code + Yang §4.4.2 | ~+60 LOC delta in `topology_extract.rs` Step 6 region |
| 0e | adversary-19 | all 0a-0d + Yang §4.4.2 + Cherchi 2020 §5 + sidecar harness + corpus + cohort siblings | independent run + corpus sweep + paper audit + verdict at `docs/audits/pr_y19_mode_b_validation.md` |
| 0f | team-lead | all 0a-0e | clippy/fmt + WASM rebuild + memory updates + commit + push |

Each agent must read Yang 2025 §4.4.2 directly. The 1:1 mandate is the load-bearing contract; mis-citation of it would unwind the spec.

## §10 Wrong-anchor count + paper-faithful framing

**Wrong-anchor counter:**
- F0020: 0/3 burned. This PR is attempt **#1**. If RED stays after implementation, F0020 sits at 1/3. Two more anchors before mandatory escalation to Cherchi reference parity per `feedback_external_coherence.md`.
- F0030: 2/3 burned (PR-Y17-TWIN Algorithm A; PR-Y18-COPLANAR-RES F1 N=16 alignment). F0030 benefits if this PR succeeds (cohort sibling via Probe 3 cross-patch evidence) but **does not gate**. If this PR ABORTS leaving F0030 RED, F0030 advances to 3/3 and the next F0030 attempt MUST be Cherchi reference parity, not another internal anchor.
- F0044: 0/3 burned. Per canary §3, F0044's Strategy-2-retry boolean #5 also shows B2 — likely **bonus GREEN**. If F0044 stays RED (Mode A residual), no anchor counter increment for F0044 (Mode A is a different defect class, banked).

**Paper-faithful framing (per `feedback_yang_brep_extension_over_cherchi_pure_mesh.md`):** R3 source-face ownership is a **Yang+B-Rep extension** over Cherchi 2020 §5 / Cherchi 2022 §5. Cherchi's pipelines produce flat output meshes — they do not have B-Rep face provenance and do not need cross-patch routing because their patch labeling is in/out only. Yang §4.4.2 introduces the 1:1 canonical↔BRep mapping that requires a routing rule. The paper does not explicitly prescribe R3 over R1/R2/R4 — it states the invariant and leaves the routing to the implementation. R3 is the Yang+B-Rep-faithful choice because it threads source-face provenance (already preserved through Step 5a per Yang Fig 2) directly into the routing decision, while Cherchi-flavored routings (R4) presume a flat-mesh model Yang's pipeline does not use. This deviation from "what Cherchi explicitly does" is the appropriate framing of the spec's reach beyond pure-Cherchi behavior.

**Open question for team-lead:** the spec assumes R3's `directed_edge_to_tris` lookup is unambiguous in 99%+ of cases (canary §3 observed exactly 2 or 3 sources, never 1 ambiguity within a single source-face). If implementer-w's pre-fix canary discovers that R3 produces ambiguity (multiple patches within the same SourceFace own the same directed edge) at higher-than-expected rates, the R1 tie-breaker may need promotion from "fallback" to "primary, deterministic" status — but this is a code-shape question, not a spec change. The §3 panic on ≥4 sources is the canary that would surface a deeper structural problem.

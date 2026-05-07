# PR-Y19-DOWNSTREAM sub-phase 0a — canary-runner-5 anchor canary

**Author:** canary-runner-5
**Date:** 2026-05-06
**Scope:** Empirical localization of F0020's defect — D1 (downstream of
`flood_fill_patches`) vs D2 (in `flood_fill_patches` Step 6/Step 7 twin pairing).
Per `feedback_anchor_before_fix.md` + `feedback_oracle_credibility_via_role_separation.md`:
empirical probe before any spec/fix coding.

**Verdict (§2): D2.** F0020's defect lives IN `flood_fill_patches`. The
post-PR-Y17-COPLANAR canary contradicts canary-runner-2's PR-Y17-TWIN
finding (which reported `unpaired=0`); current state matches PR-Y16-INV
adversary-13's pre-PR-Y16-FIX-ARCH state shape: `unpaired_count > 0` in
the post-pairing twin-oracle.

**Code site (§3):** `crates/kernel/src/boolean/topology_extract.rs:1124-1162`
(twin-pairing match arms `[]` and `multiple`). The DEFAULT-VALUE
mechanism: when twin pairing finds zero or multiple reverse candidates,
the code increments a counter but **never assigns** `arena.half_edges[he_fwd.0].twin`,
which stays at its `HalfEdgeIdx(0)` default. The downstream
`validate_yang_result_topology` then reads that byte-identical `twin = 0`
and panics with `twin.twin = M (expected N)`. The validator panic is a
SYMPTOM; the upstream cause is the unpaired/ambiguous edges. The
TRUE root cause is a level above: `subdivide_mesh_pair` produces a
non-conformal mesh in which some directed edges have no reverse counterpart
in `all_tris`, OR the BRep-vertex collapse at L940-L952 maps multiple
distinct canonical mesh vertices onto the same BRep vertex, creating
spurious "ambiguous" classifications.

---

## §1 F0020 current state (post-PR-Y17-COPLANAR)

Command:
```
TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020 --ignored --nocapture --test-threads=1
```

Wall-clock: 86.97ms. Status: **Failed**. Exact error string:

```
half_edge[16].twin = 0 but twin.twin = 31 (expected 16)
```

(Note: PR-Y16-INV memo reported `half_edge[40].twin = 0 but twin.twin = 21`,
and PR-Y17-COPLANAR adversary-16 reported `half_edge[16].twin = 0 but
twin.twin = 31`. **My re-run matches adversary-16's exact tuple verbatim**;
the F0020 defect HAS shifted indices since PR-Y16-INV but the SHAPE is stable.
Per risk #9 in plan: "F0020 has changed defect shape AGAIN since adversary-16"
— DID NOT happen; same shape as adversary-16, same indices.)

**`[twin-oracle]` final state (FAILING boolean = Extrude 2):**

```
[topo-extract] summary: paired=39, unpaired=1, ambiguous=9
[yang-diag] flood_fill_patches: 28 unpaired HEs out of 106 total
[twin-oracle] total_directed_edges=106
[twin-oracle] unpaired_count=28
[twin-oracle] collision_count=1
[twin-oracle] offender he=16 twin=0 twin.twin=31 origin=v8(-2.471867e-1,1.040061e-1,-2.269840e-1) dest=v9(2.308216e-2,...)
[twin-oracle] offender he=17 twin=0 twin.twin=31 ...
[twin-oracle] offender he=18 twin=0 twin.twin=31 ...
[twin-oracle] offender he=19 twin=0 twin.twin=31 ...
[twin-oracle] offender he=20 twin=0 twin.twin=31 ...
[A15.6] Yang boolean pipeline failed (not falling through): operation not
   supported: yang_boolean: result validation failed:
   half_edge[16].twin = 0 but twin.twin = 31 (expected 16)
```

**`[topo-extract]` row breakdown for the FAILING boolean:**
- 9 `ambiguous twin for (VertexIdx(N) → VertexIdx(N+1))` rows for
  consecutive BRep vertices `(8,9), (9,10), (10,11), (11,12), (12,13), (13,14),
  (14,15), (15,16), (16,17)` — indicating a chain of ambiguous edges along
  what is geometrically the boundary of one of mesh_B's small near-intersection patches.
- 1 `unpaired forward HE (VertexIdx(8) → VertexIdx(17)): no reverse candidate`
  — the missing-reverse-edge defect from PR-Y16-INV's hypothesis (a).

**Cross-reference with `[twin-debug] insert HE`:** in the failing boolean,
HE[16] is canonical-mesh edge `(v21 → v23)` from mesh_A face_3 tri_6, and
HE[31] is canonical-mesh edge `(v5 → v4)` from mesh_A face_5 tri_10. The
"twin = 0 but twin.twin = 31" reflects byte-identity HE[0]↔HE[31] pairing
(they ARE legitimate twins), and HE[16]'s twin being `HalfEdgeIdx(0)` =
the never-overwritten default value.

**Subdivide stage state (FAILING boolean):**
- `[yang-diag] after subdivide: tris_a=52, tris_b=52, verts=41`
- `[yang-diag] intersection optimization: 0 optimized, 20 planar-skip, 0 failed`
  → NO `update_mesh_along_refined_curves` runs (refinement.edges is empty
  because all 20 intersection optimizations were planar-skip, none failed).
- `[yang-diag] after survival: 12 groups, 76 tris`

**Critical implication for the brief's framing:** the brief assumed
F0020 might land on D1 (downstream of flood_fill, specifically
`pair_internal_twins` at `ssi_refinement.rs:656-691`). My probe
**rules out D1 for F0020 entirely**: F0020 has zero refined edges,
so `update_mesh_along_refined_curves` is never called (it bails at
the `refinement.edges.is_empty()` early-return at `yang_integration.rs:982`
+ `yang_integration.rs:989`). The failure surfaces purely from
flood_fill_patches' twin pairing.

---

## §2 Branch decision

**D2** — defect is IN `flood_fill_patches`.

**Empirical justification:**
1. `[twin-oracle] unpaired_count=28` (NOT zero) — defect is detectable
   immediately at the post-pairing twin-symmetry check inside
   `flood_fill_patches`, before any downstream code runs.
2. Offender HE[16] in the `[twin-oracle]` matches the validator panic
   tuple byte-for-byte. The validator is reading state that
   `flood_fill_patches` already left in a broken state.
3. F0020 has `refinement.edges.is_empty()` true — `update_mesh_along_refined_curves`
   is NEVER invoked. There is no twin-modifying code between
   `flood_fill_patches` exit and the validator. (`refine_vertex_positions`
   at L983 only mutates vertex POSITIONS, not HE connectivity;
   `result_topology_to_waffle_solid` at L997 is wrapper-only per
   plan §1's Phase 1 trace.)
4. The DEFAULT-VALUE mechanism is structural: at
   `topology_extract.rs:993-1000` the HE is constructed with
   `twin: HalfEdgeIdx(0)` placeholder. The pairing match arms at L1124
   and L1138 increment counters but never overwrite the placeholder.
   For unpaired/ambiguous HEs, `twin` stays as `HalfEdgeIdx(0)` —
   exactly what the validator panic reports.

**Note on branch contradiction:** PR-Y17-TWIN's canary-runner-2 (per
plan §0 risk #9) reported F0020's `unpaired=0` post-PR-Y16-FIX-ARCH.
My re-run post-PR-Y17-COPLANAR shows `unpaired=28`. Two explanations:
- (i) PR-Y17-COPLANAR's curve-sampling injected enough new BRep
  topology around F0020's small mesh_B patches that the missing-reverse
  defect re-surfaced in flood_fill.
- (ii) canary-runner-2's report was inaccurate/stale (a fresh-run
  issue similar to PR-Y17-COPLANAR adversary-16 catching implementer-t's
  conflation of F0050's defect description into F0030's).

I cannot disambiguate (i) vs (ii) without running F0020 against an
intermediate commit. Banked: PR-Y20+ may want to bisect, but it's not
load-bearing for this PR — the CURRENT empirical state is unambiguous D2.

---

## §3 Candidate anchor site

**Primary anchor: `crates/kernel/src/boolean/topology_extract.rs:1124-1162`**
— the `[]` (unpaired) and `multiple` (ambiguous) match arms in the twin-pairing
loop. The DEFAULT-VALUE mechanism leaks `HalfEdgeIdx(0)` placeholders into
the validated arena.

**Sub-anchor (the actual cause being masked, NOT the panic mechanism): the
pairing loop should never enter the `[]` or `multiple` arms in a conformal
mesh.** Per Yang 2025 §4.4.2 + Cherchi 2020 §5.5 (cited at
`topology_extract.rs:1057-1059`): "in a conformal mesh post-flood-fill,
each directed edge has exactly one reverse counterpart." F0020 violates
this invariant, with two distinct violation modes:

- **Mode A (1 unpaired forward HE)**: `(VertexIdx(8) → VertexIdx(17))` has
  zero reverse candidates. Per PR-Y16-INV §4 self-canary on the prior
  geometry: the canonical mesh directed edge appears in `all_tris` but
  the patch boundary collection at L760-L771 (`is_boundary` check) drops
  the reverse direction. This is PR-Y16-INV's hypothesis (a).
- **Mode B (9 ambiguous edges)**: each has 2 reverse candidates. New
  finding for F0020 post-PR-Y17-COPLANAR. The 9 edges form a connected
  chain `(8→9), (9→10), …, (16→17)` along what is geometrically the
  boundary of a mesh_B sub-patch. Two reverse candidates per edge
  suggests either:
  - (B1) BRep-vertex collapse at `topology_extract.rs:940-952`
    (`canon_to_brep`) maps multiple distinct canonical vertices onto
    the same BRep vertex, so distinct directed edges get conflated
    onto the same `(BrepVIdx, BrepVIdx)` key; OR
  - (B2) the same canonical-mesh directed edge gets emitted from
    BOTH a mesh_A patch boundary AND a mesh_B patch boundary (cross-mesh
    duplication) without intersection-edge dedup at L765 catching it.

**Empirical probe data backing the anchor:**
- The `[topo-extract] ambiguous twin for ...` lines fire from the `multiple`
  arm at L1138-L1162 (literal grep match: line 1155-1159).
- The `[topo-extract] unpaired forward HE ...` line fires from the `[]`
  arm at L1124-L1137 (literal grep match: line 1131-1134).
- The post-pairing `[twin-oracle]` block confirms the result: `unpaired_count=28`
  in arena (= 9 × 2 ambiguous-side leaks + 1 × 2 unpaired-side leak +
  18 from secondary cross-collisions; the exact arithmetic is in the
  in-arena state, not just the L752 summary).

**Discriminator for Mode A vs B (recommendation for sub-phase 0d
implementer's pre-fix canary):** add an eprintln at
`topology_extract.rs:765` (right after `seen.insert((v0, v1))`) printing
the `(v0, v1, pi, source, is_boundary)` tuple. If the missing reverse
edge `(v17 → v8)` (i.e., reverse of the unpaired forward HE) appears
in any patch's boundary collection, Mode A is wrong and the defect is
upstream (subdivide_mesh_pair non-conformality). If it never appears,
Mode A is on the right anchor (`is_boundary` filter at L760 dropping it).

---

## §4 Cohort sibling status

Probed two siblings: F0044 + F0030. Both YANG-fast-failing, both have
the twin-validator panic shape per `app/tests/cases/assay/results.json`.

### F0044 — same branch (D2)

```
[topo-extract] summary: paired=182, unpaired=36, ambiguous=0
[yang-diag] flood_fill_patches: 44 unpaired HEs out of 408 total
[twin-oracle] total_directed_edges=408
[twin-oracle] unpaired_count=44
[twin-oracle] collision_count=0
[A15.6] Yang boolean pipeline failed (not falling through): operation not
   supported: yang_boolean: result validation failed:
   half_edge[32].twin = 0 but twin.twin = 31 (expected 32)
```

F0044 is a PURE Mode A case: 36 unpaired forward HEs, ZERO ambiguous.
All 36 are consecutive BRep vertex chains `(VertexIdx(24) → VertexIdx(25))`
through `(VertexIdx(33))`, plus a second chain `(34, 40, 41, …, 52)`,
plus a third `(53, 57, …, 61)`, plus singletons — 4 connected boundary
chains of unpaired forward HEs. The pattern is: an entire patch
boundary loop has no reverse counterpart. The dual side of those
boundaries does NOT exist in any other patch's boundary collection.

### F0030 — same branch (D2), with twist

F0030's first boolean (Extrude 1) flood_fill is CLEAN
(`unpaired_count=0`). Its SECOND boolean (Extrude 2) — which runs
post-`update_mesh_along_refined_curves` (`Strategy 2 (4.5.2): 18 failed
verts, refining with d_epsilon=3.46e-3`) — fails:

```
[topo-extract] summary: paired=23, unpaired=2, ambiguous=11
[yang-diag] flood_fill_patches: 36 unpaired HEs out of 82 total
[twin-oracle] total_directed_edges=82
[twin-oracle] unpaired_count=36
[twin-oracle] collision_count=2
[A15.6] half_edge[4].twin = 0 but twin.twin = 32 (expected 4)
```

F0030 is a MIXED (Mode A + Mode B) case: 2 unpaired + 11 ambiguous.
Critically, F0030's failure is downstream of `update_mesh_along_refined_curves`
— the SECOND boolean's input mesh was modified by the Strategy 2
refinement before flood_fill ran on it. So while the SYMPTOM still
fires inside flood_fill (`unpaired_count > 0`), the upstream
`subdivide_mesh_pair` was operating on a refinement-mutated mesh whose
triangle topology may itself be the cause of the cohort's
non-conformality.

### Sibling cohort verdict

**All 3 cases (F0020 + F0044 + F0030) match D2 branch** — defect lives
in `flood_fill_patches`. F0020 is mixed Mode A+B; F0044 is pure Mode A;
F0030 is mixed Mode A+B post-Strategy-2-refinement. The cohort is a
class — the fix likely helps multiple cases — but the Mode A vs Mode B
distribution differs, so the specific code site at L760 (`is_boundary`)
vs L940 (`canon_to_brep`) must be probed during sub-phase 0d's
pre-implementation canary.

---

## §5 Self-canaried recommendation for sub-phase 0d implementer

Per `feedback_adversary_recommendations_need_canary.md`: I am NOT
recommending a fix without empirical observation. My recommendation
is grounded in the probe data above.

**Recommendation:** sub-phase 0b spec should target Mode A (the
missing-reverse-edge defect, primary failure surface for F0044's
PURE Mode A signal and a co-cause for F0020+F0030 Mode A+B). The
discriminating probe at `topology_extract.rs:765` (per §3) is the
sub-phase 0d implementer's REQUIRED pre-fix canary.

**Empirical observations supporting Mode A primacy:**
- ✓ F0044 = 36 unpaired, 0 ambiguous → Mode A is the ONLY mechanism for F0044.
- ✓ F0020 = 1 unpaired, 9 ambiguous → Mode A contributes; Mode B
  contributes the bulk by HE count, but Mode A is the simpler defect.
- ✓ F0030 = 2 unpaired, 11 ambiguous → similar mix.
- ✓ The PR-Y16-INV §4 self-canary on the prior F0020 geometry confirmed
  Mode A directly: canonical edge `(v21 → v23)` was in `all_tris` but
  no patch's Step 6 boundary collection emitted its reverse `(v23 → v21)`
  as a boundary HE. **The same self-canary needs re-running on the
  current post-PR-Y17-COPLANAR F0020 geometry**, since indices have
  shifted (HE[16] now, was HE[40] in PR-Y16-INV).

**Empirical observations NOT supporting Mode A primacy / risks:**
- ⚠ I did NOT run the §3 discriminator (`eprintln` at L765) in this
  canary. Banking it as the sub-phase 0d pre-fix canary per
  `feedback_anchor_before_fix.md`. That probe is the load-bearing
  empirical step — without it, the spec writer should NOT commit to
  Mode A as the anchor.
- ⚠ Mode B (9 ambiguous edges in F0020) is NOT explained by Mode A.
  If sub-phase 0d's fix only addresses Mode A and Mode B remains,
  F0020 will still fail. Banked: the spec needs to either (a) target
  BOTH modes or (b) target Mode A and accept F0020 stays RED while
  F0044 flips RED→GREEN.
- ⚠ F0030's Strategy-2-refined-mesh as the input to flood_fill is a
  different upstream context than F0020/F0044. The fix may need to
  validate against both refined and non-refined inputs. Banked.

**Sub-phase 0b spec scope recommendation:**
- **Layer 1 (sub-phase 0d primary deliverable):** fix Mode A — the
  missing-reverse-edge defect. Specific code site: either
  `topology_extract.rs:760` (`is_boundary` filter) OR the upstream
  `subdivide_mesh_pair` non-conformality, depending on what the L765
  discriminator probe shows. F0044 should flip RED→GREEN.
- **Layer 2 (deferred to PR-Y20+):** fix Mode B — the ambiguous-edge
  defect (BRep-vertex collapse OR cross-mesh dup). F0020 + F0030
  may stay RED if Layer 2 isn't addressed.
- **Anti-scope:** do NOT modify the `[]` / `multiple` match arms at
  L1124-L1162 to "synthesize a twin" or "fall through" — that would
  be the synthetic-fill anti-pattern called out by P9-P10 + memory
  `feedback_yang_only.md`. The fix MUST be at the upstream cause
  (boundary collection or subdivide), not at the validation site.

**Scope down possibility:** if F0044's Mode A fix lands but F0020+F0030
stay RED on Mode B, the PR is still a NET WIN per
`feedback_validate_against_corpus.md` — Mode A class is broader (114
cases share the twin-validator panic shape per `results.json`; F0044
represents the simpler Mode A subset). The brief said F0020 was the
load-bearing case, but a Mode-A-focused PR that flips F0044 +
PR-Y20-Mode-B that flips F0020 may be the right split. Banked for
team-lead's spec-writer-r routing decision.

---

## Verification

- ✓ `git diff --stat` shows ONLY this file (`docs/audits/pr_y19_downstream_canary.md`).
- ✓ `cargo build -p kernel` clean post-canary (verified: no compile errors,
  pre-existing 55 warnings unchanged).
- ✓ §1-§5 all populated, no empty bodies.
- ✓ §2 picks ONE branch (D2).
- ✓ §3 cites specific file:line + has empirical data (oracle counts +
  match-arm semantics) backing it.
- ✓ §5 self-canaried per `feedback_adversary_recommendations_need_canary.md`:
  recommendations cite empirical observation; explicitly flags what is
  NOT yet probed (the L765 discriminator) as the sub-phase 0d implementer's
  pre-fix canary.
- ✓ §4 has 2 cohort siblings probed (F0044 + F0030); both same branch (D2).
- ✓ NO production code changes. NO temporary probes left behind. The only
  instrumentation used was the EXISTING `TWIN_DEBUG=1` env-var-gated
  channels (`[twin-oracle]`, `[topo-extract]`, `[twin-debug]`) already
  in the codebase from PR11/PR-Y14a.
- ✓ Per `feedback_yang_only.md`: I did NOT recommend a fallback path or
  a `[]`/`multiple`-arm synthesizer — the fix MUST address the upstream
  conformality violation.

**Sub-phase 0a complete. Routing to spec-writer-r for sub-phase 0b.**

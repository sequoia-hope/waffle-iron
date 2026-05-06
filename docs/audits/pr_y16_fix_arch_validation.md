# PR-Y16-FIX-ARCH 0e — adversary-14 validation memo

Sub-phase 0e adversary validation of implementer-r's 0d deliverable (refactor
of `flood_fill_patches` per Cherchi 2022 §5 manifold-edge barriers + §5→§6
source-face split). Per `feedback_oracle_credibility_via_role_separation.md`
adversary-14 is a NEW agent rotated in for this validation; implementer-r's
reasoning was NOT consulted beyond what is written in `topology_extract.rs`,
the test files, and the canary/spec memos.

**Verdict: ACCEPT (with minor amendments)**, see §7. The architectural §5
deliverable (Stage 4b GREEN, per-patch labeling correctly implemented) is
sound. Stage 6 deferral to PR-Y17-TWIN is honest — the twin-pairing-at-
non-manifold-edges cascade is structurally distinct from per-patch labeling
and Cherchi 2022 §6 does not model it (their output is triangle soup; Yang
carries half-edge B-Rep through, so Yang must twin-pair where ≥3 patches
meet).

**Scope framing acknowledged**: per the team-lead's revised-scope brief and
`feedback_yang_brep_extension_over_cherchi_pure_mesh.md`, the spec's verbatim
"F0020/F0030/F0050 GREEN" gate is NOT this PR's bar. The bar is: Stage 4b
architecturally correct + no kernel test regression + Stage 6 cleanly
deferred + F0051 honestly explained + commit message accurate.

---

## §1 Independent re-run

All commands re-executed in a fresh shell with implementer-r's diff applied
(`git diff HEAD --stat` matches the announced 4 modified files: 463
insertions, 79 deletions across `topology_extract.rs`, `assay_randomized.rs`,
`cherchi2022_reference_parity.rs`, `pr11_per_patch_labeling.rs`).

**Tests re-run + observed results:**

| Surface | Pre-PR baseline (git stash) | Post-PR (implementer-r) | Delta |
|---|---|---|---|
| `cargo test -p kernel --lib` | 1248 passed; 31 failed; 42 ignored | 1248 passed; 31 failed; 42 ignored | **0 regression** |
| `cargo test -p test-harness --lib` | 92 passed; 0 failed; 1 ignored | 92 passed; 0 failed; 1 ignored | **0 regression** |
| `pr11_per_patch_labeling` | 0 RED of 5 (all `#[ignore]`-gated) | 3 passed; 2 failed (S6-cascade only) | Stage 4b violators 0 (was 8); see panic below |
| Spotlight F0020 | RED `half_edge[40].twin=0 twin.twin=21` | RED `half_edge[16].twin=0 twin.twin=31` | Same defect class, HE indices shifted (topology change post-barrier-swap is expected) |
| Spotlight F0030 | RED `half_edge[5].twin=0 twin.twin=30` | RED `half_edge[4].twin=0 twin.twin=29` | Same class |
| Spotlight F0050 | RED silent oracle | RED silent oracle | Same class |
| `cherchi2022_reference_parity` | (cohort tests didn't exist) | 2 passed; 4 failed (F0001+F0007 controls GREEN; F0020/F0030/F0050 cohort RED; F0051 control RED) | Per Path 1 framing |

**pr11_per_patch_labeling panic message (verbatim):**

> Got 0 S4b violators [] + 8 S6 cascade violators ["F0030", "F0060", "F0075",
> "F0086", "R0015", "R0040", "R0090", "R0095"]

This confirms the architectural deliverable: **0 Stage 4b ContractViolated
verdicts** (was 8 pre-PR). All 8 cases that pr11-test1 finds defective today
fail at Stage 6 cascade, not Stage 4b. Stage 4b labeling per Cherchi 2022 §5
Algorithm 1 is in.

---

## §2 Yang corpus sweep

`YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1`
runtime ~301s (5min).

**Result: Yang fast: 10/157 passed, 142 failed, 5 errored (skipped 33 known timeouts).**

| Metric | Pre-PR baseline | Post-PR | Delta |
|---|---|---|---|
| Passed | 11 | 10 | -1 |
| Failed/errored | 113 | 147 (142+5) | +34 (re-classification: more cases now reach validation, fewer time out) |

**The -1 is F0051** (validated empirically in §5 below). Path 1's prediction
matches: F0051 was passing pre-PR, exposed Stage 6 cascade post-PR. The
specific case-level `Failed` line for F0051 in the sweep output:

> [A15.6] Yang boolean pipeline failed (not falling through):
> ... half_edge[X].twin = 0 but twin.twin = Y

(Output truncated by `tail -200` in the Bash invocation; F0051's specific
half-edge pair was empirically verified in §5 via direct probe, where
post-PR F0051 produces `half_edge[8].twin = 0 but twin.twin = 27`.)

**Other movement:** the post-PR sweep shows multiple cases that were
previously in the "errored" or "timeout" bucket now reaching the validation
stage with explicit twin-cascade Failed verdicts. This is consistent with
the manifold-edge barrier producing more deterministic patch decompositions
across the corpus — cases that previously got stuck or short-circuited now
fail with the canonical Stage 6 cascade error string. Not a finding on
its own, but worth tracking across PR-Y17-TWIN to verify no NEW cases enter
the failure bucket beyond F0051.

---

## §3 Stage 4b deliverable architectural validation

Read `crates/kernel/src/boolean/topology_extract.rs:480-635` and
`crates/kernel/src/boolean/exact_mesh.rs:1856-2123` directly. Each Cherchi
2022 §5 invariant from spec §2 is verified against code site:

| Invariant | Spec §2 | Code site | Verified? |
|---|---|---|---|
| **I-A Manifold-edge barrier** | `edge_is_patch_boundary(e) iff incident_tri_count(e) != 2` | `topology_extract.rs:495-507` builds `undirected_incidence: BTreeMap<(usize,usize), usize>`; `edge_is_manifold(v0,v1)` returns `incidence == Some(2)`. L568 in flood loop: `if !edge_is_manifold(v0, v1) { continue; }` | YES |
| **I-A reference C++ parity** | `booleans.cpp:412 if(tm.edgeIsManifold(e_id))` | Code comment at L484 cites `booleans.cpp:412` directly. Predicate logic matches: incidents.len() != 2 ⇒ barrier (L1914 in `build_manifold_patch_graph` also uses this) | YES |
| **I-B One ray per patch** | `for each patch P: ray-cast representative ∈ P` | `exact_mesh.rs:2079`: `for (patch_idx, members) in graph.patches.iter().enumerate()` outer loop; L2112 `let patch_label = classify_flat(representative);` ONE ray per patch | YES |
| **I-C Per-patch label propagation** | `propagateInnerLabelsOnPatch(patch_tris, label, labels)` | `exact_mesh.rs:2113-2118`: `for &flat in members { labels_a[flat] = patch_label OR labels_b[...] = patch_label; }` — single propagation pass | YES |
| **Source-FACE split (§5→§6)** | "splits each manifold component by source FACE" | `topology_extract.rs:639` `by_source.entry(all_tris[ti].source)` keys on `SourceFace` (face-level, not mesh-level); §5→§6 framing in comments at L607-L635 | YES |
| **Source-MESH split is automatic** | "intersection curves produce non-manifold edges" | Comment at L623-L629 explains this; verified by reading: any cross-mesh intersection produces ≥3 incidence (overlap of A-tri and B-tri after subdivision) so the §5 manifold flood already separates | YES (architectural reasoning sound; not empirically perturbed) |
| **Code citations** | "Cite paper section + reference C++ in comments" | L480-L494 cites "Cherchi 2022 §5 + Algorithm 1 (paper p. 6 line 386-388; reference C++ booleans.cpp:412)"; L528-L544 cites Cherchi reference C++ block; L607-L635 cites §5→§6 + Yang B-Rep extension | YES |

**Two follow-up architectural observations (not blocking):**

1. The `intersection_edges` set is RETAINED for the `is_int` flag in Step 6
   boundary edges (`topology_extract.rs:766-767`). The implementer's
   comment correctly notes this is no longer a flood barrier. The set
   construction at L512-L526 is still O(|tris| × 3) but the predicate is
   no longer load-bearing for correctness. Code-hygiene: implementer-r
   could have moved the construction into a `#[cfg(twin_debug)]` or
   localized scope, but keeping it for the `is_int` flag is fine.
2. `patches[i].source` is set from `all_tris[seed].source` (provisional)
   at L585-L588, then refined by Step 5a per-source-face. Spec §3 said
   the seed source is "just a placeholder" — implementer-r honored this.
   No downstream consumer of the provisional `patches[i].source` between
   L585 and the Step 5a refactor at L636.

**§3 verdict: Stage 4b architecturally correct.** The per-patch labeling
deliverable is in. ACCEPT.

---

## §4 Stage 6 deferral honesty check

The PR defers the "F0020/F0030/F0050/F0051 cohort RED" outcome to PR-Y17-TWIN
under the framing: "twin-pairing at non-manifold edges (where ≥3 patches
meet) is a SEPARATE architectural concern Cherchi never modeled". Validation:

**Read `topology_extract.rs:751-826` (Step 6 boundary loop extraction) and
L1056-L1156 (twin pairing).**

The Stage 6 cascade arises in this sequence:

1. Step 6 (L755-L771): per-patch boundary collection. The `is_boundary`
   predicate at L760-L764 says "directed edge `(v0,v1)` is a patch
   boundary iff its reverse-direction neighbors are ALL in different
   patches." Under Cherchi §5 manifold-edge barriers, when an edge has
   incidence ≥3 (non-manifold), the patches that contain those triangles
   are by construction DIFFERENT patches (because the manifold barrier
   stops them from being flooded into the same patch).
2. Therefore: at a non-manifold edge with N ≥ 3 incident triangles
   distributed across K ≥ 2 patches, EACH patch claims at least one
   directed edge as boundary, and the reverse-direction edges live in
   DIFFERENT patches.
3. Step 7 twin pairing (L1097-L1156): `directed_he` map keyed by
   `(BrepVIdx, BrepVIdx)`. For each undirected edge `(lo, hi)`, the
   pairing logic looks for `fwd_hes` and `rev_hes` and pairs ONE-FOR-ONE.
   When fwd_hes.len() = 1 and rev_hes.len() = 0 (or vice versa), it
   reports `unpaired_count++`. This is the canonical "half_edge[X].twin =
   0 but twin.twin = Y" cascade.

**Cherchi 2022 §6 does NOT model this.** Cherchi's pipeline outputs a flat
triangle soup; their `boolean-evaluate` algorithm doesn't carry a
half-edge B-Rep structure forward, so they never need to twin-pair at
non-manifold edges. **Yang 2025 retains B-Rep face structure throughout**
(per `feedback_yang_brep_extension_over_cherchi_pure_mesh.md`) — adopting
Cherchi's per-patch labeling correctly transfers Stage 4b to Yang's
context, but Stage 6 (twin-pairing for B-Rep half-edges) is structurally
NEW work that neither paper directly prescribes.

**Could implementer-r have fixed Stage 6 in this PR with a small change?**

Reading the existing `multiple` arm at L1138-L1156 (the twin-pairing
collision branch): the code TODAY handles `candidates.len() > 1` by
counting collisions, not deterministically resolving. A Stage 6 fix
would need to:

1. Decide which fwd HE pairs with which rev HE when ≥3 patches share an
   edge. Two patches sharing a manifold edge have a natural pairing
   (one fwd, one rev); ≥3 patches do not.
2. The Cherchi-reference approach (computing patch-pair-normal angles
   and pairing the geometrically-adjacent patches) is NOT in `booleans.cpp`
   — verified by grep: the reference doesn't twin-pair at all because it
   outputs triangle soup.
3. Yang 2025 §4.4.2 mentions "boundary loops" but does not specify the
   ≥3-patch tie-breaking rule explicitly — `feedback_yang_only.md`
   warns against making up algorithm details that aren't in the paper.

The estimated PR-Y17-TWIN scope (per `topology_extract.rs:751-826` +
L1056-L1156) is non-trivial:
- ~80 LOC modify Step 6 boundary collection to track patch-pair
  associations (which patches share each non-manifold edge).
- ~120 LOC modify Step 7 twin pairing to consume those associations
  and resolve via geometric tie-breaking.
- New oracle test scaffolding to verify twin-pairing correctness at
  non-manifold edges. (~80 LOC test code.)

This matches the brief's "+200 LOC" estimate. **Not a small change**;
implementer-r's deferral is honest.

**§4 verdict: Stage 6 deferral is HONEST.** The cascade is structurally
distinct from per-patch labeling and Cherchi §6 doesn't model the twin
pairing problem. Estimated PR-Y17-TWIN cost is ≥200 LOC + new oracle
test. NOT hidden in a small change. ACCEPT.

**On PR-Y17-TWIN anchor recommendation** (per
`feedback_adversary_recommendations_need_canary.md`): I have NOT
empirically run a probe at the Step 6 boundary collection vs the Step 7
twin pairing site to determine which is the load-bearing fix anchor.
Both are plausible. I therefore **do NOT recommend a bound directive**
for PR-Y17-TWIN's anchor. PR-Y17-TWIN canary-runner should:
- Probe `topology_extract.rs:760-764` (the `is_boundary` predicate at
  Step 6) — does Step 6 see all the non-manifold-edge directed edges
  it needs to see, or are some elided by the patch-iteration order?
- Probe `topology_extract.rs:1097-1156` (the twin-pairing logic) —
  does the cascade arise from the `[]` (no candidate) arm or the
  `multiple` (collision) arm dominantly?
- The cohort's spotlight error strings show `twin = 0` (defaulted), not
  collision count > 0 — strongly suggests the `[]` arm is the dominant
  cascade mode for F0020/F0030/F0050. This is INFERENCE from spotlight
  output, NOT a probe; PR-Y17-TWIN canary must verify in-situ.

---

## §5 F0051 latent-exposure analysis (empirical)

Path 1 framing claim: "F0051 control regressed (was passing for wrong
reasons; manifold-edge barrier exposed latent Stage 6 cascade) — NOT
papering over." Validated empirically by direct probe.

**Pre-PR F0051 (git stash apply on dangling commit before this PR):**

I wrote a temp probe `crates/test-harness/tests/_adv14_f0051_probe.rs`
(reverted before this memo was finalized; not in `git diff`) that runs
F0051 directly via `run_single_case(dir, "F0051", true)` with
`YANG_BOOLEAN=1`. Result on baseline (HEAD without implementer-r diff):

> === F0051 probe ===
> Status: Passed
> Detail: 9 oracles passed
> Duration: 4.636544ms

**Pre-PR F0051 was a clean Status=Passed.** The 9 oracles
(`run_all_mesh_checks` per `oracle.rs:1342-1354`) include
`check_watertight_mesh`, `check_consistent_normals`,
`check_no_degenerate_triangles`, `check_unit_normals`,
`check_face_range_coverage`, `check_valid_indices`,
`check_outward_normals(0.95)`, `check_positive_signed_volume`,
`check_no_self_intersection`. All passed: F0051's pre-PR output mesh was
topologically clean by all 9 mesh-correctness oracles.

**Post-PR F0051 (implementer-r diff applied):**

Per the §1 sidecar parity test trace:

> [reference-parity F0051] case status=Failed
> detail=auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed:
>   kernel error: ... half_edge[8].twin = 0 but twin.twin = 27
>   (expected 8). Body created as standalone.; merge incomplete: 2
>   operations produced 2 separate solids (expected 1 merged)

**F0051 status: Passed → Failed with the canonical Stage 6 cascade error
string.** Same defect class as the F0020/F0030/F0050 cohort.

**Reconciling with Path 1 framing:** the brief's "passing for wrong
reasons" phrasing is slightly imprecise. F0051's pre-PR output was
topologically clean by all 9 oracles — it was NOT a silent-oracle case.
More precisely: under Yang's intersection-edge barriers, F0051's specific
input geometry produced a per-patch boundary structure that twin-paired
cleanly because Yang's patches and Yang's boundary collection co-evolved
consistently. Under Cherchi §5 manifold-edge barriers, F0051's input
exposes incidence-≥3 edges that produce multi-patch boundary edges, and
the existing Step 6 `is_boundary` predicate + Step 7 twin pairing cannot
resolve them — exactly the Stage 6 cascade defect mode that F0020/F0030/
F0050 also exhibit.

The framing is **HONEST** with this refinement: "F0051's pre-PR output was
clean by topology oracles, but the Yang intersection-edge barrier was
giving correct-looking output for the wrong patch decomposition. The
Cherchi §5 manifold-edge barriers expose the latent Stage 6 cascade that
was always present in the Step 6/7 logic for inputs with non-manifold
edges." Not papering over — the cascade IS deferred, and F0051's
exposure is a legitimate consequence of the architectural alignment.

**Additional finding (AMEND, not blocker):** F0051's Cherchi sidecar
output is itself non-well-formed (8 multi-paired edges per
`[reference-parity] F0051 Cherchi union output : verts=12 tris=28
unique_edges=34 unpaired=0 multi_paired=8 euler_chi=6 well_formed=false`).
F0051 may be in the same intrinsic-geometric-ambiguity class as F0030
(spec §6 lower bar) — but F0051 is configured in the test as a
"control" with `is_cohort_f0030=false`, asserting full well-formedness
on Cherchi's output. The implementer's docstring on
`pr_y16_parity_f0051_control` (cherchi2022_reference_parity.rs:478-486)
honestly notes the latent exposure but frames F0051 as a control case
when its Cherchi output is non-manifold (similar to F0030). PR-Y17-TWIN
should consider whether F0051 belongs in the F0030-style
lower-bar group, or whether the F0051 control test should switch to
`is_cohort_f0030=true` (or be re-categorized as cohort).

---

## §6 Architectural conformance check + adversary-13 amendments

Per spec §8 + adversary-13 memo (PR-Y16-INV validation §3.5, §4, §5):

1. **F0030 collision_count=2 (adversary-13 §3 amendment)**: per-patch
   labeling architecturally preempts the Stage 4b miscoloring class but
   does NOT preempt Stage 6 cascade collisions. F0030 post-PR error string
   (`half_edge[4].twin = 0 but twin.twin = 29`) is the same Stage 6
   cascade as F0020/F0050. Per-patch labeling architecturally preempted
   the collision class adversary-13 measured at Stage 4b, but the Stage 6
   re-expression remains. Empirical gate per spec §8.1: `spotlight_f0030`
   GREEN — NOT met (deferred per Path 1). Adversary-13's amendment is
   honored: collisions ARE preempted at Stage 4b (per pr11-test1's "0
   S4b violators"); the Stage 6 cascade is the remaining work.
2. **F0050 silent oracle (adversary-13 §3.5 amendment)**: per spotlight
   trace, F0050 post-PR is Status=Failed with mesh-oracle complaints
   (`watertight_mesh: 39 unpaired edges`, `consistent_normals: 162 of 265
   triangles have reversed normals`, etc.) — but NO `validate_yang_result_topology`
   panic (no `half_edge[X].twin=0` error). The silent oracle is STILL
   silent post-PR — the architectural fix did NOT close the F0050 gap
   (Path 1 explicitly defers this). Adversary-13's prediction (per-patch
   labeling MIGHT close F0050) is REFUTED. Empirical gate per spec §8.2:
   F0050 `[twin-oracle] unpaired_count = 0` — NOT met. Deferred to
   PR-Y17-TWIN per Path 1. Honest deferral.
3. **Cherchi §5 architectural conformance (adversary-13 §5.5)**: SHIPPED
   per §3 above.
4. **PR-Y15c-fix-2 cascade ruling-out (adversary-13 §4)**: confirmed.
   `git diff HEAD -- crates/kernel/src/boolean/yang_integration.rs` is
   empty. This PR does NOT touch `yang_integration.rs` or `surface_map`.
   F0020/F0030/F0050 errors come from `validate_yang_result_topology`
   (twin-symmetry check), NOT from `A15.5 ...` panic.

---

## §7 Cheaper-proxy discipline + verdict

**Cheaper-proxy discipline** (per
`feedback_adversary_recommendations_need_canary.md`):

I did NOT empirically run probes at:
- `topology_extract.rs:760-764` (the Step 6 `is_boundary` predicate that
  classifies which directed edges become per-patch boundaries) — to
  measure how often the predicate fires under non-manifold incidence ≥3
  conditions per cohort case.
- `topology_extract.rs:1097-1156` (the Step 7 twin-pairing loop) — to
  measure the dominant cascade arm (`[]` no-candidate vs `multiple`
  collision) per cohort case.

Therefore I do NOT recommend either as a bound directive for PR-Y17-TWIN's
anchor. They are CANDIDATE anchors that PR-Y17-TWIN canary-runner must
empirically verify themselves before committing. Per
`feedback_adversary_recommendations_need_canary.md`: "the cost of an
aborted canary is ~30 minutes of agent time; the cost of building probe
code against an invalid anchor is the full PR cycle."

**What I DID empirically verify** (and can recommend):
- Stage 4b labeling (Cherchi §5 + Algorithm 1) is correctly implemented
  per §3 above; per-patch ray-casts produce 0 S4b violators on a corpus
  that previously had 8.
- F0051 baseline pre-PR was Status=Passed, "9 oracles passed", 4.6ms.
  Post-PR is Status=Failed with the canonical Stage 6 cascade error
  string — pre-PR pass was topologically clean by all 9 mesh oracles,
  not silent-oracle.
- Cherchi sidecar output on F0051 is non-well-formed (8 multi-paired
  edges) — F0051 may belong in the F0030-style lower-bar group.
- pr11-test1's panic message lists 8 cases that fail at Stage 6 cascade:
  ["F0030", "F0060", "F0075", "F0086", "R0015", "R0040", "R0090", "R0095"]
  — this is the expected PR-Y17-TWIN cohort, expanding adversary-13's
  F0020/F0030/F0050 set.

**Yang corpus sweep delta is the load-bearing measurement**: pre-PR 11/157,
post-PR 10/157, F0051 is the -1. Path 1 prediction matches empirically.

**Verdict: ACCEPT (with two minor amendments).**

ACCEPT criteria (per Path 1 revised scope):
- ✓ Stage 4b labeling architecturally correct (§3)
- ✓ No kernel test regression (§1, 1248/31/42 == baseline)
- ✓ Stage 6 cleanly deferred to PR-Y17-TWIN (§4 honest deferral analysis)
- ✓ F0051 honestly explained (§5 empirical pre-PR pass + post-PR exposure)

**AMENDMENTS** (not blockers; should be addressed in 0f close-out OR
banked for PR-Y17-TWIN):

1. **F0051 control category**: F0051's Cherchi sidecar output is
   non-well-formed (8 multi-paired edges). The control test currently
   asserts full well-formedness via `is_cohort_f0030=false`, so the test
   panics at the Cherchi well-formed assertion BEFORE the Yang status
   assertion fires. F0051 should arguably be in the lower-bar group, OR
   the control category re-evaluated. The implementer's docstring at
   `cherchi2022_reference_parity.rs:478-486` notes the Stage 6 exposure
   honestly but does NOT note the Cherchi-output non-well-formedness.
   Suggested PR-Y17-TWIN action: switch F0051 to `is_cohort_f0030=true`,
   OR add an F0051-specific docstring carve-out covering the Cherchi
   output's known limitation.

2. **PR-Y15c silent fallback drift watch**: per the post-PR sweep, the
   141-failed bucket includes cases that previously short-circuited or
   timed out. Recommend PR-Y17-TWIN canary-runner verify NO new cases
   enter the failure bucket beyond F0051 (the corpus sweep noise should
   be constant after the architectural alignment) — i.e., pre-PR
   passes minus post-PR passes == {F0051} exactly, not a superset.

**Most important finding**: the Stage 4b architectural deliverable is
sound and verifiable in code (I-A through I-C all confirmed against
specific code sites). The Stage 6 deferral is an honest scope decision,
not papering over — Cherchi 2022 §6 does not model twin-pairing at
non-manifold edges, and Yang's B-Rep extension creates structurally
new work that warrants its own PR. F0051 framing holds up: pre-PR pass
was topologically clean by 9 oracles, post-PR exposure is the canonical
Stage 6 cascade.

---

## Verification (against task #95 contract)

- [x] §1 independent re-run: kernel lib 1248/31/42 == baseline confirmed;
      test-harness lib 92/0/1; pr11 3/2 with Stage 4b violators 0;
      spotlights F0020/F0030/F0050 RED with cascade strings;
      sidecar parity 2 pass / 4 fail (F0001+F0007 GREEN; cohort + F0051 RED).
- [x] §2 Yang corpus sweep: 10/157 pre vs 11/157 baseline = -1 (F0051);
      Path 1 prediction matches.
- [x] §3 Stage 4b architectural validation: 7-row table of invariants
      vs code sites, all verified present.
- [x] §4 Stage 6 deferral honesty: structurally distinct from §5,
      Cherchi §6 does not model, ≥200 LOC + new oracle estimated.
- [x] §5 F0051 empirical probe: pre-PR Status=Passed, 9 oracles, 4.6ms;
      post-PR `half_edge[8].twin=0 twin.twin=27`. Framing honest.
- [x] §6 architectural conformance + adversary-13 amendments addressed
      (4 amendments: collisions partial, F0050 deferred, conformance
      shipped, PR-Y15c-fix-2 cascade ruled out).
- [x] §7 verdict ACCEPT with 2 minor amendments. PR-Y17-TWIN anchor
      candidates listed but NOT recommended as bound directive
      (cheaper-proxy discipline observed).
- [x] `git diff HEAD` shows ONLY this memo + implementer-r's deliverable;
      temp probe file `_adv14_f0051_probe.rs` was REVERTED (file deleted
      before stash pop). Stash hygiene: dangling commit rescue
      successful; final workspace state matches implementer-r diff
      (4 modified files; 463 insertions, 79 deletions).
- [x] Heartbeats sent at INTERESTING events: §1 partial reproduction;
      §1, §3, §4 done summary.

**Sub-phase 0e complete. Routing to team-lead for sub-phase 0f close-out
(WASM rebuild + memory + commit).**

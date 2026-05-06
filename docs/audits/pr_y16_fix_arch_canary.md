# PR-Y16-FIX-ARCH 0a — anchor pre-verification canary memo

Sub-phase 0a deliverable. Read-only investigation. No fix code shipped.
Canary-runner verifies that F0020/F0030/F0050 fail BECAUSE of the
per-patch labeling absence in `flood_fill_patches` and NOT because of
upstream Stage 1 defects, by comparing Waffle's behavior against the
Cherchi 2022 reference C++ sidecar (`mesh_booleans_inputcheck` and
`mesh_booleans union`).

Reference inputs: `/tmp/pr_y16_arch_canary/{F0020,F0030,F0050}/{case}_{a,b,union}.obj`.
All sidecar runs use a 30 s timeout. The OBJ dumps are last-write-wins
across multiple boolean operations within a case (per investigator-a
memo §2 PR-VIZ-2 caveat) — for cases F0020 and F0030 we dumped the LAST
boolean only. F0050's dump captures the first failing boolean.

---

## §1 Sidecar input-check results per case

`mesh_booleans_inputcheck` validates Cherchi 2022 §3 input preconditions:
manifold (every undirected edge has 0/1/2 incident tris locally —
2-manifold or boundary), watertight (no boundary edges), local
orientation (consistent triangle winding within each connected
component), global orientation (outward normals), and intersection-free
(no self-intersecting tri pairs). Source: pre-existing PR-S2 corpus
sweep TSV at `docs/audits/cherchi_inputcheck_sweep_2026-05-04.tsv`,
re-confirmed by direct invocation on the OBJ dumps captured 2026-05-06.

| Case  | Mesh | Manifold | Watertight | Local Orient | Global Orient | Intersection-free | Verdict |
|-------|------|----------|------------|--------------|---------------|-------------------|---------|
| F0020 | A    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |
| F0020 | B    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |
| F0030 | A    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |
| F0030 | B    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |
| F0050 | A    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |
| F0050 | B    | PASS     | PASS       | PASS         | PASS          | PASS              | valid   |

**All 6 input meshes (3 cases × A,B) pass all five Cherchi preconditions.**
Stage 1 tessellation output is NOT the source of the F0020/F0030/F0050
defect. This is the OPPOSITE outcome of F0002 (PR-S1 finding: F0002
inputs were non-manifold and Cherchi hung 6 hours on them); the defect
class here is downstream of Stage 1.

---

## §2 Sidecar boolean results per case

Invocation: `mesh_booleans union <a>.obj <b>.obj <out>.obj`, 30 s
timeout cap. Wall-time measured by `date +%s.%N` around the binary call.
Output verts/tris counted by `grep -c '^[vf] '`. Output well-formedness
re-confirmed by feeding the union OBJ back into `mesh_booleans_inputcheck`.

| Case  | Input-check (§1) | Boolean status | Wall-time | Output verts | Output tris | Output well-formed? |
|-------|------------------|----------------|-----------|--------------|-------------|---------------------|
| F0020 | valid            | PASS           | 0.009 s   | 44           | 84          | YES (all 5 checks)  |
| F0030 | valid            | PASS           | 0.003 s   | 24           | 50          | NO — Manifold + Local Orient FAIL; Watertight + Global + Intersection PASS |
| F0050 | valid            | PASS           | 0.005 s   | 81           | 158         | YES (all 5 checks)  |

**Cherchi succeeds in milliseconds on all 3 cases**, in contrast to
Waffle (which fails Extrude 2 of F0020 with `half_edge[40].twin = 0`,
Extrude 2 of F0030 with `half_edge[5].twin = 0`, and produces 39
unpaired edges + 162/265 reversed normals on F0050). Output ranges
from 44–81 verts / 50–158 tris — well within Cherchi's normal operating
envelope.

**F0030 nuance**: Cherchi's union output is NOT fully well-formed
(Manifold check fails + Local Orientation check fails on the OUTPUT,
even though Watertight + Global + Intersection-free still pass). This
is consistent with adversary-13's `collision_count=2` finding for F0030:
F0030 has an intrinsic geometric ambiguity that even the reference
implementation handles imperfectly. The boolean still completes (no
hang), but the output topology has a defect mode the per-patch
labeling alone may not fully resolve. See §4 footnote.

---

## §3 Anchor verification — does the cohort fail because of per-patch absence, or upstream Stage 1?

Three pieces of evidence converge on the SAME conclusion:

1. **§1 input-check is unanimous PASS** on all 6 input meshes. By
   Cherchi 2022 §3's definition, the inputs are 2-manifold + watertight
   + intersection-free — the EXACT precondition Cherchi 2022 §5 + §5.1
   Algorithm 1 require for the per-patch labeling to be well-defined.
   No Stage 1.5 mesh-cleanup gap is exposed by these three cases.

2. **§2 sidecar boolean produces clean output** in milliseconds on
   F0020 and F0050 (and a partial-quality output on F0030). The
   reference implementation, fed the SAME inputs, does NOT exhibit
   the twin-symmetry defect that Waffle's `flood_fill_patches` does.
   This eliminates the hypothesis that the defect originates upstream
   of `flood_fill_patches`.

3. **In-situ canary at the candidate Phase 0d implementer site**
   (run inline by canary-runner, ENV-gated, REVERTED before this
   memo was finalized — `git diff` clean except this file). Counted
   `undirected_incidence` per F0020/F0030/F0050 boolean inside
   `flood_fill_patches`. Findings:

   | Case-boolean    | total_edges | count1 | count2 (manifold) | count3 | count≥4 | manifold-barrier (≠2) | yang-barrier (current) |
   |-----------------|-------------|--------|-------------------|--------|---------|-----------------------|------------------------|
   | F0020 boolean 1 (FAILING) | 109     | 0      | 99                | 10     | 0       | 10                    | 20                     |
   | F0020 boolean 2 (passing) | 126     | 0      | 126               | 0      | 0       | 0                     | 28                     |
   | F0030 boolean 1 (FAILING) | 71      | 0      | 58                | 9      | 4       | 13                    | 12                     |
   | F0050 boolean 1 (FAILING) | 231     | 0      | 225               | 0      | 6       | 6                     | 51                     |
   | F0050 boolean 2 (FAILING) | 303     | 0      | 297               | 0      | 6       | 6                     | 67                     |
   | F0050 boolean 3 (FAILING) | 413     | 0      | 409               | 0      | 4       | 4                     | 91                     |

   For every failing boolean: `count1 = 0` (no boundary edges, all
   inputs watertight ✓), and `manifold-barrier-count != yang-barrier-count`
   for ALL cases. The two predicates classify boundary edges differently.
   For F0020 boolean 2 (the SUCCEEDING one), `manifold-barrier-count = 0`
   — the mesh is fully manifold, so the Cherchi predicate produces NO
   boundary edges at all, and the Yang barrier has 28 edges that are
   NOT patch boundaries by Cherchi's definition. The two predicates
   thus disagree even on a passing case; the per-patch-labeling refactor
   shifts the barrier semantics across the board.

**Verdict**: F0020 + F0030 + F0050 fail BECAUSE of the missing per-patch
labeling + manifold-edge-barrier invariant in `flood_fill_patches`,
NOT because of any Stage 1 defect. Adversary-13 memo §5's
`yang_audit_c_cherchi2022.md` YC-06 deviation is the architectural
root cause. Investigator-a memo §3 hypothesis (a) (Step 6 boundary
classification at L697-L701 dropping forward HEs because the reverse
neighbor is in a different patch by Yang's barrier) is a SYMPTOM of
the same architectural deviation — the manifold-edge invariant would
guarantee that for every patch boundary edge, BOTH its forward and
reverse half-edges live within the boundary set of SOME patch.

---

## §4 Spec scope decision

**(A) Per-patch refactor only.** The spec proceeds with sub-phase 0b as
planned: `flood_fill_patches` switches to manifold-edge barriers (Step 5
predicate change at L545: `intersection_edges.contains(...)` →
`undirected_incidence.get(canon_key) != Some(&2)`), `label_cells` already
uses per-patch labeling per PR11 (`exact_mesh.rs:1964`); the wire-up is
ensuring `flood_fill_patches`'s LOCAL `patches` array (built at
`topology_extract.rs:526` via the Yang-style flood-fill) is REPLACED by
`build_manifold_patch_graph`'s `patches`, with Step 6 boundary
classification (L688-L708) consuming the manifold-edge barrier.

Rationale:
- **§1 unanimous valid** rules out Stage 1.5 mesh-cleanup expansion.
- **§2 Cherchi succeeds** on F0020 + F0050 confirms the per-patch
  algorithm DOES converge for the cohort's input geometry.
- **§3 incidence canary** confirms the manifold-edge predicate is
  well-defined on the cohort and produces a non-trivial barrier set
  for every failing case.
- F0030's partial-quality Cherchi output (Manifold + Local Orientation
  fail in §2) is a PR-Y16-followup risk: per-patch refactor SHOULD fix
  the twin-symmetry defect (the user-visible error string), but may not
  fully resolve F0030's intrinsic ambiguity even after the refactor.
  Adversary-14 must check this empirically per the spec's existing §0e
  step 4 ("F0050 silent-oracle question") — extend the same check to F0030.

**Footnote on F0030 followup**: even if PR-Y16-FIX-ARCH ships and F0030's
twin-symmetry defect goes away, the Cherchi reference's own output on
F0030 is non-manifold + locally-misoriented. This means F0030 may have
an intrinsic geometric ambiguity (see Cherchi 2022 §6 boolean-evaluate
discussion of edge cases) that requires either (a) the existing F0030
spotlight test asserting a weaker invariant (e.g., Watertight + Global
Orient + Intersection-free, dropping Manifold + Local Orient), or
(b) a follow-up PR characterizing F0030's specific geometry as a
known limitation. NOT a blocker for PR-Y16-FIX-ARCH.

**ABORT not triggered.** §3 answers cleanly. §4 picks (A).

---

## §5 Recommended Phase 0d implementer canary site (self-canaried)

Per `feedback_adversary_recommendations_need_canary.md`, the recommended
canary site is one this canary-runner empirically ran during §3, NOT
inferred from reading code.

**Recommended site**: `crates/kernel/src/boolean/topology_extract.rs`
inside `flood_fill_patches`, immediately AFTER the `directed_edge_to_tris`
HashMap is built (around L477, just before the existing Step 4 `boundary_edges
+ intersection_edges` build at L489). Add an env-gated `eprintln!` that:

1. Builds an `undirected_incidence: HashMap<(usize, usize), usize>` over
   `all_tris`, using `min(v0,v1):max(v0,v1)` as canonical key.
2. Counts how many edges have incidence != 2 (the new manifold-edge
   barrier set, Cherchi 2022 §5).
3. Compares to the size of the existing `intersection_edges` set
   (the current Yang barrier).
4. Prints both numbers.

The canary FIRES on F0020/F0030/F0050 with the data captured in §3
(table). The test invocation:

```
PR_Y16_CANARY=1 YANG_BOOLEAN=1 cargo test -p test-harness \
  --test assay_randomized -- spotlight_f0020 --ignored --nocapture
```

ABORT condition for the implementer: if the canary prints
`manifold-barrier (count != 2) edges = 0` for the failing F0020 boolean
1 (i.e., the F0020 input is fully manifold from `flood_fill_patches`'s
own view), the planned per-patch-refactor anchor is wrong. In this
canary's run, F0020 boolean 1 produced 10 manifold-barrier edges, so
the anchor IS valid for F0020. F0030 produced 13, F0050 produced 6/6/4
across its three booleans. The canary fires with data on every cohort
case — anchor verified.

**What the implementer should NOT use as a canary**: the existing
`[twin-oracle]` block at `flood_fill_patches`'s end (PR-Y16-INV
deliverable). That oracle measures the SYMPTOM (post-pairing
twin-symmetry violation), not the SOURCE (barrier predicate). It will
not distinguish a wrong refactor from a right one — both could yield
`unpaired_count = 0` while the topology is still subtly wrong. Use the
recommended incidence-counter canary above for anchor verification,
and the `[twin-oracle]` block plus the spotlight tests for
GREEN-confirmation post-refactor.

**Banked secondary canary** (NOT run by canary-runner; for adversary-14
or implementer-r if a deeper investigation is needed during 0d): per
investigator-a memo §4 banked candidates, an `eprintln!` at L702
reporting when `seen.insert((v0,v1))` returns false (the dedup-drop
hypothesis a2). This is a useful secondary if the manifold-edge-barrier
refactor still leaves residual unpaired edges after the predicate
change at L545.

---

## Verification (against task #91 contract)

- [x] §1 sidecar input-check on F0020/F0030/F0050 (both meshes A and B).
- [x] §2 sidecar `mesh_booleans union` on each case with 30 s cap;
      reported wall-time + output verts/tris + output well-formedness.
- [x] §3 anchor verification: cites §1 + §2 + adversary-13 §5 + an
      empirically-run incidence canary inside `flood_fill_patches`.
- [x] §4 picks ONE scope decision: (A) per-patch refactor only.
- [x] §5 self-canaried recommendation: incidence-counter canary at
      L477, fires with data on all 3 cohort cases (run during §3).
- [x] No fix code shipped: temporary OBJ-dump test
      `_pr_y16_arch_canary_dump.rs` and the §3/§5 incidence-counter
      canary in `topology_extract.rs` were both REVERTED;
      `git diff` post-task shows ONLY this memo.
- [x] All sidecar runs used 30 s timeout; all completed in <10 ms
      (well-formed input ⇒ Cherchi is fast, per
      `cherchi2022_sidecar_feasibility.md` "Build verified 2026-05-03").
- [x] Heartbeat to team-lead at INTERESTING events (input-check
      result; boolean run completion; canary fire-with-data;
      §4 decision).

**Sub-phase 0a complete. Routing to spec-writer-n for sub-phase 0b
(per task #92).**

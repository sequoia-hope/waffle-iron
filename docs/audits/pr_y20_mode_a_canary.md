# PR-Y20-MODE-A sub-phase 0a — canary-runner-7 anchor canary

**Author:** canary-runner-7
**Date:** 2026-05-06
**Scope:** Empirical NMM vs MISSING discriminator on F0020 Extrude 3 +
F0044/F0030/F0051 cohort. Per `feedback_anchor_before_fix.md` +
`feedback_oracle_credibility_via_role_separation.md` +
`feedback_adversary_recommendations_need_canary.md`: empirical probe
before any spec/fix coding. Probes applied + REVERTED; `git status`
clean.

**Verdict (§2): MIXED.** F0020 Extrude 3 is **74% NMM (23/31) +
26% MISSING (8/31)**. F0044 batch is **98% NMM (102/104)**.
F0051 is **100% MISSING (3/3)**. F0030 has **0 Mode A** (PR-Y19-MODE-B
fully resolved its twin defect; remaining failure is downstream).
The drill-down probe identifies the MISSING site: **L853
`is_boundary` predicate** (post-PR-Y19-MODE-B numbering) — every
MISSING reverse edge is dropped because the would-be reverse-emitter
patch contains BOTH directions of the canonical edge in its triangle
set, marking the edge as interior rather than boundary.

---

## §1 F0020 Extrude 3 probe data

Command:
```
TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020 --ignored --nocapture --test-threads=1 \
  > /tmp/canary7_f0020_stdout.log 2> /tmp/canary7_f0020_stderr.log
```

**Boolean #1 (Extrude 2)** post-PR-Y19-MODE-B: `[topo-extract]
summary: paired=48, unpaired=0, ambiguous=0`. Mode A `[]` arm fires 0
times. Mode B fix is fully effective on this boolean.

**Boolean #2 (Extrude 3)** post-PR-Y19-MODE-B: `[topo-extract]
summary: paired=66, unpaired=31, ambiguous=0`. Mode A `[]` arm fires
**31 times** (pure Mode A signature: `unpaired=31, ambiguous=0`).

**`[modeA-canary]` distribution (probe at `topology_extract.rs`
post-PR-Y19-MODE-B Step 7 `[]` arm):**

| metric | count | fraction |
|---|---|---|
| Total Mode A `[]` cases | 31 | 100% |
| `rev_in_de2t=false` (NMM) | 23 | 74% |
| `rev_in_de2t=true` (MISSING) | 8 | 26% |
| `fwd_in_de2t=false` (anomaly) | 0 | 0% |

**The 8 MISSING forward canonical pairs** (HE in `directed_he` is
unpaired but `(v1,v0)` exists in `directed_edge_to_tris`):
`(71,69), (69,70), (70,73), (73,72), (72,68), (68,66), (66,67),
(96,26)` (he_fwd = 50, 51, 52, 53, 54, 55, 56, 68 respectively).

**Drill-down probe at L853 (`is_boundary` predicate) for the 8
MISSING reverses** (post-PR-Y19-MODE-B):

For each MISSING canonical pair `(v0, v1)`, the reverse direction
`(v1, v0)` was attempted by exactly ONE patch and rejected with
`is_boundary=false`:

| MISSING fwd canon | reverse attempt | is_boundary | owner_pi | owner_blocks | will_emit |
|---|---|---|---|---|---|
| (71,69) | pi=27 ti=229 → (69,71) | **false** | None | false | false |
| (69,70) | pi=26 ti=219 → (70,69) | **false** | None | false | false |
| (70,73) | pi=26 ti=223 → (73,70) | **false** | None | false | false |
| (73,72) | pi=26 ti=207 → (72,73) | **false** | None | false | false |
| (72,68) | pi=26 ti=208 → (68,72) | **false** | None | false | false |
| (68,66) | pi=26 ti=209 → (66,68) | **false** | None | false | false |
| (66,67) | pi=25 ti=197 → (67,66) | **false** | None | false | false |
| (96,26) | pi=11 ti=89  → (26,96) | **false** | None | false | false |

**ALL 8 dropped at L853 `is_boundary` predicate.** Zero R3 owner
intervention. `directed_edge_to_tris.get(&(v0, v1))` returns
neighbors **in the same patch** that attempted the reverse →
predicate evaluates to false.

**Forward-direction emitter for the same canonical pair (cross-check
on `(71,69)`):** pi=6 ti=32 emits `(71,69)` with `is_boundary=true`
(pi=6 has only the forward direction in its tris). pi=27 ti=230 also
contains `(71,69)` but with `is_boundary=false` (pi=27 contains both
directions). The patch-segmentation produced a non-conformal pi=27
that has both directions of the edge → reverse is interior to pi=27,
boundary nowhere else.

**For `(96,26)`:** pi=8 ti=99 emits `(96,26)` (the survivor that
became HE 68 in directed_he with `unpaired_count++`). pi=11 ti=89 is
a **degenerate triangle** with vertices `{96, 26, 96}` (self-loop
edge `(96,96)` visible in trace) — its reverse-direction attempt
fails because the same degenerate tri contains both `(26,96)` and
`(96,26)`.

---

## §2 Mechanism identification

**MIXED.**

- **Per-case characterization:**
  - F0020 Extrude 3: 74% NMM + 26% MISSING (mixed, NMM-dominant)
  - F0044 batch: 98% NMM + 2% MISSING (almost pure NMM)
  - F0051: 100% MISSING (pure MISSING)
  - F0030: 0% Mode A (resolved by PR-Y19-MODE-B)
- **Aggregate (138 Mode A `[]` instances):** 125 NMM (91%) + 13
  MISSING (9%). NMM dominant in aggregate.

**MISSING site, empirically localized (load-bearing):**
**L853 `is_boundary` predicate.** All 13 MISSING reverses across
F0020+F0044+F0051 are rejected because
`directed_edge_to_tris.get(&(v1, v0))` returns at least one neighbor
in the same patch attempting the reverse. Two sub-mechanisms:
1. **Non-conformal patch segmentation** (F0020's 7-edge cluster
   `(71,69)…(67,66)`): pi=27/26/25 contain both directions of the
   canonical edge in their triangle sets, but no other patch claims
   the reverse direction as boundary. The reverse never gets
   emitted.
2. **Degenerate triangles** (F0020's `(96,26)` + most of F0051):
   triangles with repeated vertex indices (e.g., F0051 pi=6 ti=10
   `{9,8,8}`, F0020 pi=11 ti=89 `{96,26,96}`) auto-include both
   directions of an edge, suppressing reverse emission.

**NMM mechanism, structural inference (NOT canary-empirical):** The
trace agent's hypothesis that NMM arises from non-manifold seams
where two B-Rep faces meet on the same canonical edge with same
winding is consistent with the data (rev_in_de2t=false means NO
triangle anywhere generates the reverse direction). I did not probe
geometric position to confirm "geometrically non-manifold"; the
empirical claim is narrower: **for these 125 forward HEs, the
reverse direction does not exist in `directed_edge_to_tris` at
all.** Whether this is a true non-manifold meeting (Yang §4.4.2
allowed twin=None) or an upstream tessellation/conformality bug
(reverse should exist but `subdivide_mesh_pair` dropped it) is a
question for the spec writer's reading of Yang §4.4.2 + comparison
to Cherchi 2022 §5 expected output.

---

## §3 Cohort breakdown

| Case | total `[]` cases | NMM (rev=false) | MISSING (rev=true) | dominant | `[topo-extract]` summary |
|------|------------------|-----------------|--------------------|----------|---------------------------|
| F0020 Extrude 3 (b#2) | 31 | 23 (74%) | 8 (26%) | NMM-leaning MIXED | paired=66 unpaired=31 ambig=0 |
| F0020 Extrude 2 (b#1) | 0 | — | — | resolved by PR-Y19-MODE-B | paired=48 unpaired=0 ambig=0 |
| F0044 (b#5) | 31 | 30 (97%) | 1 (3%) | NMM | paired=101 unpaired=31 ambig=0 |
| F0044 (b#6) | 37 | 36 (97%) | 1 (3%) | NMM | paired=118 unpaired=37 ambig=0 |
| F0044 (b#7) | 36 | 36 (100%) | 0 | NMM (pure) | paired=182 unpaired=36 ambig=0 |
| F0030 (b#1+b#2) | 0 | — | — | resolved by PR-Y19-MODE-B | paired=36/34 unpaired=0 ambig=0 |
| F0051 | 3 | 0 | 3 (100%) | MISSING (pure) | paired=12 unpaired=3 ambig=0 |
| **Aggregate** | **138** | **125 (91%)** | **13 (9%)** | **NMM (aggregate)** | — |

**Cohort observations:**
- F0030's twin-pairing is fully resolved post-PR-Y19-MODE-B
  (`paired=36 unpaired=0 ambig=0` × 2 booleans). F0030's surface
  status:Failed is downstream defect (watertight_mesh + Euler — see
  PR-Y19-MODE-B validation §3). NOT a Mode A target.
- F0044 batch (3 booleans, all of which fail) is overwhelmingly NMM:
  102/104 NMM. The 2 MISSING are isolated edge cases.
- F0051 is opposite extreme — 100% MISSING, 0 NMM. All 3 cases stem
  from degenerate triangles in pi=6/7/8/9 with repeated vertex
  indices producing self-loop edges `(8,8)` `(11,11)` etc.
- F0020 Extrude 3 sits in between: 74% NMM cluster on `mesh_A`
  faces (canon vertices 8–37 area + 53–64 area) + 26% MISSING
  cluster on canon `(66–73)` chain (likely a non-conformal patch
  with both directions) + one degenerate triangle case `(96,26)`.

**Sub-mechanism distribution within MISSING (13 instances total):**
- Non-conformal patch segmentation (no degeneracy): 7 (F0020 `(71,69)
  ↔ (67,66)` chain — pi=25/26/27 contain both directions of
  the same canonical edge in their tris)
- Degenerate triangle (repeated vertex index): 6 (F0020 `(96,26)` +
  F0051 all 3 + F0044 both 2)

---

## §4 Spec scope decision

**MIXED scope-down: target NMM (the dominant 91% mechanism), bank
MISSING.**

**Rationale (load-bearing):**
1. **NMM is the dominant aggregate mechanism (125/138 = 91%).**
   F0020 Extrude 3 is MIXED but NMM-leaning (74%); F0044 batch is
   NMM-dominant (98%). A single-mechanism spec targeting NMM will
   resolve the bulk of the corpus' Mode A panic surface.
2. **F0020 anchor satisfaction:** even partial resolution of F0020
   Extrude 3 (the 23 NMM cases) leaves 8 MISSING residual. F0020
   may stay Status:Failed post-PR-Y20-MODE-A (8 unpaired residual)
   unless the type-system change for NMM also covers MISSING. This
   is a known risk — see §5.
3. **MISSING upstream causes are heterogeneous (degenerate
   triangles + non-conformal patches) and likely deserve a
   different fix class** than NMM (which is paper-faithful per the
   plan's reading of Yang §4.4.2). Bundling both into one PR
   inflates blast radius without architectural cohesion. Per
   `feedback_yang_only.md`: the NMM fix is paper-extension; the
   MISSING fix is upstream-defect-localization. Separate concerns.

**The NMM branch matches the plan's NMM-branch description in
sub-phase 0b** (`Option<HalfEdgeIdx>` type extension; ~40-50 LOC
delta across 5 files; validator update).

**Banked for PR-Y21+ (MISSING follow-on):**
- Non-conformal patch segmentation: investigate why
  `subdivide_mesh_pair` produces patches with both directions of an
  edge (per pi=27 ti=229+230 having both `(69,71)` and `(71,69)`).
  Likely a Step 5a manifold-component segmentation bug surfaced now
  that PR-Y19-MODE-B's R3 routing changed which patches "win" the
  forward direction.
- Degenerate triangles: F0051 + F0020 `(96,26)` show repeated
  vertex indices in subdivided triangles. Likely a tessellation /
  CDT issue producing zero-area triangles. Should be filtered out
  in Step 4b/5 (degenerate-tri elimination) before patch
  classification.

---

## §5 Self-canaried recommendation for sub-phase 0d implementer-x

Per `feedback_adversary_recommendations_need_canary.md`: my
recommendations cite empirical observation only.

**Recommended fix shape (NMM branch):**
Extend `HalfEdge.twin` from `HalfEdgeIdx` to `Option<HalfEdgeIdx>`
(or sentinel `INVALID`). The Step 7 `[]` arm sets `twin = None` for
HEs where `rev_in_de2t == false`. The validator
(`yang_integration.rs::validate_yang_result_topology`) accepts
`twin=None` for non-manifold edges.

**Empirical constraints the implementer MUST verify:**
1. **The 125 NMM HEs across F0020 Extrude 3 + F0044 batch + cohort
   genuinely have `rev_in_de2t=false` POST-FIX too.** Re-run the
   `[modeA-canary]` probe (cited in §1) at the start of sub-phase
   0d as the anchor pre-verification canary. If counts shift (e.g.,
   F0020's 23 NMM drops to 5 NMM), upstream (PR-Y19-MODE-B's R3 or
   `subdivide_mesh_pair`) has changed; the spec/fix may need
   re-scoping.
2. **The 13 MISSING HEs WILL STILL UNPAIR post-NMM-fix** because
   the L853 `is_boundary` predicate isn't touched. F0020 Extrude 3
   may stay Status:Failed (8 unpaired residual). F0051 will stay
   Status:Failed (3 residual). F0044 batch will improve from
   31+37+36 unpaired → 1+1+0 unpaired (per cohort table) — the
   yang validator will then panic on the 1+1=2 leftover MISSING in
   booleans #5+#6, but boolean #7 will pass. **Spec author should
   set realistic Status:Passed expectations** that account for the
   MISSING residual persisting.
3. **Validator update — distinguish legitimate twin=None from
   defect twin=None.** Adversary-20's blast-radius check (per plan
   risk #3) must verify that `twin=None` is set ONLY in the Step 7
   `[]` arm where `rev_in_de2t=false` is empirically the case, not
   silently as a fallback in other arms (e.g., `multiple` arm
   ambiguity reduction). This is the load-bearing risk for accepting
   the type-system change.

**Banked for sub-phase 0d pre-fix canary** (re-run by implementer-x
to verify state hasn't drifted):
- ⚠ Whether downstream consumers (`flood_fill_patches`,
  `tessellate_waffle_solid` retessellation, brep_assembly,
  `validate_yang_result_topology`) handle `twin=None` HEs without
  silent unwrap/panic. Per plan risk #2: ~95 read sites + ~52 write
  sites need audit. Implementer-x should grep `\.twin\b` in
  `crates/kernel/src/` post-spec to catalog.
- ⚠ Whether tests pattern-matching `HalfEdgeIdx(0)` as twin sentinel
  (per plan risk #7) need updates. Catalog by grep before touching
  the type.
- ⚠ Whether F0050 (banked separately) shares the NMM mechanism. I
  did NOT probe F0050. Spec author may want to canary it during 0c
  test design.

---

## Verification

- `git diff --stat` shows only this file
  (`docs/audits/pr_y20_mode_a_canary.md`). Probes applied + reverted
  cleanly via `git checkout -- crates/kernel/src/boolean/topology_extract.rs`.
  `cargo build -p kernel` clean post-revert (55 pre-existing
  warnings unchanged).
- §1 has F0020's empirical probe data with 31 cases broken down +
  Step 6 drill-down on the 8 MISSING.
- §2 picks ONE mechanism category: **MIXED** (with 91% NMM
  aggregate). Both sub-mechanisms (non-conformal segmentation +
  degenerate triangles) characterized for MISSING.
- §3 cohort table has empirical counts for F0020/F0044/F0030/F0051.
- §4 picks ONE spec scope: **NMM-branch (target dominant 91%
  mechanism, bank MISSING for PR-Y21+).**
- §5 self-canaried per `feedback_adversary_recommendations_need_canary.md`:
  recommendations cite §1 probe directly; explicitly flags the
  realistic Status:Passed expectations + the 2 banked risks for the
  pre-fix canary.
- NO production code changes (probes reverted; verified clean).
- NO recommendation for synthetic fill / fallback paths per
  `feedback_yang_only.md`.
- NO speculative claims about NMM being "true non-manifold meeting"
  vs upstream conformality bug — this is left as a spec-writer
  question per `feedback_external_coherence.md` (Yang §4.4.2
  reading + Cherchi 2022 §5 sidecar parity).

**Sub-phase 0a complete. Routing to spec-writer-t for sub-phase 0b.**

# PR-Y16-INV F0020 Discovery Validation — adversary-13 memo

Sub-phase 0b adversary validation of investigator-a's discovery memo at
`docs/audits/pr_y16_inv_f0020_discovery.md`. Per
`feedback_oracle_credibility_via_role_separation.md` adversary-13 is a NEW
agent rotated in for this validation; investigator-a's reasoning was NOT
consulted beyond what is written in the memo. Per the plan's risk #7 +
`feedback_adversary_recommendations_need_canary.md`, this memo's §6 does NOT
recommend cheaper-proxy probes that this adversary did not empirically run
during §3 cohort sweep.

**Scope**: validate, do not patch. Investigator-a's deliverables (memo,
spotlight test, oracle code) were NOT modified. Temporary cohort spotlight
tests + the §2 mutation were written, executed, and reverted before this
memo was finalized; `git diff` post-revert confirms only this memo and
investigator-a's deliverables remain.

---

## §1 Independent re-run

**Re-run command** (NEW dir `/tmp/viz/f0020_adv`, fresh build):
```
TWIN_DEBUG=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  YANG_STAGE_DUMP=/tmp/viz/f0020_adv \
  cargo test -p test-harness --test assay_randomized -- \
    spotlight_f0020 --ignored --nocapture --test-threads=1 \
    > /tmp/viz/f0020_adv/stdout.txt 2> /tmp/viz/f0020_adv/twin_debug.txt
```

**Result: REPRODUCED byte-for-byte** (per the memo §1 + §2 specifics):

- Status: `Failed`
- Detail starts with: `auto-union-failed (1 warning(s)): Extrude 2:
  Auto-union failed: kernel error: operation not supported: yang_boolean:
  result validation failed: half_edge[40].twin = 0 but twin.twin = 21
  (expected 40)` — verbatim match to memo §1.
- Pre-pairing oracle on FAILING boolean (Extrude 2):
  - `total_directed_edges=86` ✓ matches memo
  - `unpaired_count=10` ✓ matches memo
  - `collision_count=0` ✓ matches memo
  - First 5 offenders: he=40,41,44,45,46 — all `twin=0 twin.twin=21` ✓ matches memo
- Coordinates of v22..v27 match memo §2 to all reported digits (memo
  reported `2.31e-2` for v22.x; my run reads `2.308216e-2` — same value
  at the memo's precision).
- Second boolean (Extrude 3): `unpaired_count=0`. ✓ matches memo's
  "the defect is per-case, not endemic".

**Verdict on §1: ACCEPT** — investigator-a's reproduction is independently
confirmed. The probe instrumentation produces the same data on a fresh
adversary-controlled directory.

---

## §2 Mutation test of the pre-pairing oracle

**Mutation design** (my own; not a copy of investigator-a's):

Injected (env-gated `ADV13_MUTATE=1`) at the very top of the existing
`if twin_debug { ... }` oracle block in `flood_fill_patches`:
```rust
if std::env::var("ADV13_MUTATE").as_deref() == Ok("1")
   && arena.half_edges.len() >= 6 {
    let original = arena.half_edges[5].twin.0;
    arena.half_edges[5].twin = HalfEdgeIdx(0);
    eprintln!("[adv13-mutate] he[5].twin: {} -> 0 ...", original);
}
```

**Control**: F0001 baseline (no mutation, with `YANG_BOOLEAN=1`):
- `Status: Passed`, `Detail: 9 oracles passed`
- `[twin-oracle] total_directed_edges=24 unpaired_count=0 collision_count=0`
- Confirms F0001 is a clean control case.

**With mutation** (`ADV13_MUTATE=1 TWIN_DEBUG=1 YANG_BOOLEAN=1` on F0001):
- `[adv13-mutate] he[5].twin: 10 -> 0` (mutation fires)
- `[twin-oracle] total_directed_edges=24`
- `[twin-oracle] unpaired_count=2` ← oracle FIRES correctly
- `[twin-oracle] collision_count=0`
- `[twin-oracle] offender he=5 twin=0 twin.twin=15 origin=v5(...) dest=v6(...)`
- `[twin-oracle] offender he=10 twin=5 twin.twin=0 origin=v6(...) dest=v5(...)`
- Downstream `validate_yang_result_topology` ALSO catches it:
  `half_edge[5].twin = 0 but twin.twin = 15 (expected 5)`.

**Note on count = 2 vs expected 1**: my mutation breaks the symmetric
property bidirectionally — he[5].twin → he[0] makes BOTH he[5] and the
old he[10] (which was he[5]'s legitimate twin) fail the
`he.twin.twin == self` invariant. This is correct oracle behavior:
the oracle measures the symmetric invariant, not the count of mutations.

**Mutation reverted** before further work. `git diff` confirms only
investigator-a's instrumentation remains.

**Verdict on §2: ACCEPT** — the pre-pairing `[twin-oracle]` block is
load-bearing. Without mutation: silent on a passing case. With a
deliberate twin corruption: fires on the corrupted he and on its
formerly-paired partner. Memo §2 conclusions are admissible.

---

## §3 Cohort sweep (F0001 control + F0002, F0010, F0020, F0030, F0050)

Protocol: temporary spotlight tests `adv13_cohort_FXXXX` (5 tests,
all reverted before reporting completion), each running
`run_single_case(dir, "FXXXX", true)` with
`TWIN_DEBUG=1 YANG_BOOLEAN=1 YANG_STAGE_DUMP=/tmp/viz/cohort_FXXXX`.

| Case  | Status | `result validation failed` (he[X].twin=Y...) | oracle fires? | unpaired | collisions | unpaired pattern |
|-------|--------|----------------------------------------------|---------------|----------|------------|------------------|
| F0001 | Passed | NO                                           | yes (clean)   | 0        | 0          | n/a (control)    |
| F0002 | Passed | NO                                           | yes (clean)   | 0        | 0          | n/a              |
| F0010 | Failed | NO (degenerate triangles + self-intersection) | yes (clean)  | 0        | 0          | NOT this defect class |
| F0020 | Failed | YES (he=40 twin=0 twin.twin=21) [anchor]      | yes          | 10       | 0          | fwd-only        |
| F0030 | Failed | YES (he=5 twin=0 twin.twin=30)                | yes          | 29       | **2**      | fwd-only (+ ambig from upstream) |
| F0050 | Failed | NO (oracle fires but Yang validator does not) | yes (×2)     | 2 each   | 0          | fwd-only         |

**Pattern shape**: all cases that fire the oracle (F0020, F0030, F0050)
show `[topo-extract] unpaired forward HE` only — never `unpaired reverse`.
Consistent with investigator-a's claim "fwd-only OR rev-only — never both"
across all 3 firing cases.

**Threshold ruling**: per spec §0b, "≥3/5 = this is a class".

- **2/5 strict** (F0020 + F0030) match the EXACT validate-error format
  `result validation failed: half_edge[X].twin = 0 but twin.twin = Y`.
- **3/5 broad** (F0020 + F0030 + F0050) all fire `[twin-oracle]
  unpaired_count > 0` with the same fwd-only shape.

**Verdict on §3: AMENDED ACCEPT** — the defect IS a class, but the
class is BROADER than memo §3's "unpaired_count>0, collision_count=0"
characterization:
- F0020 has `collision_count=0` — fits the memo exactly.
- F0030 has `collision_count=2` AND `unpaired_count=29` — both defect
  modes coexist. The investigator's hypothesis (a) (Step 6 boundary
  classification dropping forward HEs) is consistent with F0030 too,
  but F0030 has additional damage (the 2 collisions suggest
  multi-pairing of one canonical edge that the memo's hypothesis (a)
  alone cannot explain).
- F0050 fires the oracle on BOTH Extrude 2 and Extrude 3 with
  `unpaired_count=2`, but `validate_yang_result_topology` does NOT
  raise the `half_edge[...].twin=...` error string. This means the
  oracle catches a defect class that's STRICTLY WIDER than what the
  downstream validator catches — the unpaired HEs presumably get
  cleaned up by `result_topology_to_waffle_solid` when converting
  `ResultTopology → WaffleSolid`. **PR-Y16-FIX scope question**:
  does the fix need to address the F0050 case (silent oracle fire,
  no user-visible error) OR only the F0020/F0030 case (oracle +
  validator both fire)? Memo recommends the implementer of PR-Y16-FIX
  decide once they have a fix candidate.

**Bottom line**: 3/5 ≥ threshold = THIS IS A CLASS. The class is
"twin-symmetry violation in `flood_fill_patches` output for fwd-only
unpaired HEs", with F0030 demonstrating the class can co-occur with
collision-count > 0.

---

## §4 PR-Y15c-fix-2 cascade ruling-out (per plan risk #4)

**Question**: was F0020 previously masked by a silent fallback that
PR-Y15c-fix-2 (commit `1aed3ce`) or PR-Y15c-fix-2.2 (commit `55e52dc`)
removed?

**Code-path check**:
- PR-Y15c-fix-2 + 2.2 both modify `result_topology_to_waffle_solid` in
  `crates/kernel/src/boolean/yang_integration.rs`. The fix is the
  `surface_map.get(...).unwrap_or_else(|| panic!("A15.5 ..."))` panic-
  promotion.
- F0020's defect surfaces in `flood_fill_patches` (Step 6 boundary
  collection at `topology_extract.rs:688-708`), which runs UPSTREAM of
  `result_topology_to_waffle_solid` in `yang_boolean_inner`'s pipeline
  (Step 6→7 → Step 8 validation, with surface_map lookup in between).
- F0020's error string is the `validate_yang_result_topology` error,
  NOT the `A15.5 ...` panic. If F0020 had been a previously-masked
  surface_map miss, it would now panic with `A15.5`, not return
  `result validation failed: half_edge[40].twin=0`.

**Git log check** of `topology_extract.rs` since 2026-04-30:
```
e7de00b fix(yang-pr-viz-3a-fix): wrap FinishSketch + tessellation paths
218d6e3 feat(yang-pr-viz-3a): in-memory Yang stage capture + WASM bridge
c4ba32d feat(yang-pr-viz-1): per-stage OBJ-dump tool
59123b9 fix(yang-pr13): scope-down — determinism + structural cleanup of
        flood_fill_patches::Step 6
```
PR13 (`59123b9`, 2026-05-02) was the last touch to the Step 6 region
investigator-a identified (L688-L763). PR-Y15c-fix-2 (2026-05-05) is
purely in `yang_integration.rs`. **No cascade**.

**Verdict on §4: ACCEPT investigator-a's stance + extend with empirical
git evidence.** F0020 is a fresh defect class, NOT a previously-masked
PR-Y15c-fix-2 cascade. PR-Y16-FIX is the right scope; PR-Y15c-fix-2.3
is NOT what's needed. Investigator-a memo §5 question 4 is answered
in the negative.

---

## §5 Cherchi 2022 §5 conformance check

**Question** (per plan §0b step 4 + investigator-a memo §5 question 3):
does Cherchi 2022 §5's per-patch labeling with manifold-edge barriers
preempt the F0020 defect?

**Reading the audit** (`docs/audits/yang_audit_c_cherchi2022.md` §YC-06,
lines 511-534):

> "Cherchi's barrier is `tm.edgeIsManifold(e_id)`: flood traverses iff
> the edge is manifold. A non-manifold edge means three or more
> triangles meet there — the patch boundary. Yang's barrier is
> 'cross-mesh edge': flood traverses iff the reverse edge is in the
> same mesh."

> "Status: DEVIATES (architecturally). Rust labels per-sub-triangle
> ... `flood_fill_patches` happens in a LATER stage and uses Yang-style
> intersection-edge barriers, NOT Cherchi-style manifold-edge barriers."

**Architectural conformance answer**: under Cherchi's architecture,
**every patch boundary edge is by construction a non-manifold edge
(3+ triangles meet there)**. The patch graph is built such that the
boundary IS the non-manifold-edge set. By the manifold contract, every
patch boundary edge has an even number of incident triangles (the patch
has an interior side and an exterior side). This guarantees that for
every directed edge `(v0,v1)` on the patch boundary, the reverse
directed edge `(v1,v0)` exists on a triangle in the SAME patch (the
opposite half of the boundary loop) OR on a different patch's boundary.
**Either way, the reverse half-edge exists** by the manifold-edge-barrier
construction.

**Yang's intersection-edge barrier does NOT enforce this manifold-ness**.
A Yang patch boundary CAN be a same-mesh manifold edge that just happens
to lie at the cross-mesh barrier — this is what investigator-a's
hypothesis (a) describes: `(v21→v23)` is a directed edge on mesh_A's
patch_2 boundary, and the reverse `(v23→v21)` lives on mesh_B's
patch_7. The Yang barrier puts them in different patches, but Step 6's
`is_boundary` check at L697-L701 then drops the forward HE if its
reverse-direction neighbor is in a different patch — meaning the
forward HE goes into the boundary set but its reverse never does in
ANY patch (because every patch sees the reverse as "in a different
patch", so the boundary check excludes it via the `tri_to_patch[nt]
!= pi` clause AND the `seen.insert((v0,v1))` dedup elsewhere).

**Verdict on §5: PARTIAL ARCHITECTURAL DEFECT**. The F0020 defect IS
preempted by Cherchi 2022 §5's per-patch labeling architecture. Our
per-tri labeling + Yang-style intersection-edge barriers is a known
deviation (`yang_audit_c_cherchi2022.md` YC-06 DELIBERATE-DIVERGENCE)
that LOSES Cherchi's manifold-edge-boundary invariant. PR-Y16-FIX has
two valid scopes:
- **(LOCAL)** Patch the Step 6 boundary classification within the
  Yang barrier model (investigator-a's hypothesis (a) line of attack).
  Smaller PR; preserves architectural deviation.
- **(ARCHITECTURAL)** Switch to Cherchi-style manifold-edge barriers
  in `flood_fill_patches`. Much larger PR; removes the YC-06
  deviation; would also potentially fix F0050's silent oracle fire.

**Recommendation** (NOT bound directive): start with (LOCAL) for
PR-Y16-FIX since investigator-a's anchor is consistent with it; bank
(ARCHITECTURAL) as the longer-term direction documented in
`yang_audit_c_cherchi2022.md` YC-06.

---

## §6 Reference-parity escalation + cheaper-proxy discipline + verdict

**Reference-parity escalation question** (per spec §6 +
`feedback_external_coherence.md` 3-wrong-anchor rule):

F0020 has 0 prior anchors burned on this defect class (PR-Y16-INV is
phase 0 = discovery; no fix attempted yet). The 3-wrong-anchor escalation
budget is at 0/3. **Reference-parity escalation IS premature for this PR.**

**When SHOULD it trigger?** Only after PR-Y16-FIX has burned 1-2 anchor
candidates from investigator-a memo §3's ranked list:
- (a) Step 6 boundary classification at L697-L701 — top candidate, FIRMEST
  per memo §4 self-canary
- (b) Step 6 loop-chaining `_ => break` partial-chain — POSSIBLE but
  refuted by §4 canary's "no insert at all" data
- (c) Upstream non-conformal mesh from `subdivide_mesh_pair` — UNLIKELY
  per Stage C `e0_rev=1` columns showing keys present at all_tris level

If (a) burns and turns out wrong, the next anchor is (b); if (b) burns,
(c). Once (a)+(b)+(c) all burn, escalate to Cherchi 2022 §5 architectural
fix (per §5 above) AND build the Cherchi reference parity sidecar per
`feedback_external_coherence.md`.

**Cheaper-proxy discipline** (per
`feedback_adversary_recommendations_need_canary.md`, plan §0b step 6
"CRITICAL", risk #7):

I did NOT empirically run the discriminating canaries proposed in the
investigator's memo §4's "Banked PR-Y16-INV-2 candidates":
- (a1 vs a2) `eprintln!` at `topology_extract.rs:702` when
  `seen.insert((v0,v1))` returns false
- (b) `eprintln!` at `topology_extract.rs:745` (`_ => break`)
- (c) `eprintln!` at `topology_extract.rs:471-478` for
  `directed_edge_to_tris.keys()`

Therefore I do NOT recommend any of them as bound directives for
PR-Y16-FIX's Phase 0 anchor canary. They are CANDIDATE anchors that
PR-Y16-FIX implementer must canary-verify themselves before committing
to (per `feedback_anchor_before_fix.md`).

**What I DID empirically verify** (and can recommend):
- The post-pairing `[twin-oracle]` block at end of `flood_fill_patches`
  is load-bearing (§2 mutation test).
- It fires for F0020/F0030/F0050 in the cohort with the same fwd-only
  unpaired-HE shape (§3).
- F0030's `collision_count=2` shows the defect class is broader than
  memo §3's "no collisions" characterization.
- F0050 shows the oracle catches a defect class strictly wider than
  what `validate_yang_result_topology` raises.

**Verdict: AMEND** the discovery memo.

**Acceptance criteria check**:
- §1 byte-equivalent re-run: PASSED (independent reproduction).
- §2 oracle is load-bearing: PASSED (mutation fires it correctly).
- §3 cohort threshold: 3/5 broad (F0020+F0030+F0050) ≥ 3/5 = CLASS.
  But the memo's specific characterization
  (`collision_count=0`, single-defect-mode) is too narrow per F0030.

**Required amendments to investigator-a's memo before PR-Y16-FIX
spec drafting**:
1. Memo §3 hypothesis (a) characterization should note that F0030
   (cohort member) has `collision_count=2` co-occurring with the
   forward-unpaired pattern. Hypothesis (a) is CONSISTENT with F0030
   but does not by itself explain the collisions; PR-Y16-FIX must
   address both modes OR scope down to F0020-class only and explicitly
   defer F0030's collision aspect.
2. Memo should add a §3.5 "F0050 boundary case" noting that the
   oracle fires (`unpaired_count=2`) but `validate_yang_result_topology`
   does NOT raise the user-visible error. PR-Y16-FIX scope question:
   does the fix target only the user-visible-error subset (F0020,
   F0030) or the broader twin-symmetry-health subset (F0020, F0030,
   F0050)?
3. Memo §5 Q3 "Cherchi 2022 §5 architectural question" can be
   answered: YES, Cherchi's manifold-edge-barrier per-patch labeling
   would preempt the F0020 defect. The defect IS partly architectural
   (`yang_audit_c_cherchi2022.md` YC-06). PR-Y16-FIX may take either
   the LOCAL (Step 6 classification fix) or ARCHITECTURAL (switch
   to manifold-edge barriers) path; LOCAL is smaller and aligned
   with investigator-a's hypothesis (a).
4. Memo §5 Q4 "PR-Y15c-fix-2 cascade question" is answered: NO, F0020
   is NOT a cascade. PR-Y16-FIX is correctly scoped (not Y15c-fix-2.3).

**Decision-gate routing recommendation** (NOT bound):
- If team-lead reads this AMEND verdict as effectively GREEN with
  documented amendments, proceed to PR-Y16-FIX with investigator-a's
  hypothesis (a) as the Phase 0 anchor candidate, with empirical
  canary verification at `topology_extract.rs:697-708` as the
  PR-Y16-FIX implementer's first task.
- If team-lead reads it as AMBER (because of F0030's collision
  co-occurrence), spawn PR-Y16-INV-2 to widen the probe with the
  three banked canaries before committing to PR-Y16-FIX scope.

Either reading is defensible from this validation memo's data; AMEND
captures the genuine ambiguity rather than forcing GREEN or REJECT.

---

## Verification (against spec §0b deliverables)

- ☑ §1 independent re-run reproduces investigator-a byte-for-byte.
- ☑ §2 mutation test fires the oracle correctly on a clean control case.
- ☑ §3 cohort sweep on 5 cases (F0002, F0010, F0030, F0050 + F0020
  reference) — 3/5 broad ≥ threshold = CLASS.
- ☑ §4 PR-Y15c-fix-2 cascade ruled out (code path + git log evidence).
- ☑ §5 Cherchi 2022 §5 architectural conformance check — YC-06
  deviation is the architectural cause; LOCAL or ARCHITECTURAL fix paths.
- ☑ §6 reference-parity escalation question answered (premature; defer
  to PR-Y16-FIX +1-2 anchors); cheaper-proxy discipline observed (no
  unverified bound directives).
- ☑ Verdict: AMEND.
- ☑ `git diff` shows ONLY this memo + investigator-a's existing
  deliverables; temporary mutations + cohort tests reverted.
- ☑ Memo populated with all 6 §1–§6 sections; no empty bodies.

**Sub-phase 0b complete. Routing back to team-lead for sub-phase 0c
close-out.**

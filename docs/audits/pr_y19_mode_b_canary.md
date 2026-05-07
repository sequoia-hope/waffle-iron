# PR-Y19-MODE-B sub-phase 0a — canary-runner-6 anchor canary

**Author:** canary-runner-6
**Date:** 2026-05-06
**Scope:** Empirical localization of F0020 Mode B mechanism — B1
(L940 collapse-induced duplicates) vs B2 (L765 cross-patch dedup
failure). Per `feedback_anchor_before_fix.md` +
`feedback_oracle_credibility_via_role_separation.md` +
`feedback_adversary_recommendations_need_canary.md`: empirical probe
before any spec/fix coding.

**Verdict (§2): B2.** F0020's Mode B mechanism is cross-patch
dedup failure. The same canonical directed edge `(v_a → v_b)` is
emitted by multiple patches sourced from DIFFERENT B-Rep faces; the
per-patch `seen: BTreeSet<(usize, usize)>` at `topology_extract.rs:753`
prevents intra-patch duplicates but allows cross-patch duplicates.
B1 (L940 `canon_to_brep` collapse) is **ruled out** — all three
cohort cases show `canon_to_brep_size == unique_positions` (zero
position-aliased canonical indices).

---

## §1 F0020 probe data (failing boolean = Extrude 2)

Command:
```
TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020 --ignored --nocapture --test-threads=1
```

**Probe 1 — L940 `[canon2brep]` mapping integrity:**
```
[canon2brep-summary] canon_to_brep_size=36 unique_positions=36
```
**No B1 collisions.** Every one of the 36 canonical mesh indices
inserted into `canon_to_brep` has a unique geometric position
(rounded to 1e-9 m). No collision detector entries fired. The
`or_insert_with` collapse mechanism at L940-L948 is operating on
**already-distinct** canonical indices with **already-distinct**
geometric positions; it cannot be the source of Mode B duplicates.

**Probe 2 — L1100 `[directed-he]` survey before twin pairing:**
```
[directed-he] total_keys=96 non_singleton_count=10
[directed-he] DUPE key=(VertexIdx(8), VertexIdx(17))   hes=[HE 70, HE 87]
[directed-he] DUPE key=(VertexIdx(9), VertexIdx(8))    hes=[HE 79, HE 86]
[directed-he] DUPE key=(VertexIdx(10), VertexIdx(9))   hes=[HE 78, HE 92]
[directed-he] DUPE key=(VertexIdx(11), VertexIdx(10))  hes=[HE 77, HE 91]
[directed-he] DUPE key=(VertexIdx(12), VertexIdx(11))  hes=[HE 76, HE 90]
[directed-he] DUPE key=(VertexIdx(13), VertexIdx(12))  hes=[HE 75, HE 97]
[directed-he] DUPE key=(VertexIdx(14), VertexIdx(13))  hes=[HE 74, HE 96]
[directed-he] DUPE key=(VertexIdx(15), VertexIdx(14))  hes=[HE 73, HE 103]
[directed-he] DUPE key=(VertexIdx(16), VertexIdx(15))  hes=[HE 72, HE 102]
[directed-he] DUPE key=(VertexIdx(17), VertexIdx(16))  hes=[HE 71, HE 101]
```
10 directed_he keys have 2 HEs each. Geometrically these are the 10
edges along the intersection-curve loop on `mesh_A FaceIdx(3)` —
canonical indices `[21, 22, 23, 24, 25, 26, 27, 28, 29, 30]` mapped
to BrepVIdx `[8, 9, 10, 11, 12, 13, 14, 15, 16, 17]`. The
twin-pairing match arms see fwd_count=1, rev_count=2 → ambiguous;
or fwd_count=1, rev_count=0 → unpaired. (`(8 → 17)` is a forward
that is duplicated; the chain `(9→8), (10→9), …` is reverse-side
duplicated.)

**Probe 3 — L765 `[boundary-dedup]` cross-patch evidence (decisive):**
The same canonical directed edge `(23 → 21)` (canonical mesh
indices, both on `mesh_A FaceIdx(3)`) is emitted by **two distinct
patches**:
```
[boundary-dedup] pi=11 ti=9  v0=23 v1=21 source=SourceFace { mesh_id: A, face_idx: FaceIdx(3) }
[boundary-dedup] pi=13 ti=52 v0=23 v1=21 source=SourceFace { mesh_id: B, face_idx: FaceIdx(2) }
```
And `(21 → 23)`:
```
[boundary-dedup] pi=2  ti=6  v0=21 v1=23 source=SourceFace { mesh_id: A, face_idx: FaceIdx(2) }
[boundary-dedup] pi=3  ti=7  v0=21 v1=23 source=SourceFace { mesh_id: A, face_idx: FaceIdx(3) }
```
Two patches on `mesh_A FaceIdx(2)` and `mesh_A FaceIdx(3)`
both emit `(21 → 23)` as a per-patch boundary; their per-patch
`seen` sets dedup intra-patch but not cross-patch. The intersection
loop is correctly identified as a boundary by both patches (both
sides of the cut see it as their own boundary), and the L765
de-duplication scope is too narrow to catch the cross-patch
collision.

**Final pairing summary:**
```
[topo-extract] summary: paired=39, unpaired=1, ambiguous=9
[twin-oracle] unpaired_count=28, collision_count=1
[A15.6] half_edge[16].twin = 0 but twin.twin = 31 (expected 16)
```
The 9 ambiguous + 1 unpaired correspond exactly to the 10
non-singleton directed_he keys in Probe 2. The 28 unpaired_count
in the post-pairing arena reflects: 9 ambig × 2 reverse + 1 unpaired
fwd + 9 ambig fwd that landed `[]` because their reverse came from
the wrong source = consistent with Mode B alone.

**Strategy 2 retry status (Risk #9 check):** F0020 has
`[yang-diag] intersection optimization: 0 optimized, 20 planar-skip, 0 failed`
in the failing boolean. `update_mesh_along_refined_curves` is
**never called** (all intersections planar-skip → empty
refinement.edges). Risk #9 does NOT apply for F0020: the canary-time
probe state IS the to-be-fixed state.

---

## §2 Mechanism identification

**B2 (cross-patch dedup failure).**

**Empirical justification (load-bearing):**
1. `canon_to_brep_size == unique_positions` (36 == 36) — rules out
   B1's collapse-induced-duplicate hypothesis. Each canonical
   index already has a unique geometric position; the `or_insert_with`
   merely allocates BrepVIdx in 1:1 correspondence.
2. The 10 non-singleton directed_he keys (Probe 2) all stem from
   **two patches emitting the same canonical edge** at L765 (Probe 3
   verifies for a sample). The per-patch `seen` BTreeSet (L753, L765)
   does not deduplicate across patches.
3. The 10 BrepVIdx-keyed dupes match 1:1 to the 10 canonical-mesh
   intersection edges along mesh_A FaceIdx(3)'s cut loop. Geometric
   position never aliases; only the patch-side from which the boundary
   was emitted aliases.
4. The cohort comparison (§3) confirms: F0030 + F0044's flood_fill
   (Strategy-2 retry context for both) show the **identical pattern**:
   `canon_to_brep_size == unique_positions`, non-singleton directed_he
   from cross-patch sources.

**B1 + B2 ruled out** — the two are mutually exclusive (B1 requires
position-aliased canonical indices; F0020/F0030/F0044 all show 0).

---

## §3 Cohort comparison

| Case  | canon_to_brep_size | unique_positions | non_singleton directed_he | unpaired (post-pair) | ambiguous | mechanism |
|-------|--------------------|------------------|---------------------------|----------------------|-----------|-----------|
| F0020 | 36                 | 36               | 10                        | 1                    | 9         | B2        |
| F0030 (Strategy-2 retry) | 29 | 29       | 13                        | 2                    | 11        | B2        |
| F0044 (Strategy-2 retry, boolean #5) | 125 | 125 | 10                  | 31                   | 10        | B2        |

**F0030 provenance trace (decisive):**
The directed-he `(VertexIdx(5) → VertexIdx(6))` has **3 reverse
candidates** from 3 distinct patches:
```
rev_hes=[HE 36 from SourceFace A FaceIdx(0),
         HE 51 from SourceFace B FaceIdx(0),
         HE 71 from SourceFace B FaceIdx(2)]
```
Three different B-Rep faces emit the same canonical edge as a
boundary. This cleanly rules out any per-vertex collapse mechanism
(B1) and points unambiguously at L765's per-patch dedup scope.

**F0044 caveat:** the L1100 directed_he non-singleton count of 10
shows up only in Strategy-2-retry boolean #5 (line 5749 of the log).
Booleans #1–#4 all show `non_singleton_count=0` and pass yang_boolean
cleanly. F0044's user-visible failure is `watertight_mesh: 12
unpaired edges` AFTER the boolean — a downstream defect distinct
from the yang validator panic. Notably, canary-runner-5's memo
described F0044 as "pure Mode A, 36 unpaired, 0 ambiguous" — that
characterization has **shifted**: the current cohort state has F0044
exhibiting B2 in its Strategy-2-retry path (still mixed Mode A + B2,
but B2 dominates). The canary-runner-5 memo stands as historical
record; current empirical state is in this memo.

**R0092** (third case in `spotlight_f0044` batch) is the only case in
that batch that triggers the yang_boolean validator panic
(`half_edge[32].twin = 0 but twin.twin = 31 (expected 32)`).
Investigation of R0092's mechanism is OUT OF SCOPE here (banked).

**Cohort verdict:** all three probed cases match B2 mechanism. None
of them aliases canonical-vertex positions. Mode A (F0044's
canary-runner-5-described "pure Mode A, 36 unpaired") was not
reproduced in the current corpus run at the same severity; it has
been partially folded into the B2 pattern. The PR-Y19-MODE-B fix
should target the cross-patch dedup failure at the L765 region.

---

## §4 Spec scope decision

**L765 region fix only (B2).**

- **In scope for sub-phase 0b spec:** add cross-patch dedup at the
  boundary-collection site. The canonical-edge → patch_index pair
  must be globally tracked, not just per-patch. The intersection
  edge between two B-Rep faces should be emitted **once** (with
  forward direction owned by one face and reverse by the other),
  not twice from each face's patch.
- **Out of scope (banked for PR-Y20+):** the L940 `canon_to_brep`
  collapse — the empirical evidence shows it is operating correctly
  (1:1 in current cohort) and **does not need to be modified**. The
  L940 design intent (geometric-position dedup) is preserved by the
  current 1:1 reality (each unique position already has a unique
  canonical index). NO modification to L940.
- **Out of scope:** Mode A pure-unpaired path (F0044's earlier-state
  characterization). Banked; canary-runner-5's recommendation for a
  Mode-A PR is overtaken by the cohort's drift toward B2.
- **Out of scope:** synthesizing twins in the `[]` / `multiple` arms
  at L1124-L1162 — that would be the synthetic-fill anti-pattern
  forbidden by P9-P10 and `feedback_yang_only.md`.
- **Out of scope:** R0092's yang validator panic (different defect
  shape; may share root cause but cohort sweep needed to confirm).

---

## §5 Self-canaried recommendation for sub-phase 0d implementer

Per `feedback_adversary_recommendations_need_canary.md`: my
recommendations cite empirical observation only.

**Recommended fix shape:**
The current per-patch `seen: BTreeSet<(usize, usize)>` (line 753) is
too narrow. The fix should hoist a **cross-patch directed-edge
ownership map** out of the per-patch loop. Each canonical directed
edge `(v0, v1)` should appear at most once across all patches'
boundary collections:

```rust
// Pseudocode (B2 fix):
let mut global_seen: BTreeSet<(usize, usize)> = BTreeSet::new();
for (pi, patch) in patches.iter().enumerate() {
    let mut boundary: Vec<(usize, usize, bool)> = Vec::new();
    for &ti in &patch.tris {
        for ei in 0..3 {
            let (v0, v1) = (sub.verts[ei], sub.verts[(ei + 1) % 3]);
            let is_boundary = /* same as today */;
            if is_boundary && global_seen.insert((v0, v1)) {
                /* emit (v0, v1, is_int) into THIS patch's boundary */
            }
            // else: another patch already owns this directed edge;
            //       this patch silently skips (it does NOT emit the reverse).
        }
    }
    // Chain into loops as before.
}
```

**Key empirical constraints the fix must satisfy:**
1. The intersection edge between two B-Rep faces is geometrically
   one edge; in canonical-mesh indices it is one directed pair
   `(v0 → v1)` that triangle T_a emits and `(v1 → v0)` that
   triangle T_b emits. **Both directions must remain available**
   for twin pairing — Probe 3 shows the forward direction comes
   from one mesh-A patch and the reverse from a mesh-B patch. The
   fix must NOT dedup both `(v0, v1)` AND `(v1, v0)` to the same
   side; it must keep them as a directed-edge pair (one per side).
2. Empirical cross-validation: after the fix, F0020's
   `[directed-he] non_singleton_count` should be 0; F0030 + F0044
   should also drop to 0 in their respective failing booleans.
3. Loop-chaining at L786-L820 currently consumes `boundary` per
   patch. If the global dedup drops some edges from later patches'
   boundaries, the chaining may fail to close loops on those
   patches. **The implementer MUST re-run the canary diagnostic
   (this same `[directed-he] non_singleton_count` probe) and verify
   it goes to 0 AND that no new unpaired/loop-closure failures
   emerge.** Per `feedback_anchor_before_fix.md`: instrument
   before coding.

**Risk to flag for implementer-w:**
If the fix simply skips the duplicate emission, one patch loses an
edge from its boundary loop, potentially leaving a half-loop. The
L765 boundary edges feed L786's `adj` loop-chaining; an asymmetric
drop could break a loop. The principled fix may need to **route**
the directed edge to the "correct" patch (the one whose face owns
the cosurface side) rather than first-come-first-served. This is a
spec-level question for sub-phase 0b. I **do not recommend** a
specific routing rule without further empirical investigation by
the spec writer.

**Empirical observations NOT yet probed (banked for sub-phase 0d
pre-fix canary):**
- ⚠ Whether the loop-chaining at L786 produces complete closed
  loops on each patch when global dedup is applied. (Probable
  failure mode if both directions are dedup'd to one side.)
- ⚠ Whether F0030's Strategy-2-retry context's mesh non-conformality
  has additional sources beyond cross-patch dedup. F0030 has
  `unpaired_count=2` distinct from the ambiguous chain — those 2
  may be Mode A residual. The fix likely resolves the 11 ambiguous
  but might not resolve the 2 unpaired.
- ⚠ Whether R0092 (in the F0044 batch) shares the B2 mechanism. Not
  probed in this canary. Banked for adversary-19's cohort sweep.

**Strategy 2 retry survival:** F0020 does not enter Strategy 2;
F0030 + F0044 do. The fix at L765 is BEFORE Strategy 2 retry runs
(flood_fill is invoked per-boolean), so retry erasure does not apply
— the fix's effect is contained to one flood_fill invocation, and
each retry runs a fresh flood_fill with the fix in place. Risk #9
mitigated.

---

## Verification

- `git diff --stat` shows ONLY this file (`docs/audits/pr_y19_mode_b_canary.md`).
- `cargo build -p kernel` clean post-revert (55 pre-existing warnings unchanged).
- §1 has F0020's L765 + L940 + L1100 probe data with concrete
  numbers.
- §2 picks ONE mechanism (B2). NO empty bodies.
- §3 has cohort table with concrete numbers + provenance evidence
  for F0030.
- §4 picks ONE spec scope decision (L765 region fix only).
- §5 self-canaried per `feedback_adversary_recommendations_need_canary.md`:
  recommendations cite Probe 1–3 directly; explicitly flags the
  loop-chaining risk + banked questions for sub-phase 0d's
  pre-implementation canary.
- NO production code changes (probes reverted; verified clean).
- NO recommendation for synthetic fill / fallback paths per
  `feedback_yang_only.md`.

**Sub-phase 0a complete. Routing to spec-writer-s for sub-phase 0b.**

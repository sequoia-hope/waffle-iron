# PR-Y23 Anchor Canary — H1' confirmed: open-loop wrap-back next-pointer

**Author:** canary-z23
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0
**Verdict:** **H1' confirmed** — open-loop emitted from L961 produces a circular
`next`-ring in Step 7 whose wrap-back next-pointer manufactures the phantom
`(38→27)` reverse direction the validator panics on.

This memo names the empirical anchor only. It does NOT propose a fix; that is
`impl-z23`'s job.

---

## §0 Discipline — live tree untouched

### Live tree at session start

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

### Live tree just before writing this memo

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

All probe instrumentation was applied inside a separate worktree:

```
$ git worktree add /tmp/y23-probe-wt HEAD
Preparing worktree (detached HEAD 8de94e5)
HEAD is now at 8de94e5 feat(yang-pr-y22-recovery): F0020 Mode A MISSING residual GREEN | M1 NMM-incidence + M2 canon-degen filter

$ cd /tmp/y23-probe-wt && git diff --stat
 crates/kernel/src/boolean/topology_extract.rs | 123 +++++++++++++++++++++++++-
 1 file changed, 122 insertions(+), 1 deletion(-)
```

No `git stash`, `git checkout --`, `git reset --hard`, or any other destructive
op was used on the live working tree. Per
`feedback_adversary_no_destructive_git.md`.

### Probe gate

Every probe is gated on `std::env::var("Y23_PROBE").as_deref() == Ok("1")` — when
unset the codepath is byte-identical to PR-Y22 baseline. Confirmed by reading
the diff: the only mutation outside the gate is the `he_construct_keys` parallel
`Vec` push at the same site as `directed_he.entry(...).push(he_idx)`. That push
runs unconditionally so the indices stay aligned, but it allocates O(n_he) once
and is never read unless `y23_probe`. The vec is dropped at function exit.

### Reproduction command

```
cd /tmp/y23-probe-wt
YANG_BOOLEAN=1 Y23_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y23-probe-output.txt 2>&1
```

Exit 0 (the test panics inside an inner asserts but the spotlight test wraps it,
so the outer test result is "ok"). The case status itself is `Failed` —
expected for F0020 at PR-Y22 baseline.

---

## §1 Multi-stage probe results — F0020 boolean #2 only

The first F0020 boolean (Extrude #1) is clean: `[topo-extract] summary:
paired=48, unpaired=0, ambiguous=0`, `[twin-oracle] unpaired_count=0`. All H1'
analysis below is on the **second** flood_fill_patches invocation, the one
that reproduces the validator panic.

### P0 — Step 6 boundary emission (`topology_extract.rs:866-898`)

The R3 ownership pre-pass (built at L810-863) blocks patch 7 from emitting the
closing edge `(10→11)`:

```
[y23-probe-p0] patch=6 sf=midA_face4 dir=(10,11) is_boundary=true owner_blocks=false seen_inserted=true
[y23-probe-p0] patch=7 sf=midA_face4 dir=(10,11) is_boundary=true owner_blocks=true seen_inserted=false
[y23-probe-p0] patch=7 sf=midA_face4 dir=(11,12) is_boundary=true owner_blocks=false seen_inserted=true
[y23-probe-p0] patch=7 sf=midA_face4 dir=(12,10) is_boundary=true owner_blocks=false seen_inserted=true
```

Patches 6 and 7 are both from `midA_face4`. R3's lex tie-break awards `(10→11)`
to patch 6 (smaller mesh_id/face_idx/patch_index). Patch 7 emits only `(11→12)`
and `(12→10)` — which is an open path `11 → 12 → 10`, not a closed loop. There
is no other patch in the invocation that emits any of `(11,12)`, `(12,10)`, or
`(11,10)` either to balance, so this asymmetry is final.

### P1 — Loop-chaining at `topology_extract.rs:961`

```
[y23-probe-p1] patch=7 loop_idx=0 chain_len=2 first_v0=11 last_v1=10 closed=false chain=[(11, 12, false), (12, 10, false)]
```

`closed = first_v0 == last_v1`. Here `11 != 10` → **closed=false**. Per the
soft-break documented at L933-947 (PR-Y19-MODE-B banked residual), the chain is
emitted via `loops.push(chain)` at L961 anyway. This is the chain that becomes
face=7, loop=7, n=2 in Step 7.

### P2 — Step 7 arena push at `topology_extract.rs:1131-1152`

```
[y23-probe-p2] he_idx=58 face=7 loop=7 i=0 n=2 v0_canon=11 v1_canon=12 v0_brep=27 v1_brep=38 next_idx=59 prev_idx=59
[y23-probe-p2] he_idx=59 face=7 loop=7 i=1 n=2 v0_canon=12 v1_canon=10 v0_brep=38 v1_brep=26 next_idx=58 prev_idx=58
```

`next_idx = HalfEdgeIdx(he_base.0 + (i + 1) % n)`. With `n=2`:

- HE 58 (i=0): `next_idx = 58 + 1 = 59` ✓ goes to HE 59 whose origin is BV38 — agrees with HE 58's declared dest.
- **HE 59 (i=1): `next_idx = 58 + 0 = 58`** ← wraps back to HE 58 whose origin is BV27 — does NOT agree with HE 59's declared dest BV26. **This is the wrap-back.**

`directed_he.entry((v0_brep, v1_brep)).push(he_idx)` is keyed by construction-time
B-Rep ids:

- HE 58 → directed_he[`(BV27, BV38)`] = [HE 58]
- HE 59 → directed_he[`(BV38, BV26)`] = [HE 59]

Critically, `directed_he` does NOT contain a `(BV38, BV27)` key — the canonical
reverse of HE 58. (This is consistent with P0: only patch 6 emitted the
`(10,11)` direction, and patch 6's tris index it differently.)

### P3 — Post-pairing arena audit, after `topology_extract.rs:1380`

```
[y23-probe-p3] he=58 origin_brep=27 ck0=27 ck1=38 constructed_dest_brep=38 arena_traversal_dest_brep=38 traversal_matches_construction=true  key_in_directed_he=true rev_key_in_directed_he=false
[y23-probe-p3] he=59 origin_brep=38 ck0=38 ck1=26 constructed_dest_brep=26 arena_traversal_dest_brep=27 traversal_matches_construction=false key_in_directed_he=true rev_key_in_directed_he=false
```

The H1' signature is right here, on **HE 59**:

- `constructed_dest_brep = 26` (the B-Rep id of `v1_canon=10`)
- `arena_traversal_dest_brep = 27` (because `next_idx=58` and `arena.half_edges[58].origin = BV27`)
- `traversal_matches_construction = false`

For HE 58 the two agree (no wrap-back at i=0 of an open chain). For HE 59 the
wrap-back manufactures a phantom directed edge `(BV38 → BV27)` in the arena's
*traversal* topology that has no construction-time counterpart in `directed_he`.

### P4 — `[twin-oracle]` reverse-existence at `topology_extract.rs:1463-1469`

```
[y23-probe-p4] he=58 origin=27 traversal_dest=38 constructed_dest=38 ck0=27 ck1=38 key_in_directed_he=true rev_constructed_in_directed_he=false rev_traversal_in_arena=true
[y23-probe-p4] he=59 origin=38 traversal_dest=27 constructed_dest=26 ck0=38 ck1=26 key_in_directed_he=true rev_constructed_in_directed_he=false rev_traversal_in_arena=true
```

`rev_constructed_in_directed_he` (build-time view): false for both HEs — neither
`(BV38,BV27)` nor `(BV26,BV38)` was inserted into `directed_he`.

`rev_traversal_in_arena` (oracle's view via `arena_dir_edges` built at
L1445-1449 from `(he.origin.0, arena.half_edges[he.next.0].origin.0)`): true
for both — because:

- HE 58's traversal-derived directed edge is `(27 → 38)`. The oracle therefore
  also computes `(38 → 27)` from HE 59 (since HE 59's traversal yields
  `(38 → 27)`, the wrap-back). Each one looks like "the reverse exists" to
  the other.
- HE 59's traversal yields `(38 → 27)`, and the reverse `(27 → 38)` is HE 58.

Both HEs see the other's wrap-back-traversal edge as their reverse, neither has
a construction-time twin, and both end up with `twin = None`. The
[twin-oracle] L1466 `if rev_present` branch counts both as unpaired
(`unpaired_count=2`), and the downstream
`yang_integration::validate_yang_result_topology` panics on HE 58 with the
"reverse direction (38→27)" message — that "(38→27)" is **HE 59's wrap-back
traversal edge**, not a real third HE.

### Twin-oracle confirmation

```
[topo-extract] summary: paired=65, unpaired=0, ambiguous=0
[twin-oracle] total_directed_edges=169
[twin-oracle] unpaired_count=2
[twin-oracle] offender he=58 twin=-3 twin.twin=-3 origin=v27(-2.749189e-1,9.921157e-2,1.052632e-1) dest=v38(-2.749189e-1,9.921157e-2,5.152014e-2)
[twin-oracle] offender he=59 twin=-3 twin.twin=-3 origin=v38(-2.749189e-1,9.921157e-2,5.152014e-2) dest=v27(-2.749189e-1,9.921157e-2,1.052632e-1)
```

The oracle reports `dest=v27` for HE 59 because it computes `dest` via
`arena.half_edges[he.next.0].origin` (L1515) — the same arena-traversal logic
that P3 / P4 captured. **Construction-time, HE 59's destination was BV26, not
BV27.** The dest=v27 in the oracle output is itself a symptom of H1'.

---

## §2 Hypothesis decision table — only H1' fits

| Hypothesis | P1 prediction | P3 prediction | P3/P4 observation | Verdict |
|---|---|---|---|---|
| **H1'** | At least one chain `closed=false` containing HE 58 / HE 59 lifecycle | constructed_dest != arena_traversal_dest for at least one HE | **patch=7 chain_len=2 closed=false** for the chain that becomes face=7. **HE 59: traversal_matches_construction=false (constructed=26, traversal=27)** | **CONFIRMED** |
| H2 (keying mismatch) | All chains closed | HE 58 in directed_he at unexpected key | `key_in_directed_he=true` for HE 58 at constructed key `(BV27,BV38)` — no asymmetric canonicalization detected | refuted |
| H3 (M1 over-classification) | All chains closed | HE 58 in fwd_hes correctly; rev_hes already-paired before iteration | Pairing summary: `paired=65, unpaired=0, ambiguous=0`. M1 NMM-classification didn't fire (per PR-Y22 v2 §1 + this canary). The 2-residual is born downstream of M1, not from M1 over-classifying. | refuted |
| H4 (post-pairing modification) | normal | `paired_he` shows HE 58 was paired then unset | P3 readback after L1380 shows HE 58 / HE 59 still have `twin=None` (consistent with never-paired, not paired-then-unset). No write-site between L1346 and L1452 detected. | refuted |

H1' is uniquely supported. Patch 7's open chain (P1: closed=false), born from
R3 ownership stripping the `(10,11)` direction (P0), wraps back through Step 7's
circular `next`-ring (P2: HE 59.next=58), produces a phantom `(38→27)` arena
traversal edge (P3: traversal_matches_construction=false), is observed by the
[twin-oracle] as a `rev_present=true` orphan (P4), and propagates as the
validator's `(38, 27)` MISSING-defect panic.

---

## §3 Mechanism line range

The defect spans three layers; PR-Y23's fix anchor lies at the **first**
layer below.

| # | Layer | File:lines | Role in the defect |
|---|---|---|---|
| 1 | Loop-chaining open-chain emission (PR-Y23 ANCHOR) | `crates/kernel/src/boolean/topology_extract.rs:913-963` | `loops.push(chain)` at L961 emits the open chain unconditionally. The soft-break at L948-L958 (PR-Y19-MODE-B residual, documented at L933-947) is functioning correctly *for that PR's contract* — it allows the chain to drop instead of panicking. The closure check is missing at L961. |
| 2 | Step 7 circular `next`-ring | `crates/kernel/src/boolean/topology_extract.rs:1131-1146` | `next_idx = HalfEdgeIdx(he_base.0 + (i+1) % n)` builds a circular ring of `next` pointers. For an open chain, this means the last HE wraps back to the first HE's origin — manufacturing the phantom traversal-direction reverse. (Symptom site, not anchor: the wrap is correct for closed loops; fixing layer 2 would break legitimate closed loops.) |
| 3 | `[twin-oracle]` keys on arena traversal | `crates/kernel/src/boolean/topology_extract.rs:1445-1449` | `arena_dir_edges.insert((he.origin.0, arena.half_edges[he.next.0].origin.0))` keys the orphan-detection set on arena traversal, which sees the layer 2 wrap-back as a real reverse direction. (Diagnostic site, not anchor: the oracle is correctly reporting what the arena says; the bug is upstream.) |

**Recommended PR-Y23 anchor: layer 1 (`topology_extract.rs:913-963`).** Layer 2
and layer 3 are downstream consumers of the open-chain artifact; closing the
loop or refusing to emit it removes the artifact at its source. Per CLAUDE.md
"Fix It Right or Don't Fix It (P9-P10)", the fix should not change Step 7's
circular-ring policy for legitimate closed loops nor weaken the [twin-oracle]'s
arena-traversal keying — both are correct on closed inputs.

The plan §"If H1' confirmed" already sketches the patch shape (closure check
before `loops.push`). That sketch is one workable fix; whether it is the
correct minimal fix per Yang §3 + Cherchi 2022 §3 (every patch boundary must
be a closed loop) is for `spec-z23` and `impl-z23` to decide.

---

## §4 Banked findings — observations not load-bearing for H1'

These are non-anchor observations that may matter for follow-on PRs:

1. **Two F0020 booleans, only the second has the residual.** Boolean #1 is
   clean (`[twin-oracle] unpaired_count=0`). The R3 ownership routing doesn't
   produce open chains in every invocation; an obvious correctness invariant
   for the spec is that R3's lex tie-break must not strip a direction from a
   patch unless that patch's loop can still close via other emitted edges.
   This is a stronger contract than what the soft-break currently provides.

2. **Patches 6 and 7 share the same SourceFace (`midA_face4`).** R3's
   tie-break by `(mesh_id, face_idx, pi)` is a fallback because there is no
   topological discriminator left at this point — both patches sit on the same
   B-Rep face. If patch segmentation upstream had not split `midA_face4` into
   patches 6 and 7 in a way that puts a 2-edge sliver in patch 7, this defect
   would not exist. Possibly a follow-on for the per-patch flood-fill seeding.

3. **HE 59's `prev_idx = 58` and HE 58's `prev_idx = 59`** — the prev-pointer
   ring is also circular for n=2, but the validator complaint and the
   [twin-oracle] both consult `next.origin` not `prev.origin`. Symmetric in
   principle; not load-bearing for the chosen anchor.

4. **Many other patches in F0020 boolean #2 also produce open chains** (P1
   shows patches 13, 14, 15, 16, 10, 11, 12, 22, 23, etc. with `closed=false`).
   The 2-residual at the [twin-oracle] is specifically due to patch 7's n=2
   open chain because n=2 wraps the next-pointer between exactly 2 HEs (the
   wrap-back manufactures a 2-cycle pseudo-pair). Higher-n open chains may
   also contribute orphans but their wrap-backs land at vertices that don't
   coincide with any other open chain's first vertex, so they show up as
   "no reverse in arena → legitimate-NMM" rather than "reverse in arena →
   missing-defect". Checking whether F0020's other open chains contribute to
   the 39/169 `[yang-diag]` unpaired count vs the 2/169 `[twin-oracle]`
   unpaired count is left for the implementer; the H1' anchor naming itself
   does not depend on it.

5. **Probe code overhead is bounded.** P3 and P4 each iterate `arena.half_edges`
   once; P0/P1/P2 emit O(boundary) and O(loop_count) lines respectively.
   Total probe output for the F0020 spotlight: 1494 `[y23-probe-p*]` lines,
   2706 lines total file. Probes are fast and gated.

---

## §5 Final-report block

### Probe diff (worktree only — NOT committed)

```
$ cd /tmp/y23-probe-wt && git diff --stat
 crates/kernel/src/boolean/topology_extract.rs | 123 +++++++++++++++++++++++++-
 1 file changed, 122 insertions(+), 1 deletion(-)
```

The probe scaffolding will be discarded by `git worktree remove
/tmp/y23-probe-wt` (or left in /tmp until close-out). Per plan §"Phase 0":
"Probe code lives only in the worktree. Final memo can quote the patch text
but the patch is NOT committed."

### Live tree status at memo write

```
$ cd /home/claude/workspace && git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

Identical to start-of-session status. No live-tree mutation occurred during
canary work.

### Probe output artifact

`/tmp/y23-probe-output.txt` — 2706 lines, retained for follow-up phases.

---

## §6 Routing

- **Hypothesis selected:** H1' (open-loop wrap-back next-pointer).
- **Anchor:** `crates/kernel/src/boolean/topology_extract.rs:913-963` (layer 1).
- **Next agent:** `spec-z23` (do NOT spawn until team-lead confirms).
- **No fix proposed here.** Per `feedback_anchor_before_fix.md`: canary names
  WHERE; spec/impl decide WHAT.
- **Spec phase MUST cite both papers per §3 of plan:** Yang 2025 §3 ("each edge
  shared by two adjacent faces" — the closed-loop contract for B-Rep face
  boundaries) and Cherchi 2022 §3 ("surface patches are bounded by closed loops
  of non-manifold edges, namely the intersection lines" — the closed-loop
  contract for arrangement output). Both papers establish that patch
  boundaries are closed loops; an open chain at L961 violates this invariant.

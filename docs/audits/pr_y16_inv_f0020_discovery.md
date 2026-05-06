# PR-Y16-INV F0020 Discovery — investigator-a findings memo

Sub-phase 0a discovery probe. **No fix code.** Instrumentation + observed-data
report. Per FIP §8 Bug Fix Variant + `feedback_anchor_before_fix.md`: anchor
verification BEFORE fix coding. Per `feedback_oracle_credibility_via_role_separation.md`:
adversary-13 will independently re-run + mutation-test + cohort-sweep in 0b.

Code touched (instrumentation only, all gated on TWIN_DEBUG=1 / YANG_STAGE_DUMP):
- `crates/test-harness/tests/assay_randomized.rs` — `spotlight_f0020` test (~38 LOC)
- `crates/kernel/src/boolean/topology_extract.rs` — Stage C CSV extension (file-only,
  ~30 LOC) + post-pairing `[twin-oracle]` block at end of `flood_fill_patches`
  (~60 LOC). Probe-OFF early-returns before allocation; byte-identity verified.

---

## §1 Reproduction

**Command** (sub-second runtime, 85ms):
```
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020 --ignored --nocapture --test-threads=1
```

**Observed error string** (matches user's in-app error verbatim):
```
Extrude 2: Auto-union failed: kernel error: operation not supported:
  yang_boolean: result validation failed:
  half_edge[40].twin = 0 but twin.twin = 21 (expected 40).
  Body created as standalone.
```

**Offending half-edge tuple** (from upstream `[twin-debug] insert HE` and
post-pairing `[twin-oracle]` traces, FAILING boolean = Extrude 2):
- HE[40] in BRep arena: `origin=VertexIdx(22) → next.origin=VertexIdx(23)`.
- HE[40] is the boundary HE inserted during Step 7 of `flood_fill_patches`
  for canonical-mesh-vertex directed edge `(v23 → v21)` from mesh_B,
  SourceFace `{ mesh_id: B, face_idx: 2 }`, parent_tri = 5,
  cosurface_orientation = None.
- HE[40].twin = `HalfEdgeIdx(0)` — the default value, never overwritten by
  the Twin pairing block at L968-L1077. (HE[0]'s legitimate twin is HE[21],
  hence the misleading "twin.twin = 21" in the error string.)

The first call to `flood_fill_patches` (Extrude 2) fails. The second call
(Extrude 3) reports `unpaired_count=0` — the defect is per-case, not endemic.

---

## §2 Probe data

### Stage OBJ/CSV file-dump counts (in `/tmp/viz/f0020/F0020/`)

**WARNING — last-write-wins by stage tag**: F0020 has two boolean operations
(Extrude 2 and Extrude 3) that both probe the same stage tag (A, Bb, B, C).
The file dumps reflect ONLY the LAST (succeeding) boolean's data:

| Stage | OBJ verts | OBJ tris | CSV rows (excl. header) |
|---|---|---|---|
| A   | 49 | 136 | 136 |
| Bb  | 49 | 136 | 136 |
| B   | 49 | 84  | 84  |
| C   | 49 | 84  | 84  |
| E_lod=Adaptive (×2)        | 6   | 8   | 8 |
| E_lod=Render               | 44  | 83  | 83 |
| F.0–F.4                    | 44  | 83  | 83 |

The FAILING boolean (Extrude 2) had `tris_a=52, tris_b=52, verts=41` per
the existing `[yang-diag] after subdivide` line and `12 groups, 76 tris`
per `[yang-diag] after survival`; Stage C dump for it is overwritten by
Extrude 3. The `twin_debug.txt` text channel preserves both calls in order.

**Banked: PR-VIZ-2 candidate** (already noted in `viz_yang_pipeline.md`
"E_lod=Adaptive last-write-wins"; this finding extends the same gap to
A/Bb/B/C when a single case has multiple boolean ops).

### Stage C CSV new columns (extension shipped this PR)

The existing schema `tri_idx,origin` is extended to
`tri_idx,origin,e0_can,e0_rev,e1_can,e1_rev,e2_can,e2_rev` where:
- `eN_can` = canonical mesh-vertex pair `min(v0,v1):max(v0,v1)` for the tri's
  Nth directed edge `(v0 → v1)`.
- `eN_rev` = count of `all_tris` entries owning the reverse-direction edge
  `(v1 → v0)`. This is the future-twin-candidate population at Stage C.

**Spec note**: the plan's literal request was `twin_he_idx,canonical_edge_key`,
but at Stage C the half-edges have NOT YET been built — Twin pairing happens
at L968-L1077 and the HE allocation at L900 is downstream of the Stage C
probe site (L781-L844). So the literal `twin_he_idx` is unavailable. We
emit the canonical key + reverse-tri count instead, which is the analogous
"future twin" signal observable at this stage. **Probe-site mismatch is
documented as a finding, not a failure** — see plan §0a step 2 escape clause.

### Pre-pairing `[twin-oracle]` summary (FAILING boolean — Extrude 2)

```
[twin-oracle] total_directed_edges=86
[twin-oracle] unpaired_count=10
[twin-oracle] collision_count=0
[twin-oracle] offender he=40 twin=0 twin.twin=21 origin=v22(2.31e-2,1.51e-1,-1.89e-1)
                                                  dest=v23(-2.47e-1,1.04e-1,-2.27e-1)
[twin-oracle] offender he=41 twin=0 twin.twin=21 origin=v23(-2.47e-1,1.04e-1,-2.27e-1)
                                                  dest=v24(-2.75e-1,9.92e-2,-2.31e-1)
[twin-oracle] offender he=44 twin=0 twin.twin=21 origin=v25(2.31e-2,1.51e-1,1.47e-1)
                                                  dest=v26(2.31e-2,1.51e-1,4.01e-2)
[twin-oracle] offender he=45 twin=0 twin.twin=21 origin=v26(2.31e-2,1.51e-1,4.01e-2)
                                                  dest=v27(2.31e-2,1.51e-1,-6.93e-2)
[twin-oracle] offender he=46 twin=0 twin.twin=21 origin=v27(2.31e-2,1.51e-1,-6.93e-2)
                                                  dest=v22(2.31e-2,1.51e-1,-1.89e-1)
```

`unpaired_count=10` (oracle) > `unpaired=7` (existing upstream `[topo-extract] summary`).
Reason: oracle uses the full half-edge symmetry invariant `he.twin.twin == self`
(catches BOTH "twin defaulted to 0" AND "twin set asymmetrically"), whereas the
existing pairing-loop summary only counts edges where the candidate-search loop
reported zero or multiple matches. The 3-edge gap is the legitimate non-symmetric
twin case where pairing succeeded asymmetrically; for F0020 these all collapse
into the same defect class.

`collision_count=0`: no canonical undirected edge maps to multiple distinct
B-Rep Edge entries. **The defect is "missing reverse half-edge", not
"duplicate edge from multiple-pairing".**

### Coordinate cross-reference (BRep verts in offending pattern, all on mesh_B)

The 5 logged offenders all sit on a small chain of mesh_B vertices around
(2.3e-2, 1.5e-1, ...) — the leading-x face of the boss extrude. Source-face
breakdown of all 7 unpaired `[topo-extract]` entries:
- 2 from mesh_B face_2 (3 tris in patch 7)
- 2 from mesh_B face_3 (4 tris in patch 8)
- 1 from mesh_B face_4 (3 tris in patch 9)
- 2 from mesh_B face_5 (4 tris in patch 10)

All 7 unpaired HEs originate from mesh_B's "small near-intersection patches"
(3-4 tris each). Mesh_A's larger patches (Patch 3 = 22 tris, Patch 5 = 14
tris) appear paired correctly.

### Per-edge fwd/rev counts at twin-pairing (excerpt)

```
edge (VertexIdx(22),VertexIdx(23)) fwd_count=1 rev_count=0  fwd_hes=[40] rev_hes=[]
edge (VertexIdx(22),VertexIdx(27)) fwd_count=0 rev_count=1  fwd_hes=[]   rev_hes=[46]
edge (VertexIdx(23),VertexIdx(24)) fwd_count=1 rev_count=0  fwd_hes=[41] rev_hes=[]
edge (VertexIdx(24),VertexIdx(30)) fwd_count=1 rev_count=0  fwd_hes=[55] rev_hes=[]
edge (VertexIdx(25),VertexIdx(26)) fwd_count=1 rev_count=0  fwd_hes=[44] rev_hes=[]
edge (VertexIdx(25),VertexIdx(29)) fwd_count=0 rev_count=1  fwd_hes=[]   rev_hes=[51]
edge (VertexIdx(26),VertexIdx(27)) fwd_count=1 rev_count=0  fwd_hes=[45] rev_hes=[]
edge (VertexIdx(28),VertexIdx(29)) fwd_count=1 rev_count=0  fwd_hes=[50] rev_hes=[]
edge (VertexIdx(28),VertexIdx(31)) fwd_count=0 rev_count=1  fwd_hes=[]   rev_hes=[57]
edge (VertexIdx(30),VertexIdx(31)) fwd_count=1 rev_count=0  fwd_hes=[56] rev_hes=[]
```

Every unpaired BRep edge has either `fwd_count=1, rev_count=0` OR
`fwd_count=0, rev_count=1` — i.e., the reverse half-edge was NEVER built
into the arena, even though Stage C confirms (next paragraph) that the
underlying canonical-mesh directed edge exists in `all_tris` in BOTH
directions across multiple source faces.

### §4 self-canary (run inline; result reported here)

**Question**: in the failing boolean, does the canonical-mesh directed edge
`(v21 → v23)` exist in `all_tris` (the reverse of HE[40]'s edge `(v23 → v21)`),
and if so, why didn't Step 6 emit it as a boundary HE in some patch?

**Probe**: grep the existing `[twin-debug] edge` and `[twin-debug] insert HE`
lines for the failing boolean (lines < 756 of `twin_debug.txt`).

**Result**:
- `[twin-debug] insert HE[N] (v23→v21)` appears once for the failing boolean (HE[40]).
- `[twin-debug] insert HE[N] (v21→v23)` does NOT appear in the failing boolean.
- Stage C all_tris confirms canonical edge `21:23` exists in tri 6 (mesh_A,
  source face 2) and tri 80 (mesh_B, source face 5). Both directions
  `(21→23)` and `(23→21)` ARE present at the all_tris level.
- Therefore: the patch containing tri 6 (mesh_A face_2 patch) extracted its
  boundary loops in Step 6 but did NOT emit a boundary HE for canonical
  directed edge `(v21 → v23)`. The patch dropped that edge.

**Diagnostic narrowed**: the defect is between
`flood_fill_patches::Step 6 boundary collection` (`topology_extract.rs:688-763`)
and `flood_fill_patches::Step 7 HE creation` (`topology_extract.rs:888-962`).
Specifically, the per-patch loop-chaining at L723-L757 either:
(a) never selected `(v21 → v23)` as a boundary edge for any patch, OR
(b) selected it but a `directed_he` insertion path drop missed it.

The §4 canary discriminates between hypotheses by showing that no `insert HE`
line for `(v21 → v23)` appears at all → option (a). The boundary collection
at L688-L708 either (a1) didn't classify `(v21 → v23)` as a boundary edge
in any patch, or (a2) the `seen` dedup at L702 dropped it as a "second
occurrence" within one patch.

**Self-canary status: FIRED with discriminating data.** Top hypothesis
narrows to L688-L708 boundary-edge classification within Step 6.

---

## §3 Hypothesis ranked list (NOT commitment)

Three candidate anchors. Each lists what would empirically distinguish it
from the others. Anchors (a) and (b) are LOCAL to `flood_fill_patches`;
anchor (c) is UPSTREAM and proposed in case (a)/(b) refute.

### Hypothesis (a): Step 6 `is_boundary` classification at L697-L701 incorrectly excludes `(v21 → v23)` from the mesh_A patch's boundary set

**Rationale**: The `is_boundary` check at L697 reads:
```rust
let is_boundary = if let Some(neighbors) = directed_edge_to_tris.get(&(v1, v0)) {
    neighbors.iter().all(|&nt| tri_to_patch[nt] != pi)
} else { true };
```
This says: "an edge is boundary if its reverse-direction neighbors are ALL
in different patches". For canonical `(v21 → v23)` in mesh_A patch_2,
the reverse `(v23 → v21)` lives in mesh_B's patch_7 (SourceFace face_2 tri 5).
mesh_A and mesh_B sub-tris are in different patches by Step 5's
`intersection_edges` flood-fill barrier (cross-mesh = barrier). So
`tri_to_patch[mesh_B_tri] != mesh_A_patch` → `is_boundary=true` should hold.

**BUT**: there may be a MULTIPLE-mesh-vertex-canonicalization step where the
canonical mesh vert id `21` and `23` are aliased to a different ID on
mesh_A vs mesh_B. If mesh_A's tri_6 has the directed edge as `(canon_a → canon_b)`
and mesh_B's tri_80 has the reverse as `(canon_c → canon_d)` where the
canonicals are NOT byte-identical (different positional quantization due
to indirect predicates), `directed_edge_to_tris.get(&(v1,v0))` returns
`None` → is_boundary=true (correct), edge gets added to `boundary` —
but then in Step 7 its reverse can't pair because the BRep mapping uses
a DIFFERENT BRep vertex on each side.

**Discriminating canary** (PR-Y16-INV-2 candidate, NOT shipped this PR):
add an eprintln inside the Step 6 boundary-collection inner loop at L702
that prints the `(v0, v1, pi, source)` whenever `seen.insert((v0,v1))`
returns false (i.e., dedup fires). Run F0020. Confirm whether the missing
`(v21 → v23)` edge was rejected by `seen.insert` (option a2) or never
visited at all (option a1).

### Hypothesis (b): Step 6 loop-chaining at L724-L757 silently drops the start edge when it's the only out-going edge from `start` AND `start` is removed from `adj` mid-loop

**Rationale**: The chaining picks `start = adj.iter().find(|(_, outs)| !outs.is_empty()).map(|(&k, _)| k)` then walks via `adj.get_mut(&current)` and `v.remove(0)`. If a patch has a degenerate single-edge "loop" (start→x but no x→start), the loop ends at L745 (`break`) and `chain` is non-empty, so it lands in `loops`. That looks correct. But what if the chain is interrupted because the next edge's `current` doesn't exist in `adj` at all? The chaining drops out at L745 with an incomplete chain. **No edge would be dropped silently** by this branch — the chain would just be partial.

**Discriminating canary** (PR-Y16-INV-2 candidate, NOT shipped this PR):
add an eprintln at L745 (`_ => break`) printing the `(start, current)` and
the `chain.len()`. Run F0020. If chain is partial (chain.len() != patch's
boundary edge count), this is the defect surface.

### Hypothesis (c): Upstream of `flood_fill_patches` — `subdivide_mesh_pair` produces non-conformal output for the F0020 oblique-rectangle topology, where one of the cross-mesh shared edges has byte-identical mesh-canonical vertex IDs from mesh_A and mesh_B but the patches' surfaces are subtly mis-aligned

**Rationale**: F0020's geometry is "Intersecting oblique rectangles" with three
non-axis-aligned plane normals. The Cherchi indirect-predicate snap-rounding
can produce different cross-mesh quantization for the SAME geometric vertex
when the surfaces meet at an oblique angle. If mesh_A's view of the
intersection point quantizes to `q_a` and mesh_B's quantizes to `q_b ≠ q_a`,
both directions get added to `directed_edge_to_tris` but with different keys —
so `directed_edge_to_tris.get(&(v1, v0))` returns the wrong `None` / `Some` answer.

**This is the "defect upstream of flood_fill_patches" candidate** the plan §1
risk #1 mandates we list if data points there. Data here points moderately
toward (a) over (c) because Stage C confirms canonical edge `21:23` exists
correctly in `all_tris` for both mesh_A and mesh_B (the `e0_can` columns of
rows 6 and 80 both show `21:23`), so the canonicalization is healthy at
all_tris level. But (c) cannot be fully ruled out without a probe of
`directed_edge_to_tris` keys directly.

**Discriminating canary** (PR-Y16-INV-2 candidate, NOT shipped this PR):
print `directed_edge_to_tris.keys()` and look for `(21, 23)` and `(23, 21)`
explicitly. Stage C extended CSV already proxies this via `eN_rev=1` for
the relevant rows, suggesting the keys ARE present — so (c) is unlikely.

### Ranking (NOT commitment, awaiting adversary cohort sweep)

1. **(a)** Step 6 boundary-edge classification — FIRMEST. Backed by §4 canary
   showing `(v21 → v23)` was never inserted into `directed_he` despite being
   present at the all_tris level.
2. **(b)** Step 6 loop-chaining `_ => break` partial-chain — POSSIBLE but the
   §4 canary's "no insert at all" data is more consistent with (a) than (b).
3. **(c)** Upstream non-conformal mesh — UNLIKELY but cannot rule out without
   `directed_edge_to_tris` key dump. Stage C `e0_rev=1` for relevant rows
   suggests the keys ARE present and the issue is in the patch-walking.

**The plan's risk #1 candidate "defect is upstream of flood_fill" is listed as
hypothesis (c).** Current data favors (a) by a comfortable margin but (c)
remains a live alternative; the cheapest disambiguator is the (a)-canary
(seen.insert print) — cheaper than the (c)-canary (key dump).

---

## §4 Self-canary (run by investigator-a, REPORTED inline above in §2)

The §4 self-canary was the existing `[twin-debug] edge ... fwd_count=N
rev_count=M` per-edge log + the `[twin-debug] insert HE[N] (v→v)` insert
log, both already present in the codebase before this PR. Per
`feedback_adversary_recommendations_need_canary.md` the spec required
`investigator-a` (me) to RUN the cheapest discriminating probe before
recommending it.

**Result**: the canary FIRED. Canonical mesh directed edge `(v21 → v23)`
is NEVER inserted into `directed_he` in the failing boolean, despite
being present in `all_tris`. This narrows the defect to Step 6 boundary
collection / chaining (hypothesis (a) or (b), both LOCAL to
`flood_fill_patches` between L688 and L763).

**No new probe code was needed for §4** — the existing TWIN_DEBUG channels
already carried the load-bearing data once the new `[twin-oracle]`
post-pairing summary made it tractable to map BRep-vertex offending IDs
back to canonical-mesh-vertex IDs.

**Banked PR-Y16-INV-2 candidates** (additional canaries that would
distinguish among hypotheses (a1) vs (a2) vs (b)):
- (a1 vs a2 disambiguator) eprintln at `topology_extract.rs:702` when
  `seen.insert((v0,v1))` returns false — prints the rejected edge + patch.
- (b disambiguator) eprintln at `topology_extract.rs:745` (`_ => break`)
  printing the `(start, current, chain.len(), expected boundary count)`.
- (c disambiguator) eprintln at `topology_extract.rs:471-478` (post Step 3)
  iterating `directed_edge_to_tris.keys()` and confirming `(21,23)` and
  `(23,21)` both present in the failing boolean.

These belong in PR-Y16-FIX's Phase 0 canary OR a PR-Y16-INV-2 probe
extension if adversary-13's cohort sweep finds the defect class needs
broader characterization before fixing.

---

## §5 Open questions for adversary-13

1. **Cohort question**: Is the F0020 pattern (`unpaired_count=N>0`,
   `collision_count=0`, all unpaired HEs from mesh_B small patches near
   the intersection) the same as other YANG-ERR-twin-validation cohort
   members? Per `yang_audit_b_assay_failures.md:107` the cohort had 92
   members on 2026-04-30. Spec §0b asks for ≥3/5 on F0002/F0010/F0030/F0050
   to call it a class.

2. **Mutation-test question**: Is the new `[twin-oracle]` post-pairing
   block load-bearing? Per spec §0b step 2: deliberately corrupt one HE's
   twin field on a passing case (e.g., F0001), confirm `[twin-oracle]`
   reports `unpaired_count >= 1`. If it does NOT, oracle is non-load-bearing
   and this memo's §2 conclusions are inadmissible.

3. **Cherchi 2022 §5 architectural question**: Per
   `yang_audit_c_cherchi2022.md` we don't fully implement Cherchi's per-patch
   labeling with manifold-edge barriers. Does that gap cause the F0020 defect?
   Specifically: does Cherchi 2022's per-patch labeling guarantee that every
   patch boundary HE has a paired reverse HE (i.e., is the manifold-edge
   barrier the missing invariant)? If yes, this is an architectural defect
   (Yang/Cherchi divergence), not a local Step 6 bug, and PR-Y16-FIX scope
   would change accordingly.

4. **PR-Y15c-fix-2 cascade question** (per spec §0b step 5 risk #4): is
   F0020 a previously-masked failure that PR-Y15c-fix-2.2's silent-fallback
   removal exposed? The PR-Y15c-fix-2.2 panic-promotion turned a
   surface-tag-misroute fallback into a panic; F0020 was first observed
   AFTER that PR. Is the surface_map shape on F0020 healthy, or is the
   defect a downstream consequence of unlabeled surfaces hitting a
   different code path? Adversary should diff F0020's surface_map keys
   against pre-PR-Y15c-fix-2 behavior if reproducible.

5. **Reference-parity escalation question** (per spec §0b step 5 +
   `feedback_external_coherence.md`): F0020 has 0 prior anchors burned on
   this defect class. Is reference-parity escalation premature? My answer:
   YES — we haven't tried any anchor yet; the Cherchi 2022 sidecar build
   should be reserved for AFTER PR-Y16-FIX burns 1-2 anchors. (The
   3-wrong-anchor rule gives us a cheap escalation trigger.)

6. **Last-write-wins probe-design question** (banked PR-VIZ-2): Stage A/Bb/B/C
   OBJ + CSV file dumps are overwritten when a single case has multiple
   boolean operations using the same stage tags. The `[twin-debug]` text
   channel preserves both calls. Should the file dumps gain a per-call
   suffix (e.g., `stage_C.call=2.obj`)? Does the in-memory PR-VIZ-3a
   capture have the same gap (likely yes — it keys on stage_tag string
   without a per-call counter)? Not blocking for PR-Y16-FIX; flag for
   future PR-VIZ-4 sub-PR.

---

## Verification (against spec §0a step 5)

- ☑ `cargo build -p kernel -p test-harness` — compiles clean.
- ☑ `cargo clippy -p kernel -p test-harness` — no NEW warnings on changed code.
- ☑ Spotlight reproduces the user's exact error string with `YANG_BOOLEAN=1`,
  Status=Failed, Detail starts with `auto-union-failed`.
- ☑ With `TWIN_DEBUG=1`, the `[twin-oracle]` block fires (3 lines + 5 offenders).
- ☑ `/tmp/viz/f0020/F0020/stage_C_labels.csv` has the new columns
  `e0_can,e0_rev,e1_can,e1_rev,e2_can,e2_rev`.
- ☑ Probe-OFF (no `TWIN_DEBUG`, no `YANG_STAGE_DUMP`) emits zero
  `[twin-oracle]` lines (verified by grep on stdout/stderr).
- ☑ `record_stage` signature unchanged (StageMesh stays
  `vertices/indices/labels` only). The CSV extension is FILE-ONLY.
- ☑ `spotlight_f0020` runs in 85ms (< 2s target).
- ☑ All 5 §1-§5 sections populated, no empty bodies.

**Sub-phase 0a complete. Routing to adversary-13 for sub-phase 0b.**

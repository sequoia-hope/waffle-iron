# PR-Y15b Phase 0 — Cluster-birth diagnostic

**Author:** implementer-c (PR-Y15b Phase 0)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15b_pre_cherchi_input_validation.md`
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` Phase 0
**Probe:** `YANG_CLUSTER_PROBE=1`, sites 1/2/3 in `crates/kernel/src/boolean/yang_integration.rs`

## TL;DR

The 8-vertex cluster (per PR-Y14a §1, §3.2) is born at **Site 3
(`inject_partial_overlap_mesh`)** via `inject_face_with_shared_first`
at `crates/kernel/src/boolean/coplanar_preprocess.rs:1742-1747`. Both
F0002 and F0004 produce the **same byte-identical signature**:

- Sites 1, 2: 16 verts, 16 unique canonical-keys, 0 duplicates
- Site 3: 24 verts, 16 unique canonical-keys, **8 duplicate keys with
  count=2 each** (16 duplicate verts total — every corner of the
  second extrude is duplicated)

**The "two clouds of four" pattern from the brief is incorrect.** The
actual signature is **8 pairs of duplicates**, not 2 clusters of 4.
All 8 duplicates collapse to a single canonical-nanometer key per
corner — i.e., I4 (no tolerance escalation) is satisfied; nanometer
quantization is sufficient to detect them.

The fix shape is a near-mechanical clone of PR-Y14b's coplanar dedup,
applied to `inject_face_with_shared_first` step 2 (shared-vert append).
Estimated **<30 LOC**, **Low risk**, single function.

## Anchor pre-verification (per `feedback_anchor_before_fix.md`)

Before writing the real probe code, three `eprintln!("[anchor-check]
reached site_N")` canaries were added at the planned anchor sites and
verified empirically.

**Result:** All 6 canaries fired (3 sites × 2 meshes). Anchors are
correct as the brief estimated.

```
[anchor-check] reached site_1 mesh=A    (after dedup_mesh_vertices for A)
[anchor-check] reached site_1 mesh=B    (after dedup_mesh_vertices for B)
[anchor-check] reached site_2 mesh=A    (after inject_identical_footprint_mesh)
[anchor-check] reached site_2 mesh=B
[anchor-check] reached site_3 mesh=A    (after inject_partial_overlap_mesh)
[anchor-check] reached site_3 mesh=B
```

**Empirical line numbers (matching the brief's estimates of
L701/L717/L725):**

- Site 1 (post-dedup A & B): `yang_integration.rs:659-660`
- Site 2 (post-`inject_identical_footprint_mesh`): `yang_integration.rs:701-711`
- Site 3 (post-`inject_partial_overlap_mesh`): `yang_integration.rs:717-729`

Canaries removed before final probe code landed. Verified by
re-running and grepping `[anchor-check]` (0 hits).

## Verbatim probe output — F0002

```
[cluster-probe] site=1 mesh=A target_corner=F0002 count=1 keys=[(-1000000,1000000,4000000):1]
[cluster-probe] site=1 mesh=A target_corner=F0004 count=0 keys=[]
[cluster-probe] site=1 mesh=A global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=1 mesh=B target_corner=F0002 count=1 keys=[(-1000000,1000000,4000000):1]
[cluster-probe] site=1 mesh=B target_corner=F0004 count=0 keys=[]
[cluster-probe] site=1 mesh=B global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=2 mesh=A target_corner=F0002 count=1 keys=[(-1000000,1000000,4000000):1]
[cluster-probe] site=2 mesh=A target_corner=F0004 count=0 keys=[]
[cluster-probe] site=2 mesh=A global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=2 mesh=B target_corner=F0002 count=1 keys=[(-1000000,1000000,4000000):1]
[cluster-probe] site=2 mesh=B target_corner=F0004 count=0 keys=[]
[cluster-probe] site=2 mesh=B global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=3 mesh=A target_corner=F0002 count=2 keys=[(-1000000,1000000,4000000):2]
[cluster-probe] site=3 mesh=A target_corner=F0004 count=0 keys=[]
[cluster-probe] site=3 mesh=A global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
[cluster-probe] site=3 mesh=B target_corner=F0002 count=2 keys=[(-1000000,1000000,4000000):2]
[cluster-probe] site=3 mesh=B target_corner=F0004 count=0 keys=[]
[cluster-probe] site=3 mesh=B global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
```

## Verbatim probe output — F0004

```
[cluster-probe] site=1 mesh=A target_corner=F0002 count=0 keys=[]
[cluster-probe] site=1 mesh=A target_corner=F0004 count=1 keys=[(-100000001,100000001,500000000):1]
[cluster-probe] site=1 mesh=A global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=1 mesh=B target_corner=F0002 count=0 keys=[]
[cluster-probe] site=1 mesh=B target_corner=F0004 count=1 keys=[(-100000001,100000001,500000000):1]
[cluster-probe] site=1 mesh=B global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=2 mesh=A target_corner=F0002 count=0 keys=[]
[cluster-probe] site=2 mesh=A target_corner=F0004 count=1 keys=[(-100000001,100000001,500000000):1]
[cluster-probe] site=2 mesh=A global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=2 mesh=B target_corner=F0002 count=0 keys=[]
[cluster-probe] site=2 mesh=B target_corner=F0004 count=1 keys=[(-100000001,100000001,500000000):1]
[cluster-probe] site=2 mesh=B global_dups: total_verts=16 unique_keys=16 dup_keys=0 dup_vert_count=0
[cluster-probe] site=3 mesh=A target_corner=F0002 count=0 keys=[]
[cluster-probe] site=3 mesh=A target_corner=F0004 count=2 keys=[(-100000001,100000001,500000000):2]
[cluster-probe] site=3 mesh=A global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
[cluster-probe] site=3 mesh=B target_corner=F0002 count=0 keys=[]
[cluster-probe] site=3 mesh=B target_corner=F0004 count=2 keys=[(-100000001,100000001,500000000):2]
[cluster-probe] site=3 mesh=B global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
```

Note: F0004's actual canonical key is `[-100000001, 100000001,
500000000]` (≈ ±100 mm + 1 nm drift, +500 mm), NOT `[-400000000,
400000000, 500000000]` as the brief estimated. Probe was updated to
use the correct key after empirical observation. The targeted-corner
finding (count=2) and the global-dup finding (8 dup keys) agree.

## Cluster-birth verdict

**Site 3 only — `inject_partial_overlap_mesh` introduces all 8
duplicate pairs.**

Per-site delta evidence:

| Site | Verts (A & B both) | Unique keys | Dup keys | Dup verts | Delta from previous |
|---:|---:|---:|---:|---:|---|
| 1 | 16 | 16 | 0 | 0 | (baseline post-dedup) |
| 2 | 16 | 16 | 0 | 0 | identical-footprint inject = no-op |
| 3 | **24** | 16 | **8** | **16** | **partial-overlap inject = +8 verts, +8 dup keys** |

Inspection of `coplanar_preprocess.rs:1716`
(`inject_face_with_shared_first`) reveals the mechanism. Step 2
(L1741-1747):

```rust
// 2. Append shared verts verbatim — preserves canonical bits.
let shared_offset = verts.len();
let mut added_verts: Vec<usize> = Vec::new();
for sv in shared_verts {
    verts.push(*sv);            // ← unconditional append
    added_verts.push(verts.len() - 1);
}
```

The shared verts come from `verts_2d_to_3d(&shared_2d_verts, ...)`
applied to the i_overlay-computed 2D overlap polygon. They are
geometrically the corners of the overlap region (which **coincide**
with corners of the original B-Rep face), but their float bits are
the result of independent arithmetic: `2D from i_overlay → 3D back-
projected via plane basis`. The existing face corners came from a
DIFFERENT path (B-Rep vertex `position` → tessellate). The two paths
produce the same nanometer canonical key but bit-different f64
positions.

**Step 3 (L1750-1770)** does snap exclusive verts to existing within
`TAU_MODEL`, but step 2 explicitly does NOT — the comment says "to
preserve canonical bits". That comment is wrong: the canonical bits
that matter for downstream Cherchi correctness are the EXISTING vert's
bits (which other tessellation triangles already index). Appending a
new vert with bit-different position at the same canonical key is what
breaks Cherchi's `mesh_booleans_inputcheck`.

Crucially, the `[coplanar-tele] partial_overlap=0` line is misleading
in this context. That counter is incremented at L1321 (after the full
loop body completes successfully), but the telemetry is logged from
inside `split_brep_for_coplanar_pairs` BEFORE inject is called. So
`partial_overlap=0` only means "no pair has yet been counted as a
successful inject", not "inject didn't run". Diagnostic instrumentation
confirmed inject_partial_overlap_mesh does run for both F0002 face
pairs (face_a=0/face_b=0 and face_a=1/face_b=1), each producing 2
shared overlap triangles and triggering 4 unconditional vert appends.

## "Two clouds of four" verification

The brief's "two clouds of four" pattern (PR-Y14a §1) refers to the
8-cluster signature observed at the **post-Cherchi-subdivide stage**
(`subdivided.verts`, Stage A of the conformal probe), not at our
pre-Cherchi probe sites. The PR-Y14a §3.2 dump showed all 8 raw
positions collapsed to a SINGLE canonical key
`[-1000000, 1000000, 4000000]` (one cluster of eight).

Our pre-Cherchi probe shows a DIFFERENT (but related) signature:
**8 distinct canonical keys, each with count=2** (an 8-pair pattern).
The 8 pairs cover the 8 corners of the second extrude — each
corner has one "real" tessellation vert + one "shared overlap"
duplicate inserted by partial-overlap injection.

When Cherchi's `subdivide_mesh_pair` runs on this input, it amplifies
each duplicate-pair into a larger cluster (likely via intersection
geometry resampling each corner from multiple incoming triangles).
That's how 8 pairs of duplicates pre-Cherchi become an 8-cluster at
canonical key 0 post-Cherchi.

**Resolution:** 1 key per duplicate pair, 8 pairs total. Not 1 key
with 8 verts ("one cloud of eight"); not 2 keys with 4 verts each
("two clouds of four"). All 8 dup keys quantize cleanly at nanometer
scale — **I4 (no tolerance escalation) is satisfied**. The fix can
use the existing `QUANT_NANOMETER_SCALE = 1e9` quantization without
any new tolerance constants.

## Phase 2 sizing recommendation

**Estimated LOC for the fix: <30**
**Files touched: 1** (`crates/kernel/src/boolean/coplanar_preprocess.rs`)
**Risk level: Low** (additive change to a single function; the existing
`tau_model`-based snap-to-existing logic for exclusive verts already
demonstrates the pattern; converting it to canonical-nanometer-key
dedup is mechanical)

**Recommended fix shape (informational, NOT spec):**

In `inject_face_with_shared_first` at L1741-1747, replace the
unconditional `verts.push(*sv)` with a canonical-key dedup:

```rust
use std::collections::BTreeMap;
let scale = crate::units::QUANT_NANOMETER_SCALE;
let mut canon_to_idx: BTreeMap<[i64; 3], usize> = BTreeMap::new();
for (i, mv) in verts.iter().enumerate() {
    let key = [
        (mv[0] * scale).round() as i64,
        (mv[1] * scale).round() as i64,
        (mv[2] * scale).round() as i64,
    ];
    canon_to_idx.entry(key).or_insert(i);  // first-seen wins
}

let shared_offset = verts.len();  // unused if all shared verts dedupe
let mut shared_remap: Vec<usize> = Vec::with_capacity(shared_verts.len());
let mut added_verts: Vec<usize> = Vec::new();
for sv in shared_verts {
    let key = [
        (sv[0] * scale).round() as i64,
        (sv[1] * scale).round() as i64,
        (sv[2] * scale).round() as i64,
    ];
    if let Some(&existing) = canon_to_idx.get(&key) {
        shared_remap.push(existing);
    } else {
        let idx = verts.len();
        verts.push(*sv);
        canon_to_idx.insert(key, idx);
        shared_remap.push(idx);
        added_verts.push(idx);
    }
}
let shared_index = |i: usize| -> usize { shared_remap[i] };
```

The `shared_index` closure (L1748) currently returns `shared_offset
+ i`; the fix changes it to `shared_remap[i]`. Step 4 (L1772-1780)
that uses `shared_index(tri[k])` continues to work unchanged.

The exclusive-vert snap at step 3 (L1750-1770) should ideally also
move to canonical-key dedup for consistency (currently uses
`tau_sq = TAU_MODEL * TAU_MODEL` Euclidean distance, which is a
floating-point comparison rather than integer canonical-key
equality — a potential source of nondeterminism per I5). However,
that is OUT OF SCOPE for the F0002 anchor (no evidence currently
shows the exclusive-vert snap producing the cluster). Leave for
PR-Y15b.1 if the corpus sweep shows residual `combined_failures`.

## Spec ambiguities encountered

The plan's expected birth site (Site 1 tessellation, ~70% probability;
Site 2/3 inject as backup hypothesis) was approximately correct but
imprecisely localized. Two spec items deserve note:

1. **The brief's F0004 corner estimate (`[-0.4m, +0.4m, +0.5m]`) was
   wrong** — F0004's actual extrude corners are at `±100 mm`, and
   even the `±1mm` corner has a `+1nm` quantization drift (key
   `100000001`, not `100000000`). This is an artifact of the assay
   case generator's float arithmetic; nanometer quantization handles
   it correctly. The probe was updated empirically; the spec amendment
   is just to note that case-specific corner targets in any future
   probe should be derived from the dumped OBJ rather than guessed.

2. **The "two clouds of four" framing** (brief Phase 1 explore agent +
   PR-Y14a memo §3.2) describes the cluster shape AFTER Cherchi's
   `subdivide_mesh_pair`, not at our pre-Cherchi probe sites. At our
   sites, the signature is "8 pairs", not "2 clouds of 4". The fix
   sites in the PR-Y15b spec §3 should be updated to point to the
   `inject_face_with_shared_first` function specifically, since the
   spec currently lists the broader `inject_*_mesh` functions without
   distinguishing between identical-footprint (which is a no-op for
   F0002) and partial-overlap (which is the actual culprit).

3. **`[coplanar-tele] partial_overlap=0` is misleading.** The counter
   is logged before the inject call but reset by `snap_partial =
   COPLANAR_PARTIAL_OVERLAP.load(...)` at the start of
   `split_brep_for_coplanar_pairs`. After all the partial-overlap
   injects ran successfully, the next telemetry would show `>0`. This
   isn't a defect requiring fix — just noting that diagnostic readers
   shouldn't infer "no inject ran" from `partial_overlap=0`. The
   actual evidence is the +8 verts delta between Site 2 and Site 3.

No spec amendment requested in this memo. The PR-Y15b spec §3
branch table (mask families) and §4 invariants (I1–I8) all hold;
only the §3 "implementation guidance" should mention
`inject_face_with_shared_first` as the specific Phase 2 anchor.

## Production safety

- **Probe-disabled F0002 trace** (`cargo test ... | grep -c
  cluster-probe`): **0** ✓
- **Probe-enabled F0002 trace**: emits 18 lines deterministically
  (3 sites × 2 meshes × 3 lines per emission: 2 corner + 1 global
  dup) ✓
- **`cargo clippy -p kernel --no-deps`**: 91 warnings (vs PR-S3
  baseline of 92; my probe code introduces 0 new warnings) ✓
- **No new tolerance constants**: probe uses
  `crate::units::QUANT_NANOMETER_SCALE` only ✓
- **No new env vars beyond `YANG_CLUSTER_PROBE`**: ✓
- **All anchor canaries removed before commit**: ✓

## Conclusion

Phase 2 may proceed in this session. The fix is bounded (<30 LOC,
single function, one file), low risk (additive over the existing
`tau_model` snap pattern), and the anchor is empirically pinned
(`coplanar_preprocess.rs:1742-1747`, the `inject_face_with_shared_first`
shared-vert append).

# PR-Y15c-fix Phase 0 (v2) — Stage F multi-probe diagnostic

**Author:** implementer-h (PR-Y15c-fix Phase 0 v2)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15c_fix_phase0_stage_f_repair.md`
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0b
**Probe family:** Stage F (5 sub-stages, gated on `YANG_CONFORMAL_PROBE=1`,
tagged `[stage-f]`) inside `crates/kernel/src/tessellation::tessellate_solid_bounded`
at `crates/kernel/src/tessellation/mod.rs:4164-4380`
**Reproducers:** F0031 + F0040 spot-check; full F0031–F0040 cluster validated.
**Wrong-anchor count for PR-Y15c-fix:** 1 of 3 (weld site refuted, see v1 abort).
The Stage F locus is the second anchor candidate; this Phase 0 produces an
empirically-pinned answer rather than a refutation.

## TL;DR

**The cohort SPLITS on the dropper axis.** Two distinct mechanisms remove
triangles inside `tessellate_solid_bounded`, and a THIRD mechanism removes
triangles BEFORE F.0 (per-face dispatch). All three are load-bearing for
the F0031–F0040 cohort:

1. **Pre-F.0 (per-face dispatch)** removes a uniform **−8 tris per case** across
   all 10 cases. Source is the per-face dispatch loop at L4181-4272 (or the
   `discretize_edges` call upstream at L4170). Phase 0 cannot localize further.
2. **F.0 → F.1 (`remove_winding_insensitive_duplicates`** at `repair.rs:502-574`,
   called L4287) is the dominant dropper for **F0031–F0035 + F0039 (6/10 cases)**,
   removing 4 to 24 triangles per case.
3. **F.2 → F.3 (`remove_nonmanifold_duplicates_aggressive`** at `repair.rs:1870-2154`,
   called L4351) is the dominant dropper for **F0036–F0038, F0040 (4/10 cases)**,
   removing 40 to 48 triangles per case after F.1→F.2 ADDS triangles
   (Steiner-fan re-tessellation) — a counter-intuitive net pattern.

**No single decision-tree row fires uniformly.** Decision-tree row 1 fires on
6/10 cases (F.0→F.1 dropper); row 3 fires on 4/10 cases (F.2→F.3 dropper);
row 5 fires concurrently on 10/10 cases (pre-F.0 −8 tri loss). Anchor
recommendation for PR-Y15c-fix is two-pronged + a v3 follow-up for pre-F.0:

- **PR-Y15c-fix-1**: `remove_winding_insensitive_duplicates` over-removes
  (F0031–F0035 + F0039 cohort) — `repair.rs:502-574`.
- **PR-Y15c-fix-3**: `remove_nonmanifold_duplicates_aggressive` over-removes
  (F0036–F0038 + F0040 cohort) — `repair.rs:1870-2154`.
- **PR-Y15c-fix-Phase0-v3**: per-face dispatch probes in
  `tessellate_cylindrical_face_bounded` / `tessellate_planar_face_bounded`
  to localize the uniform −8 tri pre-F.0 loss (this is wrong-anchor count #2;
  v3 instrumentation per spec §5 row 5).

## Anchor pre-verification (per `feedback_anchor_before_fix.md`)

Per the strategic-escalation rule and standing canary discipline, an
`eprintln!("[stage-f-canary] reached tessellate_solid_bounded face_count={}", sorted_faces.len())`
was inserted at `tessellation/mod.rs` (just before the per-face dispatch
loop, after `sorted_faces` is built) BEFORE coding the real probes.

**Result:** `batch_enclosed_subtract_fix` (F0031–F0040) executed under
`YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1`. The canary fired **20 times
total** = 2 fires per case: 10 fires with `face_count=6` (the small-box
operand for F0031–F0035 / the small-box pre-Cherchi result for F0036–F0040)
and 10 fires with `face_count=10` (the cylinder operand or cylinder-side
result mesh). Canary fired ≥1 time per case → ABORT-if-zero-fires (P10)
does not trigger. Canary verified at the planned probe site. **Canary
removed before final probe code landed** — verified by re-grep on
probe-on rerun (0 hits in `/tmp/probe_stderr` for `stage-f-canary`).

**Spec note:** spec §4.2 said "≈10 fires total". Actual is 20 (one per
operand-or-result B-Rep that goes through `tessellate_solid_bounded`).
Implementer-g's earlier dispatch trace specifically traced **result-mesh
calls** at L231 in `tessellate_solid_ext` and reported 10; the additional
10 fires here are operand-side calls that also reach `tessellate_solid_bounded`
through the same early-return path. Distinguishing operand vs result-mesh
in the probe output is by tri_count signature: operand calls show
`tri_count=12` flat across F.0–F.4 (clean cube), result-mesh calls show
the larger non-trivial deltas analyzed below.

## Stage F probe family — implementation

5 probes inserted in `tessellation/mod.rs` (~25 LOC additive):

| sub | After call/state | Line range |
|---|---|---|
| 0 | F.0 baseline: after per-face dispatch loop, before `fix_winding_consistency` | L4274-4279 |
| 1 | F.1: after `remove_winding_insensitive_duplicates` | L4292-4297 |
| 2 | F.2: after `remove_nonmanifold_topology_aware` | L4341-4346 |
| 3 | F.3: after `remove_nonmanifold_duplicates_aggressive` | L4353-4358 |
| 4 | F.4: after `weld_smooth_vertices`, before `Ok(RenderMesh { ... })` | L4368-4373 |

Each probe form (gated on `YANG_CONFORMAL_PROBE=1`):
```rust
if std::env::var("YANG_CONFORMAL_PROBE").as_deref() == Ok("1") {
    let unpaired = repair::count_unpaired_in_mesh(&vertices, &indices);
    let tri_count = indices.len() / 3;
    eprintln!("[stage-f] sub=N tri_count={tri_count} unpaired={unpaired}");
}
```

Reuses `repair::count_unpaired_in_mesh` (`pub(super)` at `repair.rs:81-124`,
already callable from `tessellation/mod.rs` because of `use self::repair::*;`
at L29 — no visibility change required). The explicit `repair::` path is
used in the probe code for self-documenting clarity.

## Spec ambiguity #1 — repair pipeline has 6 stages, spec described 4

Spec §1 framed the repair pipeline as 4 stages
(`remove_winding_insensitive_duplicates`, `remove_nonmanifold_topology_aware`,
`remove_nonmanifold_duplicates_aggressive`, `weld_smooth_vertices`). The
actual code path between `fix_winding_consistency` and the final return
includes TWO additional repair stages between F.1 and F.2 that the spec
did NOT call out:

- `flip_nonmanifold_interior_diagonals` (called at L4290, between dedup
  and topology-aware)
- `retessellate_nonmanifold_faces_with_steiner_fan` (called at L4304,
  between flip and topology-aware)

This means the F.1 → F.2 transition probed in this Phase 0 captures the
CUMULATIVE effect of: `flip_nonmanifold_interior_diagonals` +
`retessellate_nonmanifold_faces_with_steiner_fan` +
`remove_nonmanifold_topology_aware`. The Steiner-fan stage is
re-tessellation (it can ADD triangles), and indeed the F.1 → F.2 transition
shows tri_count INCREASING for F0036–F0038, F0040 (56 → 84, +28) — see
data below.

The 5-probe layout is the spec-mandated layout and the implementation
adheres to it. The cumulative F.1 → F.2 attribution is documented here
so that PR-Y15c-fix-N can sub-divide the F.1 → F.2 transition with
intermediate probes if the topology-aware-vs-Steiner-fan distinction
becomes load-bearing.

## Spec ambiguity #2 — canary fire count expected ≈10, actual 20

Documented under §"Anchor pre-verification" above. The 20-vs-10
discrepancy is a function of operand-side calls also reaching
`tessellate_solid_bounded` and is informative (it explains the 20 stage-f
blocks emitted per batch). Per-case attribution of result-mesh blocks is
unambiguous via tri_count signature.

## Verbatim probe output — F0031 (canonical reproducer, box-minus-cyl)

The two `tessellate_solid_bounded` calls per case emit consecutive blocks.
For F0031 (the first of 10 cases in batch order):

```
# Operand or pre-Cherchi small-box mesh (face_count=6, clean throughout)
[stage-f] sub=0 tri_count=12 unpaired=0
[stage-f] sub=1 tri_count=12 unpaired=0
[stage-f] sub=2 tri_count=12 unpaired=0
[stage-f] sub=3 tri_count=12 unpaired=0
[stage-f] sub=4 tri_count=12 unpaired=0
# Result mesh (face_count=10, the non-trivial case)
[stage-f] sub=0 tri_count=40 unpaired=4
[stage-f] sub=1 tri_count=36 unpaired=12
[stage-f] sub=2 tri_count=36 unpaired=12
[stage-f] sub=3 tri_count=36 unpaired=12
[stage-f] sub=4 tri_count=36 unpaired=12
[conformal-probe] stage=E_lod=Render unpaired=12 multi_paired=0 euler_chi=2 well_formed=false verts=26 tris=36 unique_edges=60
```

**F0031 result-mesh per-stage delta:**

| Sub-stage | tri_count | Δ tri | unpaired | Δ unpaired |
|---|---:|---:|---:|---:|
| F.0 (before fix_winding) | 40 | — | 4 | — |
| F.1 (after dedup) | 36 | **−4** | 12 | **+8** |
| F.2 (after topo-aware) | 36 | 0 | 12 | 0 |
| F.3 (after aggressive) | 36 | 0 | 12 | 0 |
| F.4 (after weld_smooth) | 36 | 0 | 12 | 0 |

**F0031 dropper: F.0 → F.1 = `remove_winding_insensitive_duplicates`.**

## Verbatim probe output — F0040 (operand-order spot-check, cyl-minus-box)

For F0040 (the 10th case, the OTHER operand-order spot-check):

```
# Operand or pre-Cherchi small-box mesh (face_count=6, clean throughout)
[stage-f] sub=0 tri_count=12 unpaired=0
[stage-f] sub=1 tri_count=12 unpaired=0
[stage-f] sub=2 tri_count=12 unpaired=0
[stage-f] sub=3 tri_count=12 unpaired=0
[stage-f] sub=4 tri_count=12 unpaired=0
# Result mesh (face_count=10, the non-trivial case)
[stage-f] sub=0 tri_count=76 unpaired=20
[stage-f] sub=1 tri_count=56 unpaired=52
[stage-f] sub=2 tri_count=84 unpaired=36
[stage-f] sub=3 tri_count=40 unpaired=20
[stage-f] sub=4 tri_count=40 unpaired=20
[conformal-probe] stage=E_lod=Render unpaired=24 multi_paired=4 euler_chi=12 well_formed=false verts=42 tris=40 unique_edges=70
```

**F0040 result-mesh per-stage delta:**

| Sub-stage | tri_count | Δ tri | unpaired | Δ unpaired |
|---|---:|---:|---:|---:|
| F.0 (before fix_winding) | 76 | — | 20 | — |
| F.1 (after dedup) | 56 | **−20** | 52 | **+32** |
| F.2 (after topo-aware) | 84 | **+28** | 36 | **−16** |
| F.3 (after aggressive) | 40 | **−44** | 20 | **−16** |
| F.4 (after weld_smooth) | 40 | 0 | 20 | 0 |

**F0040 dropper: F.2 → F.3 = `remove_nonmanifold_duplicates_aggressive`** (drops 44 tris in one pass).

**Counter-intuitive observation #1:** F.1 → F.2 INCREASES tri_count by 28.
This is the spec-ambiguity-#1 effect: `retessellate_nonmanifold_faces_with_steiner_fan`
re-tessellates faces between F.1 and F.2, producing more triangles than the
earcut output it replaced.

**Counter-intuitive observation #2 (Risk #5 documentation):** F.3 → F.4
`weld_smooth_vertices` does NOT change tri_count or `unpaired` for any case.
Either welding doesn't fire on these meshes (no cylindrical-face vertex pairs
in the close-position-and-normal-grid relation) OR the weld merges position
but `count_unpaired_in_mesh`'s TAU_TESS_GRID_FACTOR quantization already
treats them as the same vert. Either way, F.4 is a no-op on this cohort —
so the F.3 → F.4 quant-scale risk does not surface here. The risk remains
documented for future cohorts where weld_smooth fires.

## Cluster homogeneity — F0031–F0040

**The cohort SPLITS** into two sub-clusters:

| Case | F.0 tri | F.1 tri | F.2 tri | F.3 tri | F.4 tri | Final unpaired | Dominant dropper | Decision row |
|---|---:|---:|---:|---:|---:|---:|---|---|
| F0031 | 40 | 36 | 36 | 36 | 36 | 12 | F.0→F.1 (−4) | **1** |
| F0032 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0033 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0034 | 44 | 32 | 32 | 32 | 32 | 28 | F.0→F.1 (−12) | **1** |
| F0035 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0036 | 76 | 56 | 84 | 36 | 36 | 16 | F.2→F.3 (−48) | **3** |
| F0037 | 76 | 56 | 84 | 40 | 40 | 12 | F.2→F.3 (−44) | **3** |
| F0038 | 76 | 56 | 84 | 40 | 40 | 20 | F.2→F.3 (−44) | **3** |
| F0039 | 68 | 44 | 44 | 44 | 44 | 40 | F.0→F.1 (−24) | **1** |
| F0040 | 76 | 56 | 84 | 40 | 40 | 20 | F.2→F.3 (−44) | **3** |

**Sub-cluster A (decision row 1):** F0031–F0035, F0039 (6/10 cases).
Dropper is `remove_winding_insensitive_duplicates` (F.0→F.1). Tri loss
ranges from −4 to −24. F.1–F.4 are stable. Operand pattern: F0031–F0035
are box-minus-cyl (small box), F0039 is cyl-minus-box (the only F003x
case that falls into sub-cluster A despite being cyl-minus-box).

**Sub-cluster B (decision row 3):** F0036–F0038, F0040 (4/10 cases).
Dropper is `remove_nonmanifold_duplicates_aggressive` (F.2→F.3). Tri
journey is non-monotonic: F.0=76 → F.1=56 (dedup drops 20) →
**F.2=84 (Steiner fan adds 28)** → F.3=40 (aggressive drops 44).
F.3–F.4 are stable. All four are cyl-minus-box.

**Key observation:** Sub-cluster B's mass tri loss is MASKED by the
mid-pipeline Steiner-fan addition. If you only looked at start/end tri
counts (76 → 40), you'd miss the fact that the pipeline transiently
inflates to 84 tris before the aggressive removal pass collapses it
to 40. Sub-cluster B is `remove_nonmanifold_duplicates_aggressive`
fighting Steiner-fan inflation.

## Decision-tree row determination

Per spec §5:

- **Row 1 fires on 6/10 cases** (F0031–F0035, F0039): F.0 → F.1 drop ≥4
  triangles. Anchor: `remove_winding_insensitive_duplicates` over-removes.
  Next PR: PR-Y15c-fix-1 — `repair.rs:502-574` tri_key dedup logic.
- **Row 2 fires on 0/10 cases**: no F.1 → F.2 drop ≥12 triangles.
  (F0036–F0038, F0040 show F.1 → F.2 INCREASE by 28 tris due to Steiner-fan
  re-tessellation; not a drop.)
- **Row 3 fires on 4/10 cases** (F0036–F0038, F0040): F.2 → F.3 drop −44
  to −48 triangles. Anchor: `remove_nonmanifold_duplicates_aggressive`
  over-removes. Next PR: PR-Y15c-fix-3 — `repair.rs:1870-2154` aggressive
  10-pass removal.
- **Row 4 fires on 0/10 cases**: F.3 → F.4 is a no-op for all cases
  (weld_smooth doesn't fire, or doesn't change quantized topology).
- **Row 5 fires on 10/10 cases (concurrent):** sum of tri_drops F.0 → F.4
  is LESS than the PR-Y15c Stage E delta — see Reconciliation below. The
  loss starts BEFORE F.0 (per-face dispatch). Per spec §5 row 5 routing:
  this is wrong-anchor count #2 territory and routes to PR-Y15c-fix-Phase0-v3
  with per-face dispatch probes.

## Reconciliation (load-bearing per Risk #1)

Spec §8 deliverable 3: `tri_drop` summed across F.0 → F.4 MUST match the
Stage E delta from PR-Y15c (F0031: −12; F0040: −44). If sum < expected:
loss starts BEFORE F.0; row 5 fires.

| Case | Stage C tris | F.0 tris | Pre-F.0 Δ | F.4 tris | F.0→F.4 Δ | Sum (Stage C → F.4) | PR-Y15c E delta |
|---|---:|---:|---:|---:|---:|---:|---:|
| F0031 | 48 | 40 | **−8** | 36 | −4 | −12 | −12 ✓ |
| F0032 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 ✓ |
| F0033 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 ✓ |
| F0034 | 52 | 44 | **−8** | 32 | −12 | −20 | −20 ✓ |
| F0035 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 ✓ |
| F0036 | 84 | 76 | **−8** | 36 | −40 | −48 | −48 ✓ |
| F0037 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 ✓ |
| F0038 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 ✓ |
| F0039 | 76 | 68 | **−8** | 44 | −24 | −32 | −32 ✓ |
| F0040 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 ✓ |

**Pre-F.0 loss is uniformly −8 tris per case across all 10 cases.**

This is a strong signal that ONE specific upstream operation (likely a
single face in `tessellate_cylindrical_face_bounded` or
`tessellate_planar_face_bounded`, OR a single edge in `discretize_edges`)
loses exactly 8 triangles per result-mesh call. The −8 is suspiciously
uniform — it could be a single face emitting only 4 triangles instead of
12 (cylindrical fan with too few segments), or two faces each losing 4.

Because the −8 is uniform across box-minus-cyl AND cyl-minus-box and is
identical for all cohort sizes (F.0 ranging from 36 to 76), the pre-F.0
loss mechanism is **not** dependent on operand size — strongly suggesting
the loss is in a per-face primitive that's invoked once per result mesh
(e.g., a single bottom or top cylindrical cap).

**Reconciliation outcome:** sum (pre-F.0 + F.0→F.4) = PR-Y15c E delta
for all 10 cases. The sum is positive (matches), but ONLY when the
pre-F.0 portion is included. Per spec §5 row 5 framing:
- F.0→F.4 sum ALONE is < PR-Y15c E delta for all cases (e.g., F0031: F.0→F.4 = −4,
  Stage E delta = −12, gap = −8) → row 5 partially fires.
- The Steiner-fan inflation on sub-cluster B ALSO complicates the simple
  "sum of drops" arithmetic — F0040 has F.1 → F.2 = +28 (an INCREASE),
  so a naive monotonic sum is misleading.

**Both row 5 (pre-F.0) and row 1 / row 3 (F.0 → F.4 droppers) need fixes.**
The pre-F.0 loss is constant (−8) across cases; the F.0 → F.4 loss explains
the case-to-case variance.

## Risk #5 documentation — quant-scale mismatch (count_unpaired vs weld_smooth)

`count_unpaired_in_mesh` quantizes positions at `TAU_TESS_GRID_FACTOR`
(per `repair.rs:81-124`). `weld_smooth_vertices` may quantize at a
different scale (position + normal grid).

**Observation:** F.3 → F.4 transition is a no-op on every case in the
F0031–F0040 cohort (tri_count and `unpaired` both unchanged). This means
either:
- `weld_smooth_vertices` doesn't fire on these meshes (no candidate
  vertex pairs meeting its position+normal grid criteria), OR
- `weld_smooth_vertices` does fire but the merges don't change what
  `count_unpaired_in_mesh`'s quantization considers a "shared" edge.

Either way, the quant-scale risk does NOT confound the F.3 → F.4 reading
in this cohort. **Risk #5 remains a future concern** for cohorts where
weld_smooth actually fires; this Phase 0 cannot test that hypothesis.

## Named anchor functions

Two confirmed anchors + one escalation:

1. **PR-Y15c-fix-1 anchor (sub-cluster A: F0031–F0035, F0039 — 6/10 cases):**
   `remove_winding_insensitive_duplicates` at `crates/kernel/src/tessellation/repair.rs:502-574`.
   Called from `tessellate_solid_bounded` at `crates/kernel/src/tessellation/mod.rs:4287`.
   Drops 4 to 24 triangles between F.0 and F.1 — the dedup logic is treating
   triangles as duplicates that should be retained as legitimately-distinct
   adjacent face triangles.

2. **PR-Y15c-fix-3 anchor (sub-cluster B: F0036–F0038, F0040 — 4/10 cases):**
   `remove_nonmanifold_duplicates_aggressive` at `crates/kernel/src/tessellation/repair.rs:1870-2154`.
   Called from `tessellate_solid_bounded` at `crates/kernel/src/tessellation/mod.rs:4351`.
   Drops 44 to 48 triangles between F.2 and F.3 — the docstring's "no safety
   checks" warning manifests; the aggressive 10-pass removal collapses
   meshes that the upstream Steiner-fan re-tessellation just re-built. This
   is a Steiner-fan-vs-aggressive fight and the aggressive pass wins.

3. **PR-Y15c-fix-Phase0-v3 anchor (10/10 cases, pre-F.0 loss — anchor unknown):**
   Per-face dispatch loop at `crates/kernel/src/tessellation/mod.rs:4181-4272`,
   sub-helpers `tessellate_cylindrical_face_bounded`,
   `tessellate_planar_face_bounded`, `discretize_edges` at L3136. Phase 0 v3
   needs probes inside these helpers to localize the −8 tri loss.

## Production safety verification

Per spec §8 deliverable 4 + DoD §6:

1. **Probe-off byte identity** (`YANG_CONFORMAL_PROBE` unset):
   - Command: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --release -- yang_trace_f0002 --ignored --nocapture --test-threads=1`
   - `[stage-f]` lines: **0** ✓
   - `[stage-f-canary]` lines: **0** ✓
   - Test result: **1 passed; 0 failed** ✓ (F0002 trace test passes)
   - results.json baseline: `passed: 11, failed: 179`. The probes are
     gated on `YANG_CONFORMAL_PROBE=1`; probe-off runs cannot affect
     pass/fail counts. Determinism preserved.

2. **`cargo clippy -p kernel --no-deps`:**
   - Pre-edit baseline (spec-cited): 91 warnings.
   - Post-edit (5 probes + 0 canary): **91 warnings** ✓
   - Net delta: **0** ✓
   - (The team-lead bound contract said baseline=91. This Phase 0 measurement
     confirms baseline=91 with my probes added — implementer-f's earlier
     observation of 92 was a transient delta unrelated to my work.)

3. **`rustfmt --check` on edited file only:**
   - `crates/kernel/src/tessellation/mod.rs`: **clean** ✓ (exit=0)
   - Per the fmt-cascade lesson: `cargo fmt -p kernel` was NOT run.

4. **DoD §6 (Infrastructure / Tooling Change) re-verification:**
   - "Does not alter modeling behavior unintentionally": ✓
     (5 probes are env-gated; default-off; no mutation of vertices/indices.)
   - "Tests still pass": ✓ (F0002 trace passes; F0031–F0040 still fail with
     same `watertight_mesh` signatures — failure modes unchanged.)
   - "No silent change in determinism": ✓ (all probe ops are pure reads + eprintln.)
   - "Build remains reproducible": ✓ (no Cargo.toml edits, no new deps,
     no feature flags.)

5. **Anchor canary removed before final probe code landed:**
   verified by `grep -n stage-f-canary /home/claude/workspace/crates/kernel/src/tessellation/mod.rs`
   → no matches ✓.

6. **No new env vars beyond existing `YANG_CONFORMAL_PROBE`:** ✓.

## Spec ambiguities encountered (summary)

1. **Repair pipeline has 6 stages, not 4** — see §"Spec ambiguity #1" above.
   F.1 → F.2 captures cumulative effect of 3 stages (flip + Steiner + topology-aware).
   This is load-bearing for sub-cluster B's interpretation: the F.1 → F.2 INCREASE
   of 28 tris is Steiner-fan inflation, NOT topology-aware repair.

2. **Canary fire count expected ≈10, actual 20** — see §"Spec ambiguity #2".
   Operand-side calls also reach `tessellate_solid_bounded`. Per-case
   attribution unambiguous via tri_count signature (operand=12 flat,
   result=non-trivial deltas).

3. **Cohort homogeneity assumed; cohort splits in reality.** Spec §"Decision tree"
   implicitly assumed one row would fire across the cohort. Two rows fire on
   disjoint sub-clusters (row 1 on 6/10, row 3 on 4/10), plus row 5 fires
   concurrently on 10/10. Recommendation: PR-Y15c-fix-1 and PR-Y15c-fix-3 are
   BOTH needed; PR-Y15c-fix-Phase0-v3 is needed to localize the pre-F.0 loss.

## Reference comparison status

Per spec §7 + PR-Y15c diagnostic § "Reference comparison status": no
Cherchi reference exists for the render-LOD layer (Cherchi outputs the
conformal mesh directly; no analogous render-LOD step). This Phase 0
relies on internal canary discipline + multi-stage probe per
`feedback_multi_stage_anchor_probe.md`. Reference parity for repair
stages specifically is not buildable; the multi-stage probe approach
delivered the expected per-stage attribution.

## Conclusion

PR-Y15c-fix Phase 0 v2 fires **decision rows 1 + 3 + 5 concurrently on
disjoint sub-clusters**:

- **Sub-cluster A (F0031–F0035, F0039 = 6/10): row 1 — `remove_winding_insensitive_duplicates`.**
- **Sub-cluster B (F0036–F0038, F0040 = 4/10): row 3 — `remove_nonmanifold_duplicates_aggressive`.**
- **All 10 cases: row 5 (partial) — uniform −8 tri loss BEFORE F.0** (per-face dispatch).

Recommended next actions:

1. **PR-Y15c-fix-1** (highest priority — fixes 6/10 cases): scope to
   `repair.rs:502-574` `remove_winding_insensitive_duplicates`. Investigate
   why dedup keys treat 4-24 legitimate triangles as duplicates per case.
2. **PR-Y15c-fix-3** (fixes 4/10 cases): scope to `repair.rs:1870-2154`
   `remove_nonmanifold_duplicates_aggressive`. Investigate the
   Steiner-fan-vs-aggressive fight; either constrain the aggressive pass
   (it's fighting upstream re-tessellation) OR fix the Steiner fan to not
   create non-manifold edges that the aggressive pass then over-removes.
3. **PR-Y15c-fix-Phase0-v3** (per-case constant fix, −8 tris each):
   scope a v3 spec for per-face dispatch probes inside
   `tessellate_cylindrical_face_bounded` / `tessellate_planar_face_bounded`
   / `discretize_edges`. This is wrong-anchor count #2 territory; per
   spec §5 row 5 routing.

**A15.6 cross-domain coordination required for all three fixes (still
inside `tessellation::`).**

PR-Y15c-fix v1 (weld_shared_edge_vertices, refuted) → PR-Y15c-fix v2
(this Phase 0, two anchors confirmed + one escalation) → PR-Y15c-fix-1
+ PR-Y15c-fix-3 + PR-Y15c-fix-Phase0-v3 chain (next).

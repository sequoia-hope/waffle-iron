# PR-Y15c-fix Phase 0 (v3) — Per-face dispatch probe diagnostic

**Author:** implementer-i (PR-Y15c-fix Phase 0 v3)
**Date:** 2026-05-05
**Spec:** `specs/yang_pr_y15c_fix_phase0_v3_per_face_dispatch.md` (sub-phase 0a)
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0b
**Probe family planned:** `[unequal-ring-probe]` at `crates/kernel/src/tessellation/mod.rs:4051-4056`
**Canary family used (P10 anchor pre-verification per spec §4.2):** `[unequal-ring-canary]` at L4027 + escalation canaries `[cyl-dispatch-canary]` at L3921, `[cyl-fn-entry-canary]` at L3489, `[bounded-entry-canary]` + `[face-dispatch-canary]` at L4181-4184
**Reproducer:** `batch_enclosed_subtract_fix` (F0031–F0040) at `crates/test-harness/tests/assay_randomized.rs:445`
**Wrong-anchor count for PR-Y15c-fix arc:** moves to 2 of 3 (v1 weld site refuted; **v3 L4053 unequal-ring earcut REFUTED**; v2 stage-f anchor still pinned).

## TL;DR — ABORT P10

**Decision-tree row 3 fires: anchor canary at L4027 did not fire on any case in the F0031–F0040 cohort.** The unequal-ring branch in `tessellate_cylindrical_face_bounded` is dead code for this cohort. The L4053 silent-failure earcut hypothesis is **REFUTED**.

Canary fire tally on `batch_enclosed_subtract_fix` with `YANG_CONFORMAL_PROBE=1`:

| Canary | Site | Fires |
|---|---|---:|
| `[bounded-entry-canary]` | `tessellate_solid_bounded` entry, after `sorted_faces` build (L4181) | **20** (matches v2 = 2/case) |
| `[face-dispatch-canary]` | per-face match dispatch (L4184), tagged with `geom` variant | **160** (= sum of `sorted_faces.len()` over 20 calls) |
| `[cyl-fn-entry-canary]` | `tessellate_cylindrical_face_bounded` entry (L3489) | **0** |
| `[cyl-dispatch-canary]` | `if top_ring.len() == bottom_ring.len()` (L3921) | **0** |
| `[unequal-ring-canary]` | unequal-ring `} else {` branch (L4027) | **0** |

160 face-dispatch fires, 100% tagged `geom=Planar`. **Zero `Cylindrical`-tagged faces** in the F0031–F0040 cohort. `tessellate_cylindrical_face_bounded` is never called. The unequal-ring earcut at L4053 cannot be the locus of the −8 tris/case loss because the function containing it never executes.

Per spec §4.2 + spec §8 deliverable 1: ABORT-if-zero-fires (P10) triggers. Real probe code at L4051-4056 was NOT landed (would have been moot). All canaries have been removed (`grep -c '\[*-canary\]' crates/kernel/src/tessellation/mod.rs` = 0; `git diff --stat` = empty).

## Anchor pre-verification (per `feedback_anchor_before_fix.md`)

Per the strategic-escalation rule, the spec-mandated canary at L4027 was inserted FIRST (before the real probe at L4051-4056). Spec §4.2 verbatim:

> **ABORT-if-zero-fires per ENGINEERING_CONSTITUTION P10**: if 0 fires for ANY case, the unequal-ring path is NOT the locus and the suspect must be revised.

**Result of first canary run (F0031–F0040 batch, `YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1`):** zero `[unequal-ring-canary]` lines in either stderr or stdout. P10 fires.

Rather than abort immediately, two escalation canaries were added to localize WHY the unequal-ring branch wasn't reached (per `feedback_multi_stage_anchor_probe.md` — probe at multiple stages, not just one):

1. `[cyl-dispatch-canary]` at the if-condition (L3921): determines whether the equal-ring branch was taken instead.
2. `[cyl-fn-entry-canary]` at function entry (L3489): determines whether `tessellate_cylindrical_face_bounded` is called at all.

**Both also fired zero times.** This proves the unequal-ring branch is unreachable from above — `tessellate_cylindrical_face_bounded` itself is never invoked on this cohort.

Two further escalation canaries were added inside `tessellate_solid_bounded`'s per-face dispatch loop:

3. `[bounded-entry-canary]` after `sorted_faces` build (L4181): confirms `tessellate_solid_bounded` runs.
4. `[face-dispatch-canary]` inside the per-face match (L4184): tags each face's `geom` variant.

These fired 20 + 160 times respectively, with 100% `geom=Planar`. The dispatch path for F0031–F0040 sends every face through `tessellate_planar_face_bounded`, never `tessellate_cylindrical_face_bounded`.

## Why the cohort skips `tessellate_cylindrical_face_bounded`

Reading `tessellate_solid_ext` at `tessellation/mod.rs:191-238`: Boolean results route to `tessellate_solid_bounded` only when `!has_arcs && !is_polygon_soup` (L220-232). When `has_arcs == true`, the dispatch falls through to the fan path at L237 (`needs_fan_welding = true`) and uses `tessellate_cylindrical_patch` (L349) for cylindrical faces — NOT the bounded path's `tessellate_cylindrical_face_bounded`.

But the v2 diagnostic shows `[stage-f]` probes firing inside `tessellate_solid_bounded` (= 20 calls/batch, matching `[bounded-entry-canary]`). So the cohort IS in the bounded path. The reason `tessellate_cylindrical_face_bounded` never fires is more specific: **the F0031–F0040 cohort's boolean results have all faces tagged `Planar` in `face_geometry`** — there are no `SurfaceGeom::Cylindrical` faces.

This is consistent with the polygon-clipping comment at L3500-3503 of `tessellate_cylindrical_face_bounded`: "Polygon-clipping boolean results tag faces with SurfaceGeom::Cylindrical but have only linear edge geometry". For F0031–F0040, even that fallback isn't invoked — the boolean result tags ALL faces as Planar, including what should be cylindrical side surfaces. The cylindrical geometry is lost in the boolean.

## Verbatim probe output — F0031 (canonical, box-minus-cyl)

(First-of-cohort, in batch order. Each case calls `tessellate_solid_bounded` twice — once for the small-box operand/result, once for the cylinder-side operand/result.)

```
[bounded-entry-canary] sorted_faces.len()=6
[face-dispatch-canary] kid=N face_idx=N geom=Planar      # ×6 (one per face)
[stage-f] sub=0 tri_count=12 unpaired=0
[stage-f] sub=1 tri_count=12 unpaired=0
[stage-f] sub=2 tri_count=12 unpaired=0
[stage-f] sub=3 tri_count=12 unpaired=0
[stage-f] sub=4 tri_count=12 unpaired=0
[bounded-entry-canary] sorted_faces.len()=10
[face-dispatch-canary] kid=N face_idx=N geom=Planar      # ×10 (one per face)
[stage-f] sub=0 tri_count=40 unpaired=4
[stage-f] sub=1 tri_count=36 unpaired=12
[stage-f] sub=2 tri_count=36 unpaired=12
[stage-f] sub=3 tri_count=36 unpaired=12
[stage-f] sub=4 tri_count=36 unpaired=12
```

**Zero `[unequal-ring-canary]`, `[cyl-dispatch-canary]`, `[cyl-fn-entry-canary]` lines in F0031.**

## Verbatim probe output — F0040 (operand-order spot-check, cyl-minus-box)

```
[bounded-entry-canary] sorted_faces.len()=6
[face-dispatch-canary] kid=N face_idx=N geom=Planar      # ×6
[stage-f] sub=0..4 (clean cube path; tri_count=12 throughout)
[bounded-entry-canary] sorted_faces.len()=10
[face-dispatch-canary] kid=N face_idx=N geom=Planar      # ×10
[stage-f] sub=0 tri_count=76 unpaired=20
[stage-f] sub=1 tri_count=56 unpaired=52
[stage-f] sub=2 tri_count=84 unpaired=36
[stage-f] sub=3 tri_count=40 unpaired=20
[stage-f] sub=4 tri_count=40 unpaired=20
```

**Zero `[unequal-ring-canary]`, `[cyl-dispatch-canary]`, `[cyl-fn-entry-canary]` lines in F0040.**

## Cluster homogeneity — F0031–F0040

| Case | bounded-entry calls | face-dispatch fires | Cylindrical | Planar | Spherical | Conical | Toroidal | unequal-ring-canary |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| F0031 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0032 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0033 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0034 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0035 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0036 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0037 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0038 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0039 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| F0040 | 2 | 16 | 0 | 16 | 0 | 0 | 0 | **0** |
| **Total** | **20** | **160** | **0** | **160** | **0** | **0** | **0** | **0** |

Cluster-homogeneous: zero cylindrical faces in the entire cohort.

## Decision-tree row determination

Per spec §5:

| Row | Condition | Outcome |
|---|---|---|
| 1 | SILENT_SKIP fires on all 10 | DOES NOT APPLY (canary never fired; SILENT_SKIP probe never landed) |
| 2 | Canary fires but SILENT_SKIP doesn't | DOES NOT APPLY (canary never fired) |
| 3 | **Canary doesn't fire** | **FIRES — primary suspect refuted; re-investigate per-face dispatch from scratch; wrong-anchor count #3 → escalate** |

**Anchor verdict: REFUTED.** L4053 unequal-ring earcut is NOT the locus of the −8 tris/case pre-F.0 loss for the F0031–F0040 cohort.

## Reconciliation (load-bearing per spec §8 deliverable 3)

Spec §8 reconciliation requires: tri count saved by SILENT_SKIPs MUST equal the −8 pre-F.0 loss from v2.

- SILENT_SKIPs measured: **0** (canary never fired; SILENT_SKIP probe never landed).
- Pre-F.0 −8 tris/case loss from v2: **−80 tris total across the 10 cases**.

**Reconciliation outcome:** the L4053 silent-failure earcut accounts for **0 of the 80 missing triangles**. Reconciliation FAILS catastrophically. The −8 tris/case loss is somewhere else entirely.

## Why earcut fails — N/A (canary never fired)

Spec §8 deliverable 3 asks for an earcut failure-mode analysis from the SILENT_SKIP boundary/ring data. **No SILENT_SKIP data was collected** because the SILENT_SKIP probe was never landed. The canary at L4027 fired zero times, so by spec §4.2 the real probe at L4051-4056 was withheld and the v3 investigation aborted at the canary stage.

The −8 tris/case loss must originate in `tessellate_planar_face_bounded` (which handles ALL faces of this cohort), or in `discretize_edges` upstream (where the boundary loop vertex pool is built), or in earlier B-Rep construction (where face geometry is tagged Planar instead of Cylindrical).

## Spec ambiguities encountered

1. **Spec template ring binding names `ring1`/`ring2` do not exist in the L4027-4087 scope.** The actual in-scope local bindings are `boundary` (combined polygon, used for thetas/axials), plus the upstream `top_ring` and `bottom_ring` (defined L3870-3920 as the upper and lower vertex rings of the cylindrical patch). The if-dispatch at L3921 is `if top_ring.len() == bottom_ring.len()`. The unequal-ring `else` branch operates only on `boundary`, not on `top_ring`/`bottom_ring` directly. The canary used `boundary.len() top_ring.len() bottom_ring.len()` to capture all three, but the planned probe template would have used `top_ring.len()` and `bottom_ring.len()` in the SILENT_SKIP emission. The actual probe was never landed, so the substitution was not exercised.

2. **L4053 line stability:** confirmed unshifted at the time of canary insertion (the canary was inserted at L4027 inside the else block as the first line after the existing comment, leaving L4053-4056 untouched). Since no real probe landed, no shifts occurred. Line numbers remain valid for the next investigator.

3. **`probe_on` idiom:** spec §4.3 used a `probe_on` boolean, while spec §4.2 + plan said to mirror v2's stage-f inline `std::env::var(...)` style. Resolution per team-lead's bound contract: use the v2 inline style at each probe site. Not exercised because no real probe landed.

4. **`tessellate_cylindrical_face_bounded` is not the only cylindrical tessellation entry point.** There are three: `tessellate_cylindrical_face` (L1035), `tessellate_cylindrical_patch` (L2422), and `tessellate_cylindrical_face_bounded` (L3479). The dispatch at `tessellate_solid_ext` L329-364 routes Boolean cylindrical faces (those without `cylinder_params`) through `tessellate_cylindrical_patch` when `has_arcs == true`. The bounded path is only reached when `!has_arcs && !is_polygon_soup`. F0031–F0040 cases reach the bounded path (per `[bounded-entry-canary]` × 20), but their faces are all tagged `Planar` — so neither the bounded path's cylindrical helper nor the fan path's `tessellate_cylindrical_patch` is invoked for these cases. **The cohort is purely planar inside `tessellate_solid_bounded`.**

## Production safety verification

Per spec §8 deliverable 4 + DoD §6:

1. **Probe-off byte identity** (`YANG_CONFORMAL_PROBE` unset, after canary removal):
   - Command: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --release -- yang_trace_f0002 --ignored --nocapture --test-threads=1`
   - `[unequal-ring-probe]` lines: **0** ✓ (probe never landed)
   - `[unequal-ring-canary]`, `[cyl-dispatch-canary]`, `[cyl-fn-entry-canary]`, `[bounded-entry-canary]`, `[face-dispatch-canary]` lines: **0** ✓ (all canaries removed)
   - Test result: **1 passed; 0 failed** ✓
   - results.json baseline (`/home/claude/workspace/app/tests/cases/assay/results.json`): `passed: 11, failed: 179` ✓ (unchanged from team-lead's bound contract)

2. **`cargo clippy -p kernel --no-deps --release`:**
   - Pre-edit baseline (team-lead-cited): 91. v2 observed: 92. Post-canary-removal: **92** ✓
   - Net delta vs HEAD (post-canary-removal): **0** ✓ (file is byte-identical to HEAD; clippy delta is the v2-observed +1, not introduced by v3)

3. **`rustfmt --check` on edited file only:**
   - `crates/kernel/src/tessellation/mod.rs`: **clean** ✓ (exit=0)
   - Per the fmt-cascade lesson: `cargo fmt -p kernel` was NOT run.

4. **DoD §6 (Infrastructure / Tooling Change) re-verification:**
   - "Does not alter modeling behavior unintentionally": ✓ (file byte-identical to HEAD; no shipped edits)
   - "Tests still pass": ✓ (yang_trace_f0002 passes; F0031–F0040 still fail with same signatures)
   - "No silent change in determinism": ✓ (no shipped edits)
   - "Build remains reproducible": ✓ (no Cargo.toml edits, no new deps)

5. **Anchor canary removed before final probe code landed:**
   verified by `grep -c '\[*-canary\]\|\[unequal-ring-' crates/kernel/src/tessellation/mod.rs` → **0** ✓
   verified by `git diff --stat crates/kernel/src/tessellation/mod.rs` → **empty** ✓

6. **No new env vars beyond existing `YANG_CONFORMAL_PROBE`:** ✓.

## Recommendation for PR-Y15c-fix-2 (anchor for next investigation)

The L4053 hypothesis is refuted. Wrong-anchor count moves to **2 of 3** for the PR-Y15c-fix arc. Per `feedback_anchor_before_fix.md` strategic-escalation rule (three wrong anchors → reference comparison), one more wrong anchor exhausts the budget; per `feedback_external_coherence.md`, escalation goes to differential testing against the Cherchi 2022 reference C++ implementation.

For the immediate next step, two anchor candidates remain inside `tessellate_solid_bounded`:

- **A. `tessellate_planar_face_bounded`** — handles 100% of the cohort's faces. Probe candidates: earcut sites at L3425, L3463, L3704 (per spec §7 "Out of scope" list, deferred to v3-redirect — that v3-redirect IS the next step). Look for `unwrap_or_default()` patterns or other silent-failure modes.
- **B. `discretize_edges`** at L3128-3215 (upstream of per-face dispatch). The vertex pool feeds all face boundaries; if this drops one edge's worth of vertices, it propagates uniformly to −8 tris regardless of face count.

Recommendation: **redirect Phase 0 to v3-redirect** with probes at the planar earcut sites (A) AND at `discretize_edges` entry/exit (B). Single probe-batch can localize whichever is upstream. The cluster-homogeneity finding (uniform −8 across all 10 cases, regardless of operand-order) suggests a per-call constant — favoring A (one specific face that always loses 4 tris) or B (one specific edge that always loses 1 vertex).

The architectural insight from this v3 ABORT is **load-bearing for v3-redirect:** the F0031–F0040 cohort has zero cylindrical faces inside `tessellate_solid_bounded`. Whatever was supposed to be cylindrical (the inside-pocket side wall in box-minus-cyl, the outside-cylinder wall in cyl-minus-box) has been re-tagged as Planar by the boolean. This is itself suspicious — it may be that the −8 tris/case loss is a downstream symptom of the cylindrical→planar geometry-tag loss, not a primitive emission bug. v3-redirect should consider probing where the Planar tag is assigned to ostensibly-curved faces (e.g., in the boolean result B-Rep assembly).

## Conclusion

PR-Y15c-fix Phase 0 v3 fires **decision-tree row 3 on 10/10 cases**: anchor canary did not fire. The L4053 unequal-ring earcut hypothesis is **REFUTED** for the F0031–F0040 cohort.

- Wrong-anchor count for PR-Y15c-fix arc: **2 of 3** (v1 weld site refuted; v3 L4053 refuted; v2 stage-f anchor pinned).
- One more wrong anchor → reference comparison per `feedback_external_coherence.md`.
- File is byte-identical to HEAD — no production shipping required.
- Recommend Phase 0 v3-redirect with probes at planar earcut sites + `discretize_edges` entry/exit.

PR-Y15c-fix v1 (weld_shared_edge_vertices, refuted) → PR-Y15c-fix v2 (Stage F, two anchors confirmed + one escalation) → PR-Y15c-fix-Phase0-v3 (THIS — L4053 refuted) → next: PR-Y15c-fix-Phase0-v3-redirect or reference comparison.

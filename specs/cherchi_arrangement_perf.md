# cherchi-rs arrangement performance — where F0072's time actually goes

**Status:** measured 2026-07-23 (task #198). **The original "spatial-index the
O(n²) triangle-pair detector" premise was REFUTED by measurement** — see §2.
This doc records the true cost model and the two real levers, so the next
perf increment builds against a confirmed bucket (case-first discipline).

**Crate:** `crates/cherchi-rs/` (Stage-2 arrangement).

---

## 1. Context

F0072 (20-op chained stacked-Z gear/polygon union) takes ~132s as an honest
ERROR. It is **not** a hang — the corpus TIMEOUT was a budget artifact, already
fixed by the 120→240s budget bump (task #196). This doc is purely about *making
the arrangement faster*, which matters for the app on complex models and for
future heavier corpus cases — not for F0072's verdict (ERROR either way).

## 2. Measured cost model (the refutation)

Per-stage timing at the yang boundary (`YANG_STAGE_TIME`) and inside the
arrangement (`CHERCHI_ARR_TIME`, `CHERCHI_MA_TIME`) on F0072:

- The boolean pipeline is **90% Stage-2 arrangement**; Stage-3 (SSI) and
  Stage-4 (relocation/reassembly) are ~0.3% combined.
- Inside `native_labeled_arrangement`, **`mesh_arrangement` is ~100%**;
  `compute_all_patches` and `compute_inside_out` are single/double-digit ms.
- Inside `mesh_arrangement`, on the heaviest op (op-7, 14,342 tris):

  | sub-phase | time | note |
  |---|---|---|
  | `detect_intersecting_pairs` | **16.5s** | O(pairs) triangle-triangle tests |
  | `classify_all` | **16.2s** | O(pairs) exact classification |
  | group points/segments | 0.5s | |
  | split + retriangulate (CDT) | ~0s | |

  and **`pairs = 666,731`** (~46 intersecting pairs per triangle).

**Why the "broad-phase" premise was wrong:** the O(n²) in
`detect_intersecting_pairs` is the *pair enumeration* (n²/2 ≈ 100M cheap
6-float AABB-overlap checks ≈ ~0.5s). The real 16.5s is the
**triangle-triangle predicate run on the ~666K pairs that GENUINELY overlap**.
A uniform-grid/octree broad-phase only prunes *non*-overlapping pairs, so it
cannot reduce a genuinely-overlapping set. **Measured:** a correct,
byte-identical grid broad-phase (built + reverted in task #198) changed op-7's
arrangement by <1% (33.4s vs 33.1s). The enumeration was never the bottleneck.

**Root of the huge pair count:** F0072 stacks solids in Z, so adjacent solids
meet at **coplanar faces**. In a triangle-soup arrangement, every triangle of
one solid's contact face overlaps many triangles of the other's coplanar face
→ O(face_tris²) intersecting pairs per contact. This is the classic
**coplanar-contact explosion**.

## 3. The two real levers

### Lever A — reduce the pair count at the source (structural, biggest)

The 666K pairs are dominated by **coplanar stacked/flush face contact**. Yang
**Stage-0 coplanar preprocessing (§4.5.5, roadmap M8)** is exactly the stage
meant to segment coplanar face pairs *before* tessellation so the arrangement
never sees the O(face_tris²) contact. F0072 is fundamentally an **M8 coplanar
case** whose incomplete handling surfaces here as runtime (not just the known
correctness gaps). Closing the stacked/flush-face slice of M8 would collapse
the pair count. **Largest win, but a large structural effort** (M8 is an open
roadmap milestone). Owner: yang-rs Stage-0, not cherchi-rs.

### Lever B — parallelize the per-pair maps (clean, orthogonal, cherchi-rs) — IMPLEMENTED 2026-07-23

**Shipped** behind the `parallel` cargo feature (off by default). Measured:
**F0072 132s → 49.4s wall-clock (2.7×)** on 24 cores, same ERROR verdict.
Only ~2.7× (not ~N×) because only detect+classify parallelize (~80% of
runtime) and Amdahl + per-op region overhead cap it. Byte-identical: the full
cherchi-rs suite passes under BOTH `--features parallel` and default, and the
full corpus `results.json` is unchanged with the feature on.

**Two important reach caveats (do not oversell):**
- rayon cuts **wall-clock, not CPU time**, so it does NOT change the
  CPU-budgeted assay verdicts (F0072 still ~same CPU-seconds) and it does NOT
  help the **WASM-deployed app** (single-threaded; `rayon` is not WASM-clean
  without wasm-threads). Payoff = native wall-clock (dev loop, any native
  embedding) + the clean foundation if wasm-threads land later.
- Reducing the actual work (Lever A) is what helps everywhere; B only makes the
  same work finish sooner on a multicore native host.

Original design notes below.



Both hotspots are **pure, independent, order-preserving maps** over the pair
list:

- `detect_intersecting_pairs`: for each candidate pair → triangle-triangle test.
- `classify_all` (`intersection_points.rs`): literally
  `pairs.iter().map(|&(ta,tb)| ((ta,tb), classify_pair(soup, ta, tb)))`.

Each pair is independent and `classify_pair` / the triangle test are pure
functions of the (immutable) soup. A `rayon` `par_iter().map().collect()`
preserves input order, so the output `Vec` is **bit-identical** to the serial
version — **no reduction reordering, no determinism risk** (this is a *map*,
not a float reduction). On 24 cores this is a ~10–20× wall-clock win on the
heavy ops.

**Constraint:** crate **Hard Rule #5** ("Single-threaded by default;
parallelism via `rayon` is a future feature flag, not the default; determinism
trumps speed"). So this lands **behind a `parallel` cargo feature**, off by
default, with the byte-identical property as the gate. The feature is consumed
by kernel-v2's production build (native, not WASM — WASM stays single-threaded;
`rayon` is not WASM-clean without threads support).

**Oracle:** a test asserting serial output == parallel output on random +
stacked-coplanar soups (the F0072-like fixture), plus the full-corpus
byte-identity (`results.json` unchanged) with the feature on. Cherchi sidecar
parity green.

## 4. Recommendation

1. **Lever B first** if the goal is a fast, low-risk arrangement speedup that
   also directly answers "can we multithread it?": a feature-flagged,
   order-preserving parallel `map` over pairs in `detect` + `classify`.
   ~10–20× on heavy ops, byte-identical, determinism-safe.
2. **Lever A** is the deeper structural fix (fewer pairs beats faster pairs)
   but is an M8 Stage-0 effort in yang-rs, tracked with the coplanar roadmap.
3. The uniform-grid broad-phase is **NOT** worth landing for the corpus (it
   gave <1% on F0072 and adds a code path the corpus can't exercise the value
   of). It would help a *different* input class — huge `n`, sparse
   intersections (e.g. two finely-tessellated solids barely touching) — and can
   be revisited if such a workload appears. Reverted in #198.

## 5. Diagnostic instrumentation (reusable)

The env-gated timing probes used for this measurement were reverted to keep the
tree clean but are trivial to re-add when Lever B lands:
- yang-rs `boolean.rs`: `YANG_STAGE_TIME` — cumulative prep/arrangement/stage3/
  stage4 per op.
- cherchi-rs `labeling/native.rs`: `CHERCHI_ARR_TIME` —
  mesh_arrangement / patches / inside_out.
- cherchi-rs `arrangements/soup.rs`: `CHERCHI_MA_TIME` —
  detect / classify / group / split, plus the `pairs` count.

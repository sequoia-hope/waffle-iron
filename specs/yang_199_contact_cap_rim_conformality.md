# yang #199 — contact-cap ⇄ rim conformality (Direction A design)

**Companion to** `specs/yang_199_stacked_coplanar_pair_explosion.md` (the
measured characterization). That doc REFUTES the two earlier premises (Lever A
cross-solid preprocessing; off-plane cap snap) and pins the driver: at each
stacked coplanar contact the arrangement pays **O(cap_tris × rim_subdivisions)**
because a few **coarse coplanar cap triangles** interact with **many fine
wall/sliver triangles** in one thin slab. This doc is the DESIGN for the fix.

**Goal (perf, not verdict):** collapse F0072's per-op arrangement pair count —
the heaviest op is **666,731 pairs, 77% `selfA` (tower-operand) transversal in
the z≈1.52 contact slab** — without changing any corpus verdict. F0072 stays an
honest ERROR (Extrude 11 azimuth-merge); success = the `selfA` pair count (and
wall-clock) drops, corpus **category-identical 255C/0W**, cherchi sidecar parity
green.

**Crate:** `yang-rs` Stage-0/1 (`stage0/mesh_build.rs`, `stage0/mod.rs`,
`coplanar_overlay.rs`). NOT cherchi-rs (that is Direction B).

---

## 1. Confirmed cost model (from the characterization)

Per stacked union op, Stage-0 detects the `(tower_top_cap, extrude_bottom_cap)`
cross-pair (19/19 detected, 0 walls) and runs the §4.5.5 overlay. The overlay
re-tessellates the cap via an **exact vertical (trapezoidal) decomposition +
ear-clip** (`coplanar_overlay.rs` §2) — T-junction-free *within* the cap, but
**coarse where x-events are sparse** (a slab with no interior crossing becomes
one wide trapezoid → 2 big triangles; e.g. the dumped `ta=4906` spans the full
0.56-wide cap). The shared rim's crossing points are propagated to the adjacent
**wall** faces by `collect_edge_splits` (`mesh_build.rs:160`), and each wall
base-tri is re-triangulated by **`fan_split_tri`** (`mesh_build.rs:302`) — which
fans from the single OPPOSITE (bottom) vertex through every inserted top point.

The dumped `selfA`-transversal pairs are exactly this pairing: one coarse cap
triangle (all verts z=1.5412) × many wall/sliver triangles that all share a
single bottom corner and reach fine top-rim points ~4e-4 apart.

## 2. Prime suspect (H1): the wall corner-fan

`fan_split_tri` triangulates a wall quad whose top edge is finely subdivided
(N crossings) as a **fan from ONE bottom corner** → N long thin slivers, each
spanning from the corner across the whole subdivided edge. Consequences:

1. **Huge sliver AABBs** — a fan sliver's box spans corner→far-rim, so it
   overlaps the cap's box and many cap-triangle boxes → inflates
   `detect_intersecting_pairs`' candidate set.
2. **Near-degenerate slivers** (top verts ~4e-4 apart, apex far away) — the
   exact tri-tri test and `classify_all` are slow and fragile on these, and a
   coarse cap triangle sharing the slab plane with many such slivers is the
   O(cap × rim) interaction.

A **local** wall triangulation (a strip using both the bottom and top
subdivisions, or a monotone triangulation of the trapezoid, instead of a
single-corner fan) would give each wall triangle a SMALL local box touching ONE
cap triangle → O(rim), not O(cap × rim). This is a contained change to the wall
re-triangulation path only.

## 3. Secondary hypotheses (to confirm/exclude in inc-0)

- **H2 — cap ⇄ wall rim subdivision mismatch (genuine T-junctions).** If
  `collect_edge_splits` propagates a subdivision to the wall that the cap's own
  overlay boundary does NOT carry (or vice-versa), the coarse side's edge spans
  the fine side's vertices → real T-junction arrangement events. Fix: make the
  two share ONE rim vertex set exactly.
- **H3 — near-duplicate sliver rim samples.** The overlay mints rim crossings
  ~4e-4 apart (profile-edge intersections at grazing angles). If these are
  sub-feature-size duplicates, deduping them removes whole sliver fans at the
  source. (Must not drop a genuine feature — gate on `MIN_FEATURE_SIZE`.)

These are not exclusive; H1 (fan) likely dominates but H2/H3 may co-contribute.

## 4. Increment plan (spec-first, gated, de-risked)

**inc-0 — precise pair-nature measurement + cheap A/B (NO production change).**
Re-add the reverted probes (`CHERCHI_PAIR_PROBE` selfA/cross buckets,
`CHERCHI_SELFA_DUMP`) plus a per-pair nature classifier for a sample of the
z≈1.52 `selfA`-transversal pairs: how many shared vertices (0/1/2), the exact
intersection type (empty / point / segment), and whether each triangle is a fan
sliver (traces to `fan_split_tri`). Then, behind an env gate
(`YANG_WALL_STRIP_TRI`), swap `fan_split_tri`'s corner fan for a local strip/
monotone triangulation and measure the `selfA` pair count delta on F0072.
**Decision gate:** proceed to inc-1 ONLY if the gated A/B drops `selfA`
materially (say ≥2×) AND the corpus is byte-identical gate-OFF. If H1 is
refuted (no drop), pivot to H2/H3 per the inc-0 findings — do NOT build H1
anyway.

**inc-1 — productionize the confirmed fix, gated off.** Implement the winning
lever (local wall triangulation, and/or conformal rim propagation, and/or
sub-feature sliver dedup) behind an env gate defaulting OFF, with unit tests on
hand-built fixtures (a wall quad with a coarse bottom + finely-subdivided top;
assert the re-triangulation is local — bounded per-triangle AABB — and still
exactly tiles the quad, area certificate). Gate-OFF corpus byte-identical.

**inc-2 — wire + prove.** Flip the gate ON. Gates:
- corpus **category-identical 255C/0W** (the move only re-triangulates a
  planar/wall region within its own boundary — no surface, topology, or curve
  change; it must not flip any verdict);
- cherchi **sidecar parity** green (the mesh handed to Stage-2 changed, so the
  arrangement output must still match the C++ reference on the affected cases);
- `selfA` pair count on F0072 **materially lower** (the perf metric);
- the seven-crate `./scripts/test.sh rewrite` + `fast` green.
Then remove the gate (or leave it as a dev A/B knob, the `weld_enabled`
pattern), update `docs/yang_deviations.md` and the roadmap.

## 5. Correctness posture (why this is safe)

The change re-triangulates a **planar cap region and its adjacent wall faces
within their existing boundaries** — it inserts/rearranges interior triangles
only; it introduces NO new surface, NO new B-Rep vertex on a rim it did not
already carry, and NO topology change. The overlay's exact coverage certificate
(`coplanar_overlay.rs` area identity) and the existing `triangulate_ring` /
`fan_split_tri` exactness carry over: the new wall triangulation must satisfy
the same exact-area tiling post-condition (a unit-test gate). So it is a mesh
**quality/conformality** change, not a geometry change — the P9/P10 hazard
(tolerance widening / wrong-reason fixes) does not apply. If any corpus verdict
moves, that is a real regression and the increment aborts (it means the wall
re-triangulation was NOT within-boundary as claimed).

## 6. Risks / open questions

- **Exact per-pair nature not yet pinned** (hand-analysis of the dump was
  inconclusive — the coarse-cap × fan-sliver intersections did not resolve
  cleanly to T-junctions vs near-degenerate crossings). inc-0's measurement is
  the gate; do not skip it. This is the third mechanism refinement on #199 —
  measure before building.
- **A local wall triangulation of a coarse-bottom / fine-top trapezoid** still
  needs SOME triangle bridging the width; a monotone triangulation bounds this
  far better than a single-corner fan but is not zero. Verify the AABB
  reduction empirically (inc-0), not just structurally.
- **Sidecar parity** is the hard gate: changing the Stage-2 input mesh must not
  diverge the arrangement output from the C++ reference on any parity case.
- If H1 is refuted and the driver is H2 (genuine T-junctions), the fix moves to
  `collect_edge_splits` conformal propagation — larger and closer to the #146
  junction-sampling family.

## 6a. inc-0 RESULTS (2026-07-23, probe-only — MEASURED, probe reverted)

The probes of §7 were re-added and run on F0072 (`single_case`, `--release`,
`multiplier` was 1–2, coords reported in model space). Verdict unchanged
(honest ERROR at Extrude 11 — the probe is read-only). **The measurement pins
the mechanism exactly and REFUTES H1-as-scoped (wall re-triangulation alone).**

**Pair-nature (heavy op, 666,731 pairs — the 132 s driver):**

```
tris=14342  pairs=666731
  selfA T: n=342849  sv0=303773 (89%)  sv1=33117  sv2=5959  slivers=276441 (81%)
  cross  T: sv0=33181 (73%)   merged T: sv0=34448 (74%)   (all sliver-heavy)
  top-z: z≈1.52:328336  z≈1.48:158237  z≈1.56:102912   (one thin contact slab)
```

The light ops are `sv1`-benign (single-shared-vertex adjacency); **the explosion
op is `sv0`-dominant (89 % share ZERO vertices) and 81 % sliver** — all in the
z≈1.52 contact slab.

**Geometry (`CHERCHI_SELFA_DUMP=1.52`) — the confirmed mechanism:**

- **`ta`** (coarse side, in every pair): all three verts at z=1.5412 → a
  **horizontal cap triangle** spanning the FULL cap height (y −0.28→+0.28),
  `min_edge≈0.078`. The cap INTERIOR is coarsely triangulated (few big tris).
- **`tb`** (the slivers): all three verts at y=−0.28 → **vertical wall
  triangles**, each with apex at the SAME single bottom corner
  (−0.356, −0.28, 1.3642) and base = a consecutive ~4e-4-wide top-rim segment
  (x=−0.097, −0.098, −0.101, …). `min_edge≈1e-4…2e-3`.

The wall face is a **single-corner apex fan** (produced by
`triangulate_ring`'s B1 apex-fan for the PLANAR gear/polygon wall — **NOT**
`fan_split_tri`, which only handles curved walls and is not exercised here).
Each fan sliver's AABB spans apex→rim = **0.26 wide** despite a 4e-4 base, so it
overlaps EVERY coarse cap triangle → **O(cap_tris × rim_samples)**. `sv0`
because the fan apex (z=1.3642) is not a cap vertex (z=1.5412).

**Analytical A/B (zero corpus risk — no triangulation change; local-AABB
survival test):** of the selfA-T sliver pairs, how many overlaps survive if a
triangle is re-triangulated LOCALLY (footprint = its short edge × full height)?

| lever | heavy-op survivors | reduction |
|---|---|---|
| **wall-local** (shrink wall sliver only) | 69.5 % | **1.4×** — below gate |
| **both-local** (shrink cap AND wall) | 26.8 % | **3.7×** — clears ≥2× |

Invariant across all dense ops: **wall-only 1.2–1.5× (never ≥2×); both-sides
1.9–3.7×** (≥2× on the heavy ops). 

**CONCLUSION — H1 (wall-only strip/monotone) is REFUTED by the decision gate**
(1.4× < 2×): a local wall triangle still collides with the coarse cap interior
triangle above it. **The `≥2×` win requires making BOTH the wall AND the cap
interior conformal to the shared fine rim** (the §4.5.5 "real lever" H2). This
was measured WITHOUT building the wall swap — the analytical A/B refuted H1's
premise first, per "measure before building / do NOT build H1 anyway".

**inc-1 (revised) — conformal contact-region re-triangulation (H2).** In
`build_stage0_mesh`, when a coplanar contact subdivides a shared rim, make the
re-triangulation of BOTH the adjacent wall face AND the coplanar cap face
LOCAL to the rim subdivision, so every contact triangle's AABB is bounded to
its rim segment (× wall height / × cap depth). Concretely: the coarse cap
interior must carry the fine rim samples on its boundary AND be triangulated so
no single triangle spans the whole cap (a strip/windowed triangulation keyed to
the rim samples, or an interior grid), and the wall must be triangulated
locally against a bottom edge that carries matching subdivision (or a windowed
top↔bottom strip). Spec-first, gated, corpus category-identical 255C/0W,
cherchi sidecar parity green. **H3 (near-duplicate ~4e-4 rim-sample dedup) is a
co-lever** — it shrinks the rim-sample count that both sides fan over (the
slivers are 81 % of the pairs); measure its standalone contribution in inc-1.
The pure-perf alternative (cherchi dense-slab short-circuit, Direction B) is
unaffected by this refutation and remains open.

## 7. Instrumentation (re-add for inc-0, revert before each commit)

- `CHERCHI_PAIR_PROBE` — post-dedup tris/pairs, Transversal/Coplanar ×
  selfA/selfB/merged/cross buckets, top-z (cherchi `soup.rs` after
  `classify_all`).
- `CHERCHI_SELFA_DUMP` — selfA-transversal pair coords, z-slab filtered.
- New for inc-0: per-pair shared-vertex count + intersection-type + fan-sliver
  provenance tag.
- `YANG_WALL_STRIP_TRI` — the gated fan→strip A/B switch.

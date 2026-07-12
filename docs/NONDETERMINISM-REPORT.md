# Nondeterminism Audit — Hash-Collection Iteration in the Live Kernel Stack

**Status:** current as of 2026-07-12 (design-review remediation F14).
**Scope:** `crates/yang-rs`, `crates/kernel-v2`, `crates/cherchi-rs` production code
(excludes `#[cfg(test)]`, `tests_unit/`, `*test*` files).

> **Historical note.** The prior version of this file audited the `truck-shapeops`
> boolean kernel and its raw-pointer `truck_base::id` identity model. That kernel
> was **DELETED** at the Phase 6 migration (2026-06-11, see root `CLAUDE.md`). None
> of its findings (pointer-derived IDs, `truck_shapeops::transversal` hotspots,
> the `DetContext`/`DetId` remediation spec) apply to the current stack, which is a
> clean-room Rust port with no pointer-based identity. This report replaces it in
> full.

## Governance basis

A4.2 / Engineering Constitution §8 forbid nondeterministic iteration **where it
affects output**. The concern is cross-process / run-to-run divergence: a rebuild
of the same model on a different process or machine must produce byte-identical
topology (the assay corpus is asserted "byte-stable" — see MEMORY).

## The risk surface: std hash collections only

The stack uses `std::collections::HashMap` / `HashSet` (SipHash with per-map
`RandomState`). It has **no** `rustc_hash`/`FxHashMap`, `indexmap`, or `ahash`
dependency (confirmed absent from all three `Cargo.toml`s). Two properties of std
`RandomState` matter here:

1. **Cross-process order is arbitrary.** The base seed is drawn from the OS at
   first use per thread, so iteration order over the same insertion sequence
   differs on every process launch. This is the real determinism hazard.
2. **Same-process order is ALSO not repeatable between two maps.** `RandomState::new()`
   bumps the seed counter per map, so two `HashMap`s built from an identical
   insertion sequence *in the same process* iterate in **different** orders.
   Verified empirically (12-key map, same process):
   `A = [10,2,5,0,4,6,7,1,11,8,9,3]` vs `B = [2,5,11,6,1,4,9,10,0,3,7,8]`.
   This corrects a common assumption ("same-process replay wouldn't vary the
   order") and, as noted below, means the existing in-process determinism test is
   *not* blind to hash-order divergence.

The codebase is already heavily biased toward determinism: production code uses
**517 `BTreeMap`/`BTreeSet`** references vs **121 `HashMap`/`HashSet`**. BTree
collections iterate in sorted key order and are not a risk; the audit concerns
only the std hash collections.

## Inventory (production iteration sites)

Method: for each crate, identify every std `HashMap`/`HashSet` (not BTree), then
find every place it is *iterated* (`.iter()`, `.values()`, `.keys()`,
`.into_iter()`, `.drain()`, `.retain()`, `for x in &coll`, or `.collect()` from
one of those) — as opposed to point access (`.get`/`.insert`/`.contains`/`.entry`),
which cannot leak order. Each iteration site is classified:

- **SAFE** — order cannot affect output (commutative reduction; filling an
  order-independent map/set; membership/length only; sorted immediately after).
- **LAUNDERED** — order-dependent locally but canonicalized downstream before it
  affects output.
- **RISK** — a geometric/topological decision rides on iteration order with no
  downstream canonicalization.

### Counts per crate

| Crate | std hash collections | actual iteration sites | SAFE | LAUNDERED | RISK |
|---|---|---|---|---|---|
| yang-rs | 22 | 6 | 5 | 0 | **1 (latent)** |
| kernel-v2 | 5 (production) | 1 | 1 | 0 | 0 |
| cherchi-rs | 5 (production) | 3 | 3 | 0 | 0 |
| **total** | **32** | **10** | **9** | **0** | **1** |

The great majority of hash collections (≈22 of 32) are **never iterated** — they
are pure O(1) lookup / dedup / union-find side-tables accessed only by key. Those
cannot leak iteration order and are SAFE by construction.

### Why the hot paths are clean

- **yang-rs `boolean.rs`** — all six hash collections are union-find / spatial-weld
  lookup tables (`grid`, `parent`, `first`, `seen`). None are iterated; the
  union-find always re-points to the `min` index as representative, so the chosen
  representative is order-independent.
- **yang-rs `stage4_correct.rs`** — the relocation maps (`vert_*`) are all
  `BTreeMap`. The one non-probe hash iteration (`by_triple.values()`, membrane
  cancellation, ~L447/461) collects victims into a `BTreeSet` and rebuilds the
  survivor set by a range filter, so the result is set-based and order-independent.
  Four other `by_triple`/`by_pos` iterations are `eprintln!` under the
  `YANG_DOUBLECOVER_PROBE` env gate (no output effect).
- **yang-rs `stage1_tessellate.rs`** — hash uses (`local_of_global`,
  `pos_to_global`) are intern tables; local ids are assigned by `Vec::len()` in
  deterministic polyline order, never by hash iteration.
- **kernel-v2 `boolean.rs`/`recover.rs`/`validate.rs`/`arena.rs`/`introspect.rs`** —
  contain **zero** std hash types; entirely `BTreeMap`/`BTreeSet`. The only
  production hash iteration in the crate (`adapter.rs:920`,
  `positions.keys().collect()`) is `k.sort()`-ed on the very next line before use.
- **cherchi-rs arrangement/labeling** (`aux_structure.rs`, `soup.rs`,
  `coplanar_propagate.rs`, `labeling/*.rs`) — the determinism-critical
  triangle/vertex ID assignment and label propagation use `BTreeMap`/`BTreeSet`
  exclusively. The three production hash sites (`triangulation/mod.rs` flood-fill
  `exterior` sets, `fast_trimesh.rs` `rev_vtx_map`, `enforce.rs`
  `constraint_planes`) are membership/lookup only; triangle emission walks spade's
  deterministic `inner_faces()` order and is additionally canonicalized via
  `rotate_min_first` + `sort_unstable` before return.

## The one RISK site (latent — not currently reachable)

**`crates/yang-rs/src/stage4_update.rs:162`** — in `stage4_mesh_update`, the
nearest-unclaimed-boundary-vertex pick:

```rust
let boundary_set: std::collections::HashSet<u32> = /* L146 */ ... .collect();
...
let bv = boundary_set
    .iter()
    .map(|&i| i as usize)
    .filter(|&i| !claimed[i])
    .map(|i| (i, dist2(q, patch.verts[i])))
    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());   // L162–167
if let Some((bi, bd)) = bv {
    if bd <= tol2 {
        claimed[bi] = true;
        poly_vidx.push(bi as u32);   // feeds CDT constraint edges
        continue;
    }
}
```

**Failure scenario.** When a polyline point `q` is at bit-identical `dist2` to two
distinct unclaimed boundary vertices (a symmetric patch — e.g. `q` on the
perpendicular bisector of two boundary verts), `Iterator::min_by` returns the
*first* minimum in iteration order. `boundary_set` is a std `HashSet`, so its
iteration order differs run-to-run. Run A claims vertex `i1`, run B claims `i2`.
The claimed index (a) is pushed into `poly_vidx`, which becomes a CDT
constraint-edge endpoint downstream, and (b) sets `claimed[bi]=true`, changing the
candidate pool for every later polyline point. Two orders → different constraint
set → different triangulation → **a different Stage-4 output mesh**. No downstream
canonicalization: `poly_vidx` is consumed directly by the constrained
triangulation.

**Why it is latent, not live.** `stage4_mesh_update` (def L89) has **no production
caller** — a cross-crate grep finds all 24 call sites at line ≥469, past the
`#[cfg(test)]` boundary at L400. It is the banked/unwired N2 mesh-update
prototype; the live Stage-4 path returns a typed `LocalRefinementRequired` error
instead of running it (see the N2 memory trail). The divergence also only
triggers on an exact distance tie; generic inputs pick a unique nearest vertex
deterministically. So this cannot affect kernel output today — but it will the
moment N2 is wired in, and it should be fixed *before* that wiring.

The sibling interior-vertex pick at L179 already iterates the deterministic
`(0..patch.verts.len())` range, so only the boundary pick needs the fix.

## What `assay_determinism` actually covers

`scripts/test.sh run_assay` runs `crates/test-harness/tests/assay_determinism.rs`,
which drives `assay::determinism::check_determinism`. Behavior:

- Generates random 2-body box/circle extrude-then-boolean scenarios via proptest
  (10 cases default, 50 in `assay-deep`), runs each scenario **3× in-process**,
  and compares the `(V, E, F)` topology **counts**.
- Because each in-process run allocates fresh `HashMap`s with a fresh per-map
  seed (see property 2 above), the 3 runs *do* exercise different hash-iteration
  orders. So this is a genuine differential check — it is **not** blind to
  hash-order nondeterminism, contrary to the naive "same process ⇒ same order"
  assumption.

**But the coverage has real gaps:**

1. **It compares counts, not geometry.** `(V, E, F)` equality misses any
   order-dependent divergence that preserves counts but moves a vertex, flips a
   valid-but-different face partition, or reorders emission. The RISK site above
   is exactly this class — a tie that changes *which* vertex is claimed can keep
   V/E/F identical while producing a different mesh. The sibling
   `boolean_determinism.rs` compares face count + volume with a generous `< 1.0`
   tolerance, which similarly masks small geometric drift.
2. **Only 3 runs.** Low probability of hitting the specific permutation that flips
   a tie-break, even when one exists.
3. **Corpus is simple 2-body box/circle booleans.** It does not exercise the
   complex multi-surface-junction / Stage-4 refinement paths where the RISK site
   lives (and which are the interesting determinism surface).
4. **All in-process.** No test spawns a separate process or compares across
   machines/architectures (`rebuild_stability.rs`, `assay_chain_determinism.rs`,
   and `boolean_determinism.rs` are all in-process; `fork: false` in the chain
   test is a proptest option, not process forking). Cross-process is the primary
   hazard from std `RandomState`, and no test asserts it directly — the in-process
   seed variation is a decent proxy but not a guarantee.

## Recommendations (no code changes made)

1. **Fix the latent RISK before N2 wiring.** In `stage4_update.rs:162`, give
   `min_by` a total tie-break on vertex index, e.g.
   `.min_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)))`, and/or
   iterate a sorted snapshot of `boundary_set` rather than the `HashSet` directly.
   Swapping `boundary_set` to `BTreeSet<u32>` also closes it (the set is small and
   this path is not hot). Track it against the N2 mesh-update milestone so it lands
   in the same PR that wires `stage4_mesh_update`.
2. **Strengthen the determinism oracle to a canonical geometry digest.** Compare a
   stable digest — quantized coordinates (integer `round(x / tol_sort)`) plus
   sorted topology (edges/faces by canonical vertex tuples) — instead of `(V,E,F)`
   counts. This is what would actually catch an order-dependent geometric flip.
3. **Add a cross-process digest test.** Run a fixed complex scenario (or the full
   kv2 assay corpus) in *separate process invocations* and diff the canonical
   digest. This directly asserts the property std `RandomState` threatens and that
   no in-process test can fully guarantee. A small harness that shells out to a
   test binary N times and compares stdout digests is sufficient.
4. **Optional hardening — lint against new hash iteration in kernel paths.** The
   stack is already ~94% BTree in these crates; a clippy/CI guard that flags
   `.iter()`/`.values()`/`.keys()` on a `std::collections::HashMap`/`HashSet` in
   `yang-rs`/`kernel-v2`/`cherchi-rs` production code would keep the surface at
   zero without per-audit re-inventory. (yang-rs `CLAUDE.md` hard-rule 7 already
   mandates single-threaded execution for determinism; this extends the same
   intent to iteration order.)

These are recommendations only; no production `.rs` code was modified by this audit.

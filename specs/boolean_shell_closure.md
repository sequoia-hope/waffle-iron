# Boolean Shell Closure

**Status:** Implemented (Sprint 27)
**Crates:** `truck-shapeops`, `kernel-fork`
**Tests:** `boolean_shell_closure.rs`, `extrude_chains.rs` (K1, K8, L4, M6)

## Problem

After chained boolean operations (boss → cut, multiple unions → cut), `Solid::try_new` rejects the result shell because some edges have != 2 face references (open boundaries). The shell is geometrically correct but topologically broken.

### Root causes

1. **Non-deterministic iteration**: `FxHashMap` in `weld_coincident_edges` Phases 1 and 3 uses pointer-based hashing. Iteration order varies between runs, causing different canonical edge selections. After Sprint 26 fixed polyline non-determinism, the remaining non-determinism in edge welding became the blocking issue.

2. **Incomplete edge welding**: After the first boolean, some edges are geometrically coincident (same vertex positions and curve midpoint) but remain topologically distinct. When the solid enters a second boolean, these unclosed edges cause `finalize_boolean_shell` to fail.

## Solution

### 1. Deterministic iteration (BTreeMap)

Replace `FxHashMap`/`FxHashSet` with `BTreeMap`/`BTreeSet` in `weld_coincident_edges` Phases 1-3. This requires `Ord` on `ID<T>` (the truck identity type), which is derived from the inner `usize` field.

**Files:**
- `vendor/truck/truck-base/src/id.rs` — Add `PartialOrd`, `Ord` to `ID<T>`
- `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` — Replace hash containers

### 2. Targeted open-edge re-weld

After the wider-tolerance retry loop in `finalize_boolean_shell`, add a targeted step:
1. Call `diagnose_open_edges()` to find edges with != 2 face references
2. For each pair of open edges (face_count == 1), check if they're geometric twins (same vertex positions and midpoint within tolerance)
3. Replace the duplicate with the canonical edge in all face wires
4. Retry `Solid::try_new`

This is more surgical than blanket `weld_coincident_edges` retries.

### 3. Asymmetric scale perturbation

Add asymmetric scale perturbation to `try_boolean_with_perturbation` in `kernel-fork/src/healing.rs`. When tool edges exactly overlap target edges, scale the tool slightly along individual axes to break edge alignment.

## Invariants

1. After `weld_coincident_edges`, every edge in the shell must appear in exactly 2 faces (Closed condition)
2. `diagnose_open_edges()` returns empty for any valid solid
3. `finalize_boolean_shell` must produce a valid `Solid` or return a structured `BooleanStageError`
4. Edge welding iteration order must be independent of memory allocation order

## Test coverage

- `boolean_shell_closure::shell_closure_boss_then_cut` — simplest failure case
- `boolean_shell_closure::shell_closure_two_bosses_then_cut` — multi-union topology
- `boolean_shell_closure::shell_closure_boss_cut_boss_repeatable` — 3x repeatability
- `boolean_shell_closure::shell_closure_overlapping_cuts` — coplanar pocket handling
- `extrude_chains::k1_boss_cut_boss` — boss → cut → boss
- `extrude_chains::k8_three_bosses_then_three_cuts` — 3 unions then 3 cuts
- `extrude_chains::l4_boss_on_top_cut_through_boss` — cut through compound body
- `extrude_chains::m6_three_overlapping_cuts` — overlapping coplanar cuts

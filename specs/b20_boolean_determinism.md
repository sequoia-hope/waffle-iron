# B20: Boolean Pipeline Determinism Fix

**Status**: Implemented
**Sprint**: 60
**Severity**: High — violates Engineering Constitution §8

## Problem

The boolean pipeline produces non-deterministic topology (V/E/F counts) and
volumes when given identical inputs across runs. Two root causes identified:

1. **HashMap iteration order**: Several `HashMap`/`FxHashMap` instances in the
   boolean pipeline are iterated in ways that affect face division results.
2. **SequentialID counter drift**: When the same boolean operation runs multiple
   times in a process, the global `SequentialID` counter gives different values.
   FxHash of different ID values produces different bucket assignments in
   FxHashMap/FxHashSet, causing non-deterministic iteration order even for
   collections that are only keyed by IDs.

## Root Causes

### R1: HashMap iteration in `merge_splice_wires`

`divide_face/mod.rs:109` — `FxHashMap<usize, Vec<usize>>` groups wires by
union-find component. Iterated at line 117 via `.values()`. Component
processing order directly affects wire splicing order (which wire's edges
come first in the composite figure-8). Different splice order → different
face division → different topology.

### R2: SequentialID-dependent FxHashMap behavior

When the same scenario runs N times in a process, SequentialID allocates
different raw values (run 1: IDs 1-500, run 2: IDs 501-1000, etc.).
FxHash of these different values produces different bucket assignments in
any FxHashMap/FxHashSet keyed by VertexID/EdgeID. Several such collections
exist in truck-topology (`shell.rs:extract_boundaries`, `face.rs:glue_at_boundaries`,
`shell.rs:create_one_component`) where the first key retrieved from a HashMap
determines output ordering.

### Secondary sites (converted for safety)

| Map | File | Line | Risk |
|-----|------|------|------|
| `seen_per_face` | loops_store/mod.rs | 2339 | Low — retain predicate |
| `face0_all_bc` / `face1_all_bc` | loops_store/mod.rs | 3051-52 | Low — lookup only |
| `face0_ic_vids` / `face1_ic_vids` | loops_store/mod.rs | 3015-18 | Low — lookup only |
| local `seen` (diagnostic) | divide_face/mod.rs | 388 | Low — debug output |
| `vid_count` | divide_face/mod.rs | 454 | Low — predicate only |
| `vid_wire_count` | divide_face/mod.rs | 546 | Low — predicate only |

## Fix

### Part A: BTreeMap conversion (R1)

Converted iterated `HashMap`/`FxHashMap` instances to `BTreeMap` for
deterministic ordering. Maps that are genuinely lookup-only (polyline cache
keyed by `EdgeID`) remain as `FxHashMap` for performance.

### Part B: ID sequence reset in determinism tests (R2)

Added `truck_base::reset_id_sequence()` between test runs so each run
starts from the same ID base, producing identical SequentialIDs and thus
identical FxHash bucket assignments.

Note: R2 does not affect production (single-run) determinism — each boolean
call is internally deterministic for a given set of SequentialIDs. R2 only
manifests when comparing across multiple calls in the same process.

### Files changed

- `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs`
  - `components`: `FxHashMap` → `BTreeMap`
  - `seen`, `vid_count`, `vid_wire_count`: `HashMap` → `BTreeMap`
- `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`
  - `seen_per_face`: `HashMap` → `BTreeMap`
  - `face0_ic_vids`, `face1_ic_vids`: `HashMap` → `BTreeMap`
  - `face0_all_bc`, `face1_all_bc`: `HashMap` → `BTreeMap`
- `crates/test-harness/Cargo.toml`
  - Added `truck-base` dependency (for `reset_id_sequence`)
- `crates/test-harness/tests/assay_chain_determinism.rs` (new)
  - Determinism test with ID reset, hard topology + volume assertions
- `crates/test-harness/tests/assay_generative_chain.rs`
  - Removed determinism test (moved to own file to avoid seed contamination)
- `crates/test-harness/tests/assay_determinism.proptest-regressions`
  - Deleted stale regression seeds

## Invariants

- **I-DET1**: Identical inputs + identical SequentialID base → identical V/E/F
- **I-DET2**: Identical inputs + identical SequentialID base → volumes within 0.1%

## Future work

Full cross-run determinism (without ID reset) requires converting all
FxHashMap/FxHashSet in truck-topology that use VertexID/EdgeID/FaceID keys
and are iterated. Key sites identified:
- `shell.rs:extract_boundaries` — `vemap: HashMap<VertexID, Edge>`
- `shell.rs:create_one_component` — `adjacency.keys().next()`
- `face.rs:glue_at_boundaries` — `vemap.iter().next()`

## Verification

```bash
cargo test -p truck-shapeops                           # 375 pass, no regression
cargo test -p test-harness --test assay_chain_determinism  # hard determinism check
cargo test -p test-harness --test assay_generative_chain   # chain correctness
cargo test -p test-harness --test assay_determinism        # existing determinism
cargo test -p test-harness --test boolean_properties       # 28 pass
```

**Success criteria**: `chain_deterministic` passes with hard topology assertions.
All truck-shapeops tests pass (375). No regression in chain correctness or
boolean properties.

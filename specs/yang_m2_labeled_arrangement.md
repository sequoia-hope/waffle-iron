# Spec: M2 — Patched sidecar emits a `LabeledArrangement`

**Status:** active (roadmap `docs/yang_functional_roadmap.md` M2)
**Feature cycle:** yang-m2
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Produce the **Stage-2 output** of the Yang pipeline from the interim C++ sidecar:
the full exact mesh arrangement plus, per arrangement triangle, which input
solid(s) it lies on, its inside/outside classification per input solid, and its
Cherchi patch id. Expose it to Rust as a `LabeledArrangement` and validate the
shape on two-cube (clean) and coplanar (multi-attribution) cases. This unblocks
M3 (yang-rs Stage 5/6 consuming real labels). M2 does **not** rewire yang-rs.

## Contract revision (research basis: source inspection)

The roadmap §2 originally specified per-triangle `source: (InputId,
parent_tri_index)`. Inspecting `/home/claude/cherchi2022/.../code/booleans.cpp`
shows the arrangement tracks provenance only at **input-solid** granularity
(`labels.surface[t]`, a `bitset<32>`); the input-triangle index is lost during
subdivision (daughters inherit the parent's `labels.surface` via `ts.triLabel`,
no parent-index pointer). Recovering it needs an invasive patch to arrangement
internals. **Decision (user-confirmed):** accept solid-level provenance — Yang
reassembles *faces*, and the face is recoverable from solid-id + plane-membership
in M3. §2 amended accordingly.

## The `LabeledArrangement` type (frozen by M2; lives in `cherchi-rs`)

```
InputId(u32)                          // input solid index; 0 = A, 1 = B

LabeledArrangement {
    mesh: Mesh,                       // FULL arrangement (all sub-tris, pre-filter)
    surface: Vec<Vec<InputId>>,       // per tri: which solid(s); len ≥1 (≥2 coplanar)
    inside:  Vec<Vec<bool>>,          // per tri: inside[k] = inside solid k
    patch:   Vec<u32>,                // per tri: Cherchi patch id
    num_inputs: u32,                  // 2 for a binary boolean
}
```
`surface`, `inside`, `patch` are each indexed 1:1 with `mesh.tris`. `Vec` (not
`SmallVec`) for M2; SmallVec is a later optimization. Provide a method
`keep_set(op: BoolOp) -> Vec<usize>` applying Cherchi's op keep-rules to the
arrangement (the validation oracle): union = `inside[t]` empty; intersection =
`(surface ^ inside).count() == num_inputs`; subtraction / xor per
`booleans.cpp` (cite line numbers). Computed from `surface`/`inside` — no
geometry, no tolerance.

## C++ patch design (D1)

Env-var-gated dump in `code/booleans.cpp::customBooleanPipeline`, inserted
**between `computeInsideOut(...)` (booleans.cpp:58) and the op filter (:60)** —
where `tm`, `labels.surface`, `labels.inside`, `patches` are all populated. Add
`#include <cstdlib>` + `#include <fstream>`. When `CHERCHI_DUMP_LABELS=<path>`:
1. Invert `patches` (`vector<flat_hash_set<uint>>`) → `tri_to_patch[t]` (every
   tri in exactly one patch).
2. `for t: tm.setTriInfo(t,1)`; `computeFinalExplicitResult(tm, labels,
   tm.numTris(), arr_coords, arr_tris, arr_surface, true)` → full explicit
   arrangement. Alignment is 1:1 by construction: that function iterates
   `t_id=0..numTris` and (all kept) emits tri `i` ⟺ `tm` tri `i` ⟺ `labels.*[i]`
   (booleans.cpp:1324-1337).
3. Write `<path>.obj` (`cinolib::write_OBJ(.., arr_coords, arr_tris, {})`) and
   `<path>.labels`: header `num_tris num_inputs` (=`labels.num`), then per tri in
   id order: set-bit positions of `surface[t]` `|` set-bit positions of
   `inside[t]` `|` `tri_to_patch[t]`.
The dump's `setTriInfo` is discarded by the op's `resetTrianglesInfo()`
(booleans.cpp:1397+), so normal output is unaffected. Patch is version-controlled
at `patches/cherchi2022_labeled_arrangement.patch`, applied idempotently by
`scripts/build_sidecars.sh` (sentinel + force rebuild).

## Branch / case table

| # | Case | Expectation |
|---|---|---|
| C1 | two overlapping cubes (non-coplanar) | every tri `surface.len()==1`, `inside.len()==2`; arrangement tris > input tris (subdivision) |
| C2 | keep-rule vs stock op | `keep_set(op)` triangle set == stock `boolean(a,b,op)` result, for union AND subtraction |
| C3 | coplanar overlap (cubes sharing a face) | ≥1 tri with `surface.len()==2` (multi-attribution) |
| C4 | determinism | identical inputs → byte-identical `.labels` / identical `LabeledArrangement` |
| C5 | binary absent | producer returns `Err(SidecarError::BinaryNotFound)`; tests self-skip |

## Invariants

- **I1:** `surface.len() == inside.len() == patch.len() == mesh.tris.len()`.
- **I2:** every `surface[t]` non-empty; every `inside[t].len() == num_inputs`.
- **I3 (acceptance oracle):** `keep_set(op)` reproduces the stock op result
  (C2) — ties the labels to the trusted boolean output (reference parity within
  the same binary). This is the M2 GREEN bar (roadmap §6).
- **I4:** coplanar case exhibits multi-attribution (C3) — proves the shape isn't
  wrongly scalar before freezing.
- **I5 (determinism):** C4.

## Oracles

- **Primary:** the patched C++ binary via `cherchi_sidecar_rs::labeled_arrangement`
  + the stock `boolean()` (both external, not weakenable — P9).
- **Secondary:** pure-Rust structural asserts (I1, I2).

## Failure modes

- Binary absent → `SidecarError::BinaryNotFound` (self-skip).
- Malformed/missing `.labels` → new `SidecarError::LabelsParse` (never silent).
- Input not Cherchi-clean → may hang; bounded by `run_with_timeout` (M1 harness).

## Research basis

- **Cherchi et al. 2022** §3 — "for each output triangle we propagate
  information on its origin" (the `labels.surface`/`labels.inside` bitsets + patch
  decomposition this patch surfaces); op keep-rules in `code/booleans.cpp`.
- **Yang et al. 2025** §4.4.2 — Stage 5/6 consume per-triangle surface labels +
  patches; this is the producer for that consumption (M3).
- No new tolerances introduced (only values Cherchi already computes — A14.3).

## Definition of Done (DoD §1)

Spec (this file) ✓; roadmap §2 amended; RED→GREEN with separate commits (P7:
C++ patch+build committed separately from Rust type+producer); every case
C1–C5 tested; structural + acceptance-oracle asserts (not "no panic"); patch
version-controlled + applied deterministically; determinism (I5); no test
weakened; clippy/fmt clean for touched Rust crates; no regression in siblings.

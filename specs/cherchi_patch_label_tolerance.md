# cherchi-rs — patch-label tolerance at coplanar-sheet borders (reference parity)

**Status:** spec (FIP Phase 1) — root cause localized by reference parity
2026-07-03. **Change class:** bug fix (modeling-related).
**Crate:** `cherchi-rs` (`labeling/patches.rs::compute_all_patches`).

## 1. Goal

Coplanar booleans whose arrangement produces a merged `[A,B]` overlap sheet
bordering a single-label region across a manifold edge must label patches
the way the reference implementation does, instead of failing with
`PatchError::LabelMismatch`. This is the post-canon wall of R0046 / R0088 /
F0063 (task #14) and the residual `LabelMismatch` class after the §2c
input-hygiene cycle disproved the femto-twin hypothesis.

## 2. Measured mechanism (reference-parity fork, R0046)

- R0046's exact post-Stage-0 `mesh_a`/`mesh_b` (OBJ dump at the backend
  call) fed to the C++ `mesh_booleans` sidecar: **C++ succeeds** — same
  210-triangle arrangement as the native port, same 8-triangle `[A,B]`
  coplanar sheet, 5 patches, valid boolean output, exit 0 on both ops.
- The native port fails at `patches.rs:118`: our port converted the C++
  `assert(labels.surface[t_id] == ref_l)` (booleans.cpp:449) into a hard
  error. In the RELEASE reference (NDEBUG) that assert is a NO-OP: the C++
  floods across the label boundary and the patch takes the SEED triangle's
  label (booleans.cpp:629). Confirmed from the C++ label dump: its patch 0
  mixes 108 `[A]` triangles with the 8 `[A,B]` sheet triangles.
- The label-homogeneity invariant is genuinely FALSE in the coplanar world:
  a merged overlap sheet's border to the single-solid region is a manifold
  edge (only 2 incident triangles — the B-side copies were DEDUP'd into the
  sheet), so the flood legitimately crosses it. The C++ debug assert
  reflects the pre-coplanar assumption; the release behavior is the
  reference semantics the port must match (crate hard rule 2: reference
  parity is the correctness oracle).

## 3. Branch table

| # | Configuration | Behavior |
|---|---|---|
| L1 | Flood stays label-homogeneous (all non-coplanar booleans) | Byte-identical patches (no behavior change) |
| L2a | Flood reaches a triangle whose canonical label is COMPATIBLE with the seed's (one label set ⊆ the other — the coplanar `[A,B]` sheet extending a single-input `[A]` region, either direction) | CONTINUE flooding (reference release semantics on the measured class); patch label = the SEED's label |
| L2b | Flood reaches a triangle with a DISJOINT label (neither ⊆ — e.g. `[A]` vs `[B]`) | Loud `LabelMismatch` (UNCHANGED) — a genuine arrangement corruption; deliberately STRICTER than the release reference (which would silently mix), per crate P9 doctrine and the deviation policy (safe-direction, documented) |
| L3 | `labels.len() != tris.len()` | Unchanged loud `InputMismatch` |

(2026-07-03 amendment, implementer delta #3: L2 split into L2a/L2b —
subset-compatibility instead of blind tolerance. The I1 output-parity gate
validates the choice; the C++ debug assert would fire on BOTH sub-cases,
release tolerates both; we match release exactly where correctness is
proven (L2a) and stay loud where it is not (L2b).)

## 4. Invariants / correctness argument (why tolerance is not a P9 hack)

The downgrade of a loud guard requires proving the tolerant behavior
CORRECT, not merely non-crashing:

- I1 (reference output parity — the binding oracle): for R0046, R0088,
  F0063's failing boolean calls, the native backend's boolean OUTPUT
  matches the C++ sidecar's on the same inputs (the established
  canonicalized sidecar-parity comparison used by the M6/M7 suites), or
  where full output parity is impractical for a case, the case passes the
  full mesh-oracle suite end-to-end.
- I2 (keep-rule independence): the mixed patch's label is consumed by the
  per-patch inner/outer ray-cast (BL2); coplanar `[A,B]` sheet triangles'
  keep decisions are made by the per-triangle coplanar rules
  (booleans.cpp:1430/1468 equivalents), not by the patch label — the cycle
  verifies this by the I1 parity plus the existing coplanar boolean suites
  (coplanar_pocket_parity, m8_disc_coplanar) staying green.
- I3 (non-coplanar unchanged): patches, labels, and boolean outputs are
  byte-identical for label-homogeneous arrangements (L1) — the whole
  existing cherchi-rs + yang-rs suites and sidecar-parity tests gate this.
- I4 (E2E): R0046/R0088/F0063 lose the `LabelMismatch` wall (the
  m8_rim_clustering_campaign trackers flip); full assay 0 WRONG, no
  SUPPORTED_CORRECT lost.

## 5. Oracles

- Unit RED (cherchi-rs): a minimal soup with a merged 2-label sheet
  triangle manifold-adjacent to single-label triangles (derivable from the
  measured R0046 structure or built synthetically via the existing
  coplanar-arrangement test fixtures) → today `Err(LabelMismatch)`; after:
  patches partition the soup, the mixed patch carries the seed's label,
  and a NEW parity-style assertion pins the patch count/membership against
  the C++-measured expectation (5 patches for the R0046-derived fixture).
- Reference-parity RED (dev-only, `#[ignore]` + sidecar feature, per the
  established FFI-parity convention): R0046's banked Stage-0 meshes →
  native boolean output ≡ sidecar output (I1).
- E2E: the three m8_rim_clustering_campaign trackers (already committed,
  RED today for exactly this wall).
- Full existing suites (I3) + full assay (I4).

## 6. Failure modes

- A mixed-label patch whose seed label misleads the BL2 in/out
  classification would surface as an output-parity failure (I1) or a mesh-
  oracle failure — both loud in the cycle's gates; if measured, STOP and
  re-diagnose (the fix would then need the C++'s exact downstream keep
  semantics ported deeper, not a guard change).
- The removed hard error masks genuinely-corrupt upstream labels: L3 plus
  the arrangement's own validation remain; the deviation note must record
  that the debug-build C++ would assert where we now proceed (parity with
  RELEASE reference, documented in `docs/yang_deviations.md`).

## 6a. Parity-fork deltas (implementer, 2026-07-03)

- `mesh_booleans_inputcheck` on the Stage-0 operands reports
  `Local Orientation: failed` + `Intersection: failed` (Global passes, no
  loop). Compatible with the primary mechanism (the dedup'd sheet border);
  flags the ALTERNATIVE, DEEPER fix: a yang-rs Stage-0 emitting
  inputcheck-clean overlap meshes (AOnly/Overlap border as a shared
  non-manifold intersection edge on both solids) would remove mixed-label
  patches at the source. Out of scope here; recorded for the M8 general
  Stage-0 work.
- Source-confirmed: `propagateInnerLabelsOnPatch` (booleans.cpp:1336)
  writes `labels.inside` only, never `labels.surface` — the dumped mixed
  patch is a genuine patch-time mix, not a post-hoc relabel (strengthens
  I2's framing).

## 7. Research basis

Cherchi 2022 §5 (labeling/patches); the reference implementation's release
behavior IS the shipped, published semantics (crate hard rule 2). The
port's hard-error was a deviation that predated coplanar-overlap support
(N13 fully-coplanar work made `[A,B]` sheets reachable); this cycle
retires it with the parity evidence. Precedent: N20 (C++ t=0 touch-hit
mislabeling) for reference-behavior analysis; the deviation-policy memo
(`cherchi_rs_cpp_deviation_policy`).

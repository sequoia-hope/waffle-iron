# N4 retirement — Stage-6 geometric face resolution demoted to the no-provenance oracle path

Status: IN PROGRESS (task #53). Follow-up to the N4 provenance attribution
(§4.2.3) and the `YANG_N4_FALLBACK_PROBE` measurement: **zero fallback hits
across the full corpus** on the native backend — the geometric
centroid-proximity path is dead wherever provenance exists.

## 0. Goal

A kept arrangement triangle whose provenance attribution MISSES (its
`source` entry is absent, unmapped, or the `u32::MAX` sentinel) on a
provenance-carrying arrangement must FAIL LOUDLY
(`FaceResolutionFailed { tri }`) instead of silently falling back to
geometric centroid-proximity resolution. A silent fallback can
misattribute (the exact failure class N4 was built to eliminate) and
masks provenance regressions in Stage-0/Stage-1 producers.

The geometric path is NOT deleted: producers that emit **no provenance at
all** (`la.source.is_empty()`) keep it — that is the documented contract
of the dev-only C++ sidecar oracle (`cherchi-sidecar-rs` emits
`source: Vec::new()`), and reference parity (roadmap §6,
`tests/backend_parity.rs` runs FULL booleans through the sidecar when
`CHERCHI2022_BIN` is set) is not optional. The in-crate M3/M4 mock-label
fixtures are source-less for the same reason and continue to exercise the
geometric path (including its degenerate-sliver handling and the F2/F3
loud-error contracts).

## 1. Parameters

No public API change. `YangError::FaceResolutionFailed { tri }` (existing
variant) is returned for the new loud case.

## 2. Branch table

| Arrangement `source` | Input lineage (`tri_face`) | Per-tri provenance | Today | After |
|---|---|---|---|---|
| non-empty (native backend) | present | hit | provenance attribution | unchanged |
| non-empty | present | MISS (NoSourceEntry / NoMap-too-short / Sentinel) | silent geometric fallback | **`FaceResolutionFailed { tri }`** (probe eprintln retained, env-gated) |
| non-empty | EMPTY (lineage-less: a yang boolean OUTPUT chained directly back in, or a `from_mesh` B-Rep — the F0066/yr27 direct-chaining pattern) | n/a (`ProvMiss::NoLineage`) | geometric resolution | unchanged (documented lineage-less path) |
| empty (sidecar oracle, mock fixtures) | any | n/a | geometric resolution | unchanged (documented oracle path) |

## 3. Invariants

- **I1 (loud miss):** with `source` non-empty, a triangle whose provenance
  cannot name a face errors typed — never a geometric guess.
- **I2 (oracle path intact):** source-less arrangements resolve exactly as
  before, byte-for-byte (sliver handling, tiered exact/band tie-break,
  F2/F3 errors).
- **I3 (no corpus movement):** measured zero misses ⇒ full corpus
  categories unchanged.

## 4. Oracles

- **Red/canonical:** a mock arrangement WITH `source` populated whose one
  triangle's entry names only the other input (NoSourceEntry) → today
  boolean() succeeds via geometry; after → `FaceResolutionFailed`.
- Sentinel variant: `source` maps the triangle to `u32::MAX` → same loud
  error.
- Existing M3/M4 mock tests (source-less) stay green unchanged.
- yang-rs suite + rewrite tier green; corpus spot checks unchanged.

## 5. Failure modes

`FaceResolutionFailed { tri }` — with the env-gated `[n4-fallback]`
diagnostic naming the miss reason (`ProvMiss`) retained for debugging.

## 6. Research basis

Yang 2025 §4.2.3 (provenance-based attribution). The retirement enforces
the paper's attribution as the sole production path; the geometric method
survives only as the oracle-input contract for provenance-less reference
backends (Cherchi 2022 sidecar).

## 7. Analytical vs. approximate

Not an SSI change. Attribution only.

## 8. Design

In `boolean()` step (5): the `Err(reason)` arm of
`provenance_face_reason` returns `Err(YangError::FaceResolutionFailed)`
after the (env-gated) diagnostic, instead of falling through. The
geometric block is reached only under `la.source.is_empty()`.
`docs/yang_deviations.md` N4 sign-off updated to resolved.

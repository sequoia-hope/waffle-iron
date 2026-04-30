# PR3 Corpus Dump — Per-case Bijective Oracle Results (Yang Fast)

Adversary T3 deliverable. Per-case `check_face_pair_bijective` measurement
across the yang_fast assay corpus (190 cases minus 33 known-timeout skips
= 157 cases). Output sorted by `gate_class`, then descending
`non_bijective_pairs`. Companion to `specs/tessellation_bounded_residuals.md`.

## How this was produced

A temporary measurement binary (NOT committed) replicated the assay-runner
flow per case:

1. `discover_cases` over `app/tests/cases/assay/` (skip set matches `yang_fast`).
2. `ModelBuilder::kernel().load(waffle_json)`. If `engine_errors()` is
   non-empty, record the `?` gate class and the first error.
3. Tessellate the last non-suppressed feature's solid via `kernel.tessellate(handle, scale*0.01)`.
4. Run `kernel::tessellation::bijective::check_face_pair_bijective(mesh, face_map, arena)`.
5. Classify the gate via `(is_polygon_soup, primitive_params, edge_geometry)`:
   - `primitive-dispatch` if any of `cylinder/revolve/sphere/cone/torus_params` is `Some`
     (these never enter the bounded path; they hit the per-primitive
     tessellator at `tessellation/mod.rs:242+`).
   - else `polygon-soup` if `is_polygon_soup`.
   - else `arc-fan` if any `edge_geometry` is `CurveGeom::Arc(_)`.
   - else `linear-bounded` (the watertight bounded path at
     `tessellation/mod.rs:217-235`).
6. Per-case 30s recv_timeout (orphaning the worker on timeout, matching
   yang_fast's behaviour).

Run env: `YANG_BOOLEAN=1`. `R0080`/`R0018` are nondeterministic and may
flip across runs. The raw output is what I observed on this run.

## Per-case table

Sorted by `gate_class` ascending, then `non_bijective_pairs` descending,
then `case_id` ascending.

| case_id | gate_class | total_pairs | bijective_pairs | non_bijective_pairs | note |
|---------|------------|-------------|-----------------|---------------------|------|
| F0042 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0054 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0055 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0058 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0060 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0061 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0062 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| F0073 | ? | 0 | 0 | 0 | engine errors: revolve self-intersection |
| F0074 | ? | 0 | 0 | 0 | engine errors: revolve self-intersection |
| F0086 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0004 | ? | 0 | 0 | 0 | engine errors: revolve self-intersection |
| R0005 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0007 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0016 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0019 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0020 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0022 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0023 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0025 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0031 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0032 | ? | 0 | 0 | 0 | TIMEOUT 30s |
| R0037 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0038 | ? | 0 | 0 | 0 | engine errors: yang_boolean: bijective oracle |
| R0041 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0043 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0046 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0047 | ? | 0 | 0 | 0 | engine errors: yang_boolean: bijective oracle |
| R0049 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0050 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0051 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0055 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0058 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0061 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0063 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0071 | ? | 0 | 0 | 0 | TIMEOUT 30s |
| R0075 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0076 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0078 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0079 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0081 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0087 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0088 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0092 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0093 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0094 | ? | 0 | 0 | 0 | engine errors: yang_boolean: triangle-plane |
| R0095 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0096 | ? | 0 | 0 | 0 | engine errors: yang_boolean: result validation failed |
| R0044 | linear-bounded | 1344 | 1314 | 30 |  |
| R0098 | linear-bounded | 1454 | 1448 | 6 |  |
| F0024 | linear-bounded | 40 | 35 | 5 |  |
| F0020 | linear-bounded | 40 | 36 | 4 |  |
| F0025 | linear-bounded | 42 | 38 | 4 |  |
| F0021 | linear-bounded | 54 | 51 | 3 |  |
| F0022 | linear-bounded | 46 | 43 | 3 |  |
| R0057 | linear-bounded | 212 | 209 | 3 |  |
| R0064 | linear-bounded | 866 | 863 | 3 |  |
| R0033 | linear-bounded | 12 | 10 | 2 |  |
| F0023 | linear-bounded | 54 | 53 | 1 |  |
| R0060 | linear-bounded | 17 | 16 | 1 |  |
| R0067 | linear-bounded | 10 | 9 | 1 |  |
| R0086 | linear-bounded | 7 | 6 | 1 |  |
| F0001 | linear-bounded | 12 | 12 | 0 |  |
| F0002 | linear-bounded | 12 | 12 | 0 |  |
| F0003 | linear-bounded | 40 | 40 | 0 |  |
| F0004 | linear-bounded | 12 | 12 | 0 |  |
| F0005 | linear-bounded | 12 | 12 | 0 |  |
| F0006 | linear-bounded | 12 | 12 | 0 |  |
| F0007 | linear-bounded | 12 | 12 | 0 |  |
| F0008 | linear-bounded | 12 | 12 | 0 |  |
| F0009 | linear-bounded | 40 | 40 | 0 |  |
| F0010 | linear-bounded | 40 | 40 | 0 |  |
| F0011 | linear-bounded | 24 | 24 | 0 |  |
| F0012 | linear-bounded | 24 | 24 | 0 |  |
| F0013 | linear-bounded | 24 | 24 | 0 |  |
| F0014 | linear-bounded | 24 | 24 | 0 |  |
| F0015 | linear-bounded | 24 | 24 | 0 |  |
| F0016 | linear-bounded | 12 | 12 | 0 |  |
| F0017 | linear-bounded | 12 | 12 | 0 |  |
| F0018 | linear-bounded | 12 | 12 | 0 |  |
| F0019 | linear-bounded | 12 | 12 | 0 |  |
| F0031 | linear-bounded | 14 | 14 | 0 |  |
| F0032 | linear-bounded | 14 | 14 | 0 |  |
| F0033 | linear-bounded | 14 | 14 | 0 |  |
| F0034 | linear-bounded | 14 | 14 | 0 |  |
| F0035 | linear-bounded | 14 | 14 | 0 |  |
| F0036 | linear-bounded | 14 | 14 | 0 |  |
| F0037 | linear-bounded | 14 | 14 | 0 |  |
| F0038 | linear-bounded | 14 | 14 | 0 |  |
| F0039 | linear-bounded | 13 | 13 | 0 |  |
| F0040 | linear-bounded | 14 | 14 | 0 |  |
| F0044 | linear-bounded | 4 | 4 | 0 |  |
| F0051 | linear-bounded | 12 | 12 | 0 |  |
| F0053 | linear-bounded | 12 | 12 | 0 |  |
| F0064 | linear-bounded | 24 | 24 | 0 |  |
| F0066 | linear-bounded | 24 | 24 | 0 |  |
| F0076 | linear-bounded | 18 | 18 | 0 |  |
| R0001 | linear-bounded | 2018 | 2018 | 0 |  |
| R0002 | linear-bounded | 12 | 12 | 0 |  |
| R0006 | linear-bounded | 14 | 14 | 0 |  |
| R0008 | linear-bounded | 12 | 12 | 0 |  |
| R0009 | linear-bounded | 2 | 2 | 0 |  |
| R0011 | linear-bounded | 1188 | 1188 | 0 |  |
| R0013 | linear-bounded | 924 | 924 | 0 |  |
| R0014 | linear-bounded | 12 | 12 | 0 |  |
| R0017 | linear-bounded | 12 | 12 | 0 |  |
| R0018 | linear-bounded | 12 | 12 | 0 |  |
| R0027 | linear-bounded | 6 | 6 | 0 |  |
| R0029 | linear-bounded | 12 | 12 | 0 |  |
| R0036 | linear-bounded | 1344 | 1344 | 0 |  |
| R0039 | linear-bounded | 672 | 672 | 0 |  |
| R0042 | linear-bounded | 0 | 0 | 0 |  |
| R0045 | linear-bounded | 192 | 192 | 0 |  |
| R0048 | linear-bounded | 12 | 12 | 0 |  |
| R0052 | linear-bounded | 2016 | 2016 | 0 |  |
| R0056 | linear-bounded | 1848 | 1848 | 0 |  |
| R0062 | linear-bounded | 204 | 204 | 0 |  |
| R0068 | linear-bounded | 14 | 14 | 0 |  |
| R0069 | linear-bounded | 24 | 24 | 0 |  |
| R0072 | linear-bounded | 12 | 12 | 0 |  |
| R0073 | linear-bounded | 768 | 768 | 0 |  |
| R0074 | linear-bounded | 194 | 194 | 0 |  |
| R0077 | linear-bounded | 12 | 12 | 0 |  |
| R0080 | linear-bounded | 12 | 12 | 0 |  |
| R0082 | linear-bounded | 1514 | 1514 | 0 |  |
| R0083 | linear-bounded | 14 | 14 | 0 |  |
| R0084 | linear-bounded | 14 | 14 | 0 |  |
| R0091 | linear-bounded | 204 | 204 | 0 |  |
| R0097 | linear-bounded | 1692 | 1692 | 0 |  |
| F0026 | primitive-dispatch | 2 | 2 | 0 |  |
| F0027 | primitive-dispatch | 2 | 2 | 0 |  |
| F0028 | primitive-dispatch | 2 | 2 | 0 |  |
| F0029 | primitive-dispatch | 2 | 2 | 0 |  |
| F0030 | primitive-dispatch | 2 | 2 | 0 |  |
| F0041 | primitive-dispatch | 2 | 2 | 0 |  |
| F0043 | primitive-dispatch | 2 | 2 | 0 |  |
| F0045 | primitive-dispatch | 2 | 2 | 0 |  |
| F0046 | primitive-dispatch | 2 | 2 | 0 |  |
| F0047 | primitive-dispatch | 2 | 2 | 0 |  |
| F0048 | primitive-dispatch | 2 | 2 | 0 |  |
| F0049 | primitive-dispatch | 2 | 2 | 0 |  |
| F0050 | primitive-dispatch | 2 | 2 | 0 |  |
| F0052 | primitive-dispatch | 2 | 2 | 0 |  |
| F0056 | primitive-dispatch | 2 | 2 | 0 |  |
| F0057 | primitive-dispatch | 2 | 2 | 0 |  |
| F0059 | primitive-dispatch | 2 | 2 | 0 |  |
| F0075 | primitive-dispatch | 12 | 12 | 0 |  |
| R0015 | primitive-dispatch | 192 | 192 | 0 |  |
| R0021 | primitive-dispatch | 2 | 2 | 0 |  |
| R0024 | primitive-dispatch | 2 | 2 | 0 |  |
| R0030 | primitive-dispatch | 2 | 2 | 0 |  |
| R0034 | primitive-dispatch | 840 | 840 | 0 |  |
| R0035 | primitive-dispatch | 2 | 2 | 0 |  |
| R0040 | primitive-dispatch | 12 | 12 | 0 |  |
| R0054 | primitive-dispatch | 1596 | 1596 | 0 |  |
| R0066 | primitive-dispatch | 2 | 2 | 0 |  |
| R0089 | primitive-dispatch | 2 | 2 | 0 |  |
| R0090 | primitive-dispatch | 2 | 2 | 0 |  |

## Subtotals by gate_class

| gate_class | cases_total | cases_with_nb | total_nb_pairs |
|------------|-------------|---------------|----------------|
| ? (engine error or timeout, no measured tessellation) | 47 | 0 | 0 |
| linear-bounded | 81 | 14 | 67 |
| primitive-dispatch | 29 | 0 | 0 |
| arc-fan | 0 | 0 | 0 |
| polygon-soup | 0 | 0 | 0 |

Total: 157 cases (33 yang_fast skips excluded).

## Top non-bijective cases (PR4 anchor candidates)

Only the linear-bounded class has any non-bijective pairs in this run.
Sorted by descending nb_pairs:

| Rank | case_id | total_pairs | nb_pairs | nb-rate | Notes |
|------|---------|-------------|----------|---------|-------|
| 1 | R0044 | 1344 | 30 | 2.2% | Heaviest hitter — large mesh, many face pairs to inspect; longer to bisect but highest signal. |
| 2 | R0098 | 1454 | 6 | 0.4% | Large mesh, low rate. |
| 3 | F0024 | 40 | 5 | 12.5% | Small mesh, high rate — good simplicity-vs-signal tradeoff. |
| 4 | F0020 | 40 | 4 | 10.0% | Same shape class as F0024. |
| 5 | F0025 | 42 | 4 | 9.5% | Same shape class. |
| 6 | F0021 | 54 | 3 | 5.6% | Smaller-still F-series. |
| 7 | F0022 | 46 | 3 | 6.5% | Smaller-still F-series. |
| 8 | R0057 | 212 | 3 | 1.4% | Mid-size R-series. |
| 9 | R0064 | 866 | 3 | 0.3% | Large R-series, low rate. |
| 10 | R0033 | 12 | 2 | 16.7% | **Smallest mesh with multiple nb pairs** — single-digit total pair count. Likely simplest reproducer. |
| 11 | F0023 | 54 | 1 | 1.9% |  |
| 12 | R0060 | 17 | 1 | 5.9% | Tiny mesh. |
| 13 | R0067 | 10 | 1 | 10.0% | Tiny mesh. |
| 14 | R0086 | 7 | 1 | 14.3% | **Smallest total_pairs with non-zero nb** — 7 pairs total. |

### Recommended PR4 anchor

For a red test in the kernel suite, the simplest reproducers are
`R0086` (7 pairs total, 1 nb), `R0067` (10 pairs total, 1 nb), or
`R0033` (12 pairs total, 2 nb). Of these, `R0033` has the highest
nb-rate among the small cases — the simplest topology that exhibits the
defect with redundancy (two face pairs both wrong, increasing the chance
of stable reproduction).

For a higher-signal test, `R0044` and `F0024` exercise the mechanism at
materially different scales — `R0044` proves the defect can occur at
1344-pair scale; `F0024` proves it occurs in 40-pair F-series geometry
where the topology is canonical-axis enough for hand analysis.

**PR4 test-author can pick from `R0033`, `R0086`, `F0024`, or `R0044`.**

The "?" rows are NOT viable PR4 anchors — they error before tessellation,
so the bijective oracle never measures them.

## Comparison to PR2 adversary's earlier numbers

The PR3 brief cited PR2 adversary's count as `linear-bounded ~350 nb
pairs, arc-fan ~10, polygon-soup ~108`. This run produced
`linear-bounded 67 nb pairs, arc-fan 0, polygon-soup 0`. Possible
explanations:

1. **The PR2 adversary measured pre-PR2 state.** The PR2 fix
   (`f01dd68` — share cap-to-lateral boundary vertex IDs in revolve
   primitive) lifted many revolve-primitive cases from non-bijective to
   bijective, and likely shifted some from `arc-fan` to
   `primitive-dispatch` classification.
2. **The 47 `?` rows.** Cases that error before tessellation
   (`yang_boolean: result validation failed`, `revolve self-intersection`,
   etc.) are unreachable to the oracle in this run. If PR2's adversary
   measured a state where some of those errored cases instead succeeded
   into `polygon-soup`/`arc-fan` paths, those would have shown up.
3. **Gate-class semantics differ.** This run classifies by the same gate
   that `tessellation/mod.rs:217-235` uses. PR2's adversary may have
   classified differently.

This is a divergence to flag, not to resolve here. PR4 still has 14
linear-bounded anchor candidates with stable nb counts — sufficient for
the next cycle's red test.

## References

- `specs/tessellation_bounded_residuals.md` — PR3 investigation that pivoted scope.
- `specs/tessellation_bounded_gate.md` — PR1 spec characterizing the gate.
- `crates/kernel/src/tessellation/bijective.rs:317` —
  `check_face_pair_bijective` oracle (Yang §4.1.1).
- `crates/kernel/src/tessellation/mod.rs:217-235` — bounded-path gate.
- PR1 commit `5f5423c`. PR2 commit `f01dd68`. Pre-PR3 baseline
  `c4f0fcb`. PR3 pivot commit `8ad64b5`.
- `docs/audits/cherchi_port_audit.md` D-10 — `weld_mesh_vertices` removal
  blocked on bijective tessellation.
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6 — Yang hybrid pipeline.

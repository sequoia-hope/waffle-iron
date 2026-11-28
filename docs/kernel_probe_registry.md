# Kernel env-gated probe registry

**Design review 2026-07-12 F12.** The kernel stack carries ~68 distinct
`std::env::var`-gated probes across `yang-rs`, `kernel-v2`, `cherchi-rs`, and
`test-harness`. They are per-campaign bisection scaffolding. This registry
records the policy and, crucially, enumerates the ones that **change geometry
output** so they are auditable.

## Policy

- **Tracing probes** (the large majority) gate `eprintln!` diagnostics ONLY —
  they never alter a geometric or topological decision. They default off
  (unset ⇒ silent) and are safe to leave in the tree, though probes belonging
  to shipped tasks may be swept opportunistically.
- **Behavior-altering neuters** are dev-only bisection kill-switches that DO
  change output. Since F12 they are gated with `cfg!(debug_assertions)`, so
  they are **compiled to the default (correct) path in release / WASM builds**
  and cannot be flipped by a stray environment variable in production. Use them
  only in debug builds when bisecting; the assay runs in release and never
  depends on them.

## Behavior-altering neuters (release-gated)

| Env var | Site | Effect when set (debug only) | Default (always in release) |
|---|---|---|---|
| `TIEBREAK_NEUTER` | `yang-rs/src/stage1_tessellate.rs:~1257` | forces the tessellation tiebreak comparator to `Equal` | real exact-rational tiebreak |
| `YANG_SHIFT_NEUTER` | `yang-rs/src/stage1_tessellate.rs:~2582` | forces the rim sample `shift` to 0 | computed shift |
| `YANG_NSEG_FLOOR` | `yang-rs/src/stage1_tessellate.rs:~271` | raises every rim segment count to a floor | producer-chosen `n_seg` |
| `YANG_RIM_JUNCTION_DISABLE` | `yang-rs/src/boolean.rs:~1853` | disables the Stage-1 rim-junction insertion | insertion enabled |
| `YANG_441_APPLY_BOOL_CAP` | `yang-rs/src/stage5_topology.rs` (`run_construct_passes`) | booleans at/above this process-order index run the I5 construct pass census-only | every boolean applies |
| `YANG_441_APPLY_SEAM_CAP` | same | caps the total applied-seam budget | uncapped |
| `YANG_434_OUT=census` | `yang-rs/src/stage5_output_refine.rs` (`restore_gate_enabled`) | reports the §4.4.2 restoration and does NOT apply it | restoration applied |
| `YANG_451=census` | `yang-rs/src/stage4_correct.rs` (`s451_census`) | OffCurve gates record-and-skip instead of abort, and the §4.5.1 repair is OFF | `on` (record and repair) |
| `YANG_451_TRIPLE_DOMAIN=1\|on`, `YANG_S45_BOUNDARY_DOMAIN=stop\|1\|on` | `yang-rs/src/stage4_correct.rs`, `stage4_correct/domain_postcondition.rs` | turn a domain fire into a hard STOP (`census` remains a print-only trace in release) | record-and-continue (`0\|off`) |
| `YANG_S4_CARRIER_DOMAIN=census` | `yang-rs/src/stage4_correct.rs` (`carrier_domain_mode`) | skips the carrier-domain abort (changes flow) | `stop` |

## Kill switches of landed mechanisms: DELETED 2026-10-16 / 2026-10-24

Cleanup spec §4 (`specs/yang_faithful_cleanup.md`, Phase 0): a default-ON
`0|off` kill switch is legitimate for ONE corpus run after its mechanism
lands, then it goes with its off-branch; a gated-OFF experiment is dead code.
Deleted on 2026-10-16 (yang-rs + kernel-v2): `YANG_433_RULEOUT`,
`YANG_433_TANGENT_INSERT`, `YANG_434_DECIMATE`, `YANG_434_INSERT`,
`YANG_434_MERGE`, `YANG_441_CONE_CHART`, `YANG_441_TORUS_CHART`,
`YANG_441_FAN_OF_ONE`, `YANG_441_FOLD_MERGE`, `YANG_441_MERGE_CARRIER`,
`YANG_441_ONCURVE_MERGE`, `YANG_451_TRANSIT`, `YANG_451_TRANSIT_EMIT`,
`YANG_EDGE_IN_PLANE`, `YANG_EDGE_PINCH_SPLIT`, `YANG_EDGE_PINCH_ENTRY_ONLY`,
`YANG_JUNCTION_LINE`, `YANG_JUNCTION_SLAB`, `YANG_JUNCTION_SAMPLING_ENABLE`
(with its `edge` / `face` half modes), `YANG_LINE_PAIR_CORNER`,
`YANG_P3B_PIERCE_ENABLE`, `YANG_PLANE_WELD`, `YANG_S0_SUBRES_FUSE`,
`YANG_S1_CHART_REFINE`, `YANG_S1_CHART_SEED`, `YANG_S1_COVER_UNROLL`,
`YANG_S5_ENVELOPE_ENABLE`, `YANG_TORUS_PAIR`, `YANG_TORUS_PLANE_PAIR`,
`YANG_SEAM_COMPONENT`, `KV2_ELLIPSE_SAG`, `KV2_PATCH_LIFT_REFINE`; and the
two gated-OFF experiments `YANG_S4_TRIPLE_POINT_ENABLE` (with
`plan_triple_point_reseats`) and `KV2_ARC_CONFORM_CURVES` (with
`boundary_curve_pool_samples`). The `0|off` value of these multi-valued knobs
went too, their other modes kept: `YANG_434_OUT`, `YANG_451`,
`YANG_451_TRIPLE_DOMAIN`, `YANG_451_TRANSIT_ANATOMY`,
`YANG_S45_BOUNDARY_DOMAIN`, `YANG_S4_CARRIER_DOMAIN`.
Deleted on 2026-10-24: `YANG_441_OPEN_CONIC_PARAM` (open-conic parameter order in
`stage4_correct/reversal.rs` is always-on for both `Parabola` and `Hyperbola`),
and the result-changing arms of the 8 multi-valued `census` / `stop` modes above
were gated behind `cfg!(debug_assertions)`.
Deleted on 2026-11-13 with the crease and corner transits (ledger N87):
`YANG_451_TRANSIT_ANATOMY`, and the tracing probes `YANG_451_CHI`,
`YANG_451_HOSTS` and `YANG_451_CORNER_PROBE`.
Deleted on 2026-11-21 with run absorption, group absorption and corner
re-homing (ledger N113): the `census` modes `YANG_441_RUN_ABSORB`,
`YANG_441_GROUP_ABSORB` and `YANG_441_REHOME`, and the tracing probes
`YANG_441_RUN_PROBE`, `YANG_441_RUN_PROBE_AT` and `YANG_441_I13F`.

## Result-changing modes still read in release (OPEN)

These are not print-only, so by the policy above they must become
debug-only or go; each needs its own decision (cleanup spec §4):

| Env var = value | Effect |
|---|---|
| `YANG_S1_SELF_CONTACT`, `YANG_S1_SELF_CONTACT_CHART` | the self-contact guard's switches — go with the guard (Phase 3) |

kernel-v2's ~24 `KV2_*_PROBE` and cherchi-rs's `CHERCHI_*_PROBE` are all
tracing-only (verified in the design review): each gates `eprintln!` and, in
the two that sit inside failure blocks, the `return Err(...)` is unconditional
whether the var is set or not.

## Full tracing-probe list

Enumerate on demand (avoids a list that rots):

```
grep -rhoE 'env::var(_os)?\("[A-Z0-9_]+"\)' \
  crates/{yang-rs,kernel-v2,cherchi-rs,test-harness}/src \
  | grep -oE '"[A-Z0-9_]+"' | sort -u
```

Any probe added to a kernel crate that **alters output** must be added to the
neuter table above AND gated with `cfg!(debug_assertions)`. A tracing probe
that only `eprintln!`s needs no registry entry.

## Validation STRENGTH is a cargo feature, never an env var

The mirror image of a neuter is a *strengthener* — a check that makes the kernel
refuse geometry it would otherwise accept. The same policy applies in reverse:
it must not be flippable at runtime in a shipped build, so it is a **compile-time
cargo feature**, not an environment variable.

- `kernel-v2/strict-validation` — compiles the debug-tier GEOMETRIC tripwires
  (loop vertices on their face's analytic surface, at the scale-relative
  `import_band`) into an optimized build. Default off; `test-harness` enables it
  so the `--release` categorized assay can see off-surface vertices at all.
  Before 2026-07-28 it could not: a body scored SUPPORTED_CORRECT while carrying
  a loop vertex 6.9 % of a cylinder radius off its own surface. See
  `docs/TESTING.md` and roadmap §0.0 item 6.

**Deleted 2026-10-16 (Phase 2a-i, the §4.5.3 sweep in the paper's order):** `YANG_453_RANK` (with the victim ranking), `YANG_453_PAIR` (kill switch and its result-changing `census` mode), `YANG_453_SPAIR`, `YANG_453_OVERTAKE` — the arms they switched are always on.

**Added 2026-10-16:** `YANG_441_UPDATE_CENSUS=<file>` — Phase 2b increment 0 (`stage4_construct::update_census`): appends, per Stage-4 run, the outcome tally of the §4.4.1 per-patch rebuild over every intersected patch, keyed by `ASSAY_CASE`. Print-only (corpus verdict-identical with it set). Also one `CHAIN_FOLD <verdict>` line per boundary-chain fold the update reads as Fig. 11(a) (`seeded`, `already-split`, `other-split`, `not-inside`, `model-vertex`, `not-a-relocated-point`) and one `CHAIN_FOLD_FAN` line per d(T)-refused patch that takes the local fan arm (ledger N2), and one `FOLD_PARTS` line per d(T)-refused patch whose lapping triangles `rebuild_patch_folds` replaces (`applied=true`, old → new triangle count) or leaves (`applied=false why=…`).

**Added 2026-10-27:** `YANG_S1_DEPS_CENSUS=<file>` — Roadmap F5 Step 1 (`stage1_tessellate::deps_census::maybe_log_stage1_deps_census`): appends, per initial operand `BRep::new`, one JSONL record comparing today's per-surface/edge Stage-1 `d_ε` values against the paper's §4.1.1 B-Rep model AABB `d_ε = 10⁻² · d` and recording Stage-1 inner/boundary `d(T)` bounds, keyed by `ASSAY_CASE`. Print-only (corpus verdict-identical with it set).

**Added 2026-10-27:** `YANG_CYL_STAGE0_CENSUS=<file>` (companion to existing `eprintln!`-only `CYLST0_PROBE`) — roadmap F6 / ledger N103 (`stage0/cylinder.rs::cyl_stage0_census_log`): appends one line per non-empty `detect_coincident_cylinder_pairs` or `coincident_cylinder_stage0` invocation, keyed by `ASSAY_CASE` (subprocess-safe under `ASSAY_JOBS`, where child stderr is discarded). Print-only (corpus verdict-identical with it set).

**Added 2026-10-27:** `YANG_S43_TANGENCY_CENSUS=<file>` — Roadmap F4 / N91 (`stage4_relocate::s43_tangency_census_log`): appends TSV lines keyed by `ASSAY_CASE` for Stage-4 pair/triple Newton refusals and tangency resolutions, Stage-1 tangent overrides, `stage4_small_loop` Gauss-map admission/pruning (`small_loop_gauss_admitted`, `small_loop_gauss_pruned`), curve-redundancy removals (`small_loop_redundant_on_solved_curve`), declines (`small_loop_decline`), and spliced small loops (`small_loop_spliced`), plus `shell_contact` crossing/non-crossing contact pairs. Print-only (corpus verdict-identical with it set).

**Added 2026-10-28:** `YANG_OFF_SURFACE_CENSUS=<file>` — ledger N100 item 9 (`boolean::failures::off_surface_census`): appends one line per emitted face whose loop has a vertex off its surface beyond kernel-v2's import band (T8), keyed by `ASSAY_CASE`, with the distance and band. Print-only (the T8 sites themselves are always on).

**Added 2026-10-28:** `YANG_N100_CENSUS=<file>` — ledger N100 (`n100_census.rs`): appends `case\titemN\tdetail` at the possible silent paths 1 (zero-distance non-crossing pair, `shell_contact`), 2 (no-witness single-candidate edge, `stage3_ssi`), 4 (sphere/torus multi-cycle outer choice vs largest Newell area, `stage5_topology/emit.rs`) and 5 (undetermined crease side, now a decline, `stage4_correct`), keyed by `ASSAY_CASE`. Print-only.

**Added 2026-11-06:** `YANG_REPAIR_CENSUS=<file>` — roadmap F2 "retire the repairs it subsumes" (`repair_census.rs`): appends `case\tpass\tcount` for each Stage-4/5 repair pass that CHANGED the mesh (the construct pass, the pinch split, the sub-τ collapse at both sites), keyed by `ASSAY_CASE`; the rigid-motion sweep keys it by motion. Also the §4.4.1 update's fold rebuild: `fold_parts_rebuilt` / `fold_parts_refused` per patch whose lapping triangles `rebuild_patch_folds` replaced or left, and `fold_whole_patch_rebuilt` per folded patch the whole-patch CDT replaced. No effect on any result.

**Added 2026-11-17:** `YANG_UV_CENSUS=<file>` — F5a increment 3 / ledger N109 (`param_surface::carried::census`): appends one TSV line per stage (`stage1-A`, `stage1-B`, `stage2-kept`, `stage4-relocated`, `stage5-exit`) keyed by `ASSAY_CASE`, with the vertex count, entries, vertices with no (u, v) (`bare`), entries the inverse refused, entries added by incidence (`settled`), the largest `|s(u, v) − p| / d_ε` over curved faces and the largest `|s(u, v) − p|` over planar ones. Print-only.

**Added 2026-11-25, extended 2026-11-26:** `YANG_FIG11B_CENSUS=<file>` — roadmap F2 "the Fig. 11(b) merge" / ledger N55, N89 item 7 (`stage4_fig11b::census`, spec `specs/yang_f2_fig11b_cdt_input_merge.md` §6, §8): appends, keyed by `ASSAY_CASE`, the merge classes `fig11b_vertex_classes` forms at `after-reloc`, `before-453-sweep`, `before-3c` and `before-441-update` — each member with its position bits, exactness rank, operand bits, relocated (`m`) or not (`s`), and the faces it carries parameters on; each mesh edge joining two members with the paper statement it falls under (`stage4_fig11b::pair_kind`: `fig11b`, `near-curve`, `same-curve`, `curve-end`, `different-faces`) — and a `taken` line for every pair the class merge (`pass=classes`, with its kind) and the sub-τ collapse (`pass=s194`) merge in the same build. Print-only; one `write_all` per line.

**Added 2026-11-26:** `YANG_CHAIN_CENSUS=<file>` — roadmap F2 "split Stage 4 into the paper's sub-steps" / ledger N81 (`stage4_chains`, hooks in `stage4_correct/reversal.rs::sweep_reversed_intersections`; spec `specs/yang_f2_fig11b_cdt_input_merge.md` §8.8–§8.9): appends, keyed by `ASSAY_CASE`, per sweep call an `entry` line (the mesh's identity `mesh=<hash of positions and triangles>`, so one call can be found again in another run; the chains the sweep walks and the chains of intersection edges per face pair: counts, and the edges only one reading holds) with up to four `only-loop` / `only-chain` detail lines (the sweep's surfaces on the edge, the face of every triangle on it); a `removal` line per removal (the test — `angle`, `param`, `progression` — or `overtake`, the curve variant on the edge to `p_n`, where `p_r` sits on the face-pair chains — `interior`, `branch`, `straddle`, `one-edge`, `off` — and whether the join is a chain edge); the `refused` reversals at the fixed point; and two `exit` lines counting every walked position by the sweep's own reading (point of C / endpoint) against the face-pair chains'. Print-only; one `write_all` per line.

**Added 2026-11-17 (N100 item 8; release-active switches that CHANGE output, listed so a stray setting is findable):** `CHERCHI_GRAZE_AWARE_RAY=0|off|1|on` — cherchi-rs `labeling/inside_out.rs` `graze_aware_ray_enabled`: kill switch / force for the N69 graze-aware explicit-ray ladder (default on, `GRAZE_AWARE_RAY_DEFAULT`). `CHERCHI_SCE_SHORTCIRCUIT=0|off` — cherchi-rs `predicates/triangle_intersect.rs` `coplanar_edge_reject_enabled`: disables the #200 coplanar-edge reject (default on; the disabled path is the pre-#200 behaviour; read once per process). Neither is a probe: unset is production. (The `YANG_S1_SELF_CONTACT` pair is registered above.)

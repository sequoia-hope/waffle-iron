## PR-Y15c-fix-2.1 — Sub-phase 0b Diagnostic Memo

**Author:** implementer-l (NEW agent; takeover from non-responsive implementer-k per
team-lead reassignment 2026-05-05; FIP §3.2 role-separation maintained — NOT spec-writer-i,
NOT adversary-7).
**Date:** 2026-05-05.
**Spec:** `specs/yang_pr_y15c_fix_2_1_a15_5_fallback_audit.md`.
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0b.
**Probe under audit:** `crates/kernel/src/boolean/yang_integration.rs:248-258` (added by
implementer-k pre-takeover; +11 LOC additive, env-gated on `YANG_A15_5_AUDIT=1`,
tag `[a15-5-fallback]`; format = spec §3 verbatim).

---

## Headline

**0 `[a15-5-fallback]` fires across the full 190-case corpus.** The
`unwrap_or_else` Newell-fallback at `yang_integration.rs:259-281` was not
reached by ANY of the 745 boolean operations that successfully ran the full
Yang pipeline through `result_topology_to_waffle_solid`. Decision-tree row
**3 / "Never fires"** per spec §5.

## §1. Reproduction

Command (spec §4 verbatim, with `randomized_assay_full_kernel` test name):

```
YANG_BOOLEAN=1 YANG_A15_5_AUDIT=1 \
  cargo test -p test-harness --test assay_randomized --release -- \
  randomized_assay_full_kernel --ignored --nocapture --test-threads=1 \
  > /tmp/a15_5_audit.stdout 2> /tmp/a15_5_audit.stderr
```

- Wall: 242.87s (under spec's 5-15min envelope).
- Cargo `test result: ok. 1 passed; 0 failed`.
- Per-case timeout (R0071 90s thread-wrap) per `randomized_runner.rs:91`
  fired once (R0071 → `error`); no manual `WAFFLE_TIMEOUT` shim required.
- `Score: 11/190 (11 pass, 178 fail, 1 error)` — matches PR-Y15c-fix-2
  baseline (adversary-7's `pr_y15c_fix_2_validation.md` §2) byte-for-byte.

## §2. Fire totals

```
$ grep -c '\[a15-5-fallback\]' /tmp/a15_5_audit.stderr
0
```

Per-case breakdown: **0 fires per case × 190 cases = 0 total fires.**

## §3. Pipeline-exercise sanity

A 0 fire count is only meaningful if the load-bearing call site was actually
reached. Cross-checks against the same stderr capture:

| Marker | Count | Meaning |
|---|---:|---|
| `label_cells` / `flood_fill_patches` / `Yang boolean pipeline` (combined) | 1,847 | Pipeline entry/midpoint diagnostics — Yang at least started 1.8k+ times. |
| `[yang-diag] after flood_fill: ` | 745 | `flood_fill_patches` returned successfully → `result_topology_to_waffle_solid` was invoked 745× (this is the function with the audit probe). |
| `[A15.6] Yang boolean pipeline failed (not falling through)` | 357 | Cases where Yang failed validation AFTER `result_topology_to_waffle_solid` returned — surface_map lookup loop already executed. |
| `AABB-disjoint short-circuit: skipping Cherchi` | 48 | Yang explicitly skipped before reaching topology extraction. Excluded from the 745. |

745 invocations × ≥1 face per result × 0 fires = the loop body's `if let Some(geom) = surface_map.get(...)` arm hit 100.0% of the time.

## §4. Probe-off byte identity

Per spec §6, verified the probe is silent without the env var:

```
$ YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --release \
    -- yang_trace_f0002 --ignored --nocapture --test-threads=1 2> /tmp/probe_off.stderr
$ grep -c '\[a15-5-fallback\]' /tmp/probe_off.stderr
0
```

Probe code is purely additive within an `if std::env::var(...).as_deref() == Ok("1")`
guard at L250-258 (verified by Read; no other touched lines).

Also re-verified `YANG_A15_5_AUDIT=1` ON for the same single case (sanity that
the env-var pathway compiles and the boolean ran):

```
$ YANG_BOOLEAN=1 YANG_A15_5_AUDIT=1 cargo test -p test-harness ... yang_trace_f0002 ...
$ grep -c '\[a15-5-fallback\]' /tmp/probe_on_check.stderr  → 0
$ grep -c 'label_cells' /tmp/probe_on_check.stderr        → 1
```

Boolean ran, probe was watching, no fires. Consistent with the corpus result.

## §5. Production safety

| Check | Result |
|---|---|
| `cargo clippy -p kernel --no-deps` | 91 warnings (delta=0 from spec baseline). |
| `rustfmt --check crates/kernel/src/boolean/yang_integration.rs` | exit=0. |
| Probe-off byte identity (spec §6) | confirmed (§4 above). |

## §6. Classification per spec §5 decision tree

| Spec row | Match? | Anchor |
|---|---|---|
| Row 1: fires non-zero on keys present in operand `face_geometry` | **NO** (0 fires) | n/a |
| Row 2: fires only on keys absent from operand `face_geometry` | **NO** (0 fires) | n/a |
| Row 3: never fires across full corpus | **YES** | `surface_map` has perfect coverage of `face_provenance` keys for the entire corpus's exercised pipeline runs. |

The cross-reference step (spec §5 ¶2 — "implementer-k cross-references
`source.mesh_id`/`source.face_idx` against the operand `WaffleSolid`s'
`face_geometry` keys") was contingent on at least one fire to classify.
With 0 fires, this step is vacuous: there are no fires to classify.
**Methodology shortcut DECLARED:** I did NOT execute the cross-reference
because the input set was empty. If adversary-8 spot-checks this, the spot
check is itself vacuous (no fires to check); the load-bearing
adversary mutation is to perturb the probe (e.g. flip `==` to `!=`) and
re-run the corpus to confirm the probe DOES fire when forced.

## §7. Recommendation — promote `unwrap_or_else` to `panic!`

Per spec §5 row 3: "Small follow-up PR to harden the contract."

`surface_map` is built from `solid_a.face_geometry ∪ solid_b.face_geometry`
(`yang_integration.rs:115-127`). `face_provenance` inserts
`SourceFace { mesh_id, face_idx }` for every result face
(`topology_extract.rs:237/260`+`:814/841`). 745 successful runs × 0 fires
shows the lookup is structurally exhaustive on the current corpus.

PR-Y15c-fix-2.2 scope (~5 LOC): replace `unwrap_or_else` arm with
`panic!("A15.5 contract violation: source ({mesh_id:?}, {face_idx:?}) absent
from surface_map (size={N})")`. Delete the L259-281 Newell-fallback block
(`verts.len()<3` and `nl<TAU_NORMALIZE` degenerate-skip guards become
unreachable). Load-bearing RED test: synthetic case with one `face_provenance`
entry intentionally absent from `surface_map` → assert panic fires.

Caveats spec-writer-i+1 should weigh: (a) corpus is bounded — a real-world
model not represented could still hit the path, but per FIP P9 a hard panic
is correct (don't silently planar-fallback); (b) function is infallible
today — `panic!` keeps that signature; `Result<WaffleSolid, KernelError>` is
a larger refactor; (c) adversary-8's mutation test should perturb the probe
(flip `Some` → `None`) on a single case to confirm the fire path is genuinely
reachable when forced.

## §8. Cross-reference logic (vacuous — documented for adversary-8)

If fires had occurred, classification per spec §5 ¶2 would be: for each
unique `(source.mesh_id, source.face_idx)` from a fire — load the case's
`.waffle`, truncate features to the pre-boolean state (cf. `spotlight_r0100`
pattern at `assay_randomized.rs:126-160`), call
`KernelIntrospect::compute_all_signatures(handle, TopoKind::Face)` on each
operand → collect FaceIdx set; in-set → row 1 (NEW A15.5 violation
upstream), not-in-set → row 2 (legitimate intersection-face miss). With
0 fires, the input set is empty.

## §9. Working tree state

- `crates/kernel/src/boolean/yang_integration.rs` — +11 LOC probe at L248-258
  (committed pre-takeover by implementer-k); no further changes by implementer-l.
- `docs/audits/pr_y15c_fix_2_1_diagnostic.md` — this file (NEW).
- `app/tests/cases/assay/results.json` — auto-write by corpus run; team-lead
  sub-phase 0d decides whether to ship or revert.
- `output.obj` — pre-existing untracked, not touched.

## Summary

Decision-tree row **3 / "Never fires"** with high confidence (745 successful
pipeline runs × 0 fires; §3 sanity confirms the probe site was reached at
scale). Recommend spec PR-Y15c-fix-2.2 to promote `unwrap_or_else` to
`panic!`. Methodology shortcuts DECLARED: §6/§8 cross-reference step is
vacuous (0 fires). No spec ambiguities encountered.

# PR-Y15c-fix-2.1 — A15.5 Newell-fallback audit instrumentation

**Status:** SPEC (FIP §8 Bug Fix Variant — diagnostic Phase 0; investigation only).
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` (sub-phases 0a–0d).
**Scope:** ONE probe at ONE site. NO fix code. Output is a diagnostic memo.

---

## 1. Goal

PR-Y15c-fix-2 (commit `1aed3ce`, 2026-05-05) shipped lookup-first surface tier
preservation in `result_topology_to_waffle_solid`
(`crates/kernel/src/boolean/yang_integration.rs:235-271`). Newell-fallback is
now restricted to the `unwrap_or_else` path of `surface_map.get(...)` at L243-247.

Adversary-7's validation (`docs/audits/pr_y15c_fix_2_validation.md` §3) confirmed
the fix dissolved track A on 5/10 cases and track B on all 10, but did not measure
how often the fallback fires across the full corpus. The question this PR answers:
**when the lookup misses, is the source face legitimately absent (e.g. a new
intersection face) or absent due to an upstream bug (an unmodified face whose
provenance the surface_map should carry but doesn't)?** The first is benign A15.5
behavior; the second is a NEW A15.5 violation.

## 2. Instrumentation site

`crates/kernel/src/boolean/yang_integration.rs:243-247`, the `unwrap_or_else`
arm of the lookup-first construction loop:

```rust
for (&face_idx, source) in result.face_provenance.iter() {
    if let Some(geom) = surface_map.get(&(source.mesh_id, source.face_idx)) {
        face_geometry.insert(face_idx, geom.clone());
        continue;
    }
    // ← probe fires HERE, before the existing Newell fallback at L248-270
    ...
}
```

The probe MUST fire exactly once per Newell-fallback hit, BEFORE the degenerate-skip
guards at L251 and L256 (those are valid silent skips, not fallback fires).

## 3. Probe payload

Single `eprintln!` line per fire, env-gated on `YANG_A15_5_AUDIT=1` (separate from
`YANG_CONFORMAL_PROBE` to keep oracle families cleanly separable; A15.5 is a
different invariant from conformal-mesh well-formedness):

```
[a15-5-fallback] face_idx={face_idx:?} source_mesh={source.mesh_id:?} source_face={source.face_idx:?} map_size={surface_map.len()}
```

Tag: `[a15-5-fallback]`. Do NOT log full surface_map contents — keys can number in
the hundreds per case; the missed lookup-key + map size are the load-bearing diagnostic.

## 4. Reproducer harness

Full corpus via `assay_randomized` (190 cases). R0071 timeout-wrap pattern (90s
per-case via `WAFFLE_TIMEOUT`) per `pr_y15b_combined_failures_parity.rs` precedent:

```
YANG_BOOLEAN=1 YANG_A15_5_AUDIT=1 cargo test -p test-harness --test assay_randomized --release \
  -- randomized_assay_full_kernel --ignored --nocapture --test-threads=1 \
  > /tmp/a15_5_audit.stdout 2> /tmp/a15_5_audit.stderr
```

Streams separated so `[a15-5-fallback]` lines (stderr) are isolated from test
runner output (stdout). Implementer-k extracts fires via `grep '\[a15-5-fallback\]'
/tmp/a15_5_audit.stderr`.

## 5. Decision tree

`face_provenance` has NO sentinel field distinguishing intersection from
unmodified faces (`topology_extract.rs:237/260` and `:814/841` — both call sites
unconditionally insert `SourceFace { mesh_id, face_idx }` for every result face).
**Classification is inferred at the lookup site itself**: a fire on a key that
SHOULD be in `surface_map` (per the operand's input `face_geometry`) is the
unmodified-face miss; a fire on a key that's NOT in either operand's input
`face_geometry` is the legitimate intersection-face miss. Implementer-k cross-references
`source.mesh_id`/`source.face_idx` against the operand `WaffleSolid`s' `face_geometry`
keys (built into `surface_map` at `yang_integration.rs:115-127`) to classify each fire.

| Fire pattern | Anchor | Next PR |
|---|---|---|
| Fires non-zero times on keys present in operand `face_geometry` (i.e. unmodified-face miss — surface_map lookup fails for a face whose source SHOULD be in the map) | NEW A15.5 violation upstream of `result_topology_to_waffle_solid` (likely in `face_provenance` construction path or `build_surface_map`) | Spec PR-Y15c-fix-2.2 to investigate WHY surface_map is incomplete relative to provenance |
| Fires only on keys absent from operand `face_geometry` (legitimate per A15.5 ¶2 — intersection face with no single-source provenance match) | Document as expected behavior; A15.5 ¶2 tier policy ("highest tier of two intersecting surfaces") deferred per PR-Y15c-fix-2 spec §3 out-of-scope clause | Optional cleanup: spec intersection-face tier policy per A15.5 ¶2 |
| Never fires across full corpus | `surface_map` has perfect coverage of `face_provenance` keys; promote `unwrap_or_else` to `expect()` so a future drift panics instead of silently planar-fallbacking | Small follow-up PR to harden the contract |

## 6. FIP role table

| Sub-phase | Agent | Reads | Writes |
|---|---|---|---|
| 0a Spec | spec-writer-i | This task brief; plan; `yang_integration.rs:230-280`; `topology_extract.rs:28-31`+`:145`+`:237/260`+`:814/841`; `build_surface_map` L115-127; PR-Y15c-fix-2 spec + validation memo; `yang_face_geometry_propagation.md`; A15.5 verbatim; FIP §3+§4+§8; DoD §6; `feedback_anchor_before_fix.md`; `feedback_adversary_recommendations_need_canary.md`; `feedback_validate_against_corpus.md`; Yang 2025 §4.5; Cherchi 2022 §5 | `specs/yang_pr_y15c_fix_2_1_a15_5_fallback_audit.md` (THIS) |
| 0b Probe + memo | implementer-k (NEW; NOT spec-writer-i) | This spec; `face_provenance` construction at `topology_extract.rs:237/260`+`:814/841` (verify both insertion call sites); `build_surface_map` L115-127 | `yang_integration.rs:243-247` (+~5 LOC env-gated `eprintln!`); `docs/audits/pr_y15c_fix_2_1_diagnostic.md` (~80-120 LOC: total fire count, per-case breakdown, classification per §5 decision tree, recommendation) |
| 0c Adversary | adversary-8 (NEW; NOT adversary-7 per `feedback_oracle_credibility_via_role_separation.md`) | All 0a-0b deliverables; `feedback_adversary_recommendations_need_canary.md` | `docs/audits/pr_y15c_fix_2_1_validation.md` (~80 LOC: verdict, mutation test, spot-check 2-3 fires for classification correctness) |
| 0d Close-out | team-lead | All 0a-0c | clippy + rustfmt on touched file; NO WASM rebuild (probe is env-gated default-off); memory updates; commit + push |

**Probe-off byte identity (DoD §6):** team-lead verifies that without `YANG_A15_5_AUDIT=1`,
the touched file produces byte-identical behavior to HEAD. Probe code is purely
additive within an `if std::env::var(...).as_deref() == Ok("1")` guard.

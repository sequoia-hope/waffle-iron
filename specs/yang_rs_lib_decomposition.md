# yang-rs `lib.rs` Decomposition — Retiring the God Module

**Status:** PROPOSED (spec only — no code moved yet)
**Scope:** `crates/yang-rs/src/lib.rs` (22,602 lines as of 2026-07-10, HEAD 9f720fb9)
**Type:** Pure mechanical refactor — **zero behavior change is the primary invariant.**

## Goal

`lib.rs` has grown to 22,602 lines (~15,650 production + ~6,950 in-file unit
tests) while the pipeline stages accreted around it as siblings (`stage0.rs`
8,602, `coplanar_overlay.rs`, `stage4_dt.rs`, `stage4_update.rs`). This
violates the spirit of P7 (small, auditable changes — a diff in a 22k-line
file is hard to audit) and makes agent sessions burn context navigating one
file. The goal is to split `lib.rs` into cohesive modules that mirror the
Yang 2025 pipeline stages (the paper is the spec — the module map should read
like §4 of the paper), with **no file over ~3,500 lines of production code**.

The file already has clean internal `// ===` section banners; the split
follows them. This is a *move*, not a redesign.

## Current section census (line ranges in today's lib.rs)

| Lines | Section banner | Contents | ~Size |
|---|---|---|---|
| 1–133 | preamble | `mod` decls, `pub use` re-exports, `native_backend()` | 130 |
| 134–274 | Surface / Curve enums | `Surface`, `Curve` | 140 |
| 275–438 | B-Rep topology / TessellationMap / PR-YR4 | `BRepVertex/Edge/Face`, `TessellationSource`, `TessellationMap`, `MATCH_TOLERANCE`, `InputId`, `TriangleAttribution(Map)` | 165 |
| 439–1858 | BRep | `BRep` struct, ctors (`new`, `from_topology*`, `from_mesh`), accessors, `stage1_tessellate_inner`, `eval_source` | 1,420 |
| 1859–5688 | PR-YR7/YR11 — Stage-1 curved tessellation | conic frames/evaluators (`ellipse_frame`, `parabola_point`, `hyperbola_point`), planar CDT faces, lateral/band tessellators, sphere/cone/torus tessellators (+ embedded `mod torus_patch_tests`, 8 tests), normal orientation, chord bounds, `signed_distance_to_surface`, `surface_normal_at` | 3,830 |
| 5689–7009 | PR-YR10 — Stage-4 relocation | `relocate_onto_implicit_pair/triple`, per-conic reloc types (`EllipseReloc`, `ConeEllipseReloc/Parabola/Hyperbola`, `LineReloc`), junction certificates, band amplification, pinch split | 1,320 |
| 7010–7884 | PR-YR9 — Stage-3 SSI refinement | `surface_to_quadric`/`quadric_to_surface`, `ssi_curve_to_curve`, chord tolerances, `build_intersection_curves` | 875 |
| 7885–8113 | Errors | `YangError`, `Stage4InvalidReason`, `SsiRefinementError`, `non_manifold_at` | 230 |
| 8114–10755 | boolean() — PR-YR3/YR4 | `boolean()` driver, `scan_near_coplanar`, provenance/attribution, KV15 near-weld, phantom rim N, rim-junction overrides, stage0 dump | 2,640 |
| 10756–15649 | PR-YR5 — topology reconstruction | Stage-4 mesh correction (`stage4_relocate_and_correct`, `collapse_vertex`, sub-resolution collapse, reversal sweeps) **and** Stage-5/6 extraction (`reconstruct_topology*`, `emit_topology`, `Patch`, flood fill, boundary cycles) | 4,890 |
| 15650–22602 | Tests | flat `#[cfg(test)] mod tests` — 144 tests, internally grouped by `// -----` banners (PR-YR1…YR5, Stage-6 sliver, attribution groups) | 6,950 |

Public API is small and stays put: 21 `pub` items + 6 `pub use` re-exports;
`kernel-v2` consumes only `BRep`, `BRepVertex/Edge/Face`, `Surface`, `Curve`,
`boolean()`, errors, and re-exported cherchi types. **The crate's external
surface does not change** — `lib.rs` keeps (or gains) `pub use` re-exports so
every existing `yang_rs::X` path still resolves.

## Target layout

Flat sibling files, consistent with the existing `stage0.rs` /
`stage4_dt.rs` / `stage4_update.rs` naming (no directory nesting — matching
what the crate already does):

```
crates/yang-rs/src/
  lib.rs               ~150   mod decls + pub use re-exports + native_backend()
  geom.rs              ~600   Surface, Curve, conic frames/evaluators
                              (ellipse_frame/point/param/tangent, parabola_point,
                              hyperbola_point), signed_distance_to_surface,
                              surface_normal_at
  brep.rs             ~1,600  BRepVertex/Edge/Face, TessellationSource/Map,
                              MATCH_TOLERANCE, InputId, TriangleAttribution(Map),
                              BRep struct + ctors + accessors + eval_source
  errors.rs             ~250  YangError, Stage4InvalidReason, SsiRefinementError,
                              non_manifold_at
  stage1_tessellate.rs ~3,300 stage1_tessellate_inner + all per-surface
                              tessellators, orientation helpers, chord bounds,
                              torus patch (+ its embedded test mod)
  stage3_ssi.rs         ~900  quadric conversions, build_intersection_curves,
                              chord tolerances
  stage4_relocate.rs  ~1,350  relocate_onto_implicit_pair/triple, Reloc types,
                              junction certificates, band amplification
  stage4_correct.rs   ~1,800  stage4_relocate_and_correct, collapse_vertex,
                              sub-resolution segment collapse, reversal sweeps,
                              relocation validation, pinch split
  stage5_topology.rs  ~3,100  reconstruct_topology(_stage4), emit_topology,
                              Patch, flood fill, boundary cycles, sliver logic
  boolean.rs          ~2,650  boolean() driver, scan_near_coplanar, provenance,
                              KV15 near-weld, phantom rim N, rim-junction
                              overrides, stage0_dump
  tests_unit/          split of the flat mod tests along its existing
    mod.rs             // ----- group banners: one file per group family
    brep_types.rs      (construction, round-trips, loud-rejects)
    stage1.rs          (Stage-1 happy paths, error paths)
    attribution.rs     (PR-YR3/YR4 matching + attribution)
    topology.rs        (PR-YR5 reconstruction, Stage-6 sliver)
    boolean_dispatch.rs(PR-YR1 dispatch, mock-backend)
  (unchanged: stage0.rs, coplanar_overlay.rs, stage4_dt.rs, stage4_update.rs)
```

Boundary rule where the old §PR-YR5 section mixes concerns: everything that
*mutates the mesh* (collapse, relocation application, sweeps) →
`stage4_correct.rs`; everything that *reads the corrected mesh and builds
B-Rep topology* (patches, loops, emission) → `stage5_topology.rs`. If a
helper is genuinely shared, it lands in the earlier stage and is
`pub(crate)`.

## Parameters

None — no runtime inputs. The only knobs are the module boundaries above.

## Branch table

Empty **by construction**: this refactor may not introduce or remove a single
behavior branch. Any `if`/`match` added during the move (other than none) is
a spec violation. Visibility promotions (`fn` → `pub(crate) fn`) are the only
permitted signature edits; no `pub` promotions to the crate's external API.

## Invariants

1. **I1 — Zero behavior change.** Byte-identical logic; moves + `use`
   rewiring + `pub(crate)` promotions only. No reordering of statements, no
   renames, no "while I'm here" cleanups (those come later, separately, under
   their own specs).
2. **I2 — Public API frozen.** Every pre-existing `yang_rs::<item>` path
   resolves after the split (via `pub use` re-exports in `lib.rs`).
   `kernel-v2` compiles with **zero source changes**.
3. **I3 — Tests are permanent.** All 144 in-file unit tests + the 8
   `torus_patch_tests` move verbatim (they use private items, so they stay
   in-crate as `#[cfg(test)]` submodules). None deleted, none weakened. The
   67 integration-test files in `tests/` are untouched.
4. **I4 — Every increment is green.** Each commit leaves
   `cargo test -p yang-rs` (incl. FFI-feature suite where configured) and
   `cargo clippy -p yang-rs` clean. No multi-commit red windows (P7).
5. **I5 — Size ceiling.** After the final increment, no `src/*.rs` file that
   this spec touches exceeds ~3,500 production lines. (`stage0.rs` at 8,602
   is out of scope — see Follow-ups.)
6. **I6 — Layering intact.** No new cross-crate imports; dependency
   direction (kernel-v2 → yang-rs → cherchi-rs/ssi-rs) unchanged (A1).

## Oracles

- **O1 (behavior):** `./scripts/test.sh rewrite` green before and after every
  increment; `./scripts/test.sh fast` green at the end. Since the change is
  move-only, the full assay is NOT rerun per increment — one final
  confirmation that the assay score is byte-identical to the pre-refactor
  baseline (currently 231 CORRECT / 0 WRONG / 47 ERROR / 15 UNSUPPORTED) is
  the completion gate.
- **O2 (API freeze):** `cargo build -p kernel-v2` with zero diffs outside
  `crates/yang-rs/`.
- **O3 (test conservation):** `grep -c '#\[test\]'` across yang-rs `src/` is
  ≥ the pre-refactor count (152 = 144 + 8) at every increment.
- **O4 (move purity, per increment):** reviewer check that the diff is
  delete-block-here / add-same-block-there; `git diff --stat` shows paired
  removals/additions. Where practical, verify with
  `git diff --color-moved=dimmed-zebra` showing the body as moved lines.

## Increment plan (one commit each, dependency-ordered)

Extraction order follows the dependency direction so each new module only
needs `use crate::…` of already-extracted or still-in-lib items:

1. **errors.rs** — leaf-ish, imported by everything. Smallest, proves the
   pattern.
2. **geom.rs** — `Surface`/`Curve` + evaluators (used by all stages).
3. **brep.rs** — topology structs, maps, attribution, `BRep` impl (calls into
   Stage-1, which is still in lib.rs at this point — fine, same crate).
4. **stage1_tessellate.rs** — the big tessellation block + embedded torus
   tests.
5. **stage4_relocate.rs** — relocation primitives (used by stage3 + stage4
   correction).
6. **stage3_ssi.rs** — SSI refinement.
7. **stage4_correct.rs** — mesh correction half of old §PR-YR5.
8. **stage5_topology.rs** — topology extraction half of old §PR-YR5.
9. **boolean.rs** — the driver (depends on all of the above; extracting it
   last means it never needs forward references).
10. **tests_unit/** — split the flat `mod tests` along its `// -----` group
    banners into `#[cfg(test)]`-gated submodule files; `lib.rs` keeps only
    `#[cfg(test)] mod tests_unit;`.

Steps 4 and 8 are the largest single moves (~3.3k / ~3.1k lines) but are
still single-section cut-and-paste; if either proves entangled mid-flight,
the fallback is to land it as two sub-moves within the same boundary — NOT to
redraw the boundary ad hoc (P10: if the plan's decomposition is wrong, stop
and re-spec).

## Failure modes

- **Hidden coupling:** a "section" references items from a later section
  (e.g. Stage-1 calling a boolean()-section helper). Expected and fine —
  same-crate `pub(crate)` handles any direction; the compiler enumerates
  every case. What is NOT fine is discovering shared *mutable state* or
  macro-order dependence; none is known to exist (no macros define items in
  lib.rs), but if found: halt, document in the roadmap, re-spec.
- **Stale line-number references:** memory files, probe docs, and specs cite
  `lib.rs:<n>` (e.g. `relocate_onto_implicit_pair` at lib.rs:4552 in
  m5_scout_findings). These go stale. Mitigation: function names are the
  durable reference; grep still finds them. No doc rewrite pass required.
- **Test flakiness masquerading as breakage:** assay TIMEOUTs are
  load-sensitive config artifacts (proven 2026-07-09) — a timeout during O1
  verification is re-measured solo before being attributed to the refactor.
- **Merge conflicts with in-flight N2 work:** the N2 epic (task #124) edits
  these same regions. Sequencing rule: land this refactor at a quiescent
  point (no uncommitted yang-rs work), complete increments 1–10 in one
  session if possible; do not interleave with feature increments.

## Follow-ups (explicitly out of scope here, each needs its own spec)

- `stage0.rs` (8,602 lines) is the next-largest god module — same treatment
  (it has its own internal banner structure).
- kernel-v2's `boolean.rs` (~2.4k+ lines) trending the same way.
- Any *semantic* cleanups discovered during the move (dead code, duplicated
  helpers) — record them in the roadmap, do not fix in-flight (I1).

## Research basis

Not an algorithm change — no external references required (P8 n/a). The
module boundaries are the Yang 2025 pipeline stage boundaries (§4.1–§4.5),
which is the same decomposition the roadmap (`docs/yang_functional_roadmap.md`)
and the existing sibling files (`stage0.rs`, `stage4_*.rs`) already use.

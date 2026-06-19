# Plan: Cherchi 2020 fully-coplanar arrangement (conformal overlap) — multi-PR scope

**Status:** scoped, not started. Task #26. License: porting from MIT
`InteractiveAndRobustMeshBooleans` (Cherchi 2022) — attribution header required.

## Goal

Make cherchi-rs **construct** (not loud-defer) fully-coplanar overlapping
triangle pairs (`DeferReason::Coplanar`, `orBA = 0 0 0`), so coincident-face
booleans succeed. End-to-end target: the user's gear-flange union (`err.waffle`)
builds **watertight, non-self-intersecting, correct volume** (combined bbox
z∈[-0.005, 0.005]). Also clears the `UNSUPPORTED(coplanar-boolean)` assay class
and is the general engine for M8 coplanar cases (subsumes parts of #14–#16).

## Why the previous two attempts hit a wall (the load-bearing insight)

Both attempts produced a coplanar result that **double-counted the overlap**
(native union vol 763.3 vs sidecar 750.0; 67 unpaired edges). Root cause: they
relied on `remove_degenerate_and_duplicated_triangles` (`soup.rs:264`, which
OR-merges labels of **bit-identical** triangles) to collapse the two coplanar
triangles' overlap region. But two coplanar triangles re-triangulated
independently produce **different** interior sub-triangulations over the same
overlap, so they are *not* bit-identical and never dedup.

**The C++ does not require bit-identical sub-triangulations.** It dedups at the
**pocket** level, keyed by the pocket's *boundary vertex-ID set*
(`addVisitedPolygonPocket`, `triangulation.cpp:1244`). The overlap pocket has
the same boundary vertex set in both triangles (because
`propagateCoplanarTrianglesIntersections` gave both the same points+segments),
so the second triangle's overlap pocket matches the first by vertex-set and is
**label-OR-merged instead of re-emitted**. A-only / B-only pockets have
different vertex sets and are emitted normally. This is robust to the interiors
differing. Replicating *this* mechanism — not the bit-identical shortcut — is
the whole job.

## The four C++ pieces to port (all in the MIT reference)

1. `addCoplanarTriangles(tA,tB)` / `coplanarTriangles(t)` / `triangleHasCoplanars(t)`
   — `aux_structure.cpp:148-171`, storage `coplanar_tris` (`aux_structure.h:188`).
2. `checkSingleCoplanarEdgeIntersections` called **6×** for the fully-coplanar
   pair — `intersection_classification.cpp:144-146, 208-210`. (Edge-contained +
   edge-CROSSING sub-configs already shipped: commits 8e3dc0cf, 6280237d.
   **vertex-in-edge** `tvX_in_edge` is the only remaining sub-config.)
3. `propagateCoplanarTrianglesIntersections` — `intersection_classification.cpp:788`.
   After all pairs classified: for each triangle with coplanars, copy the
   partner's per-edge points and per-triangle segments into *this* triangle's
   interior point/segment lists, guarded by `genericPointInsideTriangle`.
4. `findPocketsInTriangle` + `solvePocketsInCoplanarTriangle` —
   `triangulation.cpp:1226, 1271`; wired into the per-triangle emit at
   `triangulation.cpp:112`. Flood-fill sub-triangles bounded by
   constraint/boundary edges → pocket boundary polygon (vertex-orig-id set) →
   `addVisitedPolygonPocket` global dedup (new → emit triangles+label; seen →
   `new_labels[pos+i] |= label`).

## Rust integration points

- `crates/cherchi-rs/src/arrangements/aux_structure.rs` — add `coplanar_tris`
  adjacency; per-triangle interior-point list + per-triangle segment list +
  per-edge points list (verify which already exist — `group_intersection_points`
  / `group_constraint_segments` already bucket points/segments per edge/triangle,
  so this is mostly exposing accessors); new `VisitedPocketRegistry`
  (`BTreeMap<BTreeSet<vid>, position>`). The `TooManyGeometricEndpoints` cap
  (`:372/447`) must branch coplanar≤3 vs transversal≤2 (C++ `coplanar_tris`
  flag, `intersection_classification.cpp:267-270`).
- `crates/cherchi-rs/src/arrangements/intersection_points.rs` — complete
  `classify_single_coplanar_edge` vertex-in-edge branch (returns `None` today at
  ~436); fully-coplanar dispatch (~172) calls it 6× + `add_coplanar_triangles`.
- `crates/cherchi-rs/src/arrangements/retriangulate.rs` (+ the per-triangle emit
  loop, currently collecting sub-tris+labels into the global soup) — add the
  pocket path for `triangle_has_coplanars(t)` triangles; the
  `propagate` pass runs once after classification, before re-triangulation.
- `crates/cherchi-rs/src/arrangements/soup.rs` — remove the
  `CoplanarPairDeferred` escalation (`:695-702`) **only** for pairs the new path
  now constructs; `DeepRecursionRequired`/`DegeneratePairDeferred` stay loud.
  Existing `or_merge_label` (`:316`) is reused by the pocket label-OR.

## PR breakdown (each atomic, parity-gated, committable; PRs 1–3 are corpus-neutral)

| PR | Scope | Gate | Corpus effect |
|----|-------|------|---------------|
| **1. aux foundations** | `coplanar_tris` adjacency + interior point/segment accessors + `VisitedPocketRegistry` + coplanar≤3 endpoint cap branch. Pure data structures, nothing calls them. | unit tests; arrangement output byte-identical | none (neutral) |
| **2. classify fully-coplanar** | vertex-in-edge sub-config; orBA/orAB 0 0 0 → 6× `checkSingleCoplanarEdgeIntersections` + `add_coplanar_triangles`; record ≤3 coplanar pts/segs. | classification-level parity vs sidecar on coincident-cylinder + box fixtures (points/segments match); on-plane FFI + pure-dashu covering oracles | none (still defers at soup level) |
| **3. propagate** | port `propagateCoplanarTrianglesIntersections`. | unit test: both coplanar triangles carry matching interior point+segment sets over the overlap (cylinder fixture) | none (still defers) |
| **4. pocket dedup + emit** ⚠️ | port `findPocketsInTriangle` + `solvePocketsInCoplanarTriangle`; wire emit; **remove the coplanar `CoplanarPairDeferred` escalation**. This flips coplanar pairs deferred→constructed. | **MAKE-OR-BREAK: full coaxial-cylinder sidecar parity — volume == sidecar (750.0, not 763), watertight, Euler, canonicalized vertex-set/tri-count.** Plus assay no-regression + 0 new SUPPORTED_WRONG. | coplanar-boolean class drops; SUPPORTED_CORRECT rises |
| **5. gear E2E + un-quarantine** | verify `err.waffle` full `run_all_mesh_checks` (esp. `no_self_intersection`) + combined bbox/volume; un-quarantine passing `*_stays_unsupported`/`*_defers` coplanar tests; add gear regression test; rebuild WASM. | gear clean; assay delta; GUI smoke | gear builds |

**Independent, ship anytime:** the P9 silent-drop fix (task #25) — surface
`MeshBooleanFailed` loudly instead of dropping the gear body. Not blocked by
PRs 1–5; do it first so the gear *loudly* fails until PR-4 lands.

## Gates & discipline (non-negotiable)

- **Sidecar parity is THE correctness oracle** (`/home/claude/cherchi2022/.../build/mesh_booleans`, MIT). Rebuild per session via `scripts/build_sidecars.sh` (not persisted across containers).
- **Unit-level classification correctness is NOT sufficient** — proven twice. The PR-4 *output* parity (volume + watertight) is the real gate. Build the coaxial-cylinder parity fixture FIRST and let it gate PR-4.
- **P9/P10:** a 0-error build that drops/doubles geometry is silent-wrong = FAIL. `no_self_intersection` + combined bbox/volume are mandatory asserts. No tolerance widening. LOUD deferral beats wrong output: any sub-config not handled (e.g. all-jolly-coplanar codim-2) stays a typed error.
- Exact `dashu`; no `unsafe`/`panic!`/`unwrap`/`expect` in production; single-threaded; WASM-clean. `_II`/`_IIII` FFI variants segfault on explicit input — use `genericPoint::` static dispatch (`with_gp!`).
- Run `./scripts/test.sh rewrite` after each PR (low-stack change can flip a downstream `expect_err` pin — banked lesson).

## Risks

- **Pocket key must use global/interned (orig) vertex ids** consistent across both triangles (C++ `subm.vertOrigID`) — the load-bearing detail; a per-triangle-local id breaks dedup.
- **Pocket boundaries match only if `propagate` inserted identical segments** in both triangles; PR-3's unit test must assert this before PR-4.
- Re-triangulation must emit **constraint edges on the overlap boundary** so `findPockets` flood-fill stops there.

## Alternative path (rejected as primary)

yang Stage-0 coplanar preprocessing could resolve *coincident cylinder* facets
in 2D before tessellation, but the current M8 plan tags curved∩curved
out-of-scope, and it would not generalize to arbitrary coplanar overlaps. The
native cherchi path is the faithful port and the general engine — preferred.

## Effort

5 PRs (PRs 1–3 small/neutral, PR-4 is the bulk + the risk, PR-5 integration).
Realistically several work increments; PR-4's parity gate is where it lives or
dies. Land PRs 1–3 freely (corpus-neutral); hold PR-4 behind green parity.

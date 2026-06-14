# Prototype-Release Roadmap — the path to the planetary-gearbox demo

> **Scope:** the cross-cutting, goal-scoped epic that drives Waffle Iron to a
> "prototype release" — anchored by a concrete acceptance test: **model a
> planetary gearbox and 3D-print it.** This is an *epic* doc (retired/archived
> when the gearbox prints), not a standing roadmap. It owns sequencing and the
> "done" definition; it does NOT duplicate per-layer detail.
>
> **Where the detail lives:**
> - Kernel capabilities → `docs/yang_functional_roadmap.md` §4 (KV-tagged).
> - App/UX work → the relevant `projects/NN-*/PLAN.md` (as-built reference) +
>   this doc's phase entries for forward sequencing.
> - Tooling → `projects/13-dev-infrastructure/PLAN.md`.
>
> Precedent for an epic doc of this shape: `docs/SELECTION-ENHANCEMENT-PLAN.md`,
> `docs/SKETCH-SYSTEM-PLAN.md`.

## Acceptance test (the north star)

Model **sun + planets + ring + carrier** in Waffle Iron → export each body as
STL (STEP later) → slice → 3D print → it meshes and turns.

For export, only **per-body STL/STEP** is needed (no in-app assembly motion).

## Capability decomposition

| Gearbox part | Built from | Status |
|---|---|---|
| Sun / planet gears | extrude gear profile + center bore | ⛔ arc-segment profile wall (KV12) |
| Ring (internal) gear | annulus with internal teeth = gear profile as a hole | ⛔ KV12 + non-convex-hole validation |
| Carrier plate, pins | polygon / circle extrudes | ✅ supported |
| Positioned assembly | multiple disjoint bodies | ✅ (F0015 disjoint-body split, 2026-06-13) |
| Print | per-body STL export | ⚠️ exists, not multi-body-aware |

## Phases (ORDERED)

The critical path to the print is **A → B → C**. D is a cheap parallel win;
E and F are deliberate pre-release follow-ups that do NOT block the print.

### Phase A — Assay per-test timeout *(tooling; the enabler)*
Make the corpus run fast and hang-proof so gear work can be validated without
babysitting. Home: `projects/13-dev-infrastructure/PLAN.md`.
- **A1** — per-case 30s timeout (worker thread + `recv_timeout`); a new
  **`TIMEOUT`** result category; summary line counts it separately.
- **A2** — auto slow-case list + `--fast` flag that skips known-slow cases for a
  sub-minute baseline. The full run still applies the cap so it never hangs.
- *(Later, Phase E-adjacent)* optional subprocess-per-case for clean reclamation.
- **Done when:** a clean baseline corpus number is obtainable in <1 min and no
  case can wedge the run. (Context: the container's PID 1 is `ttyd`, not a
  reaping init — orphaned heavy cases zombie; the per-case cap sidesteps that.)

### Phase B — Gear extrude **Tier 1** *(kernel — `KV12`)* ✅ DONE (2026-06-13)
Routed arc-annotated profiles through their authored `vertex_ids` polygon
(treat `arc_segments` like `spline_segments`, PR-KV8). Chord-approx via the
samples the solver/viewport already use — no new approximation; printable.
Bore = inner loop of the profile (single extrude, no boolean). Detail: see
`yang_functional_roadmap.md` KV12. Validated: kernel unit + E2E prism tests +
GUI `arc-profile-extrude.spec.js` (closed arc profile → extrude → body).

### Phase C — Gearbox assembly + per-body export *(app/UX)*
Homes: `projects/04-3d-viewport/PLAN.md`, `projects/08-ui-chrome/PLAN.md`.
- **C1** — place sun/planets/ring/carrier as separate bodies (multi-body ✅).
- **C2 — ring (internal) gear.** Two sub-findings (2026-06-13):
  - *Non-convex tooth profile:* ✅ extrudes — gear caps are non-convex simple
    polygons and already work (generate_gear_profile gear-extrude tests + KV12).
  - *The hole (annulus):* ✅ **DONE — `KV14` adapter hole assembly (2026-06-13).**
    `make_faces_from_profiles` now groups inner (`is_outer=false`) loops into the
    strictly-larger containing outer and builds one holed `Profile::new(outer,
    holes)` — so a ring gear / plate-with-bore drawn as ONE sketch extrudes as an
    annulus. Robust against the region-detector's redundant same-loop pairing
    (area filter + centroid witness); `profile_index` contract preserved; circle
    rims with holes polygonized. Tests: kernel annulus-volume + GUI nested-rect
    `holed-extrude.spec.js`. Detail: `yang_functional_roadmap.md` KV14.
- **C3 — right-click a body in the Bodies list → Export STL** ✅ DONE
  (2026-06-13). Per-body export by persistent body id; whole-model `ExportStl`
  now merges all renderable bodies (was last-body-only). GUI
  `body-export.spec.js`. STEP added to the same menu when the STEP milestone
  lands.
- **Done when: the gearbox prints. ← prototype-release gate.**

### Phase D — Face → feature **Tier 1** *(app/UX; cheap parallel win)* ✅ DONE (2026-06-14)
Click a face → its producing feature is highlighted in the tree (green accent +
"◀ face" badge). Read straight off the selected `GeomRef`'s `FeatureOutput`
anchor (`getSelectedRefFeatureId`) — store + tree-highlight, no kernel work.
Exact for single-feature bodies (every gear). Ceiling: boolean-result bodies
report the last feature (the boolean), not the original — that's Phase F (KV13).
GUI `face-to-feature.spec.js`.

### Phase E — Gear extrude **Tier 2** *(kernel — `KV12` exact path; quality)* — IN PROGRESS (spec `specs/kv12_tier2_arc_extrude.md`; increments E1–E4)
Exact arc → cylinder side patches + arc-bearing planar caps (reuse the revolve
partial-angle assembler). Exact volume + true fillet walls. Bulk of cost =
exact arc-loop simplicity validation (arc–segment / arc–arc predicates). Pulls
in the KV7 curved-boolean caveat if gears are used as boolean operands. **Not on
the print critical path.**
- **E1 ✅ DONE (2026-06-14).** `ProfileRegion::ArcPolygon` + `extrude_arc_profile`
  direct assembler (mirrors `build_partial_revolve`, linear seams, per-edge
  cylinder/plane walls). Quarter-disk sector test: exact `signed_volume = πR²H/4`,
  watertight, 1 cylinder patch, typed rejections. `tests/kv12_tier2_arc_extrude.rs`.
  Kernel-only — not yet wired to the adapter / WASM (E4).
- **E2 ✅ DONE (2026-06-14).** Proved the (already k-general) assembler on
  rounded rectangle (multiple convex arcs), vesica lens (consecutive arcs,
  k=2), and a concave-arc bite (`reversed` cylinder). No kernel change; fixtures
  only. `tests/kv12_tier2_arc_extrude.rs`.
- **E3 ✅ DONE (2026-06-14).** Exact arc-loop simplicity: `Profile::arc_polygon`
  rejects self-intersecting line/arc boundaries (`ProfileNotSimple`) with NO
  sampling. Minor-arc membership = exact chord-side test; line∩circle /
  circle∩circle crossings decided by a compare-root-vs-rational predicate over
  `dashu` (the candidate points stay symbolic). New `exact2d` arc predicates +
  5 unit tests (incl. adversarial near-touch) + 6 profile RED/GREEN cases.
- **E4 ✅ DONE (2026-06-14).** Adapter wiring: `make_faces_from_profiles`
  reconstructs an `ArcPolygon` from `arc_segments` + the chord polygon (arc runs
  → minor sub-arcs, splitting a semicircle into 2 patches; wrap-aware), routes
  single arc loops through exact Tier-2, falling back LOUDLY to the Tier-1
  chord polygon on anything it declines. Adapter + kv8 + GUI tests; WASM rebuilt.
- **E4b ✅ DONE (2026-06-14).** Holed arc Tier 2: the assembler generalized to
  multi-loop (holes wound CW so the same per-edge code gives annular caps + into-
  the-cavity walls; genus = hole count) + exact arc-aware hole containment
  (`point_in_arc_region` ray-cast reusing the E3 predicates). Holed `ArcPolygon`
  outers now extrude with exact cylinder walls AND holes. Kernel annulus/genus
  tests + containment RED tests + adapter Tier-2 holed test.
- **Phase E COMPLETE.** Arc/rounded profiles and gears (with or without bores)
  extrude with exact cylinder walls.

### Phase F — Provenance / topological naming *(kernel + app — `KV13`; capstone)* — ✅ COMPLETE (2026-06-14, shipped scope): F1–F3 kernel substrate (Pid + journal + lineage), F5 contract, F6a–c bidirectional face↔feature UI, F7 verification matrix. F4 deep stable-Pid machinery designed (`specs/kv13_f4_design.md`) + deferred (goal met via recompute; consumer-less). Detail: `specs/kv13_provenance_naming.md`.
"Click any face → the feature that *introduced* it, through chained
booleans/extrudes," plus the inverse (feature → its faces), surviving rebuilds.
The persistent-naming investment (`docs/PERSISTENT-NAMING.md`). Substrate exists
(yang `TriangleAttribution`, modeling-ops `Provenance`, GeomRef Role/Signature).
**Scope decision (2026-06-14): full Parasolid-grade** — persistent entity tags
(`Pid`) + an operation journal *integrated into the kernel* (not the
role+signature heuristic alone), surviving arbitrary upstream edits.
Week-scale, multi-subagent. **Detail: `specs/kv13_provenance_naming.md`**
(architecture + F1–F7); roadmap stub in `yang_functional_roadmap.md` KV13.
Increments: F1 kernel `Pid` tags → F2 operation journal + boolean attribution
(consume yang `TriangleAttribution`) → F3 `FaceOrigin` for the current model →
**F4 persistent identity across rebuild (the long pole; F4a–F4d)** → F5
`get_face_data` emits `created_by_feature` → F6 UI face→feature (through
booleans) + inverse → F7 verification matrix + adversarial no-mislabel.
F1–F3/F5/F6 deliver current-model lineage (gearbox-grade); F4 hardens it for
arbitrary edits. Strictly after the gearbox; for the prototype-release
announcement, not the print.

## Sequencing summary

```
A (assay timeout) ─► B (gear Tier 1, KV12) ─► C (assembly + per-body export) ─► PRINT
                     D (face→feature Tier 1)  ── parallel, cheap ──┘
E (gear Tier 2)  ─┐
F (provenance, KV13) ─┴─ pre-release follow-ups (after print)
```

Note: this sequence intentionally puts **gears before revolve** (KV6c/d) — they
are independent, and the gearbox needs gears, not revolve.

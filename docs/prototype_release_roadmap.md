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
- **C2** — ring (internal) gear: verify the internal-teeth profile (non-convex
  *hole*) survives KV12's polygon path — the most likely edge case.
- **C3** — **right-click a body in the Bodies list → Export STL** (per-body,
  named). Verify the STL path emits every body of a multi-body model and that
  gear meshes are watertight for slicing. STEP added to the same menu when the
  STEP milestone lands.
- **Done when: the gearbox prints. ← prototype-release gate.**

### Phase D — Face → feature **Tier 1** *(app/UX; cheap parallel win)*
Home: `projects/04-3d-viewport/PLAN.md`.
Click a face → highlight its producing feature in the tree. The clicked
`GeomRef` already carries `anchor.feature_id`; this is store + tree-highlight
wiring, no kernel work. Exact for single-feature bodies (every gear). Ceiling:
boolean-result bodies report the last feature (the boolean), not the original —
that's Phase F.

### Phase E — Gear extrude **Tier 2** *(kernel — `KV12` exact path; quality)*
Exact arc → cylinder side patches + arc-bearing planar caps (reuse the revolve
partial-angle assembler). Exact volume + true fillet walls. Bulk of cost =
exact arc-loop simplicity validation (arc–segment / arc–arc predicates). Pulls
in the KV7 curved-boolean caveat if gears are used as boolean operands. **Not on
the print critical path.**

### Phase F — Provenance / topological naming *(kernel + app — `KV13`; capstone)*
"Click any face → the feature that *created* it, through chained
booleans/extrudes," plus the inverse (feature → its faces), surviving rebuilds.
The persistent-naming investment (`docs/PERSISTENT-NAMING.md`). Substrate exists
(yang `TriangleAttribution`, modeling-ops `Provenance`). Week-scale,
multi-subagent. **Detail: see `yang_functional_roadmap.md` KV13.** Strictly after
the gearbox; important for the prototype-release announcement, not for the print.

## Sequencing summary

```
A (assay timeout) ─► B (gear Tier 1, KV12) ─► C (assembly + per-body export) ─► PRINT
                     D (face→feature Tier 1)  ── parallel, cheap ──┘
E (gear Tier 2)  ─┐
F (provenance, KV13) ─┴─ pre-release follow-ups (after print)
```

Note: this sequence intentionally puts **gears before revolve** (KV6c/d) — they
are independent, and the gearbox needs gears, not revolve.

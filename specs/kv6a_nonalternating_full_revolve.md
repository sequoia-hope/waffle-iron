# KV6a-tilted — full-turn revolve of non-alternating profiles

**Status:** IMPLEMENTING (2026-07-11)
**Driver:** C0070 (`revolve composition: full revolve about the tilted axis
(1,1,1)/√3 (axis-alignment probe) [KV6a-tilted]`) — ERROR
`NotImplemented("PR-KV6a full-turn revolve of non-alternating profiles")`.
**Crate:** `crates/kernel-v2` (`construct.rs::build_full_revolve`).

## Goal

A full-turn (360°) polygon revolve whose profile has **consecutive wall
edges** (parallel/oblique — cylinders/cones) builds the genus-1 ring
directly. Today `build_full_revolve` demands strict wall/annulus alternation;
a rectangle revolved about a **tilted in-plane axis** (C0070) classifies all
four edges as Oblique (four cone frusta, zero annuli) and dies at the
alternation gate.

## Research basis

- Stroud 2006 §3.1.4 (single-fake-edge closed curved edges) — the existing
  KV5a/KV6a rim + seam vocabulary; no new topology is introduced.
- P8: no new algorithm — this removes an artificial input restriction on the
  existing washer assembler. Surface vocabulary (cones KV6c, cylinders KV5a,
  annuli KV6a) is unchanged.

## Why the existing twin structure already supports wall-wall adjacency

- `rim_on_edge(edge, vertex)` is class-agnostic: a wall's rim half-edge at
  shared vertex `v` twins with the *neighbour edge's* rim half-edge at `v`,
  whatever class the neighbour is.
- Rim-normal consistency (the invariant the alternation gate was protecting):
  for a wall edge the rim normal at the edge's **head** vertex evaluates to a
  fixed sign of `â` and at its **tail** vertex to the opposite sign,
  *independent of the edge's `reversed` flag*. Proof: the assembler sets
  `n_tail = rev ? −toward : +toward`, `n_head = rev ? +toward : −toward` with
  `toward = sign(Δt)·â`, and for a CCW profile `rev ⟺ sign(Δt)` is fixed
  (outward side determines `rev`, and outward flips exactly when the axial
  direction of travel flips). Substituting kills both signs: every wall rim
  half-edge carries `+â` at one end-role and `−â` at the other. Two adjacent
  walls meet head-to-tail, so their twin rims always carry **opposite**
  directional normals — exactly the curve-twin rule `validate_solid` enforces.
  (The alternating washer shipping today is the special case where the
  neighbour is an annulus.)

## Parameters

Unchanged — `revolve(arena, profile, axis_origin, axis_direction, angle)`
with `|angle − 2π| ≤ REVOLVE_FULL_TURN_TOLERANCE`.

## Branch table

| # | Profile shape (full turn, strictly off-axis) | Behavior |
|---|---|---|
| 1 | Alternating wall/annulus (rectangle, axis-aligned) | UNCHANGED (existing washer path, byte-identical) |
| 2 | All-oblique (tilted-axis rectangle → 4 cone frusta, C0070) | NEW: builds; no planar face exists → `start_cap = end_cap = None` |
| 3 | Mixed staircase (wall-wall junction, ≥1 annulus pair) | NEW: builds; extreme ∓â annuli are the caps, extra faces are walls |
| 4 | Consecutive **annular** edges (two adjacent axis-perpendicular edges — a subdivided radial edge) | typed `NotImplemented` (coplanar adjacent same-surface faces; out of scope) |
| 5 | Axis touching/crossing profile | UNCHANGED (on-axis lathe recovery / typed error) |

## API change

`RevolveResult.start_cap` / `end_cap`: `FaceId` → `Option<FaceId>`. A
capless ring (branch 2) has no planar face to name. All existing builders
return `Some(..)`; only the adapter consumes `RevolveResult` in production
and it reads only `.solid`.

## Invariants & oracles

1. **Census (branch 2, diamond):** V=4, E=8 (4 rims + 4 seams), F=4, R=0,
   shell genus 1 ⇒ χ = 0 = 2(S−G). `validate_solid` passes (twin pairing,
   curve-twin consistency, curved orientation rules, Euler–Poincaré).
2. **Census (branch 3, pentagon):** V=5, E=8 (5 rims + 3 seams), F=5, R=2,
   genus 1 ⇒ χ = 0.
3. **Analytic volume (Pappus, `geom::signed_volume`, ≤1e-12 rel):**
   diamond half-diagonal 1 centred (0,2) about x-axis → `8π`;
   pentagon fixture → `20π`.
4. **Tessellation:** watertight position-paired mesh, mesh volume within the
   chord band of Pappus.
5. **Caps:** branch 2 → both `None`; branch 3 → `Some` with outward `∓â`;
   existing washer → unchanged `Some`.
6. **Determinism:** bit-identical arena on repeat build.

## Failure modes

- Branch 4 keeps a typed `NotImplemented` (message updated to name the
  *actual* residual restriction: consecutive annular edges).
- Axis-through-profile behavior unchanged (`KernelError::Other` mapping in
  the adapter — never the NotSupported marker).

## Corpus / meta correction (R0099 precedent)

C0070's `euler_target: 2` is an authoring error: a full-turn revolve of a
simple profile strictly off-axis is a solid-torus-like ring — genus 1,
χ = 0 (this is forced by `validate_revolve_geometry`, which rejects
axis-touching profiles before this path). Fix `gen_complexity.rs`
(`Knobs::tracker(2, 4.0)` → `Knobs::tracker(0, 4.0)`) and hand-edit
`app/tests/cases/assay/C0070.meta.json` (no corpus regen — regen is not
byte-stable). `compute_euler_target` in `gen.rs` is not involved (C-series
knobs are authored per case).

## Acceptance

- New kernel-v2 tests red → green; existing kv6a suite green (branch 1
  byte-identical).
- `cargo test -p kernel-v2`, clippy, fmt clean.
- Assay: C0070 ERROR → SUPPORTED_CORRECT; zero cases lost.

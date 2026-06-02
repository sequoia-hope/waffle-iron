# PR-YR13 — curved Subtract: box − cylinder blind pocket (cavity-sense via `BRepFace.reversed`)

**Milestone:** M5 / Phase 2 step (curved `Subtract` cavity-sense — the first
M5 increment after the curved-Stage-1 primitives PR-YR7/PR-YR12 and the curved
`Union` chain PR-YR8–PR-YR11).
**Predecessor:** PR-YR8 (cylinder ∪ box, where curved cavity-sense was banked as
a deferral), PR-YR12 (sphere Stage-1 tessellation).
**Crate:** `crates/yang-rs/` only.

## Goal (narrow, single cycle)

Close the curved cavity-sense gap for **`box − cylinder`, BLIND POCKET only**
(genus 0, χ=2). The surviving cylinder-lateral cavity wall must be emitted as a
`Surface::Cylinder` carrying the input cylinder's **exact** params, with a new
`BRepFace.reversed == true` flag whose meaning is: *the face's effective outward
normal (outward from the result solid) is the negation of the surface's canonical
analytic outward normal.* For a subtracted cylinder cavity wall the canonical
outward normal points away from the axis; negated, it points **toward the axis**
(into the pocket) — which is the correct outward-from-the-result-solid direction.

Mesh winding and B-Rep face sense are derived from the **same** `flip_for_op`
signal, so they are provably consistent.

## Why a new flag (not a surface mutation)

The planar reconstruction branch (`src/lib.rs:3525-3533`) re-derives a face's
outward normal from its cycle winding and **flips the stored `Plane.normal`**
when the winding opposes the inherited normal — so a subtracted planar cavity
wall already encodes correct sense in `Plane.normal`. A curved `Surface`, by
contrast, has **no way to encode "cavity"**: a `Surface::Cylinder`'s canonical
outward side is fixed (away-from-axis) and the params (`axis_point`, `axis_dir`,
`radius`) must stay exact for downstream SSI / kernel-v2 consumers — we must NOT
perturb them to signal a flip. So sense is recorded out-of-band in an explicit
`reversed: bool` flag (approved **Option A**). Planar faces keep encoding sense
in `Plane.normal` and keep `reversed == false` (no double-flip).

## Why the mechanism is sound (no STOP condition)

Face resolution requires `surf.len() == 1` (`src/lib.rs:2392`), so for `Subtract`:

```
flip_for_op(Subtract, la, t) == !on_a == true   ⟺   patch input == InputId::B
```

That exact signal is already carried into reconstruction as `PatchInfo.input`
(`src/lib.rs:2592`, set in `compute_phase_a` at `src/lib.rs:2622-2627`). So a
curved cavity wall is **provably** the set of curved patches with
`op == Subtract && input == InputId::B` — the same triangles `flip_for_op`
flipped at mesh-compaction time (`src/lib.rs:2349`). We derive `reversed` from
that signal, NOT a new classification ⇒ mesh winding and B-Rep face sense are
guaranteed consistent.

## Scope

- **In:** `box − cylinder`, blind pocket (cylinder open top at/above box top,
  closed bottom cap inside the box → cylindrical pocket in a box). Cavity walls =
  cylinder lateral (curved — the new thing) + cylinder bottom cap (planar floor,
  already handled by the planar normal-flip). One exact `Circle` rim edge
  (cylinder ∩ box-top plane).
- **Deferred — do NOT attempt:** through-hole (genus 1, χ=0); sphere/cone
  cavities (`Cone` still rejects loudly); box-as-subtrahend. Curved `Union` and
  planar `Subtract` paths stay **byte-for-byte** identical. No new `ssi-rs` work.

## Branch table (`emit_topology`, `src/lib.rs`)

| Branch | Surface | Sense encoding | `reversed` |
|---|---|---|---|
| Planar (`src/lib.rs:3471+`) | `Plane` | possibly-flipped `Plane.normal` (winding-derived) | `false` (always) |
| Curved (`src/lib.rs:3403+`) | `Cylinder` | surface inherited UNCHANGED; flag records flip | `op == Subtract && info.input == InputId::B` |

`compute_phase_a` does **not** need `op` (pure geometry). The `op: BoolOp`
parameter is threaded `boolean()` → `reconstruct_topology_stage4` →
`emit_topology`. The `#[cfg(test)]` `reconstruct_topology` passes `BoolOp::Union`
(its fixtures are union/planar → `reversed = false`, byte-identical).

## Invariants

- **I-rev1 (consistency):** `reversed` and the mesh winding (`flip_for_op`)
  derive from the same `op == Subtract && input == InputId::B` condition — never
  independently classified.
- **I-rev2 (no double-flip):** planar faces always emit `reversed == false`
  (sense already in `Plane.normal`); a curved cavity wall emits `reversed == true`
  with the surface params UNCHANGED.
- **I-rev3 (exact params):** the cavity-wall `Surface::Cylinder` equals the input
  cylinder's surface field-for-field (`axis_point`, `axis_dir`, `radius`).
- **I-rev4 (byte-identity):** Union and planar Subtract paths emit
  `reversed == false` everywhere → behaviorally unchanged. The existing planar
  900-case fuzz + curved Union YR8–YR12 oracles are unaffected.

## Oracles (RED — `crates/yang-rs/tests/yr13_subtract_cylinder.rs`)

Drive the public `boolean(&box, &cyl, BoolOp::Subtract, &mock)` with a hand-built
`LabeledArrangement` via a `LabelMock` (pattern: `yr8_adversary.rs:356`,
`yr10_stage4_relocate.rs:479`; Subtract keep-rule + `inside`/`surface` label
semantics: `cherchi-rs/src/labeled_arrangement.rs:95-98` and `m3_adversary.rs`).
`a` = box (`InputId::A` / label id 0), `b` = cylinder (`InputId::B` / label id 1).
The sidecar-independent direct path is the GREEN gate; the E2E is env-gated.
Reuse the in-file `unpaired_half_edges` / `euler_characteristic` helpers
(`end_to_end.rs:139-173`), and the cylinder/box fixtures
(`yr7_cylinder.rs:93-215` / `m1_inputcheck.rs:36`).

1. **Cavity-sense correct:** for the surviving cylinder-lateral wall, the
   *effective* outward normal — analytic away-from-axis, negated because
   `reversed` — points **toward the axis** (into the pocket). Sampled at several
   wall points and asserted explicitly (NOT the canonical away-from-axis).
2. **Watertight 2-manifold, χ=2:** `unpaired_half_edges(mesh) == 0`,
   `euler_characteristic(mesh) == 2`.
3. **Analytic surface survives:** cavity wall is `Surface::Cylinder` with the
   input cylinder's exact params and `reversed == true`; box outer faces are
   `Surface::Plane` with `reversed == false`.
4. **Sidecar mesh-parity** (env-gated on `CHERCHI2022_BIN`, LOUD eprintln skip
   via `SidecarBoolean::from_env()`): output mesh == sidecar `Subtract` of the
   two Stage-1 tessellations.
5. **Exact rim edge:** cylinder ∩ box-top section is a `Curve::Circle`.
6. **Determinism;** planar `fuzz_boxes` (incl. planar Subtract) unregressed;
   curved Union YR8–YR12 unregressed (covered by the full-crate gate).

## Failure modes

- **F-rev1 — wrong sense:** `reversed` left `false` on a curved cavity wall →
  effective normal points away-from-axis (out of the pocket). Oracle 1 catches it.
- **F-rev2 — double-flip:** setting `reversed == true` on a planar cavity wall
  whose `Plane.normal` was already flipped → I-rev2 violation. Oracle 3 +
  planar-fuzz regression catch it.
- **F-rev3 — param perturbation:** mutating the surface params to signal the flip
  → I-rev3 violation. Oracle 3 catches it.
- **STOP (P9/P10):** if blind-pocket topology or cavity detection cannot be made
  correct without faking a sense, STOP and report — do NOT guess. (The analysis
  above shows it can be derived from the existing `flip_for_op` signal.)

## Research basis

- **Yang et al. 2025 §4.4.2 / §4.5** — Stage-6 B-Rep reassembly: a kept face
  inherits its analytical surface; for a subtracted subtrahend the bounding face
  is a cavity wall whose outward orientation reverses. (`refs/text/yang2025_hybrid_boolean.txt`.)
- **Cherchi 2022 `booleans.cpp` `boolSubtraction` (1480-1483)** — the kept
  B-surface triangles bounding the carved cavity are flipped so their outward
  normal points into A. `flip_for_op` (`src/lib.rs:2221`) mirrors this; PR-YR13's
  `reversed` flag is the B-Rep-face-level shadow of that same flip.

## Migration (faithful, behavior-preserving)

Every `BRepFace { … }` struct literal in `src/lib.rs` and `tests/*.rs` gains
`reversed: false`, EXCEPT the two `emit_topology` production sites
(`src/lib.rs:3463` curved → computed value; `src/lib.rs:3573` planar → `false`).
No structural or numeric assertion changes anywhere; only the new field is added.

## On completion

- `docs/yang_functional_roadmap.md`: add **PR-YR13 — curved Subtract box − cylinder,
  cavity-sense via `BRepFace.reversed` ✅ DONE** in the YR8–YR12 style; note
  Remaining: through-hole genus-1, sphere/cone cavities, side-face/corner guard.
- Resolve the banked deferral notes in `src/lib.rs` (`106-110`, `3399-3401`):
  the box−cylinder blind-pocket cavity-sense is now implemented; restate the
  still-deferred items.

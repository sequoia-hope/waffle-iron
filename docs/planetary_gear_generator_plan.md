# Plan: Planetary gear stage generator (task #31)

A generator that places a **sun**, **N planets**, and a **ring** gear in one
sketch — meshing, with user-defined backlash — extrudable into a planetary
stage. Builds on the existing involute gear generator.

## Foundation (existing, reuse — do NOT reinvent)
- `crates/waffle-types/src/gear.rs`: `GearParams { tooth_count, module, pressure_angle_deg, backlash, center_x, center_y, rotation_offset, internal }` + `generate_gear_profile(&GearParams) -> GearProfileResult { entities, positions, profiles, pitch/base/addendum/dedendum_radius }`. **`internal: true` already produces a ring gear** (teeth inward). Backlash thins each tooth by `backlash_angle = backlash/(2·pitch_r)` per flank.
- `SketchEntity::Gear { id, params, construction }` — a gear is ONE compact sketch entity, expanded on demand. App: `createGear(params)` adds it to the current sketch; `GearDialog.svelte` is the UI.

## Gear theory (standard involute, all gears SAME module m + pressure angle α)
- Pitch radius `r = Z·m/2` (Z = tooth count). Sun `r_s`, planet `r_p`, ring `r_r`.
- **Tooth-count meshing constraint:** `Z_r = Z_s + 2·Z_p`. (Concentric sun+ring with planets between; exact for equal-module standard gears.)
- **Carrier (sun-planet center distance):** `R_c = r_s + r_p = (Z_s + Z_p)·m/2`. Ring is concentric with the sun (center 0,0); planet-ring center distance `= r_r − r_p = R_c` ✓ (consistent — that's why `Z_r=Z_s+2Z_p`).
- **Assembly condition (equal-spaced planets):** `(Z_s + Z_r)` divisible by `N`. Required so N identical planets can be placed at equal angles and all mesh.
- **Planet non-interference:** adjacent planets must not collide — tip circles clear: `2·(r_p + addendum) < 2·R_c·sin(π/N)`, i.e. `r_p + m < R_c·sin(π/N)`. (addendum = m.)

## Placement & phasing
- Sun: `center=(0,0)`, `rotation_offset = 0`, `internal=false`.
- Planet k (k=0..N−1): carrier angle `ψ_k = 2π·k/N`; `center = (R_c·cos ψ_k, R_c·sin ψ_k)`; `internal=false`.
- Ring: `center=(0,0)`, `internal=true`.
- **Planet rotation_offset (the crux — get the phasing right):** the planet must present a tooth-space toward the sun conjugate to the sun's surface phase at `ψ_k`. Derivation (tooth i centered at `rotation_offset + i·2π/Z`, spaces at the half-pitch between):
  - sun phase toward planet k: `frac_s = (Z_s·ψ_k / 2π) mod 1` (0 = tooth center, 0.5 = space center).
  - require planet's phase toward sun `= frac_s + 0.5 (mod 1)` (tooth↔space at the pitch point).
  - ⇒ `rotation_offset_p_k = ψ_k·(1 − Z_s/Z_p) + π·(1 − 1/Z_p)  (mod 2π/Z_p)`.
  - Ring phasing toward the planet is then automatically consistent **iff** `Z_r=Z_s+2Z_p` and the assembly condition hold (that is what those constraints buy). The ring's own `rotation_offset` is 0 (its internal teeth conjugate the sun's spaces along each radius; verify).
  - **This derivation is sign/convention-sensitive — DO NOT trust it blindly. VERIFY with the meshing oracle below; if it disagrees, fix the formula, don't fudge the gears.**

## Backlash distribution
User gives ONE backlash `B` (per mesh, linear at the pitch circle). Backlash at a mesh = (tooth-thinning of gear A) + (gear B). A planet is shared by two meshes (sun-side, ring-side). Set **every gear's `GearParams.backlash = B/2`** ⇒ each mesh gets `B/2 + B/2 = B`. (Sun-planet: `b_s/2 + b_p/2`… with all = B/2, mesh = B. Planet-ring: same.) Simple, correct, symmetric. (A future option: independent sun-planet vs planet-ring backlash → distribute per side; out of scope v1.)

## Validation, hints, optional auto-adjust (the "ensure they mesh" requirement)
Compute and surface, in the dialog, BEFORE creating:
1. `Z_r = Z_s + 2·Z_p`? If user-set `Z_r` differs → hint "Ring teeth must be Z_s+2·Z_p = {X} (currently {Y})". (v1: derive `Z_r` from `Z_s,Z_p` — it's not a free input; show it computed. If we expose `Z_r` as input, validate + offer auto-fix.)
2. `(Z_s + Z_r) % N == 0`? If not → hint "For {N} equally-spaced planets, (Z_s+Z_r)={S} must be divisible by N. Valid N: {divisors of S}. Or change Z_s/Z_p." Optional auto-adjust: snap N to the nearest valid divisor.
3. Non-interference `r_p + m < R_c·sin(π/N)`? If violated → hint "{N} planets of {Z_p} teeth collide; reduce N to ≤ {max}, or increase Z_s." 
4. All gears same module/α (enforced — single inputs).
Mode toggle: **Hint only** (block create, show what to change) vs **Auto-adjust** (snap the dependent value: derive Z_r; snap N; clamp). Never silently produce a non-meshing or colliding set.

## Architecture decision
**v1 = a generator that emits N+2 individual `SketchEntity::Gear` entities** into the active sketch (reuses ALL existing gear infra: expansion, rendering, hover, extrude region detection, file format — zero new entity plumbing). The Rust core is a pure function `generate_planetary(&PlanetaryParams) -> PlanetaryResult { gears: Vec<GearParams>, validation: {...}, radii }` in `crates/waffle-types/src/gear_planetary.rs` that validates + computes each positioned `GearParams`. The app's `createPlanetary(params)` runs it and adds each gear via the existing add-entity path.
- **Future (a):** a single re-editable `SketchEntity::PlanetaryGear { params }` element (compact, edit-as-one-unit). Cleaner UX but large plumbing (enum variant + every exhaustive match + expansion + registry + migrate). Defer until v1 is validated.
- **"Meshed spur gears" primitive:** the planet-placement reuses the sun-planet center-distance + phasing logic; expose it as an internal helper `mesh_external(sun, planet_teeth, carrier_angle) -> GearParams` reused per planet. A user-facing two-gear "meshed spur" element can wrap the same helper later — define the helper now, the element later.

## The meshing ORACLE (the correctness gate — root in geometry, not hope)
A Rust test that, for a generated stage, samples each gear's involute flanks near each pitch point and asserts: (a) NO flank overlap (gears don't interpenetrate), (b) the gap at the pitch line equals the intended backlash `B` within tolerance, at every sun-planet and planet-ring mesh, for several {Z_s,Z_p,N} combos. This catches a wrong phasing formula. Plus: assembly-condition + interference validation unit tests; a degenerate-input test (non-meshing combo → loud validation error, never a silent bad sketch).

## Build steps
1. `crates/waffle-types/src/gear_planetary.rs`: `PlanetaryParams { module, pressure_angle_deg, sun_teeth, planet_teeth, planet_count, backlash, auto_adjust }`, `generate_planetary` (validate → compute Z_r, R_c, per-gear positioned GearParams with B/2 backlash + the phasing), `PlanetaryResult` with the validation report (ok / hints / adjusted values). Pure, unit-tested incl. the meshing oracle. Export from lib.rs.
2. App `createPlanetary(params)` (store) → runs the generator (via the bridge/WASM) and adds the N+2 Gear entities to the active sketch in one undo step; surface validation hints as toasts/inline.
3. `PlanetaryGearDialog.svelte`: inputs (module, pressure angle, sun teeth, planet teeth, planet count, backlash, hint-vs-auto-adjust toggle); live-computed Z_r, ring/carrier radii, validation messages; Create. Toolbar entry to open it (like GearDialog).
4. WASM rebuild (Rust changed). GUI test (create a planetary, assert N+2 gears in the sketch, extrudable, no crash) + the Rust meshing oracle + validation tests.

## Discipline
- Root the gear math in standard involute theory; the meshing oracle is the proof. NEVER ship a non-meshing or self-colliding stage — validate loudly.
- All gears share module + α (meshing requirement). Units: module/backlash in the document display unit → internal meters (the offset-plane unit bug: convert in the dialog).
- No `unsafe`/`panic!`/`unwrap`/`expect` in production; WASM-clean. GUI test per project rules.

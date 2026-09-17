# Planetary gearbox over the agent link (2026-09-17)

A planetary stage built end to end through the REAL agent link: `waffle-mcp-relay`
driven as an MCP client over stdio, paired with a headless Chromium page on the
dev server — the harness of `app/tests/gui/agent-*.spec.js`, scripted. It is the
first non-trivial multi-part document authored entirely by tool calls after the
S3 migration (every authoring tool runs in the engine).

| File | What |
|---|---|
| `gearbox.mjs` | the build script (run from `app/`: `STAGE_JSON="$(…planetary_calc…)" node gearbox.mjs`) |
| `planetary_calc.rs` | scratch `main` over `waffle_types::gear_planetary::generate_planetary` that prints `stage.json` |
| `stage.json` | the generated stage: positioned `GearParams` for sun, planets and ring, carrier radius |
| `calls.jsonl` | every tool call of the final run with its `structuredContent` |
| `Planetary gearbox.waffle.json` | the saved document (`buildDocumentJson`); also at the repo root for opening |
| `capture-*.png` | `viewport_capture` of each tab (`iso` + `fit`), and `capture-stage-axial.png` looking down the axis |
| `repro_consumed_target.mjs` | 12-line reproduction of the feature-engine defect below |

## The stage

Module 1.5 mm, 20° pressure angle, **sun 18 / planet 12 × 3 / ring 42** teeth
(`Z_r = Z_s + 2·Z_p`; `(Z_s + Z_r) = 60` divisible by 3), 50 µm backlash per
mesh (25 µm on each gear), carrier radius 22.5 mm, face width 10 mm, ring rim
Ø76 mm. Ratio with the ring fixed: 1 + 42/18 = 3.33:1.

Five Part tabs: **Sun gear** (Ø6 mm bore), **Planet gear** (Ø5 mm bore),
**Ring gear** (rim minus the internal-gear prism), **Carrier** (Ø56 × 4 mm plate,
three Ø4.8 mm pins, Ø16 mm output boss, Ø10 mm clearance hole) and **Stage**:
every part positioned as generated, meshing, with the carrier plate 1 mm
behind the gears and its pins through the planets. Each part carries a
`MateConnector` on its axis. The stage measures: sun 5259.9 mm³, planets
2218.9 mm³ each, ring 14043.3 mm³, carrier 10503.5 mm³; 93 s for 30 sketches,
30 extrudes (11 booleans), 6 connectors, 5 saves, 6 captures.

Modeling choices that matter over the link:

- **A gear is one `Gear` sketch entity**, extruded with `profile_index: 0`
  from a sketch holding only that gear. A multi-gear sketch gave regions
  without addressable loops (see findings).
- **Every boolean tool starts OFF the target's faces**: bores and the ring's
  tooth prism are sketched 2 mm before the body and extruded 4 mm longer than
  it, so no cap is coplanar (the M8 `NotSupported` boundary). Pins and the boss
  start INSIDE the plate for the same reason.
- **Chained booleans target the previous boolean's `Main`**, never the
  original body — see the defect.
- The extrude direction for an `{origin, normal}` plane was MEASURED on the
  first body (`+z` here) before any offset plane was placed.
- `viewport_view` names assume Y-up: for this Z-up stage `top` is a side
  profile and `front` looks down the axis.

## Findings

1. **feature-engine — silent duplicate on a consumed target** (recorded in
   `projects/06-feature-engine/PLAN.md` Blockers). An explicit `Strict`
   combine target naming an output that an earlier combine already consumed
   is resolved to the stale handle: plate → pin 1 `Add` → pin 2 `Add` (both
   targeting the plate) yields two plate-plus-pin bodies of 5127.1 mm³, no
   warning, no error. `resolve_combine_targets` documents `Strict ⇒
   ResolutionFailed`; `find_solid_handle` does not consult `already_consumed`.
2. **No assembly authoring tool.** `tab_add kind:"Assembly"` works but
   `tab_switch` refuses Assembly tabs and nothing exposes `EditAssembly`, so
   the "assembly" is the Stage part with hand-placed bodies.
3. **`body_measure.closed` is `false` for every boolean output**, `true` for
   plain extrudes; volumes are exact and correct.
4. **No progress frames** for minute-long booleans (relay hard-codes
   `progress: false`).
5. **`sketch_regions` on a five-gear sketch returned only sub-regions**
   (`profile_entity_ids: null` everywhere).

Details and next steps: `projects/14-agent-link/PLAN.md` § "Findings from the
planetary-gearbox exercise".

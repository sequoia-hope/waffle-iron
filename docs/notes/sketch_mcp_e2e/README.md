# The sketch system, end to end over the agent link

`sketch_e2e.mjs` drives the **whole sketch door** through the real MCP path — a
local `waffle-mcp-relay` spoken to as an MCP client over stdio, paired with a
headless Chromium page on the dev server — on sketches it authors through the
same link. It is the measurement behind S3 ("an agent can edit a sketch, and
see what the solver sees", `specs/agent_mechanical_design.md` §10.3) and the
first automated test of the sketch system that drives it the way an **agent**
does rather than the way a pointer does.

Run from the repo root with the dev server on `http://localhost:5173`:

```
node docs/notes/sketch_mcp_e2e/sketch_e2e.mjs
```

It writes `out/` (the committed copy is the 2026-10-08 run): `calls.jsonl`
(every call and its `structuredContent`) and `report.json` (the checks and the
measured numbers quoted below).

## What it measured (2026-10-08)

**87 checks, 0 failures, 11 distinct tools, ≈ 2.5 s wall.**

The oracles are closed forms, not recordings. A 60 × 40 mm rectangle's area is
2.4e-3 m²; with one 4 mm corner rounded it is that less `r²(1 − π/4)`;
extruded 6 mm thick it is that times the thickness. A region or a body can only
be right by being right.

| step | what came back |
|---|---|
| `sketch_create`, free rectangle | `dof` **8**, `free` **8** directions, `rows` 0, `rank` 0, `moved` empty, positions exactly as authored |
| `sketch_solve_state` | byte-identical `state` to the one `sketch_create` answered — and nothing committed |
| 7 × `sketch_edit` `AddConstraint` | the DOF ladder **8 → 6 → 5 → 4 → 3 → 2 → 1 → 0**, `free.length == dof` at every rung, `conflicts` empty throughout, `constraints` counting up 1…7 |
| the railed rectangle | `FullyConstrained`, every residual satisfied, `redundant` empty, the four corners at exactly (0,0)–(0.06,0.04) |
| a duplicate `Horizontal` | still `FullyConstrained`; `redundant: [7]` — the **later** of the pair — and `conflicts: []`. A duplicate is over-determined, not contradictory, and S2's decision to keep `Redundant` a field rather than a `SolveStatus` variant is what keeps the sketch green here |
| a second, different `HDistance` | refused `SketchSolveFailed`, `conflicts: [7, 5]`, the whole `state` on the refusal — and `feature_get` byte-identical across the attempt, so the default path provably did not move the document |
| the same edit with `on_error: keep` | committed `OverConstrained`, as the app's own Finish would |
| `MovePoint` (a drag) | the point lands at x = 0.09 exactly; the `Horizontal` holds, so the line stays flat; `moved` names the point that gave way; the pin is reported as `transient_constraints: [{index: 1, kind: "Pinned"}]` — index 1 being one past the single stored constraint — and **no `Pinned` reaches the document** |
| `Fillet` | three points and an `Arc` added, the two legs `changed` (repointed, not re-created, so a constraint on a filleted leg survives — S1 decision 2), two tangents added |
| `undo` after the fillet | one undo takes the arc, both repointed legs and both tangents: 7 constraints and the square area back |
| `Trim` on a cross | the right-hand half goes, the surviving line keeps **id 3** and ends at the crossing (0.05, 0); the released point is pruned |
| `Extend` with no `to` | `NothingToExtendTo` — there is nothing past the cut, and saying so is the right answer |
| `Offset`, `Mirror` | each adds its geometry and names the ids |
| `SetConstruction` | reported as a `changed` entity, not an add |
| `SetDimension` | 60 → 80 mm moves the geometry and the region area follows to 3.2e-3 m² |
| `RemoveEntity` | the line **and** the two points it released (`removed: [4, 5, 6]`) |
| five refusals | `FilletDoesNotFit{corner, radius}`, `NoSuchEntity{id}`, `NoSuchConstraint{index}`, a relay-side `SchemaRejected` for an empty `ops`, `FeatureNotFound` — each typed, none a silent no-op |
| `feature_add` Extrude on the edited sketch | volume **1.44000000012e-5 m³** against 1.44e-5 exactly (8.3e-11 relative) |
| the rounded plate extruded | volume **1.4379398223686154e-5 m³** against the analytic 1.4379398223686153e-5 — agreeing to the last ULP |

## What the run found, and what was changed because of it

### A silent wrong in `sketch_edit`: a fillet extruded as a chamfer

The first run's rounded plate measured **1.4352e-5 m³** where the arithmetic
says 1.4379e-5. The deficit, 4.8e-8 m³, is exactly `r²/2 × t` — the area of the
triangle a **single chord** cuts off the corner. The arc had reached the kernel
as one straight segment: a chamfer, not a round. No error, no warning.

The cause: `sketch_edit` re-derived the sketch's profiles with
`Sketch::recompute_derived`, which goes through `profiles::extract_profiles` —
and that function leaves `arc_segments`, `spline_segments` and `circle` EMPTY.
Only `profiles::build_finish_profiles` fills them, which is why `sketch_create`
has always called it. With no arc record the kernel builds the loop as a
polygon through its vertices.

Fixed: `sketch_edit` commits through `build_finish_profiles`, the same builder
`sketch_create` uses — parity by construction, which is S1's whole thesis.
Pinned by `a_filleted_profile_keeps_the_arc_the_kernel_needs` and
`a_circle_profile_survives_an_edit_as_a_circle`
(`crates/wasm-bridge/tests/tool_sketch_edit.rs`). After the fix the volume is
exact to 1.2e-16 relative, because the kernel builds a true cylindrical face
from the arc (A15, analytical primacy).

**The same loss is still open one crate over.**
`feature_engine::params::apply_sketch` re-derives exactly the same way
(`solved_positions.clear(); solved_profiles.clear(); recompute_derived()`) when
a dimension EXPRESSION re-solves a stored sketch, so a parameter change on a
filleted or circular profile should degrade it the same way. Not fixed here —
it is a different sub-project and it will move stored bytes, so it wants its own
increment — but it is the same mechanism and the same silence.

### An agent's extrude cannot survive an edit to its own sketch

Rounding a corner of a sketch that something is **already** extruded from
fails: `ProfileNotFound{entity_ids: [5,6,7,8], count: 1}`, and the edit rolls
back. The fillet keeps lines 6 and 7 and adds arc 12, so the loop becomes
`{5,6,12,7,8}` where the extrude stored `[5,6,7,8]`, and
`rebuild::resolve_profile_index` matches that set **exactly**.

The re-resolution that would survive it already exists:
`rebuild::resolve_extrude_regions` re-resolves a stored `Region` by boundary
identity and pushes a warning when it cannot. But it covers only the
`region`/`regions` path — the one the **app's** writers use. The agent is told
to address profiles by `profile_entity_ids` (it is what `sketch_create` and
`sketch_regions` hand back, and the engine schema says in as many words that
"the app's own writers address by index and leave this `None`"), which is the
one addressing mode no re-resolution covers.

S3 did not cause this. It made sketch editing routine, which is what brought a
latent gap into reach. It is loud and it rolls back, so it is a capability gap
rather than a silent wrong, and the fix belongs with the profile-addressing
path — putting the agent on the identity-resolved one — not with a tolerance.
Recorded in the script as an expected refusal with its own checks, so the day
it starts working the script says so.

### Two relative floors under every area and volume, neither a defect

Worth knowing before writing any oracle against these numbers:

- **A region's `area_m2` is measured on the slicer's grid.** `compute_regions`
  slices the loops with a library that snaps coordinates onto a fixed float
  grid — `regions.rs` says so at `provenance_eps` — so an **exact** 0.06 × 0.04
  rectangle comes back `2.4000000044703484e-3`, out by 2^-29 relative
  (1.86e-9). Reproduced in pure Rust against `compute_regions` with
  hand-written positions, so it is the slicer, not the page, the solver or the
  link.
- **A solved coordinate carries LM's convergence tail.** A point asked for
  0.005 comes back `0.004999999999500001`, 1e-10 relative. An area multiplies
  two of those and a volume three.

And one asymmetry between the two numbers an agent can read about the same
rounded corner: the **region area** is the chord polygon's (6.6 ppm low on a
4 mm fillet, consistent with `DEFAULT_CHORD_TOLERANCE` = 1e-3 relative), while
the **solid's volume** is analytic and exact. The 2D region is tessellated for
the slicer; the 3D face is not.

## What it does not cover

- `Project` — the op needs world positions the engine resolves from a
  `GeomRef`, which this door does not do yet (S3's notes).
- `Spline`, `Gear` and `Sprocket` entities under the ops. `sketch-solver`'s own
  tests do not cover them either (`entity_mapping.rs` skips them in the param
  layout), so there is nothing to compare against yet; S4's corpus is where
  that belongs.
- The user's own pointer paths. Those are `app/tests/gui/sketch-*.spec.js`;
  this script is deliberately the agent's view of the same system.

# Custom feature scripts, and the modeling features the bicycle asked for

Status: **PROPOSED 2026-09-18** — **B1 patterns LANDED 2026-09-18** (kernel
`transform_body` + `Operation::PatternCircular/PatternLinear`, agent-authorable;
`projects/06-feature-engine/PLAN.md` M12). **A-M0 → A-M2 LANDED 2026-09-19**
(`Operation::Script`, Rhai interpreter, `gear.rhai` bit-identical to the built-in
generator; PLAN M13). Deviations from the text below, as built: (1) API calls
RECORD children and the engine executes them afterwards (sketches are derived
immediately, so `regions()` works; queries are values as §A6 intends) — this is
what keeps the interpreter free of kernel lifetimes; (2) `v6` was not needed —
`SourceKind::Script` is an additive kind under the format's own rule; (3) the gear
`module` parameter is spelled `module_m` (`module` is a Rhai keyword). Next per
Part C: B3 sprocket sketch entity, then A-M3.
Sub-projects: `projects/06-feature-engine/` (owner), `projects/14-agent-link/`
(tools), `projects/09-file-format/` (storage), `projects/08-ui-chrome/`
(feature list, script editor), `kernel-v2` (Part B only).
Touches: `feature-engine` (new operation, interpreter, query resolver),
`waffle-types` (operation + parameter types), `wasm-bridge` (tools, session),
`file-format` (`.waffle` v6 script sources), `app` (feature list, dialogs).

## 0. Summary

Two things, in dependency order:

- **Part A — custom feature scripts.** A user (or an agent) writes a
  parametric feature once, in a small embedded language, and it appears in the
  feature list as one node that regenerates like a built-in. The script calls
  the SAME operations the feature engine already runs — sketch, extrude,
  revolve, boolean, pattern — through one API that the MCP tools also
  expose. Gears move out of the engine into the first script; a roller-chain
  sprocket is the second. This is the Onshape FeatureScript model without a
  bespoke language, hosted inside `feature-engine` so it runs identically in
  the page, in server mode, and in headless tests.
- **Part B — the built-in features a mechanical part needs first.** Ranked
  from the gravel-bicycle build (`docs/notes/`, memory
  `session_2026_09_18c_gravel_bike_assembly_mcp_fixes`): circular and linear
  **patterns**, an analytic **pipe sweep** (circle along a planar line/arc
  chain), a **sprocket** profile generator, and **multi-body union** as a
  first-class step so a part is one solid, not a pile of overlapping bodies.

Part B does not depend on Part A, but every Part B feature is an operation
the script API must be able to call, so their parameter types are designed
once, here.

### 0.1 Why now

The bicycle was 212 tool calls for ten parts. Roughly 60 of those were
repeated extrudes that a circular pattern replaces (spokes, cogs, chainring
bolts, rotor bolts). The handlebar, hoses, saddle rails and chain were
omitted because there is no sweep. Every tube overlaps its neighbours as a
separate `NewBody` because chained booleans on a many-body part are slow and
the failure tail is loud, so the STEP export is 130 solids for 14 parts. The
sprockets are involute gears because that is the only generator.

An agent authoring a part through the link is already writing a generator
(`gravel.py` → `calls.json`). A custom feature is that generator, made
parametric and stored in the document.

### 0.2 Governance fit

- **I1 (one implementation of modeling semantics).** The interpreter lives in
  `feature-engine` and calls the same `Operation` executors as the tree. No
  page-side modeling, no second geometry API for scripts.
- **A2.1 (engine authoritative).** A script's outputs are engine outputs;
  the store mirrors them like any feature's.
- **P9/P10 (no silent wrong).** A script that fails regenerates to a typed
  error on its node, never to stale geometry. A query that resolves to zero
  or many entities is an error, never "first match".
- **Fillet/chamfer/shell stay deferred.** The script API does not expose
  them until they are un-deferred project-wide.

## Part A — Custom feature scripts

### A1. Goals

1. A custom feature is **one node** in the feature list: it has parameters,
   regenerates when they change, can be suppressed, reordered, rolled back
   past, renamed, and its outputs (bodies, faces, edges, connectors) can be
   referenced by later features exactly like a built-in's.
2. A script is **deterministic and sandboxed**: same inputs, same tree, same
   geometry, on every host. No I/O, no clock, no randomness, bounded time
   and memory.
3. The script API is the **operation set the engine already has**, plus
   geometry queries. Nothing a script can do is impossible from the UI or the
   MCP tools, and vice versa.
4. Scripts are **document content**, versioned and shared the way parts are
   (embedded, or a git source), and reusable across documents.
5. **Gear is the first customer.** The built-in involute generator is
   re-expressed as a script and produces byte-identical sketch entities. If it
   cannot, the API is not ready.

### A2. Non-goals

- A general-purpose plugin system with UI extension points, network access,
  or native code. Scripts make geometry; they do not make panels.
- A visual dataflow editor.
- Replacing the expression engine (`feature_engine::expr`). Design parameters
  stay as they are; scripts read them.
- Script-defined **surfaces**. The kernel's surface set (plane, cylinder,
  cone, sphere, torus, and the boolean-derived edges) is the ceiling; a
  script composes those.

### A3. How other systems do it (what to copy, what to avoid)

| System | Model | Take |
|---|---|---|
| Onshape FeatureScript | Server-side sandboxed language; custom feature = function over primitive ops (`opExtrude`, `opBoolean`, sketch) + a query language (`qCreatedBy`, `qNthElement`, …); one tree node; versioned with the document | The shape to copy: same ops as built-ins, queries as first-class values, one node |
| FreeCAD scripted objects | Python `FeaturePython` with an `execute()` recompute; workbenches package them; the gear workbench is our sprocket case | Recompute-on-parameter-change is right; unsandboxed Python is not |
| Fusion 360 Custom Features | Python add-in with a compute callback; lives in the timeline | Confirms the "one node with a compute function" model works in a timeline CAD |
| SolidWorks macros, Creo Pro/Program | Imperative API against the live document; no regeneration contract | Avoid: a script that mutates the tree cannot be regenerated or rolled back |
| OpenSCAD / CadQuery / build123d / replicad | Code-first, the part is the program; parametric libraries as packages | The library ecosystem is the payoff; but a whole-part program does not sit in a feature list next to hand-authored features |

### A4. Architecture

```
FeatureTree
  └─ Feature { operation: Operation::Script { params: ScriptParams } }
        ScriptParams { source: SourceId, entry: String, args: Map<String, ArgValue> }

feature-engine
  ├─ script/interp.rs     Rhai engine, sandbox limits, API registration
  ├─ script/api.rs        the op surface: sketch, extrude, revolve, boolean,
  │                       pattern, datum plane, mate connector, query
  ├─ script/query.rs      Query values → Vec<GeomRef> resolution
  └─ rebuild.rs           Operation::Script arm: run interp, splice outputs
```

**Execution model.** On rebuild, the `Script` arm:

1. Loads the script text from the document's sources table by `SourceId`
   (sources are content-addressed assets already, v4 §2.3; a script is a
   text source like a STEP embed).
2. Builds an interpreter with the API bound to a `ScriptContext` that holds
   the engine state **up to this feature** (rollback semantics come free).
3. Evaluates `entry(args)`. Each API call appends a **child operation** to a
   private sub-tree owned by the node and executes it immediately through the
   ordinary executor, so the script observes real geometry (it can query what
   it just made). The sub-tree is not user-editable and is not persisted; it
   is re-derived on every regeneration.
4. The node's outputs are whatever the sub-tree's last operations leave:
   bodies keyed `<feature_id>/<n>/Main`, faces and edges with provenance
   `created_by_feature = <feature_id>` and a role `Role::ScriptChild { index,
   inner: Box<Role> }` so downstream references survive regeneration.
5. Any error (script runtime, API refusal, kernel error, limit exceeded)
   becomes the node's `feature_error`; the node's outputs are **absent**, not
   the previous ones (P10).

**Why a sub-tree rather than a flat splice.** Undo, rollback, suppression
and reorder all operate on the one visible node. Reordering a script node
reorders all of its children; suppressing it removes all of them. The
built-in `expand_gears` already works this way implicitly (a `Gear` entity
expands to lines and arcs at rebuild); this makes the pattern explicit and
general.

### A5. Language

Options evaluated:

| Option | Runs in wasm32 | Deterministic | Sandbox | Same in page / server / tests | Verdict |
|---|---|---|---|---|---|
| **Rhai** (pure Rust embedded scripting) | yes | yes (no I/O by default; `Instant` etc. not registered) | op/step limits, max string/array size, no modules unless registered | yes: it is inside feature-engine | **Chosen** |
| JavaScript in the page worker | page only | mostly | iframe/worker sandbox | no: server mode has no JS; violates I1 | rejected |
| WASM component per script | yes | yes | strong | yes | rejected for v1: users compile; tooling weight; revisit if Rhai limits bite |
| Extend the expression engine | yes | yes | trivial | yes | too weak: no loops, no geometry values |
| Lua (mlua) / Python (RustPython) | mlua needs a C build for wasm; RustPython is heavy | yes | partial | mostly | rejected: build cost |

Rhai gives closures, arrays, maps, `for`/`while`, string formatting, and a
type-safe way to register Rust functions and types. Limits are set per
evaluation: `max_operations` (interpreter steps), `max_call_levels`,
`max_array_size`, `max_string_size`, and a **geometry budget**
(`max_child_ops`, e.g. 2000) so a runaway loop fails loud in milliseconds.

The script text is UTF-8, stored verbatim. A script declares its interface
in a header block the engine parses before evaluation:

```rhai
// @feature name="Spur gear" version=1
// @param tooth_count: int = 24  min=6  max=400
// @param module_m:    length = 0.002
// @param pressure_angle_deg: angle = 20
// @param face_width: length = 0.010
// @param plane: plane
// @output body: main

fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.gear(#{ tooth_count: p.tooth_count, module: p.module_m,
               pressure_angle_deg: p.pressure_angle_deg });
    let profile = sk.finish().regions()[0];
    ctx.extrude(profile, #{ depth: p.face_width, combine: "NewBody" })
}
```

`@param` declarations drive the feature dialog (typed fields, units, limits)
and the MCP tool's input schema. Parameter values may be expressions over
design parameters (`"module_m": "tooth_pitch / pi"`), evaluated by
`feature_engine::expr` before the script runs, so scripts see numbers in
model units (meters).

### A6. The API surface

Every function below is a thin binding to an existing `Operation` or engine
query. Signatures mirror the MCP tools so the two stay in step (a test pins
them: `tool_inspect.rs`-style, the registered script functions and the
`MIGRATED` tool list must agree on names and parameter names).

**Context**

- `ctx.param(name)` — a resolved parameter (number, string, bool, plane,
  GeomRef, or Query).
- `ctx.units` — read-only: model units are meters; `mm(x)`, `inch(x)`
  helpers.
- `ctx.log(msg)` — appended to the node's info list (shown in the panel);
  never affects geometry.
- `ctx.fail(msg)` — typed script error; regeneration stops.

**Sketch**

- `ctx.sketch(plane) -> SketchBuilder`, `plane` a datum plane id, a planar
  face ref, or `#{ origin, normal }` (same basis rule as `sketch_create`).
- `sk.point / line / circle / arc / spline / polygon / rect / gear /
  sprocket(...)` — the existing entity set; `construction: true` supported.
- `sk.constrain(...)` — the existing constraint set (used sparingly by
  scripts; most scripts place geometry directly).
- `sk.finish() -> SketchRef` with `.regions()` (Rust-computed regions, area,
  `profile_entity_ids`) and `.entity(id)`.

**Solids** (each returns a `FeatureRef` whose outputs are queryable)

- `ctx.extrude(region | [regions], #{ depth, symmetric, direction,
  combine: "NewBody" | "Add" | "Cut" | "Intersect", targets: Query })`
- `ctx.revolve(region, #{ axis, angle_deg, combine, targets })`
- `ctx.boolean(op, targets: Query, tools: Query)`
- `ctx.pattern_circular(seed: Query, #{ axis, count, angle_deg, ... })`
  and `ctx.pattern_linear(seed, #{ direction, count, spacing })` (Part B1)
- `ctx.pipe(path: SketchRef | [edges], #{ radius, inner_radius })` (Part B2)
- `ctx.datum_plane(#{ ... })`, `ctx.mate_connector(#{ frame | geom_ref,
  name })`

**Queries** (values, not results; resolved when consumed)

- `q.created_by(feature_ref)`, `q.role(feature_ref, role)`,
  `q.faces(query)`, `q.edges(query)`, `q.bodies(query)`
- `q.surface_type("planar" | "cylindrical" | ...)`, `q.normal_near([x,y,z],
  tol_deg)`, `q.farthest_along([x,y,z])`, `q.nearest_to([x,y,z])`,
  `q.largest_area()`, `q.nth(i)`
- Composition: `q.created_by(f).faces().surface_type("planar")
  .farthest_along([0,0,1])` — a chain lowers to one `TopoQuery`
  (`waffle_types::topo::{Filter, TieBreak}`); `nth`/`farthest`/`largest` are
  `TieBreak`s. Existing `Selector::{Role, Query, Signature, Position}` are the
  resolution targets; no new selector kind is needed.
- A query consumed where **exactly one** entity is required (a sketch plane,
  a revolve axis) that resolves to 0 or >1 fails loud with the count and the
  candidates' provenance.

Outputs: the script's return value names the node's public outputs
(`#{ main: feature_ref, hub_face: query }`) so later features and mates
reference `Anchor::FeatureOutput { feature_id, output_key: Named("hub_face") }`
without knowing the sub-tree.

### A7. Storage and sharing

- `.waffle` **v6**: `sources[]` gains `kind: "script"` (text). A `Script`
  feature references it by `SourceId`; the same script can back many
  features. `file-format` migration v5 → v6 is additive.
- A script source can be **embedded** or a **git source** (existing
  `sources` machinery, v4 §2.3), which is how a shared library of features
  (`waffle-parts/gears.rhai`) is versioned and pinned per document.
- Script identity for provenance: `(source_id, entry, version)`; the
  `@feature version` bumps when the output topology changes, so a document
  that pins an older version keeps regenerating identically.

### A8. UI and tools

- Feature list: a `Script` node shows its declared name, a script icon, its
  parameters in the property editor (generated from `@param`), and child
  count on hover. Errors and `ctx.log` lines appear on the node.
- A **Script editor** panel (monospace textarea, run button, error line
  numbers) is enough for v1; live re-run on save.
- MCP tools (`projects/14-agent-link`): `script_source_add { text | source }`,
  `script_feature_add { source_id, entry, args }`, `script_run_check
  { source_id }` (parse + header validation without a node),
  `feature_edit` already covers args. An agent's authoring loop is: write
  the script, `script_run_check`, add the node, `feature_get` its errors.

### A9. Oracles and tests

1. **Gear parity.** The built-in `expand_gears` output for a matrix of
   `GearParams` must equal the `gear.rhai` script's sketch entities exactly
   (ids, positions, arc parameters). This is the API's acceptance test.
2. **Determinism.** Run every script fixture twice in one process and once
   in a fresh process; feature-tree JSON and tessellation hashes agree.
3. **Limits.** A `while true {}` script fails within its op budget with a
   typed `ScriptError::OperationLimit`; a `loop { extrude }` fails with
   `GeometryBudget`. Neither leaves a partial output.
4. **Query strictness.** A query resolving to two faces where one is
   required fails with both candidates named.
5. **Rollback/suppress/reorder.** GUI spec: a script node rolled back past
   hides its bodies; suppressing hides children; reordering moves them.
6. **Round trip.** v6 save → load → regenerate is byte-identical on the
   tree; an old v5 document loads unchanged.
7. **Tool/API parity.** The registered script functions and the migrated
   MCP tool list agree on operation names and parameter names (pinned).

### A10. Milestones

| # | Deliverable | Depends on |
|---|---|---|
| A-M0 | `Operation::Script` type, `SourceKind::Script`, v6 format, no interpreter (node errors "no interpreter") | — |
| A-M1 | Rhai interpreter with limits; API: `sketch`, `extrude`, `revolve`, `boolean`, `query.created_by/role/nth`; sub-tree execution; errors on node | A-M0 |
| A-M2 | `gear.rhai` passes gear parity; built-in Gear becomes a thin call into the script (or is kept but tested against it) | A-M1 |
| A-M3 | Query chain lowering to `TopoQuery`, named outputs, mate connectors from scripts | A-M1 |
| A-M4 | Script editor panel, `@param` dialog generation, MCP `script_*` tools, agent authoring loop documented | A-M2 |
| A-M5 | `sprocket.rhai` (needs Part B3's tooth form as sketch entities, or draws it from arcs directly) — **LANDED 2026-09-19**: draws the arcs directly, bit-identical to the generator (`tests/script_sprocket_parity.rs`) | A-M2 |
| A-M6 | Git-sourced script libraries, versioning rules | A-M4 |

## Part B — Built-in features, ranked

Each entry: what, why (from the bicycle), design, oracle. All are ordinary
`Operation` variants executed by `feature-engine` on the `Kernel` trait, and
all are exposed to Part A's API.

### B1. Circular and linear patterns (highest leverage)

**What.** `Operation::PatternCircular { seed: Vec<GeomRef> (bodies or
features), axis: AxisRef, count, angle_deg (full 360 default), skip:
Vec<usize> }` and `PatternLinear { seed, direction, count, spacing,
second_direction? }`.

**Why.** Spokes ×16 per wheel, cogs ×12, chainring bolts ×5, rotor bolts ×6,
handlebar tape wraps: ~60 of the bicycle's 212 calls.

**Design.** A pattern instances the seed's **bodies** as rigid copies
(placement by transform, no re-execution of the seed's operations) and then
optionally unions them into a target (`combine: NewBody | Add`). Rigid copy
is exact on analytic surfaces (a cylinder rotated is a cylinder), so the
kernel needs only a `transform_body` (already needed for assembly instance
export) plus the existing boolean. Copies carry provenance
`Role::PatternInstance { index, inner }` so a face on copy 7 is addressable.

**Oracle.** Volume of a pattern of N disjoint copies = N × seed volume
(exact-volume oracle in-line); a pattern of overlapping copies with `Add`
is one shell with the expected Euler characteristic; provenance resolves on
every copy.

### B2. Pipe sweep (analytic subset of sweep)

**What.** `Operation::Pipe { path: PathRef (a sketch's connected chain of
lines and arcs, or a body's edge chain), radius, inner_radius: Option<f64>,
combine }`.

**Why.** Handlebar (drops are arcs), brake hoses, saddle rails, chain,
frame tube bends, the fork legs' curve.

**Design.** A circle swept along a **line** is a cylinder; along a **planar
arc** it is a torus segment. The kernel already has both surfaces and their
SSI pairs (cylinder×torus lands in the M5 surface-pair family). The pipe
is built as a chain of cylinder/torus-segment bodies unioned end to end,
with the shared end caps coplanar by construction (a Stage 0 coplanar case:
it must go through the M8 coplanar path, or the union must be avoided by
building each segment's end as a **shared trimmed disc**, which is the
share-a-face cap already in the codebase). Tangent-continuous chains
(line→arc→line, the handlebar case) join with G1 continuity so no seam
edge is needed; non-tangent joins get a mitre (a planar cut). A hollow pipe
subtracts the inner pipe.

General sweep along a spline is out of scope: it needs a swept surface the
kernel does not represent. A spline path can be **approximated** by a
biarc chain in the sketch layer (a pure-2D step) before the pipe is built;
that is a sketch tool, not a kernel feature.

**Oracle.** Volume = π(r² − ri²) × path length for tangent chains (exact
for lines and arcs); the union has one shell; a pipe along a closed
rectangle-with-fillets loop is watertight with genus 1.

**Status (2026-09-21): checkpoint 1 LANDED — `specs/b2_pipe_sweep.md`.**
The join decision: NO union and NO shared cap — the pipe is ONE directly
assembled solid (`kernel_v2::pipe`, `PipePath`), consecutive laterals
sharing their rim circle as one edge, every seam on the path binormal
(the torus tessellators and yang Stage 1 now take a seam at any poloidal
phase). Solid and hollow (`inner_radius`, genus 1), any arc sweep, exact
`signed_volume = π(r² − rᵢ²)·L` (new torus-band flux term), typed refusals
(closed loop, non-tangent joint, bend ≤ tube radius). Boolean re-entry is
proven on line→arc→line, an S-bend (opposite senses, torus↔torus rim), a
hollow bend and a chained double cut; it needed four round-trip fixes
listed in the spec's §2.1. Known wall: a bend over ≈149° that survives a
boolean (the recovered seam must be one sub-π arc). **Checkpoint 2 LANDED the same day**: `Kernel::pipe`, `Operation::Pipe`
(`PipeParams`), `waffle_types::path::extract_open_chain`, `modeling_ops::execute_pipe`,
script `ctx.pipe`, `feature_add` authoring, FILE_FORMAT §7.11 (feature-engine PLAN.md
M15). **Checkpoint 3 LANDED too**: `PipeDialog.svelte` + toolbar "Pipe" (one
viewport click on an inactive-sketch line/arc selects its whole connected chain),
edit on double-click, `pipe-dialog.spec.js`. B2 is COMPLETE except the later
slices the spec lists (closed loops, mitres, non-planar chains).

### B3. Sprocket profile generator

**What.** `SketchEntity::Sprocket { params: SprocketParams { tooth_count,
pitch, roller_diameter, center, rotation_offset, standard: Iso606 |
AnsiB29_1 } }` expanded at rebuild like `Gear`.

**Why.** The bicycle's cassette and chainring are involute gears, which is
the wrong tooth form for a roller chain.

**Design.** ISO 606 / ANSI B29.1 tooth form, per tooth: pitch diameter
`D = p / sin(π/z)`, root diameter `D − d1`, a **seating curve** arc of
radius `ri` (between `0.505·d1` and `0.505·d1 + 0.069·d1^(1/3)`), tangent
**flank** arcs of radius `re` (between `0.008·d1·(z² + 180)` and
`0.12·d1·(z + 2)`), and a tip at diameter between `D + (1 − 1.6/z)·p − d1`
and `D + 1.25·p − d1`. The generator emits lines and arcs only (the existing
entity set), so extrude, pattern and boolean need nothing new. The exact
constants are to be taken from the standard text when implementing; the
ranges above are the standard's min/max and the generator picks the
mid-range by default with the two radii exposed as overrides.

**Oracle.** A roller of diameter `d1` centred on the pitch circle at every
tooth clears the profile by ≥ 0 and ≤ the seating clearance; tooth count
and pitch round-trip from the expanded geometry.

**Status (2026-09-19): LANDED** — `waffle_types::sprocket`, `SketchEntity::
Sprocket`, `GenerateSprocket{Preview,Profile}`, `sk.sprocket(...)`,
`createSprocket` in the app (display through the gear machinery), and since
checkpoint 2 the Sprocket dialog + placement tool (toolbar "Sprkt", key K,
chain presets, live preview, edit on double-click). ISO 606 only: the ANSI B29.1 form is a different construction whose
constants must come from the standard's text (not in `refs/`), so
`SprocketStandard` has one variant. Two corrections to the ranges above as
written: the flank range is `re ∈ [0.12·d1·(z+2), 0.008·d1·(z²+180)]` (min
and max were listed in the other order) and `0.069·∛d1` takes `d1` in mm.
Real-kernel oracles in `test-harness/tests/sprocket_kv2.rs`; the extrude is
exact (one cylindrical wall per arc, volume = analytic area × depth) after
a kernel-v2 arc-validator fix the sprocket's transversal flank/tip corners
exposed. A bore with COPLANAR caps still STOPs in the M8 Stage-0
mixed-loop path (PLAN.md M14 / Blockers); the general boolean path is fine.

### B4. Multi-body union as a first-class step

**Status (2026-09-23): LANDED — `specs/b4_balanced_union.md`.**
`Operation::UnionAll { targets: All | Selected }` folds every live body (or
the listed ones) by a balanced tree of ordinary pairwise unions gated by
`KernelIntrospect::solid_aabb` (a conservative box; disjoint pairs never
reach the kernel); the pattern's `Add`/`Intersect` fold shares the gate.
Progress frames per pairwise union reach the status bar (worker → bridge →
store) and the link (page `progress` frame → relay → MCP
`notifications/progress` when the client sent a `progressToken`). The
Boolean dialog gained "Union all bodies" and lists only LIVE bodies
(`ModelUpdated.consumed_features`); `feature_add` authors `UnionAll`;
scripts call `ctx.union_all()` / `ctx.union_all([bodies])`. On the way: the
2026-09-17 gearbox defect — a `BooleanCombine` or explicit combine target on
a CONSUMED body silently duplicated it — is now a loud refusal. Oracles:
`crates/test-harness/tests/union_all_kv2.rs` (exact inclusion–exclusion
volume, χ = 2, gate skip count, tree ≡ chain, bit-identical rebuilds).

**What.** `BooleanCombine` already exists; what is missing is a **many-body**
union that is fast and loud on a part with tens of overlapping `NewBody`
solids, and a UI/tool affordance "union all bodies of this part".

**Why.** The bicycle's parts are overlapping solids because each pairwise
union costs a full Yang pipeline run and the chained cost across 20 tubes
was prohibitive over the link. A part that is not one solid exports as
many solids in STEP and cannot be filleted, shelled or measured for mass
correctly later.

**Design.** `BooleanCombine { op: Union, targets: All }` executes as a
**balanced tree** of pairwise unions (log-depth instead of a chain), each
union going through the ordinary pipeline. Disjoint pairs short-circuit
through the AABB fast path (`specs/aabb_disjoint_boolean_fastpath.md`).
Progress frames per pairwise step reach the UI and the link
(`session_2026_09_17_planetary_gearbox_over_mcp` "no progress frames").

**Oracle.** Union volume ≤ Σ volumes, = Σ volumes minus overlaps computed
by the exact-volume oracle on random pairs; one shell; determinism across
orderings (a balanced tree must give the same solid as a chain: the assay's
20-op chained stacks are the reference).

### B5. Explicitly not in this plan

- Fillet, chamfer, shell: deferred indefinitely (CLAUDE.md).
- General sweep and loft: need surfaces the kernel does not carry.
- A visual scripting editor.

## Part C — Sequencing

1. **B1 patterns** first: smallest kernel surface (rigid copy + existing
   boolean), largest reduction in authoring effort, and Part A's API needs it.
2. **A-M0 → A-M2** in parallel with B1: the type, the interpreter, and gear
   parity. Gear parity is the gate for everything after.
3. **B3 sprocket** as a sketch entity (independent of the kernel), then
   **A-M5** expresses it as a script too.
4. **B2 pipe** once the coplanar end-cap join strategy is decided (share-a-
   face cap vs. M8 Stage 0).
5. **B4 balanced union** and progress frames alongside B2 (the pipe is its
   first heavy customer).
6. **A-M3 → A-M6**.

Every milestone lands with its oracle, a rebuilt WASM bundle in the same
commit, and the memory/PLAN updates the session guide requires.

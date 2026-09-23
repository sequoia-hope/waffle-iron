# Custom feature scripts

A custom feature is a small Rhai script the document carries as a `Script`
source. It appears in the feature list as ONE node that regenerates like a
built-in, and it calls the same operations the feature tree already runs.
Design: `specs/custom_features_and_modeling_roadmap.md` Part A. Format:
`docs/FILE_FORMAT.md` §5.5 (`SourceEntry.kind = Script`) and §7.10
(`ScriptParams`). This page is the user/agent reference: the language, the
API, the tools, and the authoring loop.

## 1. A script

```rhai
// @feature name="Box" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
// @output body: main

fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let regions = sk.finish().regions();
    ctx.extrude(regions[0], #{ depth: p.depth, combine: "NewBody" })
}
```

- The **header** is the leading run of `//` comment lines; the first code
  line ends it. `@feature name="…" version=N` is required. Each `@param
  name: type [= default] [min=…] [max=…]` declares an argument the node
  takes; each `@output name: kind` declares what the return value must
  provide (a contract — a missing or wrongly typed output fails the node).
- The **entry** function is `fn feature(ctx, p)` (`ScriptParams.entry`
  names another). `p` is a map of the resolved arguments; `ctx` is the
  engine.
- The **return value** names the node's outputs: a bare feature ref is the
  main body; a map `#{ main: boss, hub: ring, top: boss.faces().farthest_along([0,0,1]) }`
  gives the main body, a named body (`OutputKey::Named{"hub"}`) and a named
  face (`Role::Named{"top"}`) later features and mates reference by name.
- Every API call **records** a child operation; the engine executes the
  children in order after the script returns, so `sk.finish().regions()`
  is real (sketches are derived in Rust as the script runs) and every
  child sees the earlier ones. The private sub-tree is re-derived on
  every rebuild and never saved.
- Any failure — header, parse, an argument, a runtime error, `ctx.fail`, a
  sandbox limit, a child operation, a broken output contract — is a typed
  `Script` error on the node (`stage`: `header` | `parse` | `args` |
  `runtime` | `limit` | `fail` | `child`) and the node has **no outputs**;
  never stale geometry.

### Parameter types

| Type | Script sees | Argument (model units) | Expression-driven? |
|---|---|---|---|
| `int` | integer | a whole number | yes |
| `number` | float | a number | yes |
| `length` | meters | meters | yes (mm-space expression, converted) |
| `angle` | degrees | degrees | yes |
| `bool` | bool | `true`/`false` | no |
| `string` | string | a string | no |
| `plane` | a plane for `ctx.sketch` | `{origin, normal}` or a datum plane id | no |
| `body` / `face` / `edge` | a query over geometry OUTSIDE the script | a `GeomRef` of that kind, feature-anchored | no |

`min`/`max` bound numeric parameters. A `body` argument a child targets
(`combine: "Cut", targets: [p.target]`) is consumed by the node exactly as a
boolean would consume it. Arguments a script does not declare are refused;
declared ones without a default are required.

Units inside a script are meters and degrees; `mm(x)` and `inch(x)` convert.
`module` is a Rhai keyword — the gear script spells its parameter `module_m`.

### Sandbox

No I/O, no clock, no randomness, `eval` disabled; limits on interpreter
operations, call depth, array/map/string sizes, and a geometry budget of
2000 child operations. A runaway loop fails in milliseconds with `stage:
limit`.

## 2. The API

**Context**

| Call | Records | Notes |
|---|---|---|
| `ctx.sketch(plane)` → sketch builder | a Sketch | `plane`: a `plane` parameter, a face query's `.plane_of()`, or `plane(origin, normal)` |
| `ctx.extrude(region \| [regions], #{ depth, symmetric?, direction?, combine?, targets? })` → feature ref | an Extrude per region | `combine`: `"NewBody"` (default) \| `"Add"` \| `"Cut"` \| `"Intersect"`; `targets`: feature refs / body queries / `body` params |
| `ctx.revolve(region, #{ axis: #{ origin, direction }, angle_deg?, combine?, targets? })` | a Revolve | `angle_deg` default 360 |
| `ctx.pipe(sketch_ref, [entity_ids], #{ radius, inner_radius?, combine?, targets? })` | a Pipe | an open tangent chain of the sketch's lines/arcs |
| `ctx.boolean("union" \| "subtract" \| "intersect", a, b)` | a BooleanCombine | `a`, `b`: feature refs, queries or `body` params |
| `ctx.union_all()` / `ctx.union_all([bodies])` | a UnionAll | every live body (of the script) or the listed ones |
| `ctx.mate_connector(#{ name, on?, frame?, x_axis?, anchor?, flip_z?, rotation_deg?, offset_m? })` | a MateConnector | `on`: a face/edge query; exposed on the node under `name` |
| `ctx.log(msg)` | — | shown on the node as a warning `log: …` |
| `ctx.fail(msg)` | — | stops the script with `stage: fail` |
| `ctx.param(name)` | — | a resolved argument (same as `p.name`) |

**Sketch builder** (`sk`): `point(x, y)`, `line(a, b)`, `circle(center, r)`,
`arc(center, start, end)`, `spline([pts])`, `polygon([[x, y], …])`,
`rect(x, y, w, h)`, `gear(#{ … })`, `sprocket(#{ … })` — each returns the
entity id(s); a trailing `#{ construction: true }` map marks construction
geometry. `sk.finish()` returns a sketch ref with `.regions()` (closed
regions, each usable by extrude/revolve).

**Queries** are values resolved when consumed. From a feature ref `f`:
`f.faces()`, `f.edges()`, `f.bodies()` / `created_by(f)`, `.nth(i)` (the
i-th body of a multi-body feature), `.role(name, index)`, `.side_face(i)`;
a face query's `.plane_of()` is a plane for `ctx.sketch`. Filters and
tie-breaks chain into ONE `TopoQuery`: `.surface_type("planar" |
"cylindrical" | …)`, `.normal_near([x, y, z], tol_deg)`, `.near_point([x, y,
z], d)`, `.area_between(a, b)`, `.largest_area()`, `.nearest_to([x, y, z])`,
`.farthest_along([x, y, z])`, `.first()`. A query consumed where exactly one
entity is required that resolves to 0 or many fails loud with the count.

Two library scripts ship with the engine and are bit-identical to the
built-in generators: `gear` (`crates/feature-engine/scripts/gear.rhai`) and
`sprocket` (`scripts/sprocket.rhai`). They are the reference examples.

## 3. In the app

- **Toolbar → Script** opens the Script dialog. Choose a script source of
  the document, **New script…** (opens the editor on a template), or **Add
  built-in** gear / sprocket. The fields are generated from the header: a
  numeric parameter takes a measurement in the display unit or an
  expression over the design parameters (stored as `arg_exprs`, mm-space);
  a `plane` takes a datum plane or the selected planar face; `body` a body
  of the part; `face`/`edge` the current selection. Apply adds ONE node
  named by the script; double-click the node to edit its arguments; the
  property editor lists them.
- **Edit script…** (in the dialog) or **edit** on a Script row of the
  Sources panel opens the **editor**: a monospace textarea with line
  numbers, **Check** (Ctrl+Enter — the engine parses the header, compiles
  and confirms the entry function; the failing line is highlighted and
  clickable), **Save** (Ctrl+S) and **Save & close**. Saving an existing
  source replaces its text and regenerates every node using it; a node the
  new text breaks shows its error in the tree and in the editor's status
  line. Saving a new script adds a `Script` source to the document. Sources
  are assets, not edits: saving is **not an undo step** (the textarea keeps
  its own history while open), and a source stays in the document with no
  node using it.

## 4. Over the agent link (MCP)

The tools (`app/src/lib/agent/tools/scripts.js`, engine implementation
`crates/wasm-bridge/src/tools/script.rs`):

| Tool | Kind | Inputs | Result |
|---|---|---|---|
| `script_run_check` | query | `text` \| `source_id`, `entry?`, `args?` | `{ok, interface \| error{stage, reason}, dry_run?{ok, children[], logs[], outputs[], error?}}` |
| `script_source_add` | command (not an undo step) | `text` \| `library ("gear" \| "sprocket")`, `name?` | `{source_id, name, interface}`; `InvalidScript` when the script does not check |
| `script_source_get` | query | `source_id?` | one source: `{source_id, name, text, check, features[]}`; none: `{scripts[], library[]}` |
| `script_source_update` | command (not an undo step) | `source_id`, `text`, `on_error?` | `ModelDelta` + `{source_id, interface}`; a node the text newly breaks rolls the text back (`rolled_back`) unless `on_error: "keep"` |
| `script_feature_add` | command (one undo step) | `source_id`, `entry?`, `args?`, `arg_exprs?`, `on_error?` | `{feature_id}` + `ModelDelta`; the node takes the script's declared name |

`feature_get` returns a Script node's operation and its error; `feature_edit`
with a `Script` operation changes its arguments; `feature_add` with
`{"type":"Script","params":{…}}` is equivalent to `script_feature_add`.
Refusal codes: `InvalidScript{stage, reason}` (a script that does not check,
a malformed argument), `SourceNotFound` (an id that is not a Script source
of the document), and the usual `FeatureRebuildFailed{feature_id,
engine_error{kind: {type: "Script", stage}}, rolled_back}`.

### The authoring loop

1. Write the script text.
2. `script_run_check { text, args }` — with the arguments you intend to use
   the script is **dry-run** (no kernel): `dry_run.children` lists what it
   would record (`["sketch", "extrude"]`), `dry_run.logs` its `ctx.log`
   lines, `dry_run.outputs` the names its return value provides; a runtime
   error, a `ctx.fail`, a limit or a broken `@output` contract shows here
   with its stage. Fix and repeat until `ok` and `dry_run.ok`.
3. `script_source_add { text }` → `source_id`.
4. `script_feature_add { source_id, args }` → `feature_id`. A node that
   fails to build (a kernel error the dry run cannot see) is rolled back
   with the script's typed error; `on_error: "keep"` leaves it to inspect.
5. `feature_get { feature_id }` for the node's error; `model_summary` /
   `body_measure` for what it built.
6. `script_source_update { source_id, text }` to fix the script in place —
   every node using it regenerates; a breaking edit rolls back.

The same source can back many nodes with different arguments; changing the
source changes them all. Bump `@feature version` when the output topology
changes so documents pinning an older version keep regenerating identically
(A-M6 will make git-sourced libraries follow that rule).

## 5. Tests

- `crates/feature-engine/tests/script.rs` — the interpreter on MockKernel:
  every failure class typed with no output, limits, arguments, expressions,
  determinism, queries, named outputs, connectors, outer references.
- `crates/feature-engine/tests/script_gear_parity.rs`,
  `script_sprocket_parity.rs` — the library scripts bit-identical to the
  built-in generators.
- `crates/test-harness/tests/script_kv2.rs` — real-kernel exact-volume
  oracles.
- `crates/wasm-bridge/tests/tool_script.rs` — the five tools and the
  editor's engine messages (refusal codes, deltas, rollback of a breaking
  update, the document round trip).
- `app/tests/gui/script-dialog.spec.js` — the dialog and the editor in the
  page; `app/tests/gui/agent-script-tools.spec.js` — the authoring loop
  through the page's executor against the real engine.

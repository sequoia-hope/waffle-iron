# 06 — Feature Engine: Architecture

`crates/feature-engine` is the parametric modeling engine. It owns the feature
tree, rebuilds geometry from it, resolves persistent geometry references,
evaluates design parameters, and provides undo/redo. It also holds the
document-level models built on top of parts: assemblies, drawings, custom
feature scripts and linked KiCad boards.

The engine never talks to a kernel type directly. It executes operations
through `modeling-ops` on a `KernelBundle` (the `Kernel` + `KernelIntrospect`
traits from `waffle-types`).

## Feature tree

`FeatureTree` (`types.rs`, `tree.rs`) is an ordered list of `Feature`s plus
the part's parameters, body names, entity names, materials and provenance
table. A `Feature` is an id, a name, a suppressed flag, an `Operation` and
the references it holds.
`active_index` is the rollback point: features after it are not built.

The tree stores the recipe, not geometry. Geometry is rebuilt from it.

`Operation` variants: `Sketch`, `Sketch3d`, `Extrude`, `Revolve`, `Pipe`,
`Sweep`, `BooleanCombine`, `UnionAll`, `DatumPlane`, `MateConnector`,
`PatternCircular`, `PatternLinear`, `PatternMirror`, `ImportedBody`, `Script`,
`BoardShape`, and `Fillet`, `Chamfer`, `Shell` (deferred — see root
`CLAUDE.md`). An operation this build does not know is kept as `Unknown` and
written back unchanged.

## Rebuild

`Engine::rebuild` (`lib.rs`) drives `rebuild::rebuild` (`rebuild.rs`,
`rebuild/`). It replays features in tree order: resolve the operation's
references, evaluate its expressions, execute it through `modeling-ops`, and
store the `OpResult` in `feature_results`.

- **Incremental.** A rebuild is given a start index and a `Changed` set. A
  feature re-executes only if it changed, names a feature that re-executed, or
  finds an input by tree position after one. Every other feature keeps its
  last result. `Changed::All` re-executes everything from the start index.
- **Errors are per feature.** A feature that fails records a typed
  `FeatureError` and the rebuild goes on to the next feature. Warnings are
  recorded per feature as well.
- **Consumption.** A boolean, or an extrude that adds to or cuts a body,
  consumes its targets. `consumed_features` lists features whose bodies are no
  longer rendered; a consumed body is never picked again as a combine target.
- **Combine targets** (`rebuild/combine.rs`). An extrude or revolve combines
  as `NewBody`, `Add`, `Cut` or `Intersect` against explicit target
  references, the bodies sharing a face with the profile, or the most recent
  solid.
- **Measurement passes.** An expression may measure the model
  (`distance(a, b) / 2`). Such a rebuild runs build → measure → build again
  from the affected feature. A measurement may read only geometry earlier in
  the tree than the expression it drives, so the passes terminate; exceeding
  the budget is a typed error.
- **Progress.** Long rebuilds emit progress frames (`progress.rs`).

Tessellation is not the engine's job; the bridge tessellates bodies that have
no mesh after a rebuild.

## Persistent references

Kernel ids change on every rebuild. Anything that refers to geometry stores a
`GeomRef` (`waffle-types`): an `Anchor`, a `Selector`, a `ResolvePolicy`, and
the kind of entity wanted.

**Anchor** — where to look: `FeatureOutput { feature_id, output_key }` (output
keys are `Main`, `Body { index }`, `Profile { index }`, `Datum { name }`) or
`Datum { datum_id }`.

**Selector** — which entity:

| Selector | Meaning |
|---|---|
| `Pid { pid, root_pid }` | The kernel's persistent id. Preferred for anything that must stay on one entity. If `pid` is gone, `root_pid` finds the entity its geometry descended to. Never rebinds by geometry. |
| `Role { role, index }` | The semantic role the operation assigned (end cap, side face, pattern instance, …). |
| `Signature { signature }` | Best match on geometric fingerprint (surface type, area, centroid, normal, bounding box, adjacency). |
| `Query { query }` | Filters plus a tie-break ("largest planar face"). |
| `Position { x, y, z }` | Nearest entity to a point. |

**Policy** — `Strict` refuses when the reference does not identify exactly one
entity. `BestEffort` rebinds to the closest match, records how
(`ResolvedVia`), and emits a warning naming what it bound.

Resolution is in `resolve.rs`. `resolve_with_fallback` tries the selector and,
for a failed role, falls back to matching among entities of the same kind.
`pin_identity` runs at authoring time: it resolves the authored reference
strictly, stores a `Pid` selector as the target, and keeps the authored
reference as the fallback. `resolve_pinned` uses the pid first and the
fallback only if the policy allows. Every refusal is a typed
`ReferenceRefusal` with a reason (ambiguous, missing, scope missing, …).

`Engine::created_by_feature` maps a face to the feature that introduced its
geometry, through chained booleans, using the kernel's face provenance.

`names.rs` stores user- and agent-assigned names for entities; a name is a
pinned reference.

## Parameters and expressions

`params.rs` evaluates design parameters and applies expressions to operation
fields. `expr/` is the expression language: a lexer, a recursive-descent
parser, and an evaluator that tracks dimensions and unit literals. Parameters
have part scope and document scope. Measurement functions are in
`expr/measure.rs` and `measure.rs`. Spec: `specs/parameterized_designs.md`.

## Undo/redo

`undo.rs` is a command stack. Commands: `AddFeature`, `RemoveFeature`,
`EditFeature`, `ReorderFeature`, `SuppressFeature`, `RenameFeature`,
`RenameBody`, `SetEntityName`, `SetMaterials`, `SetParameters`. Each records
what it needs to invert itself, including a feature's provenance record. A new
command clears the redo stack. Undo and redo rebuild from the affected
feature. The bridge keeps one history per document tab
(`take_history` / `set_history`).

## Other modules

| Module | Contents |
|---|---|
| `pattern.rs` | Circular, linear and mirror patterns |
| `union_all.rs` | Many-body union as one feature |
| `script/` | Custom feature scripts in Rhai: sandboxed interpreter, the recorded host API, interface headers, the built-in library. Docs: `docs/CUSTOM_FEATURE_SCRIPTS.md` |
| `sketch3d.rs` | 3D sketch evaluation (paths for pipe and sweep) |
| `share_a_face.rs` | Predicates for the extrude auto-target default |
| `assembly.rs`, `assembly_solver.rs`, `connector.rs`, `context.rs` | Assemblies, the mate solver, mate-connector frames, in-context editing (see `projects/10-assemblies`) |
| `drawing.rs`, `drawing/` | Drawing tabs: views, dimensions, annotations |
| `kicad.rs`, `board_shape.rs` | Bodies derived from a linked `.kicad_pcb`; board outline authored on a face |
| `import_tiers.rs`, `sources.rs` | STEP import tiers; document-level source content |
| `preview_mesh.rs` | Decimated preview mesh for document thumbnails |
| `opaque.rs` | Round-trips tagged objects this build does not know |

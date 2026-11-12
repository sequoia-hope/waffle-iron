# 10 — Assemblies: Architecture

An Assembly is a tab kind. It places instances of parts by rigid transforms
and relates them with mates between mate connectors. There is no separate
assembly crate: the model and solver are in `crates/feature-engine`, the
evaluation in `crates/wasm-bridge`, and the UI in the app.

Specs: `specs/waffle_v4_document_model.md` (§9 Phase 3, §2.8),
`specs/assembly_connector_frame_resolver.md`,
`specs/assembly_connector_adjustments.md`, `specs/part_mate_connectors.md`.
File format: `docs/FILE_FORMAT.md` §5.6.

## Model (`feature-engine/src/assembly.rs`)

`AssemblyTree` holds `instances`, `connectors`, `mates` and `placements`.

- **`Instance`** — one placed occurrence of a part: a `PartRef` (a tab of
  this document, or a tab of a linked `.waffle` source), an explicit
  `Transform` (translation in meters plus a unit quaternion), `fixed`,
  `suppressed`, and optional `parameter_overrides`. The source tab may be a
  Part or another Assembly (a sub-assembly). An instance with overrides is
  built as its own solid.
- **`MateConnector`** — a coordinate frame on an instance. `instance_path` is
  the chain of instance ids down to the owning instance (longer than one for
  a member of a sub-assembly). The frame comes from, in order of precedence:
  a named connector of the part (`part_connector`), geometry (`geom_ref`), or
  an explicit `frame`. Four adjustments then apply: `anchor` (where along a
  rotational face's axis: middle or either end), `flip_z`, `rotation_deg`
  about z, and `offset_m` along the frame's own axes.
- **`Mate`** — a relation between two connectors. `MateKind`: `Fastened`,
  `Revolute`, `Slider`, `Cylindrical`, `Planar`, `Ball`. An unknown kind is
  preserved and reported, never guessed.
- **`placements`** — the solved transform of each instance. These are derived
  hints, recomputed on every evaluation and saved so a reader without the
  engine can position instances. The engine is authoritative.

A grounded (`fixed`) instance never moves. If none is marked, the first
non-suppressed instance is grounded.

## Connector frames (`feature-engine/src/connector.rs`)

`resolve_connector_frame` derives a frame from picked geometry:

| Pick | Frame |
|---|---|
| Planar face | Centroid, z along the outward normal |
| Cylindrical, conical or toroidal face | On the axis, at the anchor along that face's axial extent |
| Spherical face | The center |
| Circular or elliptical edge | The center, z out of its planar neighbor face |
| Straight edge | The midpoint, z along the edge |

Anything else is a typed error. Axes come from the kernel through
`KernelIntrospect::entity_axis`; a mesh-backed imported body has none. A pick
is judged when the connector is created (`ProbeConnectorRef`), so a connector
is never minted on geometry that has no frame.

A part can define named connectors itself with `Operation::MateConnector`.
Every instance of that part offers them.

## Solving (`assembly.rs`, `assembly_solver.rs`)

`solve_mates` runs two passes:

1. **Exact.** `solve_fastened` places chains of `Fastened` mates by
   rigid-transform composition from the grounded instances.
2. **Numeric.** The remaining mate kinds are solved by damped Gauss-Newton
   over six pose increments per free instance, starting from the instances'
   current poses. The damping is `λI` (minimum-norm steps), so a free degree
   of freedom stays where it was.

A mate that cannot be satisfied, an over-constrained pair, a missing
connector or an unusable frame is reported in `SolveResult.errors`. An
instance no mate reaches keeps its explicit transform and is reported in
`warnings`.

## Evaluation (`wasm-bridge/src/assembly_view.rs`)

`OpenAssembly` builds one engine per distinct part build, derives the
connector frames, solves, and recurses into sub-assemblies (cycle and depth
guards report errors). The result is flattened to leaves, each with its
instance path and composed world placement. `ModelUpdated.assembly` carries
placements, connectors in world coordinates, errors and warnings. Each body's
metadata carries its `instancePath` and `transform`; the viewport applies the
transform.

## In-context editing (`feature-engine/src/context.rs`)

A part can be opened in the context of an assembly (`OpenPartInContext`). The
other instances appear as ghosts in the edited part's frame, and the part may
reference their geometry. Such a reference is a `GeomRef` with a `scope`
(`RefScope`: tab, instance path). A scoped reference resolves only through
the `EditContext`; without a context it is a warning or a per-feature error,
never a guess against the local part. References into a linked document's
assembly are refused.

## UI and tools

- `app/src/lib/ui/AssemblyPanel.svelte` — the left panel of an Assembly tab:
  instances (part, fixed, hide, translation, rotation, edit in context),
  connectors and their adjustments, mates, evaluation errors.
- `app/src/lib/viewport/ConnectorFrames.svelte` — a triad per connector.
- `app/src/lib/ui/MateConnectorDialog.svelte` — part connectors.
- `app/src/lib/ui/ContextBanner.svelte` — update or exit an edit context.
- Agent tools (`wasm-bridge/src/tools/assembly.rs`): `assembly_get`,
  `instance_add/edit/delete`, `connector_add/edit/delete`,
  `mate_add/edit/delete`.

## STEP export

With an assembly open, `ExportStep` writes every rendered leaf's bodies at
their solved world placements as one flat multi-body file. Mesh-backed
imported bodies are listed in the export's warnings and not written.

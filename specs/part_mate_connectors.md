# Part mate connectors

Status: LANDED 2026-09-14. Follows `specs/assembly_connector_frame_resolver.md`
and `specs/assembly_connector_adjustments.md`.

## 1. Problem

A mate connector lived only in an assembly. Every assembly that placed a
part had to find the same face again, on every instance, and the part's
author had no way to say "this bore is where the pin goes". Onshape solves
this with mate connectors authored in the Part Studio. Waffle Iron had no
equivalent.

## 2. Model

### 2.1 The part side: a feature

`Operation::MateConnector { params: MateConnectorParams }`:

| field | meaning |
|---|---|
| `name` | the feature's name at creation (default "Mate connector"). After that, the FEATURE's name is the connector's name, so renaming the feature renames the connector. |
| `geom_ref?` | a face or an edge of the part; the frame is derived from it by `connector::resolve_connector_frame`, exactly as for an assembly connector |
| `frame` | the frame when there is no `geom_ref` (part coordinates, meters); with one, a non-zero `x_axis` is the secondary direction |
| `anchor?`, `flip_z?`, `rotation_deg?`, `offset_m?` | the adjustments of `specs/assembly_connector_adjustments.md`, same order, same semantics |

It is a feature because a feature already brings everything a connector
needs: a place in history (rollback, suppress, reorder), undo, rename,
save/load, `feature_add`/`feature_edit` for agents, and loud per-feature
errors.

Adding an operation kind needs no reader-floor bump (v4 §2.5). An older
reader keeps the feature as `Operation::Unknown` and fails it loudly.

**Rebuild.** The feature produces no geometry. Its rebuild derives the frame
(`connector::part_connector_frame`) and fails the feature with a typed
`ResolutionFailed` when the frame cannot be derived: a vertex, a freeform
face, or a degenerate explicit z. It never substitutes a default frame.
After every rebuild, `Engine::connectors` holds each built connector's frame
with the adjustments applied. The rebuild's "most recent solid" walks skip
the feature, just as they skip sketches and datum planes.

**Report.** `ModelUpdated.connectors` holds `PartConnectorInfo
{feature_id, name, kind?, origin, x_axis, y_axis, z_axis}` in part
coordinates. The viewport draws them as triads (`ConnectorFrames.svelte`).

### 2.2 The assembly side: a reference

`assembly::MateConnector.part_connector?: Uuid` names the part's
`MateConnector` feature. When it is set, the frame is that connector's frame
as the part evaluated it. It takes precedence over `geom_ref` and `frame`.
The assembly connector's own adjustments still apply on top, followed by the
member placement.

The field is additive and serde-defaulted, so it needs no reader-floor bump.
An older reader would carry the key opaquely and fall back to the connector's
explicit frame. The v5 floor already excludes readers that predate
connectors, and the fallback is the same one an unresolvable `geom_ref` gets.

A reference to a connector the part does not have (deleted, suppressed,
rolled back, or failed) is a loud evaluation error. The connector then falls
back to its explicit frame, as an unresolvable `geom_ref` does.

`AssemblyStatus.part_connectors` lists every rendered leaf's part connectors
in world coordinates, each tagged with its `instance_path`. It is what the
panel offers.

## 3. UI

- **Part:** the toolbar's `Connector` button opens `MateConnectorDialog`.
  The dialog places the connector on the selected face or edge, or at the
  part origin, and has the adjustment fields. Double-clicking the feature
  (`showEditFeatureDialog`) edits it. A failed pick leaves the feature red
  with a toast, and the dialog stays open on that feature.
- **Assembly panel:** unused part connectors are listed under "Mate
  connectors" with a `use` button. The mate A/B pickers offer them directly
  in a "Part connectors" group, and picking one there creates the assembly
  connector as part of the mate.
- **Agents (MCP):** `feature_add`/`feature_edit` accept `MateConnector`, and
  `model_summary` returns `connectors`
  (`{feature_id, name, kind, origin_m, z_axis, x_axis}`).

## 4. Tests

- `feature-engine/tests/connector_tests.rs`: derivation plus adjustments,
  the flipped anchor, the explicit frame, loud refusals, the additive wire
  form.
- `wasm-bridge/tests/assembly_tests.rs`: with the real kernel, a `Top`
  connector on the imported cube is reported by the part, offered on both
  instances, and stacks B on A through `part_connector`; a dangling
  reference is loud; a vertex pick fails the feature.
- `app/tests/gui/part-connectors.spec.js`: the dialog on a clicked face, the
  triad report, and the assembly panel's part connector into a mate.

## 5. Not yet

Part connectors cannot be vertex connectors, have no secondary-axis pick,
are not dragged in the viewport, and are not shown inside an in-context
edit's ghosts.

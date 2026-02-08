# 06 — Feature Engine: Architecture

## Purpose

THE MOST CRITICAL SUB-PROJECT. The parametric modeling brain of Waffle Iron. Manages the feature tree, implements persistent naming via the GeomRef system, executes the rebuild algorithm, and provides undo/redo.

## Feature Tree

An ordered list of `Feature`s. Each Feature contains:
- An `Operation` (with parameters) — what to do.
- `GeomRef` references — to what geometry.
- Metadata (name, suppressed flag, UUID).

The `active_index` controls rollback: features after this index are suppressed during rebuild, allowing users to "roll back" the model to an earlier state.

### Feature Operations

Operations modify the model by calling modeling-ops, which calls the kernel. The feature tree stores the recipe (parameters + references), not the geometry. Geometry is recomputed on every rebuild.

This is the correct design for parametric CAD (matches Onshape's approach): the file format stores the recipe, and the model is rebuilt from it on load.

## Rebuild Algorithm

When a parameter changes (e.g., user edits a sketch dimension or changes an extrude depth):

1. **Identify the earliest affected feature.** If feature #3 changed, everything from #3 onward is dirty.
2. **Replay features from the change point forward.** For each non-suppressed feature from the change point:
   a. Resolve all GeomRefs in the operation's parameters (find the current KernelIds for referenced geometry).
   b. Call the appropriate modeling-ops function (extrude, revolve, fillet, etc.) with the resolved parameters and the current model state.
   c. Receive OpResult (outputs + provenance).
   d. Store the OpResult for this feature (used by downstream features for GeomRef resolution).
3. **If any GeomRef fails to resolve:**
   - With `ResolvePolicy::Strict`: fail the rebuild, mark the feature as errored.
   - With `ResolvePolicy::BestEffort`: use closest match, emit a warning, continue.
4. **After rebuild:** tessellate the final solid(s), send updated meshes to the UI.

### Performance Considerations

Each feature replay involves a kernel operation + tessellation. For a 50-feature tree, this could mean 50 kernel operations. With truck's boolean performance issues (seconds per boolean), this could be slow.

Mitigation strategies:
- **Lazy tessellation:** Only tessellate the final state, not intermediate states.
- **Incremental rebuild:** Only replay from the change point, not from scratch.
- **Operation caching:** Cache OpResults for unchanged features (skip re-execution if parameters and input geometry haven't changed).
- **Background rebuild:** Run rebuild in the Web Worker, don't block the UI.

## Persistent Naming Strategy

This is the core algorithm that makes parametric CAD work. When a fillet references "the top edge of Extrude 1," the system must find that edge even after the user changes the sketch that drives Extrude 1 (which may change the topology).

### The Problem

After a rebuild, kernel-internal IDs (KernelId) may change:
- Booleans create new objects with new IDs.
- Topology changes (adding/removing faces/edges) shift IDs.
- truck provides NO old→new mapping.

### The Solution: GeomRef

Every geometry reference uses `GeomRef = Anchor + Selector + ResolvePolicy`:

**Anchor:** Which feature's output to look in.
- `FeatureOutput { feature_id, output_key }` — look in the output of a specific feature.
- `Datum { datum_id }` — look at a datum plane/axis.

**Selector:** How to find the specific entity.

Three strategies, tried in order:

1. **Role-based selection** (fast, stable):
   - Each modeling operation assigns semantic roles to created geometry.
   - Example: an extrude creates `EndCapPositive`, `EndCapNegative`, and `SideFace` roles.
   - To reference "the top face of Extrude 1": `GeomRef { anchor: Extrude1/Main, selector: Role(EndCapPositive, 0) }`.
   - Roles are stable as long as the operation type and parameter structure don't change.
   - Resolution: look up role assignments in the feature's current OpResult.

2. **Signature-based selection** (robust fallback):
   - When role-based resolution fails (topology changed, roles shifted), fall back to matching by geometric properties.
   - `TopoSignature` includes: surface type, area, centroid, normal, bbox, adjacency hash.
   - Resolution: compute signatures for all entities in the anchor's output, find the best match to the stored signature.
   - Uses weighted scoring across multiple signature fields for robustness.

3. **Query-based selection** (user-specified):
   - For advanced selections: "the face with the largest area," "the face nearest point (x,y,z)."
   - `TopoQuery` with `Filter`s and `TieBreak` strategy.
   - Used when role and signature are insufficient.

**ResolvePolicy:** What to do on ambiguity/failure.
- `Strict`: fail the rebuild. Used for critical references.
- `BestEffort`: use closest match + emit warning. Used for interactive modeling.

### Resolution Algorithm (Detailed)

```
resolve(geom_ref: &GeomRef, feature_results: &FeatureResults) -> Result<KernelId, ResolveError>:
    1. Find the anchor feature's current OpResult.
       - If anchor feature doesn't exist → error.
       - If anchor feature is suppressed → error.
    2. Get the output identified by output_key.
    3. Apply selector:
       a. Role { role, index }:
          - Look up role_assignments in the OpResult's provenance.
          - Find all entities with the matching role.
          - Return the one at the specified index.
          - If role not found → fall through to Signature.
       b. Signature { signature }:
          - Get all entities of the target kind in the output.
          - Compute similarity score between each entity's current signature and the stored signature.
          - Return the best match above a threshold.
          - If no match above threshold → error or BestEffort.
       c. Query { query }:
          - Get all entities of the target kind in the output.
          - Apply filters to narrow candidates.
          - Apply tie_break to select one.
    4. Apply policy:
       - Strict: return error if no exact match.
       - BestEffort: return closest match + warning.
```

### Known Limitations

This is a simplified version of commercial persistent naming. Cases that may break:
- **Massive topology changes** where the entire solid is restructured.
- **Operations that completely reshape** the solid (e.g., boolean that removes most faces).
- **Multiple similar faces** (e.g., all side faces of a cylinder have the same signature).
- **Pattern instances** where multiple copies of the same geometry exist.

For the majority of practical modifications to a linear feature tree, this system works.

## Undo/Redo

Command pattern on feature tree mutations:

| Action | Undo |
|--------|------|
| AddFeature | RemoveFeature |
| RemoveFeature | AddFeature (at original position) |
| EditFeature | EditFeature (restore old parameters) |
| ReorderFeature | ReorderFeature (restore old position) |
| SuppressFeature | UnsuppressFeature |

Each command is invertible. Undo stack + redo stack. Redo stack is cleared when a new command is executed.

After each undo/redo, rebuild from the earliest affected feature.

## Parameter Propagation

When a sketch dimension changes:
1. The Sketch feature's Operation is updated with new constraint values.
2. The sketch is re-solved (via sketch-solver).
3. The Sketch feature is marked dirty.
4. Rebuild from the Sketch feature forward.
5. All downstream features (extrude referencing this sketch's profile, fillet referencing an edge from the extrude, etc.) are replayed.
6. All GeomRefs in downstream features are re-resolved.

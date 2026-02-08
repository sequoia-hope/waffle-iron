# Persistent Naming in Waffle Iron

## What Is Persistent Naming?

In parametric CAD, features reference specific geometry: "fillet this edge," "extrude from this face," "measure this distance." When the user changes an earlier feature (e.g., modifies a sketch dimension), the model rebuilds from that point forward. Geometry changes — faces may move, edges may split, entities may appear or disappear.

**The persistent naming problem:** How do downstream features find the "same" geometry after a rebuild?

### Concrete Example

1. User creates a box (Extrude 1).
2. User applies a fillet to the top-front edge (Fillet 1). Fillet 1 stores: "the top-front edge of Extrude 1."
3. User goes back and changes the sketch that drives Extrude 1 (makes the box wider).
4. Extrude 1 rebuilds — it's now wider, all faces/edges have potentially new kernel IDs.
5. **Fillet 1 must find the "top-front edge" again** in the rebuilt Extrude 1.

If the system uses raw kernel IDs, the reference breaks (IDs changed). Persistent naming solves this.

## How Commercial Systems Solve It

### Parasolid (Onshape, SolidWorks, NX)
- **Operation journals** record every topological modification.
- Each entity gets a persistent "tag" that survives modifications.
- Tags track lineage: "this face was created by sweeping edge X, then trimmed by boolean Y."
- Extremely robust but deeply integrated into the kernel.

### OpenCascade (OCCT) — TNaming / OCAF
- **TNaming** provides naming services on top of OCCT's shape history.
- Shapes are tracked through "evolution" labels: GENERATED, MODIFIED, DELETED, SELECTED.
- **OCAF** (Application Framework) provides undo/redo and persistence built on TNaming.
- Evolution tracking is opt-in and must be explicitly recorded during operations.

### FreeCAD — Realthunder's Topological Naming Fix
- Encodes shape history as string-based "mapped names."
- Example: `Edge1;:G;SKT;:H1:7,F;:H5:7,F;FWD;:H2:7,E`
- Each segment records a step in the entity's creation history.
- Sub-element mappings propagate through operations.
- Complex and fragile but works for many practical cases.

## truck's Current State

truck provides NO persistent naming infrastructure:

- **Runtime IDs only.** `VertexID`, `EdgeID`, `FaceID` are derived from `Arc`-based reference counting (`rclite::Arc`). Two handles to the same entity share the same ID.
- **IDs are stable within a session** but NOT across topological modifications.
- **Booleans create entirely new objects** with new IDs. There is no mapping from input entity IDs to output entity IDs.
- **Solid has no ID at all.**
- **No operation journals.** No entity tracking. No history.

This means Waffle Iron must build its own persistent naming system.

## Our Approach: GeomRef

### Overview

Every geometry reference uses `GeomRef`, which encodes:
- **WHERE to look** (Anchor — which feature's output)
- **WHAT to find** (Selector — role, signature, or query)
- **WHAT TO DO on failure** (ResolvePolicy — strict or best-effort)

### Anchor: Where to Look

```
Anchor::FeatureOutput { feature_id: UUID, output_key: OutputKey }
```

"Look in the output of Extrude 1, in its Main body."

### Selector: What to Find

Three strategies, tried in order:

**1. Role-Based (Primary)**

Each modeling operation assigns semantic roles to the entities it creates:
- Extrude creates: `EndCapPositive`, `EndCapNegative`, `SideFace{0}`, `SideFace{1}`, ...
- Fillet creates: `FilletFace{0}`, `FilletFace{1}`, ...
- Boolean creates: `BooleanBodyAFace{0}`, `BooleanBodyBFace{0}`, ...

To reference "the top face of Extrude 1": `Selector::Role { role: EndCapPositive, index: 0 }`.

Roles are assigned by modeling-ops during execution (based on operation semantics, not geometry). They are stable as long as the operation type and structure don't change.

**2. Signature-Based (Fallback)**

When roles fail (topology changed — an extrude that used to produce 6 faces now produces 8), fall back to matching by geometric properties:

```
TopoSignature {
    surface_type: Some("planar"),
    area: Some(500.0),
    centroid: Some([50.0, 25.0, 100.0]),
    normal: Some([0.0, 0.0, 1.0]),
    bbox: Some([0.0, 0.0, 100.0, 100.0, 50.0, 100.0]),
    adjacency_hash: Some(0xABCD1234),
}
```

Resolution: compute signatures for all entities in the output, compare to the stored signature, pick the best match (weighted similarity score).

**3. Query-Based (User-Specified)**

For explicit selections: "the face with the largest area," "the face nearest to (50, 25, 100)."

### Resolution Algorithm

```
1. Find anchor feature → get its OpResult
2. Get the output body identified by output_key
3. Try Role selector:
   - Look up role assignments in OpResult.provenance
   - Find entities with matching role
   - Return entity at specified index
   - If role not found → continue to step 4
4. Try Signature selector:
   - Compute current signatures for all entities of target kind
   - Score similarity against stored signature
   - Return best match above threshold
   - If no match → step 5
5. Apply policy:
   - Strict → error (rebuild fails, feature marked broken)
   - BestEffort → return closest match + warning
```

### Provenance: The Foundation

Persistent naming depends on modeling-ops producing complete provenance for every operation:

```
Provenance {
    created: [EntityRecord(id=5, kind=Face, sig={...}), ...],
    deleted: [EntityRecord(id=2, kind=Edge, sig={...}), ...],
    modified: [Rewrite(before=3, after=7, reason=Trimmed), ...],
    role_assignments: [(id=5, EndCapPositive), (id=6, EndCapNegative), ...],
}
```

Without complete provenance, GeomRef resolution cannot work.

## Known Limitations

### What Works
- **Linear feature trees** with standard operations (sketch → extrude → fillet → chamfer).
- **Parameter changes** that don't alter topology structure (changing depth, radius, dimension).
- **Minor topology changes** where role assignments remain valid.
- **Moderate topology changes** where signature matching finds the right entity.

### What May Break
- **Massive topology changes** where the solid is completely restructured (e.g., a boolean that merges two complex bodies).
- **Multiple similar entities** (e.g., all side faces of a regular polygon have similar signatures — adjacency_hash helps disambiguate but isn't foolproof).
- **Split entities** (an edge splits into two — which one is "the" edge?).
- **Vanishing entities** (a face that no longer exists after a change — no amount of matching can find it).
- **Pattern instances** where N copies of identical geometry exist.

### Mitigation
- **BestEffort policy** with user notification when uncertain matches are made.
- **Adjacency hash** in signatures provides topological context beyond pure geometry.
- **User intervention** for truly broken references (select new entity, update reference).

## Test Scenarios

### Scenario 1: Basic Stability
1. Create box (Extrude 1, sketch = 100x50 rectangle).
2. Add fillet to top-front edge (Fillet 1, references edge via Role::SideFace boundary).
3. Change sketch width to 120.
4. Rebuild. **Expected:** Fillet 1 finds the "same" edge (role-based resolution succeeds).

### Scenario 2: Role Fallback to Signature
1. Create box. Add fillet to one edge.
2. Modify the sketch such that the extrude now produces additional geometry (e.g., a cutout that adds edges).
3. Role assignment may shift (new SideFaces with different indices).
4. **Expected:** Role fails, signature matching finds the correct edge by geometric properties.

### Scenario 3: Reference Survives Feature Insertion
1. Create sketch → extrude → fillet.
2. Insert a chamfer between the extrude and fillet.
3. **Expected:** Fillet's GeomRef still resolves (anchor is Extrude 1, which hasn't changed).

### Scenario 4: Reference Breaks Gracefully
1. Create sketch → extrude → fillet on edge.
2. Delete the extrude.
3. **Expected:** Fillet's GeomRef fails (anchor feature doesn't exist). Feature marked as errored. Clear error message.

### Scenario 5: Over-Constrained Matching
1. Create a regular hexagonal prism (6 identical side faces).
2. Fillet one edge.
3. Change the sketch.
4. **Expected:** Signature matching may be ambiguous (6 similar faces). Adjacency hash should disambiguate. If not, BestEffort picks closest + warns.

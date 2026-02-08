# 09 — File Format: Architecture

## Purpose

Save, load, and export Waffle Iron projects. The native format stores the parametric recipe (feature tree), not geometry. The model rebuilds from the recipe on load. STEP export produces standard geometry files for interchange.

## Native Format: JSON Feature Tree

The native file format is a JSON serialization of the `FeatureTree`. It stores:
- All features in order (UUID, name, operation with parameters, suppressed flag).
- All GeomRefs (anchored to features by UUID, using selectors for specific entities).
- All sketch data (entities, constraints, solved positions are NOT stored — sketches re-solve on load).
- Project metadata (name, version, creation date, last modified).

### What Is Stored

- Feature list with all Operation parameters.
- GeomRef references between features.
- Sketch entity and constraint definitions.
- Project metadata.

### What Is NOT Stored

- Kernel geometry (BREP solids, faces, edges). Rebuilt from recipe.
- KernelSolidHandles or KernelIds (runtime-only).
- Tessellated meshes. Recomputed on load.
- Solved sketch positions. Re-solved on load.

### Why This Design?

This matches Onshape's approach and is the correct design for parametric CAD:
1. **Files are small.** A 50-feature model is kilobytes of JSON, not megabytes of BREP.
2. **Full parametric history.** Every parameter is editable after load. Changing a sketch dimension rebuilds the entire model.
3. **Format independence.** Not tied to any specific kernel version. If truck's BREP representation changes, old files still load (the recipe is kernel-agnostic).
4. **Deterministic.** Same recipe always produces the same geometry.

## File Format Versioning

```json
{
  "format": "waffle-iron",
  "version": 1,
  "project": {
    "name": "My Part",
    "created": "2025-01-15T10:30:00Z",
    "modified": "2025-01-15T14:22:00Z"
  },
  "features": [...]
}
```

- `version` is incremented when the format changes.
- Loaders handle older versions via migration functions.
- Forward compatibility: unknown fields are preserved (serde `#[serde(flatten)]` with `HashMap<String, Value>`).

## STEP Export

1. Rebuild the model from the feature tree (full rebuild to get final solid).
2. Export the final solid via truck's ruststep (AP203).
3. Known limitations:
   - AP203 only (no colors, layers, or modern AP242).
   - Boolean-result solids may fail to export.
   - Assembly structure not supported.
4. Future: AP214/AP242 support as ruststep improves.

## File Extension

`.waffle` — Waffle Iron project file.

## Example File Structure

```json
{
  "format": "waffle-iron",
  "version": 1,
  "project": {
    "name": "Simple Box",
    "created": "2025-01-15T10:30:00Z",
    "modified": "2025-01-15T10:35:00Z"
  },
  "features": [
    {
      "id": "a1b2c3d4-...",
      "name": "Sketch 1",
      "operation": {
        "type": "Sketch",
        "sketch": {
          "id": "e5f6g7h8-...",
          "plane": { "type": "Datum", "datum_id": "origin-xy" },
          "entities": [
            { "type": "Point", "id": 1, "x": 0, "y": 0, "construction": false },
            { "type": "Point", "id": 2, "x": 100, "y": 0, "construction": false },
            ...
          ],
          "constraints": [
            { "type": "Horizontal", "entity": 5 },
            { "type": "Distance", "entity_a": 1, "entity_b": 2, "value": 100 },
            ...
          ]
        }
      },
      "suppressed": false,
      "references": []
    },
    {
      "id": "i9j0k1l2-...",
      "name": "Extrude 1",
      "operation": {
        "type": "Extrude",
        "params": {
          "sketch_id": "e5f6g7h8-...",
          "profile_index": 0,
          "depth": 50,
          "direction": null,
          "symmetric": false,
          "cut": false,
          "target_body": null
        }
      },
      "suppressed": false,
      "references": [
        {
          "kind": "Face",
          "anchor": { "type": "FeatureOutput", "feature_id": "a1b2c3d4-...", "output_key": { "type": "Profile", "index": 0 } },
          "selector": { "type": "Role", "role": { "type": "ProfileFace" }, "index": 0 },
          "policy": { "type": "BestEffort" }
        }
      ]
    }
  ]
}
```

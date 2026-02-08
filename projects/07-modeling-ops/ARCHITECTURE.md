# 07 — Modeling Ops: Architecture

## Purpose

Implement individual parametric modeling operations (extrude, revolve, fillet, chamfer, shell, boolean combine). Each operation takes parameters + a reference to the `Kernel` and `KernelIntrospect` traits, executes geometry, and returns a complete `OpResult` with full provenance for persistent naming.

## Design Pattern

Every operation follows the same pattern:

```
fn execute_<operation>(
    params: &<Operation>Params,
    kernel: &mut dyn Kernel,
    introspect: &dyn KernelIntrospect,
    input_solid: Option<&KernelSolidHandle>,
) -> Result<OpResult, OpError>
```

Steps:
1. **Snapshot "before" topology** — list all faces, edges, vertices of the input solid (via KernelIntrospect).
2. **Execute the kernel operation** — call the appropriate Kernel trait method.
3. **Snapshot "after" topology** — list all faces, edges, vertices of the result solid.
4. **Compute provenance** — diff before/after to determine created, deleted, and modified entities.
5. **Assign semantic roles** — label created entities with meaningful roles (EndCapPositive, SideFace, FilletFace, etc.).
6. **Compute signatures** — compute TopoSignature for all created/modified entities.
7. **Build and return OpResult** — outputs + provenance + diagnostics.

## Topology Diff Utility

The foundational utility for all operations. Given before and after entity sets:

```
diff(before: &[(KernelId, TopoSignature)], after: &[(KernelId, TopoSignature)]) -> Provenance
```

- **Created:** entities in `after` but not in `before` (by KernelId).
- **Deleted:** entities in `before` but not in `after` (by KernelId).
- **Modified:** entities present in both but with changed signatures. Produces `Rewrite` records with `RewriteReason`.

Note: truck assigns new IDs to entities created by booleans, so "modified" detection requires signature matching between old and new entities. An entity with a new ID but a very similar signature to a deleted entity is classified as "modified" (Rewrite) rather than "deleted + created."

## Operations

### Extrude

**Input:** Profile face(s) + direction + depth.

**Kernel call:** `kernel.extrude_face(face_id, direction, depth)`.

**Role assignment:**
- `EndCapPositive` — the face at the far end of the extrusion (in the extrude direction).
- `EndCapNegative` — the original profile face (at the start of the extrusion).
- `SideFace { index }` — lateral faces created by sweeping profile edges. Index corresponds to the profile edge order.

**Variants:**
- **Symmetric:** Extrude in both directions (depth/2 each way). Two EndCaps (both "positive" from their respective directions). OR: extrude full depth, translate backward by depth/2.
- **Cut:** Extrude the profile, then boolean-subtract from the target body. Produces BooleanBodyA/B roles for the cut result.

### Revolve

**Input:** Profile face(s) + axis (origin + direction) + angle.

**Kernel call:** `kernel.revolve_face(face_id, axis_origin, axis_direction, angle)`.

**Role assignment:**
- `RevStartFace` — face at the start angle (if not full revolution).
- `RevEndFace` — face at the end angle (if not full revolution).
- `SideFace { index }` — lateral faces from sweeping profile edges around the axis.
- If full revolution (angle >= 2*PI): no start/end faces.

### Fillet

**Input:** Edge GeomRefs + radius.

**Kernel call:** `kernel.fillet_edges(solid, edge_ids, radius)`.

**Role assignment:**
- `FilletFace { index }` — each new fillet surface gets an indexed FilletFace role.

**Provenance:**
- Fillet faces are "created."
- Adjacent faces that were trimmed are "modified" (Rewrite with reason Trimmed).
- The original edges are "deleted" (replaced by fillet faces).

### Chamfer

**Input:** Edge GeomRefs + distance.

**Kernel call:** `kernel.chamfer_edges(solid, edge_ids, distance)`.

**Role assignment:**
- `ChamferFace { index }` — each chamfer surface.

**Provenance:** Same pattern as fillet (created chamfer faces, trimmed adjacent faces, deleted edges).

### Shell

**Input:** Face GeomRefs to remove + thickness.

**Kernel call:** `kernel.shell(solid, face_ids, thickness)`.

**Role assignment:**
- `ShellInnerFace { index }` — the new inner faces created by offsetting.

**Provenance:**
- Removed faces are "deleted."
- Remaining faces may be "modified" (edges change where removed faces were).
- Inner faces are "created."

### Boolean Combine

**Input:** Two body GeomRefs + operation type (Union/Subtract/Intersect).

**Kernel calls:** `kernel.boolean_union/subtract/intersect(body_a, body_b)`.

**Role assignment:**
- `BooleanBodyAFace { index }` — surviving faces from body A.
- `BooleanBodyBFace { index }` — surviving faces from body B.

**Provenance:**
- Boolean results create entirely new topology (truck assigns new IDs).
- Use signature matching to trace faces from input bodies to result.

### Mirror / Pattern (Deferred)

Documented for future implementation:
- **Mirror:** Transform + optional boolean union with original.
- **Linear Pattern:** Repeated transform + boolean union.
- **Circular Pattern:** Rotational transform + boolean union.
- Role: `PatternInstance { index }` for each copy.

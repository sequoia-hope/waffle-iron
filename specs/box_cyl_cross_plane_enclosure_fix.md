# Spec: Box-Cylinder Cross-Plane Enclosure Fix

## Goal

Fix a bug where `box_cyl_boolean` in the SSI pipeline falsely reports a cylinder
as "fully enclosed" in a box when the box is extruded along a different direction
than the cylinder axis. The AABB-based enclosure check inflates the box's bounding
box after rotation to the cylinder's frame, causing false positives.

## Bug Description

When `cyl_enclosed_in_box` uses the AABB of a rotated box, and the box's extrude
direction differs from the cylinder axis (cross-plane case), the AABB is
significantly inflated. A small cylinder may appear enclosed in the inflated AABB
when it's not actually inside the real box geometry.

For `BoolOp::Union` with `fully_enclosed = true`, the code returns just the box,
silently discarding the cylinder. This produces 12-triangle meshes instead of
the expected box+cylinder union.

## Fix

The code already has a `point_in_solid` refinement for non-rectangular solids
(`face_map.len() > 6`). The fix is to always apply this refinement regardless
of face count. This tests the cylinder midpoint against the actual face geometry
(not the inflated AABB).

### Parameters

No new parameters.

### Branch Table

| Case | Before Fix | After Fix |
|------|-----------|-----------|
| Same-plane box+cyl, cyl enclosed | AABB correct, returns box | point_in_solid confirms, returns box |
| Same-plane box+cyl, cyl NOT enclosed | AABB correct, returns NotSupported | point_in_solid confirms, returns NotSupported |
| Cross-plane box+cyl, cyl NOT enclosed | AABB FALSE POSITIVE, returns box (BUG) | point_in_solid correctly rejects, returns NotSupported → polygon fallback |
| Non-rectangular solid + cyl | Already uses point_in_solid | Unchanged |

## Invariants

1. `xy_enclosed` must only be true when the cylinder is actually inside the box geometry
2. No regression for same-plane cases (point_in_solid agrees with AABB for aligned boxes)
3. Cross-plane cases fall through to polygon clipping (correct fallback)

## Oracles

1. For F0046/F0047/F0048: triangle count > 12 (not just a box)
2. For same-plane box+cyl union: result topology unchanged

## Research Basis

- [#24] Barton et al.: Hybrid B-Rep/mesh booleans. The SSI pipeline should return
  NotSupported for cases it can't handle, allowing fallback to polygon clipping.
- The existing `point_in_solid` uses winding number classification [#7 Jacobson].

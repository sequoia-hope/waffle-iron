# 07 — Modeling Ops: Plan

## Milestones

### M1: Topology Diff Utility ✅
- [x] `diff(before, after) -> Provenance` function
- [x] Created/deleted detection by KernelId
- [x] Modified detection by signature matching (new ID but similar signature)
- [x] RewriteReason classification (Trimmed, Split, Merged, Moved)
- [x] Unit tests with MockKernel: extrude → diff → verify created/deleted/modified

### M2: Extrude with Full Provenance ✅
- [x] Execute extrude via Kernel trait
- [x] Snapshot before/after topology
- [x] Compute provenance via diff
- [x] Assign roles: EndCapPositive, EndCapNegative, SideFace
- [x] Compute signatures for all created entities
- [x] Return complete OpResult
- [x] Test: extrude rectangle → verify 6 faces with correct roles

### M3: Revolve with Full Provenance ✅
- [x] Execute revolve via Kernel trait
- [x] Provenance computation
- [x] Role assignment: RevStartFace, RevEndFace, SideFace
- [x] Test: revolve profile 90° → verify roles
- [x] Test: revolve profile 360° → verify no start/end faces

### M4: Boolean Combine with Provenance ✅
- [x] Execute union/subtract/intersect via Kernel trait
- [x] Provenance via signature matching (new IDs)
- [x] Role assignment: BooleanBodyAFace, BooleanBodyBFace
- [x] Test: union two boxes → verify provenance
- [x] Test: subtract cylinder from box → verify provenance (tested with different-sized boxes)

### M5: Fillet with Provenance ✅
- [x] Execute fillet via Kernel trait (KernelBundle)
- [x] Provenance: created FilletFaces, trimmed adjacent faces, deleted edges
- [x] Role assignment: FilletFace (using surface_type="cylindrical" detection)
- [x] Tests: fillet_produces_valid_op_result, fillet_assigns_fillet_face_roles, fillet_provenance_tracks_created, fillet_invalid_radius_returns_error

### M6: Chamfer with Provenance ✅
- [x] Execute chamfer via Kernel trait (KernelBundle)
- [x] Provenance: created ChamferFaces, trimmed faces, deleted edges
- [x] Role assignment: ChamferFace (using signature_similarity < 0.7 for new face detection)
- [x] Tests: chamfer_produces_valid_op_result, chamfer_assigns_chamfer_face_roles, chamfer_invalid_distance_returns_error
- [x] Fixed MockKernel chamfer surface_type from "planar" to "chamfer" for distinguishable signatures

### M7: Shell with Provenance ✅
- [x] Execute shell via Kernel trait (KernelBundle)
- [x] Provenance: deleted face, created inner faces, modified remaining faces
- [x] Role assignment: ShellInnerFace (using signature_similarity < 0.7)
- [x] Tests: shell_produces_valid_op_result, shell_assigns_inner_face_roles, shell_invalid_thickness_returns_error
- [x] Fixed MockKernel shell inner face surface_type to "offset_planar" for distinguishable signatures

### M8: Extrude Variants ✅
- [x] Symmetric extrude (both directions, centered on sketch plane)
- [x] Tests: symmetric_extrude_produces_valid_result, symmetric_extrude_assigns_end_cap_roles, symmetric_extrude_has_diagnostic_warning, symmetric_extrude_invalid_depth_returns_error
- [ ] Cut extrude (boolean subtract from target) — deferred, requires multi-body workflow

### M9: All Ops Against MockKernel ✅
- [x] Euler formula verification (V-E+F=2)
- [x] Multi-operation pipelines: extrude→fillet, extrude→chamfer, extrude→shell, extrude→boolean
- [x] Multiple edges fillet/chamfer, multiple face shell removal
- [x] Provenance signature validation (all created entities have data)
- [x] Role index sequentiality verification
- [x] Diff topology change detection after fillet
- [x] All ops produce OutputKey::Main consistency check
- [x] Fixed fillet role assignment to use signature_similarity (consistent with chamfer/shell)

### M10: Integration with TruckKernel — PARTIAL ✅
- [x] Refactored truck_introspect.rs: extracted shared impl functions, implemented KernelIntrospect directly on TruckKernel (resolves Blocker 1)
- [x] TruckKernel now satisfies KernelBundle trait
- [x] Extrude integration tests (4): valid OpResult, correct topology, role assignment, provenance signatures
- [x] Revolve integration tests (2): valid OpResult, role assignment (side faces verified; start/end face heuristic doesn't work with real truck normals)
- [x] Not-supported tests (3): fillet, chamfer, shell correctly return NotSupported
- **Remaining limitation**: Boolean operations unreliable in truck 0.4 (panics/None for box-cylinder and coplanar faces)
- **Remaining limitation**: Revolve role detection heuristic (normal-axis dot product) doesn't reliably identify RevStartFace/RevEndFace with real truck geometry

## Test Summary

| Test Suite | Count | Status |
|-----------|-------|--------|
| M1-M4 (original) | 19 | ✅ All pass |
| M5 Fillet | 4 | ✅ All pass |
| M6 Chamfer | 3 | ✅ All pass |
| M7 Shell | 3 | ✅ All pass |
| M8 Symmetric Extrude | 4 | ✅ All pass |
| M9 Comprehensive | 12 | ✅ All pass |
| M10 TruckKernel | 9 | ✅ All pass |
| **Total** | **54** | **✅** |

## Signature Similarity Gotchas

- Role assignment uses `signature_similarity` threshold of 0.7 to identify "new" faces
- The `surface_type` field has weight 3.0 (highest), so matching surface types can push similarity above threshold even when normals/centroids differ
- MockKernel must produce distinct `surface_type` for operation-specific faces:
  - Fillet: "cylindrical"
  - Chamfer: "chamfer"
  - Shell inner: "offset_planar"

## Blockers

- Cut extrude deferred: needs multi-body pipeline (extrude + boolean subtract in one operation)
- TruckKernel boolean operations unreliable in truck 0.4
- Revolve role heuristic needs improvement for real geometry (normal-axis dot product too simplistic)

## Interface Change Requests

(None)

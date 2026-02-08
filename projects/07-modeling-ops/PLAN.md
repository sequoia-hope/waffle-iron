# 07 — Modeling Ops: Plan

## Milestones

### M1: Topology Diff Utility
- [ ] `diff(before, after) -> Provenance` function
- [ ] Created/deleted detection by KernelId
- [ ] Modified detection by signature matching (new ID but similar signature)
- [ ] RewriteReason classification (Trimmed, Split, Merged, Moved)
- [ ] Unit tests with MockKernel: extrude → diff → verify created/deleted/modified

### M2: Extrude with Full Provenance
- [ ] Execute extrude via Kernel trait
- [ ] Snapshot before/after topology
- [ ] Compute provenance via diff
- [ ] Assign roles: EndCapPositive, EndCapNegative, SideFace
- [ ] Compute signatures for all created entities
- [ ] Return complete OpResult
- [ ] Test: extrude rectangle → verify 6 faces with correct roles
- [ ] Test: extrude circle → verify face count and roles

### M3: Revolve with Full Provenance
- [ ] Execute revolve via Kernel trait
- [ ] Provenance computation
- [ ] Role assignment: RevStartFace, RevEndFace, SideFace
- [ ] Test: revolve profile 90° → verify roles
- [ ] Test: revolve profile 360° → verify no start/end faces

### M4: Boolean Combine with Provenance
- [ ] Execute union/subtract/intersect via Kernel trait
- [ ] Provenance via signature matching (new IDs)
- [ ] Role assignment: BooleanBodyAFace, BooleanBodyBFace
- [ ] Test: union two boxes → verify provenance
- [ ] Test: subtract cylinder from box → verify provenance

### M5: Fillet with Provenance
- [ ] Execute fillet via Kernel trait
- [ ] Provenance: created FilletFaces, trimmed adjacent faces, deleted edges
- [ ] Role assignment: FilletFace
- [ ] Test: fillet one edge of a box → verify roles and provenance

### M6: Chamfer with Provenance
- [ ] Execute chamfer via Kernel trait
- [ ] Provenance: created ChamferFaces, trimmed faces, deleted edges
- [ ] Role assignment: ChamferFace
- [ ] Test: chamfer one edge of a box

### M7: Shell with Provenance
- [ ] Execute shell via Kernel trait
- [ ] Provenance: deleted face, created inner faces, modified remaining faces
- [ ] Role assignment: ShellInnerFace
- [ ] Test: shell a box (remove top face)

### M8: Extrude Variants
- [ ] Symmetric extrude (both directions)
- [ ] Cut extrude (boolean subtract from target)
- [ ] Test: symmetric extrude → verify provenance
- [ ] Test: cut extrude → verify provenance (boolean subtraction)

### M9: All Ops Against MockKernel
- [ ] Full test suite using MockKernel
- [ ] Every operation produces correct OpResult
- [ ] Every operation assigns correct roles
- [ ] Every operation produces correct provenance

### M10: Integration with TruckKernel
- [ ] Run all tests with TruckKernel
- [ ] Document any truck-specific failures or discrepancies
- [ ] Benchmark kernel operation times

## Blockers

- Depends on kernel-fork (Kernel + KernelIntrospect traits, MockKernel)
- M5 (fillet) depends on kernel-fork implementing fillet (may be deferred)

## Interface Change Requests

(None yet)

## Notes

- The topology diff utility is foundational — build it first.
- Provenance must be complete and correct. Feature-engine depends on it for persistent naming.
- Document any cases where provenance is ambiguous (multiple matching signatures).

# 10 — Assemblies: Plan

## Status: DEFERRED

Prerequisites: all other sub-projects at MVP level.

## Future Milestones (Not Scheduled)

### M1: Assembly Data Structure
- [ ] Assembly tree (parts + sub-assemblies)
- [ ] Part instances with transforms
- [ ] Mate connector definitions

### M2: Mate Types
- [ ] Fastened mate
- [ ] Revolute mate
- [ ] Slider mate
- [ ] Additional mates (cylindrical, ball, planar)

### M3: Mate Solver
- [ ] 3D constraint solving for mate positions
- [ ] Evaluate libslvs for 3D mates vs custom solver

### M4: Assembly UI
- [ ] Assembly tree panel
- [ ] Mate creation workflow
- [ ] Multi-part viewport rendering

### M5: In-Context Editing
- [ ] Edit part within assembly context
- [ ] Reference geometry from other parts
- [ ] Propagate changes to all instances

### M6: Assembly File Format
- [ ] Extend .waffle format for assemblies
- [ ] Part references (file paths or embedded)
- [ ] STEP assembly export

## Blockers

- All other sub-projects must reach MVP first.

## Interface Change Requests

(None yet)
